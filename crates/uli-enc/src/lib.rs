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
pub mod pcm;
pub mod plan;
pub mod tables_default;

use std::fmt;
use std::path::PathBuf;

use uli_lite::format::M;
use uli_lite::{Album, CodeTables, Decoder, DecoderState, PremiseViolation};

pub use container::FramePos;
pub use frame::{DC_G, FrameKind, frame_bits, write_frame};
pub use pcm::{Pcm, TrackInput};
pub use plan::{
    DEFAULT_BUDGET, DEFAULT_CAP_PCT, GPlan, GPlanViolation, GPlanViolationKind, Plan, Refusal,
    chunks, frame_count,
};
pub use tables_default::{Codes, DefaultTables};

/// Everything the CLI passes; the library reads no environment.
#[derive(Clone, Debug)]
pub struct Args {
    pub album: PathBuf,
    pub tracks: PathBuf,
    /// Whole-blob byte budget, never a cart size (E-R10).
    pub budget: u64,
    /// Percent of the 262,144-cycle frame period; S1 validates it and has no RD
    /// search to spend it on.
    pub cap_pct: f64,
    pub threads: usize,
}

/// What the encoder measured while producing the blob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Report {
    pub overhead_bytes: usize,
    pub target_bits: i64,
    pub bits_per_frame: f64,
    pub n_frames: usize,
    /// Workers that actually coded a chunk; proves `--threads N` was exercised.
    pub workers_used: usize,
    pub est_max_cycles: u32,
    pub est_mean_cycles: u32,
}

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
pub fn encode_with_tables(args: &Args, tables: &DefaultTables) -> Result<Output, Error> {
    let pcm = Pcm::open(&args.album, &args.tracks)?;

    // S1's content, fixed before anything else is decided: silence at g = 0 in
    // every frame, never M/S. It is a real plan rather than a constant in the emit
    // loop because the seek table, the track entries and the byte budget are all
    // derived from it, and because E2 fills it from the RD search without changing
    // anything below this line. `Plan::new` is what refuses a `g` the reader
    // would reject.
    let g_plan = GPlan::silence(frame_count(pcm.n_samples));

    // The premise is a property of the *final* tables, so serialise and re-parse
    // them first: a bound checked against the in-memory model would not prove
    // what the blob says, and this also catches a wrong blob layout before any
    // frame exists.
    let blob = tables.to_blob();
    let mut parsed = CodeTables::new();
    parsed.parse_into(&blob).map_err(Error::Tables)?;
    parsed.premise().map_err(Error::Premise)?;

    let plan = Plan::new(&pcm, tables, &g_plan, args.budget, args.cap_pct)?;

    let ranges = chunks(plan.n_frames, args.threads.max(1));

    let coded: Vec<(bitwriter::BitWriter, Vec<u64>)> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for range in &ranges {
            let tables = tables;
            let g_plan = &g_plan;
            handles.push(scope.spawn(move || {
                let mut bw = bitwriter::BitWriter::new();
                let mut lens = Vec::with_capacity(range.len());
                // Entering state for this chunk's first frame: the previous
                // frame's g, which is 0 before frame 0.
                let mut g_prev = g_plan.g_prev(range.start);
                for f in range.clone() {
                    debug_assert_eq!(
                        g_prev,
                        g_plan.g_prev(f),
                        "the running g state disagrees with the plan at frame {f}: a seek \
                         into this chunk would then land on a different frame than the \
                         linear decode walks"
                    );
                    let start = bw.pos();
                    frame::write_frame(
                        &mut bw,
                        tables,
                        &mut g_prev,
                        g_plan.g(f),
                        g_plan.ms(f),
                        FrameKind::Silence,
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
                g_prev: g_plan.g_prev(f),
            });
            stream.append_bits(bw.halfwords(), off, len);
            off += len;
        }
    }
    let stream_bytes = stream.finish();

    let mut bytes = container::emit(&plan, tables, &fpos, &stream_bytes);

    let (est_max, est_mean) = self_verify(&bytes, plan.n_frames)?;
    container::patch_cycles(&mut bytes, est_max, est_mean);

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
            est_max_cycles: est_max,
            est_mean_cycles: est_mean,
        },
    })
}

/// Decode the finished stream with the portable decoder and return the cycle
/// model's `(max, mean)`. This is a *validity* pass, not a content check: S1's
/// content is silence, and the round-trip tests assert the samples.
///
/// It decodes every frame, so its cost is the album's decode cost — acceptable
/// for S1's test-sized inputs, and the place to revisit when the bake starts
/// feeding 43-minute albums through it.
fn self_verify(bytes: &[u8], n_frames: usize) -> Result<(u32, u32), Error> {
    let album = Album::parse(bytes).map_err(Error::Decode)?;
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).map_err(Error::Decode)?;

    let mut out8l = [0i8; M];
    let mut out8r = [0i8; M];
    let mut out16l = [0i16; M];
    let mut out16r = [0i16; M];
    let mut max_cycles = 0u32;
    let mut sum_cycles = 0u64;
    for _ in 0..n_frames {
        let info = dec
            .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
            .map_err(Error::Decode)?;
        max_cycles = max_cycles.max(info.cycles);
        sum_cycles += info.cycles as u64;
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
    Ok((max_cycles, mean))
}
