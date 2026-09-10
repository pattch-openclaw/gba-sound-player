//! Subframe decoding: CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤ 32).
//!
//! IMPLEMENTED: [`SubframeType::parse`] + [`SubframeType::order`] (§9.2.1 type
//! field). `decode_subframe`, the integrators, [`PredictorState`] and the whole
//! of `residual` are still `todo!()` scaffold (Phase 1 step 3).
//!
//! A subframe is one channel's worth of samples for one frame. Layout:
//!
//! ```text
//! [1 zero pad][subframe type][optional wasted bits][warm-up samples][residual]
//! ```
//!
//! (Order corrected 2026-09-09 while implementing the type field: §9.2.1 puts
//! the wasted-bits field *before* the warm-up samples — §9.2.6 Table 22 counts
//! warm-up as `subframe bps × order`, and the subframe bps is only known once
//! wasted bits are read. The old scaffold order was wrong in a way that only
//! bites at step 3, where warm-up width is computed.)
//!
//! Reconstruction is: decode the residual, then integrate it through the
//! predictor. Warm-up samples are the previous frame's tail samples and are the
//! only cross-frame state the decoder must retain (see [`crate::decoder`]).
//!
//! Implementation notes:
//!
//! - **The type field carries no precision.** §9.2.1's table maps the 6-bit
//!   code to a subframe type and, for FIXED/LPC, the *order* only. LPC
//!   coefficient precision is a **body** field: §9.2.6 Table 22 places `u(4)`
//!   = precision−1 (0b1111 forbidden) *after* the warm-up samples, followed by
//!   `s(5)` right shift and the coefficients. The scaffold previously claimed a
//!   2-bit code (`0b00 → 15-bit, 0b01 → 16-bit`) inside the type field — no
//!   such field exists (see FLAC.md → "Prior assumptions"), which is why
//!   `Lpc` now carries `order` alone.
//! - **`parse` consumes exactly pad + type (7 bits); the wasted-bits flag
//!   follows.** Wasted bits (§9.2.2) are a property of how the *body* is
//!   coded, not of the predictor's identity, so they belong to
//!   `decode_subframe`, which needs them to compute the subframe's bits per
//!   sample — not to this enum. `parse`'s contract is therefore "cursor lands
//!   on the wasted-bits flag", and the tests pin that cursor position on real
//!   encoder bytes.
//! - **Wasted bits:** after the type field, a flag bit, then `wasted − 1` in
//!   unary (zero run terminated by a one); decode then left-shift them back
//!   in.
//! - **FIXED orders 0–4** have *known* predictor coefficients
//!   (`[]`, `[1]`, `[2,-1]`, `[3,-3,1]`, `[4,-6,4,-1]`) — the integration is a
//!   cascade of running sums, not a coefficient dot product. Implement as
//!   nested accumulators, which is both faster and avoids multiplies entirely.
//! - **LPC (body, step 3):** coefficients are signed at the u(4) precision,
//!   then a global `shift` (5-bit signed, §9.2.6: MUST NOT be negative).
//!   `sample[i] = (Σ coeff[j]·sample[i-1-j] >> shift) + residual[i]`. The
//!   coefficient order in the stream is *most-recent-past first*. Accumulate
//!   in `i64` in debug; the perf spike decides whether `i32` is provably safe
//!   (it is for 16-bit input, but prove it before trusting it).
//! - **Profile gating is not this layer's job.** All legal LPC orders 1..=32
//!   parse as `Lpc`; rejecting `order > profile.max` (or LPC at all) happens
//!   where an [`crate::format::EncodeProfile`] is in scope, exactly as
//!   `frame::FrameHeader::parse` defers strict-profile blocksize gating.
//! - Under the constrained encode profile the constraint is **max predictor
//!   order**, not "fixed vs LPC". `-l N` is libFLAC's *max LPC order* (`-l 0` is
//!   what means FIXED-only), so a `-l 4` encode is mostly **LPC-4** subframes —
//!   measured 156/157 frames `lpc4` on the vector source, vs `fixed0` ×157 for
//!   `-l 0`. With `strict-profile` on, the check is therefore `order > 4` (and
//!   `order > 0` under a `-l 0` profile) → [`crate::Error::ProfileViolation`];
//!   never "reject LPC" as a category. See FLAC.md → "Correction 3".

use crate::bits::BitReader;

/// Subframe type, from the leading pad bit + 6-bit field (§9.2.1 Table 19).
///
/// Deliberately *not* carrying wasted bits or LPC precision: both are coded
/// after this field and describe the body, not the predictor's identity — see
/// the module docs and [`SubframeType::parse`]'s cursor contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubframeType {
    /// Single constant value for the whole block (`0b000000`).
    Constant,
    /// Unencoded samples, byte-aligned (optionally with wasted bits) (`0b000001`).
    Verbatim,
    /// FIXED linear predictor, order 0..=4 (`0b001000..=0b001100`, order = v−8).
    Fixed(u8),
    /// Full LPC predictor, order 1..=32 (`0b100000..=0b111111`, order = v−31).
    Lpc {
        /// Predictor order (1..=32).
        order: u8,
    },
}

impl SubframeType {
    /// Read the subframe-type field: the §9.2.1 leading zero pad bit, then the
    /// 6-bit type code.
    ///
    /// On success the cursor sits on the **wasted-bits flag** (§9.2.2) — the
    /// first bit `decode_subframe` needs. On `Err` the cursor may sit one bit
    /// in; the caller treats the frame as undecodable, exactly as for a failed
    /// [`crate::frame::FrameHeader::parse`].
    ///
    /// No order bits follow the type code: FIXED and LPC encode their order
    /// *inside* the 6-bit value (v−8 and v−31). The scaffold's "order bits that
    /// follow" note was wrong, and harmlessly so only because nothing read it
    /// yet.
    ///
    /// Errors: [`crate::Error::EndOfStream`] from the reader; [`crate::Error::InvalidField`]
    /// for a set pad bit ("MUST be 0") and for the reserved code ranges
    /// `0b000010..=0b000111` and `0b001101..=0b011111`. Reserved codes are
    /// valid-FLAC-never-emitted territory, so no encoder witness can exist for
    /// their rejection — `tests/subframe_header_layout.rs` mutates one field of
    /// a *real* subframe header instead, exactly as the frame-header tests do,
    /// while the field widths and the 7-bit cursor are witnessed by encoder
    /// bytes.
    pub fn parse(reader: &mut BitReader<'_>) -> crate::Result<Self> {
        if reader.read_bits(1)? != 0 {
            // §9.2.1: "The first bit of the header MUST be 0."
            return Err(crate::Error::InvalidField);
        }
        let code = reader.read_bits(6)? as u8;
        match code {
            0b000000 => Ok(Self::Constant),
            0b000001 => Ok(Self::Verbatim),
            0b001000..=0b001100 => {
                let order = code - 8;
                debug_assert!(usize::from(order) <= crate::MAX_FIXED_ORDER);
                Ok(Self::Fixed(order))
            }
            0b100000..=0b111111 => {
                let order = code - 31;
                debug_assert!(usize::from(order) <= crate::MAX_LPC_ORDER);
                Ok(Self::Lpc { order })
            }
            _ => Err(crate::Error::InvalidField),
        }
    }

    /// Predictor order (0 for CONSTANT/VERBATIM) — how many warm-up samples the
    /// body carries (§9.2.5 Table 21, §9.2.6 Table 22).
    pub fn order(self) -> usize {
        match self {
            Self::Constant | Self::Verbatim => 0,
            Self::Fixed(order) | Self::Lpc { order } => usize::from(order),
        }
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
    pub const fn new() -> Self {
        Self {
            warm_up: [0; crate::MAX_LPC_ORDER],
            len: 0,
        }
    }

    /// Seed from the tail of a decoded subframe (the next frame's warm-up).
    pub fn update(&mut self, decoded: &[i32], order: usize) {
        debug_assert!(order <= crate::MAX_LPC_ORDER);
        self.len = order;
        if order == 0 || decoded.is_empty() {
            return;
        }
        let start = decoded.len().saturating_sub(order);
        let tail = &decoded[start..];
        for (i, &sample) in tail.iter().rev().enumerate() {
            self.warm_up[i] = sample;
        }
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
    if order == 0 {
        return Ok(());
    }
    if order > 4 {
        return Err(crate::Error::UnsupportedPredictorOrder);
    }

    let mut acc0 = 0i32;
    let mut acc1 = 0i32;
    let mut acc2 = 0i32;
    let mut acc3 = 0i32;

    if order > 0 {
        acc0 = warm_up.warm_up[0];
    }
    if order > 1 {
        acc1 = acc0.wrapping_sub(warm_up.warm_up[1]);
    }
    if order > 2 {
        let d1 = warm_up.warm_up[1].wrapping_sub(warm_up.warm_up[2]);
        acc2 = acc1.wrapping_sub(d1);
        if order > 3 {
            let d2 = warm_up.warm_up[2].wrapping_sub(warm_up.warm_up[3]);
            let d12 = d1.wrapping_sub(d2);
            acc3 = acc2.wrapping_sub(d12);
        }
    }

    match order {
        1 => {
            for res in residual.iter_mut() {
                acc0 = acc0.wrapping_add(*res);
                *res = acc0;
            }
        }
        2 => {
            for res in residual.iter_mut() {
                acc1 = acc1.wrapping_add(*res);
                acc0 = acc0.wrapping_add(acc1);
                *res = acc0;
            }
        }
        3 => {
            for res in residual.iter_mut() {
                acc2 = acc2.wrapping_add(*res);
                acc1 = acc1.wrapping_add(acc2);
                acc0 = acc0.wrapping_add(acc1);
                *res = acc0;
            }
        }
        4 => {
            for res in residual.iter_mut() {
                acc3 = acc3.wrapping_add(*res);
                acc2 = acc2.wrapping_add(acc3);
                acc1 = acc1.wrapping_add(acc2);
                acc0 = acc0.wrapping_add(acc1);
                *res = acc0;
            }
        }
        _ => unreachable!(),
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predictor_state_update() {
        let mut state = PredictorState::new();
        let frame1 = [10, 20, 30, 40, 50];

        // Update with order = 3
        state.update(&frame1, 3);
        assert_eq!(state.len, 3);
        // Should be most-recent-past first: 50, 40, 30
        assert_eq!(state.warm_up[0], 50);
        assert_eq!(state.warm_up[1], 40);
        assert_eq!(state.warm_up[2], 30);
    }

    #[test]
    fn test_integrate_fixed_order_0() {
        let state = PredictorState::new();
        let mut residual = [1, 2, 3, -4];
        integrate_fixed(0, &state, &mut residual).unwrap();
        // Order 0: output = residual
        assert_eq!(residual, [1, 2, 3, -4]);
    }

    #[test]
    fn test_integrate_fixed_order_1() {
        let mut state = PredictorState::new();
        state.len = 1;
        state.warm_up[0] = 10; // y[-1]

        let mut residual = [2, -3, 5, 0];
        integrate_fixed(1, &state, &mut residual).unwrap();

        // y[0] = y[-1] + r[0] = 10 + 2 = 12
        // y[1] = y[0] + r[1] = 12 - 3 = 9
        // y[2] = y[1] + r[2] = 9 + 5 = 14
        // y[3] = y[2] + r[3] = 14 + 0 = 14
        assert_eq!(residual, [12, 9, 14, 14]);
    }

    #[test]
    fn test_integrate_fixed_order_2() {
        let mut state = PredictorState::new();
        state.len = 2;
        state.warm_up[0] = 5; // y[-1]
        state.warm_up[1] = 2; // y[-2]

        let mut residual = [1, -2, 0];
        integrate_fixed(2, &state, &mut residual).unwrap();

        // y[n] = 2y[n-1] - y[n-2] + r[n]
        // y[0] = 2(5) - 2 + 1 = 9
        // y[1] = 2(9) - 5 - 2 = 11
        // y[2] = 2(11) - 9 + 0 = 13
        assert_eq!(residual, [9, 11, 13]);
    }

    #[test]
    fn test_integrate_fixed_order_3() {
        let mut state = PredictorState::new();
        state.len = 3;
        state.warm_up[0] = 3; // y[-1]
        state.warm_up[1] = 1; // y[-2]
        state.warm_up[2] = -1; // y[-3]

        let mut residual = [2, 0];
        integrate_fixed(3, &state, &mut residual).unwrap();

        // y[n] = 3y[n-1] - 3y[n-2] + y[n-3] + r[n]
        // y[0] = 3(3) - 3(1) + (-1) + 2 = 9 - 3 - 1 + 2 = 7
        // y[1] = 3(7) - 3(3) + 1 + 0 = 21 - 9 + 1 = 13
        assert_eq!(residual, [7, 13]);
    }

    #[test]
    fn test_integrate_fixed_order_4() {
        let mut state = PredictorState::new();
        state.len = 4;
        state.warm_up[0] = 5; // y[-1]
        state.warm_up[1] = 2; // y[-2]
        state.warm_up[2] = -1; // y[-3]
        state.warm_up[3] = -4; // y[-4]

        let mut residual = [-2, 3];
        integrate_fixed(4, &state, &mut residual).unwrap();

        // y[n] = 4y[n-1] - 6y[n-2] + 4y[n-3] - y[n-4] + r[n]
        // y[0] = 4(5) - 6(2) + 4(-1) - (-4) + (-2) = 20 - 12 - 4 + 4 - 2 = 6
        // y[1] = 4(6) - 6(5) + 4(2) - (-1) + 3 = 24 - 30 + 8 + 1 + 3 = 6
        assert_eq!(residual, [6, 6]);
    }
}
