//! # flac_spike — the Phase 1 performance gate (PR 1: scaffold + assets)
//!
//! The go/no-go measurement for FLAC-on-GBA: can a 16.78 MHz ARM7TDMI decode
//! `flac-lite` frames fast enough to keep a playback buffer fed, in real time?
//! Roadmap + validation rules: [`../../../FLAC.md`](../../../../FLAC.md) →
//! "Perf gate spike — concrete plan and validation process (2026-09-18)".
//!
//! This library is `#![no_std]` and shared by both witness layers:
//!
//! - **Host** (`tests/spike_witness.rs`, `make spike-test`): decodes the
//!   exact embedded bytes frame-by-frame and compares bit-exactly against the
//!   `flac -d` reference PCM blobs in `assets/`.
//! - **Target** (`src/main.rs`, `make native-spike-rom`): a bootable ROM that
//!   logs each arm's metadata and verifies the embedded region hashes, so the
//!   bytes measured in PRs 3–5 are proven present and intact in the image.
//!
//! The two layers share this crate deliberately: the host test proves the
//! *embedded assets* decode correctly; the ROM measures *those same bytes*.
//! A fast wrong decode measures nothing (FLAC.md, correctness-before-speed).
//!
//! ## What lands in later PRs
//!
//! PR 2 adds the on-target decode checksum (gates every perf number); PR 3
//! the cycle harness; PR 4 the cadence test; PR 5 the verdict. The
//! `driver`'s per-frame callback seam exists for exactly that progression:
//! collect-and-diff (PR 1, host), hash-as-you-go (PR 2), time-as-you-go
//! (PR 3), play-as-you-go cadence (PR 4) — without touching decode logic.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod checksum;
pub mod driver;
