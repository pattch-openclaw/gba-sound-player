# ULI2 measurement-corpus metadata

`uli_cross_tracklist.json` and `uli_cross_LICENSES.md` are copied from the
sibling `gba-audio` reference project's `music/` directory so
`make uli-cross-asset` is self-contained in this repository. The audio itself is
**not** tracked: `scripts/uli_cross_album.py` downloads the CC-licensed
Internet Archive sources into the cache (`ULI_CROSS_CACHE`, default
`/tmp/uli-cross-src`) and prepares the corpus as build output.

The tracklist records each source URL, licence, processing recipe and gain. The
licence notice must travel with any distribution of the prepared corpus; read
[`uli_cross_LICENSES.md`](uli_cross_LICENSES.md) before publishing one.
