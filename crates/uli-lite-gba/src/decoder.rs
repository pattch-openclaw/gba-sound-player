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

/// The quad-digit expansion table (PR C-3a).
///
/// The 81-code quad symbol encodes four base-3 digits. Packing them into 2-bit
/// fields turns the hot loop's three divisions into one load, a shift and a
/// mask. Generated here, so the table's provenance is this repository.
const fn make_q3() -> [u8; 81] {
    let mut t = [0u8; 81];
    let mut s = 0;
    while s < 81 {
        let d0 = (s / 27) as u8;
        let d1 = ((s / 9) % 3) as u8;
        let d2 = ((s / 3) % 3) as u8;
        let d3 = (s % 3) as u8;
        t[s] = (d0 << 6) | (d1 << 4) | (d2 << 2) | d3;
        s += 1;
    }
    t
}

const Q3: [u8; 81] = make_q3();

/// Leading zeros in the top six bits, 6 when none are set (PR C-3a's escape
/// fast path).
const fn make_clz6() -> [u8; 64] {
    let mut t = [0u8; 64];
    let mut i = 0;
    while i < 64 {
        let mut z = 0u8;
        while z < 6 && (i >> (5 - z)) & 1 == 0 {
            z += 1;
        }
        t[i] = z;
        i += 1;
    }
    t
}

const CLZ6: [u8; 64] = make_clz6();

/// A 32-bit bit reader over the ULA2 halfword stream.
///
/// The stream is 16-bit little-endian halfwords consumed MSB-first. The reader
/// keeps the next 32 bits in `hw` (bit 31 is the next bit) and walks a cursor
/// over the stream, so a refill is a byte-pair load plus a cursor advance — no
/// index arithmetic and no separate bit-position counter (PR C-3a). The
/// position is *derived* from the cursor and `valid`, which is what a frame
/// reports when it ends.
///
/// `Copy` and free of borrows, so a channel decode holds it in a local (and, on
/// ARM, keeps the window in registers) and the caller re-seats it when the frame
/// ends.
#[derive(Clone, Copy)]
struct BitReader {
    base: *const u8,
    /// Next halfword to append.
    next: *const u8,
    /// One past the last whole halfword of the stream.
    end: *const u8,
    /// Next bits, MSB-aligned.
    hw: u32,
    /// Valid bits in `hw` (0..=32).
    valid: u32,
    /// A malformed/truncated stream was reached; the frame must be rejected.
    bad: bool,
}

impl BitReader {
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn new(stream: &[u8], bitpos: u32) -> Self {
        let base = stream.as_ptr();
        // Whole halfwords only: the format's stream is a halfword sequence, so
        // a trailing odd byte is not addressable.
        let end = base.wrapping_add(stream.len() & !1);
        let mut br = Self {
            base,
            next: base,
            end,
            hw: 0,
            valid: 0,
            bad: false,
        };
        br.set_pos(bitpos);
        br
    }

    /// Load the halfword at `p`. The caller has checked that both bytes exist.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    unsafe fn load(p: *const u8) -> u32 {
        // Two byte loads: the stream's byte alignment is not guaranteed, and
        // ARM7TDMI has no unaligned halfword access.
        unsafe { *p as u32 | ((*p.add(1) as u32) << 8) }
    }

    /// Absolute bit position of the next unconsumed bit.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn pos(&self) -> u32 {
        let loaded = (self.next as usize).wrapping_sub(self.base as usize) as u32;
        loaded.wrapping_mul(8).wrapping_sub(self.valid)
    }

    /// Move the cursor to absolute bit `p` and rebuild the window there. Used at
    /// construction and when a skip consumes more than the window holds.
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn set_pos(&mut self, p: u32) {
        let idx = (p >> 4) as usize;
        let off = p & 15;
        self.next = self.base.wrapping_add(idx * 2);
        self.hw = 0;
        self.valid = 0;
        if self.next.wrapping_add(4) > self.end {
            self.bad = true;
            return;
        }
        // SAFETY: the check above leaves two halfwords readable from `next`.
        let h0 = unsafe { Self::load(self.next) };
        let h1 = unsafe { Self::load(self.next.add(2)) };
        self.hw = ((h0 << 16) | h1) << off;
        self.valid = 32 - off;
        self.next = self.next.wrapping_add(4);
    }

    /// Append halfwords until at least 17 bits are buffered (or the stream
    /// ends). 17 is enough for the common `peek` calls; larger peeks fall back
    /// to one extra halfword in [`Self::peek`].
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn refill(&mut self) {
        while self.valid <= 16 {
            if self.next.wrapping_add(2) > self.end {
                self.bad = true;
                return;
            }
            // SAFETY: the check above leaves one halfword readable at `next`.
            let h = unsafe { Self::load(self.next) };
            self.next = self.next.wrapping_add(2);
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
            if self.next.wrapping_add(2) > self.end {
                self.bad = true;
                return 0;
            }
            // SAFETY: the check above leaves one halfword readable at `next`.
            let h = unsafe { Self::load(self.next) };
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
        if n < self.valid {
            self.hw <<= n;
            self.valid -= n;
            if self.valid <= 16 {
                self.refill();
            }
        } else {
            let p = self.pos().wrapping_add(n);
            self.set_pos(p);
        }
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn bitpos(&self) -> u32 {
        self.pos()
    }

    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.skip(n);
        v
    }
}

/// EG(k) with the reference's fast path (PR C-3a): when the unary prefix has at
/// most five zeros, the terminating one, the `z` value bits and the `k` raw low
/// bits form one `2z + 1 + k`-bit field `F`, and `EG = F - 2^k`. The rare long
/// prefix (and a truncated window) falls through to [`eg_slow`].
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn esc(br: &mut BitReader, k: u32) -> u32 {
    br.refill();
    let z = CLZ6[(br.hw >> 26) as usize] as u32;
    let t = 2 * z + 1 + k;
    if z < 6 && t <= 32 && br.valid >= t {
        let f = br.hw >> (32 - t);
        br.skip(t);
        f.wrapping_sub(1u32 << k)
    } else {
        eg_slow(br, k)
    }
}

/// EG(k) bit by bit: the unary zero count, the terminating one, the `z` value
/// bits, then `k` raw low bits. Mirrors `uli_lite::bitstream::BitReader::eg`.
#[inline(never)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
fn eg_slow(br: &mut BitReader, k: u32) -> u32 {
    let mut z = 0u32;
    while br.get(1) == 0 {
        z += 1;
        if z > 24 {
            br.bad = true;
            return 0;
        }
    }
    let mut v = (1u32 << z) - 1 + br.get(z);
    if k != 0 {
        v = v.wrapping_shl(k) | br.get(k);
    }
    v
}
/// Decode one canonical symbol and consume its code.
///
/// Returns the LUT entry — symbol in the high bits, code length in the low five
/// — **already consumed** (PR C-3a), so a call site does not issue a second
/// `skip`. `1` (a zero symbol with a one-bit code) is the "stream rejected"
/// sentinel: a real length is never zero, and a symbol of zero is legitimate, so
/// the caller checks [`BitReader::bad`] once per band rather than per symbol.
///
/// Nearly every symbol resolves in the first ten bits (the committed corpus
/// misses on ~8% of them), so the common case is a `peek`, one `u16` load and a
/// `skip`. Keeping the long-code search out of line measured better than
/// inlining all of it (+1.6% for 896 B rather than +2.7% for 4,252 B; PR C).
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn huff(br: &mut BitReader, p: &HuffmanParts<'_>, ops: &mut Ops) -> u16 {
    let idx = br.peek(LUTB as u32) as usize;
    // SAFETY: peek(LUTB) < 1024 == p.lut.len().
    let e = unsafe { *p.lut.get_unchecked(idx) };
    if e != 0 {
        br.skip((e & 31) as u32);
        return e;
    }
    huff_slow(br, p, ops)
}

/// The canonical-code search for a symbol longer than the LUT covers. Rare
/// enough to stay out of line; kept `#[inline(never)]` and in `.iwram` so the
/// ~8% of symbols that reach it do not pay a ROM fetch.
#[inline(never)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
fn huff_slow(br: &mut BitReader, p: &HuffmanParts<'_>, ops: &mut Ops) -> u16 {
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
            let sym = unsafe { *p.sym.get_unchecked(i) } as u32;
            return ((sym << 5) | l as u32) as u16;
        }
        l += 1;
    }
    br.bad = true;
    1
}

/// Write one zero band: 16 coefficients, four words at a time (PR C-3a).
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
unsafe fn zero16(p: *mut i32) {
    // SAFETY: the caller guarantees 16 writable i32 at `p`.
    unsafe {
        let mut i = 0;
        while i < BANDW {
            p.add(i).cast::<[i32; 4]>().write([0; 4]);
            i += 4;
        }
    }
}

/// Dequantise one coefficient straight into its slot.
///
/// `16 * m - dq[c][m]`, then one multiply by the frame step: the reference's
/// arithmetic, with the `m < 16` shape read from the album's precomputed
/// [`uli_lite::CodeTables::rq`] and escaped magnitudes using the `m = 15` shape
/// (PR C-3a). The sign bit is consumed here, in the same order the reference
/// reads it.
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
unsafe fn put_coef(
    p: *mut i32,
    m: u32,
    rq: &[i16; 16],
    dq15: i8,
    step: u32,
    index: usize,
    br: &mut BitReader,
    ops: &mut Ops,
    diag: &mut Diag,
) {
    if m == 0 {
        ops.zc = ops.zc.wrapping_add(1);
        // SAFETY: the caller guarantees `p` is a coefficient slot.
        unsafe { p.write(0) };
        return;
    }
    // SAFETY: `rq` has 16 entries and the branch constrains `m < 16`.
    let r = if m < 16 {
        unsafe { *rq.get_unchecked(m as usize) as i32 }
    } else {
        ((m as i32) << 4) - dq15 as i32
    };
    let xv = r.wrapping_mul(step as i32);
    if xv >= XLIM as i32 {
        diag.xover = diag.xover.wrapping_add(1);
    }
    let q = xv;
    let v = if br.get(1) != 0 { q.wrapping_neg() } else { q };
    // SAFETY: the caller guarantees `p` is a coefficient slot.
    unsafe { p.write(v) };
    ops.nz = ops.nz.wrapping_add(1);
    if index >= NCODED {
        diag.hfnz = diag.hfnz.wrapping_add(1);
    }
}

/// One channel: band classes, tuples, escapes, dequantisation.
///
/// Structurally the reference's `dec_chan` (PR C-3a): the reader is a local, a
/// band's coefficients are written straight into `x` as they decode, zero bands
/// and the tail after EOB are whole-band stores, and the dequant is a table load
/// plus one multiply. Nothing is pre-zeroed, so every coefficient is written
/// exactly once per frame.
///
/// Returns the number of bands before EOB, which the caller uses to bound M/S.
/// `br.bad` signals a rejected stream; the caller discards the frame, so a
/// partially written `x` is never observable.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
unsafe fn dec_chan(
    tables: &CodeTables,
    x: *mut i32,
    step: u32,
    br: &mut BitReader,
    ops: &mut Ops,
    diag: &mut Diag,
) -> usize {
    let k = tables.k;
    if k > MAXK {
        br.bad = true;
        return 0;
    }
    let mut ctx = k;
    let mut b = 0usize;
    while b < NBANDS {
        let band = tables.band_table(ctx).parts();
        let e = huff(br, &band, ops);
        ops.bandsyms = ops.bandsyms.wrapping_add(1);
        if br.bad {
            return b;
        }
        let c = (e >> 5) as usize;
        if c == k {
            break;
        }
        if c >= MAXK {
            br.bad = true;
            return b;
        }
        ctx = c;
        // SAFETY: b < NBANDS, so the band's 16 slots are inside the channel.
        let xp = unsafe { x.add(b * BANDW) };
        // SAFETY: c < MAXK == ctype.len().
        let ct = unsafe { *tables.ctype.get_unchecked(c) };
        if ct == CT_ZERO {
            unsafe { zero16(xp) };
            b += 1;
            continue;
        }
        ops.coded_bands = ops.coded_bands.wrapping_add(1);
        let cls = tables.class_table(c).parts();
        // SAFETY: c < MAXK.
        let rq = unsafe { tables.rq.get_unchecked(c) };
        let dq15 = unsafe { *tables.dq15.get_unchecked(c) };
        let esck = unsafe { *tables.esck.get_unchecked(c) } as u32;
        if ct == CT_PAIR {
            let r = unsafe { *tables.rbits.get_unchecked(c) } as u32;
            let mut i = 0usize;
            while i < BANDW {
                let e = huff(br, &cls, ops);
                ops.pairs = ops.pairs.wrapping_add(1);
                let symb = (e >> 5) as u32;
                let mut kk = 0usize;
                while kk < 2 {
                    let mut m = if kk == 0 { symb >> 4 } else { symb & 15 };
                    if m == 15 {
                        m = m.wrapping_add(esc(br, esck));
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    let mm = m.wrapping_shl(r) | br.get(r);
                    if r != 0 {
                        ops.raws = ops.raws.wrapping_add(1);
                    }
                    // SAFETY: i < BANDW and kk < 2.
                    unsafe {
                        put_coef(
                            xp.add(i + kk),
                            mm,
                            rq,
                            dq15,
                            step,
                            b * BANDW + i + kk,
                            br,
                            ops,
                            diag,
                        )
                    };
                    kk += 1;
                }
                i += 2;
            }
        } else {
            let mut i = 0usize;
            while i < BANDW {
                let e = huff(br, &cls, ops);
                ops.quads = ops.quads.wrapping_add(1);
                // SAFETY: a CT_QUAD class table has exactly 81 symbols.
                let q3 = unsafe { *Q3.get_unchecked((e >> 5) as usize) };
                let mut kk = 0usize;
                while kk < 4 {
                    let mut m = ((q3 >> (6 - 2 * kk)) & 3) as u32;
                    if m == 2 {
                        m = m.wrapping_add(esc(br, esck));
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    // SAFETY: i < BANDW and kk < 4.
                    unsafe {
                        put_coef(
                            xp.add(i + kk),
                            m,
                            rq,
                            dq15,
                            step,
                            b * BANDW + i + kk,
                            br,
                            ops,
                            diag,
                        )
                    };
                    kk += 1;
                }
                i += 4;
            }
        }
        b += 1;
    }
    // Tail after EOB: whole bands of zeros, so no coefficient is ever stale.
    while b < NBANDS {
        // SAFETY: b < NBANDS.
        unsafe { zero16(x.add(b * BANDW)) };
        b += 1;
    }
    b
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

        let z = esc(&mut br, 0);
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

        let (nb0, nb1) = {
            let tables = self.tables;
            let hot = &mut *self.hot;
            // SAFETY: each row is M coefficients and `dec_chan` is band-bounded.
            unsafe {
                let nb0 = dec_chan(
                    tables,
                    hot.x[0].as_mut_ptr(),
                    step,
                    &mut br,
                    &mut ops,
                    &mut diag,
                );
                let nb1 = dec_chan(
                    tables,
                    hot.x[1].as_mut_ptr(),
                    step,
                    &mut br,
                    &mut ops,
                    &mut diag,
                );
                (nb0, nb1)
            }
        };
        if br.bad {
            return Err(Error::BadBitstream);
        }

        if ms != 0 {
            // The reference transforms only the coded coefficients
            // (`nb = max(nb0, nb1)`); everything past them is zero in both
            // channels, so the result is identical and the loop is shorter
            // (PR C-3a).
            let n = if nb0 > nb1 { nb0 } else { nb1 } * BANDW;
            let l = self.hot.x[0].as_mut_ptr();
            let r = self.hot.x[1].as_mut_ptr();
            // SAFETY: l and r are the two M-length rows and n <= M.
            unsafe {
                for i in 0..n {
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
    use super::{BitReader, esc};
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
                    (br.next as usize - br.base as usize) * 8,
                    pos as usize + br.valid as usize,
                    "invariant case={case} step={step} pos={pos} take={take} valid={}",
                    br.valid
                );
                if step % 4 == 0 {
                    rng ^= rng << 13;
                    rng ^= rng >> 17;
                    rng ^= rng << 5;
                    let k = rng % 24;
                    let mut rp = pos;
                    let want = ref_eg(stream, &mut rp, k);
                    let got = esc(&mut br, k);
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
                    let got_eg = esc(&mut br, k);
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
