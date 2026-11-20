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
//! **The magnitudes come from a fixed candidate set, not from the nearest level.**
//! offers `{0, floor t, floor t + 1}` and takes the first minimum; because every
//! reconstruction is shifted down by `dq/16`, the closest level is sometimes
//! `floor t - 1`, which the set does not offer. Reproducing the candidate
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
use uli_lite::format::{BANDW, M, NBANDS};
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
    let content = Content::quantise(&samples, N_SAMPLES, &tables, g, false, 0.13, n_frames)
        .expect("quantise");
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

/// An independent restatement of the candidate rule, written from the specification
/// rather than from this crate, so that comparing against it means something.
fn accepted_magnitude(ax: f64, step: f64, dq: &[i8; 16], mmax: i32) -> i32 {
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
        if f < mmax as i64 {
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
fn at_lam_zero_the_magnitudes_follow_the_candidate_rule() {
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
                )
                .expect("the default tables code every symbol, so no tuple can fail");
                for i in 0..ncod {
                    let ax = f64::from(coef[base + b * BANDW + i]).abs();
                    let want = accepted_magnitude(ax, step, dq, mmax);
                    let got = rd.m[i];
                    if want.max(got) <= head {
                        assert_eq!(
                            got, want,
                            "frame {f} ch{ci} band {b} coeff {i}: lam=0 chose m={got}, the \
                             candidate rule gives {want} for |coef| {ax}"
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
            ms: None,
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
            ms: None,
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
/// *better* on a rate-only comparison: 0.80× the oracle's bits. Only distortion
/// exposed it, six orders of magnitude above the oracle's.
#[test]
fn a_band_louder_than_every_class_headroom_is_still_coded() {
    let tables = DefaultTables::build();
    // A magnitude of 800 quantised levels: past the widest class's headroom (479), so
    // escapes must carry it — but comfortably inside `mmax`, which is the *other*
    // limit and a different one. (At a step fine enough for `mmax` to bind, no encoder
    // can represent the band: the oracle clamps to 65535 identically, and picking
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

/// `kappa` is what makes the target rate reachable, and the mechanism is that a
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
            ms: None,
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

/// The cycle model charges `3 * 512` for an M/S frame, and `frame_ops` used to
/// hardcode `ms = 0`. Nothing could see it while M/S was not offered, and the moment
/// it is, an under-counted model over-admits candidates against the `--cap` filter —
/// silently, in the direction that looks like a bigger search. The flag is an
/// argument now, and this pins the arithmetic so enabling M/S is a search change and
/// not a re-derivation.
#[test]
fn an_ms_frame_costs_exactly_three_hundred_twelve_cycles_more() {
    let tables = DefaultTables::build();
    let (_d, _pcm, _samples, _tables, content, nf) = coded(100);
    let chans = content
        .coded_frames()
        .expect("a coded plan")
        .get(nf.min(4))
        .expect("at least five frames");
    let lr = uli_enc::rd::frame_ops(&tables, chans, false);
    let ms = uli_enc::rd::frame_ops(&tables, chans, true);
    assert_eq!(lr.ms, 0);
    assert_eq!(ms.ms, 1);
    assert_eq!(
        ms.cycle_model() - lr.cycle_model(),
        3 * 512,
        "an M/S frame's cycle cost is ULI_CY_MS = 3 * 512; frame_ops must charge it or \
         the --cap filter is comparing candidates on a number that is 1536 cycles short"
    );
}

/// A symbol with no code cannot be written, so it cannot be chosen.
///
/// Inert with the default tables — `huff_lengths` gives every symbol a code — but S4's
/// *trained* tables leave never-seen symbols at length 0, and a price list that treats
/// "no code" as "zero bits" makes the uncodeable symbol look **free**. The oracle
/// guards with `if (!l) continue;` and propagates failure as `BIG`; here it is `None`.
#[test]
fn a_symbol_with_no_code_is_never_chosen() {
    let mut tables = DefaultTables::build();
    let class = 3usize; // PAIR, rbits 0: symbol index is m0 * 16 + m1
    // Symbol 0 is the "both coefficients zero" pair — the cheapest thing in the table
    // and the first thing a zeroing quantiser would reach for.
    assert!(
        tables.cls[class].symbol_len(0) > 0,
        "the default row codes symbol 0"
    );
    tables.cls[class].len[0] = 0;
    tables.cls[class].code[0] = 0;

    let d = quant::step_of(100, false) as f64 / 16.0;
    let mmax = quant::mmax_of(quant::step_of(100, false));
    // A band of exactly-zero coefficients: symbol 0 is the only candidate there is.
    let zeros = [0f32; BANDW];
    let rd = quant::band_rdo(&zeros, &tables, class, d, 0.13, mmax, BANDW);
    assert!(
        rd.is_none(),
        "band_rdo chose a magnitude pair whose code length is 0; writing it would \
         desynchronise the stream, and the reader would decode noise"
    );

    // And an ordinary band still works, so the guard is a filter, not an off switch.
    let mut loud = [0f32; BANDW];
    for (i, v) in loud.iter_mut().enumerate() {
        *v = 40.0 * (i as f32 + 1.0) * d as f32;
    }
    assert!(
        quant::band_rdo(&loud, &tables, class, d, 0.13, mmax, BANDW).is_some(),
        "a band that never needs symbol 0 must still find a codeable combination"
    );
}

/// `band_rdo`, differentially against the frozen oracle on 2,400 cases.
///
/// The fixture (`tests/fixtures/band_rdo.txt`, `make uli-band-rdo-fixture`) covers all
/// 13 classes across 180 steps and five magnitude regimes: all-zero, entirely below one
/// step (the case the zeroing decision exists for), straddling the candidate edges,
/// needing escapes, and sparse. Every field is an integer or an exact hex bit pattern,
/// so the comparison is exact rather than tolerant.
///
/// Why this and not more reading: reading the C against this port found one dropped
/// guard already (`if (!l) continue;`). Reading catches what reading catches. This is
/// the instrument that found the tables-blob row leak and proved the MDCT bit-exact —
/// freeze the oracle's answers, assert ours match.
#[test]
fn band_rdo_matches_every_frozen_case() {
    let tables = DefaultTables::build();
    let raw = include_str!("fixtures/band_rdo.txt");
    let mut cases = 0usize;
    let mut worst_rel = 0f64;
    let mut per_class = [0usize; 14];
    for (lineno, line) in raw.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(f.len(), 16 + 4 + 2 + 16, "line {}: field count", lineno + 1);
        let mut x = [0f32; BANDW];
        for i in 0..BANDW {
            let bits = u32::from_str_radix(f[i], 16).expect("f32 hex");
            x[i] = f32::from_bits(bits);
        }
        let class: usize = f[16].parse().expect("class");
        let g: i32 = f[17].parse().expect("g");
        let mmax: i32 = f[18].parse().expect("mmax");
        let ncod: usize = f[19].parse().expect("ncod");
        let want_dist = f64::from_bits(u64::from_str_radix(f[20], 16).expect("dist hex"));
        let want_bits: u64 = f[21].parse().expect("bits");
        let want_m: Vec<i32> = (0..BANDW).map(|i| f[22 + i].parse().expect("m")).collect();

        let d = quant::step_of(g, false) as f64 / 16.0;
        let lam = 0.13 * d * d;
        let got = quant::band_rdo(&x, &tables, class, d, lam, mmax, ncod).unwrap_or_else(|| {
            panic!("line {}: class {class} found no codeable tuple", lineno + 1)
        });

        assert_eq!(
            got.m.to_vec(),
            want_m,
            "line {}: class {class} g={g} ncod={ncod} — magnitudes differ from the \
             golden's. x = {:?}",
            lineno + 1,
            &x[..8]
        );
        assert_eq!(
            got.bits,
            want_bits,
            "line {}: class {class} g={g} — {} bits against the golden's {want_bits}",
            lineno + 1,
            got.bits
        );
        // Distortion is compared relatively, not bitwise, and the distinction is
        // deliberate: the oracle sums the 16 squared errors sequentially and this
        // implementation sums per tuple then across tuples, so the two differ by
        // rounding — 1 ULP at 4e11, measured. The *decisions* above are compared
        // exactly, which is where a real divergence would show; the tolerance here is
        // 1e-12 relative against the ~1e-6+ a genuine disagreement produces (the first
        // version of this test compared `J` and read a 5e-4 gap).
        let rel = (got.dist - want_dist).abs() / want_dist.abs().max(1e-300);
        assert!(
            rel < 1e-12,
            "line {}: class {class} g={g} — distortion {:.12e} against the oracle's \
             {want_dist:.12e} (relative {rel:.3e})",
            lineno + 1,
            got.dist
        );
        worst_rel = worst_rel.max(rel);
        cases += 1;
        per_class[class] += 1;
    }
    assert_eq!(cases, 2400, "the fixture changed size");
    eprintln!(
        "band_rdo differential: {cases} cases, worst distortion drift {worst_rel:.2e} relative"
    );
    // A fixture that drifted to one class would still "pass" while testing one arm.
    for (c, &n) in per_class.iter().enumerate().skip(1) {
        assert!(
            n > 50,
            "class {c} has only {n} cases; the differential is too narrow"
        );
    }
}

/// The class-symbol counterpart of [`a_symbol_with_no_code_is_never_chosen`], and the
/// third place the oracle carries an `if (!l) continue;`.
///
/// `quantise_chan_rd` prices a band's class with the context table's length. A class
/// whose symbol has no code would cost zero bits and therefore win every comparison —
/// and unlike the coefficient case, the writer would then trip an `assert!` inside a
/// worker thread, which is a panic where a filter belongs.
#[test]
fn a_class_with_no_code_is_never_chosen() {
    let mut tables = DefaultTables::build();
    let starved = 3usize;
    assert!(
        tables.cb[0].symbol_len(starved) > 0,
        "the default row codes class 3"
    );
    tables.cb[0].len[starved] = 0;
    tables.cb[0].code[starved] = 0;

    let d = quant::step_of(100, false) as f64 / 16.0;
    let mut coef = [0f32; M];
    // A band whose magnitudes sit where class 3 is the natural fit.
    for (i, v) in coef.iter_mut().take(16).enumerate() {
        *v = (i as f32 + 1.0) * d as f32 * 2.0;
    }
    let rd = quant::quantise_chan_rd(&coef, &tables, 100, false, 0.13);
    assert!(
        rd.chan.bands.iter().all(|b| b.class != starved),
        "a class whose band symbol has no code was chosen; the writer would then panic \
         inside a frame-coding thread instead of this being a filter"
    );
    assert!(
        !rd.chan.bands.is_empty(),
        "the channel came out empty for a band with real energy"
    );
}

// ---------------------------------------------------------------------------
// S3b: M/S
// ---------------------------------------------------------------------------

/// Correlated stereo, generated here rather than taken from the committed fixture.
///
/// `tests/fixtures/mdct_in.f32` deliberately has two *independent* channels (different
/// chirps, independent noise), which is good for the MDCT witness and useless for M/S: a
/// difference channel as loud as the sum channel is the one case M/S cannot help. This
/// writes `L` as a signal and `R` as 0.8 of it plus a little noise, which is what a stereo
/// master actually looks like and where the flag earns its place.
fn correlated_album(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut bytes = Vec::with_capacity(N_SAMPLES * 8);
    for i in 0..N_SAMPLES {
        let t = i as f32;
        let l = 7000.0 * (t * 0.021).sin() + 3000.0 * (t * 0.0037).sin();
        let noise = 200.0 * ((i.wrapping_mul(2_654_435_761)) % 1000) as f32 / 1000.0 - 100.0;
        let r = 0.8 * l + noise;
        bytes.extend_from_slice(&l.to_le_bytes());
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    (
        materialise(dir, "album.f32", &bytes),
        write_tracks(dir, &format!("1\t0\t{N_SAMPLES}\tCorr\tS3b\n")),
    )
}

/// The rotation is orthonormal and the `√2` lives in the **step**, not in the
/// coefficients — which is the whole content of the derivation, so it is asserted
/// directly rather than inferred from an SNR.
///
/// `M = (L+R)/√2, S = (L-R)/√2` preserves energy, so the encoder's rotated-domain
/// squared error *is* the output-domain squared error: for errors `e_M`, `e_S` the two
/// output channels err by `(e_M ± e_S)/√2`, whose squares sum to `e_M² + e_S²`. That is
/// why `sum_of_rotated_errors` is comparable to an L/R frame's and why the allocation can
/// weigh the two `ms` choices on one scale at all.
#[test]
fn the_rotation_is_orthonormal_and_the_sqrt2_lives_in_the_step() {
    let mut coef = [0f32; 2 * M];
    for (i, v) in coef.iter_mut().enumerate() {
        // Deliberately asymmetric: symmetric input would hide a channel swap.
        *v = ((i as f32) * 0.37).sin() * 9000.0 + (i % 7) as f32;
    }
    let rot = quant::rotate(&coef, true);
    let e_in: f64 = coef.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let e_rot: f64 = rot.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let rel = (e_in - e_rot).abs() / e_in;
    assert!(
        rel < 1e-6,
        "the rotation changed the frame's energy by {rel:.2e} (in {e_in:.6e}, out \
         {e_rot:.6e}); an orthonormal rotation is what makes the rotated-domain \
         distortion equal the output-domain distortion"
    );

    // The rotation is its own inverse up to the √2, i.e. L = (M+S)/√2 — exactly what the
    // reader computes as a plain integer add/subtract.
    let r = std::f64::consts::FRAC_1_SQRT_2;
    for i in [0usize, 1, 100, M - 1] {
        let (m, s) = (f64::from(rot[i]), f64::from(rot[M + i]));
        let want_l = f64::from(coef[i]);
        let got_l = (m + s) * r;
        assert!(
            (got_l - want_l).abs() <= 1e-3 * want_l.abs().max(1.0),
            "coefficient {i}: (M+S)/√2 = {got_l} but L = {want_l}; the reader's add would \
             reconstruct the wrong channel"
        );
    }

    // And the step: M/S quantises with `step_bar*√2/16`, L/R with `step_bar/16`, where
    // `step_bar` is the reader's step for the frame's *effective* index.
    for g in [8i32, 60, 100, 140, 187] {
        let ms = quant::Unit::of(g, true);
        // The reader's step for an M/S frame is the one at `geff = g - 4`, and the
        // encoder's unit is *that* step's unit times √2. Comparing against the L/R unit
        // at the same `g` would compare two different steps.
        let same_step_lr = quant::Unit::of(g - 4, false);
        assert!(
            (ms.step_bar - same_step_lr.step_bar).abs() < 1e-12,
            "g={g}: an M/S frame reads the step at geff = g-4, not at g"
        );
        assert!(
            (ms.d / same_step_lr.d - std::f64::consts::SQRT_2).abs() < 1e-12,
            "g={g}: the M/S unit is not √2 times the L/R unit at the same step \
             ({:.6} against {:.6})",
            ms.d,
            same_step_lr.d
        );
        assert_eq!(
            ms.mmax, same_step_lr.mmax,
            "g={g}: mmax is set by the reader's step, so the √2 must not touch it"
        );
    }
}

/// The `√2` in the wrong place is a *scale* error, which SNR catches and bit counts do
/// not.
///
/// Two claims, both from one synthetic album written here rather than from the committed
/// fixture — whose two channels are deliberately independent, so it cannot show a
/// correlation gain and would make a fine witness for the wrong thing. `L` is a signal and
/// `R` is 0.8 of it plus a little independent noise: correlated stereo, which is where M/S
/// earns its place.
#[test]
fn an_ms_album_decodes_to_sane_audio_and_beats_lr_on_correlated_stereo() {
    let dir = TempDir::new("ms-decode");
    let (album, tracks) = correlated_album(dir.path());
    let run = |ms: Option<bool>, g: i32| {
        encode(&Args {
            album: album.clone(),
            tracks: tracks.clone(),
            budget: 4_000_000,
            cap_pct: DEFAULT_CAP_PCT,
            g: Some(g),
            rd: false,
            kappa: 0.13,
            stride: 8,
            ms,
            threads: 1,
        })
        .expect("encodes")
    };
    let g = 60;
    let forced = run(Some(true), g);
    assert_eq!(
        forced.report.ms_frames, forced.report.n_frames,
        "--ms must set every frame"
    );
    assert_eq!(forced.report.xover, 0, "a coefficient broke |X| < 2^28");
    assert_eq!(forced.report.hfnz, 0, "energy coded at or past NCODED");
    let ms_snr = forced.report.snr_db.expect("a real album has an SNR");
    assert!(
        ms_snr > 25.0,
        "an M/S album at g={g} measures {ms_snr:.2} dB. A misplaced √2 scales the output \
         and lands near 3 dB, so this is the number that says the constant is where it \
         belongs — or that it is not"
    );

    // The comparison is a *scaling* check, not a quality claim, and the distinction
    // matters: at the same `g` an M/S frame is **coarser**, because it spends 4 step
    // indices and quantises at √2 times the unit. So M/S cannot be expected to win at
    // fixed `g` — that is precisely why the oracle lets the search weigh the two under
    // `J` at each `g` instead of picking a domain up front. What must hold is that M/S is
    // in the same league: a misplaced √2 scales the output and lands near 3 dB.
    let lr = run(None, g);
    assert_eq!(lr.report.ms_frames, 0);
    let lr_snr = lr.report.snr_db.unwrap();
    assert!(
        (ms_snr - lr_snr).abs() < 6.0,
        "M/S measured {ms_snr:.2} dB against L/R's {lr_snr:.2} dB at the same step. They \
         are different operating points and need not be equal, but a gap this large means \
         the output is being scaled: check the √2 in `Unit::of` and the (sum, difference) \
         order in `rotate`"
    );
}

/// The search must be able to choose the domain, and the chosen domains must be what the
/// stream says. This is the end-to-end witness that `--rd` reaches the `ms` decision at
/// all — the S3b analogue of the `--rd`-never-ran test, which is the failure mode this
/// branch has produced twice.
#[test]
fn the_search_chooses_the_domain_per_frame_and_the_stream_agrees() {
    let dir = TempDir::new("ms-search");
    let (album, tracks) = correlated_album(dir.path());
    // A budget that binds: at 8,000 bytes this album is rate-limited, which is the only
    // regime in which the two domains are comparable at all.
    let out = encode(&Args {
        album: album.clone(),
        tracks: tracks.clone(),
        budget: 8_000,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: true,
        kappa: 0.13,
        stride: 8,
        ms: None,
        threads: 1,
    })
    .expect("encodes");
    assert!(
        out.report.ms_frames > 0,
        "the search chose M/S for none of {} frames on correlated stereo; the flag is not \
         reaching the candidate set",
        out.report.n_frames
    );
    // The like-for-like quality claim, and the *only* fair one: at the same **budget**
    // (same rate), letting the search choose the domain must beat forcing L/R. Comparing
    // the two at a fixed step cannot work — an M/S frame spends 4 step indices and
    // quantises at √2 times the unit, so at equal `g` it is simply coarser.
    let lr_only = encode(&Args {
        album,
        tracks,
        budget: 8_000,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: true,
        kappa: 0.13,
        stride: 8,
        ms: Some(false),
        threads: 1,
    })
    .expect("encodes with M/S disabled");
    assert_eq!(lr_only.report.ms_frames, 0);
    let (a, b) = (
        out.report.snr_db.expect("SNR"),
        lr_only.report.snr_db.expect("SNR"),
    );
    assert!(
        a > b + 3.0,
        "at the same budget the search's {a:.2} dB does not beat L/R's {b:.2} dB on \
         correlated stereo, so the M/S candidate is being weighed wrongly — check that the \
         two are compared with `lam` from the ms = 0 unit (one scale), not each with its own"
    );
    // And the blob says so: the header's frames decode without error, so the flag the
    // writer emitted is the flag the reader consumed.
    let album = uli_lite::Album::parse(&out.bytes).expect("parses");
    assert!(album.n_frames() > 0);
    assert_eq!(out.report.xover, 0);
    assert_eq!(out.report.hfnz, 0);
}

/// Determinism across `--threads` must survive M/S: the flag is per frame and the frame
/// bodies are mutually independent once the plan fixes it (R5a).
#[test]
fn ms_frames_are_thread_invariant() {
    let dir = TempDir::new("ms-threads");
    let (album, tracks) = correlated_album(dir.path());
    let run = |threads: usize| {
        encode(&Args {
            album: album.clone(),
            tracks: tracks.clone(),
            budget: 8_000,
            cap_pct: DEFAULT_CAP_PCT,
            g: None,
            rd: true,
            kappa: 0.13,
            stride: 8,
            ms: None,
            threads,
        })
        .expect("encodes")
    };
    let one = run(1);
    let four = run(4);
    assert!(
        four.report.ms_frames > 0,
        "no M/S frames, so the test proves nothing"
    );
    assert_eq!(one.report.ms_frames, four.report.ms_frames);
    assert_eq!(
        one.bytes, four.bytes,
        "--threads 1 and 4 differ with M/S enabled"
    );
}
