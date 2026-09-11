//! MSB-first bit reader over an immutable byte slice.
//!
//! STATUS (2026-09-10): **the read path is complete** — `new`, `read_bits`,
//! `peek_bits`, `read_signed`, `read_utf8_coded`, `byte_align`, `read_u8`,
//! `read_wasted_bits`, `bit_position`, `bits_remaining` (Phase 1 step 1,
//! complete in PRs #25–#32; `read_wasted_bits` landed with step 3a as the
//! subframe body's wasted-bits reader). Only the CRC helpers (`crc8`, `crc16`)
//! remain `todo!()` scaffold: deferred to Phase 2 step 5, because the perf
//! gate runs with CRC verification skipped.
//! The roadmap itself lives in FLAC.md → "Phased plan" (single source of truth).
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
    ///
    /// FLAC pads the end of a frame's subframe data to a byte boundary before
    /// the CRC-16 footer, and verbatim / raw-signature subframe samples start on
    /// one — both consumers call this first.
    ///
    /// Notably **not** a caller: the frame header's CRC-8. The header's fixed
    /// fields total **32** bits (sync 14 + reserved 1 + blocking 1 + blocksize 4
    /// + sample rate 4 + **channels 4** + sample size 3 + reserved 1), and every
    /// field before the CRC is whole octets, so the CRC-8 always lands on a byte
    /// boundary — bit 40 behind a 1-octet coded number, 48 behind 2, and still
    /// aligned after the optional uncommon blocksize/rate fields (8/16/24 bits).
    /// Aligning there is a no-op.
    ///
    /// Earlier revision of this comment claimed 31 fixed bits with a 3-bit
    /// `channels` field and concluded the header CRC was *never* byte-aligned,
    /// so aligning would drop the CRC's top bit. Both the width and the
    /// conclusion were wrong (measured over 979 real libFLAC frames; pinned by
    /// `tests/frame_header_layout.rs`). [`Self::read_u8`] stays bit-level for the
    /// case that genuinely needs it — the **footer** read below, right after
    /// subframe padding — not the header. FLAC.md → "Correction 1".
    ///
    /// **Infallible, and it cannot overrun the slice**: a slice's bit count is
    /// always a multiple of 8, so aligning any in-bounds cursor lands on a
    /// boundary that also fits. An already-aligned cursor discards 0 and stays
    /// put, which makes the call idempotent.
    ///
    /// Division-free per the module's ARMv4T rule: `(8 - (pos & 7)) & 7` is the
    /// distance to the next boundary with no modulo — an aligned position puts
    /// 8 in the subtraction and the mask folds it back to 0.
    pub fn byte_align(&mut self) -> u32 {
        let drop = (8 - (self.bit_pos & 7)) & 7;
        self.bit_pos += drop;
        drop as u32
    }

    /// Read a `u8` (8 bits) — the frame header's CRC-8 byte, the two bytes of a
    /// CRC-16 footer, and byte-aligned metadata.
    ///
    /// Delegates to [`Self::read_bits`], so EOF behaviour and cursor rules are
    /// identical **by construction** (the same delegation as
    /// [`Self::read_signed`]): a short read yields [`Error::EndOfStream`] and
    /// leaves the cursor untouched; `read_bits(8)` cannot return anything but
    /// the low 8 bits, so the `as u8` truncation is lossless.
    ///
    /// Like the rest of the module this is **bit-level**: it neither requires
    /// nor assumes a byte-aligned cursor. That is load-bearing, not laxness —
    /// the frame header's CRC-8 sits at bit `31 + 8k` (see
    /// [`Self::byte_align`]), so an alignment *requirement* would make the real
    /// header unreadable. Where a field genuinely is byte-aligned (padding
    /// before the CRC-16 footer) the caller contracts to call
    /// [`Self::byte_align`] first; the reader keeps no alignment invariant, so
    /// asserting one here could only ever be a tripwire, and the layer that can
    /// actually check alignment is the frame parser.
    pub fn read_u8(&mut self) -> Result<u8> {
        self.read_bits(8).map(|v| v as u8)
    }

    /// Read FLAC's wasted-bits-per-sample field (§9.2.2), returning `k`.
    ///
    /// Layout: a flag bit. `0` → `k = 0`, nothing follows. `1` → `k − 1`
    /// zero bits terminated by a one follow, so `k ≥ 1`. The caller must
    /// sit on the flag bit — exactly where
    /// [`crate::subframe::SubframeType::parse`] leaves the cursor (its 7-bit
    /// pad+type contract), witnessed against encoder bytes in
    /// `tests/subframe_header_layout.rs`.
    ///
    /// Deliberately **bit-level**: no interpretation of what `k` means. The
    /// §9.2.2 rule "resulting bits per sample MUST be larger than zero" is
    /// `k < frame bps`, which needs the frame's sample size — that check
    /// belongs to the subframe layer, not here (same separation as
    /// profile-gating everywhere else).
    ///
    /// Cursor is **atomic** like [`Self::read_utf8_coded`]: a run that
    /// reaches EOF unterminated restores the cursor to the flag bit and
    /// returns [`Error::EndOfStream`]; on success the cursor sits just past
    /// the terminating one. The run is bounded by the slice, so no separate
    /// length cap is imposed — an unbounded unary is exactly what EOF
    /// rejects.
    pub fn read_wasted_bits(&mut self) -> Result<u32> {
        let start = self.bit_pos;
        match self.wasted_inner() {
            Ok(k) => Ok(k),
            Err(e) => {
                self.bit_pos = start;
                Err(e)
            }
        }
    }

    fn wasted_inner(&mut self) -> Result<u32> {
        if self.read_bits(1)? == 0 {
            return Ok(0);
        }
        // Flag set: count the unary zeros; the terminating one is consumed.
        // Failure inside the loop propagates through the atomic wrapper.
        let mut zeros: u32 = 0;
        loop {
            if self.read_bits(1)? == 1 {
                return Ok(zeros + 1);
            }
            zeros += 1;
        }
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
            (0x7F, 0x00, 1),          // 0xxxxxxx
            (0x7FF, 0xC0, 2),         // 110xxxxx
            (0xFFFF, 0xE0, 3),        // 1110xxxx
            (0x1F_FFFF, 0xF0, 4),     // 11110xxx
            (0x3FF_FFFF, 0xF8, 5),    // 111110xx: 2+24 = 26 payload bits
            (0x7F_FF_FF_FF, 0xFC, 6), // 1111110x
            (0xF_FFFF_FFFF, 0xFE, 7), // 11111110 + 6 continuations
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
            assert_eq!(
                r.read_utf8_coded(),
                Err(Error::InvalidField),
                "{bytes:02X?}"
            );
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
                let want = (u64::from(lead & 0x1F) << 6) | u64::from(cont & 0x3F);
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
            0x00, // 0
            0xC2, 0x80, // 0x80
            0xFE, 0xAF, 0x9F, 0xB5, 0xA3, 0xB8, 0x80, // 51e9
            0x7F, // 127
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
            0,
            1,
            0x7E,
            0x7F,
            0x80,
            0x7FE,
            0x7FF,
            0x800,
            0xFFFE,
            0xFFFF,
            0x1_0000,
            0x1F_FFFE,
            0x1F_FFFF,
            0x20_0000,
            0x3_FFFF_FE,
            0x3_FFFF_FF,
            0x400_0000,
            0x7F_FF_FF_FE,
            0x7F_FF_FF_FF,
            0x8000_0000,
            0xF_FFFF_FFFE,
            0xF_FFFF_FFFF,
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

    // ---- byte_align ----------------------------------------------------------

    #[test]
    fn byte_align_drops_the_expected_bits_at_every_alignment() {
        // Walk pad = 0..7, then align. Expected drop written independently of
        // the impl's mask: `if pad == 0 { 0 } else { 8 - pad }`.
        let data = [0b1111_0001, 0b0000_1110, 0b1010_1010, 0xFF];
        for pad in 0..8usize {
            let mut r = BitReader::new(&data);
            if pad > 0 {
                let _ = r.read_bits(pad as u32).unwrap();
            }
            let want_drop = if pad == 0 { 0 } else { 8 - pad };
            assert_eq!(r.byte_align() as usize, want_drop, "pad {pad}");
            assert_eq!(r.bit_position(), pad + want_drop, "pad {pad}");
            assert_eq!(r.bit_position() % 8, 0, "pad {pad}: must land aligned");
            assert_eq!(r.bits_remaining(), data.len() * 8 - (pad + want_drop));
        }
    }

    #[test]
    fn byte_align_is_idempotent_on_an_aligned_cursor() {
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        // Bit 0 is a byte boundary: no discard, no movement.
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.byte_align(), 5);
        assert_eq!(r.bit_position(), 8);
        // Repeated calls on an aligned cursor are exact no-ops.
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn byte_align_returns_bits_dropped_not_the_new_position() {
        // Mid-slice at bit 11: the next boundary is bit 16, so the return is 5.
        // An implementation returning the absolute position would say 16.
        let mut r = BitReader::new(&[0u8; 4]);
        assert_eq!(r.read_bits(11).unwrap(), 0);
        assert_eq!(r.byte_align(), 5);
        assert_eq!(r.bit_position(), 16);
    }

    #[test]
    fn byte_align_never_moves_past_the_end_of_the_slice() {
        // Exhaustive over slice length × every reachable bit position. Reaching
        // an arbitrary position uses forward reads only (there is no Seek); 13
        // is an odd chunk width, so positions are hit at every alignment. The
        // invariant under test: aligning an in-bounds cursor stays in bounds —
        // and at EOF (already aligned) it is an exact no-op.
        // Zero-filled: `byte_align` is data-independent, and asserting the
        // skipped reads return 0 keeps them honest without an expectation that
        // depends on the bit pattern's alignment.
        for len in 0..=8usize {
            let all = [0u8; 8];
            let data = &all[..len];
            let total = len * 8;
            for pos in 0..=total {
                let mut r = BitReader::new(data);
                let mut left = pos;
                while left > 0 {
                    let chunk = left.min(13);
                    assert_eq!(r.read_bits(chunk as u32).unwrap(), 0);
                    left -= chunk;
                }
                let before = r.bit_position();
                let want_drop = if before % 8 == 0 { 0 } else { 8 - before % 8 };
                let drop = r.byte_align();
                assert!(drop <= 7, "len {len}, pos {pos}: drop {drop}");
                assert_eq!(drop as usize, want_drop, "len {len}, pos {pos}");
                assert_eq!(r.bit_position(), before + drop as usize);
                assert!(r.bit_position() <= total, "len {len}, pos {pos}: overrun");
                assert_eq!(r.bit_position() % 8, 0);
            }
        }
    }

    // ---- read_u8 -------------------------------------------------------------

    #[test]
    fn read_u8_matches_read_bits_at_every_alignment() {
        // Differential sweep over every absolute bit position, including
        // deliberately unaligned ones: read_u8 must equal read_bits(8) cast
        // down, and match the naive bit-by-bit oracle. This is the test that
        // pins down "read_u8 is a plain 8-bit read, not an aligned-only path".
        let mut rng = Lcg(0xB01F_00D1);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..=(total - 8) {
            let byte_start = start >> 3;
            let pad = start & 7;
            let mut a = BitReader::new(&data[byte_start..]);
            let mut b = BitReader::new(&data[byte_start..]);
            if pad > 0 {
                let _ = a.read_bits(pad as u32).unwrap();
                let _ = b.read_bits(pad as u32).unwrap();
            }
            let want = b.read_bits(8).unwrap() as u8;
            let got = a.read_u8().unwrap();
            assert_eq!(got, want, "start {start}");
            assert_eq!(a.bit_position(), pad + 8, "start {start}");
            let mut oracle = 0u32;
            for j in 0..8 {
                oracle = (oracle << 1) | ref_bit(&data, start + j);
            }
            assert_eq!(u32::from(got), oracle, "start {start}");
        }
    }

    #[test]
    fn read_u8_returns_every_byte_value_in_stream_order() {
        // All 256 byte values, read back in sequence: catches a truncation or
        // bit-order slip that a single aligned read could miss, and pins the
        // cursor advancing exactly 8 bits per call.
        let data: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut r = BitReader::new(&data);
        for (i, &want) in data.iter().enumerate() {
            assert_eq!(r.read_u8().unwrap(), want, "byte {i}");
            assert_eq!(r.bit_position(), (i + 1) * 8);
        }
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.read_u8(), Err(Error::EndOfStream));
    }

    #[test]
    fn read_u8_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_u8(), Err(Error::EndOfStream)); // only 5 bits left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.read_bits(5).unwrap(), 0b0_1010); // exact fit to the end
        assert_eq!(r.read_u8(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn read_u8_agrees_with_peek_bits_unaligned() {
        // Mid-byte on purpose: peek(8) and read_u8 must agree on the same
        // unaligned field, and only read moves the cursor.
        // 0x36 0xC3 = 0011_0110 1100_0011; bits [5..13) = 1101_1000 = 0xD8.
        let mut r = BitReader::new(&[0x36, 0xC3]);
        assert_eq!(r.read_bits(5).unwrap(), 0b0011_0);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(8).unwrap(), 0xD8);
            assert_eq!(r.bit_position(), 5);
        }
        assert_eq!(r.read_u8().unwrap(), 0xD8);
        assert_eq!(r.bit_position(), 13);
    }

    // ---- composition: the patterns the frame layer will actually use ---------

    #[test]
    fn unaligned_coded_number_then_byte_straddling_read() {
        // A SYNTHETIC unaligned composition. Its job is the bit reader: put a
        // UTF-8 coded number mid-byte, then read 8 bits that straddle two
        // source bytes.
        //
        // This is deliberately NOT billed as a FLAC frame header any more. Real
        // headers keep the coded number and the header CRC-8 byte-aligned (the
        // fixed fields sum to 32 bits — see the `byte_align` docs, and
        // `tests/frame_header_layout.rs`, which parses real libFLAC bytes).
        // What *is* real and unaligned is a footer read right after subframe
        // padding; that case lives in `footer_padding_pattern_...` below.
        //
        // History, kept so the mistake is not repeated: this test used to be
        // billed as "a faithful FLAC frame header" and asserted 31 fixed bits
        // with a 3-bit channels field, concluding the header CRC-8 is never
        // byte aligned. Both the width and the conclusion were wrong, and the
        // hand-packed bytes were not a valid header at all — parsed correctly
        // they decode as a 4-channel frame with reserved bits set. The lesson
        // generalises past this test: **a hand-packed vector cannot witness a
        // field-width claim; it only records what the author believed.** Real
        // encoder output is the only witness for that, so that is what
        // tests/frame_header_layout.rs uses.
        //
        // Bytes re-derived with an independent Python bit packer (the habit
        // that earlier caught sync 0x3FF8 being no sync code at all, and
        // blocksize code 9 being 512 rather than 2048):
        //   1011_1000 0001_0111 1011_0110 1100_0000
        //   ^^^ lead-in = 0b101, cursor now at bit 3
        //       ^^^^^ 11000000 = 2-octet lead at bit 3, payload 0b00001
        //                ^^^^^^ 10111101 continuation, payload 0b111101
        //                       -> coded number 0b00001_111101 = 61, bit 19
        //                          ^^^^^^^^ bits [19..27) = 0b10110110 = 0xB6,
        //                                   straddling data[2] and data[3]
        let data = [0xB8, 0x17, 0xB6, 0xC0];
        let mut r = BitReader::new(&data);

        assert_eq!(r.read_bits(3).unwrap(), 0b101, "synthetic lead-in");
        assert_eq!(r.bit_position(), 3);
        assert_ne!(r.bit_position() % 8, 0, "setup must be unaligned");

        assert_eq!(r.read_utf8_coded().unwrap(), 61, "coded number");
        assert_eq!(r.bit_position(), 19, "3 + 16 bits of coded number");
        assert_ne!(r.bit_position() % 8, 0, "still unaligned");

        assert_eq!(r.read_u8().unwrap(), 0xB6, "unaligned 8-bit read");
        assert_eq!(r.bit_position(), 27);
        assert_eq!(r.bits_remaining(), 5);
    }

    #[test]
    fn footer_padding_pattern_aligns_then_reads_the_next_byte() {
        // The other real pattern: subframe data ends mid-byte, padding bits pad
        // to the boundary, then the CRC-16 (two whole bytes) follows. Across
        // every padding width, `byte_align` + `read_u8` must reach the *whole*
        // next byte — never the field the padding bits came from.
        //
        // 0xAA 0x5C 0xA5: target byte is 0x5C. Hand-checked guard below — the
        // seven unaligned 8-bit reads are 0x54, 0xA9, 0x52, 0xA5, 0x4B, 0x97,
        // 0x2E, none of which is 0x5C, so a reader that skipped byte_align
        // could not pass this test.
        let data = [0xAA, 0x5C, 0xA5];
        for pad in 1..8usize {
            let mut r = BitReader::new(&data);
            let _ = r.read_bits(pad as u32).unwrap(); // mid-field padding bits
            assert_eq!(r.byte_align() as usize, 8 - pad, "pad {pad}");
            assert_eq!(r.bit_position(), 8, "pad {pad}");
            assert_eq!(r.read_u8().unwrap(), 0x5C, "pad {pad}");
            assert_eq!(r.read_u8().unwrap(), 0xA5, "pad {pad}: second CRC byte");
            assert_eq!(r.bit_position(), 24, "pad {pad}");

            // The counterfactual: no alignment step reads a different byte.
            let mut naive = BitReader::new(&data);
            let _ = naive.read_bits(pad as u32).unwrap();
            assert_ne!(naive.read_u8().unwrap(), 0x5C, "pad {pad}");
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

    // ---- read_wasted_bits --------------------------------------------------

    #[test]
    fn read_wasted_bits_flag_clear_consumes_only_the_flag() {
        let mut r = BitReader::new(&[0b0111_1111]);
        assert_eq!(r.read_wasted_bits().unwrap(), 0);
        assert_eq!(r.bit_position(), 1);
    }

    #[test]
    fn read_wasted_bits_hand_packed_runs() {
        // k=1: flag + terminator, no zeros: bits "11".
        let mut r = BitReader::new(&[0b1100_0000]);
        assert_eq!(r.read_wasted_bits().unwrap(), 1);
        assert_eq!(r.bit_position(), 2);
        // k=3: 1, 0, 0, 1 → "1001".
        let mut r = BitReader::new(&[0b1001_0000]);
        assert_eq!(r.read_wasted_bits().unwrap(), 3);
        assert_eq!(r.bit_position(), 4);
        // k=5: flag, k−1 = 4 zeros, terminator → bits 1,0,0,0,0,1 = 0b100001
        // from the MSB = 0b1000_0100.
        let mut r = BitReader::new(&[0b1000_0100]);
        assert_eq!(r.read_wasted_bits().unwrap(), 5);
        assert_eq!(r.bit_position(), 6);
        // k=8: flag, seven zeros, terminator → 9 bits, crossing into byte 2
        // (bit 8 is byte 1's MSB).
        let mut r = BitReader::new(&[0x80, 0x80]);
        assert_eq!(r.read_wasted_bits().unwrap(), 8);
        assert_eq!(r.bit_position(), 9);
    }

    #[test]
    fn read_wasted_bits_long_run() {
        // k=40: flag, 39 zeros, terminator → 41 bits. Byte 0 = 0x80, bytes
        // 1..5 all zero, bit 40 = byte 5's MSB. Wide runs stay the bits
        // layer's problem to read, not to judge (frame bps gates them).
        let mut r = BitReader::new(&[0x80, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(r.read_wasted_bits().unwrap(), 40);
        assert_eq!(r.bit_position(), 41);
    }

    #[test]
    fn read_wasted_bits_unterminated_run_restores_cursor() {
        // flag + zeros, no terminator before EOF.
        let mut r = BitReader::new(&[0x80, 0x00]);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
        // Mid-slice: pad three bits first (0x10's top three bits are 000);
        // the flag is bit 3 (0x10 = 0001_0000), then zeros run to EOF. A
        // failure must restore to three, not zero and not into the run.
        let mut r = BitReader::new(&[0x10, 0x00, 0x00]);
        assert_eq!(r.read_bits(3).unwrap(), 0b000);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 3);
        // Empty slice: the flag read itself fails.
        let mut r = BitReader::new(&[]);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_wasted_bits_matches_naive_oracle_at_every_alignment() {
        // Independent oracle: a plain bit-by-bit loop over `ref_bit`, run
        // from every start position; disagreement or a terminator beyond
        // the slice is a failure either way.
        let mut data = [0u8; 9];
        let mut rng = Lcg(0xCAFE_F00D);
        for d in data.iter_mut() {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;
        for start in 0..total {
            // Naive expectation, computed first.
            let mut want: Option<u32> = None;
            if ref_bit(&data, start) == 0 {
                want = Some(0);
            } else {
                let mut zeros = 0u32;
                let mut pos = start + 1;
                loop {
                    if pos >= total {
                        break;
                    }
                    if ref_bit(&data, pos) == 1 {
                        want = Some(zeros + 1);
                        break;
                    }
                    zeros += 1;
                    pos += 1;
                }
            }
            let mut r = BitReader::new(&data);
            r.bit_pos = start;
            match (want, r.read_wasted_bits()) {
                (Some(k), Ok(got)) => {
                    assert_eq!(got, k, "start {start}");
                    // flag(1) + (k−1) zeros + terminator(1): k+1 bits for k≥1,
                    // just the flag for k=0.
                    let consumed = if k == 0 { 1 } else { k + 1 };
                    assert_eq!(r.bit_position(), start + consumed as usize, "start {start}");
                }
                (None, Err(Error::EndOfStream)) => {
                    assert_eq!(
                        r.bit_position(),
                        start,
                        "start {start}: failed read must restore"
                    );
                }
                (want, got) => panic!("start {start}: oracle says {want:?}, reader {got:?}"),
            }
        }
    }
}
