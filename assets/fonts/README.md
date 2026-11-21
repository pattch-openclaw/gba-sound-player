# Fonts

| File | Used by | License |
|---|---|---|
| `ark-pixel-10px-proportional-latin.ttf` | the spike / E5 default text; the tracked parent of the ASCII cut below | OFL (`OFL.txt`) |
| `ark-pixel-10px-proportional-latin-ascii.ttf` | **the face `layout_spec.toml` declares for variant B** (`[[face]] ark10`, the product-eligible variant) — the 2a text mechanism's declared face (§5.5a step 1 of `ALBUM-PLAYER.md`) | OFL (`OFL.txt`) |
| `fusion-pixel-8px-proportional-latin-ascii.ttf` | the compact 8 px text screen (`examples/e5_fetch`); the face `layout_spec.toml` declares for variant A and for the instrument-only faces (§5.5's verdict face) | OFL (`fusion-pixel-OFL.txt`) plus the source licences in `fusion-pixel-LICENSES/` |

The fusion file is a **96-glyph ASCII subset** (`U+0020–U+007E`) of
`fusion-pixel-8px-proportional-latin.ttf` from
<https://github.com/TakWolf/fusion-pixel-font> (release 2026.11.10). The subset
exists because `agb::include_font!` rasterizes *every* glyph in the file, and the
full CJK font has 28k glyphs — 28k rasterizations for the ~20 characters this ROM
draws. The subset is 6.9 KB and 96 glyphs.

The ark file is the same kind of cut of the tracked ark parent — 96 glyphs, of
which 95 are mapped and 94 have outlines (space is mapped with no outline, and
`.notdef` is unmapped), 7.8 KB — declared by `[[face]] ark10` in
`examples/album_player/layout_spec.toml`. Counted that way rather than as "94
glyphs" because the three numbers mean different things to three different tools:
`agb::include_font!` rasterizes `numGlyphs`, a fit verdict needs the mapped codepoints
and the ones with advances, and the glyph-atlas term in the pool audit needs the
outlines. `scripts/album_bake.py --print-layout` prints all three from the file.

Both cuts are **generated, and re-checked as generated**: `--layout-self-test` runs
the command below against the tracked parent and compares sha256 with the tracked
cut, so a hand-edited subset would be caught rather than trusted. That leg passed
byte for byte when the ark cut was added (`29100b1e…a75f9`).

Regenerate with fontTools:

```sh
pyftsubset fusion-pixel-8px-proportional-latin.ttf \
  --unicodes=U+0020-007E \
  --output-file=fusion-pixel-8px-proportional-latin-ascii.ttf \
  --layout-features='' --no-hinting --desubroutinize

pyftsubset ark-pixel-10px-proportional-latin.ttf \
  --unicodes=U+0020-007E \
  --output-file=ark-pixel-10px-proportional-latin-ascii.ttf \
  --layout-features='' --no-hinting --desubroutinize
```
