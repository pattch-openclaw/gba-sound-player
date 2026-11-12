# Plan of record — the only live plan

Everything else in these docs describes what has been built or measured. This file
describes what is being built next, what bar each piece is judged against, and the
rules a landing may not bend. It replaces the item-5 plan (fully landed), the
"Phased plan" (fully landed or superseded), the perf-spike five-PR plan (PRs 1–3
landed; 4–5 open), the E10 plans (landed) and the rate-tooling proposal (landed,
with its step-4 multi-clip ROM and its cross-rate sweep still open) — all of which
now live only in git history and in the records they produced.

## Acceptance criteria — what this plan is for

Two criteria, stated by the project owner on 2026-11-10. They are the definition of
done: a rung that does not move one of them is not on the critical path, and a
measurement that does not decide one of them waits behind the work that does.

### C1 — `flac-lite` is a standalone crate

It can be used **independently** as a `no_std`, pure-Rust FLAC decoding crate, and
it is **ready to be separated into its own repository**. In checkable form:

- no `todo!()` anywhere in `src/`, and a facade a consumer who has never heard of
  this repository can drive: bytes → frames → samples. That clause is met as of
  6b-1 (2026-11-10): `decoder::Decoder`, `format::Manifest`,
  `Blocksize::samples` and `EncodeProfile::validate` all landed, and the facade is
  a shell over `scan::Frames`. The extraction itself is a decision, not missing
  implementation;
- the crate's existing claims hold with nothing scaffolding-shaped in the way:
  `core` + `alloc` only, zero dependencies, zero allocation on the decode path,
  `#![deny(unsafe_code)]`, bit-exact against `flac -d`;
- every platform-bound item confined to a documented off-by-default feature
  (`iwram-pin`) or to `flac-lite-gba` — nothing GBA-shaped in the default public
  API;
- `make flac-portable-check` still compiles it for a second bare-metal target;
- **explicitly not required:** a crates.io publication. "Ready to be separated"
  means a `git subtree split`-shaped extraction would work, not that a registry
  entry exists.

### C2 — `flac-lite-gba` decodes *and plays back*, in real time, at 32 kHz, on a GBA

The claim the project was started for, demonstrated before the split, and now to be
demonstrated **through** the split's two crates rather than around them. In
checkable form:

- a ROM that links `flac-lite-gba` **with `rice-arm` on** and plays 2048-sample
  frames at **32,768 Hz** from cartridge without an underrun, on hardware;
- **the stream declares the rate it is played at.** Ruled 2026-11-11 (Q14): a stream is
  encoded at the rate it is played at, and the player does not reinterpret a declared
  rate to fit a clock. A 32,000 Hz asset run on a 32,768 Hz clock is 1.024× fast —
  **41.06 cents** — and is not C2 however clean its period audit. The asset this
  repository currently ships is exactly that case, which is why 7-0 now sits in front
  of 7-1;
- the decode half is already measured on hardware (42.48% mean / **42.72% worst**),
  so what is unproven is the *combination* — decode + convert + push + mixer/DMA
  inside one 62.5 ms frame;
- **FIXED-0 by construction.** LPC is **174.14%** of the 32 kHz frame on
  hardware (decode alone), and only FIXED-0 has a measured row; `-l 0` disables
  LPC but does not force the fixed order. The pack-time encode profile must refuse
  FIXED-1..4 and any unmeasured residual shape. The `EncodeProfile` guard is part
  of C2, not a nicety;
- the acceptance run is on a GBA 001 **and** a GBA SP, and its verdict is
  **underrun-free playback**, not a cycle count.

### What must not delay them

The 3.7–6.9% mGBA shortfall (Q13) is parked: C2's decode number already exists as a
hardware measurement, and C2 is judged on hardware end to end, so an
emulator-vs-silicon attribution is not on the path. Also out: 24-bit decode, rates
other than the target, `dyn` dispatch, kernel sub-stage attribution (6a-8 stays
untriggered), and crates.io publication.

### The arithmetic C2 has to fit

Per 2048-sample frame at 32,768 Hz: **62.5 ms = 1,048,750 cycles**.

| stage | figure | source | share of frame |
|---|---:|---|---:|
| decode, kernel + i32, worst frame | **398,255 c ≈ 23.8 ms** | **hardware, product path** (6b-2 pt 2's image; GBA 001 + GBA SP, identical) | **37.974%** |
| convert | 14.1 ms | mGBA, 10,512 Hz pass, **word-copy era** | 22.6% |
| push (ring copy + credit, CS) | 12.7 ms | same pass, same era | 20.3% |
| **gross, on paper** | | | **≈80.9%** |

Both consequences are work items, not caveats. The producer figures are **stale** —
a different rate, the pre-kernel decode path, the word-copy era — so they are
re-measured at the target rate on hardware (item 7's first PR). And the remaining
margin is **≈19%, ≈11.9 ms**, which is where the mixer, the IRQ handler and the
feeder have to live: C2 will be decided by that margin, not by the gross. A 20%
error in the producer stages alone would consume it. (The margin grew from
≈8.7 ms when 6b-1 measured the *product* path on hardware: 38.04% worst rather
than the instrument's 42.72%, and a hair wider again at 6b-2 pt 2's product image
(**37.974%**, ≈19.1% left) — that PR's library change moved the product image's
code layout, so the row above carries its reading and 6b-1's 38.04% stays attached
to the bytes it was read from.)

## Status at a glance (2026-11-11, after 6b-3)

**The plan is now judged only against C1 and C2 above.** Item 6a is closed — the
overlay exists, the kernel is in it, the decode half of C2 is measured on two units.
C1's facade/manifest clause landed with 6b-1; what remains is C2's playback half
(item 7). The 6b decode/product half is landed (the kernel in a product ROM, the
real packer, the seek table and the anchored walk).

**6b-3 landed the same day, at the headless level, and it is the first time this
project has played anything at its target rate.** `product-play.gba` (`5dc7869…`)
plays the shipped asset at 32,768 Hz through the real manifest, ring, sound DMA and
timer path and counts its own underflows: **571 hardware periods, 571 serviced, 0
underflows, 0 coalesced, 0 late services, one tail period of 240 samples, 320,000
samples pushed and consumed, `pcm_fnv=…6E15 [MATCH]`, screen BLUE with its own fit
measured.** Its mGBA work envelope is **53.062% of a frame** — decode 40.657%,
convert 10.832%, push 1.457%, feeder 0.495% — worst single frame **59.367%**, and
**no frame's work exceeded its own period**. Its host model (`make
product-play-test`) reproduces that audit line digit for digit from the ring code
and the geometry constants alone, and measures what the geometry is actually worth,
which retired three justifications this document and `geometry.rs` had been making
for it. **None of that is C2**: 7-3 has not been run on this image on any unit, and
the stage shares are mGBA system-path numbers — which is precisely what 7-1 exists
to re-read on silicon.

**And on the same day it was measured, the rate it was measured at was ruled out
(Q14).** That image plays a stream that *declares* 32,000 Hz on a 32,768 Hz clock. The
project owner's ruling: a stream is encoded at the rate it is played at, and the player
does not reinterpret a declared rate to fit the clock — 1.024× is **41.06 cents**, and no
period audit buys that back. So `examples/flac_integration/assets/spike_l0.gfp` is **not
a product asset**, 6b-3 is infrastructure rather than a product path, the
`32000 → 32768` arm in `geometry.rs` carries a `TODO(Q14, BLOCKING)`, and item 7's first
PR is an asset at the rate it will be played at (**7-0**). Nothing measured in 6b-3 is
wasted by the ruling — the rate probe measured a stream *declaring* 32,768 decoding
bit-identically at the same flow-control floor and the same cost — but the sequence in
front of 7-1 changed, and it is written into item 7 rather than left to be inferred.

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
- **6a-7b is measured, and it did not land where the projection said (2026-11-10).**
  The 6a-7 image renders its three legs on the GBA screen
  under `item6a7-wide` (`flac-spike-6a7.gba`, `.text` `9f0b19e924cd…`, ten lines, fit
  measured in-ROM) and logs the same ten lines to serial, so an mGBA run and a
  stock unit print one string. The mGBA reference for **this image** is re-taken
  (two runs, byte-identical) and reproduces the pre-summary 6a-7 record to
  **≤18 cycles/frame** on every leg. **It then ran on a GBA 001 and a GBA SP,
  twice each: all four runs identical to the cycle.** Two results, one per half of
  the rung. **Correctness: the decoder is bit-exact on silicon** — the blue
  backdrop means assets, both PCM pins, the per-frame equality witness and
  calibration all passed on hardware, the first such claim in this project.
  **Timing: the ±2% bar failed on all five quantities** — hardware FIXED `asm` is
  **445,558 c/frame = 42.48% mean / 42.72% worst** of the 32,768 Hz frame,
  **+6.89%** over mGBA — so **39.74% is retired as a hardware number** and the
  pre-declared fork resolves to `diverges`. Decode still has **2.34× headroom**, so
  6b is unaffected. On the way it also found, and we fixed, a display bug no
  headless emulator could see: a trailing `frame.commit()` blanked the screen it
  had just painted.
- **The data-read probe is dropped from the queue, by decision (2026-11-10).** It
  was the natural next rung after 6a-7b and it is now parked behind the acceptance
  criteria. The reasoning is in the criteria section: the probe would explain the
  3.7–6.9% shortfall, but C2's decode figure is *already* a hardware measurement and
  C2 is judged by an underrun-free playback run on hardware, so an
  emulator-versus-silicon attribution cannot change the answer. Q13 stays open with
  its trigger written down — re-open it if a product path needs the last 6%, or if
  any claim is made by scaling an mGBA cycle count. Until then a *rule* replaces the
  probe: see "mGBA numbers are derated" in the ordering rules.

- **6b-1 landed and measured on hardware, 2026-11-10** (facade onto `scan`,
  `rice-arm` carried into the product ROM `flac-integration`): C1's facade exists,
  the product path decodes at **37.83% mean / 38.04% worst** of the 32,768 Hz frame
  **on a GBA 001 and a GBA SP, identical to the cycle**, pin matched, and mGBA
  agrees with hardware to **0.055%** on that leg (the default kernel's +4.03% is
  the 6a-7b shortfall, now localized). The `scan` seam is byte-identical in the
  product image. **6b-2's first half landed 2026-11-11** (the real `GAFP` writer
  `scripts/gafp_pack.py`, its fixture/facts generator, `tests/gafp_asset_facts.rs`
  and the `gafp-pack-check` gate): the packer's bytes are byte-identical to the
  6b-1 stand-in's committed product blob, so there is one container definition and
  the pack-time profile guard is now gated rather than drafted — see that item's
  section. **6b-2's second half landed 2026-11-11** (`stream::SeekTable` over real
  §8.5 tables, a `Frames::anchored` walk started from a seek point, a sixth
  reference-tool fixture, and the stand-in decision — *keep it, as the tie between
  the spike's asset and the product's*): none of the added code reaches a ROM, yet
  both product images' bytes moved, so both were re-read on silicon — the shipping
  `rice-arm` path at **37.765% mean / 37.974% worst** on a GBA 001 and a GBA SP,
  identical to the cycle on both units, and the `TableCountKernel` control image
  **+7.09% slower on hardware**, which makes mGBA's +7.90% a real silicon cost and
  not an emulator artifact. See that section. **Next, in order:** **6b-3** (32,768
  Hz rate-play through a real manifest), then **item 7**
  (playback: the real-time claim). Not started: 6a-8 (untriggered, and now further
  parked by C1/C2) and the parked Q13 probe.

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
| **6a-7b** ✅ | **Measured 2026-11-10: GBA 001 and GBA SP, two runs each, all four identical.** The 8 px on-screen summary in `flac-spike-6a7.gba` (gated under `item6a7-wide`: the default image stays byte-identical), then run it on the GBA 001 and GBA SP and read `ctrl`/`wide`/`asm`, the stack and the width marginal. This is the non-fetch path E5 did not cover. The bar **failed on all five quantities**: hardware FIXED `asm` is **445,558 c/frame = 42.48% mean / 42.72% worst** of the 32,768 Hz frame (+6.89% over mGBA), so 39.74% is retired as a hardware number and the fork resolves to `diverges`. | Cycles, same-image, per-frame paired: four runs on two units, identical to the cycle; the per-arm PCM pins and the frame-by-frame equality witness are the correctness gate; the ROM's own fit line is the screen's gate. | **it did.** Every leg is 3.7–6.9% above mGBA, the stack +6.6% on both clips, and the width marginal moves in opposite directions per clip — not calibration (deltas are not a constant offset) and not a clock error (not a constant percentage). Candidates named, not yet separated: mGBA's ROM data-read model, its prefetch/data-read interaction, the flashcart. |
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

### 6a-7b — measured: the stack on hardware, and the 6% mGBA does not charge (2026-11-10)

**Why.** E5 answered the fetch question: the same instruction stream is 4.085
cycles/add from ROM and 1.037 from IWRAM on both a GBA 001 and a GBA SP, and mGBA
reproduces that within 2 cycles/call (ratio 3.939). What E5 did *not* touch is the
rest of the decode path — the bit reader's ROM byte reads, the pinned tables,
residual, stereo. 6a-7's 39.74% stack is therefore still an mGBA reading, and this
rung is the one that can turn it into a hardware number.

**The instrument.** `flac-spike-6a7.gba` gains a
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

(the mGBA reference run; the hardware reading further down differs by 3.7–6.9% on
every leg, which is the rung's finding rather than a transcription)

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

**What the first hardware runs found — and no emulator could.** On 2026-11-10 the
image was run twice on a GBA 001. Both runs reached the summary's **pass backdrop
and nothing else: blue, no text.** The backdrop being blue rather than red is
itself a result, and the larger half of this rung: blue is the `harness_ok`
colour, so the assets, both per-arm PCM pins, the frame-by-frame equality witness
across all three legs and the calibration stability check had all passed **on
silicon** — the first time this decoder's correctness has been established on
hardware rather than in mGBA. What failed was the display, and only the display.

The mechanism, read out of `agb` 0.25 rather than guessed: `TiledGraphicsFrame::
commit` derives DISPCNT's background-enable mask from the backgrounds *shown into
that frame* (`BackgroundFrame::num_regular`, reset to 0 by `Graphics::frame()` —
`src/display/tiled.rs`). `show()` commits the ten lines; the spike ROM then
entered the frame-commit loop it has carried since its colour-only verdict. A
fresh frame holds no background, so `commit` writes `enabled_backgrounds = 0` and
the backdrop alone is repainted ~16 ms later. The palette survives because it is
a **direct VRAM write** (`PALETTE_BACKGROUND`, a `MemoryMapped1DArray`), which is
exactly why the failure presented as "the palette worked and the tiles did not".

The fix is E5's shape: under `item6a7-wide` the ROM ends in `loop {}` and presents
nothing after `show()`. Nothing else changed, and that is measured rather than
asserted — the default image is still byte-identical, the fix moved `.text` by
**−52 B** (89,484 → 89,432 B; `9f0b19e924cd…` → `1e14085e36db…`, the removed
`frame()` / `commit()` and nothing else), and **every leg's mGBA number came back
identical to the cycle** afterwards. Two lessons worth carrying: a headless
emulator cannot see a display bug, because nothing is ever drawn to be erased; and
the same trap is one `frame.commit()` away in any ROM that shows a static screen.

**The reading: four runs, two units, identical to the cycle.** The screen fix
landed and the ROM was then run twice on a GBA 001 and twice on a GBA SP. All four
runs produced the same ten lines, every leg every frame — so this divergence is
systematic rather than noise, and the instrument is deterministic across two
different units (E5's result, one instrument over).

| per-frame, 157 frames/clip | FIXED `ctrl` | FIXED `wide` | FIXED `asm` | LPC `ctrl` | LPC `wide` | LPC `asm` |
|---|---:|---:|---:|---:|---:|---:|
| mGBA mean c/frame | 1,662,797 | 1,752,714 | 416,821 | 2,987,762 | 6,573,107 | 1,751,108 |
| **hardware mean c/frame** | **1,773,272** | **1,846,926** | **445,558** | **3,134,342** | **7,020,087** | **1,816,753** |
| delta | +110,475 | +94,212 | +28,737 | +146,580 | +446,980 | +65,645 |
| delta % | **+6.64%** | +5.38% | **+6.89%** | +4.91% | +6.80% | +3.75% |
| hardware c/sample | 865.86 | 901.82 | **217.56** | 1,530.44 | 3,427.78 | 887.09 |

| derived | FIXED mGBA | FIXED hardware | LPC mGBA | LPC hardware |
|---|---:|---:|---:|---:|
| stack `ctrl − asm` | 1,245,976 | **1,327,714** (+6.56%) | 1,236,654 | **1,317,589** (+6.54%) |
| width `wide − ctrl` | 89,917 | **73,654** (−18.09%) | 3,585,345 | **3,885,745** (+8.38%) |
| `asm` mean % of 32,768 Hz | 39.74% | **42.48%** | 166.97% | **173.23%** |
| `asm` worst frame | 39.96% | **42.72%** | 167.85% | **174.14%** |

**The bar, and the fork's answer: it fails, on all five quantities.** `asm` mean,
stack and both width marginals miss ±2% (FIXED `asm` 445,558 against
408,484–425,157; FIXED stack 1,327,714 against 1,221,056–1,270,895; LPC `asm`
1,816,753 against 1,716,085–1,786,130; widths 73,654 and 3,885,745 against
88,118–91,715 and 3,513,638–3,657,051). So the pre-registered fork resolves to its
`diverges` arm, and the consequence is exactly the one written down before the
number existed: **39.74% is retired as a hardware number.** Hardware FIXED decode
is **42.48% mean / 42.72% worst** of the 32,768 Hz frame.

**What the shape rules out, and what it leaves.** It is *not* the calibration: a
per-frame offset would move every leg by the same absolute cycle count, and the
observed deltas run from +28,737 to +446,980. It is *not* a clock or timer-model
error: a global factor would move every leg by the same percentage, and the
observed range is 3.75%–6.89%. What it is, is per-leg and clip-dependent, and two
features of the table constrain any explanation:

* the **stack** grows by almost the same *absolute* amount on both clips (+81,738
  FIXED, +80,935 LPC — 1.0% apart, ≈39.6 c/sample each) while each leg grows by a
  different *fraction*: a clip-independent absolute amount points at machinery both
  clips pay per sample, not at the predictors;
* the **width marginal changes sign between clips**: −18.09% on FIXED (the i64 leg
  costs *less* extra on hardware) and +8.38% on LPC. No single multiplier does
  that, so what mGBA under-charges is not simply "the i64 path".

Three candidate mechanisms survive that, and this instrument cannot separate them:
(i) mGBA's model of **cartridge-ROM data reads** — the bit reader is the one part
of the decode path that streams bytes from ROM, and E5 arbitrated *instruction
fetch* only, which matched hardware to 0.05%; (ii) the **prefetch buffer's
interaction with those data reads**, which E5's pure-fetch stream never exercised
(a ROM data read can discard the prefetched line); (iii) the **flashcart** —
every hardware number in this repository comes from a cart running ROM out of
PSRAM, and a production mask ROM need not price a byte read the same way. (iii) is
a limit of the *measurement*, not of the decoder, and it is why the numbers above
are quoted as "hardware via flashcart".

**What survives, and it is what the rung was for.** The decode path is bit-exact on
silicon (above), and FIXED decodes on hardware at **42.72% worst frame** — **2.34×
headroom** on the 32,768 Hz budget. The rate decision is untouched by the
shortfall, exactly as pre-declared: even a 20% penalty leaves FIXED under 48%. LPC
is 174.14% worst on hardware and still does not fit full rate. And the kernel is
still a kernel: the `asm` leg is 1,327,714 c/frame cheaper than the shipped default
*on hardware*, 157/157 frames, on both units.

**Gates.** After the screen fix, one headless mGBA run reproduces all ten lines
exactly (the fix is display-only and the numbers say so). `item6a7-test`
**20 + 1 + 10** (the five new `summary` tests pin the
label rule, the fixed-point formatter, `Leg::mean`, both delta signs and the
rendered screen, including the percent-vs-per-mille factor — an off-by-100 there
is invisible on a screen), `spike-test` **15 + 10**, `item6a5-test` **15 + 3 + 10**,
`item6a5b-test` **21 + 4 + 3 + 10**, `make check` clean. Two headless mGBA runs of
one build print **byte-identical** ten lines, and a third run after a comment-only
edit — which moves `.rodata`'s panic `Location` lines but not `.text` — prints the
same ten lines again. A full run takes ~6m40 under `mgba-test-runner` (the ROM
loops after printing, so `timeout -k 5 400` is the intended invocation and exit
124 is success).

**How to quote this image.** By `.text` (`1e14085e36db…`, 89,432 B) and by its ten
printed lines, not by a whole-image sha. The image with the screen fix is
`7e0ff19b7a0a…`; the pre-fix build whose runs found the bug was `615d38be2786…`,
and those values move with any edit that shifts a line number in `main.rs`,
because `core::panic::Location` lives in `.rodata`. The control-build paragraph
above is that same fact measured rather than argued.

**Not claimed, and not doable here.** The two hardware runs are the rung's own
arbitration and this host has no unit and no display: the instrument, the
same-image mGBA reference and the bar are landed, and the reading is owed. Scope
is also unchanged from 6a-7 — decode only; the producer's convert/push stages
(~7.7 ms) are outside these windows and no full real-time claim exists. The
shipped default decode path is still `TableCountKernel` at 167%, and no product
path names `rice-arm`; that is 6b's.

**What is next after it.** The probe this rung opened (Q13) is **parked by
decision** — see "Acceptance criteria". Next is the plan's critical path: **6b-1**
(facade onto `scan`, `rice-arm` carried into a product ROM), **6b-2** (packer,
manifest, pack-time profile), **6b-3** (32,768 Hz rate-play through a manifest), then
**item 7**, which is where C2's real-time claim is actually made.

## Item 6b — the container, and the kernel in a product ROM (C1, and C2's decode half)

6b serves **C1** (the facade that makes `flac-lite` standalone) and C2's decode half
(the kernel actually reaching a shipped ROM). Its measurement is a codegen reading
(the deferred `scan` **+3,120 B** is charged and attributed there) plus bit-exactness
through a real manifest.

| PR | Scope | Witness | Failure it isolates |
|---|---|---|---|
| **6b-1** ✅ | **Landed 2026-11-10.** `stream` → frame source → `decoder::Decoder` rewired onto `scan::Frames`. Inherits `Frames` / `StreamBounds` / `ConsumedFrame` / `resync` — **not** a third frame loop. Turns `scan` on in a ROM-shaped build for the first time. **And carries `rice-arm` into that ROM** — the one-line placement 6a-5 assigned to this rung, so the first ROM built after this PR decodes with the kernel instead of `TableCountKernel`. | Codegen: the walk's cost read from the product image's own symbols; bit-exactness through the walk against the file-level walk; **the kernel's win re-read in the product ROM** (mGBA here, a unit by the owner) | that the walk's measured cost was argued-as-free and the facade pays it — and that the kernel's win lives in an instrument but never in a product |
| **6b-2** ✅ | **Landed 2026-11-11, in two halves** — pt 1 the writer, its fixtures and `gafp-pack-check`; pt 2 the `SEEKTABLE` read, the `anchored` walk and the stand-in decision. The GAFP writer is `scripts/gafp_pack.py`; `pack_flac.sh` remains the unimplemented profile front-end and now says so. Plus `format::Manifest` parse, `EncodeProfile::validate` at pack time, `SEEKTABLE` interpretation + `Frames::anchored(n)`. **The profile refuses what the target rate cannot play** — at 32,768 Hz on hardware that is LPC (174.14% of the frame, decode alone) — so C2's constraint is enforced by the packer rather than discovered by the player. | Bit-exact decode through a real manifest; profile validation convicted at pack time **on a real LPC clip as well as a synthetic one**; the seek witness `frame_run_loop.rs` already holds the reverse-order half of | that the pack-time profile guard the whole design leans on is decorative — and that a 32 kHz build quietly ships LPC assets that cannot play |
| **6b-3** ✅ | **Landed 2026-11-11, at the headless level.** The rate-play path on a real manifest **at 32,768 Hz**: `examples/flac_integration/src/product_play_main.rs`, a second binary in the product crate, drives asset → `Manifest` → `Decoder` + `rice-arm` → `playback::convert_frame` → `RingFeed` → sound DMA/FIFO, with timer0 as the sample clock, timer1 cascaded at 560 samples as the period whose IRQ *is* the feeder, and timer2 as an independent period witness. The geometry (`chunk 560`, `ring 36×560`, `prefill 16`, `grace 16`) lives in one module, `src/geometry.rs`, that both the ROM and the host model read; the spike's generated offset table is retired and the manifest supplies every frame length. | `product-play.gba` `5dc786951b91b356…`, one mGBA run: `hw_periods=571 serviced=571 und=0 coalesced=0 max_gap=0 late=0 max_delay=0 tail_periods=1 tail_real=240`, `pushed=consumed=320000`, PCM MATCH, screen verdict legible (`fit: … fits=true`). Stage shares, the closure identity, the pacer span and the sizing measurements: `measurements.md`, the two 6b-3 rows. Model gate: `make product-play-test` (8 tests, host). | that the throwaway harness was load-bearing in ways nobody wrote down — and that the *geometry* is too: three of its written justifications measured wrong and are corrected (the 36-period stall figure, the unbounded sustained cushion, the minimum-prefill figure); and, ruled the same day it landed, that the asset it plays declares 32,000 Hz and the 32,768 mapping is not a product path (Q14 — `TODO(Q14, BLOCKING)`), which is why 7-0 owns an asset at the rate it will be played at |

**Out of 6b, explicitly:** mixer/DMA and `agb` integration, 24-bit decode, `unsafe`
of any kind, and any **underrun-free playback claim** — 6b's verdict is "the
container works and the kernel is in the ROM". The playback claim is **item 7's**,
and C2 is not met until 7-3 passes.

### 6b-1 — landed: the facade, and the kernel in a product ROM (2026-11-10)

**What landed.** `format::Manifest` parses the `GAFP` contract and validates it;
`decoder::Decoder` / `FrameBuffers` / `DecodedFrame` exist and are a **shell over
`scan::Frames`** (no third frame loop); `Frames::next_frame_with<K>` is the seam
that lets a product name its kernel by type; `driver` is demoted behind
`instrument` + `#[doc(hidden)]` with its deprecation intent written down; and
`examples/flac_integration` is now the **product ROM** — it embeds a generated
`GAFP` blob, parses it, decodes all 157 frames with `rice-arm`, matches the perf
gate's PCM pin, and times a second pass on the same bytes.

**Three findings worth the ink.**

1. **The scaffold's facade signature was unsatisfiable**, and the fix is a
   contract improvement rather than a workaround: `decode_next` returns
   `DecodedFrame<'b>` where `'b` is the *borrow of the buffers*, not the scratch
   allocation's lifetime. A frame's samples are valid until the next call, which
   is exactly the double-buffer hand-off a mixer needs — and the borrow checker
   now enforces it instead of a doc comment.
2. **The offset table cannot be `&[u32]`.** The blob is a `&[u8]` with alignment
   1, so a `u32` view is an unaligned cast (UB, and this crate is
   `deny(unsafe_code)`); `align_to` is safe but returns an *empty* middle on a
   misaligned slice — a seek table that silently empties itself. The manifest
   holds the raw little-endian bytes and decodes each entry on access; parsing
   proved length and monotonicity, so every accessor stays total.
3. **The first timed pass priced the instrument, not the decoder.** Timing the
   proof pass reported **1.95 M c/frame** for a kernel the perf gate measures at
   0.42 M; the window was including the per-sample FNV fold. Split into a
   correctness pass and a timed pass — the perf gate's own two-pass shape — and
   the numbers land where they belong. Recorded because it is the kind of 5× a
   single-pass "convenience" hides.

**The `scan` seam costs nothing in a product image (the 5(A) condition).**
Measured as an A/B on the *same* ROM: the facade calling
`next_frame_with::<TableCountKernel>` (the generic entry) versus the pre-seam
shape where the facade calls the non-generic `next_frame` and the walk's
delegation does the rest. Every loadable section is **byte-identical** — `.text`
**49,056 B** `7c7d580ccbb4…`, `.iwram` 812 B `690ba38594d3…`, `.rodata`
990,512 B `898fb8849509…`. Identical sections mean identical code at identical
addresses, so the generic parameter cannot cost a cycle in a single-instantiation
ROM: the two ends of the seam are `#[inline]` delegations (`frame::decode_frame`
already delegated to `decode_frame_with::<TableCountKernel>` before this rung).

**The product ROM's own reading** (`flac-integration.gba`, mGBA, one pass per
kernel, same source, `--no-default-features` for the control):

| build | kernel | mean c/frame | worst c/frame | mean % | worst % |
|---|---|---:|---:|---:|---:|
| default | `rice-arm` | **396,659** | **398,867** | **37.82%** | **38.03%** |
| `--no-default-features` | `TableCountKernel` | 1,559,303 | 1,567,127 | 148.68% | 149.43% |

Both legs match the perf gate's pin exactly
(`0x54C7B356621B6E15`, the FNV of the committed reference PCM), so the numbers
are from proven bytes. The kernel's advantage reproduces through the product path
at **3.93×**. These are lower than the instrument's 39.74% for the same kernel,
which is expected: the instrument's arm carries its own call shape and state;
what matters here is that the *product* path is not slower.

**Measured on hardware, both builds, GBA 001 and GBA SP (owner, 2026-11-10),
identical on both units to the cycle:**

| build | mGBA mean | hardware mean | Δ | hardware % budget (mean / worst) |
|---|---:|---:|---:|---:|
| `rice-arm` | 396,492 | **396,709** | **+0.055%** | **37.83% / 38.04%** |
| `TableCountKernel` | 1,559,312 | **1,622,116** | **+4.03%** | 154.67% / 155.45% |

**The ARM path is priced correctly by mGBA — 0.055%, *not* the 3.7–6.9% of
6a-7b.** That is the sharpest fact this rung produced, and it narrows Q13 without
a probe: the shortfall lives in the leg whose time is spent fetching instructions
from cartridge ROM (the default kernel's Thumb codeword loop), not in the
IWRAM-resident kernel, and not in a global clock, timer or calibration term. It
also means the product planning number is a *hardware* number: **decode is 38.04%
worst**, 2.63× headroom, and the kernel's advantage through the product path is
**4.09×** on silicon (3.93× in mGBA).

**A trap the pin caught.** The repository's FNV-1a 64 uses a **non-standard
offset basis** (`0xCBF43CE95DE684B7`, not `0xCBF29CE484222325`) in both
`scripts/frame_vectors.py` and `examples/flac_spike/src/checksum.rs`. The first
ROM used the standard basis and went red on a *correct* decode. The generator now
cross-checks the borrowed pin against the committed reference PCM and fails
closed if they disagree, and the ROM carries the constant with a comment saying
it is the house variant.

**The blob is a stand-in, and 6b-2 owes its equality.**
`scripts/gen_gafp_blob.py` assembles the header + table from the perf gate's
committed, fail-closed generated table; `scripts/pack_flac.sh` is still a stub.
6b-2's real packer must emit a **byte-identical** `GAFP` for the same frames —
that is what turns this stand-in into a differential instead of a second
definition.

**Gates.** `flac-gates` green across all six configurations plus both combination
arms and the second bare-metal target (`gafp-check` added to the gate);
`flac-test` 143 lib; `flac-test-scan` 151 + the facade's 6; `spike-test` 15 + 10
with the spike now enabling `instrument`; `make check` clean;
`make native-flac-rom` and `native-flac-rom-default-kernel` both build and fix.

**Owed.** The product ROM's reading **on a unit** (both builds, one run each; the
owner's step) — **delivered 2026-11-10**, both builds, both units, in the hardware
table above; and 6b-2 — **landed 2026-11-11**, in two halves, whose library change
superseded the bytes of these two images and re-read both on silicon in its own
row.

### 6b-2, first half — landed: one container definition, and a guard that reads frames (2026-11-11)

**What is in the tree.** `scripts/gafp_pack.py` is the real `GAFP` writer. It
takes a `.flac` (or a WAV, which it encodes itself through libFLAC at the
profile's flags), walks every frame of the result with the strict reader in
`scripts/frame_vectors.py`, then walks **every subframe slot inside that** —
§9.2.5's partition boundaries, Rice parameters and codeword lengths, warm-ups,
LPC precision/shift/coefficients, the bps+1 mid/side slot width, escape records,
each footer CRC-16 — and only then writes the container. The offset table is
where that walk says the frames begin; nothing in the header is inferred from
`--blocksize`. It reads its own bytes back before calling the pack done
(`audit_blob`: magic, version, every manifest field, the table it just wrote
re-read and required strictly increasing with its first entry pointing at the byte
after the header, the region compared byte-for-byte against the source's frames,
then every frame re-parsed for sync and header CRC-8, its length re-matched
against what the walk recorded, its number and block size checked, and its footer
CRC-16 recomputed). Where the source PCM is in hand — a WAV input, or a re-encode
the packer did itself — it also decodes that source with `flac -d` and compares
sample-for-sample against the PCM it generated, unless `--no-verify-pcm` says
otherwise; a `.flac` handed in already has no such comparison to make, and the
facts table says so by carrying no PCM rows for that run rather than by inventing
a source. `scripts/gen_gafp_fixtures.py` packs the crate's own
test assets with it and emits `crates/flac-lite/tests/gafp_asset_vectors.txt`;
`crates/flac-lite/tests/gafp_asset_facts.rs` reads those blobs and that table
back through `Manifest`, `scan::Frames` and `Decoder`. The gate is
`make gafp-pack-check`, now a prerequisite of `flac-gates`.

**1. The control, before any edit.** `make flac-gates` green across every
configuration (1379 passed / 3 ignored / 75 test binaries) and both product images
built from that state: `flac-integration.gba`
`12732380a0aa1543…`, `flac-integration-default-kernel.gba`
`2b108d290d44cfcf…`. Both are still on disk and are the reference for the next
paragraph.

**2. No shipped byte moved — that is this change's section table.** Rebuilding
both ROMs after the change reproduces **both SHA-256s exactly**. There is nothing
to attribute symbol by symbol because nothing was linked differently: no file
under `crates/flac-lite/src/` or `crates/flac-lite-gba/src/` changed, the
embedded `.gfp` is the same bytes (which the new gate proves from the packer's
side rather than from a second run of the same generator), and the `nm` anchors
are 6b-1's by identity. No `cargo clean` control was run, and none is asked for
here: the images being bit-identical is the stronger statement for a change whose
Rust half is a test file and a `[[test]]` stanza. That claim is scoped to
**this** change, and the paragraph two further down is the reason it has to be:
the C1 doc-record pass edited comments inside those same files, and it broke the
image hash. Named, because a row that says "the hash broke" without saying to what
is not much of a record: rebuilt at the repo path, 6b-1's images are the pair above
and the post-doc-pass images are `46c231163c09…` (1,072,860 B) and `06746b84ea8d…`
(1,071,032 B) — **the same byte counts, different bytes**, the panic-`Location` line
numbers again. Nothing executable differs: both pairs time to the identical cycle
totals (62,249,317 and 244,812,048) and the same PCM pin, which is what lets 6b-1's
hardware row stand as the reference for 6b-2 pt 2's control. (6b-2 pt 1's commit
message asserted byte-identity with 6b-1's images, which this paragraph already
contradicts; the message has been corrected on the branch.)

**3. Gate counts, with the new configuration named as a new gate.** The first
attempt at the aggregate **failed**, and the reason is worth keeping: the new
suite consumes `scan::Frames` and `Manifest::bounds`, which is
`#[cfg(feature = "scan")]`, and `Cargo.toml` had no `[[test]]` stanza for it — so
`make flac-test` compiled it under default features and died with E0432/E0599
(`/tmp/flac-gates-6b2.log`). The fix is the pattern this tree already uses for
`scan_frame_walk_layout.rs` (`required-features = ["scan"]`) and
`rice_two_mechanism_differential.rs` (`["rice-hybrid"]`); the alternative —
leaving the suite out of the aggregate — is how a witness becomes a file. After
it: `flac-gates` green, **1403 passed / 0 failed / 3 ignored across 79 test
binaries** (6b-1's record was 1379 / 3 / 75). The +24 is 6 × 4 — `flac-test-scan`,
the `crc-check,scan` arm, the three-feature arm, and `gafp-pack-check`'s own named
leg — and `make flac-test`'s count is unchanged, which is the point of the
stanza. `gafp-pack-check` is a **new** gate, not an edit to `gafp-check`, which
still guards the product blob through the stand-in; `flac-gates` is the changed
aggregate, and this is its first leg that needs `flac`/`metaflac` on PATH (every
other leg reads tables and blobs generated earlier and committed). `make check`
clean (exit 0). `make flac-portable-check` green for `thumbv6m-none-eabi`.
`make test-rom` **not run**, and the reason is the paragraph above: it would be
looking at the same binary it last saw.

**The equality the stand-in was owed, paid.** The packer was run over the
FIXED-0 reference clip re-synthesized and re-encoded in a temp dir (the
`.flac` is a generator input, never a tracked file — only the frame/PCM regions
under `examples/flac_spike/assets/` are committed), and compared against
`examples/flac_integration/assets/spike_l0.gfp`, the blob `gafp-check` guards and
`flac-rom` embeds. **Byte-identical: 865,996 bytes, 157 frames, first difference
none.** That matters because the two derivations are not the same derivation: the
stand-in reads offsets out of `examples/flac_spike/src/assets.rs`, which the perf
gate generated from the frame table, while the packer re-derives every length from
where its own walk exits, verifies every footer CRC-16 (314 subframe slots
censused, `fixed0 x314`), and computes the max-FIXED-order byte from the subframes
rather than from the `-l` flag. Two independent derivations landing on one byte
string is what makes `GAFP` one definition instead of two, which was the whole
point of the "stand-in" paragraph above. `scripts/pack_flac.sh` is still a stub —
but its six-step plan (normalize to profile, *verify only profile-conforming
frames*, strip metadata, index by strict header parse + CRC-8 rather than by sync
scan, emit, round-trip against `flac -d`) is the shape this implements, including
its own corrections: `-l` is `--max-lpc-order` and not a fixed-predictor cap, and
the last frame is legitimately shorter. The remaining step for that file is to
become the front-end.

**4. Mutation witness for the new suite.** Six tests, one per claim the
packed-asset path makes, and each mutation was applied to the *generated table*,
not to the Rust — the suite's job is to convict the table and the crate together:

| Mutation | Result |
|---|---|
| None (honest green) | ✅ 6 passed — all four blobs' offsets, lengths, CRCs and PCM match the packer's record |
| Last frame's recorded CRC-16 `35d5` → `35d4` | ❌ `the_offset_table_and_a_walk_of_the_region_agree_frame_by_frame`: "frame 2's bytes are not the bytes whose CRC-16 the packer recorded" |
| One per-frame `pcm_fnv1a64` | ❌ `every_blob_decodes_to_the_pcm_the_reference_decoder_produced`: "frame 0 does not decode to the samples `flac -d` produced for it" |
| An `lpc` subframe row → `fixed` | ❌ `one_pcm_three_blobs_and_the_field_that_cannot_tell_them_apart`: "six LPC-4 subframes, no FIXED ones" |
| A frame's `blob_offset` +1 | ❌ both `the_manifest_says_what_the_packer_recorded` and `the_table_and_its_blobs_are_all_here` (and the walk's `cursor() > end` abort fires before it) |
| The record's `max_fixed_order=0` → `2` | ❌ `the_manifest_says_what_the_packer_recorded`: "the recorded max FIXED order" |
| The LPC asset's relaxations → `none` | ❌ `one_pcm_three_blobs_…`: the test that asserts it was built with **two** guards relaxed, because the profile limit and the measured cost are two facts |

Restoring the table from the copy at `/tmp/gav.orig` returned the suite to green,
and the generator revalidated green ("4 blobs match their committed bytes, 3
refusal legs still refused with their stated reasons, table matches").

**The trap, and it is the one the row was about.** The header's max-FIXED-order
byte reads **`0` for an all-LPC stream**, and it is not wrong: an all-LPC stream
has no FIXED subframe above order zero. The fixture set makes that concrete —
three of the four blobs are the *same 5,000 samples* encoded three ways
(independent stereo, mid/side, and `-l 4`), so they carry **one PCM pin and three
frame sets**, all three manifests say `0`, `EncodeProfile::validate` accepts all
three including under a hand-built `max_fixed_order: 0` profile, and all three
decode bit-exactly. The `-l 4` arm of the 157-frame reference clip is the same
trap at product scale: the walk censuses `lpc3 x2, lpc4 x312` while
`max_fixed_order_of(…)` returns 0. So the field is a *recording*, not a guard —
C2's guard is the packer's census plus the measured cost of the kind, and nothing
on the decoder's side can supply it. That is why the packer carries a measured
table (6a-7b's rows: `fixed0` 42.48% / 42.72%, `lpc4` 173.23% / 174.14%, with
the raw cycle counts and the instrument named in each row's `source`) and refuses
any kind whose cost it has not measured — including, today, `verbatim` and
`constant`, which the row names but which this target has never timed. The
arithmetic is self-checked (`check_rows`, before any packing): each row's percent
is re-derived from its own cycle count and the frame's own budget, and a
mismatched denominator or a transcribed digit stops the run. The budget is the
**32,768 Hz playback timer's** frame (1,048,576 cycles for 2048 samples), which is
the rate every figure in `flac-docs/measurements.md` is stated at — but **not the
number the ledger divides by**. The ledger's budget row is 1,048,750 cycles
(`flac-docs/method-and-gates.md`: `2048 / 32768 s × 16.78 MHz`, and
`flac-docs/measurements.md` heads its condition table with the same 1,048,750),
which is a *rounded-clock* derivation; the GBA's clock is exactly 2^24 Hz, so the
same frame period is exactly **2^20 = 1,048,576**. This record's own shares are
divided by 1,048,750 (398,255 → 37.974%; against 2^20 it is 37.981%), so the file
now quotes the exact denominator and publishes against the rounded one. The gap is
174 cycles, 0.0166%, worth ≤0.007 pp on any share here — no verdict in this file
turns on it — but it is two things at once: a share quoted to three decimals is
denominator-dependent, and `check_rows`'s 0.1 pp tolerance is *looser* than the
gap, so the guard passes its own 42.48% row against the 1,048,576 it cites (which
re-derives to 42.49%). Item 7-1 adds another column of shares; decide the
denominator before then. Using the stream's own 32,000 Hz period would have
overstated the headroom by 2.4% and made the packer's percentages not comparable
to the product ROM's 37.83%/38.04%.

**Correction, 2026-11-11: the product guard is FIXED-0, not merely FIXED-only.**
`-l 0` disables LPC; libFLAC still chooses FIXED orders 1–4 from the signal, and
only FIXED-0 has a measured row. So the product profile refuses FIXED-1..4 (and
CONSTANT/VERBATIM) as unmeasured before writing anything; the default guard also
keys the measured row to the residual envelope it was taken on (one partition, no
wasted bits, no escape partition, Rice parameter ≤ 10), because the kind/order
alone does not describe the per-codeword cost. `check_guard_shapes` asserts that
policy at every pack, and the four committed assets are inside that envelope. The
remaining axes — FIXED-1..4 and the wider residual shapes — are open measurements,
not features the guard silently accepts.

**What the three refusal legs refuse, named honestly.** `lpc_at_product_profile`
packs the `-l 4` arm of the 157-frame reference clip — 1.4M samples of real
libFLAC LPC decisions, not a hand-built bit pattern, though the underlying PCM is
the synthesized test signal rather than music — and is convicted twice, by the
`-l 0` profile cap and by the measured 174.14%. `rate_44100_at_32000_target`
starts from a committed real encode (the crate's own `s2` fixture, 96 kHz stereo),
re-encoded at 44,100 Hz with `--no-seektable` so the stream-level and the
frame-level rate readings are each contradicted independently, and says so twice.
`eight_bit_at_16bit` is a real 8-bit encode, refused because the container's
depth list is `[16]` — and the interesting half is that its `-l 0` subframes land
on **VERBATIM**: with FIXED removed by `-l 0`, verbatim is what libFLAC has left,
and verbatim is a kind this target has never measured. A new depth would not be a
header change; it would be an unmeasured cost. No `.gfp` is written for any
refusal, and the test asserts that too.

**A side probe, deliberately not finished.** C1 requires no stale scaffold records
in this crate. A grep for `todo!()` in `crates/flac-lite/src/` returned five hits
before this section's edits, **every one of them in a doc comment and not one in
a body** — the records were out of date, not the code. They are gone now; the
places that said it were `lib.rs` (three), `format.rs`, `subframe.rs` and
`driver.rs`, and the crate README said it twice more (its "remaining `todo!()`"
paragraph and the file tree's "scaffold: todo!()" row) next to a Next-steps
paragraph that still listed landed work as outstanding. One of the five was the
crate-wide `#![allow(dead_code, unused_variables)]`, whose comment
still said "REMOVE once implementations land". The honest way to answer that was
a probe: delete the attribute and build every gated feature combination. Five
items need it, all cfg-conditional names whose other arm is what a given build
compiles — `bits::peek_at`'s `end`, the `crc8_bit` position the `crc-check` arm
is the only reader of, `subframe`'s `order_u8` binding, `subframe::integrate_fixed`
(live under `--no-default-features` and under `cfg(test)`, dead in the default
i32 build), and a test-local `pos` in `bits`. The probe also surfaced something
the attribute was never doing: an integration test is its own crate, so a
crate-level allow never reached `tests/`, and one unused `lens` there already
appears in three arms of `flac-gates`. Fixing the six means editing source in the
shipped path, which owes a control build and a section table this PR does not
have; the list is recorded where the attribute lives, so the next pass is a
mechanical one rather than a re-derivation.

**And the image-hash claim that pass broke, measured rather than hand-waved.**
After those comment edits, rebuilding the two product ROMs no longer reproduces
the control SHA-256s: **8 bytes differ** in `flac-integration.gba` (at
75,565 / 75,581 / 75,597 and at 1,068,761 / 1,068,793 / 1,068,809 / 1,068,825 /
1,068,841) and **9** in the default-kernel image. Every one is a single byte
inside a run of 16-byte records that alternate a ROM pointer (`0x0801_165e`,
`0x0801_1875`…) with two small integers, and the changed byte is always the first
of that pair, moving by +1 or +7: those are `Location` constants — the line
numbers that ride next to a panic's file-name string — shifted by exactly the
lines the doc edits added above them. No instruction moved, and nothing about
execution did. The point is not that this particular delta is harmless; it is
that **whole-image identity does not survive an edit to a comment**, so the
"byte-identical image" control that a change like the packer's gets for free is
not a claim any later pass inherits — and when it breaks, a `cmp -l` listing plus
the record stride around the changed offset attributes it in minutes without
needing a section table, which is the same reason `objdump` output belongs in the
record and a hash alone does not.

**Owed by the rest of 6b-2.** `SEEKTABLE` interpretation and
`Frames::anchored(n)` — the half of this row that the fixtures deliberately do
not touch, since the GBA product path never needs random access and the packed
container has no seek table — plus the decision the paid equality enables about
`scripts/gen_gafp_blob.py`. Both landed below one section down; the decision is
*"keep it, for a different job than it was doing"*, and the reasoning is there
rather than here. Two fixture axes from this paragraph stay open and are recorded
in `crates/flac-lite/tests/README.md`: 1024-sample blocks, and packed
CONSTANT/VERBATIM/escape-shaped and higher-order-LPC assets (the highest LPC order
any real-stream table here carries is 8). They are joined by three more that the
seek-table work opened, listed at the end of that section.

### 6b-2, second half — landed: the seek table read, and a walk that starts from one (2026-11-11)

**What this half was.** RFC 9639 §8.5 read out of real files, and a frame walk
that begins at a seek point instead of at byte zero — the seam `scan::Frames::
anchored(n)` already existed for, and which nothing had used. The GBA product path
still needs neither: a packed GAFP has no seek table, and the ROM decodes a
manifest, not a container. That is exactly why this half is about the *reading*:
an item that the product never exercises is only safe if a fixture and a gate say
what it means.

**The fixture question came first, and it decided to add a sixth file.** Nothing
committed here had a multi-point seek table: `s2_seekable_96k_stereo` carries one
point, for sample 0, which is enough to test a block walk and no part of enough to
test a *seek* — one point cannot be wrong about which frame it names without
agreeing with every other reading. So the table gained
`s6_seekpoints_8k_mono`: 6,000 samples at 8 kHz, encoded
`flac -1 -f -s --no-padding -l 0 -b 512`, then edited with `metaflac
--add-seekpoint` — the reference tool's own output, not a hand-spliced table,
which is the rule for committed fixtures here (hand-built tables belong in the
`src/` unit tests, and the shapes only they can make are used there — see the
refusal table below). The file is 5,260 bytes: blocks
`STREAMINFO, SEEKTABLE, VORBIS_COMMENT`, `frame_data_offset` **234**, 12 frames,
a 144-byte table of **8 entries — 7 real points and one placeholder**, and a last
frame of **368 samples in a stream whose `max_blocksize` says 512**. That last
frame is why this fixture and not a variant of an existing one: a reader that
takes a frame's length from the declared maximum is wrong at that frame and only
there, and a seek table with a point *on* it is the only way to ask the question.

The five existing fixtures did not move: `git diff` over `tests/fixtures/` after
the regeneration is empty, and `scripts/gen_stream_vectors.sh` reproduces all
five byte for byte. `tests/stream_metadata_layout.rs` asserts the table covers
**six** fixtures and refuses one it cannot embed — a fixture named by the table
but not read by the suite is a fixture whose facts are not being checked.

**What `metaflac --add-seekpoint` was measured to do.** The requests were written
deliberately out of order and with awkward values in it, at 8 kHz with 512-sample
fixed blocks:

| asked | what landed in the table |
|---|---|
| `5632 0 2048 4608 3584 1024 2560` | sorted ascending — the tool orders the table, the encoder does not |
| `2600` | **2560**: snapped down to the first sample of the containing frame |
| `12000` | dropped — past the end of the audio |
| `2048` twice, and `0` | one entry each: exact duplicate requests are dropped |
| `X` | written as the **last** entry: `sample_number=0xFFFFFFFFFFFFFFFF`, `stream_offset=0`, `frame_samples=0` |
| (rendering) | `metaflac --list` prints a placeholder as the bare word `PLACEHOLDER` and hides its other two fields |

Two things follow, and both are now rows in the table rather than assumptions: a
table's **entry count exists only as `length / 18`** (§8.5's Table 6 carries no
count), and a placeholder's other fields are **not recoverable from the reference
tool's own rendering** — so the generator pins `seek_placeholder_other_fields_zero`
as a measured fact read from the bytes, and the cross-check against `metaflac`
compares *which entries are placeholders*, never what a placeholder holds.

**What the reference tools refuse, and what they accept.** Each shape below was
built by splicing bytes into a real file and asking both tools (probes under
`/tmp/st/measure_*.py`, recorded here because they changed what the crate does):

| table shape | `flac -t` | `metaflac --list` | `flac-lite` |
|---|---|---|---|
| payload 19 bytes | exit 1, `ERROR while decoding metadata` | exit 1, `FLAC__METADATA_CHAIN_STATUS_BAD_METADATA` | `SeekTableLength` |
| payload 0 bytes | 0 | `seek points: 0` | accepted; `find()` answers `None` |
| two byte-identical type-3 blocks | 0 | prints **both** tables | `DuplicateSeekTable` |
| placeholder before the real points | 0 | renders it at index 0 and carries on | `SeekTableOrder { at }` |
| two real points, same sample number | 0 | renders both | `SeekTableOrder { at }` |
| two placeholders | 0 | both, at the end | accepted |

The asymmetry is the deliberate one, and it is the same rule the rest of this
layer runs on: libFLAC's *decoder* does not consult the seek table unless asked to
seek, so a table it never reads cannot hurt it — which is why a duplicate table
passes `flac -t` at exit 0, exactly as duplicate `VORBIS_COMMENT` and `PADDING`
blocks do (measured in 6b-1's row). A reader that *uses* the table has no such
luxury: with two tables, "the anchor" is undefined; with a placeholder in the
middle or two points naming one sample, "the last point at or before *n*" is
undefined. So the real points must be strictly ascending and the placeholders must
all come after them — `metaflac`'s own help says placeholders "always go at the
end", and every file it writes obeys — and a payload that is not a whole number of
18-byte entries is a container fault rather than a truncation, because there is no
count field to appeal to and the last entry has no defined end. That is also the
one shape the tools refuse in the same words, quoted above.

**The API: a view, not a list.** `stream::SeekTable` borrows the metadata block's
payload and reads it at fixed stride: `len()` is `payload.len() / SEEKPOINT_LEN`,
`point(i)` bounds-checks without wrapping, `find(sample)` answers the last **real**
point at or before a target (`None` for a target before the first real point — the
first point is not an anchor for it), and `seek_table(stream)` returns
`Ok(None)` when the stream has no table, because "no seek table" is a complete
stream and not an error. Nothing is materialised: this is the same
no-allocation boundary that made the crate decline an arbitrary metadata block
list, and it is why `SEEKPOINT_LEN` is a named constant — the entry size is what
makes a payload enumerable at all.

**The two quantities a seek point carries, and the one a walk needs.** A point's
`stream_offset` counts from the **first byte after the metadata**, not from the
start of the file: point 0 of both `s6` and `s2` carries offset 0 while the frame
it names is 234 and 176 bytes in. Read as a file offset the point lands between
frames, and both suites show that reading refused — 4a, because the resolved byte
does not name a frame start; 4b, because `Frames` refuses the byte and has written
**no samples** when it does.

The other quantity is settled by RFC 9639 §9.1.5, not by a convention here.
`SeekPoint::frame_number()` (implemented by `FixedBlockNumbering`, which owns the
block size and declared end) converts the seek point's sample number to the coded
frame number. §9.1.5 says a coded frame number **MUST equal the number of frames
preceding the frame**, so a conformant whole stream starts at frame 0; the sample
number is `frame_number * block_size`, and `first_sample / block_size` is already
the frame number `Frames::anchored(n)` wants. There is no
`first_frame_number + frame_index` composition: a nonzero first coded number is a
stream for which §9.1.5 says seeking is not reliably possible (a mutilated/
recovery case), not a second base to add. The 4b test now asserts the fixture rows
start at 0 and takes its anchor from `frame_numbers[target]`; the recovery builder
still accepts the number a mutilated region's first header states.

**Correction, 2026-11-11, after reading §9.1.5 in full.** The earlier paragraph
called the quotient a *position* and proposed composing it with a first-frame
number. That was an inference from a phrase about mutilated streams, not the
RFC's rule: the coded number is required to equal the number of preceding frames,
so the first frame of a conformant stream is 0. `FixedBlockNumbering` now states
the two-direction arithmetic in one place (`frame_number_for_sample`,
`first_sample_for_frame`, block size, declared total/frame count), and both
provider suites use it rather than re-deriving the division.

**Four readings of every real point, in the generator.** For each fixture with a
table: an independent 18-byte parse in Python; `metaflac --list`'s rendering; the
strict frame walk from `frame_vectors.py`; and, for a fixed-blocksize stream, the
arithmetic. All four must agree, and each point is additionally pinned against the
walk it claims to describe — `frame_samples` equals that frame's actual block size
(the 368 case again), `frame_data_offset + stream_offset` equals the frame's real
offset, and the byte there begins with a frame sync code. Then the part only a
decoded fixture can supply: for every real point, the FNV-1a-64 of the reference
decoder's PCM **from that point's first sample to the end of the file**
(`seek_points_tail_pcm_fnv1a64`). The generator refuses to write the table if two
points share a tail hash: a pin that cannot tell two anchors apart is not a pin.

**The tests, and what each one is for.**

* `src/stream.rs`'s `tests::seek_table_shape` (9 tests, hand-built tables) —
  the rules no encoder here produces: field order and endianness (`0xDEAD_BEEF…`
  in the middle catches a reader that treats the two u64s as interchangeable),
  bounded indexing, the non-multiple-of-18 refusal, the empty table, the
  placeholder distinguished by sample number alone and nothing else (one below it
  is a real point), every ordering refusal in the table above, and
  `frame_number`'s refusals (variable blocking, a point that is not a frame
  start, and a point at/past the declared total/frame count) plus its one
  legitimate extension: a point whose `frame_samples` disagrees with the stream's
  blocking is still an anchor, because that field is a record and not a claim
  about the whole stream.
* `stream_metadata_layout::every_seek_table_is_read_as_the_tools_and_the_frame_walk_render_it`
  — the real fixtures: the helper agrees with an explicit `Blocks` walk, the view
  is exactly the block payload, the counts and the placeholder indices match
  `metaflac`, every real point's three fields match the rows, `frame_number`
  matches the frame the walk counts, and the wrong-base reading is named as not a
  frame start.
* `scan_frame_walk_layout::a_seek_point_starts_a_walk_that_finishes_the_stream_at_the_right_sample`
  — the point of the item: for every real point, anchor the walk at that frame and
  finish the stream. Both off-by-one anchors are refused **before any sample is
  written** (`FrameOutOfSequence` carrying the expected and found numbers), the
  wrong-base byte is refused with the cursor unmoved and `decoded().frames == 0`,
  every frame the suffix walk produces has the offset, number, **byte length** and
  block size the whole-region rows say it has (a walk that starts mid-stream has
  no licence to frame it differently), and the decoded suffix hashes to the
  recorded tail pin — including the last-point case, where one frame of 368
  samples is the whole remaining audio. The lookup-driven path (`find(total−1)`) is
  walked too, so the test is not only driving anchors it chose by hand.

**Mutation witness — four rows of the table, lied to one at a time.** Same
discipline as 6b-1's: the lie goes into the *table*, never into the Rust, and the
answer is which test convicts.

| Mutation (all in `s6`'s rows) | Convicted by |
|---|---|
| a point's `stream_offsets` +1 (4713→4714) | 4a only: "point 6's stream offset" — 4b reads the table's own bytes, not that row, and stays green |
| a point's `frame_indices` 5→6 | **both**: 4b "point 3" (the anchor it would have used), 4a "point 3 (sample 2560) names frame 5; counting frame starts found 6" |
| a point's tail PCM hash, last digit +1 | 4b only: "decoding from sample 0 produced audio that is not the reference decoder's from that sample to the end of the file" — 4a reads no PCM |
| `seek_placeholder_indices 7` → `none` | 4a only: "the placeholder index list is not one entry per placeholder" |
| honest green (no mutation) | 25 passed across the two suites, 0 failed |

Which suite owns which row is the finding: the container reading owns the table's
*contents*, the walk owns what the table *does*, and neither can substitute for
the other. A suite that read every row would be one reading, not three.

**A fact about the frame filter that this fixture is the first to show locally.**
On five of the six frame regions the bare two-byte sync shape matches exactly the
real frames; on `s6` it finds **13** candidates for 12 frames. (The over-match on
longer material — 1.1× to 9.4×, 204 candidates for 157 frames — has been in the
table's header for two sections.) The 4b suite now re-derives that count from the
same bytes, isolates the single extra byte, and names where it came from: it is
the **last byte of the frame before a real frame start** — that frame's own
CRC-16 footer happens to end in `0xFF` — standing immediately in front of the next
frame's sync code, so a reader that took a matching pair as a boundary would have
been one byte short of the frame it was aiming at, and the field check is what
refuses it: read at that position the pair is `FF FF`, whose reserved bit (§9.1)
is set. The strict leg still returns exactly 12. The suite refuses to keep this as
a count it repeats: if the fixture stopped showing it, the test says so instead of
quietly witnessing nothing.

**The gates.** `make flac-gates` green: 1,476 host tests across 72 test binaries,
0 failed, 3 ignored, all gated feature combinations and the second bare-metal
target. `make native-flac-rom native-flac-rom-default-kernel` builds both images.

**And the product images moved, so both were measured against a control.** This
row's library changes are test- and API-side only, so the claim "nothing reached
the ROM" is exactly the kind this plan has been burned on before (6b-1's row
recorded 8 changed bytes from a *comment* edit — the panic `Location` line numbers
that ride beside the file-name string). Control built from 6b-2 pt 1 in the same
tree, same commands:

| image | bytes | sha-256 | `.text` | `.rodata` | `.iwram` |
|---|---|---|---|---|---|
| `flac-integration.gba` — 6b-2 pt 1 (control) | 1,072,860 | `46c23116…` | 70,468 | 999,576 | 2,036 |
| — this half added | 1,070,928 (−1,932) | `e859f7bc…` | 68,532 (−1,936) | 999,580 (+4) | 2,036 |
| `flac-integration-default-kernel.gba` — control | 1,071,032 | `06746b84…` | *(not captured)* | | |
| — this half added | 1,068,132 (−2,900) | `193dd44a…` | *(not captured)* | | |

**A ROM hash is only reproducible at its build path.** The same 6b-1 source
rebuilt in a `git worktree` at `/tmp/wt6b1` comes out 1,072,660 B against
1,072,860 B here — 200 bytes of pure build-directory length, because six
**absolute source paths** ride in the image inside panic-`Location` strings
(`strings -a <rom> | grep home/sam` counts them). Nothing executable moved, and the
two builds time to the identical cycle total. So every hash in this section is a
(commit, build-directory) pair, built at the repository path, and a hash recorded
without its path is not re-checkable by anyone else — which is worth saying out
loud because this file records a fair number of hashes.

**Re-read at HEAD, and it holds.** Re-built at this same repository path, on the
same pinned `nightly-2026-09-04`, warm target dirs, with the pre-change control
taken the way this plan asks — the same tree at `1e5974c`, so the only variable is
the three follow-up commits (`7a07cbb`, `1b6e493`, `968139d`). The rows above are
reproducible to the hash: nothing in this record was mis-transcribed.

| image | `1e5974c` (the rows above) | `main` (the follow-ups) | delta |
|---|---|---|---|
| `flac-integration.gba` | 1,070,928 B `e859f7bc…` | 1,070,960 B `db003f4b…` | **+32 B** |
| — `.text` / `.rodata` / `.iwram` / `.ewram` / `.bss` | 68,532 / 999,580 / 2,036 / 400 / 20 | 68,548 / 999,596 / 2,036 / 400 / 20 | +16 / +16 / 0 / 0 / 0 |
| `flac-integration-default-kernel.gba` | 1,068,132 B `193dd44a…` | 1,068,164 B `57405056…` | **+32 B** |

+32 B in *both* arms, `.iwram` unmoved, `arm_rice_partition` still at `0x0300022c`
with `CTZ8`/`REV8` at `0x030005f4`/`0x030006f4` and the core's `LEADING_ONES` at
`0x030004d4` — all four inside `.iwram`, witnessed by `nm` on the pre-`gbafix`
ELF at `main`. The follow-ups edit seven library files — `format.rs`, `scan.rs`,
`stream.rs`, `driver.rs`, `lib.rs`, `residual.rs`, `rice_hybrid.rs` — plus the test files and fixture rows that read them, and every image in this item
links that library code,
so "test- and API-side only, nothing reached the ROM" is *not* what happened; the
honest version is that it reached the image as 32 B (0.003%): +16 B of `.text`, +16 B
of `.rodata`, `.iwram` unmoved — and no cycle change anyone has measured, because
`main` has been run nowhere. **The consequence is the one that matters for item 7:
every cycle row in this record — mGBA and hardware — belongs to `1e5974c`'s bytes.**
Ruled on the day it was measured, by the owner: **32 B is not material** — 32 B on a
1.07 MB image whose `.text` is 68 KB is not a change that owes its own hardware tip
before it can be believed — with the caveat that the growth was suspected to be "the
seek bits added". That part is measured, and it is **not** the mechanism, which this
file already established once: `nm` carries no `flac_lite::stream`, `seek` or
`FrameNumber` symbol at `1e5974c` or at `main` (re-checked while taking this row:
zero matches in 254 defined symbols), because the ROM reads a `GAFP` manifest and
never a FLAC container, so the seek API cannot be in the image at all. The mechanism
is the one 6b-2's own `nm -C` investigation named — rustc re-cloning the hot bit
reader — and that shape is still present at `main`: `BitReader::read_bits` plus a
second clone `read_bits.783` and an out-of-line `rice_quotient`. So the row is
recorded and not chased: the ruling and the mechanism agree that this is below the
level that needs its own reading, and item 7-1 re-reads both arms at the 32 kHz
product profile anyway, which is where current bytes get their numbers. What is
deliberately *not* measured is a per-commit split of the 32 B: the three follow-ups
touch seven library files, and splitting the growth would need a build at each commit
— not run, because the ruling made it moot.

**One trap in taking that section witness, hit on the way here.** Both arms build
the *same binary name* into the same target dir, so
`examples/flac_integration/target/thumbv4t-none-eabi/release/flac-integration` holds
whichever arm was built **last** — and the two differ measurably: `.iwram` is 2,036 B
(`0x7f4`) with the kernel and 812 B (`0x32c`) without, a 1,224 B gap that is exactly
the thing a witness is supposed to confirm. Running `make native-flac-rom
native-flac-rom-default-kernel` therefore leaves a control-shaped ELF sitting in the
path a product witness would read, which is how a reader can conclude the kernel is
missing from an image that contains it. Read the ELF between the two builds, or
re-run `make native-flac-rom` immediately before the witness — the `.gba` outputs are
distinct and correct even when the ELF is not, so a size/hash check on the ROMs will
not reveal the mix-up.

**The gates, both sides.** `make flac-gates` at `1e5974c` on this host reproduces
the record's **1,476 passed / 0 failed / 3 ignored across 72 test binaries**
(7 lib unittests + 65 integration binaries) exactly, and at `main` it is green at
**1,498 / 0 / 3** across the same 72 binaries. The +22 is the follow-ups, and it
is spread, not concentrated: `flac-test` 200→**202**, `flac-test-wide`
182→**184**, `flac-test-crc` 190→**192**, `flac-test-scan` 233→**237**,
`flac-test-rice` 203→**205**, `gafp_asset_facts` 6→**8**, `crc-check,scan`
223→**227**, `crc-check,scan,rice-hybrid` 239→**243** (the `+1 ignored` in each
scan-flavoured arm is unchanged). Diffing the two runs test-name by test-name
accounts for the 22 exactly — 29 added test-executions, 7 removed:
three unit tests under `stream::tests::seek_table_shape` (two new —
`a_seek_offset_outside_the_frame_region_is_refused`,
`fixed_block_numbering_owns_both_directions` — and one renamed away from
`frame_index_needs_fixed_blocking_…` to `frame_number_needs_…`), each running in
the seven lib-unittest configurations, is +14; two new `gafp_asset_facts` tests
(`the_bit_positions_start_and_end_where_the_headers_and_footer_say`,
`the_subframe_rows_are_internally_consistent_with_their_frames`) in the four
invocations that build that binary is +8. Note what that diff says about the
code: those three unit tests compile in the `instrument`, no-default-features,
`crc-check` and `rice-hybrid` arms too — `stream`'s seek-table work is not
`scan`-gated — and the follow-ups edited library files that every image in this
item links, which is the +32 B. Nothing else about the tree's state moved:
`make check` clean, `make test-rom` still green and still `Running 0 tests`.

Both images, run headless (`timeout -k 5 45 mgba-test-runner <rom>`), each image
twice, and each image's two runs identical to the cycle:

| image | kernel | mean c/frame | worst c/frame | proof |
|---|---|---|---|---|
| control | `rice-arm` (product) | **396,492** | 398,703 | `pcm_fnv=0x54C7B356621B6E15`, `verdict PASSED` |
| this half | `rice-arm` (product) | 395,850 (**−0.16%**) | 398,037 | same pin, `verdict PASSED` |
| control | `TableCountKernel` (6b-1's A/B control) | **1,559,312** | 1,567,136 | same pin, `verdict PASSED` |
| this half | `TableCountKernel` | 1,682,460 (**+7.90%**) | 1,690,900 | same pin, `verdict PASSED` |

The control was not taken on trust: 6b-1's own merge (`654c7f8`) was checked out
and built, in a worktree and at the repo path, and reproduces the mGBA figures
6b-1 recorded **to the cycle total** (62,249,317 / 244,812,048) — so the two
columns are one environment and the comparison is a comparison rather than a
recollection. Neither image decodes differently: same frames, same samples, same
PCM pin, same verdict.

**Then the two new images went to silicon** (owner's run, 2026-11-11, GBA 001 and
GBA SP; **all four readings identical on both units to the cycle**, every screen
`157 frames`, `PASS`, blue, `pcm_fnv=0x54C7B356621B6E15`):

| image | kernel | hardware mean | hardware worst | hardware vs mGBA | hardware vs 6b-1's reading |
|---|---|---|---|---|---|
| this half | `rice-arm` (product) | **396,066** (37.765%) | **398,255** (37.974%) | **+0.055%** | **−0.162%** |
| 6b-1 / pt 1 | `rice-arm` | 396,709 (37.83%) | 398,923 (38.04%) | +0.055% | the reference |
| this half | `TableCountKernel` | **1,737,135** (165.638%) | 1,745,843 (166.468%) | **+3.25%** | **+7.09%** |
| 6b-1 / pt 1 | `TableCountKernel` | 1,622,116 (154.67%) | (155.45%) | +4.03% | the reference |

Three things come out of that, and the third is the one that changes a rule.

* **The product path's small win is real, and mGBA predicted it almost exactly**
  (Q13 was parked precisely because this leg's answer did not depend on it, and the
  park holds): −0.1619% in mGBA, −0.1621% on the units. The shipping number
  improves to **37.765% mean / 37.974% worst**, 2.63× headroom, 193.4 c/sample —
  so this PR does not
  weaken C2's decode claim; it slightly strengthens it, and the claim is now pinned
  to an image whose hash is in this file.
* **The instrument kernel's +7.90% is not an emulator artifact**: +7.09% on
  silicon, ≈1.74 M c/frame, 166% of a frame. Nothing in the plan depends on the
  default kernel being *fast* — it is the A/B control, and it was already over
  budget — but 6b-1's recorded control readings (1,622,116 / 154.67%) are now
  superseded as the current baseline, and the kernel's advantage read through the
  product path becomes **4.39× on hardware** (was 4.09×) and 4.25× in mGBA (was
  3.93×) — a ratio that moved because the *slow* leg got slower, which is exactly
  why it is quoted per image.
* **The emulator's per-leg gap is itself a per-image fact.** The ARM leg's gap is
  a constant: +0.055% at 6b-1's bytes, +0.055% at these. The Thumb leg's is not:
  +4.03% then, **+3.25%** now — it moved *down* by 0.78 pp while that same leg's
  cycles moved *up* by 7%. So the derating rule ("mGBA numbers are derated, not
  trusted, for anything memory-heavy") gains a half: mGBA is a good **screen** for a
  layout shift — it got both directions right and both magnitudes to within 0.8 pp —
  but a per-leg derate may not be inherited across images, because the layout change
  moved the derate.

**Why the bytes moved, as far as the ELF supports saying.** `nm -C` over the new
image finds **no** symbol from `flac_lite::stream` and none matching `seek`: the
added API is not in the ROM, and cannot be — the ROM reads a `GAFP` manifest and
never a FLAC container. What changed is rustc's cloning of the hot bit reader. At
the control, `BitReader::read_bits` exists as three specialised copies (432 + 464
+ 588 B), `frame::resolve_sample_rate` is 1,144 B and `residual::decode_residual_
with` is 2,976 B; in the new image `read_bits` is two copies (424 + 576) and
`rice_quotient` appears as a third out-of-line copy (348 B), while
`resolve_sample_rate` falls to 222 B and `decode_residual_with` to 1,956 B.
Summed over the symbol table that is −1,948 B against a measured `.text` delta of
−1,936 B; alignment carries the difference. A one-file probe attributed the first
356 B of it to the three new `Error` variants alone — with `size_of::<Error>()`
**unchanged at 24 B / align 8 on both sides**, which is the instructive part: what
a `Result`-returning function's ABI depends on is not only the error type's size
but its discriminant range.

**What that is worth to the plan.** The shipping path got 0.16% faster and the
instrument kernel 7.9% slower, in mGBA, from library code that is not in either
image — and the units said 0.16% and 7.09%, so the whole effect is silicon and not
emulator. That is the 6a-7x finding one layer down: the leg whose time is
instruction fetch moves on layout, and it moves by an amount the emulator prices
slightly wrongly *and in a way that changes per image*. The rule to carry forward is
therefore two sentences, and the hardware row above is what the first one costs to
satisfy: (i) **cycle pins in this file are per-image facts** — any PR touching
`bits`, `subframe`, `residual`, `frame` or `Error` owes a control build and both
kernels' readings, as 6b-1's row did; and (ii) **an inherited per-leg mGBA derate is
not one of the things a pin carries** — re-read the leg if the derate matters to the
claim. Here the answer to both was "the product path is fine", which was not known
before it was measured.

**The decision about the 6b-1 stand-in: keep it, and change what it is for.**
6b-1 wrote `scripts/gen_gafp_blob.py` as a stand-in and recorded that it owed a
differential; 6b-2 pt 1 paid it (`gafp-pack-check` step 1: the real packer builds
the perf clip from its own source PCM and the result is byte-identical to
`examples/flac_integration/assets/spike_l0.gfp`). Retiring it as *the writer of
the product asset* is therefore safe — and it is still not the right move, for a
reason that only shows up once the equality is paid: the stand-in is no longer the
interesting thing, but it is load-bearing under a different name. It builds the
product blob **out of the spike track's own committed frame table**
(`examples/flac_spike/src/assets.rs`), so it is the gate that says *the asset the
ROM decodes is still the asset the perf spike measured*. Every cycle figure in
6a-7x and 6b-1 — including the four rows immediately above — is a measurement of
the ROM because of that tie, and deleting the tie would leave two committed frame
sets (the spike's, the product's) with nothing comparing them. The packer does not
supply it: it re-derives frames from an encode of synthesized PCM, which is a
different path to the same bytes and cannot say the two committed sets still
agree.

So the roles are now written down where they live rather than inferred:
**`scripts/gafp_pack.py` is the writer** — it is the thing that produces a GAFP
from audio, it is the thing `gafp-check`'s byte-identity is checked against, and
`scripts/pack_flac.sh` is its front-end when one is wanted.
**`scripts/gen_gafp_blob.py` is the tie** — an encoder-free reading of the
committed frames behind the product asset, and the generator of the ROM's pin
constants (`src/gafp_pin.rs`). Full retirement still has a defined shape and is
owed to a later PR, and what it would take is now on the record: commit the perf
clip as a `.flac` (so the packer can pack it as it stands, with no encoder in the
gate), move `gafp_pin.rs` generation onto the packer's own walk, and — the part
that is not a refactor — replace the spike-table tie with something that still
proves the ROM's asset and the spike's asset are one asset. Nothing in this half
of 6b-2 needed to settle that, and pretending otherwise would have traded a
working cross-check for a smaller script count.

**Two stale records corrected in the same pass, because they were the ones a
reader would act on.** `scripts/pack_flac.sh`'s header and its runtime message
still announced that the GAFP format contract "is settled; the implementation is
not" — six weeks after the packer landed and entered the gates. It remains exactly
what it was: the unimplemented profile-driven front-end, called by nothing,
writing nothing; what changed is the sentence a reader meets first, which now says
where the implemented thing lives (`scripts/gafp_pack.py`, gated by
`gafp-pack-check`) and why the wrapper was not bolted on when the packer arrived —
asserting a stream against the measured decode path and negotiating a stream into
it are two jobs. The Makefile's `gafp-pack-check` header used to describe
`gen_gafp_blob.py` as a stand-in whose retirement "6b-2 makes deliberately rather
than by drift"; it now points at the decision above.

**Open axes this half leaves.** The two 6b-2 pt 1 carried (1024-sample blocks;
packed CONSTANT/VERBATIM/escape-shaped and higher-order-LPC assets, the highest
LPC order any real-stream table here carrying is 8), plus three the seek work
exposed: a **variable-blocksize** stream with a seek table, where the byte-offset
half is useful and the arithmetic half is refused — nothing here shows the
accepted half; a nonzero first frame number is **not** a seek fixture: §9.1.5's
MUST makes it non-conformant and says seeking there is not reliably possible, so
it belongs to the recovery path (`Frames::anchored`), not to the table's number
line;
and a fixture that **edits an existing table** rather than writing a new one
(`metaflac --delete-all-points` + `--add-seekpoint` on a copy of `s2` was probed
and produced several placeholders — recorded as a measurement, not committed,
because it would have rewritten a fixture five other suites read).

### 6b-3 — landed at the headless level: playing at the target rate, and a model of why the geometry holds (2026-11-11)

**Ruled out as a product path the same day it landed (Q14).** This image plays the
shipped 32,000 Hz asset on a 32,768 Hz clock. The project owner ruled on 2026-11-11
that a stream is encoded at the rate it is played at and the player does not
reinterpret a declared rate: **1.024× is 41.06 cents**, and zero underflows, a matched
PCM digest and a 571/571 period audit do not buy that back. So the asset in `assets/`
is **not a product asset**, `geometry.rs`'s `32_000` arm is measurement scaffolding with
a `TODO(Q14, BLOCKING)` on it, and the guard against ever drifting back is
`ruled_rate_policy_allows_one_bridge_and_no_more`, which computes the cents instead of
quoting them and refuses any further reinterpretation. What 6b-3 established — the play
path, the geometry, the flow-control rule, the measured envelope — is carried forward
unchanged; what it played is not. The next PR is 7-0, the encode at the right rate.

**What landed.** Three files and one gate. `examples/flac_integration/src/geometry.rs`
is the single source of the playback numbers — 2^24 mixer ticks, 32,768 Hz, 512
cycles per sample, a 560-sample period (the size `agb`'s own mixer would program for
that rate), a 36-chunk ring, a 16-chunk prefill, a 16-sample re-arm grace — with
compile-time pins on every transcription taken from `agb` or `scripts/gafp_pack.py`,
and the 32,000 → 32,768 Hz rate mapping that the packer already declares — an arm now
marked **`TODO(Q14, BLOCKING)`**, ruled not a product path the same day this landed.
`src/lib.rs` makes that geometry plus the spike's real `RingFeed` and checksum code a
library both a ROM and a host test can use (shared by `#[path]`, not copied).
`src/product_play_main.rs` is the product-play binary: prefill, prime one ping-pong
buffer, arm the timebase and the sound DMAs, decode-and-push while the ring has room,
service the period from the Timer1 handler, drain, then prove the PCM and print the
ledger on the serial log and the screen. `make product-play-rom` builds it;
`make product-play-test` runs the model. `agb` and `flac-lite-gba` moved under
`[target.'cfg(target_arch = "arm")']` so the library can build for a host at all —
`agb`'s `critical-section`/`portable-atomic` chain is a hard `compile_error!` off
ARM — and the ROM binaries are cfg-gated, which is why the host model can exist. The
rebuilt image is byte-identical to the pre-restructuring one: same hash, which is how
we know the portability work moved no product code.

**What the run says (mGBA, one 100-second run, `5dc7869…`).** The full record is in
`measurements.md`; the load-bearing lines:

```
feed: pushed=320000 expected=320000 consumed=320000 frames=157/157 underflows=0
period audit: hw_periods=571 serviced=571 und=0 coalesced=0 max_gap=0 late=0
              max_delay=0 grace=16 tail_periods=1 tail_real=240
lead (samples): min=9120 max=19600 ring holds=20160 a period needs=560
                 margin over one period=8560
budget: work/frame=556405 c = 53.062% (decode 40.657% convert 10.832%
        push 1.457% feeder 0.495%) of a 1048576 c frame
worst single frame's WORK: 622517 c = 59.367%; frames over their own period: 0
```

The short final period is classified `Tail`, not an underflow, and counted separately:
one period of 240 real samples at the end of a 320,000-sample ledger is arithmetic, not
a glitch, and a run that reported it as an underflow could not be compared with one
that happened to end on a frame boundary. `tail_periods=1 / tail_real=240` is what
makes the ledger close visibly: 571 × 560 = 319,760, plus the 240-sample tail = 320,000.

**Two reporting rules this image had to learn the hard way.** A flow-control **wait is
not a cost**: an earlier version of the stage accounting let the wait ride inside
`max_work` and "over my own period", which priced a pause the producer took *because
the audio was fine* as if it were a overrun of it. So work is decode+convert+push, and
the pacer's wall span (mean 1,015,859 against a 1,048,576-cycle frame) is its own
line — it is the number that shows the producer tracking the clock, and it is not a
budget claim. Second, the period boundaries that land *inside* a stage window are
system cost, not that stage's cost: 159 of them sit inside decode windows here, so the
image reports the boundary count, prices them at the measured 5,194-cycle feeder cost,
and prints its attributed isolated estimate (420,894 c/frame) **beside** the decode
row of record (396,066/398,255) instead of reconciling them. The two are different
measurements — one is a decode instrument, one is a running player with the ring
resident in EWRAM — and the difference is the interesting part, not an error to shave.

**The model, and the one bug the cross-check caught.**
`tests/product_play_witness.rs` replays the real decoded frame lengths through the real
`RingFeed` against a credit-per-period clock, charged the cost the ROM measured. It
agrees with the ROM digit for digit on `periods=571`, `tail_periods=1`, `tail_real=240`,
`primed=560`, `min_lead=9120`, `underruns=0` — sharing only the ring source and the
constants. The one place they deliberately do not agree is the **peak** lead: the
model's is the ring's ceiling (20,160 samples), the ROM's sits exactly one period lower
(19,600 — 560 samples, exactly one period), which is what a ring that is being drained
concurrently looks like. The test
asserts the floor exactly and the peak only to within a period, and says why in the
assertion — a model that reproduced the peak too would be modelling a ring nothing
drains. That agreement was not won at the first attempt: the draft predicted a floor
of 7,840 against the ROM's 9,120 and explained the gap as clock coarseness, when the
truth was a prefill loop in the model that pushed until the ring was full instead of
stopping at its target the way `Producer::prefill` does. Fixing that rule made both
sides derive the same 10,240-sample quantised prefill and the same 9,120 floor. A
model that matched a ROM on everything except one number, and treated the difference as
physics, would have kept a bug.

**What the geometry is actually worth — and what it was worth in prose before.** The
model measures two tolerances that the plan and `geometry.rs` had been treating as one
"generous margin", and their numbers are an order of magnitude apart:

| the question | the answer, measured |
|---|---|
| how much of a **sustained** producer overage survives? | **157.3%** of the measured cost with no ring in the picture; **196.0%** with the shipped one (183.5% at the hardware derate). The ring's refill buys ~39 points of sustained overage and then nothing. |
| how long can the producer **stop**? | **17 periods of audio in hand** at the shipped geometry — the quantised prefill of 10,240 less the prime of 560 — with the **18th period the first with nothing to take**. The lead ceiling is two numbers, not one: the deepest lead at which the flow control will still **start** a frame is **32 periods** (`(ring − declared blocksize)/period`, because it holds a frame's worth free before decoding), while a frame already started pushes its 2,048 samples into that reserve, so the lead **peaks** at 35 chunks in the ROM and at the ring's full 36 in the model. |
| what is the smallest ring that carries the measured cost (plus the standing derate)? | **20 chunks**, and 20 is not a cost answer: below it the geometry cannot prefill at all. The 16-chunk prefill sets the floor under the depth (`8,960 + 2,048 = 11,008` samples), so the two knobs are one decision. |
| what is the smallest prefill that carries it? | **1 chunk** (lead floor 368 samples). Prefill costs boot time; depth costs 40 kB of EWRAM. That is why the prefill is set high and the depth is the number worth arguing about. |

Three claims fall out of that: "36 chunks = 36 periods of stall" (no: the stall answer
is 17 periods in hand with starvation at the 18th, and the 32 that appears in the
arithmetic is the lead at which a frame
will still be *started*, not a depth the lead reaches — the observed peaks are 35 and
36), "a deep ring buys tolerance for a sustained
overage" (it buys ~39 points of it, not an unbounded amount), and "the minimum prefill
lands near 2 chunks" (it is 1). `geometry.rs` now cites the measurements instead of the
assertions.

**What 6b-3 does not say.** It is not C2, and the gap is not narrow: this image has
been run on **no hardware**, its stage shares are mGBA system-path numbers, and the
plan's C2 arithmetic — the 38.04 + 22.6 + 20.3 ≈ 80.9% line, which is the older
10,512 Hz convert/pair charged against a 32,768 Hz frame — is now **suspect in the
favourable direction**: measured on this path, convert and push together came to
12.3% of a frame, not 43%. Do not rebuild C2's margin from either figure. That is
7-1's job on silicon, with one denominator chosen rather than two sitting in the tree
(2^24 = 1,048,576 c/frame here; `flac_spike`'s `stats.rs` still says 16,780,000 =
1,048,750, and the difference is 0.0166%). And 7-2 still owns the question this image
sidesteps by construction: the feeder here is a hand-built timer/DMA/IRQ path, written
because `agb`'s mixer has no way to be told "refill me from a producer you do not
own"; whether that is the shipping path or a stand-in is a decision with a witness
(bit-exact PCM into the FIFO plus an audibility check on hardware), not a preference.

**Follow-up probe, same day: the rate mapping's mechanics, measured (Q14).** The
shipped blob's own frames, re-spelled as a stream that *declares* 32,768 Hz — every
frame's 4-bit rate code set to `0b0000` (get-it-from-the-stream), each header CRC-8 and
frame CRC-16 recomputed, a hand-built STREAMINFO carrying 32,768 with min/max blocksize
at the full block — come to the **same 865,996 bytes**: no frame changes length. The
reference decoder calls it `ok` and decodes PCM byte-identical to `spike_l0_pcm.bin`.
Both product images were then rebuilt against that blob and run in mGBA, each paired
with a same-session re-run against the shipped one: the decode-only instrument moved
**−20 c/frame (0.005%)**; the playback image +415 c/frame of decode and +1,235 c/frame
of work (0.22%) against a closure residual that moved 131,822 cycles by its own account,
with `underflows=0`, `571/571` periods, `min_lead 9120` and an *identical*
`audio_span` in both. So the runtime half of "should the asset be encoded at the
playback rate" is closed, and it is free. What the probe does not buy is the pitch:
relabelling changes no sample, so the 41 cents lives in the resample step, and the
encode profile still refuses the new key `(32768, 2048, fixed, 0)` — a row whose
proper instrument is 7-1's hardware read, not this mGBA pair. Two prose corrections
come out of it: the `geometry.rs` comment saying a true-32,768 asset "would still have
to declare 32 kHz" was wrong — 32,768 has no 4-bit code, but it has two legal
spellings, `0b0000` + a stream default at no size cost and `0b1101` + a 16-bit field at
2 B/frame, and `crates/flac-lite`'s header parser already reads the second
(`s3_wide_48001hz_b3000` is a real 80-bit header) — and Q14's "the producer's budget
is 2.4% tighter" is true as arithmetic and prices nothing: the frame deadline is the
player's clock in both cases, and no instrument here budgets from the declared rate.
**Ruled the same day, and the ruling is what the probe was actually for**: the pitch is
not negotiable, so the product asset will be encoded at 32,768 (7-0), and the row
`(32768, 2048, fixed, 0)` is 7-1's to fill on hardware. The numbers are in
`measurements.md`, the 6b-3 rate row; the blocking TODO is `TODO(Q14, BLOCKING)` in
`examples/flac_integration/src/geometry.rs`.

## Item 7 — playback: the real-time claim (C2)

Item 6b makes the frame path real and puts the kernel in the ROM. Item 7 is what
turns "decodes inside the budget" into "plays in real time", and it is where **C2 is
met**. Its scope comes straight from the arithmetic in "Acceptance criteria": the
producer stages were last measured on the **pre-kernel** path at **10,512 Hz**, they
are per-frame work (2048 samples regardless of rate), and therefore at 32,768 Hz
they are ~43% of the frame against decode's 42.72%.

| PR | Scope | Witness | Failure it isolates |
|---|---|---|---|
| **7-0** | **The asset at the rate it will be played at.** Q14 was ruled on 2026-11-11: a stream is encoded at the rate it is played at, and the player does not reinterpret a declared rate to fit the clock. So: resample the source to 32,768 Hz, encode it at the product profile (FIXED-0, 2048-sample frames, the residual shapes the guard admits — a fresh census, not this asset's), repoint `scripts/gafp_pack.py`'s `PLAYBACK_RATE_FOR_STREAM` at the identity, and take the `32_000` arm out of `geometry.rs`. It will pack at first only under `--allow-unmeasured`, and that is the honest state rather than a shortcut: the `this is NOT a product blob` note comes off when 7-1's row exists. For the same playing time such a stream carries 2.4% more samples, so expect ~2.4% more blob — arithmetic, and worth measuring rather than assuming. | A **pitch witness in the same PR**: a tone fixture whose fundamental is checkable in the decoded PCM. Nothing in the present gate set can see a pitch shift at all — the pinned digest of a 32,000 Hz asset and of a reinterpreted one are the *same bytes*, which is how 41 cents passed every gate this asset has been through. Plus the profile guard with no relaxation flags, and `ruled_rate_policy_allows_one_bridge_and_no_more` going red exactly once, when the bridge is deleted. | That every gate 6b-3's asset cleared was deaf to its one real defect — and that 7-1 and 7-3 would otherwise be run against an asset the product cannot ship. |
| **7-1** | **Re-measure the producer at the target rate.** The 10,512 Hz producer pass (convert / push / wait / closure) repeated at 32,768 Hz with the kernel in the decode slot. Measurement only — no new product code, and it runs against **7-0's** asset. | Per-stage shares on **hardware**, same-image, with the rate pass's own discipline (`closure underflows 0`, shares read relatively, the clean image's audit as the wall authority, and the same run's decode figure recorded against `(32768, 2048, fixed, 0)`, which is the key `gafp_pack`'s guard is missing — 7-1's numbers are what take the non-product note off 7-0's asset) | that C2's margin is inherited arithmetic from a different rate and a pre-kernel decoder — the ~85.6% gross and the ~8.7 ms left are estimates, not observations, and that 6b-3's 53.062% envelope is an mGBA figure for an asset the ruling disqualifies |
| **7-2** | **The shipping playback path.** `agb`'s mixer/DMA, or a written argument for why the custom IRQ feeder is the shipping path. Replaces the throwaway harness the rate track has been leaning on. | Bit-exact PCM into the FIFO plus an audibility check on hardware; underruns **counted**, not eyeballed | that the throwaway feeder was load-bearing in ways nobody wrote down (the rate track's own lesson, one layer up) |
| **7-3** | **The acceptance run.** 2048-sample frames at 32,768 Hz, FIXED-0 assets (enforced by 6b-2's pack-time profile), GBA 001 **and** GBA SP, two runs each. | **Underruns = 0**, plus 7-1's stage shares on the same image | that C2 is claimed from a mean and a projection rather than from playback on a unit |

**C2 is met when 7-3 passes on both units**, and not before. "Decodes within
budget" was already true at 42.72% and is not the claim; it is the precondition.

**Sequencing note, re-cut on 2026-11-11 by Q14's ruling.** 7-1 cannot run until 7-0
lands. That is the ruling's consequence, not a slip: the image 7-1 measures has to be an
asset encoded at the rate it will be played at, because the stage shares it produces are
the shares the product would run under and the row it fills is
`(32768, 2048, fixed, 0)`. So the order is **7-0 → 7-1 → 7-2 / 7-3**; 7-1 stays the PR
that decides whether 7-2 has to find savings or merely has to land, and if 7-1 shows the
producer stages eating the whole margin that is a decode-side or rate-side finding and it
comes back to this plan rather than being absorbed by a playback patch. 6b-3's numbers do
not carry forward as product figures: they are the play path's and the geometry's
evidence, measured against an asset the ruling disqualifies.

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
counts and the default instrument** (its delta is the rung's record, above). **The
table below is the 6a-era baseline and is kept as one**; the live counts at
`main` are in the superseding table after it, measured 2026-11-11:

| Gate | Count |
|---|---|
| `flac-test` | 183 + 1 |
| `flac-test-wide` | 178 + 1 |
| `flac-test-crc` | 186 + 1 |
| `flac-test-scan` | 203 + 1 |
| `flac-test-rice` | 199 + 1 |
| `crc-check,scan` | 206 + 1 |
| `crc-check,scan,rice-hybrid` | 222 + 1 |
| `flac-gates` | **1,377 passed + 6 ignored** *(6a-era; see the note below)* |
| `spike-test` | 15 + 10 |
| `item6a7-test` | 15 + 1 + 10 |
| `kernel-test` | 20 |
| `flac-portable-check`, `make check` | green |

**Superseding counts at `main` (`e3d377e`), measured 2026-11-11 on the agent host
at the repository path**, warm target dirs, pinned `nightly-2026-09-04`; the same
command run at `1e5974c` on this host reproduces the 1,476 row, which is what
makes the per-arm deltas below trustworthy rather than remembered:

| Arm | `1e5974c` | `main` |
|---|---|---|
| `flac-test` (`--features instrument`) | 200 + 0 | **202 + 0** |
| `flac-test-wide` (`--no-default-features`) | 182 + 0 | **184 + 0** |
| `flac-test-crc` (`crc-check`) | 190 + 0 | **192 + 0** |
| `flac-test-scan` (`scan,instrument`) | 233 + 1 | **237 + 1** |
| `flac-test-rice` (`rice-hybrid`) | 203 + 0 | **205 + 0** |
| `gafp_asset_facts` (the named leg of `gafp-pack-check`) | 6 + 0 | **8 + 0** |
| `crc-check,scan` | 223 + 1 | **227 + 1** |
| `crc-check,scan,rice-hybrid` | 239 + 1 | **243 + 1** |
| **`flac-gates` aggregate** | 1,476 passed / 3 ignored / 72 binaries | **1,498 / 0 failed / 3 ignored / 72 binaries** |
| Product images | `flac-integration.gba` 1,070,928 B `e859f7bc…`; `…-default-kernel.gba` 1,068,132 B `193dd44a…` | **1,070,960 B `db003f4b…`; 1,068,164 B `57405056…`** |

The `+0`/`+1` is ignored tests; the aggregate also prints 7 empty `Doc-tests
flac_lite` result lines, which is normal and is not one of the 72. Note the shape
of that table: the arms that gain `+2` are the ones whose *only* added tests are
the three `stream::tests::seek_table_shape` unit tests — they compile in every
arm, `scan` is not required for them — and the arms that gain `+4` are exactly the
four invocations that build the `gafp_asset_facts` binary. So the drift is in
library code (`stream.rs`) plus the packer's own fact suite, and the library half
is also why both product images grew 32 B. Rebuild both at `main` with
`make native-flac-rom native-flac-rom-default-kernel` before quoting any cycle
figure against them: they have never been run on hardware or mGBA.

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
  It runs once per candidate rate × path combination, on hardware. (It has run: the
  fetch ratio is hardware, 3.939. What it did not cover is the data-read path, and
  that gap is now covered by the derating rule below rather than by a probe.)
- **mGBA numbers are derated, not trusted, for anything memory-heavy.** 6a-7b
  measured one image on two units and every leg came out **3.7–6.9% above the
  model**, while E5's pure-fetch stream matched to 0.05%. So: use a hardware figure
  where one exists; where none exists — today that is the producer's convert/push
  stages — carry the mGBA figure with a **+7% derating** and say so in the record.
  The shortfall is per-leg and does not cancel, so one arm's mGBA number may not be
  scaled onto another arm either. **6b-2 pt 2 re-read both legs at new bytes and
  the gap itself moved**: the kernel leg is +0.055% on hardware at 6b-1's image and
  +0.055% at pt 2's (a constant), while the Thumb default-kernel leg was +4.03% and
  is now +3.25% — down 0.78 pp on the same leg whose cycles rose 7%. So a per-leg
  derate is a property of an image, not of a leg: mGBA is a reliable *screen* for a
  layout change (it got both directions right, and both magnitudes to within
  0.8 pp), but a derate is not inherited across images.
- **Criteria before attribution.** C1 and C2 are the definition of done; a
  measurement that does not decide one of them waits behind the work that does. This
  is why the data-read probe is parked (Q13) and why 6a-8 stays untriggered.
