#!/usr/bin/env python3
"""Regenerate the forward-MDCT golden: `crates/uli-enc/tests/fixtures/mdct_*`.

Two files, both small, both the **C reference's own bytes**:

  mdct_in.f32        the test album — 2,048 interleaved stereo f32 frames,
                     generated deterministically below (no numpy, no rand): an
                     integer LCG and `math.sin`, so the input is reproducible on
                     any host and is itself part of the witness.
  mdct_ref_coef.f32  `mdct_frame()`'s output for frames 0..3 of that album,
                     4 x 1024 little-endian f32 (left 512 then right 512).

`uli_enc::mdct` must reproduce the second from the first, bit for bit. That is
E-R5 rule R5c's witness and S2's whole premise: the checked-in transform
constants plus the ported arithmetic produce the reference's coefficients, not
approximately them. Frame 0 is the preroll (the window starts at -M, so half the
input is out of bounds and reads as silence); frame 3 is interior. Both edges are
deliberate.

Usage:  scripts/gen_uli_mdct_fixture.py            # GBA_AUDIO= overrides the location
Requires a C compiler and $GBA_AUDIO/encoder/enc_core.c. Nothing in the repository
builds that C for a gate — E-R1 forbids the dependency, not the oracle.
"""

import math
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

NS = 2048  # interleaved stereo frames in the test album
FRAMES = 4  # how many to freeze coefficients for
M = 512
NCODED = 500

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "crates/uli-enc/tests/fixtures"
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))

PROBE = r"""
#include <stdio.h>
#include <stdlib.h>
#include "enc_core.h"
int main(int argc, char **argv) {
    long long ns = atoll(argv[2]);
    FILE *f = fopen(argv[1], "rb"); if (!f) { perror(argv[1]); return 1; }
    float *x = malloc((size_t)ns * 2 * sizeof(float));
    if (fread(x, sizeof(float), (size_t)ns * 2, f) != (size_t)ns * 2) return 1;
    fclose(f);
    mdct_init();
    FILE *o = fopen(argv[3], "wb");
    static float coef[2 * ULI_M];
    for (long long fr = 0; fr < atoll(argv[4]); fr++) {
        mdct_frame(x, ns, fr, coef);
        fwrite(coef, sizeof(float), 2 * ULI_M, o);
    }
    fclose(o);
    return 0;
}
"""


def album() -> bytes:
    """The deterministic test album, written exactly as `album.f32` wants it.

    Dense on purpose: a tonal chirp plus broad-band noise on both channels, so
    every band up to `NCODED` carries energy and the quantiser has to make real
    class choices rather than code silence. The two channels are correlated but
    not equal, which is what M/S will later have to reason about.
    """
    state = 12345
    out = bytearray()
    for i in range(NS):
        # xorshift32: integer-only, so it is exactly reproducible anywhere.
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        noise_l = (state & 0xFFFF) / 65535.0 - 0.5
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        noise_r = (state & 0xFFFF) / 65535.0 - 0.5
        chirp = math.sin(2.0 * math.pi * (300.0 * i + 3.0 * i * i / NS))
        l = 9000.0 * chirp + 6000.0 * noise_l
        r = 6000.0 * math.sin(2.0 * math.pi * (500.0 * i - 2.0 * i * i / NS)) + 6000.0 * noise_r
        l = max(-32767.0, min(32767.0, l))
        r = max(-32767.0, min(32767.0, r))
        out += struct.pack("<ff", l, r)
    return bytes(out)


def main() -> int:
    enc = GBA_AUDIO / "encoder"
    if not (enc / "enc_core.c").exists():
        print(f"gen_uli_mdct_fixture: no reference encoder at {enc} (set GBA_AUDIO=)", file=sys.stderr)
        return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    in_path = OUT_DIR / "mdct_in.f32"
    ref_path = OUT_DIR / "mdct_ref_coef.f32"
    in_path.write_bytes(album())

    cc = os.environ.get("CC", "cc")
    flags = os.environ.get("CFLAGS", "-O2 -march=native -std=gnu11 -w").split()
    with tempfile.TemporaryDirectory() as td:
        probe = Path(td) / "probe.c"
        probe.write_text(PROBE)
        exe = Path(td) / "probe"
        subprocess.run(
            [cc, *flags, "-I", str(enc), "-o", str(exe), str(probe),
             str(enc / "enc_core.c"), str(GBA_AUDIO / "src/uli/uli_tables.c"), "-lm"],
            check=True,
        )
        subprocess.run([str(exe), str(in_path), str(NS), str(ref_path), str(FRAMES)], check=True)

    n = ref_path.stat().st_size
    expect = FRAMES * 2 * M * 4
    if n != expect:
        print(f"gen_uli_mdct_fixture: got {n} bytes, expected {expect}", file=sys.stderr)
        return 1
    print(f"wrote {in_path} ({in_path.stat().st_size} B) and {ref_path} ({n} B, frames 0..{FRAMES - 1})")
    # A coefficient past NCODED would mean the fixture does not exercise the rule.
    tail = ref_path.read_bytes()
    bad = sum(1 for off in range(NCODED * 4, M * 4, 4) for _ in [0] if tail[off:off + 4] != b"\0\0\0\0")
    print(f"reference zero-coefficients beyond NCODED check: {bad} nonzero (must be 0)")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
