//! S2's forward transform, against the C reference's own coefficients.
//!
//! `tests/fixtures/mdct_in.f32` is a deterministic 2,048-sample test album and
//! `mdct_ref_coef.f32` is what `mdct_frame()` in `gba-audio/encoder/enc_core.c`
//! produces from it, frozen by `scripts/gen_uli_mdct_fixture.py`. E-R5 rule R5c
//! says the analysis constants are checked in rather than called for; this is what
//! proves the checked-in values plus the ported arithmetic give the reference's
//! answer and not merely a close one.

mod common;

use common::TempDir;
use uli_enc::Samples;
use uli_enc::mdct::mdct_frame;
use uli_lite::format::{M, NCODED};

const IN: &[u8] = include_bytes!("fixtures/mdct_in.f32");
const REF: &[u8] = include_bytes!("fixtures/mdct_ref_coef.f32");
const N_SAMPLES: usize = 2048;
const FRAMES: usize = 4;

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn the_forward_transform_is_the_reference_bit_for_bit() {
    let dir = TempDir::new("mdct");
    let album = Samples::open(
        &common::materialise(dir.path(), "mdct_in.f32", IN),
        N_SAMPLES,
    )
    .expect("input");
    let ref_coef = f32s(REF);
    assert_eq!(ref_coef.len(), FRAMES * 2 * M);

    let mut worst = 0u64;
    let mut worst_at = (0usize, 0usize, 0u32, 0u32);
    let mut mismatched = 0usize;
    for f in 0..FRAMES {
        let mut mine = [0f32; 2 * M];
        mdct_frame(&album, f, N_SAMPLES, &mut mine);
        for (k, v) in mine.iter().enumerate() {
            let theirs = ref_coef[f * 2 * M + k];
            if v.to_bits() != theirs.to_bits() {
                mismatched += 1;
                let d = (v.to_bits() as i64 - theirs.to_bits() as i64).unsigned_abs();
                if d > worst {
                    worst = d;
                    worst_at = (f, k, v.to_bits(), theirs.to_bits());
                }
            }
        }
    }
    assert_eq!(
        mismatched,
        0,
        "{mismatched} of {} coefficients differ from the reference; worst bit-pattern gap \
         {worst} at frame {} k={} ours {:#010x} reference {:#010x}. Any difference at all means \
         the checked-in constants or the operation order moved away from `enc_core.c`.",
        FRAMES * 2 * M,
        worst_at.0,
        worst_at.1,
        worst_at.2,
        worst_at.3
    );
}

/// Not a tautology: it asserts the reference *also* zeroed the top coefficients,
/// which is what makes forcing them in `mdct_frame` the format's rule rather than
/// a local convention. Frame 0 is the preroll and frame 3 reads past the end of
/// the album, so this fixture covers both out-of-bounds edges as well.
#[test]
fn the_reference_zeroes_every_coefficient_at_or_beyond_ncoded() {
    let ref_coef = f32s(REF);
    for f in 0..FRAMES {
        for ch in 0..2 {
            let base = f * 2 * M + ch * M;
            assert!(
                ref_coef[base + NCODED..base + M].iter().all(|&v| v == 0.0),
                "frame {f} channel {ch}: the reference left energy past NCODED — the fixture's \
                 claim about the format rule needs re-reading"
            );
            assert!(
                ref_coef[base..base + NCODED].iter().any(|&v| v != 0.0),
                "frame {f} channel {ch}: no coefficients at all, so the assertion above is vacuous"
            );
        }
    }
}
