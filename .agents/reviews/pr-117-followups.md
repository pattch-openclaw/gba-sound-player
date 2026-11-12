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
- **Q14 test shape.** The asset assert is now an explicit `PRODUCT_STREAM_RATES`
  allow-list (the 32,000 bridge is absent); 7-0 flips one line rather than deleting a red
  `assert_ne!`. The sampled refusal list is unchanged (the exhaustive sweep is docs-only,
  per #5).
- Verification: `make check` clean; `make product-play-test` 12/12; ROM
  `694126d406eb9b2a…`, 1,085,804 B; two identical runs, `PASSED (… fifo=true)`,
  `fit: lines=9 … fits=true`; audio numbers within 1-72 cycles (work/frame 559,848 =
  53.391%, worst 565,867 = 53.965%).

## Out of scope (follow-up PRs)

- 7-0: 32,768 Hz asset/resample/pitch witness, delete the `32_000` arm, re-key
  `MEASURED_DECODE`, `EncodeProfile` rate clause, identity-or-refusal in the packer.
- 7-1: hardware row `(32768, 2048, fixed, 0)`, remove `--allow-unmeasured`, silicon
  re-read of the shares.
- 7-2: whether the hand-built timer/DMA/IRQ feeder ships.
- 7-3: audibility, both units, C2.
