#!/usr/bin/env python3
"""Generate S8's `fast` cell: fast.f32 + fast.tsv.

Why this file exists rather than a flag. The cell must be a *cell*, not a shortcut:
truncating an album changes the trained tables, so `--frames` on a big input would
measure a different album and, worse, rewrite the track table, which is a different
input rather than a different setting (S8 §2). A purpose-built `album.f32` +
`tracks.tsv` pair keeps the cell inside the same contract every other cell has
(E-R2: identical PCM and identical flags on both sides) and needs no new flag on
either encoder.

Why 4 s. Two hundred and fifty-six frames is long enough that the plan, the seek
table and the trained tables all have something to say, and short enough that the
whole cell — both encoders, both passes — runs in seconds, which is what makes it
the progress signal for S8-5…S8-7 rather than a fifth thing to wait for.

Why this content. The S7 diagnosis was that `tiny`'s budget does not bind and its
mismatch is therefore largely tie-breaking; a cell that cannot bind cannot diagnose
the search. So the fixture is deliberately heterogeneous — correlated stereo where
M/S and its step compensation are worth something, an uncorrelated noise section
where they are not, a silence that exercises the terminator and the all-zero band
path, transient clicks that put energy at the top of the coded spectrum, and a quiet
tail where the step search has to spend bits carefully. The section boundaries are
multiples of the hop (512) so no frame straddles a regime change for an accidental
reason.

Deterministic by construction: a fixed seed, `math.sin` on exact integer indices,
and no numpy — so the cell regenerates byte-identically from a bare checkout, which
is the same rule `gen_fixture.py` follows.

Usage: run in an empty directory; writes `fast.f32` and `fast.tsv` there.
"""

import math
import random
import struct

FS = 32768
HOP = 512
FRAMES = 256
NS = HOP * FRAMES  # 131,072 samples = 4.0 s = 256 frames
# Two tracks, so the track table has a real entry to get wrong and the weights have a
# boundary to be computed across. The split is at a hop boundary (frame 128).
SPLIT = HOP * 128
SEED = 20261123  # the day S8's plan was written; any fixed seed does, this one is dated


def sample(i: int) -> tuple[float, float]:
    t = i / FS
    if i < FS // 2:
        # Correlated stereo: an M/S win, and two tones that walk across the bands.
        l = 9000 * math.sin(2 * math.pi * 523.25 * t) + 4000 * math.sin(2 * math.pi * 1046.5 * t)
        r = 0.85 * l + 2500 * math.sin(2 * math.pi * 2093.0 * t + 0.4)
    elif i < FS:  # 0.5 … 1.0 s
        # Uncorrelated: two independent noises, so M/S must lose here.
        l = 5200 * (random.random() * 2 - 1)
        r = 4800 * (random.random() * 2 - 1)
    elif i < FS + HOP * 24:  # 1.0 … 1.375 s
        # Silence, but not digital silence: a −90 dB tone, so the frame is coded with a
        # terminator rather than skipped, and the all-zero-band path is still reachable.
        l = r = 0.4 * math.sin(2 * math.pi * 300.0 * t)
    elif i < FS * 2:  # 1.375 … 2.0 s
        # Transients: energy at the top of the coded spectrum, where q and the escape
        # path get exercised. Every 1,024 samples, so a frame sees at most one.
        k = i % 1024
        click = 18000.0 if k < 6 else 0.0
        l = click + 3500 * math.sin(2 * math.pi * 6000 * t)
        r = 0.6 * click + 3000 * math.sin(2 * math.pi * 9000 * t + 0.9)
    elif i < FS * 3:  # 2.0 … 3.0 s
        # A sweeping tone pair, close together so the M/S decision flips mid-section.
        f = 200.0 + 3000.0 * ((i - FS * 2) / FS)
        l = 8000 * math.sin(2 * math.pi * f * t)
        r = 8000 * math.sin(2 * math.pi * (f * 1.03) * t + 0.15)
    else:  # 3.0 … 4.0 s
        # A quiet tail: the step search has to spend its bits carefully rather than sit
        # at the top of the range the way the S7 `tiny` cell did.
        env = 1.0 - (i - FS * 3) / FS
        l = 900 * env * math.sin(2 * math.pi * 440 * t) + 200 * (random.random() * 2 - 1)
        r = 850 * env * math.sin(2 * math.pi * 440 * t + 0.25) + 200 * (random.random() * 2 - 1)
    clamp = lambda v: max(-32768.0, min(32767.0, v))  # noqa: E731
    return clamp(l), clamp(r)


def main() -> None:
    random.seed(SEED)
    with open("fast.f32", "wb") as f:
        for i in range(NS):
            l, r = sample(i)
            f.write(struct.pack("<ff", l, r))

    # Two tracks split at a hop boundary. The titles are ASCII and short: the writer
    # stores them verbatim, so they are part of the bytes the cell compares.
    with open("fast.tsv", "w", encoding="ascii") as f:
        f.write("# n\tstart\tlen\ttitle\tartist (32768 Hz samples)\n")
        f.write(f"1\t0\t{SPLIT}\tfast-a\tuli-enc\n")
        f.write(f"2\t{SPLIT}\t{NS - SPLIT}\tfast-b\tuli-enc\n")


if __name__ == "__main__":
    main()
