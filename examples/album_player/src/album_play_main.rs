//! PR-3a step (iii): the Album Player's playback ROM — `make album-rom`.
//!
//! It boots, verifies its own embedded album, plays **track 1 ("Bioluminescent Clip 1", 240 frames /
//! 491,520 samples) end to end** through the shipped feeder, and prints a `[album-rom]` ledger that
//! is the record of the run. Then it loops forever, so `timeout`'s exit 124 **is** the pass — the
//! same contract `product-play.gba` has.
//!
//! What this ROM is not: no pause, no skip, no state machine, no progress bar, no tiles, no sprites.
//! It reads the keypad (PR-3b-i) and serves the keys once per latched video frame (PR-3b-ii-A), but
//! no [`keys::Action`] means anything yet: an edge is counted, stamped with the frame it landed on,
//! and logged. Those are PR-3b-ii-B/3c (§4.4, §4.5). What it *does* show is a screen: white while
//! it is working, and then the verdict — `PASSED` on blue or `FAILED` on red, in words, through the
//! same renderer the shipped product ROM uses (this crate's [`text`]). See "The verdict is a word"
//! below.
//!
//! ## Where every ordering choice comes from
//!
//! `examples/flac_integration/src/product_play_main.rs` is the witness, cited by line below, and
//! this file re-derives none of its protocol. The register map, `enable_sound`, `teardown_audio`,
//! `rearm_dma`, `arm_audio_chain` and `clear_timer1_if` are `crates/gba-audio-hw`'s, lifted from
//! that file (ALBUM-PLAYER.md §2.2 / D4: the app may not `#[path]`-include a spike or a probe).
//! The ring, the 8-bit conversion and both FNV folds are `crates/gba-playback`'s — the same
//! implementation the host models run.
//!
//! ## The run, in the order hardware settled it
//!
//! ```text
//! running        backdrop = white, presented with no glyphs (`text::show(gfx, "", white)`)
//! verify (§D-C)  Album::parse → open()/verify_pins → the track's pins vs meta::Track → the FLAC
//!                boot gate (start_track), each refusal a named FAIL and a stop
//! prefill        16 chunks / 8,960 samples: pull_frame → widen → convert_frame → push_bytes,
//!                one CHUNK-sized critical section per slice
//! prime          hand the first chunk to the DMA and re-arm it before the chain exists
//! enable         enable_sound → clear_timer1_if → register the feeder → arm_audio_chain
//! produce        §4.5's frame loop: one producer frame per pass while the ring has room for a
//!                whole frame and no video frame is latched; a latched frame is served (one
//!                input poll + one position read) before any further decode; until frames == the
//!                pin's total_frames
//! finish         `finished` is set in the same critical section as the final push
//! drain          poll until consumed() == pushed()
//! audit          every handler entry classifies the period: coalesced overflows, and lateness past
//!                geometry::REARM_GRACE_SAMPLES — both refuse the verdict
//! teardown       stop the chain, teardown_audio
//! proof          (feature `deep-audit`) the PCM and FIFO pins over a cold re-decode of the gate
//!                track — a decode witness; the ring ledger above (`pushed == consumed == expected`)
//!                is what witnesses the bytes the DMA actually carried
//! audit (assets) (feature `deep-audit`) the full asset-region fold, after the audio (ALBM1 v2)
//! verdict        the word on the backdrop — blue "PASSED" or red "FAILED" — one commit, then the
//!                verdict line, then `loop {}`
//! ```
//!
//! ## The verdict is a word
//!
//! Fix 2 shipped a verdict lamp whose only signal was the backdrop's hue, and the person who runs
//! this cart is colourblind: violet→blue is the pair they cannot separate, so the lamp let them
//! verify nothing. Fix 3 answered that with a blink *and* a tone, which was two inventions to work
//! around a hue the builder had ruled out by rule. The rule over-generalised: full red against full
//! blue is easy to tell, and what is hard is violet against blue. So the verdict is a **word**, from
//! the renderer the shipped product ROM already uses ([`text`]), and the colour is the belt to that
//! word's braces:
//!
//! | state | backdrop | text |
//! |---|---|---|
//! | running (boot → verdict) | white | *(none yet)* |
//! | PASSED | blue | `PASSED` |
//! | FAILED | red | `FAILED` |
//!
//! Red-vs-blue rides the blue/yellow axis, which deuteranopia and protanopia (the common cases)
//! preserve — and the word is there regardless, so the colour is decoration and the text is the
//! signal. A boot refusal prints `FAILED` on red like every other failure; that also retires fix 3's
//! §5.6 open item, where a refusal had no tone because the refusal path cannot own timer0. There is
//! no tone any more, so there is nothing for timer0 to carry.
//!
//! Nothing is animated: the screen is presented **once** per state, from a background shown into
//! that frame, and every path ends in `loop {}`. The commit's background-enable mask is why that
//! works (§4.5, and [`text`]'s docs): a commit from a frame that shows nothing repaints the backdrop
//! alone, which is exactly the "running" screen and is not something to do twice.
//!
//! **What this drops.** `arm_tone` was the first code in this application to re-arm the audio chain
//! after a teardown (`enable_sound` → a primed chunk → `arm_audio_chain`) — the shape PR-3b's §4.4
//! skip path needs. It was never exercised beyond "the beep drained", so PR-3b derives it properly,
//! with tests, rather than inheriting an annunciator's rehearsal.
//!
//! ## The two folds, and why they are a second pass
//!
//! The reference folds after the audio, in a `proof()` pass, because the house FNV-1a is 64-bit and
//! ARM7TDMI has no 64-bit multiply: ~800,000 cycles for one frame's worth of bytes
//! (`product_play_main.rs:1690-1712`). This ROM's first run folded them on the producer's push path
//! instead — this app plays the same frames at 16,384 Hz, where a frame's budget is 2,097,152
//! cycles, so it looked affordable — and measured **553 underflows in 1,430 periods** (mGBA, one
//! run — one observation, not a budget line): two folds over 12,288 bytes per frame outran the
//! producer. Both pins still matched, which is exactly the trap: a byte-perfect run that is not a
//! playback run. So the folds live in [`Producer::proof`], after the teardown, and the audio pass
//! carries no hashing at all.
//!
//! ## The period audit, and what each witness is worth
//!
//! The feeder reads the chain's two witness counters at handler entry, before anything else, the way
//! the reference does (`product_play_main.rs:746-760`): timer2 counts periods elapsed (it cascades
//! from timer1, so IRQ delivery cannot stall it) and timer1 is the chunk index inside the period, so
//! `TM1_DATA − TM1_RELOAD` is how far into the period the interrupt sat. That pair is classified
//! exactly as the witness classifies it (`:782-799`): a **coalesced overflow** is a period gap
//! greater than one — the re-arm was a whole period late, the repeat DMA replayed the buffer it
//! still had, the "inserted samples" fault — and a **late service** is a gap of one whose lateness
//! exceeds `geometry::REARM_GRACE_SAMPLES`, i.e. the DMA had already reloaded the old source and
//! this period replays a tail mid-waveform (the pop the 2026-10-19 hardware reading heard). Both are
//! part of the verdict (`:1865-1894`), not supporting evidence, and both are printed, so this ROM's
//! PASS means what `product-play.gba`'s PASS means. The handler pays two volatile reads and integer
//! compares for them: no multiply, no fold, no log.
//!
//! The two folds above are a **decode** witness. [`Producer::proof`] cold-restarts the gate track and
//! re-decodes it, so a MATCH says the decoder reproduces the bake's pins for that track; it does
//! **not** say those bytes reached the DMA — a ROM that had played track 2 would still MATCH. What
//! witnesses the bytes actually carried is the ring ledger (`pushed == consumed == expected`) plus
//! the boot-time pin cross-checks.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::{Graphics, Rgb, Rgb15};
    use agb::external::critical_section::{self, Mutex};
    use agb::interrupt::{Interrupt, add_interrupt_handler};
    use alloc::format;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    use album_player::assets;
    use album_player::backend;
    use album_player::geometry;
    use album_player::keys;
    use album_player::meta::{Album, Track};
    use album_player::pcm::{PcmSource, SourceError};
    use album_player::position;
    use album_player::text;
    use gba_audio_hw::{
        AudioChain, CycleCounter, arm_audio_chain, clear_timer1_if, enable_sound, io_read_u16,
        rearm_dma, regs, teardown_audio,
    };
    // The content proofs are the `deep-audit` feature's whole body; without it this image does not
    // link the 64-bit folds at all, which is what `make album-player-rom` means by fast.
    #[cfg(feature = "deep-audit")]
    use gba_playback::checksum::{Fnv1a64, fold_i16le_stereo};
    #[cfg(feature = "deep-audit")]
    use gba_playback::playback::fold_bytes;
    use gba_playback::playback::{Chunk, RingFeed, convert_frame};

    // ------------------------------------------------- geometry, by shorter name
    //
    // Aliases, not definitions: every value is `geometry`'s, and `geometry` pins its own
    // derivation at compile time (`geometry.rs`'s `const _: () = assert!(…)` block). The short
    // names are here so this file reads like the reference ROM it is transcribed from.
    const CHUNK: usize = geometry::CHUNK_SAMPLES as usize;
    const FRAME_SAMPLES: usize = geometry::FRAME_SAMPLES as usize;
    const RING_CHUNKS: usize = geometry::RING_CHUNKS as usize;
    const RING_SAMPLES: usize = geometry::RING_SAMPLES as usize;
    const PREFILL_CHUNKS: usize = geometry::PREFILL_CHUNKS as usize;
    const PREFILL_SAMPLES: usize = geometry::PREFILL_SAMPLES as usize;
    const PERIOD_CYCLES: u32 = geometry::PERIOD_CYCLES;

    /// Timer1's latch as `arm_audio_chain` programs it: the counter runs *up* from the negative
    /// latch to its overflow, so `TM1_DATA − TM1_RELOAD` is how far into the current period the
    /// sample clock stands. That subtraction is the whole lateness measurement
    /// (`product_play_main.rs:305-308`).
    const TM1_RELOAD: u16 = 0u16.wrapping_sub(CHUNK as u16);

    /// The FIFO's half-full refill window — a hardware fact, so it does not scale with the rate. A
    /// re-arm inside it swaps the source before the repeat controller reloads; one later splices
    /// mid-waveform (`product_play_main.rs:310-315`; `geometry::REARM_GRACE_SAMPLES` owns it).
    const REARM_GRACE_SAMPLES: u32 = geometry::REARM_GRACE_SAMPLES;

    /// The chain this image programs, built from the app's **derived** geometry rather than from a
    /// hard-coded 560 (§4.2b): `gba-audio-hw` is parameterised precisely so the app's declared facts
    /// decide the clock, and a 32,768 Hz / 512-sample arm would be this same line with other inputs.
    const CHAIN: AudioChain = AudioChain::new(
        geometry::TICKS_PER_SAMPLE as u16,
        geometry::CHUNK_SAMPLES as u16,
    );

    /// The gate track: **track 1**, "Bioluminescent Clip 1", the 30 s clip.
    ///
    /// Track 0 is the 4.3-minute full master (`assets::TRACKS[0]`: 2,069 frames / 4,237,112
    /// samples). Playing that would make the headless gate a 4.3-minute run for no extra proof —
    /// same code, same feeder, four times the clock. PR-3b's traversal walks past both.
    const PLAY_TRACK: usize = 1;

    // ------------------------------------------------------------------ colors

    /// "Still working", and the boot screen: **full white**, with no text yet.
    ///
    /// The caller reads full red against full blue without trouble; what they cannot separate is
    /// violet from blue. So the running screen is not a verdict hue, and it is bright enough that a
    /// cart sitting on it is unmistakably booted rather than hung.
    const COLOR_RUNNING: Rgb15 = Rgb::new(255, 255, 255).to_rgb15();
    /// PASSED: **full blue**. Against [`COLOR_FAIL`]'s full red, the pair rides the blue/yellow axis
    /// that deuteranopia and protanopia preserve — and the screen says `PASSED` either way, so the
    /// colour is decoration and the word is the signal.
    const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    /// FAILED: **full red**. See [`COLOR_PASS`].
    const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    // ------------------------------------------------------------------ statics

    /// Ping-pong DMA payload, word-aligned (a DMA source must be), one buffer per channel per slot.
    /// `CHUNK` bytes per buffer per period; `next` lives in the same static so both sides see the
    /// same alternation (`product_play_main.rs:346-368`).
    #[repr(align(4))]
    struct SndBufs {
        l0: [u8; CHUNK],
        r0: [u8; CHUNK],
        l1: [u8; CHUNK],
        r1: [u8; CHUNK],
        next: usize,
    }

    impl SndBufs {
        const fn new() -> Self {
            Self {
                l0: [0u8; CHUNK],
                r0: [0u8; CHUNK],
                l1: [0u8; CHUNK],
                r1: [0u8; CHUNK],
                next: 0,
            }
        }
    }

    /// `RingFeed::new()` is `const` precisely so a consumer with no allocator in an interrupt can
    /// hold one in a static; the const argument resolves to `geometry::RING_SAMPLES`, so the host
    /// models instantiate the identical type (`product_play_main.rs:370-372`).
    static FEED: Mutex<RefCell<RingFeed<RING_SAMPLES>>> = Mutex::new(RefCell::new(RingFeed::new()));
    static BUFS: Mutex<RefCell<SndBufs>> = Mutex::new(RefCell::new(SndBufs::new()));

    /// The feeder's view of the audio: whether the producer is done, the underflow count (the
    /// judge), how many times it was entered, how many of those were a classified tail, and the
    /// period audit's counters.
    ///
    /// No `Default` derive — a `static` initialiser has to name every field, which is also the
    /// statement that they all start at zero (`product_play_main.rs:373-381`).
    struct Stats {
        /// Set by the producer in the same critical section as the final push. After that a short
        /// chunk is a classified tail rather than a hole.
        finished: bool,
        /// Periods served with silence while the producer still had work to do. **The judge.**
        underflows: u32,
        /// The handler's entry count (full, tail or starved period). Not a period count: the prime
        /// handed a chunk to the DMA *before* the chain was armed, so it is one fewer than the
        /// periods the track spans (`product_play_main.rs:389-390`).
        handler_entries: u32,
        tail_periods: u32,
        // --- Period audit, in the reference's meaning and reading order
        // (`product_play_main.rs:394-412`). Timer2 cascades from timer1 and so counts hardware
        // periods even while IME is off: IRQ delivery cannot stall it, which makes it ground truth
        // for periods elapsed versus periods serviced.
        /// The last period count seen at handler entry.
        last_period: u16,
        /// Σ(period gap − 1) over handler entries: the re-arm was a whole period late, so the
        /// repeat DMA replayed the buffer it already had — the audible "inserted samples" fault.
        /// Counted, not smoothed: this is how "the ring absorbed a burst" is told apart from "the
        /// hardware lost a period".
        coalesced: u32,
        /// Entries whose `TM1_DATA` says the IRQ sat more than `REARM_GRACE_SAMPLES` past the
        /// overflow: the DMA had already reloaded the old source, so this period replays a tail and
        /// splices mid-waveform. A late re-arm *inside* the grace is harmless and is not counted.
        late_servicings: u32,
        /// The worst of those latenesses, in samples — timer1 cascades from the sample clock, so its
        /// counter is a sample counter.
        max_delay_samples: u32,
    }

    static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats {
        finished: false,
        underflows: 0,
        handler_entries: 0,
        tail_periods: 0,
        last_period: 0,
        coalesced: 0,
        late_servicings: 0,
        max_delay_samples: 0,
    }));

    /// The verdict's numbers, read in one critical section.
    ///
    /// They have to come from one instant — the instant the drain is *observed* complete. The
    /// reference reads its ledger pair straight after its drain loop, before the teardown
    /// (`product_play_main.rs:1653-1656`); this image also takes the feeder's counts there, because
    /// the chain keeps raising the interrupt until it is stopped, so a read taken later folds
    /// post-drain silence periods into an entry count.
    struct Observed {
        pushed: usize,
        consumed: usize,
        underflows: u32,
        handler_entries: u32,
        tail_periods: u32,
        hw_periods: u16,
        coalesced: u32,
        late_servicings: u32,
        max_delay_samples: u32,
    }

    // ------------------------------------------------------------------ input

    /// The video-frame latch (§4.5). The vblank handler stores; the main loop loads and compares
    /// against the frame it last served. **The counter is the latch**: one increment is both the
    /// "a frame started" event and the frame's number, so there is no second cell that could fall
    /// out of step and no flag that could be cleared before the main loop looked at it.
    ///
    /// Deliberately in `.bss`, not `.iwram`: §4.5 records that the *audio* timer IRQ owns the
    /// tightest deadline in the system and the 4-byte `.iwram` ruling (§10.4) is spent on the
    /// session clock. One store per video frame, at AP-0's measured 280,896-cycle period, pays a
    /// couple of EWRAM wait states — 0.001% of the frame it paces.
    static mut FRAME_LATCH: u32 = 0;

    /// The vblank handler: **increment the latch and nothing else.**
    ///
    /// It draws nothing, reads no key, touches no palette, no DISPCNT and no VRAM, so it cannot
    /// change what is on screen (§4.5's "nothing else. Ever." applied to the video clock rather
    /// than the audio one). What it *is* allowed to touch is DISPSTAT's vblank-IRQ enable, which
    /// agb sets as part of registering the handler (`interrupt.rs`'s `Interrupt::enable`) — that is
    /// what "register a VBlank handler" means, not a display write.
    ///
    /// **Why it cannot disturb the audio IRQ's deadline.** agb's dispatcher walks its 14-slot table
    /// in interrupt-number order (`agb-0.25.0/src/interrupt.rs:271-279`), so a vblank that lands in
    /// the same dispatch as a Timer1 overflow is serviced **before** the feeder. The cost it adds is
    /// therefore on the feeder's entry latency: one load, one add and one store (three instructions,
    /// single-digit cycles) — measured against the facts rather than asserted: the re-arm grace is
    /// 16 samples = 16,384 cycles, so the latch is under 0.06% of it; the period is 573,440 cycles,
    /// so it is under 0.002% of it; and AP-0 measured a *whole* UI frame at 1.89% of a period
    /// (~10,838 c), so this handler is about one part in a thousand of the UI line it precedes. The
    /// dispatcher's own table walk is paid once per interrupt whether this handler exists or not.
    fn vblank(_cs: critical_section::CriticalSection<'_>) {
        // SAFETY: one writer (this handler) and one aligned 32-bit read-modify-write, which is
        // atomic on ARM7TDMI. The main loop's read is the same-aligned load on the other side.
        unsafe { FRAME_LATCH = FRAME_LATCH.wrapping_add(1) };
    }

    /// The latch's current value, with no critical section needed (see [`vblank`]).
    fn frame_latch() -> u32 {
        // SAFETY: read only; an aligned 32-bit load is atomic on ARM7TDMI.
        unsafe { FRAME_LATCH }
    }

    /// AP-0's measured video period, in cycles: one commit per frame is a vblank halt at 280,896 c,
    /// 59.73 Hz (`examples/album_probe/README.md`). The unit the cadence gate counts in.
    const VIDEO_PERIOD_CYCLES: u32 = 280_896;

    /// One producer frame's **uninterruptible** slice, in cycles: `pull_frame` + widen +
    /// `convert_frame` plus the four `CHUNK`-sized pushes. Taken from this ROM's own measured
    /// figures, not from an estimate — the prefill costs 1,092,226 c per frame (5,461,128 c / 5
    /// frames) and the deep-audit proof's cold re-decode 1,041,570 c (249,976,821 c / 240 frames) —
    /// with 5% over the worse of the two. It is why the main loop cannot poll more often than it
    /// does: nothing observes a latch while the decoder is inside this slice.
    const PRODUCER_SLICE_CYCLES: u32 = 1_150_000;

    /// The largest gap between two input polls this loop can show, in video frames: the producer's
    /// slice divided by the video period, rounded up (4.09 → 5). **Not 1**, and the difference is a
    /// property of the decoder rather than of the polling code: §4.4's "screen changes within one
    /// video frame" observable needs a sliced decode or a key sample taken in the vblank handler,
    /// and neither is this step's. See [`Input::poll`].
    const MAX_POLL_GAP: u32 =
        (PRODUCER_SLICE_CYCLES + VIDEO_PERIOD_CYCLES - 1) / VIDEO_PERIOD_CYCLES;

    /// The producer loop's wedge bound, in passes with a full ring and no frame due. The deepest
    /// legitimate wait is one frame's drain (~3.7 audio periods), so this is the ring's depth plus
    /// two frames' worth of passes — bounded so a wedge fails visibly instead of at the harness
    /// timeout, which is what PR-3a's `wait_for_room` was bounded for.
    const WEDGE_PASSES: u32 = (RING_CHUNKS as u32 + 2) * 4096;

    /// How many input events this ROM keeps for its end-of-run log.
    ///
    /// Bounded on purpose: an `agb::println!` is a semihosting trap, and this list is filled from
    /// the producer loop — the one place the module docs forbid logging. So the edges are *recorded*
    /// on the audio path and *printed* after the teardown, which is the same discipline the two
    /// content folds follow for a cost reason and this follows for a latency reason.
    const EDGE_LOG: usize = 16;

    /// One recorded input event: the video frame it landed on, the mask, and whether it was a
    /// repeat rather than an edge.
    struct KeyEvent {
        frame: u32,
        mask: u16,
        repeat: bool,
    }

    /// The input seam's ROM half: a source, the platform-free [`keys::Keys`] machine, the frame it
    /// last served, and the running counts the log needs.
    ///
    /// **Inert by design.** PR-3b-ii-A has no playback state machine, so a [`keys::Action`] here is
    /// recorded and then dropped: pressing Start does not pause, Right does not skip. That is the
    /// step's contract — a button is detected, debounced, repeated, counted and logged at the frame
    /// it arrived on, and changes nothing else — and it is what lets the frame loop's cadence be
    /// measured against a source whose behaviour is already known exactly.
    struct Input<S: keys::KeySource> {
        keys: keys::Keys,
        source: S,
        /// The latch value this input last served; a poll with the same value samples nothing, and
        /// [`frame_due`](Self::frame_due) is the comparison the producer loop asks.
        served: u32,
        /// Polls taken, i.e. video frames served (§4.5: one per latched frame the loop is awake
        /// for). The cadence gate's numerator, and the count of samples actually taken.
        sampled: u32,
        /// Video frames those polls span, Σ(latch deltas): the cadence gate's denominator. Polls
        /// partition the span they cover, because every latch the loop sees is served exactly once.
        frames: u32,
        /// The largest latch delta between two consecutive polls — the producer's slice, counted in
        /// video frames.
        max_gap: u32,
        /// Edge transactions (frames with at least one newly pressed button).
        presses: u32,
        /// Autorepeat events delivered.
        repeats: u32,
        log: [KeyEvent; EDGE_LOG],
        logged: usize,
    }

    impl<S: keys::KeySource> Input<S> {
        fn new(source: S) -> Self {
            Self {
                keys: keys::Keys::new(),
                source,
                served: 0,
                sampled: 0,
                frames: 0,
                max_gap: 0,
                presses: 0,
                repeats: 0,
                log: core::array::from_fn(|_| KeyEvent {
                    frame: 0,
                    mask: 0,
                    repeat: false,
                }),
                logged: 0,
            }
        }

        /// Whether a video frame has been latched since the last poll — §4.5's `frame_due`. One
        /// load and a compare, so the producer loop asks it between frames and stops decoding the
        /// moment the frame it owes is due.
        fn frame_due(&self) -> bool {
            frame_latch() != self.served
        }

        /// Sample the source for a newly latched video frame (§4.5: poll once per latched frame).
        ///
        /// The loop polls on the latch, so a poll happens at the first frame boundary the main loop
        /// is awake for. It cannot happen *inside* a producer frame: one `pull_frame` + convert +
        /// push is 1.04–1.09 M cycles ([`PRODUCER_SLICE_CYCLES`]), i.e. ~3.7 of AP-0's 280,896-cycle
        /// video frames, so while the decoder runs nothing observes the latch at all. That is why
        /// the gap between two polls is measured and printed (`cadence`) instead of assumed to be
        /// one frame, and why it is the producer's slice, not the polling code, that decides it.
        ///
        /// A poll that arrives after more than one frame has passed serves the newest latch and
        /// samples **once** — the input machine is per-frame, not catch-up, and a REPLAY source
        /// applies one scripted transition per poll for exactly that reason (see
        /// [`keys::Replay`]). Nothing is lost: an entry is applied at its own poll and never before
        /// it, and the frame number this records is the latch the poll actually saw. The counts
        /// cannot slip at all.
        fn poll(&mut self) -> Option<keys::KeyFrame> {
            let latch = frame_latch();
            if latch == self.served {
                return None;
            }
            // The cadence accounting, kept where the latch comparison is so the two cannot drift:
            // the first poll only opens the window (the frames before it were boot, and no poll
            // could have served them), and every poll after it serves a gap of at least one.
            if self.sampled != 0 {
                let gap = latch.wrapping_sub(self.served);
                self.frames = self.frames.saturating_add(gap);
                if gap > self.max_gap {
                    self.max_gap = gap;
                }
            }
            self.served = latch;
            let raw = self.source.sample();
            let frame = self.keys.sample(raw);
            self.sampled = self.sampled.saturating_add(1);
            if frame.pressed != 0 {
                self.presses = self.presses.saturating_add(1);
            }
            if frame.repeated != 0 {
                self.repeats = self.repeats.saturating_add(1);
            }
            let mask = if frame.repeated != 0 {
                frame.repeated
            } else {
                frame.pressed
            };
            if mask != 0 && self.logged < EDGE_LOG {
                self.log[self.logged] = KeyEvent {
                    frame: latch,
                    mask,
                    repeat: frame.repeated != 0,
                };
                self.logged += 1;
            }
            Some(frame)
        }

        /// Whether the cadence gate passes: at least one gap was measured, every gap inside a
        /// producer's slice ([`MAX_POLL_GAP`]), and the polls cover the frames they claim.
        ///
        /// The gates, in order: `max_gap <= MAX_POLL_GAP` is the property a main-loop poll can have
        /// at this decode cost; `frames == Σ gaps` is structural (see [`poll`](Self::poll)), so the
        /// only way to fail it is to count polls and gaps differently, which is what a future edit
        /// would do. The limit is printed beside the measurement, always, so a reader cannot mistake
        /// 5 for 1.
        fn cadence_ok(&self) -> bool {
            self.sampled >= 2 && self.frames > 0 && self.max_gap <= MAX_POLL_GAP
        }

        /// The end-of-run summary: the cadence gate's numbers first (they are the frame loop's own
        /// witness), then what was delivered, then — one line each, only the first [`EDGE_LOG`] —
        /// what the edges were.
        fn report(&self, source: &str) {
            agb::println!(
                "[album-rom] cadence: video_frames={} input_polls={} ratio={}/{} max_gap={} frames limit={} [{}]",
                self.frames,
                self.sampled,
                self.sampled,
                self.frames,
                self.max_gap,
                MAX_POLL_GAP,
                if self.cadence_ok() { "PASS" } else { "FAIL" }
            );
            if !self.cadence_ok() {
                agb::println!(
                    "[album-rom] FAIL the cadence: {} polls over {} video frames, worst gap {} frames (limit {}) — the frame loop is not serving the latches it wakes for",
                    self.sampled,
                    self.frames,
                    self.max_gap,
                    MAX_POLL_GAP
                );
            }
            agb::println!(
                "[album-rom] input: {source} — sampled={} presses={} repeats={} logged={}",
                self.sampled,
                self.presses,
                self.repeats,
                self.logged
            );
            for event in &self.log[..self.logged] {
                if event.repeat {
                    agb::println!(
                        "[album-rom] input edge: video={} repeated=0x{:04X} cursor={:?} (inert)",
                        event.frame,
                        event.mask,
                        keys::cursor_action(event.mask)
                    );
                } else {
                    agb::println!(
                        "[album-rom] input edge: video={} pressed=0x{:04X} action={:?} (inert)",
                        event.frame,
                        event.mask,
                        keys::now_playing_action(event.mask)
                    );
                }
            }
        }
    }

    // -------------------------------------------------------------- the frame's other half

    /// §4.5's second per-frame job: the position the UI will paint from, read once per latched video
    /// frame, and the monotonicity §4.4 requires of it.
    ///
    /// PR-3b-i printed this once, after the run. The loop reads it per frame now, which is the shape
    /// the clock needs, and it carries §4.4's invariant so the ROM witnesses it rather than trusting
    /// it: within one track `paint_position` is monotone in `committed_samples`, so a regression is a
    /// broken clock and never a rounding artefact. Two frames of the map are the host gate's (§13
    /// AP-2); this is the same map at the cadence it runs at.
    struct Paint {
        /// The track's length, from the bake's pin.
        total: u32,
        /// The last painted position.
        last: u32,
        /// Frames this was read on — the other half of a poll, so it must equal `Input::sampled`.
        frames: u32,
        /// Frames whose paint was below the previous frame's. Zero on every correct run.
        regressions: u32,
    }

    impl Paint {
        const fn new(total: u32) -> Self {
            Self {
                total,
                last: 0,
                frames: 0,
                regressions: 0,
            }
        }

        /// One frame's read: the committed samples, the painted position, and the monotonicity
        /// check. No critical section — an aligned 32-bit load is atomic on ARM7TDMI
        /// (`position::committed_samples`).
        fn sample(&mut self) {
            let paint = position::paint_position(position::committed_samples(), self.total);
            if paint < self.last {
                self.regressions = self.regressions.saturating_add(1);
            }
            self.last = paint;
            self.frames = self.frames.saturating_add(1);
        }
    }

    // -------------------------------------------------------------- the replay script

    /// The compiled-in key script for the REPLAY arm (feature `input-replay`, off by default).
    ///
    /// A whole history rather than a delta list: "from script frame `F` the mask is `M`", with a
    /// frame and no entry repeating the previous mask. One entry is applied per poll
    /// ([`keys::Replay`]), so `F` is a **poll index** — the Nth video frame boundary this loop was
    /// awake for — and not a video-frame number: the loop spends up to [`MAX_POLL_GAP`] frames inside
    /// one producer slice, and the `cadence` line prints how many polls spanned how many frames. The
    /// latch each edge lands on is printed beside it, so the video frame is read rather than
    /// assumed. The *sequence* of transitions is fixed regardless, and it is what the ROM's end line
    /// can be read against: **3 edges and 7 autorepeats**, by the host twin's own arithmetic
    /// (`tests/album_seam_host.rs` drives this same shape).
    ///
    /// Why these entries: one plain press (Start/A — the pause edge, inert here), one shoulder-free
    /// direction press (Right = next), and one *held* direction long enough to cross
    /// `REPEAT_DELAY_FRAMES` and fire several `REPEAT_INTERVAL_FRAMES` repeats (Left = previous).
    /// Between them they exercise every rule `keys.rs` adds to agb, which is the whole of what a
    /// gate can witness without a cartridge. What they do **not** witness is a physical press: no
    /// emulator here can make one, and the ROM's log says so.
    #[cfg(feature = "input-replay")]
    const REPLAY_SCRIPT: [(u32, u16); 6] = [
        (8, keys::BTN_A),
        (14, 0),
        (24, keys::BTN_RIGHT),
        (30, 0),
        (40, keys::BTN_LEFT),
        (120, 0),
    ];

    // ------------------------------------------------------------------ feeder

    /// Hand the next ring chunk to the free ping-pong slot and return the two pointers
    /// [`rearm_dma`] needs.
    ///
    /// Raw pointers to the slot BEFORE filling, in their own statement: the static never moves, so
    /// they stay valid past the borrow, and the caller consumes them once the copy is done.
    /// Destructuring to field borrows avoids double-borrowing the struct through two `IndexMut`s in
    /// one expression (which is what `bufs.l[bufs.next]` would be) —
    /// `product_play_main.rs:818-860`.
    fn take_into_slot(
        bufs: &mut SndBufs,
        feed: &mut RingFeed<RING_SAMPLES>,
        finished: bool,
    ) -> (Chunk, *const u8, *const u8) {
        let (lp, rp) = {
            let SndBufs {
                l0,
                r0,
                l1,
                r1,
                next,
            } = &*bufs;
            if *next == 0 {
                (l0.as_ptr(), r0.as_ptr())
            } else {
                (l1.as_ptr(), r1.as_ptr())
            }
        };
        let verdict = {
            let SndBufs {
                l0,
                r0,
                l1,
                r1,
                next,
            } = &mut *bufs;
            let (dl, dr) = if *next == 0 {
                (&mut l0[..], &mut r0[..])
            } else {
                (&mut l1[..], &mut r1[..])
            };
            let taken = feed.take_chunk(dl, dr, CHUNK, finished);
            *next = 1 - *next;
            taken
        };
        (verdict, lp, rp)
    }

    /// The mixer interrupt: hand the next chunk to the sound DMA, or silence if the producer has
    /// not delivered one.
    ///
    /// The period audit (two volatile reads + integer compares), then `take_chunk` + the slot flip +
    /// [`rearm_dma`] + the counters, and **nothing else** (§4.5, and the reference's handler at
    /// `product_play_main.rs:746-899`): no decode, no log, no allocation, no multiply-heavy math —
    /// not even the audit's. All three static borrows use the `CriticalSection` token the handler is
    /// handed, which is also the race gate — the producer only touches the ring inside a critical
    /// section, so a copy cannot be interleaved by anything.
    fn feeder(cs: critical_section::CriticalSection<'_>) {
        // The audit reads stay first, exactly as the harness orders them: their whole point is how
        // long the interrupt sat, and any work before them would be inside the answer
        // (`product_play_main.rs:746-760`). Two reads are ~10² cycles, far under the 16-sample grace
        // this rate's DMA gives the re-arm.
        let (periods, delay) = unsafe {
            (
                io_read_u16(regs::REG_TM2_DATA),
                u32::from(io_read_u16(regs::REG_TM1_DATA).wrapping_sub(TM1_RELOAD)),
            )
        }; // SAFETY: fixed I/O addresses from `gba_audio_hw::regs`, read only.

        let mut bufs = BUFS.borrow_ref_mut(cs);
        let mut feed = FEED.borrow_ref_mut(cs);
        let mut stats = STATS.borrow_ref_mut(cs);

        let gap = periods.wrapping_sub(stats.last_period);
        stats.last_period = periods;
        if gap > 1 {
            // Whole period(s) of overflow coalesced into this one delivery: the repeat DMA
            // replayed the buffer it still had.
            stats.coalesced += u32::from(gap - 1);
        }
        // A gap greater than one means timer1 re-wrapped past a whole period, so the coalesced
        // count already owns that case; only a gap of one times a sub-period lateness, and that is
        // the late splice (`product_play_main.rs:790-793`).
        if gap == 1 && delay > REARM_GRACE_SAMPLES {
            stats.late_servicings += 1;
            if delay > stats.max_delay_samples {
                stats.max_delay_samples = delay;
            }
        }
        stats.handler_entries += 1;

        let finished = stats.finished;
        let (verdict, lp, rp) = take_into_slot(&mut *bufs, &mut *feed, finished);
        // The session clock (§10.4, ruled option A). It counts what this period actually
        // committed — a whole chunk, a classified tail's real samples, or nothing — so it equals
        // `RingFeed`'s `consumed` by construction rather than by a parallel computation. PR-3b-i
        // reads it and prints it; PR-3b-ii drives the bar and the mm:ss readout from it.
        let committed = match verdict {
            Chunk::Full => CHUNK as u32,
            Chunk::Tail { real } => {
                stats.tail_periods += 1;
                real as u32
            }
            Chunk::Starved => {
                // Only a starved period while the producer still has work is an underflow. The
                // ledger did not advance for it (`take_chunk` consumes nothing it does not copy),
                // so real samples stay queued behind the silence.
                if !finished {
                    stats.underflows += 1;
                }
                0
            }
        };
        drop(feed);
        // SAFETY: aligned pointers into the static ping-pong; the other slot was handed to the DMA
        // a whole period ago, and the FIFO's 4-word pulls (µs for a whole buffer) are long finished
        // by the next period (17.1 ms at this rate) — `product_play_main.rs:878-883`.
        unsafe { rearm_dma(lp, rp, &CHAIN) };
        // After the re-arm, never before it: the DMA's next buffer is the deadline, and this is a
        // load/add/store that buys nothing if the re-arm is late.
        position::commit(committed);
        drop(bufs);
    }

    // --------------------------------------------------------------- the audio

    /// Why the producer stopped (`product_play_main.rs`'s `Halt`).
    enum Halt {
        /// The stream ended before the manifest's frame count.
        Ended,
        /// A named stage refused; the log already says which.
        Failed,
    }

    /// The producer: the seam's source plus the scratch its frames are pulled, folded, converted
    /// and pushed through, and the three folds this image's proof is made of.
    struct Producer {
        source: backend::Source,
        /// Interleaved i16 — the seam's form (§4.2): `2 × FRAME_SAMPLES` shorts.
        interleaved: Vec<i16>,
        /// The decoder's i32 form, per channel. `convert_frame` and `fold_i16le_stereo` are defined
        /// over it, so the seam's i16 is sign-extended back exactly (`i32::from`), which is
        /// lossless for both.
        left: Vec<i32>,
        right: Vec<i32>,
        /// The FIFO byte form the ring carries, per channel.
        bytes_l: Vec<u8>,
        bytes_r: Vec<u8>,
        /// Samples per channel the whole track must deliver, from the bake's pin.
        total_samples: usize,
        frames: usize,
        /// The three folds [`Producer::proof`] builds; they are empty until that pass runs. Absent
        /// from a `deep-audit`-less build, which does not fold 64-bit FNV at all.
        #[cfg(feature = "deep-audit")]
        pcm: Fnv1a64,
        #[cfg(feature = "deep-audit")]
        fifo_l: Fnv1a64,
        #[cfg(feature = "deep-audit")]
        fifo_r: Fnv1a64,
    }

    impl Producer {
        fn new(source: backend::Source, total_samples: usize) -> Self {
            Self {
                source,
                interleaved: vec![0i16; 2 * FRAME_SAMPLES],
                left: vec![0i32; FRAME_SAMPLES],
                right: vec![0i32; FRAME_SAMPLES],
                bytes_l: vec![0u8; FRAME_SAMPLES],
                bytes_r: vec![0u8; FRAME_SAMPLES],
                total_samples,
                frames: 0,
                #[cfg(feature = "deep-audit")]
                pcm: Fnv1a64::new(),
                #[cfg(feature = "deep-audit")]
                fifo_l: Fnv1a64::new(),
                #[cfg(feature = "deep-audit")]
                fifo_r: Fnv1a64::new(),
            }
        }

        /// One frame through the seam and into the two forms the rest of this file uses: the
        /// decoder's i32 per channel (`convert_frame`'s and `fold_i16le_stereo`'s input — the
        /// seam's interleaved i16 sign-extended back, which is lossless for both) and the FIFO byte
        /// form the ring carries. `Ok(0)` is end of stream.
        fn next_frame(&mut self) -> Result<usize, Halt> {
            let n = match self.source.pull_frame(&mut self.interleaved) {
                Ok(n) => n,
                Err(error) => {
                    agb::println!(
                        "[album-rom] FAIL pull_frame at frame {}: {}",
                        self.frames,
                        error
                    );
                    return Err(Halt::Failed);
                }
            };
            if n == 0 {
                return Ok(0);
            }
            for index in 0..n {
                self.left[index] = i32::from(self.interleaved[2 * index]);
                self.right[index] = i32::from(self.interleaved[2 * index + 1]);
            }
            if convert_frame(
                &self.left[..n],
                &self.right[..n],
                &mut self.bytes_l[..n],
                &mut self.bytes_r[..n],
            )
            .is_err()
            {
                agb::println!(
                    "[album-rom] FAIL convert at frame {}: sample outside the 16-bit profile",
                    self.frames
                );
                return Err(Halt::Failed);
            }
            Ok(n)
        }

        /// One frame, end to end on the audio path: pull → widen → convert → push in `CHUNK`-sized
        /// critical sections.
        ///
        /// No borrow is held across the push: each slice is its own complete, credited
        /// `push_bytes`, so the feeder sees either the previous slice or this one, never a torn one
        /// (`product_play_main.rs:1215-1236` — the 7-2 fix: a whole-frame section held interrupts
        /// ~14,680 c against a 16-sample grace, a `CHUNK`-sized one costs ~4,014 c). Nothing here
        /// hashes: see the module docs for the measured reason.
        fn push_one(&mut self) -> Result<usize, Halt> {
            let n = self.next_frame()?;
            if n == 0 {
                return Err(Halt::Ended);
            }

            let mut offset = 0usize;
            while offset < n {
                let end = (offset + CHUNK).min(n);
                let outcome = critical_section::with(|cs| {
                    let mut feed = FEED.borrow_ref_mut(cs);
                    let outcome =
                        feed.push_bytes(&self.bytes_l[offset..end], &self.bytes_r[offset..end]);
                    let complete = outcome.is_ok() && feed.pushed() == self.total_samples;
                    drop(feed);
                    // The `finished` flag is set in the *same* critical section as the final push
                    // (`product_play_main.rs:1272-1281`): setting it after the producer loop leaves
                    // a window in which the feeder sees an empty ring while the flag is still
                    // false and counts the final short chunk as an underflow.
                    if complete {
                        STATS.borrow_ref_mut(cs).finished = true;
                    }
                    outcome
                });
                if outcome.is_err() {
                    agb::println!(
                        "[album-rom] FAIL push refused at frame {} in slice {}..{} of {}",
                        self.frames,
                        offset,
                        end,
                        n
                    );
                    return Err(Halt::Failed);
                }
                offset = end;
            }
            self.frames += 1;
            Ok(n)
        }

        /// The producer's flow control, asked once per pass by §4.5's loop: has the ring room for
        /// everything a frame can add (`product_play_main.rs:1345-1382`)?
        ///
        /// The room demanded is the declared frame size — the maximum a frame of this stream can be
        /// — not the current frame's, because a frame's true size is only known after it is
        /// decoded. The substitution is conservative in the direction that matters, and
        /// `push_bytes` re-checks with the real length and writes nothing if it refuses.
        ///
        /// **Never hold the `FEED` borrow across the call**: edition-2024 temporaries live to the
        /// end of the enclosing statement, so a borrow held while deciding would deadlock against
        /// the feeder's own borrow in IRQ context. That is also why this is a predicate and not the
        /// bounded wait PR-3a had: §4.5's loop does the waiting, and it serves the frame latch while
        /// it waits (the bound moved with it — [`WEDGE_PASSES`]).
        fn has_room(&self) -> bool {
            critical_section::with(|cs| {
                let feed = FEED.borrow_ref(cs);
                feed.free_samples() >= FRAME_SAMPLES
            })
        }

        /// Fill the ring before the chain exists (`product_play_main.rs:1313-1343`): there is
        /// always room for a frame for the first `PREFILL_CHUNKS` frames, so no flow-control wait
        /// here — a prefill gate that waited would be waiting for a consumer that does not exist
        /// yet.
        fn prefill(&mut self, target: usize) -> Result<usize, Halt> {
            loop {
                let pushed = critical_section::with(|cs| FEED.borrow_ref(cs).pushed());
                if pushed >= target {
                    return Ok(pushed);
                }
                self.push_one()?;
            }
        }

        /// The correctness pass, **after** the audio (`product_play_main.rs:1689-1785`).
        ///
        /// This is the `deep-audit` feature's largest piece: it re-decodes the whole gate track and
        /// folds both pins, which is ~817,000,000 cycles here (measured) — the cost that put it
        /// behind a feature, because a gate artifact may spend 75 s and something a human sits in
        /// front of may not.
        ///
        /// A cold restart of the gate track (the FLAC arm rebuilds its decoder from the container,
        /// so the pull starts at frame 0 again), then the same seam → widen → `convert_frame` path
        /// the producer ran, folding both pins on the way. Not timed *against the audio deadline*,
        /// for the reason the reference records: a window containing the fold prices the fold. The
        /// cycle figures printed here are the proof's own cost, read from the counter the caller
        /// starts after the teardown — they are not on the audio path and are not a budget line.
        ///
        /// **What it witnesses.** The decoder: a cold re-decode of the same track folded to the
        /// bake's pins. It is *not* a witness of the ring — a ROM that had played a different track
        /// would still MATCH here. The bytes actually carried to the DMA are witnessed by the ring
        /// ledger (`pushed == consumed == expected`) and the boot-time pin cross-checks.
        ///
        /// **Why it is a second pass.** Measured, in this ROM's first run: folding FNV-1a-64 on the
        /// producer's push path — a per-byte 64-bit multiply-and-XOR over two channels' worth of
        /// 12,288 B per frame against a 2,097,152-cycle frame budget — cost **553 underflows in
        /// 1,430 periods** (mGBA; one run, so an observation rather than a budget line — this image
        /// carries no cycle instrument, R10). Both pins still matched: a byte-perfect run that was
        /// not a playback run. The conclusion is that the folds live off the audio deadline, which
        /// is why they are here.
        ///
        /// Returns `(pcm_ok, fifo_ok)`; both are compared against the bake's pins, never against
        /// this run's own output.
        ///
        /// `clock` is the caller's post-teardown counter. Each frame is bracketed twice — one
        /// window around the pull/widen/convert (`redecode`) and one around the two folds (`fold`) —
        /// and the four reads per frame are priced into the phase they bracket. That overhead only
        /// exists in this pass; the producer path carries no clock read at all.
        #[cfg(feature = "deep-audit")]
        fn proof(&mut self, clock: &mut CycleCounter<'_>) -> (bool, bool) {
            let pin = &assets::TRACKS[PLAY_TRACK];
            if let Err(error) = self.source.start_track(PLAY_TRACK) {
                agb::println!("[album-rom] FAIL proof: the gate track would not restart: {error}");
                return (false, false);
            }
            self.pcm = Fnv1a64::new();
            self.fifo_l = Fnv1a64::new();
            self.fifo_r = Fnv1a64::new();

            let mut frames = 0usize;
            let mut samples = 0usize;
            let mut redecode = 0u32;
            let mut fold = 0u32;
            let total_start = clock.read();
            loop {
                let pull_start = clock.read();
                let n = match self.next_frame() {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => return (false, false),
                };
                let pull_end = clock.read();
                redecode = redecode.wrapping_add(pull_end.wrapping_sub(pull_start));
                let fold_start = clock.read();
                if fold_i16le_stereo(&mut self.pcm, &self.left[..n], &self.right[..n]).is_err() {
                    agb::println!(
                        "[album-rom] FAIL proof: a sample is outside the int16 profile at frame {frames}"
                    );
                    return (false, false);
                }
                fold_bytes(
                    &mut self.fifo_l,
                    &mut self.fifo_r,
                    &self.bytes_l[..n],
                    &self.bytes_r[..n],
                );
                fold = fold.wrapping_add(clock.read().wrapping_sub(fold_start));
                frames += 1;
                samples += n;
            }
            let total = clock.read().wrapping_sub(total_start);
            agb::println!("[album-rom] proof: redecode={redecode}c fold={fold}c total={total}c");

            let pcm = self.pcm.finish();
            let pcm_ok = frames == pin.frames as usize
                && samples == pin.samples as usize
                && pcm == pin.fnv_pcm;
            agb::println!(
                "[album-rom] proof: frames={} samples={} pcm_fnv=0x{:016X} expected=0x{:016X} [{}]",
                frames,
                samples,
                pcm,
                pin.fnv_pcm,
                if pcm_ok { "MATCH" } else { "MISMATCH" }
            );
            let fifo_l = self.fifo_l.finish();
            let fifo_r = self.fifo_r.finish();
            let fifo_ok = fifo_l == pin.fnv_fifo_l && fifo_r == pin.fnv_fifo_r;
            agb::println!(
                "[album-rom] fifo proof: L=0x{:016X} R=0x{:016X} expected L=0x{:016X} R=0x{:016X} [{}]",
                fifo_l,
                fifo_r,
                pin.fnv_fifo_l,
                pin.fnv_fifo_r,
                if fifo_ok { "MATCH" } else { "MISMATCH" }
            );
            (pcm_ok, fifo_ok)
        }
    }

    // ------------------------------------------------------------------ boot

    /// The selected backend's source over `album`.
    ///
    /// The shipped arm is `backend::open()` — parse, `verify_pins` (which includes every track's
    /// bounded content tripwire), then the arm's constructor. `Source::new` starts no track: the
    /// caller names the one it plays, so a boot no longer gates track 0. A stub-only image builds
    /// the same [`backend::Source`] from the manifest it still carries: the
    /// stub's track table is the caller's by design (`backend/stub.rs`'s `StubSource::new`), so it
    /// is leaked once into `'static`. That keeps this ROM's `main` free of any codec name (§4.2
    /// rule 2) — the arm changes here and nowhere else.
    #[cfg(feature = "backend-flac")]
    fn open_source(_album: &Album<'static>) -> Result<backend::Source, SourceError> {
        backend::open()
    }

    /// [`open_source`]'s stub-only arm: the same pin cross-check, then a synthetic track list
    /// derived from the manifest (no second copy of the album's numbers lives in this file).
    #[cfg(not(feature = "backend-flac"))]
    fn open_source(album: &Album<'static>) -> Result<backend::Source, SourceError> {
        use album_player::backend::stub::{StubSource, StubTrack};
        backend::verify_pins(album)?;
        let tracks: Vec<StubTrack> = (0..album.n_tracks())
            .map(|index| {
                let track = album.track(index);
                StubTrack {
                    start_sample: track.start_sample,
                    n_samples: track.n_samples,
                    title: track.title,
                    artist: track.artist,
                }
            })
            .collect();
        Ok(StubSource::new(Vec::leak(tracks)))
    }

    // ---------------------------------------------------------- the verdict terms

    /// A verdict term a build may not have checked at all.
    ///
    /// `None` is **not** a pass: it is `SKIPPED`, is printed by name in the verdict line, and does
    /// not fail the verdict because the build was explicitly asked not to check it
    /// (`make album-player-rom`). The name is the point — an image that verified no content must not
    /// read like one that did.
    fn term_ok(value: Option<bool>) -> bool {
        value.unwrap_or(true)
    }

    /// [`term_ok`]'s printout: `true`, `false`, or `SKIPPED`.
    fn term_name(value: Option<bool>) -> &'static str {
        match value {
            Some(true) => "true",
            Some(false) => "false",
            None => "SKIPPED",
        }
    }

    /// Say what was refused, by name, and put `FAILED` on screen in red.
    ///
    /// R5: a partial run that still printed PASS would be worse than no run at all, so every boot
    /// refusal ends here rather than continuing. It ends on the same verdict screen a failed run
    /// shows, not just a printed line: a refusal the caller cannot perceive is the exact defect this
    /// fixes, and this path used to fall straight into a bare `loop {}` after printing. The reason
    /// itself stays in the log — the screen carries the two-word verdict only.
    fn refuse(gfx: &mut Graphics<'_>, what: &str) -> ! {
        agb::println!("[album-rom] FAIL {what}");
        // Every exit path emits a verdict: `make album-rom`'s echo tells a human to judge by the
        // `verdict` line, so a refusal that printed only `FAIL` would leave the log with no verdict
        // to read at all.
        agb::println!(
            "[album-rom] verdict FAILED (boot refusal) — screen RED \"{}\"",
            text::FAILED
        );
        text::show(gfx, text::FAILED, COLOR_FAIL);
        loop {}
    }

    // ------------------------------------------------------------------ main

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::println!("[album-rom] entry — the album player's playback path (PR-3a step iii)");

        // The screen first, before any work that can be slow: white says "booting", and a cart left
        // white is a boot that never reached a verdict. `gfx` and `timers` are disjoint fields of
        // `gba`, so both handles live for the whole run.
        let mut gfx = gba.graphics.get();
        text::show(&mut gfx, "", COLOR_RUNNING);

        // `make album-player-rom` builds without `deep-audit`: it plays the same album but proves no
        // decoded content. Say so at boot as well as in the verdict, so an unverified image cannot be
        // mistaken for a gate artifact by someone reading only the top of the log.
        #[cfg(not(feature = "deep-audit"))]
        agb::println!(
            "[album-rom] NOTE this build does not verify decoded content; it is a player artifact, not a gate artifact"
        );

        // ---- the video clock and the input seam (PR-3b-i, §4.5). Registered here, before the
        // boot clock starts, for two reasons: the frames a REPLAY script names are video frames from
        // boot, and the boot ledger's `parse=`/`open=`/... spans stay comparable with every earlier
        // run's. The handler allocates (agb boxes the closure), which is why it is not on any audio
        // deadline — it is boot work, like the Timer1 registration further down.
        //
        // SAFETY: the handler increments one static and returns; it allocates nothing, logs nothing
        // and touches no register — agb's interrupt-handler contract.
        let _vblank = unsafe { add_interrupt_handler(Interrupt::VBlank, vblank) };
        #[cfg(not(feature = "input-replay"))]
        let mut input = Input::new(keys::Hardware::new());
        #[cfg(feature = "input-replay")]
        let mut input = Input::new(keys::Replay::new(&REPLAY_SCRIPT));
        // Which source fed this run, always, and the honesty line §10.5 requires: the shipped
        // artifact reads the keypad, and a headless run therefore cannot have observed a press. The
        // REPLAY arm is the gate's only witness of the seam and says that it is a script.
        #[cfg(not(feature = "input-replay"))]
        agb::println!(
            "[album-rom] input: HARDWARE (REG_KEYINPUT) — a headless run cannot press a key; the physical leg is AP-7a's"
        );
        #[cfg(feature = "input-replay")]
        agb::println!(
            "[album-rom] input: REPLAY ({} entries, scripted at the keys layer; the delivered count is printed at the end) — no physical press is implied",
            REPLAY_SCRIPT.len()
        );

        // ---- the boot clock (R10's counter lift): timer0 at /1 cascading into timer1, started
        // BEFORE the manifest is read and stopped before `arm_audio_chain` reprograms the same pair.
        // `CycleCounter` borrows the two timers, so "read the boot total first" is enforced by the
        // borrow checker rather than by this comment. The unit is the CPU cycle; ÷ 16,777,216 is
        // seconds on any GBA, which wall time on an emulator is not.
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM.
        let mut clock = CycleCounter::start(&mut timers);
        let boot_start = clock.read();

        // §D-C rule 1: the manifest is authoritative and is read before anything it describes is
        // believed. A refusal names the rule and its payload (`meta::Refusal`'s `Display`).
        let album = match Album::parse(assets::ALBUM1) {
            Ok(album) => album,
            Err(refusal) => refuse(&mut gfx, &format!("manifest refused: {refusal}")),
        };
        let after_parse = clock.read();
        agb::println!(
            "[album-rom] album: {} tracks, {} samples, rate {} Hz, \"{}\"",
            album.n_tracks(),
            album.album_samples(),
            album.rate_hz(),
            album.album_title()
        );

        // §D-C rule 2: the generated pins against the manifest, then the arm's own constructor.
        // `open()` cross-checks every pin (including each track's bounded content tripwire); the
        // gate track's own container gate runs at `start_track` below, so a container this image
        // cannot play is a boot refusal, not a first-frame surprise.
        let mut source = match open_source(&album) {
            Ok(source) => source,
            Err(error) => refuse(&mut gfx, &format!("the backend refused the album: {error}")),
        };
        let after_open = clock.read();

        // The gate track's own entry against its bake pin, field by field. `verify_pins` has
        // already swept every track; this is the pair the rest of `main` reads numbers from.
        let track: Track<'static> = album.track(PLAY_TRACK as u32);
        let pin = &assets::TRACKS[PLAY_TRACK];
        if track.frames != pin.frames {
            refuse(
                &mut gfx,
                "the gate track's frame count disagrees with the bake's pin",
            );
        }
        if track.n_samples != pin.n_samples {
            refuse(
                &mut gfx,
                "the gate track's sample count disagrees with the bake's pin",
            );
        }
        agb::println!(
            "[album-rom] track {}: \"{}\" {} frames, {} samples, {} asset bytes",
            PLAY_TRACK,
            track.title,
            pin.frames,
            pin.n_samples,
            pin.asset_len
        );

        // The FLAC boot gate for the gate track (§D-C rule 3): profile, stereo, rate identity and
        // the container's pins, all inside `start_track`.
        if let Err(error) = source.start_track(PLAY_TRACK) {
            refuse(
                &mut gfx,
                &format!("track {PLAY_TRACK} failed its boot gate: {error}"),
            );
        }
        let after_gate = clock.read();

        agb::println!(
            "[album-rom] geometry: chunk={} ring={}x{}={} prefill={}x{}={} dma_words={} frame={}",
            CHUNK,
            RING_CHUNKS,
            CHUNK,
            RING_SAMPLES,
            PREFILL_CHUNKS,
            CHUNK,
            PREFILL_SAMPLES,
            CHAIN.dma_words,
            FRAME_SAMPLES
        );

        let total_samples = usize::try_from(pin.samples).unwrap_or(usize::MAX);
        let total_frames = pin.frames as usize;
        let mut producer = Producer::new(source, total_samples);

        // ---- prefill (ring only; the consumer does not exist yet)
        let prefilled = match producer.prefill(PREFILL_SAMPLES) {
            Ok(n) => n,
            Err(_) => refuse(&mut gfx, "the prefill did not complete"),
        };
        agb::println!(
            "[album-rom] prefilled {} samples in {} frames (target {})",
            prefilled,
            producer.frames,
            PREFILL_SAMPLES
        );
        let after_prefill = clock.read();

        // ---- prime: the first chunk is handed to the DMA before the first period boundary can ask
        // for one, so audio begins at arming rather than one period late
        // (`product_play_main.rs:1492-1530`). In repeat mode `rearm_dma` starts playing the moment it
        // is written, and the first boundary is then a re-arm like any other.
        let primed = critical_section::with(|cs| {
            let mut bufs = BUFS.borrow_ref_mut(cs);
            let mut feed = FEED.borrow_ref_mut(cs);
            let (verdict, lp, rp) = take_into_slot(&mut *bufs, &mut *feed, false);
            match verdict {
                Chunk::Full => {
                    // SAFETY: aligned pointers into the static ping-pong, filled for a whole
                    // transfer by `take_chunk` just above.
                    unsafe { rearm_dma(lp, rp, &CHAIN) };
                    // The prime's chunk is committed to the DMA like any other, so the session clock
                    // counts it here rather than starting one chunk short of the ledger.
                    position::commit(CHUNK as u32);
                    true
                }
                _ => false,
            }
        });
        if !primed {
            refuse(
                &mut gfx,
                "the prime could not fill a whole chunk (the prefill is shorter than one)",
            );
        }

        enable_sound();
        // Clear a stale Timer1 `IF` *before* the source is enabled: registering the handler sets the
        // `IE` bit, and a pending bit left by the boot path would enter the feeder before the chain
        // has a period to serve. The first real overflow is a whole chunk away, so installing the
        // handler before arming loses no period (`product_play_main.rs:1548-1558`).
        clear_timer1_if();
        // SAFETY: the handler allocates nothing, logs nothing and touches only the statics above —
        // agb's interrupt-handler contract.
        let _handler = unsafe { add_interrupt_handler(Interrupt::Timer1, feeder) };

        // The boot total is read with the pair still owned by `clock`, i.e. before the line that
        // reprograms them. Everything from `boot_start` to here is what a human waits through
        // before audio; `enable_sound` and the IF clear are part of it.
        let boot_total = clock.read().wrapping_sub(boot_start);
        clock.stop();
        drop(clock);
        agb::println!(
            "[album-rom] boot: parse={}c open={}c gate={}c prefill={}c total={}c",
            after_parse.wrapping_sub(boot_start),
            after_open.wrapping_sub(after_parse),
            after_gate.wrapping_sub(after_open),
            after_prefill.wrapping_sub(after_gate),
            boot_total
        );
        agb::println!(
            "[album-rom] boot takes {} ms of CPU at 16.78 MHz — audio begins after prefill",
            u64::from(boot_total) * 1_000 / 16_777_216
        );

        // The timers were handed out before the manifest was read (the boot clock), because taking
        // all four is `unsafe` only where a `Mixer` would own timer0/1 and this crate never
        // constructs one — which is what makes the raw DMA path legal here
        // (`product_play_main.rs:1836-1839`).
        arm_audio_chain(&mut timers, &CHAIN);
        agb::println!(
            "[album-rom] armed: {} ticks/sample x {} samples = {} cycles/period, {} DMA words",
            CHAIN.ticks_per_sample,
            CHAIN.chunk_samples,
            PERIOD_CYCLES,
            CHAIN.dma_words
        );

        // ---- §4.5's frame loop. No logging inside: one `agb::println!` is a semihosting trap,
        // which on hardware is a syscall into a host that may not exist.
        //
        // Two clocks meet here and this loop is the whole of their arbitration. The producer is the
        // thief of leftover time: it runs only while the ring has room for another whole frame, and
        // it stops the moment a video frame is due. A latched frame is then served before any more
        // decode — one input poll and one position read, which is everything §4.5 gives the video
        // clock today. The audio path is untouched by the restructure: the period IRQ still only
        // takes a chunk, re-arms the DMA and counts, and the producer's flow control is the same
        // predicate it always was, just asked once per pass instead of spun inside a wait.
        let mut paint = Paint::new(total_samples as u32);
        let mut halt: Option<&'static str> = None;
        // Passes in a row with a full ring and no frame due — the wedge bound ([`WEDGE_PASSES`]).
        let mut idle: u32 = 0;
        'frames: loop {
            while !input.frame_due() && producer.frames < total_frames && producer.has_room() {
                idle = 0;
                match producer.push_one() {
                    Ok(_) => {}
                    Err(Halt::Ended) => {
                        agb::println!(
                            "[album-rom] FAIL the stream ended at frame {} of {}",
                            producer.frames,
                            total_frames
                        );
                        halt = Some("stream ended early");
                        break 'frames;
                    }
                    Err(Halt::Failed) => {
                        halt = Some("producer");
                        break 'frames;
                    }
                }
            }
            // The frame slice. Nothing acts on the key yet: the edge is counted, stamped with the
            // latch it landed on and logged, and PR-3b-ii-B is where an `Action` means something.
            if input.poll().is_some() {
                paint.sample();
                idle = 0;
            }
            if producer.frames >= total_frames {
                break;
            }
            // Only reachable with a full ring and no frame served, i.e. §4.5's leftover time spent
            // waiting for the consumer. Bounded so a wedge fails visibly instead of at the harness
            // timeout.
            idle += 1;
            if idle > WEDGE_PASSES {
                agb::println!(
                    "[album-rom] FAIL the ring wedged at frame {}; the consumer stopped advancing",
                    producer.frames
                );
                halt = Some("ring wedged");
                break;
            }
        }
        if halt.is_none() && producer.frames != total_frames {
            agb::println!(
                "[album-rom] FAIL frame count {} does not match the pin's {}",
                producer.frames,
                total_frames
            );
            halt = Some("frame count");
        }

        // Belt and braces: `push_one` already sets this in the same critical section as the final
        // push; this covers a producer that stopped on an error path and is idempotent otherwise.
        critical_section::with(|cs| STATS.borrow_ref_mut(cs).finished = true);
        agb::println!("[album-rom] producer finished ({} frames)", producer.frames);

        // ---- drain: whole periods take full chunks, the residue lands as one zero-padded tail, and
        // the ledger advances by real samples only — so `consumed` reaches `pushed` exactly when the
        // last real sample has been handed to the DMA (`product_play_main.rs:1626-1650`). Bounded in
        // periods so a wedged consumer fails visibly rather than at the harness timeout.
        let max_polls = (RING_CHUNKS + 2) as u64 * 8192;
        let mut polls = 0u64;
        let (drained, observed) = loop {
            // The drain spins for the ring's last 1.2 s, so it is a second place a frame is
            // served: the same pair as the loop above, on the same latch comparison.
            if input.poll().is_some() {
                paint.sample();
            }
            let (done, seen) = critical_section::with(|cs| {
                let feed = FEED.borrow_ref(cs);
                let stats = STATS.borrow_ref(cs);
                (
                    feed.consumed() >= feed.pushed(),
                    Observed {
                        pushed: feed.pushed(),
                        consumed: feed.consumed(),
                        underflows: stats.underflows,
                        handler_entries: stats.handler_entries,
                        tail_periods: stats.tail_periods,
                        hw_periods: stats.last_period,
                        coalesced: stats.coalesced,
                        late_servicings: stats.late_servicings,
                        max_delay_samples: stats.max_delay_samples,
                    },
                )
            });
            polls += 1;
            if done || polls > max_polls {
                break (done, seen);
            }
        };
        agb::println!("[album-rom] drain: {} polls", polls);
        if !drained {
            agb::println!("[album-rom] FAIL the drain never finished");
        }

        // ---- silence and teardown, in the order the harness settled on
        // (`product_play_main.rs:1844-1846`): the timer chain first (through the handle this
        // function still holds), then the sound hardware.
        timers.timer1.set_interrupt(false).set_enabled(false);
        timers.timer0.set_enabled(false);
        timers.timer2.set_enabled(false);
        teardown_audio();

        let Observed {
            pushed,
            consumed,
            underflows,
            handler_entries,
            tail_periods: tails,
            hw_periods,
            coalesced,
            late_servicings: late,
            max_delay_samples: late_max,
        } = observed;
        agb::println!(
            "[album-rom] feed: pushed={} expected={} consumed={} frames={}/{} underflows={} handler_entries={} tails={}",
            pushed,
            total_samples,
            consumed,
            producer.frames,
            total_frames,
            underflows,
            handler_entries,
            tails
        );
        // The audit, in the units it is measured in: `late_max` is in **samples** (timer1's counter
        // is a sample counter), the grace is 16 of them and one period is `CHUNK` of them. Printed
        // so a regression has numbers to diff instead of a claim. `late_max` is the worst *late*
        // lateness, so `late=0 late_max=0` reads "nothing exceeded the grace", not "the worst
        // service was on time": a re-arm inside the grace is harmless and is not recorded, which is
        // the reference's choice (`product_play_main.rs:407-412`, `:793-799`).
        agb::println!(
            "[album-rom] audit: hw_periods={} coalesced={} late={} late_max={} samples grace={}",
            hw_periods,
            coalesced,
            late,
            late_max,
            REARM_GRACE_SAMPLES
        );
        // ---- the input seam's end-of-run line (PR-3b-i). The counts are what a gate reads: a
        // REPLAY build's script has a known number of edges and repeats, and a HARDWARE build's
        // `presses=0` is the honest reading of a headless run rather than a failure. Which source it
        // was is in the line, always.
        #[cfg(not(feature = "input-replay"))]
        input.report("HARDWARE (REG_KEYINPUT) — no physical press was observed by this run");
        #[cfg(feature = "input-replay")]
        input.report(&format!(
            "REPLAY ({} entries, {} edges delivered) — scripted, not a physical press",
            input.source.entries(),
            input.source.edges()
        ));

        // ---- the session clock (§10.4), printed where the ledger it mirrors was printed. The
        // equality is structural — the clock is incremented by exactly what `take_chunk` returned —
        // so a MISMATCH means the increment and the ledger have come apart, which is why it is part
        // of the verdict rather than a remark.
        let committed = position::committed_samples();
        let painted = position::paint_position(committed, total_samples as u32);
        // The per-frame reads are the same map the host gate pins, taken once per latched frame
        // while the audio ran: `frames` must equal the input polls (the two halves of §4.5's frame
        // slice happen together) and nothing may have run backwards on the way (§4.4's invariant).
        let position_ok =
            committed == consumed as u32 && paint.frames == input.sampled && paint.regressions == 0;
        agb::println!(
            "[album-rom] position: committed={} ledger={} lead={} paint={} frames={} regressions={} [{}]",
            committed,
            consumed,
            position::LEAD_SAMPLES,
            painted,
            paint.frames,
            paint.regressions,
            if position_ok { "MATCH" } else { "MISMATCH" }
        );
        if !position_ok {
            agb::println!(
                "[album-rom] FAIL the session clock, the ring ledger and the per-frame position reads disagree — one increment per committed period and one read per poll is the contract"
            );
        }

        // At most one tail period is expected (zero if the clip ends exactly on a period boundary).
        let ledger_ok = pushed == total_samples
            && consumed == pushed
            && producer.frames == total_frames
            && underflows == 0
            && tails <= 1
            && halt.is_none()
            && pushed != 0
            && position_ok;
        // The audit is part of the verdict, not supporting evidence (`product_play_main.rs:1865-1894`):
        // a coalesced overflow is a full-buffer replay, a late service is a mid-waveform splice, and
        // both are exactly what `product-play.gba` refuses — so this image's PASS cannot be weaker
        // than that one's.
        let audit_ok = coalesced == 0 && late == 0;
        // The frame loop's own term: the polls cover the video frames they claim, and the worst gap
        // is inside the producer's slice. Its numbers are printed by `Input::report` above, beside
        // this gate's limit, so a PASS here cannot be read as "one poll per frame".
        let cadence_ok = input.cadence_ok();
        if !ledger_ok {
            agb::println!("[album-rom] FAIL the sample ledger is short — the verdict cannot pass");
        }
        if !audit_ok {
            agb::println!(
                "[album-rom] FAIL the period audit: coalesced={} late={} late_max={} samples (grace {}) — the DMA replayed or spliced",
                coalesced,
                late,
                late_max,
                REARM_GRACE_SAMPLES
            );
        }

        // ---- the two content proofs (feature `deep-audit`), after the audio and over a cold
        // restart of the gate track (see `Producer::proof`); the pins are the bake's, not this run's
        // own output. The counter is restarted here so the proof's own cost is measured: it is off
        // the audio deadline, which is why it is a second pass at all. Without the feature the terms
        // are `None` — *SKIPPED*, and named as such in the verdict rather than quietly `true`.
        #[cfg(feature = "deep-audit")]
        let (pcm_ok, fifo_ok) = {
            let mut proof_clock = CycleCounter::start(&mut timers);
            let (pcm, fifo) = producer.proof(&mut proof_clock);
            proof_clock.stop();
            drop(proof_clock);
            (Some(pcm), Some(fifo))
        };
        #[cfg(not(feature = "deep-audit"))]
        let (pcm_ok, fifo_ok): (Option<bool>, Option<bool>) = (None, None);

        // ---- the asset audit (feature `deep-audit`): the O(asset-region) fold, moved off the
        // pre-audio path in ALBM1 v2. Boot verifies the structure fold and each track's bounded
        // tripwire; this is the fold that still covers every byte, and after the audio nothing is
        // waiting on it. Measured, because its whole reason for moving is its cost.
        #[cfg(feature = "deep-audit")]
        let assets_ok: Option<bool> = {
            let mut audit_clock = CycleCounter::start(&mut timers);
            let assets = album.verify_assets();
            let audit_cycles = audit_clock.read();
            audit_clock.stop();
            drop(audit_clock);
            agb::println!(
                "[album-rom] audit: assets={audit_cycles}c [{}]",
                if assets.is_ok() { "MATCH" } else { "MISMATCH" }
            );
            if let Err(refusal) = assets {
                agb::println!("[album-rom] FAIL the asset fold refuses: {refusal}");
            }
            Some(assets.is_ok())
        };
        #[cfg(not(feature = "deep-audit"))]
        let assets_ok: Option<bool> = None;

        let pass = ledger_ok
            && audit_ok
            && cadence_ok
            && term_ok(pcm_ok)
            && term_ok(fifo_ok)
            && term_ok(assets_ok);
        let (word, backdrop, screen) = if pass {
            (text::PASSED, COLOR_PASS, "BLUE")
        } else {
            (text::FAILED, COLOR_FAIL, "RED")
        };
        text::show(&mut gfx, word, backdrop);
        // The audited image's line names what is on screen, not only a hue: "screen BLUE" alone was a
        // promise the caller could not check, since violet against blue is exactly the pair they
        // cannot separate. The unaudited one may not borrow any promise that content was checked: it
        // names the five terms and the feature that is missing, which is the whole difference between
        // the two artifacts.
        #[cfg(feature = "deep-audit")]
        agb::println!(
            "[album-rom] verdict {} — screen {} \"{}\" (ledger={}, audit={}, cadence={}, pcm={}, fifo={}, assets={})",
            word,
            screen,
            word,
            ledger_ok,
            audit_ok,
            cadence_ok,
            term_name(pcm_ok),
            term_name(fifo_ok),
            term_name(assets_ok)
        );
        #[cfg(not(feature = "deep-audit"))]
        agb::println!(
            "[album-rom] verdict {} — screen {} \"{}\" (ledger={}, audit={}, cadence={}, pcm={}, fifo={}, assets={} — built without deep-audit)",
            word,
            screen,
            word,
            ledger_ok,
            audit_ok,
            cadence_ok,
            term_name(pcm_ok),
            term_name(fifo_ok),
            term_name(assets_ok)
        );

        // Held for as long as the cart is on: one present of the word on its backdrop, then `loop {}`
        // with nothing shown into any later frame — a second commit would repaint the backdrop alone
        // (`text`'s one-shot contract). Everything is already down at this point, so there is no
        // deadline left to protect and nothing armed that could starve audio.
        loop {}
    }
}

/// Host-build stub (the same dual-target pattern the other ROM files use), so a host `cargo test`
/// of this crate can build the bin at all.
#[cfg(not(target_arch = "arm"))]
fn main() {}
