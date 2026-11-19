//! `Pcm::open`: the contract boundary's validation, on real temp files.
//!
//! S1 does not read the samples (frames are content-independent), so what can be
//! wrong at this boundary is the **album length**, which belongs to the track
//! table, and the track arithmetic. The length tests below witness the `NS` ruling
//! (`PLAN-OF-RECORD.md` → "S1 — what the review found"): the reference computes
//! `NS = TR[NT-1].start + TR[NT-1].len` and asserts the file matches, and an
//! encoder that read the length off the file instead would disagree with it on
//! every byte after the header for reasons that have nothing to do with the codec.

mod common;

use common::{TempDir, silence_album, single_track, write_tracks};
use uli_enc::{Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, Error, Pcm, encode};

#[test]
fn a_good_input_pair_reads_the_length_and_both_tracks() {
    let dir = TempDir::new("pcm-good");
    let album = silence_album(dir.path(), 1024);
    let tracks = write_tracks(
        dir.path(),
        "# n start len title artist (32768 Hz samples)\n\
         1\t0\t512\tFirst\tArtist A\n\
         2\t512\t512\tSecond\tArtist B\n",
    );

    let pcm = Pcm::open(&album, &tracks).expect("the input pair is valid");
    assert_eq!(pcm.n_samples, 1024);
    assert_eq!(pcm.tracks.len(), 2);
    assert_eq!(pcm.tracks[0].start_sample, 0);
    assert_eq!(pcm.tracks[1].start_sample, 512);
    assert_eq!(pcm.tracks[1].title, "Second");
    assert_eq!(pcm.tracks[1].artist, "Artist B");
}

#[test]
fn an_album_that_is_not_whole_stereo_frames_is_rejected() {
    let dir = TempDir::new("pcm-bad-size");
    let album = common::raw_album(dir.path(), &[0u8; 7]);
    let tracks = write_tracks(dir.path(), "1\t0\t0\tT\tA\n");

    let err = Pcm::open(&album, &tracks).expect_err("7 bytes is not a stereo f32 frame");
    assert!(
        matches!(&err, Error::Pcm(m) if m.contains("not a multiple of 8")),
        "{err}"
    );
}

#[test]
fn a_thirty_third_track_is_rejected_with_the_format_limit() {
    let dir = TempDir::new("pcm-33-tracks");
    let album = silence_album(dir.path(), 33 * 512);
    let mut text = String::from("# n start len title artist\n");
    for i in 0..33u32 {
        text.push_str(&format!("{}\t{}\t512\tT{i}\tA\n", i + 1, i * 512));
    }
    let tracks = write_tracks(dir.path(), &text);

    let err = Pcm::open(&album, &tracks).expect_err("33 tracks exceed the format's limit");
    assert!(
        matches!(&err, Error::Tracks(m) if m.contains("more than 32 tracks (the format's limit)")),
        "{err}"
    );
}

/// The `NS` ruling, case 1: a padded tail. File-size-derived length would silently encode
/// 2,048 samples and call it an album; the reference reads 1,024 off the table
/// and refuses the file. Both counts must be in the message (E-R6's rule, which
/// is not only about the budget).
#[test]
fn an_album_longer_than_the_track_table_refuses_and_cites_both_counts() {
    let dir = TempDir::new("pcm-tail");
    let album = silence_album(dir.path(), 2048);
    let tracks = single_track(dir.path(), 1024);

    let err = Pcm::open(&album, &tracks).expect_err("the tail is not part of the album");
    match err {
        Error::Pcm(m) => {
            assert!(
                m.contains("1024"),
                "message omits the track table's end: {m}"
            );
            assert!(
                m.contains("2048"),
                "message omits the file's sample count: {m}"
            );
            assert!(
                m.contains("track table"),
                "message does not say which side wins: {m}"
            );
        }
        other => panic!("expected a Pcm refusal, got {other:?}"),
    }
}

/// The other direction: a table that promises more than the file holds. Not
/// clamped to the shorter of the two — the reference's `fstat` check refuses, and
/// clamping would quietly produce a different album from the one asked for.
#[test]
fn an_album_shorter_than_the_track_table_refuses() {
    let dir = TempDir::new("pcm-short");
    let album = silence_album(dir.path(), 512);
    let tracks = single_track(dir.path(), 1024);

    let err = Pcm::open(&album, &tracks).expect_err("512 samples cannot hold a 1024 table");
    assert!(
        matches!(&err, Error::Pcm(m) if m.contains("512") && m.contains("1024")),
        "{err}"
    );
}

/// Contiguity is not required — the reference never requires it, and a gap is
/// silence that still costs frames. What *is* required is that the last row
/// defines the end, so the file length has to reach it.
#[test]
fn a_gap_between_tracks_is_allowed_and_the_last_row_defines_the_length() {
    let dir = TempDir::new("pcm-gap");
    let album = silence_album(dir.path(), 1536);
    let tracks = write_tracks(dir.path(), "1\t0\t512\tFirst\tA\n2\t1024\t512\tSecond\tA\n");

    let pcm = Pcm::open(&album, &tracks).expect("a gap is not an overlap");
    assert_eq!(
        pcm.n_samples, 1536,
        "NS is the last row's end, not the first's"
    );
    assert_eq!(pcm.tracks.len(), 2);

    // And the gap is real album: it costs frames, so the plan sees NF from 1536.
    // ceil(1536/512) + 1 = 4.
    let content = uli_enc::Content::silence(uli_enc::frame_count(pcm.n_samples));
    assert_eq!(
        uli_enc::Plan::new(
            &pcm,
            &uli_enc::DefaultTables::build(),
            &content,
            DEFAULT_BUDGET,
            DEFAULT_CAP_PCT
        )
        .expect("plans")
        .n_frames,
        4
    );
}

/// A first track need not start at zero; the reference only asks for the last
/// row's end. The `roundtrip` DC test depends on this.
#[test]
fn an_album_may_start_mid_track() {
    let dir = TempDir::new("pcm-midstart");
    let album = silence_album(dir.path(), 1536);
    let tracks = write_tracks(dir.path(), "1\t1024\t512\tLate\tA\n");

    let pcm = Pcm::open(&album, &tracks).expect("a lead-in of silence is legal");
    assert_eq!(pcm.n_samples, 1536);
    assert_eq!(pcm.tracks[0].start_sample, 1024);
}

/// The boundary refuses before the container machinery is reached at all: no
/// plan, no frames, no output. Observed through `encode`, because "nothing was
/// written" is only meaningful from outside the library.
#[test]
fn a_length_mismatch_refuses_before_any_frame_is_planned() {
    let dir = TempDir::new("pcm-boundary-first");
    let album = silence_album(dir.path(), 4096);
    let tracks = single_track(dir.path(), 1024);

    let err = encode(&Args {
        album: album.clone(),
        tracks: tracks.clone(),
        // Deliberately absurd: if the boundary did *not* run first, this would
        // be the error reported, and the length bug would hide behind it.
        budget: 1,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        threads: 1,
    })
    .expect_err("a mismatched pair cannot encode");
    assert!(
        matches!(&err, Error::Pcm(m) if m.contains("track table")),
        "the budget error was reported instead of the length: {err}"
    );

    // The CLI creates no output for the same reason.
    let out = dir.path().join("album.uli");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_uli-enc"))
        .args(["--album", album.to_str().unwrap()])
        .args(["--tracks", tracks.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap()])
        .output()
        .expect("the CLI runs");
    assert!(!result.status.success(), "a mismatch must exit nonzero");
    assert!(!out.exists(), "the CLI wrote --out for a refused pair");
}
