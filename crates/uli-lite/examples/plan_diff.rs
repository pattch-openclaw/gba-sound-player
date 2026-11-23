//! S8-3 (P3): **where** two blobs differ — by table field, and by frame's plan.
//!
//! Why this exists. S7's matrix localises a difference to a *container region*: "the
//! tables differ (2,204 of 5,212 bytes) and the stream diverges from byte 155". That
//! cannot say which table row, which symbol, or which frame's step. Two different
//! causes — a trainer that builds different lengths from the same counts, and a search
//! that hands it different counts — produce the same sentence.
//!
//! This tool names both. It parses the container, decodes the tables *by field* (`k`,
//! `ctype`, `rbits`, `esck`, `len_cb`, `len_q`, `len_p`, `dq`), and decodes every frame
//! of both blobs to compare the plan the stream actually declares: each frame's global
//! step `g` and M/S flag. It needs no reference build, so it works on any pair of blobs
//! — ours against the oracle's, or one of ours against another.
//!
//! The distinction it is built to settle: **if the first divergent table field is a
//! length row, the symbol counts it was trained on are the next question; if the first
//! divergence is a *frame's* plan, the trainer was handed different counts and the
//! tables are innocent.**
//!
//! Usage:
//!   cargo run --manifest-path crates/uli-lite/Cargo.toml --target <host-triple> \
//!     --example plan_diff -- A.uli B.uli [--entries N]
//!
//! Exit status: 0 when the blobs are byte-identical, 2 when they differ (the DIFF
//! convention `scripts/uli_accept.py` uses), 1 on a usage or parse error.

use std::env;
use std::fs;
use std::process;

use uli_lite::{Album, Decoder, DecoderState};

const M: usize = 512;

/// The tables blob's field at byte `off`, for a table set with `k` classes. Returns
/// `(group, name)` so the report can both count by group and name one entry exactly.
///
/// The layout is the format's, not this tool's, and it is derived from `k` rather than
/// hard-coded because a trained table set is allowed to change `k`'s row counts — the
/// same reason `CodeTables::parse_into` derives it.
fn field_at(k: usize, off: usize) -> (&'static str, String) {
    let ctype = 1;
    let rbits = ctype + k;
    let esck = rbits + k;
    let cb = esck + k;
    let lq = cb + (k + 1) * (k + 1);
    let lp = lq + 81 * k;
    let dq = lp + 256 * k;
    if off < ctype {
        return ("k", format!("k = {k}"));
    }
    if off < rbits {
        return ("ctype", format!("ctype[{}]", off - ctype));
    }
    if off < esck {
        return ("rbits", format!("rbits[{}]", off - rbits));
    }
    if off < cb {
        return ("esck", format!("esck[{}]", off - esck));
    }
    if off < lq {
        let rel = off - cb;
        return (
            "len_cb",
            format!("len_cb[{}][{}]", rel / (k + 1), rel % (k + 1)),
        );
    }
    if off < lp {
        let rel = off - lq;
        return ("len_q", format!("len_q[{}][{}]", rel / 81, rel % 81));
    }
    if off < dq {
        let rel = off - lp;
        return ("len_p", format!("len_p[{}][{}]", rel / 256, rel % 256));
    }
    let rel = off - dq;
    ("dq", format!("dq[{}][{}]", rel / 16, rel % 16))
}

/// Compare the two tables regions field by field. Returns the number of differing bytes.
fn compare_tables(a: &[u8], b: &[u8], entries: usize) -> usize {
    if a.is_empty() || b.is_empty() {
        println!("tables: one blob has no tables region");
        return if a.len() == b.len() { 0 } else { 1 };
    }
    let k = a[0] as usize;
    if k != b[0] as usize {
        println!(
            "tables: the two sets disagree on k ({} vs {}) — nothing field-wise is comparable",
            a[0], b[0]
        );
        return a.len().max(b.len());
    }
    if a.len() != b.len() {
        println!("tables: {} bytes vs {} — sizes differ", a.len(), b.len());
    }
    let n = a.len().min(b.len());
    let mut groups: Vec<(&'static str, usize)> = Vec::new();
    let mut shown = 0usize;
    let mut total = 0usize;
    for off in 0..n {
        if a[off] == b[off] {
            continue;
        }
        total += 1;
        let (group, name) = field_at(k, off);
        match groups.iter_mut().find(|(g, _)| *g == group) {
            Some(slot) => slot.1 += 1,
            None => groups.push((group, 1)),
        }
        if shown < entries {
            println!("  {name}: {} vs {}   (byte {off})", a[off], b[off]);
            shown += 1;
        }
    }
    if total == 0 {
        println!("tables: {} bytes, identical", a.len());
    } else {
        println!("tables: {total} of {} bytes differ (k = {k})", a.len());
        if total > shown {
            println!("  … {} more (--entries)", total - shown);
        }
        let mut parts: Vec<String> = groups.iter().map(|(g, c)| format!("{g} {c}")).collect();
        parts.sort();
        println!("  by field: {}", parts.join(", "));
    }
    total
}

/// Decode every frame of one blob, returning each frame's `(g, ms)`.
fn plan_of(album: &Album<'_>, label: &str) -> Result<Vec<(i32, u32)>, String> {
    let mut state = Box::new(DecoderState::new());
    let mut decoder =
        Decoder::new(*album, &mut state).map_err(|e| format!("{label}: tables: {e:?}"))?;
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut plan = Vec::with_capacity(album.n_frames() as usize);
    for f in 0..album.n_frames() {
        let info = decoder
            .decode_frame(&mut l8, &mut r8, None, None)
            .map_err(|e| format!("{label}: decode frame {f}: {e:?}"))?;
        plan.push((info.g, info.ops.ms));
    }
    Ok(plan)
}

fn main() {
    let mut args = env::args().skip(1);
    let (mut a, mut b, mut entries) = (None, None, 8usize);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--entries" => {
                entries = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| fail("--entries needs a number"));
            }
            other if a.is_none() => a = Some(other.to_string()),
            other if b.is_none() => b = Some(other.to_string()),
            other => fail(&format!("unexpected argument {other:?}")),
        }
    }
    let (Some(path_a), Some(path_b)) = (a, b) else {
        fail("usage: plan_diff A.uli B.uli [--entries N]");
    };

    let bytes_a = fs::read(&path_a).unwrap_or_else(|e| fail(&format!("{path_a}: {e}")));
    let bytes_b = fs::read(&path_b).unwrap_or_else(|e| fail(&format!("{path_b}: {e}")));
    let album_a = Album::parse(&bytes_a).unwrap_or_else(|e| fail(&format!("{path_a}: {e:?}")));
    let album_b = Album::parse(&bytes_b).unwrap_or_else(|e| fail(&format!("{path_b}: {e:?}")));

    let identical = bytes_a == bytes_b;
    println!("A = {path_a} ({} bytes)", bytes_a.len());
    println!("B = {path_b} ({} bytes)", bytes_b.len());

    // The header fields that follow from the plan, so a size difference is explained
    // rather than left to look like a container change.
    let (ha, hb) = (album_a.header(), album_b.header());
    println!(
        "header: n_frames {} vs {}, stream_halfwords {} vs {}, est max/mean cycles {}/{} vs {}/{}",
        ha.n_frames,
        hb.n_frames,
        ha.stream_halfwords,
        hb.stream_halfwords,
        ha.est_max_cycles,
        ha.est_mean_cycles,
        hb.est_max_cycles,
        hb.est_mean_cycles,
    );
    if ha.n_frames != hb.n_frames {
        println!("  frames differ — the plan comparison below covers the common prefix only");
    }

    let table_diff = compare_tables(album_a.tables(), album_b.tables(), entries);

    let plan_a = plan_of(&album_a, "A").unwrap_or_else(|e| fail(&e));
    let plan_b = plan_of(&album_b, "B").unwrap_or_else(|e| fail(&e));
    let n = plan_a.len().min(plan_b.len());
    let (mut first_g, mut first_ms) = (None, None);
    let (mut g_count, mut ms_count) = (0usize, 0usize);
    let (mut g_min_a, mut g_max_a) = (i32::MAX, i32::MIN);
    let (mut g_min_b, mut g_max_b) = (i32::MAX, i32::MIN);
    let mut worst_g = (0i32, 0usize);
    for f in 0..n {
        let (ga, msa) = plan_a[f];
        let (gb, msb) = plan_b[f];
        g_min_a = g_min_a.min(ga);
        g_max_a = g_max_a.max(ga);
        g_min_b = g_min_b.min(gb);
        g_max_b = g_max_b.max(gb);
        if ga != gb {
            g_count += 1;
            if first_g.is_none() {
                first_g = Some((f, ga, gb));
            }
            let d = (ga - gb).abs();
            if d > worst_g.0 {
                worst_g = (d, f);
            }
        }
        if msa != msb {
            ms_count += 1;
            if first_ms.is_none() {
                first_ms = Some((f, msa, msb));
            }
        }
    }
    println!("plan: {n} frames compared");
    println!(
        "  g range: A {}..{}, B {}..{}",
        g_min_a, g_max_a, g_min_b, g_max_b
    );
    match first_g {
        None => println!("  g: every frame agrees"),
        Some((f, ga, gb)) => println!(
            "  g: {g_count} frames differ; first at frame {f} (A g={ga}, B g={gb}); \
             widest |Δg| = {} at frame {}",
            worst_g.0, worst_g.1
        ),
    }
    match first_ms {
        None => println!("  ms: every frame agrees"),
        Some((f, msa, msb)) => {
            println!("  ms: {ms_count} frames differ; first at frame {f} (A {msa}, B {msb})")
        }
    }

    let any = !identical || table_diff != 0 || g_count != 0 || ms_count != 0;
    if !any {
        println!("identical: bytes, tables and plan agree");
    }
    process::exit(if any { 2 } else { 0 });
}

fn fail(msg: &str) -> ! {
    eprintln!("plan_diff: {msg}");
    process::exit(1);
}
