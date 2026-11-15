//! # album_probe — the Album Player's Phase 0 probes, shared half
//!
//! The library every AP-0 probe compiles: the pure parts that the ROM and the
//! host gate read through one file, so a number a probe prints and a rule the
//! host suite proves cannot come from two implementations.
//!
//! | module | what it is |
//! |---|---|
//! | [`geometry`] | `#[path]` include of `examples/flac_integration/src/geometry.rs` — the ring, chunk and prefill the probe measures against |
//! | [`playback`], [`checksum`] | the same shared pair `examples/flac_integration/src/lib.rs` includes, so the skip probe runs the real `RingFeed` |
//! | [`art1`] | the ART1 blob reader: validate, refuse by name, blit 8 words/tile |
//! | [`synth`] | the deterministic PCM source the probes play instead of a decoder |
//! | [`position`] | the 32-bit session clock (ALBUM-PLAYER.md §4.4) |
//! | [`nodiv`] | §6's arithmetic: no `/` or `%` on a variable anywhere on the measured path |
//! | [`art_pin`] | generated: what `assets/probe_art1.bin` must say, so the ROM and the host suite check the same blob |
//!
//! ## A probe is a witness, never a gate input
//!
//! This is the `ctor-probe` rule in the Makefile, said once here: a probe ROM
//! exists to replace a target with a measurement, and nothing in the product
//! path reads it, depends on it, or gets quieter because it exists. AP-0 has its
//! own aggregate (`make album-probe-test`, and the ROM leg in Stage B); it is
//! deliberately not wired into `flac-check` or `test`.
//!
//! Nothing platform-bound lives here — no `agb`, no registers, no unsafe. The
//! `agb`-facing half (the DMA/FIFO bootstrap, the feeder IRQ, the drawing) lives
//! in the ROM bins and in `probe_hw.rs`, which are arm-only, so the host suite
//! can compile and test every module above.

#![cfg_attr(target_arch = "arm", no_std)]
// `forbid` where it can hold, `deny` where one module must opt in: `probe_hw` is
// volatile I/O at fixed addresses (there is no safe agb 0.25 wrapper for the
// sound-DMA registers), and a crate-wide `forbid` cannot be relaxed by an
// `allow` on that module (E0453). Every other module — and the whole host build,
// which never compiles `probe_hw` — keeps the absolute form. The lint also
// governs the three host-visible `#[path]` includes below (`geometry`,
// `playback`, `checksum`), because an include is compiled as this crate's code:
// an `unsafe` added to any of those breaks the probe build, and that break is the
// canary, not a bug to silence. `probe_hw`'s `timer.rs` include sits inside the
// module that allows unsafe, so its existing `unsafe` is fine.
#![cfg_attr(target_arch = "arm", deny(unsafe_code))]
#![cfg_attr(not(target_arch = "arm"), forbid(unsafe_code))]

#[path = "../../flac_integration/src/geometry.rs"]
pub mod geometry;

// The shared pair. `playback.rs` reaches its checksum as `crate::checksum`, so
// the two are included together — that is the arrangement the spike crate has
// them in, and re-declaring one without the other is a compile error rather than
// a silent fork.
#[path = "../../flac_spike/src/checksum.rs"]
pub mod checksum;

/// The decode-to-FIFO path: ring accounting and underflow classification. The
/// skip probe drives this exact type, which is what makes its silence-gap number
/// a statement about the shipped ring rather than about a model of one.
//
// `allow(unexpected_cfgs)`: `playback.rs`'s unit-test suite rides a feature
// (`rate-assets`) that belongs to the crate the file *comes from*, not to this
// one. The predicate is false here on purpose — that suite was `make rate-test`'s
// (retired 2026-11-14) to run, and moving it would change the counts that gate
// reports (the coexistence contract in the Makefile). The attribute is here so
// cargo's unexpected-cfg lint doesn't call a deliberate boundary a mistake.
#[allow(unexpected_cfgs)]
#[path = "../../flac_spike/src/playback.rs"]
pub mod playback;

pub mod art1;
pub mod art_pin;
pub mod nodiv;
pub mod position;
pub mod synth;

/// The ARM-only hardware half: the sound-FIFO DMA bootstrap, the Timer1 period
/// IRQ, the ring producer helpers and the period ledger, all copied from
/// `examples/flac_integration/src/product_play_main.rs` with its reasoning and
/// its `path:line` citations. It also `#[path]`-includes `e5_fetch`'s cycle
/// counter, so a probe's numbers and the tree's existing instruments come from
/// one file.
//
// `allow(unsafe_code)`: volatile I/O at fixed register addresses is the point of
// a hardware probe. The crate `deny`s unsafe on this target and `forbid`s it on
// the host, so this is the only door and it is one module wide.
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
pub mod probe_hw;
