//! Step 3f part 2a — the frame-RUN vectors, witnessed through the
//! primitives that already exist: `FrameHeader::parse` →
//! `decode_subframe` (with the measured bps+1 side seam) → `decorrelate` →
//! footer consume. `frame::decode_frame` (part 2b) must reproduce exactly
//! this composition — and this test is what proves the table's ground
//! truth before any production frame code can hide a bug inside it.
//!
//! Table: `tests/frame_run_vectors.txt` (regenerate:
//! `scripts/gen_frame_vectors.sh`). Five 2-frame runs (256 + 100 @ -b 256;
//! the 100 tail carries the uncommon 8-bit blocksize code), one per channel
//! assignment: mono, independent, left/side, side/right, mid/side. The
//! generator already enforced fail-closed at emit time (uniform channel
//! census, side slot walked at bps+1 against stored-slot reference values,
//! footer-gap arithmetic); this test re-derives all of it through the
//! crate's own accumulator reader — a second mechanism from the harness's
//! naive bit loop, same independence rule as every layout suite here.
//!
//! What each run witnesses, in order:
//!
//!   1. **chaining** — one `BitReader` walks frame N and lands exactly on
//!      frame N+1's first byte (header parse + subframes + `byte_align` +
//!      CRC-16 consume), so the footer arithmetic is pinned by cursor
//!      position, not by a re-scan;
//!   2. **the side seam is load-bearing** — decoding the side slot at any
//!      width but bps+1 must fail to reproduce ground truth (`side_slot`
//!      is read from the table, never re-derived from the mode);
//!   3. **stored vs final PCM as two ground-truth layers** — raw slot
//!      output must equal `slot_stored_N` (the STORED values: side = L−R,
//!      mid = (L+R)>>1) *before* `decorrelate`, and the decorrelated
//!      output must equal `pcm_left`/`pcm_right` from `flac -d` after —
//!      orientation and transform math pinned against real stereo frames
//!      end to end (step 3f part 1's unit oracle re-witnessed on encoder
//!      bytes);
//!   4. **the short tail frame** — blocksize 100 decodes through the
//!      uncommon 8-bit blocksize header with everything else unchanged.

use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::format::ChannelConfig;
use flac_lite::frame::{FrameHeader, StreamDefaults};
use flac_lite::stereo::decorrelate;
use flac_lite::subframe::{PredictorState, decode_subframe};

#[derive(Debug)]
struct RunVector {
    label: String,
    /// The run's bytes: every `frame_hex` row concatenated in order. The
    /// generator emits rows that cover each frame THROUGH its footer, so
    /// the concatenation is the stream's contiguous frame region.
    frame_bytes: Vec<u8>,
    /// Byte length of each frame, same order as the `frame_hex` rows.
    frame_lens: Vec<usize>,
    blocksizes: Vec<usize>,
    stream_samplerate_hz: u64,
    stream_bits_per_sample: u64,
    channels: usize,
    decorrelation: String,
    /// The decorrelated slot (the one coded at bps + 1): `None` for mono /
    /// independent, else the table's 0/1. Read, never re-derived.
    side_slot: Option<usize>,
    /// Whether the seam is observable on this run's side slot, per the
    /// generator's walk (`none` / `true` / `false`) — see the negative
    /// control below; the flag is checked against the reader, not trusted.
    side_width_sensitive: String,
    slot_stored: Vec<Vec<i64>>,
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
                decorrelation: String::new(),
                side_slot: None,
                side_width_sensitive: String::new(),
                slot_stored: Vec::new(),
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
            // frame_hex / frame_blocksize rows repeat, pairing by position.
            "frame_hex" => {
                let frame = hex_to_bytes(value);
                v.frame_lens.push(frame.len());
                v.frame_bytes.extend_from_slice(&frame);
            }
            "frame_blocksize" => v.blocksizes.push(number() as usize),
            "stream_samplerate_hz" => v.stream_samplerate_hz = number(),
            "stream_bits_per_sample" => v.stream_bits_per_sample = number(),
            "channels" => v.channels = number() as usize,
            "decorrelation" => v.decorrelation = value.to_string(),
            "side_slot" => {
                v.side_slot = match value {
                    "none" => None,
                    other => Some(
                        other
                            .parse::<usize>()
                            .unwrap_or_else(|e| panic!("{}: bad side_slot: {e}", v.label)),
                    ),
                }
            }
            "slot_stored_0" => {
                v.slot_stored.push(ints(value));
            }
            "slot_stored_1" => {
                v.slot_stored.push(ints(value));
            }
            "side_width_sensitive" => v.side_width_sensitive = value.to_string(),
            "pcm_left" => v.pcm_left = ints(value),
            "pcm_right" => v.pcm_right = ints(value),
            _ => {}
        }
    }
    for v in &out {
        assert_eq!(
            v.blocksizes.len(),
            v.frame_lens.len(),
            "{}: frame_blocksize/frame_hex row counts disagree",
            v.label
        );
    }
    assert!(!out.is_empty(), "run vector table contained no vectors");
    out
}

fn config(v: &RunVector) -> ChannelConfig {
    match v.decorrelation.as_str() {
        "mono" => ChannelConfig::Independent { channels: 1 },
        "independent" => ChannelConfig::Independent { channels: 2 },
        "left-side" => ChannelConfig::LeftSide,
        "side-right" => ChannelConfig::RightSide,
        "mid-side" => ChannelConfig::MidSide,
        other => panic!("{}: unknown decorrelation {other:?}", v.label),
    }
}

fn defaults(v: &RunVector) -> StreamDefaults {
    StreamDefaults {
        sample_rate_hz: v.stream_samplerate_hz as u32,
        bits_per_sample: v.stream_bits_per_sample as u8,
    }
}

/// Whole-run output: (slot 0 stored, slot 1 stored, final L, final R),
/// each concatenated over all frames.
type RunOut = (Vec<i32>, Vec<i32>, Vec<i32>, Vec<i32>);

/// Decode the whole run with the crate's primitives: parse header, decode
/// each slot with the side slot at `bps + side_offset` bits, decorrelate,
/// consume the footer. `side_offset == 1` is the measured seam; any other
/// value is a deliberate misread for the negative test.
///
/// The footer-consume cursor check returns `Err(InvalidField)` rather than
/// panicking: a wrong seam that still parses must *diverge* from ground
/// truth, and the negative test treats Err and mismatch identically.
fn run_decode(v: &RunVector, side_offset: i32) -> flac_lite::Result<RunOut> {
    let mut reader = BitReader::new(&v.frame_bytes);
    let cfg = config(v);
    let stereo = v.channels == 2;
    let mut state = [PredictorState::new(), PredictorState::new()];
    let mut stored0 = Vec::new();
    let mut stored1 = Vec::new();
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
        let mut out0 = vec![0i32; blocksize];
        let mut out1 = vec![0i32; if stereo { blocksize } else { 0 }];
        for slot in 0..v.channels {
            let is_side = v.side_slot == Some(slot);
            let bits = i32::from(header.bits_per_sample) + if is_side { side_offset } else { 0 };
            let out = if slot == 0 { &mut out0 } else { &mut out1 };
            decode_subframe(
                &mut reader,
                header.blocksize,
                bits as u8,
                &mut state[slot],
                out,
            )?;
        }
        stored0.extend_from_slice(&out0);
        if stereo {
            stored1.extend_from_slice(&out1);
        }
        decorrelate(cfg, blocksize, &mut out0, &mut out1)?;
        left_all.extend_from_slice(&out0);
        if stereo {
            right_all.extend_from_slice(&out1);
        }
        // Footer: pad to byte boundary, consume the CRC-16 (verification
        // is Phase 2 step 5 — consuming here is what the cursor contract
        // needs, and chaining to the next frame proves it was the right
        // number of bytes).
        reader.byte_align();
        reader.read_u8()?;
        reader.read_u8()?;
        consumed_bytes += frame_len;
        if reader.bit_position() != consumed_bytes * 8 {
            return Err(Error::InvalidField);
        }
    }
    if reader.bit_position() != v.frame_bytes.len() * 8 {
        return Err(Error::InvalidField);
    }
    Ok((stored0, stored1, left_all, right_all))
}

/// Witness 1 + 3 + 4: every run chains frame-to-frame through the crate's
/// reader, the raw subframe output equals the stored-slot ground truth, and
/// the decorrelated output equals `flac -d`'s PCM bit-exactly — including
/// the 100-sample uncommon-blocksize tail of every run.
#[test]
fn runs_chain_and_match_the_table_bit_exactly() {
    let vectors = load_run_vectors();
    for v in &vectors {
        let (stored0, stored1, left, right) = run_decode(v, 1)
            .unwrap_or_else(|e| panic!("{}: seam-correct run failed: {e:?}", v.label));
        let want0: Vec<i32> = v.slot_stored[0].iter().map(|&x| x as i32).collect();
        assert_eq!(
            stored0, want0,
            "{}: slot 0 raw output != stored ground truth (before decorrelate)",
            v.label
        );
        if v.channels == 2 {
            let want1: Vec<i32> = v.slot_stored[1].iter().map(|&x| x as i32).collect();
            assert_eq!(
                stored1, want1,
                "{}: slot 1 raw output != stored ground truth (before decorrelate)",
                v.label
            );
        }
        let left_want: Vec<i32> = v.pcm_left.iter().map(|&x| x as i32).collect();
        assert_eq!(left, left_want, "{}: decoded L != flac -d PCM", v.label);
        let right_want: Vec<i32> = v.pcm_right.iter().map(|&x| x as i32).collect();
        assert_eq!(right, right_want, "{}: decoded R != flac -d PCM", v.label);
    }
}

/// Witness 2: the side slot's bps+1 seam, witnessed exactly where the
/// format makes it observable — and pinned as inert where it is not.
///
/// The first draft demanded divergence on *every* decorrelated run and
/// fired on leftside-2f: its side slot is **FIXED order 0** on every frame
/// (tiny quantized noise — no predictor helps), and a FIXED-0 subframe
/// never consults frame bps at all: no warm-up reads (order × bits = 0),
/// Rice codewords are width-independent, `pad_block` shifts by the wasted
/// field. bps+0 there decodes bit-identical — the seam is un-witnessable by
/// grammar, not by table bug. The generator now derives a
/// `side_width_sensitive` flag from each side slot's measured kind/order,
/// and this test CHECKS the flag instead of trusting it:
///
///   * `true`  → reading the side at bps+0 or bps+2 must fail to reproduce
///     ground truth (Err or mismatch — either is divergence). This is the
///     seam witness proper, and because the +1 is applied to the slot the
///     TABLE names, it re-witnesses part 1's orientation claim through the
///     crate's reader: a swapped orientation mis-places the +1 and fails.
///   * `false` → the side at bps+0 AND bps+2 must reproduce ground truth
///     bit-exactly — the crate's own reader proving FIXED-0 width-invariance
///     end to end. A `false` vector that diverged would mean the generator
///     mislabeled a width-bearing side slot.
#[test]
fn side_seam_is_load_bearing_where_the_format_makes_it_so() {
    for v in load_run_vectors() {
        let Some(_side) = v.side_slot else {
            assert_eq!(
                v.side_width_sensitive, "none",
                "{}: non-decorrelated run with a sensitivity label",
                v.label
            );
            continue;
        };
        let matches_ground_truth = |offset: i32| -> bool {
            match run_decode(&v, offset) {
                Err(_) => false,
                Ok((s0, s1, l, r)) => {
                    let s0_want: Vec<i32> = v.slot_stored[0].iter().map(|&x| x as i32).collect();
                    let s1_want: Vec<i32> = v.slot_stored[1].iter().map(|&x| x as i32).collect();
                    let left_want: Vec<i32> = v.pcm_left.iter().map(|&x| x as i32).collect();
                    let right_want: Vec<i32> = v.pcm_right.iter().map(|&x| x as i32).collect();
                    s0 == s0_want && s1 == s1_want && l == left_want && r == right_want
                }
            }
        };
        match v.side_width_sensitive.as_str() {
            "true" => {
                for wrong_offset in [0i32, 2] {
                    assert!(
                        !matches_ground_truth(wrong_offset),
                        "{}: side slot read at bps+{wrong_offset} still reproduced \
                         ground truth — the seam witness proves nothing",
                        v.label
                    );
                }
            }
            "false" => {
                for inert_offset in [0i32, 2] {
                    assert!(
                        matches_ground_truth(inert_offset),
                        "{}: side_width_sensitive=false but bps+{inert_offset} \
                         diverged — the generator mislabeled a width-bearing \
                         side slot",
                        v.label
                    );
                }
            }
            other => panic!(
                "{}: side_width_sensitive = {other:?}, expected true/false",
                v.label
            ),
        }
    }
}

/// Tripwire for the witness above: at least one run must carry a
/// width-observable side slot. If a libFLAC upgrade moved every
/// decorrelated side to FIXED-0, the seam witness would silently degrade
/// to the inert-branch checks alone — this fires instead.

/// Table self-consistency at test time (the coverage tripwire of the other
/// layout suites): all five channel assignments present, and each vector's
/// `side_slot` label agrees with its `decorrelation` label — a generator
/// bug that mislabeled the seam could otherwise silently move the +1 onto
/// a slot that does not carry it.
#[test]
fn table_covers_every_mode_with_consistent_seams() {
    let vectors = load_run_vectors();
    assert!(
        vectors.iter().any(|v| v.side_width_sensitive == "true"),
        "run table has no width-observable side slot (mid-side's LPC side \
         is the seam witness) — regenerate and re-check the side_slot \
         sensitivity logic"
    );
    for mode in ["mono", "independent", "left-side", "side-right", "mid-side"] {
        assert!(
            vectors.iter().any(|v| v.decorrelation == mode),
            "run table has no {mode} vector — regenerate with \
             scripts/gen_frame_vectors.sh"
        );
    }
    for v in &vectors {
        let expected_side = match v.decorrelation.as_str() {
            "side-right" => Some(0),
            "left-side" | "mid-side" => Some(1),
            _ => None,
        };
        assert_eq!(
            v.side_slot, expected_side,
            "{}: side_slot label contradicts decorrelation",
            v.label
        );
        assert_eq!(
            v.channels,
            if v.decorrelation == "mono" { 1 } else { 2 },
            "{}: channel count contradicts decorrelation",
            v.label
        );
        // The run geometry itself: 256 + 100 (the uncommon-tail point).
        assert_eq!(
            v.blocksizes,
            vec![256, 100],
            "{}: run geometry drifted",
            v.label
        );
    }
}
