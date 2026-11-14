//! Fixed-point IMDCT synthesis: pre-twiddle, 256-point radix-4 DIF FFT,
//! post-twiddle, and window/overlap-add.
//!
//! This is a line-by-line port of `uli_imdct_pre` / `uli_imdct_fft` /
//! `uli_imdct_post` / `uli_imdct_ola` in `gba-audio/src/uli/uli_dec_ref.c`
//! (bench variant P, N = 1024). The ARM overlay can replace these functions
//! with the measured assembly kernels; the arithmetic here is the portable
//! reference and must stay bit-exact with them.

use crate::format::{H, M};
use crate::tables_data::{PERM, POST, PRE, TW, WIN};

#[inline]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn mul32(a: i32, b: i32) -> i64 {
    // Thumb-1 has no 64-bit multiply, so this expression emits `__aeabi_lmul`
    // (a software 64-bit multiply) unless it is compiled in ARM mode. The
    // Phase 1b measurement (ULI2.md) found 44 such call sites and a kernel
    // sequence ~37x the C reference's, because every twiddle product took that
    // path.
    // The kernels carry `instruction_set(arm::a32)`, where LLVM lowers this
    // exact expression to SMULL -- the instruction the C reference's gcc ARM
    // build uses. The arithmetic is unchanged (full 64-bit signed product).
    (a as i64) * (b as i64)
}

#[inline]
fn hi32(v: i64) -> i32 {
    (v >> 32) as i32
}

#[inline]
fn sat16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

#[inline]
fn sat8(v: i32) -> i8 {
    v.clamp(i8::MIN as i32, i8::MAX as i32) as i8
}

#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
pub fn pre(x: &[i32; M], z: &mut [i32; 2 * H]) {
    for k in 0..H {
        let xr = x[2 * k];
        let xi = x[M - 1 - 2 * k];
        let wr = PRE[2 * k];
        let wi = PRE[2 * k + 1];
        z[2 * k] = hi32(mul32(xr, wr).wrapping_add(mul32(xi, wi.wrapping_neg())));
        z[2 * k + 1] = hi32(mul32(xr, wi).wrapping_add(mul32(xi, wr)));
    }
}

#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
pub fn fft(x: &mut [i32; 2 * H]) {
    let n = H;
    let mut tw = 0usize;
    let mut l = n;
    while l >= 4 {
        let q = l >> 2;
        if l == 4 {
            let mut p = 0usize;
            while p < 2 * n {
                let ar = x[p];
                let ai = x[p + 1];
                let br = x[p + 2];
                let bi = x[p + 3];
                let cr = x[p + 4];
                let ci = x[p + 5];
                let dr = x[p + 6];
                let di = x[p + 7];
                let t0r = ar.wrapping_add(cr);
                let t0i = ai.wrapping_add(ci);
                let t1r = ar.wrapping_sub(cr);
                let t1i = ai.wrapping_sub(ci);
                let t2r = br.wrapping_add(dr);
                let t2i = bi.wrapping_add(di);
                let t3r = br.wrapping_sub(dr);
                let t3i = bi.wrapping_sub(di);
                x[p] = t0r.wrapping_add(t2r) >> 2;
                x[p + 1] = t0i.wrapping_add(t2i) >> 2;
                x[p + 2] = t1r.wrapping_add(t3i) >> 2;
                x[p + 3] = t1i.wrapping_sub(t3r) >> 2;
                x[p + 4] = t0r.wrapping_sub(t2r) >> 2;
                x[p + 5] = t0i.wrapping_sub(t2i) >> 2;
                x[p + 6] = t1r.wrapping_sub(t3i) >> 2;
                x[p + 7] = t1i.wrapping_add(t3r) >> 2;
                p += 8;
            }
            l >>= 2;
            continue;
        }

        for j in 0..q {
            let w1r = TW[tw];
            let w1i = TW[tw + 1];
            let w2r = TW[tw + 2];
            let w2i = TW[tw + 3];
            let w3r = TW[tw + 4];
            let w3i = TW[tw + 5];
            tw += 6;

            let mut p = 2 * j;
            while p < 2 * n {
                let p1 = p + 2 * q;
                let p2 = p1 + 2 * q;
                let p3 = p2 + 2 * q;
                let ar = x[p];
                let ai = x[p + 1];
                let br = x[p1];
                let bi = x[p1 + 1];
                let cr = x[p2];
                let ci = x[p2 + 1];
                let dr = x[p3];
                let di = x[p3 + 1];
                let t0r = ar.wrapping_add(cr);
                let t0i = ai.wrapping_add(ci);
                let t1r = ar.wrapping_sub(cr);
                let t1i = ai.wrapping_sub(ci);
                let t2r = br.wrapping_add(dr);
                let t2i = bi.wrapping_add(di);
                let t3r = br.wrapping_sub(dr);
                let t3i = bi.wrapping_sub(di);
                x[p] = t0r.wrapping_add(t2r) >> 2;
                x[p + 1] = t0i.wrapping_add(t2i) >> 2;

                let yr = t1r.wrapping_add(t3i);
                let yi = t1i.wrapping_sub(t3r);
                x[p1] = hi32(mul32(yr, w1r).wrapping_add(mul32(yi, w1i.wrapping_neg())));
                x[p1 + 1] = hi32(mul32(yr, w1i).wrapping_add(mul32(yi, w1r)));

                let yr = t0r.wrapping_sub(t2r);
                let yi = t0i.wrapping_sub(t2i);
                x[p2] = hi32(mul32(yr, w2r).wrapping_add(mul32(yi, w2i.wrapping_neg())));
                x[p2 + 1] = hi32(mul32(yr, w2i).wrapping_add(mul32(yi, w2r)));

                let yr = t1r.wrapping_sub(t3i);
                let yi = t1i.wrapping_add(t3r);
                x[p3] = hi32(mul32(yr, w3r).wrapping_add(mul32(yi, w3i.wrapping_neg())));
                x[p3 + 1] = hi32(mul32(yr, w3i).wrapping_add(mul32(yi, w3r)));

                p += 2 * l;
            }
        }
        l >>= 2;
    }
}

#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
pub fn post(z: &[i32; 2 * H], y: &mut [i32; M]) {
    for k in 0..H {
        let p = PERM[k] as usize * 2;
        let zr = z[p];
        let zi = z[p + 1];
        let wr = POST[2 * k];
        let wi = POST[2 * k + 1];
        y[2 * k] = hi32(mul32(zr, wr).wrapping_add(mul32(zi, wi.wrapping_neg())));
        y[M - 1 - 2 * k] = hi32(mul32(zr, wi).wrapping_add(mul32(zi, wr))).wrapping_neg();
    }
}

#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
pub fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], mut out16: Option<&mut [i16]>) {
    for i in 0..H {
        let a = y[H + i];
        let b = p[H - 1 - i];
        let wa = WIN[i];
        let wb = WIN[M - 1 - i];
        let nwa = wa.wrapping_neg();
        let nwb = wb.wrapping_neg();
        let acc1 = mul32(a, wa).wrapping_add(mul32(b, nwb));
        let acc2 = mul32(a, nwb).wrapping_add(mul32(b, nwa));
        out8[i] = sat8(hi32(acc1.wrapping_add(1i64 << 39)) >> 8);
        out8[M - 1 - i] = sat8(hi32(acc2.wrapping_add(1i64 << 39)) >> 8);
        if let Some(o16) = out16.as_deref_mut() {
            o16[i] = sat16(hi32(acc1.wrapping_add(0x8000_0000)));
            o16[M - 1 - i] = sat16(hi32(acc2.wrapping_add(0x8000_0000)));
        }
        p[H - 1 - i] = y[H - 1 - i];
    }
}
