# E8 sample — sub-stage attribution (measured 2026-10-12, **not integrated**)

The reappliable sample code for E8 in FLAC.md's "Perf follow-ups —
experiment menu": a **measurement, not a variant** — one image, one pass,
timed windows per frame partitioning the decode into sub-stages, plus
per-sample boundary reads splitting the Rice hot loop into `unary` (E2b's
addressable share) vs `rest` (remainder read + fold + guard + unmap +
store). **Result: FIXED's cost center is named — the Rice per-sample loop
carries 94.6% of the instrumented gross (unary 56.2% + rest 38.5%), every
other bucket ≤ 4.2%; the LPC arm's cost center is the i64 integrator
(49.5%).** Full findings, witnesses, and the menu consequences:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E8 result — sub-stage attribution names the cost centers (2026-10-12)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it.

## What the probe does

One pass per arm on one image. Per frame:

1. **ctrl** — PR 3's `driver::decode_one` in a `timed()` window, the
   same-run control (its stats reproduce the recorded baseline — the
   reading that makes cross-session comparison honest);
2. **instrumented decode** — `e8::decode_one_e8` inside one **gross**
   window (`RomMeter::open/close` — agb's critical-section token carried
   between open and close so stage windows nest inside), with:
   - **stage windows** (`Meter::timed`, PR 3's IME-off shape) at: header
     parse, subframe prefix, residual header + parameter reads, escape
     bodies, integrator, wasted-bit pad, decorrelate (live shared call),
     footer, misc (CONSTANT/VERBATIM bodies);
   - **boundary reads** (`Meter::now`, raw 16-bit low half) at two points
     per Rice sample — after the unary quotient loop and at sample end —
     splitting the hot loop into `unary` / `rest`. Raw reads, not windows:
     a `timed()` window costs ~22–35 cyc and two per sample would inflate
     the loop under measurement by ~96 cyc/sample; the back-to-back read
     pair measured 4 cyc, netted per span from its own calibration.
3. **equality witness OUTSIDE the windows** — the vendored decode must
   produce bit-identical samples to the proven control (first offender
   named); the attribution numbers only count on equal frames.

Three instrument shapes, three calibrations, each with its own
unified-loop stability witness (E4's shape: 3 discarded + 5 reported,
byte-identical iterations): stage window **w=35**, gross open/close
**g=37**, boundary pair **b=4** — stable, IME=0 inside every window. A
window nested inside an open gross costs more than the plain-path window
(35 vs 22–24): never carry a plain-path overhead constant into an
attribution probe. Closure fold per frame (gross − attributed parts)
stays positive (min 4,884 / 5,897 cyc — unattributed plumbing),
`underflows=0`, near-wrap tripwire `big_deltas=0` (the 16-bit wrapping
delta is exact well below 65,536).

Witnesses before any number counted:

- **Host** (`tests/e8_witness.rs`, runs under `make spike-test`): the
  vendored path decodes the exact embedded clips bit-exactly vs the
  `flac -d` reference PCM, twin-agrees frame-for-frame with the live
  `driver::decode_one`, and `NullMeter` witnesses the fold bookkeeping
  (per-bucket occurrence counts; `integ + misc == 2` per frame only as a
  total, per the shape-determined rule).
- **ROM**: decode proof first — asset pins + PCM fnv
  `0x54C7B356621B6E15` `[MATCH]` on both arms before any timing; then the
  per-frame e8-vs-control **sample-equality** (no FAIL lines; 157/157
  frames folded both arms).
- **Determinism**: two headless runs `cmp`-identical (703 lines).

## Files

| File | What |
|---|---|
| `e8-substage-attribution.patch` | `git diff` of the probe against `main` @ `eadb7ed` (PR #65 merge), touching `examples/flac_spike/src/{e8.rs (new), lib.rs, main.rs}` + `tests/e8_witness.rs` (new). Byte-identical to `git diff eadb7ed c9a7e77` (the measurement commit). Verified `git apply --check` clean at that commit. |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e8-substage-attribution/e8-substage-attribution.patch
make spike-test && make native-spike-rom
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e8_$i.log 2>&1 &
  P=$!; until grep -q "e8 summary" /tmp/e8_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e8_1.log /tmp/e8_2.log && echo IDENTICAL
grep -E "e8 (calibration|ctrl|bucket|closure|summary)" /tmp/e8_1.log
# undo:
git apply -R examples/flac_spike/experiments/e8-substage-attribution/e8-substage-attribution.patch
```

If the patch no longer applies cleanly (PR 4/5 move `main.rs`), take the
**gross-token + nested-windows + boundary-pair + equality-witness +
three-shape-calibration** method from the patch rather than forcing the
hunks; `e8.rs` itself is a standalone module (`Meter` trait seam,
`NullMeter` on host) and should port verbatim. The reading is per-bucket
**shares of the same-image gross** — never absolute c/sample from the
instrumented path (the meter's bookkeeping rides inside the gross).

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` 0.10.5, same host runner as PR 3 / E1 / E2a / E4 /
E6 / E7, runs 2026-10-12 (twice, `cmp`-identical, 703 lines), Rust nightly
1.100.0-nightly (a69a63265 2026-09-03). Measured ROM `flac-spike.gba`
sha256 `f940ac5ea372437d2fbcbce09f9fde249c799317c8e32bbf037ea4f7ac4dbc0a`
(rebuild on the measurement commit in the same tree is `cmp`-identical;
across trees judge the control by its stats, never the sha).

Net bucket sums (each net of its own calibration) and share of the
same-image **netted gross**; 157 frames / 320,000 samples per channel:

| bucket | FIXED (`l0_stereo`) | share | LPC (`l4_stereo`) | share |
|---|---:|---:|---:|---:|
| unary | 336,179,902 | **56.2%** | 424,518,364 | 31.2% |
| rest | 230,166,712 | **38.5%** | 228,346,862 | 16.8% |
| integ | 23,864 | 0.004% | 673,676,420 | **49.5%** |
| decorr | 24,974,758 | 4.2% | 24,974,758 | 1.8% |
| prefix | 430,690 | 0.07% | 858,738 | 0.06% |
| resid_head | 271,547 | 0.05% | 906,968 | 0.07% |
| hdr | 539,657 | 0.09% | 539,657 | 0.04% |
| footer | 21,980 | 0.004% | 21,980 | 0.004% |
| pad | 6,908 | 0.001% | 6,908 | ~0% |
| escape / misc | 0 (count=0) | — | 0 (count=0) | — |
| closure (unattributed) | 768,912 | 0.13% | 925,917 | 0.07% |
| **gross (netted)** | **598,570,556** | | **1,359,963,156** | |

`escape`/`misc` count=0: these clips contain no escape partitions and no
CONSTANT/VERBATIM bodies — the attribution covers every path the profile
encodes. Cross-arm free witness: `decorr` and `hdr` are byte-identical
sums/min/max across the two arms (same live shared calls, same frame
counts).

Control (the same-run baseline witness): FIXED sum=357,989,569
max=2,294,595@60 min=580,783@156; LPC sum=1,074,091,932 max=6,881,652@136
min=1,502,291@156 — worst-frame indices **exact** against the recorded
baseline (FIXED @60, LPC @136, min @156 both arms), means within ~+0.03%
(code layout — the vendored/instrumented build shifts the plain PR-3-shape
window by more than E2a/E6's +13 cyc/frame constant; the attribution
reading is shares, not the control absolute). Calibration w=35 g=37 b=4,
stable=true, IME=0. Tripwires: closure underflows=0, big_deltas=0,
resid_samples 640,000 (FIXED) / 638,746 (LPC), windows 2,041 / 2,355.

**Known quirk on this build (deterministic, harmless):** the *legacy*
whole-frame calibration pass printed first-window-cold raws (33, 24, 24,
24, 24) on both arms and voided itself ("FAIL calibration unstable" → the
PR-3 verdict line reads FAILED / screen RED, `measured 0/2`). That is the
recorded cold-first-window bus property (E1), triggered by the E8 build's
code layout; it voids only the legacy pass's own numbers. The E8 pass
carries its own stable calibrations (w=35/g=37/b=4) and its own control —
the attribution table above is unaffected.

Layout-confound evidence (`nm` on the built ELF): `e8::decode_residual_e8`
`0xba4` vs `flac_lite::residual::decode_residual` `0x574`;
`e8::decode_subframe_e8` `0xe2c` vs `flac_lite::subframe::decode_subframe`
`0x388` — instrumentation moves inlining layout along with itself (the
menu's E6 confound), stated in the FLAC.md entry; the reading is the
per-bucket shares with that caveat.

Gates green on the patched tree: `make spike-test` (host tests 23 → 28:
lib 15 unchanged + 2 `e8.rs` unit tests, +3 `tests/e8_witness.rs`,
`spike_witness` 8 unchanged), `make flac-test` (115 passed, 1 ignored —
unchanged; `crates/flac-lite/` untouched by the patch), `make check`,
`make native-spike-rom`.
