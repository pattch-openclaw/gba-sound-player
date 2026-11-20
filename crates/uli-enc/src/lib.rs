#![forbid(unsafe_code)]
//! `uli-enc` — the host-side ULI2 encoder, step S1.
//!
//! The encoder's contract boundary is the reference's own input pair: an
//! `album.f32` (float32 interleaved stereo at PCM16 scale, 32,768 Hz) and a
//! `tracks.tsv`, in; one ULA2 blob the `uli-lite` decoder accepts, out
//! (`PLAN-OF-RECORD.md` → "The encoder", E-R2).
//!
//! What S1 is: the plan-then-emit writer. It takes the album length from the
//! track table and requires `album.f32` to be exactly that long (E-R2), fixes the
//! tables and the container layout, takes the per-frame content plan (`g`, M/S) as
//! an argument to `Plan::new` and refuses one the reader cannot decode (the step-index
//! bound), checks the whole album against `--budget` (refusing before a frame is written), then
//! codes frame bodies in index order with the workers computing but never ordering
//! (R5a). Its frames carry **silence at `g = 0`**; the DC frame exists as
//! [`FrameKind::DcImpulse`] and
//! is exercised by the round-trip test. There is no MDCT, no quantiser, no RD
//! search and no table training: those are S2…S5. See `README.md`.
//!
//! `uli-lite` stays read-side (2026-11-19 later, decision 1), so the tables
//! blob geometry, the code-length computation and the canonical code assignment
//! all live here; the decoder's parser is the authority that catches a mistake.

pub mod bitwriter;
pub mod container;
pub mod frame;
pub mod mdct;
pub mod mdct_tables;
pub mod pcm;
pub mod plan;
pub mod quant;
pub mod rd;
pub mod tables_default;

use std::fmt;
use std::path::PathBuf;

use uli_lite::format::M;
use uli_lite::{Album, CodeTables, Decoder, DecoderState, PremiseViolation};

pub use container::FramePos;
pub use frame::{DC_G, FrameKind, chan_bits, frame_bits, write_chan, write_frame};
pub use pcm::{Pcm, Samples, TrackInput};
pub use plan::{
    Content, DEFAULT_BUDGET, DEFAULT_CAP_PCT, GPlan, GPlanViolation, GPlanViolationKind, Plan,
    Refusal, chunks, frame_count,
};
pub use rd::{Curves, DEFAULT_KAPPA, G_MAX, G_MIN, cap_cycles, measure, weights};
pub use tables_default::{Codes, DefaultTables};

/// Everything the CLI passes; the library reads no environment.
#[derive(Clone, Debug)]
pub struct Args {
    pub album: PathBuf,
    pub tracks: PathBuf,
    /// Whole-blob byte budget, never a cart size (E-R10).
    pub budget: u64,
    /// Percent of the 262,144-cycle frame period. S3 gives it something to
    /// constrain: candidates whose reader cycle model exceeds it are not offered.
    pub cap_pct: f64,
    /// The fixed global step index, or `None` for S1's album of silence. S2's only
    /// content knob: it chooses the step, and therefore the distortion, and the
    /// rate falls out of it. S3 replaces it with the RD search over `g`.
    pub g: Option<i32>,
    /// Search `g` per frame against the bit budget (S3). Needs `g` unset: either a
    /// fixed step or a search, not both.
    pub rd: bool,
    /// The search's Lagrangian distortion-to-bits scale, the reference's `--kappa`.
    pub kappa: f64,
    /// Candidate spacing across `GMIN..=GMAX`, in step indices.
    pub stride: i32,
    pub threads: usize,
}

/// What the encoder measured while producing the blob.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub overhead_bytes: usize,
    pub target_bits: i64,
    pub bits_per_frame: f64,
    pub n_frames: usize,
    /// Workers that actually coded a chunk; proves `--threads N` was exercised.
    pub workers_used: usize,
    pub est_max_cycles: u32,
    pub est_mean_cycles: u32,
    /// The fixed step the album was coded at, for the report line.
    pub g: Option<i32>,
    /// Every frame's chosen step, when the search picked them (S3).
    pub g_chosen: Option<Vec<i32>>,
    /// The Lagrangian multiplier the bisection converged on, and the bit target it
    /// was solving for. Both are printed: a rate that looks wrong is undiagnosable
    /// without them, as S3's first attempt proved.
    pub lambda: Option<f64>,
    /// Bits the stream actually used, from the decoder's own per-frame counts.
    pub stream_bits: u64,
    /// Codec-only SNR of the decoded int16 output against the input, in dB.
    /// `None` when the album was silence, where it has no meaning.
    pub snr_db: Option<f64>,
    /// `|X| >= 2^28` coefficients the reader counted: the encoder's `mmax` should
    /// make this impossible, and it is the number that says whether it did.
    pub xover: u32,
    /// Nonzero coefficients at index >= `NCODED`: likewise a format rule, not a
    /// quality knob.
    pub hfnz: u32,
    /// Coefficients that coded a nonzero magnitude, both channels.
    pub nonzero_coeffs: usize,
}

/// How many times the search may re-try the bit target before refusing.
const RETRY_ATTEMPTS: usize = 10;

/// The finished blob and its report.
#[derive(Clone, Debug)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub report: Report,
}

#[derive(Debug)]
pub enum Error {
    Pcm(String),
    Tracks(String),
    Tables(uli_lite::Error),
    Premise(PremiseViolation),
    GPlan(GPlanViolation),
    Plan(String),
    Refusal(Refusal),
    Decode(uli_lite::Error),
    Verify(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Pcm(m) | Error::Tracks(m) | Error::Plan(m) | Error::Verify(m) => f.write_str(m),
            Error::Tables(e) => write!(f, "tables blob does not parse: {e:?}"),
            Error::Premise(v) => write!(f, "tables violate the encoder premise (E-R4): {v}"),
            Error::GPlan(v) => write!(f, "the frame plan cannot decode: {v}"),
            Error::Refusal(r) => write!(f, "{r}"),
            Error::Decode(e) => write!(f, "uli-lite rejected the blob: {e:?}"),
        }
    }
}

impl std::error::Error for Error {}

/// Encode with the reference's default tables.
pub fn encode(args: &Args) -> Result<Output, Error> {
    let tables = DefaultTables::build();
    encode_with_tables(args, &tables)
}

/// Encode with caller-supplied tables.
///
/// This is the seam the E-R4 witness needs: the encoder must refuse a table
/// that breaks the closed-world premise before it produces a byte, which a test
/// can only observe if it can hand the encoder such a table. Production callers
/// use [`encode`].
pub fn encode_with_tables(input: &Args, tables: &DefaultTables) -> Result<Output, Error> {
    // Destructure exhaustively, on purpose. `--rd` once parsed into a field that
    // nothing read: the crate built, the suite stayed green, and the encoder emitted
    // silence. A binding pattern with no `..` makes that a compile error — adding a
    // field to `Args` without deciding what `encode` does with it no longer survives
    // to become a test result, let alone a shipped album.
    let Args {
        album,
        tracks,
        budget,
        cap_pct,
        g,
        rd,
        kappa,
        stride,
        threads,
    } = input;
    // The parameter is `input`, not `args`, so there is no way to reach a field
    // through it and quietly bypass the bindings above. That is what the first
    // version of this guard missed: `args.rd` still resolved, so the exhaustive
    // pattern proved nothing.
    let (album, tracks) = (album.as_path(), tracks.as_path());
    let (budget, cap_pct, g, rd, kappa, stride, threads) =
        (*budget, *cap_pct, *g, *rd, *kappa, *stride, *threads);
    let _ = kappa; // S3's remaining substance: the per-coefficient RD it should scale.
    let pcm = Pcm::open(album, tracks)?;
    let mut report_g: Option<Vec<i32>> = None;
    let mut report_lambda: Option<f64> = None;

    // The premise is a property of the *final* tables, so serialise and re-parse
    // them first: a bound checked against the in-memory model would not prove
    // what the blob says, and this also catches a wrong blob layout before any
    // frame exists.
    let blob = tables.to_blob();
    let mut parsed = CodeTables::new();
    parsed.parse_into(&blob).map_err(Error::Tables)?;
    parsed.premise().map_err(Error::Premise)?;

    // S2's fork: a fixed step means real content, no step means S1's album of
    // silence. The silence path is kept because half the suite is about the
    // container and the refusals, and they should not depend on the codec.
    // Three contents, one knob: no `--g`/`--rd` is S1's silence (so the container
    // and the refusals never depend on the codec), `--g N` is S2's fixed step, and
    // `--rd` is S3's search.
    let nf = frame_count(pcm.n_samples);
    let samples = if g.is_some() || rd {
        Some(Samples::open(album, pcm.n_samples as usize)?)
    } else {
        None
    };
    let content = if rd {
        // S3's chicken-and-egg: the search needs the bit target, the bit target
        // needs the container overhead. Overhead is a function of the track table,
        // the tables blob, the seek count and the strings — none of which frame
        // content can change — so the target is taken from a silence-sized plan and
        // the real plan is then checked against the real stream, as below.
        let sizing = Plan::new(&pcm, tables, &Content::silence(nf), budget, cap_pct)?;
        let s = samples.as_ref().expect("--rd loads the album above");
        let curves = Curves::compute(s, pcm.n_samples as usize, tables, nf, stride)?;
        let mut coef = [0f32; 2 * M];
        let energy: Vec<f64> = (0..nf)
            .map(|f| {
                mdct::mdct_frame(s, f, pcm.n_samples as usize, &mut coef);
                rd::frame_energy(&coef)
            })
            .collect();
        let w = rd::weights(&pcm, &energy);
        let cap = rd::cap_cycles(cap_pct);
        // The retry loop, which is what closes the budget residue. The search solves
        // on bits measured from a candidate's *own* frame; the real stream also pays
        // the `g` delta (which a stride-8 walk makes expensive), the terminator and
        // the halfword padding, none of which any candidate owns. So the plan is
        // built, checked, and the target lowered by exactly what it overshot — the
        // reference's `tgt -= (size - budget) * 8 * 1.02 + 2000`, with the same
        // 2% and 2000-bit margins. A budget this cannot meet is refused by
        // `Plan::new` on the last attempt, citing both numbers (E-R6).
        let mut target = sizing.target_bits as f64;
        let mut chosen: Option<(Content, Vec<i32>, f64)> = None;
        for attempt in 0..RETRY_ATTEMPTS {
            let (lambda, picks) = curves.allocate(&w, target, cap)?;
            let content = curves.to_content(&picks, s, pcm.n_samples as usize, tables)?;
            match Plan::new(&pcm, tables, &content, budget, cap_pct) {
                Ok(_) => {
                    chosen = Some((content, curves.chosen_g(&picks), lambda));
                    break;
                }
                Err(Error::Refusal(r)) => {
                    if attempt + 1 == RETRY_ATTEMPTS {
                        return Err(Error::Refusal(r));
                    }
                    let over = (r.needed as f64 - r.allowed as f64) * 8.0 * 1.02 + 2000.0;
                    target -= over;
                    if target < 1000.0 {
                        // The album cannot be coded that cheaply; say so with the
                        // numbers rather than looping into a degenerate target.
                        return Err(Error::Refusal(r));
                    }
                }
                Err(e) => return Err(e),
            }
        }
        let Some((content, gs, lambda)) = chosen else {
            return Err(Error::Plan(
                "the bit target did not converge in the retry loop".into(),
            ));
        };
        report_g = Some(gs);
        report_lambda = Some(lambda);
        content
    } else if let Some(g) = g {
        Content::quantise(
            samples.as_ref().expect("--g loads the album above"),
            pcm.n_samples as usize,
            tables,
            g,
            kappa,
            nf,
        )?
    } else {
        Content::silence(nf)
    };

    let plan = Plan::new(&pcm, tables, &content, budget, cap_pct)?;

    let ranges = chunks(plan.n_frames, threads.max(1));

    let coded: Vec<(bitwriter::BitWriter, Vec<u64>)> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for range in &ranges {
            // Borrow, do not move. `Content` is not `Copy` and every worker needs
            // it, so each closure captures a shared reference; without this line
            // the first `move` closure takes the album's content out of the loop.
            let content = &content;
            handles.push(scope.spawn(move || {
                let mut bw = bitwriter::BitWriter::new();
                let mut lens = Vec::with_capacity(range.len());
                // Entering state for this chunk's first frame. `write_frame`
                // advances it from the plan, and the assert below is what proves
                // the running state and the container's recorded state never
                // drift apart mid-chunk.
                let mut g_prev = content.g_prev(range.start);
                for f in range.clone() {
                    debug_assert_eq!(
                        g_prev,
                        content.g_prev(f),
                        "the running g state disagrees with the plan at frame {f}: a seek \
                         into this chunk would then land on a different frame than the \
                         linear decode walks"
                    );
                    let start = bw.pos();
                    frame::write_frame(
                        &mut bw,
                        tables,
                        &mut g_prev,
                        content.g(f),
                        content.ms(f),
                        content.kind(f),
                    );
                    lens.push(bw.pos() - start);
                }
                (bw, lens)
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().expect("a frame-coding thread panicked"))
            .collect()
    });

    // Assembly walks frames by index (R5a): chunk completion order never reaches
    // the output, so any --threads produces the same bytes.
    let mut stream = bitwriter::BitWriter::new();
    let mut fpos: Vec<FramePos> = Vec::with_capacity(plan.n_frames);
    for (range, (bw, lens)) in ranges.iter().zip(coded.iter()) {
        let mut off = 0u64;
        for (i, &len) in lens.iter().enumerate() {
            let f = range.start + i;
            fpos.push(FramePos {
                bitpos: stream.pos(),
                g_prev: content.g_prev(f),
            });
            stream.append_bits(bw.halfwords(), off, len);
            off += len;
        }
    }
    let stream_bytes = stream.finish();

    let mut bytes = container::emit(&plan, tables, &fpos, &stream_bytes);

    let verify = self_verify(&bytes, plan.n_frames, samples.as_ref(), &pcm)?;
    container::patch_cycles(&mut bytes, verify.max_cycles, verify.mean_cycles);

    // Re-parse the *patched* blob: the cycle fields are the last bytes written,
    // and a reader that only sees the pre-patch bytes has not seen the output.
    let album = Album::parse(&bytes).map_err(Error::Decode)?;
    let mut patched = CodeTables::new();
    patched.parse_into(album.tables()).map_err(Error::Tables)?;
    patched
        .premise()
        .map_err(|v| Error::Verify(format!("the patched blob's tables break E-R4: {v}")))?;

    Ok(Output {
        bytes,
        report: Report {
            overhead_bytes: plan.overhead_bytes,
            target_bits: plan.target_bits,
            bits_per_frame: plan.bits_per_frame,
            n_frames: plan.n_frames,
            workers_used: coded.len(),
            est_max_cycles: verify.max_cycles,
            g,
            g_chosen: report_g,
            lambda: report_lambda,
            est_mean_cycles: verify.mean_cycles,
            stream_bits: verify.bits,
            snr_db: verify.snr_db,
            xover: verify.xover,
            hfnz: verify.hfnz,
            nonzero_coeffs: content.nonzero_coeffs(),
        },
    })
}

/// What a verification decode of the finished blob measured.
#[derive(Clone, Copy, Debug)]
pub struct Verify {
    pub max_cycles: u32,
    pub mean_cycles: u32,
    pub bits: u64,
    /// Codec-only SNR against the input, or `None` if there were no samples to
    /// compare against (S1's silence path never loads them).
    pub snr_db: Option<f64>,
    pub xover: u32,
    pub hfnz: u32,
}

/// Decode the finished stream with the portable decoder: the cycle model, the
/// stream's real bit count, the reader's own format-rule diagnostics, and the
/// SNR of what came back out.
///
/// This is a *validity* pass plus the two numbers S2 exists to report. It decodes
/// every frame, so its cost is the album's decode cost — acceptable at S2's sizes,
/// and the place to revisit when the bake starts feeding 43-minute albums through
/// it (decision 4 names the second pass this implies).
///
/// The sample alignment is the reference's own, `p = (f - 1) * M + i`: frame 0 is
/// the preroll and contributes nothing, and the frame's *output* is the block of
/// input it was analysed from, not the block its window started on.
fn self_verify(
    bytes: &[u8],
    n_frames: usize,
    samples: Option<&Samples>,
    pcm: &Pcm,
) -> Result<Verify, Error> {
    let album = Album::parse(bytes).map_err(Error::Decode)?;
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).map_err(Error::Decode)?;

    let mut out8l = [0i8; M];
    let mut out8r = [0i8; M];
    let mut out16l = [0i16; M];
    let mut out16r = [0i16; M];
    let mut max_cycles = 0u32;
    let mut sum_cycles = 0u64;
    let mut bits = 0u64;
    let mut xover = 0u32;
    let mut hfnz = 0u32;
    // Squared error and signal energy, accumulated in frame order (R5b: a sum that
    // feeds a reported number is reduced by index, never by completion order).
    let mut se = 0f64;
    let mut sx = 0f64;
    let n_samples = pcm.n_samples as usize;

    for f in 0..n_frames {
        let info = dec
            .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
            .map_err(Error::Decode)?;
        max_cycles = max_cycles.max(info.cycles);
        sum_cycles += info.cycles as u64;
        bits += info.bits as u64;
        xover += info.xover;
        hfnz += info.hfnz;

        if let Some(s) = samples {
            let base = f as i64 * M as i64 - M as i64;
            for i in 0..M {
                let p = base + i as i64;
                if p < 0 || p as usize >= n_samples {
                    continue;
                }
                let p = p as usize;
                for ch in 0..2 {
                    let x = f64::from(s.at(p, ch));
                    let y = f64::from(if ch == 0 { out16l[i] } else { out16r[i] });
                    let d = x - y;
                    se += d * d;
                    sx += x * x;
                }
            }
        }
    }

    let after = dec.decode_frame(&mut out8l, &mut out8r, None, None);
    if !matches!(after, Err(uli_lite::Error::EndOfStream)) {
        return Err(Error::Verify(format!(
            "expected EndOfStream after {n_frames} frames, got {after:?}"
        )));
    }

    // The reference's mean is the integer mean over frames (`csum / NF`), and an
    // album always has at least the preroll frame.
    let mean = (sum_cycles / n_frames as u64) as u32;
    // A silent input has no energy to compare against; the reference would print
    // `nan`, and `None` says the same thing without a float in the API.
    let snr_db = if samples.is_some() && sx > 0.0 && se > 0.0 {
        Some(10.0 * (sx / se).log10())
    } else if samples.is_some() {
        Some(f64::INFINITY)
    } else {
        None
    };
    Ok(Verify {
        max_cycles,
        mean_cycles: mean,
        bits,
        snr_db,
        xover,
        hfnz,
    })
}
