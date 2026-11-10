//! The free-running cycle counter, shared by the E5 ROM only.
//!
//! Copied from the calibration instrument (`examples/flac_spike/src/main.rs`,
//! the source of record for the method) so this crate adds no dependency on
//! that crate and cannot move its image. timer2 at /1 (one tick per CPU cycle
//! at 16.78 MHz) cascades into timer3; the composite read is low, high, low,
//! with one retry if the low half wrapped.
//!
//! Conventions inherited from the instrument: an empty-window calibration
//! (3 discarded + 5 reported) sets the overhead; every window runs inside
//! `critical_section` (IME off) with all logging outside it; a window that
//! wrapped the 32-bit composite is reported as [`u32::MAX`], loudly.

use agb::timer::Divider;

/// Free-running 32-bit cycle counter (timer2/timer3 cascade).
pub struct Counter<'a> {
    low: &'a mut agb::timer::Timer,
    high: &'a mut agb::timer::Timer,
}

impl<'a> Counter<'a> {
    /// Start the pair at /1 with the cascade on and zero it.
    pub fn start(timers: &'a mut agb::timer::Timers<'_>) -> Self {
        timers
            .timer2
            .set_divider(Divider::Divider1)
            .set_enabled(true);
        timers
            .timer3
            .set_cascade(true)
            .set_interrupt(false)
            .set_enabled(true);
        let mut counter = Self {
            low: &mut timers.timer2,
            high: &mut timers.timer3,
        };
        counter.soft_reset();
        counter
    }

    /// Freeze, zero both halves, unfreeze — so a pass never depends on
    /// accumulated overflow from an earlier pass.
    pub fn soft_reset(&mut self) {
        self.low.set_enabled(false);
        self.high.set_enabled(false);
        self.high.set_overflow_amount(0);
        self.low.set_overflow_amount(0);
        self.low.set_enabled(true);
        self.high.set_enabled(true);
    }

    /// The 32-bit composite, with the low/high/low stability re-read.
    pub fn read(&self) -> u32 {
        for _ in 0..4 {
            let low1 = self.low.value();
            let high = self.high.value();
            let low = self.low.value();
            if low >= low1 {
                return (u32::from(high) << 16) | u32::from(low);
            }
        }
        // Pathological (cannot happen with IRQs off against a 65,536-cycle
        // period); accept a bounded, deterministic value rather than loop.
        let high = self.high.value();
        let low = self.low.value();
        (u32::from(high) << 16) | u32::from(low)
    }
}

/// `end − start`, or [`u32::MAX`] if the composite wrapped inside the window.
pub fn cycles_between(start: u32, end: u32) -> u32 {
    if end >= start { end - start } else { u32::MAX }
}

/// Run `work` inside one IRQ-off window. Logging must stay outside it.
pub fn timed<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
    let mut raw = 0u32;
    let out = agb::external::critical_section::with(|_cs| {
        let start = counter.read();
        let out = work();
        let end = counter.read();
        raw = cycles_between(start, end);
        out
    });
    (out, raw)
}

/// Discarded and reported empty-window samples. The reported ones must all be
/// identical; otherwise the harness is unstable and the run's numbers are void.
pub const CALIBRATION_DISCARDED: usize = 3;
/// Reported calibration samples.
pub const CALIBRATION_REPORTED: usize = 5;

/// Empty-window calibration. Returns the overhead, or `None` if unstable.
pub fn calibrate(counter: &mut Counter<'_>) -> Option<u32> {
    const TOTAL: usize = CALIBRATION_DISCARDED + CALIBRATION_REPORTED;
    let mut calib = [0u32; TOTAL];
    for slot in calib.iter_mut() {
        // A volatile WAITCNT read is the probe: it keeps the closure from
        // being optimised away and is exactly the register this rung pins.
        let (_, raw) = timed(counter, || waitcnt());
        *slot = raw;
    }
    let overhead = calib[CALIBRATION_DISCARDED];
    let stable = calib[CALIBRATION_DISCARDED..]
        .iter()
        .all(|&raw| raw == overhead);
    if stable { Some(overhead) } else { None }
}

/// The GBA `WAITCNT` register (0x0400_0204).
pub fn waitcnt() -> u16 {
    // SAFETY: a 16-bit I/O register at an even address.
    unsafe { core::ptr::read_volatile(0x0400_0204 as *const u16) }
}

/// 1 when the cartridge prefetch buffer is enabled (`WAITCNT` bit 14).
pub fn prefetch_enabled(waitcnt: u16) -> bool {
    waitcnt & (1 << 14) != 0
}

/// Write `WAITCNT` (the full register). Callers restore the boot value.
pub fn set_waitcnt(value: u16) {
    // SAFETY: a 16-bit I/O register at an even address.
    unsafe { core::ptr::write_volatile(0x0400_0204 as *mut u16, value) };
}
