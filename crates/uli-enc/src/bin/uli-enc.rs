//! `uli-enc` — the reference CLI's flags, so the later byte-exactness matrix can
//! drive both with identical arguments (`PLAN-OF-RECORD.md` → "The encoder").
//!
//! A refusal is an error exit that cites needed and allowed bytes, and it
//! happens before `--out` is created: the encoder either writes a whole album or
//! nothing at all.

use std::path::PathBuf;
use std::process::ExitCode;

use uli_enc::{Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, encode};

const USAGE: &str = "usage: uli-enc --album F --tracks TSV --out U [--budget BYTES]                     [--cap PCT] [--threads N] [--g N] [--rd] [--kappa X] [--stride N]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run() -> Result<(), ExitCode> {
    let mut album: Option<PathBuf> = None;
    let mut tracks: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut budget = DEFAULT_BUDGET;
    let mut cap_pct = DEFAULT_CAP_PCT;
    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut threads = default_threads;
    // No `--g` and the album is S1's: silence, sized and refused exactly as before.
    let mut g: Option<i32> = None;
    let mut rd = false;
    let mut kappa = uli_enc::DEFAULT_KAPPA;
    let mut stride = 8i32;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--album" => album = Some(PathBuf::from(take(&mut it, "--album")?)),
            "--tracks" => tracks = Some(PathBuf::from(take(&mut it, "--tracks")?)),
            "--out" => out = Some(PathBuf::from(take(&mut it, "--out")?)),
            "--budget" => budget = parse(&mut it, "--budget")?,
            "--cap" => cap_pct = parse(&mut it, "--cap")?,
            "--threads" => threads = parse(&mut it, "--threads")?,
            "--g" => g = Some(parse(&mut it, "--g")?),
            "--rd" => rd = true,
            "--kappa" => kappa = parse(&mut it, "--kappa")?,
            "--stride" => stride = parse(&mut it, "--stride")?,
            other => return Err(usage(format!("unknown argument {other:?}"))),
        }
    }

    let (Some(album), Some(tracks), Some(out)) = (album, tracks, out) else {
        return Err(usage("--album, --tracks and --out are required".into()));
    };

    let args = Args {
        album,
        tracks,
        budget,
        cap_pct,
        threads,
        g,
        rd,
        kappa,
        stride,
    };
    if rd && g.is_some() {
        return Err(usage("--rd searches for g, so it cannot also be given one".into()));
    }
    let encoded = encode(&args).map_err(|e| {
        eprintln!("uli-enc: {e}");
        ExitCode::from(1)
    })?;
    let report = encoded.report;
    println!(
        "container overhead {} bytes; stream target {} bits = {:.1} bits/frame",
        report.overhead_bytes, report.target_bits, report.bits_per_frame
    );
    if report.stream_bits > 0 {
        // The step actually used, and the λ that chose it, are printed unconditionally:
        // silence and "all coarse" cost the same eight bits per frame, so a rate alone
        // cannot tell a search that failed from a search that never ran. S3's first
        // attempt was misdiagnosed twice because the one field that distinguished them
        // was filtered out of the output before it was read.
        let gline = match &report.g_chosen {
            Some(v) => format!(
                "searched {}..{} (lambda {:.3e}, target {} bits)",
                v.iter().min().unwrap(),
                v.iter().max().unwrap(),
                report.lambda.unwrap_or(0.0),
                report.target_bits
            ),
            None => match report.g {
                Some(g) => g.to_string(),
                None => "none (silence)".to_string(),
            },
        };
        println!(
            "stream {} bits = {:.1} bits/frame ({:.3} kbit/s at 32768 Hz); {} nonzero coefficients",
            report.stream_bits,
            report.stream_bits as f64 / report.n_frames as f64,
            report.stream_bits as f64 / (report.n_frames as f64 * 512.0 / 32768.0) / 1000.0,
            report.nonzero_coeffs,
        );
        println!(
            "g {}; xover {} hfnz {}",
            gline, report.xover, report.hfnz
        );
        if let Some(snr) = report.snr_db {
            println!(
                "verify: decoded {} frames OK; codec SNR {:.2} dB",
                report.n_frames, snr
            );
        }
    }
    std::fs::write(&out, &encoded.bytes).map_err(|e| {
        eprintln!("uli-enc: {}: {e}", out.display());
        ExitCode::from(1)
    })?;
    Ok(())
}

fn take(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, ExitCode> {
    it.next()
        .ok_or_else(|| usage(format!("{flag} needs a value")))
}

fn parse<T: std::str::FromStr>(
    it: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<T, ExitCode> {
    let value = take(it, flag)?;
    value
        .parse()
        .map_err(|_| usage(format!("{flag} {value:?} is not a number")))
}

fn usage(message: String) -> ExitCode {
    eprintln!("uli-enc: {message}\n{USAGE}");
    ExitCode::from(2)
}
