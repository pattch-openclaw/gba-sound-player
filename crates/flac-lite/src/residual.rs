//! Residual coding: partitioned Rice / Rice2 / escape-record residual.
//!
//! SCAFFOLD STATE: signatures only, bodies are `todo!()`.
//!
//! Every subframe (FIXED or LPC) ends with a residual. This is the hottest code
//! path in the decoder — it decodes the bulk of the bits — so it is where the
//! perf spike's cycle counts will concentrate. Implementation notes:
//!
//! - **Rice decoding is unary + signed-magnitude, branch-free where possible:**
//!   read the `order`-bit quotient as a run of zero bits terminated by a one
//!   (count with `leading_zeros()` on the refill buffer rather than looping one
//!   bit at a time), then the `order`-bit remainder, then combine
//!   `(quotient << order) | remainder` with the sign bit mapped
//!   `negative ? !sample : sample`.
//! - **No division.** Rice2's escaped-partition mapping and the residual's
//!   sample-size determination are shift/mask only.
//! - The sample size for the residual is the frame header's assignment minus
//!   the predictor order, with an explicit "unknown" (`0b111`) escape that must
//!   be derived from the partition's max coded value.

use crate::bits::BitReader;

/// Rice parameter coding method, from the 2-bit residual header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiceMethod {
    /// 4-bit Rice parameter.
    Rice4Bit,
    /// 5-bit Rice parameter.
    Rice5Bit,
}

/// One partition's header, as read from the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionHeader {
    /// Rice order (0..=14 for Rice4Bit/Rice2; 0..=4 raw escape marker is 0b1111).
    pub order: u8,
    /// Escape record: samples in this partition are unencoded at `raw_bits`.
    pub is_escape: bool,
    /// Sample bits for an escaped partition (4-bit field, `0` means 0 bits).
    pub raw_bits: u8,
    /// Samples in this partition.
    pub sample_count: u16,
}

/// Decode a subframe residual into `out` (length must equal the subframe's
/// block size), returning the samples as *residuals* (the caller in
/// [`crate::subframe`] integrates them through the predictor).
///
/// `expected_samples` is the block size; `sample_bits` is the frame's assigned
/// sample size minus predictor order (or `None` when the stream signals
/// "unknown", requiring derivation).
pub fn decode_residual(
    reader: &mut BitReader<'_>,
    method: RiceMethod,
    partition_count: u8,
    expected_samples: usize,
    sample_bits: Option<u8>,
    warm_up: &[i32],
    out: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: decode_residual")
}

/// Decode one partition's Rice-coded samples into `out`.
///
/// Split out from [`decode_residual`] so it can be unit-tested (and benchmarked)
/// on its own — this is the inner loop.
fn decode_rice_partition(
    reader: &mut BitReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: decode_rice_partition")
}

/// Sign-map a Rice-coded magnitude: `0 → 0`, odd → `(n+1)/2`, even → `-n/2`
/// (i.e. FLAC's `negative = n & 1; magnitude = n >> 1` convention).
fn rice_unmap(n: u32) -> i32 {
    todo!("flac-lite scaffold: rice_unmap")
}
