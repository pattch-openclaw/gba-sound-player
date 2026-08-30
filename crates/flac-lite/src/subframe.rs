//! Subframe decoding: CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤ 32).
//!
//! SCAFFOLD STATE: types and signatures only, bodies are `todo!()`.
//!
//! A subframe is one channel's worth of samples for one frame. Layout:
//!
//! ```text
//! [1 zero pad][subframe type][warm-up samples][optional wasted bits][residual]
//! ```
//!
//! Reconstruction is: decode the residual, then integrate it through the
//! predictor. Warm-up samples are the previous frame's tail samples and are the
//! only cross-frame state the decoder must retain (see [`crate::decoder`]).
//!
//! Implementation notes:
//!
//! - **Wasted bits ("blocking strategy"):** after the type field, a run of zero
//!   bits signals `wasted = count + 1` unused LSBs; decode then left-shift them
//!   back in. Byte-align after this field only when `wasted > 1` (verbatim
//!   subframes with wasted bits align before the samples).
//! - **FIXED orders 0–4** have *known* predictor coefficients
//!   (`[]`, `[1]`, `[2,-1]`, `[3,-3,1]`, `[4,-6,4,-1]`) — the integration is a
//!   cascade of running sums, not a coefficient dot product. Implement as
//!   nested accumulators, which is both faster and avoids multiplies entirely.
//! - **LPC:** `precision` field maps `0b00 → 15-bit, 0b01 → 16-bit,
//!   0b10..0b11 → reserved/unsupported`; coefficients are signed, then a
//!   global `shift` (5-bit unsigned). `sample[i] = (Σ coeff[j]·sample[i-1-j]
//!   >> shift) + residual[i]`. Accumulate in `i64` in debug; the perf spike
//!   decides whether `i32` is provably safe (it is for 16-bit input, but prove
//!   it before trusting it).
//! - Under the constrained encode profile (`-l 4`) full-LPC subframes should
//!   never appear; with `strict-profile` on, return
//!   [`crate::Error::ProfileViolation`] instead of decoding them.

use crate::bits::BitReader;

/// Subframe type, from the 6-bit field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubframeType {
    /// Single constant value for the whole block.
    Constant,
    /// Unencoded samples, byte-aligned (optionally with wasted bits).
    Verbatim,
    /// FIXED linear predictor, order 0..=4.
    Fixed(u8),
    /// Full LPC predictor, order 1..=32.
    Lpc {
        /// Predictor order (1..=32).
        order: u8,
        /// Coefficient precision code from the header: 0b00 → 15-bit,
        /// 0b01 → 16-bit, 0b10/0b11 → reserved.
        precision_bits: u8,
    },
}

impl SubframeType {
    /// Parse the 6-bit subframe-type field, plus the order bits that follow for
    /// FIXED/LPC.
    pub fn parse(reader: &mut BitReader<'_>) -> crate::Result<Self> {
        todo!("flac-lite scaffold: SubframeType::parse")
    }

    /// Predictor order (0 for CONSTANT/VERBATIM).
    pub fn order(self) -> usize {
        todo!("flac-lite scaffold: SubframeType::order")
    }
}

/// Per-subframe predictor state retained across frames.
///
/// Fixed-size and `Copy`: exactly [`crate::MAX_LPC_ORDER`] warm-up slots, so no
/// allocation and no `Vec`. Only the first `order` entries are meaningful.
#[derive(Clone, Copy, Debug, Default)]
pub struct PredictorState {
    /// Previous output samples, most recent first (warm-up source).
    pub warm_up: [i32; crate::MAX_LPC_ORDER],
    /// Number of valid entries in `warm_up`.
    pub len: usize,
}

impl PredictorState {
    /// An empty state (used for the first frame of a stream).
    ///
    /// Not `const fn`: `todo!()` is not a permitted call in a const context, and
    /// making this const is a decision for the implementation pass (it would be
    /// zero-cost to do so — a zeroed array).
    pub fn new() -> Self {
        todo!("flac-lite scaffold: PredictorState::new")
    }

    /// Seed from the tail of a decoded subframe (the next frame's warm-up).
    pub fn update(&mut self, decoded: &[i32], order: usize) {
        todo!("flac-lite scaffold: PredictorState::update")
    }
}

/// Decode one subframe into `out`.
///
/// `state` supplies warm-up samples and is updated in place with this frame's
/// tail. `sample_bits` is the frame header's sample size, adjusted by the
/// caller's `-1` for the side subframe of a decorrelated pair.
pub fn decode_subframe(
    reader: &mut BitReader<'_>,
    blocksize: usize,
    sample_bits: u8,
    state: &mut PredictorState,
    out: &mut [i32],
) -> crate::Result<SubframeType> {
    todo!("flac-lite scaffold: decode_subframe")
}

/// Integrate a residual through FIXED predictor coefficients of the given order.
///
/// Implemented as nested running sums (no multiplies) — see module docs.
fn integrate_fixed(order: u8, warm_up: &PredictorState, residual: &mut [i32]) -> crate::Result<()> {
    todo!("flac-lite scaffold: integrate_fixed")
}

/// Integrate a residual through LPC coefficients.
fn integrate_lpc(
    coefficients: &[i32],
    shift: i8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: integrate_lpc")
}
