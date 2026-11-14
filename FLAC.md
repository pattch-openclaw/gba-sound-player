# FLAC.md — compressed audio on the GBA

Goal #4 of this project: decode FLAC on a Game Boy Advance so more audio fits on
a cartridge. This file is the entry point: the constraint, what exists today,
where performance actually stands, what is next, and where every detail lives.
It is deliberately short — the detail is in [`flac-docs/`](flac-docs/), one file
per concern.

> **Scope: this file and `flac-docs/` are the FLAC line of work, exclusively.** Everything here —
> including C1 and C2, [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) (whose "only
> live plan" means *only live plan in `flac-docs/`*, not in the repository), the
> [`flac-docs/measurements.md`](flac-docs/measurements.md) ledger, and every Q-number — judges
> `crates/flac-lite`, `crates/flac-lite-gba`, `examples/flac_integration`, the `GAFP` asset and
> the `flac-*`/`product-play`/`gafp` Make targets. **It is not a repository-wide spec and it
> decides nothing outside the FLAC track.** The sibling codec tracks are planned in their own
> files, each with its own criteria and ledger: [`ULI2.md`](ULI2.md) (**accepted 2026-11-13 as
> the product codec**; its plan moves to `uli-docs/` when the crate exists),
> [`ULC.md`](ULC.md) (scope recorded, **not currently planned**) and [`OPUS.md`](OPUS.md) (an
> evaluation, not a build plan). **"The product" and "the product path" in these pages mean the
> FLAC product path** — `examples/flac_integration` and `product-play.gba`. That phrase carries a
> different, wider meaning in [`ULI2.md`](ULI2.md), so do not import a decision across the
> boundary in either direction; where the tracks genuinely share ground (the ring/FIFO feeder,
> the geometry, the measurement method, the cycle figures) it is named as such.
>
> One live disagreement to be aware of, since both sides recorded it on the same day: this
> track's C2 was amended on 2026-11-13 to **the highest rate achievable** rather than a committed
> 32 kHz ([`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) → "The rate ruling"),
> while [`ULI2.md`](ULI2.md)'s playback gate is written at **32,768 Hz**. Reconciling them is a
> decision for the owner of the product-codec choice, not an edit this file can make.

**What "done" means — two acceptance criteria** (stated 2026-11-10; the live plan is
judged only against these, and full detail is in
[`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) §"Acceptance criteria"):

- **C1 — `flac-lite` stands alone.** It can be used independently as a `no_std`,
  pure-Rust FLAC decoding crate, ready to be separated into its own repository — no
  `todo!()`, a facade a stranger can drive, platform-bound surface confined to
  off-by-default features or to the overlay crate. The no-`todo!()`/facade clause
  closed with 6b-1; the facade is `decoder::Decoder` over `scan::Frames`.
- **C2 — `flac-lite-gba` decodes *and plays back*, in real time, at the highest rate the
  hardware achieves, on a GBA.** *(Amended by the project owner on 2026-11-13, after 7-1's
  hardware runs; it read "in real time, at **32 kHz**" until then, and 32,768 Hz was a target
  rather than a result. The ruling, its arithmetic and its limits are in
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) under "The rate ruling". What
  did **not** move with it: the declared-rate identity rule (Q14), the pack-time structural
  guard, zero underruns, zero late services, both units twice each, the 16-sample grace, and
  the listening witness. The reason is the measured one — the whole chain costs 59.946% of a
  32,768 Hz frame for a **FIXED-0** stream, the cheapest thing FLAC can emit, with no strategy
  in this tree for recovering the factor two that predicted frames would need.)*
  **The rung is now named: 16,384 Hz.** Ruled by the owner on 2026-11-13 as the first candidate
  for the amended C2, on 7-1's measured figures — it is `2²⁴/1024`, so it is **exactly
  expressible**, the declared rate equals the played rate, and Q14 carries across the retarget
  with nothing relaxed. Projected from the second reading's cycles (which do not move with the
  clock): work 59.946% → **30.0%**, worst frame 60.755% → **30.4%**, attributed decode 451,763 c
  → **21.5%** of the frame, the 8,560-sample lead 0.261 s → **0.522 s**, and LPC 173.23% →
  **86.6%** — better, and still not in budget, so 8,192 Hz remains the only rung where predicted
  frames are plausible. It is a *candidate of record*, not a met criterion: C2 is satisfied only
  when 7-1's retarget run establishes it on both units. **An interim instruction to relax
  exactness and target agb's ~18 kHz was considered and withdrawn the same day** — nothing on
  this path instantiates `agb`'s mixer (the product ROM states it as a safety precondition:
  *"no `agb::sound::mixer::Mixer` is instantiated in this ROM"*), so agb's 10,512 / 18,157 /
  32,768 Hz menu constrains nothing here; and at 924 ticks/sample the clock would play
  **18,157.16 Hz**, costing the identity guarantee while making the *confirmed* latency defect
  worse — the push mask is 15.89 samples on the mean and **17.13 on the worst window, against a
  16-sample grace, so still late**, versus 14.34 / 15.46 at 16,384 Hz. Full reasoning in
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) → "The rung is chosen".)* The decode half is measured on hardware on the product path — **37.974%
  worst** of the 32,768 Hz frame (6b-2 pt 2, GBA 001/SP identical; the
  instrument's own hardware row is 42.48% / 42.72%, 6a-7b). The claim is the
  *combination* — decode + convert + push + mixer/DMA in one
  62.5 ms frame, underrun-free, on a GBA 001 and a GBA SP, with FIXED-0 assets
  enforced at pack time (`-l 0` is LPC-disabled, not a fixed-order guarantee). The paper arithmetic reads ≈80.9% of the
  frame, ≈11.9 ms of nominal margin — but two of its three inputs are stale (the
  convert/push figures are 10,512 Hz, pre-kernel, mGBA-era), which is why 7-1
  re-measures them instead of trusting the sum, and why **no real-time claim of any
  kind is made here until 7-3 passes: two runs on each unit with underruns counted
  and zero.** (An earlier revision of this paragraph read ≈85.6% / ≈8.7 ms, from
  the pre-6a-7b numbers.) **7-1's first hardware reading (2026-11-13) is now the first
  measured figure for the whole chain on silicon, and it is lower than that arithmetic:**
  `product-play.gba` at the 7-0 asset, four identical runs (GBA 001 ×2, GBA SP ×2),
  **work 59.921% and worst frame 60.738%** of the 32,768 Hz frame, zero underflows, 582
  periods elapsed and 582 serviced, minimum lead 8,560 samples (261 ms), sample counts and
  both digests exact — so the stale inputs were pessimistic rather than optimistic, and the
  run was refused by the audit for **one** reason: a sound-DMA re-arm landing 23 samples
  (11,776 cycles) into a period against a 16-sample (8,192-cycle) window, at period 22,
  deterministically in all four runs. That is a latency event, not a budget event, and it is
  7-2's to close — with a named candidate attached, marked as unmeasured reasoning in the
  plan: this design holds **interrupts masked for 28.6 samples** while it copies a whole
  decode frame into the ring (agb's critical section is `IME = 0`; the stage instrument has
  been pricing that section at 14,649 c mean, 15,831 c for its worst window, all along), against a
  **16-sample** tolerance for a sound-DMA re-arm. So the miss is a **margin defect in this
  design**, not bad luck and not the unit's fault — and the 16-sample audit clause is what
  made a 28.6-sample mask visible at all.
  **7-1's second reading (same day) settled that mechanism, and settled the rate.** Four more
  runs on both units, on the image built to attribute a late service to the leg that held it,
  all four identical, all four **`late in push 1/1`**: the period-22 boundary crossed inside the
  ring push's interrupt-masked copy, which is the candidate the arithmetic named. The rival
  candidate died from two directions at once — `2nd src 0/582` (no audio entry ever shared an
  interrupt entry) and `disp 0008`, the display's VBlank request bit clear the whole time on
  real hardware, where **mGBA had reported `0x0009` and one co-entry**: the emulator, not the
  unit, was the one with VBlank behaviour, so interrupt topology is now a question an mGBA
  control run cannot answer in either direction. The run also produced the first **attributed,
  isolated decode figure read off a unit — `attr 451,763 c/frame` = 43.08% of the frame**,
  +6.93% over mGBA's same-image 422,494 (inside 6a-7b's 3.7–6.9% band), which is the number
  `gafp_pack`'s structural row and the host model have been waiting on. And the conclusion the
  owner drew from it is the C2 amendment above: **the latency defect is explained and fixable,
  and the run is still 59.946% of the frame** — fixing the mask removes the refusal, not the
  cost. **The rate is dropped as a commitment; nothing else in C2 is.** In particular the
  retarget does not buy the latency work off: at 16,384 Hz the same 14,680-cycle mask is a
  14.34-sample window against a 16-sample grace — clear of it by 1.7 samples on the mean, 0.5 on
  the worst window ever recorded for that section. The chosen fix is what turns that into a real
  margin: `CHUNK`-sized push sections scale the measured window to 4,014 c, i.e. **3.92 samples**
  at this clock and a **4.08×** margin (7.84 samples and 2.04× at 32,768 Hz).

  *(Everything below this line in this bullet was written while 32 kHz was still the commitment,
  and is kept as the reasoning of its date.)* **The test that decides it is built and controlled**
  (image `86f6a1dd7e525dfc…`, 1,106,440 B, host gates and the fit gate green, cost **+87 c a
  frame = +0.008 pp** in mGBA with the decode figure and the minimum lead unmoved): the screen
  now prints `late in push a/b`, which attributes a late service to that section by the period
  counter the section publishes as its first act — exact, not a timing window, because a
  ~29-sample section on a 560-sample grid crosses at most one boundary — plus whether a second
  source shared the interrupt entry, and `attr`, the decode figure minus the interrupt time
  inside its windows, which is what the packer and the host model want. Two things the control
  settled without touching a unit: **the count that looked too small is not too small** (only
  0–2 boundaries land inside the push windows, not the 8.1 a random-phase producer would give,
  because `wait_for_room` phase-locks the producer to the ring — and that count shifts when the
  code layout shifts, so it has to be read from a unit), and **a shared interrupt entry is
  bounded out** (it would have to cost ≥ 11,776 c, five times this ROM's whole audio handler,
  and this build registers one handler). So there are **two** candidates, not three: that push
  section, or a bus/memory stall (Q15), which no interrupt field can see. **The next ran, and it
  came back `late in push 1/1` four times** — mechanism confirmed, second source excluded, and
  the retarget ruled on the same evidence. See the paragraph above this bullet's closing note,
  and 7-1's second reading in
  [`flac-docs/playback-results.md`](flac-docs/playback-results.md).

The 3.7–6.9% mGBA-versus-silicon shortfall (Q13) was **parked by decision**: it would
explain a number that is already measured on hardware, and C2 is judged on hardware
anyway. Until something needs it, mGBA figures for memory-heavy paths carry a **+7%
derating** instead of a probe. **That trigger arrived on 2026-11-13**, on the product path
and at a larger size (12.08% of the whole chain, with convert at +35.1%), so the question is
re-opened as **Q15** with the owner's own hypothesis attached to it — these images reach the
units from an SD-card flash cart, not a mask ROM. Two things have happened to it since.
**It no longer explains the period-22 late service**: 7-1's second reading attributed that
service to the producer's own interrupt-masked ring push (`late in push 1/1`, four runs), which
is a code property and not a bus property. **And it is no longer paired with the rate question**,
because the rate was ruled on 2026-11-13 — but it does not go away with the ruling either, since
an unpriced +35.1% on the convert leg is an unpriced leg in the budget of whatever rate ships;
at 16,384 Hz it is ~7% of the frame instead of ~14%, and the single-variable IWRAM-versus-EWRAM
ring control is still the probe that would put a number on the hypothesis instead of a name on
it. One datum from that reading favours the emulator's side of the ledger and is worth stating
plainly: the instrument cost **+87 c/frame in mGBA but +262 c/frame on silicon** — about three
times — for one `io` read, one store, a compare and a subtraction a frame. And on the interrupt
question, mGBA was the one that was wrong: it reported the display's VBlank request bit **set**
with one shared interrupt entry, where the units read it clear with `2nd src 0/582`, so
interrupt-topology claims now need silicon in both directions — an emulator control run cannot
settle them by presence *or* by absence.

**Status in one line:** the decoder is complete and bit-exact; the playback harness
plays real music at 10,512 Hz with correct tempo and pitch (audibly verified); the
first product-play image runs that whole path at the full 32,768 Hz with zero
underflows and a period audit that is part of the verdict — first in mGBA, and **now on both
units** (7-1, 2026-11-13: eight runs across two instrument images, GBA 001 x2 and GBA SP x2
each, all four readings identical within an image: zero underflows, 582/582 periods serviced, sample
counts and digests exact, and one service 7-9 samples past the half-FIFO window at period 22,
which is what the audit refuses — a lateness the second image attributed **`late in push 1/1`**,
to the ring push's interrupt mask, on every run). **It is still not a playback claim**: the
audit says no, the listening half of 7-3 has not been asked — and **C2 no longer names 32 kHz**,
the owner having ruled on 2026-11-13, on the strength of these very runs, that the product
targets the highest rate achievable rather than a rate committed to in advance; and the asset it plays **was not a product asset either** — it declared 32,000 Hz,
and Q14 was ruled on 2026-11-11 that a stream is encoded at the rate it is played at, so
1.024× (41.06 cents) disqualified it however clean the run (6b-3; the
re-encode was item **7-0**, which **landed on 2026-11-13**: the product asset is now
authored, encoded and declared at 32,768 Hz, `geometry.rs`'s bridge is gone and that module
greps "plan item 7-0 landed, the whole of Q14's ruling in one function" where
`TODO(Q14, BLOCKING)` used to, and `crates/flac-lite`'s encode profile names exactly one
admissible rate — which the ROM itself checks against its shipped asset at start-up); the
**shipped** path — the core's default `TableCountKernel` — is still over budget at
full rate (167% of the 32,768 Hz frame: that is the pre-kernel product reading, and
the control re-read inside the 6b-1/6b-2 product images lands at 154.7%);
and the ARM kernel — landed, witnessed on mGBA's ARM7TDMI, measured through the
shipped seam — now decodes FIXED at **48.3% of the 32,768 Hz budget** (worst frame
48.5%), **2.9× faster than it was a day earlier, from one section directive**.
The core's arithmetic-width default landed as 6a-6 (2026-11-09), and 6a-7
(2026-11-10) priced it same-image: **kernel + i32 is 39.74% mean / 39.96% worst** of
the 32,768 Hz budget on FIXED under mGBA, the i64 leg costs +44.1 c/sample, and
additivity reproduces to 0.06%. **On hardware that decode is 42.48% mean / 42.72%
worst** (6a-7b, GBA 001 and GBA SP, two runs each, identical) — mGBA understates
the non-fetch path by 3.7–6.9% per leg, so 39.74% is an emulator number and 42.72%
is the hardware one.

**6a-7b's instrument landed 2026-11-10:** `flac-spike-6a7.gba` prints its three
legs as a ten-line summary on the GBA screen (fit measured in-ROM: 10 lines, 133 px
wide, 121 px tall) and logs the same ten lines to serial, so the two channels
cannot disagree. Its same-image mGBA reference reproduces the pre-summary 6a-7
record to **≤18 cycles/frame on every leg** (`.text` byte-identical; the default
image byte-identical outright).

**6b-1 then put the kernel in the product ROM and measured it there (2026-11-10):
`rice-arm` decodes at 396,709 mean / 398,923 worst c/frame = 37.83% / 38.04% of the
32,768 Hz frame, on a GBA 001 and a GBA SP, identical to the cycle, PCM pin
matched.** mGBA agrees with hardware on that leg to **0.055%** — the 3.7–6.9%
shortfall 6a-7b found attaches to the *default* kernel's ROM-fetch-heavy path
(+4.03% in the same run), not to the kernel the product path ships. Decode now has
**2.63× headroom**, and C2's arithmetic reads ≈80.9% of the frame with ≈11.9 ms
left for the mixer and feeder.

**6b-2 then re-read that same product path through the real packer and a real
manifest (2026-11-11): 396,066 mean / 398,255 worst c/frame = 37.765% / 37.974% of
the 32,768 Hz frame**, GBA 001 and GBA SP, identical, PCM pin matched, with the
`TableCountKernel` control re-read on the same image at ≈154.7% — i.e. the kernel
is worth about 4× on the product path, and the walk/manifest work cost nothing
measurable.** The shape of the arithmetic did not move: decode carries ~2.6×
headroom and it is the **margin** that decides C2, whose convert/push inputs are
still stale 10,512 Hz pre-kernel mGBA numbers — which is why item 7-1 is a
measurement step and not a spreadsheet edit.

What has *not* changed across any of those rows: **no image in this repository
makes sound above 10,512 Hz.** At the product rate every `verdict:` line in every
arm prints OVER BUDGET, because there is no 32,768 Hz playback path at all — the
only rig that plays is the throwaway spike. *(True as written at the time of this
record, and now narrowed: 6b-3 landed at the headless level on 2026-11-11, and
`product-play.gba` runs the whole product path — decode, stereo convert, ping-pong
FIFO, a hand-built timer/DMA/IRQ feeder — at 32,768 Hz with `underflows=0`,
`571/571` periods serviced and the PCM pin matched, in **mGBA only**. So "makes sound
above 10,512 Hz" is still true of hardware, and "there is no 32,768 Hz playback path"
is not: there is one, and it has never been on silicon.)* That was item **6b-3** — now
landed at the headless level — and C2 is
claimed only at item **7-3**: 2048-sample frames at 32,768 Hz, FIXED-0, both
units, two runs each, underruns counted and equal to zero — and, since Q14 was ruled on
2026-11-11, at a rate the **stream declares** as well as a clock that plays it. The asset
in `assets/` does not declare it, so 7-0 owns re-encoding it and the 32,768 Hz image 7-3
will be judged on is not the one 6b-3 built. *(Updated 2026-11-13, when 7-0 landed: the
asset in `assets/` **does** declare 32,768 Hz — 160 frames, 326,144 samples, 882,949 B, its
last frame a 512-sample short tail — and the image 7-3 will be judged on is that one. The
sentence above stands as the state until it did; what still stands between this tree and C2
is silicon, which is 7-1's.)* And nothing built at
`main` after that PR has been run on hardware or on mGBA at all; the re-read table
in [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) ("Re-read at
HEAD") is the bridge from those rows to the current tree. *(Stale as a description of
state: the images this tree builds have been run — the product path on GBA 001 and a
GBA SP at 6b-1/6b-2, and the 6b-3 pair in mGBA only; every such run is a row in
[`flac-docs/measurements.md`](flac-docs/measurements.md). What survives is the
warning: a *later* HEAD owes its own re-read rather than inheriting one.)*

**What that path plays at is fixed by the hardware and the library, not chosen here, and
it is 8-bit.** The GBA's two direct-sound channels are 8-bit signed FIFO channels fed by
DMA; the only freedom is the rate the refill timer runs at, and `agb` 0.25 narrows even
that to three: `Frequency::{Hz10512, Hz18157, Hz32768}`. So the (rate, depth) grid this
product can occupy is a handful of points, and the one item 7 is built for is
**32,768 Hz / 8-bit** — which is why `playback::convert_frame` ends in `fifo_byte`'s
`(sample >> 8) as u8` (agb's own WAV-converter and mixer convention, two's complement),
why `SOUNDBIAS` is left in agb's 8-bit / 65,536 Hz PWM mode, and why `geometry`'s chunk
is 560 **bytes** per channel = 140 DMA words. Consequence for reading every pin in this
file: the 16-bit digests (`fnv_pcm`, `gafp_pin::FNV_PCM`, `fold_i16le_stereo`) are
**decode** witnesses and say nothing about what the speaker plays. The product-play image
now folds the 8-bit FIFO bytes too: `gafp_pin::FNV_FIFO_L/R`, over the converted stream,
in its post-audio proof pass, gated as `fifo` in the verdict. Folding the same bytes on the
push path was tried and measured first — the house FNV-1a is 64-bit and ARM7TDMI has no
64-bit multiply, so 4,096 FIFO bytes/frame costs ≈800,000 cycles and the run failed at
128.178% work/frame (`measurements.md`, the 6b-3 played-form row). The residual gap is
stated: this is the converter's output for the same frames, not a capture of the DMA/DAC,
and `RingFeed`'s byte copy is the host-witnessed bridge in between.
This is a property of the target, not an open question — no pitch or timbre claim in this
file is about depth, and the ~48 dB that 8-bit truncation implies (with no dither, because
none is specified) is the product's floor rather than a defect to be fixed at item 7.

**One question the rate mapping raised was measured, and then ruled (Q14, 2026-11-11).**
The asset shipped then was a **32,000 Hz** stream consumed by a **32,768 Hz** clock, so
playback ran **1.024× fast, 41.06 cents sharp** (7-0 landed the re-encode on 2026-11-13, so
the stream and the clock are one rate now; the measurements below stand, and the record of
what moved is at the end of item 7 in
[`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md)). The cost half of that question measured as free:
a stream that declares 32,768 Hz is representable two ways (there is no 4-bit rate code
for it), `flac-lite` plays one to a bit-identical PCM with the same flow-control floor,
the two product images' cycle counts do not separate from the shipped asset's at the
instrument's floor, and the 2.4% deadline tightening prices nothing at current cost
levels because the frame deadline is the player's clock either way.

**The ruling, same day, by the project owner: a stream is encoded at the rate it will be
played at, and the player does not reinterpret a declared rate to fit the clock.** The 41
cents is not acceptable in itself — zero underflows, a matched PCM digest and a 571/571
period audit do not buy it back — and the rule is general, which also rules out the
nearest-programmable-clock route (a smaller reinterpretation is still one). So: the
product asset moves to a resample-at-32,768 encode at the product profile (item **7-0**,
which now comes **before** 7-1), the `32_000 → 32_768` bridge in
`examples/flac_integration/src/geometry.rs` (`RATE_BRIDGE`, and the `playback_rate_for_stream`
arm that reads it) is marked **`TODO(Q14, BLOCKING)`** as the
measurement bridge it always was, and the guard against drifting back is a test —
`ruled_rate_policy_allows_one_bridge_and_no_more` — that computes the cents rather than
quoting them and refuses any further reinterpretation. Two things stay open and both are
work: the measured row at the key the encode profile asks for, `(32768, 2048, fixed, 0)`,
whose proper instrument is hardware (**7-1**), and a **pitch witness** — a tone fixture,
which does not exist and which belongs with 7-0, because no gate in this repository can
currently see a pitch shift at all: the pinned PCM digest of a correctly-encoded asset and
of a reinterpreted one is the *same bytes*, and that is exactly how 41 cents cleared every
gate this asset has been through. The numbers are the 6b-3 rate row in
[`flac-docs/measurements.md`](flac-docs/measurements.md). *(Closed on the second half,
2026-11-13: the pitch witness exists, and landed with 7-0 — an integer matched filter over
the decoded samples with two ratios thresholded rather than one, naming the clock the samples
were made at from its period: 410:1 on a clean design term, 340:1 on the shipped asset,
66:1/20:1 on its played 8-bit form, and 93.7579 LSB measured through the real 8-bit converter
against the exact 93.7500 — the truncation costs amplitude and none of the pitch. Its negative
leg is a re-synthesis of the 32,000-era design plus a byte-level respell of the shipped blob
(identical samples, refused by the profile); the planned "pack it as a 32,768 asset" form was
run as a mutation, and it fails by naming 32,000. The guard named above is now
`ruled_rate_policy_allows_no_reinterpretation_of_a_declared_rate`. The first half stays open,
and its key is now structural — `fixed0` at 2048-sample frames, the rate a validity check —
which is why the packer prints a provenance warning until 7-1's hardware figure exists on
7-0's frames.)*

**Measuring at 32,000 Hz was a mistake, and it is named here as one.** The *value* is fine:
32,000 is one of FLAC's four-bit table rates, and 6a-7b's hardware rows were honestly
taken on an encode at it — nobody can un-measure them, and `crates/flac-lite` has to know
that code forever. What was wrong is making that rate the target the fixtures, the packer's
default profile and the only measured decode-cost rows were all built at, while the only
clock in the plan is 32,768 Hz: `scripts/gafp_pack.py` keyed its cost table by the *stream*
rate and took the frame budget from the *playback* rate. One table, two rates, and a
fixture forced to match the half that appears in the key — which is what made
`32_000 → 32_768` load-bearing instead of absurd. A rate no piece of this project has ever
played audio at ended up owning the product's only measured cost figures.

**The failure mode being ruled out is the worst one this project has: audio coming back
resampled or pitch-shifted away from what was encoded.** For an effort whose entire point is
playing audio back correctly, 1.024× / **41.06 cents** is a correctness failure, not a
matter of taste — and it is the one failure mode none of C2's other clauses can see, because
a pinned PCM digest is a function of the samples, and the samples are identical whether they
were authored for 32,000 Hz or for 32,768 Hz. A run may service 571 hardware periods,
push and consume every sample and match its pin, and still come back 41 cents sharp. That is
not a hypothetical: it is this asset, at every gate it has been through.

**The fix, and what makes it stick.** One clock end to end: a stream is authored, encoded and
declared at the rate the timer will run at, and the sample rate stops being a lookup
dimension anywhere in the toolchain. `gafp_pack.py`'s `MEASURED_DECODE` gets re-keyed onto
what decode cost actually depends on — blocksize, predictor kind and order, the residual
envelope — which the rate probe measured directly rather than argued: relabelling 32,000 Hz
to 32,768 Hz moved decode-only cost by **−20 c/frame**, the instrument's own floor. The rate
becomes a *validity check* on the manifest instead, and that check belongs in
`crates/flac-lite`'s `EncodeProfile`, which today guards blocksize, FIXED order, full LPC
and bits-per-sample and has **no rate field at all** — the gap that let a mapping exist
where a refusal should have been. The invariant is not new to this repository: the rate
harness has held it throughout (`examples/flac_spike/src/rate_main.rs:27`, "encode rate ==
playback rate per FRAME"; `scripts/frame_vectors.py:1930-1931`, a fixture outside that
domain is refused outright), so 7-0 is porting an existing rule into the product path, and
the two tables — `examples/flac_integration/src/geometry.rs::RATE_BRIDGE`
(the `32_000` bridge, which `playback_rate_for_stream` reads) and
`scripts/gafp_pack.py::PLAYBACK_RATE_FOR_STREAM` — leave with it,
identity-or-refusal behind. And because nothing
in the present gate set can see a pitch shift, 7-0's witness measures pitch: the implied Hz
of the *decoded* PCM, by integer autocorrelation at integer lags with no float anywhere,
plus a negative leg that authors a tone at 32,000 Hz and packs it as a 32,768 Hz asset, so a
reinterpretation arrives red instead of hiding behind a matched digest. Two traps come with
the retarget and are written up in
[`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) under "Q14 follow-up", which
also carries the full inventory of where 32,000 legitimately belongs — the spec rate table,
ordinary codec test data, and the historical rows, all of which stay: the fixture generator
specifies its tones in sample *periods*, so re-synthesising "the same" fixture at 32,768 Hz
leaves the played pitch exactly where the defect is; and 32,768/32,000 is exactly 128/125,
so an exact resample exists, but ffmpeg's `-ar` is float and float would break every byte
pin in the repository — the same class of defect as the 6a-2 float-trig incident.

*(Landed 2026-11-13. Three details above describe what was planned rather than what shipped,
so they are marked where they matter: the cost key is **blocksize and predictor kind/order**,
with the residual envelope carried as a stated caveat on each row rather than as a key
component; the `EncodeProfile` rate field that "does not exist at all" now exists, is one
element wide, and is enforced both by the library facade and by the product ROM against its
own shipped blob; and the packed negative leg above cannot be a committed asset, because the
packer now refuses to make such a stream at all — that witness exists as a re-synthesised
32,000-era fixture, a byte-level respell of the shipped blob (same samples, refused by the
profile) and a mutation left running through the whole generation chain. What the plan
expected to be an `--allow-unmeasured` pack came out **clean** instead, because the census did
not change and a structurally-keyed table has no way to notice the frames are new ones; the
honest signal moved to a provenance warning on the pack, and 7-1's measured row on 7-0's
frames is what clears it. The plan-of-record entry says all of this at length.)*

**6a-7b measured the instrument on silicon: GBA 001 and GBA SP, two runs each, all four
runs identical to the cycle.** Two results. The decode path is
**bit-exact on hardware** — the blue `harness_ok` backdrop means both PCM pins, the
frame-by-frame equality witness over `ctrl`/`wide`/`asm` and calibration all passed
on the ARM7TDMI, the first such claim in this project. And the timing **missed the
pre-declared ±2% bar on all five quantities**: hardware FIXED `asm` is **445,558
c/frame = 42.48% mean / 42.72% worst** of the 32,768 Hz frame, **+6.89%** over
mGBA, so **39.74% is retired as the hardware number** (the fork resolves to
`diverges`). Every leg is 3.7–6.9% above the model, the stack +6.6% on both clips,
and the width marginal moves in opposite directions per clip — so it is not a clock
or calibration error, and the candidates (mGBA's ROM data-read model, its
prefetch/data-read interaction, the flashcart) are named but not separated. Decode
still has **2.34× headroom** at full rate, so the rate decision is untouched. The
same runs found, and we fixed, a display bug no headless emulator could see: a
trailing `frame.commit()` blanked the screen it had just painted.

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
(`TableCountKernel`) is untouched at 167% and no product path names `rice-arm` *(true
as written at the time of this record, and superseded since: 6b-1 put `rice-arm` in
the product ROM, and the product-path hardware rows — 37.83%/38.04% at 6b-1,
37.765%/37.974% at 6b-2 pt 2 — are in "Where performance stands" below)*; and
**E5 — hardware arbitration — is now measured**: the ROM→IWRAM fetch pricing is
hardware, not mGBA's model (GBA 001 and GBA SP reproduce mGBA within 2
cycles/call; ratio 3.94). The full real-time claim still needs the kernel-level
stack and the convert/push stages on hardware.
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
| `crates/flac-lite` | Portable core: `no_std`, zero deps, zero alloc, `#![deny(unsafe_code)]` plus named `cfg`-gated `.iwram` pin attributes (exactly one in a default build). Frame decode is **complete** (`bits` → `stream` → `frame` → `subframe` → `residual` → `stereo`), witnessed bit-exact against `flac -d` on host and in the ROM image. Per-frame `driver`, table-less `scan` walk (feature), CRC verification (feature), `RiceKernel` seam. Arithmetic width (6a-6): `i32-integrator` on by default, guarded per subframe, `i64` reference/escape leg (`make flac-test-wide`). Facade landed with 6b-1: `decoder::Decoder`, `format::Manifest`, `Blocksize::samples`, `EncodeProfile::validate`; no `todo!()` remains in `src/`. |
| `crates/flac-lite-gba` | The GBA overlay. Two `RiceKernel` arms behind their own features: `rice-twin` (6a-3, Rust, forwards to the core's twin) and **`rice-arm`** (6a-4: the E10 ARM-mode kernel via `global_asm!`, its FFI and `KernelAbi`, three tables under `iwram-pin`). A **default build still contains no kernel, no asm, no FFI and no `unsafe`**; the shipped default is still `TableCountKernel`. |
| `examples/flac_spike` (6a-5 / 6a-7 arms) | Three feature-gated product-path arms (`item6a5-rust`, `item6a5-asm`, `item6a5-arms`), plus 6a-7's **vendored** width leg (`item6a7-wide`, `src/wide.rs`, the pre-6a-6 i64 path) timed in one image with `ctrl` and `asm`. Under `item6a7-wide` the image also renders 6a-7b's ten-line on-screen summary (`src/summary.rs` + `src/item6a7b.rs`): `ctrl`/`wide`/`asm` means, `S=C-A`, `Wd=W-C`, `X=worst%` per clip. `make item6a5-test`, `item6a7-test`, `item6a5-arms-rom`, `item6a5-asm-rom`, `item6a7-rom`. |
| `examples/e5_fetch` | E5's hardware arbitration ROM: one synthetic ARM instruction stream assembled into `.text` and `.iwram`, timed in one image with `WAITCNT`/prefetch on/off; results on screen (no serial on hardware) and on mGBA serial. `make native-e5-fetch-rom`. |
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
**1,048,750 cycles/frame ≈ 512 cycles/sample** — that is the instrument's
rounded-clock budget (`stats.rs::GBA_CLOCK_HZ = 16_780_000`), which every
`verdict:` line and published share in this document and the ledgers divides by;
on the GBA's exact 2^24 Hz clock the same frame period is 1,048,576 cycles =
exactly 512 c/sample, 0.0166% tighter, which changes no verdict here but is why a
share quoted to three decimals needs its denominator named
([`flac-docs/method-and-gates.md`](flac-docs/method-and-gates.md), "The real-time
budget")). Decode cost is paid per sample; the budget is granted per second —
which is why lowering the sample rate is a lever at all.

| Path | FIXED mean | LPC mean | Landed? |
|---|---|---|---|
| Phase-1 baseline | 220.7% | 658.0% | superseded |
| E2a + E2b landed (today's shipped path) | 170.2% | 589.5% | **yes** |
| + E10 ARM kernel (probe) | 101.98% (worst 102.50%) | 521.6% | no |
| + E10 + E4 i32 stacked (probe) | **94.47% (worst 94.95%)** | 216.3% | no |
| Kernel + IWRAM + i32 (6a-7, mGBA) | 39.74% (worst 39.96%) | 166.97% | no — instrument only |
| **Kernel + IWRAM + i32 on hardware (6a-7b, GBA 001/SP)** | **42.48% (worst 42.72%)** | **173.23% (worst 174.14%)** | **measured, decode only** |
| **Product path on hardware (6b-1, GBA 001/SP, `rice-arm`)** | **37.83% (worst 38.04%)** | — (kernel leg, FIXED clip) | **measured, product path** |
| Product path on hardware, default kernel (6b-1 control) | 154.67% (worst 155.45%) | — | measured; this is the shipped default path |
| **Product path on hardware through the real packer + real manifest (6b-2 pt 2, GBA 001/SP, `rice-arm`)** | **37.765% (worst 37.974%)** | — (kernel leg, FIXED-0 clip) | **measured, product path — the row of record**; belongs to `1e5974c`'s bytes, `main`'s images (+32 B) have never been run |

At 10,512 Hz, real music, on today's path: **FIXED fits** (73.9% mean, 78.3%
worst) and **LPC does not** (183.9% / 211.6%). Full-rate FIXED first came in budget
as E9's stacked probe; it now does so on hardware, at 42.72% worst (6a-7b) — which
is exactly what item 6 existed
to land, measure on the shipped path, and then arbitrate on hardware.

**No full GBA-hardware real-time claim exists.** E5 has now confirmed the fetch
model on two units (below), but the kernel-level stack and the convert/push
stages are still mGBA-only.

Which numbers may be quoted, how eras and denominators differ, and the full
ledger: [`flac-docs/measurements.md`](flac-docs/measurements.md).

## Retarget status (2026-11-13): 16,384 Hz, FIXED-0, green on both units

The owner ruled Q16 as the product needs it — **FIXED-0 is the product, LPC is
measurement-only** — and the 16,384 Hz retarget has landed in the tree. The
product asset is now 80 frames / 162,304 samples / 439,396 B, declares 16,384 Hz
on every frame, and the geometry is `1,024` ticks/sample, `2,097,152` cycles per
2048-sample frame, `573,440` cycles per period; `CHUNK`, ring, prefill and the
16-sample grace are unchanged. The packer, profile, fixtures, vectors and the
spike's budget moved with it. A headless mGBA run of `product-play.gba`
(`ef1ed8ba…`) is green — 289/289 periods, `und 0`, `late 0`, PCM/FIFO pins
matched, `work 25.363%`, worst frame `26.583%`, attributed decode `417,431
c/frame` — and `make product-play-test` is 14/14. The full landing record,
including the provenance warning that still prints for the inherited FIXED-0
cost row, is in [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md)
under "7-1 retarget landing".

**7-1's retarget hardware reading then came back green on both units, twice each:
BLUE screen, `fr 80 und 0`, `per 289 ser 289`, `replay 0 late 0 fed 0 drain 0`,
`late in push 0/0`, exact sample counts and digests, and no rate/pitch row.**
Hardware work is **28.450%** / worst frame **29.613%**, decode window **19.961%**,
attributed decode **446,306 c/frame**, convert 6.824%, push 0.666%, feeder 0.997%,
minimum lead 9,120 samples (0.557 s). The hardware-versus-mGBA gap is the same
leg-shaped result measured at 32,768 Hz: decode +6.87%, convert **+35.08%**, work
+12.17% — Q15 re-confirmed, not retired. The four screens are transcribed in
[`flac-docs/playback-results.md`](flac-docs/playback-results.md) under "7-1,
retarget hardware reading".

**7-2 is now closed on hardware.** The chunk-sized push — one critical section
per 560-sample slice, about 3.92 samples / 4.08× margin — was run on GBA 001 ×2
and GBA SP ×2, and all four runs are green: `fr 80 und 0`, `per 289 ser 289`,
`replay 0 late 0 fed 0 drain 0`, `late in push 0/0`, `2nd src 0/289`,
`disp 000A`, exact sample counts and digests. Work is **28.600%** / worst frame
**29.781%**, `dec 19.961%`, **`attr 446292`**, `conv 6.825%`, `push 0.816%`,
`feed 0.997%`, `minlead 9120`. Against the whole-frame-push image the cost is
+0.150 pp work and +0.150 pp push; decode/convert are unchanged. The four screens
are in [`flac-docs/playback-results.md`](flac-docs/playback-results.md) under
"7-2: chunked push on hardware".

This is **still not C2**: 7-3's listening witness — pitch and tempo, heard on
both units with the source beside it — is the remaining acceptance half. **The
current product asset cannot supply it**: it is the synthetic encoder fixture,
about -37 dBFS, and its 8-bit FIFO form is RMS 1.9 LSB. The master
`bioluminescent.wav` is real programme material and ~21 dB louder, but at the
product profile its first excerpt censuses CONSTANT/FIXED-1..3, which the packer
refuses because only FIXED-0 is measured. The 7-3 asset therefore needs a profile
decision and new measured rows, not just a file swap; see
[`flac-docs/playback-results.md`](flac-docs/playback-results.md) under "7-3 asset
finding".

## What is next

One line per rung; the full plan, bars and invariants are in
[`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md).

**The live order (2026-11-10), each step against C1/C2** — 6b-1 and both halves of
6b-2 have since landed (2026-11-10 / 2026-11-11); the ordering rule below is
unchanged, and the per-rung state lives in the plan of record:

1. **6b-1** *(landed 2026-11-10)* — facade onto `scan::Frames` **and** `rice-arm` carried into a product
   ROM (the placement 6a-5 assigned here). Serves C1 and C2's decode half.
2. **6b-2** *(landed 2026-11-11, both halves)* — the real GAFP packer, `format::Manifest`, and a pack-time profile that
   refuses LPC at the target rate (174.14% of a 32 kHz frame on hardware).
3. ~~**6b-3** — rate-play at 32,768 Hz through a real manifest; the first product ROM
   naming the overlay crate.~~ *(Landed 2026-11-11 **at the headless level only**,
   as landed: `product-play.gba` plus a host model of the same geometry — 571/571
   periods serviced, zero underflows, PCM pin matched, in mGBA. What it does not close:
   hardware, which is 7-1, and whether its hand-built timer/DMA/IRQ feeder is the
   shipping path, which is 7-2. The question that second one would have owned — Q14, the
   41-cent pitch shift — was ruled the same day this landed, and its answer changed what
   this item is: the asset it plays declares 32,000 Hz, so it is measurement scaffolding
   and not a product asset. *(7-0 landed on 2026-11-13 and it no longer does.)*
)
4. ✅ **7-0** (landed 2026-11-13), then **7-1 / 7-2 / 7-3** — re-encode the product asset at
   32,768 Hz, the rate it is played at (Q14, ruled 2026-11-11), with a pitch witness in the
   same PR; then re-measure the producer at 32 kHz on hardware against *that* asset, land the
   shipping mixer/DMA path, and run the acceptance test: underruns = 0 on both units.
   *(Three shape details of 7-0 differ from this line, and all three are recorded in the 7-0
   entry at the end of item 7 in
   [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md): cost is keyed by blocksize
   and predictor kind/order with the residual envelope carried as a caveat rather than as a
   key component; the asset's census did not change, so it packs **cleanly** against the
   inherited hardware row and the honest signal became a provenance warning rather than the
   predicted `--allow-unmeasured`; and the packed negative leg exists as a mutation plus a
   byte-level respell, because after the ruling the packer refuses to make such an asset at
   all. 7-1's figure — measured on the frames this asset is made of — is what clears it.)*
   **7-1 is in progress: its first reading landed the same day 7-0 did** (2026-11-13, four
   runs on both units, one image `8113f04c…84cd`). What those runs established and what they
   did not, with the stage shares and the period-22 analysis, is recorded under 7-1's first
   reading in [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) and dated in
   [`flac-docs/playback-results.md`](flac-docs/playback-results.md). What 7-1 still owes:
   the packer's `fixed0 @ b2048` row (which needs an attributed, isolated decode figure from a
   current image — the screen now shows that as `attr`, so the row is one transcribed reading
   away, not one more instrument away), the host model's envelope re-derived from a
   pinned ROM-generated fixture instead of a hand-copied pre-7-0 run, and the listening
   witness. **And it opened the decision the owner has now taken.** 7-1's second reading (four more
   runs, both units, image `86f6a1dd7e525dfc…`) came back **`late in push 1/1` on all four** —
   the latency defect's mechanism, confirmed on silicon, with the VBlank/second-source candidate
   excluded by `2nd src 0/582` and a display status that never asked for that interrupt — and it
   came back at **work 59.946% of the frame, worst frame 60.755%**, on a FIXED-0 stream, with
   **`attr 451763 c/frame`**: the first attributed, isolated decode figure ever read off a unit.
   Explaining the one refusal did not change the cost of the run, and nothing in this tree
   recovers the factor of two that predicted frames would need at this clock. **So the owner
   ruled, 2026-11-13: drop the sample rate, and relax C2 from real-time playback specifically at
   ~32 kHz to real-time playback at the highest rate achievable.** The ruling, its arithmetic and
   what it pointedly does *not* relax (every audit clause, Q14's identity rule, both units twice
   each, the listening witness) are in
   [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) under "The rate ruling".
   **The rung is then named: 16,384 Hz** (`2^24/1024` — exactly expressible, so Q14's identity rule
   carries across the retarget with nothing relaxed). It is deliberately **not** agb's 18,157: no
   integer reload of a 2^24 Hz timer produces that rate (924 ticks plays 18,157.16 Hz), and agb's
   menu carries no authority here because **this ROM does not use agb's mixer** — the product ROM
   states its absence as a safety precondition. An interim instruction to relax exactness and go to
   ~18 kHz was considered and withdrawn on exactly those grounds, with the latency arithmetic that
   settled it in [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) → "The rung is
   chosen". "Highest achievable" still needs a margin bar to be judgeable, which is **Q16**; and
   because LPC projects to 86.6% of a 16,384 Hz frame, **8,192 Hz stays the only rung where
   predicted frames are plausible** — so whether 16,384 Hz is the ceiling or a way-station is that
   bar's answer. 7-0 and 7-1 were re-run at 16,384 Hz: the retarget hardware reading is
   green on both units, twice each (`fr 80 und 0`, `per 289 ser 289`, `late 0`, `late in
   push 0/0`, work 28.450% / worst 29.613%, `attr 446306`), recorded in
   [`flac-docs/playback-results.md`](flac-docs/playback-results.md). What 7-1 still owes is
   narrower: the packer's `fixed0 @ b2048` row transcribed from these bytes, and the host
   model's envelope pinned from a ROM-generated fixture rather than hand-copied from a run.
   The chunked push has since landed and closed 7-2 on hardware: four runs on both units,
   zero late services, `late in push 0/0`, work 28.600% / worst 29.781% (the expected
   +0.150 pp price of four sections). 7-3's listening witness is the remaining acceptance
   half.
   **C2 is met at 7-3**, and C2 says in terms that the stream declares the rate it is played
   at — a clause that survives the rate change, and indeed matters more for it: a rate chosen by
   measurement is exactly where a silent 1.024x reinterpretation would creep back in.
5. **Parked:** 6a-8. Q13's data-read probe was re-opened on 2026-11-13 as **Q15**, and Q15's
   scope has since narrowed twice: it no longer explains the period-22 late service (7-1's second
   reading attributed that to the producer's own interrupt-masked ring push), and it no longer
   pairs with the rate question, because the rate was **ruled** the same day. What it still owns
   is the **convert leg's +35.1%**, which is an unpriced leg in the budget of whatever rate
   ships, and the single-variable IWRAM-versus-EWRAM ring control is still the probe that puts a
   number on it rather than a name on it. **New: Q16** — the margin bar that makes the amended
   C2's "highest rate achievable" checkable, which is the owner's to state and the thing standing
   between this tree and a retarget that means anything.

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
- ~~**E5** — hardware arbitration of the ROM→IWRAM fetch ratio.~~ **Measured.**
  The same ARM stream in `.text` and `.iwram`, timed on a GBA 001 and a GBA SP
  (two runs each): **ROM 4.085 / IWRAM 1.037 cycles/add, ratio 3.939**;
  hardware reproduces mGBA within 2 cycles/call. Prefetch on/off changes nothing
  measurable. The fetch pricing is hardware; the kernel-level stack on hardware
  is **6a-7b** (next rung).
- ~~**6a-7b** — the kernel stack on hardware.~~ **Measured, and it missed the bar.**
  GBA 001 and GBA SP, two runs each, four runs identical to the cycle. Correctness:
  **bit-exact on silicon**. Timing: hardware FIXED `asm` **445,558 c/frame =
  42.48% mean / 42.72% worst**, +6.89% over mGBA; stack 1,327,714 (+6.56%); widths
  73,654 (FIXED, −18.1%) and 3,885,745 (LPC, +8.4%) — **all five quantities outside
  ±2%**, so 39.74% is retired and the fork resolves to `diverges`. Decode keeps
  2.34× headroom, so the rate decision stands. Record:
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md).
- **Data-read arbitration — parked 2026-11-10, not dropped.** E5's shape one layer
  down: the same `ldr` stream from cartridge ROM and from IWRAM, timed on both units
  with prefetch on and off, to separate mGBA's ROM data-read model from the
  flashcart's read timing. It owns the 3.7–6.9%, but it cannot move C1 or C2 — the
  decode figure is already a hardware measurement and the acceptance run is on
  hardware — so it waits. Q13 keeps the trigger; a **+7% derating** on mGBA
  memory-heavy figures stands in for it meanwhile.
- ~~**6b-1** — the facade onto `scan`, and `rice-arm` carried into a product ROM.~~
  **Landed 2026-11-10.** `Decoder`/`FrameBuffers`/`DecodedFrame` exist as a shell
  over the walk; `examples/flac_integration` is the product ROM and decodes the
  embedded clip with the ARM kernel at **396,659 c/frame = 37.82% mean / 38.03%
  worst** of the 32,768 Hz frame, pin matched (mGBA). The `scan` seam is
  **byte-identical** in that image, so the kernel's generic parameter costs
  nothing. `driver` is demoted behind `instrument` + `doc(hidden)`. Owed: the
  unit reading of both A/B builds. Record:
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md).
- ~~**6b-2** — the real GAFP packer, `format::Manifest` at pack time, and a profile
  that refuses at pack what the target rate cannot play (LPC at 32 kHz).~~
  **Landed 2026-11-11, both halves.** First half: `scripts/gafp_pack.py` is the
  writer — frame region byte-identical to what libFLAC produced, offset table
  re-derived by the walk that reads every frame, and a profile guard that refuses
  before it writes (LPC at 32 kHz, off-target rate, 8-bit), gated by
  `make gafp-pack-check` against the committed product asset byte for byte. Second
  half: the seek table read from real files, the fixed-block number line, and
  `Frames::anchored(n)` to start a walk at a seek point — and the product path
  re-read through that manifest at **396,066 / 398,255 c/frame = 37.765% mean /
  37.974% worst** on both units, identical, which is the row of record. Owed by
  that half and named in the plan: the fixture axes `tests/README.md` lists, the
  cost axes the packer refuses rather than prices (FIXED-1..4, wider residual
  shapes), and the written shape of `gen_gafp_blob.py`'s retirement.
- ~~**6b-3** — rate-play at 32,768 Hz through a real manifest: the first product ROM
  that makes sound at the target rate, driven by the manifest rather than the
  spike's generated offset table, and the thing item 7-1's stage timings have to be
  taken on. Nothing above 10,512 Hz has ever made sound in this repository.~~
  *(Landed 2026-11-11 at the headless level. The image runs the target
  rate and the whole product path — decode, convert, FIFO, a hand-built feeder — with
  zero underflows and the PCM pin matched, but only ever in mGBA: nothing above 10,512
  Hz has ever been* heard *in this repository, and the plan's record of 6b-3 says so in
  those terms. It carries a second and sharper limit now: the asset it plays declares
  32,000 Hz, and Q14 — ruled the same day — is that a stream is encoded at the rate it is
  played at, so this image is measurement scaffolding and **7-0** owns the asset. Item 7-1
  takes its stage timings on that image, not this one; 7-2 owns
  whether this feeder is the shipping path. Numbers: the 6b-3 rows in
  [`flac-docs/measurements.md`](flac-docs/measurements.md). *(7-0 landed on 2026-11-13, so
  the scaffolding is now a real product asset — 160 frames, 326,144 samples, 582/582 periods,
  both digests matched, `work 53.446% / dec 39.574%` against the control image's 53.386 /
  39.527. 7-1 takes its timings on **this** image. Its record is the 7-0 entry in
  [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) and the 7-0 row in
  [`flac-docs/measurements.md`](flac-docs/measurements.md).)*
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
| [`flac-docs/plan-of-record.md`](flac-docs/plan-of-record.md) | The only live plan **for the FLAC track** (its scope block names what it governs and what it explicitly does not): 6a-4…6a-8, 6b, the width decision, the rate ruling, invariants, baselines, the landing record, ordering rules. |
| [`flac-docs/open-questions.md`](flac-docs/open-questions.md) | The sixteen questions a future session still owes an answer, **of the FLAC track**, each naming the record that opened it and the rung that owns it. |
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
| Split-ladder item 6; 6a-1/2/3; toolchain pin; width decision; 6b; 6b-4; Q14 / 7-0 | [`plan-of-record.md`](flac-docs/plan-of-record.md); measurements in [`measurements.md`](flac-docs/measurements.md); toolchain rule in [`method-and-gates.md`](flac-docs/method-and-gates.md) |
| Recording Q&A for a future session | [`open-questions.md`](flac-docs/open-questions.md) |

## Standing caveats

- Every `podman-*` target has **never been run** in this harness (rootless podman
  cannot start here: no `newuidmap`, no podman machine, no nested virtualization).
- `make test-rom` runs and collects **zero** tests. The repo's first ROM suite
  with assertions in it is 6a-4's (`make overlay-kernel-test`, 8 cases); the root
  crate's suite is still empty.
- `flac-portable-check` is a compile gate: no host-side performance claim exists.
- E5 is measured for the fetch ratio (hardware confirms mGBA to ≤0.05%); the
  kernel-level stack and the convert/push stages on hardware are still open, so
  no full real-time claim exists.
- The `Rice` corpus behind the differentials has 0 escaped partitions, no
  `u32::MAX` fold and no codeword that overflows the guard — the mutation arms are
  green *correctly*. See [`method-and-gates.md`](flac-docs/method-and-gates.md).
