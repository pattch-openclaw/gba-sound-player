//! The frozen-plan differential, on our side: code frames at a held-still `(g, ms, tables)`
//! and check the numbers against what the reference produces.
//!
//! `crates/uli-enc/src/frozen.rs` exists so S8 can compare framewise without the search in
//! the way (`scripts/uli_frame_diff.py` pairs it with `scripts/uli_probe.py`). These tests
//! are the part of that comparison a gate can hold: the **bit counts** are pinned to the
//! reference's measured values, so a change in the coder's output at a fixed plan fails the
//! suite instead of quietly moving a differential's baseline.
//!
//! The pinned numbers come from the probe on the committed 4-frame coefficient golden
//! (`tests/fixtures/mdct_ref_coef.f32`), default tables, `--kappa 0.13`, `mu = 0`:
//!
//! ```text
//! frame 0 g 60  ms 0  bits 11205  cycles 219754
//! frame 0 g 100 ms 0  bits 5805   cycles 209437
//! frame 0 g 140 ms 0  bits 7      cycles 156806
//! frame 0 g 100 ms 1  bits 5920   cycles 212103
//! frame 2 g 60  ms 0  bits 12214  cycles 226504
//! frame 3 g 100 ms 0  bits 6349   cycles 212964
//! frame 3 g 60  ms 1  bits 12176  cycles 228246
//! ```
//!
//! They are the reference's *emitted* bit counts (`FrameRes.bits`: the `ms` bit counted, the
//! `g` delta not), which is exactly what `FrameDump::bits` reports, and its cycle estimate,
//! which is what `FrameDump::cycles` reports.
//!
//! The cycles are pinned because they stopped being merely close: the reference's *encoder*
//! never sets `ops.long_codes` (its decoder does), so `uli_cycle_model` runs with that term at
//! zero, and `rd::frame_ops` now leaves it at zero too (`PLAN-OF-RECORD.md` → inventory row
//! 14). Measured over the 24 codings, the estimate agrees on 23 and the one residual is the
//! equal-cost tie, whose `escs` genuinely differs by one (`45` cycles). Frame 1 at `g=100
//! ms=1` is that frame and is therefore **not** pinned: it is the one case where our coding
//! and the reference's cost the same and differ. Distortion is deliberately
//! **not** pinned: ours is a direct sum of squared errors and the reference derives its `D`
//! as `J − λ·(bits − 1)`, so the two cannot agree bit for bit even where the coding does
//! (measured: 23 of 24 frame/step/flag combinations emit identical streams, and the
//! distortion differs by up to 103 ULP on the ones that code anything). The differential
//! reports that distance; a test asserting it would be asserting an arithmetic coincidence.

mod common;

use uli_enc::DefaultTables;
use uli_enc::frozen::code_at;
use uli_lite::format::{M, NSTEP};

static COEF: &[u8] = include_bytes!("fixtures/mdct_ref_coef.f32");
static TABLES: &[u8] = include_bytes!("fixtures/uli2_default_tables.bin");

const KAPPA: f64 = 0.13;

fn frame(f: usize) -> [f32; 2 * M] {
    let mut coef = [0f32; 2 * M];
    for (i, c) in COEF[f * 2 * M * 4..(f + 1) * 2 * M * 4]
        .chunks_exact(4)
        .enumerate()
    {
        coef[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    }
    coef
}

#[test]
fn the_frozen_plan_bits_are_the_references() {
    let tables = DefaultTables::build();
    let pinned: &[(usize, i32, u32, u64, u32)] = &[
        (0, 60, 0, 11205, 219754),
        (0, 100, 0, 5805, 209437),
        (0, 140, 0, 7, 156806),
        (0, 100, 1, 5920, 212103),
        (2, 60, 0, 12214, 226504),
        (3, 100, 0, 6349, 212964),
        (3, 60, 1, 12176, 228246),
    ];
    for &(f, g, ms, want, want_cycles) in pinned {
        let dump = code_at(&tables, &frame(f), g, ms, KAPPA, 0.0, 0).expect("codeable");
        assert_eq!(
            dump.bits, want,
            "frame {f} at g={g} ms={ms}: we emit {} bits, the reference {want} — the coder \
             has moved away from the only frame-level ground truth we have at a frozen plan",
            dump.bits
        );
        assert_eq!(
            dump.cycles, want_cycles,
            "frame {f} at g={g} ms={ms}: our cycle estimate is {} where the reference's is \
             {want_cycles}. These agree because `ops.long_codes` is left at zero, which is \
             what the reference's encoder does (its decoder charges it) — if this moves, the \
             estimate stopped matching and `--cap` admission moved with it",
            dump.cycles
        );
        assert_eq!(
            dump.ops.long_codes, 0,
            "frame {f}: `long_codes` must stay uncounted in the encoder's model"
        );
        // The stream must be the bits it claims: `bits` is the frame minus the `g` delta,
        // and the writer must have written exactly that much more.
        let delta = uli_enc::bitwriter::eg_bits(uli_enc::bitwriter::zz(g), 0) as u64;
        assert_eq!(
            dump.stream_bits,
            dump.bits + delta,
            "frame {f} g={g} ms={ms}: stream is {} bits, claim {} + a {delta}-bit delta",
            dump.stream_bits,
            dump.bits
        );
        assert_eq!(
            dump.stream.len() as u64,
            (dump.stream_bits + 15) / 16,
            "frame {f}: {} halfwords hold {} bits",
            dump.stream.len(),
            dump.stream_bits
        );
    }
}

/// The `g` delta is the *previous* frame's business: the same frame coded with a different
/// predecessor differs **only** in that delta's width, never in the frame's own bits. This is
/// the property that lets the differential hold `--g-prev` fixed and still compare frames.
#[test]
fn the_step_delta_is_the_only_thing_a_predecessor_changes() {
    let tables = DefaultTables::build();
    let coef = frame(0);
    let a = code_at(&tables, &coef, 100, 0, KAPPA, 0.0, 0).expect("codeable");
    let b = code_at(&tables, &coef, 100, 0, KAPPA, 0.0, 99).expect("codeable");
    assert_eq!(a.bits, b.bits, "the frame's own bits moved with g_prev");
    // zz(100) = 200 costs 15 bits and zz(100-99) = 2 costs 3, so the streams differ by 12.
    assert_eq!(
        a.stream_bits - b.stream_bits,
        12,
        "the g delta is not being written as its own field"
    );
}

/// The tables blob the differential is given must be the coder's tables, not a silently
/// ignored argument: the oracle's default blob codes identically to `build()`, because it
/// *is* that set. (`--tables` with a *trained* set is the interesting case; that one changes
/// the coding, and `scripts/uli_frame_diff.py` is what witnesses it.)
#[test]
fn a_tables_blob_codes_identically_to_the_built_in_set() {
    let built = DefaultTables::build();
    let parsed = DefaultTables::from_blob(TABLES).expect("the default blob parses");
    let coef = frame(1);
    for &(g, ms) in &[(60i32, 0u32), (100, 1), (140, 0)] {
        let a = code_at(&built, &coef, g, ms, KAPPA, 0.0, 0).expect("codeable");
        let b = code_at(&parsed, &coef, g, ms, KAPPA, 0.0, 0).expect("codeable");
        assert_eq!((a.bits, &a.stream), (b.bits, &b.stream), "g={g} ms={ms}");
    }
}

/// A step the reader cannot express is refused, not coded: `g - 4*ms` indexes the step
/// table, and a stream at an out-of-window step is arithmetic no decoder can undo. The probe
/// refuses the same windows, so a disagreement about *legality* cannot masquerade as one about
/// coding.
#[test]
fn a_step_outside_the_readers_window_is_refused() {
    let tables = DefaultTables::build();
    let coef = frame(0);
    for (g, ms) in [
        (-1i32, 0u32),
        (2, 1),
        (NSTEP as i32, 0),
        (NSTEP as i32 + 4, 1),
    ] {
        assert!(
            code_at(&tables, &coef, g, ms, KAPPA, 0.0, 0).is_err(),
            "g={g} ms={ms} is step {} and must be refused",
            g - 4 * ms as i32
        );
    }
    assert!(
        code_at(&tables, &coef, 8, 1, KAPPA, 0.0, 0).is_ok(),
        "g=8 ms=1 is step index 4, inside the window, and must be codeable"
    );
}
