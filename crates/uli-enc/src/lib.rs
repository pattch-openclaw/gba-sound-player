#![forbid(unsafe_code)]
//! `uli-enc` — the host-side ULI2 encoder, steps S1 through S5.
//!
//! The contract boundary is PCM: an `album.f32` (float32 interleaved stereo at PCM16
//! scale, 32,768 Hz) and a `tracks.tsv` in; one ULA2 blob the `uli-lite` decoder accepts,
//! out (`PLAN-OF-RECORD.md` → "The encoder", E-R2). Samples rather than a waveform format,
//! because a decoder in the loop makes the boundary testable byte for byte.
//!
//! **The WAV front end is a convenience layer over that boundary, not a second one.** A
//! list of WAV sources is decoded, brought to 32,768 Hz stereo at PCM16 scale and joined
//! gaplessly (`wav`, `resample`, `assemble`), and `--emit-pcm` writes back the exact pair
//! the encoder read — so "a natively-32,768 Hz WAV reaches the same blob its PCM pair
//! does" is a checkable claim, and the resampled path is held to SNR because E-R2 says so.
//!
//! What S1 is: the plan-then-emit writer. It takes the album length from the
//! track table and requires `album.f32` to be exactly that long (E-R2), fixes the
//! tables and the container layout, takes the per-frame content plan (`g`, M/S) as
//! an argument to `Plan::new` and refuses one the reader cannot decode (the step-index
//! bound), checks the whole album against `--budget` (refusing before a frame is written), then
//! codes frame bodies in index order with the workers computing but never ordering
//! (R5a). Its frames carry **silence at `g = 0`** when no codec knob is given; the DC frame
//! exists as [`FrameKind::DcImpulse`] and is exercised by the round-trip test.
//!
//! `uli-lite` stays read-side (2026-11-19 later, decision 1), so the tables
//! blob geometry, the code-length computation and the canonical code assignment
//! all live here; the decoder's parser is the authority that catches a mistake.

pub mod assemble;
pub mod bitwriter;
pub mod container;
pub mod frame;
pub mod frozen;
pub mod mdct;
pub mod mdct_tables;
pub mod pcm;
pub mod plan;
pub mod quant;
pub mod rd;
pub mod resample;
pub mod tables_default;
pub mod train;
pub mod wav;

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

/// Where an album comes from.
///
/// Two shapes, one boundary. [`Input::Pcm`] **is** the boundary (E-R2): the pair the bake
/// and the acceptance gate both speak, and the only shape the byte-exactness gate is stated
/// over. [`Input::Wav`] is the convenience layer on top of it, and the variants
/// are separate rather than one struct with an empty field because a caller that
/// supplies both has not said which album it means — the CLI refuses that instead
/// of silently preferring one.
#[derive(Clone, Debug)]
pub enum Input {
    /// `album.f32` + `tracks.tsv`: float32 interleaved stereo at PCM16 scale, and
    /// the track table that sets the album length.
    Pcm { album: PathBuf, tracks: PathBuf },
    /// WAV sources in play order, joined gaplessly (S5). Titles and artists come
    /// from each file's `LIST/INFO` tags, falling back to its stem and `?`.
    Wav(Vec<PathBuf>),
}

impl Input {
    /// Whether this input is the PCM boundary itself. The distinction decides
    /// whether [`Output::boundary`] carries an album (a WAV front end produced one
    /// in memory) or `None` (the pair is already a file the caller owns).
    pub fn is_pcm_pair(&self) -> bool {
        matches!(self, Input::Pcm { .. })
    }
}

/// Everything the CLI passes; the library reads no environment.
#[derive(Clone, Debug)]
pub struct Args {
    pub input: Input,
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
    /// The search's Lagrangian distortion-to-bits scale (`--kappa`): the price of one bit
    /// in squared-error units, before `λ` scales it by the step.
    pub kappa: f64,
    /// Train the code tables on the album before coding it (S4). Two passes: code once with
    /// the built-in tables to measure the album's own statistics, rebuild the lengths and
    /// reconstruction offsets from those counts, then search again with the trained set.
    /// The plan that ships is the second one, measured as it will actually be coded.
    pub train: bool,
    /// Force M/S, or leave the choice to the encoder. `None` lets the search pick, per `g`,
    /// whichever of L/R and M/S is cheaper under `J` subject to the cycle cap — at a fixed
    /// step the two are two concrete codings of the same frame, so one of them dominates.
    /// `Some(true)`/`Some(false)` pin the domain, which is what a fixed-step comparison
    /// needs in order to hold it constant.
    pub ms: Option<bool>,
    /// Re-code an over-cap frame with the decoder-cycle price instead of leaving the
    /// candidate for the allocation to exclude (S6, `--musearch`). Only the search uses it;
    /// with `--g` there is no candidate set and no cap to meet.
    pub musearch: bool,
    pub threads: usize,
}

/// What the encoder measured while producing the blob.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Album length and track count, reported so the CLI can print the album line the
    /// acceptance diff compares, without re-reading the input.
    pub n_samples: u32,
    pub n_tracks: usize,
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
    /// Codec-only SNR of the decoded int16 output against the input, in dB,
    /// **pooled over the album** (one squared-error sum against one signal sum).
    /// `None` when the album was silence, where it has no meaning. The per-track
    /// column is [`Self::track_snr`]; the two differ on a heterogeneous album, and
    /// E-R7's 32.29/25.78 dB is stated against this pooled one.
    pub snr_db: Option<f64>,
    /// The same SNR at int8, pooled; the verify block's second column.
    pub snr8_db: Option<f64>,
    /// Per-track time-domain SNR, pcm16 and int8, in track order, for the verify block's
    /// per-track line. `None` per field when a track has
    /// no signal energy to compare against (the ratio is 0/0 there).
    pub track_snr: Vec<TrackSnr>,
    /// The minimum over the per-track time-domain SNRs, pcm16 and int8 — the verify
    /// block's worst-track column. Undefined when no track has a defined ratio. Not
    /// comparable with E-R7's worst track, which is codec-only.
    pub worst_pcm16_db: Option<f64>,
    pub worst_int8_db: Option<f64>,
    /// `|X| >= 2^28` coefficients the reader counted: the encoder's `mmax` should
    /// make this impossible, and it is the number that says whether it did.
    pub xover: u32,
    /// Nonzero coefficients at index >= `NCODED`: likewise a format rule, not a
    /// quality knob.
    pub hfnz: u32,
    /// Coefficients that coded a nonzero magnitude, both channels.
    pub nonzero_coeffs: usize,
    /// Whether the code tables were trained on this album (S4).
    pub trained: bool,
    /// Frames coded as M/S. The fastest way to see whether the flag is doing anything at
    /// all: on real stereo it should be most of them, and a search that silently never
    /// chose it would otherwise look merely mediocre.
    pub ms_frames: usize,
    /// Whether the cycle-aware re-code ran (S6). `false` on every path where there is no
    /// candidate search, whatever the flag said.
    pub musearch: bool,
    /// Frames whose winning candidate was re-coded under a positive `mu`, i.e. frames the
    /// cycle cap actually bound. Zero is the honest answer on a non-binding cap, and the
    /// CLI prints it so "musearch on" cannot be mistaken for "musearch did something".
    pub mu_frames: usize,
}

/// How many times the search may re-try the bit target before refusing.
///
/// Six, which is the reference's own count (`uli_enc.c:405`). It was ten: an invented
/// number, and one of S8's divergences (inventory row 11) rather than a safety margin —
/// every retry re-allocates on the same curves, so the sixth attempt and the tenth differ
/// only in how long a refusal takes to arrive.
const RETRY_ATTEMPTS: usize = 6;

/// One track's measured SNR, in the two output profiles the DMA stage writes (pcm16 and
/// int8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackSnr {
    pub pcm16: Option<f64>,
    pub int8: Option<f64>,
}

/// The finished blob and its report.
#[derive(Clone, Debug)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub report: Report,
    /// The PCM boundary this blob was encoded from, present only when the input was
    /// a set of WAVs (the front end assembled it in memory). `--emit-pcm` writes it
    /// out, so the dump is bit-for-bit what the encoder read rather than a second
    /// conversion of the same sources.
    pub boundary: Option<Boundary>,
}

/// `album.f32`'s contents plus the track table that describes them.
#[derive(Clone, Debug)]
pub struct Boundary {
    pub pcm: Pcm,
    pub samples: Samples,
}

#[derive(Debug)]
pub enum Error {
    Pcm(String),
    Tracks(String),
    Wav(String),
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
            Error::Pcm(m)
            | Error::Tracks(m)
            | Error::Wav(m)
            | Error::Plan(m)
            | Error::Verify(m) => f.write_str(m),
            Error::Tables(e) => write!(f, "tables blob does not parse: {e:?}"),
            Error::Premise(v) => write!(f, "tables violate the encoder premise (E-R4): {v}"),
            Error::GPlan(v) => write!(f, "the frame plan cannot decode: {v}"),
            Error::Refusal(r) => write!(f, "{r}"),
            Error::Decode(e) => write!(f, "uli-lite rejected the blob: {e:?}"),
        }
    }
}

impl std::error::Error for Error {}

/// Encode with the built-in default tables.
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
        input,
        budget,
        cap_pct,
        g,
        rd,
        kappa,
        train,
        ms,
        musearch,
        threads,
    } = input;
    // The parameter is `input`, not `args`, so there is no way to reach a field
    // through it and quietly bypass the bindings above. That is what the first
    // version of this guard missed: `args.rd` still resolved, so the exhaustive
    // pattern proved nothing.
    let (budget, cap_pct, g, rd, kappa, train, ms, musearch, threads) = (
        *budget, *cap_pct, *g, *rd, *kappa, *train, *ms, *musearch, *threads,
    );
    // S5's two input shapes meet here, and only here: below this point an album is
    // a `Pcm` and (when it has content) a `Samples`, whichever door it came in by.
    let from_wav = !input.is_pcm_pair();
    let (pcm, mut loaded) = match input {
        Input::Pcm { album, tracks } => (Pcm::open(album, tracks)?, None),
        Input::Wav(paths) => {
            let (pcm, samples) = assemble::assemble(paths)?;
            (pcm, Some(samples))
        }
    };
    let mut report_g: Option<Vec<i32>> = None;
    let mut report_lambda: Option<f64> = None;
    let mut report_mu_frames = 0usize;

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
    // A WAV source *is* real audio, so asking for one and no codec knob would write
    // S1's silence — the exact silent failure S3's ledger is about. The PCM path
    // keeps the silence arm because half the suite (containers, refusals) uses it
    // and must not depend on the codec.
    if from_wav && g.is_none() && !rd {
        return Err(Error::Plan(
            "a --wav source is real audio, but neither --rd nor --g was given, so the \
             album would be S1's silence: give --rd to search the budget or --g to pin \
             a step"
                .to_string(),
        ));
    }
    let samples = if loaded.is_some() {
        loaded.take()
    } else if g.is_some() || rd {
        let Input::Pcm { album, .. } = input else {
            unreachable!("a WAV input always assembles its samples above")
        };
        Some(Samples::open(album, pcm.n_samples as usize)?)
    } else {
        None
    };
    let mut tables_effective: Option<DefaultTables> = None;
    let mut trained_used = false;
    let content = if rd {
        // S3's chicken-and-egg: the search needs the bit target, the bit target
        // needs the container overhead. Overhead is a function of the track table,
        // the tables blob, the seek count and the strings — none of which frame
        // content can change — so the target is taken from a silence-sized plan and
        // the real plan is then checked against the real stream, as below.
        let sizing = Plan::new(&pcm, tables, &Content::silence(nf), budget, cap_pct)?;
        let s = samples.as_ref().expect("--rd loads the album above");
        let cap = rd::cap_cycles(cap_pct);
        // The per-frame weights the allocation trades under: a frame is normalised by its
        // track's mean energy, so a fixed price buys the same *relative* improvement
        // everywhere instead of spending the album on its loudest passage.
        let mut coef = [0f32; 2 * M];
        let energy: Vec<f64> = (0..nf)
            .map(|f| {
                mdct::mdct_frame(s, f, pcm.n_samples as usize, &mut coef);
                rd::frame_energy(&coef)
            })
            .collect();
        let w = rd::weights(&pcm, &energy);

        // One search over a given table set, so training can run it twice: once to measure
        // the album, once to code it with what the measurement produced. The measurement
        // pass never pays the cycle price: training reads only the plan's symbol counts and
        // its reconstruction errors, so re-coding there would fit the tables to codings that
        // never ship. The price belongs to the pass whose bands do.
        let run = |tbl: &DefaultTables,
                   musearch: bool|
         -> Result<(Content, Vec<i32>, f64, usize), Error> {
            let params = rd::SearchParams {
                tables: tbl,
                n_frames: nf,
                kappa,
                cap,
                musearch,
                force_ms: ms,
                threads,
            };
            // One curve set, measured once. The retry loop below re-allocates on these
            // same curves, as the reference does (inventory row 11): re-measuring would
            // move the curves underneath a target that is only being squeezed.
            let curves = Curves::compute(s, pcm.n_samples as usize, &params)?;
            let mut target = sizing.target_bits as f64;
            for attempt in 0..RETRY_ATTEMPTS {
                let (lambda, picks) = curves.allocate(&w, target, cap)?;
                let content = curves.to_content(&picks, s, pcm.n_samples as usize, tbl, kappa)?;
                match Plan::new(&pcm, tbl, &content, budget, cap_pct) {
                    Ok(_) => {
                        let mu_frames = picks
                            .iter()
                            .enumerate()
                            .filter(|&(f, &i)| curves.per_frame[f].mu[i] > 0.0)
                            .count();
                        return Ok((content, curves.chosen_g(&picks), lambda, mu_frames));
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
            Err(Error::Plan(
                "the bit target did not converge in the retry loop".into(),
            ))
        };

        let (first, _, _, _) = run(tables, false)?;
        let effective = if train {
            // Pass one measured the album with the built-in tables. What it saw is only an
            // approximation of what the trained tables will see — lengths change which
            // magnitudes are worth coding — so this pass exists to get close, not to be
            // right, and the second pass is the one that ships.
            let stats = train::collect(s, pcm.n_samples as usize, tables, &first, kappa);
            let trained = train::build(tables, &stats);
            train::validate(&trained).map_err(|e| {
                Error::Plan(format!(
                    "the trained tables are not a usable table set: {e}"
                ))
            })?;
            // The premise is a property of the bytes, so check the bytes — the same
            // discipline the untrained path applies for the same reason.
            let blob = trained.to_blob();
            let mut parsed = CodeTables::new();
            parsed.parse_into(&blob).map_err(Error::Tables)?;
            parsed.premise().map_err(Error::Premise)?;
            trained_used = true;
            trained
        } else {
            tables.clone()
        };
        let (content, gs, lambda, mu_frames) = run(&effective, musearch)?;
        tables_effective = Some(effective);
        report_g = Some(gs);
        report_lambda = Some(lambda);
        report_mu_frames = mu_frames;
        content
    } else if let Some(g) = g {
        Content::quantise(
            samples.as_ref().expect("--g loads the album above"),
            pcm.n_samples as usize,
            tables,
            g,
            // A fixed step has no search to choose the domain, so `--ms` is the only way
            // to ask for one; absent it, L/R, which is what `--g` has always meant.
            ms.unwrap_or(false),
            kappa,
            nf,
        )?
    } else {
        Content::silence(nf)
    };

    // Everything after this point must use the tables the album was actually coded with:
    // the plan's sizes, the frame bodies and the container's tables region. A trained set
    // that sized the plan and a default set that wrote the stream would produce a blob whose
    // declared sizes match neither.
    let tables: &DefaultTables = tables_effective.as_ref().unwrap_or(tables);
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
            n_samples: pcm.n_samples,
            n_tracks: pcm.tracks.len(),
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
            snr8_db: verify.snr8_db,
            track_snr: verify.track_snr.clone(),
            worst_pcm16_db: verify.worst_pcm16_db,
            worst_int8_db: verify.worst_int8_db,
            xover: verify.xover,
            hfnz: verify.hfnz,
            nonzero_coeffs: content.nonzero_coeffs(),
            trained: trained_used,
            ms_frames: content.ms_frames(),
            musearch: rd && musearch,
            mu_frames: report_mu_frames,
        },
        // Only a WAV front end has a boundary that is not already a file. Moving
        // the samples here is why `--emit-pcm` can promise the dump is the exact
        // buffer that was encoded, not a re-read of it.
        boundary: if from_wav {
            samples.map(|samples| Boundary { pcm, samples })
        } else {
            None
        },
    })
}

/// What a verification decode of the finished blob measured.
#[derive(Clone, Debug)]
pub struct Verify {
    pub max_cycles: u32,
    pub mean_cycles: u32,
    pub bits: u64,
    /// Codec-only SNR against the input, or `None` if there were no samples to
    /// compare against (S1's silence path never loads them).
    pub snr_db: Option<f64>,
    /// The int8 profile's pooled SNR, and the per-track columns the verify block also
    /// prints — both undefined when there is no input to compare against.
    pub snr8_db: Option<f64>,
    pub track_snr: Vec<TrackSnr>,
    /// The worst track's time-domain SNR in each profile: the minimum over the tracks
    /// that have a defined ratio, not a codec-only figure. E-R7's "worst track" is a
    /// different (codec proxy) measurement; this one is full-band time-domain like the
    /// mean beside it.
    pub worst_pcm16_db: Option<f64>,
    pub worst_int8_db: Option<f64>,
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
/// The sample alignment is `p = (f - 1) * M + i`: frame 0 is the preroll and contributes
/// nothing, and a frame's *output* is the block of input it was analysed from, not the
/// block its window started on — the window begins one hop earlier by construction.
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
    let mut se8 = 0f64;
    // The same three sums per track, for the verify block's per-track column.
    let n_tracks = pcm.tracks.len();
    let mut se_t = vec![0f64; n_tracks];
    let mut sx_t = vec![0f64; n_tracks];
    let mut se8_t = vec![0f64; n_tracks];
    let n_samples = pcm.n_samples as usize;
    let mut track = 0usize;

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
                // `p` ascends, so advance the track cursor instead of rescanning.
                while track + 1 < n_tracks && pcm.tracks[track + 1].start_sample as usize <= p {
                    track += 1;
                }
                for ch in 0..2 {
                    let x = f64::from(s.at(p, ch));
                    let y = f64::from(if ch == 0 { out16l[i] } else { out16r[i] });
                    // The int8 profile's scale is 256 PCM16 units per step; that is
                    // the same number the reader writes to the DMA, so this column reports
                    // the DMA's scale rather than a second convention of its own.
                    let y8 = 256.0 * f64::from(if ch == 0 { out8l[i] } else { out8r[i] });
                    let d = x - y;
                    let d8 = x - y8;
                    se += d * d;
                    sx += x * x;
                    se8 += d8 * d8;
                    se_t[track] += d * d;
                    sx_t[track] += x * x;
                    se8_t[track] += d8 * d8;
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

    // Every track must be reachable through its own preroll entry, not only as a
    // stretch of the linear decode: a track table is a claim about seeking, and S5's
    // gate is "every track decodes". A fresh decoder per track keeps this from
    // depending on where the previous seek left the overlap state.
    for t in 0..n_tracks {
        let mut st = DecoderState::new();
        let mut seeker = Decoder::new(album, &mut st).map_err(Error::Decode)?;
        seeker.seek_track(t).map_err(|e| {
            Error::Verify(format!(
                "track {} ({:?}) does not seek to its preroll frame: {e:?}",
                t + 1,
                pcm.tracks[t].title
            ))
        })?;
    }

    // The mean is the integer mean over frames, and an album always has at least the
    // preroll frame, so the division is safe.
    let mean = (sum_cycles / n_frames as u64) as u32;
    // A silent input has no energy to compare against: the ratio is 0/0, so `None` says
    // "undefined" instead of putting a NaN in the API.
    let snr_db = if samples.is_some() && sx > 0.0 && se > 0.0 {
        Some(10.0 * (sx / se).log10())
    } else if samples.is_some() {
        Some(f64::INFINITY)
    } else {
        None
    };
    let snr8_db = if samples.is_some() && sx > 0.0 && se8 > 0.0 {
        Some(10.0 * (sx / se8).log10())
    } else if samples.is_some() {
        Some(f64::INFINITY)
    } else {
        None
    };
    let track_snr: Vec<TrackSnr> = (0..n_tracks)
        .map(|t| TrackSnr {
            pcm16: ratio(sx_t[t], se_t[t]),
            int8: ratio(sx_t[t], se8_t[t]),
        })
        .collect();
    // The worst track is the min over the tracks that *have* a ratio: an inaudible
    // track is undefined, not infinitely bad, so it must not drag the minimum down.
    let worst = |f: fn(&TrackSnr) -> Option<f64>| track_snr.iter().filter_map(f).reduce(f64::min);
    let worst_pcm16_db = worst(|t| t.pcm16);
    let worst_int8_db = worst(|t| t.int8);
    Ok(Verify {
        max_cycles,
        mean_cycles: mean,
        bits,
        snr_db,
        snr8_db,
        track_snr,
        worst_pcm16_db,
        worst_int8_db,
        xover,
        hfnz,
    })
}

/// `10 log10(signal / error)`, or `None` when there is no signal or no error to
/// divide — an inaudible track has no meaningful ratio and `inf` is not a report.
fn ratio(sx: f64, se: f64) -> Option<f64> {
    (sx > 0.0 && se > 0.0).then(|| 10.0 * (sx / se).log10())
}
