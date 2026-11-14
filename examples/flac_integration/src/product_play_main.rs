//! Plan item 6b-3: the product playback path, at the target rate, driven by a
//! real `GAFP` manifest. `make product-play-rom`; run it headless under mGBA with
//! `mgba-test-runner` (the serial log is the record), or on hardware, where the
//! screen is the record.
//!
//! What this is *not*: another decode instrument. `src/main.rs` (`flac-rom`) is
//! that, and its row of record — 396,066 mean / 398,255 worst cycles per frame,
//! 37.765 % / 37.974 % of a 32,768 Hz frame (18.88 % / 18.99 % at the
//! 16,384 Hz rung this image plays) (`flac-docs/measurements.md`, 6b-2
//! pt 2) — is an **input** to the arithmetic below, not something this ROM
//! re-derives. What this ROM adds is the half that has never been measured at the
//! target rate on any path: convert, push, and the wait that paces the producer
//! against the consumer — with the **underflow count as the judge**, on the
//! product's own manifest, kernel and geometry.
//!
//! ## Where the protocol and the numbers come from
//!
//! The ring, the chunk classification and the 8-bit conversion are not a second
//! implementation. This ROM, and the host model in
//! `tests/product_play_witness.rs`, compile the *same*
//! `examples/flac_spike/src/playback.rs` and the *same* [`geometry`] module
//! through `src/lib.rs`; so do the [`gafp_pin`] constants and the [`checksum`]
//! fold the proof runs. A model that disagreed with the ROM about `CHUNK_SAMPLES`
//! would prove nothing, and a ring protocol the ROM bent would not be the one the
//! model analysed. That is the reason for the `#[path]` include rather than a
//! copy — the pattern `crates/flac-lite-gba/tests/arm_from_outside.rs` already
//! uses across these directories for the same reason.
//!
//! Every constant comes from [`geometry`], and the clock is the GBA's 2^24 Hz
//! timer clock (this ROM does not instantiate `agb`'s mixer). In register terms:
//! timer0 reloaded with `0 − 16777216/16384` (1,024 ticks per sample), a cascaded
//! timer1 reloaded with `0 − 560` and interrupting, and the
//! sound-FIFO DMA to a fixed destination in repeat mode, re-armed once per period
//! from a 140-word buffer, FIFO A on DMA1
//! and FIFO B on DMA2 (`hw.rs`'s pairing, which the rate ROM's first draft got
//! one channel wrong and Sam's ears caught, 2026-10-19). The mixer-domain
//! contract — buffer size, half-empty refill threshold, 4-word request granularity,
//! channel pairing — is met by transcription, not by arithmetic re-derived here.
//! One register agb does *not* program for sound is the transfer count: in sound-FIFO
//! mode the FIFO pulls 4 words per request (`hw.rs:56-68` writes source, destination
//! and control only), so the per-period count this ROM writes is ours — and it is the
//! shape the 10,512 Hz rate ROM already ran on silicon with.
//!
//! The hardware shape is the throwaway harness's
//! (`examples/flac_spike/src/rate_play_main.rs`), and the register names, the
//! control word, the period audit and the flow-control rule are taken from it
//! with its comments attached — it is the one implementation of this feeder that
//! has been heard to make sound. What changes is the asset side: it stopped
//! reading a generated per-frame offset table and started parsing the shipped
//! blob's manifest, and the geometry stopped living beside it.
//!
//! ## The clock
//!
//! One instrument for the whole audio phase, and it is the mixer's own timebase,
//! read through the very timers the audio runs on:
//!
//! | timer | role | reload |
//! |---|---|---|
//! | timer0 | one audio sample | `0 − 1024`, divider /1 |
//! | timer1 | one mixer period, and the feeder's interrupt | `0 − 560`, cascaded |
//! | timer2 | periods elapsed (the harness's witness, kept) | `0`, cascaded |
//!
//! Composite, that is a cycle counter whose unit *is* audio time: 1,024 cycles
//! per sample, 573,440 per period, 2,097,152 per 2,048-sample frame — the exact
//! `2^21` [`geometry::frame_cycles`] prices. See [`audio_cycles`] for why the
//! read is a median-of-three retry loop and why a single torn snapshot cannot be
//! mistaken for a good one. This replaces the free-running pair (`src/main.rs`'s `Counter`) that every
//! earlier timing row used; that instrument and this one agree to within a
//! bounded offset because both count CPU cycles at divider /1 — but this one
//! cannot drift from the audio, because it *is* the audio's clock. Deliberate
//! consequence: the prefill is **not** timed here. It runs before the audio
//! exists, and the timing it needs is not this ROM's job — the isolated control
//! for the decode stage is already measured, by `flac-rom`, on the same asset and
//! kernel, with more discipline than this loop can give it (one critical section
//! around a whole pass, no logging, no audio running underneath).
//!
//! ## How the stages are read, and how they are attributed
//!
//! Fed-phase windows are taken with interrupts **live**, so what they measure is
//! system cost — a decode window may contain a mixer interrupt, and that is the
//! truth of running this on one core. To keep that from being uninterpretable,
//! every stage also counts the **period boundaries that fell inside it** (one read
//! of the period register on each side), and the feeder's own cycles are measured
//! in its handler. The identity the throwaway harness asked for then becomes
//! checkable rather than assumed:
//!
//! ```text
//! isolated(stage) ≈ system(stage) − boundaries_in(stage) × mean(feeder)
//! ```
//!
//! For the push window, `boundaries_in` *is* the harness's `push_swallowed`, kept
//! as the same instrument under the same name, so the 2026-10-23 word-copy figure
//! (68 of 1,328 pushes ≈ 5 %) still has a successor to be compared against.
//!
//! `wait` is not a cost: it is the producer being idle because the consumer owns
//! the clock. So the report separates **work share** — decode + convert + push +
//! feeder, against the frame budget — from the wait, and prints the closure
//! identity
//!
//! ```text
//! audio time = stages + feeder + drain spin + residual
//! ```
//!
//! with the residual named. An unexplained gap in a ledger is a bug in the ledger,
//! not rounding: the residual here is this loop's bookkeeping plus the clock reads
//! themselves, and it is reported as a percentage of audio time precisely so a
//! reader can refuse the numbers if it grows.
//!
//! ## The judge
//!
//! Four things have to hold, and all four are counted rather than argued:
//!
//! 1. `underflows = 0` — every period got a whole chunk from the ring
//!    ([`playback::RingFeed::take_chunk`] consumes only what it copies, so
//!    padding cannot fool the sample ledger);
//! 2. `pushed = consumed = gafp_pin::SAMPLES` — the whole clip reached the DMA
//!    exactly once and in order (the manifest's own count, which boot holds to the
//!    pack-time pin at [`gafp_pin::SAMPLES`], so this number travels with the asset
//!    rather than being written out here), and the one short period at the end is a
//!    classified `Tail`, not a hole;
//! 3. `pcm MATCH` — the samples the proof pass decodes are the samples `flac -d`
//!    produces, pinned at pack time ([`gafp_pin::FNV_PCM`]). This is a **decode**
//!    witness: the fold is over the interleaved i16 PCM, not the 8-bit FIFO bytes the
//!    DMA plays, so it says nothing about the played signal (`FLAC.md`, the played
//!    form is 8-bit);
//! 4. `fifo MATCH` — the 8-bit FIFO bytes of the same converted frames fold to
//!    [`gafp_pin::FNV_FIFO_L`] / [`gafp_pin::FNV_FIFO_R`], the played-form witness.
//!    It is a separate pin from `pcm MATCH` on purpose: the decode witness and the
//!    played-form witness describe the same stream and must not be conflated. The
//!    fold covers the source samples handed to the ring, not the zero-padded `Tail`,
//!    and it runs in the post-audio proof pass because the push-path version was
//!    measured too expensive (the 64-bit FNV costs ~800,000 c/frame on ARM7TDMI).
//!
//! The period audit is **part of the verdict**, not supporting evidence: a
//! coalesced overflow (a full-buffer replay), a service later than the 16-sample
//! DMA grace (a mid-waveform splice), a refused audio-clock read (the instrument
//! cannot vouch for the shares) and more than one classified tail period all fail
//! the run. `over_period` and `min_lead` stay advisory — a frame over its own
//! period is what the ring is for — and so does the largest single frame's work
//! against the period it has to hide inside.
//!
//! ## Rate — one clock, and the asset declares it
//!
//! **RULED (Q14, 2026-11-11, project owner): a stream is encoded at the rate it is
//! played at, and the player does not reinterpret a declared rate to fit the clock.**
//! This image is what satisfying that ruling looks like: the asset it reads —
//! `assets/spike_l0.gfp`, the same one every other gate reads — declares **16,384 Hz**
//! in its STREAMINFO, which is the rate [`geometry::PLAYBACK_RATE_HZ`] names and the
//! only rate [`geometry::playback_rate_for_stream`] accepts; and
//! `scripts/gafp_pack.py` refuses to write a blob whose declared rate is not the
//! clock its measured rows are budgeted against. So the frame of audio this producer
//! is asked to service is the frame the asset was made for — 2,048 samples at the
//! rate they were encoded at, 2,097,152 cycles at this timer — and both sides of that
//! number come from the same declaration rather than from a mapping.
//!
//! What that replaced, kept on the record because it is why the ruling exists: until
//! plan item 7-0 the asset declared **32,000 Hz** and this image played it on the
//! 32,768 Hz timer through a mapping (`geometry::RATE_BRIDGE`, mirrored by
//! `gafp_pack.PLAYBACK_RATE_FOR_STREAM`). The audio ran 32,768/32,000 = **1.024×
//! fast, 41.06 cents sharp** — computed, not approximated, by
//! `tests/product_play_witness.rs` — and **no gate in this repository could see it**:
//! the decoded samples are identical either way, so every PCM and FIFO digest passed
//! a run that was 41 cents high. That is the 6b-3 measurement scaffolding recorded in
//! `flac-docs/measurements.md` (screen `rate 32000>32768 scaffold`, log
//! `Q14 SCAFFOLD`), and those shares are **not** this image's: the retargeted asset
//! is new bytes of a different length — 80 frames, 162,304 samples, a 512-sample
//! tail instead of a 240-sample one — and it is spelled with the format's `0b1101`
//! sample-rate code plus a 16-bit Hz field, which is what `flac` emits for 16,384 Hz.
//! Re-measuring this producer against those bytes on silicon is plan item 7-1.
//!
//! The two rates are still printed side by side, and the nominal period is still
//! printed next to the budget, on purpose: with the contract satisfied the two pairs
//! are equal, which makes that log line a running check of the identity instead of an
//! explanation of a difference. If they ever differ on a real run, the asset and the
//! geometry have parted company again — and the screen says so (`rate X!Y pitch`),
//! because on a stock unit the screen is the record a run gets.
//!
//! ## What this ROM deliberately does not do
//!
//! The same scope cuts as the harness this replaces, for the same reason — each
//! one puts cycles in the interrupt or the producer that the judge does not care
//! about. No mixer, no mixing, no volume, no pan, no effects. No seek, loop or
//! resume. No input wait: audio starts as soon as the ring is prefilled. No
//! logging inside the fed loop: the screen says where the run has got to *before*
//! it starts, which is also the last thing a wedged run on hardware would show.
//! The proof pass runs **after** the audio, not before — the rate-play ROM's
//! amendment of 2026-10-20 removed a silent full-track re-decode from its boot
//! for exactly this reason, and a playback ROM that goes quiet for ten seconds
//! before making a sound is not a product. The proof still runs, on the same
//! image, afterwards, so the screen a hardware run reads carries `pcm MATCH` for
//! the bytes it just heard; if it fails, the verdict fails.
//!
//! This is a playback ROM with a measurement overlay, not the product. 7-2 decides
//! what of it ships; the protocol it has to be able to run — prefill, ping-pong,
//! re-arm, counted underflow, classified tail — is what 7-3 needs in the first
//! place.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

/// The 8×16 text window, shared with the sibling ROM (`src/main.rs`): serial
/// exists on emulators, not on a stock unit, and 6a-7b's hardware finding is why
/// nothing presents after `show`.
#[cfg(target_arch = "arm")]
mod text;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use agb::external::critical_section::{self, Mutex};
    use agb::interrupt::{Interrupt, add_interrupt_handler};
    use agb::timer::{AllTimers, Divider};
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    use flac_lite::{DecodedFrame, Decoder, FrameBuffers, Manifest};

    use crate::text;
    use flac_integration::checksum::{Fnv1a64, fold_i16le_stereo};
    use flac_integration::geometry as geo;
    use flac_integration::{gafp_pin, playback};
    use playback::{Chunk, PushError, RingFeed, convert_frame, fold_bytes};

    /// The clip: the same FIXED-0 asset `gafp-check` guards and `flac-rom`
    /// decodes. Nothing here is generated for this ROM — the manifest, the frame
    /// walk and the pins all come from the shipped blob.
    #[cfg(feature = "master-asset")]
    static SONG: &[u8] = include_bytes!("../assets/master.gfp");
    #[cfg(not(feature = "master-asset"))]
    static SONG: &[u8] = include_bytes!("../assets/spike_l0.gfp");

    // ------------------------------------------------- geometry, by shorter name
    //
    // Aliases, not definitions: every value is `geometry`'s, and `geometry` pins
    // its own transcriptions at compile time. The short names are here so this ROM
    // reads like the harness it replaces.
    const CHUNK: usize = geo::CHUNK_SAMPLES;
    const RING_CHUNKS: usize = geo::RING_CHUNKS;
    const RING_SAMPLES: usize = geo::RING_SAMPLES;
    const PREFILL_CHUNKS: usize = geo::PREFILL_CHUNKS;
    const TICKS_PER_SAMPLE: u32 = geo::TICKS_PER_SAMPLE;
    const PERIOD_CYCLES: u32 = geo::PERIOD_CYCLES;
    const DMA_WORDS: u16 = geo::DMA_WORDS;

    // ---------------------------------------------------------------- hardware
    //
    // Transcribed from `examples/flac_spike/src/rate_play_main.rs`, carrying its
    // finding with it: these are private registers, named because agb 0.25.0 gives
    // an application no other way to drive the sound FIFO itself. Channel n's DMA
    // block is at 0x0400_00B0 + 0x0C·n, so DMA1 starts at 0xBC — not 0xB0, which
    // is DMA0, for which sound-FIFO start timing is illegal. The pairing is fixed:
    // DMA1 → FIFO A (left), DMA2 → FIFO B (right). A first draft of the rate ROM
    // shifted both blocks one channel low: mGBA discarded the DMA0 scheduling
    // (left silent) and half-tolerated the mis-pairing (right glitchy), and Sam's
    // ears caught it on 2026-10-19. Headless runs cannot see that at all.
    const REG_DMA1_SOURCE: usize = 0x0400_00BC;
    const REG_DMA1_DEST: usize = 0x0400_00C0;
    const REG_DMA1_COUNT: usize = 0x0400_00C4;
    const REG_DMA1_CONTROL: usize = 0x0400_00C6;
    const REG_DMA2_SOURCE: usize = 0x0400_00C8;
    const REG_DMA2_DEST: usize = 0x0400_00CC;
    const REG_DMA2_COUNT: usize = 0x0400_00D0;
    const REG_DMA2_CONTROL: usize = 0x0400_00D2;
    const FIFO_A: usize = 0x0400_00A0;
    const FIFO_B: usize = 0x0400_00A4;
    const SOUNDCNT_H: usize = 0x0400_0082;
    const SOUNDCNT_X: usize = 0x0400_0084;
    const SOUNDBIAS: usize = 0x0400_0088;
    const REG_IE: usize = 0x0400_0200;
    const REG_IF: usize = 0x0400_0202;
    const REG_IME: usize = 0x0400_0208;
    /// DISPSTAT, not an interrupt register: bit 0 is the hardware's "generate an
    /// interrupt on VBlank" enable. `IE`'s bit 0 only says a VBlank request would
    /// be waited for; whether one is ever *raised* is decided here, and the ROM
    /// has not printed this register until now.
    const REG_DISPSTAT: usize = 0x0400_0004;
    /// The one bit of the mask agb maintains at 0x0400_0200 that is not the audio
    /// chain: `Interrupt::VBlank`'s slot in **agb's own** numbering (`VBlank = 0`,
    /// `HBlank = 1`, `VCounter = 2`, `Timer0 = 3`, `Timer1 = 4`, …), which is not
    /// GBA's hardware IE bit map. It is the only other agb-registered source in this
    /// build, so "that bit is missing from the mask seen by the audio handler" means
    /// a second source was dispatched in the same interrupt entry as the audio
    /// service — which is the question. Which hardware request agb's `VBlank` slot
    /// actually waits for is left unclaimed here: agb's enable writes DISPSTAT bit 3
    /// (`Interrupt::VBlank::enable` → `DISPLAY_STATUS.set_bits(1, 1, 3)`), and this
    /// ROM has measured, not inferred, what those registers hold.
    const IE_OTHER_SOURCE_BIT: u16 = 1 << 0;

    /// Timer data registers live at 0x0400_0100 + 4·n (agb `timer.rs`'s own
    /// arithmetic). The audio chain uses timer0/1/2.
    const REG_TM0_DATA: usize = 0x0400_0100;
    const REG_TM1_DATA: usize = 0x0400_0104;
    const REG_TM2_DATA: usize = 0x0400_0108;

    /// The reload values as `set_overflow_amount` writes them (`0 − n`): each
    /// counter runs *up* from its negative latch to zero, so
    /// `data − reload` is how far into the current sample or period the clock
    /// stands. That is also what makes a torn read unmistakable — see
    /// [`audio_cycles`].
    const TM0_RELOAD: u16 = 0u16.wrapping_sub(TICKS_PER_SAMPLE as u16);
    const TM1_RELOAD: u16 = 0u16.wrapping_sub(CHUNK as u16);

    /// The sound DMA refills the 32-byte FIFO when it falls to half — 16 samples.
    /// A re-arm inside that window swaps the source before the repeat controller
    /// reloads it; one after it has already replayed part of the old buffer
    /// (inserted samples) and splices mid-waveform (the pop the first hardware
    /// reading of this field heard, 2026-10-19).
    const REARM_GRACE_SAMPLES: u32 = geo::REARM_GRACE_SAMPLES as u32;

    /// agb `hw.rs`'s sound-DMA control word: destination fixed (2<<5) | repeat
    /// (1<<9) | transfer words (1<<10) | sound-FIFO start timing (3<<12) |
    /// enable (1<<15).
    const DMA_SOUND_CONTROL: u16 = (2 << 5) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 15);

    /// The isolated decode instrument's row of record, from
    /// `flac-docs/measurements.md` (6b-2 pt 2, hardware, through the real
    /// manifest): cycles per frame for the decode stage alone, measured with one
    /// critical section around the whole pass and no audio running. This ROM's
    /// decode stage runs in the same kernel on the same asset, so the comparison
    /// is meaningful and the difference is the answer to a question 7-1 has to
    /// ask — what the playback image's decode costs, ring and interrupt included.
    const PINNED_DECODE_MEAN: u64 = 396_066;
    /// The worst frame of the same row.
    const PINNED_DECODE_WORST: u64 = 398_255;

    // ------------------------------------------------------------------ colors

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    // ------------------------------------------------------------------ statics

    /// Ping-pong DMA payload, word-aligned (a DMA source must be), one buffer per
    /// channel per slot. The two live slots are what the two sound DMAs are
    /// reading; which one gets handed out next is the ring's business, and
    /// `next` lives in the same static so both sides see the same alternation.
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

    // `RingFeed::new()` is `const` precisely so a consumer with no allocator in an
    // interrupt can hold one in a static, which is what its module docs ask for.
    // The const argument resolves to `geometry::RING_SAMPLES`, so the host model
    // instantiates the identical type.
    static FEED: Mutex<RefCell<RingFeed<RING_SAMPLES>>> = Mutex::new(RefCell::new(RingFeed::new()));
    static BUFS: Mutex<RefCell<SndBufs>> = Mutex::new(RefCell::new(SndBufs::new()));

    /// The feeder's view of the audio: the sample ledger's consumer half, the
    /// underflow count, and the period audit. No `Default` derive — a `static`
    /// initialiser has to name every field, and naming them is also the statement
    /// that they all start at zero.
    ///
    /// thumbv4t-none-eabi ships no core atomics, so this rides the same
    /// Mutex + RefCell critical-section discipline as `FEED`/`BUFS`: one core, and
    /// the producer only touches these inside a critical section, so no
    /// read-modify-write can interleave with the handler.
    struct Stats {
        /// Set by the producer when the file is exhausted. After that, a short
        /// chunk is a classified tail rather than a hole.
        finished: bool,
        /// The judge. Periods served with silence while the producer still had
        /// work to do.
        underflows: u32,
        /// Periods the handler was entered for (full, tail or starved).
        serviced: u32,
        tail_periods: u32,
        /// Real samples handed over in tail periods — the ledger's tail half.
        tail_real: usize,
        // --- Period audit, unchanged in meaning and in reading order from the
        // harness. Timer2 cascades from timer1 and so counts hardware periods
        // even while IME is off: IRQ delivery cannot stall it, which makes it
        // ground truth for periods elapsed versus periods serviced.
        /// The last period count seen at handler entry (also the count at exit).
        last_period: u16,
        /// Σ(gap−1) over handler entries: overflows latched into IF and
        /// coalesced into one later delivery — full-buffer replays, guaranteed,
        /// because the re-arm was a whole period late. Counted, not smoothed:
        /// this is how "the ring absorbed a burst" is told apart from "the
        /// hardware lost a period".
        coalesced: u32,
        max_period_gap: u16,
        /// Gap-1 entries whose `TM1_DATA` says the IRQ sat more than
        /// `REARM_GRACE_SAMPLES` past the overflow: the repeat DMA had already
        /// reloaded the old source, so this period replays a tail and splices
        /// mid-waveform. A late re-arm *inside* the grace is harmless and is not
        /// counted.
        late_servicings: u32,
        max_delay_samples: u32,
        // --- Where the late servicings happened. `late_servicings` says how many,
        // which is what the verdict reads; it cannot say what to *do*, because a
        // late service in the fed phase is a mid-waveform splice (the pop the
        // 2026-10-19 hardware reading of this field heard) while one after the
        // producer finished can only shorten the tail silence. So: the fed/drain
        // split, and the period each one landed on. The indices are timer2's
        // period counts — the same ground-truth counter the audit reads — and a
        // count of "1 in 582" is not actionable while an index is.
        late_fed: u32,
        late_drain: u32,
        /// Smallest and the period of the largest late delay, as timer2 counts.
        /// `u32::MAX` means "none", so an unset index cannot be mis-read as
        /// period 0 — `report` prints 0 for that case, as the unset lead
        /// watermark above already does.
        late_first_period: u32,
        late_worst_period: u32,
        // --- Which leg held the late re-arm (7-2's mechanism question).
        // The ring push in `fed_frame` is now a sequence of `CHUNK`-sized
        // interrupt-masked sections, each ~4,000 c (~3.9 samples) against a
        // 16-sample grace. Publishing the period counter at the first section's
        // first act lets the handler say, for a late service, whether the boundary
        // it is late *for* crossed inside the push sequence: the whole sequence is
        // shorter than one 560-sample period, so a boundary crossing any section
        // makes the handler's period equal this counter + 1. That identifies the
        // mechanism exactly — no timing slot to tune, no extra clock reads, no
        // extra critical section (the store rides a section that already exists).
        push_start_period: u32,
        late_in_push: u32,
        // --- The other candidate for the same lateness, measured rather than
        // argued about. agb's `interrupt_handler.s` reads the mask/IF pair as one
        // 32-bit word, masks out of the mask every source that it is about to
        // dispatch in this entry, and restores it on the way out. So the mask *as
        // seen on entry to the audio handler* is exactly "what is armed, minus what
        // fired with me": bit 0 of it still set means no second source came with
        // this service, bit 0 clear means one did — and a service that had to wait
        // for another entry is the other way a re-arm can miss its window. What
        // that bit's source is in hardware is not claimed (see
        // `IE_OTHER_SOURCE_BIT`: the bit is `Interrupt::VBlank`'s slot in agb's own
        // numbering, and the field names keep the hypothesis's name, not a
        // hardware ruling). `ie_latched` is the OR of the observations, which is
        // what makes a zero mean "never shared an entry" rather than "was never
        // armed", and records that nothing changed the mask mid-run.
        //
        // The raw registers are read at arm time and again at report time — not
        // latched, because nothing in this build writes them — so the run brackets
        // them: the first build that read DISPSTAT reported 0x0008 before the loop
        // and 0x0009 after it, which is itself worth having on the log rather than
        // assumed stable.
        ie_latched: u16,
        vblank_coentry: u32,
        /// The other side of the same observation: services where the mask still
        /// carried bit 0 on entry, i.e. the second source was armed and did not
        /// fire. A co-entry
        /// count is only meaningful beside this one — without it, "0 co-entries"
        /// cannot be told apart from "VBlank was never enabled".
        ie_vblank_armed: u32,
        // --- The feeder's own cost: the ring copy, the DMA re-arm, the FIFO
        // enables. It competes with the producer for the same core, so it belongs
        // in the duty-cycle arithmetic rather than in a footnote.
        feed_cycles_sum: u64,
        feed_cycles_worst: u32,
    }

    static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats {
        finished: false,
        underflows: 0,
        serviced: 0,
        tail_periods: 0,
        tail_real: 0,
        last_period: 0,
        coalesced: 0,
        max_period_gap: 0,
        late_servicings: 0,
        max_delay_samples: 0,
        late_fed: 0,
        late_drain: 0,
        late_first_period: u32::MAX,
        late_worst_period: u32::MAX,
        // An unset watermark: the only period it could match is 0 (MAX + 1 wraps
        // to 0), and period 0 is never called late — `gap` is measured against
        // `last_period`, which starts at 0, so the first service has gap 0.
        push_start_period: u32::MAX,
        late_in_push: 0,
        ie_latched: 0,
        vblank_coentry: 0,
        ie_vblank_armed: 0,
        feed_cycles_sum: 0,
        feed_cycles_worst: 0,
    }));

    /// Clock refusals, in their own Mutex on purpose: [`audio_cycles`] is called
    /// from the feeder while `STATS` is already mutably borrowed, so counting a
    /// refusal through `STATS` re-enters that `RefCell` and panics in IRQ context
    /// — on exactly the path that is supposed to report the failure. A refusal is
    /// "no median of three readable snapshots in six attempts", and it fails the
    /// verdict (see `main`); the fallback value is diagnostic only.
    static CLOCK_REFUSALS: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(0));

    /// The last accepted audio-cycle value. [`audio_cycles`] refuses a candidate
    /// below it: the timers are cascaded, and a snapshot can read TM1 from the new
    /// period while TM2 still carries the old count, which yields a value a period
    /// low. Time cannot go backwards, so a median below the last accepted value is
    /// a torn read: the read is refused, and the run fails. This is what keeps a
    /// stage window from wrapping to ~2^32 and being silently counted as `void`
    /// (or, worse, shrinking by a period).
    static LAST_CYCLES: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(0));

    // -------------------------------------------------------------- io, clock

    /// # Safety
    /// `addr` must be a readable, even address in the I/O region; callers pass the
    /// `REG_*`/`FIFO_*` constants above.
    unsafe fn io_read_u16(addr: usize) -> u16 {
        unsafe { (addr as *const u16).read_volatile() }
    }

    /// # Safety
    /// As [`io_read_u16`], write flavour.
    unsafe fn io_write_u16(addr: usize, value: u16) {
        unsafe { (addr as *mut u16).write_volatile(value) }
    }

    /// # Safety
    /// As [`io_read_u16`], for the 32-bit DMA address registers.
    unsafe fn io_write_u32(addr: usize, value: u32) {
        unsafe { (addr as *mut u32).write_volatile(value) }
    }

    /// Composite cycles from a matched snapshot of the audio chain.
    fn cycles_of(periods: u16, tm1: u16, tm0: u16) -> u32 {
        let in_period = u32::from(tm1.wrapping_sub(TM1_RELOAD));
        let in_sample = u32::from(tm0.wrapping_sub(TM0_RELOAD));
        u32::from(periods)
            .wrapping_mul(PERIOD_CYCLES)
            .wrapping_add(in_period.wrapping_mul(TICKS_PER_SAMPLE))
            .wrapping_add(in_sample)
    }

    /// Read the audio chain as one cycle count: the unit is the CPU cycle, and it
    /// advances at exactly the rate the audio is being played.
    ///
    /// Each snapshot reads the three counters twice and accepts it only if the low
    /// counters did not decrease and the period register did not move. A boundary
    /// always *decreases* the register that rolled, because every timer reloads
    /// with its own negative latch — that is what makes this timebase repeat. The
    /// snapshot is also rejected if TM1/TM0 are outside their per-period range (a
    /// read below the reload). That in-pair test is necessary but not sufficient:
    /// the timers are cascaded, and a snapshot can catch TM1 after its reload while
    /// TM2 still carries the old period (a value a period low) or the reverse (a
    /// period high) with every in-pair test satisfied. So three accepted snapshots
    /// are taken and their **median** is used — one outlier is the observed failure
    /// mode — and the median must also be no lower than the last accepted value,
    /// because audio time cannot go backwards. Six attempts are allowed; failing to
    /// get three valid snapshots (or a monotonic median) is a refusal, which fails
    /// the verdict: a run whose clock cannot be trusted cannot vouch for its own
    /// shares.
    ///
    /// # Safety
    /// only fixed I/O ports are read. The chain must be running for the value to
    /// mean audio time; [`play`] arms it before the first read.
    unsafe fn audio_cycles() -> u32 {
        let last = critical_section::with(|cs| *LAST_CYCLES.borrow_ref(cs));
        let mut valid = [0u32; 3];
        let mut n = 0usize;
        for _ in 0..6 {
            let t0 = unsafe { io_read_u16(REG_TM0_DATA) };
            let t1 = unsafe { io_read_u16(REG_TM1_DATA) };
            let t2 = unsafe { io_read_u16(REG_TM2_DATA) };
            let after0 = unsafe { io_read_u16(REG_TM0_DATA) };
            let after1 = unsafe { io_read_u16(REG_TM1_DATA) };
            let after2 = unsafe { io_read_u16(REG_TM2_DATA) };
            let in_period = u32::from(t1.wrapping_sub(TM1_RELOAD));
            let in_sample = u32::from(t0.wrapping_sub(TM0_RELOAD));
            if t2 == after2
                && t1 <= after1
                && t0 <= after0
                && in_period < CHUNK as u32
                && in_sample < TICKS_PER_SAMPLE
            {
                valid[n] = cycles_of(t2, t1, t0);
                n += 1;
                if n == 3 {
                    break;
                }
            }
        }
        if n < 3 {
            critical_section::with(|cs| *CLOCK_REFUSALS.borrow_ref_mut(cs) += 1);
            return last;
        }
        // Median of three: a cascaded snapshot can carry TM1 from the new period
        // while TM2 still says the old one (a value a period low) or the reverse
        // (a period high). One outlier is the observed failure mode, so the median
        // discards it; the three reads are within a few hundred cycles of each
        // other and the median is biased low by about half a read sequence, which
        // is the instrument's own floor.
        let (mut a, mut b, mut c) = (valid[0], valid[1], valid[2]);
        if a > b {
            core::mem::swap(&mut a, &mut b);
        }
        if b > c {
            core::mem::swap(&mut b, &mut c);
        }
        if a > b {
            core::mem::swap(&mut a, &mut b);
        }
        let cycles = b;
        if cycles < last {
            critical_section::with(|cs| *CLOCK_REFUSALS.borrow_ref_mut(cs) += 1);
            return last;
        }
        critical_section::with(|cs| *LAST_CYCLES.borrow_ref_mut(cs) = cycles);
        cycles
    }

    /// The period register alone: one read, and the number of mixer periods
    /// elapsed since the chain was armed. Reading it on each side of a stage
    /// window counts the period boundaries that fell inside it — the attribution
    /// channel described in the module docs, and the same instrument the
    /// harness's `push()` used for `push_swallowed`.
    ///
    /// # Safety
    /// only a fixed I/O port is read.
    unsafe fn periods_elapsed() -> u16 {
        unsafe { io_read_u16(REG_TM2_DATA) }
    }

    // ----------------------------------------------------------------- enable

    /// Sound hardware on: FIFO A → left at 100 %, FIFO B → right at 100 %, both
    /// FIFOs reset, master enable, bias at 8-bit/65,536 Hz — agb `hw.rs`'s enable
    /// sequence, transcribed because those functions are `pub(super)`.
    fn enable_sound() {
        let sound_control: u16 = (1 << 2) // sound A volume 100%
            | (1 << 9) // A to LEFT out
            | (1 << 11) // A FIFO reset
            | (1 << 3) // sound B volume 100%
            | (1 << 12) // B to RIGHT out
            | (1 << 15); // B FIFO reset
        // SAFETY: fixed I/O addresses.
        unsafe {
            io_write_u16(SOUNDCNT_H, sound_control);
            io_write_u16(SOUNDCNT_X, 1 << 7);
            io_write_u16(SOUNDBIAS, io_read_u16(SOUNDBIAS) | (1 << 14));
        }
    }

    /// Stop both sound DMAs and the sound hardware. The teardown the harness
    /// learned to do: leaving the FIFOs enabled hangs mGBA's exit.
    fn teardown_audio() {
        // SAFETY: fixed I/O addresses.
        unsafe {
            io_write_u16(REG_DMA1_CONTROL, 0);
            io_write_u16(REG_DMA2_CONTROL, 0);
            io_write_u16(SOUNDCNT_X, 0);
        }
    }

    /// Re-arm both sound DMAs from the slot just filled: stop, source, destination,
    /// count, go. `agb`'s `enable_dma_for_sound` (`hw.rs:56-68`) writes source,
    /// destination and control and no count at all — in sound-FIFO mode the FIFO pulls
    /// 4 words per request — so the count here is ours, deliberately, and it is the
    /// shape the rate ROM ran with on hardware.
    ///
    /// # Safety
    /// `lsrc`/`rsrc` must be 4-byte-aligned pointers into the static ping-pong,
    /// alive for the whole transfer.
    unsafe fn rearm_dma(lsrc: *const u8, rsrc: *const u8) {
        // SAFETY: fixed I/O addresses, and the caller's aligned static pointers.
        unsafe {
            io_write_u16(REG_DMA1_CONTROL, 0);
            io_write_u32(REG_DMA1_SOURCE, lsrc as u32);
            io_write_u32(REG_DMA1_DEST, FIFO_A as u32);
            io_write_u16(REG_DMA1_COUNT, DMA_WORDS);
            io_write_u16(REG_DMA1_CONTROL, DMA_SOUND_CONTROL);

            io_write_u16(REG_DMA2_CONTROL, 0);
            io_write_u32(REG_DMA2_SOURCE, rsrc as u32);
            io_write_u32(REG_DMA2_DEST, FIFO_B as u32);
            io_write_u16(REG_DMA2_COUNT, DMA_WORDS);
            io_write_u16(REG_DMA2_CONTROL, DMA_SOUND_CONTROL);
        }
    }

    /// Program the audio timebase for the 16,384 Hz rung (1,024 ticks per
    /// sample; not an `agb` `Frequency`, because this ROM owns the timers), and
    /// the chain is stopped with its counters written to
    /// their latches first, so audio time and the measurement clock start at the
    /// same instant. Timer2 is armed *before* timer1 can overflow, so the first
    /// period of audio is not missed by the counter that judges every period.
    fn arm_audio_chain(timers: &mut AllTimers<'_>) {
        for t in [
            &mut timers.timer0,
            &mut timers.timer1,
            &mut timers.timer2,
            &mut timers.timer3,
        ] {
            t.set_enabled(false);
        }
        timers
            .timer2
            .set_cascade(true)
            .set_interrupt(false)
            .set_overflow_amount(0);
        timers
            .timer0
            .set_divider(Divider::Divider1)
            .set_interrupt(false)
            .set_overflow_amount(TICKS_PER_SAMPLE as u16);
        timers
            .timer1
            .set_cascade(true)
            .set_interrupt(true)
            .set_overflow_amount(CHUNK as u16);
        // Nothing in this ROM uses timer3; leaving a stale latch in it from an
        // earlier boot pass would only confuse the audit.
        timers.timer3.set_cascade(false).set_interrupt(false);
        timers.timer2.set_enabled(true);
        timers.timer0.set_enabled(true);
        timers.timer1.set_enabled(true);
    }

    // ----------------------------------------------------------------- feeder

    /// The mixer interrupt: hand the next chunk to the sound DMA, or silence if
    /// the producer has not delivered one.
    ///
    /// Nothing here allocates and nothing here logs — agb's interrupt-handler
    /// contract, and the reason the ring is a static array. All three static
    /// borrows use the `CriticalSection` token the handler is handed, which is
    /// also the race gate: the producer only touches the ring inside a critical
    /// section, so a copy cannot be interleaved by anything.
    fn feeder(cs: critical_section::CriticalSection<'_>) {
        // The audit reads stay first, exactly as the harness orders them: their
        // whole point is how long the interrupt sat, and any work before them
        // would be inside the answer. (agb's asm prologue plus these reads costs
        // ~10² cycles, far under the 8,192-cycle grace this rate's DMA gives it.)
        let (periods, delay) = unsafe {
            (
                io_read_u16(REG_TM2_DATA),
                u32::from(io_read_u16(REG_TM1_DATA).wrapping_sub(TM1_RELOAD)),
            )
        }; // SAFETY: fixed I/O addresses.
        let entered = unsafe { audio_cycles() };

        let mut bufs = BUFS.borrow_ref_mut(cs);
        let mut feed = FEED.borrow_ref_mut(cs);
        let mut stats = STATS.borrow_ref_mut(cs);

        // What the dispatcher left in IE: every source that triggered in this
        // entry has been masked out of it and will be restored on the way out, so
        // a bit still set here did NOT fire with this service. Taken after the
        // audit reads above, which is what keeps it out of the answer those reads
        // are the measurement of; its own cost is ~10^2 cycles and it lands in
        // the feeder's reported cost, which already says it carries the
        // instrument. `ie_latched` recovers bit 0 from any entry that had it, and
        // that is what makes a zero co-entry count mean "never happened" rather
        // than "never enabled".
        let ie_now = unsafe { io_read_u16(REG_IE) }; // SAFETY: fixed I/O address.
        stats.ie_latched |= ie_now;
        if ie_now & IE_OTHER_SOURCE_BIT == 0 {
            stats.vblank_coentry += 1;
        } else {
            // A service that could not have been delayed by that source: the mask
            // still says it is armed and it did not fire in this entry.
            stats.ie_vblank_armed += 1;
        }

        let gap = periods.wrapping_sub(stats.last_period);
        stats.last_period = periods;
        if gap > 1 {
            stats.coalesced += u32::from(gap - 1);
            if gap > stats.max_period_gap {
                stats.max_period_gap = gap;
            }
        }
        // A gap > 1 means TM1_DATA re-wrapped past a whole period, so the
        // coalesced count already owns that case; only gap-1 entries time a
        // sub-period lateness, which is the late-splice event.
        let late_splice = gap == 1 && delay > REARM_GRACE_SAMPLES;
        if late_splice {
            stats.late_servicings += 1;
            if delay > stats.max_delay_samples {
                stats.max_delay_samples = delay;
                stats.late_worst_period = u32::from(periods);
            }
            if u32::from(periods) < stats.late_first_period {
                stats.late_first_period = u32::from(periods);
            }
            // Which leg held it. See `push_start_period`: the value published at
            // the first chunk section's start names the boundary that crossed
            // inside the push sequence, and nothing else can — IME goes back when
            // each chunk section ends, but the whole sequence is shorter than a
            // period, so the handler still enters with this period + 1.
            if u32::from(periods) == stats.push_start_period.wrapping_add(1) {
                stats.late_in_push += 1;
            }
        }
        stats.serviced += 1;
        let finished = stats.finished;
        // The fed/drain split is classified here rather than inside the branch
        // above only because `finished` is read with the rest of the
        // producer-state reads. The io reads that *are* the measurement stay
        // first either way, which is what the note at the top of this handler
        // requires; the arithmetic those reads feed is unchanged by where the
        // tallies sit.
        if late_splice {
            if finished {
                stats.late_drain += 1;
            } else {
                stats.late_fed += 1;
            }
        }

        // Raw pointers to the slot BEFORE filling, in their own statement: the
        // static never moves, so they stay valid past the borrow, and
        // `rearm_dma` consumes them once the copy is done. Destructuring to field
        // borrows avoids double-borrowing the struct through two `IndexMut`s in
        // one expression (which is what `bufs.l[bufs.next]` would be).
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
        match verdict {
            Chunk::Full => {}
            Chunk::Tail { real } => {
                stats.tail_periods += 1;
                stats.tail_real += real;
            }
            Chunk::Starved => {
                // Only a starved period while the producer still has work is an
                // underflow. The ledger did not advance for it (`take_chunk`
                // consumes nothing it does not copy), so real samples stay queued
                // behind the silence.
                if !finished {
                    stats.underflows += 1;
                }
            }
        }
        // SAFETY: aligned pointers into the static ping-pong; the other slot was
        // handed to the DMA a whole period ago, and the FIFO's 4-word pulls (µs for a
        // whole buffer) are long finished by the next period (17.1 ms at this rate).
        unsafe { rearm_dma(lp, rp) };

        drop(feed);
        drop(bufs);
        let cost = unsafe { audio_cycles() }.wrapping_sub(entered);
        // The exit read is taken after the DMA work, so the measured window
        // includes the re-arm and excludes the handler's entry cost; the entry
        // read is the same instrument, ~10² cycles into the handler. Both are
        // reported as part of the feeder's cost, which is what they are.
        if cost > stats.feed_cycles_worst {
            stats.feed_cycles_worst = cost;
        }
        stats.feed_cycles_sum += u64::from(cost);
    }

    // --------------------------------------------------------------- ledgers

    /// One measured stage: how many times it ran, what it cost in total, the most
    /// and least any single call cost, and how many period boundaries fell inside
    /// its windows. A `best` is kept because a window that reads far shorter than
    /// its kind is as much a sign the instrument is measuring the wrong thing as a
    /// long one is. A `void` count keeps a refused window out of the sums while
    /// still admitting it happened.
    #[derive(Clone, Copy, Default)]
    struct Stage {
        n: u32,
        sum: u64,
        worst: u32,
        best: u32,
        /// Period boundaries inside this stage's windows. For the push stage this
        /// is the harness's `push_swallowed`.
        inside: u32,
        void: u32,
    }

    impl Stage {
        fn note(&mut self, cycles: u32, boundaries: u16) {
            self.n += 1;
            // A window over half the counter's range is not a measurement: at
            // this rate that is ~4 s, longer than any stage here can legitimately
            // run, so it means the clock was read wrong (or wrapped). A void window
            // counts in `n` (it happened) but is excluded from `sum`, `inside`,
            // `worst` and `best` — none of them is a price, and it fails the verdict
            // anyway, so no quoted number ever contains one.
            if cycles >= u32::MAX / 2 {
                self.void += 1;
                return;
            }
            self.inside += u32::from(boundaries);
            self.sum += u64::from(cycles);
            // The first *measured* window sets both extremes, not the first window
            // seen: a void one would otherwise leave `best` pinned at 0 forever.
            let measured = self.n - self.void;
            if measured == 1 || cycles > self.worst {
                self.worst = cycles;
            }
            if measured == 1 || cycles < self.best {
                self.best = cycles;
            }
        }

        fn mean(&self) -> u64 {
            // A void window is in `n` (it happened) but not in `sum` (it was not
            // priced). Dividing by `n` would bias the mean low; divide by what was
            // actually measured.
            let measured = self.n.saturating_sub(self.void);
            if measured == 0 {
                0
            } else {
                self.sum / u64::from(measured)
            }
        }
    }

    /// Everything the audio phase measured, and everything it judged. Field names
    /// are the report's section headings, so the log and the struct can be
    /// compared line by line.
    struct Ledger {
        /// Frames the producer pushed through the ring.
        frames: usize,
        /// Cycles for one frame of the manifest's blocksize — the budget every
        /// share below is divided by, from `geometry`, never recalled.
        frame_budget: u32,
        stage_names: [&'static str; 4],
        /// 0 = wait, 1 = decode, 2 = convert, 3 = push.
        stages: [Stage; 4],
        /// The smallest ring lead seen at any producer instant, in samples. A
        /// frame adds nothing to the ring until its push, and the consumer only
        /// takes at period boundaries, so the lead falls monotonically between two
        /// pushes: the value read immediately before each push *is* the run's true
        /// floor, not a sample of it.
        min_lead: usize,
        max_lead: usize,
        /// The largest work one frame's loop took — decode + convert + push, never
        /// the wait — and how many frames' work exceeded their own period. A frame
        /// over its period is survivable, that is what the ring is for, but it is
        /// the shape a sustained overage is made of, and it is the only one of
        /// these numbers that is a real-time cost. Mixing the wait into it makes
        /// the number meaningless: a flow-controlled producer deliberately takes
        /// more wall time than it has work.
        max_work: u32,
        over_period: u32,
        /// The pacer's own period: `wait + decode + convert + push` for one frame,
        /// i.e. the wall time the producer actually spends per frame. Not a cost.
        /// It is measured because it is the evidence that the flow control is
        /// tracking the audio: in steady state its mean must sit on the frame
        /// budget, and the ring is what absorbs the difference between it and the
        /// producer's true work.
        paced: Stage,
        /// Audio-phase spans, from the instant the chain was armed.
        audio_start: u32,
        audio_end: u32,
        audio_drain_start: u32,
        /// The producer's own finish instant, for the closure identity.
        audio_producer_end: u32,
    }

    impl Ledger {
        fn new(blocksize: u32) -> Self {
            Self {
                frames: 0,
                frame_budget: geo::frame_cycles(blocksize),
                stage_names: ["wait  ", "decode", "convert", "push  "],
                stages: [Stage::default(); 4],
                min_lead: usize::MAX,
                max_lead: 0,
                max_work: 0,
                over_period: 0,
                paced: Stage::default(),
                audio_start: 0,
                audio_end: 0,
                audio_drain_start: 0,
                audio_producer_end: 0,
            }
        }

        /// Producer work: everything the loop does that is not waiting. The
        /// feeder is added by the caller because it runs in the interrupt.
        fn work_sum(&self) -> u64 {
            self.stages[1].sum + self.stages[2].sum + self.stages[3].sum
        }

        /// All stage windows, wait included — the closure identity's left side.
        fn stages_sum(&self) -> u64 {
            self.stages.iter().map(|s| s.sum).sum()
        }

        /// Cycles as a share of one frame's budget, ×1000 (`37974` is
        /// "37.974 %"). Integer arithmetic: there is no float in this image.
        fn pct_x1000(&self, cycles: u64) -> u64 {
            cycles * 100_000 / u64::from(self.frame_budget)
        }
    }

    /// `total` as a per-window average ×100, so a fraction of a period per frame
    /// is visible instead of a bare count that a reader has to divide by hand.
    fn per_window_x100(total: u32, windows: u32) -> u64 {
        if windows == 0 {
            0
        } else {
            u64::from(total) * 100 / u64::from(windows)
        }
    }

    // ----------------------------------------------------------- decode seam

    /// One frame through the kernel this image names. The two shapes are identical
    /// apart from the type argument, which is the point: the seam is a delegation
    /// at the call site, exactly as in `src/main.rs`, so the kernel A/B stays an
    /// A/B across two images that otherwise differ in nothing.
    #[cfg(feature = "arm-kernel")]
    fn decode_one<'b>(
        decoder: &mut Decoder<'_>,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
        decoder.decode_next_with::<flac_lite_gba::rice_arm::ArmRiceKernel>(bufs)
    }

    #[cfg(not(feature = "arm-kernel"))]
    fn decode_one<'b>(
        decoder: &mut Decoder<'_>,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
        decoder.decode_next(bufs)
    }

    /// Parse the shipped blob's manifest and hold it to the pins and the profile —
    /// the same checks `src/main.rs` runs, so the two images cannot disagree about
    /// what they are decoding. The manifest's own declared sample rate is what the
    /// rate check is made against, and since plan item 7-0 the only answer that
    /// check accepts is the identity: the asset declares 16,384 Hz because that
    /// is the rate this geometry and the packer's budget use.
    /// Anything else is refused here rather than played 41 cents sharp
    /// (`flac-docs/open-questions.md` Q14; [`geometry::RATE_BRIDGE`] is `None`).
    fn manifest() -> Option<Manifest<'static>> {
        let manifest = match Manifest::parse(SONG) {
            Ok(m) => m,
            Err(e) => {
                agb::println!("[pp-rom] FAIL manifest: {:?}", e);
                return None;
            }
        };
        agb::println!(
            "[pp-rom] manifest: frames={} samples={} rate={} ch={} bps={} block={} max_fixed_order={}",
            manifest.frame_count(),
            manifest.total_samples(),
            manifest.sample_rate_hz(),
            manifest.channels(),
            manifest.bits_per_sample(),
            manifest.blocksize(),
            manifest.max_fixed_order()
        );
        if manifest.frame_count() != gafp_pin::FRAMES
            || manifest.total_samples() != gafp_pin::SAMPLES
            || manifest.max_fixed_order() != gafp_pin::MAX_FIXED_ORDER
        {
            agb::println!("[pp-rom] FAIL manifest disagrees with the generated pin");
            return None;
        }
        if flac_lite::format::DEFAULT_PROFILE
            .validate(&manifest)
            .is_err()
        {
            agb::println!("[pp-rom] FAIL manifest outside the encode profile");
            return None;
        }
        if manifest.channels() != 2 {
            // The DMA pairing, the ping-pong and the FIFO8 pins are all stereo
            // shaped; a mono asset is a real product case and 7-2's to add.
            agb::println!(
                "[pp-rom] FAIL this playback path is stereo-only (channels={})",
                manifest.channels()
            );
            return None;
        }
        let stream_rate = manifest.sample_rate_hz();
        match geo::playback_rate_for_stream(stream_rate) {
            // Identity, and only identity: `geo::playback_rate_for_stream` returns
            // `Some` for exactly the rate this geometry is built for now that
            // `geo::RATE_BRIDGE` is `None` (Q14, plan item 7-0). The arm is written
            // this way so that a re-introduced mapping would have to land in one of
            // the two arms below and be refused or reported, not pass silently.
            Some(geo::PLAYBACK_RATE_HZ) => {}
            Some(other) => {
                agb::println!(
                    "[pp-rom] FAIL the rate mapping gives {} Hz but this geometry is built for {}",
                    other,
                    geo::PLAYBACK_RATE_HZ
                );
                return None;
            }
            None => {
                agb::println!(
                    "[pp-rom] FAIL stream rate {} has no playback mapping (see scripts/gafp_pack.py)",
                    stream_rate
                );
                return None;
            }
        }
        // Both numbers, printed side by side, as a running check of the identity
        // rather than an explanation of a difference: the declared rate and the
        // playback rate are the same number, and the stream's own nominal frame
        // period and this timer's frame budget are therefore the same number. If a
        // real run ever prints two different ones, the asset and the geometry have
        // parted company again — which is the Q14 defect, and it is worth having in
        // the log.
        let blocksize = u32::from(manifest.blocksize());
        agb::println!(
            "[pp-rom] rate: stream {} Hz -> playback {} Hz; {} cycles/sample, budget {} cycles for a {}-sample frame; the stream's own nominal period is {} c — equal by the rate contract. Two different numbers here = the Q14 defect.",
            stream_rate,
            geo::PLAYBACK_RATE_HZ,
            TICKS_PER_SAMPLE,
            geo::frame_cycles(blocksize),
            blocksize,
            u64::from(geo::MIXER_TICK_HZ) * u64::from(blocksize) / u64::from(stream_rate)
        );
        if stream_rate != geo::PLAYBACK_RATE_HZ {
            // Unreachable while the match above admits only the identity case, and
            // kept for that reason: it is the Q14 tripwire in the one place a
            // hardware run leaves a record.
            agb::println!(
                "[pp-rom] Q14 VIOLATION: stream {} Hz is played at {} Hz — pitch-shifted, NOT a product asset",
                stream_rate,
                geo::PLAYBACK_RATE_HZ
            );
        }
        Some(manifest)
    }

    // ------------------------------------------------------------- the player

    /// The producer: prefill, then one frame at a time at the consumer's pace,
    /// then the drain. Everything that touches the ring from the producer side is
    /// in this impl, so the critical-section rule has one place to live.
    struct Producer<'d, 'b> {
        decoder: Decoder<'d>,
        bufs: FrameBuffers<'b>,
        scratch_l: Vec<u8>,
        scratch_r: Vec<u8>,
        blocksize_max: usize,
        /// The manifest's total sample count. Used to set `STATS.finished` in the
        /// same critical section as the final push, so the feeder cannot see the
        /// ring empty with the flag still false and charge the classified tail as
        /// an underflow.
        total_samples: usize,
        ledger: &'b mut Ledger,
    }

    /// Why a frame did not make it into the ring.
    enum Halt {
        /// The stream ended before the frame count said it would.
        Ended,
        /// A decode, conversion or push error, with the reason.
        Failed(&'static str),
    }

    impl Producer<'_, '_> {
        /// Prefill: decode-and-push until the ring holds the lead the first frame
        /// is allowed to spend, with the feeder not yet existing. Not timed —
        /// see the module docs on the clock, and note that this is why the ring
        /// cannot be over-filled here: it starts empty and the target is 8,960 of
        /// 20,160 samples.
        fn prefill(&mut self, target: usize) -> Result<usize, Halt> {
            let mut pushed = 0usize;
            while pushed < target {
                let n = self.decode_and_push()?;
                pushed += n;
            }
            Ok(pushed)
        }

        /// Append one decoded and converted frame in `CHUNK`-sized slices, each
        /// under its own short critical section.
        ///
        /// This is the 7-2 fix. A whole-frame push held `IME = 0` for the
        /// measured 14,680 c (~14.3 samples at 1,024 ticks/sample), against the
        /// 16-sample sound-DMA re-arm grace; the worst push window recorded was
        /// 15,831 c (~15.46 samples), leaving only 0.5 samples of margin. Four
        /// slices of at most `CHUNK` samples cost ~4,014 c each (~3.92 samples),
        /// a 4.08x margin. The slices are complete pushes: `push_bytes` credits
        /// only after the copy, so the feeder sees either the previous slice or
        /// the new one, never a torn one. Interrupts are live between slices,
        /// which is the point.
        ///
        /// `attribution_period` is the period counter read at the start of the
        /// first slice. It is stored once for `Stats::push_start_period`: the
        /// whole sequence is shorter than one period, so a boundary that crosses
        /// any slice makes the handler's period equal this value + 1. That keeps
        /// the `late in push` witness meaningful after the section was split.
        ///
        /// Returns `(lead_before, lead_after, outcome)`: the first slice's
        /// pre-push watermark and the last slice's post-push watermark, for the
        /// fed-phase lead audit. A failed slice stops the sequence and reports
        /// the error; in a passing run the room check makes that unreachable.
        fn push_frame_in_chunks(
            &mut self,
            n: usize,
            attribution_period: Option<u32>,
        ) -> (usize, usize, Result<(), PushError>) {
            let mut offset = 0usize;
            let mut lead_before = 0usize;
            let mut lead_after = 0usize;
            while offset < n {
                let end = (offset + CHUNK).min(n);
                let first = offset == 0;
                let (before, after, outcome) = critical_section::with(|cs| {
                    let (before, after, outcome) = {
                        let mut feed = FEED.borrow_ref_mut(cs);
                        let before = feed.ready();
                        if first {
                            if let Some(period) = attribution_period {
                                STATS.borrow_ref_mut(cs).push_start_period = period;
                            }
                        }
                        let outcome = feed
                            .push_bytes(&self.scratch_l[offset..end], &self.scratch_r[offset..end]);
                        (before, feed.ready(), outcome)
                    };
                    if outcome.is_ok() && FEED.borrow_ref(cs).pushed() == self.total_samples {
                        STATS.borrow_ref_mut(cs).finished = true;
                    }
                    (before, after, outcome)
                });
                if first {
                    lead_before = before;
                }
                lead_after = after;
                if outcome.is_err() {
                    return (lead_before, lead_after, outcome);
                }
                offset = end;
            }
            (lead_before, lead_after, Ok(()))
        }

        /// One frame of the **prefill**: decode, convert, push, with no stage windows
        /// (the prefill is not timed — see the module docs) and no flow-control wait
        /// (the ring is empty, so there is always room). The fed phase is
        /// [`Producer::fed_frame`], which shares the same `decode_one` /
        /// `convert_frame` / `push_bytes` seam but wraps each step in a measured window
        /// and waits for room first. The two are deliberately separate functions and
        /// not one: their error paths and their bookkeeping differ, and the aliasing
        /// rule (one critical section per chunk, never across a wait) is easier to see
        /// twice than to thread through a flag.
        fn decode_and_push(&mut self) -> Result<usize, Halt> {
            let frame = match decode_one(&mut self.decoder, &mut self.bufs) {
                Ok(Some(f)) => f,
                Ok(None) => return Err(Halt::Ended),
                Err(e) => {
                    agb::println!(
                        "[pp-rom] FAIL decode at frame {}: {:?}",
                        self.ledger.frames,
                        e
                    );
                    return Err(Halt::Failed("decode"));
                }
            };
            let n = frame.left.len();
            // `convert_frame` and `push_bytes` take the two channels separately,
            // which is the shape the ring and the DMA want (one buffer per
            // channel); the frame's right window is already the right channel.
            if convert_frame(
                frame.left,
                frame.right,
                &mut self.scratch_l[..n],
                &mut self.scratch_r[..n],
            )
            .is_err()
            {
                agb::println!(
                    "[pp-rom] FAIL convert at frame {}: sample outside the 16-bit profile",
                    self.ledger.frames
                );
                return Err(Halt::Failed("convert"));
            }
            // The `finished` flag is set in the *same* critical section as the
            // final push. Setting it after the producer loop leaves a window in
            // which the feeder can see an empty ring while the flag is still
            // false and count the final short chunk as an underflow.
            let (_, _, outcome) = self.push_frame_in_chunks(n, None);
            if let Err(e) = outcome {
                // Named rather than swallowed: the ring's own numbers are what a
                // sizing mistake is read from, and `push_bytes` re-checks with the
                // frame's real length, so this is the second of the two checks the
                // module docs describe.
                agb::println!(
                    "[pp-rom] FAIL push refused at frame {}: {:?}",
                    self.ledger.frames,
                    e
                );
                return Err(Halt::Failed("push"));
            }
            self.ledger.frames += 1;
            Ok(n)
        }

        /// The producer's flow control: hold until the ring has room for
        /// everything a frame could add.
        ///
        /// The room demanded is the manifest's declared blocksize — the maximum a
        /// frame of this stream can be — not the current frame's, because a
        /// frame's true size is only known after it is decoded. That substitution
        /// is conservative in the direction that matters: it makes the producer
        /// wait longer, never less, and `RingFeed::push_bytes` re-checks with the
        /// real length and writes nothing if it refuses. The host model applies the
        /// identical rule, so its predicted floor is this ROM's `min_lead`.
        ///
        /// NEVER hold the `FEED` borrow across an iteration: edition-2024
        /// temporaries live to the end of the enclosing statement, so a borrow
        /// held while waiting would deadlock against the feeder's own borrow in IRQ
        /// context (the harness records catching that in review, before compile).
        ///
        /// The loop is bounded in *periods* so that a wedge fails visibly instead
        /// of hanging: the deepest legitimate wait is one frame's drain (~2,048
        /// samples = 2,097,152 cycles), and the bound is the ring's depth plus two
        /// frames' worth of polling headroom. A few hundred cycles per iteration
        /// puts it well under a second; the previous `8_000_000` was "until the
        /// outer timeout", not a bound.
        fn wait_for_room(&self) -> bool {
            let bound = (RING_SAMPLES / CHUNK + 2) * 4096;
            for _ in 0..bound {
                let enough = critical_section::with(|cs| {
                    let feed = FEED.borrow_ref(cs);
                    let free = feed.free_samples();
                    drop(feed);
                    free >= self.blocksize_max
                });
                if enough {
                    return true;
                }
            }
            false
        }

        /// A fed-phase frame: the four stages, each with its own window and its
        /// own boundary count, and the ring watermark around the push.
        fn fed_frame(&mut self) -> Result<(), Halt> {
            let (t0, p0) = unsafe { (audio_cycles(), periods_elapsed()) };
            if !self.wait_for_room() {
                agb::println!("[pp-rom] FAIL the ring wedged; the consumer stopped advancing");
                return Err(Halt::Failed("wait"));
            }
            let (t1, p1) = unsafe { (audio_cycles(), periods_elapsed()) };

            let frame = match decode_one(&mut self.decoder, &mut self.bufs) {
                Ok(Some(f)) => f,
                Ok(None) => return Err(Halt::Ended),
                Err(e) => {
                    agb::println!(
                        "[pp-rom] FAIL decode at frame {}: {:?}",
                        self.ledger.frames,
                        e
                    );
                    return Err(Halt::Failed("decode"));
                }
            };
            let (t2, p2) = unsafe { (audio_cycles(), periods_elapsed()) };

            let n = frame.left.len();
            if convert_frame(
                frame.left,
                frame.right,
                &mut self.scratch_l[..n],
                &mut self.scratch_r[..n],
            )
            .is_err()
            {
                agb::println!(
                    "[pp-rom] FAIL convert at frame {}: sample outside the 16-bit profile",
                    self.ledger.frames
                );
                return Err(Halt::Failed("convert"));
            }
            let (t3, p3) = unsafe { (audio_cycles(), periods_elapsed()) };

            // One critical section per CHUNK-sized slice, not one for the whole
            // frame. The old whole-frame section was the 7-2 defect: it held
            // interrupts for ~14,680 c (~14.3 samples at this clock) while the
            // sound DMA re-arm has a 16-sample grace. A chunk-sized section is
            // ~4,014 c (~3.92 samples), a 4.08x margin. The feeder can run
            // between slices; every slice is a complete, credited push, so it
            // sees either the previous slice or this one, never a partial one.
            let (lead_before, lead_after, outcome) =
                self.push_frame_in_chunks(n, Some(u32::from(p3)));
            if let Err(e) = outcome {
                agb::println!(
                    "[pp-rom] FAIL push refused at frame {}: {:?} (room was demanded for {} samples before the frame was decoded)",
                    self.ledger.frames,
                    e,
                    self.blocksize_max
                );
                return Err(Halt::Failed("push"));
            }
            let (t4, p4) = unsafe { (audio_cycles(), periods_elapsed()) };

            let l = &mut *self.ledger;
            l.stages[0].note(t1.wrapping_sub(t0), p1.wrapping_sub(p0));
            l.stages[1].note(t2.wrapping_sub(t1), p2.wrapping_sub(p1));
            l.stages[2].note(t3.wrapping_sub(t2), p3.wrapping_sub(p2));
            l.stages[3].note(t4.wrapping_sub(t3), p4.wrapping_sub(p3));
            // `work_only` is the real-time cost and is what the shares are made
            // of; `paced` is the wall time the pacer allowed for this frame, which
            // includes deliberate idleness. Reporting only the second would make
            // the ROM look like it had no margin at all (93 of these 152 frames
            // exceed a frame budget, and that is flow control, not overruns).
            let work_only = t2
                .wrapping_sub(t1)
                .wrapping_add(t3.wrapping_sub(t2))
                .wrapping_add(t4.wrapping_sub(t3));
            if work_only > l.max_work {
                l.max_work = work_only;
            }
            if work_only > l.frame_budget {
                l.over_period += 1;
            }
            l.paced.note(t4.wrapping_sub(t0), p4.wrapping_sub(p0));
            if lead_before < l.min_lead {
                l.min_lead = lead_before;
            }
            if lead_after > l.max_lead {
                l.max_lead = lead_after;
            }
            l.frames += 1;
            Ok(())
        }
    }

    /// The audio phase: prefill (ring only), arm the chain and the sound
    /// hardware, produce at the consumer's pace until the stream is exhausted,
    /// flag it, drain, stop. Returns the counts the verdict is made of:
    /// (pushed, consumed, underflows, frames).
    // The `Producer` is taken by value on purpose: it owns the only mutable borrow
    // of the ledger, so nothing else in this function may hold one, and consuming
    // it releases that borrow for the report afterwards.
    fn play(
        mut producer: Producer<'_, '_>,
        total_frames: usize,
        total_samples: usize,
        timers: &mut AllTimers<'_>,
    ) -> (usize, usize, u32, usize) {
        let prefilled = match producer.prefill(PREFILL_CHUNKS * CHUNK) {
            Ok(n) => n,
            Err(_) => return (0, 0, 1, 1),
        };
        agb::println!(
            "[pp-rom] prefilled {} samples in {} frames (target {}), then arming",
            prefilled,
            producer.ledger.frames,
            PREFILL_CHUNKS * CHUNK
        );

        // The first buffer is handed to the DMA before the first period boundary
        // can ask for one, so audio begins at arming rather than one period late.
        // The chain is armed after that prime: `rearm_dma` in repeat mode starts
        // playing the moment it is written, and the first period boundary is then a
        // re-arm like any other — which is the case the grace window and the
        // late-service audit are about.
        let primed = critical_section::with(|cs| {
            let mut bufs = BUFS.borrow_ref_mut(cs);
            let mut feed = FEED.borrow_ref_mut(cs);
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
                let taken = feed.take_chunk(dl, dr, CHUNK, false);
                *next = 1 - *next;
                taken
            };
            match verdict {
                // SAFETY: aligned pointers into the static ping-pong; the other
                // slot is the one the next period will be handed.
                Chunk::Full => {
                    unsafe { rearm_dma(lp, rp) };
                    true
                }
                other => {
                    agb::println!("[pp-rom] FAIL prime: {:?}", other);
                    false
                }
            }
        });
        if !primed {
            return (0, 0, 1, 1);
        }

        enable_sound();
        // Clear a stale Timer1 IF *before* the source is enabled: registering the
        // handler sets the IE bit, and a pending bit left by the boot path would
        // enter the feeder before the chain has a period to serve. Writing 1 to
        // the IF bit clears it. The first real overflow is 560 samples away, so
        // installing the handler before arming loses no period.
        unsafe { io_write_u16(REG_IF, 1 << 4) };
        // SAFETY: the handler allocates nothing, logs nothing and touches only the
        // statics above — agb's interrupt-handler contract.
        let _handler = unsafe { add_interrupt_handler(Interrupt::Timer1, feeder) };
        arm_audio_chain(timers);
        producer.ledger.audio_start = unsafe { audio_cycles() };
        let periods_x100 = geo::PLAYBACK_RATE_HZ * 100 / CHUNK as u32;
        agb::println!(
            "[pp-rom] armed: {}.{:02} periods/s of {} cycles, frame budget {} cycles; IE=0x{:04X} IME={} DISPSTAT=0x{:04X}",
            periods_x100 / 100,
            periods_x100 % 100,
            PERIOD_CYCLES,
            producer.ledger.frame_budget,
            unsafe { io_read_u16(REG_IE) },
            unsafe { io_read_u16(REG_IME) },
            unsafe { io_read_u16(REG_DISPSTAT) }
        );
        // Its own line, because the printer cuts at 256 characters and the part
        // that was being cut off is the part that says what the numbers mean. IE as
        // printed here is the mask agb maintains at 0x0400_0200 in **agb's own** bit
        // numbering (`Interrupt::VBlank = 0`, `Interrupt::Timer1 = 4`), and the two
        // bits present are exactly those two: the audio chain's period source and one
        // other source. Which hardware request that other bit stands for is not
        // settled here — agb's `Interrupt::VBlank::enable()` writes DISPSTAT bit 3,
        // not bit 0, and the first build that read DISPSTAT saw 0x0008 at arm time
        // and 0x0009 after the run — so the claim this witness supports is the
        // behavioural one: was a second source dispatched in the same entry as an
        // audio service, yes or no. That answer is the `other source` line below.

        // The producer loop. No logging here: one `agb::println!` is a semihosting
        // trap, which on hardware is a syscall into a host that may not exist.
        let mut halt_reason: Option<&'static str> = None;
        while producer.ledger.frames < total_frames {
            if let Err(halt) = producer.fed_frame() {
                match halt {
                    Halt::Ended => {
                        agb::println!(
                            "[pp-rom] FAIL the stream ended at frame {}",
                            producer.ledger.frames
                        );
                        halt_reason = Some("stream ended early");
                    }
                    Halt::Failed(why) => halt_reason = Some(why),
                }
                break;
            }
        }
        if halt_reason.is_none() && producer.ledger.frames != total_frames {
            agb::println!(
                "[pp-rom] FAIL frame count {} does not match the manifest's {}",
                producer.ledger.frames,
                total_frames
            );
            halt_reason = Some("frame count");
        }

        // Belt and braces: the producer already sets this in the same critical
        // section as the final push (see `Producer::decode_and_push`), so a
        // short chunk is a classified tail from that instant. This covers a
        // producer that stopped without pushing (an error path) and is
        // idempotent otherwise.
        critical_section::with(|cs| STATS.borrow_ref_mut(cs).finished = true);
        producer.ledger.audio_producer_end = unsafe { audio_cycles() };
        agb::println!(
            "[pp-rom] producer finished ({} frames through the ring)",
            producer.ledger.frames
        );

        // Drain: whole periods take full chunks, the residue lands as one
        // zero-padded tail, and the ledger advances by real samples only — so
        // `consumed` reaches `pushed` exactly when the last real sample has been
        // handed to the DMA.
        producer.ledger.audio_drain_start = unsafe { audio_cycles() };
        // The drain can legitimately last the ring's whole depth: 36 periods at
        // 573,440 cycles. Bound the spin in periods (with headroom) so a wedged
        // consumer fails visibly rather than waiting for the harness timeout.
        let max_polls = (RING_SAMPLES / CHUNK + 2) as u64 * 8192;
        let mut polls = 0u64;
        let drained = loop {
            let done = critical_section::with(|cs| {
                let feed = FEED.borrow_ref(cs);
                let done = feed.consumed() >= feed.pushed();
                drop(feed);
                done
            });
            polls += 1;
            if done || polls > max_polls {
                break done;
            }
        };
        producer.ledger.audio_end = unsafe { audio_cycles() };
        agb::println!("[pp-rom] drain: {} polls", polls);
        if !drained {
            agb::println!("[pp-rom] FAIL the drain never finished");
        }

        let (pushed, consumed) = critical_section::with(|cs| {
            let feed = FEED.borrow_ref(cs);
            let pair = (feed.pushed(), feed.consumed());
            drop(feed);
            pair
        });
        let underflows = critical_section::with(|cs| STATS.borrow_ref(cs).underflows);
        agb::println!(
            "[pp-rom] feed: pushed={} expected={} consumed={} frames={}/{} underflows={}",
            pushed,
            total_samples,
            consumed,
            producer.ledger.frames,
            total_frames,
            underflows
        );
        if pushed != total_samples || consumed != pushed {
            agb::println!("[pp-rom] FAIL the sample ledger is short — the verdict cannot pass");
        }
        (pushed, consumed, underflows, halt_reason.map_or(0, |_| 1))
    }

    // ------------------------------------------------------------- the proof

    /// The correctness pass, after the audio, through the same shared fold the
    /// host witness runs (`checksum::fold_i16le_stereo`) against the pack-time
    /// pin. Not timed, for the reason `src/main.rs` records: a window that
    /// contains the fold is pricing the fold (1.95 M c/frame measured for a kernel
    /// the instrument measures at 0.42 M).
    fn proof() -> (bool, bool) {
        let Some(manifest) = manifest() else {
            return (false, false);
        };
        let Ok(mut decoder) = Decoder::new(&manifest) else {
            agb::println!("[pp-rom] FAIL proof: decoder refused");
            return (false, false);
        };
        let capacity = FrameBuffers::required_len(&manifest);
        let mut left: Vec<i32> = vec![0i32; capacity];
        let mut right: Vec<i32> = vec![0i32; capacity];
        let mut bufs = FrameBuffers::new(&mut left, &mut right);
        let mut scratch_l: Vec<u8> = vec![0u8; capacity];
        let mut scratch_r: Vec<u8> = vec![0u8; capacity];

        let mut hash = Fnv1a64::new();
        let mut fifo_l = Fnv1a64::new();
        let mut fifo_r = Fnv1a64::new();
        let mut frames = 0usize;
        let mut samples = 0u64;
        loop {
            match decode_one(&mut decoder, &mut bufs) {
                Ok(Some(frame)) => {
                    if fold_i16le_stereo(&mut hash, frame.left, frame.right).is_err() {
                        agb::println!("[pp-rom] FAIL proof: sample outside the i16 profile");
                        return (false, false);
                    }
                    let n = frame.left.len();
                    if convert_frame(
                        frame.left,
                        frame.right,
                        &mut scratch_l[..n],
                        &mut scratch_r[..n],
                    )
                    .is_err()
                    {
                        agb::println!("[pp-rom] FAIL proof: FIFO conversion rejected a sample");
                        return (false, false);
                    }
                    fold_bytes(&mut fifo_l, &mut fifo_r, &scratch_l[..n], &scratch_r[..n]);
                    frames += 1;
                    samples += frame.left.len() as u64;
                }
                Ok(None) => break,
                Err(e) => {
                    agb::println!("[pp-rom] FAIL proof decode: {:?}", e);
                    return (false, false);
                }
            }
        }
        let pcm = hash.finish();
        let pcm_ok =
            frames == gafp_pin::FRAMES && samples == gafp_pin::SAMPLES && pcm == gafp_pin::FNV_PCM;
        agb::println!(
            "[pp-rom] proof: frames={} samples={} pcm_fnv=0x{:016X} expected=0x{:016X} [{}]",
            frames,
            samples,
            pcm,
            gafp_pin::FNV_PCM,
            if pcm_ok { "MATCH" } else { "MISMATCH" }
        );
        // The played-form pin, over the same converted frames. This cannot be
        // folded on the push path: the house FNV-1a is 64-bit, ARM7TDMI has no
        // 64-bit multiply, and folding 4,096 bytes/frame there costs ~800,000
        // cycles/frame — measured, and it blew the period budget (128%). The
        // proof pass is after the audio and is not timed, so it can afford it.
        // `RingFeed::push_bytes`/`take_chunk` are the copy that carries these
        // bytes to the DMA, and they are byte-witnessed on the host.
        let fifo_l_pin = fifo_l.finish();
        let fifo_r_pin = fifo_r.finish();
        let fifo_ok = fifo_l_pin == gafp_pin::FNV_FIFO_L && fifo_r_pin == gafp_pin::FNV_FIFO_R;
        agb::println!(
            "[pp-rom] fifo proof: L=0x{:016X} R=0x{:016X} expected L=0x{:016X} R=0x{:016X} [{}]",
            fifo_l_pin,
            fifo_r_pin,
            gafp_pin::FNV_FIFO_L,
            gafp_pin::FNV_FIFO_R,
            if fifo_ok { "MATCH" } else { "MISMATCH" }
        );
        (pcm_ok, fifo_ok)
    }

    // ------------------------------------------------------------------ main

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[pp-rom] entry — product playback at the target rate (6b-3)");
        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        agb::println!(
            "[pp-rom] kernel: {}",
            if cfg!(feature = "arm-kernel") {
                "rice-arm (flac-lite-gba ArmRiceKernel)"
            } else {
                "TableCountKernel (core default)"
            }
        );
        agb::println!("[pp-rom] blob: {} bytes", SONG.len());
        agb::println!(
            "[pp-rom] geometry: chunk={} ring={}x{}={} samples prefill={}x{} dma_words={}",
            CHUNK,
            RING_CHUNKS,
            CHUNK,
            RING_SAMPLES,
            PREFILL_CHUNKS,
            CHUNK,
            DMA_WORDS
        );

        let Some(manifest) = manifest() else {
            report(&mut gfx, false, "manifest refused", None, None, None);
            loop {}
        };
        let capacity = FrameBuffers::required_len(&manifest);
        let total_frames = manifest.frame_count();
        let stream_rate = manifest.sample_rate_hz();
        let total_samples = usize::try_from(manifest.total_samples()).unwrap_or(usize::MAX);

        let mut ledger = Ledger::new(u32::from(manifest.blocksize()));
        let mut left: Vec<i32> = vec![0i32; capacity];
        let mut right: Vec<i32> = vec![0i32; capacity];
        let bufs = FrameBuffers::new(&mut left, &mut right);
        let Ok(decoder) = Decoder::new(&manifest) else {
            report(
                &mut gfx,
                false,
                "decoder refused",
                Some(&ledger),
                None,
                None,
            );
            loop {}
        };
        // The producer is consumed by `play`: it owns the only mutable borrow of
        // the ledger, and consuming it releases that borrow for the report below.
        let producer = Producer {
            decoder,
            bufs,
            scratch_l: vec![0u8; capacity],
            scratch_r: vec![0u8; capacity],
            // The declared blocksize is the maximum a frame of this stream can
            // add, which is what the ring must have room for *before* a frame is
            // decoded. See `Producer::wait_for_room`.
            blocksize_max: capacity,
            total_samples,
            ledger: &mut ledger,
        };

        // Say so on the screen before the first sample: on a stock unit this is the
        // only record a run gets, and it is also what a wedged run would show.
        text::show(
            &mut gfx,
            &format!("PLAYING {} FRAMES\nDO NOT POWER OFF", total_frames),
            COLOR_PENDING,
        );

        // The timers are handed out once for the whole audio phase: taking all
        // four is `unsafe` only because a `Mixer` would own timer0/1, and this
        // crate never constructs one — the rate ROM's reading of the same safety
        // condition, kept because it is the reason the call is sound.
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        let (pushed, consumed, underflows, failed) =
            play(producer, total_frames, total_samples, &mut timers);

        // Silence and teardown, in the order the harness settled on.
        timers.timer1.set_interrupt(false).set_enabled(false);
        timers.timer0.set_enabled(false);
        timers.timer2.set_enabled(false);
        teardown_audio();
        agb::println!(
            "[pp-rom] teardown: IE=0x{:04X} IF=0x{:04X} IME={}",
            unsafe { io_read_u16(REG_IE) },
            unsafe { io_read_u16(REG_IF) },
            unsafe { io_read_u16(REG_IME) }
        );

        let ledger_ok = underflows == 0
            && failed == 0
            && pushed == total_samples
            && consumed == pushed
            && ledger.frames == total_frames
            && pushed != 0;
        // The audit is part of the verdict, not supporting evidence: a coalesced
        // overflow is a full-buffer replay, a late service is a mid-waveform
        // splice, and a refused clock read means the instrument cannot vouch for
        // the shares. At most one tail period is expected (zero if the clip ends
        // exactly on a period boundary). `over_period` and `min_lead` stay
        // advisory: a frame over its own period is what the ring is for.
        let (audit_coalesced, audit_late, audit_tails, clock_refusals) =
            critical_section::with(|cs| {
                let s = STATS.borrow_ref(cs);
                (
                    s.coalesced,
                    s.late_servicings,
                    s.tail_periods,
                    *CLOCK_REFUSALS.borrow_ref(cs),
                )
            });
        // A `void` window is a clock read the instrument refused to price (a
        // window that wrapped, or a stage whose read was rejected). It is part of
        // the audit for the same reason: a share built from a void window is not a
        // measurement.
        let stages_void: u32 =
            ledger.stages.iter().map(|s| s.void).sum::<u32>() + ledger.paced.void;
        let audit_ok = audit_coalesced == 0
            && audit_late == 0
            && audit_tails <= 1
            && clock_refusals == 0
            && stages_void == 0;
        let (pcm_ok, fifo_ok) = proof();
        let pass = ledger_ok && audit_ok && pcm_ok && fifo_ok;

        report(
            &mut gfx,
            pass,
            "playback",
            Some(&ledger),
            Some((
                pushed, consumed, underflows, ledger_ok, audit_ok, pcm_ok, fifo_ok,
            )),
            Some((stream_rate, geo::PLAYBACK_RATE_HZ)),
        );
        gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });
        agb::eprintln!(
            "[pp-rom] verdict {} — screen {} (ledger={}, audit={}, pcm={}, fifo={})",
            if pass { "PASSED" } else { "FAILED" },
            if pass { "BLUE" } else { "RED" },
            ledger_ok,
            audit_ok,
            pcm_ok,
            fifo_ok
        );
        // Present nothing after `show` — `text`'s module docs say why, and 6a-7b's
        // hardware run is the evidence.
        loop {}
    }

    // ---------------------------------------------------------------- report

    /// The ledger, to the log; the verdict, to the screen. One function so the two
    /// channels print the same numbers.
    fn report(
        gfx: &mut agb::display::Graphics<'_>,
        pass: bool,
        stage: &str,
        ledger: Option<&Ledger>,
        ledger_counts: Option<(usize, usize, u32, bool, bool, bool, bool)>,
        rates: Option<(u32, u32)>,
    ) {
        let stats = critical_section::with(|cs| {
            let s = STATS.borrow_ref(cs);
            (
                s.serviced,
                s.underflows,
                s.tail_periods,
                s.tail_real,
                s.last_period,
                s.coalesced,
                u32::from(s.max_period_gap),
                s.late_servicings,
                s.max_delay_samples,
                s.late_fed,
                s.late_drain,
                s.late_first_period,
                s.late_worst_period,
                s.push_start_period,
                s.late_in_push,
                s.ie_latched,
                s.vblank_coentry,
                s.ie_vblank_armed,
                s.feed_cycles_sum,
                s.feed_cycles_worst,
                *CLOCK_REFUSALS.borrow_ref(cs),
            )
        });
        let (
            serviced,
            underflows,
            tail_periods,
            tail_real,
            periods_hw,
            coalesced,
            max_gap,
            late,
            max_delay,
            late_fed,
            late_drain,
            late_first_period,
            late_worst_period,
            push_start_period,
            late_in_push,
            ie_latched,
            vblank_coentry,
            ie_vblank_armed,
            feed_sum,
            feed_worst,
            clock_refusals,
        ) = stats;
        agb::println!(
            "[pp-rom] period audit: hw_periods={} serviced={} und={} coalesced={} max_gap={} late={} max_delay={} grace={} tail_periods={} tail_real={}",
            periods_hw,
            serviced,
            underflows,
            coalesced,
            max_gap,
            late,
            max_delay,
            REARM_GRACE_SAMPLES,
            tail_periods,
            tail_real
        );
        let replays = coalesced + late;
        agb::println!(
            "[pp-rom] audit verdict: replays={} of {} serviced periods; the ring absorbing a burst looks identical to the hardware losing one until this line is read",
            replays,
            serviced
        );
        // The late detail, in its own line so the explanation is not what gets
        // cut at the log printer's 256-character limit. Units, stated once here:
        // `max_delay` is in **samples** (timer1 cascades from the 512-cycle
        // sample clock, so its counter is a sample counter), one sample = 512
        // cycles = 30.5 us at this rate, the period is 560 samples and the grace
        // is 16 of them. `first_period`/`worst_period` are timer2 period counts,
        // i.e. the same index the audit above calls ground truth, and 0 with
        // late=0 means "none", not "period zero".
        agb::println!(
            "[pp-rom] late detail: fed={} drain={} first_period={} worst_period={} max_delay={} samples of {} (grace {} samples = {} cycles); an index below the producer's last period names a splice, one at or after it names only the tail",
            late_fed,
            late_drain,
            if late == 0 { 0 } else { late_first_period },
            if late == 0 { 0 } else { late_worst_period },
            max_delay,
            CHUNK,
            REARM_GRACE_SAMPLES,
            u64::from(REARM_GRACE_SAMPLES) * u64::from(TICKS_PER_SAMPLE)
        );
        let Some(ledger) = ledger else {
            screen(gfx, pass, stage, &mut String::new());
            return;
        };
        let frames = ledger.frames.max(1) as u64;

        agb::println!(
            "[pp-rom] stages, audio cycles ({} /sample, {} /period, {} /frame):",
            TICKS_PER_SAMPLE,
            PERIOD_CYCLES,
            ledger.frame_budget
        );
        for i in 0..4 {
            let s = ledger.stages[i];
            agb::println!(
                "[pp-rom]   {} n={:4} sum={:>12} mean={:>9} worst={:>9} best={:>9} boundaries_in={} ({} per hundred windows) void={}",
                ledger.stage_names[i],
                s.n,
                s.sum,
                s.mean(),
                s.worst,
                s.best,
                s.inside,
                per_window_x100(s.inside, s.n),
                s.void
            );
        }
        agb::println!(
            "[pp-rom]   feeder  n={:4} sum={:>12} mean={:>9} worst={:>9} (its own window; ~10^2 cycles of it is this instrument)",
            serviced,
            feed_sum,
            if serviced == 0 {
                0
            } else {
                feed_sum / u64::from(serviced)
            },
            feed_worst
        );
        agb::println!(
            "[pp-rom]   clock refusals (no median of three in six attempts): {}",
            clock_refusals
        );

        // Which leg held the late re-arm, read against the row above it. The push
        // stage's `boundaries_in` says how many period boundaries crossed inside the
        // chunked push sequence at all; `in_push` is the subset of the *late*
        // services whose boundary is the one the first chunk section published. A
        // boundary that crossed inside any chunk section cannot be served before
        // that section ends (IME goes back with it), so `late 1 / in_push 1` names
        // the mechanism instead of correlating with it, and `late 1 / in_push 0`
        // rules it out for that event and moves the search to another source in the
        // same entry or to the bus itself (Q15).
        agb::println!(
            "[pp-rom] which leg held it: late={} in_push={} boundaries_inside_push_section={} last_push_section_started_at_period={}",
            late,
            late_in_push,
            ledger.stages[3].inside,
            push_start_period
        );
        agb::println!(
            "[pp-rom] which leg meaning: a late service is called the push's by the period counter the first chunk section published; the whole push sequence is shorter than a {}-sample period, so a boundary crossing any chunk section makes the handler period equal that counter + 1 — exact, not a timing window",
            CHUNK
        );
        // The other candidate that could hold the same service: a second source in
        // the same interrupt entry. agb's dispatcher masks out of the enable mask
        // every source it is about to dispatch and restores it on the way out, so
        // the mask as seen from inside the audio handler says directly whether a
        // second source shared that entry — no handler need be registered for the
        // entry to have been made and paid for. The second count is the complement,
        // and it is printed because without it a zero co-entry count cannot be told
        // apart from "that source is not armed here". Which hardware request the bit
        // stands for is not claimed; the raw registers are printed at arm time and
        // here, and nothing in this build writes them, so the run brackets them.
        let ie_now = unsafe { io_read_u16(REG_IE) }; // SAFETY: fixed I/O address.
        let ime_now = unsafe { io_read_u16(REG_IME) }; // SAFETY: fixed I/O address.
        let if_now = unsafe { io_read_u16(REG_IF) }; // SAFETY: fixed I/O address.
        let dispstat = unsafe { io_read_u16(REG_DISPSTAT) }; // SAFETY: fixed I/O address.
        agb::println!(
            "[pp-rom] other source in the same entry: mask_now=0x{:04X} mask_seen_by_audio=0x{:04X} ime={} if_now=0x{:04X} dispstat_now=0x{:04X} dispstat_bit0={} audio_entries_with_second_source={} audio_entries_second_source_armed_but_absent={}",
            ie_now,
            ie_latched,
            ime_now,
            if_now,
            dispstat,
            u32::from(dispstat & (1 << 0) != 0),
            vblank_coentry,
            ie_vblank_armed
        );
        agb::println!(
            "[pp-rom] other source meaning (1/2): the mask as the audio handler sees it is what is armed minus what agb is dispatching in this entry, so bit 0 set means no second source came with this service"
        );
        agb::println!(
            "[pp-rom] other source meaning (2/2): the second count is its complement; without it a zero cannot be told from \"not armed here\". Both registers are read at arm time too, and what agb's bit 0 waits for in hardware is deliberately not claimed."
        );

        // The pacer's period, printed before the shares so the wait is understood
        // as what the flow control left idle rather than as time the producer ran
        // out of. Its mean should sit on the frame budget; that is the whole
        // design, and a mean far below it would mean the producer is free-running
        // (which the ring would then be unable to protect).
        agb::println!(
            "[pp-rom] pacer span per frame (wall time, NOT a cost): mean={} worst={} best={} against a frame budget of {}",
            ledger.paced.mean(),
            ledger.paced.worst,
            ledger.paced.best,
            ledger.frame_budget
        );

        // The shares. Work share is the real-time number; the wait is what the
        // pacer left idle and is deliberately not in it.
        //
        // Every term here is divided by the SAME denominator — the frame count —
        // because the terms have to add up to the total they are printed beside.
        // The stage rows above are per-*window* means over `n` fed-phase windows,
        // and a window mean over a frame budget is not a frame's share: 426,327 c
        // is 40.657% of a frame, but there are 152 decode windows for 157 frames,
        // so decode *contributes* 39.363% of the run's real time. The first
        // version of this line mixed the two denominators and printed four terms
        // summing to 53.441% next to a total of 53.062% (2026-11-11, corrected the
        // same day; the feeder term was worse than that, dividing a period's cost
        // by a frame's budget). Nothing measured changed — only where the
        // divisions happen.
        //
        // The denominator is the *whole run's* frame count, so this is a whole-clip
        // average: the five prefill frames are not timed, and the drain's feeder
        // periods are included. The fed-phase duty cycle is ~1.7 points higher
        // (~54.8%, ~574,700 c/frame), and that is the figure to quote as the
        // steady-state real-time load; the printed number is the run's share of
        // audio time, not the steady-state margin.
        let work_per_frame = (ledger.work_sum() + feed_sum) / frames;
        let decode_per_frame = ledger.stages[1].sum / frames;
        let convert_per_frame = ledger.stages[2].sum / frames;
        let push_per_frame = ledger.stages[3].sum / frames;
        let feed_per_frame = feed_sum / frames;
        // The four per-frame integers can truncate to a sum a few cycles under the
        // total, so the parts are printed with their own sum rather than a reader
        // being left to find the 4-cycle gap.
        let parts_per_frame =
            decode_per_frame + convert_per_frame + push_per_frame + feed_per_frame;
        agb::println!(
            "[pp-rom] budget: work/frame={} c = {}.{:03}% (per-frame shares: decode {}.{:03}% convert {}.{:03}% push {}.{:03}% feeder {}.{:03}%; parts sum to {} c, the gap is per-part integer truncation) of a {} c frame",
            work_per_frame,
            ledger.pct_x1000(work_per_frame) / 1000,
            ledger.pct_x1000(work_per_frame) % 1000,
            ledger.pct_x1000(decode_per_frame) / 1000,
            ledger.pct_x1000(decode_per_frame) % 1000,
            ledger.pct_x1000(convert_per_frame) / 1000,
            ledger.pct_x1000(convert_per_frame) % 1000,
            ledger.pct_x1000(push_per_frame) / 1000,
            ledger.pct_x1000(push_per_frame) % 1000,
            ledger.pct_x1000(feed_per_frame) / 1000,
            ledger.pct_x1000(feed_per_frame) % 1000,
            parts_per_frame,
            ledger.frame_budget
        );
        agb::println!(
            "[pp-rom] worst single frame's WORK: {} c = {}.{:03}% of a frame; frames whose work exceeded their own period: {}",
            ledger.max_work,
            ledger.pct_x1000(u64::from(ledger.max_work)) / 1000,
            ledger.pct_x1000(u64::from(ledger.max_work)) % 1000,
            ledger.over_period
        );

        // The attribution the design promised: a fed-phase window is system cost,
        // so subtract the period boundaries it contains at the measured feeder cost
        // to get an estimate of what the same stage would cost with no audio
        // running. It is an estimate — the interrupt's entry cost and any nesting
        // are not separable here — but it is the only bridge between this image's
        // numbers and the isolated instrument's, so it is printed rather than left
        // to a reader at home. The pinned row it is compared against is written
        // down so the comparison is checkable from the log alone.
        let feed_mean = if serviced == 0 {
            0
        } else {
            feed_sum / u64::from(serviced)
        };
        // One formula, shared by the per-stage lines below, the decode cross-check
        // and the screen's `attr` field: three copies of the same arithmetic are
        // three chances to report three different numbers for one quantity, and the
        // screen field exists precisely so that a hardware run can be read without
        // the log.
        let isolated = |st: Stage| -> u64 {
            st.mean()
                .saturating_sub((u64::from(st.inside) * feed_mean) / u64::from(st.n.max(1)))
        };
        for i in 1..4 {
            let st = ledger.stages[i];
            agb::println!(
                "[pp-rom] attribution: {} system_mean={} boundaries={} isolated_estimate={}",
                ledger.stage_names[i].trim_end(),
                st.mean(),
                st.inside,
                isolated(st)
            );
        }
        // Three lines rather than one: the ROM's log printer cuts a line at 256
        // characters, and the part of the sentence that explains what the numbers
        // mean is exactly what was being cut off the end.
        agb::println!(
            "[pp-rom] decode cross-check: this image's isolated estimate {} c/frame vs the pinned {} mean / {} worst (flac-docs/measurements.md 6b-2 pt 2, hardware)",
            isolated(ledger.stages[1]),
            PINNED_DECODE_MEAN,
            PINNED_DECODE_WORST
        );
        agb::println!(
            "[pp-rom] decode cross-check meaning: the gap is what this image's decode costs with the ring and the interrupt in it, not a contradiction of that row"
        );
        agb::println!(
            "[pp-rom] decode cross-check provenance: that row was measured on the pre-7-0 32,000 Hz-era frames — same shape, different bytes; 7-1 owns rows measured on the shipped asset"
        );
        // An unset watermark prints as 0, not as usize::MAX: a run that never
        // reached a push is a failure for other reasons, and a screen full of
        // 4294967295 hides that.
        let min_lead = if ledger.min_lead == usize::MAX {
            0
        } else {
            ledger.min_lead
        };
        agb::println!(
            "[pp-rom] lead (samples): min={} max={} ring holds={} a period needs={} margin over one period={}",
            min_lead,
            ledger.max_lead,
            RING_SAMPLES,
            CHUNK,
            min_lead.saturating_sub(CHUNK)
        );

        // The closure identity. Anything that is not one of the four named terms
        // is the loop's own bookkeeping, and it is printed rather than hidden.
        // The closure. The terms are DISJOINT by construction — stage windows
        // abut, the drain follows the producer, and what is left between them is
        // this loop's own bookkeeping, which is what `gaps` measures.
        //
        // The feeder is deliberately NOT in that sum: an interrupt runs inside
        // whichever window it interrupts, so adding it would double-count (the
        // first version of this line did, and read as a negative residual — which
        // is exactly the kind of thing an identity is for). It is reported as the
        // nested term it is, and `boundaries_in` above is what prices it.
        let audio_span = u64::from(ledger.audio_end.wrapping_sub(ledger.audio_start));
        let drain_tail = u64::from(ledger.audio_end.wrapping_sub(ledger.audio_drain_start));
        let stages = ledger.stages_sum();
        let gaps = audio_span as i64 - stages as i64 - drain_tail as i64;
        let read_pairs = (u64::from(ledger.paced.n) * 5 + u64::from(serviced) * 2 + 8) * 3;
        agb::println!(
            "[pp-rom] closure (disjoint): audio_span={} stages={} drain_spin={} gaps={} — gaps are the {} clock read-pairs between windows plus the pre-arm span and the loop's bookkeeping; read-pairs alone bound at ~{} cycles (three snapshots per read), and the median is biased low by about half a read sequence, so a small nonzero value is the instrument, not a lost term",
            audio_span,
            stages,
            drain_tail,
            gaps,
            read_pairs,
            read_pairs * 60
        );
        agb::println!(
            "[pp-rom] closure (nested, not additive): feeder={} of which the {} boundaries inside producer stages fall inside stage windows at ~{} c each",
            feed_sum,
            ledger
                .stages
                .iter()
                .map(|st| u64::from(st.inside))
                .sum::<u64>(),
            feed_mean
        );
        agb::println!(
            "[pp-rom] audio time: producer span={} drain span={} (producer span ends when the last frame is pushed; the drain is the consumer finishing what was already queued)",
            u64::from(ledger.audio_producer_end.wrapping_sub(ledger.audio_start)),
            drain_tail
        );

        // The screen. Kept short on purpose: `text::measure` wraps at 236 px and
        // reports the extent, and the fit line below is the witness that the
        // verdict a hardware run reads is actually legible.
        // Line budget, stated rather than discovered: eleven lines at the font this
        // module carries measured `bottom=135` for eleven, i.e. ~12.3 px a line, so
        // the shape below — ten content lines plus the final `stage` word — measures
        // the same eleven and leaves two lines of margin for the conditional
        // `rate …!… pitch` line below. Longest line here is 38 characters (~210 px),
        // inside the 236 px wrap. If a future row breaks either bound, the `fit:`
        // line says so in the log and on the emulator — trim here rather than
        // discovering it on a unit.
        let stages_void: u32 =
            ledger.stages.iter().map(|s| s.void).sum::<u32>() + ledger.paced.void;
        // Unset period indices print as 0, as the unset lead watermark above
        // already does: a column of 4294967295 on a unit where nothing was late
        // would read as a period number.
        let late_first = if late == 0 { 0 } else { late_first_period };
        let late_worst = if late == 0 { 0 } else { late_worst_period };
        let mut screen_text = String::new();
        screen_text.push_str("PRODUCT PLAY 16384\n");
        if let Some((pushed, consumed, underflows, ledger_ok, audit_ok, pcm_ok, fifo_ok)) =
            ledger_counts
        {
            screen_text.push_str(&format!(
                "fr {} und {} pcm {} fifo {}\n",
                ledger.frames,
                underflows,
                if pcm_ok { "ok" } else { "NO" },
                if fifo_ok { "ok" } else { "NO" }
            ));
            // `ledg`/`aud` share a line with the sample counts: the two verdicts
            // and the two totals are the same kind of thing (what the run claims to
            // have moved), and merging them buys back the line the attribution and
            // interrupt witness below need. Measured rather than assumed: the `fit:`
            // line prints the result, and the longest merged line here is 38
            // characters, which `text::measure` reports inside the 236 px wrap.
            screen_text.push_str(&format!(
                "ledg {} aud {} push {} cons {}\n",
                if ledger_ok { "ok" } else { "NO" },
                if audit_ok { "ok" } else { "NO" },
                pushed,
                consumed
            ));
            // `attr` is decode's attributed figure — the same number the
            // `attribution:` log line prints, from the same shared formula — and it
            // is on the screen because it is the figure the host model's cost
            // envelope and the packer's row are stated in, while `dec` beside it is
            // the system figure with period boundaries nested inside it. The two are
            // about 1% apart and reading the wrong one is the error 6b-3 refused to
            // paper over; they are printed side by side so the difference is visible
            // rather than something a reader has to know to look for.
            screen_text.push_str(&format!(
                "work {}.{:03}% dec {}.{:03}% attr {}\n",
                ledger.pct_x1000(work_per_frame) / 1000,
                ledger.pct_x1000(work_per_frame) % 1000,
                ledger.pct_x1000(decode_per_frame) / 1000,
                ledger.pct_x1000(decode_per_frame) % 1000,
                isolated(ledger.stages[1])
            ));
            // The other three per-frame shares. They join the screen because they
            // are the numbers item 7-1 exists to read off a unit: the host model's
            // cost envelope and the packer's `fixed0 @ b2048` row are both stated
            // in per-stage terms, and a run that prints only `work` cannot
            // separate the decode leg from the producer legs — which is exactly the
            // distinction the ~80.9% paper figure got wrong. Note these are the
            // *system* means with period boundaries nested inside them; the
            // attribution that prices those boundaries out is the `attribution:`
            // log line, and the difference is ~1%, not a rounding.
            screen_text.push_str(&format!(
                "conv {}.{:03}% push {}.{:03}% feed {}.{:03}%\n",
                ledger.pct_x1000(convert_per_frame) / 1000,
                ledger.pct_x1000(convert_per_frame) % 1000,
                ledger.pct_x1000(push_per_frame) / 1000,
                ledger.pct_x1000(push_per_frame) % 1000,
                ledger.pct_x1000(feed_per_frame) / 1000,
                ledger.pct_x1000(feed_per_frame) % 1000
            ));
            screen_text.push_str(&format!(
                "minlead {} maxwk {}.{:03}%\n",
                min_lead,
                ledger.pct_x1000(u64::from(ledger.max_work)) / 1000,
                ledger.pct_x1000(u64::from(ledger.max_work)) % 1000
            ));
            // The audit, not only its verdict. A screen that prints `aud NO` says
            // a clause refused; a hardware run cannot ask which one afterwards, so
            // every clause the verdict reads is printed here. `max_gap` is
            // deliberately absent: a gap > 1 always moves `replay` by gap−1, so
            // `replay 0` already states that no gap exceeded 1, and the line
            // budget goes to the indices instead. `tail p/r` is the one allowed
            // tail period and the real samples in it — the shipped clip ends 224
            // samples into a 560-sample period, so `tail 1/224` is the expected
            // reading, not a fault.
            screen_text.push_str(&format!(
                "per {} ser {} tail {}/{} void {}\n",
                periods_hw, serviced, tail_periods, tail_real, stages_void
            ));
            screen_text.push_str(&format!(
                "replay {} late {} fed {} drain {}\n",
                coalesced, late, late_fed, late_drain
            ));
            // Where, not just how many: the period indices are timer2's counts, and
            // `dly` is the worst lateness in samples (of 560; the grace is 16, and
            // a sample is 1,024 cycles). `refus` is the clock-refusal count, the
            // other clause the audit reads and the only one that can make the
            // shares themselves unreadable rather than merely late.
            screen_text.push_str(&format!(
                "latep {} worstp {} dly {} refus {}\n",
                late_first, late_worst, max_delay, clock_refusals
            ));
            // The two mechanism answers, in the two forms that decide 7-2 rather
            // than describe it. `in push a/b`: of the b late services, a of them had
            // their period boundary inside the producer's chunked ring-push
            // sequence — each section is now ~4,000 c (~3.9 samples) against this
            // audit's 16-sample grace — a mechanism, not a correlation, and a zero
            // says go look elsewhere. `2nd src a/b`: of the b audio
            // services, in a of them agb dispatched a second source in the same
            // interrupt entry (the other way a re-arm can be held past its window),
            // and `disp 0x…` is the raw display-status register, printed rather than
            // interpreted, because the meaning of the other source's bit belongs to
            // agb's numbering and is argued in the log, not on a unit.
            screen_text.push_str(&format!(
                "late in push {}/{} 2nd src {}/{} disp {:04X}\n",
                late_in_push, late, vblank_coentry, serviced, dispstat
            ));
            if let Some((stream, played)) = rates {
                if stream != played {
                    // The Q14 defect on the screen, in 24 columns: the run played a
                    // stream at a rate it does not declare. Nothing else in the
                    // verdict can see this — the digests match either way — so it has
                    // to be visible where a person will look.
                    screen_text.push_str(&format!("rate {}!{} pitch\n", stream, played));
                }
            }
        }
        screen_text.push_str(stage);
        screen(gfx, pass, stage, &mut screen_text);
    }

    /// Put the verdict text on the screen and log it, so the two channels carry
    /// the same strings.
    fn screen(
        gfx: &mut agb::display::Graphics<'_>,
        pass: bool,
        stage: &str,
        text_body: &mut String,
    ) {
        if text_body.is_empty() {
            text_body.push_str("PRODUCT PLAY 16384\n");
            text_body.push_str(stage);
        }
        for line in text_body.lines() {
            agb::println!("[pp-rom] screen| {}", line);
        }
        let fit = crate::text::measure(text_body);
        agb::println!(
            "[pp-rom] fit: lines={} right={} bottom={} fits={}",
            fit.lines,
            fit.right,
            fit.bottom,
            fit.fits()
        );
        crate::text::show(gfx, text_body, if pass { COLOR_PASS } else { COLOR_FAIL });
    }
}

/// Host-build stub (the same dual-target pattern the other ROM files use, so a
/// host `cargo test` of this crate can build the bin at all — the gate builds the
/// lib and the named test target instead; see the `product-play-test` target in
/// the Makefile).
#[cfg(not(target_arch = "arm"))]
fn main() {}
