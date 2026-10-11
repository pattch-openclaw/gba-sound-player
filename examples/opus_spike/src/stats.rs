//! Per-packet cost fold — running min / max / sum / worst-packet index.
//!
//! The cycle-harness fold module, ported from flac_spike's PR 3
//! `stats.rs` under this repo's shared-consumer rule: the ROM-side
//! stats fold lives in this **library**, is `no_std` pure-Rust with no
//! MMIO, and the host gate (`make opus-test`) drives the *same
//! functions* over hand-built sequences. The ROM calls this one
//! implementation, so host and ROM cannot share a transcription mistake
//! of the fold rules (tie convention, worst-case wrap arithmetic) with
//! a screen verdict as the only oracle.
//!
//! The fold is deliberately **division-free**: the ROM reports sum +
//! count and the mean is derived as arithmetic (host-side, and written
//! out in the OPUS.md step entry as long division). This keeps the
//! measurement path under the same rule the decode path follows — no
//! software divide anywhere near the numbers being measured (the
//! ARM7TDMI has none).
//!
//! One deliberate difference from flac_spike's module: no
//! `real_time_budget` helper lives here. FLAC's budget is a function of
//! the frame blocksize/rate the driver reports; the Opus profile pins
//! one geometry (20 ms × 960 samples at 48 kHz), so the budget is the
//! single derived constant `probe::BUDGET_CYCLES_PER_PACKET` — and the
//! unit tests below cross-pin it against an *independent*
//! samples-based derivation (960 × clock / 48 000), the two derivations
//! agreeing is the witness.
//!
//! Overflow model: cycles accumulate in `u64`, so even a pathological
//! 2^32-cycle window cannot wrap the sum. A *window* larger than u32
//! (the timer pair wrapped mid-decode) saturates into
//! [`WORST_WRAPPED`] — that packet is reported as worst and the flag
//! fails the ROM's serial checkpoint loudly rather than silently
//! truncating a cycle count.

/// Sentinel window value meaning "the counter pair wrapped before the
/// window closed" (raw > u32::MAX). Not a real cycle count; it makes
/// the packet worst-by-construction and trips the harness's wrap
/// checkpoint.
pub const WORST_WRAPPED: u32 = u32::MAX;

/// Saturating difference of two free-running 32-bit counter readings.
///
/// A window that never wraps gives the exact elapsed count. A window
/// whose free-running pair wrapped mid-decode cannot be recovered from
/// two reads, so it saturates to [`WORST_WRAPPED`] instead of silently
/// reporting the modulo — a truncated cycle count would understate the
/// worst packet, one of the two numbers this gate exists to catch.
#[must_use]
pub fn cycles_between(start: u32, end: u32) -> u32 {
    if end >= start {
        end - start
    } else {
        WORST_WRAPPED
    }
}

/// Running min / max / sum / worst-packet-index over per-packet cycle
/// windows.
///
/// Ties: the **first** (lowest-index) packet holds min and worst — the
/// fold updates only on strict `<` / `>`. With 501 packets per arm this
/// is a stated convention, not a coin flip: the host tests pin it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketCost {
    /// Number of packets observed so far.
    pub count: usize,
    /// Smallest window in cycles.
    pub min: u32,
    /// Index of the smallest window (first wins ties).
    pub min_packet: usize,
    /// Largest window in cycles.
    pub max: u32,
    /// Index of the largest window (first wins ties).
    pub max_packet: usize,
    /// Sum of all windows in cycles (u64: never wraps at this scale).
    pub sum: u64,
    /// True if any window saturated via [`cycles_between`].
    pub wrapped: bool,
}

impl Default for PacketCost {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketCost {
    /// An empty fold: zero packets, no extremes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            min: u32::MAX,
            min_packet: 0,
            max: 0,
            max_packet: 0,
            sum: 0,
            wrapped: false,
        }
    }

    /// Fold one packet's window into the running stats.
    pub fn observe(&mut self, packet: usize, cycles: u32) {
        self.count += 1;
        self.sum += u64::from(cycles);
        self.wrapped |= cycles == WORST_WRAPPED;
        if cycles < self.min {
            self.min = cycles;
            self.min_packet = packet;
        }
        if cycles > self.max {
            self.max = cycles;
            self.max_packet = packet;
        }
    }

    /// Arithmetic mean in cycles per packet, or `None` for an empty
    /// fold.
    ///
    /// ARM7TDMI has no hardware divide, so the **ROM must not call
    /// this**; it belongs to host-side derivation (and to any future
    /// host tooling). The ROM reports sum + count and the mean is
    /// arithmetic on paper.
    #[must_use]
    pub fn mean(&self) -> Option<u64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum / self.count as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_between_exact_no_wrap() {
        assert_eq!(cycles_between(0, 0), 0);
        assert_eq!(cycles_between(7, 7), 0);
        assert_eq!(cycles_between(100, 141), 41);
        assert_eq!(cycles_between(0, u32::MAX), u32::MAX);
    }

    #[test]
    fn cycles_between_saturates_on_wrap() {
        // end < start can only mean the free-running pair wrapped
        // mid-window: report worst, never the modulo difference (which
        // would understate).
        assert_eq!(cycles_between(u32::MAX, 5), WORST_WRAPPED);
        assert_eq!(cycles_between(0xFFFF_FFF0, 0x10), WORST_WRAPPED);
        // The sentinel is distinguishable from any exact window except
        // the single degenerate 2^32-1-cycle one, which is itself
        // ~6.4 minutes — in this harness it can only arise from wrap or
        // a stuck counter.
        assert_eq!(cycles_between(0, WORST_WRAPPED), WORST_WRAPPED);
    }

    #[test]
    fn fold_min_max_sum_worst_index() {
        let mut f = PacketCost::new();
        for (i, c) in [10u32, 300, 50, 300, 7].iter().enumerate() {
            f.observe(i, *c);
        }
        assert_eq!(f.count, 5);
        assert_eq!(f.min, 7);
        assert_eq!(f.min_packet, 4);
        assert_eq!(f.max, 300);
        assert_eq!(f.max_packet, 1, "first max wins ties");
        assert_eq!(f.sum, 667); // 10+300+50+300+7, derived not recalled
        assert!(!f.wrapped);
    }

    #[test]
    fn fold_ties_and_single_packet() {
        let mut f = PacketCost::new();
        f.observe(0, 42);
        assert_eq!(
            (f.count, f.min, f.min_packet, f.max, f.max_packet),
            (1, 42, 0, 42, 0)
        );
        f.observe(1, 42);
        assert_eq!(f.max_packet, 0, "later ties never move the index");
        assert_eq!(f.min_packet, 0);
        // Strictly smaller does move it.
        f.observe(2, 41);
        assert_eq!((f.min, f.min_packet), (41, 2));
    }

    #[test]
    fn fold_wrapped_flag_and_sum_width() {
        let mut f = PacketCost::new();
        f.observe(0, WORST_WRAPPED);
        assert!(f.wrapped);
        assert_eq!(f.max, WORST_WRAPPED);
        assert_eq!(f.max_packet, 0);
        f.observe(1, WORST_WRAPPED);
        assert_eq!(f.sum, 2 * u64::from(WORST_WRAPPED));
        assert_eq!(f.max_packet, 0, "tie stays first even at the sentinel");
    }

    #[test]
    fn mean_is_arithmetic_and_empty_is_none() {
        let mut f = PacketCost::new();
        assert_eq!(f.mean(), None);
        for (i, c) in [10u32, 20, 30].iter().enumerate() {
            f.observe(i, *c);
        }
        assert_eq!(f.mean(), Some(20));
        // Truncation direction is pinned (floor, like integer division):
        let mut f = PacketCost::new();
        f.observe(0, 1);
        f.observe(1, 2);
        assert_eq!(f.mean(), Some(1));
    }

    #[test]
    fn budget_cross_pins_an_independent_derivation() {
        // The seam derives BUDGET_CYCLES_PER_PACKET from the frame
        // duration (20 ms × 16.78 MHz / 1000). Derive the same number
        // independently from the sample geometry — 960 samples at
        // 48 kHz: 960 × 16,780,000 / 48,000 = 335,600 exactly (derived
        // here, not recalled; OPUS.md plan item 6). Two independent
        // derivations agreeing is the witness; the per-sample figure
        // 335,600 / 960 = 349.58… is the "≈ 350 c/sample" the plan
        // quotes, never an integer at this geometry.
        const FROM_SAMPLES: u64 =
            crate::probe::frame_samples(48_000) as u64 * crate::probe::GBA_CLOCK_HZ / 48_000;
        assert_eq!(
            u64::from(crate::probe::BUDGET_CYCLES_PER_PACKET),
            FROM_SAMPLES
        );
        assert_eq!(crate::probe::BUDGET_CYCLES_PER_PACKET, 335_600);
    }
}
