//! Per-frame decode loop over a borrowed frame index — the portable core's
//! **copy** of this repo's proven frame-loop driver.
//!
//! PORTED 2026-11-04 as a *narrow copy* of `examples/flac_spike/src/driver.rs`
//! (FLAC.md → split proposal, PR-ladder item 4: "the `Decoder::decode_next`
//! per-frame loop"). "Narrow" is load-bearing: the decode loop's semantics are
//! carried across verbatim, and everything the copy *needs* to change is listed
//! under [What the copy changed](#what-the-copy-changed) below — nothing else
//! moved, and no new frame-finding, error-recovery, or seeking behaviour was
//! invented here.
//!
//! Copying rather than sharing one file across a dependency edge is the
//! deliberate call, not duplication by accident:
//!
//! - **The spike's `driver::decode_one` is the measurement instrument.** It is
//!   the `ctrl` arm of every recorded E entry, and the cross-session anchors
//!   (FLAC.md, E9: "ctrl reproduces the E10-recorded sums at exactly −314
//!   total") hold because that exact call shape is unchanged. Moving it into
//!   this crate would put the fixture on the moving side of the split — and
//!   E2b's lesson (a −21.7% sample landing at +38% on the live link) is that
//!   call/link shape is exactly where measured numbers die. The instrument
//!   stays where it is; the rewire is split-ladder item 6's job.
//! - **The loop's argument types must not be generated assets.** The spike's
//!   `SpikeClip` is a `pub static` with witness-only fields (`name`, `why`,
//!   `census`, `pcm_file`, FNV pins) and a `&'static` region (the
//!   `include_bytes!` contract). A portable core needs borrowed inputs at any
//!   lifetime — which is the shape the E1 sample already proved:
//!   `experiments/e1-rom-ram-staging`'s `ClipView` decodes ROM bytes and a
//!   staged RAM copy through one code path.
//!
//! Drift is therefore **witness-enforced, not shared-dependency-enforced** —
//! the same discipline the E-samples use for vendored loops:
//!
//! | Witness | What it pins |
//! |---|---|
//! | `tests/frame_run_loop.rs` (this crate, `make flac-test`) | this loop over the committed libFLAC run table: bit-exact `flac -d` PCM, per-frame independence in reverse order, the index-integrity rejections |
//! | `examples/flac_spike/tests/spike_witness.rs` (`make spike-test`) | both copies over the same embedded clip bytes: frame-by-frame window equality, and the core copy alone reaching the generator's PCM pins through the ROM's own fold |
//!
//! ## What this module is *not*
//!
//! - **Not the [`crate::decoder`] facade.** `decoder`'s manifest-driven
//! ## Deprecation intent (decided 2026-11-10, 6b-1)
//!
//! This module is **intended to be deprecated and eventually removed**. It is the
//! instrument's per-frame loop — index-driven, kernel-generic, shaped for the
//! performance spikes — and the *product* path is the facade in `crate::decoder`,
//! which is a shell over `crate::scan`. Once the measurement instrument retires,
//! there is no caller left, and a crate meant to stand alone should not ship two
//! per-frame entry points.
//!
//! Why it is not removed *now*: deleting or relocating it moves code across a
//! crate boundary, which changes inlining and re-pins every recorded cycle
//! anchor (6a-3 measured ~168–199 K cycles/frame from merely adding a second
//! instantiator). So the sequence is: demote it behind an off-by-default
//! `instrument` feature with `#[doc(hidden)]` — so the public contract is one
//! loop — and delete it when the instrument goes, not before. The E-series
//! numbers are attributable to this file's source text; that is the thing being
//! protected.
//!
//! A future reader who finds this note after the instrument is gone should treat
//! it as a standing instruction, not a historical remark.
//!   `Decoder`/`FrameBuffers`/`DecodedFrame` are no longer scaffold: item 6b-1
//!   (2026-11-10) landed them as the thin shell this note predicted — over
//!   `scan::Frames` rather than over this loop, because the manifest-backed
//!   product path needed a frame source that finds its own frames. The shape is
//!   the one the next bullet describes; what nobody wrote down in advance is that
//!   the facade would land on the walk rather than here, which is why this file
//!   is now the instrument's and not the product's.
//! - **Not a frame *finder*.** Frames come from the caller's index. The
//!   stream-scan cursor (the measured sync+CRC-8+field-validation filter,
//!   FLAC.md → "Frame sync scanning") landed as item 4b's `scan` module, and it
//!   supplies a different frame source to the same loop — which is the point of
//!   keeping the source borrowed here, and which is exactly what 6b-1's facade
//!   then consumed.
//!
//! ## Decode contract carried over verbatim
//!
//! Nothing here is measurement-specific; the *consumers* differ, and this
//! module knows nothing about them. The loop hands each decoded block to a
//! callback at the frame boundary (hashing, timing bookkeeping, and buffer
//! rotation all have what they need there; a per-sample callback would measure
//! the callback instead of the decode).
//!
//! [`decode_one`] decodes exactly one frame **at its index offset**, with a
//! fresh cursor. Frames are independently decodable by spec (§9.2.5 — warm-up
//! lives in each subframe's own header; measured 64/64 in
//! `scripts/measure_warmup_semantics.py`), and decoding from the offset makes
//! the index itself a witness: a wrong offset fails the header parse or trips
//! the blocksize cross-check before a sample is written.
//!
//! Buffer contract (identical to [`crate::frame::decode_frame`]'s): the caller
//! owns `left`/`right` at least `source.max_blocksize` long; each frame writes
//! exactly its own blocksize and leaves the tail untouched — playback reuses
//! one fixed buffer per channel, and the final frame of a track is
//! legitimately short.
//!
//! ## What the copy changed
//!
//! Every difference, so a reviewer can diff the two files against this list
//! rather than rediscovering it (all of them required by the move, none of
//! them behavioural):
//!
//! 1. **Source type.** `&SpikeClip` → [`FrameSource`] (borrowed, any
//!    lifetime). Field set = the E1 `ClipView`'s: the decode inputs only.
//!    Dropped as not-needed-by-the-loop: `name`, `why`, `census`, `total_samples`,
//!    `pcm_file`, `fnv_frames`, `fnv_pcm`. Frame and sample counts come from
//!    the index itself, which is where the walk's end comes from too.
//! 2. **Stream defaults are stored, not rebuilt per frame.** The origin
//!    constructed a `StreamDefaults` inside `decode_one` from the clip's two
//!    fields; here the source carries one and the loop passes `&source.defaults`.
//!    That construction sat *inside* the spike's timed window, so it is called
//!    out rather than silently dropped. No behaviour change: same values.
//! 3. **`MetadataMismatch` → [`Error::IndexMismatch`].** Not cosmetic: in this
//!    crate "metadata" is a loaded term (FLAC metadata blocks, RFC 9639 §8.1,
//!    and the product-side `format::Manifest`). The variant is about the
//!    *index*, and the name now says so. Same check, same position in the
//!    order, same evidence.
//! 4. **Error type is module-local** ([`Error`], payload-carrying) wrapping the
//!    crate's flat [`crate::Error`] in [`Error::Decode`]. The evidence-carrying
//!    shape is kept because the origin's reason for it is kept ("every variant
//!    names the frame and the evidence"). Whether the crate-level flat enum
//!    should instead grow payloads — or these conditions become source-level
//!    rejections before decode — is an API decision deferred to the facade
//!    step, not decided here.
//! 5. **Naming.** `decode_clip` → [`decode_all`]: "clip" is product
//!    vocabulary for a thing this crate does not own. `decode_one`,
//!    `FrameMeta`, `Stats` keep their names so the two files diff
//!    line-for-line where they are supposed to match.
//! 6. **The index-integrity claim is generalised, honestly.** The origin could
//!    assert "the table is generator output derived from these same bytes";
//!    this crate states the weaker true form — the check is meaningful *for a
//!    table-backed source* (a packer emitting both the table and the frames
//!    from one encode), and it is not a decoder precondition: a future
//!    scan-cursor source has no table to cross-check and simply never reaches
//!    this variant.

use crate::bits::BitReader;
use crate::frame::{self, FrameHeader, StreamDefaults};
use crate::subframe::PredictorState;

/// One entry of the frame index: where a frame starts in the region, and how
/// many samples per channel it decodes to.
///
/// This is the shape the packer's frame-offset table already has (and the
/// perf spike's generated seek table is its working stand-in): the packer
/// emits *both* fields offline, from the same encode that produced the frames.
/// That is what makes the blocksize cross-check in [`decode_one`] a corruption
/// detector rather than an assumption. An index built any other way carries no
/// such claim — see module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMeta {
    /// Byte offset of the frame header within the region.
    pub offset: u32,
    /// Samples per subframe for THIS frame. Per-frame truth, not a
    /// track-wide constant: the tail frame is short.
    pub blocksize: u16,
}

/// The decode inputs a frame loop needs — fully borrowed, no allocation, no
/// lifetime longer than the caller's.
///
/// The field set is the E1 sample's `ClipView` (the shape already measured
/// decoding ROM-resident and RAM-staged bytes through one code path), minus
/// anything that only the spike's asset generator needs. Construction is a
/// struct literal on purpose: there is no parse step here, and inventing one
/// would imply this type owns a byte layout, which it does not (the packed
/// container's layout is `format`'s, and its parser is the manifest step's).
#[derive(Clone, Copy, Debug)]
pub struct FrameSource<'a> {
    /// The frame index: frame 0 at offset 0, ascending, tiling the region.
    pub frames: &'a [FrameMeta],
    /// The raw frame region the offsets index into.
    pub region: &'a [u8],
    /// Largest per-frame blocksize: the caller's required buffer length per
    /// channel, checked before any bit is read.
    pub max_blocksize: u16,
    /// 1 = mono, 2 = stereo — the `right`-buffer contract only. The frame
    /// header's own channel code is what actually drives decoding
    /// ([`crate::frame::decode_frame`] derives subframe count and side slot
    /// from it, never from this field), so a source that *understates* the
    /// count is still caught downstream by `decode_frame`'s own short-buffer
    /// gate, and one that overstates it only asks the caller for a buffer the
    /// frame will not use. It cannot make a wrong frame decode right.
    pub channels: u8,
    /// Stream defaults resolving the header fields a frame leaves unresolved
    /// (sample-rate code `0b0000`, sample-size code `0b000`) — see
    /// [`StreamDefaults`]. The manifest step will fill this from the parsed
    /// container; the spike fills it from generated constants.
    pub defaults: StreamDefaults,
}

/// Why a frame walk stopped. Every variant names the frame and the evidence,
/// because in the witness tests each one means "the index, the region, or the
/// decoder regressed" — never a transient condition to retry.
///
/// Payload-carrying on purpose (see module docs' list, item 4): the
/// crate-level [`crate::Error`] is a flat `Copy` enum, and these are
/// *caller- and index-contract* conditions whose useful diagnosis is the
/// numbers involved. Mapping them onto the flat enum would throw away exactly
/// what a witness test asserts.
#[derive(Debug)]
pub enum Error {
    /// Caller buffers shorter than `source.max_blocksize` — rejected before
    /// the first bit of the first frame, nothing written (crate rejection
    /// rule: rejections read and write nothing).
    BufferTooSmall {
        /// Required length per channel (`source.max_blocksize`).
        need: usize,
        /// Length actually provided for the failing channel.
        have: usize,
    },
    /// An index offset runs past the region — the index and the region are not
    /// a matching pair (hand-edited? bad pack?).
    FrameOffsetOutOfRange {
        /// Index into the frame index.
        frame: usize,
        /// The offset that cannot be satisfied.
        offset: usize,
        /// Region length it was checked against.
        len: usize,
    },
    /// The frame index's per-frame blocksize disagrees with the blocksize the
    /// frame header itself carries. Renamed from the origin's
    /// `MetadataMismatch` — "metadata" names a different thing in this crate
    /// (FLAC metadata blocks, §8.1; `format::Manifest`). Fires only for a
    /// table-backed source; see [`FrameMeta`].
    IndexMismatch {
        /// Index into the frame index.
        frame: usize,
        /// Blocksize the index claims.
        claimed: usize,
        /// Blocksize the parsed header actually carries.
        header: usize,
    },
    /// `flac-lite` rejected the frame (or the region ended mid-frame). The
    /// crate's own flat error, carried through unchanged.
    Decode(crate::Error),
}

/// Totals from a complete walk of an index.
#[derive(Debug, PartialEq, Eq)]
pub struct Stats {
    /// Frames decoded.
    pub frames: usize,
    /// Samples decoded per channel (sum of per-frame blocksizes).
    pub samples: usize,
}

/// Decode exactly one frame — at its index offset, with a fresh cursor — into
/// the first `blocksize` slots of the caller buffers. Returns the number of
/// samples written (the frame's own blocksize, cross-checked against the index
/// before any decode).
///
/// `frame_index` must be `< source.frames.len()`; that is the loop's own
/// domain, and indexing it directly (as the origin does) is the caller
/// contract — an out-of-range index is a caller bug, pinned by a unit test
/// below rather than turned into an error variant here.
pub fn decode_one(
    source: &FrameSource<'_>,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, Error> {
    let frame = source.frames[frame_index];
    let max = usize::from(source.max_blocksize);
    if left.len() < max {
        return Err(Error::BufferTooSmall {
            need: max,
            have: left.len(),
        });
    }
    if source.channels == 2 && right.len() < max {
        return Err(Error::BufferTooSmall {
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
            return Err(Error::FrameOffsetOutOfRange {
                frame: frame_index,
                offset: usize::MAX,
                len: source.region.len(),
            });
        }
    };
    let region = source
        .region
        .get(offset..)
        .ok_or(Error::FrameOffsetOutOfRange {
            frame: frame_index,
            offset,
            len: source.region.len(),
        })?;

    let mut reader = BitReader::new(region);
    let header = FrameHeader::parse(&mut reader, &source.defaults).map_err(Error::Decode)?;
    if usize::from(frame.blocksize) != header.blocksize {
        return Err(Error::IndexMismatch {
            frame: frame_index,
            claimed: usize::from(frame.blocksize),
            header: header.blocksize,
        });
    }
    frame::decode_frame(&mut reader, &header, left, right, state).map_err(Error::Decode)
}

/// Walk the whole index frame-by-frame, handing each decoded block to
/// `on_frame`. The callback runs per frame, not per sample (module docs).
///
/// `Ok(Stats)` means the walk reached the end of the index; the end of the
/// index *is* the end of the stream for this source kind — there is no
/// resync or `Ok(None)`-at-EOF machinery here (that belongs to the facade and
/// to the scan-cursor rung).
pub fn decode_all<F>(
    source: &FrameSource<'_>,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
    mut on_frame: F,
) -> Result<Stats, Error>
where
    F: FnMut(usize, &[i32], &[i32]),
{
    let mut stats = Stats {
        frames: 0,
        samples: 0,
    };
    for frame_index in 0..source.frames.len() {
        let written = decode_one(source, frame_index, left, right, state)?;
        // Mono sources keep a zero-length `right` (flac-lite's contract) —
        // slicing it to `written` would panic; hand the callback the empty
        // window instead.
        let right_window: &[i32] = if source.channels == 2 {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An index/region pair with no frames in it: these tests witness the
    /// loop's **index arithmetic and caller contract**, and deliberately touch
    /// no bit layout — so a hand-written region here claims nothing about
    /// encoder bytes (the hand-packed-vector rule, FLAC.md). Everything that
    /// does claim something about real frames is witnessed in
    /// `tests/frame_run_loop.rs` against the committed run table.
    fn empty_source<'a>(frames: &'a [FrameMeta], region: &'a [u8]) -> FrameSource<'a> {
        FrameSource {
            frames,
            region,
            max_blocksize: 8,
            channels: 1,
            defaults: StreamDefaults {
                sample_rate_hz: 32_000,
                bits_per_sample: 16,
            },
        }
    }

    #[test]
    fn an_out_of_range_offset_rejects_without_touching_a_buffer() {
        const SENTINEL: i32 = 0x5A5A_5A5A;
        let region = [0u8; 4];
        // offset 5 > region end 4, and blocksize claims are irrelevant: the
        // offset check runs before the parse, so nothing is decoded.
        let source = empty_source(
            &[FrameMeta {
                offset: 5,
                blocksize: 8,
            }],
            &region,
        );
        let mut left = [SENTINEL; 8];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        let err = decode_one(&source, 0, &mut left, &mut right, &mut state)
            .expect_err("an out-of-range offset must not decode");
        match err {
            Error::FrameOffsetOutOfRange { frame, offset, len } => {
                assert_eq!((frame, offset, len), (0, 5, 4), "evidence fields");
            }
            other => panic!("expected FrameOffsetOutOfRange, got {other:?}"),
        }
        assert_eq!(
            left, [SENTINEL; 8],
            "rejection wrote into the caller buffer"
        );
    }

    #[test]
    fn a_short_buffer_rejects_before_the_offset_is_consulted() {
        // Order matters and is the origin's: the caller contract gates first,
        // so a caller error is reported as itself even when the index is also
        // broken. An out-of-range offset behind a short buffer must still
        // report BufferTooSmall, with the numbers involved.
        let region = [0u8; 4];
        let source = empty_source(
            &[FrameMeta {
                offset: 99,
                blocksize: 8,
            }],
            &region,
        );
        let mut left = [0i32; 7];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        let err = decode_one(&source, 0, &mut left, &mut right, &mut state)
            .expect_err("a short left buffer must not decode");
        match err {
            Error::BufferTooSmall { need, have } => {
                assert_eq!((need, have), (8, 7), "evidence fields");
            }
            other => panic!("expected BufferTooSmall, got {other:?}"),
        }
    }

    #[test]
    fn a_stereo_source_requires_a_full_right_buffer() {
        // The `channels == 2` arm of the buffer gate: mono legitimately takes
        // a zero-length `right` (witnessed end-to-end on real mono runs in
        // tests/frame_run_loop.rs), stereo does not.
        let region = [0u8; 4];
        let mut source = empty_source(
            &[FrameMeta {
                offset: 0,
                blocksize: 8,
            }],
            &region,
        );
        source.channels = 2;
        let mut left = [0i32; 8];
        let mut right = [0i32; 7];
        let mut state = Default::default();
        let err = decode_one(&source, 0, &mut left, &mut right, &mut state)
            .expect_err("a short right buffer must not decode a stereo source");
        match err {
            Error::BufferTooSmall { need, have } => {
                assert_eq!((need, have), (8, 7), "evidence fields");
            }
            other => panic!("expected BufferTooSmall, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn an_out_of_range_frame_index_is_a_caller_contract_violation() {
        // Pinned, not designed away: the walk's domain is `0..frames.len()`,
        // and `decode_one` indexes it directly exactly as the origin does.
        let region = [0u8; 4];
        let source = empty_source(
            &[FrameMeta {
                offset: 0,
                blocksize: 8,
            }],
            &region,
        );
        let mut left = [0i32; 8];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        let _ = decode_one(&source, 1, &mut left, &mut right, &mut state);
    }
}
