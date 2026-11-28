//! The Album Player's audio-chain geometry — **derived per backend**, never transcribed.
//!
//! ALBUM-PLAYER.md §4.2b: "the app derives and owns its own geometry per backend". A second
//! backend (ULI2 at 32,768 Hz) or a second rate is therefore the *same* code with different
//! declared facts, not a second module of numbers. Everything here is a `const fn` over
//! [`ChainGeometry::derive`], and the asserts at the bottom are what keep the derivation from
//! silently disagreeing with the hardware or with the shipped FLAC track's figures.
//!
//! ## The two kinds of input
//!
//! Only two of the inputs are hardware facts and they never vary by backend:
//!
//! | fact | why it is fixed |
//! |---|---|
//! | [`MIXER_TICK_HZ`] = 16,777,216 | the GBA timer clock: 16.78 MHz / 16 exactly, i.e. `1 << 24` |
//! | [`REARM_GRACE_SAMPLES`] = 16 | the sound FIFO refills at half of 32 bytes = 16 samples; a DMA re-arm inside that window swaps the source before the latched reload |
//!
//! The rest are *declared* per backend: its native rate, its DMA chunk, its decoder frame, and the
//! two product choices expressed as durations (§4.2b: "ring depth is a lead-time decision expressed
//! in seconds and then divided up"). Everything downstream — ticks per sample, the period in cycles,
//! the DMA word count, the ring and prefill depths — is arithmetic on those five.
//!
//! ## Why the FLAC arm's figures must equal `examples/flac_integration/src/geometry.rs`'s
//!
//! The shipped product path's constants are measured, not chosen: the ring depth carries the
//! transient-tolerance numbers in `tests/product_play_witness.rs`, and the prefill depth sits at
//! the floor those numbers imply. This app may not `#[path]` the shipped module (§2.2 forbids
//! includes from an example's sources, and §4.2b puts the geometry in the app's hands), so the
//! host gate (`tests/album_seam_host.rs`) pins every derived FLAC figure to the shipped value with
//! a line citation. A transcription rot is then a red test rather than a silent geometry change.
//!
//! The derivation is deliberately *not* a restatement of those figures: `RING_CHUNKS` is
//! `ceil(lead × rate / chunk)`, and the fact that it lands on 36 is a consequence of the declared
//! 1,230 ms lead and 560-sample chunk, not of a literal 36 sitting in this file.

/// The GBA timer clock: 16.78 MHz divided by 16 exactly, i.e. `2^24` Hz.
///
/// `agb` 0.25.0 drives its mixer from this value (`sound/mixer/hw.rs:100` sets the reload as
/// `16777216 / frequency`) without naming it, and `scripts/gafp_pack.py::GBA_CYCLES_PER_SECOND`
/// budgets frames with the same exact number. The shipped product geometry
/// (`examples/flac_integration/src/geometry.rs:59`) states it for the same reason: the clock the
/// ROM actually measures is `2^24`, so a cost measured against it is measured against the thing
/// that paced the audio.
pub const MIXER_TICK_HZ: u32 = 16_777_216;

/// Sound-DMA re-arm slack, in samples. **Hardware, rate-independent.**
///
/// The 32-byte FIFO requests a refill when it falls to half — 16 samples — and the DMA in repeat
/// mode reloads its latched source at that moment. A feeder that re-arms inside this window swaps
/// the source before the reload; one that re-arms later has already replayed part of the old buffer
/// (inserted samples) and splices mid-waveform. Measured, not theorised: the audit lives in
/// `examples/flac_spike/src/rate_play_main.rs`, and the shipped geometry states the same fact at
/// `examples/flac_integration/src/geometry.rs:219`. It is *not* a duration and does not scale with
/// the sample rate, which is why it lives beside [`MIXER_TICK_HZ`] rather than inside a backend arm.
pub const REARM_GRACE_SAMPLES: u32 = 16;

/// The FLAC arm's declared inputs, and the consequences of declaring them.
///
/// One value of this type per backend; [`FLAC`] is the shipped one. Holding the inputs and the
/// derived quantities together is what makes "derive from facts" checkable: a field can always be
/// re-derived from the fields above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainGeometry {
    /// Backend-native sample rate, Hz. Declared fact.
    pub rate_hz: u32,
    /// Samples per DMA buffer — one period of the consumer. Declared fact; must be a whole number
    /// of 4-sample FIFO words.
    pub chunk_samples: u32,
    /// Samples per decoder frame. Declared fact (the backend's own block size).
    pub frame_samples: u32,
    /// Ticks of [`MIXER_TICK_HZ`] per output sample: `MIXER_TICK_HZ / rate_hz`, exact.
    pub ticks_per_sample: u32,
    /// Cycles one period of audio costs the consumer: `chunk_samples × ticks_per_sample`.
    pub period_cycles: u32,
    /// 32-bit words per DMA transfer: `chunk_samples / 4`.
    pub dma_words: u32,
    /// Lead-buffer depth in chunks, from the declared lead duration.
    pub ring_chunks: u32,
    /// Lead-buffer depth in samples per channel.
    pub ring_samples: u32,
    /// Prefill depth in chunks, from the declared prefill duration.
    pub prefill_chunks: u32,
    /// Prefill depth in samples per channel.
    pub prefill_samples: u32,
}

impl ChainGeometry {
    /// Derive every quantity from the declared facts: `(rate_hz, chunk_samples, frame_samples,
    /// lead_ms, prefill_ms)`.
    ///
    /// Durations are milliseconds because a lead "expressed in seconds and then divided up"
    /// (§4.2b) needs sub-second resolution to name the shipped depths, and integer seconds would
    /// quantise 1.23 s to 1 s. The ceilings are the two knobs §4.2b calls one decision: prefill
    /// plus one frame must fit the ring, and the flow control will not start a frame unless a whole
    /// frame's worth of ring is free — which is why the asserts below are on *both*.
    ///
    /// `const` so a backend's arm is a compile-time value and the asserts are compile-time pins.
    /// Callers pass facts that keep `lead_ms * rate_hz` inside a `u32` and a rate that divides
    /// [`MIXER_TICK_HZ`] exactly (Q14's exactness rule); every in-tree arm does, and a duration that
    /// would not is a 4,000-second lead.
    pub const fn derive(
        rate_hz: u32,
        chunk_samples: u32,
        frame_samples: u32,
        lead_ms: u32,
        prefill_ms: u32,
    ) -> Self {
        let ticks_per_sample = MIXER_TICK_HZ / rate_hz;
        // ceil(duration × rate / samples-per-chunk), with the duration in ms: the depth is a
        // lead-time decision, and a partial chunk still has to be held in whole chunks.
        let ring_chunks = ceil_div(lead_ms * rate_hz, chunk_samples * 1000);
        let prefill_chunks = ceil_div(prefill_ms * rate_hz, chunk_samples * 1000);
        Self {
            rate_hz,
            chunk_samples,
            frame_samples,
            ticks_per_sample,
            period_cycles: chunk_samples * ticks_per_sample,
            dma_words: chunk_samples / 4,
            ring_chunks,
            ring_samples: ring_chunks * chunk_samples,
            prefill_chunks,
            prefill_samples: prefill_chunks * chunk_samples,
        }
    }

    /// Cycles for a frame of `n` samples: `n × ticks_per_sample`. Not the same as
    /// [`period_cycles`](Self::period_cycles) unless `n == chunk_samples` — a 2,048-sample frame is
    /// 3.657 periods at this clock, and that fraction is why the ring buys transient time rather
    /// than sustained throughput.
    pub const fn frame_cycles(&self, n: u32) -> u32 {
        n * self.ticks_per_sample
    }
}

/// `ceil(n / d)` for positive `d`; `n == 0` yields 0.
const fn ceil_div(n: u32, d: u32) -> u32 {
    if n == 0 { 0 } else { (n + d - 1) / d }
}

// -------------------------------------------------------------------------------------------------
// The two backend arms, and the one this build selected
// -------------------------------------------------------------------------------------------------
//
// W3.1: the geometry was FLAC-shaped throughout. It is now one derivation with two declared arms:
// `FLAC` keeps the shipped facts, `ULI2` declares the 32,768 Hz / 512-sample shape, and `ACTIVE` is
// selected at compile time. The default is still FLAC; `geometry-uli2` is a host-only selection for
// the W3.1 geometry gate, not a product backend (W3.3 adds `backend-uli2`).

/// FLAC playback rate. Declared fact: 16,384 Hz is `2^24 / 1024`, an exact integer number of timer
/// ticks per sample (see `examples/flac_integration/src/geometry.rs:72`).
pub const FLAC_RATE_HZ: u32 = 16_384;

/// Samples per FLAC hardware buffer. Declared fact: 560 is the shipped period (`examples/
/// flac_integration/src/geometry.rs:120`) and a whole number of 4-word FIFO requests.
pub const FLAC_CHUNK_SAMPLES: u32 = 560;

/// Samples per FLAC decoder frame: the packer's 2,048-sample block.
pub const FLAC_FRAME_SAMPLES: u32 = 2_048;

/// FLAC lead duration, in milliseconds — a **product** choice, not a hardware fact.
///
/// 1,230 ms is the shipped ring depth expressed as time: 36 chunks of 560 samples is 20,160
/// samples = 1.2305 s at 16,384 Hz. §4.2b's rule is that the depth is decided as a lead time and
/// then divided into chunks.
pub const FLAC_LEAD_MS: u32 = 1_230;

/// FLAC prefill duration, in milliseconds — a **product** choice.
///
/// 546 ms is the shipped prefill expressed as time: 16 chunks of 560 samples is 8,960 samples =
/// 0.546875 s at 16,384 Hz.
pub const FLAC_PREFILL_MS: u32 = 546;

/// The FLAC backend's geometry, derived from the declared facts above.
pub const FLAC: ChainGeometry = ChainGeometry::derive(
    FLAC_RATE_HZ,
    FLAC_CHUNK_SAMPLES,
    FLAC_FRAME_SAMPLES,
    FLAC_LEAD_MS,
    FLAC_PREFILL_MS,
);

/// ULI2 playback rate. Declared fact: 32,768 Hz is `2^24 / 512`, an exact integer number of timer
/// ticks per sample, and the codec's fixed product rate.
pub const ULI2_RATE_HZ: u32 = 32_768;

/// Samples per ULI2 hardware buffer. One ULI2 frame is 512 samples, and the product player uses a
/// 512-sample period (`examples/uli_player/src/main.rs`: `PERIOD = M`).
pub const ULI2_CHUNK_SAMPLES: u32 = 512;

/// Samples per ULI2 decoder frame. Fixed by the format: `M = 512`.
pub const ULI2_FRAME_SAMPLES: u32 = 512;

/// ULI2 lead duration, in milliseconds. The product player carries 16 chunks (`RING_SAMPLES = 16 × M`
/// in `examples/uli_player/src/main.rs`); 16 × 512 / 32,768 = 250 ms. This is a declared product
/// choice, not a copy of FLAC's 1,230 ms; W3.3/W3.4 re-measure the UI and skip margins before the
/// product default is switched.
pub const ULI2_LEAD_MS: u32 = 250;

/// ULI2 prefill duration, in milliseconds. The product player primes 8 frames
/// (`PREFILL_FRAMES = 8` in `examples/uli_player/src/main.rs`); 8 × 512 / 32,768 = 125 ms.
pub const ULI2_PREFILL_MS: u32 = 125;

/// The ULI2 backend's geometry, derived from the same `ChainGeometry` code as FLAC's.
pub const ULI2: ChainGeometry = ChainGeometry::derive(
    ULI2_RATE_HZ,
    ULI2_CHUNK_SAMPLES,
    ULI2_FRAME_SAMPLES,
    ULI2_LEAD_MS,
    ULI2_PREFILL_MS,
);

/// The geometry this build plays. **FLAC is the default**; `geometry-uli2` selects the ULI2 arm for
/// the W3.1 host gate only. W3.3 turns this into the real backend selection.
#[cfg(feature = "geometry-uli2")]
pub const ACTIVE: ChainGeometry = ULI2;
#[cfg(not(feature = "geometry-uli2"))]
pub const ACTIVE: ChainGeometry = FLAC;

/// Playback rate for this build.
pub const RATE_HZ: u32 = ACTIVE.rate_hz;
/// log2 of [`RATE_HZ`], the §6 arithmetic shift.
#[cfg(feature = "geometry-uli2")]
pub const RATE_LOG2: u32 = 15;
#[cfg(not(feature = "geometry-uli2"))]
pub const RATE_LOG2: u32 = 14;

/// Samples per hardware buffer for this build.
pub const CHUNK_SAMPLES: u32 = ACTIVE.chunk_samples;
/// Samples per decoder frame for this build.
pub const FRAME_SAMPLES: u32 = ACTIVE.frame_samples;
/// Lead duration for this build, in milliseconds.
#[cfg(feature = "geometry-uli2")]
pub const LEAD_MS: u32 = ULI2_LEAD_MS;
#[cfg(not(feature = "geometry-uli2"))]
pub const LEAD_MS: u32 = FLAC_LEAD_MS;
/// Prefill duration for this build, in milliseconds.
#[cfg(feature = "geometry-uli2")]
pub const PREFILL_MS: u32 = ULI2_PREFILL_MS;
#[cfg(not(feature = "geometry-uli2"))]
pub const PREFILL_MS: u32 = FLAC_PREFILL_MS;

/// Ticks per sample for this build: 1,024 at 16,384 Hz, 512 at 32,768 Hz.
pub const TICKS_PER_SAMPLE: u32 = ACTIVE.ticks_per_sample;
/// Cycles per period for this build.
pub const PERIOD_CYCLES: u32 = ACTIVE.period_cycles;
/// 32-bit words per DMA transfer for this build.
pub const DMA_WORDS: u32 = ACTIVE.dma_words;
/// Lead depth in chunks for this build.
pub const RING_CHUNKS: u32 = ACTIVE.ring_chunks;
/// Lead depth in samples per channel for this build.
pub const RING_SAMPLES: u32 = ACTIVE.ring_samples;
/// Prefill depth in chunks for this build.
pub const PREFILL_CHUNKS: u32 = ACTIVE.prefill_chunks;
/// Prefill depth in samples per channel for this build.
pub const PREFILL_SAMPLES: u32 = ACTIVE.prefill_samples;
/// Producer frames needed to fill the prefill depth for this build.
pub const PREFILL_FRAMES: u32 = (PREFILL_SAMPLES + FRAME_SAMPLES - 1) / FRAME_SAMPLES;

/// Cycles for a frame of `n` samples at the selected rate.
pub const fn frame_cycles(n: u32) -> u32 {
    ACTIVE.frame_cycles(n)
}

// -------------------------------------------------------------------------------------------------
// The rate contract
// -------------------------------------------------------------------------------------------------

/// The playback rate for a declared stream rate: **identity, or refusal** (Q14).
///
/// A stream is encoded at the rate it is played at, so an asset that declares another rate is not
/// playable on this clock by reinterpretation — that mistake costs `rate/declared` in pitch and
/// tempo and is invisible to every sample-level pin, which is what Q14 was ruled on (see
/// `examples/flac_integration/src/geometry.rs:233-297` and plan item 7-0). The shipped product path
/// carries no bridge; this function is the same contract for this app, derived from [`RATE_HZ`]
/// rather than from a second constant.
///
/// `const` so the boot gate's identity case can be pinned at compile time; the negative half (that
/// nothing else resolves) is a host-suite sweep, because the declarable rate set cannot be
/// enumerated in a `const` block.
pub const fn playback_rate_for_stream(stream_rate_hz: u32) -> Option<u32> {
    if stream_rate_hz == RATE_HZ {
        Some(RATE_HZ)
    } else {
        None
    }
}

// -------------------------------------------------------------------------------------------------
// Compile-time pins: both arms are checkable here, not only in the host gate
// -------------------------------------------------------------------------------------------------

const _: () = assert!(MIXER_TICK_HZ == 1 << 24);

// FLAC keeps the shipped figures.
const _: () = assert!(FLAC.rate_hz == 16_384);
const _: () = assert!(FLAC.ticks_per_sample == 1_024);
const _: () = assert!(FLAC.chunk_samples % 4 == 0);
const _: () = assert!(FLAC.dma_words * 4 == FLAC.chunk_samples);
const _: () = assert!(FLAC.period_cycles == 573_440);
const _: () = assert!(FLAC.ring_chunks == 36);
const _: () = assert!(FLAC.ring_samples == 20_160);
const _: () = assert!(FLAC.prefill_chunks == 16);
const _: () = assert!(FLAC.prefill_samples == 8_960);
const _: () = assert!(FLAC.ring_samples > FLAC.prefill_samples + FLAC.frame_samples);
const _: () = assert!(FLAC.ring_samples >= FLAC.frame_samples + FLAC.chunk_samples);
const _: () = assert!((FLAC.prefill_samples + FLAC.frame_samples - 1) / FLAC.frame_samples == 5);

// The ULI2 arm is derived from the codec's fixed 32,768 Hz / 512-sample shape.
const _: () = assert!(ULI2.rate_hz == 32_768);
const _: () = assert!(ULI2.ticks_per_sample == 512);
const _: () = assert!(ULI2.chunk_samples % 4 == 0);
const _: () = assert!(ULI2.dma_words * 4 == ULI2.chunk_samples);
const _: () = assert!(ULI2.period_cycles == 262_144);
const _: () = assert!(ULI2.ring_chunks == 16);
const _: () = assert!(ULI2.ring_samples == 8_192);
const _: () = assert!(ULI2.prefill_chunks == 8);
const _: () = assert!(ULI2.prefill_samples == 4_096);
const _: () = assert!(ULI2.ring_samples > ULI2.prefill_samples + ULI2.frame_samples);
const _: () = assert!(ULI2.ring_samples >= ULI2.frame_samples + ULI2.chunk_samples);
const _: () = assert!((ULI2.prefill_samples + ULI2.frame_samples - 1) / ULI2.frame_samples == 8);

// The selected arm and the shared aliases are one fact.
const _: () = assert!(ACTIVE.rate_hz == RATE_HZ);
const _: () = assert!(RATE_HZ == 1 << RATE_LOG2);
const _: () = assert!(MIXER_TICK_HZ % RATE_HZ == 0);
const _: () = assert!(CHUNK_SAMPLES % 4 == 0);
const _: () = assert!(DMA_WORDS * 4 == CHUNK_SAMPLES);
const _: () = assert!(RING_SAMPLES > PREFILL_SAMPLES + FRAME_SAMPLES);
const _: () = assert!(RING_SAMPLES >= FRAME_SAMPLES + CHUNK_SAMPLES);
const _: () = assert!(PREFILL_FRAMES * FRAME_SAMPLES <= RING_SAMPLES);
const _: () = assert!(
    (PREFILL_SAMPLES + FRAME_SAMPLES - 1) / FRAME_SAMPLES == PREFILL_FRAMES
        && (PREFILL_SAMPLES - (CHUNK_SAMPLES - 1) + FRAME_SAMPLES - 1) / FRAME_SAMPLES
            == PREFILL_FRAMES
);

// The identity rate resolves to itself, and only to itself.
const _: () = assert!(matches!(
    playback_rate_for_stream(RATE_HZ),
    Some(rate) if rate == RATE_HZ
));
#[cfg(feature = "geometry-uli2")]
const _: () = assert!(matches!(playback_rate_for_stream(16_384), None));
#[cfg(not(feature = "geometry-uli2"))]
const _: () = assert!(matches!(playback_rate_for_stream(32_768), None));
