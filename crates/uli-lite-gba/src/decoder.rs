//! The GBA-first product decoder.
//!
//! This is the hot path PR B moves out of the portable core: a 32-bit cached bit
//! reader over the ULA2 halfword stream, unchecked canonical-Huffman decode,
//! dequantisation and M/S, with ARM-mode code placed in `.iwram` and the hot
//! `x`/`buf`/`p` buffers owned by [`HotState`] so the caller can put them in
//! IWRAM.
//!
//! The portable `uli_lite::Decoder` remains the reference and the oracle. This
//! module must produce the same `FrameInfo`, operation counts and int8/int16
//! samples frame for frame; `tests/product_twin.rs` proves that on the host and
//! `examples/uli_bench` pins it on target.
//!
//! # Safety
//!
//! The table parser validates every table invariant this module relies on:
//! `rbits <= 16`, `esck <= 24`, `k <= MAXK`, canonical-code offsets inside
//! `sym[256]`, LUT indices inside `lut[1024]`. The band/coefficient indices are
//! loop-bounded (`b < NBANDS`, `i < BANDW`, `idx <= 15`). The unchecked reads
//! in this module are therefore in bounds for any album the parser accepted;
//! a malformed *bitstream* sets [`BitReader::bad`] and stops the frame instead
//! of producing an out-of-range index.

use core::fmt;

use uli_lite::format::{
    BANDW, CT_PAIR, CT_ZERO, H, LUTB, M, MAXK, MAXLEN, NBANDS, NCODED, NSTEP, XLIM,
};
use uli_lite::tables_data::STEP;
use uli_lite::{Album, CodeTables, Error, FrameInfo, HuffmanParts, Ops};

/// The hot per-frame buffers, split out of the decoder state so the product
/// can place them in IWRAM independently of the ~100 KiB code tables (which
/// belong in EWRAM).
#[derive(Debug)]
pub struct HotState {
    pub x: [[i32; M]; 2],
    pub buf: [i32; 2 * H],
    pub p: [[i32; H]; 2],
}

impl HotState {
    pub const fn new() -> Self {
        Self {
            x: [[0; M]; 2],
            buf: [0; 2 * H],
            p: [[0; H]; 2],
        }
    }

    /// Zero the transform, FFT and overlap state. The crate tables are not
    /// touched; [`GbaDecoder::new`] parses them once per album.
    pub fn reset(&mut self) {
        self.x = [[0; M]; 2];
        self.buf = [0; 2 * H];
        self.p = [[0; H]; 2];
    }

    fn reset_overlap(&mut self) {
        self.p = [[0; H]; 2];
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Diag {
    xover: u32,
    hfnz: u32,
}

/// A 32-bit bit reader over the ULA2 halfword stream.
///
/// The stream is 16-bit little-endian halfwords consumed MSB-first. The reader
/// keeps the next 32 bits in `hw` (bit 31 is the next bit) and appends whole
/// halfwords, so the common `peek(<=16)` and `skip` are shifts and ORs — no
/// 64-bit window, no variable 64-bit shift helper, no per-peek bounds-checked
/// slice construction. `peek` of more bits than are buffered (only long raw
/// fields and EG extensions) reads one extra halfword and combines with two
/// 32-bit shifts.
struct BitReader<'a> {
    stream: &'a [u8],
    halfwords: usize,
    /// Index of the next halfword to append.
    next_hw: usize,
    /// Absolute bit position of the next unconsumed bit.
    bitpos: u32,
    /// Next bits, MSB-aligned.
    hw: u32,
    /// Valid bits in `hw` (0..=32).
    valid: u32,
    /// A malformed/truncated stream was reached; the frame must be rejected.
    bad: bool,
}

impl<'a> BitReader<'a> {
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn new(stream: &'a [u8], bitpos: u32) -> Self {
        let mut br = Self {
            stream,
            halfwords: stream.len() / 2,
            next_hw: 0,
            bitpos,
            hw: 0,
            valid: 0,
            bad: false,
        };
        br.rebuild();
        br
    }

    /// Load the halfword at `i`. The caller has checked `i + 1 < halfwords`.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    unsafe fn load(stream: &[u8], i: usize) -> u32 {
        // SAFETY: the caller checked that the halfword exists.
        unsafe {
            let p = stream.as_ptr().add(i * 2);
            // Two byte loads: the stream's byte alignment is not guaranteed to
            // be halfword-aligned, and ARM7TDMI has no unaligned halfword
            // access.
            *p as u32 | ((*p.add(1) as u32) << 8)
        }
    }

    /// Reset the window to start at `self.bitpos`. Used at construction and
    /// when a skip consumes more than the buffered bits.
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn rebuild(&mut self) {
        let idx = (self.bitpos >> 4) as usize;
        let off = self.bitpos & 15;
        self.hw = 0;
        self.valid = 0;
        self.next_hw = idx + 2;
        if idx + 2 > self.halfwords {
            self.bad = true;
            return;
        }
        // SAFETY: idx + 1 < halfwords by the check above.
        let h0 = unsafe { Self::load(self.stream, idx) };
        let h1 = unsafe { Self::load(self.stream, idx + 1) };
        self.hw = ((h0 << 16) | h1) << off;
        self.valid = 32 - off;
    }

    /// Append halfwords until at least 17 bits are buffered (or the stream
    /// ends). 17 is enough for the common `peek` calls; larger peeks fall back
    /// to one extra halfword in [`Self::peek`].
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn refill(&mut self) {
        while self.valid <= 16 {
            if self.next_hw >= self.halfwords {
                self.bad = true;
                return;
            }
            // SAFETY: next_hw < halfwords, so both bytes of the halfword exist.
            let h = unsafe { Self::load(self.stream, self.next_hw) };
            self.next_hw += 1;
            self.hw |= h << (16 - self.valid);
            self.valid += 16;
        }
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn peek(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        if self.valid < n {
            self.refill();
        }
        if self.valid >= n {
            self.hw >> (32 - n)
        } else if self.bad || self.valid == 0 {
            0
        } else {
            // valid is 17..=23 here, so d = n - valid is 1..=7.
            let d = n - self.valid;
            if self.next_hw >= self.halfwords {
                self.bad = true;
                return 0;
            }
            // SAFETY: next_hw < halfwords.
            let h = unsafe { Self::load(self.stream, self.next_hw) };
            // Right-align the n-bit value: the fast path above shifts by
            // 32-n, so this path must produce the same alignment. `hw`'s top
            // `valid` bits are the high part of the result; the next d bits
            // from `h` are its low d bits.
            (self.hw >> (32 - n)) | (h >> (16 - d))
        }
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn skip(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        self.bitpos = self.bitpos.wrapping_add(n);
        if n < self.valid {
            self.hw <<= n;
            self.valid -= n;
            if self.valid <= 16 {
                self.refill();
            }
        } else {
            self.rebuild();
        }
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn bitpos(&self) -> u32 {
        self.bitpos
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.skip(n);
        v
    }

    /// EG(k): unary zero count, the terminating one, `z` value bits, then `k`
    /// raw low bits. Mirrors `uli_lite::bitstream::BitReader::eg`.
    #[inline(never)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    #[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
    fn eg(&mut self, k: u32) -> u32 {
        let mut z = 0u32;
        while self.get(1) == 0 {
            z += 1;
            if z > 24 {
                self.bad = true;
                return 0;
            }
        }
        let mut v = (1u32 << z) - 1 + self.get(z);
        if k != 0 {
            v = v.wrapping_shl(k) | self.get(k);
        }
        v
    }
}

/// The 10-bit LUT fast path, inlined at the call sites.
///
/// Nearly every symbol resolves in the first ten bits (the committed corpus
/// misses on ~8% of them), so the common case is a `peek`, one `u16` load and
/// a `skip`. Inlining *only this* keeps the call out of the hot path without
/// duplicating the long-code search at every call site: inlining the whole of
/// `huff` measured 2.7% but cost 4.3 KiB of IWRAM, which the product cannot
/// afford (U6 — the UI and feeder still need room, and IWRAM tables are
/// already blocked by the stack, see the plan of record).
///
/// Returns `None` when the symbol is not in the LUT, leaving the reader
/// untouched for [`huff_slow`].
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn huff_fast(br: &mut BitReader<'_>, p: &HuffmanParts<'_>) -> Option<u16> {
    let idx = br.peek(LUTB as u32) as usize;
    // SAFETY: peek(LUTB) < 1024 == p.lut.len().
    let e = unsafe { *p.lut.get_unchecked(idx) };
    if e == 0 {
        return None;
    }
    br.skip((e & 31) as u32);
    Some(e >> 5)
}

/// Canonical Huffman decode against the raw table views. The table parser
/// validated every index this can produce.
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn huff(br: &mut BitReader<'_>, p: &HuffmanParts<'_>, ops: &mut Ops) -> u16 {
    match huff_fast(br, p) {
        Some(v) => v,
        None => huff_slow(br, p, ops),
    }
}

/// The canonical-code search for a symbol longer than the LUT covers. Rare
/// enough to stay out of line; kept `#[inline(never)]` and in `.iwram` so the
/// ~8% of symbols that reach it do not pay a ROM fetch.
#[inline(never)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
fn huff_slow(br: &mut BitReader<'_>, p: &HuffmanParts<'_>, ops: &mut Ops) -> u16 {
    let mut l = LUTB + 1;
    while l <= MAXLEN {
        // SAFETY: l <= MAXLEN < p.first/count/offs.len().
        let c = br
            .peek(l as u32)
            .wrapping_sub(unsafe { *p.first.get_unchecked(l) } as u32);
        if c < unsafe { *p.count.get_unchecked(l) } as u32 {
            br.skip(l as u32);
            ops.long_codes = ops.long_codes.wrapping_add(1);
            let i = unsafe { *p.offs.get_unchecked(l) } as usize + c as usize;
            // SAFETY: the canonical table parser guarantees offs + count <=
            // 256, and c < count.
            return unsafe { *p.sym.get_unchecked(i) };
        }
        l += 1;
    }
    br.bad = true;
    0
}

/// One channel: band classes, tuples, escapes, dequantisation.
///
/// The arithmetic and operation counters are the core's `dec_chan`; only the
/// addressing (unchecked), the bit reader and the instruction set differ.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
fn dec_chan(
    tables: &CodeTables,
    x: &mut [i32; M],
    step: u32,
    br: &mut BitReader<'_>,
    ops: &mut Ops,
    diag: &mut Diag,
) {
    // Manual zero-fill: the slice `fill` goes through a compiler-builtins
    // interworking call, and this loop is on the hottest path.
    {
        let xp = x.as_mut_ptr();
        let mut i = 0;
        while i < M {
            // SAFETY: i < M.
            unsafe { xp.add(i).write(0) };
            i += 1;
        }
    }
    let k = tables.k;
    if k > MAXK {
        br.bad = true;
        return;
    }
    let ctype = &tables.ctype;
    let rbits = &tables.rbits;
    let esck = &tables.esck;
    let dq = &tables.dq;
    let mut ctx = k;

    let mut b = 0usize;
    while b < NBANDS {
        let h = tables.band_table(ctx);
        let c = huff(br, &h.parts(), ops) as usize;
        ops.bandsyms = ops.bandsyms.wrapping_add(1);
        if br.bad {
            return;
        }
        if c == k {
            return;
        }
        if c > k {
            br.bad = true;
            return;
        }
        ctx = c;
        // SAFETY: c < MAXK == ctype.len().
        let ct = unsafe { *ctype.get_unchecked(c) };
        if ct == CT_ZERO {
            b += 1;
            continue;
        }
        ops.coded_bands = ops.coded_bands.wrapping_add(1);
        let hc = tables.class_table(c);
        let hcp = hc.parts();
        // SAFETY: c < MAXK.
        let dqc = unsafe { dq.get_unchecked(c) };
        let esc = unsafe { *esck.get_unchecked(c) } as u32;
        let r = unsafe { *rbits.get_unchecked(c) } as u32;
        let mut m = [0i32; BANDW];

        if ct == CT_PAIR {
            let mut i = 0usize;
            while i < BANDW {
                let s = huff(br, &hcp, ops) as u32;
                ops.pairs = ops.pairs.wrapping_add(1);
                let nib = [s >> 4, s & 15];
                let mut kk = 0usize;
                while kk < 2 {
                    let mut hh = nib[kk];
                    if hh == 15 {
                        hh = hh.wrapping_add(br.eg(esc));
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    let mm = hh.wrapping_shl(r) | br.get(r);
                    if r != 0 {
                        ops.raws = ops.raws.wrapping_add(1);
                    }
                    let val = mm as i32;
                    // SAFETY: i + kk < BANDW.
                    *unsafe { m.get_unchecked_mut(i + kk) } = if mm != 0 && br.get(1) != 0 {
                        val.wrapping_neg()
                    } else {
                        val
                    };
                    kk += 1;
                }
                i += 2;
            }
        } else {
            let mut i = 0usize;
            while i < BANDW {
                let s = huff(br, &hcp, ops) as u32;
                ops.quads = ops.quads.wrapping_add(1);
                let dg = [s / 27, (s / 9) % 3, (s / 3) % 3, s % 3];
                let mut kk = 0usize;
                while kk < 4 {
                    let mut mm = dg[kk];
                    if mm == 2 {
                        mm = mm.wrapping_add(br.eg(esc));
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    let val = mm as i32;
                    // SAFETY: i + kk < BANDW.
                    *unsafe { m.get_unchecked_mut(i + kk) } = if mm != 0 && br.get(1) != 0 {
                        val.wrapping_neg()
                    } else {
                        val
                    };
                    kk += 1;
                }
                i += 4;
            }
        }

        let xp = x.as_mut_ptr();
        let mut i = 0usize;
        while i < BANDW {
            let v = m[i];
            if v == 0 {
                ops.zc = ops.zc.wrapping_add(1);
                i += 1;
                continue;
            }
            let a = v.unsigned_abs();
            // Explicit compare, not `Ord::min`: `min` lowers to an
            // out-of-line generic that ARM code calls through an interworking
            // thunk (the same class of call that hung `ola`'s `clamp`).
            let idx = if a > 15 { 15 } else { a as usize };
            // SAFETY: idx < 16 == dqc.len().
            let dqv = unsafe { *dqc.get_unchecked(idx) } as i32;
            let rq = 16i64 * a as i64 - dqv as i64;
            // The valid-stream path is a 32x32 product (SMULL in ARM mode);
            // the fallback keeps an escaped magnitude exact on malformed
            // streams, matching the reference's i64 arithmetic.
            let xv = if rq >= i32::MIN as i64 && rq <= i32::MAX as i64 {
                (rq as i32 as i64) * (step as i64)
            } else {
                rq * step as i64
            };
            if xv >= XLIM {
                diag.xover = diag.xover.wrapping_add(1);
            }
            let q = xv as i32;
            let out = if v < 0 { q.wrapping_neg() } else { q };
            let off = b * BANDW + i;
            // SAFETY: off < NBANDS * BANDW == M.
            unsafe { xp.add(off).write(out) };
            ops.nz = ops.nz.wrapping_add(1);
            if off >= NCODED {
                diag.hfnz = diag.hfnz.wrapping_add(1);
            }
            i += 1;
        }
        b += 1;
    }
}

/// The product decoder: same output as `uli_lite::Decoder`, GBA-first hot path.
pub struct GbaDecoder<'a, 's> {
    album: Album<'a>,
    tables: &'s CodeTables,
    hot: &'s mut HotState,
    bitpos: u32,
    frame: u32,
    g_prev: i32,
    ops: Ops,
    xover: u32,
    hfnz: u32,
}

impl fmt::Debug for GbaDecoder<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GbaDecoder")
            .field("album", &self.album)
            .field("tables", &self.tables)
            .field("bitpos", &self.bitpos)
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

impl<'a, 's> GbaDecoder<'a, 's> {
    /// Parse `album`'s tables into `tables`, reset `hot`, and position at frame
    /// 0. The caller owns the storage: on the GBA, `tables` belongs in EWRAM
    /// and `hot` in IWRAM.
    pub fn new(
        album: Album<'a>,
        tables: &'s mut CodeTables,
        hot: &'s mut HotState,
    ) -> Result<Self, Error> {
        tables.parse_into(album.tables())?;
        hot.reset();
        let tables: &'s CodeTables = tables;
        Ok(Self {
            album,
            tables,
            hot,
            bitpos: 0,
            frame: 0,
            g_prev: 0,
            ops: Ops::ZERO,
            xover: 0,
            hfnz: 0,
        })
    }

    pub fn album(&self) -> &Album<'a> {
        &self.album
    }

    pub fn frame_index(&self) -> u32 {
        self.frame
    }

    pub fn bitpos(&self) -> u32 {
        self.bitpos
    }

    pub fn g_prev(&self) -> i32 {
        self.g_prev
    }

    pub fn ops(&self) -> Ops {
        self.ops
    }

    pub fn xover(&self) -> u32 {
        self.xover
    }

    pub fn hfnz(&self) -> u32 {
        self.hfnz
    }

    /// Decode the next frame into 512-sample L/R outputs.
    ///
    /// Same contract as `uli_lite::Decoder::decode_frame`: int8 is always
    /// written, int16 is the optional pre-DAC path, and short buffers are
    /// rejected before any state changes.
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    #[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
    #[inline(never)]
    pub fn decode_frame(
        &mut self,
        out8l: &mut [i8],
        out8r: &mut [i8],
        out16l: Option<&mut [i16]>,
        out16r: Option<&mut [i16]>,
    ) -> Result<FrameInfo, Error> {
        if out8l.len() < M || out8r.len() < M {
            return Err(Error::OutputTooSmall);
        }
        if let Some(v) = out16l.as_ref() {
            if v.len() < M {
                return Err(Error::OutputTooSmall);
            }
        }
        if let Some(v) = out16r.as_ref() {
            if v.len() < M {
                return Err(Error::OutputTooSmall);
            }
        }
        if self.frame >= self.album.n_frames() {
            return Err(Error::EndOfStream);
        }

        let start = self.bitpos;
        let mut br = BitReader::new(self.album.stream(), start);
        let mut ops = Ops::ZERO;
        let mut diag = Diag::default();

        let z = br.eg(0);
        let g = self.state_g(z);
        self.g_prev = g;
        let ms = br.get(1) as i32;
        let geff = g.wrapping_sub(4 * ms);
        if geff < 0 || geff >= NSTEP as i32 {
            return Err(Error::BadFrame);
        }
        // SAFETY: geff is in 0..NSTEP.
        let step = unsafe { *STEP.get_unchecked(geff as usize) };
        ops.ms = ms as u32;
        if br.bad {
            return Err(Error::BadBitstream);
        }

        {
            let tables = self.tables;
            let hot = &mut *self.hot;
            dec_chan(tables, &mut hot.x[0], step, &mut br, &mut ops, &mut diag);
            dec_chan(tables, &mut hot.x[1], step, &mut br, &mut ops, &mut diag);
        }
        if br.bad {
            return Err(Error::BadBitstream);
        }

        if ms != 0 {
            let l = self.hot.x[0].as_mut_ptr();
            let r = self.hot.x[1].as_mut_ptr();
            // SAFETY: l and r are the two M-length rows; i < M.
            unsafe {
                for i in 0..M {
                    let a = (*l.add(i)) as u32;
                    let b = (*r.add(i)) as u32;
                    (*l.add(i)) = a.wrapping_add(b) as i32;
                    (*r.add(i)) = a.wrapping_sub(b) as i32;
                }
            }
        }

        crate::pre(&self.hot.x[0], &mut self.hot.buf);
        crate::fft(&mut self.hot.buf);
        crate::post(&self.hot.buf, &mut self.hot.x[0]);
        crate::ola(&self.hot.x[0], &mut self.hot.p[0], out8l, out16l);

        crate::pre(&self.hot.x[1], &mut self.hot.buf);
        crate::fft(&mut self.hot.buf);
        crate::post(&self.hot.buf, &mut self.hot.x[1]);
        crate::ola(&self.hot.x[1], &mut self.hot.p[1], out8r, out16r);

        self.bitpos = br.bitpos();
        self.ops = ops;
        self.xover = self.xover.wrapping_add(diag.xover);
        self.hfnz = self.hfnz.wrapping_add(diag.hfnz);
        let frame = self.frame;
        self.frame = self.frame.wrapping_add(1);
        Ok(FrameInfo {
            frame,
            bits: self.bitpos.wrapping_sub(start),
            bitpos: self.bitpos,
            ops,
            cycles: ops.cycle_model(),
            xover: diag.xover,
            hfnz: diag.hfnz,
        })
    }

    #[inline(always)]
    fn state_g(&self, z: u32) -> i32 {
        self.g_prev.wrapping_add(if z & 1 != 0 {
            -(((z.wrapping_add(1)) >> 1) as i32)
        } else {
            (z >> 1) as i32
        })
    }

    /// Seek to a track's preroll frame and decode it, discarding output.
    pub fn seek_track(&mut self, track: usize) -> Result<usize, Error> {
        let tr = self.album.track(track).ok_or(Error::BadSeek)?;
        self.bitpos = tr.bitpos;
        self.frame = tr.preroll_frame;
        self.g_prev = tr.g_prev as i32;
        self.hot.reset_overlap();
        let mut out8l = [0i8; M];
        let mut out8r = [0i8; M];
        self.decode_frame(&mut out8l, &mut out8r, None, None)?;
        Ok(tr.skip as usize)
    }

    /// Position at a seek-table entry without a preroll.
    pub fn seek_entry(&mut self, entry: usize) -> Result<(), Error> {
        let sk = self.album.seek(entry).ok_or(Error::BadSeek)?;
        let frame = u32::try_from(entry)
            .ok()
            .and_then(|e| e.checked_mul(self.album.seek_interval()))
            .ok_or(Error::BadSeek)?;
        self.bitpos = sk.bitpos;
        self.frame = frame;
        self.g_prev = sk.g_prev as i32;
        self.hot.reset_overlap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::BitReader;
    use uli_lite::Album;

    const BLOB: &[u8] = include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli");

    fn hw(stream: &[u8], i: usize) -> u64 {
        let b = &stream[i * 2..i * 2 + 2];
        (b[0] as u64) | ((b[1] as u64) << 8)
    }

    /// The core's 64-bit peek, as an independent oracle for the 32-bit reader.
    fn ref_peek(stream: &[u8], bitpos: u32, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let i = (bitpos >> 4) as usize;
        let v = (hw(stream, i) << 48)
            | (hw(stream, i + 1) << 32)
            | (hw(stream, i + 2) << 16)
            | hw(stream, i + 3);
        ((v << (bitpos & 15)) >> (64 - n)) as u32
    }

    fn ref_get(stream: &[u8], pos: &mut u32, n: u32) -> u32 {
        let v = ref_peek(stream, *pos, n);
        *pos += n;
        v
    }

    fn ref_eg(stream: &[u8], pos: &mut u32, k: u32) -> u32 {
        let mut z = 0u32;
        while ref_get(stream, pos, 1) == 0 {
            z += 1;
        }
        let mut v = (1u32 << z) - 1 + ref_get(stream, pos, z);
        if k != 0 {
            v = v.wrapping_shl(k) | ref_get(stream, pos, k);
        }
        v
    }

    #[test]
    fn bit_reader_fuzz_across_the_stream() {
        let album = Album::parse(BLOB).expect("fixture parses");
        let stream = album.stream();
        let mut rng = 0x1234_5678u32;
        for case in 0..4000 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let start = rng % ((stream.len() as u32) * 8 - 4096);
            let mut br = BitReader::new(stream, start);
            let mut pos = start;
            for step in 0..64 {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let n = rng % 24 + 1;
                assert_eq!(
                    br.peek(n),
                    ref_peek(stream, pos, n),
                    "case={case} step={step} pos={pos} n={n}"
                );
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let take = rng % 24 + 1;
                br.skip(take);
                pos += take;
                assert_eq!(br.bitpos(), pos);
                assert_eq!(
                    br.next_hw * 16,
                    pos as usize + br.valid as usize,
                    "invariant case={case} step={step} pos={pos} take={take} valid={} next_hw={}",
                    br.valid,
                    br.next_hw
                );
                if step % 4 == 0 {
                    rng ^= rng << 13;
                    rng ^= rng >> 17;
                    rng ^= rng << 5;
                    let k = rng % 24;
                    let mut rp = pos;
                    let want = ref_eg(stream, &mut rp, k);
                    let got = br.eg(k);
                    assert_eq!(got, want, "eg case={case} step={step} pos={pos} k={k}");
                    pos = rp;
                }
            }
        }
    }

    #[test]
    fn bit_reader_matches_the_64_bit_reference() {
        let album = Album::parse(BLOB).expect("fixture parses");
        let stream = album.stream();
        // Walk the first few thousand bits from every byte/bit offset.
        for start in 0..64u32 {
            let mut br = BitReader::new(stream, start);
            let mut pos = start;
            for step in 0..512 {
                for n in 1..=24u32 {
                    assert_eq!(
                        br.peek(n),
                        ref_peek(stream, pos, n),
                        "start={start} step={step} pos={pos} n={n}"
                    );
                }
                let take = ((step * 7 + start) % 24) + 1;
                br.skip(take);
                pos += take;
                assert_eq!(br.bitpos(), pos);
                if step % 3 == 0 {
                    let k = ((step * 5 + start) % 24) as u32;
                    let mut rp = pos;
                    let want_eg = ref_eg(stream, &mut rp, k);
                    let got_eg = br.eg(k);
                    assert_eq!(
                        got_eg, want_eg,
                        "eg start={start} step={step} pos={pos} k={k}"
                    );
                    assert_eq!(br.bitpos(), rp);
                    pos = rp;
                }
            }
        }
    }
}
