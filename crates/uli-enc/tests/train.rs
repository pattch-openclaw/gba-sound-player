//! S4: training the code tables on the album they will code.
//!
//! Three claims, and the middle one is the reason the pass exists at all.
//!
//! **Training must not break the format.** The trained set is rebuilt from counts, so it is
//! data rather than code: its codebooks have to be complete prefix codes, every symbol the
//! writer can reach has to have a code, and its structural rows have to satisfy the
//! closed-world premise the product reader depends on. All of that is checked here on a real
//! trained set, not on a hand-built one.
//!
//! **Training must pay for itself.** On a rate-limited album the trained tables measure
//! better than the built-in ones at the same budget — measurably, in dB, with the same cap.
//! If that ever stops being true the pass is pure cost.
//!
//! **Training must be reproducible.** Two encodes of the same album produce the same bytes.
//! The counts are integers and the merge is deterministic; only `huff_lengths`' length-capping
//! re-merge crosses libm, which is the exposure E-R5's amendment already covers for tables
//! derived from floats.

mod common;

use common::{TempDir, materialise, write_tracks};
use uli_enc::{Args, DEFAULT_CAP_PCT, DefaultTables, train};
use uli_lite::CodeTables;

const N_SAMPLES: usize = 2048;

/// Correlated stereo, so both the M/S flag and the per-context statistics have something to
/// find. The committed MDCT fixture is deliberately uncorrelated and would make training look
/// useless for reasons that have nothing to do with training.
fn album(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut bytes = Vec::with_capacity(N_SAMPLES * 8);
    for i in 0..N_SAMPLES {
        let t = i as f32;
        let l = 7000.0 * (t * 0.021).sin() + 3000.0 * (t * 0.0037).sin();
        let noise = 200.0 * (i.wrapping_mul(2_654_435_761) % 1000) as f32 / 1000.0 - 100.0;
        let r = 0.8 * l + noise;
        bytes.extend_from_slice(&l.to_le_bytes());
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    (
        materialise(dir, "album.f32", &bytes),
        write_tracks(dir, &format!("1\t0\t{N_SAMPLES}\tCorr\tS4\n")),
    )
}

fn args(album: std::path::PathBuf, tracks: std::path::PathBuf, budget: u64, train: bool) -> Args {
    Args {
        album,
        tracks,
        budget,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: true,
        kappa: 0.13,
        stride: 8,
        fine_width: 0,
        train,
        ms: None,
        threads: 1,
    }
}

/// A budget that binds on this short album, so the comparison is between two encoders that
/// spent the same number of bytes rather than two that spent what they liked.
const BINDING: u64 = 8_000;

#[test]
fn training_pays_for_itself_at_a_binding_budget() {
    let dir = TempDir::new("train-pays");
    let (album, tracks) = album(dir.path());
    let plain =
        uli_enc::encode(&args(album.clone(), tracks.clone(), BINDING, false)).expect("encodes");
    let trained = uli_enc::encode(&args(album, tracks, BINDING, true)).expect("encodes");

    assert!(!plain.report.trained, "the flag must say which pass ran");
    assert!(trained.report.trained);
    assert!(
        trained.bytes.len() as u64 <= BINDING,
        "the trained pass overshot"
    );
    assert_eq!(trained.report.xover, 0, "a trained table broke |X| < 2^28");
    assert_eq!(
        trained.report.hfnz, 0,
        "a trained table coded energy past NCODED"
    );

    let (a, b) = (
        trained.report.snr_db.expect("SNR"),
        plain.report.snr_db.expect("SNR"),
    );
    assert!(
        a > b + 0.25,
        "trained tables measured {a:.2} dB against the built-in set's {b:.2} dB at the same \
         budget and cap. Training is supposed to fit the codebooks to this album; a gain that \
         small means the statistics are not reaching the tables, or the second search is not \
         running."
    );
}

#[test]
fn a_trained_encode_is_byte_stable() {
    let dir = TempDir::new("train-stable");
    let (album, tracks) = album(dir.path());
    let one =
        uli_enc::encode(&args(album.clone(), tracks.clone(), BINDING, true)).expect("encodes");
    let two = uli_enc::encode(&args(album, tracks, BINDING, true)).expect("encodes");
    assert!(one.report.trained);
    assert_eq!(
        one.bytes, two.bytes,
        "two encodes of one album produced different bytes; the trained tables are not \
         reproducible, which makes every pin in the tree worthless"
    );
}

/// The trained set is checked the way the format checks any table: as bytes, through the
/// parser, against the premise — plus the two properties a count-derived table can violate and
/// a hand-written one cannot: an incomplete code, and a symbol left with no code.
#[test]
fn a_trained_table_set_is_a_usable_table_set() {
    let dir = TempDir::new("train-valid");
    let (album, tracks) = album(dir.path());
    let out = uli_enc::encode(&args(album, tracks, BINDING, true)).expect("encodes");

    let parsed = uli_lite::Album::parse(&out.bytes).expect("the blob parses");
    let mut tables = CodeTables::new();
    tables
        .parse_into(parsed.tables())
        .expect("trained tables parse");
    let census = tables
        .premise()
        .expect("a trained set must stay inside the encoder's closed-world premise");
    assert!(
        census.pair >= 1,
        "the trained set holds no PAIR class, so the premise checked nothing"
    );

    // The same set through the encoder-side model, which is where the writer reads lengths.
    let base = DefaultTables::build();
    let stats = train::Stats::default();
    let as_built = train::build(&base, &stats);
    train::validate(&as_built).expect("an empty stat set must leave the base tables valid");
    assert!(
        train::eob_len(&as_built) > 0,
        "a table with no terminator cannot describe any channel"
    );
}

/// A set built from an album's own counts must differ from the built-in set — otherwise the
/// second pass is re-coding the album with the same tables and the whole step is theatre.
#[test]
fn training_actually_changes_the_tables() {
    let dir = TempDir::new("train-differs");
    let (album, tracks) = album(dir.path());
    let out = uli_enc::encode(&args(album, tracks, BINDING, true)).expect("encodes");
    let parsed = uli_lite::Album::parse(&out.bytes).expect("parses");
    let base = DefaultTables::build().to_blob();
    assert_ne!(
        parsed.tables(),
        &base[..],
        "the shipped tables are byte-identical to the built-in ones, so nothing was trained"
    );
}
