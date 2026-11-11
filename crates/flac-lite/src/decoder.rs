//! Top-level decode API: manifest-driven frame cursor + per-frame PCM.
//!
//! This is the module a ROM normally touches, and the crate's product path.
//! The usage contract the playback layer codes against:
//!
//! ```ignore
//! static SONG: &[u8] = include_bytes!("song.gfp");   // memory-mapped ROM
//!
//! let manifest = Manifest::parse(SONG)?;             // borrowed, no alloc
//! let mut dec = Decoder::new(&manifest)?;            // predictor state only
//! let mut bufs = FrameBuffers::new(&mut left, &mut right);
//!
//! while let Some(frame) = dec.decode_next(&mut bufs)? {
//!     // hand frame.left / frame.right to the DMA double-buffer / mixer
//! }
//! ```
//!
//! Design constraints that survive implementation:
//!
//! - **No allocation.** `alloc` is not imported here. Scratch memory is
//!   caller-provided (an IWRAM/EWRAM `static mut` on the ROM side) and borrowed
//!   for the lifetime of a call — the borrow checker is what lets us skip
//!   `unsafe`.
//! - **No I/O traits.** Frames are addressed through the manifest's offset
//!   table; the cursor only moves forward, and [`Decoder::seek_frame`] is an
//!   array lookup.
//! - **Incremental by frame.** One call produces one frame's PCM and returns,
//!   so the caller controls scheduling against the DMA IRQ (the real-time budget
//!   is per frame, ≈64 ms of audio at 2048/32 kHz).
//!
//! ## A shell over [`crate::scan`], not a third frame loop
//!
//! This module owns **no frame loop**. It builds a [`crate::scan::Frames`] over
//! the manifest's frame region, hands it the manifest's stream defaults and
//! bounds, and calls `next_frame` once per [`Decoder::decode_next`]. Everything
//! that makes a walk correct — the field walk that derives each frame's end
//! from its own header, the contiguity check on frame numbers, the
//! end-of-region rule, the declared-samples cross-check,
//! [`crate::scan::Frames::resync`] — is inherited rather than re-derived. The
//! alternative shape (`crate::driver`) still exists for the measurement
//! instrument; its index-driven `decode_one` is what a *timing harness* wants,
//! and this is what a *product* wants.
//!
//! ### Consequences of that choice, stated rather than discovered
//!
//! - **The facade requires the `scan` feature.** That is the honest dependency,
//!   not a build accident: with the walk absent there is nothing to be a shell
//!   over. Only a ROM that drives the facade pays for the walk — measured at
//!   **+3,120 B** of `.text` on the instrument when it is switched on — which is
//!   why it stays a feature rather than becoming unconditional.
//! - **End of stream** is the walk's rule: `Ok(None)` means *the region is
//!   exhausted and its declarations agree* — every frame consumed, and (because
//!   a manifest pins both a frame count and a sample count) those declarations
//!   checked. A region that runs out early is an error, not a quiet end.
//! - **Errors are the crate's flat enum with their numbers kept.** A decode
//!   failure passes through unchanged; a blob that disagrees with its own
//!   declarations arrives as [`crate::Error::StreamMismatch`] (with declared vs
//!   actual), and a frame out of sequence as
//!   [`crate::Error::FrameNumberMismatch`] — the numbers are the diagnosis, so
//!   flattening them to a bare variant would be throwing away the useful part.
//! - **The kernel is chosen by type**, exactly as the instrument's arms do it:
//!   [`Decoder::decode_next`] names [`crate::residual::TableCountKernel`] and
//!   [`Decoder::decode_next_with`] takes any
//!   [`crate::residual::RiceKernel`], so a ROM links the overlay's ARM kernel
//!   through this facade without the core depending on the overlay.
//!
//! ## Seeking
//!
//! [`Decoder::seek_frame`] rebuilds the walk at `offsets[i]` and re-anchors it
//! at frame number `i`. That is O(1) and needs no warm-up reconstruction
//! because every subframe in this format carries its own warm-up samples
//! (corrected 2026-09-10; the old "reset state from the previous frame" concern
//! was the scaffold's myth, and the scratch reset here is bookkeeping only).

use crate::format::Manifest;
use crate::frame::FrameHeader;
use crate::residual::{RiceKernel, TableCountKernel};
use crate::scan::Frames;
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
        Self { left, right }
    }

    /// Minimum element count each channel buffer needs for `manifest`.
    ///
    /// The manifest's block size is an upper bound, not per-frame truth — the
    /// last frame of a track is legitimately short — so this is the size that
    /// never has to grow between frames.
    pub fn required_len(manifest: &Manifest<'_>) -> usize {
        usize::from(manifest.blocksize())
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
    /// The frame walk this facade is a shell over: built from the manifest's
    /// region, defaults and bounds, re-anchored on seek, advanced one frame per
    /// call. Owning it (rather than a byte cursor) is what makes this module
    /// inherit every walk rule instead of re-deriving it.
    walk: Frames<'a>,
    /// Per-subframe warm-up scratch, one slot per channel/subframe. Reused,
    /// never retained as history — each subframe carries its own warm-up
    /// samples (see `subframe` module docs; corrected 2026-09-10).
    state: [PredictorState; 2],
    /// Next frame index to decode.
    next: usize,
    /// Samples emitted since the decoder was built or last seeked.
    samples_consumed: u64,
}

impl<'a> Decoder<'a> {
    /// Prepare a decoder for a manifest.
    ///
    /// With `strict-profile` on, the manifest is held to
    /// [`crate::format::DEFAULT_PROFILE`] — the same switch, and the same
    /// reason, as the frame layer's own profile refusal: the release path
    /// trusts the packer, while a profile-strict build convicts it. The
    /// *product* guarantee is the packer's (6b-2 validates at pack time); this
    /// is the optional runtime re-check.
    pub fn new(manifest: &Manifest<'a>) -> crate::Result<Self> {
        if cfg!(feature = "strict-profile") {
            crate::format::DEFAULT_PROFILE.validate(manifest)?;
        }
        let walk = Frames::region(
            manifest.frame_region(),
            &manifest.stream_defaults(),
            manifest.bounds(),
        )
        .anchored(0);
        Ok(Self {
            manifest: *manifest,
            walk,
            state: [PredictorState::new(), PredictorState::new()],
            next: 0,
            samples_consumed: 0,
        })
    }

    /// Decode the next frame into `bufs` through the default Rice kernel,
    /// advancing the cursor.
    ///
    /// Returns `Ok(None)` at end of stream. `Err` leaves the cursor un-advanced
    /// for recoverable frame errors, so a caller can skip a corrupt frame with
    /// [`Decoder::seek_frame`] and continue.
    pub fn decode_next<'b>(
        &mut self,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> crate::Result<Option<DecodedFrame<'b>>> {
        self.decode_next_with::<TableCountKernel>(bufs)
    }

    /// [`Decoder::decode_next`] with the Rice kernel named by the caller — the
    /// entry a ROM's kernel choice uses, exactly as
    /// [`crate::frame::decode_frame_with`] is for the frame layer.
    ///
    /// **On the returned lifetime:** `DecodedFrame<'b>` borrows the *write*, not
    /// the scratch allocation, so a frame's samples are valid until the next
    /// call and the borrow checker enforces the hand-off. The scaffold spelled
    /// this `FrameBuffers<'b>` → `DecodedFrame<'b>`, which no implementation can
    /// satisfy: handing out `&'b [i32]` (the allocation's lifetime) from a
    /// `&mut FrameBuffers<'b>` would alias the caller's own mutable borrow.
    pub fn decode_next_with<'b, K: RiceKernel>(
        &mut self,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> crate::Result<Option<DecodedFrame<'b>>> {
        // No `next >= frame_count` shortcut, deliberately: the *walk* owns the
        // end-of-stream rule, and its last step is the cross-check of the
        // region's declarations (frame count, sample count) against what the
        // bytes produced. Short-circuiting on the manifest's own count would
        // skip exactly that check and turn a lying blob into a quiet
        // end-of-stream — the one failure this facade exists to make loud.
        let written = {
            // The mutable reborrows live only for the walk's duration; the
            // shared borrows below start after this block ends.
            let left: &mut [i32] = bufs.left;
            let right: &mut [i32] = bufs.right;
            match self.walk.next_frame_with::<K>(left, right, &mut self.state) {
                Ok(Some(consumed)) => consumed,
                // The walk says the region is exhausted. With a frame count in
                // the bounds this can only follow a complete walk; a blob whose
                // bytes end early surfaces as the walk's own error instead.
                Ok(None) => return Ok(None),
                Err(err) => return Err(walk_error(err)),
            }
        };

        let index = self.next;
        self.next += 1;
        self.samples_consumed += written.samples as u64;
        let samples = written.samples;
        let right: &'b [i32] = if self.manifest.channels() == 2 {
            &bufs.right[..samples]
        } else {
            &[]
        };
        Ok(Some(DecodedFrame {
            left: &bufs.left[..samples],
            right,
            header: written.header,
            index,
        }))
    }

    /// Move the cursor to an arbitrary frame (O(1); the manifest's offset table
    /// *is* the seek table). Safe by construction: every subframe carries its
    /// own warm-up samples, so any frame is independently decodable and no
    /// cross-frame warm-up reconstruction exists to get wrong (corrected
    /// 2026-09-10 — the old "resets state from the previous frame" concern was
    /// the scaffold's myth; the scratch reset is bookkeeping only).
    ///
    /// `index == frame_count()` is legal and means "end of stream": the next
    /// [`Decoder::decode_next`] returns `Ok(None)`.
    pub fn seek_frame(&mut self, index: usize) -> crate::Result<()> {
        let count = self.manifest.frame_count();
        if index > count {
            return Err(crate::Error::Manifest);
        }
        let region = self.manifest.frame_region();
        if index == count {
            // Park the walk past the end: an empty region ends immediately, and
            // with no bounds to check nothing is declared against nothing.
            self.walk = Frames::region(
                &region[region.len()..],
                &self.manifest.stream_defaults(),
                crate::scan::StreamBounds::NOTHING,
            );
            self.next = count;
        } else {
            let offset = self.manifest.frame_offset(index)? - self.manifest.frame_data_start();
            self.walk = Frames::region(
                &region[offset..],
                &self.manifest.stream_defaults(),
                self.manifest.bounds(),
            )
            .anchored(index as u64);
            self.next = index;
        }
        self.samples_consumed = 0;
        Ok(())
    }

    /// Current frame cursor: the index the next [`Decoder::decode_next`] will
    /// decode.
    pub fn frame_index(&self) -> usize {
        self.next
    }

    /// Samples emitted by this decoder since it was built or last seeked.
    ///
    /// Not a stream position: after a seek it restarts at 0, because the count
    /// is of PCM handed out and a seek is a jump rather than a walk.
    pub fn sample_position(&self) -> u64 {
        self.samples_consumed
    }

    /// Manifest backing this decoder.
    pub fn manifest(&self) -> &Manifest<'a> {
        &self.manifest
    }
}

/// Map a walk error onto the crate's flat error, keeping the numbers.
///
/// The walk's errors carry payloads because the numbers *are* the diagnosis.
/// Three groups, three verdicts:
///
/// - a decode failure passes through unchanged;
/// - "the blob disagrees with its own declarations" becomes
///   [`crate::Error::StreamMismatch`] — a re-pack problem, and the declared vs
///   actual pair is what tells the caller which way it went;
/// - "these are not the bytes the numbering says" becomes
///   [`crate::Error::FrameNumberMismatch`];
/// - a caller-contract violation (buffers too short) becomes
///   [`crate::Error::InvalidField`], the same verdict the frame layer reaches
///   for short buffers before it reads a bit.
fn walk_error(err: crate::scan::Error) -> crate::Error {
    use crate::scan::Error as Walk;
    match err {
        Walk::Decode(inner) => inner,
        Walk::DeclaredSampleCountMismatch {
            declared, decoded, ..
        } => crate::Error::StreamMismatch {
            declared,
            actual: decoded as u64,
        },
        Walk::DeclaredFrameCountMismatch {
            declared, decoded, ..
        } => crate::Error::StreamMismatch {
            declared: declared as u64,
            actual: decoded as u64,
        },
        Walk::FrameOutOfSequence {
            expected, found, ..
        } => crate::Error::FrameNumberMismatch { expected, found },
        Walk::BufferTooSmall { .. } | Walk::DeclaredMaxExceedsBuffers { .. } => {
            crate::Error::InvalidField
        }
    }
}
