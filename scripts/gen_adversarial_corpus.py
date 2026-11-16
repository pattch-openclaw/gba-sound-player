#!/usr/bin/env python3
"""Generate the adversarial density corpus PR E's E3-a owes.

The real master does not produce the **quad-heavy** class the 129-frame fixture's
gate frame sits in (248 quads against the album's 228, and pairs are the album's
dominant group instead). A tail claim needs that class bounded too, so this
generator synthesises content chosen to *invite* it: four-sample-block structure,
impulse trains at several rates, hard-clipped harmonic-rich tones and dense
broadband bursts, at several levels. It is deliberately not music.

Writes `album.f32` (interleaved float32 stereo, PCM16 scale) and `tracks.tsv`,
exactly the pair the C encoder of record takes (`uli_enc --album … --tracks …`).

Usage:
  python3 scripts/gen_adversarial_corpus.py OUTDIR [SECONDS]
"""

import math
import os
import random
import struct
import sys

FS = 32768
CLIP = 32767.0


def block_noise(rng, n, level):
    """Noise held for four samples at a time: the 4-sample block is the point —
    it puts energy in groups of four coefficients and invites QUAD classes."""
    out = []
    for _ in range(n // 4 + 1):
        v = rng.uniform(-level, level)
        out.extend([v, v, v, v])
    return out[:n]


def impulse_train(n, period, level):
    """A click every `period` samples: broadband, sparse in time, and the fixture's
    own click section is what produced its 248-quad frame."""
    return [level if i % period == 0 else 0.0 for i in range(n)]


def clipped_tone(n, hz, level, harmonics=9):
    """Hard-clipped square-ish sum: many strong harmonics, which is what makes a
    band carry many similar-sized coefficients at once."""
    out = []
    for i in range(n):
        v = sum(math.sin(2 * math.pi * hz * h * i / FS) / h for h in range(1, harmonics + 1, 2))
        out.append(max(-level, min(level, v * level)))
    return out


def tone_pair(n, hz_a, hz_b, a, b):
    return [
        a * math.sin(2 * math.pi * hz_a * i / FS) + b * math.sin(2 * math.pi * hz_b * i / FS)
        for i in range(n)
    ]


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    seconds = int(sys.argv[2]) if len(sys.argv) > 2 else 10
    os.makedirs(out_dir, exist_ok=True)
    rng = random.Random(20261116)
    n_total = FS * seconds

    # Each segment is 1 s; the sequence walks the classes so a short corpus
    # carries all of them and the census can name which frame each came from.
    segments = []
    seg = FS
    segments.append(("block noise 8k", block_noise(rng, seg, 9000), block_noise(rng, seg, 9000)))
    segments.append(("impulses @512", impulse_train(seg, 512, 24000), impulse_train(seg, 512, 18000)))
    segments.append(("impulses @64", impulse_train(seg, 64, 20000), impulse_train(seg, 96, 16000)))
    segments.append(("clipped 6k", clipped_tone(seg, 6000, 16000), clipped_tone(seg, 6300, 15000)))
    segments.append(("clipped 11k", clipped_tone(seg, 11000, 15000), clipped_tone(seg, 11500, 14000)))
    segments.append(("tone pair", tone_pair(seg, 3000, 9000, 9000, 8000), tone_pair(seg, 3100, 9100, 8500, 7500)))
    segments.append(("block noise 15k", block_noise(rng, seg, 15000), block_noise(rng, seg, 13000)))
    segments.append(("impulses @8", impulse_train(seg, 8, 18000), impulse_train(seg, 12, 15000)))
    segments.append(("clipped 14k", clipped_tone(seg, 14000, 14000), clipped_tone(seg, 14500, 13000)))
    segments.append(("mixed", block_noise(rng, seg, 12000), impulse_train(seg, 128, 14000)))

    frames = []
    for name, l, r in segments:
        frames.extend(zip(l, r))
    frames = frames[:n_total]
    with open(os.path.join(out_dir, "album.f32"), "wb") as f:
        for l, r in frames:
            f.write(struct.pack("<ff", max(-CLIP, min(CLIP, l)), max(-CLIP, min(CLIP, r))))

    with open(os.path.join(out_dir, "tracks.tsv"), "w", encoding="ascii") as f:
        f.write(f"1\t0\t{len(frames)}\tAdversarial density\tuli-lite\n")
    print(f"wrote {out_dir}/album.f32 ({len(frames)} samples, {len(frames) / FS:.1f} s, {len(segments)} segments)")


if __name__ == "__main__":
    main()
