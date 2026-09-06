//! MSB-first bit reader over an immutable byte slice.
//!
//! STATUS (2026-09-05): **core read path implemented** — `new`, `read_bits`,
//! `peek_bits`, `read_signed`, `read_utf8_coded`, `bit_position`,
//! `bits_remaining` (roadmap Step 1 + `peek_bits` + `read_signed` +
//! `read_utf8_coded`, FLAC.md "Next steps"). The remaining methods
//! (`byte_align`, `read_u8`, CRCs) are still scaffold (`todo!()`).
//!
//! FLAC packs its fields MSB-first across byte boundaries, so the whole decoder
//! is built on this one primitive. Design notes for the implementation:
//!
//! - **Position-only design** (chosen 2026-09-04): the cursor is a plain bit
//!   offset into the backing slice; every read recomputes what it needs from
//!   `bit_pos`. No refill accumulator — the naive read is a handful of shifts,
//!   and ARMv4T (ARM7TDMI) has no hardware divide anyway, so byte-wise loads
//!   dominate regardless. If the perf spike (FLAC.md) ever shows the bit reader
//!   hot, a buffer can be added *behind the same API* without touching callers.
//! - ARMv4T (ARM7TDMI) has **CLZ** but **no hardware divide** — prefer
//!   shift/add and `& 7` masks over anything multiplicative. Nothing here
//!   divides or modulo-divides by a variable.
//! - Signed fields (LP coefficients, residuals, verbatim samples) are
//!   two's-complement in the field's stated width: sign-extend after reading.
//! - Verbatim / raw-signature subframes and metadata DIDs are byte-aligned:
//!   use [`BitReader::byte_align`], never assume alignment.

use crate::{Error, Result};

/// A cursor-based, borrow-only bit reader.
///
/// Zero-cost to construct, zero allocation, `Copy`-ish for cheap save/restore in
/// tests. There is no `Seek` — the cursor only moves forward.
#[derive(Clone, Copy, Debug)]
pub struct BitReader<'a> {
    /// Backing bytes (normally a subslice of a `&'static` ROM blob).
    data: &'a [u8],
    /// Read cursor, in bits from the start of `data`. Invariant:
    /// `bit_pos <= data.len() * 8` (the cursor never advances past the end —
    /// failed reads leave it untouched).
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    /// Wrap a byte slice for bit-level reading.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    /// Read `n` bits (1..=32) as an unsigned value, MSB-first.
    ///
    /// Returns [`Error::EndOfStream`] if fewer than `n` bits remain (cursor
    /// unchanged), and [`Error::InvalidField`] for `n == 0` or `n > 32`, which
    /// are caller bugs rather than stream conditions.
    pub fn read_bits(&mut self, n: u32) -> Result<u32> {
        let val = self.peek_at(self.bit_pos, n)?;
        // `peek_at` only succeeds when `bit_pos + n` fits in the slice, so the
        // invariant `bit_pos <= total_bits` is preserved.
        self.bit_pos += n as usize;
        Ok(val)
    }

    /// Peek at the next `n` bits (1..=32) without advancing the cursor.
    ///
    /// Same width and end-of-stream semantics as [`Self::read_bits`] — same
    /// error variants, cursor untouched on success *and* failure. This is
    /// exactly what `peek_at` already guarantees, so peek is a direct wrap;
    /// the caller-visible difference vs `read_bits` is purely the missing
    /// `bit_pos += n`.
    pub fn peek_bits(&self, n: u32) -> Result<u32> {
        self.peek_at(self.bit_pos, n)
    }

    /// Read `n` bits (1..=32) as a two's-complement signed value.
    ///
    /// Same width/EOF semantics as [`Self::read_bits`] (delegates to it, so
    /// validation and cursor rules are identical by construction). FLAC
    /// encodes LP coefficients, Rice-partition samples, verbatim samples, and
    /// unaligned warm-up values as raw two's-complement in the field's stated
    /// width — sign extension is pure post-processing.
    ///
    /// Sign extension is the classic **shl-then-arithmetic-shr** idiom: move
    /// the field's sign bit up into bit 31, then let `>>` on `i32` (an
    /// arithmetic shift on ARM, `asr`) drag the sign down through the high
    /// half. No divide, no subtract, and it needs **no special case for
    /// `n == 32`** — there the shifts are both by 0, which Rust defines, and
    /// the result is just the `as i32` reinterpretation.
    pub fn read_signed(&mut self, n: u32) -> Result<i32> {
        let raw = self.read_bits(n)?;
        // n in 1..=32, so both shift amounts are in 0..=31 (never >= 32).
        let shift = 32 - n;
        Ok(((raw << shift) as i32) >> shift)
    }

    /// Read FLAC's UTF-8-style **coded number** (RFC 9639 §9.1.5): the
    /// variable-length prefix code used for frame/sample numbers. FLAC reuses
    /// the UTF-8 shape but extends it to 36-bit values / 7-byte forms —
    /// general-purpose UTF-8 decoders cannot parse these, so this is
    /// deliberately its own reader.
    ///
    /// Leading one-bits of the first byte set the total length; the first
    /// byte's remaining payload bits are the value's top bits, and each
    /// following byte contributes 6 (`10xxxxxx` continuation) bits, MSB-first:
    ///
    /// ```text
    /// 0xxxxxxx                                    → 1 byte,  7 payload bits
    /// 110xxxxx 10xxxxxx                           → 2 bytes, 11 bits
    /// 1110xxxx 10xxxxxx 10xxxxxx                  → 3 bytes, 16 bits
    /// 11110xxx  …                                 → 4 bytes, 21 bits
    /// 111110xx  …                                 → 5 bytes, 26 bits
    /// 1111110x  …                                 → 6 bytes, 31 bits
    /// 11111110 10xxxxxx ×6                        → 7 bytes, 36 bits
    /// ```
    ///
    /// Errors ([`Error::InvalidField`]): a stray continuation lead
    /// (`0b10xxxxxx`, prefix == 1) and an all-ones first byte (`0xFF`,
    /// prefix == 8) are not legal leads; any non-`0b10xxxxxx` continuation
    /// byte is likewise rejected. (The pre-RFC spec's "doubled prefix"
    /// warning applied to `0xFE`, which RFC 9639 legalizes as the 7-byte
    /// lead — only `0xFF` remains invalid.)
    ///
    /// This is a *bit-level* decoder: it does not enforce canonical (shortest)
    /// form — non-canonical leads like `0xC0 0x80` decode to small values by
    /// design, exactly as libFLAC's decoder accepts them. Stream semantics
    /// (numbers must match the frame count / be strictly increasing) belong
    /// to the frame layer, where those invariants are actually checkable.
    ///
    /// Cursor is **atomic**: a multi-byte read that fails partway (bad lead,
    /// bad continuation, EOF) restores the cursor exactly as it was — the
    /// composite of several `read_bits` calls, rolled back on error.
    pub fn read_utf8_coded(&mut self) -> Result<u64> {
        let start = self.bit_pos;
        match self.coded_number_inner() {
            Ok(v) => Ok(v),
            Err(e) => {
                // Atomicity across the whole composite read; `bit_pos` only
                // ever moves forward, and a failed number read never moves it
                // at all.
                self.bit_pos = start;
                Err(e)
            }
        }
    }

    /// Consume bits up to the next byte boundary. Returns the number of bits
    /// discarded (0..7).
    pub fn byte_align(&mut self) -> u32 {
        todo!("flac-lite scaffold: BitReader::byte_align")
    }

    /// Read a `u8` (assumes byte-aligned cursor; used for CRC-8 / padding).
    pub fn read_u8(&mut self) -> Result<u8> {
        todo!("flac-lite scaffold: BitReader::read_u8")
    }

    /// Bits remaining in the backing slice from the current cursor.
    pub fn bits_remaining(&self) -> usize {
        (self.data.len() << 3) - self.bit_pos
    }

    /// Current cursor position in bits (useful for frame-boundary assertions
    /// and for the CRC-8-over-header check, which needs the raw header bytes).
    pub fn bit_position(&self) -> usize {
        self.bit_pos
    }

    /// Read `n` bits (1..=32, MSB-first) starting at absolute bit position
    /// `pos`, without moving the cursor. This is the single place that touches
    /// the bytes; `read_bits` (and later `peek_bits`) layer on top of it.
    ///
    /// Layout: at most `((7 + 32) + 7) >> 3 == 5` bytes can be involved in one
    /// read, so accumulating them into a `u64` (≤ 40 bits) is safe and keeps
    /// every shift below 64 — in particular there is no shift-by-32 trap even
    /// though `n == 32` is legal.
    fn peek_at(&self, pos: usize, n: u32) -> Result<u32> {
        // Width outside 1..=32 is a caller bug; `InvalidField` is the closest
        // stream-agnostic variant the crate's flat error enum offers.
        if n == 0 || n > 32 {
            return Err(Error::InvalidField);
        }
        let total_bits = self.data.len() << 3;
        let end = pos
            .checked_add(n as usize)
            .filter(|&e| e <= total_bits)
            .ok_or(Error::EndOfStream)?;

        let byte_idx = pos >> 3;
        let off = (pos & 7) as u32; // bit offset within the first byte, 0..=7
        // Bytes spanned by bits [pos, end): division-free ceil((off + n) / 8).
        let bytes_needed = (((off + n + 7) >> 3) as usize).min(self.data.len()); // <= 5

        // Gather the spanned bytes, big-endian, into a bottom-aligned u64
        // accumulator (width = bytes_needed * 8 <= 40 bits).
        let mut acc: u64 = 0;
        for &b in &self.data[byte_idx..byte_idx + bytes_needed] {
            acc = (acc << 8) | u64::from(b);
        }
        // In that accumulator the wanted field ends `off` bits below the top,
        // so its LSB sits at bit index (bytes_needed*8 - off - n). That shift
        // is always in 0..40 — never 64, never negative — even for n == 32.
        // The `off` leading bits of the first byte still sit above the field
        // after shifting, so the field must be masked to n bits.
        let shift = bytes_needed as u32 * 8 - off - n;
        let mask = (1u64 << n) - 1; // n in 1..=32, so this cannot overflow u64
        Ok(((acc >> shift) & mask) as u32)
    }

    /// Decode one UTF-8-style coded number (RFC 9639 §9.1.5), advancing the
    /// cursor as it goes. [`Self::read_utf8_coded`] wraps this to make the
    /// composite read atomic — every failure path here returns through the
    /// wrapper, which restores the cursor.
    fn coded_number_inner(&mut self) -> Result<u64> {
        let lead = self.read_bits(8)? as u8;
        // Count leading ones of the lead byte: shift/mask only, at most 8
        // iterations (`mask` reaching 0 ends the loop for every input, per
        // the module's no-divide ARMv4T rule).
        let mut prefix: u32 = 0;
        let mut mask = 0x80u8;
        while lead & mask != 0 {
            prefix += 1;
            mask >>= 1;
        }
        // prefix == 1: stray continuation byte (10xxxxxx cannot lead).
        // prefix == 8: 0xFF, the one all-ones lead RFC 9639 leaves invalid
        // (0xFE is the legal 7-byte lead; the pre-RFC "doubled prefix"
        // warning concerned it, not 0xFF).
        if prefix == 1 || prefix == 8 {
            return Err(Error::InvalidField);
        }
        // prefix == 0 → single byte; otherwise the prefix counts ALL octets.
        let total = if prefix == 0 { 1 } else { prefix };
        // Lead-byte payload width is 7 - prefix (zero for the 7-byte lead).
        // `0xFFu32 >> (prefix + 1)` is that width's mask with no subtraction
        // edge case: prefix == 7 shifts a u32 by 8 → 0, which is defined.
        let mut value = u64::from(u32::from(lead) & (0xFFu32 >> (prefix + 1)));
        for _ in 1..total {
            let cont = self.read_bits(8)? as u8;
            if cont & 0xC0 != 0x80 {
                return Err(Error::InvalidField);
            }
            value = (value << 6) | u64::from(cont & 0x3F);
        }
        Ok(value)
    }
}

/// CRC-8 with FLAC's polynomial (0x07, no reflection, init 0x00).
///
/// Only used for the frame header; gated behind `crc-check`.
pub fn crc8(data: &[u8]) -> u8 {
    todo!("flac-lite scaffold: crc8")
}

/// CRC-16 ("FLAC" variant: polynomial 0x05, init 0xFFFF) over a byte range.
///
/// Used for the frame footer and stream metadata. Table-free by design — a
/// 256×u16 table is 512 bytes of ROM we could spend elsewhere; decide in the
/// perf spike whether the table pays for itself.
pub fn crc16(data: &[u8]) -> u16 {
    todo!("flac-lite scaffold: crc16")
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). Keep these
// `core`-only: no `std`, and no dependency on any not-yet-implemented method.
#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle: bit `i` of the slice, MSB-first within each byte.
    /// Deliberately naive so a shared bug with `peek_at` is unlikely.
    fn ref_bit(data: &[u8], i: usize) -> u32 {
        u32::from((data[i >> 3] >> (7 - (i & 7))) & 1)
    }

    /// Deterministic pseudo-random bytes (LCG) — no_std-friendly, reproducible.
    struct Lcg(u32);
    impl Lcg {
        fn next_u8(&mut self) -> u8 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 24) as u8
        }
    }

    // ---- width validation -------------------------------------------------

    #[test]
    fn read_bits_rejects_out_of_range_widths() {
        let mut r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.read_bits(0), Err(Error::InvalidField));
        assert_eq!(r.read_bits(33), Err(Error::InvalidField));
        assert_eq!(r.read_bits(u32::MAX), Err(Error::InvalidField));
        // Rejected reads must not move the cursor.
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.read_bits(32).unwrap(), 0xFFFF_FFFF);
    }

    // ---- empty / EOF / cursor invariants ----------------------------------

    #[test]
    fn empty_slice_reports_end_of_stream() {
        let mut r = BitReader::new(&[]);
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_may_end_exactly_at_last_bit_but_not_one_past() {
        // 0xA55A = 1010_0101_0101_1010; top 15 bits = 0x52AD, last bit = 0.
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        assert_eq!(r.read_bits(15).unwrap(), 0x52AD);
        assert_eq!(r.bits_remaining(), 1);
        assert_eq!(r.read_bits(1).unwrap(), 0); // last bit, exact fit: ok
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.bit_position(), 16);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 16); // unchanged after failure
    }

    #[test]
    fn failed_read_leaves_cursor_untouched_mid_stream() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.bits_remaining(), 5);
    }

    // ---- hand-computed patterns (MSB-first) --------------------------------

    #[test]
    fn nibbles_within_one_byte() {
        let mut r = BitReader::new(&[0xA5]); // 1010_0101
        assert_eq!(r.read_bits(4).unwrap(), 0xA);
        assert_eq!(r.read_bits(4).unwrap(), 0x5);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
    }

    #[test]
    fn msb_first_bit_order() {
        let mut r = BitReader::new(&[0b1011_0100]);
        let mut got = [0u32; 8];
        for g in &mut got {
            *g = r.read_bits(1).unwrap();
        }
        assert_eq!(got, [1, 0, 1, 1, 0, 1, 0, 0]);
    }

    #[test]
    fn read_spans_byte_boundary() {
        let mut r = BitReader::new(&[0x12, 0x34]); // 0001_0010 0011_0100
        assert_eq!(r.read_bits(12).unwrap(), 0x123);
        assert_eq!(r.read_bits(4).unwrap(), 0x4);
    }

    #[test]
    fn full_32_bit_aligned_read() {
        // The shift-by-32 trap: a `(x >> (32 - n))` formulation with n == 32
        // shifts by 32 and panics/UBs. `n == 32` is legal, so it must work.
        let mut r = BitReader::new(&[0xDE, 0xAD, 0xBF, 0xAC]);
        assert_eq!(r.read_bits(32).unwrap(), 0xDEAD_BFAC);
        assert_eq!(r.bit_position(), 32);
    }

    #[test]
    fn full_32_bit_unaligned_read_spans_five_bytes() {
        // Worst case: off = 3, n = 32 → 5 bytes gathered, accumulator holds 40
        // bits. Reference value: bits [3..35) of 0xABCDEF0123 (40-bit BE) =
        // (0xABCDEF0123 >> 5) & 0xFFFFFFFF = 0x5E6F_7809.
        let mut r = BitReader::new(&[0xAB, 0xCD, 0xEF, 0x01, 0x23]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(32).unwrap(), 0x5E6F_7809);
    }

    // ---- peek_bits ----------------------------------------------------------

    #[test]
    fn peek_matches_read_and_never_moves_cursor() {
        // 0xA55A = 1010_0101_0101_1010. Peek a field, confirm read returns the
        // identical value and only read advances the cursor.
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        for _ in 0..3 {
            // Idempotent: repeated peeks are identical...
            assert_eq!(r.peek_bits(7).unwrap(), 0b101_0010); // top 7 bits of 0xA5
            assert_eq!(r.bit_position(), 0); // ...and never move the cursor
        }
        assert_eq!(r.read_bits(7).unwrap(), 0b101_0010);
        assert_eq!(r.bit_position(), 7);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(5).unwrap(), 0b10101); // bits [7..12)
            assert_eq!(r.bit_position(), 7);
        }
        assert_eq!(r.read_bits(5).unwrap(), 0b10101);
        assert_eq!(r.bit_position(), 12);
    }

    #[test]
    fn peek_validates_widths_like_read() {
        // `&self` receiver: this whole test needs no mutable cursor at all.
        let r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.peek_bits(0), Err(Error::InvalidField));
        assert_eq!(r.peek_bits(33), Err(Error::InvalidField));
        assert_eq!(r.peek_bits(u32::MAX), Err(Error::InvalidField));
        assert_eq!(r.bit_position(), 0);
        // Legal widths still work after the rejections.
        assert_eq!(r.peek_bits(32).unwrap(), 0xFFFF_FFFF);
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn peek_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.peek_bits(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.peek_bits(5).unwrap(), 0b0_1010); // exactly what remains
        assert_eq!(r.bit_position(), 3);
        // Drain to the end: peek fails at EOF but never before the last read.
        assert_eq!(r.read_bits(5).unwrap(), 0b0_1010);
        assert_eq!(r.peek_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn peek_32_bits_unaligned_covers_worst_case_span() {
        // Same five-byte worst case as the wide-read test (off = 3, n = 32),
        // but via peek: value must match and cursor must not move.
        let mut r = BitReader::new(&[0xAB, 0xCD, 0xEF, 0x01, 0x23]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(32).unwrap(), 0x5E6F_7809);
            assert_eq!(r.bit_position(), 3);
        }
        assert_eq!(r.read_bits(32).unwrap(), 0x5E6F_7809);
    }

    #[test]
    fn peek_agrees_with_oracle_at_every_position() {
        // Differential sweep over every absolute bit position (alignment is
        // where peek bugs live): peek must equal the oracle bits assembled
        // MSB-first, at every alignment, for every legal width.
        let mut rng = Lcg(0xCAFE_F00D);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                let pos = start - (byte_start * 8); // cursor within subslice
                match r.peek_bits(width as u32) {
                    Ok(val) => {
                        assert_eq!(
                            r.bit_position(),
                            pos,
                            "peek must not move cursor: start {start}, width {width}"
                        );
                        let mut expect = 0u32;
                        for j in 0..width {
                            expect = (expect << 1) | ref_bit(&data, start + j);
                        }
                        assert_eq!(val, expect, "start {start}, width {width}");
                    }
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }

    // ---- read_signed --------------------------------------------------------

    /// Independent oracle: assemble `w` bits MSB-first from the naive bit
    /// source, then sign-extend by **definition** — if the sign bit is set,
    /// subtract 2^w (in `i64`, so `w == 32` cannot overflow). A genuinely
    /// *different mechanism* than the shl+arith-shr implementation in
    /// `read_signed`, so a shared bug is unlikely.
    fn ref_signed(data: &[u8], start: usize, w: usize) -> i32 {
        let mut raw = 0u32;
        for j in 0..w {
            raw = (raw << 1) | ref_bit(data, start + j);
        }
        if raw & (1 << (w - 1)) == 0 {
            raw as i32 // sign bit clear: the raw bits *are* the value
        } else {
            (raw as i64 - (1i64 << w)) as i32 // raw - 2^w, the two's-complement definition
        }
    }

    #[test]
    fn read_signed_hand_computed_fields() {
        // 0xF1 0x23 = 1111_0001_0010_0011, read whole (16 bits, exact fit).
        let mut r = BitReader::new(&[0xF1, 0x23]);
        assert_eq!(r.read_signed(4).unwrap(), -1); // 1111 → -1
        assert_eq!(r.read_signed(4).unwrap(), 1); // 0001 → +1
        assert_eq!(r.read_signed(5).unwrap(), 4); // 00100 → +4 (sign bit 0)
        assert_eq!(r.read_signed(3).unwrap(), 3); // 011 → +3 (sign bit 0)
        assert_eq!(r.bits_remaining(), 0);
    }

    #[test]
    fn read_signed_unaligned_crosses_byte_boundary() {
        // Chosen so bits [3..15) are exactly 1111_1111_1001 (= -7 in 12-bit
        // two's complement): byte0 = 101_11111, byte1 = 1111_001_0.
        // 0xBF 0xF2 = 1011_1111_1111_0010.
        let mut r = BitReader::new(&[0xBF, 0xF2]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101); // re-align to bit 3
        assert_eq!(r.read_signed(12).unwrap(), -7); // bits [3..15) = 1111_1111_1001
    }

    #[test]
    fn read_signed_extremes_per_width() {
        // All-ones reads as -1 at every width (shl puts a one in bit 31, the
        // arithmetic shr fills the whole high half with ones → all-ones i32).
        // Consecutive widths 1..=32 consume 528 bits → 66 bytes exactly.
        let mut r = BitReader::new(&[0xFF; 66]);
        for n in 1..=32u32 {
            assert_eq!(r.read_signed(n).unwrap(), -1, "width {n}");
        }
        assert_eq!(r.bit_position(), (1..=32u32).sum::<u32>() as usize); // 528 bits

        // ...and the minimum negative (1000…₂) at every width is i32::MIN >> (32-n).
        for n in 1..=32usize {
            let bytes = [0x80, 0, 0, 0, 0]; // bit 0 = 1, rest 0
            let mut r = BitReader::new(&bytes);
            let want = (i32::MIN >> (32 - n)) as i64;
            assert_eq!(r.read_signed(n as u32).unwrap() as i64, want, "width {n}");
        }
    }

    #[test]
    fn read_signed_32_bit_boundary() {
        // n == 32 takes the plain `as i32` branch — the one width the shift
        // trick cannot handle (n - 1 == 31 would misplace the flip).
        let mut r = BitReader::new(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.read_signed(32).unwrap(), -1);
        let mut r = BitReader::new(&[0x80, 0, 0, 0]);
        assert_eq!(r.read_signed(32).unwrap(), i32::MIN);
        let mut r = BitReader::new(&[0x7F, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.read_signed(32).unwrap(), i32::MAX);
    }

    #[test]
    fn read_signed_validates_widths_like_read() {
        let mut r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.read_signed(0), Err(Error::InvalidField));
        assert_eq!(r.read_signed(33), Err(Error::InvalidField));
        assert_eq!(r.read_signed(u32::MAX), Err(Error::InvalidField));
        assert_eq!(r.bit_position(), 0); // rejected reads must not move the cursor
    }

    #[test]
    fn read_signed_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1111_1111]);
        assert_eq!(r.read_signed(3).unwrap(), -1);
        assert_eq!(r.read_signed(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.read_signed(5).unwrap(), -1); // exact fit on the last bits
        assert_eq!(r.bit_position(), 8);
        assert_eq!(r.read_signed(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn read_signed_agrees_with_unsigned_plus_oracle() {
        // Differential sweep over every absolute bit position × width:
        //  1. read_signed must equal read_bits on the same bits, sign-extended
        //     with the shl+shr idiom (a different mechanism from the impl),
        //  2. and match the independent assembled-oracle value.
        //  3. and neither read may disturb the other's cursor rules.
        let mut rng = Lcg(0x51ED_31ED);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                // Fresh reader per probe: the cursor only moves forward.
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                let pos = start - (byte_start * 8); // cursor within subslice

                // Reference values from the unsigned path + independent oracle.
                let unsigned_expect = {
                    let mut u = BitReader::new(&data[byte_start..]);
                    if pad > 0 {
                        let _ = u.read_bits(pad as u32).unwrap();
                    }
                    match u.read_bits(width as u32) {
                        Ok(v) => v,
                        Err(Error::EndOfStream) => {
                            assert!(start + width > total);
                            continue;
                        }
                        Err(e) => panic!("unsigned read failed: {e:?}"),
                    }
                };
                // shl+arith-shr idiom, written out here from the *unsigned*
                // read: move the field's sign bit to bit 31, drag it down.
                let sign_extended =
                    (((unsigned_expect << (32 - width)) as i32) >> (32 - width)) as i64;
                let oracle = ref_signed(&data, start, width) as i64;

                match r.read_signed(width as u32) {
                    Ok(val) => {
                        assert_eq!(val as i64, sign_extended, "start {start}, width {width}");
                        assert_eq!(val as i64, oracle, "start {start}, width {width}");
                        assert_eq!(r.bit_position(), pad + width, "cursor must advance");
                    }
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }

    // ---- read_utf8_coded -----------------------------------------------------

    /// Independent encoder for roundtrip tests: builds the canonical
    /// (shortest-form) octet sequence for a value by Table 18 magnitude
    /// boundaries — construction-by-shifting-from-the-top, a deliberately
    /// different mechanism from the decode-under-test (which walks
    /// prefix → continuations), so a shared bug is unlikely.
    ///
    /// Table 18 boundaries (RFC 9639 §9.1.5): total octets and lead base per
    /// magnitude class.
    fn encode_coded(v: u64) -> ([u8; 7], usize) {
        const FORMS: [(u64, u8, u32); 7] = [
            (0x7F, 0x00, 1),           // 0xxxxxxx
            (0x7FF, 0xC0, 2),          // 110xxxxx
            (0xFFFF, 0xE0, 3),         // 1110xxxx
            (0x1F_FFFF, 0xF0, 4),      // 11110xxx
            (0x3FF_FFFF, 0xF8, 5),     // 111110xx: 2+24 = 26 payload bits
            (0x7F_FF_FF_FF, 0xFC, 6),  // 1111110x
            (0xF_FFFF_FFFF, 0xFE, 7),  // 11111110 + 6 continuations
        ];
        let mut out = [0u8; 7];
        for &(max, base, t) in FORMS.iter() {
            if v <= max {
                // Payload below the lead byte; t == 1 shifts by 0 (identity,
                // and v ≤ 0x7F fits the lead payload exactly).
                out[0] = base | (v >> (6 * (t - 1))) as u8;
                for i in 1..t {
                    out[i as usize] = 0x80 | ((v >> (6 * (t - 1 - i))) & 0x3F) as u8;
                }
                return (out, t as usize);
            }
        }
        panic!("{v:#x} exceeds the 36-bit coded-number range");
    }

    #[test]
    fn read_utf8_coded_single_byte_identity() {
        // 0xxxxxxx is the identity form: every one-byte lead decodes to
        // itself, cursor lands on the next byte.
        for lead in 0x00u8..=0x7F {
            let data = [lead];
            let mut r = BitReader::new(&data);
            assert_eq!(r.read_utf8_coded().unwrap(), u64::from(lead), "{lead:#04X}");
            assert_eq!(r.bit_position(), 8);
        }
    }

    #[test]
    fn read_utf8_coded_rfc_table_vectors() {
        // Every form class from the RFC 9639 Table 18 shape, at minimum,
        // mid, and maximum where distinct — expectations derived by hand
        // from the bit layout (payload bits concatenated MSB-first) and
        // cross-checked against an independent spec-derived oracle script.
        // Includes the RFC §9.1.5 worked example (51 billion).
        let cases: &[(&[u8], u64)] = &[
            (&[0x00], 0),
            (&[0x7F], 127),
            (&[0xC2, 0x80], 0x80),
            (&[0xDF, 0xBF], 0x7FF),
            (&[0xE0, 0xA0, 0x80], 0x800),
            (&[0xEF, 0xBF, 0xBF], 0xFFFF),
            (&[0xF0, 0x90, 0x80, 0x80], 0x1_0000),
            (&[0xF1, 0x80, 0x80, 0x80], 0x4_0000), // lead payload 0b0001 << 18
            (&[0xF7, 0xBF, 0xBF, 0xBF], 0x1F_FFFF),
            (&[0xF8, 0x88, 0x80, 0x80, 0x80], 0x20_0000),
            (&[0xFB, 0xBF, 0xBF, 0xBF, 0xBF], 0x3_FFFF_FF), // 26-bit max of the 5-byte form
            (&[0xFC, 0x84, 0x80, 0x80, 0x80, 0x80], 0x400_0000),
            (&[0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF], 0x7F_FF_FF_FF),
            // 7-byte lead carries ZERO payload bits:
            (&[0xFE, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80], 0), // non-canonical zero
            (&[0xFE, 0x82, 0x80, 0x80, 0x80, 0x80, 0x80], 0x8000_0000), // 2^31, table minimum
            (&[0xFE, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF], 0xF_FFFF_FFFF), // 36-bit max
            (&[0xFE, 0xAF, 0x9F, 0xB5, 0xA3, 0xB8, 0x80], 51_000_000_000), // RFC worked example
        ];
        for &(bytes, want) in cases {
            let mut r = BitReader::new(bytes);
            assert_eq!(r.read_utf8_coded().unwrap(), want, "{bytes:02X?}");
            // Whole number consumed exactly one byte per octet.
            assert_eq!(r.bit_position(), bytes.len() * 8, "{bytes:02X?}");
        }
    }

    #[test]
    fn read_utf8_coded_rejects_invalid_forms_cursor_untouched() {
        // Stray continuation leads (10xxxxxx), the all-ones lead (0xFF), and
        // broken continuation bytes are all InvalidField — and because the
        // read is composite, the cursor must be fully restored in every case
        // (nothing before the failure point stays consumed).
        let cases: &[&[u8]] = &[
            &[0x80],             // stray continuation lead
            &[0xBF],             // stray continuation lead (upper)
            &[0xFF],             // prefix == 8: the invalid all-ones lead
            &[0xC2, 0x00],       // second octet is not 10xxxxxx
            &[0xC2, 0xC0],       // continuation bit pattern broken (11…)
            &[0xDF, 0x3F],       // continuation bit pattern broken (00…)
            &[0xE0, 0xA0, 0x00], // third octet not a continuation
        ];
        for &bytes in cases {
            let mut r = BitReader::new(bytes);
            assert_eq!(r.read_utf8_coded(), Err(Error::InvalidField), "{bytes:02X?}");
            assert_eq!(r.bit_position(), 0, "atomic: {bytes:02X?}");
        }
        // Every stray-continuation lead byte at all rejects before consuming
        // any continuation.
        for lead in 0x80u8..0xC0 {
            let data = [lead, 0x80];
            let mut r = BitReader::new(&data);
            assert_eq!(r.read_utf8_coded(), Err(Error::InvalidField), "{lead:#04X}");
            assert_eq!(r.bit_position(), 0);
        }
    }

    #[test]
    fn read_utf8_coded_truncation_restores_cursor() {
        // Mid-number EOF is the case atomicity exists for: a 6-byte form cut
        // to 5 octets must leave the cursor exactly where it started.
        let truncated = [0xFC, 0xBF, 0xBF, 0xBF, 0xBF]; // needs 6 octets, has 5
        let mut r = BitReader::new(&truncated);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
        // Same bytes + the missing octet → succeeds; the failed attempt
        // consumed nothing.
        // 0xFC lead carries ONE payload bit (0 here), five continuations
        // carry 30 → 0x3F_FF_FF_FF, not the 0x7F… of the 0xFD lead.
        let full = [0xFC, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF];
        let mut r = BitReader::new(&full);
        assert_eq!(r.read_utf8_coded().unwrap(), 0x3F_FF_FF_FF);
        assert_eq!(r.bit_position(), 48);
        // 7-byte lead at EOF after one byte: rejection of an incomplete
        // number never strands the cursor mid-number.
        let mut r = BitReader::new(&[0xFE]);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_utf8_coded_two_byte_form_exhaustive() {
        // All 32×64 legal two-byte pairs against the by-definition value
        // ((lead & 0x1F) << 6) | (cont & 0x3F) — no table lookup involved.
        for lead in 0xC0u8..0xE0 {
            for cont in 0x80u8..0xC0 {
                let data = [lead, cont];
                let mut r = BitReader::new(&data);
                let want =
                    (u64::from(lead & 0x1F) << 6) | u64::from(cont & 0x3F);
                assert_eq!(r.read_utf8_coded().unwrap(), want, "{lead:02X} {cont:02X}");
                assert_eq!(r.bit_position(), 16);
            }
        }
    }

    #[test]
    fn read_utf8_coded_sequential_numbers_pack_back_to_back() {
        // Mixed widths concatenated: 1 + 2 + 7 + 1 octets. The frame header
        // read path (coded number after fixed-width fields) lives or dies
        // with sequential cursor bookkeeping.
        let data = [
            0x00,                                                             // 0
            0xC2, 0x80,                                                       // 0x80
            0xFE, 0xAF, 0x9F, 0xB5, 0xA3, 0xB8, 0x80,                         // 51e9
            0x7F,                                                             // 127
        ];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_utf8_coded().unwrap(), 0);
        assert_eq!(r.read_utf8_coded().unwrap(), 0x80);
        assert_eq!(r.read_utf8_coded().unwrap(), 51_000_000_000);
        assert_eq!(r.read_utf8_coded().unwrap(), 127);
        assert_eq!(r.bit_position(), data.len() as usize * 8);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), data.len() as usize * 8);
    }

    #[test]
    fn read_utf8_coded_roundtrips_at_every_bit_alignment() {
        // Differential sweep: canonical encodings (independent encoder
        // above) placed at every bit alignment 0..7, decoded back. Covers
        // the unaligned 8-bit reads inside the composite decode — where a
        // refill/cursor bug would live — for every form width.
        let boundaries: &[u64] = &[
            0, 1, 0x7E, 0x7F, 0x80, 0x7FE, 0x7FF,
            0x800, 0xFFFE, 0xFFFF,
            0x1_0000, 0x1F_FFFE, 0x1F_FFFF,
            0x20_0000, 0x3_FFFF_FE, 0x3_FFFF_FF,
            0x400_0000, 0x7F_FF_FF_FE, 0x7F_FF_FF_FF,
            0x8000_0000, 0xF_FFFF_FFFE, 0xF_FFFF_FFFF,
        ];
        let mut rng = Lcg(0xFEED_C0DE);
        let mut samples: [u64; 64 + 22] = [0; 64 + 22];
        let mut n = 0usize;
        for &b in boundaries {
            samples[n] = b;
            n += 1;
        }
        while n < samples.len() {
            let mut v = 0u64;
            for _ in 0..5 {
                v = (v << 8) | u64::from(rng.next_u8());
            }
            samples[n] = v & 0xF_FFFF_FFFF; // clamp into the 36-bit range
            n += 1;
        }

        for &val in &samples[..n] {
            let (enc, len) = encode_coded(val);
            for pad in 0..8usize {
                // Place the number at ABSOLUTE bit `pad`: pack `pad` filler
                // bits then the enc octets bit-by-bit MSB-first. (A byte-copy
                // would put the field at bit 8, not bit `pad` — an early version of
                // this harness read filler at pad == 0 and failed.)
                let total_bits = pad + len * 8;
                let mut data = [0u8; 16];
                for i in 0..total_bits {
                    // Both branches yield an unshifted 0/1 bit; the shift into
                    // the byte happens once here. (An early version pre-shifted
                    // only the filler branch — number bits then landed at the
                    // wrong bit position for any pad > 0.)
                    let bit = if i < pad {
                        // Arbitrary but non-constant filler: a run of zeros
                        // here could mask an off-by-one into the padding.
                        (i as u8 * 37 + 11) & 1
                    } else {
                        let j = i - pad;
                        (enc[j >> 3] >> (7 - (j & 7))) & 1
                    };
                    data[i >> 3] |= bit << (7 - (i & 7));
                }
                let mut r = BitReader::new(&data);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap(); // reach the alignment
                }
                let before = r.bit_position();
                assert_eq!(
                    r.read_utf8_coded().unwrap(),
                    val,
                    "val {val:#x}, pad {pad}, enc {:?}",
                    &enc[..len]
                );
                assert_eq!(r.bit_position(), before + len * 8);
            }
        }
    }

    // ---- position bookkeeping ----------------------------------------------

    #[test]
    fn position_and_remaining_track_reads() {
        let mut r = BitReader::new(&[0u8; 2]); // 16 bits
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.bits_remaining(), 16);
        let _ = r.read_bits(5).unwrap();
        assert_eq!(r.bit_position(), 5);
        assert_eq!(r.bits_remaining(), 11);
        let _ = r.read_bits(11).unwrap();
        assert_eq!(r.bit_position(), 16);
        assert_eq!(r.bits_remaining(), 0);
    }

    // ---- differential: bit-by-bit vs wide reads vs naive oracle ------------

    #[test]
    fn wide_reads_agree_with_bit_by_bit_on_random_data() {
        let mut rng = Lcg(0xDEAD_BEEF);
        for trial in 0..64 {
            let len = 1 + (rng.next_u8() as usize % 8); // 1..=8 bytes
            let mut data = [0u8; 8];
            for d in data.iter_mut().take(len) {
                *d = rng.next_u8();
            }
            let data = &data[..len];
            let total = len * 8;

            // Oracle: every bit, from the independent ref_bit implementation.
            let mut bit_r = BitReader::new(data);
            for i in 0..total {
                assert_eq!(
                    bit_r.read_bits(1).unwrap(),
                    ref_bit(data, i),
                    "trial {trial}: bit {i} of {data:02X?}"
                );
            }

            // Wide reads (odd widths hit nasty alignments) must equal the
            // same bits assembled one at a time.
            for width in [7u32, 13, 31, 32] {
                let mut wide_r = BitReader::new(data);
                let mut bits_r = BitReader::new(data);
                while wide_r.bits_remaining() >= width as usize {
                    let wide = wide_r.read_bits(width).unwrap();
                    let mut assembled = 0u32;
                    for _ in 0..width {
                        assembled = (assembled << 1) | bits_r.read_bits(1).unwrap();
                    }
                    assert_eq!(wide, assembled, "trial {trial}: width {width}");
                    assert_eq!(wide_r.bit_position(), bits_r.bit_position());
                }
                assert_eq!(wide_r.bits_remaining(), bits_r.bits_remaining());
            }
        }
    }

    #[test]
    fn unaligned_sweeps_cover_every_bit_offset() {
        // For every absolute start bit position and every legal width, check
        // the read against the independent oracle. Exhaustive over
        // (alignment x width) — the space where refill/alignment bugs live.
        // Reaching an arbitrary bit position uses forward reads only (there is
        // no Seek): a byte-aligned subslice plus a `pad`-bit skip.
        let mut rng = Lcg(0x1234_5678);
        let mut data = [0u8; 16];
        for (i, d) in data.iter_mut().enumerate() {
            *d = rng.next_u8() ^ ((i as u8) << 3);
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                match r.read_bits(width as u32) {
                    Ok(val) => {
                        assert_eq!(r.bit_position(), pad + width);
                        let mut expect = 0u32;
                        for j in 0..width {
                            expect = (expect << 1) | ref_bit(&data, start + j);
                        }
                        assert_eq!(val, expect, "start {start}, width {width}");
                    }
                    // Only legitimate failure: fewer than `width` bits left
                    // from `start` to the end of the slice.
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected error at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }
}
