//! `keys.rs` — the input seam: per-frame edges, autorepeat, debounce, and §5.3's control map.
//!
//! This is the first code in this repository that reads a button, so it is also the module that
//! says what "reading a button" means here. The **state machine is platform-free**: it takes a
//! `u16` mask of held buttons and returns the frame's events, so the host twin
//! (`tests/album_seam_host.rs`) drives exactly the code the ROM drives, and a scripted key log is
//! the same input a human's thumb is (§5.3, §13 AP-1). Nothing in [`Keys`], [`now_playing_action`]
//! or [`cursor_action`] names `agb`; only the two [`KeySource`] implementations below the line do,
//! and each of those is three lines over a mask.
//!
//! ## What agb gives, and what this module adds
//!
//! agb 0.25's `ButtonController` gives the held set, an edge predicate
//! (`is_just_pressed`) and tri-state directions (`x_tri`/`y_tri`). It does **not** give repeat,
//! debounce, or any notion of an action. This module adds exactly those three and nothing else:
//!
//! | concern | rule | why |
//! |---|---|---|
//! | debounce | a change is accepted once [`DEBOUNCE_FRAMES`] consecutive frames agree | a real button bounces for a few ms; a bounced *press* read as press-release-press is two screen transitions instead of one |
//! | edges | `pressed = held & !previous_held`, `released = previous_held & !held`, both from the **debounced** set | an edge is a transition the application may act on, not a transition the contact did |
//! | autorepeat | first repeat [`REPEAT_DELAY_FRAMES`] after the press edge, then every [`REPEAT_INTERVAL_FRAMES`] | holding a direction down walks a track list (§5.2); a screen that needs a click per row is not a screen |
//!
//! ## §5.3's map, as code
//!
//! | button | now playing | track list / menu |
//! |---|---|---|
//! | Start, A | [`Action::PauseToggle`] | (Phase 2: confirm) |
//! | Right, R | [`Action::NextTrack`] | [`Cursor::Down`] |
//! | Left, L | [`Action::PrevTrack`] | [`Cursor::Up`] |
//! | Down | [`Action::RestartTrack`] | [`Cursor::Down`] / page down |
//! | Select | [`Action::SelectScene`] | (none — §5.3's scene cycle, 2b PR-(3)) |
//! | Up | *(nothing, §5.3's `—`)* | [`Cursor::Up`] / page up |
//!
//! Two properties of that table are *rules*, not conveniences, and both are enforced here rather
//! than left to the caller:
//!
//! - **Nothing is time-dependent.** There is no "Left twice within 3 s" chord: §4.4 replaced it
//!   with Down = restart precisely so no rule needs an audible-position ledger. The only frame
//!   counts in this module are repeat and debounce, and neither changes *what* an edge means.
//! - **L + R + Start is the BIOS's**, never the application's. There is no `Action` for the chord;
//!   [`now_playing_action`] cannot produce one, and while all three are held [`Keys`] reports no
//!   events at all (see [`SOFT_RESET_CHORD`]). What this cannot do — and says so — is stop the
//!   *component* edges of a chord that is being assembled one button at a time: pressing L, then R,
//!   then Start over three frames delivers L's edge before the chord exists. Suppressing that would
//!   need a time window, which the row above forbids.
//!
//! ## The map above is placeholder behaviour, not the design (2026-11-18, PR-3b review)
//!
//! Four actions that reach the transport directly — next, previous, restart — are what the *now
//! playing* screen needs and all that Phase 1 can witness without a single screen drawn. They are a
//! stand-in. The replacement is foreseeable, and it is not a bigger enum: once §5.2's screens exist
//! (Phase 2's A11 track list and A12 menus), the D-pad navigates **those**, and "Right" stops
//! meaning *next track* and starts meaning *whatever the focused screen does with Right* — cursor
//! down in a list, page down in a menu, nothing at all on a title card. Different screens will want
//! different verbs, and the same button will mean different things in different contexts.
//!
//! So the layer that decides that belongs **above** this module, owning the focused screen and
//! dispatching a context-dependent action. Two consequences for this module, cheap now and expensive
//! to retrofit:
//!
//! - **Do not add a variant per screen.** [`Action`] is the now-playing map. A UI layer defines its
//!   own action type and composes with this one; growing this enum to cover menus would put every
//!   screen's vocabulary in the one module that has to stay platform-free and testable with no
//!   screen in it — and would make the ISR's job depend on which screen is up.
//! - **Keep the halves separable.** What that layer needs from here is already here: per-frame
//!   edges, repeat, debounce, and a mask. Nothing in [`Keys`] or [`Sampler`] may learn what a screen
//!   is; this module keeping its ignorance is what lets a menu's key map be a table rather than a
//!   new branch in the vblank handler.
//!
//! Deliberately **not** built here, because nothing in these PRs consumes it: a context/action-map
//! trait, a notion of a focused screen, or a key→action table per screen. That layer should land
//! with A11/A12 against a real screen, not as an interface with no implementor.
//!
//! ## The two sources, and the one the gate can witness
//!
//! [`KeySource`] supplies one mask per video frame. [`HARDWARE`](Hardware) reads `REG_KEYINPUT`
//! (active-low) with a single volatile load — `agb`'s own address for it is
//! `agb-0.25.0/src/input.rs:82` — because the sample now happens in the vblank handler and the whole
//! handler is measured against the audio clock (§4.5, PR-3b-ii-B1). [`Replay`](Replay) — feature
//! `input-replay`, **off by default** — replays a compiled-in `(frame, mask)` script through
//! `update_with_state` (`agb-0.25.0/src/input.rs:145`), the injection hook agb provides for exactly
//! this.
//!
//! REPLAY exists because `mgba-test-runner` cannot press a key: the installed binary exposes only a
//! positional ROM argument, and no key-log, stdin, env or socket path is reachable from it. So the
//! scripted log is replayed **at the keys layer** — the same state machine the hardware path drives
//! — and the ROM always prints which source it used. A REPLAY run is **not** evidence that a
//! physical press works; that is AP-7's, on a cartridge. The caller must never imply otherwise, and
//! the log lines the ROM prints are worded to make it impossible to.
//!
//! ## Where the sample runs
//!
//! Not in the main loop, since PR-3b-ii-A measured why that cannot work: one producer frame is
//! 1.04–1.09 M cycles of uninterruptible decode — 3.7–3.9 video frames — so a loop can observe the
//! keypad at most once per producer frame. The sample therefore lives in the vblank handler, and
//! [`Sampler`] is that half: one call per video frame, one pending action stamped with the frame it
//! was detected on, and a [`take`](Sampler::take) for the application. It is platform-free like the
//! rest of the machine, so the host twin drives the same sample-then-take path the ROM does.

/// Button bits, in agb 0.25's `Button` discriminant order (`agb-0.25.0/src/input.rs:59-83`):
/// `A = 1 << 0` … `L = 1 << 9`. Keeping the same layout is what makes the arm half of this module a
/// pass-through rather than a translation table.
pub const BTN_A: u16 = 1 << 0;
/// See [`BTN_A`]. Reserved for the game (§5.3).
pub const BTN_B: u16 = 1 << 1;
/// See [`BTN_A`]. "Give the cart's CPU back" (§5.3) — not bound in this PR.
pub const BTN_SELECT: u16 = 1 << 2;
/// See [`BTN_A`]. Pause ⇄ resume, with [`BTN_A`].
pub const BTN_START: u16 = 1 << 3;
/// See [`BTN_A`]. Next track, with [`BTN_R`]; cursor down in a list.
pub const BTN_RIGHT: u16 = 1 << 4;
/// See [`BTN_A`]. Previous track, with [`BTN_L`]; cursor up in a list.
pub const BTN_LEFT: u16 = 1 << 5;
/// See [`BTN_A`]. Cursor up in a list only.
pub const BTN_UP: u16 = 1 << 6;
/// See [`BTN_A`]. Restart the current track — the §4.4 replacement for a time-dependent chord.
pub const BTN_DOWN: u16 = 1 << 7;
/// See [`BTN_A`]. Next track, with [`BTN_RIGHT`].
pub const BTN_R: u16 = 1 << 8;
/// See [`BTN_A`]. Previous track, with [`BTN_LEFT`].
pub const BTN_L: u16 = 1 << 9;

/// Every bit [`Keys`] will look at. `KEYINPUT`'s remaining bits are not buttons (the register is
/// active-low and its top bits read as set), so a mask outside this set is a caller bug, not data.
pub const BUTTON_MASK: u16 = 0x03FF;

/// The BIOS soft-reset chord: L + R + Start (§5.3). Held as a whole, it is the console's, not
/// ours: [`Keys`] reports nothing while it is down and no [`Action`] maps to it.
pub const SOFT_RESET_CHORD: u16 = BTN_L | BTN_R | BTN_START;

/// Frames a raw state must persist before it becomes the held set: **two**, i.e. one frame of
/// confirmation.
///
/// A GBA button's contact bounce is a few milliseconds — far inside one 16.7 ms frame at the
/// measured 280,896-cycle video period (AP-0) — so two frames is one frame more than bounce needs
/// and one frame less than a human can perceive as lag. The cost is stated rather than hidden: a
/// scripted or physical press observed at frame `F` produces its application edge at `F + 1`.
pub const DEBOUNCE_FRAMES: u8 = 2;

/// Frames from the press edge to the first autorepeat: **24** = 402 ms at the measured 280,896-cycle
/// video period. Chosen as the usual keyboard-ish delay — long enough that a deliberate single step
/// never doubles, short enough that holding a direction feels like it started moving.
pub const REPEAT_DELAY_FRAMES: u16 = 24;

/// Frames between autorepeats after the first: **8** = 134 ms. Twenty rows of a §5.2 track list in
/// 0.13 s per row is a scan, not a wait.
pub const REPEAT_INTERVAL_FRAMES: u16 = 8;

// Two constants that a future edit could silently reverse into a divide-free violation of their own
// intent: the interval must be shorter than the delay (otherwise the reset below would not schedule
// a second repeat), and the delay must exceed the interval so that reset is positive.
const _: () = assert!(REPEAT_INTERVAL_FRAMES + 1 <= REPEAT_DELAY_FRAMES);
const _: () = assert!(DEBOUNCE_FRAMES >= 1);
const _: () = assert!(BUTTON_MASK & SOFT_RESET_CHORD == SOFT_RESET_CHORD);

/// What a *press* means while a track is playing (§5.3's "Now playing" column). Menus are Phase 2
/// and get [`Cursor`] instead; the plumbing is here, the screens are not.
///
/// **Placeholder behaviour, not the design.** These four variants act on the transport directly
/// because there is no screen here to act on. This is not a UI action vocabulary and must not grow
/// into one: when §5.2's screens exist, a focused-screen layer *above* this module decides what a
/// D-pad press means in that context, and this enum stays exactly what it is — the now-playing map.
/// See the module's "The map above is placeholder behaviour" section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start or A: pause ⇄ resume. The faded pause/resume itself is §4.4's and lands in PR-3b-ii.
    PauseToggle,
    /// Right or R: next track, wrapping at the album ends.
    NextTrack,
    /// Left or L: previous track, wrapping at the album ends.
    PrevTrack,
    /// Down: restart the current track. Not a repeat action — see [`KeyFrame::repeated`].
    RestartTrack,
    /// Select: cycle the scene (§5.3, §5.2's scene registry). A scene change, not a
    /// transport state — the card is `Select`-reachable and audio keeps playing (2b PR-(3),
    /// Q20 (ii): the registry order is the order in `layout::SCENES`, filtered to the scenes
    /// this ROM implements).
    SelectScene,
}

/// The track list's cursor column (§5.3). Delivered for a *press* and, when a direction is held,
/// for a *repeat*; Phase 2 owns the list that consumes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// Up, or Left.
    Up,
    /// Down, or Right.
    Down,
}

/// §5.3's now-playing map over a mask of freshly pressed buttons.
///
/// The order is the table's row order, and that is the whole precedence rule: a frame carrying more
/// than one mapped edge (a thumb rolling across two buttons) resolves deterministically to the first
/// row rather than being ambiguous. It cannot happen in normal play, so the rule only has to be
/// stable, not fair — and it is `const`, so it is also a compile-time check of the map.
pub const fn now_playing_action(pressed: u16) -> Option<Action> {
    if pressed & (BTN_START | BTN_A) != 0 {
        Some(Action::PauseToggle)
    } else if pressed & (BTN_RIGHT | BTN_R) != 0 {
        Some(Action::NextTrack)
    } else if pressed & (BTN_LEFT | BTN_L) != 0 {
        Some(Action::PrevTrack)
    } else if pressed & BTN_DOWN != 0 {
        Some(Action::RestartTrack)
    } else if pressed & BTN_SELECT != 0 {
        // Last row: a transport button and Select on the same frame resolves to the
        // transport, which is the table's precedence rule and the safe one — a scene
        // change must never swallow a pause.
        Some(Action::SelectScene)
    } else {
        None
    }
}

/// §5.3's track-list cursor map over a mask of freshly pressed buttons.
///
/// Note what is *not* here: no `Confirm`, and no `Back`. §5.3 gives Start/A "confirm" and B "back"
/// in a list, and both belong to the screen that has something to confirm — inventing them in the
/// keys layer would put Phase 2's semantics in Phase 1's module. [`Cursor`] is only movement.
pub const fn cursor_action(pressed: u16) -> Option<Cursor> {
    if pressed & (BTN_DOWN | BTN_RIGHT) != 0 {
        Some(Cursor::Down)
    } else if pressed & (BTN_UP | BTN_LEFT) != 0 {
        Some(Cursor::Up)
    } else {
        None
    }
}

/// One video frame's worth of input, in the three forms a consumer needs.
///
/// `pressed` and `released` are *edges* and never repeat; `repeated` fires only for held directions,
/// and `held` is the debounced state after the edges were taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyFrame {
    /// The debounced held set this frame ([`BUTTON_MASK`]-limited).
    pub held: u16,
    /// Buttons that went down on this frame (the transition the application acts on).
    pub pressed: u16,
    /// Buttons that came up on this frame.
    pub released: u16,
    /// Directions whose autorepeat fires on this frame. Never a non-direction, and never
    /// [`BTN_DOWN`]'s now-playing meaning — see [`Keys::sample`].
    pub repeated: u16,
}

impl KeyFrame {
    /// The now-playing action for this frame's press, if any.
    #[must_use]
    pub const fn action(&self) -> Option<Action> {
        now_playing_action(self.pressed)
    }

    /// The track-list cursor step for this frame's press, if any.
    #[must_use]
    pub const fn cursor(&self) -> Option<Cursor> {
        cursor_action(self.pressed)
    }

    /// The cursor step for this frame's *repeat*, if any — the "hold to walk the list" half.
    #[must_use]
    pub const fn repeat_cursor(&self) -> Option<Cursor> {
        cursor_action(self.repeated)
    }
}

/// The direction bits that are singly held on their axis, i.e. the ones autorepeat may fire for.
///
/// A direction and its opposite both down reads as no direction at all, which is agb's own
/// `x_tri`/`y_tri` convention (`agb-0.25.0/src/input.rs:180-200`) and the right one for a list
/// cursor: a thumb straddling Left and Right should not scroll at all.
const fn repeatable(held: u16) -> u16 {
    let horizontal = if held & BTN_LEFT != 0 && held & BTN_RIGHT == 0 {
        BTN_LEFT
    } else if held & BTN_RIGHT != 0 && held & BTN_LEFT == 0 {
        BTN_RIGHT
    } else {
        0
    };
    let vertical = if held & BTN_UP != 0 && held & BTN_DOWN == 0 {
        BTN_UP
    } else if held & BTN_DOWN != 0 && held & BTN_UP == 0 {
        BTN_DOWN
    } else {
        0
    };
    horizontal | vertical
}

/// The platform-free input state machine: debounce, edges, autorepeat, chord suppression.
///
/// Consumes one mask per video frame and produces one [`KeyFrame`]. It holds no clock, no register
/// and no `agb` type, which is what lets the host twin drive a scripted sequence through the same
/// code the ROM runs. "Frame" here means "one call", and the caller owns the cadence — in the ROM
/// that is the vblank latch (§4.5).
pub struct Keys {
    /// The debounced held set.
    held: u16,
    /// The raw state currently being confirmed.
    candidate: u16,
    /// Consecutive frames `candidate` has been seen.
    candidate_frames: u8,
    /// The directions autorepeat is currently running for.
    repeat_dir: u16,
    /// Frames since the repeat burst's direction last changed, or since the last repeat was fired.
    repeat_frames: u16,
}

impl Keys {
    /// No buttons held, nothing repeating.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            held: 0,
            candidate: 0,
            candidate_frames: 0,
            repeat_dir: 0,
            repeat_frames: 0,
        }
    }

    /// The debounced held set as of the last [`sample`](Self::sample).
    #[must_use]
    pub const fn held(&self) -> u16 {
        self.held
    }

    /// Fold one frame's raw mask in and return what happened.
    ///
    /// The repeat counter runs off the **debounced** set, so a bounced release cannot restart a
    /// burst, and a burst's direction is the pair of singly-held axis directions, so pressing a
    /// second button does not stop a held one from repeating.
    ///
    /// One rule deserves its reason here. [`BTN_DOWN`] *is* a direction — it repeats, because in a
    /// track list it walks rows — but in "now playing" it means restart-current-track (§5.3), and a
    /// *repeating* restart would restart the track every 134 ms for as long as it is held. So the
    /// consumers differ, not the machine: `Down`'s now-playing binding takes
    /// [`KeyFrame::pressed`] (an edge, delivered once), and only a list takes
    /// [`KeyFrame::repeated`] / [`KeyFrame::repeat_cursor`]. The machine delivers both; the
    /// application picks. PR-3b-i binds neither, which is the whole point of this step.
    pub fn sample(&mut self, raw: u16) -> KeyFrame {
        let raw = raw & BUTTON_MASK;
        if raw == self.candidate {
            self.candidate_frames = self.candidate_frames.saturating_add(1);
        } else {
            self.candidate = raw;
            self.candidate_frames = 1;
        }
        let previous = self.held;
        if self.candidate_frames >= DEBOUNCE_FRAMES {
            self.held = self.candidate;
        }
        let held = self.held;

        let chord = held & SOFT_RESET_CHORD == SOFT_RESET_CHORD;

        let mut repeated = 0u16;
        let repeat_dir = if chord { 0 } else { repeatable(held) };
        if repeat_dir != self.repeat_dir {
            self.repeat_dir = repeat_dir;
            self.repeat_frames = 0;
        } else if repeat_dir != 0 {
            let next = self.repeat_frames.saturating_add(1);
            if next >= REPEAT_DELAY_FRAMES {
                repeated = repeat_dir;
                // Re-arm for the *interval*, not the delay: the delay only schedules the first
                // repeat. No modulo, so this stays a compare-and-store.
                self.repeat_frames = REPEAT_DELAY_FRAMES - REPEAT_INTERVAL_FRAMES;
            } else {
                self.repeat_frames = next;
            }
        }

        if chord {
            // The console's chord, not the application's: no edges at all leave this frame, so a
            // held L+R+Start cannot pause, skip or restart anything on its way to the BIOS reset.
            KeyFrame {
                held,
                pressed: 0,
                released: 0,
                repeated: 0,
            }
        } else {
            KeyFrame {
                held,
                pressed: held & !previous,
                released: previous & !held,
                repeated,
            }
        }
    }
}

impl Default for Keys {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------------------------------
// The ISR half: one sample per video frame, one slot for what it detected
// -------------------------------------------------------------------------------------------------

/// One action, stamped with the video frame the ISR that detected it was sampling.
///
/// `frame` is the seam's own count of sampled frames — one per [`Sampler::sample`] call — and that is
/// also the clock a [`Replay`] script's entries are stated in, because both count the same vblank
/// passes. A scripted edge's latency is then a subtraction rather than an inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detected {
    /// The video frame the action was detected on (0 is the first frame after installation).
    pub frame: u32,
    /// §5.3's now-playing meaning of that frame's press.
    pub action: Action,
}

/// The ISR half of the seam: run [`Keys`] once per video frame, and leave the newest action in a
/// single slot for the application to take.
///
/// The split is forced by a measurement, not by taste (§4.5, PR-3b-ii-A's finding): one producer
/// frame is 1.04–1.09 M cycles of uninterruptible decode — 3.7–3.9 video frames — so a main loop can
/// never sample the keypad once per frame. The vblank handler is the only code in this system that
/// runs at video rate, so it owns `source.sample()` → [`Keys::sample`] → post, and the application
/// owns [`take`](Self::take).
///
/// **Single slot, newest wins, and displacement is counted.** A consumer that is inside a producer
/// frame cannot take every action; [`dropped`](Self::dropped) says how many actions an older one was
/// displaced by, so "the application missed a press" is a number rather than a silence. A queue is
/// what an interrupt must not grow.
///
/// Nothing here divides, formats, allocates or reads a register, and nothing here names `agb`: it is
/// the same [`Keys`] machine the host twin drives, with a frame counter and a slot around it.
pub struct Sampler {
    keys: Keys,
    /// Video frames sampled, i.e. the index `sample` is about to stamp.
    sampled: u32,
    /// The newest action the application has not taken.
    pending: Option<Detected>,
    /// Actions displaced from the slot before a consumer took them.
    dropped: u32,
    /// Frames carrying a press edge, and frames carrying an autorepeat.
    presses: u32,
    repeats: u32,
}

impl Sampler {
    /// No frames sampled, nothing pending.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: Keys::new(),
            sampled: 0,
            pending: None,
            dropped: 0,
            presses: 0,
            repeats: 0,
        }
    }

    /// Fold one video frame's raw mask in and post its action, if that frame had one.
    ///
    /// One call is one video frame and **the caller owns that cadence** — in the ROM it is the vblank
    /// handler (§4.5). The index stamped on a posted action is this sampler's own count, incremented
    /// once per call, so two actions on consecutive frames are stamped `n` and `n + 1` and can never
    /// reach the consumer in the other order.
    pub fn sample(&mut self, raw: u16) -> KeyFrame {
        let frame = self.sampled;
        let events = self.keys.sample(raw);
        self.sampled = self.sampled.wrapping_add(1);
        if events.pressed != 0 {
            self.presses = self.presses.saturating_add(1);
        }
        if events.repeated != 0 {
            self.repeats = self.repeats.saturating_add(1);
        }
        if let Some(action) = events.action() {
            if self.pending.is_some() {
                self.dropped = self.dropped.saturating_add(1);
            }
            self.pending = Some(Detected { frame, action });
        }
        events
    }

    /// The newest action, if the application has not taken it yet.
    pub fn take(&mut self) -> Option<Detected> {
        self.pending.take()
    }

    /// The newest action without consuming it — what the ISR asks when it wants the frame stamp it
    /// has just posted.
    #[must_use]
    pub const fn pending(&self) -> Option<Detected> {
        self.pending
    }

    /// Video frames sampled.
    #[must_use]
    pub const fn sampled(&self) -> u32 {
        self.sampled
    }

    /// Frames carrying a press edge.
    #[must_use]
    pub const fn presses(&self) -> u32 {
        self.presses
    }

    /// Frames carrying an autorepeat.
    #[must_use]
    pub const fn repeats(&self) -> u32 {
        self.repeats
    }

    /// Actions displaced from the single slot before a consumer took them.
    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------------------------------
// The seam: one mask per video frame, from a keypad or from a script
// -------------------------------------------------------------------------------------------------

/// Supplies the held-button mask for one video frame.
///
/// Deliberately one method over a `u16`: an implementation can be the keypad, a compiled-in script
/// or a host test, and none of the three can change what a frame *means*. The caller owns the
/// cadence (one call per latched frame, §4.5) and feeds the result to [`Keys`].
pub trait KeySource {
    /// The held-button mask for the next frame.
    fn sample(&mut self) -> u16;

    /// The video frame at which the mask this source is delivering last changed, when the source
    /// knows it. A scripted log does; a keypad cannot, so the default is `None` — "not measurable",
    /// never "zero", which is what keeps the reaction latency a number only where it is a
    /// measurement.
    fn mask_changed(&self) -> Option<u32> {
        None
    }
}

/// A compiled-in `(frame, mask)` script: the gate's input, since `mgba-test-runner` cannot press a
/// key. Feature `input-replay`, **off by default** — the shipped artifacts are HARDWARE builds.
///
/// Compiled for the **host** unconditionally, so `tests/album_seam_host.rs` drives the real script
/// reader against the real [`Keys`] machine; on the arm it exists only under `input-replay`, so a
/// shipped image carries no script path at all. That split is what makes "the REPLAY arm delivers
/// these edges at these frames" a host-tested fact rather than a claim about the arm binary.
///
/// The script is a whole history, not a delta list: an entry says "from frame `F` the mask is `M`",
/// and a frame with no entry repeats the previous mask. Frames are the source's own count — one per
/// [`KeySource::sample`] call, i.e. one per **poll** — so an entry's index is the poll index and not
/// a video-frame number. The distinction is measurable rather than academic: §4.5's loop polls on
/// the frame latch (PR-3b-ii-A), so a poll happens at the first frame boundary the main loop is awake
/// for, but it is **not** awake inside a producer frame, whose pull+convert+push is ~3.7 video frames
/// at this build's decode cost. The ROM therefore prints its own cadence
/// (`cadence: video_frames=… input_polls=… ratio=… max_gap=…`) *and* the latch every edge landed on,
/// so neither number has to be assumed: the script's index is exact in polls, and the video frame it
/// arrives on is read off the log. A poll that arrives after several frames have passed applies every
/// entry that is due, in order, so no entry can be skipped and none can fire before its own poll.
///
/// On the arm build this goes through `ButtonController::update_with_state`, the same controller the
/// hardware arm drives, so "the scripted log is exactly the input the test harness feeds the ROM"
/// (§5.3) is a property of one code path rather than a claim about two.
#[cfg(any(feature = "input-replay", not(target_arch = "arm")))]
pub struct Replay {
    script: &'static [(u32, u16)],
    /// Index of the next script entry that has not been applied yet.
    next: usize,
    /// Frames this source has been sampled for (the script's clock).
    frame: u32,
    /// The mask in force.
    mask: u16,
    /// The mask last handed to the caller — the edge count's baseline.
    reported: u16,
    /// How many times a previously-unset button bit appeared (the delivered edge count).
    edges: u32,
    /// The script frame of the last entry applied — the mask transition's own frame, which is what
    /// a detection latency is measured from.
    changed: u32,
    #[cfg(target_arch = "arm")]
    controller: agb::input::ButtonController,
}

#[cfg(any(feature = "input-replay", not(target_arch = "arm")))]
impl Replay {
    /// A replay over `script`, which must be sorted by frame. Entries may not carry bits outside
    /// [`BUTTON_MASK`] — the ROM's script is a tracked literal, so that is a property of the
    /// literal rather than a runtime validation in a `no_std` binary with nowhere to report to.
    ///
    /// Not `const`: on the arm build this seeds a `ButtonController`, whose constructor reads
    /// `REG_KEYINPUT` (`agb-0.25.0/src/input.rs:392`) and so cannot run in a `const` context.
    #[must_use]
    pub fn new(script: &'static [(u32, u16)]) -> Self {
        Self {
            script,
            next: 0,
            frame: 0,
            mask: 0,
            reported: 0,
            edges: 0,
            changed: 0,
            #[cfg(target_arch = "arm")]
            controller: agb::input::ButtonController::new(),
        }
    }

    /// How many scripted entries the script carries (the boot line's `n`).
    #[must_use]
    pub const fn entries(&self) -> usize {
        self.script.len()
    }

    /// How many press edges have been delivered so far (the end line's `m`).
    #[must_use]
    pub const fn edges(&self) -> u32 {
        self.edges
    }

    /// The frame this source is currently reporting.
    #[must_use]
    pub const fn frame(&self) -> u32 {
        self.frame
    }

    /// The script frame of the entry currently in force: the frame the delivered mask last changed
    /// on. A press edge is detected one frame after this one (the debounce), which is the latency the
    /// ROM prints, so this is the number it is measured from.
    #[must_use]
    pub const fn changed(&self) -> u32 {
        self.changed
    }

    /// Advance the script's clock by one frame and return that frame's raw scripted mask.
    fn advance(&mut self) -> u16 {
        let frame = self.frame;
        while let Some(&(at, mask)) = self.script.get(self.next) {
            if at > frame {
                break;
            }
            self.mask = mask & BUTTON_MASK;
            self.changed = at;
            self.next += 1;
        }
        self.frame = frame.wrapping_add(1);
        self.mask
    }
}

#[cfg(any(feature = "input-replay", not(target_arch = "arm")))]
impl KeySource for Replay {
    fn sample(&mut self) -> u16 {
        let scripted = self.advance();
        // The controller is the delivery, and its answer is what the edge count is taken from — so
        // a mask mangled in the `update_with_state`/`is_pressed` round trip would change the count
        // the ROM prints against the script's own.
        #[cfg(target_arch = "arm")]
        let delivered = {
            self.controller.update_with_state(hw::state_of(scripted));
            hw::mask_of(&self.controller)
        };
        #[cfg(not(target_arch = "arm"))]
        let delivered = scripted;
        if delivered & !self.reported != 0 {
            self.edges += 1;
        }
        self.reported = delivered;
        delivered
    }

    fn mask_changed(&self) -> Option<u32> {
        Some(self.changed)
    }
}

#[cfg(target_arch = "arm")]
mod hw {
    //! The only `agb` types and the only register this module names: the keypad read, and the
    //! `ButtonState` round trip the REPLAY arm drives.

    use agb::input::Button;
    #[cfg(feature = "input-replay")]
    use agb::input::ButtonController;
    #[cfg(feature = "input-replay")]
    use agb::input::ButtonState;

    use super::{BTN_A, BTN_L, KeySource};
    #[cfg(feature = "input-replay")]
    use super::{BTN_B, BTN_DOWN, BTN_LEFT, BTN_R, BTN_RIGHT, BTN_SELECT, BTN_START, BTN_UP};

    /// The ten buttons against this module's bits. This table *is* the assertion that agb's
    /// discriminants and [`super`]'s constants agree; a drift in either is a wrong mask, so the
    /// compile-time check below pins the ones a table cannot. Only the REPLAY arm walks it — the
    /// keypad is one register read and needs no table.
    #[cfg(feature = "input-replay")]
    const BUTTONS: [(Button, u16); 10] = [
        (Button::A, BTN_A),
        (Button::B, BTN_B),
        (Button::Select, BTN_SELECT),
        (Button::Start, BTN_START),
        (Button::Right, BTN_RIGHT),
        (Button::Left, BTN_LEFT),
        (Button::Up, BTN_UP),
        (Button::Down, BTN_DOWN),
        (Button::R, BTN_R),
        (Button::L, BTN_L),
    ];

    const _: () = assert!(Button::A as u16 == BTN_A);
    const _: () = assert!(Button::L as u16 == BTN_L);

    /// The controller's held set as this module's mask — the REPLAY arm's delivery side.
    #[cfg(feature = "input-replay")]
    pub(super) fn mask_of(controller: &ButtonController) -> u16 {
        let mut mask = 0u16;
        let mut index = 0;
        while index < BUTTONS.len() {
            let (button, bit) = BUTTONS[index];
            if controller.is_pressed(button) {
                mask |= bit;
            }
            index += 1;
        }
        mask
    }

    /// A mask as a `ButtonState` for `update_with_state` — the injection hook the REPLAY arm uses,
    /// and so only built for that arm. The `BUTTONS` table above is what makes the scripted mask and
    /// the keypad's own bits one layout.
    #[cfg(feature = "input-replay")]
    pub(super) fn state_of(mask: u16) -> ButtonState {
        let mut state = ButtonState::empty();
        for (button, bit) in BUTTONS {
            if mask & bit != 0 {
                state = state | button;
            }
        }
        state
    }

    /// `REG_KEYINPUT`, agb's private `BUTTON_INPUT` (`agb-0.25.0/src/input.rs:82`): the ten button
    /// bits, **active-low** — a pressed button reads 0 — and the top six bits read as 1.
    const REG_KEYINPUT: *const u16 = 0x0400_0130 as *const u16;

    /// The hardware source: the keypad, read once per frame.
    ///
    /// One volatile load and a complement, because this now runs inside the vblank handler and the
    /// handler's duration is measured against the audio clock (PR-3b-ii-B1): a
    /// `ButtonController::update()` plus a ten-button mask walk would be the same information at
    /// several times the cost. `agb`'s controller is still what the REPLAY arm drives, so the
    /// scripted path goes through the same state transition the keypad's does.
    pub struct Hardware;

    impl Hardware {
        #[must_use]
        pub fn new() -> Self {
            Self
        }
    }

    impl Default for Hardware {
        fn default() -> Self {
            Self::new()
        }
    }

    impl KeySource for Hardware {
        fn sample(&mut self) -> u16 {
            // SAFETY: a fixed I/O address, read only. `BUTTON_MASK` drops the register's unused
            // top bits, which the complement turns on.
            (!unsafe { REG_KEYINPUT.read_volatile() }) & super::BUTTON_MASK
        }
    }
}

#[cfg(target_arch = "arm")]
pub use hw::Hardware;
