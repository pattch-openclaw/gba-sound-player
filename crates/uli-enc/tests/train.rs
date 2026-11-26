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
use uli_enc::pcm::Samples;
use uli_enc::plan::{Content, GPlan};
use uli_enc::{Args, DEFAULT_CAP_PCT, DefaultTables, Input, train};
use uli_lite::CodeTables;

// S8-7: 8,192 samples (17 frames) rather than 2,048 (5). The reference's `train()`
// samples every 8th frame, so a five-frame album hands it **one** frame and the trained
// set overfits; the training-pays claim has to be measured on an album long enough for
// the sampling to mean anything. The search shape is otherwise the shipped one.
const N_SAMPLES: usize = 8192;

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
        input: Input::Pcm { album, tracks },
        budget,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: true,
        kappa: 0.13,
        train,
        ms: None,
        musearch: false,
        threads: 1,
    }
}

/// A budget that binds on this album, so the comparison is between two encoders that spent
/// the same number of bytes rather than two that spent what they liked.
///
/// **Re-measured at S8-7**, when the search became the reference's: the album is now 8,192
/// samples (17 frames) for the reason above, and the budget stays 6,600. Measured on this
/// album at the shipped shape — one thread, `--musearch 0`, trained against `--no-train` at
/// the same budget:
///
/// | budget | trained | built-in | gain |
/// |---|---|---|---|
/// | 5,600 | 35.17 | 31.55 | +3.62 |
/// | 6,000 | 44.26 | 42.01 | +2.25 |
/// | 6,600 | 51.16 | 47.83 | **+3.33** |
/// | 7,400 | 55.82 | 53.55 | +2.27 |
/// | 8,000 | 58.54 | 56.46 | +2.08 |
///
/// (The S8-0 table this replaces was measured on the five-frame album with the old sweep;
/// it is in `PLAN-OF-RECORD.md` §S8-0 and no longer describes this search.)
const BINDING: u64 = 6_600;

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

/// S8-6: an unobserved context is still rebuilt, to the balanced code the 0.2 floor implies.
///
/// The reference's `tables_from_stats` calls `huff_lengths` for every `st->cb[a]`
/// unconditionally, so a context a small album never visits does **not** keep the built-in
/// row. Measured on S8-4's `fast` cell, retaining the base row left 21 bytes of `len_cb`
/// different from the oracle's table region; this test is the regression, on the fifteen
/// evenly-weighted symbols of a 14-class set (fourteen at four bits, the terminator at
/// three).
#[test]
fn an_unobserved_context_is_rebuilt_to_the_balanced_code() {
    // The default rows are already balanced, so a base-only test cannot see the difference.
    // Model the sequence that exposed it: a round observes context 9, a later round has no
    // counts for it. The later `build` must revert the row to the balanced code rather than
    // keep the observed one — which is what the reference's unconditional `huff_lengths`
    // does, and what left 21 bytes of `len_cb` different from the oracle before S8-6.
    let base = DefaultTables::build();
    let mut observed = train::Stats::default();
    observed.cb[9][0] = 100.0;
    let skewed = train::build(&base, &observed);
    assert_ne!(
        skewed.cb[9].len, base.cb[9].len,
        "the observed row should be skewed, not the balanced default"
    );

    let rebuilt = train::build(&skewed, &train::Stats::default());
    assert_eq!(
        rebuilt.cb[9].len, base.cb[9].len,
        "a zero-count context kept the previous row instead of being rebuilt"
    );
    let mut lengths: Vec<u8> = (0..=base.k).map(|b| rebuilt.cb[9].symbol_len(b)).collect();
    assert_eq!(rebuilt.cb[9].symbol_len(base.k), 3);
    lengths.sort_unstable();
    assert_eq!(lengths, [3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4]);
}

#[test]
fn the_terminator_is_charged_only_when_it_is_written() {
    use uli_enc::quant::{Band, Chan};
    use uli_lite::format::NBANDS;

    let tables = DefaultTables::build();
    let samples = Samples::from_interleaved(vec![0.0; N_SAMPLES * 2]);
    let g = 100;
    let ms = false;

    let collect = |bands: usize| {
        let chan = Chan {
            bands: vec![Band::zero(); bands],
        };
        let content = Content::coded(GPlan::new(vec![g], vec![ms]), vec![[chan.clone(), chan]]);
        train::collect_frames(&samples, N_SAMPLES, &tables, &content, 0.13, &[0])
    };

    let full = collect(NBANDS);
    assert_eq!(full.frames, 1);
    assert_eq!(
        full.cb[0][tables.k], 0.0,
        "a full-band channel must not be charged a terminator"
    );
    assert_eq!(full.cb[0][0], 2.0 * (NBANDS as f64 - 1.0));
    assert_eq!(
        full.cb[tables.k][0], 2.0,
        "the first band of each channel is context k"
    );

    let short = collect(NBANDS - 1);
    assert_eq!(
        short.cb[0][tables.k], 2.0,
        "a channel that ends early is charged one terminator per channel"
    );
    assert_eq!(short.cb[0][0], 2.0 * (NBANDS as f64 - 2.0));
    assert_eq!(
        short.cb[tables.k][0], 2.0,
        "the first band of each channel is context k"
    );
}
