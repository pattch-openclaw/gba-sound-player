#!/usr/bin/env bash
# pack_flac.sh — the profile-driven front-end. Still not implemented
# (scaffold pass, 2026-08-30; this header corrected 2026-11-11).
#
# Purpose: turn a normal source audio file into a `GAFP` blob (manifest + raw
# FLAC frames) that crates/flac-lite can decode directly out of ROM. See
# crates/flac-lite/README.md for the byte layout this must produce, and
# ../../FLAC.md for why the container exists at all.
#
# What HAS landed since this stub was written (6b-2 pt 1): the format contract
# itself is implemented and gated. `scripts/gafp_pack.py` packs a real `.flac`
# into a `.gfp` — frame region byte-identical, offset table re-derived by a walk
# that reads every frame, refuses before writing anything when a stream is not on
# the measured decode path — and `make gafp-pack-check` proves it rebuilds the
# committed product asset byte for byte. That script is the thing under test, so
# it takes an already-encoded `.flac` and asserts the profile rather than
# establishing one.
#
# What this script would be, and still is not: the operator-facing front-end —
# take a `.wav`, normalize it to the target profile (the `-l` / `-b` / `--rate` /
# `--lax` rules in the usage text below, which are measurements rather than
# guesses now), encode, pack, and refuse out loud when the profile cannot be met.
# Those are two different jobs — one asserts a stream against the decode path, the
# other negotiates a stream into it — which is why the wrapper was not bolted onto
# the packer when the packer landed. See flac-docs/plan-of-record.md item 6b-2.
#
# Until then: nothing in any build or gate calls this script, and it changes no
# bytes. `flac-rom`'s asset comes from `scripts/gen_gafp_blob.py`; the packer's
# equality with that asset is what `gafp-pack-check` holds.

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/pack_flac.sh INPUT.[wav|aiff|flac] OUTPUT.gfp

Plan of record (unimplemented; corrected 2026-09-07 after measuring real
encodes — see FLAC.md "Frame header: measured byte layout"):
  1. Normalize input to the target profile: PCM 16-bit, 32000 Hz, mono or
     stereo, block size pinned (-b 1024 or -b 2048), max predictor order capped
     (-l 4; use -l 0 for the FIXED-only arm), mid/side tried for stereo (-m).
       flac -1 -f -l 4 -b 2048 -m ...
     (-l is --max-lpc-order, NOT a fixed-predictor cap: -l 4 emits real LPC-4
     frames, -l 0 is what means FIXED-only. An earlier revision of this plan
     also carried `--force-utf8-legacy-noop`, which is not a libFLAC option at
     all — 1.5.0 exits 1 with "unrecognized option".)
     For 65536 Hz add --lax (outside FLAC's streamable subset); every frame then
     carries sample-rate code 0b0000 = "from stream", and the manifest must carry
     the literal rate because the frames no longer do.
  2. Verify the encoder produced only profile-conforming frames: predictor order
     <= the profile cap (order > 4, not "is LPC"), fixed blocking strategy, no
     variable blocksize, 16-bit. Reject otherwise -- fail at pack time, not
     decode time.
  3. Strip the STREAMINFO/Vorbis/padding metadata blocks; keep raw frames only.
  4. Index frame starts by STRICT frame-header parse + CRC-8 (scripts/
     frame_vectors.py already implements exactly this walk, and its `measure`
     subcommand proves it exact over 979 real frames). Do NOT index by bare
     0xFFxF sync scan: measured over-match is 1.1x-9.4x depending on the audio,
     and CRC-8 alone still admits 1-2 false positives per stream. Recording
     absolute offsets into the manifest's frame-offset table -- this table is the
     decoder's entire seek mechanism.
     Frame count is ceil(total_samples / blocksize) and the LAST frame is
     legitimately shorter than the track blocksize (e.g. 156 x 2048 + 1 x 512),
     so never filter candidates on "blocksize == track blocksize" -- that drops
     the last frame of every track.
  5. Emit the GAFP manifest header + concatenated frames.
  6. Round-trip check (once the decoder is real): decode OUTPUT.gfp on the host
     and diff bit-exactly against `flac --decode` of the same source.

Dependencies (intended): flac (command-line encoder + reference decoder), sips/sox
or ffmpeg for sample-rate conversion. scripts/frame_vectors.py (Python 3, stdlib
only) is the existing reference for frame walking, header parsing, CRC-8, and
deterministic test-PCM synthesis -- reuse it rather than rewriting the walk.
USAGE
}

case "${1:-}" in
  -h|--help|"") usage; exit 2 ;;
esac

echo "error: pack_flac.sh is still a scaffold stub — it normalizes and packs nothing," >&2
echo "       and it wrote no file. The GAFP format contract itself is implemented" >&2
echo "       and gated: scripts/gafp_pack.py (make gafp-pack-check). This script's" >&2
echo "       job is the step before it — see flac-docs/plan-of-record.md item 6b-2." >&2
usage >&2
exit 2
