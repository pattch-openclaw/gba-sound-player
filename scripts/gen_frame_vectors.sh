#!/usr/bin/env bash
# gen_frame_vectors.sh — regenerate the frame-header + subframe-body golden vectors.
#
# Writes crates/flac-lite/tests/frame_header_vectors.txt: real frame headers
# captured from libFLAC encodes, with expected field values derived by an
# INDEPENDENT parser (scripts/frame_vectors.py), never from libFLAC itself.
# tests/frame_header_layout.rs asserts flac_lite::bits::BitReader agrees.
#
# Also writes crates/flac-lite/tests/subframe_body_vectors.txt: whole real
# frames (one per subframe type) with per-subframe ground truth from the
# independent Bits walk + reference-decoder PCM — tests/
# subframe_body_layout.rs decodes them with decode_subframe (step 3e).
#
# Why real bytes and not hand-packed ones: hand-packed headers are how the
# 31-vs-32-bit field-width bug got baked into the scaffold (see FLAC.md
# "Frame header: measured byte layout"). Real encoder output cannot be wrong
# about what encoders emit.
#
# Usage:  scripts/gen_frame_vectors.sh [HEADER_OUT [BODY_OUT]]
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
body_out="${2:-$here/../crates/flac-lite/tests/subframe_body_vectors.txt}"

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
# Wasted-bits witness: coarse square wave (zero LSBs by construction) makes
# libFLAC signal wasted bits in the subframe header. -l 0 keeps it FIXED, so
# the vector witnesses the type field AND the wasted flag that follows it.
flac -1 -f -s -l 0 -b 2048    -o "$tmp/wasted_square.flac" "$tmp/wasted_square.wav"
# 65536 Hz is outside FLAC's streamable subset (no 4-bit code for it), so
# libFLAC needs --lax; every frame then says "rate from stream" (code 0).
flac -1 -f -s -l 4 -b 2048 -m --lax -o "$tmp/r65k.flac"    "$tmp/r65k.wav"
# Final frames whose length is not a table block size force the uncommon
# ("get 8/16-bit") blocksize form -- the only way those frames can be carried.
flac -1 -f -s -l 4 -b 2048    -o "$tmp/tail16.flac"        "$tmp/tail16.wav"
flac -1 -f -s -l 4 -b 2048    -o "$tmp/tail8.flac"         "$tmp/tail8.wav"
# Rates absent from the 4-bit table travel as uncommon sample-rate codes.
# Unlike 65536 Hz these do NOT need --lax (measured, libFLAC 1.5.0).
flac -1 -f -s -l 4 -b 1024    -o "$tmp/rate_khz.flac"      "$tmp/rate_khz.wav"
flac -1 -f -s -l 4 -b 1024    -o "$tmp/rate_hz.flac"       "$tmp/rate_hz.wav"
flac -1 -f -s -l 4 -b 1024    -o "$tmp/rate_hz10.flac"     "$tmp/rate_hz10.wav"
# VERBATIM witness (header-table vector): incompressible Knuth-hash PCM at
# -l 0 encodes VERBATIM on every frame (measured, libFLAC 1.5.0; the same
# source at -l 12 yields zero verbatim — -l 0 is the switch). The fail-closed
# census in frame_vectors.py refuses to write the table if that ever changes.
flac -1 -f -s -l 0 -b 2048    -o "$tmp/verbatim.flac"      "$tmp/verbatim.wav"
# Subframe-body vectors (step 3e): one 256-sample mono stream per type, so
# each stream is exactly one frame and its census IS the frame's type.
flac -1 -f -s -l 0 -b 256     -o "$tmp/verbatim256.flac"   "$tmp/verbatim256.wav"
flac -1 -f -s -l 0 -b 256     -o "$tmp/constant256.flac"   "$tmp/constant256.wav"
flac -1 -f -s -l 0 -b 256     -o "$tmp/square256.flac"     "$tmp/square256.wav"
flac -1 -f -s -l 4 -b 256     -o "$tmp/tonal256.flac"      "$tmp/tonal256.wav"
flac -1 -f -s -l 4 -b 256     -o "$tmp/lpcw256.flac"       "$tmp/lpcw256.wav"
flac -1 -f -s -l 4 -b 256     -o "$tmp/smooth256.flac"     "$tmp/smooth256.wav"

echo "== extracting vectors with the independent parser"
python3 "$here/frame_vectors.py" emit "$tmp" "$out"
python3 "$here/frame_vectors.py" emit_bodies "$tmp" "$body_out"

echo "wrote $out"
echo "wrote $body_out"
flac --version | head -1 | sed 's/^/  encoder: /'
