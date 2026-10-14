# E2b sample — run-length-table Rice quotient count (measured 2026-10-13, **not integrated**)

The reappliable sample code for **E2b** in FLAC.md's "Perf follow-ups —
experiment menu": replace the Rice unary per-bit loop with a table count —
a 256-entry leading-ones lookup pinned to `.iwram`, riding the E2a
refill-accumulator reader. **Result: the biggest decoder-side win the menu
has measured** — FIXED mean **−21.7%** vs the live control (**−19.2%**
marginal against the same-image accumulator arm, isolating the count
change), 217.4% → **170.2%** of budget; LPC **−9.6%** vs control
(−10.9% marginal), 652.3% → 589.5%. Both arms remove ≈35% of their E8
unary buckets. Not a collapse: FIXED stays over budget, exactly as E8's
floor arithmetic said any single lever would. Full findings and reading:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E2b result — run-length table count lands the biggest decoder-side win
on the menu so far (2026-10-13)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it.

## What the probe does

Three IRQ-off measurement windows per frame, on one image (Sam's scoping,
2026-10-13: one placement — the table's IWRAM pin — and no masked-ones
arm; the third arm below is E2a's already-recorded reader, not a new
variable):

1. **ctrl** — PR 3's `driver::decode_one` (the verbatim live path), the
   same-run control;
2. **acc** — `e2::decode_one_accumulator` (the E2a sample verbatim):
   accumulator reader, unary loop still per-bit. Carried unchanged so
   `tab − acc` prices **the count change alone** (same reader, same
   vendored body, only the loop swapped), and so the arm doubles as a
   cross-session witness — it reproduced E2a's recorded −3.14% / +1.41%
   exactly on this image;
3. **tab** — `e2b::decode_one_rice_table`: the same accumulator reader,
   but the unary loop becomes `AccumulatorReader::rice_quotient`: the
   quotient is the leading-one run of the **inverted** window's top byte,
   answered by `LEADING_ONES[inv8]` (one zero-wait `ldrb`); runs longer
   than 8 bits iterate per byte-chunk (~5 cyc per 8 bits, not per bit).
   The table is `#[link_section = ".iwram")]` (plain `.iwram`, the
   E6-verified pin target) — copied ROM→IWRAM at boot by agb's
   `CommonInit` CPUSet; the `.iwram` section grows exactly 256 B
   (0x22c → 0x32c).

Witnesses before any number counts:

- **Host** (`tests/e2b_witness.rs`, runs under `make spike-test`): the
  variant decodes the exact embedded clips bit-exactly vs `flac -d`, and
  differentially frame-by-frame against the production driver.
- **Host unit tests** (`e2b.rs`): table exhaustively checked against a
  naive leading-ones oracle; quotient lengths 0..=40 swept at every bit
  offset 0..7; EOF (unterminated run) state restoration; tail-byte edge
  cases; reader mixed-width differential (the copy never drifted).
- **ROM**: per-frame variant-vs-control **sample-equality** for both
  variant arms (628 arm-frames, 314/314 frame-windows equal); first
  offender named; divergence voids the numbers.
- **Link witness**: `nm` — `LEADING_ONES` at `0x0300022c` (the `.iwram`
  range); the boot log echoes `table=0x0300022C` as the same-image echo.
- **Calibration**: E4's verified shape — ONE byte-identical loop over 3
  discarded + 5 reported windows, unconditional stores, nothing logged
  between windows. Overhead **24** on this image, stable, nets 0.

## Files

| File | What |
|---|---|
| `e2b-rice-quotient-table.patch` | `git diff` of the probe against `main` @ `14538c4`, touching `examples/flac_spike/src/{e2.rs (new, E2a verbatim), e2b.rs (new), lib.rs, main.rs}` + `tests/{e2_witness.rs, e2b_witness.rs}` (new). Verified `git apply --check` clean at that commit; byte-identical to `git diff 14538c4 fba1bc5` (the measurement commit). |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e2b-rice-quotient-table/e2b-rice-quotient-table.patch
make spike-test && make native-spike-rom
# placement witness before trusting numbers:
/usr/bin/nm examples/flac_spike/target/thumbv4t-none-eabi/release/flac-spike | grep LEADING
# expect 0300022c t ...e2b12LEADING_ONES  (.iwram range 0x0300xxxx)
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e2b_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e2b_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e2b_1.log /tmp/e2b_2.log && echo IDENTICAL
grep -E "e2b (ctrl|acc|tab) stats|e2b summary|table placement" /tmp/e2b_1.log
# undo:
git apply -R examples/flac_spike/experiments/e2b-rice-quotient-table/e2b-rice-quotient-table.patch
```

If the patch no longer applies cleanly (the harness moved — PR 4/5 touch
`main.rs`), take the **three-window + equality-witness +
unified-calibration** shape from the patch rather than forcing the hunks;
`e2b.rs` itself is a standalone module and should port verbatim.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (agb 0.25 build), runs 2026-10-13 on the same host
runner as PR 3 / E1 / E2a / E4 / E6 / E7 / E8, Rust nightly 1.100.0
(a69a63265 2026-09-03), ROM sha256
`ba1e8a34b2f1531a68a7991946c7c87f1b33a95d44751a5200bfbf2dd24fd7a9`
(rebuild sha-stable; two runs `cmp`-identical, 1010 lines; decode proof
matched the PCM pin `0x54C7B356621B6E15` both arms before any timing;
calibration overhead 24 stable, IME = 0 in-window; net of this image's own
calibration; 157 frames / 320,000 samples per arm; budget 1,048,750
cycles/frame ≈ 512 c/sample):

| arm | ctrl sum | acc sum | tab sum | ctrl worst | tab worst |
|---|---:|---:|---:|---|---|
| FIXED (`l0_stereo`) | 357,908,557 | 346,674,620 | 280,207,045 | 2,294,079 @60 | 1,793,645 @136 |
| LPC (`l4_stereo`) | 1,073,988,532 | 1,089,159,694 | 970,588,755 | 6,880,990 @136 | 6,213,965 @145 |

- **FIXED:** tab −21.71% vs ctrl (77,701,512 saved; 494,914/frame),
  −19.17% marginal vs acc (66,467,575; 423,360/frame). Mean 217.4% →
  **170.2%** of budget (2,279,672 → 1,784,758; 1,113.1 → 871.5
  c/sample). Tab's worst index moves @60 → @136 (frame 136 is a ctrl
  near-max too, 2,292,954 — the top frames shuffle order like E2a's;
  the mean is the reading).
- **LPC:** tab −9.63% vs ctrl (103,399,777; 658,597/frame), −10.89%
  marginal vs acc. Mean 652.3% → **589.5%** (3,340.2 → 3,018.6
  c/sample).
- **acc reproduces E2a exactly:** −3.14% FIXED / +1.41% LPC vs ctrl —
  the cross-session witness (this image's PR 3-shape pass printed FIXED
  sum=357,909,970, LPC sum=1,073,989,945; the ctrl arms sit a constant
  −1,413 below each, ≈ −9 cyc/frame, code layout).
- Worst frames stay **OVER BUDGET in every arm** (tab worsts: 171.0% /
  592.5%). Screen verdict RED = measurement completed, budget missed —
  harness completed 2/2.

## Gates on the patched tree

`make spike-test` (host tests 23 → 35: lib 15 → 24 (+3 `e2.rs`, +6
`e2b.rs`), `tests/e2_witness.rs` 1, `tests/e2b_witness.rs` 2,
`spike_witness` 8), `make flac-test` (115 passed, 1 ignored — unchanged;
`crates/flac-lite/` untouched), `make check`, `make native-spike-rom`.
