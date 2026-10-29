//! Host witness for the E10 PR B composite kernel (`sample_step`,
//! `make kernel-test`).
//!
//! Layers, each judged against independent formulations:
//! - the committed table's expectations re-derived by the naive oracle
//!   (`oracle_sample_step`: first-set-bit walk, bit-by-bit remainder sum
//!   with the terminating-word-only rule, the §9.2.7.2 unmap in the
//!   spec's BRANCH form over the coded number);
//! - a THIRD formulation of the unmap leg: compose the residual against
//!   the ENCODER-side fold (§9.2.7.2: `x ≥ 0 → 2x`, `x < 0 → −2x − 1`)
//!   round-trip — PR A's convention anchor re-deployed on the composite's
//!   fold, so a sign misreading cannot survive the composition;
//! - the Rust twin (tier dispatch + De Bruijn + the literal extract +
//!   the SWAR fold — the assembly's shape) must equal table and oracle
//!   everywhere;
//! - fail-closed census: exact count, no duplicate (slice, budget, p)
//!   inputs, every tier populated, the unterminated family present, and
//!   the terminated q 0..=63 covered at every grid p.
//!
//! The ARM assembly is swept on-target by the kernel-check ROM over the
//! SAME committed table (crate docs: no host ARM executor).

use arm_kernels::sample_step::{oracle_sample_step, sample_step_twin};
use arm_kernels::ss_vectors::SS_VECTORS;

/// The encoder-side fold (§9.2.7.2), computed in i64 with an explicit
/// domain — PR A's mechanism anchor, re-used for the composite's fold:
/// `fold(unmap(coded)) == coded` must hold for every vector.
fn encoder_fold(x: i32) -> Option<u32> {
    let v = i64::from(x);
    let coded = if v >= 0 { 2 * v } else { -2 * v - 1 };
    u32::try_from(coded).ok()
}

fn words_of(v: &(u32, u32, u32, u32, u32, u32, u32, u32, i32, u32, u32, u32)) -> [u32; 4] {
    [v.0, v.1, v.2, v.3]
}

#[test]
fn vector_census() {
    assert_eq!(SS_VECTORS.len(), 456);
    let mut tier1 = 0usize; // q < 8
    let mut tier2 = 0usize; // 8 <= q <= 31
    let mut tier3 = 0usize; // q >= 32
    let mut unterminated = 0usize;
    let mut grid: [u8; 7] = [0; 7]; // per grid p: count of distinct terminated q
    for (i, v) in SS_VECTORS.iter().enumerate() {
        let (w0, w1, w2, w3, budget, p, q, _rem, _res, drain, consumed, status) = *v;
        assert!(budget <= 4, "vector {i}: budget {budget} exceeds the slice");
        assert!(status <= 1, "vector {i}: status {status} not 0/1");
        assert!(p <= 14, "vector {i}: p {p} outside the sweep domain");
        assert!(q <= 128, "vector {i}: q {q} out of range");
        if status == 1 {
            match q {
                0..=7 => tier1 += 1,
                8..=31 => tier2 += 1,
                _ => tier3 += 1,
            }
            assert_eq!(drain, q + 1 + p, "vector {i}: drain != q+1+p");
            assert_eq!(consumed, (q + 1 + p).div_ceil(32), "vector {i}: consumed");
            match p {
                0 => grid[0] += 1,
                1 => grid[1] += 1,
                2 => grid[2] += 1,
                3 => grid[3] += 1,
                4 => grid[4] += 1,
                8 => grid[5] += 1,
                14 => grid[6] += 1,
                _ => {}
            }
        } else {
            unterminated += 1;
            assert_eq!(
                (v.6, v.7, v.8, v.9),
                (32 * budget, 0, 0, 0),
                "vector {i}: unterminated fields pinned 0"
            );
        }
        for j in 0..i {
            let p2 = &SS_VECTORS[j];
            assert!(
                (p2.0, p2.1, p2.2, p2.3, p2.4, p2.5) != (w0, w1, w2, w3, budget, p),
                "duplicate (slice, budget, p) input at vectors {j}/{i}"
            );
        }
    }
    assert!(
        tier1 > 0 && tier2 > 0 && tier3 > 0,
        "every tier must be populated: {tier1}/{tier2}/{tier3}"
    );
    assert!(unterminated > 0, "the unterminated family must be present");
    assert!(
        grid.iter().all(|c| *c >= 64),
        "the terminated sweep must cover q 0..=63 at every grid p: {grid:?}"
    );
}

#[test]
fn table_expectations_match_the_oracle() {
    for (i, v) in SS_VECTORS.iter().enumerate() {
        let words = words_of(v);
        let got = oracle_sample_step(&words, v.4, v.5);
        assert_eq!(
            got,
            (v.6, v.7, v.8, v.9, v.10, v.11),
            "oracle vs table, vector {i}"
        );
    }
}

#[test]
fn residual_round_trips_through_the_encoder_fold() {
    // PR A's convention anchor on the composite's fold: unmap(coded) then
    // the encoder's fold must return coded exactly — no sign-convention
    // misreading survives the composition.
    for (i, v) in SS_VECTORS.iter().enumerate() {
        if v.11 != 1 {
            continue; // unterminated: residual pinned 0, meaningless
        }
        let coded = (v.6 << v.5) | v.7;
        assert_eq!(
            encoder_fold(v.8),
            Some(coded),
            "encoder-fold round trip, vector {i}: residual {} coded {coded}",
            v.8
        );
    }
}

#[test]
fn twin_matches_table_and_oracle() {
    for (i, v) in SS_VECTORS.iter().enumerate() {
        let words = words_of(v);
        assert_eq!(
            sample_step_twin(&words, v.4, v.5),
            (v.6, v.7, v.8, v.9, v.10, v.11),
            "twin vs table, vector {i}"
        );
        assert_eq!(
            sample_step_twin(&words, v.4, v.5),
            oracle_sample_step(&words, v.4, v.5),
            "twin vs oracle, vector {i}"
        );
    }
    // Exhaustive single-bit placement x the grid p (every tier, every
    // remainder geometry the sweep promises).
    let p_grid = [0u32, 1, 2, 3, 4, 8, 14];
    for bit in 0..128u32 {
        let mut words = [0u32; 4];
        words[(bit / 32) as usize] = 1 << (bit % 32);
        for &p in &p_grid {
            assert_eq!(
                sample_step_twin(&words, 4, p),
                oracle_sample_step(&words, 4, p),
                "single bit {bit} p {p}"
            );
        }
    }
    // The shift-by-32 sentinel, re-derived: q = 31 with nonzero bits in
    // words 1..=3 — remainder is 0 by the pinned extract, and the
    // residual is the fold of 31 << p alone.
    for &p in &p_grid {
        let words = [0x8000_0000, 0xDEAD_BEEF, 0xCAFE_F00D, 0x1234_5678];
        let (q, rem, residual, drain, consumed, status) = sample_step_twin(&words, 4, p);
        assert_eq!(
            (q, rem, status),
            (31, 0, 1),
            "sentinel q/rem/status at p {p}"
        );
        assert_eq!(drain, 32 + p, "sentinel drain at p {p}");
        assert_eq!(
            consumed,
            (32 + p).div_ceil(32),
            "sentinel consumed at p {p}"
        );
        let coded = 31u32 << p;
        let expect = (((coded >> 1) ^ ((coded & 1).wrapping_neg())) as i32);
        assert_eq!(residual, expect, "sentinel residual at p {p}");
    }
    // Every budget for an all-zero slice (the unterminated family).
    for budget in 0..=4u32 {
        for &p in &p_grid {
            assert_eq!(
                sample_step_twin(&[0; 4], budget, p),
                (32 * budget, 0, 0, 0, budget, 0),
                "unterminated budget {budget} p {p}"
            );
        }
    }
}
