#!/usr/bin/env bash
# gen_spike_assets.sh — regenerate the perf-gate spike's embedded clips
# (examples/flac_spike/assets/*.bin + src/assets.rs).
#
# What this produces and why: FLAC.md, "Perf gate spike — concrete plan and
# validation process (2026-09-18)", PR 1. Two ~10 s arms of the same
# deterministic source — `flac -1 -l 0 -b 2048 -m` (FIXED-only) and
# `flac -1 -l 4 -b 2048 -m` (LPC) — sliced to raw frame regions with a
# hand-computed offset table (the deliberate stand-in for the GAFP manifest;
# sync-scan is dead per FLAC.md's measurements).
#
# Fail-closed layers, all inside frame_vectors.py spike_assets:
#   * the strict frame finder's stream invariants (check_stream) per encode
#   * per-frame subframe walks with the measured footer-gap rule, proving the
#     region slicing and the side-slot bps+1 seam on every frame
#   * arm censuses: -l 0 must be FIXED-only, -l 4 must be majority LPC
#     (Correction 3 — if a libFLAC upgrade breaks an arm, the FIXED-vs-LPC
#     comparison is void and the generator refuses to write)
#
# Determinism: source PCM is integer-synthesized (no floats, no RNG), so the
# WAVs are byte-identical anywhere. The *clips* still depend on libFLAC's
# per-frame choices — the generating version is recorded in assets.rs, with
# sha256 pins of every blob as the regeneration tripwire.
#
# Usage:  scripts/gen_spike_assets.sh [CRATE_DIR]
# Deps:   flac + metaflac (encode + STREAMINFO), python3 (stdlib only)

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="${1:-$here/../examples/flac_spike}"

command -v flac >/dev/null || { echo "error: flac not on PATH (brew install flac)" >&2; exit 1; }
command -v metaflac >/dev/null || { echo "error: metaflac not on PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not on PATH" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== synthesizing deterministic source PCM"
python3 "$here/frame_vectors.py" synth "$tmp"

echo "== encoding the two gate arms (same source; only -l differs)"
flac -1 -f -s -l 0 -b 2048 -m -o "$tmp/l0_stereo.flac" "$tmp/stereo.wav"
flac -1 -f -s -l 4 -b 2048 -m -o "$tmp/l4_stereo.flac" "$tmp/stereo.wav"

echo "== staging assets (strict frame finder, fail-closed arm censuses)"
python3 "$here/frame_vectors.py" spike_assets "$tmp" "$crate"

flac --version | head -1 | sed 's/^/  encoder: /'
