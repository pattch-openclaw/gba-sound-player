# ULI2 plan of record

Status: **live since 2026-11-14**. This is the working plan for the ULI2
product codec. [`../../ULI2.md`](../../ULI2.md) remains the decision record
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
  src/lib.rs (PR B)         the hot bit reader, Huffman, dequant and M/S
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

### PR B — GBA-first hot path (next)

This is where the retired constraints are exercised.

- Move the hot bit reader, Huffman decode, dequant and M/S into
  `uli-lite-gba`.
- 32-bit cached bit window (no 64-bit shift helpers), sentinel `error` instead
  of `Result` per symbol, unchecked pointers with documented bounds, ARM
  state, `.iwram` code, hot buffers (`x`, `buf`, `p`) in IWRAM, cold paths out
  of line.
- Keep the core as the oracle: frame-for-frame host twinning over the corpus,
  and the album pins on target.
- Gate: **full-frame worst ≤2× the 262,144-cycle period**, pins unchanged.
  Record the ladder from PR A's number.

### PR C — close the gap (scoped by PR B)

- Tuning, block moves, asm kernels if Rust codegen stalls (OLA is the measured
  outlier), optionally IWRAM tables.
- Gate: **≤1×** worst (the C/asm reference is 69.4%), then U4. Fold into PR B
  only if B lands comfortably and the remaining delta is small; otherwise keep
  it separate so the delta stays attributable.

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
| **U3 — bit-exact product** | The product kernel set produces the pinned int8/int16 frames, on target and in the host twin. | Met for kernels (PR A); full hot path in PR B. |
| **U4 — real-time decode** | Worst full frame within the product budget at WAITCNT 4/2. | **Not met.** PR A measured 698% worst / 505% mean for the product kernel set; PR B gate ≤200%; PR C target ≤100%, then hardware. |
| **U5 — product playback** | 32,768 Hz path, zero underruns, clean period audit. | Not started (gated by U4). |
| **U6 — memory** | IWRAM/EWRAM/stack inside the product budget with UI and feeder. | Open; PR B moves buffers/code into IWRAM and must record the map. |
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

---

## Risks and open questions

1. **LLVM's ARM7TDMI codegen vs gcc.** At equal conditions (ARM, IWRAM,
   unchecked) the kernels are 3.6x the C/asm reference; the OLA is the
   outlier. If restructuring stalls, PR C uses asm, which Rust supports
   (`global_asm!`/`.s`, already used by `flac-lite-gba`).
2. **IWRAM budget.** The reference spends ~22.6 KB (17.6 KB tables/buffers +
   5 KB ARM code) of 32 KB. PR B must record the product's own map; the UI and
   feeder still need room (U6). Tables-in-IWRAM measured only 6.3% and stay
   off by default.
3. **Oracle drift.** The core must stay bit-exact and host-testable; every
   fast-path change is twinned. Do not “optimize” the oracle.
4. **Licensing.** `gba-audio` has no LICENSE; `publish = false` until settled.
5. **Encoder independence.** The C encoder remains the encoder of record until
   the Rust encoder milestone.
6. **Playback is not implied by decode.** A fast decoder is not a player;
   U4 must pass with margin before Phase D starts, and U5/U6 are separate.

---

## Decision log

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
