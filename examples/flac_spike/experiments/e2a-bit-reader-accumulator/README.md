# E2a sample — bit-reader refill-accumulator probe (measured 2026-09-24, **not integrated**)

The reappliable sample code for the E2 experiment in FLAC.md's
"Perf follow-ups — experiment menu" (the reader-swap variant, **E2a** — the
unary quotient-count idea is E2b territory and is deliberately NOT in this
probe). **Result: the accumulator reader did NOT collapse the FIXED arm** —
mean **−3.14%** (217.4% → 210.5% of the 1,048,750-cycle budget, still
1,078 c/sample against the budget's 512) — and the LPC arm got
deterministically **1.41% slower**. The reader *implementation* is ruled
out as the cost center; the per-bit unary loop and generic decode plumbing
remain. Full findings and reading:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E2 result — the accumulator reader does not collapse the FIXED arm (2026-09-24)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it, plus the
calibration-stability patch that this probe carries in both passes (part of
the patch, same as E4).

## What the probe does

Two IRQ-off measurement windows per frame, on one image (one instrument):

1. **ctrl** — PR 3's `driver::decode_one` (the verbatim i64 decode path on
   the position-only `BitReader`), the same-run control;
2. **variant** — `e2::decode_one_accumulator`: a vendored copy of the hot
   path (`decode_frame` → `decode_subframe` → residual/rice loop →
   integrators → decorrelate) whose **only** change is the bit reader —
   `e2::AccumulatorReader`: `data + pos (bytes) + acc (u32, left-aligned
   MSB-first, next bit = bit 31) + valid`, invariant
   `logical cursor = pos*8 − valid`; fast extract + shift consume,
   byte-at-a-time refill when `valid <= 24`, and a position-based
   (`peek_at`) fallback for cold paths — every `n == 32` read included,
   32-bit reads are never forced through the accumulator. All arithmetic
   stays **i64 exactly as live** (E4's i32 result was never merged; it is
   NOT part of this variable), and the unary loop stays literally
   `while read_bits(1)? == 0 { q += 1 }` (no CLZ — ARMv4T has none;
   counting the quotient another way would be E2b).

Witnesses before any number counts:

- **Host** (`tests/e2_witness.rs`, runs under `make spike-test`): the
  variant decodes the exact embedded clips bit-exactly vs `flac -d`.
- **Host reader unit tests** (`e2.rs`, `make spike-test`): differential
  sweeps against the live `BitReader` — mixed-width sweeps (incl. n=32
  cold, illegal widths, EOF), `byte_align` at every alignment, composite
  (coded-number / wasted-bits) agreement incl. atomicity.
- **ROM**: per-frame variant-vs-control **sample-equality** (E1/E4
  pattern) — 314/314 frame-windows equal; first offender named;
  divergence voids the numbers.
- **Calibration**: E4's verified shape in both passes — ONE byte-identical
  loop over 3 discarded + 5 reported windows, unconditional stores, no
  logging between windows. Overhead on this image: 24 (all three passes,
  stable, nets 0).

## Files

| File | What |
|---|---|
| `e2a-bit-reader-accumulator.patch` | `git diff` of the probe against `main` @ `743a913` (E4 docs merge), touching `examples/flac_spike/src/{e2.rs (new), lib.rs, main.rs}` + `tests/e2_witness.rs` (new). Verified `git apply --check` clean at that commit. |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e2a-bit-reader-accumulator/e2a-bit-reader-accumulator.patch
make spike-test && make native-spike-rom
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e2a_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e2a_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e2a_1.log /tmp/e2a_2.log && echo IDENTICAL
grep -E "e2 (ctrl|variant|summary)|e2 calibration" /tmp/e2a_1.log | head
# undo:
git apply -R examples/flac_spike/experiments/e2a-bit-reader-accumulator/e2a-bit-reader-accumulator.patch
```

If the patch no longer applies cleanly (the harness moved — PR 4/5 touch
`main.rs`), take the **two-window + equality-witness + unified-calibration**
shape from the patch rather than forcing the hunks; `e2.rs` itself is a
standalone module and should port verbatim.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (agb 0.25 build), runs 2026-09-24 on the same host
runner as PR 3 / E1 / E4, Rust nightly 1.100.0 (a69a63265 2026-09-03), ROM
sha256 `fc807f02d6fb4829e1b23ad94fa3a016ef7d0da823be002b0fef0aac3dd338c4`
(two runs `cmp`-identical, 1005 lines; a rebuild on a different nightly may
shift absolute numbers — the *relative* result is the finding):

| arm | ctrl sum | variant sum | delta | ctrl mean %budget | variant mean %budget | variant worst %budget |
|---|---|---|---|---|---|---|
| FIXED (`l0_stereo`) | 357,908,871 | 346,677,917 | **−3.14%** | 217.4% | 210.5% | 211.9% |
| LPC (`l4_stereo`) | 1,073,988,846 | 1,089,162,991 | **+1.41% (slower)** | 652.3% | 661.5% | 665.5% |

Net of this image's own calibration (overhead 24, stable, nets 0, all
three passes). 157 frames / 320,000 samples per arm; budget 1,048,750
cycles/frame ≈ 512 c/sample — **no arm crosses the line** (variant FIXED
1,078.2 c/sample, variant LPC 3,387.4; ctrl FIXED 1,113.1, LPC 3,340.2). Control reproduces the recorded
baseline shape: worst-frame indices **exact** (FIXED @60, LPC @136, min
@156 both arms); this image's PR 3-shape pass printed FIXED
sum=357,906,830 max=2,294,068@60 and LPC sum=1,073,986,805
max=6,880,979@136 — the e2 ctrl arm sits a constant +13 cyc/frame above it
(+2,041 sum, code layout), the E1/E4 cross-session witness. The variant's
worst frame moves index (FIXED @60 → @4, LPC @136 → @145): the top frames
sit within ~0.3% of each other and micro-layout shuffles their order —
the mean, not the worst, is the reading. Decode proof matched the PCM pin
`0x54C7B356621B6E15` (cross-arm agree) before any timing.

Gates green on the patched tree: `make spike-test` (host tests 23 → 27:
lib 15 → 18, +1 `e2_witness`, `spike_witness` 8 unchanged), `make
flac-test` (115 passed, 1 ignored — unchanged; `crates/flac-lite/` is
untouched by the patch), `make check`, `make native-spike-rom`.
