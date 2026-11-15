//! The ARM-only hardware half of the AP-0 probes: the sound-FIFO DMA bootstrap,
//! the Timer1 period IRQ, the ring producer helpers, and the period ledger.
//!
//! Nearly everything in this file is **copied** from
//! `examples/flac_integration/src/product_play_main.rs` (cited by line) rather
//! than re-derived. That file is the shipped playback path: its ordering has been
//! on hardware, and the failure the order avoids (a stale Timer1 `IF` entering the
//! feeder before the chain has a period to serve) is invisible in a build. A probe
//! that invented its own arming order would be measuring the wrong thing.
//!
//! ## The timer2/3 ruling
//!
//! `arm_audio_chain` (source 703-732) programs timer2 as a cascaded period
//! witness and leaves timer3 alone; the cycle [`Counter`] wants timer2 at /1
//! cascading into timer3. Both cannot own the pair. The ruling for these probes:
//! the audio chain is armed **first**, the `Counter` is started **second** — so the
//! Counter wins timer2/3 and the chain's period latch is sacrificed. Nothing in a
//! probe reads that latch (`Stats::periods` is a software counter incremented in
//! the feeder's critical section), so the sacrifice costs no measurement.
//!
//! The two handles cannot even be held at once: `all_timers()` and `timers()` both
//! borrow `gba.timers` mutably, and `all_timers()` *resets* all four timer
//! registers on construction (`Timer::new` writes data and control = 0). So the
//! order is per ROM: `all_timers()` once → bootstrap → `arm_audio_chain` → drop →
//! `timers()` once → `Counter::start` → keep it. The second acquisition only
//! touches timer2/3, which `Counter::start` reconfigures immediately; timer0/1
//! keep running, which is the whole point. Nothing may call `all_timers()` again:
//! that would zero the counter's registers under it. [`disarm_audio`] therefore
//! writes the timer control registers directly, the same idiom [`rearm_dma`] uses.
//!
//! ## Measurement discipline
//!
//! - Every per-frame, per-arm and whole-upload cost is measured with **interrupts
//!   live** (`Counter::read` + [`timer::cycles_between`]), because the cost being
//!   asked about is what the application pays *while the audio chain runs*.
//! - [`timer::timed`] (IRQs off) is used only for a single 8x8 tile write and the
//!   empty-window calibration — windows provably shorter than a period.
//! - Every printed number says whether the audio chain was live.
//! - No divide on the measured path: the ROMs print sums and counts, means are
//!   derived host-side.

use agb::external::critical_section::{self, Mutex};
use agb::interrupt::{Interrupt, InterruptHandler, add_interrupt_handler};
use agb::timer::{AllTimers, Divider};
use core::cell::RefCell;

use crate::geometry as geo;
use crate::playback::{Chunk, PushError, RingFeed, convert_frame};
use crate::synth::Synth;

/// The tree's cycle instrument, included rather than rewritten — timer2 /1
/// cascades into timer3, low/high/low read, `cycles_between` saturating to
/// `u32::MAX`, `timed` inside a critical section, `calibrate` with 3 discarded +
/// 5 reported empty windows. Its `Counter::start` takes `&mut Timers`; see the
/// module note above for how that is sequenced against the audio chain.
#[path = "../../e5_fetch/src/timer.rs"]
pub mod timer;

/// Samples per hardware period (560) and the ring geometry, aliased so the code
/// below reads like the harness it came from.
pub const CHUNK: usize = geo::CHUNK_SAMPLES;
/// Ring capacity in samples (20,160): 36 chunks, 1.23 s of lead at 16,384 Hz.
pub const RING_SAMPLES: usize = geo::RING_SAMPLES;
/// Cold-start prefill in samples (8,960): 16 chunks, 547 ms.
pub const PREFILL_SAMPLES: usize = geo::PREFILL_SAMPLES;
/// Cycles one audio period lasts (573,440) — the denominator of every share.
pub const PERIOD_CYCLES: u32 = geo::PERIOD_CYCLES;

/// Samples a skip fades over: 256, 15.6 ms at the product rate (ALBUM-PLAYER.md
/// §4.4).
pub const FADE_SAMPLES: usize = 256;
/// The fade is a shift, not a divide: `sample * k >> FADE_SHIFT` with
/// `FADE_SAMPLES == 1 << FADE_SHIFT`. Kept as an assertion so a future value
/// cannot quietly turn the fade into an `__aeabi` call.
const FADE_SHIFT: u32 = 8;
const _: () = assert!(FADE_SAMPLES == 1 << FADE_SHIFT);

// ---------------------------------------------------------------- hardware
//
// Transcribed from `product_play_main.rs:262-296`, carrying its finding with it:
// these are private registers, named because agb 0.25.0 gives an application no
// other way to drive the sound FIFO itself. Channel n's DMA block is at
// 0x0400_00B0 + 0x0C·n, so DMA1 starts at 0xBC — not 0xB0, which is DMA0, for
// which sound-FIFO start timing is illegal. The pairing is fixed: DMA1 → FIFO A
// (left), DMA2 → FIFO B (right); shifting the pair one channel low left the left
// channel silent and glitched the right on hardware (2026-10-19).
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
/// Timer control registers live at 0x0400_0102 + 4·n (agb `timer.rs`'s own
/// arithmetic); [`disarm_audio`] needs them because by then the `AllTimers`
/// handle is gone.
const REG_TM0_CONTROL: usize = 0x0400_0102;
const REG_TM1_DATA: usize = 0x0400_0104;
const REG_TM1_CONTROL: usize = 0x0400_0106;
/// Timer data registers, the cycle counter's two halves.
const REG_TM2_DATA: usize = 0x0400_0108;
const REG_TM3_DATA: usize = 0x0400_010C;

/// Timer1's reload as `set_overflow_amount` writes it (`0 − 560`): the counter
/// runs *up* from its negative latch to zero, so `TM1_DATA − TM1_RELOAD` is how
/// far into the current period the interrupt entered — the late-service measure
/// below. (Timer0's latch is the same trick one rung down, at
/// `0 − TICKS_PER_SAMPLE`; nothing here needs the sub-period sample position.)
const TM1_RELOAD: u16 = 0u16.wrapping_sub(CHUNK as u16);
/// The sound DMA refills the 32-byte FIFO when it falls to half — 16 samples. A
/// re-arm later than this replays part of the old buffer and splices
/// mid-waveform; the number is measured, not theorised (`geo`'s constant, and the
/// 2026-10-19 hardware reading behind it).
const REARM_GRACE_SAMPLES: u16 = geo::REARM_GRACE_SAMPLES;
/// Payload words per period the DMA is handed (140).
const DMA_WORDS: u16 = geo::DMA_WORDS;

/// agb `hw.rs`'s sound-DMA control word (source 316-319): destination fixed
/// (2<<5) | repeat (1<<9) | transfer words (1<<10) | sound-FIFO start timing
/// (3<<12) | enable (1<<15).
const DMA_SOUND_CONTROL: u16 = (2 << 5) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 15);

// ------------------------------------------------------------------ io

/// # Safety
/// `addr` must be a readable, even address in the I/O region; callers pass the
/// `REG_*`/`FIFO_*` constants above. (Source 525-538.)
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

// --------------------------------------------------------------- statics

/// Ping-pong DMA payload, word-aligned (a DMA source must be), one buffer per
/// channel per slot. The two live slots are what the two sound DMAs are reading;
/// which one gets handed out next is the ring's business, and `next` lives in the
/// same static so both sides see the same alternation. (Source 345-361.)
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

/// `RingFeed::new()` is `const` precisely so a consumer with no allocator in an
/// interrupt can hold one in a static, which is what its module docs ask for. The
/// const argument resolves to `geometry::RING_SAMPLES`, so the host model
/// instantiates the identical type. (Source 365-369.)
static FEED: Mutex<RefCell<RingFeed<RING_SAMPLES>>> = Mutex::new(RefCell::new(RingFeed::new()));
static BUFS: Mutex<RefCell<SndBufs>> = Mutex::new(RefCell::new(SndBufs::new()));

/// The feeder's ledger, and the probe's only period counter.
///
/// The chain's timer2 latch is sacrificed to the cycle counter (module note), so
/// `periods` is a software increment inside the feeder's existing critical
/// section — one add per 34.18 ms.
#[derive(Clone, Copy)]
pub struct Stats {
    /// Periods serviced since boot: the software replacement for the chain's
    /// timer2 witness.
    pub periods: u32,
    /// Sum of the feeder's own cost, in CPU cycles, interrupts live.
    pub feed_cycles_sum: u64,
    /// Worst single feeder entry.
    pub feed_cycles_max: u32,
    /// Feeder windows that wrapped the 32-bit composite — a failed instrument,
    /// never data.
    pub feed_wrapped: u32,
    /// Periods whose chunk came back `Chunk::Starved`.
    pub starved: u32,
    /// Consecutive `Starved` periods, current and worst.
    pub starved_run: u32,
    /// Worst run of consecutive `Starved` periods.
    pub starved_run_max: u32,
    /// Consecutive all-zero chunks on the wire, current and worst. Counted only
    /// when [`set_silence_scan`] turned the scan on (the skip probe's question).
    pub silence_run: u32,
    /// Worst run of consecutive all-zero chunks.
    pub silence_run_max: u32,
    /// `Starved` while the producer was still running: the underflow count.
    pub underflows: u32,
    /// Periods whose ring chunk was consumed and then replaced with silence,
    /// because the producer had marked it stale ([`drop_stale`]): §4.4's
    /// "dropped, not played" on a ring that cannot truncate.
    pub dropped: u32,
    /// Worst lateness of a period service, in samples into the period, and how
    /// many services entered later than the 16-sample re-arm grace.
    pub late_max: u16,
    /// Services that entered after the re-arm grace: a pop on hardware.
    pub late_over_grace: u32,
    /// `Chunk::Tail` periods (the ring ran dry with the producer finished).
    pub tail_periods: u32,
    /// The producer has finished: a starved period is a drain, not an underflow.
    pub finished: bool,
    /// Scan delivered chunks for all-zero bytes (costs a pass over 1,120 bytes).
    pub silence_scan: bool,
}

impl Stats {
    const fn new() -> Self {
        Self {
            periods: 0,
            feed_cycles_sum: 0,
            feed_cycles_max: 0,
            feed_wrapped: 0,
            starved: 0,
            starved_run: 0,
            starved_run_max: 0,
            silence_run: 0,
            silence_run_max: 0,
            underflows: 0,
            dropped: 0,
            late_max: 0,
            late_over_grace: 0,
            tail_periods: 0,
            finished: false,
            silence_scan: false,
        }
    }
}

static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats::new()));

/// The skip probe's drop window (ALBUM-PLAYER.md §4.4 step 2): the ring offset the
/// producer had queued to when it marked the queue stale. Every chunk the consumer
/// takes below this offset is discarded by the feeder. `0` means "no window open",
/// and is not a reachable frontier value after the first push (see [`drop_stale`]).
static DROP: Mutex<RefCell<usize>> = Mutex::new(RefCell::new(0));

/// What the skip probe's **wire watch** saw: when the bytes handed to the DMA
/// stopped carrying the old track and when they started carrying the new one.
///
/// Both are read off the delivered bytes (the same all-zero verdict the silence
/// scan forms), never off the producer's intentions, so the two queue policies are
/// asked the identical question — periods until the DMA receives new-track audio.
#[derive(Clone, Copy)]
pub struct Watch {
    /// Armed, and not yet resolved.
    pub armed: bool,
    /// The period the watch was armed at ([`Stats::periods`]).
    pub at: u32,
    /// First all-zero chunk delivered at or after `at`; `0` = not seen yet.
    pub silent_at: u32,
    /// First non-zero chunk delivered after `silent_at`; `0` = not seen yet.
    pub new_at: u32,
}

static WATCH: Mutex<RefCell<Watch>> = Mutex::new(RefCell::new(Watch {
    armed: false,
    at: 0,
    silent_at: 0,
    new_at: 0,
}));

/// One measured window's fold: count, total, worst, best, and how many windows
/// wrapped. `count` and `sum` are what the host divides; the ROM never does.
#[derive(Clone, Copy)]
pub struct Fold {
    /// Windows observed.
    pub count: u32,
    /// Cycle total.
    pub sum: u64,
    /// Shortest window.
    pub min: u32,
    /// Longest window.
    pub max: u32,
    /// Windows that wrapped the composite (a failed instrument).
    pub wrapped: u32,
}

impl Fold {
    /// An empty fold.
    pub const fn new() -> Self {
        Self {
            count: 0,
            sum: 0,
            min: u32::MAX,
            max: 0,
            wrapped: 0,
        }
    }

    /// Fold in one window's cycle count.
    pub fn observe(&mut self, cycles: u32) {
        if cycles == u32::MAX {
            self.wrapped = self.wrapped.wrapping_add(1);
            return;
        }
        self.count = self.count.wrapping_add(1);
        self.sum = self.sum.wrapping_add(u64::from(cycles));
        if cycles > self.max {
            self.max = cycles;
        }
        if cycles < self.min {
            self.min = cycles;
        }
    }
}

// --------------------------------------------------------------- enable

/// Read the free-running cycle counter's two halves.
///
/// The feeder runs in an interrupt and cannot borrow the main loop's
/// [`timer::Counter`] (it holds `&mut Timer`), so this is the same low/high/low
/// protocol against the same two registers `Counter::read` uses — not a second
/// instrument, the same one without the borrow.
fn read_cycles() -> u32 {
    for _ in 0..4 {
        // SAFETY: fixed I/O addresses (timer2/timer3 data registers).
        let (low1, high, low) = unsafe {
            (
                io_read_u16(REG_TM2_DATA),
                io_read_u16(REG_TM3_DATA),
                io_read_u16(REG_TM2_DATA),
            )
        };
        if low >= low1 {
            return (u32::from(high) << 16) | u32::from(low);
        }
    }
    // SAFETY: as above; the pathological case takes a bounded, deterministic value.
    let (high, low) = unsafe { (io_read_u16(REG_TM3_DATA), io_read_u16(REG_TM2_DATA)) };
    (u32::from(high) << 16) | u32::from(low)
}

/// Sound hardware on: FIFO A → left at 100 %, FIFO B → right at 100 %, both FIFOs
/// reset, master enable, bias at 8-bit/65,536 Hz — agb `hw.rs`'s enable sequence,
/// transcribed because those functions are `pub(super)`. (Source 645-658.)
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

/// Stop both sound DMAs and the sound hardware. The teardown the harness learned
/// to do: leaving the FIFOs enabled hangs mGBA's exit. (Source 662-667.)
pub fn teardown_audio() {
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(REG_DMA1_CONTROL, 0);
        io_write_u16(REG_DMA2_CONTROL, 0);
        io_write_u16(SOUNDCNT_X, 0);
    }
}

/// Stop the audio chain and the sound DMA, on a ROM that has already given up its
/// `AllTimers` handle to the cycle counter.
///
/// Writes timer1/timer0's control words directly rather than through
/// `AllTimers`: re-acquiring that handle resets all four timer registers,
/// including the two the [`timer::Counter`] is running on. Same idiom as
/// [`rearm_dma`] — the probe owns these registers, so it writes them.
pub fn disarm_audio() {
    // SAFETY: fixed I/O addresses.
    unsafe {
        io_write_u16(REG_TM1_CONTROL, 0);
        io_write_u16(REG_TM0_CONTROL, 0);
    }
    teardown_audio();
}

/// Re-arm both sound DMAs from the slot just filled: stop, source, destination,
/// count, go. agb's `enable_dma_for_sound` (`hw.rs:56-68`) writes source,
/// destination and control and no count at all — in sound-FIFO mode the FIFO
/// pulls 4 words per request — so the count here is ours, deliberately. (Source
/// 680-697.)
///
/// # Safety
/// `lsrc`/`rsrc` must be 4-byte-aligned pointers into the static ping-pong, alive
/// for the whole transfer.
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

/// Program the audio timebase for the 16,384 Hz rung: timer0 /1 with a 1,024-tick
/// reload (one sample), timer1 cascaded with a 560 reload and its interrupt on
/// (one period). (Source 703-732, body unchanged.)
///
/// # The timer2/3 annotation
///
/// The source arms timer2 as a cascaded period witness here, and this body still
/// does. It is **spent**: the probe starts the cycle [`timer::Counter`] on
/// timer2/3 immediately after this call, which reprograms both registers. The
/// chain's period audit is therefore not available to a probe, and
/// [`Stats::periods`] — a software counter in the feeder — carries it instead.
/// Call this **before** `Counter::start`, never after.
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
        .set_overflow_amount(geo::TICKS_PER_SAMPLE as u16);
    timers
        .timer1
        .set_cascade(true)
        .set_interrupt(true)
        .set_overflow_amount(CHUNK as u16);
    timers.timer3.set_cascade(false).set_interrupt(false);
    timers.timer2.set_enabled(true);
    timers.timer0.set_enabled(true);
    timers.timer1.set_enabled(true);
}

// ---------------------------------------------------------------- feeder

/// The mixer interrupt: hand the next chunk to the sound DMA, or silence if the
/// producer has not delivered one. (Source 737-889, shape preserved.)
///
/// Nothing here allocates and nothing here logs — agb's interrupt-handler
/// contract, and the reason the ring is a static array. All three static borrows
/// use the `CriticalSection` token the handler is handed, which is also the race
/// gate: the producer only touches the ring inside a critical section.
fn feeder(cs: critical_section::CriticalSection<'_>) {
    // The period count and the entry cycle read come first: anything before them
    // would be inside the answer they are the start of.
    let entered = read_cycles();
    let mut bufs = BUFS.borrow_ref_mut(cs);
    let mut feed = FEED.borrow_ref_mut(cs);
    let mut stats = STATS.borrow_ref_mut(cs);

    stats.periods = stats.periods.wrapping_add(1);
    let finished = stats.finished;
    let scan = stats.silence_scan;
    // SAFETY: fixed I/O address (timer1's data register).
    let late = unsafe { io_read_u16(REG_TM1_DATA).wrapping_sub(TM1_RELOAD) };
    if late > stats.late_max {
        stats.late_max = late;
    }
    if late > REARM_GRACE_SAMPLES {
        stats.late_over_grace = stats.late_over_grace.wrapping_add(1);
    }

    // Raw pointers to the slot BEFORE filling, in their own statement: the static
    // never moves, so they stay valid past the borrow, and `rearm_dma` consumes
    // them once the copy is done. Destructuring to field borrows avoids
    // double-borrowing the struct through two `IndexMut`s in one expression.
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
    // The drop window (§4.4 step 2, the skip probe's `drop` policy): a chunk the
    // producer queued before it marked the ring stale is still taken here — so
    // `consumed` advances and the DMA gets a whole buffer every period — and then
    // overwritten with silence instead of handed the old track's audio. `at` is the
    // cursor *before* the take: "this period would have delivered a stale chunk".
    let at = feed.consumed();
    let stale = {
        let frontier = *DROP.borrow_ref(cs);
        frontier != 0 && at < frontier
    };
    let (verdict, silent) = {
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
        if stale {
            dl.fill(0);
            dr.fill(0);
        }
        // Silence is a property of the bytes on the wire, not of the verdict: the
        // skip probe feeds zeros as *real* ring data (§4.4 step 4), and the drop
        // above puts zeros on the wire out of *stale* ring data, so a `Full` chunk
        // can be silent. Only counted when a probe asked for it, because the scan
        // costs a pass over both buffers inside this window.
        let silent = taken == Chunk::Starved
            || (scan && dl.iter().all(|&b| b == 0) && dr.iter().all(|&b| b == 0));
        *next = 1 - *next;
        (taken, silent)
    };
    if stale {
        stats.dropped = stats.dropped.wrapping_add(1);
    }
    // The window closes itself once the cursor has passed the frontier, so a
    // producer that stops pushing cannot leave it open.
    {
        let mut frontier = DROP.borrow_ref_mut(cs);
        if *frontier != 0 && feed.consumed() >= *frontier {
            *frontier = 0;
        }
    }
    match verdict {
        Chunk::Full => {}
        Chunk::Tail { .. } => {
            stats.tail_periods = stats.tail_periods.wrapping_add(1);
        }
        Chunk::Starved => {
            stats.starved = stats.starved.wrapping_add(1);
            stats.starved_run = stats.starved_run.wrapping_add(1);
            if stats.starved_run > stats.starved_run_max {
                stats.starved_run_max = stats.starved_run;
            }
            // Only a starved period while the producer still has work is an
            // underflow. The ledger did not advance for it (`take_chunk` consumes
            // nothing it does not copy), so real samples stay queued behind the
            // silence.
            if !finished {
                stats.underflows = stats.underflows.wrapping_add(1);
            }
        }
    }
    if verdict != Chunk::Starved {
        stats.starved_run = 0;
    }
    if silent {
        stats.silence_run = stats.silence_run.wrapping_add(1);
        if stats.silence_run > stats.silence_run_max {
            stats.silence_run_max = stats.silence_run;
        }
    } else {
        stats.silence_run = 0;
    }
    // The wire watch — §4.4's decide-by-observable question: the first silent chunk
    // after the arm, then the first non-silent one after that. It reads the verdict
    // the scan just formed, so it observes the bytes the DMA was handed.
    if scan {
        let mut watch = WATCH.borrow_ref_mut(cs);
        if watch.armed {
            if watch.silent_at == 0 {
                if silent {
                    watch.silent_at = stats.periods;
                }
            } else if !silent && watch.new_at == 0 {
                watch.new_at = stats.periods;
            }
        }
    }
    // SAFETY: aligned pointers into the static ping-pong; the other slot was
    // handed to the DMA a whole period ago, and the FIFO's 4-word pulls (µs for a
    // whole buffer) are long finished by the next period (34.2 ms at this rate).
    unsafe { rearm_dma(lp, rp) };

    drop(feed);
    drop(bufs);
    let cost = timer::cycles_between(entered, read_cycles());
    if cost == u32::MAX {
        stats.feed_wrapped = stats.feed_wrapped.wrapping_add(1);
    } else {
        stats.feed_cycles_sum = stats.feed_cycles_sum.wrapping_add(u64::from(cost));
        if cost > stats.feed_cycles_max {
            stats.feed_cycles_max = cost;
        }
    }
}

// ---------------------------------------------------------------- producer

/// Main-thread producer scratch: the i32 frames the synth writes and the 8-bit
/// FIFO bytes `convert_frame` derives from them. One chunk each, so a push never
/// needs a buffer longer than the slice it is about to hand the ring.
///
/// Heap-allocated by each bin (`Box::new`), the same way the product ROM's
/// `scratch_l/scratch_r` Vecs are: 5.6 kB does not belong on the IWRAM stack.
pub struct Scratch {
    l: [i32; CHUNK],
    r: [i32; CHUNK],
    bl: [u8; CHUNK],
    br: [u8; CHUNK],
}

impl Scratch {
    /// A zeroed scratch block.
    pub const fn new() -> Self {
        Self {
            l: [0; CHUNK],
            r: [0; CHUNK],
            bl: [0; CHUNK],
            br: [0; CHUNK],
        }
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert one frame pair and append it to the ring inside one critical section.
///
/// This is the only part of a push that needs IME off: the 7-2 fix, where a
/// whole-frame critical section held interrupts off ~14,680 cycles against the
/// 16-sample sound-DMA re-arm grace (~273 cycles) and a chunk slice costs ~4,014
/// cycles (~3.92 samples), a 4.08x margin. (Source 1219-1235, 1236-1272.)
fn push_converted(l: &[i32], r: &[i32], bl: &mut [u8], br: &mut [u8]) -> Result<(), PushError> {
    convert_frame(l, r, bl, br).map_err(PushError::OutOfProfile)?;
    critical_section::with(|cs| FEED.borrow_ref_mut(cs).push_bytes(bl, br))
}

/// Push `n` real samples from `synth`, in `CHUNK`-sized slices, one critical
/// section per slice. Returns how many samples were pushed.
///
/// `n` must be a multiple of [`CHUNK`] while the feeder is running: a partial
/// chunk left in the ring can never be taken (`take_chunk` consumes nothing when
/// `ready < chunk` with the producer unfinished), so it starves the DMA forever.
///
/// Generation happens **outside** the critical section; only the ring append is
/// inside. Never hold the `FEED` borrow across a wait for the DMA — the feeder's
/// IRQ borrows the same `RefCell` (edition-2024 temporary-lifetime deadlock, the
/// lesson documented at source 1349-1352).
pub fn push_synth(synth: &mut Synth, n: usize, scratch: &mut Scratch) -> Result<usize, PushError> {
    let Scratch { l, r, bl, br } = scratch;
    let mut offset = 0usize;
    while offset < n {
        let take = (n - offset).min(CHUNK);
        synth.fill_frame(&mut l[..take], &mut r[..take]);
        push_converted(&l[..take], &r[..take], &mut bl[..take], &mut br[..take])?;
        offset += take;
    }
    Ok(offset)
}

/// Push `n` samples of silence **as real ring data** (ALBUM-PLAYER.md §4.4 step
/// 4: the DMA keeps getting a whole chunk every period, so the underrun counter
/// stays a measurement of the producer rather than of the skip).
pub fn push_silence(n: usize) -> Result<usize, PushError> {
    const ZEROS: [u8; CHUNK] = [0u8; CHUNK];
    let mut offset = 0usize;
    while offset < n {
        let take = (n - offset).min(CHUNK);
        critical_section::with(|cs| {
            FEED.borrow_ref_mut(cs)
                .push_bytes(&ZEROS[..take], &ZEROS[..take])
        })?;
        offset += take;
    }
    Ok(offset)
}

/// Push one whole chunk of real audio whose first [`FADE_SAMPLES`] samples ramp
/// **down** to silence — the skip's step 1 (fade out). The rest of the chunk is
/// zeros; the ramp is a shift, per §6 (no divide).
///
/// The chunk is whole on purpose. `RingFeed::take_chunk` consumes nothing when
/// `ready < chunk` while the producer is still running, so a partial chunk left in
/// the ring can never be taken: it starves the DMA forever and every later period
/// counts an underflow. Every push in a running probe is chunk-aligned.
pub fn push_fade_out(synth: &mut Synth, scratch: &mut Scratch) -> Result<usize, PushError> {
    let Scratch { l, r, bl, br } = scratch;
    synth.fill_frame(&mut l[..CHUNK], &mut r[..CHUNK]);
    for i in 0..CHUNK {
        // 256..1 down to zero, then silence for the tail of the chunk.
        let k = FADE_SAMPLES.saturating_sub(i);
        l[i] = (l[i] * k as i32) >> FADE_SHIFT;
        r[i] = (r[i] * k as i32) >> FADE_SHIFT;
    }
    push_converted(&l[..CHUNK], &r[..CHUNK], &mut bl[..CHUNK], &mut br[..CHUNK])?;
    Ok(CHUNK)
}

/// Push one whole chunk of real audio whose first [`FADE_SAMPLES`] samples ramp
/// **up** from silence — the skip's step 6 (fade in). The head of the new track
/// arrives with the ramp in its first 15.6 ms, and the chunk is whole.
pub fn push_fade_in(synth: &mut Synth, scratch: &mut Scratch) -> Result<usize, PushError> {
    let Scratch { l, r, bl, br } = scratch;
    synth.fill_frame(&mut l[..CHUNK], &mut r[..CHUNK]);
    for i in 0..FADE_SAMPLES {
        let k = i + 1;
        l[i] = (l[i] * k as i32) >> FADE_SHIFT;
        r[i] = (r[i] * k as i32) >> FADE_SHIFT;
    }
    push_converted(&l[..CHUNK], &r[..CHUNK], &mut bl[..CHUNK], &mut br[..CHUNK])?;
    Ok(CHUNK)
}

/// Top the ring up to `target` samples of real audio, one chunk per step — the
/// producer's "keep the queue where the policy says" action.
pub fn fill_to(
    synth: &mut Synth,
    target: usize,
    scratch: &mut Scratch,
) -> Result<usize, PushError> {
    // Never ask for the last chunk: a push of `CHUNK` needs `free >= CHUNK`, and a
    // probe that aimed at the capacity exactly would refuse its own last push.
    let target = target.min(RING_SAMPLES - CHUNK);
    let mut pushed = 0usize;
    while ready() < target {
        push_synth(synth, CHUNK, scratch)?;
        pushed += CHUNK;
    }
    Ok(pushed)
}

/// Samples the feeder has not yet taken ([`RingFeed::ready`]).
pub fn ready() -> usize {
    critical_section::with(|cs| FEED.borrow_ref(cs).ready())
}

/// Free capacity for the producer ([`RingFeed::free_samples`]).
pub fn free() -> usize {
    critical_section::with(|cs| FEED.borrow_ref(cs).free_samples())
}

/// Wait until the ring has `need` samples free, bounded by the ring's own depth in
/// periods plus four — [`wait_consumed`]'s bound, mirrored — so a wedged wait fails
/// instead of spinning. The skip probe's drop policy needs it: the ring is still
/// full of chunks the feeder is discarding, so a push has to wait for the period
/// that frees the room for it.
pub fn wait_free(need: usize) -> bool {
    let start = stats().periods;
    let deadline = (RING_SAMPLES / CHUNK) as u32 + 4;
    while free() < need {
        if stats().periods.wrapping_sub(start) > deadline {
            return false;
        }
    }
    true
}

/// Mark the ring stale (ALBUM-PLAYER.md §4.4 step 2): every sample the producer has
/// already queued is consumed and discarded by the feeder, and only chunks pushed
/// after this call are ones the DMA hears.
///
/// The recorded frontier doubles as the window's off value, which is why it can
/// never be `0`: the ring's offsets only take 0 before the first push, and a caller
/// that marks an empty ring stale has nothing to discard anyway.
pub fn drop_stale() {
    critical_section::with(|cs| {
        let frontier = FEED.borrow_ref(cs).pushed();
        *DROP.borrow_ref_mut(cs) = frontier;
    });
}

/// Arm the wire watch at the current period ([`Stats::periods`]).
pub fn arm_watch() {
    critical_section::with(|cs| {
        let at = STATS.borrow_ref(cs).periods;
        *WATCH.borrow_ref_mut(cs) = Watch {
            armed: true,
            at,
            silent_at: 0,
            new_at: 0,
        };
    });
}

/// What the wire watch has seen so far.
pub fn watch() -> Watch {
    critical_section::with(|cs| *WATCH.borrow_ref(cs))
}

// --------------------------------------------------------------- ledger

/// A copy of the feeder's ledger.
pub fn stats() -> Stats {
    critical_section::with(|cs| *STATS.borrow_ref(cs))
}

/// Start the silence scan (or stop it).
pub fn set_silence_scan(on: bool) {
    critical_section::with(|cs| STATS.borrow_ref_mut(cs).silence_scan = on);
}

/// Zero the per-arm maxima and the underflow count, leaving the monotonic
/// `periods`, the cycle sums and the lateness marks alone — the per-setting marks
/// of the skip sweep. The lateness marks are deliberately **not** reset: they are
/// the "was the chain ever late?" witness, and a total that only covered the last
/// setting would be a worse answer than a cumulative one.
pub fn reset_marks() {
    critical_section::with(|cs| {
        let mut s = STATS.borrow_ref_mut(cs);
        s.starved = 0;
        s.starved_run = 0;
        s.starved_run_max = 0;
        s.silence_run = 0;
        s.silence_run_max = 0;
        s.underflows = 0;
        s.dropped = 0;
        s.tail_periods = 0;
    });
}

/// The producer has finished: from here a starved period is a drain, not an
/// underflow.
pub fn finish() {
    critical_section::with(|cs| STATS.borrow_ref_mut(cs).finished = true);
}

// ------------------------------------------------------------- bootstrap

/// The one bootstrap, in the order the shipped ROM settled on (source
/// 1476-1561):
///
/// 1. prefill the ring to [`PREFILL_SAMPLES`] (8,960 samples, 547 ms) before
///    anything is armed;
/// 2. prime the first DMA buffer inside one critical section — the buffer is
///    handed over before the first period boundary can ask for one, so audio
///    begins at arming rather than one period late;
/// 3. `enable_sound()`;
/// 4. clear a stale Timer1 `IF` **before** the source is enabled: registering the
///    handler sets the IE bit, and a pending bit left by the boot path would
///    enter the feeder before the chain has a period to serve;
/// 5. register the feeder on `Interrupt::Timer1`;
/// 6. [`arm_audio_chain`] — **before** the caller starts the cycle counter (the
///    timer2/3 ruling in the module docs).
///
/// The returned [`InterruptHandler`] must be held for the ROM's lifetime;
/// dropping it unregisters the feeder.
pub fn bootstrap(
    timers: &mut AllTimers<'_>,
    synth: &mut Synth,
    scratch: &mut Scratch,
) -> Option<InterruptHandler> {
    push_synth(synth, PREFILL_SAMPLES, scratch).ok()?;
    if !prime_first_buffer() {
        return None;
    }
    enable_sound();
    // SAFETY: fixed I/O address; writing 1 to an IF bit acknowledges it.
    unsafe { io_write_u16(REG_IF, 1 << 4) };
    // SAFETY: the handler allocates nothing, logs nothing and touches only the
    // statics above — agb's interrupt-handler contract.
    let handler = unsafe { add_interrupt_handler(Interrupt::Timer1, feeder) };
    arm_audio_chain(timers);
    Some(handler)
}

/// Hand the DMA its first whole chunk from the ring (source 1499-1540, body
/// preserved). `false` if the ring had no whole chunk — a probe bug, not a race:
/// [`bootstrap`] prefills first.
fn prime_first_buffer() -> bool {
    critical_section::with(|cs| {
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
            Chunk::Full => {
                // SAFETY: aligned pointers into the static ping-pong; the other
                // slot is the one the next period will be handed.
                unsafe { rearm_dma(lp, rp) };
                true
            }
            _ => false,
        }
    })
}

/// The interrupt-enable mask agb maintains at 0x0400_0200 (agb's own bit
/// numbering: `VBlank = 0`, `Timer1 = 4`), the master enable, and the two chain
/// timer control words — the "is the chain actually live?" evidence a probe log
/// needs when a number looks wrong.
pub fn chain_registers() -> (u16, u16, u16, u16) {
    // SAFETY: fixed I/O addresses.
    unsafe {
        (
            io_read_u16(REG_IE),
            io_read_u16(REG_IME),
            io_read_u16(REG_TM1_CONTROL),
            io_read_u16(REG_TM0_CONTROL),
        )
    }
}
