# `gba-art`

`ART1`, the album's art blob: the shipping reader, and the paint that writes it into a
`gba_scene::Rect`.

- `art1` — the byte layout `examples/album_probe/ART1.md` specifies, validated before any
  section is indexed (magic, version, dimensions, banks, length, checksum, map entries);
- `paint(art, rect, base_bank, &mut Writes)` — one cell per map entry, `Source::Art`, into the
  caller's rectangle. The application loads the palette bank and builds the tileset; this crate
  only says which cell gets which tile/bank.

The reader was lifted out of the Phase 0 probe in 2b PR-(4) so the probe, the app, the host gate
and `album-preview` use one implementation (ALBUM-PLAYER.md §2.2: lift what the app depends on,
never `#[path]`-include a probe).

The generated blob is `examples/album_player/assets/art1.bin` (`scripts/album_art.py`, from the
tracked cover), and `make album-art-check` re-reads it through this parser.
