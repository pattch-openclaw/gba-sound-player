//! Host witness for the E10 PR B `refill_merge` kernel (`make kernel-test`).
//!
//! Layers: the committed table's expectations re-derived by the naive
//! bit-loop reversal oracle (`oracle_rev8` — independent of the committed
//! REV8 table's shape); the twin (the assembly's single shifted-OR merge)
//! equal to table and oracle everywhere; and the fail-closed census: exact
//! count, no duplicate (window, byte) inputs, EVERY byte value 0..=255
//! pinned, and the top byte of every input window zero (the merge
//! contract the kernel documents).
//!
//! The ARM assembly is swept on-target by the kernel-check ROM over the
//! SAME committed table (crate docs: no host ARM executor).

use arm_kernels::rm_vectors::RM_VECTORS;
use arm_kernels::tables::oracle_rev8;

#[test]
fn vector_census() {
    // Fail-closed census, mirroring the generator's.
    assert_eq!(RM_VECTORS.len(), 470);
    let mut bytes = [false; 256];
    for (i, (window, byte, _merged)) in RM_VECTORS.iter().enumerate() {
        assert_eq!(
            window & 0xFF00_0000,
            0,
            "vector {i}: window top byte nonzero — violates the merge contract"
        );
        bytes[usize::from(*byte)] = true;
        for j in 0..i {
            let p = &RM_VECTORS[j];
            assert!(
                (p.0, p.1) != (*window, *byte),
                "duplicate (window, byte) input at vectors {j}/{i}"
            );
        }
    }
    assert!(bytes.iter().all(|b| *b), "every byte value 0..=255 pinned");
}

#[test]
fn table_expectations_match_the_oracle() {
    for (i, (window, byte, merged)) in RM_VECTORS.iter().enumerate() {
        assert_eq!(
            oracle_rev8(*byte) as u32,
            (merged >> 24) & 0xFF,
            "oracle vs table (reversed byte), vector {i}"
        );
        assert_eq!(
            window | (u32::from(oracle_rev8(*byte)) << 24),
            *merged,
            "oracle vs table (merge), vector {i}"
        );
        assert_eq!(
            *merged & 0xFF00_0000,
            u32::from(oracle_rev8(*byte)) << 24,
            "vector {i}: top byte is the merged byte"
        );
        assert_eq!(
            *merged & 0x00FF_FFFF,
            *window & 0x00FF_FFFF,
            "vector {i}: the merge never touches lower bytes"
        );
    }
}

#[test]
fn twin_matches_table_and_oracle() {
    for (i, (window, byte, merged)) in RM_VECTORS.iter().enumerate() {
        assert_eq!(
            arm_kernels::tables::oracle_rev8(*byte),
            oracle_rev8(*byte),
            "sanity, vector {i}"
        );
        assert_eq!(
            arm_kernels::refill_merge::refill_merge_twin(*window, *byte),
            *merged,
            "twin vs table, vector {i}"
        );
    }
    // The committed REV8 table re-derived from its oracle, exhaustively.
    for b in 0..=255u8 {
        assert_eq!(
            arm_kernels::tables::REV8[usize::from(b)],
            oracle_rev8(b),
            "committed REV8[{b}] vs the bit-loop oracle"
        );
        // Involution: reversing twice returns the byte (witnesses the
        // table's shape independently of its construction).
        assert_eq!(oracle_rev8(oracle_rev8(b)), b, "rev8 involution at {b}");
    }
    // Every byte at every one-bit window position.
    for bit in 0..24u32 {
        let window = 1u32 << bit;
        for b in [0x00u8, 0x01, 0x80, 0xFF] {
            let merged = arm_kernels::refill_merge::refill_merge_twin(window, b);
            assert_eq!(
                merged & 0x00FF_FFFF,
                window,
                "window bits survive at bit {bit} byte {b}"
            );
            assert_eq!(
                merged >> 24,
                u32::from(oracle_rev8(b)),
                "top byte at bit {bit} byte {b}"
            );
        }
    }
}
