//! **PR B — the `rev8` refill merge** (the optional step, landing at its
//! consumer's PR per PR A's scope cut).
//!
//! The ratified merge shape (FLAC.md design-of-record, ratified item 3:
//! "`rev8` refill + `ORR ..., LSL #24` merge — the barrel shifter places
//! the byte in the merge cycle"). This kernel witnesses the SHAPE only:
//! the COST of the reversal (one extra `ldrb` per 8 bits, landing in the
//! refill path shared with the remainder reads) is explicitly
//! left-to-measure item 4 — the probe's question, never this crate's.
//!
//! Dual witness (crate docs): [`refill_merge_twin`] (host,
//! `tests/rm_twin.rs`) and [`refill_merge_asm`] swept by the kernel-check
//! ROM over the SAME committed table
//! (`RM_VECTORS`). Nothing is integrated into the decode path and nothing
//! is timed.

use crate::tables::REV8;

/// Naive expectation for one merge step: `window | (rev8(byte) << 24)`
/// with `rev8` the bit-loop oracle (independent of the committed table's
/// shape). Caller contract pinned by the committed table: the window's
/// top byte is zero (the merge ORs; it never masks).
#[must_use]
pub fn oracle_refill_merge(window: u32, byte: u8) -> u32 {
    window | (crate::tables::oracle_rev8(byte) as u32) << 24
}

/// Rust twin of the [`refill_merge_asm`] kernel — the same single
/// merge: table lookup, one shifted OR.
#[must_use]
pub fn refill_merge_twin(window: u32, byte: u8) -> u32 {
    window | (u32::from(REV8[usize::from(byte)]) << 24)
}

/// The ARM-mode assembly kernel (single assembled unit — see
/// [`crate::asm`]). Target-only; host builds have no ARM executor (crate
/// docs).
#[cfg(target_arch = "arm")]
#[must_use]
#[allow(unsafe_code)] // the FFI boundary to the asm kernel; SAFETY below
pub fn refill_merge_asm(window: u32, byte: u8) -> u32 {
    unsafe extern "C" {
        fn arm_refill_merge(window: u32, byte: u32, rev8: *const u8) -> u32;
    }
    // SAFETY: the kernel reads only through `rev8` (the in-blob 256-entry
    // table, indexed by its own defensive 0xff mask) and writes no
    // memory; r0–r2 match the documented register args; `BX lr`
    // interworks back to the Thumb caller.
    unsafe { arm_refill_merge(window, u32::from(byte), REV8.as_ptr()) }
}
