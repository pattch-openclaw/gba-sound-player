//! The **closed-world premise** the product bit reader is built on.
//!
//! `dec_chan` in the product build (no `checked-reader`) carries no bounds test,
//! no error flag and a **16-bit** refill contract, because we own the encoder,
//! the container and the bake: a malformed stream is supposed to fail the build,
//! not a frame (ULI2.md, decision log 2026-11-19; `PLAN-OF-RECORD.md`, "the
//! entropy path against the C+ASM reference", P1).
//!
//! Two properties have to hold for that to be sound, and neither is a property
//! of a *code path* — they are properties of the **tables in the blob**, so this
//! is where they are asserted. It is the seed of the bake gate: the same checks
//! belong in whatever produces the shipped album.
//!
//! ## What the 16-bit contract actually constrains
//!
//! The contract is not "a group costs ≤ 16 bits" — it is that **every run of
//! reads between two `refill!`s is ≤ 16**, because one halfword load always
//! restores exactly 16 valid bits (P3). Walking the pair arm (the only arm that
//! reads a raw field, and so the only one where these two bounds bite):
//!
//! | run | reads | bound |
//! |---|---|---|
//! | band symbol | `huff_consume!` takes `len` | `len <= 16` (LUT 1..=10, `huff_slow` 11..=16) |
//! | coefficient symbol | `huff_consume!` takes `len` | `len <= 16`, same |
//! | escape field | `t = 2z + 1 + esck` with `z <= 5` (`CLZ6`'s fast path) | `t <= 11 + esck` |
//! | raw digit | `top(r)` + `take(r)` + one sign bit | `r + 1` |
//!
//! The escape and the raw digit **do not stack**: `esc!` refills after taking
//! `t`, and the raw pair arm refills again between the two coefficients of the
//! group (`decoder.rs`, "a raw field plus a sign bit can consume the whole
//! contract window"). So each of the last two rows is measured from its own
//! ≥ 16-bit window, and the window arithmetic alone permits:
//!
//! * `t <= 16`, i.e. **`esck <= 5`**
//! * `r + 1 <= 16`, i.e. **`rbits <= 15`**
//!
//! Both bounds this test enforces are **tighter than that**, by choice, and that
//! is the point: the table parser accepts `rbits <= 16` and `esck <= 24`
//! (`uli_lite::tables::CodeTables::parse_into`) and the *parser's* cap is already
//! one bit looser than the product contract allows at `rbits == 16`. Nothing
//! downstream of the encoder rejects a blob that breaks the reader, so the
//! encoder owns it — that is requirement **E-R4** in
//! [`PLAN-OF-RECORD.md`](../uli-lite/PLAN-OF-RECORD.md) → "The encoder — the last
//! missing piece", and the numbers below are the numbers E-R4 states.
//!
//! ## The measured census, and the correction this file carries
//!
//! Measured 2026-11-19 by parsing each corpus through `CodeTables::parse_into`,
//! per class as `ctype/rbits/esck` — **the same class structure in all five**:
//!
//! ```text
//! 0/0/0  1/0/0  1/0/0  2/0/2 2/0/2 2/0/2 2/0/2 2/0/2 2/0/2  2/1/1 2/2/1 2/3/1 2/4/1 2/5/1
//! ```
//!
//! one `ZERO`, **two `QUAD`** (`ctype == 1`, `rbits` never read), **eleven
//! `PAIR`** (`ctype == 2`) with `rbits = 0,0,0,0,0,0,1,2,3,4,5` — so
//! **`max rbits == 5`, not 0** — and `max esck == 2`.
//!
//! "Same class structure" is as far as the identity goes, and the distinction
//! matters: the 5,212-byte tables region differs per corpus (FNV-1a `0f5e1b96…`
//! for the fixture against `89c4eb31…` `biomaster`, `10ac765c…` `biotwo`,
//! `64809502…` `adv`, `93ef18f5…` `cross`), because the Huffman **code lengths
//! are trained per album** while the `ctype`/`rbits`/`esck` rows are the format's
//! fixed defaults. This test reads only the three fixed fields, which is exactly
//! the set E-R4 bounds.
//!
//! Until 2026-11-19 this file asserted `rbits == 0` on every PAIR class, and it
//! *passed* — because it compared against a local `const CT_PAIR: u8 = 1`, which
//! is `CT_QUAD`. The bound ran over the two QUAD classes, whose `rbits` is
//! trivially 0 and is never read by any decoder, so the eleven classes that do
//! carry a raw field were never examined. The pass was vacuous, and the prose
//! that cited it ("both pair classes have `rbits == 0`", "one `ZERO`, two `PAIR`
//! … eleven `QUAD`") had transposed the two class names along with it. The
//! 16-bit contract was never *unsafe* (`r + 1 <= 6` there, and the arm refills
//! mid-group) — it was unjustified. Two rules follow from that and are applied
//! here: **import the format's constants instead of restating them**, and
//! **witness the checker, not just the data** — see
//! [`the_checker_names_a_pair_class_that_breaks_the_bound`], which fails loudly
//! if the class id stops matching the bound's subject.
//!
//! Data-dependent vacuity is *not* a failure: an album whose pair classes all
//! have `rbits == 0` satisfies E-R4 trivially. That census is reported rather
//! than required — but the *checker's* vacuity is, and it is witnessed by the
//! control test above and by `assert_premise`'s `pair >= 1`.
//!
//! A second bug in the same file hid four of those five corpora. The generated
//! paths were joined to `CARGO_MANIFEST_DIR` with one `../` too many, so they
//! resolved **outside the repository**: the test printed "no generated corpora
//! present" while all four blobs sat on disk, and only the fixture was ever
//! checked. They are now anchored to the repo root, and because silence about a
//! corpus that exists is the same failure mode as a bound that tests nothing,
//! the test fails if the assets directory holds `.uli` files it did not read.

use uli_lite::format::{CT_PAIR, CT_QUAD, CT_ZERO, MAXK};
use uli_lite::{Album, CodeTables};

static FIXTURE_STORAGE: uli_lite::Aligned<
    { include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli").len() },
> = uli_lite::Aligned(*include_bytes!(
    "../../uli-lite/tests/fixtures/uli2_tiny.uli"
));

/// The corpora the bake step produces, relative to the **repository root**.
/// Gitignored, so absence is not a failure — but a wrong prefix is, and these
/// paths had one: joined to `CARGO_MANIFEST_DIR` with three `../` they pointed
/// *outside* the repo, so four corpora that were on disk went unchecked and the
/// test reported "no generated corpora present". See
/// [`the_generated_corpora_satisfy_the_closed_world_premise_when_present`].
const GENERATED: &[&str] = &[
    "examples/uli_bench/assets/biomaster.uli",
    "examples/uli_bench/assets/biotwo.uli",
    "examples/uli_bench/assets/adv.uli",
    "examples/uli_bench/assets/cross/cross.uli",
];

/// The repository root, derived from this crate's manifest dir (`crates/<name>`).
/// Test-relative paths are otherwise anchored to the manifest, which is where the
/// off-by-one above came from.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("this crate lives at <repo>/crates/<name>")
        .to_path_buf()
}

/// **E-R4**, the raw-field bound. `r + 1` bits follow a refill, so the window
/// itself would allow 15; 5 is what every corpus this tree owns measures and
/// what the encoder is required to hold, leaving 10 bits of margin.
const PAIR_RBITS_MAX: u8 = 5;

/// **E-R4**, the escape bound. `t = 2z + 1 + esck <= 11 + esck`, so the window
/// would allow 5; 2 is the measured maximum and the required bound.
const ESCK_MAX: u8 = 2;

/// What one corpus's tables look like, kept so the tests can assert the check
/// actually covered something instead of trusting a loop that may have run over
/// no class at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Census {
    zero: usize,
    pair: usize,
    quad: usize,
    max_pair_rbits: u8,
    max_esck: u8,
}

/// Check `tables` against E-R4 and return what was seen. `Err` names the first
/// violation; it returns a `Result` rather than panicking so the negative-control
/// tests below can assert on the rejection itself.
fn premise_check(tables: &CodeTables) -> Result<Census, String> {
    let k = tables.k;
    if k > MAXK {
        return Err(format!("k={k} exceeds MAXK={MAXK}"));
    }

    let mut c = Census {
        zero: 0,
        pair: 0,
        quad: 0,
        max_pair_rbits: 0,
        max_esck: 0,
    };

    for i in 0..k {
        match tables.ctype[i] {
            CT_ZERO => c.zero += 1,
            CT_QUAD => c.quad += 1,
            CT_PAIR => {
                c.pair += 1;
                // The raw digit is read only here, which is exactly why the
                // class id has to be the real one.
                if tables.rbits[i] > PAIR_RBITS_MAX {
                    return Err(format!(
                        "class {i} is PAIR with rbits={} > {PAIR_RBITS_MAX}: a raw pair \
                         digit costs rbits + 1 bits after the group's refill, so the \
                         encoder must refuse it (the 16-bit window itself only breaks at \
                         rbits > 15, and the table parser accepts 16 — E-R4 is the \
                         tighter bound by choice)",
                        tables.rbits[i]
                    ));
                }
                c.max_pair_rbits = c.max_pair_rbits.max(tables.rbits[i]);
            }
            other => {
                return Err(format!(
                    "class {i} has ctype={other}, which is not ZERO({CT_ZERO}) / \
                     QUAD({CT_QUAD}) / PAIR({CT_PAIR})"
                ));
            }
        }
        // esck is read on PAIR and QUAD alike (both can escape), so it is bound
        // on every class, ZERO included — stricter than needed, and cheap.
        if tables.esck[i] > ESCK_MAX {
            return Err(format!(
                "class {i} has esck={} > {ESCK_MAX}: the escape field is \
                 t = 2z + 1 + esck with z <= 5, so t <= {} — E-R4 holds 3 bits of \
                 margin over the 16-bit window's own limit of esck <= 5",
                tables.esck[i],
                11 + tables.esck[i] as u16
            ));
        }
        c.max_esck = c.max_esck.max(tables.esck[i]);
    }

    Ok(c)
}

/// Assert E-R4 over a blob's tables, and assert that the raw-field half of the
/// check had a subject: a blob with **no PAIR class at all** would satisfy
/// `rbits <= 5` without the bound being tested, which is how this file's
/// predecessor stayed green for no reason.
fn assert_premise(label: &str, blob: &[u8]) {
    let album = Album::parse(blob).unwrap_or_else(|e| panic!("{label}: album: {e:?}"));
    let mut tables = CodeTables::new();
    tables
        .parse_into(album.tables())
        .unwrap_or_else(|e| panic!("{label}: tables: {e:?}"));

    let c = premise_check(&tables).unwrap_or_else(|why| panic!("{label}: {why}"));
    assert!(
        c.pair >= 1,
        "{label}: the tables hold no PAIR class, so the rbits bound checked nothing — \
         census {c:?}"
    );

    eprintln!(
        "{label}: k={} pair={} quad={} zero={} max rbits(PAIR)={}/{} max esck={}/{}",
        tables.k, c.pair, c.quad, c.zero, c.max_pair_rbits, PAIR_RBITS_MAX, c.max_esck, ESCK_MAX
    );
}

/// A one-class table, for the controls.
fn one_class(ctype: u8, rbits: u8, esck: u8) -> CodeTables {
    let mut t = CodeTables::new();
    t.k = 1;
    t.ctype[0] = ctype;
    t.rbits[0] = rbits;
    t.esck[0] = esck;
    t
}

#[test]
fn the_committed_fixture_satisfies_the_closed_world_premise() {
    assert_premise("uli2_tiny", &FIXTURE_STORAGE.0);
}

#[test]
fn the_generated_corpora_satisfy_the_closed_world_premise_when_present() {
    let root = repo_root();
    let mut checked = 0;
    for path in GENERATED {
        if let Ok(blob) = std::fs::read(root.join(path)) {
            assert_premise(path, &blob);
            checked += 1;
        }
    }

    // Absence is legitimate (the corpora are build output). Silence about an
    // *existing* corpus is not: if there are .uli files under the assets dir and
    // this test read none of them, the paths are wrong, and that is the same
    // class of failure as the vacuous `rbits` bound this file was corrected for.
    if checked == 0 {
        let assets = root.join("examples/uli_bench/assets");
        let on_disk = std::fs::read_dir(&assets)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("uli"))
                    .count()
            })
            .unwrap_or(0);
        assert!(
            on_disk == 0,
            "premise: {on_disk} corpora exist under {} but this test read none: its \
             relative paths do not resolve (looked for {GENERATED:?})",
            assets.display()
        );
        eprintln!("premise: no generated corpora present; fixture only");
    } else {
        eprintln!("premise: {checked} generated corpora checked");
    }
}

/// The class ids the premise is stated in terms of, pinned. A renumbering would
/// silently repoint every `ctype == CT_PAIR` filter in the tree; here it is a
/// red test.
#[test]
fn the_class_ids_are_the_ones_the_bound_is_stated_against() {
    assert_eq!(
        (CT_ZERO, CT_QUAD, CT_PAIR),
        (0, 1, 2),
        "the premise's rbits bound applies to PAIR classes; if these ids moved, \
         the bound is being applied to something else"
    );
}

/// **The negative control that makes this file's green mean something.** Built
/// from the real `CT_PAIR`, so the shape of the 2026-11-19 bug — a bound written
/// for PAIR, evaluated on QUAD — cannot recur quietly: had `CT_PAIR` still been
/// the shadowed `1`, this table would read as a QUAD class, the raw field would
/// be declared unread, and the checker would pass it.
#[test]
fn the_checker_names_a_pair_class_that_breaks_the_bound() {
    let over = one_class(CT_PAIR, PAIR_RBITS_MAX + 1, 0);
    let why = premise_check(&over).expect_err("rbits 6 on a PAIR class must be rejected");
    assert!(
        why.contains("class 0 is PAIR with rbits=6"),
        "rejection did not name the class, its type or its rbits: {why}"
    );

    // And the same table one bit past the window's own limit, which is where the
    // product reader genuinely breaks rather than merely losing margin.
    assert!(premise_check(&one_class(CT_PAIR, 16, 0)).is_err());

    // A QUAD class carries no raw field at all, so rbits is not a violation
    // there — asserted so nobody "fixes" the bound by applying it to every class
    // and re-creates the coverage illusion from the other direction.
    assert!(
        premise_check(&one_class(CT_QUAD, 99, 0)).is_ok(),
        "rbits is never read on QUAD; rejecting it here would misstate the contract"
    );

    // esck is bound on every class, ZERO included.
    assert!(premise_check(&one_class(CT_ZERO, 0, ESCK_MAX)).is_ok());
    let esc = premise_check(&one_class(CT_QUAD, 0, ESCK_MAX + 1))
        .expect_err("esck 3 must be rejected under E-R4");
    assert!(esc.contains("esck=3"), "{esc}");

    // An unknown ctype is an error, not a skip.
    assert!(premise_check(&one_class(3, 0, 0)).is_err());
}
