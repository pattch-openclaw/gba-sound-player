# Plan of record — the only live plan

Everything else in these docs describes what has been built or measured. This file
describes what is being built next, what bar each piece is judged against, and the
rules a landing may not bend. It replaces the item-5 plan (fully landed), the
"Phased plan" (fully landed or superseded), the perf-spike five-PR plan (PRs 1–3
landed; 4–5 open), the E10 plans (landed) and the rate-tooling proposal (landed,
with its step-4 multi-clip ROM and its cross-rate sweep still open) — all of which
now live only in git history and in the records they produced.

## Status at a glance (2026-11-09, branch `feat/flac-item6a5b-vendored-differential`;
the 6a-5 realization reading is on `feat/flac-item6a5-realization-reading`, the
projection re-opening on `docs/flac-6a5b-projection-reopened`)

- **Landed:** the Phase-1 decode path through `decode_frame`; the
  stream layer; the per-frame `driver`; CRC verification behind `crc-check`; the
  table-less `scan` walk (feature, off); the `RiceKernel` seam (item 5, all four
  PRs) with the portable twin behind `rice-hybrid`; the overlay crate (6a-1), the
  same-image re-anchor (6a-2), the out-of-crate seam arm (6a-3), and **the ARM
  kernel itself (6a-4)** — asm, FFI, state-struct ABI, pinned tables, plus the
  kernel-check ROM that is the device-side witness.
- **Nothing in any shipped decode path has changed since item 5 PR 4.** The
  default is still `TableCountKernel`; `.text` 46,008 B `f0f88c33d150…`; the
  overlay links into exactly one ROM, the correctness instrument.
- **6a-5 landed too, on a branch:** the realization reading. The asm realization is
  **−584,856 cycles/frame (FIXED) / −286.9 cycles/sample** against the shipped
  kernel in the product-shape image, 157/157 frames faster on both clips — far
  outside the pre-declared noise band's `faster` side, so the fork resolves to
  **carry**. The carry itself is a one-line placement in a product ROM (6b), not
  in this rung: 6a-5's own rule is that no shipped count or instruction stream
  moves, and none did (the product image `flac-integration` is byte-identical; the
  instrument's cycle readings reproduce the record to the cycle).
- **The stack verdict is measured, and the projection was right: 6a-5b, done.** The
  extra arm is in the instrument and the pair is same-image:
  **`asm − vend` = +15,431,627 / +15,409,977 cycles** per 157-frame clip
  (FIXED / LPC) = **+98,291 / +98,153 c/frame = +24.11 cycles per Rice codeword =
  +48.22 / +48.16 c/sample**, the vendored realization faster on **157/157 frames
  of both clips**, reproduced across runs to +5.2 / +13.0 c/frame. That lands within
  **0.4%** of the +97,929 c/frame the 6a-5 projection drew across two records — and
  it lands because both *arms* reproduce across three images (0.02–0.04% apart)
  while only `ctrl` moves with the link. So the landed stack really is
  **103.83%** of the 32,768 Hz budget where the probe's realization projects
  94.47%, and the 2026-11-08 re-opening ("the projection is a cross-image
  comparison") is **refuted**: it measured two realizations, not two images. The
  four static facts bound *instruction counts*, and this cost is not paid in
  instruction counts — the two kernels' instruction streams are now diffed word for
  word and differ by **6 executed instructions per codeword and 3 per partition**,
  which is 24.9% of the gap at one cycle each and needs **4.02 cycles each** to be
  the whole of it. The *fork's* carry decision is untouched (the kernel is still a
  −286.9 c/sample same-image lever) and both product rates fit with it alone (61.7%
  at 18,157 Hz, 35.7% at 10,512 Hz). Nothing is refactored on a reading; the
  guard-marginal probe is what prices it.
- **What that opens, and it is now the largest single quantity on the table:** the
  measured overhang is **9.4 budget points** on the 32,768 Hz frames, against the
  **3.8 points** of deficit the projected stack has. Recovering 41% of it fits 32 kHz
  FIXED with no other lever. The item's central budget question therefore has a
  third candidate beside 6a-6's width leg: making the landed realization cost what
  the probe's realization cost — which is a kernel-side question, not a path one.
- **6a-5c ran and the fork it pre-registered is resolved, cleanly and not the way the
  fork guessed.** The 48.2 c/sample is **6 instructions at 4.0000 cycles each**, and
  nothing else: deleting one provably dead `and` (a shippable fix, all gates green)
  moves the pair by `2,560,000 = 4.000 × 640,000` cycles, and deleting the five
  guard instructions on top of it moves it by another `12,800,000 = 5 × 4 ×
  640,000`, leaving **71,627 cycles of 15,431,627** — 0.46% — which is the bridge.
  So the currency is *instruction count*, and its price is **4 cycles**, which is
  what a 32-bit ARM instruction costs when it is fetched from cartridge ROM on a
  16-bit bus at WS0 = 3,1. The two channels 6a-5b posed (instruction cost vs
  hot-loop layout) were a false pair: layout is excluded (the saving is
  proportional to codewords executed, not to a change of position), and instruction
  cost is real at 4× the rate the fork's own threshold assumed. **E6's
  "instruction fetch is ruled out" is reversed for this case**, which is exactly
  the case the 6a-5d row anticipated it would not cover.
- **Next rung:** **6a-5d**, now unconditional and the largest quantity on the table
  — the same instruction stream resident in IWRAM instead of ROM. If fetch is
  priced at 4 cycles per instruction, IWRAM (0 wait states, 32-bit) is a
  multi-point lever on a 40-instruction loop, not a layout experiment. Then **6a-6**,
  the guarded i32 default width.
- **Not started:** 6a-5d, 6a-6…6a-8, all of 6b, and the product work they unblock
  (manifest, packer, facade, mixer).

## Item 6a — the overlay (the realization)

6a changes *how a frame is decoded*. Its measurements are kernel-shaped: cycles
per frame, attributable to a realization. It does **not** change where frames come
from and it does not turn `scan` on.

| PR | Scope | Witness type | Failure it isolates |
|---|---|---|---|
| **6a-1** ✅ | `crates/flac-lite-gba` exists; nothing links it. No `agb`, no asm, no `unsafe`, no modules. | Build gate + control build: the instrument's sections come back byte-identical (`f0f88c33d150…`) | that adding a crate to the tree changes what the existing ROMs link |
| **6a-2** ✅ | The same-image baseline re-anchor. Measurement only, zero decode-path change: E10/E9 sample patches re-applied to today's base and re-run. | Cycles, same-image; plus the patch-re-application record | that the item's central number compares two instruments |
| **6a-3** ✅ | The seam passable from outside the crate, in Rust: `rice_unmap` visibility bump + `TwinKernel` behind `rice-twin`, carrying the core's twin as an instrument arm. | Section dump + symbol-level attribution; the corpus differential driven from outside; cycles as a control arm | that the seam is only passable from inside, or costs more than the in-core form |
| **6a-4** ✅ | **The asm kernel — correctness only, no timings.** `rice_partition.s` + `fold_body.s` lifted from `examples/arm-kernels/`, `global_asm!`, the FFI declaration and its `SAFETY` block, the state-struct ABI written down, tables under `iwram-pin` (this is also where pin enablement lands). | (i) a kernel-check ROM sweeping the **same committed golden tables**; (ii) `objdump -d`: 4-byte ARM encodings, `pop {…,lr}` + `bx lr`, never LDM-to-PC; (iii) the boundaries item 5 handed forward — spelled `q = 31` leg, `w == 0` handoff signal, `p = 0` mask, §9.2.7.3's two guards, unterminated/short-remainder **restore-nothing**, reads bounded by `region()` | the kernel is wrong, or the interwork/ABI is wrong — settled **before** any cycle is trusted |
| **6a-5** ✅ | **The realization reading — the item's number of record. No new code.** Three arms, one image: `ctrl`, 6a-3's Rust arm, 6a-4's asm arm; calibration subtracted, paired per frame, every delta byte attributed symbol by symbol. Carries the pre-declared fork below. | Cycles, same-image; codegen attribution. **The first ROM suite with a test in it is 6a-4's kernel check**; 6a-5 inherits a non-empty suite rather than "first to run `make test-rom`" (which already runs, collecting zero) | the win lives in a probe but not in the product path |
| **6a-5b** ✅ | **The paired realization differential. No new code — one more arm in the existing instrument.** E10/E9's committed vendored `e10::decode_one_arm` (i64, so it matches the landed `asm` arm step for step) timed in the **same link, same pass, same calibration** as `ctrl` and `asm`, so `asm − vendored` is a same-image pair. Re-opened the 103.83% projection, which compared two images. | Cycles, same-image, paired per frame; the per-frame sample-equality witness. Instrument-only: no shipped count or instruction stream moves | that the item's stack verdict is a comparison of two *images* rather than of two *realizations* — **it is not**: the projection reproduced to 0.4% inside one image |
| **6a-5c** ✅ | **The guard marginal, and the one instruction that is not a guard.** One variable at a time in the same image: **(L1)** delete the dead `and r12, r3, #0xFF` the landing left behind — semantics untouched, `asm` shrinks 684 → 680 B, 171 → 170 words; **(L2, diagnostic)** delete G1/G2's 5 instructions (they never fire on this corpus); **(L3)** re-spell G1 where the asm header names it, still open. | Cycles, same-image, `asm − vend` re-read per leg; `overlay-test-arm` + `overlay-kernel-test` (the §9.2.7.3 rejection arms at orders 14–16 on the device) + `item6a5b-test` re-run for L1 | that the only *proven* per-sample cost of the landing stays unpriced, and that a cost attributed to "the guards" is in fact priced in instructions rather than in fetch/layout — **it is instructions, at 4.0000 cycles each**, so the guards' *count* is their price and "fetch" and "instruction cost" were never two things here |
| **6a-5d** *(now unconditional, and the biggest lever on the table)* | **The codeword loop's placement, which 6a-5c made the question.** The same kernel, the same instruction stream, resident in IWRAM instead of ROM (plain `.iwram` — `.text_iwram` is E6's trap). 6a-5c measured the price of an instruction at **4.000 cycles**, which is the ROM fetch cost; IWRAM is 0 wait states, 32-bit, and is where an ARM7TDMI loop belongs. | Cycles, same-image, `asm − vend` and the absolute arms; the `objdump` kernel gate re-read at the new address; `.iwram` budget named (32,512 B, ~1.9 KB used) | that a cost paid in instruction *fetch* was measured, named, and then left in the wrong memory, because a 2024-era probe of a different code shape said fetch did not matter |
| **6a-5d** *(conditional on 6a-5c)* | **Placement of the codeword loop, if 6a-5c says the cost is fetch.** The same kernel, the same instruction stream, resident in IWRAM instead of ROM (plain `.iwram` — `.text_iwram` is E6's trap). E6 ruled instruction fetch out, but its variant arm also forced `inline(always)`/`inline(never)` and took linker veneers, and it measured a *Rust* path where each instruction carries many cycles of work; a 40-instruction ARM loop of one-cycle instructions is the case E6 did not isolate. | Cycles, same-image; the `objdump` kernel gate re-read at the new address; IWRAM budget (32,512 B; 1,908 B used) named | that a cost paid in fetch is mistaken for a cost paid in instructions, twice |
| **6a-6** | **The width change, in the core.** i32 as the default arithmetic width, guarded per frame, i64 kept and reachable. Design in "The width decision" below. **The stack verdict is not this rung's** — it is 6a-5b's, and 6a-6 must not start on a projection taken across images. What 6a-6 carries is the width half: its measured marginal replaces E9's borrowed one, and for the FIXED clip that half has exactly one named site (see 6a-5b). | Bit-exact vs `flac -d`; a width differential over the corpus and every committed frame-run table; the guard's unit witnesses incl. refusal cases; **both** configurations gated; codegen attribution of a both-legs build; cycles as a same-image arm | the guard is wrong (unsound), the legs are not equivalent, or a both-legs build costs more than the win |
| **6a-7** | **The stack confirmation. No new code** — one measured arm: 6a-4's kernel **and** 6a-6's default together, against the same-image `ctrl`. Re-anchors the E numbers on the post-6a image, which 6b-1 inherits. | Cycles, same-image | that two measured wins composed negatively (E9's additivity does not reproduce) |
| **6a-8** *(conditional)* | E8-style **sub-stage probe** over the asm kernel: tier-1 / tier-2 / tier-3 / refill / fold priced separately. Run **only** if 6a-5 lands inside or near the noise band. | Cycles, sub-stage | that a near-bar result is unattributable to a leg |

### 6a-4 — landed, and what the device witness actually found

The rung landed with one correctness bug that **no host-side arm could see**, found
by the ROM's first end-of-stream case and fixed here. That is the whole argument
for the rung existing, so it is written down rather than tidied away.

**The bug.** The probe's kernel used an 8-byte frame and read its three stack
arguments at `[sp+44]`, `[sp+48]`, `[sp+52]`. The landed `[6a-4 FRAME]` delta is
12 bytes (`[sp+8]` holds the hoisted §9.2.7.3 limit), so the arguments move **one
word**, not two: `[sp+48]`, `[sp+52]`, `[sp+56]`. The first cut added 12 instead of
4 and read `[sp+56]`, `[sp+60]`, `[sp+64]` — i.e. `BRUIJN_INDEX` where `CTZ8`
belongs, and the caller's frame for the other two. On the host every arm stayed
green, because the host's FFI leg is `model_partition` and the model receives those
pointers as Rust arguments: a wrong stack slot is not a wrong answer until the
assembly runs. On the device, `make overlay-kernel-test`'s first
`#[test_case]` returned `Done` where the incumbent said `EndOfStream`
(`8 mismatch(es)` in that family). Fixed; the offsets are now pinned by an
AAPCS-arithmetic source check in the host suite (`36 + 12 = 48`) as well as by the
ROM.

**Control build.** From `HEAD` (`0e16439`) in a separate worktree, before any
source edit: `flac-spike` `.text` **46,008 B `f0f88c33d150…`**, `.iwram` 812 B
`eeda688aa1a2…`, `.ewram` 368 B `c8beeb9e3f5f…`, `.entrypoint` 380 B
`7d6b62f63b70…`, `nm` anchors `LEADING_ONES @ 0x0300022c` (256 B), `rice_quotient`
336 B, `decode_residual` 596 B — the record of record exactly. After the change the
same image is **byte-identical in all five sections and all three anchors**;
`cargo clean` + rebuild reproduces it. `flac-integration` is unchanged in every
size (`.text` 33,100 B, `.iwram` 556 B, `.ewram` 368 B); its `.text` digest differs
between two build *directories* (measured: a clean rebuild of one tree reproduces
its own digest, so the digest is path-sensitive in that crate) — so it is compared
same-tree, where it is identical.

**The new image** (`overlay-kernel-check.gba`, built `rice-arm,iwram-pin`;
sha256 `5842ebde2c43981e…`): `.text` 67,772 B `8d8c7fe85a14…`, `.iwram` **1,356 B**
`a568ddbb5c5f…` (812 + **544**, the three tables exactly as the plan budgeted),
`.rodata` 145,988 B, `.ewram` 368 B, `.entrypoint` 380 B, 243 symbols.
`arm_rice_partition` is **684 B at `0x0800017c`** (ARM state, 171 instruction words,
one `mul`, 22 base+register `ldrb`); `LEADING_ONES` keeps `0x0300022c` and the
tables sit at `0x0300032c` (`CTZ8`), `0x0300042c` (`REV8`), `0x0300052c`
(`BRUIJN_INDEX`) — the ROM's own test asserts those ranges at runtime.

**Gates.** `flac-gates` **1,169 passed + 6 ignored** (unchanged); `flac-test`
178 + 1; `spike-test` 15 + 10; `kernel-test` 20; `opus-test` 28; `test-rom` 0
cases (still the standing state — the repo's first non-empty ROM suite is this
PR's, in a different crate); `overlay-check`, `overlay-test` (default config,
1 lib test), `overlay-test-twin` unchanged. **New configurations, named as new
gates:** `overlay-check-arm`, `overlay-test-arm` (15 host arms: corpus
differential, 5,524 swept boundary cases / 6,166 samples with 2,648 delegated,
11 guard rejections + 10 near-miss controls, the `p = 0` unary-only law, 5
end-of-stream shapes, an out-of-range/delegation arm, the table re-derivations,
the `unsafe` budget, and the corpus census), `overlay-objdump`, and
`overlay-kernel-test` — **8 device `#[test_case]`s**, the first ROM suite in this
repo that runs assertions.

**Mutation witnesses** (each applied, run, reverted):

| Mutation | Host suite | Device suite | Verdict |
|---|---|---|---|
| Stack slots 48/52/56 → 56/60/64 (the bug above) | **red** — the new AAPCS slot pin (behavioural arms stay green) | **red** on the first end-of-stream case | caught *only* on target until the pin existed |
| Swap the remainder un-reversal's two `REV8` bytes (`orr r3, r12, r3, lsl #8`) | **green 15/15** (the model is untouched — the mutation is asm-only) | **red**: `the samples written before the failure are not the packer's` | the device witness is the only witness of the instruction stream |
| `bx lr` → `mov pc, lr` | — | — | `overlay-objdump` red: "no `bx lr` in the body" |
| `pop {r4-r11, lr}` + `bx lr` → `ldmfd sp!, {r4-r10, pc}` | — | — | `overlay-objdump` red twice: pop does not restore lr, and LDM-to-PC |
| Behaviour-neutral: delete the duplicated `and r12, r3, #0xFF` | **green** | **green 8/8** | an honest green row — a no-op edit does not read as a failure |

**What 6a-4 deliberately did not do**, re-stated because the next rung depends on
it: no cycle claim (E10's −349/−348 is a different image with a different driver),
no default moved (`TableCountKernel` is still named on one line), no frame or
stream decoded through the kernel, no escape partition, no `agb` in either library
crate, and no hardware claim. The ROM cannot construct the two largest guard
regions (§9.2.7.3 at orders 12 and 13 are 131 KB and 65 KB against 256 KB of
EWRAM) and the `p = 0` extreme (2 MB); those three cases are host-only and the
device suite **counts** them rather than dropping them silently — 6a-5's record
inherits that split.

### 6a-5 — the realization reading, and what it cost to get one

**The number of record.** Through the product call path
(`driver::decode_one_with` → `frame::decode_frame_with` → `decode_subframe_with` →
`decode_residual_with` → the partition seam), same image, calibration subtracted,
paired per frame:

| Arm | FIXED sum (157 frames) | FIXED c/frame | FIXED c/sample | LPC sum | LPC c/frame | LPC c/sample |
|---|---|---|---|---|---|---|
| `ctrl` (`TableCountKernel`) | 275,111,047 | 1,752,299 | 859.7 | 1,031,916,549 | 6,572,716 | 3,224.7 |
| asm (`ArmRiceKernel`) | **183,288,727** | **1,167,444** | **572.8** | **940,629,832** | **5,991,272** | **2,939.5** |
| **asm − ctrl** | **−91,822,320** | **−584,856** | **−286.9** | **−91,286,717** | **−581,444** | **−285.3** |

157/157 frames faster on both clips (FIXED max delta 587,680 @147; LPC 584,490 @71).
As a fraction of the 32,768 Hz budget: FIXED **111.32%** mean (from `ctrl`'s 167.08%
in the same image), LPC 571.28% (from 626.72%). Still over budget — the stack that
fits FIXED at 32 kHz needs 6a-6's width leg — but the kernel's share is now real,
landed code rather than a probe.

**The verdict: carry.** −584,856 c/frame is on the fork's `faster` side by four
orders of magnitude against the ~9–13 c/frame band, 157/157 frames, on both clips.
**What "carry" means as code in this PR: nothing yet, by the rung's own rule.** The
core cannot name an overlay kernel (the dependency arrow is overlay → core), so the
placement is one line in the *product's* decode path — `decode_frame_with::<ArmRiceKernel>`
at 6b's facade — and 6a-5 changes no shipped count and no instruction stream. The
API that makes the placement one line is this rung's deliverable.

**The seam had to be threaded, which the plan's "no new code" did not price.** The
core's own docs deferred it: "threading a choice up through [the caller-facing
layers] (and through `scan`/`driver`) is item 6's decision". 6a-5 took it because
the alternative is the failure the rung exists to rule out: an arm instantiated at
the partition entry (6a-3's instrument-arm shape) is a probe, and E10/E9's numbers
came from *vendored decode loops*. The chain is now
`decode_frame_with<K>` → `decode_subframe_with<K>` → `decode_residual_with<K>` →
`decode_rice_partition_with<K>`, each non-generic entry point a one-line delegation
to `…::<TableCountKernel>`; [`decode_residual`'s delegation](architecture.md) is
where the default now lives. The retired private `decode_rice_partition` was that
home while nothing above it took a kernel.

**Control build and what moved.** From `HEAD` (`f1fb6da`), same tree, before the
change: `flac-spike` `.text` **46,008 B `f0f88c33d150…`**, `.iwram` 812 B
`eeda688aa1a2…`; cycle control **280,848,260 / 1,037,633,086**, calibration 22
stable, 157/157 frames, min/max/indices as recorded. After:

* **`flac-integration` (the product image) is byte-identical** — `.text` 33,100 B
  `075658280b4c…` and every other section unchanged. The shipped decode path's code
  did not move.
* The instrument's **cycle readings are identical to the cycle** (same four sums,
  same min/max/frame indices, calibration 22 stable). The measurement did not move.
* The instrument's `.text` is 45,996 B `f63ff65bedea…` (−12 B) and `.iwram`'s digest
  moved with its symbol order. Attributed symbol by symbol: `decode_residual` 596 B
  → `decode_residual_with::<TableCountKernel>` 596 B and `decode_subframe` 888 B →
  `decode_subframe_with::<TableCountKernel>` 888 B (same sizes, same addresses,
  renamed); every remaining byte difference is a literal-pool relocation target
  (the normalized `objdump -d` instruction stream is **identical**); −12 B is the
  ROM's own inlined harness loop (`__agb_main_func…inner` 8,592 → 8,580), which is
  instrument code and moves no cycle (the control re-run above). The 12 B came from
  keeping one transcription of the seek-table path: the spike's `decode_one` body is
  a `macro_rules!` expansion at two sites (default and kernel-parameterized) rather
  than a generic function, because a `pub fn decode_one_with<K>` changed what a
  default build links by **+2,868 B** — measured, then rejected in favour of the
  expansion.

**The image geometry (Q2, answered by measurement).** Three images, one variable
each: **A** the shipped instrument (default features); **C** `item6a5-asm` —
`ctrl` + the ARM kernel only, the *product shape*; **B** `item6a5-arms` — `ctrl` +
the Rust mechanism + the ARM kernel in one link. Both arm features also turn on the
overlay's `iwram-pin`, matching 6a-4's enablement and removing an asymmetry (the
spike already pins the core's `LEADING_ONES`; an arm whose own tables sat in ROM
would pay a read tax the control does not).

| Image | `.text` | `.iwram` | `.rodata` | ctrl FIXED | asm FIXED | rust FIXED |
|---|---|---|---|---|---|---|
| A (shipped) | 46,008 B | 812 B | 1,829,760 B | 280,848,260 | — | — |
| C (`item6a5-asm`) | 63,124 B | **1,356 B** (812 + 544) | 1,830,876 B | 275,111,047 | 183,288,727 | — |
| B (`item6a5-arms`) | 72,428 B | **1,868 B** (+544 asm, +512 hybrid) | 1,831,228 B | 249,051,451 | 183,231,371 | 301,587,959 |

Two facts fall out of that table, and they are the answer to Q2:

1. **The asm arm's absolute cost is a property of the kernel, not of the link**:
   183,288,727 (C) vs 183,231,371 (B) — **0.03% apart** across two different links,
   two different `.iwram` layouts and two different bundle sizes.
2. **`ctrl` is not**: 280,848,260 (A) → 275,111,047 (C, −36,543 c/frame) →
   249,051,451 (B, −202,529 c/frame). A second `RiceKernel` body in the link
   re-decides inlining for the shared decode path, exactly as 6a-3 measured; the
   effect is monotone in how many mechanisms are present. **So a "three arms in one
   image" reading can only be a paired, same-image reading, and the product number
   must come from the product shape** — B's `asm − ctrl` is −419,236 c/frame while
   C's is −584,856. Both are far outside the noise band; only C's is the number for
   the carry.

**The cross-check 6a-3 handed forward.** Item 5 PR 4 measured the Rust mechanism at
**+42.60 / +42.72 c/sample** against the then-control. In image B it reads
**+164.2 / +163.9 c/sample** — 3.9× larger — while its *absolute* cost reproduces:
1,920,940 c/frame here vs 1,876,087 implied by PR 4 (+2.4%). The difference is
entirely `ctrl` moving −202,529 c/frame in a two-mechanism link. That is not a
contradiction; it is the measurement telling us that **the paired-delta cross-check
is only valid same-image, and the absolute arm cost is the quantity that travels**.
Recorded so nobody "corrects" PR 4's number.

**How much of E10 the landing keeps.** E10's probe arm measured 101.98% of budget
(≈1,069,515 c/frame) with a *vendored* reader and neither §9.2.7.3 guard. The
landed realization, through the seam, is 1,167,444 c/frame — **+97,929 c/frame
(+47.8 c/sample)**, i.e. it keeps **82%** of E10's −349 c/sample probe win. The
gap is the seam's window hand-out/take-back per partition, the two guards (the asm
header's `3 + 2` instructions per sample/partition), the 12-byte frame and the
third status word. Which share belongs to which is *not* attributed here: 6a-8 (the
conditional sub-stage probe) does not run, because the fork resolved far outside
the band rather than near it. The pin's marginal is measured separately and for
free: with the overlay's tables in ROM rather than `.iwram`, the asm arm reads
202,770,217 c/frame (FIXED) — **+124,086 c/frame, +60.9 c/sample** — which is the
data-pin claim item 5 made, re-measured on this kernel.

**Gates.** `flac-gates` **1,169 + 6** (unchanged), `flac-test` 178 + 1,
`spike-test` 15 + 10 (unchanged: no arm code in a default build — the manifest's
`required-features` makes that checked), `kernel-test` 20, `opus-test` 28,
`test-rom` 0, `overlay-check` / `overlay-test` / `overlay-test-twin` /
`overlay-test-arm` unchanged, `overlay-kernel-test` 8 device cases (re-run because
the core moved), `make check` green. **New gate:** `item6a5-test` — 28 passed
(15 lib + 10 spike witness + 3 arm witness): every arm reaches the committed PCM
pin `0x54C7B356621B6E15` through the product path and agrees with `ctrl` frame
window by frame window (314/314 windows on the two clips), and the arm features are
pinned as a manifest fact.

**Determinism.** Two headless runs of one arm image: the 342 `***` lines are
byte-identical (`cmp`). The arm images' *absolute* sums moved by +2 c/frame between
the two driver layouts tried during the rung while every **paired delta reproduced
to the cycle** — the practical statement of why paired deltas are the number of
record and absolute sums are instrument-layout-sensitive.

**Mutation witness (the instrument's validity, not just its code).** With the kernel
plumb bypassed — `decode_one_with::<K>` made to call the default frame decoder — the
image's samples stay identical (they always are: every kernel computes the same
map), and the reading **collapses to `asm − ctrl = 0` on 157/157 frames** with both
arms at 280,296,337 / 1,037,081,163. So the −584,856 c/frame delta is the kernel's,
and a wrong plumb would read as "no win" rather than as a win. Restored; the
restored sources reproduce the numbers above.

**What this cost the stack — the item's first measured negative, re-opened by
6a-5b.** The landed realization is **+15,411,688 cycles/clip = +98,164 c/frame =
+48.2 c/sample (+9.18%)** slower than E10's vendored arm (183,288,727 vs
167,877,039). E9's i32
leg saved 12,330,961 in its own image; applying that same-image marginal projects
the landed stack at **170,957,766 = 1,088,902 c/frame = 534.2 c/sample = 103.83%
mean / 104.36% worst** of the 32,768 Hz budget. The vendored stack sat at
94.47/94.95: **the seam's cost (9.2 points) is larger than the stack's margin (5.5
points)**, so the menu's "first in-budget FIXED decode" does not project through
the shipped path. For the projection to be wrong by enough to fit, the i32 leg
would have to save ~40,200 c/frame more in the seam image than E9 measured (+51%);
the i32 sites are outside the kernel and the seam, so a shift that large is not
credible. It is a structural result, not a rounding — and it is the reason
`perf-results.md`'s E9 caveat (1) was recorded as fired.

The caveats, so this is not over-read: the i32 marginal is E9's, not measured on
this image (6a-6 measures it; 6a-7 confirms); the kernel alone still fits both
product rates (61.7% at 18,157 Hz, 35.7% at 10,512 Hz, versus the i64 control's
94.5% / 54.7%); LPC at 10,512 Hz is 183.3% as landed and 85.3% with E9's i32
marginal; and every number here is mGBA with E5 unrun. **Where the 48 c/sample
goes — the seam's window bridge per partition, the two §9.2.7.3 guards, the
12-byte frame and third status word, or the monomorphization's codegen — is not
attributed.** E10's arm had a vendored reader and no guards, and the conditional
sub-stage probe (6a-8) does not run because 6a-5 landed far outside the noise band
— so the split needs a probe with a *different* justification from the one the
plan wrote (Q12).

**Re-opened 2026-11-08 (6a-5b).** The paragraph above is this rung's conclusion and
stays as the record of it; it is not the stack's verdict. The projection is the one
comparison in these records taken across two records, and four static facts on the
current tree bound what it blames to a fraction of itself. The numbers stand; the
*sequencing decision* they were used to justify does not, and no code is entitled to
change until `asm − vendored` is read in one link.

**Closed 2026-11-09.** It has now been read in one link, and what that reading
found is that this re-opening was wrong and the projection it re-opened was right:
+98,291 c/frame measured inside a single image against the +97,929 projected across
two, because both *arms* reproduce across images (0.02–0.04%) while only the
control moves. The four static facts bounded *instruction counts*; this cost is not
paid in instruction counts — 6 executed instructions per Rice codeword against
24.11 measured cycles per codeword. The sequencing decision it protected (do not
refactor, and do not start 6a-6 on an un-anchored denominator) was still the right
one to take. See "6a-5b — the reading", below, before repeating anything in this
section.

### 6a-5b — the 48 c/sample is re-opened: it is a cross-image comparison (2026-11-08)

**What changed.** The 103.83% projection is arithmetically correct and
methodologically unsound. It adds one record's *paired* marginal (E9's i32,
measured on the *vendored* arm of an image that also carried the e2b four-arm pass)
to another record's *absolute* (6a-5's asm, measured in a two-window image), on a
page whose own rule forbids exactly that ([`method-and-gates.md`](method-and-gates.md):
"No cycles quoted across records … Same-image pairs only"), and whose next
paragraph already carries this record's own evidence for how large that class of
effect is: `ctrl` moves **−36,543 / −202,529 c/frame** between links that carry one
and two `RiceKernel` bodies, with its source unchanged.

Four static facts, all re-derivable on the current tree, bound the seam below the
number the projection attributes to it:

| fact | how it was taken | consequence |
|---|---|---|
| The FIXED clip is `MidSide`, **FIXED order 0 on both subframes, partition order 0**, `wasted == 0`, Rice p = 8 (mid) / 10 (side) — **157/157 frames**, 320,000 samples (matching the record) | a throwaway bit-parser over `examples/flac_spike/assets/spike_l0_frames.bin` (frame header → subframe type/order → residual header; bodies walked to advance the cursor) | **exactly 2 kernel calls per frame.** Order-0 FIXED returns from `integrate_fixed` immediately and `wasted == 0` makes `pad_block` a no-op, so the FIXED frame's per-sample cost is the kernel + the shared mid/side decorrelate + stores — nothing else |
| The seam bridge is **~114 Thumb instructions per call**: two `reverse_bits()` SWAR chains (≈28 each), a 5-word `KernelAbi` built in stack memory, three literal-pool reloads of `&CTZ8/&REV8/&BRUIJN_INDEX`, six clamp compares, one ARM↔Thumb interworking thunk | `objdump -d` of the `item6a5-asm` image, inside `decode_residual_with::<ArmRiceKernel>`: 55 instructions before the `bl` (`0x80014b2–0x800151e`), 59 after (`0x8001524–0x8001598`) | 2 calls/frame ⇒ **≈0.15 c/sample**. Q12's candidate (a), and (c) (12-byte frame, third status word) with it, cannot carry the overhang |
| The asm bodies differ by **5 instructions per sample** — G1's `ldr+cmp+bhi` and G2's `adds+beq` — plus 3 per partition (the hoisted `mvn+lsr+str`) | `diff` of the E10 sample patch's `rice_partition.s` against `crates/flac-lite-gba/asm/rice_partition.s`: the four `[6a-4 …]` deltas are the *whole* difference | the only per-sample code delta there is: **≈5–15 c/sample**, i.e. 10,240–30,720 c/frame of the −40,152 needed |
| The three tables sit at **`0x0300032c / 0x0300034c / 0x0300044c`**, `.iwram` 0x54c | `objdump -t` of the same image | byte-for-byte the addresses E9's own `nm` witness records — table placement is *not* a difference between the two measurements |

Add those up and a genuine +48.2 c/sample has nowhere to live. The decorrelate is
the same shared `flac_lite::stereo::decorrelate` in both arms, the reader
primitives are byte-identical source, and the per-sample kernel delta is five
instructions. What the projection is measuring is the difference between two
**images**, and that difference is not a property of the seam.

**What this does not say.** It does not say the seam is free — it says the seam's
*bridge* cannot be 48 c/sample, and that the landed realization must be compared to
the vendored one **in one link** before any of it is attributed. It does not
retract the fork: −584,856 c/frame is a same-image paired reading and stands. And
it moves no shipped count: nothing in 6a-5b is code.

**The two readings that settle it.**

* **6a-5b — the paired realization differential.** An extra arm in the existing
  `item6a5-*` instrument: E10/E9's committed vendored `e10::decode_one_arm` (i64,
  so it matches the landed `asm` arm step for step), timed in the same link, same
  pass and same calibration as `ctrl` and `asm`, and read as `asm − vendored` per
  frame. The machinery is already there — the three-window pass, `timed_reset`, the
  Opus sentinel rule, the per-frame equality witnesses, and the
  `e9-composite.patch` modules (re-appliable; its README carries the
  `port-item6a2-header-bits.patch` note).

  **Pre-declared fork, written before the reading:** ≤ ~15 c/sample ⇒ the projection
  is void, the landed stack re-projects to ~95–96%, 6a-6 proceeds as planned and
  **nothing in the seam is refactored**; ≥ ~30 c/sample ⇒ the static bound above is
  wrong and the suspect list is the *codegen* of the instantiated path — measured
  today as `decode_residual_with::<ArmRiceKernel>` **2,104 B** against
  `<TableCountKernel>` **1,812 B** (+292 B carrying the delegation branch, the bridge
  and inlined `read_bits`/`peek_at` cold paths in one function) — and the placement
  of the 684 B ARM body, **not** the bridge.

* **6a-5c — the guard marginal, now unconditional.** 6a-5b measured a cost it cannot
  price in instructions, so this rung's job is to separate *instruction cost* from
  *loop layout* before anything is called a fix. Three legs, one variable each, read
  as `asm − vend` re-taken per leg: **L1** the dead duplicated `and` alone (provably
  dead, semantics untouched — a pure layout probe); **L2** G1/G2 removed (the guards
  never fire on this corpus, so the equality witness still holds — but see the
  constraint below: L2 breaks the kernel-check ROM's rejection arms by construction,
  so it can only ever be diagnostic); **L3** the guards re-spelled where the asm
  header already names them, which is the only shippable shape of a win.
  Overlay-side only: the shipped default does not move, and every leg re-runs
  `overlay-kernel-test` (8 device arms — which run the §9.2.7.3 guard cases at orders
  14–16 on the device, with 12–13 host-only) and `tests/arm_from_outside.rs`.

**Why 6a-6 waits for 6a-5b.** Two structural reasons, not scheduling taste.
(1) 6a-6 is the rung that *moves a default*, and its FIXED marginal is being
projected from a quantity whose error bar (the image-effect class, ~99 c/sample
measured on `ctrl`) exceeds the conclusion (48.2 c/sample); measuring the width leg
against an un-anchored denominator buys a second un-anchored number. (2) 6a-6 adds
an arithmetic leg to the link, and adding mechanisms to a link is precisely what
moves the shared path (−36,543 → −202,529 c/frame, same `ctrl` source) — a seam
differential taken *after* 6a-6 inherits that move.

*(2026-11-09: reason (1) did not survive 6a-5b — the error bar belonged to `ctrl`,
not to the arms, and the projection reproduced to 0.4%. Reason (2) survives intact,
and the sequencing it protected was still the right sequencing: 6a-6 remains the rung
that moves a default, and it runs on its own measured i32 marginal, not on E9's.)*

**One thing the clip geometry makes newly predictable, and it belongs to 6a-6.**
Because the FIXED clip is order-0 FIXED and mid-side, E9's `i32 − arm` marginal on
it (−78,541 c/frame = −38.4 c/sample) has exactly **one** active site: the mid/side
arm of `stereo::decorrelate`, which is `i64` with two `>> 1` per sample pair — two
`__aeabi_llsr` calls per sample on ARMv4T. `integrate_fixed` is a no-op at order 0
and `pad_block` a no-op at `wasted == 0`. So the width leg's FIXED half is not "an
average that might transfer": it is one named function, called from the same place
in both arms, and 6a-6 can gate it on that site rather than on the clip average.

### 6a's pre-declared fork (stated before the reading, so it cannot be negotiated)

The asm arm **within the ~9–13 cyc/frame noise band or faster** → carry it: the
overlay kernel is named by the ROM's decode path through
`decode_rice_partition_with`, the placement being exactly one line. **Slower past
the bar** → not carried; the pre-named fallback is the vendored-loop overlay, and
the proposal's realization claim is corrected as a measured negative. No third
arm: "it was faster in the probe" is not a carry condition, and this project has
already seen that sentence cost samples once (E2b's −21.7% became +38% when it
landed).

**Resolved 2026-11-08: carry.** The asm arm reads **−584,856 c/frame (FIXED) /
−581,444 (LPC)** against `ctrl` in the product-shape image, 157/157 frames faster
on both clips — the fork's `faster` side, not a near-bar result, so 6a-8 does not
run. The bar was one-sided (a regression bar), and the reading is what decides it;
the placement itself is one line in the product's decode path
(`decode_frame_with::<ArmRiceKernel>`, 6b), because the core cannot name an overlay
kernel and 6a-5's own rule forbids moving a shipped count. Numbers, images and
attribution: "6a-5 — the realization reading", above.

### 6a-5b — the reading: the 48 c/sample is real, the re-opening of it is not (2026-11-09)

**What was built.** One more timed leg in the existing `item6a5-*` pass (that pass's
calibration line reports `arms=3` where the control reports `arms=2`), behind
`item6a5b-vendored` (which implies `item6a5-asm`, because a differential leg
without the landed leg in the same link measures nothing). E10's module is vendored
into the spike crate (`src/e10.rs`, `asm/rice_partition.s`, `asm/fold_body.s`,
`tests/e10_witness.rs`, from the committed `e10-three-arm-probe.patch` + the
`port-item6a2-header-bits.patch` header port), its kernel symbol renamed
`arm_rice_partition → e10_rice_partition` and its local labels `.L* → .Le10*`
because `global_asm!` symbols and file-local labels are not namespaced and one link
carries both. Default builds are untouched: the module is `cfg`-gated, the crate's
`forbid(unsafe_code)` relaxes to `deny` only in a 6a-5b build, and the `e10_witness`
required-features gate keeps the green from meaning nothing. Image:
`flac-spike-6a5b.gba` sha `b6e35beb1e04028c…`, `rustc 1.100.0-nightly
(a69a63265 2026-09-03)`; `make item6a5b-test` green (21 + 4 + 3 + 10 host tests);
both kernels pass `scripts/check_arm_kernel_objdump.sh`; asset verification and the
decode proof pass on all three arms (FNV `0x54C7B356621B6E15`), the 6a-5 calibration
is stable (`overhead=33 stable=true ime_inside=0`), and every per-frame
sample-equality witness holds. The PR 3 budget verdict is RED (0/2 in budget) — it
says no leg fits 32 kHz; it does not void the pair.

**The reading.**

| arm (one image, one pass, 157 frames) | FIXED sum | FIXED c/f | % budget | LPC sum | LPC c/f |
|---|---:|---:|---:|---:|---:|
| `ctrl` | 275,126,433 | 1,752,398 | 167.09% | 1,031,931,935 | 6,572,815 |
| `asm` (landed, guarded) | 183,304,113 | 1,167,542 | 111.33% | 940,645,218 | 5,991,371 |
| `vend` (E10's, unguarded) | 167,872,486 | **1,069,252** | **101.95%** | 925,235,241 | 5,893,218 |

- `asm − ctrl` = **−91,822,320 / −91,286,717** — *cycle-identical* to the 6a-5
  record's, which is the control arm's own reproduction of it.
- `asm − vend` = **+15,431,627 / +15,409,977 cycles** = **+98,291 / +98,153
  c/frame = +24.11 cycles per Rice codeword = +48.22 / +48.16 c/sample**
  (320,000 sample-frames, 640,000 codewords per clip). `vend_faster = 157,
  asm_faster = 0, equal = 0` on both clips; max +98,775 @147. Run 2: +15,432,447 /
  +15,412,012 — a difference of +5.2 / +13.0 c/frame.
- The reading-rule exception did its job: 6a-5 drew +97,929 c/frame across two
  records and 6a-5b measures +98,291 in one — **0.4% apart**. The reason they agree
  is now on the record as a fact of its own: the *arm* absolutes are link-stable
  (landed asm 183,288,727 / 183,231,371 / 183,304,113 across three links, 0.04%
  apart; E10's vendored arm 167,911,108 vs this image's 167,872,486, −0.02%), while
  `ctrl` moves 280,848,260 → 275,111,047 → 249,051,451 across those same links.
  The re-opening's premise — that the overhang was the difference between two
  *images* — is refuted. What was re-opened was a comparison of two realizations,
  and it survives being closed.
- A determinism control this rung did not plan for: **three images with different
  shas print cycle-identical arm numbers** (the record's, a pre-change rebuild, a
  current-tree rebuild: 342 `***` lines identical), while *adding one more timed leg*
  to the same source moved every absolute leg by exactly **+98 c/frame** — all 314
  frame-legs of both images, min and max alike — and
  left every paired delta cycle-identical — and the move is a *constant*: +15,386
  cycles on all four legs of both clips, mins and maxes included, and the offset is a
  property of the image rather than of any leg — which is why pairing cancels it
  exactly, and why two images' absolute arms can differ by a fixed per-frame amount
  with no link mechanism behind it (that, not a link effect, is the whole 98 c/frame
  between this record's `asm` sum and 6a-5's; *why* an extra leg shifts the others by
  a fixed 98 cycles/frame is not explained here, only bounded). The
  9–13 cyc/frame band is a band on *pairs*; absolutes move ~8× past it from a change
  this small.

**The machine code, word for word.** Normalising both disassemblies in this image
(strip addresses, symbol-relative branch targets, immediates) leaves an exhaustive
diff: 171 words vs 160. The two kernels are otherwise the *same instruction stream*,
line for line. The differences are exactly:

| where | landed adds | executed per |
|---|---|---|
| prologue | `sub sp,#12` (was `#8`), `mvn ip,#0` + `lsr ip,ip,sl` + `str ip,[sp,#8]` — the hoisted `MAX >> p` | 3, once per call (2 calls/frame on FIXED; the call count is identical in both legs on any clip) |
| `.Lrem_have` | `ldr ip,[sp,#8]` + `cmp r2,ip` + `bhi .Lvalerr` (G1), **`and ip,r3,#0xFF` twice**, `adds r3,ip,#0x80000000` + `beq .Lvalerr` (G2) | **6 per codeword** |
| cold tails | `mov r2,#2` + `b` into the epilogue | 2, never executed on this corpus |
| epilogue | `add sp,#12` (was `#8`) | 0 (same count) |

640,000 codewords × 6 + 314 partitions × 3 = **3,840,942 executed delta
instructions per clip**, against a measured gap of 15,431,627 cycles: **4.02 cycles
per inserted instruction** would be needed for the kernel body to be the whole of
it. At one cycle each it is 24.9% of the gap; at two, 49.8%.

The inserted `and` is a **defect, not a guard**: `crates/flac-lite-gba/asm/rice_partition.s`
lines 256 and 258 are the same line — the [6a-4 G1] insertion moved the `and` above
the guard and left the original behind. Emitted adjacently at `0x8000288/0x800028c`,
`r3` untouched between them, neither sets flags: the first is dead, once per
codeword, 640,000 dead executions per clip. It is **not** removed by this record —
the kernel whose bytes this record anchors must keep its bytes (those bytes are
identical, `38760f65855c`, in a base-commit worktree build, both control images and
this image; an earlier "the landed kernels differ" note was an extraction-offset
error and is withdrawn).

**What this image prices at zero, by construction.** The same number of kernel
calls per frame in both legs — 2 on FIXED, where partition order is 0; more on LPC,
where it varies frame to frame, but equal in both legs — each through a linker thunk; tables pinned in `.iwram` by both (landed
`0x0300032c/34c/44c`, vendored `0x0300054c/56c/66c`); the state struct's frame in
IWRAM for both (the boot code loads `0x03007ffc`); `stereo::decorrelate` and
`PredictorState` shared by both. So the bridge, the pin, the 12-byte frame and the
third status word — the re-opening's candidates (a) and (c) — contribute exactly
nothing to this differential. What the vend leg *does* carry that the landed leg
does not is its own `AccumulatorReader` and its own frame/subframe/prediction code,
so "realization" here is not only the asm body — but those are per-frame and
per-call, and the measured cost is **per codeword**: +48.22 vs +48.16 c/sample on
two clips whose per-frame Rust work differs 3.75×. The per-codeword loop lives in
the kernel and nowhere else (`str ip,[r8],#4` + `cmp r8,r9` + `blt .Lloop` is the
codeword loop in both bodies). That is the shape of the answer: **the regression is
inside the codeword loop**, and it is either the cost of those six instructions or
what their twenty-four bytes did to the loop's fetch. The gap between an expected
~7 cycles per codeword (one IWRAM load, three ALU ops, two not-taken conditional
branches, one dead ALU op, plus the load-use stall) and the measured 24.11 is the
open problem, and it is the same species as Q3 — a code-size/layout effect this
project has measured before and never explained. **[Resolved by 6a-5c, below: the
"expected ~7 cycles" is wrong because an instruction in this loop costs 4, not 1.
6 × 4 = 24, which is the measurement. There is no anomaly to explain — only a price
this record had not yet measured.]**

**6a-5c, pre-registered before it ran.** Read `asm − vend` again per leg: L1 one
dead instruction removed (a shippable fix), L2 G1/G2 removed (diagnostic), L3 G1
re-spelled (still open). The fork said: ~1 cycle/codeword from L1 means instruction
cost, ≥3 means hot-loop layout.

### 6a-5c — the reading: six instructions, four cycles each, and the fork's premise was wrong (2026-11-09)

Three legs were built and run, two of them twice; every run is `***`-identical to
its twin, and the decoded samples are bit-exact in all of them (per-arm decode
proof, FNV `0x54C7B356621B6E15`, 157/157 frames, both clips).

| leg | `asm` body | `asm − vend`, FIXED | `asm − vend`, LPC | that leg's saving |
|---|---|---|---|---|
| 6a-5b (anchored) | 684 B / 171 w | 15,431,627 | 15,409,977 | — |
| **L1**: dead `and` deleted | 680 B / 170 w | **12,871,627** | **12,854,993** | **2,560,000 / 2,554,984 = 4.0000 / 3.9922 c/codeword** |
| **L2**: G1+G2's 5 instructions also deleted *(diagnostic)* | 660 B / 165 w | **71,627** | **80,073** | **12,800,000 / 12,774,920 = 5 × 4.000 c/codeword** |

`12,800,000 = 5 × 4 × 640,000` and `2,560,000 = 4 × 640,000` exactly. So the
unit is **4.0000 cycles per executed instruction**, and the entire 6a-5b gap is
`6 × 4 × 4096 = 98,304 c/frame` against 98,291 measured. After all six are gone,
**71,627 cycles of 15,431,627 remain — 0.46%** — at 456 / 510 c/frame, per call
rather than per codeword, of the same order as the static bound's ~305 c/frame for
the seam bridge. The account closes.

**The fork was mis-specified, and saying how is the useful part.** It offered two
channels and a 1-cycle baseline: "instruction cost" meant "each instruction costs
about a cycle, so 6 added instructions are ~6 cycles"; "layout" meant "the same
instructions cost far more because of where they sit". The measurement is neither.
Instruction count is exactly the currency — L1 and L2 remove *only* instructions,
change nothing else, and move the pair by exactly 4 cycles per execution — but the
price of an instruction here is **4 cycles, not 1**. That price is the ARM7TDMI's
fetch cost for a 32-bit ARM instruction in cartridge ROM on a 16-bit bus at
WS0 = 3,1: not a surprise to the hardware, and not something any amount of
re-spelling changes. So "instruction cost" and "fetch cost" were never two
hypotheses in this kernel. 6a-5b's own arithmetic had already said so — it needed
"4.02 cycles per inserted instruction" to explain the gap — and the item read that
number as an anomaly instead of as the answer.

**Layout is excluded, not merely unproven.** The saving is proportional to codewords
*executed*: on every one of the 156 frames of 2048 samples it is exactly 16,384
cycles, and on the single 512-sample frame of each clip exactly 4,096 — a quarter,
tracking the sample count rather than the change of position. And the legs that
were not touched did not move: the vend leg prints 167,872,486 / 925,235,241 in all
three images even though its own body shifted 4 bytes earlier in L1's. A layout
effect cannot be 4.000 × executions, and 4 bytes of shift cannot leave the shifted
leg's absolute identical to the cycle.

**The budget, with the lever now named.** Removing the dead instruction alone is
worth **1.56 points** of the 32,768 Hz frame budget (98,291 → 81,985 c/frame of
overhang, 1,167,542 → 1,151,236 c/frame, 111.33% → **109.77%**). Deleting all six
would be worth **9.33 points** (102.00% mean) — and that is an upper bound no
shippable kernel can reach, because G1/G2 must exist in *some* form; L2's decode
proof passing only says they never fire on this corpus, and the device suite's
rejection arms are exactly what a shipped kernel has to keep passing. What the
numbers do say is that **each guard instruction is worth 4 cycles per codeword =
16,384 c/frame = 1.56 points**, so the shippable question is now a plain instruction
budget: G1 currently spends 3 (`ldr r12,[sp,#8]`; `cmp`; `bhi`) where the hoisted
bound sits in *memory* rather than a register, and G2 spends 2. Recovering the
41% of the overhang that 32 kHz needs is **two instructions**.

**And the instruction price is why 6a-5d is now the largest number on the table.**
If a fetched instruction costs 4 cycles and a loop's codeword path is some thirty
instructions, then the kernel's placement is not a micro-optimization: the same
instruction stream in IWRAM (0 wait states, 32-bit) changes the price of *every*
instruction in the loop, including the ones nobody can delete. E6 ruled this out —
"pinning Rust to IWRAM was slower" — and its own caveats are why that result does
not carry: its arm forced `inline(always)`/`inline(never)` and took linker veneers,
and it measured a path where each instruction carries many cycles of work, so a 4x
fetch price was invisible in it. A tight ARM loop of one-cycle instructions is
precisely the case E6 did not isolate, and 6a-5c has now measured the price it was
looking for. **6a-5d is not a follow-up; it is the next question the data asks.**

**What this does not say.** L2 is diagnostic: it cannot be shipped, and its number
must never be quoted as an achievable rate. It does not prove that *any* cheapest
guard spelling exists — L3 is still open, and its constraint is the device suite's
rejection arms, not this corpus. It does not say the vend realization is optimal:
the vend leg pays the same 4-cycle fetch price for its own ~30 instructions, which
is the strongest hint in these records that *both* realizations are paying ROM
fetch and that the item's real headroom is placement, not algorithm. And it does
not move 6a-6: the width leg is a separate mechanism on a separate site.

**The budget consequence, and it changes the item's arithmetic.** The overhang is
**9.37 budget points** on the frames×budget convention (98,291 / 1,048,750) against
the **3.84 points** the projected landed stack is over (103.84% mean: 1,167,542 −
E9's −78,541 = 1,089,001 c/frame vs a 1,048,750 budget; 6a-5's version of that
figure, 103.83%, differs by exactly the +98 c/frame the extra timed leg adds, so
the two are the same measurement of the same arm). Recovering **41%** of the
measured overhang fits 32,768 Hz FIXED with no other lever; the vend leg plus E9's
i32 marginal is 1,069,252 − 78,541 = 990,711 c/frame = **94.47% mean / 95.0%
worst**, which is the archive's in-budget row re-derived from a landed-tree
measurement of the arm instead of a probe's. So Q12 has a third candidate answer
beside 6a-6's width leg and 6b's rate work: the 48 c/sample may be priced back
inside the kernel. It is not yet claimed — 6a-5c prices it, and nothing here may be
used as a fix until a leg measures it.

**What this does not say.** It does not carry anything into the shipped default: no
shipped count, section or instruction stream moved (this is the same rule 6a-5 wrote
for itself, and the product image is untouched). It does not settle which of the two
channels the cost is in — that is 6a-5c's job and the fork above is written before
the reading. It does not license refactoring the seam: the seam is now *measured*
free in this image. It does not re-open the fork on the kernel (−584,856 c/frame,
157/157 frames faster, carried). And it does not make the 94.47% row usable: it is
still a projection over E9's i32 marginal, which 6a-6 must measure on this tree.

## Item 6b — the container (where frames come from)

6b changes *where frames come from*. Its measurement is a codegen reading (the
deferred `scan` **+3,120 B** is charged and attributed there) plus bit-exactness
through a real manifest.

| PR | Scope | Witness | Failure it isolates |
|---|---|---|---|
| **6b-1** | `stream` → frame source → `decoder::Decoder` rewired onto `scan::Frames`. Inherits `Frames` / `StreamBounds` / `ConsumedFrame` / `resync` — **not** a third frame loop. Turns `scan` on in a ROM-shaped build for the first time. | Codegen: the +3,120 B charged and attributed symbol by symbol; E anchors re-pinned; bit-exactness through the walk | that the walk's measured cost was argued-as-free, and the facade pays it |
| **6b-2** | `scripts/pack_flac.sh` real implementation (GAFP writer), `format::Manifest` parse, `EncodeProfile::validate` at pack time, `SEEKTABLE` interpretation + `Frames::anchored(n)`. | Bit-exact decode through a real manifest; profile validation convicted at pack time; the seek witness `frame_run_loop.rs` already holds the reverse-order half of | that the pack-time profile guard the whole design leans on is decorative |
| **6b-3** | The rate-play path on a real manifest; the spike's generated offset table retires. | The `rate-*` flow as it has been run before — assets staged, ROM built, measured — now through the manifest | that the throwaway harness was load-bearing in ways nobody wrote down |

**Out of 6b, explicitly:** mixer/DMA and `agb` integration (Phase 2 step 9),
24-bit decode, `unsafe` of any kind, and any GBA real-time claim.

## The width decision (what it is and is not safe for)

**i32 becomes the core's default arithmetic width, feature-gated, with i64
available to integrators.** This reverses the split proposal's "no width fork"
half; the "armed by a bound" half stands, and that is the half carrying the risk.

- **Unconditionally safe, and it is most of it.** At the *ring* sites —
  `integrate_fixed`'s cascade and seeds, `pad_block`, the side decorrelation arms —
  `+ − × <<` commute with i32 truncation, and the core already truncates at store.
  Wrapping i32 with the same store is bit-identical for any input, legal or
  corrupt.
- **Conditional, and exactly two sites.** `>>` does not commute: the LPC
  `(acc >> shift)` and mid/side `(m_ext ± side) >> 1` are exact **iff the true
  value fits i32**. E4's bound (`|acc| ≤ order × |coeff| × |sample|`, concretely
  `4 × 2^(prec−1) × 2^16`) holds to precision 13 and **fails at 15**; the mid/side
  margin is sample-width-dependent, so E4's `3 × 2^16` figure does not generalize
  on its own.
- **The shape.** Default i32, armed **per frame at parse time** by an O(order) sum
  of magnitudes over data the header just parsed, before the hot loop runs. If the
  bound fails, the frame decodes through the i64 leg — an off-profile stream's
  escape hatch is the other width, not an error. Which leg ran is reported per
  frame.
- **Feature semantics.** `i32-integrator`, on by default; off means exactly today's
  i64 code, guardless — the audited leg and the reference witness. This inverts
  the other three features (which defer a cost); this one chooses a default
  semantics, which is why it may be on.
- **Two consequences a reviewer must see.** (1) `make flac-test` stops witnessing
  the i64 leg, so the PR carries a companion configuration (`flac-test-wide`-shaped,
  feature off, i64 driven end to end) and a width differential suite
  (`tests/width_two_mechanism_differential.rs`). Counts going *up* is expected;
  counts staying the same while the meaning moves is the failure mode. (2) A
  both-legs build carries both legs' arithmetic; item 5 measured that a second
  implementor's *presence* re-decides inlining (+2,984 B with no symbol of the
  added mechanism), and 6a-6's gate prices it in the configuration a ROM links.
- **64-bit hosts:** `i64` is native there, so the narrower leg is expected to buy
  nothing and might cost. `flac-portable-check` is a compile gate, so nothing here
  measures that. The feature's claim is API flexibility, not host performance.

## Invariants this series must not trade away

- **`RiceKernel` stays unsealed**, and 6a is where it gets its external
  implementor.
- **`bits::rice_quotient` and `LEADING_ONES` are not deleted.** Two live mechanisms
  are what the differentials consume, and `LEADING_ONES` is the `nm` anchor cited
  by E2b, E10, the `iwram-pin` witness and item 5.
- **The spike's `driver::decode_one` is not moved or refactored.** It is the
  measurement instrument; cross-session anchors hold only because that call shape
  is unchanged.
- **No cycles quoted across records.** Same-image pairs only; every arm names its
  own control in its own session.
- **Profile facts stay the core's, never a kernel's.** The overlay kernels'
  loud-rejection of escape partitions and `p > 14` is a probe fact; the core
  decodes them (and, under the width decision, decodes off-profile streams through
  the wide leg).
- **`scan` stays off until 6b-1**, and no 6a ROM links the walk.
- **The core makes no real-time claim**; the overlay never claims correctness
  independent of the core's witnesses.
- **The default Rice kernel and the shipped decode path do not change** in any PR
  that is not 6a-6 or 6a-7. 6a-5b is instrument-only (one more arm in the 6a-5
  image); 6a-5c may move only the overlay's own arm configuration and re-runs the
  overlay's witnesses — and note what that constrains *inside* 6a-5c: an L2 that
  deletes the guards makes the kernel-check ROM's §9.2.7.3 rejection arms fail by
  construction, which is why a won L2 licenses a *re-spelled* guard (L3), never a
  shipped kernel without them.

## Deliberate scope cuts

No `Decoder`/`format::Manifest`/`scan` rewiring in 6a. No 24-bit decode. No
mixer/DMA or `agb` integration in either item. No `dyn` dispatch anywhere. No new
`unsafe` in the core. No sub-stage probe unless 6a-5 lands near the bar — 6a-5b is
not that probe (it compares two realizations in one link; it does not price one
kernel's sub-stages), so **6a-8 stays untriggered**. No GBA
hardware claim — mGBA on this host, and E5's arbitration owns that line.

## Baselines to hold

Unchanged since item 5 PR 4, and re-stated byte-identically by 6a-1/6a-2/6a-3:

| Gate | Count |
|---|---|
| `flac-test` | 178 + 1 |
| `flac-test-crc` | 181 + 1 |
| `flac-test-scan` | 198 + 1 |
| `flac-test-rice` | 194 + 1 |
| `crc-check,scan` | 201 + 1 |
| `crc-check,scan,rice-hybrid` | 217 + 1 |
| `flac-gates` | **1,169 passed + 6 ignored** |
| `spike-test` | 15 + 10 |
| `kernel-test` | 20 |
| `flac-portable-check`, `make check` | green |

Images in the configuration a ROM links: `.text` **46,008 B** (`f0f88c33d150…`) ·
`.rodata` 1,829,760 B · `.iwram` 812 B · `.ewram` 368 B · `.entrypoint` 380 B;
`nm` anchors `LEADING_ONES @ 0x0300022c`, `rice_quotient` 336 B,
`decode_residual` 596 B. Hand-invoked gates outside `flac-gates`:
`overlay-check` (thumbv4t compile witness), `overlay-test` / `overlay-test-twin`,
and 6a-4's own configuration: `overlay-check-arm`, `overlay-test-arm` (15 host
arms), `overlay-objdump`, `overlay-kernel-rom`,
`overlay-kernel-test` (8 device arms, ~16 s real). 6a-5's instrument images (same crate, arm features on; **not** the shipped
configuration and not comparable to the 46,008 B figure): product shape
(`item6a5-asm`) `.text` 63,124 B · `.iwram` 1,356 B · `.rodata` 1,830,876 B;
three-arm (`item6a5-arms`) `.text` 72,428 B · `.iwram` 1,868 B · `.rodata`
1,831,228 B. The default instrument after the rung: `.text` 45,996 B
`f63ff65bedea…` · `.iwram` 812 B, with the cycle control unchanged to the cycle.
6a-4's instrument image
(`overlay-kernel-check.gba`, `rice-arm,iwram-pin`): `.text` 67,772 B
`8d8c7fe85a14…` · `.iwram` **1,356 B** `a568ddbb5c5f…` · `.rodata` 145,988 B ·
`.ewram` 368 B · `.entrypoint` 380 B; `arm_rice_partition` 684 B at
`0x0800017c`; tables `CTZ8 @ 0x0300032c`, `REV8 @ 0x0300042c`,
`BRUIJN_INDEX @ 0x0300052c` (runtime-asserted by the ROM, never hard-coded by a
kernel-check). This image is **not** the shipped path and its numbers are not
comparable to the 46,008 B figure — different crate, different linked set, no
cycle meaning.

**Per-PR rules:** 6a-1 / 6a-2 / 6a-5 / 6a-5b / 6a-7 / 6a-8 change no shipped count
and no shipped instruction stream. 6a-3 and 6a-4 may move counts only in their own new
configurations (both verified). 6a-5 is the rung that needed the rule read
precisely: the **product** image is byte-identical, the **instrument's** cycle
readings reproduce the record to the cycle, and the instrument's own `.text` moved
by −12 B of harness code plus symbol renames (attributed in its record, and the
default build's `decode_one` body is a macro expansion at two sites so that a
`pub fn decode_one_with<K>` could not move +2,868 B — measured, then rejected).
6a-6 *is* allowed to move the default configuration, and must name its deltas
symbol by symbol.

## The landing record every PR in this item carries

Four paragraphs, every PR, no exceptions:

1. A **pre-change control build** from `HEAD` before any source edit, with a
   `cargo clean` determinism control beside it.
2. A **section table** plus `nm --print-size --size-sort`, every delta attributed
   **symbol by symbol**, and the three `nm` anchors re-stated or their absence
   explained. `.rodata` content is not a codegen signal; `.text` and `nm` are.
3. **Gate counts**, naming a new configuration as a new gate rather than a changed
   one, and stating `make test-rom` as run or not-run and why.
4. A **mutation witness** for every new correctness suite, including the honest
   green rows — a behaviour-neutral re-routing must not read as a gate failure
   (item 5 PR 3's mutation table is the template).

## Ordering rules that outlive any single ladder

- **Cumulative gains.** No lever substitutes for another: the rate track grows the
  denominator while decoder levers shrink the numerator, and utilisation composes
  roughly multiplicatively — precisely, as E9's measured absolute additivity.
  Every measured percent counts toward the product, including the small ones.
- **The rate track is orthogonal, not an alternative.** Its Part 1 tooling landed
  (rate ROM, rate-play ROM). Its step-4 multi-clip ROM and step-5 cross-rate sweep
  are still open; the sweep is re-run on the improved decoder, not on the
  baseline. The "which rates become feasible" guidelines come from that sweep,
  never from prose.
- **E1 is excluded by design** from any stack: its relief is the position-only
  reader's re-fetch premium, which the kernel's windowed refill already absorbed;
  stacking it would double-count.
- **Closed, do not re-litigate:** E3 inert (+20 cyc/frame), E6 code pinning slower,
  E7 flag inert, E2a reader not a collapse on its own, the portable twin declined
  as the default (+42.6 c/sample), `opt-level = "z"` in `flac-lite` does not govern
  the measured ROM (workspace-root profile applies).
- **E5 is the only open measurement that can turn a real-time claim into a claim.**
  It runs once per candidate rate × path combination, on hardware.
