//! The host model behind the product playback geometry — plan item 6b-3's other
//! half, and the reason `make product-play-rom` is allowed to exist at all.
//!
//! The ROM (`src/product_play_main.rs`) answers "did it work, counted, at the
//! target rate". This file answers the question a headless run cannot: *why* it
//! worked, and how much of the geometry was load-bearing. It replays the real
//! frame sequence — decoded from the shipped `GAFP` blob through the real
//! `Decoder`, at the real frame lengths the manifest declares — through the real
//! [`RingFeed`] code and the real [`geometry`] constants, against a virtual audio
//! clock whose tick is [`geo::PERIOD_CYCLES`], with the producer charged the cost
//! the ROM measured for itself. Nothing here is a re-implementation: same ring,
//! same flow-control rule, same prefill, same priming of the first DMA buffer
//! before the first period, same classification of a short final period as a
//! `Tail`. A model that disagreed with the ROM about `CHUNK_SAMPLES`, or that
//! modelled a friendlier ring, would prove nothing; that is why both sides read
//! `src/geometry.rs` and `examples/flac_spike/src/playback.rs` rather than each
//! keeping a copy.
//!
//! ## What the model is *for*
//!
//! Three of the geometry's numbers are choices, and a choice that is not derived
//! is a guess:
//!
//! | number | the question answered here |
//! |---|---|
//! | `RING_CHUNKS` = 36 | how large a producer shortfall does it buy; what is the smallest capacity that still survives the measured cost with margin |
//! | `PREFILL_CHUNKS` = 16 | how much of that is free (boot time) rather than paid for in RAM, and why the first frame is the one that needs it |
//! | the flow-control rule (room for the *declared* blocksize before a frame is decoded) | why the lead floor is where the ROM measured it, and why the rule cannot be "room for the current frame's length" |
//!
//! And it separates two tolerances that get conflated in prose because their
//! numbers are orders of magnitude apart. A **burst** — one frame far more
//! expensive than its share, or a producer that stops outright — is absorbed by the
//! ring's *depth*: measured, a producer that stops outright is survived for **17
//! more periods** — the quantised prefill of 10,240 less the prime of 560 — the
//! 18th being the first period the model finds nothing to take, while the deepest
//! lead at which a frame will still be *started* is ~32 periods even though the
//! lead itself peaks at the ring's 36.
//! A **sustained overage** — the producer costing more than its share forever,
//! which is what a wrong clock assumption or a slower part would mean — is bounded
//! by *throughput*, and by nothing else: a frame of audio is
//! `frame_cycles / PERIOD_CYCLES` = **3.657** periods of the mixer clock, so a
//! producer that keeps up forever may spend `(PERIOD_CYCLES − feeder) × 3.657`
//! cycles per frame — **186.5%** of the measured cost. Ring depth does not move that
//! number, because a bounded buffer cannot pay a permanent deficit; what depth buys
//! is *time*, its lead spent once, which on the shipped 9.906 s clip shows up as a
//! few points of overage above the ceiling (190.6%, 178.1% at the hardware derate)
//! and shrinks as the clip lengthens. [`sustained_overage_tolerance`] measures the
//! clip and a doubled one; [`burst_stall_tolerance`] measures the transient that the
//! depth *is* worth. The two are juxtaposed rather than averaged because their
//! mechanisms differ: one is the clock, the other is the lead in hand.
//!
//! ## What the model cannot see
//!
//! The clock is stepped in whole periods, and the producer is credited each
//! period's cycles before that period's chunk is taken. So the model cannot
//! represent a period boundary landing *inside* a decode — which is what the ROM's
//! log shows happening on the first fed frame (its decode window carries 1.5
//! periods). Nor does it model the FIFO, the DMA re-arm grace, the `agb` interrupt
//! dispatcher, or a hardware that might cost more than mGBA does; the derate on
//! [`MEASURED`] is the stand-in for that last one and it is an assumption, not a
//! measurement.
//!
//! The lead floor is *not* on that list. An earlier draft of this file predicted
//! 7,840 there against the ROM's measured 9,120 and explained the 1,280-sample gap
//! as the model's coarser clock. That explanation was wrong, and the cross-check is
//! what proved it: the draft's prefill loop kept pushing until the ring ran out of
//! room instead of stopping at its target the way `Producer::prefill` does, so the
//! model was running a geometry the product does not have. Fix that one rule and the
//! two implementations agree exactly — `ceil(8,960 / 2,048) × 2,048 = 10,240` of
//! prefill, less the prime chunk, is 9,680, the number the 16,384 Hz ROM prints;
//! at a slower cost the first period's take happens before the first fed-phase push
//! and the floor is one chunk lower. Keeping a disagreement that was a bug would
//! have been the more expensive kind of honesty.
//!
//! ## Where the cost figures come from
//!
//! From a run, not an estimate: `product-play.gba` as this tree builds it — sha
//! `2abf61e8b18f4bac46ed680d2320f1b9237b3586e35e3f167f8aef2885eab5ff`, the
//! 16,384 Hz image with the chunked push — run under mGBA, whose serial log gives [`MEASURED`] and
//! [`FEEDER_PER_PERIOD`]. That run is 289/289 periods, zero underflows, zero late
//! services, work 25.507% / worst frame 26.727%, attributed decode 417,451 c/frame,
//! minimum lead 9,680 samples; it is an mGBA control, not a hardware claim.
//! The envelope is tied to the bytes
//! the ROM is built from on purpose: when a code change moves that image's `stages` line,
//! this block has to be re-read from a fresh run in the same change. Be exact about what
//! witnesses that: no host test reads a ROM log, so [`the_envelope_is_the_runs_own_sums`]
//! cannot notice a wholly-stale envelope — what it catches is a *part*-re-read, because it
//! locks the sums, means, boundary charge and attributed estimate to each other and to the
//! `isolated_estimate` the image prints. Drift is therefore caught by the ledger rule (a
//! record row cites the image that printed its numbers) until those printed lines are
//! pinned as a fixture and derived from, which the plan writes against 7-1. (The envelope
//! this block first carried came from the 2026-11-11
//! image `5dc786951b91b356…`, the first in this repository to play at the target rate:
//! 555,193 raw / 549,760 attributed / 420,894 isolated decode. That stays in the
//! record as *that* image's row; this block no longer quotes it, and the ceilings the
//! geometry prose quotes belong to whichever envelope is in force here.) Those are
//! mGBA's numbers, and the
//! plan's standing assumption is that memory-heavy stages run ~7% slower on silicon
//! (measured for the kernel A/B, `flac-docs/measurements.md`), so the central
//! claims are run again at that derate. A model that only passed at the emulator's
//! cost would be modelling the emulator.

#![forbid(unsafe_code)]

use flac_lite::{Decoder, FrameBuffers, Manifest};

use flac_integration::checksum::{Fnv1a64, fold_i16le_stereo};
use flac_integration::gafp_pin;
use flac_integration::geometry as geo;
use flac_integration::playback::{Chunk, RingFeed, convert_frame, fifo_byte, fold_bytes};

/// The shipped product asset, read from this crate's own `assets/` — the same
/// bytes `gafp-check` guards, `flac-rom` embeds and `product-play-rom` plays. Not
/// a fixture, not a synthesised stream: if the model and the ROM are ever found to
/// disagree, this is the thing they disagreed about.
const BLOB: &[u8] = include_bytes!("../assets/spike_l0.gfp");

/// One decoded frame, already through `playback::convert_frame` — i.e. exactly the
/// bytes the ROM pushes: FIFO-8 planar, biased the way the hardware wants.
type FrameBytes = (Vec<u8>, Vec<u8>);

/// Decode the clip into FIFO bytes, in manifest order.
///
/// Deliberately through the public [`Decoder`] with no kernel named — the host arm
/// of the same seam the ROM uses — because this model is about the *player*, and
/// its frames have to be the ones the manifest describes rather than ones this file
/// invented.
fn decoded_frames(manifest: &Manifest<'_>) -> Vec<FrameBytes> {
    let mut decoder = Decoder::new(manifest).expect("the shipped manifest must build a decoder");
    let capacity = FrameBuffers::required_len(manifest);
    let mut left = vec![0i32; capacity];
    let mut right = vec![0i32; capacity];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);
    let mut out = Vec::new();
    loop {
        match decoder
            .decode_next(&mut bufs)
            .expect("the shipped blob decodes on the host — `flac-test` witnesses that")
        {
            Some(frame) => {
                let n = frame.left.len();
                let mut bl = vec![0u8; n];
                let mut br = vec![0u8; n];
                convert_frame(frame.left, frame.right, &mut bl, &mut br)
                    .expect("the shipped asset is inside the i16 profile — `gafp-pack-check` enforces that at pack time");
                out.push((bl, br));
            }
            None => break,
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The cost envelope, read off a run
// ---------------------------------------------------------------------------

// The envelope is **derived from the run's own sums**, not transcribed from its
// printed means. A transcribed figure is the failure mode this project has already
// paid for once (the hand-computed FNV vector, `method-and-gates.md`'s false-green
// catalogue), and the first version of this block was a transcription that got the
// attribution wrong: it priced 538 period boundaries because 538 is the count the
// *closure* line reports across all four stages, while only 158 of them fall inside
// the decode windows whose sums it was subtracting them from (379 are in `wait` and
// one in `push`, and neither belongs in a decode window's subtraction). Derived from
// the sums below, the two figures cannot drift apart again, and
// `the_envelope_is_the_runs_own_sums` checks the derivation against the ROM's printed
// means.

/// The current image's mGBA run — `2abf61e8…` at 16,384 Hz, chunked push — `[pp-rom] stages`, as the sums
/// and counts that run printed. Means are recomputed here; nothing is copied. Re-read
/// these five from a fresh run whenever ROM code changes: the sums are the only honest
/// source, and the test named in the header re-checks them against the means that same
/// run prints.
const DECODE_SUM: u64 = 31_337_010;
const CONVERT_SUM: u64 = 8_475_888;
const PUSH_SUM: u64 = 1_351_391;
/// Fed-phase windows: the frames decoded with the audio running. 75 = the 80
/// frames less the 5 the prefill decoded before arming; the consumer's tail (one
/// period, 464 real samples) is not a decode window at all.
const STAGE_WINDOWS: u64 = 75;
/// Period boundaries that fell **inside the decode windows** — the only ones
/// nested in producer work. `convert` and `push` report `boundaries_in=0`.
const DECODE_BOUNDARIES_INSIDE: u64 = 5;
/// The same run's feeder: total over 289 serviced periods.
const FEEDER_SUM: u64 = 1_629_955;
const PERIODS_SERVICED: u64 = 289;

/// The feeder's own cost per period, from the same run: the mean of 289 measured
/// samples. It is deducted from each period's budget because the model charges the
/// producer [`MEASURED`], which excludes the nesting: the two halves of the GBA's
/// one core have to add up, not overlap.
///
/// Note what is *not* deducted: the producer's spin-wait for ring room. Spinning
/// holds the core but neither advances nor blocks progress — the ring is what gates
/// it — and it does not delay the interrupt handler, which is why the model treats
/// only the handler's cycles as scarce. That is not a simplification for the
/// model's convenience; it is how a single core with interrupts enabled behaves.
const FEEDER_PER_PERIOD: u64 = FEEDER_SUM / PERIODS_SERVICED;

/// One stage's mean over a fed-phase window, from its sum.
const fn window_mean(sum: u64) -> u64 {
    sum / STAGE_WINDOWS
}

/// The producer's own work per frame with the nesting left in: the three stage
/// means added, which is exactly what the ROM's "decode + convert + push" line
/// says (417,826 + 113,011 + 18,018 = 548,855).
const MEASURED_RAW: u64 =
    window_mean(DECODE_SUM) + window_mean(CONVERT_SUM) + window_mean(PUSH_SUM);

/// The same envelope with the boundaries nested inside those windows priced out of
/// it, because that time belongs to the interrupt, not to the producer: 548,855 −
/// (5/75) × 5,639 = 548,480.
///
/// That subtraction is the attribution the ROM prints itself, and it is the same
/// arithmetic — 417,826 − (5/75) × 5,639 = 417,451 — which is the very
/// `isolated_estimate` that image prints on its `attribution:` line, so the model and
/// the ROM derive that one figure identically. One period boundary landed in a `push`
/// window this run, and this envelope prices only the decode windows' nesting, so the
/// model is charged ~37 c/frame more than the ROM's fully attributed producer figure —
/// the conservative direction, and it moves nothing. The earlier images in this lineage
/// (`5dc7869…`, `750d01e9…`, `c0d3ac3…`) keep their own figures in the record. It is an estimate: an interrupt's
/// entry cost is not separable from the work it interrupted, which is why the
/// unattributed figure is charged too, in [`MEASURED_RAW`], and why nothing here turns
/// on the difference.
const MEASURED: u64 = MEASURED_RAW - FEEDER_PER_PERIOD * DECODE_BOUNDARIES_INSIDE / STAGE_WINDOWS;

/// The plan's standing hardware derate for memory-heavy stages, applied as a second
/// scenario wherever a claim is meant to survive silicon. It is a *measured* ratio
/// for the one thing it was measured on — the kernel A/B ran +7.09% on hardware
/// against mGBA's +7.90%, `flac-docs/plan-of-record.md` — and is used here as a
/// deliberately blunt penalty, not a claim about the mechanism behind it.
const HARDWARE_DERATE_PERCENT: u64 = 7;

/// The ring watermark the current image's run observed, for the model to compare
/// against: the minimum lead seen immediately before a push, and the maximum seen
/// immediately after one.
const ROM_OBSERVED_MIN_LEAD: usize = 9_680;
/// …and the maximum.
const ROM_OBSERVED_MAX_LEAD: usize = 20_160;

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// The outcome of one replay: everything the ROM's own verdict is made of, plus the
/// lead trace that the geometry questions are answered from.
#[derive(Default)]
struct Outcome {
    underruns: u32,
    /// The period at which the consumer was first starved while the producer still
    /// had work. The underrun *count* says a run failed; this says how long the
    /// lead in hand lasted, which is the quantity the ring's depth is bought for.
    first_starve: Option<u64>,
    /// Periods the consumer was starved while the producer still had work. This is
    /// the ROM's `underflows`, and the only number here that C2 is judged on.
    periods: u64,
    tail_periods: u32,
    tail_real: usize,
    /// Samples taken into the DMA before any period was clocked (the prime). The
    /// ROM's ledger includes this in `consumed`, so any arithmetic that leaves it
    /// out is off by exactly one chunk — which is how the first draft of this model
    /// got its tail arithmetic wrong.
    primed: usize,
    pushed: usize,
    consumed: usize,
    min_lead: usize,
    max_lead: usize,
    /// Whether the flow control ever turned the producer away — the difference
    /// between a model of the player and a model of an unconstrained queue.
    gated_by_ring: u64,
    /// Whether the producer was ever rate-limited by its own cost instead of by the
    /// ring. This is the state a sustained overage is made of.
    gated_by_cost: u64,
    complete: bool,
}

/// One knob-set: what the producer costs, what the feeder costs, and how deep the
/// prefill is. The ring's *capacity* is a type parameter, not a field — the point
/// of the model is that a capacity is a compile-time fact in the product path too.
struct Settings {
    work_per_frame: u64,
    feeder_per_period: u64,
    prefill_chunks: usize,
    /// One frame charged a different cost, for the variance test: `(index, cycles)`.
    /// The mean-cost replay is the geometry test; this is what says a single frame
    /// at the ROM's measured worst still has a ring under it.
    burst: Option<(usize, u64)>,
}

impl Settings {
    /// The geometry as shipped, at a given producer cost.
    fn shipped(work_per_frame: u64) -> Self {
        Self {
            work_per_frame,
            feeder_per_period: FEEDER_PER_PERIOD,
            prefill_chunks: geo::PREFILL_CHUNKS,
            burst: None,
        }
    }

    /// The plan's derate, applied to a cost.
    fn derate(work: u64) -> u64 {
        work * (100 + HARDWARE_DERATE_PERCENT) / 100
    }
}

/// The producer's side of the state. Three fields, because that is all the ROM has:
/// where the frame walk is, whether it has finished, and how much CPU it has left
/// to spend before the next period's worth arrives.
struct Sim<'f> {
    frames: &'f [FrameBytes],
    next_frame: usize,
    /// Producer CPU credit, accumulated from audio time and spent on frames. This
    /// is the entire model of "the producer is a machine that costs N cycles per
    /// frame": it is never handed more than the audio leaves it.
    credit: u64,
    finished: bool,
    /// A push ceiling, in samples pushed: `Some` only during the prefill, because
    /// that is the one phase where the producer is asked to stop at a number
    /// rather than at the ring's edge. The ROM's `Producer::prefill` has this
    /// (`while self.feed.pushed() < target`), and without it the model fills the
    /// ring instead of the prefill — which is the bug this field exists to make
    /// impossible, and the reason an earlier draft of this model reported a lead
    /// floor of 17,312 where both it and the ROM say 9,120.
    push_ceiling: Option<usize>,
    /// The room the flow control demands before *starting* a frame: the declared
    /// blocksize, because a frame's real length is unknown until it is decoded.
    /// Passed in from the manifest, never from the frames themselves — see
    /// [`flow_control_floor_is_the_declared_blocksize`].
    reserve: usize,
    /// One frame's cost override, from [`Settings::burst`].
    burst: Option<(usize, u64)>,
    /// Watermark, recorded at the ROM's instants: before and after each push,
    /// inside the same critical section.
    min_lead: usize,
    max_lead: usize,
    record: bool,
}

impl<'f> Sim<'f> {
    /// Push while the ring allows it and the credit allows it, exactly as the
    /// producer loop does. Returns `(stopped_for_room, stopped_for_cost)`; either
    /// is a normal steady state, and which one it is decides whether the geometry
    /// is working or the cost is wrong.
    fn produce<const R: usize>(&mut self, ring: &mut RingFeed<R>, work: u64) -> (bool, bool) {
        let mut out_of_room = false;
        let mut out_of_credit = false;
        loop {
            if self.next_frame >= self.frames.len() {
                self.finished = true;
                return (out_of_room, out_of_credit);
            }
            if ring.free_samples() < self.reserve {
                out_of_room = true;
                return (out_of_room, out_of_credit);
            }
            let cost = match self.burst {
                Some((index, cycles)) if index == self.next_frame => cycles,
                _ => work,
            };
            if self.credit < cost {
                out_of_credit = true;
                return (out_of_room, out_of_credit);
            }
            if let Some(ceiling) = self.push_ceiling {
                if ring.pushed() >= ceiling {
                    return (out_of_room, out_of_credit);
                }
            }
            let (bl, br) = &self.frames[self.next_frame];
            if self.record {
                self.min_lead = self.min_lead.min(ring.ready());
            }
            ring.push_bytes(bl, br)
                .expect("the room check above is the same one `push_bytes` re-applies");
            if self.record {
                self.max_lead = self.max_lead.max(ring.ready());
            }
            self.credit -= cost;
            self.next_frame += 1;
        }
    }
}

/// Take one period's worth out of the ring, the way the feeder does, and report
/// both the verdict and the sample count it implies.
///
/// A free function rather than a method or a trait because the ring *is* the
/// product's `RingFeed<R>`: the const parameter is the only axis the model moves
/// along, so an abstraction over the ring type would be a second implementation
/// with nothing to abstract over.
fn take_chunk<const R: usize>(
    ring: &mut RingFeed<R>,
    chunk: usize,
    finished: bool,
) -> (usize, Chunk) {
    let mut dst_l = vec![0u8; chunk];
    let mut dst_r = vec![0u8; chunk];
    let verdict = ring.take_chunk(&mut dst_l, &mut dst_r, chunk, finished);
    let real = match verdict {
        Chunk::Full => chunk,
        Chunk::Tail { real } => real,
        Chunk::Starved => 0,
    };
    (real, verdict)
}
/// Run one scenario to the end of the clip.
///
/// The steps are the ROM's, in the ROM's order:
///
/// 1. prefill, with no feeder armed and no audio clock — the ring is filled before
///    sound exists, exactly as `Producer::prefill` does, and it is not charged
///    against the period budget for the same reason the ROM does not charge it: it
///    happens before the thing being budgeted begins;
/// 2. prime one chunk into the first DMA buffer, before the first period boundary,
///    so the sound hardware has something to play the moment it is enabled;
/// 3. per period: top up the producer's credit for that period's cycles less the
///    feeder's own, let the producer run while the ring and its credit allow, then
///    take one chunk — or record a starvation;
/// 4. once the frame walk is done, mark `finished` and keep taking until the
///    ledger closes; the residue lands as one classified `Tail`, not an underrun.
///
/// The watermarks are recorded at the ROM's instants (immediately before and after
/// each push) and only from the prime onward, because a lead measured before audio
/// exists is a lead of nothing. That is the same condition the ROM runs under, so
/// the two numbers are the same quantity — which is what makes
/// [`floor_matches_the_roms_watermark`] a comparison rather than a coincidence.
fn run<const R: usize>(
    settings: &Settings,
    frames: &[FrameBytes],
    total_samples: usize,
) -> Outcome {
    let chunk = geo::CHUNK_SAMPLES;
    // The reserve the flow control holds: the largest frame this asset can contain,
    // taken from the walk here but demanded by the ROM from the *manifest*, which
    // is the only place it is knowable before decoding. For this asset the two are
    // the same number, and [`flow_control_floor_is_the_declared_blocksize`] is the
    // test that says so.
    let reserve = frames
        .iter()
        .map(|(l, _)| l.len())
        .max()
        .unwrap_or(chunk)
        .max(chunk);
    assert!(
        settings.prefill_chunks * chunk + reserve <= R,
        "a prefill of {} chunks cannot coexist with a {}-sample reserve in a {}-sample ring: the prefill could never complete, so this is not a smaller geometry, it is an infeasible one",
        settings.prefill_chunks,
        reserve,
        R
    );

    let mut ring = RingFeed::<R>::new();
    let mut sim = Sim {
        frames,
        next_frame: 0,
        credit: 0,
        finished: frames.is_empty(),
        push_ceiling: None,
        reserve,
        burst: settings.burst,
        min_lead: usize::MAX,
        max_lead: 0,
        record: false,
    };

    // 1. prefill — bounded by progress rather than by the target being reachable.
    // The flow control may legitimately stop it short of the target; the assert
    // above is what says whether that would be expected or fatal.
    // The ceiling is the target, and the producer stops when a push has *crossed*
    // it, not when it has landed on it: frames are 2,048 samples and the target is
    // not a multiple of that, so the prefill's real depth is
    // `ceil(target / frame) * frame`. The model derives that the same way the ROM
    // earns it, which is what lets [`floor_matches_the_roms_watermark`] predict the
    // ROM's measured floor instead of merely tolerating it.
    sim.push_ceiling = Some(settings.prefill_chunks * chunk);
    while ring.ready() < settings.prefill_chunks * chunk {
        let before = ring.pushed();
        sim.produce(&mut ring, 0);
        if sim.finished || ring.pushed() == before {
            break;
        }
    }
    sim.push_ceiling = None;

    // 2. prime
    let (primed, verdict) = take_chunk(&mut ring, chunk, false);
    assert!(
        matches!(verdict, Chunk::Full),
        "a prefill of {} chunks cannot prime one chunk of {} (ready {} after prefilling {})",
        settings.prefill_chunks,
        chunk,
        ring.ready(),
        ring.pushed()
    );

    let mut out = Outcome {
        min_lead: usize::MAX,
        primed,
        ..Outcome::default()
    };

    // 3. the clock starts here, and so does the watermark. `+2` periods of horizon
    // because the last frame's audio outlives its push by up to a ring's worth.
    sim.record = true;
    let horizon = total_samples as u64 / chunk as u64 + 2;
    while out.periods < horizon {
        sim.credit += u64::from(geo::PERIOD_CYCLES) - settings.feeder_per_period;
        let (room, cost) = sim.produce(&mut ring, settings.work_per_frame);
        out.gated_by_ring += u64::from(room);
        out.gated_by_cost += u64::from(cost);

        let (real, verdict) = take_chunk(&mut ring, chunk, sim.finished);
        out.periods += 1;
        match verdict {
            Chunk::Full => {}
            Chunk::Tail { real } => {
                out.tail_periods += 1;
                out.tail_real += real;
            }
            Chunk::Starved => {
                if !sim.finished {
                    out.underruns += 1;
                    if out.first_starve.is_none() {
                        out.first_starve = Some(out.periods);
                    }
                }
            }
        }
        let _ = real;

        // 4. the ledger closes when everything that was pushed has been taken.
        if sim.finished && ring.consumed() >= ring.pushed() {
            out.complete = ring.consumed() == total_samples;
            break;
        }
    }
    out.min_lead = out.min_lead.min(sim.min_lead);
    out.max_lead = out.max_lead.max(sim.max_lead);
    // A run that never pushed once after arming has an unrecorded floor, and the
    // truth of that is zero, not whatever the sentinel was.
    if out.min_lead == usize::MAX {
        out.min_lead = 0;
    }
    out.pushed = ring.pushed();
    out.consumed = ring.consumed();
    out
}

// ---------------------------------------------------------------------------
// 1. the asset and the geometry, before any player is involved
// ---------------------------------------------------------------------------

/// The coupling the geometry assumes about the asset, witnessed against the asset
/// instead of assumed by a comment.
///
/// `geometry.rs` pins its own transcriptions — the clock, the divider, the ring's
/// divisibility — at compile time, because those are facts about the GBA. It cannot
/// pin the frame length, because that is a fact about the *encoding*, and
/// `geo::frame_cycles()` silently computes the wrong budget if the two ever come
/// apart. This test closes that gap. It is also where "the generated offset table
/// is retired" stops being a stated claim: every frame length here comes from the
/// manifest walk, and neither this file nor the ROM carries a per-frame table.
#[test]
fn manifest_and_geometry_agree() {
    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    assert_eq!(manifest.frame_count(), gafp_pin::FRAMES);
    assert_eq!(manifest.total_samples(), gafp_pin::SAMPLES);
    assert_eq!(manifest.channels(), 2);
    assert_eq!(manifest.bits_per_sample(), 16);
    assert_eq!(
        manifest.max_fixed_order(),
        gafp_pin::MAX_FIXED_ORDER,
        "the asset must still be FIXED-0, which is what the target rate's budget is stated for"
    );
    flac_lite::format::DEFAULT_PROFILE
        .validate(&manifest)
        .expect("the shipped asset is inside the encode profile — `gafp-pack-check` enforces that at pack time");

    // The geometry's assumption about the asset, stated as an assertion: a frame's
    // cycle budget must be exactly its sample count times the tick price, and at
    // the target rate that is 2^20.
    let block = u64::from(manifest.blocksize());
    assert_eq!(
        u64::from(geo::frame_cycles(u32::from(manifest.blocksize()))),
        block * geo::TICKS_PER_SAMPLE as u64,
        "a frame's budget must be its samples times the tick price — otherwise the packer and the player are budgeting different frames"
    );
    assert_eq!(
        u64::from(geo::frame_cycles(u32::from(manifest.blocksize()))),
        1 << 21,
        "2048-sample frames at 16,384 Hz must come to exactly 2^21 cycles, which is what every published share at this rate is divided by"
    );

    // The rate contract, witnessed against the asset rather than assumed by the
    // geometry: the blob declares the clock this file budgets against, so the
    // stream's own nominal frame period and the player's frame budget are the same
    // number. Under the bridge this test used to sit behind, the two differed by
    // 2.4% and the assert here was `budget < nominal` — a sentence about a mapping,
    // not about a contract. Q14 ruled the mapping out (2026-11-11), plan item 7-0
    // removed it, and what is left to check is stricter: equality in both the rate
    // and the period it implies. A blob on any other rate never reaches here — the
    // packer refuses to write one and the ROM refuses to play one — and
    // `ruled_rate_policy_*` below is where the refusal side is swept.
    let declared = manifest.sample_rate_hz();
    let played = geo::playback_rate_for_stream(declared)
        .expect("the product asset must declare a rate the player accepts as-is");
    assert_eq!(
        declared,
        geo::PLAYBACK_RATE_HZ,
        "the shipped asset must declare the product clock it is played at (Q14)"
    );
    assert_eq!(played, declared, "identity is the only mapping left");
    let nominal = geo::MIXER_TICK_HZ as u64 * block / u64::from(declared);
    let budget = u64::from(geo::frame_cycles(u32::from(manifest.blocksize())));
    assert_eq!(
        budget, nominal,
        "the frame the player budgets must be the frame the stream was made for: \
         a {} Hz stream's own {}-sample period is {} c and the player's budget is {} c",
        declared, block, nominal, budget
    );
    println!(
        "model/asset: {} frames of {}, {} samples ({:.3} s at {} Hz); declared {} Hz, played at {} Hz; \
         the stream's own nominal frame period and the player's budget are both {} c",
        manifest.frame_count(),
        manifest.blocksize(),
        manifest.total_samples(),
        manifest.total_samples() as f64 / declared as f64,
        declared,
        declared,
        played,
        budget
    );

    // The one ring arithmetic the geometry must never get wrong, and which is not
    // derivable from the GBA: the prefill has to leave room for a whole frame, or
    // `Producer::prefill` would spin forever on its first refusal. This is also
    // the reason the ROM's prefill overshoot (it pushes whole frames, so it lands
    // at 10,240 rather than the 8,960 it asked for) is harmless rather than
    // lucky — the overshoot is bounded by one frame, and one frame is what this
    // inequality has already paid for.
    assert!(
        geo::PREFILL_SAMPLES + block as usize <= geo::RING_SAMPLES,
        "prefill {} + a frame's reserve {} must fit a ring of {}; otherwise the prefill cannot finish and the run never starts",
        geo::PREFILL_SAMPLES,
        block,
        geo::RING_SAMPLES
    );
    assert!(
        geo::RING_SAMPLES >= 2 * block as usize,
        "the flow control demands room for a whole frame, so a ring shallower than two frames cannot be gated at all"
    );

    let frames = decoded_frames(&manifest);
    assert_eq!(frames.len(), gafp_pin::FRAMES);
    let total: usize = frames.iter().map(|(l, _)| l.len()).sum();
    assert_eq!(total as u64, gafp_pin::SAMPLES);
    assert!(
        frames[..frames.len() - 1]
            .iter()
            .all(|(l, _)| l.len() as u64 == block)
    );
    let last = frames.last().unwrap().0.len();
    assert!(last as u64 <= block);
    println!(
        "model/frames: every frame {} samples except the last ({}); the walk supplies the lengths, and no offset table exists on either side",
        block, last
    );
}

/// The played-form pin, witnessed on the host against the same converted frames
/// the ROM pushes.
///
/// `decoded_frames` already runs the real `convert_frame`, so folding its bytes
/// per channel with the shared `fold_bytes` is the host twin of the ROM's
/// post-audio proof fold. The same frames are then decoded once more and folded through the i16
/// interleave to confirm the two pins describe one stream — the decode witness
/// (`FNV_PCM`) and the played-form witness (`FNV_FIFO_L/R`) cannot part company
/// without this test going red.
#[test]
fn fifo_pins_match_the_converted_stream() {
    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    let frames = decoded_frames(&manifest);

    let mut hl = Fnv1a64::new();
    let mut hr = Fnv1a64::new();
    for (l, r) in &frames {
        fold_bytes(&mut hl, &mut hr, l, r);
    }
    assert_eq!(
        hl.finish(),
        gafp_pin::FNV_FIFO_L,
        "the left channel's pushed FIFO bytes must hash to the generated pin"
    );
    assert_eq!(
        hr.finish(),
        gafp_pin::FNV_FIFO_R,
        "the right channel's pushed FIFO bytes must hash to the generated pin"
    );

    // The same decoded stream's i16 form is the decode pin. A separate decode
    // keeps `decoded_frames`' return shape (the rest of this file indexes it),
    // and 157 frames is cheap on the host.
    let mut decoder = Decoder::new(&manifest).expect("the shipped manifest must build a decoder");
    let capacity = FrameBuffers::required_len(&manifest);
    let mut left = vec![0i32; capacity];
    let mut right = vec![0i32; capacity];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);
    let mut pcm = Fnv1a64::new();
    let mut n = 0usize;
    loop {
        match decoder
            .decode_next(&mut bufs)
            .expect("the shipped blob decodes on the host")
        {
            Some(frame) => {
                fold_i16le_stereo(&mut pcm, frame.left, frame.right)
                    .expect("the shipped blob is inside the i16 profile");
                n += frame.left.len();
            }
            None => break,
        }
    }
    assert_eq!(n as u64, gafp_pin::SAMPLES);
    assert_eq!(pcm.finish(), gafp_pin::FNV_PCM);
    println!(
        "model/fifo: {} converted frames fold to L=0x{:016X} R=0x{:016X}; the same stream's i16 fold is 0x{:016X}",
        frames.len(),
        hl.finish(),
        hr.finish(),
        pcm.finish()
    );
}

// ---------------------------------------------------------------------------
// The judgement
// ---------------------------------------------------------------------------

/// The cost envelope the rest of these tests are charged is the run's own sums, and
/// the derivation is arithmetic a reader can re-do from the log: the three stage
/// means add to 548,855 as printed, the 5 boundaries inside the decode windows
/// price out at (5/75) × 5,639 = 375, and 417,826 − 375 = 417,451 is the
/// `isolated_estimate` that same image prints for itself.
///
/// This test exists because the block it guards was a transcription first, and the
/// transcription was wrong in the permissive direction — it charged the model 2.4%
/// less than the ROM's own arithmetic says the producer costs (the 2026-11-13 16,384 Hz image).
/// Its second job is the one that earned its keep on 2026-11-12: the envelope had by
/// then been left behind by three later builds, and this is the check that says so.
#[test]
fn the_envelope_is_the_runs_own_sums() {
    assert_eq!(
        window_mean(DECODE_SUM),
        417_826,
        "the ROM printed this mean"
    );
    assert_eq!(
        window_mean(CONVERT_SUM),
        113_011,
        "the ROM printed this mean"
    );
    assert_eq!(window_mean(PUSH_SUM), 18_018, "the ROM printed this mean");
    assert_eq!(FEEDER_PER_PERIOD, 5_639, "1,629,955 / 289, as printed");
    assert_eq!(MEASURED_RAW, 548_855, "the ROM's decode + convert + push");

    // The attribution, computed the way the ROM computes it (its own per-window
    // `inside` over its own `n`), against the constant this file charges.
    let nested = FEEDER_PER_PERIOD * DECODE_BOUNDARIES_INSIDE / STAGE_WINDOWS;
    assert_eq!(nested, 375);
    assert_eq!(MEASURED, 548_480);
    let isolated_decode = window_mean(DECODE_SUM) - nested;
    assert_eq!(
        isolated_decode, 417_451,
        "the image prints `attribution: decode ... isolated_estimate=417451` itself; the \
         model derives that number from the same sums, and if it moves the two records \
         have parted company"
    );

    // And the direction of the correction, so a future edit that re-inflates the
    // envelope has to argue with a number rather than with prose.
    assert!(
        MEASURED < MEASURED_RAW,
        "the attribution can only ever remove interrupt time from producer cost"
    );
    println!(
        "model/envelope: raw {} c/frame, nesting {} ({} boundaries over {} windows at {} c), attributed {} c/frame = {:.3}% of the raw — and the pricing of a frame's deadline is unchanged either way, which is the test below",
        MEASURED_RAW,
        nested,
        DECODE_BOUNDARIES_INSIDE,
        STAGE_WINDOWS,
        FEEDER_PER_PERIOD,
        MEASURED,
        MEASURED as f64 * 100.0 / MEASURED_RAW as f64
    );
}

/// The "cannot drift" claim, witnessed structurally: the model instantiates the
/// shipped ring *type* at the shipped capacity, with the chunk size and the DMA
/// word count the geometry declares. If `geometry.rs` and the register block ever
/// part company, this says so in one line.
#[test]
fn model_and_rom_instantiate_the_same_ring() {
    let ring = RingFeed::<{ geo::RING_SAMPLES }>::new();
    assert_eq!(ring.free_samples(), geo::RING_SAMPLES);
    assert_eq!(geo::RING_SAMPLES, geo::RING_CHUNKS * geo::CHUNK_SAMPLES);
    let layout = core::alloc::Layout::for_value(&ring);
    assert!(
        layout.size() >= 2 * geo::RING_SAMPLES,
        "the ring type's size ({}) must be at least two channels of {} samples — if it is not, the capacity the model was told about is not the capacity the model is using",
        layout.size(),
        geo::RING_SAMPLES
    );
    // And the DMA shape follows from the same numbers, because it is the same
    // numbers: 560 bytes per channel is 140 words, which is what the register
    // block has to be loaded with, and a period is 560 samples at 1,024 cycles each.
    assert_eq!(geo::DMA_WORDS as usize * 4, geo::CHUNK_SAMPLES);
    assert_eq!(
        geo::PERIOD_CYCLES,
        geo::CHUNK_SAMPLES as u32 * geo::TICKS_PER_SAMPLE
    );
    assert_eq!(
        geo::PERIOD_CYCLES * geo::RING_CHUNKS as u32,
        u32::try_from(geo::RING_SAMPLES as u64 * geo::TICKS_PER_SAMPLE as u64).unwrap(),
        "the ring's depth in cycles must equal its depth in samples times the tick price — otherwise 'how long can the producer stall' has two answers"
    );
    assert_eq!(
        u32::from(geo::REARM_GRACE_SAMPLES) * geo::TICKS_PER_SAMPLE,
        16_384
    );
}

// ---------------------------------------------------------------------------
// 2. the judge, modelled
// ---------------------------------------------------------------------------

/// The model reproduces the ROM's verdict at the cost the ROM charged itself, and
/// at the hardware derate — which is the version of the claim that has to survive
/// silicon.
#[test]
fn zero_underflow_at_the_measured_cost_and_at_the_derate() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();

    for (label, work) in [
        ("measured, nesting attributed out", MEASURED),
        ("measured raw, nesting left in", MEASURED_RAW),
        ("measured + hardware derate", Settings::derate(MEASURED)),
        (
            "measured raw + hardware derate",
            Settings::derate(MEASURED_RAW),
        ),
    ] {
        let settings = Settings::shipped(work);
        let out = run::<{ geo::RING_SAMPLES }>(&settings, &frames, total);
        println!(
            "model/{}: work/frame {} c -> underruns={} pushed={} consumed={} periods={} tail_periods={} tail_real={} primed={} min_lead={} max_lead={} gated_by_ring={} gated_by_cost={}",
            label,
            work,
            out.underruns,
            out.pushed,
            out.consumed,
            out.periods,
            out.tail_periods,
            out.tail_real,
            out.primed,
            out.min_lead,
            out.max_lead,
            out.gated_by_ring,
            out.gated_by_cost
        );
        assert_eq!(
            out.underruns, 0,
            "{label}: the model must reproduce the ROM's zero"
        );
        assert_eq!(
            out.pushed, total,
            "{label}: the whole clip must reach the ring"
        );
        assert_eq!(
            out.consumed, total,
            "{label}: and reach the DMA exactly once"
        );
        assert!(out.complete, "{label}: the ledger must close");
        assert_eq!(
            out.tail_periods, 1,
            "{label}: exactly one short final period, classified rather than counted against us"
        );
        // The ledger, including the prime: a period loop that leaves the primed
        // chunk out is off by exactly one chunk, and that is the arithmetic the
        // first draft of this model got wrong.
        assert_eq!(
            out.primed
                + usize::try_from(out.periods - 1).unwrap() * geo::CHUNK_SAMPLES
                + out.tail_real,
            total,
            "prime + full periods + tail must be the whole clip"
        );
        assert_eq!(
            out.tail_real,
            total - out.primed - usize::try_from(out.periods - 1).unwrap() * geo::CHUNK_SAMPLES,
            "{label}: the tail is the arithmetic residue, not a value carried in a table"
        );
        assert!(
            out.gated_by_ring > 0,
            "{label}: the ring must actually gate the producer, or nothing here is modelling the flow control"
        );
        assert!(
            out.min_lead >= 8 * geo::CHUNK_SAMPLES,
            "{label}: the lead floor must stay above eight periods of audio"
        );
        assert!(
            out.max_lead <= geo::RING_SAMPLES,
            "{label}: the lead cannot exceed the ring"
        );
    }
}

/// A single frame at the ROM's measured worst must still be absorbed.
///
/// The mean-cost replay above is what the geometry is sized by; this is the
/// variance test it does not cover. `ROM_WORST_WORK` is the **largest** worst-frame
/// decode+convert+push any recorded build of this geometry has reported — 565,939 c,
/// the played-form row in `flac-docs/measurements.md`; the image this tree builds
/// measures 565,859 c, and runs of a single image differ by up to 72 c. Charging the
/// largest recorded value, rather than transcribing whichever log was read last, is the
/// conservative direction and the one that does not rot when the image is rebuilt. It
/// is charged at every *fed-phase* position in turn. The prefill frames are skipped: the ROM
/// does not time them and the model charges prefill with zero credit, so a burst
/// there is a different (and uninteresting) scenario.
#[test]
fn worst_frame_burst_does_not_starve() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();
    let block = usize::from(manifest.blocksize());
    let prefill_frames = (geo::PREFILL_SAMPLES + block - 1) / block;
    const ROM_WORST_WORK: u64 = 560_506; // the largest recorded; see the doc above
    for at in prefill_frames..frames.len() {
        let settings = Settings {
            burst: Some((at, ROM_WORST_WORK)),
            ..Settings::shipped(MEASURED)
        };
        let out = run::<{ geo::RING_SAMPLES }>(&settings, &frames, total);
        assert_eq!(
            out.underruns, 0,
            "a {} c frame at fed position {} must not starve the ring (consumed {}/{})",
            ROM_WORST_WORK, at, out.consumed, total
        );
        assert_eq!(
            out.consumed, total,
            "the ledger must close at position {at}"
        );
        assert!(
            out.min_lead >= geo::CHUNK_SAMPLES,
            "the lead must stay above a period at position {at}"
        );
    }
    println!(
        "model/burst: {} c charged at every one of the {} fed-phase positions -> underruns=0, lead >= one period at every position",
        ROM_WORST_WORK,
        frames.len() - prefill_frames
    );
}

/// The floor the ROM observed, derived — and the reason the flow control asks for
/// the *declared* blocksize rather than the frame it is about to decode.
#[test]
fn floor_matches_the_roms_watermark() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();
    let out = run::<{ geo::RING_SAMPLES }>(&Settings::shipped(MEASURED), &frames, total);
    println!(
        "model/floor: predicted min_lead={} max_lead={}; the current image's run observed min_lead={} max_lead={}",
        out.min_lead, out.max_lead, ROM_OBSERVED_MIN_LEAD, ROM_OBSERVED_MAX_LEAD
    );
    // The floor, derived rather than quoted. The prefill cannot land on its target:
    // it stops after the push that *crosses* it, so its depth is
    // `ceil(target / frame_len) * frame_len`. From there, one chunk leaves the ring
    // into the prime and one more is taken by the first period before the first
    // fed-phase push can happen (the producer cannot start a frame before its
    // credit arrives, and one period's worth is not enough for a frame that costs
    // several). That is the whole of the arithmetic, and it comes out at the number
    // the ROM measured — sample for sample, from an independent implementation.
    let block = usize::from(manifest.blocksize());
    let prefill_actual = (geo::PREFILL_SAMPLES + block - 1) / block * block;
    // At the post-retarget cost one period's credit is enough to start a frame
    // before the first period's chunk is taken, so the floor is the prefill less
    // the prime; at a slower cost the first take happens first and it is one chunk
    // lower. The model decides which, and the assertion below is against its answer.
    let predicted = prefill_actual - geo::CHUNK_SAMPLES;
    println!(
        "model/floor: prefill target {} -> actual {} (rounded up to whole frames of {}); less the prime = {}",
        geo::PREFILL_SAMPLES,
        prefill_actual,
        block,
        predicted
    );
    assert_eq!(
        prefill_actual, 10_240,
        "the ROM's own log says it prefilled 10,240; the model derives the same number from the target and the frame length"
    );
    assert_eq!(
        out.min_lead, predicted,
        "the floor is the quantised prefill less the prime; anything else means the model and the ROM disagree about flow control"
    );
    assert_eq!(
        out.min_lead, ROM_OBSERVED_MIN_LEAD,
        "the model predicts the ROM's measured watermark exactly — this is the test that says the model and the ROM are the same machine"
    );
    // The cap on the lead is the flow control's own guarantee inverted: room for a
    // whole frame was demanded before the frame was decoded, nothing is consumed in
    // between, so the lead after a push cannot exceed the ring. The ROM's observed
    // 19,600 sits one period below that ceiling, which is what a ring being drained
    // concurrently looks like.
    assert!(out.max_lead <= geo::RING_SAMPLES);
    assert!(
        out.max_lead + geo::CHUNK_SAMPLES >= ROM_OBSERVED_MAX_LEAD,
        "the model's cap and the ROM's observed peak must agree to within one period of clock granularity"
    );
}

/// Why the flow control's reserve is the *declared* blocksize rather than the
/// length of the frame about to be decoded: at the instant the decision has to be
/// made, that length does not exist yet. The player needs a bound it can obtain
/// before decoding, and the only such bound is the blocksize the stream declares —
/// which is a bound precisely because the encode profile enumerates the blocksizes
/// it will accept.
///
/// So this is the test that the bound the producer relies on is a real bound: the
/// profile admits the asset's blocksize, the asset's frames never exceed it, and
/// the ring is deep enough that holding one frame's worth in reserve still leaves a
/// period's worth to drain.
#[test]
fn flow_control_floor_is_the_declared_blocksize() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let block = usize::from(manifest.blocksize());
    let frames = decoded_frames(&manifest);

    assert!(
        flac_lite::format::DEFAULT_PROFILE
            .allowed_blocksizes
            .contains(&(block as u16)),
        "the profile must enumerate the blocksize the manifest declares, or the declared blocksize is not a bound the player may rely on"
    );
    assert!(
        frames.iter().all(|(l, _)| l.len() <= block),
        "no decoded frame may exceed the declared blocksize — that inequality is the whole reason a pre-decode room check can be honoured instead of hoped for"
    );
    assert!(
        geo::RING_SAMPLES >= 2 * block,
        "holding one frame in reserve must still leave the ring able to hold another, or the check can never be satisfied twice in a row"
    );
    assert!(
        block > geo::CHUNK_SAMPLES,
        "at this rate a frame is {} samples and a period is {} — which is exactly why a producer's stall is worth several periods of audio and the ring must be several frames deep",
        block,
        geo::CHUNK_SAMPLES
    );
    println!(
        "model/flow-control: reserve={} samples (the declared blocksize); profile admits it; all {} frames obey it",
        block,
        frames.len()
    );
}

// ---------------------------------------------------------------------------
// 3. what the geometry is actually worth
// ---------------------------------------------------------------------------

/// The largest producer cost, in per mille of `base`, that survives the whole of
/// `frames` with no underrun. Bisection in integers, so no float enters the search.
///
/// It is a function rather than three hand-copied loops because the interesting
/// comparison is between *scenarios* — measured, derated, and a clip twice as long —
/// and hand-copying is how the earlier version of this test ended up printing a
/// ceiling it had truncated with integer division.
fn tolerate_per_mille(frames: &[FrameBytes], total: usize, base: u64) -> u64 {
    let survives = |work: u64| -> bool {
        let out = run::<{ geo::RING_SAMPLES }>(&Settings::shipped(work), frames, total);
        out.underruns == 0 && out.consumed == total
    };
    let mut lo: u64 = 1_000;
    let mut hi: u64 = 1_000;
    while survives(base * hi / 1000) {
        lo = hi;
        hi *= 2;
        assert!(
            hi < 100_000,
            "the search ran away: the geometry is more forgiving than any cost model we would believe"
        );
    }
    while hi - lo > 10 {
        let mid = (lo + hi) / 2;
        if survives(base * mid / 1000) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// How much more the producer can cost before an underrun appears — and the two
/// different quantities that answer for two different clip lengths.
///
/// This is the number a wrong clock assumption, a slower part, or a profile
/// relaxation would spend, and it is not the same number as the burst tolerance in
/// [`burst_stall_tolerance`]. The mechanism is throughput, and the arithmetic is a
/// fraction: a frame of audio is `frame_cycles / PERIOD_CYCLES` = **3.657** periods of
/// the mixer clock (not 3 — the fraction is the whole point), so a producer that keeps
/// up forever may spend `(PERIOD_CYCLES − feeder) × 3.657` cycles per frame, i.e.
/// **186.5%** of the measured cost. That ceiling is a property of the clock; **no ring
/// depth can move it**, because a bounded buffer cannot pay a permanent deficit.
///
/// What the ring can buy is *time* — its lead, spent once. On the shipped 9.906 s clip
/// that is worth a few points of overage above the ceiling; double the clip and it is
/// worth less, because the lead is fixed and the deficit is not. This test measures the
/// first and the trend of the last. An earlier version divided `frame_cycles` by
/// `PERIOD_CYCLES` as integers, quoted the truncated 3-period bound as 152.9%, and so
/// reported the ring's one-off lead as "~38 points of sustained overage".
#[test]
fn sustained_overage_tolerance() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();

    // The ceiling that needs no simulation, computed as the fraction it is. Multiply
    // before dividing: `frame_cycles / PERIOD_CYCLES` truncated to 3 periods per frame
    // is 18% of a frame's cycles thrown away, which is exactly the error the first
    // version made.
    let throughput_bound = (u64::from(geo::PERIOD_CYCLES) - FEEDER_PER_PERIOD)
        * u64::from(geo::frame_cycles(2048))
        / u64::from(geo::PERIOD_CYCLES);
    let ceiling = throughput_bound * 1000 / MEASURED;
    println!(
        "model/sustained: throughput-only ceiling (no ring in the picture, and no depth can move it) = {} c/frame = {} per mille of the measured {}; a frame is {}/{} = 3.657 periods",
        throughput_bound,
        ceiling,
        MEASURED,
        geo::frame_cycles(2048),
        geo::PERIOD_CYCLES
    );

    let lo = tolerate_per_mille(&frames, total, MEASURED);
    // The same search at the derate, because a tolerance measured on an emulator is
    // a tolerance of the emulator.
    let dlo = tolerate_per_mille(&frames, total, Settings::derate(MEASURED));
    println!(
        "model/sustained: at the shipped clip the measured cost survives up to {} per mille over budget ({} points of the measured cost: {} per mille is the clock's ceiling, {} per mille is the ring's lead being spent); at the hardware derate, up to {} per mille over the derated cost",
        lo,
        (lo - 1000) as f64 / 10.0,
        ceiling,
        lo.saturating_sub(ceiling),
        dlo
    );

    // The same search on a clip twice as long: the ring's contribution must shrink,
    // because it is a fixed number of cycles of lead covering a deficit that grows with
    // the clip. A bounded ring buys time, not throughput — if this ever stops falling,
    // the model is claiming the second while measuring the first.
    let long_frames: Vec<FrameBytes> = frames.iter().chain(frames.iter()).cloned().collect();
    let long_total: usize = long_frames.iter().map(|(l, _)| l.len()).sum();
    let llo = tolerate_per_mille(&long_frames, long_total, MEASURED);
    println!(
        "model/sustained: at twice the clip ({:.2} s) the same search lands at {} per mille — {} per mille of the ring's help spent, and the trend is the arithmetic: the lead is fixed, the deficit is not",
        long_total as f64 / f64::from(geo::PLAYBACK_RATE_HZ),
        llo,
        lo - llo
    );

    assert!(
        lo >= 1_500,
        "the shipped geometry must survive at least a 50% overage of the measured producer cost; found {} per mille",
        lo
    );
    assert!(
        lo >= ceiling,
        "a ring can never do worse than the clock's own throughput ceiling — if this fails the model and the ceiling are describing different players"
    );
    assert!(
        llo < lo,
        "the ring's purchase is time, not throughput: a doubled clip must tolerate strictly less overage (1x = {} per mille, 2x = {} per mille)",
        lo,
        llo
    );
    assert!(
        llo >= ceiling,
        "even a doubled clip cannot fall below the clock's ceiling ({} per mille); found {}",
        ceiling,
        llo
    );
    assert!(
        dlo >= 1_400,
        "the same claim has to survive the hardware derate with at least a 40% sustained headroom; found {}",
        dlo
    );
}

/// The other tolerance: the producer stops entirely. Then the lead in hand is the
/// whole answer, and it is what a *transient* costs — one very expensive frame, one
/// cold cache, one long interrupt storm.
///
/// The producer is given a cost no period can pay (four times a 64-bit period's
/// worth), so after the prefill it can never start another frame — and the credit
/// accumulating across periods is exactly what an earlier draft of this test forgot,
/// when it used one period's cycles as the cost and found the producer keeping up
/// forever. What the run then measures is the prefill's worth in periods, and the
/// gap between that and the ring's nominal depth is the point: the flow control
/// holds one frame's worth back, so the *reachable* lead is not the ring's depth,
/// and a document that quotes 36 periods of stall would be quoting a number this
/// model can show is never available.
#[test]
fn burst_stall_tolerance() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();
    let settings = Settings {
        work_per_frame: u64::MAX / 4,
        feeder_per_period: FEEDER_PER_PERIOD,
        prefill_chunks: geo::PREFILL_CHUNKS,
        burst: None,
    };
    let out = run::<{ geo::RING_SAMPLES }>(&settings, &frames, total);
    let periods_the_prefill_carries = geo::PREFILL_SAMPLES / geo::CHUNK_SAMPLES;
    println!(
        "model/burst: producer unable to start another frame -> first starvation at period {:?}, underruns={}, consumed={}/{}; the prefill of {} samples carries {} periods on its own, and the deepest lead at which this geometry will still *start* a frame is {} periods (a frame already started pushes into that reserve, which is why the peaks actually measured run higher)",
        out.first_starve,
        out.underruns,
        out.consumed,
        total,
        geo::PREFILL_SAMPLES,
        periods_the_prefill_carries,
        (geo::RING_SAMPLES - usize::from(manifest.blocksize())) / geo::CHUNK_SAMPLES
    );
    assert!(
        out.underruns > 0,
        "a producer that cannot start a frame must eventually starve the consumer — a model in which it does not is not modelling the clock"
    );
    let block = usize::from(manifest.blocksize());
    let prefill_actual = (geo::PREFILL_SAMPLES + block - 1) / block * block;
    let carried = (prefill_actual - geo::CHUNK_SAMPLES) / geo::CHUNK_SAMPLES;
    assert_eq!(
        out.first_starve,
        Some(u64::try_from(carried + 1).unwrap()),
        "a stall is survivable exactly as long as the lead in hand lasts: the quantised prefill of {} less the prime of {} is {} periods of audio, and the {}th period is the first with nothing to take",
        prefill_actual,
        geo::CHUNK_SAMPLES,
        carried,
        carried + 1
    );
    assert!(
        carried <= geo::RING_CHUNKS,
        "nothing survives a stall longer than the ring's depth, so the model must not invent one that does"
    );
    // The ceiling on *starting* a frame: room for a whole frame is demanded before
    // the frame is decoded, so at the instant of that decision at least one frame's
    // worth of the ring is not lead. A frame that was already allowed to start may
    // then push into that reserve, so the peak lead runs higher than this gate — the
    // model's own maximum, and the ROM's, are the evidence; this arithmetic is only
    // the gate, and the assert below allows exactly that much overshoot.
    let reachable = (geo::RING_SAMPLES - usize::from(manifest.blocksize())) / geo::CHUNK_SAMPLES;
    let model_reachable = out.max_lead / geo::CHUNK_SAMPLES;
    assert!(
        model_reachable <= reachable + 1,
        "the lead the model actually reached ({}) must not exceed the ceiling the flow control implies ({})",
        model_reachable,
        reachable
    );
    // Depth in samples and depth in periods are the same number at this rate, which
    // is why "how deep is the ring" and "how long may the producer stall" cannot be
    // answered as two questions.
    assert_eq!(
        geo::RING_SAMPLES / geo::CHUNK_SAMPLES,
        geo::RING_CHUNKS,
        "the ring's depth in periods is its depth in samples divided by what a period takes"
    );
}

/// What the geometry would have to be if it were sized rather than chosen — the
/// "buffer depth as a function of producer cost" the plan asks for, as two scans of
/// real `RingFeed<R>` instances and real prefills rather than a paragraph.
///
/// The cost is the measured one plus the hardware derate — a cost the
/// [`zero_underflow_at_the_measured_cost_and_at_the_derate`] test has already shown
/// the shipped geometry carries with zero underruns, which is what makes the depth
/// question meaningful at all. Ask instead for a cost past the sustained bound and
/// every depth fails by the same amount, because depth is not the term that binds
/// in that regime; [`sustained_overage_tolerance`] measures that regime, and this
/// one is the regime where the two knobs actually matter.
///
/// Two numbers come out, and they are the two the geometry's comments currently
/// assert without evidence:
///
/// - the depth cliff: how shallow the ring could be at this cost, given the shipped
///   prefill. The first rows are *infeasible* rather than failing — with a prefill
///   of 16 chunks and a one-frame reserve held back by the flow control, a ring
///   shallower than `prefill + frame` cannot finish prefilling, so the prefill
///   choice puts a floor under the depth choice before any cost is considered;
/// - the prefill cliff: how little prefill the same depth needs. This is the knob
///   that costs boot time rather than RAM, so its cliff is the cheap end of the
///   trade, and the reason the shipped prefill is not the number worth arguing
///   about.
#[test]
fn sizing_scans_for_both_geometry_knobs() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();
    let block = usize::from(manifest.blocksize());
    let work = Settings::derate(MEASURED);
    println!(
        "model/sizing: cost = {} c/frame ({}% of the measured cost: the derate). A ring shallower than prefill {} + a frame's reserve {} = {} samples cannot prefill at all.",
        work,
        work * 100 / MEASURED,
        geo::PREFILL_SAMPLES,
        block,
        geo::PREFILL_SAMPLES + block
    );

    let mut survived_depth: Vec<usize> = Vec::new();
    macro_rules! probe_depth {
        ($chunks:expr) => {{
            const C: usize = $chunks;
            if C * geo::CHUNK_SAMPLES < geo::PREFILL_SAMPLES + block {
                println!(
                    "model/sizing: depth scan ring={:>2} chunks ({:>6} samples) -> infeasible at this prefill",
                    C,
                    C * geo::CHUNK_SAMPLES
                );
            } else {
                let out = run::<{ C * geo::CHUNK_SAMPLES }>(
                    &Settings {
                        work_per_frame: work,
                        feeder_per_period: FEEDER_PER_PERIOD,
                        prefill_chunks: geo::PREFILL_CHUNKS,
                        burst: None,
                    },
                    &frames,
                    total,
                );
                println!(
                    "model/sizing: depth scan ring={:>2} chunks ({:>6} samples) -> underruns={:>3} first_starve={:?} consumed={:>7}/{} min_lead={:>5}",
                    C,
                    C * geo::CHUNK_SAMPLES,
                    out.underruns,
                    out.first_starve,
                    out.consumed,
                    total,
                    out.min_lead
                );
                if out.underruns == 0 && out.consumed == total {
                    survived_depth.push(C);
                }
            }
        }};
    }
    probe_depth!(16);
    probe_depth!(18);
    probe_depth!(20);
    probe_depth!(24);
    probe_depth!(28);
    probe_depth!(32);
    probe_depth!(geo::RING_CHUNKS);

    let mut survived_prefill: Vec<usize> = Vec::new();
    macro_rules! probe_prefill {
        ($chunks:expr) => {{
            const P: usize = $chunks;
            let out = run::<{ geo::RING_SAMPLES }>(
                &Settings {
                    work_per_frame: work,
                    feeder_per_period: FEEDER_PER_PERIOD,
                    prefill_chunks: P,
                    burst: None,
                },
                &frames,
                total,
            );
            println!(
                "model/sizing: prefill scan prefill={:>2} chunks ({:>5} samples) -> underruns={:>3} first_starve={:?} consumed={:>7}/{} min_lead={:>5}",
                P,
                P * geo::CHUNK_SAMPLES,
                out.underruns,
                out.first_starve,
                out.consumed,
                total,
                out.min_lead
            );
            if out.underruns == 0 && out.consumed == total {
                survived_prefill.push(P);
            }
        }};
    }
    probe_prefill!(1);
    probe_prefill!(2);
    probe_prefill!(4);
    probe_prefill!(8);
    probe_prefill!(geo::PREFILL_CHUNKS);

    println!(
        "model/sizing: at this cost the depth cliff is at {:?} chunks (shipped {}) and the prefill cliff at {:?} chunks (shipped {})",
        survived_depth.first(),
        geo::RING_CHUNKS,
        survived_prefill.first(),
        geo::PREFILL_CHUNKS
    );

    assert!(
        survived_depth.contains(&geo::RING_CHUNKS),
        "the shipped ring must carry the measured cost at the hardware derate"
    );
    assert!(
        survived_prefill.contains(&geo::PREFILL_CHUNKS),
        "the shipped prefill must carry the measured cost at the hardware derate"
    );
    // Monotonicity in both knobs. It is not a property of the hardware; it is a
    // guard on the model — if a deeper ring or a deeper prefill failed where a
    // shallower one passed, this file would be measuring something other than what
    // its names say, and every other number here would be suspect.
    for w in survived_depth.windows(2) {
        assert!(
            w[0] < w[1],
            "survival must be monotonic in ring depth: {} then {}",
            w[0],
            w[1]
        );
    }
    for w in survived_prefill.windows(2) {
        assert!(
            w[0] < w[1],
            "survival must be monotonic in prefill depth: {} then {}",
            w[0],
            w[1]
        );
    }
}

/// The stream rates a **product** asset may declare: exactly one, and it is the
/// clock everything else in this path uses. Until plan item 7-0 this list named no
/// product rate at all, because the shipped asset needed a mapping to be played; it
/// now states the end of the rule rather than the interim. Written as an allow-list
/// rather than a `!=` so that widening it is a decision somebody has to make in one
/// place, with a red test next to it.
const PRODUCT_STREAM_RATES: &[u32] = &[geo::PLAYBACK_RATE_HZ];

/// The one bridge that was ever carried here, as `(declared, played)` — kept
/// whether or not the tree still carries it.
///
/// Deliberately a literal rather than a read of `geometry::RATE_BRIDGE`, which is
/// `None`: the cents below are a fact about the asset that was measured on
/// 2026-11-11, and the pin has to outlive the deletion of the mapping, or the
/// number that says *why* the ruling was made would disappear with the arm. It is a
/// historical record, not a permitted pair: the assert below demands the tree refuse
/// this exact declaration outright.
const RECORDED_BRIDGE: (u32, u32) = (32_000, RECORDED_PRODUCT_CLOCK);

/// The product clock the recorded bridge played on: the 32,768 Hz rung the
/// 2026-11-11 hardware runs were taken at. Kept as a literal because the current
/// tree plays at 16,384 Hz; using `geo::PLAYBACK_RATE_HZ` here would silently
/// re-point a historical 41.06-cent calculation at a different clock.
const RECORDED_PRODUCT_CLOCK: u32 = 32_768;

/// Q14, **ruled** by the project owner on 2026-11-11 and **landed by plan item
/// 7-0**: a stream is encoded at the rate it is played at, and the player does not
/// reinterpret a declared rate to fit a clock. What that policy is made of here, in
/// one test:
///
/// * the identity case, and the identity case alone, is accepted — including every
///   rate that looks arithmetically tempting, which are refused by name;
/// * the tree carries **no** bridge, and if a future edit adds one it must be
///   read from `geometry::RATE_BRIDGE` rather than inferred from the mapping, so
///   the constant and the function cannot drift apart; a *second* pair is a second
///   pitch shift, which the ruling forbids on its size, not its direction;
/// * the size of what the removed mapping cost is pinned in cents, computed here
///   rather than quoted from prose, and cross-checked against the pitch witness
///   (`pitch_witness_*` below) — which is the instrument that would have caught it,
///   and which is the reason this test no longer has to be the only one holding the
///   line;
/// * and the asset in this tree is **on** the rule, asserted as a fact: it declares
///   the product clock, it needs no reinterpretation, and the allow-list covers it;
/// * and the crate-side half of the rule — the rate clause `EncodeProfile` gained in
///   7-0, which every build that validates a manifest reads — names that same one
///   rate and no other. The profile and the mixer are one fact in two crates, so
///   their equality is asserted here rather than left to a start-up failure on
///   hardware.
#[test]
fn ruled_rate_policy_allows_no_reinterpretation_of_a_declared_rate() {
    // The product arm: a stream that declares the target rate is played at it.
    assert_eq!(
        geo::playback_rate_for_stream(geo::PLAYBACK_RATE_HZ),
        Some(geo::PLAYBACK_RATE_HZ),
        "the identity mapping is the only one a product asset may take"
    );

    // Every other rate a FLAC stream can declare: refused, not reinterpreted. This
    // list is the guard — a `Some(_)` appearing in it is a new pitch shift being
    // waved through by a future edit, and Q14 has ruled those out.
    //
    // 8,192 / 16,384 / 65,536 are sampled deliberately. An exact cycles/sample figure
    // is a tempting reason to add a rate — it is the property that makes 32,768 the only
    // agb clock whose period costs exactly 512 c/sample — and the rates that share it are
    // exactly the ones dividing 2^24, i.e. powers of two: in the declarable band those
    // are 8,192 (2,048 c/sample), 16,384 (1,024) and 65,536 (256). All three are
    // arithmetically clean and all three are still refused: Q14's rule is about the
    // declared rate, not about which division happens to be exact.
    for rate in [
        8_000u32, 8_192, 11_025, 16_000, 18_157, 22_050, 24_000, 26_000, 31_000, 32_001, 32_768,
        33_000, 44_100, 48_000, 65_536,
    ] {
        assert_eq!(
            geo::playback_rate_for_stream(rate),
            None,
            "{rate} Hz must be refused by the player, not mapped onto the timer"
        );
    }

    // The bridge that used to be here, and its price, computed from the *record*
    // rather than from the tree, so the pin is still checked now that the arm is
    // gone.
    let (declared, played) = RECORDED_BRIDGE;
    let ratio = f64::from(played) / f64::from(declared);
    let cents = 1200.0 * ratio.log2();
    println!(
        "q14: the bridge that was removed here played a {} Hz stream at {} Hz = {}x = {:.2} cents sharp. \
         Ruled out as a product path 2026-11-11; plan item 7-0 removed it and re-encoded \
         the asset at {} Hz; 7-1 owns the hardware row measured on those bytes.",
        declared,
        played,
        ratio,
        cents,
        geo::PLAYBACK_RATE_HZ
    );
    assert!(
        (cents - 41.06).abs() < 0.01,
        "1.024x is 41.06 cents — pinned so that no future record can call it a rounding \
         error or a semitone; it is neither, and it is disqualifying"
    );
    // The same number from the other direction. The pitch witness measures a played
    // signal's period and turns it into hertz; this test computes cents from two
    // rate fields. If they ever disagree, one of the two is measuring the wrong
    // thing, and the disagreement is the thing 7-3's human ear would notice.
    assert!(
        (cents - pitch_cents_of(RECORDED_BRIDGE.0, RECORDED_BRIDGE.1)).abs() < 1e-9,
        "the rate arithmetic here and the pitch instrument's cents must be the same number"
    );

    // The tree may carry at most this one bridge, and if it carries one it must be this
    // pair — a second pair would be a second pitch shift, which is the thing the ruling
    // forbids. With no bridge in the tree, the recorded declared rate must be refused
    // outright: that is "identity-or-refusal" as code, and it is 7-0's end state.
    let bridge = geo::RATE_BRIDGE;
    match bridge {
        Some(pair) => {
            assert_eq!(
                pair, RECORDED_BRIDGE,
                "the only reinterpretation on record is 32,000 -> 32,768; a second pair is \
                 a second pitch shift, and Q14 refuses those on their size, not their direction"
            );
            assert_eq!(
                geo::playback_rate_for_stream(pair.0),
                Some(pair.1),
                "`RATE_BRIDGE` must be the mapping the player actually applies — a constant \
                 that disagrees with the function is a guard reading the wrong tree"
            );
        }
        None => assert_eq!(
            geo::playback_rate_for_stream(declared),
            None,
            "with no bridge in the tree, a {} Hz stream must be refused rather than mapped",
            declared
        ),
    }

    // And the disqualification/ascent is about the asset in this tree, not an
    // abstract one. The single condition is kept because it is what makes a
    // half-landed 7-0 impossible to miss: an asset may be played exactly when no
    // reinterpretation of its declared rate is needed to play it. On top of it sit
    // the two absolute claims, which only hold in the completed state — the shipped
    // blob is on the rule, and the tree carries no mapping.
    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    let declared_allowed = PRODUCT_STREAM_RATES.contains(&manifest.sample_rate_hz());
    assert_eq!(
        declared_allowed,
        bridge.is_none(),
        "the asset shipped in this tree declares {} Hz; the product allow-list {} it and \
         the rate bridge {} — an asset may be played only where no reinterpretation of \
         its declared rate is needed to play it (Q14)",
        manifest.sample_rate_hz(),
        if declared_allowed {
            "allows"
        } else {
            "refuses"
        },
        if bridge.is_some() {
            "is still present"
        } else {
            "is gone"
        },
    );
    assert!(
        bridge.is_none(),
        "the product path carries no rate reinterpretation at all (plan item 7-0); \
         `geometry::RATE_BRIDGE` is back, and with it the 41 cents"
    );
    assert_eq!(
        manifest.sample_rate_hz(),
        geo::PLAYBACK_RATE_HZ,
        "the product asset in `assets/` must declare the clock its costs are stated in"
    );

    // The crate's profile and this example's mixer are one fact stated in two
    // crates, and this is the only place either half can be compared against the
    // other. The ROM plays at `geo::PLAYBACK_RATE_HZ` (which mirrors
    // `agb::mixer::SoundDriver::SAMPLE_RATE`) and holds its asset to
    // `flac_lite::format::DEFAULT_PROFILE`; if those two ever named different
    // rates, the product image would refuse its own blob at start-up — so the drift
    // is asserted here, on the host, rather than discovered on hardware in 7-1.
    assert_eq!(
        flac_lite::format::DEFAULT_PROFILE.allowed_sample_rates_hz,
        [flac_lite::format::PRODUCT_SAMPLE_RATE_HZ],
        "the product profile names exactly one rate, and states it once"
    );
    assert_eq!(
        flac_lite::format::PRODUCT_SAMPLE_RATE_HZ,
        geo::PLAYBACK_RATE_HZ,
        "`EncodeProfile::allowed_sample_rates_hz` and the mixer's clock are the same \
         fact: the profile is 7-0's crate-side half of the rate contract, and a \
         profile that named another rate would have the ROM refuse the asset it ships"
    );
    assert!(
        !flac_lite::format::DEFAULT_PROFILE
            .allowed_sample_rates_hz
            .contains(&declared),
        "the rate on the record as the removed bridge's declared half ({declared} Hz) \
         must stay outside the profile — identity-or-refusal, seen from the crate"
    );
    assert!(
        flac_lite::format::DEFAULT_PROFILE
            .validate(&manifest)
            .is_ok(),
        "the shipped blob passes the profile the ROM validates it with — the same \
         call `Decoder::new` makes under `strict-profile`"
    );
}
// ---------------------------------------------------------------------------
// PITCH — the one thing the pinned digests cannot see
// ---------------------------------------------------------------------------

// `fifo_pins_match_the_converted_stream` pins the exact bytes every frame
// produces, and `gafp_pin`'s constants pin the whole stream. Neither can see Q14.
// A stream made at 32,000 Hz and played on a 32,768 Hz clock produces
// **byte-identical output** — the defect is not in the samples, it is in what the
// sample axis means, and 6b-3 proved that on hardware: its run was 571/571 periods
// with zero underflows, its digests matched, and it was 41 cents sharp. A run that
// passes every digest and is out of tune is exactly the kind of green this project
// has already paid for once (`method-and-gates.md`'s false-green catalogue), so the
// pitch question needs its own instrument.
//
// It belongs here, in 7-0, rather than in 7-3 for a practical reason: 7-3 asks a
// person to say "in tune and in time", and a claim carried by one unrepeatable
// opinion cannot be re-checked by a later run. The machine half of the same
// question — *do the decoded samples carry the pitch their own design intends, at
// the clock this player runs?* — has to exist before the hardware run, or there is
// nothing for the human's answer to be compared against. And it is the half that
// can be run on every commit.
//
// **The instrument: a matched filter.** Given a waveform the *design* declares, the
// least-squares fit of that waveform inside the measured samples is the amplitude
// that waveform has in the samples, in the samples' own units. Everything up to the
// final division is integer (`i128` sums), there is no crate, no FFT window, and no
// float state that could drift. One property is why this form and not a transform:
// 326,144 samples is not a whole number of periods at 256 **or** at 250, so a
// length-commensurate transform would have smeared exactly the difference it is
// being asked to resolve. The filter is calibrated against the term as the
// generator actually emits it — partial cycle at the end included — and the decode
// is lossless, so "calibrated against the design" and "measured on the decoded
// stream" are the same measurement taken from opposite ends. That is why their
// agreement is evidence rather than a tautology.
//
// **The anchor.** The filter needs a frequency the design declares *and* that both
// clocks can spell exactly, or the two hypotheses are not separable.
// `32,768 = 2^15` and `32,000 = 2^8 · 125`, so their `gcd` is 256 and the only
// hertz with an exact integer period on both are divisors of 256. 128 Hz is 256
// samples at the product clock and 250 at the old one — a 2.4% period offset, which
// is the Q14 offset itself, and it is the one frequency both design tables declare
// at the same weight: `scripts/frame_vectors.py`'s current `PARTIALS_DEVICE` carries
// `(256, 3)` where the 32,000-era table carried `(250, 3)`. 32 Hz is arithmetically
// usable and was tried and dropped on measurement rather than on taste: its energy
// sits in the **side** band, which is where this fixture puts its noise, and a period
// one sample off answers a large part of what the instrument is being asked. The
// side-band probe in the positive leg measures that; it is why the anchor is 128 Hz
// in the centre band and not 32 Hz in the side band.
//
// **The band.** The generator's pitched material is in the centre (`L = c + s`,
// `R = c − s`), so `(L+R)>>1` keeps it and halves away the side band's noise.
// Arithmetic shift, floor, so this file and the Python generator cannot drift apart
// on a negative odd sample.
//
// **What "resolved" means here, and why a bare ratio is not enough.** Each reading
// answers at four periods: the two candidates, and at each candidate's own offset
// from the other — period 262 for the product candidate, 244 for the recorded one.
// Those controls sit at the *same* period distance as the hypothesis separation, so
// they ask the only question that matters about a peak: is it a peak, or is it the
// shoulder of something else? The figures quoted below are measured on this run's
// decode rather than assumed: a clean design term answers 410:1 between the
// hypotheses and 212:1 over its own shoulders, and the same term authored at the
// *other* clock answers 413:1 the other way — which is why the rule compares the two
// answers instead of thresholding either one. The shipped asset answers 340:1 and
// 209:1; its played 8-bit form still answers 66:1 and 20:1. The weakest observation
// in this file is that 20:1, and both thresholds sit at 10.
//
// The first version of this block defined its floor as "every other period in a
// window around the candidates" and the shipped asset failed it at 1.9:1 — for a
// correct reason, now probed rather than remembered: the fixture's per-channel hash
// noise is a ramp of period 801 whose third harmonic (period 267) is a real component
// of the centre band — it answers 1.57 counts there against the anchor's 2.92 — and
// any window wide enough to hold a shoulder holds it.
// Reading that as noise was the mistake. Both rejected alternatives — the window floor
// and the 32 Hz anchor — are re-measured in the positive leg below, because an
// instrument whose noise model is wrong in one place is wrong everywhere and a claim
// left in prose is a claim a later change voids without noticing.

/// The clock the 6b-3 asset was made at, and the clock of the negative leg below. A
/// record, kept for the same reason as [`RECORDED_BRIDGE`]: the tree no longer
/// encodes at it, and the number has to outlive the change so the reason for the
/// ruling stays checkable.
const RECORDED_OLD_CLOCK: u32 = 32_000;

/// The anchor frequency, and the weight the design gives it. The section header
/// above is why this frequency and no other; the weight is
/// `scripts/frame_vectors.py`'s `(256, 3)` centre partial, which is what the filter
/// is calibrated against.
const ANCHOR_HZ: u32 = 128;
const ANCHOR_WEIGHT: i32 = 3;

/// The pre-7-0 centre table as `scripts/frame_vectors.py` declared it at 32,000 Hz,
/// before plan item 7-0 authored the product fixtures at the device rate. A record,
/// not a fixture: the negative leg below synthesises it, so that leg is *the same
/// design at the old clock* rather than an invented signal — the identical 128 Hz at
/// 250 samples instead of 256, with nothing else about the design changed. The side,
/// square and noise parameters go with it for the same reason: a half-copied design
/// would change what the noise is doing, and the noise is what the controls below
/// exist to discount.
const RECORDED_CENTRE_AT_32000: &[(usize, i32)] = &[(128, 5), (192, 4), (250, 3), (80, 2)];
const RECORDED_SIDE_AT_32000: (usize, i32) = (1000, 3);
const RECORDED_SQUARE_AT_32000: usize = 32;
/// The spans `scripts/frame_vectors.py::synth_pcm` mixes its two hash noises at —
/// named for where they actually land rather than for the script's parameter names,
/// which do not say: the signature calls them `noise_l, noise_r`, but the call site
/// passes `700` into the *side* term's noise and `400` into each channel's, so
/// `noise_l` is not left-channel noise and copying the names across would have put a
/// ±700 ramp in the centre band this instrument reads. Measured on the 400-span
/// pair, whose ramps have period 801 (7919/104729/15485863 are all coprime to the
/// moduli 1401 and 801): a period-267 filter — the third harmonic of the 801-sample
/// ramp, inside any window that contains a shoulder — answers **1.57 counts** on the
/// shipped asset's centre band, over half the anchor's own 2.92, where the
/// offset-matched shoulders answer 0.014. That is what a component of the signal looks
/// like to an instrument that was told to treat everything off-candidate as noise, and
/// it is why the floor in this file is defined by period distance rather than by
/// "anywhere else".
const NOISE_SPAN_SIDE: usize = 700;
const NOISE_SPAN_CHANNEL: usize = 400;
/// The multipliers `synth_pcm` mixes noise with, named for the same reason: one of
/// them is load-bearing in a way nobody predicted (see [`NOISE_SPAN_SIDE`]).
const NOISE_MULT_SIDE: usize = 7919;
const NOISE_MULT_L: usize = 104729;
const NOISE_MULT_R: usize = 15485863;

/// One FIFO step in i16 counts. The conversion rule is `sample >> 8` as u8
/// (`playback::fifo_byte`) and `agb` reads the byte back signed (`ldrsb`), so a
/// step is exactly 256 counts and a component smaller than that cannot set a step on
/// its own — which is what "below the DAC's own floor" means, numerically.
const FIFO_STEP_COUNTS: f64 = 256.0;

/// Cents between the clock a stream was made at and the clock it is played at:
/// `1200 · log2(played / made)`. One implementation on purpose — the rate guard
/// computes this from two header fields and the instrument below computes the same
/// number from measured sample periods, and if those two ever disagree, one of them
/// is measuring the wrong thing.
fn pitch_cents_of(made_at_hz: u32, played_at_hz: u32) -> f64 {
    (f64::from(played_at_hz) / f64::from(made_at_hz)).log2() * 1200.0
}

/// The generator's triangle, exactly as `scripts/frame_vectors.py::tri` defines it.
/// Copied, not approximated: the claim under test is that a design term is present
/// at its declared amplitude inside the decoded samples, and that comparison is only
/// evidence if both sides run the same arithmetic. Integer only, for the same reason
/// the script gives — the numbers below must not depend on the host.
fn tri(index: usize, period: usize) -> i32 {
    let x = index % period;
    let half = period / 2;
    if x < half {
        (2000 * x as i32) / half as i32 - 1000
    } else {
        1000 - (2000 * (x - half) as i32) / half as i32
    }
}

/// `scripts/frame_vectors.py::square` and `::hash_noise`, same reason.
fn square(index: usize, period: usize) -> i32 {
    if (index % period) * 100 < period * 50 {
        1000
    } else {
        -1000
    }
}

fn hash_noise(index: usize, mult: usize, span: usize) -> i32 {
    ((index * mult) % (2 * span + 1)) as i32 - span as i32
}

/// Python's `//` — floor division — because that is what the generator uses, and
/// every term below is a `weight * tri(...) // 1000`. Rust's `/` truncates toward
/// zero, which for a negative dividend is a whole count away from the generator's
/// value on every negative half-epoch: half the samples, so the error does not cancel
/// in the fit, it changes the waveform. Measured, not asserted: the first version of
/// this block used `/` and reported the design's own weight-3 anchor term as
/// **2.19 counts** — a 27% shortfall on a signal this file generated itself, from one
/// arithmetic convention. A witness that cannot recover a number it wrote down
/// itself is not a witness, so this helper is what makes the calibration at the top
/// of the pitch test mean anything.
fn fl(x: i32, d: i32) -> i32 {
    debug_assert!(d > 0, "every generator term divides by a positive 1000");
    let q = x / d;
    if x % d != 0 && x < 0 { q - 1 } else { q }
}

/// One centre-band term as the generator emits it — the thing the filter is
/// calibrated against. `weight * tri // 1000` with truncating division, as in the
/// script: that truncation is why a weight-3 term measures 2.93 rather than 3.00,
/// and the shortfall is the generator's, not the instrument's.
fn design_term(weight: i32, period: usize, count: usize) -> Vec<i32> {
    (0..count)
        .map(|i| fl(weight * tri(i, period), 1000))
        .collect()
}

/// The same term at an arbitrary peak: the loud-tone legs below, where the point is
/// that the *depth* of the FIFO is not what limits a pitch witness.
fn tone(peak: i32, period: usize, count: usize) -> Vec<i32> {
    (0..count)
        .map(|i| fl(peak * tri(i, period), 1000))
        .collect()
}

/// The pre-7-0 design, synthesised at 32,000 Hz, planar. Not decoded — it never has
/// to be: what this leg asks is what the instrument answers when the samples were
/// authored on the other clock, and the lossless-decode witnesses elsewhere (plus the
/// calibration agreement asserted below) are what cover the decode half. Same
/// arithmetic as `scripts/frame_vectors.py::synth_pcm`, clip included. It is given
/// the *same sample count* as the shipped asset on purpose: filter resolution
/// depends on record length, so both legs have to be read over the same length for
/// their ratios to be comparable.
///
/// One term of the recorded design is deliberately omitted, and the reason is a
/// retarget collision rather than a preference: the old design's 250 Hz partial has
/// period 128 at 32,000 Hz, which is exactly the current 128 Hz anchor's period at
/// 16,384 Hz. Keeping it would make the negative leg answer at `p_new` with that
/// partial (weight 5, about 4.89 counts) instead of at the old anchor (weight 3,
/// about 2.92), so the instrument would be measuring the wrong tone. Every other
/// recorded term — the 128 Hz anchor, the 166.7 Hz and 400 Hz partials, the square,
/// the side band and both noise ramps — is unchanged.
fn recorded_design_at_old_clock(count: usize) -> (Vec<i32>, Vec<i32>) {
    let mut l = Vec::with_capacity(count);
    let mut r = Vec::with_capacity(count);
    for i in 0..count {
        let mut center: i32 = RECORDED_CENTRE_AT_32000
            .iter()
            .filter(|(p, _)| *p != 128)
            .map(|(p, w)| fl(w * tri(i, *p), 1000))
            .sum();
        center += fl(2 * square(i, RECORDED_SQUARE_AT_32000), 1000);
        let (sp, sw) = RECORDED_SIDE_AT_32000;
        let side = fl(sw * tri(i, sp), 1000) + hash_noise(i, NOISE_MULT_SIDE, NOISE_SPAN_SIDE);
        l.push(
            (center + side + hash_noise(i, NOISE_MULT_L, NOISE_SPAN_CHANNEL)).clamp(-32768, 32767),
        );
        r.push(
            (center - side + hash_noise(i, NOISE_MULT_R, NOISE_SPAN_CHANNEL)).clamp(-32768, 32767),
        );
    }
    (l, r)
}

/// The centre band, recovered the way the generator's own mixing makes it
/// recoverable: with `L = c + s + n_L` and `R = c − s + n_R`, `(L+R)>>1` is `c`
/// plus half the sum of two incoherent ramps.
fn centre_band(l: &[i32], r: &[i32]) -> Vec<i32> {
    l.iter()
        .zip(r.iter())
        .map(|(a, b)| ((*a as i64 + *b as i64) >> 1) as i32)
        .collect()
}

/// The matched filter: the least-squares amplitude of `tri(·, period)` inside `x`,
/// in `x`'s own units. Exact in `i128` up to the one division — 326,144 samples of
/// a full-scale signal against a ±1000 template is 1.1e12, nine orders of magnitude
/// inside `i128` — and every magnitude is printed, so an f64 result can be checked
/// against the design's declared weight by hand.
fn matched(x: &[i32], period: usize) -> f64 {
    let mut num: i128 = 0;
    let mut den: i128 = 0;
    for (i, &v) in x.iter().enumerate() {
        let t = tri(i, period) as i128;
        num += v as i128 * t;
        den += t * t;
    }
    // `tri` peaks at 1000, so `1000 · Σx·t / Σt²` is that waveform's peak amplitude
    // in `x`'s units: a signal containing exactly `3*tri(i,256)//1000` answers 2.93,
    // and 2.3% short of the declared 3 is the generator's truncation, which the
    // calibration reproduces rather than corrects.
    1000.0 * (num as f64 / den as f64)
}

/// One reading: the response at each candidate period, and the largest response at
/// either candidate's own offset from the other. See the section header for why the
/// controls are at ±Δ and not "everywhere else in a window".
struct Pitch {
    period_product: usize,
    period_recorded: usize,
    at_product_clock: f64,
    at_recorded_clock: f64,
    /// The two offset-matched shoulders.
    control: f64,
}

impl Pitch {
    fn winner(&self) -> (usize, f64) {
        if self.at_product_clock.abs() >= self.at_recorded_clock.abs() {
            (self.period_product, self.at_product_clock.abs())
        } else {
            (self.period_recorded, self.at_recorded_clock.abs())
        }
    }

    /// How much the two hypotheses differ — the discriminator.
    fn candidate_ratio(&self) -> f64 {
        let (p, w) = self.winner();
        let loser = if p == self.period_product {
            self.at_recorded_clock.abs()
        } else {
            self.at_product_clock.abs()
        };
        w / loser.max(1e-12)
    }

    /// How much the winner beats the shoulders of its own peak — the resolvability
    /// condition, and the one that keeps a ripple from being read as a tone.
    fn resolved_ratio(&self) -> f64 {
        let (_, w) = self.winner();
        w / self.control.max(1e-12)
    }
}

/// Read the instrument. The two candidates are `Δ` apart by construction (the
/// anchor's two exact periods at the two clocks), so the control set is the
/// reflection of each candidate about the other: `p_new − Δ` and `p_new + Δ` for the
/// product candidate, and the mirror for the recorded one. For a 128 Hz anchor on
/// these clocks that is periods 244, 250, 256, 262 — four filter passes, and the
/// reason a Q14-sized pitch shift cannot hide in the instrument's own sidelobes is
/// that the only periods it could hide at are exactly the two being measured.
fn read_pitch(x: &[i32], period_product: usize, period_recorded: usize) -> Pitch {
    let delta = period_product - period_recorded;
    let mut p = Pitch {
        period_product,
        period_recorded,
        at_product_clock: 0.0,
        at_recorded_clock: 0.0,
        control: 0.0,
    };
    for period in [
        period_recorded - delta,
        period_recorded,
        period_product,
        period_product + delta,
    ] {
        let m = matched(x, period);
        if period == period_product {
            p.at_product_clock = m;
        } else if period == period_recorded {
            p.at_recorded_clock = m;
        } else if m.abs() > p.control {
            p.control = m.abs();
        }
    }
    p
}

/// The two thresholds of the naming rule, discussed and measured in the section
/// header: candidate-to-candidate and winner-to-shoulder both have to clear 10. The
/// nearness of the second one to what is observed — 209 on the shipped asset, 20 on its
/// played 8-bit form — is not the point: the 20 is only a factor of 2.0 over this line,
/// and it is the weakest thing in the file. What the line is actually for is that a
/// signal with no tone in it answers ~1:1 on both, so a ratio of 10 is a qualitative
/// rather than a tuned boundary.
const RATIO_NAMES: f64 = 10.0;
const RATIO_RESOLVED: f64 = 10.0;

/// Which clock these samples were made at, or `None` when the instrument declines to
/// say. The named period times the anchor's declared frequency *is* the clock: that
/// is the whole trick, and it is why the answer is not fed in — 256 samples per
/// period of a 128.000 Hz tone can only come from a 32,768 Hz axis, and 250 of the
/// same tone only from a 32,000 Hz one.
fn name_the_clock(p: &Pitch) -> Option<u32> {
    let (period, winner) = p.winner();
    let loser = if period == p.period_product {
        p.at_recorded_clock.abs()
    } else {
        p.at_product_clock.abs()
    };
    if winner / loser.max(1e-12) > RATIO_NAMES && winner / p.control.max(1e-12) > RATIO_RESOLVED {
        Some((period as u64 * u64::from(ANCHOR_HZ)) as u32)
    } else {
        None
    }
}

/// The periods are exact on both clocks by construction of the anchor; asserted
/// rather than assumed, because a non-exact anchor would make the two hypotheses
/// overlap and the instrument meaningless.
fn exact_period(clock_hz: u32, hz: u32) -> usize {
    assert_eq!(
        clock_hz % hz,
        0,
        "{} Hz has no exact sample period at {} Hz: the anchor must divide both clocks \
         (gcd(32768, 32000) = 256, so only its divisors qualify)",
        hz,
        clock_hz
    );
    (clock_hz / hz) as usize
}

/// Decode the shipped asset to planar i32 samples in stream order — the lossless
/// output, before `convert_frame` takes it to 8-bit. The same public [`Decoder`]
/// seam as [`decoded_frames`], for the same reason: the samples have to be the ones
/// the manifest describes.
fn decoded_planar(manifest: &Manifest<'_>) -> (Vec<i32>, Vec<i32>) {
    let mut decoder = Decoder::new(manifest).expect("the shipped manifest must build a decoder");
    let capacity = FrameBuffers::required_len(manifest);
    let mut left = vec![0i32; capacity];
    let mut right = vec![0i32; capacity];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);
    let mut l = Vec::new();
    let mut r = Vec::new();
    loop {
        match decoder
            .decode_next(&mut bufs)
            .expect("the shipped blob decodes on the host — `flac-test` witnesses that")
        {
            Some(frame) => {
                l.extend_from_slice(frame.left);
                r.extend_from_slice(frame.right);
            }
            None => break,
        }
    }
    (l, r)
}

/// The positive leg runs on the shipped bytes: the filter finds the design's 128 Hz
/// tone at the period the **product clock** implies for it and does not find it at
/// the period the recorded clock implies. The negative leg takes the pre-7-0 design
/// and shows the instrument answering the other way, which is what makes the first
/// sentence a measurement rather than a preference. Then the naming rule's answer is
/// compared with what the manifest declares — the only place in this suite where an
/// asset's rate is checked against its samples rather than against its own header.
#[test]
fn the_pitch_witness_names_the_clock_the_samples_were_made_at() {
    let p_new = exact_period(geo::PLAYBACK_RATE_HZ, ANCHOR_HZ);
    let p_old = exact_period(RECORDED_OLD_CLOCK, ANCHOR_HZ);
    assert_eq!(
        p_old - p_new,
        ((RECORDED_OLD_CLOCK - geo::PLAYBACK_RATE_HZ) / ANCHOR_HZ) as usize,
        "the two candidate periods must be exactly the rate offset apart, in samples: \
         that offset IS the Q14 error, so if the anchor's periods were not separated by \
         it, the instrument could not see it"
    );

    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    let (l, r) = decoded_planar(&manifest);
    assert_eq!(
        l.len() as u64,
        gafp_pin::SAMPLES,
        "the decode must be the whole clip the pin describes"
    );
    let n = l.len();

    // Calibration first, and it is the generator's own arithmetic rather than a
    // hand-tuned constant: each design term, filtered against itself and against the
    // other clock's period. `N` is the asset's length throughout, because filter
    // resolution is a function of record length and every ratio below is compared
    // across legs.
    let term_new = design_term(ANCHOR_WEIGHT, p_new, n);
    let term_old = design_term(ANCHOR_WEIGHT, p_old, n);
    let cal_new = read_pitch(&term_new, p_new, p_old);
    let cal_old = read_pitch(&term_old, p_new, p_old);
    println!(
        "calibration: a weight-{} term authored at period {} measures {:.4} counts (declared \
         weight {}; the {:.1}% shortfall is the generator's own integer truncation), and \
         answers {:.0}:1 against the other clock's period {} and {:.0}:1 against its own \
         shoulders. The same term authored at period {} ({} Hz) answers {:.0}:1 the other \
         way — the two hypotheses are separated by the ratio between those answers, not by \
         either answer, which is why the rule thresholds two ratios rather than one (both of \
         them at {}).",
        ANCHOR_WEIGHT,
        p_new,
        cal_new.at_product_clock,
        ANCHOR_WEIGHT,
        (1.0 - cal_new.at_product_clock / f64::from(ANCHOR_WEIGHT)) * 100.0,
        cal_new.candidate_ratio(),
        p_old,
        cal_new.resolved_ratio(),
        p_old,
        RECORDED_OLD_CLOCK,
        cal_old.candidate_ratio(),
        RATIO_NAMES as u64
    );
    assert_eq!(name_the_clock(&cal_new), Some(geo::PLAYBACK_RATE_HZ));
    assert_eq!(name_the_clock(&cal_old), Some(RECORDED_OLD_CLOCK));
    assert!(
        (cal_new.at_product_clock - f64::from(ANCHOR_WEIGHT)).abs()
            < 0.1 * f64::from(ANCHOR_WEIGHT),
        "the instrument must report a declared weight-3 term as ~3 counts, and it reports \
         {:.4}: if this is wrong, nothing measured below means anything",
        cal_new.at_product_clock
    );

    // ---- the shipped asset --------------------------------------------------
    let mid = centre_band(&l, &r);
    let real = read_pitch(&mid, p_new, p_old);
    println!(
        "shipped asset ({} decoded samples, centre band): the 128.000 Hz anchor measures \
         {:.4} counts at period {} (= {} Hz) and {:.4} at period {} (= {} Hz); its \
         offset-matched shoulders are {:.4}; {:.0}:1 candidate-to-candidate, {:.0}:1 over the \
         shoulders",
        n,
        real.at_product_clock,
        p_new,
        p_new as u64 * u64::from(ANCHOR_HZ),
        real.at_recorded_clock,
        p_old,
        p_old as u64 * u64::from(ANCHOR_HZ),
        real.control,
        real.candidate_ratio(),
        real.resolved_ratio()
    );
    let named = name_the_clock(&real).expect(
        "the instrument cannot name a clock for the shipped asset: the anchor measures \
         unresolvably, so the asset's material has changed and this test's premise with it",
    );
    // Present, at its declared level. This is a C1-side claim sitting inside a
    // pitch test: the design term reaches the decoded stream at its authored
    // amplitude, so the chain from what the asset was authored from to what the
    // samples say is intact end to end.
    assert!(
        real.at_product_clock > 0.9 * cal_new.at_product_clock,
        "the anchor's tone must be present at the design level in the decoded stream: \
         measured {:.4}, the declared term alone measures {:.4}",
        real.at_product_clock,
        cal_new.at_product_clock
    );
    // And absent at the other clock's period. A tone of the same design weight
    // sitting there would have answered 0.24% of the winner (that is the calibration's
    // 410:1), and this measures 0.29% — the same order as the instrument's own
    // shoulders, which is how "there is nothing at 250" sounds when it is said by a
    // measurement rather than by an assertion.
    assert!(
        real.at_recorded_clock.abs() < 0.02 * real.at_product_clock.abs(),
        "the anchor must not be present at the {}-clock period {}: {:.4} against {:.4}",
        RECORDED_OLD_CLOCK,
        p_old,
        real.at_recorded_clock,
        real.at_product_clock
    );

    // Two probes. The first is the confounder that broke the earlier version of this
    // instrument: the fixture's per-channel hash ramps have period 801, so a template
    // at period 267 is their third harmonic — a real component of the centre band, and
    // inside any window that contains a shoulder.
    let harmonic = matched(&mid, 801 / 3);
    println!(
        "probe: period {} — the third harmonic of the fixture's 801-sample noise ramp, and \
         inside the window a 250/256 reading sweeps — answers {:.4} counts on this asset \
         against the anchor's {:.4} and its shoulders' {:.4}. A window-as-noise floor \
         counts that as noise and fails the asset at {:.1}:1.",
        801 / 3,
        harmonic,
        real.at_product_clock,
        real.control,
        real.at_product_clock / harmonic.abs().max(1e-12)
    );
    assert!(
        harmonic.abs() > 5.0 * real.control,
        "the 801/3 harmonic has to stay a component of this signal, well clear of the \
         offset-matched shoulders — it answers {:.4} against {:.4}. That is what makes a \
         window-as-noise floor wrong rather than merely loose; if this ever does read as \
         noise, the anchor choice above has to be re-argued rather than kept.",
        harmonic,
        real.control
    );
    // The second is the rejected 32 Hz anchor. Period 1024 divides both clocks
    // exactly, so it is arithmetically eligible and is ruled out on what it measures:
    // its energy is in the side band, where the fixture deliberately puts its noise.
    let side: Vec<i32> = l
        .iter()
        .zip(r.iter())
        .map(|(a, b)| ((*a as i64 - *b as i64) >> 1) as i32)
        .collect();
    let at_1024 = matched(&side, 1024);
    let at_1023 = matched(&side, 1023);
    println!(
        "probe: the side band answers {:.4} counts at period 1024 (32.000 Hz, the rejected \
         anchor) and {:.4} at period 1023 — {:.0}% of that answer arrives at a period no \
         part of the design declares. That is what an anchor sitting inside noise looks \
         like.",
        at_1024,
        at_1023,
        at_1023.abs() * 100.0 / at_1024.abs().max(1e-12)
    );
    assert!(
        at_1023.abs() > 0.25 * at_1024.abs(),
        "the 32 Hz rejection rests on the neighbouring side-band period answering a large \
         share of the anchor: measured {:.4} against {:.4}. If that stopped being true the \
         rejection would have to be re-derived, not kept.",
        at_1023,
        at_1024
    );
    // The point of the whole exercise, in the units Q14 is stated in.
    assert_eq!(
        named,
        manifest.sample_rate_hz(),
        "the clock the samples imply ({}) and the clock the manifest declares ({}) are \
         different numbers: the header is not describing the audio",
        named,
        manifest.sample_rate_hz()
    );
    assert_eq!(
        named,
        geo::PLAYBACK_RATE_HZ,
        "the samples must imply the clock the player runs on, or the run is pitch-shifted \
         whatever the header says"
    );
    println!(
        "shipped asset: the samples imply a {} Hz axis, the manifest declares {} Hz, the \
         player runs {} Hz — the three agree, and the pitch error a listener would meet is \
         {:.2} cents.",
        named,
        manifest.sample_rate_hz(),
        geo::PLAYBACK_RATE_HZ,
        pitch_cents_of(named, geo::PLAYBACK_RATE_HZ)
    );
    assert_eq!(pitch_cents_of(named, geo::PLAYBACK_RATE_HZ), 0.0);

    // ---- the negative leg: the same design, authored at the recorded clock ----
    let (ol, or) = recorded_design_at_old_clock(n);
    let neg = read_pitch(&centre_band(&ol, &or), p_new, p_old);
    let neg_named = name_the_clock(&neg);
    println!(
        "negative leg: the design recorded at {} Hz, resynthesised for this run at {} Hz — \
         the same anchor measures {:.4} \
         counts at period {} and {:.4} at period {} ({:.0}:1, {:.0}:1 over the shoulders); \
         the instrument names {:?}",
        RECORDED_OLD_CLOCK,
        RECORDED_OLD_CLOCK,
        neg.at_recorded_clock,
        p_old,
        neg.at_product_clock,
        p_new,
        neg.candidate_ratio(),
        neg.resolved_ratio(),
        neg_named
    );
    assert_eq!(
        neg_named,
        Some(RECORDED_OLD_CLOCK),
        "the instrument must refuse the same anchor at the product period when the samples \
         were authored at {} — if it names {} here, the positive leg's answer was a \
         preference, not a measurement",
        RECORDED_OLD_CLOCK,
        geo::PLAYBACK_RATE_HZ
    );
    assert_eq!(p_old * ANCHOR_HZ as usize, RECORDED_OLD_CLOCK as usize);
    let cents = pitch_cents_of(RECORDED_OLD_CLOCK, RECORDED_PRODUCT_CLOCK);
    println!(
        "negative leg: the recorded 32,000 Hz design against the 32,768 Hz clock it was \
         played on is {:.2} cents; every digest in this file would have passed it. That is \
         the 6b-3 configuration: the sample side cannot see it, so the rule side has to, \
         and now there is an instrument behind both.",
        cents
    );
    assert!(
        (cents - 41.06).abs() < 0.01,
        "the same figure `ruled_rate_policy_allows_no_reinterpretation_of_a_declared_rate` \
         pins, reached from measured sample periods instead of two header fields"
    );
    // The instrument's resolution, stated as a measured bound rather than claimed: a
    // 2.4% period offset separates 410:1 on a clean term and 340:1 on the shipped
    // bytes, so a Q14-class error is not near this instrument's floor. What *is* near
    // a floor is a tone at the bottom of an 8-bit range, and that is the next
    // test.
    assert!(
        real.candidate_ratio() > RATIO_NAMES && real.resolved_ratio() > RATIO_NAMES,
        "the shipped asset must clear both thresholds by a real margin, not marginally: \
         {:.0} and {:.0}",
        real.candidate_ratio(),
        real.resolved_ratio()
    );
}

/// The other half of the measurement, and the half that says what 7-3 can and
/// cannot be asked to witness. Everything above is measured at 16-bit depth, before
/// the FIFO. The GBA plays at 8 bits, so the question is whether the 8-bit path is
/// what would hide a pitch error — and the answer, on the product's own conversion
/// path, is that it is not:
///
/// * A ±24,000-count anchor tone through `convert_frame` measures **93.758 LSB**
///   against the exact arithmetic of 24,000/256 = 93.750 — 0.008% out — and its
///   period separates 414:1 from the other clock's period and 211:1 from its own
///   shoulders. A tone at *half an LSB* (±128 counts) still answers 394:1 and 209:1.
///   So the truncation costs amplitude and none of the pitch: `sample >> 8` keeps
///   the sign of a sub-LSB component, and its zero crossings, which is where period
///   lives.
/// * The shipped fixture's own anchor is **2.92 i16 counts** — its declared weight,
///   exactly as authored — which is **1.1% of one FIFO step**. Run through the real
///   converter, its period is still resolvable: 66:1 and 20:1 on the played bytes.
///   So this fixture *is* a pitch witness for a machine, at both depths, and it is
///   not one for a person: at that level the component cannot set a quantisation
///   step of its own.
///
/// That second bullet is the one the plan needs written down, and it belongs in the
/// record of 7-0 rather than in a footnote on 7-3: **the fixture that was chosen
/// because it makes the encoder behave cannot be listened to.** Its centre partials
/// are single-digit i16 counts against a centre band of 822 counts peak-to-peak,
/// because a strong broadband floor is exactly what makes `-l 4`
/// choose LPC while `-l 0` stays FIXED — `scripts/frame_vectors.py`'s own docstring
/// for `synth_pcm` says so. C2's acceptance wording promises a human being will
/// judge pitch and tempo, and the profile question the plan keeps deferring (FIXED-0
/// only; real music out of scope) is the same question: material a person could hear
/// in tune is material that costs LPC-side cycles to decode. This test does not
/// decide that. It puts a measured number on it, and it demonstrates the instrument
/// is ready for whichever material the answer uses.
#[test]
fn the_pitch_witness_survives_the_fifo_and_the_product_fixture_does_not() {
    let p_new = exact_period(geo::PLAYBACK_RATE_HZ, ANCHOR_HZ);
    let p_old = exact_period(RECORDED_OLD_CLOCK, ANCHOR_HZ);
    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    let (l, r) = decoded_planar(&manifest);
    let mid = centre_band(&l, &r);
    let frames = decoded_frames(&manifest);

    let at_16 = read_pitch(&mid, p_new, p_old);
    // The played form: the exact bytes `decoded_frames` produced, read back signed
    // the way `agb`'s mixer reads them (`ldrsb`), which is the only reason a
    // sub-LSB component has any period left to be found.
    let played: Vec<i32> = frames
        .iter()
        .flat_map(|(fl, _)| fl.iter().map(|&b| b as i8 as i32))
        .collect();
    assert_eq!(played.len(), mid.len(), "every sample has a played byte");
    let at_8 = read_pitch(&played, p_new, p_old);
    println!(
        "the product fixture: anchor = {:.4} i16 counts = {:.4} of one FIFO step, in a centre \
         band whose own peak-to-peak is {}; at 8-bit the same filter answers {:.4} LSB at \
         period {} against shoulders of {:.4} — {:.0}:1 and {:.0}:1, so the period arrives \
         and the level does not.",
        at_16.at_product_clock,
        at_16.at_product_clock / FIFO_STEP_COUNTS,
        mid.iter().max().unwrap() - mid.iter().min().unwrap(),
        at_8.at_product_clock,
        p_new,
        at_8.control,
        at_8.candidate_ratio(),
        at_8.resolved_ratio()
    );
    assert!(
        at_16.at_product_clock / FIFO_STEP_COUNTS < 0.02,
        "the fixture's anchor should sit far below one FIFO step by any measure: it is at \
         {:.4} of a step",
        at_16.at_product_clock / FIFO_STEP_COUNTS
    );
    assert!(
        at_8.candidate_ratio() > RATIO_NAMES && at_8.resolved_ratio() > RATIO_NAMES,
        "the played form is expected to *keep* a nameable pitch — that is the finding, and \
         if it stopped being true the conversion path changed: {:.0}:1, {:.0}:1",
        at_8.candidate_ratio(),
        at_8.resolved_ratio()
    );
    assert_eq!(name_the_clock(&at_8), Some(geo::PLAYBACK_RATE_HZ));
    // What a sub-LSB component actually becomes at 8 bits, stated exactly rather than
    // argued. The term alone, on its own, maps to two byte values: the non-negative
    // half-epochs to 0x00 and the negative ones to 0xFF. Truncation therefore turns a
    // triangle the DAC cannot represent into a one-step square wave at the anchor's own
    // period — same pitch, wrong waveform, at the mixer's own floor. (Length 4,096
    // here, because this probe is about the byte values rather than about filter
    // resolution, and the shortest window that shows the pattern is the clearest one.)
    let term = design_term(ANCHOR_WEIGHT, p_new, 4096);
    let bytes: Vec<u8> = term.iter().map(|&v| fifo_byte(v)).collect();
    let distinct: Vec<u8> = {
        let mut d = bytes.clone();
        d.sort_unstable();
        d.dedup();
        d
    };
    println!(
        "the fixture's anchor term alone, at the FIFO depth, takes exactly the byte values \
         {:?} — a {} LSB peak-to-peak square wave at the anchor's own period, not the tone \
         that was authored.",
        distinct
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>(),
        distinct.iter().map(|&b| b as i8 as i32).max().unwrap()
            - distinct.iter().map(|&b| b as i8 as i32).min().unwrap()
    );
    assert_eq!(
        distinct,
        vec![0x00, 0xFF],
        "a weight-{} term (±{} counts) must map to exactly the two bytes straddling zero, \
         because {} is far below one FIFO step of {} counts — that is what 'below the DAC \
         floor' means here, in the mixer's own terms",
        ANCHOR_WEIGHT,
        ANCHOR_WEIGHT,
        ANCHOR_WEIGHT,
        FIFO_STEP_COUNTS as u32
    );

    // And the control that keeps the previous paragraph honest: the 8-bit depth is not
    // what limits this instrument. Same anchor, same period, same real
    // `convert_frame`, at a level that has something to preserve — and the same record
    // length as every other leg in this file, because filter resolution is a function
    // of record length and a ratio measured over a shorter window is not comparable
    // with one measured over a longer one. (That was not obvious the first time
    // through: these two controls were 65,536 samples and came out at 49:1 / 50:1
    // where the identical signal over the asset's own length answers ~410:1 / ~210:1.)
    let peak = 24_000i32;
    let loud = tone(peak, p_new, mid.len());
    let mut out_l = vec![0u8; loud.len()];
    let mut out_r = vec![0u8; loud.len()];
    convert_frame(&loud, &[], &mut out_l, &mut out_r).expect("a mono window converts alone");
    let loud_bytes: Vec<i32> = out_l.iter().map(|&b| b as i8 as i32).collect();
    let loud_read = read_pitch(&loud_bytes, p_new, p_old);
    let exact = f64::from(peak) / FIFO_STEP_COUNTS;
    println!(
        "control: a ±{}-count anchor tone through the real convert_frame measures {:.4} LSB \
         against the exact arithmetic {:.4} ({:.3}% out) and separates {:.0}:1 / {:.0}:1. \
         The 8-bit path costs 8 bits of amplitude and no pitch.",
        peak,
        loud_read.at_product_clock,
        exact,
        (loud_read.at_product_clock - exact).abs() * 100.0 / exact,
        loud_read.candidate_ratio(),
        loud_read.resolved_ratio()
    );
    assert!(
        (loud_read.at_product_clock - exact).abs() < 0.01 * exact,
        "a tone with something to preserve must measure its own arithmetic through the \
         conversion path: expected {} LSB, measured {:.4}",
        exact,
        loud_read.at_product_clock
    );
    assert_eq!(name_the_clock(&loud_read), Some(geo::PLAYBACK_RATE_HZ));
    // Half an LSB, for the floor of where that stops: still nameable, so the
    // fixture's level — 1.1% of a step, 44x below this — is the whole of the
    // difference between "a machine can tell" and "a person could hear".
    let half_peak = (FIFO_STEP_COUNTS as i32) / 2;
    let half = tone(half_peak, p_new, mid.len());
    let mut half_out = vec![0u8; half.len()];
    convert_frame(&half, &[], &mut half_out, &mut out_r).expect("a half-LSB tone fits i16");
    let half_bytes: Vec<i32> = half_out.iter().map(|&b| b as i8 as i32).collect();
    let half_read = read_pitch(&half_bytes, p_new, p_old);
    println!(
        "control: a half-LSB tone (+-{}) still measures {:.4} LSB and separates {:.0}:1 / \
         {:.0}:1 — {:.0}x louder in i16 counts than the product fixture's anchor, which is \
         the entire gap between a machine being able to name this asset's pitch and a person \
         being able to hear it.",
        half_peak,
        half_read.at_product_clock,
        half_read.candidate_ratio(),
        half_read.resolved_ratio(),
        f64::from(half_peak) / at_16.at_product_clock
    );
    assert_eq!(name_the_clock(&half_read), Some(geo::PLAYBACK_RATE_HZ));
    assert!(
        f64::from(half_peak) / at_16.at_product_clock > 20.0,
        "the gap between the fixture's level and a level this instrument can read at 8 bits \
         is the measured finding: {:.1}x. If it ever narrows below 20x, the fixture got \
         louder and this test's conclusion has to be re-read, not re-run.",
        f64::from(half_peak) / at_16.at_product_clock
    );
}
