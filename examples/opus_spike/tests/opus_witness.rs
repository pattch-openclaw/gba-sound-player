//! Opus gate step-2 witness suite: the embedded arms decode **onto the
//! ffmpeg/libopus reference** on the host, from the exact bytes the ROM will
//! embed.
//!
//! OPUS.md's correctness-before-speed rule (the Opus sibling of
//! flac_spike's `spike_witness.rs`): no perf number is ever trusted from an
//! image whose decode is not proven on its own embedded bytes. This suite is
//! that proof for the *assets* — the ROM's own decode proof is the next PR.
//! It drives the production [`probe::decode_clip`] walk over the generated
//! manifest and compares against the ffmpeg/libopus float decode — a second,
//! independent decoder.
//!
//! Layers, each pinning a different failure mode:
//!
//! 1. **Hash pins** — the Rust FNV-1a reproduces the Python generator's pins
//!    over both the packet regions and the reference PCM blobs. Two
//!    independent implementations of the hash meeting on committed bytes.
//!    Layer 1b adds the ROM decode proof's golden: the FNV of the walk's
//!    own folded output (`fnv_walk_fold`) — what the on-target decode proof
//!    reproduces, since a lossy codec cannot pin against the reference hash.
//! 2. **Grid decode at the MEASURED alignment** — the sequential walk, whole
//!    clip, folded through the crate's shared `fold_f32_to_i16` (round half
//!    up), compared to the folded reference from the per-arm `align_shift`.
//!    Both the alignment and the residual count are measured pins, and this
//!    layer RE-DERIVES them in Rust (a second implementation of the
//!    generator's discovery, agreeing on the production walk's bytes) before
//!    judging them. Two measured findings this layer encodes (2026-10-02):
//!    OpusHead `pre_skip` is NOT always the walk→reference alignment (the
//!    SILK arm aligns 3 samples early — comparing at pre_skip "disagreed" on
//!    478,439/480,000 samples that decode correctly); and on the round-half-up
//!    grid the residuals are soft-float drift, bounded: exactly
//!    `fold_mismatch` samples off, every one ≤ 1 LSB (SILK: 0; CELT: 127).
//!    Raw f32 is never compared — it is not bit-exact between the decoders.
//! 3. **Geometry pins** — manifest/region/census agreement re-derived from
//!    the generated table (a no-degenerate-arms sanity net: packet count,
//!    walk = packets × frame, alignment/tolerance pins inside their
//!    fail-closed bands).
//! 4. **Negative controls** — six corrupt manifests (past-region, gap,
//!    overlap, in-region lying length, truncated table, empty packet) each
//!    produce their NAMED failure at the right index, plus an identity
//!    positive control proving the guard rejects for the corruption, not
//!    because everything fails. The in-region lying length is the case an
//!    earlier draft's tautological guard (`packet.len() != entry.len` on a
//!    slice built from `entry.len`) accepted silently.
//!
//! Deliberate divergence from the FLAC witness: no cross-arm PCM agreement
//! layer. FLAC's two arms are lossless encodes of one source, so their
//! reference PCM had to be byte-identical; Opus is lossy and each arm's
//! reference is the decode of its OWN encode — cross-arm PCM equality is not
//! a meaningful invariant here, and no layer pretends otherwise.

use opus_spike::assets::{CLIPS, OPUS_MUSIC, OPUS_SILK, OpusClip, PacketMeta};
use opus_spike::checksum::{fnv1a64, fold_f32_to_i16};
use opus_spike::probe;

/// The reference PCM blobs (f32-LE mono, ffmpeg/libopus float decode with
/// pre-skip applied), included at compile time — host-test ground truth
/// only; the ROM never embeds these, it pins them by hash. Included by
/// literal path because `include_bytes!` is compile-time; the per-clip
/// `ref_file` field is asserted to name exactly these files below.
const SILK_REF: &[u8] = include_bytes!("../assets/opus_silk_ref.bin");
const MUSIC_REF: &[u8] = include_bytes!("../assets/opus_music_ref.bin");

/// The reference blob for an arm, plus the file name it is included from.
/// Same rule as flac_spike's `pcm_for`: the assertion is "the field names
/// the blob I actually read", not a re-derived naming formula.
fn ref_for(name: &str) -> (&'static [u8], &'static str) {
    match name {
        n if n == OPUS_SILK.name => (SILK_REF, "opus_silk_ref.bin"),
        n if n == OPUS_MUSIC.name => (MUSIC_REF, "opus_music_ref.bin"),
        other => panic!("witness suite does not know a blob for arm {other:?}"),
    }
}

/// Decode `bytes` (f32-LE) onto the i16 fold grid.
fn fold_f32le_to_i16_vec(bytes: &[u8]) -> Vec<i16> {
    assert_eq!(bytes.len() % 4, 0, "f32-LE blob with a tail byte");
    bytes
        .chunks_exact(4)
        .map(|b| fold_f32_to_i16(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        .collect()
}

/// The walk's full folded output for a clip (all `walk_samples` samples,
/// alignment shift still to be applied).
fn walk_folded(clip: &OpusClip) -> Vec<i16> {
    let mut folded = Vec::with_capacity(clip.walk_samples as usize);
    let walk = probe::decode_clip(clip, |window| {
        folded.extend(window.iter().copied().map(fold_f32_to_i16));
    });
    walk.expect("production walk must accept the committed manifest");
    assert_eq!(
        folded.len(),
        clip.walk_samples as usize,
        "{}: walk produced {} samples, manifest says {}",
        clip.name,
        folded.len(),
        clip.walk_samples
    );
    folded
}

/// Rust twin of the generator's `discover_alignment` (scripts/opus_assets.py)
/// — same fold (the crate's own), same scan band (± 16 of `pre_skip`), same
/// tie-break (fewest mismatches; ties prefer the shift nearest `pre_skip`),
/// same early exits. The pins in assets.rs are written by the Python side;
/// this side re-derives them from the production walk's bytes, and layer 2
/// asserts the two implementations agree. Two transcriptions of one rule is
/// how a shared misreading hides — but here the discovery is over bytes both
/// sides read independently of each other's scan logic, and the pins are
/// claims about decode CONTENT, so agreement is evidence, not collusion.
///
/// Returns `(shift, mismatches, max_abs_delta_lsb)` at the best shift.
fn discover_alignment_rust(
    walk: &[i16],
    reference: &[i16],
    pre_skip: usize,
) -> (usize, usize, i32) {
    let n = reference.len();
    assert!(walk.len() >= n, "walk shorter than reference");
    let lo = pre_skip.saturating_sub(16);
    let hi = (pre_skip + 16).min(walk.len() - n);
    let mut shifts: Vec<usize> = (lo..=hi).collect();
    shifts.sort_by_key(|s| (*s as isize - pre_skip as isize).abs());
    let mut best: Option<(usize, i32, usize)> = None; // (mismatches, max_lsb, shift)
    for shift in shifts {
        let mut mismatches = 0usize;
        let mut max_lsb: i32 = 0;
        let cap = best.map_or(n, |b| b.0);
        for (a, b) in walk[shift..shift + n].iter().zip(reference) {
            let d = i32::from(*a) - i32::from(*b);
            if d != 0 {
                mismatches += 1;
                max_lsb = max_lsb.max(d.abs());
                if mismatches >= cap {
                    break; // cannot beat the incumbent; abandon this scan
                }
            }
        }
        if best.is_none_or(|b| mismatches < b.0) {
            best = Some((mismatches, max_lsb, shift));
        }
        if best.is_some_and(|b| b.0 == 0) {
            break; // a perfect shift nearest pre_skip is the answer
        }
    }
    let (mismatches, max_lsb, shift) = best.expect("band is non-empty");
    (shift, mismatches, max_lsb)
}

#[test]
fn rust_fnv_reproduces_the_generators_pins() {
    // Layer 1: region + reference pins.
    for clip in CLIPS {
        assert_eq!(
            fnv1a64(clip.region),
            clip.fnv_packets,
            "{}: packet-region bytes in the tree no longer match the \
             generator's pin — assets drifted, or the two FNV implementations \
             diverged",
            clip.name
        );
        let (reference, file) = ref_for(clip.name);
        assert_eq!(
            fnv1a64(reference),
            clip.fnv_ref_pcm,
            "{}: reference PCM blob no longer matches the generator's pin",
            clip.name
        );
        assert_eq!(
            clip.ref_file, file,
            "{}: ref_file field must name the blob this test includes",
            clip.name
        );
        assert_eq!(
            clip.ref_bytes_len as usize,
            reference.len(),
            "{}: ref_bytes_len disagrees with the blob on disk",
            clip.name
        );
    }
}

#[test]
fn walk_fold_hash_reproduces_the_rom_decode_golden() {
    // Layer 1b: the ROM decode proof's golden (PR 3, 2026-10-03). The
    // vendored walk's OWN folded output — every walk_sample through the
    // shared round-half-up grid, i16 LE, no alignment shift — hashed to
    // `fnv_walk_fold`. The FLAC gate pinned its ROM against the reference
    // PCM hash; that pattern does NOT transfer to a lossy codec: the arms
    // are decimated and the port drifts ±1 LSB from libopus, so the walk's
    // hash can never equal the reference's. The walk's own hash is the
    // deterministic property the ROM reproduces; the reference comparison
    // stays layer 2's job. The Python twin is `fold_walk_fnv` in
    // scripts/opus_assets.py (the generator measures the pin over the
    // dump_walk output — the production seam); this is the Rust re-derivation.
    for clip in CLIPS {
        let folded = walk_folded(clip);
        let mut bytes = Vec::with_capacity(folded.len() * 2);
        for sample in folded {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        assert_eq!(
            fnv1a64(&bytes),
            clip.fnv_walk_fold,
            "{}: FNV of the walk's own folded output disagrees with the \
             generator's fnv_walk_fold pin — the ROM would fail its \
             golden on-target",
            clip.name
        );
    }
}

#[test]
fn decode_walk_meets_the_reference_at_the_measured_alignment() {
    // Layer 2: the core witness. The production walk over the exact embedded
    // bytes, folded with the shared fold, meets the folded ffmpeg/libopus
    // reference at the per-arm MEASURED alignment, with residuals pinned to
    // the generator's measured count and bounded by the float-drift band.
    for clip in CLIPS {
        let (reference, _) = ref_for(clip.name);
        let walk = walk_folded(clip);
        let expected = fold_f32le_to_i16_vec(reference);
        let frame = clip.frame_samples as usize;
        let align = clip.align_shift as usize;
        let pre_skip = clip.pre_skip as usize;

        // The generator pins the alignment fail-closed; re-derive the band
        // here so a hand-edited assets.rs cannot quietly widen the game.
        assert!(
            (align as isize - pre_skip as isize).abs() <= 16,
            "{}: align_shift {align} is more than 16 samples from OpusHead \
             pre_skip {pre_skip} — a toolchain change, not a delay difference",
            clip.name
        );
        assert!(
            align + expected.len() <= walk.len(),
            "{}: walk ({} samples) does not cover align_shift {align} + reference {}",
            clip.name,
            walk.len(),
            expected.len()
        );
        let padding = walk.len() - align - expected.len();
        assert!(
            padding <= 2 * frame,
            "{}: walk tail padding {padding} samples exceeds 2 frames — \
             the measured trimming convention no longer describes ffmpeg",
            clip.name
        );

        // Independent re-derivation: the Rust discovery must land on the
        // generator's Python pins over the same bytes.
        let (shift, _, _) = discover_alignment_rust(&walk, &expected, pre_skip);
        assert_eq!(
            shift, align,
            "{}: alignment re-derived by the witness ({shift}) disagrees with \
             the generator's pin ({align}) — one of the two discovery \
             implementations drifted",
            clip.name
        );

        // Judge at the pinned alignment: residuals must match the pin
        // exactly, and every residual must sit inside the float-drift band.
        let ours = &walk[align..align + expected.len()];
        let mut mismatches = 0usize;
        let mut max_lsb: i32 = 0;
        let mut first = None;
        for (i, (&a, &b)) in ours.iter().zip(expected.iter()).enumerate() {
            let d = i32::from(a) - i32::from(b);
            if d != 0 {
                mismatches += 1;
                max_lsb = max_lsb.max(d.abs());
                if first.is_none() {
                    first = Some((i, a, b));
                }
            }
        }
        assert_eq!(
            mismatches, clip.fold_mismatch as usize,
            "{}: fold-grid residuals at align_shift {align} are {mismatches}, \
             pin says {} — first offender {first:?} (ours vs reference); \
             decode content changed or the pin is stale",
            clip.name, clip.fold_mismatch,
        );
        assert!(
            max_lsb <= 1,
            "{}: residual |delta| {max_lsb} LSB exceeds the float-drift band \
             (< 1 LSB) at align_shift {align} — beyond rounding is decode \
             divergence, first offender {first:?}",
            clip.name,
        );
    }
}

#[test]
fn manifest_geometry_agrees_with_the_census() {
    // Layer 3: re-derive from the generated table what the generator claims.
    for clip in CLIPS {
        assert_eq!(
            clip.packets,
            clip.index.len(),
            "{}: packet count",
            clip.name
        );
        assert_eq!(
            clip.walk_samples as usize,
            clip.packets * clip.frame_samples as usize,
            "{}: walk_samples != packets x frame_samples",
            clip.name
        );
        assert_eq!(clip.channels, 1, "{}: mono profile", clip.name);
        assert_eq!(
            clip.frame_samples as usize,
            probe::frame_samples(clip.sample_rate_hz) as u16 as usize,
            "{}: frame_samples disagrees with the pinned 20 ms duration",
            clip.name
        );
        // Manifest tiling: packet 0 at 0, ascending, exact cover.
        let mut cursor = 0u32;
        for (i, entry) in clip.index.iter().enumerate() {
            assert_eq!(
                entry.offset, cursor,
                "{}: index gap/overlap at {i}",
                clip.name
            );
            assert!(entry.len > 0, "{}: empty packet at {i}", clip.name);
            cursor += entry.len as u32;
        }
        assert_eq!(
            cursor as usize,
            clip.region.len(),
            "{}: index does not tile the region exactly",
            clip.name
        );
        // Alignment/tolerance pins inside their fail-closed bands (the same
        // bands the generator refuses to write outside of).
        assert!(
            (clip.align_shift as isize - clip.pre_skip as isize).abs() <= 16,
            "{}: align_shift {} outside the ± 16 band around pre_skip {}",
            clip.name,
            clip.align_shift,
            clip.pre_skip
        );
        let ref_samples = clip.ref_bytes_len as usize / 4;
        assert!(
            clip.fold_mismatch as usize <= ref_samples / 100,
            "{}: fold_mismatch {} exceeds the 1% fail-closed band over \
             {ref_samples} samples",
            clip.name,
            clip.fold_mismatch
        );
        // Census pins (generated strings — assert the decisive prefixes so
        // a regenerated drift shows up HERE before the decode layers).
        assert!(
            clip.census
                .starts_with(&format!("config {} ", clip.toc_config)),
            "{}: census '{}' disagrees with pinned toc_config {}",
            clip.name,
            clip.census,
            clip.toc_config
        );
    }
    // The two arms measure the modes the gate's decision rule is written
    // against: speech = SILK-only, music = CELT-only.
    assert!(
        OPUS_SILK.census.contains("SILK-only"),
        "silk arm census: {}",
        OPUS_SILK.census
    );
    assert!(
        OPUS_MUSIC.census.contains("CELT-only"),
        "music arm census: {}",
        OPUS_MUSIC.census
    );
}

/// Reach `'static` for a test-built corrupt table by leaking a boxed array —
/// fine in a host test process that exits at the end of the suite.
fn leaked(index: Vec<PacketMeta>) -> &'static [PacketMeta] {
    Box::leak(index.into_boxed_slice())
}

#[test]
fn negative_controls_corrupt_manifest_fails_the_walk() {
    // Layer 4: the walk must consume the index. Every corruption shape the
    // tiling guard claims to catch has to produce its NAMED failure at the
    // right index — and the identity control proves the guard rejects for
    // the corruption, not because every input fails.
    let real = OPUS_SILK.index;

    // Identity positive control: the real table walks clean.
    assert!(
        probe::decode_clip(&OPUS_SILK, |_| {}).is_ok(),
        "identity control: the committed manifest must walk Ok"
    );

    // 1. Past-region entry at index 1 (offset one past the end, len > 0).
    let mut past = real[..2].to_vec();
    past[1].offset = OPUS_SILK.region.len() as u32;
    past[1].len = 4;
    let clip = OpusClip {
        index: leaked(past),
        ..OPUS_SILK
    };
    let result = probe::decode_clip(&clip, |_| {});
    assert_eq!(
        result,
        Err(("manifest gap/overlap", 1)),
        "past-region entry must fail at its own index"
    );

    // 2. Gap: entry 1 starts 8 bytes late.
    let mut gap = real[..2].to_vec();
    gap[1].offset += 8;
    let clip = OpusClip {
        index: leaked(gap),
        ..OPUS_SILK
    };
    assert_eq!(
        probe::decode_clip(&clip, |_| {}),
        Err(("manifest gap/overlap", 1)),
        "gap must fail at the entry that skips bytes"
    );

    // 3. Overlap: entry 1 starts 8 bytes early (re-decodes packet 0's tail).
    let mut over = real[..2].to_vec();
    over[1].offset -= 8;
    let clip = OpusClip {
        index: leaked(over),
        ..OPUS_SILK
    };
    assert_eq!(
        probe::decode_clip(&clip, |_| {}),
        Err(("manifest gap/overlap", 1)),
        "overlap must fail at the entry that re-reads bytes"
    );

    // 4. In-region lying length: entry 0 claims 4 extra bytes (still inside
    //    the region), everything else consistent. The tiling cursor then
    //    disagrees with entry 1's offset. This is the exact case the earlier
    //    draft's tautological per-entry length check accepted silently — the
    //    decoder itself never catches it either (measured 2026-10-02: a
    //    truncated packet still decodes Ok(960)).
    let mut lying = real[..2].to_vec();
    lying[0].len += 4;
    let clip = OpusClip {
        index: leaked(lying),
        ..OPUS_SILK
    };
    assert_eq!(
        probe::decode_clip(&clip, |_| {}),
        Err(("manifest gap/overlap", 1)),
        "an in-region lying length must fail where it disagrees with the \
         next entry's offset"
    );

    // 5. Truncated table: drop the last entry — the region ends 1 packet
    //    beyond what the table covers.
    let mut truncated = real.to_vec();
    truncated.pop();
    let blame = truncated.len() - 1;
    let clip = OpusClip {
        index: leaked(truncated),
        ..OPUS_SILK
    };
    assert_eq!(
        probe::decode_clip(&clip, |_| {}),
        Err(("manifest does not tile region", blame)),
        "a table that under-covers the region must fail at its last entry"
    );

    // 6. Empty packet: a zero length can only come from a corrupt manifest.
    let mut empty = real[..2].to_vec();
    empty[1].len = 0;
    let clip = OpusClip {
        index: leaked(empty),
        ..OPUS_SILK
    };
    assert_eq!(
        probe::decode_clip(&clip, |_| {}),
        Err(("manifest empty packet", 1)),
        "a zero-length entry must fail at its own index"
    );
}
