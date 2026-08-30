//! Frame layer: header parse + one-frame decode driver.
//!
//! SCAFFOLD STATE: types and signatures only, bodies are `todo!()`.
//!
//! Frame layout (RFC 9629 §7):
//!
//! ```text
//! [sync 0b1111111x][reserved 0][blocking strategy][blocksize code][sample rate code]
//! [channel assignment][sample size code][reserved 0][UTF-8 frame/sample number]
//! [optional get-blocksize][optional get-sample-rate][CRC-8]
//! [subframes...][optional padding][CRC-16]
//! ```
//!
//! Notes for the implementation:
//!
//! - The sync code is `0b11111110` (fixed blocksize) or `0b11111111`
//!   (variable/blocking strategy UTF-8). Anything else → [`crate::Error::FrameSync`],
//!   which is what lets the frame *stream* rescan to the next sync if a
//!   manifest offset is ever found to be wrong (it shouldn't be — the packer
//!   emits exact offsets — but the fallback is nearly free).
//! - **Blocking strategy is pinned by our profile**: fixed blocksize only. The
//!   variable-blocksize path stays unimplemented and returns
//!   [`crate::Error::ProfileViolation`].
//! - CRC-8 covers the frame header bytes *including* the CRC-8 position's zero
//!   placeholder convention; the reader needs raw header bytes, so
//!   [`crate::bits::BitReader::bit_position`] brackets the header span. Gated
//!   behind `crc-check`.
//! - Sample size codes: only 16-bit is in the target profile (and 8-bit for
//!   experiments); everything else → [`crate::Error::UnsupportedSampleSize`].

use crate::bits::BitReader;
use crate::format::{ChannelConfig, SampleRate};
use crate::subframe::PredictorState;

/// Parsed frame header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Samples per subframe.
    pub blocksize: usize,
    /// Sample rate (resolved; `FromStreamDefault` needs the manifest's value).
    pub sample_rate: SampleRate,
    /// Channel assignment, including decorrelation mode.
    pub channels: ChannelConfig,
    /// Bits per sample for the primary subframe (2ⁿ-1 field, minus 1 for
    /// "get from stream info", which our profile forbids).
    pub bits_per_sample: u8,
    /// UTF-8 coded frame number (or sample number under variable blocking).
    pub number: u64,
}

impl FrameHeader {
    /// Read a frame header. `reader` must be positioned at the frame's first
    /// byte (a manifest offset).
    pub fn parse(reader: &mut BitReader<'_>) -> crate::Result<Self> {
        todo!("flac-lite scaffold: FrameHeader::parse")
    }
}

/// Decode one whole frame into `left` / `right`.
///
/// `left` is the only slice used for mono. `state` carries predictor warm-up
/// across frames (one [`PredictorState`] per channel/subframe slot). PCM is
/// written as sign-extended `i32` in the stream's native precision; conversion
/// to the mixer's 8-bit unsigned format happens at the playback boundary, not
/// here, so this stays reusable (and testable) independent of `agb`.
///
/// Returns the header so the caller can track sample position / sample rate.
pub fn decode_frame(
    reader: &mut BitReader<'_>,
    header: &FrameHeader,
    stream_default_rate: u32,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> crate::Result<usize> {
    todo!("flac-lite scaffold: decode_frame")
}
