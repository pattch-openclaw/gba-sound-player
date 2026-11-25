#!/usr/bin/env python3
"""Check that the oracle the encoder is measured against has not changed shape.

Why this exists. Byte-exactness is a claim about *a binary*, not about a format: the
reference encoder's search compares floats (`w·D + λ·R` in `allocate`, the λ bisection,
`g_guess`'s `log2`, the trainer's `sqrt`/`pow`), so its answers depend on how that C was
compiled. Two things make that invisible until it bites:

  * `-march=native` in `gba-audio/encoder/Makefile`, which pins the oracle to the ISA of
    the machine that built it, and
  * GCC's default `-ffp-contract=fast`, which fuses `a*b + c` into one FMA wherever the
    ISA has one.

**Measured 2026-11-25 on this tree's aarch64 box, and the finding is not the one an
earlier reading of the binary suggested.** The oracle carries 35 fused multiply-add
instructions — 31 scalar `fmadd`, 3 `fmla`, 1 mixed — and they are *not* confined to the
transform:

    16  main                 (verify/report arithmetic; not on the byte path)
     6  frame_code           ** decision path: the coder **
     3  band_rdo             ** decision path: per-band rate/distortion **
     3  dct4_ortho           the forward MDCT — benign, see below
     2  curves_compute._omp_fn.0  ** decision path: the curve pass **
     1  allocate._omp_fn.0   ** decision path: the Lagrangian `w[f]*D[k] + lam*R[k]` **
     1  allocate._omp_fn.1   ** decision path: the final pick **
     1  proxy_report.constprop.0   (report only)
     1  now                  (timing only)
     1  main._omp_fn.0       (report only)

The MDCT entry is the one case that is *safe*: `mdct_frame` narrows its `f64` output to
`f32` (`enc_core.c:186`), so a one-ULP double difference almost never survives the
narrowing — which is why `tests/mdct.rs` passes bitwise while the two sides do not agree
at `f64` precision. Note the "almost": it is a rate, not a guarantee.

The decision-path entries are the opposite: those values are compared, branched on and
tie-broken in `f64`, where one ULP is a different plan. Rust does not fuse these
expressions (verified on the pinned toolchain: `a*b + c` becomes `fmul`+`fadd`), so a
byte-identical blob cannot be derived from an oracle that does — it can only be
*approximated* by luck, which is what the frozen-plan differential's 24/24 bit agreement
on the default tables is: agreement that happened, not parity that is guaranteed.

So this script is an **instrument, not a gate**: it prints the oracle's identity and its
fused-multiply map, and exits 2 when that map differs from the recorded one, because a
difference there means every S8 measurement taken afterwards is against a different oracle
than the one the divergence inventory was built on (S8 inventory row 15).

`--expect clean` is the check for the remedy: a rebuild of the oracle with
`-ffp-contract=off` (or `-ffp-contract=off -march=<fixed>`), which empties the decision
path's map and makes arithmetic parity attainable by construction rather than by luck.
Nothing in this tree can rebuild that binary — it lives off-tree by E-R1 — so the script
verifies it once someone has, and refuses to pretend otherwise.

Usage:  scripts/check_uli_oracle.py [--ref PATH] [--expect native|clean]
Exit:   0 the map matches the expectation, 2 it differs, 1 harness error.

No part of this needs our build: it reads an off-tree binary, exactly the posture P1/P2
take (E-R1 forbids a dependency, not the oracle).
"""

import argparse
import hashlib
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))
REF = GBA_AUDIO / "build/uli_enc"

# The recorded map, measured 2026-11-25 on aarch64 from the off-tree Makefile's
# `CFLAGS ?= -O2 -march=native -std=gnu11 -Wall -Wno-unused-function` (GCC defaults to
# `-ffp-contract=fast`, and this binary has fused sites in ten functions).
RECORDED_NATIVE_FMA = {
    "main": 16,
    "frame_code": 6,
    "band_rdo": 3,
    "dct4_ortho": 3,
    "curves_compute._omp_fn.0": 2,
    "allocate._omp_fn.0": 1,
    "allocate._omp_fn.1": 1,
    "proxy_report.constprop.0": 1,
    "now": 1,
    "main._omp_fn.0": 1,
}

# The functions whose arithmetic can change the *bytes*. `frame_code` codes a frame;
# `band_rdo` prices a band; `curves_compute` measures the candidates; `allocate` picks the
# plan under λ. A fused site in any of these is a place where our arithmetic cannot match
# the oracle's by construction. `dct4_ortho` is excluded on purpose and the module doc
# says why (the `f32` narrowing); `main`, `proxy_report` and `now` only print or time.
DECISION_PATH = {
    "frame_code",
    "band_rdo",
    "curves_compute._omp_fn.0",
    "allocate._omp_fn.0",
    "allocate._omp_fn.1",
}
REPORT_ONLY = {"main", "main._omp_fn.0", "proxy_report.constprop.0", "now", "dct4_ortho"}

# The instruction that means "fused" on each ISA this tree has built on. A host the
# script does not know reports an error rather than pretending.
FMA_MNEMONIC = {
    "aarch64": ("fmla", "fmadd"),
    "x86_64": ("vfmadd", "fmadd"),
}

FLAG_KEYS = ("CFLAGS", "ENCFLAGS")


def fail(msg: str) -> None:
    sys.exit(f"uli-oracle-check: {msg}")


def makefile_flags(makefile: Path) -> dict[str, str]:
    """The flags the oracle is *built* with, read as text.

    Read rather than assumed: `CFLAGS ?=` means an environment override wins, so the file
    is evidence about the default. Nothing here can recover the flags of a binary built
    elsewhere — which is exactly why the fma map, not the Makefile, is what fails.
    """
    out = {}
    if not makefile.exists():
        return out
    for line in makefile.read_text().splitlines():
        m = re.match(r"\s*([A-Z_]+)\s*[?:+]?=\s*(.*)$", line)
        if m and m.group(1) in FLAG_KEYS:
            out[m.group(1)] = m.group(2)
    return out


def fma_map(binary: Path, mnemonics: tuple[str, ...]) -> dict[str, int]:
    """Count the fused multiply-add sites in each function of a static binary."""
    try:
        p = subprocess.run(["objdump", "-d", str(binary)], capture_output=True, text=True)
    except FileNotFoundError:
        fail("objdump is not installed; the fma map cannot be read")
    if p.returncode != 0:
        fail(f"objdump failed on {binary}: {p.stderr.strip()[:200]}")
    counts: dict[str, int] = {}
    fn = "?"
    for line in p.stdout.splitlines():
        m = re.match(r"^[0-9a-f]+ <(.+)>:$", line)
        if m:
            fn = m.group(1)
            continue
        if any(re.search(rf"\b{mn}\b", line) for mn in mnemonics):
            counts[fn] = counts.get(fn, 0) + 1
    return counts


def class_of(fn: str) -> str:
    if fn in DECISION_PATH:
        return "DECISION PATH"
    if fn in REPORT_ONLY:
        return "report/MDCT"
    return "unclassified"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--ref", type=Path, default=REF, help=f"oracle binary (default {REF})")
    ap.add_argument("--expect", choices=("native", "clean"), default="native",
                    help="native: the map recorded 2026-11-25. clean: a contraction-free "
                         "rebuild, which must have no fused site on the decision path")
    args = ap.parse_args()

    ref = args.ref
    if not ref.exists():
        fail(f"no oracle at {ref} (make -C {GBA_AUDIO}/encoder)")
    machine = os.uname().machine
    mnemonics = FMA_MNEMONIC.get(machine)
    if mnemonics is None:
        fail(f"no fused-multiply mnemonic known for {machine}; add one to FMA_MNEMONIC "
             "before trusting any byte-exactness claim on this host")

    sha = hashlib.sha256(ref.read_bytes()).hexdigest()
    print(f"oracle   {ref}")
    print(f"host     {machine}")
    print(f"binary   {ref.stat().st_size} bytes  sha256 {sha}")
    flags = makefile_flags(GBA_AUDIO / "encoder/Makefile")
    for key in FLAG_KEYS:
        if key in flags:
            print(f"{key:<8} {flags[key]}")
    if "-march=native" in flags.get("CFLAGS", ""):
        print("note     -march=native is set in CFLAGS: the oracle is ISA-specific")
    if "-ffp-contract=off" not in flags.get("CFLAGS", ""):
        print("note     CFLAGS does not pin -ffp-contract: GCC's default is `fast` (fuse)")
    print()
    print("byte-exactness against this binary is a same-host, same-binary claim (E-R5's")
    print("float clause). A rebuild elsewhere, or with different float flags, is a different")
    print("oracle.")
    print()

    counts = fma_map(ref, mnemonics)
    print("fused multiply-add sites, by function:")
    if not counts:
        print("  none")
    for fn in sorted(counts, key=lambda k: (-counts[k], k)):
        print(f"  {counts[fn]:>4}  {fn:<28} {class_of(fn)}")
    print()
    decision = {fn: n for fn, n in counts.items() if fn in DECISION_PATH}
    if decision:
        print(f"the decision path carries {sum(decision.values())} fused site(s) — our Rust")
        print("does not fuse, so byte-exactness here cannot follow from arithmetic parity.")
        print("See S8 inventory row 15; the remedy is a `-ffp-contract=off` oracle build")
        print("(`--expect clean`) and re-taken measurements.")
    else:
        print("the decision path is contraction-free: arithmetic parity is attainable by")
        print("construction, and the MDCT's remaining sites are absorbed by the f32 narrowing.")
    print()

    if args.expect == "native":
        ok = counts == RECORDED_NATIVE_FMA
        if ok:
            print("OK: the oracle's fused-multiply map is the one the inventory was built on.")
            return 0
        print("CHANGED: the oracle's float arithmetic is not the shape the record describes.",
              file=sys.stderr)
        for fn in sorted(set(counts) | set(RECORDED_NATIVE_FMA)):
            got, want = counts.get(fn, 0), RECORDED_NATIVE_FMA.get(fn, 0)
            if got != want:
                print(f"  {fn}: {got} fused sites, recorded {want}", file=sys.stderr)
        print("", file=sys.stderr)
        print("Every S8 measurement, and every divergence already attributed, was taken",
              file=sys.stderr)
        print("against a different oracle. Fix the build flags off-tree or re-take the",
              file=sys.stderr)
        print("measurements; do not re-pin this script.", file=sys.stderr)
        return 2

    # `--expect clean`: a contraction-free oracle. Anything left on the decision path means
    # the rebuild did not do what it claims, and the MDCT's sites are tolerated by class.
    if not decision:
        print("OK: no fused multiply-add on the decision path (contraction-free oracle).")
        return 0
    print("NOT CLEAN: the decision path still carries fused multiply-add sites.",
          file=sys.stderr)
    for fn, n in sorted(decision.items()):
        print(f"  {fn}: {n}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
