#!/usr/bin/env python3
"""Freeze `band_rdo`'s answers from the C reference, for a differential unit test.

`band_rdo` is the one function in the encoder whose behaviour is a pile of small
decisions — three candidates per coefficient, a joint price over each tuple, an escape
and a raw field and a sign, a clipped symbol head, a zero-length-code guard. Reading it
against the C found one dropped guard (the `if (!l) continue;`); reading catches what
reading can catch, and the rest needs a differential. This is the same instrument that
found the tables-blob row leak and proved the MDCT bit-exact: freeze the oracle's
answers, then assert ours match.

The probe `#include`s `enc_core.c` so it can reach the function, which is `static`. That
keeps the dependency off-tree and out of every gate (E-R1 forbids the dependency, not
the oracle), exactly as `gen_uli_mdct_fixture.py` and `gen_uli_default_tables.sh` do.

Line format, one case per line, all integers so nothing depends on float printing:

    x0..x15 : f32 bits, hex          (the band's 16 coefficients)
    class   : decimal                 the class index
    g       : decimal                 the step index (d = uli_step[g]/16, kappa 0.13)
    mmax    : decimal                 the magnitude clamp
    ncod    : decimal                 how many coefficients may be nonzero
    dist    : f64 bits, hex           the reference's total squared error, recovered
                                      from the magnitudes it chose (band_rdo returns
                                      J = D + lam*bits, so D is not its return value)
    bits    : decimal                 the reference's bit count for the band
    m0..m15 : decimal                 the magnitudes it chose

Usage:  scripts/gen_uli_band_rdo_fixture.py        (GBA_AUDIO= overrides the location)
Regenerate with `make uli-band-rdo-fixture`.
"""

import math
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "crates/uli-enc/tests/fixtures/band_rdo.txt"
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))
CASES = 2400
KAPPA = 0.13

PROBE = r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include "enc_core.c"   /* for the static band_rdo, which is the point */

#define KAPPA 0.13

int main(int argc, char **argv) {
    tables_default(&T);
    FILE *o = fopen(argv[1], "w");
    if (!o) { perror(argv[1]); return 1; }
    unsigned s = 0x1234567u;
    const int classes[] = {1,2,3,4,5,6,7,8,9,10,11,12,13};
    const int nc = (int)(sizeof classes / sizeof classes[0]);
    for (int it = 0; it < atoi(argv[2]); it++) {
        s ^= s << 13; s ^= s >> 17; s ^= s << 5;   /* xorshift32 */
        int g = 8 + (int)((s >> 3) % 180);
        int class = classes[(s >> 11) % nc];
        int ncod = ((s >> 17) & 1) ? 16 : (int)(((s >> 18) % 16) + 1);
        double d = uli_step[g] / 16.0;
        double lam = KAPPA * d * d;
        long long mmax = ((1LL << 28) / (long long)uli_step[g]) / 16 - 1;
        if (mmax > 65535) mmax = 65535;
        if (mmax < 1) mmax = 1;
        float x[16];
        int mode = (s >> 25) % 5;
        for (int i = 0; i < 16; i++) {
            s ^= s << 13; s ^= s >> 17; s ^= s << 5;
            double u = (double)(s & 0xFFFFFF) / 16777216.0;
            double mag;
            switch (mode) {
                case 0: mag = 0.0; break;                       /* all zero */
                case 1: mag = u * 0.4 * d; break;               /* all sub-step: the zeroing case */
                case 2: mag = u * 3.5 * d; break;               /* straddles the candidate edges */
                case 3: mag = u * 40.0 * d; break;              /* escapes */
                default: mag = (u < 0.5) ? 0.0 : u * 500.0 * d; break; /* sparse */
            }
            x[i] = (float)((s & 1) ? -mag : mag);
        }
        int mo[16];
        double bo = 0;
        /* `band_rdo` returns J = D + lam*bits, NOT D — the fixture has to recover D
         * from the magnitudes it chose, or the "dist" column would be the Lagrangian
         * and the differential would fail on arithmetic that agrees. (It did: that was
         * the first thing this fixture caught, in the probe rather than the code.) */
        double J = band_rdo(x, class, d, (int)mmax, ncod, lam, mo, &bo, NULL);
        if (!isfinite(J)) { it--; continue; }
        double D = 0;
        for (int i = 0; i < 16; i++) {
            double ax = fabs(x[i]);
            double rec = rec_of(class, mo[i]) * d;
            double e = ax - rec;
            D += e * e;
        }
        for (int i = 0; i < 16; i++) {
            unsigned b; memcpy(&b, &x[i], 4); fprintf(o, "%08x ", b);
        }
        fprintf(o, "%d %d %d %d ", class, g, (int)mmax, ncod);
        { unsigned long long db; memcpy(&db, &D, 8); fprintf(o, "%016llx ", db); }
        fprintf(o, "%d ", (int)bo);
        for (int i = 0; i < 16; i++) fprintf(o, "%d%s", mo[i], i == 15 ? "\n" : " ");
    }
    fclose(o);
    return 0;
}
'''


def main() -> int:
    enc = GBA_AUDIO / "encoder"
    if not (enc / "enc_core.c").exists():
        print(f"gen_uli_band_rdo_fixture: no reference encoder at {enc}", file=sys.stderr)
        return 1
    cc = os.environ.get("CC", "cc")
    flags = os.environ.get("CFLAGS", "-O2 -std=gnu11 -w").split()
    with tempfile.TemporaryDirectory() as td:
        probe = Path(td) / "probe.c"
        probe.write_text(PROBE)
        exe = Path(td) / "probe"
        subprocess.run(
            [cc, *flags, "-I", str(enc), "-o", str(exe), str(probe),
             str(GBA_AUDIO / "src/uli/uli_tables.c"), "-lm"],
            check=True,
        )
        OUT.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([str(exe), str(OUT), str(CASES)], check=True)

    lines = OUT.read_text().splitlines()
    # A fixture that collapsed to one behaviour would pass while testing nothing.
    classes = {int(l.split()[16]) for l in lines}
    kinds = {int(l.split()[17]) for l in lines}
    if len(lines) < CASES // 2 or len(classes) < 8 or len(kinds) < 20:
        print(f"gen_uli_band_rdo_fixture: {len(lines)} lines, {len(classes)} classes, "
              f"{len(kinds)} steps — too narrow to witness anything", file=sys.stderr)
        return 1
    print(f"wrote {OUT} ({len(lines)} cases, {len(classes)} classes, {len(kinds)} steps)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
