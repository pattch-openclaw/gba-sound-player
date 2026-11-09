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
// [6a-5b] `forbid(unsafe_code)` is the shipped posture and stays the shipped
// posture. It is relaxed to `deny` ONLY in a build that links 6a-5b's vendored
// leg, because that leg is the E10 archive verbatim and the archive carries a
// module-scoped `allow(unsafe_code)` for its `.iwram` placement attributes and
// its FFI boundary (E2b's pattern) — which `forbid` makes impossible to express.
// cfg-gating the relaxation is what keeps the sentence "the instrument's default
// build forbids unsafe" true with the feature off and checkable, rather than a
// claim that a feature-flag quietly weakened.
#![cfg_attr(not(feature = "item6a5b-vendored"), forbid(unsafe_code))]
#![cfg_attr(feature = "item6a5b-vendored", deny(unsafe_code))]
#![warn(missing_docs)]

pub mod assets;
pub mod checksum;
pub mod driver;
// [6a-5b] the E10 archive's vendored decode path, kept verbatim, as the fourth
// measurement leg. Off by default: a default build of this crate contains no
// vendored decoder, and every pre-6a-5b number in these docs was taken without
// it. Why a *vendored* leg rather than another `RiceKernel` arm is the point of
// the rung — plan-of-record 6a-5b.
#[cfg(feature = "item6a5b-vendored")]
pub mod e10;
pub mod playback;
pub mod stats;
