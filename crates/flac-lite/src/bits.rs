//! MSB-first bit reader over an immutable byte slice.
//!
//! SCAFFOLD STATE: signatures only, bodies are `todo!()`.
//!
//! FLAC packs its fields MSB-first across byte boundaries, so the whole decoder
//! is built on this one primitive. Design notes for the implementation:
//!
//! - Accumulate into a `u32` buffer with a bit-count; refill 8 bits at a time
//!   (or 32 when available) from the slice. Reads are limited to `<= 32` bits.
//! - ARMv4T (ARM7TDMI) has **CLZ** but **no hardware divide** and no
//!   barrel-shifter-free multiply-accumulate for free — prefer shift/add and
//!   `leading_zeros()`-based refill tricks over anything multiplicative.
//! - Signed fields (LP coefficients, residuals, verbatim samples) are
//!   two's-complement in the field's stated width: sign-extend after reading.
//! - Verbatim / raw-signature subframes and metadata DIDs are byte-aligned:
//!   use [`BitReader::byte_align`], never assume alignment.

/// A cursor-based, borrow-only bit reader.
///
/// Zero-cost to construct, zero allocation, `Copy`-ish for cheap save/restore in
/// tests. There is no `Seek` — the cursor only moves forward.
#[derive(Clone, Copy, Debug)]
pub struct BitReader<'a> {
    /// Backing bytes (normally a subslice of a `&'static` ROM blob).
    data: &'a [u8],
    /// Read cursor, in bits from the start of `data`.
    bit_pos: usize,
    /// Refill buffer, MSB-aligned. Scaffold: layout not yet decided.
    buffer: u32,
    /// Number of valid bits currently held in `buffer`.
    bits_left: u32,
}

impl<'a> BitReader<'a> {
    /// Wrap a byte slice for bit-level reading.
    pub fn new(data: &'a [u8]) -> Self {
        todo!("flac-lite scaffold: BitReader::new")
    }

    /// Read `n` bits (1..=32) as an unsigned value.
    pub fn read_bits(&mut self, n: u32) -> crate::Result<u32> {
        todo!("flac-lite scaffold: BitReader::read_bits")
    }

    /// Peek at the next `n` bits (1..=32) without advancing the cursor.
    pub fn peek_bits(&mut self, n: u32) -> crate::Result<u32> {
        todo!("flac-lite scaffold: BitReader::peek_bits")
    }

    /// Read `n` bits (1..=32) as a two's-complement signed value.
    pub fn read_signed(&mut self, n: u32) -> crate::Result<i32> {
        todo!("flac-lite scaffold: BitReader::read_signed")
    }

    /// Read the UTF-8-style "zero-padded" number used for frame/sample numbers
    /// and channel assignments (RFC 9629 §5.1.4.1 / §7.2).
    ///
    /// Leading zero bits encode the width of the following value. A value with
    /// all-ones prefix (`1111111`) is an invalid/reserved doubled prefix.
    pub fn read_utf8_coded(&mut self) -> crate::Result<u64> {
        todo!("flac-lite scaffold: BitReader::read_utf8_coded")
    }

    /// Consume bits up to the next byte boundary. Returns the number of bits
    /// discarded (0..7).
    pub fn byte_align(&mut self) -> u32 {
        todo!("flac-lite scaffold: BitReader::byte_align")
    }

    /// Read a `u8` (assumes byte-aligned cursor; used for CRC-8 / padding).
    pub fn read_u8(&mut self) -> crate::Result<u8> {
        todo!("flac-lite scaffold: BitReader::read_u8")
    }

    /// Bits remaining in the backing slice from the current cursor.
    pub fn bits_remaining(&self) -> usize {
        todo!("flac-lite scaffold: BitReader::bits_remaining")
    }

    /// Current cursor position in bits (useful for frame-boundary assertions
    /// and for the CRC-8-over-header check, which needs the raw header bytes).
    pub fn bit_position(&self) -> usize {
        todo!("flac-lite scaffold: BitReader::bit_position")
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
