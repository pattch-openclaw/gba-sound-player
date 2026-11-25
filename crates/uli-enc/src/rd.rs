//! The rate/distortion search: choosing `g` and the M/S flag per frame so the album
//! lands on the bit budget at the least distortion, subject to the decoder-cycle cap.
//!
//! This is where `--cap` finally constrains something, and where rate stops being an
//! accident of the step.
//!
//! **Why a price rather than a greedy allocation.** Each frame is measured at a set of
//! candidate steps, giving a curve of `(bits, distortion, cycles)` points. The budget is
//! a single constraint over the whole album, so frames cannot be decided one at a time
//! by any local rule. They can, however, be decided *independently* given one price `λ`:
//! if every frame minimises `w[f]·D + λ·R` on its own, the total lands on the budget for
//! exactly one `λ`, because at that point every frame's marginal distortion per bit is
//! equal — spend one more bit anywhere and you get the same distortion back. So the
//! search bisects `λ`, which is a monotone function of total rate, and the frames stay
//! trivially parallel.
//!
//! `w[f]` normalises each frame by its track's mean energy. Without it a fixed `λ` buys
//! more absolute distortion reduction where the signal is loud, so the allocation spends
//! the album on its loudest passage; the weights make the trade relative to each track's
//! own scale, and are normalised to a geometric mean of one so that `λ` keeps its units.
//!
//! **The candidate grid is one uniform sweep, and there is no knob for it.** Every frame is
//! measured every `CANDIDATE_STRIDE` steps across `G_MIN..=G_MAX` (S8-0). The two-stage sweep
//! this file used to advertise — a coarse grid, then a fine window around each frame's coarse
//! optimum — is **deleted rather than defaulted off**, because it was a repair for a coarse
//! grid this encoder invented and the reference has no equivalent flag (S8's inventory rows 6,
//! 7 and 12). Its measured 2.6 dB was against our own coarse baseline, not against the
//! reference, so it is not evidence of an improvement; it returns at S9 only as a labelled
//! arm admitted by the SNR licence test.
//!
//! **The grid is a shape, not a decision, and S8-7 owns it.** The reference does not sweep a
//! grid at all: it searches `NG = 28` *contiguous* steps centred on `g_guess(λ_train, w) − 7`
//! on its first pass and `NG = 20` contiguous steps centred on its pass-1 answer on the
//! second. Reproducing those windows is S8-7 (inventory rows 6 and 7), and until it lands
//! this sweep is our shape: it reaches steps the reference's window cannot — exactly what
//! S8-3 measured at frame 0 of `biomaster` (ours `g = 8`, the reference `135`). What S8-0
//! removed is only the *knob* and the refinement stage; the grid spacing itself is a constant
//! here on purpose, because the flag surface has to equal the reference's and on that surface
//! the candidate set is not a setting.
//!
//! Sweeping `stride = 1` instead would be a quieter improvement and it is deliberately not
//! taken: it can only lower distortion, so it would be an *output claim* in a step that makes
//! none, and it costs ~8x on every album encode in the tree (measured: the 4 s `fast` cell at
//! four threads goes 12 s → 82 s, against the oracle's 2.4 s). A finer grid is S8-7's, arrived at
//! from the reference's own windows rather than from this constant.
//!
//! **A candidate's measured cost excludes the frame header.** The `g` delta and the M/S
//! bit belong to the *sequence*: the delta depends on the previous frame's `g`, so
//! charging it per candidate would double-count one bit or thirty depending on visit
//! order, and the flag is not a property of the bands at all. Their real cost is
//! ~5 bits/frame when `g` moves, so the search subtracts a per-frame header allowance from
//! the target and the final plan checks the *real* stream against the budget afterwards —
//! which is what E-R6 is actually about.
//!
//! **Two domains, one scale.** M/S is part of the candidate set, but at each `g` the two
//! domains are quantised with their own unit (an M/S frame's magnitudes are sized by a
//! `√2`-scaled step) and then *compared* with a single `λ` computed from the L/R unit.
//! Comparing them each on their own unit's `λ` would weigh them on different scales and
//! systematically favour whichever had the larger. The cycle cap is a constraint rather
//! than a term: a candidate over the cap loses before `J` is consulted at all.
//!
//! R5b and R5d are load-bearing here, not decoration: the allocation sums are reduced in
//! frame-index order, and ties break to the lower `g` because candidates are visited in
//! ascending `g` and the comparison is strict.

use uli_lite::Ops;
use uli_lite::format::{CT_PAIR, M, NBANDS};
use uli_lite::tables_data::STEP;

use crate::pcm::{Pcm, Samples};
use crate::plan::{Content, GPlan};
use crate::quant::{self, Chan};
use crate::tables_default::DefaultTables;
use crate::{Error, FrameKind, frame};

/// The window the search considers. It is **policy**, not a container limit: the format
/// accepts any `g` with `0 ≤ g − 4·ms < NSTEP`, and the plan refuses only what the reader
/// would reject. Outside this window a frame is legal and simply worse — the bottom of the
/// range wastes bits on precision nothing can hear, the top leaves audible error.
pub const G_MIN: i32 = 8;
pub const G_MAX: i32 = 187;
/// The spacing of the candidate sweep, in step indices. **A shape, not a setting** (S8-0):
/// there is no flag for it, because the acceptance target is the reference's flag surface and
/// on that surface the candidate set is not a knob. S8-7 replaces this sweep with the
/// reference's contiguous windows; until then this is what stays (see the module doc for why
/// `stride = 1` is not taken in the meantime).
const CANDIDATE_STRIDE: i32 = 8;
/// The default `--kappa`: the price of a bit in units of squared error, before `λ` scales
/// it by the step. Chosen so that at a typical rate the search sits near the knee of the
/// rate/distortion curve rather than on either flat end of it.
pub const DEFAULT_KAPPA: f64 = 0.13;
/// The λ bracket, in log space, and the bisection's depth. `2^-25 … 2^40` spans every
/// operating point from "bits are free" to "bits are everything" by many orders of
/// magnitude; 64 halvings resolve `λ` far finer than the candidate grid can express, so the
/// bisection is not the limiting precision.
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
    /// The distortion-to-bits scale, `--kappa`.
    pub kappa: f64,
    /// `--cap PCT` expressed as cycles.
    pub cap: u32,
    /// Whether an over-cap candidate is re-coded with the decoder-cycle price instead of
    /// being left for the allocation to exclude (`--musearch`, S6). Off is the S5 search.
    pub musearch: bool,
    /// `Some(_)` pins the domain; `None` lets the search choose per `g` (the rule is in
    /// [`Curves::compute`]).
    pub force_ms: Option<bool>,
    /// Workers for the curve passes. Frames are independent once the tables are
    /// fixed, so this is where E-R9's frame-parallelism lives; assembly is by frame
    /// index, never by completion order (R5a), and each frame's own float sums stay
    /// in their written order (R5b).
    pub threads: usize,
}

/// One frame's RD curve: one candidate every [`CANDIDATE_STRIDE`] steps across the policy
/// window, cheapest-rate-last order.
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
    /// Whether each candidate is an M/S frame. The domain is decided *per `g`*, inside the
    /// curve, rather than by the allocation: at a fixed step the two domains are two
    /// concrete codings of the same frame, so one of them strictly dominates on
    /// `w·D + λ·R`, and letting the allocation choose would compare them under a `λ` that
    /// had already moved. A candidate is therefore a `(g, ms)` pair with `ms` settled.
    pub ms: Vec<bool>,
    /// The cycle price each candidate was re-coded under (S6). `0.0` means the candidate
    /// already met the cap; a positive value is the `mu` that bought it under. The final
    /// encode re-codes at exactly this price, so the shipped bands are the measured ones.
    pub mu: Vec<f64>,
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
/// **`dist` is derived, not summed.** The reference computes a frame's distortion out of
/// its Lagrangian — `D = J − λ·(bits − 1)` (`enc_core.c:404`), where `J` is the two channels'
/// returned `J` and `bits` counts the M/S bit — and that is the number its `allocate`
/// compares frames on. Summing the per-band squared errors instead gives a value that
/// differs by tens of ULP on a matching coding (measured: 76), which is a different plan the
/// moment it lands on a `<`. So the measurement reproduces the derivation (S8-5 item 1,
/// inventory row 1).
///
/// The `−1` is already taken out here: this function's `bits` is the bands' bit count, and
/// the reference's starts at 1 for the M/S bit the header carries. `D = J − λ·bits` is the
/// same expression written in this function's units.
///
/// The distortion still accounts for every coefficient below `NCODED` — the tail's energy is
/// distortion, not a free saving, and it arrives through the path's `suffix`/`Dz` term.
pub fn measure(
    coef: &[f32; 2 * M],
    tables: &DefaultTables,
    g: i32,
    ms: bool,
    kappa: f64,
    mu: f64,
) -> Result<(Point, [Chan; 2]), Error> {
    // Rotate once for the frame, then quantise both channels in the rotated domain. The
    // rotation is orthonormal, which is what makes a rotated frame's measured distortion
    // directly comparable to an L/R frame's — the two output channels err by
    // `(e_M ± e_S)/√2`, whose squares sum to `e_M² + e_S²`.
    let rot = quant::rotate(coef, ms);
    let mut chans = [Chan::empty(), Chan::empty()];
    let mut j = 0f64;
    for ch in 0..2 {
        let slice = &rot[ch * M..ch * M + M];
        let lr = crate::quant::quantise_chan_rd(slice, tables, g, ms, kappa, mu);
        j += lr.j;
        chans[ch] = lr.chan;
    }
    let d = quant::Unit::of(g, ms).d;
    let lam = kappa * d * d;
    let mut bw = crate::bitwriter::BitWriter::new();
    // The header is excluded from the measured bits: the `g` delta depends on the
    // previous frame's `g`, so charging it per candidate would double-count one bit or
    // thirty depending on order, and the `ms` bit belongs to the flag rather than to the
    // bands. The final pass sizes the real stream and checks that against the budget, which
    // is what E-R6 is about. `ops.ms` *is* set, because the cycle model charges for it.
    for chan in &chans {
        frame::write_chan(&mut bw, tables, chan);
    }
    let bits = bw.pos();
    let dist = j - lam * bits as f64;
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

/// One candidate with the cycle-aware re-code applied (S6).
///
/// Measure at `mu = 0` first. If the search is off, or the candidate already meets the cap,
/// that is the answer. Otherwise raise a price on decoder cycles until the coding fits, then
/// bisect *down* to the cheapest price that still does. The two phases are separate because
/// `mu` enters the search multiplicatively: the admissible prices form a ray `mu >= mu*`,
/// so there is nothing to interpolate towards — find one by growing a price, then narrow it
/// geometrically. If no price reaches the cap the frame is genuinely over the cap at this
/// step: the plain candidate comes back unchanged and the allocation's cap filter excludes
/// it, which is the honest outcome rather than a re-code that quietly broke the constraint.
///
/// The seed, the growth ratio and the two iteration counts come from the acceptance target,
/// because S7 compares bytes: a different ladder lands on a different `mu` and therefore on
/// different bands. They are not tuned for speed.
///
/// The returned `mu` is the price the winning coding was made under, and the final encode
/// must re-code at exactly it: the bands that met the cap are a function of that price.
pub fn point(
    coef: &[f32; 2 * M],
    p: &SearchParams<'_>,
    g: i32,
    ms: bool,
) -> Result<(Point, [Chan; 2], f64), Error> {
    let (plain, chans) = measure(coef, p.tables, g, ms, p.kappa, 0.0)?;
    if !p.musearch || plain.cycles <= p.cap {
        return Ok((plain, chans, 0.0));
    }
    let d = quant::Unit::of(g, ms).d;
    let lam = p.kappa * d * d;
    let mut lo = 0.0;
    let mut hi = lam * 0.01;
    let mut rh = measure(coef, p.tables, g, ms, p.kappa, hi)?;
    let mut it = 0;
    while rh.0.cycles > p.cap && it < 14 {
        lo = hi;
        hi *= 2.5;
        rh = measure(coef, p.tables, g, ms, p.kappa, hi)?;
        it += 1;
    }
    if rh.0.cycles > p.cap {
        return Ok((plain, chans, 0.0));
    }
    for _ in 0..6 {
        let mid = if lo > 0.0 { (lo * hi).sqrt() } else { hi * 0.3 };
        let rm = measure(coef, p.tables, g, ms, p.kappa, mid)?;
        if rm.0.cycles <= p.cap {
            hi = mid;
            rh = rm;
        } else {
            lo = mid;
        }
    }
    Ok((rh.0, rh.1, hi))
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
                // `ops.long_codes` is deliberately **not** counted here, and not because it
                // does not matter: the reference's *encoder* never sets that field.
                // `long_codes` exists in `UliOps` for the reader, which charges it whenever a
                // Huffman code is longer than `LUTB` (`uli_dec_ref.c:32`), and `frame_code`
                // passes `uli_cycle_model` an ops struct whose field is therefore always zero
                // — `grep long_codes encoder/*.c` finds no writer. Counting it made every
                // coded frame's estimate read high by exactly `40 * long_codes` (`40` is
                // `ULI_CY_LONGCODE`'s weight), which put our `--cap` admission above the
                // reference's. Measured, `cycles - 40*long_codes` **is** the reference's number
                // on 23 of 24 frozen codings, and the one residual is the equal-cost tie whose
                // coding genuinely differs (S8 inventory row 14, `PLAN-OF-RECORD.md`).
                //
                // Dropping it is what matching means, and it is no claim that the estimate is
                // now honest: our count was never the reader's either, because it counted
                // coefficient symbols and never band symbols. A faithful estimate would count
                // every code read — which is a better number, so it belongs behind the S9
                // licence test rather than inside the exactness gate. The *decoder* keeps
                // counting: `tables.rs` charges them on real streams, where they are real.
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
            // The `+ 1.0` keeps a silent track's weight finite: a track of digital
            // silence is exactly what a gap between songs looks like, and dividing by zero
            // there would poison every other frame's weight through the normalisation.
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
}

impl Curves {
    /// Code every frame at every candidate step. This is the expensive part of S3 —
    /// and, measured at S5's sizes, the part that made a 43-minute album take tens of
    /// minutes on one thread: it is now frame-parallel (`p.threads`), with the same
    /// ordered-assembly rule as the emit pass (R5a).
    ///
    /// Since S8-0 this is one uniform sweep per frame with no refinement stage, so it costs
    /// what the deleted coarse grid cost on its own — the fine pass's ~9 extra candidates per
    /// frame are gone and the grid spacing is unchanged.
    pub fn compute(
        samples: &Samples,
        n_samples: usize,
        p: &SearchParams<'_>,
    ) -> Result<Self, Error> {
        let n_frames = p.n_frames;
        let per_frame = map_frames(samples, n_samples, p, |_f, coef| curve(coef, p))?;
        debug_assert_eq!(per_frame.len(), n_frames, "one curve per frame");
        Ok(Self { per_frame })
    }

    /// Measure one `(g, ms)` candidate: re-code each domain under the cycle price if it is
    /// over the cap, take the cheaper under one price, and report the winner with its `mu`.
    fn measure_g(
        coef: &[f32; 2 * M],
        p: &SearchParams<'_>,
        g: i32,
    ) -> Result<(Point, bool, f64), Error> {
        let lam0 = p.kappa * quant::Unit::of(g, false).d * quant::Unit::of(g, false).d;
        let (a, _, mua) = point(coef, p, g, false)?;
        let (b, _, mub) = point(coef, p, g, true)?;
        let fa = a.cycles <= p.cap;
        let fb = b.cycles <= p.cap;
        let useb = match p.force_ms {
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
        Ok((if useb { b } else { a }, useb, if useb { mub } else { mua }))
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
        // The header allowance: the `g` delta plus the M/S bit, which belong to the
        // sequence and so to no single candidate. 3.5 bits/frame is what they cost when
        // `g` moves by a few steps; the module doc has the reasoning.
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
            let mu = self.per_frame[f].mu[pick];
            g_by_frame.push(g);
            ms_by_frame.push(ms);
            crate::mdct::mdct_frame(samples, f, n_samples, &mut coef);
            let rot = quant::rotate(&coef, ms);
            let mut frame_chans = [Chan::empty(), Chan::empty()];
            for ch in 0..2 {
                let slice = &rot[ch * M..ch * M + M];
                frame_chans[ch] = quant::quantise_chan_rd(slice, tables, g, ms, kappa, mu).chan;
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

/// Run `f` over every frame, frame-parallel, and return the results **by frame
/// index** — never by completion order (R5a).
///
/// This is the shape E-R9 asks for and the reason a 43-minute album encodes in
/// minutes rather than tens of them: every frame's MDCT and every candidate's
/// quantisation are independent, and only their *assembly* has to be ordered. Each
/// worker owns its coefficient scratch and its frames' float sums, so a frame's
/// numbers are the same at one thread or many (R5b); `chunks` is contiguous, so the
/// per-frame work does not depend on the split either.
fn map_frames<F>(
    samples: &Samples,
    n_samples: usize,
    p: &SearchParams<'_>,
    f: F,
) -> Result<Vec<Curve>, Error>
where
    F: Fn(usize, &[f32; 2 * M]) -> Result<Curve, Error> + Sync,
{
    let ranges = crate::plan::chunks(p.n_frames, p.threads.max(1));
    let parts: Vec<Result<Vec<Curve>, Error>> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for range in ranges {
            let f = &f;
            handles.push(scope.spawn(move || {
                let mut out = Vec::with_capacity(range.len());
                let mut coef = [0f32; 2 * M];
                for frame in range {
                    crate::mdct::mdct_frame(samples, frame, n_samples, &mut coef);
                    out.push(f(frame, &coef)?);
                }
                Ok(out)
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().expect("an RD-search thread panicked"))
            .collect()
    });
    let mut out = Vec::with_capacity(p.n_frames);
    for part in parts {
        out.extend(part?);
    }
    Ok(out)
}

/// One frame's curve: a point every [`CANDIDATE_STRIDE`] steps across `GMIN..=GMAX`, ascending.
///
/// Two rules decide which domain each candidate is in, and both matter:
///
///  * each is quantised with **its own** unit — an M/S frame's magnitudes are sized
///    by a √2-scaled step, because the reader's add recovers the amplitude and the
///    quantiser has to anticipate that;
///  * the two are then *compared* under one `λ`, computed from the L/R unit, so that
///    they are weighed on the same scale. Giving each its own unit's `λ` would
///    silently favour whichever came out larger.
///
/// A candidate over the cycle cap loses to one that is not before `J` is consulted
/// at all: the cap is a constraint, not a term.
fn curve(coef: &[f32; 2 * M], p: &SearchParams<'_>) -> Result<Curve, Error> {
    let mut c = Curve {
        g: Vec::new(),
        r: Vec::new(),
        d: Vec::new(),
        c: Vec::new(),
        ms: Vec::new(),
        mu: Vec::new(),
    };
    let mut g = G_MIN;
    while g <= G_MAX {
        let (point, useb, mu) = Curves::measure_g(coef, p, g)?;
        c.g.push(g);
        c.r.push(point.bits as f64);
        c.d.push(point.dist);
        c.c.push(point.cycles);
        c.ms.push(useb);
        c.mu.push(mu);
        if g == G_MAX {
            break;
        }
        g = (g + CANDIDATE_STRIDE).min(G_MAX);
    }
    Ok(c)
}

/// `--cap PCT` as a cycle count for one frame period.
pub fn cap_cycles(cap_pct: f64) -> u32 {
    (cap_pct / 100.0 * CYCLE_PERIOD as f64) as u32
}

/// An initial guess at the step a given `λ` and weight would choose, for diagnostics.
///
/// Inverting `J = w·D + λ·R` for a `D ∝ step²` model gives `step = √(λ/(w·κ))`, and
/// `g = 32 + 8·log₂(step)` is the inverse of the step table's `step = 2^(g/8)` scaling
/// about its midpoint. Today it says *where* an allocation landed without running the
/// search; S8-7 puts it on the search path, where it centres the reference's pass-1 window,
/// and that is why its rounding is stated rather than inherited: `lrint` (`uli_enc.c:48`)
/// resolves an exact half to **even**, so copying C's arithmetic with a ties-away-from-zero
/// `round` (inventory row 5) would move the window by one step on exactly the frames whose
/// guess is a half — and a one-step window shift changes the plan.
pub fn g_guess(lambda: f64, w: f64, kappa: f64) -> i32 {
    let d = (lambda / (w * kappa)).sqrt();
    let g = (32.0 + 8.0 * d.log2()).round_ties_even() as i32;
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
