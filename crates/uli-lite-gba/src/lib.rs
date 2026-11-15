//! `uli-lite-gba` — the GBA overlay's first realisation of the ULI2 synthesis
//! kernels.
//!
//! This crate is the measurement seam between the portable `uli-lite` core and
//! a future hand-written ARM kernel. It reimplements the four synthesis
//! kernels (`pre`, `fft`, `post`, `ola`) with two changes over the core's
//! reference transcription, and nothing else:
//!
//! 1. **ARM mode.** Thumb-1 has no 64-bit multiply, so the core's
//!    `(a as i64) * (b as i64)` twiddle products compile to `__aeabi_lmul`
//!    (a software multiply). The canonical GBA build runs the hot kernels as
//!    ARM code, where that same expression is `SMULL`. The configuration is
//!    behavioural, not source-visible: `instruction_set(arm::a32)`.
//! 2. **No bounds checks.** The core indexes fixed-size arrays with dynamic
//!    indices, so every access carries a bounds check and the FFT's inner
//!    butterfly spills its twiddles to the stack. This crate addresses the
//!    buffers through raw pointers with the index bounds proved by the loop
//!    structure (below), which is what a hand-written kernel would do.
//!
//! The arithmetic is a line-for-line transcription of
//! `uli_lite::imdct`; that is deliberate, because the overlay is only
//! useful if it is bit-exact with the core. `tests/twin.rs` proves that on the
//! host.
//!
//! **Hand-written FFT (PR C-1).** The `fft` stage additionally has three
//! hand-written ARM kernels in `asm/` (`fft_arm.s` = v1, `fft_arm_v2.s`, and
//! `fft_arm_v3.s`), assembled with `global_asm!` and placed in `.iwram`. They
//! are enabled by the `asm-fft` feature (off by default), bit-exact with the
//! Rust kernel, and measured on the ARM7TDMI in `examples/uli_bench`. The
//! measured ladder is v1 (88,205 c/call, spill-heavy, *slower* than Rust) →
//! v2 (73,363, spill/reload traffic removed) → **v3 (71,635, `ldmia`/`stmia`
//! block transfers)** against the Rust ARM kernel's 85,065; see
//! `../../crates/uli-lite/PLAN-OF-RECORD.md` → "PR C-1". `ArmKernels::fft`
//! uses the best variant (v3) when the feature is on. v1 and v2 are kept as
//! measured comparison arms, not shipping kernels.
//!
//! # Index bounds (the `unsafe` contract)
//!
//! - `pre`: `k < H = 256`; `2k + 1 <= 511 < M`; `M - 1 - 2k >= 1`; `z` indices
//!   `<= 2H - 1 = 511`.
//! - `fft`: `n = H = 256`, the loop runs `l = 256, 64, 16` (then the `l == 4`
//!   stage); `q = l / 4`; `p` starts at `2j < 2q <= 2n` and steps `2l` while
//!   `p < 2n`; `p3 = p + 6q = p + 3l/2 < 2n - 2l + 3l/2 = 2n - l/2 < 2n`, so
//!   every read/write is inside `2n = 512` coefficients. `TW` has 504 entries
//!   and is consumed 6 at a time, 84 times over the three stages (`tw` max 503).
//! - `post`: `PERM[k] < 256`, so `2 * PERM[k] + 1 <= 511 < 2H`; `M - 1 - 2k`
//!   and `2k` are inside `M`.
//! - `ola`: `i < H`; `H + i < M`; `H - 1 - i` and `M - 1 - i` are inside their
//!   buffers.
//!
//! Table values are compile-time constants from the format's reference; only
//! the `unsafe` addressing above is audited here.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod decoder;

pub use decoder::{GbaDecoder, HotState};

use uli_lite::format::{H, M};

/// The synthesis tables: cartridge-resident by default (they are read-only
/// constants), or copied to `.iwram` with the `iwram-tables` feature. The
/// reference GBA port copies them; the feature exists so the cost of that
/// choice is measured rather than assumed (ULI2.md, Phase 1b).
#[cfg(not(any(feature = "iwram-tables", feature = "iwram-tw")))]
mod tables {
    pub use uli_lite::tables_data::{PERM, POST, PRE, TW, WIN};
}

// Narrower placement arm: only the FFT's twiddle table in `.iwram`, the rest
// (PRE/POST/WIN/PERM) still in ROM. TW is 504 words = 2,016 B — the table the
// hot butterfly touches on every output — so this isolates the twiddle-read
// cost from the pre/post/OLA tables' cost, and fits `.iwram` where the full
// `iwram-tables` set (8,672 B) does not.
#[cfg(all(not(feature = "iwram-tables"), feature = "iwram-tw"))]
mod tables {
    pub use uli_lite::tables_data::{PERM, POST, PRE, WIN};
    #[unsafe(link_section = ".iwram")]
    pub static TW: [i32; 504] = uli_lite::tables_data::TW;
}

#[cfg(feature = "iwram-tables")]
mod tables {
    #[unsafe(link_section = ".iwram")]
    pub static PERM: [u16; 256] = uli_lite::tables_data::PERM;
    #[unsafe(link_section = ".iwram")]
    pub static PRE: [i32; 512] = uli_lite::tables_data::PRE;
    #[unsafe(link_section = ".iwram")]
    pub static POST: [i32; 512] = uli_lite::tables_data::POST;
    #[unsafe(link_section = ".iwram")]
    pub static TW: [i32; 504] = uli_lite::tables_data::TW;
    #[unsafe(link_section = ".iwram")]
    pub static WIN: [i32; 512] = uli_lite::tables_data::WIN;
}

use tables::{PERM, POST, PRE, TW, WIN};

/// Full 64-bit signed product of two 32-bit values.
///
/// On the GBA build the enclosing kernel is ARM mode, where LLVM lowers this
/// to `SMULL`; on the host it is an ordinary multiply. Bit-identical to the
/// core's `(a as i64) * (b as i64)`.
#[inline(always)]
fn mul32(a: i32, b: i32) -> i64 {
    (a as i64) * (b as i64)
}

#[inline(always)]
fn hi32(v: i64) -> i32 {
    (v >> 32) as i32
}

// Explicit compares rather than `i32::clamp`: LLVM lowers `clamp` to an
// out-of-line generic that the ARM kernels then call through an interworking
// thunk. On this target that thunk resolved into an ARM function and hung the
// kernel; the compare form is also two instructions instead of a call.
#[inline(always)]
fn sat16(v: i32) -> i16 {
    if v > i16::MAX as i32 {
        i16::MAX
    } else if v < i16::MIN as i32 {
        i16::MIN
    } else {
        v as i16
    }
}

#[inline(always)]
fn sat8(v: i32) -> i8 {
    if v > i8::MAX as i32 {
        i8::MAX
    } else if v < i8::MIN as i32 {
        i8::MIN
    } else {
        v as i8
    }
}

/// Pre-twiddle, ARM mode, unchecked addressing.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn pre(x: &[i32; M], z: &mut [i32; 2 * H]) {
    let xp = x.as_ptr();
    let zp = z.as_mut_ptr();
    // SAFETY: see the module-level index bounds; all accesses are inside the
    // two fixed-size arrays by the loop range `k < H`.
    unsafe {
        for k in 0..H {
            let xr = *xp.add(2 * k);
            let xi = *xp.add(M - 1 - 2 * k);
            let wr = PRE[2 * k];
            let wi = PRE[2 * k + 1];
            zp.add(2 * k).write(hi32(
                mul32(xr, wr).wrapping_add(mul32(xi, wi.wrapping_neg())),
            ));
            zp.add(2 * k + 1)
                .write(hi32(mul32(xr, wi).wrapping_add(mul32(xi, wr))));
        }
    }
}

/// 256-point radix-4 DIF FFT, ARM mode, unchecked addressing.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn fft(x: &mut [i32; 2 * H]) {
    let n = H;
    let p0 = x.as_mut_ptr();
    let mut tw = 0usize;
    let mut l = n;
    // SAFETY: see the module-level index bounds. `p`, `p1`, `p2`, `p3` and
    // their `+1` neighbours stay below `2n`; `tw + 5 < TW.len()`.
    unsafe {
        while l >= 4 {
            let q = l >> 2;
            if l == 4 {
                let mut p = 0usize;
                while p < 2 * n {
                    let ar = *p0.add(p);
                    let ai = *p0.add(p + 1);
                    let br = *p0.add(p + 2);
                    let bi = *p0.add(p + 3);
                    let cr = *p0.add(p + 4);
                    let ci = *p0.add(p + 5);
                    let dr = *p0.add(p + 6);
                    let di = *p0.add(p + 7);
                    let t0r = ar.wrapping_add(cr);
                    let t0i = ai.wrapping_add(ci);
                    let t1r = ar.wrapping_sub(cr);
                    let t1i = ai.wrapping_sub(ci);
                    let t2r = br.wrapping_add(dr);
                    let t2i = bi.wrapping_add(di);
                    let t3r = br.wrapping_sub(dr);
                    let t3i = bi.wrapping_sub(di);
                    p0.add(p).write(t0r.wrapping_add(t2r) >> 2);
                    p0.add(p + 1).write(t0i.wrapping_add(t2i) >> 2);
                    p0.add(p + 2).write(t1r.wrapping_add(t3i) >> 2);
                    p0.add(p + 3).write(t1i.wrapping_sub(t3r) >> 2);
                    p0.add(p + 4).write(t0r.wrapping_sub(t2r) >> 2);
                    p0.add(p + 5).write(t0i.wrapping_sub(t2i) >> 2);
                    p0.add(p + 6).write(t1r.wrapping_sub(t3i) >> 2);
                    p0.add(p + 7).write(t1i.wrapping_add(t3r) >> 2);
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
                    let ar = *p0.add(p);
                    let ai = *p0.add(p + 1);
                    let br = *p0.add(p1);
                    let bi = *p0.add(p1 + 1);
                    let cr = *p0.add(p2);
                    let ci = *p0.add(p2 + 1);
                    let dr = *p0.add(p3);
                    let di = *p0.add(p3 + 1);
                    let t0r = ar.wrapping_add(cr);
                    let t0i = ai.wrapping_add(ci);
                    let t1r = ar.wrapping_sub(cr);
                    let t1i = ai.wrapping_sub(ci);
                    let t2r = br.wrapping_add(dr);
                    let t2i = bi.wrapping_add(di);
                    let t3r = br.wrapping_sub(dr);
                    let t3i = bi.wrapping_sub(di);
                    p0.add(p).write(t0r.wrapping_add(t2r) >> 2);
                    p0.add(p + 1).write(t0i.wrapping_add(t2i) >> 2);

                    let yr = t1r.wrapping_add(t3i);
                    let yi = t1i.wrapping_sub(t3r);
                    p0.add(p1).write(hi32(
                        mul32(yr, w1r).wrapping_add(mul32(yi, w1i.wrapping_neg())),
                    ));
                    p0.add(p1 + 1)
                        .write(hi32(mul32(yr, w1i).wrapping_add(mul32(yi, w1r))));

                    let yr = t0r.wrapping_sub(t2r);
                    let yi = t0i.wrapping_sub(t2i);
                    p0.add(p2).write(hi32(
                        mul32(yr, w2r).wrapping_add(mul32(yi, w2i.wrapping_neg())),
                    ));
                    p0.add(p2 + 1)
                        .write(hi32(mul32(yr, w2i).wrapping_add(mul32(yi, w2r))));

                    let yr = t1r.wrapping_sub(t3i);
                    let yi = t1i.wrapping_add(t3r);
                    p0.add(p3).write(hi32(
                        mul32(yr, w3r).wrapping_add(mul32(yi, w3i.wrapping_neg())),
                    ));
                    p0.add(p3 + 1)
                        .write(hi32(mul32(yr, w3i).wrapping_add(mul32(yi, w3r))));

                    p += 2 * l;
                }
            }
            l >>= 2;
        }
    }
}

/// Post-twiddle, ARM mode, unchecked addressing.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn post(z: &[i32; 2 * H], y: &mut [i32; M]) {
    let zp = z.as_ptr();
    let yp = y.as_mut_ptr();
    // SAFETY: see the module-level index bounds.
    unsafe {
        for k in 0..H {
            let p = PERM[k] as usize * 2;
            let zr = *zp.add(p);
            let zi = *zp.add(p + 1);
            let wr = POST[2 * k];
            let wi = POST[2 * k + 1];
            yp.add(2 * k).write(hi32(
                mul32(zr, wr).wrapping_add(mul32(zi, wi.wrapping_neg())),
            ));
            yp.add(M - 1 - 2 * k)
                .write(hi32(mul32(zr, wi).wrapping_add(mul32(zi, wr))).wrapping_neg());
        }
    }
}

/// Window and overlap-add, ARM mode, unchecked addressing.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], mut out16: Option<&mut [i16]>) {
    let yp = y.as_ptr();
    let pp = p.as_mut_ptr();
    let o8 = out8.as_mut_ptr();
    let o16 = out16.as_deref_mut().map(|s| s.as_mut_ptr());
    // SAFETY: see the module-level index bounds.
    unsafe {
        for i in 0..H {
            let a = *yp.add(H + i);
            let b = *pp.add(H - 1 - i);
            let wa = WIN[i];
            let wb = WIN[M - 1 - i];
            let nwa = wa.wrapping_neg();
            let nwb = wb.wrapping_neg();
            let acc1 = mul32(a, wa).wrapping_add(mul32(b, nwb));
            let acc2 = mul32(a, nwb).wrapping_add(mul32(b, nwa));
            o8.add(i)
                .write(sat8(hi32(acc1.wrapping_add(1i64 << 39)) >> 8));
            o8.add(M - 1 - i)
                .write(sat8(hi32(acc2.wrapping_add(1i64 << 39)) >> 8));
            if let Some(o16) = o16 {
                *o16.add(i) = sat16(hi32(acc1.wrapping_add(0x8000_0000)));
                *o16.add(M - 1 - i) = sat16(hi32(acc2.wrapping_add(0x8000_0000)));
            }
            *pp.add(H - 1 - i) = *yp.add(H - 1 - i);
        }
    }
}

/// Window and overlap-add, **int8 only**, format fixed outside the sample loop.
///
/// This is C-3d's output fusion: the C reference ships one kernel per output
/// format (`uli_ola_s8_asm`, `uli_ola_s8w_asm`, `uli_ola_s16_asm`) and selects
/// the format *before* the loop, so no sample pays a per-sample format branch or
/// the arithmetic of the format it is not writing. [`ola`] branches on
/// `Option<&mut [i16]>` every sample and computes both formats; this kernel
/// computes and stores only int8, which is the shipping headphone path.
///
/// Bit-exact with `ola(.., Some(..))`'s int8 half. `out8` must be `M` long (it
/// writes `i` and `M - 1 - i`).
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn ola_s8(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8]) {
    let yp = y.as_ptr();
    let pp = p.as_mut_ptr();
    let o8 = out8.as_mut_ptr();
    // SAFETY: `i < H`; `H + i < M`; `M - 1 - i` is inside `M`; the caller
    // guarantees `out8` is `M` long.
    unsafe {
        for i in 0..H {
            let a = *yp.add(H + i);
            let b = *pp.add(H - 1 - i);
            let wa = WIN[i];
            let wb = WIN[M - 1 - i];
            let nwa = wa.wrapping_neg();
            let nwb = wb.wrapping_neg();
            let acc1 = mul32(a, wa).wrapping_add(mul32(b, nwb));
            let acc2 = mul32(a, nwb).wrapping_add(mul32(b, nwa));
            o8.add(i)
                .write(sat8(hi32(acc1.wrapping_add(1i64 << 39)) >> 8));
            o8.add(M - 1 - i)
                .write(sat8(hi32(acc2.wrapping_add(1i64 << 39)) >> 8));
            *pp.add(H - 1 - i) = *yp.add(H - 1 - i);
        }
    }
}

/// Window and overlap-add, **int16 only**, format fixed outside the sample loop.
///
/// The pre-DAC int16 profile's fused kernel (C's `uli_ola_s16_asm` shape).
/// Bit-exact with `ola(.., Some(..))`'s int16 half; `out16` must be `M` long.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
pub fn ola_s16(y: &[i32; M], p: &mut [i32; H], out16: &mut [i16]) {
    let yp = y.as_ptr();
    let pp = p.as_mut_ptr();
    let o16 = out16.as_mut_ptr();
    // SAFETY: as for [`ola_s8`]; `out16` must be `M` long.
    unsafe {
        for i in 0..H {
            let a = *yp.add(H + i);
            let b = *pp.add(H - 1 - i);
            let wa = WIN[i];
            let wb = WIN[M - 1 - i];
            let nwa = wa.wrapping_neg();
            let nwb = wb.wrapping_neg();
            let acc1 = mul32(a, wa).wrapping_add(mul32(b, nwb));
            let acc2 = mul32(a, nwb).wrapping_add(mul32(b, nwa));
            *o16.add(i) = sat16(hi32(acc1.wrapping_add(0x8000_0000)));
            *o16.add(M - 1 - i) = sat16(hi32(acc2.wrapping_add(0x8000_0000)));
            *pp.add(H - 1 - i) = *yp.add(H - 1 - i);
        }
    }
}

/// The OLA the product decoder should call. With no int16 buffer this is the
/// **fused int8-only kernel** (no per-sample format branch, no int16 arithmetic);
/// with one it is the dual-output [`ola`] the int16 profile and the bench pins
/// need. Same shape as [`fft_kernel`]: the `Option` is resolved once here rather
/// than per sample.
#[inline]
pub fn ola_kernel(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], out16: Option<&mut [i16]>) {
    match out16 {
        Some(o16) => crate::ola(y, p, out8, Some(o16)),
        None => crate::ola_s8(y, p, out8),
    }
}

/// The ARM-mode kernel set, plugged into [`uli_lite::Decoder`] through the
/// `Kernels` seam.
///
/// This is the product's synthesis path: `Decoder::<ArmKernels>::with_kernels`
/// keeps the whole frame decode (bitstream, dequant, M/S, frame state) in the
/// portable core while running the four synthesis kernels as the ARM-mode,
/// unchecked, `.iwram`-resident functions above. PR B moves the remaining hot
/// path here; until then this type is what makes a **full-frame** product
/// measurement possible without duplicating `decode_frame`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArmKernels;

impl uli_lite::Kernels for ArmKernels {
    #[inline]
    fn pre(x: &[i32; M], z: &mut [i32; 2 * H]) {
        crate::pre(x, z);
    }

    #[inline]
    fn fft(z: &mut [i32; 2 * H]) {
        // With `asm-fft`, use the best hand-written kernel (v3). The Rust
        // ARM-mode `fft` stays the default so every arm keeps its control in
        // the same image; v1 and v2 remain measurable comparison arms.
        crate::fft_kernel(z);
    }

    #[inline]
    fn post(z: &[i32; 2 * H], y: &mut [i32; M]) {
        crate::post(z, y);
    }

    #[inline]
    fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], out16: Option<&mut [i16]>) {
        crate::ola_kernel(y, p, out8, out16);
    }
}

/// Hand-written ARM asm for the FFT (PR C-1). Off unless the `asm-fft`
/// feature is on; the Rust ARM-mode [`fft`] above stays the default so every
/// existing arm keeps its control in the same image.
///
/// The kernel is bit-exact with [`fft`] (which is bit-exact with the core),
/// so it is a drop-in replacement. `examples/uli_bench`'s on-target kernel twin
/// compares the two on the ARM7TDMI, which is what licenses a cycle number
/// measured through this path.
#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
mod fft_asm {
    core::arch::global_asm!(
        include_str!("../asm/fft_arm.s"),
        "\n",
        include_str!("../asm/fft_arm_v2.s"),
        "\n",
        include_str!("../asm/fft_arm_v3.s"),
        "\n",
        include_str!("../asm/fft_arm_v4.s")
    );

    // `(x, tw)` — the buffer (512 words) and the twiddle table, passed by
    // pointer so the asm never depends on a Rust symbol's mangled name and so
    // `iwram-tables` moves TW for free (the pointer follows the active table).
    unsafe extern "C" {
        fn uli_fft_arm(x: *mut i32, tw: *const i32);
        fn uli_fft_arm_v2(x: *mut i32, tw: *const i32);
        fn uli_fft_arm_v3(x: *mut i32, tw: *const i32);
        fn uli_fft_arm_v4(x: *mut i32, tw: *const i32);
    }

    /// Call the asm FFT (v1) on `x` (`[i32; 2*H]`). SAFETY: `x` points at 512
    /// i32 and `tables::TW` at the 504-entry twiddle table.
    #[inline]
    pub fn fft(x: &mut [i32; 2 * uli_lite::format::H]) {
        // SAFETY: `x` is a `[i32; 2*H]` buffer; the asm writes exactly 2*H
        // words at `x` and reads TW[0..504] at `TW`.
        unsafe { uli_fft_arm(x.as_mut_ptr(), crate::TW.as_ptr()) };
    }

    /// Call the memory-traffic-reduction FFT (v2) on `x`. Same contract as
    /// [`fft`].
    #[inline]
    pub fn fft_v2(x: &mut [i32; 2 * uli_lite::format::H]) {
        // SAFETY: as for `fft`.
        unsafe { uli_fft_arm_v2(x.as_mut_ptr(), crate::TW.as_ptr()) };
    }

    /// Call the block-transfer FFT (v3) on `x`. Same contract as [`fft`].
    #[inline]
    pub fn fft_v3(x: &mut [i32; 2 * uli_lite::format::H]) {
        // SAFETY: as for `fft`.
        unsafe { uli_fft_arm_v3(x.as_mut_ptr(), crate::TW.as_ptr()) };
    }

    /// Call the block-transfer + trivial-butterfly FFT (v4) on `x`. Same
    /// contract as [`fft`].
    #[inline]
    pub fn fft_v4(x: &mut [i32; 2 * uli_lite::format::H]) {
        // SAFETY: as for `fft`.
        unsafe { uli_fft_arm_v4(x.as_mut_ptr(), crate::TW.as_ptr()) };
    }

    /// Measurement-only: call v4 with an explicit twiddle-table pointer so a
    /// single image can time the same kernel against the cartridge-resident
    /// `TW` and an `.iwram` copy of it. SAFETY: `tw` must point at the 504-word
    /// twiddle table.
    #[inline]
    pub fn fft_v4_tw(x: &mut [i32; 2 * uli_lite::format::H], tw: *const i32) {
        // SAFETY: caller promises `tw` is the 504-entry table.
        unsafe { uli_fft_arm_v4(x.as_mut_ptr(), tw) };
    }
}

#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
pub use fft_asm::fft as fft_asm_call;
#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
pub use fft_asm::fft_v2 as fft_asm_v2_call;
#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
pub use fft_asm::fft_v3 as fft_asm_v3_call;
#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
pub use fft_asm::fft_v4 as fft_asm_v4_call;
#[cfg(all(target_arch = "arm", feature = "asm-fft"))]
pub use fft_asm::fft_v4_tw as fft_asm_v4_tw_call;

/// The FFT the product decoder (and [`ArmKernels`]) should call: the best
/// hand-written kernel (v3) when the `asm-fft` feature is on, otherwise the
/// Rust ARM-mode [`fft`]. Centralising the `#[cfg]` here is what lets the
/// product hot path (`decoder.rs`) pick up the asm kernel without repeating the
/// gate at every call site.
#[inline]
pub fn fft_kernel(z: &mut [i32; 2 * H]) {
    #[cfg(all(target_arch = "arm", feature = "asm-fft"))]
    {
        crate::fft_asm_v4_call(z);
    }
    #[cfg(not(all(target_arch = "arm", feature = "asm-fft")))]
    {
        crate::fft(z);
    }
}
