//! Host witness for the E10 PR A `rice_unmap` fold kernel (`make kernel-test`).
//!
//! Four layers, per the crate's dual-witness rule, with the sign convention
//! anchored to the encoder side (the step-3a history: the scaffold's doc had
//! the fold sign-FLIPPED, and only the RFC's worked example + §9.2.7.2
//! settled it):
//! - the committed table's own expectations are re-derived by
//!   `oracle_rice_unmap` — the spec's branch form, independent of the SWAR
//!   identity the kernel uses;
//! - the **mechanism anchor**: composing every table entry with the
//!   encoder-side fold from §9.2.7.2 (`x ≥ 0 → 2x`, `x < 0 → −2x − 1`)
//!   must recover the folded input — a sign-convention misreading does not
//!   survive composition with the encoder's rule (this is the witness that
//!   convicted the scaffold's version in step 3a);
//! - the twin (SWAR, the assembly's shape) must equal table, oracle, and a
//!   negate-and-subtract oracle (a fourth mechanism) everywhere, including
//!   exhaustive low/high sweeps;
//! - the fail-closed census re-asserts the generator's counts and pins.
//!
//! The ARM assembly is NOT reachable from the host (aarch64 machine, no ARM
//! executor — see crate docs); it is swept on-target by the kernel-check ROM
//! over this same committed table.

use arm_kernels::rice_unmap::{oracle_rice_unmap, rice_unmap_twin};
use arm_kernels::unmap_vectors::UNMAP_VECTORS;

/// The **encoder-side** fold of §9.2.7.2 (`x ≥ 0 → 2x`, `x < 0 → −2x − 1`),
/// computed in i64 so the domain question never becomes an overflow bug in
/// the witness itself. `i32::MIN` is not in the fold's domain (no u32 maps
/// to it — §9.2.7.3 forbids it in streams); the round-trip tests below keep
/// the domain explicit instead of wrapping it away.
fn encoder_fold(x: i32) -> Option<u32> {
    let folded = if x >= 0 {
        i64::from(x) * 2
    } else {
        i64::from(x).wrapping_mul(-2) - 1
    };
    u32::try_from(folded).ok()
}

#[test]
fn vector_census() {
    // Fail-closed census, mirroring the generator's: shape, uniqueness,
    // both input parities, the convention pins with their source inputs,
    // and the RFC worked example.
    assert_eq!(UNMAP_VECTORS.len(), 839);
    let mut outcomes = [false; 4]; // 0, -1, i32::MAX, i32::MIN
    for (i, (n, e)) in UNMAP_VECTORS.iter().enumerate() {
        match *e {
            0 => outcomes[0] = true,
            -1 => outcomes[1] = true,
            i32::MAX => outcomes[2] = true,
            i32::MIN => outcomes[3] = true,
            _ => {}
        }
        for j in 0..i {
            assert!(
                UNMAP_VECTORS[j].0 != *n,
                "duplicate input 0x{n:08X} at vectors {j}/{i}"
            );
        }
    }
    assert!(
        UNMAP_VECTORS.iter().any(|(n, _)| n & 1 == 0)
            && UNMAP_VECTORS.iter().any(|(n, _)| n & 1 == 1),
        "both input parities must be present (the two branches)"
    );
    assert!(
        outcomes.iter().all(|o| *o),
        "pins 0 / -1 / i32::MAX / i32::MIN all present"
    );
    // ... each from its pinned source input, not by coincidence.
    for (n, e) in [
        (0u32, 0i32),
        (1, -1),
        (0xFFFF_FFFE, i32::MAX),
        (0xFFFF_FFFF, i32::MIN),
    ] {
        assert!(
            UNMAP_VECTORS.contains(&(n, e)),
            "pin ({n:#010X}, {e}) missing"
        );
    }
    assert!(
        UNMAP_VECTORS.contains(&(38, 19)),
        "RFC §9.2.7.2 worked example folded 38 → +19 must be a committed vector"
    );
}

#[test]
fn table_expectations_match_the_oracle() {
    // The committed expectations are the author's belief until the spec's
    // own branch rule re-derives them.
    for (i, (n, e)) in UNMAP_VECTORS.iter().enumerate() {
        assert_eq!(oracle_rice_unmap(*n), *e, "vector {i} for input 0x{n:08X}");
    }
}

#[test]
fn oracle_is_the_inverse_of_the_encoder_fold() {
    // The mechanism anchor, per table entry: fold(expected) == n. Compose
    // with the encoder's rule and a sign-convention flip cannot survive.
    for (i, (n, e)) in UNMAP_VECTORS.iter().enumerate() {
        assert_eq!(
            encoder_fold(*e),
            Some(*n),
            "vector {i}: fold({e}) != 0x{n:08X}"
        );
    }
    // And the other direction over the fold's whole domain (every i32
    // except the §9.2.7.3-forbidden MIN): unmap(fold(x)) == x. Exhaustive
    // around every structural edge, deterministic LCG elsewhere.
    for x in [i32::MIN + 1, -2, -1, 0, 1, 2, i32::MAX - 1, i32::MAX] {
        let f = encoder_fold(x).expect("domain excludes only i32::MIN");
        assert_eq!(oracle_rice_unmap(f), x, "unmap(fold({x})) at edge");
    }
    let mut s = 0x2026_1016u32;
    let mut next = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s
    };
    // LCG over i + deterministic ±2^k offsets around each draw — dense at
    // the fold's sign boundary without a 4-billion-iteration sweep.
    for _ in 0..50_000 {
        let base = {
            let v = next() as i32;
            if v == i32::MIN { i32::MIN + 1 } else { v }
        };
        for d in [-2i32, -1, 0, 1, 2] {
            let x = match base.checked_add(d) {
                Some(x) if x != i32::MIN => x,
                _ => continue,
            };
            let f = encoder_fold(x).expect("domain excludes only i32::MIN");
            assert_eq!(oracle_rice_unmap(f), x, "unmap(fold({x})) LCG±offsets");
        }
        let k = next() % 31;
        for x in [1i32 << k, -1i32 << k, (1i32 << k) - 1, (-1i32 << k) - 1] {
            if x == i32::MIN {
                continue;
            }
            let f = encoder_fold(x).expect("domain excludes only i32::MIN");
            assert_eq!(
                oracle_rice_unmap(f),
                x,
                "unmap(fold({x})) power-of-two grid"
            );
        }
    }
}

#[test]
fn twin_matches_table_oracle_and_negate_subtract() {
    // A fourth mechanism: odd → −(m+1) by wrapping negate-and-subtract —
    // different code path from both the branch form and the SWAR twin.
    let negate_subtract = |n: u32| -> i32 {
        let m = (n >> 1) as i32;
        if n & 1 == 0 {
            m
        } else {
            m.wrapping_add(1).wrapping_neg()
        }
    };
    for (i, (n, e)) in UNMAP_VECTORS.iter().enumerate() {
        assert_eq!(rice_unmap_twin(*n), *e, "twin vs table, vector {i}");
        assert_eq!(
            rice_unmap_twin(*n),
            oracle_rice_unmap(*n),
            "twin vs oracle, vector {i}"
        );
        assert_eq!(
            rice_unmap_twin(*n),
            negate_subtract(*n),
            "twin vs neg-sub, vector {i}"
        );
    }
    // Exhaustive low sweep and the dense top of the folded domain (where
    // i32::MIN and the §9.2.7.3 forbidden extreme land).
    for n in 0..(1u32 << 20) {
        assert_eq!(rice_unmap_twin(n), oracle_rice_unmap(n), "for 0x{n:08X}");
    }
    // (`..=u32::MAX` — RangeInclusive handles the top boundary; a
    // RangeFrom overflows incrementing past MAX, measured in this run.)
    for n in 0xFFFF_F000u32..=u32::MAX {
        assert_eq!(rice_unmap_twin(n), oracle_rice_unmap(n), "for 0x{n:08X}");
    }
    // Every single-bit input and its neighbours; odd mantissa at every
    // shift position (the parity branch boundary at bit 0 and bit k).
    for b in 0..32u32 {
        let p = 1u32 << b;
        for n in [p, p.wrapping_sub(1), p.wrapping_add(1), !p] {
            assert_eq!(
                rice_unmap_twin(n),
                oracle_rice_unmap(n),
                "single-bit edge 0x{n:08X}"
            );
        }
        let mut s = 0x0bad_c0deu32 ^ b;
        let mut next = || {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            s
        };
        for _ in 0..64 {
            let n = if b == 0 {
                next() | 1
            } else {
                (next() | 1) << b | 1
            };
            assert_eq!(rice_unmap_twin(n), oracle_rice_unmap(n), "mantissa at {b}");
        }
    }
}
