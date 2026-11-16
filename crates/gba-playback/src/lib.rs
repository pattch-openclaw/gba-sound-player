//! # gba-playback — the playback data path, hardware-free
//!
//! Lift of the feeder logic the instrument measured: decoded i32 samples are
//! truncated to the GBA sound hardware's 8-bit FIFO form and pushed through a
//! chunk ring that a timer-IRQ feeder drains at the hardware rate. Everything
//! that can be witnessed on the host lives HERE, pure and `no_std`: the
//! truncation rule, the two-channel FIFO byte folds, the ring's
//! producer/consumer accounting, the underflow classification, and
//! [`playback::RingFeed::truncate`] (the skip seam).
//!
//! Provenance: lifted 2026-11 from `examples/flac_spike/src/{playback,checksum}.rs`
//! into a crate the application may own — ALBUM-PLAYER.md §2.2. The instrument
//! (and the product ROM beside it) now depends on this crate instead of
//! `#[path]`-including those files, so host and ROM still run one
//! implementation. [`nodiv`] (ALBUM-PLAYER.md §6) arrived by the same rule one
//! PR later, moved 2026-11-15 out of `examples/album_probe/src/nodiv.rs`: the
//! probe, the application and their host gates all paint from it, and a §2.2
//! lift is what makes that one implementation instead of two.
//!
//! ## No critical section lives here
//!
//! The crate owns no interrupt masking and no atomics: `thumbv4t-none-eabi`
//! ships no atomic integer types, and the ARM7TDMI has no atomic instructions.
//! The *caller* holds the critical section around every shared access — the
//! producer side around [`playback::RingFeed::push_bytes`], the feeder side
//! around [`playback::RingFeed::take_chunk`] and [`playback::RingFeed::truncate`].
//! The concurrency contract that makes that sufficient is stated in
//! [`playback`]'s module docs.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod checksum;
pub mod nodiv;
pub mod playback;

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::vec::Vec;

    /// §6's arithmetic carries exactly one dividing code line, and it is the
    /// bake tool's. The census lives here rather than in `nodiv.rs` because the
    /// file it scans would otherwise contain the scanner and count its own
    /// source (the operator checks spelled a literal `'/'`).
    #[test]
    fn nodiv_source_has_one_tagged_bake_time_divide() {
        let src = include_str!("nodiv.rs");
        let mut dividing = Vec::new();
        for (index, raw) in src.lines().enumerate() {
            let code = raw.split("//").next().unwrap_or("");
            if code.contains('/') || code.contains('%') {
                dividing.push((index + 1, raw));
            }
        }
        assert_eq!(
            dividing.len(),
            1,
            "src/nodiv.rs must carry exactly one dividing code line, the bake-time \
             reciprocal; found {dividing:?}"
        );
        let (line, raw) = dividing[0];
        assert!(
            raw.contains("bake-time divide"),
            "line {line} of src/nodiv.rs divides at run time: {raw}"
        );
    }
}
