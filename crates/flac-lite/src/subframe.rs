//! Subframe decoding: CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤ 32).
//!
//! IMPLEMENTED: [`SubframeType::parse`] + [`SubframeType::order`] (§9.2.1 type
//! field), the whole residual path beneath this module —
//! `residual::rice_unmap` (3a), `residual::decode_rice_partition` (3b), and
//! `residual::decode_residual` (3c) — the step 3d integrators
//! ([`PredictorState::fill`], the FIXED cascade, the LPC dot product), and
//! the step 3e composition [`decode_subframe`]. What remains `todo!()`
//! here: nothing — frame-level wiring (`frame::decode_frame`, stereo
//! decorrelation, footer) is 3f.
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
use crate::residual::decode_residual;

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
    /// `order` must be ≤ [`crate::MAX_LPC_ORDER`] (a larger one is a caller
    /// bug → [`crate::Error::UnsupportedPredictorOrder`], cursor untouched);
    /// the subframe layer gates profile order before calling.
    /// `subframe_bits` is the *subframe* sample width — frame bps minus
    /// wasted bits, the caller's subtraction (§9.2.2's "MUST be larger than
    /// zero" gate is `decode_subframe`'s, 3e). `wasted ≥ 32` is rejected as
    /// [`crate::Error::InvalidField`]: the format keeps
    /// `subframe_bits + wasted` ≤ 24, so a wider pad is a corrupt or
    /// mis-placed read, and rejecting it keeps the pad shift from being a
    /// silent wrap.
    ///
    /// Padding is computed in `i64` and truncated to `i32` — legal streams
    /// cannot overflow it under the profile (see the module's i64 note), and
    /// the perf spike revisits the intermediate, not this seam.
    ///
    /// Cursor contract: on success lands on the first bit after the warm-up
    /// (LPC's precision/shift/coefficient fields sit between warm-up and
    /// residual and are the caller's job — `decode_subframe`, 3e). On error
    /// the *state* is untouched (reads land in a local buffer, committed only
    /// on success) while the cursor stays where the failing read left it —
    /// per-read atomicity, same contract as [`crate::residual`]'s readers.
    /// `order == 0` consumes nothing and yields an empty state.
    pub fn fill(
        &mut self,
        reader: &mut crate::bits::BitReader<'_>,
        order: usize,
        subframe_bits: u8,
        wasted: u32,
    ) -> crate::Result<()> {
        if order > crate::MAX_LPC_ORDER {
            return Err(crate::Error::UnsupportedPredictorOrder);
        }
        if wasted >= 32 {
            // Pad shift must stay in `i32`'s defined range; no legal field
            // reaches it (see doc).
            return Err(crate::Error::InvalidField);
        }
        let mut warm = [0i32; crate::MAX_LPC_ORDER];
        // Stream order is oldest-first; storing into descending slots means
        // slot j ends up `sample[i-1-j]` — the most-recent-first layout the
        // integrators index directly.
        for slot in (0..order).rev() {
            let v = reader.read_signed(u32::from(subframe_bits))?;
            warm[slot] = ((i64::from(v)) << wasted) as i32;
        }
        self.warm_up = warm;
        self.len = order;
        Ok(())
    }
}

/// Decode one whole subframe into `out`: type field → wasted bits → bps gate
/// → warm-up → body dispatch (CONSTANT / VERBATIM / residual) → LPC body
/// fields → integrate. Returns the subframe type for the caller's dispatch
/// bookkeeping (3f's stereo layer needs CONSTANT-vs-side shapes).
///
/// `state` is per-subframe scratch (NOT cross-frame state — see module
/// docs): this function fills it from the subframe's own header and uses it
/// to seed the integrator. `sample_bits` is the frame header's sample size,
/// adjusted by the caller's `+1` for the **side** subframe of a decorrelated
/// pair (3f's seam): §4.2's "the side channel needs one extra bit of bit
/// depth", libFLAC 1.5.0 `read_subframe_` does `bps++` on the side slot, and
/// encoder bytes confirm it — anti-phase frames carry side warm-ups beyond
/// the 16-bit field (FLAC.md → step 3f). (An earlier version of this note
/// said `-1`, copied from the plan line; measured false — the side is the
/// *wider* subframe, since L−R can reach twice the bit-depth maximum.)
///
/// **Cursor contract:** starts at the subframe's first bit (the §9.2.1 pad
/// bit) and lands exactly at the end of the subframe body — *before* the
/// frame footer (pad-to-byte + CRC-16), which is 3f's. Witnessed bit-exactly
/// by `tests/subframe_body_vectors.txt` (`exit_bits` over real libFLAC
/// frames).
///
/// **The warm-up doubles as the block's first `order` samples.** Measured
/// 64/64 (step 3a): each subframe's warm-up equals its own first decoded
/// samples. So FIXED/LPC write the prefix directly and decode `out[order..]`
/// through residual + integrator — the only source those samples have.
/// Witnessed end-to-end by the body vectors' `pcm_samples`
/// (reference-`flac -d` PCM).
///
/// **Wasted-bit padding is applied once, at block exit, to the whole decoded
/// block** — RFC 9639 §9.2.2's decoder rule ("the decoded samples… are
/// multiplied by 2^wasted"), measured on libFLAC 1.5.0: `read_subframe_`
/// shifts the entire output array *after* the type dispatch, so prediction
/// runs entirely on the **stripped** scale — warm-up (read via `fill` with
/// `wasted = 0`), residual, and integrators alike. Padding *before*
/// prediction is not equivalent: the LPC dot product's `>> shift` floor does
/// not commute with `<< wasted`, and the mixed-scale variant was witnessed
/// wrong twice — `fixed1-wasted5-256` decoded the ±20000 square wave as
/// 20000/18750, and the `lpc-wasted-256` vector (tonal quantized to
/// multiples of 32, measured LPC-3 + wasted 5) is the committed regression
/// that discriminates padding *order*, not just its presence.
///
/// **Rejections read and write nothing** (crate rule): the `out`-length and
/// `order > blocksize` caller contracts gate before the first bit; §9.2.2's
/// "resulting bits per sample MUST be larger than zero" gate and the
/// §9.2.6 field rejections (precision `0b1111`, negative shift) fire before
/// the bits *behind* them are consumed. Stream errors (`EndOfStream`) leave
/// the cursor at the failing read, per the module-wide per-read atomicity
/// contract; a failed subframe is discarded wholesale, so a partially
/// written `out` is undefined.
pub fn decode_subframe(
    reader: &mut BitReader<'_>,
    blocksize: usize,
    sample_bits: u8,
    state: &mut PredictorState,
    out: &mut [i32],
) -> crate::Result<SubframeType> {
    // Caller contract first, before any bit is consumed: one caller-provided
    // slice per subframe (3d buffer contract).
    if out.len() != blocksize {
        return Err(crate::Error::InvalidField);
    }
    let kind = SubframeType::parse(reader)?;
    let order = kind.order();
    // A predictor longer than the block is corruption (or a caller bug);
    // `decode_residual` rejects it one read deeper, but the prefix write
    // below must not panic first.
    if order > blocksize {
        return Err(crate::Error::InvalidField);
    }
    let wasted = reader.read_wasted_bits()?;
    // §9.2.2's MUST: this is the only place it can be judged (needs frame
    // bps and wasted together). Width > 32 is unreachable for frame bps
    // ≤ 32 but keeps `read_signed`'s width contract local instead of
    // trusting it (same reasoning as the integrators' shift guard).
    let sub_bits = u32::from(sample_bits)
        .checked_sub(wasted)
        .filter(|&b| b > 0 && b <= 32)
        .ok_or(crate::Error::InvalidField)?;
    let subframe_bits = sub_bits as u8;
    // `wasted = 0` here on purpose: prediction runs on the stripped scale
    // (see contract note); `pad_block` applies §9.2.2's multiply once, after
    // the whole block — warm-up prefix included.
    state.fill(reader, order, subframe_bits, 0)?;
    // Warm-up = the block's own first samples (see contract note). Stored
    // most-recent-first; the output is stream order, hence the reversal.
    for (i, slot) in state.warm_up[..order].iter().rev().enumerate() {
        out[i] = *slot;
    }
    match kind {
        SubframeType::Constant => {
            // One stripped value for the whole block; pad_block multiplies it
            // into the output scale like every other type.
            let v = reader.read_signed(sub_bits)?;
            out.fill(v);
        }
        SubframeType::Verbatim => {
            // Raw two's-complement samples (stripped scale), no predictor, no
            // residual. The exit cursor lands at start + blocksize ×
            // subframe_bits exactly (witnessed by the `verbatim-256` body
            // vector).
            for sample in out.iter_mut() {
                *sample = reader.read_signed(sub_bits)?;
            }
        }
        SubframeType::Fixed(order_u8) => {
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_fixed(order_u8, state, &mut out[order..])?;
        }
        SubframeType::Lpc { order: order_u8 } => {
            // §9.2.6 Table 22 body fields, in measured order: u(4) =
            // precision − 1 (0b1111 forbidden), s(5) shift (MUST NOT be
            // negative), then order × s(precision) coefficients,
            // most-recent-past first.
            let precision_minus_one = reader.read_bits(4)?;
            if precision_minus_one == 0b1111 {
                return Err(crate::Error::InvalidField);
            }
            let precision = precision_minus_one + 1;
            let shift = reader.read_signed(5)?;
            // Reject before the coefficient reads: integrate_lpc keeps its
            // own guard (refusing to trust its caller), but here the earlier
            // rejection also consumes zero coefficient bits.
            if shift < 0 {
                return Err(crate::Error::InvalidField);
            }
            let mut coeffs = [0i32; crate::MAX_LPC_ORDER];
            for c in coeffs.iter_mut().take(order) {
                *c = reader.read_signed(precision)?;
            }
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_lpc(&coeffs[..order], shift as i8, state, &mut out[order..])?;
        }
    }
    pad_block(out, wasted);
    Ok(kind)
}

/// §9.2.2's decoder multiply: shift the *fully decoded* block from the
/// wasted-stripped scale back to the sample scale, once, at block exit —
/// exactly what libFLAC's `read_subframe_` does after its type dispatch.
/// Same i64-then-truncate convention as the integrators; `wasted == 0` is a
/// no-op. The shift is always < 32 here: the §9.2.2 gate above forces
/// `wasted < sample_bits ≤ 32`.
fn pad_block(out: &mut [i32], wasted: u32) {
    if wasted == 0 {
        return;
    }
    for s in out.iter_mut() {
        *s = ((i64::from(*s)) << wasted) as i32;
    }
}

/// Integrate a residual through FIXED predictor coefficients of the given
/// order, in place: on success `residual` holds the decoded samples.
///
/// Implemented as nested running sums (no multiplies) — see module docs.
/// Table 20's FIXED-k polynomial `Σ C(k,j)·(−1)^{j+1}·out[i−1−j] + r[i]` is
/// exactly "the k-th forward difference of the output equals the residual",
/// so the cascade runs k difference accumulators plus the output accumulator:
///
/// ```text
/// for each sample: acc_k += r;  ... acc_1 += acc_2;  out += acc_1;  emit out
/// ```
///
/// seeded with the warm-up's forward differences
/// (`acc_1 = w0 − w1`, `acc_2 = w0 − 2w1 + w2`, `acc_3 = w0 − 3w1 + 3w2 − w3`,
/// differences of the differences). The unit tests pin this against the
/// oracle's explicit Table 20 dot product for every order — the cascade-vs-
/// dot-product agreement the 3d plan calls its free cross-check, escalated
/// from "agreement between two impls" to "agreement with generated vectors"
/// so a shared misunderstanding of the seed values still fails.
///
/// Accumulates in `i64`, stores `as i32` (truncation, same convention as the
/// module's perf note; legal profile streams never reach the edge).
///
/// Contract: `order ≤ MAX_FIXED_ORDER` (else [`crate::Error::UnsupportedPredictorOrder`])
/// and `warm_up.len ≥ order` (a shorter warm-up is a caller bug — the
/// subframe's `fill` guarantees `len == order` — and yields
/// [`crate::Error::InvalidField`] before any sample is written).
fn integrate_fixed(order: u8, warm_up: &PredictorState, residual: &mut [i32]) -> crate::Result<()> {
    let order = usize::from(order);
    if order > crate::MAX_FIXED_ORDER {
        return Err(crate::Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(crate::Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    // Forward differences of the warm-up (most-recent-first: w0 = out[-1],
    // w1 = out[-2], ...). Only the arms that index them read them, so the
    // order-0/1 arms never touch uninitialized slots.
    // d1 needs no initializer: every arm that reaches the loop (orders 1–4)
    // assigns it first, and order 0 returns below.
    let mut d1;
    let mut d2 = 0i64;
    let mut d3 = 0i64;
    let mut out = 0i64;
    match order {
        // Order 0: the residual IS the signal.
        0 => return Ok(()),
        1 => {
            d1 = i64::from(w[0]);
        }
        2 => {
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        3 => {
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        _ => {
            // order == 4 (gated above)
            d3 = i64::from(w[0]) - 3 * i64::from(w[1]) + 3 * i64::from(w[2]) - i64::from(w[3]);
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
    }
    for sample in residual.iter_mut() {
        let r = i64::from(*sample);
        match order {
            1 => {
                d1 += r;
                out = d1;
            }
            2 => {
                d1 += r;
                out += d1;
            }
            3 => {
                d2 += r;
                d1 += d2;
                out += d1;
            }
            _ => {
                d3 += r;
                d2 += d3;
                d1 += d2;
                out += d1;
            }
        }
        *sample = out as i32;
    }
    Ok(())
}

/// Integrate a residual through LPC coefficients (§9.2.6):
/// `sample[i] = (Σ coeff[j]·sample[i-1-j] >> shift) + residual[i]`, seeded
/// from `warm_up`; in place, like [`integrate_fixed`].
///
/// `coefficients` are the stream's **most-recent-past-first** order, which is
/// exactly the order [`PredictorState::fill`] stores warm-up in — index `j`
/// addresses both. Past samples are the already-integrated prefix of
/// `residual` (that is what makes in-place integration correct: the LPC
/// recurrence reads *decoded* samples, not residuals).
///
/// **`shift` MUST NOT be negative (§9.2.6)** → [`crate::Error::InvalidField`].
/// A shift ≥ 64 is rejected the same way (the s(5) field caps at 15; the
/// guard exists because `i64 >> 64` is not a thing to panic on, and a
/// standalone-pure function should not rely on its caller — 3e's parser —
/// for memory-safety-adjacent bounds).
///
/// Contract: `coefficients.len() ≤ MAX_LPC_ORDER` (else
/// [`crate::Error::UnsupportedPredictorOrder`]) and `warm_up.len ≥
/// coefficients.len()` (caller bug → [`crate::Error::InvalidField`], no
/// sample written). Accumulates the dot product in `i64`; the right shift is
/// Rust's arithmetic shift on `i64` (floor for negatives — matches ARM `asr`
/// and the oracle's Python `>>`, whose negative-accumulator vectors pin
/// exactly this behaviour).
fn integrate_lpc(
    coefficients: &[i32],
    shift: i8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> crate::Result<()> {
    if shift < 0 {
        return Err(crate::Error::InvalidField);
    }
    let order = coefficients.len();
    if order > crate::MAX_LPC_ORDER {
        return Err(crate::Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(crate::Error::InvalidField);
    }
    let shift = u32::from(shift as u8);
    if shift >= 64 {
        return Err(crate::Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    for i in 0..residual.len() {
        let mut acc = 0i64;
        for (j, &c) in coefficients.iter().enumerate() {
            // past[i-1-j]: decoded prefix, else warm-up. Warm-up is stored
            // most-recent-first (w[m] = sample[-1-m]), so for k = i-1-j < 0
            // the slot index is m = -1-k = j-i.
            let past = if j < i {
                i64::from(residual[i - 1 - j])
            } else {
                i64::from(w[j - i])
            };
            acc += i64::from(c) * past;
        }
        residual[i] = ((acc >> shift) + i64::from(residual[i])) as i32;
    }
    Ok(())
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). `core`-only,
// like the rest of the crate's unit tests.
//
// Witness rule (3d plan): *synthetic residuals + an independent Python
// reference — no bitstream dependency, no golden-vector work needed.* The
// vectors below were emitted by `drafts/flac3d_oracle.py` (throwaway, not
// committed — the step 3c drafts/ precedent), which implements §9.2.5/§9.2.6
// reconstruction with arbitrary-precision integers. Two properties make the
// vectors witnesses, not transcriptions of the author's belief (the lesson
// FLAC.md records firing four times):
//
// 1. **Generation, not assertion.** Each vector is built by deriving a
//    residual from a known smooth signal via the encoder's inverse formula,
//    then requiring the decode to recover *the signal itself*. A mis-indexed
//    past, wrong shift, or dropped warm-up breaks recovery even if oracle
//    and impl shared the same summing bug.
// 2. **Independent mechanism.** Python big-int dot products (no i64, no
//    cascade, no in-place aliasing) vs the impl's nested accumulators, i64
//    dot product, and in-place writes. The FIXED-as-cascade vs FIXED-as-
//    dot-product agreement the plan calls its free cross-check is also
//    pinned in-test (against Table 20's explicit coefficients) and on
//    pseudo-random residuals.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // ---- oracle-generated vectors (drafts/flac3d_oracle.py, 2026-09-12) ----

    // FIXED order 0: warm-up is a dummy — order 0 means "the residual IS the
    // signal"; the vector's identity pins that the impl never touches it.
    const FIXED0_RESID: [i32; 24] = [
        377, 402, 362, 391, 292, -77, -127, -33, 60, -60, -101, -441, -357, -417, -361, -479, -371,
        -706, -719, -523, -296, -118, 49, -137,
    ];
    const FIXED0_EXPECT: [i32; 24] = FIXED0_RESID;

    // FIXED order 1: warm-up (most-recent-first), residual, expected signal
    const FIXED1_WARM: [i32; 1] = [498];
    const FIXED1_RESID: [i32; 24] = [
        -128, -54, 329, 129, -209, -76, 23, -110, 319, -182, -66, 128, 48, 96, 117, -322, -111,
        -256, 216, -219, 17, 10, 80, 6,
    ];
    const FIXED1_EXPECT: [i32; 24] = [
        370, 316, 645, 774, 565, 489, 512, 402, 721, 539, 473, 601, 649, 745, 862, 540, 429, 173,
        389, 170, 187, 197, 277, 283,
    ];

    // FIXED order 2
    const FIXED2_WARM: [i32; 2] = [-433, -142];
    const FIXED2_RESID: [i32; 24] = [
        614, -456, 166, -405, 654, -65, -121, -346, 194, -124, 89, 22, 17, -233, 463, -39, 132,
        -448, 21, 123, -160, 35, 215, -396,
    ];
    const FIXED2_EXPECT: [i32; 24] = [
        -110, -243, -210, -582, -300, -83, 13, -237, -293, -473, -564, -633, -685, -970, -792,
        -653, -382, -559, -715, -748, -941, -1099, -1042, -1381,
    ];

    // FIXED order 3
    const FIXED3_WARM: [i32; 3] = [-226, 138, 50];
    const FIXED3_RESID: [i32; 24] = [
        989, -922, 398, -22, 389, -643, 459, -90, -147, -371, 538, -212, 200, -155, 409, -672, 421,
        -203, 193, -27, 29, -697, 884, -349,
    ];
    const FIXED3_EXPECT: [i32; 24] = [
        -53, -265, -464, -672, -500, -591, -486, -275, -105, -347, -463, -665, -753, -882, -643,
        -708, -656, -690, -617, -464, -202, -528, -558, -641,
    ];

    // FIXED order 4
    const FIXED4_WARM: [i32; 4] = [518, 418, 289, 363];
    const FIXED4_RESID: [i32; 24] = [
        120, 3, 820, -1450, 1251, -670, 58, 17, -86, 346, 149, -274, -657, 1146, -611, -580, 1257,
        -497, -65, -866, 1699, -1137, 44, 601,
    ];
    const FIXED4_EXPECT: [i32; 24] = [
        477, 186, 356, 248, 374, 576, 754, 825, 620, 316, 239, 441, 317, 408, 644, 375, 208, 253,
        555, 293, 345, 452, 399, 572,
    ];

    // LPC case 0: coeffs [64] shift 6
    const LPC0_COEFFS: [i32; 1] = [64];
    const LPC0_SHIFT: i8 = 6;
    const LPC0_WARM: [i32; 1] = [-372];
    const LPC0_RESID: [i32; 20] = [
        -99, 132, 13, 5, 191, -209, 159, -124, 142, 125, 356, -17, -177, 77, -5, -240, -177, -103,
        352, 140,
    ];
    const LPC0_EXPECT: [i32; 20] = [
        -471, -339, -326, -321, -130, -339, -180, -304, -162, -37, 319, 302, 125, 202, 197, -43,
        -220, -323, 29, 169,
    ];

    // LPC case 1: coeffs [96, -32] shift 5 (mixed signs → negative
    // accumulators reach the shift: the floor-vs-truncate seam)
    const LPC1_COEFFS: [i32; 2] = [96, -32];
    const LPC1_SHIFT: i8 = 5;
    const LPC1_WARM: [i32; 2] = [-670, -340];
    const LPC1_RESID: [i32; 20] = [
        878, 909, 665, 997, 977, 1173, 1252, 1229, 986, 495, 736, 763, 587, 1122, 1047, 1094, 997,
        1018, 1956, 932,
    ];
    const LPC1_EXPECT: [i32; 20] = [
        -792, -797, -934, -1008, -1113, -1158, -1109, -940, -725, -740, -759, -774, -976, -1032,
        -1073, -1093, -1209, -1516, -1383, -1701,
    ];

    // LPC case 2: coeffs [80, 40, -20, -8] shift 4
    const LPC2_COEFFS: [i32; 4] = [80, 40, -20, -8];
    const LPC2_SHIFT: i8 = 4;
    const LPC2_WARM: [i32; 4] = [234, 493, 240, 346];
    const LPC2_RESID: [i32; 20] = [
        -1776, -538, 171, -1652, -1226, -1595, -2705, -2455, -1183, -2159, -2005, -2199, -1880,
        -3709, -3481, -1898, -2570, -2768, -3035, -3069,
    ];
    const LPC2_EXPECT: [i32; 20] = [
        153, 75, 389, 172, 436, 491, 430, 291, 515, 360, 503, 426, 800, 547, 470, 606, 551, 641,
        555, 316,
    ];

    // LPC case 3: coeffs [-12345, 999, -7, 33, 4096] shift 0 (extreme coeffs,
    // shift-0 identity path, order 5 > MAX_FIXED_ORDER — LPC-only order)
    const LPC3_COEFFS: [i32; 5] = [-12345, 999, -7, 33, 4096];
    const LPC3_SHIFT: i8 = 0;
    const LPC3_WARM: [i32; 5] = [-64, 188, -158, -311, -170];
    const LPC3_RESID: [i32; 20] = [
        -272583, -730021, -3920006, -3090565, -5874370, -4749244, -1994688, -4131982, -121137,
        3190315, -1377617, 3905411, 916380, 4159667, 7660184, 3755631, 677187, -377940, -1877786,
        3351499,
    ];
    const LPC3_EXPECT: [i32; 20] = [
        -168, -383, -219, -515, -483, -328, -435, -217, 80, -215, 154, 15, 364, 579, 402, 93, 99,
        49, 409, 313,
    ];

    // fill case 0: order 3, subframe_bits 13, wasted 0, stream oldest-first
    // [-1234, 567, -8], 0 filler bits
    const FILL0_BYTES: [u8; 5] = [217, 112, 141, 255, 240];
    const FILL0_EXPECT: [i32; 3] = [-8, 567, -1234];
    const FILL0_START_BITS: usize = 0;
    const FILL0_ORDER: usize = 3;
    const FILL0_BITS: u8 = 13;
    const FILL0_WASTED: u32 = 0;

    // fill case 1: order 2, subframe_bits 11, wasted 3, stream oldest-first
    // [17, -25], 5 filler bits (mid-byte cursor + wasted padding)
    const FILL1_BYTES: [u8; 4] = [208, 17, 252, 224];
    const FILL1_EXPECT: [i32; 2] = [-200, 136];
    const FILL1_START_BITS: usize = 5;
    const FILL1_ORDER: usize = 2;
    const FILL1_BITS: u8 = 11;
    const FILL1_WASTED: u32 = 3;

    // fill case 2: order 1, subframe_bits 16, wasted 0, stream oldest-first
    // [-32768], 3 filler bits (i16 extreme)
    const FILL2_BYTES: [u8; 3] = [80, 0, 0];
    const FILL2_EXPECT: [i32; 1] = [-32768];
    const FILL2_START_BITS: usize = 3;
    const FILL2_ORDER: usize = 1;
    const FILL2_BITS: u8 = 16;
    const FILL2_WASTED: u32 = 0;

    // fill case 3: order 4, subframe_bits 8, wasted 2, stream oldest-first
    // [100, -100, 127, -128], 7 filler bits (worst-case alignment)
    const FILL3_BYTES: [u8; 5] = [180, 201, 56, 255, 0];
    const FILL3_EXPECT: [i32; 4] = [-512, 508, -400, 400];
    const FILL3_START_BITS: usize = 7;
    const FILL3_ORDER: usize = 4;
    const FILL3_BITS: u8 = 8;
    const FILL3_WASTED: u32 = 2;

    // Shared combo residuals: fill's padded state feeding FIXED order 1
    // (oracle-computed expectations per case).
    const COMBO_RESID: [i32; 6] = [5, -5, 7, 0, -1, 3];
    const FILL0_COMBO_EXPECT: [i32; 6] = [-3, -8, -1, -1, -2, 1];
    const FILL1_COMBO_EXPECT: [i32; 6] = [-195, -200, -193, -193, -194, -191];
    const FILL2_COMBO_EXPECT: [i32; 6] = [-32763, -32768, -32761, -32761, -32762, -32759];
    const FILL3_COMBO_EXPECT: [i32; 6] = [-507, -512, -505, -505, -506, -503];

    // ---- helpers -----------------------------------------------------------

    /// Build scratch state from an already-most-recent-first warm-up slice
    /// (the oracle emits warm-up in exactly the stored order).
    fn warm_state(most_recent_first: &[i32]) -> PredictorState {
        let mut s = PredictorState::new();
        for (slot, &v) in s.warm_up.iter_mut().zip(most_recent_first) {
            *slot = v;
        }
        s.len = most_recent_first.len();
        s
    }

    /// Table 20's explicit FIXED coefficients — the dot-product form of the
    /// same predictor the cascade implements. Independent mechanism, per the
    /// 3d plan's free cross-check.
    const FIXED_COEFFS: [&[i32]; crate::MAX_FIXED_ORDER + 1] =
        [&[], &[1], &[2, -1], &[3, -3, 1], &[4, -6, 4, -1]];

    fn fixed_dot(order: usize, warm: &PredictorState, residual: &[i32], out: &mut [i32]) {
        for i in 0..residual.len() {
            let mut acc = i64::from(residual[i]);
            for (j, &c) in FIXED_COEFFS[order].iter().enumerate() {
                let past = if j < i {
                    i64::from(out[i - 1 - j])
                } else {
                    i64::from(warm.warm_up[j - i])
                };
                acc += i64::from(c) * past;
            }
            out[i] = acc as i32;
        }
    }

    fn lcg(seed: &mut u32) -> i32 {
        *seed = seed
            .wrapping_mul(1664525)
            .wrapping_add(1013904223)
            .wrapping_shr(16);
        (*seed % 4001) as i32 - 2000
    }

    // ---- integrate_fixed ---------------------------------------------------

    #[test]
    fn fixed_orders_recover_the_oracle_signal() {
        let cases: [(u8, &[i32], &[i32; 24], &[i32; 24]); 5] = [
            (0, &[0], &FIXED0_RESID, &FIXED0_EXPECT),
            (1, &FIXED1_WARM, &FIXED1_RESID, &FIXED1_EXPECT),
            (2, &FIXED2_WARM, &FIXED2_RESID, &FIXED2_EXPECT),
            (3, &FIXED3_WARM, &FIXED3_RESID, &FIXED3_EXPECT),
            (4, &FIXED4_WARM, &FIXED4_RESID, &FIXED4_EXPECT),
        ];
        for (order, warm, resid, expect) in cases {
            let mut out = [0i32; 24];
            out.copy_from_slice(resid);
            integrate_fixed(order, &warm_state(warm), &mut out)
                .unwrap_or_else(|e| panic!("FIXED order {order}: {e:?}"));
            assert_eq!(&out, expect, "FIXED order {order} vs oracle signal");
        }
    }

    #[test]
    fn fixed_cascade_matches_table20_dot_product_on_vectors() {
        // The plan's cross-check, pinned on the oracle vectors: cascade and
        // explicit-coefficient dot product must agree sample-for-sample —
        // which also proves the cascade's forward-difference warm-up seeds
        // (a wrong seed agrees on nothing but the first few samples at best).
        let cases: [(u8, &[i32], &[i32; 24]); 5] = [
            (0, &[0], &FIXED0_RESID),
            (1, &FIXED1_WARM, &FIXED1_RESID),
            (2, &FIXED2_WARM, &FIXED2_RESID),
            (3, &FIXED3_WARM, &FIXED3_RESID),
            (4, &FIXED4_WARM, &FIXED4_RESID),
        ];
        for (order, warm, resid) in cases {
            let st = warm_state(warm);
            let mut cascade = [0i32; 24];
            cascade.copy_from_slice(resid);
            integrate_fixed(order, &st, &mut cascade).unwrap();
            let mut dot = [0i32; 24];
            fixed_dot(usize::from(order), &st, resid, &mut dot);
            assert_eq!(cascade, dot, "FIXED order {order}: cascade vs dot product");
        }
    }

    #[test]
    fn fixed_cascade_matches_dot_product_on_pseudo_random_residuals() {
        // The smooth vectors keep residuals small (real audio does); a
        // differential sweep with wider residuals catches cascade wiring the
        // vectors' structure might not, without needing any stream witness.
        for order in 0..=crate::MAX_FIXED_ORDER as u8 {
            let mut seed = 0x2545_F49Bu32 ^ (u32::from(order) * 7919);
            let mut resid = [0i32; 40];
            for r in resid.iter_mut() {
                *r = lcg(&mut seed);
            }
            let st = warm_state(&[7i32, -3, 9, -1][..usize::from(order)]);
            let mut cascade = resid;
            integrate_fixed(order, &st, &mut cascade).unwrap();
            let mut dot = [0i32; 40];
            fixed_dot(usize::from(order), &st, &resid, &mut dot);
            assert_eq!(cascade, dot, "FIXED order {order}: sweep");
        }
    }

    // ---- integrate_lpc -----------------------------------------------------

    #[test]
    fn lpc_recovers_the_oracle_signal() {
        let mut out = [0i32; 20];
        out.copy_from_slice(&LPC0_RESID);
        integrate_lpc(&LPC0_COEFFS, LPC0_SHIFT, &warm_state(&LPC0_WARM), &mut out).unwrap();
        assert_eq!(&out, &LPC0_EXPECT, "LPC case 0");

        out.copy_from_slice(&LPC1_RESID);
        integrate_lpc(&LPC1_COEFFS, LPC1_SHIFT, &warm_state(&LPC1_WARM), &mut out).unwrap();
        assert_eq!(&out, &LPC1_EXPECT, "LPC case 1 (negative accumulators)");

        out.copy_from_slice(&LPC2_RESID);
        integrate_lpc(&LPC2_COEFFS, LPC2_SHIFT, &warm_state(&LPC2_WARM), &mut out).unwrap();
        assert_eq!(&out, &LPC2_EXPECT, "LPC case 2");

        out.copy_from_slice(&LPC3_RESID);
        integrate_lpc(&LPC3_COEFFS, LPC3_SHIFT, &warm_state(&LPC3_WARM), &mut out).unwrap();
        assert_eq!(&out, &LPC3_EXPECT, "LPC case 3 (shift 0, extreme coeffs)");
    }

    #[test]
    fn lpc_in_place_matches_separate_history_impl() {
        // In-place integration aliases residuals and decoded samples in one
        // slice; the reference keeps them apart in an absolute-position
        // history buffer (warm-up oldest-first, then decoded) — a different
        // indexing scheme than the impl's decoded-prefix + warm-slot math.
        let coeffs = [64i32, -32, 16, -8];
        let shift = 4i8;
        let warm_mrf = [5i32, -9, 3, 11];
        let mut seed = 0xC0FF_EEu32;
        let mut resid = [0i32; 36];
        for r in resid.iter_mut() {
            *r = lcg(&mut seed);
        }
        let st = warm_state(&warm_mrf);
        let mut out = resid;
        integrate_lpc(&coeffs, shift, &st, &mut out).unwrap();

        const W: usize = 4; // warm-up length
        let mut hist = [0i32; W + 36];
        for m in 0..W {
            hist[W - 1 - m] = warm_mrf[m]; // oldest-first by absolute position
        }
        for i in 0..36usize {
            let mut acc = 0i64;
            for (j, &c) in coeffs.iter().enumerate() {
                acc += i64::from(c) * i64::from(hist[W + i - 1 - j]);
            }
            hist[W + i] = ((acc >> shift) + i64::from(resid[i])) as i32;
            assert_eq!(hist[W + i], out[i], "LPC sweep sample {i}");
        }
    }

    #[test]
    fn lpc_shift_is_arithmetic_like_asr() {
        // (-1) >> 1 must floor to -1 (ARM asr, Python >>), not truncate to 0.
        // Mixed-sign vectors exercise negative accumulators broadly; this
        // pins the seam alone.
        let mut resid = [0i32; 3];
        integrate_lpc(&[1i32], 1, &warm_state(&[-1]), &mut resid).unwrap();
        assert_eq!(resid, [-1, -1, -1]);
    }

    // ---- PredictorState::fill ----------------------------------------------

    #[test]
    fn fill_reads_pads_and_stores_most_recent_first() {
        // (bytes, start bits, order, subframe bps, wasted, expected state)
        let cases: [(&[u8], usize, usize, u8, u32, &[i32]); 4] = [
            (
                &FILL0_BYTES,
                FILL0_START_BITS,
                FILL0_ORDER,
                FILL0_BITS,
                FILL0_WASTED,
                &FILL0_EXPECT,
            ),
            (
                &FILL1_BYTES,
                FILL1_START_BITS,
                FILL1_ORDER,
                FILL1_BITS,
                FILL1_WASTED,
                &FILL1_EXPECT,
            ),
            (
                &FILL2_BYTES,
                FILL2_START_BITS,
                FILL2_ORDER,
                FILL2_BITS,
                FILL2_WASTED,
                &FILL2_EXPECT,
            ),
            (
                &FILL3_BYTES,
                FILL3_START_BITS,
                FILL3_ORDER,
                FILL3_BITS,
                FILL3_WASTED,
                &FILL3_EXPECT,
            ),
        ];
        for (idx, (bytes, start, order, bits, wasted, expect)) in cases.iter().enumerate() {
            let mut r = BitReader::new(bytes);
            if *start > 0 {
                // `read_bits(0)` is an InvalidField by design, so the
                // aligned case skips rather than zero-reads.
                r.read_bits((*start) as u32).unwrap();
            }
            let mut st = PredictorState::new();
            st.fill(&mut r, *order, *bits, *wasted)
                .unwrap_or_else(|e| panic!("fill case {idx}: {e:?}"));
            assert_eq!(st.len, *order, "fill case {idx}: len");
            assert_eq!(&st.warm_up[..*order], *expect, "fill case {idx}: state");
            assert_eq!(
                r.bit_position(),
                start + order * usize::from(*bits),
                "fill case {idx}: cursor lands on the next field"
            );
        }
    }

    #[test]
    fn fill_commit_and_rejection_contracts() {
        // order 0: consumes nothing, even from empty input.
        let mut r = BitReader::new(&[]);
        let mut st = PredictorState::new();
        st.fill(&mut r, 0, 16, 0).unwrap();
        assert_eq!(st.len, 0);
        assert_eq!(r.bit_position(), 0);

        // EOF: state untouched, cursor exactly where the failing read stopped
        // (per-read atomicity + commit-only-on-success, the module-wide rule).
        let mut st = warm_state(&[111, 222, 333]);
        let mut r = BitReader::new(&FILL0_BYTES[..3]); // 24 bits; order 3 wants 39
        assert_eq!(st.fill(&mut r, 3, 13, 0), Err(Error::EndOfStream));
        assert_eq!(st.len, 3, "state untouched on failure");
        assert_eq!(&st.warm_up[..3], &[111, 222, 333]);
        assert_eq!(r.bit_position(), 13, "one 13-bit read succeeded");

        // wasted >= 32: InvalidField before any read (keeps the pad shift
        // from being a silent wrap; no legal field reaches it).
        let mut r = BitReader::new(&FILL0_BYTES);
        assert_eq!(st.fill(&mut r, 3, 13, 32), Err(Error::InvalidField));
        assert_eq!(r.bit_position(), 0);

        // order > MAX_LPC_ORDER: caller bug, zero reads.
        assert_eq!(
            st.fill(&mut r, crate::MAX_LPC_ORDER + 1, 16, 0),
            Err(Error::UnsupportedPredictorOrder)
        );
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn fill_pads_feed_the_integrator() {
        // End-to-end witness for the two halves of 3d meeting: fill's
        // `<< wasted` padding is exactly what makes the padded warm-up the
        // integrator's seed (the padded signal = real signal, wasted LSBs
        // included — the oracle composes the same way).
        let cases: [(&[u8], usize, usize, u8, u32, &[i32; 6]); 4] = [
            (
                &FILL0_BYTES,
                FILL0_START_BITS,
                FILL0_ORDER,
                FILL0_BITS,
                FILL0_WASTED,
                &FILL0_COMBO_EXPECT,
            ),
            (
                &FILL1_BYTES,
                FILL1_START_BITS,
                FILL1_ORDER,
                FILL1_BITS,
                FILL1_WASTED,
                &FILL1_COMBO_EXPECT,
            ),
            (
                &FILL2_BYTES,
                FILL2_START_BITS,
                FILL2_ORDER,
                FILL2_BITS,
                FILL2_WASTED,
                &FILL2_COMBO_EXPECT,
            ),
            (
                &FILL3_BYTES,
                FILL3_START_BITS,
                FILL3_ORDER,
                FILL3_BITS,
                FILL3_WASTED,
                &FILL3_COMBO_EXPECT,
            ),
        ];
        for (idx, (bytes, start, order, bits, wasted, expect)) in cases.iter().enumerate() {
            let mut r = BitReader::new(bytes);
            if *start > 0 {
                r.read_bits((*start) as u32).unwrap();
            }
            let mut st = PredictorState::new();
            st.fill(&mut r, *order, *bits, *wasted).unwrap();
            // FIXED order 1 needs only warm[0]; every case has order >= 1.
            let mut out = [0i32; 6];
            out.copy_from_slice(&COMBO_RESID);
            integrate_fixed(1, &st, &mut out).unwrap();
            assert_eq!(&out, *expect, "fill+FIXED1 combo case {idx}");
        }
    }

    // ---- contract rejections -------------------------------------------------

    #[test]
    fn integrator_contract_rejections_write_nothing() {
        let st3 = warm_state(&[1, 2, 3]);
        let mut resid = [7i32; 4];

        assert_eq!(
            integrate_fixed(5, &st3, &mut resid),
            Err(Error::UnsupportedPredictorOrder)
        );
        assert_eq!(
            integrate_fixed(2, &warm_state(&[1]), &mut resid),
            Err(Error::InvalidField)
        );
        assert_eq!(resid, [7; 4], "FIXED rejections precede every write");

        assert_eq!(
            integrate_lpc(&[1, 2], -1, &st3, &mut resid),
            Err(Error::InvalidField)
        );
        assert_eq!(
            integrate_lpc(&[1i32; crate::MAX_LPC_ORDER + 1], 0, &st3, &mut resid),
            Err(Error::UnsupportedPredictorOrder)
        );
        assert_eq!(
            integrate_lpc(&[1, 2, 3, 4], 0, &st3, &mut resid),
            Err(Error::InvalidField)
        );
        assert_eq!(
            integrate_lpc(&[1], 127, &st3, &mut resid),
            Err(Error::InvalidField)
        );
        assert_eq!(resid, [7; 4], "LPC rejections precede every write");
    }
}
