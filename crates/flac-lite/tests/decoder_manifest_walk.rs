//! The manifest facade's witness: a GAFP blob built at test time from a
//! committed libFLAC fixture, walked two ways.
//!
//! **What is independent here.** The facade reads its stream defaults and its
//! end-of-stream bounds from the *manifest*; the reference walk reads them from
//! the fixture's own `STREAMINFO` (`scan::Frames::stream`). Same bytes, two
//! different declarations, so agreement means the facade carries what the
//! container says rather than quietly re-deriving it from the frames — the
//! failure mode a manifest-driven decoder is most likely to have.
//!
//! The blob is built here rather than committed because the real GAFP packer is
//! 6b-2's rung. This is the stand-in the plan names: header + offset table over
//! *real* encoder bytes, and 6b-2 owes a byte-identical output for the same
//! input, which turns this stand-in into a differential instead of a blind spot.
//!
//! `s2_seekable_96k_stereo.flac` is the fixture because its block size is 4608 —
//! outside the packer's {1024, 2048} profile and legal FLAC all the same, which
//! is exactly the property a general-purpose `Manifest::parse` must not refuse
//! (the profile check is `EncodeProfile::validate`'s job, and this suite proves
//! the two are separate).

use flac_lite::frame::StreamDefaults;
use flac_lite::scan::Frames;
use flac_lite::{Decoder, Error, FrameBuffers, Manifest};

const FIXTURE: &[u8] = include_bytes!("fixtures/s2_seekable_96k_stereo.flac");

/// One frame as the *reference* walk saw it: where it starts, how many bytes,
/// how many samples, and the PCM.
struct ReferenceFrame {
    offset: usize,
    bytes: usize,
    samples: usize,
    left: Vec<i32>,
    right: Vec<i32>,
}

/// Walk the fixture the file-level way: `stream::parse` for the offset and the
/// declarations, `Frames::stream` for the frames.
fn reference() -> (Vec<ReferenceFrame>, StreamDefaults, u32, u64) {
    let facts = flac_lite::stream::parse(FIXTURE).expect("fixture parses");
    let info = &facts.info;
    let defaults = info.as_defaults();
    let blocksize = usize::from(info.max_blocksize);

    let mut walk = Frames::stream(FIXTURE, &facts);
    let mut left = vec![0i32; blocksize];
    let mut right = vec![0i32; blocksize];
    let mut state = [flac_lite::subframe::PredictorState::new(); 2];
    let mut out = Vec::new();
    while let Some(frame) = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect("walk")
    {
        out.push(ReferenceFrame {
            offset: facts.frame_data_offset + frame.offset,
            bytes: frame.bytes,
            samples: frame.samples,
            left: left[..frame.samples].to_vec(),
            right: right[..frame.samples].to_vec(),
        });
    }
    assert!(!out.is_empty(), "fixture must contain frames");
    let samples: u64 = out.iter().map(|f| f.samples as u64).sum();
    (out, defaults, u32::from(info.max_blocksize), samples)
}

/// Build a GAFP blob over the reference frames: the container contract from
/// `crates/flac-lite/README.md`, little-endian, table immediately after the
/// 28-byte header.
fn gafp(
    frames: &[ReferenceFrame],
    defaults: &StreamDefaults,
    blocksize: u32,
    samples: u64,
) -> Vec<u8> {
    const HEADER: usize = flac_lite::format::HEADER_LEN;
    let table_end = HEADER + 4 * frames.len();
    let first = frames[0].offset;

    let mut blob = vec![0u8; table_end + (FIXTURE.len() - first)];
    blob[0..4].copy_from_slice(&flac_lite::format::MAGIC);
    blob[4] = flac_lite::format::VERSION;
    blob[5] = 2; // stereo: s2 is stereo
    blob[6] = defaults.bits_per_sample;
    blob[7] = 4; // packer's recorded max FIXED order; s2 is a normal encode
    blob[8..10].copy_from_slice(&(blocksize as u16).to_le_bytes());
    blob[10..14].copy_from_slice(&defaults.sample_rate_hz.to_le_bytes());
    blob[14..22].copy_from_slice(&samples.to_le_bytes());
    blob[22..24].copy_from_slice(&(frames.len() as u16).to_le_bytes());
    blob[24..28].copy_from_slice(&(table_end as u32).to_le_bytes());
    for (i, frame) in frames.iter().enumerate() {
        let at = HEADER + 4 * i;
        // The table's offsets are absolute blob offsets: the frame bytes move
        // from `first` to `table_end`, so each entry moves by that difference.
        let new_offset = table_end + (frame.offset - first);
        blob[at..at + 4].copy_from_slice(&(new_offset as u32).to_le_bytes());
    }
    blob[table_end..].copy_from_slice(&FIXTURE[first..]);
    blob
}

#[test]
fn the_facade_walks_a_manifest_to_the_same_pcm_as_the_file_level_walk() {
    let (frames, defaults, blocksize, samples) = reference();
    let blob = gafp(&frames, &defaults, blocksize, samples);

    let manifest = Manifest::parse(&blob).expect("stand-in blob parses");
    assert_eq!(manifest.frame_count(), frames.len());
    assert_eq!(manifest.total_samples(), samples);
    assert_eq!(manifest.blocksize(), blocksize as u16);
    assert_eq!(manifest.channels(), 2);
    assert_eq!(manifest.sample_rate_hz(), defaults.sample_rate_hz);
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(
            manifest.frame_offset(i),
            Ok(table_base(&blob) + frame.offset - frames[0].offset)
        );
        assert_eq!(manifest.frame_bytes(i).map(<[u8]>::len), Ok(frame.bytes));
    }

    let mut decoder = Decoder::new(&manifest).expect("decoder");
    let mut left = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut right = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);

    for (i, frame) in frames.iter().enumerate() {
        let decoded = decoder
            .decode_next(&mut bufs)
            .expect("decode")
            .expect("a frame");
        assert_eq!(decoded.index, i);
        assert_eq!(decoded.header.blocksize, frame.samples);
        assert_eq!(decoded.left, &frame.left[..], "frame {i} left channel");
        assert_eq!(decoded.right, &frame.right[..], "frame {i} right channel");
        assert_eq!(decoder.frame_index(), i + 1);
    }
    assert_eq!(decoder.sample_position(), samples);
    // End of stream: the manifest's own frame count, not the byte cursor.
    assert!(decoder.decode_next(&mut bufs).expect("end").is_none());
    assert!(decoder.decode_next(&mut bufs).expect("end again").is_none());
}

/// The absolute blob offset of frame 0, which the table's first entry names.
fn table_base(blob: &[u8]) -> usize {
    flac_lite::format::HEADER_LEN + 4 * usize::from(u16::from_le_bytes([blob[22], blob[23]]))
}

#[test]
fn seeking_reproduces_the_frame_the_walk_would_have_produced() {
    let (frames, defaults, blocksize, samples) = reference();
    let blob = gafp(&frames, &defaults, blocksize, samples);
    let manifest = Manifest::parse(&blob).expect("blob");
    let mut decoder = Decoder::new(&manifest).expect("decoder");
    let mut left = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut right = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);

    for target in [0usize, frames.len() / 2, frames.len() - 1] {
        decoder.seek_frame(target).expect("seek");
        assert_eq!(decoder.frame_index(), target);
        assert_eq!(
            decoder.sample_position(),
            0,
            "a seek restarts the handed-out count"
        );
        let decoded = decoder
            .decode_next(&mut bufs)
            .expect("decode")
            .expect("a frame");
        assert_eq!(decoded.index, target);
        assert_eq!(decoded.left, &frames[target].left[..], "seek to {target}");
        assert_eq!(decoded.right, &frames[target].right[..], "seek to {target}");
        // ... and the walk continues from there, contiguously.
        if target + 1 < frames.len() {
            let next = decoder
                .decode_next(&mut bufs)
                .expect("decode")
                .expect("frame");
            assert_eq!(next.index, target + 1);
            assert_eq!(next.left, &frames[target + 1].left[..]);
        }
    }

    // `frame_count` is a legal seek target meaning end-of-stream.
    decoder.seek_frame(frames.len()).expect("seek to end");
    assert!(decoder.decode_next(&mut bufs).expect("end").is_none());
    // Beyond it is not.
    assert_eq!(decoder.seek_frame(frames.len() + 1), Err(Error::Manifest));
}

#[test]
fn a_blob_that_overstates_its_samples_is_a_stream_mismatch() {
    let (frames, defaults, blocksize, samples) = reference();
    // Declare one block more than the frames hold. The frames walk fine; the
    // *manifest's* claim is what fails, at end of region, with both numbers.
    let blob = gafp(
        &frames,
        &defaults,
        blocksize,
        samples + u64::from(blocksize),
    );

    let manifest = Manifest::parse(&blob).expect("a header can lie");
    let mut decoder = Decoder::new(&manifest).expect("decoder");
    let mut left = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut right = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);

    let mut err = None;
    for _ in 0..=frames.len() {
        match decoder.decode_next(&mut bufs) {
            Ok(None) => break,
            Ok(Some(_)) => {}
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }
    match err {
        Some(Error::StreamMismatch { declared, actual }) => {
            assert_eq!(declared, samples + u64::from(blocksize), "the declaration");
            assert_eq!(actual, samples, "the samples the bytes hold");
        }
        other => panic!("expected StreamMismatch, got {other:?}"),
    }
}

#[test]
fn a_table_that_drops_a_frame_is_a_stream_mismatch_too() {
    let (frames, defaults, blocksize, all_samples) = reference();
    let short = frames.len() - 1;
    // The bytes still hold every frame; the manifest admits only `short` of
    // them, and declares samples for `short`. The walk consumes the whole
    // region, then convicts the declaration — the case a `next >= frame_count`
    // shortcut in the facade would have turned into a quiet end-of-stream.
    //
    // Which cross-check fires first is the walk's business: with a fixed block
    // size the sample count is the one that fails first here, and the numbers it
    // reports are the manifest's claim against the bytes' output. That is the
    // verdict the facade must pass through with its payload.
    let declared_samples: u64 = frames[..short].iter().map(|f| f.samples as u64).sum();
    let blob = gafp(&frames[..short], &defaults, blocksize, declared_samples);

    let manifest = Manifest::parse(&blob).expect("a header can lie");
    assert_eq!(manifest.frame_count(), short);
    let mut decoder = Decoder::new(&manifest).expect("decoder");
    let mut left = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut right = vec![0i32; FrameBuffers::required_len(&manifest)];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);

    let mut err = None;
    for _ in 0..=frames.len() {
        match decoder.decode_next(&mut bufs) {
            Ok(None) => break,
            Ok(Some(_)) => {}
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }
    match err {
        Some(Error::StreamMismatch { declared, actual }) => {
            assert_eq!(declared, declared_samples, "the manifest's claim");
            assert_eq!(actual, all_samples, "what the bytes produced");
        }
        other => panic!("expected StreamMismatch, got {other:?}"),
    }
}

#[test]
fn short_buffers_are_a_caller_error_the_frame_layer_also_names() {
    let (frames, defaults, blocksize, samples) = reference();
    let blob = gafp(&frames, &defaults, blocksize, samples);
    let manifest = Manifest::parse(&blob).expect("blob");
    let mut decoder = Decoder::new(&manifest).expect("decoder");

    // One sample too few: the walk refuses before writing anything.
    let mut left = vec![0i32; frames[0].samples - 1];
    let mut right = vec![0i32; frames[0].samples - 1];
    let mut bufs = FrameBuffers::new(&mut left, &mut right);
    assert_eq!(
        decoder.decode_next(&mut bufs).err(),
        Some(Error::InvalidField)
    );
    assert_eq!(decoder.frame_index(), 0, "a refused frame does not advance");
}

#[test]
fn the_profile_is_what_refuses_the_packers_contract_not_the_parser() {
    let (frames, defaults, blocksize, samples) = reference();
    let blob = gafp(&frames, &defaults, blocksize, samples);
    let manifest = Manifest::parse(&blob).expect("4608 is legal FLAC");
    // 6b-2's pack-time check is `DEFAULT_PROFILE::validate`; this fixture is a
    // normal libFLAC encode, so it is outside the packer's profile and the
    // *decoder* still works on it (the test above walks it end to end).
    assert_eq!(
        flac_lite::format::DEFAULT_PROFILE.validate(&manifest),
        Err(Error::ProfileViolation)
    );
}
