# E1 sample — ROM→RAM staging probe (measured 2026-09-22, **not integrated**)

The reappliable sample code for the E1 experiment in FLAC.md's
"Perf follow-ups — experiment menu". **Result: cartridge reads are not the
story** — staging the frame bytes into EWRAM before decoding saved only
4.72% (FIXED) / 1.78% (LPC); worst frames stayed 210% / 646% of the
1,048,750-cycle budget. Full findings, method, and reading:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E1 result — ROM→RAM staging: cartridge reads are not the story (2026-09-22)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it.

## What the probe does

Three IRQ-off measurement windows per frame, on one image (one instrument):

1. **ctrl** — PR 3's `driver::decode_one` from the embedded ROM region,
   verbatim — the same-run control;
2. **copy** — the staging memcpy (frame slice offset N..N+1, tail to region
   end) into a per-frame EWRAM scratch — timed separately: the copy *is*
   the ROM-read traffic;
3. **staged** — the same decode over the staged copy through a
   `driver::ClipView` seam introduced by the patch (borrowed decode inputs
   with any lifetime — `SpikeClip::region` is `&'static`, so a RAM copy
   cannot be handed to it; `decode_one` delegates through the view, keeping
   one decode path). A per-frame staged-vs-ctrl sample-equality witness
   proves the staged arm decodes bit-exactly before any number counts.

Also in the patch: 3 discarded calibration warm-up windows (this code
layout's first window measured 2 cycles cold — 31 vs steady 33; the
stability witness must judge the hot path).

## Files

| File | What |
|---|---|
| `e1-rom-ram-staging.patch` | `git diff` of the probe against `main` @ `756220b` (PR 3 merge), touching only `examples/flac_spike/src/driver.rs` + `src/main.rs`. Verified `git apply --check` clean at that commit. |

## Re-applying (e.g. for a future E3/E4 comparison run)

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e1-rom-ram-staging/e1-rom-ram-staging.patch
make native-spike-rom
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e1_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e1_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e1_1.log /tmp/e1_2.log && echo IDENTICAL
grep -E "stats:|budget\(|verdict" /tmp/e1_1.log
# undo:
git apply -R examples/flac_spike/experiments/e1-rom-ram-staging/e1-rom-ram-staging.patch
```

If the patch no longer applies cleanly (the spike harness moved), take the
three-window shape from the patch rather than forcing the hunks — the
method is the point, the line numbers are not.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (agb 0.25 build), runs 2026-09-22, ROM sha256
`6a9cd9f184cb347440f5a951bf82aed83243fe8c2b498f0dc7f5fcaa56e19d8c`
(built on the host VM; a rebuild on a different nightly may differ in
bytes — the *relative* result is the finding):

| arm | ctrl mean | copy mean | staged mean | staged saving | staged worst %budget |
|---|---|---|---|---|---|
| FIXED (`l0_stereo`) | 2,300,072 | 50,678 | 2,191,537 | **4.72%** | 210.3% |
| LPC (`l4_stereo`) | 6,861,091 | 48,096 | 6,739,107 | **1.78%** | 646.3% |

Calibration: overhead 22, stable, nets 0. Decode proof matched the PCM pin
`0x54C7B356621B6E15` (cross-arm agree) before any timing; screen RED =
budget miss, not harness failure.
