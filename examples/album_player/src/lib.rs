//! # album_player — the Album Player's manifest half
//!
//! Phase 1, PR-2: the pack step and the reader it is checked through. Nothing
//! here plays audio. The crate carries three things:
//!
//! | module | what it is |
//! |---|---|
//! | [`meta`] | the validating `ALBM1` reader — validate, bounds-check, slice, refuse by name (ALBUM-PLAYER.md §4.3) |
//! | [`assets`] | generated: `ALBM1` via `include_bytes!` and the per-track pins the bake cross-checked against each packed `GAFP` container |
//! | [`pcm`] | [`pcm::PcmSource`], the codec seam (§4.2): nine methods, album samples across the boundary, no codec name |
//! | [`geometry`] | the app's audio-chain geometry (§4.2b), derived per backend from declared facts rather than transcribed |
//! | [`backend`] | the only module that knows an implementor exists — `flac` (shipped) or `stub` (synthetic, codec-free) |
//! | [`nodiv`] | `crates/gba-playback` (path dependency) — §6's no-divide arithmetic, the same implementation the probe and its host gate paint from |
//!
//! `nodiv` is a dependency, not a `#[path]` include: §2.2 rules that no Phase 1
//! module may include or depend on anything under a spike or probe directory, and
//! the probe's copy of this arithmetic was lifted into the crate both sides now
//! depend on. §4.2b rules the same way about geometry — the app derives and owns
//! its own per-backend [`geometry`] rather than borrowing the FLAC track's — so
//! [`meta::PRODUCT_RATE_HZ`] and [`geometry::RATE_HZ`] are one fact asserted
//! against each other in `geometry.rs`.
//!
//! PR-2 landed the manifest half; PR-3 step (ii) lands this seam and its geometry.
//! The ROM main and the `agb` bootstrap are step (iii): no binary is declared
//! here and `backend/` links no display or timer registers.
//!
//! ## Generated, never tracked
//!
//! `album.toml` is tracked; `assets/albm1.bin`, `assets/trackNN.gfp` and
//! `src/assets.rs` are build output (§2.3, §9.0). `make album-bake` writes them;
//! `make album-pack-check` packs twice and compares bytes, then runs this
//! crate's host gate, which parses the finished `ALBM1` through [`meta`] and
//! compares it field by field with the config restated as literals.
//!
//! A fresh clone has no `src/assets.rs`, so `make check` (rustfmt, which resolves
//! `#[path]` modules statically) formats this crate through
//! `scripts/fmt_with_generated_stub.sh`'s `GENERATED` override.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(not(target_arch = "arm"), forbid(unsafe_code))]

// The FLAC arm owns its decode scratch in `Vec`s (the reference ROM does the same); the stub arm
// and the seam itself allocate nothing. `alloc` is `no_std`'s heap, so this is not a host-only
// dependency.
#[cfg(feature = "backend-flac")]
extern crate alloc;

// §6's no-divide arithmetic, from the crate that owns it (ALBUM-PLAYER.md §2.2):
// one implementation for the probe, this crate and the host gates that hold both
// to it. No `#[path]` include stands here any more — that is the point of the lift.
pub use gba_playback::nodiv;

pub mod assets;
pub mod backend;
pub mod geometry;
pub mod meta;
pub mod pcm;
