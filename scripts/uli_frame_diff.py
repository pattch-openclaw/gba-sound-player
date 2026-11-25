#!/usr/bin/env python3
"""The frozen-plan differential: our frame coding against the reference's, plan held still.

S8-1 built the reference half of this (`scripts/uli_probe.py` drives the oracle's own
`frame_code` at a frozen `(g, ms, tables)`); `crates/uli-enc/src/bin/frame-dump.rs` is the
same coding from this crate, in the same units. This script runs both on one corpus and
diffs them field by field.

Why not a `diff` of the two outputs: the fields do not mean the same thing under
disagreement. `bits` and `stream` are what the encoder *emits*, `D` is what it *measured*,
and `cycles` is a model of the decoder. A single off-by-one bit and a 3-ULP distortion
rounding are different findings, and the report has to say which.

Known distinction, recorded rather than papered over: the reference's `D` is **derived**, as
`J - lam*(bits-1)` from its Lagrangian (`enc_core.c:404`), while ours is the direct sum of
squared reconstruction errors. Those cannot agree bit for bit even where the codings do, so
`D` is reported with its ULP distance and the summary separates "same coding, different
arithmetic" from "different coding".

Usage:
    scripts/uli_frame_diff.py                       # --coef tests/fixtures/mdct_ref_coef.f32
    scripts/uli_frame_diff.py --coef corpus.f32 --g 60 --g 100 --ms 0
    scripts/uli_frame_diff.py --tables trained.bin  # code with the oracle's trained set

Exit status: 0 when every field agrees, 2 on a difference (the `uli_accept.py` convention),
1 on a harness error.
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE = ROOT / "scripts/uli_probe.py"
DEFAULT_COEF = ROOT / "crates/uli-enc/tests/fixtures/mdct_ref_coef.f32"

# The line the probe and `frame-dump` both print. Kept in one place so a format change
# breaks the parser loudly instead of being silently skipped.
#
# The `ops` counters are compared because the cycle model is a weighted sum of them (twelve
# terms): "our estimate is 0.5% high" says nothing about *which* term moved, while the counts
# do. `D` is separate — it is measured, not emitted, and the two sides derive it differently.
OPS = ["bandsyms", "coded_bands", "pairs", "quads", "nz", "zc", "raws", "escs", "long_codes"]
EMITTED = ["bits", "halfwords", "stream", "cycles"]
FIELDS = ["frame", "g", "ms", "bits", "D", "cycles", "halfwords", "stream", *OPS]


class HarnessError(Exception):
    pass


def parse(text, label):
    """Map each line to `{field: value}`; refuse anything that is not the documented shape."""
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        toks = line.split()
        keys = toks[0::2]
        if keys != FIELDS:
            raise HarnessError(
                f"{label}: line {n} has keys {keys}, not {FIELDS} — the two tools disagree "
                f"about the format, so nothing below would be comparable"
            )
        out.append(dict(zip(keys, toks[1::2])))
    if not out:
        raise HarnessError(f"{label}: no frames emitted")
    return out


def ulp(a, b):
    """Distance between two `f64` bit patterns, as an integer (0 = identical)."""
    ia, ib = int(a, 16), int(b, 16)

    def ordered(v):
        # Map the IEEE-754 total order onto integers so negative values do not wrap.
        return v - (1 << 63) if v & (1 << 63) else v + (1 << 63)

    return abs(ordered(ia) - ordered(ib))


def run(cmd, label):
    cp = subprocess.run(cmd, capture_output=True, text=True)
    if cp.returncode != 0:
        raise HarnessError(f"{label} exited {cp.returncode}:\n{cp.stderr.strip()}")
    return cp.stdout


def main() -> int:
    ap = argparse.ArgumentParser(description="frozen-plan differential (see the docstring)")
    ap.add_argument("--ours", type=Path,
                    help="path to frame-dump (default: beside the release uli-enc build)")
    ap.add_argument("--coef", type=Path, default=DEFAULT_COEF)
    ap.add_argument("--g", type=int, action="append", dest="gs")
    ap.add_argument("--ms", type=int, action="append", dest="mss", choices=(0, 1))
    ap.add_argument("--mu", type=float, default=0.0)
    ap.add_argument("--kappa", type=float, default=0.13)
    ap.add_argument("--g-prev", type=int, default=0)
    ap.add_argument("--tables", type=Path)
    ap.add_argument("--entries", type=int, default=4,
                    help="how many differing frames to print in full (default 4)")
    args = ap.parse_args()

    if not PROBE.exists():
        raise HarnessError(f"no probe at {PROBE}")
    if not args.coef.exists():
        raise HarnessError(f"no coefficient corpus at {args.coef}")
    if args.tables and not args.tables.exists():
        raise HarnessError(f"no table blob at {args.tables}")

    ours = args.ours
    if ours is None:
        host = subprocess.run(["rustc", "-vV"], capture_output=True, text=True).stdout
        triple = next((ln.split(": ", 1)[1] for ln in host.splitlines()
                       if ln.startswith("host: ")), None)
        candidate = ROOT / "crates/uli-enc/target" / (triple or "") / "release/frame-dump"
        if not candidate.exists():
            raise HarnessError(
                f"no frame-dump at {candidate} — build it with "
                f"`make uli-frame-diff`, or pass --ours"
            )
        ours = candidate

    # The flags, in the one form both tools accept.
    common = ["--coef", str(args.coef), "--mu", repr(args.mu), "--kappa", repr(args.kappa),
              "--g-prev", str(args.g_prev)]
    for g in args.gs or []:
        common += ["--g", str(g)]
    for ms in args.mss or []:
        common += ["--ms", str(ms)]
    if args.tables:
        common += ["--tables", str(args.tables)]

    print(f"corpus: {args.coef}")
    print(f"plan: g={args.gs or 'default 60,100,140'} ms={args.mss or 'default 0,1'} "
          f"mu={args.mu} kappa={args.kappa} g_prev={args.g_prev}")
    print(f"tables: {args.tables or 'the default set'}")

    theirs = parse(run([sys.executable, str(PROBE), *common], "uli_probe.py"), "probe")
    mine = parse(run([str(ours), *common], "frame-dump"), "frame-dump")
    if len(theirs) != len(mine):
        raise HarnessError(
            f"{len(theirs)} frames from the probe, {len(mine)} from frame-dump"
        )

    # Exact fields first, D on its own because it is derived differently on each side.
    diffs = {f: 0 for f in [*EMITTED, *OPS]}
    worst_d = (0, None)
    printed = 0
    for a, b in zip(mine, theirs):
        differing = [f for f in diffs if a[f] != b[f]]
        if ulp(a["D"], b["D"]) > worst_d[0]:
            worst_d = (ulp(a["D"], b["D"]), f"frame {a['frame']} g={a['g']} ms={a['ms']}")
        for f in differing:
            diffs[f] += 1
        if differing and printed < args.entries:
            printed += 1
            print(f"\nframe {a['frame']} g={a['g']} ms={a['ms']}: " + ", ".join(differing))
            for f in differing:
                print(f"  {f:9} ours {a[f]}"[:160])
                print(f"  {' ' * 9} ref  {b[f]}"[:160])
            if a["stream"] != b["stream"]:
                # Name the first differing halfword rather than dumping two 700-char strings.
                for i in range(0, max(len(a["stream"]), len(b["stream"])), 4):
                    if a["stream"][i:i + 4] != b["stream"][i:i + 4]:
                        print(f"  stream differs at halfword {i // 4}: "
                              f"ours {a['stream'][i:i + 4]}, ref {b['stream'][i:i + 4]}")
                        break

    n = len(mine)
    print(f"\ncompared {n} frame(s)")
    for f in [*EMITTED, *OPS]:
        print(f"  {f:12} {'same' if diffs[f] == 0 else f'{diffs[f]} of {n} differ'}")
    total_d = sum(1 for a, b in zip(mine, theirs) if a["D"] != b["D"])
    print(f"  {'D':12} bitwise same on {n - total_d} of {n}; "
          f"worst ULP distance {worst_d[0]}" + (f" ({worst_d[1]})" if worst_d[1] else ""))

    # A counters difference is the actionable half: name the term, because each one has its
    # own weight in the cycle model and only one of them can explain a given delta.
    ops_bad = [(f, a[f], b[f], f"frame {a['frame']} g={a['g']} ms={a['ms']}")
               for a, b in zip(mine, theirs) for f in OPS if a[f] != b[f]]
    if ops_bad:
        seen = {}
        for f, mine_v, theirs_v, where in ops_bad:
            seen.setdefault(f, (mine_v, theirs_v, where))
        print("\n  the read-side counters differ:")
        for f, (mine_v, theirs_v, where) in seen.items():
            print(f"    {f:12} ours {mine_v}, ref {theirs_v}   ({where})")

    coding_same = all(diffs[f] == 0 for f in EMITTED)
    if coding_same and total_d == 0 and not ops_bad:
        print("\nidentical: bits, stream, cycles, counters and distortion all agree")
        return 0
    if coding_same and not ops_bad:
        print("\nthe coding agrees — bits, stream and cycles are identical; only the "
              "distortion's arithmetic differs (ours is a direct sum, the reference derives "
              "it as J - lam*(bits-1))")
        return 2
    if coding_same:
        print("\nthe coding agrees — the same bits, the same stream — but the *counters* "
              "differ, so the cycle model reads differently for the same frame")
        return 2
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except HarnessError as e:
        print(f"uli-frame-diff: {e}", file=sys.stderr)
        sys.exit(1)
