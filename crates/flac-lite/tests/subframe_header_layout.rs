//! Subframe-header tests over **real libFLAC output** — `SubframeType::parse`
//! against the `subframe0_*` witness lines of the golden-vector table.
//!
//! Same rule as `frame_header_layout.rs`, one layer down the bitstream: the
//! field *widths* and the cursor outcome are witnessed by encoder bytes, never
//! by hand-packed arrays (that is exactly how the 31-bit frame-header error
//! survived review — FLAC.md → "Frame header: measured byte layout").
//!
//! Provenance, in two layers:
//!
//!   * **Acceptance** (`Constant` / `Verbatim` / `Fixed(n)` / `Lpc { order }`,
//!     and the 7-bit cursor landing on the wasted-bits flag) is witnessed by
//!     `tests/frame_header_vectors.txt`, whose `subframe0_*` lines are derived
//!     by the independent Python oracle in `scripts/frame_vectors.py` from
//!     libFLAC 1.5.0 bytes.
//!   * **Rejection** (set pad bit, reserved type codes) cannot be
//!     encoder-witnessed — libFLAC never emits an invalid stream. Those cases
//!     mutate ONE field of a real subframe header at runtime, exactly as the
//!     frame-header tests mutate a real frame header.
//!   * **Boundary codes** (FIXED 0/4, LPC 1/32) are Table 19 mapping claims:
//!     real encodes land mid-range (fixed0/1, lpc3/4 measured), so those four
//!     codes are synthesized from the table itself, with the encodings of the
//!     witnessed mid-range codes as the calibration.

use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::frame::{FrameHeader, StreamDefaults};
use flac_lite::subframe::SubframeType;

/// One vector, restricted to the fields this test consumes.
#[derive(Debug)]
struct Vector {
    label: String,
    /// Frame header bytes, through the CRC-8 (the table's `bytes`).
    header: Vec<u8>,
    header_bits: usize,
    /// The two bytes following the CRC-8 (`subframe0_bytes`): the pad+type
    /// octet and the first octet of the wasted-bits run.
    subframe0: Vec<u8>,
    kind: String,
    order: Option<u64>,
    wasted: Option<u64>,
    stream_samplerate_hz: u64,
    stream_bits_per_sample: u64,
}

fn hex_to_bytes(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .unwrap_or_else(|e| panic!("vector has a non-hex byte {byte:?}: {e}"))
        })
        .collect()
}

/// Load the shared golden-vector table. Deliberately a second, narrower loader
/// rather than a refactor of `frame_header_layout.rs`: the two files witness
/// different layers, and neither should be able to break the other's harness.
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
                header: Vec::new(),
                header_bits: 0,
                subframe0: Vec::new(),
                kind: String::new(),
                order: None,
                wasted: None,
                stream_samplerate_hz: 0,
                stream_bits_per_sample: 0,
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
            "bytes" => v.header = hex_to_bytes(value),
            "header_bits" => v.header_bits = number() as usize,
            "subframe0_bytes" => v.subframe0 = hex_to_bytes(value),
            "subframe0_kind" => v.kind = value.to_string(),
            // "None" appears only for the invalid/reserved placeholder, which
            // no real vector carries.
            "subframe0_order" => {
                v.order = if value == "None" {
                    None
                } else {
                    Some(number())
                }
            }
            "subframe0_wasted" => {
                v.wasted = if value == "None" {
                    None
                } else {
                    Some(number())
                }
            }
            "stream_samplerate_hz" => v.stream_samplerate_hz = number(),
            "stream_bits_per_sample" => v.stream_bits_per_sample = number(),
            // Everything else belongs to the frame-header layer's assertions.
            _ => {}
        }
    }
    assert!(!out.is_empty(), "vector table contained no vectors");
    out
}

/// The bits immediately after a frame header, as the decoder would see them:
/// header bytes + the witnessed subframe bytes, one cursor, header parsed.
fn cursor_past_header(v: &Vector) -> BitReader<'_> {
    // Borrow lives as long as the vectors this test owns per-call; the table is
    // `include_str!`, so a static-extended slice is available for free.
    let mut bytes: Vec<u8> = v.header.clone();
    bytes.extend_from_slice(&v.subframe0);
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let defaults = StreamDefaults {
        sample_rate_hz: v.stream_samplerate_hz as u32,
        bits_per_sample: v.stream_bits_per_sample as u8,
    };
    let mut reader = BitReader::new(bytes);
    FrameHeader::parse(&mut reader, &defaults)
        .unwrap_or_else(|e| panic!("{}: frame header rejected: {e:?}", v.label));
    assert_eq!(
        reader.bit_position(),
        v.header_bits,
        "{}: frame-header cursor moved before the subframe read",
        v.label
    );
    reader
}

/// Read the wasted-bits field (§9.2.2) from the cursor `SubframeType::parse`
/// leaves: flag bit, then `k − 1` zeros terminated by a one.
///
/// This is an independent reading of the field (loop over `read_bits(1)`, not
/// the accumulator style the library will eventually use), which is the point:
/// if the cursor had landed one bit off, the derived `k` would disagree with
/// the oracle's table value.
fn read_wasted(reader: &mut BitReader<'_>) -> u64 {
    if reader.read_bits(1).expect("wasted flag") == 0 {
        return 0;
    }
    let mut k = 1;
    while reader.read_bits(1).expect("wasted unary") == 0 {
        k += 1;
        assert!(k < 33, "wasted-bits run unreasonably long: {k}");
    }
    k
}

fn expected_type(kind: &str, order: Option<u64>) -> SubframeType {
    match (kind, order) {
        ("constant", Some(0)) => SubframeType::Constant,
        ("verbatim", Some(0)) => SubframeType::Verbatim,
        ("fixed", Some(n)) => SubframeType::Fixed(n as u8),
        ("lpc", Some(n)) => SubframeType::Lpc { order: n as u8 },
        (kind, order) => panic!("vector carries unusable subframe type {kind:?} {order:?}"),
    }
}

// ===========================================================================
// Acceptance: every real subframe header in the table
// ===========================================================================

/// The load-bearing test: `SubframeType::parse` reproduces the oracle's kind +
/// order for every real subframe header, and stops exactly on the wasted-bits
/// flag (witnessed by deriving the oracle's `wasted` value from there).
#[test]
fn parse_matches_the_vector_table_on_every_real_subframe() {
    let mut kinds: Vec<&'static str> = Vec::new();
    for v in load_vectors() {
        let what = format!("vector {}", v.label);
        assert_eq!(v.subframe0.len(), 2, "{what}: need the subframe octet + 1");
        let mut reader = cursor_past_header(&v);

        let parsed = SubframeType::parse(&mut reader)
            .unwrap_or_else(|e| panic!("{what}: parse rejected a real subframe: {e:?}"));
        let want = expected_type(&v.kind, v.order);
        assert_eq!(parsed, want, "{what}: subframe type");

        // The 7-bit contract: pad + type, no further consumption.
        assert_eq!(
            reader.bit_position(),
            v.header_bits + 7,
            "{what}: parse must land on the wasted-bits flag \
             (read too few bits and the residual lands mid-header; \
             too many and the wasted run is eaten)"
        );

        // Cursor witness: the wasted value derived from where parse stopped
        // must equal the oracle's independently-derived value. The harness
        // read is deliberately naive (bit-by-bit loop, this file's own code).
        let want_wasted = v.wasted.expect("real vectors carry a wasted value");
        assert_eq!(
            read_wasted(&mut reader),
            want_wasted,
            "{what}: wasted bits read from parse's exit cursor"
        );

        // Library agreement (step 3a): `BitReader::read_wasted_bits` must
        // reproduce the oracle's value from the same cursor — encoder bytes
        // witness the library reader, not just the harness's copy of it.
        let mut reader2 = cursor_past_header(&v);
        let parsed2 = SubframeType::parse(&mut reader2)
            .unwrap_or_else(|e| panic!("{what}: re-parse rejected: {e:?}"));
        assert_eq!(parsed2, want, "{what}: re-parse must be deterministic");
        assert_eq!(
            reader2.read_wasted_bits().unwrap() as u64,
            want_wasted,
            "{what}: library wasted reader vs oracle"
        );
        // Same composite cursor: library read lands where the naive loop did.
        assert_eq!(
            reader2.bit_position(),
            reader.bit_position(),
            "{what}: library and harness must consume the same field width"
        );

        // `order()` is the warm-up count the body carries (§9.2.5/9.2.6 tables).
        assert_eq!(parsed.order(), v.order.unwrap() as usize, "{what}: order()");

        kinds.push(match parsed {
            SubframeType::Constant => "constant",
            SubframeType::Verbatim => "verbatim",
            SubframeType::Fixed(_) => "fixed",
            SubframeType::Lpc { .. } => "lpc",
        });
    }

    // Coverage, split by what is guaranteed: the REQUIRED vectors carry FIXED
    // (`fixed-only-first`) and LPC (`stereo-6byte-first`), so those must always
    // be witnessed. CONSTANT comes from the optional `silence-constant` and
    // VERBATIM has no vector yet, so their absence is a note, never a pass that
    // pretends to be coverage.
    let seen: std::collections::HashSet<&str> = kinds.into_iter().collect();
    assert!(
        seen.contains("fixed") && seen.contains("lpc"),
        "the required vectors must witness FIXED and LPC; saw {seen:?}"
    );
    println!("subframe kinds witnessed by the table: {seen:?}");
    if !seen.contains("constant") {
        println!("note: no CONSTANT vector in this table (optional vector, encoder-dependent)");
    }
    if !seen.contains("verbatim") {
        println!("note: no VERBATIM vector yet (needs an incompressible source stream)");
    }
    // Wasted bits > 0 must be witnessed for the cursor contract to mean
    // anything; `subframe-wasted-bits` carries it. If it ever silently drops,
    // this fires instead of the test quietly degrading to wasted==0 only.
    let any_wasted = load_vectors().iter().any(|v| v.wasted == Some(5));
    assert!(
        any_wasted,
        "no vector exercises wasted bits > 0; the exit-cursor witness \
         (parse stops BEFORE the flag) would be untested"
    );
}

// ===========================================================================
// Rejection: one-field mutations of a real subframe header
// ===========================================================================

/// The real subframe octet every mutation below derives from (pad 0 + type 6 +
/// wasted flag 1, MSB-first). Fetched from the table at runtime, never copied.
fn base_subframe_byte() -> u8 {
    load_vectors()
        .into_iter()
        .find(|v| v.label == "stereo-6byte-first")
        .expect("the required vector stereo-6byte-first must exist")
        .subframe0[0]
}

/// Encode a 7-bit subframe-type field the way the bitstream does, so every
/// mutation below changes the type code and nothing else.
fn octet(code: u8, wasted_flag: bool) -> Vec<u8> {
    debug_assert!(code < 64);
    vec![(code << 1) | u8::from(wasted_flag), 0x00]
}

fn parse_bytes(bytes: &[u8]) -> Result<SubframeType, Error> {
    SubframeType::parse(&mut BitReader::new(bytes))
}

#[test]
fn mutation_rejects_a_set_pad_bit() {
    // Same real type code, pad bit flipped: §9.2.1 "MUST be 0".
    let real = base_subframe_byte();
    let mutated = real | 0x80;
    assert_ne!(
        real & 0x80,
        0x80,
        "the base vector must have a clear pad bit"
    );
    assert_eq!(
        parse_bytes(&[mutated, 0x00]),
        Err(Error::InvalidField),
        "set pad bit is InvalidField"
    );
    // …and the untouched byte parses fine: the mutation is the only variable.
    assert!(parse_bytes(&[real, 0x00]).is_ok(), "base must be accepted");
}

#[test]
fn mutation_rejects_reserved_type_codes() {
    // Both reserved ranges of Table 19, at their extremes and middles. libFLAC
    // cannot witness these, so the pattern is synthesized from the table —
    // with the real vector's wasted flag preserved.
    for code in [0b000010u8, 0b000100, 0b000111, 0b001101, 0b010000, 0b011111] {
        assert_eq!(
            parse_bytes(&octet(code, false)),
            Err(Error::InvalidField),
            "reserved subframe type {code:#08b} must be InvalidField"
        );
    }
    // Calibration: the synthesized encoding of a code the table DOES witness
    // must match the real byte's type-code bits, proving `octet()` builds
    // fields the same way the encoder does (otherwise the rejections above
    // would be testing a private invention).
    let real = base_subframe_byte();
    let real_code = (real >> 1) & 0x3F;
    assert_eq!(
        parse_bytes(&octet(real_code, real & 1 != 0)).ok(),
        parse_bytes(&[real, 0x00]).ok(),
        "synthesized field must agree with the real byte for the same code"
    );
}

#[test]
fn type_code_boundaries_match_table_19() {
    // The four boundary codes are Table 19 mapping claims; measured encodes
    // land mid-range (fixed0/1, lpc3/4), so these are built from the table.
    // The `octet()` encoding itself is calibrated against real bytes above.
    for (code, want) in [
        (0b000000u8, SubframeType::Constant),
        (0b000001, SubframeType::Verbatim),
        (0b001000, SubframeType::Fixed(0)),
        (0b001100, SubframeType::Fixed(4)),
        (0b100000, SubframeType::Lpc { order: 1 }),
        (0b111111, SubframeType::Lpc { order: 32 }),
    ] {
        let got = parse_bytes(&octet(code, false))
            .unwrap_or_else(|e| panic!("code {code:#08b} is legal, rejected: {e:?}"));
        assert_eq!(got, want, "code {code:#08b}");
        let want_order = match want {
            SubframeType::Constant | SubframeType::Verbatim => 0,
            SubframeType::Fixed(n) | SubframeType::Lpc { order: n } => usize::from(n),
        };
        assert_eq!(got.order(), want_order, "order() for code {code:#08b}");
    }
    // FIXED order never exceeds MAX_FIXED_ORDER, LPC never MAX_LPC_ORDER.
    assert_eq!(SubframeType::Fixed(4).order(), 4);
    assert_eq!(SubframeType::Lpc { order: 32 }.order(), 32);
}

/// EOF for this field is degenerate, and pinning *why* is the point.
///
/// The field is 7 bits and reads are byte-granular, so any non-empty slice
/// holds it: `EndOfStream` is reachable only on an empty input, where the pad
/// bit read fails and the cursor never moves. The "cursor may sit one bit in"
/// clause on `parse` is therefore about the *pad-bit rejection* (below), never
/// about EOF — an earlier draft of this test assumed a 1-byte slice was too
/// short, which the reader contradicts.
#[test]
fn eof_is_degenerate_because_the_field_fits_one_byte() {
    // Empty: fails on the pad bit, cursor never moves.
    let mut r = BitReader::new(&[]);
    assert_eq!(SubframeType::parse(&mut r), Err(Error::EndOfStream));
    assert_eq!(r.bit_position(), 0, "no bits consumed on an empty read");

    // One byte is always enough (7 < 8) for any *legal* field: pad clear, so
    // the only outcome is accept-or-reserved-code, never EOF. (An earlier draft
    // used 0xFF here and read the resulting InvalidField as a failure — it is
    // not: 0xFF's top bit is the pad bit, and rejecting it is correct.)
    for byte in [0x00u8, 0x46, 0x7E] {
        let buf = [byte];
        let mut r = BitReader::new(&buf);
        let parsed = SubframeType::parse(&mut r);
        assert!(
            !matches!(parsed, Err(Error::EndOfStream)),
            "byte {byte:#04X}: a single byte holds the whole 7-bit field, got {parsed:?}"
        );
        if parsed.is_ok() {
            assert_eq!(r.bit_position(), 7, "byte {byte:#04X}: exactly 7 bits read");
        }
    }

    // The error state that IS reachable: pad bit set. Cursor sits one bit in,
    // which is the documented outcome — pinned so a switch to "atomic on
    // error" is a deliberate decision, not a silent drift.
    let mut r = BitReader::new(&[0x8E]); // pad = 1, code 0b000111 (reserved too)
    assert_eq!(SubframeType::parse(&mut r), Err(Error::InvalidField));
    assert_eq!(r.bit_position(), 1, "pad bit consumed before the rejection");
}
