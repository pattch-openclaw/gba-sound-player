//! S1: a `g` the reader cannot look up is refused by the encoder.
//!
//! `g` reaches three places — the frame header's Exp-Golomb delta, the container's
//! `u16` `g_prev` in the seek table and the track entries, and the reader's
//! `STEP[g - 4*ms]` lookup — and none of them bounded it on the way out. The
//! failure mode was the worst available one: a whole album written, every
//! encoder-side check passed, and the frame rejected on hardware as `BadFrame`.
//!
//! The bound is the **decoder's own** (`uli_lite::format::NSTEP`, applied in
//! `decode_frame`), not the search's `GMIN..GMAX`: that window is its RD
//! search's policy, which is E3's business and would refuse legal albums if it were
//! copied here. The last test below is the reason the rule is worth having at all —
//! it shows the decoder really does reject what an unvalidated plan produces.

mod common;

use common::{TempDir, silence_album, single_track};
use uli_enc::{
    Args, Content, DEFAULT_BUDGET, DEFAULT_CAP_PCT, DefaultTables, FrameKind, FramePos, GPlan, Pcm,
    Plan, bitwriter::BitWriter, container, frame_count, write_frame,
};
use uli_lite::format::{M, NSTEP};
use uli_lite::{Album, Decoder, DecoderState, Error as DecodeError};

/// Plan an album of `album_frames` frames against a content vector, so the
/// length-agreement rule can be tested apart from the range rule.
fn plan_built(album_frames: usize, g: &[i32], ms: &[bool]) -> Result<Plan, uli_enc::Error> {
    let dir = TempDir::new("gplan");
    let n_samples = (album_frames - 1) * M;
    let album = silence_album(dir.path(), n_samples);
    let tracks = single_track(dir.path(), n_samples as u32);
    let pcm = Pcm::open(&album, &tracks).expect("inputs open");
    assert_eq!(frame_count(pcm.n_samples), album_frames, "test setup");
    let tables = DefaultTables::build();
    Plan::new(
        &pcm,
        &tables,
        &Content::from_plan(GPlan::new(g.to_vec(), ms.to_vec())),
        DEFAULT_BUDGET,
        DEFAULT_CAP_PCT,
    )
}

fn plan_with(g: &[i32], ms: &[bool]) -> Result<Plan, uli_enc::Error> {
    plan_built(g.len(), g, ms)
}

#[test]
fn a_step_index_outside_the_window_is_refused_and_names_the_frame() {
    for (g, ms, why) in [
        (NSTEP as i32, false, "one past the top of the table"),
        (
            NSTEP as i32 + 4,
            true,
            "one past, counting the ms offset back",
        ),
        (-1, false, "below zero"),
        (0, true, "legal as g, but ms costs 4 and it lands at -4"),
    ] {
        let g_plan = GPlan::new(vec![g; 2], vec![ms; 2]);
        let err = g_plan
            .validate()
            .expect_err(&format!("g={g} ms={ms}: {why}"));
        assert_eq!(err.frame, 0, "the first frame is the first offender");
        assert_eq!(err.g, g);
        assert_eq!(err.ms, ms);
        assert_eq!(
            err.geff,
            g - i32::from(ms) * 4,
            "geff is the reader's own sum"
        );
        let text = err.to_string();
        assert!(text.contains(&format!("frame 0 wants g={g}")), "{text}");
        assert!(
            text.contains(&format!("0..{NSTEP}")),
            "the message must state the interval it violated: {text}"
        );
    }
}

/// `ms` is not decoration: it spends 4 step indices out of `g`, so the top of the
/// window moves with it. Getting this wrong by one either refuses legal albums or
/// accepts frames the reader rejects, and only one of those is loud.
#[test]
fn the_ms_bit_moves_the_window_by_exactly_four() {
    assert!(GPlan::legal(0, false), "g=0 ms=0 is S1's own silence frame");
    assert!(
        !GPlan::legal(0, true),
        "ms costs 4, so g=0 cannot be an M/S frame"
    );
    assert!(GPlan::legal(4, true), "the first g that affords ms");
    assert!(!GPlan::legal(3, true), "one short of that");
    assert!(GPlan::legal(NSTEP as i32 - 1, false));
    assert!(
        GPlan::legal(NSTEP as i32 + 3, true),
        "the top of the window with ms paid: geff = {}",
        GPlan::step_index(NSTEP as i32 + 3, true)
    );
    assert!(!GPlan::legal(NSTEP as i32 + 4, true));
}

/// The refusal is `Plan::new`'s, not a private helper's — that is what makes it a
/// refusal rather than a panic somewhere in the emit loop, and what E2 leans on
/// when the RD search proposes a `g` the container cannot carry.
#[test]
fn the_plan_refuses_an_illegal_content_vector() {
    let err = plan_with(&[0, 0, NSTEP as i32], &[false; 3]).expect_err("frame 2 is out of range");
    match err {
        uli_enc::Error::GPlan(v) => assert_eq!(v.frame, 2, "the offender, not frame 0"),
        other => panic!("expected a frame-plan refusal, got {other:?}"),
    }

    // A legal plan of the same shape passes, so the test above is not passing on
    // an unrelated error.
    plan_with(&[0, 0, NSTEP as i32 - 1], &[false; 3]).expect("the same plan inside the window");
}

/// The two vectors describe the same frames or there is no plan.
#[test]
fn a_content_plan_of_the_wrong_length_is_refused() {
    // The album is 3 frames; the plan covers 2. Not the range rule — every g here
    // is legal — so the two refusals cannot be confused for one another.
    let err = plan_built(3, &[0, 0], &[false; 2]).expect_err("2 frames is not 3");
    assert!(
        matches!(&err, uli_enc::Error::Plan(m) if m.contains("2 frames") && m.contains("3 frames")),
        "{err}"
    );
}

/// `g_prev` is the state a seek has to reproduce, and frame 0 is special: it is 0
/// no matter what frame 0's own `g` is, because there is no previous delta. This is
/// the oracle's `e.g_prev = (uint16_t)(e.preroll_frame ? g[e.preroll_frame-1] : 0)`
/// and it now has exactly one Rust definition, which every caller reads.
#[test]
fn the_state_entering_a_frame_has_one_definition() {
    let g_plan = GPlan::new(vec![40, 0, 77, 77], vec![false; 4]);
    assert_eq!(g_plan.g_prev(0), 0, "frame 0 enters at 0, not at its own g");
    assert_eq!(g_plan.g_prev(1), 40);
    assert_eq!(g_plan.g_prev(2), 0);
    assert_eq!(g_plan.g_prev(3), 77);
    g_plan.validate().expect("all four are in range");
}

/// **Why the rule is worth having:** an unvalidated plan really does produce a blob
/// that parses, passes every container check, and then cannot be decoded. Built by
/// hand here because `encode` cannot reach it any more — the refusal happens one
/// function earlier, which is the whole point of the bound.
#[test]
fn an_out_of_range_g_decodes_as_badframe_not_as_wrong_audio() {
    let dir = TempDir::new("gplan-decode");
    let n_frames = 3usize;
    let n_samples = (n_frames - 1) * M;
    let album = silence_album(dir.path(), n_samples);
    let tracks = single_track(dir.path(), n_samples as u32);
    let pcm = Pcm::open(&album, &tracks).expect("inputs open");
    let tables = DefaultTables::build();

    // The container is planned against legal silence so `Plan::new` is not what
    // this test trips over; the stream written under it is the illegal one.
    let plan = Plan::new(
        &pcm,
        &tables,
        &Content::silence(n_frames),
        DEFAULT_BUDGET,
        DEFAULT_CAP_PCT,
    )
    .expect("a legal plan is what the container needs");

    let bad_g = NSTEP as i32 + 8;
    assert!(!GPlan::legal(bad_g, false), "the point of the test");
    let mut bw = BitWriter::new();
    let mut g_prev = 0i32;
    let mut fpos = Vec::new();
    for _ in 0..n_frames {
        fpos.push(FramePos {
            bitpos: bw.pos(),
            g_prev,
        });
        write_frame(&mut bw, &tables, &mut g_prev, bad_g, 0, FrameKind::Silence);
    }
    let bytes = container::emit(&plan, &tables, &fpos, &bw.finish());

    // It parses. That is what made this dangerous: nothing on the way in objects.
    let album = Album::parse(&bytes).expect("the container is well formed");
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("decoder opens");
    let mut out8l = [0i8; M];
    let mut out8r = [0i8; M];
    let err = dec
        .decode_frame(&mut out8l, &mut out8r, None, None)
        .expect_err("a step index past the table must not decode");
    assert!(
        matches!(err, DecodeError::BadFrame),
        "expected BadFrame from the reader's own bound, got {err:?}"
    );
}

/// The encoder's default content passes its own rule — the trivial half, but it is
/// the half that would break if `NSTEP` ever stopped containing 0.
#[test]
fn e1s_own_content_is_legal() {
    GPlan::silence(frame_count(64 * M as u32))
        .validate()
        .expect("silence at g=0 is inside the window");
    // And `encode` still produces a whole album under the new rule.
    let dir = TempDir::new("gplan-encode");
    let n_samples = 8 * M;
    let album = silence_album(dir.path(), n_samples);
    let tracks = single_track(dir.path(), n_samples as u32);
    let out = uli_enc::encode(&Args {
        album,
        tracks,
        budget: DEFAULT_BUDGET,
        cap_pct: DEFAULT_CAP_PCT,
        g: None,
        rd: false,
        kappa: 0.13,
        stride: 8,
        fine_width: 0,
        train: false,
        ms: None,
        threads: 2,
    })
    .expect("S1 still encodes");
    assert_eq!(out.report.n_frames, 9);
}
