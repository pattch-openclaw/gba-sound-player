//! `position.rs` — the session clock (§10.4, ruled option A) and §4.4's painted position.
//!
//! ## The clock
//!
//! One `u32` in `.iwram`, incremented by the audio period IRQ with the number of samples that
//! period actually committed to the DMA. That is §10.4's ruled option A, and the price it carries
//! is stated there and paid here: **4 bytes** of `.iwram` and roughly one load/add/store per
//! 34.18 ms period (573,440 cycles). timer3 stays free, so `uli_bench`'s cycle-counter pair and any
//! future use of it stay available.
//!
//! Two consequences, both of them the reason this is a clock and not a computation:
//!
//! - **A read needs no critical section.** An aligned 32-bit load is a single `ldr`, and a single
//!   `ldr`/`str` of an aligned word is atomic on ARM7TDMI: a reader sees either the value before or
//!   the value after an increment, never a torn one. So the main loop reads
//!   [`committed_samples`] with one instruction and no interrupt masking — which matters because
//!   the main loop is the producer's path and the audio IRQ owns the tightest deadline in the
//!   system (§4.5).
//! - **It freezes with the chain.** It is incremented *in* the period IRQ, so a stopped timer stops
//!   the clock. PR-3b-ii's pause stops timers 0/1/2 without dropping the ring, so the displayed time
//!   holds still during a pause for free rather than by a compensating subtraction, and resumes
//!   where it was because nothing was dropped.
//!
//! Why a *value* counter and not a period counter: the let the last period of a track be a
//! classified tail (`Chunk::Tail { real }`), which commits fewer than a whole chunk. Adding the
//! number the feeder already has in hand keeps this clock equal to `RingFeed`'s `consumed` **by
//! construction**, so there is one ledger in the image rather than two that can drift. The ROM
//! prints both at the end of its run so the equality is observed rather than assumed.
//!
//! ## The painted position
//!
//! §4.4:
//!
//! ```text
//! paint_position = committed_samples − LEAD_SAMPLES      // one named constant
//! ```
//!
//! [`LEAD_SAMPLES`] is the audio that has been committed and is therefore not queued any more but
//! has not reached the listener yet: the two ping-pong DMA buffers plus the FIFO's half-full refill
//! window. It is derived from [`geometry`] rather than written down, so a second backend's chunk
//! size moves it automatically. The residual error is that constant lead during steady playback,
//! which §4.4 already ruled invisible in a bar or an `mm:ss` readout.
//!
//! Two clamps make §4.4's invariants true rather than approximately true:
//!
//! - at a track start `committed == 0`, so the subtraction saturates to **0** (the counter is reset
//!   at the drop — `RingFeed::truncate` and PR-3b-ii's track start own that, and the model here only
//!   says the map is exact when it does);
//! - at the track's end `committed == n_samples`, and the position is **`n_samples` exactly**, not
//!   `n_samples − LEAD_SAMPLES`. That is a deliberate one-period forward step over the last ~69 ms
//!   of a track: the alternative is a display that never reaches its own total, which AP-2 would
//!   have to explain instead.
//!
//! No divide, no float, no lookup table: `saturating_sub` and one compare.

use crate::geometry;

/// How many DMA buffers the chain alternates. Two, and it is a property of the ping-pong hand-off
/// rather than a tunable: `rearm_dma` swaps between the slot being played and the slot being filled.
pub const DMA_BUFFERS: u32 = 2;

/// The audio committed to the DMA but not yet audible: the buffers in the ping-pong plus the FIFO's
/// half-full window, in samples per channel.
///
/// `2 × 560 + 16 = 1,136` samples = **69.3 ms** at 16,384 Hz. The FIFO term is the same
/// [`geometry::REARM_GRACE_SAMPLES`] the period audit uses, so the two numbers that describe "how
/// much is in flight" cannot disagree. It is conservative by at most that window, which is the
/// direction a lead should err in: the display never claims to be ahead of what is heard.
pub const LEAD_SAMPLES: u32 = DMA_BUFFERS * geometry::CHUNK_SAMPLES + geometry::REARM_GRACE_SAMPLES;

/// What the position display shows for `committed` samples of a `total`-sample track.
///
/// See the module docs for the two clamps and why each is there. `total == 0` is a track with no
/// samples: 0, which is also its end.
#[must_use]
pub const fn paint_position(committed: u32, total: u32) -> u32 {
    if committed >= total {
        total
    } else {
        committed.saturating_sub(LEAD_SAMPLES)
    }
}

// -------------------------------------------------------------------------------------------------
// The arm half: the counter itself, and the only two functions that touch it
// -------------------------------------------------------------------------------------------------

#[cfg(target_arch = "arm")]
mod clock {
    /// §10.4's ruled option A: one `u32` in `.iwram`.
    ///
    /// The placement attribute is the tree's existing idiom for a hot static
    /// (`crates/uli-lite-gba/src/lib.rs:94`, `crates/flac-lite/src/bits.rs:924`): `.iwram` is the
    /// plain GBA link-script name, never `.text_iwram`, which `.text*(.text*)` would capture back
    /// into ROM. Why here rather than in `.bss`: this is written from an interrupt handler, and
    /// EWRAM reads and writes carry wait states; the 4 bytes sit in the UI's own allocation, which
    /// is where §10.3 says affordances belong.
    ///
    /// `static mut` rather than an atomic because thumbv4t declares no atomics — agb itself uses
    /// `portable_atomic` for that reason — and because an aligned 32-bit load/store is what the
    /// atomicity argument in the module docs is about. Neither function ever forms a reference, so
    /// this is a copy in and a copy out.
    #[unsafe(link_section = ".iwram")]
    static mut COMMITTED: u32 = 0;

    /// One period's committed samples, from the audio IRQ.
    ///
    /// `wrapping_add` because the alternative is a panic path in an interrupt handler; a wrap is
    /// 2³² samples = 72.6 hours at 16,384 Hz, long past the album's own end.
    #[inline]
    pub fn commit(samples: u32) {
        // SAFETY: single-threaded writer (the audio IRQ is the only caller) and an aligned 32-bit
        // read-modify-write is atomic on ARM7TDMI; no other context writes this cell.
        unsafe { COMMITTED = COMMITTED.wrapping_add(samples) };
    }

    /// The session clock, readable with no critical section (module docs).
    #[inline]
    #[must_use]
    pub fn committed_samples() -> u32 {
        // SAFETY: read only; an aligned 32-bit load is atomic on ARM7TDMI.
        unsafe { COMMITTED }
    }
}

#[cfg(target_arch = "arm")]
pub use clock::{commit, committed_samples};
