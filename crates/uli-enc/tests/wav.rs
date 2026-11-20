//! S5: the WAV front end and album assembly.
//!
//! Two things are witnessed here that the rest of the suite cannot see:
//!
//! * **the boundary is unchanged.** A 32,768 Hz stereo 16-bit WAV encodes to *the
//!   same bytes* as the `album.f32` + `tracks.tsv` pair holding the same samples.
//!   That is E-R2's actual claim about the convenience layer, and it is checkable
//!   only by doing both and comparing blobs;
//! * **the resampled path is held to SNR, not to bytes.** A 44,100 Hz WAV reaches a
//!   different blob by design (the resample is exempt from the byte contract), but
//!   it must still decode to something that correlates with its input — so the test
//!   that runs it quotes the codec SNR instead of a hash.
//!
//! The parser tests are hand-built headers rather than committed WAV binaries: the
//! cases that matter (24-bit, mono, extensible, a truncated `data` chunk) are cheap
//! to state and unpleasant to keep as fixtures.

mod common;

use common::{TempDir, WavSpec, wav_bytes, wav_bytes_f32};
use uli_enc::assemble::fold;
use uli_enc::{
    Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, Error, Input, Pcm, Samples, assemble, encode, wav,
};
use uli_lite::format::M;

fn args(input: Input, g: Option<i32>, rd: bool) -> Args {
    Args {
        input,
        budget: DEFAULT_BUDGET,
        cap_pct: DEFAULT_CAP_PCT,
        g,
        rd,
        kappa: 0.13,
        stride: 8,
        fine_width: 0,
        train: false,
        ms: None,
        threads: 2,
    }
}

// --- parsing ---------------------------------------------------------------

#[test]
fn every_pcm_depth_lands_on_the_pcm16_scale() {
    let cases: [(u16, &[i32], &[f32]); 4] = [
        (8, &[228, 28, 128, 255], &[25600.0, -25600.0, 0.0, 32512.0]),
        (
            16,
            &[1000, -1000, 32767, -32768],
            &[1000.0, -1000.0, 32767.0, -32768.0],
        ),
        (24, &[2560, -2560], &[10.0, -10.0]),
        (32, &[196_608, -196_608], &[3.0, -3.0]),
    ];
    for (bits, raw, expected) in cases {
        let bytes = wav_bytes(&WavSpec::pcm(1, 32768, bits), raw);
        let w = wav::parse(&bytes).unwrap_or_else(|e| panic!("{bits}-bit: {e}"));
        assert_eq!(w.bits, bits);
        assert_eq!(w.samples, expected, "{bits}-bit samples");
    }
}

#[test]
fn a_16_bit_source_is_unchanged_which_is_why_the_blob_matches() {
    // The scale rule in one line: a 16-bit WAV's samples *are* PCM16-scale samples,
    // so the front end has nothing to undo and nothing to round.
    let raw: Vec<i32> = (0..512).map(|i| ((i * 37) % 4001) - 2000).collect();
    let bytes = wav_bytes(&WavSpec::pcm(2, 32768, 16), &raw);
    let w = wav::parse(&bytes).expect("parses");
    let expected: Vec<f32> = raw.iter().map(|&s| s as f32).collect();
    assert_eq!(w.samples, expected);
}

#[test]
fn float32_is_scaled_and_a_nan_is_refused() {
    let spec = WavSpec {
        tag: 3,
        bits: 32,
        ..WavSpec::pcm(1, 32768, 32)
    };
    let bytes = wav_bytes_f32(&spec, &[0.5, -0.25]);
    let w = wav::parse(&bytes).expect("float32 parses");
    assert_eq!(w.samples, vec![16384.0, -8192.0]);

    let bytes = wav_bytes_f32(&spec, &[f32::NAN]);
    let err = wav::parse(&bytes).expect_err("a NaN sample is refused");
    assert!(
        err.to_string().contains("non-finite"),
        "the refusal should say what it found: {err}"
    );
}

#[test]
fn extensible_pcm_is_read_by_its_subformat() {
    let spec = WavSpec {
        tag: 0xFFFE,
        ..WavSpec::pcm(2, 44100, 24)
    };
    let bytes = wav_bytes(&spec, &[2560, -2560, 5120, -5120]);
    let w = wav::parse(&bytes).expect("WAVE_FORMAT_EXTENSIBLE with a PCM subformat parses");
    assert_eq!(w.samples, vec![10.0, -10.0, 20.0, -20.0]);
}

#[test]
fn a_truncated_data_chunk_is_a_refusal_not_a_short_album() {
    let mut bytes = wav_bytes(&WavSpec::pcm(1, 32768, 16), &[1, 2, 3, 4]);
    // Declare four more bytes than the file holds. Clamping would silently drop the
    // last two samples, which is exactly the "invented tail" rule in reverse.
    let data_len_at = bytes.len() - 12;
    bytes[data_len_at..data_len_at + 4].copy_from_slice(&12u32.to_le_bytes());
    let err = wav::parse(&bytes).expect_err("a truncated chunk is refused");
    assert!(err.to_string().contains("truncated"), "{err}");
}

#[test]
fn unsupported_shapes_are_refused_with_the_reason() {
    let cases: [(WavSpec, &str); 3] = [
        (WavSpec::pcm(3, 32768, 16), "channels"),
        (
            WavSpec {
                tag: 6,
                ..WavSpec::pcm(1, 32768, 16)
            },
            "not PCM",
        ),
        (
            WavSpec {
                tag: 3,
                ..WavSpec::pcm(1, 32768, 16)
            },
            "only 32-bit float",
        ),
    ];
    for (spec, needle) in cases {
        let bytes = wav_bytes(&spec, &[1, 2, 3, 4]);
        let err = wav::parse(&bytes).expect_err("the shape is not supported");
        assert!(
            err.to_string().contains(needle),
            "{spec:?}: expected {needle:?} in {err}"
        );
    }

    // A depth the builder cannot encode but the header can declare: 64-bit PCM.
    // The `bits` field is the seventh u16 of the fmt body, which starts at 20.
    let mut bytes = wav_bytes(&WavSpec::pcm(2, 32768, 32), &[0; 8]);
    bytes[34..36].copy_from_slice(&64u16.to_le_bytes());
    let err = wav::parse(&bytes).expect_err("64-bit PCM is not read");
    assert!(err.to_string().contains("8/16/24/32"), "{err}");
}

// --- assembly --------------------------------------------------------------

#[test]
fn tracks_are_joined_gaplessly_and_the_table_partitions_the_album() {
    let dir = TempDir::new("wav-assemble");
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    std::fs::write(&a, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[1; 200])).unwrap();
    std::fs::write(&b, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[2; 100])).unwrap();

    let (pcm, samples) = assemble::assemble(&[a, b]).expect("assembly succeeds");

    // 200 and 100 interleaved *samples* are 100 and 50 frames.
    assert_eq!(pcm.n_samples, 150);
    assert_eq!(samples.n_samples(), 150);
    assert_eq!(pcm.tracks.len(), 2);
    assert_eq!(pcm.tracks[0].start_sample, 0);
    assert_eq!(pcm.tracks[0].n_samples, 100);
    assert_eq!(pcm.tracks[1].start_sample, 100);
    assert_eq!(pcm.tracks[1].n_samples, 50);
    // NS is the last row's end, which the boundary's own rule also demands.
    let last = &pcm.tracks[1];
    assert_eq!(
        u64::from(pcm.n_samples),
        u64::from(last.start_sample) + u64::from(last.n_samples)
    );
    // Gapless means no silence was inserted between them.
    assert_eq!(samples.at(99, 0), 1.0);
    assert_eq!(samples.at(100, 0), 2.0);
}

#[test]
fn mono_is_duplicated_and_tags_are_folded() {
    let dir = TempDir::new("wav-mono-tags");
    let path = dir.path().join("synthetic.wav");
    let spec = WavSpec {
        title: Some("Synthetic — Öffnung (Mix)"),
        artist: Some("Sömebödy"),
        ..WavSpec::pcm(1, 32768, 16)
    };
    std::fs::write(&path, wav_bytes(&spec, &[1000, 2000])).expect("write");
    let (pcm, samples) = assemble::assemble(&[path]).expect("assembly succeeds");
    assert_eq!(pcm.n_samples, 2);
    assert_eq!(samples.at(0, 0), 1000.0);
    assert_eq!(samples.at(0, 1), 1000.0, "mono is duplicated, not mixed");
    // The §5.5 fold: the em dash is dropped, Ö decomposes to O, whitespace collapses.
    assert_eq!(pcm.tracks[0].title, "Synthetic Offnung (Mix)");
    assert_eq!(pcm.tracks[0].artist, "Somebody");
}

#[test]
fn a_missing_artist_and_title_fall_back_like_the_reference() {
    let dir = TempDir::new("wav-fallback");
    let path = dir.path().join("Track One.wav");
    std::fs::write(&path, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[0, 0])).unwrap();
    let (pcm, _) = assemble::assemble(&[path]).expect("assembly succeeds");
    assert_eq!(pcm.tracks[0].title, "Track One");
    assert_eq!(
        pcm.tracks[0].artist, "?",
        "an empty artist is the reference's '?'"
    );
}

#[test]
fn an_empty_source_is_refused() {
    let dir = TempDir::new("wav-empty");
    let path = dir.path().join("empty.wav");
    std::fs::write(&path, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[])).unwrap();
    let err = assemble::assemble(&[path]).expect_err("an empty track cannot be an album");
    assert!(err.to_string().contains("0 samples"), "{err}");
}

#[test]
fn fold_matches_the_packers_cases() {
    // The same shapes `scripts/album_prep.py`'s FOLD_CASES pin, so the two folds
    // cannot drift without one of the two suites saying so.
    assert_eq!(fold("Bioluminescent 1"), "Bioluminescent 1");
    assert_eq!(fold("Synthetic — Öffnung"), "Synthetic Offnung");
    assert_eq!(fold("  spaced   out  "), "spaced out");
    assert_eq!(fold("(wrapped title)"), "wrapped title");
    assert_eq!(fold("a (phrase) inside"), "a (phrase) inside");
    assert_eq!(
        fold("日本語"),
        "",
        "a script with no Latin base folds to nothing"
    );
    assert_eq!(fold("Æther"), "AEther", "ligatures expand, as NFKD would");
}

// --- the boundary itself ---------------------------------------------------

/// A deterministic pseudo-musical pattern, no RNG: the same numbers every run.
fn pattern(n_frames: usize) -> Vec<f32> {
    (0..n_frames)
        .flat_map(|i| {
            let t = i as f32;
            let l = 6000.0 * (t * 0.031).sin() + 1200.0 * (t * 0.21).sin();
            let r = 5000.0 * (t * 0.033).sin() + 900.0 * (t * 0.17).cos();
            [l, r]
        })
        .collect()
}

#[test]
fn a_32768_hz_stereo_wav_reaches_the_pcm_blobs_exact_bytes() {
    let dir = TempDir::new("wav-equivalence");
    let n = M + 200;
    let pcm_samples = pattern(n);

    // The PCM boundary, written the way `--emit-pcm` writes it.
    let samples = Samples::from_interleaved(pcm_samples.clone());
    let album = dir.path().join("album.f32");
    samples.write_f32(&album).expect("album.f32");
    let tracks = common::write_tracks(
        dir.path(),
        &format!("# n\tstart\tlen\ttitle\tartist\n1\t0\t{n}\tPattern\tuli-enc test\n"),
    );

    // The same samples as a 16-bit WAV. `pattern` is integral-valued by construction
    // (it is not, so round it here: the equivalence claim is about *identical*
    // samples, and a 16-bit WAV can only carry integers).
    let raw: Vec<i32> = pcm_samples.iter().map(|&s| s.round() as i32).collect();
    let wav_path = dir.path().join("pattern.wav");
    let spec = WavSpec {
        title: Some("Pattern"),
        artist: Some("uli-enc test"),
        ..WavSpec::pcm(2, 32768, 16)
    };
    std::fs::write(&wav_path, wav_bytes(&spec, &raw)).unwrap();

    // Round the PCM pair the same way, so the two inputs hold identical samples.
    let rounded: Vec<f32> = raw.iter().map(|&s| s as f32).collect();
    Samples::from_interleaved(rounded)
        .write_f32(&album)
        .expect("album.f32");

    let from_pcm = encode(&args(
        Input::Pcm {
            album: album.clone(),
            tracks: tracks.clone(),
        },
        Some(100),
        false,
    ))
    .expect("the PCM pair encodes");
    let from_wav = encode(&args(Input::Wav(vec![wav_path.clone()]), Some(100), false))
        .expect("the WAV encodes");

    assert_eq!(
        from_pcm.bytes, from_wav.bytes,
        "a 32,768 Hz stereo WAV must reach the identical blob its PCM pair would (E-R2)"
    );
    assert!(
        from_wav.boundary.is_some(),
        "the WAV path reports the boundary it assembled, so --emit-pcm can dump it"
    );
    assert!(
        from_pcm.boundary.is_none(),
        "the PCM path's boundary is already the caller's files"
    );
}

#[test]
fn a_resampled_wav_is_held_to_snr_not_to_bytes() {
    let dir = TempDir::new("wav-resample-encode");
    // 0.1 s of a 1 kHz tone at 44,100 Hz, stereo, 16-bit: a rate the product does not
    // play at, so the resampler is in the path by design.
    let fs = 44100u32;
    let n_in = (fs / 10) as usize;
    let raw: Vec<i32> = (0..n_in)
        .flat_map(|i| {
            let v = (10000.0 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / fs as f32).sin())
                .round() as i32;
            [v, v]
        })
        .collect();
    let path = dir.path().join("tone.wav");
    std::fs::write(&path, wav_bytes(&WavSpec::pcm(2, fs, 16), &raw)).unwrap();

    let out = encode(&args(Input::Wav(vec![path]), Some(100), false)).expect("encodes");
    let expected = n_in * 32768 / fs as usize;
    let boundary = out.boundary.as_ref().expect("assembled boundary");
    assert_eq!(
        boundary.samples.n_samples(),
        expected,
        "one duration, not one byte"
    );
    assert_eq!(boundary.pcm.n_samples as usize, expected);
    assert!(out.report.n_frames > 1);
    let snr = out.report.snr_db.expect("a tone has a defined SNR");
    assert!(
        snr > 20.0,
        "44,100 Hz → 32,768 Hz then g=100 should stay well above 20 dB, got {snr:.2}"
    );
}

#[test]
fn a_wav_with_no_codec_knob_is_refused_rather_than_written_as_silence() {
    let dir = TempDir::new("wav-no-knob");
    let path = dir.path().join("tone.wav");
    std::fs::write(&path, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[1000; 64])).unwrap();
    let err = encode(&args(Input::Wav(vec![path]), None, false))
        .expect_err("a WAV with neither --rd nor --g would be silence");
    match err {
        Error::Plan(m) => assert!(m.contains("silence"), "{m}"),
        other => panic!("expected a named refusal, got {other:?}"),
    }
}

#[test]
fn the_cli_assembles_wavs_and_dumps_the_pair_it_encoded() {
    let dir = TempDir::new("wav-cli");
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    std::fs::write(&a, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[500; 128])).unwrap();
    std::fs::write(&b, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[-500; 64])).unwrap();
    let out = dir.path().join("album.uli");
    let dump = dir.path().join("pcm");

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_uli-enc"))
        .args(["--wav", a.to_str().unwrap()])
        .args(["--wav", b.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap()])
        .args(["--g", "100"])
        .args(["--emit-pcm", dump.to_str().unwrap()])
        .output()
        .expect("the CLI runs");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("2 tracks"), "album line: {stdout}");
    assert!(stdout.contains("dacw off"), "album line: {stdout}");

    // The dump is a real boundary: it round-trips through `Pcm::open` and encodes to
    // the same bytes the WAV path just produced.
    let dumped = std::fs::read(dump.join("album.f32")).expect("album.f32 was dumped");
    assert_eq!(dumped.len(), 96 * 8, "128 + 64 samples = 96 stereo frames");
    let pcm = Pcm::open(&dump.join("album.f32"), &dump.join("tracks.tsv")).expect("the dump opens");
    let again = encode(&args(
        Input::Pcm {
            album: dump.join("album.f32"),
            tracks: dump.join("tracks.tsv"),
        },
        Some(100),
        false,
    ))
    .expect("the dumped pair encodes");
    assert_eq!(
        again.bytes,
        std::fs::read(&out).expect("the blob was written"),
        "the dumped pair must be the pair the WAV path encoded"
    );
    assert_eq!(pcm.n_samples, 96);
    assert_eq!(pcm.tracks.len(), 2);
}

#[test]
fn the_cli_refuses_two_input_shapes_and_emit_pcm_without_wavs() {
    let dir = TempDir::new("wav-cli-refusals");
    let a = dir.path().join("a.wav");
    std::fs::write(&a, wav_bytes(&WavSpec::pcm(2, 32768, 16), &[1; 16])).unwrap();
    let album = common::silence_album(dir.path(), 8);
    let tracks = common::single_track(dir.path(), 8);
    let out = dir.path().join("album.uli");

    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_uli-enc"))
            .args(args)
            .output()
            .expect("the CLI runs")
    };

    let result = run(&[
        "--album",
        album.to_str().unwrap(),
        "--tracks",
        tracks.to_str().unwrap(),
        "--wav",
        a.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--g",
        "100",
    ]);
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("two input shapes"), "{stderr}");

    let result = run(&[
        "--album",
        album.to_str().unwrap(),
        "--tracks",
        tracks.to_str().unwrap(),
        "--emit-pcm",
        dir.path().to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("already the input"), "{stderr}");

    // A WAV and no codec knob: refused, and the message names the silence.
    let result = run(&["--wav", a.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("silence"), "{stderr}");
    assert!(!out.exists(), "--out was created despite a refusal");
}
