# Notices and third-party licences

The project's own code and assets are **MIT licensed** ([`LICENSE`](LICENSE)), copyright
(c) 2026 Samuel Sampson.
The following components keep their own terms; they are not relicensed by the
project licence.

## Third-party code

| component | licence | where |
|---|---|---|
| `agb` 0.25.0 | **MPL-2.0** | crates.io dependency; the ROM links it. Distributing a ROM built with `agb` must satisfy MPL-2.0's source-availability for `agb` (and for any modifications to its files) |
| `opus-rs` (vendored) | **BSD-3-Clause** | `examples/opus_spike/vendor/opus-rs/` (`COPYING`); used by the Opus spike, not the album player |
| `fusion-pixel-8px` and `ark-pixel-10px` cuts | **SIL OFL 1.1** | `assets/fonts/` (licence files beside the fonts) |
| `uli-lite`, `uli-lite-gba`, `uli-enc` | **unresolved** | derived from the `gba-audio` reference; see `ULI2.md`. Not covered by the project licence until that provenance is settled |
| `scripts/uli_prep.py`, `scripts/uli_resample.py` | **unresolved** | copied from the `gba-audio` reference's `encoder/prep.py` and `eval/gba_dac_model.py` so the asset path is self-contained; the same provenance question as the ULI2 crates |

The rest of `crates/`, `examples/` and `scripts/` is first-party and covered by
the root licence. `flac-lite` is validated against libFLAC (BSD-3-Clause); if it
is ever treated as a port rather than an independent implementation, libFLAC's
notice should be added here.

## Assets

- `assets/images/bioluminescent.jpeg` — the fixture album cover, a project asset
  (copyright (c) 2026 Samuel Sampson). It is tracked because AP-4's reproduction
  claim needs the source (`scripts/album_art.py` regenerates `ART1` from it); the
  committed file is 17,473,190 B, mostly a Photoshop APP1 preview chain, which
  should be stripped before redistribution.
- `assets/sound/*.wav` — the reference masters the fixture album is packed from;
  project assets unless a source is noted otherwise.
- `scripts/data/uli_cross_tracklist.json` and `scripts/data/uli_cross_LICENSES.md` —
  the measurement album's tracklist and CC licence notice, copied from the sibling
  reference; the audio itself is downloaded under the licences in that notice and is
  never committed.

If you distribute a ROM or a cart, carry this notice, the root `LICENSE` and the
OFL/MPL/BSD notices above with it.
