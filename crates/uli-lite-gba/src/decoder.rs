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

use uli_lite::format::{BANDW, CT_PAIR, CT_ZERO, H, LUTB, M, MAXK, MAXLEN, NBANDS, NSTEP};
// The diagnostics' operands only exist in a `verify` build (PR C-3b).
#[cfg(feature = "verify")]
use uli_lite::format::{NCODED, XLIM};
use uli_lite::tables_data::STEP;
use uli_lite::{Album, CodeTables, Error, FrameInfo, Huffman, Ops};

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

/// The per-class decode tables, in the layout the coefficient loops want.
///
/// **This is the 560 B of `CodeTables` that belongs in IWRAM**, and it is kept
/// separate from [`HotState`] on purpose: `HotState` holds per-frame buffers that
/// [`HotState::reset`] clears, while these are per-album tables written once by
/// [`GbaDecoder::new`] and never reset. Mixing them would make `reset()`'s
/// contract ambiguous.
///
/// The reference splits the same way — `uli_g.ctype`, `rbits`, `esck` and
/// `rq[16][16]` are copied into IWRAM while the ~96 KiB of Huffman LUTs stay in
/// EWRAM — and the split measures **−897 mean / −2,498 at the densest frame**
/// (2026-11-19). Nearly all of it is `rq[m]`, read once per **nonzero
/// coefficient** (616 per channel, so ~1,232/frame), which costs 4 cycles from
/// EWRAM against 1 from IWRAM at WAITCNT 4/2. It is less than the raw access
/// count suggests because the read still sits behind a stack reload of the
/// `rq` *pointer*: that half is register pressure, and it is P1's job, not this
/// type's. `ctype`/`rbits`/`esck` are read once per *band* (~64/frame) and
/// are carried only because they are the same table and the reference keeps them
/// together.
///
/// Caller-owned, like `HotState`: the caller declares the placement.
#[derive(Debug, Clone, Copy)]
pub struct ClassTables {
    pub ctype: [u8; MAXK],
    pub rbits: [u8; MAXK],
    pub esck: [u8; MAXK],
    pub rq: [[i16; 16]; MAXK],
}

impl ClassTables {
    pub const fn new() -> Self {
        Self {
            ctype: [0; MAXK],
            rbits: [0; MAXK],
            esck: [0; MAXK],
            rq: [[0; 16]; MAXK],
        }
    }

    /// Copy the four per-class arrays out of the (EWRAM) code tables.
    ///
    /// One `memcpy` per album, at construction: the whole point is that the
    /// inner loops then read `rq`/`ctype`/`rbits`/`esck` from IWRAM instead.
    pub fn fill(&mut self, tables: &CodeTables) {
        self.ctype = tables.ctype;
        self.rbits = tables.rbits;
        self.esck = tables.esck;
        self.rq = tables.rq;
    }
}

/// Format-rule diagnostics: `xover` (a coefficient past the encoder's `|X|`
/// bound) and `hfnz` (a nonzero coefficient at index >= `NCODED`).
///
/// Both are a compare per nonzero coefficient, measured at ~19,000 cycles on the
/// worst frame, so the **product build compiles them out** (PR C-3b). The rules
/// stay witnessed: `examples/uli_bench`'s correctness pass reads them off the
/// core decoder, and `tests/product_twin.rs` reads them from a `verify` build.
#[derive(Clone, Copy, Debug, Default)]
struct Diag {
    #[cfg(feature = "verify")]
    xover: u32,
    #[cfg(feature = "verify")]
    hfnz: u32,
}

/// Count one operation on the hot path.
///
/// A no-op in the product build. The counters feed the reference twin's
/// `FrameInfo` comparison and the cycle model; in the loop they cost ~11,000
/// cycles on the worst frame (PR C-3b), so they only exist under `verify`.
/// Call sites read the same as the reference's `ops`.
macro_rules! counter {
    ($($name:ident => $field:ident),* $(,)?) => {
        $(
            #[inline(always)]
            #[allow(unused_variables)]
            fn $name(ops: &mut Ops) {
                #[cfg(feature = "verify")]
                {
                    ops.$field = ops.$field.wrapping_add(1);
                }
            }
        )*
    };
}

counter!(
    count_band_symbol => bandsyms,
    count_coded_band => coded_bands,
    count_pair => pairs,
    count_quad => quads,
    count_nonzero => nz,
    count_zero => zc,
    count_raw => raws,
    count_escape => escs,
    count_long => long_codes,
);

/// Record an `|X|`-bound violation. No-op in the product build; see [`Diag`].
#[inline(always)]
#[allow(unused_variables, dead_code)]
fn note_xover(diag: &mut Diag) {
    #[cfg(feature = "verify")]
    {
        diag.xover = diag.xover.wrapping_add(1);
    }
}

/// Record a nonzero coefficient past `NCODED`. No-op in the product build.
#[inline(always)]
#[allow(unused_variables, dead_code)]
fn note_hfnz(diag: &mut Diag) {
    #[cfg(feature = "verify")]
    {
        diag.hfnz = diag.hfnz.wrapping_add(1);
    }
}

/// `huff_slow` hits, for the sub-frame entropy attribution (`attrib-count`).
///
/// A `static mut` accessed only through `addr_of!`/`addr_of_mut!` with volatile
/// reads and writes: `Processor::AtomicU32` does not exist on ARMv4T (there is
/// no `LDREX`/`STREX` on an ARM7TDMI), and the GBA decode path is single-
/// threaded by construction. Volatile keeps the increment from being folded away
/// in the frames whose hits are never read. Off by default — `Ops::long_codes`
/// carries the same number in a `verify` build, and this exists because the
/// target `verify` image fails the `ovl` pin at HEAD.
#[cfg(feature = "attrib-count")]
static mut SLOW_HITS: u32 = 0;

/// Long-code (`huff_slow`) hits recorded by this image; 0 unless `attrib-count`.
pub fn slow_hit_count() -> u32 {
    #[cfg(feature = "attrib-count")]
    {
        // SAFETY: single-threaded; no reference to the static outlives the read.
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(SLOW_HITS)) }
    }
    #[cfg(not(feature = "attrib-count"))]
    {
        0
    }
}

#[inline(always)]
#[allow(unused_variables, dead_code)]
fn count_slow_hit() {
    #[cfg(feature = "attrib-count")]
    {
        // SAFETY: single-threaded; the address is taken without forming a
        // reference, and a volatile read-modify-write keeps the increment.
        unsafe {
            let p = core::ptr::addr_of_mut!(SLOW_HITS);
            core::ptr::write_volatile(p, core::ptr::read_volatile(p).wrapping_add(1));
        }
    }
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

/// The quad-digit and escape-prefix lookup tables, **in IWRAM**.
///
/// `Q3` is read once per quad group (~496/frame for the pair) and `CLZ6` once
/// per escape, so at WAITCNT 4/2 the cartridge's 4-cycle first access against
/// IWRAM's 1 is worth ~1k cycles/frame. The reference keeps `uli_g_q3` in IWRAM
/// for exactly this reason. Both are `static` (not `const`) so they are one
/// object with an address rather than a value the compiler is free to re-emit
/// into `.rodata` at each use.
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
static Q3: [u8; 81] = make_q3();

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

#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
static CLZ6: [u8; 64] = make_clz6();

/// A 32-bit bit reader over the ULA2 halfword stream.
///
/// The stream is 16-bit little-endian halfwords consumed MSB-first. The reader
/// keeps the next 32 bits in `hw` (bit 31 is the next bit) and walks a cursor
/// over the stream, so a refill is a halfword load plus a cursor advance — no
/// index arithmetic and no separate bit-position counter. The position is
/// *derived* from the cursor and `valid`.
///
/// # The hot-path contract (Step 1: entropy/dequant rewrite)
///
/// The product path holds this struct **by value in a local**, never behind a
/// `&mut`, so the window lives in registers: `dec_chan` takes it and returns it,
/// and the out-of-line slow path (the long escape) takes a copy and hands it
/// back. Nothing takes `&mut BitReader` across a call boundary except functions
/// that are `#[inline(always)]`, which is what stops the struct's address from
/// escaping — the measured cost of that escape was a store of `next`/`hw`/
/// `valid`/`bad` on every refill and every slow call (Step 0b).
///
/// The contract:
///
/// * [`Self::refill`] guarantees `valid >= 17` (or sets `bad`), so a symbol of up
///   to 16 bits followed by a sign bit can be consumed without another check;
/// * every `huff`, `esc` and raw-field read is preceded by a `refill`;
/// * [`Self::take`] is branchless and requires `n <= valid`, which the two rules
///   above establish. [`Self::skip`] (which may resynchronise the cursor) is for
///   cold paths and the tests only.
///
/// `valid >= 17` rather than `>= 16` because a group reads `rbits` raw bits and
/// then a sign bit: `take(16)` followed by `take(1)` must not underflow.
///
/// `Copy` and free of borrows. `base` is read only by the cold position and
/// resynchronisation paths, so it is dead in the hot loops.
#[derive(Clone, Copy)]
struct BitReader {
    /// Start of the stream: cold paths only (position reporting, resync).
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
        // The stream is read with aligned `u16` loads (one `ldrh` per refill
        // instead of two `ldrb` + an `orr`; see `load`). That is sound only if
        // the base is halfword-aligned. The container already requires an even
        // *offset* (`Album::parse` rejects `stream_off & 1`), so the only way
        // to get an odd base is a caller-supplied blob at an odd address — and
        // `&[u8]` gives no alignment guarantee, so it is checked here rather
        // than assumed. Rejection, not arithmetic: the decision log's rule for
        // input this decoder cannot honour.
        if (base as usize) & 1 != 0 {
            br.bad = true;
        }
        br.set_pos(bitpos);
        br
    }

    /// Load the halfword at `p` — **one `ldrh`**.
    ///
    /// The C reference reads `*(b)->p++` from a `const uint16_t*`, i.e. one
    /// halfword load. Ours used to be two byte loads plus a shift-or, because
    /// ARM7TDMI has no unaligned halfword access and a `&[u8]` base could be
    /// odd — four instructions on the single most-executed path in the decoder
    /// (~900 refills per frame). [`Self::new`] now establishes even alignment
    /// once, so the load is an aligned `u16` read and LLVM emits `ldrh`.
    ///
    /// # Safety
    /// `p` must be halfword-aligned and have two readable bytes. [`Self::new`]
    /// pins the alignment for every pointer the reader forms, and each call site
    /// checks the bound first.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    unsafe fn load(p: *const u8) -> u32 {
        // SAFETY: the caller guarantees alignment (invariant of `new`) and
        // two readable bytes (`refill`/`peek`/`set_pos` bound-check first).
        unsafe { (p as *const u16).read() as u32 }
    }

    /// Absolute bit position of the next unconsumed bit.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn pos(&self) -> u32 {
        let loaded = (self.next as usize).wrapping_sub(self.base as usize) as u32;
        loaded.wrapping_mul(8).wrapping_sub(self.valid)
    }

    /// Move the cursor to absolute bit `p` and rebuild the window there. Cold:
    /// used at construction, and by [`Self::skip`] when a skip is longer than the
    /// window holds (which the hot contract never allows).
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

    /// Hot-path refill: append halfwords until `valid >= 17`, or set `bad`.
    ///
    /// One conditional load, with a second only when the window was empty
    /// (`valid == 0`, which leaves 16 after one load), so the common case is a
    /// compare and a predicated load rather than a loop. A caller that already
    /// has `valid >= 17` pays just the compare.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn refill(&mut self) {
        if self.valid >= 17 {
            return;
        }
        if self.next.wrapping_add(2) > self.end {
            self.bad = true;
            return;
        }
        // SAFETY: the check above leaves one halfword readable at `next`.
        let h = unsafe { Self::load(self.next) };
        self.next = self.next.wrapping_add(2);
        self.hw |= h << (16 - self.valid);
        self.valid += 16;
        if self.valid >= 17 {
            return;
        }
        // `valid` was 0: one more halfword, appended at shift 0.
        if self.next.wrapping_add(2) > self.end {
            self.bad = true;
            return;
        }
        // SAFETY: as above.
        let h = unsafe { Self::load(self.next) };
        self.next = self.next.wrapping_add(2);
        self.hw |= h;
        self.valid += 16;
    }

    /// Consume `n` bits. Branchless: requires `n <= valid` (the hot contract).
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn take(&mut self, n: u32) {
        self.hw <<= n;
        self.valid -= n;
    }

    /// The top `n` bits, not consumed. Requires `1 <= n <= valid`.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn top(&self, n: u32) -> u32 {
        self.hw >> (32 - n)
    }

    // ---- general reader: cold paths and the host tests ---------------------
    //
    // These carry the "may resynchronise" behaviour the hot contract removes, so
    // `tests/` can drive the reader against the 64-bit reference at arbitrary
    // positions and lengths.

    /// The next `n` bits without consuming them, refilling or falling back to a
    /// partial load as needed. Cold paths and tests only.
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
            self.top(n)
        } else if self.bad || self.valid == 0 {
            0
        } else {
            // valid is > 0 and < n, so d = n - valid is in 1..=7 for the cold
            // callers (n <= 24).
            let d = n - self.valid;
            if self.next.wrapping_add(2) > self.end {
                self.bad = true;
                return 0;
            }
            // SAFETY: the check above leaves one halfword readable at `next`.
            let h = unsafe { Self::load(self.next) };
            self.top(n) | (h >> (16 - d))
        }
    }

    /// Consume `n` bits, resynchronising the cursor if the window cannot cover
    /// them. Cold paths and tests only.
    #[inline(always)]
    #[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
    fn skip(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        if n < self.valid {
            self.hw <<= n;
            self.valid -= n;
            self.refill();
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

/// EG(k) with the reference's fast path: when the unary prefix has at most five
/// zeros, the terminating one, the `z` value bits and the `k` raw low bits form
/// one `2z + 1 + k`-bit field `F`, and `EG = F - 2^k`.
///
/// Under the hot contract `valid >= 17` on entry, so the fast path covers every
/// escape with `t <= 13`; a long prefix (or a general reader whose window is
/// short) falls through to [`eg_slow`]. Refills again on exit, so the caller is
/// back under the contract.
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn esc(br: &mut BitReader, k: u32) -> u32 {
    br.refill();
    let z = CLZ6[(br.hw >> 26) as usize] as u32;
    let t = 2 * z + 1 + k;
    // `t < 32` keeps `take` in range (a 32-bit shift is not a shift); a general
    // reader can reach `t == 32`, the product contract cannot (`t <= 13`).
    if z < 6 && t < 32 && t <= br.valid {
        let f = br.top(t);
        br.take(t);
        br.refill();
        f.wrapping_sub(1u32 << k)
    } else {
        let (next, v) = eg_slow(*br, k);
        *br = next;
        v
    }
}

/// EG(k) bit by bit: the unary zero count, the terminating one, the `z` value
/// bits, then `k` raw low bits. Mirrors `uli_lite::bitstream::BitReader::eg`.
///
/// Takes the reader by value and returns it: that is what keeps `&mut BitReader`
/// out of the product path, so the window stays in registers across this call.
///
/// Walks the bits with the **hot** primitives (`refill`/`take`/`top`) rather
/// than the general `peek`/`skip`. Measured, that is 1,820 B → ~300 B of
/// `.iwram`: the general pair inlines `peek`, `skip`, `set_pos` and `pos` at
/// every step, and this is the coldest code in the decoder, so its size matters
/// more than its latency (U6 is the binding memory constraint). Moving it to the
/// cartridge instead measured **+22,657 cycles on the worst frame** — a long
/// escape prefix is rare but not cheap, so it stays in IWRAM, just small.
#[inline(never)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
fn eg_slow(mut br: BitReader, k: u32) -> (BitReader, u32) {
    // One bit, refilling first. `bad` (a truncated stream) stops the walk; the
    // caller rejects the frame anyway.
    macro_rules! bit {
        () => {{
            br.refill();
            if br.bad {
                return (br, 0);
            }
            let b = br.hw >> 31;
            br.take(1);
            b
        }};
    }
    let mut z = 0u32;
    while bit!() == 0 {
        z += 1;
        if z > 24 {
            br.bad = true;
            return (br, 0);
        }
    }
    let mut v = 0u32;
    for _ in 0..z {
        v = (v << 1) | bit!();
    }
    v = v.wrapping_add((1u32 << z) - 1);
    if k != 0 {
        let mut lo = 0u32;
        for _ in 0..k {
            lo = (lo << 1) | bit!();
        }
        v = v.wrapping_shl(k) | lo;
    }
    br.refill();
    (br, v)
}

/// Decode one canonical symbol from the buffered window.
///
/// Pure: it reads `hw` and returns `(entry, len)` where `entry` is the LUT value
/// (symbol in the high bits, code length in the low five) and `len` is the code
/// length. **`len == 0` means no code matched** — [`huff_slow`] returns 0 — and
/// the caller turns that into `bad` and a `1` sentinel entry.
///
/// It takes `hw` by value rather than the reader because the hot loop keeps the
/// window in `let mut` scalars (see `dec_chan`): anything that takes a
/// `&mut BitReader` on that path puts the 24-byte struct back in an alloca and
/// every refill stores four fields to memory. `uli_lite_gba`'s C reference gets
/// the same effect with `REFILL`/`SKIP` as macros; this is that shape.
///
/// Requires `valid >= 17` (the hot contract) and consumes nothing.
///
/// The long-code search stays out of line: full inlining measured +2.7% for
/// 4,252 B of IWRAM and was rejected on the memory gate (PR C); the split-LUT
/// path is +1.6% for 896 B.
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn huff_at(hw: u32, lut: *const u16, band: &Huffman, ops: &mut Ops) -> (u16, u32) {
    // SAFETY: the tables parser guarantees `1 << LUTB` entries in `lut`.
    let e = unsafe { *lut.add((hw >> (32 - LUTB)) as usize) };
    if e != 0 {
        return (e, (e & 31) as u32);
    }
    let s = huff_slow(hw, band);
    if s == 0 {
        return (1, 0);
    }
    count_long(ops);
    count_slow_hit();
    (s, (s & 31) as u32)
}

/// One coefficient symbol from a pair/quad class table.
///
/// In the product build this is just [`huff_at`]. The sub-frame attribution arm
/// (`attrib-entropy-nolut`) substitutes a fixed 5-bit read, so symbol
/// *resolution* is gone while the pair/quad loops, the escape shape, the raw
/// bits, the dequant and the stores all stay. Deliberately not applied to the
/// band-class symbol: a constant class collapsed every band to zero and the arm
/// then measured nothing (measured — the first version of this arm substituted
/// the whole symbol decode and the path came back at ~1,400 cycles).
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
fn coef_huff_at(hw: u32, lut: *const u16, cls: &Huffman, ops: &mut Ops) -> (u16, u32) {
    #[cfg(feature = "attrib-entropy-nolut")]
    {
        let _ = (hw, lut, cls, ops);
        // Symbol 1: digit 0 then digit 1, i.e. one nonzero coefficient per pair.
        (0x20, 5)
    }
    #[cfg(not(feature = "attrib-entropy-nolut"))]
    {
        huff_at(hw, lut, cls, ops)
    }
}

/// The canonical-code search for a symbol longer than the LUT covers.
///
/// Pure: it reads the already-buffered window and returns 0 when no code
/// matches, which the caller turns into `bad`. Keeping the reader out of it is
/// the largest single change in Step 1 — 13.4% of the worst frame's symbols take
/// this path, and the previous version `peek`ed up to sixteen times with each
/// `peek` able to refill, bounds-check and re-derive the window.
#[inline(never)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
fn huff_slow(hw: u32, p: &Huffman) -> u16 {
    // The five slice views are built here, off the hot path: this function is
    // the only consumer, and keeping them live across the band loop was the
    // register pressure that spilled the window (see `lut_ptr` in `uli-lite`).
    let p = p.parts();
    let mut l = LUTB + 1;
    while l <= MAXLEN {
        // SAFETY: l <= MAXLEN < p.first/count/offs.len(), and l <= 16 <= valid.
        let c = (hw >> (32 - l)).wrapping_sub(unsafe { *p.first.get_unchecked(l) } as u32);
        if c < unsafe { *p.count.get_unchecked(l) } as u32 {
            let i = unsafe { *p.offs.get_unchecked(l) } as usize + c as usize;
            // SAFETY: the canonical parser guarantees offs + count <= 256.
            let sym = unsafe { *p.sym.get_unchecked(i) } as u32;
            return ((sym << 5) | l as u32) as u16;
        }
        l += 1;
    }
    0
}

/// Write one zero band: 16 coefficients, four words at a time.
#[inline(always)]
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
unsafe fn zero16(p: *mut i32) {
    // `stmia`, in asm: the reference's `zero16` is two zeroed registers and four
    // `stmia`. The Rust `[i32; 4]` stores below were instead lowered to a
    // **call** to `__aeabi_memclr4` (an ARM→Thumb veneer, a push/pop and a
    // loop), because both the in-band fill and the post-EOB tail went through
    // it — so a zero band cost a call frame and a loop.
    //
    // Eight `stmia` of one register pair rather than four of two pairs: the
    // 4-register form is 4 instructions instead of 8, but it clobbers `r3` and
    // `r12` as well, and `r12` is LLVM's scratch in ARM mode. Measured, the
    // 2-register form is faster on the worst frame (210,833 vs 210,967) — this
    // kernel's cost is dominated by what the clobbers do to the *surrounding*
    // register allocation, not by its own instruction count. Bit-identical
    // either way.
    #[cfg(target_arch = "arm")]
    unsafe {
        core::arch::asm!(
            "mov r1, #0",
            "mov r2, #0",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            "stmia {p}!, {{r1, r2}}",
            p = inout(reg) p => _,
            out("r1") _, out("r2") _,
            options(nostack),
        );
    }
    // Host build (and anything else): the portable form. Bit-identical.
    #[cfg(not(target_arch = "arm"))]
    unsafe {
        let mut i = 0;
        while i < BANDW {
            p.add(i).cast::<[i32; 4]>().write([0; 4]);
            i += 4;
        }
    }
}

/// One channel: band classes, tuples, escapes, dequantisation.
///
/// Structurally the reference's `dec_chan`, and shaped like it in the one way
/// that turned out to matter: **the bit window lives in `let mut` scalars**, and
/// `refill`/`take`/`top`/`esc` are macros over them rather than methods on a
/// struct. Measured, that is what the C reference gets from its `REFILL`/`SKIP`
/// macros, and it is the only shape that did not put the 24-byte reader back in
/// an alloca — three ABI/promotion experiments left every `refill` storing
/// `next`/`hw`/`valid`/`bad` through a base register.
///
/// The rest is the reference's shape too: the band-class LUT pointer is derived
/// once per band, the per-band live set is one `rq` pointer plus `dq15`, and
/// coefficients are written straight into `x` as they decode, so nothing is
/// pre-zeroed and every coefficient is written exactly once per frame.
///
/// Returns the number of bands before EOB, which the caller uses to bound M/S,
/// and the reader. `bad` signals a rejected stream; the caller discards the
/// frame, so a partially written `x` is never observable.
#[cfg_attr(target_arch = "arm", instruction_set(arm::a32))]
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
#[inline(never)]
#[allow(unused_variables, clippy::too_many_arguments)]
unsafe fn dec_chan(
    tables: &CodeTables,
    class: &ClassTables,
    x: *mut i32,
    step: u32,
    br: BitReader,
    ops: &mut Ops,
    diag: &mut Diag,
) -> (usize, BitReader) {
    // The window, in scalars. `base` is only touched by the cold escape path.
    let BitReader {
        mut base,
        mut next,
        end,
        mut hw,
        mut valid,
        mut bad,
    } = br;

    #[cfg(feature = "verify")]
    // SAFETY: NCODED <= M, so this is inside the channel.
    let xn = unsafe { x.add(NCODED) };

    #[cfg(feature = "verify")]
    // SAFETY: NCODED <= M, so this is inside the channel.
    let xn = unsafe { x.add(NCODED) };

    /// Ensure the window holds a whole Huffman code (or, in a checked build, set
    /// `bad`).
    ///
    /// **Two shapes, selected by `checked-reader`.** The product build (the
    /// default) is *closed-world*: we own the encoder, the container and the
    /// bake, and the bake gate decodes the whole album with the **checked**
    /// portable decoder against the C-pinned hashes before the ROM exists, so a
    /// malformed stream fails the build instead of reaching this loop. The
    /// product therefore carries **no bounds test and no error flag**, which is
    /// what keeps `end` and `bad` out of the inner loops — and those two words
    /// are exactly the registers `rq`/`dq15` need. (P2 moved `rq` to IWRAM and
    /// still had to reload its *pointer* from the stack, for want of them.)
    ///
    /// **The contract is 16 bits, not 17.** A raw pair digit reads `r` bits and
    /// then a sign bit, which is why the old shape demanded 17; but the raw arm
    /// refills again between the two coefficients of the group (see the pair loop
    /// below), so each digit's `r + 1` is measured from its own window, and on
    /// every corpus this tree owns the eleven PAIR classes have `rbits <= 5`
    /// (`0,0,0,0,0,0,1,2,3,4,5` — `tests/premise.rs` asserts the bound and prints
    /// the census; `esck <= 2` keeps the escape field at `t <= 13`). 16 also
    /// makes **one load always sufficient** (`valid < 16` implies
    /// `valid + 16 >= 16`), so the nested second load disappears with it.
    #[cfg(not(feature = "checked-reader"))]
    macro_rules! refill {
        () => {{
            if valid < 16 {
                // SAFETY (closed world): `valid < 16` and the stream is
                // well-formed and halfword-padded, so `next` has a readable
                // halfword. A malformed stream would read past the buffer; that
                // is accepted by policy, because the boundary that guarantees
                // well-formedness is the bake gate, not the decoder.
                let h = unsafe { BitReader::load(next) };
                next = next.wrapping_add(2);
                hw |= h << (16 - valid);
                valid += 16;
            }
        }};
    }

    /// The checked refill: 17 valid bits, a bounds test on every load and `bad`
    /// on truncation. Used by the attribution arms (which *desynchronise* the
    /// stream by removing a stage and need `bad` to resynchronise) and by
    /// `checked-reader` builds.
    ///
    /// **The bounds check stays, and it was measured.** Three shapes were tried
    /// here in the 2026-11-16 sweep and this is the fastest:
    /// 1. *branchless clamp* — pick `min(next, end - 2)` with a predicated move
    ///    and detect truncation once per channel: defeats the taken branch, but
    ///    the load address now depends on `end`, which puts an extra link in the
    ///    critical path ahead of every load. **Measured +1,352 cycles/frame**
    ///    against this shape, so it is not the win the branch count suggests.
    /// 2. *`bad` updated per refill* (`bad |= over`): the flag stops being
    ///    written only on the error path and becomes live across the whole
    ///    loop. Measured worse again, for the register pressure.
    /// 3. this one. The check is a compare and a branch, and the branch is
    ///    taken on the common path — but the address chain stays 1 instruction.
    #[cfg(feature = "checked-reader")]
    macro_rules! refill {
        () => {{
            if valid < 17 {
                if next.wrapping_add(2) > end {
                    bad = true;
                } else {
                    // SAFETY: the check above leaves one halfword readable.
                    let h = unsafe { BitReader::load(next) };
                    next = next.wrapping_add(2);
                    hw |= h << (16 - valid);
                    valid += 16;
                    if valid < 17 {
                        if next.wrapping_add(2) > end {
                            bad = true;
                        } else {
                            // SAFETY: as above; `valid` was 0, so the shift is 0.
                            let h = unsafe { BitReader::load(next) };
                            next = next.wrapping_add(2);
                            hw |= h;
                            valid += 16;
                        }
                    }
                }
            }
        }};
    }

    /// Consume `n` bits. Branchless, and requires `n <= valid` (established by
    /// `refill!` at every site).
    macro_rules! take {
        ($n:expr) => {{
            let n = $n;
            debug_assert!(n <= valid, "take past the window");
            hw <<= n;
            valid -= n;
        }};
    }

    /// The top `n` bits, not consumed. Requires `1 <= n <= valid`.
    macro_rules! top {
        ($n:expr) => {{ hw >> (32 - $n) }};
    }

    /// EG(k) with the reference's fast path: when the unary prefix has at most
    /// five zeros, the terminating one, the `z` value bits and the `k` raw low
    /// bits form one `2z + 1 + k`-bit field `F`, and `EG = F - 2^k`. Rare long
    /// prefixes hand the reader to the out-of-line [`eg_slow`] and take it back.
    macro_rules! esc {
        ($k:expr) => {{
            refill!();
            let k = $k;
            let z = CLZ6[(hw >> 26) as usize] as u32;
            let t = 2 * z + 1 + k;
            // Closed world: `esck <= 2` on every corpus this tree owns (see
            // `tests/premise.rs`), so `t <= 2*5 + 1 + 2 = 13` and both `t < 32`
            // and `t <= valid` are unfalsifiable — the leading `refill!` has
            // already put 16 bits in the window. The product drops the two
            // compares; the checked build keeps them.
            #[cfg(not(feature = "checked-reader"))]
            let fast = z < 6;
            #[cfg(feature = "checked-reader")]
            let fast = z < 6 && t < 32 && t <= valid;
            if fast {
                let f = top!(t);
                take!(t);
                refill!();
                f.wrapping_sub(1u32 << k)
            } else {
                let tmp = BitReader {
                    base,
                    next,
                    end,
                    hw,
                    valid,
                    bad,
                };
                let (nb, v) = eg_slow(tmp, k);
                base = nb.base;
                next = nb.next;
                hw = nb.hw;
                valid = nb.valid;
                bad = nb.bad;
                v
            }
        }};
    }

    /// One symbol, consumed. `len == 0` from the pure decode means the stream is
    /// malformed: set `bad` and leave the window alone.
    macro_rules! huff_consume {
        ($decode:expr) => {{
            let (e, len) = $decode;
            // Closed world: `len` is a LUT entry's length (1..=10) or a
            // `huff_slow` match (11..=16). It is 0 only when a symbol matched no
            // canonical code, which the bake gate rules out before the ROM
            // exists — so the product pays no per-symbol test and the checked
            // build keeps one.
            #[cfg(not(feature = "checked-reader"))]
            take!(len);
            #[cfg(feature = "checked-reader")]
            {
                if len == 0 {
                    bad = true;
                } else {
                    take!(len);
                }
            }
            e
        }};
    }

    /// One coefficient of a pair/quad group, straight into its slot.
    ///
    /// `$raw` is `true` or `false` as a literal so the `rbits == 0` and
    /// `rbits != 0` cases are two specialisations rather than a per-coefficient
    /// branch on a loop-invariant value. `$esc_d` is the digit that escapes: 15
    /// for a pair, 2 for a quad — the reference's rule, which for a quad is
    /// `== 2` and not C's `else if (m)`, so a digit of 3 never escapes.
    ///
    /// The store is unconditional (zero included), so there is no branch on the
    /// common zero digit, and the sign bit is read only when the magnitude is
    /// nonzero — the reference's order (`mm != 0 && get(1)`).
    macro_rules! coef {
        ($raw:literal, $slot:expr, $digit:expr, $esc_d:expr, $r:expr, $rqp:expr, $dq15:expr,
         $step:expr, $esck:expr) => {{
            let mut m = $digit;
            if m == $esc_d {
                m = m.wrapping_add(esc!($esck));
                count_escape(ops);
            }
            let mm = if $raw {
                let v = m.wrapping_shl($r) | top!($r);
                take!($r);
                count_raw(ops);
                v
            } else {
                m
            };
            let mut v = 0i32;
            if mm != 0 {
                // NOTE: C hoists `rq[1] * step` and special-cases `m == 1` here.
                // Measured in this image, that hoist is not a win — its two
                // operands are spilled, so the extra branch costs more than the
                // EWRAM `rq` load it avoids (mean 219,747 with vs 220,757
                // without, but worst frame 250,945 with vs 249,835 without, and
                // the gate reads the worst frame). Not taken; see
                // RECORD.md "Step 1".
                #[cfg(feature = "attrib-entropy-nodequant")]
                let mag = {
                    let _ = ($rqp, $dq15, $step);
                    mm as i32
                };
                #[cfg(not(feature = "attrib-entropy-nodequant"))]
                let mag = if mm < 16 {
                    // SAFETY: the branch constrains `mm < 16`.
                    unsafe { *$rqp.add(mm as usize) as i32 }.wrapping_mul($step)
                } else {
                    ((mm as i32) << 4).wrapping_sub($dq15).wrapping_mul($step)
                };
                #[cfg(feature = "verify")]
                if mag >= XLIM as i32 {
                    note_xover(diag);
                }
                // Sign off the window's top bit, then one shift: no separate peek.
                let neg = (hw as i32) < 0;
                take!(1);
                v = if neg { mag.wrapping_neg() } else { mag };
            }
            // Sub-frame attribution (`attrib-entropy-nowrite`): the arithmetic
            // above still runs but the coefficient is not stored; `black_box`
            // keeps the value alive so the work cannot be deleted.
            #[cfg(feature = "attrib-entropy-nowrite")]
            core::hint::black_box(v);
            // SAFETY: the caller guarantees `$slot` is a coefficient slot.
            #[cfg(not(feature = "attrib-entropy-nowrite"))]
            unsafe {
                $slot.write(v)
            };
            if mm == 0 {
                count_zero(ops);
            } else {
                count_nonzero(ops);
                // The format rule is "nonzero at index >= NCODED"; with cursor
                // addressing that is a pointer compare against `x + NCODED`
                // rather than an index, which is what lets the loops below stop
                // carrying a coefficient counter.
                #[cfg(feature = "verify")]
                if $slot >= xn {
                    note_hfnz(diag);
                }
            }
        }};
    }

    let k = tables.k;
    // A rejected stream never reaches the band loop: `decode_frame` checks
    // `bad` after the frame header and before this call, so this is a
    // belt-and-braces early-out that keeps the reader's preconditions local to
    // this function instead of an unstated property of the caller. One compare
    // per channel, not per symbol.
    if bad || k > MAXK {
        bad = true;
        return (
            0,
            BitReader {
                base,
                next,
                end,
                hw,
                valid,
                bad,
            },
        );
    }
    let step32 = step as i32;
    let mut ctx = k;
    let mut b = 0usize;
    // A cursor over the band's 16 slots instead of `x.add(b * BANDW)` recomputed
    // per band: `b` survives only as the band counter and as dead weight in a
    // product build (the `verify` index went away with the pointer compare
    // above).
    let mut xc = x;
    while b < NBANDS {
        let band = tables.band_table(ctx);
        let blut = band.lut_ptr();
        refill!();
        let e = huff_consume!(huff_at(hw, blut, band, ops));
        count_band_symbol(ops);
        if bad {
            break;
        }
        let c = (e >> 5) as usize;
        if c == k {
            break;
        }
        if c >= MAXK {
            bad = true;
            break;
        }
        ctx = c;
        // SAFETY: b < NBANDS, so the cursor's 16 slots are inside the channel.
        let xp = xc;
        // SAFETY: c < MAXK == ctype.len().
        let ct = unsafe { *class.ctype.get_unchecked(c) };
        if ct == CT_ZERO {
            // SAFETY: as above.
            unsafe {
                zero16(xp);
                xc = xp.add(BANDW);
            }
            b += 1;
            continue;
        }
        count_coded_band(ops);
        let cls = tables.class_table(c);
        let clut = cls.lut_ptr();
        // SAFETY: c < MAXK, and the parser validated every per-class table.
        let rq = unsafe { class.rq.get_unchecked(c) };
        // SAFETY: `rq` has 16 entries.
        let dq15 = 16 * 15 - unsafe { *rq.get_unchecked(15) } as i32;
        // SAFETY: c < MAXK == esck.len().
        let esck = unsafe { *class.esck.get_unchecked(c) } as u32;
        let rqp = rq.as_ptr();
        // A cursor over this band's slots. `xf` is one past the end, so the
        // loops below compare pointers instead of carrying a coefficient
        // counter — the shape that paid ~20% in `pre` and `ola`.
        let mut sc = xp;
        // SAFETY: the band is 16 slots inside the channel.
        let xf = unsafe { xp.add(BANDW) };
        if ct == CT_PAIR {
            // SAFETY: c < MAXK == rbits.len().
            let r = unsafe { *class.rbits.get_unchecked(c) } as u32;
            if r == 0 {
                while sc < xf {
                    refill!();
                    let e = huff_consume!(coef_huff_at(hw, clut, cls, ops));
                    count_pair(ops);
                    refill!();
                    let s = (e >> 5) as u32;
                    // SAFETY: `sc` and `sc + 1` are both inside the band.
                    coef!(false, sc, s >> 4, 15, 0, rqp, dq15, step32, esck);
                    coef!(
                        false,
                        unsafe { sc.add(1) },
                        s & 15,
                        15,
                        0,
                        rqp,
                        dq15,
                        step32,
                        esck
                    );
                    // SAFETY: two slots consumed.
                    sc = unsafe { sc.add(2) };
                }
            } else {
                while sc < xf {
                    refill!();
                    let e = huff_consume!(coef_huff_at(hw, clut, cls, ops));
                    count_pair(ops);
                    refill!();
                    let s = (e >> 5) as u32;
                    // SAFETY: `sc` is inside the band.
                    coef!(true, sc, s >> 4, 15, r, rqp, dq15, step32, esck);
                    // A raw field plus a sign bit can consume the whole contract
                    // window (16 + 1), so re-establish it before the second
                    // coefficient of the group.
                    refill!();
                    // SAFETY: `sc + 1` is inside the band.
                    coef!(
                        true,
                        unsafe { sc.add(1) },
                        s & 15,
                        15,
                        r,
                        rqp,
                        dq15,
                        step32,
                        esck
                    );
                    // SAFETY: two slots consumed.
                    sc = unsafe { sc.add(2) };
                }
            }
        } else {
            while sc < xf {
                refill!();
                let e = huff_consume!(coef_huff_at(hw, clut, cls, ops));
                count_quad(ops);
                refill!();
                // SAFETY: a CT_QUAD class table has exactly 81 symbols.
                let q3 = unsafe { *Q3.get_unchecked((e >> 5) as usize) };
                let mut kk = 0usize;
                while kk < 4 {
                    let m = ((q3 >> (6 - 2 * kk)) & 3) as u32;
                    // SAFETY: `sc + kk` is inside the band.
                    coef!(
                        false,
                        unsafe { sc.add(kk) },
                        m,
                        2,
                        0,
                        rqp,
                        dq15,
                        step32,
                        esck
                    );
                    kk += 1;
                }
                // SAFETY: four slots consumed.
                sc = unsafe { sc.add(4) };
            }
        }
        // SAFETY: one band decoded; step the band cursor.
        xc = unsafe { xp.add(BANDW) };
        b += 1;
    }
    // Tail after EOB: whole bands of zeros, so no coefficient is ever stale.
    while b < NBANDS {
        // SAFETY: b < NBANDS.
        unsafe {
            zero16(xc);
            xc = xc.add(BANDW);
        }
        b += 1;
    }
    (
        b,
        BitReader {
            base,
            next,
            end,
            hw,
            valid,
            bad,
        },
    )
}

/// The product decoder: same output as `uli_lite::Decoder`, GBA-first hot path.
pub struct GbaDecoder<'a, 's> {
    album: Album<'a>,
    tables: &'s CodeTables,
    /// The IWRAM-resident per-class tables. Read on every nonzero coefficient,
    /// so it is a separate object from `tables` for placement reasons only.
    class: &'s ClassTables,
    hot: &'s mut HotState,
    bitpos: u32,
    frame: u32,
    g_prev: i32,
    ops: Ops,
    xover: u32,
    hfnz: u32,
    /// Sub-frame attribution only (`attrib-entropy-reader`): the next frame's
    /// true bit count, replayed by the reader in place of the band loop.
    #[cfg(feature = "attrib-entropy-reader")]
    replay: Option<u32>,
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
    /// Parse `album`'s tables into `tables`, copy the per-class slice into
    /// `class`, reset `hot`, and position at frame 0. The caller owns the
    /// storage: on the GBA, `tables` belongs in EWRAM and `class` + `hot` in
    /// IWRAM.
    pub fn new(
        album: Album<'a>,
        tables: &'s mut CodeTables,
        class: &'s mut ClassTables,
        hot: &'s mut HotState,
    ) -> Result<Self, Error> {
        // The bit reader loads the stream with aligned `u16` reads (one `ldrh`
        // per refill instead of two `ldrb` + a shift-or). That is sound only for
        // a halfword-aligned stream, which the container guarantees as an even
        // *offset* (`Album::parse` rejects `stream_off & 1`) but cannot
        // guarantee as an *address* — `include_bytes!` has alignment 1. Checked
        // once here rather than per load, and named rather than reported as a
        // bitstream error: see `uli_lite::Aligned` for the fix.
        if (album.stream().as_ptr() as usize) & 1 != 0 {
            return Err(Error::UnalignedStream);
        }
        tables.parse_into(album.tables())?;
        // The hot per-class slice moves to IWRAM here: one copy per album, so
        // the coefficient loops stop paying EWRAM wait states on `rq[m]`.
        class.fill(tables);
        hot.reset();
        let tables: &'s CodeTables = tables;
        let class: &'s ClassTables = class;
        Ok(Self {
            album,
            tables,
            class,
            hot,
            bitpos: 0,
            frame: 0,
            g_prev: 0,
            ops: Ops::ZERO,
            xover: 0,
            hfnz: 0,
            #[cfg(feature = "attrib-entropy-reader")]
            replay: None,
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

    /// Operation counts for the last frame. Zero unless built with `verify` (PR C-3b).
    pub fn ops(&self) -> Ops {
        self.ops
    }

    /// Cumulative `|X|`-bound violations. Zero unless built with `verify` (PR C-3b).
    pub fn xover(&self) -> u32 {
        self.xover
    }

    /// Cumulative nonzero coefficients past `NCODED`. Zero unless built with `verify` (PR C-3b).
    pub fn hfnz(&self) -> u32 {
        self.hfnz
    }

    /// Sub-frame attribution only (`attrib-entropy-reader`): tell the next
    /// [`Self::decode_frame`] call the bit count the real band decoder consumed
    /// for that frame, so the reader's refill/skip mechanics can be timed at the
    /// true bit volume with no symbol work at all. Inert in the product build.
    #[cfg(feature = "attrib-entropy-reader")]
    pub fn replay_bits(&mut self, bits: u32) {
        self.replay = Some(bits);
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
        // Timing-only arms read headers from a desynchronised position, so they
        // clamp instead of rejecting; the product build rejects (PR: rejection
        // only, no arithmetic promise for malformed streams).
        #[cfg(feature = "attrib-timing")]
        let geff = if geff < 0 {
            0
        } else if geff >= NSTEP as i32 {
            NSTEP as i32 - 1
        } else {
            geff
        };
        #[cfg(not(feature = "attrib-timing"))]
        if geff < 0 || geff >= NSTEP as i32 {
            return Err(Error::BadFrame);
        }
        // SAFETY: geff is in 0..NSTEP (guarded in the product build, clamped in
        // a timing arm).
        let step = unsafe { *STEP.get_unchecked(geff as usize) };
        #[cfg(feature = "verify")]
        {
            ops.ms = ms as u32;
        }
        #[cfg(feature = "attrib-timing")]
        if br.bad {
            br.bad = false;
        }
        #[cfg(not(feature = "attrib-timing"))]
        if br.bad {
            return Err(Error::BadBitstream);
        }

        // by-difference attribution arm (`attrib-entropy`): the whole band
        // decoder is removed, so entropy, dequant, zero-band fills and (via
        // `nb = 0`) the M/S loop all disappear. Timing only; the samples are
        // no longer the album, which is why the bench's pins fail by design in
        // that build.
        #[cfg(feature = "attrib-entropy-reader")]
        let (nb0, nb1) = {
            let _ = (&mut ops, &mut diag, step);
            // Replay the frame's true bit count in 16-bit chunks: one refill and
            // one skip for each, i.e. the same window mechanics the band decoder
            // performs, at the same bit volume, with no symbols or coefficients.
            let mut left = self.replay.take().unwrap_or(0);
            while left > 0 {
                br.refill();
                let take = if left > 16 { 16 } else { left };
                br.take(take);
                left -= take;
            }
            (0usize, 0usize)
        };
        #[cfg(all(feature = "attrib-entropy", not(feature = "attrib-entropy-reader")))]
        let (nb0, nb1) = {
            let _ = (&mut br, &mut ops, &mut diag, step);
            (0usize, 0usize)
        };
        #[cfg(not(any(feature = "attrib-entropy", feature = "attrib-entropy-reader")))]
        let (nb0, nb1) = {
            let tables = self.tables;
            let class = self.class;
            let hot = &mut *self.hot;
            // SAFETY: each row is M coefficients and `dec_chan` is band-bounded.
            // The reader goes in and comes back by value so the channel decode
            // owns it outright: a `&mut` handed to an out-of-line function pins
            // it in memory, and the measured cost of that was a store of
            // `next`/`hw`/`valid`/`bad` on every refill and every skip.
            unsafe {
                #[cfg(feature = "attrib-entropy2")]
                let br0 = br;
                let (nb0, next) = dec_chan(
                    tables,
                    class,
                    hot.x[0].as_mut_ptr(),
                    step,
                    br,
                    &mut ops,
                    &mut diag,
                );
                br = next;
                // `attrib-entropy2`: replay the channel from the saved reader.
                // `dec_chan` is a deterministic function of (tables, reader,
                // step), so the replay writes byte-identical coefficients and
                // returns the same cursor — bit-exact, and the delta is exactly
                // one channel decode. (`Ops` counters double-count in this arm;
                // they are product-build no-ops, and the arm is never built with
                // `verify`.)
                #[cfg(feature = "attrib-entropy2")]
                {
                    let (_, next) = dec_chan(
                        tables,
                        class,
                        hot.x[0].as_mut_ptr(),
                        step,
                        br0,
                        &mut ops,
                        &mut diag,
                    );
                    br = next;
                }
                #[cfg(feature = "attrib-entropy2")]
                let br1 = br;
                let (nb1, next) = dec_chan(
                    tables,
                    class,
                    hot.x[1].as_mut_ptr(),
                    step,
                    br,
                    &mut ops,
                    &mut diag,
                );
                br = next;
                #[cfg(feature = "attrib-entropy2")]
                {
                    let (_, next) = dec_chan(
                        tables,
                        class,
                        hot.x[1].as_mut_ptr(),
                        step,
                        br1,
                        &mut ops,
                        &mut diag,
                    );
                    br = next;
                }
                (nb0, nb1)
            }
        };
        // A timing-only arm removes a layer, which desynchronises the bit stream.
        // Rejecting the frame would abort the run instead of measuring it, so the
        // arm resynchronises a short fixed distance and continues; the product
        // build keeps the rejection (the `not` branch below).
        #[cfg(feature = "attrib-timing")]
        let resync = if br.bad {
            br.bad = false;
            true
        } else {
            false
        };
        #[cfg(not(feature = "attrib-timing"))]
        if br.bad {
            return Err(Error::BadBitstream);
        }

        #[cfg(not(feature = "attrib-ms"))]
        if ms != 0 {
            // The reference transforms only the coded coefficients
            // (`nb = max(nb0, nb1)`); everything past them is zero in both
            // channels, so the result is identical and the loop is shorter
            // (PR C-3a).
            let n = if nb0 > nb1 { nb0 } else { nb1 } * BANDW;
            let l = self.hot.x[0].as_mut_ptr();
            let r = self.hot.x[1].as_mut_ptr();
            // SAFETY: l and r are the two M-length rows, n <= M, and `n` is a
            // multiple of BANDW = 16, which is the kernel's alignment/grouping
            // contract. Zero bands are handled by the kernel doing nothing.
            unsafe {
                crate::ms_kernel(l, r, n);
            }
        }

        // The eight synthesis calls. Each is individually removable for the
        // by-difference attribution arms (`attrib-pre`/`-fft`/`-post`/`-ola`,
        // and `attrib-synth` which implies all four). In the default build the
        // `cfg` folds away and this is exactly the product sequence.
        #[cfg(not(feature = "attrib-pre"))]
        crate::pre_kernel(&self.hot.x[0], &mut self.hot.buf);
        // `attrib-pre2`: a *second* `pre` call. Idempotent, so `z` is unchanged
        // and the FFT sees the same data — the delta is one real `pre`, with none
        // of `attrib-pre`'s stale-`z` contamination. Bit-exact, so pins PASS.
        #[cfg(feature = "attrib-pre2")]
        crate::pre_kernel(&self.hot.x[0], &mut self.hot.buf);
        #[cfg(not(feature = "attrib-fft"))]
        crate::fft_kernel(&mut self.hot.buf);
        // `attrib-fft2`: `fft` is in place, so the second call needs its input put
        // back. `pre` is idempotent into `z`, so calling it again rewrites exactly
        // the bytes the first `fft` consumed; the second `fft` then produces
        // identical output and every later stage sees the data it would have.
        // Bit-exact, so pins PASS — but the delta is one `pre` **plus** one `fft`;
        // subtract `attrib-pre2`'s delta to read the FFT alone.
        #[cfg(feature = "attrib-fft2")]
        {
            crate::pre_kernel(&self.hot.x[0], &mut self.hot.buf);
            crate::fft_kernel(&mut self.hot.buf);
        }
        #[cfg(not(feature = "attrib-post"))]
        crate::post_kernel(&self.hot.buf, &mut self.hot.x[0]);
        // `attrib-post2`: `post` reads `z` and writes `y`, so a second call
        // writes the same bytes and every later stage sees the data it would
        // have. Delta = one real `post`; bit-exact.
        #[cfg(feature = "attrib-post2")]
        crate::post_kernel(&self.hot.buf, &mut self.hot.x[0]);
        #[cfg(not(feature = "attrib-ola"))]
        crate::ola_kernel(&self.hot.x[0], &mut self.hot.p[0], out8l, out16l);

        #[cfg(not(feature = "attrib-pre"))]
        crate::pre_kernel(&self.hot.x[1], &mut self.hot.buf);
        #[cfg(feature = "attrib-pre2")]
        crate::pre_kernel(&self.hot.x[1], &mut self.hot.buf);
        #[cfg(not(feature = "attrib-fft"))]
        crate::fft_kernel(&mut self.hot.buf);
        // `attrib-fft2`, channel 1: same restore-then-repeat as channel 0 above.
        #[cfg(feature = "attrib-fft2")]
        {
            crate::pre_kernel(&self.hot.x[1], &mut self.hot.buf);
            crate::fft_kernel(&mut self.hot.buf);
        }
        #[cfg(not(feature = "attrib-post"))]
        crate::post_kernel(&self.hot.buf, &mut self.hot.x[1]);
        // `attrib-post2`: `post` reads `z` and writes `y`, so a second call
        // writes the same bytes and every later stage sees the data it would
        // have. Delta = one real `post`; bit-exact.
        #[cfg(feature = "attrib-post2")]
        crate::post_kernel(&self.hot.buf, &mut self.hot.x[1]);
        #[cfg(not(feature = "attrib-ola"))]
        crate::ola_kernel(&self.hot.x[1], &mut self.hot.p[1], out8r, out16r);

        #[cfg(feature = "attrib-ms")]
        let _ = (nb0, nb1, ms);

        self.bitpos = br.bitpos();
        #[cfg(feature = "attrib-timing")]
        if resync {
            self.bitpos = start.wrapping_add(256);
        }
        // The whole diagnostic block is product-build dead code: `Diag` is empty
        // and the counters are no-ops there (PR C-3b). The reference twin builds
        // with `verify` and compares every field.
        #[cfg(feature = "verify")]
        let (info_ops, info_cycles, info_xover, info_hfnz) = {
            self.ops = ops;
            self.xover = self.xover.wrapping_add(diag.xover);
            self.hfnz = self.hfnz.wrapping_add(diag.hfnz);
            (ops, ops.cycle_model(), diag.xover, diag.hfnz)
        };
        #[cfg(not(feature = "verify"))]
        let (info_ops, info_cycles, info_xover, info_hfnz) = {
            let _ = (&ops, &diag);
            (Ops::ZERO, 0, 0, 0)
        };
        let frame = self.frame;
        self.frame = self.frame.wrapping_add(1);
        Ok(FrameInfo {
            frame,
            g: self.g_prev,
            bits: self.bitpos.wrapping_sub(start),
            bitpos: self.bitpos,
            ops: info_ops,
            cycles: info_cycles,
            xover: info_xover,
            hfnz: info_hfnz,
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

    // Halfword-aligned: the product reader uses aligned `u16` loads (see
    // `uli_lite::Aligned`), and this blob is odd-based without the wrapper.
    static BLOB_STORAGE: uli_lite::Aligned<
        { include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli").len() },
    > = uli_lite::Aligned(*include_bytes!(
        "../../uli-lite/tests/fixtures/uli2_tiny.uli"
    ));
    const BLOB: &[u8] = &BLOB_STORAGE.0;

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
