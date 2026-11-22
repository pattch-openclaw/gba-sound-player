//! `uli-enc` — the encoder's command line.
//!
//! The flag names and meanings are a fixed interface rather than a preference: the
//! acceptance harness drives this binary and an external one with identical arguments, so
//! `--budget`, `--cap` and `--kappa` cannot be renamed freely. The rest are this encoder's
//! own knobs — `--g` for a fixed step, `--rd` for the search, `--train` for the two-pass
//! tables, `--stride`/`--fine` for the candidate grid, `--ms` to pin the stereo domain
//! for a fixed-step comparison, and `--musearch 0|1` to control the cycle-aware re-code
//! (S6; on by default, because the cap is a playback constraint and meeting it is the
//! shipping behaviour — `0` is the A/B arm).
//!
//! **Two input shapes, one boundary.** `--album`/`--tracks` *is* the boundary E-R2 fixes;
//! `--wav` is the convenience layer on top of it (S5), and `--emit-pcm DIR` writes back the
//! pair it assembled so the front end is auditable rather than a black box. Supplying both
//! shapes is refused: a caller that does has not said which album it means.
//!
//! A refusal is an error exit that cites needed and allowed bytes, and it
//! happens before `--out` is created: the encoder either writes a whole album or
//! nothing at all.
//!
//! **The printed lines are the oracle's columns**, because S7's acceptance matrix compares
//! the two encoders and a diff has to be readable: album summary, container overhead and bit
//! target, the written size against the budget, the `g` range and M/S share, and the verify
//! block's per-track pcm16 / int8 SNR. Ours prints `dacw off` where the oracle prints its
//! DAC-weight pass number: that pass does not exist here, and a made-up `0` would read as a
//! knob that ran.

use std::path::PathBuf;
use std::process::ExitCode;

use uli_enc::rd::CYCLE_PERIOD;
use uli_enc::{Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, Input, encode};
use uli_lite::format::{M, NCODED, RATE};

const USAGE: &str = "usage: uli-enc {--album F --tracks TSV | --wav F [--wav F ...]} --out U \
                     [--budget BYTES] [--cap PCT] [--threads N] [--g N] [--rd] [--no-train] \
                     [--kappa X] [--stride N] [--fine N] [--ms] [--musearch 0|1] \
                     [--emit-pcm DIR]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn run() -> Result<(), ExitCode> {
    let mut album: Option<PathBuf> = None;
    let mut tracks: Option<PathBuf> = None;
    let mut wavs: Vec<PathBuf> = Vec::new();
    let mut emit_pcm: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut budget = DEFAULT_BUDGET;
    let mut cap_pct = DEFAULT_CAP_PCT;
    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut threads = default_threads;
    // No `--g` and no `--rd` and the album is S1's: silence, sized and refused exactly as
    // before. A `--wav` input refuses that combination instead, because real audio written
    // as silence is the silent failure S3's ledger is about.
    let mut g: Option<i32> = None;
    let mut rd = false;
    // Training is the shipped path (S4) and `--no-train` is the A/B arm, as before.
    let mut train = true;
    let mut kappa = uli_enc::DEFAULT_KAPPA;
    let mut stride = 8i32;
    let mut fine_width = 4i32;
    // `--ms` forces M/S; absent, the encoder chooses per frame.
    let mut ms: Option<bool> = None;
    // The cycle-aware re-code is the shipped default (S6): the cap is a playback constraint,
    // so meeting it is what the product does. `--musearch 0` is the A/B arm.
    let mut musearch = true;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--album" => album = Some(PathBuf::from(take(&mut it, "--album")?)),
            "--tracks" => tracks = Some(PathBuf::from(take(&mut it, "--tracks")?)),
            "--wav" => wavs.push(PathBuf::from(take(&mut it, "--wav")?)),
            "--emit-pcm" => emit_pcm = Some(PathBuf::from(take(&mut it, "--emit-pcm")?)),
            "--out" => out = Some(PathBuf::from(take(&mut it, "--out")?)),
            "--budget" => budget = parse(&mut it, "--budget")?,
            "--cap" => cap_pct = parse(&mut it, "--cap")?,
            "--threads" => threads = parse(&mut it, "--threads")?,
            "--g" => g = Some(parse(&mut it, "--g")?),
            "--rd" => rd = true,
            "--no-train" => train = false,
            "--kappa" => kappa = parse(&mut it, "--kappa")?,
            "--stride" => stride = parse(&mut it, "--stride")?,
            "--fine" => fine_width = parse(&mut it, "--fine")?,
            "--musearch" => {
                let v: u32 = parse(&mut it, "--musearch")?;
                musearch = match v {
                    0 => false,
                    1 => true,
                    _ => return Err(usage("--musearch takes 0 or 1".into())),
                };
            }
            "--ms" => ms = Some(true),
            other => return Err(usage(format!("unknown argument {other:?}"))),
        }
    }

    let Some(out) = out else {
        return Err(usage("--out is required".into()));
    };
    let input = match (album, tracks) {
        (Some(album), Some(tracks)) if wavs.is_empty() => Input::Pcm { album, tracks },
        (None, None) if !wavs.is_empty() => Input::Wav(wavs),
        (Some(_), None) | (None, Some(_)) => {
            return Err(usage("--album and --tracks are given together".into()));
        }
        _ if !wavs.is_empty() => {
            return Err(usage(
                "--wav and --album/--tracks are two input shapes, not two halves: give \
                 one. The PCM pair is the contract boundary; --wav assembles one from \
                 files, and a caller that supplies both has not said which album it means"
                    .into(),
            ));
        }
        _ => {
            return Err(usage(
                "no input: give --album/--tracks, or --wav one or more times".into(),
            ));
        }
    };
    if emit_pcm.is_some() && input.is_pcm_pair() {
        return Err(usage(
            "--emit-pcm dumps the PCM a --wav front end assembled; with --album/--tracks \
             that pair is already the input"
                .into(),
        ));
    }
    if rd && g.is_some() {
        return Err(usage(
            "--rd searches for g, so it cannot also be given one".into(),
        ));
    }

    let args = Args {
        input,
        budget,
        cap_pct,
        threads,
        g,
        rd,
        kappa,
        stride,
        fine_width,
        train,
        ms,
        musearch,
    };
    let encoded = encode(&args).map_err(|e| {
        eprintln!("uli-enc: {e}");
        ExitCode::from(1)
    })?;
    let report = &encoded.report;

    // The oracle's album line, then its sizing line. Printed after the encode because
    // this encoder does not stream its progress; the columns are the same.
    println!(
        "album: {} samples ({:.1} s), {} tracks, {} frames; budget {} bytes; cap {:.1}%; \
         dacw off; musearch {}; threads {}",
        report.n_samples,
        f64::from(report.n_samples) / f64::from(uli_lite::format::RATE),
        report.n_tracks,
        report.n_frames,
        budget,
        cap_pct,
        if report.musearch { "on" } else { "off" },
        threads
    );
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
                "g range {}..{}, M/S frames {:.1}% (lambda {:.3e}, target {} bits)",
                v.iter().min().unwrap(),
                v.iter().max().unwrap(),
                100.0 * report.ms_frames as f64 / report.n_frames as f64,
                report.lambda.unwrap_or(0.0),
                report.target_bits
            ),
            None => match report.g {
                Some(g) => format!(
                    "g {} (fixed), M/S frames {:.1}%",
                    g,
                    100.0 * report.ms_frames as f64 / report.n_frames as f64
                ),
                None => "g none (silence)".to_string(),
            },
        };
        println!(
            "stream {} bits = {:.1} bits/frame ({:.3} kbit/s at {RATE} Hz); {} nonzero \
             coefficients; tables {}",
            report.stream_bits,
            report.stream_bits as f64 / report.n_frames as f64,
            report.stream_bits as f64
                / (report.n_frames as f64 * M as f64 / f64::from(RATE))
                / 1000.0,
            report.nonzero_coeffs,
            if report.trained {
                "trained"
            } else {
                "built-in"
            }
        );
        println!("{gline}; xover {} hfnz {}", report.xover, report.hfnz);
        if report.musearch {
            println!(
                "musearch: {} of {} frames re-coded under a cycle price (mu > 0)",
                report.mu_frames, report.n_frames
            );
        }
        // The oracle's verify block, column for column.
        println!(
            "verify: decoded {} frames OK, |X| overflows {}, nonzero coefs >= {}: {}, \
             model max cycles {:.1}%",
            report.n_frames,
            report.xover,
            NCODED,
            report.hfnz,
            100.0 * f64::from(report.est_max_cycles) / f64::from(CYCLE_PERIOD)
        );
        if let Some(snr) = report.snr_db {
            println!("verify: album codec SNR {snr:.2} dB (pooled over the album)");
        }
        if !report.track_snr.is_empty() {
            let pairs: Vec<String> = report
                .track_snr
                .iter()
                .map(|t| match (t.pcm16, t.int8) {
                    (Some(a), Some(b)) => format!("{a:.2}/{b:.2}"),
                    _ => "-".to_string(),
                })
                .collect();
            println!(
                "verify: per-track time-domain SNR (full band, vs input) pcm16 / int8: {}",
                pairs.join(" ")
            );
            if let (Some(a), Some(b)) = (mean16(&report.track_snr), mean8(&report.track_snr)) {
                println!("verify: mean {a:.2} / {b:.2} dB (mean of per-track)");
            }
        }
        println!(
            "est. cycles mean {:.1}% max {:.1}% of the period",
            100.0 * f64::from(report.est_mean_cycles) / f64::from(CYCLE_PERIOD),
            100.0 * f64::from(report.est_max_cycles) / f64::from(CYCLE_PERIOD)
        );
    }
    if let Some(dir) = &emit_pcm {
        let boundary = encoded.boundary.as_ref().ok_or_else(|| {
            eprintln!("uli-enc: --emit-pcm needs a --wav input");
            ExitCode::from(1)
        })?;
        std::fs::create_dir_all(dir).map_err(|e| {
            eprintln!("uli-enc: {}: {e}", dir.display());
            ExitCode::from(1)
        })?;
        let album_f32 = dir.join("album.f32");
        let tracks_tsv = dir.join("tracks.tsv");
        boundary.samples.write_f32(&album_f32).map_err(|e| {
            eprintln!("uli-enc: {e}");
            ExitCode::from(1)
        })?;
        boundary.pcm.write_tracks(&tracks_tsv).map_err(|e| {
            eprintln!("uli-enc: {e}");
            ExitCode::from(1)
        })?;
        println!(
            "wrote {} and {} ({} samples, the pair this blob was encoded from)",
            album_f32.display(),
            tracks_tsv.display(),
            boundary.pcm.n_samples
        );
    }
    std::fs::write(&out, &encoded.bytes).map_err(|e| {
        eprintln!("uli-enc: {}: {e}", out.display());
        ExitCode::from(1)
    })?;
    let unused = budget.saturating_sub(encoded.bytes.len() as u64);
    println!(
        "wrote {}: {} bytes (budget {}, {} unused); {:.3} kbit/s stream",
        out.display(),
        encoded.bytes.len(),
        budget,
        unused,
        report.stream_bits as f64 / (f64::from(report.n_samples) / f64::from(RATE)) / 1000.0
    );
    Ok(())
}

fn mean16(tracks: &[uli_enc::TrackSnr]) -> Option<f64> {
    mean(tracks.iter().filter_map(|t| t.pcm16))
}

fn mean8(tracks: &[uli_enc::TrackSnr]) -> Option<f64> {
    mean(tracks.iter().filter_map(|t| t.int8))
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
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
