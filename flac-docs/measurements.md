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
| 6a-5 projected asm + i32 (**projection**: E9's same-image i32 marginal, not measured on this image) | **103.83%** (worst 104.36%) | 265.99% | 6a-5 + E9 marginal |
| 6a-5b: E10's vendored realization re-measured **as an additional arm of the product-shape image** — that pass's calibration line goes from `arms=2` to `arms=3` (instrument-only; nothing ships) | **101.95%** (worst 102.47% @147) | 561.93% | 6a-5b differential, `flac-spike-6a5b.gba` |
| 6a-5b: that arm + E9's i32 marginal (**projection**, same caveat as the row above) | 94.47% (worst 95.0%) | — | 1,069,252 − 78,541 = 990,711 c/frame — E9's in-budget row, reproduced from a landed-tree measurement of the arm instead of a probe's |

**None of the in-budget rows is landed code.** E9's FIXED stack exists as a
sample under `examples/flac_spike/experiments/e9-composite/`; the product path
still measures the 170.2% / 589.5% row. LPC at 216.3% fits 10,512 Hz (69.4%) and
misses 18,157 Hz (~119.9%) decode-only.

**And the one in-budget row does not survive landing.** 6a-5 measured the real
thing and projected the stack: the landed realization is +48.2 c/sample slower
than E10's vendored arm, which puts the FIXED stack at **103.83%** at 32,768 Hz —
over budget — where the probe projected 94.47%. The E9 record's own caveat (1)
("these numbers describe the probe's vendored link, and the kernel + i32 landing
must re-witness them") is now a measured negative; see the re-anchoring note at
the end of E9's entry in [`perf-results.md`](perf-results.md) and Q12 in
[`open-questions.md`](open-questions.md). The kernel itself still stands as the
menu's biggest lever (−286.9 c/sample landed), and both product rates fit with it
alone (61.7% at 18,157 Hz, 35.7% at 10,512 Hz).

**Re-opened 2026-11-08 (6a-5b): the +48.2 c/sample is a cross-image quantity.**
The projection's two inputs come from two different images, and this file's own
item-6 rows above show what that costs: `ctrl` moves −36,543 / −202,529 c/frame
across links carrying one and two `RiceKernel` bodies, with its source unchanged.
Four static facts on the current tree (the FIXED clip's geometry ⇒ 2 kernel calls
per frame, the bridge's ~114 instructions per call, the asm's 5-instruction
per-sample delta, and the tables at E9's own `0x0300032c/34c/44c`) leave no room for
48 c/sample of *code*. The 103.83% row is a projection whose error bar is larger
than its conclusion; `asm − vendored` read in one link (6a-5b) is what settles it,
and no sequencing decision should be taken on the row until then.

**Measured 2026-11-09 (6a-5b): the projection's number stands, its re-opening does
not.** The extra arm is in the instrument and the reading is a same-image pair:
**`asm − vend` = +98,291 c/frame (FIXED) / +98,153 (LPC) = +24.11 cycles per Rice
codeword = +48.22 / +48.16 c/sample**, the vendored leg faster on **157/157 frames
of both clips**, reproduced to +5.2 / +13.0 c/frame across runs. That lands within
**0.4%** of the +97,929 the projection drew from two records — and the reason it
lands is that both *arms* are link-stable across three separate images
(0.02–0.04% apart) while only `ctrl` moves with the link. The re-opening's premise,
that the overhang was an image effect, is refuted; the four static facts bound
*instruction counts*, and instruction counts are not the unit this cost is paid in.
What the differential is *not*: the bridge, the table placement, the ABI or the
state frame (all symmetric in this image, priced at zero by construction), and not
the frame path (the two clips differ 3.75× in per-frame Rust work and the per-sample
gap moves 0.14%). What it is: the per-codeword loop, of which the landed kernel's
own executed delta is 6 instructions — **24.9% of the gap if each costs one cycle,
74.7% if each costs three, and 4.02 cycles each would be needed for the kernel body
to be the whole of it**. Those two channels — instruction cost and hot-loop layout —
are separated by construction in **6a-5c**, whose first leg is the removal of one
provably dead instruction (the shipped asm carries `and r12, r3, #0xFF` twice, see
the table below): semantics untouched, layout moved, and the reading decides which
unit the cost is in. The 103.83% row is therefore re-admitted as the item's working
stack figure, with 6a-6's own measured i32 marginal still required before it becomes
a measurement.

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
| **6a-5 stack projection (the item's first measured negative)** | landed asm **183,288,727** vs E9's vendored arm **167,877,039** = **+15,411,688 / +98,164 c/frame / +48.2 c/sample (+9.18%)**; + E9's same-image i32 marginal (−12,330,961) projects **170,957,766 = 1,088,902 c/frame = 103.83% mean / 104.36% worst** of 32,768 Hz | the vendored stack's 94.47% does **not** reproduce through the landed seam; the seam costs 9.2 points against the stack's 5.5-point margin. i32 marginal to be re-measured by 6a-6; 18,157/10,512 Hz still fit (61.7% / 35.7% asm alone). **Re-opened by 6a-5b: cross-image comparison** |
| **6a-5b static bound (2026-11-08, no cycles)** | FIXED clip `MidSide`, both subframes **FIXED order 0, partition order 0**, `wasted 0`, Rice p = 8/10, **157/157 frames** ⇒ **2 kernel calls/frame**; seam bridge **~114 Thumb instr/call** ⇒ **≈0.15 c/sample**; asm delta **5 instr/sample** (G1+G2) ⇒ **≈5–15 c/sample**; tables at `0x0300032c/34c/44c` (E9's own addresses, `.iwram` 0x54c); `decode_residual_with::<ArmRiceKernel>` **2,104 B** vs `<TableCountKernel>` **1,812 B** | the overhang cannot be the bridge or the frame/status word; the only *proven* per-sample cost is the guards; the remaining share is image effect, which 6a-5b pairs away or confirms |
| 6a-5 determinism / layout | two headless runs of one arm image: 342 `***` lines `cmp`-identical; every *paired* delta reproduced to the cycle across a driver-layout change while the *absolute* sums moved +2 c/frame | paired deltas are the number of record; absolute sums are layout-sensitive at the 2 c/frame level |
| **6a-5b realization differential (measured)** | `flac-spike-6a5b.gba` sha `b6e35beb1e04028c…`, `rustc 1.100.0-nightly (a69a63265)`; four windows in one link, `overhead=33 stable=true ime_inside=0`; **`asm − vend` = +15,431,627 / +15,409,977 cycles** per 157-frame clip (FIXED / LPC) = **+98,291 / +98,153 c/frame = +24.11 c/codeword = +48.22 / +48.16 c/sample**; `vend_faster=157, asm_faster=0` on both clips, max +98,775 @147; run 2 differs by +820 / +2,035 cycles in total | the number of record for this rung. It confirms the 6a-5 projection and refutes its re-opening: the landed realization *is* 9.4 budget points of the 32,768 Hz frame worse than the probe's, in one image, on identical bytes |
| 6a-5b legs of the same pass | ctrl 275,126,433 (1,752,398 c/f, 167.09%) → asm 183,304,113 (1,167,542, 111.33%) → vend 167,872,486 (1,069,252, **101.95%**); `asm − ctrl` = −91,822,320 **cycle-identical to the 6a-5 record**; `vend − ctrl` = −107,253,947 (−683,146 c/f) | all three legs decoded to the pinned FNV `0x54C7B356621B6E15` and the per-frame sample-equality witness held, so the difference is realization only. The PR 3 budget verdict (0/2 in budget, screen RED) says no leg fits 32 kHz; it does not void the pair |
| 6a-5b arm absolutes are **link-stable** | landed asm 183,288,727 (product shape) / 183,231,371 (three-arm link) / 183,304,113 (this image) — 0.04% spread; E10's vendored arm 167,911,108 / this image's vend leg 167,872,486 — −0.02%; while `ctrl` spans 280,848,260 → 275,111,047 → 249,051,451 across links | the reading rule's exception ("absolute *paired* marginals") is symmetric: an absolute that reproduces across images is quotable across records. That is *why* 6a-5's cross-record projection came out right, and why 6a-5b was still the right way to settle it |
| 6a-5b machine-code delta, same image | `arm_rice_partition` 684 B / 171 words @`0x0800017c` vs `e10_rice_partition` 640 B / 160 words @`0x08000428`: +11 words static; **executed delta 6 per Rice codeword** — G1 `ldr [sp,#8] + cmp + bhi`, G2 `adds + beq`, and one **dead duplicated `and r12, r3, #0xFF`** — plus 3 per partition (`mvn + lsr + str`). `.Lvalerr` and the `mov r2, #2` arm are never executed on this corpus | 640,000 codewords × 6 + 314 partitions × 3 = **3,840,942 executed delta instructions per clip = 24.9% of the gap at 1 cycle each**, 49.8% at 2, 74.7% at 3; the kernel body accounts for the whole gap only at **4.02 cycles per delta instruction**. The gap is **24.11 cycles per codeword** — the unit to reason in |
| 6a-5b the units, pinned from the instrument's own line | asset verification prints the clip geometry: `rate=32000 ch=2 max_blocksize=2048 frames=157 samples=320000` — 156 frames of 2048 plus one of 512 = **320,000 samples per channel**, so **640,000 Rice codewords** per clip (one per sample per subframe), and on FIXED with partition order 0, **314 partitions** (2 subframes × 157 frames). The house convention divides a delta by 320,000 to get c/sample; per codeword is half that | 24.11 c/codeword, the 3,840,942 count and the 4.02 figure are all re-checkable from that one printed line, without re-deriving the clip's structure |
| 6a-5b landed-kernel byte identity across four images | the kernel's bytes at file offset `0x17c`, length 684, sha prefix **`38760f65855c`**, in: a base-commit (`9345232`) worktree build, the pre-change control (`6a30a3f5…`), the current-tree control (`262cdf0a…`) and the 6a-5b image. An earlier "the landed kernels differ" note was an extraction-offset error and is withdrawn | the differential is not an accidental codegen mismatch of the kernel under test; and an 112 B whole-image difference between two of those images coexists with a cycle-identical arm |
| 6a-5b a real defect in the shipped kernel | `crates/flac-lite-gba/asm/rice_partition.s` lines **256 and 258 are the same line**: `and r12, r3, #0xFF  @ lo = rem_w & 0xFF`. The [6a-4 G1] insertion moved that line above the guard and left the original in place. Emitted adjacently at `0x8000288 / 0x800028c`; `r3` is unchanged between them and neither sets flags ⇒ the first is **dead, once per codeword**, 640,000 dead executions per clip | the cheapest counted instruction on the table, and a single-variable probe for the layout channel. **Not removed by this record**: the kernel this record anchors must keep its bytes, and removing it is a build change that re-runs `overlay-kernel-test` and `arm_from_outside` |
| 6a-5b what is symmetric, so this image prices it at zero | the **same number of kernel calls per frame in both legs** — 2 on FIXED (partition order 0, 2 subframes), more on LPC (its partition orders vary per frame), equal in both legs either way — each call through a linker thunk (`__Thumbv4ABSLongBXThunk_arm_rice_partition`, `…_e10_rice_partition`); tables in `.iwram` in both (landed `0x0300032c/34c/44c`, vendored `0x0300054c/56c/66c`); the stack is IWRAM for both (the boot code loads `0x03007ffc`); `stereo::decorrelate` and `PredictorState` are shared by both | the bridge, the pin, the 12-byte frame and the state ABI contribute **nothing** to this differential — which is the static bound's one conclusion that survives. What the vend leg *does* carry that the landed leg does not: its own 32-bit-window `AccumulatorReader`, its own frame/subframe/residual/prediction code. "Realization" here means the whole per-codeword path, not only the asm body |
| 6a-5b the cost tracks codewords, not frames | +48.22 vs +48.16 c/sample on two clips whose control costs differ 3.75× (1,752,398 vs 6,572,815 c/f) and whose per-frame work is different code; both clips are 320,000 sample-frames = 640,000 Rice codewords | the regression is attached to the per-codeword loop, not to the frame path, the LPC work, or anything per-frame. Any candidate that scales per *call* has to survive the FIXED/LPC invariance, which the bridge (2 calls/frame in both) does only by being ~0. **The per-partition term is ruled out by the same measurement:** the LPC clip's partition count is *not* 314 — its partition orders vary frame to frame — yet the per-codeword cost moves 0.14% between the clips, so the only per-partition item in the diff (the `mvn/lsr/str` bound hoist) cannot be a meaningful share of a cost that does not scale with partitions |

## Determinism controls (why a zero is believable)

- Repeated headless runs of one ROM are byte-identical (`cmp`), including every
  `***` cycle line. 6a-5b re-took this on the three-leg image: 664 `***` lines,
  zero differences across two runs.
- **6a-5b bounds the recompile class on both sides at once.** Three images with
  different shas print *cycle-identical* arm and control numbers (the record's
  `de51bead…`, a pre-change rebuild and the current-tree rebuild `262cdf0a…`: 342
  `***` lines identical, sums identical to the cycle). Meanwhile *adding a fourth
  timed window* to the same source moved every absolute leg by exactly **+98
  c/frame** — every leg, min and max alike — while every paired delta came
  out cycle-identical. **And that offset is a constant, not noise:** every leg of
  both clips moved by exactly **+15,386 cycles** — l0 `ctrl` 275,111,047 →
  275,126,433, l0 `asm` 183,288,727 → 183,304,113, l4 `ctrl` 1,031,916,549 →
  1,031,931,935, l4 `asm` 940,629,832 → 940,645,218 — which is 98.0 c/frame, and the
  per-frame mins and maxes carry the same +98, and so does every one of the 314 frame-legs
  the two images print (314/314 read exactly +98). So the ~9–13 cyc/frame band is a band
  on *paired deltas*, and absolutes move ~8× past it from a change this small; an
  image's size (112 B, embedded absolute paths) says nothing about whether its arms
  moved. The mechanism is not mysterious and is worth naming as instrument
  mechanics, and the mechanism is not claimed here: the shift is a **property of the
  image, identical for every arm and every frame**, which is exactly why per-frame
  pairing cancels it to the cycle — and why an absolute arm compared between images can
  be off by a fixed per-frame amount that has nothing to do with the arm. The 6a-5
  record's `asm` absolute and this image's differ by 98 c/frame *for that reason*,
  not for a link reason. Why adding a leg shifts the other legs by a fixed 98
  cycles/frame is unexplained; it is bounded, deterministic and cancels.
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
- The retraction of 6a-5's stack projection, as a *retraction*. 6a-5b confirmed the
  projected figure to 0.4% because both arms reproduce across images. Quoting the
  projection is fine; quoting the re-opening is stale.
