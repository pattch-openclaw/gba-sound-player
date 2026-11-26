//! S8-6: the trainer, fed the reference's own pass-1 plan.
//!
//! Why this exists. A blob carries the plan that was *emitted* and the table set trained on
//! the **previous** pass's plan, so the input the trainer actually consumed is not in the
//! artefact. S8-2's `--dump-plan` (`scripts/uli_dump_plan.py` / `make uli-dump-plan`) writes
//! that input: the pass-1 plan, the four `train-itN` `EncStats` sets that build the initial
//! tables, and the `retrain` set that produces the blob's table region. This binary is the
//! other side of that instrument: it rebuilds the initial tables from the dumped train
//! stats, then checks the final table region two ways —
//!
//!   (a) `train::build` from the dumped `retrain` stats, which isolates the trainer's
//!       table construction (Huffman lengths, `dq`, the carry-over of `dq` rows with too few
//!       observations to be fitted);
//!   (b) `train::collect_frames` over the dumped pass-1 plan, sampled every second frame, which
//!       adds the coder and the collector to the chain. This is S8-6's gate as written:
//!       feed the plan to `collect` + `build` and require the reference blob's table region.
//!
//! The gate is the **5,212-byte table region, `dq` included**, byte for byte. The integer
//! counts (`cb`, `q`, `p`, `cen_n`) must also match the dumped `retrain` stats exactly — a
//! count difference is a real trainer-input divergence even when this cell's tables happen
//! to survive it. `cen_s` is reported as a ULP distance, not asserted: it is a sum of
//! doubles whose reference-side order is the OpenMP chunking of `count_frames`
//! (`stats_add` over thread-local structs), and its consumer is `round(16·cen_s/cen_n)`.
//! Measured on S8-4's `fast` cell, our frame-order sum differs by at most 1 ULP on twelve
//! values and the rounded `dq` is identical (S8-6; inventory row 20).
//!
//! Usage:
//!   train-dump --dump PLAN.txt --blob REF.uli --album A.f32 --tracks T.tsv [--kappa 0.13]
//!
//! Exit status: 0 when both table checks match and the counts are exact; 2 on a DIFF (the
//! `uli_accept.py` convention); 1 on a harness/parse error.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process;

use uli_enc::pcm::{Pcm, Samples};
use uli_enc::plan::{Content, GPlan};
use uli_enc::quant::{quantise_chan_rd, rotate};
use uli_enc::tables_default::DefaultTables;
use uli_enc::train::{self, Stats};
use uli_lite::Album;
use uli_lite::format::{CT_PAIR, M, MAXK};

const K: usize = 14;

fn usage(msg: &str) -> ! {
    eprintln!("train-dump: {msg}");
    process::exit(1);
}

fn hex_f64(s: &str) -> f64 {
    if s.len() != 16 {
        usage(&format!("not a 16-digit f64 bit pattern: {s:?}"));
    }
    let mut b = [0u8; 8];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        b[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap_or_else(|_| {
            usage(&format!("not a hex f64 bit pattern: {s:?}"));
        });
    }
    f64::from_be_bytes(b)
}

/// A stats set from the dump, in the trainer's units.
struct Round {
    stats: Stats,
}

fn empty_round(base: &DefaultTables) -> Round {
    let mut stats = Stats::default();
    stats.cls = std::array::from_fn(|_| Vec::new());
    for (c, slot) in stats.cls.iter_mut().enumerate() {
        let n = if base.ctype[c] == CT_PAIR { 256 } else { 81 };
        *slot = vec![0.0; n];
    }
    Round { stats }
}

fn parse_dump(text: &str, base: &DefaultTables) -> (BTreeMap<String, Round>, Vec<(i32, bool)>) {
    let mut rounds: BTreeMap<String, Round> = BTreeMap::new();
    let mut pass1 = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        if t[0] == "plan" && t.get(1) == Some(&"pass1") {
            // plan pass1 attempt 0 frame F g G ms M bits … D … cycles … mu …
            if t.len() < 10 {
                usage(&format!("dump line {}: short plan line: {line}", n + 1));
            }
            let f: usize = t[5].parse().unwrap_or_else(|_| usage("bad frame index"));
            let g: i32 = t[7].parse().unwrap_or_else(|_| usage("bad g"));
            let ms: u32 = t[9].parse().unwrap_or_else(|_| usage("bad ms"));
            if f != pass1.len() {
                usage(&format!(
                    "dump line {}: pass1 frame {f}, expected {}",
                    n + 1,
                    pass1.len()
                ));
            }
            pass1.push((g, ms != 0));
            continue;
        }
        if t[0] != "stats" {
            continue;
        }
        let tag = t[1].to_string();
        let round = rounds
            .entry(tag.clone())
            .or_insert_with(|| empty_round(base));
        if t[2] == "esc" {
            // stats TAG esc H frames H ms_frames H
            if t.len() != 8 {
                usage(&format!("dump line {}: bad stats header: {line}", n + 1));
            }
            round.stats.frames = hex_f64(t[5]) as usize;
            continue;
        }
        let a: usize = t[3].parse().unwrap_or_else(|_| usage("bad stats index"));
        let b: usize = t[4].parse().unwrap_or_else(|_| usage("bad stats index"));
        let h = t[5];
        match t[2] {
            "cb" => round.stats.cb[a][b] = hex_f64(h),
            "q" | "p" => {
                if a >= MAXK || b >= round.stats.cls[a].len() {
                    usage(&format!("dump line {}: class symbol out of range", n + 1));
                }
                round.stats.cls[a][b] = hex_f64(h);
            }
            "cen_s" => round.stats.cen_s[a][b] = hex_f64(h),
            "cen_n" => round.stats.cen_n[a][b] = hex_f64(h),
            other => usage(&format!("dump line {}: unknown stats field {other}", n + 1)),
        }
    }
    (rounds, pass1)
}

/// The tables blob's field at byte `off`, derived from `k` (the format's layout; the same
/// derivation `plan_diff` and `CodeTables::parse_into` use). Only the first differing byte
/// is named, because that is the finding; the rest is `make uli-plan-diff`'s job.
fn field_at(k: usize, off: usize) -> String {
    let ctype = 1;
    let rbits = ctype + k;
    let esck = rbits + k;
    let cb = esck + k;
    let lq = cb + (k + 1) * (k + 1);
    let lp = lq + 81 * k;
    let dq = lp + 256 * k;
    match off {
        o if o < ctype => "k".into(),
        o if o < rbits => format!("ctype[{}]", o - ctype),
        o if o < esck => format!("rbits[{}]", o - rbits),
        o if o < cb => format!("esck[{}]", o - esck),
        o if o < lq => {
            let r = o - cb;
            format!("len_cb[{}][{}]", r / (k + 1), r % (k + 1))
        }
        o if o < lp => {
            let r = o - lq;
            format!("len_q[{}][{}]", r / 81, r % 81)
        }
        o if o < dq => {
            let r = o - lp;
            format!("len_p[{}][{}]", r / 256, r % 256)
        }
        o => {
            let r = o - dq;
            format!("dq[{}][{}]", r / 16, r % 16)
        }
    }
}

fn compare_tables(label: &str, got: &[u8], want: &[u8]) -> bool {
    if got.len() != want.len() {
        println!(
            "  {label}: ours {} B, reference {} B — sizes differ",
            got.len(),
            want.len()
        );
        return false;
    }
    let mut diffs = 0usize;
    let mut first = None;
    for (i, (&a, &b)) in got.iter().zip(want).enumerate() {
        if a != b {
            diffs += 1;
            first.get_or_insert(i);
        }
    }
    match first {
        None => {
            println!("  {label}: {} bytes, identical", got.len());
            true
        }
        Some(i) => {
            println!(
                "  {label}: {diffs} of {} bytes differ; first {}: ours {} vs reference {}",
                got.len(),
                field_at(want[0] as usize, i),
                got[i],
                want[i]
            );
            false
        }
    }
}

fn ulp(a: f64, b: f64) -> u64 {
    fn ordered(v: f64) -> u64 {
        let bits = v.to_bits();
        if bits & (1 << 63) != 0 {
            !bits
        } else {
            bits | (1 << 63)
        }
    }
    ordered(a).abs_diff(ordered(b))
}

fn compare_stats(want: &Stats, got: &Stats) -> bool {
    let mut count_diffs = 0usize;
    let mut first = None;
    for a in 0..=K {
        for b in 0..=K {
            if want.cb[a][b] != got.cb[a][b] {
                count_diffs += 1;
                first.get_or_insert(format!(
                    "cb[{a}][{b}]: want {} got {}",
                    want.cb[a][b], got.cb[a][b]
                ));
            }
        }
    }
    for c in 0..K {
        if want.cls[c].len() != got.cls[c].len() {
            count_diffs += 1;
            first.get_or_insert(format!(
                "class {c}: symbol rows {} vs {}",
                want.cls[c].len(),
                got.cls[c].len()
            ));
            continue;
        }
        for s in 0..want.cls[c].len() {
            if want.cls[c][s] != got.cls[c][s] {
                count_diffs += 1;
                first.get_or_insert(format!(
                    "cls[{c}][{s}]: want {} got {}",
                    want.cls[c][s], got.cls[c][s]
                ));
            }
        }
        for m in 1..16 {
            if want.cen_n[c][m] != got.cen_n[c][m] {
                count_diffs += 1;
                first.get_or_insert(format!(
                    "cen_n[{c}][{m}]: want {} got {}",
                    want.cen_n[c][m], got.cen_n[c][m]
                ));
            }
        }
    }
    let mut worst = 0u64;
    let mut ulp_values = 0usize;
    for c in 0..K {
        for m in 1..16 {
            let d = ulp(want.cen_s[c][m], got.cen_s[c][m]);
            if d != 0 {
                ulp_values += 1;
                worst = worst.max(d);
            }
        }
    }
    match first {
        None => println!(
            "  counts: cb/q/p/cen_n exact; frames {} vs {}",
            want.frames, got.frames
        ),
        Some(f) => println!("  counts: {count_diffs} differ; first {f}"),
    }
    println!(
        "  cen_s: {ulp_values} values differ, worst {worst} ULP (the reference sums by \
         OpenMP chunk order; only the rounded `dq` consumes this)"
    );
    let frames_ok = want.frames == got.frames;
    if !frames_ok {
        println!(
            "  frames: want {} got {} — the sampling differs",
            want.frames, got.frames
        );
    }
    count_diffs == 0 && frames_ok
}

fn main() {
    let mut dump_path = None;
    let mut blob_path = None;
    let mut album_path = None;
    let mut tracks_path = None;
    let mut kappa = 0.13f64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut take = |name: &str| {
            args.next()
                .unwrap_or_else(|| usage(&format!("{name} needs a value")))
        };
        match arg.as_str() {
            "--dump" => dump_path = Some(take("--dump")),
            "--blob" => blob_path = Some(take("--blob")),
            "--album" => album_path = Some(take("--album")),
            "--tracks" => tracks_path = Some(take("--tracks")),
            "--kappa" => {
                kappa = take("--kappa")
                    .parse()
                    .unwrap_or_else(|_| usage("bad --kappa"))
            }
            other => usage(&format!("unknown argument {other:?}")),
        }
    }
    let (Some(dump_path), Some(blob_path), Some(album_path), Some(tracks_path)) =
        (dump_path, blob_path, album_path, tracks_path)
    else {
        usage("usage: train-dump --dump PLAN.txt --blob REF.uli --album A.f32 --tracks T.tsv");
    };

    let text =
        fs::read_to_string(&dump_path).unwrap_or_else(|e| usage(&format!("{dump_path}: {e}")));
    let blob = fs::read(&blob_path).unwrap_or_else(|e| usage(&format!("{blob_path}: {e}")));
    let album = Album::parse(&blob).unwrap_or_else(|e| usage(&format!("{blob_path}: {e:?}")));
    let want_tables = album.tables();

    let base = DefaultTables::build();
    let (rounds, pass1) = parse_dump(&text, &base);
    let mut train_rounds: Vec<&String> = rounds
        .keys()
        .filter(|tag| tag.starts_with("train-it"))
        .collect();
    train_rounds.sort_by_key(|tag| {
        tag.trim_start_matches("train-it")
            .parse::<usize>()
            .unwrap_or(0)
    });
    let Some(retrain) = rounds.get("retrain") else {
        usage("the dump has no `retrain` stats — S8-2's dump predates this check");
    };
    if train_rounds.len() != 4 {
        usage(&format!(
            "the dump has {} train rounds, expected 4",
            train_rounds.len()
        ));
    }

    println!(
        "train-dump: {} frames, {} train rounds, retrain ({} sampled frames)",
        pass1.len(),
        train_rounds.len(),
        retrain.stats.frames
    );

    // The initial tables are a pure function of the default set and the train rounds' stats.
    let mut initial = base.clone();
    for tag in &train_rounds {
        initial = train::build(&initial, &rounds[*tag].stats);
    }

    // (a) the trainer alone: the dumped retrain stats -> tables.
    let t_a = train::build(&initial, &retrain.stats);
    let a_ok = compare_tables("(a) dumped stats -> build", &t_a.to_blob(), want_tables);

    // (b) the whole trainer input: code the dumped pass-1 plan every second frame.
    let pcm = Pcm::open(Path::new(&album_path), Path::new(&tracks_path))
        .unwrap_or_else(|e| usage(&format!("PCM: {e}")));
    let n_samples = pcm.n_samples as usize;
    let samples = Samples::open(Path::new(&album_path), n_samples)
        .unwrap_or_else(|e| usage(&format!("PCM samples: {e}")));
    let n_frames = (n_samples.div_ceil(M)) + 1;
    if pass1.len() != n_frames {
        usage(&format!(
            "the dump has {} pass-1 frames, the album has {n_frames}",
            pass1.len()
        ));
    }
    let mut gs = Vec::with_capacity(n_frames);
    let mut msf = Vec::with_capacity(n_frames);
    let mut chans = Vec::with_capacity(n_frames);
    let mut coef = [0f32; 2 * M];
    for (f, &(g, ms)) in pass1.iter().enumerate() {
        gs.push(g);
        msf.push(ms);
        uli_enc::mdct::mdct_frame(&samples, f, n_samples, &mut coef);
        let rot = rotate(&coef, ms);
        let l = quantise_chan_rd(&rot[..M], &initial, g, ms, kappa, 0.0).chan;
        let r = quantise_chan_rd(&rot[M..], &initial, g, ms, kappa, 0.0).chan;
        chans.push([l, r]);
    }
    let content = Content::coded(GPlan::new(gs, msf), chans);
    let sampled: Vec<usize> = (0..n_frames).step_by(2).collect();
    let ours = train::collect_frames(&samples, n_samples, &initial, &content, kappa, &sampled);
    let stats_ok = compare_stats(&retrain.stats, &ours);
    let t_b = train::build(&initial, &ours);
    let b_ok = compare_tables(
        "(b) pass-1 plan -> collect -> build",
        &t_b.to_blob(),
        want_tables,
    );

    if a_ok && b_ok && stats_ok {
        println!(
            "train-dump: MATCH — the table region is the reference's ({} bytes, dq included)",
            want_tables.len()
        );
        return;
    }
    println!("train-dump: DIFF — the trainer does not reproduce the reference's table region");
    process::exit(2);
}
