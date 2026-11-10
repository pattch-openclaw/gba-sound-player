# FLAC.md — compressed audio on the GBA

Goal #4 of this project: decode FLAC on a Game Boy Advance so more audio fits on
a cartridge. This file is the entry point: the constraint, what exists today,
where performance actually stands, what is next, and where every detail lives.
It is deliberately short — the detail is in [`flac-docs/`](flac-docs/), one file
per concern.

**Status in one line:** the decoder is complete and bit-exact; the playback harness
plays real music at 10,512 Hz with correct tempo and pitch (audibly verified); the
**shipped** path is still over budget at full rate (167% of the 32,768 Hz frame);
and the ARM kernel — landed, witnessed on mGBA's ARM7TDMI, measured through the
shipped seam — now decodes FIXED at **48.3% of the 32,768 Hz budget** (worst frame
48.5%), **2.9× faster than it was a day earlier, from one section directive**.
The core's arithmetic-width default landed as 6a-6 (2026-11-09), and 6a-7
(2026-11-10) priced it same-image and re-anchored the stack: **kernel + i32 is
39.74% mean / 39.96% worst** of the 32,768 Hz budget on FIXED, and the i64 leg
costs +44.1 c/sample against the i32 default. Additivity reproduces to 0.06%.

That is where four rungs of differential went, and the answer was not any of the
things they suspected. It is worth reading in order, because each rung rules out
what the next one would otherwise have blamed:

- **6a-5b (2026-11-09).** With E10's vendored arm timed *beside* the landed one in
  one link, one pass, one calibration, `asm − vend` = **+98,291 c/frame =
  +48.2 c/sample = 24.11 cycles per Rice codeword**, on 157/157 frames of both clips.
  The projection it was built to check was right to 0.4%, so the seam really did lose
  48 c/sample against the probe. What the image priced at **zero**: the bridge, the
  pinned tables, the 12-byte state frame, the ABI — all symmetric.
- **6a-5c (2026-11-09).** The cost is the codeword loop's **instruction count**, at
  **4.0000 cycles per instruction**: deleting the provably dead duplicated `and` the
  guard insertion left behind moves the pair by exactly `4 × 640,000` cycles, and
  deleting the five guard instructions on top of it by exactly `5 × 4 ×
  640,000`, leaving 0.46% — the bridge. Not a layout effect: the saving tracks
  codewords *executed* (16,384 on every 2048-sample frame, 4,096 on the 512-sample
  frame). The old "4.02 cycles per inserted instruction" that 6a-5b treated as an
  anomaly was the answer.
- **6a-5d (2026-11-09).** 4 cycles is the price of an ARM instruction **fetched from
  cartridge ROM** (16-bit bus, WS0 = 3,1, no cache). Put the same instruction stream
  in **IWRAM** and the landed arm goes **1,151,236 → 506,413 c/frame**,
  **109.77% → 48.29%** of the 32,768 Hz budget, with the `ctrl` leg cycle-identical
  and `.text`/`.iwram` moving by exactly the body's 680 B. Both realizations were
  paying the ROM tax: E10's body pinned on its own drops −290.4 c/sample, and with
  both in IWRAM the pair reads **+14.2 c/sample instead of +48.2**. "The landed
  realization lost 48 c/sample" was a statement about two *addresses*.
- **E6's "instruction fetch is ruled out" is refuted, with its scope named**: E6
  pinned a *Rust* path where each instruction carried many cycles of work, and its
  variant arms forced inlining and took linker veneers. A tight ARM loop of one-cycle
  instructions is the case it did not isolate.

What this is **not**: a product claim. It is *decode only*; the producer's convert +
push stages (~7.7 ms combined) are not in these windows; the shipped default
(`TableCountKernel`) is untouched at 167% and no product path names `rice-arm`; and
**E5 — hardware arbitration — is unrun**, which matters more now than ever,
because a 4→1.4 fetch ratio is exactly what a cartridge model can get wrong. The
direction is not model-dependent (no cache, 16-bit bus); the multiplier is mGBA's.
Both product rates still fit with the kernel alone (61.7% at 18,157 Hz, 35.7% at
10,512 Hz). See [`flac-docs/measurements.md`](flac-docs/measurements.md),
[`plan-of-record.md`](flac-docs/plan-of-record.md) (6a-5c/6a-5d readings) and Q12 in
[`flac-docs/open-questions.md`](flac-docs/open-questions.md).

## The constraint

`thumbv4t-none-eabi` is Tier-3 bare-metal: `#![no_std]`, **`core` + `alloc` only**.
Any crate that references `std` — directly or transitively — cannot compile for
this target. That single fact is why this project has its own FLAC decoder:

- The mature Rust decoders are built on `std::io` abstractions. In symphonia's
  no-std fork the blockers are structural, not dependency-tree accidents:
  `Error::IoError(std::io::Error)` as the one error type, an `io/` module built on
  `Read`/`Seek`/`BufRead`, `HashMap` in the codec registry, `std::sync::Arc` in
  metadata.
- So on 2026-08-30 the project took **option (c): write a minimal `no_std`,
  zero-dependency, zero-allocation FLAC frame decoder of its own** (`flac-lite`).
  We control both ends — encoding is offline and ours, the "filesystem" is
  memory-mapped ROM — which deletes the entire stack that made the port
  impossible.
- The failed-port probe is kept as evidence: `examples/symphonia_flac_probe/`
  (expected to fail compilation).

Full reasoning: [`flac-docs/architecture.md`](flac-docs/architecture.md). The
alternative codec, evaluated under the same constraint, is
[`OPUS.md`](OPUS.md).

## What exists today

| Piece | State |
|---|---|
| `crates/flac-lite` | Portable core: `no_std`, zero deps, zero alloc, `#![deny(unsafe_code)]` plus named `cfg`-gated `.iwram` pin attributes (exactly one in a default build). Frame decode is **complete** (`bits` → `stream` → `frame` → `subframe` → `residual` → `stereo`), witnessed bit-exact against `flac -d` on host and in the ROM image. Per-frame `driver`, table-less `scan` walk (feature), CRC verification (feature), `RiceKernel` seam. Arithmetic width (6a-6): `i32-integrator` on by default, guarded per subframe, `i64` reference/escape leg (`make flac-test-wide`). Still `todo!()`: the `decoder` facade, `format::Manifest`, `Blocksize::samples` and `EncodeProfile::validate`. |
| `crates/flac-lite-gba` | The GBA overlay. Two `RiceKernel` arms behind their own features: `rice-twin` (6a-3, Rust, forwards to the core's twin) and **`rice-arm`** (6a-4: the E10 ARM-mode kernel via `global_asm!`, its FFI and `KernelAbi`, three tables under `iwram-pin`). A **default build still contains no kernel, no asm, no FFI and no `unsafe`**; the shipped default is still `TableCountKernel`. |
| `examples/flac_spike` (6a-5 / 6a-7 arms) | Three feature-gated product-path arms (`item6a5-rust`, `item6a5-asm`, `item6a5-arms`), plus 6a-7's **vendored** width leg (`item6a7-wide`, `src/wide.rs`, the pre-6a-6 i64 path) timed in one image with `ctrl` and `asm`. `make item6a5-test`, `item6a7-test`, `item6a5-arms-rom`, `item6a5-asm-rom`, `item6a7-rom`. |
| `examples/overlay-kernel-check` | 6a-4's instrument ROM — the only place the assembly runs. Sweeps the committed corpus and the synthesized boundaries through three legs (incumbent, assembled kernel, model) on mGBA's ARM7TDMI, and is the repo's first ROM suite that actually runs assertions (8 `#[test_case]`s). It also asserts the three tables' `.iwram` placement at runtime. |
| `examples/flac_spike` | The measurement instrument and the `ctrl` arm of every performance record. Host witness + in-ROM decode proof + cycle harness. |
| `examples/flac_spike/experiments/` | Every performance experiment as a reappliable sample (README + patch), never on the live sources. |
| `examples/arm-kernels` | The ARM-mode kernels, proven correct by a host twin and an mGBA sweep over committed golden tables. Outside the decode path. |
| `examples/flac_integration` | Standalone ROM that links the decoder: build/link/boot sanity, no audio. |
| `crates/flac-lite/README.md` | The `GAFP` container byte contract and encode-profile guarantees. The packer (`scripts/pack_flac.sh`) is still a stub; the spike's generated offset table is the stand-in. |
| Playback | The rate-play harness plays `bioluminescent.wav` at 10,512 Hz with correct tempo and pitch (ring buffer + FIFO8). The shipping `agb` mixer/DMA path is still the product playback step (Phase 2 step 9 in the retired roadmap; see the plan of record). |

Module-by-module detail, the seam contract and the design rules that must
survive implementation: [`flac-docs/architecture.md`](flac-docs/architecture.md).

## Where performance stands

The real-time budget at 2048-sample frames and 32,768 Hz is
**1,048,750 cycles/frame ≈ 512 cycles/sample**. Decode cost is paid per sample;
the budget is granted per second — which is why lowering the sample rate is a
lever at all.

| Path | FIXED mean | LPC mean | Landed? |
|---|---|---|---|
| Phase-1 baseline | 220.7% | 658.0% | superseded |
| E2a + E2b landed (today's shipped path) | 170.2% | 589.5% | **yes** |
| + E10 ARM kernel (probe) | 101.98% (worst 102.50%) | 521.6% | no |
| + E10 + E4 i32 stacked (probe) | **94.47% (worst 94.95%)** | 216.3% | no |

At 10,512 Hz, real music, on today's path: **FIXED fits** (73.9% mean, 78.3%
worst) and **LPC does not** (183.9% / 211.6%). The only in-budget full-rate FIXED
decode ever measured was E9's stacked probe — which is exactly what item 6 exists
to land, measure on the shipped path, and then arbitrate on hardware.

**No GBA-hardware real-time claim exists.** All numbers are mGBA on this host;
E5 (hardware arbitration) has never been run and is the only thing that can turn
that sentence into a claim.

Which numbers may be quoted, how eras and denominators differ, and the full
ledger: [`flac-docs/measurements.md`](flac-docs/measurements.md).

## What is next

One line per rung; the full plan, bars and invariants are in
[`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md).

- ~~**6a-5** — the realization reading, three arms in one image; the item's number
  of record, against a pre-declared fork.~~ **Landed: carry.** asm − ctrl =
  **−584,856 c/frame / −286.9 c/sample** (FIXED), 157/157 frames faster, in the
  product-shape image. The seam is now threaded from `frame` down to `residual`
  (`…_with<K>`), which is what made the arm a product-path reading; no shipped
  count or instruction stream moved. Record:
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md).
- ~~**6a-4** — the ARM kernel lands in the overlay: correctness only, no timings,
  dual-witnessed over the committed golden tables.~~ **Landed.** It found one
  real ABI bug on the device (the 12-byte frame's stack-argument offsets) that no
  host-side arm could see; the landing record in
  [`plan-of-record.md`](flac-docs/plan-of-record.md) has the control build, the
  new image's section table, the gate counts and the mutation table.
- ~~**6a-5b** — the paired realization differential: the vendored arm and the landed
  arm in one link, one pass, one calibration, so the seam is priced as a same-image
  pair instead of across two records. Instrument-only; it decides whether 6a-5's
  103.8% projection is real.~~ **Measured.** It is real (+48.2 c/sample same-image,
  0.4% from the projection), the seam is exonerated, and the instrument — one more
  timed leg behind `item6a5b-vendored`, with E10's module vendored into the spike
  crate and its own witness green beside the others — is reusable for the next leg.
- ~~**6a-5c** — the guard marginal, one variable at a time.~~ **Measured.** An
  instruction in the codeword loop costs **4.0000 cycles** (two independent
  removals, exact to the cycle, proportional to codewords executed); the six-instruction
  delta is the whole 48 c/sample, and the dead `and` is deleted as a fix.
- ~~**6a-5d** — the codeword loop's placement: the same instruction stream resident
  in IWRAM instead of cartridge ROM (plain `.iwram`; `.text_iwram` is E6's trap).~~
  **Measured, and it is the largest result in these records:** −644,824 c/frame on
  the FIXED clip, 109.77% → **48.29%** of the 32,768 Hz budget, from one directive
  and no other change, with the control leg cycle-identical. An ARM loop in
  cartridge ROM pays 4 cycles per instruction; in IWRAM, ~1.4.
- ~~**6a-6** — guarded i32 becomes the core's default width, i64 kept as the
  always-correct escape leg.~~ **Landed.** `i32-integrator` on by default, guarded
  per subframe by `Σ|c_j|·2^(b−1) ≤ i32::MAX`, `i64` as the reference/escape leg
  (`ArithmeticWidth` reports it); `flac-test` 183, `flac-test-wide` 178,
  `flac-gates` 1,377; default instrument `.text` +432 B, symbol-attributed.
- ~~**6a-7** — stack confirmation (kernel + i32) and the E-number re-anchor.~~
  **Landed.** Three legs in one image (`ctrl` i32, `wide` vendored i64, `asm`
  kernel+i32): width marginal **+44.1 / +1,759.1 c/sample** (FIXED / LPC),
  stack **39.74% / 39.96%** FIXED, additivity reproduces to **0.06% / 0.005%**.
  New gate `item6a7-test`; the default image is unchanged.
- **E5** — hardware arbitration, once per candidate rate × path; the only route to a
  real-time claim, and now the highest-value confirmation in the item, since a
  4→1.4 fetch ratio is exactly what a cartridge model can get wrong.
- **6b** — the container: the facade rewired onto `scan`, the real GAFP packer and
  manifest, and the rate-play path through a real manifest.
- **Rate sweep** — the cross-rate profiling sweep on the improved decoder, which
  produces the "which rates become feasible" guidelines from measurement.

## Documentation map

| File | What it answers |
|---|---|
| [`flac-docs/architecture.md`](flac-docs/architecture.md) | What exists and why it is shaped this way: the constraint, the two crates, the `RiceKernel` seam, the container, the design rules, the delivered portability ladder, decisions of record. |
| [`flac-docs/format-notes.md`](flac-docs/format-notes.md) | Every measured FLAC byte fact, with its witness: frame header layout and three corrections, subframe/residual/stereo rules, footer, and the 28-row table of assumptions measurement killed. |
| [`flac-docs/measurements.md`](flac-docs/measurements.md) | The ledger: what each number belongs to, the baseline of record, absolute marginals that survive an era, the item-6 constraints, and what not to quote. |
| [`flac-docs/perf-results.md`](flac-docs/perf-results.md) | The E-series experiments in their own words — positives and negatives, each with its control and witnesses. |
| [`flac-docs/playback-results.md`](flac-docs/playback-results.md) | The rate flow and audible-playback records: push, convert, FIFO, feeder-off control, the landing regression and its fix. |
| [`flac-docs/arm-kernels.md`](flac-docs/arm-kernels.md) | The ARM-mode kernel track: measured ARMv4T facts, the kernel set, the dual-witness method, and the machine facts the assembly taught (no LDM-to-PC interwork, `MUL` hazards, `global_asm!`). |
| [`flac-docs/method-and-gates.md`](flac-docs/method-and-gates.md) | How a number gets taken here: budget arithmetic, run discipline, determinism, golden vectors, the gate surface, toolchain pin, cargo-config inheritance, corpus blindness. |
| [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) | The only live plan: 6a-4…6a-8, 6b, the width decision, invariants, baselines, the landing record, ordering rules. |
| [`flac-docs/open-questions.md`](flac-docs/open-questions.md) | The eleven questions a future session still owes an answer, each naming the record that opened it and the rung that owns it. |
| [`crates/flac-lite/README.md`](crates/flac-lite/README.md) | The `GAFP` container contract, the encode profile, and the crate's own gate list. |
| [`examples/flac_spike/README.md`](examples/flac_spike/README.md) | The measurement vehicle: embedded arms, censuses, gates, and how to regenerate the assets. |

## Record map (where the old sections went)

`FLAC.md` was a single lab notebook for ~9,000 lines; code comments and older notes
cite its section names. Those names now resolve here:

| Old section | Now |
|---|---|
| The Constraint; Rust FLAC Landscape; Candidate Evaluated; Upstream Status; Decision (c) | [`architecture.md`](flac-docs/architecture.md) §"Why we wrote our own decoder" and §"Alternatives considered" |
| Perf gate spike — concrete plan; Budget; Validation process | [`method-and-gates.md`](flac-docs/method-and-gates.md) §"The real-time budget", §"Rules every run follows" |
| Frame header: measured byte layout; Corrections 1–3; frame sync scanning; encoder choices; `--force-utf8-legacy-noop` | [`format-notes.md`](flac-docs/format-notes.md) |
| Golden vectors | [`method-and-gates.md`](flac-docs/method-and-gates.md) §"Golden vectors" |
| Prior assumptions that did not survive measurement | [`format-notes.md`](flac-docs/format-notes.md) (last section) |
| Completed: `FrameHeader::parse`, `SubframeType::parse`, step 3a–3f | [`format-notes.md`](flac-docs/format-notes.md) §"Facts established while implementing the frame layer" (narratives retired) |
| Completed: spike PR 1/2/3 | [`method-and-gates.md`](flac-docs/method-and-gates.md) + [`measurements.md`](flac-docs/measurements.md) + [`examples/flac_spike/README.md`](examples/flac_spike/README.md) |
| Perf follow-ups — experiment menu; E1–E10 entries; ordering log | verdicts in [`measurements.md`](flac-docs/measurements.md); ordering rules in [`plan-of-record.md`](flac-docs/plan-of-record.md) |
| E1/E4/E2/E7/E6/E8/E2b/E10/E9 results | [`perf-results.md`](flac-docs/perf-results.md) |
| E10 algorithm plan; groundwork; unit-test kernel track; PR A; PR B | [`arm-kernels.md`](flac-docs/arm-kernels.md) |
| Sample-rate reduction proposal; rate ROM; rate-play ROM; playback attribution; feeder-off | [`playback-results.md`](flac-docs/playback-results.md) (+ budget table in [`method-and-gates.md`](flac-docs/method-and-gates.md)) |
| flac-lite / flac-lite-gba split proposal | [`architecture.md`](flac-docs/architecture.md) §"Why two crates"; measured correction in [`measurements.md`](flac-docs/measurements.md) |
| Split-ladder item 5 (`RiceKernel` seam, PRs 1–4) | seam in [`architecture.md`](flac-docs/architecture.md) §"The seam"; reading in [`measurements.md`](flac-docs/measurements.md) |
| Split-ladder item 6; 6a-1/2/3; toolchain pin; width decision; 6b | [`plan-of-record.md`](flac-docs/plan-of-record.md); measurements in [`measurements.md`](flac-docs/measurements.md); toolchain rule in [`method-and-gates.md`](flac-docs/method-and-gates.md) |
| Recording Q&A for a future session | [`open-questions.md`](flac-docs/open-questions.md) |

## Standing caveats

- Every `podman-*` target has **never been run** in this harness (rootless podman
  cannot start here: no `newuidmap`, no podman machine, no nested virtualization).
- `make test-rom` runs and collects **zero** tests. The repo's first ROM suite
  with assertions in it is 6a-4's (`make overlay-kernel-test`, 8 cases); the root
  crate's suite is still empty.
- `flac-portable-check` is a compile gate: no host-side performance claim exists.
- E5 (hardware arbitration) is unrun, so no real-time claim exists.
- The `Rice` corpus behind the differentials has 0 escaped partitions, no
  `u32::MAX` fold and no codeword that overflows the guard — the mutation arms are
  green *correctly*. See [`method-and-gates.md`](flac-docs/method-and-gates.md).
