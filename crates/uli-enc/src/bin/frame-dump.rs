//! Code frames at a **frozen** `(g, ms, tables)` and print one line per frame.
//!
//! This is the Rust half of S8-1's probe (`scripts/uli_probe.py`): the same corpus, the same
//! frozen plan, the same line format, so `scripts/uli_frame_diff.py` can compare our coding
//! against the reference's with the *search* held out of the picture. Without it the probe
//! is one-sided — it can only tell you what the oracle does.
//!
//! The corpus is binary `f32`, `2*M` (1024) coefficients per frame, frame-major: the layout
//! `crates/uli-enc/tests/fixtures/mdct_ref_coef.f32` already has, and the one the probe's
//! `--dump-coef` writes. That file is a frozen 4-frame corpus the reference produced, which
//! is why the default invocation has something real to chew on.
//!
//! Line format (integers only, matching the probe's `frame` mode exactly):
//!
//! ```text
//! frame <f> g <g> ms <ms> bits <n> D <hex16> cycles <n> halfwords <n> stream <hex>
//! ```
//!
//! Usage:
//!   frame-dump --coef FILE [--g N ...] [--ms 0|1 ...] [--mu X] [--kappa X]
//!              [--g-prev N] [--tables BLOB]
//!
//! Exit status: 0 on success, 1 on a usage or coding error.

use std::env;
use std::fs;
use std::process;

use uli_enc::DefaultTables;
use uli_enc::frozen::code_at;
use uli_lite::format::M;

const FRAME_FLOATS: usize = 2 * M;

fn usage(msg: &str) -> ! {
    eprintln!("frame-dump: {msg}");
    process::exit(1);
}

fn main() {
    let mut coef_path = None;
    let mut tables_path = None;
    let mut gs: Vec<i32> = Vec::new();
    let mut mss: Vec<u32> = Vec::new();
    let mut kappa = uli_enc::rd::DEFAULT_KAPPA;
    let mut mu = 0.0f64;
    let mut g_prev = 0i32;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut take = |name: &str| {
            args.next()
                .unwrap_or_else(|| usage(&format!("{name} needs a value")))
        };
        match arg.as_str() {
            "--coef" => coef_path = Some(take("--coef")),
            "--tables" => tables_path = Some(take("--tables")),
            "--g" => gs.push(
                take("--g")
                    .parse()
                    .unwrap_or_else(|_| usage("--g takes a number")),
            ),
            "--ms" => {
                let v: u32 = take("--ms")
                    .parse()
                    .unwrap_or_else(|_| usage("--ms takes 0 or 1"));
                if v > 1 {
                    usage("--ms takes 0 or 1");
                }
                mss.push(v);
            }
            "--mu" => {
                mu = take("--mu")
                    .parse()
                    .unwrap_or_else(|_| usage("--mu takes a number"))
            }
            "--kappa" => {
                kappa = take("--kappa")
                    .parse()
                    .unwrap_or_else(|_| usage("--kappa takes a number"))
            }
            "--g-prev" => {
                g_prev = take("--g-prev")
                    .parse()
                    .unwrap_or_else(|_| usage("--g-prev takes a number"))
            }
            other => usage(&format!("unknown argument {other:?}")),
        }
    }

    let Some(coef_path) = coef_path else {
        usage("--coef FILE is required (binary f32, 1024 coefficients per frame)");
    };
    if gs.is_empty() {
        gs = vec![60, 100, 140];
    }
    if mss.is_empty() {
        mss = vec![0, 1];
    }

    let tables = match tables_path {
        Some(path) => {
            let blob = fs::read(&path).unwrap_or_else(|e| usage(&format!("{path}: {e}")));
            DefaultTables::from_blob(&blob).unwrap_or_else(|e| usage(&format!("{path}: {e}")))
        }
        None => DefaultTables::build(),
    };

    let bytes = fs::read(&coef_path).unwrap_or_else(|e| usage(&format!("{coef_path}: {e}")));
    let frame_bytes = FRAME_FLOATS * 4;
    if bytes.is_empty() || bytes.len() % frame_bytes != 0 {
        usage(&format!(
            "{coef_path}: {} bytes is not a whole number of {frame_bytes}-byte frames",
            bytes.len()
        ));
    }

    let mut out = String::new();
    for (f, chunk) in bytes.chunks_exact(frame_bytes).enumerate() {
        let mut coef = [0f32; FRAME_FLOATS];
        for (i, c) in chunk.chunks_exact(4).enumerate() {
            coef[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        }
        for &g in &gs {
            for &ms in &mss {
                let dump = code_at(&tables, &coef, g, ms, kappa, mu, g_prev)
                    .unwrap_or_else(|e| usage(&format!("frame {f} g={g} ms={ms}: {e}")));
                let halfwords = ((dump.stream_bits + 15) / 16) as usize;
                out.push_str(&format!(
                    "frame {f} g {g} ms {ms} bits {} D {:016x} cycles {} halfwords {} stream ",
                    dump.bits,
                    dump.dist.to_bits(),
                    dump.cycles,
                    halfwords
                ));
                for w in &dump.stream {
                    out.push_str(&format!("{w:04x}"));
                }
                out.push('\n');
            }
        }
    }
    print!("{out}");
}
