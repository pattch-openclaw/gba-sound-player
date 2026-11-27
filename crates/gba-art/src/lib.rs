//! `gba-art` — the album art blob and the scene that draws it (2b PR-(4), ALBUM-PLAYER.md §7).
//!
//! Two halves:
//!
//! - [`art1`] reads and refuses the `ART1` byte layout. The format is the one
//!   `examples/album_probe/ART1.md` specifies and the probe's reader/writer already pin; PR-(4)
//!   ratifies it as the shipping format rather than inventing a second one, because every rule in
//!   it is a hardware fact (4bpp tile order, the map-entry encoding, 5-bit channels, an
//!   off-palette colour being an error).
//! - [`paint`] turns a parsed image into [`gba_scene::Writes`] cells inside a rectangle. It is the
//!   **instance** of the scene vocabulary: a scene crate draws by writing cells, not by owning a
//!   background or a commit.
//!
//! **What is absent, on purpose:** no palette loading, no `TileSet`, no agb type, no commit, no
//! registry. The application loads the ART1 palettes and tiles into the background's own tileset
//! and palette banks; this crate only says which cell gets which tile/bank.
//!
//! The reader is host-pure so the probe, the application and their gates use one implementation
//! (§2.2: what the app needs from a probe is lifted, never `#[path]`-included).
#![no_std]
#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod art1;
pub mod paint;

pub use paint::paint;
