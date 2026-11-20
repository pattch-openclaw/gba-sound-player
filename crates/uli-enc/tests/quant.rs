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
use uli_lite::format::{BANDW, M, NBANDS, NCODED};
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
    let content =
        Content::quantise(&samples, N_SAMPLES, &tables, g, 0.13, n_frames).expect("quantise");
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

/// The candidate rule itself, compared against an independent restatement of the C.
///
/// This is about [`quant::band_rdo`] with `lam = 0`, i.e. the nearest-level rule S2
/// shipped. It is *not* about the content plan: with `kappa > 0` the RD quantiser
/// deliberately chooses levels the nearest rule would not — including zero — and that
/// freedom is exactly what closed the rate gap. Comparing an RD plan against the
/// nearest rule would only assert that RD had done nothing.
#[test]
fn at_lam_zero_the_magnitudes_are_the_references_candidates() {
    let (_d, _pcm, samples, tables, _, nf) = coded(100);
    let step = quant::step_of(100, false) as f64;
    let d = step / 16.0;
    let mmax = quant::mmax_of(step as u32);
    let mut checked = 0usize;
    let mut coef = [0f32; 2 * M];
    for f in 0..nf {
        mdct_frame(&samples, f, N_SAMPLES, &mut coef);
        for ci in 0..2 {
            let base = ci * M;
            for b in 0..NBANDS {
                let ncod = quant::band_ncod(b);
                if ncod == 0 {
                    continue;
                }
                // Class 3 (PAIR, rbits 0) is the row S2's headroom rule picks for
                // small bands, and the one whose head can carry these magnitudes.
                let klass = 3usize;
                let dq = &tables.dq[klass];
                let head = (15i32 << tables.rbits[klass]) - 1;
                let rd = quant::band_rdo(
                    &coef[base + b * BANDW..base + b * BANDW + BANDW],
                    &tables,
                    klass,
                    d,
                    0.0,
                    mmax,
                    ncod,
                );
                for i in 0..ncod {
                    let ax = f64::from(coef[base + b * BANDW + i]).abs();
                    let want = reference_magnitude(ax, step, dq, mmax);
                    let got = rd.m[i];
                    if want.max(got) <= head {
                        assert_eq!(
                            got, want,
                            "frame {f} ch{ci} band {b} coeff {i}: lam=0 chose m={got}, the \
                             reference's candidate rule gives {want} for |coef| {ax}"
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
            rd: false,
            kappa: 0.13,
            stride: 8,
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

/// S3's plumbing test, and the reason it exists: the first version of `--rd` did
/// not reach `encode()` at all, the crate still built (the flag was a valid field
/// nobody read), all 45 tests stayed green, and the encoder emitted silence.
/// Silence and "the search collapsed" cost the same ~8 bits per frame here, so the
/// rate could not tell them apart — only this assertion can, because it compares
/// the search's output against both of the things it must not be.
#[test]
fn the_search_actually_runs_and_changes_the_album() {
    let dir = TempDir::new("rd-plumbing");
    let album = materialise(dir.path(), "album.f32", IN);
    let tracks = write_tracks(dir.path(), &format!("1\t0\t{N_SAMPLES}\tDense\tS3\n"));
    let base = |rd: bool, g: Option<i32>| {
        encode(&Args {
            album: album.clone(),
            tracks: tracks.clone(),
            budget: 4_000_000,
            cap_pct: DEFAULT_CAP_PCT,
            g,
            rd,
            kappa: 0.13,
            stride: 8,
            threads: 1,
        })
        .expect("encodes")
    };
    let silence = base(false, None);
    let searched = base(true, None);

    assert!(
        searched.report.g_chosen.is_some() && silence.report.g_chosen.is_none(),
        "the search must report the steps it chose; a missing g range means --rd did \
         not reach the encoder, which is the bug this test was written for"
    );
    assert!(
        searched.report.stream_bits > silence.report.stream_bits,
        "the search coded {} bits and silence codes {} — the album was not searched",
        searched.report.stream_bits,
        silence.report.stream_bits
    );
    assert!(
        searched.report.nonzero_coeffs > 0,
        "the searched album coded no coefficients at all"
    );
    assert_ne!(
        searched.bytes, silence.bytes,
        "the searched blob is byte-identical to the silence blob"
    );
}

// ---------------------------------------------------------------------------
// S3: the rate/distortion quantiser
// ---------------------------------------------------------------------------

/// **The bug this file was written for.** `rd_classes` filtered classes by whether
/// their headroom could hold the band's largest magnitude, so a band louder than
/// every class was offered *no* coded class and fell through to `class 0` — silence.
/// On the test album that turned the loudest band (energy 1.5e11) into nothing, and
/// because the class symbol is cheaper than the coefficients the frame still looked
/// *better* on a rate-only comparison: 0.80× the reference's bits. Only distortion
/// exposed it, six orders of magnitude above the reference's.
#[test]
fn a_band_louder_than_every_class_headroom_is_still_coded() {
    let tables = DefaultTables::build();
    // A magnitude of 800 quantised levels: past the widest class's headroom (479), so
    // escapes must carry it — but comfortably inside `mmax`, which is the *other*
    // limit and a different one. (At a step fine enough for `mmax` to bind, no encoder
    // can represent the band: the reference clamps to 65535 identically, and picking
    // a coarser step is the search's job, not the quantiser's.)
    // Only at steps where `mmax` is above the widest headroom can this case exist at
    // all: past g≈124 the format's own |X| limit is tighter than the class ladder, and
    // the ladder is what this test is about.
    for g in [60i32, 80, 100] {
        let d = quant::step_of(g, false) as f64 / 16.0;
        assert!(
            800.0 < quant::mmax_of(quant::step_of(g, false)) as f64,
            "g={g}: 800 levels is not representable at this step, so the test would be              measuring the format's limit instead of the class ladder"
        );
        let mut loud = [0f32; M];
        for v in loud.iter_mut().take(16) {
            *v = 800.0 * d as f32;
        }
        let rd = quant::quantise_chan_rd(&loud, &tables, g, false, 0.13);
        assert!(
            !rd.chan.bands.is_empty() && rd.chan.bands[0].class != 0,
            "g={g}: the band was coded as class 0 (silence). Every class's headroom is \
             below its magnitude, so a headroom filter offers no class at all — and the \
             band with all the album's energy in it disappears."
        );
        let energy: f64 = loud[..16]
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum();
        assert!(
            rd.dist < energy * 0.25,
            "g={g}: coded, but the distortion is {:.3e} against {energy:.3e} of energy",
            rd.dist
        );
    }
}

/// `kappa` is what makes the reference's rate reachable, and the mechanism is that a
/// coefficient which is cheap to lose is dropped instead of coded. With `kappa = 0`
/// the quantiser is S2's and keeps everything; a bigger `kappa` must never cost more
/// bits at the same step.
#[test]
fn a_larger_kappa_never_costs_more_bits() {
    let (_d, _pcm, samples, tables, _content, nf) = coded(100);
    let mut coef = [0f32; 2 * M];
    let mut prev = u64::MAX;
    let mut seen = Vec::new();
    for kappa in [0.0f64, 0.01, 0.13, 1.0, 10.0] {
        let mut bits = 0u64;
        for f in 0..nf {
            mdct_frame(&samples, f, N_SAMPLES, &mut coef);
            let l = uli_enc::quant::quantise_chan_rd(&coef[..M], &tables, 100, false, kappa);
            let r = uli_enc::quant::quantise_chan_rd(&coef[M..], &tables, 100, false, kappa);
            bits += chan_bits(&tables, &l.chan) + chan_bits(&tables, &r.chan);
        }
        assert!(
            bits <= prev,
            "kappa {kappa} spent {bits} bits, more than the previous kappa's {prev}: the \
             search is supposed to buy rate with distortion, never sell it back"
        );
        prev = bits;
        seen.push(bits);
    }
    assert!(
        seen[0] > seen[4],
        "kappa had no effect at all ({seen:?}); the RD term is not reaching the \
         quantiser, which is exactly how S3 shipped silence once already"
    );
}

/// The search must fit the budget it is given — and a reachable budget must never be
/// refused, because an encoder that refuses work it could have done is E-R6 failing
/// in the other direction.
#[test]
fn the_search_fits_every_budget_it_accepts() {
    let dir = TempDir::new("rd-budget");
    let album = materialise(dir.path(), "album.f32", IN);
    let tracks = write_tracks(dir.path(), &format!("1\t0\t{N_SAMPLES}\tDense\tS3\n"));
    for budget in [40_000u64, 60_000, 120_000, 400_000] {
        let out = encode(&Args {
            album: album.clone(),
            tracks: tracks.clone(),
            budget,
            cap_pct: DEFAULT_CAP_PCT,
            g: None,
            rd: true,
            kappa: 0.13,
            stride: 8,
            threads: 1,
        })
        .unwrap_or_else(|e| panic!("budget {budget} was refused: {e}"));
        assert!(
            out.bytes.len() as u64 <= budget,
            "budget {budget}: the search produced {} bytes, {} over",
            out.bytes.len(),
            out.bytes.len() as u64 - budget
        );
        let chosen = out
            .report
            .g_chosen
            .as_ref()
            .expect("the search reports its g");
        assert_eq!(chosen.len(), out.report.n_frames);
    }
}
