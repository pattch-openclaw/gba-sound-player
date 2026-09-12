//! Subframe decoding: CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤ 32).
//!
//! IMPLEMENTED: [`SubframeType::parse`] + [`SubframeType::order`] (§9.2.1 type
//! field), plus the whole residual path beneath this module —
//! `residual::rice_unmap` (3a), `residual::decode_rice_partition` (3b), and
//! `residual::decode_residual` (3c). What remains `todo!()` here: the
//! composition (`decode_subframe`), the integrators, and
//! [`PredictorState::fill`] (Phase 1 step 3, substeps 3d–3f).
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
//! Reconstruction is: read the warm-up samples from the subframe header, then
//! decode the residual and integrate it through the predictor.
//!
//! **Corrected 2026-09-10 — warm-up samples are NOT cross-frame state.** This
//! module claimed they were "the previous frame's tail samples", which would
//! have forced sequential decode + retained state across frames. §9.2.5 is
//! explicit: "each subframe in FLAC is coded completely independently", and
//! the warm-up samples "are stored unencoded, bypassing the predictor and
//! residual coding stages" — *in the subframe itself* (Table 21/22: `s(n)`,
//! `n = subframe bps × order`). Measured over real libFLAC encodes
//! (`scripts/measure_warmup_semantics.py`): every frame's warm-up — including
//! **frame 0, which has no previous frame** — sits in its own bitstream
//! position and equals its own subframe's first decoded samples
//! (64/64 frames across `-l 0` and `-l 4` encodes; the previous-frame-tail
//! hypothesis matched 0/64). Consequences that matter: [`PredictorState`] is a
//! per-subframe scratch buffer reused across calls, not retained history;
//! [`crate::decoder::Decoder::seek_frame`] needs no warm-up reconstruction
//! beyond a reset; and frames are independently decodable, which is what
//! makes the O(1) manifest seek design sound. FLAC.md → "Prior assumptions"
//! + step 3a entry.
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

/// Per-subframe warm-up scratch: the `order` unencoded samples the subframe
/// header carries, consumed by the integrators as the seed of the predictor.
///
/// Fixed-size and `Copy`: exactly [`crate::MAX_LPC_ORDER`] slots, so no
/// allocation and no `Vec`. Only the first `order` entries are meaningful.
///
/// **Not cross-frame state** (corrected 2026-09-10 — see the module docs and
/// `scripts/measure_warmup_semantics.py`). Each subframe carries its own
/// warm-up samples in its own header; the next subframe does not inherit
/// this buffer. What remains true about the struct's shape: the frame layer
/// can own one per subframe slot and *reuse* it — `fill()` overwrites the
/// meaningful prefix on every call, so reuse costs nothing and the type
/// never pretends to be history.
#[derive(Clone, Copy, Debug, Default)]
pub struct PredictorState {
    /// Warm-up samples for the subframe being decoded, most recent first
    /// (stream order is oldest-first; `fill` stores them reversed so the
    /// integrators index `warm_up[j] = sample[i-1-j]` directly, matching the
    /// stream's most-recent-past coefficient order).
    pub warm_up: [i32; crate::MAX_LPC_ORDER],
    /// Number of valid entries in `warm_up` (the subframe's predictor order).
    pub len: usize,
}

impl PredictorState {
    /// An empty scratch (valid for any order-0 subframe; `fill` populates it
    /// for predictors with order > 0).
    pub const fn new() -> Self {
        Self {
            warm_up: [0; crate::MAX_LPC_ORDER],
            len: 0,
        }
    }

    /// Read this subframe's warm-up samples from the bitstream (§9.2.5
    /// Table 21, §9.2.6 Table 22: `s(subframe bps × order)`, two's
    /// complement, oldest first in the stream), left-padding each by
    /// `wasted` per §9.2.2's decoder rule, and store them most-recent-first.
    ///
    /// `order` must be ≤ [`crate::MAX_LPC_ORDER`]; the subframe layer gates
    /// profile order before calling. Cursor contract: lands on the first bit
    /// of the coded residual (LPC's precision/shift/coefficient fields sit
    /// between warm-up and residual and are this method's caller's job —
    /// `decode_subframe`, step 3d).
    ///
    /// Scaffold (step 3e wires it into `decode_subframe`): the implementation
    /// is a `read_signed(subframe_bits)` × `order` loop plus a pad shift.
    pub fn fill(
        &mut self,
        reader: &mut crate::bits::BitReader<'_>,
        order: usize,
        subframe_bits: u8,
        wasted: u32,
    ) -> crate::Result<()> {
        todo!("flac-lite scaffold: PredictorState::fill (step 3d)")
    }
}

/// Decode one subframe into `out`.
///
/// `state` is per-subframe scratch (NOT cross-frame state — see module
/// docs): this function fills it from the subframe's own header and uses it
/// to seed the integrator. `sample_bits` is the frame header's sample size,
/// adjusted by the caller's `-1` for the side subframe of a decorrelated
/// pair.
pub fn decode_subframe(
    reader: &mut BitReader<'_>,
    blocksize: usize,
    sample_bits: u8,
    state: &mut PredictorState,
    out: &mut [i32],
) -> crate::Result<SubframeType> {
    todo!("flac-lite scaffold: decode_subframe (step 3e)")
}

/// Integrate a residual through FIXED predictor coefficients of the given order.
///
/// Implemented as nested running sums (no multiplies) — see module docs.
/// Scaffold (step 3d): pure array transform, verifiable against synthetic
/// residuals — no bitstream needed.
fn integrate_fixed(order: u8, warm_up: &PredictorState, residual: &mut [i32]) -> crate::Result<()> {
    todo!("flac-lite scaffold: integrate_fixed (step 3e)")
}

/// Integrate a residual through LPC coefficients.
///
/// Scaffold (step 3d): `sample[i] = (Σ coeff[j]·out[i-1-j] >> shift) +
/// residual[i]`, seeded from `warm_up`; pure array transform.
fn integrate_lpc(
    coefficients: &[i32],
    shift: i8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: integrate_lpc (step 3e)")
}
