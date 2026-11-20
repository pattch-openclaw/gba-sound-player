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
//! The quantiser reproduces the reference's candidate enumeration rather than
//! inventing a round-to-nearest: `t = |x| * (1 / (step/16))`, candidates `{0, floor
//! t, floor t + 1}` clipped to `mmax`, first minimum wins. It matters — `{0, f, f+1}`
//! is not always the closest `m`, because `dq` shifts every reconstruction down by
//! `3 * step / 16`, and an encoder that rounds differently from the reference
//! encodes a different `m`, and therefore a different blob.

use uli_lite::format::{BANDW, CT_PAIR, CT_QUAD, CT_ZERO, NBANDS, NCODED};
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

/// The coefficients a band may hold nonzero values in: the format zeroes
/// everything at or beyond `NCODED`, and the reference codes whole bands, so a
/// band's coefficient count is 16, a partial `NCODED - 16b`, or 0.
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

/// The largest magnitude the format can carry without overflowing the reader's
/// `|X| < 2^28` guarantee: `(16m - dq) * step < 2^28`. The reference derives
/// `mmax` the same way and clamps it to `1..=65535`; the lower clamp matters
/// because a coarse `g` can make the quotient zero, and a band needs at least
/// `m = 1` to be codeable at all.
pub fn mmax_of(step: u32) -> i32 {
    let m = (1i64 << 28) / step as i64 / 16 - 1;
    m.clamp(1, 65535) as i32
}

/// The magnitude a single coefficient decodes to, under the reference's candidate
/// set. `ax` is `|coef|` in the analysis domain; `d` is `step / 16`; `dq` is the
/// row of the class the band will actually be coded with.
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
    // Candidates in the reference's order, first minimum wins on a tie: 0 only
    // when something nonzero is actually closer than nothing.
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

/// The class whose `dq` row sizes up the bands before any class is chosen.
///
/// The reference quantises *inside* each Viterbi state, so it always has a class;
/// S2 picks a class from the magnitudes, which needs a `dq` first. Every default
/// class carries the same row, so any choice is the same choice — and
/// `quantise_chan` re-quantises with the class it actually picked, which is what
/// keeps this honest once S4 trains `dq` per class.
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

/// The most candidates `make_cand` can offer for one coefficient.
const MAX_CANDS: usize = 3;

/// `make_cand` from `enc_core.c`, transcribed: `{0, floor t, floor t + 1}` clipped
/// to `mmax`, and **only** `0` when the coefficient is exactly zero or the band's
/// headroom is closed (`mmax <= 0`, i.e. at or past `NCODED`).
///
/// The order is the reference's and it matters: `0` is offered first and the
/// comparison that consumes these is strict, so a tie keeps the zero — the
/// cheaper symbol — rather than the nonzero one.
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
/// This is `band_rdo` from `enc_core.c`. The whole point of it — and the reason S2's
/// `lam = 0` quantiser was never going to reach the reference's rate — is that the
/// candidate set includes **zero**, so a coefficient whose magnitude is small
/// relative to its bit cost is dropped instead of coded. With `lam = 0` the zero is
/// never chosen; with `lam = kappa·D²` it wins whenever `x²` is cheaper to lose than
/// the bits are to spend, which at a coarse step is most of the spectrum.
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
    // A symbol with no code cannot be written, so it cannot be chosen — `make_cand`
    // offers it, the price list has nothing for it, and a `lam * 0` cost would make it
    // look free. The reference guards the same way (`int l = L[sym]; if (!l) continue;`)
    // and propagates failure up as `BIG`; here it is `None`, and the caller drops the
    // class. Inert with the default tables, where `huff_lengths` gives every symbol a
    // code — but S4's *trained* tables leave never-seen symbols at length 0, and this
    // is exactly the class of bug that would then encode an uncodeable symbol and
    // desync the stream silently.

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
            // No combination of this tuple is codeable under this class: the
            // reference propagates the same thing as `BIG` and the class is dropped.
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
/// Which classes S3 offers a band: **all of the nonzero ones**, which is what the
/// reference's Viterbi explores and what the measurement demanded.
///
/// The first version of this function tried a heuristic — the two tightest headroom
/// groups that could hold the band, with the group member chosen by matching the
/// model's `mu` to the band's mean magnitude — on the theory that going up a headroom
/// group must lose. Measured against the reference's own coder at a fixed step, it
/// cost 1.07×/1.16×/1.20× the bits at `g = 60/100/140`; offering every class brings
/// that to 0.9997×/0.9989×/0.968×, with distortion identical to the reference's to
/// seven significant figures. The heuristic's premise was wrong: a wider head group
/// shortens the *symbol code* by more than the raw field widens it, because the
/// Huffman table of a high-`mu` class prices small heads cheaply. Measure the class
/// ladder, do not reason about it.
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

/// Rate/distortion quantisation of one channel at a fixed step (S3).
///
/// Three decisions, all taken with `lam = kappa·D²`:
///
/// - **per band**, whether it is worth coding at all — class 0 costs one band symbol
///   and leaves the band's whole energy as distortion; a coded class costs its
///   coefficients and less distortion. S2 could not decline to code a band, because
///   with `lam = 0` coding is always weakly better.
/// - **per coefficient**, magnitude zero or not, which is what closes the rate gap
///   measured against the reference (1.41× at `g = 100`, 2.58× at `g = 140`).
/// - **where the spectrum ends.** The reader stops at the first terminator, so
///   everything from `B` on reconstructs as zero; `B` is chosen by scanning prefix
///   cost against the energy left uncoded. Exact rather than heuristic, because every
///   context table in the default set is identical (measured: all 15 rows equal), so
///   a band's class symbol costs the same from any context and the class sequence
///   carries no coupling. **That equality is what makes the scan optimal; S4's
///   trained tables break it and will need the reference's band-class Viterbi.**
pub fn quantise_chan_rd(
    coef: &[f32],
    tables: &DefaultTables,
    g: i32,
    ms: bool,
    kappa: f64,
) -> ChanRd {
    // M/S is S3b, and the gap is not cosmetic: the reference quantises a rotated frame
    // at `step*sqrt(2)/16`, while this function only shifts the step index. A frame
    // passed here with `ms = true` would be assigned the flag, written with it, and
    // quantised without the rotation — silently mis-scaled audio, which is the family
    // of bug this file has already produced twice. Loud beats plausible.
    assert!(
        !ms,
        "M/S quantisation is S3b: the 1/sqrt(2) forward rotation and the sqrt(2) step \
         compensation are not implemented, so a frame quantised with ms = true would be \
         silently mis-scaled. `GPlan` and the writer carry the flag; the quantiser is \
         what is missing."
    );
    let step = step_of(g, ms) as f64;
    let d = step / 16.0;
    let lam = kappa * d * d;
    let mmax = mmax_of(step as u32);
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

    // Best option per band, kept as the band itself so nothing is recomputed.
    let mut best_cost = [f64::INFINITY; NBANDS];
    let mut best_class = [0usize; NBANDS];
    let mut best_band: Vec<Band> = (0..NBANDS).map(|_| Band::zero()).collect();
    let mut best_bits = [0u64; NBANDS];
    let mut best_dist = [0f64; NBANDS];

    for b in 0..NBANDS {
        let ncod = band_ncod(b);

        // Option: code nothing in this band. One band symbol, all its energy lost.
        best_class[b] = 0;
        best_band[b] = Band::zero();
        best_bits[b] = zero_len;
        best_dist[b] = energy[b];
        best_cost[b] = energy[b] + lam * best_bits[b] as f64;

        for klass in rd_classes(tables) {
            // The reference guards its DP the same way (`int l = T.len_cb[s][c]; if (!l)
            // continue;`): a class whose band symbol has no code cannot be written, so it
            // must not be priced. Without this, a zero-length class symbol costs zero
            // bits and wins every comparison — the same shape as the coefficient-symbol
            // guard inside `band_rdo`, one level up.
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
            let bits = rd.bits + tables.cb[0].symbol_len(klass) as u64;
            let cost = rd.dist + lam * bits as f64;
            if cost < best_cost[b] {
                let mut band = Band {
                    class: klass,
                    m: [0; BANDW],
                };
                for i in 0..ncod {
                    let v = rd.m[i];
                    // The sign rides on the magnitude, as the writer expects.
                    band.m[i] = if v != 0 && coef[b * BANDW + i] < 0.0 {
                        -v
                    } else {
                        v
                    };
                }
                best_cost[b] = cost;
                best_class[b] = klass;
                best_band[b] = band;
                best_bits[b] = bits;
                best_dist[b] = rd.dist;
            }
        }
    }

    // Where does the spectrum end? Prefix cost against the energy left uncoded.
    let mut suffix = [0f64; NBANDS + 1];
    for b in (0..NBANDS).rev() {
        suffix[b] = suffix[b + 1] + energy[b];
    }
    let mut prefix = 0f64;
    let mut best_total = suffix[0]; // terminate before band 0: everything zero
    let mut best_b = 0usize;
    for b in 0..NBANDS {
        prefix += best_cost[b];
        // Stopping after `b + 1` bands owes the terminator; stopping after all
        // `NBANDS` owes none, because the reader's loop ends first.
        // The terminator is *not* priced, and that is deliberate fidelity rather
        // than an oversight: the reference's DP charges `lam` for a coded band's
        // bits and nothing for the EOB symbol, so pricing it here makes this
        // search terminate earlier than the reference's and diverge — measured at
        // `g = 140`: 2.5% more distortion and 3 bits/frame more, versus exact
        // agreement once the charge is dropped. It is the reference's quirk, and
        // S7 has to reproduce it to be byte-exact.
        let term = 0.0;
        let total = prefix + term + suffix[b + 1];
        if total < best_total {
            best_total = total;
            best_b = b + 1;
        }
    }

    let bands: Vec<Band> = best_band.into_iter().take(best_b).collect();
    let dist = best_dist[..best_b].iter().sum::<f64>() + suffix[best_b];
    let mut bits = best_bits[..best_b].iter().sum::<u64>();
    if best_b < NBANDS {
        bits += eob_len;
    }
    ChanRd {
        chan: Chan { bands },
        dist,
        bits,
    }
}
