//! Stereo decorrelation (RFC 9629 §7.4.3).
//!
//! SCAFFOLD STATE: signatures only, bodies are `todo!()`.
//!
//! FLAC stores a stereo pair as two subframes plus a channel-assignment code
//! that says how to recombine them. Cheap integer work — one add, one shift, one
//! subtract per sample — but note the ordering hazard below.
//!
//! In-place, `left`/`right` buffers must be written from the *primary* and
//! *secondary* decoded subframes:
//!
//! - **mid/side:** `mid` (primary) + `side` (secondary). `mid` is an
//!   *extended-precision* value: `(mid << 1) | (side & 1)` recovers the LSB
//!   stolen by the side subframe. Then `left = (m + s) >> 1`,
//!   `right = (m - s) >> 1`. Accumulate in `i32` (the `+1` bit of rounding on
//!   the halving is handled by the recovered LSB, not by rounding).
//! - **left/side:** `left = primary`, `right = left - side`.
//! - **right/side:** `right = primary`, `left = side + right`.
//!
//! The decorrelation is **not** in place safe for mid/side: both outputs read
//! both inputs, so iterate with temporaries (no aliasing tricks).

use crate::format::ChannelConfig;

/// Recombine two decoded subframes into left/right PCM, in place.
///
/// On entry, `left` holds the primary subframe and `right` the secondary
/// (side). On exit they hold final PCM. `blocksize` samples are processed.
pub fn decorrelate(
    config: ChannelConfig,
    blocksize: usize,
    left: &mut [i32],
    right: &mut [i32],
) -> crate::Result<()> {
    todo!("flac-lite scaffold: decorrelate")
}

/// Mid/side: recover the LSB stolen from `mid` by the side residual.
fn mid_side_recover(mid: &mut [i32], side: &[i32], blocksize: usize) -> crate::Result<()> {
    todo!("flac-lite scaffold: mid_side_recover")
}
