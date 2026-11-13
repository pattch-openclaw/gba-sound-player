#!/usr/bin/env python3
"""Generate the ULI2 test fixture source: tiny.f32 + tiny.tsv.

The committed .uli is produced from these by the gba-audio C encoder; see
README.md in this directory. Pure Python, no numpy, so the fixture is
reproducible from a bare checkout.
"""
import math
import random
import struct

FS = 32768
NS = FS * 2


def main() -> None:
    random.seed(12345)
    with open("tiny.f32", "wb") as f:
        for i in range(NS):
            t = i / FS
            if t < 0.5:
                l = 12000 * math.sin(2 * math.pi * 440 * t) + 4000 * math.sin(2 * math.pi * 660 * t)
                r = 11000 * math.sin(2 * math.pi * 440 * t + 0.3) - 3000 * math.sin(2 * math.pi * 880 * t)
            elif t < 0.9:
                l = r = 0.0
            elif t < 1.4:
                l = 5000 * random.uniform(-1, 1) + 6000 * math.sin(2 * math.pi * 3000 * t)
                r = 4500 * random.uniform(-1, 1) + 5000 * math.sin(2 * math.pi * 3000 * t + 0.7)
            else:
                k = int((t - 1.4) * FS)
                click = 20000.0 if k % 4096 < 8 else 0.0
                l = click + 3000 * math.sin(2 * math.pi * 11000 * t)
                r = click * 0.7 + 2500 * math.sin(2 * math.pi * 14000 * t + 0.2)
            f.write(struct.pack("<ff", max(-32768, min(32767, l)), max(-32768, min(32767, r))))

    with open("tiny.tsv", "w", encoding="ascii") as f:
        f.write(f"1\t0\t{NS}\tTiny Fixture\tuli-lite\n")


if __name__ == "__main__":
    main()
