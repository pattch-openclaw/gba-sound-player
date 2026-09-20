//! The frame-loop driver — the shared seam every spike PR plugs into.
//!
//! [`decode_clip`] walks a clip's generated seek table and decodes each frame
//! through `flac_lite::frame::decode_frame`, handing the decoded block to a
//! caller callback. Nothing here is measurement-specific; the *consumers*
//! differ per PR, and this module deliberately knows nothing about them:
//!
//! | PR | Consumer | Uses |
//! |---|---|---|
//! | 1 | host witness test | collect + diff against reference PCM |
//! | 2 | ROM decode proof | hash-as-you-go (`Fnv1a64::update_i16le`) |
//! | 3 | cycle harness | time each `decode_one` window |
//! | 4 | cadence test | decode into alternating playback halves |
//!
//! The entry point [`decode_one`] decodes exactly one frame **from its
//! seek-table offset**, with a fresh cursor. Frames are independently
//! decodable by spec (§9.2.5 — warm-up lives in each subframe's own header,
//! measured 64/64 in `scripts/measure_warmup_semantics.py`), and decoding
//! from the offset makes the seek table itself a witness: a wrong offset
//! fails the header parse or trips the blocksize cross-check against the
//! table before a sample is written. [`decode_clip`] loops `decode_one` over
//! the whole clip, calling the per-frame callback.
//!
//! Buffer contract (mirrors `flac-lite`'s): the caller owns `left`/`right`
//! at least `clip.max_blocksize` long; each frame writes exactly its own
//! blocksize and leaves the tail untouched — playback reuses one fixed buffer
//! per channel, and the final frame is legitimately short.

use crate::assets::SpikeClip;
use flac_lite::bits::BitReader;
use flac_lite::frame::{self, FrameHeader, StreamDefaults};
use flac_lite::subframe::PredictorState;

/// Why a clip walk stopped. Every variant names the frame and the evidence,
/// because in the witness tests each one means "the committed assets or the
/// decoder regressed" — never a transient condition to retry.
#[derive(Debug, PartialEq, Eq)]
pub enum DriverError {
    /// Caller buffers shorter than `clip.max_blocksize` — rejected before the
    /// first bit of the first frame, nothing written (crate rejection rule).
    BufferTooSmall {
        /// Required length per channel (`clip.max_blocksize`).
        need: usize,
        /// Length actually provided for the failing channel.
        have: usize,
    },
    /// A seek-table offset runs past the embedded region — the table and the
    /// blob are not a matching pair (hand-edit? bad regen?).
    FrameOffsetOutOfRange {
        /// Index into the seek table.
        frame: usize,
        /// The offset that cannot be satisfied.
        offset: usize,
        /// Region length it was checked against.
        len: usize,
    },
    /// The parsed header disagrees with the seek table's per-frame blocksize.
    /// The table is generator output derived from these same bytes, so a
    /// mismatch means one side was corrupted after generation.
    MetadataMismatch {
        /// Index into the seek table.
        frame: usize,
        /// Blocksize the table claims.
        table: usize,
        /// Blocksize the parsed header actually carries.
        header: usize,
    },
    /// `flac-lite` rejected the frame (or the region ended mid-frame).
    Decode(flac_lite::Error),
}

/// Totals from a complete clip walk.
#[derive(Debug, PartialEq, Eq)]
pub struct Stats {
    /// Frames decoded.
    pub frames: usize,
    /// Samples decoded per channel (sum of per-frame blocksizes).
    pub samples: usize,
}

/// Decode exactly one frame — at its seek-table offset, with a fresh cursor —
/// into the first `blocksize` slots of the caller buffers. Returns the number
/// of samples written (the frame's own blocksize, cross-checked against the
/// table before any decode). This is the window PR 3 wraps in the cycle
/// counter; PR 4 alternates buffer halves between calls.
pub fn decode_one(
    clip: &SpikeClip,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, DriverError> {
    let frame = &clip.frames[frame_index];
    let max = usize::from(clip.max_blocksize);
    if left.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: left.len(),
        });
    }
    if clip.channels == 2 && right.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: right.len(),
        });
    }
    // `usize::from(u32)` does not exist on ANY platform (usize's width is
    // target-dependent, so core never implements it) — route through
    // try_from. Infallible on this project's targets (arm 32-bit, host
    // 64-bit); on a hypothetical narrower target it degrades to a clean
    // out-of-range rejection instead of a silent wrap.
    let offset = match usize::try_from(frame.offset) {
        Ok(offset) => offset,
        Err(_) => {
            return Err(DriverError::FrameOffsetOutOfRange {
                frame: frame_index,
                offset: usize::MAX,
                len: clip.region.len(),
            });
        }
    };
    let region = clip
        .region
        .get(offset..)
        .ok_or(DriverError::FrameOffsetOutOfRange {
            frame: frame_index,
            offset,
            len: clip.region.len(),
        })?;

    let defaults = StreamDefaults {
        sample_rate_hz: clip.sample_rate_hz,
        bits_per_sample: clip.bits_per_sample,
    };
    let mut reader = BitReader::new(region);
    let header = FrameHeader::parse(&mut reader, &defaults).map_err(DriverError::Decode)?;
    if header.blocksize != usize::from(frame.blocksize) {
        return Err(DriverError::MetadataMismatch {
            frame: frame_index,
            table: usize::from(frame.blocksize),
            header: header.blocksize,
        });
    }
    frame::decode_frame(&mut reader, &header, left, right, state).map_err(DriverError::Decode)
}

/// Walk the whole clip frame-by-frame from the seek table, handing each
/// decoded block to `on_frame`. The callback runs per frame (not per sample):
/// hashing, timing bookkeeping, and buffer rotation all have what they need
/// at the frame boundary, and a per-sample callback would measure the
/// callback instead of the decode.
pub fn decode_clip<F>(
    clip: &SpikeClip,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
    mut on_frame: F,
) -> Result<Stats, DriverError>
where
    F: FnMut(usize, &[i32], &[i32]),
{
    let mut stats = Stats {
        frames: 0,
        samples: 0,
    };
    for frame_index in 0..clip.frames.len() {
        let written = decode_one(clip, frame_index, left, right, state)?;
        // Mono clips keep a zero-length `right` (flac-lite's contract) —
        // slicing it to `written` would panic; hand the callback the empty
        // window instead.
        let right_window: &[i32] = if clip.channels == 2 {
            &right[..written]
        } else {
            &[]
        };
        on_frame(frame_index, &left[..written], right_window);
        stats.frames += 1;
        stats.samples += written;
    }
    Ok(stats)
}
