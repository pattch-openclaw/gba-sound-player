#!/usr/bin/env bash
# gen_opus_assets.sh — regenerate the Opus perf-gate spike's embedded clips
# (examples/opus_spike/assets/*.bin + src/assets.rs). OPUS.md, "Measurement
# plan — start here (2026-09-29)", step 2.
#
# Two arms of the SAME deterministic 10 s source (frame_vectors.py's integer
# synthesis — here mono at 48 kHz with PARTIALS_64K, the FLAC gate's music
# material):
#   * silk  — libopus -application voip  @ 12 kbps: the speech-first profile.
#   * music — libopus -application audio @ 96 kbps: the CELT/hybrid worst case.
# Both at the 48 kHz API rate with the 20 ms pin. DEVIATION from the plan's
# sketch (added 2026-09-30): the sketch asked the SILK arm at the 16 kHz API
# rate; measured here (and in the 2026-09-29 probe), libopus at 48 kHz voip
# stays SILK-only WB at 12 kbps, and a 48 kHz API decode is the CONSERVATIVE
# shape for the speech product — it includes the decoder-internal 16→48
# upsampler the 16 kHz API decode would skip, at the same per-packet budget.
# The TOC census (not the -application flag) is the witness that each arm is
# mode-pure; a blend fails closed and refuses to write.
#
# Determinism: the source PCM is integer-synthesized, but the *clips* depend
# on the encoder build's per-packet choices. assets.rs records the encoder
# version plus sha256 pins of every blob as the regeneration tripwire.
#
# Usage:  scripts/gen_opus_assets.sh [CRATE_DIR]
# Deps:   ffmpeg with libopus (encode + reference decode), python3 (stdlib)

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="${1:-$here/../examples/opus_spike}"

command -v ffmpeg >/dev/null || { echo "error: ffmpeg not on PATH (brew install ffmpeg)" >&2; exit 1; }
command -v python3 >/dev/null || { echo "error: python3 not on PATH" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== synthesizing deterministic source PCM (mono 48 kHz, PARTIALS_64K)"
python3 - "$tmp" "$here" <<'PY'
import os, sys
tmp, scripts = sys.argv[1], sys.argv[2]
sys.path.insert(0, scripts)
from frame_vectors import synth_pcm, write_wav, PARTIALS_64K
write_wav(os.path.join(tmp, "mono48k.wav"), 48000, 1,
          synth_pcm(10.0, 48000, PARTIALS_64K, 1024, 3, 700, 400, mono=True))
print("  wrote mono48k.wav (480000 samples, integer synthesis, no floats/RNG)")
PY

echo "== encoding the two gate arms (same source; -application/bitrate differs)"
# -compression_level 10 on both arms: pin the encoder's effort so the profile
# is stated fully at pack time rather than inheriting a build default.
ffmpeg -hide_banner -loglevel error -y -i "$tmp/mono48k.wav" \
    -c:a libopus -application voip  -frame_duration 20 -b:a 12k -vbr on -compression_level 10 \
    "$tmp/silk.opus"
ffmpeg -hide_banner -loglevel error -y -i "$tmp/mono48k.wav" \
    -c:a libopus -application audio -frame_duration 20 -b:a 96k -vbr on -compression_level 10 \
    "$tmp/music.opus"

echo "== decoding the reference PCM (ffmpeg/libopus float, host ground truth)"
# THREE things are pinned here, each measured 2026-09-30 the hard way:
#  * the OUTPUT codec: a bare -f s16le fails the muxer's codec check;
#  * the INPUT decoder (-c:a libopus BEFORE -i): ffmpeg resolves decoder by
#    codec NAME, and bare "opus" picked its NATIVE decoder — byte-identical
#    to it (sha witness), ~99.8% of samples away from libopus. Force libopus;
#  * the OUTPUT SAMPLE FORMAT: f32, not s16. The vendored decoder is a port
#    of libopus's FLOAT path; its measured equivalence class is the i16 fold
#    grid over FLOAT output (0 mismatches / 480,000 samples on both arms).
#    libopus's FIXED-point s16 decode is a different decoder profile — it
#    diverges from float on ~100% of SILK samples (max ~500 LSB, the SILK
#    float-vs-fixed resampler) — an s16 reference would compare the gate
#    against a decoder profile it never implements. The witness folds both
#    sides identically (crate checksum::fold_f32_to_i16); the reference blob
#    is raw f32-LE ground truth, pinned by FNV/sha like every other blob.
# Both arms decode at 48 kHz mono.
for arm in silk music; do
    ffmpeg -hide_banner -loglevel error -y -c:a libopus -i "$tmp/$arm.opus" \
        -c:a pcm_f32le -f f32le "$tmp/${arm}_ref.f32le"
done

echo "== staging assets (strict Ogg demux with CRC-32, fail-closed TOC censuses)"
OPUS_ENCODER_VERSION="$(ffmpeg -version 2>/dev/null | head -1)" \
    python3 "$here/opus_assets.py" opus_assets "$tmp" "$crate"
