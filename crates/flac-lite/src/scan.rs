//! # Walking a FLAC stream with no index: how a frame ends, and how a stream ends
//!
//! [`crate::stream`] (split-ladder item 4a) answers two questions: where this
//! stream's frames begin, and what the stream declares about itself. This module
//! answers the two that follow: **how does a reader get from one frame to the
//! next with no table in front of it**, and **how does it know the stream is
//! over**. The format has two answers, and only two — so this module implements
//! those two and nothing else.
//!
//! ## Rule 1: a frame ends itself
//!
//! Nothing in a FLAC stream says where the *next* frame starts. What the format
//! gives you instead is that one frame's **own fields** describe every bit up to
//! its end: a self-describing header (§9.1.1 — block size, sample rate, channel
//! assignment, sample size, the coded frame number, CRC-8), then exactly one
//! subframe body per channel, each of which is walkable without knowing its
//! length in advance (constant-bits-per-sample: `blocksize × (bits per sample −
//! wasted bits)`; Rice: partition order, then parameters and quotients per
//! partition), then the footer — padding bits up to the byte boundary, then the
//! 16-bit frame CRC. [`crate::frame::decode_frame`] ends exactly that way, and
//! the run table's per-frame gap rule (*exit on a byte boundary → a 16-bit gap,
//! otherwise pad + 16*) is witnessed there.
//!
//! A reader that walks a frame's fields therefore **lands on the next frame's
//! first byte by construction**. No index, no byte count, no stride. `Frames`
//! below is that walk, and the reason its clock is a bit cursor rather than a
//! byte counter is visible in the stream table's own rows:
//!
//! ```text
//! frame_bytes            117 116 116 116 116 116 115 116 116 51
//! frame_footer_gaps_bits  23  17  16  16  21  21  16  16  19  22
//! ```
//!
//! (`s1_plain_8k_mono.flac`; `s3_wide_48001hz_b3000.flac` puts a 16-bit custom
//! block-size field in the mix — 3000, and 1801 for its final frame under the same
//! declared maximum — and its two frames are 2450 and 1477 bytes.)
//!
//! **No byte stride is the rule.** Adjacent frames differ by a byte, and the last
//! is a quarter of the length: a fixed stride from the first frame's length
//! either stops short of the region's end or runs past it, and this is what
//! `tests/scan_frame_walk_layout.rs`'s stride check actually asserts — not that
//! the start list is non-arithmetic, because for `s3_wide_48001hz_b3000` it is
//! (two frames: any two points are an arithmetic sequence). I wrote the weaker
//! claim first and the fixture corrected it.
//!
//! **The tail is zero-padding (0–7 bits) plus the 16-bit CRC**, recorded as a
//! 16..23-bit gap; generation asserts the format's bound rather than trusting it.
//! On 23 of these 35 frames that padding is non-zero, and the table counts what a
//! reader that treated the footer as exactly 16 bits would then be standing on:
//!
//! * at its **bit** cursor, 1–7 bits short — `footer_slip_positions_strict_headers
//!   0`: the strict reading finds no frame header at any of those 23 positions;
//! * at its **byte** cursor, if it rounded a non-byte-aligned end *down* —
//!   `footer_byte_truncation_strict_headers 0` over 30 positions.
//!
//! So a mis-counted footer is refused rather than followed, and loudly: a sync
//! error reported *inside* a frame the walk had already decoded correctly, which
//! is a confusing thing to hand a caller. What makes it worth reading the other
//! column is the row beside it: one of those 23 positions is sync-SHAPED
//! (`footer_slip_positions_sync_shape 1`) — 0xFF followed by a byte whose top
//! five bits are set, and fields that are not a frame. A player that trusted the
//! two-byte pattern would start a frame there; the field leg is what refuses it.
//!
//! The rule that *does* work is the one [`crate::frame`] documents: skip the
//! 16-bit CRC and re-anchor **up** to the next byte boundary. Arithmetic, not
//! luck — the footer ends on a byte boundary, so rounding up absorbs the padding.
//! It is safe for one reason only: the body's *bit* end was computed exactly, by
//! walking the fields. Nothing recovers it from byte counts, and a frame's byte
//! count is not a substitute for it either.
//!
//! So the walk counts the bits the format says a frame contains, and
//! `bit_position() / 8` after the footer is a *measurement* of that frame's
//! length rather than a rounding of a guess — which is why `ConsumedFrame::bytes`
//! equals the table's `frame_bytes` on all 35 frames, in both feature builds.
//! The bit cursor cannot slip, because it only ever advances by bits the format
//! says a frame contains.
//!
//! Those rows are independent ground truth, not this module's arithmetic
//! restated: the generator walks every frame's subfields with its own reference
//! parser (`scripts/stream_vectors.py`'s `walk_frame_bytes`, over
//! `frame_vectors.py`'s subframe walk) and derives the same numbers **twice** —
//! once from that bit walk, once from framing (distance to the next frame start,
//! file end for the last). It refuses to write the table unless the two agree for
//! every frame *and* the frames tile the region exactly
//! (`sum(frame_bytes) == frame_region_len`).
//!
//! ## Rule 2: the end of the bytes is the end of the stream
//!
//! "Is the stream over" is the question this whole effort exists to answer, and
//! the format states it once: §6, *"the end-of-stream condition, which is the
//! last byte of the last frame, and thus of the whole stream"*. That is the only
//! end **every** FLAC stream has. Everything else the format offers is a
//! *cross-check*, and some streams decline to offer any of those:
//!
//! * §8.2's `total_samples` is the interchannel sample count **or 0 for
//!   unknown**. `s5_unseekable_total_unknown.flac` is that stream: same audio,
//!   same 10 frames, same 1095-byte frame region as `s1`, and it still verifies
//!   (`flac -t`: `No errors found`). It exists to say in bytes rather than prose
//!   that a frame count may not be derived from a sample count. The generated
//!   rows spell out the split: `frame_exact_count_from_declarations 10` on the
//!   first four fixtures, `… none` on that one.
//! * `min_blocksize == max_blocksize` together with a known `total_samples` does
//!   give an exact frame count ([`crate::stream::StreamInfo::exact_frame_count`],
//!   4a) — fixed-blocksize streams only, and only where the sample count is
//!   known.
//! * §8.2's `minframesize`/`maxframesize` bound a frame's *byte length*; they do
//!   not say how many frames there are, and this crate does not pretend
//!   otherwise ([`crate::stream::StreamInfo::frame_sizes_known`]).
//! * §8.3's `SEEKTABLE` gives `(sample number, byte offset)` **points**, not a
//!   frame count; interpreting it is out of scope here — see "Not a seek
//!   strategy" below.
//! * §8.2's `MD5` would *verify* a sample count, but only once every sample has
//!   been read, so it cannot say where the stream ends while you are inside it.
//!   Where it is set, the reference decoder's output hashes to it bit for bit
//!   (`stream_vectors.txt`'s `pcm_md5 == streaminfo_md5`) — which is why it is
//!   usable as an acceptance check on a walk rather than as a stop condition.
//!
//! So: read to the end of the bytes in hand, and the end of those bytes is the
//! end of the stream. What the stream declares is used to **check that the walk
//! was the walk the stream said it was**, never to stop one short. Stopping at a
//! declared count would throw away the one signal that catches an off-by-one
//! framing error (the region ending somewhere other than where the audio ends)
//! and would give the unseekable stream a second code path for no benefit; with
//! the rule as stated both streams run the same lines and differ only in whether
//! a cross-check is available.
//!
//! A walk therefore ends three ways, and each ending is a fact:
//!
//! 1. `Ok(None)` at the end of the region, with the declared totals (where the
//!    stream states any) agreeing with what was walked. The unseekable fixture
//!    ends here; so does every packed blob and every seek range.
//! 2. `Err(DeclaredSampleCountMismatch)` or `Err(DeclaredFrameCountMismatch)` at
//!    the end of the region — those bytes ended somewhere the stream's own
//!    declarations say audio cannot end. A truncated file, a frame region taken
//!    with the wrong length, or a declaration that does not belong to these
//!    bytes. The frame-count arm is 4a's arithmetic used as a **cross-check
//!    rather than a stop rule**: a stream that declares one block size for every
//!    frame and a sample count has, in effect, also declared its frame count, so
//!    reaching the right number of samples in a different number of frames means
//!    some frame carried a block size no encoder of this stream writes.
//! 3. `Err(Decode(_))` short of the end — the bytes at the cursor are not the
//!    next frame: [`crate::Error::FrameSync`] for something that is not a header,
//!    [`crate::Error::InvalidField`] for a field no encoder writes,
//!    [`crate::Error::EndOfStream`] for a frame running past the last byte. The
//!    reason arrives unchanged from [`crate::Error`]; where the walk stopped
//!    arrives from [`Frames::cursor`].
//!
//! ## The scan, which is recovery — and the only part of it that searches
//!
//! When a walk lands in case 3, [`next_frame_start`] is how a reader finds its
//! footing again. It is the **only** place in this design that hunts for a
//! header, and it is deliberately not a way to enumerate a region. Its candidate
//! rule is `0xFF` + `(next & 0xF8) >= 0xF8`, then full RFC field validation
//! under the frame layer's width bound, then the header CRC-8. The field
//! validation is not decoration: measured across 979 real frames of two clean
//! encodes it is exactly the difference between 1223 candidates and 979 frames,
//! and over noise-like material the two-byte shape alone over-matches by ~9.4×
//! (FLAC.md's `find_frames` measurement). The reason is structural: a sync prefix
//! is only a *prefix* — it fixes the reserved bit and the blocking strategy and
//! says nothing about the 22 bits behind it, which is where the block size,
//! sample rate, channel and depth tables live.
//!
//! The committed fixtures make the same point from the other side. On all five,
//! every leg of the ladder — sync shape, field validation without CRC, full
//! strict validation, and strict validation with non-minimal number forms allowed
//! — counts exactly the real frames, because a 1095-byte frame region has no room
//! to hide a false start. The fixtures therefore prove the strict filter is exact
//! on real streams, and do **not** show false-start pressure; that is measured in
//! longer material, and the table header says so, so nobody reads those rows as
//! "false starts do not happen". The constructed shapes (bytes that are not a
//! frame start, a region that does not begin on one, trailing junk, a frame cut
//! at several boundaries) are witnessed over hand-cut regions in unit tests, per
//! the rule in `tests/README.md`.
//!
//! Per candidate the read is bounded: at most [`frame::MAX_HEADER_BYTES`] bytes —
//! the bound derived from the format in [`crate::frame`] for exactly this
//! purpose ("a scanner … reads a bounded window when testing a candidate sync
//! position"). A header whose real spelling would exceed what this parser accepts
//! is refused there (`MAX_ACCEPTED_HEADER_BITS`), and one that would need bytes
//! the region does not have is refused too, which is right: near the end of a
//! region there are no bytes to finish it. A candidate test therefore costs the
//! same in a 1 KB region and a 2 MB one.
//!
//! ## Frame numbers: the third leg of the strict filter, for a walk
//!
//! Every real encode measured here carries coded frame numbers **exactly
//! `0..N-1` in stream order** — the harness asserts it across 979 frames
//! (`scripts/frame_vectors.py`), and `stream_vectors.txt` carries the rows
//! (`frame_numbers 0 1 2 …`). That is both a measured property of encoders and
//! RFC 9639 §9.1.5's MUST: a frame number equals the number of frames preceding
//! the frame, so a conformant stream starts at 0. §9.1.5's other arm (a *sample*
//! number, under variable blocking) is refused inside
//! [`crate::frame::FrameHeader::parse`] before that field is read, because
//! variable blocking is outside this crate's profile.
//!
//! For a walk that knows where it stands, the sequence is a free and strict
//! filter, and [`Frames`] applies it **before any sample is written**: a frame
//! whose number is not the next one is refused without decoding it. That check is
//! also what turns a mis-taken byte offset into an error instead of a wrong
//! sample — precisely the case a table-less walk has to be able to report. A walk
//! over bytes that are only frames (a packed blob, a seek range) has no anchor, so
//! it enforces contiguity only ([`Frames::expect_number`]), and
//! [`Frames::anchored`] supplies an anchor when the caller has one — a manifest
//! entry, a seek point. [`Frames::resync`] re-anchors to whatever frame it lands
//! on, which is the honest version of recovery: the frame it skipped is lost.
//!
//! ## Not a seek strategy
//!
//! Reading front-to-back is not seeking, and this module does not pretend
//! otherwise. The product's random access stays what it is in Phase 2:
//! `format::Manifest`' frame-offset index, whose shape `tests/frame_run_loop.rs`
//! already drives. `scan` is two things, and both are real:
//!
//! * how an ordinary `.flac` file is decoded at all — the files `flac` writes
//!   contain no `SEEKTABLE`, and none is needed;
//! * how a *table-backed* walk re-anchors when a table's offset turns out to be
//!   wrong — the recovery path [`crate::frame`]'s contract already names ("the
//!   frame is dead — caller re-seeks"), for the case where there is no table to
//!   re-seek from.
//!
//! Item 6's packer side is where a table gets written. If that turns out to be a
//! real `SEEKTABLE`, reusing a stream's own seek points becomes a second anchor
//! source for the same walk, and interpreting §8.3 becomes its own change.
//! Nothing here forecloses that: [`Frames::anchored`] is the seam, and it takes a
//! sample-accurate byte offset the format itself wrote in one test below.
//!
//! ## Buffers, allocation, and the tail frame
//!
//! No allocation in this module: the walk is built from borrowed bytes and decodes
//! into the caller's buffers with one reused `[PredictorState; 2]`, exactly like
//! [`crate::driver`]. The required buffer length per channel is the maximum block
//! size **the stream itself declares**, where it declares one — and §8.2's caveat
//! that the actual maximum *may* be smaller than the listed one means the listed
//! number is an upper bound, never a too-small one. Where the stream declares no
//! bound (an encode whose framesizes field is 0/0 as well as its sample count:
//! `s5`) the walk cannot know a frame's block size before its header, so it
//! checks each frame against its own header instead. Which of the two the
//! requirement came from is part of the diagnosis, so it is in the error: a
//! refusal against a bound the **stream declares** is
//! [`Error::DeclaredMaxExceedsBuffers`], a refusal against the block size **this
//! frame's header** carries is [`Error::BufferTooSmall`]. Either way the frame is
//! refused with the numbers involved and its samples are not written.
//!
//! The tail frame follows the same contract as [`crate::driver`]'s tail test: a
//! 48-sample final frame writes its 48 and leaves the rest of the caller's buffer
//! alone — measured on `s1`, whose last frame is exactly that (`frame_blocksizes
//! … 128 48`, `frame_bytes … 51`).
//!
//! ## Feature gates
//!
//! **This module is itself behind a gate** — `scan`, off by default, so a build
//! that has no use for a frame walk does not carry one. Nothing in this crate
//! links it yet. What the gate moves is code size, never results: everything
//! below holds with it on or off, and both features in this section behave the
//! same either way. Its suite is `make flac-test-scan` (the only gate that runs
//! it); `make flac-portable-check` is the only gate that compiles the walk for a
//! non-thumbv4t bare-metal target, and it turns the feature on for exactly that
//! reason — an unbuilt module has no portability witness.
//!
//! `crc-check` changes what is *verified*, never what is *found*: the scan's
//! candidate rule checks the header CRC-8 in both builds (in a `crc-check` build
//! `FrameHeader::parse` has already done it; otherwise this module does), so a
//! scan result does not move with the feature. The frame-footer CRC-16 stays
//! `crc-check`'s alone, as everywhere in this crate — and it is verification, not
//! framing: the framing comes from the field walk above, which is why a build
//! without the feature still knows where the next frame begins.
//!
//! ### The profile question, stated exactly
//!
//! "The scan enforces no profile" is true in one sense and false in another, and
//! the two are worth separating because the difference decides where a profile
//! check belongs.
//!
//! The scan has **no profile leg of its own**: it adds no check, and it relaxes
//! none. Every frame-level refusal it reports arrives from
//! [`crate::frame::FrameHeader::parse`], which **is** the frame-start filter —
//! the sync code, the field tables, §9.1.5's blocking strategy, the sample-size
//! table, 3–8-channel configurations, reserved codes, and the minimal-number-form
//! rule. Those refusals are unconditional, not gated by `strict-profile`; they are
//! why a walk over `s4_24bit_2048.flac` reports `UnsupportedSampleSize` at both
//! entry points. That is the frame layer's claim, reached through the walk, not a
//! claim this module makes.
//!
//! What the walk genuinely does not do is state **which frames a packer is
//! allowed to write** — the encode profile: the block-size set and sample-rate set
//! that `strict-profile` guards on the encode side and that
//! [`crate::format::EncodeProfile`] / [`crate::format::Manifest`] will pin at item
//! 6's seam. A walk across someone else's `.flac` file makes no such claim, and
//! nothing here enforces one: `s3_wide_48001hz_b3000.flac`
//! exists to keep that honest, and it walks here (its 3000-sample blocks, and the
//! 1801-sample frame under that same declared maximum, decode
//! bit-exactly against the reference decoder, per `tests/frame_run_layout.rs`'s
//! rule). So the honest sentence is: the scan inherits the frame layer's
//! refusals, asserts nothing about a stream's shape beyond what its own bytes
//! say, and leaves profile enforcement to the manifest seam where a profile is
//! pinned — which is also where the profile-anchored measurements get re-pinned.
//!
//! ## What this module is not yet
//!
//! Not `Decoder`/`format::Manifest` rewiring — that is a later item, and the
//! manifest path (`tests/frame_run_loop.rs`) is untouched by this one. Not
//! `SEEKTABLE` interpretation. Not a `minframesize`/`maxframesize` prefilter (the
//! fields are bounds on one frame's length, and using them as one would be a
//! claim about encoders this crate has not measured). Not an end-of-input
//! *reader*: like every layer here it works on bytes in hand, and feeding it a
//! growing buffer is a caller-side concern until there is a source that needs one.

use crate::Error as DecodeError;
use crate::bits::BitReader;
use crate::frame::{self, FrameHeader, StreamDefaults};
use crate::stream::StreamInfo;
use crate::subframe::PredictorState;

/// Parse a frame header at `at`, or say that no frame header is there.
///
/// The single candidate rule, shared by [`next_frame_start`] and
/// [`Frames::resync`], in the measured order of cheapness: the two-byte sync
/// shape and the full field validation (both inside [`FrameHeader::parse`]), then
/// the header CRC-8 — which `parse` verifies when `crc-check` is on, and this
/// function supplies otherwise so that the scan finds the same bytes either way.
///
/// The window is **bounded**: `at` may be anywhere in a region, and the read
/// covers at most [`frame::MAX_HEADER_BYTES`] bytes, the bound
/// [`crate::frame`] derives from the format for exactly this purpose. A header
/// whose real spelling is wider than this parser can accept
/// ([`frame::MAX_ACCEPTED_HEADER_BITS`]) is refused, and so is one that would
/// need bytes the region does not have — correct, because near a region's end
/// there are no bytes to finish it.
fn header_at(region: &[u8], at: usize, defaults: &StreamDefaults) -> Option<FrameHeader> {
    let rest = region.get(at..)?;
    let bound = core::cmp::min(rest.len(), frame::MAX_HEADER_BYTES);
    let mut reader = BitReader::new(rest.get(..bound)?);
    let header = FrameHeader::parse(&mut reader, defaults).ok()?;
    #[cfg(not(feature = "crc-check"))]
    {
        // The measured filter has a CRC leg; `FrameHeader::parse` performs it
        // only in a `crc-check` build, so without this the scan would find
        // different frames depending on a build flag. A header is whole bytes by
        // construction (its width is always a multiple of 8 — [`crate::frame`]),
        // so covered-span and stored byte split exactly at `header_bits / 8`.
        let total = header.header_bits / 8;
        debug_assert!((1..=frame::MAX_HEADER_BYTES).contains(&total));
        let covered = rest.get(..total - 1)?;
        if crate::bits::crc8(covered) != *rest.get(total - 1)? {
            return None;
        }
    }
    Some(header)
}

/// The next byte at or after `from` that carries a frame header, or `None` if the
/// region holds none.
///
/// **Recovery, not a frame finder.** This is the right move once a walk has lost
/// its frame boundary — the caller knows the bytes at the cursor are not a frame
/// and needs a position from which a walk can continue (module docs). It is the
/// wrong move for enumerating a region: the candidate rule is a prefix test over a
/// field layout that carries no length, and measured, that is 1223 sync-shaped
/// candidates for 979 real frames, ~9.4× over-counting on noise-like material.
/// Where a stream has an index, use [`crate::driver`]; where it has frames and no
/// index, use [`Frames`], whose cursor comes from the frames themselves and needs
/// no scan at all.
///
/// `defaults` resolves the two header field values that mean "same as the stream"
/// (sample-rate code `0b0000`, sample-size code `0b000`) — the same parameters
/// [`crate::stream::StreamInfo::as_defaults`] produces, because a candidate header
/// cannot resolve itself without them.
pub fn next_frame_start(region: &[u8], from: usize, defaults: &StreamDefaults) -> Option<usize> {
    (from..region.len()).find(|&at| header_at(region, at, defaults).is_some())
}

/// What a stream declares that a walk can check itself against, in the shape a
/// walk consumes: `Some` where the stream says it, `None` where it does not.
///
/// The `None` arms are not a fallback path — they are what real streams declare.
/// [`StreamInfo`]'s sentinel fields (0 = "unknown or unset") are the format's, and
/// [`crate::stream::StreamInfo::total_samples_known`] and its neighbours exist to
/// make interpreting them explicit rather than scattered; a walk over bytes with
/// no streaminfo in front of them (a packed blob of frames) gets
/// [`StreamBounds::NOTHING`] and reads to the end of what it has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamBounds {
    /// Interchannel samples the stream declares (§8.2), or `None` where it
    /// declares none. Checked when the walk reaches the end of the region; never
    /// used to stop a walk short of it.
    pub samples: Option<u64>,
    /// A frame count derivable from the stream's own declarations
    /// ([`crate::stream::StreamInfo::exact_frame_count`]: a fixed block size plus
    /// a known sample count), or `None` where the stream does not state enough to
    /// derive one — which includes every variable-blocksize stream and
    /// `s5_unseekable_total_unknown.flac`.
    pub frames: Option<usize>,
    /// The largest block size the stream declares (§8.2), or `None` where it
    /// declares none. **A claim, and an upper bound — not per-frame truth:**
    /// §8.2's own caveat is that the actual maximum may be *smaller* than the
    /// listed one, and a walk that takes a bound is asking to size its buffers
    /// once for the whole stream, so it refuses a frame against the bound before
    /// it reads that frame's header ([`Error::DeclaredMaxExceedsBuffers`]). A
    /// caller whose buffers come from something else — a manifest's fixed block
    /// size, or a range whose frames it knows — passes `None` and lets each
    /// frame's own header size the requirement ([`Error::BufferTooSmall`]).
    pub max_blocksize: Option<usize>,
}

impl StreamBounds {
    /// The stream says nothing: the end of the bytes is the only end, and each
    /// frame's buffer requirement comes from that frame's own header.
    pub const NOTHING: Self = Self {
        samples: None,
        frames: None,
        max_blocksize: None,
    };

    /// Read the declarations out of a real stream's streaminfo block, turning its
    /// sentinel fields into the `None` arms those sentinels describe.
    pub fn of_stream_info(info: &StreamInfo) -> Self {
        Self {
            samples: info.total_samples_declared(),
            frames: info.exact_frame_count(),
            max_blocksize: usize::try_from(u32::from(info.max_blocksize)).ok(),
        }
    }
}

/// Why a walk stopped, where the reason is the walk's own to name.
///
/// Payload-carrying on purpose, like [`crate::driver::Error`]: what a witness test
/// needs here *is* the numbers involved (declared against decoded, expected
/// against found), and the crate's flat [`crate::Error`] cannot carry them.
/// Anything `flac-lite` rejects is passed through unchanged as [`Error::Decode`].
#[derive(Debug)]
pub enum Error {
    /// Caller buffers shorter than **this frame's own header** asked for. Reached
    /// where the stream declared no block-size bound ([`StreamBounds::max_blocksize`]
    /// is `None`: an encode that states neither a sample count nor frame sizes —
    /// `s5` — and a caller that deliberately hands the walk no bound), and where
    /// the second subframe of a stereo frame is the short one: whether a frame
    /// carries two subframes is that frame's channel code, per-frame truth rather
    /// than a declared field like [`crate::driver::FrameSource::channels`]. Either
    /// way the frame is refused and its samples are not written. Where the stream
    /// *does* declare a bound the refusal is
    /// [`Error::DeclaredMaxExceedsBuffers`], which says which claim answered.
    BufferTooSmall {
        /// Samples per subframe this frame's header asked for.
        need: usize,
        /// Length actually provided for the failing channel.
        have: usize,
    },
    /// The caller's buffers are shorter than the maximum block size **the stream
    /// declares** (§8.2, [`StreamBounds::max_blocksize`]) — which is *not* a claim
    /// that the frame at the cursor needs `declared` samples, because at this
    /// point the walk has not read it.
    ///
    /// Asking before reading is deliberate: where a bound is declared the
    /// requirement is knowable in advance, and a caller error must not be masked
    /// by a stream one ([`crate::driver`]'s order, and the reason the buffers are
    /// gated at all).
    ///
    /// The cost of asking early is that the number here belongs to the stream, not
    /// to the frame: §8.2's own caveat is that a declared maximum may exceed every
    /// frame in the region, and a walk over part of a stream may be handed a bound
    /// larger than anything in it. The committed table has the case at full-stream
    /// scale: `s3` declares minimum and maximum block size **3000/3000** and its
    /// last frame carries **1801**, so a caller whose buffers are 1801 samples is
    /// refused on that frame by the *declaration*, not by the frame — a frame that
    /// would have decoded. Which is precisely why the refusal names the declaration.
    /// So the two possible fixes are told apart by the
    /// variant rather than guessed at: make the buffers fit the stream, or hand the
    /// walk no bound ([`StreamBounds::NOTHING`], or `max_blocksize: None`) and let
    /// each frame's own header size the requirement — that path answers as
    /// [`Error::BufferTooSmall`]. Same refusal point, same "nothing read, nothing
    /// written", either way.
    DeclaredMaxExceedsBuffers {
        /// Maximum block size the stream declares, in samples per subframe.
        declared: usize,
        /// Length actually provided for the failing channel.
        have: usize,
    },
    /// The frame at the cursor carries a number other than the next in the
    /// sequence the walk is following, and was refused **before any sample was
    /// written**. Measured, every real encode numbers its frames exactly `0..N-1`
    /// in stream order; for an anchored walk a mismatch is also the sign that
    /// these bytes are not where the stream's own counting says the next frame
    /// should be.
    FrameOutOfSequence {
        /// Byte offset of the rejected frame's header.
        offset: usize,
        /// Number the walk was waiting for.
        expected: u64,
        /// Number the header carries.
        found: u64,
    },
    /// The region ended on a byte boundary and the samples walked do not add up to
    /// what the stream declared: a truncated file, a frame region taken with the
    /// wrong length, or declarations that do not belong to these bytes.
    DeclaredSampleCountMismatch {
        /// `total_samples` as the stream states it (interchannel samples).
        declared: u64,
        /// Samples per channel the walk decoded.
        decoded: usize,
        /// Frames the walk decoded before the region ended.
        frames: usize,
        /// Length of the region the walk read.
        region_len: usize,
    },
    /// The region ended on a byte boundary and the frame count does not match what
    /// the stream's own declarations derive. A stream that declares one block size
    /// for every frame and a total sample count has in effect also declared its
    /// frame count, so reaching the right sample count in a different number of
    /// frames means some frame carried a block size no encoder of this stream
    /// writes. A cross-check, not a stop rule (module docs).
    DeclaredFrameCountMismatch {
        /// Frame count the declarations derive.
        declared: usize,
        /// Frames the walk decoded.
        decoded: usize,
        /// Length of the region the walk read.
        region_len: usize,
    },
    /// `flac-lite` rejected the frame at the cursor, or that frame runs past the
    /// last byte. Carried through unchanged: [`crate::Error::FrameSync`] means the
    /// bytes there are not a frame header, [`crate::Error::InvalidField`] a field
    /// no encoder writes, [`crate::Error::EndOfStream`] a frame cut short by the
    /// region's end. Where the walk stopped is [`Frames::cursor`].
    Decode(DecodeError),
}

/// What a walk has produced so far. The same shape as [`crate::driver::Stats`],
/// reached incrementally because [`Frames`] is a pull walk, not a loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Frames decoded.
    pub frames: usize,
    /// Samples decoded per channel — the sum of the frames' own blocksizes.
    pub samples: usize,
}

/// What one call to [`Frames::next_frame`] consumed, and where.
///
/// `bytes` is a value the stream table states independently of this module
/// (`frame_bytes 117 116 116 116 116 116 115 116 116 51` for
/// `s1_plain_8k_mono.flac`), and `samples` is that frame's own block size
/// (`frame_blocksizes … 128 48`) — the short tail is not a rounding detail but
/// the reason a frame count is only derivable for streams that declare something
/// else as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumedFrame {
    /// Byte offset of this frame's header, within the region the walk reads.
    pub offset: usize,
    /// Bytes this frame occupies — header, bodies, padding, footer. Counted by the
    /// walk's own cursor; equal to the table's row for the same frame.
    pub bytes: usize,
    /// Samples this frame decoded, per channel.
    pub samples: usize,
    /// The coded frame number this frame carries.
    pub number: u64,
    /// Its parsed header: per-frame truth about block size, sample rate, channel
    /// mode and depth. Kept because a walk is exactly where that truth is
    /// available for free, and because a caller that wants to re-anchor needs it.
    pub header: FrameHeader,
}

/// A frame walk: frames in stream order, boundaries derived from the frames
/// themselves, ending where the bytes end.
///
/// Constructed, never hand-assembled: [`Frames::stream`] for the ordinary case (a
/// `.flac` file's bytes plus the facts [`crate::stream::parse`] read from them),
/// [`Frames::region`] for bytes that are only frames. Both run the **same code**:
/// a stream that declares nothing differs only in which cross-checks exist (module
/// docs, Rule 2).
#[derive(Clone, Debug)]
pub struct Frames<'a> {
    region: &'a [u8],
    at: usize,
    defaults: StreamDefaults,
    bounds: StreamBounds,
    /// The number the next frame must carry, or `None` where the walk has no
    /// anchor to count from — see [`Frames::anchored`] and
    /// [`Frames::expect_number`].
    expect_number: Option<u64>,
    stats: Stats,
}

impl<'a> Frames<'a> {
    /// Walk the frames of an ordinary `.flac` file: its whole bytes, and the facts
    /// [`crate::stream::parse`] read from them.
    ///
    /// The frames begin at `frame_data_offset` (§8.1's last-flagged block ends
    /// there; `frame_data_offset 86` on every fixture in the stream table), the
    /// stream defaults come from that stream's own streaminfo block, the bounds
    /// come from the same block, and the walk is anchored at frame 0: a stream's
    /// first frame is frame 0, and every real encode measured here counts
    /// `0..N-1` in order.
    ///
    /// A `facts`/`stream` mismatch is the one caller error this constructor can
    /// make invisible: it cannot happen for a caller holding both (4a returns the
    /// offset it computed from the bytes it was given), and a debug build asserts
    /// it. The shape the mistake takes when it does happen is worth knowing,
    /// because it is subtle in exactly the way this module's subject matter is:
    /// the slice gets sliced again, the walk starts `frame_data_offset` bytes
    /// into the bytes the caller meant it to have, the first thing it touches is
    /// not a header, and it reports that as [`crate::Error::FrameSync`] with a
    /// cursor of 0 — a position that is *true* relative to the region it chose,
    /// which is the point: positions in a walk mean positions in a region.
    /// [`scan_frame_walk_layout.rs`](tests/scan_frame_walk_layout.rs) witnesses
    /// that mistake being made on purpose, and [`Frames::region`] is the entry
    /// point for a slice that already begins at a frame.
    pub fn stream(stream: &'a [u8], facts: &crate::stream::StreamFacts) -> Self {
        debug_assert!(facts.frame_data_offset <= stream.len());
        let region = stream.get(facts.frame_data_offset..).unwrap_or(&[]);
        Self {
            region,
            at: 0,
            defaults: facts.info.as_defaults(),
            bounds: StreamBounds::of_stream_info(&facts.info),
            expect_number: Some(0),
            stats: Stats::default(),
        }
    }

    /// Walk a region that contains frames and nothing else: a packed blob, a seek
    /// range, a hand-cut slice of a file.
    ///
    /// `defaults` and `bounds` are the caller's to supply — for a packed blob,
    /// `format::Manifest` (item 6); for a range taken from a real stream, the
    /// frame a `SEEKTABLE` entry names supplies both. An entry's byte offset is
    /// relative to the first byte after the metadata, so the region to hand this
    /// constructor starts there; its
    /// [`crate::stream::SeekPoint::frame_number`] is the coded frame number RFC
    /// 9639 §9.1.5 says the frame header carries, exactly what
    /// [`Frames::anchored`] wants. §9.1.5 requires that number to equal the
    /// number of frames preceding the frame, so a conformant whole stream starts
    /// at frame 0; a nonzero first coded number is a non-seekable/mutilated
    /// stream, not a separate base to add. With no anchor the walk still enforces
    /// contiguity (each frame's number follows the last one); it cannot say that
    /// the frame it found *should* have been number 7.
    pub fn region(region: &'a [u8], defaults: &StreamDefaults, bounds: StreamBounds) -> Self {
        Self {
            region,
            at: 0,
            defaults: *defaults,
            bounds,
            expect_number: None,
            stats: Stats::default(),
        }
    }

    /// Anchor the walk's frame numbering at `first_frame_number`, the coded
    /// number the region's first frame carries. A conformant whole stream starts
    /// at 0, and a seek point's
    /// [`crate::stream::SeekPoint::frame_number`] is the same number; the
    /// builder-style form also lets recovery use the number a mutilated region's
    /// first header actually states.
    pub fn anchored(mut self, first_frame_number: u64) -> Self {
        self.expect_number = Some(first_frame_number);
        self
    }

    /// Byte offset, within the region, where the next call will read. Equal to the
    /// region's length exactly at a clean end (module docs, Rule 2 case 1), and
    /// otherwise at the start of the frame that was rejected — which is what makes
    /// [`Frames::resync`] well-defined.
    pub fn cursor(&self) -> usize {
        self.at
    }

    /// Whether the walk has reached the end of the region.
    pub fn at_end(&self) -> bool {
        self.at >= self.region.len()
    }

    /// The region's length: the byte position a clean walk ends at. Measured on
    /// the fixtures (`frame_region_len 1095`), the last frame's
    /// `offset + bytes` equals it, and the frames tile the region exactly.
    pub fn region_len(&self) -> usize {
        self.region.len()
    }

    /// Totals so far.
    pub fn decoded(&self) -> Stats {
        self.stats
    }

    /// The number the next frame must carry, or `None` where the walk has no
    /// anchor.
    pub fn expect_number(&self) -> Option<u64> {
        self.expect_number
    }

    /// The refusal for a buffer that is too short, naming **which claim** the
    /// requirement came from.
    ///
    /// Same check, same position, same "nothing read and nothing written" either
    /// way — the variant is the diagnosis. A declared bound is a claim about the
    /// stream (§8.2's caveat: it may exceed every frame in the region); no bound
    /// means the requirement can only come from this frame's own header. A caller
    /// whose buffers are sized from something else — a manifest's fixed block size
    /// being the product-shaped case — has to be able to tell the two apart,
    /// because only one of them is fixed by making the buffers bigger.
    fn capacity_refusal(&self, needed: usize, have: usize) -> Error {
        match self.bounds.max_blocksize {
            Some(declared) => Error::DeclaredMaxExceedsBuffers { declared, have },
            None => Error::BufferTooSmall { need: needed, have },
        }
    }

    /// The next frame, decoded into the caller's buffers, or `None` at the end of
    /// the region.
    ///
    /// One frame per call, in stream order, with no lookahead and no index: the
    /// next position is wherever this frame's own fields ended — which is why the
    /// caller's buffers are used and re-used and no frame length is assumed. **No
    /// search is part of the steady state**: nothing here looks ahead for a sync
    /// code or tries to land on a frame start, and the walk's only step is the
    /// field walk of Rule 1. [`next_frame_start`] is reached from exactly one
    /// place, [`Frames::resync`], and only after a rejection has already left the
    /// walk somewhere that is not a frame. A
    /// short final frame writes into the first `samples` slots only and leaves the
    /// rest of the caller's buffers alone (measured: 48 samples into a buffer
    /// 128 long — [`crate::driver`]'s tail-frame rule, same contract).
    ///
    /// `state` is reused across frames exactly as [`crate::driver`] requires:
    /// per-frame decode state, not per-stream, and not reallocated either.
    /// Decode the next frame through the default Rice kernel.
    ///
    /// Exactly [`Frames::next_frame_with`] at `TableCountKernel` — one body, so
    /// the walk cannot drift into a second codeword loop. The `K` seam exists
    /// because the *product* path has to name its kernel by type: the overlay's
    /// `ArmRiceKernel` lives in another crate, and a walk that hardcodes the
    /// default can only ever be a default-kernel walk (6b-1).
    #[inline]
    pub fn next_frame(
        &mut self,
        left: &mut [i32],
        right: &mut [i32],
        state: &mut [PredictorState; 2],
    ) -> core::result::Result<Option<ConsumedFrame>, Error> {
        self.next_frame_with::<crate::residual::TableCountKernel>(left, right, state)
    }

    /// Decode the next frame through the Rice kernel `K`.
    ///
    /// The body is the walk, unchanged; `K` reaches it through
    /// [`crate::frame::decode_frame_with`], the same generic entry the
    /// measurement instrument's arms use. An image holding one instantiation
    /// pays for one; a build holding two (a gate, or a differential arm) pays the
    /// second-instantiator cost the E-series recorded, which is why the codegen
    /// A/B for this seam is part of 6b-1's witness rather than an assumption.
    pub fn next_frame_with<K: crate::residual::RiceKernel>(
        &mut self,
        left: &mut [i32],
        right: &mut [i32],
        state: &mut [PredictorState; 2],
    ) -> core::result::Result<Option<ConsumedFrame>, Error> {
        if self.at >= self.region.len() {
            return self.finish();
        }
        let start = self.at;
        // The caller contract first, in `crate::driver`'s order: where the stream
        // declares a block-size bound, a short buffer is the error to report —
        // before a bit of this frame is read, and even if the bytes at the cursor
        // are also wrong, so a caller error is never masked by a stream one. The
        // `right` buffer is gated per frame below, because whether a frame needs
        // two subframes comes from that frame's own channel code (per-frame truth,
        // not a declared field like `driver::FrameSource::channels`).
        if let Some(bound) = self.bounds.max_blocksize {
            if left.len() < bound {
                return Err(self.capacity_refusal(bound, left.len()));
            }
        }
        let rest = self
            .region
            .get(start..)
            .ok_or(Error::Decode(DecodeError::EndOfStream))?;
        let mut reader = BitReader::new(rest);
        let header = FrameHeader::parse(&mut reader, &self.defaults).map_err(Error::Decode)?;

        // The number first: it is the cheapest strict filter this walk has, and
        // refusing here means no sample is written (module docs).
        if let Some(expected) = self.expect_number {
            if header.number != expected {
                return Err(Error::FrameOutOfSequence {
                    offset: start,
                    expected,
                    found: header.number,
                });
            }
        }

        // Buffers again, now at the frame's own number: where the stream declared
        // a bound this is the same value and already passed above, and where it
        // declared none this is the earliest point at which the number exists
        // (s5 declares neither a total nor a frame size, so neither bound is
        // available to a walk). `frame::decode_frame`'s short-buffer gate stays
        // the backstop either way.
        let need = self.bounds.max_blocksize.unwrap_or(header.blocksize);
        if left.len() < need {
            return Err(self.capacity_refusal(need, left.len()));
        }
        if usize::from(header.channels.subframe_count()) == 2 && right.len() < need {
            return Err(self.capacity_refusal(need, right.len()));
        }

        let samples = frame::decode_frame_with::<K>(&mut reader, &header, left, right, state)
            .map_err(Error::Decode)?;

        // Framing: the frame's own length, read off the cursor the walk already
        // has. A frame ends on a byte boundary by construction (its footer pads to
        // one before the CRC-16), so `bit_position()` *is* the count of whole bytes
        // this frame occupies — the arithmetic the run table's gap rule witnesses
        // per frame in [`crate::frame`].
        let bits = reader.bit_position();
        debug_assert_eq!(bits % 8, 0, "a frame must end on a byte boundary");
        let bytes = bits / 8;
        debug_assert!(start + bytes <= self.region.len(), "reads stayed in range");

        self.at = start + bytes;
        self.expect_number = header.number.checked_add(1);
        self.stats.frames += 1;
        self.stats.samples += samples;

        Ok(Some(ConsumedFrame {
            offset: start,
            bytes,
            samples,
            number: header.number,
            header,
        }))
    }

    /// Move the walk to the next position at or after the current one that carries
    /// a frame header, re-anchoring its frame numbering on that frame, and return
    /// that position — or `None`, leaving the walk untouched, when the rest of the
    /// region carries no frame header.
    ///
    /// [`next_frame_start`] applied to the walk's own state, searching from **one
    /// byte past** the cursor rather than from it. That is what guarantees
    /// progress: a frame rejected *after* its header parsed leaves the cursor on
    /// that header, so searching from the cursor would return the same position
    /// forever. The cost is the honest one, and it is why resync is recovery rather
    /// than a framing rule: after a frame whose body was corrupt, the header found
    /// can sit **inside** the frame that just failed, and the walk resumes from
    /// there — samples in that frame's middle are gone, which is what "the frame is
    /// dead" ([`crate::frame`]) already means for a player. A caller that knows
    /// where a range begins uses [`Frames::region`] with [`Frames::anchored`]
    /// instead of searching.
    ///
    /// Cost, and where it is allowed to run: the search is **O(region)** in the
    /// worst case — every byte position from one past the cursor to the end, each
    /// candidate read bounded by [`frame::MAX_HEADER_BYTES`] (module docs, "The
    /// scan, which is recovery") — and a candidate test is the expensive filter,
    /// measured at 1223 sync-shaped candidates for 979 real frames. That is what
    /// makes this recovery rather than a loop body: nothing in the steady-state
    /// walk searches ([`Frames::next_frame`]), and a real-time caller keeps a
    /// rescan **out of the pull loop** — item 6's facade re-anchors at the seam,
    /// charged to the seek whose offset was wrong rather than to a frame's budget.
    /// `resync` is deliberately caller-invoked for that reason; a walk that called
    /// it after every rejection would be paying a rescan inside the audio path.
    ///
    /// The walk is left positioned **on** the frame it found, which is what
    /// re-anchoring means here: that frame's own number becomes the next one
    /// expected, and [`Frames::next_frame`] checks it against the frame it is about to
    /// decode. Anchoring on the number *after* it would reject the very frame
    /// resync landed on — witnessed by
    /// `scan_frame_walk_layout.rs::a_walk_that_lost_its_frame_boundary_recovers_onto_the_next_real_frame`,
    /// which is where the first version of this function was caught. From there,
    /// sequencing proceeds normally: [`Frames::next_frame`] advances one number per frame
    /// accepted, and turns itself off if a number ever reaches the top of the
    /// range ([`u64`]), rather than wrapping into a false refusal.
    pub fn resync(&mut self) -> Option<usize> {
        let from = self.at.saturating_add(1);
        let at = next_frame_start(self.region, from, &self.defaults)?;
        // Re-anchor to the only number available: the one the frame at the new
        // position carries. It has not been consumed, so it IS the next one
        // expected.
        if let Some(header) = header_at(self.region, at, &self.defaults) {
            self.expect_number = Some(header.number);
        }
        self.at = at;
        Some(at)
    }

    /// The end-of-region rule, with the stream's declarations as a check on it
    /// (module docs, Rule 2, cases 1 and 2).
    fn finish(&self) -> core::result::Result<Option<ConsumedFrame>, Error> {
        // `samples` is a count of decoded samples, so it is bounded by the region
        // it was read from: widening it to u64 loses nothing on any target.
        let decoded = self.stats.samples as u64;
        if let Some(declared) = self.bounds.samples {
            if decoded != declared {
                return Err(Error::DeclaredSampleCountMismatch {
                    declared,
                    decoded: self.stats.samples,
                    frames: self.stats.frames,
                    region_len: self.region.len(),
                });
            }
        }
        if let Some(declared) = self.bounds.frames {
            if self.stats.frames != declared {
                return Err(Error::DeclaredFrameCountMismatch {
                    declared,
                    decoded: self.stats.frames,
                    region_len: self.region.len(),
                });
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream;

    /// Hand-written regions and hand-built headers here witness the walk's **state
    /// machine and caller contract** and claim nothing about encoder bytes, exactly
    /// as `crate::driver`'s unit tests do (the hand-packed-vector rule, FLAC.md).
    /// Everything that claims something about real frames is witnessed in
    /// `tests/scan_frame_walk_layout.rs` against the committed stream table.
    const RESOLVER: StreamDefaults = StreamDefaults {
        sample_rate_hz: 32_000,
        bits_per_sample: 16,
    };

    #[test]
    fn an_empty_region_is_a_complete_walk_when_nothing_is_declared() {
        // The degenerate case of Rule 2: zero bytes is a whole walk, and with no
        // declarations there is nothing for it to contradict.
        let mut walk = Frames::region(&[], &RESOLVER, StreamBounds::NOTHING);
        let mut state = Default::default();
        assert!(
            walk.next_frame(&mut [], &mut [], &mut state)
                .unwrap()
                .is_none(),
            "an empty region must end cleanly"
        );
        assert_eq!(
            walk.decoded(),
            Stats {
                frames: 0,
                samples: 0
            }
        );
        assert!(walk.at_end());
        assert_eq!(walk.cursor(), 0);
    }

    #[test]
    fn an_empty_region_contradicts_a_declared_sample_count() {
        // Truncated-to-nothing is the most truncated a walk can be. A stream that
        // says it holds 1200 samples per channel must not be reported complete by
        // an empty region, and the test wants the numbers, not just the fact.
        let bounds = StreamBounds {
            samples: Some(1200),
            ..StreamBounds::NOTHING
        };
        let mut walk = Frames::region(&[], &RESOLVER, bounds);
        let mut state = Default::default();
        let err = match walk.next_frame(&mut [], &mut [], &mut state) {
            Err(e) => e,
            Ok(_) => panic!("an empty region must not satisfy a declaration"),
        };
        match err {
            Error::DeclaredSampleCountMismatch {
                declared,
                decoded,
                frames,
                region_len,
            } => assert_eq!((declared, decoded, frames, region_len), (1200, 0, 0, 0)),
            other => panic!("expected DeclaredSampleCountMismatch, got {other:?}"),
        }
    }

    #[test]
    fn bytes_that_are_not_a_header_reach_the_decoder_and_leave_the_walk_in_place() {
        // Rule 2 case 3 in its cheapest form: the region holds bytes no encoder
        // writes. The walk neither guesses nor moves, so `cursor()` names the place
        // to ask `resync` about, and resync finds nothing to find.
        let region = [0x00u8; 24];
        let mut walk = Frames::region(&region, &RESOLVER, StreamBounds::NOTHING);
        let mut left = [0i32; 64];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        match walk.next_frame(&mut left, &mut right, &mut state) {
            Err(Error::Decode(DecodeError::FrameSync)) => {}
            other => panic!("expected FrameSync, got {other:?}"),
        }
        assert_eq!(
            walk.cursor(),
            0,
            "a rejected frame must not advance the walk"
        );
        assert_eq!(walk.decoded().frames, 0);
        assert_eq!(
            walk.resync(),
            None,
            "and nothing to resync to in pure garbage"
        );
        assert_eq!(walk.cursor(), 0, "a failed resync leaves the walk alone");
    }

    #[test]
    fn a_short_buffer_is_refused_at_the_declared_bound_before_the_frame_is_read() {
        // Where a block-size bound is declared, the caller contract is knowable
        // before the frame — `crate::driver`'s rule — and the region's contents are
        // irrelevant, because the gate runs first. The variant is part of the point:
        // the number in this refusal belongs to the *stream's declaration*, not to a
        // frame the walk never read, and it says so.
        let bounds = StreamBounds {
            max_blocksize: Some(2048),
            ..StreamBounds::NOTHING
        };
        let region = [0u8; 8];
        let mut walk = Frames::region(&region, &RESOLVER, bounds);
        let mut left = [0i32; 2047];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        match walk.next_frame(&mut left, &mut right, &mut state) {
            Err(Error::DeclaredMaxExceedsBuffers { declared, have }) => {
                assert_eq!((declared, have), (2048, 2047))
            }
            other => panic!("expected DeclaredMaxExceedsBuffers, got {other:?}"),
        }
    }

    #[test]
    fn a_candidate_near_the_end_of_a_region_is_not_a_frame_start() {
        // The bounded-window claim, witnessed: a sync pair with no bytes left to
        // finish it is not a frame start, and the scan says so without reading past
        // the bound. This is what makes scanning cost the same per position at the
        // end of a region as anywhere else in one.
        let mut region = [0u8; 64];
        let last = region.len() - 1;
        region[last - 1] = 0xFF;
        region[last] = 0xF8;
        assert_eq!(
            header_at(&region, last - 1, &RESOLVER),
            None,
            "a candidate the region cannot complete is not a frame start"
        );
        assert_eq!(next_frame_start(&region, 0, &RESOLVER), None);
        assert_eq!(next_frame_start(&region, last, &RESOLVER), None);
    }

    #[test]
    fn an_anchored_walk_refuses_a_frame_out_of_sequence_without_decoding_it() {
        // The ordering rule and its payoff, over two hand-built headers whose
        // bodies are deliberately not subframes: anchored, the number is what the
        // walk reports — and it reports it before the body could have written
        // anything. Unanchored, the same bytes produce a body error instead, which
        // witnesses that the number check is a separate leg and that an unanchored
        // walk does not invent an anchor for it.
        let mut region = [0u8; 12];
        region[..6].copy_from_slice(&hand_header(4));
        region[6..].copy_from_slice(&hand_header(9));

        let sentinel = 0x5A5A_5A5A;
        let mut left = [sentinel; 192];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();

        let mut walk = Frames::region(&region, &RESOLVER, StreamBounds::NOTHING).anchored(9);
        match walk.next_frame(&mut left, &mut right, &mut state) {
            Err(Error::FrameOutOfSequence {
                offset,
                expected,
                found,
            }) => assert_eq!((offset, expected, found), (0, 9, 4)),
            other => panic!("expected FrameOutOfSequence, got {other:?}"),
        }
        assert_eq!(
            left, [sentinel; 192],
            "a frame refused on its number must not have been decoded"
        );
        assert_eq!(walk.cursor(), 0, "and the walk stays at that frame");

        let mut walk = Frames::region(&region, &RESOLVER, StreamBounds::NOTHING);
        match walk.next_frame(&mut left, &mut right, &mut state) {
            // The number (4) is accepted, so what is left to complain about is the
            // body — the separation the ordering rule is about.
            Err(Error::Decode(_)) => {}
            other => panic!("expected a decode error, got {other:?}"),
        }
        assert_eq!(
            walk.expect_number(),
            None,
            "an unanchored walk claims no number"
        );
    }

    #[test]
    fn a_resync_lands_on_a_frame_and_expects_that_frames_number_next() {
        // Where the first version of [`Frames::resync`] was wrong, in the one
        // place a unit test can see it: the walk's position and the walk's frame
        // numbering are two pieces of state, and re-anchoring one without the
        // other makes the walk refuse the frame it just went looking for. Real
        // bytes are witnessed in `tests/scan_frame_walk_layout.rs`
        // (`a_walk_that_lost_its_frame_boundary_recovers_onto_the_next_real_frame`);
        // this is the same ordering statement told with hand-built headers, which
        // is the only way to see it without also asserting things about bodies.
        let mut region = [0u8; 12];
        region[6..].copy_from_slice(&hand_header(9));

        let mut left = [0i32; 192];
        let mut right: [i32; 0] = [];
        let mut state = Default::default();
        let mut walk = Frames::region(&region, &RESOLVER, StreamBounds::NOTHING);
        match walk.next_frame(&mut left, &mut right, &mut state) {
            Err(Error::Decode(DecodeError::FrameSync)) => {}
            other => panic!("expected FrameSync at a region of zeros, got {other:?}"),
        }
        assert_eq!(walk.cursor(), 0);
        assert_eq!(
            walk.expect_number(),
            None,
            "an unanchored walk that has accepted nothing claims no number"
        );

        assert_eq!(
            walk.resync(),
            Some(6),
            "the search must start past the cursor, not at it"
        );
        assert_eq!(walk.cursor(), 6, "…and stop ON the header it found");
        assert_eq!(
            walk.expect_number(),
            Some(9),
            "the frame it landed on has not been consumed, so it is the next              one expected — anchoring past it would refuse this frame"
        );

        // The number leg passes, which is the point. The body of a hand-built
        // header is not a subframe, so the walk now reports a *body* error, and
        // stays put: a caller that wants to try again from the next header calls
        // resync from here and gets the honest answer that nothing after byte 6
        // is one.
        match walk.next_frame(&mut left, &mut right, &mut state) {
            Err(Error::Decode(_)) => {}
            other => panic!("the number leg must have passed; got {other:?}"),
        }
        assert_eq!(
            walk.cursor(),
            6,
            "a frame refused after its header stays              where it was found"
        );
        assert_eq!(walk.decoded().frames, 0, "and produced nothing");
        assert_eq!(
            walk.resync(),
            None,
            "the region holds one header, and it has been found once already"
        );
    }

    #[test]
    fn stream_bounds_turn_sentinels_into_none_arms() {
        // The two shapes of "the stream does not say", both real fixture shapes:
        // total_samples 0 with block sizes present, and framesizes 0/0. Neither may
        // become a `Some(0)`: a 0 sample count is not a claim of zero audio, and a
        // 0 maximum block size is not a claim of zero-length buffers.
        let payload = streaminfo_payload(0, 128, 128);
        let info = stream::StreamInfo::parse(&payload).expect("s5-shaped streaminfo");
        let bounds = StreamBounds::of_stream_info(&info);
        assert_eq!(bounds.samples, None, "a 0 total_samples is not a claim");
        assert_eq!(bounds.frames, None, "so no frame count is derivable");
        assert_eq!(bounds.max_blocksize, Some(128));

        // And one that does state a count: 1200 samples at a fixed 128 block —
        // s1's declarations, whose derived frame count (10) is the table's row.
        let payload = streaminfo_payload(1200, 128, 128);
        let info = stream::StreamInfo::parse(&payload).expect("s1-shaped streaminfo");
        let bounds = StreamBounds::of_stream_info(&info);
        assert_eq!(bounds.samples, Some(1200));
        assert_eq!(bounds.frames, Some(10));
        assert_eq!(bounds.max_blocksize, Some(128));
    }

    /// Table 3's field order, written bit by bit in the order
    /// [`stream::StreamInfo::parse`] reads it, with 8 kHz mono 16-bit fixed. Built
    /// to state the sentinel question without claiming what an encoder's bytes are.
    fn streaminfo_payload(total_samples: u64, min_block: u16, max_block: u16) -> [u8; 34] {
        let mut out = [0u8; 34];
        let mut bit = 0usize;
        put_bits(&mut out, &mut bit, u32::from(min_block), 16);
        put_bits(&mut out, &mut bit, u32::from(max_block), 16);
        put_bits(&mut out, &mut bit, 0, 24); // min frame size: unknown
        put_bits(&mut out, &mut bit, 0, 24); // max frame size: unknown
        put_bits(&mut out, &mut bit, 8_000, 20);
        put_bits(&mut out, &mut bit, 0, 3); // channels minus one: mono
        put_bits(&mut out, &mut bit, 15, 5); // bits per sample minus one: 16
        // §8.2: 36 bits. `parse` reads it as 32 + 4 because the crate's reader
        // window is 32 bits wide, so the field is written in the same two parts.
        put_bits(
            &mut out,
            &mut bit,
            ((total_samples >> 4) & 0xFFFF_FFFF) as u32,
            32,
        );
        put_bits(&mut out, &mut bit, (total_samples & 0xF) as u32, 4);
        debug_assert_eq!(bit, 144, "Table 3's fields are 144 bits");
        out
    }

    /// Write `n` bits of `value` MSB-first into a byte buffer — the inverse of the
    /// crate's bit reader, and used only to state an input in this module's tests.
    fn put_bits(out: &mut [u8], bit: &mut usize, value: u32, n: u32) {
        for i in (0..n).rev() {
            let b = (value >> i) & 1;
            out[*bit / 8] |= (b as u8) << (7 - (*bit % 8));
            *bit += 1;
        }
    }

    /// A minimal valid 6-byte frame header — fixed blocking, 192-sample block,
    /// 32 kHz, mono, 16-bit, one-octet frame number — with the header CRC-8
    /// computed by the crate's own CRC so both builds accept it. Built for the
    /// state machine above: it asserts nothing about what an encoder emits, and its
    /// body is deliberately not a subframe.
    fn hand_header(number: u8) -> [u8; 6] {
        // Fixed fields: sync 0xFFE0|…, reserved 0, fixed blocking, block size code
        // 0b0001 (192), sample-rate code 0b1010 (32000), channel code 0b0000
        // (mono), sample-size code 0b100 (16), reserved 0 — then the one-octet
        // coded number, whose 0-prefixed form is the value itself for 0..=127.
        let mut h = [0xFFu8, 0xF8, 0x1A, 0x08, number & 0x7F, 0x00];
        h[5] = crate::bits::crc8(&h[..5]);
        h
    }
}
