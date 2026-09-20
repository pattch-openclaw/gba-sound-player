//! # flac_spike — the Phase 1 performance gate (PR 2: on-target decode proof)
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
//!   verifies the embedded region hashes (PR 1) **and decodes every frame of
//!   both arms on-target**, folding the decoded PCM into FNV-1a-64 and
//!   comparing against the generator's `fnv_pcm` pins (PR 2). BLUE now means
//!   decode proven on the image's own bytes, not just asset integrity.
//!
//! The two layers share this crate deliberately: the host test proves the
//! *embedded assets* decode correctly, and proves the ROM's exact fold
//! (`checksum::fold_i16le_stereo`) reaches the PCM pins on the host; the ROM
//! then runs the *same fold code* on *those same bytes*. A fast wrong decode
//! measures nothing (FLAC.md, correctness-before-speed).
//!
//! ## What lands in later PRs
//!
//! PR 3 adds the cycle harness; PR 4 the cadence test; PR 5 the verdict. The
//! `driver`'s per-frame callback seam exists for exactly that progression:
//! collect-and-diff (PR 1, host), hash-as-you-go (PR 2, landed), time-as-you-go
//! (PR 3), play-as-you-go cadence (PR 4) — without touching decode logic.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod checksum;
pub mod driver;
