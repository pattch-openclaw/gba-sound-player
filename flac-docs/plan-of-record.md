# Plan of record — the only live plan

Everything else in these docs describes what has been built or measured. This file
describes what is being built next, what bar each piece is judged against, and the
rules a landing may not bend. It replaces the item-5 plan (fully landed), the
"Phased plan" (fully landed or superseded), the perf-spike five-PR plan (PRs 1–3
landed; 4–5 open), the E10 plans (landed) and the rate-tooling proposal (landed,
with its step-4 multi-clip ROM and its cross-rate sweep still open) — all of which
now live only in git history and in the records they produced.

## Status at a glance (2026-11-08, branch `feat/flac-lite-gba-arm-kernel`)

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
- **Next rung:** **6a-5** — the realization reading, three arms in one image.
- **Not started:** 6a-5…6a-8, all of 6b, and the product work they unblock
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
| **6a-5** | **The realization reading — the item's number of record. No new code.** Three arms, one image: `ctrl`, 6a-3's Rust arm, 6a-4's asm arm; calibration subtracted, paired per frame, every delta byte attributed symbol by symbol. Carries the pre-declared fork below. | Cycles, same-image; codegen attribution. **The first ROM suite with a test in it is 6a-4's kernel check**; 6a-5 inherits a non-empty suite rather than "first to run `make test-rom`" (which already runs, collecting zero) | the win lives in a probe but not in the product path |
| **6a-6** | **The width change, in the core.** i32 as the default arithmetic width, guarded per frame, i64 kept and reachable. Design in "The width decision" below. | Bit-exact vs `flac -d`; a width differential over the corpus and every committed frame-run table; the guard's unit witnesses incl. refusal cases; **both** configurations gated; codegen attribution of a both-legs build; cycles as a same-image arm | the guard is wrong (unsound), the legs are not equivalent, or a both-legs build costs more than the win |
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

### 6a's pre-declared fork (stated before the reading, so it cannot be negotiated)

The asm arm **within the ~9–13 cyc/frame noise band or faster** → carry it: the
overlay kernel is named by the ROM's decode path through
`decode_rice_partition_with`, the placement being exactly one line. **Slower past
the bar** → not carried; the pre-named fallback is the vendored-loop overlay, and
the proposal's realization claim is corrected as a measured negative. No third
arm: "it was faster in the probe" is not a carry condition, and this project has
already seen that sentence cost samples once (E2b's −21.7% became +38% when it
landed).

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
  that is not 6a-6 or 6a-7.

## Deliberate scope cuts

No `Decoder`/`format::Manifest`/`scan` rewiring in 6a. No 24-bit decode. No
mixer/DMA or `agb` integration in either item. No `dyn` dispatch anywhere. No new
`unsafe` in the core. No sub-stage probe unless 6a-5 lands near the bar. No GBA
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
`overlay-kernel-test` (8 device arms, ~16 s real). 6a-4's instrument image
(`overlay-kernel-check.gba`, `rice-arm,iwram-pin`): `.text` 67,772 B
`8d8c7fe85a14…` · `.iwram` **1,356 B** `a568ddbb5c5f…` · `.rodata` 145,988 B ·
`.ewram` 368 B · `.entrypoint` 380 B; `arm_rice_partition` 684 B at
`0x0800017c`; tables `CTZ8 @ 0x0300032c`, `REV8 @ 0x0300042c`,
`BRUIJN_INDEX @ 0x0300052c` (runtime-asserted by the ROM, never hard-coded by a
kernel-check). This image is **not** the shipped path and its numbers are not
comparable to the 46,008 B figure — different crate, different linked set, no
cycle meaning.

**Per-PR rules:** 6a-1 / 6a-2 / 6a-5 / 6a-7 / 6a-8 change no count and no
instruction stream. 6a-3 and 6a-4 may move counts only in their own new
configurations. 6a-4 moved none outside its own (verified: the default image is
byte-identical, above). 6a-6 *is* allowed to move the default configuration, and
must name its deltas symbol by symbol.

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
