# Playback attribution probe — live-producer sub-stage map (measured 2026-10-22, **not integrated**)

The reappliable sample code for the live-producer attribution on the
rate-play ROM (PR #76 terrain): one image, one pass, the producer's own
loop partitioned into **wait / decode / convert / push-CS** at
sample-resolution, per frame, IRQ-live. **Result: decode alone is
103.0 % of the 194.8 ms frame budget on the live path (200.6 ms mean,
1,643 c/sample — +39 % over the rate ROM's IME-off 1,180 c/s); the push
critical section is 12.7 ms (not the "~µs" claimed by the hoist); convert
is 14.1 ms; wait is 0.9 ms.** Full findings and the revised ordering:
[`../../../../FLAC.md`](../../../../FLAC.md) →
"Completed: rate-play ROM" entry, 2026-10-22 attribution amendment.

This directory is **sample code only**. The probe never lands on the live
spike sources: `src/rate_play_main.rs` in the PR is byte-identical to the
base commit below with the patch *not* applied.

## Why a different instrument than E8

E8 measured one decode pass inside IME-off `timed()` windows. The question
here is the **wall-time composition of the IRQ-live producer** — decode
running under the feeder, the convert with IRQs live, and the push CS with
IME off — so IME-off windows would measure a different bus state than the
path under study (the module docs' own rule: this ROM's numbers without
window discipline are system cost, never comparable to the E-menu's
windows). The instrument is instead the already-armed hardware chain:

- **Position clock**: `timer2` (cascades from timer1 = hardware periods,
  keeps counting with IME off) × CHUNK + the in-period sample count from
  `TM1_DATA`. Position = periods × 176 + samples-into-period, wrapping
  u32 (the full-track pass spans ≈ 3.1 M samples ≪ 2^32). One tick = one
  sample slot = 2^24/10512 ≈ **1,596 cycles** — every span is read in
  units of 1596 cycles, so the micro-window layout sensitivity (±4 cyc)
  that matters at cycle resolution is 4 orders below the reading.
- **Latch race**: tm2 → tm1 → tm2 re-read; a period boundary between the
  reads retries (`clock_retries` witnesses it: 2 across the pass).
- **Boundaries, not windows**: one clock read at each producer stage
  boundary (wait→decode→convert→CS-entry→CS-exit→frame end), the E4
  unified-loop discipline (3 discarded + 5 reported back-to-back pairs
  calibrate, nothing logged between). The calibration pairs measured
  **0/0 (sum/max)** — the instrument's own cost is far under one tick.
- **Closure**: per frame `gross − (wait+decode+convert+cs) ≥ 0`
  (telescoping on consecutive shared boundaries); the pass summed 321
  samples of inter-boundary gap over 1,326 frames (0.02 ms/frame) and
  **zero closure underflows**.

## Re-apply / run / compare

Base commit: `1ee3033` (PR #76 tip, branch `feat/playback-fix-e2b`).

```bash
git apply examples/flac_spike/experiments/playback-attribution/probe.patch
make rate-assets INPUT=path/to/bioluminescent.wav RATE=10512   # staged assets are temporary
make native-rate-play-rom
shasum -a 256 flac-rate-play.gba        # expect cb682591… (provenance only)
(mgba-test-runner flac-rate-play.gba > run.log 2>&1 &)          # runner hangs by design
# poll: grep -q verdict run.log   then:  pkill -f "mgba-test-runner flac-rate-play.gba"
git checkout -- examples/flac_spike/src/rate_play_main.rs       # revert the probe
```

The input is `bioluminescent.wav` sha256
`8f35fa88b3859e669b597f50c5360378811e02111b6ce50cadbe4f50b8145442`
(the ROM logs it at boot; the staged assets must reproduce from it).
Compare the **log**, not the sha: same-tree rebuilds can drift sha while
logs stay `cmp`-identical. Two runs must be `cmp`-identical before any
number counts.

## What a faithful re-run reproduces (the measured image printed these)

ROM sha256 `cb682591466098a1f1f48b983b60b93472c9c9c41672057c1bd4d755607e2159`
(built with `rustup run nightly` / rustc 1.100.0-nightly a69a63265 2026-09-03,
headless mGBA 0.10.5; two runs `cmp`-identical — `run1.log`/`run2.log`
here). Stage sums in **sample ticks**, division-free (means derived
host-side, ÷ 1,326 attributed frames):

| stage | sum | max @idx | mean/frame | ms | % of budget |
|---|---|---|---|---|---|
| wait | 12,390 | 1,795 @1321 | 9.34 | 0.89 | 0.5 % |
| decode | 2,796,018 | 2,146 @564 | 2,108.6 | 200.6 | **103.0 %** |
| convert | 196,874 | 150 @4 | 148.5 | 14.1 | 7.2 % |
| push_cs | 177,148 | 135 @73 | 133.6 | 12.7 | 6.5 % |
| gross | 3,182,751 | 2,429 @572 | 2,400.3 | 228.3 | 117.2 % |

Budget derived: 2,048 samples ÷ 10,512 Hz = **194.8 ms = 3,268,621
cycles**; ms = samples × 1000/10512. Witnesses in the log: `cal=0/0`,
`clock_retries=2`, `closure: sum=321 underflows=0`; audit-derived wall
(18,121 hw periods × 16.74 ms ÷ 1,328) = 228.3 ms **equals** the measured
gross; `probe: frames=1326` (prefill excluded by design — the clock reads
0 until the timers arm); feed/audit lines `underflows=2674`,
`push_swallowed=1007 in 1,007 pushes`, `late=871 max_delay_ticks=132`,
`coalesced=0`, `verdict FAILED — screen RED`. The probe image is ≈ 5 ms/
frame heavier than the clean pinned image (223 ms): eight clock reads +
tally stores per frame — judge shares relatively, and the clean image's
audit stays the wall-time authority.

The wait outlier (frame 1321, 1,795 samples ≈ 171 ms) is a single
near-tail stall, not a trend — the wait mean is 0.9 ms.
