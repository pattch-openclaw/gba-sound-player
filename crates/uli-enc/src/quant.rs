//! Fixed-step quantisation: MDCT coefficients in, band symbols out.
//!
//! S2's whole job, and deliberately not S3's. Given a frame's coefficients and one
//! global step `g`, every coefficient is quantised to the magnitude `m` whose
//! reconstruction the decoder will produce closest to it, and each band is given a
//! class. There is no Lagrangian search, no `kappa`, no cycle term and no M/S
//! choice: `lam = 0`, and the class is chosen by a rule that never needs an escape
//! rather than by the rate it costs.
//!
//! **The step fixes the distortion and the class fixes the rate; they are
//! independent here, and that is not an accident of this file.** Reconstruction is
//! `sign * (16m - dq[c][min(m,15)]) * step`, and the default tables carry `dq = 3`
//! for every class at every `m >= 1`, so two classes that both encode the same `m`
//! decode to bit-identical coefficients. What differs is how many bits `m` costs.
//! S3's band-class Viterbi is what turns that freedom into a rate decision; S2 only
//! has to be exact, cheap and deterministic.
//!
//! The candidate set for `t = |x| / (step/16)` is `{0, floor t, floor t + 1}`, clipped to
//! `mmax`, with the first minimum winning.
//!
//! It is not round-to-nearest, and it does not need to be: the set is **complete**. A level
//! reconstructs to `(16m − dq)·step`, which in magnitude units is `m − dq/16`, so for a
//! given `t` the best level is `t + dq/16`. With `dq` in `0..16` that lies in
//! `(floor t, floor t + 1]` — never below `floor t`. Offering `floor t − 1` would be
//! offering a strictly worse level, so the three candidates are exactly the ones that can
//! contain the optimum. (Verified exhaustively over `dq` and `t` when this comment was
//! written; an earlier version of it claimed the opposite and had the sign of the shift
//! backwards.)

use uli_lite::format::{BANDW, CT_PAIR, CT_QUAD, CT_ZERO, MAXK, NBANDS, NCODED};
use uli_lite::tables_data::STEP;

use crate::tables_default::DefaultTables;

/// One coded band: its class, and the signed magnitude index of each of its 16
/// coefficients. A negative `m` is the format's sign bit (the reader takes the
/// magnitude and then flips it), and `class == 0` is a ZERO band: it costs one band
/// symbol and codes no coefficients at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    pub class: usize,
    pub m: [i32; BANDW],
}

impl Band {
    /// A band whose every coefficient is zero, coded as class 0.
    pub fn zero() -> Self {
        Self {
            class: 0,
            m: [0; BANDW],
        }
    }

    pub fn is_zero_class(&self) -> bool {
        self.class == 0
    }
}

/// One channel's coded bands, in the order the reader consumes them. The end of
/// the list *is* the EOB: the writer emits the class table's terminator after the
/// last band, and every band past it is zero by the reader's own construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chan {
    pub bands: Vec<Band>,
}

impl Chan {
    /// A channel with nothing coded — the initial context's EOB, nothing else.
    pub fn empty() -> Self {
        Self { bands: Vec::new() }
    }
}

/// The coefficients a band may hold nonzero values in.
///
/// The format forbids energy at or beyond `NCODED`, so bands are coded whole and the
/// last one is partial: a band's count is 16, a remainder, or 0.
pub fn band_ncod(band: usize) -> usize {
    let start = band * BANDW;
    if start + BANDW <= NCODED {
        BANDW
    } else if start > NCODED - 1 {
        0
    } else {
        NCODED - start
    }
}

/// The decoder's step for a frame's effective index (`geff = g - 4*ms`).
pub fn step_of(g: i32, ms: bool) -> u32 {
    STEP[(g - i32::from(ms) * 4) as usize]
}

/// Everything a quantiser needs to know about a `(g, ms)` pair, in one place.
///
/// An M/S frame costs 4 step indices out of `g` **and** changes the size of a magnitude
/// step, and those are two different things. The reader reconstructs with
/// `step_bar = STEP[g - 4*ms]`, while the encoder compares against coefficients the
/// forward rotation left `√2` larger than the reader's per-channel value — hence
/// `d = step_bar*√2/16` for M/S against `step_bar/16` for L/R. `mmax` comes from
/// `step_bar`, not from `d`, because it is the reader's `|X| < 2^28` that bounds it.
#[derive(Clone, Copy, Debug)]
pub struct Unit {
    /// The reader's step for this frame.
    pub step_bar: f64,
    /// The encoder's reconstruction unit: `step_bar/16`, or `step_bar*√2/16` for M/S.
    pub d: f64,
    /// The largest magnitude the reader can carry at this step.
    pub mmax: i32,
}

impl Unit {
    pub fn of(g: i32, ms: bool) -> Self {
        let step_bar = step_of(g, ms) as f64;
        Self {
            step_bar,
            d: if ms {
                step_bar * std::f64::consts::SQRT_2 / 16.0
            } else {
                step_bar / 16.0
            },
            mmax: mmax_of(step_bar as u32),
        }
    }
}

/// The forward M/S rotation: `M = (L+R)/√2`, `S = (L-R)/√2`. Orthonormal, so a frame's
/// energy — and therefore its squared error — is unchanged by rotating it.
///
/// The **rounding** is not free choice: the two `f32` coefficients are added in `f32`,
/// that sum is promoted to `f64` for the `√2` multiply, and the product is rounded back to
/// `f32`. Doing the add in `f64` rounds differently on some inputs, and since every
/// downstream decision (magnitude, class, sign) is taken on these values, a single
/// differing bit changes the blob.
pub fn rotate(coef: &[f32], ms: bool) -> Vec<f32> {
    let m = coef.len() / 2;
    if !ms {
        return coef.to_vec();
    }
    let r = std::f64::consts::FRAC_1_SQRT_2;
    let mut out = vec![0f32; coef.len()];
    for i in 0..m {
        let sum = coef[i] + coef[m + i];
        let diff = coef[i] - coef[m + i];
        out[i] = (f64::from(sum) * r) as f32;
        out[m + i] = (f64::from(diff) * r) as f32;
    }
    out
}

/// The largest magnitude the format can carry without breaking the reader's
/// `|X| < 2^28` guarantee: `(16m − dq)·step < 2^28`, so `m ≤ (2^28/step)/16 − 1`.
///
/// Clamped to `1..=65535`. The lower clamp is not decorative — at a coarse enough step the
/// quotient falls below 1, and a band with no representable magnitude could not be coded at
/// all. The upper clamp keeps escapes within a length that is worth writing.
pub fn mmax_of(step: u32) -> i32 {
    let m = (1i64 << 28) / step as i64 / 16 - 1;
    m.clamp(1, 65535) as i32
}

/// The magnitude one coefficient is coded as, under the candidate set above. `ax` is
/// `|coef|` in the analysis domain, `d` is the reconstruction unit, and `dq` is the row of
/// the class the band will be coded with.
fn quantise_magnitude(ax: f64, d: f64, inv: f64, mmax: i32, dq: &[i8; 16]) -> i32 {
    if mmax <= 0 || ax == 0.0 {
        return 0;
    }
    let t = ax * inv;
    let mut f = t.floor() as i64;
    if f > mmax as i64 {
        f = mmax as i64;
    }
    let f = f as i32;
    // Zero is offered first and the comparison is strict, so a tie keeps it: coding
    // nothing is the cheaper symbol, and a tie is not a reason to spend bits.
    let mut best_m = f;
    let mut best_e = f64::INFINITY;
    let mut consider = |m: i32| {
        if m < 0 || m > mmax {
            return;
        }
        // The reconstruction in analysis units: (m - dq/16) * d, which is the
        // reader's `(16m - dq) * step` divided by 256.
        let rec = (m as f64 - dq[m.min(15) as usize] as f64 / 16.0) * d;
        let e = (ax - rec) * (ax - rec);
        if e < best_e {
            best_e = e;
            best_m = m;
        }
    };
    if f > 0 {
        consider(0);
    }
    consider(f);
    if f < mmax {
        consider(f + 1);
    }
    best_m
}

/// The `dq` row used to size bands before a class has been chosen.
///
/// Sizing a band needs a `dq`, and the class that supplies it is chosen *from* the sizes,
/// so the dependency is circular by construction. Every class in the default set carries
/// the same row, so any choice is the same choice; and `quantise_chan_rd` re-quantises with
/// the class it actually picked, which is what keeps this honest once `dq` is trained per
/// class.
const PROVISIONAL_CLASS: usize = 1;

/// The largest magnitude a class can carry in its symbol alone, with no escape.
/// PAIR keeps `m >> rbits` in `0..=14` with the low `rbits` bits raw, so the
/// ceiling is `(15 << rbits) - 1`; nibble 15 *is* the escape. QUAD keeps a base-3
/// digit, and digit 2 is its escape, so only `0` and `1` ride free.
fn class_headroom(tables: &DefaultTables, class: usize) -> Option<i32> {
    match tables.ctype[class] {
        CT_QUAD => Some(1),
        CT_PAIR => Some((15i32 << tables.rbits[class]) - 1),
        CT_ZERO => Some(0),
        _ => None,
    }
}

/// The class for a band whose largest quantised magnitude is `m_max`: the lowest
/// class index with enough headroom, falling back to the widest class available
/// (whose escapes then carry anything beyond).
///
/// Lowest index rather than best rate, because with the default `dq` rows all
/// equal, rate is the only thing being traded and the table is ordered so that
/// rising index means rising dynamic range. S3 replaces this with the Viterbi.
pub fn pick_class(tables: &DefaultTables, m_max: i32) -> usize {
    if m_max <= 0 {
        return 0;
    }
    for c in 1..tables.k {
        if class_headroom(tables, c).is_some_and(|h| m_max <= h) {
            return c;
        }
    }
    tables.k - 1
}

/// Quantise one channel's `M` coefficients at a fixed step.
pub fn quantise_chan(coef: &[f32], tables: &DefaultTables, g: i32, ms: bool) -> (Chan, ChanStats) {
    let step = step_of(g, ms) as f64;
    let d = step / 16.0;
    let inv = 1.0 / d;
    let mmax = mmax_of(step as u32);

    let mut chan = Chan::default();
    let mut stats = ChanStats::default();

    // Pass one: magnitudes under the provisional `dq` row, enough to size each
    // band's dynamic range and therefore pick its class.
    let dq_prov = &tables.dq[PROVISIONAL_CLASS];
    let mut m = [0i32; NCODED];
    for (k, out) in m.iter_mut().enumerate() {
        let ax = f64::from(coef[k]).abs();
        *out = quantise_magnitude(ax, d, inv, mmax, dq_prov);
    }
    // Where does the spectrum actually end? Everything past the last nonzero
    // magnitude is EOB, which is the reader's own definition of "band absent".
    let last = m.iter().rposition(|&v| v != 0).map(|i| i / BANDW);
    let Some(last_band) = last else {
        return (Chan::empty(), stats);
    };

    for b in 0..=last_band {
        let ncod = band_ncod(b);
        let mut band = Band {
            class: 0,
            m: [0; BANDW],
        };
        let mut m_max = 0i32;
        for i in 0..ncod {
            let v = m[b * BANDW + i];
            if v != 0 {
                m_max = m_max.max(v);
            }
        }
        if m_max == 0 {
            chan.bands.push(Band::zero());
            continue;
        }
        band.class = pick_class(tables, m_max);
        // Pass two: the magnitudes under the class actually chosen. With the
        // default tables this is a no-op, and with a trained `dq` it is the
        // difference between coding what the reader will reconstruct and coding
        // something else. A magnitude that overflows the class after this rides
        // an escape, which is legal rather than an error.
        let dq = &tables.dq[band.class];
        for i in 0..ncod {
            let c = b * BANDW + i;
            let ax = f64::from(coef[c]).abs();
            let v = quantise_magnitude(ax, d, inv, mmax, dq);
            if v != 0 {
                m[c] = v;
                // The sign rides on the magnitude; the reader takes `|m|` and then
                // a sign bit, so zero must stay unsigned.
                band.m[i] = if coef[c] < 0.0 { -v } else { v };
            }
        }
        if m_max == 0 {
            chan.bands.push(Band::zero());
            continue;
        }
        band.class = pick_class(tables, m_max);
        for &v in &band.m {
            let a = v.abs();
            if a != 0 {
                stats.nz += 1;
                if tables.ctype[band.class] == CT_PAIR {
                    if (a >> tables.rbits[band.class]) >= 15 {
                        stats.escs += 1;
                    }
                    if tables.rbits[band.class] != 0 {
                        stats.raws += 1;
                    }
                } else if a >= 2 {
                    stats.escs += 1;
                }
            } else {
                stats.zc += 1;
            }
        }
        stats.bands += 1;
        stats.tuples += if tables.ctype[band.class] == CT_PAIR {
            8
        } else {
            4
        };
        chan.bands.push(band);
    }
    (chan, stats)
}

/// What a channel's coding cost the reader, in the units its cycle model counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChanStats {
    /// Nonzero coefficients.
    pub nz: u32,
    /// Zero coefficients inside a coded band (still decoded, still costed).
    pub zc: u32,
    /// Coded bands of a nonzero class.
    pub bands: u32,
    /// Tuple symbols: pairs + quads.
    pub tuples: u32,
    /// Escape extensions.
    pub escs: u32,
    /// Raw `rbits` fields.
    pub raws: u32,
}

// ---------------------------------------------------------------------------
// S3: rate/distortion quantisation
// ---------------------------------------------------------------------------

/// The most candidates one coefficient can offer.
const MAX_CANDS: usize = 3;

/// The candidate magnitudes for one coefficient: `{0, floor t, floor t + 1}` clipped to
/// `mmax`, and **only** `0` when the coefficient is exactly zero or the band's headroom is
/// closed (`mmax <= 0`, i.e. at or past `NCODED`, where the format forbids energy).
///
/// Zero comes first and the consumer's comparison is strict, so a tie keeps it: on equal
/// cost the cheaper symbol wins, and a tie is not a reason to spend bits.
#[inline]
fn make_cands(ax: f64, d: f64, mmax: i32, out: &mut [i32; MAX_CANDS]) -> usize {
    if mmax <= 0 || ax == 0.0 {
        out[0] = 0;
        return 1;
    }
    let mut f = (ax / d).floor() as i64;
    if f > mmax as i64 {
        f = mmax as i64;
    }
    let f = f as i32;
    let mut n = 0;
    if f > 0 {
        out[n] = 0;
        n += 1;
    }
    out[n] = f;
    n += 1;
    if f < mmax {
        out[n] = f + 1;
        n += 1;
    }
    n
}

/// `nib_of`: the part of a magnitude that rides inside the tuple symbol.
#[inline]
fn nib_of(tables: &DefaultTables, class: usize, m: i32) -> u32 {
    if tables.ctype[class] == CT_PAIR {
        ((m as u32) >> tables.rbits[class]).min(15)
    } else {
        (m as u32).min(2)
    }
}

/// `extra_bits`: everything a coefficient costs *besides* its share of the tuple
/// symbol — the raw field, the sign, and any escape.
///
/// Two details the writer forces and the naive cost model gets wrong: the raw field
/// is written for **every** coefficient of a PAIR class with `rbits > 0`, including
/// zero magnitudes (the reader always takes the bits), and the sign bit exists only
/// for a nonzero magnitude.
#[inline]
fn extra_bits(tables: &DefaultTables, class: usize, m: i32) -> u32 {
    if tables.ctype[class] == CT_PAIR {
        let r = tables.rbits[class] as u32;
        let mut b = r;
        if m != 0 {
            b += 1;
        }
        let h = (m as u32) >> r;
        if h >= 15 {
            b += crate::bitwriter::eg_bits(h - 15, tables.esck[class] as u32);
        }
        b
    } else {
        let mut b = 0;
        if m != 0 {
            b += 1;
        }
        if m > 1 {
            b += crate::bitwriter::eg_bits((m - 2) as u32, tables.esck[class] as u32);
        }
        b
    }
}

/// The reconstruction of a magnitude, in analysis units: `(m - dq/16) · step/16`,
/// which is the reader's `(16m - dq) · step` divided by the 2^8 the format's `X` is
/// scaled by. One definition, used by both the distortion and the search.
#[inline]
fn recon(dq: &[i8; 16], m: i32, d: f64) -> f64 {
    (m as f64 - dq[m.min(15) as usize] as f64 / 16.0) * d
}

/// One band coded by a given class: the magnitudes chosen under
/// `J = Σ error² + λ·bits`, and what that cost.
#[derive(Clone, Copy, Debug)]
pub struct BandRd {
    pub m: [i32; BANDW],
    pub dist: f64,
    pub bits: u64,
}

impl BandRd {
    pub fn j(&self, lam: f64) -> f64 {
        self.dist + lam * self.bits as f64
    }
}

/// Rate/distortion quantisation of one 16-coefficient band under one class.
///
/// The candidate set includes **zero**, and that is the whole point: a coefficient whose
/// magnitude is small relative to its bit cost should be dropped rather than coded. With
/// `lam = 0` the zero is never chosen, because coding always weakly reduces distortion —
/// which is why a distortion-only quantiser spends far more bits than the rate it is given
/// allows. With `lam = kappa·D²` the zero wins whenever `x²` is cheaper to lose than the
/// bits are to spend, and at a coarse step that is most of the spectrum.
pub fn band_rdo(
    x: &[f32],
    tables: &DefaultTables,
    class: usize,
    d: f64,
    lam: f64,
    mmax: i32,
    ncod: usize,
) -> Option<BandRd> {
    let pair = tables.ctype[class] == CT_PAIR;
    debug_assert!(
        pair || tables.rbits[class] == 0,
        "the QUAD arm writes no raw field, so a QUAD class with rbits > 0 would be \
         costed by a rule the writer does not follow"
    );
    let per = if pair { 2 } else { 4 };
    let len = &tables.cls[class].len;
    let dq = &tables.dq[class];
    let mut out = BandRd {
        m: [0; BANDW],
        dist: 0.0,
        bits: 0,
    };
    // A symbol with no code cannot be written, so it must not be priced. A zero-length
    // symbol costs nothing to "send" in this arithmetic, so leaving it in the candidate set
    // makes it look free and it wins. Inert while every symbol in the tables has a code —
    // which `huff_lengths` guarantees for the defaults — but a *trained* table leaves
    // never-seen symbols at length 0, and choosing one writes bits the reader cannot parse.

    let mut i = 0;
    while i < BANDW {
        let mut cs = [[0i32; MAX_CANDS]; 4];
        let mut cn = [0usize; 4];
        let mut ax = [0f64; 4];
        for kk in 0..per {
            let k = i + kk;
            ax[kk] = f64::from(x[k].abs());
            let mx = if k < ncod { mmax } else { 0 };
            cn[kk] = make_cands(ax[kk], d, mx, &mut cs[kk]);
        }
        let mut best_j = f64::INFINITY;
        let mut best = [0i32; 4];
        let mut best_bits = 0u64;
        let mut best_dist = 0.0f64;

        if pair {
            for p in 0..cn[0] {
                for q in 0..cn[1] {
                    let (m0, m1) = (cs[0][p], cs[1][q]);
                    let sym = (nib_of(tables, class, m0) * 16 + nib_of(tables, class, m1)) as usize;
                    let l = len[sym] as u64;
                    if l == 0 {
                        continue;
                    }
                    let bits = l
                        + extra_bits(tables, class, m0) as u64
                        + extra_bits(tables, class, m1) as u64;
                    let e0 = ax[0] - recon(dq, m0, d);
                    let e1 = ax[1] - recon(dq, m1, d);
                    let dist = e0 * e0 + e1 * e1;
                    let j = dist + lam * bits as f64;
                    if j < best_j {
                        best_j = j;
                        best = [m0, m1, 0, 0];
                        best_bits = bits;
                        best_dist = dist;
                    }
                }
            }
        } else {
            for p0 in 0..cn[0] {
                for p1 in 0..cn[1] {
                    for p2 in 0..cn[2] {
                        for p3 in 0..cn[3] {
                            let m = [cs[0][p0], cs[1][p1], cs[2][p2], cs[3][p3]];
                            let sym = (27 * nib_of(tables, class, m[0])
                                + 9 * nib_of(tables, class, m[1])
                                + 3 * nib_of(tables, class, m[2])
                                + nib_of(tables, class, m[3]))
                                as usize;
                            let l = len[sym] as u64;
                            if l == 0 {
                                continue;
                            }
                            let mut bits = l;
                            let mut dist = 0.0f64;
                            for kk in 0..4 {
                                bits += extra_bits(tables, class, m[kk]) as u64;
                                let e = ax[kk] - recon(dq, m[kk], d);
                                dist += e * e;
                            }
                            let j = dist + lam * bits as f64;
                            if j < best_j {
                                best_j = j;
                                best = m;
                                best_bits = bits;
                                best_dist = dist;
                            }
                        }
                    }
                }
            }
        }
        if best_j.is_infinite() {
            // No combination of this tuple is codeable under this class, so the class
            // is dropped rather than coded with a symbol that does not exist.
            return None;
        }
        out.m[i..i + per].copy_from_slice(&best[..per]);
        out.bits += best_bits;
        out.dist += best_dist;
        i += per;
    }
    Some(out)
}

/// The class groups of the default rows: `(ctype, rbits)`. Classes in one group cost
/// a coefficient the same way and differ only in the distribution their Huffman
/// table was built from, so within a group the choice is a model fit, not a
/// structural one.
/// Which classes a band is offered: **every nonzero one**.
///
/// The obvious optimisation is to prune — a class whose headroom cannot hold the band's
/// largest magnitude has to escape, and escapes cost bits, so it "must" lose. That
/// reasoning is wrong, and measurably so: a first version of this function kept only the
/// two tightest fitting headroom groups and chose the group member by matching the model's
/// `mu` to the band's mean magnitude, and it cost 1.07×/1.16×/1.20× the bits at
/// `g = 60/100/140` against offering everything. A wider head group shortens the *symbol
/// code* by more than the raw field widens it, because the Huffman table of a high-`mu`
/// class prices small heads cheaply. The class ladder has to be measured, not reasoned
/// about — which is also why the cost of trying all of them is worth paying.
pub fn rd_classes(tables: &DefaultTables) -> Vec<usize> {
    (1..tables.k)
        .filter(|&c| tables.ctype[c] != CT_ZERO)
        .collect()
}

/// What a whole channel costs under the RD quantiser.
#[derive(Clone, Debug)]
pub struct ChanRd {
    pub chan: Chan,
    /// Squared error over the coded range, in analysis units.
    pub dist: f64,
    /// Bits the coefficients and band symbols occupy, EOB included.
    pub bits: u64,
}

/// The decoder's ARM7TDMI cycle constants, in cycles, mirrored from
/// `uli_lite::Ops::cycle_model` (and `uli_dec_ref.h`). The cycle-aware search prices a
/// band's decode work with these, and the witness in `tests/quant.rs` reconstructs the
/// reader's whole model from them — a constant that drifts on either side is a failing
/// test rather than a `--cap` that quietly stopped binding.
pub const CY_FRAME: u32 = 5_200;
pub const CY_CH: u32 = 300;
pub const CY_BANDSYM: u32 = 30;
pub const CY_CODEDBAND: u32 = 40;
pub const CY_ZEROFILL_X4: u32 = 5;
pub const CY_PAIR: u32 = 44;
pub const CY_QUAD: u32 = 52;
pub const CY_NZ: u32 = 22;
pub const CY_ZC: u32 = 3;
pub const CY_RAW: u32 = 8;
pub const CY_ESC: u32 = 45;
pub const CY_LONGCODE: u32 = 40;
pub const CY_MS: u32 = 3 * 512;
pub const CY_IMDCT_CH: u32 = 73_297 + 3 * 512;

/// A coded band's decoder cycles beyond its band symbol, relative to a zero band — the
/// reference's `band_cyc`. The band symbol itself is charged by the shortest path, because
/// every band (and the terminator) sends one; this is only what changes when the band is
/// *coded*. A zero class costs nothing here.
pub fn band_cycles(tables: &DefaultTables, band: &Band) -> u32 {
    cycles_of(tables, band.class, &band.m)
}

fn cycles_of(tables: &DefaultTables, class: usize, m: &[i32; BANDW]) -> u32 {
    if class == 0 {
        return 0;
    }
    let pair = tables.ctype[class] == CT_PAIR;
    let r = tables.rbits[class];
    let mut nz = 0u32;
    let mut escs = 0u32;
    for &v in m {
        let a = v.unsigned_abs();
        if a != 0 {
            nz += 1;
        }
        if pair {
            if (a >> r) >= 15 {
                escs += 1;
            }
        } else if a >= 2 {
            escs += 1;
        }
    }
    let zc = BANDW as u32 - nz;
    let raws = if pair && r != 0 { BANDW as u32 } else { 0 };
    let (nsym, per_sym) = if pair {
        (8u32, CY_PAIR)
    } else {
        (4u32, CY_QUAD)
    };
    CY_CODEDBAND + nsym * per_sym + nz * CY_NZ + zc * CY_ZC + raws * CY_RAW + escs * CY_ESC
        - 4 * CY_ZEROFILL_X4
}

/// Rate/distortion quantisation of one channel at a fixed step (S3).
///
/// Three decisions, all taken with `lam = kappa·D²`:
///
/// - **per band**, whether it is worth coding at all — class 0 costs one band symbol
///   and leaves the band's whole energy as distortion; a coded class costs its
///   coefficients and less distortion. S2 could not decline to code a band, because
///   with `lam = 0` coding is always weakly better.
/// - **per coefficient**, magnitude zero or not — which is where almost all of the rate
///   saving is: a distortion-only quantiser is ~1.4× to ~2.6× more expensive than this one
///   at the same step, and the ratio grows as the step coarsens and more of the spectrum
///   becomes cheaper to discard than to code.
/// - **where the spectrum ends.** The reader stops at the first terminator, so
///   everything from `B` on reconstructs as zero; `B` is chosen by scanning prefix
///   cost against the energy left uncoded. Exact rather than heuristic, because every
///   context table in the default set is identical (measured: all 15 rows equal), so
///   a band's class symbol costs the same from any context and the class sequence
///   carries no coupling. **That equality is what makes the scan exact. A table set whose
///   context codebooks differ would couple consecutive bands, and the choice would become a
///   shortest-path problem over (band, class) instead of a prefix scan.**
///
/// **`mu` is the decoder-cycle price (S6).** With `mu = 0` this is exactly the rate/distortion
/// search above, bit for bit. With `mu > 0` every band transition also costs
/// `mu · (CY_BANDSYM + band_cycles)`, so a band whose decoder work is expensive relative to
/// its distortion is more readily dropped — which is how an over-cap frame is re-coded below
/// the cap instead of refusing the candidate.
pub fn quantise_chan_rd(
    coef: &[f32],
    tables: &DefaultTables,
    g: i32,
    ms: bool,
    kappa: f64,
    mu: f64,
) -> ChanRd {
    // The coefficients handed in must already be in the domain the reader will see: the
    // caller applies [`rotate`] when `ms` is set. The `√2` step compensation belongs
    // here, next to the unit it scales; the rotation is per frame rather than per
    // channel, so applying it in both places would silently rotate twice.
    let unit = Unit::of(g, ms);
    let d = unit.d;
    let lam = kappa * d * d;
    let mmax = unit.mmax;
    let k = tables.k;
    let eob_len = tables.cb[0].symbol_len(k) as u64;
    let zero_len = tables.cb[0].symbol_len(0) as u64;
    // A table with no terminator, or no way to say "this band is zero", cannot describe
    // any channel at all. That is a table-integrity precondition rather than data, so it
    // is an assert and not a refusal — and it is reached before the per-class skip below
    // can hide it by choosing a class.
    assert!(
        eob_len > 0 && zero_len > 0,
        "the class tables have no code for EOB ({eob_len} bits) or for class 0 \
         ({zero_len} bits): no channel is representable"
    );

    // Every band's energy, uncoded. The tail's energy is distortion, not a saving:
    // a search that ignored it would think "code nothing" is optimal, which is how
    // this kind of search fails quietly.
    let mut energy = [0f64; NBANDS];
    for b in 0..NBANDS {
        let mut e = 0.0;
        for i in 0..band_ncod(b) {
            let v = f64::from(coef[b * BANDW + i]);
            e += v * v;
        }
        energy[b] = e;
    }

    // Per-band, per-class cost of coding that band **ignoring the band symbol**, which is
    // charged in the shortest-path below because it depends on the *previous* class.
    //
    // `band_dist[b][c]` and `band_bits[b][c]` are `f64::INFINITY` / `None` where the class
    // cannot code the band (a symbol with no code), which the search then skips.
    let mut band_dist = [[f64::INFINITY; MAXK]; NBANDS];
    let mut band_bits = [[0u64; MAXK]; NBANDS];
    // The decoder work a class adds to this band, for the `mu` term. Zero for class 0,
    // which is what makes a zero band cheap in the cycle-aware search as well as the rate
    // one.
    let mut band_cyc = [[0u32; MAXK]; NBANDS];
    for b in 0..NBANDS {
        let ncod = band_ncod(b);
        // Code nothing: one band symbol, all of this band's energy left as error.
        band_dist[b][0] = energy[b];
        band_bits[b][0] = 0;
        for klass in rd_classes(tables) {
            // A class whose band symbol has no code cannot be written, so it must not be
            // priced: a zero-length symbol costs nothing here and therefore wins.
            if tables.cb[0].symbol_len(klass) == 0 {
                continue;
            }
            let Some(rd) = band_rdo(
                &coef[b * BANDW..b * BANDW + BANDW],
                tables,
                klass,
                d,
                lam,
                mmax,
                ncod,
            ) else {
                continue;
            };
            band_dist[b][klass] = rd.dist;
            band_bits[b][klass] = rd.bits;
            band_cyc[b][klass] = cycles_of(tables, klass, &rd.m);
        }
    }

    // Shortest path over (band, last class). The class symbol for band `b` is read from the
    // context the *previous* band's class selected, so the bands are not independent once
    // the context codebooks differ — which the default set's do not (all 15 rows are
    // byte-identical) but a trained set's will. This is the general form; with identical
    // rows it collapses to the per-band greedy that a single context would allow.
    let k = tables.k;
    let mut suffix = [0f64; NBANDS + 1];
    for b in (0..NBANDS).rev() {
        suffix[b] = suffix[b + 1] + energy[b];
    }
    const NONE: usize = usize::MAX;
    let mut v = [[f64::INFINITY; MAXK + 1]; NBANDS + 1];
    let mut from = [[NONE; MAXK + 1]; NBANDS + 1];
    v[0][k] = 0.0; // the reader's initial context is the sentinel `k`
    // Terminating before band 0 is a real option: the channel is silence. It still sends
    // the terminator's band symbol, and the cycle term charges for that symbol whenever the
    // price is live — the `mu > 0` guard is what keeps `0 * cycles` off the `f64::INFINITY`
    // sentinels this search uses for unreachable states.
    let mut best_total = suffix[0]
        + if mu > 0.0 {
            mu * CY_BANDSYM as f64
        } else {
            0.0
        };
    let mut best_end = (0usize, k);
    for b in 0..NBANDS {
        for s in 0..=k {
            if v[b][s].is_infinite() {
                continue;
            }
            // Terminate here: the terminator's *bits* are deliberately unpriced (see
            // above); its band symbol still costs `mu` cycles when the price is live.
            let stop = v[b][s]
                + suffix[b]
                + if mu > 0.0 {
                    mu * CY_BANDSYM as f64
                } else {
                    0.0
                };
            if stop < best_total {
                best_total = stop;
                best_end = (b, s);
            }
            for c in 0..k {
                if band_dist[b][c].is_infinite() {
                    continue;
                }
                let j = v[b][s]
                    + lam * tables.cb[s].symbol_len(c) as f64
                    + band_dist[b][c]
                    + lam * band_bits[b][c] as f64
                    + if mu > 0.0 {
                        mu * (CY_BANDSYM + band_cyc[b][c]) as f64
                    } else {
                        0.0
                    };
                if j < v[b + 1][c] {
                    v[b + 1][c] = j;
                    from[b + 1][c] = s;
                }
            }
        }
    }
    // Coding every band needs no terminator, so it is a candidate end too.
    for s in 0..=k {
        if v[NBANDS][s] < best_total {
            best_total = v[NBANDS][s];
            best_end = (NBANDS, s);
        }
    }

    let (best_b, _) = best_end;
    // Backtrack the class of each coded band.
    let mut classes = vec![0usize; best_b];
    let mut s = best_end.1;
    for b in (0..best_b).rev() {
        classes[b] = s;
        s = from[b + 1][s];
        debug_assert_ne!(s, NONE, "the shortest path lost its predecessor");
    }

    // Re-derive the magnitudes for the classes the path chose. Keeping them for every
    // (band, class) would be 32 KB of dead alternatives per channel.
    let mut bands = Vec::with_capacity(best_b);
    let mut dist = 0f64;
    let mut bits = 0u64;
    let mut ctx = k;
    for (b, &klass) in classes.iter().enumerate() {
        bits += tables.cb[ctx].symbol_len(klass) as u64;
        ctx = klass;
        if klass == 0 {
            // A zero band inside the coded range still loses all of its energy, and that
            // energy is distortion like any other. Charging it in the shortest path but not
            // in the reported total would understate the cost of every plan that leaves a
            // hole in the middle of the spectrum — and this number is what the allocation
            // compares frames on.
            dist += energy[b];
            bands.push(Band::zero());
            continue;
        }
        let rd = band_rdo(
            &coef[b * BANDW..b * BANDW + BANDW],
            tables,
            klass,
            d,
            lam,
            mmax,
            band_ncod(b),
        )
        .expect("a class on the shortest path was codeable when it was costed");
        let mut band = Band {
            class: klass,
            m: [0; BANDW],
        };
        for i in 0..band_ncod(b) {
            let v = rd.m[i];
            band.m[i] = if v != 0 && coef[b * BANDW + i] < 0.0 {
                -v
            } else {
                v
            };
        }
        dist += rd.dist;
        bits += rd.bits;
        bands.push(band);
    }
    dist += suffix[best_b];
    if best_b < NBANDS {
        bits += tables.cb[ctx].symbol_len(k) as u64;
    }
    ChanRd {
        chan: Chan { bands },
        dist,
        bits,
    }
}
