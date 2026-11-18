//! PR E's **gate total**: decode + convert + IRQ + DMA, in one measured frame.
//!
//! The decode wall (`prod8`) is what the plan of record has tracked to 83.6% of
//! the 262,144-cycle period. U4 is not the decode wall: it is the *gate total*,
//! and its ceiling has to be **measured, not assigned** — so this arm puts the
//! real output stage on the same image and the same frames.
//!
//! What runs here, and why it is the shipping shape rather than a stand-in:
//!
//! * **The product decoder, int8 output** (`GbaDecoder`, the C-3d fused path) —
//!   the arm ALBUM's product number already comes from.
//! * **The shipping feeder's ring** (`gba_playback::playback::RingFeed`), the
//!   same type `crates/gba-playback` ships to the FLAC product path and the
//!   album player. ULI2 writes int8 DAC bytes directly, so the "convert" ULI2
//!   pays is a byte copy into the ring — not the FLAC path's `>> 8` PCM16
//!   conversion. That difference is the reason this had to be measured.
//! * **The hardware timebase**: timer0 = one sample (512 ticks at 16.78 MHz =
//!   32,768 Hz), timer1 cascaded = one feeder period (**512 samples = one ULI2
//!   frame**, 64 periods/s) with its overflow as the feeder interrupt, the sound
//!   FIFOs on DMA1/DMA2 in repeat mode from a two-slot ping-pong, `SOUNDCNT_H`
//!   at 100%/100% and `SOUNDBIAS` in 8-bit mode. Same register recipe as
//!   `examples/flac_integration/src/product_play_main.rs` (which carries the
//!   hardware findings: DMA1→FIFO A/DMA2→FIFO B is fixed, and a mid-period
//!   re-arm splices the waveform).
//!
//! Measurement discipline:
//!
//! * The window is `decode → convert → push`, with **interrupts live**, so the
//!   feeder's IRQ and the sound DMA are inside it. That is what makes this a
//!   gate total: the FLAC ledger's three-stage split would be a different
//!   instrument and a different image.
//! * The producer's *wait for ring space* is **outside** the window. A real
//!   player is paced by the feeder; charging the pacing to the frame would
//!   measure the hardware clock, not the work.
//! * The feeder's ledger is reported, not judged: `full_takes`, `starved_entries`
//!   and `consumed + RING == pushed` are printed together, because the first two
//!   are entry counts and only the third says whether a sample was ever lost.
//!   The run on 2026-11-15 closed the ledger exactly with the ring full at the
//!   end and reported 772 starved *entries* — an instrument question recorded in
//!   the plan of record for PR D, deliberately not claimed as underruns.
//!
//! Caveats that travel with the number (also in the run log):
//! * mGBA, not hardware. It models neither the cart prefetch buffer nor bus
//!   arbitration between DMA and code fetch, so this errs *optimistic* about the
//!   DMA's interference (the FLAC ledger's R3 row).
//! * The empty-window overhead subtracted is the bench's IRQ-off calibration
//!   (35 cycles); an IRQ landing inside a calibration window would have
//!   inflated it, and the calibration reports its spread so that is visible.

use core::cell::RefCell;

use agb::external::critical_section::{self, Mutex};
use agb::interrupt::{Interrupt, add_interrupt_handler};
use agb::timer::{Divider, Timer};
use gba_playback::playback::{Chunk, RingFeed};
use uli_lite::format::M;
use uli_lite::{Album, CodeTables};
use uli_lite_gba::{ClassTables, GbaDecoder, HotState};

use super::{Counter, FrameCost, timed_irq};
#[cfg(feature = "gate-hash")]
use super::{fnv_bytes, fnv_i8};

/// 16,777,216 Hz / 32,768 Hz: one DAC sample per 512 CPU cycles.
const TICKS_PER_SAMPLE: u32 = 512;
/// One feeder period: the ULI2 frame, so the feeder interrupt and the frame are
/// the same tick (64 per second) and there is no cross-rate arithmetic.
const PERIOD_SAMPLES: usize = M;
/// Ring depth in samples: 16 frames (250 ms). Deep enough that a burst cannot
/// starve it, shallow enough that the measurement is not hiding a slow producer
/// behind a long queue.
const RING_SAMPLES: usize = 16 * M;
/// Frames decoded before the clock is armed. The prefill is outside every
/// window; it exists so the first period is not born starved.
const PREFILL_FRAMES: u32 = 8;
/// One FIFO's transfer count in words: `PERIOD_SAMPLES` 8-bit samples.
const DMA_WORDS: u16 = (PERIOD_SAMPLES / 4) as u16;
/// Seconds' worth of periods per second, for the log only.
const PERIODS_PER_SEC: u32 = 32_768 / PERIOD_SAMPLES as u32;

// ------------------------------------------------------------------ registers
//
// Named here because `agb` 0.25.0 gives an application no other way to drive the
// sound FIFO itself; the addresses and the pairing are `product_play_main.rs`'s.
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
const REG_IF: usize = 0x0400_0202;

/// agb `hw.rs`'s sound-DMA control word: destination fixed (2<<5) | repeat
/// (1<<9) | transfer words (1<<10) | sound-FIFO start timing (3<<12) | enable.
const DMA_SOUND_CONTROL: u16 = (2 << 5) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 15);

unsafe fn io_write_u16(addr: usize, v: u16) {
    // SAFETY: fixed MMIO address, 16-bit access as documented.
    unsafe { core::ptr::write_volatile(addr as *mut u16, v) };
}

unsafe fn io_write_u32(addr: usize, v: u32) {
    // SAFETY: as above, 32-bit access.
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) };
}

unsafe fn io_read_u16(addr: usize) -> u16 {
    // SAFETY: as above.
    unsafe { core::ptr::read_volatile(addr as *const u16) }
}

// ------------------------------------------------------------------- statics

/// The two DMA slots, one sample run per channel per slot. `repr(align(4))`
/// because a DMA source must be word-aligned; each `[u8; M]` is M-byte aligned
/// from offset 0 (M = 512), so all four arrays are aligned too.
#[repr(align(4))]
struct SndBufs {
    l: [[u8; PERIOD_SAMPLES]; 2],
    r: [[u8; PERIOD_SAMPLES]; 2],
    /// Which slot the next period fills.
    next: usize,
}

impl SndBufs {
    const fn new() -> Self {
        Self {
            l: [[0u8; PERIOD_SAMPLES]; 2],
            r: [[0u8; PERIOD_SAMPLES]; 2],
            next: 0,
        }
    }
}

/// The feeder's ledger. Plain `u32`s written only inside the handler's critical
/// section (thumbv4t has no atomics; the crate's contract is the critical
/// section, not atomics).
struct SndStats {
    serviced: u32,
    /// Entries that took a whole period from the ring.
    full_takes: u32,
    /// Entries that found **less than one period** ready. Not the same thing as
    /// a missed period: the sample ledger (`consumed + RING == pushed`) is what
    /// says whether the ring ever really ran dry, and the arm prints both.
    starved_entries: u32,
}

static FEED: Mutex<RefCell<RingFeed<RING_SAMPLES>>> = Mutex::new(RefCell::new(RingFeed::new()));
static BUFS: Mutex<RefCell<SndBufs>> = Mutex::new(RefCell::new(SndBufs::new()));
static STATS: Mutex<RefCell<SndStats>> = Mutex::new(RefCell::new(SndStats {
    serviced: 0,
    full_takes: 0,
    starved_entries: 0,
}));

/// The frame the producer is on, so a starved period can name itself. Written
/// once per frame outside the window (one store); read only when a period
/// starves, which is the event under investigation.
static CURRENT_FRAME: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(0));
/// The first starved period's frame index (`u32::MAX` = none yet).
static FIRST_STARVE: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(u32::MAX));

// ------------------------------------------------------------------- handler

/// Re-arm both sound DMAs from the slot just filled: stop, source, destination,
/// count, go. The count is ours because `agb`'s sound-DMA setup writes none, and
/// in sound-FIFO mode the FIFO pulls 4 words per request.
unsafe fn rearm_dma(lsrc: *const u8, rsrc: *const u8) {
    // SAFETY: fixed MMIO addresses; the caller's pointers are word-aligned
    // arrays in a `static` that outlives every transfer.
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

/// The feeder interrupt body: one period, from the ring into the off-screen
/// slot, then hand the DMA that slot. Allocates nothing, logs nothing — agb's
/// interrupt-handler contract, which is why the ring is a static and the token
/// is the one agb hands the handler (the race gate, exactly as the product-play
/// ROM's feeder documents).
fn feeder(cs: critical_section::CriticalSection<'_>) {
    let mut bufs = BUFS.borrow_ref_mut(cs);
    let mut feed = FEED.borrow_ref_mut(cs);
    let mut stats = STATS.borrow_ref_mut(cs);
    // Split fields so both slots can be borrowed at once; they are disjoint.
    let SndBufs { l, r, next } = &mut *bufs;
    let i = *next;
    let verdict = feed.take_chunk(&mut l[i], &mut r[i], PERIOD_SAMPLES, false);
    *next = 1 - i;
    stats.serviced += 1;
    if matches!(verdict, Chunk::Full) {
        stats.full_takes += 1;
    } else {
        stats.starved_entries += 1;
        let mut first = FIRST_STARVE.borrow_ref_mut(cs);
        if *first == u32::MAX {
            *first = *CURRENT_FRAME.borrow_ref(cs);
        }
    }
    // SAFETY: both sources are word-aligned fields of a `static`, and the
    // transfer is re-armed before the next period can consume the old one.
    unsafe { rearm_dma(l[i].as_ptr(), r[i].as_ptr()) };
}

// --------------------------------------------------------------- sound setup

/// Sound on: FIFO A → left at 100 %, FIFO B → right at 100 %, both FIFOs reset,
/// master enable, bias in 8-bit mode. agb `hw.rs`'s enable sequence, transcribed
/// because those functions are `pub(super)` (the product-play ROM's own note).
fn enable_sound() {
    let sound_control: u16 = (1 << 2)      // sound A volume 100%
        | (1 << 9)                          // A to LEFT out
        | (1 << 11)                          // A FIFO reset
        | (1 << 3)                           // sound B volume 100%
        | (1 << 12)                          // B to RIGHT out
        | (1 << 15); // B FIFO reset
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(SOUNDCNT_H, sound_control);
        io_write_u16(SOUNDCNT_X, 1 << 7);
        io_write_u16(SOUNDBIAS, io_read_u16(SOUNDBIAS) | (1 << 14));
    }
}

/// Stop the chain. The teardown the FLAC harness learned to do: leaving the
/// FIFOs enabled hangs mGBA's exit.
fn teardown_audio() {
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(REG_DMA1_CONTROL, 0);
        io_write_u16(REG_DMA2_CONTROL, 0);
        io_write_u16(SOUNDCNT_X, 0);
    }
}

/// Program timer0 (one sample) and timer1 (one feeder period, interrupting).
/// Timer2/timer3 are the bench's 32-bit cycle counter and are left alone.
fn arm_chain(timer0: &mut Timer, timer1: &mut Timer) {
    timer0.set_enabled(false);
    timer1.set_enabled(false);
    timer0
        .set_divider(Divider::Divider1)
        .set_interrupt(false)
        .set_overflow_amount(TICKS_PER_SAMPLE as u16);
    timer1
        .set_cascade(true)
        .set_interrupt(true)
        .set_overflow_amount(PERIOD_SAMPLES as u16);
    timer0.set_enabled(true);
    timer1.set_enabled(true);
}

fn stop_chain(timer0: &mut Timer, timer1: &mut Timer) {
    timer1.set_enabled(false);
    timer0.set_enabled(false);
}

// ------------------------------------------------------------------- the arm

/// What the gate arm measured.
pub struct GateReport {
    pub cost: FrameCost,
    pub all8: u32,
    pub serviced: u32,
    pub full_takes: u32,
    pub starved_entries: u32,
    pub pushed: usize,
    pub consumed: usize,
    /// The frame index of the first starved period (`u32::MAX` = none).
    pub first_starve: u32,
    /// Frames whose measured window (decode + convert + push, IRQ live) exceeded
    /// one 262,144-cycle period. These are the frames that can cost the feeder
    /// ground, so the count explains an underrun tally rather than leaving it to
    /// be guessed at.
    pub over_period: u32,
    /// The worst such frame's window.
    pub over_period_worst: u32,
}

/// One full album decode through the product decoder **with the output stage
/// running**: ring push inside the window, feeder IRQ and FIFO DMA live.
pub fn gate_pass(
    album: Album<'_>,
    tables: &mut CodeTables,
    class: &mut ClassTables,
    hot: &mut HotState,
    counter: &mut Counter<'_>,
    timer0: &mut Timer,
    timer1: &mut Timer,
    overhead: u32,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
) -> Result<GateReport, uli_lite::Error> {
    use uli_lite::Error;

    let mut decoder = GbaDecoder::new(album, tables, class, hot)?;
    counter.soft_reset();
    let mut cost = FrameCost::new();
    let mut all8 = 2_166_136_261u32;
    let mut over_period = 0u32;
    let mut over_period_worst = 0u32;
    // The convert buffers: ULI2's int8 is already the DAC byte, so this is a
    // cast, not the FLAC path's PCM16 `>> 8`.
    let mut bl = [0u8; M];
    let mut br = [0u8; M];

    // Prime the hardware from an empty ring, then arm the clock. The first
    // PREFILL_FRAMES frames are decoded **outside** every window so the first
    // period is not born starved; they are still hashed, so the pin covers the
    // whole album.
    // Prefill: decode the first frames untimed and push them.
    for _ in 0..PREFILL_FRAMES {
        decoder.decode_frame(l8, r8, None, None)?;
        #[cfg(feature = "gate-hash")]
        {
            all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        }
        for i in 0..M {
            bl[i] = l8[i] as u8;
            br[i] = r8[i] as u8;
        }
        critical_section::with(|cs| {
            let _ = FEED.borrow_ref_mut(cs).push_bytes(&bl, &br);
        });
    }
    let primed = critical_section::with(|cs| {
        let mut bufs = BUFS.borrow_ref_mut(cs);
        let mut feed = FEED.borrow_ref_mut(cs);
        let SndBufs { l, r, next } = &mut *bufs;
        let verdict = feed.take_chunk(&mut l[0], &mut r[0], PERIOD_SAMPLES, false);
        *next = 1;
        // SAFETY: the slots are word-aligned statics that outlive the transfer.
        unsafe { rearm_dma(l[0].as_ptr(), r[0].as_ptr()) };
        matches!(verdict, Chunk::Full)
    });
    if !primed {
        return Err(Error::BadAlbum);
    }
    enable_sound();
    // Clear a stale Timer1 IF before the source is enabled: registering the
    // handler sets the IE bit, and a pending bit from the boot path would enter
    // the feeder before the chain has a period to serve.
    // SAFETY: fixed I/O address; writing 1 to the bit clears it.
    unsafe { io_write_u16(REG_IF, 1 << 4) };
    // SAFETY: the handler allocates nothing, logs nothing, and touches only the
    // statics above.
    let handler = unsafe { add_interrupt_handler(Interrupt::Timer1, feeder) };
    arm_chain(timer0, timer1);

    for f in PREFILL_FRAMES..album.n_frames() {
        critical_section::with(|cs| *CURRENT_FRAME.borrow_ref_mut(cs) = f);
        // Pace **outside** the window: a real player waits here. Charging the
        // wait would measure the hardware clock rather than the work.
        while critical_section::with(|cs| FEED.borrow_ref_mut(cs).free_samples()) < M {
            core::hint::spin_loop();
        }
        let (res, raw) = timed_irq(counter, || {
            let r = decoder.decode_frame(l8, r8, None, None);
            for i in 0..M {
                bl[i] = l8[i] as u8;
                br[i] = r8[i] as u8;
            }
            // The ring copy is the only part that needs the critical section;
            // the conversion above is producer-private and runs with IRQs live.
            critical_section::with(|cs| {
                let _ = FEED.borrow_ref_mut(cs).push_bytes(&bl, &br);
            });
            r
        });
        res?;
        // Hash outside the window, `gate-hash` only. **This is not free**: the
        // gate pass is paced by the audio clock, so producer work the window does
        // not count still costs the feeder ground. Measured: the hash is
        // ~12,200 cycles/frame, which made the producer 4.7% slower than real
        // time and starved 772 periods (12.06 s) over the album. A player hashes
        // nothing, so the default arm does not either — the pin is witnessed by
        // the untimed passes in the same image.
        #[cfg(feature = "gate-hash")]
        {
            all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        }
        let net = raw.saturating_sub(overhead);
        if net > 262_144 {
            over_period += 1;
            over_period_worst = over_period_worst.max(net);
        }
        cost.observe(f, net, decoder.ops());
    }

    // Drain: the producer runs ~14% ahead of the clock, so it finishes with the
    // ring full. A player keeps the chain running until the queue is empty; so
    // does this arm, which is what makes its runtime exactly the album's length
    // instead of 250 ms short of it.
    while critical_section::with(|cs| FEED.borrow_ref(cs).ready()) > 0 {
        core::hint::spin_loop();
    }
    stop_chain(timer0, timer1);
    drop(handler);
    teardown_audio();

    let (serviced, full_takes, starved_entries) = critical_section::with(|cs| {
        let s = STATS.borrow_ref(cs);
        (s.serviced, s.full_takes, s.starved_entries)
    });
    let first_starve = critical_section::with(|cs| *FIRST_STARVE.borrow_ref(cs));
    let (pushed, consumed) = critical_section::with(|cs| {
        let feed = FEED.borrow_ref(cs);
        (feed.pushed(), feed.consumed())
    });
    Ok(GateReport {
        cost,
        all8,
        serviced,
        full_takes,
        starved_entries,
        first_starve,
        pushed,
        consumed,
        over_period,
        over_period_worst,
    })
}

/// The period rate, printed so the log says what the chain was asked to do.
pub const fn periods_per_sec() -> u32 {
    PERIODS_PER_SEC
}
