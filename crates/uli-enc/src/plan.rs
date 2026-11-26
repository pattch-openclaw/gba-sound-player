//! The album plan: every size decision, taken before a frame is written.
//!
//! This is decision 3 of the 2026-11-19 ruling (`PLAN-OF-RECORD.md` → "The
//! encoder"): the container's fixed overhead is computed exactly, the per-frame
//! bit target is derived from `--budget`, the whole album is checked against the
//! budget, and a refusal is a returned value — never a warning, never a smaller
//! album than the caller asked for.

use std::fmt;
use std::ops::Range;

use crate::tables_default::DefaultTables;
use crate::{Error, Pcm};

use uli_lite::format::{
    ALBUM_HEADER_BYTES, M, NSTEP, SEEK_ENTRY_BYTES, SEEK_INTERVAL, TRACK_ENTRY_BYTES,
};

/// The default `--budget`: 32 MiB − 256 KiB. An audio producer's own envelope, not a
/// cartridge's — the cart's total is the bake's business, and art and reaction data are
/// carved out of *it*, never out of the audio's share (E-R6).
pub const DEFAULT_BUDGET: u64 = 33_292_288;

/// The default `--cap`, as a percentage of the 262,144-cycle frame period. Leaves a
/// quarter of each frame for everything the decoder is not: interrupts, the mixer, the UI.
pub const DEFAULT_CAP_PCT: f64 = 74.0;

/// Slack added to `overhead_bytes` for the **target**: the alignment and padding that
/// squaring each region up to four bytes contributes. The reference's retry/budget `size`
/// does **not** include it (`uli_enc.c:398`), so `needed` subtracts it (S8-7, inventory
/// row 13); `target_bits` uses the padded number.
const OVERHEAD_SLACK: usize = 16;
/// Bits held back from the stream target, so a plan that lands exactly on its arithmetic is
/// still inside the budget rather than on the boundary.
const TARGET_RESERVE_BITS: i64 = 64;

/// What an M/S frame pays out of `g` before the step lookup: the reader computes
/// `geff = g - 4 * ms` (`uli-lite/src/frame.rs`, `decode_frame`). This is the
/// format's number, not a tunable — `write_frame`'s `ms` bit and this offset are
/// the same field seen from either side.
const MS_G_OFFSET: i32 = 4;

/// Frames in an album of `n_samples` interleaved stereo frames: `ceil(NS/M) + 1`, the
/// `+ 1` being the frame-0 preroll, which carries the overlap into the first real frame.
///
/// Free because the content plan needs it before the `Plan` that owns the number
/// exists — `Plan::new` takes the content as an argument (decision 1), so the frame count
/// cannot be something only `Plan` knows how to compute.
pub fn frame_count(n_samples: u32) -> usize {
    (n_samples as usize).div_ceil(M) + 1
}

/// Which of a frame plan's rules a frame broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GPlanViolationKind {
    /// `geff = g - 4*ms` falls outside the reader's `0..NSTEP`.
    StepIndex,
}

/// A frame whose quantiser step index the decoder would refuse.
///
/// `Display` states the whole rule, because this is the error a plan author reads
/// at 1 a.m. with an RD search that disagrees with them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GPlanViolation {
    pub frame: usize,
    pub g: i32,
    pub ms: bool,
    pub geff: i32,
    pub kind: GPlanViolationKind,
}

impl fmt::Display for GPlanViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            GPlanViolationKind::StepIndex => write!(
                f,
                "frame {} wants g={} ms={}, a step index of {}, outside the reader's \
                 0..{NSTEP}: it decodes as `BadFrame`, so the album would be written, \
                 pass every encoder-side check, and then refuse to play. The bound is \
                 the decoder's own (`uli_lite::format::NSTEP`, applied in `decode_frame`). \
                 The search's own `GMIN..GMAX` window is narrower, and is a policy choice \
                 about where good rates live, not a limit of the format.",
                self.frame, self.g, self.ms, self.geff
            ),
        }
    }
}

impl std::error::Error for GPlanViolation {}

/// The per-frame content plan: `g` and the M/S flag for every frame.
///
/// This is where the step-index bound lives, and where S1 refuses. `g` reaches three
/// places — the frame header's Exp-Golomb delta, the container's `u16` `g_prev` in the seek table and the
/// track entries, and the reader's `STEP[geff]` lookup — and before this type
/// existed, nothing bounded it on the way out. `frame::write_frame` would encode
/// any `i32`, the container would truncate it with `as u16`, and the failure
/// surfaced on hardware as `BadFrame` after a whole album had been written and
/// every encoder-side check had passed.
///
/// It also carries the per-frame bit target into the budget arithmetic: `Plan::new`
/// walks this plan to size the stream, so a coarse `g` that costs more bits is
/// refused by the *budget* rather than discovered after emission. That is what
/// makes the plan an argument to `Plan::new` and not a detail of the emit loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GPlan {
    g: Vec<i32>,
    ms: Vec<bool>,
}

impl GPlan {
    /// S1's content: silence at `g = 0` in every frame, never M/S. Legal by
    /// inspection (`geff = 0`, and `0 < NSTEP`), so it skips the check that
    /// [`Self::new`] defers to [`Self::validate`].
    pub fn silence(n_frames: usize) -> Self {
        Self {
            g: vec![0; n_frames],
            ms: vec![false; n_frames],
        }
    }

    /// A plan from the two per-frame vectors. Does **not** validate: the refusal
    /// belongs to `Plan::new`, where the content and the budget meet, and a
    /// constructor that could not produce an illegal plan would leave the rule
    /// itself unwitnessable. Call [`Self::validate`] (which `Plan::new` does).
    ///
    /// The length mismatch is a logic bug in the caller rather than data, so it is
    /// an assert; S4's panic-to-error work covers turning this module's remaining panics into errors.
    pub fn new(g: Vec<i32>, ms: Vec<bool>) -> Self {
        assert_eq!(g.len(), ms.len(), "a frame plan needs one ms bit per g");
        Self { g, ms }
    }

    pub fn len(&self) -> usize {
        self.g.len()
    }

    pub fn is_empty(&self) -> bool {
        self.g.is_empty()
    }

    /// The reader's step index for one frame.
    pub fn step_index(g: i32, ms: bool) -> i32 {
        g - i32::from(ms) * MS_G_OFFSET
    }

    /// Whether one `(g, ms)` pair is inside the window the reader accepts. Also
    /// implies the container can store it: `ms` is a single bit, so a legal frame
    /// has `0 ≤ g < NSTEP + 4`, which is non-negative and fits `u16` — the reason
    /// there is no separate representability check here.
    pub fn legal(g: i32, ms: bool) -> bool {
        (0..NSTEP as i32).contains(&Self::step_index(g, ms))
    }

    /// The first frame the reader would refuse.
    pub fn validate(&self) -> Result<(), GPlanViolation> {
        for (frame, (&g, &ms)) in self.g.iter().zip(&self.ms).enumerate() {
            let geff = Self::step_index(g, ms);
            if !Self::legal(g, ms) {
                return Err(GPlanViolation {
                    frame,
                    g,
                    ms,
                    geff,
                    kind: GPlanViolationKind::StepIndex,
                });
            }
        }
        Ok(())
    }

    /// The frame's own `g`.
    pub fn g(&self, frame: usize) -> i32 {
        self.g[frame]
    }

    /// The frame's M/S flag, in the `u32` the frame writer takes.
    pub fn ms(&self, frame: usize) -> u32 {
        u32::from(self.ms[frame])
    }

    /// The `g` state *entering* `frame`, which is the previous frame's `g` and 0
    /// before frame 0. This is the one definition; the frame writer's running
    /// state, the seek table and the track entries all read it from here, because
    /// a seek that lands on a different state than the linear decode walked is a
    /// blob that plays wrong from the second track on.
    pub fn g_prev(&self, frame: usize) -> i32 {
        if frame == 0 { 0 } else { self.g[frame - 1] }
    }
}

/// One track entry, with the string offsets already assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanTrack {
    pub start_sample: u32,
    pub n_samples: u32,
    pub preroll_frame: u32,
    pub skip: u16,
    pub title_off: u16,
    pub artist_off: u16,
}

/// Everything the container writer needs that does not depend on frame content.
#[derive(Clone, Debug)]
pub struct Plan {
    pub n_samples: u32,
    pub n_frames: usize,
    pub tracks: Vec<PlanTrack>,
    /// The strings region, NUL-terminated and padded to 4 bytes.
    pub strings: Vec<u8>,
    pub tables_bytes: usize,
    pub seek_count: usize,
    pub overhead_bytes: usize,
    /// Total bits the stream may use, from `--budget`.
    pub target_bits: i64,
    pub bits_per_frame: f64,
}

/// Why an album cannot fit: both numbers, as E-R6 requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Bytes the plan needs: overhead plus the encoded stream as planned.
    pub needed: u64,
    /// Bytes `--budget` allows.
    pub allowed: u64,
    pub overhead_bytes: u64,
    pub target_bits: i64,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the album needs {} bytes but --budget allows {} bytes (container overhead {} \
             bytes, stream target {} bits)",
            self.needed, self.allowed, self.overhead_bytes, self.target_bits
        )
    }
}

impl std::error::Error for Refusal {}

impl Plan {
    /// Plan the whole album, or refuse it. Runs before any frame is encoded.
    ///
    /// `content` is the per-frame `g`/M-S plan, and it is an argument rather than
    /// something `encode` decides later, for two reasons that turn out to be one:
    /// describes have to be inside the window the reader accepts, and they are what
    /// determines the stream's size — so a plan that cannot fit the budget, or that
    /// cannot decode, is refused here, in one place, before a byte exists.
    pub fn new(
        pcm: &Pcm,
        tables: &DefaultTables,
        content: &Content,
        budget: u64,
        cap_pct: f64,
    ) -> Result<Self, Error> {
        // S2 has no RD search, so the cap has no allocation to constrain yet; it
        // is still an input and a nonsense value is a usage error, not silence.
        if !(cap_pct > 0.0 && cap_pct <= 100.0) {
            return Err(Error::Plan(format!(
                "--cap {cap_pct} is outside (0, 100]: it is a percentage of the \
                 262,144-cycle period"
            )));
        }

        let n_samples = pcm.n_samples as usize;
        let n_frames = frame_count(pcm.n_samples);
        let seek_count = n_frames.div_ceil(SEEK_INTERVAL as usize);

        let mut strings = Vec::new();
        let mut tracks = Vec::with_capacity(pcm.tracks.len());
        for t in &pcm.tracks {
            let title_off = u16::try_from(strings.len()).map_err(|_| {
                Error::Plan("the strings region exceeds the format's 16-bit offsets".into())
            })?;
            strings.extend_from_slice(t.title.as_bytes());
            strings.push(0);
            let artist_off = u16::try_from(strings.len()).map_err(|_| {
                Error::Plan("the strings region exceeds the format's 16-bit offsets".into())
            })?;
            strings.extend_from_slice(t.artist.as_bytes());
            strings.push(0);

            let preroll_frame = t.start_sample / M as u32;
            tracks.push(PlanTrack {
                start_sample: t.start_sample,
                n_samples: t.n_samples,
                preroll_frame,
                skip: (t.start_sample % M as u32) as u16,
                title_off,
                artist_off,
            });
        }
        // The parser requires the region's last byte to be NUL and the stream offset to be
        // even; squaring up to 4 satisfies both, and keeps every later region 4-aligned.
        while strings.len() & 3 != 0 {
            strings.push(0);
        }
        let strings_bytes = strings.len();
        if strings_bytes > u16::MAX as usize + 1 {
            return Err(Error::Plan(format!(
                "the strings region is {strings_bytes} bytes; the format's offsets are u16"
            )));
        }

        let tables_bytes = tables.tables_bytes();
        let overhead_bytes = ALBUM_HEADER_BYTES
            + tracks.len() * TRACK_ENTRY_BYTES
            + tables_bytes
            + seek_count * SEEK_ENTRY_BYTES
            + strings_bytes
            + OVERHEAD_SLACK;

        let target_bits =
            (budget as i128 - overhead_bytes as i128) * 8 - TARGET_RESERVE_BITS as i128;
        let target_bits = target_bits.clamp(i64::MIN as i128, i64::MAX as i128) as i64;

        // The content plan has to describe exactly the frames the album holds, and
        // every one of them has to be inside the reader's step window: a `g` the
        // decoder rejects is otherwise discovered on hardware, after a whole album
        // has been written and every size check has passed (the step-index bound).
        if content.len() != n_frames {
            return Err(Error::Plan(format!(
                "the frame plan covers {} frames but the album is {n_frames} frames: \
                 NS/{n_samples} samples at a hop of {M} is {}, plus the frame-0 preroll",
                content.len(),
                n_frames - 1
            )));
        }
        content.validate().map_err(Error::GPlan)?;

        // The size is taken from the content one
        // frame at a time rather than assumed uniform: `g` is exactly what makes
        // frames differ in size, and S3 fills this from the RD search.
        let mut stream_bits = 0u64;
        for f in 0..n_frames {
            stream_bits += content.frame_bits(tables, f);
        }
        let stream_halfwords = crate::bitwriter::padded_halfwords(stream_bits);
        // The reference's `size` (the number its retry and its budget check compare) adds
        // the actual regions and `hw` only — it does **not** include the 16-byte target
        // slack that `overhead_bytes` reserves. Keeping the two numbers distinct is what
        // makes the retry's `size − budget` the reference's: on S8-4's `fast` cell the
        // difference was 16 bytes, which moved the second allocation target by 130.56 bits
        // and five frames' steps (S8-7, inventory row 13).
        let size_overhead = overhead_bytes - OVERHEAD_SLACK;
        let needed = size_overhead as u64 + (stream_halfwords * 2) as u64;
        if needed > budget {
            return Err(Error::Refusal(Refusal {
                needed,
                allowed: budget,
                overhead_bytes: size_overhead as u64,
                target_bits,
            }));
        }

        let bits_per_frame = target_bits as f64 / n_frames as f64;
        Ok(Self {
            n_samples: pcm.n_samples,
            n_frames,
            tracks,
            strings,
            tables_bytes,
            seek_count,
            overhead_bytes,
            target_bits,
            bits_per_frame,
        })
    }
}

/// Contiguous frame-index ranges, one per worker. Threads **compute, never
/// order** (R5a): the caller assembles in index order, so any partition gives
/// the same bytes. For a non-empty frame range (`nf >= 1`, which [`Plan`]
/// guarantees) there is always at least one chunk and never an empty one, even
/// when `threads > nf`.
pub fn chunks(nf: usize, threads: usize) -> Vec<Range<usize>> {
    let n = threads.max(1).min(nf.max(1));
    let base = nf / n;
    let rem = nf % n;
    let mut out = Vec::with_capacity(n);
    let mut start = 0;
    for i in 0..n {
        let len = base + usize::from(i < rem);
        out.push(start..start + len);
        start += len;
    }
    out
}

/// What every frame will contain: the step plan, and from S2 the coded bands.
///
/// `GPlan` alone was enough for S1, whose frames were all silence. Once real
/// content exists the two cannot be decided apart: `g` fixes the step, the step
/// fixes the magnitudes, and the magnitudes fix how many bits each frame costs —
/// and E-R6 needs the frame sizes before a byte is written. So this is what
/// `Plan::new` takes, and `GPlan` is the half of it that S1 could check.
///
/// A `Content` with `coded == None` is S1's album: every frame silence, sized as
/// such. That is not a placeholder to be ashamed of — it is what keeps the
/// budget arithmetic honest while there is nothing to count, and
/// `tests/gplan.rs` depends on being able to build one.
#[derive(Clone, Debug)]
pub struct Content {
    plan: GPlan,
    coded: Option<Vec<[crate::quant::Chan; 2]>>,
}

impl Content {
    /// The album of silence S1 shipped, and the shape `Plan` still accepts.
    pub fn silence(n_frames: usize) -> Self {
        Self {
            plan: GPlan::silence(n_frames),
            coded: None,
        }
    }

    /// A finished plan: step states plus the coded bands that go with them.
    pub fn coded(plan: GPlan, coded: Vec<[crate::quant::Chan; 2]>) -> Self {
        debug_assert_eq!(plan.len(), coded.len(), "one content per planned frame");
        Self {
            plan,
            coded: Some(coded),
        }
    }

    /// A step plan with no coded content yet: for tests of the bound itself.
    pub fn from_plan(plan: GPlan) -> Self {
        Self { plan, coded: None }
    }

    /// Quantise the whole album at a fixed step. This is the S2 encoder's only
    /// pass over the samples, and it happens before the container is sized.
    ///
    /// Frames are independent once `g` is fixed (that is decision 1's payoff), so
    /// this is where S5's parallelism will go; it is sequential here because the
    /// result must be identical either way and R5a is easier to see in one loop.
    pub fn quantise(
        samples: &crate::pcm::Samples,
        n_samples: usize,
        tables: &DefaultTables,
        g: i32,
        ms: bool,
        kappa: f64,
        n_frames: usize,
    ) -> Result<Self, Error> {
        if !GPlan::legal(g, ms) {
            return Err(Error::GPlan(GPlanViolation {
                frame: 0,
                g,
                ms,
                geff: GPlan::step_index(g, ms),
                kind: GPlanViolationKind::StepIndex,
            }));
        }
        let mut frames = Vec::with_capacity(n_frames);
        let mut coef = [0f32; 2 * M];
        for f in 0..n_frames {
            crate::mdct::mdct_frame(samples, f, n_samples, &mut coef);
            let rot = crate::quant::rotate(&coef, ms);
            // The rate/distortion quantiser, not a nearest-level rule: `kappa` is the
            // encoder's own knob and a fixed step is only the *step*, so leaving `lam` at
            // zero here would make `--g` a different coder from `--rd`, and a fixed-step
            // comparison between the two would be measuring that instead of the step. The
            // cycle price is zero: a fixed step does not search, so there is no cap for it
            // to meet and no candidate to re-code.
            let l = crate::quant::quantise_chan_rd(&rot[..M], tables, g, ms, kappa, 0.0).chan;
            let r = crate::quant::quantise_chan_rd(&rot[M..], tables, g, ms, kappa, 0.0).chan;
            frames.push([l, r]);
        }
        Ok(Self {
            plan: GPlan::new(vec![g; n_frames], vec![ms; n_frames]),
            coded: Some(frames),
        })
    }

    pub fn len(&self) -> usize {
        self.plan.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plan.is_empty()
    }

    pub fn validate(&self) -> Result<(), GPlanViolation> {
        self.plan.validate()
    }

    pub fn g(&self, frame: usize) -> i32 {
        self.plan.g(frame)
    }

    pub fn ms(&self, frame: usize) -> u32 {
        self.plan.ms(frame)
    }

    pub fn g_prev(&self, frame: usize) -> i32 {
        self.plan.g_prev(frame)
    }

    /// What `write_frame` should emit for this frame.
    pub fn kind(&self, frame: usize) -> crate::frame::FrameKind<'_> {
        match &self.coded {
            None => crate::frame::FrameKind::Silence,
            Some(frames) => crate::frame::FrameKind::Coded(&frames[frame]),
        }
    }

    /// The exact bits this frame will occupy.
    pub fn frame_bits(&self, tables: &DefaultTables, frame: usize) -> u64 {
        crate::frame::frame_bits(
            tables,
            self.g_prev(frame),
            self.g(frame),
            self.ms(frame),
            self.kind(frame),
        )
    }

    /// Coded bands across the album, for the report. `None` while nothing is coded.
    pub fn coded_frames(&self) -> Option<&[[crate::quant::Chan; 2]]> {
        self.coded.as_deref()
    }

    /// How many frames are coded as M/S.
    pub fn ms_frames(&self) -> usize {
        (0..self.len()).filter(|&f| self.plan.ms(f) != 0).count()
    }

    /// Coefficients that actually carry a nonzero magnitude, both channels.
    pub fn nonzero_coeffs(&self) -> usize {
        self.coded
            .as_ref()
            .map(|frames| {
                frames
                    .iter()
                    .map(|chs| {
                        chs.iter()
                            .map(|c| {
                                c.bands
                                    .iter()
                                    .filter(|b| !b.is_zero_class())
                                    .map(|b| b.m.iter().filter(|&&m| m != 0).count())
                                    .sum::<usize>()
                            })
                            .sum::<usize>()
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
}
