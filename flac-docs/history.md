# History and rulings — how the FLAC track got to its present shape

> **Scope: the FLAC line of work.** This is the record of decisions, rulings,
> experiments and closed questions for `crates/flac-lite`, `crates/flac-lite-gba`,
> `examples/flac_integration`, `examples/flac_spike` and the `GAFP` asset. Nothing here
> decides anything for the other codec tracks ([`../ULI2.md`](../ULI2.md),
> [`../ULC.md`](../ULC.md), [`../OPUS.md`](../OPUS.md)); "the product" means the **FLAC**
> product path.
>
> This file replaces four working documents that the completion of the work made
> redundant — `history.md` (4,055 lines), `history.md` (1,008),
> `history.md` (1,434) and `history.md` (1,386). Their *conclusions* are
> here; their dated, per-run, per-image narratives are in git history at
> `87f1b86` and earlier, and the numbers of record they produced are in
> [`measurements.md`](measurements.md). Quote this file for what was decided and why;
> quote git for who measured what on which day.

## The acceptance criteria, and where each stands

Two criteria, stated by the project owner on 2026-11-10, replaced the earlier phase/item
roadmap as the definition of done. Both are now closed.

**C1 — `flac-lite` stands alone.** A `no_std`, pure-Rust FLAC decoding crate, ready to be
separated into its own repository: no `todo!()` in `src/`, a facade a stranger can drive
(`decoder::Decoder` over `scan::Frames`, fed by `format::Manifest`), `core` + `alloc` only,
zero dependencies, zero allocation on the decode path, `#![deny(unsafe_code)]`, bit-exact
against `flac -d`, every platform-bound item confined to an off-by-default feature or to
the overlay crate, and `make flac-portable-check` still compiling it for a second
bare-metal target. Closed with 6b-1 (2026-11-10) on the facade clause; the crate has been
complete since. **Explicitly not required:** a crates.io publication — "ready to be
separated" means a `git subtree split` would work.

**C2 — `flac-lite-gba` decodes *and plays back*, in real time, at the highest rate the
hardware achieves, on a GBA.** Met on 2026-11-13 by the master-path hardware reading in
[`measurements.md`](measurements.md): real audio, 16,384 Hz, GBA 001 ×2 and GBA SP ×2, all
four runs audible and recognizable, all BLUE, `und 0`, `late 0`, 289/289 periods, exact
counts and digests. The clauses that made that run a verdict rather than a demo — zero
underruns, zero late services, the 16-sample re-arm grace, both units twice each, the
listening witness, and the declared-rate identity rule — were all kept when the rate came
out of the criterion, and are what a future regression test has to reproduce.

## The rate ruling (2026-11-13)

C2 originally read "in real time, **at 32 kHz**". Four identical hardware runs on two units
put the whole producer chain at **59.946% of the 32,768 Hz frame** for a **FIXED-0** stream
— the cheapest thing FLAC can emit, with no prediction at all — and there was no strategy on
the table for recovering the factor of two that 32 kHz would have needed for real material.
The owner's ruling: *the attribution may tell us where we might make improvements, but
unless we have a real strategy for meaningfully driving those improvements, the conclusion
from these on-hardware runs is that we need to drop the sampling rate.* So the rate became a
**measured result** rather than a commitment, and the honest statement of what was given up
is: **32,768 Hz was reachable only for an asset that is not real music.**

What the release bought, as arithmetic on rows the ledger already held (per-frame costs are
sample-denominated and a frame is 2,048 samples wide at every rate, so dropping a rung
halves every percentage and changes no cycle count): work 59.946% → 29.97%, worst frame
60.755% → 30.38%, attributed decode unchanged at 451,763 c/frame but 43.08% → 21.54% of the
frame, and the ring's 8,560-sample lead going from 0.261 s to 0.522 s. Three consequences
were larger than the percentages:

1. **LPC changed sign**: 173.23% / 174.14% became 86.6% / 87.1% — still not shippable, but
   the product stopped being *forced by the clock* to play an un-predicted clip. **8,192 Hz
   remains the only rung where predicted frames project inside a frame** (43.3% / 43.5%).
2. **An IWRAM-resident ring became arithmetically possible**: the ring is 40,320 B against
   32,768 B of IWRAM, so at 32,768 Hz that design-around was excluded rather than expensive;
   the same 0.615 s of lead at 16,384 Hz is 20,160 B.
3. **The grace window doubled in cycles** (8,192 → 16,384), so the observed 12,800-cycle
   miss would not even have been counted. Read that carefully: a late re-arm at the lower
   rate duplicates 16 samples of a *slower* clock — ~0.98 ms instead of ~0.49 ms — so the
   same miss is **more** audible, not less. Lowering the rate moved the probability and the
   consequence of a miss; it did not remove either, and the fix (below) is what turned
   "inside by luck" into "inside by design".

**Guard-rail against quietly reversing this.** 32,768 Hz was declined on the **cost of real
material**, not merely on the late service. Fixing the push mask does not re-admit it; that
needs a fresh ruling with a measured reason, and the reason would have to be a decode or
convert figure, not a latency one.

## The rung is chosen: 16,384 Hz (same day)

Having ruled the rate out of C2, the owner first directed the project to *relax exactness*
and target "the ~18 kHz rate agb already supports". That instruction rested on a premise
this repository does not satisfy, and was withdrawn the same day in favour of **16,384 Hz**.
Both halves are recorded, because the wrong half is the easier one to re-derive:

1. **Nothing on this path uses `agb`'s mixer.** `product_play_main.rs` states *"no
   `agb::sound::mixer::Mixer` is instantiated in this ROM, so timer0-3 are unowned"* as a
   safety precondition, and the only `gba.mixer.mixer(…)` call sites in the tree are the
   baseline `examples/pcm_playback.rs` demo and the *failed* `symphonia_flac_probe`. The ROM
   programs timer0, the timer1 cascade, the sound-DMA enable and every per-period re-arm
   itself — agb's `enable_dma_for_sound` does not even write a transfer count. So
   `Frequency::{Hz10512, Hz18157, Hz32768}` constrains nothing here. **But it was never
   irrelevant either**: it is a domain this track imposed on itself deliberately
   (`CHUNK_SAMPLES = 560` is transcribed from agb's `Hz32768` buffer size, and the rate
   generator documents failing closed outside the mixer domain). Targeting 16,384 Hz
   knowingly steps outside it, which is free on the clock but makes 560 a parameter to
   choose rather than to copy.
2. **18 kHz was worse, not merely inexact.** The exactly-expressible set is the integer
   divisors of 2²⁴: 65,536 / 32,768 / **16,384** / 8,192. A 924-tick reload plays
   **18,157.16 Hz** (+8.82 ppm, 0.015 cents), and because the re-arm grace is
   *sample*-denominated while the interrupt-masked push window is *cycle*-denominated, the
   faster clock eats more of the grace:

   | | 32,768 Hz | 18,157.16 Hz | **16,384 Hz** |
   |---|---|---|---|
   | ticks/sample | 512, exact | 924.0082 → not an integer | 1,024, exact |
   | declared == played | ✓ | ✗ — needs a tolerance | ✓ |
   | push mask vs the 16-sample grace (mean / worst ever recorded) | 28.7 / 30.9 → **late** | 15.89 / **17.13 → still late** | **14.34 / 15.46 → inside** |
   | FIXED-0 chain work | 59.946% | 33.2% | **30.0%** |

   So ~18 kHz would have traded Q14's machine-checkable equality *and* three points of
   headroom for a defect that still failed. Note also that a lower rung does not fix the
   pitch question for free in the other direction: FLAC's four-bit table has **16,000** and
   **32,000** and neither power of two, so `16,384 / 16,000` is exactly the 1.024 ratio that
   made Q14's 41-cent defect — a retarget has to re-point all three rate guards at the new
   number *before* the asset is re-authored.

The fix that made the margin a design margin: **`CHUNK`-sized critical sections** — one
560-sample push per interrupt mask instead of a whole 2,048-sample frame — scale the
measured 14,680-cycle window to **4,014 cycles**, i.e. 3.92 samples at 1,024 ticks/sample
against the same 16-sample grace, a **4.08× margin** (7.84 samples and 2.04× at 32,768 Hz).
It cost +0.150 pp of work and +0.150 pp of push, measured.

## The pitch ruling (Q14, 2026-11-11) and what it cost to find

**Ruled by the owner: a stream is encoded at the rate it will be played at, and the player
does not reinterpret a declared rate to fit the clock.** This was opened by the discovery
that the shipped asset declared 32,000 Hz while the only clock in the plan was 32,768 Hz —
1.024× fast, **41.06 cents** — and it is worth keeping as a lesson rather than as a
changelog, because of *how* it survived every gate the asset had been through: a pinned PCM
digest is a function of the samples, and the samples are identical whether they were
authored for 32,000 Hz or for 32,768 Hz. A run may service every hardware period, push and
consume every sample, match its pin and still come back 41 cents sharp. For a project whose
entire point is playing audio back correctly, that is a correctness failure and not a matter
of taste — and it is the one failure mode none of C2's other clauses can see.

**The root cause was structural, and a ruling without naming it would have left the shape
that produced the bridge intact.** `gafp_pack.py` keyed its measured-cost table by the
*stream* rate while taking the frame budget from the *playback* rate: one table, two rates,
and a fixture forced to match the half that appears in the key. That is what made
`32,000 → 32,768` load-bearing instead of absurd. Cost does not depend on the declared rate
(measured: re-spelling it moved decode-only cost by −20 c/frame, the instrument's own floor),
so the fix was to re-key cost **structurally** — blocksize and predictor kind/order, with the
residual envelope carried as a stated caveat on each row rather than as a key component — and
demote the rate to a **validity check** asked once. `EncodeProfile` gained
`allowed_sample_rates_hz` (it had no rate field at all, which was the gap that let a mapping
exist where a refusal should have), `RATE_BRIDGE` became `None`, the packer refuses a
foreign-declaring stream before it encodes, and the ROM checks its own embedded blob at
start-up.

Two traps that came with the retarget, both easy to walk into: the fixture generator
specifies its tones in sample *periods*, so re-synthesising "the same" fixture at the new
rate leaves the played pitch exactly where the defect is; and 16,384/16,000 is 128/125, so an
exact resample exists — but ffmpeg's `-ar` is float, and float would break every byte pin in
the repository (the same class of defect as the 6a-2 float-trig incident).

**And a witness had to be built, because the plan's predicted one could not exist.** No gate
in this repository could see a pitch shift, so 7-0 landed one: an integer matched filter over
the decoded samples, two ratios thresholded rather than one, naming the clock the samples were
made at from its period (410:1 on a clean design term, 340:1 on the shipped asset, 66:1/20:1 on
its played 8-bit form, and 93.7579 LSB measured through the real 8-bit converter against the
exact 93.7500 — the truncation costs amplitude and none of the pitch). Its negative leg is a
re-synthesis of the 32,000-era design plus a byte-level respell of the shipped blob. What the
plan expected to be a committed "pack a 32,000 tone as a 32,768 asset" fixture **cannot
exist**, because after the ruling the packer refuses to make such a stream at all — so that
witness runs as a mutation through the whole generation chain instead.

One thing the ruling did *not* buy back: because the cost table is now keyed by shape, a
newly encoded asset **packs cleanly against a row measured on different frames**. That is the
cost model behaving as designed and simultaneously the failure mode the re-keying introduces,
so the packer prints a provenance warning on every pack instead of refusing, and the honest
signal is a warning a gate reading only exit status cannot see.

## Acceptance model (amended 2026-11-13)

C2 is a claim about the **code path**, not a requirement to re-prove the whole stack for
every asset. The path was manually validated once end to end with real audio on both units;
subsequent encodes are judged by the build-time gates and the mGBA control. The
FIXED-0-by-construction and measured-packer-row clauses are retained as **preflight and
hardening** — they catch a bad asset before it reaches a unit — and are no longer the
acceptance gate. Post-closure coverage that would be worth having, recorded here as
*optional* rather than as outstanding work: a stress corpus for `VERBATIM`, FIXED-4 and
wider residual envelopes; a derated mGBA budget model (measured hardware runs ~×1.16 above
mGBA on the worst frame, so a 95% warning has to be applied to the *derated* estimate);
per-asset host-model tests using the mGBA run's own cost; mono playback; and pinning the
ffmpeg resampler version or replacing `-ar` with an integer resampler.

## The E-series — what each probe found

Full narratives with their controls are in git history
(`flac-docs/history.md`, at `87f1b86` and earlier); the probes themselves remain reappliable
under `examples/flac_spike/experiments/<name>/`. The verdicts:

| id | Tested | Verdict | Headline, on its own image |
|---|---|---|---|
| **E1** | Stage ROM→RAM per frame | no collapse, ruled out | FIXED −4.7%, LPC −1.8%; the copy cost eats it (net ≈2.5% FIXED) |
| **E2a** | Bit-reader refill accumulator | no collapse on its own, but it fixed a later regression | FIXED −3.14%, LPC **+1.41%** |
| **E2b** | Run-length table quotient count | **big win, landed** | FIXED −19.17% same-image; FIXED mean 217.4% → 170.2% |
| **E3** | Decode scratch in IWRAM | inert | +20 cycles/frame on both clips |
| **E4** | i64 → i32 intermediates | big win, LPC only | LPC **−46.9%**, FIXED −3.7% — and its safety is profile-bound (holds to precision 13, **fails at 15**) |
| **E5** | Emulator vs hardware arbitration | **measured on two units** | ROM 4.085 / IWRAM 1.037 cycles per add, ratio **3.939**; mGBA reproduced within 2 cycles/call. The fetch pricing is hardware |
| **E6** | Hot decode *code* in IWRAM | **slower**, ruled out — and later refuted in scope | FIXED +17.8%, LPC +9.0%. E6 pinned a *Rust* path where each instruction carried many cycles of work; a tight ARM loop of one-cycle instructions is the case it did not isolate |
| **E7** | `-Cforce-frame-pointers` | inert | byte-identical ROM with and without (thumbv4t forces frame pointers anyway) |
| **E8** | Sub-stage attribution | measurement | FIXED: unary 56.2% + rest 38.5% = 94.6% of gross; LPC: the i64 integrator 49.5% |
| **E9** | Stacked variants | additive; first in-budget FIXED | arm+i32 94.47% mean / 94.95% worst; additivity reproduces to 0.06% |
| **E10** | ARM-mode hybrid codeword loop | **biggest FIXED-side win** | arm − tab **−40.08% FIXED / −11.51% LPC**; 170.2% → 101.98% |

The absolute marginals that survive an era boundary (percentages do not):
**E10 `tab − arm` = −715,300 c/frame** (≈−349 c/sample, FIXED);
**E9 `arm − i32` = −78,550 c/frame** (its LPC equivalent is *not* stable across
denominators); **item 5 PR 4 `arm − ctrl` = +173,649 / +174,165 c/frame** = +42.6 / +42.7
c/sample — the portable twin costs the same constant on a frame a quarter the size.

## The ladder, one line per rung

| Rung | What landed, and what it settled |
|---|---|
| 2026-08-30 | **Option (c): write `flac-lite`.** symphonia's no-std blockers are structural, not accidental; controlling both ends deletes the stack. |
| 2026-09-07 | Frame header measured at **32 bits**, not 31 (the correction that killed a hand-packed vector); the encode profile is a *max LPC order plus flags*, never a "reject LPC" switch — `-l 4` emits real LPC-4 frames. |
| Phase 1 / 2 | Decode path complete through `frame::decode_frame`; CRC-8/16 behind `crc-check`; `driver`'s per-frame loop; `stream::parse`; the table-less `scan` walk (4b). |
| Item 5 | The `RiceKernel` seam: whole-partition, monomorphized, unsealed, default `TableCountKernel`. PR 3 added the portable hybrid twin; **PR 4 took the on-target reading the default flip was gated on and declined it** (+42.6 c/sample). Also the founding measurement of the second-instantiator effect: a `RiceKernel` body merely *present* in a link moves the default path by 26.5 M cycles and grows `.text` by 2,984 B with no symbol of it in the image. |
| 2026-11-02 | Portability ladder rungs 1–3: metadata reframed (GBA is the founding platform, not the boundary), `iwram-pin` off by default, second-target build witness. |
| 2026-11-06 | The **toolchain pin** (`nightly-2026-09-04`) and the **width decision** (guarded i32 as the default) recorded as decisions of record. |
| 6a-1…6a-4 | The overlay crate `crates/flac-lite-gba`; the `rice-twin` arm; the ARM kernel `rice-arm` with its FFI, `KernelAbi` and three pinned tables, witnessed on mGBA's ARM7TDMI. 6a-4 found one real ABI bug **on the device** — the 12-byte state frame's stack-argument offsets read two words too high — that every host arm stayed green about. |
| 6a-5 | The seam threaded up to the frame (`decode_frame_with<K>` → `…_with` at each layer) so a *product* can name an overlay kernel without the core depending on the overlay. Reading: `asm − ctrl` = **−584,856 c/frame (−286.9 c/sample)**, 157/157 frames faster. Fork resolved to *carry*. |
| 6a-5b…6a-5d | Three rungs to explain a +48.2 c/sample overhang against the probe, and the answer was none of the things they suspected: **the seam is priced at zero** (a same-image differential), **an instruction in the codeword loop costs exactly 4.000 cycles** (two independent removals, exact to the cycle, proportional to codewords executed), and **it costs 4 because it is being fetched from cartridge ROM** — the same 680 B of instruction stream in IWRAM runs **2.9× faster** (109.77% → 48.29% of a 32,768 Hz frame) from one section directive. The largest single result in these records, and the reason the shipped kernel body is IWRAM-resident. |
| 6a-6 / 6a-7 / 6a-7b | Guarded i32 as the core's default with an i64 escape leg; the width marginal priced same-image (+44.1 c/sample FIXED, +1,759.1 LPC); additivity confirmed to 0.06%. Then the first time a decode number in this repository was taken **on silicon**: bit-exact on hardware, and every leg **3.7–6.9% above** the model, which retired 39.74% as the hardware figure and set the +7% derating rule. |
| 6b-1 / 6b-2 | The kernel in the product ROM, then the product path through the **real packer and a real manifest**: 396,066 / 398,255 c/frame on both units, identical, PCM pin matched — the decode row of record — against the shipped default kernel's 165.6%. The packer's refusals (LPC, off-rate, 8-bit) are C2's guard and are witnessed as refusals, not just as accepted blobs. |
| 6b-3 | The first playback image at the target rate: asset → manifest → `Decoder` → convert → ring → sound DMA, with underflows *counted* and a period audit that is part of the verdict. Its host model reproduced the ROM's audit line digit for digit from the ring code and the constants alone — and retired three justifications `geometry.rs` had been making for itself. |
| 7-0 / 7-1 / 7-2 / 7-3 | The rate identity fix and its pitch witness; the first hardware stage shares on the product path, and the second reading that named the mechanism (`late in push 1/1`, four runs, both units); the chunk-sized push that closed it; and the real-audio master run that met C2. |

## The questions, and what answered them

Sixteen questions were kept in `history.md`; all are closed. Compressed to the
answer, with the file that now carries the detail:

| Q | Asked | Answer |
|---|---|---|
| Q1 | Is the ARM kernel right *through the seam*, on whose tables, with how much `unsafe`? | Yes — swept on the device over the **same** committed tables; the overlay owns its three tables (544 B, `.iwram` 812 → 1,356 B) witnessed by re-derivation and runtime addresses, **never** a hard-coded address. `unsafe` budget: 0 in a default build, 3 with `rice-arm`, 6 with `iwram-pin`. `arm-kernels.md` |
| Q2 | Can one image carry three arms? | **No.** A second `RiceKernel` body in a link moves the *control* by −36,543 / −202,529 c/frame. Arms may share an image for a paired same-image reading; the number that belongs to the record comes from the product-shape image. |
| Q3 | Why did a code-size increase *buy* 26–31 M cycles? | **Never explained.** Named, bounded and reproducible; it is the same second-instantiator/inlining effect seen from the profitable side. It survives as a warning that a measurement plan can accidentally buy a mechanism win. |
| Q4 | The exact width guard — per frame or per subframe? | Per **subframe**, `Σ|c_j|·2^(b−1) ≤ i32::MAX`, computed from data the header already parsed; the frame reports the OR of its halves. `architecture.md`, the width decision. |
| Q5 | Does E9's additivity reproduce? Would a Thumb-1-native portable kernel win? | Additivity reproduces to **0.06% / 0.005%**. The second half was never built and is not planned. |
| Q6 | Can the pin move forward without losing the thread? | The discipline is the answer: the instance is a string, a version string does not pin an instrument, and moving the pin is a codegen change that carries a control build. The era gap (+66,403,116 cycles, i64 LPC legs only) cannot be attributed because that era's binary is gone. |
| Q7 | What does the walk cost, and what replaces the frame-number rule with no table? | +3,120 B of `.text` — and the walk's charge was later measured as **zero cycles** in the product image. A frame ends itself; the end of the bytes is the end of the stream. |
| Q8 | Is the pack-time guard load-bearing? | It is, and it had to be built to convict **at pack time**: the profile guard reads the frames, not the header. `-l 0` ≠ FIXED-0 was the concrete catch. |
| Q9 | Which gates have never been run anywhere, and which assert nothing? | Every `podman-*` target (never run in this harness); `test-rom` collects 0 tests; `flac-portable-check` is compile-only; `rate-test` needed gitignored assets. All restated in `measurements.md` and `../FLAC.md` rather than left implied. |
| Q10 | Does the committed corpus witness what the new code needs? | **Blind in three measured ways** (0 escapes, no `u32::MAX` fold, no guard-overflowing codeword) and blind to a kernel swap until the device sweep shared its tables. The census is now a tripwire in both consumers. `measurements.md` |
| Q11 | Is anything in these docs still a phantom? | A 28-row table of assumptions that measurement killed lives in `format-notes.md`; the general lesson filed with it is that a "measured" claim must state what the command *varied*. |
| Q12 | Where do the seam's 48 c/sample go? | **It was placement, not the seam.** 6a-5b/5c/5d, above. |
| Q13 | What is the 3.7–6.9% mGBA does not charge? | Parked by decision (C2 is judged on hardware), with a +7% derating standing in for a probe; re-opened as Q15. |
| Q14 | Is the 2.4% pitch shift part of the product? | **Ruled: no** — see "The pitch ruling" above. |
| Q15 | Is the emulator/hardware gap a property of how these images reach a unit? | **Not for the late service** (attributed to the producer's own masked push), and **still open as a mechanism** for the convert leg's +35%. It is a stated limit of every mGBA figure, not a task. `measurements.md` |
| Q16 | What makes a rate "the highest achievable"? | The owner chose 16,384 Hz and accepted the real-audio path after the four-run reading, so the search was closed rather than given a bar. If a future session wants a *ceiling* rather than a rung, the bar has to state a worst-frame headroom floor, what shapes the rate must carry (FIXED-0 vs predicted), and what the headroom is for — and note that LPC at 16,384 Hz projects to 86.6%. |

## Closed, do not re-litigate

- **E3** inert (+20 c/frame), **E6** code pinning slower *for Rust paths*, **E7** the flag
  inert, **E2a** not a collapse on its own.
- **The portable twin as the default: declined**, at the measured +42.6 c/sample. It stays a
  feature and the differential's second mechanism.
- **`opt-level = "z"` in `flac-lite`** does not govern the measured ROM (the workspace-root
  profile applies).
- **`dyn` dispatch anywhere**: monomorphization is the seam's contract.
- **24-bit decode**, `Decoder`-side random access on the product path, and crates.io
  publication: out of scope by decision, not by omission.
- **E1 in any stack**: excluded by design, because its relief is the position-only reader's
  re-fetch premium that the kernel's windowed refill already absorbed — stacking it would
  double-count.
- **Rates that are not exactly expressible** on the 2²⁴ timer chain (this replaced "rates
  other than the target" as the exclusion when the rate became a result).

## Where an old citation resolves

`FLAC.md` was a single lab notebook of ~9,000 lines before it was split, and code comments
and older docs still cite its section names. They resolve here, and then into the files above.

| Old `FLAC.md` section | Now |
|---|---|
| The Constraint; Rust FLAC Landscape; Candidate Evaluated; Upstream Status; Decision (c) | [`architecture.md`](architecture.md) §"Why there is a decoder of our own", §"Alternatives" |
| Perf gate spike — concrete plan; Budget; Validation process; correctness-before-speed | [`measurements.md`](measurements.md) §"The budget", §"Rules every run follows" |
| Frame header: measured byte layout; Corrections 1–3; frame sync scanning; encoder choices; `--force-utf8-legacy-noop`; step 3a–3f | [`format-notes.md`](format-notes.md) |
| Golden vectors | [`measurements.md`](measurements.md) §"Golden vectors" |
| Prior assumptions that did not survive measurement | [`format-notes.md`](format-notes.md), last section |
| Perf follow-ups — experiment menu; E1–E10 entries and results; the ordering log | [`history.md`](history.md) §"The E-series", §"Closed, do not re-litigate"; verdicts in [`measurements.md`](measurements.md) |
| E10 algorithm plan; groundwork; unit-test kernel track; PR A; PR B | [`arm-kernels.md`](arm-kernels.md) |
| Sample-rate reduction proposal; rate ROM; rate-play ROM; playback attribution; feeder-off | [`history.md`](history.md) §"The ladder"; the numbers in [`measurements.md`](measurements.md) at `87f1b86` and earlier |
| split proposal; the two-crate split; split-ladder item 5 / 6 / 6a / 6b; the width decision; the toolchain pin | [`architecture.md`](architecture.md), [`history.md`](history.md) |
| Baselines to hold; Per-PR rules; the landing record; ordering rules | [`history.md`](history.md) §"How a change to this track lands", §"The ladder"; counts in [`measurements.md`](measurements.md) |
| Recording Q&A for a future session; Q1…Q16 | [`history.md`](history.md) §"The questions" |
| Plan of record; item 7 and its rungs 7-0…7-3; the rate ruling; "The rung is chosen"; C1/C2 and C2 closure | [`history.md`](history.md) |

### Retired Make entrypoints (2026-11-14)

The historical instrument arms that these pages cite no longer have Make targets; their
sources are unchanged and buildable by hand. `native-<target>` and `podman-<target>` are
still valid for everything that remains — they are now pattern rules over the bare target
names rather than one alias per target.

| Retired | Was | Reproduce it with |
|---|---|---|
| `item6a5-arms-rom`, `item6a5-asm-rom`, `item6a5b-rom`, `item6a7-rom`, `item6a5-test`, `item6a5b-test`, `item6a7-test` | the 6a-3/6a-5/6a-5b/6a-7 measurement arms | `cargo build --release --target thumbv4t-none-eabi --features <arm>` in `examples/flac_spike` |
| `rate-assets`, `rate-test`, `rate-rom`, `rate-play-rom` (+ `native-` twins) | the 10,512 Hz rate flow | `bash scripts/gen_rate_assets.sh IN RATE`, then the `rate-assets` feature build |
| `kernel-rom`, `kernel-test` | E10's groundwork crate | `cargo` in `examples/arm-kernels` |
| `e5-fetch-rom` | the fetch-ratio arbitration | `cargo` in `examples/e5_fetch` |
| `flac-rom-default-kernel` | the 6b-1 kernel A/B control | `cargo build --no-default-features` in `examples/flac_integration` |
| `overlay-test-twin`, `podman-overlay-*` | the overlay's `rice-twin` arm and the containerized overlay gates | `cargo test --features rice-twin` from a gate-neutral cwd; `make podman-overlay-gates` |

## How a change to this track lands

The ladder above was executed under a per-PR discipline that still applies to any
measurement-bearing change here. Four paragraphs, every PR:

1. A **pre-change control build** from `HEAD` before any source edit, with a clean-worktree
   determinism control beside it.
2. A **section table** plus `nm --print-size --size-sort`, every delta attributed **symbol by
   symbol**, and the `nm` anchors re-stated or their absence explained. `.rodata` content is
   not a codegen signal (a comment edit above a panicking call site moves one byte of the
   panic-location table and nothing else); `.text` plus the anchors are the code invariant.
3. **Gate counts**, naming a new configuration as a *new gate* rather than silently
   broadening an existing one, and stating `make test-rom` as run or not-run and why.
4. A **mutation witness** for every new correctness suite — including the honest green rows,
   so that a behaviour-neutral re-routing cannot read as a gate failure.

Two ordering rules that outlive the ladder: **cumulative gains** (no lever substitutes for
another — the rate track grows the denominator while decoder levers shrink the numerator,
and utilisation composes as measured absolute additivity), and **criteria before
attribution** (a measurement that does not decide something waits behind the work that does
— which is why the data-read probe was parked and why the sub-stage probe never ran).
