//! Host witness for the E10 PR B C2 kernel (`word_skip`, `make kernel-test`).
//!
//! Layers, each judged against independent formulations:
//! - the committed table's expectations re-derived by the naive bit-walk
//!   oracle (`oracle_word_skip`) — the table cannot silently encode its
//!   author's run-counting misreading;
//! - a THIRD formulation: the four words concatenated little-endian into a
//!   u128 and the builtin `trailing_zeros`, bounded by the budget;
//! - the Rust twin (the assembly's whole-word + ctz8-walk shape) must
//!   equal table and oracle everywhere;
//! - fail-closed census: exact count, no duplicate inputs, BOTH statuses,
//!   budget 0 pinned, every terminated q 0..=127 present.
//!
//! The ARM assembly is swept on-target by the kernel-check ROM over the
//! SAME committed table (crate docs: no host ARM executor).

use arm_kernels::word_skip::{oracle_word_skip, word_skip_twin};
use arm_kernels::ws_vectors::WS_VECTORS;

/// Third formulation: concatenate the four words little-endian into a
/// u128; the builtin `trailing_zeros` answers when it lands inside the
/// budget, else the unterminated triple (decision 3's shape).
fn builtin_word_skip(words: &[u32; 4], budget: u32) -> (u32, u32, u32) {
    let stream = u128::from(words[0])
        | (u128::from(words[1]) << 32)
        | (u128::from(words[2]) << 64)
        | (u128::from(words[3]) << 96);
    let tz = stream.trailing_zeros();
    if tz < 32 * budget {
        (tz, tz / 32 + 1, 1)
    } else {
        (32 * budget, budget, 0)
    }
}

fn words_of(v: &(u32, u32, u32, u32, u32, u32, u32, u32)) -> [u32; 4] {
    [v.0, v.1, v.2, v.3]
}

#[test]
fn vector_census() {
    // Fail-closed census, mirroring the generator's: count, uniqueness of
    // the (slice, budget) input, both statuses, budget 0, and full
    // terminated-q coverage.
    assert_eq!(WS_VECTORS.len(), 241);
    let mut statuses = 0u8;
    let mut terminated_q: [bool; 128] = [false; 128];
    let mut budget_zero = false;
    for (i, v) in WS_VECTORS.iter().enumerate() {
        let (_w0, _w1, _w2, _w3, budget, q, _consumed, status) = *v;
        assert!(budget <= 4, "vector {i}: budget {budget} exceeds the slice");
        assert!(status <= 1, "vector {i}: status {status} not 0/1");
        assert!(q <= 128, "vector {i}: q {q} out of range");
        if status == 1 {
            terminated_q[usize::try_from(q).unwrap()] = true;
        }
        statuses |= 1u8 << status;
        budget_zero |= budget == 0;
        for j in 0..i {
            let p = &WS_VECTORS[j];
            assert!(
                (p.0, p.1, p.2, p.3, p.4) != (_w0, _w1, _w2, _w3, budget),
                "duplicate input at vectors {j}/{i}"
            );
        }
    }
    assert_eq!(statuses, 0b11, "both statuses must be present");
    assert!(budget_zero, "budget 0 pinned (decision 4's boundary)");
    assert!(
        terminated_q.iter().all(|t| *t),
        "the terminated sweep must cover every q 0..=127"
    );
}

#[test]
fn table_expectations_match_the_oracle() {
    for (i, v) in WS_VECTORS.iter().enumerate() {
        let words = words_of(v);
        let got = oracle_word_skip(&words, v.4);
        assert_eq!(got, (v.5, v.6, v.7), "oracle vs table, vector {i}");
    }
}

#[test]
fn oracle_pinned_against_builtin() {
    for (i, v) in WS_VECTORS.iter().enumerate() {
        let words = words_of(v);
        assert_eq!(
            builtin_word_skip(&words, v.4),
            oracle_word_skip(&words, v.4),
            "builtin vs oracle, vector {i}"
        );
    }
    // Deterministic LCG slices beyond the committed table's draws.
    let mut s = 0x2026_1028u32;
    let mut next = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s
    };
    for _ in 0..50_000 {
        let words = [next(), next(), next(), next()];
        let budget = next() % 5;
        assert_eq!(
            builtin_word_skip(&words, budget),
            oracle_word_skip(&words, budget),
            "sweep slice {words:?} budget {budget}"
        );
    }
}

#[test]
fn twin_matches_table_and_oracle() {
    for (i, v) in WS_VECTORS.iter().enumerate() {
        let words = words_of(v);
        assert_eq!(
            word_skip_twin(&words, v.4),
            (v.5, v.6, v.7),
            "twin vs table, vector {i}"
        );
        assert_eq!(
            word_skip_twin(&words, v.4),
            oracle_word_skip(&words, v.4),
            "twin vs oracle, vector {i}"
        );
    }
    // Exhaustive single-bit placement over the 128-bit slice at budget 4:
    // every depth of the whole-word skip, the table walk's every landing.
    for bit in 0..128u32 {
        let mut words = [0u32; 4];
        words[(bit / 32) as usize] = 1 << (bit % 32);
        assert_eq!(
            word_skip_twin(&words, 4),
            (bit, bit / 32 + 1, 1),
            "single bit {bit}"
        );
    }
    // Every budget for an all-zero slice (the unterminated family).
    for budget in 0..=4u32 {
        assert_eq!(
            word_skip_twin(&[0; 4], budget),
            (32 * budget, budget, 0),
            "budget {budget}"
        );
    }
}
