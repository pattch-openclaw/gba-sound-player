//! `transport.rs` — the playback transport, and §4.4's pause / resume contract.
//!
//! PR-3b, step ii-B2a. Skip lands in ii-B2b, so this module knows exactly four states and every one
//! of them is reachable: `Playing`, `Pausing`, `Paused`, `Resuming`. There is no `Skipping`, no
//! `TrackEnd` and no `Error` variant, because a state the caller cannot act on is a lie about what
//! the code can do.
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
//! | freeze | the main loop stops timer1, then timer0/timer2, then both sound DMAs | the next main-loop pass after silence |
//!
//! **`pause`:** ramp the hand-off gain to zero, then freeze — stop both sound DMAs and the timer
//! chain, halt the producer, and leave the ring exactly as it is.
//!
//! **`resume`:** re-prime both DMA buffers from the ring head and re-arm in the shipped order
//! (`enable_sound` → prime → `clear_timer1_if` → the already-installed handler →
//! `arm_audio_chain`), ramping the gain back up.
//!
//! Four consequences follow, and each is the reason a simpler-looking design was rejected:
//!
//! - **The position is exact.** Nothing is dropped, so no truncate, no seek and no re-decode is
//!   needed, and the resume continues on the sample the freeze stopped on.
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
/// by construction. It must finish inside ONE DMA buffer so the chain can be frozen at the next
/// period boundary rather than mid-ramp — the app's chunk is 560 samples and its geometry asserts
/// `FADE_SAMPLES < CHUNK_SAMPLES`.
pub const FADE_SAMPLES: i32 = 256;

/// Full gain: a byte passes unchanged (`k / 256` == 1).
pub const GAIN_UNITY: i32 = 256;

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
}

impl HandOffGain {
    /// Full gain, not ramping: the steady state.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            k: GAIN_UNITY,
            step: 0,
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
        self.k == GAIN_UNITY && self.step == 0
    }

    /// Whether the gain has reached zero — the pause contract's freeze point.
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        self.k == 0
    }

    /// Whether the gain is back at full scale — a resume's completion point.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.k == GAIN_UNITY
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
        if self.is_transparent() {
            return;
        }
        let n = left.len().min(right.len());
        let mut k = self.k;
        for (l, r) in left[..n].iter_mut().zip(right[..n].iter_mut()) {
            *l = scale_byte(*l, k);
            *r = scale_byte(*r, k);
            k = (k + self.step).clamp(0, GAIN_UNITY);
        }
        self.k = k;
    }
}

impl Default for HandOffGain {
    fn default() -> Self {
        Self::full()
    }
}

/// The four states of PR-3b-ii-B2a. See the module docs for each one's entry and exit.
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
}

impl Transport {
    /// Playing, at full gain, at a track's start: the state after the boot prime.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: State::Playing,
            gain: HandOffGain::full(),
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
                Step::FadeOut
            }
            State::Paused => {
                self.state = State::Resuming;
                self.gain.fade_in();
                Step::Prime
            }
            State::Pausing | State::Resuming => Step::Ignored,
        }
    }

    /// One main-loop pass's decision, independent of commands: has the fade finished, and has the
    /// ramp back returned to full gain?
    ///
    /// `Freeze` moves the state to `Paused` here rather than in a second call, so the caller cannot
    /// perform the freeze and forget to record it — or record it and forget to perform it.
    pub fn tick(&mut self) -> Step {
        match self.state {
            State::Pausing if self.gain.is_silent() => {
                self.state = State::Paused;
                Step::Freeze
            }
            State::Resuming if self.gain.is_full() => {
                self.state = State::Playing;
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
