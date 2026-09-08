#!/usr/bin/env bash
# gen_frame_vectors.sh — regenerate the frame-header golden vectors.
#
# Writes crates/flac-lite/tests/frame_header_vectors.txt: real frame headers
# captured from libFLAC encodes, with expected field values derived by an
# INDEPENDENT parser (scripts/frame_vectors.py), never from libFLAC itself.
# tests/frame_header_layout.rs asserts flac_lite::bits::BitReader agrees.
#
# Why real bytes and not hand-packed ones: hand-packed headers are how the
# 31-vs-32-bit field-width bug got baked into the scaffold (see FLAC.md
# "Frame header: measured byte layout"). Real encoder output cannot be wrong
# about what encoders emit.
#
# Usage:  scripts/gen_frame_vectors.sh [OUTPUT]
# Deps:   flac (encode + metaflac), python3 (stdlib only)
#
# Reproducibility: the source audio is synthesized deterministically (a fixed
# formula, no RNG), so the PCM is byte-identical across machines and runs. The
# emitted *vectors* depend on libFLAC's frame choices and therefore on its
# VERSION — the generating version is recorded in the file header. If a libFLAC
# upgrade changes them, regenerate; the expected values are recomputed from the
# RFC layout, so they stay honest either way.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="${1:-$here/../crates/flac-lite/tests/frame_header_vectors.txt}"

command -v flac >/dev/null || { echo "error: flac not on PATH (brew install flac)" >&2; exit 1; }
command -v metaflac >/dev/null || { echo "error: metaflac not on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not on PATH" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== synthesizing deterministic source PCM"
python3 "$here/frame_vectors.py" synth "$tmp"

echo "== encoding (each vector's encode profile is recorded in the output)"
# -m: allow mid/side. -b: pin blocksize. -l: max predictor order (0 => FIXED only).
flac -1 -f -s -l 4 -b 2048 -m -o "$tmp/l4_stereo.flac"     "$tmp/stereo.wav"
flac -1 -f -s -l 0 -b 2048 -m -o "$tmp/l0_stereo.flac"     "$tmp/stereo.wav"
flac -1 -f -s -l 4 -b 1024    -o "$tmp/l4_mono.flac"       "$tmp/mono.wav"
flac -1 -f -s -l 4 -b 2048 -m -o "$tmp/l4_silence.flac"    "$tmp/silence.wav"
# 65536 Hz is outside FLAC's streamable subset (no 4-bit code for it), so
# libFLAC needs --lax; every frame then says "rate from stream" (code 0).
flac -1 -f -s -l 4 -b 2048 -m --lax -o "$tmp/r65k.flac"    "$tmp/r65k.wav"

echo "== extracting vectors with the independent parser"
python3 "$here/frame_vectors.py" emit "$tmp" "$out"

echo "wrote $out"
flac --version | head -1 | sed 's/^/  encoder: /'
