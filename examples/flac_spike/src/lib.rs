//! # flac_spike — the Phase 1 performance gate (PR 3: cycle harness + per-frame cost)
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
//!   verifies the embedded region hashes (PR 1), **decodes every frame of
//!   both arms on-target** against the generator's `fnv_pcm` pins (PR 2),
//!   then times every frame inside free-running cycle-counter windows and
//!   reports min/max/sum + worst-frame index per arm to serial (PR 3). BLUE
//!   means decode proven **and** worst-frame-vs-budget passed on these bytes.
//!
//! The two layers share this crate deliberately: the host test proves the
//! *embedded assets* decode correctly, and proves the ROM's exact fold
//! (`checksum::fold_i16le_stereo`) reaches the PCM pins on the host; the ROM
//! then runs the *same fold code* on *those same bytes*. A fast wrong decode
//! measures nothing (FLAC.md, correctness-before-speed).
//!
//! ## What lands in later PRs
//!
//! PR 3 landed the cycle harness; PR 4 adds the cadence test; PR 5 the
//! verdict. The `driver`'s per-frame callback seam exists for exactly that
//! progression: collect-and-diff (PR 1, host), hash-as-you-go (PR 2, landed),
//! time-as-you-go (PR 3, landed), play-as-you-go cadence (PR 4) — without
//! touching decode logic.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod checksum;
pub mod driver;
pub mod stats;
