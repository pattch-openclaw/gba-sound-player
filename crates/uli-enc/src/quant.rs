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

use uli_lite::format::{BANDW, CT_PAIR, CT_QUAD, CT_ZERO, NCODED};
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
    } else if start >= NCODED {
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
    let m = ((1i64 << 28) / step as i64 / 16 - 1) as i64;
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
    if f + 1 <= mmax {
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
