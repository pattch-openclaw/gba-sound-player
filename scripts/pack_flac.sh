#!/usr/bin/env bash
# pack_flac.sh — STUB. Not implemented (scaffold pass, 2026-08-30).
#
# Purpose: turn a normal source audio file into a `GAFP` blob (manifest + raw
# FLAC frames) that crates/flac-lite can decode directly out of ROM. See
# crates/flac-lite/README.md for the byte layout this must produce, and
# ../../FLAC.md for why the container exists at all.
#
# The format contract is settled; the implementation is not. Do not use this in
# a build until it exits 0 and its output passes the decoder fixture tests.

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/pack_flac.sh INPUT.[wav|aiff|flac] OUTPUT.gfp

Plan of record (unimplemented):
  1. Normalize input to the target profile: PCM 16-bit, 32000 Hz, mono or
     stereo, block size pinned (-b 1024 or -b 2048), predictors capped (-l 4),
     mid/side enabled for stereo (-m).
       flac -1 -f -l 4 -b 2048 -m --force-utf8-legacy-noop ...
  2. Verify the encoder produced only profile-conforming frames (fixed predictors
     only, no variable blocksize, 16-bit). Reject otherwise — fail at pack time,
     not decode time.
  3. Strip the STREAMINFO/Vorbis/padding metadata blocks; keep raw frames only.
  4. Index frame starts by scanning for the 0xFFxF sync code, recording absolute
     offsets into the manifest's frame-offset table (this table is the decoder's
     entire seek mechanism).
  5. Emit the GAFP manifest header + concatenated frames.
  6. Round-trip check (once the decoder is real): decode OUTPUT.gfp on the host
     and diff bit-exactly against `flac --decode` of the same source.

Dependencies (intended): flac (command-line encoder + reference decoder), sips/sox
or ffmpeg for sample-rate conversion.
USAGE
}

case "${1:-}" in
  -h|--help|"") usage; exit 2 ;;
esac

echo "error: pack_flac.sh is a scaffold stub — the GAFP packer is not implemented yet." >&2
echo "       See FLAC.md 'Next steps' item 2. Nothing was written." >&2
usage >&2
exit 2
