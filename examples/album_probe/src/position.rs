//! The 32-bit session clock — ALBUM-PLAYER.md §4.4.
//!
//! §10.4's ruling is the shape of this: one `u32` in `.iwram`, incremented in
//! the period IRQ, read by the UI without a critical section (an aligned 32-bit
//! read is atomic on this part), covering 2³² samples — 72.8 h at 16,384 Hz,
//! 36.4 h at 32,768, against an album that needs minutes. No 16-bit timer, no
//! rollover ledger, and timer3 stays free.
//!
//! The displayed position is the committed count minus one named lead:
//!
//! ```text
//! paint_position = committed_samples − LEAD_SAMPLES
//! ```
//!
//! The subtraction saturates at zero, which is what makes the invariants hold by
//! construction rather than by a caller checking: the number is 0 before the
//! lead has played out, it can never exceed what the producer committed, and it
//! can never move backwards within a track because `committed` never does. A
//! skip is the one deliberate discontinuity — the queue is dropped, the counter
//! is zeroed, and the new track starts at exactly 0 (§4.4: "track starts
//! exact"; the residual error is the constant lead, by design).

use crate::geometry;

/// The samples between what the feeder has handed to the DMA and what the ear is
/// hearing, as a fixed policy: the counter's zero is the moment playback starts,
/// and by then the pipe already holds the prefill.
///
/// It is the deepest the queue can get *after* that zero, i.e. the ring's
/// headroom above the prefill plus the chunk the sound DMA has already taken out
/// of the ring. Choosing the worst case (rather than tracking the queue) is
/// deliberate: a displayed position that occasionally ran *ahead* of the audio
/// would be a lie the UI cannot see, while a constant lag is a constant, and
/// §4.4 accepts exactly that residual.
pub const LEAD_SAMPLES: u32 =
    (geometry::RING_SAMPLES - geometry::PREFILL_SAMPLES + geometry::CHUNK_SAMPLES) as u32;

// The two bounds the choice rests on: the lead is at least the re-prime's
// silence plus the buffer in flight (so a skip does not show as a jump), and at
// most the ring plus one chunk (so it cannot exceed the geometry it describes).
const _: () = assert!(LEAD_SAMPLES >= geometry::PREFILL_SAMPLES as u32);
const _: () = assert!(LEAD_SAMPLES as usize <= geometry::RING_SAMPLES + geometry::CHUNK_SAMPLES);

/// The session clock: one monotonic `u32`, plus the lead policy above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionClock {
    committed: u32,
}

impl SessionClock {
    /// A clock at a track start: committed 0, so the position is 0.
    pub const fn new() -> Self {
        Self { committed: 0 }
    }

    /// Credit samples the feeder has handed to the sound DMA. Called once per
    /// period with one chunk (use [`Self::commit_chunk`]); adding at any other
    /// granularity is still correct, just less like the hardware.
    ///
    /// Wraps at 2³² samples by design (§10.4: 72.8 h at the product rate).
    pub fn commit(&mut self, samples: u32) {
        self.committed = self.committed.wrapping_add(samples);
    }

    /// Credit one hardware period.
    pub fn commit_chunk(&mut self) {
        self.commit(geometry::CHUNK_SAMPLES as u32);
    }

    /// The raw counter, for the ledger and for tests.
    pub const fn committed(&self) -> u32 {
        self.committed
    }

    /// The 32-bit position a bar or an `m:ss` label paints: committed minus the
    /// lead, saturating at 0.
    pub const fn paint_position(&self) -> u32 {
        self.committed.saturating_sub(LEAD_SAMPLES)
    }

    /// The position clamped to a track's length, so an end-of-track label reads
    /// the manifest's `n_samples` and never one past it.
    pub const fn paint_position_in_track(&self, n_samples: u32) -> u32 {
        let pos = self.paint_position();
        if pos < n_samples { pos } else { n_samples }
    }

    /// The skip's drop step: the queued chunks are gone and the new track starts
    /// at 0. Everything committed before this call belonged to the old track and
    /// is discarded with its queue — that is why this is a reset and not a
    /// subtraction.
    pub fn restart(&mut self) {
        self.committed = 0;
    }
}
