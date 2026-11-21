//! # album_player — the Album Player's manifest half
//!
//! Phase 1, PR-2: the pack step and the reader it is checked through. Nothing
//! here plays audio. The crate carries three things:
//!
//! | module | what it is |
//! |---|---|
//! | [`meta`] | the validating `ALBM1` reader — validate, bounds-check, slice, refuse by name (ALBUM-PLAYER.md §4.3) |
//! | [`assets`] | generated: `ALBM1` via `include_bytes!` and the per-track pins the bake cross-checked against each packed `GAFP` container |
//! | [`layout`] | generated: the selected variant's geometry — bands, region, elements, the bar grid, and every number derived from them (ALBUM-PLAYER.md §5, §5.1, §6) |
//! | [`pcm`] | [`pcm::PcmSource`], the codec seam (§4.2): nine methods, album samples across the boundary, no codec name |
//! | [`geometry`] | the app's audio-chain geometry (§4.2b), derived per backend from declared facts rather than transcribed |
//! | [`backend`] | the only module that knows an implementor exists — `flac` (shipped) or `stub` (synthetic, codec-free) |
//! | [`nodiv`] | `crates/gba-playback` (path dependency) — §6's no-divide arithmetic, the same implementation the probe and its host gate paint from |
//! | [`text`] | the verdict screen — two words, the tracked OFL font, one commit (§4.1) |
//! | [`keys`] | the input seam (§5.3): edges, autorepeat, debounce and the control map, platform-free, with a keypad source and a scripted-replay one |
//! | [`position`] | the 32-bit session clock (§10.4, ruled) and §4.4's `paint_position` |
//! | [`transport`] | the playback transport: §4.4's pause / resume contract and the hand-off gain's schedule |
//!
//! `nodiv` is a dependency, not a `#[path]` include: §2.2 rules that no Phase 1
//! module may include or depend on anything under a spike or probe directory, and
//! the probe's copy of this arithmetic was lifted into the crate both sides now
//! depend on. §4.2b rules the same way about geometry — the app derives and owns
//! its own per-backend [`geometry`] rather than borrowing the FLAC track's — so
//! [`meta::PRODUCT_RATE_HZ`] and [`geometry::RATE_HZ`] are one fact asserted
//! against each other in `geometry.rs`.
//!
//! PR-2 landed the manifest half; PR-3 step (ii) landed this seam and its geometry, and step (iii)
//! adds the ROM main (`src/album_play_main.rs`, `[[bin]] album-play`, `make album-rom`). PR-3b-i
//! adds [`keys`] and [`position`]: the input seam and the 32-bit session clock, both platform-free
//! in the half that decides anything. PR-3b-ii adds [`transport`]: §4.4's pause/resume contract and
//! the hand-off gain's schedule and the gain itself, again platform-free. The gain is kept out of
//! `crates/gba-playback` because the shipped image links that crate and an app PR does not
//! re-baseline an accepted image. The library is still platform-free: it declares no binary of
//! its own, and `backend/` links no display or timer registers — the `agb` bootstrap lives in the
//! bin. [`text`] and [`keys`] are the exceptions in the other direction, because the checks that
//! matter are on the host: the words, the subset check, the debounce/edge/repeat machine and the
//! control map are platform-free, and only the drawing half of `text` and the two `KeySource`
//! implementations of `keys` are `#[cfg(target_arch = "arm")]`.
//!
//! ## Generated, never tracked
//!
//! `album.toml` and `layout_spec.toml` are tracked; `assets/albm1.bin`,
//! `assets/trackNN.gfp`, `src/assets.rs` and `src/layout.rs` are build output
//! (§2.3, §9.0). `make album-bake` writes them; `make album-pack-check` packs
//! twice and compares bytes, then runs this crate's host gate, which parses the
//! finished `ALBM1` through [`meta`] and compares it field by field with the
//! config restated as literals. `make album-layout-test` holds [`layout`] itself:
//! §9.0's eight 2a refusals fire by id, and the module is byte-identical across
//! two emits.
//!
//! Two of those artifacts must agree about one number, and the agreement is §6's:
//! the bar grid the reciprocals in `ALBM1` were computed for is [`layout::BAR_CELLS`],
//! the same const the generated module declares, which is the same value
//! `layout_spec.toml` declares for the variant. [`meta::parse`] compares the
//! manifest against [`layout::BAR_CELLS`] at boot — that is the backstop and it is
//! kept on purpose (R11: a stale generated module linked against yesterday's
//! manifest is invisible to every host test that shares the module).
//!
//! A fresh clone has no `src/assets.rs` or `src/layout.rs`, so `make check`
//! (rustfmt, which resolves `#[path]` modules statically) formats this crate
//! through `scripts/fmt_with_generated_stub.sh`'s `GENERATED` override, which
//! takes both paths.

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
pub mod keys;
// `layout.rs` is `make album-bake`'s output, not source (§2.3): its advance tables are packed
// for scannability and its assertions are one line each, so rustfmt is told to leave the module
// alone. `scripts/fmt_with_generated_stub.sh` still stubs it when absent, so a fresh clone
// formats without the artifact ever existing.
#[rustfmt::skip]
pub mod layout;
pub mod meta;
pub mod pcm;
pub mod position;
pub mod text;
pub mod transport;
