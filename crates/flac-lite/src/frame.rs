//! Frame layer: header parse + one-frame decode driver.
//!
//! STATE (2026-09-08): [`FrameHeader::parse`] is **implemented** (Phase 1
//! step 2), validated against the golden vectors in `tests/` (real libFLAC
//! 1.5.0 bytes). `decode_frame` below it is still `todo!()` scaffold — the
//! subframe/residual math is step 3.
//!
//! Frame layout (RFC 9639 §9.1). Field widths below are **measured against real
//! libFLAC output**, not transcribed from memory — see the byte-level breakdown
//! and the correction note in [`../../../FLAC.md`](../../../FLAC.md) → "Frame
//! header: measured byte layout".
//!
//! ```text
//! fixed fields — 32 bits total:
//!   [sync 14 = 0b11111111111110][reserved 1 = 0][blocking strategy 1]
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
//! As-built notes (were "notes for the implementation" before step 2 landed):
//!
//! - **Sync.** RFC 9639 §9.1 defines a *15-bit* sync `0b111111111111100`
//!   followed by the blocking strategy bit, so the first two bytes of a frame
//!   are `0xFFF8` (fixed blocksize) or `0xFFF9` (variable). Read as 14 bits,
//!   the sync is `0x3FFE` for **both** strategies — the strategy is the 16th
//!   bit, not part of the sync. (An earlier draft of these module docs called
//!   `0b11111111111111` "the variable-blocksize sync code"; it is not — that
//!   14-bit pattern is not a sync at all. Caught while implementing `parse`,
//!   against §9.1 and the oracle.) A 14-bit read of anything else →
//!   [`crate::Error::FrameSync`], the resync/recovery signal; a variable
//!   blocking strategy bit → [`crate::Error::ProfileViolation`] (our profile
//!   is fixed-blocksize only; the variable path stays unimplemented).
//! - **Uncommon fields are real and load-bearing.** The blocksize codes
//!   `0b0110`/`0b0111` and sample-rate codes `0b1100..=0b1110` append an
//!   8- or 16-bit value *after the coded number* (RFC §9.1.6/§9.1.7 — never
//!   24-bit; an earlier scaffold note said `8 | 16 | 24` and had no witness).
//!   `parse` reads them at the right cursor positions, because a header whose
//!   tail it does not fully consume strands the reader before the CRC-8 and
//!   the subframe data. Measured (libFLAC 1.5.0): a final frame of 1512
//!   samples emits blocksize code `0b0111` + 16-bit value, one of 100 samples
//!   `0b0110` + 8-bit; 56000 Hz emits `0b1100` + 8-bit kHz, 10010 Hz
//!   `0b1110` + 16-bit Hz÷10, 48001 Hz `0b1101` + 16-bit Hz. Golden vectors
//!   cover all five.
//! - **CRC-8 is consumed, not verified.** Verification is gated behind the
//!   `crc-check` feature and Phase 2 step 5 (the `crc8` implementation); the
//!   perf gate runs with it skipped, which the design allows (checked in
//!   debug, skippable in release).
//! - **"From stream" codes resolve through [`StreamDefaults`]** — sample-rate
//!   code `0b0000` and sample-size code `0b000` are *not* errors, and the
//!   rate path is where the whole 65 kHz goal lives (see [`StreamDefaults`]).
//! - **Error taxonomy** (applied consistently below, and matching the
//!   [`crate::Error`] variant docs): a *spec-invalid* value — reserved bit or
//!   code, forbidden rate code, over-large frame number, zero uncommon rate —
//!   is [`crate::Error::InvalidField`]; spec-valid-but-outside-our-profile
//!   (3–8 channel layouts, variable blocking) is
//!   [`crate::Error::ProfileViolation`]; valid FLAC bit depths the decoder
//!   does not support (12/20/24/32-bit) are
//!   [`crate::Error::UnsupportedSampleSize`].
//! - **Checks deliberately deferred, with their homes:**
//!   * uncommon blocksize values 1..=15 are legal **only** on the final frame
//!     (§9.1.6) — "final" is not knowable from a header alone, so the decoder
//!     loop (Phase 2) or the packer owns it.
//!   * strict-profile blocksize gating (`allowed_blocksizes`) belongs to
//!     [`crate::format::EncodeProfile::validate`] on the *manifest*, not here:
//!     the final frame's short blocksize is legal by construction and looks
//!     like a violation to any frame-local check.
//!   * predictor-order gating (`-l N`) lands with step 3 in the subframe
//!     parser, where the order field is.

use crate::Error;
use crate::bits::BitReader;
use crate::format::{ChannelConfig, SampleRate};
use crate::subframe::PredictorState;

/// The 14-bit frame sync shared by both blocking strategies (see module
/// docs: RFC 9639 §9.1 — 15-bit sync `0b111111111111100` + strategy bit).
const SYNC_14: u32 = 0x3FFE;

/// Stream-level facts a frame header cannot carry by itself.
///
/// FLAC deliberately omits values from frames when they match the stream
/// default: sample-rate code `0b000` and sample-size code `0b000` both mean
/// "look it up in STREAMINFO". Our container has no STREAMINFO (the GAFP
/// manifest replaces it), so [`FrameHeader::parse`] needs these two values
/// handed in.
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
    /// manifest's block size. Resolved from the 4-bit code, including the
    /// uncommon `+1` forms — see [`FrameHeader::parse`].
    pub blocksize: usize,
    /// Sample rate, unresolved code preserved; resolve with
    /// [`StreamDefaults::sample_rate_hz`] via [`SampleRate::hz`]. The three
    /// uncommon codes arrive here already `Explicit` (their value follows the
    /// coded number and `parse` reads it).
    pub sample_rate: SampleRate,
    /// Channel assignment, including decorrelation mode.
    pub channels: ChannelConfig,
    /// Bits per sample for the primary subframe, already resolved through
    /// [`StreamDefaults`] (code `0b000` → stream default; reserved code
    /// `0b011` → [`crate::Error::InvalidField`]).
    pub bits_per_sample: u8,
    /// UTF-8 coded frame number (or sample number under variable blocking,
    /// which our profile rejects before this field is read). Consumed so the
    /// cursor lands on the subframe data; our seek never uses the value (the
    /// manifest's offset table is authoritative).
    pub number: u64,
}

/// Which blocksize a 4-bit code denotes: a table size, or an uncommon value
/// appended after the coded number (§9.1.6). Two-phase because the *code* is
/// read before the *value* exists at the cursor.
enum BlocksizeCode {
    Table(u16),
    Uncommon8,
    Uncommon16,
}

/// Block size bits → table size, RFC 9639 Table 14, with the two uncommon
/// codes classified for later resolution. Division-free throughout (ARMv4T):
/// `144·2^v` and `2^v` are just shifts by the code value.
fn blocksize_code(code: u8) -> crate::Result<BlocksizeCode> {
    match code {
        0b0000 => Err(Error::InvalidField), // reserved
        0b0001 => Ok(BlocksizeCode::Table(192)),
        // 0b0010..=0b0101: 144 * 2^v  (576, 1152, 2304, 4608)
        0b0010..=0b0101 => Ok(BlocksizeCode::Table((144u32 << code) as u16)),
        0b0110 => Ok(BlocksizeCode::Uncommon8), // value + 1, 8-bit
        0b0111 => Ok(BlocksizeCode::Uncommon16), // value + 1, 16-bit
        // 0b1000..=0b1111: 2^v  (256..=32768; MAX_BLOCKSIZE is this top end)
        0b1000..=0b1111 => Ok(BlocksizeCode::Table((1u32 << code as u32) as u16)),
        _ => Err(Error::InvalidField), // unreachable for 4-bit input
    }
}

/// Channel assignment bits → config, RFC 9639 Table 16. Split exactly along
/// the profile/spec fault line (see module docs' error taxonomy): 3–8 channel
/// layouts are valid FLAC but outside our 1/2-channel profile →
/// `ProfileViolation`; reserved codes `0b1011..=0b1111` are not valid FLAC →
/// `InvalidField`.
fn channel_config(code: u8) -> crate::Result<ChannelConfig> {
    match code {
        0b0000 => Ok(ChannelConfig::Independent { channels: 1 }),
        0b0001 => Ok(ChannelConfig::Independent { channels: 2 }),
        0b0010..=0b0111 => Err(Error::ProfileViolation), // 3..8 channels
        0b1000 => Ok(ChannelConfig::LeftSide),
        0b1001 => Ok(ChannelConfig::RightSide),
        0b1010 => Ok(ChannelConfig::MidSide),
        _ => Err(Error::InvalidField), // 0b1011..=0b1111 reserved
    }
}

/// Bit depth bits → resolved bits per sample, RFC 9639 Table 17. Code
/// `0b000` resolves through the stream default (and the default itself must
/// be a depth this decoder supports — a caller handing in 24 is a config
/// error surfaced as a stream error, which is the closest honest mapping the
/// flat error enum offers).
fn bits_per_sample(code: u8, defaults: &StreamDefaults) -> crate::Result<u8> {
    match code {
        0b000 => match defaults.bits_per_sample {
            8 | 16 => Ok(defaults.bits_per_sample),
            _ => Err(Error::UnsupportedSampleSize),
        },
        0b001 => Ok(8),
        0b100 => Ok(16),
        0b011 => Err(Error::InvalidField), // reserved code
        // 12/20/24/32-bit: valid FLAC, not what this decoder carries.
        _ => Err(Error::UnsupportedSampleSize),
    }
}

/// Sample rate bits → resolved [`SampleRate`], RFC 9639 Table 15. The three
/// uncommon codes carry their value *after the coded number* (§9.1.7) in
/// specific units — kHz (8-bit), Hz (16-bit), Hz÷10 (16-bit) — so they resolve
/// here, not in `SampleRate::from_flac_code`. The multipliers are cheap:
/// ARM7TDMI has a hardware `MUL` (the no-divide rule is about `SDIV`-less
/// division), and this runs once per header. A value of 0 means "no audio"
/// (§9.1.7) and is refused — every stream we pack is audio.
fn sample_rate(code: u8, reader: &mut BitReader<'_>) -> crate::Result<SampleRate> {
    let hz = match code {
        0b1100 => {
            let khz = reader.read_u8()? as u32;
            if khz == 0 {
                return Err(Error::InvalidField);
            }
            khz * 1000
        }
        0b1101 => {
            let hz = reader.read_bits(16)?;
            if hz == 0 {
                return Err(Error::InvalidField);
            }
            return Ok(SampleRate::Explicit(hz));
        }
        0b1110 => {
            let hz10 = reader.read_bits(16)?;
            if hz10 == 0 {
                return Err(Error::InvalidField);
            }
            hz10 * 10
        }
        // Self-contained codes: 0b0000 → FromStreamDefault, the eleven table
        // rates → Explicit, 0b1111 (forbidden) → InvalidField.
        _ => return SampleRate::from_flac_code(code),
    };
    Ok(SampleRate::Explicit(hz))
}

impl FrameHeader {
    /// Read a frame header. `reader` must be positioned at the frame's first
    /// byte (a manifest offset); a cursor that is not byte-aligned is refused
    /// with [`crate::Error::FrameSync`] — RFC 9639 §9.1: "Each frame MUST
    /// start on a byte boundary", so a mid-byte cursor means "no frame here"
    /// just as surely as a bad sync does. `defaults` resolves the "from
    /// stream" codes; see [`StreamDefaults`].
    ///
    /// Validates every field, and rejects *values* outside the encode profile
    /// — the subset lives in what is accepted, never in what is parsed. A
    /// header is 6–9 bytes; skipping a field saves nothing measurable, while a
    /// partial parse mispositions the cursor and surfaces as corrupt samples
    /// three modules away. On success the cursor sits exactly on the first
    /// subframe bit; on `Err` it may sit part-way through the header, and the
    /// caller must treat the frame as undecodable (the manifest's offsets stay
    /// authoritative — re-seek, do not continue).
    ///
    /// Field order walks RFC §9.1.1–9.1.8 exactly; the two-phase blocksize
    /// resolution (code now, uncommon value after the coded number) is the
    /// only reordering the format forces.
    pub fn parse(reader: &mut BitReader<'_>, defaults: &StreamDefaults) -> crate::Result<Self> {
        if reader.bit_position() % 8 != 0 {
            return Err(Error::FrameSync);
        }

        // --- fixed fields: 14 + 1 + 1 + 4 + 4 + 4 + 3 + 1 = 32 bits --------
        let sync = reader.read_bits(14)?;
        if sync != SYNC_14 {
            return Err(Error::FrameSync);
        }
        if reader.read_bits(1)? != 0 {
            return Err(Error::InvalidField); // reserved bit must be zero
        }
        if reader.read_bits(1)? != 0 {
            return Err(Error::ProfileViolation); // variable blocksize: off-profile
        }
        let bs_bits = reader.read_bits(4)? as u8;
        let rate_code = reader.read_bits(4)? as u8;
        let channels_code = reader.read_bits(4)? as u8;
        let size_code = reader.read_bits(3)? as u8;
        if reader.read_bits(1)? != 0 {
            return Err(Error::InvalidField); // reserved bit must be zero
        }
        debug_assert_eq!(reader.bit_position() % 8, 0, "fixed fields are 32 bits");

        // Validate what is fully determined by the fixed fields. Doing this
        // before consuming the post-number bytes keeps garbage headers cheap
        // to reject; it does not change any cursor outcome the caller can act
        // on (errors end the frame either way).
        let channels = channel_config(channels_code)?;
        let bits_per_sample = bits_per_sample(size_code, defaults)?;
        let bs_code = blocksize_code(bs_bits)?;

        // --- coded frame number: whole octets, cursor stays byte-aligned ----
        let number = reader.read_utf8_coded()?;
        // Fixed-blocksize streams carry a *frame* number, which §9.1.5 caps at
        // 31 bits ("MUST NOT be larger than what fits a value of 31 bits").
        if number > 0x7FFF_FFFF {
            return Err(Error::InvalidField);
        }

        // --- uncommon blocksize, after the coded number (§9.1.6) ------------
        // The stored value is blocksize-minus-1. 65535 (→ 65536) is forbidden
        // outright; values 1..=15 are final-frame-only, which a frame-local
        // parse cannot judge (module docs: deferred to the decoder/packer).
        let blocksize = match bs_code {
            BlocksizeCode::Table(n) => n as usize,
            BlocksizeCode::Uncommon8 => reader.read_u8()? as usize + 1,
            BlocksizeCode::Uncommon16 => {
                let stored = reader.read_bits(16)?;
                if stored == 0xFFFF {
                    return Err(Error::InvalidField);
                }
                stored as usize + 1
            }
        };

        // --- uncommon sample rate, after the uncommon blocksize (§9.1.7) ----
        let sample_rate = sample_rate(rate_code, reader)?;

        // --- CRC-8: consumed here, verified never (Phase 2 step 5) ----------
        // Always byte-aligned (module docs); `read_u8` is bit-level anyway, so
        // no byte_align call — writing one would assert a falsehood.
        let _crc8 = reader.read_u8()?;

        Ok(FrameHeader {
            blocksize,
            sample_rate,
            channels,
            bits_per_sample,
            number,
        })
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
///
/// SCAFFOLD STATE: step 3 (FIXED + Rice, then the LPC decision the perf gate
/// makes). Callers that only need header facts use [`FrameHeader::parse`].
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
