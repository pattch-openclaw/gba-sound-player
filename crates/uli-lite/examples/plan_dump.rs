//! S8-2's blob half: the plan a blob declares, in the units P2 dumps it in.
//!
//! Why this exists. S8-2 (`scripts/uli_dump_plan.py` / `make uli-dump-plan`) builds the
//! reference with a `--dump-plan` flag and writes the per-frame plan it *chose* — both its
//! pass-1 plan (the trainer's real input, which is not in the artefact) and the shipped
//! pass-2 plan — plus the per-class symbol counts `tables_from_stats` consumed. Its gate is
//! that the dumped **shipped** plan equals the plan recovered from the run's own blob: an
//! instrument checked against the artefact it explains, so a wrong dump cannot masquerade
//! as a finding.
//!
//! Recovering that plan is a decode — the stream carries `g` as a delta from the previous
//! frame and `ms` as one bit, so it cannot be read off the container — which is exactly
//! what `uli_lite`, the checked oracle, already does. This tool is that decode with no
//! analysis: one machine-readable line per frame, plus the seek entries and the header
//! cycle fields the plan implies.
//!
//! It is also useful on its own: it is the same `FrameInfo` data `plan_diff` compares, in
//! a form a script can diff (`make uli-plan-diff` remains the two-blob report).
//!
//! Usage:
//!   cargo run --manifest-path crates/uli-lite/Cargo.toml --target <host-triple> \
//!     --example plan_dump -- BLOB.uli
//!
//! Output (stable, one fact per line, `#` lines are comments):
//!   header n_frames N stream_halfwords N est_max_cycles N est_mean_cycles N seek_interval N
//!   seek I frame F bitpos P g_prev G
//!   frame F g G ms M cycles C bits B bitpos P
//!
//! Exit status: 0 on success, 1 on a usage or parse error.

use std::env;
use std::fs;
use std::process;

use uli_lite::{Album, Decoder, DecoderState};

const M: usize = 512;

fn main() {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => fail("usage: plan_dump BLOB.uli"),
    };
    if let Some(other) = args.next() {
        fail(&format!("unexpected argument {other:?}"));
    }

    let bytes = fs::read(&path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    let album = Album::parse(&bytes).unwrap_or_else(|e| fail(&format!("{path}: {e:?}")));
    let h = album.header();

    println!("# uli plan dump 1");
    println!("blob {path} bytes {}", bytes.len());
    println!(
        "header n_frames {} stream_halfwords {} est_max_cycles {} est_mean_cycles {} seek_interval {}",
        h.n_frames, h.stream_halfwords, h.est_max_cycles, h.est_mean_cycles, h.seek_interval
    );
    for i in 0..album.seek_count() {
        let seek = album
            .seek(i)
            .unwrap_or_else(|| fail(&format!("{path}: seek entry {i} does not parse")));
        println!(
            "seek {i} frame {} bitpos {} g_prev {}",
            (i as u32) * album.seek_interval(),
            seek.bitpos,
            seek.g_prev
        );
    }

    let mut state = Box::new(DecoderState::new());
    let mut decoder =
        Decoder::new(album, &mut state).unwrap_or_else(|e| fail(&format!("{path}: {e:?}")));
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    for _ in 0..album.n_frames() {
        let info = decoder
            .decode_frame(&mut l8, &mut r8, None, None)
            .unwrap_or_else(|e| fail(&format!("{path}: decode: {e:?}")));
        println!(
            "frame {} g {} ms {} cycles {} bits {} bitpos {}",
            info.frame, info.g, info.ops.ms, info.cycles, info.bits, info.bitpos
        );
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("plan_dump: {msg}");
    process::exit(1);
}
