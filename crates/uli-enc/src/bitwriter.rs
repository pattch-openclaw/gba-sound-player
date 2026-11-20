//! MSB-first bit writer for the ULA2 frame stream.
//!
//! Bits go MSB-first into little-endian 16-bit halfwords, chunked across the halfword
//! boundary as they are written, and the stream is padded to an even halfword count at the
//! end. Pure integers — no float, no libm — because this path has to be deterministic on
//! every host.
//!
//! The chunking is not obvious and it is what makes the format readable: a field that
//! straddles a halfword occupies the low bits of one word and the high bits of the next,
//! rather than being split into byte order or buffered. The reader's 16-bit window depends
//! on exactly that.

/// Accumulates bits into halfwords; `pos` is the absolute bit position.
#[derive(Debug, Default)]
pub struct BitWriter {
    bits: Vec<u16>,
    pos: u64,
}

/// The format's trailing padding: four extra halfwords, then the count is rounded up to
/// even. The four give the reader room to peek eight bytes past the last frame without
/// running off the buffer; the round to even lands the stream on a 32-bit boundary.
const PAD_HALFWORDS: usize = 4;

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Absolute bit position, i.e. how many bits have been written.
    pub fn pos(&self) -> u64 {
        self.pos
    }

    /// The halfwords written so far. The last one may be partially filled; use
    /// [`BitWriter::pos`] to know how many bits are meaningful.
    pub fn halfwords(&self) -> &[u16] {
        &self.bits
    }

    /// Append `n` bits of `v`, MSB first. Chunked across the halfword boundary
    /// like `bw_put`: `off = pos & 15`, `room = 16 - off`.
    pub fn put(&mut self, v: u32, n: u32) {
        let mut n = n;
        while n > 0 {
            let wi = (self.pos >> 4) as usize;
            let off = (self.pos & 15) as u32;
            let room = 16 - off;
            let take = n.min(room);
            if wi >= self.bits.len() {
                self.bits.resize(wi + 1, 0);
            }
            let part = (v >> (n - take)) & ((1u32 << take) - 1);
            if off == 0 {
                self.bits[wi] = 0;
            }
            self.bits[wi] |= (part << (room - take)) as u16;
            self.pos += take as u64;
            n -= take;
        }
    }

    /// Exp-Golomb order `k`, matching `bw_put_eg`: `nb - 1` zero bits, `x` in
    /// `nb` bits, then the `k` low bits of `v`.
    pub fn put_eg(&mut self, v: u32, k: u32) {
        let x = (v >> k) + 1;
        let nb = 32 - x.leading_zeros();
        self.put(0, nb - 1);
        self.put(x, nb);
        if k != 0 {
            self.put(v & ((1u32 << k) - 1), k);
        }
    }

    /// Append `n` bits starting at bit `start` of `src`, used to concatenate
    /// per-chunk frame streams in frame-index order.
    pub fn append_bits(&mut self, src: &[u16], start: u64, n: u64) {
        let mut done = 0u64;
        while done < n {
            let take = (n - done).min(16) as u32;
            let v = read_bits(src, start + done, take);
            self.put(v, take);
            done += take as u64;
        }
    }

    /// The complete stream as little-endian halfword bytes, padded to an even
    /// halfword count with the format's trailing halfwords.
    pub fn finish(self) -> Vec<u8> {
        let hw = padded_halfwords(self.pos);
        let mut out = Vec::with_capacity(hw * 2);
        for i in 0..hw {
            let w = self.bits.get(i).copied().unwrap_or(0);
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }
}

/// The stream's declared length in halfwords: the bit count rounded up to whole halfwords,
/// plus four, rounded up to even. The trailing halfwords are what a reader's 8-byte peek at
/// the last frame relies on, so they are part of the format rather than slack.
pub fn padded_halfwords(bits: u64) -> usize {
    let mut hw = ((bits as usize) + 15) / 16 + PAD_HALFWORDS;
    if hw & 1 != 0 {
        hw += 1;
    }
    hw
}

/// Read `n <= 16` bits at absolute bit position `off`, MSB first.
fn read_bits(src: &[u16], off: u64, n: u32) -> u32 {
    let wi = (off >> 4) as usize;
    let b = (off & 15) as u32;
    let hi = src.get(wi).copied().unwrap_or(0) as u64;
    let lo = src.get(wi + 1).copied().unwrap_or(0) as u64;
    ((((hi << 16) | lo) << b) >> (32 - n)) as u32
}

/// Exp-Golomb bit count, matching `eg_bits`: `x = (v >> k) + 1`,
/// `nb = bitlen(x)`, `2 * nb - 1 + k`.
pub fn eg_bits(v: u32, k: u32) -> u32 {
    let x = (v >> k) + 1;
    let nb = 32 - x.leading_zeros();
    2 * nb - 1 + k
}

/// Zigzag an `i32` — the mapping the decoder inverts for the `g` delta
/// (`frame.rs`: `g = g_prev + (z odd ? -(z+1)/2 : z/2)`).
pub fn zz(d: i32) -> u32 {
    if d >= 0 {
        2 * d as u32
    } else {
        2 * (d.unsigned_abs()) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_eg_of_zero_at_k_zero_is_one_bit() {
        let mut bw = BitWriter::new();
        bw.put_eg(zz(0), 0);
        assert_eq!(bw.pos(), 1);
        assert_eq!(bw.halfwords(), &[0x8000]);
    }

    #[test]
    fn eg_bits_agrees_with_the_bits_actually_written() {
        for v in 0..1000u32 {
            for k in 0..=5u32 {
                let mut bw = BitWriter::new();
                bw.put_eg(v, k);
                assert_eq!(
                    bw.pos(),
                    eg_bits(v, k) as u64,
                    "v={v} k={k}: eg_bits disagrees with put_eg"
                );
            }
        }
    }

    #[test]
    fn append_bits_reproduces_a_concatenation() {
        // Three arbitrary fields, concatenated by hand and by append_bits.
        let mut a = BitWriter::new();
        a.put(0b101, 3);
        a.put(0xabcd, 16);
        let mut b = BitWriter::new();
        b.put(0b11, 2);
        b.put(0x7, 3);

        let mut joined = BitWriter::new();
        joined.append_bits(a.halfwords(), 0, a.pos());
        joined.append_bits(b.halfwords(), 0, b.pos());

        let mut expected = BitWriter::new();
        expected.put(0b101, 3);
        expected.put(0xabcd, 16);
        expected.put(0b11, 2);
        expected.put(0x7, 3);

        assert_eq!(joined.pos(), expected.pos());
        assert_eq!(joined.halfwords(), expected.halfwords());
    }

    #[test]
    fn finish_pads_to_an_even_halfword_count() {
        let mut bw = BitWriter::new();
        bw.put(1, 1);
        let out = bw.finish();
        // 1 bit -> 1 halfword + 4 pad = 5, rounded up to 6.
        assert_eq!(out.len(), 12);
        assert_eq!(&out[..2], &[0x00, 0x80], "halfword 0x8000 is little-endian");
        assert!(out[2..].iter().all(|&b| b == 0));
    }
}
