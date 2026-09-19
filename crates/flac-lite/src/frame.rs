//! Frame layer: header parse + one-frame decode driver.
//!
//! STATE (2026-09-17): [`FrameHeader::parse`] (step 2) and [`decode_frame`]
//! (step 3f) are both **implemented** and validated against real libFLAC
//! 1.5.0 bytes in `tests/` — header vectors, and the frame-RUN table
//! (`tests/frame_run_vectors.txt`) for whole-frame chaining + PCM diffs.
//! CRC *verification* (header CRC-8, footer CRC-16) stays Phase 2 step 5:
//! both are consumed, never checked.
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
use crate::stereo::decorrelate;
use crate::subframe::{PredictorState, decode_subframe};

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

/// Decode one whole frame — subframe loop, decorrelation, footer — into
/// `left` / `right`.
///
/// `header` must be the header parsed at this exact cursor position (its
/// fields are the composition's only inputs — `parse` has already resolved
/// blocksize, bit depth, and channel mode through [`StreamDefaults`], so
/// stream defaults never reach the frame body and this driver takes none).
///
/// Buffers: **exactly `header.blocksize` samples per channel are written;**
/// samples beyond `blocksize` in a longer buffer are left untouched. The
/// contract is therefore `len() >= blocksize`, not `==` — playback reuses one
/// fixed 2048-slot buffer across frames, and the final frame of a track is
/// legitimately short (measured; see module docs on `blocksize`), so "leave
/// the tail alone" is load-bearing, not cosmetic. Mono may pass a
/// zero-length `right`; the secondary window is only touched for
/// two-subframe channel modes.
///
/// `state` is per-subframe scratch (one [`PredictorState`] per slot, reused
/// across frames — warm-up samples travel in each subframe's own header, not
/// across frames; see `subframe` module docs, corrected 2026-09-10). PCM is
/// written as sign-extended `i32` in the stream's native precision;
/// conversion to the mixer's 8-bit unsigned format happens at the playback
/// boundary, not here, so this stays reusable (and testable) independent of
/// `agb`.
///
/// **Side slot width:** the decorrelated slot is coded at `bps + 1` (§4.2:
/// "the side channel needs one extra bit of bit depth"), slot 0 for
/// side/right and slot 1 for left/mid-side (Table 16 "stored as side-right"
/// + libFLAC's `bps++` on the side slot — both measured on encoder bytes,
/// FLAC.md step 3f part 1). The mapping is derived from the header's own
/// channel code, never from a caller-supplied hint.
///
/// **Footer:** after the last subframe the frame pads to a byte boundary and
/// carries a big-endian CRC-16 (measured, libFLAC 1.5.0). Both are consumed,
/// never verified — verification is Phase 2 step 5. The run vectors' chaining
/// (`tests/frame_run_layout.rs`) pins that this consume lands the cursor
/// exactly on the next frame's first byte.
///
/// Errors: caller-contract violations (short buffers) gate **before any bit
/// is consumed**, rejections write nothing (crate rule). Past the first bit,
/// a stream error leaves the cursor wherever the failing sub-read stopped
/// (per-read atomicity from the layers below); either way the frame is dead —
/// re-seek from the manifest offset, do not continue.
///
/// Returns the number of samples written per channel (`header.blocksize`).
pub fn decode_frame(
    reader: &mut BitReader<'_>,
    header: &FrameHeader,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> crate::Result<usize> {
    let blocksize = header.blocksize;
    let slots = usize::from(header.channels.subframe_count());

    // Caller contract before the first bit: mono never touches `right`, so
    // only the two-subframe modes need it at full length.
    if left.len() < blocksize || (slots == 2 && right.len() < blocksize) {
        return Err(Error::InvalidField);
    }

    // Which slot carries the extra side bit (see doc note).
    let side_slot = match header.channels {
        ChannelConfig::RightSide => Some(0),
        ChannelConfig::LeftSide | ChannelConfig::MidSide => Some(1),
        ChannelConfig::Independent { .. } => None,
    };
    // Resolved bps is 8 or 16, so +1 cannot overflow u8; compute in u32 so
    // the arithmetic stays honest without relying on that fact.
    let slot_bits = |slot: usize| -> u8 {
        (u32::from(header.bits_per_sample) + u32::from(side_slot == Some(slot))) as u8
    };

    decode_subframe(
        reader,
        blocksize,
        slot_bits(0),
        &mut state[0],
        &mut left[..blocksize],
    )?;
    if slots == 2 {
        decode_subframe(
            reader,
            blocksize,
            slot_bits(1),
            &mut state[1],
            &mut right[..blocksize],
        )?;
    }

    // Mono passes a zero-width right window: `decorrelate`'s
    // Independent-1-arm contract covers `left` only, and slicing
    // `right[..blocksize]` here would panic on the legal empty call.
    let right_span = if slots == 2 { blocksize } else { 0 };
    decorrelate(
        header.channels,
        blocksize,
        &mut left[..blocksize],
        &mut right[..right_span],
    )?;

    // Footer: pad to the byte boundary, consume the CRC-16. `byte_align` is
    // the honest call here (unlike the header, where it would assert a
    // falsehood — module docs): subframe bodies end unaligned whenever the
    // total body bits miss a byte boundary, and the run vectors' gap rule
    // (`exit % 8 == 0 → 16-bit gap, else pad + 16`) is witnessed per frame.
    reader.byte_align();
    let _crc16_hi = reader.read_u8()?;
    let _crc16_lo = reader.read_u8()?;

    Ok(blocksize)
}
