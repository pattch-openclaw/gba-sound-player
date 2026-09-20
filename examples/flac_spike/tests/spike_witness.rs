//! Spike PR 1 witness suite: the embedded clips decode **bit-exactly** on the
//! host, from the exact bytes the ROM will embed.
//!
//! FLAC.md's correctness-before-speed rule: no perf number is ever trusted
//! from an image whose decode is not proven on its own embedded bytes. This
//! suite is that proof for the *assets* — the ROM's own decode proof is PR 2.
//! It runs the production `flac_lite::frame::decode_frame` through the
//! spike's seek-table driver (the same code path the ROM walks), and compares
//! against `flac -d`'s reference PCM — a second, independent decoder.
//!
//! Layers, each pinning a different failure mode:
//!
//! 1. **Hash pins** — the Rust FNV-1a (this crate) reproduces the Python
//!    generator's pins over both the frame regions and the reference PCM.
//!    Two independent implementations of the hash meeting on committed bytes;
//!    a mismatch means an implementation drifted or the assets were edited.
//! 2. **Bit-exact decode** — driver over the seek table, whole clip,
//!    interleaved i16-LE output equal to the reference blob sample-for-sample.
//! 3. **Cross-arm agreement** — the two arms encode the *same source*, so
//!    their reference PCM must be byte-identical. Any divergence means the
//!    generator stopped measuring one comparison against one ground truth.
//! 4. **Seek-table structure** — the GAFP stand-in's invariants re-checked
//!    from the generated table against the blob (frame 0 at 0, ascending,
//!    tiling, blocksize sums).
//! 5. **Negative control** — corrupt one seek entry by +1 byte and require
//!    the walk to *fail*: proof the driver actually consumes the table, not
//!    just the region's natural frame chaining.

use flac_spike::assets::{CLIPS, FrameMeta, L0_FIXED, L4_LPC};
use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
use flac_spike::driver;

/// The reference PCM blobs, included at compile time (host-test ground truth
/// only — the ROM never embeds these, it pins them by hash). Included by
/// literal path because `include_bytes!` is compile-time; the per-clip
/// `pcm_file` field is asserted to name exactly these files below.
const L0_PCM: &[u8] = include_bytes!("../assets/spike_l0_pcm.bin");
const L4_PCM: &[u8] = include_bytes!("../assets/spike_l4_pcm.bin");

/// The reference blob for an arm, plus the file name it is included from.
/// The names here are ground truth for `SpikeClip::pcm_file` because this
/// suite's `include_bytes!` paths ARE the files: the assertion is "the field
/// names the blob I actually read", not a re-derived naming formula (a first
/// draft built the expectation from `name` instead and fired — the field
/// follows the blob prefix, `spike_l0_*`, which is also how the region files
/// are named; consistent generator output, wrong test arithmetic).
fn pcm_for(name: &str) -> (&'static [u8], &'static str) {
    match name {
        n if n == L0_FIXED.name => (L0_PCM, "spike_l0_pcm.bin"),
        n if n == L4_LPC.name => (L4_PCM, "spike_l4_pcm.bin"),
        other => panic!("witness suite does not know a blob for arm {other:?}"),
    }
}

#[test]
fn rust_fnv_reproduces_the_generators_pins() {
    // Layer 1: region pins.
    for clip in CLIPS {
        assert_eq!(
            fnv1a64(clip.region),
            clip.fnv_frames,
            "{}: region bytes in the tree no longer match the generator's \
             pin — assets drifted, or the two FNV implementations diverged",
            clip.name
        );
    }
}

#[test]
fn pcm_blobs_match_their_pins_and_names() {
    // Layer 1 continued: PCM pins + the file-name fields the ROM logs.
    for clip in CLIPS {
        let (pcm, file) = pcm_for(clip.name);
        assert_eq!(
            fnv1a64(pcm),
            clip.fnv_pcm,
            "{}: reference PCM blob no longer matches the generator's pin",
            clip.name
        );
        assert_eq!(
            clip.pcm_file, file,
            "{}: pcm_file field must name the blob this test includes",
            clip.name
        );
    }
}

#[test]
fn decode_is_bit_exact_against_the_reference_pcm() {
    // Layer 2: the core witness. Driver + production decode_frame over the
    // exact embedded region, whole clip, chained through reused buffers.
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut got: Vec<u8> = Vec::with_capacity(clip.pcm_expected_len());

        let stats = driver::decode_clip(clip, &mut left, &mut right, &mut state, |_frame, l, r| {
            for (a, b) in l.iter().zip(r.iter()) {
                let (a, b) = (
                    i16::try_from(*a).expect("decoded left fits i16 (16-bit profile)"),
                    i16::try_from(*b).expect("decoded right fits i16 (16-bit profile)"),
                );
                got.extend_from_slice(&a.to_le_bytes());
                got.extend_from_slice(&b.to_le_bytes());
            }
        })
        .unwrap_or_else(|e| panic!("{}: clip walk failed: {e:?}", clip.name));

        assert_eq!(
            stats.frames,
            clip.frames.len(),
            "{}: frame count",
            clip.name
        );
        assert_eq!(
            stats.samples,
            usize::try_from(clip.total_samples).unwrap(),
            "{}: sample count",
            clip.name
        );
        assert_eq!(
            got,
            pcm_for(clip.name).0,
            "{}: decoded PCM is not bit-exact against `flac -d` reference",
            clip.name
        );
    }
}

#[test]
fn rom_fold_reproduces_pcm_pins_on_host() {
    // PR 2's shared-fold witness. The ROM's decode proof (src/main.rs) folds
    // every decoded block through `checksum::fold_i16le_stereo` and compares
    // against `fnv_pcm`. This test runs the *same library fold function*
    // through the *same driver* on the *same embedded bytes* on the host and
    // requires the same pins — so the on-target hash is not a second
    // transcription of the interleave rule that could agree with its own
    // mistake. A fold that reaches the pin here is the fold the ROM runs;
    // agreement of ROM and host is then a property of the hardware, not of
    // two separately-written folds.
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut hash = Fnv1a64::new();

        let stats = driver::decode_clip(clip, &mut left, &mut right, &mut state, |_frame, l, r| {
            fold_i16le_stereo(&mut hash, l, r)
                .expect("bit-exact decode of a 16-bit stream always fits i16")
        })
        .unwrap_or_else(|e| panic!("{}: clip walk failed: {e:?}", clip.name));

        assert_eq!(stats.frames, clip.frames.len(), "{}: frames", clip.name);
        assert_eq!(
            hash.finish(),
            clip.fnv_pcm,
            "{}: the ROM's fold sequence must reach the generator's PCM pin",
            clip.name
        );
        // The fold is over exactly the reference blob's bytes: pin == hash of
        // the blob is asserted separately (pcm_blobs_match_their_pins_*), so
        // equality here means fold-sequence == blob, byte for byte.
        assert_eq!(hash.finish(), fnv1a64(pcm_for(clip.name).0));
    }
}

#[test]
fn both_arms_decode_the_same_source() {
    // Layer 3: one source, two encodes => one ground truth. If the arms' PCM
    // blobs ever differ, the generator synthesized or decoded the arms from
    // different material and the FIXED-vs-LPC comparison lost its footing.
    assert_eq!(
        fnv1a64(L0_PCM),
        fnv1a64(L4_PCM),
        "arms' reference PCM diverged — same source, same expected samples"
    );
    assert_eq!(L0_PCM, L4_PCM);
}

#[test]
fn seek_table_structural_invariants_hold() {
    // Layer 4: the GAFP stand-in's contract, re-derived from the table.
    for clip in CLIPS {
        assert!(!clip.frames.is_empty(), "{}: empty seek table", clip.name);
        assert_eq!(
            clip.frames[0].offset, 0,
            "{}: frame 0 must start the region",
            clip.name
        );
        let ends: Vec<u64> = clip
            .frames
            .iter()
            .skip(1)
            .map(|f| u64::from(f.offset))
            .chain(std::iter::once(clip.region.len() as u64))
            .collect();
        let mut sample_sum = 0usize;
        let mut max_block = 0u16;
        for (frame, &end) in clip.frames.iter().zip(&ends) {
            assert!(
                u64::from(frame.offset) < end,
                "{}: offsets must strictly ascend / stay in-bounds",
                clip.name
            );
            sample_sum += usize::from(frame.blocksize);
            max_block = max_block.max(frame.blocksize);
        }
        assert_eq!(
            usize::from(max_block),
            usize::from(clip.max_blocksize),
            "{}: max_blocksize field",
            clip.name
        );
        assert_eq!(
            sample_sum,
            usize::try_from(clip.total_samples).unwrap(),
            "{}: blocksize sum",
            clip.name
        );
        // Only the final frame may be short of max blocksize.
        for frame in &clip.frames[..clip.frames.len() - 1] {
            assert_eq!(
                frame.blocksize, clip.max_blocksize,
                "{}: non-final short frame",
                clip.name
            );
        }
    }
}

#[test]
fn a_corrupted_seek_entry_breaks_the_walk() {
    // Layer 5: negative control. +1 byte off one mid-clip entry must fail —
    // either at header parse (sync gone) or at the table/header blocksize
    // cross-check. If this ever decodes "fine", the driver is ignoring the
    // table and chaining by bytes instead, and the seek table is un-witnessed.
    // L4_LPC is a const SpikeClip *value*, not a reference — borrow it
    // directly (CLIPS holds the &-ed form; here the struct is copied anyway
    // by the struct-update below, and `frames` is the only replaced field).
    let clip = &L4_LPC;
    let index = clip.frames.len() / 2;
    let mut frames: Vec<FrameMeta> = clip.frames.to_vec();
    frames[index].offset += 1;
    let bad = flac_spike::assets::SpikeClip {
        frames: Box::leak(frames.into_boxed_slice()),
        ..*clip
    };

    let max = usize::from(clip.max_blocksize);
    let mut left = vec![0i32; max];
    let mut right = vec![0i32; max];
    let mut state = Default::default();
    let err = driver::decode_one(&bad, index, &mut left, &mut right, &mut state)
        .expect_err("a corrupted offset must NOT decode");
    // Control: the pristine table decodes that same frame cleanly — the
    // failure above is the corruption, not the harness.
    let ok = driver::decode_one(clip, index, &mut left, &mut right, &mut state)
        .expect("pristine table frame must decode");
    assert_eq!(ok, usize::from(clip.frames[index].blocksize));
    assert!(
        !format!("{err:?}").is_empty(),
        "error must be descriptive: {err:?}"
    );
}

/// Small helper trait so the test reads intent-first: expected byte length of
/// a clip's reference PCM (interleaved stereo i16).
trait PcmLen {
    fn pcm_expected_len(&self) -> usize;
}

impl PcmLen for flac_spike::assets::SpikeClip {
    fn pcm_expected_len(&self) -> usize {
        usize::try_from(self.total_samples).unwrap() * usize::from(self.channels) * 2
    }
}
