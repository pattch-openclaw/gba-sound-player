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
//! 6. **Copy drift (2026-11-04)** — the crate-side frame loop
//!    (`crates/flac-lite/src/driver.rs`) is a *copy* of this file's loop, not
//!    a shared dependency (FLAC.md → split proposal). Both run over the same
//!    embedded bytes here: the copy must reach the same PCM pins, and the two
//!    must agree frame window by frame window. That is the price of copying,
//!    and this is where it is paid.

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

/// PR 3 witness: the ROM's shared cycle-cost fold (`stats::FrameCost`) and
/// its budget function (`stats::real_time_budget`) agree, frame-by-frame,
/// with the driver's own blocksize accounting over the **exact embedded
/// bytes**. The ROM folds each timed window through `observe`; this test
/// folds the per-frame real-time allowance (derived from the same blocksize
/// the seek table carries) through the same fold, pinning the tie convention
/// and the plan's budget arithmetic against an independent walk of the
/// stream.
#[test]
fn cycle_fold_witness_on_embedded_bytes() {
    use flac_spike::stats::{FrameCost, real_time_budget};

    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut cost = FrameCost::new();

        let stats = driver::decode_clip(clip, &mut left, &mut right, &mut state, |frame, l, r| {
            // FLAC's blocksize is samples *per channel*: the driver hands
            // a stereo callback both channel windows sliced to `written`
            // each (mono: the right window is empty), so l.len() is the
            // frame's blocksize — summing the channels would double it
            // (caught live by this very assertion: 4096 vs the table's 2048).
            let block = l.len();
            assert_eq!(
                block,
                usize::from(clip.frames[frame].blocksize),
                "{}: frame {frame} blocksize vs seek table",
                clip.name
            );
            cost.observe(frame, real_time_budget(block, clip.sample_rate_hz));
        })
        .unwrap_or_else(|e| panic!("{}: clip walk failed: {e:?}", clip.name));

        assert_eq!(
            stats.frames,
            clip.frames.len(),
            "{}: frame count",
            clip.name
        );
        assert_eq!(
            cost.count as usize, stats.frames,
            "{}: fold frame count",
            clip.name
        );
        // The budget arithmetic PR 3's verdict runs on, recomputed here from
        // FLAC.md's plan rather than recalled. Since plan item 7-0 these are
        // ONE number, and that equality is the thing being pinned: the rate
        // contract says a stream is encoded at the rate it is played at, so
        // the stream's own worst-frame window (2048 at the rate its own
        // STREAMINFO declares) and the 32,768 Hz budget the ROM screen
        // compares against must be equal. Two different numbers here is the
        // Q14 defect re-opened, and the rate check below is what names it.
        assert_eq!(
            clip.sample_rate_hz, 32_768,
            "{}: rate contract — the spike's assets must be encoded at the clock \
             they are played at, or the window derived from their declared rate is \
             not the window the ROM's screen verdict budgets against",
            clip.name
        );
        let true_window = real_time_budget(usize::from(clip.max_blocksize), clip.sample_rate_hz);
        assert_eq!(
            true_window, 1_048_750,
            "{}: true worst-window arithmetic",
            clip.name
        );
        assert_eq!(
            real_time_budget(2048, 32_768),
            1_048_750,
            "{}: ROM verdict budget (the plan's 512 cycles/sample arithmetic)",
            clip.name
        );
        assert!(
            cost.max <= true_window,
            "{}: table-derived worst-frame allowance {} exceeds true window {true_window}",
            clip.name,
            cost.max
        );
        // min/max identity guards: every window is positive (no zero-length
        // frames in the table), and worst == min only for a constant table.
        assert!(cost.min > 0, "{}: zero-length frame window", clip.name);
        let expect_worst = clip
            .frames
            .iter()
            .map(|m| real_time_budget(usize::from(m.blocksize), clip.sample_rate_hz))
            .max()
            .unwrap();
        assert_eq!(cost.max, expect_worst, "{}: worst window", clip.name);
    }
}

// ---------------------------------------------------------------------------
// Layer 6: copy drift (2026-11-04). `crates/flac-lite/src/driver.rs` is a
// narrow copy of THIS file's frame loop, made deliberately: the module header
// there explains why the instrument could not simply move (its `decode_one` is
// the `ctrl` arm of every recorded E measurement, and the cross-session anchors
// depend on that call shape being unchanged). Copying buys separation; the cost
// is drift, and these two tests are how it is watched — the same discipline the
// E-sample suites use for vendored loops ("re-verify in context", not "assume
// the sample still behaves").
// ---------------------------------------------------------------------------

/// The spike's seek table re-expressed as the crate's index type. Field for
/// field, because the *only* difference is the type name — which is itself the
/// point of the copy: the crate's `FrameSource` takes borrowed decode inputs
/// and nothing else, so the witness-only fields (`name`, `why`, `census`,
/// `total_samples`, `pcm_file`, the FNV pins) stay on this side.
fn core_frames(clip: &flac_spike::assets::SpikeClip) -> Vec<flac_lite::driver::FrameMeta> {
    clip.frames
        .iter()
        .map(|m| flac_lite::driver::FrameMeta {
            offset: m.offset,
            blocksize: m.blocksize,
        })
        .collect()
}

/// The borrowed decode inputs the crate's loop needs, built from a clip: same
/// table, same embedded region bytes, same stream parameters.
fn core_source<'a>(
    clip: &flac_spike::assets::SpikeClip,
    frames: &'a [flac_lite::driver::FrameMeta],
) -> flac_lite::driver::FrameSource<'a> {
    flac_lite::driver::FrameSource {
        frames,
        region: clip.region,
        max_blocksize: clip.max_blocksize,
        channels: clip.channels,
        defaults: flac_lite::frame::StreamDefaults {
            sample_rate_hz: clip.sample_rate_hz,
            bits_per_sample: clip.bits_per_sample,
        },
    }
}

#[test]
fn the_crate_frame_loop_reaches_the_pcm_pins() {
    // The copy's own correctness proof, on the ROM's exact embedded bytes: the
    // crate's `decode_all` walked through the *same* shared fold this suite
    // uses for the spike driver (`checksum::fold_i16le_stereo`) must reach the
    // generator's PCM pin. Pin == hash of the reference blob is asserted in
    // `pcm_blobs_match_their_pins_and_names`, so reaching it here means the
    // crate's loop decodes bit-exactly against `flac -d` — not merely "the
    // same as the spike", which would inherit a shared mistake.
    use flac_lite::driver as core;
    for clip in CLIPS {
        let frames = core_frames(clip);
        let source = core_source(clip, &frames);
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut hash = Fnv1a64::new();

        let stats = core::decode_all(
            &source,
            &mut left,
            &mut right,
            &mut state,
            |_frame, l, r| {
                fold_i16le_stereo(&mut hash, l, r)
                    .expect("bit-exact decode of a 16-bit stream always fits i16")
            },
        )
        .unwrap_or_else(|e| panic!("{}: crate frame-loop walk failed: {e:?}", clip.name));

        assert_eq!(stats.frames, clip.frames.len(), "{}: frames", clip.name);
        assert_eq!(
            stats.samples,
            usize::try_from(clip.total_samples).unwrap(),
            "{}: samples per channel",
            clip.name
        );
        assert_eq!(
            hash.finish(),
            clip.fnv_pcm,
            "{}: the crate's copied loop must reach the generator's PCM pin",
            clip.name
        );
    }
}

#[test]
fn the_crate_frame_loop_agrees_with_this_driver_frame_by_frame() {
    // The drift witness proper: both implementations decode the SAME frame
    // index, over the SAME embedded region, into their OWN buffers and with
    // their OWN predictor state (a shared array or shared buffer would let one
    // side's scratch hide the other's divergence), and every window must be
    // byte-identical. The two whole-index walkers are compared too — the unit
    // both are built from agreeing does not by itself prove the loop bounds,
    // the callback window widths, and the totals still agree.
    use flac_lite::driver as core;
    for clip in CLIPS {
        let frames = core_frames(clip);
        let source = core_source(clip, &frames);
        let max = usize::from(clip.max_blocksize);

        let mut spike_left = vec![0i32; max];
        let mut spike_right = vec![0i32; max];
        let mut core_left = vec![0i32; max];
        let mut core_right = vec![0i32; max];
        let mut spike_state = Default::default();
        let mut core_state = Default::default();

        for frame in 0..clip.frames.len() {
            let a = driver::decode_one(
                clip,
                frame,
                &mut spike_left,
                &mut spike_right,
                &mut spike_state,
            )
            .unwrap_or_else(|e| panic!("{}: spike driver frame {frame}: {e:?}", clip.name));
            let b = core::decode_one(
                &source,
                frame,
                &mut core_left,
                &mut core_right,
                &mut core_state,
            )
            .unwrap_or_else(|e| panic!("{}: crate driver frame {frame}: {e:?}", clip.name));
            assert_eq!(a, b, "{}: frame {frame} written count", clip.name);
            assert_eq!(
                a,
                usize::from(clip.frames[frame].blocksize),
                "{}: frame {frame} blocksize vs the seek table",
                clip.name
            );
            assert_eq!(
                spike_left[..a],
                core_left[..a],
                "{}: frame {frame} left window drift",
                clip.name
            );
            assert_eq!(
                spike_right[..a],
                core_right[..a],
                "{}: frame {frame} right window drift",
                clip.name
            );
        }

        // And the walkers: same index, same totals.
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut spike_state = Default::default();
        let spike_stats =
            driver::decode_clip(clip, &mut left, &mut right, &mut spike_state, |_, _, _| {})
                .unwrap_or_else(|e| panic!("{}: spike walk failed: {e:?}", clip.name));
        let mut core_state = Default::default();
        let core_stats = core::decode_all(
            &source,
            &mut left,
            &mut right,
            &mut core_state,
            |_, _, _| {},
        )
        .unwrap_or_else(|e| panic!("{}: crate walk failed: {e:?}", clip.name));
        assert_eq!(
            (spike_stats.frames, spike_stats.samples),
            (core_stats.frames, core_stats.samples),
            "{}: whole-index walk totals",
            clip.name
        );
    }
}
