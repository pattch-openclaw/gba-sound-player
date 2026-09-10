//! Frame-header tests over **real libFLAC output** — golden-vector parse
//! (Phase 1 step 2) plus single-field mutations of a real header.
//!
//! What this pins: `FrameHeader::parse` agrees with the independent Python
//! oracle on every field of every real header, and rejects exactly what the
//! spec/profile says to reject.
//!
//! Provenance of expectations — the rule from FLAC.md ("Prior assumptions that
//! did not survive measurement"), applied in two layers:
//!
//!   * **Field widths and code meanings** are witnessed by `flac` output:
//!     vectors are machine-generated (`scripts/gen_frame_vectors.sh`), with
//!     expected values derived by the independent RFC parser. Hand-packing is
//!     exactly how the 31-bit/3-channel error was "confirmed" before, so the
//!     acceptance tests below consume only the vector table.
//!   * **Rejection behaviour** (`Error::FrameSync`, `InvalidField`,
//!     `ProfileViolation`, `UnsupportedSampleSize`) is not something libFLAC
//!     can witness — it never emits an invalid stream. Those tests take a real
//!     header and mutate **one field at a time**, so the mutation is the only
//!     variable and the untouched bytes keep the field widths honest. Every
//!     such case says which field it mutated; none of them asserts anything
//!     about what encoders emit.
//!
//! It is a `#[cfg(test)]`-style integration test (separate crate, `std`
//! available) but stays `core`-flavoured in spirit: explicit expected values
//! throughout, no allocation beyond local parsing.

#![allow(dead_code)]

use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::format::{ChannelConfig, SampleRate};
use flac_lite::frame::{FrameHeader, StreamDefaults};

/// One parsed vector from `frame_header_vectors.txt`.
#[derive(Debug)]
struct Vector {
    label: String,
    bytes: Vec<u8>,
    blocksize: u64,
    /// `None` = "from-stream" (sample-rate code `0b0000`).
    samplerate_hz: Option<u64>,
    stream_samplerate_hz: u64,
    channels_code: u64,
    subframes: u64,
    decorrelation: String,
    /// `None` = "from-stream" (sample-size code `0b000`).
    bits_per_sample: Option<u64>,
    stream_bits_per_sample: u64,
    coded_number: u64,
    coded_number_octets: u64,
    extra_field_bytes: u64,
    crc8_byte: u8,
    crc8_bit_position: usize,
    header_bits: usize,
    crc8_byte_aligned: bool,
}

fn hex_to_bytes(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .unwrap_or_else(|e| panic!("vector has a non-hex byte {byte:?}: {e}"))
        })
        .collect()
}

fn load_vectors() -> Vec<Vector> {
    let table = include_str!("frame_header_vectors.txt");
    let mut out: Vec<Vector> = Vec::new();

    for line in table.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(char::is_whitespace)
            .unwrap_or_else(|| panic!("malformed vector-table line: {line:?}"));
        let value = value.trim();
        if key == "vector" {
            out.push(Vector {
                label: value.to_string(),
                bytes: Vec::new(),
                blocksize: 0,
                samplerate_hz: None,
                stream_samplerate_hz: 0,
                channels_code: 0,
                subframes: 0,
                decorrelation: String::new(),
                bits_per_sample: None,
                stream_bits_per_sample: 0,
                coded_number: 0,
                coded_number_octets: 0,
                extra_field_bytes: 0,
                crc8_byte: 0,
                crc8_bit_position: 0,
                header_bits: 0,
                crc8_byte_aligned: false,
            });
            continue;
        }
        let v = out
            .last_mut()
            .unwrap_or_else(|| panic!("vector field {key:?} outside a vector block"));
        let number = || {
            value
                .strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .or_else(|| value.parse::<u64>().ok())
                .unwrap_or_else(|| panic!("{}: not a number: {value:?}", v.label))
        };
        // "from-stream" is a legal value for exactly the two codes that mean
        // "look it up in the stream" (rate `0b0000`, size `0b000`).
        let opt_number = || {
            if value == "from-stream" {
                None
            } else {
                Some(number())
            }
        };
        match key {
            "bytes" => v.bytes = hex_to_bytes(value),
            "blocksize" => v.blocksize = number(),
            "samplerate_hz" => v.samplerate_hz = opt_number(),
            "stream_samplerate_hz" => v.stream_samplerate_hz = number(),
            "channels_code" => v.channels_code = number(),
            "subframes" => v.subframes = number(),
            "decorrelation" => v.decorrelation = value.to_string(),
            "bits_per_sample" => v.bits_per_sample = opt_number(),
            "stream_bits_per_sample" => v.stream_bits_per_sample = number(),
            "coded_number" => v.coded_number = number(),
            "coded_number_octets" => v.coded_number_octets = number(),
            "extra_field_bytes" => v.extra_field_bytes = number(),
            "crc8_byte" => v.crc8_byte = number() as u8,
            "crc8_bit_position" => v.crc8_bit_position = number() as usize,
            "header_bits" => v.header_bits = number() as usize,
            "crc8_byte_aligned" => v.crc8_byte_aligned = value == "true",
            // Bookkeeping / prose fields this test does not assert on. (The
            // subframe0_* witness lines are consumed by
            // tests/subframe_header_layout.rs, not here.)
            "source_stream" | "note" | "required" | "frame_offset" | "stream_frame_count"
            | "fixed_fields_bits" | "sync" | "blocksize_code" | "samplerate_code"
            | "samplesize_code" | "subframe0_type" | "subframe0_kind" | "subframe0_order"
            | "subframe0_wasted" | "subframe0_bytes" => {}
            other => panic!("unexpected vector-table key {other:?} in {}", v.label),
        }
    }
    assert!(!out.is_empty(), "vector table contained no vectors");
    out
}

/// The vector table's channel code → the enum `parse` must produce. If a
/// vector ever carries a code outside this map, the table itself needs a new
/// arm — fail loudly rather than silently skipping coverage.
fn expected_channels(code: u64) -> ChannelConfig {
    match code {
        0b0000 => ChannelConfig::Independent { channels: 1 },
        0b0001 => ChannelConfig::Independent { channels: 2 },
        0b1000 => ChannelConfig::LeftSide,
        0b1001 => ChannelConfig::RightSide,
        0b1010 => ChannelConfig::MidSide,
        other => panic!("vector carries out-of-profile channel code {other:#06b}"),
    }
}

/// Cross-check our variant against the oracle's prose label for the code.
fn decorrelation_name(cfg: ChannelConfig) -> &'static str {
    match cfg {
        ChannelConfig::Independent { .. } => "none",
        ChannelConfig::LeftSide => "left-side",
        ChannelConfig::RightSide => "side-right",
        ChannelConfig::MidSide => "mid-side",
    }
}

// ===========================================================================
// Golden vectors: `FrameHeader::parse` vs the independent oracle
// ===========================================================================

/// The step-2 acceptance test: every field of every real header, parsed
/// through `FrameHeader::parse` and compared against values derived by the
/// independent Python parser (never by hand, never by libFLAC).
///
/// The load-bearing assertions are the ones the scaffold got wrong: the 4-bit
/// `channels` field (read it as 3 bits and the coded number, the CRC-8 and the
/// cursor all come out wrong — this fails loudly here instead of silently in
/// step 3's subframe code), and the uncommon blocksize/rate tail fields, which
/// only parse correctly if the reader consumes exactly 8 or 16 bits at the
/// right position (see the `tail-*` and `rate-*` vectors).
#[test]
fn parse_matches_the_vector_table_on_every_field() {
    for v in load_vectors() {
        let what = format!("vector {}", v.label);
        assert!(!v.bytes.is_empty(), "{what}: no bytes");

        // Defaults handed in exactly as the decoder would (manifest values).
        let defaults = StreamDefaults {
            sample_rate_hz: v.stream_samplerate_hz as u32,
            bits_per_sample: v.stream_bits_per_sample as u8,
        };

        let mut r = BitReader::new(&v.bytes);
        let header = FrameHeader::parse(&mut r, &defaults)
            .unwrap_or_else(|e| panic!("{what}: parse rejected a real header: {e:?}"));

        assert_eq!(header.blocksize, v.blocksize as usize, "{what}: blocksize");
        assert_eq!(header.number, v.coded_number, "{what}: frame number");

        match v.samplerate_hz {
            // Code `0b0000`: the frame carries no rate; the stream default is
            // the whole story (this is the 65 kHz path).
            None => {
                assert_eq!(
                    header.sample_rate,
                    SampleRate::FromStreamDefault,
                    "{what}: rate code 0b0000 must stay unresolved"
                );
                assert_eq!(
                    header.sample_rate.hz(v.stream_samplerate_hz as u32),
                    v.stream_samplerate_hz as u32,
                    "{what}: FromStreamDefault must resolve to the stream default"
                );
            }
            Some(hz) => {
                // Includes the uncommon codes: their value follows the coded
                // number, and parse must have consumed it (8 or 16 bits).
                assert_eq!(
                    header.sample_rate,
                    SampleRate::Explicit(hz as u32),
                    "{what}: resolved sample rate"
                );
                // An explicit rate must ignore the stream default entirely.
                assert_eq!(
                    header.sample_rate.hz(999_999),
                    hz as u32,
                    "{what}: Explicit must not consult the default"
                );
            }
        }

        let want_channels = expected_channels(v.channels_code);
        assert_eq!(header.channels, want_channels, "{what}: channels");
        assert_eq!(
            v.decorrelation,
            decorrelation_name(want_channels),
            "{what}: vector's decorrelation label disagrees with its code"
        );
        assert_eq!(
            usize::from(header.channels.subframe_count()),
            v.subframes as usize,
            "{what}: subframe count"
        );

        assert_eq!(
            header.bits_per_sample,
            v.bits_per_sample
                .expect("vector must resolve bps or be from-stream") as u8,
            "{what}: bits per sample"
        );

        // Cursor: exactly past the CRC-8, i.e. on the first subframe bit.
        assert_eq!(
            r.bit_position(),
            v.crc8_bit_position + 8,
            "{what}: cursor past CRC-8"
        );
        assert_eq!(
            r.bit_position(),
            v.header_bits,
            "{what}: whole header consumed"
        );
        assert_eq!(
            v.crc8_bit_position % 8,
            0,
            "{what}: a real header's CRC-8 is byte aligned; if this ever fires, \
             the generator or the field widths are wrong"
        );
        assert!(v.crc8_byte_aligned);
        assert!(
            r.bits_remaining() < 8,
            "{what}: vectors hold the header only"
        );

        // Header length as a function of the variable tail: 32 fixed bits,
        // whole coded octets, whole uncommon-field octets, one CRC-8 octet.
        assert_eq!(
            v.header_bits,
            32 + 8 * (v.coded_number_octets + v.extra_field_bytes + 1) as usize,
            "{what}: header length must decompose into fixed + coded + extra + CRC"
        );
    }
}

/// `byte_align()` before the header CRC-8 is a no-op on real data — and the
/// scaffold's reasoning for calling it ("the header CRC is never byte aligned")
/// was based on the 31-bit field count. Pinned so nobody re-adds the call for
/// that reason: it reads as defensive but encodes a false claim about FLAC.
#[test]
fn header_crc_needs_no_byte_align_because_it_is_already_aligned() {
    for v in load_vectors() {
        let mut r = BitReader::new(&v.bytes);
        let _ = r.read_bits(32);
        let _ = r.read_utf8_coded();
        // Step over any uncommon blocksize / sample rate octets: still whole
        // octets, so alignment survives (this is why the CRC-8 is structurally
        // aligned, not merely observed to be).
        for _ in 0..v.extra_field_bytes {
            let _ = r.read_u8();
        }
        assert_eq!(
            r.byte_align(),
            0,
            "vector {}: cursor is already byte-aligned at the header CRC-8, so \
             byte_align() there is dead code",
            v.label
        );
        assert_eq!(r.read_u8().unwrap(), v.crc8_byte, "vector {}", v.label);
    }
}

/// Header lengths across the real set, now that the uncommon fields are in
/// it: six, seven, eight and nine bytes all occur, and the extra-field count
/// from the oracle matches the length exactly.
#[test]
fn observed_header_lengths_span_six_to_nine_bytes() {
    let mut seen: Vec<usize> = load_vectors().iter().map(|v| v.header_bits / 8).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen,
        vec![6, 7, 8, 9],
        "expected 6..=9-byte real headers (7/8/9 come from the coded number \
         and the uncommon blocksize/rate fields)"
    );
}

// ===========================================================================
// Mutations of a real header: rejection behaviour + uncovered accept paths
// ===========================================================================
//
// libFLAC cannot witness rejection — it never emits an invalid stream. So
// these cases take the real `stereo-6byte-first` header (`FF F8 B8 A8 00 81`,
// fetched from the table at runtime, never copied by hand) and mutate exactly
// ONE thing. Each case names its mutation; the untouched bytes keep the field
// widths honest, which is the property the golden set already pins.

/// The real header every mutation below derives from.
fn base_header() -> Vec<u8> {
    load_vectors()
        .into_iter()
        .find(|v| v.label == "stereo-6byte-first")
        .expect("the required vector stereo-6byte-first must exist")
        .bytes
}

fn defaults_16_32k() -> StreamDefaults {
    StreamDefaults {
        sample_rate_hz: 32_000,
        bits_per_sample: 16,
    }
}

/// Parse a mutated header; the trailing `0x00` is filler for the CRC-8 byte,
/// which parse consumes but never verifies (Phase 2 step 5 owns verification).
fn parse_case(bytes: &[u8]) -> Result<FrameHeader, Error> {
    let mut with_crc = bytes.to_vec();
    if with_crc.len() * 8 > 40 {
        with_crc.push(0x00);
    }
    FrameHeader::parse(&mut BitReader::new(&with_crc), &defaults_16_32k())
}

#[test]
fn mutation_rejects_sync_and_reserved_bits() {
    // Sync: the 14-bit pattern must be 0b11111111111110 (see frame.rs module
    // docs: the RFC sync is 15 bits + strategy bit; 0x3FFE covers both).
    let mut b = base_header();
    b[0] = 0xFA; // 11111010…: breaks the 14-bit sync
    assert_eq!(parse_case(&b), Err(Error::FrameSync), "mutated sync");

    // Reserved bit 1 (bit 14, low bit of byte 1's upper nibble… it is the
    // 15th bit): must be zero.
    let mut b = base_header();
    b[1] = 0xFA; // sync intact (0x3FFE), reserved bit set
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "mutated reserved bit"
    );

    // Blocking strategy (bit 15): 1 = variable blocksize = outside our
    // profile, but valid FLAC — ProfileViolation, not InvalidField.
    let mut b = base_header();
    b[1] = 0xF9;
    assert_eq!(
        parse_case(&b),
        Err(Error::ProfileViolation),
        "variable blocking"
    );
}

#[test]
fn mutation_rejects_reserved_and_forbidden_codes() {
    // Blocksize code 0b0000 is reserved.
    let mut b = base_header();
    b[2] = 0x08; // blocksize 0b0000, rate 0b1000
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "reserved blocksize code"
    );

    // Sample-rate code 0b1111 is forbidden by the spec.
    let mut b = base_header();
    b[2] = 0xBF; // blocksize 0b1011, rate 0b1111
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "forbidden rate code"
    );

    // Channel code 0b1011..=0b1111: reserved, not valid FLAC.
    let mut b = base_header();
    b[3] = 0xB8; // channels 0b1011
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "reserved channel code"
    );

    // Sample-size code 0b011 is reserved.
    let mut b = base_header();
    b[3] = 0xA6; // channels 0b1010, size 0b011
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "reserved sample-size code"
    );
}

#[test]
fn mutation_separates_out_of_profile_from_unsupported_depth() {
    // 3–8 channel layouts are valid FLAC, outside our 1/2-channel profile.
    let mut b = base_header();
    b[3] = 0x38; // channels 0b0011 (4-channel), size 0b100
    assert_eq!(
        parse_case(&b),
        Err(Error::ProfileViolation),
        "4-channel layout"
    );

    // 12/20/24/32-bit depths are valid FLAC the decoder does not carry.
    for (name, size_code) in [
        ("12-bit", 0b010u8),
        ("20-bit", 0b101),
        ("24-bit", 0b110),
        ("32-bit", 0b111),
    ] {
        let mut b = base_header();
        // Byte 3 = [channels 4 bits][size code 3 bits][reserved 1 bit], so the
        // size code sits at bit weight 2, not in the low nibble's bottom bit.
        // (An earlier draft shifted by 4, which left the size code at 0b000 =
        // "from stream" and made the test silently assert nothing.)
        b[3] = 0xA0 | (size_code << 1);
        assert_eq!(
            parse_case(&b),
            Err(Error::UnsupportedSampleSize),
            "{name} depth must be UnsupportedSampleSize"
        );
    }
}

#[test]
fn mutation_rejects_a_broken_coded_number() {
    // Stray continuation lead (0b10xxxxxx) where the frame number lives.
    let mut b = base_header();
    b[4] = 0x80;
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "stray continuation lead"
    );

    // A frame number above the §9.1.5 cap (31 bits) can only arrive in the
    // 7-byte form: 0xFE 0x82 0x80… = 2^31. The base header's coded number is
    // one octet, so the number is *replaced* (truncate) and grown, not
    // overwritten in place.
    let mut b = base_header();
    b.truncate(4); // drop the 1-octet coded number and the real CRC byte
    b.extend_from_slice(&[0xFE, 0x82, 0x80, 0x80, 0x80, 0x80, 0x80]);
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "fixed-blocksize frame number must fit 31 bits"
    );
}

#[test]
fn mutation_rejects_forbidden_uncommon_values() {
    // Uncommon blocksize 16-bit (code 0b0111) storing 0xFFFF → blocksize
    // 65536, forbidden outright (§9.1.6).
    let b = [0xFF, 0xF8, 0x78, 0xA8, 0x00, 0xFF, 0xFF];
    assert_eq!(
        parse_case(&b),
        Err(Error::InvalidField),
        "blocksize 65536 forbidden"
    );

    // Uncommon rate in kHz (code 0b1100) storing 0 → rate 0, which §9.1.7
    // reserves for non-audio streams. We only decode audio.
    let b = [0xFF, 0xF8, 0xBC, 0xA8, 0x00, 0x00];
    assert_eq!(parse_case(&b), Err(Error::InvalidField), "zero sample rate");
}

#[test]
fn mutation_accepts_the_from_stream_depth_code_via_defaults() {
    // Sample-size code 0b000 = "depth only in STREAMINFO". The GAFP manifest
    // replaces STREAMINFO, so parse resolves it through StreamDefaults —
    // the same rule the rate path already proves on the r65k vector.
    let mut b = base_header();
    b[3] = 0xA0; // channels 0b1010, size 0b000, reserved 0
    let h = parse_case(&b).expect("from-stream depth must resolve via defaults");
    assert_eq!(
        h.bits_per_sample, 16,
        "resolved from StreamDefaults.bits_per_sample"
    );

    // …and a default the decoder does not support is refused, not trusted.
    let mut with_crc = b.clone();
    with_crc.push(0x00);
    let weird = StreamDefaults {
        sample_rate_hz: 32_000,
        bits_per_sample: 24,
    };
    assert_eq!(
        FrameHeader::parse(&mut BitReader::new(&with_crc), &weird),
        Err(Error::UnsupportedSampleSize),
        "a 24-bit stream default is not decodeable"
    );
}

#[test]
fn parse_requires_a_byte_aligned_cursor() {
    // RFC 9639 §9.1: "Each frame MUST start on a byte boundary." A mid-byte
    // cursor means there is no frame here, same verdict as a bad sync.
    let bytes = base_header();
    let mut r = BitReader::new(&bytes);
    let _ = r.read_bits(3).unwrap();
    assert_eq!(
        FrameHeader::parse(&mut r, &defaults_16_32k()),
        Err(Error::FrameSync),
        "unaligned cursor is 'no frame here', not a field error"
    );
}

#[test]
fn blocksize_table_matches_rfc_table_14() {
    // Witnesses OUR table (the division-free shift forms in frame.rs) against
    // RFC 9639 Table 14 — the widths themselves are witnessed by the golden
    // set above. Mutating only the blocksize nibble keeps everything else at
    // the real header's values.
    let cases: &[(u8, usize)] = &[
        (0b0001, 192),
        (0b0010, 576),
        (0b0011, 1152),
        (0b0100, 2304),
        (0b0101, 4608),
        (0b1000, 256),
        (0b1001, 512),
        (0b1010, 1024),
        (0b1011, 2048),
        (0b1100, 4096),
        (0b1101, 8192),
        // The top end the FLAC.md prose wrongly called "reserved":
        (0b1110, 16384),
        (0b1111, 32768),
    ];
    for &(code, want) in cases {
        let mut b = base_header();
        b[2] = (code << 4) | 0x8; // rate nibble stays 0b1000 (32 kHz)
        let h = parse_case(&b).unwrap_or_else(|e| panic!("blocksize code {code:#06b}: {e:?}"));
        assert_eq!(h.blocksize, want, "blocksize code {code:#06b}");
    }
    // 0b0000 reserved; 0b0110/0b0111 are the uncommon forms, exercised by the
    // tail-* golden vectors (accept) and the 0xFFFF case above (reject).
    let mut b = base_header();
    b[2] = 0x08;
    assert_eq!(parse_case(&b), Err(Error::InvalidField), "reserved 0b0000");
}
