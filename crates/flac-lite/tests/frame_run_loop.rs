//! The crate's **frame loop** (`driver`) over the same committed run table
//! whose ground truth `frame_run_decode.rs` witnessed through the frame layer.
//!
//! `frame_run_decode.rs` drives those real libFLAC frames with a per-test
//! single `BitReader`, chained frame after frame. This suite drives the same
//! bytes through `driver::decode_all` / `driver::decode_one`: a frame index
//! whose offsets are derived from the table's own per-frame row lengths, and
//! one **fresh cursor per frame at its index offset**. Two ways of reaching
//! the same bytes — a chained cursor and a table-driven seek — that must
//! agree, and both must agree with the reference decoder.
//!
//! Table: `tests/frame_run_vectors.txt` (regenerate:
//! `scripts/gen_frame_vectors.sh`). Five 2-frame runs (256 + 100 @ -b 256),
//! one per channel assignment. Ground truth unchanged: `frame_hex` rows are
//! pinned by the generator to libFLAC's exact output, and `pcm_left`/
//! `pcm_right` are `flac -d`'s — a decoder independent of this one. Nothing
//! here hand-packs a frame (the hand-packed-vector rule, FLAC.md).
//!
//! Layers, each pinning a different failure mode:
//!
//!   0. **the index tiles the region's own frame boundaries** — setup check:
//!      the offsets this suite builds come from the table's documented row
//!      framing (each `frame_hex` row covers its frame through its footer, to
//!      the next frame's first byte), so they are encoder output, not guesses;
//!   1. **bit-exact whole-run walk** — the loop's concatenated output equals
//!      `flac -d`'s PCM on every run, mono and stereo, with the loop's totals
//!      and per-frame callback window widths matching the table;
//!   2. **per-frame independence, in reverse order** — every frame decoded at
//!      its own index offset, newest first, through ONE reused
//!      `[PredictorState; 2]`: the seek claim stated where it actually
//!      matters (a seek lands on an arbitrary frame, and in walk order its
//!      "predecessor" is the frame that comes *after* it in the stream).
//!      Warm-up is per-subframe (measured 64/64, 2026-09-10), so a reversed
//!      walk must be indistinguishable from a forward one;
//!   3. **the reused-buffer tail contract at loop level** — a short frame
//!      writes its own blocksize and leaves the slots past it exactly as the
//!      *previous* frame left them (not zeros, not cleared): the property that
//!      lets a player hold one fixed buffer per channel while the final frame
//!      is legitimately short;
//!   4. **index/caller rejections name their evidence and write nothing** —
//!      an index disagreeing with the bytes it indexes (`IndexMismatch`,
//!      `FrameOffsetOutOfRange`) and short caller buffers (`BufferTooSmall`)
//!      each fail before a sample is written, carrying the numbers a witness
//!      needs, each with a same-source control;
//!   5. **mono through the whole loop** — the zero-length `right` is the
//!      contract end to end, and the callback's right window is legitimately
//!      empty rather than a panic.

use flac_lite::driver::{self, FrameMeta, FrameSource};
use flac_lite::frame::StreamDefaults;

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

impl RunVector {
    fn stereo(&self) -> bool {
        self.channels == 2
    }

    /// The largest per-frame blocksize, DERIVED from the per-frame rows: the
    /// run table carries no `max_blocksize` field at all (its keys are
    /// `frame_blocksize` per frame, `frame_count`, ...). That is the honest
    /// shape of the claim `FrameSource::max_blocksize` makes — whoever builds
    /// a frame source derives it from the frames, and witness 0 checks the
    /// derived value against what the runs actually are (uniform blocksizes,
    /// short tail). A source that carried a stale maximum would either reject
    /// good buffers or let short ones through and be caught at the frame level
    /// instead.
    fn max_blocksize(&self) -> usize {
        self.blocksizes
            .iter()
            .max()
            .copied()
            .expect("a run has at least one frame")
    }

    /// A frame index built from the table's own per-frame row lengths: frame 0
    /// at offset 0, each next frame where the previous row ends. Witness 0
    /// checks the result tiles the region — see the module header.
    fn index(&self) -> Vec<FrameMeta> {
        let mut offset = 0u32;
        let mut frames = Vec::with_capacity(self.frame_lens.len());
        for (i, &len) in self.frame_lens.iter().enumerate() {
            frames.push(FrameMeta {
                offset,
                blocksize: u16::try_from(self.blocksizes[i]).expect("blocksize fits u16"),
            });
            offset += u32::try_from(len).expect("frame row length fits u32");
        }
        frames
    }

    /// The decode inputs a frame source needs — the same values a packer would
    /// emit alongside the frames it just encoded.
    fn source<'a>(&'a self, frames: &'a [FrameMeta]) -> FrameSource<'a> {
        FrameSource {
            frames,
            region: &self.frame_bytes,
            max_blocksize: u16::try_from(self.max_blocksize()).expect("max blocksize fits u16"),
            channels: u16::try_from(self.channels).expect("channel count fits u8") as u8,
            defaults: StreamDefaults {
                sample_rate_hz: u32::try_from(self.stream_samplerate_hz).expect("sample rate"),
                bits_per_sample: u8::try_from(self.stream_bits_per_sample).expect("bit depth"),
            },
        }
    }

    /// Sample offset of each frame in the decoded stream: prefix sums of the
    /// per-frame blocksizes.
    fn starts(&self) -> Vec<usize> {
        let mut starts = Vec::with_capacity(self.blocksizes.len());
        let mut at = 0usize;
        for &block in &self.blocksizes {
            starts.push(at);
            at += block;
        }
        starts
    }

    fn total_samples(&self) -> usize {
        self.blocksizes.iter().sum()
    }
}

/// Witness 0 (setup, not a claim about the decoder): the index built above
/// tiles the region the table's rows concatenate to, the generator's
/// the derived maximum matches the runs' actual shape (uniform blocksizes,
/// short tail).
#[test]
fn the_index_tiles_the_regions_own_frame_boundaries() {
    for v in &load_run_vectors() {
        let frames = v.index();
        assert_eq!(
            frames.len(),
            v.blocksizes.len(),
            "{}: one index entry per frame row",
            v.label
        );
        assert_eq!(
            frames[0].offset, 0,
            "{}: frame 0 starts the region",
            v.label
        );

        let mut cursor = 0usize;
        for (i, frame) in frames.iter().enumerate() {
            assert_eq!(
                usize::try_from(frame.offset).expect("index offset fits usize"),
                cursor,
                "{}: index entry {i} offset",
                v.label
            );
            assert_eq!(
                usize::from(frame.blocksize),
                v.blocksizes[i],
                "{}: index entry {i} blocksize",
                v.label
            );
            cursor += v.frame_lens[i];
        }
        assert_eq!(
            cursor,
            v.frame_bytes.len(),
            "{}: the frame rows tile the region exactly (no gap, no straddle)",
            v.label
        );

        let max = v.max_blocksize();
        for (i, &block) in v.blocksizes.iter().enumerate() {
            if i + 1 < v.blocksizes.len() {
                assert_eq!(block, max, "{}: non-final short frame", v.label);
            }
        }
        assert!(
            v.blocksizes.len() < 2 || *v.blocksizes.last().unwrap() < max,
            "{}: the tail frame must be the short one",
            v.label
        );
    }
}

/// Witness 1: the loop's whole-run output is bit-exact against `flac -d`, its
/// totals agree with the table, and the per-frame callback windows have the
/// widths the loop promises its consumer.
#[test]
fn the_frame_loop_walks_a_run_bit_exactly() {
    for v in &load_run_vectors() {
        let frames = v.index();
        let source = v.source(&frames);
        let max = v.max_blocksize();
        let stereo = v.stereo();
        let mut left = vec![0i32; max];
        let mut right = if stereo { vec![0i32; max] } else { Vec::new() };
        let mut state = Default::default();
        let mut got_left: Vec<i32> = Vec::new();
        let mut got_right: Vec<i32> = Vec::new();

        let stats =
            driver::decode_all(&source, &mut left, &mut right, &mut state, |frame, l, r| {
                assert_eq!(
                    l.len(),
                    v.blocksizes[frame],
                    "{}: frame {frame} left window width",
                    v.label
                );
                assert_eq!(
                    r.len(),
                    if stereo { l.len() } else { 0 },
                    "{}: frame {frame} right window width",
                    v.label
                );
                got_left.extend_from_slice(l);
                got_right.extend_from_slice(r);
            })
            .unwrap_or_else(|e| panic!("{}: frame walk failed: {e:?}", v.label));

        assert_eq!(stats.frames, v.blocksizes.len(), "{}: frame count", v.label);
        assert_eq!(
            stats.samples,
            v.total_samples(),
            "{}: sample total",
            v.label
        );
        let widen = |v: &[i32]| -> Vec<i64> { v.iter().map(|&s| i64::from(s)).collect() };
        assert_eq!(
            widen(&got_left),
            v.pcm_left,
            "{}: the frame loop's left channel is not bit-exact against `flac -d`",
            v.label
        );
        assert_eq!(
            widen(&got_right),
            v.pcm_right,
            "{}: the frame loop's right channel is not bit-exact against `flac -d`",
            v.label
        );
    }
}

/// Witness 2: every frame decodes correctly at its own index offset, walked in
/// reverse, through one reused state array. That combination is the seek claim
/// at the only level where it matters: a seek lands on an arbitrary frame
/// whose walk-order predecessor is the frame *after* it in the stream.
#[test]
fn every_frame_decodes_at_its_offset_in_reverse_order() {
    for v in &load_run_vectors() {
        let frames = v.index();
        let source = v.source(&frames);
        let starts = v.starts();
        let max = v.max_blocksize();
        let stereo = v.stereo();
        // ONE state array across the whole reversed sequence: reused scratch,
        // never cross-frame history (the 2026-09-10 correction, at loop level).
        let mut state = Default::default();

        for frame in (0..frames.len()).rev() {
            let block = v.blocksizes[frame];
            // Fresh buffers per frame here, so a stale value can never be
            // mistaken for a fresh decode (witness 3 owns that contract).
            let mut left = vec![!0i32; max];
            let mut right = if stereo { vec![!0i32; max] } else { Vec::new() };
            let written = driver::decode_one(&source, frame, &mut left, &mut right, &mut state)
                .unwrap_or_else(|e| {
                    panic!("{}: reverse-walk frame {frame} failed: {e:?}", v.label)
                });
            assert_eq!(written, block, "{}: frame {frame} written count", v.label);
            let widen = |v: &[i32]| -> Vec<i64> { v.iter().map(|&s| i64::from(s)).collect() };
            assert_eq!(
                widen(&left[..block]),
                v.pcm_left[starts[frame]..starts[frame] + block],
                "{}: frame {frame} left samples (reverse walk)",
                v.label
            );
            if stereo {
                assert_eq!(
                    widen(&right[..block]),
                    v.pcm_right[starts[frame]..starts[frame] + block],
                    "{}: frame {frame} right samples (reverse walk)",
                    v.label
                );
            }
        }
    }
}

/// Witness 3: the loop's reused-buffer contract, on the frame pair the table
/// is built for (256 then 100 at -b 256). The tail frame writes 100 samples,
/// and the slots past it keep what the FIRST frame wrote there — not zeros,
/// not the tail's own values, not a cleared buffer.
#[test]
fn a_short_frame_leaves_the_reused_buffers_tail_alone() {
    for v in &load_run_vectors() {
        let frames = v.index();
        assert_eq!(frames.len(), 2, "{}: this suite's frame pair", v.label);
        let source = v.source(&frames);
        let starts = v.starts();
        let max = v.max_blocksize();
        let block0 = v.blocksizes[0];
        let block1 = v.blocksizes[1];
        assert_eq!(block0, max, "{}: the first frame fills the buffer", v.label);
        assert!(block1 < max, "{}: the tail frame must be short", v.label);

        let stereo = v.stereo();
        let mut left = vec![0i32; max];
        let mut right = if stereo { vec![0i32; max] } else { Vec::new() };
        let mut state = Default::default();
        let widen = |v: &[i32]| -> Vec<i64> { v.iter().map(|&s| i64::from(s)).collect() };

        // Frame 0 fills every slot — which is what makes the next assertion
        // mean something: the tail it leaves behind is real signal.
        driver::decode_one(&source, 0, &mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("{}: frame 0: {e:?}", v.label));
        assert_eq!(
            widen(&left),
            v.pcm_left[..max],
            "{}: frame 0 should have filled the buffer",
            v.label
        );
        let left_tail: Vec<i64> = widen(&left[block1..]);
        // A mono source's `right` is a zero-length slice (the contract), and
        // `right[block1..]` would panic on an empty slice: a mono stream has
        // no right tail to keep, so there is nothing to snapshot.
        let right_tail: Vec<i64> = if stereo {
            widen(&right[block1..])
        } else {
            Vec::new()
        };

        // Frame 1 into the SAME buffers.
        driver::decode_one(&source, 1, &mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("{}: frame 1: {e:?}", v.label));

        assert_eq!(
            widen(&left[..block1]),
            v.pcm_left[starts[1]..starts[1] + block1],
            "{}: the tail frame's own samples",
            v.label
        );
        assert_eq!(
            widen(&left[block1..]),
            left_tail,
            "{}: the short tail frame wrote past its blocksize (left) — the \
             slots past it must still hold what frame 0 wrote",
            v.label
        );
        if stereo {
            assert_eq!(
                widen(&right[block1..]),
                right_tail,
                "{}: the short tail frame wrote past its blocksize (right)",
                v.label
            );
        }
    }
}

/// Witness 4a: an index whose per-frame blocksize disagrees with the frame's
/// own header is rejected, names both numbers, and writes nothing — in BOTH
/// directions. The packer emits the table and the frames from one encode, so a
/// disagreement means the pair was edited afterwards — which is exactly what
/// this check is for (and exactly how this suite induces it: by editing the
/// table by hand, which is the only way a disagreement can arise for material
/// the generator pinned).
///
/// The under-claim half was earned by mutation, not by argument: with the
/// check written `claimed > header`, the over-claim case alone still fails and
/// the suite stays green while an index claiming 100 for a 256-sample frame
/// decodes, reports the header's 256, and drifts every total the loop returns.
#[test]
fn an_index_that_disagrees_with_the_frames_is_rejected_with_evidence() {
    for v in &load_run_vectors() {
        let mut frames = v.index();
        let last = frames.len() - 1;
        // Over-claim with the MAX blocksize: true for every non-final frame,
        // false for the tail. The check runs after the header parse but before
        // the decode, so no sample is written either way.
        let claimed = v.max_blocksize();
        frames[last].blocksize = u16::try_from(claimed).expect("fits u16");
        let source = v.source(&frames);

        let max = v.max_blocksize();
        let stereo = v.stereo();
        let mut left = vec![0x5A5A_5A5Ai32; max];
        let mut right = if stereo {
            vec![0x5A5A_5A5Ai32; max]
        } else {
            Vec::new()
        };
        let mut state = Default::default();
        let err = driver::decode_one(&source, last, &mut left, &mut right, &mut state)
            .expect_err("an index that over-claims blocksize must not decode");
        match err {
            driver::Error::IndexMismatch {
                frame,
                claimed: got_claimed,
                header,
            } => {
                assert_eq!(
                    (frame, got_claimed, header),
                    (last, claimed, v.blocksizes[last]),
                    "{}: IndexMismatch evidence fields",
                    v.label
                );
            }
            other => panic!("{}: expected IndexMismatch, got {other:?}", v.label),
        }
        assert_eq!(
            left,
            vec![0x5A5A_5A5Ai32; max],
            "{}: the rejection wrote into the caller buffer",
            v.label
        );

        // Control, same run and same buffers: the untouched index decodes that
        // frame, and returns the header's blocksize, not the table's claim.
        let good = v.index();
        let good_source = v.source(&good);
        let ok = driver::decode_one(&good_source, last, &mut left, &mut right, &mut state)
            .expect("control: the pristine index must decode the same frame");
        assert_eq!(
            ok, v.blocksizes[last],
            "{}: control frame's blocksize",
            v.label
        );

        // (b) The other direction: an index that UNDER-claims, induced on the
        // FIRST frame by claiming the tail frame's (smaller) blocksize. A
        // short claim is the dangerous one: read alone, `claimed < header`
        // still leaves every buffer long enough, so a check that only looks
        // one way lets the frame decode, report the header's larger blocksize,
        // and desynchronise the loop's own totals from its index.
        let under = v.blocksizes[last];
        let mut frames = v.index();
        frames[0].blocksize = u16::try_from(under).expect("fits u16");
        assert!(
            under < v.blocksizes[0],
            "{}: fixture must under-claim",
            v.label
        );
        let source = v.source(&frames);
        let mut left = vec![0x5A5A_5A5Ai32; max];
        let mut right = if stereo {
            vec![0x5A5A_5A5Ai32; max]
        } else {
            Vec::new()
        };
        let err = driver::decode_one(&source, 0, &mut left, &mut right, &mut state)
            .expect_err("an index that under-claims blocksize must not decode");
        match err {
            driver::Error::IndexMismatch {
                frame,
                claimed: got_claimed,
                header,
            } => {
                assert_eq!(
                    (frame, got_claimed, header),
                    (0, under, v.blocksizes[0]),
                    "{}: IndexMismatch evidence fields (under-claim)",
                    v.label
                );
            }
            other => panic!("{}: expected IndexMismatch, got {other:?}", v.label),
        }
        assert_eq!(
            left,
            vec![0x5A5A_5A5Ai32; max],
            "{}: the under-claim rejection wrote into the caller buffer",
            v.label
        );

        // Control: the pristine index still decodes frame 0 to its full width.
        let ok = driver::decode_one(&good_source, 0, &mut left, &mut right, &mut state)
            .expect("control: the pristine index must decode frame 0");
        assert_eq!(
            ok, v.blocksizes[0],
            "{}: control frame 0 blocksize",
            v.label
        );
    }
}

/// Witness 4b: an index offset PAST the end of the region means the index and
/// the region are not a matching pair. Note the boundary the distinction
/// needs: `offset == region.len()` is a *valid* empty slice, so the index has
/// nothing to complain about and the header parse is what rejects it —
/// `Error::EndOfStream`, the framing layer's "there are no bytes here" signal
/// (`bits::BitReader::read_bits` reports `EndOfStream` when fewer bits remain
/// than requested; the sync test never runs on empty input, which is why this
/// case is NOT `FrameSync`). Asserted separately below so neither behaviour is
/// an accident of the test data — and worth pinning precisely because the
/// distinction is what any future end-of-stream rule has to rest on:
/// `EndOfStream` says "no bytes left", `FrameSync` says "bytes left, and they
/// are not a frame start". Only the first one terminates a stream.
#[test]
fn an_index_offset_past_the_region_is_rejected_with_evidence() {
    for v in &load_run_vectors() {
        let max = v.max_blocksize();
        let stereo = v.stereo();
        let last = v.blocksizes.len() - 1;
        let mut state = Default::default();

        // (a) offset PAST the region: index/region pair mismatch.
        let mut frames = v.index();
        frames[last].offset = u32::try_from(v.frame_bytes.len() + 1).expect("offset");
        let source = v.source(&frames);
        let mut left = vec![0x5A5A_5A5Ai32; max];
        let mut right = if stereo {
            vec![0x5A5A_5A5Ai32; max]
        } else {
            Vec::new()
        };
        let err = driver::decode_one(&source, last, &mut left, &mut right, &mut state)
            .expect_err("an offset past the region must not decode");
        match err {
            driver::Error::FrameOffsetOutOfRange { frame, offset, len } => {
                assert_eq!(
                    (frame, offset, len),
                    (last, v.frame_bytes.len() + 1, v.frame_bytes.len()),
                    "{}: FrameOffsetOutOfRange evidence fields",
                    v.label
                );
            }
            other => panic!("{}: expected FrameOffsetOutOfRange, got {other:?}", v.label),
        }
        assert_eq!(
            left,
            vec![0x5A5A_5A5Ai32; max],
            "{}: the rejection wrote into the caller buffer",
            v.label
        );

        // (b) offset exactly AT the region end: a legal empty slice, so the
        // header parse is what rejects it. Asserted, not assumed.
        let mut frames = v.index();
        frames[last].offset = u32::try_from(v.frame_bytes.len()).expect("offset");
        let source = v.source(&frames);
        let mut left = vec![0i32; max];
        let mut right = if stereo { vec![0i32; max] } else { Vec::new() };
        let err = driver::decode_one(&source, last, &mut left, &mut right, &mut state)
            .expect_err("a frame starting past every byte must not decode");
        match err {
            driver::Error::Decode(flac_lite::Error::EndOfStream) => {}
            other => panic!(
                "{}: expected Decode(EndOfStream) at the region end, got {other:?}",
                v.label
            ),
        }
    }
}

/// Witness 4c: the caller-contract gate stops a whole walk before it starts —
/// the callback never runs and nothing is written. This is the loop-level form
/// of the crate's "rejections read and write nothing" rule (why
/// `bits::BitReader::try_read_u` exists at all: an eager read consumed past
/// EOF and reported only `FrameSync`, hiding the caller error behind a
/// framing claim — see the crc-check entry, 2026-10-25).
#[test]
fn a_short_caller_buffer_stops_the_walk_before_any_bit_is_read() {
    for v in &load_run_vectors() {
        let frames = v.index();
        let source = v.source(&frames);
        let max = v.max_blocksize();
        let stereo = v.stereo();
        let mut left = vec![0x5A5A_5A5Ai32; max - 1];
        let mut right = if stereo {
            vec![0x5A5A_5A5Ai32; max]
        } else {
            Vec::new()
        };
        let mut state = Default::default();
        let mut called = 0usize;
        let err = driver::decode_all(&source, &mut left, &mut right, &mut state, |_, _, _| {
            called += 1;
        })
        .expect_err("a short caller buffer must stop the walk");
        match err {
            driver::Error::BufferTooSmall { need, have } => {
                assert_eq!(
                    (need, have),
                    (max, max - 1),
                    "{}: BufferTooSmall evidence fields",
                    v.label
                );
            }
            other => panic!("{}: expected BufferTooSmall, got {other:?}", v.label),
        }
        assert_eq!(
            called, 0,
            "{}: the callback ran despite a rejection",
            v.label
        );
        assert_eq!(
            left,
            vec![0x5A5A_5A5Ai32; max - 1],
            "{}: the rejection wrote into the caller buffer",
            v.label
        );
    }
}

/// Witness 4d: a stereo source with a short `right` buffer is rejected too —
/// the `channels == 2` arm of the gate, on real stereo frames. (The module's
/// own unit tests can only reach this with a synthetic region, which claims
/// nothing about encoder bytes; the hand-packed-vector rule applies to
/// negative controls as much as positive ones.)
#[test]
fn a_short_right_buffer_is_rejected_on_a_stereo_run() {
    for v in &load_run_vectors() {
        if !v.stereo() {
            continue;
        }
        let frames = v.index();
        let source = v.source(&frames);
        let max = v.max_blocksize();
        let mut left = vec![0i32; max];
        let mut right = vec![0i32; max - 1];
        let mut state = Default::default();
        let err = driver::decode_one(&source, 0, &mut left, &mut right, &mut state)
            .expect_err("a short right buffer must not decode a stereo frame");
        match err {
            driver::Error::BufferTooSmall { need, have } => {
                assert_eq!((need, have), (max, max - 1), "{}: evidence fields", v.label);
            }
            other => panic!("{}: expected BufferTooSmall, got {other:?}", v.label),
        }
    }
}

/// Witness 5: mono through the whole loop — the zero-length `right` is the
/// documented contract end to end, and the callback sees an empty right window
/// rather than a panic or a fabricated second channel.
#[test]
fn a_mono_run_walks_with_a_zero_length_right_buffer() {
    let table = load_run_vectors();
    let mono: Vec<_> = table.iter().filter(|v| !v.stereo()).collect();
    assert!(!mono.is_empty(), "the table must carry a mono run");
    for v in mono {
        let frames = v.index();
        let source = v.source(&frames);
        let max = v.max_blocksize();
        let mut left = vec![0i32; max];
        let mut right: Vec<i32> = Vec::new(); // the contract, not an oversight
        let mut state = Default::default();
        let mut walked = 0usize;
        let stats = driver::decode_all(&source, &mut left, &mut right, &mut state, |_, l, r| {
            assert_eq!(
                r.len(),
                0,
                "{}: a mono walk must hand the callback an empty right window",
                v.label
            );
            assert!(!l.is_empty(), "{}: empty left window", v.label);
            walked += 1;
        })
        .unwrap_or_else(|e| panic!("{}: mono walk failed: {e:?}", v.label));
        assert_eq!(walked, stats.frames, "{}: callback count", v.label);
        assert_eq!(stats.samples, v.pcm_left.len(), "{}: sample total", v.label);
        assert!(
            v.pcm_right.is_empty(),
            "{}: a mono run has no right PCM",
            v.label
        );
    }
}
