//! Phase 2 step 5 (2026-11-03) — the `crc-check`
//! witness suite. Runs only with `--features crc-check` (`required-features`
//! in Cargo.toml); the gate is `make flac-test-crc`.
//!
//! What this suite owns, and what it deliberately does not:
//!
//!   1. **Pass witness under the feature** — every committed run vector
//!      (real libFLAC 1.5.0 frames) parses AND decodes through production
//!      `FrameHeader::parse` + `decode_frame` with verification ON, with the
//!      same bit-exact PCM equality as the default-build suite. This is also
//!      the coverage witness for both CRCs: the stored CRC values come from
//!      the encoder, never from our own implementation, so the passes pin
//!      that our coverage windows are the encoder's coverage windows
//!      (header: sync through the byte before the CRC-8; footer: sync
//!      through the padding, both CRC bytes excluded).
//!   2. **CRC-8 mutation** — a real frame header with exactly ONE covered
//!      byte flipped (the 1-octet coded frame number `0 → 1`: a *valid*
//!      field value, so no field check can pre-empt the CRC compare).
//!      Decoding the mutated bytes must yield [`Error::HeaderCrc`]; the
//!      unmutated control must yield `Ok` (crate mutation rule: decode the
//!      mutated bytes, keep an identity control).
//!   3. **CRC-16 mutation** — a real frame with its stored footer word
//!      flipped (high byte, then low byte). Subframes decode fine; the
//!      compare must condemn: [`Error::FrameCrc`] with the unmutated
//!      control `Ok`. Together, tests 1–3 pin *both operands* of the
//!      footer compare: real stored values pass (operand = stored word),
//!      stored-value flips fail (operand = computed window), and the flip
//!      location (byte n−2 / n−1, inside the CRC field, outside the covered
//!      window) is exactly where a *stored*-value corruption lives.

#![cfg(feature = "crc-check")]

use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::frame::{FrameHeader, StreamDefaults, decode_frame};
use flac_lite::subframe::PredictorState;

#[derive(Debug)]
struct RunVector {
    label: String,
    frame_bytes: Vec<u8>,
    frame_lens: Vec<usize>,
    blocksizes: Vec<usize>,
    stream_samplerate_hz: u64,
    stream_bits_per_sample: u64,
    channels: usize,
    pcm_left: Vec<i64>,
    pcm_right: Vec<i64>,
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
    text.split_whitespace()
        .map(|v| {
            v.parse::<i64>()
                .unwrap_or_else(|e| panic!("not an integer {v:?}: {e}"))
        })
        .collect()
}

fn load_run_vectors() -> Vec<RunVector> {
    let table = include_str!("frame_run_vectors.txt");
    let mut out: Vec<RunVector> = Vec::new();

    for line in table.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let value = value.trim();
        if key == "run_vector" {
            out.push(RunVector {
                label: value.to_string(),
                frame_bytes: Vec::new(),
                frame_lens: Vec::new(),
                blocksizes: Vec::new(),
                stream_samplerate_hz: 0,
                stream_bits_per_sample: 0,
                channels: 0,
                pcm_left: Vec::new(),
                pcm_right: Vec::new(),
            });
            continue;
        }
        let v = out
            .last_mut()
            .unwrap_or_else(|| panic!("run field {key:?} outside a run_vector block"));
        let number = || {
            value
                .parse::<u64>()
                .unwrap_or_else(|e| panic!("{}: not a number {value:?}: {e}", v.label))
        };
        match key {
            "frame_hex" => {
                let frame = hex_to_bytes(value);
                v.frame_lens.push(frame.len());
                v.frame_bytes.extend_from_slice(&frame);
            }
            "frame_blocksize" => v.blocksizes.push(number() as usize),
            "stream_samplerate_hz" => v.stream_samplerate_hz = number(),
            "stream_bits_per_sample" => v.stream_bits_per_sample = number(),
            "channels" => v.channels = number() as usize,
            "pcm_left" => v.pcm_left = ints(value),
            "pcm_right" => v.pcm_right = ints(value),
            _ => {}
        }
    }
    assert!(!out.is_empty(), "run vector table contained no vectors");
    out
}

fn defaults(v: &RunVector) -> StreamDefaults {
    StreamDefaults {
        sample_rate_hz: v.stream_samplerate_hz as u32,
        bits_per_sample: v.stream_bits_per_sample as u8,
    }
}

/// Test 1: verification ON changes nothing about what is accepted or decoded
/// — every frame parses, every frame decodes bit-exactly, the cursor chains
/// frame N → N+1 exactly as in the default build.
#[test]
fn all_run_vectors_pass_with_crc_check_on() {
    for v in &load_run_vectors() {
        let stereo = v.channels == 2;
        let mut reader = BitReader::new(&v.frame_bytes);
        let mut state = [PredictorState::new(), PredictorState::new()];
        let mut left_all = Vec::new();
        let mut right_all = Vec::new();
        let mut consumed_bytes = 0usize;

        for (i, (&blocksize, &frame_len)) in v.blocksizes.iter().zip(&v.frame_lens).enumerate() {
            let header = FrameHeader::parse(&mut reader, &defaults(v)).unwrap_or_else(|e| {
                panic!(
                    "{} frame {i}: real header rejected under crc-check: {e:?}",
                    v.label
                )
            });
            let mut left = vec![0i32; blocksize];
            let mut right = vec![0i32; if stereo { blocksize } else { 0 }];
            decode_frame(&mut reader, &header, &mut left, &mut right, &mut state).unwrap_or_else(
                |e| {
                    panic!(
                        "{} frame {i}: real frame rejected under crc-check: {e:?}",
                        v.label
                    )
                },
            );
            left_all.extend_from_slice(&left);
            if stereo {
                right_all.extend_from_slice(&right);
            }
            consumed_bytes += frame_len;
            assert_eq!(
                reader.bit_position(),
                consumed_bytes * 8,
                "{} frame {i}: chaining moved under crc-check",
                v.label
            );
        }
        let left_want: Vec<i32> = v.pcm_left.iter().map(|&x| x as i32).collect();
        let right_want: Vec<i32> = v.pcm_right.iter().map(|&x| x as i32).collect();
        assert_eq!(
            left_all, left_want,
            "{}: L changed under crc-check",
            v.label
        );
        assert_eq!(
            right_all, right_want,
            "{}: R changed under crc-check",
            v.label
        );
    }
}

/// First frame of every run, as owned bytes. Frame 0's coded frame number is
/// the one-octet `0x00` in every committed run (a run starts at frame 0),
/// asserted here so the mutation below can rely on it.
fn first_frame(v: &RunVector) -> Vec<u8> {
    let f = v.frame_bytes[..v.frame_lens[0]].to_vec();
    assert_eq!(
        f[4], 0x00,
        "{}: frame 0 coded number must be the 1-octet 0",
        v.label
    );
    f
}

/// Test 2: CRC-8 over a real header with exactly ONE covered byte flipped.
/// The flip (coded number `0 → 1`) is a *valid field value*: every field
/// check passes, so the only thing left to condemn the frame is the CRC —
/// which must. The unmutated frame is the identity control.
#[test]
fn header_crc8_mutation_reports_header_crc() {
    for v in &load_run_vectors() {
        let clean = first_frame(v);
        // Identity control: the real frame parses under the feature.
        FrameHeader::parse(&mut BitReader::new(&clean), &defaults(v))
            .unwrap_or_else(|e| panic!("{}: control header rejected: {e:?}", v.label));

        let mut mutated = clean.clone();
        mutated[4] ^= 0x01; // frame number 0 -> 1: valid, covered, CRC now stale
        assert_eq!(
            FrameHeader::parse(&mut BitReader::new(&mut mutated[..]), &defaults(v)),
            Err(Error::HeaderCrc),
            "{}: stale header CRC-8 must be HeaderCrc",
            v.label
        );
    }
}

/// Test 3: CRC-16 stored-word mutation (high byte, then low byte). The
/// subframe bodies decode untouched; the footer compare must condemn with
/// [`Error::FrameCrc`]. Control: the untouched frame decodes `Ok` (covered
/// by test 1 as well, repeated here per the mutation-suite rule).
#[test]
fn footer_crc16_mutation_reports_frame_crc() {
    for v in &load_run_vectors() {
        for flip in [0u8, 1] {
            let mut bytes = v.frame_bytes.clone();
            let end = v.frame_lens[0];
            bytes[end - 2 + flip as usize] ^= 0x01;

            let mut reader = BitReader::new(&bytes);
            let header = FrameHeader::parse(&mut reader, &defaults(v))
                .expect("header is untouched; must parse");
            let blocksize = header.blocksize;
            let stereo = v.channels == 2;
            let mut left = vec![0i32; blocksize];
            let mut right = vec![0i32; if stereo { blocksize } else { 0 }];
            let mut state = [PredictorState::new(), PredictorState::new()];
            assert_eq!(
                decode_frame(&mut reader, &header, &mut left, &mut right, &mut state),
                Err(Error::FrameCrc),
                "{}: flipped stored CRC-16 byte {flip} must be FrameCrc",
                v.label
            );
        }
    }
}
