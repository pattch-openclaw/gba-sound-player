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

# The two recorded builds. **`clean` is the oracle of record since 2026-11-25 (S8-4b)**;
# `native` is the binary it replaced, kept as a named profile so a tree that still has it
# can be told what it is rather than only that it is wrong.
#
# `native` was built with the off-tree Makefile's default
# `CFLAGS ?= -O2 -march=native -std=gnu11 -Wall -Wno-unused-function`, and GCC's default
# `-ffp-contract=fast` fused multiply-adds in ten functions — 13 of them on the decision
# path, which is why exactness against it was never attainable by construction.
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

RECORDED = {
    "clean": {
        "sha256": "1991ee6c2e9f039cd2bc974abcd193f793c147cfdfd040cba2fd2d009b50ae6b",
        "cflags": "-O2 -std=gnu11 -Wall -Wno-unused-function -ffp-contract=off "
        "-march=armv8-a",
        "fma": {},
        "note": "rebuilt 2026-11-25 (S8-4b); zero fused multiply-add sites anywhere",
    },
    "native": {
        "sha256": "71df29fef11af785cd19f8c72c32afc947cfd2321819a7312718c3dad266e3a7",
        "cflags": "-O2 -march=native -std=gnu11 -Wall -Wno-unused-function",
        "fma": RECORDED_NATIVE_FMA,
        "note": "the pre-S8-4b binary: 35 fused sites, 13 on the decision path",
    },
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
    ap.add_argument("--expect", choices=("native", "clean"), default="clean",
                    help="clean: the oracle of record — no fused site on the decision path "
                         "and the recorded sha256 (default). native: the pre-S8-4b binary, "
                         "as a named historical profile")
    ap.add_argument("--allow-rebuild", action="store_true",
                    help="tolerate a sha256 that is not the recorded one, for a deliberate "
                         "rebuild that will re-record it. The map is still checked")
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
        if flags.get(key):
            print(f"{key:<8} {flags[key]}")
    print("         ^ the Makefile's default. `CFLAGS ?=` means an override wins, and the")
    print("           recorded builds below were built with one — so this line describes the")
    print("           *file*, not necessarily the binary. The map is what describes the binary.")
    print()
    print("byte-exactness against this binary is a same-host, same-binary claim (E-R5's")
    print("float clause). A rebuild elsewhere, or with different float flags, is a different")
    print("oracle.")
    print()

    counts = fma_map(ref, mnemonics)
    want = RECORDED[args.expect]
    print(f"expect   {args.expect} ({want['note']})")
    print(f"         built with {want['cflags']}")
    sha_ok = sha == want["sha256"]
    print(f"recorded {want['sha256'][:16]}…  {'MATCH' if sha_ok else 'DIFFERS'}")
    print()
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
        print("construction, and any MDCT sites are absorbed by the f32 narrowing.")
    print()

    failures = []
    if args.expect == "clean":
        if decision:
            failures.append(
                f"{sum(decision.values())} fused site(s) on the decision path "
                f"({', '.join(sorted(decision))})"
            )
    elif counts != want["fma"]:
        for fn in sorted(set(counts) | set(want["fma"])):
            got, exp = counts.get(fn, 0), want["fma"].get(fn, 0)
            if got != exp:
                failures.append(f"{fn}: {got} fused sites, recorded {exp}")
    if not sha_ok and not args.allow_rebuild:
        failures.append(
            f"the binary is not the recorded {args.expect} build (sha256 {sha[:16]}… vs "
            f"{want['sha256'][:16]}…)"
        )
    if not failures:
        print(f"OK: this is the recorded {args.expect} oracle"
              + ("" if sha_ok else " (map matches; sha differs, tolerated)"))
        return 0
    print("CHANGED: the oracle is not what the record describes.", file=sys.stderr)
    for f in failures:
        print(f"  {f}", file=sys.stderr)
    print("", file=sys.stderr)
    print("Every S8 measurement, and every divergence already attributed, was taken",
          file=sys.stderr)
    print("against a specific binary. Fix the build flags off-tree, re-record with",
          file=sys.stderr)
    print("`--allow-rebuild` once the new sha is written into PLAN-OF-RECORD §S8-4b, or",
          file=sys.stderr)
    print("re-take the measurements; do not re-pin this script quietly.", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
