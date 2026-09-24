# E4 sample — i32-intermediates probe (measured 2026-09-23, **not integrated**)

The reappliable sample code for the E4 experiment in FLAC.md's
"Perf follow-ups — experiment menu". **Result: the LPC arm's cost was half
software 64-bit math** — switching the hot-path intermediates from `i64`
to `i32` cut the LPC mean by **46.90%** (652% → 346% of the 1,048,750-cycle
budget); the FIXED arm moved only **3.74%** because its FIXED-0 integrator
early-returns and the width change is inert in its Rice hot loop. Full
findings, proof, and reading:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E4 result — i32 intermediates halve the LPC arm; FIXED barely moves (2026-09-23)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it (plus the
calibration-stability patch that the probe branch also carries — see
"Calibration" below; it is part of the patch).

## What the probe does

Two IRQ-off measurement windows per frame, on one image (one instrument):

1. **ctrl** — PR 3's `driver::decode_one` (the verbatim i64 decode path),
   the same-run control;
2. **variant** — `e4::decode_one_i32`: a vendored copy of the hot path
   (`decode_frame` → `decode_subframe` → integrators → `decorrelate`) with
   `i32` intermediates (`wrapping_*` + arithmetic `>>` = single `asr`); the
   field readers, `PredictorState::fill`, and `decode_residual` are shared
   with the control — the variable is the hot-path arithmetic width, only.

Witnesses before any number counts:

- **Host** (`tests/e4_witness.rs`, runs under `make spike-test`): the
  variant decodes the exact embedded clips bit-exactly vs `flac -d`.
- **ROM**: per-frame variant-vs-control **sample-equality** (E1 pattern) —
  314/314 frame-windows equal; first offender named; divergence voids the
  numbers.
- **Calibration**: the E1 finding hardened on this layout — a serial write
  immediately before a micro-window shifts it ~2 cycles, and separate
  warm-up windows were not enough. The patch's fix (both passes): ONE
  byte-identical loop over 3 discarded + 5 reported windows, unconditional
  stores, no logging between windows. Overhead on this image: 24, stable,
  nets 0.

## Files

| File | What |
|---|---|
| `e4-i32-intermediates.patch` | `git diff` of the probe against `main` @ `73566b8` (E1 docs merge), touching `examples/flac_spike/src/{e4.rs (new), lib.rs, main.rs}` + `tests/e4_witness.rs` (new). Verified `git apply --check` clean at that commit. |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e4-i32-intermediates/e4-i32-intermediates.patch
make spike-test && make native-spike-rom
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e4_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e4_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e4_1.log /tmp/e4_2.log && echo IDENTICAL
grep -E "e4 (ctrl|variant|summary)|e4 calibration" /tmp/e4_1.log | head
# undo:
git apply -R examples/flac_spike/experiments/e4-i32-intermediates/e4-i32-intermediates.patch
```

If the patch no longer applies cleanly (the harness moved — PR 4/5 touch
`main.rs`), take the **two-window + equality-witness + unified-calibration**
shape from the patch rather than forcing the hunks; `e4.rs` itself is a
standalone module and should port verbatim.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (agb 0.25 build), runs 2026-09-23 on the same host
as PR 3 / E1, ROM sha256
`59be708d53f9a7b1943cadbb677368beaceab6c29ddc472254a75f87294403f8`
(two runs `cmp`-identical, 1005 lines; a rebuild on a different nightly may
shift absolute numbers — the *relative* result is the finding):

| arm | ctrl sum | variant sum | delta | ctrl mean %budget | variant mean %budget | variant worst %budget |
|---|---|---|---|---|---|---|
| FIXED (`l0_stereo`) | 357,948,906 | 344,549,255 | **−3.74%** | 217.4% | 209.3% | 210.6% |
| LPC (`l4_stereo`) | 1,074,038,289 | 570,270,032 | **−46.90%** | 652.3% | 346.4% | 348.6% |

Worst-frame indices stay 60 (FIXED) / 136 (LPC) / min @156 in both arms —
the control reproduces the PR 3 baseline shape. Calibration: overhead 24
(this layout; PR 3's was 22), stable, nets 0. Decode proof matched the PCM
pin `0x54C7B356621B6E15` (cross-arm agree) before any timing. The same
image's PR 3-shape pass (calibration-patched) printed FIXED
sum=357,947,179 max=2,294,325@60 and LPC sum=1,074,036,562 max=6,881,296@136 —
the e4 ctrl arm differs from it by the constant +1,727 (≈ +11 cyc/frame,
code layout), which is what makes ctrl-vs-variant a single-instrument
comparison.

## Proof inputs measured by the probe (the i32-safety bounds)

From the exact embedded regions (field walk through the public readers):
`l4_stereo`: LPC precision ≤ 11 bits, shift ∈ [0, 11], order ≤ 4,
|coeff| ≤ 751, warm-up (stripped scale) |≤ 412, final decoded |sample| ≤ 1115
(cross-arm, both arms = same source). Dot-product bound: 4 × 751 × 1115 ≈
3.35e6 ≪ 2^31. Profile-form bound: order ≤ 4 × |coeff| < 2^(precision−1) ×
|sample| ≤ 2^16 (side bps+1) → safe for precision ≤ 13 (2^30); precision 15
would NOT be (2^32) — production needs a pack-time precision guard if the
i32 path is ever adopted.
