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
//! by *throughput*: 157.3% of the measured cost with no ring in the picture, 196.0%
//! with this one. So a deep ring is worth ~39 points of sustained overage and then
//! nothing further, while being worth a great deal of stall — the two numbers are
//! nowhere near each other, which is why [`sustained_overage_tolerance`] and
//! [`burst_stall_tolerance`] measure them separately instead of quoting one
//! "generous margin" figure that would mean whichever of the two the reader
//! happened to need.
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
//! prefill, less the prime chunk and one period's drain, is 9,120, the number the
//! ROM prints. Keeping a disagreement that was a bug would have been the more
//! expensive kind of honesty.
//!
//! ## Where the cost figures come from
//!
//! From a run, not an estimate: `product-play.gba` built on 2026-11-11 under mGBA —
//! the first image in this repository to play at the target rate — whose serial log
//! gives [`MEASURED`] and [`FEEDER_PER_PERIOD`]. Those are mGBA's numbers, and the
//! plan's standing assumption is that memory-heavy stages run ~7% slower on silicon
//! (measured for the kernel A/B, `flac-docs/measurements.md`), so the central
//! claims are run again at that derate. A model that only passed at the emulator's
//! cost would be modelling the emulator.

#![forbid(unsafe_code)]

use flac_lite::{Decoder, FrameBuffers, Manifest};

use flac_integration::gafp_pin;
use flac_integration::geometry as geo;
use flac_integration::playback::{Chunk, RingFeed, convert_frame};

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

/// The producer's own work per frame, in audio cycles, as the 2026-11-11 mGBA run
/// of `product-play.gba` reported it: decode 426,327 + convert 113,586 + push
/// 15,280 = 555,193 mean, **less** the period boundaries nested inside those
/// windows (538 of them, at 5,194 c of feeder each = 2,794,572 over 152 windows =
/// 18,385 per window), because that time belongs to the interrupt, not to the
/// producer.
///
/// That subtraction is the attribution the ROM prints itself. It is an estimate —
/// an interrupt's entry cost is not separable from the work it interrupted — which
/// is why the unattributed figure is charged too, in [`MEASURED_RAW`], and why
/// nothing here turns on the difference.
const MEASURED: u64 = 536_808;

/// The same envelope with the nesting left in — the ROM's raw stage means. Charged
/// as a second scenario so the attribution's effect on the conclusion is visible
/// rather than assumed away.
const MEASURED_RAW: u64 = 555_193;

/// The feeder's own cost per period, from the same run: the mean of 571 measured
/// samples. It is deducted from each period's budget because the model charges the
/// producer [`MEASURED`], which excludes the nesting: the two halves of the GBA's
/// one core have to add up, not overlap.
///
/// Note what is *not* deducted: the producer's spin-wait for ring room. Spinning
/// holds the core but neither advances nor blocks progress — the ring is what gates
/// it — and it does not delay the interrupt handler, which is why the model treats
/// only the handler's cycles as scarce. That is not a simplification for the
/// model's convenience; it is how a single core with interrupts enabled behaves.
const FEEDER_PER_PERIOD: u64 = 5_194;

/// The plan's standing hardware derate for memory-heavy stages, applied as a second
/// scenario wherever a claim is meant to survive silicon. It is a *measured* ratio
/// for the one thing it was measured on — the kernel A/B ran +7.09% on hardware
/// against mGBA's +7.90%, `flac-docs/plan-of-record.md` — and is used here as a
/// deliberately blunt penalty, not a claim about the mechanism behind it.
const HARDWARE_DERATE_PERCENT: u64 = 7;

/// The 2026-11-11 mGBA run's observed ring watermark, for the model to compare
/// against: the minimum lead seen immediately before a push, and the maximum seen
/// immediately after one.
const ROM_OBSERVED_MIN_LEAD: usize = 9_120;
/// …and the maximum.
const ROM_OBSERVED_MAX_LEAD: usize = 19_600;

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
}

impl Settings {
    /// The geometry as shipped, at a given producer cost.
    fn shipped(work_per_frame: u64) -> Self {
        Self {
            work_per_frame,
            feeder_per_period: FEEDER_PER_PERIOD,
            prefill_chunks: geo::PREFILL_CHUNKS,
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
            if self.credit < work {
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
            self.credit -= work;
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
        1 << 20,
        "2048-sample frames at 32,768 Hz must come to exactly 2^20 cycles, which is what every published share at this rate is divided by"
    );

    // The rate mapping is the packer's, and its consequence is arithmetic: the
    // frames were encoded for a 32,000 Hz stream and are played against a 32,768 Hz
    // clock, so the budget the player enforces is the *shorter* one. Both numbers
    // are printed because that 2.4% is the price of the mapping and belongs in the
    // open, not in a footnote. The mapping itself is **`TODO(Q14, BLOCKING)`** —
    // ruled not a product path on 2026-11-11 because the other price it takes is 41
    // cents of pitch (`ruled_rate_policy_allows_one_bridge_and_no_more` below, and
    // `src/geometry.rs`); what this test models is the geometry, which survives the
    // ruling unchanged.
    let declared = manifest.sample_rate_hz();
    let mapped = geo::playback_rate_for_stream(declared)
        .expect("the packer's table must cover the rate the packer wrote");
    assert_eq!(mapped, geo::PLAYBACK_RATE_HZ);
    let nominal = geo::MIXER_TICK_HZ as u64 * block / u64::from(declared);
    let budget = u64::from(geo::frame_cycles(u32::from(manifest.blocksize())));
    assert!(
        budget < nominal,
        "playing a {} Hz stream at {} Hz must make the budget tighter, not looser",
        declared,
        mapped
    );
    println!(
        "model/asset: {} frames of {}, {} samples; declared {} Hz -> played at {} Hz; the stream's own nominal frame period is {} c, the player's budget is {} c (the mapping costs {:.3}% of the producer's time)",
        manifest.frame_count(),
        manifest.blocksize(),
        manifest.total_samples(),
        declared,
        mapped,
        nominal,
        budget,
        (nominal - budget) as f64 * 100.0 / nominal as f64
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
    // block has to be loaded with, and a period is 560 samples at 512 cycles each.
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
        8192
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

/// The floor the ROM observed, derived — and the reason the flow control asks for
/// the *declared* blocksize rather than the frame it is about to decode.
#[test]
fn floor_matches_the_roms_watermark() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();
    let out = run::<{ geo::RING_SAMPLES }>(&Settings::shipped(MEASURED), &frames, total);
    println!(
        "model/floor: predicted min_lead={} max_lead={}; the 2026-11-11 ROM run observed min_lead={} max_lead={}",
        out.min_lead, out.max_lead, ROM_OBSERVED_MIN_LEAD, ROM_OBSERVED_MAX_LEAD
    );
    // The floor, derived rather than quoted. The prefill cannot land on its target:
    // it stops after the push that *crosses* it, so its depth is
    // `ceil(target / frame_len) * frame_len`. From there, one chunk leaves the ring
    // into the prime and one more is taken by the first period before the first
    // fed-phase push can happen (the producer cannot start a frame before its
    // credit arrives, and one period's worth is not enough for a 536,808-cycle
    // frame). That is the whole of the arithmetic, and it comes out at the number
    // the ROM measured — sample for sample, from an independent implementation.
    let block = usize::from(manifest.blocksize());
    let prefill_actual = (geo::PREFILL_SAMPLES + block - 1) / block * block;
    let predicted = prefill_actual - 2 * geo::CHUNK_SAMPLES;
    println!(
        "model/floor: prefill target {} -> actual {} (rounded up to whole frames of {}); less the prime and one period's drain = {}",
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
        "the floor is the quantised prefill less the prime and the first period's drain; anything else means the model and the ROM disagree about flow control"
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

/// How much more the producer can cost, *for the whole clip*, before an underrun
/// appears.
///
/// This is the number a wrong clock assumption, a slower part, or a profile
/// relaxation would spend, and it is not the same number as the burst tolerance in
/// [`burst_stall_tolerance`]. The mechanism is throughput: past some multiplier the
/// producer can no longer deliver its 0.2734 frames per period, and from there the
/// ring loses a little every period. Its depth buys a specific and computable
/// amount of that — the refill — which is why the answer is larger than the
/// throughput-only bound and not unboundably larger.
#[test]
fn sustained_overage_tolerance() {
    let manifest = Manifest::parse(BLOB).expect("parses");
    let frames = decoded_frames(&manifest);
    let total = usize::try_from(manifest.total_samples()).unwrap();

    // Bisection in per-mille, so no float enters the search.
    let survives = |work: u64| -> bool {
        let out = run::<{ geo::RING_SAMPLES }>(&Settings::shipped(work), &frames, total);
        out.underruns == 0 && out.consumed == total
    };

    let mut lo: u64 = 1_000;
    let mut hi: u64 = 1_000;
    while survives(MEASURED * hi / 1000) {
        lo = hi;
        hi *= 2;
        assert!(
            hi < 100_000,
            "the search ran away: the geometry is more forgiving than any cost model we would believe"
        );
    }
    while hi - lo > 10 {
        let mid = (lo + hi) / 2;
        if survives(MEASURED * mid / 1000) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // The same search at the derate, because a tolerance measured on an emulator is
    // a tolerance of the emulator.
    let derated_work = Settings::derate(MEASURED);
    let survives_dera = |work: u64| -> bool {
        let out = run::<{ geo::RING_SAMPLES }>(&Settings::shipped(work), &frames, total);
        out.underruns == 0 && out.consumed == total
    };
    let mut dlo: u64 = 1_000;
    let mut dhi: u64 = 1_000;
    while survives_dera(derated_work * dhi / 1000) {
        dlo = dhi;
        dhi *= 2;
        assert!(dhi < 100_000);
    }
    while dhi - dlo > 10 {
        let mid = (dlo + dhi) / 2;
        if survives_dera(derated_work * mid / 1000) {
            dlo = mid;
        } else {
            dhi = mid;
        }
    }

    // And the bound that needs no simulation: the producer is handed
    // `PERIOD_CYCLES - feeder` cycles per period, and owes a frame every
    // `frame_cycles / PERIOD_CYCLES` periods. That is a pure-throughput ceiling on
    // the cost, at which the ring never fills and never empties.
    let periods_per_frame = u64::from(geo::frame_cycles(2048)) / u64::from(geo::PERIOD_CYCLES);
    let throughput_bound = (u64::from(geo::PERIOD_CYCLES) - FEEDER_PER_PERIOD) * periods_per_frame;
    println!(
        "model/sustained: throughput-only ceiling (no ring in the picture) = {} c/frame = {} per mille of the measured {}",
        throughput_bound,
        throughput_bound * 1000 / MEASURED,
        MEASURED
    );
    println!(
        "model/sustained: with the shipped ring, measured cost survives up to {} per mille ({:.1}% over); at the hardware derate, up to {} per mille over the derated cost — the excess over the ceiling above is exactly what the ring's refill buys, and it is finite",
        lo,
        (lo - 1000) as f64 / 10.0,
        dlo
    );
    let _ = MEASURED * lo / 1000;
    assert!(
        lo >= 1_500,
        "the shipped geometry must survive at least a 50% sustained overage of the measured producer cost; found {} per mille",
        lo
    );
    assert!(
        lo >= throughput_bound * 1000 / MEASURED,
        "a ring can only ever buy more than the throughput ceiling, never less — if this fails the model and the ceiling are describing different players"
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

/// Q14, **ruled** by the project owner on 2026-11-11: a stream is encoded at the rate
/// it is played at, and the player does not reinterpret a declared rate to fit a
/// clock — the 1.024× that the shipped asset buys is not acceptable in itself, however
/// clean the run. The asset in `assets/` predates the ruling, so this test does not
/// assert compliance. It does three other things: it pins the *size* of the violation
/// in cents, computed here rather than quoted from prose; it refuses every rate
/// reinterpretation except the one bridge that is on record; and it states which
/// asset in this tree is the disqualified one.
///
/// **`TODO(Q14, BLOCKING)`** — when the 32,768 Hz product asset lands (plan item 7-0),
/// the `32_000` arm in `src/geometry.rs` comes out and this test is rewritten to
/// assert the identity rule alone. The final assert below is expected to go red at
/// exactly that moment; that is what it is for.
#[test]
fn ruled_rate_policy_allows_one_bridge_and_no_more() {
    // The product arm: a stream that declares the target rate is played at it.
    assert_eq!(
        geo::playback_rate_for_stream(geo::PLAYBACK_RATE_HZ),
        Some(geo::PLAYBACK_RATE_HZ),
        "the identity mapping is the only one a product asset may take"
    );

    // Every other rate a FLAC stream can declare: refused, not reinterpreted. This
    // list is the guard — a `Some(_)` appearing in it is a new pitch shift being
    // waved through by a future edit, and Q14 has ruled those out.
    for rate in [
        8_000u32, 11_025, 16_000, 18_157, 22_050, 24_000, 26_000, 31_000, 32_001, 33_000, 44_100,
        48_000, 65_536,
    ] {
        assert_eq!(
            geo::playback_rate_for_stream(rate),
            None,
            "{rate} Hz must be refused by the player, not mapped onto the timer"
        );
    }

    // The one bridge, and its price, computed rather than transcribed.
    let played = geo::playback_rate_for_stream(32_000)
        .expect("the bridge is still in the tree; 7-0 removes it");
    let ratio = f64::from(played) / 32_000.0;
    let cents = 1200.0 * ratio.log2();
    println!(
        "q14: the bridge plays a 32,000 Hz stream at {} Hz = {}x = {:.2} cents sharp. \
         TODO(Q14, BLOCKING): ruled out as a product path 2026-11-11; 7-0 owns the \
         encode at 32,768 and 7-1 owns the hardware row at (32768, 2048, fixed, 0).",
        played, ratio, cents
    );
    assert_eq!(played, 32_768);
    assert!(
        (cents - 41.06).abs() < 0.01,
        "1.024x is 41.06 cents — pinned so that no future record can call it a rounding \
         error or a semitone; it is neither, and it is disqualifying"
    );

    // And the disqualification is about the asset in this tree, not about an abstract
    // one: the shipped blob declares a rate that is not the rate it is played at.
    let manifest = Manifest::parse(BLOB).expect("the shipped blob parses");
    assert_ne!(
        manifest.sample_rate_hz(),
        geo::PLAYBACK_RATE_HZ,
        "the asset currently shipped is the one the ruling disqualifies; this assert \
         going red is the sound of 7-0 landing"
    );
}
