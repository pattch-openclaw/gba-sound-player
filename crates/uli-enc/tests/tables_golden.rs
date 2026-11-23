//! The default tables blob, compared **byte for byte against a pinned golden**.
//!
//! This is S1's answer to the row leak (`PLAN-OF-RECORD.md` → "S1 — what the review found").
//! Nothing else in the tree could catch it: `parse_into` picks a class's
//! coefficient row by `ctype` and never reads the other one, and every round-trip
//! test hands `uli-lite` the *same* `DefaultTables` object the writer used, so the
//! bytes sitting in the unused row were invisible to a green suite. A blob that
//! parses and decodes but differs from the golden is exactly the residue a
//! round-trip is blind to, and it is the whole of the E6 gate.
//!
//! The golden is the tables region of a blob produced by an independent encoder and
//! frozen here by `scripts/gen_uli_default_tables.sh`, so that no gate depends on building
//! it (E-R1). Regenerate it when the length model moves, and read the diff before accepting
//! it: the failure message below localises by section, because that localisation is what
//! turned a 1,053-byte surprise into a three-line fix.

mod common;

use uli_enc::DefaultTables;
use uli_lite::Album;
use uli_lite::format::{CT_PAIR, CT_QUAD};

/// The oracle's `tables_blob(tables_default)`, 5,212 bytes. Included rather
/// than read so a missing golden is a compile error, not a skipped test.
static REF: &[u8] = include_bytes!("fixtures/uli2_default_tables.bin");

/// The committed corpus, included for the same reason: this file has no subject
/// without it.
static TINY: &[u8] = include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli");

/// The bake's corpora. Gitignored build output, so absence is not a failure —
/// but each one present is another witness, and the count is echoed so a silent
/// path change is at least visible in the log. `premise.rs` had exactly that
/// failure: four corpora on disk, reported as "no generated corpora present".
const GENERATED: &[&str] = &[
    "examples/uli_bench/assets/biomaster.uli",
    "examples/uli_bench/assets/biotwo.uli",
    "examples/uli_bench/assets/adv.uli",
    "examples/uli_bench/assets/cross/cross.uli",
];

/// Where each section of the tables blob starts, so a mismatch can name the row
/// that moved instead of handing the reader a byte offset to decode by hand.
struct Sections {
    k: usize,
}

impl Sections {
    /// `(name, offset, row bytes, row count)`, in blob order.
    ///
    /// This restates the layout that `DefaultTables::to_blob` walks, on purpose:
    /// a diagnostic that computed its offsets by calling the writer could not
    /// report on the writer, and the independent restatement is what makes the
    /// failure message a second opinion rather than an echo. If the geometry
    /// moves, `the_frozen_golden_is_the_measured_size` and the golden itself are
    /// the two things that say so.
    fn table(&self) -> Vec<(&'static str, usize, usize, usize)> {
        let k = self.k;
        let len_cb = 1 + 3 * k;
        let len_q = len_cb + (k + 1) * (k + 1);
        let len_p = len_q + 81 * k;
        let dq = len_p + 256 * k;
        vec![
            ("k", 0, 1, 1),
            ("ctype", 1, 1, k),
            ("rbits", 1 + k, 1, k),
            ("esck", 1 + 2 * k, 1, k),
            ("len_cb", len_cb, k + 1, k + 1),
            ("len_q", len_q, 81, k),
            ("len_p", len_p, 256, k),
            ("dq", dq, 16, k),
        ]
    }

    fn name(&self, at: usize) -> String {
        for (name, base, row, count) in self.table() {
            if at >= base && at < base + row * count {
                return if name == "k" {
                    name.to_string()
                } else {
                    format!("{name} class {}", (at - base) / row)
                };
            }
        }
        "trailing padding".to_string()
    }
}

#[test]
fn the_default_tables_blob_is_the_pinned_golden() {
    let mine = DefaultTables::build().to_blob();
    assert_eq!(mine.len(), REF.len(), "the blob length moved");
    assert_eq!(mine[0], REF[0], "the class count moved");

    let diffs: Vec<usize> = (0..mine.len()).filter(|&i| mine[i] != REF[i]).collect();
    if diffs.is_empty() {
        return;
    }
    let secs = Sections { k: REF[0] as usize };
    let mut detail = String::new();
    for &d in diffs.iter().take(8) {
        detail.push_str(&format!(
            "\n  byte {d:5}: ours {:3} golden {:3}   [{}]",
            mine[d],
            REF[d],
            secs.name(d)
        ));
    }
    // Group by section: "11 classes' len_q rows" is a different bug from 900
    // scattered bytes, and seeing which one it was is what localised the leak in a
    // single read instead of a bisect.
    let mut by_section: Vec<(String, usize)> = Vec::new();
    for &d in &diffs {
        let full = secs.name(d);
        let head = full.split(' ').next().unwrap_or(full.as_str()).to_string();
        match by_section.iter_mut().find(|(h, _)| *h == head) {
            Some((_, n)) => *n += 1,
            None => by_section.push((head, 1)),
        }
    }
    let summary: Vec<String> = by_section
        .iter()
        .map(|(h, n)| format!("{h}: {n} byte(s)"))
        .collect();
    panic!(
        "{} of {} bytes differ from the oracle's default tables.{}\n\n\
         by section — {}\n\
         If the length model moved on purpose, regenerate the golden with \
         scripts/gen_uli_default_tables.sh and read the diff first.",
        diffs.len(),
        mine.len(),
        detail,
        summary.join(", ")
    );
}

/// The same invariant, read off the oracle's own bytes.
///
/// `tables_default.rs`'s unit test asserts that the unused row of every class is
/// zeros in **our** blob. Without this half, a writer and a checker that agree
/// with each other and with nobody else would pass — which is the situation the
/// row leak came out of. The oracle's zeros come from a `memset` in `tables_default`
/// that `tables_from_stats` never overwrites, and they are unchanged in the golden
/// and in all five corpora alike.
#[test]
fn the_golden_keeps_the_same_shape() {
    let t = DefaultTables::build();
    let k = t.k;
    assert_eq!(REF[0] as usize, k, "the frozen blob's class count");
    assert_eq!(REF.len(), t.tables_bytes(), "the frozen blob's length");
    let len_q = 1 + 3 * k + (k + 1) * (k + 1);
    let len_p = len_q + 81 * k;
    for c in 0..k {
        let row = |base: usize, n: usize| &REF[base + n * c..base + n * (c + 1)];
        let all_zero = |r: &[u8]| r.iter().all(|&x| x == 0);
        match t.ctype[c] {
            CT_QUAD => assert!(
                all_zero(row(len_p, 256)),
                "class {c}: the oracle's len_p row of a QUAD class is not zeros"
            ),
            CT_PAIR => assert!(
                all_zero(row(len_q, 81)),
                "class {c}: the oracle's len_q row of a PAIR class is not zeros"
            ),
            _ => assert!(
                all_zero(row(len_q, 81)) && all_zero(row(len_p, 256)),
                "class {c}: the oracle carries lengths for a ZERO class"
            ),
        }
    }
}

/// The transcribed parameter rows, witnessed against the golden.
///
/// `ctype`/`rbits`/`esck` are the format's parameter rows, transcribed into this crate, and
/// nothing in the tree compared them to anything: `premise.rs` asserts each corpus
/// *satisfies* E-R4, which a mis-transcribed row would happily do too. Bytes
/// `0..1+3k` are the part of the tables blob that is **not** retrained per album, so
/// every corpus carries the same ones and any of them is a witness.
///
/// The lengths are deliberately not compared here — every corpus is a *trained*
/// album, and they disagree with the defaults from the first length row on. That
/// comparison is the golden above, and no corpus can stand in for it.
#[test]
fn the_transcribed_rows_match_every_corpus_present() {
    let mine = DefaultTables::build().to_blob();
    let k = mine[0] as usize;
    let rows = 1 + 3 * k;

    let mut checked = 0;
    check_rows("uli2_tiny (committed fixture)", TINY, &mine, rows);
    checked += 1;
    for path in GENERATED {
        let full = common::repo_root().join(path);
        let Ok(blob) = std::fs::read(&full) else {
            eprintln!("{path}: absent (gitignored build output), not a failure");
            continue;
        };
        check_rows(path, &blob, &mine, rows);
        checked += 1;
    }
    assert!(
        checked >= 1,
        "no corpus was read, so the transcribed rows went unwitnessed"
    );
    eprintln!("transcribed rows ({rows} bytes) matched {checked} corpus/corpora");
}

fn check_rows(label: &str, blob: &[u8], mine: &[u8], rows: usize) {
    let album = Album::parse(blob).unwrap_or_else(|e| panic!("{label}: not a ULA2 blob: {e:?}"));
    let tables = album.tables();
    assert_eq!(
        tables.len() % 4,
        0,
        "{label}: tables region is not 4-padded"
    );
    assert_eq!(
        tables.len(),
        mine.len(),
        "{label}: a different k means a different region size; the rows comparison \
         below would then be comparing unrelated bytes"
    );
    assert_eq!(
        &tables[..rows],
        &mine[..rows],
        "{label}: the transcribed rows moved. Compared bytes are k, then ctype, \
         rbits and esck for each of the classes — {rows} bytes in all."
    );
}

/// The golden is 5,212 bytes because that is what the geometry computes. Pinning
/// the number means a layout edit cannot slide it silently: `to_blob` and
/// `parse_into` would still agree with each other, and every round-trip would pass.
#[test]
fn the_frozen_golden_is_the_measured_size() {
    assert_eq!(REF.len(), 5212);
    assert_eq!(DefaultTables::build().to_blob().len(), 5212);
}

/// `from_blob` is `to_blob`'s inverse, and the **oracle's** bytes are the testimony that
/// matters — a round trip through our own writer would only prove the two halves of one
/// file agree with each other.
///
/// This is also what makes a *trained* table set usable here: S8's differential codes frames
/// with the reference's trained tables (`scripts/uli_frame_diff.py --tables`), so a parser
/// that silently dropped or shifted a row would show up as a coding difference attributed to
/// the coder.
#[test]
fn from_blob_reads_the_oracles_default_set_back() {
    let parsed = DefaultTables::from_blob(REF).expect("the committed default set parses");
    assert_eq!(parsed.k, 14, "the default set declares 14 classes");
    assert_eq!(
        parsed.to_blob(),
        REF,
        "from_blob(to_blob(x)) is not REF: the blob is not the set this crate builds"
    );
    assert_eq!(
        parsed.to_blob(),
        DefaultTables::build().to_blob(),
        "the oracle's default set and ours differ — the S1 golden says they must not"
    );
    // The shape that made the row leak invisible: each class has one coefficient row and
    // the other shape's is zeros. `from_blob` must leave the unused one empty rather than
    // build a codebook out of it.
    for c in 0..parsed.k {
        let used = match parsed.ctype[c] {
            CT_QUAD => 81,
            CT_PAIR => 256,
            _ => 0,
        };
        assert_eq!(
            parsed.cls[c].len.len(),
            used,
            "class {c} is ctype {} so its codebook must hold {used} lengths",
            parsed.ctype[c]
        );
    }
}
