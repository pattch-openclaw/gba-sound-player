#!/usr/bin/env python3
"""The S3 acceptance harness: our encoder against the reference, same PCM, same budgets.

Why this exists. Twice during S3 a broken encoder looked fine because a single number
was consistent with two explanations: a rate of 0.512 kbit/s is what silence costs and
also what "the search collapsed" costs, and a rate *below* the reference's read as a
win when it was the loudest band being thrown away. So this harness never prints one
column. It prints four, and the fourth is distortion, because that is the one that
caught the band bug.

What it compares. `uli-enc --rd` against `$GBA_AUDIO/build/uli_enc`, on one input, at
the same budgets, both at the same `--cap`. The reference explores M/S internally and
cannot be told not to (there is no flag), so its numbers include the M/S gain and ours
do not: **this is a full-pipeline comparison, not a like-for-like L/R one.** The
like-for-like measurement is the fixed-step A/B in `tools/rd_ab` (see
`PLAN-OF-RECORD.md` → S3), which drives both coders at one `g` and compares bits,
distortion and cycles per frame.

Metric hygiene, which is a real trap here: `verify: ... SNR` from either encoder is
**full-band time-domain** SNR against the resampled input. E-R7's 32.29 mean / 25.78
worst dB is a **codec-only** metric and the two must never be quoted against each
other.

Usage:
    scripts/uli_rd_ab.py --album album.f32 --tracks tracks.tsv [--budgets 400k,120k,40k]
    scripts/uli_rd_ab.py --album ... --require-fit      # exit nonzero if any overshoots

Exit status is 0 when every row fit its budget and both encoders decoded, 1 otherwise.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))
REF = GBA_AUDIO / "build/uli_enc"
OURS = ROOT / "crates/uli-enc/target"

KIB = re.compile(r"(\d+(?:\.\d+)?)\s*kbit/s")
SNR = re.compile(r"SNR (\d+\.\d+) dB")
REF_OVERHEAD = re.compile(r"container overhead (\d+) bytes; stream target (\d+) bits")
REF_RAW = re.compile(r"(\d+) bytes \((\d+) unused\)")


def parse_budget(s: str) -> int:
    s = s.strip().lower()
    mult = 1
    if s.endswith("k"):
        mult, s = 1_000, s[:-1]
    elif s.endswith("m"):
        mult, s = 1_000_000, s[:-1]
    return int(float(s) * mult)


def run(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def find_ours() -> Path:
    for profile in ("release", "debug"):
        for target in sorted(OURS.glob(f"*/{profile}/uli-enc")):
            return target
    sys.exit(f"uli_rd_ab: no uli-enc binary under {OURS}; build one first (make uli-enc)")


def ours_row(binary: Path, album: Path, tracks: Path, budget: int, cap: float, work: Path):
    out = work / f"ours-{budget}.uli"
    if out.exists():
        out.unlink()
    code, text = run(
        [str(binary), "--album", str(album), "--tracks", str(tracks), "--out", str(out),
         "--rd", "--budget", str(budget), "--cap", str(cap), "--threads", "1"]
    )
    row = {"tool": "uli-enc (ours)", "ok": False, "bytes": None, "kbit": None, "snr": None,
           "g": None, "lambda": None, "detail": ""}
    if code != 0:
        row["detail"] = text.strip().splitlines()[-1] if text.strip() else f"exit {code}"
        return row
    m = KIB.search(text)
    row["kbit"] = float(m.group(1)) if m else None
    m = SNR.search(text)
    row["snr"] = float(m.group(1)) if m else None
    m = re.search(r"^g searched (\d+)\.\.(\d+) \(lambda ([\d.e+-]+)", text, re.M)
    if m:
        row["g"] = f"{m.group(1)}..{m.group(2)}"
        row["lambda"] = float(m.group(3))
    row["bytes"] = out.stat().st_size if out.exists() else None
    row["ok"] = row["bytes"] is not None and row["bytes"] <= budget
    return row


def ref_row(album: Path, tracks: Path, budget: int, cap: float, work: Path):
    out = work / f"ref-{budget}.uli"
    code, text = run(
        [str(REF), "--album", str(album), "--tracks", str(tracks), "--out", str(out),
         "--budget", str(budget), "--cap", str(cap), "--threads", "1"]
    )
    row = {"tool": "reference (C)", "ok": False, "bytes": None, "kbit": None, "snr": None,
           "g": None, "lambda": None, "detail": ""}
    if code != 0:
        row["detail"] = text.strip().splitlines()[-1] if text.strip() else f"exit {code}"
        return row
    m = KIB.search(text)
    row["kbit"] = float(m.group(1)) if m else None
    # The reference prints per-track `pcm16 / int8` SNR pairs; take the mean of the
    # pcm16 column, which is the same definition our `SNR` line reports.
    snrs = [float(x) for x in re.findall(r"(\d+\.\d+)/\d+\.\d+", text)]
    row["snr"] = sum(snrs) / len(snrs) if snrs else None
    m = re.search(r"g range (\d+)\.\.(\d+)", text)
    if m:
        row["g"] = f"{m.group(1)}..{m.group(2)}"
    row["bytes"] = out.stat().st_size if out.exists() else None
    row["ok"] = row["bytes"] is not None and row["bytes"] <= budget
    return row


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--album", required=True)
    ap.add_argument("--tracks", required=True)
    ap.add_argument("--budgets", default="400000,120000,40000")
    ap.add_argument("--cap", type=float, default=74.0)
    ap.add_argument("--kappa", type=float, default=0.13)
    args = ap.parse_args()

    album, tracks = Path(args.album), Path(args.tracks)
    for p in (album, tracks):
        if not p.exists():
            sys.exit(f"uli_rd_ab: {p} does not exist")
    if not REF.exists():
        sys.exit(f"uli_rd_ab: no reference encoder at {REF} (make -C {GBA_AUDIO}/encoder)")

    ours = find_ours()
    work = Path(tempfile.mkdtemp(prefix="uli-rd-ab-"))
    failures = []
    try:
        print(f"album  {album}  ({album.stat().st_size // 8} stereo frames)")
        print(f"ours   {ours}")
        print(f"ref    {REF}")
        print(f"cap    {args.cap}%   kappa {args.kappa}")
        print()
        hdr = f"{'budget':>9} {'tool':<16} {'bytes':>9} {'fit':>4} {'kbit/s':>8} {'SNR dB':>7}  g"
        print(hdr)
        print("-" * len(hdr))
        for b in [parse_budget(x) for x in args.budgets.split(",")]:
            for row in (ours_row(ours, album, tracks, b, args.cap, work),
                        ref_row(album, tracks, b, args.cap, work)):
                fit = "yes" if row["ok"] else "NO"
                if not row["ok"]:
                    failures.append(f"budget {b} {row['tool']}: {row['detail'] or 'over budget'}")
                print(
                    f"{b:>9} {row['tool']:<16} "
                    f"{row['bytes'] if row['bytes'] is not None else '-':>9} {fit:>4} "
                    f"{row['kbit'] if row['kbit'] is not None else float('nan'):>8.3f} "
                    f"{row['snr'] if row['snr'] is not None else float('nan'):>7.2f}  "
                    f"{row['g'] or '-'}"
                )
            print()
        print("Reminder: the SNR column is full-band time-domain on both sides. E-R7's")
        print("32.29/25.78 dB is codec-only; do not compare the two metrics.")
        print("The reference explores M/S and cannot be told not to; ours does not, yet.")
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print()
        for f in failures:
            print(f"FAIL {f}", file=sys.stderr)
        return 1
    print()
    print("OK: every row fit its budget and decoded")
    return 0


if __name__ == "__main__":
    sys.exit(main())
