# `ART1` — the Phase 0 probe's art blob, byte for byte

ALBUM-PLAYER.md §7 names `ART1` as the album player's art format and Phase 0 (§11)
asks for "a PNG on screen from a hand-written `ART1`". This file is that
hand-written format, written down **before** either side of it is built, so the
generator (`scripts/album_probe_art.py`) and the reader
(`src/art1.rs`, in this crate) are written against one spec rather than
converging on each other by accident.

**This is the shipping format as of 2b PR-(4) (2026-11-26).** It began as a probe layout,
and the probe's reader/writer pinned it; PR-(4) ratified it because every rule below is a
hardware fact, and the alternative was a second spec for the same conventions. The reader now
lives in `crates/gba-art` (`gba_art::art1`), used by the probe, the app and their gates; this
file is the reference. The original note, kept: §7's own note is that the
real `ART1` gets designed under AP-4 with the bake tool in the loop; what Phase 0
needs is the two things that format is made of — the *conventions* (4bpp tile
order, BG-map entry encoding, `Rgb15` banks, an off-palette colour being an error)
and the *discipline* (a writer that refuses before it emits, a reader that refuses
before it indexes, and a checksum the reader re-verifies). Those are testable
here; a tile-pool allocation policy and multi-mode art are not, and are left out
on purpose.

## Conventions (hardware facts, restated from §7 so a generator has one spec)

- A 4bpp tile is **8 words / 32 bytes**; row *y* is word *y*; pixel *x* of a row
  is nibble *x*, **low nibble = leftmost pixel**.
- A BG map entry is `tile | hflip<<10 | vflip<<11 | palette_bank<<12`.
- A palette bank is 16 `Rgb15` entries, 2 bytes each, entry 0 first. `Rgb15` is
  `R | G<<5 | B<<10`, 5 bits per channel.
- **An off-palette source colour is an error, never a nearest-match.** The
  generator refuses; it does not silently snap a colour to the nearest palette
  entry, because that is how a cover quietly turns to mud (§7).

## Layout

All integers are `u32` little-endian unless the row says otherwise. The header is
64 bytes; it is followed immediately by the three sections, in this order, each
naturally aligned (palette and tile sections are always multiples of 4; the map
section may not be, so the file is padded with zeros to a multiple of 4 at the
end only).

| off | size | field | must be |
|---|---|---|---|
| 0 | 4 | magic | `ART1` |
| 4 | 4 | version | `1` |
| 8 | 4 | `width_px` | multiple of 8, `8..=240` |
| 12 | 4 | `height_px` | multiple of 8, `8..=160` |
| 16 | 4 | `tile_bytes` | `32` (4bpp — the only mode this probe reads) |
| 20 | 4 | `n_palette_banks` | `1..=4` (the §5.4 allocation is 3–6 for art) |
| 24 | 4 | `tile_cols` | `== width_px / 8` |
| 28 | 4 | `tile_rows` | `== height_px / 8` |
| 32 | 4 | `dither_strength` | `0..=255`, informational (what the tool used) |
| 36 | 4 | `source_hash` | FNV-1a/32 of the pre-quantisation RGB bytes; informational, printed by the tool |
| 40 | 8 | reserved | zero |
| 48 | 4 | `checksum` | FNV-1a/32 over `palettes ‖ tiles ‖ map`, in that order |
| 52 | 12 | reserved | zero |

Sections:

1. **palettes** — `n_palette_banks × 32` bytes: bank 0 first, 16 `Rgb15` entries
   per bank, entry 0 first.
2. **tiles** — `tile_cols × tile_rows × 32` bytes, in row-major tile order
   (tile 0 is the top-left 8×8 block).
3. **map** — `tile_cols × tile_rows` `u16` LE BG-map entries, same order. The
   high 4 bits of the flags field (hflip/vflip) must be 0 in this probe, and the
   `palette_bank` field must be `< n_palette_banks`.

Total file length is therefore
`64 + 32·n_palette_banks + 32·tile_cols·tile_rows + 2·tile_cols·tile_rows`,
rounded up to a multiple of 4 by trailing zero pad. Both sides compute this and
refuse if the actual length disagrees — that check is the whole reason the header
carries derived counts rather than leaving them to be inferred from the file size.

## What the reader refuses (and the probe tests each one)

bad magic · version ≠ 1 · `tile_bytes` ≠ 32 · a dimension not a multiple of 8 ·
a dimension out of the visible range · `n_palette_banks` = 0 or > 4 ·
`tile_cols`/`tile_rows` inconsistent with the dimensions · file length
inconsistent with the sections · checksum mismatch · a map entry naming a tile id
≥ the tile count · a map entry naming a palette bank ≥ `n_palette_banks` · a
non-zero reserved word.

Every refusal names which rule failed. Nothing in the reader defaults, clamps or
re-parses: **it refuses before it indexes**, which is the `gafp_pack.py`
`audit_blob` rule ("a blob is not accepted because it was written") applied to the
app's own blob, as ALBUM-PLAYER.md §4.3 requires.

## What the generator produces

A **procedural test card**, not a photograph: a colour-bars band, a two-colour
Bayer-dithered luminance ramp, a tile-boundary grid, and a diagonal edge. A test
card shows the things a probe has to show — tile seams, dither behaviour, palette
seams — and it is reproducible from the script alone, with no image file in the
repository and no Python imaging library in the toolchain
(`import PIL` fails on this host; `ffmpeg` is present and is not needed here).

The tool draws the card in 24-bit RGB into a frame buffer, collects the **exact**
set of distinct colours it produced, and **refuses** if that set exceeds
`--palette-max` (default 15): the palette is the set of colours actually used, so
"off-palette" cannot happen after quantisation and the refusal is real rather than
decorative. A PNG twin (same colours, unpalettised) is written next to the blob
for a human to look at; PNG is chosen because it needs only `zlib` from the
standard library, and the reader of the blob never sees it.
