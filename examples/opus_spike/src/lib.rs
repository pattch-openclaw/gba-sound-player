//! # opus_spike — the Opus perf-gate spike scaffold (PR 1: step 1 only)
//!
//! The go/no-go measurement vehicle OPUS.md's "Measurement plan — start here
//! (added 2026-09-29)" calls for: can a 16.78 MHz ARM7TDMI decode patched
//! `opus-rs` packets fast enough to keep a playback buffer fed? Roadmap +
//! rules: [`../../../OPUS.md`](../../../../OPUS.md).
//!
//! **What PR 1 lands:** the vendored patched `opus-rs`
//! (`vendor/opus-rs/`, patch inventory in `vendor/opus-rs/PATCHES.md`) wired
//! into a standalone crate, and the compile/link proof — `make
//! native-opus-rom` builds a bootable ROM image embedding the decoder, and
//! `make opus-test` pins the host-side facts (inline decoder-state size,
//! constructor contract). **What it deliberately does NOT land:** the two
//! embedded arms + pack script (step 2), the host decode witness against the
//! ffmpeg/libopus reference, the ROM decode proof, and any cycle counting —
//! correctness-before-speed orders those next, exactly like flac_spike PRs
//! 1–3 ordered the FLAC gate.
//!
//! Like the FLAC spike, this library is `#![no_std]` and shared by both
//! layers: the host tests (`make opus-test`) and the ROM entry (`src/main.rs`,
//! `make native-opus-rom`) consume the *same* [`probe`] seam, so the code the
//! later decode proof and cycle harness measure is the code the host witness
//! exercised. One structural difference from flac_spike, recorded up front:
//! Opus decode is a **state machine** — packets are not independently
//! decodable — so the future `decode_clip` walk is sequential per clip with a
//! fresh decoder, and the packet index is the product manifest, not a
//! seek-independence witness (OPUS.md, measurement plan item 3).

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod probe;
