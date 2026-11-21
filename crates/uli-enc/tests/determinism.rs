//! Determinism (E-R5, rules R5a/R5b/R5d): the same input at any `--threads`
//! produces byte-identical output, and the workers really did split the frames.
//!
//! The comparison is bytes, not a hash: it is stronger and needs no dependency.

mod common;

use common::{TempDir, silence_album, single_track};
use uli_enc::{Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, Input, chunks, encode};
use uli_lite::format::M;

fn encode_with_threads(dir: &std::path::Path, threads: usize) -> uli_enc::Output {
    let n_samples = 15 * M as u32; // 16 frames
    let album = silence_album(dir, n_samples as usize);
    let tracks = single_track(dir, n_samples);
    encode(&Args {
        input: Input::Pcm { album, tracks },
        budget: DEFAULT_BUDGET,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        fine_width: 0,
        train: false,
        ms: None,
        threads,
    })
    .expect("the silence album encodes")
}

#[test]
fn one_worker_and_four_workers_produce_the_same_bytes() {
    let dir = TempDir::new("determinism");
    let one = encode_with_threads(dir.path(), 1);
    let four = encode_with_threads(dir.path(), 4);

    assert!(one.report.n_frames >= 8);
    assert_eq!(one.report.workers_used, 1);
    assert!(
        four.report.workers_used >= 2,
        "four workers produced {} chunk(s); the parallel path was not exercised",
        four.report.workers_used
    );
    assert_eq!(
        one.bytes, four.bytes,
        "--threads 1 and --threads 4 differ; assembly is not index-ordered"
    );
    assert_eq!(one.report.n_frames, four.report.n_frames);
}

#[test]
fn chunks_partition_the_frames_for_any_worker_count() {
    for nf in [1usize, 2, 7, 8, 16, 129] {
        for threads in [1usize, 2, 3, 4, 16, 64] {
            let parts = chunks(nf, threads);
            assert!(!parts.is_empty(), "nf={nf} threads={threads}: no chunks");
            assert!(
                parts.iter().all(|r| !r.is_empty()),
                "nf={nf} threads={threads}: an empty chunk"
            );
            assert_eq!(parts.first().unwrap().start, 0);
            assert_eq!(parts.last().unwrap().end, nf);
            for pair in parts.windows(2) {
                assert_eq!(pair[0].end, pair[1].start, "nf={nf} threads={threads}: gap");
            }
        }
    }
}

/// A short non-silent album: the RD search is the parallel part this covers.
fn tone_album(dir: &std::path::Path, n_samples: u32) -> std::path::PathBuf {
    let path = dir.join("album.f32");
    let mut bytes = Vec::with_capacity(n_samples as usize * 8);
    for i in 0..n_samples {
        let t = i as f32;
        let l = 5000.0 * (t * 0.05).sin() + 800.0 * (t * 0.31).sin();
        let r = 4000.0 * (t * 0.07).cos();
        bytes.extend_from_slice(&l.to_le_bytes());
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    std::fs::write(&path, bytes).expect("write album.f32");
    path
}

#[test]
fn the_rd_search_is_thread_invariant() {
    // S5 moved the curve passes (`Curves::compute`/`refine`) from one thread to
    // `--threads`, so R5a/R5b now cover them too: same bytes at 1 and 4 workers,
    // and the 4-worker run really did split the frames.
    let dir = TempDir::new("determinism-rd");
    let n_samples = 16 * M as u32;
    let album = tone_album(dir.path(), n_samples);
    let tracks = single_track(dir.path(), n_samples);
    let run = |threads: usize| {
        encode(&Args {
            input: Input::Pcm {
                album: album.clone(),
                tracks: tracks.clone(),
            },
            budget: 60_000,
            cap_pct: DEFAULT_CAP_PCT,
            g: None,
            rd: true,
            kappa: 0.13,
            stride: 8,
            fine_width: 4,
            train: false,
            ms: None,
            threads,
        })
        .expect("the tone album searches")
    };
    let one = run(1);
    let four = run(4);
    assert!(one.report.g_chosen.is_some(), "the search did not run");
    assert!(
        four.report.workers_used >= 2,
        "four workers were not exercised"
    );
    assert_eq!(
        one.bytes, four.bytes,
        "the RD search is not thread-invariant"
    );
}
