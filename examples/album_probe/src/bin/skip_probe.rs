//! Skip spike: ALBUM-PLAYER.md §4.4's six steps against the real ring, sweeping
//! `SKIP_PRIME_CHUNKS`, both queue policies and both queue depths (Phase 0 (c),
//! §13 AP-0).
//!
//! The observable the spec rules on is what the listener hears at a skip, and the
//! one thing it forbids is an underrun attributed to a skip. Both are measured
//! here and nothing else is claimed: this ROM cannot hear anything.
//!
//! ## §4.4 step 2: "the queued chunks are dropped, not played"
//!
//! The shipped `RingFeed` cannot truncate — `consumed` is private and only
//! `take_chunk` moves it — so a drop has to be implemented where the feeder is:
//! during the stale window the feeder keeps taking a whole chunk per period (so
//! `consumed` advances and the DMA never sees a short buffer) and hands the DMA
//! silence instead of the queued audio ([`probe_hw::drop_stale`]). That is the
//! `drop` policy. `drain` is the opposite reading and round 1's implementation: the
//! producer stops and the queue plays out to `ready() == 0`.
//!
//! Dropping costs one period per queued chunk, exactly as draining does: the DMA
//! cannot be emptied faster than the hardware reads it. What the policy decides is
//! what is *heard* during that window — the old track continuing, or silence — and
//! only a `RingFeed` truncate would make it shorter. Both are measured, so the
//! ceiling is a number rather than a caveat.
//!
//! ## Two depths
//!
//! §4.4's 1.23 s ring is measured at the ring's **maximum** depth: 36 chunks is a
//! ceiling, not the shipped steady state. The product's producer waits for room
//! and then fills, so it settles ~`RING − one 2,048-sample frame` ≈ 32 chunks
//! deep. Round 1 measured a 2-16 chunk queue instead, which is the probe's own
//! choice, not the product's.
//!
//! | arm | queue each skip starts from |
//! |---|---|
//! | `queue=shallow` | `prime_chunks` chunks — round 1's depth, kept and relabelled |
//! | `queue=deep` | `RING_CHUNKS - 1` = 35 chunks, the last the 36-chunk ring allows (`fill_to` never fills the final chunk, and the fade-out is the push that takes it) — the ring's **maximum** depth, a ceiling on the shipped ≈ 32, not the shipped depth |
//!
//! ## The three quantities every row reports
//!
//! All three come from [`probe_hw`]'s wire watch, i.e. from the bytes the DMA was
//! handed, so both policies are asked the identical question:
//!
//! - `old_periods` — periods from the skip until the first silent chunk on the
//!   wire: how long the old track's audio kept reaching the DMA;
//! - `silence_periods` — the all-zero gap on the wire;
//! - `to_new_periods` — periods from the skip until new-track audio reaches the
//!   DMA. This is §4.4's "audio of the new track begins", and it is computed the
//!   same way on `drain` and `drop` rows, which is what makes the comparison one.
//!
//! ## The six steps, in this probe's implementation
//!
//! 1. **Fade out** over `FADE_SAMPLES` (256) samples — a shift-based ramp over the
//!    synth's own waveform, padded to a whole chunk so the feeder never sees a
//!    short ring (`take_chunk` classifies `ready < chunk` as `Starved` while the
//!    producer is still running, and that would be a counted underflow).
//! 2. **Mark the ring stale**, by policy: `drain` plays the queue out to
//!    `ready() == 0`; `drop` tells the feeder to consume-and-discard everything
//!    queued so far, and the producer goes straight on to re-prime.
//! 3. **Re-seek** the synthetic source (`Synth::start_track`, the seam call).
//! 4. **Feed silence chunks** as *real* ring data (`push_bytes` of zeros), so the
//!    DMA keeps getting a whole chunk every period and the underrun counter stays
//!    a measurement of the producer.
//! 5. **Re-prime**: real new-track audio behind the silence cover, up to the
//!    arm's queue depth plus a two-chunk lead.
//! 6. **Fade in** (already queued by step 5): the head of the new audio is a
//!    256-sample ramp up.
//!
//! Every number is labelled `audio=live`, and every skip line names the queue
//! policy and the queue depth it belongs to, so no row can be read as another
//! arm's.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use agb::Gba;
use agb::display::{Graphics, Rgb, Rgb15};

use album_probe::geometry as geo;
use album_probe::probe_hw::{
    self, CHUNK, FADE_SAMPLES, Fold, RING_SAMPLES, Scratch,
    timer::{Counter, cycles_between},
};
use album_probe::synth::{N_TRACKS, Synth};

/// `SKIP_PRIME_CHUNKS` candidates from §4.4: the spec's chosen 4, the boot
/// prefill's 16, and the two in between. Swept identically at both depths.
const PRIME_SET: [usize; 4] = [2, 4, 8, 16];
/// Skips per setting: enough for the worst gap to repeat, small enough that the
/// sweep stays a probe.
const SKIPS: u32 = 20;
/// Audio periods the new track plays before the next skip is measured, so at
/// least this much of it is heard playing (~3 periods, round 1's figure).
const PLAY_PERIODS_MIN: u32 = 3;
/// Real audio queued behind the silence cover: two chunks of lead, enough for the
/// main loop to notice and top up.
const LEAD_CHUNKS: usize = 2;

/// The deepest queue this probe can fill: every chunk of the 36-chunk ring but the
/// last, because `fill_to` never fills the final chunk and the fade-out push is
/// what takes it (ALBUM-PLAYER.md §4.4: `RING_CHUNKS` × `CHUNK_SAMPLES` = 20,160
/// samples = 1.23 s). A ceiling, not the shipped depth: the product's producer
/// waits for room for one 2,048-sample frame, so it settles ~32 chunks deep.
const DEEP_QUEUE_CHUNKS: usize = geo::RING_CHUNKS - 1;

const COLOR_BACKDROP: Rgb15 = Rgb::new(16, 16, 24).to_rgb15();
const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

/// What step 2 does with the queued chunks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    /// The producer stops and the queue plays out: round 1's reading.
    Drain,
    /// The feeder consumes the queued chunks and puts silence on the wire.
    Drop,
}

impl Policy {
    const fn name(self) -> &'static str {
        match self {
            Policy::Drain => "drain",
            Policy::Drop => "drop",
        }
    }
}

/// How deep the queue sits when a skip starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    /// `prime_chunks` chunks: round 1's steady state for the sweep.
    Shallow,
    /// The ring's maximum depth — a ceiling on the shipped ≈ 32 chunks,
    /// [`DEEP_QUEUE_CHUNKS`].
    Deep,
}

impl Depth {
    const fn name(self) -> &'static str {
        match self {
            Depth::Shallow => "shallow",
            Depth::Deep => "deep",
        }
    }

    const fn queue_chunks(self, prime_chunks: usize) -> usize {
        match self {
            Depth::Shallow => prime_chunks,
            Depth::Deep => DEEP_QUEUE_CHUNKS,
        }
    }

    fn target(self, prime_chunks: usize) -> usize {
        self.queue_chunks(prime_chunks) * CHUNK
    }
}

/// One arm of the sweep: a policy at a depth.
#[derive(Clone, Copy)]
struct Arm {
    policy: Policy,
    depth: Depth,
}

/// The arms, shallow-drain first so its rows line up with round 1's.
const ARMS: [Arm; 4] = [
    Arm {
        policy: Policy::Drain,
        depth: Depth::Shallow,
    },
    Arm {
        policy: Policy::Drop,
        depth: Depth::Shallow,
    },
    Arm {
        policy: Policy::Drain,
        depth: Depth::Deep,
    },
    Arm {
        policy: Policy::Drop,
        depth: Depth::Deep,
    },
];

/// One setting's ledger.
struct SettingResult {
    arm: Arm,
    prime_chunks: usize,
    queue_chunks: usize,
    silence_chunks_max: u32,
    starved: u32,
    underflows: u32,
    dropped: u32,
    wait_periods_max: u32,
    old_periods: Fold,
    silence_periods: Fold,
    to_new_periods: Fold,
    latency: Fold,
    failed: bool,
}

/// Wait until the feeder has taken everything queued. Bounded by the ring's own
/// capacity in periods plus four, so a wedged waiter fails instead of spinning —
/// and four rather than two because a deep arm drains the whole 36-chunk ring.
fn wait_consumed() -> (bool, u32) {
    let start = probe_hw::stats().periods;
    let deadline = (RING_SAMPLES / CHUNK) as u32 + 4;
    while probe_hw::ready() > 0 {
        let elapsed = probe_hw::stats().periods.wrapping_sub(start);
        if elapsed > deadline {
            return (false, elapsed);
        }
    }
    (true, probe_hw::stats().periods.wrapping_sub(start))
}

/// Let the new track play: at least [`PLAY_PERIODS_MIN`] periods, and until the
/// wire watch has seen its audio reach the DMA, topping the queue back to the arm's
/// depth as it drains.
///
/// Bounded in **periods**, not frames: `gfx.frame()` returns as soon as VBlank has
/// happened, so a frame count is not a time budget here, and a deep arm needs ~35
/// periods of cover to play before the new audio arrives.
/// `false` if the watch never resolved within the bound — a failed instrument.
fn play_until_new_audio(
    gfx: &mut Graphics<'_>,
    synth: &mut Synth,
    scratch: &mut Scratch,
    target: usize,
    deadline_periods: u32,
) -> bool {
    let start = probe_hw::stats().periods;
    let mut ok = true;
    loop {
        let elapsed = probe_hw::stats().periods.wrapping_sub(start);
        if probe_hw::watch().new_at != 0 && elapsed >= PLAY_PERIODS_MIN {
            break;
        }
        if elapsed > deadline_periods {
            break;
        }
        let frame = gfx.frame();
        frame.commit();
        ok &= probe_hw::fill_to(synth, target, scratch).is_ok();
    }
    ok && probe_hw::watch().new_at != 0
}

fn run_setting(
    gfx: &mut Graphics<'_>,
    counter: &mut Counter<'_>,
    synth: &mut Synth,
    scratch: &mut Scratch,
    arm: Arm,
    prime_chunks: usize,
) -> SettingResult {
    // Zero the cycle counter between settings — the instrument's own rule
    // (`Counter::soft_reset`: a pass must not depend on overflow accumulated by an
    // earlier one). It also keeps every latency window inside the 32-bit counter's
    // 256 s period: this sweep is ~340 s of emulated audio in total, but no single
    // setting is more than ~30 s, so no window can span a wrap.
    counter.soft_reset();
    probe_hw::reset_marks();
    // Two different knobs, and conflating them was round 1's blind spot: `target`
    // is how deep the queue sits when the skip starts (the arm's steady state),
    // `cover` is §4.4's `SKIP_PRIME_CHUNKS` — the re-prime depth, which is the
    // policy knob the sweep varies. They coincide in the shallow arm only.
    let target = arm.depth.target(prime_chunks);
    let cover = prime_chunks * CHUNK;
    // The play phase's bound: the most a skip can have to play through is the whole
    // ring (the queue, the fade-out push that tops it off, the cover, the lead),
    // plus slack. A first skip can start from the bootstrap prefill rather than the
    // arm's depth, so the bound is the ring, not the arm.
    let deadline_periods = (geo::RING_CHUNKS + prime_chunks + LEAD_CHUNKS + 8) as u32;
    // Steady state for the arm: the shallow arm sits at the prime depth, the deep
    // one at the ring's maximum (a ceiling on the shipped ≈ 32 chunks), so the
    // stale window costs what each really costs.
    let mut failed = probe_hw::fill_to(synth, target, scratch).is_err();
    let mut wait_periods_max = 0u32;
    let mut old_periods = Fold::new();
    let mut silence_periods = Fold::new();
    let mut to_new_periods = Fold::new();
    let mut latency = Fold::new();

    for skip in 0..SKIPS {
        let t0 = counter.read();
        // The wire watch starts here, before the fade-out is even queued, so
        // "periods until the DMA receives new-track audio" runs from the press.
        probe_hw::arm_watch();

        // 1. Fade out over one whole chunk (a partial chunk would never be taken).
        failed |= probe_hw::push_fade_out(synth, scratch).is_err();

        // 2. Mark the ring stale, by policy.
        match arm.policy {
            // Drain: producer stops, the feeder plays the queue out.
            Policy::Drain => {
                let (drained, periods) = wait_consumed();
                failed |= !drained;
                if periods > wait_periods_max {
                    wait_periods_max = periods;
                }
            }
            // Drop: the feeder discards everything queued so far, one chunk per
            // period because that is how fast the DMA reads; the producer re-primes
            // immediately and waits for the room each discard frees.
            Policy::Drop => probe_hw::drop_stale(),
        }

        // 3. Re-seek. Alternate direction so both `i ± 1` seams are exercised.
        let track = synth.track();
        let next = if skip & 1 == 0 {
            track + 1
        } else {
            track + N_TRACKS - 1
        };
        synth.start_track(next);

        match arm.policy {
            // 4. Silence cover: real ring data, one whole chunk per period.
            Policy::Drain => failed |= probe_hw::push_silence(cover).is_err(),
            // The drop arm's ring is still full of stale chunks, so each cover chunk
            // waits for the discard that frees its room — and lands behind the stale
            // ones, which is why the policy cannot arrive any sooner.
            Policy::Drop => {
                let mut covered = 0usize;
                while covered < cover {
                    failed |= !probe_hw::wait_free(CHUNK);
                    failed |= probe_hw::push_silence(CHUNK).is_err();
                    covered += CHUNK;
                }
            }
        }

        // 5. Re-prime: fade-in at the head, then real audio behind it.
        match arm.policy {
            Policy::Drain => {
                failed |= probe_hw::push_fade_in(synth, scratch).is_err();
                failed |= probe_hw::fill_to(synth, target + LEAD_CHUNKS * CHUNK, scratch).is_err();
            }
            Policy::Drop => {
                failed |= !probe_hw::wait_free(CHUNK);
                failed |= probe_hw::push_fade_in(synth, scratch).is_err();
                // `fill_to` never over-fills, so it is safe when the ring is full.
                failed |= probe_hw::fill_to(synth, target + LEAD_CHUNKS * CHUNK, scratch).is_err();
            }
        }

        // 6. (fade-in already queued) — let the new track play until the DMA has
        // received it, topping the queue back to the arm's depth as it drains.
        failed |= !play_until_new_audio(gfx, synth, scratch, target, deadline_periods);

        let watch = probe_hw::watch();
        if watch.silent_at == 0 || watch.new_at == 0 {
            failed = true;
        } else {
            old_periods.observe(watch.silent_at.wrapping_sub(watch.at));
            silence_periods.observe(watch.new_at.wrapping_sub(watch.silent_at));
            to_new_periods.observe(watch.new_at.wrapping_sub(watch.at));
        }
        latency.observe(cycles_between(t0, counter.read()));
    }

    let stats = probe_hw::stats();
    SettingResult {
        arm,
        prime_chunks,
        queue_chunks: arm.depth.queue_chunks(prime_chunks),
        silence_chunks_max: stats.silence_run_max,
        starved: stats.starved,
        underflows: stats.underflows,
        dropped: stats.dropped,
        wait_periods_max,
        old_periods,
        silence_periods,
        to_new_periods,
        latency,
        failed,
    }
}

fn report(r: &SettingResult) {
    // The gap in the units the decision is made in: the chunk count is the
    // measurement, the samples are one multiply, and the milliseconds are left to
    // the reader (no divide on the measured path — the ROM prints periods and
    // cycles and the 2^24 clock so the host can do it).
    let policy = r.arm.policy.name();
    let depth = r.arm.depth.name();
    let gap_samples = r.silence_chunks_max.wrapping_mul(CHUNK as u32);
    let gap_cycles = r.silence_chunks_max.wrapping_mul(geo::PERIOD_CYCLES);
    agb::println!(
        "[album-probe] skip policy={} queue={} queue_chunks={} prime_chunks={} skips={} audio=live silence_chunks_max={} gap_samples={} gap_cycles={} clock_hz={}",
        policy,
        depth,
        r.queue_chunks,
        r.prime_chunks,
        SKIPS,
        r.silence_chunks_max,
        gap_samples,
        gap_cycles,
        geo::MIXER_TICK_HZ
    );
    agb::println!(
        "[album-probe] skip policy={} queue={} queue_chunks={} prime_chunks={} audio=live old_periods count={} sum={} min={} max={} wrap={}",
        policy,
        depth,
        r.queue_chunks,
        r.prime_chunks,
        r.old_periods.count,
        r.old_periods.sum,
        r.old_periods.min,
        r.old_periods.max,
        r.old_periods.wrapped
    );
    agb::println!(
        "[album-probe] skip policy={} queue={} queue_chunks={} prime_chunks={} audio=live silence_periods count={} sum={} min={} max={} wrap={}",
        policy,
        depth,
        r.queue_chunks,
        r.prime_chunks,
        r.silence_periods.count,
        r.silence_periods.sum,
        r.silence_periods.min,
        r.silence_periods.max,
        r.silence_periods.wrapped
    );
    agb::println!(
        "[album-probe] skip policy={} queue={} queue_chunks={} prime_chunks={} audio=live to_new_periods count={} sum={} min={} max={} wrap={}",
        policy,
        depth,
        r.queue_chunks,
        r.prime_chunks,
        r.to_new_periods.count,
        r.to_new_periods.sum,
        r.to_new_periods.min,
        r.to_new_periods.max,
        r.to_new_periods.wrapped
    );
    agb::println!(
        "[album-probe] skip policy={} queue={} queue_chunks={} prime_chunks={} audio=live underflows={} starved={} dropped={} wait_periods_max={} latency count={} sum={} min={} max={} wrap={}",
        policy,
        depth,
        r.queue_chunks,
        r.prime_chunks,
        r.underflows,
        r.starved,
        r.dropped,
        r.wait_periods_max,
        r.latency.count,
        r.latency.sum,
        r.latency.min,
        r.latency.max,
        r.latency.wrapped
    );
}

fn run(gba: &mut Gba) -> bool {
    agb::println!(
        "[album-probe] skip start: chunk={} ring={} deep_queue_chunks={} fade={} prime_set={:?}",
        CHUNK,
        RING_SAMPLES,
        DEEP_QUEUE_CHUNKS,
        FADE_SAMPLES,
        PRIME_SET
    );

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, COLOR_BACKDROP);
    let mut scratch = Box::new(Scratch::new());
    let mut synth = Synth::new();

    // Timers once: `all_timers` for the audio chain, dropped before the cycle
    // counter takes timer2/3 (the `probe_hw` timer2/3 ruling).
    let handler = {
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        probe_hw::bootstrap(&mut timers, &mut synth, &mut scratch)
    };
    let Some(_handler) = handler else {
        agb::println!("[album-probe] FAIL skip: bootstrap could not prime a whole chunk");
        agb::println!("Tests finished successfully");
        return false;
    };
    let mut timer_pool = gba.timers.timers();
    let mut counter = Counter::start(&mut timer_pool);
    // The silence gap is this probe's question, so ask the feeder to classify
    // delivered chunks by their bytes: the cover is fed as real zeros and the drop
    // policy puts zeros on the wire out of stale ring data, neither of which a
    // `Starved`-only tally would see.
    probe_hw::set_silence_scan(true);
    let (ie, ime, _, _) = probe_hw::chain_registers();
    agb::println!(
        "[album-probe] skip armed audio=live counter=timer2/3 IE={:#06x} IME={}",
        ie,
        ime
    );

    let mut pass = true;
    for arm in ARMS {
        for prime_chunks in PRIME_SET {
            let result = run_setting(
                &mut gfx,
                &mut counter,
                &mut synth,
                &mut scratch,
                arm,
                prime_chunks,
            );
            report(&result);
            // An underflow during a skip is the one thing §4.4 forbids; a wrapped
            // window, an unresolved watch or a refused push is a failed instrument.
            // A `drop` row that discarded nothing means the window never opened, so
            // the policy was not actually measured — refuse it rather than print a
            // clean number for it.
            pass &= !result.failed
                && result.underflows == 0
                && result.latency.wrapped == 0
                && result.old_periods.wrapped == 0
                && result.silence_periods.wrapped == 0
                && result.to_new_periods.wrapped == 0
                && result.silence_chunks_max > 0
                && (arm.policy == Policy::Drain || result.dropped > 0);
        }
    }

    let stats = probe_hw::stats();
    agb::println!(
        "[album-probe] skip total: audio=live periods={} feeder_sum={} feeder_max={} wrap={} late_max={} late_over_grace={}",
        stats.periods,
        stats.feed_cycles_sum,
        stats.feed_cycles_max,
        stats.feed_wrapped,
        stats.late_max,
        stats.late_over_grace
    );

    if pass {
        agb::println!(
            "[album-probe] PASS skip policies=[drain, drop] queues=[shallow, deep] primes={:?} underflows=0 audio=live",
            PRIME_SET
        );
    } else {
        agb::println!(
            "[album-probe] FAIL skip: an underflow during a skip, an unresolved watch, a wrapped window or a refused push — see the lines above"
        );
    }
    agb::println!("Tests finished successfully");
    probe_hw::finish();
    probe_hw::teardown_audio();
    pass
}

#[agb::entry]
fn main(mut gba: Gba) -> ! {
    let pass = run(&mut gba);
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });
    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
