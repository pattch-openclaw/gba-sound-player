//! Rate-flow host witness (FLAC.md "Sample-rate reduction" Part 1 step 2's
//! gate): the GENERATED rate clips decode **bit-exactly** on the host, from
//! the exact bytes the rate ROM embeds, before any cycle number counts.
//!
//! Same correctness-before-speed layering as `spike_witness.rs` — but on
//! temporary, input-derived assets instead of the committed ones, so it is
//! a separate suite, gated by `required-features = ["rate-assets"]`:
//! `make spike-test` never compiles it; `make rate-test` runs it. It fails
//! (not skips) when the feature is on and the generated modules are missing
//! — the `include!` paths name the files cargo needs generated first
//! (`make rate-assets INPUT=… RATE=…`).
//!
//! Layers (mirroring the committed suite's pin taxonomy):
//! 1. provenance — every clip's rate == the generated RATE_HZ; the budget
//!    const re-derived through the shared const fn (never recalled).
//! 2. hash pins — Rust FNV reproduces the generator's region + PCM pins.
//! 3. bit-exact decode — production driver over the embedded region vs the
//!    `flac -d` reference blob, sample-for-sample.
//! 4. cross-arm agreement — both arms encode the same normalized source.
//! 5. seek-table structure + corrupted-offset negative control (with the
//!    pristine-frame control decode).
//! 6. FIFO8 playback pins — the generator's Python `>>8` fold and the
//!    library's Rust fold meet on the reference PCM, and the ROM's own
//!    data path (`playback::fold_fifo8` through the driver) reaches the
//!    same pins from the embedded region bytes.

use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
use flac_spike::driver;
use flac_spike::playback::{fifo_byte, fold_fifo8};
use flac_spike::stats::real_time_budget;

/// The generated clip module — the very file the rate ROM `include!`s.
/// Included here too so this suite judges the ROM's own bytes, not a copy.
mod rate {
    include!("../src/rate_assets.rs");
}

/// The generated reference-PCM module (host ground truth only — the ROM
/// pins these by hash, never embeds them).
mod pcm {
    include!("../src/rate_pcm.rs");
}

#[test]
fn provenance_and_derived_budget_hold() {
    assert!(
        matches!(rate::RATE_HZ, 10512 | 18157 | 32768),
        "generated rate {} outside the mixer domain — the generator was \
         supposed to fail closed, this suite is its second witness",
        rate::RATE_HZ
    );
    // The budget is derived, at the pinned rate, through the SAME const fn
    // the ROM's verdict uses — asserted, never recalled.
    assert_eq!(
        rate::BUDGET_CYCLES_PER_FRAME,
        real_time_budget(2048, rate::RATE_HZ),
        "budget const drifted from its own derivation"
    );
    for clip in rate::RATE_CLIPS {
        assert_eq!(
            clip.sample_rate_hz,
            rate::RATE_HZ,
            "{}: encode rate != pinned playback rate (the rate proposal's \
             fail-closed rule)",
            clip.name
        );
        assert_eq!(clip.bits_per_sample, 16, "{}: 16-bit profile", clip.name);
        assert_eq!(clip.channels, 2, "{}: stereo profile", clip.name);
        assert_eq!(
            usize::from(clip.max_blocksize),
            2048,
            "{}: fixed blocksize profile",
            clip.name
        );
    }
    assert!(!rate::INPUT_SHA256.is_empty(), "input sha tripwire empty");
}

#[test]
fn rust_fnv_reproduces_the_generators_region_pins() {
    for clip in rate::RATE_CLIPS {
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
    // Positional mapping: RATE_CLIPS is generated [l0, l4], the PCM module
    // is generated [L0_PCM, L4_PCM] from the same arm loop.
    let blobs = [pcm::L0_PCM, pcm::L4_PCM];
    for (clip, blob) in rate::RATE_CLIPS.iter().zip(blobs) {
        assert_eq!(
            fnv1a64(blob),
            clip.fnv_pcm,
            "{}: reference PCM blob no longer matches the generator's pin",
            clip.name
        );
        let suffix = if clip.name.ends_with("_l0") {
            "_l0_pcm.bin"
        } else if clip.name.ends_with("_l4") {
            "_l4_pcm.bin"
        } else {
            panic!("{}: unexpected arm name", clip.name)
        };
        assert!(
            clip.pcm_file.ends_with(suffix),
            "{}: pcm_file {} must name the blob this suite includes",
            clip.name,
            clip.pcm_file
        );
    }
}

#[test]
fn decode_is_bit_exact_against_the_reference_pcm() {
    for clip in rate::RATE_CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut got: Vec<u8> = Vec::with_capacity(
            usize::try_from(clip.total_samples).unwrap() * usize::from(clip.channels) * 2,
        );

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
        let reference = if clip.name.ends_with("_l0") {
            pcm::L0_PCM
        } else {
            pcm::L4_PCM
        };
        assert_eq!(
            got, reference,
            "{}: decoded PCM is not bit-exact against `flac -d` reference",
            clip.name
        );
    }
}

#[test]
fn rom_fold_reproduces_pcm_pins_on_host() {
    // The ROM's decode proof folds through the SHARED fold; the host drives
    // the same function through the same driver to the same pins — so the
    // on-target hash is not a second transcription of the interleave rule.
    for clip in rate::RATE_CLIPS {
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
    }
}

#[test]
fn both_arms_decode_the_same_source() {
    // One normalized input, two encodes => one ground truth.
    assert_eq!(pcm::L0_PCM, pcm::L4_PCM);
}

#[test]
fn seek_table_structural_invariants_hold() {
    for clip in rate::RATE_CLIPS {
        assert!(!clip.frames.is_empty(), "{}: empty seek table", clip.name);
        assert_eq!(clip.frames[0].offset, 0, "{}: frame 0", clip.name);
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
fn fifo8_pins_meet_generator_library_and_rom_path() {
    // Three implementations of the playback data path must agree:
    // (a) the generator's Python fold that emitted the pins,
    // (b) the library's `fifo_byte` applied to the reference PCM,
    // (c) the ROM's exact sequence: driver decode -> `fold_fifo8`.
    // (b) vs the pins is the Python-vs-Rust differential (arithmetic-shift
    // semantics included); (c) vs the pins proves the bytes the DMA feeder
    // will play are the reference bytes, through the shared library code.
    for (clip, (pin_l, pin_r)) in rate::RATE_CLIPS.iter().zip(rate::FIFO8_PINS) {
        let reference = if clip.name.ends_with("_l0") {
            pcm::L0_PCM
        } else {
            pcm::L4_PCM
        };
        // (b) Library fold over the reference PCM, per channel.
        let l_bytes: Vec<u8> = reference
            .chunks_exact(4)
            .map(|p| fifo_byte(i16::from_le_bytes([p[0], p[1]]) as i32))
            .collect();
        let r_bytes: Vec<u8> = reference
            .chunks_exact(4)
            .map(|p| fifo_byte(i16::from_le_bytes([p[2], p[3]]) as i32))
            .collect();
        assert_eq!(
            l_bytes.len(),
            usize::try_from(clip.total_samples).unwrap(),
            "{}: L FIFO8 length",
            clip.name
        );
        assert_eq!(
            fnv1a64(&l_bytes),
            pin_l,
            "{}: Python generator vs library `fifo_byte` on the reference PCM",
            clip.name
        );
        assert_eq!(
            fnv1a64(&r_bytes),
            pin_r,
            "{}: Python generator vs library `fifo_byte` on the reference PCM (R)",
            clip.name
        );

        // (c) The ROM's exact sequence on the embedded region bytes.
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        let stats = driver::decode_clip(clip, &mut left, &mut right, &mut state, |_frame, l, r| {
            fold_fifo8(&mut hl, &mut hr, l, r)
                .expect("bit-exact decode of a 16-bit stream always fits i16")
        })
        .unwrap_or_else(|e| panic!("{}: clip walk failed: {e:?}", clip.name));
        assert_eq!(stats.samples, usize::try_from(clip.total_samples).unwrap());
        assert_eq!(
            hl.finish(),
            pin_l,
            "{}: ROM fold path must reach the generator's FIFO8 L pin",
            clip.name
        );
        assert_eq!(
            hr.finish(),
            pin_r,
            "{}: ROM fold path must reach the generator's FIFO8 R pin",
            clip.name
        );
    }
    // The playback aliases name arm 0 — the ROM's single judgement.
    assert_eq!(rate::PLAYBACK_FIFO8_L, rate::FIFO8_PINS[0].0);
    assert_eq!(rate::PLAYBACK_FIFO8_R, rate::FIFO8_PINS[0].1);
}

#[test]
fn a_corrupted_seek_entry_breaks_the_walk() {
    // Negative control on the generated table (LPC arm), with the pristine
    // control decode of the same frame — the committed suite's shape.
    let clip = rate::RATE_CLIPS[1];
    let index = clip.frames.len() / 2;
    let mut frames: Vec<flac_spike::assets::FrameMeta> = clip.frames.to_vec();
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
    let ok = driver::decode_one(clip, index, &mut left, &mut right, &mut state)
        .expect("pristine table frame must decode");
    assert_eq!(ok, usize::from(clip.frames[index].blocksize));
    assert!(
        !format!("{err:?}").is_empty(),
        "error must be descriptive: {err:?}"
    );
}
