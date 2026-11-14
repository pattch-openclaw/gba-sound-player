# Playback pipeline measurements — the rate flow, FIFO playback, and the live-producer readings

> **Scope: the FLAC track's playback log.** Every run below was produced by a FLAC image —
> `flac-integration.gba`, `product-play.gba`, the rate ROM — decoding a `GAFP` asset through
> `crates/flac-lite` and `crates/flac-lite-gba`. "The product path" is the **FLAC** product path.
> Other codec tracks keep their own records ([`../ULI2.md`](../ULI2.md),
> [`../ULC.md`](../ULC.md), [`../OPUS.md`](../OPUS.md)); the feeder geometry these runs measure
> is shared ground that ULI2 reuses, so a *figure* here may be re-usable elsewhere while its
> *verdict* is not.

Decoder cost is paid per sample; the budget is granted per second. That asymmetry
is why the rate track exists, and this file is its measured record: the rate ROM
(real music at 10,512 Hz), the rate-play ROM (audible FIFO playback), and the
playback-attribution / feeder-off probes that followed.

The dated amendment log is kept intact because each entry carries a measurement
that changed a decision — the push window, the convert stage, the +38% landing
regression and its fix, the feeder-off control that voided an earlier reading.
Summarised numbers and the rules for quoting them: [`measurements.md`](measurements.md).
The tooling proposal that preceded it (the four-step ladder and the sweep design)
is retired; what survived is at the end of [`plan-of-record.md`](plan-of-record.md)
("the rate track is orthogonal, not an alternative").

**Readiness facts that constrain any rate work** (measured 2026-10-11):

- The mixer domain is **exactly three rates** — agb 0.25.0's `Frequency` is
  `Hz10512` / `Hz18157` / `Hz32768` (buffer sizes 176 / 304 / 560). Arbitrary
  rates are unplayable until Goal #5's custom mixer exists, so the tooling is
  fail-closed to these three at build time.
- libFLAC 1.5.0 encodes all three **without `--lax`**, and every
  playback-rate-exact encode travels on the uncommon rate code `0b1101`
  (16-bit Hz) — **including 32,768 Hz**, which is not in the table (32,000 is).
  Encode rate must therefore equal playback rate, and the decoder seam already
  parses those frames (`SampleRate::from_flac_code` constructs `Explicit` from the
  appended value). No decoder work is needed to read them.
- The playback harness's ring buffer + FIFO8 feed is *not* the shipping mixer;
  the `agb` mixer/DMA integration remains Phase 2 step 9. The harness exists to
  prove the seam and to price the convert/push stages, which are on the critical
  path for full-rate playback.

---

## Completed: rate ROM flow — real audio at 10,512 Hz, measured (2026-10-17)

**What landed.** The proposal's Part 1 steps 1–3 in the spike crate: a
`rate-assets`-feature-gated ROM bin (`src/rate_main.rs`, its own `[[bin]]`
+ `[[test]]` entries), a staging generator (`scripts/gen_rate_assets.sh` →
`frame_vectors.py rate_assets`), a host witness (`tests/rate_witness.rs`,
8 tests), Makefile targets `rate-assets` / `rate-test` / `rate-rom` /
`native-rate-rom` / `podman-rate-rom`, and `make clean` + `.gitignore`
coverage for everything temporary the flow writes. Real audio —
`assets/sound/bioluminescent.wav` (the known-good 96 kHz / 24-bit master,
258.6 s), ffmpeg-normalized to 16-bit stereo `pcm_s16le` at exactly
10,512 Hz — replaces synthesized tones for this gate: reference tracks are
listenable, which synthesized material never was (Sam's call).

**Witness strategy.** Same correctness-before-speed layering as the
committed spike, one input: the generator fail-closes before writing
(mixer-domain rate; per-FRAME resolved rate == requested rate on the
`0b1101` path; 16-bit/2048 profile; `check_stream` exactness; region
tiling; a full two-slot subframe walk with the footer-gap rule — and a
`-l 4` majority-LPC rule measured on THIS input's bytes, not the
synthesized clip's); the host witness (`make rate-test`) proves the
embedded bytes decode bit-exactly vs `flac -d` and that the ROM's shared
fold reaches the PCM pins; the ROM re-proves decode on its own bytes
(both arms FNV `0xD20880D6D6D89E03` = generator pin, cross-arm agree);
timing only engages on a proven image. Determinism: two headless mGBA
runs of sha `35314e1e…` (2026-10-17) `cmp`-byte-identical; the sha is
stable across same-tree rebuilds.

**Prior assumptions that did not survive.**

1. **The `-l 0` FIXED-only census is false for real material.** The
   committed `spike_census` demands FIXED-only for `-l 0` — true of the
   synthesized tones, wrong for a song: silence encodes CONSTANT.
   Measured: the real `-l 0` arm emitted `constant0 x10` on slot 0 (and
   `fixed0 x3` + `fixed1/2` on slot 1). The rate flow therefore census-
   checks the *profile* (kinds inside constant/fixed/lpc, `-l 4` majority
   LPC), which is what Correction 3 always said was enforceable: max
   predictor order + flags, never a kind census.
2. **The proposal's "LPC post-E4 ≈ 1,770 c/s" was a variant measurement,
   not the live path.** E4's i32 integrators were measured as a vendored
   spike variant and never landed in `flac-lite` (the module's own note:
   accumulate in `i64`; the perf spike *decides* whether i32 is provably
   safe). This ROM measures the live i64 path — see the result table; the
   projection's error was assuming a landed lever.
3. **The committed calibration shape does not port unchanged.** First
   build's calibration came back 26/22/22/22/22 on both arms —
   deterministic, not noise: the first micro-window after the decode-
   proof log burst carries the cold-window artifact (+4), the same
   sensitivity E1 and the first PR-3 draft recorded. Adopting the E4
   unified loop (3 discarded + 5 reported, byte-identical iterations,
   nothing logged between windows) made all 8 windows read 20; overhead
   is netted per this image's own shape (20, not the committed 22).

**Deliberate scope cuts.** No mixer, no cadence alternation, no double
buffer (Phase 2 step 9's, per Sam's scope call — see the amendment); no
multi-clip ROM (step 4); no Part 2 sweep (this is its first data point);
`podman-rate-rom` added per pattern, not yet exercised (native path
verified). The screen verdict ANDs both arms, so the rate ROM runs RED
when only the LPC arm misses — red here means "measured, over budget",
not harness failure (the log separates the two).

**Gates.** `make spike-test` 15+8 counts unchanged; default spike ROM
sha `0d976591…` reproduced byte-for-byte; `make rate-test` 8 green; `make
check` clean; `make native-spike-rom` and `make native-rate-rom` build.
Committed surface (`assets.rs`, committed blobs, `main.rs`, `crates/`,
`spike_witness.rs`) diffs zero lines against `origin/main`.

**The measurement — does bioluminescent decode inside the budget at
10,512 Hz? FIXED yes, LPC no (live i64 path).** ROM sha `35314e1e…`,
input sha256 `8f35fa88…`, flac 1.5.0, ffmpeg 9.0.1, mGBA (this host),
2026-10-17. Budget (derived): 2048 / 10,512 Hz × 16.78 MHz = **3,269,162
cycles/frame** (1,596.27 c/sample). Calibration overhead 20 c net, IME 0
in-window, per-window counter reset (this pass spans > 2^32: the LPC arm
sums 7.98 G — the Opus port's rollover rule applied pre-emptively; no
window ever hit the sentinel). Means derived host-side (division-free ROM
rule):

| arm (1,328 frames) | min c/f | mean c/f | mean c/s | mean % budget | worst c/f @ frame | worst % budget | verdict |
|---|---|---|---|---|---|---|---|
| FIXED `-l 0` | 28,082 @ 1327 | 2,417,429 | 1,180.4 | **73.9 %** | 2,559,507 @ 505 | **78.3 %** | **FITS** |
| LPC `-l 4` | 28,082 @ 1327 | 6,010,517 | 2,934.8 | **183.9 %** | 6,916,436 @ 413 | **211.6 %** | OVER |

(min = the legitimately short final frame, 842 samples — consistent
across both arms.)

**Reading.** The proposal's baseline reading is confirmed on real music:
the FIXED arm's mean AND worst fit at the baseline decode path — the
tone-based projection (69.7 % mean) was 4.2 points optimistic (real audio
73.9 %), close enough that the margin's sign held; per-sample cost is
~6 % above the 32 kHz tone figure (1,180 vs ≈1,113 c/s) — content, not
rate. The LPC arm lands at the live-i64 scale (2,934.8 c/s ≈ 0.88 × the
live LPC figure — E6's same-run control, 3,340.2 c/sample; mixed orders
1–4 + real mid/side), not the projected post-E4 scale: **E4's i32 landing
is now the named next lever for LPC-at-10,512** — projecting E4's
measured −46.9 % onto this mean lands ≈ 97.6 % mean / ≈ 112 % worst, i.e.
mean inside, worst out; the stacking question is E9's, measured not
assumed (PR-3's own mean figure was ≈ 3,350 c/sample). Size facts for the Part
2 sweep (measured, this material): ≈ 1.91 MB/min compressed per arm
(SI, 10,512 Hz stereo) — a 4:18 track embeds 8.2 MB/arm; the two-arm ROM
is 16,522,992 B, inside a 32 MB-class cart with one arm to spare.

## Completed: rate-play ROM — audible FIFO playback at 10,512 Hz, measured (2026-10-19)

**What landed.** `examples/flac_spike/src/playback.rs` (the ring-buffer +
FIFO-fold library, host-testable) and `src/rate_play_main.rs` (bin
`flac-rate-play`, `required-features = ["rate-assets"]` — invisible to the
committed spike build, same coexistence contract as the rate ROM), built by
`make rate-play-rom` / `make native-rate-play-rom` behind the same
fail-closed staged-assets gate. Playback is a **custom IRQ feeder, not
agb's Mixer**: timer0 ticks at the sample rate (overflow 1,596 @ 2^24 Hz =
1/10,512 s), timer1 cascades and interrupts once per mixer-domain hardware
buffer (176 samples @ 10,512 Hz — the measured `Frequency::buffer_size`
readiness fact); each tick copies the next ring chunk into the idle
ping-pong word-aligned buffer and re-arms both sound DMAs with agb
`hw.rs`'s control word (dest-fixed | repeat | words | sound-FIFO start |
enable). `enable_sound` transcribes the `pub(super)` hw.rs enable sequence
(FIFO resets, A→left / B→right at 100 %, bias 8-bit/65,536 Hz). The ring is
36 × 176 samples ≈ 603 ms; 16 chunks prefill before the feeder arms, so
the lead starts above the measured worst-frame drain. The producer's
flow-control spin never holds the ring borrow across an iteration (the
feeder needs it in IRQ context — single-core deadlock, caught in review).

**Ledger semantics — a decision the debug gate caught.** A starved period
zero-fills the DMA destinations, the feeder still re-arms (the DMA moves),
and the consume ledger advances by `real` — the bytes actually taken; zeros
are not ring data. The first design advanced the ledger by CHUNK every
period, and the debug run caught the consequence: `ready = pushed −
consumed` subtract-overflows once the consumer runs ahead, and at recovery
the producer writes next while the cursor has already passed those slots —
samples silently skipped, and the witness claim ("the bytes that reached
the sound hardware are proven") breaks, because the pins fold at push
time. Pinning the ledger at the producer realigns the cursor exactly where
the producer writes next: nothing skipped, invariant `pushed ≥ consumed`,
and drain termination holds (finished ⇒ the residue lands as Tail; a
Starved at the pin means done). The rewritten
`underflow_and_tail_are_classified_apart` witnesses it: a starved period
leaves the cursor at 0 and the tail is the whole residue in FIFO order.

**FIFO8 pins — the three-way witness.** The generator grows per-arm
`FIFO8_L/R` pins (a Python `>>8` fold over the `flac -d` PCM, mirroring
what the hardware does to each byte); the host witness
`fifo8_pins_meet_generator_library_and_rom_path` folds the same embedded
bytes through the library path and checks the ROM's aliases; the ROM folds
at push time and its serial asserts MATCH against the embedded pins.
Generator == library == what the ROM handed its DMA registers — one fold
lower than the PCM chain-of-custody. The pins fold at push time: the last
hop, DMA→paired FIFO, is witnessed by the audible check, not the hash (see
the routing fix below — and above, why the headless run could not see it).

**Review fix — DMA routing off-by-one (2026-10-19, post-opening).** The
first commit transcribed agb's DMA intent but not its base arithmetic
(`0x0400_00B0 + 0x0C·n` — hw DMA1 starts at `0x0BC`, not `0xB0`): both
sound DMAs were programmed one channel low — hw DMA0→FIFO A, illegal
sound-FIFO scheduling (mGBA discards it; left silent), and hw DMA1→FIFO B,
mis-paired (right glitchy). Headless runs passed blind — pins fold at push
time, and the runner has no audio device and ignores the scheduling
warning; Sam's ears caught it. Fixed by routing hw DMA1→FIFO A, DMA2→FIFO
B. Re-verified on the fixed image (sha `7d58e0cd…`): verdict unchanged —
PASSED/BLUE, underflows = 0, FIFO8 pins MATCH, two runs `cmp`
byte-identical; lead 4,528–4,752 (a chunk higher than the pre-fix range —
DMA1's FIFO A refills now actually steal bus cycles).

**Gates** (all pre-commit, 2026-10-19). `make spike-test` counts unchanged
(15+8); `make native-spike-rom` rebuilds **byte-identical** to the
calibration anchor `0d976591…`; `make rate-test` 9 green; `make check`
clean; committed surface (`crates/`, `assets.rs`, blobs, `main.rs`,
`spike_witness.rs`) diffs zero lines against `origin/main`. Two headless
mGBA runs of `flac-rate-play.gba` (sha256 `79be6d60…`) `cmp`
byte-identical, verdict line PASSED (screen BLUE).

**The run** — same material as the rate ROM entry (input sha256
`8f35fa88…`; 1,328 frames, 2,718,538 samples, stereo @ 10,512 Hz; the
FIXED arm plays, the LPC arm is skipped — 2× over its own budget at this
rate, measured, and no feeder fixes that). The feeder took 15,447 chunks:
**underflows = 0**; pushed = consumed = 2,718,538 exactly; FIFO8 L
`0x1189676B…` MATCH, R `0xFEC71BF5…` MATCH. Producer lead at the frame
checkpoints: **4,448–4,752 samples**, everywhere above the worst-frame
drain (2,559,507 cyc ≈ 152.6 ms ≈ 1,604 samples @ 10,512 Hz). IRQ-on:
unlike every E-probe's IME-off windows, this ROM runs with timer interrupts
live — the first measurement of the decode path coexisting with the
feeder, and the 26.1 % mean-frame headroom (rate ROM entry) shows up as
ring lead, not starvation.

**Reading.** The deferred playback half now has its baseline: real music
streamed end-to-end through the GBA sound FIFO at the reduced rate, every
byte pinned. The mixer seam (double-buffer cadence, volume, `Sound` API)
remains Phase 2 step 9's; what is proven here is the shape — decode +
prefill + ring + ping-pong + DMA re-arm keeps up with IRQ live at the
baseline decode path. Next levers unchanged (E4's i32 landing for LPC at
10,512; the stacking question stays E9's, measured not assumed).

**Amended 2026-10-20 (Sam): decode-only boot pass removed — audio starts
on load.** The full-track `decode_proof` that ran before the sound hardware
enabled (a silent re-decode of the whole song for the `fnv_pcm` pin) is
gone from this ROM only: it put minutes of decode-only silence before any
audio, and the playback pass re-derives the same bytes into the FIFO8 pins
anyway. Boot now runs the cheap region-FNV asset check, prefills 16 chunks
(2 frames), arms the feeder — audible within a second of load. The
rate ROM's own decode proof (`rate_main.rs`) is untouched; the playback
verdict still judges the FIFO8 pins, the sample ledger, and the period
audit on the measured image. Re-measured after the removal (rebuilt ROM
sha256 `fb7a2fc2…`, two headless runs `cmp`-identical): verdict
PASSED/BLUE, FIFO8 L/R pins MATCH, pushed = consumed = 2,718,538,
underflows = 0, and the period audit reproduces the `a4f7560` numbers
exactly (hw_elapsed 19,170 vs serviced 15,447, coalesced 3,724,
push_swallowed 5,049 across 1,326/1,326) — as expected: the removal
touches boot order only, not the feeder or the push window, and the glitch
fix remains the next step. Gates: `make rate-test` green, `make check`
clean, calibration anchor `flac-spike.gba` rebuilds byte-identical
(`0d976591…` — coexistence held), pre-commit hook full suite green.

**Amended 2026-10-22 (Sam): witness work removed from this ROM — no fold,
no boot asset check; audio hot path only.** Product-shape scope call: this
ROM plays sound; nothing that contributes no audio runs in it. Removed:
the FIFO8 fold from the producer (per-byte FNV-1a-64 on ARM7TDMI is an
emulated 64-bit multiply per byte on the producer wall) with its pin
judgment, and the boot region-FNV asset check — the staged bytes are
verified at generation time: the fail-closed census and the host witness
(`make rate-test`) still fold the embedded bytes through the library path
against the FIFO8 pins as a *build-time* gate; the ROM trusts the assets.
Correctness of the byte stream now rests on the gates plus the audible
check, not on in-ROM hashing. Kept (they measure what it plays): the
underflow count, the sample ledger with the pushed==total_samples
judgment, and the period audit as regression witness. The spike ROM and
the rate ROM (`rate_main.rs`) keep their decode proofs and pin folds —
research instruments; this ROM is the thing researched. Library
(`playback.rs`) keeps `fold_fifo8`/`fold_bytes` for the host witness.
`push()` is now convert (IRQ-live) → ring copy + credit (CS), hashes
gone. Measured on the stripped image (ROM sha256 `8f620d2d…`, stable
across rebuild; two headless runs `cmp` byte-identical; input
`8f35fa88…`): **underflows 5,752 → 2,399**; hw periods 17,846 (coalesced
0, late 904, push_swallowed 1,032 in 1,032 pushes — the ~1-period
swallow persists); ledger balanced at 2,718,538, samples OK. Verdict
still FAILED/RED: producer wall ≈ 225 ms/frame (17,846 × 16.74 ms ÷
1,328, was ≈ 267 ms) against the 195 ms budget — the fold was ≈ 40 ms/
frame of the debt, near the 10–20 ms/frame derived estimate's upper half
plus per-call overhead, leaving ≈ 30 ms to attribute across decode
(unpinned E2b table in question), convert, and the push swallow. Gates:
`make rate-test` 8 green, `make check` clean, `make native-rate-play-rom`
builds; committed surface unchanged (the edit touches only the
`rate-assets`-gated bin, so the calibration anchor is untouched by
construction).

**Amended 2026-10-22 (Sam): E2b table pinned to `.iwram` — re-measured,
worth ≈1%, not the missing 30 ms.** The landing's stated re-measurement,
run on this ROM. Policy exception ratified with it: flac-lite
`#![forbid(unsafe_code)]` → `#![deny(unsafe_code)]` with one named
exception — `unsafe(link_section = ".iwram")` on `bits::LEADING_ONES`
(cfg-gated `target_arch = "arm"`, item-scoped `#[allow(unsafe_code)]`; a
placement attribute on an immutable const table, no unsafe code). Built-ELF
placement witness (`lto = "fat"` survived): `nm` puts the symbol at
`0x0300022c`, `.iwram` 0x22c → 0x32c — exactly +256 B, matching the probe's
record; agb `CommonInit` CPUSet copies at boot. Measured (ROM sha256
`410d5f8b…`, stable across rebuild; two headless runs `cmp`-identical;
input `8f35fa88…`): **underflows 2,399 → 2,263**, hw periods
17,846 → 17,710, producer wall ≈ **223 ms/frame** (was ≈ 225), replays
stay 0 (coalesced 0; late 910, push_swallowed 1,039 in 1,039 pushes),
ledger balanced at 2,718,538. **Verdict still FAILED/RED.** Reading: the
pin is worth ≤1.7 ms/frame (~0.8%) — the E2b table win was already banked
unpinned (a `.rodata` *data* table costs a few cycles per 8-bit window, not
per bit; E6's cartridge-fetch lesson does not replicate at 256 B data
scale), and the pin stacks as a small cumulative gain (levers-compose —
small harvestable percents count). The remaining ≈28 ms/frame is therefore
**not** the pin question: convert cost and the push-swallow window are
unattributed, and the decode side needs live-ROM attribution before it is
blamed — 144 ms is the rate ROM's IME-off figure; the live path runs with
feeder IRQs on. Gates: `make flac-test` green (thumbv4t compile gate proves
the attribute; counts unchanged — placement-only change), `make rate-test`
green, `make check` clean, `make native-rate-play-rom` builds; the
calibration anchor moves by construction (this diff touches
`crates/flac-lite` — decoder-landing rule, anchor baselines re-recorded by
the next E probe, not chased here).

**Amended 2026-10-22 (Sam): live-producer attribution probe — the stage
map is measured; revised ordering recorded.** The ~28 ms the pin
amendment left unattributed is now a budget map, measured IRQ-live on this ROM (E8's
attribution shape re-tooled: the IME-off `timed()` window measures a bus
state the producer never runs in, so the instrument is the armed chain
itself — a sample-resolution position clock, `timer2` periods × 176 +
in-period samples from `TM1_DATA`, one tick = 2^24/10512 ≈ 1,596 cycles;
eight clock reads per frame at the stage boundaries, no logging between —
E4's unified-loop discipline; probe code is sample-only, preserved under
`examples/flac_spike/experiments/playback-attribution/` with the
reappliable patch and both `cmp`-identical logs, never in the live
sources). Witnessed on the measured image (ROM sha256 `cb682591…`, rustc
1.100.0-nightly a69a63265, mGBA 0.10.5, input `8f35fa88…`; calibration
pairs **0/0** sum/max — instrument cost far under one tick; `clock_retries
= 2` over the pass; per-frame **closure underflows 0**, inter-boundary gap
321 samples in 1,326 frames = 0.02 ms/frame):

| stage | mean (samples/frame) | ms | % of budget | max |
|---|---|---|---|---|
| wait | 9.34 | 0.89 | 0.5 % | 1,795 @frame 1321 (single near-tail stall) |
| decode | 2,108.6 | 200.6 | **103.0 %** | 2,146 @564 |
| convert | 148.5 | 14.1 | 7.2 % | 150 @4 |
| push CS | 133.6 | 12.7 | 6.5 % | 135 @73 |
| gross | 2,400.3 | 228.3 | **117.2 %** | 2,429 @572 |

Budget derived: 2,048 samples ÷ 10,512 Hz = 194.8 ms = 3,268,621 cycles.
Cross-check: this image's audit-derived wall (hw periods 18,121 ×
176/10,512 s = 16.74 ms ÷ 1,328 frames) = **228.3 ms — equals the measured
gross**; there is no hidden producer phase outside the four stages.
The probe image runs ≈ 5 ms/frame over the clean pinned image's 223.2
(17,710 × 16.74 ÷ 1,328) — the instrument's own reads and tally stores;
shares are read relatively, the clean image's audit stays the wall
authority.

**Three readings that change decisions.** (1) **The push CS is real debt,
not a counter artifact**: 133.6 samples = 12.7 ms IME-off — the
"~2·n byte stores, µs" module claim is off by three orders (≈ 52 cyc/byte
through the per-byte wrap-checked loop under Mutex/RefCell: 133.6 × 1,596
cyc ÷ 4,096 stereo bytes = 52.1). The swallow arithmetic closes: 133.6/176 = 75.9 % of a
period is IME-off per push, and the observed swallow rate is 1,007/1,326 =
75.9 % of pushes — phase coincidence at the measured window width, exactly
as expected once the width is known. `late=871, max_delay_ticks=132` sits
at the CS width: the late splices ARE the push window. (2) **Decode pays a
+39 % live penalty**: 2,108.6 samples = 3,365,427 cyc/frame over 2,048
samples = **1,643 c/sample** (3,365,356 cyc ÷ 2,048, from the mean 2,108.6109
ticks × 1,596.006 cyc) vs the rate ROM's IME-off 1,180 c/s on this
material; +56.6 ms/frame = 200.6 − 144.0, the IME-off figure derived at
1,180 c/s × 2,048 = 2,416,640 cyc = 144.0 ms. The obvious suspects
account for only a fraction: the feeder handler's own work rides inside
the producer's window (≈ 13.6 handlers/frame — 228.3 ÷ 16.74 — × a ~3·10³
cyc estimate for the 352-byte drain copy + audit reads + DMA re-arm
≈ 2.4 ms/frame) and the sound-DMA bursts steal bus cycles (2 × 44 words
× ~8 cyc/word ≈ 700 cyc/period ≈ 0.6 ms/frame); together ≈ 3 ms against
+56.6 — the remaining ≈ 50 ms is unexplained and the round-2 question
(bus contention vs mGBA's DMA-stall model vs handler stacking; a feeder-off control arm settles it, and if the penalty is
emulator-specific the product's real-hardware decode is nearer 144 ms —
which re-values E1 data staging from −2.5 % to potentially the biggest
single lever, decode currently fetching from the cartridge bus while the
sound DMAs run). (3) **Convert is 14.1 ms** ≈ 55 cyc/channel-sample —
above the bounded estimate ("single-digit ms"), a table/word-pack fast
path has real target.

**Corrected 2026-10-26 (Sam): reading (2) is void — the comparator went
stale.** The 1,180 c/s figure is the pre-E2b rate ROM; the committed
tree's IME-off decode on this material is 1,614.6 c/s (same host,
this session). Live coexistence costs ≈ +1.5 %, not +39 %; the ≈ 50 ms
never existed as live-path cost (the bisect convicts the E2b landing —
rate-play entry, 2026-10-26 feeder-off amendment), and the E1 staging
re-valuation is not supported (sound-DMA marginal contention measured
+0.22 %). The decode mean itself (2,108.6 t/frame) was and remains
correct — the stage map stands; only its comparison baseline moved.

**Revised ordering (proposed after the probe; recorded 2026-10-22 on
Sam's request — the working order until measurements move it).** ① **Word-copy push** —
the CS's per-byte loop to u32 stores: the cheapest proven lever, cutting
wall ≈ 10 ms and killing the swallow + late-splice clicks together.
② **Convert fast path**, rides the same byte-path PR (combined debt
26.8 ms; expected harvest ~13 ms, measured not assumed). ③ **Feeder-off
control arm** on the live image — accounts the decode penalty; decides
E1 staging's true size before kernel work is planned against a 200 ms or a
145 ms decode. ④ **E10 PR B** (C2 word-skip + composite sample-step) —
the structural unary harvest, unchanged in rank as the required big win:
decode alone is 103.0 % of budget, so zeroing convert + CS + wait still
leaves it over. Catch-up stays margin, never the fix; the rate knob stays
the budget-growing fallback, re-derived on the improved path (levers
compose).

**Amended 2026-10-23 (Sam): word-copy push lands — CS window 12.7 → ≈0.9 ms,
producer wall 223.2 → 211.4 ms, seam artifacts structurally gone.** The
revised ordering's ①, on Sam's request. `push_bytes`' per-byte slip walk (the
probe's 52.1 cyc/byte) becomes a whole-span `copy_from_slice` per wrap span:
the target's memcpy dispatch (agb `agbabi`'s `__aeabi_memcpy`, `.iwram`-resident,
word-copying when the span is 4-byte aligned) moves the steady-state 2·2,048-byte
pushes in words. No unsafe and no reinterpretation — the crate stays
`#![forbid(unsafe_code)]`-clean, the word path is the memcpy dispatch, not
hand-written `u32` stores (a shape cut from the ordering's "to u32 stores";
the effect is measured equal-or-better, see below). Built-ELF witness:
`objdump` of the measured ELF shows `rom::push` calling `__aeabi_memcpy` ×4 —
two wrap spans × two channels. Steady-state alignment holds by arithmetic:
`pushed` advances by 2,048-divisible blocksizes, the chunk (176) and ring
(36·176) are 4-divisible, the ring is an aligned static and the scratch a
word-aligned heap buffer; odd-sized tail frames ride memcpy's own byte path —
exactly the pre-word shape, no special case needed.

**Witness (byte stream cannot drift).** Host suite grows 24 → 25 lib tests:
the existing `push_bytes_path_matches_push_frame_byte_for_byte` plus a new
`push_bytes_word_copy_matches_byte_walk_across_residues` — every dst×src
residue pairing (offsets 0..3 × cycling push residues), sizes 1..=5 on a
12-byte ring forcing wraps on both channels, three-way meet: word path vs the
pinned per-byte walk (`push_frame`) vs intent (the pattern bytes). ROM side:
the ledger (`pushed == consumed == 2,718,538` `[OK]`) and `make rate-test`'s
FIFO8-pin fold over the embedded bytes judge the stream the copy path moves.

**Measured** (ROM sha256 `29ba6e23…`, input `8f35fa88…`, rustc 1.100.0-nightly
`a69a63265`, mGBA 0.10.5; two headless runs `cmp`-identical):

| witness | before (pinned, `1ee3033` era) | after |
|---|---|---|
| producer wall (audit-derived) | 223.2 ms/frame | **211.4 ms/frame** |
| hw periods elapsed / serviced | 17,710 / 16,764-era split | **16,764 = 16,764** (blind replays 0) |
| late / max_delay_ticks | 910 / 132 | **0 / 0** |
| push_swallowed / pushes | 1,039 / 1,039 | 68 / 68 |
| underflows | 2,263 | 1,317 |
| ledger | 2,718,538 OK | 2,718,538 OK |

Wall derived: 16,764 × 16.74 ms ÷ 1,328 = 211.4 ms/frame — 108.5 % of the
194.8 ms budget (was 114.6 %). Harvest 11.9 ms — the stage map's 12.7 ms CS
minus a ≈0.9 ms residual window: swallow ratio 68/1,326 = 5.1 % of a period
= 9.0 samples = 0.86 ms ≈ 3.5 cyc/byte (was 52.1). The ordering predicted
≈10 ms for ①; measured 11.9, within the estimate's shape — measured, not
assumed, and the wall and the window move by the same number, so the map's
book closes.

**The reading that changes decisions: window < grace closes the seam.** The
residual 9-sample window is *narrower than the DMA's 16-tick rearm grace*, so
every overflow swallowed inside the CS is serviced within the grace — the
repeat controller never reloads the old source. That is exactly what
`late=0` + `elapsed == serviced` alongside 68 nonzero swallows says: the
remaining swallows are harmless phase coincidences, not events, with 7 samples
of margin. The pop + dragged-tempo channel (~1 period/push across the whole
era) is closed structurally, not statistically. What remains is pure speed
debt: 16.6 ms/frame, verdict still FAILED/RED — expected, decode alone is
200.6 ms.

**Gates.** `make rate-test` 25 lib + 9 rate_witness + 8 spike_witness green;
`make spike-test` counts unchanged (15 + 8); `make check` clean;
`make native-rate-play-rom` builds. Anchor witness: a clean-main (`e4ac78a`)
build and this branch's `flac-spike.gba` print `cmp`-identical serial logs
(cross-tree rebuild sha drift is pre-existing — the harness rule judges the
logs, never the sha); `crates/flac-lite` untouched, committed surface diffs
zero lines. **Ordering:** ① banked; ② convert fast path becomes its own
byte-path PR next (14.1 ms stage); ③ feeder-off control arm; ④ E10 PR B —
ranks unchanged, decode still dominates the budget on its own.

**Amended 2026-10-24 (Sam): convert fast path lands — the safe shape
harvests 1.8 ms of the probe's 14.1 ms; the word-store remainder needs
asm.** The revised ordering's ②, on Sam's request, as its own byte-path
PR. Two jobs get fast paths in `playback.rs`, both safe (`#![forbid(unsafe_code)]`
held — the pinned nightly offers no safe reinterpret route, measured:
`align_to_mut` is an *unsafe* fn (E0133) and `align_chunks_mut` does not
exist (E0599)). The fit gate becomes `fit_gate`: branch-free
OR-accumulate — a sample fits the i16 profile iff `(s as u32) + 0x8000`
fits in `u16`, so `acc >> 16 == 0` decides the whole frame in one branch;
on failure (stream desync — cold) a second pass walks the pinned
interleaved order (l0, r0, l1, …) so the `Err` value keeps its exact
first-offender identity. The pack becomes `pack_fifo`:
`chunks_exact_mut(4)` with a fixed-4 `copy_from_slice` per group and the
sub-word tail on the pinned per-sample walk; the earlier index-window draft
left four `panic_bounds_check` sites in the disassembly, the iterator
shape removes them (and measures identically — see below).

**Witness (byte stream and error cannot drift).** Lib suite 25 → 27:
`convert_fast_path_matches_byte_walk_across_residues` — dst offsets 0..=3 ×
lens 1..=17, stereo and mono, guard bytes before/after the slice must never
move, three-way meet against the per-sample intent (`fifo_byte` per
sample); `convert_error_identity_first_offender_interleaved_order` — the
Err is the first offender in *interleaved* order (right[0] = 50 000 wins
over left[1] = 40 000, as the pinned scalar path decided), destinations
untouched, mono leaves `dst_r` alone. ROM side unchanged: ledger `pushed ==
consumed == 2,718,538` [OK] + `make rate-test`'s FIFO8-pin fold.

**Measured** (ROM sha256 `9205084f…`, input `8f35fa88…`, rustc
1.100.0-nightly `a69a63265`, mGBA 0.10.5; two headless runs `cmp`-identical;
a comment-only rebuild `012b50c8…` → `9205084f…` prints `cmp`-identical
logs — the harness rule judges logs, never sha):

| witness | before (`29ba6e23…`, PR #77) | after |
|---|---|---|
| producer wall (audit-derived) | 211.4 ms/frame (108.5 %) | **209.5 ms/frame (107.5 %)** |
| hw elapsed / serviced | 16,764 = 16,764 | 16,618 = 16,618 (replays 0) |
| late / max_delay_ticks | 0 / 0 | 0 / 0 |
| push_swallowed / pushes | 68 / 68 | 71 / 71 |
| underflows | 1,317 | 1,171 |
| ledger | OK | 2,718,538 OK |

Harvest **1.8 ms** (Δperiods 146 × 16.74 ms ÷ 1,328) — **not the ~13 ms the
ordering's estimate hoped for**, and the honest reading is the point:
expected ≠ measured, the stage map's book stays open. Disassembly explains:
even the checkless 4-window emits four constant-offset `strb` per group on
thumbv4t (LLVM does not merge them), and the two safe drafts — index-window
with bounds checks vs `chunks_exact` without — measure the *same*
16,618 periods. The residual ≈ 12 ms of convert is per-byte-store cost
under IRQ-live bus state; killing it means packing four FIFO bytes in a
register and one `str` — asm (the E10 kernel track: `pack_fifo` is a pure
fn, golden-table-able, a natural candidate kernel) or an unsafe-reinterpret
policy change on the spike crate (a product-shape call, not this PR's).
The safe 1.8 ms banks anyway (levers-compose) with zero drift risk.

**Gates.** `make rate-test` 27 lib + 9 rate_witness + 8 spike_witness
green; `make spike-test` counts unchanged (15 + 8); `make check` clean;
`make native-rate-play-rom` builds. Anchor witness: `flac-spike.gba` builds
**byte-identical** with and without the diff (sha `4ac0c33a…` both trees) —
`playback.rs` is invisible on the anchor surface (zero playback symbols in
that ELF); `crates/flac-lite` untouched, committed surface diffs zero
lines. Verdict still FAILED/RED — expected; debt now **14.7 ms**.
**Ordering:** ② partially banked (1.8 of 14.1); the convert word-store
remainder moves onto the E10 asm-kernel track as a candidate. ③ feeder-off
control arm is next — it sizes the decode live penalty before kernel
planning; ④ E10 PR B stays the required structural harvest: decode alone
is 103 % of budget, and 107.5 % total says every remaining ms is decode's.

**Amended 2026-10-26 (Sam): feeder-off control probe (ordering ③) — the
live penalty is ≈ +1.5 %, and the bisect convicts the E2b landing (+38 %).**
The round-2 question from the attribution amendment (what is the +50 ms?)
is settled, and the answer rewrites the entry's cost story. Three arms in
ONE image (`feat/playback-feeder-off-probe`, measurement commit `acf42be`,
sample preserved under `examples/flac_spike/experiments/feeder-off-control/`
with the reappliable patch + every run log), one producer loop, one
variable (sound activity): **A** live baseline (shipped feeder IRQ + sound
DMA, period audit intact); **B** feeder off — timer chain keeps running as
the position clock, timer1's IRQ latch bit masked, no sound DMA, ring
drained synchronously between frames (outside every window — the flow-
control detail the plan left open: in-loop draining, not ring over-read,
keeps the ledger real and wait ≈ 0 by construction); **C** B plus both
sound DMAs armed against the ping-pong in repeat mode — FIFO drains keep
re-triggering 44-word bursts from the latched source with zero handler
CPU. Position clock per the 2026-10-22 method (port, not the old hunks —
#77/#78 moved `push`/convert): calibration pairs **0/0** all arms,
closure underflows **0** all arms, `clock_retries=0`; arm-equality witness
(FNV-32 fold of every decoded window outside the windows) prints
`hash A=B=C=3E765D9D samples 5,437,076/5,437,076/5,437,076 [OK]` — the
arms decoded identical samples while their clocks differed. A's decode
mean reproduces the 2026-10-22 map (2,108.72 vs 2,108.61 t, +0.006 %);
A gross 2,199.7 t = 209.26 ms meets #78's clean-image 209.5 ms audit wall
within 0.2 %. ROM sha256 `8ccb3dde…`, input `8f35fa88…`, rustc
1.100.0-nightly `a69a63265`, mGBA 0.10.5, two runs `cmp`-identical.

| stage (ticks/frame) | A live | B off | C dma |
|---|---|---|---|
| wait | 9.90 | 0.10 | 0.09 |
| decode | **2,108.72** | **2,077.28** | **2,081.89** |
| convert | 71.78 | 70.87 | 71.05 |
| push CS | 9.11 | 8.98 | 9.00 |
| gross | 2,199.71 | 2,157.47 | 2,162.24 |

**decode A−B = +31.4 t = +2.99 ms = +1.51 %** (frame-composition bias
≤ ~1.5 t: A skips the two prefilled constant frames; the delta is
≈ +2.9 ms either way). Of it: **C−B = +4.6 t = +0.22 %** is the marginal
sound-DMA bus cost; **A−C = +26.8 t = +2.55 ms** is handler CPU + IRQ
delivery. Handler estimate vs handler measurement: the ~3 ms theory lands
— on an honest baseline. A's audit stays healthy (replays 0, `late=0`,
`push_swallowed=71`); A's underflows 2,107 are the probe image's ~11.8
ms/frame instrument+fold weight (17,554 hw periods vs the clean 16,618),
not a seam failure. Convert reads 6.83 ms on the instrument across all
arms — the stage-window authority now, replacing the wall-derived ~12 ms
remainder arithmetic.

**The comparator was stale — same-session rate-ROM bisect** (two
`cmp`-identical runs per point, censuses identical, PCM pins MATCH; logs
in the sample dir): `e6577e0` FIXED mean **2,417,429 c/f reproduces the
recorded baseline exactly** (sum, mean, worst 2,559,507 @505, min 28,082
@1327); `e455dba` (the E2b `rice_quotient` landing + fold hoist) prints
**3,335,100 c/f = +37.96 %**; `1ee3033` (`.iwram` pin) 3,306,661 (pin
recovers 0.85 %); current `main` = 3,306,661 (playback-only PRs invisible
by construction). LPC: +15.2 % at `e455dba`, +14.8 % now. So the +39 %
"live penalty" was never live-path cost — **the committed reader is
slower than the per-bit loop it replaced**, on the ROM's own link, for
this material. The E2b result section carries the landing-correction;
the probe's vendored-arm numbers stand as facts about that arm (the E6
inlining-layout confound is the plausible shape), but the landed harvest
did not survive contact with the live path. The hardware delays Sam
heard are fully explained by the 209.5-vs-194.8 ms debt — no contention
term needed. E1 staging is **not** re-valued: adding all sound-DMA
traffic to a read-heavy decode moves it +0.22 % on this emulator, and a
staged arm was cut before running on exactly that measurement.

**Revised ordering (re-cut after ③; supersedes 2026-10-22's ③④ ranks).**
① **E2b fix-or-revert** — the project's biggest measured lever: the
regression is +38 % of FIXED decode (≈ 48 ms/frame); restoring `e6577e0`'s
reader live-adjusted lands decode ≈ 146 ms, producer gross ≈ 155 ms ≈
**80 % of budget with ~20 % headroom** (derived from two measured legs —
the fix PR measures it). Shape first: `nm`/`objdump` the landed
`rice_quotient` inner loop for why the table count lost (inlining, the
`peek_at` bounds path, or `LEADING_ONES` addressing through `.rodata`
under `lto = "fat"`), revert if no cheap fix. ② E10 PR B (C2 word-skip +
composite sample-step) stays the structural harvest, sized against
**~146 ms** decode, not 200; the convert word-store remainder stays on
the same track. ③ E4 i32 for LPC unchanged in kind; its LPC projection
must now be re-derived against the post-fix tree. The rate knob remains
the fallback. Levers-compose holds: ①'s harvest and everything after it
stack multiplicatively, each measured.

**Amended 2026-10-27 (Sam): E2a refill-accumulator landed alongside E2b —
root cause of the +38 % regression fixed; real-time playback confirmed at
correct tempo and pitch.** Inspection of `crates/flac-lite/src/bits.rs`
identified why `e455dba` regressed by +37.96 % on FIXED where the E2b
probe measured −21.71 %: the probe's `rice_quotient` rode **E2a's `u32`
refill-accumulator (`AccumulatorReader`)**, whereas `e455dba` landed the
8-bit `LEADING_ONES` lookup onto the stateless position-only `BitReader`
without E2a (`self.peek_at(self.bit_pos, 8)`). Without the accumulator,
every 8-bit window at an unaligned bit offset (7/8 of positions) ran
`peek_at`'s 2-byte slice loop into a 64-bit `u64` accumulator, shifted and
masked it to 8 bits, discarded the unused bits (most Rice quotients are
0–2 bits), and then `decode_rice_partition`'s `reader.read_bits(order)`
called `peek_at` a *second* time across the exact same bytes to read the
remainder.

**The fix (`crates/flac-lite/src/bits.rs`):** `BitReader` now carries
E2a's left-aligned, MSB-first `u32` refill window (`pos: usize`,
`acc: u32`, `valid: u32`; invariant `logical cursor = pos * 8 − valid`,
byte-at-a-time `refill()` while `valid <= 24`, register-shift fast path
for `read_bits(1..=31)`, `peek_bits(1..=32)`, and `byte_align()`).
`BitReader::rice_quotient` now refills `self.acc` when `self.valid < 8`,
indexes `LEADING_ONES[usize::from(!(self.acc >> 24) as u8)]` directly from
the top byte of the `u32` register, consumes `run + 1` via
`self.consume(run + 1)`, and leaves the buffered window hot in `self.acc`
for the immediately following `reader.read_bits(order)` remainder extract.

**Toolchain pin note (`agb_hashmap v0.25.0`):** `agb v0.25.0` enables
`agb_hashmap/allocator_api` (`#![feature(allocator_api)]` under
`-Zbuild-std=core,alloc`). Newer Rust nightlies (e.g. `1.101.0-nightly
2026-10-26`) split `core::alloc::Allocator` by adding an unstable
`AllocatorNightly` bound on `Vec::drain(..)` and moving `const fn
Vec::new_in` behind `#![feature(allocator_ext)]`, failing to compile
`agb_hashmap v0.25.0` with `E0277: the trait bound ALLOCATOR:
AllocatorNightly is not satisfied`. Pinning the host `nightly` toolchain
to the recorded **`nightly-2026-09-04`** (`rustc 1.100.0-nightly
(a69a63265 2026-09-03)`, LLVM 23.1.1) resolves the build and matches the
comparator toolchain across the entire measurement history.

**Gates & on-target result:** `make flac-test` (96 unit + 24 integration
tests, including `accumulator_window_invariants_hold_on_mixed_width_sweeps`),
`make spike-test` (15 + 8), `make rate-test` (27 + 9 + 8), and
`make check` all green. Built ELF witness: `LEADING_ONES` at `0x0300022c`
(`.iwram`); ROMs built: `flac-rate-play.gba` sha256 `2102c2a0…`,
`flac-rate-spike.gba` sha256 `67d0e2b0…`, `flac-spike.gba` sha256
`1f51ac8d…`. **Audible verification (Sam):** `flac-rate-play.gba` plays
`bioluminescent.wav` at 10,512 Hz at the **correct tempo and correct pitch
in real time** for the first time.

---

## In progress: 7-1, the product-play ROM on both units — four readings (2026-11-13)

**What was run.** `product-play.gba`, the item-7 product path: FIXED-0 / blocksize 2048 /
40.2 s / **32,768 Hz**, 160 frames, 326,144 samples per channel, through the 560-byte
planar ring and the timer0→timer1→timer2 chain, on **GBA 001** and **GBA SP**, **two runs
each**. This is the first time that path has been read off a unit rather than a log.

**The image was extended for this, and the extension's cost is measured on both channels.**
The 7-0 image printed 8 lines and an audit verdict (`aud NO`) with none of the clauses
behind it, and a unit has no serial. The extended image prints the other three per-frame
shares, the period ledger, the replay/late split (`fed`/`drain`) and the **period indices of
the late services** (`latep`/`worstp`/`dly`/`refus`) — 11 lines, and it measures its own
fit: `fit: lines=11 right=174 bottom=135 fits=true`. Cost against the 7-0 image: mGBA work
53.446% → 53.462% (+170 c/frame); hardware work 59.907/59.909% → 59.921% (+147 c/frame),
with min lead and decode unchanged — the two channels agreeing to ~20 c/frame. sha256
**`8113f04c…84cd`**, 1,104,592 B; gates 14/14 `product-play-test`, `check` clean,
`gafp-check` clean. **Recorded defect: the two readings taken *before* the extension (also
2026-11-13: work 59.907/59.909%, `late 1`, red on both units) have no image hash.** From
here the procedure takes the sha *before* the cart goes into a unit.

**The screens, as transcribed.** Four readings — GBA 001 twice, GBA SP twice — with every
number identical in all four, to the third decimal of every percentage, both lead
watermarks and the late index. Two transcription slips are kept as read and marked: `Lfr…`
at the head of one row, `worsp` in another.

```
PRODUCT PLAY 32768
fr 160 und 0 pcm ok fifo ok
ledg ok aud NO
push 326144 cons 326144
work 59.921% dec 42.288%
conv 14.228% push 1.397% feed 2.006%
minlead 8560 maxwk 60.738%
per 582 ser 582 tail 1/224 void 0
replay 0 late 1 fed 1 drain 0
latep 22 worstp 22 dly 23 refus 0
```

| field | all four runs | reading |
|---|---|---|
| frames / periods | 160 frames, `per 582 ser 582` | every period that elapsed was serviced |
| underflows, voids, refusals | `und 0`, `void 0`, `refus 0` | no starvation at any instant; every window measurable |
| sample accounting | `push 326144 cons 326144` | exactly the manifest total |
| digests | `pcm ok fifo ok`, `ledg ok` | exact over the bytes *pushed* — **not** over what the DMA/DAC played, see the caveat below |
| audit | `aud NO` | one clause refused it: `late 1` |
| replay behaviour | `replay 0 late 1 fed 1 drain 0` | one late re-arm in the fed phase, none in the tail |
| where | `latep 22 worstp 22 dly 23` | period 22 = audio sample 12,320 = **0.376 s** into the clip |
| work | 59.921%, worst frame **60.738%** | `maxwk` is per-frame: the worst frame is not the average |
| lead | min **8,560 samples = 15.3 periods = 261 ms** | the ring never came close to emptying |
| tail | `tail 1/224` | as designed — the clip ends 224 samples into a 560-sample period |
| rate contract | no `rate …!… pitch` row on any unit | Q14's identity observed on silicon: declared rate = played rate |
| screen colour | **RED** ("RED screen at the end, not blue") | `product_play_main.rs:1748` paints `COLOR_FAIL` when any of `ledger_ok`/`audit_ok`/`pcm_ok`/`fifo_ok` is false; here only `audit_ok` was, so the colour is the gate reporting the `late` clause, exactly as designed |

**Stage shares, hardware versus mGBA, same image.** Both are percentages of the same
1,048,576-cycle frame, so a percentage delta *is* a cycle delta. The instrument is
self-consistent on both sides: the four shares sum to 59.919% against the total's 59.921%,
as they do on mGBA (53.460 against 53.462).

| leg | mGBA | hardware | Δ c/frame | Δ |
|---|---|---|---|---|
| decode window | 39.577% (414,995) | 42.288% (443,422) | +28,427 | +6.85% |
| convert | 10.533% (110,447) | 14.228% (149,191) | **+38,744** | **+35.1%** |
| push | 1.382% (14,491) | 1.397% (14,649) | +158 | +1.09% |
| feeder | 1.968% (20,552) | 2.006% (21,034) | +482 | +2.35% |
| **work** | **53.462% (560,590)** | **59.921% (628,317)** | **+67,727** | **+12.08%** |

The legs sum to the total on both sides (Δ-stages +67,811 vs Δ-total +67,727, i.e.
rounding), so this is a leg-shaped gap and not a clock error. Read it against **E5** (the
ROM→IWRAM fetch pricing is hardware, and mGBA reproduces it to 0.012% per call) and
**6b-1** (the IWRAM-resident kernel matches mGBA to 0.055%): a generic "the cartridge is
slow" cannot carry a +35% convert penalty by itself. And read it against **6a-7b**, which
already measured every leg of a non-kernel path sitting 3.7–6.9% above the model on these
same units and named the flashcart as one of its candidates — the +6.85% decode window here
is inside that band. The leg with no company is convert, and the property that distinguishes
it in this image is that its writes go to the 40,320 B ring in **EWRAM**, a placement that
is default rather than chosen (there is no custom linker script in this repository).

**The one thing that refused, in the units the ROM states.** A `late` service is a re-arm of
the sound-DMA source landing more than 16 samples (**8,192 cycles**) after the timer1
overflow that ended the previous period. Here it landed **23 samples = 11,776 cycles** in —
**7 samples, 3,584 cycles past the window** — at **period 22**, with `gap == 1` (no period
skipped) and in the fed phase (`drain 0`), so the repeat controller had already reloaded the
old source and the audit's own definition calls it a mid-waveform splice. The field comment
records the first hearing of that defect class: 2026-10-19, "a pop in the audio that had not
been heard in any earlier run". **Whether a click is audible at 0.376 s has not been asked
yet.** What is *not* implicated is the duty cycle: a 1.4× miss against a latency window does
not care whether the budget is 20% or 60%.

**Why the identical index is the interesting part.** Same period, same delay, two runs, two
different units. This platform does that: 6a-7b's four runs on these same units were
identical *to the cycle*. So this is a deterministic scheduling collision, once in 582
periods, and the delay is interrupt **latency** rather than a slow handler (the handler's own
cost is ~285 c): something holds the timer1 IRQ pending for ~11.8 k cycles.
The leading candidate is in this design rather than in the hardware: **the ring push holds one
critical section per decode frame** (`product_play_main.rs:1267-1284`) — up to 2,048 samples
per channel copied in a single `push_bytes` under one `critical_section::with`, which in agb
means **IME = 0** (`agb/src/interrupt.rs:327-339`, `INTERRUPTS_ENABLED` = `0x04000208` =
REG_IME); every other critical section on the path only reads a static, and the decode and
convert legs are not masked at all. The stage instrument has been pricing that one section all
along: **14,649 c here**, 14,885–14,899 c per frame in 6b-3's rows with per-window worsts of
**15,776–15,831 c**, i.e. **28.6 samples of masked interrupts and 30.9 at the worst** against
the audit's **16-sample** tolerance for a late re-arm. That makes it a **margin defect rather
than bad luck: the design masks interrupts for about 1.8× the window in which the re-arm has
to land.** The arithmetic then reproduces the observed figure rather than merely tolerating it:
a boundary *d* samples into a push window is served when the window ends, so the late zone is
the window's first **12.6** samples and an observed delay of **23** samples puts that boundary
**≈5.6 samples into the window**, inside the measured envelope. What the model does *not* yet
explain is the **count** — 28.6 samples of mask per fed frame over 159 frames against a
560-sample grid suggests of the order of eight coincidences, not one — and the free test below
is what settles that too; the seam is written out in `plan-of-record.md`, 7-1's record, rather
than smoothed over. The ledger already contains a related count, recorded before it could be
read this way: 6b-3's rows note that tens-of-cycles shifts in the leg timings moved **exactly
one period boundary into a push window** (`boundaries_in 158`, "one boundary now lands in a
push window") while those runs printed `late=0`. Weaker candidate, not yet excludable: agb's
VBlank chain (`IE=0x0011` shows the bit enabled, and agb runs handler chains under a critical
section) — but no mixer is created here, the chain is probably empty, and whether a VBlank IRQ
is generated at all is a `DISPSTAT` bit-0 question the ROM does not print. The test that
settles it is nearly free: **attribute each late service to the leg it landed in**, since the
producer already measures the period range of each leg and the handler already records the
index of every late service. Prediction: `late in push 1`; if it comes back `late in push 0`
the search re-opens and the bus (Q15) moves up. The fix candidates, with their arithmetic,
belong to 7-2: `CHUNK`-sized push sections (≈7.2 samples of mask, a 2.2× margin), a lock-free
ring so no part of the copy is masked, or re-arming ahead of the boundary — and the 16-sample
clause that exposed all of this should not move whichever is chosen.

**A caveat that must travel with these rows.** `pcm ok fifo ok` claims exactness about the
bytes the player pushed into the ring and the converted FIFO bytes, not about what the DMA
and DAC played. A re-played stretch of the old buffer is compatible with every digest in this
table.
That is why the audit clause exists, and why 7-3 asks a person to listen.

**Sam's conclusions from this run, recorded as stated** — the arithmetic of a retarget, and
why the rung that is actually available is **16,384 Hz** rather than agb's 18,157, is in
`plan-of-record.md` under 7-1's first reading: the budget at 32,768 Hz is too tight to keep
optimising against; the emulator-versus-hardware gap is probably real and probably a property
of running from an SD-card flash cart rather than a mask ROM; a larger IWRAM-resident buffer
in the pattern of the FIFO is one way to design around that, but the simplest way to finish
is a lower rate; and **7-0/7-1 are to be re-run as necessary at the next rate down**.
Nothing in these four readings was a decision *at the time they were taken*: C2 still named
32 kHz until a ruling said otherwise, and the audit was not being relaxed. **The ruling came
later the same day**, after the second reading recorded below — C2 now asks for the highest rate
achievable rather than a committed 32 kHz, which changes the target and none of the clauses.



## In progress (part 2): the same run, now naming which leg held the re-arm (2026-11-13)

The four readings above left the failure named but not explained: `late 1`, `latep 22`,
`dly 23`, `fed 1`. The plan's next step was a screen that can say *which leg of the
producer held that re-arm*. Built, controlled, and this section records the control,
because the control itself answered two of the questions the four readings left open.

**The image.** `86f6a1dd7e525dfc14cac7ddac4151c4e860aafbf099b11174ea07ed9d08a082`,
1,106,440 bytes, built with `make native-product-play-rom`; `make check` exit 0,
`make product-play-test` exit 0 (14 passed), `gafp-check` clean (160 frames, 882,949 B).
Fit gate: `lines=11 right=188 bottom=135 fits=true`. The row is 14 characters wider than
the previous image's longest (`right=174`) and still inside the 240×160 screen — the headroom
is now ~50 px, about six characters, which is the budget the deferred rate/pitch line would
need. Three mGBA runs of this image agree to the cycle (work 560,677 c in all three), so
"the same image in the same emulator" is reproducible here even though hardware is the only
place the failure appears.

**The fields, and how each one works.**

| Field | What it is |
|---|---|
| `late in push a/b` | of `b` late services, `a` had their period boundary inside the producer's ring-push section |
| `2nd src a/b` | of `b` audio services, in `a` agb dispatched a second source in the *same* interrupt entry |
| `disp 0009` | the raw display-status register, printed not interpreted |
| `attr 422494` | the decode figure **minus** the interrupt time that fell inside its windows — the number the packer and the host model want, on the screen a unit can show |

The push attribution does not use a timing window or a tuned threshold. The push section
publishes the period counter as its *first act*, inside the critical section that is already
there; a ~29-sample section against a 560-sample grid can cross **at most one** boundary, so
`late service's period == the period the section published + 1` is an exact identification.
The second field uses what the interrupt dispatcher does to the enable mask: agb's
`interrupt_handler.s` masks out of the mask every source it is about to dispatch and restores
it on the way out, so the mask *as the audio handler sees it* is "what is armed, minus what
came with me". One `io_read_u16` per service. `mask_seen_by_audio=0x0001` over the whole run
says that bit was never cleared from the mask, i.e. it was armed and did not fire in that
entry; the paired count is the complement, because without it a zero cannot be told apart
from "not armed at all".

**Cost.** work/frame 560,590 → 560,677 c in mGBA: **+87 cycles a frame, +0.008 pp**
(work 53.462 → 53.470%). The decode figure is unmoved (isolated estimate 422,487 → 422,494,
+7 c), the minimum lead is unmoved (9,120 samples), and the push stage itself is unchanged
(14,491 → 14,942 c is the section's own cost, which the new instrument only reads). So the
instrument that decides the mechanism costs less than a hundred cycles a frame — one store
inside a critical section that exists anyway, one `io` read and a compare in the handler, and
the subtraction that turns the window mean into the isolated figure, which is computed once
and feeds the log line, the cross-check and the screen alike.

**Free finding 1: the count that looked impossible is not impossible.** The previous section
asked why the model expected "of the order of eight" late re-arms but the units reported one.
The new counter answers it: in mGBA, `boundaries_inside_push_section` is **2** in the previous
build and **0** in this one, against a nominal expectation of 159 windows × 28.6 samples ÷ 560
= **8.1**. The producer is not in random phase with the audio grid: `wait_for_room` parks it
until the ring has drained, so its push windows sit at a nearly fixed phase of that grid and
boundaries cross them occasionally, not at the random rate. And the count moves when the code
layout moves — those two builds differ in nothing but log strings and instruction addresses —
which is also why the decode stage's boundary count here is 161 and the earlier build's was
160. So "one late service in 582 periods" is not a statistical oddity: it is one boundary
crossing, which is what a phase-locked producer does.

**Free finding 2: a second interrupt source cannot hold a re-arm for 23 samples.** In mGBA the
count is 1 audio entry in 582 that shared an entry with the other armed source, and `late=0`.
For a shared entry to be what made period 22 seventeen samples late, that entry would have to
have cost ≥ 11,776 cycles — about **five times the measured cost of this ROM's entire audio
handler** (2,286 c mean, 2,999 c worst). And this build registers exactly one interrupt
handler, the feeder (`add_interrupt_handler(Interrupt::Timer1, …)`), so a shared entry contains
no project work of its own. What is *not* excluded is a stall on the bus or in memory (Q15):
that is not an interrupt-entry effect and this field cannot see it. The field is still worth
having, because a non-zero count on a unit would be evidence against the model rather than for
it — but the leading candidate stays the ring-push section, whose measured cost (14,649 c,
28.6 samples) is comfortably above the 16-sample grace.

**What the unit run will decide, in advance.** With the same `late 1`:

- `in_push 1` — the boundary that was missed crossed inside the push section. The mechanism is
  named, and 7-2's fix is to stop masking interrupts for 28.6 samples: push in `CHUNK`-sized
  critical sections (≈7.2 samples, 2.2× margin — superseded by 7-1's measured window: **4,014 c =
  3.92 samples at 16,384 Hz, a 4.08× margin**, 7.84 / 2.04× here), or make the ring lock-free and push with
  interrupts on.
- `in_push 0` — it did not. Then the push-section story is dead for that event and Q15's bus
  leg moves to the front, with the placement control (ring in EWRAM vs ring in IWRAM at 20
  chunks = 22,400 B, which fits) as the test.
- `boundaries_inside_push_section = 0` while `late ≥ 1` — a third answer, and a clean one: no
  boundary was inside any push section, so no late service can have been caused by one.

The reading to ask for on both units, twice each, is `late in push a/b`, `2nd src a/b`, `disp`,
`attr`, and the unchanged audit block. Ask at the same time whether anything is audible at
0.376 s — period 22 is the first quarter-second of the clip, and whether a click is there is
part of 7-3's witness, not a separate question.

## 7-1, second hardware reading (2026-11-13): the push section held it, and the rate is ruled

**Image.** `product-play.gba`, sha256 **`86f6a1dd7e525dfc14cac7ddac4151c4e860aafbf099b11174ea07ed9d08a082`**,
1,106,440 bytes, md5 `9861b47b0116183e05c21018dc6776df`, built from source commit **`ad37932`**
and rebuilt before flashing to confirm the bytes reproduce. The sha was recorded before the cart
went in, and the copy in the unit matches it by md5 after transfer. Four runs: GBA 001 ×2,
GBA SP ×2.

**The four screens as read.** Reproduced with the transcription slips left in, because they are
part of the evidence about how these were taken — `Lfr`/`Ledg`/`Latep` capitalisation, one
missing `ser 582`, one `0/562`, one `drain -`. The four agree everywhere else, exactly.

```text
GBA 001 (1)                          GBA 001 (2)
PRODUCT PLAY 32768                   PRODUCT PLAY 32768
Fr 160 und 0 pcm ok fifo ok          Fr 160 und 0 pcm ok fifo ok
Ledg ok aud NO push 326144           Ledg ok aud NO push 326144 cons 326144
     cons 326144                      Work 59.946% dec 42.292% attr 451763
Work 59.946% dec 42.292% attr 451763 Conv 14.231% push 1.400% feed 2.021%
Conv 14.231% push 1.400% feed 2.021% Minlead 8560 maxwk 60.755%
Minlead 8560 maxwk 60.755%           Per 582 tail 1/224 void 0
Per 582 ser 582 tail 1/224 void 0     [ser 582 not transcribed]
Replay 0 late 1 fed 1 drain 0        Replay 0 late 1 fed 1 drain 0
Latep 22 worstp 22 dly 25 refus 0    Latep 22 worstp 22 dly 25 refus 0
Late in push 1/1 2nd src 0/562       Late in push 1/1 2nd src 0/582
     disp 0008                            disp 0008
playback                             Playback

GBA SP (1)                           GBA SP (2)
PRODUCT PLAY 32768                   PRODUCT PLAY 32768
Fr 160 und 0 pcm ok fifo ok          Fr 160 und 0 pcm ok fifo ok
Ledg ok aud NO push 326144           Ledg ok aud NO push 326144 cons 326144
     cons 326144                      Work 59.946% dec 42.292% attr 451763
Work 59.946% dec 42.292% attr 451763 Conv 14.231% push 1.400% feed 2.021%
Conv 14.231% push 1.400% feed 2.021% Minlead 8560 maxwk 60.755%
Minlead 8560 maxwk 60.755%           Per 582 ser 582 tail 1/224 void 0
Per 582 ser 582 tail 1/224 void 0    Replay 0 late 1 fed 1 drain -
Replay 0 late 1 fed 1 drain 0        Latep 22 worstp 22 dly 25 refus 0
Latep 22 worstp 22 dly 25 refus 0    Late in push 1/1 2nd src 0/582
Late in push 1/1 2nd src 0/582            disp 0008
     disp 0008                        Playback
Playback
```

`drain -` is read as `drain 0`: every other row on that screen is identical to the three that
say `drain 0`, and `fed 1` with `late 1` leaves nothing else to be. Same for the missing
`ser 582` and the `0/562`, which the other three screens and `Per 582` on that very screen
contradict. Recorded rather than corrected silently.

**What it establishes: the attribution came back 1 of 1, four times.**

| Field | Read | Meaning |
|---|---|---|
| `late in push 1/1` | **1 of 1** | the one late service's period boundary crossed inside the producer's ring-push critical section |
| `2nd src 0/582` | **0** | no audio entry ever shared an interrupt entry with another source on a unit |
| `disp 0008` | bit 0 **clear** | the display was not generating that interrupt at all on real hardware |
| `latep / worstp` | **22 / 22** | the same period index the first reading gave, on a different image |
| `dly` | **25 samples** | 12,800 c, against a 16-sample / 8,192-c grace — **9 samples, 4,608 c late** |
| `attr` | **451,763 c** | the first *attributed, isolated* decode figure ever read off a unit |
| `late 1 fed 1 drain 0` | unchanged | still one, still in the fed phase, so still a possible mid-waveform splice |

The mechanism is no longer a hypothesis. The push section publishes the period counter as its
first act, so "this service's boundary was inside that section" is an identity rather than a
timing window, and it matched on every run on both units. The competing candidate died the same
way from two directions: the co-entry count is zero, and `DISPSTAT` bit 0 — the enable that
makes such a request possible — was clear the whole time. **Note which channel got this wrong:
mGBA reported `dispstat_now=0x0009` and `2nd src 1/582`, i.e. the emulator had VBlank behaviour
the real hardware did not.** An interrupt-topology question cannot be settled by an emulator
control run, in either direction.

**The delay moved, the index did not.** 23 samples on image `8113f04c…`, 25 on `86f6a1dd…`, same
period, same classification, +1,024 cycles — on a build whose producer code differs in log
strings and addresses. That is what a phase-locked collision looks like: the *instant* is a
property of the schedule and the *held time* follows wherever the push window lands. It also
prices the instrument honestly and unflatteringly: adding one store inside the push critical
section made the defect it measures two samples worse.

**First hardware attributed decode, and how to read it.** `attr 451,763 c/frame` = **43.08%** of
the 32,768 Hz frame. Against mGBA's same-image `422,494` it is **+29,269 c, +6.93%** — landing
at the top of the 3.7–6.9% non-kernel band 6a-7b measured on these units. Against 6b-2's row of
record (396,066 mean / 398,255 worst) it is +14.1%, and that gap is **instrument, not silicon**:
6b-2 priced the kernel in a decode slot with nothing else running, this prices the decode leg of
a live producer with ring writes and a timer interrupt inside its windows. Both are correct
figures of different things; only this one describes the product. This is the number the
packer's structural `fixed0 @ b2048` row has been waiting for, and it is now readable from a
screen.

**Cost of the second instrument, on both channels.** Against `8113f04c…`: hardware work 59.921 →
**59.946%** (+262 c/frame), maxwk 60.738 → 60.755%, dec 42.288 → 42.292%, conv 14.228 →
14.231%, push 1.397 → 1.400%, feed 2.006 → 2.021%; min lead, all counts, every digest, `void 0`,
`refus 0` and the late index **unchanged**. In mGBA the same extension cost **+87 c/frame**. So
the instrument is ~3× more expensive on silicon than in the emulator — +262 c/frame for one
store, one `io` read, a compare and a subtraction a frame — which is a Q15-sized observation in
its own right, and small enough not to matter at 0.025 pp. The four stage shares still sum
self-consistently: 42.292 + 14.231 + 1.400 + 2.021 = **59.944** against a reported **59.946**.

**And the conclusion the run actually forces.** The refused clause is now fully explained and
fixable, and the run still costs **59.946% of the frame**, worst frame **60.755%**, on a
**FIXED-0** stream — a clip with no prediction at all, which is the cheapest thing FLAC can
emit and is not what real music censes. Fixing the push mask removes the only thing this screen
refused; it removes none of the 60%. Given no strategy for driving an attributed 43%-of-frame
decode and a 14%-of-frame convert down by the factor that would make the product able to carry
predicted frames, the owner ruled on 2026-11-13: **drop the sample rate, and relax C2 from
real-time playback at ~32 kHz to real-time playback at the highest rate achievable.** Recorded
in full in `plan-of-record.md` under "The rate ruling", including what it buys (halved
percentages, LPC from 173% to 86.6% of the frame, the IWRAM ring becoming arithmetically
possible) and what it pointedly does not buy (**at 16,384 Hz this same 14,680-cycle mask is a
14.3-sample window against a 16-sample grace — inside the tolerance by 1.7 samples on the mean,
and by 0.5 on the worst push window this repository has ever recorded for that section**). The
rate question is closed — **and the rung was then chosen as 16,384 Hz** (`2²⁴/1024`, exact, so
Q14's identity rule carries across with nothing relaxed; agb's ~18 kHz was considered and declined
the same day, on `late in push` arithmetic that a faster clock makes *worse*, not better);
7-2's push fix, the retarget,
the packer row, the host fixture, and the listening witness are all still open — and the
listening question at 0.376 s is still unanswered, now with a mechanism attached to it: at
period 22 the re-arm was held 25 samples by the producer's own copy into the ring.

## 7-1, retarget hardware reading (2026-11-13): 16,384 Hz is green on both units

**Image.** `product-play.gba`, the 16,384 Hz FIXED-0 product image from PR #124,
sha256 **`ef1ed8baf0feadb696e0b0c5e3fb24c92ced31a6fbab6d40edfb5ce45b789351`**,
662,840 bytes. Two runs on **GBA 001** and two on **GBA SP**, all four ending on a
**BLUE screen**. The readings are the first hardware run of the retargeted path:
80 frames / 162,304 samples, 289 periods, the shipped 439,396 B / 16,384 Hz GAFP
asset.

**The four screens as transcribed.** All four agree on every measured field. Four
transcription slips are kept as read and marked rather than silently corrected:
`Work 28/450%` (slash for decimal point), `cubs` (`cons`), `drain -` (the other
three rows say `drain 0`), and `PRODUCT PLAY 16374` (the header on the other three
rows says `16384`; the ROM's own header is fixed-width and the measured fields on
that screen are identical to the other three).

```text
GBA 001 (1)                          GBA 001 (2)
BLUE SCREEN                          BLUE SCREEN
PRODUCT PLAY 16384                   PRODUCT PLAY 16384
Fr 80 und 0 pcm ok fifo ok           Fr 80 und 0 pcm ok fifo ok
Ledg ok aud ok push 162304           Ledg ok aud ok push 162304 cons 162304
     cubs 162304                      Work 28.450% dec 19.961% attr 446306
Work 28/450% dec 19.961% attr 446306 Conv 6.824% push 0.666% feed 0.997%
Conv 6.824% push 0.666% feed 0.997%  Minlead 9120 maxwk 29.613%
Minlead 9120 maxwk 29.613%           Per 289 ser 289 tail 1/464 void 0
Per 289 ser 289 tail 1/464 void 0    Replay 0 late 0 fed 0 drain 0
Replay 0 late 0 fed 0 drain -        Latep 0 worstp 0 dly 0 refus 0
Latep 0 worstp 0 dly 0 refus 0        Late in push 0/0 2nd src 0/289
Late in push 0/0 2nd src 0/289            disp 0008
     disp 0008                       Playback
Playback

GBA SP (1)                           GBA SP (2)
BLUE SCREEN                          BLUE SCREEN
PRODUCT PLAY 16384                   PRODUCT PLAY 16374
Fr 80 und 0 pcm ok fifo ok           Fr 80 und 0 pcm ok fifo ok
Ledg ok aud ok push 162304           Ledg ok aud ok push 162304 cons 162304
     cons 162304                      Work 28.450% dec 19.961% attr 446306
Work 28.450% dec 19.961% attr 446306 Conv 6.824% push 0.666% feed 0.997%
Conv 6.824% push 0.666% feed 0.997%  Minlead 9120 maxwk 29.613%
Minlead 9120 maxwk 29.613%           Per 289 ser 289 tail 1/464 void 0
Per 289 ser 289 tail 1/464 void 0    Replay 0 late 0 fed 0 drain 0
Replay 0 late 0 fed 0 drain 0        Latep 0 worstp 0 dly 0 refus 0
Latep 0 worstp 0 dly 0 refus 0       Late in push 0/0 2nd src 0/289
Late in push 0/0 2nd src 0/289            disp 0008
     disp 0008                       Playback
Playback
```

`drain -` is read as `drain 0`: `fed 0` and `late 0` leave no other reading, and
the other three screens on the same image say `drain 0`. The `16374` header is read
as `16384` for the same reason: the ROM prints the rate from
`geometry::PLAYBACK_RATE_HZ`, the same field the `rate:` line and the manifest
report as 16,384, and every other field on that screen is identical to the other
three.

| field | all four runs | reading |
|---|---|---|
| frames / periods | 80 frames, `per 289 ser 289` | every elapsed period was serviced |
| underflows / voids / refusals | `und 0`, `void 0`, `refus 0` | no starvation; every window measurable |
| replay / lateness | `replay 0 late 0 fed 0 drain 0`, `dly 0` | **no late re-arm on this image at this rate, on either unit** |
| sample accounting | `push 162304 cons 162304` | exactly the manifest total |
| digests | `pcm ok fifo ok`, `ledg ok` | exact over the bytes pushed and the converted FIFO bytes; not over what the DAC played |
| audit | `aud ok` | all audit clauses pass |
| attribution fields | `late in push 0/0`, `2nd src 0/289`, `disp 0008` | no late section to attribute; no shared interrupt entry; display VBlank request bit clear |
| work | **28.450%**, worst frame **29.613%** | 596,640 c/frame mean, 621,030 c/frame worst, against the 2,097,152-cycle frame |
| decode | `dec 19.961%`, `attr 446306` | 418,613 c/frame window mean; **446,306 c/frame** attributed, isolated decode |
| convert / push / feeder | 6.824% / 0.666% / 0.997% | 143,110 / 13,967 / 20,909 c/frame; the four shares sum to 28.448% against the 28.450% total (rounding) |
| lead | `minlead 9120` | 9,120 samples = **16.3 periods = 0.557 s**; the ring never approached empty |
| tail | `tail 1/464` | the designed short final period |
| rate contract | no `rate …!… pitch` row | declared 16,384 Hz = played 16,384 Hz on every run |
| screen | **BLUE** on all four | the combined ledger/audit/PCM/FIFO gate passed |

**Same-image hardware/minus-mGBA, per leg.** Both sides divide by the same
2,097,152-cycle frame, so a percentage delta is a cycle delta. The mGBA control is
the same image (`ef1ed8ba…`), run as `fr 80 und 0`, `per 289 ser 289`, `late 0`,
`work 25.363%`.

| leg | mGBA | hardware | Δ c/frame | Δ |
|---|---:|---:|---:|---:|
| decode window | 18.677% (391,685) | 19.961% (418,613) | +26,928 | +6.87% |
| convert | 5.052% (105,948) | 6.824% (143,110) | **+37,162** | **+35.08%** |
| push | 0.662% (13,883) | 0.666% (13,967) | +84 | +0.60% |
| feeder | 0.971% (20,363) | 0.997% (20,909) | +545 | +2.68% |
| **work** | **25.363% (531,901)** | **28.450% (596,640)** | **+64,739** | **+12.17%** |
| attributed decode | 417,431 c | **446,306 c** | +28,875 | +6.92% |

This is the same leg-shaped gap the 32,768 Hz reading measured: the decode window
is inside the 3.7–6.9% non-kernel band 6a-7b established, and **convert is again
the outlier at +35%**. The four shares sum to 28.448% against the reported 28.450%
(hardware) and 25.362% against 25.363% (mGBA), so the instrument is
self-consistent on both sides. Q15 is not retired by this run; it is re-confirmed
at the new rate, where the convert leg is about 7% of the frame instead of 14%.

**What this establishes, and what it does not.**

- **It establishes 16,384 Hz on hardware**, on both units and twice each, with
  zero underruns, zero late services, all 289 periods serviced, exact sample and
  digest pins, and the rate identity observed on silicon (no rate/pitch row).
  The product path now has a hardware reading at the rate of record, not only an
  emulator projection.
- **`attr 446,306 c/frame` is the new hardware attributed decode figure.** At
  16,384 Hz it is 21.28% of the frame. It is the live-producer figure, not 6b-2's
  decode-slot row (396,066 / 398,255 at 32,768), and the two must not be averaged;
  6b-3 refused that comparison once and the refusal still stands.
- **`late 0` four times does not close 7-2.** The push section still masks
  interrupts for the measured 14,680 cycles: that is **14.3 samples** at
  1,024 ticks/sample against the **16-sample** grace on the mean, and **15.46
  samples** on the worst push window ever recorded for that section — inside by
  only 0.5 samples on that worst case. This run did not hit the phase-locked
  collision that the 32,768 Hz image hit at period 22. The chunk-sized push fix has
  since landed in code (the 7-2 control below); its hardware re-run is the remaining
  7-2 step. The retarget lowered the probability and consequence of a miss, it did
  not remove the masked section by itself.
- **It is not 7-3/C2.** The hardware count/digest/late clauses are met on this
  image, but the human listening witness — pitch and tempo, heard on both units
  with the source beside it — has not been done, and 7-2's fix is not in this
  image. C2 remains unmet until 7-3 passes on both units.

## 7-2 implementation control (2026-11-13, mGBA headless — not hardware): chunked push

**The change under test.** `Producer::push_frame_in_chunks` appends a frame in slices of at
most `CHUNK_SAMPLES = 560`, each under its own critical section, instead of one
interrupt-masked 2,048-sample copy. The measured whole-frame mask was 14,680 c (14.3 samples
at this clock, worst recorded 15.46) against the 16-sample re-arm grace; a chunk-sized slice
is about 4,014 c = 3.92 samples, a 4.08x margin.

**Image.** `product-play.gba`, sha256
**`2abf61e8b18f4bac46ed680d2320f1b9237b3586e35e3f167f8aef2885eab5ff`**, 662,440 B. One
headless mGBA run:

```text
fr 80 und 0 pcm ok fifo ok
per 289 ser 289 tail 1/464 void 0
replay 0 late 0 fed 0 drain 0
late in push 0/0 2nd src 0/289 disp 000A
work 25.507% dec 18.678% attr 417451
conv 5.051% push 0.805% feed 0.971%
minlead 9680 maxwk 26.727%
verdict PASSED — screen BLUE
```

The price of splitting the mask is the push stage's whole-sequence mean: **14,817 →
18,018 c/frame** (four sections instead of one), work **25.363 → 25.507%** (+0.144 pp),
worst frame **26.583 → 26.727%**. Decode (417,826 c mean, 417,451 attributed) and convert
(113,011 c) are unchanged; every count, every digest, `void 0`, `refus 0` and `late 0` are
unchanged. The host envelope was re-read from this run (`make product-play-test` 14/14).

**Not a hardware result.** The 7-1 retarget hardware reading above belongs to the previous,
whole-frame-push image `ef1ed8ba…`; this image has not been on a unit. The hardware re-run
(name the sha before flashing, both units twice, read the same late-attribution fields) is
what closes 7-2, and then 7-3 asks the listening question.
