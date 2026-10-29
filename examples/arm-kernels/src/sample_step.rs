//! **PR B — the composite sample-step** (the comprehensive demonstration).
//!
//! The kernel that owns the hybrid's tier dispatch (design of record,
//! decision 2 — dispatch lives ONLY here): tier 1 the committed `ctz8`
//! byte table (q ≤ 7), C1's De Bruijn count for 8 ≤ q ≤ 31 (flat), and
//! [`crate::word_skip`] beyond 31 (the handoff signal is `w == 0`, never
//! an answer value; C2's answer is the TOTAL run from bit 0 — the caller
//! never adds). It computes the remainder EXACTLY as decision 7 pins it —
//! `(terminating_word >> (q+1)) & ((1<<p) - 1)` over the pre-assembled
//! bit-reversed window, ARMv4's defined shift-by-32 carrying the q = 31
//! boundary — the drain COUNT q+1+p (bit retirement is the probe's
//! reader), and the fold via the shared `fold_body.s` source (decision 6):
//! the residual is §9.2.7's unmap of the coded number `(q << p) | rem`,
//! the composition `decode_residual` performs.
//!
//! Deliberately NOT here: any rev8 reversal or w_{t+1} straddle merge.
//! Stitching the reversed-window domain to spec-order streams is the
//! probe's vendored seam (the design's left-to-measure items). The
//! committed table carries pinned q = 31 vectors whose remainder bits
//! live in words[1] with expectation `rem = 0` — the sentinel that
//! convicts a merge-or-reversal reading of this kernel.
//!
//! Dual witness (crate docs): [`sample_step_twin`] mirrors the assembly
//! step-for-step (host, `tests/ss_twin.rs`); [`sample_step_asm`] is swept
//! by the kernel-check ROM over the SAME committed table
//! (`SS_VECTORS`). Nothing is integrated into the decode path and nothing
//! is timed.

// BRUIJN_INDEX reaches the asm wrapper only; the twin counts through
// ctz32_twin (the same committed table behind it).
#[cfg(target_arch = "arm")]
use crate::ctz32::BRUIJN_INDEX;
use crate::ctz32::ctz32_twin;
use crate::tables::CTZ8;
use crate::word_skip::word_skip_twin;

/// The naive bit-walk oracle for one sample-step. Independent of the
/// twin's tier shapes: a first-set-bit walk for q, a bit-by-bit remainder
/// sum (bits at word positions ≥ 32 contribute 0 — decision 7's pinned
/// extract), and the §9.2.7.2 unmap as the spec's BRANCH form applied to
/// the coded number `(q << p) | remainder`.
///
/// Returns `(q, remainder, residual, drain_bits, words_consumed, status)`
/// — status 0 = the budgeted slice ends before a terminating 1
/// (decision 3: status data, no state save; the other fields pinned 0).
#[must_use]
pub fn oracle_sample_step(words: &[u32; 4], budget: u32, p: u32) -> (u32, u32, i32, u32, u32, u32) {
    // q: first set bit from bit 0 of word 0, within budget
    let mut q = None;
    'scan: for w in 0..budget.min(4) {
        for b in 0..32 {
            if (words[w as usize] >> b) & 1 == 1 {
                q = Some((32 * w + b, w + 1));
                break 'scan;
            }
        }
    }
    let (q, _scan_consumed) = match q {
        Some(v) => v,
        None => return (32 * budget, 0, 0, 0, budget, 0),
    };
    let t = (q / 32) as usize;
    let s = q % 32;
    // remainder: bits q+1 .. q+p of the WINDOW, terminating word only
    let mut rem = 0u32;
    for i in 0..p {
        let pos = s + 1 + i;
        if pos < 32 && (words[t] >> pos) & 1 == 1 {
            rem |= 1 << i;
        }
    }
    let coded = (q << p) | rem;
    // §9.2.7.2 unmap, spec branch form, 32-bit result as i32
    let residual = if coded & 1 == 0 {
        (coded >> 1) as i32
    } else {
        (!(coded >> 1) as u32) as i32
    };
    let drain = q + 1 + p;
    let consumed = drain.div_ceil(32);
    (q, rem, residual, drain, consumed, 1)
}

/// Rust twin of the [`sample_step_asm`] kernel — the same steps in the
/// same order: tier-1 `ctz8` byte lookup; `w == 0` handoff to the C2
/// twin; C1 De Bruijn (via the committed index table — the same shape the
/// asm's `MUL` takes); the literal extract with the shift-by-32 leg; the
/// mask; drain/consumed; the coded number and the SWAR fold. Witnessed
/// against [`oracle_sample_step`] and the committed vectors on the host.
#[must_use]
pub fn sample_step_twin(words: &[u32; 4], budget: u32, p: u32) -> (u32, u32, i32, u32, u32, u32) {
    let w0 = words[0];
    // ---- tier 1: ctz8[byte0]; 8 means "run past the byte" ----
    let c = u32::from(CTZ8[(w0 & 0xFF) as usize]);
    let q = if c < 8 {
        c
    } else if w0 == 0 {
        // ---- handoff to C2 (decision 2: total from bit 0, never added) ----
        let (q, _consumed, status) = word_skip_twin(words, budget);
        if status == 0 {
            return (q, 0, 0, 0, budget, 0);
        }
        q
    } else {
        // ---- C1 tier: De Bruijn on the lowbit (the asm's MUL shape) ----
        // The asm counts from the lowbit; reuse the committed C1 twin,
        // which is the same De Bruijn mechanism over the same table.
        ctz32_twin(w0)
    };
    // ---- remainder (decision 7's literal extract) ----
    let t = (q / 32) as usize;
    let s = q % 32;
    let mask = if p == 0 { 0u32 } else { (1u32 << p) - 1 };
    // s + 1 == 32 (q ≡ 31): ARMv4's defined shift-by-32 yields 0; Rust's
    // `>> 32` would panic in debug builds, so the leg is taken explicitly
    // (the same boundary the asm's register shift settles for free — the
    // committed table pins it).
    let rem = if s + 1 >= 32 {
        0
    } else {
        (words[t] >> (s + 1)) & mask
    };
    // ---- drain / consumed ----
    let drain = q + 1 + p;
    let consumed = drain.div_ceil(32);
    // ---- coded + fold (shared fold_body's SWAR identity) ----
    let coded = (q << p) | rem;
    let residual = (((coded >> 1) as u32) ^ ((coded & 1).wrapping_neg())) as i32;
    (q, rem, residual, drain, consumed, 1)
}

/// The ARM-mode assembly kernel (single assembled unit — see
/// [`crate::asm`]). ABI per the kernel header: r0–r3 register args +
/// two table pointers as AAPCS stack args (the extern declaration passes
/// all six; Rust lowers 5–6 to the stack exactly where the kernel reads
/// them). Target-only: host builds have no ARM executor, which is
/// precisely why the twin + committed table witness exists (crate docs).
#[cfg(target_arch = "arm")]
#[must_use]
#[allow(unsafe_code)] // the FFI boundary to the asm kernel; SAFETY below
pub fn sample_step_asm(words: &[u32; 4], budget: u32, p: u32) -> (u32, u32, i32, u32, u32, u32) {
    unsafe extern "C" {
        fn arm_sample_step(
            out: *mut u32,
            words: *const u32,
            budget: u32,
            p: u32,
            ctz8: *const u8,
            bruijn: *const u8,
        );
    }
    let mut out = [0u32; 6];
    // SAFETY: the kernel writes exactly the 6-word output struct through
    // `out`; reads only through `words` (bounded by `budget` ≤ the slice
    // length per the kernel contract) and the two in-blob tables; the
    // interwork return comes back through the popped pc.
    unsafe {
        arm_sample_step(
            out.as_mut_ptr(),
            words.as_ptr(),
            budget,
            p,
            CTZ8.as_ptr(),
            BRUIJN_INDEX.as_ptr(),
        );
    }
    (out[0], out[1], out[2] as i32, out[3], out[4], out[5])
}
