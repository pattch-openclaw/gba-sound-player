//! Frame layer: header parse + one-frame decode driver.
//!
//! SCAFFOLD STATE: types and signatures only, bodies are `todo!()`.
//!
//! Frame layout (RFC 9639 §9.1). Field widths below are **measured against real
//! libFLAC output**, not transcribed from memory — see the byte-level breakdown
//! and the correction note in [`../../../FLAC.md`](../../../FLAC.md) → "Frame
//! header: measured byte layout".
//!
//! ```text
//! fixed fields — 32 bits total:
//!   [sync 14 = 0b11111111111110x][reserved 1 = 0][blocking strategy 1]
//!   [blocksize code 4][sample rate code 4][channel assignment 4]
//!   [sample size code 3][reserved 1 = 0]
//! then whole octets:
//!   [UTF-8 coded frame/sample number]
//!   [optional uncommon blocksize 8|16][optional uncommon sample rate 8|16]
//!   [CRC-8]
//! then the frame body:
//!   [subframes...][padding to byte boundary][CRC-16]
//! ```
//!
//! Two properties that fall out of those widths and that the implementation
//! depends on:
//!
//! * **Channel assignment is 4 bits**, not 3. It has to cover mono, independent
//!   stereo, three decorrelation modes, 3–8 channel layouts, and reserved
//!   values. Reading 3 bits here desynchronises every later field, the coded
//!   number and the CRC — the header *parses*, and the subframe data three
//!   steps later is garbage.
//! * **The CRC-8 is always byte-aligned.** The fixed fields sum to 32 and
//!   everything up to the CRC is whole octets, so the CRC sits at bit
//!   `32 + 8·octets (+ 8|16 optional fields)` — a multiple of 8 in every case.
//!   `byte_align()` before it is a no-op, and calling it *because* "the header
//!   CRC is unaligned" would document a falsehood (that was the earlier shape
//!   of these notes; it was wrong).
//!
//! Notes for the implementation:
//!
//! - The sync code is `0b11111111111110` (fixed blocksize) or `0b11111111111111`
//!   (variable blocking strategy). Anything else → [`crate::Error::FrameSync`],
//!   which is what lets the frame *stream* rescan to the next sync if a
//!   manifest offset is ever found to be wrong (it shouldn't be — the packer
//!   emits exact offsets — but the fallback is nearly free). Note that sync
//!   scanning is a *recovery* path only: measured on real encodes it
//!   over-matches by 1.1–12x depending on the audio, so it is never the
//!   primary frame finder. FLAC.md → "Frame sync scanning: what actually
//!   filters" has the numbers.
//! - **Blocking strategy is pinned by our profile**: fixed blocksize only. The
//!   variable-blocksize path stays unimplemented and returns
//!   [`crate::Error::ProfileViolation`].
//! - CRC-8 verification covers the header bytes before the CRC byte. Gated
//!   behind `crc-check` (Phase 2 step 5); step 2 only *consumes* the byte.
//! - Sample size codes: only 16-bit is in the target profile (and 8-bit for
//!   experiments); everything else → [`crate::Error::UnsupportedSampleSize`].
//!   Code `0b000` means "from stream" and is resolved through
//!   [`StreamDefaults`] — it is **not** an error, and it is the only code real
//!   encoders emit at 65,536 Hz (see the struct docs).

use crate::bits::BitReader;
use crate::format::{ChannelConfig, SampleRate};
use crate::subframe::PredictorState;

/// Stream-level facts a frame header cannot carry by itself.
///
/// FLAC deliberately omits values from frames when they match the stream
/// default: sample-rate code `0b000` and sample-size code `0b000` both mean
/// "look it up in STREAMINFO". Our container has no STREAMINFO (the GAFP
/// manifest replaces it), so `parse` needs these two values handed in.
///
/// This is not a corner case. Measured on a 65,536 Hz encode: **every frame
/// carried sample-rate code `0b000`**, because 65,536 has no 4-bit code — the
/// project's whole 65 kHz goal runs down the `FromStreamDefault` path. And the
/// encoder needs `--lax` to produce such a stream at all (it is outside FLAC's
/// streamable subset), so it is a path we will be on for as long as we chase
/// that sample rate.
///
/// Callers: Phase 2's `Decoder` passes values from the parsed `Manifest`; the
/// Phase 1 perf spike passes hand-written constants — which is what lets the
/// spike decode real frames with no manifest at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamDefaults {
    /// Stream sample rate in Hz, used for sample-rate code `0b000`.
    pub sample_rate_hz: u32,
    /// Stream bit depth, used for sample-size code `0b000`.
    pub bits_per_sample: u8,
}

/// Parsed frame header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Samples per subframe. Note the **final frame of a track is legitimately
    /// shorter** than the track's block size (measured: a 157-frame 2048-block
    /// track ends in a 512-sample frame), so this is per-frame truth, not the
    /// manifest's block size.
    pub blocksize: usize,
    /// Sample rate, unresolved code preserved; resolve with
    /// [`StreamDefaults::sample_rate_hz`] via [`SampleRate::hz`].
    pub sample_rate: SampleRate,
    /// Channel assignment, including decorrelation mode.
    pub channels: ChannelConfig,
    /// Bits per sample for the primary subframe, already resolved through
    /// [`StreamDefaults`] (code `0b000` → stream default; reserved code
    /// `0b011` → [`crate::Error::UnsupportedSampleSize`]).
    pub bits_per_sample: u8,
    /// UTF-8 coded frame number (or sample number under variable blocking).
    /// Consumed so the cursor lands on the subframe data; our seek never uses
    /// the value (the manifest's offset table is authoritative).
    pub number: u64,
}

impl FrameHeader {
    /// Read a frame header. `reader` must be positioned at the frame's first
    /// byte (a manifest offset). `defaults` resolves the "from stream" codes;
    /// see [`StreamDefaults`].
    ///
    /// Validates every field, and rejects *values* outside the encode profile —
    /// the subset lives in what is accepted, never in what is parsed. A header
    /// is 6–8 bytes; skipping a field saves nothing measurable, while a partial
    /// parse mispositions the cursor and surfaces as corrupt samples three
    /// modules away.
    pub fn parse(reader: &mut BitReader<'_>, defaults: &StreamDefaults) -> crate::Result<Self> {
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
    defaults: &StreamDefaults,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> crate::Result<usize> {
    todo!("flac-lite scaffold: decode_frame")
}
