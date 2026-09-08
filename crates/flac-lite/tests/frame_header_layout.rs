//! Frame-header **layout** tests over real libFLAC output.
//!
//! What this pins, and why it exists at all: the scaffold documented the
//! frame header's fixed fields as 31 bits with a 3-bit `channels` field, and a
//! unit test asserted it — a mis-transcription of RFC 9639 9.1.3 that would
//! have made `FrameHeader::parse` misposition the cursor on every frame and
//! then read garbage subframe data three steps later. The vectors below are
//! real encoder bytes, so they cannot agree with a wrong transcription.
//!
//! Two deliberate choices about provenance:
//!   * Vectors are machine-generated from `flac` output, never hand-packed
//!     (regenerate: `scripts/gen_frame_vectors.sh`). Hand-packing is exactly
//!     how the 31-bit error was "confirmed" before.
//!   * Expected values are parsed out of the vector table, not restated in
//!     Rust literals, so the only thing this test can disagree about is the
//!     *bit layout* — which is the thing under test.
//!
//! It is a `#[cfg(test)]`-style integration test (separate crate, `std`
//! available) but stays `core`-flavoured in spirit: fixed-size buffers, no
//! allocation beyond local parsing, explicit expected values throughout.

#![allow(dead_code)]

use flac_lite::bits::BitReader;

/// One parsed vector from `frame_header_vectors.txt`.
#[derive(Debug)]
struct Vector {
    label: String,
    bytes: Vec<u8>,
    blocksize_code: u32,
    samplerate_code: u32,
    channels_code: u32,
    samplesize_code: u32,
    coded_number: u64,
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
                blocksize_code: 0,
                samplerate_code: 0,
                channels_code: 0,
                samplesize_code: 0,
                coded_number: 0,
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
        match key {
            "bytes" => v.bytes = hex_to_bytes(value),
            "blocksize_code" => v.blocksize_code = number() as u32,
            "samplerate_code" => v.samplerate_code = number() as u32,
            "channels_code" => v.channels_code = number() as u32,
            "samplesize_code" => v.samplesize_code = number() as u32,
            "coded_number" => v.coded_number = number(),
            "crc8_byte" => v.crc8_byte = number() as u8,
            "crc8_bit_position" => v.crc8_bit_position = number() as usize,
            "header_bits" => v.header_bits = number() as usize,
            "crc8_byte_aligned" => v.crc8_byte_aligned = value == "true",
            // Bookkeeping / prose fields this test does not assert on.
            "source_stream" | "note" | "required" | "frame_offset"
            | "stream_frame_count" | "fixed_fields_bits" | "sync" | "blocksize"
            | "samplerate_hz" | "stream_samplerate_hz" | "subframes"
            | "decorrelation" | "bits_per_sample" | "stream_bits_per_sample"
            | "coded_number_octets" | "extra_field_bytes" | "subframe0_type" => {}
            other => panic!("unexpected vector-table key {other:?} in {}", v.label),
        }
    }
    assert!(!out.is_empty(), "vector table contained no vectors");
    out
}

/// Walk a real header through `BitReader` exactly the way `FrameHeader::parse`
/// will, asserting every field width and cursor position.
///
/// The load-bearing assertion is the 4-bit `channels` field: read it as 3 bits
/// and every subsequent field, the coded number, and the CRC-8 byte are wrong,
/// which fails loudly here instead of silently in the decoder.
#[test]
fn real_headers_parse_with_32_bit_fixed_fields() {
    for v in load_vectors() {
        let what = format!("vector {}", v.label);
        assert!(!v.bytes.is_empty(), "{what}: no bytes");

        let mut r = BitReader::new(&v.bytes);
        // --- fixed fields: 14 + 1 + 1 + 4 + 4 + 4 + 3 + 1 = 32 -------------
        assert_eq!(r.read_bits(14).unwrap(), 0x3FFE, "{what}: sync (fixed blocksize)");
        assert_eq!(r.read_bits(1).unwrap(), 0, "{what}: reserved bit must be 0");
        assert_eq!(r.read_bits(1).unwrap(), 0, "{what}: blocking strategy (fixed)");
        assert_eq!(
            r.read_bits(4).unwrap(),
            v.blocksize_code,
            "{what}: blocksize code is a 4-bit field"
        );
        assert_eq!(
            r.read_bits(4).unwrap(),
            v.samplerate_code,
            "{what}: sample-rate code is a 4-bit field"
        );
        assert_eq!(
            r.read_bits(4).unwrap(),
            v.channels_code,
            "{what}: channels is a 4-BIT field (RFC 9639 9.1.3) — a 3-bit read \
             here desynchronises the whole header"
        );
        assert_eq!(
            r.read_bits(3).unwrap(),
            v.samplesize_code,
            "{what}: sample-size code is a 3-bit field"
        );
        assert_eq!(r.read_bits(1).unwrap(), 0, "{what}: trailing reserved bit");
        assert_eq!(r.bit_position(), 32, "{what}: fixed fields are 32 bits");

        // --- coded number: whole octets, so alignment is preserved ---------
        let number = r.read_utf8_coded().unwrap();
        assert_eq!(number, v.coded_number, "{what}: coded frame number");

        // --- CRC-8: always byte aligned, given the two facts above ---------
        assert_eq!(
            r.bit_position(),
            v.crc8_bit_position,
            "{what}: CRC-8 must follow the coded number directly"
        );
        assert_eq!(
            v.crc8_bit_position % 8,
            0,
            "{what}: a real header's CRC-8 is byte aligned; if this ever fires, \
             the generator or the field widths are wrong"
        );
        assert_eq!(
            r.read_u8().unwrap(),
            v.crc8_byte,
            "{what}: CRC-8 byte (consumed here, verified by the generator)"
        );
        assert_eq!(r.bit_position(), v.header_bits, "{what}: whole header consumed");
        assert!(r.bits_remaining() <= 8, "{what}: vectors hold the header only");
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

/// The two header lengths the decoder must handle, straight from real output:
/// a 1-octet coded number gives a 6-byte header, a 2-octet one gives 7.
#[test]
fn observed_header_lengths_are_six_and_seven_bytes() {
    let mut seen: Vec<usize> = load_vectors()
        .iter()
        .map(|v| v.header_bits / 8)
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen,
        vec![6, 7],
        "expected exactly the 6-byte and 7-byte real headers in the vector table"
    );
}
