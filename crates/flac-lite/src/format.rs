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
    ///
    /// `Explicit` ignores the default entirely — a frame that names its rate
    /// (table code or uncommon value) never consults the stream, which is what
    /// lets the perf spike hand-write `StreamDefaults` without corrupting
    /// explicit-rate frames. Only [`SampleRate::FromStreamDefault`] consults it.
    pub fn hz(self, stream_default: u32) -> u32 {
        match self {
            SampleRate::Explicit(hz) => hz,
            SampleRate::FromStreamDefault => stream_default,
        }
    }

    /// Decode FLAC's 4-bit sample-rate code (RFC 9639 Table 15) for the codes
    /// that are **self-contained**: `0b0000` → [`SampleRate::FromStreamDefault`],
    /// the eleven table rates → [`SampleRate::Explicit`], `0b1111` (forbidden)
    /// → [`crate::Error::InvalidField`].
    ///
    /// The three *uncommon* codes `0b1100..=0b1110` carry their value **after
    /// the coded number** (8-bit kHz, 16-bit Hz, 16-bit Hz÷10 respectively —
    /// never 24-bit; measured against libFLAC 1.5.0, see FLAC.md), so the code
    /// alone cannot resolve them and this function refuses them with
    /// [`crate::Error::InvalidField`]. That rejection is a *domain* boundary,
    /// not a stream verdict: `FrameHeader::parse` reads the appended value at
    /// the right cursor position and constructs `Explicit` directly. Pinned by
    /// `format_refuses_codes_that_carry_a_value` below.
    pub fn from_flac_code(code: u8) -> crate::Result<Self> {
        // Table 15 in full; every arm is a constant, no division (ARMv4T).
        let hz = match code {
            0b0000 => return Ok(SampleRate::FromStreamDefault),
            0b0001 => 88_200,
            0b0010 => 176_400,
            0b0011 => 192_000,
            0b0100 => 8_000,
            0b0101 => 16_000,
            0b0110 => 22_050,
            0b0111 => 24_000,
            0b1000 => 32_000,
            0b1001 => 44_100,
            0b1010 => 48_000,
            0b1011 => 96_000,
            // 0b1100..=0b1110: value lives after the coded number — see docs.
            0b1100..=0b1110 => return Err(crate::Error::InvalidField),
            // 0b1111: forbidden by the spec.
            _ => return Err(crate::Error::InvalidField),
        };
        Ok(SampleRate::Explicit(hz))
    }
}

/// Block size (samples per subframe), from FLAC's 4-bit code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocksize {
    /// A concrete sample count, resolved from a table code or an uncommon
    /// ("get 8/16-bit") value.
    Explicit(u16),
    /// Reserved code `0b0000`, or an uncommon value the profile forbids. The
    /// manifest must never carry one; frame-side resolution lives in
    /// [`crate::frame`] (the frame header's blocksize is resolved per frame,
    /// since the final frame is legitimately short).
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
    ///
    /// Every variant the type can hold is 1 or 2 — 3–8-channel assignments
    /// never become a `ChannelConfig` ([`crate::Error::ProfileViolation`]),
    /// which is what keeps this total without an error path.
    pub fn subframe_count(self) -> u8 {
        match self {
            ChannelConfig::Independent { channels } => channels,
            ChannelConfig::MidSide | ChannelConfig::LeftSide | ChannelConfig::RightSide => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn from_flac_code_covers_the_self_contained_table() {
        // RFC 9639 Table 15, every self-contained row. The r65k frame path
        // (code 0b0000) is asserted here as well as in the golden vectors.
        let cases: &[(u8, SampleRate)] = &[
            (0b0000, SampleRate::FromStreamDefault),
            (0b0001, SampleRate::Explicit(88_200)),
            (0b0010, SampleRate::Explicit(176_400)),
            (0b0011, SampleRate::Explicit(192_000)),
            (0b0100, SampleRate::Explicit(8_000)),
            (0b0101, SampleRate::Explicit(16_000)),
            (0b0110, SampleRate::Explicit(22_050)),
            (0b0111, SampleRate::Explicit(24_000)),
            (0b1000, SampleRate::Explicit(32_000)),
            (0b1001, SampleRate::Explicit(44_100)),
            (0b1010, SampleRate::Explicit(48_000)),
            (0b1011, SampleRate::Explicit(96_000)),
        ];
        for &(code, want) in cases {
            assert_eq!(
                SampleRate::from_flac_code(code),
                Ok(want),
                "code {code:#06b}"
            );
        }
    }

    #[test]
    fn format_refuses_codes_that_carry_a_value() {
        // The domain boundary, pinned: 0b1100..=0b1110 are legal FLAC codes but
        // NOT resolvable from the code alone (their value follows the coded
        // number), and 0b1111 is forbidden. `FrameHeader::parse` never calls
        // this function for the first range — it reads the appended value and
        // builds `Explicit` itself.
        for code in 0b1100u8..=0b1111 {
            assert_eq!(
                SampleRate::from_flac_code(code),
                Err(Error::InvalidField),
                "code {code:#06b} must not resolve code-only"
            );
        }
    }

    #[test]
    fn hz_only_consults_the_default_for_from_stream() {
        // Explicit ignores the stream default entirely (spike-safe hand-written
        // defaults), FromStreamDefault is exactly the default.
        assert_eq!(SampleRate::Explicit(32_000).hz(65_536), 32_000);
        assert_eq!(SampleRate::Explicit(56_000).hz(999), 56_000);
        assert_eq!(SampleRate::FromStreamDefault.hz(65_536), 65_536);
    }

    #[test]
    fn subframe_count_matches_the_channel_table() {
        assert_eq!(
            ChannelConfig::Independent { channels: 1 }.subframe_count(),
            1
        );
        assert_eq!(
            ChannelConfig::Independent { channels: 2 }.subframe_count(),
            2
        );
        for cfg in [
            ChannelConfig::MidSide,
            ChannelConfig::LeftSide,
            ChannelConfig::RightSide,
        ] {
            assert_eq!(cfg.subframe_count(), 2, "{cfg:?}");
        }
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
