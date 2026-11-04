//! The file-level metadata layer: RFC 9639 §6–§8 — the `fLaC` signature, the
//! metadata-block header chain of §8.1, and the streaminfo block of §8.2.
//!
//! This is what stands between "a caller handed me frame bytes" (what
//! [`crate::driver`] and [`crate::frame`] consume) and "an ordinary `.flac`
//! file" (what any real encoder writes, and what every other FLAC tool in this
//! repo produces). Before this module, the crate could only decode frames whose
//! bytes and stream properties a caller already knew: [`crate::frame::StreamDefaults`]
//! has no producer, and no code here had ever seen the bytes before the first
//! frame. Two things come out of this module and both are load-bearing for the
//! rest of split-ladder item 4:
//!
//! * **`frame_data_offset`** — the byte offset of the first audio frame, which
//!   is the only way to reach a frame region from a file, and the number the
//!   future packer (`scripts/pack_flac.sh`) will build its index from; and
//! * **a validated [`StreamInfo`]** — the sole producer of
//!   [`crate::frame::StreamDefaults`] ([`StreamInfo::as_defaults`]), because
//!   frame-header sample-rate code `0b0000` and sample-size code `0b000` mean
//!   "ask the stream" (see `format.rs`, and measured: libFLAC encodes the
//!   65,536 Hz rate that way in *every* frame).
//!
//! # No allocation, and no block list
//!
//! Like the rest of the crate this module is `core`-only: it allocates nothing,
//! not even once per stream. So the metadata-block list is *not* materialised.
//! The walk is streaming ([`Blocks`], borrowed, single pass) and the facts that
//! outlive it ([`StreamFacts`]) are fixed-size values: the streaminfo fields,
//! the block *count*, and the frame-data offset. A caller that genuinely wants
//! the list of blocks drives [`Blocks`] itself and keeps whatever it needs —
//! which is what `tests/stream_metadata_layout.rs` does against real files.
//! (If a later rung needs a retained list — a seektable-backed seek, say — the
//! no-alloc shape is a caller-provided `&mut [MetadataBlock]`, the same idiom
//! as the sample buffers in [`crate::frame::decode_frame`]. Nothing here
//! guesses at that.)
//!
//! # What is enforced here, and what is only reported
//!
//! The rules below are the ones §5/§8.1/§8.2 state normatively, each with what
//! the reference tools actually do — because libFLAC's *decoder* is deliberately
//! more forgiving than the format, and `metaflac` (its *metadata* tool) is
//! stricter still. Where the three disagree, the choice here is stated, not
//! implied; the witnesses are real files (`tests/stream_vectors.txt` records
//! `flac`/`metaflac` behaviour for the cases the fixtures can't carry, and
//! `FLAC.md`'s split-proposal entry records the measurements behind them).
//!
//! | Condition | Here | Measured libFLAC 1.5.0 |
//! |---|---|---|
//! | no `fLaC` signature | `Error::StreamMagic` | `flac -d`: "stream error" |
//! | block chain ends without a last-flagged block | `Error::MetadataTruncated` | `ERROR while decoding metadata \| state = …END_OF_STREAM` |
//! | a block's length runs past the end of the stream | `Error::MetadataTruncated` | same read error |
//! | no streaminfo, or streaminfo not first | `Error::StreamInfoMissing` | decodes, with `WARNING, cannot check MD5 signature since there was no STREAMINFO` (`metaflac`: exit 1) |
//! | more than one streaminfo | `Error::DuplicateStreamInfo` | `ERROR, more than one STREAMINFO found` |
//! | streaminfo data length ≠ 34 | `Error::InvalidField` | read error for 0/33; for 35 it decodes while `metaflac` exits 1 |
//! | block size < 16 or > 65535, or min > max | `Error::InvalidField` | decodes, with a warning about frame/sample numbering |
//! | bits per sample < 4 | `Error::InvalidField` | `ERROR: bits per sample is 1, must be 4-32` |
//! | bits per sample 4–32 other than 8/16 | `Error::UnsupportedSampleSize` | decodes fine (it is a valid FLAC stream) |
//! | sample rate 0 | `Error::InvalidField` | `ERROR, sample rate is 8000 in frame starting at sample 0 but 0 in STREAMINFO` |
//! | total samples 0 | kept, reported as unknown | `WARNING, cannot check total number of samples since it was unset` |
//! | MD5 all-zero | kept, reported as unknown | `WARNING, cannot check MD5 signature since it was unset` |
//! | frame sizes 0 | kept, reported as unknown | normal for an encode written to a pipe |
//! | metadata block type 7–126 (reserved) | walked past, by length | `flac -d` decodes; `metaflac` exits 1 |
//! | metadata block type 127 (forbidden, §5 Table 1) | `Error::InvalidField` | `flac -d` decodes; `metaflac` exits 1 |
//!
//! Three of those rows are choices rather than restatements, so each gets a
//! reason:
//!
//! * **No streaminfo is fatal here** where libFLAC merely warns. A stream
//!   without streaminfo has no sample rate or bit depth, and *this* crate's
//!   frame layer needs both (its `0b0000`/`0b000` codes resolve through
//!   [`StreamInfo::as_defaults`]). Warning and continuing would not make such
//!   a stream decodable; it would move the failure into `frame::parse` as a
//!   vague field error three layers from its cause.
//! * **A bit depth outside 8/16 is refused, not ignored.** The refusal belongs
//!   here, where the stream says what it is, rather than at the first frame
//!   header. The witness is `tests/fixtures/s4_24bit_8k_mono.flac`: a real,
//!   `flac -t`-verified 24-bit stream that both layers refuse (`UnsupportedSampleSize`
//!   from [`parse`] and again from `frame::parse`'s sample-size code `0b101`),
//!   so the capability statement is a tested fact instead of a sentence.
//! * **Type 127 is refused** because §5 lists it among the patterns that "MUST
//!   NOT appear in any bitstream", for a format-level reason (§8.1: it would be
//!   confused with the frame sync code) — the pattern that decides where a
//!   frame *begins*, which is exactly what a scanner like the one in item 4b
//!   depends on. libFLAC's decoder tolerates it; a decoder that also scans is
//!   the one case where tolerating it costs something. Reserved types 7–126 are
//!   a different matter: §8.1 lets a decoder abort on methods it doesn't
//!   recognise but the block framing is unambiguous, so they are skipped by
//!   length, which is what §8.1's MUST allows.
//!
//! Values that are *unknown* rather than wrong (total samples, MD5, frame
//! sizes) are kept and reported, never defaulted: an unknown total sample count
//! is ordinary for an encode written to a pipe ([`StreamInfo::total_samples_known`])
//! and pretending it were 0 would silently truncate a whole class of real
//! streams.
//!
//! # The frame count is arithmetic only in one case
//!
//! [`StreamInfo::exact_frame_count`] is `Some` for a fixed-blocksize stream
//! whose total sample count is known, and `None` otherwise — deliberately not a
//! "frame count bound": §8.2 says a variable-blocksize stream's actual maximum
//! block size MAY be smaller than the declared one (an encoder cannot know its
//! own block sizes when it writes the field), so `ceil(total / max_blocksize)`
//! is not an upper bound there, and `total == 0` says nothing at all. That
//! distinction is the input to item 4b's end-of-stream rule, which for an
//! unknown-total stream has to be end-of-input plus the next-frame-number check
//! instead.

use crate::bits::BitReader;
use crate::frame::StreamDefaults;
use crate::{Error, Result};

/// §6: "A FLAC bitstream consists of the fLaC (i.e., 0x664C6143) marker at the
/// beginning of the stream, followed by a mandatory metadata block (called the
/// streaminfo metadata block), any number of other metadata blocks, and then the
/// audio frames."
pub const MAGIC: [u8; 4] = *b"fLaC";

/// §8.1: "Each metadata block starts with a 4-byte header." The 4 bytes carry a
/// last-block flag, the block type, and the payload length; they are not part of
/// that length, which is why the walk advances by `4 + length`.
pub const BLOCK_HEADER_LEN: usize = 4;

/// §8.2 Table 3's exact data length: 16+16+24+24+20+3+5+36+128 = 272 bits.
/// A streaminfo block whose length differs cannot be parsed, and §8.2 gives no
/// version or extension field that would let a decoder read past it, so this is
/// the only length accepted for type 0.
pub const STREAMINFO_LEN: usize = 34;

/// §8.1: metadata block type 0 (streaminfo). The only type this crate reads.
pub const TYPE_STREAMINFO: u8 = 0;

/// §8.1: the type value that is forbidden outright ("A value of 127 … is
/// forbidden"), listed among §5's MUST-NOT patterns because it would be confused
/// with the frame sync code. Types 7–126 are Reserved, not forbidden.
pub const TYPE_FORBIDDEN: u8 = 127;

/// §8.1: the first bit of a metadata block header flags whether it is the last
/// one — "It is 0 when other metadata blocks follow; otherwise, it is 1."
#[inline]
pub const fn is_last_flag(first_byte: u8) -> bool {
    first_byte & 0x80 != 0
}

/// §8.1: the 7 remaining bits of the first header byte are the block type.
#[inline]
pub const fn block_type(first_byte: u8) -> u8 {
    first_byte & 0x7F
}

/// §8.1: the three bytes after the first "code for the size of the metadata
/// block in bytes, excluding the 4 header bytes, as an unsigned number coded
/// big-endian". RFC Appendix D.1.2 shows the arithmetic on the smallest possible
/// real example: `8000 0022` is last-flag + type 0 + 0x22 = 34 bytes.
#[inline]
pub const fn block_length(header: &[u8; 3]) -> usize {
    ((header[0] as usize) << 16) | ((header[1] as usize) << 8) | header[2] as usize
}

/// One metadata block's header, as it appears in the stream.
///
/// A value describes bytes in a slice rather than owning them, which is what
/// keeps this module allocation-free: [`MetadataBlock::payload`] borrows from
/// whichever slice the caller has, and a block that is skipped leaves nothing
/// behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetadataBlock {
    /// §8.1's 7-bit type. Only [`TYPE_STREAMINFO`] is interpreted.
    pub block_type: u8,
    /// §8.1's last-block flag: the block that ends the metadata chain and
    /// leaves the next byte to be a frame sync code.
    pub is_last: bool,
    /// Byte offset of this block's 4-byte header, so a caller can re-read it
    /// from the same slice, and so a refusal can name the block it is about.
    pub offset: usize,
    /// Payload length in bytes, excluding the header.
    pub length: usize,
}

impl MetadataBlock {
    /// This block's data, borrowed. `None` if the block runs past the end of
    /// the stream (a truncated stream, not a corrupt field).
    #[inline]
    pub fn payload<'a>(&self, stream: &'a [u8]) -> Option<&'a [u8]> {
        let (start, end) = self.span()?;
        stream.get(start..end)
    }

    /// `(start, end)` byte range of this block's payload within the stream, or
    /// `None` on offset overflow. Does *not* consult the stream length — that
    /// is [`Self::payload`]'s job.
    #[inline]
    pub fn span(&self) -> Option<(usize, usize)> {
        let start = self.offset.checked_add(BLOCK_HEADER_LEN)?;
        let end = start.checked_add(self.length)?;
        Some((start, end))
    }

    /// The byte immediately after this block: the next block's header, or — if
    /// [`Self::is_last`] — the first audio frame.
    #[inline]
    pub fn after(&self) -> Option<usize> {
        self.span().map(|(_, end)| end)
    }
}

/// The metadata-block chain, walked as borrowed bytes without materialising a
/// list (module docs: "No allocation, and no block list").
///
/// Errors end the walk: after an `Err` the iterator's position is unspecified,
/// because §8.1 gives no resynchronising rule for metadata — there is no sync
/// code to hunt for between blocks, only byte counts.
pub struct Blocks<'a> {
    stream: &'a [u8],
    next: usize,
    done: bool,
}

impl<'a> Blocks<'a> {
    /// Position a walk just past the `fLaC` magic.
    ///
    /// [`Error::StreamMagic`] if the stream does not begin with it. The check is
    /// made here rather than in the first `next_block` call so that a caller
    /// walking for a frame offset and a caller reading streaminfo agree on what
    /// a non-FLAC stream is.
    pub fn new(stream: &'a [u8]) -> Result<Self> {
        if stream.get(..MAGIC.len()) != Some(&MAGIC) {
            return Err(Error::StreamMagic);
        }
        Ok(Self {
            stream,
            next: MAGIC.len(),
            done: false,
        })
    }

    /// The next block header, or `Ok(None)` once the last metadata block has
    /// been returned (or once [`Self::cursor`] has reached the end of the
    /// stream).
    ///
    /// [`Error::MetadataTruncated`] when the header itself is cut short or the
    /// block's declared length runs past the end of the stream: either way the
    /// chain cannot be continued, and guessing is what §8.1's "could be
    /// vulnerable to buffer overflows" warning (§8.2, §11) is about.
    /// [`Error::InvalidField`] for the one type §5/§8.1 forbid outright
    /// ([`TYPE_FORBIDDEN`]); reserved types are returned like any other and are
    /// the caller's decision to skip or abort.
    pub fn next_block(&mut self) -> Result<Option<MetadataBlock>> {
        if self.done {
            return Ok(None);
        }
        let offset = self.next;
        let header: &[u8] = match self.stream.get(offset..offset + BLOCK_HEADER_LEN) {
            Some(h) => h,
            None => {
                self.done = true;
                return if offset == self.stream.len() {
                    Ok(None)
                } else {
                    Err(Error::MetadataTruncated)
                };
            }
        };
        let block = MetadataBlock {
            block_type: block_type(header[0]),
            is_last: is_last_flag(header[0]),
            offset,
            // `header` is four bytes by construction (sliced above), so the
            // three length bytes are always present.
            length: block_length(&[header[1], header[2], header[3]]),
        };
        if block.block_type == TYPE_FORBIDDEN {
            return Err(Error::InvalidField);
        }
        if block.span().is_none_or(|(_, end)| end > self.stream.len()) {
            self.done = true;
            return Err(Error::MetadataTruncated);
        }
        self.next = block.after().unwrap_or(usize::MAX);
        if block.is_last {
            self.done = true;
        }
        Ok(Some(block))
    }

    /// The byte the walk has reached: the next block header, or the first audio
    /// frame after a last-flagged block. This is the value `frame_data_offset`
    /// reports.
    #[inline]
    pub fn cursor(&self) -> usize {
        self.next
    }
}

/// §8.2 Table 3, the streaminfo block: what the whole stream is.
///
/// Every field is kept exactly as declared, including the values §8.2 marks as
/// "not known" (frame sizes 0, total samples 0, MD5 all-zero); the
/// `*_known`/`exact_frame_count` helpers interpret those sentinels instead of
/// the fields being rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamInfo {
    /// Minimum block size in samples, *excluding* the last block (§8.2). Equal
    /// to [`Self::max_blocksize`] exactly when the stream is fixed-blocksize.
    pub min_blocksize: u16,
    /// Maximum block size in samples. A frame's own blocksize may be below it
    /// (the last frame always may be; a variable-blocksize stream's actual
    /// maximum MAY be), which is why the frame layer — not this struct — is
    /// what decides how many samples a frame holds.
    pub max_blocksize: u16,
    /// Minimum frame size in bytes, 0 = unknown (§8.2 Table 3).
    pub min_framesize: u32,
    /// Maximum frame size in bytes, 0 = unknown. The future manifest/packer
    /// layer is where a bounded frame window could come from; nothing in the
    /// decode path needs it.
    pub max_framesize: u32,
    /// Sample rate in Hz. 0 means non-audio or a rate outside FLAC's
    /// representable range (§8.2) — refused by [`parse`] rather than carried,
    /// because this crate decodes audio.
    pub sample_rate_hz: u32,
    /// Bits per sample, 4–32 per Table 3. Only 8 and 16 are supported by this
    /// decoder; the refusal is [`parse`]'s, and the value stays here as read.
    pub bits_per_sample: u8,
    /// Channel count, 1–8 (Table 3 stores channels-minus-one).
    pub channels: u8,
    /// Total interchannel samples, 0 = unknown (§8.2).
    pub total_samples: u64,
    /// MD5 of the unencoded audio, interleaved signed little-endian; all-zero
    /// means not known (§8.2, and the definition is in the §8.2 prose rather
    /// than in the frame layer, so nothing here can recompute it).
    pub md5: [u8; 16],
}

impl StreamInfo {
    /// Read Table 3 from a streaminfo payload whose length has already been
    /// checked. Public so a caller that has its own streaminfo block (a future
    /// manifest carrying a copy of these facts, for instance) can read it here
    /// rather than duplicate the layout.
    ///
    /// [`Error::InvalidField`] for a wrong length or for the values §5 Table 1
    /// and §8.2 put in the MUST-NOT class; [`Error::UnsupportedSampleSize`] for
    /// a bit depth this decoder cannot carry.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() != STREAMINFO_LEN {
            // §8.2 gives no length or version field inside the block, so a
            // length other than 34 leaves nothing to key the layout off.
            // Measured: libFLAC refuses 0 and 33 outright with a metadata read
            // error, and accepts 35 while `metaflac` exits 1. Refusing every
            // other length is the only reading that does not depend on which
            // trailing bytes happen to follow.
            return Err(Error::InvalidField);
        }
        let mut r = BitReader::new(payload);
        // Table 3's order, exactly. Hoisting the 36-bit total above the fields
        // that precede it would read the wrong bits: the field order *is* the
        // layout here.
        let min_blocksize = r.read_bits(16)? as u16;
        let max_blocksize = r.read_bits(16)? as u16;
        let min_framesize = r.read_bits(24)?;
        let max_framesize = r.read_bits(24)?;
        let sample_rate_hz = r.read_bits(20)?;
        let channels = (r.read_bits(3)? as u8).wrapping_add(1); // 1..=8 by construction
        let bits_per_sample = (r.read_bits(5)? as u8).wrapping_add(1); // 1..=32
        // The only field wider than the reader's 32-bit window: two reads and a
        // shift. Its 36-bit maximum is what the unit test reads, because a
        // sentinel-free 36-bit value is otherwise indistinguishable from a
        // truncated one.
        let total_hi = r.read_bits(32)?;
        let total_samples = (u64::from(total_hi) << 4) | u64::from(r.read_bits(4)?);
        let info = StreamInfo {
            min_blocksize,
            max_blocksize,
            min_framesize,
            max_framesize,
            sample_rate_hz,
            channels,
            bits_per_sample,
            total_samples,
            md5: [0u8; 16],
        };
        // The field widths above total exactly 144 bits, so the MD5 begins on a
        // byte boundary (bit 144 = byte 18). That is arithmetic, not luck: it is
        // why this slice is legal and why no bit-shifting is needed for it.
        debug_assert_eq!(r.bit_position(), 144, "Table 3 fields end byte-aligned");
        let mut md5 = [0u8; 16];
        // Bounds proven by the length gate above: 144 bits = byte 18, and the
        // payload is exactly 34 bytes.
        md5.copy_from_slice(&payload[18..STREAMINFO_LEN]);

        // §5 Table 1 ("the patterns MUST NOT appear in any bitstream") and
        // §8.2's own normative paragraph: block sizes in the 16–65535 range, and
        // min ≤ max. Measured: libFLAC's *decoder* only warns and carries on,
        // and decodes such a file; enforcing it here is what makes
        // `max_blocksize` usable as a buffer bound by later layers, which is
        // exactly what §8.2's buffer-overflow warning is about.
        if !(16..=65535).contains(&u32::from(info.min_blocksize))
            || !(16..=65535).contains(&u32::from(info.max_blocksize))
            || info.min_blocksize > info.max_blocksize
        {
            return Err(Error::InvalidField);
        }
        // Table 3: "FLAC supports from 4 to 32 bits per sample." Measured:
        // libFLAC says it too, in as many words ("bits per sample is 1, must be
        // 4-32").
        if info.bits_per_sample < 4 {
            return Err(Error::InvalidField);
        }
        // §8.2: "The sample rate MUST NOT be 0 when the FLAC file contains
        // audio." This crate decodes audio. Measured: a streaminfo whose rate
        // was rewritten to 0 makes libFLAC stop with a frame/STREAMINFO
        // mismatch — it treats the same value as an error, just one frame
        // later.
        if info.sample_rate_hz == 0 {
            return Err(Error::InvalidField);
        }
        // Capability, not validity: 4/12/20/24/32-bit streams are real FLAC
        // (the 24-bit fixture is verified by `flac -t`) and this decoder carries
        // only 8- and 16-bit samples. Refusing here, where the stream states
        // its depth, keeps the frame layer's own refusal as the redundant
        // second witness it should be.
        match info.bits_per_sample {
            8 | 16 => {}
            _ => return Err(Error::UnsupportedSampleSize),
        }
        Ok(StreamInfo { md5, ..info })
    }

    /// Whether §8.2's "value of 0 means unknown" applies to the total sample
    /// count. Ordinary for an encode written to a pipe (`s5_unseekable_total_unknown.flac`).
    #[inline]
    pub fn total_samples_known(&self) -> bool {
        self.total_samples != 0
    }

    /// The same field as an option: `Some(count)` where the stream states one,
    /// `None` where it does not. Kept next to [`Self::total_samples_known`] so the
    /// sentinel and its interpretation stay together.
    ///
    /// The consumer is [`crate::scan`]'s end-of-stream cross-check, and the point
    /// of the `None` arm is that it is *ordinary*: a stream encoded to a pipe
    /// declares no total, decodes fine, and must be walked by counting frames to
    /// the end of the bytes instead.
    #[inline]
    pub fn total_samples_declared(&self) -> Option<u64> {
        if self.total_samples_known() {
            Some(self.total_samples)
        } else {
            None
        }
    }

    /// Whether the MD5 field carries a checksum rather than §8.2's "not known".
    /// Nothing here verifies it — computing it needs the whole decoded PCM,
    /// which is a decoder-level concern (and an unrequested one: see FLAC.md's
    /// open questions).
    #[inline]
    pub fn md5_known(&self) -> bool {
        self.md5 != [0u8; 16]
    }

    /// Whether the frame-size fields were written (they are 0 for an encode
    /// whose output could not be seeked back into).
    #[inline]
    pub fn frame_sizes_known(&self) -> bool {
        self.min_framesize != 0 && self.max_framesize != 0
    }

    /// §8.2: "If the minimum block size is equal to the maximum block size, the
    /// file contains a fixed block size stream."
    #[inline]
    pub fn is_fixed_blocksize(&self) -> bool {
        self.min_blocksize == self.max_blocksize
    }

    /// The number of frames in the stream, when the stream itself states enough
    /// to compute it: fixed block size (§8.2) plus a known total sample count.
    /// The last frame may be short, hence the ceiling.
    ///
    /// Deliberately *not* offered as a bound for other streams. §8.2: for a
    /// variable-blocksize stream "the actual maximum block size MAY be smaller
    /// than the maximum block size listed in the streaminfo metadata block" —
    /// an encoder writes those fields before it has any audio — so
    /// `ceil(total / max_blocksize)` is neither an upper bound on the frame
    /// count nor a safe buffer size. And with `total_samples == 0` there is
    /// nothing to divide: measured on `s5_unseekable_total_unknown.flac` (and
    /// independently on a 2100-sample stream whose declared maximum block size
    /// was 4096), a stream that declares no total decodes fine. Item 4b's
    /// end-of-stream rule therefore has to combine this — where it applies —
    /// with end-of-input plus the next-frame-number check where it does not.
    #[inline]
    pub fn exact_frame_count(&self) -> Option<usize> {
        if !self.total_samples_known() || !self.is_fixed_blocksize() {
            return None;
        }
        let blocksize = u64::from(self.max_blocksize);
        Some(usize::try_from(self.total_samples.div_ceil(blocksize)).unwrap_or(usize::MAX))
    }

    /// What [`crate::frame::FrameHeader::parse`] needs from the stream: frame
    /// headers carry sample-rate code `0b0000` and sample-size code `0b000` to
    /// mean "from the stream", and libFLAC uses the former for rates above the
    /// 4-bit table's top (measured: every frame of a 65,536 Hz encode).
    ///
    /// Only meaningful for a [`StreamInfo`] that came from [`StreamInfo::parse`]
    /// or [`parse`] — those are where the bit-depth capability gate lives, and
    /// a hand-built `StreamInfo` can still name a depth this decoder refuses.
    #[inline]
    pub fn as_defaults(&self) -> StreamDefaults {
        StreamDefaults {
            sample_rate_hz: self.sample_rate_hz,
            bits_per_sample: self.bits_per_sample,
        }
    }
}

/// What a whole-stream read yields: the streaminfo facts, how many metadata
/// blocks were walked, and where the audio frames begin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamFacts {
    /// The validated streaminfo.
    pub info: StreamInfo,
    /// Metadata blocks walked, including the last-flagged one. Reported rather
    /// than stored: this crate has no use for the list (module docs), but a
    /// packer or a diagnostic can use the count to notice a container it did
    /// not expect (an Ogg-style pile of unknown blocks, a 12,000-block padding
    /// chain).
    pub block_count: usize,
    /// Byte offset of the first audio frame — the first byte after the
    /// last-flagged metadata block. Everything downstream (this crate's
    /// `driver`, the future manifest's frame index, a scanner's starting point)
    /// is relative to this.
    pub frame_data_offset: usize,
}

/// Read a whole FLAC stream's file-level metadata: signature, block chain,
/// streaminfo.
///
/// The single entry point for "an ordinary `.flac` file", and the only place
/// the stream-level rules are applied. Borrows the caller's bytes, allocates
/// nothing, and stops at the end of the metadata: it neither parses a frame nor
/// requires one, so a headerless stream (metadata only) reads fine — which is
/// what makes this usable on a truncated file without pretending it is audio.
///
/// # Errors
///
/// Any of the stream-level refusals in the module table:
/// [`Error::StreamMagic`], [`Error::MetadataTruncated`],
/// [`Error::StreamInfoMissing`], [`Error::DuplicateStreamInfo`],
/// [`Error::InvalidField`], [`Error::UnsupportedSampleSize`].
pub fn parse(stream: &[u8]) -> Result<StreamFacts> {
    let mut walk = Blocks::new(stream)?;
    let mut info: Option<StreamInfo> = None;
    let mut first_block_type: Option<u8> = None;
    let mut block_count: usize = 0;

    let frame_data_offset = loop {
        let Some(block) = walk.next_block()? else {
            // The chain ran to the end of the stream without ever carrying the
            // last-block flag. Measured: libFLAC reports
            // `FLAC__STREAM_DECODER_END_OF_STREAM` for exactly this.
            return Err(Error::MetadataTruncated);
        };
        block_count += 1;
        if first_block_type.is_none() {
            first_block_type = Some(block.block_type);
        }
        if block.block_type == TYPE_STREAMINFO {
            if info.is_some() {
                // §8.2: "There MUST be no more than one streaminfo metadata
                // block per FLAC stream." Measured: libFLAC's own words are
                // "more than one STREAMINFO found". Decided here, at the second
                // occurrence, and before that block's contents are read: the
                // count is the rule, so a second streaminfo is refused whether
                // or not it would have parsed.
                return Err(Error::DuplicateStreamInfo);
            }
            let payload = block.payload(stream).ok_or(Error::MetadataTruncated)?;
            info = Some(StreamInfo::parse(payload)?);
        }
        if block.is_last {
            break walk.cursor();
        }
    };
    // §8.2: the streaminfo block "MUST be present as the first metadata block".
    // Judged from the first block's recorded type rather than by assuming that
    // reading a streaminfo means reading it first, so a stream with a streaminfo
    // in second position and none first is refused with the reason that actually
    // applies (its defaults are unusable) rather than the duplicate one.
    // libFLAC decodes such a stream with a warning; see the module table for
    // why this one does not.
    let info = match (info, first_block_type) {
        (Some(info), Some(TYPE_STREAMINFO)) => info,
        _ => return Err(Error::StreamInfoMissing),
    };
    Ok(StreamFacts {
        info,
        block_count,
        frame_data_offset,
    })
}

/// The byte offset of the first audio frame, walking the metadata chain by
/// length only.
///
/// This is [`parse`] without the streaminfo read or any of its rules: no stream
/// properties are needed to know where the frames begin, and a tool that packs
/// a file (the future `scripts/pack_flac.sh`) wants the offset without deciding
/// whether the stream's declared bit depth is something a decoder can carry. It
/// is *not* a lenient version of [`parse`]: the chain rules still apply, and a
/// caller that goes on to decode must call [`parse`] — otherwise a stream with
/// no streaminfo reaches [`crate::frame::parse`] with no stream defaults to
/// resolve a `0b0000`/`0b000` field through.
pub fn frame_data_offset(stream: &[u8]) -> Result<usize> {
    let mut walk = Blocks::new(stream)?;
    loop {
        let Some(block) = walk.next_block()? else {
            return Err(Error::MetadataTruncated);
        };
        if block.is_last {
            return Ok(walk.cursor());
        }
    }
}

#[cfg(test)]
mod tests {
    // Test-only: the structural tests assemble byte sequences, which needs a
    // growable buffer. The library code allocates nothing (module docs: "No
    // allocation, and no block list"), and `cargo check --target
    // thumbv4t-none-eabi` does not compile this module at all, so nothing here
    // puts `alloc` anywhere near a decode path or a shipped artifact.
    extern crate alloc;
    use alloc::vec::Vec;

    use super::*;
    use crate::MAX_BLOCKSIZE;
    use crate::bits;
    use crate::format::{ChannelConfig, SampleRate};
    use crate::frame::{
        FrameHeader, MAX_ACCEPTED_HEADER_BITS, MAX_HEADER_BITS, MAX_HEADER_BYTES, MIN_HEADER_BITS,
        StreamDefaults,
    };

    /// Bit-packs Table 3 for the structural tests.
    ///
    /// This is **not** a vector: the values exist to make a chain shape or a
    /// field value reachable, and every field's *correct* reading against real
    /// streams is witnessed by `tests/stream_metadata_layout.rs` against
    /// `metaflac`'s own numbers (the hand-packed-vector rule, FLAC.md). What
    /// this helper can witness that no real-file fixture can is the *shape* of
    /// the rules §8.1/§8.2 state — a chain that never sets a flag, a duplicate
    /// streaminfo, a bit depth of 24 — because no encoder in this repo writes
    /// those files.
    fn table3(min_b: u16, max_b: u16, rate: u32, channels: u8, bits: u8, total: u64) -> [u8; 34] {
        let mut bytes = [0u8; 34];
        let mut pos = 0usize;
        let mut put = |value: u64, width: u32| {
            for i in 0..width {
                let bit = (value >> (width - 1 - i)) & 1;
                bytes[(pos + i as usize) / 8] |= (bit as u8) << (7 - (pos + i as usize) % 8);
            }
            pos += width as usize;
        };
        put(u64::from(min_b), 16);
        put(u64::from(max_b), 16);
        put(0, 24); // minimum frame size: the "not known" sentinel
        put(0, 24); // maximum frame size: the "not known" sentinel
        put(u64::from(rate), 20);
        put(u64::from(channels) - 1, 3);
        put(u64::from(bits) - 1, 5);
        put(total, 36); // the helper takes u64 so the field's own top is reachable
        debug_assert_eq!(pos, 144, "Table 3's fields occupy exactly 144 bits");
        bytes
    }

    /// `fLaC` plus the given blocks, in order; each entry is
    /// (kind, last-metadata-block flag, payload).
    fn stream_with(blocks: &[(&str, bool, &[u8])]) -> Vec<u8> {
        let mut out = Vec::from(MAGIC);
        for (kind, is_last, payload) in blocks {
            let number: u8 = match *kind {
                "streaminfo" => TYPE_STREAMINFO,
                "padding" => 1,
                "reserved" => 7,
                "reserved126" => 126,
                "forbidden" => TYPE_FORBIDDEN,
                other => panic!("unknown block kind {other}"),
            };
            let len = u32::try_from(payload.len()).expect("test payload under 16 MiB");
            out.push(if *is_last { number | 0x80 } else { number });
            // §8.1: the length is three bytes, not four — this helper is a
            // fixture builder, so getting it wrong here would desynchronise
            // every chain test at once rather than failing one of them. The
            // `block_length` test above is what pins the field width itself.
            out.extend_from_slice(&len.to_be_bytes()[1..]);
            out.extend_from_slice(payload);
        }
        out
    }

    /// One last-flagged streaminfo block and nothing after it: the smallest
    /// stream that §8.1/§8.2 accept.
    fn minimal_stream() -> Vec<u8> {
        stream_with(&[("streaminfo", true, &table3(4096, 4096, 44100, 2, 16, 88200))])
    }

    /// The same chain with the three §8.2 "value of 0 … is not known" sentinels
    /// in place — the shape an encode to a pipe really has, which the layout
    /// suite reads out of `s5_unseekable_total_unknown.flac`. It is a separate
    /// helper from `minimal_stream` because the two witness opposite things: one
    /// declares everything, the other declares nothing.
    fn uninformative_stream() -> Vec<u8> {
        stream_with(&[("streaminfo", true, &table3(4096, 4096, 44100, 2, 16, 0))])
    }

    mod chain_shape {
        use super::*;

        #[test]
        fn a_stream_must_begin_with_the_magic() {
            // §6. A WAV header is the real near miss: `RIFF....WAVE` is what a
            // `.wav` — the file a GBA ROM build gets pointed at most often —
            // actually starts with, and it shares no byte with `fLaC`.
            let wav = *b"RIFF\x00\x00\x00\x00WAVEfmt ";
            assert_eq!(Blocks::new(&wav[..]).err(), Some(Error::StreamMagic));
            assert_eq!(parse(&wav[..]).err(), Some(Error::StreamMagic));
            assert_eq!(frame_data_offset(&wav[..]).err(), Some(Error::StreamMagic));
            // Bytes that are the wrong length for the magic get the same
            // answer, not an out-of-bounds: `get(..4)` is the whole check.
            assert_eq!(parse(&b"FLAC"[..]).err(), Some(Error::StreamMagic));
            assert_eq!(parse(&b"FLA"[..]).err(), Some(Error::StreamMagic));
            assert_eq!(parse(&[][..]).err(), Some(Error::StreamMagic));
            // Off-by-one is the other half of the check.
            let mut shifted = Vec::from(&b" fLaC"[..]);
            shifted.extend_from_slice(&minimal_stream()[4..]);
            assert_eq!(parse(&shifted).err(), Some(Error::StreamMagic));
        }

        #[test]
        fn the_block_header_is_flag_then_type_then_length() {
            // RFC Appendix D.1.2's own smallest worked example, byte for byte:
            // a 34-byte streaminfo carrying the last-block flag is `8000 0022`.
            assert!(is_last_flag(0x80));
            assert_eq!(block_type(0x80), TYPE_STREAMINFO);
            assert_eq!(block_length(&[0x00, 0x00, 0x22]), STREAMINFO_LEN);
            // The flag is not part of the type, and the type is not part of the
            // length: the same three bytes read three ways.
            assert_eq!(block_type(0x81), 1);
            assert!(!is_last_flag(0x01));
            assert_eq!(block_length(&[0xFF, 0xFF, 0xFF]), 0xFF_FFFF);
        }

        #[test]
        fn a_chain_that_never_sets_the_last_flag_is_truncated() {
            // §8.1's flag is the only thing that ends the chain. Without it the
            // next byte is either another block header or a frame sync, and
            // metadata has no sync code to search for in order to decide —
            // which is why the answer here is truncation rather than "assume
            // the frames start here". Measured: libFLAC reports
            // `FLAC__STREAM_DECODER_END_OF_STREAM`.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[("streaminfo", false, &si)]);
            assert_eq!(parse(&s).err(), Some(Error::MetadataTruncated));
            assert_eq!(frame_data_offset(&s).err(), Some(Error::MetadataTruncated));
            // The walk itself ends cleanly at the last byte; requiring the flag
            // is `parse`'s business, because only `parse` claims to know where
            // the audio begins.
            let mut walk = Blocks::new(&s).unwrap();
            assert!(walk.next_block().unwrap().is_some());
            assert!(walk.next_block().unwrap().is_none());
            assert_eq!(walk.cursor(), s.len());
        }

        #[test]
        fn a_block_whose_length_runs_past_the_stream_is_truncated() {
            // A length field is a promise about bytes the caller may not have.
            // Measured: the reference reports `FLAC__STREAM_DECODER_READ_METADATA`
            // for a streaminfo whose declared length cannot be satisfied.
            let mut s = Vec::from(MAGIC);
            s.extend_from_slice(&[TYPE_STREAMINFO, 0x00, 0x00, 0xFF]); // claims 255 bytes
            s.extend_from_slice(&table3(4096, 4096, 44100, 2, 16, 0));
            assert_eq!(parse(&s).err(), Some(Error::MetadataTruncated));
            let mut walk = Blocks::new(&s).unwrap();
            assert_eq!(walk.next_block().err(), Some(Error::MetadataTruncated));
            // The header itself was readable, so the refusal is about the
            // payload. A *partial* header is the other half of the same rule,
            // and the distinction is the walk's to make: running out of bytes
            // exactly at a block boundary is the clean end of the chain, while
            // a fragment of a header is a truncated stream.
            let nothing_left = Vec::from(MAGIC);
            assert!(
                Blocks::new(&nothing_left)
                    .unwrap()
                    .next_block()
                    .unwrap()
                    .is_none()
            );
            let mut partial = nothing_left;
            partial.extend_from_slice(&[TYPE_STREAMINFO, 0x00, 0x00]); // 3 of 4 bytes
            assert_eq!(
                Blocks::new(&partial).unwrap().next_block().err(),
                Some(Error::MetadataTruncated)
            );
        }

        #[test]
        fn a_block_that_fits_exactly_at_the_end_of_the_stream_is_not_truncated() {
            // The boundary the other test sits next to: a metadata-only stream
            // whose last block ends on the last byte is *complete*, with zero
            // frames after it. Nothing here requires audio to exist.
            let s = minimal_stream();
            let facts = parse(&s).unwrap();
            assert_eq!(facts.frame_data_offset, s.len());
            assert_eq!(facts.block_count, 1);
        }

        #[test]
        fn reserved_types_are_walked_past_by_length() {
            // §8.1: a decoder MAY abort on methods it does not recognise, but
            // the framing is unambiguous whatever the type says, so skipping by
            // length is the §8.1-compliant option that needs no registry.
            // Measured: libFLAC's decoder skips type 7 and type 8 blocks (the
            // PCM is byte-identical to the unmodified file) while `metaflac`
            // exits 1.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[
                ("streaminfo", false, &si),
                ("reserved", false, b"whatever a decoder is meant to ignore"),
                ("padding", true, b"\x00\x00"),
            ]);
            let facts = parse(&s).unwrap();
            assert_eq!(facts.block_count, 3);
            assert_eq!(facts.frame_data_offset, s.len());
            assert_eq!(frame_data_offset(&s).unwrap(), s.len());
            // The skipped block is still describable if a caller wants it: the
            // walk hands back headers it did not interpret, with offsets.
            let mut walk = Blocks::new(&s).unwrap();
            let skipped = walk.next_block().unwrap().unwrap();
            let _ = walk.next_block().unwrap().unwrap();
            let _ = walk.next_block().unwrap().unwrap();
            assert_eq!(skipped.payload(&s).unwrap().len(), skipped.length);
        }

        #[test]
        fn the_forbidden_type_is_refused_whereas_reserved_types_are_not() {
            // §5 Table 1 puts type 127 among the patterns that "MUST not
            // appear" in any bitstream, and §8.1 gives the reason: a value of
            // 127 would be confused with the frame sync code. The module table
            // records that libFLAC's decoder tolerates it anyway (its PCM comes
            // out byte-identical) while `metaflac` exits 1 — the place where
            // tolerating it can cost something is a decoder that also *scans*
            // for frames, which is item 4b.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[("streaminfo", false, &si), ("forbidden", true, b"")]);
            assert_eq!(parse(&s).err(), Some(Error::InvalidField));
            assert_eq!(frame_data_offset(&s).err(), Some(Error::InvalidField));
            // Type 126 is Reserved, not forbidden: the boundary is exactly one
            // type value wide, so the test for the boundary is the value next
            // to it. The streaminfo stays *first* — §8.2's ordering rule is a
            // separate rule, asserted by `the_first_block_must_be_the_streaminfo`
            // below; mixing the two here would make a pass ambiguous about which
            // rule held.
            let s = stream_with(&[("streaminfo", false, &si), ("reserved126", true, b"")]);
            let facts = parse(&s).unwrap();
            assert_eq!(facts.block_count, 2);
            assert_eq!(facts.frame_data_offset, s.len());
            assert_eq!(frame_data_offset(&s).unwrap(), s.len());
        }

        #[test]
        fn the_walk_reports_offsets_a_caller_can_re_read_from() {
            // The no-alloc contract, asserted: the walk is the list, and a
            // caller that wants to keep it keeps it from its own storage.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[
                ("streaminfo", false, &si),
                ("padding", false, b"\x00\x01\x02"),
                ("padding", true, b"\xff"),
            ]);
            let mut walk = Blocks::new(&s).unwrap();
            let mut got = [(0usize, 0usize); 3];
            for slot in got.iter_mut() {
                let b = walk.next_block().unwrap().unwrap();
                *slot = (b.offset, b.length);
            }
            assert_eq!(got[0], (MAGIC.len(), STREAMINFO_LEN));
            assert_eq!(got[1], (MAGIC.len() + 4 + STREAMINFO_LEN, 3));
            assert_eq!(got[2], (got[1].0 + 4 + 3, 1));
            assert!(walk.next_block().unwrap().is_none());
            assert_eq!(walk.cursor(), got[2].0 + 4 + 1);
        }
    }

    mod streaminfo_position_and_count {
        use super::*;

        #[test]
        fn the_first_block_must_be_the_streaminfo() {
            // §8.2's MUST. libFLAC only warns ("cannot check MD5 signature
            // since there was no STREAMINFO") and goes on decoding, because it
            // can take its stream properties from the frame headers instead;
            // this crate's frame layer resolves `0b000`/`0b000` through the
            // streaminfo it just refused to accept, so warning here would
            // relocate the failure rather than avoid it. The module table states
            // the difference and this paragraph is the reason for it.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[("padding", false, &[0u8; 8]), ("streaminfo", true, &si)]);
            assert_eq!(parse(&s).err(), Some(Error::StreamInfoMissing));
            // Walking for the frame offset is a different question and has no
            // such rule: the chain is well-formed either way (`frame_data_offset`'s
            // doc says what a caller must still do).
            assert_eq!(frame_data_offset(&s).unwrap(), s.len());
        }

        #[test]
        fn a_stream_with_no_streaminfo_at_all_is_refused() {
            let s = stream_with(&[("padding", true, &[0u8; 4])]);
            assert_eq!(parse(&s).err(), Some(Error::StreamInfoMissing));
            // An empty metadata chain is the degenerate case of the *chain*
            // rule, not of this one: there is no first block to judge.
            assert_eq!(
                parse(&stream_with(&[])).err(),
                Some(Error::MetadataTruncated)
            );
            assert_eq!(
                frame_data_offset(&stream_with(&[])).err(),
                Some(Error::MetadataTruncated)
            );
        }

        #[test]
        fn a_second_streaminfo_is_refused() {
            // §8.2's "MUST be no more than one", and the measured reference
            // wording is literally "ERROR, more than one STREAMINFO found".
            // Refused even when the second one is unreadable-by-length or
            // identical to the first: the count is the rule.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            for second in [si, table3(2048, 2048, 22050, 1, 16, 0)] {
                let s = stream_with(&[("streaminfo", false, &si), ("streaminfo", true, &second)]);
                assert_eq!(parse(&s).err(), Some(Error::DuplicateStreamInfo));
            }
            // …while a streaminfo plus a *reserved* block is ordinary (§8.1).
            let s = stream_with(&[("streaminfo", false, &si), ("reserved", true, b"x")]);
            assert_eq!(parse(&s).unwrap().block_count, 2);
        }

        #[test]
        fn block_count_is_reported_and_the_list_is_left_to_the_caller() {
            // A four-block chain of the shape the s2 fixture really has
            // (streaminfo, seektable, vorbis comment, padding). Nothing is
            // materialised; a caller that wants the list drives `Blocks`.
            let si = table3(4096, 4096, 44100, 2, 16, 0);
            let s = stream_with(&[
                ("streaminfo", false, &si),
                (
                    "reserved",
                    false,
                    b"seektable-shaped bytes are not what matters here",
                ),
                ("reserved", false, b"nor are vorbis comment bytes"),
                ("padding", true, b""),
            ]);
            let facts = parse(&s).unwrap();
            assert_eq!(facts.block_count, 4);
            assert_eq!(facts.frame_data_offset, s.len());
            assert_eq!(frame_data_offset(&s).unwrap(), s.len());
        }
    }

    mod streaminfo_fields {
        use super::*;

        #[test]
        fn table_3_is_read_field_by_field_across_the_byte_boundaries() {
            // The one hand-built streaminfo whose *values* are checked, and the
            // reason it is the one: the 20-bit rate, the 3+5 channel/depth pair
            // and the 36-bit sample count are the only fields that straddle
            // bytes, so they are the fields a layout bug could hide in. Real
            // streams' values come from `metaflac` in the layout suite.
            let mut payload = table3(0x1234, 0x5678, 0xF_FFFF, 8, 16, 0xF_FFFF_FFFF_F);
            // The two u24 frame-size fields are byte-aligned at bits 32 and 56
            // (bytes 4..7 and 7..10), so they can be patched without touching
            // any other field — the check that the packing above is the layout
            // and not a coincidence.
            payload[4..7].copy_from_slice(&[0x00, 0x01, 0x02]);
            payload[7..10].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
            payload[18..].copy_from_slice(&[
                0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
                0x32, 0x10,
            ]);
            let info = StreamInfo::parse(&payload).unwrap();
            assert_eq!(info.min_blocksize, 0x1234);
            assert_eq!(info.max_blocksize, 0x5678);
            assert_eq!(info.min_framesize, 0x0102);
            assert_eq!(info.max_framesize, 0xFF_FFFF); // the u24 field's top
            assert!(info.frame_sizes_known());
            assert_eq!(info.sample_rate_hz, 0xF_FFFF); // the u20 field's top
            assert_eq!(info.channels, 8); // the u3 field's top, +1 applied once
            assert_eq!(info.bits_per_sample, 16);
            assert_eq!(info.total_samples, 0xF_FFFF_FFFF); // the u36 field's top (2^36-1):
            // read as 32+4 because `read_bits` tops out at 32 bits, so the seam in
            // the middle of this value is the thing the assert is for.
            assert_eq!(info.md5[0], 0x01);
            assert_eq!(info.md5[8], 0xFE);
            assert_eq!(info.md5[15], 0x10);
            assert!(info.total_samples_known());
            assert!(info.md5_known());
            // The defaults the frame layer needs, and nothing else.
            assert_eq!(
                info.as_defaults(),
                StreamDefaults {
                    sample_rate_hz: 0xF_FFFF,
                    bits_per_sample: 16
                }
            );
        }

        #[test]
        fn the_bit_depth_field_is_read_at_its_own_maximum() {
            // The bit-depth *code* field (u5, value+1) tops out at 32, and 32
            // can never appear in an `Ok` result because of the capability gate.
            // Reading it as UnsupportedSampleSize rather than as a length error
            // is what proves the field was read as five bits and not six, or
            // from the wrong offset.
            assert_eq!(
                StreamInfo::parse(&table3(4096, 4096, 44100, 2, 32, 0)).err(),
                Some(Error::UnsupportedSampleSize)
            );
        }

        #[test]
        fn a_streaminfo_of_any_other_length_is_refused() {
            // Measured split, and the reason for refusing them all: libFLAC
            // refuses 0 and 33 with a metadata read error but *decodes* 35 while
            // `metaflac` exits 1. Any length other than 34 makes the reading
            // depend on which bytes happen to follow the block, so none of them
            // is a layout this crate could commit to.
            for (extra, note) in [
                (0usize, "empty"),
                (1, "short"),
                (33, "one short"),
                (35, "one over"),
                (68, "double"),
            ] {
                let mut payload = Vec::new();
                payload.extend_from_slice(&table3(4096, 4096, 44100, 2, 16, 0)[..extra.min(34)]);
                if extra > 34 {
                    payload.resize(extra, 0xFF);
                }
                assert_eq!(
                    StreamInfo::parse(&payload).err(),
                    Some(Error::InvalidField),
                    "{note}: length {extra}"
                );
            }
            // 34 is the only accepted length, asserted alongside the refusals so
            // the loop cannot pass on a blanket rejection.
            assert_eq!(
                StreamInfo::parse(&table3(4096, 4096, 44100, 2, 16, 0))
                    .unwrap()
                    .bits_per_sample,
                16
            );
        }

        #[test]
        fn block_sizes_outside_the_formats_range_or_out_of_order_are_invalid() {
            // §5 Table 1 ("Minimum and maximum block sizes smaller than 16 in
            // streaminfo metadata block" is a forbidden pattern) and §8.2's own
            // MUSTs ("MUST be in the 16-65535 range", "minimum … MUST be equal
            // to or less than the maximum"). Measured: libFLAC's decoder only
            // warns and decodes anyway; §8.2 leaves that choice open ("decoder
            // behavior is left unspecified"). Enforcing it here is what makes
            // `max_blocksize` usable as a buffer bound at all, which is exactly
            // what §8.2's buffer-overflow warning warns about.
            for (min_b, max_b) in [
                (0u16, 4096),
                (15, 4096), // the §5 boundary itself
                (4096, 15),
                (4096, 4095), // min > max
            ] {
                assert_eq!(
                    StreamInfo::parse(&table3(min_b, max_b, 44100, 2, 16, 0)).err(),
                    Some(Error::InvalidField),
                    "{min_b}/{max_b}"
                );
            }
            // Both ends of the allowed range, and min == max, which is what
            // §8.2 calls a fixed-blocksize stream.
            assert!(StreamInfo::parse(&table3(16, 16, 44100, 1, 16, 0)).is_ok());
            assert!(StreamInfo::parse(&table3(16, 65535, 44100, 1, 16, 0)).is_ok());
        }

        #[test]
        fn a_bit_depth_below_the_formats_floor_is_not_a_capability_gap() {
            // Table 3: "FLAC supports from 4 to 32 bits per sample" — and the
            // measured reference wording for a mutated stream is the same rule
            // in libFLAC's voice ("bits per sample is 1, must be 4-32"). Below 4
            // is a broken stream; 4..32 outside 8/16 is this decoder's limit.
            // Keeping those two refusals different is the point: the format and
            // this crate are not the same thing, and a caller has to know which
            // one it fell over.
            for bits in 1..4u8 {
                assert_eq!(
                    StreamInfo::parse(&table3(4096, 4096, 44100, 2, bits, 0)).err(),
                    Some(Error::InvalidField),
                    "{bits} bits"
                );
            }
            for bits in [4u8, 12, 20, 24, 32] {
                assert_eq!(
                    StreamInfo::parse(&table3(4096, 4096, 44100, 2, bits, 0)).err(),
                    Some(Error::UnsupportedSampleSize),
                    "{bits} bits"
                );
            }
            for bits in [8u8, 16] {
                assert!(StreamInfo::parse(&table3(4096, 4096, 44100, 2, bits, 0)).is_ok());
            }
        }

        #[test]
        fn a_sample_rate_of_zero_is_refused_rather_than_carried_to_the_frames() {
            // §8.2/§9.1.7 allow 0 for non-audio; an audio decoder cannot use it.
            // Measured: libFLAC agrees with the verdict and disagrees with the
            // timing — "sample rate is 8000 in frame starting at sample 0 but 0
            // in STREAMINFO". Refusing at the stream is the same answer without
            // spending a frame on it, and it is the answer a caller can act on
            // before allocating anything.
            assert_eq!(
                StreamInfo::parse(&table3(4096, 4096, 0, 2, 16, 0)).err(),
                Some(Error::InvalidField)
            );
            // 1 Hz is absurd but representable; the field's own range is the
            // only bound this layer imposes on a rate.
            assert_eq!(
                StreamInfo::parse(&table3(4096, 4096, 1, 2, 16, 0))
                    .unwrap()
                    .sample_rate_hz,
                1
            );
        }

        #[test]
        fn unknown_is_kept_as_unknown() {
            // §8.2's three "a value of 0 signifies that the value is not known"
            // sentinels, in the shape that carries them in real files (an encode
            // to a pipe — `s5_unseekable_total_unknown.flac`): nothing is
            // defaulted, nothing is refused, and the frame count simply is not
            // computable. The frame offset, by contrast, is known exactly.
            let facts = parse(&uninformative_stream()).unwrap();
            assert_eq!(facts.info.total_samples, 0);
            assert_eq!(facts.info.min_framesize, 0);
            assert_eq!(facts.info.max_framesize, 0);
            assert_eq!(facts.info.md5, [0u8; 16]);
            assert!(!facts.info.total_samples_known());
            assert!(!facts.info.md5_known());
            assert!(!facts.info.frame_sizes_known());
            assert_eq!(facts.info.exact_frame_count(), None);
            assert_eq!(facts.frame_data_offset, MAGIC.len() + 4 + STREAMINFO_LEN);
            // …and a streaminfo that *does* declare them keeps them verbatim.
            assert!(
                StreamInfo::parse(&{
                    let mut p = table3(4096, 4096, 44100, 2, 16, 1);
                    p[18] = 0x80;
                    p
                })
                .is_ok_and(|i| i.total_samples_known() && i.md5_known())
            );
        }

        #[test]
        fn the_frame_count_is_computed_only_when_the_stream_states_enough() {
            // §8.2: "If the minimum block size is equal to the maximum block
            // size, the file contains a fixed block size stream" — and then the
            // count is the ceiling over the block size, because the last frame
            // legitimately may be short (§8.2: "The last frame MUST have a
            // block size equal to or less than the maximum block size").
            let fixed = StreamInfo::parse(&table3(4096, 4096, 44100, 2, 16, 88200)).unwrap();
            assert!(fixed.is_fixed_blocksize());
            assert_eq!(fixed.exact_frame_count(), Some(22)); // 88200 / 4096 = 21.5…
            let ragged = StreamInfo::parse(&table3(4096, 4096, 44100, 2, 16, 1)).unwrap();
            assert_eq!(ragged.exact_frame_count(), Some(1));
            let exact = StreamInfo::parse(&table3(4096, 4096, 44100, 2, 16, 4096)).unwrap();
            assert_eq!(exact.exact_frame_count(), Some(1));
            // Variable blocking: §8.2 is explicit that "the actual maximum block
            // size MAY be smaller than the maximum block size listed in the
            // streaminfo metadata block" (an encoder cannot know its own block
            // sizes when it writes the field), so ceil(total / max_blocksize) is
            // not an upper bound and no bound is offered instead. Offering one
            // would be the mistake this half of the test exists to prevent —
            // item 4b's end-of-stream rule has to be end-of-input plus the next
            // frame number where this returns None.
            let variable = StreamInfo::parse(&table3(2048, 4096, 44100, 2, 16, 88200)).unwrap();
            assert!(!variable.is_fixed_blocksize());
            assert_eq!(variable.exact_frame_count(), None);
            // And a known total with an unknown blocksize range is the same
            // absence, not a guess.
            assert_eq!(
                StreamInfo::parse(&table3(2048, 4096, 44100, 2, 16, 0))
                    .unwrap()
                    .exact_frame_count(),
                None
            );
        }
    }

    mod header_width_bounds {
        use super::*;

        /// The widest header a *legal* frame can spell: both appended value
        /// forms (§9.1.6 with the largest representable block size, §9.1.7 with
        /// a rate the 4-bit table cannot name), the longest legal coded number
        /// (§9.1.5's 31-bit frame-number cap, in its minimal six-octet form),
        /// and a real CRC-8. Built from the field rules, not copied from
        /// libFLAC — and its byte 2/byte 3 pair is the same code pair the real
        /// 80-bit header in `s3_wide_48001hz_b3000.flac` carries.
        fn widest_accepted_header() -> ([u8; MAX_HEADER_BYTES - 1], StreamDefaults) {
            let mut bytes = [0u8; MAX_HEADER_BYTES - 1]; // 15 bytes = 120 bits
            bytes[0] = 0xFF;
            bytes[1] = 0xF8; // sync + reserved 0 + fixed-blocksize strategy
            bytes[2] = 0b0111_1101; // block size: 16-bit form; rate: 16-bit Hz form
            bytes[3] = 0x08; // mono, bit depth from the stream, reserved 0
            // Minimal six-octet form of 0x7FFF_FFFF — the 31-bit cap reached,
            // i.e. exactly the boundary §9.1.5 draws for a frame number.
            bytes[4..10].copy_from_slice(&[0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF]);
            bytes[10] = 0x7F; // block size value 32767 → 32768 samples, the
            bytes[11] = 0xFF; // top the 16-bit form carries (MAX_BLOCKSIZE)
            // Sample rate 48001 Hz, taken byte for byte from the real header in
            // `s3_wide_48001hz_b3000.flac` (`FF F8 7D 08 00 BB 81 8B B8 49`)
            // rather than from hand arithmetic — the appended form is the part
            // this test shares with that fixture.
            bytes[12] = 0xBB;
            bytes[13] = 0x81;
            bytes[14] = bits::crc8(&bytes[..MAX_HEADER_BYTES - 2]);
            (
                bytes,
                StreamDefaults {
                    sample_rate_hz: 44100,
                    bits_per_sample: 16,
                },
            )
        }

        #[test]
        fn the_two_width_ceiling_follows_from_the_number_rules() {
            // 128 bits is the widest *spelling* the format has (§9.1.5: "a
            // maximum of 36 bits unencoded or 7 bytes encoded"); libFLAC sizes
            // its own header scratch buffer by the same arithmetic
            // (`raw_header[16]`). 120 is the widest header this parser can
            // return, because the 7-octet number form cannot occur here — §9.1.5
            // caps a frame number at 31 bits and the 7-octet form begins at
            // 2^31 (Table 18), and a *sample* number (variable blocking) is
            // refused before the number is read. The one-octet gap between them
            // is that single form, and the test asserts the gap by its cause.
            assert_eq!(MAX_HEADER_BITS, 32 + 7 * 8 + 16 + 16 + 8);
            assert_eq!(MAX_HEADER_BYTES * 8, MAX_HEADER_BITS);
            assert_eq!(
                bits::coded_number_octets(0xFE),
                Some(7),
                "the 7-octet form is the ceiling"
            );
            assert_eq!(
                bits::coded_number_octets(0xFD),
                Some(6),
                "the widest a legal frame number reaches"
            );
            assert_eq!(MAX_ACCEPTED_HEADER_BITS, MAX_HEADER_BITS - 8);
            assert_eq!(MIN_HEADER_BITS, 32 + 8 + 8);
            assert!(MIN_HEADER_BITS < MAX_ACCEPTED_HEADER_BITS);
        }

        #[test]
        fn the_accepted_ceiling_is_reachable_and_the_parse_stops_exactly_on_it() {
            let (bytes, defaults) = widest_accepted_header();
            let mut reader = BitReader::new(&bytes);
            let header = FrameHeader::parse(&mut reader, &defaults).unwrap();
            assert_eq!(header.header_bits, MAX_ACCEPTED_HEADER_BITS);
            assert_eq!(reader.bit_position(), MAX_ACCEPTED_HEADER_BITS);
            assert_eq!(
                reader.bit_position(),
                bytes.len() * 8,
                "no byte left unread"
            );
            // The widest header also carries the largest legal block size and
            // the largest legal frame number — the three maxima in one frame,
            // each derived from its own rule rather than asserted.
            assert_eq!(header.blocksize, MAX_BLOCKSIZE);
            assert_eq!(header.number, 0x7FFF_FFFF);
            assert_eq!(header.sample_rate, SampleRate::Explicit(48001));
            assert_eq!(header.sample_rate.hz(defaults.sample_rate_hz), 48001);
            assert_eq!(header.channels, ChannelConfig::Independent { channels: 1 });
            // Bit-depth code 0b000 in a max-width header still resolves through
            // the stream, so streaminfo is load-bearing even at the ceiling.
            assert_eq!(header.bits_per_sample, 16);
            // Not something a real encoder emits: the widest header in any file
            // this repo commits is 80 bits (`s3_wide_48001hz_b3000.flac`,
            // parsed from real bytes by tests/stream_metadata_layout.rs), and
            // the census in tests/stream_vectors.txt keeps even *that* claim
            // measured. This is the bound a scanner (item 4b) has to respect.
        }

        #[test]
        fn a_seven_octet_frame_number_is_refused_before_the_cursor_reaches_the_ceiling() {
            // The ordering that keeps the ceiling honest, asserted as behaviour
            // rather than as arithmetic: the 7-octet form's minimum is 2^31
            // (§9.1.5 Table 18), past the 31-bit cap, so a header spelled that
            // way stops at the number — 88 bits — and never gets to push a
            // cursor towards 128. If either rule ever changes, this test is
            // where the two derivations are found out separately.
            let mut bytes = [0u8; MAX_HEADER_BYTES];
            bytes[0] = 0xFF;
            bytes[1] = 0xF8;
            bytes[2] = 0b0111_1101;
            bytes[3] = 0x08;
            bytes[4..11].copy_from_slice(&[0xFE, 0x82, 0x80, 0x80, 0x80, 0x80, 0x80]);
            let mut reader = BitReader::new(&bytes);
            let defaults = StreamDefaults {
                sample_rate_hz: 44100,
                bits_per_sample: 16,
            };
            assert_eq!(
                FrameHeader::parse(&mut reader, &defaults).err(),
                Some(Error::InvalidField)
            );
            assert_eq!(reader.bit_position(), 32 + 7 * 8, "stopped at the number");
            assert!(reader.bit_position() < MAX_HEADER_BITS);
        }

        #[test]
        fn a_header_needing_more_bytes_than_the_region_has_says_end_of_stream() {
            // The last frame of a region is the ordinary case of this: a wide
            // header whose appended forms run past the caller's slice. The frame
            // layer's rule, not this module's — it lives here because the width
            // arithmetic is what makes it a question at all.
            let bytes = [0xFFu8, 0xF8, 0b0111_1101, 0x08, 0x00]; // number = 0, then needs 16+16+CRC
            let mut reader = BitReader::new(&bytes);
            let defaults = StreamDefaults {
                sample_rate_hz: 44100,
                bits_per_sample: 16,
            };
            assert_eq!(
                FrameHeader::parse(&mut reader, &defaults).err(),
                Some(Error::EndOfStream)
            );
        }

        #[test]
        fn a_wide_header_is_refused_inside_its_fixed_prefix() {
            // Item 4b's scan cost depends on this: a candidate sync position is
            // thrown away after at most the 32 fixed bits, not after 120. Each
            // row mutates one byte of the legal max-width header above, so the
            // only thing a row tests is the fixed-field rule that lives in that
            // byte — and the asserted `bit_position` is where the refusal
            // happened, which is the claim about cost.
            let (base, defaults) = widest_accepted_header();
            for (at, value, expected, why) in [
                (
                    1usize,
                    0xFAu8,
                    Error::InvalidField,
                    "the reserved bit after the 14-bit sync code is set (§9.1)",
                ),
                (
                    2usize,
                    0b0111_1111u8,
                    Error::InvalidField,
                    "sample-rate code 0b1111 is not in Table 15",
                ),
                (
                    3usize,
                    0x09u8,
                    Error::InvalidField,
                    "the reserved bit at the end of the fixed fields is set (§9.1)",
                ),
                (
                    3usize,
                    0x06u8,
                    Error::InvalidField,
                    "sample-size code 0b011 is reserved (Table 17)",
                ),
                (
                    3usize,
                    0x28u8,
                    Error::ProfileViolation,
                    "a 3-channel layout: valid FLAC, outside this crate's 1/2-channel profile",
                ),
            ] {
                let mut bytes = base;
                bytes[at] = value;
                let mut reader = BitReader::new(&bytes);
                let err = FrameHeader::parse(&mut reader, &defaults)
                    .err()
                    .unwrap_or_else(|| panic!("{why}: parsed Ok"));
                assert_eq!(err, expected, "{why}");
                assert!(
                    reader.bit_position() <= 32,
                    "{why}: read {} bits before refusing",
                    reader.bit_position()
                );
            }
        }
    }
}
