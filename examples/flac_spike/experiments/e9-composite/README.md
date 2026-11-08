# E9 sample — composite arms: stacked variants vs isolated gains (measured 2026-10-31, **not integrated**)

The reappliable sample code for **E9** in flac-docs/perf-results.md: every menu gain had been measured in isolation; E9 measures
the levers **stacked** on one image. Four arms per frame: **ctrl / arm /
i32 / iw**, where `i32` = E10's ARM hybrid codeword kernel + E4's i32
arithmetic width, and `iw` = the same decode writing `.iwram` static scratch
(E3). **Result: the levers compose additively in absolute cycles** (LPC's
stacked marginal reproduces E4's isolated saving at 99.78%), and the FIXED
stack **fits the full-rate budget — mean 94.47%, worst 94.95% — the first
in-budget FIXED decode in the menu's history** (mGBA; pending E5 hardware
arbitration and the landing). LPC lands 216.3% (fits 10,512 Hz at 69.4%,
misses 18,157 Hz at ~120%). E3's scratch pin is **inert** (+20 cyc/frame,
both clips). Full findings:
[`../../../../flac-docs/perf-results.md`](../../../../flac-docs/perf-results.md) →
"E9 result — stacking ARM and i32 lands the first in-budget FIXED decode (2026-10-31)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as the E10-era tree shipped it.

> **Re-applied 2026-11-06 (item 6a-2).** Applying this archive now needs
> `port-item6a2-header-bits.patch` after it, and this README's **LPC percentages**
> are no longer quotable across records. See
> [Re-application note](#re-application-note-item-6a-2-2026-11-06) at the bottom.

## What the probe does

Four IRQ-off measurement windows per frame, one image (the E10 four-window
shape; `timed_reset` per window from the start — the Opus rollover rule, the
pass spans ≈ 7 G cycles):

1. **ctrl** — `driver::decode_one` (verbatim live path; the same-run control
   and the cross-session anchor).
2. **arm** — `e10::decode_one_arm` carried verbatim from
   `experiments/e10-three-arm-probe/` (the cross-session witness: reproducing
   E10's recorded sums proves the image carries the arm E9 stacks on).
3. **i32** — `e9::decode_one_arm_i32`: the E10 vendored body with E4's i32
   layer swapped in at exactly E4's vendor boundary — `integrate_fixed_i32`,
   `integrate_lpc_i32`, `pad_block_i32` (E4 patch, verbatim) and
   `decorrelate_i32` (the shared `flac_lite::stereo::decorrelate` call
   becomes a vendored i32 copy — the side arms are width sites). The reader
   and the ARM kernel are **shared, not re-vendored** (`e10::AccumulatorReader`
   and its `rice_partition_kernel` seam). `i32 − arm` is E4's marginal
   inside the stack.
4. **iw** — the same `decode_one_arm_i32` decoding into the `.iwram` static
   scratch (2 × 2048 × i32 = 16 KiB, `unsafe(link_section = ".iwram")` via
   `addr_of_mut!`). `iw − i32` is E3's scratch-placement lever measured
   *renormalized* against the stacked frame.

**E1 was excluded by design** (ordering decision, recorded in the flac-docs/perf-results.md entry): staging relieves the position-only reader's per-byte ROM re-fetch
premium, which the ARM kernel's windowed refills already absorbed; shipping
decode reads ROM regardless, and E1 was ruled out as a production
mitigation. Stacking it would double-count the same relief.

Per-frame variant-vs-control **sample-equality** OUTSIDE the windows for all
three variant arms (E1/E2a/E4/E10 pattern), first offender named.

## Files

| File | What |
|---|---|
| `e9-composite.patch` | `git diff` of the probe against `main` @ `a70174a`, touching `examples/flac_spike/{asm/fold_body.s + rice_partition.s (new, E10 vendor verbatim), src/e2.rs, src/e2b.rs, src/e10.rs (E10 sample carried verbatim), src/e9.rs (new), src/lib.rs, src/main.rs, tests/{e2_witness.rs, e2b_witness.rs, e10_witness.rs, e9_witness.rs (new)}}`. Verified `git apply --check` clean at that commit; byte-identical to `git diff a70174a 2089854` (the measurement commit). |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e9-composite/e9-composite.patch
make spike-test && make native-spike-rom
# placement witnesses before trusting numbers:
nm examples/flac_spike/target/thumbv4t-none-eabi/release/flac-spike | grep -E "e10|e9|LEADING" | grep 0300
# expect BRUIJN_INDEX @0300032c, CTZ8 @0300034c, REV8 @0300044c,
#        e2b LEADING_ONES @0300054c, e9 scratch4::LEFT @0300064c, RIGHT @0300264c
/usr/bin/objdump -h examples/flac_spike/target/thumbv4t-none-eabi/release/flac-spike | grep iwram
# expect .iwram Size 0x464c (the E10 image's 0x64c + exactly 0x4000 = 16 KiB scratch)
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e9_$i.log 2>&1 &
  P=$!; until grep -q "PR 3 verdict" /tmp/e9_$i.log; do sleep 2; done; kill $P 2>/dev/null); done
cmp /tmp/e9_1.log /tmp/e9_2.log && echo IDENTICAL
grep -E "e9 (calibration|summary)|e9 .* stats|eq=1" /tmp/e9_1.log | head
# undo:
git apply -R examples/flac_spike/experiments/e9-composite/e9-composite.patch
```

If the patch no longer applies cleanly (the harness moved), take the
**four-window + equality-witness + unified-calibration + timed_reset** shape
from the patch rather than forcing the hunks; `src/e9.rs` and the carried
`src/e10.rs` + `asm/*.s` are standalone modules and should port verbatim.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` (same host binary as the recorded era), Rust nightly
1.100.0 (a69a63265 2026-09-03), runs 2026-10-31. ROM sha256
`d95fe9ccbbf004d08154f3174acad0f5c4cc6a7711e922487a2de2d4d0cd5611` (stable
across the measured runs and the post-commit re-hash; sha is provenance —
the two-run log `cmp` is the determinism witness: 1,357 lines, byte-identical).
Decode proof PASSED before timing (region pins + `fnv_pcm`
`0x54C7B356621B6E15` cross-arm agree); calibration overhead **35** stable,
IME = 0 in-window (the E10 timed_reset shape); 157 frames per arm; budget
1,048,750 cycles/frame; net of this image's own calibration; equality
witness `eq=1` on all 628 frame-windows (both passes × 2 clips × 157):

| arm | FIXED sum | LPC sum | FIXED worst | LPC worst |
|---|---:|---:|---|---|
| ctrl | 280,206,417 | 970,588,127 | 1,793,641 @136 | 6,213,961 @145 |
| arm | 167,877,039 | 858,836,678 | 1,074,727 @147 | 5,498,927 @145 |
| **i32** | **155,546,078** | **356,168,695** | **995,812 @147 FITS** | 2,280,770 @145 |
| iw | 155,549,218 | 356,171,835 | 995,832 @147 FITS | 2,280,790 @145 |

- **FIXED:** `i32 − arm` **−7.35%** (12,330,961 saved ≈ 38.4 c/sample);
  mean 101.96% → **94.47%** of budget, worst **94.95%** — **FITS**
  mean-and-worst at 32,768 Hz (first arm in menu history).
- **LPC:** `i32 − arm` **−58.53%** (502,667,983 ≈ 1,563 c/sample); mean
  521.60% → **216.31%** (worst 217.5%) — over at 32,768 and 18,157
  (119.9% mean), fits 10,512 (69.4% mean / 69.8% worst).
- **E3 (`iw − i32`):** **+3,140 total = +20 cyc/frame on both clips** — the
  identical constant, a layout artifact, not memory timing: **inert**.
- **Additivity (the composite question):** stacked saving vs E4's isolated
  saving — LPC 502,667,983 / 503,768,257 = **99.78%**, FIXED 12,330,961 /
  13,399,651 = 92.0%. Disjoint buckets ⇒ **additive in absolute cycles**;
  the stacked *percentages* are amplified only because the frame shrank.
- **Cross-session witnesses:** ctrl reproduces the E10-recorded sums at
  exactly −314 total (−2 cyc/frame, both clips); `arm` sits −34,069
  (−217 cyc/frame, the same constant on both clips — a layout offset of
  this image, −0.13% of the recorded sums), and the carried e2b pass
  reproduces the e9 pass's arm within +7 cyc/frame; arm means land at
  101.96% / 521.60% vs E10's recorded 101.98% / 521.62%.
- Screen verdict RED = PR 3's AND over the ctrl arms; the e9 summary lines
  carry `i32_fits=true iw_fits=true` on the FIXED clip.

## Gates on the patched tree

`make spike-test` (host tests 45 → **47**: lib 30 unchanged, carried
`e2` 1 / `e2b` 2 / `e10` 4, new `tests/e9_witness.rs` **2**, `spike_witness`
8), `make flac-test` (120 passed, 1 ignored — unchanged; no `crates/` file
touched), `make check` clean, `make native-spike-rom` builds; pre-commit
hook full suite green on the measurement commit.

## Re-application note (item 6a-2, 2026-11-06)

Re-applied at `b15b756` as part of that item's baseline re-anchor. `git apply
--check` is clean; the result does **not** compile — four `error[E0063] missing
field 'header_bits' in initializer of 'FrameHeader'`, at `src/e10.rs:1050`,
`src/e2.rs:481`, `src/e2b.rs:571` and `src/e9.rs:468`. Apply
**`port-item6a2-header-bits.patch` after** `e9-composite.patch`: this archive's
copies of `e10.rs`/`e2.rs`/`e2b.rs` are byte-identical to the E10 archive's, so
its port is that same four-line-per-parser port plus the `e9.rs` case. The port is
compile-only: with it reverted (`header_bits: 0`) the image's `.entrypoint`,
`.text` (87,224 B) and `.iwram` (17,996 B) are byte-identical and the run log is
unchanged — only `.rodata` moves, because it carries those parsers' panic-location
strings.

Re-measured, two runs byte-identical (1,357 lines / 121,107 B, `eq=1` 628/628, region FNVs
`[OK]`, `i32_fits=true iw_fits=true` on FIXED and `false` on LPC, `.iwram` scratch
`e9::scratch::{LEFT,RIGHT}` @`0x0300064c`/`0x0300264c`); tables in
[flac-docs/measurements.md](../../../../flac-docs/measurements.md). Versus the sums recorded above, measured here on the
post-item-5 tree:

| leg | FIXED Δ (total) | LPC Δ (total) |
|---|---|---|
| `ctrl` | +8,908 | **+66,412,024** |
| `arm` | −3,768 | **+66,399,348** |
| `i32` | **−5,024** | **−5,024** |
| `iw` | **−5,024** | **−5,024** |

That table is the single cleanest witness this item found: the two legs that run
the i64 LPC arithmetic pick up ~66.4M cycles, the two that deliberately do not are
unmoved to 5,024 (−32 cyc/frame, tree drift plus layout noise — this file's
existing ±9k band). Nothing about this probe changed; the LPC clip's *denominator*
did. It lands the way this record already read it — **additivity is a statement in
absolute cycles** — and the absolute marginals confirm it: `arm − i32` FIXED
12,330,961 → 12,332,217 (−78,542 → −78,550 cyc/frame, **−7.35% in both worlds**),
while the same margin as a *percentage* of LPC moves **−58.53% → −61.51%**
(−3,201,707 → −3,624,665 cyc/frame). E3's scratch pin reproduces exactly: **`iw −
i32` = +3,140 cycles total, +20.0 cyc/frame, both clips, both eras.**

So quote this probe's marginals forward, not its percentages; and never against a
live-decoder row — inside one image the live window and this probe's `ctrl` window
differ by a constant 1,727 cycles (11 cyc/frame), and adding this probe to an image
moves the live decode by a constant −631,208 cycles, both clips alike.
