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
//! 1. **`rbits == 0` on every PAIR class.** A raw pair digit reads `r` bits and
//!    then a sign bit, which is what forced the old 17-bit contract. With `r = 0`
//!    a group needs at most 4 bits (a quad's four sign bits) and 16 is ample.
//! 2. **`esck <= 2` on every class.** The escape fast path reads
//!    `t = 2z + 1 + k` bits with `z <= 5`, so `t <= 13`; that is what makes the
//!    product's `z < 6`-only guard safe, and it is why `t < 32` / `t <= valid`
//!    can be dropped.
//!
//! Both are recorded measurements across five corpora (fixture, album, two-track,
//! adversarial, and the 167,222-frame cross album). This test keeps the committed
//! fixture honest and checks the generated corpora when they are present, since
//! they are gitignored build output.

use uli_lite::{Album, CodeTables};

static FIXTURE_STORAGE: uli_lite::Aligned<
    { include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli").len() },
> = uli_lite::Aligned(*include_bytes!(
    "../../uli-lite/tests/fixtures/uli2_tiny.uli"
));

/// The corpora the bake step produces. Gitignored, so absence is not a failure.
const GENERATED: &[&str] = &[
    "../../../examples/uli_bench/assets/biomaster.uli",
    "../../../examples/uli_bench/assets/biotwo.uli",
    "../../../examples/uli_bench/assets/adv.uli",
    "../../../examples/uli_bench/assets/cross/cross.uli",
];

/// `CT_PAIR` in `uli_lite::format`, named here so the assertion reads as prose.
const CT_PAIR: u8 = 1;

fn assert_premise(label: &str, blob: &[u8]) {
    let album = Album::parse(blob).unwrap_or_else(|e| panic!("{label}: album: {e:?}"));
    let mut tables = CodeTables::new();
    tables
        .parse_into(album.tables())
        .unwrap_or_else(|e| panic!("{label}: tables: {e:?}"));

    let k = tables.k;
    assert!(k <= uli_lite::format::MAXK, "{label}: k={k} exceeds MAXK");

    for c in 0..k {
        if tables.ctype[c] == CT_PAIR {
            assert_eq!(
                tables.rbits[c], 0,
                "{label}: class {c} is PAIR with rbits={}, so a raw digit reads \
                 raw+sign bits and the product's 16-bit refill contract is unsound",
                tables.rbits[c]
            );
        }
        assert!(
            tables.esck[c] <= 2,
            "{label}: class {c} has esck={} > 2, so the escape field can be \
             longer than the 16-bit window the product refills to",
            tables.esck[c]
        );
    }
}

#[test]
fn the_committed_fixture_satisfies_the_closed_world_premise() {
    assert_premise("uli2_tiny", &FIXTURE_STORAGE.0);
}

#[test]
fn the_generated_corpora_satisfy_the_closed_world_premise_when_present() {
    let mut checked = 0;
    for path in GENERATED {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        if let Ok(blob) = std::fs::read(&full) {
            assert_premise(path, &blob);
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!("premise: no generated corpora present; fixture only");
    } else {
        eprintln!("premise: {checked} generated corpora checked");
    }
}
