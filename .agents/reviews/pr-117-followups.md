# PR #117 review follow-ups (agent work note)

Owner feedback on the code-review issues for `feat/flac-6b3-product-play`, kept here so
steps 2 and 3 can be picked up without re-deriving the decisions. This is an agent work
note, not a product record; the dated measurements belong in `flac-docs/`.

## Decisions

| # | Issue | Owner decision | State |
|---|---|---|---|
| 1 | Verdict ignores the period audit | Fix in this PR | **Done in step 1** |
| 2 | No FIFO/played-bytes witness (`pcm MATCH` is decode-only) | Fix in this PR | **Done in step 2** |
| 3 | Clock-refusal path re-enters `STATS` and panics | Fix in this PR | **Done in step 1** (and widened to the clock read itself — see below) |
| 4 | ARM-mode policy not host-testable | **Deferred.** Owner: the ARM-mode optimizations being host-untestable by design is fine; e2e is the intended witness. Do not extract the policy for test coverage. | Closed unless a concrete bug needs it |
| 5 | Q14 guard is a sampled list / no Python guard | **Docs-only for now.** Clarify the two rate sets (agb's playback rates vs declarable stream rates) and describe the cheap exhaustive sweep; do not add it. | **Done in step 3 (docs)** |
| 6 | Host model has no variance/burst test | Add the test | **Done in step 3** |
| 7 | Screen does not flag the scaffolding | Add it | **Done in step 1** (`rate 32000>32768 scaffold`, `Q14 SCAFFOLD` log) |
| 8 | Smaller fixes (`finished` ordering, IRQ IF, units, `Stage` void mean, bounds, `convert_frame` length, cosmetics) | Address | **Done** (ordering in step 1; the rest in step 3) |
| 9 | The PR's own report: a stale host-side cost envelope, a mis-described Q14 guard, and three arithmetic slips in the prose | Fix in this PR where it is report/constant hygiene; the two structural fixes (envelope as a pinned fixture, the sound-DMA/FIFO re-arm mechanism) belong to 7-1/7-2 | **Done in step 4** |

## Step 1 — verdict integrity (working tree, 2026-11-12)

Code changes in `examples/flac_integration/src/product_play_main.rs`:

- The period audit is **part of the verdict**: `coalesced == 0`, `late_servicings == 0`,
  `clock_refusals == 0`, `tail_periods <= 1` and every `Stage::void == 0` must hold;
  `over_period` and `min_lead` stay advisory.
- Clock refusals are counted in their own `Mutex` (not `STATS`), so the feeder's
  end-of-handler read cannot re-enter `STATS`' `RefCell` and panic.
- `audio_cycles` takes **three accepted snapshots and returns the median** (six attempts),
  plus the per-period range checks and a monotonic guard. This is what the 2-4 void
  windows and the inflated 59.367% worst frame were: a cascaded read with TM1 in the new
  period and TM2 still in the old one (a value a period low), which the in-pair test
  accepted. The median removes one outlier; the monotonic check rejects a below-last
  median; a run that cannot get a median fails rather than silently biasing the shares.
- `STATS.finished` is set in the same critical section as the final push
  (`Producer::total_samples`), closing the window in which the feeder could see an empty
  ring with the flag still false.
- A stale Timer1 `IF` is cleared and the handler is registered **before** the audio chain
  is armed.
- Screen: `ledg ok aud ok`, `rate 32000>32768 scaffold`; log: `Q14 SCAFFOLD` and
  `clock refusals (no median of three in six attempts)`; verdict line carries `audit=`.

Verification (all from this tree):

- `make check` clean; `make product-play-test` 10/10.
- `make native-product-play-rom` → `product-play.gba` **`fe00b590c71fb28e…`, 1,084,572 B**.
- `mgba-test-runner`: three identical runs — 571/571 periods, `underflows=0`,
  `coalesced=0 late=0`, `tail_periods=1 tail_real=240`, `min_lead 9120`, PCM MATCH,
  `verdict PASSED (ledger=true, audit=true, pcm=true)`.
- Shares: work/frame **559,845 c = 53.390%**, worst frame **565,874 c = 53.965%**,
  push worst 15,831, `void=0`, refusals 0, closure `gaps=195,184` over 5,730 snapshot
  pairs. The old 53.062% / 622,517 / 59.367% / 79,922-cycle push worst are superseded;
  the 79,922 was the torn read, and the +0.328 points are the median's read cost.

Records updated: `flac-docs/measurements.md` (new verdict-integrity row + hash/reading
rules), `flac-docs/plan-of-record.md` (two dated notes), `flac-docs/open-questions.md`
(53.390%/53.062% parenthetical), `README.md`, `flac-docs/method-and-gates.md`, `FLAC.md`.

## Step 2 — FIFO / played-bytes witness (#2) — done (working tree)

- `scripts/gen_gafp_blob.py` now emits `FNV_FIFO_L` / `FNV_FIFO_R` over each channel's
  `(sample >> 8) as u8` stream, derived from the committed `spike_l0_pcm.bin`;
  `src/gafp_pin.rs` is regenerated (`L=0x7CC75F50B4DF75EC`, `R=0x916BA1AF3B1F4342`).
- The ROM folds the same converted frames in the **post-audio `proof()` pass** (which now
  returns `(pcm_ok, fifo_ok)`) and gates `fifo` in the verdict. The screen says
  `pcm ok fifo ok`; the log has a separate `fifo proof` line.
- Host test `fifo_pins_match_the_converted_stream` folds `decoded_frames()` and re-decodes
  the i16 stream to `FNV_PCM`; suite is **11 tests**.
- **The push-path fold was tried first and measured.** Folding `scratch_l/r` on every push
  put the push stage at 816,714 mean / 822,153 worst c/frame and work/frame at **128.178%**
  (154 underflows, 4 late) — the house FNV is 64-bit, ARM7TDMI has no 64-bit multiply, and
  4,096 FIFO bytes/frame costs ≈800,000 cycles. The fold therefore runs after the audio,
  where it is not timed; the audio-phase shares are unchanged (559,849 c = 53.391%,
  worst 565,939 = 53.972%).
- **Residual gap, recorded:** the FIFO pin is the converter's output for the same frames,
  not a capture of the DMA/DAC. `RingFeed::push_bytes`/`take_chunk` are the byte-witnessed
  copy in between. A true push-path witness would need a 32-bit hot-path hash and a new
  pin; not this PR.
- Verification: `make check` clean; `make product-play-test` 11/11; ROM
  `ba237039e394330f…`, 1,085,604 B; run `PASSED (ledger=true, audit=true, pcm=true,
  fifo=true)` with FIFO MATCH.

## Step 3 — verification and hygiene — done (working tree)

- **#5 docs-only rate clarification.** `geometry.rs` (`playback_rate_for_stream`),
  `scripts/gafp_pack.py` and `flac-docs/open-questions.md` now say the two sets
  explicitly: `agb` 0.25 can *play* 10,512 / 18,157 / 32,768 Hz (the product geometry is
  32,768), while a FLAC *stream* may declare any table rate or a 16-bit Hz value. The Q14
  guard is about the declarable stream rate; the exhaustive form is a cheap host sweep
  over `0..=65535` plus the table (65,536 calls). It is **described, not added** — the
  identity-or-refusal behaviour in the packer/player is 7-0's.
- **#6 model burst test.** `Settings`/`Sim` carry an optional `(index, cycles)` burst;
  `worst_frame_burst_does_not_starve` charges the guarded build's 565,939 c at every
  fed-phase frame position and asserts `underruns == 0`, the ledger closes and the lead
  stays above a period. Suite is **12 tests**.
- **#8 remaining.** `REARM_GRACE_SAMPLES` / `max_delay_samples` (samples, not ticks);
  `Stage::mean` divides by measured windows (void excluded); the wait/drain loops are
  bounded in periods instead of "until the outer timeout"; `convert_frame` release-checks
  destination lengths; the dead `let _ = stage` is gone; the armed line prints
  `58.51 periods/s`; the unused `AUDIO_BUDGET_CYCLES_PER_SAMPLE` is removed.
- **Q14 test shape.** The asset assert is an explicit `PRODUCT_STREAM_RATES` allow-list —
  the *bridge* is still in `geometry.rs`; what is absent is `32000` from the allow-list.
  Step 4 rewrote it as **one** condition (see below), so 7-0's job is to delete the arm and
  add `32_768` to the list, not to fix a red assert. *(Done as described on 2026-11-13: the
  bridge arm is gone, `RATE_BRIDGE` is `None`, and the rate clause moved up into
  `flac-lite`'s `EncodeProfile.allowed_sample_rates_hz` rather than staying a product-side
  allow-list — so the library, the packer and the ROM all ask the same question. The guard is
  now `ruled_rate_policy_allows_no_reinterpretation_of_a_declared_rate`; the suite is 14
  tests. Record: item 7's 7-0 entry in `flac-docs/plan-of-record.md`.)* The sampled refusal list is unchanged
  in step 3 (the exhaustive sweep stays docs-only, per #5); step 4 added the
  arithmetically exact non-product rates to it.
- Verification: `make check` clean; `make product-play-test` 12/12; ROM
  `694126d406eb9b2a…`, 1,085,804 B; two identical runs, `PASSED (… fifo=true)`,
  `fit: lines=9 … fits=true`; audio numbers within 1-72 cycles (work/frame 559,848 =
  53.391%, worst 565,867 = 53.965%).

## Step 4 — inspecting the PR against its own records: report hygiene (2026-11-12)

An independent pass over the PR's claims — building the image at HEAD, running it, and
re-deriving every number in the body and the records. The load-bearing claims held: the
image, the ledger, the period audit, both digests, `min_lead 9120`, work/frame 559,848 =
53.391%, worst frame 565,867, the 41.06-cent arithmetic, the geometry's 512 c/sample /
286,720 c/period / 1,048,576 c/frame / 17-periods-of-stall arithmetic, and the probe's
0 B/frame and 2 B/frame rate-spelling results all reproduced. What did not hold, and what
was done:

- **The model's cost envelope was the 2026-11-11 image's** (`555,193` raw / `549,760`
  attributed / `420,894` decode), carried through three later builds while the test named
  itself after "the run's own sums". Re-read from the image the tree builds: sums
  65,079,938 / 17,331,888 / 2,263,016 over 152 windows → **557,070 raw**, less 159
  boundaries at 5,641 c → **551,170 attributed**, decode half **422,257** — which is that
  image's own printed `isolated_estimate`, so model and ROM now derive the same number.
  The sustained ceilings quoted in `plan-of-record.md` and `geometry.rs` follow: **152.9%
  / 190.6% / 178.1%**, bound 843,237 c/frame (they are computed by
  `sustained_overage_tolerance`, not scaled). `ROM_WORST_WORK` deliberately keeps the
  larger recorded 565,939 c rather than this run's 565,867.
- **The Q14 guard was described as something it is not.** The body and the plan said the
  asset assert "goes red exactly once, when the bridge is deleted" — it does not: it is an
  allow-list, and deleting the arm leaves it green. The real tripwire is
  `.expect("the bridge is still in the tree; 7-0 removes it")`; there is no `assert_ne!`
  anywhere in the file, and the records still cited one, at line numbers this PR's own next
  commit had moved. Rewritten as one condition — `declared_allowed == !bridge_in_tree` —
  so the shipped state and the completed-7-0 state are both green and a **half**-landed 7-0
  is red; the refusal list now samples 8,192 / 16,384 / 65,536, the other rates dividing
  2^24. Records now cite symbols, not lines. *Step 5 finished what that condition was
  reaching for: the `.expect` still made the completed-7-0 state red, so the bridge is now
  a named constant (`geometry::RATE_BRIDGE`) read by both sides, and the cents are pinned
  against a `RECORDED_BRIDGE` literal.*
- **Two arithmetic slips in prose** (conclusions unaffected): decode stage shares quoted
  as 40.835% / 41.655% were 152 window means over a 157-frame budget — the same
  denominator class this branch already corrected, and not this image's figures (it prints
  39.531%); and agb's rounded reloads are +3.8 / +8.8 ppm (10,512.04 / 18,157.16 Hz), not
  "a few hundred". Stated positively in the source comments and sampled by the guard, so a
  "clean-looking rate" cannot be reached for by that property: the rates with an exact
  cycles/sample are exactly the divisors of 2^24 — in the declarable band 8,192 / 16,384 /
  32,768 / 65,536 — and the near-miss lookalikes are not exact (2^24 ÷ 36,864 = 455.11;
  448 c/sample is 37,449 Hz). "Three identical runs" in the body belongs to the
  `fe00b590…` image; the current row records two runs within 1–72 cycles, and this pass's
  run is the third that agreed.
- **Two instrument descriptions that 7-1/7-2 depend on:** `agb` 0.25's
  `enable_dma_for_sound` (`hw.rs:56-68`) writes source/destination/control and **no
  transfer count**, so the "140-word DMA transfer agb would program" framing was backwards
  — `buffer_size` = 560 is what one period consumes, and the count and the re-arm are this
  ROM's own — and there is **no `MIXER_TICK_HZ` constant** in `agb::sound::mixer` (the
  value is inlined at `hw.rs:100`). Comments now say that; the mechanism question is written
  into 7-2 with the observation that would settle it (re-arm early/late on purpose, count
  `grace`/`late`).
- Nits fixed: "the C2 clip plays 572 periods" restated as 571 full + 1 tail (and 586 for a
  same-playing-time 32,768 Hz asset), the `MIXER_TICK_HZ` symbol attributed to agb removed,
  `make test-rom` labelled as collecting **0 cases** wherever it is quoted as a gate.

Structural items deliberately **not** done here, both recorded against their owner: pinning
the ROM's printed `stages:`/`budget:`/`attribution:` lines as a fixture and deriving the
model's envelope from it (7-1 — no host test reads a ROM log today, so constants drift by
hand either way), and the sound-DMA/FIFO re-arm observation (7-2).

Verification of this pass: `make product-play-test` **12/12**; `make check` clean;
`make gafp-check` clean (157 frames, 865,996 B); `gafp-pack-check` 8/8; `flac-gates`
green; rebuilt `product-play.gba` **`750d01e97800de95…`, 1,085,804 B** — same size and
section map, and its headless log is **byte-identical** to the `694126d406eb9b2a…` run
(`diff` clean: 571/571, `underflows=0`, `min_lead 9120`, PCM/FIFO MATCH,
`verdict PASSED (… fifo=true)`, exit 124 by design). The decode-only pair is unchanged at
`f73097c144fe4157…` / `aecae6ef815d48c0…`. Working tree clean before and after.

## Step 5 — the sustained-overage arithmetic, the guard's shape, four nits (2026-11-12)

A closing review pass found one substantive defect and several description errors that
step 4 had recorded as fixed but that the code still contradicted.

- **The sustained-overage ceiling was truncated.** `frame_cycles(2048) / PERIOD_CYCLES` is
  **3.657** periods per frame, but the test divided as integers and used 3, so the "no ring
  in the picture" bound printed **843,237 c/frame = 152.9%** of the measured cost instead
  of `281,079 × 1,048,576 / 286,720 =` **1,027,946 = 186.5%** — 18% of a frame's cycles
  dropped, and with them the conclusion: the ring was credited with **~38 points (377 per
  mille) of *sustained* overage**. A bounded buffer cannot buy sustained throughput. The
  shipped clip survives to 190.6% and a clip twice as long to 188.2%, so depth buys **41
  per mille = 4.1 points** of lead spent once. `sustained_overage_tolerance` now
  multiplies before dividing, factors the bisection into `tolerate_per_mille`, and bisects
  both clip lengths, asserting the tolerance *falls* and never goes below the ceiling (the
  old assertion could not catch it: it compared the tolerance to the same truncated
  bound). Geometry decisions unchanged — depth cliff 20 chunks, prefill cliff 1, stall 17
  periods — so the fix is to the test and the records, not the geometry.
- **The Q14 tripwire left the `.expect`.** `geometry::RATE_BRIDGE` is one named
  `Option<(u32, u32)>` that both `playback_rate_for_stream` and the guard read, and the
  41.06 cents are pinned against a `RECORDED_BRIDGE` literal that survives the arm's
  removal. `PRODUCT_STREAM_RATES` crossed with `bridge.is_none()` is therefore green in
  both end states and red only if 7-0 lands half-way; 7-0's edit is that constant to
  `None` plus one allow-list entry, and nothing has to be deleted in a hurry.
- **Four descriptions fixed, as owed:** the `armed:` witness no longer claims "only the
  timer1 bit should be set" while printing `IE=0x0011` (VBlank is agb's, and the line now
  says so); `decode_and_push` no longer claims the fed phase "runs the same code" when
  `fed_frame` is a separate function (it shares the `decode_one`/`convert_frame`/
  `push_bytes` seam, which is what is now claimed); the rate probe's **59.4%** worst frame
  is labelled as the pre-median instrument's reading in all four places (this image prints
  53.964%); and the "and a / a sound-FIFO" duplication is gone. `Stage::note` additionally
  excludes a void window from `inside`/`worst`/`best` as well as from `sum`.
- **The image and the envelope moved, and the record says why.** The ROM's translation unit
  changed (one string and several comments), so `product-play.gba` is now
  **`c0d3ac36e4580a7c…`, 1,085,868 B**; the stage windows moved by tens of cycles because
  of layout, so the host model's envelope is re-read from this build — 65,073,008 +
  17,331,966 + 2,262,520 over 152 windows = **557,022 raw**, less 158 boundaries at 5,641 c
  = **551,159 attributed**, decode half **422,248** — and
  `the_envelope_is_the_runs_own_sums` pins it. Ledger, audit, watermarks, PCM and FIFO pins
  and the verdict are unchanged.

Verification of this pass: `make product-play-test` **12/12**; `make check` clean; rebuilt
`product-play.gba` **`c0d3ac36e4580a7c…`, 1,085,868 B**; one headless run: 571/571 periods,
`underflows=0`, 0 coalesced/late, tail 1/240, `min_lead 9120`, `max_lead 19600`, PCM and
FIFO MATCH, `verdict PASSED (ledger=true, audit=true, pcm=true, fifo=true)`, `fit: lines=9
… fits=true`, exit 124 by design. Its diff against the previous image's log is the
`armed:` witness line plus the stage/attribution jitter above — no audit, digest or ledger
field moved. The decode-only pair is unchanged at `f73097c144fe4157…` /
`aecae6ef815d48c0…`.

## Out of scope (follow-up PRs)

- 7-0: 32,768 Hz asset/resample/pitch witness, set `geometry::RATE_BRIDGE` to `None`,
  re-key `MEASURED_DECODE`, `EncodeProfile` rate clause, identity-or-refusal in the packer.
- 7-1: hardware row `(32768, 2048, fixed, 0)`, remove `--allow-unmeasured`, silicon
  re-read of the shares.
- 7-2: whether the hand-built timer/DMA/IRQ feeder ships.
- 7-3: audibility, both units, C2.
