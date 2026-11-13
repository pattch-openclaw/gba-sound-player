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
| Budget | 1,048,750 cycles/frame (@2048 × 32,768 Hz) = 512.08 c/sample — the instrument's **rounded-clock** budget (`stats.rs::GBA_CLOCK_HZ = 16_780_000`), which is what every `verdict:` line in the ROMs this ledger reports compares against. The exact `Hz32768` period on the GBA's 2^24 Hz clock is 2^20 = **1,048,576** = exactly 512 c/sample; the two differ by 174 cycles (0.0166%), which shifts a published share by ≤0.03 pp and changes no verdict. Reconciliation in `method-and-gates.md`, "The real-time budget" (corrected 2026-11-11). |

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
| **E5** | Emulator vs hardware arbitration | **measured 2026-11-10** | GBA 001 + GBA SP: ROM 4.085 / IWRAM 1.037 cycles/add, ratio **3.939**; mGBA reproduced within 2 cycles/call. The fetch pricing is hardware. |
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
| **6a-5c L1: the landed asm with the dead `and` deleted** (a real fix; `overlay-kernel-test` + `overlay-test-arm` + `item6a5b-test` green) | **109.77%** (1,151,236 c/frame) | 561.6% | `flac-spike-6a5b.gba` sha `77a63e2b506d0bd9…`, same-image pair |
| 6a-5c L2: the same body with G1+G2 also deleted — **diagnostic only, cannot ship** (the device rejection arms fail by construction) | 102.00% (1,069,708 c/frame) | — | same image, `arm_rice_partition` 165 words |
| **6a-5d: the landed asm with its body in IWRAM** (shipped overlay config; `overlay-kernel-test`, `overlay-test-arm`, `item6a5b-test`, both objdump gates green) | **48.29%** (506,413 c/frame; worst frame 48.5%) | 508.8% | `flac-spike-6a5b.gba` sha `232d1498…`, same-image pair |
| 6a-5d diagnostic: *both* legs pinned, so the pair prices code and not memory | 48.29% | — | same image plus the vendored body in `.iwram`; `asm − vend` = +4,549,115 |

**None of the *probe-era* in-budget rows is landed code** — E9's FIXED stack exists
as a sample under `examples/flac_spike/experiments/e9-composite/` — and the shipped
product path still measures the 170.2% / 589.5% row **(that sentence is the state as
of this record; the current product rows are in this ledger — 37.765% mean /
37.974% worst on hardware through the real manifest, 6b-2 pt 2, with its mGBA twin and
the `main` byte re-read next to it)**. **The 6a-5d row is different in
kind and that is the item's status change:** 48.29% is the *landed overlay kernel*
(`rice-arm`, all gates green, byte-identical across rebuilds), measured through the
product-shape seam; what keeps it out of the product is only that no product path
names `rice-arm` yet (6b's one-line placement), not that the code is a probe. LPC at
216.3% fits 10,512 Hz (69.4%) and misses 18,157 Hz (~119.9%) decode-only.

**And the one in-budget row does not survive landing.** *(True of E9's row; 6a-5d
replaced it with a landed 48.29% — see the table and the 6a-5d narrative.)* 6a-5
measured the real thing and projected the stack: the landed realization is +48.2 c/sample slower
than E10's vendored arm, which puts the FIXED stack at **103.83%** at 32,768 Hz —
over budget — where the probe projected 94.47%. The E9 record's own caveat (1)
("these numbers describe the probe's vendored link, and the kernel + i32 landing
must re-witness them") is now a measured negative; see the re-anchoring note at
the end of E9's entry in [`perf-results.md`](perf-results.md) and Q12 in
[`open-questions.md`](open-questions.md). The kernel itself still stands as the
menu's biggest lever (−286.9 c/sample landed), and both product rates fit with it
alone (61.7% at 18,157 Hz, 35.7% at 10,512 Hz) — and it became a *larger* lever
than this paragraph knew: 6a-5d's IWRAM placement makes the landed arm 48.29% of the
FIXED budget, which is 2.9× the kernel cost this paragraph is describing.

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
a measurement. *(Since superseded: 6a-5c priced the gap in instructions and 6a-5d
found they cost four cycles because of where they lived. 48.29% is the number now.)*

**Measured 2026-11-09 (6a-5c): six instructions, 4.0000 cycles each, and the residue
is 0.46%.** The separation 6a-5b asked for came out neither way it was posed. Two
legs, each run twice and `***`-identical, both bit-exact on every frame: deleting the
provably dead `and` moved `asm − vend` by `2,560,000 = 4.000 × 640,000` cycles
(FIXED) / `2,554,984 = 3.992 × 638,746` (LPC), and deleting the five guard
instructions on top of it moved it by `12,800,000 = 5 × 4 × 640,000` exactly,
leaving **71,627 of 15,431,627 (0.46%)** at 456 / 510 c/frame — per call, not per
codeword, of the order the static bound gave for the bridge. So the currency is
**instruction count** and its price is **4 cycles**, which the whole gap confirms
independently: `6 × 4 × 4096 = 98,304 c/frame` against 98,291 measured. Layout is
*excluded*, not merely unproven: the saving is exactly 16,384 cycles on every one of
the 156 frames of 2048 samples and exactly 4,096 on the single 512-sample frame of
each clip — proportional to codewords executed — and the untouched vend leg prints
the same sums to the cycle in all three images although its own body moved 4 bytes
earlier. Four cycles is the ARM7TDMI's fetch cost for a 32-bit ARM instruction in
cartridge ROM on a 16-bit bus at WS0 = 3,1; **E6's "instruction fetch is ruled out"
is reversed for this case**. Consequences, in budget points of the 32,768 Hz frame:
the dead instruction was worth **1.56** (111.33% → 109.77% for the landed arm); each
further instruction is worth **1.56**; all six would be worth 9.33 and would reach
102.00%, an upper bound no shippable kernel can reach because G1/G2 must exist in
some form. The item therefore has a plain instruction budget in front of it — and a
larger question behind it, since *every* instruction in the loop, deletable or not,
pays the same 4 cycles: **6a-5d**, the same stream in IWRAM.

**Measured 2026-11-09 (6a-5d): the placement *is* the item's headline, and the
"realization" differential was mostly a memory-location artifact.** One directive —
`.section .iwram,"ax",%progbits` in place of `.text` — moves the same 680 B of
instruction stream from `0x0800017c` to `0x0300022c` and takes the landed arm from
**1,151,236 to 506,413 c/frame** on the FIXED clip: **−644,824 c/frame =
−316.4 c/sample = −158.2 c/codeword**, i.e. **109.77% → 48.29%** of the
32,768 Hz frame budget (worst frame 509,121 = 48.5%), with LPC going 569.7% →
508.8%. The `ctrl` leg is cycle-identical to the pre-pin image and `.text` / `.iwram`
move by exactly the body's 680 B, so nothing else in the image changed. Two runs are
`***`-identical, and an independent rebuild of the same configuration reproduces the
run to the cycle. The device witness (`overlay-kernel-test`, 8 arms on mGBA's
ARM7TDMI, the §9.2.7.3 rejection cases included) and the host contract
(`overlay-test-arm`) are green with the body in IWRAM.

**Why it was worth 2.9× rather than a few percent.** 6a-5c had already priced an
instruction at 4.000 cycles *as fetched from cartridge ROM*. IWRAM is 0 wait states
and 32 bits wide, so the same instruction costs about 1.4 — and the two kernels
show it independently. The landed body's 6-instruction guard delta falls from
**20.11 to 7.11 c/codeword**; the vendored body pinned on its own falls
167,872,486 → **74,957,768** (−591,813 c/frame = −290.4 c/sample). With both
bodies in IWRAM the pair reads `asm − vend` = **+4,549,115 = +14.2 c/sample**,
against **+48.2 c/sample** with both in ROM.

**That last number is the finding that re-writes the previous three rungs.** The
48.2 c/sample differential was never the seam (6a-5b priced it at zero, correctly),
never the ABI, the tables, the state frame, or even the guards' *semantics*: it was
**6 instructions multiplied by the difference between a ROM fetch and an IWRAM
fetch**, paid by *both* realizations wherever they happened to live. The residual
14.2 c/sample is the guards at IWRAM prices — real, and the only part a cheaper
G1 spelling (L3) can still buy back. So the item's earlier framing, "the landed
realization lost 48 c/sample against the probe's", was a statement about two
*addresses*, not two realizations.

**The caveat that decides how far this may be quoted.** mGBA is a model, and ROM
wait-state/prefetch behaviour is exactly the class of thing a model can get wrong in
either direction; E5 (hardware arbitration) is still unrun, and the 62.5 ms frame
budget is the only deadline in the item. What is *not* model-dependent: the
instruction that stops being fetched from the cartridge bus, the identical
instruction stream, the cycle-identical control leg, and the direction of the effect
(the ARM7TDMI has no cache, and GBA ROM is a 16-bit bus at WS0 = 3,1 — an ARM
loop there is slow on real hardware by construction; only the exact multiplier is
mGBA's). Also unchanged: this is *decode* only. The producer's convert + push stages
(≈7.7 ms combined, word-copy era) are not in these windows, the shipped default
(`TableCountKernel`) is untouched at 167%, and no product path names `rice-arm`.


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
| **6a-5b realization differential (measured)** | `flac-spike-6a5b.gba` sha `b6e35beb1e04028c…`, `rustc 1.100.0-nightly (a69a63265)`; three timed legs in one link (`arms=3` in that pass's calibration line), `overhead=33 stable=true ime_inside=0`; **`asm − vend` = +15,431,627 / +15,409,977 cycles** per 157-frame clip (FIXED / LPC) = **+98,291 / +98,153 c/frame = +24.11 c/codeword = +48.22 / +48.16 c/sample**; `vend_faster=157, asm_faster=0` on both clips, max +98,775 @147; run 2 differs by +820 / +2,035 cycles in total | the number of record for this rung. It confirms the 6a-5 projection and refutes its re-opening: the landed realization *is* 9.4 budget points of the 32,768 Hz frame worse than the probe's, in one image, on identical bytes |
| 6a-5b legs of the same pass | ctrl 275,126,433 (1,752,398 c/f, 167.09%) → asm 183,304,113 (1,167,542, 111.33%) → vend 167,872,486 (1,069,252, **101.95%**); `asm − ctrl` = −91,822,320 **cycle-identical to the 6a-5 record**; `vend − ctrl` = −107,253,947 (−683,146 c/f) | all three legs decoded to the pinned FNV `0x54C7B356621B6E15` and the per-frame sample-equality witness held, so the difference is realization only. The PR 3 budget verdict (0/2 in budget, screen RED) says no leg fits 32 kHz; it does not void the pair |
| 6a-5b arm absolutes are **link-stable** | landed asm 183,288,727 (product shape) / 183,231,371 (three-arm link) / 183,304,113 (this image) — 0.04% spread; E10's vendored arm 167,911,108 / this image's vend leg 167,872,486 — −0.02%; while `ctrl` spans 280,848,260 → 275,111,047 → 249,051,451 across links | the reading rule's exception ("absolute *paired* marginals") is symmetric: an absolute that reproduces across images is quotable across records. That is *why* 6a-5's cross-record projection came out right, and why 6a-5b was still the right way to settle it |
| 6a-5b machine-code delta, same image | `arm_rice_partition` 684 B / 171 words @`0x0800017c` vs `e10_rice_partition` 640 B / 160 words @`0x08000428`: +11 words static; **executed delta 6 per Rice codeword** — G1 `ldr [sp,#8] + cmp + bhi`, G2 `adds + beq`, and one **dead duplicated `and r12, r3, #0xFF`** — plus 3 per partition (`mvn + lsr + str`). `.Lvalerr` and the `mov r2, #2` arm are never executed on this corpus | 640,000 codewords × 6 + 314 partitions × 3 = **3,840,942 executed delta instructions per clip = 24.9% of the gap at 1 cycle each**, 49.8% at 2, 74.7% at 3; the kernel body accounts for the whole gap only at **4.02 cycles per delta instruction**. The gap is **24.11 cycles per codeword** — the unit to reason in |
| 6a-5b the units, pinned from the instrument's own line | asset verification prints the clip geometry: `rate=32000 ch=2 max_blocksize=2048 frames=157 samples=320000` — 156 frames of 2048 plus one of 512 = **320,000 samples per channel**, so **640,000 Rice codewords** per clip (one per sample per subframe), and on FIXED with partition order 0, **314 partitions** (2 subframes × 157 frames). The house convention divides a delta by 320,000 to get c/sample; per codeword is half that | 24.11 c/codeword, the 3,840,942 count and the 4.02 figure are all re-checkable from that one printed line, without re-deriving the clip's structure |
| 6a-5b landed-kernel byte identity across four images | the kernel's bytes at file offset `0x17c`, length 684, sha prefix **`38760f65855c`**, in: a base-commit (`9345232`) worktree build, the pre-change control (`6a30a3f5…`), the current-tree control (`262cdf0a…`) and the 6a-5b image. An earlier "the landed kernels differ" note was an extraction-offset error and is withdrawn | the differential is not an accidental codegen mismatch of the kernel under test; and an 112 B whole-image difference between two of those images coexists with a cycle-identical arm |
| 6a-5b a real defect in the shipped kernel | `crates/flac-lite-gba/asm/rice_partition.s` lines **256 and 258 are the same line**: `and r12, r3, #0xFF  @ lo = rem_w & 0xFF`. The [6a-4 G1] insertion moved that line above the guard and left the original in place. Emitted adjacently at `0x8000288 / 0x800028c`; `r3` is unchanged between them and neither sets flags ⇒ the first is **dead, once per codeword**, 640,000 dead executions per clip | the cheapest counted instruction on the table, and a single-variable probe for the layout channel. **Not removed by this record**: the kernel this record anchors must keep its bytes, and removing it is a build change that re-runs `overlay-kernel-test` and `arm_from_outside` |
| 6a-5b what is symmetric, so this image prices it at zero | the **same number of kernel calls per frame in both legs** — 2 on FIXED (partition order 0, 2 subframes), more on LPC (its partition orders vary per frame), equal in both legs either way — each call through a linker thunk (`__Thumbv4ABSLongBXThunk_arm_rice_partition`, `…_e10_rice_partition`); tables in `.iwram` in both (landed `0x0300032c/34c/44c`, vendored `0x0300054c/56c/66c`); the stack is IWRAM for both (the boot code loads `0x03007ffc`); `stereo::decorrelate` and `PredictorState` are shared by both | the bridge, the pin, the 12-byte frame and the state ABI contribute **nothing** to this differential — which is the static bound's one conclusion that survives. What the vend leg *does* carry that the landed leg does not: its own 32-bit-window `AccumulatorReader`, its own frame/subframe/residual/prediction code. "Realization" here means the whole per-codeword path, not only the asm body |
| 6a-5b the cost tracks codewords, not frames | +48.22 vs +48.16 c/sample on two clips whose control costs differ 3.75× (1,752,398 vs 6,572,815 c/f) and whose per-frame work is different code; both clips are 320,000 sample-frames = 640,000 Rice codewords | the regression is attached to the per-codeword loop, not to the frame path, the LPC work, or anything per-frame. Any candidate that scales per *call* has to survive the FIXED/LPC invariance, which the bridge (2 calls/frame in both) does only by being ~0. **The per-partition term is ruled out by the same measurement:** the LPC clip's partition count is *not* 314 — its partition orders vary frame to frame — yet the per-codeword cost moves 0.14% between the clips, so the only per-partition item in the diff (the `mvn/lsr/str` bound hoist) cannot be a meaningful share of a cost that does not scale with partitions |
| **6a-5c L1: one dead instruction removed = 4.0000 cycles per codeword** | `flac-spike-6a5b.gba` sha `77a63e2b506d0bd9…` (same image, asm body 684 → 680 B, 171 → 170 words, the duplicated `and` deleted, all gates re-run green). `asm − vend` 15,431,627 → **12,871,627** (FIXED) and 15,409,977 → **12,854,993** (LPC) | removed **2,560,000 / 2,554,984 cycles** = **4.0000 / 3.9922 c/codeword**, 16,305.7 / 16,273.8 c/frame, 8.00 / 7.98 c/sample. Per frame the fall is *exactly* 16,384 cycles on every 2048-sample frame (156 of 157) and exactly 4,096 on the 512-sample frame — proportional to codewords executed, **not** a constant per frame and so **not** alignment |
| **6a-5c L2 (diagnostic): the guards are the rest of it** | same image, G1+G2's 5 instructions deleted (`arm_rice_partition` 170 → **165 words**). `asm − vend` **71,627** (FIXED) / **80,073** (LPC); the per-arm decode proof still passes on every frame, so the guards never fire on this corpus | 12,871,627 − 71,627 = **12,800,000 = 5 × 4 × 640,000 exactly** (LPC: 12,774,920 = 5 × 4 × 638,746). So each of the five guard instructions is worth **4.000 cycles per execution**, and the whole 6-instruction delta is **6 × 4 × 4096 = 98,304 c/frame** against a measured 98,291 |
| **6a-5d: the same instruction stream, resident in IWRAM** | `flac-spike-6a5b.gba` sha `232d14982718daf7…`; the *only* change is the section directive in the kernel body. `arm_rice_partition` moves `0x0800017c` → **`0x0300022c`**; `.text` shrinks by exactly 680 B and `.iwram` grows by exactly 680 B (1,900 → 2,580 B). `asm` sum **180,744,113 → 79,506,883** (FIXED) / 938,090,234 → **837,775,841** (LPC) | saves **101,237,230 / 100,314,393 cycles** = **644,824 / 638,945 c/frame = 316.4 / 313.5 c/sample = 158.2 / 156.7 c/codeword**. The landed arm goes **109.77% → 48.29%** of the 32,768 Hz frame budget (worst frame 509,121 = 48.5%); LPC 569.7% → 508.8%. The `ctrl` leg is **cycle-identical** to the pre-pin image (275,126,433 / 1,031,931,935), so nothing but the asm arm moved |
| **6a-5d: the per-instruction price, twice measured** | 6a-5c priced an instruction at 4.000 cycles *fetched from ROM*; 6a-5d prices the same instructions at **~1.4 cycles from IWRAM** — the 6-instruction guard delta falls from 20.11 to **7.11 c/codeword** | the ratio is the fetch model, and it applies to both kernels: pinning the *vendored* leg on its own drops it 167,872,486 → **74,957,768** = −92,914,718 = **−591,813 c/frame = −290.4 c/sample** |
| **6a-5d with both realizations in the same memory: the "seam" differential mostly evaporates** | landed pinned 79,506,883 vs vend pinned 74,957,768: `asm − vend` = **+4,549,115** = +28,975 c/frame = +7.11 c/codeword = **+14.2 c/sample**, against +15,431,627 / +48.2 c/sample with both in ROM | the 48.2 c/sample was **never** the seam, the ABI, the tables, or the guards' *semantics*: it was 6 instructions × the difference between a ROM fetch and an IWRAM fetch. The residual 14.2 c/sample is the guards at IWRAM prices — what a cheaper G1 spelling (L3) could still buy back |
| **6a-5c: the measured instruction cost in this loop is 4.00 cycles** | three independent removals (1 instruction, then 5 more) at two clip workloads, all scaling with codeword count at 4.000 / 3.992 cycles each; 4 cycles is the ARM7TDMI fetch cost of a 32-bit ARM instruction from cartridge ROM on a 16-bit bus at WS0 = 3,1 | the 6a-5b gap is **fully accounted for by instruction count**, at 4 cycles per instruction, and *not* by hot-loop layout: the "4.02 cycles per inserted instruction" that 6a-5b's arithmetic demanded is not an anomaly, it is what an instruction costs here. This reverses E6's "instruction fetch is ruled out" **for this case** — E6 measured a Rust path where each instruction carries many cycles of work |
| 6a-5c what is left after all six instructions are removed | L2 residual `asm − vend` = **71,627 (456 c/frame)** FIXED, **80,073 (510 c/frame)** LPC, against a per-partition prologue of 3 instructions × 4 cycles × the partition count | the residual is per-call, not per-codeword, and of the same order as the static bound's ~0.15 c/sample (≈305 c/frame) for the seam bridge — i.e. after the kernel delta is gone, what remains is the surrounding path, as predicted |
| **6a-6 width default (2026-11-09, codegen + gates; no cycles)** | default instrument `.text` 45,996 `f63ff65bedea…` → **46,428 B** `a03cc332f080…` (+432 B, A/B vs `HEAD`): `integrate_lpc` 256 → `integrate_lpc_guarded` 696 (+440; guard + both LPC legs), width report on `decode_subframe_with` → `_with_width` +4, `driver::decode_one` −12; `integrate_fixed` 244 → `integrate_fixed_i32` 244 (0); `.rodata` +32 B, `.iwram` 812 / `.ewram` 368 unchanged. Gates: `flac-test` 183 (+5), `flac-test-wide` 178, `flac-gates` 1,377 / 6 ignored | the narrow default carries the i64 LPC escape leg (its presence is the +440 B); FIXED's i64 form is dead-code-eliminated. The width marginal is **not** read here — 6a-7 owns the same-image cycle arm |

| **6a-7 width + stack (2026-11-10, one image, 157 frames/clip)** | `flac-spike-6a7.gba`; `ctrl` (i32) FIXED 1,662,788 / LPC 2,987,753 c/frame; `wide` (vendored pre-6a-6 i64) 1,752,704 / 6,573,096; `asm` (kernel+i32, IWRAM) **416,804 / 1,751,090**. Width `wide − ctrl` = **+14,116,716 / +562,898,810 cycles per clip = +89,915 / +3,585,343 c/frame = +44.1 / +1,759.1 c/sample**, 157/157 frames. Stack `ctrl − asm` = **1,245,984 / 1,236,663 c/frame**, 157/157. | Q5a: additivity reproduces — `asm_i32 + width` = 506,719 / 5,336,433 c/frame vs 6a-5d's `asm_i64` 506,413 / 5,336,152 (**+0.06% / +0.005%**). The wide leg reproduces 6a-5d's i64 `ctrl` to **+0.017% / +0.004%**. FIXED stack **39.74% mean / 39.96% worst** (from 48.29%); LPC 166.97% (from 508.8%). |
| 6a-7 determinism / gates | two headless runs (one across a rebuild after `cargo fmt`) byte-identical, 1009 lines `cmp`; `item6a7-test` 26 passed | the wide leg is pinned to the reference PCM and to `ctrl` frame by frame; the default image is unchanged (`a03cc332f080…`, `.text` 46,428 B) |
| **6a-7b same-image reference (2026-11-10, mGBA, summary-bearing image)** | `flac-spike-6a7.gba` `615d38be2786…` / `.text` `9f0b19e924cd…`; FIXED `ctrl` 1,662,797 / `wide` 1,752,714 / **`asm` 416,821** c/frame, stack 1,245,976, width 89,917, worst 39.961%; LPC 2,987,762 / 6,573,107 / **1,751,108**, stack 1,236,654, width 3,585,345, worst 167.854%. Two runs, byte-identical. | the reference for a hardware reading is **this** image, because adding code to a link can move a decode path (6a-3). Measured cost of the screen: **≤18 cycles/frame (≤0.005%)** against the pre-summary 6a-7 record, every leg. Control build: default image **byte-identical** (`a1acc5980a6b…`); 6a-5 / 6a-5b `.text`/`.iwram`/`.ewram` byte-identical, `.rodata` differing in **2 / 4 bytes** — all `core::panic::Location` line numbers. Gates `item6a7-test` 20+1+10, `spike-test` 15+10, `item6a5b-test` 21+4+3+10. |

| **6b-1 product path on hardware (2026-11-10, GBA 001 + GBA SP, both A/B builds, identical on both units)** | `flac-integration.gba` `12732380a0aa…` / `flac-integration-default-kernel.gba` `2b108d290d44…`, same images as the mGBA run below. `rice-arm`: **396,709 mean / 398,923 worst c/frame = 37.83% / 38.04%** of the 32,768 Hz frame. `TableCountKernel`: **1,622,116 / 1,630,247 = 154.67% / 155.45%**. Both legs match the PCM pin `0x54C7B356621B6E15`. | **The headline is the agreement, not the headroom.** The ARM leg is **+0.055%** over mGBA (396,492 → 396,709) — mGBA prices the IWRAM-resident kernel essentially exactly, and the 3.7–6.9% shortfall of 6a-7b does **not** apply to it. The default kernel is **+4.03%** (1,559,312 → 1,622,116), inside that band. So the shortfall is located in the ROM-instruction-fetch-dominated leg, not in a clock/timer/calibration term (Q13). Kernel advantage on silicon: **4.09×**. Decode headroom **2.63×**; C2's arithmetic becomes 38.04 + 22.6 + 20.3 ≈ **80.9%**, margin ≈11.9 ms. |

| **6b-1 product path (2026-11-10, mGBA, `flac-integration.gba`)** | The product ROM (manifest facade over `scan` + `rice-arm`), one pass per kernel on the same source: `rice-arm` **396,659 mean / 398,867 worst c/frame = 37.82% / 38.03%** of the 32,768 Hz frame; `TableCountKernel` 1,559,303 / 1,567,127 = 148.68% / 149.43%. Both legs match the PCM pin `0x54C7B356621B6E15`. | The kernel's win reproduces **through the product path**: 3.93×. Lower than the instrument's 39.74% for the same kernel (the instrument's arm carries its own call shape and state; this measures the product). Hardware expectation ~39–41% until a unit reads it (mGBA understates by 3.7–6.9%, 6a-7b). Seam A/B on the same ROM: `.text` **49,056 B `7c7d580ccbb4…`** byte-identical with the generic entry and with the pre-seam non-generic call, `.iwram`/`.rodata` likewise — the `K` parameter costs nothing in a single-instantiation image. Method trap: timing the proof pass reported 1.95 M c/frame because the window priced the FNV fold; correctness and timing are two passes. |
| **6b-2 pt 2 product path, both kernels (2026-11-11, mGBA headless `mgba-test-runner`, both images built at the repository path)** | `flac-integration.gba` `e859f7bc…` with control `flac-integration-default-kernel.gba` `193dd44a…`; the control was taken by checking out 6b-1's merge (`654c7f8`) and rebuilding it, which reproduces 6b-1's recorded totals **to the cycle** (62,249,317 / 244,812,048), so the two columns are one environment and the comparison is a comparison. `rice-arm`: **395,850 mean / 398,037 worst c/frame (−0.16%** against 6b-1's 396,492 / 398,703). `TableCountKernel`: **1,682,460 / 1,690,900 (+7.90%)**. Each image run twice, its two runs identical to the cycle; all four legs match `pcm_fnv=0x54C7B356621B6E15`. | Full record in `plan-of-record.md`, 6b-2 item 2 part 2. Kept here because it is the mGBA twin of the hardware row below: the control's +7.90% is what reads as +7.09% on silicon, and the ARM leg's −0.16% is what reads as −0.162%. |
| **6b-2 pt 2 product path on hardware (2026-11-11, GBA 001 + GBA SP, owner's run, two runs each — all four readings identical on both units to the cycle)** | The same two images. `rice-arm` (product): **396,066 mean / 398,255 worst c/frame = 37.765% / 37.974%** of the 32,768 Hz frame (37.981% against 2^20), 193.4 c/sample, decode headroom **2.63×**. `TableCountKernel`: **1,737,135 / 1,745,843 = 165.638% / 166.468%**. Every screen: `157 frames`, `PASS`, blue, `pcm_fnv=0x54C7B356621B6E15`. | **The product path's row of record, superseding 6b-1's 37.83% / 38.04%.** Hardware vs mGBA: the ARM leg is **+0.055%**, the same gap as at 6b-1's bytes — a constant across two image generations — while the Thumb leg is **+3.25%** where 6b-1 measured +4.03%, and that same leg's cycles moved **+7%**. So the derating rule gains a half: mGBA is a good **screen** for a layout shift (it got both directions right and both magnitudes within 0.8 pp), but a per-leg derate does not carry across images. Kernel advantage through the product path is now **4.39×** on hardware (4.25× in mGBA) — the ratio moved because the slow leg got slower, which is why it is quoted per image. |
| **6b-2's product images re-read at `main` (2026-11-11, agent host — sizes, sections and gates only; no cycles)** | The same tree at `1e5974c`, rebuilt here, reproduces those image rows **to the hash**. At `main` (`e3d377e`) both images are **+32 B** (`db003f4b…` / `57405056…`): `.text` 68,532 → **68,548**, `.rodata` +16, `.iwram` 2,036 B / `.ewram` / `.bss` unmoved, with `arm_rice_partition @ 0x0300022c` and `CTZ8`/`REV8` and the core's `LEADING_ONES` still inside `.iwram`. `flac-gates` 1,476 → **1,498 passed / 3 ignored across the same 72 binaries** (+22, attributed test-name by test-name). | Owner's ruling, same day: **+32 B is not material** and owes no hardware tip of its own; item 7-1 re-reads both arms at the product profile regardless. The mechanism is **not** the seek-table API — `nm` finds no `stream`/`seek`/`FrameNumber` symbol in either image — it is rustc re-cloning the hot bit reader, which 6b-2's own record identified. **Every cycle row in this ledger belongs to `1e5974c`'s bytes; `main`'s images have been run nowhere.** Method trap: both A/B arms write the *same* ELF path, so re-run the arm you mean immediately before taking a section witness. |
| **6b-3's host-portability restructuring: images moved, cycles did not (2026-11-11, agent host, mGBA runs of both arms)** | Rebuilding the two decode ROMs after `agb`/`flac-lite-gba` moved under `[target.'cfg(target_arch = "arm")']` and `main.rs` was cfg-gated gives new images — `flac-integration.gba` `f73097c1…` (1,070,960 B), `flac-integration-default-kernel.gba` `aecae6ef…` (1,068,164 B) — while the sections read exactly the recorded sizes (`rice-arm`: `.text` **68,548 B**, `.rodata` 1,000,108 B, `.iwram` **2,036 B**, `.ewram` 400 B; the control: `.text` 66,968, `.iwram` 812). Both arms re-run here: **`rice-arm` 395,850 mean / 398,037 worst c/frame, total 62,148,461; `TableCountKernel` 1,682,460 / 1,690,900** — digit-for-digit the 6b-2 pt 2 mGBA row, both PCM pins `0x54C7B356621B6E15`. `product-play.gba` `5dc7869…` likewise rebuilt byte-identical across the same change. | The host-build requirement (a model of the playback geometry cannot link `agb`: `portable_atomic_unsafe_assume_single_core` is a hard `compile_error!` off ARM) changed the dependency *shape* of the product crate, and the honest witness that this moved no code is the cycle lines, not the source diff: a hash that moves while every count and pin reproduces is a build-metadata move (panic locations, embedded paths), which is what the record's own `Location`-bytes note predicts. **This row exists so nobody has to re-derive the hashes**; it also re-states the trap the previous row names — both arms write the same ELF path, so those section reads were taken one arm at a time, each immediately after its own build. 7-1 re-reads both arms on hardware at the product profile regardless (owner's ruling on the +32 B). |
| **6b-3 product-play: the whole path in one image (2026-11-11, mGBA headless — **not** hardware)** | `product-play.gba`, 1,083,488 B, sha `5dc786951b91b356…`; `.text` 0x13048 (77,896 B) at `0x0800017c`, `.rodata` 0xf4d14 at `0x080131c8`, `.iwram` 2,036 B, `.bss` **42,672 B at `0x02000190`** under agb's own generated `gba.ld` (no linker script of ours). *Correction, same day: this row said `.text` 0x125fc. The build that reproduces this row's own hash and file size — and its `.iwram`/`.bss` figures — reports the map quoted above at the same address, so 0x125fc was simply wrong for the image described; the flat image is unchanged (same hash), so nothing measured here moves.* *One more, for the reader who rebuilds: this row's hash is the hash of the image this row ran, not the hash whatever a checkout of this tree happens to build — see "Image hashes move on comment edits" below.* One headless run of the 9.766 s clip at 32,768 Hz: **`underflows=0`, `pushed=consumed=320000`, `hw_periods=571 serviced=571 coalesced=0 max_gap=0 late=0 max_delay=0 tail_periods=1 tail_real=240`**, `pcm_fnv=0x54C7B356621B6E15 [MATCH]`, `fit: lines=7 right=132 bottom=87 fits=true`, **`verdict PASSED — screen BLUE`**. Stages per 2,048-sample frame, on the timer's own 2^24 clock (512 c/sample exactly, frame = 1,048,576 c), over **152 fed-phase windows**: decode **426,327 mean / 434,617 worst**, convert 113,586, push 15,280, feeder 5,194/period ⇒ at 157 frames the four contribute **decode 39.362% + convert 10.487% + push 1.410% + feeder 1.801% = work/frame 556,405 = 53.062%** (the parts' integer sum is 556,403 c); worst single frame's work **622,517 = 59.367%**; **frames whose work exceeded their own period: 0**. *Correction, same day: this row first quoted the four terms as 40.657% / 10.832% / 1.457% / 0.495%, which is each stage's mean **window** divided by a **frame** budget — 152 windows against 157 frames — and, for the feeder, one period's cost divided by a frame's budget. Those four summed to 53.441% beside a total of 53.062%. The image now prints both denominators' worth in one line (window means in the stage rows, per-frame shares in the `budget:` line, with their own sum so the truncation is visible) and the rebuilt image `c06d993cabcc18c1…` (1,083,364 B, −124 B: the corrected line is fewer instructions) reproduces **every** sum, mean, worst, boundary count, audit line, `audio_span` and the verdict unchanged, so no measured cost moved — only where the divisions happen.* Flow-control watermarks as the image reports them: **`lead (samples): min=9120 max=19600 ring holds=20160 a period needs=560 margin over one period=8560`**; prefill "10,240 in 5 frames (target 8,960)". Rebuilt byte-identical after `agb`/`flac-lite-gba` moved under `[target.'cfg(target_arch = "arm")']` and `main.rs` was cfg-gated for the host — the same hash is the evidence those portability edits moved no product code. | **The first end-to-end playback run in this project's records** — asset → manifest → `Decoder` + `rice-arm` → `convert_frame` → `RingFeed` → sound DMA/FIFO → timer-paced periods, with underflows *counted* rather than inferred, and with the period audit (coalesced / late-servicing / grace / tail) present so a silent replay cannot masquerade as a clean pass. It is a **system** row: 40.657% is this image's decode cost, ring residency and interrupt context included, and is **not** the 37.765% decode row; the image prints its own attributed estimate (420,894, after pricing the 159 period boundaries nested inside its decode windows out of the mean) beside the pinned 396,066/398,255 rather than pretending they are the same quantity. Two reporting rules earned their keep here: `max_work`/`over_period` count only decode+convert+push (a flow-control **wait is not a cost** — the pacer's wall span, mean 1,015,859 vs the 1,048,576 budget, is a separate line), and closure is stated as disjoint terms plus a separately-priced nested interrupt. No *clock*-denominator drift in this image: the pace is the timer's 2^24, so the 16.78 MHz rows differ by 0.0166% and 7-1 still owns the reconciliation (the *share*-line denominators drifted, which is the correction above and a different thing). **Headless is not C2: this image has been run on no hardware.** And, ruled the same day
this row was taken (**Q14**): the asset it plays declares 32,000 Hz and is therefore
**not a product asset** — a stream is encoded at the rate it is played at, and 1.024×
(41.06 cents) is not acceptable on its own terms. The geometry, the flow-control rule and
this cost envelope carry over to 7-0's asset; the asset measured here does not. |
| **6b-3's host model, agreeing with that run from the other side (2026-11-11, `make product-play-test`: 10 tests green)** | Same blob, same parsed manifest, same `RingFeed`, same `geometry` constants, no `agb` (the crate's platform deps are arm-only, so the library builds for the host): it replays the real decoder's real frame lengths against a credit-per-period clock at the cost the ROM charged itself — **549,760 c/frame**, which the test now *derives* from the run's own printed sums (decode 64,801,816 + convert 17,265,217 + push 2,322,620 over 152 windows = 555,193, less the 159 period boundaries inside the decode windows at 5,194 c = 549,760) and re-checks against the ROM's printed means and against the image's own attributed decode estimate (420,894), which the two records now compute the same way. *Correction, same day: the first version of the test transcribed **536,808** — it priced the closure line's 538 boundaries instead of the decode windows' 159 — so the model was charged 2.4% less than the producer costs and the three ceilings below were briefly recorded as 157.3% / 196.0% / 183.5%. At the correct envelope they are **153.6% / 191.3% / 178.8%**; the absolute ceilings do not move (844,578 c/frame is the throughput bound either way), and the assertions' floors (150% / 140%) still hold, so no conclusion turns on the correction — which is the point of charging both and deriving rather than transcribing.* It reports **`periods=571 tail_periods=1 tail_real=240 primed=560 min_lead=9120 max_lead=20160 underruns=0`** — the ROM's audit lines and its **floor** reproduced digit for digit. The **peak** is the one place they do not, and by design: the model tops out at the ring's ceiling (20,160) where the ROM peaked one period lower (19,600), because a ring the DMA is draining concurrently never quite fills; `floor_matches_the_roms_watermark` states that as a one-period bound with that reason attached, and it is quoted as a bound, not folded into agreement. The floor itself is derived rather than quoted — `ceil(8,960/2,048)×2,048 − 560 − 560` = 9,120 — and the test asserts the derivation and the ROM's observed number as two separate facts. Then the geometry, measured: sustained ceiling **153.6%** of the measured cost with no ring in the picture, **191.3%** with the shipped one (at the derate: 178.8%); *(correction 2026-11-12, ninth row below: the "no ring" figure here and the "~38 points" it implies were computed from a ceiling truncated from 3.657 to 3 periods per frame — the clock's bound is **186.5%**, the shipped clip's 190.6%, so the ring buys **4.1 points**, not 38.)* **stall tolerance: the quantised prefill of 10,240 less the prime of 560 is 17 periods of audio in hand, and the 18th period is the first with nothing to take** (`burst_stall_tolerance` asserts that index, and that no stall deeper than the ring's 36 survives); **the deepest lead at which the flow control will still *start* a frame is 32 periods** (`(ring − declared blocksize)/period` — it holds a frame's worth of ring free before decoding), while the lead actually *peaks* higher still, because a frame already started pushes its 2,048 samples into that reserve: 35 chunks observed in the ROM, the ring's full 36 in the model; **sizing scans** put the depth cliff at **20 chunks** and the prefill cliff at **1 chunk** (lead floor 368 samples there), with rows below 20 *infeasible* because `PREFILL_SAMPLES` + one frame's reserve = 11,008 samples cannot be prefilled. | The reason `src/geometry.rs`'s comments are no longer assertions. Three of them over-claimed and are now corrected against these numbers: "36 chunks = 36 periods of stall" (the stall answer is **17 periods in hand, starved at the 18th**, and the 32 that shows up in the arithmetic is the lead at which a frame will still be *started*, not a depth the lead reaches — the peaks are 35 and 36), "a deep ring buys tolerance for a sustained overage" (it buys ~38 points of it and then nothing), and "the minimum prefill lands near 2 chunks" (it is 1 — which is exactly why 16 is the cheap knob and the depth the expensive one). Note the one place the model *disagreed* first and was wrong: a draft prefill loop pushed until the ring was full instead of stopping at the target, predicted a floor of 7,840, and explained away the 1,280-sample gap as clock coarseness. The cross-check against the ROM caught the bug. A model and a ROM that agree on 571/1/240/9,120 while sharing only the ring code and the constants are evidence they run the same protocol; they are not evidence about silicon, which the model does not model. |
| **6b-3's rate question, measured from the shipped frames (2026-11-11, agent host, mGBA headless — **not** hardware; the test asset is a *relabelled* copy of the shipped frames, not a 32,768 Hz pipeline asset; the probe now lives in `examples/flac_spike/experiments/rate-respell-32768/` — committed after the fact on 2026-11-11, when this row's own admission that `no script or fixture is committed for the probe` was recognised as a breach of the standing rule that probes live there; `python3 probe.py` re-derives every number in this row and asserts the pins rather than printing them)** | The probe: take the shipped blob's own frame region — sliced with its own offset table, every frame's CRC-16 verified — change every frame's 4-bit sample-rate code from `0b1000` (32 kHz) to `0b0000` ("get it from the stream"), recompute each header CRC-8 and each frame CRC-16, and wrap the region in a hand-built STREAMINFO declaring **32,768 Hz**. Two traps on the way: those STREAMINFO blocksize fields must carry the **full** block (2048/2048), not 512/2048 — that is what libFLAC's own encodes record for a stream with a short tail, and with 512/2048 the reference decoder reads the stream as variable-blocksize and prints `sample or frame number does not increase correctly` for all 157 frames; and `flac -d` writes a 44-byte RIFF header whatever the output file is named, so hashing the file instead of the `data` chunk turns a bit-exact decode into a false mismatch. What the probe produced: the blob is **865,996 B again — not one frame changes length** (4 B per frame differ: the nibble and the two CRCs); `flac -t` → `ok`; `flac -d` → 320,000 samples **byte-identical to `examples/flac_spike/assets/spike_l0_pcm.bin`**, FNV `0x54C7B356621B6E15` = the pinned `fnv_pcm`. `gafp_pack` still refuses it: `fixed0 x314: no measured decode figure for (32768 Hz, 2048-sample frames, fixed0)`; with `--allow-unmeasured` it writes `p32768.gfp` (865,996 B, sha `d49cf0e6…`, region 865,340 B at `0x290`) under its own `this is NOT a product blob` note, and its strict walk reports the same shape as the shipped asset's: 157 frames, 314 slots, `fixed0 x314`, every exit landing on the next frame. Then each product image against that blob, paired in one session with the same source against the shipped one: **decode-only** `flac-integration.gba` `f73097c1…` re-run here reproduces its row **to the cycle** (`total=62,148,461 mean=395,850 worst=398,037`, PCM `[MATCH]`), and the same source built against the 32,768 blob (throwaway image, `manifest: … rate=32768 …`, PCM `[MATCH]`) reads `total=62,145,321 mean=395,830 worst=398,017`. **product-play**: `product-play.gba` `5dc7869…` re-run here byte-identical to the row above (the two logs `cmp`), and the same source on the 32,768 blob (throwaway image `d3e5c904…`, same 1,083,488 B) reads `underflows=0`, `hw_periods=571 serviced=571`, `grace=16`, `min_lead 9120`, `pcm MATCH`, `audio_span` **identical at 163,722,350**, decode 426,742 mean / 434,577 worst against 426,327/434,617, **work/frame 557,640 = 53.180%** against 556,405 = 53.062%, worst frame 622,528 against 622,517, and its closure `gaps` term moved 128 → −131,694. Assets were swapped in place and restored byte-identically; both shipped images rebuild to their recorded hashes. | Four things the mapping question was waiting on, and one it was never going to answer. **(a) Representability:** 32,768 has no 4-bit rate code, so a 32,768 stream spells it as `0b0000` + a stream default — measured here: **zero** size cost — or as `0b1101` + a 16-bit Hz field, which is what `flac` 1.5.0 emits when simply *asked* for 32,768 (no `--lax` needed; `format-notes.md` has the uncommon-form row) and which costs 2 B/frame, ≈+314 B on this asset; `crates/flac-lite` already reads that second form in its `s3_wide_48001hz_b3000` fixture. **(b) Cost, answered at the instrument's floor:** −20 c/frame decode-only (0.005%), and inside the player +415 c/frame of decode and +1,235 c/frame of work (0.22%) against a closure residual that moved 131,822 cycles by the image's own account — the two readings do not separate at this resolution, so "decode cost depends on block size and residual, not on the declared rate" is now tested rather than asserted. **(c) The 2.4% deadline tightening prices nothing at today's levels:** the frame deadline is 1,048,576 c in both cases, no instrument here budgets from the declared rate, the worst frame's work is 59.4% either way, and `audio_span` is the same number. It would bite only if work/frame approached the period. **(d) What it cannot answer is the pitch:** relabelling changes no sample — the probe's decoded bytes *are* the shipped PCM — so the 41 cents lives in which rate the source audio was resampled to, and only the encode pipeline moves it. **(e) What still blocks a true-32,768 product asset is a measurement key, not a format limit:** `MEASURED_DECODE` carries rows only at `(32000, 2048, …)` and every row in it is a **hardware** figure, so this mGBA pair cannot fill `(32768, 2048, fixed, 0)`; that row belongs with 7-1's hardware reads, and the asset worth measuring would come from a resample-at-32,768 encode whose census is not this one's. Owner: Q14, which this row half-answers — and the half it answers is the expensive one.
**Ruled the same day (2026-11-11): the pitch is not acceptable, a stream is encoded at
the rate it is played at, and the product asset moves to 32,768 (plan item 7-0).** What
this row bought is that the ruling is priced rather than guessed — representability, byte
cost, bit-exactness, the flow-control floor and the runtime cost all measured, so the
cost of the ruling is known to be pipeline work and one hardware row (7-1), not a
redesign. The pitch witness (a tone fixture) still does not exist, and it belongs with
7-0: no gate in this repository can see a pitch shift, because the pinned PCM digest is
identical for a correct-rate asset and a reinterpreted one. |
| **6b-3 verdict integrity: the audit is gated and the clock is a median of three (2026-11-12, mGBA headless — **not** hardware; supersedes the *shares and worst frame* in the 6b-3 rows above, not their ledger)** | `product-play.gba` `fe00b590c71fb28e…`, **1,084,572 B**. One run (three identical): `underflows=0`, `pushed=consumed=320000`, `hw_periods=571 serviced=571 coalesced=0 max_gap=0 late=0 max_delay=0 tail_periods=1 tail_real=240`, `pcm_fnv=…6E15 [MATCH]`, `verdict PASSED (ledger=true, audit=true, pcm=true)`; screen `ledg ok aud ok` and `rate 32000>32768 scaffold`; log `Q14 SCAFFOLD: stream 32000 Hz is played at 32768 Hz`. Shares: work/frame **559,845 c = 53.390%** (decode 39.531%, convert 10.527%, push 1.375%, feeder 1.956%), decode mean **428,152**, convert 114,017, push 14,899, feeder 5,641/period; worst single frame **565,874 = 53.965%**, push worst **15,831**, `void=0`, clock refusals 0, closure `gaps=195,184` over 5,730 snapshot pairs; `min_lead 9120`, `max_lead 19600`. | What changed, and why the older shares are superseded: (1) the period audit is now **part of the verdict** — `coalesced`, `late`, `clock_refusals`, `tail_periods > 1` and any `void` stage fail the run, where before they were printed and ignored; (2) `audio_cycles` takes **three snapshots and returns the median**, because the cascaded timers can be read with TM1 in period N+1 and TM2 still at N (a period low) or the reverse (a period high) while every in-pair test passes; the median discards one outlier and a monotonic check rejects a below-last median; (3) `STATS.finished` is set in the same critical section as the final push; (4) the handler is registered and a stale Timer1 `IF` cleared before the chain is armed. **The 59.367% worst frame and the 79,922-cycle push worst in the row above were this instrument artefact**: the guarded build's push worst is 15,831 c and its worst frame is 53.965%, and the three runs are identical. The +0.328-point work share against 53.062% is the median's own read cost (the extra snapshots sit inside the stage windows) plus the removed 79,922-cycle push window; the ledger, audit and PCM claims are unchanged. Still **no hardware**; 7-1/7-3 own the silicon and audibility. |
| **6b-3 played-form witness: the 8-bit FIFO pins, and why they are folded in the proof pass (2026-11-12, mGBA headless — **not** hardware)** | `product-play.gba` `ba237039e394330f…`, **1,085,604 B**. The generated pins are **`FNV_FIFO_L=0x7CC75F50B4DF75EC`** and **`FNV_FIFO_R=0x916BA1AF3B1F4342`** — FNV-1a 64 over each channel's `(sample >> 8) as u8` stream, derived by `gen_gafp_blob.py` from the committed `spike_l0_pcm.bin`. The run prints `fifo proof: L=… R=… expected … [MATCH]` and `verdict PASSED (ledger=true, audit=true, pcm=true, fifo=true)`; the audio-phase numbers are unchanged from the verdict-integrity row — work/frame **559,849 c = 53.391%**, worst frame **565,939 = 53.972%**, 571/571 periods, 0 underflows, 0 coalesced/late, PCM MATCH. A host test (`fifo_pins_match_the_converted_stream`, 11 tests green) folds `decoded_frames()`' converted output to the same two pins and re-decodes the i16 stream to `FNV_PCM`, so the decode and played-form pins are tied to one stream. | **The push-path fold was tried first, measured, and does not fit.** Folding the pushed `scratch_l/r` on every frame put the push stage at **816,714 mean / 822,153 worst c/frame**, work/frame **1,344,047 = 128.178%**, with 154 underflows and 4 late services — the run failed. The mechanism is the house FNV-1a's 64-bit multiply: ARM7TDMI has no 64-bit multiply instruction, and 4,096 FIFO bytes/frame costs ≈800,000 cycles (≈195 c/byte). The proof pass runs **after** the audio and is not timed, so the witness folds the same converted frames there and the audio phase does not pay for it. The residual gap is stated rather than papered over: this is the converter's output for the same frames, not a capture of the DMA/DAC, and `RingFeed::push_bytes`/`take_chunk` are the byte-witnessed copy in between. A true push-path witness would need a 32-bit hot-path hash (a new pin and implementation), which is not this PR. |
| **6b-3 verification and hygiene (2026-11-12, host + mGBA headless — **not** hardware)** | The host model adds `worst_frame_burst_does_not_starve` — the guarded build's measured **565,939 c** charged at every fed-phase frame position; `underruns=0` and the lead stays above a period at all of them — so the suite is **12 tests**. The ROM's wait/drain loops are bounded in periods, `Stage::mean` divides by measured (non-void) windows, `convert_frame` release-checks its destination lengths, `REARM_GRACE_SAMPLES`/`max_delay_samples` say what they are, the armed line prints **58.51 periods/s**, and the Q14 asset assert is an explicit `PRODUCT_STREAM_RATES` allow-list. `product-play.gba` `694126d406eb9b2a…`, **1,085,804 B**; two identical runs: 571/571, 0 underflows, 0 coalesced/late, `min_lead 9120`, PCM/FIFO MATCH, `verdict PASSED (… fifo=true)`, `fit: lines=9 … fits=true`; audio numbers within 1–72 cycles of the played-form row (work/frame **559,848 = 53.391%**, worst **565,867 = 53.965%**). | These are verifier/hygiene changes, not a new measurement; the one new **property** is the burst replay, which is the variance test the mean-cost geometry check did not have. The rate clarification is **docs-only**, as requested: `agb` 0.25 can *play* 10,512 / 18,157 / 32,768 Hz and the product geometry is 32,768, while a FLAC *stream* may declare any table rate or a 16-bit Hz value; the exhaustive guard is a cheap sweep over the declarable set (`0..=65535` plus the table), and the behaviour change (identity-or-refusal in the packer and player) is 7-0's, not this PR's. No hardware. |
| **6b-3 report hygiene: the model's envelope re-read from the current image, and two instrument descriptions corrected (2026-11-12, host + mGBA headless — **not** hardware)** | Comment/test-constant changes only; rebuilt `product-play.gba` at **`750d01e97800de95…`**, **1,085,804 B** — same size, same section map as `694126d406eb9b2a…` (`.text` 0x136f0, `.rodata` 0xf4f78, `.iwram` 0x7f4, `.ewram` 0x190, `.bss` 0xa6a8 = 42,672 B). Its log is **byte-identical** to that image's run (`diff` clean): 571/571 periods, `underflows=0`, 0 coalesced/late, tail 1/240, `min_lead 9120`, stage sums decode **65,079,938** / convert **17,331,888** / push **2,263,016** over 152 windows with means 428,157 / 114,025 / 14,888, `boundaries_in=159`, feeder **3,221,393** at 5,641/period, work/frame **559,848 = 53.391%**, worst frame **565,867 = 53.965%**, push worst 15,776, `void=0`, clock refusals 0, PCM and FIFO MATCH, `fit: lines=9 … fits=true`, exit 124. Host gates: `product-play-test` **12 passed**, `check` clean, `gafp-check` clean (157 frames, 865,996 B), `gafp-pack-check` 8 passed, `flac-gates` green; `make test-rom` exits 0 with **0 cases collected** — a known state, recorded so it is not read as evidence. The model's envelope is now the image's own: **557,070 raw** from those sums, less 159 boundaries at 5,641 c = **551,170 attributed** (98.941% of raw), and its decode half derives 428,157 − 5,900 = **422,257**, which is the `isolated_estimate` that image prints on its own `attribution:` line. | The run did not move at all — the doc-comment edits moved the hash and the byte-identical log is what proves that. What was wrong is **which run the host model was describing**: it had kept transcribing the 2026-11-11 image's sums (`555,193` raw / `549,760` attributed / `420,894` decode) through three later builds, so the sustained ceilings in `plan-of-record.md` and `geometry.rs` belonged to a producer that no longer existed. At the current image's envelope the model computes **843,237 c/frame with no ring (152.9% of measured)**, **190.6% with the shipped ring**, **178.1% at the hardware derate** — against 844,578 / 153.6% / 191.3% / 178.8% on the older one: percentages 0.26% lower over an envelope 0.26% higher, the ring's purchase unchanged at **377 per mille**, the depth and prefill cliffs unchanged at 20 and 1 chunks. *(Correction 2026-11-12, ninth row below: the throughput bound is **1,027,946 c/frame = 186.5%**, not 843,237 / 152.9% — that figure is `frame_cycles / PERIOD_CYCLES` truncated to 3 periods per frame — and the ring's purchase is **41 per mille**, not 377.)* No conclusion of 6b-3 turns on it, which is why the fix is to the record and the constants rather than to the geometry. The limits of the guard are stated rather than oversold: `the_envelope_is_the_runs_own_sums` locks sums, means, boundary charge and attributed estimate to one named run and to each other, so a half-edited envelope fails — but it cannot see a *future* build's log, so drift is caught by the row-cites-its-image rule above until the envelope itself is a pinned fixture (recorded against 7-1). Two instrument descriptions were also wrong in ways 7-1/7-2 depend on: **`agb` 0.25's `enable_dma_for_sound` (`hw.rs:56-68`) writes source, destination and control and no transfer count at all** — the 140-word write and the per-period re-arm are this ROM's own, and what `buffer_size` = 560 fixes is the audio one period consumes, not a DMA burst — and **there is no `MIXER_TICK_HZ` constant in `agb::sound::mixer`**, the value is inlined as `16777216 / frequency` (`hw.rs:100`). Corrected in prose: agb's rounded reloads for its other two clocks put them at 10,512.04 Hz
and 18,157.16 Hz (**+3.8 / +8.8 ppm**, 0.007 / 0.015 cents), not "a few hundred ppm". And
an exactness rule that the old comments left as an implication is now written into
`geometry.rs` and sampled by the guard: the rates with an integer cycles/sample are exactly
the ones dividing 2^24 — in the declarable band 8,192 / 16,384 / 32,768 / 65,536 — while
the plausible lookalikes are not exact at all (2^24 ÷ 36,864 = 455.11; 448 c/sample would
be 37,449 Hz). The Q14 asset guard is now one condition instead of two, and its sampled
refusals cover the exact-but-refused rates. **Still no hardware**; 7-1/7-3 own the silicon and the audibility. |
| **6b-3 sustained-overage arithmetic corrected, the rate guard given a name, and four instrument descriptions fixed (2026-11-12, host + mGBA headless — **not** hardware)** | Rebuilt `product-play.gba` at **`c0d3ac36e4580a7c…`, 1,085,868 B** (+64 B: the `armed:` line and comments are part of the translation unit). One run: the ledger, period audit, both watermarks and both digests are **identical** to the `750d01e9…` row — 571/571, `underflows=0`, 0 coalesced/late, tail 1/240, `min_lead 9120`, `max_lead 19600`, PCM and FIFO MATCH, `verdict PASSED (… fifo=true)`, `fit: lines=9 … fits=true`, exit 124 — while the stage windows moved by tens of cycles: work/frame **559,800 = 53.386%**, decode mean **428,111**, convert **114,026**, push **14,885**, `boundaries_in` **158** (one boundary now lands in a push window), feeder **3,221,180** at 5,641/period, worst frame **565,859 = 53.964%**, `isolated_estimate` **422,248**. That is string and code layout, not player work. Host gates: `product-play-test` **12 passed**, `check` clean, `gafp-check` clean (157 frames, 865,996 B), `gafp-pack-check` 8 passed, `flac-gates` green. The model's envelope is re-read from **this** image — 65,073,008 + 17,331,966 + 2,262,520 over 152 windows = **557,022 raw**, less 158 boundaries at 5,641 c = **551,159 attributed**, decode half 428,111 − 5,863 = **422,248** — which is the figure this image prints on its own `attribution:` line. | Four things, one of them arithmetic and load-bearing. **(i) The sustained-overage ceiling was truncated.** `frame_cycles / PERIOD_CYCLES` is **3.657** periods per frame, but the test divided as integers and used 3, so the "no ring in the picture" bound printed **843,237 c/frame = 152.9%** of the measured cost instead of **1,027,946 = 186.5%**. That inverted the conclusion drawn from it: the ring's refill was credited with **~38 points** of sustained overage ("377 per mille"), when a bounded buffer cannot buy sustained throughput at all. What depth buys is *time*: the shipped 9.766 s clip survives to **190.6%**, i.e. **41 per mille = 4.1 points** above the 186.5% ceiling, and the same search on a clip twice as long lands at **188.2%** — the purchase shrinks as the deficit is spread over more audio. `sustained_overage_tolerance` now computes the bound as the fraction it is and bisects both clips, asserting that the tolerance *falls* and never goes below the clock's ceiling. No geometry decision moves: the depth cliff is still 20 chunks (prefill-bound), the prefill cliff 1 chunk, and the transient answer is still the measured 17 periods in hand. **(ii) The Q14 guard no longer lives in an `.expect`.** `geometry::RATE_BRIDGE` is now the single named `Option<(u32, u32)>` that both the mapping and the guard read, so `declared_allowed == bridge.is_none()` is green in **both** end states — the shipped one and the completed 7-0 one — and red only on a half-landed 7-0; the 41.06 cents are pinned against a `RECORDED_BRIDGE` literal that survives the arm's removal, instead of a `.unwrap` that 7-0 would have had to delete. **(iii) Four descriptions corrected:** the `armed:` witness claimed "only the timer1 bit should be set" while printing `IE=0x0011` (VBlank is agb's, and the line now says so); the `decode_and_push` doc claimed the prefill and the fed phase "run the same code and cannot drift" when the fed phase is a separate function (now stated as the shared `decode_one`/`convert_frame`/`push_bytes` seam it is); the rate probe's **59.4%** worst frame was cited as current in four places without saying it is the pre-median instrument's reading (this image prints 53.964%); and a duplicated "and a / a sound-FIFO" in the module docs. **(iv) `Stage::note` excludes a void window from `inside`, `worst` and `best`** as well as from `sum`, so a refused window cannot leave `best` pinned at 0; behaviour-neutral while `void=0`, which a passing run requires. |
| **6a-7b on hardware: the stack and the shortfall (2026-11-10, GBA 001 + GBA SP, two runs each, all four identical to the cycle)** | `flac-spike-6a7.gba` `7e0ff19b7a0a…`, same image as the mGBA reference above. FIXED `ctrl` **1,773,272** / `wide` **1,846,926** / `asm` **445,558** c/frame (= 217.56 c/sample, **42.48% mean of the 32,768 Hz frame**, worst **42.72%**); stack **1,327,714**; width **73,654**. LPC `ctrl` **3,134,342** / `wide` **7,020,087** / `asm` **1,816,753** (**173.23% mean**, worst **174.14%**); stack **1,317,589**; width **3,885,745**. | **The pre-declared ±2% bar fails on all five quantities**, so the fork resolves to `diverges` and **39.74% is retired as a hardware number**: 42.72% worst is the hardware figure. Every leg is 3.7–6.9% above mGBA (+6.64/+5.38/+6.89 FIXED, +4.91/+6.80/+3.75 LPC), the stack +6.56%/+6.54%, and the width marginal flips direction per clip (−18.09% / +8.38%) — so not calibration (deltas are not a constant offset) and not a clock error (not a constant percentage). Candidates not yet separated: mGBA's ROM data-read model, its prefetch/data-read interaction, the flashcart (Q13). Decode keeps **2.34× headroom**, so the rate decision stands. |

| **6a-7b on hardware, correctness half (2026-11-10, GBA 001 + GBA SP, four runs)** | `flac-spike-6a7.gba`, pre-fix build `615d38be2786…` (first two runs) and `7e0ff19b7a0a…` (screen fixed, two per unit): every run reached the summary's **blue `harness_ok` backdrop** — assets, both per-arm PCM pins, the frame-by-frame equality witness over `ctrl`/`wide`/`asm`, and calibration stability all passed on the ARM7TDMI. The first two (pre-fix build) displayed no cycles — blank blue — and the fix is display-only, so the correctness verdict is the same on both builds. | **The decoder is bit-exact on silicon**, first claim of its kind in this project, and reproducible across two units. The screen itself was blank-blue because the ROM's trailing `frame.commit()` loop repaints with `enabled_backgrounds = 0` (agb 0.25 derives the mask from the backgrounds shown *into that frame*), erasing the text `show()` had committed. Fixed with E5's shape (`loop {}` under `item6a7-wide`): `.text` 89,484 → **89,432 B** (`9f0b19e924cd…` → `1e14085e36db…`), default image still byte-identical, **every leg's mGBA number identical to the cycle** afterwards. Headless mGBA cannot see this class of bug. **Timing numbers still owed.** |

| **E5 harness check (2026-11-10, mGBA — not hardware)** | `e5-fetch.gba`, `WAITCNT=0x4317`: prefetch on ROM 16,735 / IWRAM 4,248 c/call, ratio **3.939**, ROM/add **4.085**, IWRAM/add **1.037**; prefetch off 16,749 / 4,259, ratio 3.932 | the harness reproduces the ~4-cycle ROM cost; mGBA prices prefetch on/off within 0.1% (it does not model the buffer). **No hardware result** — E5's arbitration is the on-silicon run on the GBA 001 and GBA SP. |

| **E5 hardware arbitration (2026-11-10, GBA 001 + GBA SP, two runs each)** | prefetch on: ROM **16,733** / IWRAM **4,248** c/call, ratio **3.939**, add **4.085 / 1.037**; off: 16,747 / 4,257, ratio 3.933, add 4.088 / 1.039. Both units and both runs are identical. | hardware reproduces mGBA within **2 cycles/call** on ROM (0.012%) and **≤0.03%** on the ratio; prefetch on/off moves ROM by 14 c/call (0.08%) and IWRAM by 9 (0.2%). The 6a-5d placement is real; the kernel-level stack (39.74%) is **6a-7b**, whose instrument is landed and whose timing numbers are owed. |
| **7-0: the asset re-authored and encoded at 32,768 Hz, paired against its own control (2026-11-13, agent host + mGBA headless — **not** hardware; control = `git worktree add --detach /tmp/ctrl70 f4777f8`, built before any edit on the same toolchain: `rustc 1.98.0-nightly (a3563f0602 2026-12-09)`, `nightly-2026-09-04` GBA tier, `flac`/`metaflac` 1.5.0)** | **Images.** `flac-integration.gba` `46a8d38e…` → `5e212af0…`; `product-play.gba` `6aaec4c1…` → `7e0ecf74…`; `flac-spike.gba` `227c0f29…` → `9c8a0d0b…`; a clean control reproduces all three 7-0 hashes, and the obvious command is the one that fails — from the root, `cargo clean -p flac-lite …` answers *package ID specification did not match any packages* and exits 101, the root manifest being one package rather than a workspace containing those members — so the control that works is `cargo clean` inside `examples/flac_integration` and `examples/flac_spike`, then `make native-flac-rom native-product-play-rom native-spike-rom` with no prior artifacts; all three hashes above reproduce and both rebuilt product images pass again (`verdict PASSED — screen BLUE` on each, exit 124 as a pass-and-loop run must). The control caught a stale artifact: the pre-clean `flac-spike.gba` on disk was `47b09f02…`, which is this code carrying the control's old 32,000 Hz spike assets from the cross-experiment arm, whose files were restored without that image being rebuilt with them; the clean tree gives the recorded `9c8a0d0b…`, so `47b09f02…` is a fourth point on the pair and not a fifth build of the 7-0 tree. ELF sections (the `.gba` is not ELF — `agb-gbafix` rewrites the header and `readelf -S` says *"Not an ELF file"*, so sections are read from `examples/*/target/thumbv4t-none-eabi/release/*`): `flac-integration` `.text` **0x010bc4 → 0x010bc4 (+0)**, `.rodata` 0xf3fec → 0xf82e4 (+17,144), `.iwram`/`.ewram`/`.bss` unchanged, file 1,070,768 → 1,087,912 B; `product-play` `.text` 0x136f4 → **0x137b8 (+196)**, `.rodata` 0xf4ef8 → 0xf92fc (+17,412), `.bss` 42,672 B unchanged, file 1,085,676 → 1,103,288 B; `flac-spike` `.text` 0x0cad8 → **0x0c504 (−1,492)**, `.rodata` 0x1beb10 → 0x1c6de0 (+33,488), file 1,883,140 → 1,881,756 B. `nm --print-size --size-sort` with hash-suffix normalisation: **`flac-integration` total +0 B with zero symbols changed**; `product-play` +196 B all in `product_play::rom::report` (+192) and `::manifest` (+4) with four `String` glue objects moving ±4–12 B, no decoder symbol present; `flac-spike` −1,494 B, of which `residual::decode_residual_with<TableCountKernel>` −1,716 against `TableCountKernel::decode_partition` +266 and `read_bits` ±576, `count_ones` unchanged at 6 B in both. **Assets.** `spike_l0.gfp` 865,996 → **882,949 B (+1.96%)**, frames 157 → **160**, samples 320,000 → **326,144 (+1.92%)**, final frame a 512-sample tail; `spike_l0_frames.bin` 865,340 → 882,281, `spike_l4_frames.bin` 835,884 → 852,255, PCM 1,280,000 → 1,304,576 B. New pins: `FNV_PCM 0x054DD8B24062C27D`, `FNV_FIFO_L 0x15D1CF3167E9D6A9`, `FNV_FIFO_R 0xE830329FF1056309`, `MAX_FIXED_ORDER 0`. **The player pair, same instrument both sides.** Control: `stream 32000 Hz -> playback 32768 Hz … (the stream's own nominal period would be 1073741)`, `hw_periods=571 serviced=571 und=0 coalesced=0 late=0 tail 1/240`, `feed: pushed=consumed=320000 frames=157/157`, **work 53.386% / dec 39.527%**, min_lead 9120, `fit: lines=9 … fits=true`, BLUE. 7-0: `stream 32768 Hz -> playback 32768 Hz … the stream's own nominal period is 1048576 c — equal by the rate contract`, `hw_periods=582 serviced=582 und=0 coalesced=0 max_gap=0 late=0 max_delay=0 grace=16 tail_periods=1 tail_real=224`, `feed: pushed=326144 expected=326144 consumed=326144 frames=160/160 underflows=0`, both digests MATCH, **work 53.446% / dec 39.574%**, min_lead 9120, `fit: lines=8 right=132 bottom=99 fits=true`, BLUE. **The bench, three-way (the isolation that says which side moved).** control code + control frames: mean **1,470,570** c/frame, worst 1,477,975 (157 frames); 7-0 code + **control** frames: mean **1,686,388**, worst 1,694,849; 7-0 code + 7-0 frames: mean **1,686,658**, worst 1,694,961 (160 frames) — and the L4 arm likewise 2,755,752 / 2,968,627. **Pitch, at both depths, on the shipped bytes** (integer matched-filter, two ratios thresholded at 10): clean design term **410:1 / 212:1**, the same term authored at 32,000 **413:1 the other way**; shipped centre band 2.9240 counts at period 256, −0.0086 at period 250, shoulders 0.0140 → **340:1 / 209:1**; its played 8-bit form **66:1 / 20:1**; the 801/3 harmonic probe 26.7 counts; the rejected 32 Hz anchor answers 81% at period 1023 against 1024. Through the real `convert_frame`: a ±24,000-count tone measures **93.7579 LSB against the exact 93.7500 (0.008% out)**, 414:1 / 211:1; a **half-LSB** tone measures 0.7499 LSB, 394:1 / 209:1; the fixture's own anchor is **2.9240 i16 counts = 1.1% of one FIFO step** (44× below the half-LSB control) and at the FIFO depth takes exactly the bytes `0x00, 0xFF`. **Gates:** `check`, `test-rom`, `test`, `flac-gates`, `product-play-test`, `item6a5-test`, `overlay-kernel-test`, `spike-test`, `kernel-test`, `opus-test`, `gafp-check`, `gafp-pack-check` all exit 0 — 1,509/0/3 ignored, 203/0, product-play **14** (10 at control); `test-rom` exits 0 with **0 cases collected**, as at the control. `flac-spike`'s correctness censuses: `fixed0 x160` per slot at the product profile, `lpc3 x1, lpc4 x159` per slot at `-l 4`. | **Three separations, and one number that is not a measurement.** **(a) The product decode path's code did not move**: +0 B of `.text` and zero changed symbols on `flac-integration`. So the only honest cross-tree comparison on the product side is data-vs-data, and that pair is the player one: **+0.047 points of decode share, +0.12% relative, +11 periods of tail audit from 2.4% more samples**, with underflows, coalescing, lateness and the watermarks unchanged. Nothing in this PR makes the product cheaper or dearer in any way that matters; C2 is still unproven because it has never been run on silicon. **(b) The bench's −1,492 B of `.text` is code-motion, not a faster decoder**, and the bench's **absolute** numbers are not comparable across this change: identical frames run +14.7% dearer (FIXED) / +7.7% (LPC) through the 7-0 code, while the 7-0 frames through the same code are **+0.016%**. Read the other way round — "the new asset costs 14.7% more" — it would be an error of two orders of magnitude. Every E-menu anchor comparison that crosses this change must be re-run, not re-quoted. **(c) The censuses are the fixture doing its job at the new rate**: the re-authored material still keeps `-l 0` at FIXED-0 on every slot and still makes `-l 4` choose LPC, so 7-0 did not accidentally change the property C3 was chosen for; the pitch-defining partial moved (250 → 256) and the low partial moved off a now-non-integer period (80 → 82, since 32,768/80 = 409.6), while the broadband partials deliberately kept their periods. **(d) The performance condition the plan already carries is unchanged**: `flac-spike`'s PR-3 verdict stays FAILED (`within budget 0/2`, 1.69 Mc and 2.98 Mc against a 1,048,750 c budget) on control, on 7-0 and on the cross-experiment alike — the bench's budget line does not belong to the product path, and this row does not reinterpret it. **(e) Not a measurement of the new bytes:** the packer's cost row for `fixed0 @ b2048` is inherited from pre-7-0 frames, and `gafp_pack.py` says so on every successful pack (`the region being packed is not the region those cycles were measured on … the match is structural`). 7-1 owns the figure that clears that warning and the `this is NOT a product blob` note. **(f)** The 1,073,741 c in the control's line and the 1,048,576 c in this one are the *same* deadline; only the stream's nominal period moved, from a number nothing in the pipeline was honouring to the one the clock actually runs. |
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
- **6a-5c re-took it on a changed kernel body.** Two runs of the L1 image (680 B
  body) and two of the L2 image (660 B) are each `***`-identical (664/664 lines),
  and the vend leg prints the *same* sums to the cycle in all three images
  (167,872,486 / 925,235,241) even though its own code moved 4 bytes earlier in
  the L1 image. A paired delta that changes by exactly `4 × 640,000` cycles is
  therefore a measurement and not a layout artifact: the arm's absolute moved by
  the same amount on every frame, and the leg that did not change did not move.

## What not to quote

- Percent-of-budget figures from a different image than the arm you are comparing.
- Any LPC percentage in a record older than 6a-2 without the absolute cycle pair.
- `0x0300022c` as a fixed table address.
- A worst-frame number without its arm and frame index.
- Any host-side performance claim: `flac-portable-check` is a compile gate.
- The retraction of 6a-5's stack projection, as a *retraction*. 6a-5b confirmed the
  projected figure to 0.4% because both arms reproduce across images. Quoting the
  projection is fine; quoting the re-opening is stale.
- **Any mGBA cycle count as a hardware cycle count.** 6a-7b measured the same image
  on two units and every leg came out 3.7–6.9% above the model, so 39.74% is the
  emulator figure and **42.72% worst / 42.48% mean** is the hardware one. The
  shortfall is per-leg and does not cancel, so it is not safe to scale one arm's
  mGBA number onto another arm either.
- **6b-3's product-play stage shares as decode numbers.** The window means are one
  image's decode cost *inside a running player* — ring resident in EWRAM, 159
  interrupt boundaries landing inside its own decode windows — against the decode
  row of record's 37.765%, and the frame's share (39.362%) is a third quantity
  again, being 152 windows apportioned over 157 frames. The image prints its
  attributed estimate (420,894) beside the pinned 396,066/398,255 precisely so
  nobody merges them, and prints both denominators so nobody divides a window by a
  frame's budget. One consequence to state rather than leave implicit: the printed
  `work/frame = 556,405 = 53.062%` is a **whole-clip** average (the 152 fed-phase
  windows plus all 571 feeder periods, divided by the run's 157 frames); the
  fed-phase duty cycle is ≈**54.8%** (≈574,700 c/frame), and that is the figure to
  quote as the steady-state real-time load and margin. **The verdict-integrity build
  (2026-11-12) supersedes both figures with `559,845 = 53.390%` (fed-phase ≈55.1%,
  ≈578,300 c/frame):** the +0.328 points are the median clock's own read cost plus
  the removed 79,922-cycle push artefact, not a change in the player's work. **The newest
  row (`c0d3ac36e4580a7c…`) prints `559,800 = 53.386%` with decode mean 428,111 and its
  own `isolated_estimate` 422,248**, and the host model derives its envelope from those
  same printed sums (557,022 raw → 551,159 attributed); the row before it
  (`750d01e97800de95…`, whose log is byte-identical to `694126d406eb9b2a…`'s) printed
  559,848 / 428,157 / 422,257, and the 420,894 and 556,405 further up are the *2026-11-11*
  image's figures and stay in that row as history.
- **The 32,768-declared rows as "a 32,768 Hz product".** They are the *shipped*
  frames with the rate spelled a different way — same samples, same PCM, same byte
  sizes — so they answer a format-and-cost question, not "what does a 32,768 pipeline
  asset cost": a real one carries 2.4% more samples per second of playing time and
  whatever census its own encode chose. The same caution disqualifies the
  −20 c/frame decode-only difference as a *speedup*: it is the instrument's floor,
  and the honest reading of both pairs is "no difference".
  **Ruled on the same day (Q14), which turns that caution into a sequence:** the
  32,768-declared blobs in this section are probes, and the asset that will be quoted
  as "the product" is a resample-at-32,768 encode (plan item 7-0), whose 2.4% larger
  blob and fresh predictor census are still unmeasured — as is the hardware row at
  `(32768, 2048, fixed, 0)` its encode profile waits on (7-1). Until those exist, every
  32,768 figure in this section is about a stream that was re-spelled, not re-made.
- **Image hashes move on comment edits; cycle rows do not.** Recorded 2026-11-11, when
  the rows above were re-verified after the Q14 ruling was written into the sources.
  Editing only comments — doc comments in `src/product_play_main.rs`, doc comments in
  `src/geometry.rs`, prose in the docs, nothing in the geometry, nothing in the flow
  control, nothing in the decode path — rebuilt
  `product-play.gba` at the **same 1,083,488 B and byte-for-byte the same section map**
  (`.entrypoint` 0x17c, `.text` 0x13048, `.rodata` 0xf4d14, `.iwram` 0x7f4, `.ewram`
  0x190, `.bss` 0xa6b0) but at a **different image hash**: `51af16f06b144ca8…` instead of
  the `5dc786951b91b356…` the rows above cite. The mechanism is in the image: it embeds
  panic-location metadata (`strings` finds `src/product_play_main.rs` in the ROM), so
  moving a `assert!` down a few lines by adding a comment changes bytes without changing
  code. The re-run is the proof that nothing else moved: the headless log of the new
  image `cmp`s **byte-identical** to the recorded 46-line run — every count, every cycle
  figure, the same `verdict PASSED`. Only `product-play.gba` moved, and it moved because
  it is the only image whose own file's `assert!` lines shifted; the two decode-only
  images, whose `main.rs` was not touched, are still `f73097c144fe4157…` (1,070,960 B)
  and `aecae6ef815d48c0…` (1,068,164 B).
  *(Third image, same day, and this one moved because a line of the report did: fixing
  the share line's denominators — see the 6b-3 row above — rebuilt `product-play.gba` at
  `c06d993cabcc18c1…`, 1,083,364 B, with every sum, mean, worst, boundary count, audit
  field and the verdict identical to the two images before it. Three hashes, one
  behavior: `5dc786951b91b356` (as first run) → `51af16f06b144ca8` (after comment-only
  edits) → `c06d993cabcc18c1` (after the report fix).)*
  *(Fourth image, 2026-11-12, doc-only: correcting the judge's `pcm MATCH` wording in
  `src/product_play_main.rs` rebuilt `product-play.gba` at `cd9f0044dc4696c1…`, same
  1,083,364 B, and the headless log `cmp`s byte-identical to the run above — the same
  decode witness, described as what it is.)*
  *(Fifth image, 2026-11-12, verdict integrity: the audit gate, the median-of-three
  clock and the scaffolding screen line rebuilt `product-play.gba` at
  `fe00b590c71fb28e…`, 1,084,572 B. This one is a code change, so its numbers move as
  the verdict-integrity row records — work/frame 53.390%, worst frame 53.965% — and
  the pre-guard 59.367% is identified there as the clock artefact the median removed.)*
  *(Sixth image, 2026-11-12, played-form witness: the FIFO pins, the post-audio proof
  fold and the screen `fifo ok` rebuilt `product-play.gba` at `ba237039e394330f…`,
  1,085,604 B. The audio-phase numbers are unchanged within a few cycles (the fold is
  post-audio); the push-path attempt is recorded in the played-form row as the 128%
  measurement that moved it off the hot path.)*
  *(Seventh image, 2026-11-12, verifier/hygiene: the bounded drain/wait, measured-window
  mean, release length checks, renamed sample units, `58.51 periods/s`, allow-list Q14
  assert — rebuilt `product-play.gba` at `694126d406eb9b2a…`, 1,085,804 B. The audio
  numbers moved by 1–72 cycles; the log differs only where those strings are.)*
  **The reporting rule this settles:** a `.gba` hash pin is only meaningful next to the
  source tree that produced it, and it is not evidence of behavior. The behavior claims in
  these rows are the ledger, the period audit and the cycle figures, and those are what
  reproduce. A future re-verification should cite the hash of
  *its own* build and say which older image the recorded run used, which is what the rows
  above do.
  *(Eighth image, 2026-11-12, report hygiene: corrected source comments and host-side
  cost constants rebuilt `product-play.gba` at `750d01e97800de95…`, 1,085,804 B — same
  size, same section map as the seventh — and its headless log `diff`s **byte-identical**
  to the seventh image's run. That is what makes this pass a description change rather
  than a measurement: no sum, mean, worst, boundary count, audit field, digest or verdict
  moved. What had been wrong is which image the **host model's** cost constants were
  transcribing; see the report-hygiene row above and item 3 of 6b-3's corrections in
  `plan-of-record.md`.)*
  *(Ninth image, 2026-11-12, the sustained-overage correction: `product-play.gba` at
  `c0d3ac36e4580a7c…`, 1,085,868 B. The ledger, audit, watermarks, digests and verdict are
  identical to the eighth image; the stage and attribution numbers moved by tens of cycles
  because the `armed:` string and the doc text changed length, which is a layout effect,
  and the host envelope is re-read from this build's own log. The arithmetic that moved is
  in `sustained_overage_tolerance`: the throughput ceiling is 186.5%, not 152.9%, and the
  ring's sustained purchase is 4.1 points, not 38. See the row above.)*
- **Any figure quoted as "a percentage of the measured cost" without its envelope.**
  The sustained ceilings are computed from the cost constants in force, and those
  constants belong to a named run: 6b-3's model carried the 2026-11-11 image's sums
  three builds past them, and the ceiling moved once they were re-read from the image
  the tree builds. Quote the pair — ceiling and the envelope beneath it —
  or quote neither.
- **The "throughput ceiling" as an integer number of periods per frame.** Recorded
  2026-11-12: `frame_cycles / PERIOD_CYCLES` is 3.657, and truncating it to 3 threw 18%
  of a frame's cycles away — 152.9% of the measured cost where the clock's bound is
  **186.5%** (1,027,946 c/frame). Multiply before dividing, and if a claim says the ring
  buys *sustained* overage, check it on a clip longer than one: a bounded buffer buys
  time, so the measured tolerance must fall toward the clock's ceiling as the clip grows
  (190.6% at 9.766 s, 188.2% at twice that).
- **"The geometry has X% margin" as one number.** The sustained and transient
  regimes answer differently by an order of magnitude (186.5% of cost is the clock's own
  sustained ceiling, with the ring adding 4.1 points of *time* on the shipped clip, vs 17
  periods of stall at the shipped prefill). Quote the regime, not the margin.
