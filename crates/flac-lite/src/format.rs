//! Container format (`GAFP`) + manifest parsing, and the constrained encode
//! profile the packer and decoder agree on.
//!
//! SCAFFOLD STATE: types and signatures only, bodies are `todo!()`.
//! The byte-level contract lives in [`../README.md`](../README.md).
//!
//! ## Why a manifest instead of FLAC's own metadata blocks
//!
//! Standard FLAC puts a `STREAMINFO`/`SEEKTABLE`/Vorbis-comment metadata stack in
//! front of the frames, and decoders use it plus `Seek`/`Read` to navigate. On
//! this target we replace all of that with an offline-emitted, fixed-layout index
//! that we can read by direct addressing out of ROM:
//!
//! - stream facts the decoder needs (sample rate, channels, bits, block size)
//! - a **frame-offset table** → seeking is `offsets[frame_index]`, an array
//!   lookup with no I/O traits anywhere
//!
//! Parsing is fully borrowed (`&'a [u8]` in, `Manifest<'a>` out) so there is no
//! allocation in the decode path.

/// Magic bytes at the start of a packed blob: "GAFP" (GBA FLAC Pack).
pub const MAGIC: [u8; 4] = *b"GAFP";

/// Manifest format version. Bump on any layout change; decoder rejects mismatch.
pub const VERSION: u8 = 1;

/// Sample rate, resolved from FLAC's 4-bit code table at manifest-parse time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleRate {
    /// Directly encoded via one of the 4-bit table codes (and the manifest
    /// stores the literal Hz value).
    Explicit(u32),
    /// FLAC code `0b0000` — "get from stream". Not a corner case: it is the
    /// *only* way a sample rate outside the 4-bit table can be carried, which
    /// includes 65,536 Hz (measured: all 320 frames of a 65,536 Hz encode use
    /// it). Resolve through [`crate::frame::StreamDefaults`].
    FromStreamDefault,
}

impl SampleRate {
    /// Samples per second, given the stream's default rate.
    pub fn hz(self, stream_default: u32) -> u32 {
        todo!("flac-lite scaffold: SampleRate::hz")
    }

    /// Decode FLAC's 4-bit sample-rate code (frame header, 0b1100..0b1110 range).
    pub fn from_flac_code(code: u8) -> crate::Result<Self> {
        todo!("flac-lite scaffold: SampleRate::from_flac_code")
    }
}

/// Block size (samples per subframe), from FLAC's 4-bit code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocksize {
    /// Single value written after the code (`0b0010` = 8, `0b0110` = get 8-bit,
    /// `0b0111` = get 16-bit).
    Explicit(u16),
    /// Sample rate is unknown at frame time — illegal in our constrained
    /// profile; the packer must always pin an explicit block size.
    Invalid,
}

impl Blocksize {
    /// Number of samples per subframe in this frame.
    pub fn samples(self) -> crate::Result<usize> {
        todo!("flac-lite scaffold: Blocksize::samples")
    }
}

/// Channel assignment, from the 4-bit frame-header field (RFC 9639 9.1.3,
/// Table 16). Codes `0b0010..0b0111` (3–8 channels) and `0b1011..0b1111`
/// (reserved) are outside the profile → [`crate::Error::ProfileViolation`].
///
/// There is **no** swapped/"side is first" mid/side variant in FLAC: the three
/// decorrelation codes are distinct and unambiguous. An earlier draft of this
/// enum carried a `MidSide { side_bit }` flag modelled on assignments "0b101
/// vs 0b110" — those codes are actually 6-channel and 7-channel, and the flag
/// had nothing in the format to set it. Phantom field, phantom branch: removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelConfig {
    /// Independent channel(s), no interchannel decorrelation (assignment
    /// `0b0000` mono / `0b0001` left-right). `channels` is the subframe count.
    Independent {
        /// Number of independently-coded subframes (1 or 2).
        channels: u8,
    },
    /// Mid/side, assignment `0b1010`: subframe 0 is the mid, subframe 1 the side.
    MidSide,
    /// Left/side, assignment `0b1000`: subframe 0 is the left, subframe 1 the side.
    LeftSide,
    /// Side/right, assignment `0b1001`: subframe 0 is the side, subframe 1 the right.
    RightSide,
}

impl ChannelConfig {
    /// Number of subframes in the frame (1 or 2).
    pub fn subframe_count(self) -> u8 {
        todo!("flac-lite scaffold: ChannelConfig::subframe_count")
    }
}

/// A parsed, borrowed manifest. No allocation; every field is either copied out
/// of the header or a subslice of the ROM blob.
#[derive(Clone, Copy, Debug)]
pub struct Manifest<'a> {
    /// Backing blob (header + frame data) — the same `&'static [u8]` the frames
    /// are read from.
    blob: &'a [u8],
    /// Byte offset (from blob start) of the first frame byte.
    frame_data_start: u32,
    /// Frame-offset table: absolute blob offsets, ascending.
    frame_offsets: &'a [u32],
    /// Track sample rate in Hz.
    sample_rate_hz: u32,
    /// Bits per sample (16 in the initial target; 8 accepted for experiments).
    bits_per_sample: u8,
    /// Samples per frame, pinned per track by the packer.
    blocksize: u16,
    /// 1 = mono, 2 = stereo.
    channels: u8,
    /// Total samples in the track.
    total_samples: u64,
    /// Highest FIXED predictor order used by the encoder (0..=4); the packer
    /// records the true maximum so the decoder can report profile conformance.
    max_fixed_order: u8,
}

impl<'a> Manifest<'a> {
    /// Parse and validate a packed blob.
    ///
    /// Validates: magic, version, header length, field sanity (channels ∈ 1..=2,
    /// bps ∈ {8, 16}, blocksize ∈ {1024, 2048} per profile), ascending offsets,
    /// and that every offset lies inside the blob.
    pub fn parse(blob: &'a [u8]) -> crate::Result<Self> {
        todo!("flac-lite scaffold: Manifest::parse")
    }

    /// Total number of frames in the track.
    pub fn frame_count(&self) -> usize {
        todo!("flac-lite scaffold: Manifest::frame_count")
    }

    /// Absolute blob offset of frame `index`. Out-of-range → `Error::Manifest`.
    /// This *is* the seek operation: O(1), no `Seek` trait.
    pub fn frame_offset(&self, index: usize) -> crate::Result<usize> {
        todo!("flac-lite scaffold: Manifest::frame_offset")
    }

    /// The frame bytes for `index`, sliced `[offset .. next_offset]` (last frame
    /// runs to end of blob) — lets the frame decoder detect its own end without
    /// depending on the next frame's header.
    pub fn frame_bytes(&self, index: usize) -> crate::Result<&'a [u8]> {
        todo!("flac-lite scaffold: Manifest::frame_bytes")
    }

    /// Track sample rate in Hz.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Bits per sample (16 in the target profile).
    pub fn bits_per_sample(&self) -> u8 {
        self.bits_per_sample
    }

    /// Samples per subframe, pinned per track.
    pub fn blocksize(&self) -> u16 {
        self.blocksize
    }

    /// Channel count (1 or 2).
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Total samples in the track.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }
}

/// The encode profile the packer guarantees and (optionally) the decoder
/// enforces. Anything outside this set must be rejected at pack time.
///
/// `'static` slices by design: profiles are compiled-in constants (see
/// [`DEFAULT_PROFILE`]), never parsed from the stream — that keeps the type
/// `Copy` and free of lifetime plumbing in a hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeProfile {
    /// Allowed block sizes.
    pub allowed_blocksizes: &'static [u16],
    /// Max FIXED predictor order (0..=4).
    pub max_fixed_order: u8,
    /// Whether full LPC subframes are permitted at all. Target profile says
    /// `false` (encoder runs with `-l 4`); the perf spike decides whether this
    /// becomes permanent.
    pub allow_full_lpc: bool,
    /// Max LPC precision, if LPC is allowed.
    pub max_lpc_precision: u8,
    /// Allowed bits per sample.
    pub allowed_bits_per_sample: &'static [u8],
}

/// The default constrained profile documented in README.md.
pub const DEFAULT_PROFILE: EncodeProfile = EncodeProfile {
    allowed_blocksizes: &[1024, 2048],
    max_fixed_order: 4,
    allow_full_lpc: false,
    max_lpc_precision: 0,
    allowed_bits_per_sample: &[16],
};

impl EncodeProfile {
    /// Check a parsed manifest against this profile.
    pub fn validate(&self, manifest: &Manifest<'_>) -> crate::Result<()> {
        todo!("flac-lite scaffold: EncodeProfile::validate")
    }
}
