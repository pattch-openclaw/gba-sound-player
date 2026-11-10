//! Item 6a-6 — the width differential, narrow leg.
//!
//! `i32-integrator` is on by default, so this suite is built exactly when the
//! narrow leg is the shipped default (`required-features` in `Cargo.toml`).
//! It pins that leg to an **external** oracle: the `pcm_samples` in
//! `tests/subframe_body_vectors.txt` came from `flac -d`, not from this crate.
//! The companion configuration is `make flac-test-wide` (`--no-default-features`),
//! which runs the same tables through the i64 reference leg. Two builds, one
//! oracle: neither leg's green can stand in for the other's.
//!
//! What this file adds above the existing body-layout suite:
//!
//! 1. every commit-fed vector reports [`ArithmeticWidth::I32`] (they are
//!    profile-compliant, so the guard must pass — an off-profile vector would
//!    have to state a different expectation here rather than be accepted
//!    silently);
//! 2. the guard itself is exercised at its boundary — including the two cases
//!    E4 says fail (order 32 at coefficient precision 15, and a sample width
//!    that doubles the bound over `i32::MAX`);
//! 3. [`ArithmeticWidth::and`] combines two subframe reports the way a frame
//!    does.
//!
//! The same-build proof that the wide escape leg actually runs, and produces
//! the same samples as the narrow one below the bound, lives in
//! `subframe::tests` (it needs the private dispatch, which is not public API).

use flac_lite::bits::BitReader;
use flac_lite::frame::{FrameHeader, StreamDefaults};
use flac_lite::residual::TableCountKernel;
use flac_lite::subframe::lpc_accumulator_fits_i32;
use flac_lite::subframe::{
    ArithmeticWidth, PredictorState, SubframeType, decode_subframe_with_width,
};

#[derive(Debug)]
struct BodyVector {
    label: String,
    frame: Vec<u8>,
    stream_samplerate_hz: u64,
    stream_bits_per_sample: u64,
    blocksize: u64,
    kind: String,
    order: u64,
    exit_bits: u64,
    pcm_samples: Vec<i64>,
}

fn hex_to_bytes(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .unwrap_or_else(|e| panic!("vector has a non-hex byte {byte:?}: {e}"))
        })
        .collect()
}

fn ints(text: &str) -> Vec<i64> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    text.split_whitespace()
        .map(|v| {
            v.parse::<i64>()
                .unwrap_or_else(|e| panic!("not an integer {v:?}: {e}"))
        })
        .collect()
}

fn load_body_vectors() -> Vec<BodyVector> {
    let mut out: Vec<BodyVector> = Vec::new();
    for line in include_str!("subframe_body_vectors.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let value = value.trim();
        if key == "body_vector" {
            out.push(BodyVector {
                label: value.to_string(),
                frame: Vec::new(),
                stream_samplerate_hz: 0,
                stream_bits_per_sample: 0,
                blocksize: 0,
                kind: String::new(),
                order: 0,
                exit_bits: 0,
                pcm_samples: Vec::new(),
            });
            continue;
        }
        let v = out
            .last_mut()
            .unwrap_or_else(|| panic!("body field {key:?} outside a body_vector block"));
        let number = || {
            value
                .parse::<u64>()
                .unwrap_or_else(|e| panic!("{}: not a number: {value:?} ({e})", v.label))
        };
        match key {
            "frame_hex" => v.frame = hex_to_bytes(value),
            "stream_samplerate_hz" => v.stream_samplerate_hz = number(),
            "stream_bits_per_sample" => v.stream_bits_per_sample = number(),
            "blocksize" => v.blocksize = number(),
            "subframe_kind" => v.kind = value.to_string(),
            "subframe_order" => v.order = number(),
            "exit_bits" => v.exit_bits = number(),
            "pcm_samples" => v.pcm_samples = ints(value),
            _ => {}
        }
    }
    assert!(!out.is_empty(), "body vector table contained no vectors");
    out
}

fn expected_type(v: &BodyVector) -> SubframeType {
    match v.kind.as_str() {
        "constant" => SubframeType::Constant,
        "verbatim" => SubframeType::Verbatim,
        "fixed" => SubframeType::Fixed(v.order as u8),
        "lpc" => SubframeType::Lpc {
            order: v.order as u8,
        },
        other => panic!("{}: unknown kind {other:?}", v.label),
    }
}

fn frame_reader(v: &BodyVector) -> (BitReader<'_>, FrameHeader) {
    let mut reader = BitReader::new(&v.frame);
    let header = FrameHeader::parse(
        &mut reader,
        &StreamDefaults {
            sample_rate_hz: v.stream_samplerate_hz as u32,
            bits_per_sample: v.stream_bits_per_sample as u8,
        },
    )
    .unwrap_or_else(|e| panic!("{}: header parse failed: {e:?}", v.label));
    (reader, header)
}

/// The narrow leg, over every real body vector, against `flac -d` PCM.
#[test]
fn narrow_leg_matches_the_reference_decoder() {
    let vectors = load_body_vectors();
    for kind in ["fixed", "lpc"] {
        assert!(
            vectors.iter().any(|v| v.kind == kind),
            "body table has no {kind} vector — regenerate with scripts/gen_frame_vectors.sh"
        );
    }

    for v in &vectors {
        let what = format!("vector {}", v.label);
        let (mut reader, header) = frame_reader(v);
        let mut state = PredictorState::new();
        let mut out = vec![0i32; v.blocksize as usize];

        let (kind, width) = decode_subframe_with_width::<TableCountKernel>(
            &mut reader,
            header.blocksize,
            header.bits_per_sample,
            &mut state,
            &mut out,
        )
        .unwrap_or_else(|e| panic!("{what}: decode_subframe_with_width failed: {e:?}"));

        assert_eq!(kind, expected_type(v), "{what}: subframe type");
        assert_eq!(
            width,
            ArithmeticWidth::I32,
            "{what}: every committed vector is profile-compliant, so the guard must \
             select the narrow leg — an off-profile vector needs an explicit \
             expectation here rather than silent acceptance"
        );
        assert_eq!(
            reader.bit_position(),
            v.exit_bits as usize,
            "{what}: exit cursor"
        );
        let expect: Vec<i32> = v.pcm_samples.iter().map(|&x| x as i32).collect();
        assert_eq!(out, expect, "{what}: narrow samples != flac -d PCM");
    }
}

/// The guard's boundary, named rather than sampled: both the last safe value
/// and the first unsafe one, plus E4's two failing shapes.
#[test]
fn guard_boundary_is_exact() {
    // One coefficient, one-bit sample: magnitude == i32::MAX is the last fit.
    assert!(lpc_accumulator_fits_i32(&[i32::MAX], 1));
    // The same coefficient at a two-bit sample doubles the bound over.
    assert!(!lpc_accumulator_fits_i32(&[i32::MAX], 2));

    // E4's concrete safe profile: precision 13 (|c| <= 8191), 16-bit samples.
    assert!(lpc_accumulator_fits_i32(&[8191; 4], 16));
    // E4's failing shape: order 32 at precision 15 on the wider side slot.
    assert!(!lpc_accumulator_fits_i32(&[16383; 32], 17));

    // Degenerate inputs are refused, never trusted.
    assert!(lpc_accumulator_fits_i32(&[], 16));
    assert!(!lpc_accumulator_fits_i32(&[1], 0));
    assert!(!lpc_accumulator_fits_i32(&[1], 33));
}

/// A frame is wide if either subframe is.
#[test]
fn frame_report_is_wide_if_either_half_is() {
    use ArithmeticWidth::{I32, I64};
    assert_eq!(I32.and(I32), I32);
    assert_eq!(I32.and(I64), I64);
    assert_eq!(I64.and(I32), I64);
    assert_eq!(I64.and(I64), I64);
}
