//! Vendor patch 2 witness (PATCHES.md): the in-place decoder constructor
//! (`OpusDecoder::new_in_place`) must build **exactly the decoder `new()`
//! builds** — written directly into the heap slot instead of through a
//! ~178 KB by-value stack temporary.
//!
//! Why these three layers (the patch is pure construction; the decode
//! algorithms are untouched — the witness proves the construction writes the
//! same state, which is all the patch claims):
//!
//! 1. **Bit-identical differential** — the embedded arms decode through a
//!    `new()` decoder and a `new_in_place()` decoder side by side, every
//!    packet, raw f32 compared bit-for-bit. Two decoders of the SAME code on
//!    one machine have no drift to tolerate: any initialization difference
//!    (a mis-sized `FixedVec`, a skipped sub-decoder init, a stale field)
//!    diverges the state machine's output at or before the divergence point.
//!    501 packets × 2 arms of sequential state-carried decode is a
//!    field-by-field exercise of everything the decode path reads.
//! 2. **The committed golden** — the walk from the in-place ctor reproduces
//!    each arm's generator-measured `fnv_walk_fold` pin (the same golden the
//!    ROM decode proof chases, measured through the by-value seam). Layer 1
//!    proves the two ctors agree; this layer proves they agree with the
//!    ground truth, so a shared construction mistake cannot pass both.
//! 3. **Contract parity** — the five rates × mono/stereo construct; every
//!    rejection returns `new()`'s exact error string (validation runs before
//!    the allocation, so a rejection allocates nothing). Building and
//!    dropping all ten states also exercises drop soundness on the
//!    in-place-initialized memory.
//!
//! Coverage note (deliberate, PATCHES.md Patch 2): the embedded arms are
//! mono, so the differential exercises mono decode; stereo-only state is
//! written by the same `init_in_place` path with the `channels` parameter
//! and is covered here by construction + drop only, until the gate grows
//! stereo material.

use opus_spike::assets::{CLIPS, OpusClip};
use opus_spike::checksum::{Fnv1a64, fold_f32_window};
use opus_spike::probe::{
    OpusDecoder, SUPPORTED_RATES_HZ, decode_packet, frame_samples, new_decoder,
};

/// The packet slices of an arm, straight from its manifest.
fn packets(clip: &OpusClip) -> Vec<&[u8]> {
    clip.index
        .iter()
        .map(|entry| {
            let start = entry.offset as usize;
            &clip.region[start..start + entry.len as usize]
        })
        .collect()
}

/// Layer 1: same code, two construction shapes — outputs must be
/// bit-identical (no tolerance: this compares a decoder against itself, not
/// against the external reference).
#[test]
fn in_place_ctor_decodes_bit_identically_to_the_by_value_ctor() {
    for clip in CLIPS {
        let rate = clip.sample_rate_hz as i32;
        let channels = clip.channels as usize;
        let mut legacy = new_decoder(rate, channels).expect("by-value ctor must succeed");
        let mut patched = OpusDecoder::new_in_place(rate, channels).expect("in-place ctor");
        let n = frame_samples(clip.sample_rate_hz);
        for (i, packet) in packets(clip).iter().enumerate() {
            let mut a = vec![0.0f32; n];
            let mut b = vec![0.0f32; n];
            let da = decode_packet(&mut legacy, rate, packet, &mut a).unwrap_or_else(|e| {
                panic!("{} packet {i}: by-value decode rejected: {e}", clip.name)
            });
            let db = decode_packet(&mut patched, rate, packet, &mut b).unwrap_or_else(|e| {
                panic!("{} packet {i}: in-place decode rejected: {e}", clip.name)
            });
            assert_eq!((da, db), (n, n), "{} packet {i}: geometry drift", clip.name);
            for (j, (x, y)) in a.iter().zip(&b).enumerate() {
                assert_eq!(
                    x.to_bits(),
                    y.to_bits(),
                    "{} packet {i} sample {j}: the in-place ctor decoded differently \
                     from the by-value ctor — construction wrote different state",
                    clip.name
                );
            }
        }
    }
}

/// Layer 2: the in-place walk reproduces the generator's `fnv_walk_fold`
/// pins — the ROM decode proof's golden, measured through the OLD
/// construction (dump_walk over `walk_region`). Independent of layer 1.
#[test]
fn in_place_ctor_reproduces_the_walk_fold_goldens() {
    for clip in CLIPS {
        let rate = clip.sample_rate_hz as i32;
        let n = frame_samples(clip.sample_rate_hz);
        let mut decoder = OpusDecoder::new_in_place(rate, clip.channels as usize)
            .expect("in-place ctor at the arm profile");
        let mut hash = Fnv1a64::new();
        let mut window = vec![0.0f32; n];
        let mut samples = 0usize;
        for (i, packet) in packets(clip).iter().enumerate() {
            let decoded = decode_packet(&mut decoder, rate, packet, &mut window)
                .unwrap_or_else(|e| panic!("{} packet {i}: decode rejected: {e}", clip.name));
            assert_eq!(decoded, n, "{} packet {i}: geometry drift", clip.name);
            fold_f32_window(&mut hash, &window);
            samples += n;
        }
        assert_eq!(
            samples, clip.walk_samples as usize,
            "{}: walk length",
            clip.name
        );
        assert_eq!(
            hash.finish(),
            clip.fnv_walk_fold,
            "{}: the in-place ctor's folded walk does not reproduce the golden",
            clip.name
        );
    }
}

/// Layer 3: constructor contract parity — the five rates × mono/stereo
/// construct; rejections carry `new()`'s exact error strings, and every
/// built state drops soundly (the test owns the boxes to the closing brace).
#[test]
fn in_place_ctor_keeps_the_constructor_contract_and_rejections() {
    for rate in SUPPORTED_RATES_HZ {
        for channels in [1usize, 2] {
            assert!(
                OpusDecoder::new_in_place(rate, channels).is_ok(),
                "in-place ctor: {rate} Hz x {channels} ch must construct"
            );
        }
    }
    // Rejections: identical to the by-value ctor's contract, error strings
    // included (validation happens before the allocation in both).
    for bad_rate in [0, 7999, 44100, 65536, 96000, -16000] {
        assert_eq!(
            OpusDecoder::new_in_place(bad_rate, 1).err(),
            new_decoder(bad_rate, 1).err(),
            "in-place ctor: {bad_rate} Hz must be rejected like `new` rejects it"
        );
    }
    for bad_channels in [0usize, 3] {
        assert_eq!(
            OpusDecoder::new_in_place(48000, bad_channels).err(),
            new_decoder(48000, bad_channels).err(),
            "in-place ctor: {bad_channels} channels must be rejected like `new` rejects them"
        );
    }
}
