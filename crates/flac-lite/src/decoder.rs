//! Top-level decode API: manifest-driven frame cursor + per-frame PCM.
//!
//! SCAFFOLD STATE: types and signatures only, bodies are `todo!()`.
//!
//! This is the only module a ROM normally touches. The usage contract the
//! playback layer will code against:
//!
//! ```ignore
//! static SONG: &[u8] = include_bytes!("song.gfp");   // memory-mapped ROM
//!
//! let manifest = Manifest::parse(SONG)?;             // borrowed, no alloc
//! let mut dec = Decoder::new(&manifest)?;            // predictor state only
//! let mut bufs = FrameBuffers::new(&mut scratch);    // caller-owned scratch
//!
//! while let Some(frame) = dec.decode_next(&mut bufs)? {
//!     // hand frame.samples to the DMA double-buffer / mixer
//! }
//! ```
//!
//! Design constraints that must survive implementation:
//!
//! - **No allocation.** `alloc` is not imported here. Scratch memory is
//!   caller-provided (an IWRAM/EWRAM `static mut` on the ROM side) and borrowed
//!   for the lifetime of a call — the borrow checker is what lets us skip
//!   `unsafe`.
//! - **No I/O traits.** Frames are addressed through the manifest's offset
//!   table; the cursor only moves forward, and [`Decoder::seek_frame`] is an
//!   array lookup.
//! - **Incremental by frame.** One call produces one frame's PCM and returns,
//!   so the caller controls scheduling against the DMA IRQ (real-time budget is
//!   per frame, ≈64ms of audio at 2048/32kHz).

use crate::format::Manifest;
use crate::frame::FrameHeader;
use crate::subframe::PredictorState;

/// Caller-owned decode scratch: the two channel buffers a frame is decoded into.
///
/// Wraps a `&mut [i32]` pair so the decoder can assert length ≥ block size and
/// reuse the same memory for every frame (no per-frame allocation, no
/// reallocation).
#[derive(Debug)]
pub struct FrameBuffers<'a> {
    left: &'a mut [i32],
    right: &'a mut [i32],
}

impl<'a> FrameBuffers<'a> {
    /// Borrow scratch memory. `left`/`right` must each be ≥ the manifest's
    /// block size; mono streams may pass a zero-length `right`.
    pub fn new(left: &'a mut [i32], right: &'a mut [i32]) -> Self {
        todo!("flac-lite scaffold: FrameBuffers::new")
    }

    /// Minimum element count each channel buffer needs for `manifest`.
    pub fn required_len(manifest: &Manifest<'_>) -> usize {
        todo!("flac-lite scaffold: FrameBuffers::required_len")
    }
}

/// One decoded frame of PCM.
#[derive(Clone, Copy, Debug)]
pub struct DecodedFrame<'a> {
    /// Mono: left channel. Stereo: left channel.
    pub left: &'a [i32],
    /// Stereo right channel; empty for mono.
    pub right: &'a [i32],
    /// Parsed header of the frame just decoded.
    pub header: FrameHeader,
    /// Index of this frame within the track.
    pub index: usize,
}

/// Sequential decoder over a packed blob's frames.
#[derive(Debug)]
pub struct Decoder<'a> {
    /// Stream facts + frame index (borrowed from the ROM blob).
    manifest: Manifest<'a>,
    /// Predictor warm-up, one slot per subframe (per channel).
    state: [PredictorState; 2],
    /// Next frame index to decode.
    next: usize,
    /// Total samples emitted so far (for position reporting / partial frames).
    samples_consumed: u64,
}

impl<'a> Decoder<'a> {
    /// Prepare a decoder for a manifest. Validates the manifest against the
    /// default constrained profile.
    pub fn new(manifest: &Manifest<'a>) -> crate::Result<Self> {
        todo!("flac-lite scaffold: Decoder::new")
    }

    /// Decode the next frame into `bufs`, advancing the cursor.
    ///
    /// Returns `Ok(None)` at end of stream. `Err` leaves the cursor
    /// un-advanced for recoverable frame errors, so a caller can skip a corrupt
    /// frame with [`Decoder::seek_frame`] + continue.
    pub fn decode_next<'b>(
        &mut self,
        bufs: &mut FrameBuffers<'b>,
    ) -> crate::Result<Option<DecodedFrame<'b>>> {
        todo!("flac-lite scaffold: Decoder::decode_next")
    }

    /// Move the cursor to an arbitrary frame (O(1); the manifest's offset table
    /// *is* the seek table). Resets predictor state, since warm-up samples from
    /// a non-adjacent preceding frame would be wrong.
    pub fn seek_frame(&mut self, index: usize) -> crate::Result<()> {
        todo!("flac-lite scaffold: Decoder::seek_frame")
    }

    /// Current frame cursor.
    pub fn frame_index(&self) -> usize {
        todo!("flac-lite scaffold: Decoder::frame_index")
    }

    /// Samples decoded so far.
    pub fn sample_position(&self) -> u64 {
        todo!("flac-lite scaffold: Decoder::sample_position")
    }

    /// Manifest backing this decoder.
    pub fn manifest(&self) -> &Manifest<'a> {
        todo!("flac-lite scaffold: Decoder::manifest")
    }
}
