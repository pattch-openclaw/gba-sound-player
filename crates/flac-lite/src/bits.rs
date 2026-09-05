//! MSB-first bit reader over an immutable byte slice.
//!
//! STATUS (2026-09-04): **core read path implemented** — `new`, `read_bits`,
//! `bit_position`, `bits_remaining` (roadmap Step 1, FLAC.md "Next steps").
//! The remaining methods (`peek_bits`, `read_signed`, `read_utf8_coded`,
//! `byte_align`, `read_u8`, CRCs) are still scaffold (`todo!()`).
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
    pub fn peek_bits(&mut self, n: u32) -> Result<u32> {
        todo!("flac-lite scaffold: BitReader::peek_bits (Step 2 — trivially wraps peek_at)")
    }

    /// Read `n` bits (1..=32) as a two's-complement signed value.
    pub fn read_signed(&mut self, n: u32) -> Result<i32> {
        todo!("flac-lite scaffold: BitReader::read_signed")
    }

    /// Read the UTF-8-style "zero-padded" number used for frame/sample numbers
    /// and channel assignments (RFC 9629 §5.1.4.1 / §7.2).
    ///
    /// Leading zero bits encode the width of the following value. A value with
    /// all-ones prefix (`1111111`) is an invalid/reserved doubled prefix.
    pub fn read_utf8_coded(&mut self) -> Result<u64> {
        todo!("flac-lite scaffold: BitReader::read_utf8_coded")
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
