//! The round-trip gate: the blob this crate writes, decoded by the real decoder.
//!
//! Nothing here asserts success by inspection. A silence album is encoded, then
//! parsed (`Album::parse` + `CodeTables::parse_into`), then **every frame is
//! decoded** through `uli_lite::Decoder` and every output sample is asserted
//! zero. The frame bit counts are checked against `frame_bits`, and the decoder
//! must report `EndOfStream` exactly after the last frame.

mod common;

use common::{TempDir, silence_album, single_track};

use uli_enc::{
    Args, Content, DC_G, DEFAULT_BUDGET, DEFAULT_CAP_PCT, DefaultTables, FrameKind, FramePos,
    GPlan, Input, Pcm, Plan, bitwriter::BitWriter, container, encode, frame_bits, write_frame,
};
use uli_lite::format::M;
use uli_lite::{Album, CodeTables, Decoder, DecoderState, Error as DecodeError};

fn args(album: std::path::PathBuf, tracks: std::path::PathBuf, threads: usize) -> Args {
    Args {
        input: Input::Pcm { album, tracks },
        budget: DEFAULT_BUDGET,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        train: false,
        ms: None,
        musearch: false,
        threads,
    }
}

#[test]
fn a_silence_album_decodes_to_zero_samples() {
    let dir = TempDir::new("roundtrip-silence");
    // 130 frames: NF = ceil(NS / M) + 1.
    let n_frames = 130u32;
    let n_samples = (n_frames - 1) * M as u32;
    let album_path = silence_album(dir.path(), n_samples as usize);
    let tracks = single_track(dir.path(), n_samples);

    let out = encode(&args(album_path, tracks, 2)).expect("silence album encodes");
    let album = Album::parse(&out.bytes).expect("the blob parses");
    assert_eq!(album.n_frames(), n_frames);
    assert_eq!(album.header().n_samples, n_samples);
    assert_eq!(album.header().est_max_cycles, out.report.est_max_cycles);
    assert_eq!(album.header().est_mean_cycles, out.report.est_mean_cycles);
    assert!(
        out.report.est_max_cycles > 0,
        "the cycle patch did not land in the header"
    );

    let mut tables = CodeTables::new();
    tables
        .parse_into(album.tables())
        .expect("the tables blob parses");

    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    let mut out8l = [1i8; M];
    let mut out8r = [1i8; M];
    let mut out16l = [1i16; M];
    let mut out16r = [1i16; M];

    let per_frame = frame_bits(&DefaultTables::build(), 0, 0, 0, FrameKind::Silence);
    let mut bits = 0u32;
    for f in 0..n_frames {
        let info = dec
            .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
            .unwrap_or_else(|e| panic!("frame {f}: {e:?}"));
        assert_eq!(info.frame, f);
        assert_eq!(info.ops.coded_bands, 0, "frame {f} coded a band");
        assert_eq!(info.bits as u64, per_frame, "frame {f} bit count");
        bits += info.bits;
        assert!(
            out16l.iter().all(|&s| s == 0) && out16r.iter().all(|&s| s == 0),
            "frame {f} produced non-zero i16 output"
        );
        assert!(
            out8l.iter().all(|&s| s == 0) && out8r.iter().all(|&s| s == 0),
            "frame {f} produced non-zero i8 output"
        );
    }
    assert_eq!(bits as u64, n_frames as u64 * per_frame);
    assert!(
        matches!(
            dec.decode_frame(&mut out8l, &mut out8r, None, None),
            Err(DecodeError::EndOfStream)
        ),
        "the decoder did not stop exactly after the last frame"
    );
}

#[test]
fn a_dc_impulse_frame_decodes_as_one_coded_band_per_channel() {
    let dir = TempDir::new("roundtrip-dc");
    let n_frames = 4u32;
    let n_samples = (n_frames - 1) * M as u32;
    let album_path = silence_album(dir.path(), n_samples as usize);
    // The track starts 1024 samples in, i.e. at frame 2: its container entry must
    // carry that frame's bitpos and the `g` state entering it (DC_G, not 0).
    let tracks = common::write_tracks(
        dir.path(),
        &format!(
            "# n start len title artist ({} Hz samples)\n\
             1\t1024\t512\tDC\tuli-enc test\n",
            uli_lite::format::RATE
        ),
    );

    let tables = DefaultTables::build();
    let pcm = Pcm::open(&album_path, &tracks).expect("inputs open");
    // The content plan says what the stream actually does — `g` jumps to DC_G at
    // frame 0 and stays — so the plan's bit arithmetic accounts for that delta
    // instead of assuming the silence S1's `encode` would have written. What the
    // plan cannot yet say is the frame *kind*, which is E2's per-frame work.
    let content = Content::from_plan(GPlan::new(
        vec![DC_G; n_frames as usize],
        vec![false; n_frames as usize],
    ));
    content
        .validate()
        .expect("DC_G must be inside the reader's step window, or the DC frame is a lie");
    let plan =
        Plan::new(&pcm, &tables, &content, DEFAULT_BUDGET, DEFAULT_CAP_PCT).expect("plan fits");
    assert_eq!(plan.n_frames as u32, n_frames);

    // The DC frame is not reachable from `encode` in S1 (its content is silence);
    // the frame writer and the container writer are the two pieces under test.
    let mut bw = BitWriter::new();
    let mut g_prev = 0i32;
    let mut fpos = Vec::new();
    for _ in 0..n_frames {
        fpos.push(FramePos {
            bitpos: bw.pos(),
            g_prev,
        });
        write_frame(&mut bw, &tables, &mut g_prev, DC_G, 0, FrameKind::DcImpulse);
    }
    assert_eq!(g_prev, DC_G);
    let stream = bw.finish();
    let bytes = container::emit(&plan, &tables, &fpos, &stream);

    let album = Album::parse(&bytes).expect("the blob parses");
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    let mut out8l = [0i8; M];
    let mut out8r = [0i8; M];
    let mut out16l = [0i16; M];
    let mut out16r = [0i16; M];
    let mut nonzero = 0u64;
    for f in 0..n_frames {
        let info = dec
            .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
            .unwrap_or_else(|e| panic!("frame {f}: {e:?}"));
        assert_eq!(
            info.ops.coded_bands, 2,
            "frame {f}: one coded band per channel"
        );
        assert!(info.ops.pairs >= 2, "frame {f}: no pair symbols");
        assert_eq!(info.xover, 0, "frame {f}: the pulse broke |X| < 2^28");
        assert_eq!(info.hfnz, 0, "frame {f}: a coefficient at index >= NCODED");
        nonzero += out16l.iter().filter(|&&s| s != 0).count() as u64;
        nonzero += out16r.iter().filter(|&&s| s != 0).count() as u64;
    }
    assert!(
        nonzero > 0,
        "the DC frame decoded to silence; the pulse proves nothing"
    );

    // The container's track entry: preroll frame 2, its bitpos, and `g_prev =`
    // the previous frame's g. Seeking must land on the same stream the linear
    // decode walked.
    let album = Album::parse(&bytes).expect("the blob parses");
    let track = album.track(0).expect("one track");
    assert_eq!(track.preroll_frame, 2);
    assert_eq!(track.g_prev as i32, DC_G);
    assert_eq!(track.skip, 0);
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    assert_eq!(dec.seek_track(0).expect("track seeks"), 0);
    let info = dec
        .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
        .expect("the frame after the preroll decodes");
    assert_eq!(info.frame, 3);
    assert_eq!(info.ops.coded_bands, 2);

    // The seek table's entry 0 points at frame 0 with g_prev = 0.
    let album = Album::parse(&bytes).expect("the blob parses");
    assert_eq!(album.seek_count(), 1);
    assert_eq!(album.seek(0).expect("seek entry").g_prev, 0);
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    dec.seek_entry(0).expect("seek entry applies");
    let info = dec
        .decode_frame(&mut out8l, &mut out8r, None, None)
        .expect("frame 0 decodes from the seek entry");
    assert_eq!(info.frame, 0);
    assert_eq!(info.ops.coded_bands, 2);
}

#[test]
fn a_seek_entry_past_the_first_points_at_its_frame() {
    let dir = TempDir::new("roundtrip-seek");
    let n_frames = 300u32;
    let n_samples = (n_frames - 1) * M as u32;
    let album_path = silence_album(dir.path(), n_samples as usize);
    let tracks = single_track(dir.path(), n_samples);

    let out = encode(&args(album_path, tracks, 1)).expect("silence album encodes");
    let album = Album::parse(&out.bytes).expect("the blob parses");
    assert_eq!(album.n_frames(), n_frames);
    // ceil(300 / 256) = 2: entries at frame 0 and frame 256.
    assert_eq!(album.seek_count(), 2);
    assert_eq!(album.seek(1).expect("second seek entry").bitpos, {
        let f = 256usize;
        let mut bitpos = 0u64;
        let per_frame = frame_bits(&DefaultTables::build(), 0, 0, 0, FrameKind::Silence);
        for _ in 0..f {
            bitpos += per_frame;
        }
        bitpos as u32
    });

    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    dec.seek_entry(1).expect("second seek entry applies");
    let mut out8l = [0i8; M];
    let mut out8r = [0i8; M];
    let mut out16l = [0i16; M];
    let mut out16r = [0i16; M];
    // The frame right after a seek is the overlap warm-up, then the stream must
    // walk on from frame 256 exactly as a linear decode would.
    for expected in 256..258u32 {
        let info = dec
            .decode_frame(&mut out8l, &mut out8r, Some(&mut out16l), Some(&mut out16r))
            .unwrap_or_else(|e| panic!("frame {expected}: {e:?}"));
        assert_eq!(info.frame, expected);
        assert_eq!(info.ops.coded_bands, 0);
        assert!(out16l.iter().all(|&s| s == 0) && out16r.iter().all(|&s| s == 0));
    }
}
