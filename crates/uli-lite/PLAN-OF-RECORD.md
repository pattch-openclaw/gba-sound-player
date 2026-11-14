# ULI2 plan of record

Status: **live since 2026-11-14; PR C open, U4 not met.** This is the working
plan for the ULI2 product codec. [`../../ULI2.md`](../../ULI2.md) remains the decision record
(why ULI2, what it is, the licensing and encoder risks); this file owns the
phases, gates, measurement ledger and the order of work.

The Phase 1b measurement that forced this rewrite is recorded in
[`../../ULI2.md`](../../ULI2.md) → “Phase 1b — what the on-target measurement
found”. The instrument is [`../../examples/uli_bench`](../../examples/uli_bench)
(`make uli-bench`).

---

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

- **Kernels are 2.4–2.7× the C/asm reference** (328,502 vs ~146,000 for the
  C `asm_all` row). The FFT butterfly alone spills 66 intermediates to the
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
| synthesis tables in IWRAM (`iwram-tables`) | **crashes**: `.iwram` 18,848 B leaves 13,664 B of stack against ~15,040 needed, so the stack overruns `__iwram_end` into code and the CPU jumps to an invalid address (the same failure class as PR B's hang). Quantified in the U6 row. |

Gate: **≤1×** worst. **Not met by this PR** (523,267 = 200%); it is a
measurement, one small win, and the scope statement for the two rewrites
below. U4 stays open.

#### PR C-1 — asm kernels (the 63%)

Hand-written ARM kernels in `.iwram`, twinned bit-exactly against the Rust
reference (the existing `tests/twin.rs` and `tests/decoder_twin.rs` are the
witness). Target the C `asm_all` row: ~146,000 cycles/frame for the pair.
Prize: ~182,000 cycles/frame.

#### PR C-2 — entropy rewrite (the 33%)

Redesign the bit-reader protocol. The 32-bit cached window from PR B is in
place, so this is not “more of the same”: candidate directions are holding the
reader state in registers across a whole band instead of behind `&mut`, a
byte-oriented refill path, and folding `huff`+`skip`+sign into one symbol
routine. Prize: ~90,000 cycles/frame.

Together C-1 and C-2 project to ~247,000 cycles (~94%), which is the first
plan that reaches U4's ceiling. Neither is a tuning change, which is why PR C
splits into them rather than closing here.

### PR D — playback (after U4)

- Reuse the 32,768 Hz ring/FIFO feeder; int8 direct and optional int16 speaker
  path; UI from the ULA2 tables; `PcmSource` seam.
- Gate: U5 (zero underruns, clean period audit) in mGBA, then hardware.

---

## Gates ledger

| Gate | Claim | Status |
|---|---|---|
| **U1 — reference** | `uli-lite` remains a bit-exact, host-testable reference (no longer a portable-product claim). | Met (Phase 0/1); redefined 2026-11-14. |
| **U2 — bit-exact host** | The reference matches the C-pinned corpus, frame for frame, including error paths. | Met. |
| **U3 — bit-exact product** | The product kernel set produces the pinned int8/int16 frames, on target and in the host twin. | **Met for the full hot path (PR B).** `tests/product_twin.rs` (host, frame-for-frame) + `make uli-bench` on-target pins. |
| **U4 — real-time decode** | Worst full frame within the product budget at WAITCNT 4/2. | **Not met.** PR B measured 204% worst / 177% mean; PR C added the attribution and a split-`huff` win to **200% worst / 174% mean** (523,267 / 456,187) and scoped the remainder as two rewrites (C-1 asm kernels 63%, C-2 entropy 33%). Projected ~94% when both land. |
| **U5 — product playback** | 32,768 Hz path, zero underruns, clean period audit. | Not started (gated by U4). |
| **U6 — memory** | IWRAM/EWRAM/stack inside the product budget with UI and feeder. | **Open, and now the binding constraint.** The product path needs ≈15.0 KiB of stack. Budget at PR C: `.iwram` 11,072 B baseline, 32,512 B usable, so 21,440 B spare — but 8,672 B of synthesis tables in IWRAM leaves 12,768 B, under the stack need, and `iwram-tables` **crashes** (stack overruns `__iwram_end` into code; the CPU jumps to an invalid address). IWRAM tables are therefore blocked until the stack requirement or the code size falls. `HotState` and the tables are both EWRAM. Full `huff` inlining was rejected on this row: +4,252 B for 2.7% (U6 is binding), vs +896 B for 1.6% with the split. |
| **U7 — licensing/spec** | Permissive license + standalone format spec before publishing; otherwise `publish = false`. | Open; crate stays `publish = false`. |

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
Kernels against the C `asm_all` row (~146,000) are 2.25×; entropy per
coefficient against the C bench's entropy+dequant (~41 c/coef) is ~6.6×.

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
   `flac-lite-gba`) — PR C-1. The same is true of the OLA (2.5x).
2. **IWRAM budget — and the stack is the real limit.** The reference spends
   ~22.6 KB (17.6 KB tables/buffers + 5 KB ARM code) of 32 KB. PR B measured
   that the product path needs **≈15.0 KiB of stack** (floor `0x03004440`), so
   an 8 KiB `HotState` in `.iwram` left ≈14 KiB between `__iwram_end` and the
   stack top and overflowed (a silent hang at `pc=0x4`). `HotState` is EWRAM
   in the bench. Any move to put hot buffers back in IWRAM must budget stack
   first, not just code+data. Tables-in-IWRAM measured only 6.3% and stay off
   by default.
3. **Oracle drift.** The core must stay bit-exact and host-testable; every
   fast-path change is twinned. Do not “optimize” the oracle.
4. **Licensing.** `gba-audio` has no LICENSE; `publish = false` until settled.
5. **Encoder independence.** The C encoder remains the encoder of record until
   the Rust encoder milestone.
6. **Playback is not implied by decode.** A fast decoder is not a player;
   U4 must pass with margin before Phase D starts, and U5/U6 are separate.

---

## Decision log

- **2026-11-14 — PR C: measured the gap instead of guessing at it.** Frame 101
  (the worst) is **63% synthesis kernels / 33% entropy (`dec_chan`) / 4% M/S +
  wrapper**. Four cheap levers were built and run on target: fusing dequant was
  neutral, full `huff` inlining bought 2.7% for 4,252 B of IWRAM (rejected on
  U6), a split `huff` LUT fast path bought 1.6% for 896 B (kept), frame
  pointers were inert, and `iwram-tables` crashed because 8,672 B of tables
  leave less stack than the ~15,040 B the path needs. Consequence: the product
  frame is **523,267 worst / 456,187 mean (200% / 174%)**, and U4 stays open
  with the remaining work scoped as PR C-1 (asm kernels, ~182,000 cycles) and
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
