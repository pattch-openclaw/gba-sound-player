//! # gba-audio-hw — the arm-only GBA sound hardware bootstrap
//!
//! Every register the shipped playback path writes, plus the three routines that
//! write them in an order that has been on hardware: sound enable, the
//! sound-FIFO DMA re-arm, and the timer-chain arming that feeds it. Nothing here
//! holds state.
//!
//! ## Why this crate exists
//!
//! ALBUM-PLAYER.md §2.2 / D4: the application may not `#[path]`-include a spike
//! or a probe. The same plumbing sits in three places today —
//! `examples/flac_integration/src/product_play_main.rs` (the shipped path),
//! `examples/flac_spike/src/rate_play_main.rs` (a spike, so excluded by §2.2) and
//! `examples/album_probe/src/probe_hw.rs` (a Phase 0 probe, synth/statics-bound
//! and `#[path]`-including `e5_fetch/src/timer.rs`) — and the Album Player may
//! reach into none of them. It reaches here instead.
//!
//! ## What it was transcribed from
//!
//! `examples/flac_integration/src/product_play_main.rs`, cited by line in each
//! item's docs: the register map (`:262-330`), the I/O helpers (`:526-542`),
//! [`enable_sound`] (`:646-661`), [`teardown_audio`] (`:663-670`),
//! [`rearm_dma`] (`:681-702`), [`arm_audio_chain`] (`:704-732`) and
//! [`clear_timer1_if`] (`:1557`). That file is the shipped playback path: its
//! ordering has been on hardware, and the failure the order avoids (a stale
//! Timer1 `IF` entering the feeder before the chain has a period to serve) is
//! invisible in a build. This crate re-derives none of it.
//!
//! ## Parameterised, not pinned
//!
//! The source ROM hard-codes its FLAC geometry. This crate knows no rate and no
//! chunk size: the two hardware-visible figures arrive in an [`AudioChain`] at
//! every entry point, so a 32,768 Hz / 512-sample arm is the same code (§4.2b).
//! `REARM_GRACE_SAMPLES` is deliberately **not** here — it is hardware-fixed but
//! the re-arm decision is the feeder's, so it stays the caller's constant.
//!
//! ## No state and no critical section
//!
//! The crate owns no statics, no interrupt masking and no ping-pong slot index.
//! The caller owns the buffer, the slot and the critical section around both, and
//! passes what [`rearm_dma`] needs as arguments — which is also what keeps this
//! crate free of the feeder that decides *when* a re-arm is due.
//!
//! ## MIGRATION (a later PR, deliberately not this one)
//!
//! A follow-up PR moves `product_play_main.rs`, `rate_play_main.rs` and
//! `probe_hw.rs` onto this crate, witnessed by `make flac-check`,
//! `product-play-test` and `album-probe-test`. This PR does **not** do that
//! migration: it would re-baseline two accepted product ROM images, and §2.2's
//! own reasoning is that a re-baseline belongs in a PR that says so. So for now
//! this crate is a fourth copy of the plumbing, on purpose (recorded deviation,
//! D-A).

#![no_std]
#![warn(missing_docs)]
#![warn(unsafe_op_in_unsafe_fn)]

use agb::timer::{AllTimers, Divider};

/// The GBA sound-chain register map, transcribed from
/// `product_play_main.rs:262-330`.
///
/// These are private registers, named because agb 0.25.0 gives an application no
/// other way to drive the sound FIFO itself. Channel *n*'s DMA block starts at
/// `0x0400_00B0 + 0x0C·n`, so DMA1 is at `0xBC` — not `0xB0`, which is DMA0, for
/// which sound-FIFO start timing is illegal. The pairing is fixed: DMA1 → FIFO A
/// (left), DMA2 → FIFO B (right). A first draft of the rate ROM shifted both
/// blocks one channel low; mGBA discarded the DMA0 scheduling (left silent) and
/// half-tolerated the mis-pairing (right glitchy) — headless runs cannot see
/// that, ears did (2026-10-19).
pub mod regs {
    /// DMA1 source (DMA1 → FIFO A, left).
    pub const REG_DMA1_SOURCE: usize = 0x0400_00BC;
    /// DMA1 destination (writing `FIFO_A` here selects sound-FIFO timing).
    pub const REG_DMA1_DEST: usize = 0x0400_00C0;
    /// DMA1 word count.
    pub const REG_DMA1_COUNT: usize = 0x0400_00C4;
    /// DMA1 control.
    pub const REG_DMA1_CONTROL: usize = 0x0400_00C6;
    /// DMA2 source (DMA2 → FIFO B, right).
    pub const REG_DMA2_SOURCE: usize = 0x0400_00C8;
    /// DMA2 destination.
    pub const REG_DMA2_DEST: usize = 0x0400_00CC;
    /// DMA2 word count.
    pub const REG_DMA2_COUNT: usize = 0x0400_00D0;
    /// DMA2 control.
    pub const REG_DMA2_CONTROL: usize = 0x0400_00D2;
    /// Sound FIFO A (left). Also what a DMA destination write targets.
    pub const FIFO_A: usize = 0x0400_00A0;
    /// Sound FIFO B (right).
    pub const FIFO_B: usize = 0x0400_00A4;
    /// SOUNDCNT_H: FIFO routing, volume and reset.
    pub const SOUNDCNT_H: usize = 0x0400_0082;
    /// SOUNDCNT_X: master sound enable.
    pub const SOUNDCNT_X: usize = 0x0400_0084;
    /// SOUNDBIAS: sample resolution and rate.
    pub const SOUNDBIAS: usize = 0x0400_0088;
    /// Interrupt enable (the mask agb maintains).
    pub const REG_IE: usize = 0x0400_0200;
    /// Interrupt request flags. Writing a 1 to a bit clears it.
    pub const REG_IF: usize = 0x0400_0202;
    /// Master interrupt enable.
    pub const REG_IME: usize = 0x0400_0208;
    /// DISPSTAT, **not** an interrupt register: bit 0 is the hardware's
    /// "generate an interrupt on VBlank" enable. `IE`'s bit 0 only says a VBlank
    /// request would be waited for; whether one is ever *raised* is decided here.
    pub const REG_DISPSTAT: usize = 0x0400_0004;
    /// Timer data registers live at `0x0400_0100 + 4·n` (agb `timer.rs`'s own
    /// arithmetic). The audio chain uses timer0/1/2.
    pub const REG_TM0_DATA: usize = 0x0400_0100;
    /// Timer1 counter — the chunk index inside a period.
    pub const REG_TM1_DATA: usize = 0x0400_0104;
    /// Timer2 counter — the period witness.
    pub const REG_TM2_DATA: usize = 0x0400_0108;

    /// agb `hw.rs`'s sound-DMA control word: destination fixed (`2<<5`) | repeat
    /// (`1<<9`) | transfer words (`1<<10`) | sound-FIFO start timing (`3<<12`) |
    /// enable (`1<<15`). `product_play_main.rs:320`.
    pub const DMA_SOUND_CONTROL: u16 = (2 << 5) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 15);

    /// Hardware `IE`/`IF` bit for the Timer1 overflow that runs the feeder
    /// (GBA's own numbering, which is not agb's `Interrupt` numbering).
    pub const IF_TIMER1: u16 = 1 << 4;
}

/// The figures one playback geometry puts on the wire, in the units the hardware
/// takes them: timer0's latch in CPU ticks per sample, timer1's latch and the
/// DMA count in samples.
///
/// 1,024 ticks / 560 samples at 16,384 Hz; 512 samples at 32,768 Hz (§4.2b — the
/// rate is the caller's choice, so no field here has a default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioChain {
    /// CPU cycles per sample: timer0's overflow amount.
    pub ticks_per_sample: u16,
    /// Samples per feeder period: timer1's overflow amount, and the length of
    /// one ping-pong slot.
    pub chunk_samples: u16,
    /// Words per slot: what the sound DMA is told to transfer. Always
    /// `chunk_samples / 4` — the FIFO pulls 4 words per request.
    pub dma_words: u16,
}

impl AudioChain {
    /// Build a chain from the two figures a caller actually knows; `dma_words`
    /// is derived, so it cannot disagree with the slot length.
    ///
    /// # Panics
    /// When `chunk_samples` is not a multiple of 4: the sound DMA transfers
    /// whole words, so a slot that is not word-aligned in samples would
    /// truncate. In a `const` construction that is a compile error.
    #[must_use]
    pub const fn new(ticks_per_sample: u16, chunk_samples: u16) -> Self {
        assert!(
            chunk_samples % 4 == 0,
            "chunk_samples must be a multiple of 4 (the sound DMA transfers words)"
        );
        Self {
            ticks_per_sample,
            chunk_samples,
            dma_words: chunk_samples / 4,
        }
    }
}

/// # Safety
/// `addr` must be a readable, even address in the I/O region; callers pass the
/// [`regs`] constants.
pub unsafe fn io_read_u16(addr: usize) -> u16 {
    // SAFETY: the caller guarantees the address, alignment and region.
    unsafe { (addr as *const u16).read_volatile() }
}

/// # Safety
/// As [`io_read_u16`], write flavour.
pub unsafe fn io_write_u16(addr: usize, value: u16) {
    // SAFETY: the caller guarantees the address, alignment and region.
    unsafe { (addr as *mut u16).write_volatile(value) }
}

/// # Safety
/// As [`io_read_u16`], for the 32-bit DMA address registers.
pub unsafe fn io_write_u32(addr: usize, value: u32) {
    // SAFETY: the caller guarantees the address, alignment and region.
    unsafe { (addr as *mut u32).write_volatile(value) }
}

/// Sound hardware on: FIFO A → left at 100 %, FIFO B → right at 100 %, both
/// FIFOs reset, master enable, bias at 8-bit/65,536 Hz.
///
/// `product_play_main.rs:646-661`: agb `hw.rs`'s enable sequence, transcribed
/// because those functions are `pub(super)`.
pub fn enable_sound() {
    let sound_control: u16 = (1 << 2) // sound A volume 100%
        | (1 << 9) // A to LEFT out
        | (1 << 11) // A FIFO reset
        | (1 << 3) // sound B volume 100%
        | (1 << 12) // B to RIGHT out
        | (1 << 15); // B FIFO reset
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(regs::SOUNDCNT_H, sound_control);
        io_write_u16(regs::SOUNDCNT_X, 1 << 7);
        io_write_u16(regs::SOUNDBIAS, io_read_u16(regs::SOUNDBIAS) | (1 << 14));
    }
}

/// Stop both sound DMAs and the sound hardware.
///
/// `product_play_main.rs:663-670`: the teardown the harness learned to do —
/// leaving the FIFOs enabled hangs mGBA's exit. The timer chain is not touched
/// here: the caller still holds its `AllTimers` handle at teardown and disarms
/// timer1/0/2 through it first, in the order the shipped ROM settled on.
pub fn teardown_audio() {
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(regs::REG_DMA1_CONTROL, 0);
        io_write_u16(regs::REG_DMA2_CONTROL, 0);
        io_write_u16(regs::SOUNDCNT_X, 0);
    }
}

/// Re-arm both sound DMAs from the slot just filled: stop, source, destination,
/// count, go.
///
/// `product_play_main.rs:681-702`. agb's `enable_dma_for_sound` (`hw.rs:56-68`)
/// writes source, destination and control and no count at all — in sound-FIFO
/// mode the FIFO pulls 4 words per request — so [`AudioChain::dma_words`] is
/// ours, deliberately, and it is the shape the rate ROM ran with on hardware.
///
/// The ping-pong slot index stays the caller's: this takes the two pointers it
/// is handed and holds neither.
///
/// # Safety
/// `lsrc`/`rsrc` must be 4-byte-aligned pointers into the caller's static
/// ping-pong, alive and already filled for the whole transfer — the DMA reads
/// them with no further reference from Rust.
pub unsafe fn rearm_dma(lsrc: *const u8, rsrc: *const u8, chain: &AudioChain) {
    // SAFETY: fixed I/O addresses, and the caller's aligned static pointers.
    unsafe {
        io_write_u16(regs::REG_DMA1_CONTROL, 0);
        io_write_u32(regs::REG_DMA1_SOURCE, lsrc as u32);
        io_write_u32(regs::REG_DMA1_DEST, regs::FIFO_A as u32);
        io_write_u16(regs::REG_DMA1_COUNT, chain.dma_words);
        io_write_u16(regs::REG_DMA1_CONTROL, regs::DMA_SOUND_CONTROL);

        io_write_u16(regs::REG_DMA2_CONTROL, 0);
        io_write_u32(regs::REG_DMA2_SOURCE, rsrc as u32);
        io_write_u32(regs::REG_DMA2_DEST, regs::FIFO_B as u32);
        io_write_u16(regs::REG_DMA2_COUNT, chain.dma_words);
        io_write_u16(regs::REG_DMA2_CONTROL, regs::DMA_SOUND_CONTROL);
    }
}

/// Program the audio timebase from `chain` and start it.
///
/// `product_play_main.rs:704-732`. Timer0 counts CPU cycles at /1 to
/// `ticks_per_sample` and cascades into timer1, which overflows every
/// `chunk_samples` and raises its interrupt — one period per sound-DMA refill.
/// Timer2 is a cascaded period witness (this crate never reads it; the audit
/// that counts periods does); timer3 is cleared so a stale latch from an
/// earlier boot pass cannot confuse that audit.
///
/// The chain is stopped with its counters written to their latches first, so
/// audio time and the caller's measurement clock start at the same instant, and
/// timer2 is armed *before* timer1 can overflow so the counter that judges every
/// period misses nothing (`product_play_main.rs:722-731`).
///
/// The stale Timer1 `IF` is **not** cleared here: the caller clears it before
/// registering its handler and then calls this (see [`clear_timer1_if`]).
pub fn arm_audio_chain(timers: &mut AllTimers<'_>, chain: &AudioChain) {
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
        .set_overflow_amount(chain.ticks_per_sample);
    timers
        .timer1
        .set_cascade(true)
        .set_interrupt(true)
        .set_overflow_amount(chain.chunk_samples);
    // Nothing in the shipped build uses timer3; leaving a stale latch in it from
    // an earlier boot pass would only confuse the audit.
    timers.timer3.set_cascade(false).set_interrupt(false);
    timers.timer2.set_enabled(true);
    timers.timer0.set_enabled(true);
    timers.timer1.set_enabled(true);
}

/// Clear a stale Timer1 `IF` before the feeder's source is enabled.
///
/// `product_play_main.rs:1551-1558`: registering the handler sets the `IE` bit,
/// and a pending bit left by the boot path would enter the feeder before the
/// chain has a period to serve. Writing a 1 to the `IF` bit clears it. The first
/// real overflow is a whole chunk away, so installing the handler before arming
/// loses no period — this runs between the two.
pub fn clear_timer1_if() {
    // SAFETY: fixed I/O address; write-1-to-clear.
    unsafe { io_write_u16(regs::REG_IF, regs::IF_TIMER1) };
}
