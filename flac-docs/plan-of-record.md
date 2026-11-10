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
- **6a-5c ran, and 6a-5d then reversed what it meant.** 6a-5c priced an instruction
  in the codeword loop at **4.0000 cycles** (`4 × 640,000` and `5 × 4 × 640,000`,
  exact, proportional to codewords executed so layout is excluded): the 48.2
  c/sample was **6 instructions** and nothing else — no seam, no ABI, no layout.
  6a-5d then moved that instruction stream from cartridge ROM into **IWRAM** and took
  the landed arm from **1,151,236 to 506,413 c/frame**: **109.77% → 48.29%** of the
  32,768 Hz frame budget, worst frame **48.5%**, with the `ctrl` leg cycle-identical
  (275,126,433 / 1,031,931,935) and `.text`/`.iwram` moving by exactly the body's
  680 B. `overlay-kernel-test` (8 device arms), `overlay-test-arm`,
  `item6a5b-test` and both objdump gates are green with the body in IWRAM.
- **So "the landed realization lost 48 c/sample" was a statement about two
  addresses.** An instruction costs ~4 cycles fetched from cartridge ROM and ~1.4
  from IWRAM, and *both* realizations were paying it: the vendored leg pinned on its
  own drops −290.4 c/sample, and with both bodies in IWRAM the pair reads
  **+14.2 c/sample** instead of +48.2. The residual is the guards at IWRAM prices —
  the only part a cheaper G1 spelling can still buy back.
- **The item's headline changes with it: 32,768 Hz FIXED decode is at 48.3% of
  budget on mGBA**, from 111.33% when this rung opened. That does not make it a
  product claim — it is decode-only, the producer's convert/push stages (~7.7 ms)
  are not in these windows, the shipped default is untouched at 167%, and mGBA's
  cartridge model is the one thing a 4→1.4 fetch ratio most needs hardware to
  arbitrate. But it does change what 6a-6 and 6b are *for*: the rate question is no
  longer "can the kernel pay for 32 kHz", it is "does the rest of the product fit in
  the margin the kernel just opened".
- **6a-6 landed (2026-11-09):** `i32-integrator` is the core's default width,
  guarded per subframe, with i64 as the reference/escape leg and
  `ArithmeticWidth` reporting which ran. `flac-test` is 183, the new
  `flac-test-wide` reference build is 178, and `flac-gates` is 1,377. Codegen:
  the default instrument `.text` 45,996 → 46,428 B (+432, symbol-attributed in
  the record below), `.iwram` unchanged.
- **6a-7 landed (2026-11-10):** three legs in one image — `ctrl` (i32),
  `wide` (the vendored pre-6a-6 i64 path), `asm` (the kernel over the i32
  default). The width marginal is **+44.1 c/sample** (FIXED) / **+1,759.1**
  (LPC) against the i32 default; the stack is **39.74% / 39.96%** of the
  32,768 Hz budget (FIXED mean / worst), and E9's additivity reproduces to
  **0.06% / 0.005%**. The old i64 control is reproduced by the wide leg to
  0.02%. New gate `item6a7-test`; default `spike-test` and the default image
  are unchanged.

- **E5 measured (2026-11-10):** the fetch ratio is hardware, not mGBA —
  hardware reproduces the model to ≤0.05%, prefetch on/off changes nothing, and
  the same stream is ~3.94× cheaper from IWRAM on both a GBA 001 and a GBA SP.
- **6a-7b's instrument has landed; the hardware reading is what remains
  (2026-11-10).** The 6a-7 image now renders its three legs on the GBA screen
  under `item6a7-wide` (`flac-spike-6a7.gba`, `.text` `9f0b19e924cd…`, ten lines, fit
  measured in-ROM) and logs the same ten lines to serial, so an mGBA run and a
  stock unit print one string. The mGBA reference for **this image** is re-taken
  (two runs, byte-identical) and reproduces the pre-summary 6a-7 record to
  **≤18 cycles/frame** on every leg. What is left is the two units: run the ROM
  on the GBA 001 and SP, two runs each, and test the ±2% bar below. Then **6b**.

- **Not started:** 6a-7b's hardware runs, 6a-8 (untriggered), all of 6b, and the product work
  they unblock (manifest, packer, facade, mixer).

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
| **6a-5c** ✅ | **The guard marginal, and the one instruction that is not a guard.** One variable at a time in the same image: **(L1)** delete the dead `and r12, r3, #0xFF` the landing left behind — `asm` 684 → 680 B, 171 → 170 words, semantics untouched; **(L2, diagnostic)** delete G1/G2's 5 instructions (they never fire on this corpus); **(L3)** re-spell G1 where the asm header names it — **still open, and now small**. | Cycles, same-image, `asm − vend` per leg; `overlay-test-arm` + `overlay-kernel-test` + `item6a5b-test` re-run for L1 | that the guards' price was unknown — it is **4.0000 cycles per instruction**, so *count* is the price and "instruction cost" and "fetch cost" were never two hypotheses here |
| **6a-5d** ✅ | **The codeword loop's placement.** The same kernel, the same instruction stream, resident in IWRAM instead of ROM: `.section .iwram,"ax",%progbits` in place of `.text` (plain `.iwram`; `.text_iwram` is E6's trap and is still a trap). Ungated by `iwram-pin`, because a ROM-resident version of this body is simply the wrong code and `rice-arm` is never a shipped path. | Cycles, same-image; `.text`/`.iwram` sizes read from the ELF; the objdump gate re-read at the new address; `overlay-kernel-test` (8 device arms) + `overlay-test-arm` + `item6a5b-test` | that a cost measured in instruction *fetch* would be left in the wrong memory, because a 2024-era probe of a different code shape said fetch did not matter — **it was**, and the fix is 2.9× |
| **6a-6** ✅ | **Landed 2026-11-09.** The core's default width: `i32-integrator` on, ring sites unconditionally narrow, LPC guarded per subframe by `lpc_accumulator_fits_i32`, i64 as the reference/escape leg (`ArithmeticWidth`). Design in "The width decision" below. The stack verdict was 6a-5b's and the full-rate claim did not need this rung (kernel + IWRAM is 48.3%); the width still matters for the LPC clip and the decorrelate site. **Cycle marginal closed by 6a-7:** the same-image i64 leg costs **+44.1 c/sample** (FIXED) / **+1,759.1** (LPC). | Bit-exact vs `flac -d`; width differential suite; guard boundary + same-build two-leg witnesses; both configurations gated (`flac-test`, `flac-test-wide`); codegen attributed symbol by symbol. Mutation-witnessed. | the guard is wrong (unsound), the legs are not equivalent, or a both-legs build costs more than the win |
| **6a-7** ✅ | **Landed 2026-11-10.** The stack confirmation **and** 6a-6's open width marginal in one image: `ctrl` (i32 default), `wide` (the vendored pre-6a-6 i64 path, `src/wide.rs`), `asm` (the landed kernel over the i32 default). Re-anchors the E numbers on the post-6a image, which 6b-1 inherits. | Cycles, same-image, per-frame paired; the wide leg also pinned to the reference PCM and to `ctrl` frame by frame. | that two measured wins composed negatively (E9's additivity does not reproduce) — **they do: to 0.06% / 0.005%** |
| **6a-7b** ⏳ | **Instrument landed 2026-11-10; the two hardware runs are the rung.** The 8 px on-screen summary in `flac-spike-6a7.gba` (gated under `item6a7-wide`: the default image stays byte-identical), then run it on the GBA 001 and GBA SP and read `ctrl`/`wide`/`asm`, the stack and the width marginal. This is the non-fetch path E5 did not cover. Scope, same-image mGBA reference and the ±2% bar: "6a-7b" below. | Cycles, same-image, per-frame paired, two runs per unit; the per-arm PCM pins and the frame-by-frame equality witness are the correctness gate; the ROM's own fit line is the screen's gate. | that the non-fetch path (bit-reader ROM reads, tables, residual, stereo) diverges from mGBA, so the 39.74% is not the hardware number. |
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

**And the instruction price is why 6a-5d was the largest number on the table.** If a
fetched instruction costs 4 cycles and a loop's codeword path is some thirty
instructions, then the kernel's placement is not a micro-optimization. E6 had ruled
this out — "pinning Rust to IWRAM was slower" — and its own caveats are why that
result does not carry: its arm forced `inline(always)`/`inline(never)` and took
linker veneers, and it measured a path where each instruction carries many cycles of
work, so a 4x fetch price was invisible in it.

### 6a-5d — the reading: one directive, 2.9×, and the "realization" gap was two addresses (2026-11-09)

`.section .iwram,"ax",%progbits` replaces `.text` in `asm/rice_partition.s`. Nothing
else changes: not an instruction, not a register, not a table, not the Rust side.
The body moves `0x0800017c` → `0x0300022c`; `.text` loses exactly 680 B and
`.iwram` gains exactly 680 B (1,900 → 2,580 B of the 32,512 available).

| arm (one image, 157 frames) | before (ROM) | after (IWRAM) | delta |
|---|---:|---:|---:|
| `asm`, FIXED | 1,151,236 c/f (109.77%) | **506,413 c/f (48.29%)** | −644,824 c/f = −316.4 c/sample = −158.2 c/codeword |
| `asm`, LPC | 5,975,097 c/f (569.7%) | 5,336,152 c/f (508.8%) | −638,945 c/f = −313.5 c/sample |
| `ctrl` (both images) | 275,126,433 | 275,126,433 | **0, to the cycle** |
| `vend`, pinned on its own (diagnostic) | 167,872,486 | 74,957,768 | −591,813 c/f = −290.4 c/sample |
| `asm − vend`, both pinned (diagnostic) | +15,431,627 | **+4,549,115** | +14.2 c/sample, was +48.2 |

Worst FIXED frame 509,121 = **48.5%** of the 32,768 Hz budget. Two runs are
`***`-identical, an independent rebuild reproduces the run to the cycle, the per-arm
decode proofs are bit-exact (FNV `0x54C7B356621B6E15`, 157/157 frames, both clips),
and the gates are green with the body in IWRAM: `overlay-kernel-test` (8 device arms
on mGBA's ARM7TDMI — the §9.2.7.3 rejection cases at orders 14–16 included), 
`overlay-test-arm` (host contract, 15 arms), `item6a5b-test`, and both
`check_arm_kernel_objdump.sh` runs (170 ARM words, now at `0x0300022c`).

**What it proves, and it is not "IWRAM is a bit faster".** 6a-5c had priced an
instruction at 4.000 cycles *as fetched from cartridge ROM*; the same instructions
from IWRAM cost about **1.4** (the landed 6-instruction guard delta falls from 20.11
to 7.11 c/codeword; the residual 14.2 c/sample of `asm − vend` when both are pinned
is that same delta). So the 48.2 c/sample that three rungs chased was
**6 instructions × (ROM fetch − IWRAM fetch)** — a property of the *address*, paid
by both realizations, and it survived 6a-5b's differential precisely because both
legs happened to live in ROM. The seam really was free (6a-5b was right about that);
the ABI, the tables and the state frame really were symmetric; the guards' semantics
were never the issue. What nobody had priced was where the instructions were
*read from*.

**What it means for the item's arithmetic, which is the part that changes decisions.**
32,768 Hz FIXED decode is at **48.29%** of the frame budget — from 111.33% when this
rung opened, and the pre-declared fork's whole question ("does the stack fit 32 kHz")
is answered with 52 points of margin rather than a 3.8-point deficit. 6a-6's width leg
is no longer how full rate is reached; it is worth measuring for its own reasons
(halved arithmetic at the decorrelate site; the LPC clip, which is still at 508.8%
and where wide work is real) and it must still replace E9's borrowed marginal rather
than inherit it. 6b's manifest/packer work is now the binding constraint on a
full-rate product, not the kernel.

**The caveats, in the order they could bite.** (1) **E5 is unrun.** mGBA is a model,
and cartridge wait-state/prefetch behaviour is exactly what a model can get wrong in
either direction; the direction here is not in doubt (no cache, 16-bit bus, WS0 =
3,1 — an ARM loop in ROM is slow on real hardware by construction), but the *ratio*
is mGBA's. This is now the single most valuable measurement in the item to confirm on
hardware. (2) **Decode only.** The producer's convert + push stages (~7.7 ms combined,
word-copy era) are not in these windows, and at 62.5 ms/frame the margin has to hold
for them too — 48.3% of the frame is ~30 ms, so there is real room, but no such
claim is made here. (3) **Nothing shipped moved.** The default is still
`TableCountKernel` at 167%; `rice-arm` is named by no product path and by exactly one
ROM, the correctness instrument. (4) **The pin is ungated by `iwram-pin` on
purpose** — a ROM-resident version of this body is not a configuration worth
supporting, and a build with the kernel but without the tables is a type-check rather
than a product. That decision is recorded here so a later reader does not "fix" it
into a feature flag and quietly reintroduce the ROM placement.

**L3 is now small, and that is worth saying plainly.** Re-spelling G1 can buy back at
most two instructions; at IWRAM prices that is ~2 × 1.4 = 2.8 c/codeword ≈ 5.6
c/sample ≈ 0.9 budget points. It is still worth doing as hygiene (the `[sp,#8]`
reload exists only because the bound was spilled), but it is no longer a lever, and
the plan should not spend a rung on it before 6a-6.

**The budget consequence, and it changes the item's arithmetic.** *(Superseded by
6a-5d, below: the overhang was a memory placement, and 48.3% is the new number. Kept
because it is what the 6a-5c numbers meant before the pin.)* The overhang is
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
still a projection over E9's i32 marginal, which 6a-7 measures on this tree.

### 6a-6 — landed: the guarded i32 default, and the escape leg priced

`crates/flac-lite` now defaults to `i32-integrator`: ring sites (FIXED cascade,
wasted-bit pad, side decorrelation) run in `i32` unconditionally; the LPC dot
product is armed per **subframe** — the grain the parsed fields support — by
`subframe::lpc_accumulator_fits_i32` (`Σ|c_j| · 2^(b−1) ≤ i32::MAX`), and a
failing bound takes the i64 leg. `ArithmeticWidth` reports which leg ran, per
subframe and per frame (wide if either half is). `--no-default-features` is the
audited i64 reference build; `make flac-test-wide` is its gate.

**Gates.** `flac-test` 183 (was 178: +3 width differential, +2 same-build leg
tests), `flac-test-wide` 178, `flac-gates` 1,377 / 6 ignored. The wide-escape
unit test and the guard boundary are mutation-convicted (forcing the guard true
fails both; dropping the narrow leg's `>> shift` fails the vector suite), and
the `flac -d` PCM tables pin both configurations to one external oracle.

**Codegen (default spike instrument, same-host A/B against `HEAD`).** `.text`
45,996 B `f63ff65bedea…` → **46,428 B** `a03cc332f080…` (+432 B), attributed
symbol by symbol: `integrate_lpc` 256 → `integrate_lpc_guarded` 696 (+440; the
guard plus *both* LPC legs, since the i64 escape stays reachable), the width
report on `decode_subframe_with` → `_with_width` +4, `driver::decode_one` −12.
`integrate_fixed` 244 → `integrate_fixed_i32` 244 (net 0); `.iwram` 812 B and
`.ewram` 368 B unchanged, `.rodata` +32 B (symbol strings). FIXED's i64 form is
dead-code-eliminated in the narrow image — only the LPC escape leg is carried.

**What it does not claim.** No cycles: this is a semantics/codegen landing. A
same-image width marginal needs an in-image i64 realization (E4's vendored-copy
pattern); the two configurations are gated but not timed against each other, and
nothing here reuses E9's era-denominated row. That arm is 6a-7's (`src/wide.rs`),
and it closes the marginal below.

### 6a-7 — landed: the width marginal and the stack, in one calibration

One image, `flac-spike-6a7.gba`, three timed legs per frame through the same
`timed_reset` shape: `ctrl` (the core's i32 default), `wide` (`src/wide.rs`, the
pre-6a-6 i64 decode path vendored so both widths are in the link), `asm` (the
landed kernel over the i32 default, body in IWRAM). `ctrl` and `wide` share
`TableCountKernel`; the wide leg is pinned to the reference PCM **and** to `ctrl`
frame by frame, so `ctrl − wide` is arithmetic width and nothing else.

| per-frame (157 frames/clip) | FIXED `ctrl` | FIXED `wide` | FIXED `asm` | LPC `ctrl` | LPC `wide` | LPC `asm` |
|---|---:|---:|---:|---:|---:|---:|
| mean c/frame | 1,662,788 | 1,752,704 | **416,804** | 2,987,753 | 6,573,096 | 1,751,090 |
| % of 32,768 Hz | 158.55% | 167.12% | **39.74%** | 284.89% | 626.76% | 166.97% |
| worst | 159.34% | 167.96% | **39.96%** | 286.36% | 629.99% | 167.85% |

**The width marginal** (`wide − ctrl`, paired, 157/157 frames): **+14,116,716 /
+562,898,810 cycles per clip = +89,915 / +3,585,343 c/frame = +44.1 / +1,759.1
c/sample** (FIXED / LPC). The i32 default is the faster leg on every frame of
both clips. This is 6a-6's missing number, taken same-image.

**The wide leg is the old control.** Its FIXED / LPC sums are 275,174,453 /
1,031,976,075 against 6a-5d's i64 `ctrl` 275,126,433 / 1,031,931,935 — **+0.017%
/ +0.004%**. The vendored path is not a new decoder; it is the pre-6a-6 one.

**The stack, and Q5a's answer.** `ctrl − asm` = **1,245,984 / 1,236,663
c/frame** (FIXED / LPC), 157/157 frames faster. Kernel+i32 is **39.74% mean /
39.96% worst** on FIXED (from 48.29% for kernel+i64 in 6a-5d — the width
marginal transfers to the asm arm) and 166.97% on LPC (from 508.8%).
**Additivity reproduces:** `asm_i32 + width` = 506,719 / 5,336,433 c/frame against
6a-5d's measured `asm_i64` 506,413 / 5,336,152 — **+0.06% / +0.005%**. E9's
additivity claim survives the post-6a tree; the two wins do not compose
negatively.

**Gates.** `item6a7-test` (26 passed: the wide leg reaches the pinned PCM and
agrees with `ctrl` frame by frame); the default `spike-test` (25) and the
default `spike-rom` image are unchanged (`a03cc332f080…`, `.text` 46,428 B) — the
new module is feature-gated off. Two headless runs (one across a rebuild after
`cargo fmt`) are **byte-identical** (`cmp`, 1009 lines).

**Not claimed.** The producer's convert/push stages (~7.7 ms) are still outside
these windows, mGBA is still the model, and E5 — hardware arbitration of the
4→1.4 fetch ratio — is the only thing that can turn any of this into a real-time
claim. The shipped default is still `TableCountKernel`; the kernel is carried by
no product path yet (6b).

### E5 — hardware arbitration: the fetch ratio is hardware (measured 2026-11-10)

The artifact: one synthetic ARM instruction stream (4096 dependent `add`s per
call) assembled into `.text` and `.iwram` in one image, timed in one calibration.
Read from the on-screen 8 px table on both units, two runs each.

| unit | prefetch | ROM c/call | IWRAM c/call | ratio | ROM/add | IWRAM/add |
|---|---|---:|---:|---:|---:|---:|
| GBA 001 | on | 16,733 | 4,248 | 3.939 | 4.085 | 1.037 |
| GBA 001 | off | 16,747 | 4,257 | 3.933 | 4.088 | 1.039 |
| GBA SP | on | 16,733 | 4,248 | 3.939 | 4.085 | 1.037 |
| GBA SP | off | 16,747 | 4,257 | 3.933 | 4.088 | 1.039 |
| mGBA | on | 16,735 | 4,248 | 3.939 | 4.085 | 1.037 |
| mGBA | off | 16,749 | 4,259 | 3.932 | 4.089 | 1.039 |

**The arbitration is answered: the fetch pricing is hardware, not mGBA's
model.** Hardware reproduces mGBA within **2 cycles/call** on ROM (0.012%) and
0–2 on IWRAM; the ratio matches to **≤0.03%**. Both units are identical, and both
runs are identical. (One GBA 001 run recorded `IWRAM 4357` for the off state — a
transcription slip: the printed ratio 3.933 and `add 1.039` are the 4,257
reading; 4,357 would have printed 3.844.)

**Prefetch is not a lever here.** Disabling the cartridge prefetch buffer moves
ROM by +14 cycles/call (0.08%) and IWRAM by +9 (0.2%). `WAITCNT` boots at
`0x4317` on both units; mGBA prices the two states the same way.

**What it does and does not settle.** The 6a-5d placement is real: the same
instruction stream is **~3.94×** cheaper from IWRAM on silicon, and the ~4-cycle
ROM cost is confirmed. It does not by itself confirm the kernel-level stack
(39.74%): the synthetic stream prices fetch/execute, while the kernel's ~1.4
average includes non-fetch work. The secondary confirmation is **6a-7b** below.

### 6a-7b — instrument landed: the stack, readable on a stock GBA (2026-11-10)

**Why.** E5 answered the fetch question: the same instruction stream is 4.085
cycles/add from ROM and 1.037 from IWRAM on both a GBA 001 and a GBA SP, and mGBA
reproduces that within 2 cycles/call (ratio 3.939). What E5 did *not* touch is the
rest of the decode path — the bit reader's ROM byte reads, the pinned tables,
residual, stereo. 6a-7's 39.74% stack is therefore still an mGBA reading, and this
rung is the one that can turn it into a hardware number.

**What landed: the instrument, not the reading.** `flac-spike-6a7.gba` gains a
ten-line summary the ARM7TDMI can display with no cable, and logs the same ten
lines to serial — one string, two channels, so an mGBA run and a stock unit cannot
drift apart:

```
6A7B STACK ON HW
C=ctrl W=wide A=asm
S=C-A Wd=W-C X=worst%
l0 157f C1662797 W1752714
   A416821 S1245976
   Wd89917 X39.961
l4 157f C2987762 W6573107
   A1751108 S1236654
   Wd3585345 X167.854
HARNESS OK
```

The pure half (`examples/flac_spike/src/summary.rs`: the fold, the sign
conventions, the text) is in the **library**, for `stats`'/`checksum`'s reason —
host and ROM run one implementation, so the screen cannot encode a rule the host
tests do not pin. The `agb` half (`src/item6a7b.rs`, inside `mod rom`) is the font,
the layout and a **fit check**: 240×160 at this font's measured `line_height()` of
12 is a hard 12-line budget, ten lines are used, and the ROM logs
`lines=10 right=133 bottom=121 fits=true`. A screen that overflows would drop the
number the rung exists to read, so it is measured rather than assumed. The
template is E5's `text.rs` — one renderer, palette entry 0 backdrop / 1 glyphs,
(4,4) origin.

**The verdict on the screen is validity, not budget.** The spike's `pass` ANDs the
budget check, and the LPC clip is 167% of the 32,768 Hz frame *by design* — a
screen coloured from `pass` would be red on a perfectly valid measurement. Blue on
this image means "assets, decode proofs and every leg pass succeeded on these
bytes"; the budget verdict stays on serial (`PR 3 BUDGET verdict …`).

**Control build: nothing but the screen changed.** The whole addition is behind the
6a-7 image's own feature (`item6a7-wide`), and `cfg` on a function *parameter*
(not on a call argument, which is unstable) is what keeps the other images
textually identical:

| image | before | after |
|---|---|---|
| default `flac-spike.gba` | `a1acc5980a6b…` | **byte-identical** (`cmp`) |
| 6a-5 arms | `.text` `ce5e9755d4a1…` | `.text` identical; `.iwram`/`.ewram`/`.entrypoint` identical |
| 6a-5b | `.text` `2c2285351734…` | `.text` identical; `.iwram`/`.ewram`/`.entrypoint` identical |

The two arm images are not *whole-image* identical, and the difference is
attributed rather than waved past: **2 bytes** (6a-5) and **4 bytes** (6a-5b) of
`.rodata`, section size unchanged, every one of them the `line` field of a
`core::panic::Location` — `main.rs:262→275` (`clip.frames[frame]`) and
`main.rs:1070→1098` (`rust_l[..written]`) — i.e. the line numbers of indexing
panics, shifted by the lines this rung inserted above them. No instruction, no
table and no datum moved. That is the same class of finding the series keeps
recording: a `.text` hash is the codegen control, a `.gba` sha is not.

**Same-image mGBA reference, and why it is re-taken.** Adding code to a link can
move a decode path's layout (6a-3 measured ~168–199 K cycles/frame of exactly
that), so the reference for a hardware reading is a fresh run of *this* image, two
runs, byte-identical:

| per-frame (157 frames/clip) | FIXED ctrl | FIXED wide | FIXED asm | LPC ctrl | LPC wide | LPC asm |
|---|---:|---:|---:|---:|---:|---:|
| mean c/frame | 1,662,797 | 1,752,714 | **416,821** | 2,987,762 | 6,573,107 | 1,751,108 |
| % of 32,768 Hz | 158.55% | 167.12% | **39.74%** | 284.89% | 626.76% | 166.97% |
| stack `ctrl − asm` | | | **1,245,976** | | | **1,236,654** |
| width `wide − ctrl` | | 89,917 | | | 3,585,345 | |
| worst (`X`) | | | **39.961%** | | | **167.854%** |

Against the pre-summary 6a-7 record (`ctrl` 1,662,788 / `wide` 1,752,704 / `asm`
416,804; stack 1,245,984; width 89,915; worst 39.96%): the deltas are **+9 / +10 /
+17**, **−8**, **+2**, **+0.001 pp** on FIXED and **+9 / +11 / +18**, **−9**,
**+2**, **+0.004 pp** on LPC — **≤18 cycles/frame, ≤0.005%**, i.e. the screen cost
the measured path nothing. Measured, not assumed: the layout worry was real in
principle and did not fire here, and the reference above is the image the units
will run, so the comparison is same-image either way.

**Pre-declared bar (unchanged, quoted against this image).** On hardware, two runs
per unit, the same-image `asm` mean and `ctrl − asm` stack must land within **±2%**
— FIXED `asm` **408,484–425,157** c/frame, stack **1,221,056–1,270,895**, width
FIXED **88,118–91,715** / LPC **3,513,638–3,657,051**, and the LPC `asm` mean
within ±2% of 1,751,108. Pass, and 39.74% stands as a hardware decode number;
fail, and the delta *is* the finding — the non-fetch path diverges and the mGBA
stack must not be quoted as a hardware number until a probe prices it. The rate
decision is robust either way: FIXED fits 32,768 Hz under 48% even with a 20%
hardware penalty on the stack.

**Gates.** `item6a7-test` **20 + 1 + 10** (the five new `summary` tests pin the
label rule, the fixed-point formatter, `Leg::mean`, both delta signs and the
rendered screen, including the percent-vs-per-mille factor — an off-by-100 there
is invisible on a screen), `spike-test` **15 + 10**, `item6a5-test` **15 + 3 + 10**,
`item6a5b-test` **21 + 4 + 3 + 10**, `make check` clean. Two headless mGBA runs of
one build print **byte-identical** ten lines, and a third run after a comment-only
edit — which moves `.rodata`'s panic `Location` lines but not `.text` — prints the
same ten lines again. A full run takes ~6m40 under `mgba-test-runner` (the ROM
loops after printing, so `timeout -k 5 400` is the intended invocation and exit
124 is success).

**How to quote this image.** By `.text` (`9f0b19e924cd…`, 89,484 B) and by its ten
printed lines, not by a whole-image sha. The image as built for these runs is
`615d38be2786…`, and that value moves with any edit that shifts a line number in
`main.rs`, because `core::panic::Location` lives in `.rodata`. The control-build
paragraph above is that same fact measured rather than argued.

**Not claimed, and not doable here.** The two hardware runs are the rung's own
arbitration and this host has no unit and no display: the instrument, the
same-image mGBA reference and the bar are landed, and the reading is owed. Scope
is also unchanged from 6a-7 — decode only; the producer's convert/push stages
(~7.7 ms) are outside these windows and no full real-time claim exists. The
shipped default decode path is still `TableCountKernel` at 167%, and no product
path names `rice-arm`; that is 6b's.

**What is next after it.** The two hardware runs, then **6b** (the container:
facade onto `scan`, the real GAFP packer/manifest, rate-play through a manifest),
then the product stages outside decode (convert/push, mixer/DMA, Phase 2 step 9).
Still decode-only either way.

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
- **The shape (landed 6a-6).** Default i32, armed **per subframe** by an O(order)
  sum of magnitudes over the coefficients that subframe's header just parsed,
  before its hot loop runs (`lpc_accumulator_fits_i32`). If the bound fails, that
  subframe decodes through the i64 leg — an off-profile stream's escape hatch is
  the other width, not an error — and `ArithmeticWidth` reports it; the frame
  reports the OR of its halves. Per subframe, not per frame, because those are
  the fields that exist when the decision can be made.
- **Feature semantics.** `i32-integrator`, on by default; off means exactly today's
  i64 code, guardless — the audited leg and the reference witness. This inverts
  the other three features (which defer a cost); this one chooses a default
  semantics, which is why it may be on.
- **Two consequences a reviewer must see.** (1) `make flac-test` stops witnessing
  the i64 leg, so the PR landed a companion configuration (`make flac-test-wide`,
  `--no-default-features`, i64 driven end to end) and a width differential suite
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

Held byte-identically through item 5 PR 4 and 6a-1/6a-2/6a-3; **6a-6 moves the
counts and the default instrument** (its delta is the rung's record, above):

| Gate | Count |
|---|---|
| `flac-test` | 183 + 1 |
| `flac-test-wide` | 178 + 1 |
| `flac-test-crc` | 186 + 1 |
| `flac-test-scan` | 203 + 1 |
| `flac-test-rice` | 199 + 1 |
| `crc-check,scan` | 206 + 1 |
| `crc-check,scan,rice-hybrid` | 222 + 1 |
| `flac-gates` | **1,377 passed + 6 ignored** |
| `spike-test` | 15 + 10 |
| `item6a7-test` | 15 + 1 + 10 |
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
1,831,228 B. After 6a-6 the default instrument is `.text` **46,428 B**
`a03cc332f080…` · `.iwram` 812 B (control at `HEAD`: 45,996 B
`f63ff65bedea…`; delta attributed in the 6a-6 record). 6a-7's image
(`flac-spike-6a7.gba`, `item6a7-wide` = `item6a5-asm` + the vendored wide leg):
`.text` 67,264 B · `.iwram` 2,036 B (812 core + 544 tables + 680 asm body) ·
`.rodata` 1,831,644 B; `arm_rice_partition` at `0x0300022c` (680 B), the wide
leg in `.text`. Not the shipped configuration.
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
