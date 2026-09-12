//! Residual coding: partitioned Rice / Rice2 / escape-record residual.
//!
//! SCAFFOLD STATE (2026-09-11): [`rice_unmap`] (step 3a) and
//! [`decode_rice_partition`] (step 3b — the unary/remainder inner loop plus
//! the §9.2.7.1 escape-record partition) are implemented. `decode_residual`
//! — the header (method/partition order), per-partition sample counts, and
//! the "unknown" sample-size derivation — remains `todo!()` (step 3c).
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

/// One partition's header, as read from the stream (§9.2.7/§9.2.7.1).
///
/// The caller reads the parameter field itself (width follows [`RiceMethod`])
/// and splits this header out of it: the escape code (`0b1111` / `0b11111`)
/// sets `is_escape` and the 5 raw-width bits fill `raw_bits`; otherwise the
/// unsigned parameter fills `order`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionHeader {
    /// Rice parameter = bits of least-significant (binary remainder) part.
    /// Legal range follows the method: 0..=14 under [`RiceMethod::Rice4Bit`]
    /// (`0b1111` is the escape code), 0..=30 under [`RiceMethod::Rice5Bit`]
    /// (`0b11111` is the escape).
    pub order: u8,
    /// Escape record (§9.2.7.1): samples in this partition are unencoded,
    /// stored at `raw_bits` each.
    pub is_escape: bool,
    /// Bits per residual sample for an escaped partition. **5-bit field**
    /// ("Directly following the escape code are 5 bits containing the number
    /// of bits with which each residual sample is stored", §9.2.7.1 — the
    /// scaffold's "4-bit field" note was wrong; corrected 2026-09-11 while
    /// implementing 3b). `0` is legal and means all-zero samples stored with
    /// no bits at all.
    pub raw_bits: u8,
    /// Samples in this partition (per §9.2.7's first-partition-minus-order
    /// rule — the caller computes it; this function only checks `out`).
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

/// Decode one partition's samples into `out` (RFC 9639 §9.2.7.2 Rice, or
/// §9.2.7.1 escape-record raw samples).
///
/// Split out from [`decode_residual`] so it can be unit-tested on its own —
/// this is the inner loop. The parameter field (and the escape code that
/// selects between the two partition forms) is read by the caller; this
/// function consumes the partition *body* only, from a cursor sitting on the
/// first body bit.
///
/// Contract: `out.len()` must equal `header.sample_count`. The caller
/// (step 3c's [`decode_residual`]) slices `out` per partition; a mismatch is
/// a caller bug, not a stream condition, and yields `InvalidField` without
/// moving the cursor.
///
/// **Rice partition (§9.2.7.2):** per sample, count unary zeros until the
/// terminating one (the quotient), read `order` remainder bits, combine
/// `folded = (quotient << order) | remainder`, and unfold with [`rice_unmap`].
/// `order == 0` means unary-only: the remainder read is *skipped*, not
/// zero-width — [`BitReader::read_bits`] rejects `n == 0` by design, and
/// conflating the two desynchronises every subsequent word.
///
/// **Escape partition (§9.2.7.1):** `raw_bits` signed two's-complement bits
/// per sample (via [`BitReader::read_signed`], which owns sign extension).
/// `raw_bits == 0` is legal and means every sample is 0 with **no bits
/// consumed at all** — the partition is just its escape code and `0b00000`
/// (which the caller read).
///
/// **§9.2.7.3 value limit:** residuals MUST NOT reach `±2^31`. Two stream
/// conditions implement that here as `InvalidField`:
/// a folded value that cannot fit `u32` (checked shift — such a residual
/// cannot exist under the limit), and `folded == 0xFFFF_FFFF`, the unique
/// pre-image of `i32::MIN`. [`rice_unmap`] deliberately stays total and
/// returns `i32::MIN`; rejecting it is this function's job (the seam pinned
/// in step 3a).
///
/// Cursor: on success it sits exactly past the partition body. On error it
/// stays where the failing read left it (per-read atomicity only, the same
/// rule as the header parses — an errored frame is discarded wholesale, so
/// partially written `out` samples are undefined).
fn decode_rice_partition(
    reader: &mut BitReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> crate::Result<()> {
    if out.len() != usize::from(header.sample_count) {
        // Caller contract violation (slice length must track the header).
        return Err(crate::Error::InvalidField);
    }
    if header.is_escape {
        // §9.2.7.1: fixed-width raw residuals; 0 bits means all-zero samples
        // and consumes nothing ("the partition contains nothing except the
        // escape code and 0b00000").
        for sample in out.iter_mut() {
            *sample = if header.raw_bits == 0 {
                0
            } else {
                reader.read_signed(u32::from(header.raw_bits))?
            };
        }
        return Ok(());
    }
    let order = u32::from(header.order);
    for sample in out.iter_mut() {
        // Unary quotient: count zeros until the terminating one.
        let mut quotient: u32 = 0;
        while reader.read_bits(1)? == 0 {
            quotient += 1;
        }
        // Binary remainder — only when the parameter says so. `order == 0`
        // is unary-only words; there is nothing to read.
        let remainder = if order == 0 {
            0
        } else {
            reader.read_bits(order)?
        };
        // §9.2.7.3: a residual must be representable in 32 bits, so a folded
        // value that does not fit `u32` cannot occur in a conforming stream.
        // Guard on the *quotient* against `MAX >> order`: `checked_shl` is NOT
        // that check — it only rejects a shift *amount* >= 32 and wraps bits
        // shifted out of the value (found by test, 2026-09-11: quotient 4 at
        // order 30 silently yielded folded = 0). The `order >= 32` arm
        // short-circuits before the shift, so `MAX >> order` is never a
        // shift-by-32. Under the profile order <= 30 (0b11111 is the escape),
        // so this is belt-and-braces for a bad caller header.
        if order >= 32 || quotient > (u32::MAX >> order) {
            return Err(crate::Error::InvalidField);
        }
        let folded = (quotient << order) | remainder;
        if folded == u32::MAX {
            // The i32::MIN pre-image: forbidden in a stream (§9.2.7.3).
            return Err(crate::Error::InvalidField);
        }
        *sample = rice_unmap(folded);
    }
    Ok(())
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

    // ---- decode_rice_partition (step 3b) ----------------------------------
    //
    // Witness rule per Sam's 2026-09-11 direction: reuse what exists and
    // cover edge cases with unit tests; the frame_vectors.py per-partition
    // oracle extension is deferred (it only pays off once 3c/3e make the
    // whole residual path live). Witnesses here: the RFC's own worked
    // example (§9.2.7.2), hand-derived codewords, and a differential sweep
    // against an independent bit packer that encodes straight from the
    // spec text (fold → unary → remainder) — a different mechanism than the
    // decoder's zeros-count loop.

    use crate::Error;

    /// Independent MSB-first bit packer. Deliberately naive (one bit at a
    /// time) so a shared bug with `BitReader`/`peek_at` is unlikely.
    struct Packer<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl<'a> Packer<'a> {
        fn new(buf: &'a mut [u8]) -> Self {
            buf.iter_mut().for_each(|b| *b = 0);
            Self { buf, pos: 0 }
        }
        fn put(&mut self, value: u32, width: u32) {
            for i in (0..width).rev() {
                let bit = ((value >> i) & 1) as u8;
                self.buf[self.pos >> 3] |= bit << (7 - (self.pos & 7));
                self.pos += 1;
            }
        }
    }

    /// Write one Rice codeword straight from §9.2.7.2's encoding text:
    /// fold signed → unsigned (`x ≥ 0 → 2x`, `x < 0 → −2x − 1`), unary the
    /// quotient (`folded >> order`), then `order` remainder bits. `order`
    /// must be ≥ 1 here; order-0 words are packed explicitly in the test
    /// that needs them.
    fn rice_put(p: &mut Packer<'_>, order: u32, residual: i32) {
        let folded: u32 = if residual >= 0 {
            (2i64 * i64::from(residual)) as u32
        } else {
            (-2i64 * i64::from(residual) - 1) as u32
        };
        let quotient = folded >> order;
        let remainder = folded & (((1u32 << order) - 1) as u32);
        for _ in 0..quotient {
            p.put(0, 1);
        }
        p.put(1, 1);
        p.put(remainder, order);
    }

    fn rice_header(order: u8, sample_count: u16) -> PartitionHeader {
        PartitionHeader {
            order,
            is_escape: false,
            raw_bits: 0,
            sample_count,
        }
    }

    fn escape_header(raw_bits: u8, sample_count: u16) -> PartitionHeader {
        PartitionHeader {
            order: 0,
            is_escape: true,
            raw_bits,
            sample_count,
        }
    }

    #[test]
    fn rfc_9_2_7_2_worked_example() {
        // The RFC's own example: Rice parameter 3, folded residual 38 →
        // unary 4 = 0b00001, binary 6 = 0b110, codeword 0b00001110.
        // Unfolds to +19.
        let mut r = BitReader::new(&[0b0000_1110]);
        let mut out = [0i32; 1];
        decode_rice_partition(&mut r, &rice_header(3, 1), &mut out).unwrap();
        assert_eq!(out, [19]);
        assert_eq!(r.bit_position(), 8); // exact-fit, no over/under-read
    }

    #[test]
    fn hand_packed_multi_sample_partition() {
        // Residuals [0, -1, 2, -2] at order 1. Folds via §9.2.7.2 (`x ≥ 0 →
        // 2x`, `x < 0 → −2x − 1`): [0, 1, 4, 3] — note fold(−2) = **3**, not
        // 5 (5 is fold(−3); an earlier draft of this vector mis-derived it and
        // the decoder faithfully returned −3, i.e. the test was wrong and the
        // impl right — FLAC.md's recurring lesson).
        // Words (unary quotient, then `order` remainder bits):
        //   fold 0 → q0 "1"   r0 "0"  = 10
        //   fold 1 → q0 "1"   r1 "1"  = 11
        //   fold 4 → q2 "001" r0 "0"  = 0010
        //   fold 3 → q1 "01"  r1 "1"  = 011
        // = 1011_0010_011 → 0xB2, 0x60. 11 bits consumed; the 5 slack bits
        // must NOT be eaten.
        let mut r = BitReader::new(&[0xB2, 0x60]);
        let mut out = [0i32; 4];
        decode_rice_partition(&mut r, &rice_header(1, 4), &mut out).unwrap();
        assert_eq!(out, [0, -1, 2, -2]);
        // 2 + 2 + 4 + 3 = 11 codeword bits.
        assert_eq!(r.bit_position(), 11);
    }

    #[test]
    fn order_zero_partitions_are_unary_only() {
        // order 0 ⇒ remainder width 0 ⇒ nothing is read (§9.2.7.2: the
        // parameter "is the number of bits in the least-significant part").
        // `read_bits(0)` returns InvalidField by design, so the width-0 case
        // must skip the remainder read entirely — a counterfactual this test
        // enforces: calling read_bits(0) would error, and over-consuming a
        // bit would break the cursor.
        // Folds [0, 1, 2] (residuals [0, -1, 1]) → "1", "01", "001"
        // = 1_01_001 → 101_001 00 → 0xA4.
        let mut r = BitReader::new(&[0xA4]);
        let mut out = [0i32; 3];
        decode_rice_partition(&mut r, &rice_header(0, 3), &mut out).unwrap();
        assert_eq!(out, [0, -1, 1]);
        assert_eq!(r.bit_position(), 6); // exactly the 6 unary bits
    }

    #[test]
    fn escape_partition_raw_twos_complement() {
        // §9.2.7.1: raw signed two's-complement at raw_bits width. The
        // RFC's example is included: with 3 bits, -1 is 0b111.
        // [-1, 2, -2] at 3 bits → 111 010 110 → 0xEB, 0x0 (1 slack bit).
        let mut r = BitReader::new(&[0xEB, 0x00]);
        let mut out = [0i32; 3];
        decode_rice_partition(&mut r, &escape_header(3, 3), &mut out).unwrap();
        assert_eq!(out, [-1, 2, -2]);
        assert_eq!(r.bit_position(), 9);
    }

    #[test]
    fn escape_partition_zero_bits_consumes_nothing() {
        // §9.2.7.1: "the number of bits ... is 0 ... all residual samples
        // have a value of 0 and no bits are used". The partition is *just*
        // the escape code + 0b00000 (the caller's), so the empty rest of the
        // buffer must be left untouched.
        let mut r = BitReader::new(&[0xAB]); // must not be read
        let mut out = [0xFFi32; 3];
        decode_rice_partition(&mut r, &escape_header(0, 3), &mut out).unwrap();
        assert_eq!(out, [0, 0, 0]);
        assert_eq!(r.bit_position(), 0); // zero bits consumed
    }

    #[test]
    fn rejects_the_forbidden_i32_min_residual() {
        // §9.2.7.3: residual MUST NOT be i32::MIN, i.e. folded 0xFFFF_FFFF.
        // Reachable witness without a 4-billion-bit unary run: order 30
        // (legal in a 5-bit parameter), quotient 3, remainder all-ones:
        // folded = (3 << 30) | 0x3FFF_FFFF = 0xFFFF_FFFF.
        // Bits: 0001 (unary 3) + 30 ones = 34 bits.
        let mut r = BitReader::new(&[0x1F, 0xFF, 0xFF, 0xFF, 0xC0]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(30, 1), &mut out),
            Err(Error::InvalidField)
        );
    }

    #[test]
    fn rejects_quotients_that_cannot_fit_u32() {
        // A folded value ≥ 2^32 cannot exist under §9.2.7.3 either. Order
        // 30, quotient 4: 4 << 30 overflows u32 → checked_shl rejects.
        // Bits: 00001 (unary 4) + 30 zero remainder bits = 35 bits.
        let mut r = BitReader::new(&[0x08, 0x00, 0x00, 0x00, 0x00]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(30, 1), &mut out),
            Err(Error::InvalidField)
        );
    }

    #[test]
    fn out_slice_length_must_match_sample_count() {
        // Caller-contract violation, checked before any bit is consumed.
        let mut r = BitReader::new(&[0xFF; 4]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(1, 2), &mut out),
            Err(Error::InvalidField)
        );
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn truncated_stream_fails_as_end_of_stream() {
        // Order 2, two samples; only the first codeword + two unary zeros
        // of the second fit. First sample must decode; second fails as EOF
        // with per-read (not per-partition) cursor atomicity.
        // Sample 1: fold 15 → q=3 ("0001"), r=3 ("11") → 0b0001_11xx.
        let mut r = BitReader::new(&[0b0001_1100]);
        let mut out = [-7i32; 2];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(2, 2), &mut out),
            Err(Error::EndOfStream)
        );
        // fold 15 is odd → !(15 >> 1) = !7 = -8 (hand-computed, not via the
        // map under test elsewhere): the first sample must have landed.
        assert_eq!(out[0], -8);
        assert_eq!(out[1], -7); // second never written
    }

    #[test]
    fn rice_sweep_vs_packer_oracle() {
        // Encode straight from the spec text, decode through the impl, and
        // compare — at every Rice parameter 1..=14 (0 is covered by the
        // hand-built order-0 test) and at every start bit alignment, so
        // byte-crossing quotients and remainders are witnessed rather than
        // just the aligned happy path.
        const RESIDUALS: [i32; 12] = [0, -1, 1, -2, 2, 19, -20, 3, -4, 5, 6, -7];
        for order in 1u32..=14 {
            for align in 0u32..8 {
                let mut buf = [0u8; 96];
                let mut p = Packer::new(&mut buf);
                // `align` filler bits: put() with width 0 is a no-op, so the
                // aligned case needs no special case.
                p.put(0xAB & ((1 << align) - 1), align);
                for &x in &RESIDUALS {
                    rice_put(&mut p, order, x);
                }
                let packed_bits = p.pos;
                drop(p); // release &mut buf before the reader borrows it
                let mut r = BitReader::new(&buf);
                if align > 0 {
                    // Walk past the filler — `read_bits(0)` would be an
                    // InvalidField, hence the guard.
                    r.read_bits(align).unwrap();
                }
                let mut out = [0i32; RESIDUALS.len()];
                decode_rice_partition(&mut r, &rice_header(order as u8, 12), &mut out)
                    .unwrap_or_else(|e| panic!("order {order} align {align}: {e:?}"));
                assert_eq!(out, RESIDUALS, "order {order} align {align}");
                // The cursor lands exactly past the last codeword — no bit
                // of slack eaten, none left inside the word.
                assert_eq!(
                    r.bit_position(),
                    packed_bits,
                    "order {order} align {align}: cursor"
                );
            }
        }
    }

    #[test]
    fn escape_sweep_raw_widths_vs_packer_oracle() {
        // Same rule for the escape path: pack masked words at every legal
        // raw width 1..=31 (the 5-bit field's whole range; the GBA profile
        // tops out at 16, the format does not), decode via read_signed, and
        // pin the cursor.
        //
        // Expectations are derived from the *packed* bits, not from the
        // original sample: a value only round-trips at widths wide enough to
        // hold it (-128 needs 8 bits), and asserting the raw samples back
        // would encode exactly the "recalled" bug this project keeps
        // catching. The sign-extension oracle is subtract-based
        // (`m ≥ 2^(w-1) → m − 2^w`), a different mechanism than
        // read_signed's shl-then-asr.
        const SAMPLES: [i32; 10] = [0, -1, 1, -2, 2, -64, 63, -128, 32_767, -32_768];
        for raw in 1u32..=31 {
            let mask = (1u32 << raw) - 1;
            let half = 1i64 << (raw - 1);
            let whole = 1i64 << raw;
            let mut buf = [0u8; 64];
            let mut p = Packer::new(&mut buf);
            let mut expect = [0i32; SAMPLES.len()];
            for (i, &x) in SAMPLES.iter().enumerate() {
                let masked = (x as u32) & mask;
                p.put(masked, raw);
                let m = i64::from(masked);
                expect[i] = if m >= half { m - whole } else { m } as i32;
            }
            let packed_bits = p.pos;
            drop(p); // release &mut buf before the reader borrows it
            let mut r = BitReader::new(&buf);
            let mut out = [0i32; SAMPLES.len()];
            decode_rice_partition(
                &mut r,
                &escape_header(raw as u8, SAMPLES.len() as u16),
                &mut out,
            )
            .unwrap_or_else(|e| panic!("raw {raw}: {e:?}"));
            assert_eq!(out, expect, "raw {raw}");
            assert_eq!(r.bit_position(), packed_bits, "raw {raw}: cursor");
        }
    }
}
