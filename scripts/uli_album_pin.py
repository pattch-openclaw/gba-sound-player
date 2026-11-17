#!/usr/bin/env python3
"""Emit the ULI2 album-corpus pin module from the C reference decoder's output.

PR E's corpus is real music at the product configuration (32,768 Hz, hop 512,
`--cap 74`, budget scaled to ~102 kbit/s). This script turns the *C reference
decoder's* own output on that blob into the pin the bench ROM checks itself
against, so the pin is C-sourced rather than a transcript of the decoder under
test:

  * `EXPECTED_INT8` is the C decoder's `hash_of_frame_hashes` from
    `vectors.txt` — FNV-1a 32 of the little-endian per-frame int8 hashes, which
    is exactly the fold `examples/uli_bench` performs (`fnv_bytes(all, &hash)`).
  * The reconstruction below is a *witness that we read the container the same
    way the C decoder does*: it rebuilds the album timeline from the per-track
    `s8/NN.s8` files, re-hashes frames, and refuses to emit a pin unless every
    checkpoint `vectors.txt` publishes (frames 0..63 and every 4096th) matches.

Nothing here is an oracle of its own: the authority is the C decode.

Usage:
  python3 scripts/uli_album_pin.py --blob <album.uli> --ref <uli_dec OUTDIR> \\
      --rs examples/uli_bench/src/album_pin.rs

The emitted file is gitignored (ALBUM-PLAYER.md §2.3's division: the input is
tracked, the blob and its pins are build output).
"""

import argparse
import os
import struct
import sys

M = 512          # hop: samples per channel per frame
FNV_OFFSET = 2166136261
FNV_PRIME = 0x01000193


def u32(blob, off):
    return struct.unpack_from("<I", blob, off)[0]


def fnv1a32(h, data):
    for b in data:
        h ^= b
        h = (h * FNV_PRIME) & 0xFFFFFFFF
    return h


def frame_hash_i8(stream, frame):
    """FNV over 512 L bytes then 512 R bytes, the C decoder's per-frame fold."""
    # Frame k carries samples (k-1)*512 .. k*512 (frame 0 is the preroll), and
    # the track files are interleaved L,R int8.
    base = (frame - 1) * 2 * M
    h = fnv1a32(FNV_OFFSET, stream[base:base + 2 * M:2])
    return fnv1a32(h, stream[base + 1:base + 2 * M:2])


def read_vectors(path):
    frames = None
    agg = None
    checkpoints = {}
    with open(path, encoding="ascii") as f:
        for line in f:
            p = line.split()
            if not p or p[0].startswith("#"):
                continue
            if p[0] == "frames":
                frames = int(p[1])
            elif p[0] == "hash_of_frame_hashes":
                agg = int(p[1], 16)
            elif p[0] == "frame":
                checkpoints[int(p[1])] = int(p[2], 16)
    if frames is None or agg is None:
        sys.exit(f"{path}: no 'frames' / 'hash_of_frame_hashes' line")
    return frames, agg, checkpoints


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--blob", required=True)
    ap.add_argument("--ref", required=True, help="the C decoder's OUTDIR (has vectors.txt and s8/)")
    ap.add_argument("--rs", required=True)
    args = ap.parse_args()

    blob = open(args.blob, "rb").read()
    n_frames = u32(blob, 16)
    n_samples = u32(blob, 20)
    n_tracks = u32(blob, 24)
    tracks_off = u32(blob, 28)
    rate = u32(blob, 8)
    if rate != 32768:
        sys.exit(f"{args.blob}: sample rate {rate}, expected 32768")

    frames, agg, checkpoints = read_vectors(os.path.join(args.ref, "vectors.txt"))
    if frames != n_frames:
        sys.exit(f"C decoder reports {frames} frames, header says {n_frames}")

    # Rebuild the album timeline from the per-track 8-bit files (C layout:
    # track entry = start_sample, n_samples, preroll_frame, g_prev, skip, ...).
    timeline = bytearray(2 * n_samples)
    covered = bytearray(n_samples)
    for t in range(n_tracks):
        off = tracks_off + 24 * t
        start = u32(blob, off)
        count = u32(blob, off + 4)
        if start + count > n_samples:
            sys.exit(f"track {t + 1}: {start}+{count} past n_samples {n_samples}")
        path = os.path.join(args.ref, "s8", f"{t + 1:02d}.s8")
        data = open(path, "rb").read()
        if len(data) != 2 * count:
            sys.exit(f"{path}: {len(data)} B, expected {2 * count}")
        timeline[2 * start:2 * (start + count)] = data
        covered[start:start + count] = b"\x01" * count
    if not all(covered):
        sys.exit("track files do not cover the album timeline")

    # Per-frame hashes the stream can produce: frame k covers samples
    # (k-1)*512 .. k*512, so k>=1 needs k*512 <= n_samples.
    full_frames = min(n_frames - 1, n_samples // M)      # highest frame index fully in the stream

    # Witness: every checkpoint the C decoder published must reconstruct exactly.
    # Frame 0 is the preroll buffer and is deliberately not part of the stream,
    # so a published frame-0 hash is consistent with the C decoder's own buffer,
    # not with the timeline; it is the one checkpoint we can only note.
    checked = skipped = 0
    for k, want in sorted(checkpoints.items()):
        if k == 0 or k > full_frames:
            skipped += 1
            continue
        got = frame_hash_i8(timeline, k)
        if got != want:
            sys.exit(f"frame {k}: reconstructed {got:08x}, C decoder says {want:08x}")
        checked += 1
    # Every published checkpoint the *stream* can produce must have matched (a
    # mismatch already exits above). The guard is that at least one did: a short
    # corpus publishes only frames 0..63, so a fixed "64 checked" floor would
    # refuse pins for any corpus shorter than ~4 s.
    if checked == 0:
        sys.exit("no checkpoint was reconstructable — refusing to emit a pin")

    # The aggregate itself is the C decoder's, not ours: it is the only value
    # that covers frame 0 and the partial tail frame.
    src = os.path.relpath(args.blob, os.path.dirname(os.path.dirname(os.path.dirname(args.rs))))
    body = f"""// GENERATED by scripts/uli_album_pin.py from the C reference decoder — do not edit.
//
// Corpus: {src} ({len(blob)} B)
//   {n_frames} frames, {n_samples} samples ({n_samples / 32768.0:.2f} s), {n_tracks} track(s), 32,768 Hz
//
// EXPECTED_INT8 is the C decoder's own `hash_of_frame_hashes` from its
// vectors.txt (FNV-1a 32 over the little-endian per-frame int8 hashes, frames
// 0..{n_frames - 1} inclusive of the frame-0 preroll). It is an oracle value,
// not a transcript: the C decoder produced it, and this script additionally
// re-hashed {checked} published per-frame checkpoints from the C decoder's 8-bit
// track output and required every one to match ({skipped} checkpoint(s) not
// reconstructable from the stream: the preroll frame and past the last full
// frame).

pub const N_FRAMES: u32 = {n_frames};
pub const EXPECTED_INT8: u32 = 0x{agg:08x};
"""
    with open(args.rs, "w", encoding="utf-8") as f:
        f.write(body)
    print(f"album pin: {n_frames} frames, {n_samples} samples, int8={agg:08x} "
          f"({checked} checkpoints verified) -> {args.rs}")


if __name__ == "__main__":
    main()
