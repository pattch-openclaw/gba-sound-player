# ULI2 plan of record

Status: **live since 2026-11-14; PR C open, U4 not met.** This is the working
plan for the ULI2 product codec. [`../../ULI2.md`](../../ULI2.md) remains the decision record
(why ULI2, what it is, the licensing and encoder risks); this file owns the
phases, gates, measurement ledger and the order of work.

The target is **C-class performance** (the C/asm product decoder's 69.4% worst
decode wall), not merely a decode frame under the 262,144-cycle period. A parity
audit of C-1 + C-2 against the C product decoder found that they project to
~91% — the period, and no margin for the feeder — so **PR C-3 (proposed
2026-11-14, below) carries the differential items that reach parity.** C-1, C-2
and C-3 are all projections until each lands as its own measured arm.

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
| synthesis tables in IWRAM (`iwram-tables`) | **crashes**: `.iwram` 18,848 B leaves 13,664 B of stack against ~15,040 needed, so the stack overruns `__iwram_end` into code and the CPU jumps to an invalid address (the same failure class as PR B's hang). Quantified in the U6 row. |

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

#### PR C-2 — entropy rewrite (the 33%)

Redesign the bit-reader protocol. The 32-bit cached window from PR B is in
place, so this is not “more of the same”: candidate directions are holding the
reader state in registers across a whole band instead of behind `&mut`, a
byte-oriented refill path, and folding `huff`+`skip`+sign into one symbol
routine. Prize: ~90,000 cycles/frame.

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
| PR C (today) | 523,267 | 200% | measured |
| C-1: kernels → C's 134,600 | ~329,000 | 126% | C stage-2 per-kernel rows |
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

**Read the range honestly.** The optimistic end (~193,000 ≈ 73%) is inside the
±5% band this file calls parity; the pessimistic end (~213,000 ≈ 81%) is ~11
points above it. So C-3 makes parity *plausible* without proving it: if every
item lands and the frame is still ~81%, the residual needs its own attribution
before U4 is claimed, and the first candidates are the FFT register story (C-1)
and whatever entropy still sits above C's 42–48k after C-3a. Do not let the
table become a forecast.

**Proposed order.** Each step is small enough to twin and measure on its own,
and each changes the frame the next one is measured against — so every arm keeps
its control in the same image (PR A's rule).

1. **C-3b + C-3c** — delete work from the hot loop first: cheapest, and it makes
   the product arm comparable to the C record before anything is tuned against
   it.
2. **C-1** — asm kernels, specified as *bit-exact and stack-reducing*; the
   `iwram-tables` arm is re-measured afterwards and is expected to become legal
   (today it crashes: 13,664 B of stack against ~15,040 needed).
3. **C-2 = C-3a** — the entropy/dequant structure port, item by item from the
   table, each twinned against the core.
4. **C-3d, C-3e** — OLA fusion and `nb`-limited M/S; re-time the bench on the
   **production output format** instead of the dual int8+int16 write.
5. **Re-measure and take the U4 verdict** against the parity target, not against
   100%.

**Parity target, stated so it can be checked.** "C-class" means within ~5% of
the C record's decode wall **measured in this tree's instrument**, because the
C record's 69.4% was taken on gba-audio's whole album (167,222 frames, worst
frame 27806, tracks 02/03) with its own toolchain and image, and absolute frame
numbers here are image-specific (the same kernel sequence measured 2,970,042 in
one image and 2,676,986 in another). The kernel row is settled in C's favour:
its stage-2 report decomposes the worst frame as `pre 9.7k + fft 31.4k +
post 10.9k + ola 15.3k` per channel, i.e. **134,600 for the pair**, so
`ULI2.md`'s row is the reference and PR C's ~146,000 is struck. Still pin the
row in this tree's instrument before C-1's gate is written.

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
| **U4 — real-time decode** | Worst full frame within the product budget at WAITCNT 4/2 — read as **C-class (≤~70–75% decode wall)**, because the gate total adds convert + IRQ + DMA to whatever the decoder costs. **The ceiling itself is measured, not assigned** (2026-11-14): it is written down at the first near-real-time arm, from the measured gate total plus this tree's non-audio cost. Its worst-frame witness is PR E, not the 129-frame corpus. | **Not met.** PR B measured 204% worst / 177% mean; PR C added the attribution and a split-`huff` win to **200% worst / 174% mean** (523,267 / 456,187) and scoped the remainder as two rewrites (C-1 asm kernels 63%, C-2 entropy 33%). C-1 + C-2 project to ~91% — the period with no feeder margin, and **not C-class**. PR C-3 (above) carries the ~57,000 remainder: stats-free build, `dec_chan` structure, OLA fusion, `nb`-limited M/S, and C-1's stack cut. Revised projection **~193,000–213,000 (~73–81%)** — parity plausible, not proven; parity remains the target. |
| **U5 — product playback** | 32,768 Hz path, zero underruns, clean period audit. | Not started (gated by U4). |
| **U6 — memory** | IWRAM/EWRAM/stack inside the product budget with UI and feeder. | **Open, and now the binding constraint.** The product path needs ≈15.0 KiB of stack. Budget at PR C: `.iwram` 11,072 B baseline, 32,512 B usable, so 21,440 B spare — but 8,672 B of synthesis tables in IWRAM leaves 12,768 B, under the stack need, and `iwram-tables` **crashes** (stack overruns `__iwram_end` into code; the CPU jumps to an invalid address). IWRAM tables are therefore blocked until the stack requirement or the code size falls — and C-3's re-specification of C-1 (asm that also cuts the spilled intermediates) is the intended way to fall under it. `HotState` and the tables are both EWRAM now. **Priority decided 2026-11-14: audio first** — IWRAM budget goes to code, hot state and stack; synthesis tables next if the stack allows; UI affordances are the slack and may be dropped to buy audio headroom. Full `huff` inlining was rejected on this row: +4,252 B for 2.7% (U6 is binding), vs +896 B for 1.6% with the split. |
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
7. **The C comparison is not yet like-for-like.** Two asymmetries inflate our
   measured frame relative to the C record: our hot loop carries `Ops`/`Diag`
   bookkeeping the C product build compiles out (C-3b), and the bench times the
   dual int8+int16 output while the C record timed one format per arm (C-3d).
   Until both are removed, every `×` against 69.4% is an overstatement and a
   pessimistic estimate must not be used to justify a work item.
8. **Worst-frame and corpus mismatch — narrowed.** The C record's 69.4% is
   frame 27806 of its whole album (167,222 frames); ours is frame 101 of a
   129-frame corpus. The kernel-row disagreement (134,600 vs 146,000) is
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
