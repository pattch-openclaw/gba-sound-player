# Measurements — the numbers of record, and what each one belongs to

This is the ledger. `FLAC.md` used to be the only place a cycle count could live,
so the same number was restated in a menu entry, a result section and a sample
README, each with a different denominator. Here every number appears once, tied to
the image that produced it. The experiment narratives are in
[`perf-results.md`](perf-results.md); the rules that make a run quotable are in
[`method-and-gates.md`](method-and-gates.md).

## How to read a cycle number here

1. **Same image or nothing.** A comparison is only valid between arms measured in
   one session from one image, with a same-run control. Cross-record comparisons
   are allowed only as **absolute paired marginals** (cycles/frame, cycles/sample),
   never as percentages of budget and never as "X% faster" against another
   record's denominator.
2. **Percentages are era-denominated.** Item 6a-2 re-ran E9's and E10's patches on
   the current base and the same patches moved LPC percentages by up to three
   points on an unchanged decode path. The percentages in older records are
   therefore *true of their image* and not transferable.
3. **Every row names its control, its image and its toolchain.** A ROM sha and a
   rustc commit hash are the identity of an instrument. A version string is not.
4. **The noise band is ~9–13 cycles/frame** — budgeted for *recompiled* layouts.
   A fixed image repeats to the cycle (proved; see below). A delta inside the band
   is not a mechanism.
5. **The tail counts.** Worst-frame numbers always name the arm and the frame
   index; a worst frame without a named arm is quoting a different frame.

## The current instrument

| Fact | Value |
|---|---|
| Toolchain | `nightly-2026-09-04` = `rustc 1.100.0-nightly (a69a63265 2026-09-03)`, LLVM 23.1.1 |
| Host | aarch64 (no ARM executor; mGBA is the ARM oracle) |
| Emulator | mGBA (the agb 0.25 `mgba-test-runner` harness) |
| Calibration overhead | 22 cycles (20 on the rate images), subtracted by the harness; raw reported beside net |
| On-target decode proof pin | PCM FNV-1a-64 `0x54C7B356621B6E15`, matched on both arms and cross-arm (157/157 frames each) |
| Budget | 1,048,750 cycles/frame (@2048 × 32,768 Hz) = 512 c/sample |

**Section table of the calibration instrument** (`examples/flac_spike`, the ROM
shape that links the decoder with default features — this is the `ctrl` arm of
every E record):

| Section | Size | sha-256 prefix |
|---|---|---|
| `.text` | **46,008 B** | `f0f88c33d150…` |
| `.rodata` | 1,829,760 B | host-dependent (carries absolute paths) |
| `.iwram` | 812 B | `eeda688aa1a2…` |
| `.ewram` | 368 B | `c8beeb9e3f5f…` |
| `.entrypoint` | 380 B | `7d6b62f63b70…` |

`nm` anchors: `LEADING_ONES @ 0x0300022c`, `rice_quotient` live at 336 B,
`decode_residual` 596 B, 172 symbols total. **`.rodata` content is not a codegen
signal** — a comment edit above a panicking call site moves one byte of the
panic-location table and nothing else. `.text` plus the `nm` anchors are the code
invariant.

Gate counts at the same baseline: `flac-test` 178 + 1 · `flac-test-crc` 181 + 1 ·
`flac-test-scan` 198 + 1 · `flac-test-rice` 194 + 1 · `crc-check,scan` 201 + 1 ·
`crc-check,scan,rice-hybrid` 217 + 1 · `make flac-gates` **1,169 passed + 6
ignored** · `spike-test` 15 + 10 · `kernel-test` 20.

## Baseline of record (the shipped decode path)

`l0_stereo` = `-l 0 -b 2048 -m`, `l4_stereo` = `-l 4 -b 2048 -m`, 157 frames each,
320,000 stereo samples, calibration subtracted:

| clip | mean c/frame | worst c/frame @ frame | min c/frame (short tail) | sum |
|---|---|---|---|---|
| FIXED `l0_stereo` | **1,788,842** | **1,797,752 @136** | 454,632 @156 | **280,848,260** |
| LPC `l4_stereo` | **6,609,128** | **6,643,224 @145** | 1,411,224 @156 | **1,037,633,086** |

These are the numbers item 5 PR 4's control arm reproduced and 6a-2 re-anchored
byte-identically. **They belong to an image in which exactly one `RiceKernel` is
instantiated.** Putting a second one in the link moves the *default* path by
≈168–199 K cycles/frame (see the 6a-3 rows below); read that before quoting these
against any arm.

The original Phase-1 spike numbers, for the historical record only (they predate
E2a/E2b): FIXED mean 2,299,968 = 220.7% of budget, LPC mean 6,860,987 = 658.0%
(2026-09-21).

## The E-series, one line each

Full narratives, controls and method: [`perf-results.md`](perf-results.md).

| id | Tested | Verdict | Headline (as measured on its own image) |
|---|---|---|---|
| **E1** | Stage ROM→RAM per frame | no collapse, ruled out | FIXED −4.7%, LPC −1.8%; copy cost eats it (net ≈2.5% FIXED) |
| **E2a** | Bit-reader refill accumulator | no collapse, but it fixed a later regression | FIXED −3.14%, LPC **+1.41%** |
| **E2b** | Run-length table quotient count | **big win** | FIXED −19.17% marginal same-image (−21.71% vs live control), LPC −10.89%; FIXED mean 217.4% → 170.2% |
| **E3** | Decode scratch in IWRAM | inert | +20 cycles/frame on both clips (measured inside E9) |
| **E4** | i64 → i32 intermediates | big win, LPC only | LPC **−46.9%**, FIXED −3.7% |
| **E5** | Emulator vs hardware arbitration | **never run** | — (the only thing that can turn "no real-time claim" into one) |
| **E6** | Hot decode *code* in IWRAM | **slower**, ruled out | FIXED +17.8% (217.4% → 256.1%), LPC +9.0% (652.3% → 711.0%) |
| **E7** | `-Cforce-frame-pointers` | inert | byte-identical ROM with and without |
| **E8** | Sub-stage attribution | measurement | FIXED: unary 56.2% + rest 38.5% = 94.6% of gross; LPC: i64 integrator 49.5% |
| **E9** | Stacked variants | additive; first in-budget FIXED | arm+i32 FIXED **94.47% mean / 94.95% worst**; LPC 216.3%; additivity 99.78% (LPC) / 92.0% (FIXED) |
| **E10** | ARM-mode hybrid codeword loop | **biggest FIXED-side win** | arm − tab **−40.08% FIXED / −11.51% LPC**; FIXED 170.2% → 101.98% mean, worst 102.50% |

The absolute marginals that survive an era boundary:

- **E10 `tab − arm` = −715,300 cycles/frame** (FIXED), ≈−349/−348 cycles/sample.
- **E9 `arm − i32` = −78,550 cycles/frame** (FIXED). Its LPC equivalent is *not*
  stable across denominators; use the absolute cycle pair from the E9 record.
- **Item 5 PR 4 `arm − ctrl` = +173,649 / +174,165 cycles/frame** (FIXED / LPC),
  i.e. **+42.60 / +42.72 cycles/sample** — the portable twin costs the same
  constant on a frame a quarter the size, 314/314 frames slower.

## Where the budget stands, per arm

| Path | FIXED mean | LPC mean | Source |
|---|---|---|---|
| Phase-1 baseline | 220.7% | 658.0% | spike PR 3 |
| + E2a/E2b landed | 170.2% | 589.5% | E2b |
| E10 ARM kernel as measured (probe) | 101.98% (worst 102.50%) | 521.6% | E10 result |
| E10 + E4 i32 stacked (probe) | **94.47% (worst 94.95%)** | 216.3% | E9 result |
| 6a-5 realized asm through the seam (landed code, product-shape image) | **111.32%** | 571.28% | 6a-5 result |

**None of the in-budget rows is landed code.** E9's FIXED stack exists as a
sample under `examples/flac_spike/experiments/e9-composite/`; the product path
still measures the 170.2% / 589.5% row. LPC at 216.3% fits 10,512 Hz (69.4%) and
misses 18,157 Hz (~119.9%) decode-only.

## 10,512 Hz, real music (rate ROM)

`bioluminescent.wav` (96 kHz / 24-bit master) normalised to 16-bit stereo at
exactly 10,512 Hz; live i64 path; budget 3,269,162 cycles/frame; 1,328 frames;
calibration 20 cycles:

| arm | mean c/f | mean c/s | mean % budget | worst c/f @ frame | worst % | verdict |
|---|---|---|---|---|---|---|
| FIXED `-l 0` | 2,417,429 | 1,180.4 | **73.9%** | 2,559,507 @505 | **78.3%** | FITS |
| LPC `-l 4` | 6,010,517 | 2,934.8 | **183.9%** | 6,916,436 @413 | **211.6%** | OVER |

Size facts: ≈1.91 MB/min compressed per arm; a 4:18 track embeds 8.2 MB/arm; the
two-arm ROM is 16,522,992 B. A 60-minute album is 151.4 MB of raw stereo at
10,512 Hz alone, so album-scale content forces mono and/or a measured compression
ratio.

## Playback numbers of record

See [`playback-results.md`](playback-results.md) for the dated log.

| Fact | Value |
|---|---|
| Audible playback at 10,512 Hz | correct tempo and pitch (Sam, 2026-10-27) |
| Pushed = consumed | 2,718,538 samples exactly, **underflows 0** |
| Convert stage, banked | 1.8 ms |
| Word-copy push | CS window 12.7 ms → **≈0.9 ms** |
| Live-producer coexistence (feeder off/on) | **+1.51%** total, not +39% |
| E2b landing on the unpatched reader | **+37.96%** FIXED (fixed by landing E2a alongside) |
| Decode at 10,512 Hz, E2b-only (the +37.96% bisect point) | 3,335,100 cycles/frame ≈ 199 ms of decode per 195 ms of audio |
| Decode at 10,512 Hz, FIXED rate-ROM mean | 2,417,429 cycles/frame ≈ 144 ms |

## Item 6 / 6a numbers that constrain how anything else is read

| Fact | Value | Why it matters |
|---|---|---|
| 6a-2 re-anchor | byte-identical image; cycle rows reproduce item 5 PR 4 **to the cycle** | the instrument is the same instrument |
| 6a-2 era gap | **+66,403,116 cycles** constant, LPC i64 legs only, all i64 legs in both archives, no FIXED clip | percentages across records are not comparable; the era's binary is gone |
| 6a-3 crate boundary | `.text` 46,008 B unchanged; overlay contributes **0 bytes** to any ROM that does not name it | the split costs nothing until used |
| 6a-3 second mechanism *present, unreachable* | **−26.5 M / −26.7 M cycles** on FIXED/LPC; worst frame 136 → 147 | a second `RiceKernel` body in the link re-decides inlining for the shared decode path |
| 6a-3 made reachable | a further **≈4.4–4.8 M cycles** | same effect, profitable side |
| 6a-3 `.iwram` with a second mechanism | 812 → **1,324 B (+512)**; `LEADING_ONES` *relocates* to `0x0300042c`, bytes unchanged | table placement is a property of reachability, not of the attribute; never hard-code `0x0300022c` as a scheme constant |
| 6a-3 attribution | `decode_residual` +1,888 / +1,216 B, `decode_subframe` +688 B, a second `read_bits` body, `integrate_lpc` +20 B | the size change is real codegen, and its *cause* is still an open question (Q3) |
| 6a-4 default configuration | `.text` 46,008 B `f0f88c33d150…`, `.iwram` 812 B `eeda688aa1a2…`, `nm` anchors unchanged | the kernel moved **no** shipped path; `rice-arm` is off everywhere the product builds |
| 6a-4 instrument image (`overlay-kernel-check.gba`, `rice-arm,iwram-pin`) | `.text` 67,772 B `8d8c7fe85a14…`; `.iwram` **1,356 B** = 812 + **544**; `.rodata` 145,988 B; 243 symbols | the three tables' budget was exact; `LEADING_ONES` keeps `0x0300022c`, tables at `0x0300032c/0x0300042c/0x0300052c` |
| 6a-4 kernel body | `arm_rice_partition` **684 B** at `0x0800017c`, ARM state, 171 instruction words, one `mul`, 22 base+register `ldrb` | the ELF fact a green sweep cannot show (`overlay-objdump`) |
| 6a-4 device sweep | corpus 11 partitions / 1,015 samples; 5,524 synthesized clean cases / 6,166 samples, 2,648 above order 16 delegated; 11 guard rejections at orders 12–16 + 30 with 10 near-miss controls; `p = 0` 14 codewords; 5 end-of-stream shapes | the asm, the model and the incumbent agree on ARM7TDMI |
| 6a-4 end-of-stream cursor | **3 of 5** arms leave the cursor where the incumbent does not | permitted by the seam (per-read atomicity only) and ratified by the probe; reported, never asserted to be zero |
| 6a-4 ROM memory limits | §9.2.7.3 guard regions at orders 12 (131 KB) and 13 (65 KB) and the `p = 0` extreme (2 MB) do not fit EWRAM (256 KB) | those three cases are host-only and the device suite counts them |
| **6a-5 realization, product shape** | asm − ctrl = **−91,822,320 / −91,286,717 cycles** per 157-frame clip = **−584,856 / −581,444 c/frame = −286.9 / −285.3 c/sample** (FIXED / LPC); 157/157 frames faster each clip | the item's number of record; the fork's `faster` side by 4 orders of magnitude |
| 6a-5 same image | ctrl FIXED 275,111,047 → asm 183,288,727 c/frame-sum (1,752,299 → 1,167,444 c/frame = 167.08% → **111.32%** of the 32,768 Hz budget); LPC 1,031,916,549 → 940,629,832 (626.72% → 571.28%) | FIXED still over budget at 32 kHz; 6a-6's width leg is the next lever |
| 6a-5 arm cost is link-independent | asm sum 183,288,727 (product shape) vs 183,231,371 (three-arm link): **0.03%** | the arm is a property of the kernel; the control is not |
| 6a-5 control is *not* link-independent | ctrl 280,848,260 (shipped) → 275,111,047 (one extra mechanism) → 249,051,451 (two) = **−36,543 / −202,529 c/frame** | a second `RiceKernel` body re-decides inlining; "three arms in one image" can only be a paired same-image reading (Q2, answered) |
| 6a-5 three-arm link | asm − ctrl = −65,820,080 / −65,404,651 cycles (−419,236 / −416,590 c/frame) | the same win, measured in a link that also carries the Rust mechanism — smaller because the control moved, not because the arm did |
| 6a-5 Rust cross-check | rust absolute 1,920,940 c/frame vs item 5 PR 4's implied 1,876,087 (**+2.4%**); paired delta **+164.2 / +163.9 c/sample** vs PR 4's +42.6 / +42.7 | the absolute cost travels; the paired delta does not, when the control moves. Do not "correct" PR 4 |
| 6a-5 vs E10's probe | landed asm 1,167,444 c/frame vs E10's 1,069,515 (101.98%): **+97,929 c/frame = +47.8 c/sample**, keeping **82%** of E10's −349 c/sample win | the seam's window bridge + the two §9.2.7.3 guards + the 12-byte frame; the split between them is unattributed (6a-8 does not run — far outside the band) |
| 6a-5 `iwram-pin` marginal (asm) | tables in ROM 202,770,217 → pinned 183,288,727 c/frame-sum = **−124,086 c/frame / −60.9 c/sample** | item 5's data-pin claim, re-measured on this kernel |
| 6a-5 codegen, product shape | `.text` 46,008 → 63,124 B, `.iwram` 812 → **1,356 B** (812 + 544), `.rodata` 1,829,760 → 1,830,876 B; the ROM harness itself is +8,584 B of the `.text` | an instrument image; the decode-path share is ~4.1 KB (arm residual/subframe/driver instantiations + the 684 B kernel), not the whole delta |
| 6a-5 determinism / layout | two headless runs of one arm image: 342 `***` lines `cmp`-identical; every *paired* delta reproduced to the cycle across a driver-layout change while the *absolute* sums moved +2 c/frame | paired deltas are the number of record; absolute sums are layout-sensitive at the 2 c/frame level |

## Determinism controls (why a zero is believable)

- Repeated headless runs of one ROM are byte-identical (`cmp`), including every
  `***` cycle line.
- The docs-only image in item 5 PR 4 held the instruction stream (byte-identical
  `.text`/`.iwram`/`.ewram`/`.entrypoint`; one byte of `.rodata`, the panic-location
  line number) and reprinted bit-identical timing: same sums, min/max, frame
  indices. **Upper bound on what a re-run of a fixed image can hide: zero.**
- That control bounds *re-runs*, not recompiles. The ~9–13 cyc/frame band exists
  for recompiled layouts and is still an assumption, not a measurement.

## What not to quote

- Percent-of-budget figures from a different image than the arm you are comparing.
- Any LPC percentage in a record older than 6a-2 without the absolute cycle pair.
- `0x0300022c` as a fixed table address.
- A worst-frame number without its arm and frame index.
- Any host-side performance claim: `flac-portable-check` is a compile gate.
