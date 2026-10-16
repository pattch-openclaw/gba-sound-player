//! **PR A — the `rice_unmap` fold** (§9.2.7.2 folded-residual sign map).
//!
//! The independent leaf of FLAC.md's "E10 unit-test kernel track — design
//! of record (2026-10-16)": the sample-step's final fold from the Rice
//! folded residual back to the signed sample. The *measured* identity is
//! `(n >> 1) ^ (-(n & 1))` for the corrected §9.2.7.2 convention — even →
//! `n >> 1`, odd → `!(n >> 1)` (the sign-flip correction, FLAC.md step-3a
//! entry; the RFC's worked example: folded 38 → +19). Four instructions,
//! no table, no branch, no flag writes — zero design decisions, exactly
//! what the split promised for this PR.
//!
//! Dual witness (crate docs) over one committed table
//! ([`unmap_vectors::UNMAP_VECTORS`](crate::unmap_vectors)): the same
//! algorithm lives as [`rice_unmap_twin`] and as the ARM-mode assembly
//! [`rice_unmap_asm`]; `tests/unmap_twin.rs` additionally anchors the sign
//! convention against the **encoder-side** fold (`x ≥ 0 → 2x`,
//! `x < 0 → −2x − 1`) so no shared misreading of §9.2.7.2 can survive the
//! composition. Nothing here is integrated into the decode path and
//! nothing is timed; the `.include` sharing of the fold between the
//! composite's two call sites (decision 6) arrives with PR B's composite.

/// `rice_unmap(n)` — the corrected §9.2.7.2 unmap **as a definition**:
/// even → `n >> 1`, odd → `!(n >> 1)`, reinterpreted as `i32`.
///
/// Deliberately the spec's branch form — independent of the SWAR identity
/// the twin and the assembly use — so the committed table's expectations
/// cannot share a misreading with either implementation (witness rule).
/// The fold is total: `0xFFFF_FFFF` maps to `i32::MIN`, the value
/// §9.2.7.3 forbids in a stream; *rejecting* it stays upstream in
/// `decode_rice_partition` (flac-lite's 3a seam, pinned there).
#[must_use]
pub fn oracle_rice_unmap(n: u32) -> i32 {
    if n & 1 == 0 {
        (n >> 1) as i32
    } else {
        (!(n >> 1)) as i32
    }
}

/// Rust twin of the [`rice_unmap_asm`] kernel — the same four steps in the
/// same order: parity isolate (`AND`), negate to an all-ones/all-zeros
/// mask (`RSB`), strip the folded LSB (`LSR`), apply the mask (`EOR`).
/// Witnessed against [`oracle_rice_unmap`], the encoder-fold round-trip,
/// and the committed vectors on the host.
#[must_use]
pub fn rice_unmap_twin(n: u32) -> i32 {
    let mask = (n & 1).wrapping_neg(); // -(n & 1): 0 (even) or 0xFFFF_FFFF (odd)
    ((n >> 1) ^ mask) as i32
}

/// The assembly body lives in a module so the crate's `deny(unsafe_code)`
/// gets exactly one reviewed exception per asm kernel (edition-2024
/// `global_asm!` is an unsafe op and a macro-item `#[allow]` is ignored —
/// the module attribute is what binds; see `ctz32`'s same-shape note).
/// The exception covers only the kernel contract documented below (r0–r1,
/// no flag writes, no memory, `BX lr` interwork return); both witnesses —
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
    .global arm_rice_unmap
    .type arm_rice_unmap, %function
@ --------------------------------------------------------------------------
@ PR A rice_unmap fold — ARM mode (mode-ness witnessed offline from the
@ built ELF: `objdump -d` shows 4-byte ARM encodings; a passing sweep alone
@ cannot tell the two modes apart).
@
@ r0 = n (the folded residual value). Returns the unmapped i32 in r0.
@ Clobbers r1 only — and writes NO flags (no S bit anywhere): unlike C1,
@ this kernel is invisible to the condition-code state around it, which is
@ what lets the probe's composite keep live flags across the fold.
@ Straight-line: AND / RSB / LSR / EOR — no branch, no table, no memory.
@ Interwork-safe return (`BX lr`).
@ --------------------------------------------------------------------------
arm_rice_unmap:
    and     r1, r0, #1          @ r1 = n & 1 (parity)
    rsb     r1, r1, #0          @ r1 = -(n & 1): 0 or 0xFFFFFFFF mask
    lsr     r0, r0, #1          @ r0 = n >> 1 (strip the folded LSB)
    eor     r0, r0, r1          @ even: unchanged; odd: bitwise-not
    bx      lr
"#
    );
}

/// The ARM-mode assembly kernel, called through the C ABI (the argument
/// lands in r0 exactly as the kernel documents). Target-only: host builds
/// have no ARM executor, which is precisely why the twin + committed table
/// witness exists (crate docs).
#[cfg(target_arch = "arm")]
#[must_use]
#[allow(unsafe_code)] // the FFI boundary to the asm kernel; SAFETY below
pub fn rice_unmap_asm(n: u32) -> i32 {
    unsafe extern "C" {
        fn arm_rice_unmap(n: u32) -> i32;
    }
    // SAFETY: the kernel reads and writes only r0/r1 (the EABI argument
    // registers; the single u32 argument lands in r0), writes no memory,
    // touches no flags, and the `BX lr` return interworks back to the
    // Thumb caller.
    unsafe { arm_rice_unmap(n) }
}
