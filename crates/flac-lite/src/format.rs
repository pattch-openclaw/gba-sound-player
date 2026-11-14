//! Container format (`GAFP`) + manifest parsing, and the constrained encode
//! profile the packer and decoder agree on.
//!
//! Landed — and the other half of this contract is now a program rather than a
//! promise: `Manifest` parses and validates these bytes, `Blocksize::samples` and
//! `EncodeProfile::validate` are implemented, and the packer side is
//! `scripts/gafp_pack.py` (item 6b-2, 2026-11-11). That generator emits the
//! facts table `tests/gafp_asset_vectors.txt`, which
//! `tests/gafp_asset_facts.rs` reads back through this module and the decode
//! path, so the packer's record of a byte string and this crate's reading of the
//! same bytes are gated against each other rather than assumed consistent. The
//! byte-level contract itself lives in [`../README.md`](../README.md).
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
    ///
    /// [`Blocksize::Invalid`] is the reserved `0b0000` code or an uncommon value
    /// the profile forbids: a frame carrying it is malformed for this decoder,
    /// so it is [`crate::Error::InvalidField`] — the same verdict
    /// `frame::FrameHeader::parse` reaches from the same field.
    pub fn samples(self) -> crate::Result<usize> {
        match self {
            Blocksize::Explicit(samples) => Ok(usize::from(samples)),
            Blocksize::Invalid => Err(crate::Error::InvalidField),
        }
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
///
/// The frame-offset table is kept as **bytes**, not as `&[u32]`, and each
/// accessor decodes its own four little-endian bytes. That is deliberate: the
/// blob is a `&[u8]` with alignment 1, so a `&[u32]` view of it would be an
/// unaligned cast — undefined behaviour, and this crate is
/// `#![deny(unsafe_code)]` and will stay that way. One `u32` decode per seek is
/// the cost, and a seek is one array lookup either way.
#[derive(Clone, Copy, Debug)]
pub struct Manifest<'a> {
    /// Backing blob (header + frame data) — the same `&'static [u8]` the frames
    /// are read from.
    blob: &'a [u8],
    /// Byte offset (from blob start) of the first frame byte.
    frame_data_start: u32,
    /// Frame-offset table, raw little-endian bytes (`4 * frame_count` long,
    /// already length-checked by [`Manifest::parse`]).
    frame_offsets_le: &'a [u8],
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

/// Bytes of a `GAFP` header before the offset table (README.md, `format`
/// section): magic 4, version 1, channels 1, bps 1, max order 1, blocksize 2,
/// sample rate 4, total samples 8, frame count 2, frame-data offset 4.
pub const HEADER_LEN: usize = 0x1C;

impl<'a> Manifest<'a> {
    /// Parse and validate a packed blob.
    ///
    /// Validates: magic, version, header length, field sanity (channels ∈ 1..=2,
    /// bps ∈ {8, 16}, blocksize ≠ 0, FIXED order ≤ 4), ascending offsets, and
    /// that every offset lies inside the blob. The *packer's* contract —
    /// blocksize ∈ {1024, 2048}, 16-bit only — is
    /// [`EncodeProfile::validate`]'s: a standalone decoder should not refuse a
    /// stream because this project's encoder would not have produced it.
    ///
    /// The error split is a decision, not an accident: a property this decoder
    /// cannot handle at all is a *stream* verdict ([`crate::Error::UnsupportedStream`]
    /// / [`crate::Error::UnsupportedSampleSize`]); a property the decoder could
    /// handle but the packer's profile forbids is a *profile* verdict
    /// ([`crate::Error::ProfileViolation`]); anything self-inconsistent
    /// ([`crate::Error::Manifest`]) is a malformed blob.
    pub fn parse(blob: &'a [u8]) -> crate::Result<Self> {
        let header = blob.get(..HEADER_LEN).ok_or(crate::Error::Manifest)?;
        if header[0..4] != MAGIC || header[4] != VERSION {
            return Err(crate::Error::Manifest);
        }
        let channels = header[5];
        let bits_per_sample = header[6];
        let max_fixed_order = header[7];
        let blocksize = u16::from_le_bytes([header[8], header[9]]);
        let sample_rate_hz = u32::from_le_bytes([header[10], header[11], header[12], header[13]]);
        let total_samples = u64::from_le_bytes([
            header[14], header[15], header[16], header[17], header[18], header[19], header[20],
            header[21],
        ]);
        let frame_count = usize::from(u16::from_le_bytes([header[22], header[23]]));
        let frame_data_start =
            u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as usize;

        if !(1..=2).contains(&channels) {
            return Err(crate::Error::UnsupportedStream);
        }
        if bits_per_sample != 8 && bits_per_sample != 16 {
            return Err(crate::Error::UnsupportedSampleSize);
        }
        // Block size is a *format* property here, not a profile one: FLAC allows
        // 1..=65535 and the decoder's buffers are the caller's, so a general
        // manifest parser must not refuse a 4608-block stream merely because
        // this project's packer emits 1024/2048. The profile (`DEFAULT_PROFILE`)
        // is where that contract lives — `EncodeProfile::validate` is the check
        // a product build applies.
        if blocksize == 0 {
            return Err(crate::Error::Manifest);
        }
        if max_fixed_order > 4 {
            return Err(crate::Error::ProfileViolation);
        }
        if sample_rate_hz == 0 {
            return Err(crate::Error::Manifest);
        }

        // The table is `4 * N` bytes and the frame data begins where it ends:
        // the contract states both fields, so a blob that disagrees with itself
        // is refused rather than trusted on one of them.
        let table_end = HEADER_LEN
            .checked_add(frame_count * 4)
            .ok_or(crate::Error::Manifest)?;
        if frame_data_start != table_end || table_end > blob.len() {
            return Err(crate::Error::Manifest);
        }
        let frame_offsets_le = &blob[HEADER_LEN..table_end];

        // Offsets ascending, inside the blob, and the first one *is* the start
        // of the frame data. Frame `i` runs to `i + 1`, so an order violation
        // would make `frame_bytes` a reversed slice; catching it here is what
        // lets every later access be total.
        let mut previous = frame_data_start;
        for index in 0..frame_count {
            let offset = read_u32(frame_offsets_le, index) as usize;
            if index == 0 {
                if offset != frame_data_start {
                    return Err(crate::Error::Manifest);
                }
            } else if offset <= previous {
                return Err(crate::Error::Manifest);
            }
            if offset >= blob.len() && !(index == 0 && frame_count == 1 && offset == blob.len()) {
                return Err(crate::Error::Manifest);
            }
            previous = offset;
        }
        if frame_count == 0 && frame_data_start > blob.len() {
            return Err(crate::Error::Manifest);
        }

        Ok(Manifest {
            blob,
            frame_data_start: frame_data_start as u32,
            frame_offsets_le,
            sample_rate_hz,
            bits_per_sample,
            blocksize,
            channels,
            total_samples,
            max_fixed_order,
        })
    }

    /// Total number of frames in the track.
    pub fn frame_count(&self) -> usize {
        self.frame_offsets_le.len() / 4
    }

    /// Absolute blob offset of frame `index`. Out-of-range → `Error::Manifest`.
    /// This *is* the seek operation: O(1), no `Seek` trait.
    pub fn frame_offset(&self, index: usize) -> crate::Result<usize> {
        if index >= self.frame_count() {
            return Err(crate::Error::Manifest);
        }
        Ok(read_u32(self.frame_offsets_le, index) as usize)
    }

    /// The frame bytes for `index`, sliced `[offset .. next_offset]` (last frame
    /// runs to end of blob) — lets the frame decoder detect its own end without
    /// depending on the next frame's header.
    pub fn frame_bytes(&self, index: usize) -> crate::Result<&'a [u8]> {
        let start = self.frame_offset(index)?;
        let end = if index + 1 < self.frame_count() {
            self.frame_offset(index + 1)?
        } else {
            self.blob.len()
        };
        self.blob.get(start..end).ok_or(crate::Error::Manifest)
    }

    /// The whole frame region: every frame's bytes, contiguous. This is what a
    /// [`crate::scan::Frames`] walk reads when the facade drives the blob.
    ///
    /// Gated with the walk ([`crate::scan`]): a manifest in a build with no walk
    /// has no one to hand a region to, and the accessor would be the crate's only
    /// `format`-side reference to a feature-gated module.
    #[cfg(feature = "scan")]
    pub fn frame_region(&self) -> &'a [u8] {
        &self.blob[self.frame_data_start as usize..]
    }

    /// Blob offset of the first frame byte — the base a walked cursor is
    /// relative to.
    pub fn frame_data_start(&self) -> usize {
        self.frame_data_start as usize
    }

    /// The stream defaults this manifest pins, in the shape the frame layer
    /// consumes. A packed blob has no `STREAMINFO` block, so the manifest *is*
    /// the stream declaration: sample-rate code `0b0000` and sample-size code
    /// `0b0000` in any frame resolve to these values.
    pub fn stream_defaults(&self) -> crate::frame::StreamDefaults {
        crate::frame::StreamDefaults {
            sample_rate_hz: self.sample_rate_hz,
            bits_per_sample: self.bits_per_sample,
        }
    }

    /// What this manifest lets a walk check itself against: a frame count and a
    /// sample count, both exact by construction (the packer wrote them), and no
    /// block-size bound — each frame's own header carries per-frame truth, and
    /// the final frame of a track is legitimately short, so a declared bound
    /// would be a claim the frames do not have to honour.
    #[cfg(feature = "scan")]
    pub fn bounds(&self) -> crate::scan::StreamBounds {
        crate::scan::StreamBounds {
            samples: Some(self.total_samples),
            frames: Some(self.frame_count()),
            max_blocksize: None,
        }
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

    /// Highest FIXED predictor order the packer recorded (0..=4).
    pub fn max_fixed_order(&self) -> u8 {
        self.max_fixed_order
    }

    /// The encode profile a decoder should hold this blob to, as the packer
    /// recorded it: sample rate, block size, bit depth and predictor order come
    /// from the header, so a manifest is validated against itself plus
    /// [`DEFAULT_PROFILE`]'s category rules.
    pub fn profile(&self) -> crate::Result<()> {
        DEFAULT_PROFILE.validate(self)
    }
}

/// Little-endian `u32` at index `i` of a byte table. Total by construction: call
/// sites only ask for indices the table contains ([`Manifest::parse`] proved the
/// length), and a short table cannot reach here.
fn read_u32(table: &[u8], index: usize) -> u32 {
    let at = index * 4;
    u32::from_le_bytes([table[at], table[at + 1], table[at + 2], table[at + 3]])
}

/// The encode profile the packer guarantees and (optionally) the decoder
/// enforces. Anything outside this set must be rejected at pack time.
///
/// `'static` slices by design: profiles are compiled-in constants (see
/// [`DEFAULT_PROFILE`]), never parsed from the stream — that keeps the type
/// `Copy` and free of lifetime plumbing in a hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeProfile {
    /// Sample rates (Hz) a manifest may declare.
    ///
    /// This clause was missing until plan item 7-0 (Q14, ruled 2026-11-11: a
    /// stream is encoded at the rate it is played at). It belongs in the profile
    /// rather than only in the packer and in one example's player because the
    /// profile is the single object every path that validates a manifest reads —
    /// [`Manifest::profile`], `Decoder::new` under the `strict-profile` feature,
    /// and the product ROM's start-up check. A build that plays at one clock and
    /// is handed a stream that declares another has only two options, and both are
    /// wrong: reinterpret the declared rate to fit the timer (1.024x, i.e. 41.06
    /// cents of pitch — the defect Q14 was ruled on), or play it at the declared
    /// rate and change the sample clock of the whole mixer. Refusing it is the
    /// third option, and it is the one that costs nothing downstream.
    ///
    /// Note what this is *not*: not a lookup, and not a cost key. Cost is keyed
    /// structurally in `scripts/gafp_pack.py`'s `MEASURED_DECODE`, by **block
    /// size and predictor kind/order**; the residual envelope a row was measured
    /// on travels as a stated caveat on that row rather than as a key component,
    /// which is why the packer says out loud when it falls back to the nearest
    /// shape. The declared rate is a validity check asked before any cost
    /// question, because a stream at a rate this build does not play at has no
    /// frame period for a cost figure to be a percentage *of*.
    pub allowed_sample_rates_hz: &'static [u32],
    /// Allowed block sizes.
    pub allowed_blocksizes: &'static [u16],
    /// Max FIXED predictor order (0..=4).
    pub max_fixed_order: u8,
    /// Whether full LPC subframes are permitted at all. The target profile's
    /// reference encode is `-l 4` (which emits LPC), so the *order* cap — not
    /// this category bit — is the real guard (README "Correction 3"); the
    /// packer's subframe census is what enforces it at product time.
    pub allow_full_lpc: bool,
    /// Max LPC precision, if LPC is allowed.
    pub max_lpc_precision: u8,
    /// Allowed bits per sample.
    pub allowed_bits_per_sample: &'static [u8],
}

/// The one rate the product path plays at, spelled here as well as in
/// `examples/flac_integration/src/geometry.rs::PLAYBACK_RATE_HZ` (16,384 Hz,
/// `2^24 / 1024`). They are one fact in two crates, and the example's own rate
/// guard asserts they stay equal — a crate-side profile naming a different rate
/// from the player would have the ROM refuse its own asset at start-up.
pub const PRODUCT_SAMPLE_RATE_HZ: u32 = 16_384;

/// The default constrained profile documented in README.md.
pub const DEFAULT_PROFILE: EncodeProfile = EncodeProfile {
    allowed_sample_rates_hz: &[PRODUCT_SAMPLE_RATE_HZ],
    allowed_blocksizes: &[1024, 2048],
    max_fixed_order: 4,
    allow_full_lpc: false,
    max_lpc_precision: 0,
    allowed_bits_per_sample: &[16],
};

impl EncodeProfile {
    /// Check a parsed manifest against this profile.
    ///
    /// What a *manifest* can decide, and what it cannot, is worth stating: the
    /// declared sample rate, block size, bit depth and the packer's recorded
    /// maximum FIXED order are header fields, so they are checked here. The rate
    /// is checked **first**, and not because it is cheaper: a stream at a rate
    /// this build does not play at has no frame period for any other clause's
    /// budget to be stated against, so every other answer about it would be a
    /// number in the wrong units. What this check cannot see is the frame layer:
    /// a frame may carry its own 4-bit rate code and disagree with its own
    /// `STREAMINFO` — the 6b-3 respell trap — and only a walk of the frames sees
    /// that (`scripts/gafp_pack.py`'s frame census refuses it at pack time).
    ///
    /// Whether the stream contains
    /// **LPC subframes** — the distinction `allow_full_lpc` names, and the one
    /// the 32 kHz target depends on, since LPC is 174.14% of that frame on
    /// hardware — is *not* in the manifest: it lives in the subframes. That is a
    /// pack-time scan of the frames (6b-2's `EncodeProfile` enforcement on real
    /// assets), not something a borrowed header can answer. `#[strict-profile]`
    /// keeps the per-frame rejection where it already is, in `subframe`.
    pub fn validate(&self, manifest: &Manifest<'_>) -> crate::Result<()> {
        if !self
            .allowed_sample_rates_hz
            .contains(&manifest.sample_rate_hz())
        {
            return Err(crate::Error::ProfileViolation);
        }
        if !self.allowed_blocksizes.contains(&manifest.blocksize()) {
            return Err(crate::Error::ProfileViolation);
        }
        if !self
            .allowed_bits_per_sample
            .contains(&manifest.bits_per_sample())
        {
            return Err(crate::Error::ProfileViolation);
        }
        if manifest.max_fixed_order() > self.max_fixed_order {
            return Err(crate::Error::ProfileViolation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod manifest_tests {
    //! The parser's refusal table and one happy path.
    //!
    //! Every case is a blob the packer must never emit, and the *verdict* is the
    //! point: a property the decoder cannot handle is a stream error, a property
    //! the profile forbids is a profile error, and a self-inconsistent header is
    //! a manifest error. A test that only asserted "is an Err" would let those
    //! three drift into each other.
    use super::*;
    use crate::Error;

    /// Build a blob. `offsets` are absolute; the table therefore ends at
    /// `HEADER_LEN + 4 * offsets.len()`, which the caller may override to make a
    /// blob disagree with itself.
    fn blob(
        channels: u8,
        bps: u8,
        blocksize: u16,
        order: u8,
        offsets: &[u32],
        frame_data_start: u32,
        bytes: usize,
    ) -> [u8; 64] {
        let mut b = [0u8; 64];
        b[0..4].copy_from_slice(&MAGIC);
        b[4] = VERSION;
        b[5] = channels;
        b[6] = bps;
        b[7] = order;
        b[8..10].copy_from_slice(&blocksize.to_le_bytes());
        b[10..14].copy_from_slice(&PRODUCT_SAMPLE_RATE_HZ.to_le_bytes());
        b[14..22].copy_from_slice(&(blocksize as u64 * offsets.len() as u64).to_le_bytes());
        b[22..24].copy_from_slice(&(offsets.len() as u16).to_le_bytes());
        b[24..28].copy_from_slice(&frame_data_start.to_le_bytes());
        for (i, off) in offsets.iter().enumerate() {
            b[HEADER_LEN + i * 4..HEADER_LEN + i * 4 + 4].copy_from_slice(&off.to_le_bytes());
        }
        let _ = bytes;
        b
    }

    fn good() -> [u8; 64] {
        // 2 frames, table ends at 36, frames are 36..40 and 40..44.
        blob(2, 16, 2048, 4, &[36, 40], 36, 44)
    }

    #[test]
    fn parses_a_well_formed_blob_and_slices_its_frames() {
        let b = good();
        let m = Manifest::parse(&b[..44]).expect("valid");
        assert_eq!(m.frame_count(), 2);
        assert_eq!(m.frame_data_start(), HEADER_LEN + 8);
        assert_eq!(m.frame_offset(0), Ok(36));
        assert_eq!(m.frame_offset(1), Ok(40));
        assert_eq!(m.frame_bytes(0).map(<[u8]>::len), Ok(4));
        assert_eq!(m.frame_bytes(1).map(<[u8]>::len), Ok(4), "last runs to end");
        assert_eq!(m.channels(), 2);
        assert_eq!(m.bits_per_sample(), 16);
        assert_eq!(m.blocksize(), 2048);
        assert_eq!(m.sample_rate_hz(), PRODUCT_SAMPLE_RATE_HZ);
        assert_eq!(m.max_fixed_order(), 4);
        assert_eq!(m.total_samples(), 4096);
        let defaults = m.stream_defaults();
        assert_eq!(defaults.sample_rate_hz, PRODUCT_SAMPLE_RATE_HZ);
        assert_eq!(defaults.bits_per_sample, 16);
        assert_eq!(m.frame_offset(2), Err(Error::Manifest), "out of range");
    }

    #[test]
    fn refuses_magic_and_version() {
        let mut b = good();
        b[0] = b'X';
        assert_eq!(Manifest::parse(&b[..44]).err(), Some(Error::Manifest));
        let mut b = good();
        b[4] = VERSION + 1;
        assert_eq!(Manifest::parse(&b[..44]).err(), Some(Error::Manifest));
    }

    #[test]
    fn refuses_a_header_that_disagrees_with_its_own_table() {
        // frame_data_start says 40 while the table ends at 36.
        let b = blob(2, 16, 2048, 4, &[40, 44], 40, 48);
        assert_eq!(Manifest::parse(&b[..48]).err(), Some(Error::Manifest));
        // Table runs past the blob.
        let b = good();
        assert_eq!(Manifest::parse(&b[..20]).err(), Some(Error::Manifest));
    }

    #[test]
    fn refuses_offsets_that_are_not_ascending_or_not_first() {
        // Second offset does not advance.
        let b = blob(2, 16, 2048, 4, &[36, 36], 36, 44);
        assert_eq!(Manifest::parse(&b[..44]).err(), Some(Error::Manifest));
        // First offset is not the frame-data start.
        let b = blob(2, 16, 2048, 4, &[40, 44], 36, 48);
        assert_eq!(Manifest::parse(&b[..48]).err(), Some(Error::Manifest));
        // Offset outside the blob.
        let b = blob(2, 16, 2048, 4, &[36, 99], 36, 44);
        assert_eq!(Manifest::parse(&b[..44]).err(), Some(Error::Manifest));
    }

    #[test]
    fn the_three_refusal_kinds_stay_distinct() {
        // What the decoder cannot do at all.
        let b = blob(3, 16, 2048, 4, &[36], 32, 40);
        assert_eq!(
            Manifest::parse(&b[..40]).err(),
            Some(Error::UnsupportedStream)
        );
        let b = blob(1, 12, 2048, 4, &[32], 32, 40);
        assert_eq!(
            Manifest::parse(&b[..40]).err(),
            Some(Error::UnsupportedSampleSize)
        );
        // A block size the packer would not emit parses (it is legal FLAC); the
        // *profile* is what refuses it. Zero is not a block size at all.
        let b = blob(1, 16, 512, 4, &[32], 32, 40);
        let m = Manifest::parse(&b[..40]).expect("512 is a legal FLAC block size");
        assert_eq!(DEFAULT_PROFILE.validate(&m), Err(Error::ProfileViolation));
        let b = blob(1, 16, 0, 4, &[32], 32, 40);
        assert_eq!(Manifest::parse(&b[..40]).err(), Some(Error::Manifest));
        let b = blob(1, 16, 2048, 5, &[32], 32, 40);
        assert_eq!(
            Manifest::parse(&b[..40]).err(),
            Some(Error::ProfileViolation)
        );
    }

    #[test]
    fn blocksize_samples_resolves_or_refuses() {
        assert_eq!(Blocksize::Explicit(2048).samples(), Ok(2048));
        assert_eq!(Blocksize::Explicit(1024).samples(), Ok(1024));
        assert_eq!(Blocksize::Invalid.samples(), Err(Error::InvalidField));
    }

    #[test]
    fn the_default_profile_accepts_the_contract_and_refuses_outside_it() {
        let b = good();
        let m = Manifest::parse(&b[..44]).expect("valid");
        assert_eq!(DEFAULT_PROFILE.validate(&m), Ok(()));

        let b = blob(2, 16, 1024, 4, &[32], 32, 40);
        let m = Manifest::parse(&b[..40]).expect("1024 is allowed");
        assert_eq!(DEFAULT_PROFILE.validate(&m), Ok(()));

        // 8-bit is outside the default profile even though the decoder accepts
        // it as a stream property.
        let b = blob(2, 8, 2048, 4, &[32], 32, 40);
        let m = Manifest::parse(&b[..40]).expect("8-bit parses");
        assert_eq!(DEFAULT_PROFILE.validate(&m), Err(Error::ProfileViolation));
    }

    /// The rate clause plan item 7-0 added to the profile (Q14, ruled
    /// 2026-11-11: a stream is encoded at the rate it is played at).
    ///
    /// The failure mode this pins is the one every sample-level digest in this
    /// repository is blind to: a 32,000 Hz asset and a 32,768 Hz asset are the
    /// same samples, so a byte-pinned run of the first one passes with the pitch
    /// 41.06 cents sharp. The retarget moves the target to 16,384 Hz, but the
    /// same 1.024x trap now arrives as 16,000 Hz against 16,384 Hz. The *profile* is where that is refused, because a build
    /// has one mixer clock and a manifest that declares another rate has no frame
    /// period for any figure about it to be stated against.
    #[test]
    fn the_profile_refuses_a_stream_it_is_not_built_to_play() {
        // 32,000 Hz is legal in every other sense. It is FLAC's own `0b1000`
        // rate code (`sample_rate_code_table` above), it parses into a
        // well-formed manifest, and the decoder itself has no opinion about it.
        let mut b = good();
        b[10..14].copy_from_slice(&32_000u32.to_le_bytes());
        let m = Manifest::parse(&b[..44]).expect("a 32,000 Hz manifest is well-formed");
        assert_eq!(m.sample_rate_hz(), 32_000);
        assert_eq!(
            DEFAULT_PROFILE.validate(&m),
            Err(Error::ProfileViolation),
            "the profile, not the parser, is what refuses it"
        );

        // …so it is the profile clause doing the work, and not some other check
        // that happens to fire on the same blob: widen only that clause and the
        // same manifest is accepted.
        let wider = EncodeProfile {
            allowed_sample_rates_hz: &[PRODUCT_SAMPLE_RATE_HZ, 32_000],
            ..DEFAULT_PROFILE
        };
        assert_eq!(
            wider.validate(&m),
            Ok(()),
            "a profile that names the rate accepts the stream — the refusal above \
             must be the clause, not an accident of the fixture"
        );

        // The rates a future edit might be tempted to add, all refused: the FLAC
        // table rates near the target, one hertz off the target (an off-by-one in
        // a profile literal), and 32,768 Hz, which is the rung this retarget moved
        // away from — it divides the GBA clock exactly, and Q14's rule is about
        // the declared rate, not about which division happens to be exact.
        for rate in [
            8_000, 16_000, 22_050, 24_000, 31_000, 32_001, 32_767, 32_768, 44_100, 48_000,
        ] {
            let mut b = good();
            b[10..14].copy_from_slice(&u32::to_le_bytes(rate));
            let m = Manifest::parse(&b[..44]).unwrap_or_else(|e| panic!("{rate} Hz parses: {e:?}"));
            assert_eq!(
                DEFAULT_PROFILE.validate(&m),
                Err(Error::ProfileViolation),
                "{rate} Hz is not the rate this profile was built for"
            );
        }

        // The profile states exactly one rate, and the header's zero-rate guard
        // is a *manifest* verdict, not a profile one: a blob with no rate at all
        // is malformed, which is a different error and stays that way.
        assert_eq!(
            DEFAULT_PROFILE.allowed_sample_rates_hz,
            [PRODUCT_SAMPLE_RATE_HZ],
            "one clock, stated once"
        );
        let mut b = good();
        b[10..14].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            Manifest::parse(&b[..44]).err(),
            Some(Error::Manifest),
            "0 Hz is not a rate, and refuses as a malformed blob"
        );
    }
}
