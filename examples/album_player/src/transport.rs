//! `transport.rs` — the playback transport, and §4.4's pause / resume contract.
//!
//! PR-3b, step ii-B2a defined pause; ii-B2b adds skip. Every state below is reachable and every one
//! of them has a caller: `Playing`, `Pausing`, `Paused`, `Resuming`, `Skipping`, `Repriming`.
//! `TrackEnd`/`Ended` and the decode-error state are ii-B2b's stage 2 and are not declared before
//! they exist.
//!
//! ## The contract, written down here because pause had none anywhere in the record
//!
//! Pause is the first definition of pause in this project — §4.4, the probe and the shipped
//! `truncate()` all covered *skip* only — so it is stated as a sequence with its bounds, and the
//! states below are that sequence:
//!
//! | step | what happens | bound |
//! |---|---|---|
//! | press → edge | the vblank ISR's [`keys::Sampler`](crate::keys::Sampler) debounces and posts | ≤ 1 video frame |
//! | edge → fade begins | the main loop takes the post and issues [`Transport::toggle`] at a period boundary | ≤ 1 producer frame + 1 period (the loop is inside a decode until then) |
//! | fade | the feeder scales the hand-off slot, full scale → silence | ≤ one period, and 256 samples inside it |
//! | play-out | the feeder serves one more period, so the ramped buffer is played to its silent tail | one period |
//! | freeze | the main loop stops timer1, then timer0/timer2, then both sound DMAs | the next main-loop pass after that period |
//!
//! **`pause`:** ramp the hand-off gain to zero, let the buffer carrying the ramp be played out, then
//! freeze — stop both sound DMAs and the timer chain, halt the producer, and leave the ring exactly
//! as it is.
//!
//! **`resume`:** re-prime both DMA buffers from the ring head and re-arm in the shipped order
//! (`enable_sound` → prime → `clear_timer1_if` → the already-installed handler →
//! `arm_audio_chain`), ramping the gain back up.
//!
//! **`skip`** is §4.4's six steps with the gain at the hand-off (D4, ii-B2b):
//!
//! | step | what happens | who |
//! |---|---|---|
//! | 1 | the hand-off gain ramps to zero over one chunk — the listener hears the track that is actually playing fade | feeder, via [`Transport::skip`] |
//! | 2 | `RingFeed::truncate()` **and the first whole chunk of the silence cover in the same critical section** | main loop, inside [`Transport::begin_reprime`]'s section |
//! | 3 | `source.start_track(i ± 1)` — the seam call; per-backend repositioning stays adapter-internal | main loop |
//! | 4 | the rest of the cover: [`SKIP_PRIME_CHUNKS`] whole chunks of zeros, so no feeder period can starve | main loop |
//! | 5 | the new track's audio is produced behind the cover | producer |
//! | 6 | the ramp back up starts on the chunk **after** the cover, so the new track's own samples are the ones that fade in | feeder, via [`HandOffGain::fade_in_after`] |
//!
//! Three properties of that table are load-bearing and each is a bug the probe paid for:
//!
//! - **The truncate and the cover's first chunk share one critical section.** A bare truncate leaves
//!   a window in which a feeder period finds an empty ring with the producer still running: that
//!   period is `Starved` and books an underflow that is really an artifact of the window
//!   (`examples/album_probe/src/probe_hw.rs:840-875`, whose `truncate_stale` does exactly this).
//! - **The ramp-in cannot start before the cover has been handed over.** The gain is applied by the
//!   feeder to whatever chunk it takes; if the ramp began while cover chunks were still queued, the
//!   ramp would be spent on silence and the new track's first chunk would arrive at full gain — a
//!   click, which is the one thing the ramp exists to avoid. [`HandOffGain::fade_in_after`] holds the
//!   gain at zero for exactly the cover's chunks.
//! - **A skip while paused, or during another transition, is [`Step::Ignored`].** The contract has no
//!   cancel — reversing a fade half-way is a click of its own — and a frozen chain has no hand-off to
//!   fade: re-targeting while frozen would need a second re-prime path and a way to cancel a queued
//!   silence cover, and no §5.3 row asks for it. The caller counts the ignore.
//!
//! **A skip's wait to audible audio is four numbers, not one, and they belong to three different
//! parties** (§4.4's D2 row is corrected to read this): *press → detection* is the ISR's debounce
//! (≤ 1 video frame); *detection → the post being taken* is the frame loop's and is **one producer
//! frame**, 3.7–3.9 video frames of uninterruptible decode (measured 3 on the gate run) — the leg a
//! log that only printed the audio clock's phase at detection could not see; *take → hand-off* is
//! [`take_to_handoff_bound`] periods, whose name predates the four-legged split and whose
//! value bounds only this leg (measured 2 periods); *hand-off → the new track's first chunk* is the
//! cover's ([`SKIP_PRIME_CHUNKS`] + 1 = 5 periods, and that is §4.4 step 5's claim); and anything
//! beyond that is the **track's own head** — a chunk of digital silence at the top of a track is
//! byte-identical to a chunk of the cover, so `to_new` reads `content_at + head_silence` and only the
//! track can say how long its own lead-in is. The gate fixture's two full-master tracks read 8
//! periods of that (its head is 4,480 all-zero samples; the first non-zero one is a −1 in the ninth
//! chunk); its two clips read 0. So the worst case a listener waits, press to the new track's first
//! audible byte, is **14 periods = 479 ms, plus one producer frame ≈ 65 ms for the ISR→take wait and
//! ≤ 1 video frame = 16.7 ms for the debounce: ~561 ms** on this album — inside §4.4's ≤ ~0.6 s, but
//! not the 171 ms D2 counted, because D2 counted the cover alone. The ROM prints each leg and their
//! sum (`latency:` line), and both the `reaction` term and the skip rows gate them.
//!
//! Four consequences follow, and each is the reason a simpler-looking design was rejected:
//!
//! - **The position is exact.** Nothing is dropped, so no truncate, no seek and no re-decode is
//!   needed, and the resume continues on the ring's next sample. (The clock is exact; the buffer the
//!   DMA had already played out is not replayed — see [`Transport::tick`]'s deferral, which is what
//!   makes that buffer the ramped one and not a mid-waveform cut.)
//! - **The displayed time does not run while paused.** [`position`](crate::position)'s session
//!   clock is incremented *in* the period IRQ, so stopping the timer chain stops the clock; §4.4's
//!   never-backwards / 0-at-start / `n_samples`-at-end invariants hold across a pause by
//!   construction rather than by a compensating subtraction.
//! - **No underrun is booked while frozen.** The underflow counter is incremented only in the
//!   feeder's starved arm, and a frozen chain never enters the feeder — so there is no "paused"
//!   test in the hot path that could fall out of step with the hardware.
//! - **The producer must not be resumed into a full ring.** A pause holds `ready()` samples it is
//!   not consuming, so the producer's flow control is still the gate: `RingFeed::push_bytes`
//!   refuses a frame that does not fit and the caller waits for room instead of pushing.
//!
//! Rejected, with the reason on the record: *truncate + decode-forward* would need
//! `start_track` then a discard-forward from the **track start** — the seam has no seek (Q5), so
//! four minutes into a track that is ~77 s of CPU to get back to where the listener was — and
//! *keep-feeding-silence* would let the 1.23 s already queued play out before any fade began, and
//! `committed` would keep advancing, so the UI clock would run while the cart looked paused.
//!
//! ## Why the gain is here and not with the producer
//!
//! The ring holds 20,160 samples = 1.23 s of already-decoded audio, so a producer-side fade dims
//! audio that has not been queued to play yet and cannot make a pause respond. The gain therefore
//! belongs to the hand-off — [`HandOffGain::apply`] on the ping-pong slot the feeder has just
//! filled — which is also what leaves the ring untouched.
//!
//! The gain itself lives here rather than beside [`fifo_byte`](gba_playback::playback::fifo_byte) in
//! `crates/gba-playback` because that crate is linked into the shipped `product-play.gba`: an
//! additive line there moves that image's sha256 and so owes a re-baseline in
//! `flac-docs/measurements.md`, which is the FLAC track's record, not this app's. The format ruling
//! [`scale_byte`] obeys is still `fifo_byte`'s, cited by reference so the two cannot drift silently.
//!
//! ## Who runs what
//!
//! The audio period IRQ owns the cheap per-period decision: which gain applies to the next slot,
//! and it applies it. The main loop owns the expensive transitions: [`Transport::toggle`] for a
//! command, [`Transport::tick`] for "the fade has finished, freeze" / "the ramp is back, play", and
//! the hardware work each step names. The state lives here so the host model drives the same
//! machine the ROM does; nothing in this module names `agb`, a register or a timer.

// -------------------------------------------------------------------------------------------------
// The hand-off gain (ALBUM-PLAYER.md §4.4, PR-3b-ii-B2a's D3)
// -------------------------------------------------------------------------------------------------
//
// A fade the listener is meant to hear is applied to the bytes **at the moment they are handed to
// the DMA**, not by the producer: the ring holds up to 20,160 samples = 1.23 s of already-decoded
// audio, so scaling the producer's output dims audio that has not been queued to play yet.
// `take_chunk` fills the ping-pong slot that starts playing on the next `rearm_dma`, so scaling that
// slot in the same critical section fades exactly what is about to be heard and leaves the ring's
// contents untouched — which is what makes a pause resumable with no seek (the probe has the
// producer-side shape because it was measuring a transition, not serving a listener).

/// The hand-off fade window, in samples: **256** = 15.6 ms at the album's 16,384 Hz.
///
/// §4.4's shape and §6's arithmetic: one gain step per sample, `>> 8` rather than a divide, monotone
/// by construction. It must finish inside ONE DMA buffer so that buffer is the one the chain lets
/// play out before freezing — the app's chunk is 560 samples and its geometry asserts
/// `FADE_SAMPLES < CHUNK_SAMPLES`.
pub const FADE_SAMPLES: i32 = 256;

/// The feeder periods a fade costs before a transition may take effect: **2**.
///
/// The first is the period whose hand-off slot carries the ramp (it *starts* playing at that
/// boundary); the second is the one served at the next boundary, when the ramped buffer has finished
/// playing and [`Transport::note_hand_off`] lets [`Transport::tick`] proceed. A transition that fired
/// after one period would stop the chain on the fade it exists for and cut the previous chunk
/// mid-waveform — §4.4's "fades, never a mid-waveform splice". The deferral is why every transition
/// is one period slower than the fade itself, and why the ring still holds at most
/// `RING_SAMPLES − FADE_PERIODS × CHUNK` when a skip truncates it.
pub const FADE_PERIODS: u32 = 2;

/// Full gain: a byte passes unchanged (`k / 256` == 1).
pub const GAIN_UNITY: i32 = 256;

/// The skip's silence cover, in chunks: **4** — §4.4 step 5's re-prime depth (D2).
///
/// Chosen from measured evidence, not guessed: the probe's deep-arm sweep recorded `to_new =
/// prime + 1` periods with `underflows = 0` at **every** prime including 2
/// (`examples/album_probe/README.md` → "The deep arm", truncate rows `:319-322`), so 4 sits inside
/// witnessed territory with 2× margin over the shallowest arm tested. Four chunks = 2,240 samples =
/// **171 ms** of silence between tracks, well inside §4.4's ≤ ~0.6 s observable, where prime 16
/// (581 ms) sits at its edge.
///
/// The observable does not decide this number — with a truncate in the ring the queue term is gone,
/// so `to_new` is linear in the prime and only binds past 16. What decides it is the silence a
/// listener hears between two tracks: this is a **product choice about the gap**, not a budget.
/// Q15's note applies to the ramp at the end of it: a clip that starts loud is the better transient
/// test, and this fixture's clips start in the master's quietest 50 ms window.
///
/// 171 ms is this cover alone, and it is **not** the whole wait a listener feels: add
/// [`take_to_handoff_bound`] on the front and the new track's own `head_silence` on the back.
pub const SKIP_PRIME_CHUNKS: u32 = 4;

/// The bound on a skip's **take → hand-off** latency, in feeder periods, for a run whose worst
/// main-loop pass measured `pass_periods` periods: **the pass's own length plus one, floored at the
/// deferral**.
///
/// The name predates the three-legged split; the value bounds only the leg the frame loop owns, from
/// the main loop taking the post to the hand-off's critical section — the press→detect and
/// detect→take legs are in front of it and are the ROM's `reaction` term.
///
/// **The derivation, and why this is a function and not the constant it replaced.** The post is
/// taken at the top of a pass, the feeder spends one period fading (§4.4 step 1 — `FADE_SAMPLES` <
/// `CHUNK_SAMPLES`, so it completes inside that one chunk), and the transition then waits one more
/// period so that chunk is played out before the hand-off ([`FADE_PERIODS`]). So the wait is the
/// number of period boundaries the pass crossed, plus that deferral — and nothing else, because the
/// decision is taken at the pass's end. A pass is one producer frame of decode plus the pass's own
/// work, and *how many cycles a frame costs belongs to the machine*: mGBA models no cartridge wait
/// states (R3), so the old fixed `3` was 1.85–1.9 periods of emulator-measured decode rounded up,
/// and a real cart's longer pass would fail a correct run (R13 — a red must not be louder than its
/// witness). The caller therefore measures the pass itself — the ROM times every pass on the audio
/// clock and takes the worst (`Input::pass_periods`), the host model takes it as a parameter — and
/// this bound moves with the machine instead of against it.
///
/// At the emulator's measured 2-period pass the value is the historical **3**, so the gate keeps the
/// same headroom where it was measured, and a 2× pass (4 periods) yields 5 rather than failing. It
/// still fails a loop that stopped serving the deferral latch or needed two extra passes to hand
/// off: those add whole passes to the wait and no measured pass length explains them.
///
/// The other two terms of the listener's wait are not this one's: the cover is [`SKIP_PRIME_CHUNKS`]
/// + 1 = 5 periods (`content_at`), and the track's own lead-in is `head_silence` — 8 periods on both
/// full-master tracks in the gate fixture, 0 on its clips.
#[must_use]
pub const fn take_to_handoff_bound(pass_periods: u32) -> u32 {
    let crossed = pass_periods + 1;
    if crossed < FADE_PERIODS {
        FADE_PERIODS
    } else {
        crossed
    }
}

/// The bound on a pause's **action → freeze** leg, in feeder periods, for a run whose worst main-loop
/// pass measured `pass_periods`: **[`FADE_PERIODS`] + 1 period of boundary alignment + one pass**.
///
/// **This is the leg §4.4 never bounded.** The plan's D2 chain bounds a skip's `taken → hand-off`
/// (`take_to_handoff_bound`) and bounds the press legs against the run's own worst pass, and the ROM
/// has always *recorded* the pause equivalent — `PauseRecord::fade_periods`, the feeder's entry count
/// at the toggle subtracted from its entry count at the freeze, in the audio clock's own unit, the
/// machine-independent one (`audio_clock.md`) — and compared it to nothing. So the pause rows in the
/// gate passed on `gain == 0 && dropped == 0`: both arms of a clock-delta tautology, as the gate's own
/// summary says. A run that took ten periods to reach `Step::Freeze` would have printed the same
/// `[PASS]`, and the only witness that the pause is *instant* was a human reading `fade_periods=2` off a
/// log. That is this repository's own "a green that names nothing is not a green" with the polarity
/// reversed: the number *was* printed, and nothing read it. A leg that is recorded and never bounded is a
/// leg that can only fail loudly enough for a person to happen to be looking.
///
/// **The derivation, term by term.** The front half is the same deferral the skip's bound names, and
/// for the same reason: the ramp is confined to one chunk (`FADE_SAMPLES` < `CHUNK_SAMPLES`), so one
/// period hands over the chunk carrying it and [`FADE_PERIODS`] is the period count at which the
/// deferral's latch (`Transport::served_silent_tail`) is allowed to open; +1 is the same boundary
/// alignment the sibling allows itself, since the latch opens on a period edge and the leg is measured
/// from a toggle that lands anywhere inside one. The back half is what makes this bound *not* the
/// sibling's value: **the pause's transition is performed by the main loop, and the skip's is
/// performed by the period ISR.** `Step::Freeze` is only ever observed from `tick()` inside a pass, so
/// the loop's next turn is inside this leg — and one pass is exactly what bounds that turn. The skip's
/// hand-off needs no such term because the hand-off runs on the ISR's own cadence.
///
/// At this repo's measured pass (3 periods) the value is **6**; the legs measured **2** — the
/// mechanism's floor, because a pass during `Pausing` is cheap (`State::feeds()` is false, so the
/// producer is not decoding and the loop's turn comes quickly). The headroom is stated rather than
/// hidden: this bound turns an *unbounded* leg into a bounded one, and it fails a ramp that never
/// reaches silence, a lost `served_silent` latch, or a loop that stopped calling `tick()`; it does not
/// by itself claim a pause is instantaneous. Raising a threshold is not a bound, so nothing here is a
/// `const` except the two numbers the mechanism itself owns (`FADE_PERIODS` and the alignment period),
/// and the pass term is the same measurement `take_to_handoff_bound` takes — the ROM's
/// `Input::pass_periods`, the host model's `pass_periods` parameter — never this emulator's history.
#[must_use]
pub const fn pause_to_freeze_bound(pass_periods: u32) -> u32 {
    FADE_PERIODS + 1 + pass_periods
}

/// One FIFO byte in the form [`fifo_byte`](gba_playback::playback::fifo_byte) produces, scaled by
/// `k / 256`.
///
/// That function is the authority this must match, and it is `(sample >> 8) as u8`, so a FIFO byte
/// is **signed 8-bit around zero** — the hardware's 8-bit FIFO output — which means silence is
/// `0x00` and `0x80` is full-scale negative, never a centre. That is why the probe's all-zero chunks
/// read as silence on the wire, and it is why a fade to `k = 0` must land on `0x00`: `0x80` would be
/// a jump to a DC rail, i.e. the pop the fade exists to avoid.
///
/// `>> 8` of a sign-extended product is `* k / 256` without a divide (§6), and the `i8` cast is
/// `fifo_byte`'s `as u8` in reverse.
#[inline]
#[must_use]
pub fn scale_byte(byte: u8, k: i32) -> u8 {
    ((i32::from(byte as i8) * k) >> 8) as i8 as u8
}

/// The gain applied to the ping-pong slot between `take_chunk` and `rearm_dma`: a value in
/// `0..=256` and a per-sample step of −1, 0 or +1.
///
/// The steady state is `k = 256, step = 0` and [`apply`](Self::apply) returns immediately, so an
/// unfaded period is the plain copy it always was. A fade costs one multiply, shift and clamp per
/// channel-sample over one chunk — a handful of periods per transition, and no work at all between
/// them.
///
/// Monotone by construction: `k` only moves by `step`, is clamped to `0..=256`, and only a caller
/// command changes `step`, so a ramp cannot reverse inside a chunk and cannot overshoot either rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandOffGain {
    k: i32,
    step: i32,
    /// Hand-off chunks still to be handed over at gain zero before the ramp-in starts: a skip's
    /// silence cover. Zero in every state but a skip.
    hold: u32,
}

impl HandOffGain {
    /// Full gain, not ramping: the steady state.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            k: GAIN_UNITY,
            step: 0,
            hold: 0,
        }
    }

    /// The gain the next sample would be scaled by (`0..=256`).
    #[must_use]
    pub const fn gain(&self) -> i32 {
        self.k
    }

    /// Whether the next slot passes byte-for-byte unchanged.
    #[must_use]
    pub const fn is_transparent(&self) -> bool {
        self.k == GAIN_UNITY && self.step == 0 && self.hold == 0
    }

    /// Whether the gain has reached zero — the pause contract's freeze point.
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        self.k == 0
    }

    /// Whether the gain is back at full scale — a resume's, or a skip's ramp-in's, completion point.
    ///
    /// The `hold` term matters: while a skip's silence cover is still being handed over the gain is
    /// at zero, so this cannot be true, and a caller that used it to mean "the transition is over"
    /// cannot end the transition on a cover chunk.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.k == GAIN_UNITY && self.hold == 0
    }

    /// Hand-off chunks the gain will hold at zero before ramping in (a skip's cover remainder).
    #[must_use]
    pub const fn held_chunks(&self) -> u32 {
        self.hold
    }

    /// Fade in **after** `chunks` more hand-off chunks, which are the skip's silence cover.
    ///
    /// From silence, not from wherever the gain was: step 6 is a ramp up from nothing, and the cover
    /// chunks between here and the new track are zeros, so the gain is pinned at zero and each cover
    /// chunk decrements the hold in [`apply`](Self::apply). The chunk that spends the last count is
    /// still silence (it is a cover chunk), and the ramp starts on the chunk **after** it — the new
    /// track's first samples — which is what keeps the fade-in on the audio a listener hears.
    pub fn fade_in_after(&mut self, chunks: u32) {
        self.k = 0;
        self.step = if chunks == 0 { 1 } else { 0 };
        self.hold = chunks;
    }

    /// Fade toward silence from wherever the gain currently stands (a pause).
    ///
    /// From the current `k`, not from full: a pause pressed during a fade-in must keep falling
    /// rather than jump back up to 256, which would be a click.
    pub fn fade_out(&mut self) {
        self.step = -1;
    }

    /// Fade back to full from wherever the gain currently stands (a resume).
    pub fn fade_in(&mut self) {
        self.step = 1;
    }

    /// Hold the current gain (no ramp) — the state a resume leaves behind once the ramp has
    /// completed.
    pub fn hold(&mut self) {
        self.step = 0;
    }

    /// Scale one hand-off slot in place — both channels, one period's worth of samples — and advance
    /// the ramp past it.
    ///
    /// Call it on the buffers `take_chunk` just filled and before they are re-armed: the bytes it
    /// touches are the bytes about to play, and the ring they came from is not read or written here.
    pub fn apply(&mut self, left: &mut [u8], right: &mut [u8]) {
        // A skip's cover chunk is handed over at gain zero, and the ramp-up is armed for the chunk
        // *after* the last one — never on it, or the new track would arrive at full gain.
        let covering = self.hold > 0;
        if covering {
            self.hold -= 1;
        }
        if !self.is_transparent() {
            let n = left.len().min(right.len());
            let mut k = self.k;
            for (l, r) in left[..n].iter_mut().zip(right[..n].iter_mut()) {
                *l = scale_byte(*l, k);
                *r = scale_byte(*r, k);
                k = (k + self.step).clamp(0, GAIN_UNITY);
            }
            self.k = k;
        }
        if covering && self.hold == 0 {
            self.step = 1;
        }
        // A fade that has reached the floor is finished, not still falling: the next slot is scaled
        // at a constant zero rather than re-playing the ramp. The output is identical either way
        // (the clamp already held `k` at 0), so this is the schedule saying what actually happened.
        if self.k == 0 && self.step < 0 {
            self.step = 0;
        }
    }
}

impl Default for HandOffGain {
    fn default() -> Self {
        Self::full()
    }
}

/// The transport's states. See the module docs for each one's entry and exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The chain is armed and the hand-off is at full gain.
    Playing,
    /// A fade-out has been issued and the chain is still running; the gain is falling to zero.
    Pausing,
    /// The fade reached silence and the chain is frozen. The ring and the decoder are untouched.
    Paused,
    /// The chain has been primed and re-armed with the gain ramping up; the ramp completes inside
    /// the primed chunk, so this state lasts one period.
    Resuming,
    /// A skip's fade-out is in flight: the chain still runs and is playing the old track, ramping
    /// down. The hand-off (truncate + cover) has not happened yet, so the ring is untouched.
    Skipping,
    /// The hand-off happened: the ring holds the silence cover and the gain is held at zero for
    /// exactly those chunks, then ramps in on the new track's first one. The producer feeds in this
    /// state (that is what puts the new track behind the cover).
    Repriming,
}

impl State {
    /// Whether the producer may push a frame in this state.
    ///
    /// A pause must not be fed another frame — that is D3's whole point — while a skip is the
    /// opposite: the new track's audio has to reach the ring behind the silence cover, or the cover
    /// runs out and the feeder starves. [`State::Repriming`] is therefore a feeding state, and this
    /// predicate is the one place that judgement lives.
    #[must_use]
    pub const fn feeds(self) -> bool {
        matches!(self, State::Playing | State::Repriming)
    }
}

/// What the main loop must do to the hardware for one decision. Both the per-command and the
/// per-pass decisions return one of these, so there is a single place in the ROM that performs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing to do this pass.
    Idle,
    /// The fade-out has been issued: keep running the feeder until [`Step::Freeze`] arrives.
    FadeOut,
    /// The hand-off gain is at zero — stop the timer chain, then both sound DMAs. The transport has
    /// already moved to [`State::Paused`], so the step cannot be issued twice.
    Freeze,
    /// Re-prime the hand-off from the ring head and re-arm the chain; the gain is already ramping
    /// in, and the primed chunk is the ramp's first chunk.
    Prime,
    /// A skip's fade reached silence and the ramped buffer has been played out: perform the
    /// hand-off **now**, and in one critical section —
    /// [`Transport::begin_reprime`] (which arms the gain's cover hold), `RingFeed::truncate()`, the
    /// producer's `finished` reset and the cover's first whole chunk. The state stays
    /// [`State::Skipping`] until that section runs, so no feeder period can land between the hold
    /// being armed and the ring being emptied.
    SkipHandOff,
    /// A toggle arrived while a transition was in flight. The contract has no cancel — reversing a
    /// fade half-way would be a click of its own — so it is ignored, and the caller counts it.
    Ignored,
}

/// The transport: one state and the one gain that is applied at the DMA hand-off.
///
/// `const`-constructible and `Copy`-free (it is mutated from both the main loop and the feeder), so
/// the ROM holds one in a static and the host model holds one as a field. There is no third place
/// the truth about "paused?" could live.
pub struct Transport {
    state: State,
    gain: HandOffGain,
    /// Whether a whole hand-off has been served since the gain reached silence.
    ///
    /// The ramp is applied to the slot handed to the DMA, and that slot *starts* playing at the
    /// `rearm_dma` that hands it over. So "the gain is silent" means the chunk that ends silent has
    /// just begun — freezing or truncating there throws the fade away and cuts the previous chunk
    /// mid-waveform (§4.4's "fades, never a mid-waveform splice" forbids exactly that). The feeder
    /// sets this at the next period it serves, which is the boundary at which the ramped buffer has
    /// finished playing; [`tick`](Self::tick) refuses to transition until it is set. Cleared
    /// whenever a fade starts or the state changes, so it can never shorten a later transition.
    served_silent: bool,
}

impl Transport {
    /// Playing, at full gain, at a track's start: the state after the boot prime.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: State::Playing,
            gain: HandOffGain::full(),
            served_silent: false,
        }
    }

    /// The current state (the log prints it, and the main loop branches on it).
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// The hand-off gain as it stands: what the next sample will be scaled by, and the ramp's
    /// direction. Copied out so a caller never holds a reference across a critical section.
    #[must_use]
    pub const fn gain(&self) -> HandOffGain {
        self.gain
    }

    /// Scale the DMA slot the feeder has just filled, in place, and advance the ramp.
    ///
    /// This is the whole of the per-period gain cost, called between `take_chunk` and `rearm_dma`:
    /// a transparent gain returns immediately, so a period with no fade in flight costs one compare.
    pub fn apply(&mut self, left: &mut [u8], right: &mut [u8]) {
        self.gain.apply(left, right);
    }

    /// The feeder served one period: one `take_chunk` and one `rearm_dma`.
    ///
    /// Call it after the chunk is taken and **before** [`apply`](Self::apply) — it is asking whether
    /// the previous hand-off's buffer was already silent, i.e. whether the ramped buffer has now
    /// finished playing. One compare and one store, no multiply: the timer1 feeder's per-period
    /// work stays inside `REARM_GRACE_SAMPLES`.
    pub fn note_hand_off(&mut self) {
        if self.gain.is_silent() {
            self.served_silent = true;
        }
    }

    /// Whether a period has been served since the gain reached silence.
    #[must_use]
    pub const fn served_silent_tail(&self) -> bool {
        self.served_silent
    }

    /// One command: `toggle` is "the listener pressed pause/resume this pass".
    ///
    /// A toggle from [`State::Playing`] starts the fade-out; a toggle from [`State::Paused`] starts
    /// the ramp-in and asks for the re-prime. Any other toggle is [`Step::Ignored`].
    pub fn toggle(&mut self, toggle: bool) -> Step {
        if !toggle {
            return Step::Idle;
        }
        match self.state {
            State::Playing => {
                self.state = State::Pausing;
                self.gain.fade_out();
                self.served_silent = false;
                Step::FadeOut
            }
            State::Paused => {
                self.state = State::Resuming;
                self.gain.fade_in();
                self.served_silent = false;
                Step::Prime
            }
            // A transition in flight, or a skip's: the contract has no cancel, and a pause pressed
            // during a skip would have to choose between the two fades. Ignored, and counted.
            State::Pausing | State::Resuming | State::Skipping | State::Repriming => Step::Ignored,
        }
    }

    /// One skip request: a direction command while a track is playing.
    ///
    /// From [`State::Playing`] this issues the ramp-out and returns [`Step::FadeOut`]; from every
    /// other state it is [`Step::Ignored`], and the caller counts it (see the module docs for why a
    /// skip while paused is not defined rather than merely unimplemented).
    pub fn skip(&mut self) -> Step {
        if self.state == State::Playing {
            self.state = State::Skipping;
            self.gain.fade_out();
            self.served_silent = false;
            Step::FadeOut
        } else {
            Step::Ignored
        }
    }

    /// The skip's hand-off, first half: move to [`State::Repriming`] and hold the gain at zero for
    /// the `cover_chunks` chunks the caller is about to push as silence.
    ///
    /// **Call this inside the same critical section as the truncate.** It is what arms
    /// [`HandOffGain::fade_in_after`], and a feeder period landing between the two would consume a
    /// cover count on a chunk of the old track — starting the ramp one chunk early, which puts the
    /// new track's first samples on the wire at full gain.
    ///
    /// Returns `false` if the transport was not [`State::Skipping`], so the caller cannot re-arm a
    /// hold it has already spent.
    pub fn begin_reprime(&mut self, cover_chunks: u32) -> bool {
        if self.state != State::Skipping {
            return false;
        }
        self.state = State::Repriming;
        self.gain.fade_in_after(cover_chunks);
        self.served_silent = false;
        true
    }

    /// One main-loop pass's decision, independent of commands: has the fade finished, is a skip
    /// ready to hand off, and has the ramp back returned to full gain?
    ///
    /// `Freeze` moves the state to `Paused` here rather than in a second call, so the caller cannot
    /// perform the freeze and forget to record it — or record it and forget to perform it. The skip
    /// hand-off is deliberately *not* like that: see [`Step::SkipHandOff`].
    ///
    /// **Neither fires on `is_silent()` alone.** The gain is zero at the first `apply`, i.e. at the
    /// hand-off of the chunk that ends the ramp, and the DMA is at sample 0 of that chunk when the
    /// main loop next passes. Firing there stops the chain (a pause) or truncates the ring (a skip)
    /// on the ramp that was meant to be heard, so the deferral is the condition: a whole period must
    /// have been served since the gain reached silence, which is exactly the boundary at which the
    /// ramped buffer finished playing (`Transport::note_hand_off`). One rule, both transition arms —
    /// the track-end advance goes through `skip`/`SkipHandOff` and gets it without a second path.
    pub fn tick(&mut self) -> Step {
        match self.state {
            State::Pausing if self.gain.is_silent() && self.served_silent => {
                self.state = State::Paused;
                self.served_silent = false;
                Step::Freeze
            }
            State::Skipping if self.gain.is_silent() && self.served_silent => Step::SkipHandOff,
            State::Resuming if self.gain.is_full() => {
                self.state = State::Playing;
                self.served_silent = false;
                Step::Idle
            }
            State::Repriming if self.gain.is_full() => {
                self.state = State::Playing;
                self.served_silent = false;
                Step::Idle
            }
            _ => Step::Idle,
        }
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}
