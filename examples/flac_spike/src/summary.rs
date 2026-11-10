//! # 6a-7b — the summary the 6a-7 STACK ON HW ROM prints
//!
//! 6a-7b puts the three-leg 6a-7 image (`ctrl` i32, `wide` vendored i64, `asm`
//! the landed kernel over the i32 default) in front of a real ARM7TDMI. E5
//! arbitrated the *fetch* half of the hardware question; the bit reader's
//! cartridge-ROM byte reads, the pinned tables, residual and stereo are still
//! mGBA-only, and this rung is the reading that closes them.
//!
//! This module is the **pure** half of that reading: the fold the pass leaves
//! behind, and the text both channels print. It lives in the library rather
//! than in the ROM for the reason `stats` and `checksum` do — host and ROM must
//! run one implementation, so a transcription mistake of the sign conventions
//! cannot hide behind a screen nothing on the host can read:
//!
//! * the ROM renders exactly this string and logs it line for line, so the
//!   serial log of an mGBA run and the pixels on a stock GBA cannot disagree;
//! * `make item6a7-test` runs the host tests below, which pin the label rule,
//!   the fixed-point formatter, and the two delta signs the record quotes.
//!
//! The `agb` half (font, layout, fit check) is `examples/flac_spike/src/
//! item6a7b.rs`, inside `mod rom`, and compiles only for the GBA target.
//!
//! ## Why the whole thing is gated on `item6a7-wide`
//!
//! Everything here is behind the 6a-7 image's own feature, so the **default**
//! instrument image (`make native-spike-rom`) and the **6a-5 / 6a-5b** images
//! are byte-identical to what their records quote. The 6a-7 image changes,
//! necessarily, and that is stated rather than hidden. The rule that follows:
//! the hardware numbers are compared against an **mGBA run of this same image**,
//! never against the pre-summary 6a-7 record — adding code to a link can move a
//! decode path's layout (6a-3 measured ~168–199 K cycles/frame of exactly that),
//! and a comparison across two images is the one thing this series forbids.
//!
//! ## The screen's verdict is validity, not budget
//!
//! The spike's `pass` ANDs the budget check, and the LPC clip is 167% of the
//! 32,768 Hz frame *by design* — a screen coloured from `pass` would be red on a
//! perfectly valid measurement. [`render`]'s `harness_ok` therefore means "the
//! numbers above come from an image whose assets, decode proofs and calibration
//! all passed", the only thing a hardware reader can act on. The budget verdict
//! stays where it always was: on serial.

use crate::assets::{CLIPS, SpikeClip};
use crate::stats::FrameCost;
use alloc::format;
use alloc::string::String;

/// One leg's folded cost: what the summary needs and nothing else.
///
/// Deliberately not `FrameCost`: the summary is a *copy* taken after the pass
/// has finished, so nothing the screen holds can be written to by a later
/// window and the measurement's own fold keeps its single owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leg {
    /// Sum of net per-frame windows, cycles — the undivided number.
    pub sum: u64,
    /// Worst (largest) net window, cycles: the frame compared to the budget.
    pub max: u32,
    /// Frames folded into `sum`.
    pub count: usize,
}

impl Leg {
    /// Copy a finished [`FrameCost`] into this leg.
    #[must_use]
    pub const fn of(cost: &FrameCost) -> Self {
        Self {
            sum: cost.sum,
            max: cost.max,
            count: cost.count,
        }
    }

    /// Mean cycles per frame, `None` on an empty fold.
    ///
    /// `FrameCost`'s docs forbid the ROM from calling `mean()`; this is the
    /// scoped exception the rung's plan takes, and the scope is exact: it runs
    /// only from [`render`], after every timed window has closed, on values
    /// already folded. No measurement path divides, and the undivided `sum` /
    /// `count` remain on serial (`history` below) for anyone who wants the
    /// arithmetic from the raw numbers. E5's ROM already divides in its
    /// reporting path for the same reason (`rom_per_add_x1000`).
    #[must_use]
    pub fn mean(&self) -> Option<u64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum / self.count as u64)
        }
    }
}

/// One clip's three legs, as its pass leaves them.
#[derive(Clone, Copy, Debug)]
pub struct ClipSummary {
    /// Frames timed — the divisor of every mean below.
    pub count: usize,
    /// The frame budget the "%" column divides by (32,768 Hz, 2048 samples).
    pub budget: u32,
    /// The i32 product path (`TableCountKernel`): 6a-7's control and the
    /// baseline both marginals are read against.
    pub ctrl: Leg,
    /// The vendored pre-6a-6 i64 path: `wide − ctrl` is 6a-6's width marginal.
    pub wide: Leg,
    /// The landed ARM kernel over the i32 default: `ctrl − asm` is the stack.
    pub asm: Leg,
}

impl ClipSummary {
    /// `ctrl − asm`: positive when the ARM kernel is the faster realization —
    /// 6a-7's *stack*.
    #[must_use]
    pub fn stack(&self) -> Option<i64> {
        match (self.ctrl.mean(), self.asm.mean()) {
            (Some(ctrl), Some(asm)) => Some(ctrl as i64 - asm as i64),
            _ => None,
        }
    }

    /// `wide − ctrl`: positive when the vendored i64 leg costs more than the
    /// i32 default — 6a-6's *width marginal*, which 6a-7 read as +44.1 c/sample.
    #[must_use]
    pub fn width(&self) -> Option<i64> {
        match (self.wide.mean(), self.ctrl.mean()) {
            (Some(wide), Some(ctrl)) => Some(wide as i64 - ctrl as i64),
            _ => None,
        }
    }

    /// Worst `asm` frame as a PERCENT, scaled by 1000 (so `39_960` is
    /// "39.960 %"), truncated — the `X` column, computed the way
    /// `stats::real_time_budget` is (multiply first, divide once).
    ///
    /// The 100_000 factor is `100 × 1000`: percent, then thousandths of a
    /// percent, which is the precision the 6a-7 record quotes (39.96 / 167.85).
    /// Getting this wrong by one factor of 100 is exactly the kind of quiet
    /// arithmetic error a screen can carry for a whole session, which is why
    /// the host test below pins a number whose percent is already known.
    #[must_use]
    pub fn worst_percent_x1000(&self) -> u64 {
        u64::from(self.asm.max) * 100_000 / u64::from(self.budget.max(1))
    }
}

/// The per-clip slots, keyed by `CLIPS` index.
///
/// A fixed array rather than a `Vec`: the instrument has exactly two clips, and
/// a form that cannot grow is a form that cannot silently gain a third block the
/// screen has no room for.
pub struct Summaries {
    slots: [Option<ClipSummary>; CLIPS.len()],
}

impl Default for Summaries {
    fn default() -> Self {
        Self::new()
    }
}

impl Summaries {
    /// No slots filled: every clip renders `VOID` until its pass records.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [None; CLIPS.len()],
        }
    }

    /// Record one clip's summary. Called once per clip, after its pass has
    /// completed and every window has closed.
    pub fn record(&mut self, slot: usize, summary: ClipSummary) {
        if let Some(slot) = self.slots.get_mut(slot) {
            *slot = Some(summary);
        }
    }

    /// Each slot's summary in `CLIPS` order, paired with its clip.
    pub fn iter(&self) -> impl Iterator<Item = (&'static SpikeClip, Option<&ClipSummary>)> {
        CLIPS
            .iter()
            .enumerate()
            .map(move |(i, clip)| (*clip, self.slots[i].as_ref()))
    }
}

/// `x.yyy` from a value scaled by 1000, without floating point (E5's helper and
/// its truncation direction).
#[must_use]
pub fn fixed3(value: u64) -> String {
    format!("{}.{:03}", value / 1000, value % 1000)
}

/// The clip's short label: its name before the first `_` — `l0`, `l4`. The full
/// name and the arm's `why` line go to serial, where there is room for them.
#[must_use]
pub fn short_label(name: &str) -> &str {
    match name.split_once('_') {
        Some((head, _)) => head,
        None => name,
    }
}

/// Screen-space budget of the summary, in lines: three header lines, three per
/// clip, one verdict. A font with `line_height() == 12` fits 12 lines in 160 px,
/// so this is the check that keeps a later edit from silently pushing the
/// verdict line off the bottom.
pub const SCREEN_LINES: usize = 3 + 3 * CLIPS.len() + 1;

/// Build the text both channels print.
///
/// `harness_ok` is validity, not budget — see the module docs.
#[must_use]
pub fn render(summaries: &Summaries, harness_ok: bool) -> String {
    let mut text = String::new();
    text.push_str("6A7B STACK ON HW\n");
    text.push_str("C=ctrl W=wide A=asm\n");
    text.push_str("S=C-A Wd=W-C X=worst%\n");
    for (clip, summary) in summaries.iter() {
        let label = short_label(clip.name);
        let Some(summary) = summary else {
            text.push_str(&format!("{} VOID\n", label));
            continue;
        };
        let (Some(ctrl), Some(wide), Some(asm)) =
            (summary.ctrl.mean(), summary.wide.mean(), summary.asm.mean())
        else {
            text.push_str(&format!("{} VOID\n", label));
            continue;
        };
        let (stack, width) = match (summary.stack(), summary.width()) {
            (Some(stack), Some(width)) => (stack, width),
            _ => {
                text.push_str(&format!("{} VOID\n", label));
                continue;
            }
        };
        text.push_str(&format!(
            "{} {}f C{} W{}\n",
            label, summary.count, ctrl, wide
        ));
        text.push_str(&format!("   A{} S{}\n", asm, stack));
        text.push_str(&format!(
            "   Wd{} X{}\n",
            width,
            fixed3(summary.worst_percent_x1000())
        ));
    }
    text.push_str(if harness_ok {
        "HARNESS OK"
    } else {
        "HARNESS FAILED"
    });
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(ctrl: u64, wide: u64, asm: u64, max: u32) -> ClipSummary {
        ClipSummary {
            count: 2,
            budget: 1_048_750,
            ctrl: Leg {
                sum: ctrl * 2,
                max,
                count: 2,
            },
            wide: Leg {
                sum: wide * 2,
                max,
                count: 2,
            },
            asm: Leg {
                sum: asm * 2,
                max,
                count: 2,
            },
        }
    }

    #[test]
    fn short_label_is_the_name_before_the_first_underscore() {
        assert_eq!(short_label("l0_stereo"), "l0");
        assert_eq!(short_label("l4_stereo"), "l4");
        assert_eq!(short_label("bare"), "bare");
    }

    #[test]
    fn fixed3_formats_thousandths_without_floating_point() {
        assert_eq!(fixed3(39_960), "39.960");
        assert_eq!(fixed3(167_850), "167.850");
        assert_eq!(fixed3(0), "0.000");
        assert_eq!(fixed3(5), "0.005");
    }

    #[test]
    fn leg_mean_is_sum_over_count_and_empty_is_none() {
        let leg = Leg {
            sum: 667,
            max: 300,
            count: 5,
        };
        assert_eq!(leg.mean(), Some(133), "integer division truncates");
        assert_eq!(
            Leg {
                sum: 0,
                max: 0,
                count: 0
            }
            .mean(),
            None
        );
    }

    #[test]
    fn stack_is_ctrl_minus_asm_and_width_is_wide_minus_ctrl() {
        // The 6a-7 FIXED means, rounded to whole cycles: the kernel wins the
        // stack and the i64 leg costs more. Both signs are the record's.
        let s = summary(1_662_788, 1_752_704, 416_804, 419_048);
        assert_eq!(s.stack(), Some(1_245_984));
        assert_eq!(s.width(), Some(89_916));
        assert!(s.stack().unwrap() > 0, "positive stack = kernel faster");
        assert!(s.width().unwrap() > 0, "positive width = i64 costs more");
        // The X column is the asm leg against the budget as a PERCENT ×1000,
        // truncated: 419,048 / 1,048,750 = 39.9569… %, so 39,956 → "39.956".
        // The 6a-7 record's own FIXED worst frame read 39.96 %, the same number
        // to the two decimals it was quoted at. Pinned here because an
        // off-by-100 in this factor (percent vs per-mille) is invisible on a
        // screen and this is the only place that catches it.
        assert_eq!(s.worst_percent_x1000(), 39_956);
        assert_eq!(fixed3(s.worst_percent_x1000()), "39.956");
    }

    #[test]
    fn render_names_both_clips_and_carries_the_verdict() {
        let mut summaries = Summaries::new();
        summaries.record(0, summary(1_662_788, 1_752_704, 416_804, 419_048));
        summaries.record(1, summary(2_987_753, 6_573_096, 1_751_090, 1_760_036));
        let text = render(&summaries, true);
        let lines: alloc::vec::Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), SCREEN_LINES, "the screen budget is exact");
        assert_eq!(lines[0], "6A7B STACK ON HW");
        assert!(text.contains("l0 2f C1662788 W1752704"), "{text}");
        assert!(text.contains("   A416804 S1245984"), "{text}");
        assert!(text.contains("   Wd89916 X39.956"), "{text}");
        assert!(text.contains("l4 2f C2987753 W6573096"), "{text}");
        // 1,760,036 / 1,048,750 = 167.8222… % — the LPC worst frame was quoted
        // as 167.85 in the 6a-7 record from the unrounded maximum, so this pins
        // the formatter's own value rather than a recalled decimal.
        assert!(text.contains("   Wd3585343 X167.822"), "{text}");
        assert_eq!(lines[lines.len() - 1], "HARNESS OK");

        // The void path is one line per clip, and the verdict is independent of
        // how many clips reported: a missing leg can never silently shorten the
        // screen into something a reader mistakes for a full run.
        let text = render(&Summaries::new(), false);
        assert!(text.contains("l0 VOID\n"));
        assert!(text.contains("l4 VOID\n"));
        assert!(text.ends_with("HARNESS FAILED"));
        // Three header lines, one line per void clip, one verdict — shorter than
        // `SCREEN_LINES` because a void clip needs one line where a reporting
        // one needs three.
        assert_eq!(text.lines().count(), 3 + CLIPS.len() + 1);
    }
}
