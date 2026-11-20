//! Determinism (E-R5, rules R5a/R5b/R5d): the same input at any `--threads`
//! produces byte-identical output, and the workers really did split the frames.
//!
//! The comparison is bytes, not a hash: it is stronger and needs no dependency.

mod common;

use common::{TempDir, silence_album, single_track};
use uli_enc::{Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, chunks, encode};
use uli_lite::format::M;

fn encode_with_threads(dir: &std::path::Path, threads: usize) -> uli_enc::Output {
    let n_samples = 15 * M as u32; // 16 frames
    let album = silence_album(dir, n_samples as usize);
    let tracks = single_track(dir, n_samples);
    encode(&Args {
        album,
        tracks,
        budget: DEFAULT_BUDGET,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
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
