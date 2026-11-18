#!/usr/bin/env python3
"""Check that our checked-in GBA target spec is still the built-in one plus a
known, allowed set of differences.

`targets/thumbv4t-gba.json` is `rustc --print target-spec-json --target
thumbv4t-none-eabi` with one field changed (`frame-pointer: may-omit`). That is
sound only for as long as *everything else* still matches the pinned toolchain:
the data layout, the float ABI, the relocation model, the linker flavour and the
panic strategy are all inherited silently, and a toolchain move can change any of
them. When it does, this spec is lying about the target it claims to be — the
build still succeeds, and the ROM it produces is quietly not the target the
records in `crates/uli-lite/PLAN-OF-RECORD.md` describe.

So this is a witness, not a formality: run it when `rust-toolchain.toml` moves
(`make target-spec-check`).

Usage:
    check_target_spec.py BUILTIN.json OURS.json --allow-diff frame-pointer [more...]

Exit 0 when the two specs are identical except for the allowed keys, and
`--allow-diff` keys may differ in *value* only (they must still be present on
both sides, so an allowed key can never be used to hide a *missing* field).
"""

from __future__ import annotations

import argparse
import json
import sys


def load(path: str) -> dict:
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except OSError as e:
        sys.exit(f"check_target_spec: cannot read {path}: {e}")
    except json.JSONDecodeError as e:
        sys.exit(f"check_target_spec: {path} is not valid JSON: {e}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("builtin", help="output of `rustc --print target-spec-json`")
    ap.add_argument("ours", help="the checked-in spec")
    ap.add_argument(
        "--allow-diff",
        nargs="+",
        default=[],
        metavar="KEY",
        help="keys whose *values* may differ (they must still exist on both sides)",
    )
    args = ap.parse_args()

    builtin = load(args.builtin)
    ours = load(args.ours)
    allowed = set(args.allow_diff)
    problems: list[str] = []

    # A key on one side only is always a problem: a field we dropped would be
    # filled from rustc's defaults, and a field rustc added would be silently
    # missing from ours.
    for key in sorted(builtin.keys() - ours.keys()):
        problems.append(f"missing from ours: {key!r} (built-in has {builtin[key]!r})")
    for key in sorted(ours.keys() - builtin.keys()):
        problems.append(f"extra in ours: {key!r} (ours has {ours[key]!r})")

    for key in sorted(builtin.keys() & ours.keys()):
        if builtin[key] == ours[key]:
            continue
        if key in allowed:
            continue
        problems.append(
            f"{key!r} differs but is not an allowed difference: "
            f"built-in {builtin[key]!r} vs ours {ours[key]!r}"
        )

    # An allowed key that is *identical* is not an error (someone may have
    # reverted it deliberately), but silence is how a spec quietly stops being
    # what the docs claim, so say so on stdout.
    for key in sorted(allowed):
        if key in builtin and key in ours and builtin[key] == ours[key]:
            print(
                f"check_target_spec: note: allowed difference {key!r} matches the "
                f"built-in value ({builtin[key]!r}) — is that intended?",
                file=sys.stderr,
            )

    if problems:
        print("check_target_spec: FAILED", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\nRe-derive the spec and re-apply only the allowed differences; see\n"
            "targets/README.md. Do not edit this check to make it pass.",
            file=sys.stderr,
        )
        return 1

    changed = ", ".join(
        f"{k}: {builtin[k]!r} -> {ours[k]!r}"
        for k in sorted(allowed)
        if k in builtin and k in ours and builtin[k] != ours[k]
    )
    print(f"check_target_spec: ok ({len(ours)} fields, only {changed} differs)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
