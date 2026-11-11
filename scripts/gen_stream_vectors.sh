#!/usr/bin/env bash
# gen_stream_vectors.sh — regenerate the stream-layer (file-level metadata)
# fixtures and their pinned expectations.
#
# Writes:
#   crates/flac-lite/tests/fixtures/*.flac   six real FLAC files, verbatim
#   crates/flac-lite/tests/stream_vectors.txt  what each one declares, as rows
#
# tests/stream_metadata_layout.rs asserts flac_lite::stream::parse (-> StreamFacts)
# against those rows (split-ladder item 4a: the `fLaC` signature, the
# metadata-block walk of RFC 9639 section 8.1, and the streaminfo block of
# section 8.2 — plus the frame-header width the following frame layer must be
# able to reach).
#
# Why the fixtures are committed binaries and the expectations are a text
# table: the point of this layer is the *container*, so the byte-for-byte file
# is the fixture (like examples/*/assets/*.bin, which are committed the same
# way). The rows are separate so they stay reviewable, and none of them comes
# from the code under test: block types/flags/lengths and every streaminfo
# field are `metaflac --list`'s own output, cross-checked at generation time
# against an independent bit-level reading of section 8 in
# scripts/stream_vectors.py; the frame rows come from frame_vectors.py's
# proven strict frame filter. Three independent readings of the same spec.
#
# What the six streams are for (their `note` rows say it in full): a two-block
# container, a four-block container with every real block type this encoder
# emits, a stream whose FIRST frame header is 80 bits (outside our packer's
# 48..=72-bit profile — a wide header is not an unstreamable stream), a valid
# 24-bit stream this decoder cannot carry, a raw-through-a-pipe encode that
# declares no total sample count, no frame sizes and no MD5, and a multi-point
# SEEKTABLE written by `metaflac --add-seekpoint`: points requested out of order
# and sorted by the tool, one request snapped down to the containing frame
# boundary, one past the end dropped, a placeholder entry, and one point naming
# a short last frame (368 samples where the declared maximum block size says
# 512).
#
# The encodes themselves live in stream_vectors.py next to the encode_command
# rows, so a row cannot drift away from the command that produced the bytes.
#
# Usage:  scripts/gen_stream_vectors.sh [FIXTURE_DIR [TABLE_OUT]]
# Deps:   flac (encode + metaflac), python3 (stdlib only)
#
# Reproducibility: the source PCM is synthesized with integer arithmetic only
# (the same helpers as frame_vectors.py), so the WAVs are byte-identical across
# machines. The fixtures depend on libFLAC's choices and therefore on its
# VERSION — the generating version is recorded in the table header. If a
# libFLAC upgrade changes an encoder choice, the generator's fail-closed
# censuses refuse to write rather than moving the goalposts under the Rust
# suite; regenerate deliberately when the change is understood.
#
# Note on the 24-bit source: Python's `wave` module writes a legacy type-1
# fmt chunk, and libFLAC warns ("legacy WAVE file has format type 1 but
# bits-per-sample=24") because 24-bit PCM is only well-defined in
# WAVEFORMATEXTENSIBLE. The warning is about our *source* container, not the
# emitted fixture: metaflac reports a plain 24-bit FLAC stream and `flac -t`
# verifies it. Left as-is rather than hand-rolling a RIFF writer.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="${1:-$here/../crates/flac-lite/tests/fixtures}"
table="${2:-$here/../crates/flac-lite/tests/stream_vectors.txt}"

command -v flac >/dev/null || { echo "error: flac not on PATH (brew install flac)" >&2; exit 1; }
command -v metaflac >/dev/null || { echo "error: metaflac not on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not on PATH" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== synthesizing deterministic source PCM (short on purpose)"
python3 "$here/stream_vectors.py" synth "$tmp"

echo "== encoding + fail-closed container censuses + staging fixtures"
python3 "$here/stream_vectors.py" emit "$tmp" "$fixtures" "$table"

echo "wrote $fixtures/*.flac"
echo "wrote $table"
flac --version | head -1 | sed 's/^/  encoder: /'
metaflac --version | head -1 | sed 's/^/  lister:  /'
