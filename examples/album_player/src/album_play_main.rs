//! PR-3a step (iii): the Album Player's playback ROM — `make album-rom`.
//!
//! It boots, verifies its own embedded album, plays **track 1 ("Bioluminescent Clip 1", 240 frames /
//! 491,520 samples) end to end** through the shipped feeder, and prints a `[album-rom]` ledger that
//! is the record of the run. Then it loops forever, so `timeout`'s exit 124 **is** the pass — the
//! same contract `product-play.gba` has.
//!
//! What this ROM is not: no keys, no pause, no skip, no session clock, no progress bar, no text,
//! no tiles, no sprites. Those are PR-3b/3c (§4.5). What it *does* show is the backdrop palette
//! entry: violet and solid while it is working, and then a verdict annunciated by **blink rate and
//! tone** — see "The verdict a colourblind human can perceive" below. That is a verdict lamp, not a
//! screen — a silent cart run has to be readable (§4.5's palette trap is the reason a frame with
//! nothing shown repaints the backdrop alone).
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
//! lamp           backdrop = violet, committed from a frame that shows nothing
//! verify (§D-C)  Album::parse → open()/verify_pins → the track's pins vs meta::Track → the FLAC
//!                boot gate (start_track), each refusal a named FAIL and a stop
//! prefill        16 chunks / 8,960 samples: pull_frame → widen → convert_frame → push_bytes,
//!                one CHUNK-sized critical section per slice
//! prime          hand the first chunk to the DMA and re-arm it before the chain exists
//! enable         enable_sound → clear_timer1_if → register the feeder → arm_audio_chain
//! produce        wait_for_room (a whole frame free) then one frame; until frames == total_frames
//! finish         `finished` is set in the same critical section as the final push
//! drain          poll until consumed() == pushed()
//! audit          every handler entry classifies the period: coalesced overflows, and lateness past
//!                geometry::REARM_GRACE_SAMPLES — both refuse the verdict
//! teardown       stop the chain, teardown_audio
//! proof          (feature `deep-audit`) the PCM and FIFO pins over a cold re-decode of the gate
//!                track — a decode witness; the ring ledger above (`pushed == consumed == expected`)
//!                is what witnesses the bytes the DMA actually carried
//! audit (assets) (feature `deep-audit`) the full asset-region fold, after the audio (ALBM1 v2)
//! verdict        backdrop = blue (PASS) or red (FAIL), then the verdict line
//! annunciator    blink black ↔ that colour (fast on FAIL), and a tone: 3 × 120 ms beeps on PASS,
//!                one 1 s low buzz on FAIL — the blink is white-on-black and the rate carries the
//!                verdict, so hue is never load-bearing and a colourblind human can read it
//! ```
//!
//! ## The verdict a colourblind human can perceive
//!
//! Fix 2 shipped a verdict lamp whose only signal was the backdrop's hue, and the person who runs
//! this cart is colourblind: they cannot see violet→blue, so the lamp let them verify nothing. A
//! signal the intended verifier cannot perceive is not a signal. The scheme is therefore **blink
//! rate + tone**, with a violet solid held while work is still happening (steady vs moving is itself a
//! no-colour cue):
//!
//! | state | backdrop | sound |
//! |---|---|---|
//! | working (boot → verdict) | solid violet, as before | silent |
//! | PASS | slow blink, ~1 Hz, **black ↔ white** | three short beeps, ~1 kHz, 120 ms on / 120 ms off |
//! | FAIL | fast blink, ~5 Hz, **black ↔ white** | one long low buzz, ~200 Hz, ~1 s (verdict only; a boot
//! refusal has no tone — see below) |
//!
//! The signal is the **luminance** swing, and the lit phase is deliberately white, not the verdict's
//! colour: a first cut blinked blue against black and called that hue-free, but pure blue's relative
//! luminance is 0.072 against black's 0.000, so the swing was 7% of the range — nearly the nothing it
//! replaced. White is 1.000, so the blink is legible with no colour perception at all, and nothing in
//! the scheme leans on hue. Because the lit phase is white for both verdicts, **rate and tone** are
//! the only carriers of pass/fail, which is the point: two channels, neither of them colour. The
//! pacing is the display's own frame clock — `GraphicsFrame::commit`
//! waits for vblank — so the blink needs no timer and no interrupt, and after the album audio is torn
//! down there is no deadline left to protect. The palette is never written from a handler.
//!
//! The tone is a square wave generated locally into the ring (no codec, no `#[path]` include of a
//! probe's `synth.rs`: §2.2 / D4), played by *re-arming the chain after the audio*
//! (`enable_sound` → a primed chunk → `arm_audio_chain`). That is the same sequence PR-3b's §4.4
//! skip path needs after a track change, so the beep is a rehearsal of it, not a detour. The beep is
//! a post-run annunciator: the album's ledger and its counters were printed before it starts and are
//! never re-stated.
//!
//! A **boot refusal** is the one case with no tone, and only because none can be played there: direct
//! sound A's sample rate *is* timer0's overflow rate, and every refusal happens while the boot clock
//! still owns timer0 at its own 256 Hz. That path paints red and blinks fast instead (see
//! `refuse`); a refusal is still perceivable without colour, which is the whole requirement.
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
    use agb::timer::{AllTimers, Divider};
    use alloc::format;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    use album_player::assets;
    use album_player::backend;
    use album_player::geometry;
    use album_player::meta::{Album, Track};
    use album_player::pcm::{PcmSource, SourceError};
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

    /// "Still working": violet, so a human watching a cart can tell booting from hung. This is the
    /// shipped `product-play.gba` pending colour, used the same way.
    const COLOR_PENDING: Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    /// The lit phase of **every** blink, and the solid flash before one: **full white**.
    ///
    /// A first cut blinked the verdict colour (pure blue `Rgb::new(0,0,255)`) against black, on the
    /// reasoning that a luminance swing needs no colour vision. It does not hold: relative luminance
    /// of pure blue is **0.072** against black's **0.000**, so the "high contrast" blink modulated 7%
    /// of the range and was very nearly the nothing the caller could not see. Red is 0.213 — better,
    /// still dim. White is **1.000**, so the swing is the whole screen and the blink is legible with
    /// no colour perception whatsoever. Hue therefore carries nothing in this scheme, not even
    /// decoratively on the blink: **rate** (slow = pass, fast = fail) and **tone** (three beeps) are
    /// the verdict, and both are unambiguous on a non-backlit screen in a sunlit room.
    const COLOR_LIT: Rgb15 = Rgb::new(255, 255, 255).to_rgb15();
    /// The "off" half of every blink: **full black**. Paired with [`COLOR_LIT`] that is a 0.000 →
    /// 1.000 luminance swing, which needs no colour perception at all — the caller is colourblind, and
    /// a violet→blue transition is exactly what they cannot see.
    const COLOR_OFF: Rgb15 = Rgb::new(0, 0, 0).to_rgb15();

    // ------------------------------------------------------- the annunciator's clock

    /// One display frame: 228 scanlines × 1,232 cycles = 280,896 CPU cycles, i.e. 59.7275 frames per
    /// second at 16,777,216 Hz. `GraphicsFrame::commit` waits for the frame boundary, so this is the
    /// quantum the blink is paced on.
    const FRAME_CYCLES: u32 = 228 * 1_232;

    /// PASS: 30 frames per phase. 30 × 280,896 = 8,426,880 c = 502 ms, so a full black↔blue cycle is
    /// 995 mHz — 0.4 % under the 1 Hz target, and nothing like the 0.2 Hz that would not read as a
    /// blink at all.
    const PASS_HALF_FRAMES: u32 = 30;

    /// FAIL: 6 frames per phase. 6 × 280,896 = 1,685,376 c = 100 ms, so a full cycle is 4,977 mHz —
    /// clearly faster than PASS without being a flicker.
    const FAIL_HALF_FRAMES: u32 = 6;

    // ------------------------------------------------------------- the annunciator's tone

    /// A square wave is a level alternation, so the generator is four lines and its pitch is exact —
    /// no codec is linked for an annunciator, and no probe's `synth.rs` is `#[path]`-included
    /// (§2.2 / D4). ±96 in the 8-bit FIFO form is audible and far from clipping.
    const TONE_LEVEL: u8 = 96;
    const PASS_BEEP_HZ: u32 = 1_000;
    const FAIL_BUZZ_HZ: u32 = 200;

    /// 120 ms on / 120 ms off, three times, at 16,384 Hz: 1,966 samples per 120.0 ms segment, so the
    /// whole PASS announcement is 9,830 samples. The FAIL buzz is one second, 16,384 samples. Both
    /// fit the 20,160-sample ring, which is what lets the entire tone be pushed before the chain
    /// arms — see the `const` assert below.
    const BEEP_ON_SAMPLES: usize = 1_966;
    const BEEP_OFF_SAMPLES: usize = 1_966;
    const BEEP_COUNT: usize = 3;
    const BUZZ_SAMPLES: usize = geometry::RATE_HZ as usize;

    const _: () =
        assert!(BEEP_ON_SAMPLES * BEEP_COUNT + BEEP_OFF_SAMPLES * (BEEP_COUNT - 1) <= RING_SAMPLES);
    const _: () = assert!(BUZZ_SAMPLES <= RING_SAMPLES);

    /// The beep is bounded in frames (the longest tone is 1 s): twice that is enough headroom for a
    /// post-audio chain to drain 20,160 samples, and a wedge fails visibly instead of blinking
    /// forever with the ring still armed.
    const BEEP_MAX_FRAMES: u32 = 120;

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
        match verdict {
            Chunk::Full => {}
            Chunk::Tail { .. } => stats.tail_periods += 1,
            Chunk::Starved => {
                // Only a starved period while the producer still has work is an underflow. The
                // ledger did not advance for it (`take_chunk` consumes nothing it does not copy),
                // so real samples stay queued behind the silence.
                if !finished {
                    stats.underflows += 1;
                }
            }
        }
        drop(feed);
        // SAFETY: aligned pointers into the static ping-pong; the other slot was handed to the DMA
        // a whole period ago, and the FIFO's 4-word pulls (µs for a whole buffer) are long finished
        // by the next period (17.1 ms at this rate) — `product_play_main.rs:878-883`.
        unsafe { rearm_dma(lp, rp, &CHAIN) };
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

        /// The producer's flow control: hold until the ring has room for everything a frame can
        /// add (`product_play_main.rs:1345-1382`).
        ///
        /// The room demanded is the declared frame size — the maximum a frame of this stream can be
        /// — not the current frame's, because a frame's true size is only known after it is
        /// decoded. The substitution is conservative in the direction that matters, and
        /// `push_bytes` re-checks with the real length and writes nothing if it refuses.
        ///
        /// **Never hold the `FEED` borrow across an iteration**: edition-2024 temporaries live to
        /// the end of the enclosing statement, so a borrow held while waiting would deadlock
        /// against the feeder's own borrow in IRQ context.
        ///
        /// Bounded in periods so a wedge fails visibly instead of hanging: the deepest legitimate
        /// wait is one frame's drain, and the bound is the ring's depth plus two frames of polling
        /// headroom.
        fn wait_for_room(&self) -> bool {
            let bound = (RING_CHUNKS + 2) * 4096;
            for _ in 0..bound {
                let enough = critical_section::with(|cs| {
                    let feed = FEED.borrow_ref(cs);
                    feed.free_samples() >= FRAME_SAMPLES
                });
                if enough {
                    return true;
                }
            }
            false
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

    /// Paint the verdict lamp: backdrop palette entry 0, committed from a frame that
    /// shows **nothing**.
    ///
    /// §4.5's trap is what makes this work: `TiledGraphicsFrame::commit` derives
    /// DISPCNT's background-enable mask from the backgrounds shown that frame, so a
    /// commit from a fresh frame with nothing shown repaints the backdrop alone. No
    /// font, no tiles, no sprites — this stays inside PR-3a's "no screen" boundary.
    fn lamp(gfx: &mut Graphics<'_>, colour: Rgb15) {
        gfx.set_background_palette_colour(0, 0, colour);
        gfx.frame().commit();
    }

    // -------------------------------------------------------------- the annunciator

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

    /// Fill `dst` with `count` × (`on` samples of a `hz` square wave, `off` samples of silence) and
    /// return the number of samples written. The last `off` is omitted, so a PASS pattern ends on its
    /// final beep.
    ///
    /// `2^32 / rate` is exact — the rate is a power of two (262,144 per Hz at 16,384 Hz) — so both
    /// 1 kHz and 200 Hz are whole multiples of the phase step and the pitch carries no rounding.
    fn fill_tone(dst: &mut [u8], hz: u32, on: usize, off: usize, count: usize) -> usize {
        let step = ((1u64 << 32) / u64::from(geometry::RATE_HZ)) as u32 * hz;
        let low = 0u8.wrapping_sub(TONE_LEVEL);
        let mut phase = 0u32;
        let mut at = 0usize;
        for beep in 0..count {
            for _ in 0..on {
                dst[at] = if phase >> 31 == 0 { TONE_LEVEL } else { low };
                phase = phase.wrapping_add(step);
                at += 1;
            }
            if beep + 1 < count {
                dst[at..at + off].fill(0);
                at += off;
            }
        }
        at
    }

    /// Arm the chain a second time and play the verdict tone.
    ///
    /// This repeats the whole arm-the-chain sequence over the same ring and `push_bytes` path the
    /// album played through — `enable_sound` → a primed chunk → `arm_audio_chain` — because that is
    /// exactly what PR-3b's §4.4 skip path needs after a track change. The beep is a rehearsal of it,
    /// not a detour from it.
    ///
    /// The album's ledger is untouched: `finished` stays true (the feeder classifies the tone's tail
    /// and the silence after it as a tail/starved period, never as an underflow), its counters were
    /// copied into `Observed` and printed before this ran, and the only thing reset is the period
    /// witness's baseline — a fresh chain restarts timer2 at 0, and a stale baseline would book the
    /// album's whole period count as one coalesced overflow.
    ///
    /// Returns the number of samples pushed; zero means the tone did not fit (excluded by the `const`
    /// assert above) and the caller must not arm anything.
    fn arm_tone(timers: &mut AllTimers<'_>, pass: bool) -> usize {
        let mut tone = vec![0u8; RING_SAMPLES];
        let pushed = if pass {
            fill_tone(
                &mut tone,
                PASS_BEEP_HZ,
                BEEP_ON_SAMPLES,
                BEEP_OFF_SAMPLES,
                BEEP_COUNT,
            )
        } else {
            fill_tone(&mut tone, FAIL_BUZZ_HZ, BUZZ_SAMPLES, 0, 1)
        };
        let tone = &tone[..pushed];

        let pushed_ok = critical_section::with(|cs| {
            STATS.borrow_ref_mut(cs).last_period = 0;
            // Both channels carry the same bytes; two immutable borrows of one buffer are fine.
            FEED.borrow_ref_mut(cs).push_bytes(tone, tone).is_ok()
        });
        if !pushed_ok {
            return 0;
        }

        let primed = critical_section::with(|cs| {
            let mut bufs = BUFS.borrow_ref_mut(cs);
            let mut feed = FEED.borrow_ref_mut(cs);
            let (verdict, lp, rp) = take_into_slot(&mut *bufs, &mut *feed, true);
            match verdict {
                Chunk::Full => {
                    // SAFETY: aligned pointers into the static ping-pong, filled for a whole
                    // transfer by `take_chunk` just above.
                    unsafe { rearm_dma(lp, rp, &CHAIN) };
                    true
                }
                _ => false,
            }
        });
        if !primed {
            return 0;
        }

        enable_sound();
        clear_timer1_if();
        arm_audio_chain(timers, &CHAIN);
        pushed
    }

    /// Whether the annunciator's ring has been fully handed to the DMA.
    fn tone_drained() -> bool {
        critical_section::with(|cs| {
            let feed = FEED.borrow_ref(cs);
            feed.consumed() >= feed.pushed()
        })
    }

    /// Stop the annunciator's chain in the album run's order: the timer chain through the handle
    /// first, then the sound hardware (`teardown_audio` does not touch the timers).
    fn stop_tone(timers: &mut AllTimers<'_>) {
        timers.timer1.set_interrupt(false).set_enabled(false);
        timers.timer0.set_enabled(false);
        timers.timer2.set_enabled(false);
        teardown_audio();
    }

    /// Blink the verdict lamp between full black and `colour`, `half_frames` display frames per
    /// phase, until `until` returns true (polled once per frame).
    ///
    /// The pacing is the display's own frame clock: `GraphicsFrame::commit` waits for vblank, so each
    /// iteration halts the CPU until the frame boundary rather than spinning, and no timer or
    /// interrupt is involved — after the album audio is torn down there is no deadline left to
    /// protect. The palette is written here, in the main loop, and never from a handler.
    ///
    /// A commit from a frame that shows nothing repaints the backdrop alone (§4.5's trap), so the
    /// empty frames between flips leave the lamp colour on screen. A flip's `lamp()` commits too, so
    /// every iteration costs one frame either way and the cadence cannot drift.
    fn blink_until(
        gfx: &mut Graphics<'_>,
        colour: Rgb15,
        half_frames: u32,
        mut until: impl FnMut() -> bool,
    ) {
        let mut frames = 0u32;
        let mut on = true; // the verdict lamp already painted `colour`
        loop {
            if until() {
                return;
            }
            frames += 1;
            if frames >= half_frames {
                frames = 0;
                on = !on;
                lamp(gfx, if on { colour } else { COLOR_OFF });
            } else {
                gfx.frame().commit();
            }
        }
    }

    /// Say what was refused, by name, and paint it — and make the refusal **perceivable**, which is
    /// the point of the annunciator.
    ///
    /// R5: a partial run that still printed PASS would be worse than no run at all, so every boot
    /// refusal ends here rather than continuing. It ends in the FAIL lamp blinking at its fast rate
    /// forever, not just a printed line: a refusal the caller cannot perceive is the exact defect
    /// this fixes, and this path used to fall straight into a bare `loop {}` after printing.
    ///
    /// A refusal has **no tone**, deliberately. Direct sound A's sample rate is timer0's overflow
    /// rate (`SOUNDCNT_H` bit 10 selects it), and every refusal happens while the boot clock still
    /// owns and runs timer0 at its own 256 Hz — so at this point no 200 Hz tone exists to play, and
    /// a static one-period buffer would be a slow 0.8 Hz thump rather than a buzz. The lamp is the
    /// signal a human reads on this path; the verdict tone belongs to a build that reached
    /// `arm_tone`, where the chain programs timer0 to 16,384 Hz itself.
    fn refuse(gfx: &mut Graphics<'_>, what: &str) -> ! {
        // Paint before printing: `blink_until` starts on the assumption that the lamp already
        // shows the verdict colour, so the first phase is the colour and the second is black.
        lamp(gfx, COLOR_LIT);
        agb::println!("[album-rom] FAIL {what}");
        // Every exit path emits a verdict: `make album-rom`'s echo tells a human to judge by the
        // `verdict` line, so a refusal that printed only `FAIL` would leave the log with no verdict
        // to read at all.
        agb::println!("[album-rom] verdict FAILED (boot refusal) — screen red, blinking fast");
        blink_until(gfx, COLOR_LIT, FAIL_HALF_FRAMES, || false);
        loop {}
    }

    // ------------------------------------------------------------------ main

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::println!("[album-rom] entry — the album player's playback path (PR-3a step iii)");

        // The lamp first, before any work that can be slow: violet says "booting", and a
        // cart left violet is a boot that never reached a verdict. `gfx` and `timers` are
        // disjoint fields of `gba`, so both handles live for the whole run.
        let mut gfx = gba.graphics.get();
        lamp(&mut gfx, COLOR_PENDING);

        // `make album-player-rom` builds without `deep-audit`: it plays the same album but proves no
        // decoded content. Say so at boot as well as in the verdict, so an unverified image cannot be
        // mistaken for a gate artifact by someone reading only the top of the log.
        #[cfg(not(feature = "deep-audit"))]
        agb::println!(
            "[album-rom] NOTE this build does not verify decoded content; it is a player artifact, not a gate artifact"
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

        // ---- the producer loop. No logging inside: one `agb::println!` is a semihosting trap,
        // which on hardware is a syscall into a host that may not exist.
        let mut halt: Option<&'static str> = None;
        while producer.frames < total_frames {
            if !producer.wait_for_room() {
                agb::println!(
                    "[album-rom] FAIL the ring wedged at frame {}; the consumer stopped advancing",
                    producer.frames
                );
                halt = Some("wait_for_room");
                break;
            }
            match producer.push_one() {
                Ok(_) => {}
                Err(Halt::Ended) => {
                    agb::println!(
                        "[album-rom] FAIL the stream ended at frame {} of {}",
                        producer.frames,
                        total_frames
                    );
                    halt = Some("stream ended early");
                    break;
                }
                Err(Halt::Failed) => {
                    halt = Some("producer");
                    break;
                }
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
        // At most one tail period is expected (zero if the clip ends exactly on a period boundary).
        let ledger_ok = pushed == total_samples
            && consumed == pushed
            && producer.frames == total_frames
            && underflows == 0
            && tails <= 1
            && halt.is_none()
            && pushed != 0;
        // The audit is part of the verdict, not supporting evidence (`product_play_main.rs:1865-1894`):
        // a coalesced overflow is a full-buffer replay, a late service is a mid-waveform splice, and
        // both are exactly what `product-play.gba` refuses — so this image's PASS cannot be weaker
        // than that one's.
        let audit_ok = coalesced == 0 && late == 0;
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

        let pass =
            ledger_ok && audit_ok && term_ok(pcm_ok) && term_ok(fifo_ok) && term_ok(assets_ok);
        let colour = COLOR_LIT;
        let half_frames = if pass {
            PASS_HALF_FRAMES
        } else {
            FAIL_HALF_FRAMES
        };
        lamp(&mut gfx, colour);
        // The audited image's line is fix 2's shape with the verdict channel renamed: it used to
        // promise "screen BLUE", a hue, and the caller cannot read hues. The unaudited one may not
        // borrow any promise that content was checked: it names the five terms and the feature that
        // is missing, which is the whole difference between the two artifacts.
        #[cfg(feature = "deep-audit")]
        agb::println!(
            "[album-rom] verdict {} — screen {} (ledger={}, audit={}, pcm={}, fifo={}, assets={})",
            if pass { "PASSED" } else { "FAILED" },
            if pass {
                "BLINKS SLOW ~1 Hz + 3 beeps"
            } else {
                "BLINKS FAST ~5 Hz, silent"
            },
            ledger_ok,
            audit_ok,
            term_name(pcm_ok),
            term_name(fifo_ok),
            term_name(assets_ok)
        );
        #[cfg(not(feature = "deep-audit"))]
        agb::println!(
            "[album-rom] verdict {} (ledger={}, audit={}, pcm={}, fifo={}, assets={} — built without deep-audit)",
            if pass { "PASSED" } else { "FAILED" },
            ledger_ok,
            audit_ok,
            term_name(pcm_ok),
            term_name(fifo_ok),
            term_name(assets_ok)
        );

        // ---- the annunciator: the blink rate carries the verdict and the tone names it too. The
        // album's chain is already torn down above and its ledger is already printed, so nothing
        // here is on a deadline and nothing here re-states the run's numbers.
        let beep_samples = arm_tone(&mut timers, pass);
        if beep_samples != 0 {
            let mut frames = 0u32;
            blink_until(&mut gfx, colour, half_frames, || {
                frames += 1;
                frames >= BEEP_MAX_FRAMES || tone_drained()
            });
        }
        let (beep_played, beep_periods) = critical_section::with(|cs| {
            let feed = FEED.borrow_ref(cs);
            let stats = STATS.borrow_ref(cs);
            (
                feed.consumed() - consumed,
                stats.handler_entries - handler_entries,
            )
        });
        stop_tone(&mut timers);
        agb::println!(
            "[album-rom] beep: {} Hz {} — {} samples, played {}/{} in {} periods [{}] — chain stopped",
            if pass { PASS_BEEP_HZ } else { FAIL_BUZZ_HZ },
            if pass {
                "three 120 ms beeps"
            } else {
                "one 1 s buzz"
            },
            beep_samples,
            beep_played,
            beep_samples,
            beep_periods,
            if beep_played == beep_samples {
                "DRAINED"
            } else {
                "SHORT"
            }
        );

        // ---- the annunciator's budget, in the units a headless log can carry. The blink is paced by
        // one commit per display frame, and a commit waits for vblank on SWI 0x05: the wait is a CPU
        // halt, not a spin, so a flip costs one frame. Timer3 (/1024) measures it rather than this
        // comment asserting it; two commits are ~2 frames, far inside timer3's 4 s wrap. The first
        // `lamp` phase-aligns the sample to a vblank boundary — otherwise the interval starts
        // part-way into a frame and under-reads by up to a whole scanline's worth of phase.
        lamp(&mut gfx, COLOR_OFF);
        timers
            .timer3
            .set_divider(Divider::Divider1024)
            .set_cascade(false)
            .set_interrupt(false)
            .set_overflow_amount(0)
            .set_enabled(true);
        let before = u32::from(timers.timer3.value());
        lamp(&mut gfx, colour);
        lamp(&mut gfx, COLOR_OFF);
        let flip_cycles = u32::from(timers.timer3.value()).wrapping_sub(before) / 2 * 1_024;
        timers.timer3.set_enabled(false);
        // Leave the lamp on the verdict colour: `blink_until` starts from "the colour is showing".
        lamp(&mut gfx, colour);
        let half_cycles = half_frames * FRAME_CYCLES;
        agb::println!(
            "[album-rom] lamp: {} — black ↔ {} every {} frames ({} c = {} ms); full cycle {} mHz",
            if pass { "PASS" } else { "FAIL" },
            if pass { "blue" } else { "red" },
            half_frames,
            half_cycles,
            u64::from(half_cycles) * 1_000 / 16_777_216,
            u64::from(16_777_216u32) * 1_000 / (2 * u64::from(half_cycles))
        );
        agb::println!(
            "[album-rom] lamp: one commit per frame ({} c, 59.73/s) is a vblank halt, so 0 c/s is spun; measured commit={flip_cycles} c (2 phase-aligned commits); album chain torn down, beep chain stopped",
            FRAME_CYCLES
        );

        // The verdict, held for as long as the cart is on. Nothing else is armed: the album's chain
        // is down and the beep's is stopped, so the annunciator cannot starve audio that is playing.
        blink_until(&mut gfx, colour, half_frames, || false);
        loop {}
    }
}

/// Host-build stub (the same dual-target pattern the other ROM files use), so a host `cargo test`
/// of this crate can build the bin at all.
#[cfg(not(target_arch = "arm"))]
fn main() {}
