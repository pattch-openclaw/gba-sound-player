//! The product player's geometry — the numbers that say what "play at
//! 32,768 Hz" means on this hardware, shared by the ROM
//! (`src/product_play_main.rs`) and the host model
//! (`tests/product_play_witness.rs`).
//!
//! Item 6b-3 (plan-of-record: "rate-play through a real `GAFP` manifest at the
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
//! `scripts/gafp_pack.py`'s, and the rate mapping is that packer's
//! `PLAYBACK_RATE_FOR_STREAM`. The compile-time assertions at the bottom are what
//! keeps the transcriptions from silently rotting.

// ---------------------------------------------------------------------------
// The clock, and the frame it is divided into
// ---------------------------------------------------------------------------

/// The GBA timer clock: the 16.78 MHz system clock divided by 16 exactly, i.e.
/// 2^24 = 16,777,216 Hz. `agb` drives its mixer from this (`MIXER_TICK_HZ` in
/// `agb::sound::mixer`), and `scripts/gafp_pack.py::GBA_CYCLES_PER_SECOND`
/// budgets frames with the same exact value — deliberately *not* the rounded
/// 16,780,000 that `examples/flac_spike/src/stats.rs::GBA_CLOCK_HZ` has used
/// for every published share so far.
///
/// The two differ by 174 cycles per 2048-sample frame (0.0166%); item 7-1
/// standardises on one. This module uses the exact one for a reason that is not
/// preference: the clock the product ROM measures *is* 2^24 — timer0 is loaded
/// with `0 − 512` ticks of it per sample — so a stage measured against this
/// number is measured against the thing that actually paced the audio.
pub const MIXER_TICK_HZ: u32 = 16_777_216;

/// The playback rate: `agb`'s `Frequency::Hz32768`, the mixer-domain rate C2 is
/// stated at. It is the only one of agb's three rates whose period is an exact
/// integer number of timer ticks (`MIXER_TICK_HZ / 32_768 == 512`); 10,512 Hz
/// needs 1596.1 and 18,157 Hz needs 924.0, so agb's own reload values for those
/// are rounded and their real rates are a few hundred ppm off.
pub const PLAYBACK_RATE_HZ: u32 = 32_768;

/// Timer ticks (= cycles at divider /1) per sample at the playback rate: exactly
/// 512. This is the unit every cost in this project eventually reduces to — see
/// `MEASURED_DECODE` in `scripts/gafp_pack.py`, which is quoted per frame, and
/// `flac-docs/measurements.md`, which quotes 193.4 cycles/sample for the same
/// row. At this rate the hardware spends 512 cycles on each sample it plays, and
/// the whole real-time question is whether the producer spends less.
pub const TICKS_PER_SAMPLE: u32 = MIXER_TICK_HZ / PLAYBACK_RATE_HZ;

/// A frame's period in cycles: `blocksize × TICKS_PER_SAMPLE`, which is what
/// `scripts/gafp_pack.py::frame_budget_cycles` computes as
/// `2^24 × blocksize / playback_rate`. For the product's 2,048-sample blocks
/// that is 2^20 = 1,048,576 cycles. The rounded-clock reading of the same frame
/// is 1,048,750; the ratio of one frame to the other is what the 0.0166% note
/// above is about.
pub const fn frame_cycles(blocksize: u32) -> u32 {
    blocksize * TICKS_PER_SAMPLE
}

/// A period in cycles: the interval at which the sound hardware asks for the
/// next buffer. `CHUNK_SAMPLES × TICKS_PER_SAMPLE` = 286,720 = 17.08 ms.
pub const PERIOD_CYCLES: u32 = (CHUNK_SAMPLES as u32) * TICKS_PER_SAMPLE;

// ---------------------------------------------------------------------------
// The buffer the hardware consumes, and the ring that feeds it
// ---------------------------------------------------------------------------

/// Samples per mixer-domain hardware buffer. `agb` 0.25.0's
/// `Frequency::buffer_size` (`src/sound/mixer/mod.rs`) returns 176 / 304 / 560
/// for 10,512 / 18,157 / 32,768 Hz; the function is `pub(crate)`, so the value
/// is transcribed and pinned by the assertion below. This is not a free choice:
/// it is the size of the DMA transfer agb's own mixer would program, and the
/// 32-byte FIFO refills when it falls to half, so a buffer must be a multiple of
/// 4 words — 560 is.
///
/// The producer never gets to pick a smaller buffer to make its own life easier:
/// the period is fixed by the hardware rate, and the DMA is re-armed once per
/// period. A smaller chunk would just mean more interrupts.
pub const CHUNK_SAMPLES: usize = 560;

/// Ring depth in chunks. 36 chunks = 20,160 samples = 615 ms of lead per
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
/// - **What depth buys against a *sustained* overage is real and finite.** With
///   no ring in the picture the producer fails at 153.6% of the measured cost;
///   with this ring it fails at 191.3% (178.8% against the hardware derate). The
///   refill is therefore worth ~38 points
///   of sustained overage — a purchase, not an infinite cushion
///   (`sustained_overage_tolerance` bisects both).
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
/// first time). 16 chunks = 8,960 samples = 273 ms of lead.
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

/// One hardware buffer in 32-bit words: the sound DMA's transfer count.
pub const DMA_WORDS: u16 = (CHUNK_SAMPLES / 4) as u16;

// ---------------------------------------------------------------------------
// The stream rate vs the playback rate
// ---------------------------------------------------------------------------

/// The playback rate this *stream* rate is played at. **For the product there is one
/// legitimate answer and it is the stream's own rate; every other entry on this table
/// is scaffolding** (Q14, ruled 2026-11-11 — the block below is the ruling).
/// The packer's table (`scripts/gafp_pack.py::PLAYBACK_RATE_FOR_STREAM`) is the
/// source: the asset in `assets/` is encoded at 32,000 Hz and played on the exact
/// 32,768 Hz timer, because
/// 32,768 is the only playback rate whose period is an integer number of ticks
/// and 32,000 is the only stream rate the encode profile holds a measured row at
/// (`scripts/gafp_pack.py::MEASURED_DECODE`, both rows keyed `(32000, 2048, …)`).
///
/// The consequence is stated, not hidden: the clip plays 32,768/32,000 = 1.024×
/// fast — 2.4% shorter and 41.06 cents sharp, the cents computed rather than
/// approximated in
/// `tests/product_play_witness.rs::ruled_rate_policy_allows_one_bridge_and_no_more` —
/// and the deadline the player runs is
/// 2.4% *tighter* than the stream's own frame period would ask for. `frame_cycles`
/// above prices the tight one. No instrument in this repository budgets from the
/// declared rate — every share here is divided by the timer's frame — and the
/// 2026-11-11 rate probe measured that the tightening prices nothing at current
/// cost levels: same `audio_span`, same period count, worst frame 59.4% either way.
///
/// **RULED (Q14, 2026-11-11, by the project owner): a stream is encoded at the rate
/// it is played at, and the player does not reinterpret a declared rate to fit the
/// clock. The 41 cents this mapping buys is not acceptable in any circumstances, so
/// the product carries a true 32,768 Hz stream and the `32_000` arm below is not a
/// shipping path.**
///
/// **`TODO(Q14, BLOCKING)` — delete the `32_000` arm when the 32,768 product asset
/// exists, and add nothing to this table before then.** Unblocking is four steps and
/// none of them are format work: resample the source to 32,768, encode it at the
/// product profile, measure on hardware the row `(32768, 2048, fixed, 0)` that
/// `gafp_pack`'s guard asks for (7-1), and re-run the play path against the result —
/// with a pitch witness in the same PR, because the pinned PCM digest cannot see a
/// pitch shift: the samples are identical either way, which is exactly how a 41-cent
/// error survived every gate this asset has been through.
///
/// What the 2026-11-11 rate probe settled, recorded here so the ruling is not
/// re-litigated from this comment: 32,768 is representable — no entry in the FLAC
/// 4-bit rate table, so such a stream spells the rate as code `0b0000` plus a 32,768
/// stream default (measured: not one frame changes length, the blob stays 865,996 B),
/// or as code `0b1101` plus a 16-bit Hz field, which is what `flac` itself emits when
/// asked and which costs 2 bytes per frame — and `flac-lite` decodes the first
/// spelling to a bit-identical PCM at the same flow-control floor and the same cost
/// within the instrument's floor. Neither the format nor the cost was ever the
/// obstacle; the pipeline and the pitch are. `flac-docs/measurements.md`, the 6b-3
/// rate row; `flac-docs/open-questions.md`, Q14; `flac-docs/plan-of-record.md`, 7-0.
pub fn playback_rate_for_stream(stream_rate_hz: u32) -> Option<u32> {
    match stream_rate_hz {
        // **`TODO(Q14, BLOCKING)`** — this is the measurement bridge, not a product
        // path: it buys 2.4% of speed and 41.06 cents of pitch. Ruled out on
        // 2026-11-11; it stays only until 7-0 replaces the asset, and it is deleted
        // here when that lands.
        32_000 => Some(32_768),
        // A stream that already declares the target rate needs no mapping.
        PLAYBACK_RATE_HZ => Some(PLAYBACK_RATE_HZ),
        _ => None,
    }
}

/// The cycles-per-sample the audio timeline itself allows: 512. Anything the
/// producer spends per sample beyond this is a debt the ring can cover once but
/// not repeatedly. Named so the model and the ROM say the same thing in the same
/// unit.
pub const AUDIO_BUDGET_CYCLES_PER_SAMPLE: u32 = TICKS_PER_SAMPLE;

// ---------------------------------------------------------------------------
// Compile-time pins: every transcription above is checkable here
// ---------------------------------------------------------------------------

const _: () = assert!(MIXER_TICK_HZ == 1 << 24);
const _: () = assert!(PLAYBACK_RATE_HZ == 32_768);
const _: () = assert!(TICKS_PER_SAMPLE == 512);
const _: () = assert!(MIXER_TICK_HZ % PLAYBACK_RATE_HZ == 0);
const _: () = assert!(frame_cycles(2048) == 1_048_576);
const _: () = assert!(CHUNK_SAMPLES == 560);
const _: () = assert!(CHUNK_SAMPLES % 4 == 0);
const _: () = assert!(PERIOD_CYCLES == 286_720);
const _: () = assert!(DMA_WORDS == 140);
const _: () = assert!(RING_SAMPLES == 20_160);
const _: () = assert!(PREFILL_SAMPLES == 8_960);
// The ring has to be able to hold the prefill *and* the largest block the
// profile admits, and the flow-control rule needs both a producer side and a
// consumer side to be satisfiable at once: free >= 2048 and ready >= 560.
const _: () = assert!(RING_SAMPLES > PREFILL_SAMPLES + 2048);
const _: () = assert!(RING_SAMPLES >= 2048 + CHUNK_SAMPLES);
// The period counter (timer2) is 16-bit and counts periods, so the composite
// cycle clock this module's arithmetic prices against wraps at 65,536 periods =
// 18.8 minutes. The ROM reports the period count it used and refuses to divide
// any share by a clock that wrapped; this pin says the ceiling is far above any
// asset the profile admits (the C2 clip plays 572 periods).
const _: () = assert!(u16::MAX as u64 * PERIOD_CYCLES as u64 > 1_600_000_000);
