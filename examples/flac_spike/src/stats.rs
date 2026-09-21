//! Per-frame cost fold — running min / max / sum / worst-frame index.
//!
//! PR 3's shared-fold module, following PR 2's `checksum::fold_i16le_stereo`
//! rule: the ROM-side stats fold lives in this **library**, is `no_std`
//! pure-Rust with no MMIO, and the host witness (`make spike-test`) drives
//! the *same functions* through hand-built sequences and the *same driver*
//! over the embedded clips. The ROM calls this one implementation, so host
//! and ROM cannot share a transcription mistake of the fold rules (tie
//! convention, worst-case wrap arithmetic) with a screen verdict as the only
//! oracle.
//!
//! The fold is deliberately **division-free**: the ROM reports sum + count
//! and the mean is derived as arithmetic (host-side, and written out in the
//! step entry as long division). This keeps the measurement path under the
//! same rule the decode path follows — no software divide anywhere near the
//! numbers being measured.
//!
//! Overflow model: cycles accumulate in `u64`, so even a pathological
//! 2^32-cycle window cannot wrap the sum. A *window* larger than u32 (the
//! timer pair wrapped mid-decode) saturates into [`WORST_WRAPPED`] — that
//! frame is reported as worst and the flag fails the ROM's serial checkpoint
//! loudly rather than silently truncating a cycle count.

/// Sentinel window value meaning "the counter pair wrapped before the
/// window closed" (raw > u32::MAX). Not a real cycle count; it makes the
/// frame worst-by-construction and trips the harness's wrap checkpoint.
pub const WORST_WRAPPED: u32 = u32::MAX;

/// The GBA machine clock: 16.78 MHz — the reference the free-running
/// timer/div counter counts against.
pub const GBA_CLOCK_HZ: u64 = 16_780_000;

/// Real-time allowance in CPU cycles for one frame of `blocksize` samples
/// encoded at `sample_rate_hz` — the number a measured decode window must
/// fit. `blocksize / sample_rate` is the frame's audio duration; × clock.
/// Multiplies in u64 first, then divides once — and only ever at compile
/// time in the ROM (`const fn`), so no software divide ever runs on the
/// measured path; the ARM7TDMI has none.
///
/// Canonical profile value (FLAC.md's plan arithmetic, derived not
/// recalled): 2048 @ 32,768 Hz → 1,048,750 cycles/frame (exactly
/// 512.0849609375 cycles/sample — the plan's "≈ 512" is an approximation;
/// only the frame-level number is integral at this rate). Panics for
/// `sample_rate_hz == 0`.
#[must_use]
pub const fn real_time_budget(blocksize: usize, sample_rate_hz: u32) -> u32 {
    let cycles = blocksize as u64 * GBA_CLOCK_HZ / sample_rate_hz as u64;
    cycles as u32
}

/// Saturating difference of two free-running 32-bit counter readings.
///
/// A window that never wraps gives the exact elapsed count. A window whose
/// free-running pair wrapped mid-decode cannot be recovered from two reads,
/// so it saturates to [`WORST_WRAPPED`] instead of silently reporting the
/// modulo — a truncated cycle count would understate the worst frame, the
/// one number this gate exists to catch.
#[must_use]
pub fn cycles_between(start: u32, end: u32) -> u32 {
    if end >= start {
        end - start
    } else {
        WORST_WRAPPED
    }
}

/// Running min / max / sum / worst-frame-index over per-frame cycle windows.
///
/// Ties: the **first** (lowest-index) frame holds min and worst — the fold
/// updates only on strict `<` / `>`. With 157 frames per arm this is a
/// stated convention, not a coin flip: the host tests pin it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCost {
    /// Number of frames observed so far.
    pub count: usize,
    /// Smallest window in cycles.
    pub min: u32,
    /// Index of the smallest window (first wins ties).
    pub min_frame: usize,
    /// Largest window in cycles.
    pub max: u32,
    /// Index of the largest window (first wins ties).
    pub max_frame: usize,
    /// Sum of all windows in cycles (u64: never wraps at this scale).
    pub sum: u64,
    /// True if any window saturated via [`cycles_between`].
    pub wrapped: bool,
}

impl Default for FrameCost {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCost {
    /// An empty fold: zero frames, no extremes.
    #[must_use]
    pub const fn new() -> Self {
        FrameCost {
            count: 0,
            min: u32::MAX,
            min_frame: 0,
            max: 0,
            max_frame: 0,
            sum: 0,
            wrapped: false,
        }
    }

    /// Fold one frame's window into the running stats.
    pub fn observe(&mut self, frame: usize, cycles: u32) {
        self.count += 1;
        self.sum += u64::from(cycles);
        self.wrapped |= cycles == WORST_WRAPPED;
        if cycles < self.min {
            self.min = cycles;
            self.min_frame = frame;
        }
        if cycles > self.max {
            self.max = cycles;
            self.max_frame = frame;
        }
    }

    /// Arithmetic mean in cycles per frame, or `None` for an empty fold.
    ///
    /// ARM7TDMI has no hardware divide, so the **ROM must not call this**;
    /// it belongs to host-side derivation (and to any future host tooling).
    /// The ROM reports sum + count and the mean is arithmetic on paper.
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
        // end < start can only mean the free-running pair wrapped mid-window:
        // report worst, never the modulo difference (which would understate).
        assert_eq!(cycles_between(u32::MAX, 5), WORST_WRAPPED);
        assert_eq!(cycles_between(0xFFFF_FFF0, 0x10), WORST_WRAPPED);
        // The sentinel is distinguishable from any exact window except the
        // single degenerate 2^32-1-cycle one, which is itself ~6.4 minutes —
        // in this harness it can only arise from wrap or a stuck counter.
        assert_eq!(cycles_between(0, WORST_WRAPPED), WORST_WRAPPED);
    }

    #[test]
    fn fold_min_max_sum_worst_index() {
        let mut f = FrameCost::new();
        for (i, c) in [10u32, 300, 50, 300, 7].iter().enumerate() {
            f.observe(i, *c);
        }
        assert_eq!(f.count, 5);
        assert_eq!(f.min, 7);
        assert_eq!(f.min_frame, 4);
        assert_eq!(f.max, 300);
        assert_eq!(f.max_frame, 1, "first max wins ties");
        assert_eq!(f.sum, 667); // 10+300+50+300+7, derived not recalled
        assert!(!f.wrapped);
    }

    #[test]
    fn fold_ties_and_single_frame() {
        let mut f = FrameCost::new();
        f.observe(0, 42);
        assert_eq!(
            (f.count, f.min, f.min_frame, f.max, f.max_frame),
            (1, 42, 0, 42, 0)
        );
        f.observe(1, 42);
        assert_eq!(f.max_frame, 0, "later ties never move the index");
        assert_eq!(f.min_frame, 0);
        // Strictly smaller does move it.
        f.observe(2, 41);
        assert_eq!((f.min, f.min_frame), (41, 2));
    }

    #[test]
    fn fold_wrapped_flag_and_sum_width() {
        let mut f = FrameCost::new();
        f.observe(0, WORST_WRAPPED);
        assert!(f.wrapped);
        assert_eq!(f.max, WORST_WRAPPED);
        assert_eq!(f.max_frame, 0);
        f.observe(1, WORST_WRAPPED);
        assert_eq!(f.sum, 2 * u64::from(WORST_WRAPPED));
        assert_eq!(f.max_frame, 0, "tie stays first even at the sentinel");
    }

    #[test]
    fn real_time_budget_matches_the_plan_arithmetic() {
        // FLAC.md plan: 2048 / 32768 s × 16.78 MHz = 1,048,750 cycles/frame
        // (16,780,000 / 32,768 = 512.0849609375 cycles/sample exactly — the
        // plan's "≈ 512" is an approximation; only the frame-level number
        // is integral at this rate). The clips encode at 32,000 Hz, whose
        // true window (1,073,920) is 2.4% looser — the plan keeps the
        // stricter 32,768-Hz budget deliberately.
        assert_eq!(real_time_budget(2048, 32_768), 1_048_750);
        assert_eq!(real_time_budget(2048, 32_000), 1_073_920);
        // Sub-frame windows truncate toward zero at both rates (computed
        // with b × 16,780,000 / r, not recalled):
        // 100 × 16,780,000 / 32,768 = 51,208.49609375 → 51,208
        assert_eq!(real_time_budget(100, 32_768), 51_208);
        // 100 × 16,780,000 / 32,000 = 52,437.5 → 52,437
        assert_eq!(real_time_budget(100, 32_000), 52_437);
        assert_eq!(real_time_budget(1, 16_780_000), 1);
        assert_eq!(real_time_budget(0, 32_768), 0);
    }

    #[test]
    fn mean_is_arithmetic_and_empty_is_none() {
        let mut f = FrameCost::new();
        assert_eq!(f.mean(), None);
        for (i, c) in [10u32, 20, 30].iter().enumerate() {
            f.observe(i, *c);
        }
        assert_eq!(f.mean(), Some(20));
        // Truncation direction is pinned (floor, like integer division):
        let mut f = FrameCost::new();
        f.observe(0, 1);
        f.observe(1, 2);
        assert_eq!(f.mean(), Some(1));
    }
}
