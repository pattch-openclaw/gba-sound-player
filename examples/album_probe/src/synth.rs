//! A deterministic PCM source, so a probe can measure the ring, the feeder and
//! the skip without a decoder or an asset in the way.
//!
//! The probes are about plumbing — DMA re-arm, ring depth, silence gaps, where
//! the cycles go — and every one of those questions is easier to answer when the
//! samples are known: a square tone makes an inserted sample or a mid-waveform
//! splice visible in a dump, and the loud/quiet half-seconds make a fallback to
//! silence obvious by ear and by counter. Nothing here decodes anything, reads
//! anything, or allocates: it is arithmetic over a position counter, so the host
//! test and the ARM probe get the same frames from the same code.
//!
//! The eight-bit FIFO the hardware plays is `i16`-shaped after
//! `playback::convert_frame`, so every sample here stays inside the 16-bit
//! profile — an out-of-profile sample would be a `PushError`, not a louder note.

use crate::geometry;

/// Samples per second the source is written for: the product rate, so a probe's
/// numbers are in the same units as the shipped geometry's.
pub const SAMPLE_RATE_HZ: u32 = geometry::PLAYBACK_RATE_HZ;

/// log2 of the loud/quiet half-cycle, so the per-sample switch is a shift: the
/// producer path runs it once per sample and §6 keeps divides off that path.
/// `2^13` is half a second at the product rate.
const HALF_SECOND_SHIFT: u32 = 13;

/// Samples in the half-second loud/quiet half-cycle.
pub const HALF_SECOND_SAMPLES: u32 = 1 << HALF_SECOND_SHIFT;

/// Loud-section amplitude. A quarter of `i16`'s range: audible, and far from the
/// clipping end even after a fade or a mixer sum.
pub const AMPLITUDE_LOUD: i32 = 8_192;

/// Quiet-section amplitude: one eighth of loud, i.e. 24 dB down.
pub const AMPLITUDE_QUIET: i32 = AMPLITUDE_LOUD / 8;

/// How many synthetic tracks the model has.
pub const N_TRACKS: usize = 4;

/// Track lengths in samples, one per synthetic track. Chosen to bracket the
/// cases the ring and the clock have to survive rather than to be pretty:
///
/// - `65_520` = 117 chunks exactly (a track that ends on a period boundary);
/// - `90_000` ends mid-chunk (the `Chunk::Tail` path, and a bar cell that is not
///   on a grid line);
/// - `131_072` is a power of two — a length whose bar reciprocal is exact;
/// - `6_000` is shorter than a prefill, so a skip out of it is a skip out of a
///   track that never filled the ring.
pub const TRACK_SAMPLES: [u32; N_TRACKS] = [65_520, 90_000, 131_072, 6_000];

/// A power of two, so [`Synth::start_track`]'s wrap of an arbitrary index is a
/// mask — §6 keeps `/` and `%` off every path a measured window spans.
const _: () = assert!(N_TRACKS & (N_TRACKS - 1) == 0);

/// Base tone frequency; track `i` plays `BASE_HZ × (i + 1)`.
const BASE_HZ: u32 = 110;

/// log2 of the sweep arm's octave length (`2^15` = two seconds at 16,384 Hz).
const SWEEP_SHIFT: u32 = 15;

/// Phase accumulator: one half-cycle is `2^31`, a full cycle `2^32`.
const PHASE_ONE: u32 = 1 << 31;

/// Phase advance per Hz per sample: `2^32 / rate`, and the rate is a power of
/// two (§6's own reason for choosing it), so this is a shift. The assertion
/// below is what keeps the shift honest if the geometry ever moves.
const PHASE_STEP_SHIFT: u32 = 18;

const _: () = assert!(HALF_SECOND_SAMPLES * 2 == SAMPLE_RATE_HZ);
const _: () = assert!((1u64 << 32) / geometry::PLAYBACK_RATE_HZ as u64 == 1u64 << PHASE_STEP_SHIFT);

/// The synthetic player: one position counter and the phase it implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Synth {
    track: usize,
    pos_in_track: u32,
    phase: u32,
}

impl Synth {
    /// A source sitting at track 0, sample 0.
    pub const fn new() -> Self {
        Self {
            track: 0,
            pos_in_track: 0,
            phase: 0,
        }
    }

    /// Seek to the start of track `track` — the seam call a skip makes between
    /// dropping the queue and re-priming it. Out-of-range indices wrap, because
    /// the probe's skip sweep wants `i ± 1` without a branch on the ends; the
    /// length table is the model's, not a manifest's.
    ///
    /// [`Self::roll`]'s compare wraps `track + 1`, not an arbitrary index, so
    /// this is a mask: `N_TRACKS` is a power of two (asserted above), the seam
    /// runs inside a skip's measured latency window, and a bit-and is not a
    /// divide.
    pub fn start_track(&mut self, track: usize) {
        self.track = track & (N_TRACKS - 1);
        self.pos_in_track = 0;
        self.phase = 0;
    }

    /// Which synthetic track the source is on.
    pub const fn track(&self) -> usize {
        self.track
    }

    /// How many samples of it have been produced.
    pub const fn pos_in_track(&self) -> u32 {
        self.pos_in_track
    }

    /// The track's length, from [`TRACK_SAMPLES`].
    pub const fn track_len(&self) -> u32 {
        TRACK_SAMPLES[self.track]
    }

    /// True once the track's last sample has been produced — the state in which
    /// the next sample rolls to the next track (see [`Self::fill_frame`]).
    pub const fn finished(&self) -> bool {
        self.pos_in_track >= self.track_len()
    }

    /// Fill one frame into the caller's buffers — the decode-style API every
    /// consumer of the ring uses. Writes `min(left.len(), right.len())` frames of
    /// real audio and returns how many it wrote, which is always that many: **the
    /// source never runs out**.
    ///
    /// It cannot run out because a probe pushes far more than one track's length.
    /// The UI sweep alone consumes ~91,000 samples against track 0's 65,520, so a
    /// source that stopped at the track end would have the most expensive arms
    /// converting silence — exactly the defect round 2 removed (`saturating_sub`
    /// plus a zero-fill). At the end of a track the source **rolls to the next
    /// synthetic track** (`N_TRACKS` of them, so track 3 rolls to track 0): `pos`
    /// and `phase` restart at 0, which makes a rolled track byte-identical to a
    /// [`Self::start_track`] re-seek of the same index — one deterministic rule,
    /// and the one the skip's re-seek already uses.
    ///
    /// Rolling rather than wrapping inside the track keeps every length in
    /// [`TRACK_SAMPLES`] an observable boundary: `pos_in_track` reaches it exactly
    /// and then restarts, so `track()` names the seam a reader can check.
    pub fn fill_frame(&mut self, left: &mut [i32], right: &mut [i32]) -> usize {
        let n = left.len().min(right.len());
        for i in 0..n {
            let (l, r) = self.next_sample();
            left[i] = l;
            right[i] = r;
        }
        n
    }

    /// One stereo sample: the tone in both channels, amplitude square-switched
    /// every half second, frequency swept upward with position.
    fn next_sample(&mut self) -> (i32, i32) {
        if self.pos_in_track >= self.track_len() {
            self.roll();
        }
        let pos = self.pos_in_track;
        self.pos_in_track = pos + 1;

        // The sweep: one octave per SWEEP_SAMPLES_PER_OCTAVE, applied as a phase
        // step that grows linearly with position (a chirp, cheap and monotone).
        let octave_up = pos >> SWEEP_SHIFT;
        let hz = BASE_HZ * (self.track as u32 + 1) * (1 + octave_up);
        self.phase = self.phase.wrapping_add(hz << PHASE_STEP_SHIFT);

        let loud = (pos >> HALF_SECOND_SHIFT) & 1 == 0;
        let amp = if loud {
            AMPLITUDE_LOUD
        } else {
            AMPLITUDE_QUIET
        };
        let sample = if self.phase < PHASE_ONE { amp } else { -amp };
        (sample, sample)
    }

    /// The end-of-track seam: the next track, from its first sample.
    ///
    /// Not [`Self::start_track`], because that one wraps an arbitrary index with
    /// `%` and this runs on the producer's measured path; four tracks make the
    /// wrap a compare (§6's no-divide rule, `nodiv`'s host gate).
    fn roll(&mut self) {
        self.track = if self.track + 1 == N_TRACKS {
            0
        } else {
            self.track + 1
        };
        self.pos_in_track = 0;
        self.phase = 0;
    }
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}
