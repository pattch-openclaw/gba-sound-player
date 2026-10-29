//! **PR B — C2 word-wide continuation skip** (the standalone kernel).
//!
//! The only kernel of the split with real design content (FLAC.md "E10
//! unit-test kernel track — design of record 2026-10-16"): counting a
//! unary run past 8 bits is where the shipped Thumb loop pays per-byte,
//! because Thumb v4's comparison immediates force the walk. ARM tests an
//! entire accumulated word at once — an empty word adds 32 to the count
//! for ~2 cycles, making continuation O(q/32). The design decisions this
//! kernel carries: standalone (decision 1 — the terminating word's tail
//! uses the ctz8 byte table, deliberately NOT C1's De Bruijn shape); it
//! answers the TOTAL run from bit 0 of the slice, the caller never adds
//! (decision 2); unterminated runs report status with no state save
//! (decision 3); the budget is the input slice length, never a hard-coded
//! cap (decision 4).
//!
//! Dual witness (crate docs): [`word_skip_twin`] mirrors the assembly
//! step-for-step (host, `tests/ws_twin.rs`); [`word_skip_asm`] is swept by
//! the kernel-check ROM over the SAME committed table
//! ([`ws_vectors::WS_VECTORS`](crate::ws_vectors)). Nothing is integrated
//! into the decode path and nothing is timed.

use crate::tables::CTZ8;

/// The naive bit-walk oracle: first set bit over the concatenated words
/// from bit 0 of word 0, within `budget`. Deliberately independent of both
/// the twin's whole-word shape and the ctz8 table — three formulations
/// that cannot share a misreading (witness rule).
///
/// Returns `(q, words_consumed, status)`: the TOTAL run from bit 0
/// (decision 2 — never a "+32" the caller adds), the words the scan
/// consumed, and the status (1 = terminated, 0 = the run reached the end
/// of the budgeted slice unterminated — decision 3: status data, no
/// state save).
#[must_use]
pub fn oracle_word_skip(words: &[u32; 4], budget: u32) -> (u32, u32, u32) {
    for w in 0..budget.min(4) {
        for b in 0..32 {
            if (words[w as usize] >> b) & 1 == 1 {
                return (32 * w + b, w + 1, 1);
            }
        }
    }
    (32 * budget, budget, 0)
}

/// Rust twin of the [`word_skip_asm`] kernel — the same steps in the same
/// order: whole-word zero test (+32 each, the O(q/32) loop), then the
/// terminating word's tail counted by the committed ctz8 byte walk (≤ 4
/// lookups). Witnessed against [`oracle_word_skip`] and the committed
/// vectors on the host.
#[must_use]
pub fn word_skip_twin(words: &[u32; 4], budget: u32) -> (u32, u32, u32) {
    let budget0 = budget;
    let mut q = 0u32;
    let mut i = 0u32;
    let term;
    loop {
        if i >= budget0 {
            // budget exhausted before a terminating one (decision 4: the
            // answer is 32 * consumed — the walk's answer, never a cap
            // artifact; q == 32 * budget0 here means every word was zero)
            return (q, budget0, 0);
        }
        let w = words[i as usize];
        i += 1;
        if w != 0 {
            term = w;
            break;
        }
        q += 32;
    }
    // terminating word: ctz8 byte walk — 8 means "run past the byte"
    let bytes = term.to_le_bytes();
    let mut q = q;
    let mut k = 0usize;
    loop {
        let c = u32::from(CTZ8[usize::from(bytes[k])]);
        if c < 8 {
            q += c;
            break;
        }
        q += 8;
        k += 1;
    }
    (q, i, 1)
}

/// The ARM-mode assembly kernel (single assembled unit — see
/// [`crate::asm`]). Target-only; host builds have no ARM executor, which
/// is precisely why the twin + committed table witness exists (crate
/// docs).
#[cfg(target_arch = "arm")]
#[must_use]
#[allow(unsafe_code)] // the FFI boundary to the asm kernel; SAFETY below
pub fn word_skip_asm(words: &[u32; 4], budget: u32) -> (u32, u32, u32) {
    unsafe extern "C" {
        fn arm_word_skip(out: *mut u32, words: *const u32, budget: u32, ctz8: *const u8);
    }
    let mut out = [0u32; 3];
    // SAFETY: the kernel writes exactly the 3-word output struct through
    // `out` and reads only through `words` (bounded by `budget`, which the
    // caller keeps ≤ the slice length per the kernel contract) and the
    // in-blob CTZ8 table; r0–r3 match the kernel's documented register
    // args; the interwork return comes back through the popped pc.
    unsafe {
        arm_word_skip(out.as_mut_ptr(), words.as_ptr(), budget, CTZ8.as_ptr());
    }
    (out[0], out[1], out[2])
}
