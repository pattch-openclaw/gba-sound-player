//! §6's arithmetic: the numbers the UI paints, computed without dividing.
//!
//! `ALBUM-PLAYER.md` §6 owns this. It lives in project source because three
//! things paint from it — the Phase 0 probe, the application, and the host gates
//! that hold both to it — and §2.2's rule is that what an app needs from a probe
//! is lifted into project source first, not `#[path]`-included from it. One
//! implementation, so a number a probe prints and a cell the app paints cannot
//! come from two arithmetics.
//!
//! The ARM7TDMI has no divide instruction, so `/` on a variable is a call into
//! `__aeabi_*div*` — ~50–150 cycles on a path with a 34 ms budget, and AP-2's
//! gate wording is a symbol fact on the linked image. Every quantity here is a
//! shift or a multiply-accumulate because the clocks are powers of two:
//! `2^14 = 16,384 Hz` and `2^32` of accumulator per unit.
//!
//! There is exactly **one** divide in this file, [`bar_recip`], and it is the
//! bake tool's: `floor(2^32 × BAR_CELLS / n_samples)` is computed once per track
//! on the host and carried in the manifest, so the ROM only ever multiplies. This
//! crate's host suite reads this file and refuses a second one — that is the
//! code-level half of the §6 rule; the linked-image half (`__aeabi_u*div*`) is
//! AP-2's.
//!
//! ## The clock is the link site's fact, not this module's
//!
//! [`RATE_LOG2`] is §6's arithmetic — both clocks are powers of two and the shift
//! is the whole trick — so it is stated here, once. What this module must not do
//! is *import* a geometry to check it with: the probe checks against the shipped
//! `examples/flac_integration/src/geometry.rs`, and the application will check
//! against the per-backend geometry §4.2b says it derives, so a shared module
//! naming either one would couple them through it. The tie is inverted: each
//! link site asserts it with its own `const _: () = assert!(… == 1 <<
//! nodiv::RATE_LOG2);`, which is one line per consumer and cannot be skipped by
//! the arithmetic itself.

/// log2 of the product rate: `1 << 14 == 16_384` Hz. §6's table has both rows
/// (`>> 14` at 16,384 Hz, `>> 15` at 32,768); the product path runs at 16,384, so
/// 14 is the one the ROM uses. Each consumer asserts that its own clock is this
/// power of two — see the module docs — so this stays an arithmetic fact rather
/// than a claim about someone else's geometry file.
pub const RATE_LOG2: u32 = 14;

/// Cells in the progress bar: §6's fixed grid of 32 equal cells (32 cells × 7 px
/// on a 256-px-wide bar; a 64-cell bar is the other priced option).
pub const BAR_CELLS: u32 = 32;

/// The reciprocal for `mm:ss`: `floor(2^32 / 60) + 1`, the constant §6 verified
/// exact for `sec < 2^20` (an album needs `sec < 2^12`; the margin is the point
/// of the constant, not a coincidence).
pub const MMSS_RECIP: u64 = 0x0444_4445;

/// The largest second count [`mmss`] is exact for.
pub const MMSS_EXACT_BELOW: u32 = 1 << 20;

/// Samples → whole seconds, one shift.
///
/// Truncating is intentional: the label shows whole seconds elapsed, and the bar
/// carries the sub-second resolution.
pub const fn seconds(pos_samples: u32) -> u32 {
    pos_samples >> RATE_LOG2
}

/// Seconds → `(minutes, seconds)` by reciprocal multiply: one UMULL and a
/// multiply-subtract, no divide. Exact for `sec < MMSS_EXACT_BELOW`.
pub const fn mmss(sec: u32) -> (u32, u32) {
    let m = ((sec as u64 * MMSS_RECIP) >> 32) as u32;
    let s = sec - m * 60;
    (m, s)
}

/// Samples → `(minutes, seconds)` in one call, for a line of text per repaint.
pub const fn label(pos_samples: u32) -> (u32, u32) {
    mmss(seconds(pos_samples))
}

/// The bake-time reciprocal for a track of `n_samples`: `floor(2^32 × BAR_CELLS /
/// n_samples)`. Host-side and per track — the *only* divide in this file, and it
/// never runs on the GBA.
///
/// # Panics
///
/// `n_samples == 0` has no bar; the bake tool refuses an empty track before it
/// ever calls this.
pub const fn bar_recip(n_samples: u32) -> u32 {
    (((BAR_CELLS as u64) << 32) / n_samples as u64) as u32 // bake-time divide
}

/// Position → bar cell: one UMULL and a shift. Never reaches `BAR_CELLS` for
/// `pos < n_samples`, never goes backwards while `pos` does not, and lands on
/// `BAR_CELLS - 1` at the last sample — the three properties the §6 bake-time
/// assertions name, proved over 1,000..2.6 M samples by the host suite.
///
/// The caller passes `pos` already clamped to the track
/// (`position::paint_position_in_track`), because a bar cell past the end is a
/// caller bug, not a length.
pub const fn bar_cell(pos: u32, recip: u32) -> u32 {
    ((pos as u64 * recip as u64) >> 32) as u32
}
