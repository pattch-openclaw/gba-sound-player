# Fonts

| File | Used by | License |
|---|---|---|
| `ark-pixel-10px-proportional-latin.ttf` | the spike / E5 default text | OFL (`OFL.txt`) |
| `fusion-pixel-8px-proportional-latin-ascii.ttf` | the compact 8 px text screen (`examples/e5_fetch`) | OFL (`fusion-pixel-OFL.txt`) plus the source licences in `fusion-pixel-LICENSES/` |

The fusion file is a **96-glyph ASCII subset** (`U+0020–U+007E`) of
`fusion-pixel-8px-proportional-latin.ttf` from
<https://github.com/TakWolf/fusion-pixel-font> (release 2026.11.10). The subset
exists because `agb::include_font!` rasterizes *every* glyph in the file, and the
full CJK font has 28k glyphs — 28k rasterizations for the ~20 characters this ROM
draws. The subset is 6.9 KB and 96 glyphs.

Regenerate with fontTools:

```sh
pyftsubset fusion-pixel-8px-proportional-latin.ttf \
  --unicodes=U+0020-007E \
  --output-file=fusion-pixel-8px-proportional-latin-ascii.ttf \
  --layout-features='' --no-hinting --desubroutinize
```
