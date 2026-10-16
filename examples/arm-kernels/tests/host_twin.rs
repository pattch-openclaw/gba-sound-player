//! Host witness for the E10 C1 kernel (`make kernel-test`).
//!
//! Three layers, each judged against an independent oracle:
//! - the committed table's own expectations are re-derived by a naive
//!   bit-walk (`oracle_ctz`) — the table cannot silently encode its
//!   author's `ctz` misreading;
//! - the oracle is pinned against `u32::trailing_zeros` (a third
//!   independent formulation) over deterministic sweeps;
//! - the Rust twin must equal the table and the oracle everywhere,
//!   including exhaustive low-bit sweeps and every single-bit input.
//!
//! The ARM assembly is NOT reachable from the host (this machine is
//! aarch64; no ARM executor exists here — see crate docs); it is swept
//! on-target by the kernel-check ROM over this same committed table.
//! (Integration tests are separate crates, so `std` is fine here — the
//! same convention as flac-lite's host suites.)

use arm_kernels::ctz32::{BRUIJN_INDEX, BRUIJN_K, ctz32_twin, oracle_ctz};
use arm_kernels::vectors::VECTORS;

#[test]
fn vector_census() {
    // Fail-closed census, mirroring the generator's: shape, uniqueness,
    // and full outcome coverage (0..=31 plus the 32 sentinel).
    assert_eq!(VECTORS.len(), 346);
    let mut outcomes = 0u64; // bit q set when some vector expects q
    for (i, (w, q)) in VECTORS.iter().enumerate() {
        assert!(*q <= 32, "vector {i} expects out-of-range {q}");
        outcomes |= 1u64 << *q;
        for j in 0..i {
            assert!(
                VECTORS[j].0 != *w,
                "duplicate input 0x{w:08X} at vectors {j}/{i}"
            );
        }
    }
    assert_eq!(
        outcomes,
        (1u64 << 33) - 1,
        "outcome coverage must span every ctz 0..=31 plus the 32 sentinel"
    );
}

#[test]
fn table_expectations_match_the_oracle() {
    // The committed expectations are the author's belief until an
    // independent oracle re-derives them.
    for (i, (w, q)) in VECTORS.iter().enumerate() {
        assert_eq!(oracle_ctz(*w), *q, "vector {i} for input 0x{w:08X}");
    }
}

#[test]
fn oracle_pinned_against_builtin() {
    // A third formulation: the naive bit walk must agree with
    // `trailing_zeros` (and its 32-sentinel behavior for 0) everywhere.
    assert_eq!(oracle_ctz(0), 0u32.trailing_zeros());
    assert_eq!(oracle_ctz(0), 32);
    for b in 0..32u32 {
        let w = 1u32 << b;
        assert_eq!(oracle_ctz(w), b);
        assert_eq!(oracle_ctz(w), w.trailing_zeros());
    }
    // Deterministic LCG sweeps, odd-mantissa-shifted included.
    let mut s = 0x2026_1014u32;
    let mut next = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s
    };
    for _ in 0..200_000 {
        let w = next();
        assert_eq!(oracle_ctz(w), w.trailing_zeros(), "for 0x{w:08X}");
        let w_shift = (next() | 1) << (next() % 32);
        assert_eq!(oracle_ctz(w_shift), w_shift.trailing_zeros());
    }
}

#[test]
fn twin_matches_table_oracle_and_builtin() {
    for (i, (w, q)) in VECTORS.iter().enumerate() {
        assert_eq!(ctz32_twin(*w), *q, "twin vs table, vector {i}");
        assert_eq!(ctz32_twin(*w), oracle_ctz(*w), "twin vs oracle, vector {i}");
        assert_eq!(
            ctz32_twin(*w),
            w.trailing_zeros(),
            "twin vs builtin, vector {i}"
        );
    }
    // Exhaustive low sweep: every word below 2^20.
    for w in 0..(1u32 << 20) {
        assert_eq!(ctz32_twin(w), w.trailing_zeros(), "for 0x{w:08X}");
    }
    // Every single-bit input and every shift of an odd mantissa.
    let mut s = 0x0bad_c0deu32;
    let mut next = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s
    };
    for b in 0..32u32 {
        assert_eq!(ctz32_twin(1u32 << b), b);
        for _ in 0..64 {
            let w = (next() | 1) << b;
            assert_eq!(ctz32_twin(w), b, "odd mantissa shifted to {b}");
        }
    }
}

#[test]
fn bruijn_table_is_a_bijection() {
    // The De Bruijn identity measured in FLAC.md, re-derived here: every
    // single-bit input indexes a distinct entry answering its own bit.
    let mut used = [false; 32];
    for b in 0..32u32 {
        let idx = ((1u32 << b).wrapping_mul(BRUIJN_K) >> 27) as usize;
        assert!(
            !used[idx],
            "index {idx} used twice (bit {b} collides with an earlier bit)"
        );
        used[idx] = true;
        assert_eq!(BRUIJN_INDEX[idx] as u32, b, "index {idx} for bit {b}");
    }
    assert!(used.iter().all(|u| *u), "32 entries, 32 uses — bijection");
}
