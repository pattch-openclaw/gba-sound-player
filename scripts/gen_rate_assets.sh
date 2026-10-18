#!/usr/bin/env bash
# gen_rate_assets.sh — build the rate ROM's TEMPORARY clips from a real input
# WAV at a mixer-domain sample rate (FLAC.md "Sample-rate reduction — tooling
# and profiling proposal (2026-10-11)", Part 1 steps 1+2+3).
#
# What this produces and why:
#   scripts/frame_vectors.py rate_assets — the single fail-closed stage:
#     * ffmpeg-normalized 16-bit stereo PCM at EXACTLY the requested rate
#       (convert_stereo.sh is not reusable: it emits pcm_u8, outside the
#       16-bit profile — measured fact in FLAC.md's rate proposal)
#     * the encode profile: flac -1 -l 0 -b 2048 -m and flac -1 -l 4 -b 2048 -m
#       (the spike's two arms; same flags the committed clips used)
#     * fail-closed censuses before anything is written: rate in the mixer
#       domain {10512, 18157, 32768}; every FRAME (not just STREAMINFO)
#       resolves to the requested rate on the 0b1101 path — encode rate ==
#       playback rate; profile walk with the measured footer-gap rule; the
#       -l 4 arm must stay majority LPC (real-material rule, rate_census)
#
# Outputs are TEMPORARY, like ROM files: gitignored, removed by `make clean`.
# The committed spike assets (assets/spike_*.bin, src/assets.rs) are the
# E-menu's calibration anchor and are deliberately NOT touched — the default
# spike ROM must keep reproducing them byte-for-byte.
#
# Usage:  scripts/gen_rate_assets.sh INPUT_WAV RATE [CRATE_DIR]
# Deps:   ffmpeg + flac + metaflac + python3 (stdlib only)

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
input="${1:-}"
rate="${2:-}"
crate="${3:-$here/../examples/flac_spike}"

if [[ -z "$input" || -z "$rate" ]]; then
  echo "usage: $0 INPUT_WAV RATE [CRATE_DIR]" >&2
  echo "  RATE must be one of the mixer domain: 10512 18157 32768" >&2
  exit 1
fi
if [[ ! -f "$input" ]]; then
  echo "error: input not found: $input" >&2
  exit 1
fi
case "$rate" in
  10512 | 18157 | 32768) ;;
  *)
    echo "error: RATE=$rate outside the mixer domain {10512, 18157, 32768}." >&2
    echo "  agb 0.25.0's Frequency has exactly these three (FLAC.md rate" >&2
    echo "  proposal, measured readiness facts). Arbitrary rates are" >&2
    echo "  unplayable until Goal #5's custom mixer exists. Refusing." >&2
    exit 1
    ;;
esac

command -v ffmpeg >/dev/null || { echo "error: ffmpeg not on PATH" >&2; exit 1; }
command -v flac >/dev/null || { echo "error: flac not on PATH (brew install flac)" >&2; exit 1; }
command -v metaflac >/dev/null || { echo "error: metaflac not on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not on PATH" >&2; exit 1; }

# Input provenance sha — recorded into the generated module (the measured
# image carries the sha of the file it was built from). shasum first
# (macOS), sha256sum fallback (Linux container).
if command -v shasum >/dev/null; then
  input_sha="$(shasum -a 256 "$input" | awk '{print $1}')"
else
  input_sha="$(sha256sum "$input" | awk '{print $1}')"
fi
ffmpeg_version="$(ffmpeg -version 2>/dev/null | head -1)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== normalizing to 16-bit stereo pcm_s16le at exactly ${rate} Hz"
ffmpeg -v error -y -i "$input" -ac 2 -ar "$rate" -c:a pcm_s16le "$tmp/input.wav"

echo "== encoding the two gate arms (same normalized source; only -l differs)"
flac -1 -f -s -l 0 -b 2048 -m -o "$tmp/rate_l0.flac" "$tmp/input.wav"
flac -1 -f -s -l 4 -b 2048 -m -o "$tmp/rate_l4.flac" "$tmp/input.wav"

echo "== staging rate assets (strict frame finder, fail-closed censuses)"
python3 "$here/frame_vectors.py" rate_assets "$tmp" "$crate" "$rate" \
  "$input_sha" "$input" "$ffmpeg_version"

flac --version | head -1 | sed 's/^/  encoder: /'
echo "rate assets staged (TEMPORARY: gitignored, removed by make clean)"
