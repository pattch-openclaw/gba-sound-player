//! **C1 — De Bruijn multiply-index count** (the CLZ-as-a-MUL shape).
//!
//! The hybrid's flat 8 ≤ q ≤ 31 tier in FLAC.md's "E10 algorithm plan of
//! action (2026-10-14)": converts the absent ARMv4T `CLZ` into the
//! documented single-cycle hardware `MUL`. In the probe's bit-reversed
//! window the unary run reads as *trailing zeros*, so the kernel answers
//! `ctz(w)`; the sentinel `32` for `w == 0` is the "run past the window"
//! signal.
//!
//! Dual witness (crate docs): the same algorithm lives as
//! [`ctz32_twin`] (host-tested by `tests/host_twin.rs` against a naive
//! bit-walk oracle and the committed table) and as the ARM-mode assembly
//! [`ctz32_asm`] (swept by the kernel-check ROM on mGBA's ARM7TDMI against
//! the SAME table). Nothing here is integrated into the decode path and
//! nothing is timed.

/// The De Bruijn multiplier. Not rotatable as an ARMv4 immediate
/// (FLAC.md's measured substrate fact), which is exactly why the kernel
/// takes it from a register staged by the caller — the probe stages it
/// once outside the sample loop; this isolated kernel receives it per call.
pub const BRUIJN_K: u32 = 0x077CB531;

/// 32-entry De Bruijn index table: `BRUIJN_INDEX[(((1 << b) * BRUIJN_K) >>
/// 27) as usize] == b` for every `b` in `0..32`, all entries distinct
/// (witnessed exhaustively by `tests/host_twin.rs`).
///
/// Deliberately NOT pinned to `.iwram`: IWRAM pinning is a probe-stage
/// decision with a budget witness (FLAC.md's E10 probe shape) — this crate
/// measures nothing, so it pins nothing.
pub const BRUIJN_INDEX: [u8; 32] = [
    0, 1, 28, 2, 29, 14, 24, 3, //
    30, 22, 20, 15, 25, 17, 4, 8, //
    31, 27, 13, 23, 21, 19, 16, 7, //
    26, 12, 18, 6, 11, 5, 10, 9, //
];

/// `ctz(w)` — the count of trailing zeros (the low-end run in the probe's
/// bit-reversed window), with the sentinel `32` for `w == 0`.
///
/// Deliberately a naive bit walk: independent of `u32::trailing_zeros` and
/// of the De Bruijn identity, so the committed table's expectations cannot
/// share a misreading with either implementation (witness rule).
#[must_use]
pub fn oracle_ctz(w: u32) -> u32 {
    let mut q = 0u32;
    while q < 32 && (w >> q as usize) & 1 == 0 {
        q += 1;
    }
    q
}

/// Rust twin of the [`ctz32_asm`] kernel — the same steps in the same order:
/// isolate the lowest set bit, multiply by the De Bruijn constant (wrapping
/// low word, matching `MUL`), index the table. Witnessed against
/// [`oracle_ctz`] and the committed vectors on the host.
#[must_use]
pub fn ctz32_twin(w: u32) -> u32 {
    let low = w & w.wrapping_neg(); // w & -w; 0 iff w == 0
    if low == 0 {
        return 32;
    }
    let idx = low.wrapping_mul(BRUIJN_K) >> 27;
    u32::from(BRUIJN_INDEX[idx as usize])
}

/// The assembly body lives in a module so the crate's `deny(unsafe_code)`
/// gets exactly one reviewed exception: in edition 2024 `global_asm!` is an
/// unsafe op (the lint fires on the macro itself, and an `#[allow]` on a
/// macro-item invocation is ignored — the module attribute is what binds).
/// The exception covers only the kernel contract documented above (r0–r3,
/// flags, no memory writes, `BX lr` interwork return); both witnesses —
/// host twin + on-target sweep — judge exactly these bytes.
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
mod asm {
    core::arch::global_asm!(
        r#"
    .syntax unified
    .arm
    .text
    .code 32
    .global arm_ctz32
    .type arm_ctz32, %function
@ --------------------------------------------------------------------------
@ C1 De Bruijn ctz — ARM mode (the mode-ness is witnessed offline from the
@ built ELF: `objdump -d` shows 4-byte ARM encodings, never a Thumb
@ transcription; a passing sweep alone cannot tell the two modes apart).
@
@ r0 = w (window word); r1 = De Bruijn multiplier K; r2 = &BRUIJN_INDEX[0].
@ Returns ctz(w) in r0, 32 when w == 0. Clobbers r0, r3 and flags — all
@ caller-saved under the ARM EABI; no memory written, no other register
@ touched. Interwork-safe return (`BX lr` switches on bit 0 of the link
@ address, so Thumb Rust callers return in Thumb).
@ --------------------------------------------------------------------------
arm_ctz32:
    rsbs    r3, r0, #0
    ands    r3, r0, r3          @ r3 = w & -w (lowest set bit); Z = (w == 0)
    moveq   r0, #32             @ sentinel: run past the window
    bxeq    lr
    mul     r0, r1, r3          @ MUL with Rd/Rm/Rs ALL distinct: ARMv4
    lsr     r0, r0, #27         @   leaves Rd == Rm UNPREDICTABLE and clang
    ldrb    r0, [r2, r0]        @   assembles the naive form silently; w is
    bx      lr                  @   dead in r0 here, so reuse it (and the
                                @   base+register LDRB is ARM-only addressing)
"#
    );
}

/// The ARM-mode assembly kernel, called through the C ABI (args land in
/// r0–r2 exactly as the kernel documents). Target-only: host builds have
/// no ARM executor, which is precisely why the twin + committed table
/// witness exists (crate docs).
#[cfg(target_arch = "arm")]
#[must_use]
#[allow(unsafe_code)] // the FFI boundary to the asm kernel; SAFETY below
pub fn ctz32_asm(w: u32) -> u32 {
    unsafe extern "C" {
        fn arm_ctz32(w: u32, k: u32, table: *const u8) -> u32;
    }
    // SAFETY: the kernel reads only through `table` (a pointer to the
    // in-blob 32-entry BRUIJN_INDEX, indexed by a value already reduced to
    // 0..31 by the >>27) and writes no memory; r0–r3 are the EABI
    // argument registers matching the kernel's documented inputs; the
    // `BX lr` return interworks back to the Thumb caller.
    unsafe { arm_ctz32(w, BRUIJN_K, BRUIJN_INDEX.as_ptr()) }
}
