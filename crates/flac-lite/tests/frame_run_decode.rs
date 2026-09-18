//! Step 3f part 2b — the production frame decode, `frame::decode_frame`,
//! against the same committed run table whose ground truth
//! `frame_run_layout.rs` witnessed through the primitives. If production and
//! witness agree on every run, `decode_frame` is exactly the composition:
//! subframe loop with the bps+1 side seam → `decorrelate` → footer consume.
//!
//! Table: `tests/frame_run_vectors.txt` (regenerate:
//! `scripts/gen_frame_vectors.sh`). Five 2-frame runs (256 + 100 @ -b 256),
//! one per channel assignment. The layout suite owns the seam negative
//! control and the stored-slot diff (only the primitive composition can
//! express those); this suite owns what only the production driver can show:
//!
//!   1. **chaining through `decode_frame`** — one `BitReader`, frame N's
//!      decode lands exactly on frame N+1's first byte (the footer consume
//!      is cursor-pinned, frame after frame);
//!   2. **bit-exact whole-run PCM** — decorrelated `left`/`right` equal
//!      `flac -d`'s output on every sample of every run, including the
//!      100-sample uncommon-blocksize tail;
//!   3. **the reused-buffer tail contract** — the 100-sample tail decoded
//!      into a full 256-slot buffer must leave slots 100..256 untouched;
//!      playback reuses one fixed buffer per channel, so a driver that
//!      cleared or wrote the tail would corrupt the *next* frame's head
//!      (and this frame's tail of a reused buffer's previous contents);
//!   4. **caller-contract rejection before the first bit** — short buffers
//!       return `InvalidField` with the cursor unmoved and nothing written.

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

/// Decode a whole run through production `decode_frame`, chaining one
/// reader frame after frame. Returns (left, right) concatenated over the
/// run; `right` is empty for mono. Asserts the cursor lands on each frame's
/// byte end after every call — the chaining witness is part of the driver,
/// not an afterthought.
fn run_production(v: &RunVector) -> (Vec<i32>, Vec<i32>) {
    let mut reader = BitReader::new(&v.frame_bytes);
    let stereo = v.channels == 2;
    let mut state = [PredictorState::new(), PredictorState::new()];
    let mut left_all = Vec::new();
    let mut right_all = Vec::new();
    let mut consumed_bytes = 0usize;

    for (i, (&blocksize, &frame_len)) in v.blocksizes.iter().zip(&v.frame_lens).enumerate() {
        let header = FrameHeader::parse(&mut reader, &defaults(v))
            .unwrap_or_else(|e| panic!("{} frame {i}: header parse failed: {e:?}", v.label));
        assert_eq!(
            header.blocksize, blocksize,
            "{} frame {i}: table blocksize disagrees with the parsed header",
            v.label
        );
        let mut left = vec![0i32; blocksize];
        let mut right = vec![0i32; if stereo { blocksize } else { 0 }];
        let written = decode_frame(&mut reader, &header, &mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("{} frame {i}: decode_frame failed: {e:?}", v.label));
        assert_eq!(
            written, blocksize,
            "{} frame {i}: decode_frame returned {written}, expected the blocksize",
            v.label
        );
        left_all.extend_from_slice(&left);
        if stereo {
            right_all.extend_from_slice(&right);
        }
        consumed_bytes += frame_len;
        assert_eq!(
            reader.bit_position(),
            consumed_bytes * 8,
            "{} frame {i}: cursor did not land on the next frame's first byte \
             — the footer consume is the wrong width",
            v.label
        );
    }
    assert_eq!(
        reader.bit_position(),
        v.frame_bytes.len() * 8,
        "{}: run did not consume the frame region exactly",
        v.label
    );
    (left_all, right_all)
}

/// Witnesses 1 + 2: production chains frame-to-frame through the crate's
/// reader and its decorrelated output equals `flac -d`'s PCM bit-exactly on
/// every run — including the 100-sample uncommon-blocksize tail.
#[test]
fn decode_frame_matches_the_run_table_bit_exactly() {
    for v in &load_run_vectors() {
        let (left, right) = run_production(v);
        let left_want: Vec<i32> = v.pcm_left.iter().map(|&x| x as i32).collect();
        assert_eq!(
            left, left_want,
            "{}: decode_frame L != flac -d PCM",
            v.label
        );
        let right_want: Vec<i32> = v.pcm_right.iter().map(|&x| x as i32).collect();
        assert_eq!(
            right, right_want,
            "{}: decode_frame R != flac -d PCM",
            v.label
        );
    }
}

/// Witness 3: the tail contract on the frame where it matters — the
/// 100-sample tail decoded into a full 256-slot reused buffer. Slots
/// beyond blocksize must survive: playback reuses one fixed buffer per
/// channel across frames, so `decode_frame` writing or clearing past
/// `blocksize` corrupts whatever the buffer's next use reads.
#[test]
fn short_tail_frame_leaves_the_reused_buffer_tail_untouched() {
    const SENTINEL: i32 = 0x5A5A_5A5A;
    for v in &load_run_vectors() {
        let stereo = v.channels == 2;
        let mut reader = BitReader::new(&v.frame_bytes);
        let mut state = [PredictorState::new(), PredictorState::new()];

        // Frame 0 fills the buffer fully; frame 1 is the 100-sample tail.
        let header0 = FrameHeader::parse(&mut reader, &defaults(v)).unwrap();
        let mut left = [SENTINEL; 256];
        let mut right = [SENTINEL; 256];
        let n0 = decode_frame(&mut reader, &header0, &mut left, &mut right, &mut state).unwrap();
        assert_eq!(n0, 256, "{}: first frame should be full", v.label);
        let after_head = left; // every slot written; snapshot for mono below

        let header1 = FrameHeader::parse(&mut reader, &defaults(v)).unwrap();
        assert_eq!(header1.blocksize, 100, "{}: tail frame geometry", v.label);
        let mut left = [SENTINEL; 256];
        let mut right = [SENTINEL; 256];
        let n1 = decode_frame(&mut reader, &header1, &mut left, &mut right, &mut state).unwrap();
        assert_eq!(n1, 100, "{}: tail frame sample count", v.label);

        // The 100 written samples are the run's final PCM...
        let off = 256; // frame 0's sample count
        for i in 0..100 {
            assert_eq!(
                left[i],
                v.pcm_left[off + i] as i32,
                "{}: tail L[{i}] != flac -d PCM",
                v.label
            );
            if stereo {
                assert_eq!(
                    right[i],
                    v.pcm_right[off + i] as i32,
                    "{}: tail R[{i}] != flac -d PCM",
                    v.label
                );
            }
        }
        // ...and slots 100..256 are exactly the sentinel — untouched.
        for i in 100..256 {
            assert_eq!(
                left[i], SENTINEL,
                "{}: decode_frame wrote past blocksize into the reused \
                 left buffer at tail L[{i}]",
                v.label
            );
            if stereo {
                assert_eq!(
                    right[i], SENTINEL,
                    "{}: decode_frame wrote past blocksize into the reused \
                     right buffer at tail R[{i}]",
                    v.label
                );
            }
        }
        // Frame 0 filled every left slot (otherwise the sentinel logic above
        // is vacuous for the full-frame case): a full decode writes all
        // blocksize samples, and 16-bit PCM can never *equal* the sentinel,
        // so "no sentinel survives frame 0" is the contract check proper.
        assert!(
            after_head.iter().all(|&x| x != SENTINEL),
            "{}: a full-frame decode left a sentinel slot in the left buffer",
            v.label
        );
    }
}

/// Witness 4: caller-contract rejections before the first bit — short
/// buffers return `InvalidField` with the cursor exactly where it started
/// and nothing written (crate rejection rule: rejections read and write
/// nothing).
#[test]
fn short_buffers_reject_before_the_first_bit() {
    for v in &load_run_vectors() {
        let mut reader = BitReader::new(&v.frame_bytes);
        let mut state = [PredictorState::new(), PredictorState::new()];
        let header = FrameHeader::parse(&mut reader, &defaults(v)).unwrap();
        let blocksize = header.blocksize;
        let stereo = v.channels == 2;

        // left one sample short.
        let start = reader.bit_position();
        let mut left = vec![0i32; blocksize - 1];
        let mut right = vec![0i32; if stereo { blocksize } else { 0 }];
        assert!(matches!(
            decode_frame(&mut reader, &header, &mut left, &mut right, &mut state),
            Err(Error::InvalidField)
        ));
        assert_eq!(
            reader.bit_position(),
            start,
            "{}: rejection moved the cursor",
            v.label
        );
        assert!(
            left.iter().all(|&x| x == 0),
            "{}: rejection wrote samples",
            v.label
        );

        // right one sample short — only meaningful for two-subframe modes;
        // mono legitimately passes a zero-length right (witnessed by the
        // mono run decoding Ok in the suite above).
        if stereo {
            let mut left = vec![0i32; blocksize];
            let mut right = vec![0i32; blocksize - 1];
            assert!(matches!(
                decode_frame(&mut reader, &header, &mut left, &mut right, &mut state),
                Err(Error::InvalidField)
            ));
            assert_eq!(
                reader.bit_position(),
                start,
                "{}: rejection moved the cursor",
                v.label
            );
            assert!(
                left.iter().all(|&x| x == 0),
                "{}: rejection wrote left samples",
                v.label
            );
            assert!(
                right.iter().all(|&x| x == 0),
                "{}: rejection wrote right samples",
                v.label
            );
        }
    }
}
