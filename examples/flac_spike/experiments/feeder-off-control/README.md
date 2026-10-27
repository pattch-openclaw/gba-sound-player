# Feeder-off control probe — A/B/C arms on the rate-play ROM (measured 2026-10-26, not integrated)

The reappliable sample code for FLAC.md ordering step ③ (rate-play entry,
2026-10-22 revised ordering): split the decode path's claimed **+39 % live
penalty** (1,643 c/s IRQ-live vs the rate ROM's recorded IME-off 1,180 c/s)
into handler CPU, sound-DMA bus contention, and everything else — with
three arms in **one image, one producer loop, one variable** (sound
activity). **Result: the live path costs ≈ +1.5 % (≈ 3 ms/frame), not
+39 %; the +39 % was a stale comparator — the committed rate ROM on the
current tree prints FIXED mean 1,614.6 c/s, and a bisect convicts the E2b
`rice_quotient` landing (`e455dba`, 2026-10-21) with +37.96 % decode.
Full findings: [`../../../../FLAC.md`](../../../../FLAC.md) →
"Completed: rate-play ROM" entry, 2026-10-26 amendment.**

## Arms

Same loop, same position-clock windows, same buffers; `drain` is a uniform
in-loop check shared by all arms (layout constant):

- **A — live baseline:** shipped shape. Timer1 IRQ feeder + both sound DMAs
  (prefill → arm → play). Also carries the shipped period audit.
- **B — feeder off:** the timer chain keeps running as clock only (timer1's
  IRQ latch bit masked — agb `set_bits` on T1CNT bit 6, verified 0.25.0: no
  latch, no delivery), no sound DMA. The producer drains the ring
  synchronously between frames via `take_chunk` — strictly **outside**
  every measured window, so flow control never spins and no IRQ consumer
  exists. (Design detail the plan left open — the ring holds ~3 frames;
  in-loop draining beats over-reading the ring and keeps the ledger real.)
- **C — dma:** B plus both sound DMAs armed against the static ping-pong in
  repeat mode. The paired FIFOs drain at the sample rate; every half-empty
  request re-triggers a 44-word burst from the same latched source (the
  repeat behavior the period audit documents) — bus traffic with zero
  handler CPU. No software involved in sustaining it.

Instrument: the 2026-10-22 sample-resolution position clock (timer2
periods × 176 + in-period samples; one tick = 2^24/10512 ≈ 1,596 cycles).
Method ported from `../playback-attribution/` — the old hunks do not apply
(#77/#78 changed `push`/convert), the window discipline does: calibration
pairs 0/0, closure underflows 0, division-free sums, nothing logged between
boundaries.

Arm-equality witness: FNV-1a-32 fold of every decoded window (L then R,
frames 0..N once per arm), computed **outside** the measured spans — the
arms must prove they decoded the identical samples while their stage
numbers differ.

Quiesce between arms: mask timer1's latch bit, write-1-clear the Timer1 IF
bit (bit 4), stop both DMAs — without the IF clear one late handler
dispatch would re-arm the DMAs and bleed sound activity into B.

## Re-apply / run / compare

Base commit: `5a27435` (PR #78 merge, current main).

```bash
git apply examples/flac_spike/experiments/feeder-off-control/probe.patch
make rate-assets INPUT=assets/sound/bioluminescent.wav RATE=10512
make native-rate-play-rom
shasum -a 256 flac-rate-play.gba    # expect 8ccb3dde… (provenance only)
(mgba-test-runner flac-rate-play.gba > run.log 2>&1 &)   # runner hangs by design
# poll: grep -q 'verdict' run.log  then:  pkill -f "mgba-test-runner flac-rate-play.gba"
git checkout -- examples/flac_spike/src/rate_play_main.rs
```

Input `bioluminescent.wav` sha256
`8f35fa88b3859e669b597f50c5360378811e02111b6ce50cadbe4f50b8145442` (the
ROM logs it at boot; the staged assets must reproduce from it).
Compare the **log**, not the sha (rebuild sha drift is pre-existing; logs
stay `cmp`-identical). Two runs must be `cmp`-identical before any number
counts.

## What a faithful re-run reproduces (measured image printed these)

ROM sha256 `8ccb3dde81e4de8dc3d567f18ad995ff436e1688c965b79edc1b26870c48eae5`
(rustc 1.100.0-nightly `a69a63265`, mGBA 0.10.5; two runs
`cmp`-identical — `run1.log`/`run2.log` here). Stage sums in **sample
ticks**, division-free (means host-side, ÷ 1,326 A-frames / 1,328 B,C):

| stage | A sum | A max@idx | B sum | C sum |
|---|---|---|---|---|
| wait | 13,125 | 1,871 @1321 | 132 | 126 |
| decode | 2,796,166 | 2,147 @572 | 2,758,630 | 2,764,749 |
| convert | 95,182 | 74 @183 | 94,117 | 94,349 |
| push_cs | 12,081 | 12 @957 | 11,926 | 11,958 |
| gross | 2,916,819 | 2,229 @572 | 2,865,120 | 2,871,455 |

Means (ticks/frame): decode A 2,108.72 / B 2,077.28 / C 2,081.89 →
1,643.3 / 1,618.8 / 1,622.4 c/s. **decode A−B = +31.4 t = +2.99 ms =
+1.51 %; C−B = +4.6 t = +0.22 %; A−C = +26.8 t.** Witnesses: `cal=0/0` all
arms; `closure underflows=0` all arms; `clock_retries=0`; equality line
`hash A=B=C=3E765D9D samples 5437076/5437076/5437076 [OK]`; A's decode
mean reproduces the 2026-10-22 map (2,108.72 vs 2,108.61 t, +0.006 %);
A gross 2,199.7 t = 209.3 ms meets #78's clean-image 209.5 ms audit wall
within 0.2 %. (A's decode worst moved 564→572 with max within 1 tick —
top frames within micro-layout noise across the #77/#78 image shift; per
the E2a precedent the mean is the reading.) A's ledger/audit healthy
(replays 0, `late=0`, `push_swallowed=71`); A underflows 2,107 reflect the
probe image's ~11.8 ms/frame instrument+fold weight (17,554 hw periods vs
the clean 16,618), not a seam failure. The convert stage reads 6.83 ms —
below the #78 amendment's residual-convert arithmetic (≈12 ms); the
position-clock window on the measured image is the stage authority.

## Rate-ROM bisect logs (`rom-*.log`)

Same host, same session, two `cmp`-identical headless runs per point;
staged-asset censuses identical (the generator only added FIFO8 pin
constants after `e6577e0`; compressed region bytes census-identical; both
arms' PCM FNV pins MATCH at every point). FIXED means
(`*** bioluminescent_l0 stats: … sum=`):

| log | commit | sum | mean c/f |
|---|---|---|---|
| `rom-e6577e0-run1.log` | `e6577e0` (2026-10-17 rate-ROM landing) | 3,210,345,896 | 2,417,429 — **reproduces the recorded baseline exactly** (mean, sum, worst 2,559,507 @505, min 28,082 @1327) |
| `rom-e455dba-run1.log` | `e455dba` (2026-10-21 — E2b `rice_quotient` landing + fold hoist) | 4,429,012,975 | 3,335,100 (**+37.96 %**) |
| `rom-1ee3033-run1.log` | `1ee3033` (LEADING_ONES `.iwram` pin) | 4,391,245,301 | 3,306,661 (pin recovers 0.85 %) |
| `rom-current-run1.log` | `5a27435` (main, post-#77/#78) | 4,391,245,301 | 3,306,661 (= `1ee3033` exactly; playback-only PRs invisible by construction) |

ROM shas: `2a72ab1c…` / `51a94f0e…` / `a79a154b…` / `1a53e4b4…`
(provenance only — the log `cmp` is the determinism witness). LPC means:
6,010,516.6 → 6,925,877.3 c/f at `e455dba` (+15.2 %); current main
6,897,466.8 (+14.8 %) — the pin recovered 0.4 % of the LPC side. The
staged-asset censuses print byte-identically at every point (frame bytes
8,225,487 / 8,101,690, same per-slot counts) and both arms' PCM FNV pins
MATCH in every bisect log.

## Reading (settled here)

- Handler + IRQ delivery ≈ +26.8 t/frame; sound-DMA fetch+contention
  (marginal) ≈ +4.6 t/frame; total live coexistence cost ≈ 3 ms/frame.
  The +39 % "live penalty" was the decoder moving under the comparator,
  not the feeder.
- E1 staging is **not** re-valued by the contention theory (the entire
  marginal cost of adding sound-DMA traffic to a read-heavy decode is
  +0.22 % on this emulator); a staging arm was cut before running on
  exactly that measurement.
- The hardware delays Sam heard are fully explained by the 209.5-vs-194.8
  ms/frame debt; no contention term is needed to account for them.
- The regression fix (revert or re-shape the landed Rice path) is the
  project's biggest lever: e6577e0 decode live-adjusted ≈ 146 ms/frame ⇒
  derived producer gross ≈ 155 ms ≈ 80 % of budget (**derived, not
  measured** — the fix PR measures it).
