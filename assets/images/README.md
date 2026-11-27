# Fixture album cover

`bioluminescent.jpeg` (3000×3000) is the cover of the tracked fixture album
(`examples/album_player/album.toml`), a project asset under the root MIT licence
([`LICENSE`](../../LICENSE)) — copyright (c) 2026 Samuel Sampson.

It is **tracked** because the bake's art step must be reproducible from a clone:
`scripts/album_art.py` decodes it with ffmpeg, quantises and dithers it, and
writes the generated `ART1` (≈8.8 KB at 128×128) into `examples/album_player/assets/`
(build output, gitignored). The committed source is 17,473,190 B, most of it a
Photoshop APP1 preview chain; strip that chain before any redistribution or size
review — it is not part of the image the tool reads.
