# E10 sample — hybrid ARM-mode Rice codeword loop, three-arm probe (measured 2026-10-30, **not integrated**)

The reappliable sample code for **E10** in FLAC.md's "Perf follow-ups —
experiment menu" (algorithm plan 2026-10-14, design of record 2026-10-16,
left-to-measure item 6): the whole non-escape Rice partition sample loop
hand-written in ARM mode — tier-1 `ctz8` (q ≤ 7), C1 De Bruijn (8..=31),
C2 word-skip (> 31), bit-reversed window with `rev8` refill — measured as
**ctrl / tab / arm** on one image. **Result: the biggest decoder-side win
the menu has measured** — marginal `arm − tab` **−40.08%** FIXED /
**−11.51%** LPC (both past the ≥ ~10% carry bar); FIXED mean 170.2% →
**101.98%** of budget (522 c/sample), worst 102.5%. Nothing closes
mean-and-worst alone; the reading is cumulative. Full findings:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E10 result — the ARM-mode hybrid codeword loop lands the menu's biggest FIXED-side win (2026-10-30)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as the E2b-landed tree shipped it.

## What the probe does

Four IRQ-off measurement windows per frame, one image (the E2b three-window
shape + the new arm; `acc` carried verbatim as the cross-session witness):

1. **ctrl** — `driver::decode_one` (verbatim live path). NOTE for re-runs
   against newer bases: the E2b table count has since **landed** in
   `crates/flac-lite` (`bits::LEADING_ONES` pinned at `0x0300022c` —
   visible in `nm`), so ctrl ≈ tab on this image; `arm − tab` and
   `arm − ctrl` coincide.
2. **acc** — `e2::decode_one_accumulator` (E2a sample verbatim).
3. **tab** — `e2b::decode_one_rice_table` (E2b sample verbatim — the plan's
   baseline: E10 replaces the count, not stacks on it).
4. **arm** — `e10::decode_one_arm`: the e2b vendored body verbatim except
   `decode_rice_partition`, which hands the whole non-escape sample loop
   to the ARM kernel `arm_rice_partition` (`asm/rice_partition.s` + the
   committed `asm/fold_body.s` pasted via `include_str!`/`concat!`).
   Tables `CTZ8` + `REV8` + `BRUIJN_INDEX` pinned `.iwram` — the plan's
   544 B budget, witnessed by `nm`/`objdump -h` and the boot log echo.

**The stitch (probe-time finding before measurement):** the plan's C1
consume formula `(w >> (q+1)) & ((1<<p)-1)` is window-domain; FLAC's Rice
remainder is MSB-first, so the kernel un-reverses the p extracted bits
(`rev16` via two `REV8` lookups, `>> (16 − p)`, ~8 cyc/sample when p > 0)
before composing `(q << p) | rem` and folding. Bit-exactness vs `flac -d`
(host) and the ROM's 314/314 frame sample-equality are the witnesses.

**Harness port (Opus rollover rule):** the four-arm pass spans > 2^32
cycles, so every window zeroes the timer pair first (`timed_reset`, the
reset outside the start→end span) and the wrap sentinel rides the
calibration netting un-subtracted (`net_sentinel`). The first build's run
convicted both halves of this rule (LPC arm frame 125 printed
`4294967271` with `wrapped=false`); the closure arithmetic pre-dated the
re-run — see the FLAC.md entry.

## Files

| File | What |
|---|---|
| `e10-three-arm-probe.patch` | `git diff` of the probe against `main` @ `c3ebd3b`, touching `examples/flac_spike/{asm/fold_body.s (new, verbatim vendor), asm/rice_partition.s (new), src/e10.rs (new), src/e2.rs, src/e2b.rs (E2b sample carried verbatim), src/lib.rs, src/main.rs, tests/{e10_witness.rs, e2_witness.rs, e2b_witness.rs}}`. Verified `git apply --check` clean at that commit; byte-identical to `git diff c3ebd3b 5525336` (the measurement commit). |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e10-three-arm-probe/e10-three-arm-probe.patch
make spike-test && make native-spike-rom
# placement witnesses before trusting numbers:
nm examples/flac_spike/target/thumbv4t-none-eabi/release/flac-spike | grep -E "e10.*(CTZ8|REV8|BRUIJN)"
# expect 0300032c BRUIJN_INDEX / 0300034c CTZ8 / 0300044c REV8 (.iwram range)
/usr/bin/objdump -h examples/flac_spike/target/thumbv4t-none-eabi/release/flac-spike | grep iwram
# expect .iwram Size 0x64c (0x32c + 256 + 544 over the e2b-landed layout)
# ARM-mode-ness (ELF fact): objdump -d ... | awk '/<arm_rice_partition>:/,/^$/'
# — 4-byte encodings, `pop {…, lr}` + `bx lr` tail (never an LDM-to-PC)
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e10_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e10_$i.log; do sleep 2; done; kill $P 2>/dev/null); done
cmp /tmp/e10_1.log /tmp/e10_2.log && echo IDENTICAL
grep -E "e2b summary|placement witness|calibration" /tmp/e10_1.log
# undo:
git apply -R examples/flac_spike/experiments/e10-three-arm-probe/e10-three-arm-probe.patch
```

If the patch no longer applies cleanly (the harness moved), take the
**four-window + equality-witness + unified-calibration + timed_reset**
shape from the patch rather than forcing the hunks; `src/e10.rs` and
`asm/*.s` are standalone modules and should port verbatim.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (0.10.5, same host binary as the recorded era),
Rust nightly 1.100.0 (a69a63265 2026-09-03), runs 2026-10-30. ROM sha256
`87c80fa54e208e5940b9c1c4dc3a7dc55a0ac6f5c8fa51347e45137343c093db` — the
image that printed the quoted log; a clean-worktree rebuild of the
measurement commit printed a different sha `2aba8dcf…` (the recorded
rebuild drift) with the **whole serial log byte-identical** — the log is
the witness, the sha provenance; two runs of the measured image
`cmp`-identical, 1,017 lines;
decode proof PASSED before timing; calibration overhead **35** stable —
the timed_reset window shape — IME = 0 in-window; 157 frames per arm;
budget 1,048,750 cycles/frame; net of this image's own calibration):

| arm | FIXED sum | LPC sum | FIXED worst | LPC worst |
|---|---:|---:|---|---|
| ctrl | 280,206,731 | 970,588,441 | 1,793,643 @136 | 6,213,963 @145 |
| acc | 346,674,306 | 1,089,159,380 | 2,222,575 @4 | 6,979,748 @145 |
| tab | 280,212,854 | 970,594,564 | 1,793,682 @136 | 6,214,002 @145 |
| arm | **167,911,108** | **858,870,747** | 1,074,944 @147 | 5,499,144 @145 |

- **FIXED:** arm − tab **−40.08%** (112,301,746 saved ≈ 349 c/sample at
  the house convention mean/blocksize), mean 170.2% → **101.98%** of
  budget (871.5 → 522.2 c/sample), worst **102.50%** — menu-best, over.
- **LPC:** arm − tab **−11.51%** (111,723,817 ≈ 348 c/sample — the same
  per-sample constant as FIXED: the win rides the per-sample loop, not
  run length), mean 589.5% → **521.6%**.
- **Cross-session witnesses:** acc − ctrl +23.72% / +12.22% reproduce
  E2b's recorded marginals exactly; ctrl sums sit −2 cyc/frame from the
  tab sums recorded in E2b (the E2b count now lives in the live decoder).
- **Tier census (twin, exact embedded bytes):** FIXED t1 100.00%; LPC t1
  99.21% / t2 0.79% / **t3 0.00%** — the C2 tail is dormant on this
  content; the win is ARM mode across the whole partition loop.
- Worst frames stay **OVER BUDGET in every arm**. Screen verdict RED =
  measurement completed, budget missed.

## Gates on the patched tree

`make spike-test` (host tests 23 → 45: lib 15 → 30 (+6 `e10`),
`tests/e10_witness.rs` 4, carried `e2`/`e2b` suites 3, `spike_witness` 8),
`make flac-test` (120 passed, 1 ignored — unchanged; `crates/flac-lite/`
untouched by the patch), `make check` clean, `make native-spike-rom`
builds. `make kernel-test` untouched (arm-kernels crate unchanged).
