//! ULI2 frame decoder: bitstream, dequantisation, M/S, and synthesis.
//!
//! The normative arithmetic is `uli_dec_frame` / `dec_chan` in the C reference.
//! This module keeps the same state split: caller-owned [`DecoderState`] holds
//! the code tables and the hot transform/lapping buffers; [`Decoder`] holds the
//! album borrow and decodes into caller-owned output slices.

use core::fmt;
use core::marker::PhantomData;

use crate::Error;
use crate::album::Album;
use crate::bitstream::BitReader;
use crate::format::{BANDW, CT_PAIR, CT_ZERO, H, M, MAXK, NBANDS, NCODED, NSTEP, XLIM};
use crate::imdct;
use crate::tables::CodeTables;
use crate::tables_data::STEP;

/// Per-frame operation counts. These are the inputs to the reference cycle
/// model and are useful for measurement; they do not affect decoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ops {
    pub bandsyms: u32,
    pub coded_bands: u32,
    pub pairs: u32,
    pub quads: u32,
    pub nz: u32,
    pub zc: u32,
    pub raws: u32,
    pub escs: u32,
    pub ms: u32,
    pub long_codes: u32,
}

impl Ops {
    pub const ZERO: Self = Self {
        bandsyms: 0,
        coded_bands: 0,
        pairs: 0,
        quads: 0,
        nz: 0,
        zc: 0,
        raws: 0,
        escs: 0,
        ms: 0,
        long_codes: 0,
    };

    /// The reference ARM7TDMI cycle model, in cycles at WAITCNT 4/2.
    /// The IMDCT term is the measured bench-P asm number plus the fused 8-bit
    /// output stage; the rest are estimates. See `uli_dec_ref.h`.
    pub fn cycle_model(&self) -> u32 {
        const FRAME: u32 = 5_200;
        const CH: u32 = 300;
        const BANDSYM: u32 = 30;
        const CODEDBAND: u32 = 40;
        const ZEROFILL_X4: u32 = 5;
        const PAIR: u32 = 44;
        const QUAD: u32 = 52;
        const NZ: u32 = 22;
        const ZC: u32 = 3;
        const RAW: u32 = 8;
        const ESC: u32 = 45;
        const LONGCODE: u32 = 40;
        const MS: u32 = 3 * 512;
        const IMDCT_CH: u32 = 73_297 + 3 * 512;
        let coded = self.coded_bands.wrapping_mul(16);
        let zf = (2 * M as u32).wrapping_sub(coded);
        FRAME
            .wrapping_add(2 * CH)
            .wrapping_add(2 * IMDCT_CH)
            .wrapping_add(if self.ms != 0 { MS } else { 0 })
            .wrapping_add(BANDSYM.wrapping_mul(self.bandsyms))
            .wrapping_add(CODEDBAND.wrapping_mul(self.coded_bands))
            .wrapping_add(ZEROFILL_X4.wrapping_mul(zf / 4))
            .wrapping_add(PAIR.wrapping_mul(self.pairs))
            .wrapping_add(QUAD.wrapping_mul(self.quads))
            .wrapping_add(NZ.wrapping_mul(self.nz))
            .wrapping_add(ZC.wrapping_mul(self.zc))
            .wrapping_add(RAW.wrapping_mul(self.raws))
            .wrapping_add(ESC.wrapping_mul(self.escs))
            .wrapping_add(LONGCODE.wrapping_mul(self.long_codes))
    }
}

/// Result of decoding one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    /// Frame index that was decoded (0 is the preroll frame).
    pub frame: u32,
    /// Bits consumed by this frame.
    pub bits: u32,
    /// Stream bit position after the frame.
    pub bitpos: u32,
    /// Operation counts for this frame.
    pub ops: Ops,
    /// Reference cycle-model estimate for this frame.
    pub cycles: u32,
    /// Coefficients in this frame that violated the encoder's |X| bound.
    pub xover: u32,
    /// Nonzero coefficients at index >= NCODED (must stay zero for a valid stream).
    pub hfnz: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Diag {
    xover: u32,
    hfnz: u32,
}

/// Caller-owned decoder state. On the GBA this belongs in EWRAM (the code
/// tables dominate at roughly 100 KiB); the hot `x`, `buf` and `p` buffers
/// belong in IWRAM in the overlay's placement.
pub struct DecoderState {
    pub tables: CodeTables,
    x: [[i32; M]; 2],
    buf: [i32; 2 * H],
    p: [[i32; H]; 2],
    bitpos: u32,
    frame: u32,
    g_prev: i32,
    ops: Ops,
    xover: u32,
    hfnz: u32,
}

impl fmt::Debug for DecoderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecoderState")
            .field("tables", &self.tables)
            .field("bitpos", &self.bitpos)
            .field("frame", &self.frame)
            .field("g_prev", &self.g_prev)
            .field("ops", &self.ops)
            .field("xover", &self.xover)
            .field("hfnz", &self.hfnz)
            .finish_non_exhaustive()
    }
}

impl DecoderState {
    /// Construct a zeroed state. This is a `const fn`, so a GBA product can
    /// place it in a `static` without a runtime large-stack construction.
    pub const fn new() -> Self {
        Self {
            tables: CodeTables::new(),
            x: [[0; M]; 2],
            buf: [0; 2 * H],
            p: [[0; H]; 2],
            bitpos: 0,
            frame: 0,
            g_prev: 0,
            ops: Ops::ZERO,
            xover: 0,
            hfnz: 0,
        }
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

    /// Cumulative count of coefficients that violated the encoder's |X| bound.
    /// Must stay zero for a valid stream.
    pub fn xover(&self) -> u32 {
        self.xover
    }

    /// Cumulative count of nonzero coefficients at index >= NCODED. Must stay
    /// zero for a valid stream.
    pub fn hfnz(&self) -> u32 {
        self.hfnz
    }
    fn reset(&mut self) {
        self.x = [[0; M]; 2];
        self.buf = [0; 2 * H];
        self.p = [[0; H]; 2];
        self.bitpos = 0;
        self.frame = 0;
        self.g_prev = 0;
        self.ops = Ops::ZERO;
        self.xover = 0;
        self.hfnz = 0;
    }

    fn reset_overlap(&mut self) {
        self.p = [[0; H]; 2];
    }
}

/// The four synthesis kernels used by [`Decoder`].
///
/// This is the product seam: the portable core's default is
/// [`PortableKernels`], while the GBA product supplies its ARM-mode
/// realization (`uli_lite_gba::ArmKernels`) without duplicating
/// `decode_frame`. The trait is monomorphized, so a build does not pay a
/// per-call dispatch for it.
pub trait Kernels {
    fn pre(x: &[i32; M], z: &mut [i32; 2 * H]);
    fn fft(z: &mut [i32; 2 * H]);
    fn post(z: &[i32; 2 * H], y: &mut [i32; M]);
    fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], out16: Option<&mut [i16]>);
}

/// The portable, bit-exact reference kernels.
///
/// This is what [`Decoder::new`] uses, and it is the oracle every product
/// kernel set is twinned against. On ARM targets these four functions carry
/// `instruction_set(arm::a32)` (see `imdct.rs`), so even the reference uses
/// `SMULL` there instead of a software 64-bit multiply.
#[derive(Clone, Copy, Debug, Default)]
pub struct PortableKernels;

impl Kernels for PortableKernels {
    #[inline]
    fn pre(x: &[i32; M], z: &mut [i32; 2 * H]) {
        imdct::pre(x, z);
    }

    #[inline]
    fn fft(z: &mut [i32; 2 * H]) {
        imdct::fft(z);
    }

    #[inline]
    fn post(z: &[i32; 2 * H], y: &mut [i32; M]) {
        imdct::post(z, y);
    }

    #[inline]
    fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], out16: Option<&mut [i16]>) {
        imdct::ola(y, p, out8, out16);
    }
}

/// A decoder borrows a validated album and caller-owned state.
///
/// `K` is the synthesis kernel set. It defaults to [`PortableKernels`], so
/// `Decoder::new` is the portable decoder; the product opens
/// `Decoder::<ArmKernels>::with_kernels`.
pub struct Decoder<'a, 's, K: Kernels = PortableKernels> {
    album: Album<'a>,
    state: &'s mut DecoderState,
    _kernels: PhantomData<fn() -> K>,
}

impl<K: Kernels> fmt::Debug for Decoder<'_, '_, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("album", &self.album)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl<'a, 's> Decoder<'a, 's, PortableKernels> {
    /// Open the portable decoder over an album. This resets the state and
    /// parses the album's tables blob; it does not decode anything.
    pub fn new(album: Album<'a>, state: &'s mut DecoderState) -> Result<Self, Error> {
        Self::with_kernels(album, state)
    }
}

impl<'a, 's, K: Kernels> Decoder<'a, 's, K> {
    /// Open a decoder whose synthesis runs through `K`. Same state reset and
    /// table parse as [`Decoder::new`].
    pub fn with_kernels(album: Album<'a>, state: &'s mut DecoderState) -> Result<Self, Error> {
        state.reset();
        state.tables.parse_into(album.tables())?;
        Ok(Self {
            album,
            state,
            _kernels: PhantomData,
        })
    }

    pub fn album(&self) -> &Album<'a> {
        &self.album
    }

    pub fn state(&self) -> &DecoderState {
        self.state
    }

    /// Decode the next frame into 512-sample L/R buffers.
    ///
    /// `out8l`/`out8r` are the DAC samples the GBA FIFOs receive. The optional
    /// `out16*` buffers receive the pre-DAC PCM16 path used by the speaker
    /// profile. Either int16 buffer may be omitted independently.
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
        if self.state.frame >= self.album.n_frames() {
            return Err(Error::EndOfStream);
        }

        let start = self.state.bitpos;
        let mut br = BitReader::new(self.album.stream(), start);
        let mut ops = Ops::ZERO;
        let mut diag = Diag::default();

        let z = br.eg(0)?;
        let g = self.state.g_prev.wrapping_add(if z & 1 != 0 {
            -(((z.wrapping_add(1)) >> 1) as i32)
        } else {
            (z >> 1) as i32
        });
        self.state.g_prev = g;
        let ms = br.get(1)? as i32;
        let geff = g.wrapping_sub(4 * ms);
        if !(0..NSTEP as i32).contains(&geff) {
            return Err(Error::BadFrame);
        }
        let step = STEP[geff as usize];
        ops.ms = ms as u32;

        {
            let DecoderState { tables, x, .. } = &mut *self.state;
            dec_chan(tables, &mut x[0], step, &mut br, &mut ops, &mut diag)?;
            dec_chan(tables, &mut x[1], step, &mut br, &mut ops, &mut diag)?;
        }

        if ms != 0 {
            for i in 0..M {
                let a = self.state.x[0][i] as u32;
                let b = self.state.x[1][i] as u32;
                self.state.x[0][i] = a.wrapping_add(b) as i32;
                self.state.x[1][i] = a.wrapping_sub(b) as i32;
            }
        }

        K::pre(&self.state.x[0], &mut self.state.buf);
        K::fft(&mut self.state.buf);
        K::post(&self.state.buf, &mut self.state.x[0]);
        K::ola(&self.state.x[0], &mut self.state.p[0], out8l, out16l);

        K::pre(&self.state.x[1], &mut self.state.buf);
        K::fft(&mut self.state.buf);
        K::post(&self.state.buf, &mut self.state.x[1]);
        K::ola(&self.state.x[1], &mut self.state.p[1], out8r, out16r);

        self.state.bitpos = br.bitpos();
        self.state.ops = ops;
        self.state.xover = self.state.xover.wrapping_add(diag.xover);
        self.state.hfnz = self.state.hfnz.wrapping_add(diag.hfnz);
        let frame = self.state.frame;
        self.state.frame = self.state.frame.wrapping_add(1);
        Ok(FrameInfo {
            frame,
            bits: self.state.bitpos.wrapping_sub(start),
            bitpos: self.state.bitpos,
            ops,
            cycles: ops.cycle_model(),
            xover: diag.xover,
            hfnz: diag.hfnz,
        })
    }

    /// Seek to a track's preroll frame and decode it, discarding its output.
    /// Returns the number of samples to drop from the next decoded frame.
    pub fn seek_track(&mut self, track: usize) -> Result<usize, Error> {
        let tr = self.album.track(track).ok_or(Error::BadSeek)?;
        self.state.bitpos = tr.bitpos;
        self.state.frame = tr.preroll_frame;
        self.state.g_prev = tr.g_prev as i32;
        self.state.reset_overlap();

        // Preroll output is discarded. The 1 KiB stack cost is paid only on a
        // track change; the product overlay can route this through its own
        // scratch buffers if the stack budget demands it.
        let mut out8l = [0i8; M];
        let mut out8r = [0i8; M];
        self.decode_frame(&mut out8l, &mut out8r, None, None)?;
        Ok(tr.skip as usize)
    }

    /// Seek to a seek-table entry without a preroll. The next decoded frame is
    /// valid only after one frame has been decoded (the overlap state is reset).
    pub fn seek_entry(&mut self, entry: usize) -> Result<(), Error> {
        let sk = self.album.seek(entry).ok_or(Error::BadSeek)?;
        let frame = u32::try_from(entry)
            .ok()
            .and_then(|e| e.checked_mul(self.album.seek_interval()))
            .ok_or(Error::BadSeek)?;
        self.state.bitpos = sk.bitpos;
        self.state.frame = frame;
        self.state.g_prev = sk.g_prev as i32;
        self.state.reset_overlap();
        Ok(())
    }
}

fn dec_chan(
    tables: &CodeTables,
    x: &mut [i32; M],
    step: u32,
    br: &mut BitReader<'_>,
    ops: &mut Ops,
    diag: &mut Diag,
) -> Result<(), Error> {
    x.fill(0);
    let k = tables.k;
    let mut ctx = k;
    for b in 0..NBANDS {
        let c = tables.huff_cb(ctx, br, ops)? as usize;
        ops.bandsyms = ops.bandsyms.wrapping_add(1);
        if c == k {
            return Ok(());
        }
        if c >= MAXK {
            return Err(Error::BadFrame);
        }
        ctx = c;
        let ctype = tables.ctype[c];
        if ctype == CT_ZERO {
            continue;
        }
        ops.coded_bands = ops.coded_bands.wrapping_add(1);
        let dq = &tables.dq[c];
        let esck = tables.esck[c];
        let mut m = [0i32; BANDW];

        if ctype == CT_PAIR {
            let r = tables.rbits[c];
            if r > 31 {
                return Err(Error::BadTables);
            }
            let mut i = 0;
            while i < BANDW {
                let s = tables.huff_cls(c, br, ops)? as u32;
                ops.pairs = ops.pairs.wrapping_add(1);
                let nib = [s >> 4, s & 15];
                for (kk, &nibble) in nib.iter().enumerate() {
                    let mut hh = nibble;
                    if hh == 15 {
                        hh = hh.wrapping_add(br.eg(esck)?);
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    let mm = hh.wrapping_shl(r as u32) | br.get(r)?;
                    if r != 0 {
                        ops.raws = ops.raws.wrapping_add(1);
                    }
                    let val = mm as i32;
                    m[i + kk] = if mm != 0 && br.get(1)? != 0 {
                        val.wrapping_neg()
                    } else {
                        val
                    };
                }
                i += 2;
            }
        } else {
            let mut i = 0;
            while i < BANDW {
                let s = tables.huff_cls(c, br, ops)? as u32;
                ops.quads = ops.quads.wrapping_add(1);
                let dg = [s / 27, (s / 9) % 3, (s / 3) % 3, s % 3];
                for (kk, &digit) in dg.iter().enumerate() {
                    let mut mm = digit;
                    if mm == 2 {
                        mm = mm.wrapping_add(br.eg(esck)?);
                        ops.escs = ops.escs.wrapping_add(1);
                    }
                    let val = mm as i32;
                    m[i + kk] = if mm != 0 && br.get(1)? != 0 {
                        val.wrapping_neg()
                    } else {
                        val
                    };
                }
                i += 4;
            }
        }

        for (i, &v) in m.iter().enumerate() {
            if v == 0 {
                ops.zc = ops.zc.wrapping_add(1);
                continue;
            }
            let a = v.unsigned_abs() as usize;
            let idx = a.min(15);
            let rq = 16i64 * a as i64 - dq[idx] as i64;
            // Stays a 64-bit product: this function is Thumb-1, where 64-bit
            // multiplies go through `__aeabi_lmul`. That is 1 call per
            // nonzero coefficient, not the 2-4 per coefficient the twiddle
            // kernels were paying before their ARM-mode build; measure the
            // entropy share before moving this path too (ULI2.md, Phase 1b).
            let xv = rq * step as i64;
            if xv >= XLIM {
                diag.xover = diag.xover.wrapping_add(1);
            }
            let q = xv as i32;
            x[b * BANDW + i] = if v < 0 { q.wrapping_neg() } else { q };
            ops.nz = ops.nz.wrapping_add(1);
            if b * BANDW + i >= NCODED {
                diag.hfnz = diag.hfnz.wrapping_add(1);
            }
        }
    }
    Ok(())
}
