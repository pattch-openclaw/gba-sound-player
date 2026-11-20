//! The rate/distortion search: choosing `g` per frame so the album lands on the
//! bit budget at the least distortion, subject to the decoder-cycle cap.
//!
//! This is where S1's `--cap` finally constrains something, and where rate stops
//! being an accident of the step. The shape is the reference's (`uli_enc.c`):
//! precompute an RD curve per frame, then bisect a Lagrangian multiplier λ until
//! the chosen points sum to the target rate. What the search trades is exactly
//! `w[f]·D + λ·R` per frame — weighted distortion against bits.
//!
//! **Three things are deliberately different from the reference, and each is a
//! byte-exactness caveat for S7, not an optimisation.**
//!
//! - The reference computes a *local* window of `NG` candidates around
//!   `g_guess(λ)`, sliding it while the cap is unmet; this walks a fixed stride
//!   across the whole `GMIN..=GMAX` window once. Cheaper to reason about and it
//!   makes the curves λ-independent, at the cost of resolution: the chosen `g` can
//!   sit up to `stride - 1` away from the reference's.
//! - The reference's target is `target_bits − 3.5·NF`. That allowance is *not*
//!   slack for loose accounting, as this file first assumed: it is the frame
//!   header. A candidate's measured cost omits the `g` delta and the M/S bit
//!   because those depend on the previous frame's `g` and so cannot be attributed
//!   to one candidate — and with `g` varying across the album they really cost
//!   ~5 bits/frame, so a search that ignores them overshoots the budget and trips
//!   its own E-R6 refusal. The same allowance is taken here, now for a known
//!   reason, and the final plan still checks the real stream.
//! - M/S is **not in the candidate set**. The reference rotates the coefficients by
//!   `1/√2` and quantises with `D = step·√2/16` while the reader combines the two
//!   channels with a plain integer add/subtract; that √2 needs a derivation of its
//!   own before it can be trusted, and a search that silently mis-scaled half the
//!   album is worse than a search without it. `Args::ms` is off, `Content` still
//!   carries the flag, and the plan says so.
//!
//! R5b and R5d are load-bearing here, not decoration: the allocation sums are
//! reduced in frame-index order, and ties break to the lower `g` because the
//! candidates are visited in ascending `g` and the comparison is strict.

use uli_lite::Ops;
use uli_lite::format::{CT_PAIR, M, NBANDS};
use uli_lite::tables_data::STEP;

use crate::pcm::{Pcm, Samples};
use crate::plan::{Content, GPlan};
use crate::quant::{self, Chan};
use crate::tables_default::DefaultTables;
use crate::{Error, FrameKind, frame};

/// The reference's search window (`enc_core.h`). It is **policy**, not a container
/// limit: the format accepts any `g` with `0 ≤ g − 4·ms < NSTEP`, and S1's refusal
/// enforces only that. Stepping outside this window is legal and worse.
pub const G_MIN: i32 = 8;
pub const G_MAX: i32 = 187;
/// The reference's default `--kappa`, the scale that turns distortion into bits.
pub const DEFAULT_KAPPA: f64 = 0.13;
/// The reference's λ bisection bracket and iteration count.
const LAMBDA_LO: f64 = -25.0;
const LAMBDA_HI: f64 = 40.0;
const LAMBDA_ITERS: usize = 64;
/// The frame period, and so the unit `--cap` is a percentage of.
pub const CYCLE_PERIOD: u32 = 262_144;

/// Everything the curve computation needs that is not the samples.
///
/// Bundled because the list had reached eight positional arguments, five of them numbers
/// with different meanings — the kind of signature where swapping `stride` and `cap` still
/// compiles. The command-line flags are all here, so this is also the one place to read to
/// see what the search is told.
#[derive(Clone, Copy, Debug)]
pub struct SearchParams<'a> {
    pub tables: &'a DefaultTables,
    pub n_frames: usize,
    /// Candidate spacing across `GMIN..=GMAX`, in step indices.
    pub stride: i32,
    /// The reference's `--kappa`: the distortion-to-bits scale.
    pub kappa: f64,
    /// `--cap PCT` expressed as cycles.
    pub cap: u32,
    /// `Some(_)` pins the domain; `None` lets the reference's rule choose per `g`.
    pub force_ms: Option<bool>,
}

/// One frame's RD curve: `stride`-spaced candidates, cheapest-rate-last order.
#[derive(Clone, Debug)]
pub struct Curve {
    /// Candidate step indices, ascending.
    pub g: Vec<i32>,
    /// Coded bits of each candidate, measured by coding it.
    pub r: Vec<f64>,
    /// Squared error of each candidate, in analysis units.
    pub d: Vec<f64>,
    /// Reader cycle model of each candidate.
    pub c: Vec<u32>,
    /// Whether each candidate is an M/S frame. The reference pre-chooses this *per `g`*
    /// (`uli_enc.c:165-171`) rather than letting the allocation choose it, so a candidate
    /// is a `(g, ms)` pair with `ms` already decided.
    pub ms: Vec<bool>,
}

/// One measured candidate.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub bits: u64,
    pub dist: f64,
    pub cycles: u32,
}

/// Measure one candidate exactly: code the frame, count what the reader will do.
///
/// The distortion counts every coefficient below `NCODED`, including those the
/// coder left zero because the band was never reached — the tail's energy is
/// distortion, not a free saving. Skipping it would make "code nothing" look
/// optimal, which is how a search like this goes wrong quietly.
pub fn measure(
    coef: &[f32; 2 * M],
    tables: &DefaultTables,
    g: i32,
    ms: bool,
    kappa: f64,
) -> Result<(Point, [Chan; 2]), Error> {
    // Rotate once for the frame, then quantise both channels in the rotated domain —
    // the reference's `frame_code` does exactly this, and the domain is what makes the
    // measured distortion comparable to an L/R frame's (see `PLAN-OF-RECORD.md` → S3b).
    let rot = quant::rotate(coef, ms);
    let mut chans = [Chan::empty(), Chan::empty()];
    let mut dist = 0f64;
    let mut bits = 0u64;
    for ch in 0..2 {
        let slice = &rot[ch * M..ch * M + M];
        let lr = crate::quant::quantise_chan_rd(slice, tables, g, ms, kappa);
        dist += lr.dist;
        chans[ch] = lr.chan;
    }
    let mut bw = crate::bitwriter::BitWriter::new();
    // The header is excluded from the measured bits: the `g` delta depends on the
    // previous frame's `g`, so charging it per candidate would double-count one bit or
    // thirty depending on order, and the `ms` bit belongs to the flag rather than to the
    // bands. The reference's curve omits both too; the final pass sizes the real stream,
    // and that is what E-R6 is checked against. `ops.ms` *is* set, because the cycle
    // model charges for it.
    for chan in &chans {
        frame::write_chan(&mut bw, tables, chan);
    }
    bits += bw.pos();
    let ops = frame_ops(tables, &chans, ms);
    Ok((
        Point {
            bits,
            dist,
            cycles: ops.cycle_model(),
        },
        chans,
    ))
}

/// `(coef − reconstruction)²` summed over the coded range. The reconstruction is
/// the *reader's* arithmetic — `(16|m| − dq) · step`, right-shifted back into
/// analysis units — so a disagreement here is a disagreement with the decoder and
/// not with a model of it.
/// The op counts the reader will report for these two channels.
///
/// This is decision 4's resolution in code: the encoder does not keep a second
/// cycle model, it builds the reader's own `Ops` and calls the reader's own
/// `cycle_model()`. `long_codes` is counted exactly — a symbol longer than the
/// first-level LUT takes the slow path, and the encoder knows every code length it
/// wrote.
pub fn frame_ops(tables: &DefaultTables, chans: &[Chan; 2], ms: bool) -> Ops {
    let mut ops = Ops::ZERO;
    // `ops.ms` is not decoration: the cycle model charges `3 * 512` for an M/S frame,
    // and this function hardcoded `0`. Nothing could see that while M/S was not
    // offered — and the moment it is, an under-counted cycle model silently over-admits
    // candidates against the `--cap` filter. Taking the flag as an argument is what
    // makes the omission impossible rather than merely absent.
    ops.ms = u32::from(ms);
    for chan in chans {
        let full = chan.bands.len() >= NBANDS;
        for band in &chan.bands {
            ops.bandsyms += 1;
            if tables.ctype[band.class] == uli_lite::format::CT_ZERO {
                continue;
            }
            ops.coded_bands += 1;
            let r = tables.rbits[band.class];
            let pair = tables.ctype[band.class] == CT_PAIR;
            for group in band.m.chunks(if pair { 2 } else { 4 }) {
                let heads: Vec<u32> = group
                    .iter()
                    .map(|m| {
                        let a = m.unsigned_abs();
                        if pair { (a >> r).min(15) } else { a.min(2) }
                    })
                    .collect();
                let sym = if pair {
                    (heads[0] * 16 + heads[1]) as usize
                } else {
                    (27 * heads[0] + 9 * heads[1] + 3 * heads[2] + heads[3]) as usize
                };
                let cls = &tables.cls[band.class];
                if cls.symbol_len(sym) as usize > uli_lite::format::LUTB {
                    ops.long_codes += 1;
                }
                if pair {
                    ops.pairs += 1;
                } else {
                    ops.quads += 1;
                }
                for &m in group {
                    let a = m.unsigned_abs();
                    if pair {
                        if (a >> r) >= 15 {
                            ops.escs += 1;
                        }
                        if r != 0 {
                            ops.raws += 1;
                        }
                    } else if a >= 2 {
                        ops.escs += 1;
                    }
                    if a != 0 {
                        ops.nz += 1;
                    } else {
                        ops.zc += 1;
                    }
                }
            }
        }
        if !full {
            ops.bandsyms += 1;
        }
    }
    ops
}

/// Per-frame weights, a port of `weights_plain`: every frame is weighted by the
/// inverse of its own track's mean energy, normalised to a geometric mean of one.
/// Without this the allocation would spend the album on its loudest track.
pub fn weights(pcm: &Pcm, energy: &[f64]) -> Vec<f64> {
    let n_frames = energy.len();
    let mut track_of = vec![0usize; n_frames];
    {
        let mut t = 0usize;
        for (f, slot) in track_of.iter_mut().enumerate() {
            while t + 1 < pcm.tracks.len()
                && (pcm.tracks[t + 1].start_sample as u64) <= (f as u64 * M as u64)
            {
                t += 1;
            }
            *slot = t;
        }
    }
    let n_tracks = pcm.tracks.len().max(1);
    let mut sum = vec![0f64; n_tracks];
    let mut count = vec![0f64; n_tracks];
    for f in 0..n_frames {
        sum[track_of[f]] += energy[f];
        count[track_of[f]] += 1.0;
    }
    let mean: Vec<f64> = (0..n_tracks)
        .map(|t| {
            let m = if count[t] > 0.0 {
                sum[t] / count[t]
            } else {
                0.0
            };
            // The `+ 1.0` is the reference's: it keeps a silent track's weight
            // finite, and a track of digital silence is exactly what a real album's
            // gap between songs looks like.
            m + 1.0
        })
        .collect();
    let mut w: Vec<f64> = (0..n_frames).map(|f| 1.0 / mean[track_of[f]]).collect();
    // `lg = exp(mean(log w))`, then normalise. `log(0)` cannot happen: every weight
    // is the reciprocal of something at least 1.
    let lg = (w.iter().map(|v| v.ln()).sum::<f64>() / n_frames.max(1) as f64).exp();
    for v in &mut w {
        *v /= lg;
    }
    w
}

/// The frame energy the weights are built from: `Σ coef²` over both channels.
pub fn frame_energy(coef: &[f32; 2 * M]) -> f64 {
    coef.iter().map(|&v| f64::from(v) * f64::from(v)).sum()
}

/// An album's worth of curves, computed once and reused across the λ bisection.
#[derive(Clone, Debug)]
pub struct Curves {
    pub per_frame: Vec<Curve>,
    pub stride: i32,
}

impl Curves {
    /// Code every frame at every candidate step. This is the expensive part of S3
    /// and the part S5 parallelises: frames are independent once `g` is fixed, so
    /// the same ordered-assembly argument as the emit pass applies.
    pub fn compute(samples: &Samples, n_samples: usize, p: &SearchParams<'_>) -> Result<Self, Error> {
        let SearchParams {
            tables,
            n_frames,
            stride,
            kappa,
            cap,
            force_ms,
        } = *p;
        let stride = stride.max(1);
        let mut per_frame = Vec::with_capacity(n_frames);
        let mut coef = [0f32; 2 * M];
        for f in 0..n_frames {
            crate::mdct::mdct_frame(samples, f, n_samples, &mut coef);
            let mut c = Curve {
                g: Vec::new(),
                r: Vec::new(),
                d: Vec::new(),
                c: Vec::new(),
                ms: Vec::new(),
            };
            let mut g = G_MIN;
            while g <= G_MAX {
                // The reference's rule, transcribed from `curves_compute`. Two details
                // matter and neither is obvious:
                //
                //  * each candidate is quantised with **its own** unit (`step_D(g, ms)`,
                //    so an M/S frame's magnitudes are sized by the √2-scaled step), but
                //  * the *comparison* between the two uses `lam` from the **ms = 0**
                //    unit, so the two are weighed on one scale rather than each on its
                //    own. Weighing them separately would systematically favour whichever
                //    unit happened to have the larger λ.
                //
                // And a candidate over the cycle cap loses to one that is not, before J
                // is consulted at all — the cap is a constraint, not a term.
                let lam0 = kappa * quant::Unit::of(g, false).d * quant::Unit::of(g, false).d;
                let (a, _) = measure(&coef, tables, g, false, kappa)?;
                let (b, _) = measure(&coef, tables, g, true, kappa)?;
                let fa = a.cycles <= cap;
                let fb = b.cycles <= cap;
                let useb = match force_ms {
                    Some(m) => m,
                    None => {
                        if fa == fb {
                            let jb = b.dist + lam0 * b.bits as f64;
                            let ja = a.dist + lam0 * a.bits as f64;
                            jb < ja
                        } else {
                            fb
                        }
                    }
                };
                let p = if useb { b } else { a };
                c.g.push(g);
                c.r.push(p.bits as f64);
                c.d.push(p.dist);
                c.c.push(p.cycles);
                c.ms.push(useb);
                if g == G_MAX {
                    break;
                }
                g = (g + stride).min(G_MAX);
            }
            per_frame.push(c);
        }
        Ok(Self { per_frame, stride })
    }

    /// Pick one candidate per frame for a given λ, minimising `w·D + λR` subject to
    /// the cycle cap. Frame-index order throughout (R5b); ascending candidates with
    /// a strict comparison, so a tie takes the lower `g` (R5d).
    fn pick(&self, w: &[f64], lambda: f64, cap: u32) -> Option<Vec<usize>> {
        let mut k = Vec::with_capacity(self.per_frame.len());
        for (f, curve) in self.per_frame.iter().enumerate() {
            let mut best = f64::INFINITY;
            let mut bk = None;
            for (i, &g) in curve.g.iter().enumerate() {
                let _ = g;
                if curve.c[i] > cap {
                    continue;
                }
                let j = w[f] * curve.d[i] + lambda * curve.r[i];
                if j < best {
                    best = j;
                    bk = Some(i);
                }
            }
            k.push(bk?);
        }
        Some(k)
    }

    /// Bisect λ until the chosen rate fits `target`. Returns `(lambda, picks)`.
    ///
    /// `hi` is the side that fits, so the answer is taken from `hi`, not from the
    /// midpoint: a λ slightly too generous overshoots the budget, and E-R6 makes
    /// overshooting a refusal rather than a rounding error.
    pub fn allocate(&self, w: &[f64], target: f64, cap: u32) -> Result<(f64, Vec<usize>), Error> {
        // Header allowance: the `g` delta plus the M/S bit, which no single
        // candidate owns. See the module doc; the reference uses the same number.
        let target = target - 3.5 * self.per_frame.len() as f64;
        let mut lo = LAMBDA_LO;
        let mut hi = LAMBDA_HI;
        for _ in 0..LAMBDA_ITERS {
            let mid = 0.5 * (lo + hi);
            let lambda = mid.exp();
            let picks = self.pick(w, lambda, cap).ok_or_else(|| {
                Error::Plan(format!(
                    "no candidate frame meets a cap of {cap} cycles ({:.1}% of {CYCLE_PERIOD}); \
                     --cap is a hard constraint here, and refusing is the honest answer",
                    100.0 * cap as f64 / CYCLE_PERIOD as f64
                ))
            })?;
            let total: f64 = picks
                .iter()
                .enumerate()
                .map(|(f, &i)| self.per_frame[f].r[i])
                .sum();
            if total > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let lambda = hi.exp();
        let picks = self.pick(w, lambda, cap).ok_or(Error::Plan(
            "the cap became unmeetable between the last probe and the final λ".into(),
        ))?;
        Ok((lambda, picks))
    }

    /// Turn the search's decisions into the content plan `Plan::new` and the emit
    /// pass consume. Re-codes each frame at its chosen step: the curves cached only
    /// the measurements, not the symbols, so the album is coded once more at exactly
    /// the rate the budget was checked against.
    pub fn to_content(
        &self,
        picks: &[usize],
        samples: &Samples,
        n_samples: usize,
        tables: &DefaultTables,
        kappa: f64,
    ) -> Result<Content, Error> {
        let n_frames = self.per_frame.len();
        let mut g_by_frame = Vec::with_capacity(n_frames);
        let mut ms_by_frame = Vec::with_capacity(n_frames);
        let mut chans = Vec::with_capacity(n_frames);
        let mut coef = [0f32; 2 * M];
        for (f, &pick) in picks.iter().enumerate() {
            let g = self.per_frame[f].g[pick];
            let ms = self.per_frame[f].ms[pick];
            g_by_frame.push(g);
            ms_by_frame.push(ms);
            crate::mdct::mdct_frame(samples, f, n_samples, &mut coef);
            let rot = quant::rotate(&coef, ms);
            let mut frame_chans = [Chan::empty(), Chan::empty()];
            for ch in 0..2 {
                let slice = &rot[ch * M..ch * M + M];
                frame_chans[ch] = quant::quantise_chan_rd(slice, tables, g, ms, kappa).chan;
            }
            chans.push(frame_chans);
        }
        Ok(Content::coded(GPlan::new(g_by_frame, ms_by_frame), chans))
    }

    /// The chosen steps, for the report.
    pub fn chosen_g(&self, picks: &[usize]) -> Vec<i32> {
        picks
            .iter()
            .enumerate()
            .map(|(f, &i)| self.per_frame[f].g[i])
            .collect()
    }
}

/// `--cap PCT` as a cycle count, the way the reference computes it.
pub fn cap_cycles(cap_pct: f64) -> u32 {
    (cap_pct / 100.0 * CYCLE_PERIOD as f64) as u32
}

/// The step the search would start from, ported from `g_guess` — kept for the
/// report line that says how far the stride landed from the reference's window.
pub fn g_guess(lambda: f64, w: f64, kappa: f64) -> i32 {
    let d = (lambda / (w * kappa)).sqrt();
    let g = (32.0 + 8.0 * d.log2()).round() as i32;
    g.clamp(G_MIN, G_MAX)
}

/// The largest magnitude the reader can carry at `g`, from the same `|X| < 2^28`
/// bound S1's refusal enforces. Exposed so `rd` and `quant` cannot disagree.
pub fn mmax_for(g: i32) -> i32 {
    quant::mmax_of(STEP[g as usize])
}

/// Frame kind for a coded channel pair, so callers do not reach into `frame`.
pub fn kind<'a>(chans: &'a [Chan; 2]) -> FrameKind<'a> {
    FrameKind::Coded(chans)
}
