//! Differential witness: the overlay's ARM-mode, unchecked kernels are
//! bit-exact with `uli_lite::imdct`'s portable reference on the host.
//!
//! The overlay exists to change *codegen* (ARM mode; no bounds checks) and
//! nothing else. The host build has neither of those properties, so a green
//! run here proves the transcription/`unsafe` addressing, not the GBA speed.
//! The speed is the bench ROM's job; this test is what licenses the bench to
//! call the overlay a realisation of the same arithmetic.
//!
//! Inputs are a deterministic LCG over the *full* i32 range: the format's
//! `|X| < 2^28` guarantee is an encoder property, and the kernels must still
//! agree with the core when handed values outside it (both use wrapping
//! arithmetic).

use uli_lite::format::{H, M};
use uli_lite::imdct as core;

fn next(state: &mut u32) -> i32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state as i32
}

#[test]
fn pre_fft_post_ola_match_the_reference() {
    let mut state = 0x9e37_79b9u32;
    for case in 0..64 {
        let mut x = [0i32; M];
        let mut z = [0i32; 2 * H];
        let mut y = [0i32; M];
        let mut p_ref = [0i32; H];
        let mut p_ov = [0i32; H];
        let mut o8_ref_l = [0i8; M];
        let mut o8_ref_r = [0i8; M];
        let mut o8_ov_l = [0i8; M];
        let mut o8_ov_r = [0i8; M];
        let mut o16_ref = [0i16; M];
        let mut o16_ov = [0i16; M];

        for v in x.iter_mut() {
            *v = next(&mut state);
        }
        for v in z.iter_mut() {
            *v = next(&mut state);
        }
        for v in y.iter_mut() {
            *v = next(&mut state);
        }
        for v in p_ref.iter_mut() {
            *v = next(&mut state);
        }
        p_ov = p_ref;

        let mut z_ref = z;
        let mut z_ov = z;
        let mut y_ref = y;
        let mut y_ov = y;

        core::pre(&x, &mut z_ref);
        uli_lite_gba::pre(&x, &mut z_ov);
        assert_eq!(z_ref, z_ov, "pre diverged at case {case}");

        core::fft(&mut z_ref);
        uli_lite_gba::fft(&mut z_ov);
        assert_eq!(z_ref, z_ov, "fft diverged at case {case}");

        core::post(&z_ref, &mut y_ref);
        uli_lite_gba::post(&z_ov, &mut y_ov);
        assert_eq!(y_ref, y_ov, "post diverged at case {case}");

        core::ola(&y_ref, &mut p_ref, &mut o8_ref_l, Some(&mut o16_ref));
        uli_lite_gba::ola(&y_ov, &mut p_ov, &mut o8_ov_l, Some(&mut o16_ov));
        assert_eq!(o8_ref_l, o8_ov_l, "ola int8 diverged at case {case}");
        assert_eq!(o16_ref, o16_ov, "ola int16 diverged at case {case}");
        assert_eq!(p_ref, p_ov, "ola overlap state diverged at case {case}");

        // The int8-only arm must also agree; it is the product HP path.
        core::ola(&y_ref, &mut p_ref, &mut o8_ref_r, None);
        uli_lite_gba::ola(&y_ov, &mut p_ov, &mut o8_ov_r, None);
        assert_eq!(o8_ref_r, o8_ov_r, "ola int8-only diverged at case {case}");
        assert_eq!(p_ref, p_ov, "ola int8-only overlap diverged at case {case}");
    }
}

#[test]
fn pre_is_exact_on_the_encoder_bound() {
    // |X| < 2^28 is the format's guarantee; pin the boundary values too.
    let mut x = [0i32; M];
    for (i, v) in x.iter_mut().enumerate() {
        let v28 = (1i32 << 28) - 1;
        *v = if i % 3 == 0 {
            v28
        } else if i % 3 == 1 {
            -v28
        } else {
            0
        };
    }
    let mut a = [0i32; 2 * H];
    let mut b = [0i32; 2 * H];
    core::pre(&x, &mut a);
    uli_lite_gba::pre(&x, &mut b);
    assert_eq!(a, b);
}
