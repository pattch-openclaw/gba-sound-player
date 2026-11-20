//! Refusal (E-R6): a budget that cannot hold the album exits nonzero citing both
//! numbers, before a frame is encoded — and `--out` is not created.
//!
//! The library never writes files, so "needed and allowed" is witnessed at two
//! levels: the `Refusal` value's own fields and `Display`, and the CLI's exit
//! status plus stderr.

mod common;

use common::{TempDir, silence_album, single_track};
use std::process::Command;
use uli_enc::{
    Args, Content, DEFAULT_CAP_PCT, DefaultTables, Error, Pcm, Plan, Refusal, encode, frame_count,
};
use uli_lite::format::M;

fn inputs(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let n_samples = 3 * M as u32;
    (
        silence_album(dir, n_samples as usize),
        single_track(dir, n_samples),
    )
}

#[test]
fn a_budget_below_overhead_plus_the_stream_refuses_and_cites_both_numbers() {
    let dir = TempDir::new("refusal-floor");
    let (album, tracks) = inputs(dir.path());
    let tables = DefaultTables::build();
    let pcm = Pcm::open(&album, &tracks).expect("inputs open");
    // The overhead is what the plan itself computes; the encoded stream then has
    // to fit above it. S1's content is silence, so the plan's walk over it is the
    // same uniform size the emit loop will produce.
    let content = Content::silence(frame_count(pcm.n_samples));
    let planned = Plan::new(&pcm, &tables, &content, 1 << 30, DEFAULT_CAP_PCT).expect("plans");
    let budget = planned.overhead_bytes as u64;

    let err = encode(&Args {
        album,
        tracks,
        budget,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        threads: 1,
    })
    .expect_err("the stream does not fit in the overhead alone");

    match err {
        Error::Refusal(r) => {
            let Refusal {
                needed,
                allowed,
                overhead_bytes,
                ..
            } = r;
            assert_eq!(allowed, budget);
            assert_eq!(overhead_bytes, planned.overhead_bytes as u64);
            assert!(needed > allowed, "needed {needed} == allowed {allowed}");
            let text = r.to_string();
            assert!(text.contains(&needed.to_string()), "{text}");
            assert!(text.contains(&allowed.to_string()), "{text}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_budget_below_the_container_overhead_refuses() {
    let dir = TempDir::new("refusal-overhead");
    let (album, tracks) = inputs(dir.path());
    let err = encode(&Args {
        album,
        tracks,
        budget: 88,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        threads: 1,
    })
    .expect_err("88 bytes cannot hold the header, tables, track and stream");
    assert!(matches!(err, Error::Refusal(_)), "{err}");
}

#[test]
fn the_cli_exits_nonzero_and_creates_no_output_file() {
    let dir = TempDir::new("refusal-cli");
    let (album, tracks) = inputs(dir.path());
    let out = dir.path().join("album.uli");

    let result = Command::new(env!("CARGO_BIN_EXE_uli-enc"))
        .args(["--album", album.to_str().unwrap()])
        .args(["--tracks", tracks.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap()])
        .args(["--budget", "1024"])
        .output()
        .expect("the CLI runs");

    assert!(!result.status.success(), "a refusal must exit nonzero");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("needs"),
        "stderr does not cite what it needs: {stderr}"
    );
    assert!(
        stderr.contains("allows"),
        "stderr does not cite what it is allowed: {stderr}"
    );
    assert!(
        !out.exists(),
        "the CLI created --out despite refusing the album"
    );
}

#[test]
fn the_cli_writes_the_blob_and_prints_the_target_line() {
    let dir = TempDir::new("refusal-cli-ok");
    let (album, tracks) = inputs(dir.path());
    let out = dir.path().join("album.uli");

    let result = Command::new(env!("CARGO_BIN_EXE_uli-enc"))
        .args(["--album", album.to_str().unwrap()])
        .args(["--tracks", tracks.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap()])
        .output()
        .expect("the CLI runs");

    assert!(result.status.success(), "{result:?}");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("container overhead") && stdout.contains("bits/frame"),
        "missing the reference's sizing line: {stdout}"
    );
    let bytes = std::fs::read(&out).expect("--out was written");
    assert!(
        uli_lite::Album::parse(&bytes).is_ok(),
        "the CLI wrote a bad blob"
    );
}
