//! The E-R4 premise is checked by the encoder, on the tables it is about to
//! write, before a byte of output exists.
//!
//! Two witnesses: the encoder's own default tables pass (and have a subject — 11
//! PAIR classes, so the `rbits` bound is not vacuous), and a table one bit past
//! `ENC_PAIR_RBITS_MAX` is refused. The refusal is checked with a budget that
//! would *also* be refused by the plan, so the error ordering is witnessed too:
//! the premise runs before the budget arithmetic.

mod common;

use common::{TempDir, silence_album, single_track};
use uli_enc::{
    Args, DEFAULT_BUDGET, DEFAULT_CAP_PCT, DefaultTables, Error, encode, encode_with_tables,
};
use uli_lite::format::ENC_PAIR_RBITS_MAX;
use uli_lite::{Album, CodeTables, PremiseViolationKind};

fn args(album: std::path::PathBuf, tracks: std::path::PathBuf, budget: u64) -> Args {
    Args {
        album,
        tracks,
        budget,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        ms: None,
        threads: 1,
    }
}

#[test]
fn the_encoders_own_tables_satisfy_the_premise() {
    let dir = TempDir::new("premise-own");
    let album_path = silence_album(dir.path(), 4 * 512);
    let tracks = single_track(dir.path(), 4 * 512);

    let out = encode(&args(album_path, tracks, DEFAULT_BUDGET)).expect("encodes");
    let album = Album::parse(&out.bytes).expect("the blob parses");
    let mut tables = CodeTables::new();
    tables.parse_into(album.tables()).expect("tables parse");

    let census = tables
        .premise()
        .expect("the encoder's own tables pass E-R4");
    assert!(
        census.pair >= 1,
        "the default row holds no PAIR class, so the rbits bound checked nothing"
    );
    assert_eq!(census.pair, 11, "the default row's PAIR classes");
    assert_eq!(census.quad, 2, "the default row's QUAD classes");
    assert_eq!(census.zero, 1, "the default row's ZERO class");
    assert_eq!(census.max_pair_rbits, ENC_PAIR_RBITS_MAX);
    assert_eq!(census.max_esck, 2);
}

#[test]
fn a_pair_class_over_the_raw_field_bound_is_refused_before_anything_is_written() {
    let dir = TempDir::new("premise-over");
    let album_path = silence_album(dir.path(), 4 * 512);
    let tracks = single_track(dir.path(), 4 * 512);

    let mut tables = DefaultTables::build();
    tables.rbits[3] = ENC_PAIR_RBITS_MAX + 1;

    // A budget of one byte cannot fit any album, so a plan error would be a
    // Refusal. The premise error must win: the tables are checked first.
    let err = encode_with_tables(&args(album_path, tracks, 1), &tables)
        .expect_err("rbits 6 on a PAIR class must be refused");

    match err {
        Error::Premise(v) => {
            assert_eq!(v.class, 3);
            assert_eq!(v.value, ENC_PAIR_RBITS_MAX + 1);
            assert_eq!(v.kind, PremiseViolationKind::PairRbits);
            let text = v.to_string();
            assert!(text.contains("class 3 is PAIR with rbits=6"), "{text}");
        }
        other => panic!("expected the premise violation, got {other:?}"),
    }
}
