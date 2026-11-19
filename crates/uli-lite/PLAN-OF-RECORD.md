# ULI2 plan of record

[`../../ULI2.md`](../../ULI2.md) is the decision record (why ULI2, what it is,
the licensing and encoder risks). **This file owns the phases, gates, the
measurement ledger and the order of work.**

## Current state (2026-11-19)

mGBA, WAITCNT 4/2, 262,144-cycle period, no UI. Both corpora pinned
(`xover=0 hfnz=0`), two runs per image identical to the cycle.

| | worst | mean | of period |
|---|---:|---:|---:|
| 129-frame fixture decode | 193,317 (f98) | 172,109 | 73.75% / 65.66% |
| album decode wall (16,553 frames) | **194,695** (f4,998) | **169,354** | **74.27% / 64.60%** |
| gate total (decode + convert + IRQ + DMA) | **226,152** | **200,624** | **86.27% / 76.53%** |

- **U2, U3 and U4 are met.** U4's ceiling stays the period: **13.73 points of
  margin at the worst frame, 0 starved periods over 16,552**.
- **1.070x the C + ASM reference on the decode wall** (C: decode 69.43%, gate
  73.63%). Of the 12.6-point gate gap, 4.84 points are decode and **7.80 are the
  convert/IRQ/DMA half at ~2.9x C's cost** — the output path, not the codec, is
  now the largest remaining difference.
- **All five hot kernels are hand-written asm**, each twinned bit-exactly on the
  ARM7TDMI (`make uli-kernel-twin`): `pre` 1.00x C, `fft` 1.05–1.09x, `post`
  ~1.03x, `ola` 1.00x, M/S 0.64x (mean).
- **Entropy/dequant is ~35,656 mean, at or below C's 42–48k.** The product
  reader is **closed-world** — no bounds test, no error flag, 16-bit contract —
  with `crates/uli-lite-gba/tests/premise.rs` asserting the premise and
  `uli_lite` remaining the checked oracle.
- **Placement:** `.iwram` 26,496 B of 32,768 (code + `HotState` 8,192 +
  `ClassTables` 560 + `q3`/`clz6` 148); stack high-water 948 B (fixture) / 1,988 B
  (gate); code tables in `.ewram`.
- **Open:** the non-decode gate half; the dense-frame tail of entropy (the
  sub-frame partition needs re-measuring on the product reader first — see "The
  entropy path against the C+ASM reference"); a licensing/publish decision.

**Everything below this line is the ledger that produced those numbers, in
chronological order, and it is kept deliberately: this project's method is that
a claim is worthless without its measured arm and a falsification is worth more
than a projection.** Several intermediate figures are therefore superseded by
the table above — where a passage was contradicted outright it is marked
*Superseded* in place. Read the current state from this block; read the reasoning
from the ledger. Two exceptions worth knowing: the kernel table's `fft` row and
the `pre`/`ola`/M/S ratios were re-derived with bit-exact arms (2026-11-18), and
`post` was the only instruction-named deficit and is closed.

---

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

**Step 1 of the critical path landed (2026-11-15): the entropy/dequant rewrite,
and the decode wall is under the frame period for the first time.** Two pieces,
in the order the plan required:

- **The sub-frame instrument** — five timing-only arms
  (`attrib-entropy-{reader,nolut,nodequant,nowrite}` plus an `attrib-count`
  `huff_slow` hit counter) partition the entropy path at the gate frame instead
  of guessing at it. `make uli-attrib-subframe`. It found that the dequant
  arithmetic is **13%** and the bit-reader mechanics **5.5%** of the path: the
  cost is symbol resolution and the per-coefficient work around it.
- **The rewrite** — reader by value in and out of the channel decode, a refill
  contract that guarantees 17 valid bits (so `skip` becomes branchless), a
  long-code search that reads the window instead of `peek`ing up to sixteen
  times, the sign straight off the window's top bit, and a smaller per-band live
  set. The entropy path fell **111,227 → 86,826 at the gate frame** (−22%, the
  projection was ~12% for the period alone) and the product frame went
  **274,198 → 249,835 worst (105% → 95.3% of the period)** and **232,090 →
  220,757 mean (89% → 84.2%)**, pins unchanged in both twin builds.

**Step 1e (2026-11-15): budget work — `.iwram` down 1,352 B and the frame at
83.6%, with one hypothesis falsified.** Three arms, each measured on its own:

- **The Rust control kernels left `.iwram`** — `pre`, `ola_s8` and `ola_s16` are
  the in-image *controls* now that asm kernels exist, so their placement is gated
  on the asm feature being off, exactly as `fft` is gated on `asm-fft`. **−496 B,
  and the frame is byte-identical** (nothing on the gate path calls them).
- **Register relief in `dec_chan` was tried and rejected.** Folding `bad` into a
  `valid` sentinel (−1,672 B of code, `dec_chan` 1,610 → 1,192 instructions) cost
  **+1,450 cycles mean**; adding the intended relief (deriving `esck`/`d15` in the
  rare branches instead of holding them) made it **+3,351**. The reason is
  recorded: indexing `tables` inside those branches **resurrects `tables` as a
  live value across the whole loop**, which is exactly the pressure the change
  was meant to relieve. The register-pressure hypothesis for this kernel is
  falsified — the second such falsification after the FFT's v1.
- **The cursor rewrite landed.** Three slot loops and the band loop compared
  pointers instead of carrying counters, and the verify-only `hfnz` rule became a
  pointer compare against `x + NCODED` instead of an index — which is what let
  the counters go. **−856 B and −665 cycles mean / −2,081 worst.**
  But the mean is the honest number here (the worst frame moved from f93 to
  f125, so worst-vs-worst is not like-for-like), and 665 is a third of what
  `pre`/`ola` got from the same shape: `dec_chan`'s stores were already
  index-scaled by LLVM, so there was less to remove.

Net: **`.iwram` 28,632 → 27,280 B (−1,352), product headroom 828 → 2,196 B**, and
the frame **221,332 → 219,251 worst (84.4% → 83.6%) / 195,154 → 194,489 mean
(74.4% → 74.2%)**, **1.22x → 1.21x** the C wall.

**Step 1d (2026-11-15): the overlap-add, `ola` at C parity, and the <=85% headroom
target met.** `ola` was 34,375 at the gate frame = 17,187/channel = 1.12x C,
and the disassembly put our loop at **33 instructions per iteration against the
reference's 26**: the clamp was 8 instructions per output where the reference
needs 6 (it saturates to int16 with a three-instruction idiom, then shifts right
8 — and `sat8(x >> 8) == sat16(x) >> 8` for every i32), and the addressing was 7
instructions of index arithmetic where the reference keeps six post-indexed
cursors and reads the overlap word with `ldr [b_cursor, Y_minus_P]`. Rewritten in
that shape (`asm/ola_arm.s`, 160 B per format, **26 instructions**):
**34,375 → 30,539 at the gate frame** (−3,836, exactly the frame delta) =
**15,270/channel = 1.00x C**, the first kernel at parity. The product frame is
**225,168 → 221,332 worst (85.9% → 84.4%)** and **198,992 → 195,154 mean (75.9%
→ 74.4%)**, **1.24x → 1.22x** the C wall, pins unchanged and the asm twin green
(`make uli-kernel-twin`, now covering `ola` too). **The <=85% headroom target was
met at that point, with 1,490 cycles of margin** (2,940 after Step 1e), and the
remaining gap to C was 62% entropy / 37% synthesis.

**Step 1c (2026-11-15): the `pre` pre-twiddle, and the frame was at 85.9%.** The
instrument came first and **falsified the projection**: `pre` was not
address-rebuilt and not spilling — LLVM's loop was 15 instructions per output
pair with zero stack traffic — but the reference's is **10**, because it uses
four post-indexed cursors and one `stmia` per pair instead of a computed index
plus split pre/post store pairs. Rewritten in that shape (`asm/pre_arm.s`, 76 B,
10 instructions, no address arithmetic): **26,458 → 23,889 at the gate frame**
(−2,569, exactly the frame delta), **11,945/channel, 1.36x → 1.23x** C. The
product frame is **227,736 → 225,168 worst (86.9% → 85.9%)** and **201,554 →
198,992 mean (76.9% → 75.9%)**, **1.25x → 1.24x** the C wall, pins unchanged and
the asm twin green (`make uli-kernel-twin`, now covering `pre` too). The <=85%
headroom target needs **~2,350**.

**Step 1b (2026-11-15): FFT v5, and the frame was at 86.9%.** The hand-written
FFT took the reference's address shape — an absolute byte cursor with
register-offset addressing, a register-held twiddle pointer, and the destructive
sum formation that needs no moves — for **81,035 → 71,519 at the gate frame**
(−9,516, exactly the frame delta) and **35,760/channel = 1.14x the C record**,
the closest any kernel has come. The product frame is **237,252 → 227,736 worst
(90.5% → 86.9% of the period)** and **211,071 → 201,554 mean (80.5% → 76.9%)**,
**1.30x → 1.25x** the C decode wall, pins unchanged and the asm twin now a real
gate (`make uli-fft-twin` — it had been compiled out of `make uli-bench` since
PR C-1, so that line read `fftasm=true` vacuously). The <=85% headroom target
now needs **~4,900** cycles.

**Step 1a of the critical path landed the same day: the bit window is in
registers.** `dec_chan` destructures the reader into five `let mut` scalars and
`refill`/`take`/`top`/`esc`/`coef` are **macros over them** — the reference's
`REFILL`/`SKIP` shape, and the only Rust shape that did not put the 24-byte
struct back in an alloca (three ABI/promotion attempts had failed). Another
**−12,583 / −9,686**: the product frame is **237,252 worst / 211,071 mean =
90.5% / 80.5%**, **1.30x** the C record, **−36,946** off the worst frame across
the two steps. That flipped the order of work: the deficit is now 42% entropy /
57% synthesis, and the FFT (35% of the frame, 1.29x C) is the only block large
enough to close the ~14,400 the <=85% target still needs.

**90.5% is the decode wall, not the gate total.** Convert + IRQ + DMA and this
tree's UI are not in it, so U4 is still open and playback still does not start;
the headroom target is the <=~85% the critical path names.

**The next lever is measured, not guessed: the bit reader is still in memory.**
`dec_chan`'s disassembly (1,491 ARM instructions, 5,976 B of `.iwram`) addresses
the 24-byte `BitReader` through one base register, so every `refill` and `take`
stores `next`/`hw`/`valid`/`bad` back — and the per-band hoists (`rq`, `step`,
`dq15`) are spilled alongside it. Two ABI experiments and one promotion attempt
did not move it (see "Step 1" below); the reference gets its window into
registers because its `REFILL`/`SKIP` are **macros over locals**, not function
calls through a pointer. That is the next arm. Also: with the entropy path down
23,000, **synthesis is again the larger share of the frame (65%)** and the gap
to C is now split ~58/42 between entropy and synthesis, not 69/31.

The target is **real time inside the hardware's cycle budget** (U4/U5), not a
percentage borrowed from C: the C/asm decoder's 69.4% is a proof that the work
fits this machine, so Rust+asm is expected to as well. See "The target is the
real-time budget" below. The measured ladder is **2526% → … → 175% → 148% → 138%
→ 105% → 99% → 95.3%**. The fast path is the default; **C-3d (2026-11-15)**
removed the int16 output the product never used; **C-3 (2026-11-15)** moved
`HotState` to IWRAM for **−87,040 cycles/frame**; and **Step 1 (2026-11-15)**
rewrote the entropy path for **−24,363**, which is the change that took the
decode wall under the period. **Step 0 of the critical path attributed the frame
by difference** — on the worst frame, entropy/dequant/M-S and synthesis each own
about 40% of the distance to C now. See "Critical path to real time" below;
every item there is a projection until it lands as its own measured arm.

The Phase 1b measurement that forced this rewrite is recorded in
[`../../ULI2.md`](../../ULI2.md) → “Phase 1b — what the on-target measurement
found”. The instrument is [`../../examples/uli_bench`](../../examples/uli_bench)
(`make uli-bench`).

---

## This file, and the record it summarises

**Two files, one topic each.**

| file | what it is | how to read it |
|---|---|---|
| **this file** | current state, architecture, memory layout, the measurement *method*, the closed-world premise, and the reference tables | the parts that stay true between measurements |
| [`RECORD.md`](RECORD.md) | the chronological record: the critical-path steps, every PR, every measured arm, the landing log and the risks as they were understood at the time | read it to learn *why* something is the way it is, or to find the arm behind a number |
| [`../../ULI2.md`](../../ULI2.md) | the decision record: why ULI2, its gates, its risks, its decisions | the front door |
| [`../uli-lite-gba/README.md`](../uli-lite-gba/README.md) | the product path’s architecture and gates | using the codec |

The split is deliberate: this file was 4,600 lines of interleaved reference and chronology, so every reader had to know which half they were in. Nothing was deleted — the record keeps its arms, its falsifications and its superseded drafts, because a claim here is worthless without the measurement behind it.

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

### Caveats that must travel with these numbers

- **Per-kernel rows are ±3%** (they disagree by up to ~10% between two
  re-measurements a day apart). The single-kernel deltas sum to 148,347 at f98
  against the measured sequence 147,074 (+0.9%). After 1e the *mean* is the
  comparator of record: the worst frame moved from f93 to f125, so worst-vs-worst
  across images is not like-for-like. The interaction is the FFT's
  data-dependent trivial-butterfly path: removing `pre` feeds it a stale `buf`.
  Quote the sequence row for synthesis.
- **`attrib-entropy` alone is not quotable.** Removing `dec_chan` freezes the bit
  position, so all 129 frames synthesise one stale frame. The entropy row comes
  from `synth` minus `synth-entropy`.
- **`attrib-entropy-nowrite` is not quotable at all** — see the table.
- **The sub-frame arms are substitutions, not pure removals, wherever they touch
  symbol resolution** (`reader`, `nolut`): their bit volume and their coefficient
  counts differ from the real stream, so the delta prices "resolution plus what
  it consumes" with a ~7,200-cycle correction, not the lookup alone. `nodequant`
  and `count` are exact.
- **The wrapper is 432 cycles.** The frame has no glue left to win.
- **83.6% is the worst of a 129-frame iteration corpus, not a worst-case
  guarantee** — C's hardest frame was #27,806 of 167,222. Plan for margin; PR E
  is the real witness.
- Hand-written asm is only worth it once placement is right. Placement is still
  right (code, tables, hot buffers in IWRAM) and Step 1 did not disturb it —
  `.iwram` *shrank* 76 B.

### The kernel table, re-derived with bit-exact arms (2026-11-18, mGBA, WAITCNT 4/2)

**Why the old table was replaced rather than adjusted.** Every by-difference arm
worked by *removing* a stage, and every stage after it then multiplied different
data — so each arm charged the downstream cost change to the part it removed.
The 2026-11-18 `attrib-pre2` arm measured that bias directly: `pre`'s delta came
out **1.61x** too large at the densest frame (31,150 vs a true 19,336), and the
inflation tracked frame density exactly. Three bit-exact arms replace it —
they **add a second, identical instance** of a stage instead of removing one, so
every downstream stage sees the data it would have and the pins still PASS:

| arm | how | bit-exact? |
|---|---|---|
| `attrib-pre2` | `pre` is a pure function of `x` into `z`; call it twice | **yes** |
| `attrib-post2` | `post` reads `z`, writes `y`; call it twice | **yes** |
| `attrib-entropy2` | `bitReader` is `Copy`; replay `dec_chan` from a saved reader | **yes** |
| `attrib-ola` | removal; nothing runs downstream of `ola` | contaminated only via its own stale `p` |
| `attrib-synth` | removes all 8 synthesis calls; no *other* stage changes | **yes** as a total |

`fft` is in-place and `ola` mutates `p`, so neither can be double-called; `fft`
is therefore the one **derived** row (`synth − pre − post − ola`), and it is
flagged as such.

**Clean attribution, 129-frame corpus, product image** (control f83 = 208,867,
mean = 182,132):

| part | at f83 | share | mean | share | source |
|---|---:|---:|---:|---:|---|
| `fft` | 68,694 | 32.9% | 66,079 | 36.3% | **derived** |
| entropy + dequant | 61,751 | 29.6% | 41,972 | 23.0% | bit-exact |
| `ola` | 30,521 | 14.6% | 30,120 | 16.5% | removal (clean) |
| `post` | 23,682 | 11.3% | 23,458 | 12.9% | bit-exact |
| `pre` | 19,336 | 9.3% | 17,669 | 9.7% | bit-exact |
| ms + wrapper | 4,883 | 2.3% | 2,834 | 1.6% | residual |
| synthesis total | 142,233 | 68.1% | 137,326 | 75.4% | removal (`synth`), exact |

Closes exactly by construction. The previous table's per-kernel rows summed to
+5.9% over `synth`; this one to −2.4%, inside the file's own ±3% tolerance.

**Three corrections this buys, against C's per-channel rows doubled:**

| part | old claim | **now** |
|---|---|---|
| `pre` | "31,150 → 1.61x C, the worst ratio left" | **19,336 = 1.00x**; mean 17,669 = **0.91x, ahead of C** |
| `ola` | "1.12x" | 30,521 = **1.00x**; mean 30,120 = **0.98x, ahead** |
| ms | "1.20x" | mean 2,834 = **0.64x, ahead** |

**And one methodological limit the numbers impose.** Because the audit put the
*data* in `Rs` (so multiplies early-terminate), kernel cost now genuinely varies
with frame content, and it varies by more than the gaps being adjudicated:

| part | spread across our 129 frames (max/min) |
|---|---:|
| `pre` | 1.20x |
| `post` | 1.15x |
| `fft` | 1.13x |
| `ola` | 1.08x |
| entropy | 26x (2,478 → 64,227) |

C's entropy row (42,600) sits at our **43rd percentile** — C's worst frame is
synthesis-heavy and entropy-light, ours is the opposite. So a single-frame ratio
against C cannot distinguish a deficit from a frame-mix difference; only an
all-frames mean on the *same* corpus can, and that is why the `cross` row above
is the number to trust.

## The entropy path against the C+ASM reference (2026-11-19)

**Status: nothing here is landed, and nothing here is a measurement of a change.**
It is the comparison that decides what to do next, made by reading
`uli_gba_dec.iwram.c` and our `dec_chan` line by line and disassembling the
built product image.

The FFT is now closed at 1.05x-1.09x C with its whole remainder worth ~5,200, so
entropy/dequant is the only row left with a double-digit deficit. (That figure
comes from the bit-exact `attrib-fft2` arm in the open `perf/uli2-fft-attribution`
PR; this tree's kernel table still reads 1.14x until that lands.) This section is
what the entropy deficit actually consists of.

**Measured shape: a density gap, not a flat one.** The bit-exact
`attrib-entropy2` arm (replays `dec_chan` from a saved `BitReader`; pins PASS)
gives the entropy block for the pair on the 129-frame fixture:

| | cycles/frame | of frame |
|---|---:|---:|
| mean | **41,973** | 23.4% |
| median | 54,868 | — |
| p90 / p99 | 63,456 / 63,963 | — |
| f83 (the control's worst frame) | **61,750** | 30.0% |
| f62 / f98 | 64,226 / 54,737 | 31.6% / — |

The C record's own line is *"entropy decoding about 21–24k cycles per channel"*,
i.e. 42–48k per frame. **On the mean we are already at C; on dense frames we are
1.3–1.4x.** So this is not a fixed overhead to shave off — it is a per-symbol and
per-nonzero-coefficient cost that only shows up on dense frames, which is exactly
why the record's warning that per-part ratios cannot be read off single frames
applies here as much as it did to the kernels.

The sub-frame arms locate it (each removes one layer; `nolut` desynchronises the
stream, so these order the work rather than grading the parts): full path 46,631 ·
coefficient store 2,111 · dequant arithmetic 9,285 · **symbol resolution 27,036** ·
reader mechanics 2,791 · residual (loops, escapes, sign bits, band control) 4,954.

**Structural comparison.**

| | C + ASM reference | Rust + ASM (ours) |
|---|---|---|
| reader state | `{w, n, p}` — **3 words** | `{base, next, end, hw, valid, bad}` — 6 |
| refill | `if (n<16) { w \|= *p++ << (16-n); n += 16; }` — post-indexed, **no bounds test** | `valid<17` **plus a nested `valid<17`** second load, a `next+2 > end` test and a `bad` store |
| refill contract | 16 bits, **refilled again before the raw bits** | **17 bits** up front |
| symbol decode | returns packed `(sym << 5) \| len`, `SKIP` unconditional | `(u16, u32)` tuple; per-symbol `len == 0` branch into `bad` |
| per-class arrays | `ctype`/`rbits`/`esck`/`rq[16][16]` **copied into IWRAM** (`uli_g`) | read from `PRODUCT_TABLES`, **`.ewram` @ 0x02000a00** |
| `q3` table | `uli_g_q3[81]` in IWRAM | `const Q3` in `.rodata` = **cartridge ROM** (0x08038d5c) |
| quad `m == 1` | hoists `r1 = rq[1] * step`, special-cases m == 1 | dropped — see below |
| code | `dec_chan` compact inside ~22.6 KiB of IWRAM | `dec_chan` alone **5,000 B** (`0x1388`) |

**Cause 1 — the reader's safety contract costs the registers the coefficient
loop needs.** The product `dec_chan` opens with `push {r4-r11, lr}` and
`sub sp, sp, #140` (**35 spill slots**), and contains **155 stack references in
1,250 instructions**. The spills are exactly the hot data:

```asm
ldr   r1, [sp, #56]        @ Huffman LUT pointer — every symbol
mvn   r0, #1
and   r0, r0, r7, lsr #21  @ + add: 3 instructions to form the byte offset
ldrh  r0, [r0]

ldr   r4, [sp, #80]        @ rq pointer — every nonzero coefficient
add   r3, r4, r3, lsl #1
ldrsh r7, [r3]
mul   r3, lr, r7

ldr   r4, [sp, #76]        @ dq15 — every m >= 16 coefficient
```

and `step` lives at `[sp, #68]`, reloaded after every `bl huff_slow`
(`ldr lr, [sp, #68]`).

In the inner loops we hold `next`(r4), `end`(r5), `hw`(r7), `valid`(r8),
`bad`(fp) and `step`(lr) — **six** — against C's `p`, `w`, `n`, `step` — four.
The two extra are **`end` and `bad`**, i.e. the bounds check and the error flag,
and those are precisely the registers `rq` and `dq15` would need.

**This re-frames the item, and that matters for how it is sized.** The record
graded this category as a *branch* problem — "almost every item here is a branch,
ARM7TDMI charges 3 cycles for a taken one, remaining ceiling ~2,000" — and that
is also why the two shapes built in the 2026-11-16 sweep failed: the branchless
clamp kept `end` live *and* put it in the load-address chain (+1,352 measured),
and `bad |= over` made `bad` longer-lived. **Neither freed a register.** Read as
register allocation, the same item is worth several times the branch estimate.

**Cause 2 — our per-class tables are in EWRAM and C's are in IWRAM.** At the
bench's WAITCNT 4/2, EWRAM is 4 cycles against IWRAM's 1. The reference
deliberately copies `ctype`, `rbits`, `esck` and `rq[16][16]` into `uli_g` and
leaves only the ~96 KiB of LUTs in EWRAM. Ours reads `rq[m]` from EWRAM on
**every nonzero coefficient** — 616/channel x 2 = 1,232 reads/frame, ~3,700
cycles/frame of avoidable latency. The fix costs **560 B of IWRAM** (`rq` 512 B +
three 16-byte arrays) against **3,876 B free** (`iwram_end` 0x0300673c to `sp`
0x03007660). Same story, smaller: `Q3` (81 B) is in ROM, and the reference keeps
`uli_g_q3` in IWRAM — ~500 cycles/frame across 496 quad groups.

**Cause 3 — the 17-bit contract paid for a run that cannot exceed 16.**
`refill!` demanded 17 valid bits because a raw coefficient reads `r` bits *then*
a sign bit, and 16 + 1 would underflow a 16-bit window. But `r` is only read on
the `pair` path, and **on every corpus this tree owns the PAIR classes have
`rbits <= 5`**:

| corpus | frames | k | max `esck` | `rbits` over PAIR classes |
|---|---:|---:|---:|---:|
| fixture `uli2_tiny` | 129 | 14 | 2 | 0,0,0,0,0,0,1,2,3,4,5 → **max 5** |
| `biomaster` (album) | 16,553 | 14 | 2 | **max 5** |
| `biotwo` | 33,104 | 14 | 2 | **max 5** |
| `adv` (adversarial) | 641 | 14 | 2 | **max 5** |
| `cross` | 167,222 | 14 | 2 | **max 5** |

Measured by parsing the blob through `CodeTables::parse_into`, per class as
`ctype/rbits/esck`: `0/0/0 1/0/0 1/0/0` then eleven `2/0..5/…` — one `ZERO`, **two
`QUAD`** with `r == 0` (where `rbits` is never read), **eleven `PAIR`** with
`r = 0,0,0,0,0,0,1,2,3,4,5`.

That census sits on top of the reference's own `tables_default` (`enc_core.c`) with
**one deviation**:

| field | `tables_default` | every corpus | verdict |
|---|---|---|---|
| `ctype` | `0,1,1,2,2,2,2,2,2,2,2,2,2,2` | identical | the class structure ships unchanged |
| `rbits` | `0,0,0,0,0,0,0,0,0,1,2,3,4,5` | identical | so **E-R4's `rbits ≤ 5` is the format's designed maximum**, not a margin chosen after the fact |
| `esck` | `0,0,**2**,2,2,2,2,2,2,1,1,1,1,1` | `0,0,**0**,2,2,2,2,2,2,1,1,1,1,1` | **class 2 escapes in the default and does not escape in the blob** — training tightened it, in the safe direction |

Two things follow, and both are load-bearing for the encoder. First, `esck` is
**derived**, not copied: the row that reaches the blob is not the row in
`tables_default`, so the encoder must validate its **final, trained** tables against
E-R4 and may not assume the defaults already satisfy it. Second, the bound itself
is the format's own: `max esck = 2` is what the default ships and what training
kept, and `rbits ≤ 5` is unchanged from the default in all five corpora — E-R4 is
the design, stated as a rule.

What genuinely differs per album is the Huffman **length rows**: the tables region
is 5,212 bytes in all five corpora under five different hashes (fixture
`0f5e1b96…`, `biomaster` `89c4eb31…`, `biotwo` `10ac765c…`, `adv` `64809502…`,
`cross` `93ef18f5…`), which is the "trained per album" property showing up exactly
where the plan says it should.

> **Corrected 2026-11-19.** This block previously read "both pair classes have
> `rbits == 0`" and named the two `1/0/0` classes PAIR and the eleven `2/…`
> classes QUAD — the two class names were transposed, and the counts with them.
> The citation was `tests/premise.rs`, which asserted `rbits == 0` on "every PAIR
> class" and passed, because it compared against a local `const CT_PAIR: u8 = 1`
> — which is `CT_QUAD`. The raw path *is* taken (up to 5 raw bits + a sign), so
> the claim was false; the conclusion was not, because the raw pair arm refills
> between the two coefficients of the group (its second `refill!`, `decoder.rs`),
> so no read run exceeds 16 bits. The 16-bit contract stands on that refill, not
> on `r == 0`. The test now imports the format's real constants, asserts
> `rbits <= 5`, carries a negative control that fails if the class id and the
> bound's subject ever part company again, and (a second, unrelated fix) actually
> reads the four generated corpora, whose paths had an off-by-one `../` and
> resolved outside the repository.

The right frame is not "what does a group cost" but **what is the longest run of
reads between two `refill!`s**, since one halfword load always restores exactly
16 valid bits: a band or coefficient symbol is `len <= 16` (LUT 1..=10,
`huff_slow` 11..=16); an escape field is `t = 2z + 1 + esck <= 13` with `z <= 5`
(`CLZ6`'s fast path) — and `esc!` refills after taking it, so the raw digit that
follows is measured from a fresh window at `r + 1 <= 6`. Every run fits 16, the
nested second load exists only for a run that does not occur, and `esck <= 2` is
likewise what makes the escape guard dead: both `t < 32` and `t <= valid` are
unfalsifiable on our data.

Worth naming, because it is the reason E-R4 must live in an encoder: the window
arithmetic alone would tolerate `rbits <= 15` and `esck <= 5`, and the table
parser accepts `rbits <= 16`/`esck <= 24` — **one bit looser than the product
contract at `rbits == 16`**. The parser is not the guard; only the blob's author
is.

**The premise this project can actually rely on.** The product ROM is
closed-world: we control the encoder, the container and the bake, so *"the
stream is well formed" is an input assumption, not a runtime obligation.* It does
not have to be proven during decode, and it does not have to be proven at open
either — it can be **asserted before the ROM is baked**, in the host gate that
already produces the album and its pins, and it is *free* there because that gate
already decodes the whole album with the checked decoder and compares to the
C-pinned hashes.

This is not a new shape for this tree: it is the pattern already used for the
synthesis kernels — `PortableKernels` (checked, host, the oracle) versus
`ArmKernels` (product, unchecked, `unsafe`). The same split applied to the
entropy reader makes the checked `uli_lite::frame` decoder **the oracle for a
premise**, and the product `dec_chan` the unchecked realisation of it. The
portable decoder keeps every guarantee it has (U2 still requires it to reject
malformed frames without panicking on the host); the product build simply does
not carry them.

**Proposals.** Sizes are estimates until each lands as its own arm; every one of
P1-P5 is bit-exact by construction, so the pins stay the witness.

| # | change | expected | risk | prior art |
|---|---|---|---|---|
| **P1** | Closed-world reader in the product build: **no `end`, no `bad`, no bounds test**. Refill becomes `ldrh …, [next], #2` + shift/or. Frees 2 live registers so `rq`/`dq15` become register-resident and the ~1,300 per-frame stack reloads go | **6–10k/frame** (est.) | low *under the premise* — needs the bake gate to be the only entry point; the checked decoder stays as the oracle | C's structure exactly; the *branchless* and `bad \|=` shapes were falsified, this one was never tried |
| **P2** | Copy `rq` + `ctype`/`rbits`/`esck` into IWRAM (560 B) | **~3.7k/frame**, near-certain | very low — placement only, bit-identical | C's `uli_g`; `iwram-tables` was the same move for the synthesis tables (−10.4%) |
| **P3** | 16-bit contract everywhere (justified by pair `rbits <= 5` plus the raw arm's mid-group refill, which caps every read run at 16 bits): removes the nested second load, and is what makes P1's single-load refill correct | 1–2k/frame | very low — measured across all five corpora | C's 16-bit contract |
| **P4** | Re-test C's `r1 = rq[1] * step` + `m == 1` fast path **after P1/P2** | **FALSIFIED 2026-11-19: +2,310 mean worse** | — | the old note blamed spilling; P1 fixed the spilling and it *still* lost — see below |
| **P5** | `Q3` (81 B) and `CLZ6` (64 B) to IWRAM | **LANDED 2026-11-19: −570 mean / −104 worst** | trivial, +148 B IWRAM | `uli_g_q3` in IWRAM |

**P2 LANDED (2026-11-19) — and the estimate above was 4x too high.** Shipped as
`ClassTables`: a 560 B caller-owned IWRAM object (`ctype`/`rbits`/`esck`/`rq`)
that `GbaDecoder::new` fills once per album, kept separate from `HotState` so the
8,192 B invariant and `reset()`'s per-frame contract both survive. Measured,
bit-exact, pins PASS on fixture and album:

| | before | after | delta |
|---|---:|---:|---:|
| entropy + dequant, mean (bit-exact `entropy2`) | 41,973 | **41,075** | **−897** |
| entropy at its worst frame (f62) | 64,226 | **61,728** | **−2,498** |
| 129-frame product frame, worst / mean | 205,793 / 179,058 | **203,394 / 178,168** | −2,399 / −890 |
| album decode wall, worst / mean | 206,101 / 175,280 | **203,913 / 174,532** | −2,188 / −748 |
| gate total, worst / mean | 237,646 / 206,637 | **235,418 / 205,844** | −2,228 / −793 |
| gate margin at the worst frame | 9.35 pts | **10.20 pts** | +0.85 |

`.iwram` grew **784 B**, not 560: the table is 560 (the size assertion confirms
it) and `dec_chan` grew **196 B** (0x1388 → 0x144c) because `class` arrives as a
third pointer argument. Stack high-water 964 → 948 B.

**Why the 3.7k projection was wrong, in the informative direction.** It counted
the access (`ldrsh` from EWRAM, 4 cycles vs 1) and not what the access sits
behind: the disassembly shows `ldr r4,[sp,#80]` — the **`rq` *pointer* reloaded
from the stack** — then `add`, then `ldrsh`, then `mul`. Moving the table to
IWRAM removes the last link and leaves the first two, and the extra pointer
argument adds to the very register pressure that causes the spill. So the win is
real and density-shaped (−2,498 at the densest frame against −897 on the mean),
but it is a fifth of the projection, and the remaining `rq` cost is **P1's** —
it does not go away until `rq`/`dq15` are register-resident.

**P1 LANDED (2026-11-19), with P3 folded in — and this time the projection held.**
The product bit reader is now **closed-world**: no `end`, no `bad`, no bounds test,
no per-symbol `len == 0` test, and a **16-bit** refill contract instead of 17
(P3, justified by the `rbits <= 5` measurement above together with the raw arm's
mid-group refill), which is what makes one load always sufficient. The checked
reader survives behind `checked-reader`, and
every attribution image now builds with it through `attrib-dump` — a by-difference
arm desynchronises the stream and needs `bad` to resynchronise, and keeping the
control image on the same reader keeps the deltas comparable to each other (their
*absolutes* were never the product number).

| | before | after | delta |
|---|---:|---:|---:|
| 129-frame product, worst / mean | 203,394 (f83) / 178,168 | **193,421 (f98) / 172,748** | −9,973 / −5,420 |
| album decode wall, worst / mean | 203,913 / 174,532 | **195,439 / 169,851** | −8,474 / −4,681 |
| gate total, worst / mean | 235,418 / 205,844 | **226,853 / 201,126** | −8,565 / −4,718 |
| gate total, % of period | 89.80% / 78.53% | **86.54% / 76.72%** | −3.26 / −1.81 points |
| margin at the worst frame | 10.20 pts | **13.46 pts** | +3.26 |
| `dec_chan` code | 5,196 B | **3,820 B** | **−1,376 B** |

`.iwram` *shrank* 1,376 B — the closed-world reader deletes more code than the
registers it frees cost — so the P1 register story came with memory back.

**The projected 6–10k was right, unlike P2's.** The worst frame lands at ~9,973
and the mean at ~5,420: the win is again density-shaped, but this time the
mechanism *was* the address chain and the spills, which is what the projection
counted. `end`/`bad` are no longer live inside the coefficient loops, so
`rq`/`dq15`/`step` are register-resident and the per-symbol `ldr [sp,#56]` /
per-coefficient `ldr [sp,#80]` / `ldr [sp,#76]` reloads are gone.

**Entropy/dequant is now at or below C, by subtraction.** The bit-exact arm reads
41,076 on the *checked* reader and nothing but the reader changed, so the product
block is **~35,656 mean** (41,076 − 5,420) against C's 42–48k. The mean is now
ahead of the reference; what remains is the densest frames.

**The premise is now an assertion, not a comment.** `crates/uli-lite-gba/tests/premise.rs`
checks `rbits <= 5` on every PAIR class and `esck <= 2` on every class, for the
committed fixture and for all five generated corpora when present — all of them
measuring `max rbits == 5`, `max esck == 2` on all of them (the class structure is
`tables_default`'s apart from one trained tightening; the length rows differ, as
they must). It also witnesses itself: a negative control rejects a PAIR class at
`rbits == 6`, so the
green cannot come back from checking nothing (it had, once — see the correction
under Cause 3). That is the seed of the bake gate: it is a property of the
**tables in the blob**, so it belongs wherever the shipped album is produced, and
it must run before the ROM exists rather than in it.

**P4 is FALSIFIED (2026-11-19) — and the falsification is the interesting
result.** The change was built exactly as the reference has it: `r1 = rq[1] *
step` hoisted out of the quad group loop, and digit 1 decoded as a sign bit and
nothing else. Bit-exact, pins PASS, and **slower on both corpora**:

| | baseline (P1) | with P4 | delta |
|---|---:|---:|---:|
| 129-frame, worst / mean | 193,421 / 172,748 | 199,121 / 175,077 | +5,700 / **+2,329** |
| album decode wall, worst / mean | 195,439 / 169,851 | 199,532 / 172,161 | +4,093 / **+2,310** |
| gate total, worst / mean | 226,853 / 201,126 | 230,976 / 203,506 | +4,123 / **+2,380** |

Two shapes were measured, because the first one was suspected of being the
problem rather than the idea: the fast path *nested inside* `if mm != 0`
(+2,106 mean) and *tested first*, making C's own two-branch chain `if (m == 1) …
else if (m)` (+2,329). Both lost, so it is not the branch nesting.

**Why the old rejection was right for a reason that had already stopped being
true, and then became true again the other way round.** The note said the hoist
lost because *"its two operands are spilled"* — a symptom of the reader's
register pressure. P1 removed that, so the item deserved the re-test. But P1 and
P2 also **removed the cost the fast path exists to avoid**: `rq[1]` is now an
IWRAM read in a register (1 cycle, not a 4-cycle EWRAM load behind a stack
reload) and the multiply's `Rs` is the small dequant value (1–2 cycles). What is
left is smaller than the price of skipping it — ARM7TDMI charges 3 cycles for a
taken branch and has no predication, and the digit is data-dependent so it
predicts poorly. **P4's value was consumed by the two PRs that unblocked it.**

That is the same lesson the 2026-11-16 loose-ends sweep learned from the other
direction (there: *"almost every item is a branch"*; here: an item that looks
like arithmetic is really a branch once the arithmetic gets cheap), and it is
why an item must be re-sized **after** the changes that alter its inputs, not
before.

**P5 LANDED (2026-11-19): −570 mean / −104 worst** — `Q3` and `CLZ6` moved from
`.rodata` (cartridge) to `.iwram`, as `static` rather than `const` so each is one
object with an address instead of a value the compiler may re-emit at every use.
Album **194,695 / 169,354** (was 195,439 / 169,851); gate **226,152 / 200,624 =
86.27%**, margin **13.73 points**. Cost: **+148 B of IWRAM** (145 B of table).
The projection (0.5–1k) held.

**The sub-frame partition, re-run (2026-11-19, in PR #165).** Mean cycles/frame
on the 129-frame fixture, each arm removing one layer from the full path:

| layer | pre-P2/P5 | **re-run** | share |
|---|---:|---:|---:|
| symbol resolution | 27,036 | **26,175** | 58% |
| loops, escapes, sign bits, band control | 16,350 | **16,035** | — |
| dequant | 9,285 | **8,774** | — |
| residual | 4,954 | **5,187** | — |
| reader mechanics | 2,791 | **2,793** | — |
| coefficient store | 2,111 | **2,003** | — |
| **full path** | 46,631 | **45,381** | 100% |

**The shape is unchanged — symbol resolution is still ~58% of the entropy path —
and the absolute moved only 1,250** (P2's −897 and P5's −570 on this reader).
Two caveats come with that, and the second is the important one:

1. **This is the *checked* reader.** `attrib-dump` now implies `checked-reader`,
   because the `nolut` and `reader` arms desynchronise the stream and need `bad`
   to resynchronise. So these are not the product's numbers: the product's
   entropy block is ~35,656, and the ~9,700 difference is exactly what P1
   removed — the per-symbol LUT-pointer reload and the per-coefficient
   `rq`/`dq15` reloads, which every arm here still pays.
2. **Only two of the six arms desync.** `nolut` and `reader` substitute or remove
   the symbol decode, so the bit cursor diverges and what follows is a different
   workload. **`nodequant` and `nowrite` do not touch the reader at all** — they
   remove the dequant arithmetic and the store — so they are sound on any reader.

**So the next instrument step is not another optimisation.** It is to decouple
`attrib-dump` from `checked-reader` so `nodequant`/`nowrite` can be built against
the **product** reader, and to add a symbol-resolution arm that does not desync
(the 10-bit LUT covers every code on this corpus — `slow_hits=0` — so a
length-preserving substitution exists: read the same number of bits, return a
fixed symbol). Until then "symbol resolution is 58%" is a statement about a
reader the product does not ship, and sizing work from it would repeat P2's
mistake in the other direction.

**Where this leaves the gate against C.** The reference's own record gives
comparable figures — decode wall **69.43%** and gate total **73.63%** at 4/2 — so
the two halves can be compared for the first time:

| | C + ASM | ours (after P5) | delta |
|---|---:|---:|---:|
| decode wall, worst | 182,007 (69.43%) | **194,695 (74.27%)** | **1.070x**, +4.84 pts |
| gate total, worst | 193,017 (73.63%) | **226,152 (86.27%)** | 1.172x, +12.64 pts |
| **non-decode half** (gate − decode) | **11,010 (4.20 pts)** | **31,457 (12.00 pts)** | **2.86x** |

**The decoder is no longer where the gate gap lives.** Of the 12.64 points we are
behind C on the gate, 4.84 are decode and **7.80 are the convert/IRQ/DMA path**,
which costs ~2.9x C's. C's figure also errs *high* (their doc notes the DMA share
is counted twice in the gate total), so the true gap is at least as large. That
makes the player's output half — not the codec — the largest remaining single
difference from the reference, and it is where the next measurement should go.

**P4 deserves its own paragraph, because the record currently treats it as
settled.** `coef!` carries *"C hoists `rq[1] * step` and special-cases `m == 1`
here. Measured in this image, that hoist is not a win — its two operands are
spilled."* That is a description of Cause 1, not a property of the change. The
fixture is **248 quads and 0 pairs**, so `m == 1` is the most common digit in the
stream and the hoist would remove both an **EWRAM** table read and a multiply
from it. It must be re-measured once `rq` and `step` are register-resident.

**What the premise re-opens.** Three items the 2026-11-16 sweep declined
*for safety reasons* become free once the product build carries no error path:

| item | why it was declined | under the premise |
|---|---|---|
| `huff_consume!` length test | "removing it needs a failure signal that is not a per-symbol branch" | no failure signal is needed — ~624 symbols/frame x the branch |
| `esc` fast-path guard | "~2 compares per escape; the parser's `esck <= 24` bound makes `t < 32` live" | measured `esck <= 2`, so `t <= 13` and both compares are dead |
| `huff_slow` "no match" return, band-level `if bad`, `k > MAXK` early-out, base-alignment check | each guards a premise | all become **bake assertions** |

**How each would be falsified.** `attrib-entropy2` gives the clean per-frame
total and every one of P1-P5 is bit-exact, so the pins PASS and the two-runs-to-
the-cycle rule applies unchanged. P2/P5 are readable off the section table
(`objdump -t | grep .iwram`) as well as off cycles. P1 needs a *positive* witness
for its premise, which is the one genuinely new piece of work in the list: the
bake gate must decode the whole album with the checked decoder and fail the
build, not the frame, on any rejection — and the unchecked product build must
still reproduce the pinned hashes.

**Caveats.** C's 21–24k is measured on *their* corpus, ours on the fixture: the
ratio is a diagnostic and the comparison of record is the structural table. The
sub-frame arms are timing-only and `nolut` desynchronises the stream. And P1's
premise is about *our* encoder: a third-party or re-encoded blob with different
tables is not covered unless the bake assertions cover it.

> **One caveat withdrawn (2026-11-19).** This paragraph also said "the fixture has
> **zero pairs**, so two of the five inlined `coef!` expansions never execute
> there — `cross` should confirm before P3 is sized." It is false, and it is the
> same class-name transposition wearing a different hat: believing the only PAIR
> classes were the two with `r == 0` implies the raw pair arm is dead code on our
> data, and therefore that two of the five `coef!` expansions never run. `make
> uli-frame-ops` on the fixture counts **15,984 pair groups and 10,480 raw-field
> reads** over its 129 frames (`adv`: 78,104 / 8,016; `biomaster`: 1,626,832 /
> 670,464 — frame counts match the Cause 3 table, so these are the same corpora).
> Both pair arms and the quad arm execute on the fixture, so all five expansions
> are covered there and P3 needed no confirmation from `cross`. The product path
> executes the same symbols as the portable decoder that produced these counts —
> that is what `make uli-overlay-test`'s full-corpus bit-exact differential
> witnesses.

## The encoder — the last missing piece (plan, 2026-11-19)

**Why it blocks the album player.** Everything downstream of the decoder is
ready: `uli-playback` already implements the app's `PcmSource` seam
(`UliSource<'a, D: FrameDecoder>`), `ALBUM-PLAYER.md` §4.2b already specifies
`src/backend/uli2.rs` under `make album-rom BACKEND=flac|uli2`, and the ULA2
container already carries tracks, titles, artists and a seek table. What is
missing is the ability to **make** a `.uli` blob from our own audio: today that
requires the C encoder in `gba-audio`, which the project's rule forbids depending
on. Until an in-repo encoder exists, ULI2 cannot be used as the album player's
codec — the assets would come from a build we do not own.

**What the reference is.** ~890 lines of C plus two Python tools, and the shape
matters more than the size:

| piece | what it does |
|---|---|
| `enc_core.c` | the codec: `tables_default`/`tables_derive`/`tables_from_stats` (Huffman lengths trained from counts, with a floor), the bit writer (`bw_put`, `bw_put_eg`, zigzag), the forward MDCT (`mdct_frame`, orthonormal, hop 512, coefficients ≥500 zeroed), and `frame_code` — an **RD-optimised** per-frame search over the global step `g` and the M/S flag, cost `D + kappa·bits + mu·decoder_cycles` |
| `uli_enc.c` | the CLI and album assembly: `--budget`/`--cap %` sizing, two-pass table training, per-frame parallelism, ULA2 container write |
| `prep.py` | `ffmpeg` decode, resample to 32,768 Hz, mono→stereo, gapless track join, `tracks.tsv` with ASCII-folded titles |
| `gen_tables.py` | regenerates `src/uli/uli_tables.c` — the **synthesis** constants (PERM/PRE/POST/TW/WIN, variant P, N = 1024 / M = 512) and the step table `uli_step[g] = round(2^(g/8))`. **It does not emit the code tables**: the default `ctype`/`rbits`/`esck`/`mu` rows are C literals in `enc_core.c:tables_default`, and the default lengths come from a geometric model through `huff_lengths(·, 0.5)` |

Two of those deserve emphasis because they are not obvious from the format:
`mu` makes the encoder **cost the decoder's cycles in the RD objective**, so it
can trade a little distortion for decode speed — a knob this project values more
than a general-purpose codec would; and the tables are **trained per album**, so
the encoder writes the Huffman codebooks into the blob.

### Requirements

| # | requirement | why |
|---|---|---|
| **E-R1** | In-repo **Rust**, host-only, no C, no `gba-audio` dependency at build or encode time | the standing rule; and a C oracle we cannot ship is not an encoder |
| **E-R2** | **The contract boundary is PCM**: `album.f32` + `tracks.tsv` in, one blob out. `album.f32` is float32 **interleaved stereo at PCM16 scale** (samples × 32768), tracks joined gaplessly at 32,768 Hz; `tracks.tsv` is `n start len title artist` in 32,768 Hz samples. A WAV front end (PCM 8/16/24/32, mono or stereo, any rate) is a convenience layer on top, and the resample inside it is **not** held to byte-equality | the reference's own CLI takes exactly this pair, `uli-album-asset` already feeds it, and a source already at 32,768 Hz needs no resampling at all (`gen_fixture.py` emits the fixture straight into this form). Holding byte-exactness at the PCM boundary makes it testable; holding it at the WAV boundary would make it depend on which resampling library answered — see "The acceptance question" |
| **E-R3** | Writes exactly the ULA2 container `uli-lite`/`GbaDecoder` parse: one blob, ≤32 tracks, per-track title/artist/preroll/bitpos, seek table every `SEEK_INTERVAL` frames, strings, tables blob | anything else is a second format |
| **E-R4** | **The encoder owns the closed-world premise: `rbits ≤ 5` on every PAIR class and `esck ≤ 2`.** Refuse to emit otherwise | the product reader has a 16-bit refill contract and no error path; a raw pair coefficient costs `r+1` bits after the group's refill, so `r ≤ 5` keeps 10 bits of margin. `esck ≤ 2` caps the escape field at `t = 2z+1+esck ≤ 13`. Both are **tighter than the window needs** (`r ≤ 15`, `esck ≤ 5`) and far tighter than the *parser* allows (`rbits ≤ 16`/`esck ≤ 24` — note 16 is already one bit past the contract); only the encoder can make the product's premise true. Measured maxima on all five corpora: `r = 5`, `esck = 2` (`tests/premise.rs`) |
| **E-R5** | **Deterministic**: identical input + identical flags → byte-identical blob, at any `--threads` and on any host, by the four rules in decision 2 below | pins are the whole verification story, and byte-exactness (the acceptance gate) is unreachable without it |
| **E-R6** | **Size is an argument, not a fact about the cartridge**: `--budget BYTES` (default `33_292_288` = 32 MiB − 256 KiB, the reference's own default) and `--cap PCT` (default **74**, i.e. % of the 262,144-cycle period). Compute the container's fixed overhead and the resulting bits/frame target, and **refuse before encoding any frame** if the settings cannot fit — whole album, exit nonzero, both numbers cited | the ROM holds code, art, `viz` streams and more than one asset producer, so no encoder can know the cart layout; and per `ALBUM-PLAYER.md` §10.1 **art and reaction data are carved out of the audio budget, never the other way round**, which is exactly why the audio budget has to be an input |
| **E-R7** | Quality parity with the reference on identical PCM: codec-only SNR within a stated tolerance of **32.29 mean / 25.78 worst dB** at cap 74 | otherwise the format's own record stops describing our encoder. Note the metric is **codec-only**: the reference CLI's own `verify:` line prints a different thing (full-band time-domain SNR of the decoded album against the resampled input, at pcm16 and int8 — e.g. `31.78/30.80` for `biomaster`), so the two numbers are not comparable and must not be quoted against each other |
| **E-R8** | The RD search carries the decoder-cycle term (`mu`), at least as an option | it is the project's distinguishing knob and it feeds the playback-loop budget |
| **E-R9** | A 43-minute album encodes in **minutes**: frame-parallel (parallel compute, ordered emit — decision 1) | the bake is in the edit loop |
| **E-R10** | **Never writes a ROM, and never learns that one exists.** Its output is a file, sized against `--budget`; the aggregation into a cartridge belongs to `scripts/album_bake.py`, which owns `cart_bytes` (§9) and will call several independent asset producers | the tools are independently configurable precisely because the set of assets per ROM is not fixed |

### Three decisions, settled 2026-11-19

The three questions the plan left open (writer shape, determinism mechanism,
refusal semantics), each cheap **only** if taken before the code exists. Two
placements they imply — where the write side lives, and where the E-R4 bounds
live — are under "Where it lives"; the boundary at which quality and
byte-exactness are judged is under "The acceptance question".

**1. The writer plans, then emits — and the byte target is an argument.**
One dependency has to be settled at E1 because it is not obvious: **the default
code tables are not in this repository.** `tables_data.rs` carries the synthesis
constants only, and the default `ctype`/`rbits`/`esck`/`mu` rows are C literals in
`enc_core.c`, which may not be vendored as C source. The precedented route is the
one `tables_data.rs` already takes — transcribe the integers into a Rust module with
a provenance line naming the source, under `publish = false` — and the committed
fixture then *witnesses* the transcription, since its parsed class structure must
equal it (`mu`'s own row, `{0, .12, .45, .8, 1.3, 2, 3, 4.5, 6.5, 10, 20, 40, 80,
160}`, is data of the same kind and is needed to regenerate the default lengths).

The shape of the writer itself:
Two shapes were on the table: *stream as you go* (encode frame 0, append; encode
frame 1, append) or *plan then emit* (fix tables and the per-frame `g` plan, size
every frame, then write frames in index order). **Plan then emit.** Note that the
streaming shape was never actually available: every `SEEK_INTERVAL`-th frame needs
a container entry carrying that frame's bit position and its `g`, and `g` is
differentially coded across frames (`g = g_prev + zigzag(z)`, `frame.rs`), so the
encoder must know every frame's size and every frame's `g` before it can write the
header. What the decision really fixes is *where that requirement gets designed
in* — at E1, where the writer is new, rather than at E5, under tests, measurements
and pinned hashes. It also settles the parallel shape: because the `g` plan comes
from the allocation stage, the frame bodies are mutually independent once it is
fixed, which is what the reference itself does ("final encode (parallel chunks,
concatenated)").

**2. Determinism, stated as four rules** (the mechanism behind E-R5):

| # | rule | cost |
|---|---|---|
| R5a | threads **compute**, never **order**: any work may be parallel, assembly walks frames by index | none — forced by decision 1 |
| R5b | every sum that feeds a decision is reduced in **frame-index order**, never in completion order | a few percent in one loop (IEEE addition is not associative, so an unordered float reduction can flip a near-tie and change every byte after it) |
| R5c | transform constants come from **checked-in integer tables**, not from libm at encode time | one generator script + a checked-in file; the precedent is `uli-lite/src/tables_data.rs` |
| R5d | every tie names its winner (lower `g`; then L/R over M/S) | none |

The witness is cheap and belongs to E1: encode the same input at `--threads 1`
and `--threads N` and assert the same SHA-256.

**3. Refusal is early, whole-album, and arithmetic.** Compute overhead → derive the
bits/frame target → check the plan → *then* encode, which is the order the
reference uses (`container overhead 5880 bytes; stream target 26304896 bits =
1589.1 bits/frame`). If the chosen `--cap`/`--budget` cannot fit, refuse the whole
album with a nonzero exit citing needed and allowed, in the style of the bake's
other refusals ("with a cited number") — **not** by trimming quality on some
tracks until it squeaks in, since a silent squeeze both varies by input and breaks
E-R5. And `--budget` means **blob bytes**, never cart bytes: the bake is what
subtracts code reserve, art and `viz` from `cart_bytes` and passes the remainder in.

### Where it lives

`crates/uli-enc` — host-only, `publish = false`, depending on `uli-lite` for
`format` and the `Huffman`/`CodeTables` types so the bitstream is defined
**once** and the encoder cannot disagree with the decoder about it. A `uli-enc`
binary is the CLI. Python remains for orchestration only (source decode,
resample, track metadata), which is what `ULI2.md` already says: the rule is
"no C tooling", not "no Python". The one thing that must not happen is a second
copy of the format constants — that is exactly how these docs drifted.

**`uli-lite` stays read-only, and that is intentional.** It is the codec as the
GBA needs it: parse, decode, no writing. So the *write* side of the format lives
in `uli-enc` and nothing is added to `uli-lite` to support it. One consequence is
worth stating rather than discovering later: the tables-blob geometry is not
named constants today, it is arithmetic inside `CodeTables::parse_into` —
`1 + 3k + (k+1)² + k·(81+256+16)` bytes, with class-row offsets `lq = p` and
`lp = lq + 81k` — so `uli-enc` reproduces a layout it is not handed. That
duplication is **tolerated** because it is self-witnessing — get an offset wrong
and `parse_into` rejects the blob — unlike
the bounds and defaults, which drift silently. The residue that a round-trip does
*not* catch is a blob that parses cleanly but differs from the reference's bytes
(row order, padding), and that is caught only by the acceptance gate below.

**The E-R4 bounds get one home, not three.** They currently exist as a private
helper inside `crates/uli-lite-gba/tests/premise.rs`, where the encoder cannot
reach them; repeating `5` and `2` a second time in `uli-enc` is the failure mode
this file has already documented. Promote the predicate to `uli-lite` as
validation of *already-parsed* tables — `format::ENC_PAIR_RBITS_MAX` /
`ENC_ESCK_MAX` plus a `CodeTables::premise()` returning an allocation-free
`Result<Census, Violation>` (`uli-lite` is `no_std`; the host callers format) — and
have the test, the encoder and the bake gate all call it. Validation is read-side,
so this does not reopen the paragraph above.

### Slices, each with an exit gate

| # | slice | exit gate |
|---|---|---|
| **E1** | Skeleton: PCM (`album.f32` + `tracks.tsv`) reader, container/bit-writer written **plan-then-emit** (decision 1), `tables_default`, the overhead/budget arithmetic of decision 3 even while nothing can yet overflow it. Encode silence and DC | blob parses (`uli_lite::Album::parse` + `CodeTables::parse_into`); decodes; the shared E-R4 check (see "Where it lives") is called by the encoder itself; `--threads 1` and `--threads N` give the same SHA-256 (R5a/R5d) |
| **E2** | Forward MDCT + fixed-step quantisation with default tables | decodes bit-exactly; SNR sane; rate reported |
| **E3** | RD search: `g`, `ms`, `kappa`; per-frame bit accounting | rate/quality on the fixture vs the reference's curve (E-R7) |
| **E4** | Two-pass table training (`tables_from_stats`, length floor) | output byte-stable across runs; trained tables still inside E-R4 |
| **E5** | Album assembly: tracks, titles, seek table, `--budget`/`--cap` wired to real sizes, the pre-flight refusal, and the **WAV/resample front end** that E1 leaves out (E-R2's convenience layer, held to SNR only) | a 43-minute album encodes inside a **passed-in** `--budget` at cap ≤ 74; every track decodes; hashes pinned; a budget chosen too small refuses **before** the first frame is encoded, citing both numbers; an arbitrary WAV reaches the same blob as its pre-resampled PCM would not, but its SNR is quoted |
| **E6** | `mu` (decoder-cycle-aware) search, optional | a knob that demonstrably trades SNR for decode cycles, measured on the bench |

### The acceptance question, answered (revised 2026-11-19 — it is measurable, and it was measured)

**Target gate: given the same PCM and the same flags, the Rust encoder reproduces
the C encoder's blob byte-for-byte.** The plan first treated this as a hope with a
documented fallback; it is now a gate that runs today. Re-encoding off-tree with
the reference at `gba-audio/encoder` reproduced two committed corpora **exactly** —
the 129-frame fixture and the 16,553-frame `biomaster` album — and `--threads 1`
agreed with `--threads 8` byte for byte. The dated entry, with commands and
hashes, is in [`RECORD.md`](RECORD.md) → "the reference encoder reproduces its own
corpora".

So the gate stands, with three refinements:

- **Measured at the PCM boundary** (`album.f32` + `tracks.tsv` → blob), not at the
  WAV boundary. The earlier reason for exempting the resampler was the wrong
  reason: it is not that `ffmpeg`'s filter is unmatchable, it is that the
  reference's own PCM preparation is **library-dispatched** — `prep.py` calls
  `gba_dac_model.resample`, which takes its FFT path when the rate ratio's
  denominator fits in 2¹⁶ and otherwise falls through to `soxr` *if installed*,
  then to `scipy.resample_poly`. The same flags can therefore yield different
  bytes on two machines depending on which library answers, which is not a
  property worth pinning a codec gate to. A source already at 32,768 Hz bypasses
  the resampler entirely — which is why the fixture has no resampler in its path.
- **The proof is a matrix, not an exhaustive sweep.** Fixture at `--threads 1`;
  `biomaster` at its build settings (`--budget 3294000 --cap 74 --threads 8`);
  one point from `adv` or `cross`; **plus one deliberately off-default sweep**
  (say `--cap 60`, budget halved). Without that last cell both encoders could
  agree by sharing a lucky path and the gate would pass for nothing.
- **It is host-qualified.** The reference builds with `-O2 -march=native`, so
  "byte-identical" means byte-identical *on the reference host* (this machine,
  where both reproductions above ran). Fine for a dev gate; worth naming so nobody
  reads it as a cross-platform guarantee — which is also why R5c keeps libm out of
  our own path.

**Standing on this gate does not break E-R1, and this needs saying.** E-R1 forbids
a *build, link or vendoring* dependency on `gba-audio`; it has never forbidden
running the reference as an oracle, which is what `uli-album-asset` already does
when it shells out to `$(GBA_AUDIO)/build/uli_enc` to make a corpus. So the
byte-exactness gate is a host target that builds nothing of ours from C source,
calls the off-tree binary, and compares SHA-256s — the same posture the project
took for the decoder's C-pinned hashes, with the oracle one notch further out.

If exactness nevertheless proves unreachable on some input, the fallback gate is
unchanged in shape: **same rate and quality within tolerance, all E-R4 bounds
held, and a bit-exact decode** — with the C-produced blobs frozen as the reference
corpus and the Rust encoder's output pinned separately. But it is now a fallback
for a *cell* of the matrix, not for the gate: the reproductions above show the
reference is reachable on both a synthetic fixture and a real album.

Two aids fall out of the same experiment, and taking them makes a future mismatch
localise instead of becoming a bisect: the reference prints its intermediates
(`bits/frame`, λ per training iteration, the `retrain … (±x%)` delta, `g range
89..135`, `M/S frames 87.1%`, `verify: decoded N frames OK`, SNR), so `uli-enc`
should print the same columns; and `musearch` showed **no activity** on either
reproduced corpus, so E6 stays off the byte-exactness critical path *for these
two* — re-test on `adv`/`cross`, which are denser, before calling it optional for
albums.

### What it does not have to do

Not a general-purpose codec; not portable (host only); no encoder-side seeking or
streaming; no obligation to accept non-WAV input directly; it does not validate
its own output — `uli-lite` is the oracle, in the same round-trip test that
already exists for the decoder. And, per decision 3 and **E-R10**: it does not
write ROMs, does not know `cart_bytes`, and does not decide what else shares the
cartridge. It is one asset producer among several, and the bake aggregates them.

## Gate verdicts (the claims live in ULI2.md §Gates)

**One owner per topic:** the gate *claims* are stated in
[`../../ULI2.md`](../../ULI2.md) §Gates, which is the acceptance record. This is
the dated history of their *verdicts*, which belongs to the ledger because every
one of them moved on a measurement.

| gate | verdict | when |
|---|---|---|
| U1 — reference | met; redefined to "oracle, not portable product" | 2026-11-14 |
| U2 — bit-exact host | met from Phase 0 | 2026-11-13 |
| U3 — bit-exact product | met for the kernel set | 2026-11-14 (PR A) |
| U4 — real-time decode | met — decode wall under the period | 2026-11-15 (Step 1) |
| | met with the gate total and 13.73 pts of margin | 2026-11-19 |
| U5 — product playback | met on hardware (GBA SP), three ROMs, 0 underruns; one open defect (104 late re-arms from the 1 Hz redraw) | 2026-11-16 |
| U6 — memory | **corrected, then met**: the "≈15.0 KiB stack" figure was an artifact of stack-local twin buffers and is withdrawn; the measured high-water is 948 B (bench) / 1,988 B (gate), `.iwram` 26,496 of 32,768 B | 2026-11-15, 2026-11-19 |
| U7 — licensing/spec | deferred by decision: `publish = false`, no license work | 2026-11-14 |

---