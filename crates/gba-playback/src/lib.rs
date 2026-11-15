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
//! implementation.
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
pub mod playback;
