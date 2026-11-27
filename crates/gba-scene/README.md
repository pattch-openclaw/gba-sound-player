# `gba-scene`

The vocabulary a scene writes through, platform-free `no_std`:

- `Rect` — the cells a scene owns (§5.0's "the rectangle is the enforcement");
- `Write` / `Writes` — a bounded batch of map-cell writes, with the tileset (`Source`) named;
- `Source` — `Chrome`, `Glyph`, `Art`: which tileset an index refers to.

**No `Scene` trait, no registry, no palette or tileset loading, no agb type.** The application
owns the backgrounds, the tilesets, the palette banks and the commit; a scene crate owns what it
draws into its rectangle. The absence is deliberate: ALBUM-PLAYER.md §11 requires a scene kind to
be statically derivable so the bake can audit its tile term, and a runtime registry would make
that impossible.

The app re-exports the types as `surface::{Rect, Source, Write, Writes, MAX_WRITES}`, so the
2a surface and its host legs keep their names. Host suite: `cargo test` in this crate (or the
`album-art-check`/`check` gates).
