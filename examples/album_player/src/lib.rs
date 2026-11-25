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
//! | [`chrome`] | §5.1's BG0: the band fills, §5.3's authored control art, §6's bar patterns |
//! | [`surface`] | the one surface (§5.2): the band stack, element-addressed redraw, the `blank` scene |
//! | [`preview`] | host-only: the surface's map as a 30×20 character grid and a PNG (§5.6's witness) |
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
//! 2a PR-(3) adds [`chrome`] and [`surface`]: the screen the player actually draws, from the two
//! generated modules. Both keep the crate's split — the decision half (what cells hold what tiles,
//! and which of those changed) is platform-free and host-tested; only the `RegularBackground`s and
//! the commit are `#[cfg(target_arch = "arm")]`, as in [`keys`] and [`text`].
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
//! A fresh clone has no `src/assets.rs`, `src/layout.rs` or `src/glyphs.rs`, so `make check`
//! (rustfmt, which resolves `#[path]` modules statically) formats this crate
//! through `scripts/fmt_with_generated_stub.sh`'s `GENERATED` override, which
//! takes all three paths.

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
pub mod chrome;
pub mod geometry;
pub mod keys;
// `layout.rs` is `make album-bake`'s output, not source (§2.3): its advance tables are packed
// for scannability and its assertions are one line each, so rustfmt is told to leave the module
// alone. `scripts/fmt_with_generated_stub.sh` still stubs it when absent, so a fresh clone
// formats without the artifact ever existing.
#[rustfmt::skip]
pub mod layout;
// `glyphs.rs` is the other generated module of the same bake (§5.5a): the ASCII cut of the ONE
// face this variant's text elements declare, as 4bpp tiles. Same class as `layout.rs` — gitignored,
// written by `make album-bake`, reached through the same fmt stub — and deliberately a separate
// module rather than more tables in `layout.rs`: the two are generated independently, so the atlas
// restates the layout module's face id, raster, line box and counts in `const _: () = assert!`
// items below. That is where "one face per ROM" stops being a sentence in a document.
#[rustfmt::skip]
pub mod glyphs;
pub mod meta;
pub mod pcm;
pub mod position;
// The host renderer for the surface: the picture §5.6's human witness looks at, and the one the
// gate's assertions are about. Host-only because it allocates and the ROM never draws a preview.
#[cfg(not(target_arch = "arm"))]
pub mod preview;
pub mod surface;
pub mod text;
pub mod transport;
