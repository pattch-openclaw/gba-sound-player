//! E10 probe witness: the vendored **hybrid ARM codeword loop** decode path
//! (`e10::decode_one_arm`) decodes the exact embedded clips **bit-exactly**
//! against the `flac -d` reference PCM — the same layer-2 witness
//! `spike_witness.rs` runs for the production path and `e2b_witness.rs`
//! runs for the table arm. On the host the path runs the step-for-step
//! twin (`e10::twin_partition` — there is no ARM executor here); the ROM's
//! per-frame sample-equality witness proves the assembly implements the
//! same semantics on the measured image.
//!
//! Second layer: a differential frame-by-frame sample comparison against
//! the production `driver::decode_one` on the same bytes — the host twin
//! of the ROM's equality witness.
//!
//! Third layer (E10-specific): the **tier census** of the hybrid over the
//! exact embedded bytes (the algorithm plan's "per-clip quotient
//! distribution decides how much" — the C2 tail question), plus the
//! fold-source pin: the probe vendor's `asm/fold_body.s` must stay
//! byte-identical to the kernel crate's committed copy (decision 6's
//! no-drift property, extended to the vendoring).
//!
//! Sample code, never lands on `main`'s spike sources (see
//! `experiments/e10-three-arm-probe/`).

use flac_spike::assets::{CLIPS, L0_FIXED, L4_LPC};
use flac_spike::driver;
use flac_spike::e10;

/// The reference PCM blobs (host ground truth only — same blobs
/// `spike_witness.rs` includes; re-included so this file is self-contained).
const L0_PCM: &[u8] = include_bytes!("../assets/spike_l0_pcm.bin");
const L4_PCM: &[u8] = include_bytes!("../assets/spike_l4_pcm.bin");

fn pcm_for(name: &str) -> &'static [u8] {
    if name == L0_FIXED.name {
        L0_PCM
    } else if name == L4_LPC.name {
        L4_PCM
    } else {
        panic!("e10 witness does not know a blob for arm {name:?}");
    }
}

#[test]
fn hybrid_arm_variant_decodes_bit_exact_against_the_reference_pcm() {
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let expected_len =
            usize::try_from(clip.total_samples).unwrap() * usize::from(clip.channels) * 2;
        let mut got: Vec<u8> = Vec::with_capacity(expected_len);

        for frame_index in 0..clip.frames.len() {
            let written = e10::decode_one_arm(clip, frame_index, &mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {frame_index} failed: {e:?}", clip.name));
            for k in 0..written {
                let (a, b) = (
                    i16::try_from(left[k]).expect("decoded left fits i16 (16-bit profile)"),
                    i16::try_from(right[k]).expect("decoded right fits i16 (16-bit profile)"),
                );
                got.extend_from_slice(&a.to_le_bytes());
                got.extend_from_slice(&b.to_le_bytes());
            }
        }

        assert_eq!(
            got,
            pcm_for(clip.name),
            "{}: the hybrid ARM variant is not bit-exact against `flac -d`",
            clip.name
        );
    }
}

#[test]
fn hybrid_arm_variant_matches_production_driver_frame_by_frame() {
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut ctrl_l = vec![0i32; max];
        let mut ctrl_r = vec![0i32; max];
        let mut var_l = vec![0i32; max];
        let mut var_r = vec![0i32; max];
        let mut state_c = Default::default();
        let mut state_v = Default::default();

        for frame_index in 0..clip.frames.len() {
            let w_ctrl =
                driver::decode_one(clip, frame_index, &mut ctrl_l, &mut ctrl_r, &mut state_c)
                    .unwrap_or_else(|e| panic!("{}: ctrl frame {frame_index}: {e:?}", clip.name));
            let w_var =
                e10::decode_one_arm(clip, frame_index, &mut var_l, &mut var_r, &mut state_v)
                    .unwrap_or_else(|e| panic!("{}: var frame {frame_index}: {e:?}", clip.name));
            assert_eq!(
                w_ctrl, w_var,
                "{}: frame {frame_index} blocksize",
                clip.name
            );
            for k in 0..w_ctrl {
                assert_eq!(
                    ctrl_l[k], var_l[k],
                    "{}: frame {frame_index} left[{k}]",
                    clip.name
                );
                assert_eq!(
                    ctrl_r[k], var_r[k],
                    "{}: frame {frame_index} right[{k}]",
                    clip.name
                );
            }
        }
    }
}

#[test]
fn tier_census_over_the_exact_embedded_bytes() {
    // The plan's distribution question, answered on the measured bytes:
    // how much of the residual mass lands in each tier, and how big is
    // the C2 (tier-3) tail the word-skip prices. Identity + zero-status
    // asserts are gates; the counts are evidence for the FLAC.md entry
    // (print with --nocapture).
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max];
        let mut state = Default::default();
        let mut stats = e10::TierStats::default();
        for frame_index in 0..clip.frames.len() {
            e10::decode_one_arm_stats(
                clip,
                frame_index,
                &mut left,
                &mut right,
                &mut state,
                &mut stats,
            )
            .unwrap_or_else(|e| panic!("{}: frame {frame_index}: {e:?}", clip.name));
        }
        assert_eq!(
            stats.unterm, 0,
            "{}: unterminated on committed clips",
            clip.name
        );
        assert_eq!(
            stats.escapes, 0,
            "{}: escape partitions off-profile",
            clip.name
        );
        assert!(
            stats.max_p <= 14,
            "{}: Rice order {} past the kernel contract",
            clip.name,
            stats.max_p
        );
        assert_eq!(
            stats.samples,
            stats.t1 + stats.t2 + stats.t3 + stats.slow,
            "{}: tier identity",
            clip.name
        );
        assert!(stats.samples > 0);
        println!(
            "e10 tier census {}: samples={} t1={} ({:.2}%) t2={} ({:.2}%) t3={} ({:.2}%) slow={} refill_bytes={} max_p={}",
            clip.name,
            stats.samples,
            stats.t1,
            100.0 * stats.t1 as f64 / stats.samples as f64,
            stats.t2,
            100.0 * stats.t2 as f64 / stats.samples as f64,
            stats.t3,
            100.0 * stats.t3 as f64 / stats.samples as f64,
            stats.slow,
            stats.refill_bytes,
            stats.max_p,
        );
    }
}

#[test]
fn vendored_fold_is_byte_identical_to_the_kernel_crate_copy() {
    // Decision 6's no-drift property across the vendoring: the probe's
    // asm/fold_body.s is a verbatim copy of the kernel crate's committed
    // single source. A drift here means the probe measured a different
    // fold than the track witnessed.
    let vendored: &[u8] = include_bytes!("../asm/fold_body.s");
    let committed: &[u8] = include_bytes!("../../arm-kernels/asm/fold_body.s");
    assert_eq!(
        vendored, committed,
        "the probe's fold_body.s drifted from the kernel crate's committed copy"
    );
}
