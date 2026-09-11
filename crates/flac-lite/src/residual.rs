//! Residual coding: partitioned Rice / Rice2 / escape-record residual.
//!
//! SCAFFOLD STATE (2026-09-10): [`rice_unmap`] is implemented (Phase 1 step
//! 3a — the folded-residual sign map, RFC 9639 §9.2.7.2). `decode_residual`
//! and `decode_rice_partition` remain `todo!()` (steps 3b–3c).
//!
//! Every subframe (FIXED or LPC) ends with a residual. This is the hottest code
//! path in the decoder — it decodes the bulk of the bits — so it is where the
//! perf spike's cycle counts will concentrate. Implementation notes:
//!
//! - **Rice decoding is unary + folded magnitude, branch-free where
//!   possible:** count zeros until the terminating one (the quotient), read the
//!   `order`-bit remainder, combine `(quotient << order) | remainder` into the
//!   folded value, then unfold it with [`rice_unmap`]. (The module previously
//!   sketched a `negative ? !sample : sample` sign-bit mapping here — that is
//!   not FLAC's residual mapping; see [`rice_unmap`]'s correction note.)
//! - **No division.** Rice2's escaped-partition mapping and the residual's
//!   sample-size determination are shift/mask only.
//! - The sample size for the residual is the frame header's assignment minus
//!   the predictor order, with an explicit "unknown" (`0b111`) escape that must
//!   be derived from the partition's max coded value.

use crate::bits::BitReader;

/// Rice parameter coding method, from the 2-bit residual header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiceMethod {
    /// 4-bit Rice parameter.
    Rice4Bit,
    /// 5-bit Rice parameter.
    Rice5Bit,
}

/// One partition's header, as read from the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionHeader {
    /// Rice order (0..=14 for Rice4Bit/Rice2; 0..=4 raw escape marker is 0b1111).
    pub order: u8,
    /// Escape record: samples in this partition are unencoded at `raw_bits`.
    pub is_escape: bool,
    /// Sample bits for an escaped partition (4-bit field, `0` means 0 bits).
    pub raw_bits: u8,
    /// Samples in this partition.
    pub sample_count: u16,
}

/// Decode a subframe residual into `out` (length must equal the subframe's
/// block size), returning the samples as *residuals* (the caller in
/// [`crate::subframe`] integrates them through the predictor).
///
/// `expected_samples` is the block size; `sample_bits` is the frame's assigned
/// sample size minus predictor order (or `None` when the stream signals
/// "unknown", requiring derivation).
pub fn decode_residual(
    reader: &mut BitReader<'_>,
    method: RiceMethod,
    partition_count: u8,
    expected_samples: usize,
    sample_bits: Option<u8>,
    out: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: decode_residual (step 3c)")
}

/// Decode one partition's Rice-coded samples into `out`.
///
/// Split out from [`decode_residual`] so it can be unit-tested (and benchmarked)
/// on its own — this is the inner loop.
fn decode_rice_partition(
    reader: &mut BitReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: decode_rice_partition (step 3b)")
}

/// Unfold a Rice-coded residual: folded magnitude → signed sample
/// (RFC 9639 §9.2.7.2).
///
/// The encoder folds signed → unsigned as `x ≥ 0 → 2x`, `x < 0 → −2x − 1`
/// ("for positive numbers, the representation is the number doubled. For
/// negative numbers, the representation is the number multiplied by -2 and
/// with 1 subtracted"). So the decoder's inverse is: **even `n → n >> 1`,
/// odd `n → !(n >> 1)`**.
///
/// ```text
///  0 →  0    1 → −1    2 →  1    3 → −2    4 →  2    38 → 19    39 → −20
/// ```
///
/// **Corrected 2026-09-10:** this scaffold's doc claimed the mirrored
/// convention ("odd → (n+1)/2, even → −n/2", `negative = n & 1`), which
/// sign-flips every decoded residual. §9.2.7.2's own worked example is the
/// witness: folded 38 unfolds to **+19**; the scaffold's mapping returns
/// −19. FLAC.md → step 3a entry.
///
/// (Rendered: `0 → 0`, `1 → −1`, `2 → 1`, `3 → −2`, `38 → 19`, `39 → −20`.)
///
/// Pure map, no validation: §9.2.7.3 forbids residual values equal to
/// `i32::MIN` (folded input `0xFFFF_FFFF`), and enforcing that is the
/// partition decoder's job — it sees the stream and can fail the frame
/// (step 3b). Here the mapping stays total and returns `i32::MIN` for that
/// input, pinned by a test so the division of responsibility is explicit.
fn rice_unmap(n: u32) -> i32 {
    if n & 1 == 0 {
        (n >> 1) as i32
    } else {
        (!(n >> 1)) as i32
    }
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). `core`-only,
// like the rest of the crate's unit tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_vectors_per_rfc_9_2_7_2() {
        // 0 and the sign alternation are the mapping's whole personality.
        assert_eq!(rice_unmap(0), 0);
        assert_eq!(rice_unmap(1), -1);
        assert_eq!(rice_unmap(2), 1);
        assert_eq!(rice_unmap(3), -2);
        assert_eq!(rice_unmap(4), 2);
        assert_eq!(rice_unmap(5), -3);
        // The RFC §9.2.7.2 worked example: Rice parameter 3, folded value 38
        // (unary 4 << 3 | binary 6). The spec unfolds it to +19; the retired
        // scaffold convention would give -19. This assertion is the witness.
        assert_eq!(rice_unmap(38), 19);
        assert_eq!(rice_unmap(39), -20);
    }

    #[test]
    fn roundtrips_the_encoders_fold() {
        // The encoder's fold, written straight from the spec text in i64 so
        // the extremes cannot overflow the intermediate.
        let fold = |x: i32| -> u32 {
            let v = if x >= 0 {
                2 * i64::from(x)
            } else {
                -2 * i64::from(x) - 1
            };
            u32::try_from(v).expect("fold must fit u32 for any i32")
        };
        // Dense around zero (where off-by-one signs live), sparse-wide, plus
        // the exact edges. i32::MIN folds to the §9.2.7.3-forbidden value;
        // it is exercised separately in `extremes_and_the_forbidden_value`.
        for x in -2000i32..=2000 {
            assert_eq!(rice_unmap(fold(x)), x, "roundtrip {x}");
        }
        for x in (i32::MIN + 1..=i32::MAX).step_by(1_000_003) {
            assert_eq!(rice_unmap(fold(x)), x, "roundtrip {x}");
        }
        assert_eq!(rice_unmap(fold(i32::MAX)), i32::MAX);
        assert_eq!(rice_unmap(fold(i32::MIN + 1)), i32::MIN + 1);
    }

    #[test]
    fn matches_subtraction_oracle_at_both_ends() {
        // Independent oracle via a different mechanism: odd n = 2k+1 folds
        // -(k+1), computed by negate-and-subtract rather than bitwise NOT.
        // The intermediate is i64: at n = 0xFFFF_FFFF the negation of
        // 2^31 must not overflow the i32 intermediate (that input is the
        // §9.2.7.3-forbidden one, and the oracle still has to name it).
        let oracle = |n: u32| -> i32 {
            if n & 1 == 0 {
                (n >> 1) as i32
            } else {
                (-(i64::from(n >> 1) + 1)) as i32
            }
        };
        for n in 0..65_536u32 {
            assert_eq!(rice_unmap(n), oracle(n), "n {n}");
        }
        // The high window: near u32::MAX the folded magnitudes are the huge
        // residuals; sweep the last 64Ki values (65535 candidates every
        // other parity, cheap).
        for k in 0..32_768u32 {
            let n = u32::MAX - 2 * k;
            assert_eq!(rice_unmap(n), oracle(n), "n {n:#x}");
            let n = u32::MAX - (2 * k + 1);
            assert_eq!(rice_unmap(n), oracle(n), "n {n:#x}");
        }
    }

    #[test]
    fn extremes_and_the_forbidden_value() {
        // The largest legal positive residual (fold of i32::MAX).
        assert_eq!(rice_unmap(0xFFFF_FFFE), i32::MAX);
        // n = 0xFFFF_FFFF maps to i32::MIN — the value §9.2.7.3 forbids in a
        // stream. This pure map returns it (mapping is total); rejecting it
        // is decode_rice_partition's contract with the stream (step 3b).
        assert_eq!(rice_unmap(0xFFFF_FFFF), i32::MIN);
    }
}
