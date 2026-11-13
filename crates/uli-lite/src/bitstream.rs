//! MSB-first ULI2 bit reader over little-endian 16-bit stream halfwords.
//!
//! The reference reads four halfwords at a time so a GBA `LDRH` pipeline can
//! refill the 64-bit window. This port reads the same eight little-endian
//! bytes from a byte slice; the album parser guarantees the four padding
//! halfwords the reference relies on.

use crate::Error;

#[derive(Clone, Copy, Debug)]
pub(crate) struct BitReader<'a> {
    stream: &'a [u8],
    bitpos: u32,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(stream: &'a [u8], bitpos: u32) -> Self {
        Self { stream, bitpos }
    }

    pub(crate) fn bitpos(&self) -> u32 {
        self.bitpos
    }

    /// Peek 1..=32 bits without consuming them.
    pub(crate) fn peek(&self, n: u8) -> Result<u32, Error> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(Error::BadBitstream);
        }
        let hw = (self.bitpos >> 4) as usize;
        let off = hw.checked_mul(2).ok_or(Error::BadBitstream)?;
        let end = off.checked_add(8).ok_or(Error::BadBitstream)?;
        let b = self.stream.get(off..end).ok_or(Error::BadBitstream)?;
        let w0 = u16::from_le_bytes([b[0], b[1]]) as u64;
        let w1 = u16::from_le_bytes([b[2], b[3]]) as u64;
        let w2 = u16::from_le_bytes([b[4], b[5]]) as u64;
        let w3 = u16::from_le_bytes([b[6], b[7]]) as u64;
        let v = (w0 << 48) | (w1 << 32) | (w2 << 16) | w3;
        let v = v << (self.bitpos & 15);
        Ok((v >> (64 - n as u32)) as u32)
    }

    #[inline]
    pub(crate) fn skip(&mut self, n: u8) -> Result<(), Error> {
        if n == 0 {
            return Ok(());
        }
        self.bitpos = self
            .bitpos
            .checked_add(n as u32)
            .ok_or(Error::BadBitstream)?;
        Ok(())
    }

    #[inline]
    pub(crate) fn get(&mut self, n: u8) -> Result<u32, Error> {
        let v = self.peek(n)?;
        self.skip(n)?;
        Ok(v)
    }

    /// EG(k): unary count of zero bits, the terminating one, `z` value bits,
    /// then `k` raw low bits.
    pub(crate) fn eg(&mut self, k: u8) -> Result<u32, Error> {
        if k > 31 {
            return Err(Error::BadBitstream);
        }
        let mut z = 0u32;
        while self.get(1)? == 0 {
            z += 1;
            if z > 24 {
                return Err(Error::BadBitstream);
            }
        }
        let mut v = ((1u32 << z) - 1) + self.get(z as u8)?;
        if k != 0 {
            v = v.wrapping_shl(k as u32) | self.get(k)?;
        }
        Ok(v)
    }
}
