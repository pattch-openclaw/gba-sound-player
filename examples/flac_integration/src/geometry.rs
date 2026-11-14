//! The product player's geometry — the numbers that say what "play at
//! 16,384 Hz" means on this hardware, shared by the ROM
//! (`src/product_play_main.rs`) and the host model
//! (`tests/product_play_witness.rs`).
//!
//! Item 6b-3 (history.md: "rate-play through a real `GAFP` manifest at the
//! target rate"). Before this module, the target-rate geometry lived in
//! `examples/flac_spike/src/rate_play_main.rs` beside the throwaway harness that
//! used it, and the throwaway harness turned out to be load-bearing in
//! undocumented ways — which is why the plan asks for the geometry to move to the
//! product path instead of the product path growing a copy of the harness.
//!
//! ## Why a module and not the ROM file
//!
//! The same constants have to answer two questions, and they must not drift:
//!
//! | consumer | question |
//! |---|---|
//! | the ROM | did the producer keep up, on hardware, counted not estimated |
//! | the host model | is the geometry *sufficient* — arithmetic, before a run |
//!
//! The host model is a deterministic replay of this geometry against a measured
//! per-sample cost (`flac-docs/measurements.md`, 6b-2 pt 2 row of record), driven
//! by the real `RingFeed` code and the real frame sizes from the real manifest.
//! A model that disagreed with the ROM about `CHUNK_SAMPLES` would prove nothing,
//! so both read this file — the same "one implementation, two consumers" rule
//! `examples/flac_spike/src/playback.rs` states for the ring itself, and the
//! `#[path]` sharing pattern `crates/flac-lite-gba/tests/arm_from_outside.rs`
//! already uses across these directories.
//!
//! ## Where each number comes from
//!
//! Every constant here is transcribed from something that already exists, with
//! the source named at the constant. Nothing in this module is a guess: the
//! buffer size is `agb`'s, the clock is the GBA's, the frame budget is
//! `scripts/gafp_pack.py`'s, and there is exactly one sample rate in the product
//! path — the one this module names, which is also the rate the shipped asset
//! declares (see the rate contract below). The compile-time assertions at the
//! bottom are what keeps the transcriptions from silently rotting.

// ---------------------------------------------------------------------------
// The clock, and the frame it is divided into
// ---------------------------------------------------------------------------

/// The GBA timer clock: the 16.78 MHz system clock divided by 16 exactly, i.e.
/// 2^24 = 16,777,216 Hz. `agb` drives its mixer from this value — its
/// `sound/mixer/hw.rs` sets the timer reload as `16777216 / frequency` (0.25.0:
/// `hw.rs:100`) and does not name the constant anywhere — and
/// `scripts/gafp_pack.py::GBA_CYCLES_PER_SECOND`
/// budgets frames with the same exact value — deliberately *not* the rounded
/// 16,780,000 that `examples/flac_spike/src/stats.rs::GBA_CLOCK_HZ` has used
/// for every published share so far.
///
/// The two differ by 174 cycles per 2048-sample frame (0.0166%); item 7-1
/// standardises on one. This module uses the exact one for a reason that is not
/// preference: the clock the product ROM measures *is* 2^24 — timer0 is loaded
/// with `0 − 1024` ticks of it per sample — so a stage measured against this
/// number is measured against the thing that actually paced the audio.
pub const MIXER_TICK_HZ: u32 = 16_777_216;

/// The playback rate: the ruler's 16,384 Hz rung (`2^24 / 1024`), which is an
/// exact integer number of timer ticks per sample (`MIXER_TICK_HZ / 16_384 ==
/// 1024`). It is deliberately *not* one of `agb`'s mixer rates (10,512 / 18,157 /
/// 32,768): this ROM programs its own timer chain and instantiates no
/// `agb::sound::mixer::Mixer`, so the mixer's menu constrains nothing here. The
/// other two are not exact — 10,512 Hz needs a reload of 1596.006 and agb's
/// integer 1596 plays 10,512.04 Hz (+3.8 ppm, 0.007 cents), and 18,157 Hz needs
/// 924.009 against its 924 → 18,157.16 Hz (+8.8 ppm, 0.015 cents). Exactness is
/// not a matter of picking a nicer-looking clock. The only rates with an integer
/// cycles/sample are the ones that divide 2^24 — powers of two, i.e. 8,192 /
/// 16,384 / 32,768 / 65,536 — and the retarget chose the highest one that fits.
pub const PLAYBACK_RATE_HZ: u32 = 16_384;

/// Timer ticks (= cycles at divider /1) per sample at the playback rate: exactly
/// 1,024. This is the unit every cost in this project eventually reduces to — see
/// `MEASURED_DECODE` in `scripts/gafp_pack.py`, which is quoted per frame, and
/// `flac-docs/measurements.md`, which quotes the same cycle counts against the
/// 32,768 Hz denominator. At this rate the hardware spends 1,024 cycles on each
/// sample it plays, and the whole real-time question is whether the producer
/// spends less.
pub const TICKS_PER_SAMPLE: u32 = MIXER_TICK_HZ / PLAYBACK_RATE_HZ;

/// A frame's period in cycles: `blocksize × TICKS_PER_SAMPLE`, which is what
/// `scripts/gafp_pack.py::frame_budget_cycles` computes as
/// `2^24 × blocksize / playback_rate`. For the product's 2,048-sample blocks
/// that is 2^21 = 2,097,152 cycles. The rounded-clock reading of the same frame
/// is 2,097,500; the ratio of one frame to the other is what the 0.0166% note
/// above is about.
pub const fn frame_cycles(blocksize: u32) -> u32 {
    blocksize * TICKS_PER_SAMPLE
}

/// A period in cycles: the interval at which the sound hardware asks for the
/// next buffer. `CHUNK_SAMPLES × TICKS_PER_SAMPLE` = 573,440 = 34.18 ms.
pub const PERIOD_CYCLES: u32 = (CHUNK_SAMPLES as u32) * TICKS_PER_SAMPLE;

// ---------------------------------------------------------------------------
// The buffer the hardware consumes, and the ring that feeds it
// ---------------------------------------------------------------------------

/// Samples per hardware buffer. At 32,768 Hz this was a transcription of `agb`
/// 0.25.0's `Frequency::buffer_size` (`src/sound/mixer/mod.rs`, 176 / 304 / 560
/// for 10,512 / 18,157 / 32,768 Hz). At 16,384 Hz none of those menu entries
/// applies, so the value is now a free parameter rather than a transcription; it
/// is kept at 560 because the rest of the geometry and every measured run are
/// stated on it, and 7-2 owns re-deriving it for the new clock. This is exactly
/// what one period costs the consumer — the buffer the ROM hands over and re-arms
/// once per period — and the 32-byte FIFO refills in 4-word requests when it falls
/// to half, so a buffer must be a multiple of 4 words — 560 is.
///
/// It is worth stating that precisely, because the same fact is easy to describe
/// backwards: in sound-FIFO DMA mode the transfer count is *not* what sizes a
/// period, and `agb` programs no count at all (`hw.rs`'s `enable_dma*_for_sound`
/// write source, destination and control only). What sizes a period is one
/// `buffer_size` of samples plus the re-arm.
///
/// The producer never gets to pick a smaller buffer to make its own life easier:
/// the period is fixed by the hardware rate, and the DMA is re-armed once per
/// period. A smaller chunk would just mean more interrupts.
pub const CHUNK_SAMPLES: usize = 560;

/// Ring depth in chunks. 36 chunks = 20,160 samples = 1.23 s of lead per
/// channel (40,320 B of `.bss`, one channel's bytes per array; EWRAM, which the
/// sound DMAs read fine — the rate ROM proved that at 10,512 Hz for 12.7 kB).
///
/// What this depth is for, as the host model measures it rather than as this
/// comment used to assume — `tests/product_play_witness.rs` (run:
/// `make product-play-test`) prints the table this summary is taken from:
///
/// - **Sustained throughput does not need it.** At the measured product-play cost
///   with the standing hardware derate applied, the run survives down to 20
///   chunks, and even 20 is not a cost limit: it is where the geometry can no
///   longer prefill (`PREFILL_SAMPLES` + one frame's reserve = 11,008 samples).
///   So the prefill choice puts a floor under the depth choice before any cost is
///   considered, and the two knobs are one decision rather than two.
/// - **What depth buys against a *sustained* overage is nothing; against a
///   *transient* it is time.** A frame of audio is 3.657 periods of this clock
///   (`frame_cycles(2048) / PERIOD_CYCLES`, and the fraction is the point), so a
///   producer that keeps up forever can spend at most
///   `(PERIOD_CYCLES − feeder) × 3.657` cycles per frame — **186.5%** of the measured
///   cost — and that ceiling is a property of the clock, not of [`RING_CHUNKS`]: a
///   bounded buffer cannot pay a permanent deficit. What a deep ring can do is spend
///   its lead *once*, which on the shipped 9.766 s clip shows up as **4.1 points** of
///   extra overage above the ceiling (190.6% of the measured cost; 178.1% at the
///   derate), and shrinks as the clip lengthens — the same search on a clip twice as
///   long lands at 188.2% (`sustained_overage_tolerance` runs both). An earlier
///   version of this comment — and of that test — divided `frame_cycles` by
///   `PERIOD_CYCLES` as integers, quoted the truncated 3-period bound as 152.9%, and
///   so credited the ring with "~38 points" of sustained overage. It buys transient
///   tolerance, which is measured next door at 17 periods in hand.
/// - **What depth buys in the regime a decoder actually operates in is time.** A
///   transient costs the lead in hand, and at this rate the lead in hand *is* a
///   period count: stop the producer entirely and the ring is holding 17 periods'
///   worth of audio (the quantised prefill of 10,240 less the prime of 560), and
///   the 18th period is the first with nothing to take — arithmetic this file can
///   derive and the model confirms period for period (`burst_stall_tolerance`).
///   Depth buys more of that, up to a ceiling that is two numbers and not one: the
///   flow control will not **start** a frame unless a whole frame's worth of ring is
///   free, so the deepest lead at which a frame still gets started is
///   `(RING_SAMPLES − blocksize)/CHUNK_SAMPLES` ≈ 32 periods — but a frame that was
///   allowed to start pushes its 2,048 samples into that reserve, so the lead
///   *peaks* higher: the ROM's run peaked at 35 chunks (19,600 of the 20,160 it
///   holds), the model at the ring's full 36, and the gap between those two is the
///   DMA draining concurrently (`floor_matches_the_roms_watermark` asserts the peak
///   only to within one period, and says why). None of that changes the stall
///   answer: at the shipped prefill it is 17 periods in hand, starved at the 18th.
///
/// The floor that matters is one chunk (`CHUNK_SAMPLES`), because a period whose
/// lead is under one chunk is an underflow: the feeder takes real samples only
/// when a whole chunk is present (`playback::RingFeed`). That is why the audit
/// counts periods rather than bytes.
pub const RING_CHUNKS: usize = 36;

/// Ring capacity in samples per channel.
pub const RING_SAMPLES: usize = RING_CHUNKS * CHUNK_SAMPLES;

/// Chunks pushed before the feeder is armed and the first buffer primed. The
/// prefill is the lead the first frame is allowed to spend: while the producer is
/// inside its first decode, the consumer is already taking one chunk per period,
/// and the first frame is the most expensive one in the run (cold caches: the
/// decode tables, the .iwram kernel, the frame's own bytes coming out of ROM the
/// first time). 16 chunks = 8,960 samples = 547 ms of lead.
///
/// Two facts the model turned from assertion into measurement, and both say this
/// is the cheap knob and the ring depth the expensive one:
///
/// - **The minimum that survives the measured cost at the shipped depth is one
///   chunk**, with the lead floor then 368 samples — sixteen times more prefill
///   than the run needs (`sizing_scans_for_both_geometry_knobs`). What the extra
///   buys is cold-start and transient tolerance, paid for in boot time (five
///   frames of decode before any sound appears) rather than in the 40 kB the depth
///   costs. A pre-filled pipe is the cheapest protection on this path, so it is
///   set high.
/// - **It cannot land on its target, and the overshoot is derivable.** The
///   producer stops at the push that *crosses* the target, so the real depth is
///   `ceil(8,960 / 2,048) × 2,048 = 10,240`; subtract the prime chunk and one
///   period's drain and you get the 9,120-sample floor that both the product-play
///   ROM's log and the model report, independently and identically
///   (`floor_matches_the_roms_watermark`).
///
/// The coupling to `RING_CHUNKS`, stated once because it is a real constraint and
/// not a style choice: a ring shallower than `PREFILL_SAMPLES` + one frame's
/// reserve cannot finish this prefill, so this constant sets the floor under the
/// depth (20 chunks at the shipped value). The sizing scan reports shallower rows
/// as infeasible rather than as failures, which is the same fact seen from there.
pub const PREFILL_CHUNKS: usize = 16;

/// Samples in the prefill.
pub const PREFILL_SAMPLES: usize = PREFILL_CHUNKS * CHUNK_SAMPLES;

/// The sound DMA re-arm grace, in samples: the FIFO refill happens when the
/// 32-byte FIFO falls to half = 16 samples, and the DMA in repeat mode reloads
/// its *latched* source at that moment. A feeder that rearms inside this window
/// swaps the source before the reload; one that rearms later has already replayed
/// part of the old buffer (inserted samples) and splices mid-waveform (the pop).
/// Measured, not theorised: `examples/flac_spike/src/rate_play_main.rs` carries
/// the audit that counts it, and its first hardware reading of the field named
/// the same event Sam heard on 2026-10-19.
pub const REARM_GRACE_SAMPLES: u16 = 16;

/// One hardware buffer in 32-bit words — the count this ROM writes when it re-arms the
/// sound DMA each period. It is *not* what sizes a period: in sound-FIFO mode the FIFO
/// pulls 4 words per request, and `agb`'s own sound enable (`hw.rs:56-68`) writes no
/// count at all. The count is ours, and it is the shape the rate ROM already ran on
/// silicon with (`flac-docs/history.md`, the 10,512 Hz row).
pub const DMA_WORDS: u16 = (CHUNK_SAMPLES / 4) as u16;

// ---------------------------------------------------------------------------
// The rate contract: one clock, declared by the stream that plays on it
// ---------------------------------------------------------------------------

/// The rate reinterpretations this product path carries. **There are none.**
///
/// This is plan item 7-0 landed, and it is the whole of Q14's ruling in one
/// constant: a stream is encoded at the rate it is played at, so
/// `assets/spike_l0.gfp` declares 16,384 Hz because that is the rate
/// [`PLAYBACK_RATE_HZ`] names, and there is no mapping left for a player to
/// apply. It is a named `Option` rather than an absent concept for one reason:
/// [`playback_rate_for_stream`] below still *reads* it, so a future
/// re-introduction of a mapping changes the player's behaviour and the guard in
/// `tests/product_play_witness.rs` (`ruled_rate_policy_*`) goes red in the same
/// test run — the tripwire is the pair, not the constant.
///
/// What used to sit here was `Some((32_000, PLAYBACK_RATE_HZ))`: an asset encoded
/// at 32,000 Hz played on a 32,768 Hz timer. It bought a measured row
/// (`scripts/gafp_pack.py::MEASURED_DECODE` held only 32,000-Hz-keyed rows) and it
/// cost 32,768/32,000 = 1.024x — **2.4% faster, 41.06 cents sharp** — a defect no
/// sample-level pin in this repository could see, because the samples were
/// identical either way; the pitch and tempo are not in the digest. That is
/// recorded in `flac-docs/history.md` Q14 and in the cents arithmetic
/// `tests/product_play_witness.rs` still computes, and it is why the cost rows in
/// the packer are keyed to *structure* (block size, predictor kind/order, residual
/// envelope) rather than to a declared rate: the rate is a validity check, and the
/// cost of a frame never was a function of it.
///
/// **What the declaration looks like on the wire.** 16,384 has no entry in the
/// FLAC 4-bit sample-rate table either, so a stream either spells it as code
/// `0b0000` ("get it from the stream") plus a 16,384 stream default, or as code
/// `0b1101` plus an appended 16-bit Hz field. `flac` emits the second spelling,
/// and the retargeted asset uses it: `crates/flac-lite/tests/frame_header_vectors.txt`
/// pins `samplerate_code 0xD` with `extra_field_bytes 2` on every frame of it, and
/// `crates/flac-lite/tests/gafp_asset_vectors.txt` pins the same for the packed
/// blobs. The cost of that spelling is 2 header bytes per frame — 160 bytes across
/// the retargeted product blob's 80 frames. Representability and cost were never
/// the obstacle; the pipeline was.
pub const RATE_BRIDGE: Option<(u32, u32)> = None;

/// The playback rate for a declared stream rate: **identity, or refusal.**
///
/// Two rate sets, not one, and the distinction is why this function takes a
/// *stream* rate at all. `agb` 0.25 can *play* 10,512 / 18,157 / 32,768 Hz, and
/// this geometry is built for 16,384 — outside agb's menu; a FLAC **stream** may
/// declare any of the format's 4-bit table rates or a 16-bit Hz value. Before 7-0
/// this function bridged the two sets for one pair, and the guard test's refusal
/// list is still the interim form of the sweep over the declarable set (`0..=65535`
/// plus the table values). It deliberately samples the rates that look
/// *arithmetically* tempting: the only rates with an exact cycles/sample are those
/// dividing 2^24 — powers of two — so 8,192, 32,768 and 65,536 are exact and still
/// not product rates. Exactness of the division is a property of the clock, not a
/// ruling about the stream.
///
/// The `RATE_BRIDGE` arm is inert in this tree — [`RATE_BRIDGE`] is `None`, and
/// the compile-time pin at the bottom of this module says so — but it is kept on
/// purpose: it is what makes the constant load-bearing rather than decorative, so
/// re-adding a mapping there would change what this function returns and the
/// witness would notice. Deleting the arm would leave a constant nothing reads.
///
/// `const` so the contract below can be pinned at compile time: the identity case
/// is checkable there, and only the identity case — everything else this function
/// can return is a `RATE_BRIDGE` lookup, which the pin says is absent.
pub const fn playback_rate_for_stream(stream_rate_hz: u32) -> Option<u32> {
    if stream_rate_hz == PLAYBACK_RATE_HZ {
        // The identity case is the product case: the asset declares the clock.
        return Some(PLAYBACK_RATE_HZ);
    }
    match RATE_BRIDGE {
        Some((declared, played)) if stream_rate_hz == declared => Some(played),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Compile-time pins: every transcription above is checkable here
// ---------------------------------------------------------------------------

const _: () = assert!(MIXER_TICK_HZ == 1 << 24);
const _: () = assert!(PLAYBACK_RATE_HZ == 16_384);
const _: () = assert!(TICKS_PER_SAMPLE == 1_024);
const _: () = assert!(MIXER_TICK_HZ % PLAYBACK_RATE_HZ == 0);
const _: () = assert!(frame_cycles(2048) == 2_097_152);
const _: () = assert!(CHUNK_SAMPLES == 560);
const _: () = assert!(CHUNK_SAMPLES % 4 == 0);
const _: () = assert!(PERIOD_CYCLES == 573_440);
const _: () = assert!(DMA_WORDS == 140);
const _: () = assert!(RING_SAMPLES == 20_160);
const _: () = assert!(PREFILL_SAMPLES == 8_960);
// The ring has to be able to hold the prefill *and* the largest block the
// profile admits, and the flow-control rule needs both a producer side and a
// consumer side to be satisfiable at once: free >= 2048 and ready >= 560.
const _: () = assert!(RING_SAMPLES > PREFILL_SAMPLES + 2048);
const _: () = assert!(RING_SAMPLES >= 2048 + CHUNK_SAMPLES);
/// The period counter (timer2) is 16-bit and counts periods, so the composite
/// cycle clock this module's arithmetic prices against wraps at 65,536 periods =
/// 37.4 minutes of audio at this rate. The ROM reports the period count it used
/// and refuses to divide any share by a clock that wrapped; this pin says the
/// ceiling is far above any asset the profile admits. What a run arms is
/// `ceil(total_samples / CHUNK_SAMPLES)` periods, so the shipped 162,304-sample
/// clip — 16,384 Hz for 9.91 s, 80 frames with a 512-sample tail — arms 290.
/// The wrap is two orders of magnitude above that, and the asset cannot be
/// lengthened past ~36.7 million samples (37.4 minutes) without the counter
/// needing a wider field.
const _: () = assert!(u16::MAX as u64 * PERIOD_CYCLES as u64 > 1_600_000_000);

// The rate contract itself, as a compile-time fact: this product path carries no
// rate reinterpretation. Paired with the runtime guard in
// `tests/product_play_witness.rs`, which checks what the *player* does with a
// declared rate; a constant nothing reads would pass this and fail that.
const _: () = assert!(matches!(RATE_BRIDGE, None));
// …and identity is total for the product clock: the rate the shipped asset
// declares resolves to the rate this geometry's frame budget is stated in. The
// negative half of that — nothing else resolves at all — is what the runtime
// guard in `tests/product_play_witness.rs` sweeps, because the declarable rate
// set is 65,536 wide and a compile-time pin cannot enumerate it honestly.
const _: () = assert!(matches!(
    playback_rate_for_stream(PLAYBACK_RATE_HZ),
    Some(rate) if rate == PLAYBACK_RATE_HZ
));
