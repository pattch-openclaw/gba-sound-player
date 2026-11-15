# ULI2 plan of record

Status: **live since 2026-11-14; PR C open, U4 not met.** This is the working
plan for the ULI2 product codec. [`../../ULI2.md`](../../ULI2.md) remains the decision record
(why ULI2, what it is, the licensing and encoder risks); this file owns the
phases, gates, measurement ledger and the order of work.

**PR C-2 (as C-3a, with C-3c) landed 2026-11-14** — 523,267 → **488,685 worst /
429,339 mean (200% → 186%)**, a measured 35,000 where 90,000 was projected.
**PR C-3b landed the same day**: the operation counters and format-rule
diagnostics moved behind a `verify` feature, and the product build measures
**457,863 worst / 410,912 mean (175% / 157%)** — a further **30,822 / 18,427**,
which is the attribution's prediction met (it predicted 30,482 / ~20,000). The
ladder is now 2526% → 1133% → 698% → 204% → 186% → **175%**. Every twin and pin
is green in both builds; the ledger carries the arms. Next: coefficient buffers
to IWRAM with C-1. Parity (69.4%) still needs the symbol-path codegen gap the
attribution localized.

**PR C-1 landed (2026-11-14): hand-written FFT, three measured arms, and the
product frame moved.** A bit-exact hand-written ARM FFT was taken from
"same speed as Rust" to **15.8% faster than the Rust ARM kernel** in three
measured steps, and then wired into the product decoder:

- **Ladder** (cycles/FFT call, same image, all bit-exact): Rust ARM 85,065 →
  v1 spill 88,205 (*slower*) → **v2 traffic 73,363** → **v3 block transfers
  71,635**. v2 removed the spill/reload traffic (no t-value spilled, loop vars
  resident); v3 added `ldmia`/`stmia` block transfers — the structural
  difference the C reference had. The register-pressure hypothesis C-1 rested
  on is falsified as the *lever*; **memory traffic** is the lever, and it is
  now 18.8% down from v1.
- **Product frame**: `GbaDecoder`'s synthesis now routes through `fft_kernel`,
  so with `asm-fft` the full product frame drops **457,863 → 430,997 worst
  (175% → 164% of the period)** and **410,912 → 384,046 mean (157% → 147%)** —
  −5.9% / −6.5%, a real product-path gain, pins unchanged. Before this wiring
  the asm gains only reached `ArmKernels`, not the product decoder.

**PR C-1 follow-up landed (2026-11-15): the trivial butterfly (v4).** The last
structural difference from C's `TRIV_BFLY_P` is now taken: at j==0 of each
stage the twiddles are exactly (2^30,0)×3, so `cmul(y,1.0) == y>>2` and the
three twiddled outputs are plain sums shifted right 2 — no multiplies and no
twiddle loads. `fft_arm_v4.s` measures **69,034 cycles/call (−2,601, −3.6% vs
v3)**, and the product frame **425,795 / 378,845 (162% / 145%)**, a further
−5,202 worst. Ladder: 85,065 (Rust) → 88,205 (v1) → 73,363 (v2) → 71,635 (v3)
→ **69,034 (v4)**, 2.20× the C record.

**PR C-1 follow-up 3 (2026-11-15): the twiddle table's *location* was the real
twiddle-load cost.** The planned `ldr`->`ldmia` was worth ~1-2%; the table was
sitting in **cartridge ROM** (`.rodata` at `0x0800d640`) while C keeps its tables
in IWRAM, so every hot butterfly paid ROM wait states. A same-image A/B (same
kernel, only the table pointer differs) measures **69,034 → 61,852 cycles/call,
−10.4%**, and the new `iwram-tw` feature takes the product frame **425,795 →
411,431 worst (162% → 157%)**, mean 378,845 → 364,481 (145% → 139%). The FFT is
now **1.97×** the C record — under 2× for the first time. `iwram-tw` costs
2,016 B of `.iwram` (14,860 → 16,876) and is **opt-in** pending the U6 budget
(see the ledger). Lesson recorded: a large part of "why are we slower than C"
was table placement, not kernel codegen.

**Still slower than C+asm, and where to look next** (see the ledger's "PR C-1"
sections for the full analysis): v4 is ~1.97× the C record's ~31,400, and the
remaining known differences are (1) ~~the trivial-butterfly path~~ **done**,
(2) the remaining ~2× is the multiply core and the A..D/output traffic, and
(3) the C record is from a different corpus/image so the ratio is not
like-for-like — the honest sub-goal is to pull the FFT under 31,400 *in this
instrument*. There is no reason this Rust+asm stack should lose to C+asm; the
next research items are in that order in the ledger.

The target is **real time inside the hardware's cycle budget** (U4/U5), not a
percentage borrowed from C: the C/asm decoder's 69.4% is a proof that the work
fits this machine, so Rust+asm is expected to as well. See "The target is the
real-time budget" below. The measured ladder is **2526% → … → 175% → 148% → 138%
→ 105%**. The fast path is the default; **C-3d (2026-11-15)** removed the int16
output the product never used; and **C-3 (2026-11-15)** moved `HotState` to IWRAM
for **−87,040 cycles/frame**. The production frame is now **274,198 / 232,090
(105% / 89%)** — **1.51×** the C decode wall, with the mean already under the
period and only **12,054 cycles (4.6%)** left to reach real time. **Step 0 of the
critical path is now done (2026-11-15): the frame is attributed by difference in
the product image, and it reordered the remaining work** — on the worst frame,
entropy/dequant/M-S is 40% of the frame and 69% of the distance to C, while the
three Rust kernels are within 1.10–1.22× of hand-written asm. See "Critical path
to real time" below; every item there is a projection until it lands as its own
measured arm.

The Phase 1b measurement that forced this rewrite is recorded in
[`../../ULI2.md`](../../ULI2.md) → “Phase 1b — what the on-target measurement
found”. The instrument is [`../../examples/uli_bench`](../../examples/uli_bench)
(`make uli-bench`).

---

## Critical path to real time (2026-11-15)

**This is the first thing to read if the question is "what next".** Status lines
above are history; this section is the live order of work.

### Where we are

Production frame **274,198 worst / 232,090 mean = 105% / 89%** of the
262,144-cycle period. The mean already fits; the worst frame is **12,054 cycles
(4.6%) over**. Against the C record (181,928 = 69.4%) we are **1.51x**.

Two different targets, not one:

- **to work** — the decode wall must fit with room for the gate total (DMA, IRQ,
  convert) and this tree's UI: target **<= ~85%**, so cut **>= ~50,000** cycles.
- **to match C** — cut ~92,000. Not a requirement; C has 31% slack by design.

### Step 0 done (2026-11-15): the frame, attributed by difference

The per-kernel table this section used to carry chose the next kernel from
numbers taken **before C-3** — while `x`/`buf`/`p` were still in EWRAM — and from
the isolated `ovl` arm, which times the kernels on the bench's **EWRAM** buffers
(`KX`/`KZ`/`KY`/`KP`), not on the product's IWRAM `HotState`. Those readings
overstated every kernel by 21–33%, and C-3's 87,040-cycle gain was never
attributed. It is now.

**Method** (PR C's, one image, `make uli-attrib`): time the product arm, then the
same frames with exactly one part removed, and difference the per-frame windows
at the same frame index. The arms are timing-only cargo features on
`uli-lite-gba` (`attrib-pre`/`-fft`/`-post`/`-ola`/`-synth`/`-entropy`/`-ms`);
the pins fail by construction there and are never quoted. `attrib-dump` costs a
constant **+16 cycles per window** (control 274,214 vs the product's 274,198), so
the absolutes below are the product arm's and every delta is a frame-aligned
control-minus-arm.

**Production frame 274,198 worst (frame 98) / 232,090 mean, attributed** (the f98
column and its shares are the dump image's, which is +16; the deltas are exact):

| part | mean | share | at f98 | share | C/asm | ratio |
|---|---:|---:|---:|---:|---:|---:|
| `pre` (Rust) | 23,747 | 10.2% | 26,464 | 9.7% | 19,400 | 1.22x |
| `fft` (asm v4) | **83,341** | **35.9%** | **81,034** | **29.6%** | 62,800 | 1.33x |
| `post` (Rust) | 23,882 | 10.3% | 22,378 | 8.2% | 21,800 | 1.10x |
| `ola` (Rust, fused s8) | 33,988 | 14.6% | 34,387 | 12.5% | 30,600 | 1.11x |
| synthesis, all 8 calls | 160,268 | 69.0% | 162,987 | 59.4% | 134,600 | 1.19x |
| entropy + dequant | 66,943 | 28.8% | **101,122** | **36.9%** | ~42,600 | ~1.6x |
| M/S | 4,413 | 1.9% | 9,619 | 3.5% | ~4,400 | ~1.0x |
| wrapper / glue | 481 | 0.2% | 486 | 0.2% | — | — |

The C column is the record's own per-channel decomposition doubled (pre 9,700 +
fft 31,400 + post 10,900 + ola 15,300) plus its entropy/dequant/M-S row. Those
rows sum to **181,600, within 0.2% of the record's worst frame (181,928)**, so
they are best read as *C's worst-frame shape* — which is the comparison the gate
actually needs. It is still a different corpus and image: a diagnostic, never an
acceptance criterion.

**What it corrects, in order of consequence:**

1. **On the worst frame, entropy is the critical path, not the kernels.**
   Entropy + dequant + M/S is **110,741 cycles (40%)** at f98 against C's
   ~47,000 — a **~63,700 gap**, i.e. **69% of the frame's 92,286-cycle distance
   to C**. Synthesis is 162,987 against 134,600: a 28,387 gap (31%). The previous
   order put entropy *fourth*, behind three kernels whose combined prize is
   under a sixth of it.
2. **The kernels are 1.10–1.33x C, not 2.4x.** Three of the four are
   compiler-generated Rust at 10–22% above hand-written asm — not 40–82%. The
   "C hand-wrote four kernels, we hand-wrote one" framing was true of the
   *provenance* and wrong about the *cost*.
3. **The one kernel already in asm is the largest single kernel.** The FFT was
   believed 1.97x C from a 61,852 c/call reading; that reading was on EWRAM
   buffers. In-frame it is **41,671 per channel = 1.33x C**, and its prize is
   **20,541/frame (18,234 at f98)** — still the biggest kernel item, now
   correctly sized.
4. **`pre` + `post` + `ola` together are worth 9,817/frame**, not the ~38,000
   the previous order projected. Even all three at C-class would not clear the
   12,054-cycle deficit on their own.
5. **M/S is closed** (4,413 mean vs C's ~4,400). Its 9,619 at f98 is the dense
   frame transforming more coefficients, not a slow loop.

### The order of work

Every item is a projection until it lands as its own measured arm, with a
same-image control, twinned bit-exactly (the discipline that has held).

1. **Entropy + dequant — the sub-frame attribution first, then the rewrite.**
   It owns 69% of the worst frame's gap to C and it is the only item big enough
   to buy the ≤85% target rather than just 100%. PR C already required a
   per-stage attribution one level down (bit reader / `huff` / `esc` / dequant /
   sign / band loop) before code; the number that justifies it now exists.
   Reference point: 110,741 at f98 against C's ~47,000 for the same stage is
   **2.36x**. The frame's counts are known (`make uli-frame-ops`) and the gate
   frame is **pair-class and M/S — 352 pairs, 4 quads, 13% long codes** — so the
   hot path is `CT_PAIR`, not the quad path the earlier model was tuned on.
   The five levers, the code measurements behind them and the next instrument
   are in "Step 0b" below.
2. **The asm FFT, v5 — against the in-frame baseline of 41,671/channel.** The
   remaining known differences are the multiply core and the A..D/output traffic;
   prize 20,541 mean / 18,234 at f98. Note the earlier "1.97x, near its floor"
   reading was measured with EWRAM buffers and is now superseded.
3. **`pre` asm (4,347) → `ola` asm (3,388) → `post` asm (2,082).** In that
   order, and with no expectation that they alone reach real time.
4. **PR E — full-album worst frame**, and the gate total (decode + convert + IRQ
   + DMA). This is U4's real witness.

The arithmetic on the two targets, from these numbers: **<100%** needs 12,054 on
the worst frame — any ~12% entropy win clears it, as would ~65% of the FFT's f98
prize. **≤85%** needs 51,376, which means C-class entropy *and* the FFT prize
both land; nothing smaller adds up to it.

### Caveats that must travel with these numbers

- **Per-kernel rows are ±3%.** The single-kernel deltas sum to 164,958 mean
  against the measured sequence 160,268 (+2.9%; at f98, 164,263 vs 162,987,
  +0.8%). The interaction is the FFT's data-dependent trivial-butterfly path:
  removing `pre` feeds it a stale `buf`. Quote the sequence row for synthesis.
- **`attrib-entropy` alone is not quotable.** Removing `dec_chan` freezes the bit
  position, so all 129 frames synthesise one stale frame (155,999 mean against an
  expected 160,749). The entropy row comes from `synth` minus `synth-entropy`.
- **The wrapper is 481 cycles.** The frame has no glue left to win.
- **105% is the worst of a 129-frame iteration corpus, not a worst-case
  guarantee** — C's hardest frame was #27,806 of 167,222. Plan for margin.
- Hand-written asm is only worth it once placement is right. Placement is now
  right (code, tables and hot buffers all in IWRAM); that is the precondition
  these steps depend on, and it is met.

## Decision (2026-11-14): ULI2 is GBA-first

ULI2 is a GBA product codec by definition: one fixed rate (32,768 Hz), one
hop (512), one target (ARM7TDMI), one player. The portability constraints
inherited from the `flac-lite` split are therefore **retired for the product
path**. The portable core is not deleted — it becomes the bit-exact reference
and the test oracle — but it is no longer the thing that has to ship, and the
product is no longer required to be safe, portable or platform-free.

### Constraints retired, and constraints kept

| Constraint | Product path | Why |
|---|---|---|
| `#![deny(unsafe_code)]` | **retired** | Raw pointers, unchecked indexing, block moves and asm are the performance work. |
| Portable facade / no platform codegen | **retired** | ARM state, `.iwram` placement and GBA-shaped layout are allowed. |
| The core *is* the product | **retired** | The product decoder lives in `uli-lite-gba`; the core is the oracle. |
| No `agb` in the codec | **retired for the product**; keep the decoder itself `agb`-free | The decoder needs no MMIO; keeping it `agb`-free is what lets the host twin tests run. `agb` belongs to the player. |
| `no_std` | **kept** | `agb` is `no_std`; the GBA has no OS. Not a portability tax. |
| Zero allocation, caller-owned state | **kept** | Real-time and memory constraints, not portability. |
| Integer/fixed-point only | **kept** | No FPU; the format's arithmetic. |
| Bit-exactness against the C-pinned corpus | **kept** | The correctness witness; every optimization is twin-tested. |
| A live reference implementation | **kept** | The oracle is what keeps a fast unsafe decoder honest. |
| Licensing / `publish = false` | **kept** | Unresolved; unchanged by this decision. |
| Rust encoder later | **kept** | Separate milestone; the C encoder remains of record. |

---

## Architecture

```
crates/uli-lite/            reference + oracle, not shipped
  album.rs, tables.rs       cold ULA2/table parsing (reused by the product)
  bitstream.rs, frame.rs    safe codec + the Kernels seam
  imdct.rs                  portable synthesis (ARM-mode on ARM targets)
  PLAN-OF-RECORD.md         this file

crates/uli-lite-gba/        the product decoder
  src/lib.rs                ARM-mode, unchecked synthesis kernels in .iwram
  src/decoder.rs (PR B)     GbaDecoder + HotState: the 32-bit bit reader,
                            unchecked Huffman, dequant and M/S hot path
  tests/                    host twins against the core and the corpus pins

examples/uli_bench/         the measurement vehicle
  full frames through both kernel sets, plus per-kernel decomposition
```

The seam is `uli_lite::Kernels` (landed in PR A). `Decoder<K>` defaults to
`PortableKernels`, so every existing caller and test is unchanged; the product
instantiates `Decoder<uli_lite_gba::ArmKernels>`. This lets the bench measure a
**full frame through the product kernels** instead of projecting it from
per-kernel sums.

The product may call the core for cold work (album and table parsing). The hot
decode path is expected to move into `uli-lite-gba` in PR B; the core remains
the control and the host-testable oracle.

---

## Memory layout — mirrored from the C reference (2026-11-15)

Placement is **not a guess and not trial-and-error**. It is read from the C
reference's own linker script (`gba-audio/tools/gba/gba.ld`) and working-set
declarations (`src/uli/gba/uli_gba_int.h`, `uli_gba_dec.iwram.c`). Analysis
only — no C source is incorporated.

### The two projects have opposite defaults

This is the root of the earlier ambiguity:

- **C**: plain `static`/global data goes to **IWRAM** — `.data` and `.bss` are
  both placed in IWRAM. Only things explicitly marked `.ewram`/`.ewram_bss`, or
  compiled into a `*.ewram.o`, go to EWRAM. The linker `ASSERT`s that `≥1 KiB`
  of IWRAM is left for the user stack, so **C's decoder uses essentially no
  stack**: every working buffer is a static in IWRAM.
- **ours (agb)**: `.bss`/`.data` go to **EWRAM**; IWRAM receives only what is
  explicitly `#[link_section = ".iwram"]`. A plain `static` here is the opposite
  of the same declaration in C.

So "which section does this buffer belong in" is answered by C's source, not by
our defaults.

### C's IWRAM working set (measured from the source)

| C object | bytes | meaning |
|---|---:|---|
| `uli_g_X[2][512]` i32 | 4,096 | per-channel coefficient buffer |
| `uli_g_buf[512]` i32 | 2,048 | FFT scratch |
| `uli_g_P[2][256]` i32 | 2,048 | overlap (OLA) state |
| `uli_g_preP[512]` i32 | 2,048 | pre-twiddle table (IWRAM copy) |
| `uli_g_postP[512]` i32 | 2,048 | post-twiddle table |
| `uli_g_twP[504]` i32 | 2,016 | FFT twiddles |
| `uli_g_winP[512]` i32 | 2,048 | window table |
| `uli_g_perm[256]` u16 | 512 | post permutation |
| `uli_g_q3[81]` u8 | 81 | quad-class lookup |
| `UliGState uli_g` | ~600 | bit reader + per-frame state |
| **sum** | **~19,545** | |
| + ARM code (their doc: ~5 KB) | ~5,000 | kernels + IRQ + conv |
| + user stack (assert floor) | ≥1,024 | C uses almost none |
| **total** | **~25.5 KB of 32 KB** | |

The ~100 KB Huffman `UliCodeTables` is the only large object, and it is in
**EWRAM** (`UliGState.T` is documented `/* EWRAM */`). The audio ring buffer is
also EWRAM.

### Ours today vs the C-mirrored target

| buffer | C | ours now | target |
|---|---|---|---|
| X/buf/P (`HotState`) | IWRAM 8,192 | **EWRAM** 8,192 | IWRAM (C-3) |
| tables (perm/pre/post/tw/win) | IWRAM 8,672 | **ROM** (feature copies) | IWRAM |
| synthesis kernels | IWRAM | IWRAM | IWRAM |
| Huffman code tables | EWRAM | EWRAM | EWRAM |
| decoder state | IWRAM ~600 | EWRAM | IWRAM (small) |

### The stack figure in this file was stale — corrected by measurement

Every `~15.0 KiB stack` number in this file (PR B onwards) came from a period
when the bench's twin scratch buffers were **stack locals**. They were moved to
EWRAM statics during PR C-1, and the figure was never re-measured. It is now
measured directly: `examples/uli_bench` fills the free IWRAM stack region below
`sp` with a sentinel before the product pass and scans it afterwards
(`stack prod high-water`).

**Measured: 768 B of descent below `run()`'s frame** — not 15,040 B. Repeatable.

**Refined 2026-11-15 (C-3): 768 B is not the whole requirement.** The probe fills
below the *current* `sp`, so it measures only the descent from `run()`. The real
budget runs from the BIOS stack top (`0x03007F00`; agb never sets `sp`), and the
instrument's total high-water is **≈2,960 B** = 2,192 B above `run()` (entry
wrapper + `agb::Gba`) + 768 B of descent. Write the budget as
`usable = 0x03007F00 - __iwram_end`, and require `usable >= ~2,960 B` for this
instrument. (This is the same lesson one level down: the first figure measured a
*relative* quantity and was read as an absolute one.)

### Consequence: the C layout fits, and two "blocked" items are unblocked

With the real stack cost the arithmetic reverses. Measured in the bench
(`--features asm-fft,iwram-tables`, i.e. C's table layout):

| arm | worst | mean | of period |
|---|---:|---:|---:|
| product, TW only in IWRAM (`asm-fft,iwram-tw`) | 411,431 | 364,481 | 157% |
| **product, all tables in IWRAM (`asm-fft,iwram-tables`)** | **387,879** | **340,929** | **148%** |

`.iwram` = 25,548 B, leaving 6,964 B, minus the 768 B stack → **6,196 B
headroom**. `iwram-tables` was recorded as *crashing* on the stale figure; it
now runs with all twins and pins green. Mean and worst fall together, and the
`ovl` full-frame arm falls to 1,570,540.

This is the placement lesson again, one level up: **the binding constraint was
never the stack, it was a stale measurement.** The next placement step (C-3,
`HotState` → IWRAM, 8,192 B) needs the superseded v1/v2 kernels dropped (1,520 B
frees it) — arithmetic in the C-3 row below.

## Measurement method

Every number in this file follows the Phase 1b rules, so the arms stay
comparable:

- **Correctness before speed.** The measured image proves its own decode
  against the C-reference pins (`e6e87652` / `b3530978`) before any timing
  line is emitted; the timed pass is hashed too.
- `WAITCNT = 0x0000` (4/2, no prefetch) for the recorded arm; the boot value
  (`0x4317`, 3/1 + prefetch) is printed alongside.
- Free-running timer2→timer3 at 16.78 MHz, IRQs off inside every window,
  empty-window overhead measured and subtracted.
- Sums and counts are reported; means are derived host-side. No divide on the
  measured path (ARM7TDMI has none).
- A wrapped window is `u32::MAX` and fails the verdict; it is never truncated.
- **mGBA, not hardware.** mGBA does not model the ROM prefetch buffer, so
  cartridge-resident code is charged pessimistically; this is stated per
  record. U4 keeps a hardware run as its witness.
- Any arm that changes arithmetic must have an on-target twin against the
  reference, not just a host twin.
- **Iteration corpus vs verification corpus (decided 2026-11-14).** The
  129-frame `uli2_tiny` corpus is the iteration corpus: it is what every arm is
  measured on while the decoder is still multiple × over budget, because
  full-album runs would slow the loop without changing which differential is
  next. It is **not** a worst-case witness — gba-audio's hardest frame was
  frame 27,806 of 167,222. A comprehensive worst-frame analysis (FLAC-style:
  min/max with frame index and arm, paired deltas, a second mechanism rather
  than a second transcription — see [`../../flac-docs/measurements.md`](../../flac-docs/measurements.md))
  is **deferred** to PR E, and no real-time claim may rest on the 129-frame
  number alone.

---

## Phases and PRs

### PR A — GBA-first architecture + full-frame instrument (landed 2026-11-14)

- The decision above, recorded here and in `ULI2.md`.
- The `uli_lite::Kernels` seam and `PortableKernels`; `Decoder<K>` generic with
  a default.
- `uli_lite_gba::ArmKernels` implementing the seam with the existing ARM/IWRAM
  kernels.
- `examples/uli_bench` decodes **full frames through both kernel sets** and
  keeps the core as the control; the per-kernel arms and the on-target twin
  stay.
- Host twin: `uli-lite-gba` decodes the committed corpus frame-for-frame
  through `ArmKernels` and compares every sample with the core.
- Gate: both full-frame arms match the corpus pins, `xover=0 hfnz=0`, no
  wrapped window. No speed claim beyond the recorded number.

Result: **PR A measurement** (full-frame worst / mean, 129-frame corpus,
262,144-cycle period):

| arm | worst | mean |
|---|---:|---:|
| core kernels (control, same image) | 2,676,986 (1021%) | 2,171,620 (828%) |
| overlay kernels, full frame | 1,829,247 (**698%**) | 1,322,800 (**505%**) |

The within-image A/B is the valid one: **1.46x worst / 1.64x mean**. The
overlay full frame is exactly `core_rest + 2 × overlay_seq` within 0.02%,
which confirms the Phase 1b decomposition. The core control in this image is
~8% faster than Phase 1b's image (2,970,042 → 2,676,986 worst) although the
kernel sequence is unchanged (667,264 both times); the difference is code
layout and instruction fetch, not arithmetic. **Absolute frame numbers are
image-specific; the control must live in the same image as the arm.**

The Phase 1b projection (810% / 534%) is therefore replaced by a measurement:
**698% worst / 505% mean** for the product kernel set. Still ~7x over the
frame period; PR B's gate is ≤2x.

### PR B — GBA-first hot path (landed 2026-11-14)

This is where the retired constraints are exercised.

- Move the hot bit reader, Huffman decode, dequant and M/S into
  `uli-lite-gba`: `GbaDecoder` + `HotState` in `src/decoder.rs`.
- 32-bit cached bit window (no 64-bit shift helpers), sentinel `error` instead
  of `Result` per symbol, unchecked pointers with documented bounds, ARM
  state, `.iwram` code, cold paths out of line.
- Keep the core as the oracle: frame-for-frame host twinning over the corpus
  (`tests/product_twin.rs`), and the album pins on target.
- Gate: **full-frame worst ≤2× the 262,144-cycle period**, pins unchanged.
  **Met: 204% worst / 177% mean.**

Result: **PR B measurement** (full frame, mGBA, WAITCNT 4/2, 129-frame corpus,
262,144-cycle period):

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control, same image) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path (`GbaDecoder`)** | **533,605 (204%)** | **463,660 (177%)** | 351,556 |

All three arms pinned `e6e87652` / `b3530978`, `xover=0 hfnz=0`; the on-target
twin passed. The product frame is `rest + 2 × overlay_seq` within 0.1%:
overlay seq/channel 169,061, so **`rest` (non-synthesis) fell from PR A's
837,092 to 195,484 cycles/frame — a 4.3× cut**, and synthesis is now 63% of the
frame (it was 27% at PR A). The ladder: 2526% (Thumb core) → 1133% (ARM core)
→ 698% (PR A kernels) → **204%** (PR B hot path).

**Placement finding (recorded because it cost a hang).** PR B's first on-target
run hung with `pc=0x00000004` (the undefined-instruction vector). Root cause:
`HotState` (8 KiB) placed in `.iwram` left only `0x03007f00 − __iwram_end`
(≈14 KiB) for the stack, and the product path needs **≈15.0 KiB** (measured
stack pointer floor `0x03004440`, i.e. the stack overflowed past `__iwram_end`
into `PRODUCT_HOT` and the ARM/Thumb thunks, then branched into uninitialised
IWRAM). The host twin was green throughout, so this was purely a target
placement defect, not arithmetic. **`HotState` lives in EWRAM in the bench**;
its IWRAM placement is a U6 decision that must first budget stack, not just
code+data. Weathervane: the ARM build was 1,236 B larger in `.iwram` than the
Thumb one, and the handful of bytes between `__iwram_end` and the stack floor is
all the margin there was.

Open item this buys: the remaining synthesis gap. `ArmKernels` seq/channel is
169,061 against the C/asm reference's ~67,300 (2.51×); the OLA is still the
outlier. PR C is now scoped to kernels, not to the entropy path.

### PR C — real-time: attribution first, then the two rewrites (in progress)

PR C opened by measuring where the 523,267-cycle worst frame actually goes,
because at 2.0× the period a guessed optimisation is as likely to move code
into a worse register allocation as to help. The attribution below is the
substance of this PR; the code change it produced is small and the remaining
gap is scoped as two named rewrites.

**Attribution, worst frame (frame 101), 523,267 cycles, mGBA 4/2:**

| part | cycles | share | of which |
|---|---:|---:|---|
| synthesis kernels (2 × pre+fft+post+ola) | 328,502 | 63% | fft 170,130, ola 76,932, pre 42,478, post 41,238 |
| entropy + dequant (`dec_chan` × 2) | ~175,000 | 33% | huff, bit reader, dequant |
| M/S + frame wrapper | ~20,000 | 4% | |

Measured by difference on one image: total frame, then the same frame with
the eight kernel calls removed (194,765), then with `dec_chan` removed too
(~20,600). The kernel figure agrees with `2 × overlay_seq` to 3%.

The two numbers that set the work:

- **Kernels are 2.4× the C/asm reference** (328,502 vs the C record's
  **134,600** for the pair: 67,300 per channel, from its own stage-2
  decomposition `pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k`). The FFT
  butterfly alone spills 66 intermediates to the
  stack because 14 values are live at once against 16 architectural registers;
  the multiply codegen is already optimal (6 `SMULL` + 6 `SMLAL`, no
  `__aeabi_lmul`). This is a register-allocation problem, and Rust cannot
  express the allocation: **the kernels need hand-written ARM asm** (the
  milestone ULI2.md always placed after the hot path).
- **Entropy is ~6.6× the C reference per coefficient** (~272 c/coef against
  the C bench's measured ~41 for entropy+dequant). This is *not* explained by
  the obvious suspects — see “ruled out” below — so it needs a redesign of the
  bit-reader protocol, not tuning.

**Cheap levers, measured and ruled out** (each built and run on target):

| lever | result |
|---|---|
| fuse dequant into the coefficient loop, drop the `m[16]` scratch array | **neutral** (535,941 vs 533,605) — LLVM already handled the two-pass form |
| inline all of `huff` at the call sites | +2.7% (519,397) but **+4,252 B of IWRAM** — poor trade where U6 is binding |
| inline only the `huff` LUT fast path, keep the long search out of line | **+1.6% (523,267) for +896 B** — kept |
| `-Cforce-frame-pointers=no` | inert (identical cycles), matches the FLAC track's E7 |
| synthesis tables in IWRAM (`iwram-tables`) | **superseded** — recorded as *crashes* on the then-believed ~15,040 B stack. The stack is now measured at ~2,960 B total and `iwram-tables` runs and wins (148% -> see "Memory layout"). Kept here only as a record of what was believed at the time. |

Gate: **≤1×** worst. **Not met by this PR** (523,267 = 200%); it is a
measurement, one small win, and the scope statement for the two rewrites
below. U4 stays open.

#### PR C-1 — asm kernels (the 63%)

Hand-written ARM kernels in `.iwram`, twinned bit-exactly against the Rust
reference (the existing `tests/twin.rs` and `tests/decoder_twin.rs` are the
witness). **Provenance decided 2026-11-14:** ARM-mode asm may be adapted from
the reference implementation; **C source may not be incorporated**, and there
must be **no build, link or vendoring dependency on `gba-audio`** — every
artefact this project uses lives in this repository. The asm is ours to write
and maintain (`global_asm!`), and it is twinned against our own oracle, not
theirs. Target the C record's own decomposition: **~134,600 cycles/frame for
the pair**, i.e. pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k per channel.
Prize: **~194,000 cycles/frame**. Its per-kernel rows are the useful part — our
FFT is 170,130 for the pair against 62,800, and our OLA is 76,932 against 30,600
— so FFT carries ~107,000 of the prize and OLA ~46,000.

#### PR C-2 — entropy rewrite (the 33%) — **landed as C-3a, 2026-11-14**

Redesign the bit-reader protocol. The 32-bit cached window from PR B is in
place, so this is not “more of the same”: candidate directions are holding the
reader state in registers across a whole band instead of behind `&mut`, a
byte-oriented refill path, and folding `huff`+`skip`+sign into one symbol
routine. Prize: ~90,000 cycles/frame.

**Landed as C-3a plus C-3c**, with the reader as a `Copy` local whose position
is derived from a stream cursor, packed `huff`+`skip`, the inline `clz6` escape
path, `q3`/`rq` tables, direct coefficient writes with no pre-zero pass, and
`nb`-bounded M/S. Measured: **−35,000 cycles/frame**, not the projected 90,000.
See “PR C-2 / C-3a” in the measurement ledger. The rest of this section is the
pre-measurement text and stays for context.

*Step 0b (2026-11-15) adds the caveat this paragraph implies but does not
say: the symbol-level packing landed, the **reader contract did not**. `dec_chan`
still takes `br: &mut BitReader`, so the struct's address escapes into
`huff_slow`/`eg_slow` and every refill stores the window back to memory; `skip`
still branches on validity, and the sign is still a `peek` plus a `skip`. The C
decoder's `REFILL`-once/branchless-`SKIP` protocol is the part of "the reader
in registers" that is still open — see "Step 0b".*

Together C-1 and C-2 project to ~239,000 cycles (~91%) once the C kernel row is
pinned at 134,600 (PR C recorded ~94% against the earlier ~146,000). Neither is
a tuning change, which is why PR C splits into them rather than closing here.

**They are not sufficient for the target, and ~91% is not C-class.** The gap to
the C/asm record is 523,267 → ~182,000 = **341,267 cycles**; C-1 (~194,000) +
C-2 (~90,000) account for **284,000**, leaving **~57,000 unidentified by the
plan**. A parity audit against the C product decoder
(`../../gba-audio/src/uli/gba/uli_gba_dec.iwram.c`, 287 lines) found every
remaining differential as a named item, listed as C-3 below. 94% would pass a
bare `≤100%` read of U4 and then fail the gate total (decode + convert + IRQ +
DMA): the C record's 69.4% decode wall sits inside a 73.63% gate total, i.e.
~4 points of headroom for everything that is not the decoder. **The plan
therefore targets parity, not the period.**

#### PR C-3 — differential port against the C product decoder (proposed 2026-11-14)

Reference: `../../gba-audio/src/uli/gba/uli_gba_dec.iwram.c` (the C product
decoder: 287 lines, ARM-in-IWRAM, bit-exact against its own reference) and
`../../gba-audio/src/uli/gba/uli_imdct_arm.S`. Reading it against
[`../../crates/uli-lite-gba/src/decoder.rs`](decoder.rs) and
[`lib.rs`](lib.rs) yields the items below; each is a differential to be removed,
not a redesign. Same discipline as everything else in this file: one arm, same
image, same corpus, one measurement each.

**C-3a — `dec_chan` structure (the content C-2 lacked).** C-2 named a prize but
not a design; the C decoder *is* the design, and every per-coefficient cost left
in our hot path has a named counterpart there.

| `uli-lite-gba` today | C reference | scale (frame 101) |
|---|---|---|
| `[s / 27, (s / 9) % 3, (s / 3) % 3, s % 3]` per quad | `uli_g_q3[81]` byte table: one load, shift by `6 - 2k` | 248 quads |
| clamp compare + `rq as i64` range dance per nonzero coefficient | `rq[16]` per-class table of `16m − dq`, `(m << 4) − dq15` for escapes, then one `*= step` | 616 nz |
| `m: [0i32; BANDW]` scratch written then re-read per band | direct writes into `X` | 512 extra ld/st per channel |
| pre-zero fill of all 512 coefficients, then zero bands again | `zero16()` = 4 × `STM`; EOB tail `STM`-filled; no memset at all | 512 stores per channel |
| `huff` returns a symbol, then `br.skip(len)`; `br.bad` checked per band | `huff` returns the LUT entry: symbol packed with its code length in the low 5 bits, already consumed | per symbol |
| `eg()` is `#[inline(never)]` | `esc()` with a `clz6[64]` fast path inline; out of line only for z ≥ 6 | 140 escapes |
| `&mut BitReader { stream, bitpos, .. }` across the whole path | local `BR { w, n, p }` copied in and out **once per channel** | everything |

Prize: the bulk of C-2's ~90,000, made auditable item by item. **C-2 should be
executed as C-3a rather than as an independent rewrite** — otherwise the same
work is specified twice.

**C-3b — stats-free product build.** The C product decoder keeps **no
counters**: no `nz`/`zc`/`pairs`/`quads`/`escs`/`raws`, no per-coefficient
`xover`/`hfnz` compares; its stage profiling is behind `ULI_GBA_PROFILE` and
compiled out of the shipping build. Ours are inside the timed loop. Put `Ops`
and `Diag` behind a `verify` feature (the twin and corpus tests keep them), so
the product arm measures the product. This also repairs the fairness of every
future `×%` comparison against the C record.

**C-3c — malformed-stream exactness off the valid path (approved 2026-11-14).**
The i64 range fallback in the dequant step exists to keep escaped magnitudes
exact on malformed streams; every *valid* coefficient pays for it. This project
controls its own encoded data and **promises nothing about the arithmetic of
malformed streams**, so the fallback is not a product requirement. Keep it in
the verifying build (it is still what makes the oracle bit-exact) and let the
product use the 32-bit `SMULL` path. Rejection-only behaviour stays: a corrupt
stream must set a sentinel and return, never panic, hang or corrupt state.

**C-3d — OLA output fusion, one format per arm.** C ships three kernels
(`uli_ola_s8_asm`, `uli_ola_s8w_asm`, `uli_ola_s16_asm`) with the rounding
`(acc + 2^39) >> 40` and the store fused in, and the format selected **outside**
the sample loop. Our `ola` branches on `Option<&mut [i16]>` per sample, and
`examples/uli_bench` times `Some(l16)` + `Some(r16)`, so the measured arm writes
**both** int8 and int16 for every sample — a configuration the C record never
timed. OLA is the 2.5× outlier (76,932 vs ~15,300), so this is a measurable part
of C-1, not decoration.

**C-3e — `nb`-limited M/S.** C's `dec_chan` returns the bands-before-EOB count
and `uli_gba_frame` runs `uli_ms_asm` over `nb * BANDW`, not all 512. We track
the same value and ignore it for M/S.

**Revised budget** (the ledger's worst frame, 523,267 → the C record's
~182,000). Cumulative, because the items overlap: the C-1 target row already
contains C's OLA, so C-3d only counts the part of our dual-output overhead that
C's row never had.

| after | frame | % of period | basis |
|---|---:|---:|---|
| PR C | 523,267 | 200% | measured |
| **+ C-3a/C-3c** | **488,685** | **186%** | **measured** — 35,000 saved, not 90,000 |
| C-1: kernels → C's 134,600 | ~295,000 | ~112% | C's per-kernel rows; re-based on the measured frame |
| C-3a: entropy/dequant → ~85,000 | ~239,000 | 91% | C-3a table; still ~2× C's 42–48k |
| + C-3b/C-3c (counters, i64 gating) | ~224,000–234,000 | 85–89% | not yet measured |
| + C-3d/C-3e (OLA single-format, `nb`-limited M/S) | ~202,000–222,000 | 77–85% | not yet measured |
| + C-1's stack cut → `iwram-tables` | **~193,000–213,000** | **73–81%** | 6.3% of kernels |
| C/asm reference | ~182,000 | 69.4% | gba-audio record |

The last two rows are the point of the audit: C-class parity needs the
**structural** items (C-3a, C-3d) and the **build-shape** items (C-3b, C-3c),
and it needs C-1 to be specified as *asm that also cuts the stack*, because
that is what buys the IWRAM table budget back (see U6). These projections are no
more trustworthy than PR C's levers were; they are ranges to be replaced by
measurements, one arm at a time.

**Read the range honestly — and it moved when C-3a was measured.** The rows
above were projections; the C-3a/C-3c row is now a measurement, and it delivered
**35,000 of the projected 90,000**. Re-based on that, the item list projects
**~250,000–280,000 (95%–107%)**, i.e. the period at best, **not the ±5% band
this file calls parity**. Two consequences are recorded rather than smoothed
over: (1) C-3b and the diagnostics are the largest *measured* lever left
(~11,000 for the stats alone, ~30,000 with the diagnostics) and are worth doing
next; (2) parity now needs something the list does not contain — a per-stage
attribution of the ~140,000 cycles entropy and dequant still cost (5.3× the C
record per coefficient, down from 6.6×), because the structural port did not
move that ratio nearly enough to close the gap. Do not let the table become a
forecast.

**Proposed order.** Each step is small enough to twin and measure on its own,
and each changes the frame the next one is measured against — so every arm keeps
its control in the same image (PR A's rule).

1. **C-3b (+ C-3c, landed 2026-11-14)** — the counters and format-rule
   diagnostics now sit behind `verify`; measured **−30,822 worst / −18,427 mean**
   (the attribution predicted −30,482 / ~20,000). Gate unchanged: `verify` is off
   for the product arm, on for the twins and for the arm that witnesses the
   product decoder's own `xover/hfnz`.
2. ~~**Coefficient buffers to IWRAM**~~ — **DONE 2026-11-15 (C-3): −87,040
   cycles/frame, 138% → 105%.** The block never existed (it was the stale stack
   figure) and the prize was ~6x the 10,000–15,000 projection, because it
   captures all coefficient traffic through `x`/`buf`/`p`, not just M/S. See the
   C-3 section below.
3. ~~**C-1** — asm kernels~~ — **one of four landed** (the FFT, v1→v4). The other
   three (`pre`, `post`, `ola`) are still Rust and are now the critical path:
   see **"Critical path to real time"** at the top of this file for the ordered
   list and the measured headroom. (This item's original justification — that
   `iwram-tables` crashes on a ~15,040 B stack — was disproven by measurement.)
4. ~~**C-3d**~~ — **DONE 2026-11-15**: OLA output fused per format; production
   OLA is 1.40x C and the int8-only frame dropped 148% → 138%.
5. ~~**C-2 = C-3a (landed 2026-11-14, −35,000 measured)**~~ — done.
6. **Re-measure and take the U4 verdict** against the **real-time budget**, not
   against a percentage borrowed from C, and not against the 129-frame corpus
   alone (PR E is the witness).

**The target is the real-time budget, not a percentage borrowed from C
(clarified 2026-11-15).** The C record's 69.4% is **not a benchmark to match** —
it is a *compute-budget existence proof*. C+asm runs this codec in real time on
this exact hardware (ARM7TDMI, 16.78 MHz, 262,144 cycles per 512-sample frame),
which settles the only question that matters: **the work fits the machine.** The
figure is therefore used for what it is — evidence, and a coarse sanity
reference for where a competent implementation lands — and *not* as a target to
be "within 5% of". Two consequences follow, and they replace the earlier
"C-class parity" framing:

- **The goal is U4/U5: real time with headroom**, i.e. the decode wall under the
  period with room for the gate total (convert + IRQ + DMA) and this tree's UI.
  The ceiling is written down from a measured near-real-time arm, not assigned.
- **"C-class" is retired as a gate.** There is no reason a Rust+asm stack should
  be slower than a C+asm one on the same hardware; if C proves the budget is
  reachable, Rust+asm is expected to reach it too. The comparison to C is kept
  only as a diagnostic for *where* time goes, never as an acceptance criterion.

The C record's 69.4% was taken on gba-audio's whole album (167,222 frames, worst
frame 27806) with its own toolchain and image, and absolute frame numbers here
are image-specific (the same kernel sequence measured 2,970,042 in one image and
2,676,986 in another), so no `×` against it is like-for-like until PR E. Its
per-kernel decomposition (`pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k` per
channel = 134,600/pair) remains useful as a **rough shape of a solved
implementation**, which is how this file now uses it.

### PR D — playback (after U4)

- Reuse the 32,768 Hz ring/FIFO feeder; int8 direct and the optional pre-DAC
  int16 path; UI from the ULA2 tables; `PcmSource` seam.
- **Two output regimes, one decoder (decided 2026-11-14).** The speaker profile
  is **in scope**, not an afterthought: the decoder always produces full stereo
  (two channels synthesised, stereo sample pairs out) and the mono collapse
  happens in the **output stage**, after the decoder — never inside it, and
  never by shortening synthesis to one channel. Both regimes are measured: the
  headphone path at the stereo decode wall, and the speaker path including its
  fold-down in the gate total.
- Fold-down method is a PR D call, to be measured there: `(L+R)/2` (the mono mix
  gba-audio's driver uses, ~13.65%/frame in their stage) or single-channel
  (`L` only). **They are not equivalent** — single-channel output is not a
  fold-down: hard-panned content vanishes, and on an M/S frame `L = M + S`
  reproduces mid+side rather than mid. Single-channel is cheaper and is
  acceptable for the speaker if the measurement supports it; record which one
  ships and what it costs.
- Gate: U5 (zero underruns, clean period audit) in mGBA, then hardware, with
  both regimes reported.

#### Cross-track facts PR D's owner needs (reported by the application track, 2026-11-15)

Not codec rulings and not claims on U4 — three things decided above the seam that change
what PR D reuses. Authority: [`../../ALBUM-PLAYER.md`](../../ALBUM-PLAYER.md) §2.2, §2.3,
§4.3, §9.0.

1. **The feeder PR D says it will reuse is moving.** `RingFeed` + its `checksum` pair live in
   `examples/flac_spike/src/`, a spike directory that both shipped FLAC images happen to
   `#[path]`-include. The application track has ruled it must not stay there (§2.2): Phase 1
   lifts the pair into project source (proposal `crates/gba-playback`) and adds `truncate()`
   to the lifted type (§4.4, Q13b — AP-0 measured that without it a skip cannot beat one
   period per queued chunk: 39–53 periods at the ring's ceiling). `flac-check` +
   `product-play-test` stay the witnesses, because the move re-baselines the shipped images'
   bytes. **If PR D wants the feeder, ask for it on the lifted crate rather than `#[path]`-ing
   the spike.**
2. **"UI from the ULA2 tables" overlaps the application manifest (Q13).** The app rules its UI
   reads `ALBM1` — its own per-track map of ROM locations, album-sample boundaries, titles and
   the progress reciprocal (§4.3) — because the features A1/A6/A10/A11 must exist on both
   backends and `ULA2`'s tables are one backend's. The resolution keeps both statements true:
   the adapter reads `ULA2`'s tables to position the decoder and **cross-checks them against
   the manifest at bake time, refusing on mismatch**, so "driven from `ULA2`" and "driven from
   `ALBM1`" are the same sentence wherever they agree and a build failure where they do not.
   What still needs a PR D ruling is where the output stage stops and the player begins — that
   decides where U5's gate gets written.
3. **The non-audio addend in the ≤ ~85% target is partly measured, and it is the app's to
   hand across.** AP-0 (`examples/album_probe/README.md`) measured the basic UI at **5,306
   cycles per video frame** = 10,832 of the FLAC 573,440-cycle period (**1.89%** of wall
   time), feeder 0.92%. Because that cost is VBlank-paced rather than per audio period, the
   share travels as a fraction of *time*: against ULI2's 262,144-cycle period the same
   arithmetic reads ≈4,950 cycles/period (**≈1.9%**). That is a conversion, not a ULI2
   measurement — the app re-measures it on the `BACKEND=uli2` arm (AP-6/AP-9, Phase 5) before
   anyone spends it, and viz + game cost are still unmeasured targets.

Also for PR D's assets: the album line's policy is that **no album audio is committed** — the
track list is tracked and every blob is generated at build time and pinned (§2.3). For the
lossy backend that composes with Q3 (encoder used off-tree): the bake hands the adapter
*already-encoded asset bytes plus a tracklist*, decodes nothing, and performs the same
manifest/viz/cost work as the lossless path.

---

### PR E — comprehensive worst-frame analysis (deferred; after the real-time regime)

The 129-frame corpus is an iteration instrument, not a worst-case witness. Once
the decoder is inside a realistic regime, run the worst-frame program on a
full-album corpus: worst/mean with frame index, sum and arm; a paired control
in the same image; the FLAC methodology in
[`../../flac-docs/measurements.md`](../../flac-docs/measurements.md) (the tail
is first-class, absolute arms are image-specific, pair rather than compare
across images). Deferred deliberately: it is not required to pick the next
differential, and full-album runs (~70 s each, 167,222 frames) would slow the
loop while the frame is still multiples over budget. **No real-time or U4 claim
may rest on the 129-frame number alone** — U4's witness is this analysis.

---

## Gates ledger

| Gate | Claim | Status |
|---|---|---|
| **U1 — reference** | `uli-lite` remains a bit-exact, host-testable reference (no longer a portable-product claim). | Met (Phase 0/1); redefined 2026-11-14. |
| **U2 — bit-exact host** | The reference matches the C-pinned corpus, frame for frame, on **valid** streams. Malformed input only has to be rejected safely — no arithmetic-exactness promise (decided 2026-11-14). | Met. |
| **U3 — bit-exact product** | The product kernel set produces the pinned int8/int16 frames, on target and in the host twin. | **Met for the full hot path (PR B).** `tests/product_twin.rs` (host, frame-for-frame) + `make uli-bench` on-target pins. |
| **U4 — real-time decode** | Worst full frame inside the **real-time budget** at WAITCNT 4/2: the decode wall must leave room for the gate total (convert + IRQ + DMA) and this tree's UI. **Not a "match C" target** (2026-11-15): C+asm proves the budget is reachable on this hardware, so Rust+asm is expected to reach it too; the C percentage is an existence proof and a diagnostic, not an acceptance criterion. **The ceiling itself is measured, not assigned** (2026-11-14): it is written down at the first near-real-time arm, from the measured gate total plus this tree's non-audio cost. Its worst-frame witness is PR E, not the 129-frame corpus. | **Not met.** PR B measured 204% worst / 177% mean; PR C added the attribution and a split-`huff` win to **200% worst / 174% mean** (523,267 / 456,187) and scoped the remainder as two rewrites (C-1 asm kernels 63%, C-2 entropy 33%). C-1 + C-2 projected to ~91%. **C-3a/C-3c then measured 488,685 / 429,339 (186% / 164%)**, and **C-3b measured 457,863 / 410,912 (175% / 157%)**, and **PR C-1 landed a bit-exact hand-written FFT that, wired into the product decoder, took the full frame to 430,997 / 384,046 (164% / 147%)**, then its trivial-butterfly follow-up (**v4**) to **425,795 / 378,845 (162% / 145%)** — see the ledger's “PR C-1 landed” section.  The rest of the ladder: `iwram-tw` (157%) and `iwram-tables` (148%) were placement arms, **C-3d** made the timed frame int8-only (**138%**), and **C-3** moved `HotState` to IWRAM (**−87,040**) to **274,198 / 232,090 (105% / 89%)**. **Step 0 (2026-11-15) then attributed that frame by difference in the product image:** at the worst frame (f98) synthesis is 162,987 (59%) and entropy/dequant/M/S 110,741 (40%, wrapper 486), so **69% of the 92,286-cycle distance to the C worst-frame shape is the entropy path** and 31% synthesis; per channel the kernels are 1.10–1.33x C, with the already-asm FFT (41,671) the largest kernel item and `pre`+`post`+`ola` worth 9,817/frame together. The previous "C-1 kernel prize ~194,000, the largest single item" is retired. **What remains is ordered in "Critical path to real time": entropy/dequant sub-frame attribution and rewrite (the gate-critical item), then FFT v5, then `pre`/`ola`/`post` asm, then PR E.** See the ledger's "Step 0 attribution". |
| **U5 — product playback** | 32,768 Hz path, zero underruns, clean period audit. | Not started (gated by U4). |
| **U6 — memory** | IWRAM/EWRAM/stack inside the product budget with UI and feeder. | **Open.** ~~The product path needs ≈15.0 KiB of stack.~~ **Corrected 2026-11-15: the measured stack high-water is 768 B** (sentinel probe in `examples/uli_bench`); the ≈15.0 KiB figure was an artifact of stack-local twin buffers and is withdrawn. That reverses this row's conclusion: `iwram-tables` (all synthesis tables in IWRAM, C's own layout) **now runs** — `.iwram` 25,548 B, 6,964 B free, 768 B stack used, **6,196 B headroom** — and measures the product frame at **148%** (387,879 / 340,929). See "Memory layout — mirrored from the C reference". Remaining placement item is C-3 (`HotState` → IWRAM, 8,192 B), which fits once the superseded v1/v2 kernels are dropped (1,520 B). Historical note (superseded): the row previously concluded IWRAM tables were blocked because a stale ≈15.0 KiB stack left no room and `iwram-tables` crashed. **Priority decided 2026-11-14 and still in force: audio first** — IWRAM budget goes to code, hot state and stack; UI affordances are the slack and may be dropped to buy audio headroom. Full `huff` inlining was rejected on this row: +4,252 B for 2.7%, vs +896 B for 1.6% with the split. |
| **U7 — licensing/spec** | Permissive license + standalone format spec before publishing; otherwise `publish = false`. | **Deferred by decision 2026-11-14: `publish = false`, no license work now.** Revisit only if a publish decision is taken; the license and `FORMAT.md` remain prerequisites for it. |

---

## Measurement ledger

### Phase 1b baseline (2026-11-13, mGBA, WAITCNT 4/2)

Worst / mean frame, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean |
|---|---:|---:|
| portable core, Thumb-1, ROM | 6,622,160 (2526%) | 5,897,421 (2250%) |
| core kernels ARM-mode, ROM | 2,970,042 (1133%) | 2,245,992 (857%) |
| overlay kernels ARM/IWRAM (projection) | 2,122,748 (810%) | 1,398,699 (534%) |
| C/asm reference (`gba-audio`) | ~182,000 (69.4%) | ~153,000 (58.4%) |

Kernel sequence per channel: core Thumb/ROM 2,513,421; core ARM/ROM 667,263;
overlay ARM/IWRAM 243,617; overlay + IWRAM tables 228,313; C/asm ~67,300.
The projection is now replaced by PR A's measured full frame.

### PR A (2026-11-14)

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,676,986 (1021%) | 2,171,620 (828%) | 1,372,892 |
| overlay kernels (`ArmKernels`) | 1,829,247 (698%) | 1,322,800 (505%) | 523,102 |

Both arms: pins `e6e87652` / `b3530978`, `xover=0 hfnz=0`, no wrapped window;
calibration overhead 38 cycles (spread 3). Kernel sequence per channel:
core 667,264; overlay 243,619. Core full-frame rest (non-synthesis):
837,092 cycles; the overlay frame equals that plus 2 × 243,619 within 0.02%.
The on-target overlay twin passed for all four kernels.

Recorded consequence: PR A is a **measured** full-frame product kernel path at
~7x the period, down from the ~11x core arm in the same image and the 25x
Phase 1b starting point. It is not yet a product decoder: the bit reader,
Huffman, dequant and M/S are still the portable Thumb/ROM code, and they are
the 837,092-cycle rest that PR B attacks.

### PR B (2026-11-14)

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path (`GbaDecoder`)** | **533,605 (204%)** | **463,660 (177%)** | 351,556 |

All arms pinned, `xover=0 hfnz=0`, on-target twin passed. Overlay
seq/channel 169,061; **`rest` = 195,484 cycles/frame (from 837,092 at PR A),
so synthesis is back to 63% of the frame.** The PR B gate (≤2×) is met at
204%. The remaining gap to the C/asm reference is now the kernels: 169,061 vs
~67,300 per channel (2.51×), with OLA the outlier.

Placement: `HotState` (8 KiB) is in EWRAM, not IWRAM — an IWRAM placement
stack-overflowed on target (≈15.0 KiB of stack needed; see PR B above). The
host twin was green either way, so this was a target placement defect only.

### PR C-3b (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period, same-environment baseline.

| arm | worst | mean | min |
|---|---:|---:|---:|
| C-3a + C-3c (before) | 488,685 (186%) | 429,339 (164%) | 353,340 |
| **C-3b product build** (`verify` off) | **457,863 (175%)** | **410,912 (157%)** | 352,940 |
| C-3b `verify` build (counters on) | 488,682 (186%) | 429,336 (164%) | 353,337 |
| core kernels (control, same image) | 2,643,897 (1009%) | 2,152,958 (821%) | 1,376,966 |
| overlay kernels (`ArmKernels`) | 1,640,505 (626%) | 1,148,486 (438%) | 371,523 |

**−30,822 worst / −18,427 mean**, against a prediction of −30,482 / ~20,000. The
`verify` build reproduces the previous arm to within 3 cycles (488,682 vs
488,685), which is the layout noise this file records elsewhere — so the feature
is a switch, not a re-implementation. Both builds pin `e6e87652` / `b3530978`,
`xover=0 hfnz=0`, no wrapped window, overlay twin PASS.

Design: `uli-lite-gba`'s `verify` feature gates the per-symbol counters (a
no-op call per increment otherwise) and the two diagnostic compares per nonzero
coefficient; `Diag` is empty and `FrameInfo::ops/cycles/xover/hfnz` are zeroed in
the product build. The format rules are **not** unwitnessed: `examples/uli_bench`
reads `xover/hfnz` from the core decoder's correctness pass, and
`tests/product_twin.rs` compares full `FrameInfo` under `--features verify` plus
a samples/bits/bitpos twin without it; `make uli-overlay-test` runs both. A guard
test fails if anything re-enables the counters outside the gate.

### PR C-2 / C-3a (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period. The baseline was rebuilt
from `main` in this environment immediately before the change (its numbers
reproduce PR C's record exactly, which is what makes the arms comparable).

| arm | worst | mean | min |
|---|---:|---:|---:|
| baseline (PR C, `main`) | 523,267 (200%) | 456,187 (174%) | 351,446 |
| **C-3a + C-3c** | **488,685 (186%)** | **429,339 (164%)** | 353,340 |
| core kernels (control, same image) | 2,643,897 (1009%) | 2,152,958 (821%) | 1,376,966 |
| overlay kernels (`ArmKernels`) | 1,640,505 (626%) | 1,148,486 (438%) | 371,523 |

All arms pinned `e6e87652` / `b3530978`, `xover=0 hfnz=0`, no wrapped window,
overlay twin PASS; calibration overhead 35 cycles, spread 0. The worst frame
moved 101 → 106, so the **mean is the comparable number** (−5.9%); worst is
−6.6%. The controls moved too (1009% vs PR C's 1023%), which is the recorded
image-specificity effect, not a kernel change.

**What landed**: reader as a `Copy` local over a stream cursor with the bit
position *derived* rather than maintained; `huff` returning the packed
`(sym << 5) | len` already consumed; the inline `clz6` escape path; `q3` and
`clz6` generated in-repo; `rq`/`dq15` precomputed at album parse; coefficients
written straight into `x` with no pre-zero pass and an EOB tail fill; M/S
bounded by `nb`; and C-3c's 32-bit dequant (approved: no arithmetic promise on
malformed streams). Host twins, the in-file bit-reader fuzz (4,000 random
windows × 64 steps) and the on-target pins are all green.

**Measured decomposition of what is left** — each arm built and run on target,
same image, varying only the counters and diagnostics:

| variant | worst | mean | vs landed |
|---|---:|---:|---:|
| C-3a + C-3c (landed) | 488,685 | 429,339 | — |
| …counters hoisted into locals | 509,888 | 439,098 | **worse, +21,000** |
| …`Ops` counters removed (C-3b) | 477,370 | 422,173 | −11,315 |
| …`Ops` and `Diag` removed | 458,203 | 411,253 | −30,482 |

Two findings worth keeping. **Hoisting the counters into locals costs 21,000
cycles** — register pressure displaces the reader, the same failure mode that
made PR C's dequant fusion neutral; recorded so it is not attempted again. And
**C-3b is worth ~11,000 alone, ~30,000 with the diagnostics** — the largest
measured lever left, and still open because gating it behind `verify` changes
the twin's comparison basis and the bench's on-target `xover/hfnz` witness.

Entropy + dequant is now ~140,000 cycles/frame against the C record's ~45,000
(≈5.3× per coefficient, down from 6.6×). **The structural port did not close
that ratio**, so the next entropy step has to be a per-stage attribution at
sub-frame granularity (PR C's by-difference method one level down), not another
structural guess.

### PR C-2 attribution: where the remaining ~140,000 cycles go (2026-11-14)

Two instruments, both run rather than reasoned about:

1. **Per-frame dump.** `examples/uli_bench --features per-frame` prints one row
   per frame with the measured window and the decoder's operation counts
   (`make uli-bench` is unchanged; the feature is off by default). The dump
   build reproduces the gate arm exactly — worst 488,685, sum 55,384,726 — so
   the instrument does not perturb what it measures.
2. **By-difference arms.** Each rebuild removes exactly one part and re-runs the
   bench. These are **timing-only**: the pins fail by construction (their output
   is no longer the album), and only the window is quoted.

Fitting the 129 dumped rows with a per-event model

    cycles = c + Σ (events × cycles_per_event)

explains the corpus at **R² = 0.9994** with a residual sd of **1,170 cycles**.
Per-event costs (cycles, ± the fit's standard error): **pair symbol ≈ 302 ± 19**,
**quad symbol ≈ 341 ± 23**, **escape ≈ 134 ± 9**, **raw field ≈ 57 ± 4**,
**long code ≈ 221 ± 16**, **M/S frame ≈ 18,124 ± 440**, nonzero-over-zero
coefficient ≈ +25 ± 9. The band-symbol term is **not identifiable** and must not
be quoted: `coded = pairs/8 + quads/4` exactly (`nz + zc = 16 × coded`), so the
band term is a collinear proxy — it fits negative, which is the tell that it is
absorbing something else, not that zero bands are free.

By-difference arms against the landed arm (worst / mean):

| part removed | worst | mean | Δ worst | Δ mean |
|---|---:|---:|---:|---:|
| — (landed C-3a + C-3c) | 488,685 | 429,339 | — | — |
| M/S loop | 472,618 | 419,852 | 16,067 | 9,487 |
| dequant arithmetic | 465,655 | 414,434 | 23,030 | 14,905 |
| zero-band + EOB fills | 486,874 | 421,937 | 1,811 | 7,402 |
| `Ops` + `Diag` (measured earlier) | 458,203 | 411,253 | 30,482 | ~20,000 |

**What that says, and the harvest order it implies:**

1. **The diagnostics are the biggest single item: ~30,000 cycles.** They are two
   compares plus a counter per coefficient/symbol — work the C product build
   does not do at all. **C-3b is therefore 3× the harvest of the M/S loop.**
2. **M/S costs 16,067 but its loop shape is not the problem.** Rewriting it as
   four-coefficient blocks — the unit the reference's `uli_ms_asm` moves — was
   built and measured: it recovers only 1,776 worst / 896 mean. The cost is
   **memory traffic**: our `HotState` (and therefore `x`) is in EWRAM, while the
   reference's `uli_g_X`/`uli_g_P` live in IWRAM (`uli_gba_dec.iwram.c`), where
   the same M/S is ~4,400. Every coefficient written or transformed pays 16-bit
   EWRAM wait states. **The lever is coefficient buffers in IWRAM**, which is
   blocked on the ≈15.0 KiB stack — and C-1's asm is what removes the spills that
   need it. This is now the second-largest identified item and it is *not* an
   entropy rewrite.
3. **The dequant arithmetic is only ~4,000.** Of the 23,030 in its row, ~19,200
   is the `xover` diagnostic compare that the `Ops`+`Diag` arm also captures. The
   32-bit `rq` path is already near its floor; do not spend more here.
4. **Fills cost ~7,400 mean and are not removable.** Zero bands and the post-EOB
   tail must be written because `pre`/`post` read all `M` coefficients. They do
   set the floor: sparse frames drop from 353,340 to 325,109 without them.
5. **Per event, escapes (134) and long codes (221) are the most expensive things
   after the symbol itself** — 74 escapes and 8 long codes in the mean frame,
   ≈11,700 cycles. Worth a look once 1 and 2 are done; not before.

**Conclusion.** The huff/field path is ~200 cycles per symbol, which is the same
order as the C record's whole non-synthesis budget per symbol. The remaining gap
is therefore **not** an unexplained per-symbol inefficiency: it is diagnostics
(~30,000) plus **EWRAM traffic on the coefficient buffers**, and both of those
are understood. Re-based: C-3b alone puts the frame at ~458,000 (175%); C-3b plus
IWRAM coefficient buffers is the path to ~170% before C-1 lands, after which the
kernel prize (~194,000) is what closes the budget.

### PR C-1 probe: hand-written ARM FFT, measured (2026-11-14, mGBA, WAITCNT 4/2)

**This is the keystone experiment the C-1 projection rested on, and it does NOT
confirm the projection.** A complete hand-written radix-4 DIF FFT
(`crates/uli-lite-gba/asm/fft_arm.s`, ARM mode, `.iwram`, `global_asm!`) was
written against our own oracle, twinned bit-exactly, and measured in isolation
against the Rust ARM-mode `fft` in the same image. Instrument:
`examples/uli_asm_probe` (on-target twin) and the bench's `asmfft` arm
(`make uli-bench` with `--features asm-fft`).

| arm | cycles / FFT call | vs Rust | vs C record (~31,400) |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (`overlay`, control) | **85,065** | 100% | 2.71× |
| **hand-written asm FFT** | **88,205** | **103.7%** | **2.81×** |
| C/asm reference (per channel) | ~31,400 | — | 1× |

Correctness: the asm FFT is **bit-exact** with the reference on the ARM7TDMI
(`[asm-probe] FFT matches the reference`; bench `fftasm=true` on a 512-sample
buffer and across the corpus). It assembles, links into `.iwram`, and its own
frame is 84 B (push + 48-byte frame).

**Design, as the allocation PR C said Rust could not express:** t0..t3 live in
r4–r11 (8 registers held across all four outputs), one twiddle pair at a time in
r2/r3, SMULL/SMLAL scratch in r8/r9, x base in r12; the stage bookkeeping and
out2's two sums (t0, t2) spill to a 48-byte stack frame. The inner butterfly
emits **exactly 6 SMULL + 6 SMLAL** (the irreducible complex-multiply count)
and **no `__aeabi_lmul`** — the same codegen quality PR C recorded for the Rust
version, now guaranteed by hand.

**The finding: hand asm did not move the FFT.** ~3.7% *slower* than the Rust ARM
kernel, still ~2.8× the C record. The register-pressure hypothesis ("LLVM spills
66 intermediates; a hand allocation will recover the gap") is **falsified as the
primary lever**: the same arithmetic with a hand allocation lands in the same
place. Per-butterfly it is 12 multiplies + ~40 LDR + ~17 STR — the memory
traffic (twiddle reloads and the t0/t2 stack spill per output) is what remains,
and the 12-multiply core is an arithmetic floor of roughly ~30–38k cycles for
64 butterflies at ~4–5 cycles per SMULL/SMLAL, i.e. **the C number is near the
multiply floor, not a register-allocation gap we can close for free.**

**Consequence for the plan, stated before the follow-up work:** C-1's ~194,000
prize is **not** available from FFT asm as scoped. Either (a) the remaining FFT
gap is paid by cutting the memory traffic toward the multiply floor (fewer stack
reloads, twiddles kept resident, possibly unrolling the whole stage) — a
smaller, different prize than ~194k — or (b) the 63%-synthesis half of the
parity plan does not close and the target must be re-based. The **stack** half
of C-1's premise is also not delivered by this kernel: its own frame is 84 B,
but the ~15.0 KiB product-path stack that blocks IWRAM coefficient buffers is
not addressed by the FFT, so C-3's "coefficient buffers to IWRAM" stays blocked
until the real stack consumers are found. Recorded here so the ~73–81% parity
row is not read as reachable by asm alone.

Feature gating: `asm-fft` (off by default). `ArmKernels::fft` uses the asm when
the feature is on; `overlay::fft` (the Rust kernel) stays the control and every
other arm keeps its baseline in the same image. The product decoder
(`GbaDecoder`) does not route through `ArmKernels`, so `prod` is unchanged in
the `asm-fft` build — a separate wiring fact, not a result.

#### PR C-1 traffic follow-up: the memory-traffic hypothesis, measured

Because v1 landed at the same cost as the Rust kernel, the residual was
identified as **memory traffic**, not spills. A second kernel (`fft_arm_v2.s`)
removes exactly that traffic and nothing else — no t-value is spilled, `p` and
the twiddle base stay resident, and the loop-invariant stride/bound stop being
reloaded per butterfly — and was measured in the same image, bit-exact.

| arm | cycles / FFT call | % of Rust | × C record (~31,400) |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (control) | 85,065 | 100% | 2.71× |
| asm v1 (spill-heavy) | 88,205 | 103.7% | 2.81× |
| **asm v2 (traffic-reduced + TWJ hoist + stride/bound in-place)** | **73,363** | **86.2%** | **2.34×** |

**v2 is 16.8% faster than v1 and 13.8% faster than the Rust ARM kernel**, bit-
exact on the ARM7TDMI (probe `v2 matches`; bench twin `fftasm=true` for both
arms). The experiment therefore **confirms the memory-traffic hypothesis as a
real lever**: the ~15,000 cycles/call v1 lost to stack spill and reload are a
true cost, and a hand allocation that deletes them pays. The kernel is still
2.3× the C record, so the FFT is **not** closed by this alone, but the gap is
now demonstrably traffic-bound and stepping down monotonically:
85,065 → 88,205 (spill) → **73,363** (traffic removed).

The remaining ~42,000 over the C record is the multiply core plus the
irreducible load/store of A..D (8 loads) and the four outputs (8 stores) plus
six twiddle words per butterfly. Whether that can be cut further is the next
question; the instrument (probe + bench arms, same image, same twin) is in place
to answer it incrementally.

#### PR C-1 traffic follow-up 2: block transfers, and why our traffic > C's

A third bit-exact kernel (`fft_arm_v3.s`) converts the general-stage loads and
the out0 store to `ldmia`/`stmia` (2-word block transfers) — the structural
difference the C reference has and we did not. Measured in the same image:

| arm | cycles / FFT call | % of Rust | × C record |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (control) | 85,065 | 100% | 2.71× |
| asm v1 (spill-heavy) | 88,205 | 103.7% | 2.81× |
| asm v2 (traffic-reduced) | 73,363 | 86.2% | 2.34× |
| **asm v3 (v2 + `ldmia`/`stmia`)** | **71,635** | **84.2%** | **2.28×** |

**Why our memory traffic was worse than the C reference** — read from
`../../gba-audio/src/uli/gba/uli_imdct_arm.S` (analysis only; nothing copied into
this tree, the asm here is written against our own oracle):

1. **Block transfers.** C loads the complex operands with `ldmia r0,{r3,r4}` /
   `add …; ldmia …` — two words per instruction — and stores y0 with `stmia`.
   v1/v2 emitted **separate `ldr`/`str` per word** (16 load/store instructions
   per butterfly vs C's ~8). On a 4-cycle-fetch ARM7TDMI that halves the fetch
   cost of the operand traffic. This is v3, and it bought **−2.4%** on its own.
2. **A trivial-butterfly path.** C runs `TRIV_BFLY_P` for the identity-twiddle
   butterfly (j == 0 of each stage, where `TW[0..6] = (2^30,0)*3`, i.e. exactly
   1.0 in Q30): pure adds + `asr #2`, **no multiplies and no twiddle loads**.
   We still run the general path there. This is ~1.5% of the butterflies (3 of
   192 radix-4) — a small, not-yet-taken win.
3. **Macro-fused form-y.** C's `FORM_Y_P` computes the four t-sums, does y0's
   `asr #2` and store inline, and reuses registers so the sums are never
   spilled — the same idea v2 implements by hand (the outputs overwrite the
   t-registers they consume). v2 matches this structurally.

The ladder — one lever at a time, same image, all bit-exact: **85,065 (Rust) →
88,205 (v1 spill) → 73,363 (v2 traffic) → 71,635 (v3 block transfers)**, i.e.
v1→v3 is **−18.8%** and v3 is **−15.8% vs the Rust kernel**. The two structural
differences from C are now closed (block transfers) or quantified (trivial
path ~1.5%); the residual to the C *record* is the multiply core plus the
remaining genuine A..D loads / output stores / twiddle loads, and that record
was measured on a different corpus and image, so `2.28×` is not a like-for-like
statement — it is the honest gap to the number of record.

Instruments: `examples/uli_asm_probe` (all three kernels twinned on the
ARM7TDMI) and the bench's `asmfft`/`asmfft2`/`asmfft3` arms + `fftasm` twin.
Note for future arms: the twin's scratch buffers had to move from `run()`'s
stack to EWRAM statics — three 512-word kernels plus the ~15 KiB product-path
stack overflowed `.iwram` once the asm grew it (the same stack class PR B and
the `iwram-tables` crash recorded).

#### PR C-1 landed: wired into the product, and where to look next

The kernels were then wired into the **product hot path**: `GbaDecoder`'s
synthesis calls `crate::fft_kernel`, which dispatches to the best asm variant
(v4 since 2026-11-15) when `asm-fft` is on (the `#[cfg]` lives in one helper).
Before this, the asm only reached `ArmKernels`, so `prod` was unchanged; now it
moves.

| arm | worst | mean | of period (worst) |
|---|---:|---:|---:|
| product frame, Rust FFT (`asm-fft` off) | 457,863 | 410,912 | 175% |
| product frame, asm FFT v3 (`asm-fft` on) | 430,997 | 384,046 | 164% |
| **product frame, asm FFT v4 (+ trivial butterfly)** | **425,795** | **378,845** | **162%** |

**v3: −26,866 / −26,866 (−5.9% / −6.5%)**; **v4: a further −5,202 / −5,201**,
i.e. **−32,068 / −32,067 vs the Rust-FFT product build (−7.0% / −7.8%)**. Both
pins `e6e87652` / `b3530978` PASS, `xover=0 hfnz=0`, on-target twin PASS. The
product-frame delta is exactly `2 ×` the per-FFT saving (two channels). Ladder
now **2526% → … → 175% → 164% → 162%** worst (145% mean). U4 (≤100%) is still
not met.

**Why this Rust+asm stack still loses to C+asm, and the research order to close
it** (2.20× the C record's ~31,400 per FFT — and, since C's number is from a
different album/image, the sub-goal is to get under 31,400 *in this tree's
instrument*, not to match a foreign percentage):

1. **~~Trivial-butterfly path~~ — DONE as v4 (`fft_arm_v4.s`), 2026-11-15.**
   C's `TRIV_BFLY_P` handles the identity twiddle (j==0 of each stage, where
   `TW[base..base+6] = (2^30,0)*3`, i.e. 1.0 in Q30, verified for l=256/64/16,
   and `cmul(y,1.0) == y>>2`) with pure adds + `asr #2`: no multiplies, no
   twiddle loads. v4 takes that path and measures **69,034 cycles/call**, i.e.
   **−2,601 (−3.6%) vs v3** — larger than the ~1,000 projected, because the path
   also skips the per-output twiddle-pointer load and the six twiddle loads.
   It covers `1 + 4 + 16 = 21` of the 192 radix-4 butterflies (an earlier note
   said 3/192 by counting j-iterations, not butterflies). **This closes the last
   structural difference to C.**
2. **~~Twiddle loads~~ — RESOLVED 2026-11-15, and not by `ldmia`.** The
   `ldr`->`ldmia` idea was worth ~1-2%. The real cost was that `TW` sat in
   cartridge ROM (`.rodata` @ `0x0800d640`) while C keeps tables in IWRAM: a
   same-image A/B measures **−10.4%** for relocating the table, delivered by the
   opt-in `iwram-tw` feature (see "follow-up 3" below). With the table in IWRAM
   the read is ~1 cycle, so `ldmia` would now save only instruction count (~1%)
   — still worth one measured arm, but it is no longer where the money is.
3. **The multiply core is the floor.** 12 SMULL/SMLAL per butterfly (6+6) ×
   192 radix-4 + tail is ~the C kernel's own arithmetic. If 1 and 2 land and
   the FFT is still over, the gap is the per-butterfly load/store of A..D (4
   `ldmia`), the four output stores, and the twiddle loads — the genuinely
   irreducible memory traffic — not codegen.

4. **Corpus/image.** PR E's worst-frame program (deferred) is what makes the
   gap to C like-for-like. Until then every `×` is against a foreign number.

**C-3d LANDED 2026-11-15 — the OLA outlier was mostly a measurement artifact.**
`ola` fused the format selection per sample and computed **both** int8 and int16
whenever the caller passed an int16 buffer, and the bench passed one (to pin both
hashes) — so the arm the plan was reading (34,780/call, 2.27x C) was a
configuration the C record never timed. C ships one kernel per format
(`uli_ola_s8_asm` / `uli_ola_s8w_asm` / `uli_ola_s16_asm`) and selects the format
outside the loop.

Two fused kernels now do the same (`ola_s8`, `ola_s16`, format fixed at compile
time, no per-sample branch, no arithmetic for the format not being written), and
`ola_kernel` resolves the `Option` once so the product path gets the int8 kernel
when it has no int16 buffer. Measured, same image, same kernel family:

| OLA arm | cycles/call | vs C (15,300) |
|---|---:|---:|
| dual (what the plan was reading) | 34,780 | 2.27x |
| **s8 — the production headphone path** | **21,418** | **1.40x** |
| s16 — pre-DAC profile | 23,220 | 1.52x |

**OLA's headroom against C shrank ~69%** (19,480 -> 6,118 per channel), so OLA
asm is now a small item, not the second-biggest kernel prize the earlier table
suggested. The earlier `2.28x` row is superseded.

**And it is a real frame win, not just a fairer measurement.** The product was
writing int16 it never uses: an int8-only product pass (`decode_frame(.., None,
None)`, fused OLA, identical int8 pin `e6e87652`) measures

| product frame | worst | mean | of period |
|---|---:|---:|---:|
| dual output (previous default) | 387,878 | 340,928 | 148% |
| **int8-only (production)** | **361,238** | **314,288** | **138%** |

**-26,640 worst / -26,640 mean (-6.9%)**, `1.99x` the C decode wall. Ladder:
2526% -> ... -> 175% -> 148% -> **138%**. The int16 profile keeps its own arm.

**C-3 LANDED 2026-11-15 — `HotState` in IWRAM: the largest single win so far.**
The decoder's hot working set (`x` 4,096 + `buf` 2,048 + `p` 2,048 = **8,192 B**,
the same three buffers C keeps as `uli_g_X`/`uli_g_buf`/`uli_g_P` in IWRAM) moved
from EWRAM to IWRAM. EWRAM charges 16-bit wait states on a 32-bit bus, so every
coefficient write, every M/S read/write, and every pre/post/OLA pass over the
buffers was paying ~5 extra cycles per access.

| product frame | worst | mean | of period | vs C |
|---|---:|---:|---:|---:|
| before C-3 (int8-only) | 361,238 | 314,288 | 138% | 1.99x |
| **after C-3 (int8-only, production)** | **274,198** | **232,090** | **105%** | **1.51x** |
| after C-3 (dual output arm) | 300,838 | — | 115% | 1.65x |

**−87,040 cycles/frame (−24.1%)**, both pins `e6e87652` / `b3530978` PASS, all
twins PASS. **The plan projected 10,000–15,000 for this item; it delivered
87,040 — about 6x** — because the projection counted only the M/S pass (16,067)
while the win captures *all* coefficient traffic through `x`/`buf`/`p`. Mean is
now **under** the period (89%); worst is **12,054 cycles (4.6%) from real time**.

**How it was paid for in IWRAM** (the binding constraint). `.iwram` went from
22,436 B to 28,852 B of 32,512. Two principled savings made the 8,192 B fit:
1. **The Rust `fft` moved to ROM** when `asm-fft` is on (1,056 B). It is not on
   the product path — `fft_kernel` dispatches to v4 — so IWRAM should not pay for
   it. With `--no-default-features` it *is* the product kernel and stays in
   IWRAM; the placement is gated on the feature.
2. **v3 moved to ROM** (708 B): superseded by v4 (v4 = v3 + trivial butterfly).
   v1/v2 went the same way in the same change. All three remain callable
   comparison arms; their recorded numbers were taken in `.iwram`.

Stack: usable from `0x03007F00` is 3,660 B, high-water ≈2,960 B, leaving **~700 B**
— usable for this image, below C's own ~1 KiB assert, and the real product ROM
(UI + feeder) must re-check it before shipping. `HotState` size is pinned by a
compile-time assert so the budget cannot drift silently.

**PLANNED NEXT — `pre`/`post`/`ola` as asm (the rest of C-1), 2026-11-15.**
*(Live version, with the step-0 re-measurement caveat, is the "Critical path to
real time" section at the top of this file. The numbers below predate C-3 —
buffers were still in EWRAM — so they bound the ratios, not today's absolutes.)*
The FFT is now near its multiply floor; the next synthesis target is the *other
three kernels*, which are still the **Rust ARM-mode** kernels. Measured per channel
with tables in IWRAM, against the C record (`ovl` arm of `uli_bench`):

| kernel | ours (Rust ARM) | C/asm record | ratio | headroom vs C |
|---|---:|---:|---:|---:|
| pre | 17,654 | 9,700 | 1.82× | 7,954 |
| fft (v4, asm) | 61,852 | 31,400 | 1.97× | 30,452 |
| post | 16,010 | 10,900 | **1.47×** | 5,110 |
| ola (dual, superseded) | 34,780 | 15,300 | 2.27× | 19,480 |
| **ola (s8, production)** | **21,418** | **15,300** | **1.40×** | **6,118** |
| pair | 260,797 | 134,600 | 1.94× | 126,197 |

**OLA's contribution to this list is much smaller than it first looked** (C-3d,
below: the production int8 kernel is 1.40x C, not 2.28x). What remains of the
list is `pre` (1.82x) and `post` (1.47x), which together are the larger half
now. Historical note on the ordering:
1. ~~**C-3d first (measurement fairness)**~~ — **DONE 2026-11-15**, above: the
   format is now selected outside the loop and the production arm is 1.40× C.
2. **Then OLA asm** on the *s8* kernel — but note the prize is now ~6,100/channel
   (~12,200/frame), not the ~39,000 the dual-output arm implied, so `pre`/`post`
   are the larger half of what is left.

`pre` and `post` follow with one asm kernel each (a twiddle-table walk with a
single `SMULL`/`SMLAL` pair per output — structurally simpler than the FFT, so
they should come cheap). Together the three are now **~38,000 cycles/frame** of
identified headroom per the pre-C-3 kernel table, and `pre` is the largest single
item. **Note after C-3 (105%):** only **12,054 cycles** separate the production
frame from the period, so the *next* synthesis arm is not chosen by kernel ratio
any more — `pre` asm alone (~16,000/frame) would clear the period if it lands at
C's number. That makes `pre` the single most valuable next kernel.

#### PR C-1 follow-up 3: the twiddle table is the real "twiddle load" cost

**The planned item was `ldr` -> `ldmia` for the three twiddle pairs (~1-2%). The
measurement found the dominant cost is PLACEMENT, not instruction count.** With
`iwram-tables` off, `TW` is a plain `const` and therefore lives in `.rodata` —
which the linker map puts at `0x0800d640`, i.e. **cartridge ROM**. Every
twiddle load in the hot butterfly was therefore a slow ROM read (~6 cycles at
WAITCNT 4/2 with no prefetch), the same class of problem as ROM-resident code.
The C reference keeps its tables in IWRAM (~9.4 KB); we did not.

Measured as a **same-image A/B** (one build, one kernel, only the table pointer
differs — `fft_v4_tw(x, tw)` timed against the ROM `TW` and against an `.iwram`
copy):

| arm (v4, 69,034 with ROM twiddles) | cycles/call | vs ROM |
|---|---:|---:|
| twiddle table in `.rodata` (ROM) | 69,034 | — |
| **twiddle table in `.iwram`** | **61,852** | **−7,182 (−10.4%)** |

That is **~6.2 cycles saved per twiddle load**, exactly the ROM wait-state cost
over ~1,152 loads/call — the mechanism confirms itself. Product frame, with the
new `iwram-tw` feature moving `crate::TW` into `.iwram` (the product path picks
it up through the same `tw` argument, no asm change):

| arm | worst | mean | of period (worst) |
|---|---:|---:|---:|
| product, v4, TW in ROM (`asm-fft`) | 425,795 | 378,845 | 162% |
| **product, v4, TW in IWRAM (`asm-fft,iwram-tw`)** | **411,431** | **364,481** | **157%** |

**−14,364 worst / −14,364 mean**, exactly `2 ×` the per-FFT saving (two
channels). The FFT is now **61,852 vs the C record's ~31,400 = 1.97x** — under
2x for the first time. Ladder: 85,065 (Rust) → 88,205 → 73,363 → 71,635 →
69,034 → **61,852** (IWRAM twiddles).

**Budget, stated because it is the binding constraint.** `iwram-tw` costs
**2,016 B** (TW is 504 words). `.iwram` goes 14,860 → 16,876 B of 32,512,
leaving **~600 B** against the recorded ~15 KiB product stack — viable in the
bench image (it runs, all twins/pins PASS) but **not** comfortably safe for the
full product ROM, which adds UI and a feeder. Two ways to pay for it, both
deferred to the U6 budget decision: drop the superseded v1/v2 kernels from the
unit (frees 1,520 B; their numbers are recorded here), or accept a smaller
stack margin. The feature is therefore **opt-in**, exactly like `iwram-tables`.

**Consequence for the research order.** Item 2 was "twiddle `ldmia`". With the
table in IWRAM the read cost collapses, so `ldmia` would now save only the
instruction-count difference (~1%), not a wait-state penalty — it is worth
measuring once, but the 10.4% was in the table's location. This also revises
the "why are we slower than C" answer: **a large part of it was table
placement**, not kernel codegen. The remaining gap is now the multiply core,
the A..D/output traffic, and a like-for-like corpus (PR E).

### PR C (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path, split `huff`** | **523,267 (200%)** | **456,187 (174%)** | 351,446 |

All arms pinned, `xover=0 hfnz=0`, on-target twin passed. PR B was 533,605 /
463,660; the split-`huff` change is the 1.6% in between (IWRAM +896 B).

Frame-101 attribution of the 523,267 (by difference, one image): kernels
328,502 (63%), `dec_chan` × 2 ≈175,000 (33%), M/S + wrapper ≈20,000 (4%).
Kernels against the C record's 134,600 are 2.44×; entropy per
coefficient against the C bench's entropy+dequant (~41 c/coef) is ~6.6×.

### Step 0 attribution — the product frame by difference (2026-11-15, mGBA, WAITCNT 4/2)

Instrument: `make uli-attrib` (target added with this measurement). Nine builds,
one image each, `--features attrib-dump` on every arm so the per-frame windows
can be differenced at the same frame index. The arms are **timing-only** cargo
features on `uli-lite-gba` that remove exactly one part of `GbaDecoder`'s frame;
the album pins fail by construction in all of them and no pin is quoted from
this section. The dump costs a constant **+16 cycles per window** (control
274,214 vs the product arm's 274,198; Σ = 16 × 129), which is why every absolute
below is the product arm's and every delta is control-minus-arm.

**Control (this instrument, 129 frames):** 274,214 worst at frame 98 /
232,106 mean; the product arm in the same tree without the dump is **274,198 /
232,090 (105% / 89%)** — the recorded number, reproduced exactly. Nothing in the
instrumentation perturbed the frame.

| arm (part removed) | worst | mean | Δmean | Δ f98 |
|---|---:|---:|---:|---:|
| — control | 274,214 | 232,106 | — | — |
| `attrib-synth` (all 8 kernel calls) | 112,860 (f101) | 71,838 | 160,268 | 162,987 |
| `attrib-pre` | 249,383 (f101) | 208,359 | 23,747 | 26,464 |
| `attrib-fft` | 193,180 (f98) | 148,765 | 83,341 | 81,034 |
| `attrib-post` | 251,836 (f98) | 208,224 | 23,882 | 22,378 |
| `attrib-ola` | 239,827 (f98) | 198,118 | 33,988 | 34,387 |
| `attrib-ms` | 266,252 (f0) | 227,692 | 4,413 | 9,619 |
| `attrib-entropy` (not quotable, see below) | 156,277 | 156,016 | 76,090 | 118,193 |
| `attrib-synth,attrib-entropy` | 742 | 481 | 231,625 | 273,728 |

**Decomposition (product frame):**

| part | mean | share | f98 | share |
|---|---:|---:|---:|---:|
| synthesis, 8 calls | 160,268 | 69.0% | 162,987 | 59.4% |
| └ `fft` (asm v4) | 83,341 | 35.9% | 81,034 | 29.6% |
| └ `ola` (Rust, fused s8) | 33,988 | 14.6% | 34,387 | 12.5% |
| └ `post` (Rust) | 23,882 | 10.3% | 22,378 | 8.2% |
| └ `pre` (Rust) | 23,747 | 10.2% | 26,464 | 9.7% |
| entropy + dequant | 66,943 | 28.8% | 101,122 | 36.9% |
| M/S | 4,413 | 1.9% | 9,619 | 3.5% |
| wrapper / glue | 481 | 0.2% | 486 | 0.2% |

**Why each number is quoted the way it is:**

- **Synthesis** is `control − attrib-synth` (the sequence, not the sum of the
  singles). The singles sum to 164,958 mean / 164,263 at f98, i.e. +2.9% / +0.8%
  over the sequence; the interaction is the FFT's data-dependent trivial-butterfly
  path, whose input changes when `pre` is removed.
- **Entropy + dequant** is `attrib-synth − (attrib-synth,attrib-entropy)`:
  71,838 − 481 = 66,943 mean. `attrib-entropy` on its own is **not quotable** —
  removing `dec_chan` freezes the bit position, so every frame re-synthesises one
  stale frame (156,016 mean against the 160,749 = synthesis + wrapper the arm
  should measure), and the 6.6× figure in the PR C section above is superseded by
  this one.
- **M/S** is `control − attrib-ms`. It is ~C-class in the mean (4,413 vs the
  record's ~4,400) and 9,619 on the dense worst frame, so the earlier "16,067 for
  the M/S loop" (PR C-2, pre-C-3, EWRAM `x`) is retired: what that arm measured
  was mostly EWRAM traffic, and C-3 fixed it.
- **Wrapper** is the `synth,entropy` arm: 481 cycles. There is no frame glue left
  to win.

**Per-channel in-frame kernel costs, against the C record:**

| kernel | ours | C/asm | ratio | prize/frame (mean) |
|---|---:|---:|---:|---:|
| `pre` | 11,874 | 9,700 | 1.22x | 4,347 |
| `fft` (asm v4) | 41,671 | 31,400 | 1.33x | 20,541 |
| `post` | 11,941 | 10,900 | 1.10x | 2,082 |
| `ola` (fused s8) | 16,994 | 15,300 | 1.11x | 3,388 |

These replace the pre-C-3 table (which read `pre` 17,654 and `post` 16,010 from
the isolated `ovl` arm, `fft` 61,852 from the asm-v4 + `iwram-tw` A/B and `ola`
21,418 from `ola_s8` — all timed on the bench's **EWRAM** buffers). The in-frame
numbers are 21–33% lower because C-3 put `x`/`buf`/`p` in IWRAM; the FFT's 61,852 c/call reading that made it "1.97x C, near its floor" was
the same artifact. **In-frame the FFT is 1.33x C and carries 20,541 of the
25,668-cycle synthesis gap.**

**The C rows are a worst-frame shape, and that is how the gate should read them.**
The record's decomposition (134,600 synthesis/pair + ~47,000 entropy/dequant/M-S)
sums to 181,600, within 0.2% of its recorded worst frame of 181,928 — so on the
worst frame the comparison is ours 274,198 against 181,928. The f98 components
are the dump image's, so they sum to 274,214 — 16 cycles (the constant) above
the product frame:

| | ours f98 (dump image) | C worst-frame shape | gap |
|---|---:|---:|---:|
| synthesis | 162,987 | 134,600 | 28,387 |
| entropy + dequant + M/S | 110,741 | ~47,000 | 63,741 |
| wrapper | 486 | — | — |
| **total** | **274,214** (product: 274,198) | **181,928** | **92,286** |

Entropy/dequant/M/S is therefore **69% of the distance to C on the gate frame**
and synthesis 31%. That inverts the previous order of work, which had entropy
fourth behind three kernels worth 9,817/frame together. It also re-answers the
old question "can asm alone close it?": no — asm on all four kernels at C-class
recovers 25,668 mean / 28,387 at f98, which clears the 12,054-cycle deficit to
100% but nowhere near the ~85% working target.


### Step 0b — entropy pre-analysis: where the 101,122 cycles go (2026-11-15)

Step 0 named the entropy/dequant path as the gate-critical item and left one
question open: *inside* it, what costs the cycles? This section answers the
cheap half of that with counts and code, before any rewrite. It is the input to
the sub-frame attribution PR, not a substitute for it.

**Method.** Symbol counts are a property of the bitstream, so they are read on
the host with `make uli-frame-ops` (`crates/uli-lite/examples/frame_ops.rs`)
instead of from a `verify` emulator build. Nothing here derives cycles; it
counts operations and pairs them with the Step 0 timed arms.

**The worst frame (f98) versus the frame the cycle model was tuned on (f72):**

| | f98 (gate frame) | f72 (old "worst") |
|---|---:|---:|
| bits in frame | 3,639 | 2,274 |
| Huffman symbols (`band + pair/2 + quad/4`) | **238** | 126 |
| band-class symbols | 61 | 64 |
| pair groups / quad groups | **352 / 4** | 0 / 248 |
| coefficients written (nz + zc) | 720 | 992 |
| nonzero / zero | 666 / 54 | 616 / 376 |
| raw-bit fields (`rbits`) | **128** | 0 |
| escapes | 21 | 140 |
| long codes (`huff_slow`) | **32 (13.4%)** | 0 |
| M/S frame | **yes** | no |

**The timed gate frame is not the busiest frame, and this is the trap to avoid.**
`make uli-frame-ops` names frame 125 as the most bits (4,225 vs f98's 3,639),
frame 90 as the most symbols (250 vs 238) and frame 32 as the most nonzero
coefficients (726 vs 666) — yet f98 is the worst *timed* frame. Its mix is what
costs: 352 pair groups, 128 raw-bit fields, 32 long codes, an M/S frame. Cost is
not a function of bits or of symbol count, so any attribution must be taken on
the timed frame rather than a proxy for density.

Reading it with Step 0's 101,122 cycles for the path: **~140 cycles per
coefficient slot** (720) and **~425 per Huffman symbol** (238). Per *nonzero*
coefficient it is ~152. The honest comparison stays the frame-level **2.36x**
(110,741 vs C's ~47,000); the per-coefficient form is for sizing levers, not for
a ratio claim. **Retired:** the "~272 cycles/coefficient against C's ~41" figure
carried by the PR C ledger — it divides our per-symbol work by the C bench's
per-coefficient work on a different frame and is not derivable.

**What this changes about the target.** f98 is **pair-class and M/S**: 352 pair
groups, 4 quads, 128 raw-bit fields. The quad path — the one the corpus model
and PR C-3a's `q3` expansion were built around, and the shape of the old worst
frame f72 — is almost absent from the gate frame. The hot path at f98 is
`CT_PAIR`: one Huffman symbol per 2 coefficients, two `get(rbits)` raw reads,
two escapes, two dequant+sign+store.

**What the code costs today** (`crates/uli-lite-gba/src/decoder.rs`; disassembly
of `dec_chan` in `.iwram`, 1,155 instructions = 4,620 B, 188-byte frame, 86
stack accesses):

- **The reader is addressed through memory.** `dec_chan` takes `br: &mut
  BitReader` (`:507`), and the out-of-line `huff_slow` (`:408`) and `eg_slow`
  (`:362`) take the same `&mut`, so the struct's address escapes: every refill
  and every slow call stores `next`/`hw`/`valid`/`bad` back into it
  (`str r1,[r5,#12]`, `str …, [r5,#16]`, `strb …, [r5,#20]`).
- **The `huff` fast path is 9 instructions with two conditional branches**
  (`lsr; add; ldrh; cmp; beq` for the LUT miss, then `ands; beq` for the
  `len == 0` sentinel the C decoder does not have), **plus a third branch in
  `skip`** (`cmp valid,len; bls` to the slow path at `:307`).
- **Refill is a loop with an end-pointer check and two byte loads**
  (`:261` + `:221`: `ldrb r4,[r3]; ldrb r6,[r3,#1]`, four in `set_pos`).
- **The sign bit costs a `peek` and a `skip`** (`put_coef`, `:454`, via `get`),
  i.e. two more branch-bearing window operations, where C tests the window's
  sign bit and shifts.

**What C does instead** (structural comparison; external reference, no source
incorporated — `gba-audio/src/uli/gba/uli_gba_dec.iwram.c` and
`uli_gba_int.h`):

- `REFILL(b)` runs once per symbol/group and *guarantees* n >= 16, so
  `huff` is `lut[w >> 22]` with one unlikely branch, and **`SKIP(b,k)` is
  unconditionally `w <<= k; n -= k`** — two instructions, no branch, no refill
  check.
- The sign comes from `((int32_t)b->w < 0)`, then `SKIP(b,1)` — no separate
  peek.
- Slow paths copy the reader in and out (`BR t = *b; …; *b = t`) so **only the
  temporary escapes to memory** — their comment says exactly this. The hot
  reader stays in registers.
- Per band they hoist what our loop reloads per coefficient: `r1 = rq[1]*step`,
  `dq15 = 16*15 - rq[15]`, and a **single** table pointer (`huff_slow` recovers
  the struct base from the LUT pointer), where our `HuffmanParts` keeps five
  slices live across the loop.
- They carry no bounds check on the refill path at all (malformed codes set an
  error flag instead); ours checks `next + 2 > end` per refill and per peek
  fallback.

**The levers, in expected-value order** (each its own measured arm):

1. **Take the reader off the stack** — by-value cursor, everything
   `#[inline(always)]`, slow paths take and return a copy. Biggest item, and the
   precondition for (2).
2. **Adopt the refill contract** — one refill per symbol/group guaranteeing
   >= 16 bits, then `huff`, `skip` and the sign are branchless. That removes most
   of the taken branches a pair iteration pays — one or more per `peek`, `skip`,
   sign, refill and escape — and on ARM7TDMI a taken branch is a 3-cycle pipeline
   flush with no predictor to hide it.
3. **Drop the hot-path bounds/`bad` checks** (validate at album parse) and
   refill with **one `ldrh`** instead of two `ldrb` (needs a 2-byte-alignment
   assertion on the ULA2 stream, which the format already assumes).
4. **Shrink the per-band live set** — one table pointer per band, hoist
   `rq[1]*step` and `dq15`, and skip the EWRAM `rq` load for the common `m == 1`
   digit. Register pressure is this project's measured failure mode (the FFT).
5. **LUT geometry last** — 32/238 symbols (13.4%) take `huff_slow` at f98;
   `LUTB` 11-12 or a two-level LUT, but only once the split says it matters.

**Do not retry** (each already measured, PR C): dequant fusion (neutral), full
`huff` inlining (+2.7% for 4,252 B of IWRAM), `Ops`/`Diag` on the hot path
(removed in C-3b).

**Next instrument — the sub-frame arms.** `attrib-entropy-*` cargo features on
`uli-lite-gba`, in the Step 0 style, all timing-only: `reader-only` (consume the
frame's bit count, no Huffman), `no-lut` (fixed-bit skip), `no-dequant` (store
the magnitude, no `rq`/multiply/sign), `symbols-only` (no coefficient writes),
plus a `huff_slow` hit counter. That partitions the 101,122 before the rewrite
lands. The arithmetic that keeps it honest: 12,054 cycles (to 100%) is ~12% of
this path, so levers 1-2 plausibly clear the period on their own; the ~51,000
needed for <= 85% wants the whole list.

**Blocked instrument, recorded so it is not silently relied on.** The bench's
`verify` build — which `per-frame` implies, and which the file's own wording
calls the ops witness — **fails the `ovl` pin on target at HEAD**
(`b3613428`/`b607b02e` with `--features verify`; `1b4f486e`/`28d04b52` with
`per-frame`), and the run stops before the product arms. The differing hashes
across two builds point at corruption rather than an arithmetic difference; the
leading hypothesis is `.iwram` headroom (29,688 B of 32,512 in the verify image,
i.e. 2,824 B below the stack top against a 768 B measured product high-water),
not yet demonstrated. `make uli-overlay-test` (host twins, `verify`) is green.
This is pre-existing, unrelated to the attribution work, and it is why the
counts above come from the host.

---

## Risks and open questions

1. **LLVM's ARM7TDMI codegen vs gcc — now diagnosed, and it is register
   allocation.** PR C measured that the codegen is *not* missing instructions:
   the FFT butterfly carries 6 `SMULL` + 6 `SMLAL` and zero `__aeabi_lmul`
   (the PR B fix held). What it does instead is **spill 66 intermediates to the
   stack**: the radix-4 butterfly has 14 values live at once against 16
   architectural registers, and the 32-bit `i64` accumulation widens every live
   range. That is a decision Rust gives to the allocator and cannot express, so
   the kernels need hand-written ARM asm (`global_asm!`, already used by
   `flac-lite-gba`) — PR C-1. **Superseded in part by Step 0 (2026-11-15):** with placement fixed,
  `pre`/`post`/`ola` are 1.10–1.22x C, so this risk now sizes only the FFT
  (1.33x) and the entropy path, not the three Rust kernels.
2. **IWRAM budget — the stack figure here is stale; see U6.** The reference
   spends ~22.6 KB (17.6 KB tables/buffers + 5 KB ARM code) of 32 KB. The
   ≈15.0 KiB stack quoted in earlier arms of this file was an artifact of
   stack-local twin buffers and was **withdrawn 2026-11-15**: the measured
   product stack high-water is **768 B**, `.iwram` sits at **25,548 B of
   32,512**, and `HotState` is therefore in IWRAM (C-3). Any further placement
   move still budgets stack first, but the budget is 768 B, not 15 KiB.
3. **Oracle drift.** The core must stay bit-exact and host-testable; every
   fast-path change is twinned. Do not “optimize” the oracle.
4. **Licensing — deferred, not resolved.** `gba-audio` has no LICENSE and the crate stays
   `publish = false` by decision (2026-11-14): no license outreach now. Note what
   this leaves standing: asm adapted from the reference is derived work from a
   repository with no license, which is acceptable only while nothing is
   published. A publish decision re-opens both this and `FORMAT.md`.
5. **Encoder independence — v1 boundary set.** The encoder of record stays the
   off-tree C tool for v1; this project builds nothing from it and vendors no
   part of it. The isolation rule is unchanged and now explicit: **no C source
   incorporated, no build/link/vendoring dependency on `gba-audio`, every
   artefact in this repo.** The Rust encoder is deferred, and v1's format is
   fixed (32,768 Hz, hop 512, stereo, no long-block) by decision 2026-11-14:
   the goal is matching the C project's fidelity here, not adding rates.
6. **Playback is not implied by decode.** A fast decoder is not a player;
   U4 must pass with margin before Phase D starts, and U5/U6 are separate.
7. **The C comparison is not yet like-for-like.** Step 0 narrowed the two
   instrument asymmetries that used to inflate our frame: the hot loop no longer
   carries the `Ops`/`Diag` bookkeeping the C product build compiles out (C-3b),
   and the frame quoted is the **int8-only** `prod8` pass the product actually
   writes, not a dual int8+int16 pass (C-3d). What remains asymmetric is the
   corpus and image, so every `×` against 69.4% stays a diagnostic, and a
   pessimistic estimate must not be used to justify a work item.
8. **Worst-frame and corpus mismatch — narrowed.** The C record's 69.4% is
   frame 27806 of its whole album (167,222 frames); ours is frame 98 (Step 0;
   frame 101 before C-3) of a 129-frame corpus. The kernel-row disagreement (134,600 vs 146,000) is
   **resolved in favour of 134,600** by C's own stage-2 decomposition, which
   also removes ~11,400 from C-1's prize (it is ~194,000, not ~182,000). What
   remains open is the comparison itself: pin the reference row in this tree's
   instrument before writing C-1's numeric gate, and treat "C-class" as ±5% in
   this tree, not an inherited percentage. **Corpus decision 2026-11-14:** the
   129-frame corpus stays the iteration instrument — at multiples over budget,
   reaching a realistic regime matters more than worst-case fidelity, and
   full-album runs would slow the loop. The worst-frame program is PR E,
   deferred until the decoder is near real time; no claim may rest on the
   129-frame number alone.
9. **Stereo is the product target; the speaker is served by the output stage
   — decided 2026-11-14.** The decoder and the playback path are stereo, aimed
   at the headphone output: fidelity on this hardware is the point of the
   project and is not traded for headroom. The GBA speaker is physically mono
   and the headphone jack is stereo, so the **speaker profile is in scope but
   is not a decoder mode** — full stereo decode always, mono fold-down after it
   (PR D), with both regimes measured. C's `ULI_GBA_MONO` / `ULI_OUT_M16` (one
   IMDCT per frame) is **explicitly not taken**, and U4 is sized against the
   two-channel frame. Worth recording, since it looks like a free lever: C's
   69.4% is already the *stereo* wall (its synthesis loop runs per channel), and
   their mono attempt landed at 84.2% worst / 77.3% mean because the collapse
   lived in the driver at 13.65%/frame — over their own target, and that output
   was never hash-checked. Single-channel output is **not** equivalent to a
   fold-down (hard-panned content vanishes; on an M/S frame `L = M + S`), so if
   it is adopted for cost it must be recorded as a fidelity trade, not as the
   same feature.

---

## Decision log

- **2026-11-15 — Step 0 done: the frame is attributed, and it inverts the order
  of work.** `make uli-attrib` measured the product frame by difference in one
  image, per-frame aligned: synthesis **160,268** mean / 162,987 at the worst
  frame (f98), entropy+dequant **66,943** / 101,122, M/S 4,413 / 9,619, wrapper
  481. In-frame the kernels are **1.10–1.33x C**, not 2.4x — the pre-C-3
  readings were taken on the bench's EWRAM buffers, not the product's IWRAM
  `HotState`, and overstated every kernel by 21–33%. The already-asm FFT is
  **41,671/channel (1.33x C)** and carries 20,541 of the 25,668-cycle synthesis
  gap; `pre`+`post`+`ola` are 9,817 together. On the gate frame,
  entropy/dequant/M/S is **110,741 = 40% of the frame and 69% of the 92,286-cycle
  distance to the C worst-frame shape**, so the entropy rewrite is now item 1
  and the three Rust kernels are item 3. Retired: "`pre` asm is the largest
  single headroom (~15,900)", "the FFT is 1.97x and near its floor", "the M/S
  loop costs 16,067". Method recorded in the ledger: timing-only cargo features
  (`attrib-*`), pins fail by construction, `attrib-dump` costs a constant +16
  cycles per window, and `attrib-entropy` alone is not quotable because the bit
  position freezes.

- **2026-11-14 — PR C-1 landed: hand-written FFT, measured as a ladder, wired
  into the product.** The FFT started as v1 (a careful hand allocation) at
  88,205 c/call — *slower* than the Rust ARM kernel's 85,065. That falsified
  the register-pressure premise: it is not spills but **memory traffic**.
  Removing the spill/reload traffic (v2, 73,363) and adding `ldmia`/`stmia`
  block transfers (v3, **71,635**) delivered **−18.8% from v1 and −15.8% vs
  Rust**, all bit-exact. Routing `GbaDecoder`'s synthesis through the asm FFT
  took the **full product frame 457,863 → 430,997 (175% → 164%)**, i.e. the
  gains are now real product-path gains, not just `ArmKernels`. Method kept:
  one lever per arm, same image, always a Rust control, always twinned on the
  ARM7TDMI. Still 2.28× the C record; the next research order (trivial
  butterfly, twiddle `ldmia`, then the multiply/memory floor, then a
  like-for-like corpus) is written in the “PR C-1 landed” ledger section. The
  remaining stack gap to C is not accepted as inherent: nothing about Rust+asm
  forces it, so it is scoped as continued work.

- **2026-11-15 — C-3 landed: `HotState` in IWRAM, the largest single win.**
  Moving the decoder's 8,192 B hot working set (`x`/`buf`/`p`, exactly the three
  buffers C keeps in IWRAM) out of EWRAM took the production frame **361,238 →
  274,198 (138% → 105%)**, **−87,040 (−24.1%)**, mean 232,090 (89%, now under the
  period), and **1.99× → 1.51×** the C decode wall. **The projection was
  10,000–15,000; the measurement is 87,040 — ~6x** — because the projection
  counted only the M/S pass while every coefficient access through those buffers
  was paying EWRAM wait states. Paid for by moving the Rust `fft` to ROM when
  `asm-fft` is on (it is not on the product path) and the superseded v1/v2/v3 FFT
  kernels to ROM, which is the general rule this work keeps teaching: **spend
  IWRAM on what the product actually executes.** `.iwram` 28,852 B of 32,512;
  usable stack from the BIOS top is 3,660 B against a ~2,960 B high-water, so
  ~700 B margin — usable here, below C's own ~1 KiB assert, and the full product
  ROM must re-check it. Also corrected the stack record: the earlier "768 B" was
  the descent below `run()`'s frame only, not the total from the stack top.

- **2026-11-15 — C-3d landed: OLA was measured in a configuration C never ran.**
  The `ola` kernel selected its output format *per sample* and computed both
  formats whenever an int16 buffer was passed — and the bench passed one, to pin
  both hashes. So the 2.28× OLA ratio the plan carried was partly a measurement
  artifact. Fused per-format kernels (`ola_s8`, `ola_s16`, format fixed outside
  the loop, C's own structure) plus an `ola_kernel` dispatcher measure the
  production int8 kernel at **21,418/call = 1.40× C** (dual 34,780 = 2.27×;
  s16 23,220 = 1.52×), shrinking OLA's headroom ~69%. It also found a real win:
  the product was writing int16 it never uses, so the int8-only pass takes the
  frame **387,878 → 361,238 (148% → 138%)**, mean 340,928 → 314,288, with the
  same int8 pin (`e6e87652`). Ladder: 175% → 148% → 138%, **1.99× C**. Recorded
  with the lesson that recurred three times in this work: *measure the
  configuration that ships*, not the one that is convenient to pin.

- **2026-11-15 — the fast path is the DEFAULT, and "C-class parity" is retired.**
  Two decisions. (1) `uli-lite-gba` now defaults to `["asm-fft",
  "iwram-tables"]`: the measured wins (v4 FFT + all synthesis tables in IWRAM,
  the C layout) are the product path rather than opt-in flags, so the **default
  build is 148%** (387,879) instead of 175%, with 8,212 B of `.iwram` headroom
  after the 768 B measured stack. The control is not lost: `frame core` (the
  portable/Thumb decoder) and the isolated `kern`/`ovl` arms still measure the
  Rust kernels in the same image, and `--no-default-features` gives the
  pure-Rust-kernel build. (2) **The 69.4% figure is re-read as a compute-budget
  existence proof, not a comparison to match.** C+asm runs this codec in real
  time on this exact hardware, which proves the *work fits the machine*; that is
  the fact we need. "C-class parity" is therefore retired as a gate — there is no
  reason Rust+asm should lose to C+asm here, so the target is U4/U5 (real time
  with gate/UI headroom), with the C figure kept only as a diagnostic for where
  time goes and as a rough shape of a solved implementation.

- **2026-11-15 — memory layout is read from C, and the stack figure was stale.**
  Placement is now derived, not guessed: C's linker script puts plain statics in
  **IWRAM** (`.data`/`.bss` both) and only explicit `.ewram` in EWRAM — the
  opposite of our agb default — and C's decoder uses ~no stack because every
  working buffer is a static. Profiling C's working set (`uli_g_X/buf/P` 8,192 B,
  five tables 8,672 B, state ~600 B, ~5 KB ARM code, ~100 KB Huffman in EWRAM)
  gave the target directly. Measuring our own stack with a sentinel probe found
  **768 B, not the ≈15.0 KiB this file had carried since PR B** — that figure was
  an artifact of stack-local twin buffers, already fixed in PR C-1 but never
  re-measured, and it had been *blocking* the tables-in-IWRAM item. With the real
  number, `iwram-tables` (C's own table layout) runs and measures the product
  frame at **148%** (387,879 / 340,929) with 6,196 B of `.iwram` headroom, and
  C-3 (`HotState` → IWRAM) is unblocked (fits once the superseded v1/v2 kernels
  are dropped). Lesson recorded next to the placement findings: a stale number in
  a decision record is a blocked work item.

- **2026-11-15 — PR C-1 follow-up: the twiddle table's location dominates.**
  Investigating the planned "twiddle `ldmia`" step revealed the actual cost: with
  `iwram-tables` off, `TW` lives in `.rodata` = cartridge ROM, so every hot
  twiddle load paid ~6 ROM wait-state cycles. A same-image A/B (same kernel,
  only the table pointer changed) measures **69,034 → 61,852 cycles/call
  (−10.4%)** for moving the table to IWRAM; the `iwram-tw` feature delivers it
  and takes the product frame **425,795 → 411,431 (162% → 157%)**. FFT now
  **1.97× the C record** — under 2× for the first time. Recorded because it
  revises the "why slower than C" answer: table placement, not codegen, was a
  large share. `iwram-tw` is opt-in: it costs 2,016 B of `.iwram` (margin ~600 B
  vs the ~15 KiB product stack), and making it the default waits on the U6
  budget (or dropping the superseded v1/v2 kernels to free 1,520 B).

- **2026-11-15 — PR C-1 follow-up landed: the trivial butterfly (v4).** C's
  `TRIV_BFLY_P` was the last structural difference in the FFT, and it is now
  taken. At j==0 of every stage the twiddle triple is exactly (2^30,0)×3
  (verified for l=256/64/16), so `cmul(y,1.0) == y>>2`: the three twiddled
  outputs are sums shifted right 2, with no multiplies and no twiddle loads.
  `fft_arm_v4.s` measures **69,034 cycles/call — −2,601 (−3.6%) vs v3**,
  larger than the ~1,000 the ledger projected, because the path also drops the
  per-output twiddle-pointer load and all six twiddle loads. It covers 21 of
  the 192 radix-4 butterflies (an earlier note's "3/192" counted j-iterations,
  not butterflies). Wired into `fft_kernel`, the product frame goes
  **430,997 → 425,795 worst (164% → 162%)**, the delta being exactly 2× the
  per-FFT saving. Ladder: 85,065 → 88,205 → 73,363 → 71,635 → **69,034**,
  2.20× the C record. U4 still open; next is twiddle `ldmia`, then the
  multiply/memory floor, then a like-for-like corpus (PR E).

- **2026-11-14 — C-3b landed; the prediction held.** The counters and format-rule
  diagnostics moved behind `uli-lite-gba`'s `verify` feature: the product build
  measures **457,863 worst / 410,912 mean (175% / 157%)**, **−30,822 / −18,427**
  against the attribution's predicted −30,482 / ~20,000 — the first projection in
  this file to land on its number. The `verify` build reproduces the previous arm
  within 3 cycles, so it is a switch rather than a rewrite. The product decoder is
  no longer the arm that witnesses `xover=0 hfnz=0`: that stays with the core
  decoder's pass in the bench and with the `verify` twins
  (`make uli-overlay-test` now runs both builds, and a guard test fails if the
  counters come back outside the gate). Ladder: 2526% → 1133% → 698% → 204% →
  186% → **175%**. Next is coefficient buffers to IWRAM with C-1.

- **2026-11-14 — the attribution is built, run, and it reorders the work.** Two
  instruments: a per-frame dump in the bench (`--features per-frame`, which
  reproduces the gate arm exactly, so it does not perturb the measurement) and
  by-difference arms. The 129 rows fit a per-event model at R² = 0.9994 (residual
  sd 1,170 cycles); the notable coefficients are pair 302, quad 341, escape 134,
  raw field 57, long code 221, M/S frame 18,124. Findings: **the diagnostics are
  the biggest item (~30,000)**; **M/S costs 16,067 but a four-coefficient block
  rewrite recovers only 1,776**, so the cost is EWRAM traffic, not loop shape —
  the reference's coefficient buffers are in IWRAM; the dequant arithmetic is
  only ~4,000 (its row is mostly the `xover` compare); and the fills (~7,400
  mean) are required because `pre`/`post` read all `M` coefficients. The
  band-symbol term is **not identifiable** (collinear) and is not quoted. Work
  order becomes C-3b → IWRAM coefficient buffers (with C-1) → C-1 → OLA, and the
  remaining gap is now explained rather than mysterious: diagnostics plus
  memory placement, not an unexplained per-symbol cost.

- **2026-11-14 — C-2 landed as C-3a (+C-3c), and it was measured, not assumed.**
  The structural port and the 32-bit dequant measured **488,685 worst / 429,339
  mean (186% / 164%)** against a re-measured `main` baseline of 523,267 /
  456,187 — **35,000 saved where the plan projected 90,000**. Consequences
  recorded: the counter-hoist idea was built, measured **worse** (+21,000,
  register pressure) and rejected; C-3b is the largest measured lever left
  (−11,000 alone, −30,000 with the diagnostics); and the item list now projects
  95–107%, so **parity needs a per-stage attribution of the remaining ~140,000
  cycles**, which the plan did not previously contain. All twins and pins green
  (`xover=0 hfnz=0`, overlay twin PASS).

- **2026-11-14 — the eight open questions are answered; scope is frozen for v1.**
  Recorded as one block because they interact:

  - **asm provenance.** ARM-mode asm may be adapted from the reference; **C
    source may not be incorporated**, and there is **no build, link or vendoring
    dependency on `gba-audio`** — every artefact lives in this repo and is
    twinned against our own oracle. This unblocks C-1; the license consequence
    stays deferred with `publish = false`.
  - **U4 ceiling.** Not an inherited or assigned number: it is **measured** at
    the first near-real-time arm, from the measured gate total plus this tree's
    non-audio cost. Until then 73–81% is a projection, not a grade.
  - **Corpus.** `uli2_tiny` (129 frames) is the iteration instrument, deliberately:
    at multiples over budget, reaching a realistic regime beats worst-case
    fidelity, and full-album runs slow the loop. The comprehensive worst-frame
    analysis is **PR E**, deferred to the real-time regime, FLAC-style.
  - **Output regimes.** Speaker profile **in scope**; the decoder is always
    stereo and the mono collapse happens in the output stage (never one IMDCT).
    Fold-down method — `(L+R)/2` vs single-channel — is priced at PR D; they are
    not equivalent, and single-channel is accepted as the cheap option if the
    measurement supports it.
  - **U6 priority.** Audio first: IWRAM goes to code, hot state and stack, then
    synthesis tables if the stack allows; UI affordances are the slack.
  - **U7 licensing.** `publish = false`; no license work now.
  - **Malformed streams.** No promise: this project controls its own encodes.
    Rejection-only stays (sentinel, no panic/hang); C-3c's i64 path moves to the
    verifying build.
  - **Format scope.** v1 is fixed (32,768 Hz, hop 512, stereo, no long-block);
    the target is matching the C project's fidelity here, not new rates. The
    Rust encoder stays deferred with the same no-C/no-dependency rule.

- **2026-11-14 — stereo is the product target (mono risk closed).** The decoder
  and the playback path are stereo, aimed at the headphone output; fidelity is
  the project's purpose and is not spent to buy CPU headroom. Hardware note:
  the GBA speaker is mono, the headphone jack is stereo, so the speaker profile
  is served by an **output-stage** mono fold-down over a full stereo decode —
  the decoder never collapses, and C's `ULI_GBA_MONO`/`ULI_OUT_M16` one-IMDCT
  path is not taken. U4 is read against the two-channel frame, and both output
  regimes are measured. Supporting evidence: C's headline 69.4% decode
  wall is itself stereo (its synthesis loop runs twice per frame); the mono
  speaker profile measured 84.2% worst / 77.3% mean because the collapse lived
  in the driver (13.65%/frame), and that output is driver-defined and was never
  hash-checked. Same day, the C kernel-pair row was pinned at **134,600** (C's
  own `pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k` per channel), striking PR
  C's ~146,000; C-1's prize is therefore ~194,000 and the revised projection is
  **~193,000–213,000 (73–81%)** — parity plausible at the optimistic end, not
  proven.
- **2026-11-14 — C-1 + C-2 are insufficient for the target; PR C-3 proposed.** A
  parity audit read the C product decoder
  (`gba-audio/src/uli/gba/uli_gba_dec.iwram.c`) against our hot path and found
  ten differentials the plan had not scoped: a `q3[81]` quad-expansion table
  (we divide per quad), a per-class `rq[16]` dequant table plus one multiply (we
  clamp-compare and carry an i64 exactness path per coefficient), direct writes
  into `X` (we write a `m[16]` scratch), zero bands and the EOB tail as `STM`
  fills (we pre-zero all 512 and zero again per band), a packed
  `(sym << 5) | len` `huff` (we pair it with a separate `skip` and a per-band
  `bad` check), an inline `clz6` escape fast path (ours is `inline(never)`), a
  register-resident `{w,n,p}` reader copied once per channel (ours is
  `&mut BitReader` throughout), no stats counters in the product build, and
  format-fused OLA kernels with M/S limited to `nb * BANDW`. Recorded
  consequence: the ~94% projection (now ~91% once the C row is pinned) is the
  period, not parity (69.4%); C-3 is
  split into C-3a (executed *as* C-2, which lacked a design), C-3b/C-3c (build
  shape), C-3d/C-3e (output stage and wrapper), with C-1 re-specified as asm
  that also cuts the ~15.0 KiB stack — that cut is what makes `iwram-tables`
  legal again. Revised projection ~193,000–213,000 (73–81%); the U4 gate is
  re-read as C-class,
  since the gate total adds convert + IRQ + DMA to the decoder.
- **2026-11-14 — PR C: measured the gap instead of guessing at it.** Frame 101
  (the worst) is **63% synthesis kernels / 33% entropy (`dec_chan`) / 4% M/S +
  wrapper**. Four cheap levers were built and run on target: fusing dequant was
  neutral, full `huff` inlining bought 2.7% for 4,252 B of IWRAM (rejected on
  U6), a split `huff` LUT fast path bought 1.6% for 896 B (kept), frame
  pointers were inert, and `iwram-tables` crashed because 8,672 B of tables
  leave less stack than the ~15,040 B the path needs. Consequence: the product
  frame is **523,267 worst / 456,187 mean (200% / 174%)**, and U4 stays open
  with the remaining work scoped as PR C-1 (asm kernels, ~194,000 cycles — the
  ~182,000 figure was struck when the C row was pinned at 134,600) and
  PR C-2 (entropy rewrite, ~90,000 cycles) — projected ~94% together. Recorded
  because the plan had assumed tuning might close this; measurement says the
  gap is architectural.

- **2026-11-14 — PR B landed: the hot path is GBA-first and the ≤2× gate is met.**
  `uli_lite_gba::GbaDecoder` (32-bit cached bit reader, unchecked canonical
  Huffman, dequant, M/S, ARM-mode `.iwram`) measures a full frame at **533,605
  worst / 463,660 mean = 204% / 177%** of the period, both pinned, against the
  same-image core control at 1023% / 830% and `ArmKernels` at 640% / 447%.
  Non-synthesis `rest` fell 837,092 → 195,484 cycles/frame (4.3×). PR B's gate
  (≤200% worst) is met; U4 is not (204%). The remaining gap is the synthesis
  kernels (2.51× the C/asm reference, OLA the outlier) — that is PR C's scope,
  and it is now the whole remaining delta, not a part of it.
  Placement record: the first on-target run **hung** (`pc=0x00000004`) because
  an 8 KiB `HotState` in `.iwram` left less stack than the ≈15.0 KiB the
  product path needs; the stack overflowed past `__iwram_end` into the hot
  buffers and the ARM/Thumb thunks. The host twin was green throughout, so this
  was a target placement defect, not arithmetic. `HotState` is EWRAM in the
  bench; the IWRAM/stack budget is now U6's binding constraint.

- **2026-11-14 — ULI2 is GBA-first.** Portability constraints retired for the
  product path; `uli-lite` becomes the oracle, `uli-lite-gba` the product
  decoder. PR A lands the kernel seam and the full-frame instrument; PR B
  rewrites the hot path; PR C closes the gap; PR D is playback.
- **2026-11-14 — PR A landed.** The `Kernels` seam and full-frame instrument
  make the product kernel set measurable end to end: 1,829,247 worst / 1,322,800
  mean (698% / 505% of the period), both pinned. The Phase 1b projection is
  retired. Recorded caveat: absolute frame numbers are image-specific at this
  fetch-dominated scale; every future arm keeps the control in the same image.
- **2026-11-14 — plan of record moves here.** `ULI2.md` stays the decision
  record; phases, gates and the measurement ledger live in this file.
