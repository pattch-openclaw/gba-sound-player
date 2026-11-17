//! PR-3a step (iii): the Album Player's playback ROM — `make album-rom`.
//!
//! It boots, verifies its own embedded album, plays **track 1 ("Bioluminescent Clip 1", 240 frames /
//! 491,520 samples) end to end** through the shipped feeder, and prints a `[album-rom]` ledger that
//! is the record of the run. Then it loops forever, so `timeout`'s exit 124 **is** the pass — the
//! same contract `product-play.gba` has.
//!
//! What this ROM is not: no progress bar, no tiles, no sprites, no menu.
//!
//! **Track-end auto-advance** is here (PR-3b-iii-A): when a visit's producer has finished and the
//! feeder has drained its ring, the run advances to the next track through the skip's hand-off minus
//! the truncate (there is no stale audio left to drop), and stops at the album's last track instead
//! of wrapping — §1's start-to-finish. The `traversal:` line states the visit count, the last track
//! and whether the album's end was reached.
//!
//! **Pause and resume** are here (PR-3b-ii-B2a): Start or A fades the hand-off gain to zero and
//! freezes the chain; a second press primes both DMA buffers from the same ring head, re-arms, and
//! ramps the gain back up. That contract is [`transport`]'s and is modelled on the host; §4.4 has
//! its bounds.
//!
//! **Skip and restart** are here too (PR-3b-ii-B2b): Right/R and Left/L step to the next and previous
//! track with §5.3's wrap at both album ends, and Down restarts the current one — §4.4's six steps,
//! with the fade at the hand-off, a truncate whose first silence chunk shares its critical section,
//! and a measured `to_new` per skip rather than an asserted one ([`Skips`]). A skip while paused is
//! refused and counted, by design: see [`transport`]'s docs.
//!
//! What this ROM *does* show is a screen: white while it is working, and then the verdict —
//! `PASSED` on blue or `FAILED` on red, in words, through the same renderer the shipped product ROM
//! uses (this crate's [`text`]). See "The verdict is a word" below.
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
//! vblank         every frame: increment the latch. While the chain is armed, also: read
//!                REG_KEYINPUT, run keys::Sampler, post the newest Action with its frame, and
//!                bracket the body with REG_TM1_DATA → the handler's cost in samples of audio time
//! verify (§D-C)  Album::parse → open()/verify_pins → the track's pins vs meta::Track → the FLAC
//!                boot gate (start_track), each refusal a named FAIL and a stop
//! prefill        16 chunks / 8,960 samples: pull_frame → widen → convert_frame → push_bytes,
//!                one CHUNK-sized critical section per slice
//! prime          hand the first chunk to the DMA and re-arm it before the chain exists
//! enable         enable_sound → clear_timer1_if → register the feeder → arm_audio_chain
//! produce        §4.5's frame loop: one producer frame per pass while the ring has room for a
//!                whole frame and no video frame is latched; a latched frame is served (the
//!                posted action + one position read) before any further decode; until frames ==
//!                the pin's total_frames
//! pause / resume a Start/A edge is taken at the next period boundary: the hand-off gain ramps to
//!                zero inside one chunk, that chunk is played out over the next period, then timers
//!                1/0/2 and both sound DMAs stop with the ring and the decoder untouched; a second
//!                edge primes both DMA buffers from the ring head (the next sample — the ramped
//!                buffer is not replayed), re-arms, and ramps back up
//! finish         `finished` is set in the same critical section as the final push
//! track end      drained (`consumed == pushed`) → advance through the same hand-off a skip uses minus
//!                the truncate, or, on the last track, `end=ALBUM_END` with no wrap
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
//!
//! ## What the ROM cannot witness, and what stands in for it
//!
//! The cart's own witnesses stop at the hand-off. The ring ledger says what `take_chunk` consumed;
//! the period audit says the re-arm beat the FIFO's grace; `old`/`content_at` say what the bytes
//! after a hand-off looked like. None of them can see the *audible stream* across a transition: a
//! pause tears the DMA down between periods and nothing in the cart samples what left the DAC, so a
//! chunk served at zero gain and a chunk of the cover are indistinguishable on the wire. The
//! transport's witness is therefore the **host model** (`tests/album_seam_host.rs`), which drives
//! the same `RingFeed` + `Transport` + double-buffer hand-off and records the post-gain bytes
//! exactly as they are handed to `rearm_dma`; `pcm`/`fifo` here are a **content/decoder** witness
//! (which bytes of which track the decoder produced) and are never cited for the transport. Sound
//! on a cartridge is AP-7's, by ear.

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
    use album_player::transport::{
        FADE_PERIODS, SKIP_PRESS_TO_HANDOFF_PERIODS, SKIP_PRIME_CHUNKS, State, Step, Transport,
    };
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

    /// **The track this image starts on: 1, and that is a fixture choice, not the album's.**
    ///
    /// The album starts at track 0, and §1's start-to-finish playthrough begins there; this image
    /// says its own start out loud in the boot log so the number cannot be read as a property of the
    /// album. The reason for 1 is the gate's clock: track 0 is the 4.3-minute full master
    /// (`assets::TRACKS[0]`: 2,069 frames / 4,237,112 samples, 4:18), 8.6× the clip's 491,520
    /// samples, so a run starting at 0 plays 9,457,264 album samples (~9.6 min of audio) against
    /// this one's 5,220,152 (~5.3 min) for no extra proof — same code, same feeder, same traversal.
    /// A product build starts at 0; a gate build starts at 1. There is deliberately **no** feature or
    /// make knob for it: each artifact is one image, and the log names which start it used.
    const START_TRACK: usize = 1;

    /// The audited whole-track fold's caps, in frames: **512** for one track, **1,024** for a whole
    /// proof pass.
    ///
    /// The arithmetic, not a guess: the gate track's 240 frames are measured at 812,370,488 c of
    /// cold fold, i.e. **3.39 M c/frame**. 512 frames is therefore ~1.74 G c per track and 1,024 the
    /// ~3.5 G c a whole pass may spend — against a gate run whose leash is 600 s (10.07 G cycles).
    /// The number that makes this necessary is the fixture's own: tracks 0 and 2 point at the
    /// 4.3-minute master's blob (2,069 frames, ~7 G c, ~8.6x the gate track), so folding them is not
    /// a matter of patience but of fitting the run at all. A track past the cap is printed as
    /// `SKIPPED` with this arithmetic and is never reported as `MATCH`.
    #[cfg(feature = "deep-audit")]
    const PROOF_FRAME_CAP: u32 = 512;
    /// See [`PROOF_FRAME_CAP`]: the whole pass's frame budget, so a fixture with many short tracks
    /// cannot make the gate's post-audio phase unbounded either.
    #[cfg(feature = "deep-audit")]
    const PROOF_FRAMES_TOTAL: u32 = 1_024;

    /// How many track visits the run keeps for its per-track ledger and its content proof.
    ///
    /// Bounded like every other record in this file, and **gated** rather than quietly truncated: the
    /// proof enumerates the tracks from this list, so a visit the list dropped would be a track whose
    /// bytes nothing folded. A run that overflows it fails its verdict.
    const VISIT_LOG: usize = 24;

    /// How many skips the run keeps for its end-of-run log (the first [`VISIT_LOG`] visits can hold
    /// at most one fewer, and a script with more skips than this is not a gate).
    const SKIP_LOG: usize = 16;

    /// The cover's zeros: one whole chunk, pushed as real ring data (§4.4 step 4).
    const ZEROS: [u8; CHUNK] = [0u8; CHUNK];

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
        /// periods the track spans (`product_play_main.rs:389-390`). A resume's prime is the same
        /// again, which is why the hand-off ledger below adds the two.
        handler_entries: u32,
        tail_periods: u32,
        /// Samples the most recent chunk did not carry: the zero padding of a classified tail, and
        /// `0` for a full chunk. This is the first term of a track boundary's audible gap — the
        /// silence at the end of the last period that carried the outgoing track's audio.
        ///
        /// A `Starved` period leaves it alone on purpose: after a visit drains, the feeder serves
        /// starved periods before the hand-off, and the padding they follow is still the tail's.
        tail_padding: u32,
        /// `starved_periods` when the main loop observed a visit's drain and posted the advance.
        /// The delta to the hand-off is that boundary's starved count: the fade, the loop's
        /// detection, and nothing of the next track.
        advance_starved_at: u32,
        /// Periods the feeder entered with no ring data at all. One of these is the silence
        /// between a track's last sample and its hand-off (PR-3b-iii-A): `finished` keeps it from
        /// being an underflow, but it is still a hand-off that carried nothing, so the hand-off
        /// ledger has to count it.
        starved_periods: u32,
        /// Chunk hand-offs the **main loop** made rather than the feeder: the boot prime, plus one
        /// per resume. `handler_entries + primes` is the number of chunks the DMA was handed, which
        /// is `ceil(n_samples / CHUNK)` whether or not the run was paused.
        primes: u32,
        /// Periods served in the epochs before the current one. `arm_audio_chain` reloads timer2
        /// from zero, so a resume starts a new epoch; without banking the old count the first entry
        /// after a resume would read a ~65,000-period gap and book it as coalesced overflows.
        periods_before: u32,
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
        // --- the skip's wire watch (§4.4's "measured at AP-1, not asserted", PR-3b-ii-B2b) ----
        //
        // The probe's idea, re-implemented in app source: latch the first all-silent chunk handed to
        // the DMA after the hand-off, and the first non-silent one after that, so `to_new` is read
        // off the bytes the DMA was given rather than off the producer's intentions. The scan costs
        // a pass over both slots, so it runs only while a skip is unresolved — an ordinary period
        // pays one compare.
        /// A skip's hand-off has happened and the watch has not resolved yet.
        watch_armed: bool,
        /// The period (a `handler_entries` value) the hand-off's mark was taken at.
        watch_at: u32,
        /// The first all-silent period after `watch_at`; `0` = not seen yet (the same sentinel the
        /// probe uses, and why its raw `old_periods` had a floor of one).
        watch_silent_at: u32,
        /// The first non-silent period after `watch_silent_at`; `0` = not seen yet.
        watch_new_at: u32,
        /// The period of the first chunk handed over **after** the skip's cover countdown — the new
        /// track's first chunk, by the machinery. `0` = not seen yet.
        ///
        /// This is the second half of the skip's measurement, and it exists because the wire cannot
        /// answer the first half alone: a track whose own head is digital silence (this fixture's
        /// master: its first non-zero sample is at 0.4539 s) is byte-identical to the cover, so "the
        /// first non-silent chunk" lags the transition by however long the track starts quiet. The
        /// feeder reads this off the hand-off gain's cover countdown, so it witnesses that the cover
        /// was exactly `SKIP_PRIME_CHUNKS` chunks and that the ramp started on the track's own first
        /// chunk rather than on a cover chunk.
        watch_content_at: u32,
        /// `underflows` when the mark was taken: each skip's delta is the whole claim.
        watch_underflows_at: u32,
        /// Samples `RingFeed::truncate()` has stepped over in this run. The session clock does not
        /// see them (`consumed` moves, `commit` does not), so this is the term that keeps
        /// `consumed == committed + dropped` exact with skips in the run.
        dropped_samples: u32,
        /// Samples of silence cover pushed as real ring data (§4.4 step 4): the other term the ring's
        /// ledger needs, because a cover chunk is pushed and consumed but is not track content.
        cover_samples: u32,
        /// Samples of *track content* pushed by the producer, over every visit.
        content_pushed: u32,
    }

    static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats {
        finished: false,
        underflows: 0,
        handler_entries: 0,
        tail_periods: 0,
        tail_padding: 0,
        advance_starved_at: 0,
        starved_periods: 0,
        primes: 0,
        periods_before: 0,
        last_period: 0,
        coalesced: 0,
        late_servicings: 0,
        max_delay_samples: 0,
        watch_armed: false,
        watch_at: 0,
        watch_silent_at: 0,
        watch_new_at: 0,
        watch_content_at: 0,
        watch_underflows_at: 0,
        dropped_samples: 0,
        cover_samples: 0,
        content_pushed: 0,
    }));

    /// The playback transport (§4.4, PR-3b-ii-B2a): the state the main loop drives and the hand-off
    /// gain the feeder applies, in one cell — so "is the hand-off faded?" has exactly one answer.
    /// The feeder and the main loop are the only two borrowers, and each holds it for integer work.
    static TRANSPORT: Mutex<RefCell<Transport>> = Mutex::new(RefCell::new(Transport::new()));

    /// The verdict's numbers, read in one critical section.
    ///
    /// They have to come from one instant — the instant the drain is *observed* complete. The
    /// reference reads its ledger pair straight after its drain loop, before the teardown
    /// (`product_play_main.rs:1653-1656`); this image also takes the feeder's counts there, because
    /// the chain keeps raising the interrupt until it is stopped, so a read taken later folds
    /// post-drain silence periods into an entry count. The skip's wire watch is read in the same
    /// section, so "did the last skip resolve, and what was its underflow delta" comes from the same
    /// instant as the ledger.
    struct Observed {
        pushed: usize,
        consumed: usize,
        underflows: u32,
        handler_entries: u32,
        tail_periods: u32,
        starved_periods: u32,
        primes: u32,
        /// Periods in every epoch the run had (`periods_before + last_period`), so a paused run's
        /// count is the sum of its epochs rather than only the last one.
        hw_periods: u32,
        coalesced: u32,
        late_servicings: u32,
        max_delay_samples: u32,
        /// Samples `truncate()` stepped over, samples of silence cover pushed, and samples of track
        /// content the producer pushed: the three terms the ring's ledger is stated on once a run can
        /// skip. Read in the same instant as `pushed`/`consumed`, or they would not add up.
        dropped_samples: u32,
        cover_samples: u32,
        content_pushed: u32,
    }

    // ------------------------------------------------------------------ input

    /// The video-frame latch (§4.5, §10.4's sibling). The vblank handler increments it; the main
    /// loop and the position reader load it and compare against the frame they last served. **The
    /// counter is the latch**: one increment is both the "a frame started" event and the frame's
    /// number, so there is no second cell that could fall out of step and no flag that could be
    /// cleared before a reader looked at it.
    ///
    /// Deliberately in `.bss`, not `.iwram`: §4.5 records that the *audio* timer IRQ owns the
    /// tightest deadline in the system and the 4-byte `.iwram` ruling (§10.4) is spent on the
    /// session clock. One store per video frame, at AP-0's measured 280,896-cycle period, pays a
    /// couple of EWRAM wait states — 0.001% of the frame it paces.
    static mut FRAME_LATCH: u32 = 0;

    /// The input seam's ISR half, installed for exactly the window in which the audio clock runs.
    ///
    /// `Some` between the chain being armed and the timer chain being stopped; `None` during boot
    /// (there is no audio clock to measure a sample against, and no track to act on) and after the
    /// run (nothing may keep sampling into a stopped timer, and the numbers the log prints must be
    /// the ones the run produced).
    static INPUT: Mutex<RefCell<Option<Input<RomSource>>>> = Mutex::new(RefCell::new(None));

    /// The shipped arm samples the keypad; `input-replay` swaps the compiled-in script in.
    #[cfg(not(feature = "input-replay"))]
    type RomSource = keys::Hardware;
    #[cfg(feature = "input-replay")]
    type RomSource = keys::Replay;

    /// The re-arm slack, in samples of the audio clock: `TM1_DATA` counts one tick per sample, so a
    /// bracket of two readings is the handler's duration in samples, and this is what it is compared
    /// against (`geometry::REARM_GRACE_SAMPLES`, hardware rather than a knob).
    const _: () = assert!(REARM_GRACE_SAMPLES == 16);

    /// The vblank handler: sample the keypad, run the input machine, and measure what that cost the
    /// audio clock.
    ///
    /// **Why the sample lives here.** PR-3b-ii-A measured one producer frame at 1.04–1.09 M cycles of
    /// uninterruptible decode — 3.7–3.9 of AP-0's 280,896-cycle video frames — so a main loop cannot
    /// observe the keypad once per frame at any poll rate. The vblank handler is the only code in
    /// this system that runs at video rate, so the sample moves here and the main loop consumes what
    /// it posts (`Input::take`).
    ///
    /// **The floor, in full.** §4.4's observable is *not* three video frames, which is what an
    /// earlier version of this comment claimed by counting only the sample and the period boundary.
    /// The chain is: the debounce frame (≤ 1 video frame, `detect_max`), **plus the wait for the main
    /// loop to take the post** — the consumer runs at producer cadence, so that leg is one producer
    /// frame, 3.7–3.9 video frames (`take_max`) — plus the boundary the transition cannot straddle
    /// (one period = 2.04 video frames, `boundary_max`). The ROM prints all three and their sum; the
    /// `reaction` verdict term gates each against its bound, and the sum is ~18 video frames ≈ 306 ms
    /// end to end once the skip's 5-period cover is added. None of these legs can be removed: the
    /// sample is a vblank's, the take cannot pre-empt a decode frame, and a transition may not
    /// straddle a period.
    ///
    /// **What it costs the audio, and how that is known rather than hoped.** This handler and the
    /// feeder are both IRQs at the same priority, so a timer1 overflow that is pending while this
    /// runs *waits*: ARM7TDMI has no nested interrupts. The shipped margin is
    /// `geometry::REARM_GRACE_SAMPLES` = 16 samples of the sound FIFO's half-full window, and a
    /// re-arm later than that splices mid-waveform. So the handler brackets its whole body with the
    /// audio clock — timer1 is the sample counter inside the period and timer2 the period counter
    /// (it cascades from timer1) — and the worst bracket is part of the verdict, not a remark: a
    /// worst past the grace fails the run. The grace is hardware, not a knob (`geometry.rs`), so the
    /// handler fits inside it or this step is not done.
    ///
    /// It draws nothing, logs nothing, allocates nothing, divides nothing, and touches no palette,
    /// no DISPCNT and no VRAM, so it also cannot change what is on screen (§4.5's "nothing else.
    /// Ever.", applied to the video clock).
    fn vblank(cs: critical_section::CriticalSection<'_>) {
        // The audio clock, read *first*: the measurement's whole point is how long the interrupt
        // sat, and any work before it would be outside the answer.
        let entry = audio_clock();
        // SAFETY: one writer (this handler) and one aligned 32-bit read-modify-write, which is
        // atomic on ARM7TDMI. The readers' loads are the same-aligned read on the other side.
        let latch = unsafe {
            FRAME_LATCH = FRAME_LATCH.wrapping_add(1);
            FRAME_LATCH
        };
        let mut slot = INPUT.borrow_ref_mut(cs);
        // Boot, and after the run: the latch above is the whole job.
        let Some(input) = slot.as_mut() else {
            return;
        };
        input.sample(latch);
        input.record_handler(handler_samples(entry, audio_clock()));
    }

    /// A stable `(samples, periods)` pair from the audio clock: `TM1_DATA` (the sample inside the
    /// period), `TM2_DATA` (the periods elapsed, cascaded from timer1), and `TM1_DATA` again — with a
    /// retry when the first reading was on the far side of a period boundary, so the pair is never
    /// torn.
    ///
    /// The retry is the same discipline as `CycleCounter::read`'s re-read, and it is needed even
    /// though this handler is short: without it a boundary inside the four I/O reads of a bracket
    /// would pair an old sample with a new period, which is exactly what the measured first run of
    /// this step looked like — a worst of 64,977 samples for a handler that took one.
    fn audio_clock() -> (u16, u16) {
        // SAFETY: fixed I/O addresses, read only.
        let mut samples = unsafe { io_read_u16(regs::REG_TM1_DATA) };
        let mut spins = 0;
        loop {
            // SAFETY: as above.
            let periods = unsafe { io_read_u16(regs::REG_TM2_DATA) };
            // SAFETY: as above.
            let again = unsafe { io_read_u16(regs::REG_TM1_DATA) };
            if again >= samples || spins == 2 {
                return (again, periods);
            }
            samples = again;
            spins += 1;
        }
    }

    /// The handler's duration in samples of the audio clock, from two [`audio_clock`] readings.
    ///
    /// Timer1 **does not wrap to zero**: it reloads to `TM1_RELOAD` at every overflow, so a plain
    /// subtraction across a period boundary is 559 samples too large rather than one sample — the
    /// bug this step's first run printed. Timer2 counts the boundaries, so the elapsed time is the
    /// part of the old period still to run, the part of the new one already run, and any whole
    /// periods in between. The result is exact for any bracket shorter than 65,536 periods (38
    /// minutes), which is what "bounded" means here.
    fn handler_samples(entry: (u16, u16), exit: (u16, u16)) -> u32 {
        let (entry_samples, entry_periods) = entry;
        let (exit_samples, exit_periods) = exit;
        let periods = u32::from(exit_periods.wrapping_sub(entry_periods));
        if periods == 0 {
            u32::from(exit_samples.wrapping_sub(entry_samples))
        } else {
            let before = u32::from(entry_samples.wrapping_neg());
            let after = u32::from(exit_samples.wrapping_sub(TM1_RELOAD));
            (periods - 1) * CHUNK as u32 + before + after
        }
    }

    /// The latch's current value, with no critical section needed (see [`vblank`]).
    fn frame_latch() -> u32 {
        // SAFETY: read only; an aligned 32-bit load is atomic on ARM7TDMI.
        unsafe { FRAME_LATCH }
    }

    /// AP-0's measured video period, in cycles: one commit per frame is a vblank halt at 280,896 c,
    /// 59.73 Hz (`examples/album_probe/README.md`). The unit the cadence gate counts in, and the
    /// unit the action floor is stated in.
    const VIDEO_PERIOD_CYCLES: u32 = 280_896;

    /// The largest gap between two input samples the ROM can show, in video frames: **one**, because
    /// the sample is the vblank handler's and the handler runs on every vblank.
    ///
    /// PR-3b-ii-A's limit here was five, and it was five for a measured reason: a main-loop poll
    /// cannot happen inside a producer frame's 1.04–1.09 M cycles (3.7–3.9 video frames), so one
    /// poll per frame was unreachable from the loop. Moving the sample into the ISR is exactly what
    /// replaces that number, so the gate tightens to the property the ISR has: a gap of two means a
    /// vblank was missed — the handler did not run at all — and the run fails rather than reporting
    /// a slower rate as a pass.
    const MAX_SAMPLE_GAP: u32 = 1;

    /// The wait for the next audio period boundary, in video frames: one period is 573,440 cycles =
    /// 2.04 of AP-0's 280,896-cycle frames, rounded up. §4.4's action floor — an action may not
    /// change what the DMA is playing mid-buffer, so it waits for the boundary the feeder already
    /// owns.
    const MAX_BOUNDARY_FRAMES: u32 =
        (PERIOD_CYCLES + VIDEO_PERIOD_CYCLES - 1) / VIDEO_PERIOD_CYCLES;
    const _: () = assert!(MAX_BOUNDARY_FRAMES == 3);

    /// The largest main-loop pass, in video frames, that this system can show: **6**.
    ///
    /// `4` is measured, not chosen: a pass that decodes spends one producer frame inside its inner
    /// loop, PR-3b-ii-A measured that frame at 1.04–1.09 M cycles of uninterruptible decode, and one
    /// video frame is 280,896 cycles — so 1.09 M c is 3.88 frames, i.e. 4. The other two are the
    /// pass's own edges: its length is the difference of two latch reads, so a pass that begins just
    /// before a vblank and ends just after the next due one reads one frame longer than the work it
    /// did, and the body after the inner loop (the take, one position read, the transport's tick)
    /// can straddle one more. Both the const asserts below and the gate read this as a **ceiling on
    /// the run's own measured worst pass** ([`Input::pass_max`]); the bound a take is gated against
    /// is that same measurement ([`Input::take_max`]), which is why neither number is this file's
    /// opinion. The gate run measures 5.
    const MAX_PASS_FRAMES: u32 = 4 + 1 + 1;
    // The 4 of the sum above, from the measurement: 3 video frames is under the 1.09 M-cycle
    // producer frame and 4 is over it, so four is the least whole-frame count that holds one.
    const _: () = assert!(3 * VIDEO_PERIOD_CYCLES < 1_090_000);
    const _: () = assert!(4 * VIDEO_PERIOD_CYCLES > 1_090_000);
    // Only the REPLAY arm measures a pass and reads the ceiling; the shipped arm still pins the
    // arithmetic so the number cannot drift unnoticed.
    #[cfg(not(feature = "input-replay"))]
    const _: () = assert!(MAX_PASS_FRAMES == 6);

    /// Video frames from a detection to the next audio period boundary.
    ///
    /// `cycles` is what is left of the current period, so it is at most `PERIOD_CYCLES` = 573,440
    /// and the answer is 1..=3. Two compares are therefore exact, and there is no divide on a
    /// measured path (§6).
    fn frames_to_boundary(cycles: u32) -> u32 {
        if cycles <= VIDEO_PERIOD_CYCLES {
            1
        } else if cycles <= 2 * VIDEO_PERIOD_CYCLES {
            2
        } else {
            3
        }
    }

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

    /// The input seam's ROM half as the vblank handler sees it: a source, the platform-free
    /// [`keys::Sampler`] (machine + pending action + frame stamp), the frame accounting, and the
    /// measurements the log prints.
    ///
    /// **Inert by design.** PR-3b-ii-B1 has no playback state machine, so a [`keys::Action`] here is
    /// detected, stamped, posted, taken and counted, and changes nothing: pressing Start does not
    /// pause, Right does not skip. That is the step's contract — the sample moved into the ISR, and
    /// the audio numbers are the acceptance test — and it is what lets the sample's cost be measured
    /// against a source whose behaviour is already known exactly.
    struct Input<S: keys::KeySource> {
        source: S,
        sampler: keys::Sampler,
        /// The latch the previous sample served.
        served: u32,
        /// Σ(latch deltas) over the frames served after the first: the cadence line's denominator.
        frames: u32,
        /// The largest latch delta between two consecutive samples, in video frames.
        max_gap: u32,
        /// The worst bracket of the handler body, in samples of the audio clock.
        worst_handler: u32,
        /// The largest frame distance from a scripted mask transition to its press being detected.
        /// Only a script knows when a press was supposed to happen, so only the REPLAY arm measures
        /// it (and the host twin pins the arithmetic).
        #[cfg(feature = "input-replay")]
        detect_max: u32,
        /// The largest wait, in video frames, from a detection to the next period boundary.
        boundary_max: u32,
        /// The latch of the vblank that posted the action now in the slot. It is what makes the
        /// ISR→consumer half of the lateness measurable: `Input::take` subtracts the latch it reads
        /// from this one, and the consumer runs at producer cadence, not video cadence. Measured
        /// beside `detect_max` and for the same reason: only a scripted run has a press to time.
        #[cfg(feature = "input-replay")]
        posted_latch: u32,
        /// The largest wait, in video frames, from a post to the main loop taking it. **This is the
        /// term the previous log did not have**: the action is taken at the top of a pass, after the
        /// producer's inner loop, and one producer frame is 3.7–3.9 video frames of uninterruptible
        /// decode, so a press cannot be served inside the frame it was detected on.
        #[cfg(feature = "input-replay")]
        take_max: u32,
        /// The largest main-loop pass, in video frames: the bound [`Self::take_max`] is gated
        /// against, measured by this run rather than chosen here.
        #[cfg(feature = "input-replay")]
        pass_max: u32,
        /// Actions the main loop took out of the slot.
        consumed: u32,
        log: [KeyEvent; EDGE_LOG],
        logged: usize,
    }

    impl<S: keys::KeySource> Input<S> {
        fn new(source: S) -> Self {
            Self {
                source,
                sampler: keys::Sampler::new(),
                served: 0,
                frames: 0,
                max_gap: 0,
                worst_handler: 0,
                #[cfg(feature = "input-replay")]
                detect_max: 0,
                boundary_max: 0,
                #[cfg(feature = "input-replay")]
                posted_latch: 0,
                #[cfg(feature = "input-replay")]
                take_max: 0,
                #[cfg(feature = "input-replay")]
                pass_max: 0,
                consumed: 0,
                log: core::array::from_fn(|_| KeyEvent {
                    frame: 0,
                    mask: 0,
                    repeat: false,
                }),
                logged: 0,
            }
        }

        /// One video frame, from the vblank handler: sample the source, run the machine, post what
        /// it detected, and accumulate the cadence and the two reaction latencies.
        ///
        /// The source's own frame clock and the sampler's count advance together — one call of this
        /// is one video frame — so a post's frame stamp, the `cadence` line and a scripted entry's
        /// frame are one clock, and the measurements below are subtractions rather than inferences.
        fn sample(&mut self, latch: u32) {
            // The cadence accounting: the first sample only opens the window (the frames before it
            // were boot, and no sample could have served them); every sample after it must serve
            // exactly one frame ([`MAX_SAMPLE_GAP`]).
            if self.sampler.sampled() != 0 {
                let gap = latch.wrapping_sub(self.served);
                self.frames = self.frames.saturating_add(gap);
                if gap > self.max_gap {
                    self.max_gap = gap;
                }
            }
            self.served = latch;
            let raw = self.source.sample();
            let events = self.sampler.sample(raw);

            if events.action().is_some() {
                #[cfg(feature = "input-replay")]
                {
                    self.posted_latch = latch;
                }
                // What is left of the current period, in audio cycles: timer1's counter runs up from
                // `TM1_RELOAD` to its overflow, so `TM1_DATA − TM1_RELOAD` is the samples elapsed and
                // the rest of the chunk is the wait. `TICKS_PER_SAMPLE` is a constant, so the
                // multiply is a shift.
                // SAFETY: fixed I/O address, read only.
                let elapsed =
                    u32::from(unsafe { io_read_u16(regs::REG_TM1_DATA) }.wrapping_sub(TM1_RELOAD));
                let remaining = (CHUNK as u32).saturating_sub(elapsed);
                let boundary = frames_to_boundary(remaining * geometry::TICKS_PER_SAMPLE);
                if boundary > self.boundary_max {
                    self.boundary_max = boundary;
                }
                // From the scripted mask transition to the frame its press was detected on. The
                // press is `DEBOUNCE_FRAMES - 1` after the transition, which is the bound this is
                // measured against.
                #[cfg(feature = "input-replay")]
                {
                    let detected = self.sampler.sampled().saturating_sub(1);
                    if let Some(scripted) = self.source.mask_changed() {
                        let latency = detected.saturating_sub(scripted);
                        if latency > self.detect_max {
                            self.detect_max = latency;
                        }
                    }
                }
            }

            let mask = if events.repeated != 0 {
                events.repeated
            } else {
                events.pressed
            };
            if mask != 0 && self.logged < EDGE_LOG {
                self.log[self.logged] = KeyEvent {
                    frame: latch,
                    mask,
                    repeat: events.repeated != 0,
                };
                self.logged += 1;
            }
        }

        /// The handler's bracket, from the vblank handler: keep the worst case and nothing else, so
        /// the measurement cannot grow into the thing it is measuring.
        fn record_handler(&mut self, samples: u32) {
            if samples > self.worst_handler {
                self.worst_handler = samples;
            }
        }

        /// Take the newest action the handler posted, if any — the consumer half of the seam.
        ///
        /// The ISR→take wait is measured **here**, at the instant the main loop actually takes the
        /// post: the latch difference to the vblank that posted it, in video frames. That wait is a
        /// producer frame wide, not a video frame — the consumer runs at producer cadence — which is
        /// the reason this term exists.
        fn take(&mut self) -> Option<keys::Detected> {
            let detected = self.sampler.take();
            if detected.is_some() {
                self.consumed = self.consumed.saturating_add(1);
                #[cfg(feature = "input-replay")]
                {
                    let waited = frame_latch().wrapping_sub(self.posted_latch);
                    if waited > self.take_max {
                        self.take_max = waited;
                    }
                }
            }
            detected
        }

        /// One main-loop pass's length, in video frames — the bound `take_max` is gated against,
        /// because a take happens at the top of a pass and its predecessor is the widest wait the
        /// slot can hold.
        #[cfg(feature = "input-replay")]
        fn note_pass(&mut self, frames: u32) {
            if frames > self.pass_max {
                self.pass_max = frames;
            }
        }

        /// Video frames sampled.
        fn sampled(&self) -> u32 {
            self.sampler.sampled()
        }

        /// Whether the cadence gate passes: at least one gap was measured, every gap is one frame
        /// ([`MAX_SAMPLE_GAP`]), and the samples cover the frames they claim.
        fn cadence_ok(&self) -> bool {
            self.sampled() >= 2 && self.frames > 0 && self.max_gap <= MAX_SAMPLE_GAP
        }

        /// Whether the handler stayed inside the sound FIFO's half-full window — the audio's
        /// acceptance test for this step.
        fn handler_ok(&self) -> bool {
            self.worst_handler <= REARM_GRACE_SAMPLES
        }

        /// Whether a scripted press was detected within its debounce frame, the action was taken
        /// inside one main-loop pass, and the action's wait for the next period boundary was inside
        /// one period. The first two are the seam's floor — a press cannot be served sooner than the
        /// pass it landed in (see [`MAX_PASS_FRAMES`]) — and the third the audio clock's. Not
        /// measurable without a script, so the verdict term is `SKIPPED` on the shipped arm.
        #[cfg(feature = "input-replay")]
        fn reaction_ok(&self) -> bool {
            self.detect_max <= keys::DEBOUNCE_FRAMES as u32 - 1
                && self.pass_max <= MAX_PASS_FRAMES
                && self.take_max <= self.pass_max
                && self.boundary_max <= MAX_BOUNDARY_FRAMES
        }

        /// The end-of-run summary: the cadence gate's numbers first (they are the video clock's own
        /// witness), then the handler's cost against the audio clock, then what was delivered, then —
        /// one line each, only the first [`EDGE_LOG`] — what the edges were.
        fn report(&self, source: &str) {
            agb::println!(
                "[album-rom] cadence: video_frames={} input_polls={} ratio={}/{} max_gap={} frames limit={} [{}]",
                self.frames,
                self.sampled(),
                self.sampled(),
                self.frames,
                self.max_gap,
                MAX_SAMPLE_GAP,
                if self.cadence_ok() { "PASS" } else { "FAIL" }
            );
            if !self.cadence_ok() {
                agb::println!(
                    "[album-rom] FAIL the cadence: {} samples over {} video frames, worst gap {} frames (limit {}) — the sample is ISR-driven, so it must serve every vblank",
                    self.sampled(),
                    self.frames,
                    self.max_gap,
                    MAX_SAMPLE_GAP
                );
            }
            agb::println!(
                "[album-rom] input: vblank ISR worst={} samples = {}c over {} frames (grace limit {} samples) [{}]",
                self.worst_handler,
                self.worst_handler * geometry::TICKS_PER_SAMPLE,
                self.sampled(),
                REARM_GRACE_SAMPLES,
                if self.handler_ok() { "PASS" } else { "FAIL" }
            );
            if !self.handler_ok() {
                agb::println!(
                    "[album-rom] FAIL the vblank handler: worst {} samples of audio time is past the {} -sample re-arm grace — it delays the feeder past the point where the DMA reloads the old source",
                    self.worst_handler,
                    REARM_GRACE_SAMPLES
                );
            }
            #[cfg(feature = "input-replay")]
            {
                agb::println!(
                    "[album-rom] input: edges={} detect_latency max={} frames (bound {}) action@next-period max={} frames (bound {})",
                    self.sampler.presses(),
                    self.detect_max,
                    keys::DEBOUNCE_FRAMES as u32 - 1,
                    self.boundary_max,
                    MAX_BOUNDARY_FRAMES
                );
                agb::println!(
                    "[album-rom] input: take_latency max={} frames (bound {} = this run's worst main-loop pass, ceiling {} frames) — ISR→take, gated by the reaction term",
                    self.take_max,
                    self.pass_max,
                    MAX_PASS_FRAMES
                );
                if !self.reaction_ok() {
                    agb::println!(
                        "[album-rom] FAIL the reaction: a scripted press was detected {} frames after its mask change (limit {}), taken {} frames after its post (bound {} = the run's worst pass), and an action waited {} frames for a period boundary (limit {})",
                        self.detect_max,
                        keys::DEBOUNCE_FRAMES as u32 - 1,
                        self.take_max,
                        self.pass_max,
                        self.boundary_max,
                        MAX_BOUNDARY_FRAMES
                    );
                }
            }
            agb::println!(
                "[album-rom] input: {source} — sampled={} presses={} repeats={} logged={} consumed={} dropped={}",
                self.sampled(),
                self.sampler.presses(),
                self.sampler.repeats(),
                self.logged,
                self.consumed,
                self.sampler.dropped()
            );
            for event in &self.log[..self.logged] {
                if event.repeat {
                    agb::println!(
                        "[album-rom] input edge: video={} repeated=0x{:04X} cursor={:?} (list cursor only: a repeat posts no now-playing action)",
                        event.frame,
                        event.mask,
                        keys::cursor_action(event.mask)
                    );
                } else {
                    // Which action acts is a property of this build, so it is stated per edge rather
                    // than left to a reader's memory.
                    let action = keys::now_playing_action(event.mask);
                    agb::println!(
                        "[album-rom] input edge: video={} pressed=0x{:04X} action={:?} ({})",
                        event.frame,
                        event.mask,
                        action,
                        match action {
                            Some(keys::Action::PauseToggle) => "acts: pause⇄resume",
                            Some(keys::Action::NextTrack) => "acts: next track",
                            Some(keys::Action::PrevTrack) => "acts: previous track",
                            Some(keys::Action::RestartTrack) => "acts: restart track",
                            None => "no action",
                        }
                    );
                }
            }
        }
    }

    /// Take the newest action the vblank handler posted, if any.
    ///
    /// The ISR owns the seam now, so the consumer's half is a critical section — the same idiom the
    /// ring and the stats use. **Inert until PR-3b-ii-B2**: the action is counted at the call site to
    /// witness that the slot is reachable from the main loop, and changes nothing else.
    fn take_action() -> Option<keys::Detected> {
        critical_section::with(|cs| match INPUT.borrow_ref_mut(cs).as_mut() {
            Some(input) => input.take(),
            None => None,
        })
    }

    /// Record one main-loop pass's length, in video frames, for the reaction gate's measured bound
    /// ([`Input::pass_max`]). Called once per pass from the frame loop, the only place that knows
    /// where a pass begins and ends.
    #[cfg(feature = "input-replay")]
    fn note_pass(frames: u32) {
        critical_section::with(|cs| {
            if let Some(input) = INPUT.borrow_ref_mut(cs).as_mut() {
                input.note_pass(frames);
            }
        });
    }

    // -------------------------------------------------------------- the frame's other half

    /// §4.5's second per-frame job: the position the UI will paint from, read once per latched video
    /// frame the main loop is awake for, and the monotonicity §4.4 requires of it.
    ///
    /// The key sample of §4.5's pair moved into the vblank handler (PR-3b-ii-B1); this read stayed
    /// the main loop's, because it happens between producer frames. A producer frame is 3.7–3.9 video
    /// frames, so this reads at most as many frames as the ISR sampled and usually fewer — the ROM
    /// prints both numbers (`frames=<read> of <sampled>`). What it witnesses is unchanged: within one
    /// track `paint_position` is monotone in `committed_samples`, so a regression is a broken clock
    /// and never a rounding artefact. Two frames of the map are the host gate's (§13 AP-2); this is
    /// the same map at the cadence it runs at.
    struct Paint {
        /// The track's length, from the bake's pin.
        total: u32,
        /// The session clock when this visit's timeline starts: the hand-off's instant plus the
        /// silence cover, because the cover is committed to the clock (it is handed to the DMA) but is
        /// not part of any track. `committed - base` is therefore the visit's own position, which is
        /// what makes §4.4's "0 at a track start" true again once a run can skip.
        base: u32,
        /// The last painted position.
        last: u32,
        /// The latch this reader last served; the ISR owns the sampler's own cursor.
        served: u32,
        /// Frames this was read on.
        frames: u32,
        /// Frames whose paint was below the previous frame's. Zero on every correct run.
        regressions: u32,
    }

    impl Paint {
        const fn new() -> Self {
            Self {
                total: 0,
                base: 0,
                last: 0,
                served: 0,
                frames: 0,
                regressions: 0,
            }
        }

        /// Start a visit: its length, and the clock its painted position counts from. The monotonicity
        /// counter is not reset — §4.4's "never backwards" is asked of *every* frame the loop reads,
        /// and a visit's base only makes the position smaller, which is the one intended step back
        /// (a new track starts at 0) and cannot happen inside a visit.
        fn start(&mut self, total: u32, base: u32) {
            self.total = total;
            self.base = base;
            self.last = 0;
        }

        /// Whether a video frame has been latched since this reader last served one — §4.5's
        /// `frame_due`. One load and a compare, so the producer loop asks it between frames.
        fn due(&self) -> bool {
            frame_latch() != self.served
        }

        /// One frame's read: the committed samples, the painted position, and the monotonicity
        /// check. No critical section — an aligned 32-bit load is atomic on ARM7TDMI
        /// (`position::committed_samples`).
        fn sample(&mut self) {
            self.served = frame_latch();
            let visit = position::committed_samples().saturating_sub(self.base);
            let paint = position::paint_position(visit, self.total);
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
    /// frame and no entry repeating the previous mask. One entry is applied per sample, and since
    /// PR-3b-ii-B1 samples once per vblank, `F` **is** a video-frame number again: the seam is
    /// installed when the chain is armed, so frame 0 is the first vblank of the audio window and
    /// frame `n` is the `n`th one after it, exactly as the host twin counts it. The ROM prints the
    /// measured detection latency against these frames (`detect_latency max=…`), so the claim is
    /// checked rather than assumed. The *sequence* of transitions is fixed regardless, and it is what
    /// the ROM's end line can be read against: **14 edges and 7 autorepeats**, by the host twin's own
    /// arithmetic (`tests/album_seam_host.rs` drives this same shape).
    ///
    /// What it covers, and every row is a §5.3 control the gate has to see:
    ///
    /// | frames | input | what it witnesses |
    /// |---|---|---|
    /// | 8–66 | A, then Start | one pause/resume pair — Start and A are the same action, so this is also the control map |
    /// | 120–200 | Right, **held** 80 frames | next (1 → 2) and the autorepeat burst: 7 repeats, and they post no action, so a held direction skips once |
    /// | 240 | Left | prev (2 → 1) |
    /// | 300 | Down | restart the current track (1 → 1) — §4.4's replacement for a time-dependent chord |
    /// | 360–480 | Start, Right, Start | a skip **while paused** is refused and counted, then the resume |
    /// | 540 | Left | prev (1 → 0) |
    /// | 600 | Left | **wrap at the album's start** (0 → 3) |
    /// | 660 | Right | **wrap at the album's end** (3 → 0) |
    /// | 720 | Right | back onto a clip (0 → 1) |
    /// | 2470 | Right | a skip **near the end** of that clip's visit (29.2 s into 30 s), with the ring nearly a full queue deep |
    /// | 2510 | Left | and straight back; that visit then plays the clip out and the album **auto-advances** 1 → 2 → 3 to its end |
    ///
    /// The last two rows used to be discarded: the old loop broke as soon as the producer finished and
    /// its drain served no actions. The loop now stays alive through the drain (it has to — that is
    /// where a track's end is detected), so they act, and the run reaches the album's end at track 3
    /// with no wrap. That is what makes this arm witness D5 as well: the visit after the final Left
    /// plays to its own end and the log's `traversal:` line says `end=ALBUM_END`.
    ///
    /// Between them they exercise every rule `keys.rs` adds to agb and §4.4's pause, skip and traversal
    /// contracts, which is the whole of what a gate can witness without a cartridge. What they do
    /// **not** witness is a physical press: no emulator here can make one, and the ROM's log says so.
    ///
    /// The skipper visits the album's two 4.3-minute tracks only in passing (a few tenths of a second
    /// each, one skip apart) on purpose: they are the same blob and their whole-track folds are past
    /// the proof's cap, and the point of visiting them early is the *wrap*, not their length. The
    /// final visit does play track 2 in full on its way to the album's end, which is the traversal
    /// witness and costs the divergence only (the proof skips it by its frame cap). The pauses are
    /// short and both resume: a run that ended paused would never reach its verdict, and a run with a
    /// minutes-long pause would only cost the gate wall-clock, since the audio clock is stopped for the
    /// whole frozen span.
    #[cfg(feature = "input-replay")]
    const GATE_REPLAY_SCRIPT: [(u32, u16); 28] = [
        (8, keys::BTN_A),
        (14, 0),
        (60, keys::BTN_START),
        (66, 0),
        (120, keys::BTN_RIGHT),
        (200, 0),
        (240, keys::BTN_LEFT),
        (246, 0),
        (300, keys::BTN_DOWN),
        (306, 0),
        (360, keys::BTN_START),
        (366, 0),
        (390, keys::BTN_RIGHT),
        (396, 0),
        (480, keys::BTN_START),
        (486, 0),
        (540, keys::BTN_LEFT),
        (546, 0),
        (600, keys::BTN_LEFT),
        (606, 0),
        (660, keys::BTN_RIGHT),
        (666, 0),
        (720, keys::BTN_RIGHT),
        (726, 0),
        (2470, keys::BTN_RIGHT),
        (2476, 0),
        (2510, keys::BTN_LEFT),
        (2516, 0),
    ];

    // ------------------------------------------------------- R13's diagnostic scenarios

    /// R13's scenarios, in the same `(frame, mask)` units as [`GATE_REPLAY_SCRIPT`]: three press
    /// histories from the 2026-11-17 hardware session, so the emulator can be asked which of them a
    /// headless run refuses. Selected at build time by `replay-script-a|b|c`; with none of those
    /// features set [`REPLAY_SCRIPT`] is the gate's script, so `make album-test` is unchanged.
    ///
    /// These are **diagnostic arms, not gates**: they exist to classify R13 (a transport defect, a
    /// saturating counter, or a bound shaped by the gate's own script), and no bound was moved to
    /// suit them.
    ///
    /// To run one (the log is the verdict, and exit 124 is the pass path — a verdict loops forever):
    ///
    /// ```text
    /// cd examples/album_player
    /// cargo build --release --target thumbv4t-none-eabi --bin album-play \
    ///   --features "input-replay deep-audit replay-script-c"
    /// agb-gbafix target/thumbv4t-none-eabi/release/album-play -o /tmp/r13-c.gba
    /// timeout -k 5 240 mgba-test-runner /tmp/r13-c.gba | grep '^\[album-rom\]'
    /// ```
    ///
    /// **A — no presses at all.** The human's first leg: an unpressed playthrough of tracks 1→2→3
    /// ending `PASSED`. The fixture starts on track 1 (`START_TRACK`), plays it out, and the album
    /// auto-advances to its end.
    #[cfg(all(feature = "input-replay", feature = "replay-script-a"))]
    const REPLAY_SCRIPT_A: &[(u32, u16)] = &[];

    /// **B — Right, Right, then play the third track out.** The human's second leg. Two presses a
    /// few hundred ms apart (0.6 s here), each strictly after the previous transition settled, then
    /// nothing: track 3 plays to the album's end.
    #[cfg(all(feature = "input-replay", feature = "replay-script-b"))]
    const REPLAY_SCRIPT_B: &[(u32, u16)] = &[
        (150, keys::BTN_RIGHT),
        (156, 0),
        (288, keys::BTN_RIGHT),
        (294, 0),
    ];

    /// **C — interactive-heavy at human cadence.** The leg that ends in `FAILED` on the cartridge,
    /// and the one that reproduces it here. Every press is 5 frames (84 ms) held; the gaps are in
    /// video frames (~59.7/s):
    ///
    /// | frames | input | why it is here |
    /// |---|---|---|
    /// | 300–805 | **three** Start pause/resume pairs, 100 frames (1.67 s) apart | the report's "repeated Start pause/resume", inside track 1's 30 s visit so a resume always has a ring head to prime |
    /// | 900–1045 | Right/Left alternating every 20 frames (335 ms) | the report's "repeated Left/Right": one press per period triplet, so some land inside the previous transition |
    /// | 1100–1870 | **18** Rights every 45 frames (753 ms) | "more than three Right skips" — past the four the report names and past [`SKIP_LOG`]'s 16 |
    /// | 1870– | *(no more input)* | the settling: track 3, which the 18 Rights wrap onto from track 1, plays out to the album's end |
    ///
    /// The cadence is the whole point of the scenario: the gate's script spaces every action a track
    /// apart (~1000 frames), so it never has two transitions in flight and never approaches a fixed
    /// cap. This one does both, at the rate a human reaches them.
    #[cfg(all(feature = "input-replay", feature = "replay-script-c"))]
    const REPLAY_SCRIPT_C: &[(u32, u16)] = &[
        (300, keys::BTN_START),
        (305, 0),
        (400, keys::BTN_START),
        (405, 0),
        (500, keys::BTN_START),
        (505, 0),
        (600, keys::BTN_START),
        (605, 0),
        (700, keys::BTN_START),
        (705, 0),
        (800, keys::BTN_START),
        (805, 0),
        (900, keys::BTN_RIGHT),
        (905, 0),
        (920, keys::BTN_LEFT),
        (925, 0),
        (940, keys::BTN_RIGHT),
        (945, 0),
        (960, keys::BTN_LEFT),
        (965, 0),
        (980, keys::BTN_RIGHT),
        (985, 0),
        (1000, keys::BTN_LEFT),
        (1005, 0),
        (1020, keys::BTN_RIGHT),
        (1025, 0),
        (1040, keys::BTN_LEFT),
        (1045, 0),
        (1100, keys::BTN_RIGHT),
        (1105, 0),
        (1145, keys::BTN_RIGHT),
        (1150, 0),
        (1190, keys::BTN_RIGHT),
        (1195, 0),
        (1235, keys::BTN_RIGHT),
        (1240, 0),
        (1280, keys::BTN_RIGHT),
        (1285, 0),
        (1325, keys::BTN_RIGHT),
        (1330, 0),
        (1370, keys::BTN_RIGHT),
        (1375, 0),
        (1415, keys::BTN_RIGHT),
        (1420, 0),
        (1460, keys::BTN_RIGHT),
        (1465, 0),
        (1505, keys::BTN_RIGHT),
        (1510, 0),
        (1550, keys::BTN_RIGHT),
        (1555, 0),
        (1595, keys::BTN_RIGHT),
        (1600, 0),
        (1640, keys::BTN_RIGHT),
        (1645, 0),
        (1685, keys::BTN_RIGHT),
        (1690, 0),
        (1730, keys::BTN_RIGHT),
        (1735, 0),
        (1775, keys::BTN_RIGHT),
        (1780, 0),
        (1820, keys::BTN_RIGHT),
        (1825, 0),
        (1865, keys::BTN_RIGHT),
        (1870, 0),
    ];

    // The selection itself: one compiled-in script per build, chosen by feature. A gate build names
    // none of the three, so it gets the script `make album-test` asserts on.
    #[cfg(all(feature = "input-replay", feature = "replay-script-a"))]
    const REPLAY_SCRIPT: &[(u32, u16)] = REPLAY_SCRIPT_A;
    #[cfg(all(feature = "input-replay", feature = "replay-script-b"))]
    const REPLAY_SCRIPT: &[(u32, u16)] = REPLAY_SCRIPT_B;
    #[cfg(all(feature = "input-replay", feature = "replay-script-c"))]
    const REPLAY_SCRIPT: &[(u32, u16)] = REPLAY_SCRIPT_C;
    #[cfg(all(
        feature = "input-replay",
        not(any(
            feature = "replay-script-a",
            feature = "replay-script-b",
            feature = "replay-script-c"
        ))
    ))]
    const REPLAY_SCRIPT: &[(u32, u16)] = &GATE_REPLAY_SCRIPT;

    // One script per build: two scenario features would silently pick one of them by `cfg` order.
    #[cfg(all(feature = "replay-script-a", feature = "replay-script-b"))]
    compile_error!("replay-script-a and replay-script-b are two different key scripts; pick one");
    #[cfg(all(feature = "replay-script-b", feature = "replay-script-c"))]
    compile_error!("replay-script-b and replay-script-c are two different key scripts; pick one");
    #[cfg(all(feature = "replay-script-a", feature = "replay-script-c"))]
    compile_error!("replay-script-a and replay-script-c are two different key scripts; pick one");

    /// The track a direction command moves to, §5.3's wrap at the album's ends.
    ///
    /// No modulo: §6's rule is "no divide on any path", and two compares are cheaper than the divide
    /// the ring's own code refuses at the same size.
    fn neighbour(track: usize, action: keys::Action, n_tracks: usize) -> usize {
        match action {
            keys::Action::NextTrack => {
                let next = track + 1;
                if next == n_tracks { 0 } else { next }
            }
            keys::Action::PrevTrack => {
                if track == 0 {
                    n_tracks - 1
                } else {
                    track - 1
                }
            }
            // Down: the track it is already on (§5.3's restart, which exists so no rule here needs a
            // clock — §4.4/D1).
            _ => track,
        }
    }

    // ------------------------------------------------------------------ feeder

    /// Hand the next ring chunk to the free ping-pong slot and return the two pointers
    /// [`rearm_dma`] needs.
    ///
    /// Raw pointers to the slot BEFORE filling, in their own statement: the static never moves, so
    /// they stay valid past the borrow, and the caller consumes them once the copy is done.
    /// Destructuring to field borrows avoids double-borrowing the struct through two `IndexMut`s in
    /// one expression (which is what `bufs.l[bufs.next]` would be) —
    /// `product_play_main.rs:818-860`.
    ///
    /// `transport` is the hand-off gain's home (§4.4, PR-3b-ii-B2a): `take_chunk` has just copied the
    /// ring's next chunk into the slot that starts playing on the next `rearm_dma`, so scaling it
    /// *here* — after the copy and before the re-arm, in the same critical section — fades exactly
    /// what is about to be heard and never touches the ring. That is what makes a pause resumable
    /// without a seek; the producer-side shape the probe uses would dim 1.23 s of queued audio.
    ///
    /// The gain's home also owns the transition's deferral (`Transport::note_hand_off`): the note is
    /// taken **before** the gain is applied, so it asks about the slot the previous period handed over
    /// — the one that is finishing now — not about the ramp this period is about to apply.
    fn take_into_slot(
        bufs: &mut SndBufs,
        feed: &mut RingFeed<RING_SAMPLES>,
        finished: bool,
        transport: &mut Transport,
        scan: bool,
    ) -> (Chunk, *const u8, *const u8, bool) {
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
            // The deferral's whole cost (see `Transport::note_hand_off`): this is the boundary at
            // which the previous slot has finished playing, so the transport can tell that a
            // ramped-but-silent chunk has been served rather than merely handed over.
            transport.note_hand_off();
            // The hand-off, in place: no copy, no second buffer, and the ring's bytes are read
            // once and never written.
            transport.apply(dl, dr);
            // The bytes the DMA is about to be given, after the gain: that is what the skip's wire
            // watch asks about (see `all_silent`).
            let silent = scan && all_silent(dl) && all_silent(dr);
            *next = 1 - *next;
            (taken, silent)
        };
        (verdict.0, lp, rp, verdict.1)
    }

    /// Whether every byte of a handed-off slot is `0x00`.
    ///
    /// `0x00` **is** silence: `fifo_byte` is `(sample >> 8) as u8`, i.e. signed 8-bit around zero
    /// (`transport::scale_byte`'s docs), and `take_chunk` zero-fills whatever a `Starved` or `Tail`
    /// chunk did not fill. So "all zeros" is the wire's own silence verdict — the question the probe's
    /// skip arm asked (`probe_hw.rs:560-575`), asked here of the bytes after the hand-off gain has
    /// been applied rather than of the ring's.
    fn all_silent(bytes: &[u8]) -> bool {
        bytes.iter().all(|&byte| byte == 0)
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
    ///
    /// **The hand-off gain is the one addition (PR-3b-ii-B2a), and it is not bracketed.** A bracket
    /// here would be a second timer read pair on the deadline the bracket is protecting, so the
    /// measure of this handler's cost is the audit the reads above already feed: a feeder slower than
    /// the FIFO's 16-sample grace is `late`/`late_max`, and one that misses a period entirely is
    /// `coalesced`. The steady-state cost is one `is_transparent` compare plus the deferral's one
    /// `is_silent` compare and a store (a period with no fade in flight returns immediately); a fade
    /// period pays a multiply, shift and clamp per channel-sample over one chunk. Both runs' audits
    /// read `coalesced=0 late=0 late_max=0`, including the fade chunks the REPLAY run served, so the
    /// addition did not move the deadline.
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
        let mut transport = TRANSPORT.borrow_ref_mut(cs);

        // The skip's second measurement, read *before* the gain spends this chunk's cover count: a
        // countdown of one means this is the last cover chunk, so the next period is the new track's
        // own first chunk. `handler_entries` is incremented in the audit below (the reference's
        // order), so this entry's own index is `+ 1` and the next one is `+ 2`.
        if stats.watch_armed && stats.watch_content_at == 0 && transport.gain().held_chunks() == 1 {
            stats.watch_content_at = stats.handler_entries.wrapping_add(2);
        }

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
        let scan = stats.watch_armed && stats.watch_new_at == 0;
        let (verdict, lp, rp, silent) =
            take_into_slot(&mut *bufs, &mut *feed, finished, &mut transport, scan);
        // The wire watch (D4's "measured at AP-1, not asserted"): the first all-silent period after
        // the hand-off's mark, then the first non-silent one after that. `handler_entries` is the
        // period clock — it was just incremented, so this entry's index is the value latched.
        if scan {
            if stats.watch_silent_at == 0 {
                if silent {
                    stats.watch_silent_at = stats.handler_entries;
                }
            } else if !silent && stats.watch_new_at == 0 {
                stats.watch_new_at = stats.handler_entries;
            }
        }
        // The session clock (§10.4, ruled option A). It counts what this period actually
        // committed — a whole chunk, a classified tail's real samples, or nothing — so it equals
        // `RingFeed`'s `consumed` by construction rather than by a parallel computation. PR-3b-i
        // reads it and prints it; PR-3b-ii drives the bar and the mm:ss readout from it.
        let committed = match verdict {
            Chunk::Full => {
                stats.tail_padding = 0;
                CHUNK as u32
            }
            Chunk::Tail { real } => {
                stats.tail_periods += 1;
                // The boundary gap's sub-period term: what this period did not carry, and the DMA
                // therefore rendered as silence after the visit's last real sample.
                stats.tail_padding = CHUNK as u32 - real as u32;
                real as u32
            }
            Chunk::Starved => {
                stats.starved_periods += 1;
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

    // ------------------------------------------------- §4.4's pause, from both sides

    /// Hand the ring's head to the DMA through the hand-off gain: the boot prime, and a resume's.
    ///
    /// One chunk, then `rearm_dma`, exactly as the boot path does. `take_chunk` zero-pads a
    /// classified tail, so a prime at the very end of a track still hands over a whole buffer. The
    /// session clock is committed here for the same reason the boot prime commits: this chunk
    /// reached the DMA, so it is no longer queued.
    ///
    /// `false` means the ring had nothing to give — at boot that is a refusal, and after a resume it
    /// is a FAIL. Arming a chain with no primed buffer would start a starved run, which is the one
    /// thing the shipped order exists to prevent.
    fn prime_hand_off() -> bool {
        critical_section::with(|cs| {
            let mut bufs = BUFS.borrow_ref_mut(cs);
            let mut feed = FEED.borrow_ref_mut(cs);
            let mut transport = TRANSPORT.borrow_ref_mut(cs);
            let mut stats = STATS.borrow_ref_mut(cs);
            let finished = stats.finished;
            let (verdict, lp, rp, _) =
                take_into_slot(&mut *bufs, &mut *feed, finished, &mut transport, false);
            let committed = match verdict {
                Chunk::Full => CHUNK as u32,
                Chunk::Tail { real } => real as u32,
                Chunk::Starved => return false,
            };
            // SAFETY: aligned pointers into the static ping-pong, filled for a whole transfer by
            // `take_chunk` just above.
            unsafe { rearm_dma(lp, rp, &CHAIN) };
            position::commit(committed);
            stats.primes += 1;
            true
        })
    }

    // ----------------------------------------------------- §4.4's skip, from both sides

    /// The wire watch's state and the counters around it, read in one instant.
    struct Watch {
        /// The period the hand-off's mark was taken at.
        at: u32,
        /// The first all-silent period after the mark, and the first non-silent one after that.
        silent_at: u32,
        new_at: u32,
        /// The first period after the cover countdown: where the new track's own chunk starts.
        content_at: u32,
        /// The underflow count now, and at the mark: the delta is the skip's own claim. The mark's
        /// value is taken inside the hand-off's critical section, so a period starved between the mark
        /// and the record being opened cannot be lost from the delta.
        underflows: u32,
        underflows_at: u32,
        /// The starved periods now, and when the advancing visit's drain was observed: the delta is
        /// the track boundary's own silence, in whole periods (PR-3b-iii-B). A press adds a
        /// press-to-hand-off wait; a drain has none, so this delta is the whole of the machinery's
        /// share of the gap.
        starved: u32,
        starved_at: u32,
    }

    /// What the wire watch has seen so far (D4's "measured at AP-1, not asserted").
    fn watch_state() -> Watch {
        critical_section::with(|cs| {
            let stats = STATS.borrow_ref(cs);
            Watch {
                at: stats.watch_at,
                silent_at: stats.watch_silent_at,
                new_at: stats.watch_new_at,
                content_at: stats.watch_content_at,
                underflows: stats.underflows,
                underflows_at: stats.watch_underflows_at,
                starved: stats.starved_periods,
                starved_at: stats.advance_starved_at,
            }
        })
    }

    /// Stop the watch's scan. The next arm re-zeros it, so a resolved skip costs nothing until then.
    fn watch_disarm() {
        critical_section::with(|cs| STATS.borrow_ref_mut(cs).watch_armed = false);
    }

    /// One whole chunk of zeros as real ring data — §4.4 step 4's cover.
    ///
    /// **Whole chunks, always.** `take_chunk` takes whole chunks only, so a partial push left in the
    /// ring while the producer is running can never be taken: it starves the DMA forever and every
    /// later period books an underflow (`examples/album_probe/src/probe_hw.rs:735-740`). Each push
    /// is its own critical section for the same reason the producer's slices are: a `CHUNK`-sized
    /// section costs ~4,014 c, a whole cover's would be 4× that against a 16-sample (9,199 c) grace.
    fn push_cover_chunk() -> bool {
        critical_section::with(|cs| {
            let outcome = FEED.borrow_ref_mut(cs).push_bytes(&ZEROS, &ZEROS);
            let ok = outcome.is_ok();
            if ok {
                let mut stats = STATS.borrow_ref_mut(cs);
                stats.cover_samples = stats.cover_samples.saturating_add(CHUNK as u32);
            }
            ok
        })
    }

    /// What a skip's or a track end's hand-off did, in the units the ledger and the log read.
    struct HandOff {
        /// Samples `truncate()` stepped over — §4.4 step 2's own witness. **Zero on a track end**:
        /// the ring was already drained, so there was nothing stale to step over.
        dropped: u32,
        /// The session clock at the instant of the hand-off, before any cover chunk could be
        /// committed. The new visit's paint base is this plus the cover (see the caller).
        committed: u32,
        /// Samples of silence in the outgoing visit's last chunk: the zero padding of its classified
        /// tail, read inside the hand-off's section so a cover chunk taken by the feeder afterwards
        /// cannot clear it. Zero when the visit ended on a full chunk. This is the sub-period term of
        /// a track boundary's gap (PR-3b-iii-B).
        tail_padding: u32,
    }

    /// §4.4's steps 2, 4 (its first chunk) and 5's arming — in **one** critical section.
    ///
    /// Three things have to happen together, and each was learned the hard way:
    ///
    /// - the transport's transition (`begin_reprime`), because it arms the gain's cover hold and a
    ///   feeder period landing before the truncate would spend a cover count on a chunk of the old
    ///   track — starting the ramp-in one chunk early, which puts the new track's first samples on
    ///   the wire at full gain;
    /// - `RingFeed::truncate()`, because the queued span is the *old* track and §4.4 step 2 says it is
    ///   dropped, not played;
    /// - the cover's first whole chunk, because a **bare** truncate leaves a window in which a feeder
    ///   period finds an empty ring with the producer still running: that period is `Starved` and
    ///   books an underflow that is really an artefact of the window
    ///   (`examples/album_probe/src/probe_hw.rs:840-875` — `truncate_stale` does exactly this, and
    ///   this is the app's own re-implementation of that shape, not a call into the probe: §2.2/D4).
    ///
    /// `finished` is cleared here too. It marks *this visit's* last push, and the new track has not
    /// pushed anything yet — leaving it set would classify the new track's first starved period as a
    /// `Tail` and hide it from the underflow count.
    ///
    /// The wire watch's mark is taken at this instant rather than at the command, and that is the
    /// honest place for it: the fade-out was applied to the chunk the feeder had *already taken*, so
    /// the old track's ramp-out is heard **before** this mark, and what the mark measures is the
    /// hand-off — whether any stale audio follows it.
    ///
    /// **`truncate` is the one thing a press and a track end do not share.** A press's queued span
    /// is the old track and §4.4 step 2 drops it. A track end reaches here only after
    /// [`visit_drained`] (`consumed == pushed`), so the ring holds none of the old track and there
    /// is nothing stale to drop; calling `truncate()` there would put a zero in `dropped` that no
    /// truncate earned. The rest — the transport's transition, the cleared `finished`, the cover's
    /// first whole chunk, in one section — is the same sequence for both, which is why this is one
    /// function and not two.
    fn transition_hand_off(truncate: bool) -> Result<HandOff, &'static str> {
        critical_section::with(|cs| {
            let mut feed = FEED.borrow_ref_mut(cs);
            let mut transport = TRANSPORT.borrow_ref_mut(cs);
            let mut stats = STATS.borrow_ref_mut(cs);
            if !transport.begin_reprime(SKIP_PRIME_CHUNKS) {
                return Err("the hand-off was not in its transition");
            }
            let dropped = if truncate {
                let dropped = feed.truncate() as u32;
                stats.dropped_samples = stats.dropped_samples.saturating_add(dropped);
                dropped
            } else {
                // Nothing stale to drop: the ring was drained before the hand-off, so `dropped` is
                // zero by construction rather than by measurement.
                0
            };
            // The wire watch is armed for **both** kinds of hand-off (PR-3b-iii-B). A press's row
            // uses it to say no stale audio followed the truncate; a track end's uses the same latch
            // to say the same of a drained ring, and to put a number on the boundary's
            // silence. A press adds its own press-to-hand-off wait; a drain has none, because the
            // trigger is the drain itself.
            stats.watch_armed = true;
            stats.watch_at = stats.handler_entries;
            stats.watch_silent_at = 0;
            stats.watch_new_at = 0;
            stats.watch_content_at = 0;
            stats.watch_underflows_at = stats.underflows;
            let tail_padding = stats.tail_padding;
            stats.finished = false;
            let committed = position::committed_samples();
            feed.push_bytes(&ZEROS, &ZEROS)
                .map_err(|_| "the cover's first chunk did not fit")?;
            stats.cover_samples = stats.cover_samples.saturating_add(CHUNK as u32);
            Ok(HandOff {
                dropped,
                committed,
                tail_padding,
            })
        })
    }

    /// One track visit: which track, how much of it reached the ring, and where its timeline starts.
    #[derive(Clone, Copy)]
    struct Visit {
        track: usize,
        /// Samples of this track's own audio pushed into the ring during the visit.
        pushed: u32,
        /// The session clock when the visit's first content could reach the DMA — the hand-off's
        /// instant plus the cover. §4.4's "0 at a track start" is this subtraction.
        base: u32,
        /// The session clock when the visit ended (the next hand-off, or the end of the run).
        end: u32,
    }

    /// The run's track visits, in play order — the per-track ledger's rows and the content proof's
    /// list of what to fold.
    struct Visits {
        list: [Visit; VISIT_LOG],
        len: usize,
        /// Visits past the log. Counted, and part of the verdict: the proof enumerates the tracks
        /// from this list, so a dropped visit is a track nothing proved.
        overflow: u32,
    }

    impl Visits {
        fn new() -> Self {
            Self {
                list: core::array::from_fn(|_| Visit {
                    track: 0,
                    pushed: 0,
                    base: 0,
                    end: 0,
                }),
                len: 0,
                overflow: 0,
            }
        }

        /// Open a visit for `track`: `base` is where its painted position starts from.
        fn begin(&mut self, track: usize, base: u32) {
            if self.len == VISIT_LOG {
                self.overflow = self.overflow.saturating_add(1);
                return;
            }
            self.list[self.len] = Visit {
                track,
                pushed: 0,
                base,
                end: base,
            };
            self.len += 1;
        }

        /// Close the open visit: how much of the track reached the ring, and where the clock stood.
        fn close(&mut self, pushed: u32, end: u32) {
            if self.len == 0 {
                return;
            }
            let visit = &mut self.list[self.len - 1];
            visit.pushed = pushed;
            visit.end = end;
        }

        fn list(&self) -> &[Visit] {
            &self.list[..self.len]
        }

        /// Σ of every visit's content pushes — the figure `Stats::content_pushed` is checked against,
        /// so the two bookkeepings cannot part.
        fn pushed_total(&self) -> u32 {
            let mut total = 0u32;
            for visit in self.list() {
                total = total.saturating_add(visit.pushed);
            }
            total
        }
    }

    /// One skip, in feeder periods — the probe's units, so these rows can be read against its table.
    #[derive(Clone, Copy)]
    struct SkipRecord {
        from_track: usize,
        to_track: usize,
        /// The mark: the hand-off's period.
        at: u32,
        /// The first all-silent period after the mark, and the first non-silent one after that.
        silent_at: u32,
        new_at: u32,
        /// The first chunk after the cover countdown — the new track's own first chunk.
        content_at: u32,
        /// `underflows` at the mark, so the delta is the skip's own claim.
        underflows_at: u32,
        underflows: u32,
        /// Samples `truncate()` stepped over at this hand-off.
        dropped: u32,
        /// The period the main loop took the post on: what makes the frame loop's own cost
        /// measurable rather than inferable. It is the **take**, not the press: the press→detect and
        /// detect→take legs are the ISR seam's and are measured by `Input` (the verdict's `reaction`
        /// term), so this field's row starts where the main loop woke up. See
        /// [`SkipRecord::take_to_handoff`].
        take_at: u32,
        /// The watch has not resolved yet.
        open: bool,
    }

    impl SkipRecord {
        /// Periods from the mark to the first silent chunk on the wire. `1` is the watch's floor (the
        /// first post-mark period), so the *delivered* stale periods are [`SkipRecord::old`].
        fn silent_after_mark(&self) -> u32 {
            self.silent_at.wrapping_sub(self.at)
        }

        /// Stale-track periods heard after the hand-off: the raw reading minus the watch's floor, the
        /// same convention the probe's table uses (`README.md` → "The deep arm", the † note).
        ///
        /// In this architecture the gain has already reached silence by the mark, so a stale chunk
        /// that did reach the wire would read as silence and this term would not see it — the term
        /// that catches a missing truncate here is [`SkipRecord::to_new`] (41 periods for a drain at
        /// this prime instead of 5) and `dropped`, which is what the truncate actually stepped over.
        /// Said here rather than left for a reader to discover: this is the weakest of the three.
        fn old(&self) -> u32 {
            self.silent_after_mark().saturating_sub(1)
        }

        /// Periods the wire carried the cover's silence.
        fn silence(&self) -> u32 {
            self.new_at.wrapping_sub(self.silent_at)
        }

        /// Periods from the hand-off to the new track's first audible chunk — the probe's own
        /// question (`new_at - at`), and the listener's.
        ///
        /// It equals `SKIP_PRIME_CHUNKS + 1` for a track that starts audibly, which is every clip in
        /// this fixture and was every row of the probe's table. It reads **longer** for a track whose
        /// own head is digital silence, and this fixture has two of those: tracks 0 and 2 are both the
        /// packed master from offset 0, whose own head is all-zero for 4,480 samples (273 ms at
        /// 16,384 Hz) — the wire watch reads 8 periods of lead-in there and 0 on the two clips, which
        /// are cut at t=1.0 s and t=57.0 s and open audibly. A silent chunk of the track and a chunk
        /// of the cover are the same bytes on the wire, so `to_new` cannot separate them: it is a
        /// property of the track, not of the skip. The gate is therefore stated on
        /// [`SkipRecord::content_at`] and this figure is reported beside it, with
        /// [`SkipRecord::head_silence`] naming the difference as the track's own.
        fn to_new(&self) -> u32 {
            self.new_at.wrapping_sub(self.at)
        }

        /// Periods from the hand-off to the first chunk handed over after the cover countdown: the
        /// machinery's transition, which is `SKIP_PRIME_CHUNKS + 1` and nothing else.
        fn content_at(&self) -> u32 {
            self.content_at.wrapping_sub(self.at)
        }

        /// Periods of the new track's own silence that follow the cover — `to_new - content_at`.
        ///
        /// Zero for a track that opens audibly, and **8** for both full-master tracks in this fixture.
        /// Measured: the master's head is all-zero for the first 4,480 samples at 16,384 Hz, and the
        /// first non-zero sample is a **−1** at index 4,951 — inside the ninth chunk, where
        /// `(sample >> 8) as u8` turns it into `0xFF`, a full-scale-negative byte. So the wire stops
        /// calling the track silent exactly 8 periods after its own first chunk. That is the track's,
        /// so it is printed with its own name and is not gated: a gate on it would fail on every album
        /// whose first track fades in from nothing, which is a *musical* fact and not a defect in the
        /// hand-off.
        ///
        /// **Pinned on the host, not inferred from the log**: (`album_seam_host`'s
        /// `the_masters_head_is_digital_silence_in_the_fifos_own_bytes`) decodes that head through
        /// `FlacSource` → `pull_frame` → `convert_frame` and asserts the FIFO bytes are **exactly
        /// `0x00`**, with the first non-zero byte at index 4,951. So the wire watch's all-zero
        /// predicate is not misreporting a quiet passage as silence — the 8 is real music's digital
        /// silence and `to_new` needs no bound of its own.
        fn head_silence(&self) -> u32 {
            self.to_new().saturating_sub(self.content_at())
        }

        fn underflow_delta(&self) -> u32 {
            self.underflows.saturating_sub(self.underflows_at)
        }

        /// Periods from the main loop taking the post to the hand-off's critical section: the part
        /// of the listener's wait that this app owns.
        ///
        /// Measured, not inferred, and **not** the press's whole latency: it starts at the take, so
        /// the press→detect (≤ 1 video frame) and detect→take (one main-loop pass — a producer frame
        /// is 3.7–3.9 video frames of uninterruptible decode) legs are outside it and are the
        /// `reaction` term's. What it does span is the one fade period §4.4 step 1 costs plus
        /// however long the frame loop takes to come back around to see the gain has reached silence,
        /// so a regression in that loop (a longer decode pass, a loop that stops serving latches)
        /// moves this number and nothing else in the ledger. The rest of the wait to audio is
        /// [`SkipRecord::content_at`] — the cover, 5 periods = 171 ms — and
        /// [`SkipRecord::head_silence`], which is the new track's own.
        ///
        /// The physical floor, stated once: the transition cannot straddle a decode frame (the take
        /// happens after the producer's inner loop, so its floor is one producer frame) and cannot
        /// straddle a period (`apply` runs at a boundary, and one period is 2.04 video frames). The
        /// three legs read together are therefore ~1 + 3.9 + 2.04 ≈ 7 video frames ≈ 115 ms to the
        /// hand-off, before the cover.
        ///
        /// The bound it is gated against is the contract's [`SKIP_PRESS_TO_HANDOFF_PERIODS`], whose
        /// name predates the three-legged split: its value bounds this leg only.
        fn take_to_handoff(&self) -> u32 {
            self.at.wrapping_sub(self.take_at)
        }

        /// The truncate's real witness: the samples `RingFeed::truncate()` discarded.
        ///
        /// `old` cannot be that witness — during the ramped tail the gain is at zero, so a stale
        /// chunk served there reads as silence just like an empty ring, and `old == 0` cannot tell
        /// "nothing stale was served" from "something stale was served quietly". `dropped` is what
        /// makes the claim checkable: a pressed skip must step over a queued span (at least one
        /// sample), and cannot step over more than the ring can hold less the [`FADE_PERIODS`]
        /// chunks the feeder served before the hand-off (`RING_SAMPLES - FADE_PERIODS × CHUNK`).
        fn dropped_is_a_truncate(&self) -> bool {
            self.dropped >= 1
                && self.dropped <= (RING_SAMPLES - FADE_PERIODS as usize * CHUNK) as u32
        }

        /// The machinery's claim about one skip, and the frame loop's — nothing about the *track*.
        ///
        /// `content_at` is the gated transition: the cover's own depth plus the hand-off period,
        /// so `SKIP_PRIME_CHUNKS + 1`. `to_new` is **not** gated against it, because a track whose
        /// own head is digital silence cannot be told from the cover on the wire (see
        /// [`SkipRecord::head_silence`]); `to_new >= content_at` is the only order claim the bytes
        /// can make. `old` and `underflow_delta` are what a hand-off that got its critical section
        /// wrong would book, but `old` is the weaker term (see
        /// [`SkipRecord::dropped_is_a_truncate`]) — `dropped` is the truncate's own witness — and
        /// `take_to_handoff` is the frame loop's own latency, bounded by
        /// [`SKIP_PRESS_TO_HANDOFF_PERIODS`].
        fn ok(&self) -> bool {
            !self.open
                && self.new_at != 0
                && self.content_at() == SKIP_PRIME_CHUNKS + 1
                && self.to_new() >= self.content_at()
                && self.old() == 0
                && self.underflow_delta() == 0
                && self.dropped_is_a_truncate()
                && self.take_to_handoff() <= SKIP_PRESS_TO_HANDOFF_PERIODS
        }
    }

    /// The run's skips: the verdict's terms, and the rows the log prints.
    struct Skips {
        records: [SkipRecord; SKIP_LOG],
        recorded: usize,
        /// Direction commands delivered but refused — a skip while paused or mid-transition. Counted
        /// because a silently dropped press is the thing §10.5's honesty discipline forbids.
        ignored: u32,
        /// Skips past the log: gated like [`Visits::overflow`].
        overflow: u32,
    }

    impl Skips {
        fn new() -> Self {
            Self {
                records: core::array::from_fn(|_| SkipRecord {
                    from_track: 0,
                    to_track: 0,
                    at: 0,
                    silent_at: 0,
                    new_at: 0,
                    content_at: 0,
                    underflows_at: 0,
                    underflows: 0,
                    dropped: 0,
                    take_at: 0,
                    open: false,
                }),
                recorded: 0,
                ignored: 0,
                overflow: 0,
            }
        }

        /// Record a skip's hand-off. The watch fields stay zero until it resolves.
        fn begin(
            &mut self,
            from: usize,
            to: usize,
            take_at: u32,
            hand_off: &HandOff,
            watch: &Watch,
        ) {
            if self.recorded == SKIP_LOG {
                self.overflow = self.overflow.saturating_add(1);
                return;
            }
            self.records[self.recorded] = SkipRecord {
                from_track: from,
                to_track: to,
                at: watch.at,
                silent_at: 0,
                new_at: 0,
                content_at: 0,
                underflows_at: watch.underflows_at,
                underflows: watch.underflows,
                dropped: hand_off.dropped,
                take_at,
                open: true,
            };
            self.recorded += 1;
        }

        /// Whether a skip is still waiting for the new track to reach the wire.
        fn open(&self) -> bool {
            self.recorded > 0 && self.records[self.recorded - 1].open
        }

        /// Take the watch's reading for the open skip, if it has resolved.
        fn resolve(&mut self, watch: &Watch) -> bool {
            if !self.open() {
                return false;
            }
            let record = &mut self.records[self.recorded - 1];
            record.underflows = watch.underflows;
            if watch.new_at == 0 {
                return false;
            }
            record.silent_at = watch.silent_at;
            record.new_at = watch.new_at;
            record.content_at = watch.content_at;
            record.open = false;
            true
        }

        /// Every skip's cover was exactly `SKIP_PRIME_CHUNKS` chunks, the new track's own chunk
        /// followed it immediately, no stale audio was heard after the hand-off, no underflow was
        /// booked, and every press reached its hand-off inside the contract's bound.
        ///
        /// The terms are [`SkipRecord::ok`]'s, so this summary and the rows below cannot disagree
        /// about which skip failed or why.
        fn ok(&self) -> bool {
            let mut clean = self.overflow == 0 && !self.open();
            for record in &self.records[..self.recorded] {
                clean &= record.ok();
            }
            clean
        }

        /// The worst observed `take→hand-off` and `content_at`, in periods: the two legs of the
        /// listener's wait the skip rows own. The end-of-run chain line adds them to the ISR seam's
        /// three legs, so press→audible is printed as one number instead of inferred.
        #[cfg(feature = "input-replay")]
        fn worst_legs(&self) -> (u32, u32) {
            let mut hand_off = 0u32;
            let mut audible = 0u32;
            for record in &self.records[..self.recorded] {
                hand_off = hand_off.max(record.take_to_handoff());
                audible = audible.max(record.content_at());
            }
            (hand_off, audible)
        }

        /// The end-of-run lines: one summary, then one row per skip in the probe's units.
        ///
        /// Every gated term is printed next to the number it is gated against, and the track's own
        /// lead-in is printed last as `head_silence`: a row that showed `to_new=13` beside an
        /// unlabelled "expected 5" is what a reader (rightly) took for a failing assertion
        /// (ALBUM-PLAYER.md §4.5's ledger rule). The row's `old=(weak: zero-gain hides stale)` is the
        /// short form of the summary's note: `old` cannot witness a stale chunk served at zero gain.
        /// The numeric `dropped` bound is in the summary and the ROM's `ok()`, not the row, because
        /// a line past 256 bytes is truncated by the harness's logger.
        fn report(&self) {
            agb::println!(
                "[album-rom] skip: skips={} ignored={} limit={} expected_content_at={} take\u{2192}hand-off<={} periods (old=0 is weak: zero-gain hides stale; dropped= is the truncate witness, 1..={}) [{}]",
                self.recorded,
                self.ignored,
                SKIP_LOG,
                SKIP_PRIME_CHUNKS + 1,
                SKIP_PRESS_TO_HANDOFF_PERIODS,
                RING_SAMPLES - FADE_PERIODS as usize * CHUNK,
                if self.ok() { "PASS" } else { "FAIL" }
            );
            for record in &self.records[..self.recorded] {
                agb::println!(
                    "[album-rom] skip: \u{2192}track {} (from {}) content_at={} periods (expected {}) old={} (weak: zero-gain hides stale) underflow_delta={} take\u{2192}hand-off={} periods (bound {}) head_silence={} periods to_new={} periods silence={} dropped={} [{}]",
                    record.to_track,
                    record.from_track,
                    record.content_at(),
                    SKIP_PRIME_CHUNKS + 1,
                    record.old(),
                    record.underflow_delta(),
                    record.take_to_handoff(),
                    SKIP_PRESS_TO_HANDOFF_PERIODS,
                    record.head_silence(),
                    record.to_new(),
                    record.silence(),
                    record.dropped,
                    if record.ok() { "PASS" } else { "FAIL" }
                );
            }
            if !self.ok() {
                agb::println!(
                    "[album-rom] FAIL a skip did not land: every skip's cover must be {} chunks followed at once by the new track's own chunk, with no stale audio after the hand-off, no underflow booked, a truncate that dropped a queued span no larger than the ring, and every press inside its {}-period hand-off bound (old=0 alone cannot see a stale chunk served at zero gain — dropped= is the truncate's witness)",
                    SKIP_PRIME_CHUNKS,
                    SKIP_PRESS_TO_HANDOFF_PERIODS
                );
            }
        }
    }

    /// How many track-end advances the run keeps for its end-of-run log: [`VISIT_LOG`]'s visits can
    /// hold at most one fewer, and a fixture with more advances than this is not a gate.
    const ADVANCE_LOG: usize = 16;

    /// One track-end auto-advance, in feeder periods and off the same wire watch as a skip's row, so
    /// the two can be read side by side (PR-3b-iii-B).
    ///
    /// Two things a skip's row has are missing here, and both are properties: there is no press, so
    /// no `take_to_handoff` term (the trigger is the drain); and there is no truncate, so `dropped`
    /// is zero by construction rather than measured. What the row adds is the **gap**: a drained
    /// ring means the silence a listener hears at the boundary is countable from three terms — the
    /// outgoing visit's tail padding, the periods the feeder was starved between the drain and the
    /// hand-off, and the cover the hand-off pushes. Q4 rules that the lossless path *has* inter-track
    /// gaps and says so; this is the number it says it with. It is **derived**, not measured: 4 of
    /// its periods are the `SKIP_PRIME_CHUNKS` design constant, and only `tail_padding` and `starved`
    /// are counted.
    #[derive(Clone, Copy)]
    struct AdvanceRecord {
        from_track: usize,
        to_track: usize,
        /// The mark: the hand-off's period.
        at: u32,
        silent_at: u32,
        new_at: u32,
        content_at: u32,
        underflows_at: u32,
        underflows: u32,
        /// Starved periods from the drain to the hand-off.
        starved: u32,
        /// Samples of zero padding in the outgoing visit's last chunk.
        tail_padding: u32,
        /// Samples `truncate()` stepped over: **zero by construction** here — a track end reaches
        /// its hand-off only after the ring has drained, so there was nothing stale to drop. Printed
        /// and gated so the track-end row's claim is the same shape as a press's, not an unstated
        /// omission.
        dropped: u32,
        open: bool,
    }

    impl AdvanceRecord {
        /// Stale-track periods heard after the hand-off, on the skip row's convention: the raw
        /// reading minus the watch's floor. A drained visit left nothing queued, so this can only be
        /// zero — a non-zero reading would be a hand-off that replayed the old track.
        fn old(&self) -> u32 {
            self.silent_at.wrapping_sub(self.at).saturating_sub(1)
        }

        fn underflow_delta(&self) -> u32 {
            self.underflows.saturating_sub(self.underflows_at)
        }

        /// Periods from the hand-off to the first chunk handed over after the cover countdown: the
        /// machinery's transition, `SKIP_PRIME_CHUNKS + 1` and nothing else.
        fn content_at(&self) -> u32 {
            self.content_at.wrapping_sub(self.at)
        }

        /// Samples of silence between the outgoing visit's last real sample and the incoming
        /// track's first chunk: tail padding + starved periods + the cover.
        ///
        /// The cover is [`SKIP_PRIME_CHUNKS`] periods here, not + 1: the mark's own period is
        /// already counted in `starved` (the hand-off happens inside it, after the feeder served a
        /// starved chunk), which is why a skip's `content_at` reads 5 while four cover chunks are
        /// pushed. The new track's own head, if it opens on digital silence, is not in this figure:
        /// it belongs to the track, exactly as a skip's `head_silence` does.
        fn gap_samples(&self) -> u32 {
            self.tail_padding.saturating_add(
                self.starved
                    .saturating_add(SKIP_PRIME_CHUNKS)
                    .saturating_mul(CHUNK as u32),
            )
        }

        /// The boundary's claim: the new track's first chunk at the machinery's
        /// `SKIP_PRIME_CHUNKS + 1` periods, no stale audio after the mark, no underflow booked, and
        /// no truncate — the ring had already drained, so `dropped` must be zero. Note that `old`
        /// alone is the weaker witness (a stale chunk served at zero gain would read as silence);
        /// the advance's stronger witness is that every sample was handed over before the mark.
        /// The gap is derived and printed but **not gated** — §4.4 and Q4 accept an inter-track gap
        /// in v1, and closing it is Phase 5's cross-track ring bridge.
        fn ok(&self) -> bool {
            !self.open
                && self.new_at != 0
                && self.content_at() == SKIP_PRIME_CHUNKS + 1
                && self.old() == 0
                && self.underflow_delta() == 0
                && self.dropped == 0
        }
    }

    /// The run's advances: the verdict's term, and the rows the log prints.
    struct Advances {
        records: [AdvanceRecord; ADVANCE_LOG],
        recorded: usize,
        /// Advances past the log: gated like [`Visits::overflow`].
        overflow: u32,
    }

    impl Advances {
        fn new() -> Self {
            Self {
                records: core::array::from_fn(|_| AdvanceRecord {
                    from_track: 0,
                    to_track: 0,
                    at: 0,
                    silent_at: 0,
                    new_at: 0,
                    content_at: 0,
                    underflows_at: 0,
                    underflows: 0,
                    starved: 0,
                    tail_padding: 0,
                    dropped: 0,
                    open: false,
                }),
                recorded: 0,
                overflow: 0,
            }
        }

        /// Record an advance's hand-off. The watch fields stay zero until it resolves.
        fn begin(&mut self, from: usize, to: usize, hand_off: &HandOff, watch: &Watch) {
            if self.recorded == ADVANCE_LOG {
                self.overflow = self.overflow.saturating_add(1);
                return;
            }
            self.records[self.recorded] = AdvanceRecord {
                from_track: from,
                to_track: to,
                at: watch.at,
                silent_at: 0,
                new_at: 0,
                content_at: 0,
                underflows_at: watch.underflows_at,
                underflows: watch.underflows,
                starved: watch.starved.saturating_sub(watch.starved_at),
                tail_padding: hand_off.tail_padding,
                dropped: hand_off.dropped,
                open: true,
            };
            self.recorded += 1;
        }

        /// Whether an advance is still waiting for the new track to reach the wire.
        fn open(&self) -> bool {
            self.recorded > 0 && self.records[self.recorded - 1].open
        }

        /// Take the watch's reading for the open advance, if it has resolved.
        fn resolve(&mut self, watch: &Watch) -> bool {
            if !self.open() {
                return false;
            }
            let record = &mut self.records[self.recorded - 1];
            record.underflows = watch.underflows;
            if watch.new_at == 0 {
                return false;
            }
            record.silent_at = watch.silent_at;
            record.new_at = watch.new_at;
            record.content_at = watch.content_at;
            record.open = false;
            true
        }

        /// Every advance landed at the machinery's `SKIP_PRIME_CHUNKS + 1` periods with no stale
        /// audio and no underflow, per [`AdvanceRecord::ok`].
        fn ok(&self) -> bool {
            let mut clean = self.overflow == 0 && !self.open();
            for record in &self.records[..self.recorded] {
                clean &= record.ok();
            }
            clean
        }

        /// The end-of-run lines: one summary, then one row per advance with its gap in periods, in
        /// samples and in milliseconds. One period is `CHUNK / RATE_HZ` = 34.18 ms at 16,384 Hz;
        /// that conversion is stated here rather than left to the reader because it is the figure
        /// Q4's ruling is quoted with.
        fn report(&self) {
            agb::println!(
                "[album-rom] advance: advances={} limit={} expected_content_at={} periods [{}]",
                self.recorded,
                ADVANCE_LOG,
                SKIP_PRIME_CHUNKS + 1,
                if self.ok() { "PASS" } else { "FAIL" }
            );
            for record in &self.records[..self.recorded] {
                let gap = record.gap_samples();
                // Two divides, once per advance and after the teardown — never on the audio path.
                let tenths = (gap * 10 + CHUNK as u32 / 2) / CHUNK as u32;
                let ms = u64::from(gap) * 1_000 / u64::from(geometry::RATE_HZ);
                agb::println!(
                    "[album-rom] advance: \u{2192}track {} (from {}) content_at={} periods old={} (weak: zero-gain hides stale) underflow_delta={} starved={} dropped={} (a drained ring has nothing to drop) gap={}.{} periods ({} samples = {} ms at {} Hz) [{}]",
                    record.to_track,
                    record.from_track,
                    record.content_at(),
                    record.old(),
                    record.underflow_delta(),
                    record.starved,
                    record.dropped,
                    tenths / 10,
                    tenths % 10,
                    gap,
                    ms,
                    geometry::RATE_HZ,
                    if record.ok() { "PASS" } else { "FAIL" }
                );
            }
            if !self.ok() {
                agb::println!(
                    "[album-rom] FAIL a track-end advance did not land: every advance must reach the new track's first chunk {} periods after the hand-off, with no stale audio after the mark, no underflow booked, and dropped=0 (the ring had drained; a track end never truncates)",
                    SKIP_PRIME_CHUNKS + 1
                );
            }
        }
    }

    /// §4.4's pause, applied by the main loop at a period boundary: the timer chain, then both sound
    /// DMAs, and nothing else — the ring and the decoder are left exactly as they are.
    ///
    /// The order is the shipped teardown's (`product_play_main.rs:1844-1846`): timer1 first, so no
    /// period is entered after its counter is frozen, then timer0/timer2, then `teardown_audio`.
    ///
    /// **The underflow counter cannot tick while frozen, and that is a property rather than a flag.**
    /// It is incremented only in the feeder's `Chunk::Starved` arm, and a stopped timer1 means the
    /// feeder is not entered at all — so a frozen chain cannot look like a starved one, and there is
    /// no "paused" test in the hot path that could fall out of step with the hardware. A starved
    /// period pending at the freeze was booked before it, and nothing is rewritten after.
    ///
    /// The period audit's epoch ends here: `arm_audio_chain` reloads timer2 from zero, so the periods
    /// served so far are banked and `last_period` restarts at zero. Without that, the first entry
    /// after a resume would read a ~65,000-period gap and book it as coalesced overflows.
    ///
    /// The whole transition is one critical section, and so is the resume below. That is not about
    /// the ring: it is so no **vblank** bracket can straddle the counter reset. The handler brackets
    /// itself on `TM1_DATA`/`TM2_DATA`, so one that entered before the stop and left after the re-arm
    /// would read a 65,000-period jump and record a handler that sat for a whole minute — a FAIL with
    /// no fault behind it. A few hundred masked cycles twice per pause cost nothing the audio can
    /// notice, and the feeder is not running yet when this returns.
    fn freeze_chain(timers: &mut agb::timer::AllTimers<'_>) {
        critical_section::with(|cs| {
            timers.timer1.set_interrupt(false).set_enabled(false);
            timers.timer0.set_enabled(false);
            timers.timer2.set_enabled(false);
            teardown_audio();
            let mut stats = STATS.borrow_ref_mut(cs);
            stats.periods_before = stats
                .periods_before
                .saturating_add(u32::from(stats.last_period));
            stats.last_period = 0;
        });
    }

    /// The transport's state, read in one critical section.
    fn transport_state() -> State {
        critical_section::with(|cs| TRANSPORT.borrow_ref(cs).state())
    }

    /// Whether this visit's audio has all reached the DMA: the producer pushed its last sample
    /// (`finished`, set in that push's own critical section) and the feeder has handed every queued
    /// sample over, the classified tail included (`consumed == pushed`).
    ///
    /// This is the trigger for a track's end (PR-3b-iii-A). It is deliberately *not* "the producer
    /// finished": at that instant up to 1.23 s of the track is still queued, and a hand-off there
    /// would drop the track's own ending.
    fn visit_drained() -> bool {
        critical_section::with(|cs| {
            let stats = STATS.borrow_ref(cs);
            let feed = FEED.borrow_ref(cs);
            stats.finished && feed.consumed() >= feed.pushed()
        })
    }

    /// Latch the starved periods at the instant a visit's drain is observed and its advance posted.
    ///
    /// The feeder is running (the chain is stopped only at teardown), and from the drain until the
    /// hand-off's cover lands its ring is empty, so every period there is `Starved` — with `finished`
    /// set, so none is an underflow. Those periods are the machinery's share of the boundary's
    /// silence; `transition_hand_off` reads the delta against this baseline (PR-3b-iii-B).
    fn note_advance_start() {
        critical_section::with(|cs| {
            let mut stats = STATS.borrow_ref_mut(cs);
            stats.advance_starved_at = stats.starved_periods;
        });
    }

    /// The feeder's entry count — the fade's own budget, read at the two ends of a transition.
    fn handler_entries() -> u32 {
        critical_section::with(|cs| STATS.borrow_ref(cs).handler_entries)
    }

    /// The hand-off gain as it stands, for the pause log (it must read 0 at a freeze).
    fn handoff_gain() -> i32 {
        critical_section::with(|cs| TRANSPORT.borrow_ref(cs).gain().gain())
    }

    /// How many pause/resume pairs this ROM keeps for its end-of-run log.
    ///
    /// Bounded for the same reason [`EDGE_LOG`] is: the record is filled from the audio path, where
    /// an `agb::println!` (a semihosting trap) is forbidden, and printed after the teardown.
    const PAUSE_LOG: usize = 4;

    /// One pause/resume pair as the main loop observed it.
    ///
    /// `resumed_committed − frozen_committed` is the whole claim — **0**, because the ring is never
    /// touched — and `frozen_gain` is its other half (the fade reached silence before the chain
    /// stopped). The frame span is the response, in the unit the contract's bounds are stated in.
    struct PauseRecord {
        action_frame: u32,
        frozen_frame: u32,
        frozen_gain: i32,
        frozen_committed: u32,
        /// Feeder periods between the toggle and the freeze: the fade's own cost, in periods.
        fade_periods: u32,
        resumed_frame: u32,
        resumed_committed: u32,
    }

    /// The pause ledger: totals for the verdict, and the first [`PAUSE_LOG`] pairs in full.
    struct Pauses {
        /// Completed pairs (frozen and resumed).
        pairs: u32,
        /// Pause/resume toggles taken, and the ones ignored because a transition was in flight.
        toggles: u32,
        ignored: u32,
        /// The pair currently open: the toggle's frame and the feeder's entry count.
        opening_frame: u32,
        opening_periods: u32,
        records: [PauseRecord; PAUSE_LOG],
        recorded: usize,
    }

    impl Pauses {
        fn new() -> Self {
            Self {
                pairs: 0,
                toggles: 0,
                ignored: 0,
                opening_frame: 0,
                opening_periods: 0,
                records: core::array::from_fn(|_| PauseRecord {
                    action_frame: 0,
                    frozen_frame: 0,
                    frozen_gain: 0,
                    frozen_committed: 0,
                    fade_periods: 0,
                    resumed_frame: 0,
                    resumed_committed: 0,
                }),
                recorded: 0,
            }
        }

        /// A pause was requested: the fade-out is issued and the chain is still running.
        fn begin(&mut self, frame: u32, periods: u32) {
            self.toggles += 1;
            self.opening_frame = frame;
            self.opening_periods = periods;
        }

        /// The chain stopped. The gain must be zero here or the pause ends on a step.
        fn frozen(&mut self, frame: u32, committed: u32, gain: i32, periods: u32) {
            let record = PauseRecord {
                action_frame: self.opening_frame,
                frozen_frame: frame,
                frozen_gain: gain,
                frozen_committed: committed,
                fade_periods: periods.saturating_sub(self.opening_periods),
                resumed_frame: 0,
                resumed_committed: 0,
            };
            if self.recorded < PAUSE_LOG {
                self.records[self.recorded] = record;
                self.recorded += 1;
            }
        }

        /// The chain was primed and re-armed: close the open pair and read off what it cost.
        fn resumed(&mut self, frame: u32, committed: u32) {
            self.toggles += 1;
            if self.recorded > 0 {
                let record = &mut self.records[self.recorded - 1];
                if record.resumed_frame == 0 {
                    record.resumed_frame = frame;
                    record.resumed_committed = committed;
                }
            }
            self.pairs += 1;
        }

        /// A toggle arrived while a transition was already running: no cancel, so it is counted.
        fn ignored(&mut self) {
            self.toggles += 1;
            self.ignored += 1;
        }

        /// Samples lost across a pair: zero by contract.
        fn dropped(record: &PauseRecord) -> u32 {
            record
                .resumed_committed
                .saturating_sub(record.frozen_committed)
        }

        /// Every pair kept the position exactly, every fade reached silence before the chain stopped,
        /// and every toggle is accounted for.
        fn ok(&self) -> bool {
            let mut clean = self.recorded > 0 || self.pairs == 0;
            for record in &self.records[..self.recorded] {
                clean &= record.frozen_gain == 0
                    && Self::dropped(record) == 0
                    && record.frozen_frame >= record.action_frame
                    && record.resumed_frame >= record.frozen_frame;
            }
            clean && self.pairs * 2 + self.ignored == self.toggles
        }

        /// The end-of-run lines: one summary, then one line per pair (the first [`PAUSE_LOG`]).
        fn report(&self) {
            agb::println!(
                "[album-rom] pause: pairs={} toggles={} ignored={} limit={} [{}]",
                self.pairs,
                self.toggles,
                self.ignored,
                PAUSE_LOG,
                if self.ok() { "PASS" } else { "FAIL" }
            );
            for (index, record) in self.records[..self.recorded].iter().enumerate() {
                agb::println!(
                    "[album-rom] pause {}: action@frame={} frozen@frame={} fade_periods={} gain={} committed={} ; resume@frame={} committed={} dropped={} [{}]",
                    index + 1,
                    record.action_frame,
                    record.frozen_frame,
                    record.fade_periods,
                    record.frozen_gain,
                    record.frozen_committed,
                    record.resumed_frame,
                    record.resumed_committed,
                    Self::dropped(record),
                    if record.frozen_gain == 0 && Self::dropped(record) == 0 {
                        "PASS"
                    } else {
                        "FAIL"
                    }
                );
            }
            if !self.ok() {
                agb::println!(
                    "[album-rom] FAIL a pause did not hold: every pair must freeze at gain 0, resume on the sample it stopped on (dropped=0), and account for its toggles"
                );
            }
        }
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
        /// The track being decoded: this *visit*'s track (PR-3b-ii-B2b's traversal). A visit's
        /// counters are reset by [`Producer::open_track`], so `frames == total_frames` still means
        /// "this track is complete" and `pushed == total_samples` still means "the ring has every
        /// sample of it" — both were true only by accident of a single-track run before.
        track: usize,
        /// Samples per channel this visit's track must deliver, from the bake's pin.
        total_samples: usize,
        /// Frames this visit's track carries, from the bake's pin.
        total_frames: usize,
        frames: usize,
        /// Samples of this visit's track pushed into the ring so far — the per-track term the
        /// ledger's arithmetic is stated on, and the reason `finished` can be per visit.
        pushed: usize,
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
        /// A producer whose first visit is `track`: every counter is that track's pin, so the boot
        /// path below needs no separate seeding.
        fn new(source: backend::Source, track: usize) -> Self {
            let pin = &assets::TRACKS[track];
            Self {
                source,
                interleaved: vec![0i16; 2 * FRAME_SAMPLES],
                left: vec![0i32; FRAME_SAMPLES],
                right: vec![0i32; FRAME_SAMPLES],
                bytes_l: vec![0u8; FRAME_SAMPLES],
                bytes_r: vec![0u8; FRAME_SAMPLES],
                track,
                total_samples: usize::try_from(pin.samples).unwrap_or(usize::MAX),
                total_frames: pin.frames as usize,
                frames: 0,
                pushed: 0,
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
                let start = offset;
                let end = (start + CHUNK).min(n);
                let outcome = critical_section::with(|cs| {
                    let mut feed = FEED.borrow_ref_mut(cs);
                    let outcome =
                        feed.push_bytes(&self.bytes_l[offset..end], &self.bytes_r[offset..end]);
                    let complete =
                        outcome.is_ok() && self.pushed + (end - start) == self.total_samples;
                    drop(feed);
                    let mut stats = STATS.borrow_ref_mut(cs);
                    // The per-visit counter and the ring-wide one move together, in the push's own
                    // critical section, so the ledger cannot see a half-counted push.
                    if outcome.is_ok() {
                        stats.content_pushed =
                            stats.content_pushed.saturating_add((end - offset) as u32);
                    }
                    // The `finished` flag is set in the *same* critical section as the final push
                    // (`product_play_main.rs:1272-1281`): setting it after the producer loop leaves
                    // a window in which the feeder sees an empty ring while the flag is still
                    // false and counts the final short chunk as an underflow. It is **this visit's**
                    // last push, not the album's: a skip clears it again at its hand-off, so the
                    // new track's first starved period is an underflow and not a tail.
                    if complete {
                        stats.finished = true;
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
                self.pushed += end - start;
            }
            self.frames += 1;
            Ok(n)
        }

        /// Move to another track — §4.4 step 3's seam call, and this visit's counter reset.
        ///
        /// The per-backend repositioning stays the adapter's (`PcmSource::start_track`: the FLAC arm
        /// rebuilds its decoder from the container, the stub arm resets its cursor), so a skip costs
        /// a container parse here and nothing else. `finished` is the caller's to clear, because it
        /// must happen inside the hand-off's critical section rather than before it.
        fn open_track(&mut self, index: usize) -> Result<(), SourceError> {
            self.source.start_track(index)?;
            let pin = &assets::TRACKS[index];
            self.track = index;
            self.total_samples = usize::try_from(pin.samples).unwrap_or(usize::MAX);
            self.total_frames = pin.frames as usize;
            self.frames = 0;
            self.pushed = 0;
            Ok(())
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
        ///
        /// **Per track, once a run can skip (PR-3b-ii-B2b).** A skip makes the audio pass cover more
        /// than one track's bytes, and a single accumulator over two tracks would match neither
        /// track's pin — the strongest witness in this image would fail while the bytes were
        /// perfect. So the passes are per track: the accumulators are reset at each `start_track`,
        /// each visited track's fold is compared against **that track's own** `assets::TRACKS[i]`
        /// pins, and one line is printed per visited track. That is also the only shape that can
        /// tell track 1's bytes from track 3's when two tracks share one packed blob (Q15/D8).
        ///
        /// **What it can and cannot afford.** A whole-track fold costs ~3.39 M cycles per frame
        /// (measured: 812,370,488 c for the gate track's 240 frames). This fixture's other two
        /// entries point at the 4.3-minute master's blob — 2,069 frames, ~8.6x the gate track — so
        /// its cold fold is ~7 G cycles (~419 s of CPU), which does not fit the gate's 600 s leash
        /// twice over. Those tracks are therefore named `SKIPPED` with their arithmetic, **never**
        /// reported as `MATCH`: their content is witnessed by the boot-time prefix/suffix tripwire
        /// and by the whole-region asset fold, and the log says exactly which leg is missing.
        #[cfg(feature = "deep-audit")]
        fn proof(&mut self, clock: &mut CycleCounter<'_>, visits: &[usize]) -> Proof {
            let total_start = clock.read();
            let mut folded = 0u32;
            let mut skipped = 0u32;
            let mut pcm_ok = true;
            let mut fifo_ok = true;
            // One fold per packed blob: two tracks pointing at the same `(offset, len)` are the same
            // bytes, and re-folding them would buy nothing (§15 Q15's aliased pair).
            let mut done: Vec<(u32, u32)> = Vec::new();
            let mut folded_frames = 0u32;
            self.pcm = Fnv1a64::new();
            self.fifo_l = Fnv1a64::new();
            self.fifo_r = Fnv1a64::new();

            for &track in visits {
                let pin = &assets::TRACKS[track];
                let range = (pin.asset_off, pin.asset_len);
                if done.contains(&range) {
                    agb::println!(
                        "[album-rom] proof track {track}: same packed blob as a track already folded (off={} len={} sha256={}) — one fold covers both",
                        pin.asset_off,
                        pin.asset_len,
                        pin.blob_sha256
                    );
                    continue;
                }
                done.push(range);
                if pin.frames > PROOF_FRAME_CAP || folded_frames + pin.frames > PROOF_FRAMES_TOTAL {
                    skipped += 1;
                    agb::println!(
                        "[album-rom] proof track {track}: SKIPPED — {} frames is past the audited fold's cap ({} per track, {} a pass; the gate track's {} frames are measured at 3.39 M c/frame), so its whole-track fold would not fit this run's leash; its content is covered by the boot tripwire (prefix 0x{:08X} suffix 0x{:08X}) and by the asset-region fold",
                        pin.frames,
                        PROOF_FRAME_CAP,
                        PROOF_FRAMES_TOTAL,
                        pin.frames,
                        pin.prefix_fnv,
                        pin.suffix_fnv
                    );
                    continue;
                }
                folded_frames += pin.frames;

                // A cold restart of *this* track (the FLAC arm rebuilds its decoder from the
                // container, so the pull starts at frame 0 again) and a fresh accumulator: the
                // reset is the whole point of doing this per track.
                if let Err(error) = self.source.start_track(track) {
                    agb::println!(
                        "[album-rom] FAIL proof: track {track} would not restart: {error}"
                    );
                    return Proof {
                        pcm_ok: false,
                        fifo_ok: false,
                        folded,
                        skipped,
                    };
                }
                self.pcm = Fnv1a64::new();
                self.fifo_l = Fnv1a64::new();
                self.fifo_r = Fnv1a64::new();

                let mut frames = 0usize;
                let mut samples = 0usize;
                let mut redecode = 0u32;
                let mut fold = 0u32;
                loop {
                    let pull_start = clock.read();
                    let n = match self.next_frame() {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => {
                            return Proof {
                                pcm_ok: false,
                                fifo_ok: false,
                                folded,
                                skipped,
                            };
                        }
                    };
                    let pull_end = clock.read();
                    redecode = redecode.wrapping_add(pull_end.wrapping_sub(pull_start));
                    let fold_start = clock.read();
                    if fold_i16le_stereo(&mut self.pcm, &self.left[..n], &self.right[..n]).is_err()
                    {
                        agb::println!(
                            "[album-rom] FAIL proof: a sample is outside the int16 profile at track {track} frame {frames}"
                        );
                        return Proof {
                            pcm_ok: false,
                            fifo_ok: false,
                            folded,
                            skipped,
                        };
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
                let cycle = clock.read().wrapping_sub(total_start);
                agb::println!(
                    "[album-rom] proof track {track}: redecode={redecode}c fold={fold}c cumulative={cycle}c"
                );

                let pcm = self.pcm.finish();
                let this_pcm_ok = frames == pin.frames as usize
                    && samples == pin.samples as usize
                    && pcm == pin.fnv_pcm;
                agb::println!(
                    "[album-rom] proof track {track}: frames={} samples={} pcm_fnv=0x{:016X} expected=0x{:016X} [{}]",
                    frames,
                    samples,
                    pcm,
                    pin.fnv_pcm,
                    if this_pcm_ok { "MATCH" } else { "MISMATCH" }
                );
                let fifo_l = self.fifo_l.finish();
                let fifo_r = self.fifo_r.finish();
                let this_fifo_ok = fifo_l == pin.fnv_fifo_l && fifo_r == pin.fnv_fifo_r;
                agb::println!(
                    "[album-rom] proof track {track}: fifo L=0x{:016X} R=0x{:016X} expected L=0x{:016X} R=0x{:016X} [{}]",
                    fifo_l,
                    fifo_r,
                    pin.fnv_fifo_l,
                    pin.fnv_fifo_r,
                    if this_fifo_ok { "MATCH" } else { "MISMATCH" }
                );
                // And the proof leaves the decoder where the run left it — on the last visited
                // track — so nothing after this pass can read a stale "current" track.
                pcm_ok &= this_pcm_ok;
                fifo_ok &= this_fifo_ok;
                folded += 1;
            }
            let total = clock.read().wrapping_sub(total_start);
            agb::println!(
                "[album-rom] proof: tracks folded={folded} skipped={skipped} frames={folded_frames} total={total}c"
            );
            Proof {
                // A pass that folded nothing proves nothing on content, so it is a failure rather
                // than a pass with a quiet weaker term.
                pcm_ok: pcm_ok && folded > 0,
                fifo_ok: fifo_ok && folded > 0,
                folded,
                skipped,
            }
        }
    }

    /// One run of [`Producer::proof`]: what it proved, and what it cost.
    ///
    /// `folded == 0` is a failure of the content terms rather than a pass — see the proof's docs.
    #[cfg(feature = "deep-audit")]
    struct Proof {
        pcm_ok: bool,
        fifo_ok: bool,
        folded: u32,
        skipped: u32,
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

        // ---- the video clock and the input seam (PR-3b-i, PR-3b-ii-B1, §4.5). Registered here,
        // before the boot clock starts, so the boot ledger's `parse=`/`open=`/... spans stay
        // comparable with every earlier run's. The handler allocates (agb boxes the closure), which
        // is why registration is not on any audio deadline — it is boot work, like the Timer1
        // registration further down. The handler itself does nothing until the seam is installed
        // ([`INPUT`]), which happens with the chain.
        //
        // SAFETY: the handler allocates nothing, logs nothing and touches no register beyond the
        // keypad read and the two timer reads — agb's interrupt-handler contract.
        let _vblank = unsafe { add_interrupt_handler(Interrupt::VBlank, vblank) };
        #[cfg(not(feature = "input-replay"))]
        let input_source = keys::Hardware::new();
        #[cfg(feature = "input-replay")]
        let input_source = keys::Replay::new(REPLAY_SCRIPT);
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
        agb::println!(
            "[album-rom] start_track={} (this gate starts at track 1; the product plays the record from its first track)",
            START_TRACK
        );
        // §14 R5's gap, stated rather than implied: R5 promises a named `DATA ERROR`, a skip forward
        // and a stop at the end when a mid-album decode fails. PR-3b builds none of that — a decode
        // failure aborts `FAILED` — so the omission is printed in every run's log and cannot be read
        // as an implemented path. Building it is a later PR's work; this line is the whole of it here.
        agb::println!(
            "[album-rom] error path: not implemented in PR-3b; a mid-album decode failure aborts FAILED (R5 open)"
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
        let track: Track<'static> = album.track(START_TRACK as u32);
        let pin = &assets::TRACKS[START_TRACK];
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
            START_TRACK,
            track.title,
            pin.frames,
            pin.n_samples,
            pin.asset_len
        );

        // The FLAC boot gate for the gate track (§D-C rule 3): profile, stereo, rate identity and
        // the container's pins, all inside `start_track`.
        if let Err(error) = source.start_track(START_TRACK) {
            refuse(
                &mut gfx,
                &format!("track {START_TRACK} failed its boot gate: {error}"),
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

        let mut producer = Producer::new(source, START_TRACK);

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
        // is written, and the first boundary is then a re-arm like any other. A resume runs this
        // same function: the ring's head is the sample the freeze stopped on, which is why no seek
        // is needed.
        if !prime_hand_off() {
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
        // ---- the seam goes live with the chain: the handler samples the keypad once per vblank and
        // measures itself from `REG_TM1_DATA` only from here, so the scripted frames of the REPLAY
        // arm are video frames of the audio window and the cost is read from a free-running audio
        // clock rather than from the boot counter.
        critical_section::with(|cs| {
            *INPUT.borrow_ref_mut(cs) = Some(Input::new(input_source));
        });

        // ---- §4.5's frame loop. No logging inside: one `agb::println!` is a semihosting trap,
        // which on hardware is a syscall into a host that may not exist.
        //
        // Two clocks meet here and this loop is the whole of their arbitration. The producer is the
        // thief of leftover time: it runs only while the ring has room for another whole frame, it
        // stops the moment a video frame is due, and it does not run at all while the transport is
        // mid-transition — a pause that is being applied must not be fed another frame. A latched
        // frame is then served before any more decode — the action the vblank handler posted, and
        // one position read — which is everything §4.5 gives the video clock today. The key *sample*
        // is the handler's (PR-3b-ii-B1), so it keeps happening while the decoder is inside a
        // producer frame; this loop only reads what it left. The audio path is untouched by the
        // restructure: the period IRQ takes a chunk, applies the hand-off gain, re-arms the DMA and
        // counts, and the producer's flow control is the same predicate it always was, asked once
        // per pass.
        let mut paint = Paint::new();
        paint.start(producer.total_samples as u32, 0);
        let mut visits = Visits::new();
        visits.begin(START_TRACK, 0);
        let mut skips = Skips::new();
        let mut advances = Advances::new();
        let mut pauses = Pauses::new();
        // A transition whose fade is in flight: the track its hand-off must go to. Kept apart from
        // the transport because the target is the caller's arithmetic (§5.3's wrap for a press,
        // `i + 1` for a track's end), not the machine's.
        let mut pending: Option<usize> = None;
        // Whether that transition is a track's end rather than a press (PR-3b-iii-A). It is the
        // one difference at the hand-off: there is no truncate, because the ring was drained when
        // the transition started. It has a row of its own all the same (PR-3b-iii-B): the
        // `/advance:` ledger reads the same wire watch a skip's does.
        let mut pending_auto = false;
        // The album reached its last track's end: the loop stops there instead of wrapping (§1).
        let mut album_end = false;
        // The period the loop took that post on — the start of the one wait this app owns. See
        // `SkipRecord::take_to_handoff`.
        let mut pending_at: u32 = 0;
        let n_tracks = album.n_tracks() as usize;
        let mut halt: Option<&'static str> = None;
        // Passes in a row with a full ring and no frame due — the wedge bound ([`WEDGE_PASSES`]).
        let mut idle: u32 = 0;
        'frames: loop {
            // Where this pass starts on the video clock. The reaction gate's bound is the widest pass
            // the run shows ([`Input::pass_max`]): a take happens at the top of a pass and drains a
            // slot filled during the pass before, so no take can wait longer than one pass. Only the
            // REPLAY arm measures it — it is the arm with a scripted press to time.
            #[cfg(feature = "input-replay")]
            let pass_start = frame_latch();
            // The producer runs while the transport is a feeding state — `Playing`, or `Repriming`,
            // where the new track's audio has to reach the ring behind the skip's silence cover.
            if transport_state().feeds() {
                while !paint.due() && producer.frames < producer.total_frames && producer.has_room()
                {
                    idle = 0;
                    match producer.push_one() {
                        Ok(_) => {}
                        Err(Halt::Ended) => {
                            agb::println!(
                                "[album-rom] FAIL the stream ended at frame {} of {}",
                                producer.frames,
                                producer.total_frames
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
            }
            // The frame slice: the vblank handler detected the edge, stamped it with the frame it
            // landed on and posted it. Every action §5.3 maps acts here and only here, on the next
            // period boundary, because changing what the DMA is mid-buffer is the splice §4.4
            // forbids: Start/A is the pause (PR-3b-ii-B2a), Right/R, Left/L and Down are the skip,
            // the skip and the restart (ii-B2b).
            let detected = take_action();
            if detected.is_some() {
                idle = 0;
            }
            if let Some(detected) = detected
                && detected.action != keys::Action::PauseToggle
            {
                // A skip's first step only: the transport issues the ramp-out and the track's
                // arithmetic is kept here. A command the state machine will not take (paused, or a
                // transition already in flight) is counted rather than dropped in silence.
                match critical_section::with(|cs| TRANSPORT.borrow_ref_mut(cs).skip()) {
                    Step::FadeOut => {
                        pending = Some(neighbour(producer.track, detected.action, n_tracks));
                        pending_auto = false;
                        pending_at = handler_entries();
                    }
                    Step::Ignored => skips.ignored = skips.ignored.saturating_add(1),
                    _ => {}
                }
            }
            if let Some(detected) = detected
                && detected.action == keys::Action::PauseToggle
            {
                match critical_section::with(|cs| TRANSPORT.borrow_ref_mut(cs).toggle(true)) {
                    Step::FadeOut => pauses.begin(frame_latch(), handler_entries()),
                    Step::Prime => {
                        // §4.4's resume, in the boot path's order: the ring's head is the sample the
                        // freeze stopped on, so this is a prime and not a seek. The transport has
                        // already set the gain ramping in, and the primed chunk is the ramp's first.
                        //
                        // The position is read **before** the prime: the prime commits a chunk to
                        // the DMA, and the pause's claim is about the sample the resume started
                        // from, not the one after its first buffer.
                        let at_resume = position::committed_samples();
                        //
                        // One critical section, for the reason `freeze_chain` gives: no vblank
                        // bracket may straddle the timer reset. `prime_hand_off` takes its own
                        // (nested) section inside this one, which agb's implementation allows.
                        let armed = critical_section::with(|_cs| {
                            if !prime_hand_off() {
                                return false;
                            }
                            enable_sound();
                            clear_timer1_if();
                            arm_audio_chain(&mut timers, &CHAIN);
                            true
                        });
                        if !armed {
                            agb::println!(
                                "[album-rom] FAIL the resume had nothing to prime — the ring was empty"
                            );
                            halt = Some("resume");
                            break 'frames;
                        }
                        pauses.resumed(frame_latch(), at_resume);
                    }
                    Step::Ignored => pauses.ignored(),
                    _ => {}
                }
            }
            if paint.due() {
                paint.sample();
                idle = 0;
            }
            // A skip's or an advance's measurement closes as soon as the wire has carried the new
            // track: the watch is read and disarmed here so the scan stops costing the feeder its
            // pass over the slots.
            if skips.open() {
                let watch = watch_state();
                if skips.resolve(&watch) {
                    watch_disarm();
                }
            } else if advances.open() {
                let watch = watch_state();
                if advances.resolve(&watch) {
                    watch_disarm();
                }
            }
            // One main-loop pass's transport decision, independent of commands: has the fade
            // finished (freeze the chain), is a skip ready to hand off, or has the ramp back returned
            // (Playing again)?
            match critical_section::with(|cs| TRANSPORT.borrow_ref_mut(cs).tick()) {
                Step::Freeze => {
                    freeze_chain(&mut timers);
                    pauses.frozen(
                        frame_latch(),
                        position::committed_samples(),
                        handoff_gain(),
                        handler_entries(),
                    );
                }
                Step::SkipHandOff => {
                    // §4.4's steps 2-6, in one main-loop pass: the hand-off's critical section, the
                    // rest of the cover, the seam call, and the bookkeeping that keeps both ledgers
                    // and the painted position per track.
                    let Some(next) = pending.take() else {
                        agb::println!(
                            "[album-rom] FAIL a transition reached its hand-off with no target track"
                        );
                        halt = Some("skip target");
                        break 'frames;
                    };
                    let auto = pending_auto;
                    pending_auto = false;
                    let hand_off = match transition_hand_off(!auto) {
                        Ok(hand_off) => hand_off,
                        Err(why) => {
                            agb::println!("[album-rom] FAIL the hand-off: {why}");
                            halt = Some("skip hand-off");
                            break 'frames;
                        }
                    };
                    // Step 4: the rest of the cover, one whole chunk per push. Every push is whole
                    // for `take_chunk`'s reason (`push_cover_chunk`).
                    let mut covered = 1;
                    while covered < SKIP_PRIME_CHUNKS {
                        if !push_cover_chunk() {
                            agb::println!(
                                "[album-rom] FAIL the skip's cover was refused after {covered} chunks"
                            );
                            halt = Some("skip cover");
                            break 'frames;
                        }
                        covered += 1;
                    }
                    let from = producer.track;
                    visits.close(producer.pushed as u32, hand_off.committed);
                    if !auto {
                        let watch = watch_state();
                        skips.begin(from, next, pending_at, &hand_off, &watch);
                    } else {
                        let watch = watch_state();
                        advances.begin(from, next, &hand_off, &watch);
                    }
                    // Step 3: the seam call. Its cost is a container parse plus the first frame's
                    // decode, and it is spent behind `SKIP_PRIME_CHUNKS` periods of silence cover.
                    if let Err(error) = producer.open_track(next) {
                        agb::println!(
                            "[album-rom] FAIL the skip to track {next} could not start it: {error}"
                        );
                        halt = Some("skip seam");
                        break 'frames;
                    }
                    // The cover is committed to the session clock (it is handed to the DMA) but it is
                    // not part of any track, so the visit's timeline starts *after* it: that is what
                    // makes §4.4's "0 at a track start" true again with a skip in the run.
                    let base = hand_off
                        .committed
                        .saturating_add(SKIP_PRIME_CHUNKS * CHUNK as u32);
                    visits.begin(next, base);
                    paint.start(assets::TRACKS[next].n_samples as u32, base);
                    idle = 0;
                }
                _ => {}
            }
            if transport_state() != State::Playing {
                idle = 0;
            }
            // A visit's producer finishing is not the album's end (PR-3b-iii-A): its last samples
            // are still in the ring, and the feeder has to hand them over before the listener has
            // heard the track. So wait for the drain here, then either post the advance through the
            // same transport entry a press uses or stop at the album's last track.
            if producer.frames >= producer.total_frames && transport_state() == State::Playing {
                if !visit_drained() {
                    // Draining is the feeder's job, not the wedge the bound above is for: the ring
                    // is *emptying*, and no frame is due only because this visit is over.
                    idle = 0;
                } else if producer.track + 1 == n_tracks {
                    // §1 plays the record start to finish, so there is no wrap here: §5.3's wrap is
                    // the buttons' rule, not the album's.
                    album_end = true;
                    break;
                } else {
                    // §12.2's admission rule: the ring has just drained, so it has the whole
                    // transition's headroom. The fade is the press path's — the one omission is the
                    // truncate, which the hand-off makes because there is nothing stale left. The
                    // starved baseline is latched here, before the fade spends its period, so the
                    // advance's row counts every period the boundary carried nothing.
                    note_advance_start();
                    match critical_section::with(|cs| TRANSPORT.borrow_ref_mut(cs).skip()) {
                        Step::FadeOut => {
                            pending = Some(producer.track + 1);
                            pending_auto = true;
                        }
                        _ => {
                            agb::println!(
                                "[album-rom] FAIL a track end could not start its transition"
                            );
                            halt = Some("track end");
                            break 'frames;
                        }
                    }
                    idle = 0;
                }
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
            #[cfg(feature = "input-replay")]
            note_pass(frame_latch().wrapping_sub(pass_start));
        }
        if halt.is_none() && producer.frames != producer.total_frames {
            agb::println!(
                "[album-rom] FAIL frame count {} does not match the pin's {}",
                producer.frames,
                producer.total_frames
            );
            halt = Some("frame count");
        }
        // Take the last skip's (or advance's) reading before the drain: the wire watch resolves on
        // the new track's first chunk, long before the ring empties, so this is not the drain's
        // business.
        if skips.open() {
            let watch = watch_state();
            skips.resolve(&watch);
            watch_disarm();
        }
        if advances.open() {
            let watch = watch_state();
            advances.resolve(&watch);
            watch_disarm();
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
            // served: the same pair as the loop above, on the same latch comparison. The posted
            // action is taken here too, so an edge that arrived after the producer finished cannot
            // leave the slot occupied when the seam is removed.
            let _ = take_action();
            if paint.due() {
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
                        starved_periods: stats.starved_periods,
                        primes: stats.primes,
                        hw_periods: stats
                            .periods_before
                            .saturating_add(u32::from(stats.last_period)),
                        coalesced: stats.coalesced,
                        late_servicings: stats.late_servicings,
                        max_delay_samples: stats.max_delay_samples,
                        dropped_samples: stats.dropped_samples,
                        cover_samples: stats.cover_samples,
                        content_pushed: stats.content_pushed,
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
        // Close the open visit **after** the drain: its `end` is the session clock when its audio
        // finished reaching the DMA, and the hand-off ledger below reads each visit's consumed span
        // as `end - base`. Closing it when the producer stopped (as a single-track run could) would
        // leave the last visit's queued span out of its own row.
        visits.close(producer.pushed as u32, position::committed_samples());

        // ---- the seam comes out with the chain, before the timer stop: the handler must not sample
        // into a stopped audio clock, and the counts printed below have to be the run's. The local
        // owns them from here.
        let Some(input) = critical_section::with(|cs| INPUT.borrow_ref_mut(cs).take()) else {
            refuse(&mut gfx, "the input seam was never installed");
        };

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
            starved_periods: starved,
            primes,
            hw_periods,
            coalesced,
            late_servicings: late,
            max_delay_samples: late_max,
            dropped_samples: dropped,
            cover_samples: cover,
            content_pushed: content,
        } = observed;
        // The per-track ledger, one row per visit: what the run actually did with each track. This is
        // what makes "the album played track by track" visible per track rather than as one total
        // (PR-3b-ii-B2b), and its `pushed` column is the figure the content proof and the run's
        // arithmetic below are stated on.
        let visits_pushed = visits.pushed_total();
        for (index, visit) in visits.list().iter().enumerate() {
            let pin = &assets::TRACKS[visit.track];
            agb::println!(
                "[album-rom] feed: visit={} track={} \"{}\" pushed={} of {} samples base={} end={} [{}]",
                index + 1,
                visit.track,
                album.track(visit.track as u32).title,
                visit.pushed,
                pin.n_samples,
                visit.base,
                visit.end,
                if visit.pushed <= pin.n_samples {
                    "ok"
                } else {
                    "OVER-PUSHED"
                }
            );
        }
        // The ring's arithmetic, in the terms a skip adds. Every pushed sample is either track content
        // or the skip's silence cover, so those two must add up to the producer's frontier exactly;
        // and after the final drain every pushed sample has been taken except the ones a truncate
        // stepped over, so `consumed == pushed` and `taken == pushed - dropped`. The per-track sum is
        // checked against the ring-wide counter so the two bookkeepings cannot part.
        let visits_ok = visits.overflow == 0 && visits_pushed == content;
        agb::println!(
            "[album-rom] feed: pushed={} content={} cover={} consumed={} dropped={} taken={} frames={}/{} underflows={} handler_entries={} tails={} primes={} visits={} [{}]",
            pushed,
            content,
            cover,
            consumed,
            dropped,
            pushed.saturating_sub(dropped as usize),
            producer.frames,
            producer.total_frames,
            underflows,
            handler_entries,
            tails,
            primes,
            visits.len,
            if visits_ok { "MATCH" } else { "MISMATCH" }
        );
        if !visits_ok {
            agb::println!(
                "[album-rom] FAIL the per-track ledger disagrees with the ring's: visits sum {} vs content {} (a visit was dropped from the log its own proof reads)",
                visits_pushed,
                content
            );
        }
        // D5's one-line statement of the traversal: how many visits, which track it stopped on, and
        // whether that was the album's last. `NOT_REACHED` is the honest reading of a run that
        // stopped for a halt or a frame-count mismatch, so a short run cannot read as an album end.
        let last_track = visits.list().last().map(|visit| visit.track).unwrap_or(0);
        agb::println!(
            "[album-rom] traversal: visits={} last_track={} end={}",
            visits.len,
            last_track,
            if album_end {
                "ALBUM_END"
            } else {
                "NOT_REACHED"
            }
        );
        // The hand-off ledger, restated for a run that visits tracks, can skip, and advances at a
        // track's end: a chunk reaches the DMA either through the feeder (a handler entry) or
        // through the main loop's prime (the boot one, and one per resume). Per visit, the content
        // that actually reached the DMA is `end - base` — both are session-clock readings, and the
        // cover between two visits is committed but belongs to neither — and that span is handed
        // over in whole chunks plus at most one classified tail, so it needs `ceil(span / chunk)`
        // hand-offs. Add the cover's own whole chunks and the periods that carried nothing (a
        // track end's fade is starved with `finished` set: not an underflow, but a hand-off the DMA
        // was re-armed on). `handler_entries` alone would look short by one per resume and long by
        // one per truncated queue, which is the shape of a dishonest comparison.
        //
        // One divide per visit, computed once here and never on the audio path.
        let mut expected_hand_offs = cover as usize / CHUNK;
        for visit in visits.list() {
            let consumed = visit.end.saturating_sub(visit.base) as usize;
            expected_hand_offs += (consumed + CHUNK - 1) / CHUNK;
        }
        expected_hand_offs += starved as usize;
        let hand_offs = handler_entries as usize + primes as usize;
        let hand_offs_ok = hand_offs == expected_hand_offs;
        agb::println!(
            "[album-rom] handoffs: handler_entries+primes={} expected={} [{}]",
            hand_offs,
            expected_hand_offs,
            if hand_offs_ok { "MATCH" } else { "MISMATCH" }
        );
        if !hand_offs_ok {
            agb::println!(
                "[album-rom] FAIL the hand-off ledger is short: {} chunks reached the DMA, {} were expected",
                hand_offs,
                expected_hand_offs
            );
        }
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
        // ---- the input seam's end-of-run lines (PR-3b-i, extended by PR-3b-ii-B1). The counts are
        // what a gate reads: a REPLAY build's script has a known number of edges and repeats, and a
        // HARDWARE build's `presses=0` is the honest reading of a headless run rather than a failure.
        // Which source it was is in the line, always.
        #[cfg(not(feature = "input-replay"))]
        input.report("HARDWARE (REG_KEYINPUT) — no physical press was observed by this run");
        #[cfg(feature = "input-replay")]
        input.report(&format!(
            "REPLAY ({} entries, {} edges delivered) — scripted, not a physical press",
            input.source.entries(),
            input.source.edges()
        ));

        // ---- §4.4's pause ledger (PR-3b-ii-B2a): the pairs, and the one line each that says the
        // position did not move. Printed here, after the teardown, for the reason the key edges are:
        // `agb::println!` is a semihosting trap and the record is filled on the audio path.
        pauses.report();

        // ---- §4.4's skip ledger (PR-3b-ii-B2b): one row per skip, in the probe's own units, so these
        // lines can be read against its table ("The deep arm": `to_new = prime + 1`, and 5 periods at
        // prime 4 is 171 ms of silence). Printed after the teardown for the same reason every other
        // record is: `agb::println!` is a semihosting trap and the rows are filled on the audio path.
        skips.report();

        // ---- the listener's whole wait, term by term and then summed. The reaction line above prints
        // the audio clock's phase at detection and the ISR seam's two frame legs; the skip rows print
        // the two period legs. None of them is the press→audible number on its own, and reading any
        // one of them as it is what made the earlier "≤ 3 video frames" claim: the physical floor is
        // the debounce frame, plus one producer frame of decode the take cannot interrupt, plus the
        // period boundary the transition cannot straddle, plus the cover. This line is the sum of the
        // worst of each term, not the worst of the sum — it is an upper bound, and it is what a
        // listener's worst press actually costs.
        #[cfg(feature = "input-replay")]
        {
            let (worst_hand_off, worst_audible) = skips.worst_legs();
            let seam_frames = input.detect_max + input.take_max;
            // One u64 divide in the report path, after the teardown, never on a measured path.
            let cycles = u64::from(seam_frames) * u64::from(VIDEO_PERIOD_CYCLES)
                + u64::from(worst_hand_off + worst_audible) * u64::from(PERIOD_CYCLES);
            agb::println!(
                "[album-rom] latency: press→detect={}f + detect→take={}f + take→hand-off={}p + hand-off→audible={}p (cover) = press→audible {} frames = {} ms (worst per term; gated by reaction + skip rows; +the new track's own head_silence if it opens silent)",
                input.detect_max,
                input.take_max,
                worst_hand_off,
                worst_audible,
                cycles / u64::from(VIDEO_PERIOD_CYCLES),
                cycles * 1_000 / 16_777_216
            );
        }

        // ---- the track boundaries (PR-3b-iii-B): one row per auto-advance, read off the same wire
        // watch a skip's row is, with the boundary's audible gap in periods, samples and ms. This is
        // the derived form of Q4's ruling — the lossless path has inter-track gaps and says so.
        advances.report();

        // ---- the session clock (§10.4), printed where the ledger it mirrors was printed. The
        // equality is structural — the clock is incremented by exactly what `take_chunk` returned —
        // so a MISMATCH means the increment and the ledger have come apart, which is why it is part
        // of the verdict rather than a remark.
        let committed = position::committed_samples();
        let last = visits.list().last().copied();
        let (painted, last_total) = match last {
            Some(visit) => {
                let total = assets::TRACKS[visit.track].n_samples;
                (
                    position::paint_position(committed.saturating_sub(visit.base), total),
                    total,
                )
            }
            None => (0, 0),
        };
        // The arithmetic the session clock owes once a skip can truncate: the clock is incremented by
        // exactly what `take_chunk` returned, `consumed` by that *plus* what a truncate stepped over,
        // so `consumed == committed + dropped` is the identity — and `taken == committed` says the
        // clock saw every sample the DMA was given and nothing else.
        let position_ok = committed.saturating_add(dropped) == consumed as u32
            && painted == last_total
            && paint.frames <= input.sampled()
            && paint.regressions == 0;
        agb::println!(
            "[album-rom] position: committed={} dropped={} ledger={} lead={} paint={}/{} of track {} frames={} of {} regressions={} [{}]",
            committed,
            dropped,
            consumed,
            position::LEAD_SAMPLES,
            painted,
            last_total,
            last.map(|visit| visit.track).unwrap_or(0),
            paint.frames,
            input.sampled(),
            paint.regressions,
            if position_ok { "MATCH" } else { "MISMATCH" }
        );
        if !position_ok {
            agb::println!(
                "[album-rom] FAIL the session clock, the ring ledger and the per-frame position reads disagree — one increment per committed period and no regression in what was painted is the contract"
            );
        }

        // At most one tail period per visit: a skip truncates the queued span instead of draining
        // it, and a track end drains it, so each visit can leave at most its own residue. The latter
        // is why the bound is the visit count and not 1 any more.
        let last_played =
            last.is_some_and(|visit| visit.pushed == assets::TRACKS[visit.track].n_samples);
        let ledger_ok = pushed == content as usize + cover as usize
            && consumed == pushed
            && producer.frames == producer.total_frames
            && last_played
            && underflows == 0
            && tails as usize <= visits.len
            && halt.is_none()
            && pushed != 0
            && position_ok
            && visits_ok;
        // The audit is part of the verdict, not supporting evidence (`product_play_main.rs:1865-1894`):
        // a coalesced overflow is a full-buffer replay, a late service is a mid-waveform splice, and
        // both are exactly what `product-play.gba` refuses — so this image's PASS cannot be weaker
        // than that one's.
        let audit_ok = coalesced == 0 && late == 0;
        // The frame loop's own term: every sample serves exactly one video frame, and the worst gap
        // is one frame — the sample is the vblank handler's (PR-3b-ii-B1). Its numbers are printed by
        // `Input::report` above, beside this gate's limit.
        let cadence_ok = input.cadence_ok();
        // The two terms this step is accepted or rejected on: the handler's measured cost against
        // the sound FIFO's re-arm grace, and — on the REPLAY arm, the only one where an edge's
        // scripted frame is known — the reaction's two latencies. Both are printed by
        // `Input::report` with their limits.
        let handler_ok = input.handler_ok();
        #[cfg(feature = "input-replay")]
        let reaction_ok: Option<bool> = Some(input.reaction_ok());
        #[cfg(not(feature = "input-replay"))]
        let reaction_ok: Option<bool> = None;
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
        let (pcm_ok, fifo_ok, proof_tracks) = {
            let mut proof_clock = CycleCounter::start(&mut timers);
            let visits: Vec<usize> = visits.list().iter().map(|visit| visit.track).collect();
            let proof = producer.proof(&mut proof_clock, &visits);
            proof_clock.stop();
            drop(proof_clock);
            (
                Some(proof.pcm_ok),
                Some(proof.fifo_ok),
                Some((proof.folded, proof.skipped)),
            )
        };
        #[cfg(not(feature = "deep-audit"))]
        let (pcm_ok, fifo_ok, _proof_tracks): (
            Option<bool>,
            Option<bool>,
            Option<(u32, u32)>,
        ) = (None, None, None);

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

        // A pause is part of the verdict on every arm, not a remark: every pair must have frozen at
        // gain 0 and resumed on the sample it stopped on, and the hand-off ledger must add up
        // (`handler_entries + primes`). On the HARDWARE arm `pairs=0` is the honest reading of a
        // headless run — nothing pressed a key — and the term is trivially true, exactly as
        // `presses=0` is.
        let pauses_ok = pauses.ok();
        // §4.4's skip, measured per skip rather than asserted: every skip must reach its new track in
        // `SKIP_PRIME_CHUNKS + 1` periods with no stale audio after it and no underflow booked, exactly
        // as `Skips::report` prints. On the shipped HARDWARE arm `skips=0` is the honest reading of a
        // headless run and the term is trivially true, the same way `presses=0` is.
        let skips_ok = skips.ok();
        // The track boundaries, checked per advance rather than assumed from `visits >= 2`: every
        // advance must reach the new track's first chunk at `SKIP_PRIME_CHUNKS + 1` periods with no
        // stale audio after the mark and no underflow booked, exactly as `Advances::report` prints.
        let advances_ok = advances.ok();
        let pass = ledger_ok
            && audit_ok
            && hand_offs_ok
            && pauses_ok
            && skips_ok
            && advances_ok
            && cadence_ok
            && handler_ok
            && term_ok(reaction_ok)
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
            "[album-rom] verdict {} — screen {} \"{}\" (ledger={}, audit={}, handoffs={}, pause={}, skip={}, advance={}, cadence={}, isr={}, reaction={}, pcm={}, fifo={}, assets={}, proof_tracks={}/{})",
            word,
            screen,
            word,
            ledger_ok,
            audit_ok,
            hand_offs_ok,
            pauses_ok,
            skips_ok,
            advances_ok,
            cadence_ok,
            handler_ok,
            term_name(reaction_ok),
            term_name(pcm_ok),
            term_name(fifo_ok),
            term_name(assets_ok),
            proof_tracks.map(|(folded, _)| folded).unwrap_or(0),
            proof_tracks
                .map(|(folded, skipped)| folded + skipped)
                .unwrap_or(0)
        );
        #[cfg(not(feature = "deep-audit"))]
        agb::println!(
            "[album-rom] verdict {} — screen {} \"{}\" (ledger={}, audit={}, handoffs={}, pause={}, skip={}, advance={}, cadence={}, isr={}, reaction={}, pcm={}, fifo={}, assets={} — built without deep-audit)",
            word,
            screen,
            word,
            ledger_ok,
            audit_ok,
            hand_offs_ok,
            pauses_ok,
            skips_ok,
            advances_ok,
            cadence_ok,
            handler_ok,
            term_name(reaction_ok),
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
