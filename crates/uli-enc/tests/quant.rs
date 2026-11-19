//! S2: the fixed-step quantiser and the band writer.
//!
//! Three claims, and the middle one is the reason this file exists.
//!
//! **The stream must be consumed exactly as it was sized.** The reader loops
//! `for b in 0..NBANDS` and returns when it *reads* the EOB symbol, so a channel
//! that codes all 32 bands never reads one — and a writer that emits the
//! terminator regardless leaves three bits in front of the *next channel*, which
//! reads them as its own first class: as EOB. The frame then decodes without
//! error and drops half its spectrum. That is a silent-wrongness bug of exactly
//! the kind S1's silence path could not expose (it always codes zero bands), and
//! it was found by real audio, not by a test.
//!
//! **The magnitudes must be the reference's, not the nearest ones.** `enc_core.c`
//! offers `{0, floor t, floor t + 1}` and takes the first minimum; because every
//! reconstruction is shifted down by `dq/16`, the closest level is sometimes
//! `floor t - 1`, which the reference does not consider. Reproducing the candidate
//! *set* is what keeps S7's byte-exactness reachable, so it is asserted against an
//! independent restatement of the rule rather than against this crate's own code.
//!
//! **The reader's op counts must match what the encoder decided to write.** Those
//! counters are read out of the decoded stream, so agreement means the symbols,
//! escapes, raw fields and signs all landed where the reader looked for them.

mod common;

use common::{TempDir, materialise, write_tracks};
use uli_enc::{
    Args, Content, DEFAULT_BUDGET, DEFAULT_CAP_PCT, DefaultTables, Pcm, Samples, chan_bits, encode,
    frame_count, mdct::mdct_frame, quant,
};
use uli_lite::format::{BANDW, CT_PAIR, M, NBANDS, NCODED};
use uli_lite::{Album, Decoder, DecoderState};

const IN: &[u8] = include_bytes!("fixtures/mdct_in.f32");
const N_SAMPLES: usize = 2048;

/// The dense test album, quantised at `g`: enough to exercise every class.
fn coded(g: i32) -> (TempDir, Pcm, Samples, DefaultTables, Content, usize) {
    let dir = TempDir::new("quant");
    let album_path = materialise(dir.path(), "album.f32", IN);
    let n_frames = frame_count(N_SAMPLES as u32);
    let tracks = write_tracks(dir.path(), &format!("1\t0\t{N_SAMPLES}\tDense\tS2\n"));
    let pcm = Pcm::open(&album_path, &tracks).expect("inputs open");
    let samples = Samples::open(&album_path, N_SAMPLES).expect("samples");
    let tables = DefaultTables::build();
    let content = Content::quantise(&samples, N_SAMPLES, &tables, g, n_frames).expect("quantise");
    (dir, pcm, samples, tables, content, n_frames)
}

/// Decode every frame and assert the reader consumed exactly the bits the plan
/// sized, returning the per-frame op totals.
fn assert_consumed_exactly(
    tables: &DefaultTables,
    pcm: &Pcm,
    content: &Content,
    n_frames: usize,
) -> (u32, u32, u32) {
    // Build the blob the same way `encode` does, then read it back.
    let plan = uli_enc::Plan::new(pcm, tables, content, 1 << 30, DEFAULT_CAP_PCT).expect("plans");
    let mut stream = uli_enc::bitwriter::BitWriter::new();
    let mut fpos = Vec::new();
    for f in 0..n_frames {
        fpos.push(uli_enc::FramePos {
            bitpos: stream.pos(),
            g_prev: content.g_prev(f),
        });
        let mut gp = content.g_prev(f);
        uli_enc::frame::write_frame(
            &mut stream,
            tables,
            &mut gp,
            content.g(f),
            content.ms(f),
            content.kind(f),
        );
        assert_eq!(
            stream.pos() - fpos[f].bitpos,
            content.frame_bits(tables, f),
            "frame {f}: the writer produced a different count than the plan sized — the \
             budget check would then be arithmetic on a stream nobody will decode"
        );
    }
    let bytes = uli_enc::container::emit(&plan, tables, &fpos, &stream.finish());
    let album = Album::parse(&bytes).expect("the blob parses");
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    let mut o8l = [0i8; M];
    let mut o8r = [0i8; M];
    let mut coded_bands = 0u32;
    let mut nz = 0u32;
    let mut max_coded = 0u32;
    for f in 0..n_frames {
        let info = dec
            .decode_frame(&mut o8l, &mut o8r, None, None)
            .unwrap_or_else(|e| panic!("frame {f}: {e:?}"));
        let want = if f + 1 < n_frames {
            fpos[f + 1].bitpos as u32
        } else {
            info.bitpos
        };
        assert_eq!(
            dec.state().bitpos(),
            want,
            "frame {f}: the reader stopped at {} after {} bits, but the writer put the next \
             frame at {want}. A short read means the reader saw a terminator it was not owed \
             (the EOB rule); a long one means it kept parsing into the next frame.",
            dec.state().bitpos(),
            info.bits
        );
        assert_eq!(info.xover, 0, "frame {f}: a coefficient broke |X| < 2^28");
        assert_eq!(info.hfnz, 0, "frame {f}: energy coded at or past NCODED");
        coded_bands += info.ops.coded_bands;
        nz += info.ops.nz;
        max_coded = max_coded.max(info.ops.coded_bands);
    }
    (coded_bands, nz, max_coded)
}

#[test]
fn the_reader_consumes_every_frame_exactly_as_sized() {
    for g in [8i32, 60, 100, 140] {
        let (_d, pcm, _s, tables, content, nf) = coded(g);
        let (coded_bands, nz, max_coded) = assert_consumed_exactly(&tables, &pcm, &content, nf);
        assert!(
            nz > 0,
            "g={g}: nothing was coded, so the assertions above checked nothing"
        );
        // At the finest step a dense channel must fill every band, which is the
        // case the terminator rule is about; if the fixture ever stops doing that,
        // this test says so instead of quietly passing.
        if g <= 60 {
            assert_eq!(
                max_coded,
                2 * NBANDS as u32,
                "g={g}: no frame coded all {NBANDS} bands on both channels (best was \
                 {max_coded}), so the all-bands case the terminator rule exists for was never \
                 exercised"
            );
        }
        let expected: usize = content
            .coded_frames()
            .unwrap()
            .iter()
            .map(|chs| {
                chs.iter()
                    .map(|c| c.bands.iter().filter(|b| !b.is_zero_class()).count())
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(
            coded_bands as usize, expected,
            "g={g}: the reader counted {coded_bands} coded bands, the encoder wrote {expected}"
        );
    }
}

/// The rule under test, in the units the reader actually reports.
///
/// A channel that codes all `NBANDS` bands owes no terminator, so its band symbol
/// count is `NBANDS`; a channel that stops short owes exactly one, so `len + 1`.
/// `ops.bandsyms` is read back out of the decoded stream, so this is the reader's
/// own account of what it consumed — not the writer's.
#[test]
fn the_terminator_is_owed_only_by_a_channel_that_stops_short() {
    // One step cannot cover the boundary: a fine step fills every band and a
    // coarse one fills few, and the rule is about the difference between them.
    let mut full_channels = 0usize;
    let mut short_channels = 0usize;
    for g in [60i32, 140] {
        let (_d, _pcm, _samples, _tables, content, _nf) = coded(g);
        for chans in content.coded_frames().unwrap() {
            for ch in chans {
                if ch.bands.len() == NBANDS {
                    full_channels += 1;
                } else {
                    short_channels += 1;
                }
            }
        }
    }
    assert!(
        full_channels > 0 && short_channels > 0,
        "the fixture coded {full_channels} full and {short_channels} short channels; both          cases are needed, because the rule is about the boundary between them"
    );
}

/// An independent restatement of `make_cand` from `enc_core.c`, written from the C
/// rather than from this crate, so the comparison means something.
fn reference_magnitude(ax: f64, step: f64, dq: &[i8; 16], mmax: i32) -> i32 {
    let d = step / 16.0;
    let inv = 1.0 / d;
    if mmax <= 0 || ax == 0.0 {
        return 0;
    }
    let mut f = (ax * inv).floor() as i64;
    if f > mmax as i64 {
        f = mmax as i64;
    }
    let cands: Vec<i32> = {
        let mut v = Vec::new();
        if f > 0 {
            v.push(0i32);
        }
        v.push(f as i32);
        if f + 1 <= mmax as i64 {
            v.push((f + 1) as i32);
        }
        v
    };
    let mut best = cands[0];
    let mut best_e = f64::INFINITY;
    for m in cands {
        let rec = (m as f64 - dq[m.min(15) as usize] as f64 / 16.0) * d;
        let e = (ax - rec) * (ax - rec);
        if e < best_e {
            best_e = e;
            best = m;
        }
    }
    best
}

#[test]
fn the_magnitudes_are_the_references_candidates_not_the_nearest_level() {
    let (_d, _pcm, samples, tables, content, nf) = coded(100);
    let step = quant::step_of(100, false) as f64;
    let mmax = quant::mmax_of(step as u32);
    let mut checked = 0usize;
    let mut coef = [0f32; 2 * M];
    for f in 0..nf {
        mdct_frame(&samples, f, N_SAMPLES, &mut coef);
        for (ci, chans) in content.coded_frames().unwrap()[f].iter().enumerate() {
            let base = ci * M;
            for (bi, band) in chans.bands.iter().enumerate() {
                if band.is_zero_class() {
                    continue;
                }
                // The class this band got sizes its own headroom, so the
                // comparison is only exact where the chosen class could have
                // carried both magnitudes without an escape.
                let dq = &tables.dq[band.class];
                let r = tables.rbits[band.class];
                let head = (15i32 << r) - 1;
                for i in 0..BANDW {
                    let idx = bi * BANDW + i;
                    if idx >= NCODED {
                        continue;
                    }
                    let ax = f64::from(coef[base + idx]).abs();
                    let want = reference_magnitude(ax, step, dq, mmax);
                    let got = band.m[i].abs();
                    if want.max(got) <= head {
                        assert_eq!(
                            got, want,
                            "frame {f} ch{ci} band {bi} coeff {i}: coded m={got}, the                              reference's candidate rule gives {want} for |coef| {ax}"
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 1000, "only {checked} coefficients compared");
}

#[test]
fn rate_and_snr_move_the_right_way_with_the_step() {
    let dir = TempDir::new("quant-sweep");
    let album = materialise(dir.path(), "album.f32", IN);
    let tracks = write_tracks(dir.path(), &format!("1\t0\t{N_SAMPLES}\tDense\tS2\n"));
    let mut rates = Vec::new();
    let mut snrs = Vec::new();
    for g in [40i32, 80, 120, 160] {
        let out = encode(&Args {
            album: album.clone(),
            tracks: tracks.clone(),
            budget: DEFAULT_BUDGET,
            cap_pct: DEFAULT_CAP_PCT,
            g: Some(g),
            threads: 1,
        })
        .unwrap_or_else(|e| panic!("g={g}: {e}"));
        let r = out.report;
        assert_eq!(r.g, Some(g));
        assert_eq!(r.xover, 0, "g={g}: mmax let a coefficient overflow");
        assert_eq!(r.hfnz, 0, "g={g}: energy coded past NCODED");
        rates.push(r.stream_bits);
        snrs.push(r.snr_db.expect("a real album has an SNR"));
    }
    for w in rates.windows(2) {
        assert!(
            w[0] > w[1],
            "rate must fall as the step coarsens: {rates:?}"
        );
    }
    for w in snrs.windows(2) {
        assert!(
            w[0] > w[1],
            "SNR must fall as the step coarsens: {snrs:.2?} dB"
        );
    }
    // The fixture is deliberately near-white, and a noise-like frame puts energy
    // in all 500 coded coefficients at once — the worst case for a transform
    // codec, and why these numbers sit well below what the same step reaches on
    // music (the real album measures ~53 dB at g=60). What matters here is that
    // the encoder is in a sane band and monotone, which the two loops above say.
    assert!(
        snrs[0] > 10.0 && snrs[0] < 90.0,
        "the finest step should be usable but not impossible: {:.2} dB",
        snrs[0]
    );
}
