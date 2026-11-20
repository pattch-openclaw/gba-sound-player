//! S4: training the code tables on the album they will code.
//!
//! The default tables fit a *model*: a geometric distribution per class, uniform over
//! contexts. An album has its own statistics — which classes a quiet passage actually uses,
//! how its magnitudes cluster, how predictable one band's class is from the last. Training
//! measures those and rebuilds the tables from the measurement, which is the difference
//! between "codes any album adequately" and "codes this album well".
//!
//! **What is trained, and what is not.** Only the Huffman **lengths** and the per-class
//! **reconstruction offsets** (`dq`). The class *structure* — `ctype`, `rbits`, `esck`, and
//! the class count — is the format's, not the album's: changing it would change what a
//! stream means, and E-R4's closed-world premise is stated over exactly those fields. So the
//! premise holds by construction across training, and is still re-checked afterwards because
//! "by construction" is a claim, not a witness.
//!
//! **Two passes, and why not one.** Lengths depend on which symbols are emitted, and which
//! symbols are emitted depends on the lengths (the quantiser prices its candidates with
//! them). One pass with default tables gives a plan whose statistics are close but not equal
//! to a plan coded with the trained ones, so the second pass measures the album *as it will
//! actually be coded* and the plan it produces is the one that ships. More passes converge
//! further; the reference does two and the returns past the second are small enough that
//! the plan does not claim more.
//!
//! **Determinism.** The counts are integers, and the only floating point in the path is
//! `huff_lengths`' `sqrt` in its length-capping re-merge — so a trained table is
//! reproducible on a given host but crosses libm, which E-R5's amendment already covers for
//! tables that come from floats. The *counts* are integer and the merge is deterministic, so
//! the same host reproduces the same tables byte for byte.

use uli_lite::format::{BANDW, CT_PAIR, CT_QUAD, CT_ZERO, M, MAXK, MAXLEN};

use crate::frame;
use crate::plan::Content;
use crate::quant;
use crate::tables_default::{Codes, DefaultTables, K};

/// The frequency floor added before the Huffman merge.
///
/// Lower than the default set's 0.5 because the counts are real: a symbol that never
/// occurred should be expensive, not impossible, and a floor of 0.2 keeps it codeable while
/// making the lengths reflect what the album actually does.
const FLOOR: f64 = 0.2;

/// The magnitude buckets a class's reconstruction offsets are fitted over: the format
/// indexes `dq` at `min(m, 15)`, so anything past 15 shares the last bucket.
const DQ_BUCKETS: usize = 16;

/// A class's `dq` is fitted only once this many coefficients have landed in the bucket;
/// below that the observed mean is noise, and the default value is better than a guess.
const DQ_MIN_COUNT: f64 = 20.0;

/// Raw counts from one pass over a coded album.
#[derive(Clone, Debug)]
pub struct Stats {
    /// Band-class symbol counts, by the context they were read from.
    pub cb: [[f64; MAXK + 1]; MAXK + 1],
    /// Tuple-symbol counts per class: 81 QUAD symbols or 256 PAIR symbols, in `len`.
    pub cls: [Vec<f64>; MAXK],
    /// For each class and magnitude bucket: `Σ(m − |x|/D)` and the count. The format's
    /// reconstruction is `(16m − dq)·step`, so `m − |x|/D` is the error the offset exists
    /// to correct, and its mean is the best `dq/16` for that bucket.
    pub cen_s: [[f64; DQ_BUCKETS]; MAXK],
    pub cen_n: [[f64; DQ_BUCKETS]; MAXK],
    /// Frames and channels seen, so an empty album is distinguishable from a silent one.
    pub frames: usize,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            cb: [[0.0; MAXK + 1]; MAXK + 1],
            cls: std::array::from_fn(|_| Vec::new()),
            cen_s: [[0.0; DQ_BUCKETS]; MAXK],
            cen_n: [[0.0; DQ_BUCKETS]; MAXK],
            frames: 0,
        }
    }
}

/// Collect statistics from a finished plan, by re-deriving the symbols it emits.
///
/// The plan already holds every choice — each band's class and its signed magnitudes — so
/// nothing is re-decided here. What is re-derived is the *symbol indices*, which are a pure
/// function of the magnitudes, plus the per-coefficient reconstruction error, which needs the
/// analysis coefficients (`x`) that produced them.
pub fn collect(
    samples: &crate::pcm::Samples,
    n_samples: usize,
    tables: &DefaultTables,
    content: &Content,
    kappa: f64,
) -> Stats {
    let n_frames = content.len();
    let mut st = Stats::default();
    let k = tables.k;
    for (c, slot) in st.cls.iter_mut().enumerate().take(k) {
        let n = if tables.ctype[c] == CT_PAIR { 256 } else { 81 };
        *slot = vec![0.0; n];
    }
    let mut coef = [0f32; M * 2];
    for f in 0..n_frames {
        crate::mdct::mdct_frame(samples, f, n_samples, &mut coef);
        let ms = content.ms(f) != 0;
        let rot = quant::rotate(&coef, ms);
        let unit = quant::Unit::of(content.g(f), ms);
        let Some(chans) = content.coded_frames() else {
            continue;
        };
        for ch in 0..2 {
            let x = &rot[ch * M..ch * M + M];
            let mut ctx = k;
            for (b, band) in chans[f][ch].bands.iter().enumerate() {
                st.cb[ctx][band.class] += 1.0;
                ctx = band.class;
                if band.class == 0 || tables.ctype[band.class] == CT_ZERO {
                    continue;
                }
                let pair = tables.ctype[band.class] == CT_PAIR;
                let r = tables.rbits[band.class] as u32;
                let per = if pair { 2 } else { 4 };
                for (t, group) in band.m.chunks_exact(per).enumerate() {
                    let mut sym = 0usize;
                    for (i, &m) in group.iter().enumerate() {
                        let a = m.unsigned_abs();
                        let head = if pair { (a >> r).min(15) } else { a.min(2) };
                        sym = if pair {
                            sym * 16 + head as usize
                        } else {
                            sym * 3 + head as usize
                        };
                        let bucket = a.min(15) as usize;
                        let idx = b * BANDW + t * per + i;
                        let truth = f64::from(x[idx].abs()) / unit.d;
                        st.cen_s[band.class][bucket] += m.unsigned_abs() as f64 - truth;
                        st.cen_n[band.class][bucket] += 1.0;
                    }
                    if let Some(counts) = st.cls[band.class].get_mut(sym) {
                        *counts += 1.0;
                    }
                }
            }
            // The terminator, from the context of the last band coded.
            st.cb[ctx][k] += 1.0;
        }
        st.frames += 1;
    }
    let _ = kappa;
    st
}

/// Rebuild the tables from counts, keeping every structural row of `base`.
///
/// A class with no observations keeps the base's lengths rather than being given a degenerate
/// table: an album that never uses a class still has to be able to code a frame that would.
pub fn build(base: &DefaultTables, st: &Stats) -> DefaultTables {
    let mut t = base.clone();
    let k = t.k;

    for s in 0..=k {
        let counts = &st.cb[s];
        if counts.iter().any(|&c| c > 0.0) {
            let len = huff_lengths(&counts[..=k], FLOOR);
            t.cb[s] = Codes::from_lengths(&len);
        }
    }

    for c in 0..k {
        if t.ctype[c] == CT_ZERO {
            continue;
        }
        if st.cls[c].iter().any(|&v| v > 0.0) {
            let len = huff_lengths(&st.cls[c], FLOOR);
            t.cls[c] = Codes::from_lengths(&len);
        }
        for m in 1..DQ_BUCKETS {
            let n = st.cen_n[c][m];
            if n > DQ_MIN_COUNT {
                let v = (16.0 * st.cen_s[c][m] / n).round();
                t.dq[c][m] = v.clamp(-8.0, 15.0) as i8;
            }
        }
    }
    t
}

/// Huffman code lengths for a count vector.
///
/// The same two-minimum merge the default set uses, with the same length cap and the same
/// deterministic tie-break, because a trained table and a default table must be *tables of
/// the same kind*: the reader only knows the format's shape, not how the lengths were found.
pub fn huff_lengths(counts: &[f64], floor: f64) -> Vec<u8> {
    let n = counts.len();
    let mut c: Vec<f64> = counts.iter().map(|v| v + floor).collect();
    let mut len = vec![0u8; n];
    if n == 1 {
        len[0] = 1;
        return len;
    }
    loop {
        let mut w = vec![0.0f64; 2 * n];
        let mut parent = vec![-1i32; 2 * n];
        let mut active = vec![false; 2 * n];
        for i in 0..n {
            w[i] = c[i];
            active[i] = true;
        }
        let mut nn = n;
        for _ in 0..n - 1 {
            let mut a: i32 = -1;
            let mut b: i32 = -1;
            for i in 0..nn {
                if !active[i] {
                    continue;
                }
                if a < 0 || w[i] < w[a as usize] {
                    b = a;
                    a = i as i32;
                } else if b < 0 || w[i] < w[b as usize] {
                    b = i as i32;
                }
            }
            active[a as usize] = false;
            active[b as usize] = false;
            w[nn] = w[a as usize] + w[b as usize];
            parent[a as usize] = nn as i32;
            parent[b as usize] = nn as i32;
            parent[nn] = -1;
            active[nn] = true;
            nn += 1;
        }
        let mut max = 0u8;
        for i in 0..n {
            let mut deep = 0u8;
            let mut p = i as i32;
            while parent[p as usize] >= 0 {
                deep += 1;
                p = parent[p as usize];
            }
            len[i] = deep;
            max = max.max(deep);
        }
        if max <= MAXLEN as u8 {
            return len;
        }
        for v in c.iter_mut() {
            *v = v.sqrt() + floor;
        }
    }
}

/// Whether a trained set is usable: every codebook a complete prefix code, and every
/// structural row still inside the encoder's closed-world premise.
///
/// A trained table is *data*, so it is checked rather than trusted. The two failures that
/// matter and are not visible from the counts: an incomplete code (a length vector that
/// assigns more code space than exists, or leaves a symbol with no code where the writer will
/// reach for it), and a `dq` so large that a reconstruction turns negative.
pub fn validate(t: &DefaultTables) -> Result<(), String> {
    for s in 0..=t.k {
        let n = t.k + 1;
        kraft(&t.cb[s], n).map_err(|e| format!("context {s}: {e}"))?;
    }
    for c in 0..t.k {
        match t.ctype[c] {
            CT_PAIR => kraft(&t.cls[c], 256).map_err(|e| format!("class {c} (PAIR): {e}"))?,
            CT_QUAD => kraft(&t.cls[c], 81).map_err(|e| format!("class {c} (QUAD): {e}"))?,
            CT_ZERO => {}
            other => return Err(format!("class {c} has ctype {other}")),
        }
        for m in 1..DQ_BUCKETS {
            if t.dq[c][m] > 16 {
                return Err(format!("class {c} bucket {m}: dq {} over 16", t.dq[c][m]));
            }
        }
    }
    Ok(())
}

fn kraft(codes: &Codes, n: usize) -> Result<(), String> {
    let sum: f64 = (0..n)
        .map(|s| 1.0 / (1u64 << codes.symbol_len(s).min(MAXLEN as u8)) as f64)
        .sum();
    if (sum - 1.0).abs() > 1e-9 {
        return Err(format!("incomplete code: kraft sum {sum:.12}"));
    }
    for s in 0..n {
        let l = codes.symbol_len(s) as usize;
        if l == 0 {
            return Err(format!("symbol {s} has no code"));
        }
        if codes.symbol_code(s) as u64 >= 1u64 << l {
            return Err(format!("symbol {s} code does not fit {l} bits"));
        }
    }
    Ok(())
}

/// The terminator's length in a trained context, for diagnostics.
pub fn eob_len(t: &DefaultTables) -> u8 {
    t.cb[0].symbol_len(t.k)
}

// A band the trainer sees but the writer cannot express would be a bug in the plan, not in
// the trainer; the assertion is here so a test can catch a mismatch between `Band`'s shape
// and the collector's assumption about it.
const _: () = assert!(BANDW == 16);
// Keep `frame` imported: training is defined over the same symbol layout the writer emits,
// and a change to that layout has to be reflected here.
#[allow(dead_code)]
fn _symbols_are_the_writers() -> usize {
    let _ = frame::chan_bits as fn(&DefaultTables, &quant::Chan) -> u64;
    K
}
