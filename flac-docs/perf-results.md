# Decoder performance experiments — the E-series results

> **Scope: the FLAC track's experiment narratives** (the E-series and its predecessors),
> measured on `crates/flac-lite` / `crates/flac-lite-gba` code. See
> [`measurements.md`](measurements.md) for the ledger those experiments feed and
> [`../FLAC.md`](../FLAC.md) for the track's entry point.

The records below are the measurements themselves, kept close to the form they
were written in (each names its own image, control, calibration and gates). What
was removed is the planning that preceded them: the experiment menu, its status
log, and the "which experiment runs next" ordering — now a one-line table in
[`measurements.md`](measurements.md) and the ordering rules in
[`plan-of-record.md`](plan-of-record.md).

Read the era rules first: a percentage belongs to its image, and only absolute
paired marginals travel across records ([`measurements.md`](measurements.md)).

**Reproduction.** Each experiment's reappliable sample lives under
`examples/flac_spike/experiments/<name>/` (README + patch against the tree it was
measured on). Probes never land on the spike's live sources; `src/` stays the
instrument. Re-applying an E archive now also needs the
`port-item6a2-header-bits.patch` beside it (item 6a-2).

| Order | Record | One-line verdict |
|---|---|---|
| 1 | E1 — ROM→RAM staging | no collapse; cartridge reads are not the story |
| 2 | E4 — i32 intermediates | LPC −46.9%, FIXED −3.7% |
| 3 | E2 — accumulator reader | E2a: no collapse (FIXED −3.14%, LPC **+1.41%**) |
| 4 | E7 — frame-pointer flag | inert; the target forces frame pointers |
| 5 | E6 — IWRAM code pinning | slower on both arms; instruction fetch ruled out |
| 6 | E8 — sub-stage attribution | FIXED's cost is the Rice per-sample loop; LPC's is the i64 integrator |
| 7 | E2b — run-length table count | FIXED −19.17% marginal (−21.71% vs live control) |
| 8 | E10 — ARM hybrid codeword loop | arm − tab −40.08% FIXED / −11.51% LPC |
| 9 | E9 — composite arms | additive; arm+i32 FIXED fits 32,768 Hz mean and worst |

Closed without a result record: **E3** (IWRAM decode scratch) measured inert
inside E9 (+20 cycles/frame); **E5** (emulator vs hardware arbitration) has never
been run.

---

## E1 result — ROM→RAM staging: cartridge reads are not the story (2026-09-22)

**Sample, not integrated.** The probe never lands on the live spike sources:
its code is preserved as a reappliable sample under
`examples/flac_spike/experiments/e1-rom-ram-staging/` (README +
`e1-rom-ram-staging.patch` — the diff against main @ `756220b`, verified
`git apply --check` clean there; re-apply instructions and the measured
values are in that README). The plan of record — PR 4 cadence, PR 5 verdict
— is untouched. ROM
`flac-spike.gba` sha256 `6a9cd9f184cb347440f5a951bf82aed83243fe8c2b498f0dc7f5fcaa56e19d8c`;
mGBA runs 2026-09-22 on the same host runner as PR 3 (same mGBA build; the
agb 0.25 `mgba-test-runner`), two headless runs byte-identical (`cmp`, 691
lines). Gates green on the patched tree: `make spike-test`, `make
flac-test`, `make check`.

**Harness shape (three IRQ-off windows per frame, one image, one instrument):**

1. **ctrl** — PR 3's `driver::decode_one` from the embedded ROM region,
   verbatim, kept as the *same-run control* (the comparison needs the old
   number from the same image, not a recalled log);
2. **copy** — the staging memcpy itself (frame slice offset N..N+1, tail to
   region end) into a per-frame EWRAM scratch — timed separately, per the
   E1 method (the copy *is* the ROM-read traffic);
3. **staged** — the same decode over the staged copy through the
   `driver::ClipView` seam the sample patch introduces (borrowed decode
   inputs, any lifetime —
   `SpikeClip::region` is `&'static` so a RAM copy cannot be handed to it;
   `decode_one` now delegates through it, one decode path). A one-entry
   seek table at offset 0 keeps the table-vs-header blocksize cross-check
   witnessing the copy for drift, and a **per-frame staged-vs-ctrl sample
   equality witness** (ctrl output parked, staged compared after the
   windows; first offender named, numbers void on divergence) passed 157
   frames × 2 arms — the staged numbers come from a proven-correct decode.

Calibration witness: overhead 22, stable, nets 0 — identical to PR 3's.
One harness finding en route: on *this* layout the pass's first calibration
window measured 2 cycles below steady state (raws 31/33/33/33/33 — stable
but cold), so 3 discarded warm-up windows now precede the reported 5.
Micro-windows are code-layout sensitive (±4 cycles, prefetch-scale); noted
because it also explains why ctrl stats here differ from PR 3's by a
constant +104 cycles/frame (+0.005%) — which doubles as the control
reproducing the baseline: worst-frame **indices** match exactly (frame 60
FIXED / 136 LPC / 156 min both arms).

**Results (net of this variant's own calibration; 157 frames, 320,000
samples; budget 1,048,750/frame ≈ 512 c/sample):**

| arm | fold | sum | mean | mean %budget | max @frame | min @frame |
|---|---|---|---|---|---|---|
| FIXED ctrl | ROM decode | 361,111,383 | 2,300,072 | 219.3% | 2,314,577 @60 | 585,405 @156 |
| FIXED copy | staging memcpy | 7,956,425 | 50,678 | — | 77,701 @136 | 3,911 @156 |
| **FIXED staged** | **RAM decode** | **344,071,398** | **2,191,537** | **209.0%** | **2,205,189 @60** | **557,770 @156** |
| LPC ctrl | ROM decode | 1,077,191,358 | 6,861,091 | 654.2% | 6,901,488 @136 | 1,506,785 @156 |
| LPC copy | staging memcpy | 7,551,209 | 48,096 | — | 75,083 @105 | 9,666 @156 |
| **LPC staged** | **RAM decode** | **1,058,039,914** | **6,739,107** | **642.6%** | **6,778,205 @136** | **1,475,566 @156** |

- **FIXED:** staging saves 17,039,985 cycles (108,535/frame, **4.72%**);
  paying for the copy too, net −2.51%. Staged still 1,075 c/sample.
- **LPC:** staging saves 19,151,444 (121,984/frame, **1.78%**); net of the
  copy −1.08%. Staged still 3,306 c/sample.
- Worst frames stay **OVER BUDGET in every arm** (staged worst: 210.3% /
  646.3%). Screen verdict RED = measurement completed, budget missed —
  harness completed 2/2, decode proof matched the PCM pin
  (`0x54C7B356621B6E15`, cross-arm agree) before any timing.

**Reading (the E1 rule, executed):** staged decode did **not** collapse →
**cartridge-read timing is not the story.** The bound is arithmetic: a
frame's whole input is ~5.5 KB, and the measured memcpy — a *bulk* path
with better per-byte timing than the bit reader's per-bit access — costs
~50,700 cycles/frame (≈9.2 c/B FIXED, ≈9.0 c/B LPC, consistent across
arms); even generously counting ALL of decode's ROM traffic at that rate
bounds the wait-state component well under 10% of the 2.3M-frame. The
remaining ≥90% is inside the decoder loop. Two hypotheses the result
promotes: **E4 (i64 intermediates — ~7–20× above a plausible compute floor
for FIXED-0 arithmetic, region-independent)** and **E3 (EWRAM scratch
wait-states — a different region with far more traffic than the input
bytes)**. E1 also kills *staging* as a production idea: shipping decode
always reads ROM, and per this measurement staging recovers ~1–3% net at
best. One observation, hypothesis-grade: the staged saving (108k/frame)
exceeds the copy's own cost (51k), i.e. the bit reader's per-access ROM
pattern pays a larger wait-state premium per byte than a bulk copy does —
relevant if E2 is revisited, but immaterial against the 2.3M total.

**Method compliance (the menu's rules):** one variable per window set
(memory region of the decode *input* only — decode logic, assets, and
decode-proof gate untouched; `make spike-test` green ⇒ same *what*, only
*how* changed); new ROM sha recorded; two headless runs `cmp`-identical;
full stats (min/max/sum/worst-index) per fold, net of this variant's own
calibration; raw reported alongside net on every per-frame line. This is a
tinkering result, not the gate verdict — PR 5 still owns the verdict table
and the hardware arbitration (E5): if a future on-cart run shows mGBA
diverges from hardware on gamepak timing, the *absolute* numbers here shift
but the ctrl-vs-staged *relative* result was measured within one model.

**Preservation:** everything the probe added — the `ClipView` /
`decode_view_one` seam, the three-window pass, the staging scratch, the
staged-vs-ctrl witness, and the calibration warm-up — lives in the sample
patch, nothing in the live tree. Re-applying against a harness that has
since moved (PR 4/5) should take the three-window *method* from the patch
rather than forcing hunks; the method is the finding's carrier, the line
numbers are not. The `perf/flac-spike-e1-ram-staging` branch that ran the
measurement is superseded by the sample (delete after this docs PR merges).

## E4 result — i32 intermediates halve the LPC arm; FIXED barely moves (2026-09-23)

**Sample, not integrated.** As with E1, the probe never lands on the live
spike sources: its code is preserved as a reappliable sample under
`examples/flac_spike/experiments/e4-i32-intermediates/` (README +
`e4-i32-intermediates.patch` — the diff against main @ `73566b8`, verified
`git apply --check` clean there). The plan of record — PR 4 cadence, PR 5
verdict — is untouched, and no decoder file in `crates/flac-lite/` changed:
the variant is a **vendored copy of the hot path** in the spike crate
(`e4.rs`), so the measurement compares two decode paths on one image. ROM
`flac-spike.gba` sha256
`59be708d53f9a7b1943cadbb677368beaceab6c29ddc472254a75f87294403f8`;
mGBA runs 2026-09-23 on the same host runner as PR 3 / E1, two headless
runs byte-identical (`cmp`, 1005 lines). Gates green on the patched tree:
`make spike-test` (including the new `tests/e4_witness.rs`), `make
flac-test`, `make check`.

**Harness shape (two IRQ-off windows per frame, one image, one instrument):**

1. **ctrl** — PR 3's `driver::decode_one` (verbatim i64 path), the same-run
   control;
2. **variant** — `e4::decode_one_i32`: vendored
   `decode_frame`/`decode_subframe`/integrators/`decorrelate` with `i32`
   intermediates (`wrapping_*` + arithmetic `>>` — one `asr` instead of the
   `__aeabi` helper calls a *variable* i64 shift compiles to). Shared with
   the control: field readers, `PredictorState::fill`, `decode_residual` —
   the variable is the hot-path arithmetic width, only.

Correctness before timing, both layers: on the host the variant decodes the
exact embedded clips bit-exactly vs `flac -d` (`e4_witness.rs`); on the ROM,
a per-frame variant-vs-control **sample-equality** witness (the E1 pattern)
passed 314/314 frame-windows. Decode proof matched the PCM pin
(`0x54C7B356621B6E15`, cross-arm agree) before any timing.

**Calibration finding (harness science, applies to every future micro-window
run):** this layout broke PR 3's calibration twice over. First, E1's
cold-first-window property returned (raws 22/24/24/24/24) — E1's separate
warm-up windows fixed that; then the e4 pass showed a *serial write
immediately before a window shifts that window ~2 cycles* — so warm-up
windows logged between reported windows re-poisoned them. Verified fix (in
the patch, both passes): **ONE byte-identical loop over 3 discarded + 5
reported windows, unconditional stores, nothing logged between windows**;
every reported window is then entered with exactly the preceding code the
discarded ones ran. Overhead on this image: 24 (PR 3's was 22), stable,
nets 0. Cross-session consequence repeats E1's: the same image's PR
3-shape pass (same patch, calibration only) read FIXED sum=357,947,179
max=2,294,325@60 / LPC sum=1,074,036,562 max=6,881,296@136 — the e4 ctrl
arm sits a constant +11 cyc/frame above it (code layout), which is exactly
why ctrl-vs-variant must come from one image.

**Results (net of this variant's own calibration; 157 frames, 320,000
samples; budget 1,048,750/frame):**

| arm | fold | sum | mean | mean %budget | max @frame | min @frame |
|---|---|---|---|---|---|---|
| FIXED ctrl | i64 decode | 357,948,906 | 2,279,929 | 217.4% | 2,294,336 @60 | 580,524 @156 |
| **FIXED variant** | **i32 decode** | **344,549,255** | **2,194,581** | **209.3%** | **2,208,577 @60** | **559,277 @156** |
| LPC ctrl | i64 decode | 1,074,038,289 | 6,841,008 | 652.3% | 6,881,307 @136 | 1,501,952 @156 |
| **LPC variant** | **i32 decode** | **570,270,032** | **3,632,293** | **346.3%** | **3,656,120 @136** | **862,867 @156** |

- **LPC:** i32 intermediates save 503,768,257 cycles (3,208,715/frame,
  **46.90%**). The arm halves: 3,340 → 1,774 cycles/sample. Worst-frame
  index unchanged (136); min moves −42.6%.
- **FIXED:** saves 13,399,651 (85,348/frame, **3.74%**). Worst stays 210.6%
  of budget.

**Reading (the menu's own rule, executed):** E4 was predicted "region-
independent, expect the LPC arm to move most" — confirmed, and then some:
**half of the LPC arm's cost was software 64-bit arithmetic.** The asymmetry
is the diagnostic: the FIXED arm's integrator is FIXED-0, which
`integrate_fixed` early-returns (`the residual IS the signal`) — there is
**no i64 math in its hot loop at all**, so a width change is inert there by
construction; its 3.7% is the non-integrator sites (`pad_block` — its
wasted=1 frames — and side decorrelation) plus layout. What the FIXED arm's
remaining 2.19M/frame is NOT, after E1 + E4: cartridge reads (E1, ruled
out), 64-bit integrator math (E4, ruled out by inertness). What remains
standing in its hot loop: the position-only **bit reader** (per-bit ROM
re-fetches — E1's note that its per-access pattern pays an above-bulk
premium) and generic decode plumbing. **E4's result promotes E2 (bit-reader
refill accumulator) as the next FIXED-side lever**; for LPC, E2 + whatever
else sits under the halved 1.77 c/sample. E3's bounded ceiling (sub-1% —
EWRAM is one extra cycle per 32-bit access) is unchanged by this result.

**The i32 proof (the menu's "prove it before trusting it"), two-tier:**

- **Ring sites — unconditional equivalence.** `+`, `−`, `×`, `<<` commute
  with i32 truncation (π(x⊕y) = π(π(x)⊕π(y))), and the i64 path's existing
  convention is truncate-at-store — so wrapping-i32 with the same store is
  bit-identical for ANY input, legal or corrupt: `integrate_fixed` cascade
  + seeds, `pad_block`, the side decorrelation arms.
- **Arithmetic-shift sites — conditional range bound.** `>>` does not
  commute; two sites right-shift an intermediate (LPC `(acc >> shift)`,
  mid/side `(m_ext ± side) >> 1`), exact whenever the true value fits i32.
  Profile bound: |acc| ≤ order × |coeff| × |sample| ≤ 4 × 2^(prec−1) ×
  2^16 — safe for precision ≤ 13 (2^30); precision 15 would NOT be (2^32),
  so production adoption needs a **pack-time LPC-precision guard** (the
  encode profile is ours). Measured on the exact embedded regions (field
  walk through the public readers): precision ≤ 11, shift ∈ [0, 11],
  |coeff| ≤ 751, final |sample| ≤ 1115 ⇒ bound 3.35e6 ≪ 2^31. Mid/side:
  |m_ext ± side| ≤ 3×2^16 for legal 8/16-bit streams — fits with margin.
  The witnesses: host bit-exact (`e4_witness.rs`) + ROM per-frame equality
  + libFLAC's own bit-exactness of the clips pin the outcome; the proof
  pins *why* they cannot diverge.

**Method compliance (the menu's rules):** one variable (hot-path arithmetic
width; shared field readers/fill/residual keep the comparison honest);
`make spike-test` green including the variant's own bit-exact witness ⇒
same *what*, only *how*; new ROM sha recorded; two headless runs
`cmp`-identical; full stats per fold net of this variant's own calibration;
raw alongside net per frame; equality witness gates every variant number.
Still tinkering evidence, not the gate verdict — PR 5 owns the verdict
table and E5's hardware arbitration; the ctrl-vs-variant *relative* result
is measured within one emulator model.

**Preservation:** the vendored variant, the two-window pass, the equality
witness, the host witness test, and the unified calibration loop all live
in the sample patch; nothing in the live tree. The `perf/flac-spike-e4-i32-
intermediates` branch that ran the measurement is superseded by the sample
(delete after this docs PR merges). If the harness has moved when a future
run re-applies (PR 4/5), take the two-window + equality-witness +
unified-calibration *method* from the patch; `e4.rs` is a standalone module
and should port verbatim.

## E2 result — the accumulator reader does not collapse the FIXED arm (2026-09-24)

**Correction first (the menu and `bits.rs`'s module doc were wrong about
the target):** the E2 menu item and `crates/flac-lite/src/bits.rs` both
claim "ARMv4T has CLZ". **ARMv4T (ARM7TDMI) has no CLZ — CLZ arrived with
ARMv5T.** The GBA's CPU can count a unary run with shifts/compares (or a
table), but there is no single-instruction leading-zero count to fold the
Rice quotient loop into; any E2b must count without CLZ. This entry records
the correction; the live crate doc is deliberately *not* edited here (a
docs-only probe PR leaves the live tree untouched — the fix belongs with
whichever change next touches `bits.rs`).

**Sample, not integrated.** As with E1/E4, the probe never lands on the
live spike sources: its code is preserved as a reappliable sample under
`examples/flac_spike/experiments/e2a-bit-reader-accumulator/` (README +
`e2a-bit-reader-accumulator.patch` — the diff against main @ `743a913`,
verified `git apply --check` clean there). The plan of record — PR 4
cadence, PR 5 verdict — is untouched, and no decoder file in
`crates/flac-lite/` changed. ROM `flac-spike.gba` sha256
`fc807f02d6fb4829e1b23ad94fa3a016ef7d0da823be002b0fef0aac3dd338c4`;
mGBA runs 2026-09-24 on the same host runner as PR 3 / E1 / E4 (agb 0.25
`mgba-test-runner`, Rust nightly 1.100.0-nightly a69a63265 2026-09-03),
two headless runs byte-identical (`cmp`, 1005 lines). Gates green on the
patched tree: `make spike-test` (host tests 23 → 27: lib 15 → 18 + the new
`tests/e2_witness.rs`, `spike_witness` 8 unchanged), `make flac-test` (115
passed, 1 ignored — unchanged, `crates/flac-lite/` untouched), `make
check`, `make native-spike-rom`.

**Harness shape (two IRQ-off windows per frame, one image, one
instrument — the E4 patch's shape, applied to a different variable):**

1. **ctrl** — PR 3's `driver::decode_one` (verbatim i64 path on the
   position-only reader), the same-run control;
2. **variant** — `e2::decode_one_accumulator`: the vendored hot path with
   the reader swapped to `AccumulatorReader` — `data + pos (bytes consumed)
   + acc (u32, left-aligned MSB-first, next bit = bit 31) + valid`,
   invariant *logical cursor = pos·8 − valid* (`bit_position()` /
   `bits_remaining()` derive exactly). Fast path
   (`1 ≤ n ≤ 31, valid ≥ n`): `val = acc >> (32−n); acc <<= n; valid −= n`.
   Cold path (`valid < n`, **every n == 32 read** — 32-bit reads are never
   forced through the accumulator — and every illegal width, so rejection
   rules stay identical): the retained position-based `peek_at` recompute
   at the derived cursor, then an exact resync of `(pos, acc, valid)`.
   Refill byte-at-a-time only (ldrb-class, no word loads / alignment
   assumptions) while `valid ≤ 24`: `acc |= byte << (24 − valid); valid += 8`
   — cursor-neutral, so failed reads are atomic with no rollback.
   `byte_align`: `drop = valid & 7; acc <<= drop; valid &= !7` — through
   the invariant, `bit_pos ≡ −valid (mod 8)`, so this is exactly the live
   `(8 − (bit_pos & 7)) & 7` rule (witnessed, see below). All arithmetic
   **i64 exactly as live** (E4's i32 was never merged — not this variable),
   and the unary loop stays literally `while read_bits(1)? == 0 { q += 1 }`
   (counting the quotient another way = E2b, not E2a).

Correctness before timing, both layers: on the host the variant decodes the
exact embedded clips bit-exactly vs `flac -d` (`e2_witness.rs`), plus
reader-level differential sweeps against the live `BitReader` (mixed-width
incl. n=32/illegal widths/EOF, `byte_align` at every alignment, composite
coded-number/wasted atomicity); on the ROM, a per-frame variant-vs-control
**sample-equality** witness (the E1/E4 pattern) passed **314/314**
frame-windows — including `byte_align` landing identically on every real
frame footer. Decode proof matched the PCM pin (`0x54C7B356621B6E15`,
cross-arm agree) before any timing. Calibration (unified loop in both
passes, E4's verified shape): overhead **24** on this image, stable, nets
0 — all three passes.

**Results (net of this variant's own calibration; 157 frames, 320,000
samples; budget 1,048,750/frame):**

| arm | fold | sum | mean | mean %budget | max @frame | min @frame |
|---|---|---|---|---|---|---|
| FIXED ctrl | position reader | 357,908,871 | 2,279,674 | 217.4% | 2,294,081 @60 | 580,269 @156 |
| **FIXED variant** | **accumulator reader** | **346,677,917** | **2,208,139** | **210.5%** | **2,222,598 @4** | **560,665 @156** |
| LPC ctrl | position reader | 1,073,988,846 | 6,840,693 | 652.3% | 6,880,992 @136 | 1,501,649 @156 |
| **LPC variant** | **accumulator reader** | **1,089,162,991** | **6,937,344** | **661.5%** | **6,979,771 @145** | **1,524,325 @156** |

- **FIXED:** accumulator saves 11,230,954 cycles (71,535/frame,
  **3.14%**). Mean 1,113 → **1,078 c/sample**; worst stays 211.9% of
  budget. Worst-frame index moves @60 → @4 — the top frames sit within
  ~0.3% of each other and micro-layout shuffles their order; the mean is
  the reading.
- **LPC:** accumulator **costs** 15,174,145 cycles (96,651/frame,
  **+1.41% slower**, deterministic across both runs). The vendored LPC
  reads (warm-up `read_signed`, coefficients, escaped partitions) are
  multi-bit — they were already cold-path-shaped — and the vendored
  header/subframe plumbing costs slightly more than the shared functions
  it replaces; the reader swap is not a free substitution there.
- Worst frames stay **OVER BUDGET in every arm**. Screen verdict RED =
  measurement completed, budget missed — harness completed 2/2.
- **Control reproduces the recorded baseline shape:** worst-frame indices
  exact (FIXED @60 / LPC @136 / min @156 both arms); this image's PR
  3-shape pass read FIXED sum=357,906,830 max=2,294,068@60 and LPC
  sum=1,073,986,805 max=6,880,979@136, the e2 ctrl arm a constant +13
  cyc/frame above it (code layout) — the cross-session witness, same
  pattern as E1 (+104) and E4 (+11).

**Reading (the E2 menu rule, executed): the accumulator did NOT collapse
the FIXED arm.** −3.14% is the same order as E4's FIXED movement (3.7%) —
layout-adjacent noise around a real but small saving — against the
"collapse" the menu predicted. So the *implementation* of the reader was
never the FIXED arm's cost center; the reader's per-access ROM premium E1
observed is intrinsic to fetching many small fields bit-by-bit, not to how
`bit_pos` is turned into a value. Adding to the ruled-out list (E1 cartridge
reads, E4 i64 integrator math): a position-vs-accumulator reader swap.
What is left standing in the FIXED hot loop after E1+E4+E2a: the Rice
**unary loop as a shape** — per-quotient-bit iterations, ~2 reads/sample
plus the quotient run, each an independent function-call + branch — and
generic decode plumbing (call layers, bounds checks). E2b (count the
quotient in fewer steps) remains the only reader-side lever with
headroom, but it must count **without CLZ** (see the correction above):
byte-at-a-time masked-ones counting or a run-length table, not `clz`.
Neither arm crosses 512 c/sample on E2a; PR 5's verdict table and E5's
hardware arbitration are unchanged.

**Method compliance (the menu's rules):** one variable (reader
implementation — i64 arithmetic verbatim, unary loop verbatim,
`decorrelate` a real shared call, integrators/pad verbatim copies of the
live functions); `make spike-test` green including the variant's own
bit-exact witness ⇒ same *what*, only *how*; new ROM sha recorded; two
headless runs `cmp`-identical; full stats per fold net of this variant's
own calibration; raw alongside net per frame; equality witness gates
every variant number. Still tinkering evidence, not the gate verdict.

**Preservation:** the vendored variant module (`e2.rs` — reader + vendored
header/subframe/residual, since the live signatures take
`&mut BitReader`), the two-window pass, the equality witness, the host
witness test, and the unified calibration loop (both passes) all live in
the sample patch; nothing in the live tree. The
`perf/flac-spike-e2a-accumulator` branch that ran the measurement is
superseded by the sample (delete after this docs PR merges). If the
harness has moved when a future run re-applies (PR 4/5), take the
two-window + equality-witness + unified-calibration *method* from the
patch; `e2.rs` is a standalone module and should port verbatim.

## E7 result — the flag is inert: thumbv4t-none-eabi forces frame pointers (2026-09-25)

**Correction first (the menu entry's premise was half wrong):** E7 claimed
`-Cforce-frame-pointers=yes` "applies to every release ROM measured so
far" — true — and that removing it "costs a live register in exactly the
small hot loops" recoverable by a flag flip — **false, and not
recoverably so**: the `thumbv4t-none-eabi` **target spec itself** sets
`"frame-pointer": "always"`. The flag line in `.cargo/config.toml` has
always been redundant, and the frame-pointer cost is not a
rustflag-addressable variable on this target.

**Method (two builds of identical source at `6a6beb4`, one variable — the
rustflag line; no probe code, so no measurement patch of decode paths):**

1. **Control** — committed source, committed rustflags. `make
   native-spike-rom` → ROM sha256
   `0d97659165b2900600305ee1d7438f90c9f3e05d479068431ef7b6cc6bfeedb3`,
   *bit-identical to PR 3's recorded image* (rebuild-hashes-stable
   property confirmed cross-session); mgba runs twice, `cmp`-identical
   (675 lines), decode proof `0x54C7B356621B6E15` MATCH both arms, stats
   reproduce PR 3 exactly (FIXED sum=361,095,055 max=2,314,473@60; LPC
   sum=1,077,175,030 max=6,901,384@136) — the same-run control witness
   for every future probe.
2. **Variant** — the one-line removal of `-Cforce-frame-pointers=yes`
   (sample patch), full clean rebuild of `target/thumbv4t-none-eabi`.
   `-c` verbose build confirmed the flag absent from every rustc
   invocation (26 crates, 0 occurrences). **Result: identical ROM sha —
   `0d97659165…`** — so the flag-flip variant image does not exist. mgba
   run of the variant-built ROM is `cmp`-identical to the control's
   serial log, as it must be for a byte-identical image.
3. **Mechanism probe (outside the repo tree):** `rustc --target
   thumbv4t-none-eabi --print target-spec-json -Z unstable-options`
   (rustc 1.100.0-nightly a69a63265 2026-09-03, the toolchain that builds
   the measured ROMs) reports `"frame-pointer": "always"` in the target
   spec. Third probe to pin the override direction: a minimal `no_std`
   crate built on the same target with `=yes` vs `=no` — **asm
   byte-identical**, both emitting `push {r4, r5, r7, lr}` + `.setfp r7`
   / `add r7, sp` prologues. The target default wins in both directions;
   only a custom target JSON (unstable, out of scope) would reclaim r7.

**Reading (the menu's own rule, executed):** E7 was "the cheapest probe on
the menu" — it closed cheaper, without a measurement, because **there is
nothing to compare**: control and variant are the same bytes. The register
cost E7 hypothesized is real (visible in every prologue) but fixed by the
target, not by our flags — so it joins the ruled-out list as *not a
variable*, and the frame-pointer line in `.cargo/config.toml` may stay or
go without any perf consequence. It also corrects the crash-diagnostic
framing from the 2026-09-25 discussion: agb's r7 backtrace chain is held
by the target's `frame-pointer: always`, **not** by our rustflag —
removing the flag line cannot degrade panic traces, and PR 5 keeps the QR
crash report either way. No trade-off exists on this lever.

**Consequences:** the ordering note is amended (E6 leads); build-flag
levers on the measured image are exhausted for frame pointers — if the
prologue/register cost ever needs attacking, the lever is a custom target
spec, which is not menu-grade (it changes the toolchain contract of every
build). E6 (code placement) and E8 (attribution) are unaffected and remain
the live levers.

**Preservation:** probe code is one config line, kept as a reappliable
sample under `examples/flac_spike/experiments/e7-frame-pointer-flag/`
(README + `e7-frame-pointer-flag.patch` — the removal against main @
`6a6beb4`, verified `git apply --check` clean there; the README records
the build/run recipe, the sha a faithful re-run must reproduce, and the
target-spec command). Live tree untouched beyond docs + the sample dir.
The `perf/flac-spike-e7-frame-pointers` branch carried no commits (the
measurement varied only the uncommitted config line) and was deleted on
the spot — nothing superseded, the patch carries the variant.

## E6 result — IWRAM code pinning is slower: instruction fetch is ruled out (2026-09-26)

**Correction first (the menu named a section name that cannot work):** E6
claimed `#[link_section = ".text_iwram"]` "lands in machinery that already
runs." The machinery does run — that section name never reaches it.
gba.ld's `.text : { *(.text .text*) } > rom` pattern matches
`.text_iwram`, and GNU ld assigns an input section to the **first**
matching output section, so `.text_iwram` is swallowed into ROM before
the `.iwram` output section's own `*(.text_iwram .text_iwram.*)` entry can
claim it. Measured on the built ELF: with the attribute set to
`.text_iwram`, the symbols stayed at `0800xxxx` and `.iwram` stayed at its
baseline size. Plain `.iwram` — agb's own IWRAM-resident-code section,
where its `__aeabi`/`__agbabi` mem* helpers live — is the working pin
target: `> iwram AT>rom`, copied ROM→IWRAM at boot by `CommonInit`'s BIOS
`CPUSet`, and it lands.

**Sample, not integrated.** As with E1/E2a/E4, the probe never lands on
the live spike sources: its code is preserved as a reappliable sample
under `examples/flac_spike/experiments/e6-iwram-code-pinning/` (README +
`e6-iwram-code-pinning.patch` — the diff against main @ `7d8b412`, byte-
identical to the measurement commit `bd48b04`'s diff, verified `git apply
--check` clean there). The plan of record — PR 4 cadence, PR 5 verdict —
is untouched, and no decoder file in `crates/flac-lite/` changed. Variant
ROM `flac-spike.gba` sha256
`ff3a2f56a84aa77197dc2d4c1b355bb59a0e87bbbd0d59f6d8b85a8ab3ea30f9`; mGBA
0.10.5 `mgba-test-runner` runs 2026-09-26 on the same host runner as PR 3
/ E1 / E2a / E4 / E7 (Rust nightly 1.100.0-nightly a69a63265 2026-09-03);
two headless runs byte-identical (`cmp`, 1006 lines). The control image
was built from a **separate worktree at base** `7d8b412` (sha256
`65cd0c36e9e204ebbaf6a9376f20fb81ecc3936ffc617179fa908482b0c41e84`; two
runs `cmp`-identical, 675 lines) — per the cross-tree rule its **stats**
witness the control, never the sha; its serial log is `cmp`-identical to
the E7 control run recorded on 2026-09-25, the same base commit. Gates
green on the patched tree: `make spike-test` (host tests 23 → 25: lib 15
unchanged, +2 `tests/e6_witness.rs`, `spike_witness` 8 unchanged), `make
flac-test` (115 passed, 1 ignored — unchanged; `crates/flac-lite/`
untouched by the patch), `make check`, `make native-spike-rom`.

**Harness shape (two IRQ-off windows per frame, one image — the E2a
pattern, applied to placement as the only source-level variable):**

1. **ctrl** — PR 3's `driver::decode_one` (the verbatim i64 path on the
   position-only `BitReader`), the same-run control;
2. **variant** — `e6::decode_one`: a **verbatim vendored copy** of the hot
   path (reader, header/subframe/residual decode, Rice loop, both
   integrators — same arithmetic, same unary loop, same rejection rules)
   whose hot functions carry
   `#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]`:
   a pinned **spine** (`inline(never)`): `decode_frame`,
   `decode_subframe`, `decode_residual`, `decode_rice_partition`,
   `integrate_fixed`, `integrate_lpc`; pinned **inline leaves**
   (`inline(always)`): the reader primitives + `rice_unmap`/`pad_block`/
   `fill_state`, so the per-bit loop carries no extra call vs the
   LTO-inlined control; **unpinned** per-frame plumbing (header parse, the
   `decode_one` wrapper, and the shared `stereo::decorrelate` — a real
   shared call, E2a's vendor boundary) stays in ROM. `lib.rs`
   `forbid(unsafe_code)` relaxes to `deny` only so the e6 module's
   placement attribute can carry a module-scoped `allow` (edition-2024:
   `link_section` is an unsafe attribute) — part of the sample patch.

**Witnesses before any number counted:** host (`tests/e6_witness.rs`):
the vendored path decodes the exact embedded clips bit-exactly vs the
`flac -d` reference PCM, and differentially frame-by-frame against the
production `driver::decode_one`. ROM: per-frame variant-vs-control
**sample-equality** — 314/314 frame-windows equal, checksum delta 0,
first offender named had any diverged. Link witness: `nm` on the built
ELF — the six pinned spine functions from `decode_frame`
(`0x0300_022c`) through `decode_rice_partition` (`0x0300_1178`, size
`0x330`); the `.iwram` section totals `0x1540` = 5,440 B (pinned code
4,732 B + agb's own IWRAM helpers + interworking thunks) of the 32,512 B
budget. Runtime placement witness in the boot log:
`pinned_rice=0x03001179` — the running image prints the hot function's
IWRAM address. Calibration overhead 24, stable across passes, nets 0.
Decode proof matched the PCM pin `0x54C7B356621B6E15` on both arms before
any timing.

**Numbers (net of this image's own calibration; 157 frames / arm; budget
1,048,750 cycles/frame ≈ 512 c/sample against 157 × 2,048 block
samples):**

| arm | ctrl sum | variant sum | delta | ctrl mean %budget | variant mean %budget | variant worst %budget |
|---|---|---|---|---|---|---|
| FIXED (`l0_stereo`) | 357,908,871 | 421,705,665 | **+17.8% (slower)** | 217.4% | 256.1% | 257.8% |
| LPC (`l4_stereo`) | 1,073,988,846 | 1,170,737,712 | **+9.0% (slower)** | 652.3% | 711.0% | 715.7% |

c/sample: ctrl FIXED 1,113.1 → variant 1,311.5; ctrl LPC 3,340.2 →
variant 3,641.1. Control reproduces the recorded baseline shape exactly:
worst-frame indices **exact** (FIXED @60, LPC @136, min @156 both arms);
this image's PR 3-shape pass printed FIXED sum=357,906,830
max=2,294,068@60 and LPC sum=1,073,986,805 max=6,880,979@136 — the e6
ctrl arm sits a constant +13 cyc/frame above it (+2,041 sum, code
layout), the identical offset E2a recorded on its image. The variant's
worst frame moves index (FIXED @60 → @4, LPC @136 → @145, deterministic
across both runs) — the mean, not the worst, is the reading.

**Reading (the menu's own rule, executed):** "if FIXED collapses, the 2.2×
floor is instruction fetch and E6 is the production fix; if it doesn't,
code timing joins the ruled-out list." It did not collapse — both arms
deterministically **regressed**. E6 is demoted: code placement joins the
ruled-out list, and E8's attribution picks the next lever, exactly the
fallback this entry's rule named.

**On the regression itself (one confound, stated honestly):** placement
was implemented via vendoring (the menu's own discipline: the vendor
boundary follows the type signatures), so the variant differs from the
control in inlining layout as well as placement — visible in `nm` sizes:
`e6::decode_subframe` `0x7f0` vs `flac_lite::decode_subframe` `0x56c`
(the `inline(always)` leaves materialize inside it), `e6::decode_residual`
`0x3fc` vs `0x574`. The answer's **direction** survives the confound:
the collapse this menu predicted (217% → ≈100%) would have required the
layout change alone to cost over 50% — no probe on this harness has ever
moved the mean that far without a real algorithmic change (the pure-
layout cross-session offset is the constant +13 cyc/frame both vendored
probes print). For fetch starvation to still be the cost center, pinning
every hot instruction to zero-wait IWRAM would have to have paid for a
large layout regression *and* still hidden a collapse. If code timing is
ever revisited, the layout-controlled follow-up is the same vendored
module **un**pinned (placement isolated from layout) — not menu-grade
while E8 remains unrun.

**Consequences:** the ruled-out list grows — cartridge data reads (E1),
reader implementation (E2a), i64 integrator math on the FIXED arm (E4),
the frame-pointer flag (E7), and now **instruction fetch location** (E6).
What remains for FIXED's ~1,113 c/sample is the unary loop's per-bit
*execution* cost and generic plumbing — the menu's stated fallback, so
**E8 (sub-stage attribution) leads**. E10's rationale is per-iteration
work (three-operand arithmetic, barrel-shifter modifiers), a cost lever
not a fetch lever, and this result leaves it untouched; E2b stays
tempered. The ordering note above is amended.

**Preservation:** probe code lives in the sample dir above, never on
`main`'s spike sources. The `perf/flac-spike-e6-iwram-code` branch
(`bd48b04`, never pushed) is superseded by the sample and is deleted
after this docs PR merges, per convention.

## E8 result — sub-stage attribution names the cost centers (2026-10-12)

**Prior assumptions:** none of the menu's claims needed correcting — the
E8 build is pure instrumentation, and the control arm reproduced the
recorded baseline shape exactly (below). The menu's "~1,078 c/sample" for
FIXED is the PR-3-shape figure; this image's control printed 1,113
(357,989,569 / 157 / 2,048) — the same code-layout offset E2a and E6
recorded on their images, not a new fact. The burden was carried by the
measurement itself, whose premise — the cost center sits *inside* the
per-sample loop, not in the stage plumbing around it — the table below
confirms directly for the first time.

**Sample, not integrated.** As with E1/E2a/E4/E6/E7, the probe never lands
on the live spike sources: it is preserved as a reappliable sample under
`examples/flac_spike/experiments/e8-substage-attribution/` (README +
`e8-substage-attribution.patch` — the diff against main @ `eadb7ed`,
byte-identical to the measurement commit `c9a7e77`'s diff, verified `git
apply --check` clean at that base). Nothing in `crates/flac-lite/`
changed. Measured ROM `flac-spike.gba` sha256
`f940ac5ea372437d2fbcbce09f9fde249c799317c8e32bbf037ea4f7ac4dbc0a`; mGBA
0.10.5 `mgba-test-runner` runs 2026-10-12 on the same host runner as PR 3
/ E1 / E2a / E4 / E6 / E7 (Rust nightly 1.100.0-nightly a69a63265
2026-09-03); two headless runs byte-identical (`cmp`, 703 lines); rebuild
from the measurement commit in the same tree `cmp`-identical to the
measured image. Gates green on the patched tree: `make spike-test` (host
tests 23 → 28: lib 15 + 2 `e8.rs` unit + 3 `tests/e8_witness.rs` +
`spike_witness` 8), `make flac-test` (115 passed, 1 ignored — unchanged),
`make check`, `make native-spike-rom`.

**Harness shape (measurement, not a variant — one pass per arm, one
image).** Per frame: the verbatim PR 3 `driver::decode_one` in a `timed()`
window (the same-run control), then the vendored instrumented decode
inside one **gross** window — `agb::critical_section::acquire` held across
the whole frame with the token carried between `open`/`close` (IME = 0
read inside as witness), so the stage windows nest inside without
re-acquiring. Stage windows (`Meter::timed`) at header / subframe prefix /
residual-header + parameter reads / escape bodies / integrator / wasted-
bit pad / decorrelate (live shared call) / footer / misc. **Inside the
Rice loop no windows** — a `timed()` costs 22–35 cyc and two per sample
would inflate the loop under measurement by ~96 cyc/sample; the cheap
instrument is the raw 16-bit low half: two boundary reads per sample
(after the unary quotient loop; at sample end) with wrapping deltas,
back-to-back pair measured 4 cyc. Three instrument shapes, three
calibrations, each with its own E4-shape stability witness (3 discarded +
5 reported, byte-identical iterations): stage window **35**, gross open/
close **37**, boundary pair **4** — a window nested inside an open gross
costs more than the plain-path 22–24, so nothing was carried over from
earlier images. Every calibration stable on both arms, IME = 0 in every
window. Tripwires: near-wrap deltas (≥ 60,000 of the 65,536 period)
**0**; closure fold `gross − (Σ windows + Σ spans)` positive every frame
(min 4,884 FIXED / 5,897 LPC — the unattributed plumbing), underflows
**0**; the per-frame e8-vs-control **sample-equality** witness (outside
the windows) clean — no `FAIL` lines, 157/157 frames both arms — and the
asset + PCM-pin decode proof (`0x54C7B356621B6E15` `[MATCH]` both arms)
ran before any timing. Host witness (`tests/e8_witness.rs`, under
`make spike-test`): the vendored path bit-exact vs `flac -d` on the
embedded bytes, twin-agreement with the live driver, and `NullMeter`
bookkeeping (bucket occurrence counts; `integ + misc` witnessed only as a
total per the shape-determined rule).

**Numbers (bucket sums net of each shape's own calibration; share of the
same-image netted gross; 157 frames, 320,000 samples/channel, resid
samples 640,000 FIXED / 638,746 LPC = 640,000 − 4 × 314 order-4 warm-ups
— the tally's own arithmetic witness):**

| bucket | FIXED sum | FIXED share | LPC sum | LPC share |
|---|---:|---:|---:|---:|
| unary (quotient loop) | 336,179,902 | **56.2%** | 424,518,364 | 31.2% |
| rest (rem. read + fold + unmap + store) | 230,166,712 | **38.5%** | 228,346,862 | 16.8% |
| integ (FIXED/LPC integrators) | 23,864 | 0.004% | 673,676,420 | **49.5%** |
| decorr (live shared call) | 24,974,758 | 4.2% | 24,974,758 | 1.8% |
| resid head + params | 271,547 | 0.05% | 906,968 | 0.07% |
| subframe prefix | 430,690 | 0.07% | 858,738 | 0.06% |
| header parse | 539,657 | 0.09% | 539,657 | 0.04% |
| footer (align + CRC reads) | 21,980 | 0.004% | 21,980 | 0.002% |
| wasted-bit pad | 6,908 | 0.001% | 6,908 | 0.001% |
| escape / misc | 0 (count 0) | — | 0 (count 0) | — |
| closure (unattributed) | 768,912 | 0.13% | 925,917 | 0.07% |
| **gross (netted)** | **598,570,556** | | **1,359,963,156** | |

`escape`/`misc` never fired: these clips encode no escape partitions and
no CONSTANT/VERBATIM bodies, so the attribution covers every path the
profile produces. `decorr` and `hdr` are byte-identical sums/min/max
across the two arms — the same live shared calls, a free cross-arm
witness that the two passes really did decode the same thing.

**Control reproduces the recorded baseline:** FIXED sum 357,989,569
max 2,294,595@60 min 580,783@156; LPC sum 1,074,091,932 max 6,881,652@
136 min 1,502,291@156 — worst-frame and min indices **exact** against the
recorded baseline (FIXED @60, LPC @136, min @156 both arms); sums
+0.023%/+0.010% over the E6 image's control (+514/+657 cyc/frame —
code-layout offset, the same phenomenon E2a/E6 recorded, this image's
larger because the vendored instrumented module sits in the same LTO
codegen unit and reshuffles more: `nm` sizes `e8::decode_subframe_e8`
`0xe2c` vs `flac_lite::subframe::decode_subframe` `0x388`,
`e8::decode_residual_e8` `0xba4` vs `0x574`). The instrumented gross
sits above the control frame (+67% FIXED / +27% LPC — meter bookkeeping +
layout): **the reading is per-bucket shares of the same-image gross, not
c/sample taken from the instrumented path** (the menu's E6-confound rule,
stated with its bound: the shares name where cost sits, they do not
restate its magnitude).

**Reading (the menu's rule executed — name the cost center from the
table, not inference):**

1. **FIXED's cost center is the Rice per-sample loop: 94.6% of gross**
   (unary 56.2% + rest 38.5%); every bucket outside the loop totals 5.4%.
   The stage plumbing (header, prefix, resid head, footer, pad) is noise
   (≤ 0.1% each). E2a's non-collapse now has a mechanism reading: the
   reader feeds the whole loop, but the loop's cost is per-**bit**
   iteration and per-sample plumbing, neither of which the accumulator
   swap touched.
2. **The span data separates the two loop halves by mechanism:** `rest`
   is arm-invariant (359.6 vs 357.5 c/resid-sample — magnitude-independent
   plumbing: remainder read + fold + guard + unmap + store), while
   `unary` scales with residual magnitude (525.3 FIXED-0 clip vs 664.6
   LPC clip) — quotient-length-dependent iteration, exactly the cost E2b
   counts another way and E10 folds. E2b's addressable share is real:
   56.2% of FIXED's gross.
3. **The prize is bounded, and the bound is arithmetic:** zero-cost
   `unary` (unrealizable — E2b's point is to shrink it, not delete it)
   leaves `rest` + non-loop = 43.8% of gross; applied to the control's
   217.4% mean that is **≈ 95.3% of budget** — FIXED reaches real-time
   only in the free-loop limit, so no codeword-loop lever alone closes
   FIXED. E2b cutting `unary` in half lands ≈ 217.4 × 0.719 ≈ 156%; and
   clearing the *mean* alone needs `unary` down ≈ 96% (the worst frame
   sits ≈ 0.6% above the mean, so mean-and-worst together is beyond any
   single lever). Per the
   cumulative-gains rule (2026-10-11), this is a numerator lever for the
   product with the rate track, not a standalone fix.
4. **LPC's cost center is the i64 integrator: 49.5%.** E4's i32 variant
   measured −46.9% on LPC / −3.7% on FIXED — exactly the asymmetry this
   table draws (integrator ≈ 0 on FIXED-0's early-returning integration;
   half the LPC frame otherwise). The table predicts E4-i32's magnitude
   without E4 having been run for it — the attribution is load-bearing.

**Legacy-pass quirk on this image (deterministic, harmless):** the legacy
whole-frame calibration pass printed first-window-cold raws (33, 24, 24,
24, 24) on both arms and voided *itself* (`FAIL calibration unstable` →
the PR-3 verdict line reads FAILED, `measured 0/2`). That is the recorded
cold-first-window bus property (E1: raws 31/33/33/33/33 byte-identical
across runs) re-triggered by this build's code layout; it voids only the
legacy pass's own numbers, which nothing here reads. The E8 pass carries
its own three stable calibrations and its own control; the E8 summary
lines read `COMPLETED` on both arms in both runs. A re-run that wants the
legacy pass green needs the E8 module out of the image — not worth a
second build; the attribution method is self-contained by design.

**Consequences:** E2b (count the quotient without a per-bit loop —
masked-ones / run-length table, no CLZ in ARMv4T) **leads the decoder-
side menu**, its ceiling now priced (56.2% of FIXED's gross; floor
arithmetic above). E10's first gate ("E8 attributes a large share to the
codeword loop") is **met**, its second ("E2b underdelivers") still
pending. E9 (composite arms) runs before PR 5's verdict, now with its
minimum stack pre-argued by the table (E4-i32 + E2b target different
buckets — 49.5% LPC vs 56.2% FIXED — the orthogonality argument it
measures). Nothing joins the ruled-out list: E8 measured where cost
sits, not what moves it. The rate track's step-5 sweep re-run gains its
named decode levers from this table (integrator-i32 already landed;
loop-shape levers pending E2b).

**Preservation:** probe code lives in the sample dir above, never on
`main`'s spike sources. The `perf/e8-substage-attribution` branch
(`c9a7e77`, never pushed) is superseded by the sample and is deleted
after this docs PR merges, per convention (content-based proof: the
committed patch byte-identical to `git diff eadb7ed c9a7e77`).

## E2b result — run-length table count lands the biggest decoder-side win on the menu so far (2026-10-13)

**Sample, not integrated.** As with E1/E2a/E4/E6/E7/E8, the probe never
lands on the live spike sources: its code is preserved as a reappliable
sample under `examples/flac_spike/experiments/e2b-rice-quotient-table/`
(README + `e2b-rice-quotient-table.patch` — the diff against main @
`14538c4`, byte-identical to the measurement commit `fba1bc5`'s diff,
verified `git apply --check` clean at that base). The plan of record —
PR 4 cadence, PR 5 verdict — is untouched, and no decoder file in
`crates/flac-lite/` changed. Measured ROM `flac-spike.gba` sha256
`ba1e8a34b2f1531a68a7991946c7c87f1b33a95d44751a5200bfbf2dd24fd7a9`
(rebuild sha-stable); mGBA 0.10.5 `mgba-test-runner` runs 2026-10-13 on
the same host runner as PR 3 / E1 / E2a / E4 / E6 / E7 / E8 (Rust nightly
1.100.0-nightly a69a63265 2026-09-03); two headless runs byte-identical
(`cmp`, 1,010 lines). Gates green on the patched tree: `make spike-test`
(host tests 23 → 35: lib 15 → 24 (+3 `e2.rs`, +6 `e2b.rs`),
`tests/e2_witness.rs` 1, `tests/e2b_witness.rs` 2, `spike_witness` 8
unchanged), `make flac-test` (115 passed, 1 ignored — unchanged;
`crates/flac-lite/` untouched by the patch), `make check`, `make
native-spike-rom`.

**Harness shape (three IRQ-off windows per frame, one image — the E2a
two-window pattern, one arm extended):**

1. **ctrl** — PR 3's `driver::decode_one`, the verbatim live path, the
   same-run control;
2. **acc** — `e2::decode_one_accumulator`, the **E2a sample carried
   verbatim**: the accumulator reader with the unary loop still per-bit.
   This arm is not a new variable — it prices **the count change alone**
   (`tab − acc` is same-reader, same-vendored-body, only the loop
   swapped) and doubles as a cross-session witness (below);
3. **tab** — `e2b::decode_one_rice_table`: the same accumulator reader,
   unary loop replaced by the run-length-table count — `LEADING_ONES[
   !(top byte of window)]` answers quotients 0..=7 in one zero-wait
   `ldrb`, 8 means "whole byte of zeros, keep counting **per byte**";
   the slice tail folds to the top of the inverted byte with a stopper
   bit so the same table answers short tails; remainder extraction rides
   the same window (one shift pair consumes `q + 1`, no `peek_at`
   round-trip). The 256 B `u8` table is `#[link_section = ".iwram")]`
   (plain `.iwram`, the E6-verified pin target, never `.text_iwram`),
   copied ROM→IWRAM at boot by agb's `CommonInit` CPUSet.

Calibration: E4's verified unified-loop shape, overhead **24**, stable,
nets 0, all passes; IME = 0 in-window. Decode proof matched the PCM pin
(`0x54C7B356621B6E15`, cross-arm agree) before any timing. The legacy
PR-3-shape pass completed on this image (no E8-style cold-first-window
void): FIXED sum 357,909,970 / LPC 1,073,989,945, worst indices @60/
@136 — the e2b ctrl arms sit a constant −1,413 below (≈ −9 cyc/frame,
code layout), and ctrl's worst-frame indices are **exact** against the
recorded baseline (FIXED @60, LPC @136, min @156 both arms).

**Witnesses before any number counted:** host (`tests/e2b_witness.rs`):
the variant decodes the exact embedded clips bit-exactly vs `flac -d`
**and** differentially frame-by-frame against the production driver.
Host unit tests (`e2b.rs`): the table exhaustively checked against a
naive leading-ones oracle; quotient lengths 0..=40 swept at every bit
offset 0..7 against a per-bit oracle; EOF (unterminated run) restores
reader state; tail/edge bytes. ROM: per-frame variant-vs-control
**sample-equality for both variant arms — 314/314 frame-windows equal**,
first offender named, divergence voids. Link witness: `nm` —
`LEADING_ONES` at `0x0300022c`; the `.iwram` section grew **exactly
256 B** (0x22c → 0x32c, `objdump -h`); the boot log echoes
`table=0x0300022C` as the same-image runtime witness. Vendor-boundary
size evidence: `e2::decode_subframe` and `e2b::decode_subframe` are the
same size (`0x378` each), `decode_residual` `0x27c` vs `0x254` — the two
vendored bodies differ only by the loop replaced by the count, so
layout moved least between the arms that the marginal reading compares.
One deliberate divergence, scope-stated: on an unterminated unary run
the table count restores the reader to the word start where the live
loop leaves the cursor after the consumed zeros — observable only on a
corrupt stream (frame dead on both paths), and never reached on the
committed clips (bit-exact witness).

**Numbers (net of this image's own calibration; 157 frames / 320,000
samples per arm; budget 1,048,750 cycles/frame ≈ 512 c/sample):**

| arm | ctrl sum | acc sum | tab sum | ctrl worst | tab worst |
|---|---:|---:|---:|---|---|
| FIXED (`l0_stereo`) | 357,908,557 | 346,674,620 | **280,207,045** | 2,294,079 @60 | 1,793,645 @136 |
| LPC (`l4_stereo`) | 1,073,988,532 | 1,089,159,694 | **970,588,755** | 6,880,990 @136 | 6,213,965 @145 |

- **FIXED:** tab −21.71% vs ctrl (77,701,512 cycles; 494,914/frame),
  **−19.17% marginal vs acc** (66,467,575; 423,360/frame). Mean
  217.4% → **170.2%** of budget (2,279,672 → 1,784,758; 1,113.1 →
  871.5 c/sample). Tab's worst index moves @60 → @136 — frame 136 is a
  ctrl near-max (2,292,954 ctrl_net) and the top frames shuffle order
  under layout like E2a's; deterministic across both runs; the mean is
  the reading.
- **LPC:** tab −9.63% vs ctrl (103,399,777; 658,597/frame), **−10.89%
  marginal vs acc** (118,570,939; 755,229/frame). Mean 652.3% →
  **589.5%** (3,340.2 → 3,018.6 c/sample).
- **The acc arm reproduces E2a's recorded result exactly** (−3.14%
  FIXED / +1.41% LPC vs ctrl; sums within 0.0004% of E2a's own image) —
  the cross-session witness that this image's reader arm *is* the E2a
  arm.
- Worst frames stay **OVER BUDGET in every arm** (tab worsts 171.0% /
  592.5%). Screen verdict RED = measurement completed, budget missed —
  harness completed 2/2.

**Reading (the menu's rule, executed — E2b was to "count the quotient
without a per-bit loop"; does it collapse the FIXED arm?):** the
biggest decoder-side movement the menu has produced, and E8's table is
load-bearing a second time. Expressed as the implied cut to the E8
`unary` bucket (marginal reading, same-reader basis): FIXED removes
34.1% of its 56.2% share (19.17/56.2), LPC 34.9% of its 31.2% share
(10.89/31.2) — **two clips with different bucket shares, one mechanism,
the same ≈⅓ depth**. E8's floor arithmetic frames the win honestly: it
priced *halving* unary at ≈156% — the measured depth is ≈⅓, landing
170.2%, between the arithmetic's illustrative points and nowhere near
the zero-unary 95.3% limit. Clearing the mean still needs unary down
≈96%; the table took a third of it. So the count *shape* was the cost,
confirmed — and the remaining ≈⅔ of the unary bucket (≈37% of FIXED's
gross) is now the best-priced standing cost on the decoder side.
Nothing joins the ruled-out list: E2a's ruling was about the reader
*implementation*; this is the loop *shape*, and it moved.

**Placement (the one placement Sam scoped, 2026-10-13):** the table's
IWRAM pin is baked into the single measured tab arm (ROM-vs-EWRAM-vs-
IWRAM placement was deliberately not varied — stated, not silently
assumed: the arm's number is "table in IWRAM", and E6's code-pinning
regression did not transfer to 256 B of hot *data* — the win is large
and positive where code pinning was negative; if the pin ever cost
anything here it would have to hide inside a −19.17% marginal).

**Consequences:** **E9 (composite arms) leads** — run before PR 5's
verdict with the stack now concrete: E4-i32 + E2b-table (different
buckets by E8's table: 49.5% LPC integrator vs the unary loop; E2b's
arm measured on i64 live arithmetic, so orthogonality is now argued
across two independently-measured levers and E9 measures the product);
E6 drops from the minimum stack (ruled-out lever, don't stack it). E10
stays open with its gates re-read (first gate met; second gate not
literally met — E2b *delivered* — but ≈⅔ of the bucket standing *is*
E10's headroom, and its own ceiling is bounded by the same 95.3%
zero-all-unary floor). The rate track's step-5 re-run gains its first
landed decoder-side relief factor: ≈1.28× on FIXED's mean (217.4 →
170.2), which under the cumulative-gains rule multiplies with the rate
switch (18,157 Hz needs ≈1.21× decoder relief — E2b's measured marginal
alone already exceeds it, pending E9's in-stack confirmation). Per the
cumulative rule the E2b delta is harvestable regardless of who leads:
it is a measured numerator cut on the decode path.

**Amended 2026-10-26 (Sam): the landing did not reproduce the win —
resolved 2026-10-27 by landing the missing E2a refill-accumulator.**
`e455dba` landed this table in the live `flac-lite` decode path; a
same-host rate-ROM bisect (two `cmp`-identical runs per point, staged
assets census-identical, PCM pins MATCH) measured that first landing at
**+37.96 % FIXED / +15.2 % LPC** decode vs the pre-landing tree — the
landing's own era printed producer walls consistent with that (≈267 ms).
**Root cause:** the E2b probe measured `rice_quotient` riding E2a's `u32`
refill-accumulator (`AccumulatorReader`), where the next 8-bit window sits
pre-shifted at `self.acc >> 24` and the subsequent `read_bits(order)`
remainder extract shares the same buffered register window. `e455dba`
instead bolted the 8-bit table lookup onto the stateless position-only
`BitReader`, calling `peek_at(self.bit_pos, 8)` per window (a 2-byte
`u64` gather at 7/8 of bit offsets) and then calling `peek_at` a second
time over the same bytes for the remainder. **Fix (2026-10-27):** E2a's
`u32` refill-accumulator (`pos`, `acc`, `valid`, `refill`, `consume`)
is now landed inside `flac_lite::bits::BitReader` alongside E2b, restoring
the probe's register-window shape (`!(self.acc >> 24) as u8`) and
achieving real-time audible playback at correct tempo and pitch on
`flac-rate-play.gba` — see the rate-play entry's 2026-10-27 amendment.

**Preservation:** probe code lives in the sample dir above, never on
`main`'s spike sources. The `perf/flac-spike-e2b-rl-table` branch
(`fba1bc5`, never pushed) is superseded by the sample and is deleted
after this docs PR merges, per convention (content-based proof: the
committed patch byte-identical to `git diff 14538c4 fba1bc5`).


---

## E10 result — the ARM-mode hybrid codeword loop lands the menu's biggest FIXED-side win (2026-10-30)

**Sample, not integrated.** As with E1/E2a/E2b/E4/E6/E7/E8, the probe never
lands on the live spike sources: its code is preserved as a reappliable
sample under `examples/flac_spike/experiments/e10-three-arm-probe/`
(README + `e10-three-arm-probe.patch` — the diff against main @ `c3ebd3b`,
byte-identical to the measurement commit `5525336`'s diff, verified
`git apply --check` clean at that base). The plan of record — PR 4 cadence,
PR 5 verdict — is untouched, and no decoder file in `crates/flac-lite/`
changed. Measured ROM `flac-spike.gba` sha256
`87c80fa54e208e5940b9c1c4dc3a7dc55a0ac6f5c8fa51347e45137343c093db` (the
image that printed the quoted log; a clean-worktree rebuild of the
measurement commit printed a **different** sha `2aba8dcf…` — the recorded
rebuild drift — while reproducing the **whole serial log byte-identically**
(`diff` clean, 1,017 lines): the log is the witness, the sha provenance);
mGBA `mgba-test-runner` runs 2026-10-30 on the same host binary as every
recorded entry (toolchain `nightly` = rustc 1.100.0-nightly `a69a63265`);
two headless runs byte-identical (`cmp`).

**Harness shape (four IRQ-off windows per frame, one image — the E2b
three-window pattern, one arm added):** **ctrl** — `driver::decode_one`
(verbatim live path); **acc** — E2a's sample carried verbatim (cross-session
witness); **tab** — E2b's sample carried **verbatim**: the plan's baseline,
because E10 *replaces* the count rather than stacking on it (left-to-measure
item 6's reading `arm − tab`); **arm** — `e10::decode_one_arm`: the e2b
vendored body verbatim except `decode_rice_partition`, which hands the whole
non-escape sample loop to the ARM kernel `arm_rice_partition`
(`asm/rice_partition.s`; the fold rides the committed `asm/fold_body.s`
paste, pinned byte-identical to the kernel crate's copy by a host test).
One realization note: the probe composes the hybrid as a single whole-
partition kernel rather than calling the track's word-in/word-out kernels —
the seam needs the byte-window ABI, so `arm_rice_partition` re-embodies the
tier map (tier-1 `ctz8` q ≤ 7 → C1 De Bruijn 8..=31 → C2 word-skip, handoff
on `w == 0`, decisions 2/5) around the reader; the strict full-window tier-
boundary test re-witnesses the boundaries. The E2b table count has meanwhile
**landed** in `crates/flac-lite` (visible in `nm`: `bits::LEADING_ONES` at
`0x0300022c`), so ctrl ≈ tab on this image (sums within −2 cyc/frame,
worst indices exact) — `tab` is carried verbatim so the plan's marginal
stands as written.

**The arm's tables pin `.iwram` (ratified item 2): `nm`** — `BRUIJN_INDEX`
@`0x0300032c`, `CTZ8` @`0x0300034c`, `REV8` @`0x0300044c`; `objdump -h` —
`.iwram` grew **exactly 544 B** over the e2b layout (the plan's budget:
256 + 256 + 32); the boot log echoes the three runtime addresses as the
same-image echo. ARM-mode-ness is an ELF fact: 4-byte encodings throughout,
`pop {…, lr}` + `bx lr` tail (never an LDM-to-PC, PR B's measured rule).
Calibration: the `timed_reset` window shape (below), overhead **35** stable,
nets 0, IME = 0 in-window.

**Witnesses before any number counted:** host (`tests/e10_witness.rs`, under
`make spike-test`): the variant decodes the exact embedded clips
**bit-exactly** vs `flac -d` **and** differentially frame-by-frame against
the production driver; lib unit tests (+6): tables exhaustive vs naive
oracles (incl. rev32 bit-mapping, 200k-word De Bruijn agreement), the
**bit-offset sweep** — quotients 0..=70 at every reader alignment 0..7 ×
p ∈ {0,1,2,4,8,14} against a naive per-bit oracle (decision 7's cut,
left-to-measure item 5, discharged at the seam), random multi-sample streams
× p 0..=14 vs the per-bit oracle with tier identity
(`samples == t1+t2+t3+slow`), strict tier boundaries on a full window, the
unterminated/short-remainder **restore-nothing** contract (ratified item 1,
witnessed naming the stopped state), and the cold slice-tail leg. ROM:
per-frame variant-vs-control **sample-equality, 314/314 frame-windows
equal** across both variant arms, first offender named, divergence voids.

**Harness port — the Opus rollover rule, convicted mid-run.** The four-arm
pass's windows alone sum ≈ 1.1 G (FIXED clip) + ≈ 3.9 G (LPC clip) — past
2³² = 4.295 G cumulatively, window time only (the inter-window serial
logging rides the same free-running span): the inherited free-running
counter wrapped mid-pass, and the calibration netting turned the wrap
sentinel into `4,294,967,271` printed with `wrapped=false` (LPC frame 125,
where cumulative window time first crosses the roll — exactly the two
failure modes the Opus port documented; the E-menu probes had always
spanned < 2³², so the shape looked safe). The closure arithmetic
convicted the harness before any re-run: run-1 sum − sentinel + neighbour-
frame estimate = 858,864,716 vs the fixed image's 858,870,747 (Δ 0.0007%).
Fix per the recorded recipe: `timed_reset` zeroes the timer pair before
**every** window (reset outside the start→end span — every window still
enters with byte-identical preceding code) and `net_sentinel` lets the
sentinel ride the netting un-subtracted. The first-build image (sha
`b8f11923…`) is cited as the harness-failure image only; every number below
comes from the fixed image.

**Probe-time findings (each witnessed).**

1. **The stitch the plan left to the probe.** The algorithm plan's C1
   consume formula `(w >> (q+1)) & ((1<<p)-1)` is **window-domain**: FLAC's
   Rice remainder is MSB-first natural binary (§9.2.7.2), so the bits the
   reversed window hands over are bit-reversed against the coded number.
   The kernel un-reverses them (`rev16` via two `REV8` lookups + `>> (16−p)`,
   ≈ 8 cyc/sample when p > 0; p == 0 rides the same path for free). This is
   a **second** reversal cost beyond the plan's "one extra ldrb per 8 bits"
   refill tax — the plan priced the refill; the remainder reversal belongs
   to it too. Bit-exactness vs `flac -d` (host) and 314/314 ROM equality are
   the witnesses: no reversal misreading survives them.
2. **C2 is dormant on this content.** Tier census of the twin over the exact
   embedded bytes: FIXED t1 **100.00%**; LPC t1 99.21% / t2 0.79% / t3
   **0.00%** (`unterm` 0, `escapes` 0, max p 10 ≤ 14). The plan's
   "per-clip quotient distribution decides how much" answers **zero** for
   these clips: the win rides ARM across the whole per-sample loop, not
   run-length continuation — the per-sample saving vs tab is the same
   constant on both clips (≈349 FIXED / ≈348 LPC c/sample).
3. **The `rev8` refill tax pays for itself (left-to-measure item 4).**
   The measured saving is a per-sample *constant* across clips with
   different residual densities — if the reversal tax ate the count win,
   `arm − tab` would not sit at the same per-sample constant everywhere;
   the headline risk ("if `rest` moves, the reversal ate the win") did not
   materialize on this shape.
4. Recorded, not attributed by this probe: ARM7TDMI prices register-value
   shifts at 1 + ceil(n/8) — the plan's "one cycle" substrate claim is
   optimistic — and the measured −40% subsumes it. The measured totals are
   the authority.

**Numbers (net of this image's own calibration; 157 frames per arm; budget
1,048,750 cycles/frame ≈ 512 c/sample):**

| arm | FIXED sum | LPC sum | FIXED worst | LPC worst |
|---|---:|---:|---|---|
| ctrl | 280,206,731 | 970,588,441 | 1,793,643 @136 | 6,213,963 @145 |
| acc | 346,674,306 | 1,089,159,380 | 2,222,575 @4 | 6,979,748 @145 |
| tab | 280,212,854 | 970,594,564 | 1,793,682 @136 | 6,214,002 @145 |
| arm | **167,911,108** | **858,870,747** | 1,074,944 @147 | 5,499,144 @145 |

- **FIXED:** `arm − tab` **−40.08%** (112,301,746 saved ≈ 349 c/sample at
  the house mean/blocksize convention). Mean 170.18% → **101.98%** of
  budget (1,784,795 → 1,069,498; 871.5 → 522.2 c/sample); worst
  **102.50%** @147 (frames 136/147 tie at ctrl 1,793,643 — the top-frame
  shuffle like E2a/E2b; the mean is the reading).
- **LPC:** `arm − tab` **−11.51%** (111,723,817 ≈ 348 c/sample). Mean
  589.48% → **521.62%** (3,018.6 → 2,671.1 c/sample); worst 524.35% @145.
- **Cross-session witnesses:** ctrl reproduces the tab sums recorded in E2b
  (−2 cyc/frame, worst indices @136/@145 and min @156 exact), and `acc`
  reproduces E2b's marginals through the landed ctrl: acc/ctrl = 1.23721
  FIXED / 1.12216 LPC vs 1/(1−0.1917)/1/(1−0.1089) = 1.23716/1.12221
  predicted (agreement to ~5·10⁻⁵ both) — the image carries the arms it
  claims.
- Worst frames stay **OVER BUDGET in every arm**. Screen verdict RED =
  measurement completed, budget missed — harness completed 2/2.

**Reading (the menu's rule, executed).** Both marginals sit far past the
demote band (~9–13 cyc/frame layout noise) and past the ≥ ~10% carry bar —
**carry both arms to PR 5's discussion table**. Relief factors on this
image: `tab/arm` = **1.669×** FIXED, **1.130×** LPC; against the PR 3
baseline the FIXED mean has moved 217.4% → 101.98% (2.13× total relief,
E2b's landed count plus this). The measured relief **exceeds** the plan's
"full elimination of the remaining unary ≈ 107.2%" estimate because the
kernel subsumes the whole per-sample loop — fold, remainder, drain, refill —
not just the count the estimate priced; the plan's floor arithmetic
(`rest` + non-loop ≈ 43.8% of frame) was a count-only ceiling and holds
only for that shape. Nothing here closes mean-and-worst (worst 102.50%):
E10 is numerator relief, and per the cumulative rule (2026-10-11) it
multiplies with the rate track rather than substituting — arithmetic on the
measured mean: the FIXED arm path clears **18,157 Hz mean-and-worst**
(56.5%/56.8% of that rate's budget) and 10,512 Hz by ~3×; LPC clears
neither rate on this path alone (167% of budget even at 10,512) and still
needs the E9 stack question. Stacked combinations stay argued, not measured
(E9's discipline; its table now takes `arm` as the FIXED-side arm).

**Prior assumptions that did not survive:** (1) the plan's C1 consume
formula as a literal remainder extract (finding 1 — window-domain vs
spec-order); (2) the free-running counter shape as portable to a four-arm
pass (the Opus rule had to be convicted by an artifact first — **estimate
the pass span before reusing the inherited shape**: mean × frames > 2³² ⇒
per-window reset); (3) otherwise the plan's gates, hybrid, probe shape, and
reading rule survived contact unchanged.

**Deliberate scope cuts:** escape partitions and p > 14 are rejected loudly
(`InvalidField`) — off-profile, census-asserted absent on the clips; the
kernel's cold per-bit tail is kept as defensive code (witnessed directly,
unreachable at the seam by the reader's ≥ 8-bit invariant); the folded-value
overflow guard wraps instead (dead-frame divergence, module docs); no
decode-path wiring; no rate change — the E-menu's fixed 32 kHz embedded
clips, budget `real_time_budget(2048, 32_768)`; no IWRAM budget beyond the
plan's 544 B.

**Gates** (all green 2026-10-30): `make spike-test` host tests **23 → 45**
(lib 15 → 30 (+6 `e10`), `tests/e10_witness.rs` 4, carried `e2` 1 +
`e2b` 2, `spike_witness` 8; old count measured at `c3ebd3b` in a clean
worktree), `make flac-test` 120 passed + 1 ignored (unchanged by
construction — the patch touches no `crates/` file), `make check` clean,
`make native-spike-rom` builds; pre-commit hook full suite green. The
arm-kernels crate is untouched — `kernel-test` records stand.

## E9 result — stacking ARM and i32 lands the first in-budget FIXED decode (2026-10-31)

**Sample, not integrated.** As with E1/E2a/E2b/E4/E6/E7/E8/E10, the probe
never lands on the live spike sources: its code is preserved as a
reappliable sample under `examples/flac_spike/experiments/e9-composite/`
(README + `e9-composite.patch` — the diff against `main` @ `a70174a`,
verified `git apply --check` clean at that base and byte-identical to the
measurement commit `2089854`'s diff). The plan of record — PR 4 cadence,
PR 5 verdict — is untouched, and no decoder file in `crates/flac-lite/`
changed. Measured ROM `flac-spike.gba` sha256
`d95fe9ccbbf004d08154f3174acad0f5c4cc6a7711e922487a2de2d4d0cd5611`
(stable across the measured runs and the post-commit re-hash; sha is
provenance, the log `cmp` is the witness); mGBA `mgba-test-runner` runs
2026-10-31 on the same host binary and pinned nightly (rustc
1.100.0-nightly `a69a63265`) as every recorded entry; two headless runs
byte-identical (`cmp`, 1,357 lines).

**The composite, re-scoped to what the menu had become.** E9 as proposed
(2026-09-25) asked for "E4-i32 + E2b + E6, reporting each component's
marginal inside the stack". Since then E2a + E2b moved *inside* the E10
`arm` (the e2b vendored body with the kernel swap), and E6 was ruled out
(pinning slower on both arms). The measured stack therefore adds the only
two levers still outside it: **E4-i32** — the unstacked lever with a
measured effect — and **E3** (IWRAM decode scratch), riding the run as
the renormalization question (its "sub-1% ceiling" was priced against a
frame 2× slower than the stacked one). **E1 was excluded by design** and
is recorded here so the decision is not re-litigated: staging relieves the
position-only reader's per-byte ROM re-fetch premium, which the ARM
kernel's windowed `rev8` refill already absorbed; shipping decode reads
ROM regardless (E1's production-mitigation ruling), so a staged arm would
double-count the same relief and read at or below the layout-noise band.

**Harness shape (four IRQ-off windows per frame, one image).** **ctrl** —
`driver::decode_one` verbatim (same-run control, cross-session anchor);
**arm** — `e10::decode_one_arm` carried verbatim from its sample (the
cross-session witness that the image carries the arm E9 stacks on);
**i32** — `e9::decode_one_arm_i32`: the e10 vendored body with E4's i32
layer swapped at exactly E4's vendor boundary (`integrate_fixed_i32`,
`integrate_lpc_i32`, `pad_block_i32`, `decorrelate_i32` — verbatim from
the E4 sample; the shared `decorrelate` call becomes a vendored copy —
side arms are width sites; the reader and ARM kernel are **shared** with
`e10.rs`, not re-vendored; `fill_state`'s cold i64 pad untouched by E4
and by this stack alike); **iw** — the same `decode_one_arm_i32` decoding
into two 8 KiB `.iwram` static buffers (`nm`: `scratch::LEFT`
@`0x0300064c`, `RIGHT` @`0x0300264c`; `objdump -h`: `.iwram` **0x64c →
0x464c**, exactly +16 KiB over the E10 image; boot-log echo as the
same-image witness). The image also carries the E10-era e2b four-arm pass
unchanged — it runs before the e9 pass and cross-checks `arm` inside the
same boot (+7 cyc/frame between the two passes' arm arms — the passes'
own micro-layout, well inside the noise band). Windows use `timed_reset`
per window (the pass spans ≈ 7 G cycles — the Opus rollover rule applied
pre-emptively; zero sentinel events).

**Witnesses before any number counted:** host (`tests/e9_witness.rs` under
`make spike-test`): the stacked path decodes the exact embedded clips
**bit-exactly** vs `flac -d` **and** differentially frame-by-frame against
the production driver. ROM: per-frame variant-vs-control sample-equality
for all three variant arms — **628/628 frame-windows equal** (e2b + e9
passes × 2 clips × 157 frames, `eq=1`), first offender named, divergence
voids. Decode proof PASSED before timing (region pins; cross-arm PCM
`0x54C7B356621B6E15` agree); calibration overhead **35** stable, IME = 0
in-window (the E10 timed_reset shape).

**Numbers (net of this image's own calibration; 157 frames per arm;
budget 1,048,750 cycles/frame ≈ 512 c/sample):**

| arm | FIXED sum | LPC sum | FIXED worst | LPC worst |
|---|---:|---:|---|---|
| ctrl | 280,206,417 | 970,588,127 | 1,793,641 @136 | 6,213,961 @145 |
| arm | 167,877,039 | 858,836,678 | 1,074,727 @147 | 5,498,927 @145 |
| **i32** | **155,546,078** | **356,168,695** | **995,812 @147** | 2,280,770 @145 |
| iw | 155,549,218 | 356,171,835 | 995,832 @147 | 2,280,790 @145 |

- **FIXED:** `i32 − arm` **−7.35%** (12,330,961 saved ≈ 38.4 c/sample);
  mean 101.96% → **94.47%** of budget (483.8 c/sample), worst **94.95%**
  @147 — **FITS mean-and-worst at 32,768 Hz**: the first in-budget FIXED
  arm in the menu's history (every earlier arm, E10's included, sat at
  ≥ 102.4% worst).
- **LPC:** `i32 − arm` **−58.53%** (502,667,983 ≈ 1,563 c/sample); mean
  521.60% → **216.31%** (worst 217.48% @145). Over at 32,768 and still
  over at 18,157 Hz (119.9% mean — closing it needs a further ≈17%
  numerator cut, derived 1 − 1,892,682/2,268,590); **fits 10,512 Hz**
  (69.4% mean / 69.8% worst).
- **E3 (`iw − i32`):** **+3,140 total = +20 cyc/frame, the identical
  constant on both clips** — different content, same delta: a code-layout
  artifact, not memory timing. **Inert**; scratch placement joins the
  ruled-out list (output-buffer traffic pays no wait-state premium at
  this scale — consistent with E1's data-staging and E6's code-fetch
  results).

**The composite answer (E9's actual question): levers compose additively
in absolute cycles.** Stacked saving vs E4's isolated saving: **LPC
99.78%** (502,667,983 of 503,768,257), FIXED 92.0% (12,330,961 of
13,399,651). E8's table had priced the buckets as disjoint (i64
integrator vs codeword loop); the measurement agrees — no super-linear
compounding to chase, no sub-linear collision. The stacked *percentages*
are amplified (−46.9% → −58.53% LPC; −3.74% → −7.35% FIXED) purely
because the denominator shrank under an unchanged absolute saving. The
menu's super/sub-linear framing is an answered question: the answer is
addition, so the cumulative-product arithmetic on measured absolutes is
the right composition rule.

**Cross-session witnesses.** ctrl reproduces the E10-recorded sums at
exactly **−314 total** (−2 cyc/frame both clips; worst indices
@136/@145, min @156 exact). `arm` sits **−34,069** (−217 cyc/frame, the
same constant on both clips — this image's vendored-layout offset,
≈0.02% of the sums; the E6 confound, stated and bounded: far under every
reading-rule threshold), means 101.96% / 521.60% vs E10's recorded
101.98% / 521.62%; the carried e2b pass reproduces the e9 pass's `arm` to
+7 cyc/frame within the same boot. The image carries the arms it claims.

**Reading (the menu's rule).** The `i32` arm carries to PR 5's discussion
table as the best-known decode path: **FIXED fits the full-rate budget
mean-and-worst on the emulator**; LPC at 216.3% remains the rate track's
denominator question. Three standing caveats, none settleable by this
probe: **(1) Landing** — E2b's −21.7% sample landed at +38%; these
numbers describe the probe's vendored link, and the kernel + i32 landing
into `crates/flac-lite` must re-witness them on the live link before any
product claim. **(2) E5** — mGBA's gamepak model is unarbitrated
hardware. **(3) Producer scope** — the windows are IME-off decode alone.
At 32,768 Hz the decode FIT (worst 995,812 cyc = 59.3 ms of the 62.5 ms
frame) leaves ≈3.2 ms of margin while the live producer's convert (~6.8
ms, the feeder-off probe's instrument reading) + push (~0.9 ms, the
word-copy era) stages measure ≈7.7 ms — so **full-rate playback** needs
the convert word-store asm-kernel candidate too; decode-side, the gate
closed here. At 18,157 Hz the stacked FIXED decode plus producer stages
derive to ≈59% of budget with room; LPC fits 10,512 with the stages
included (≈73% derived) and misses 18,157. A rate-play ROM re-run at the
target rate is what converts these derived figures into audible claims.

**Prior assumptions that did not survive:** (1) the pre-run
renormalization prediction that E3's "sub-1%" ceiling would read 1–3% of
the stacked frame — it read +0.002%: renormalization amplifies a *real*
absolute cost, and E3 had none to amplify; (2) the pre-run FIXED stack
projection (arm × E4's isolated −3.74% ⇒ ≈98% mean) under-called it —
composition is absolute addition, not percentage-on-percentage, and the
measured saving (12.33M ≈ 7.35% of the arm) landed the mean at 94.47%.
Both corrections point the same way: E9's discipline (measure the marginal
inside the stack) was worth running precisely because the isolation-era
projections were the wrong composition rule.

**Deliberate scope cuts:** E1 staging (double-count rationale above);
E6 code pinning (ruled-out negative — re-measuring it in-stack would
re-measure a known regression); escape partitions and p > 14 inherit the
kernel's loud rejection (census-asserted absent on the clips); no
decode-path wiring; no rate change — the E-menu's fixed 32 kHz embedded
clips; no live-producer measurement (the 32,768 Hz rate-play re-run and
the convert kernel are follow-ups, not this probe).

**Gates** (all green 2026-10-31): `make spike-test` host tests **45 → 47**
(new `tests/e9_witness.rs` 2; carried lib 30, e10 4, e2 1, e2b 2,
spike_witness 8), `make flac-test` 120 passed + 1 ignored (unchanged —
the patch touches no `crates/` file), `make check` clean, `make
native-spike-rom` builds; pre-commit hook full suite green on the
measurement commit `2089854`.


---

**Re-anchored 2026-11-08 — caveat (1) has fired, and the FIXED fit does not survive it.**
Split-ladder item 6 PR 6a-5 measured the *landed* realization (6a-4's ARM kernel,
through the core's `RiceKernel` seam, through the product call path) in the
product-shape image:

| arm (FIXED, 157-frame clip) | sum | c/frame | c/sample | % of 32,768 Hz |
|---|---|---|---|---|
| this record's `ctrl` | 280,206,417 | 1,784,754 | 875.6 | 170.18% |
| this record's vendored `arm` | 167,877,039 | 1,069,281 | 524.6 | 101.96% |
| this record's `i32` stack | **155,546,078** | **990,739** | **486.1** | **94.47%** |
| 6a-5 landed `ctrl` | 275,111,047 | 1,752,300 | 859.7 | 167.08% |
| 6a-5 landed `asm` | 183,288,727 | 1,167,444 | 572.8 | 111.32% |
| **projected landed stack** | **170,957,766** | **1,088,902** | **534.2** | **103.83%** (worst 104.36%) |

The landed arm is **+15,411,688 cycles/clip = +98,164 c/frame = +48.2 c/sample
(+9.18%)** slower than this record's vendored arm. Applying this record's own
*same-image* i32 marginal (−12,330,961 = −78,541 c/frame) to it projects **103.8%
mean / 104.4% worst — over budget**, where the vendored stack sat at 94.47/94.95.
The seam's cost (9.2 points) is larger than the stack's margin (5.5 points). To
recover 32,768 Hz FIXED the i32 leg would have to save ~40,200 c/frame *more* than
measured here (+51%), which is implausible — so this is a structural result, not a
rounding.

Three things this does **not** say. (i) The i32 marginal was measured on the
vendored arm in *this* image; the i32 sites (`integrate_*`, `pad_block`,
`decorrelate`) are outside the kernel and outside the seam, so the transfer is
reasonable, but item 6a-6 landed the width default (6a-7 takes its cycle marginal
in the seam image) and 6a-7 confirms the stack. (ii) FIXED still fits both product rates with the landed kernel *alone*:
61.7% at 18,157 Hz and 35.7% at 10,512 Hz (i64 control: 94.5% / 54.7%); LPC at
10,512 Hz is 183.3% as landed and 85.3% with this record's i32 marginal. (iii) All
of it is mGBA; E5 is still unrun, so neither 94.47% nor 103.83% is a hardware
claim. Where the 48 c/sample goes — seam bridge, the two §9.2.7.3 guards, the
12-byte frame/status, or monomorphization codegen — is **unattributed**: E10's arm
had a vendored reader and no guards, and 6a-5 landed far outside the noise band so
the conditional sub-stage probe did not run. Records:
[`plan-of-record.md`](plan-of-record.md) ("6a-5 — the realization reading"),
[`measurements.md`](measurements.md) (the projection row), and
[`open-questions.md`](open-questions.md) Q12.

**Re-opened 2026-11-08 (6a-5b) — the +48.2 c/sample is a cross-image comparison.**
The paragraph above adds *this record's* paired marginal to *another record's*
absolute, which the reading rule forbids, and the table directly above it says how
much that can be worth: this record's `arm` sits −34,069 cycles from E10's, a
layout offset stated and bounded at ≈0.02% — but the 6a-5 image set measured `ctrl`
moving **−202,529 c/frame** (−99 c/sample) between links that carry one and two
`RiceKernel` bodies, with its source unchanged. Twice the overhang being claimed.

Four static facts bound the claim on the current tree. The FIXED clip is `MidSide`
with **FIXED order 0 on both subframes and partition order 0, 157/157 frames** —
`integrate_fixed` is a no-op and `pad_block` is a no-op, so the frame's per-sample
cost is the kernel plus the shared decorrelate and **there are exactly 2 kernel
calls per frame**; the seam bridge is **~114 Thumb instructions per call**
(≈0.15 c/sample at that call count); the asm bodies differ by **5 instructions per
sample** (the two §9.2.7.3 guards, the only in-kernel delta) plus 3 per partition;
and the three tables sit at `0x0300032c / 0x0300034c / 0x0300044c` — the very
addresses this record's own `nm` witness lists. A +48.2 c/sample *code* difference
has nowhere to live, so the number is an image effect until `asm − vendored` is
read in one link (6a-5b). **Caveat (1) is therefore not "fired":** it is still
open, and it is now the rung that must close it. The fork's carry decision is
untouched — −584,856 c/frame is a same-image paired reading.

**Closed 2026-11-09 (6a-5b) — caveat (1) *has* fired, and it now carries a measured
number.** The differential ran in one image (one more timed leg in the existing pass, E10's module
vendored into the spike tree, `flac-spike-6a5b.gba` sha `b6e35beb1e04028c…`), and
what it found is that the re-opening above was wrong: `asm − vend` =
**+15,431,627 cycles/clip FIXED (+98,291 c/frame = +48.22 c/sample = 24.11 cycles
per Rice codeword)**, +15,409,977 on LPC, the vendored realization faster on
**157/157 frames of both clips**, reproduced to ±13 c/frame across runs — within
**0.4%** of the projection this paragraph called an image effect. Two of this
record's numbers turned out to be load-bearing precisely because they reproduce:
E10's absolute `arm` sum 167,911,108 re-measures at **167,872,486** in a
landed-tree image (−0.02%), and the record's own ≈0.02% layout-offset caveat held
across three links. So the landing did lose 48 c/sample against the probe, and the
question is not whether but where: the bridge, the pinned tables and the 12-byte
state frame are symmetric in that image and priced at **zero**; the two kernels'
instruction streams are identical word for word except **6 executed instructions per
codeword** (G1, G2, and a dead duplicated `and` the guard insertion left behind) and
3 per partition, against 24.11 measured cycles per codeword. The four static facts
above narrowed it correctly — they exonerated the bridge — and missed the unit:
instruction counts bound the floor, not the price. The 94.47% row's arithmetic
survives as a *projection* re-derived from a landed-tree arm (1,069,252 − 78,541 =
990,711 c/frame), which is what it always was; the width default landed in 6a-6
and its cycle marginal is 6a-7's. See plan-of-record ("6a-5b — the reading") and
measurements.md.

**And 2026-11-09 (6a-5c) put a price on the instruction itself: 4.0000 cycles.** The
gap's composition is now arithmetic, not attribution. Deleting one executed
instruction from the landed kernel — the provably dead duplicated `and` the guard
insertion left behind — moved the same-image pair by `2,560,000 = 4.000 × 640,000`
cycles; deleting the five guard instructions as well moved it by another
`12,800,000 = 5 × 4 × 640,000`; 0.46% of the gap is left, and it is per call rather
than per codeword. So 6 × 4 = 24 cycles/codeword is the 24.11 that this record's
own earlier note called "nowhere to live": it lived in instruction count, at four
cycles an instruction — the ARM7TDMI fetch cost of a 32-bit ARM instruction in
cartridge ROM on a 16-bit bus at WS0 = 3,1. Two things follow, and one of them is a
correction to this file. The 6a-5b note above said the static bound's 5-instruction
delta "≈5–15 c/sample" could not be 48; at 4 cycles an instruction it is 20
c/codeword ≈ 40 c/sample, i.e. almost all of it — the bound was right about the
*count* and the item was wrong about the *price*. And E6's "instruction fetch is
ruled out" does not survive as a general fact: it measured a Rust arm where each
instruction carried many cycles of work, which is the one shape in which a 4x fetch
price cannot be seen. That reframes the remaining headroom as a placement question
(6a-5d, the same stream in IWRAM) rather than a spelling one, and it is the first
time in these records that a cycle number has a denominator for it.

**And 6a-5d then showed the denominator was a memory address.** The same instruction
stream, placed in IWRAM instead of cartridge ROM (one section directive, nothing
else changed), takes the landed arm from **1,151,236 to 506,413 c/frame** on the
FIXED clip: **−644,824 c/frame = −316.4 c/sample = −158.2 c/codeword**, i.e.
**109.77% → 48.29%** of the 32,768 Hz frame budget, worst frame 48.5%. The `ctrl`
leg is cycle-identical and `.text`/`.iwram` move by exactly the body's 680 B, so no
other change is available to explain it. The per-instruction price falls from 4.000
(ROM) to ~1.4 (IWRAM): the landed 6-instruction guard delta drops 20.11 → 7.11
c/codeword, and E10's vendored body pinned on its own drops **−290.4 c/sample**.
With both bodies in IWRAM the "realization" pair reads **+14.2 c/sample instead of
+48.2** — so this record's E10 row and its re-anchoring notes were describing two
*addresses*, not two realizations, and the vendored probe's 101.98% was itself a
ROM-resident measurement of the same code. Two caveats, and they are the ones that
matter: mGBA's cartridge wait-state/prefetch model is doing half the arithmetic here
(E5, unrun, is the confirmation that matters), and this is decode-only —
the producer's convert/push stages are not in these windows.

## Appendix — the Phase-1 baseline (spike PR 3, measured 2026-09-21)

The first real numbers, kept because every later percentage is relative to this
image's *shape* and because the tail reading below is still the reason worst-case
is reported everywhere. mGBA 0.10.5; ROM `flac-spike.gba` sha256
`0d97659165b2900600305ee1d7438f90c9f3e05d479068431ef7b6cc6bfeedb3` (a rebuild of
the committed source; hashes identical across rebuilds); both arms 157 frames /
320,000 samples; net of the 22-cycle calibration overhead:

| arm | min @frame | max @frame | sum (cycles) | mean = sum/157 | worst % of 1,048,750 | mean cycles/sample |
|---|---|---|---|---|---|---|
| `l0_stereo` (FIXED) | 585,301 @156 | 2,314,473 @60 | 361,095,055 | 2,299,968 | 220.7% | ≈1,128 |
| `l4_stereo` (LPC ≤4) | 1,506,681 @156 | 6,901,384 @136 | 1,077,175,030 | 6,860,987 | 658.0% | ≈3,366 |

Readings that still hold:

- **Even the cheapest frame is over its own budget.** Frame 156 is the legitimate
  512-sample tail frame; its budget is 262,187 cycles and it cost 585,301 on FIXED
  — 2.2×, so the overrun is not a long-frame or tail effect. Cost tracks sample
  count roughly proportionally.
- **The LPC/FIXED ratio ≈2.98** is the gate's comparison signal, and the reason
  both arms are always measured.
- **The harness did not measure itself:** empty-window calibration was 22 cycles,
  stable on all five samples, all nets 0; repeated runs were `cmp`-identical, and
  the calibration's 5-identical-raw requirement voids a run rather than reporting
  it.
- **Correctness gated the timing:** this image's decode was first proven on its own
  bytes (both arms 157/157 frames, PCM FNV-1a-64 `0x54C7B356621B6E15` matched on
  both arms and cross-arm), which is the rule every later number inherits.
