# Measurements and method — the FLAC ledger

> **Scope: the FLAC track's numbers and the rules that make one quotable.** Every
> figure here belongs to `crates/flac-lite`, `crates/flac-lite-gba`,
> `examples/flac_integration`, `examples/flac_spike` and the `flac-*`/`product-play`/
> `gafp`/`overlay` Make targets; "the product path" means the **FLAC** one. Nothing
> here prices, permits or forbids a codec outside FLAC — the sibling tracks keep their
> own ledgers ([`../ULI2.md`](../ULI2.md), [`../ULC.md`](../ULC.md),
> [`../OPUS.md`](../OPUS.md)). The *method* is deliberately transferable and is offered
> as something the other tracks may copy, not as a rule that binds them.
>
> The narratives behind these rows — the E-series probes, the item-by-item ladder, the
> hardware readings as they happened, and the questions that opened — are in
> [`history.md`](history.md). This file keeps what each number *is*.

The rule everything else is a special case of:

> **A claim carries its witness.** For a format fact, an RFC section or encoder bytes.
> For a performance fact, a same-image control, a named image, a named toolchain
> instance, and the arithmetic that turns a raw counter into the printed number. For a
> correctness fact, a witness the implementation could disagree with — ideally a second
> mechanism, never a transcription of the first.

## The budget

Two clocks are in use in this tree and every percentage depends on which one a figure
divides by:

```text
2048 / 16384 s × 16,777,216 = 2,097,152 cycles/frame = 1024 c/sample   ← the GBA's real clock (2^24)
2048 / 16384 s × 16,780,000 = 2,097,500 cycles/frame = 1024.15 c/sample ← the spike instrument's
```

`examples/flac_spike/src/stats.rs::GBA_CLOCK_HZ` is the rounded **16,780,000**, so the
`verdict:` lines and the E-record tables divide by 2,097,500; `scripts/gafp_pack.py`,
`examples/flac_integration/src/geometry.rs` and every share quoted in
[`../FLAC.md`](../FLAC.md) divide by the exact **2^24** period. The two differ by 348
cycles (0.0166%), which shifts a three-decimal share by ~0.03 pp and changes no verdict —
but it is why a share stated to three decimals must name its denominator, and why the
packer's 0.1 pp row-consistency tolerance is deliberately wider than the gap.

Derived from `PLAYBACK_RATE_HZ` in `geometry.rs` (one constant moves all of these):

| Quantity | Value |
|---|---|
| ticks/sample (`MIXER_TICK_HZ / rate`) | 1,024 — exact, which is why the rate is 16,384 and not 18,157 |
| frame (2048 samples) | 2,097,152 cycles = 125 ms |
| period (`CHUNK_SAMPLES` = 560) | 573,440 cycles |
| re-arm grace (`REARM_GRACE_SAMPLES` = 16) | 16,384 cycles |
| ring (`RING_CHUNKS` 36 × 560, per channel) | 20,160 samples = 0.615 s of lead |
| prefill (`PREFILL_CHUNKS` 16) | 8,960 samples |

Decode cost is paid **per sample**; the budget is granted **per second**. That is the
whole reason a lower rate is a lever: cycle counts do not move with the clock, only the
denominator does. Halving the rate halves every percentage.

## The numbers of record

**Playback, hardware, 16,384 Hz.** GBA 001 ×2 + GBA SP ×2 per row, all runs in a row
identical to the cycle, all ending BLUE with `pcm`/`fifo`/`ledg`/`aud` ok.

| Asset / build | work/frame | worst frame | decode | `attr` (attributed decode) | convert | push | feeder | lead floor |
|---|---|---|---|---|---|---|---|---|
| Committed synthetic FIXED-0 asset, whole-frame push | 28.450% | 29.613% | 19.961% | 446,306 c | 6.824% | 0.666% | 0.997% | 9,120 |
| …same, `CHUNK`-sized push (the shipped shape) | **28.600%** | **29.781%** | 19.961% | 446,292 c | 6.825% | 0.816% | 0.997% | 9,120 |
| **Real audio through the master path (C2's run)** | **32.208%** | **35.831%** | 23.656% | 527,339 c | 6.733% | 0.820% | 0.997% | 9,120 |

All three: `per 289 ser 289`, `und 0`, `late 0`, `late in push 0/0`, exact sample counts
and digests. The master asset is the first 9.9 s of `bioluminescent.wav` — 80 frames,
162,304 samples, census `constant ×4, fixed1 ×102, fixed2 ×53, fixed3 ×1`. The
`2nd src 2/289` on that run (against `0/289` for the synthetic asset) is recorded and
unexplained; it caused no lateness.

**Decode alone.** These were taken at the 32,768 Hz-era framing and are
sample-denominated, so they hold at any rate — divide by 2 where a percentage is quoted
at the product clock.

| Path | mean / worst c/frame | Share of a 16,384 Hz frame |
|---|---|---|
| Product path through the real manifest, `rice-arm` (6b-2 pt 2, hardware, the decode row of record) | **396,066 / 398,255** | 18.88% / 18.99% (193.4 c/sample) |
| Same image, core's default `TableCountKernel` | 1,737,135 / 1,745,843 | 82.8% / 83.2% — the kernel is worth **4.39×** on the product path |
| Instrument (decode slot, kernel + i32, mGBA, 6a-7) | 416,804 / — | 19.9% |
| …same on hardware (6a-7b) | 445,558 mean, worst 42.72% of a 32,768 Hz frame | 21.2% / 21.4% |
| LPC `-l 4`, instrument, hardware (6a-7b) | 173.23% mean / 174.14% worst **of a 32,768 Hz frame** | **86.6% / 87.1%** — why LPC is refused at pack time |

The 6b-2 and 6a-7b rows were taken on the pre-retarget frames (same shape, different bytes);
that is exactly what the packer's provenance warning says out loud on every pack today. Their
cycle counts are sample-denominated, which is why they remain the decode rows — but a new
number measured on the current asset should displace them, not be averaged with them.

The three ways a decode figure can be stated, and they are **not** interchangeable:
the decode-slot row (396,066 — nothing else running), the **attributed isolated** figure
(`attr`, 446,292–527,339 — the producer's decode leg with ring writes and timer IRQs
inside its windows, priced out of the interrupt time), and the **window mean** (`dec`,
19.96–23.66% — the same leg measured in a live player). The packer's structural row and
the host model's envelope want `attr`. 6b-3 refused to reconcile 436,756 with 422,248 for
the same asset, and that refusal is still right: do not quote one against the other as if
a kernel had got slower.

**The instrument baseline.** `examples/flac_spike` in the shape the decoder ships in is the
`ctrl` arm of every record. Re-measured on this host at the current tree (2026-11-14, after
the 16,384 Hz retarget): `.text` 50,132 B · `.rodata` 990,464 B · `.iwram` 812 B · `.ewram`
368 B. The product images built here today: `flac-integration.gba` 644,340 B (`.text` 68,528,
`.iwram` 2,036), `product-play.gba` 662,440 B (`.text` 81,176, `.iwram` 2,036 = 812 core + 544
overlay tables + 680 asm body, `.bss` 42,608 for the ring), `overlay-kernel-check.gba` 221,020 B.
The clips are now **80 frames / 162,304 samples per channel at 16,384 Hz** (`-l 0` census
`fixed0 ×80` on both slots; `-l 4` census `lpc3 ×1, lpc4 ×79`), and both arms decode to one
PCM pin: FNV-1a-64 **`0xEB3EA1C24FE1574F`**, matched on host and in the ROM and cross-arm. The
`0x54C7B356621B6E15` pin and the 157-frame / 320,000-sample clips in the older rows below are
the pre-7-0 asset: still the numbers of record for the runs that produced them, and not the
bytes this tree embeds.
**Never treat a section size or an address as a constant across trees** — `.rodata` carries the
embedded clips and absolute panic locations, so it moves with the asset and with a comment
edit. `LEADING_ONES` relocates when a second `RiceKernel` body becomes reachable, with its
bytes unchanged.

## Rules every run follows

- **Correctness before speed.** No number is reported from an image whose decode has not
  been proven correct on its own embedded bytes (host bit-exactness against `flac -d` plus
  the on-target PCM pin). A fast wrong decode measures nothing.
- **One variable per run.** A speed change that keeps `make spike-test` green changed
  *how*, never *what*.
- **Same image or nothing.** A comparison is valid only between arms measured in one
  session from one image with a same-run control. Cross-record comparisons are allowed
  only as absolute paired marginals (cycles/frame, cycles/sample) — never as percentages
  of budget, and never as "X% faster" against another record's denominator. Percentages
  are era-denominated: 6a-2 re-ran old patches on a new base and moved LPC percentages by
  up to three points with an unchanged decode path.
- **The exception, measured.** An *absolute* arm that reproduces across named images to a
  stated spread is quotable across records (the landed `asm` leg reproduced to 0.04%
  across three links; E10's vendored leg to −0.02%). That is why 6a-5's cross-record
  projection came out right to 0.4%. The rule is not "never compose"; it is *name the
  image each number came from and know whether the absolutes reproduce*.
- **Measure the net, report the raw.** Every window is net of a calibration taken in the
  same image and the same shape, and calibration windows must be byte-identical to each
  other. All bookkeeping and logging stays outside the window (a serial write inside a
  window measures the logger); IRQs off, with IME witnessed 0 inside.
- **Determinism is a witness, not an assumption.** Two headless runs of one ROM `cmp`
  byte-identical, including every `***` cycle line. The same control holds on real
  hardware: four runs across two units, identical to the cycle, and identical in the
  index and delay of a single one-in-582 event — which is what makes an index readable
  and what points at a fixed phase relationship rather than at noise.
- **Bounds on what a re-run can hide.** A fixed image re-runs to **zero** difference
  (proved by a docs-only control whose `.text` was byte-identical). A *recompile* moves
  absolutes by a fixed, deterministic per-frame amount — adding one timed window to the
  same source moved every absolute leg of both clips by exactly +15,386 cycles (+98
  c/frame on all 314 frame-legs, mins and maxes alike) while every *paired* delta stayed
  cycle-identical. So the ~9–13 c/frame layout band is a band on paired deltas, pairing
  cancels the offset rather than merely reducing it, and an absolute arm compared across
  two images can be off by a fixed amount with no link mechanism behind it.
- **The tail is first-class.** One over-budget frame is an audible glitch: report min, max
  with its frame index, sum and mean. A worst-frame number without its arm and index is
  quoting a different frame.
- **Say what the command varied, not only what it printed.** A "measured" claim that
  omits the variable is unfalsifiable, and a zero-returning throwaway script can report a
  fact that merely agrees with the prose being checked.
- **A second mechanism, not a second transcription.** Host and ROM run the same fold over
  the same bytes; oracles are written independently (naive bit-walk vs accumulator reader,
  Python big-int vs i64). "Copy the oracle's output" is still a transcription — the
  hand-typed stereo oracle diverged on one block and a mechanical re-check caught it.
- **Rejection paths need mutation witnesses.** libFLAC never emits an invalid stream, so
  every refusal test mutates one field of a *real* vector and asserts the specific error
  variant. Untouched bytes keep the widths honest.
- **Probes live in `examples/flac_spike/experiments/<name>/`.** The spike's `src/` is the
  cross-session instrument; a probe never lands on it. (The one exception, 6a-5b's vendored
  leg, is recorded in [`history.md`](history.md).)

**The false-green catalogue** — each of these cost real time, and each is why a rule
above exists: a hand-packed test vector recorded the author's belief instead of the
format (it fired four times, twice surviving review); `checked_shl` guards the shift
amount and wraps the value; a hand-computed "published" FNV vector was wrong from memory;
a test that re-parsed the pristine frame asserted `Ok` from the mutation; a generated
`\uXXXX` escape is valid JSON and invalid Rust; a gate that "runs" can collect zero tests;
and the mirror image of all of them, a negative composed by adding one record's paired
marginal to another record's absolute on the page that states the rule against it.

## Taking a cycle number with the spike

The instrument is `examples/flac_spike` (`make spike-rom`, then
`timeout -k 5 280 mgba-test-runner flac-spike.gba | grep -E 'stats:|budget:|verdict'`).
Its entry points, and the two that must not be confused:

- `timing_pass()` — the per-frame loop. The closure handed to `timed()` is exactly what is
  measured, and nothing else is inside the window.
- `timed()` — window mechanics: an IME-off critical section and a composite timer read
  (timer2 at /1 cascading into timer3, read low–high–low with a stability recheck). A window
  that outlives the composite saturates to `stats::WORST_WRAPPED` and fails loudly rather than
  reporting a modulo. `CALIBRATION_SAMPLES` sits just above it.
- `stats::real_time_budget(blocksize, rate)` — the one implementation of the budget, evaluated
  by the ROM at compile time; `BUDGET_CYCLES_PER_FRAME` is that at (2048, 16,384).
- **Never touch `src/assets.rs` or the committed `.bin`s from an experiment.** The region pins
  gate every layer above them, and the whole cross-session ledger is anchored on those bytes.

The reported verdict ANDs the budget check with the correctness gates, so a blue screen means
"these numbers come from an image whose assets, decode proofs and calibration all passed" —
not "this fits in a frame". The budget verdict is on the serial line.

## mGBA versus silicon

mGBA is a screen for a layout change and a bad source of an absolute. Measured on the same
images:

| Leg | Hardware minus mGBA |
|---|---|
| IWRAM-resident ARM kernel, product path (6b-1, 6b-2 pt 2) | **+0.055%** — a constant across two image generations |
| ROM-resident Thumb default kernel, same binaries | +4.03%, then +3.25% at new bytes |
| Whole producer chain at 32,768 Hz (7-1) | work **+12.08%**: decode +6.85%, push +1.09%, feeder +2.35%, **convert +35.1%** |
| Whole producer chain at 16,384 Hz (7-1 retarget, 7-2) | work **+12.1%**: decode +6.87%, **convert +35.1%**, `attr` +6.91% |

So: **use a hardware figure where one exists; where none exists, carry the mGBA figure
with a +7% derating and say so.** The shortfall is per-leg and does not cancel, so one
arm's mGBA number may not be scaled onto another arm either, and a per-leg derate is a
property of an image rather than of a leg — 6b-2 measured the same leg moving from +4.03%
to +3.25% while its cycles rose 7%.

The convert leg's ~35% has no mechanism attached to it yet. The candidates are named and
not separated: mGBA's pricing of cartridge-ROM *data* reads (E5 arbitrated instruction
fetch, to 0.012%, and said nothing about data), the prefetch buffer's interaction with
those reads, and the fact that these images reach a unit from an SD-card flash cart rather
than a mask ROM. The one-variable control that would price it is a ring placement run
(EWRAM vs IWRAM at 20 chunks — 22,400 B, which fits) on both units; it has never been
built. Against the emulator's side of the ledger: on the interrupt question mGBA was the
one that was *wrong* — it reported the display's VBlank request bit set with one shared
entry where the units read it clear — so interrupt-topology claims now need silicon in
both directions and cannot be settled by an emulator run's presence **or** absence.

Two related instrument facts worth carrying: instruments are not free and are not equal
across environments (one `io` read, a store, a compare and a subtraction per frame cost
+87 c/frame in mGBA and +262 c/frame on silicon — about 3×), and adding a store *inside*
the very critical section under measurement moved the quantity it was measuring (a late
re-arm's delay went 23 → 25 samples between those two images). Both were reported rather
than smoothed over, and the event reproduced at the same period index across two images
and four runs, which is what makes it safe to read.

## Gates (the Makefile is the contract)

| Gate | Proves |
|---|---|
| `flac-test` | The core's default configuration: thumbv4t `-Zbuild-std=core,alloc` compile (the gate symphonia could never pass — it keeps `std` out) plus the host suites. `--features instrument` for the host half. |
| `flac-test-wide` | `--no-default-features`: the audited i64 reference leg driven end to end. Pairs with `flac-test` against one external oracle; that pair *is* the width differential. |
| `flac-test-crc` | `--features crc-check`: header CRC-8 and footer CRC-16 verification, accept and reject paths. Verification is OFF by default, so this is what keeps the OFF default honest. |
| `flac-test-scan` | `--features scan,instrument`: the frame walk the product facade is built on, compiled for the target and witnessed against the committed layout tables. |
| `flac-test-rice` | `--features rice-hybrid`: the second `RiceKernel` mechanism and the two-mechanism differential. It stayed non-default because the on-target reading declined the flip (+42.6 c/sample). |
| `flac-portable-check` | The core compiles for a **second** bare-metal target (`thumbv7em-none-eabi`, `--features scan,rice-hybrid`, `iwram-pin` off). A build fact: **no non-GBA performance claim exists here, in either direction.** |
| `gafp-check` | The committed product blob still matches `scripts/gen_gafp_blob.py` byte for byte. A prerequisite of `flac-rom`, and the only gate that reads the *spike's* own frames — i.e. the only one that says the asset the ROM plays is the asset the instrument measured. |
| `gafp-pack-check` | The real packer's three legs: byte-identity with the committed blob, the fixture set re-generated **with its pack-time refusals** (LPC at the product profile, an off-target rate, 8-bit) re-asserted, and `gafp_asset_facts.rs` reading every fact back through `Manifest`/`scan::Frames`/`Decoder`. The only FLAC gate that runs libFLAC: it needs `flac` + `metaflac` on PATH, so a red here in an image without them is a missing tool, not a decoder regression. |
| `flac-gates` | Every configuration above, plus the `crc-check,scan` and `crc-check,scan,rice-hybrid` combination arms — the only builds in which a *second instantiation* is visible. |
| `overlay-check` / `overlay-check-arm` | The overlay (and the core through its path dependency) type-checks for the target, default and `rice-arm`. An rlib has **no link step**, so neither claims a codegen charge or a link; the link witness is `flac-rom` and `overlay-kernel-rom`. `cargo check` also does not compile `#[cfg(test)]` source, so an `unsafe` in a test is convicted by the test gates and never here. |
| `overlay-test` / `overlay-test-arm` | The default configuration (which contains no kernel, asm, FFI or `unsafe`) and the `rice-arm` boundary contract. On the host the differential runs a Rust **model** of the asm: it witnesses the contract, not the instruction stream. |
| `overlay-objdump` | The ELF fact a green sweep cannot show: `arm_rice_partition` is four-byte ARM code, returns `pop {…, lr}` + `bx lr`, and has no LDM-to-PC. A Thumb build of the same algorithm computes the same answers and would pass the sweep too. |
| `overlay-kernel-test` | The only place the assembled kernel runs: mGBA's ARM7TDMI sweeps the committed corpus and the synthesized boundaries through the incumbent, the kernel and the model. Debug profile on purpose (overflow checks and the model's domain guard are part of what is witnessed). |
| `product-play-test` | The product geometry's host model: the shipped manifest's real frame lengths replayed through the **same** `RingFeed` and the **same** `geometry` constants the ROM compiles, against a credit-per-period clock. Where flow control, the two tolerances and the ring sizing are justified rather than asserted. Its cost envelope is transcribed from one named run's printed `stages:`/`attribution:` lines — `the_envelope_is_the_runs_own_sums` locks those to each other, but **no host test reads a ROM log**, so re-reading them after a build is record-keeping, not a gate. |
| `spike-test` | The instrument's host witness: the exact embedded clips decode bit-exactly against `flac -d`, the pins reproduce, the shared fold reaches them. |
| `test-rom` / `test` | The root ROM's `#[test_case]` suite (currently **collects 0 cases** — a build/link/boot check, not evidence) and `test`'s aggregate with `flac-test` + `uli-test`. |
| `check` | `cargo fmt --check` across the workspaces. The only style gate; builds auto-format, tests never write. |

**Counts move with the tree, so they are recorded per landing and not here.** A green
run's value is its exit status; if a count is needed for a record, re-run the gate and
say which tree, which host and which instance produced it. The most recent full read on
this host (**2026-11-14, `make flac-gates`, warm target dirs, pinned
`nightly-2026-09-04`**) is **1,526 passed / 0 failed / 3 ignored across 75 test
binaries**; the previous record at `main` on 2026-11-11 was 1,498 / 3 across 72, and the
drift between them is test-name attributable. Count the *aggregate plus the arms*, not
`test result:` lines — the run also prints 9 empty `Doc-tests` lines, which are not among
the 75.
`make test-rom` exits 0 with `Running 0 tests`; that is the standing state of the root
crate, and the first ROM suite with assertions in it is `overlay-kernel-test`.

**Container path.** Every target also runs as `podman-<target>` (builder stage; the
`test`/`test-rom` wrappers use the `tester` stage which carries `mgba-test-runner`). No
`podman-*` target has ever been run in the harness that writes most of these records —
rootless podman cannot start there. `podman-flac-gates` is not a thing: the aggregate is
native-only, which is the same omission the README used to keep a TODO open for.

## Golden vectors — how to regenerate them

`crates/flac-lite/tests/*.txt` hold real encoder output with expectations derived by an
**independent** parser, never by hand-packing and never by asking libFLAC to explain
itself. The 31-bit header error entered this project through a hand-packed vector and
survived review *because* it looked authoritative.

```sh
./scripts/gen_frame_vectors.sh          # frame headers; needs flac + metaflac on PATH
./scripts/gen_stream_vectors.sh         # stream/metadata layer, the s1…s6 fixtures
python3 scripts/gen_gafp_fixtures.py --check   # the packed .gfp fixtures + refusals
python3 scripts/gen_gafp_blob.py --check       # the committed product blob (no encoder needed)
bash scripts/gen_spike_assets.sh        # the spike's embedded clips
```

`gen_frame_vectors.sh` synthesizes the source PCM with `scripts/frame_vectors.py`
(integer arithmetic only — no floats, no RNG, so bytes are identical on every platform),
runs each encode profile, refuses to write unless **every** stream passes the harness's
stream invariants, and appends a census of what the encoder chose. The invariants: frame
count == `ceil(total_samples / max_blocksize)` from STREAMINFO; every header's CRC-8
verifies; coded frame numbers are exactly `0..N-1` in stream order (the assertion that
proves every field is read at the right width, since one mis-sized field corrupts the
coded number); offsets strictly ascending; every non-final frame's blocksize equals the
declared maximum (the final frame may be short). `frame_header_layout.rs` then walks the
same bytes through `bits::BitReader` and asserts each field, the coded number, the CRC-8
byte, the exact header bit length, and that the CRC-8 position is byte-aligned.

Before trusting any new source material, run `python3 scripts/frame_vectors.py measure
DIR`: the strict-vs-lenient frame comparison, which **fails loudly** if the strict filter
is not exact. Two things it taught, both now baked into the synthesis parameters: a
too-pure tonal source makes the vectors lie (the first synthesis produced `-l 4` and `-l 0`
encodes that were *both* all-FIXED, so the census contradicted the `-l` semantics it was
there to demonstrate — the source needs a broadband floor for LPC to win), and mid/side
vectors only exist because the synthesis is `L = center + side`, `R = center − side`; with
independent channels the encoder picks plain L/R and the vector silently vanishes.

## Cargo config inheritance (why the host gates run from outside the tree)

Cargo reads `.cargo/config.toml` by **walking up parent directories** and merging every
layer; a `[workspace]` boundary does not stop it. Two measured consequences, both still
live:

1. **A host gate must run from a cwd outside the repo** (`$(GATE_NEUTRAL_CWD)`), with
   `--manifest-path` and an explicit `--target $(HOST_TRIPLE)`. Otherwise the root's
   `[build] target` and `[unstable] build-std` leak in and the run dies with `E0463` or
   `E0152: duplicate lang item`. No in-tree override is durable: `-Zbuild-std=`,
   `CARGO_UNSTABLE_BUILD_STD=` and `--config unstable.build-std=[]` have each worked or
   failed depending on the nightly, because non-inheritance is *positional*, not flag
   precedence. Copy that shape into any new gate.
2. **Sub-crates must not re-declare the root config.** `rustflags` are concatenated across
   layers, so a second local copy of `-Tgba.ld` produces `region 'ewram' already defined`
   at link time. `flac-lite`, `flac-lite-gba`, `flac_spike`, `overlay-kernel-check` and
   `flac_integration` all inherit on purpose; only the pre-scaffold `symphonia_flac_probe`
   carries a redundant copy, harmless because it dies before linking.

`-Zbuild-std=core,alloc` on the target gate is **not** "using std": rustup ships no
prebuilt `core` for this Tier-3 target, so `core`/`alloc` are compiled from `rust-src`.
`std` must never appear in a build-std list in this project.

## The toolchain pin

`rust-toolchain.toml` pins `nightly-2026-09-04` = `rustc 1.100.0-nightly (a69a63265
2026-09-03)`, LLVM 23.1.1, and the Makefile reads that string with make's own file
function rather than keeping a second copy. A version string does not pin an instrument:
this one is a specific instance, because the ledgers above compare cycle counts across
sessions.

- `agb` 0.25.0 enables `agb_hashmap/allocator_api`; newer nightlies split
  `core::alloc::Allocator` and it fails with `E0277`. The pin resolves the build *and*
  matches the comparator across the measurement history.
- Editing the pin is a **codegen change** and owes the standing control set: a pre-change
  build, a section/`nm` dump, gate counts, and a run log — naming the rustc instance in
  the record.
- `agb-gbafix` is pinned at 0.25.0 `--locked`. Byte-neutral here, never verified on another
  machine, and it rewrites the header that every ROM sha in these docs is computed over.
- Never `rustup target add thumbv4t-none-eabi`: no prebuilt `core` exists on any toolchain.
- `make toolchain-check` fails fast with install instructions rather than half-running a
  gate, and cross-checks that the instance rustup resolves *in this tree* is the instance
  the build runs. Do not comment out the `channel` line to unpin — rustup then falls back
  to the machine default silently, exit 0, and you get two instruments in one tree.

## What the committed corpus cannot see

The Rice differentials in this tree run over **4 vectors, 11 partitions, 1,015 samples,
parameters {0, 2, 3, 6, 8}**, and it is blind in three measured ways: **0 escaped
partitions**, **no sample that folds to `u32::MAX`**, and **no codeword that overflows the
§9.2.7.3 guard**. Both rejection paths and the escape path are therefore witnessed only by
in-crate spec-text comparisons, and mutation arms over this corpus are green *correctly*.
The 5-bit-parameter remainder leg (`order` 17…31) is reached only by a spec-text sweep, not
by real streams, and the cursor-restore difference between the incumbent and a spec-order
kernel on an unterminated final codeword is permitted by the trait contract and recorded
as **untested**, not passed.

Worse, nothing in the corpus can tell one kernel from another — which is why both
source-level pins exist (the shipped asm's bytes and the fold body are compared at source
level, in the host differential and in the ROM, so a differential cannot keep itself honest
by comparing a kernel with a copy of itself). The standing fix was the kernel-check ROM
sweeping the **same** committed golden tables (shared-consumer rule: never a second
transcription of an expectation), and it landed: widening the corpus now fails two gates
and says so, because the census is a tripwire in both consumers.

## What not to quote

- A percentage from a different image than the arm you are comparing it against; an LPC
  percentage older than 6a-2 without its absolute cycle pair; a worst frame without its
  arm and frame index; `0x0300022c` as a fixed table address.
- Any host-side performance claim: `flac-portable-check` is a compile gate.
- Any mGBA cycle count as a hardware cycle count (see "mGBA versus silicon").
- A product-play screen's `dec`/`conv`/`push`/`feed` shares as a stage row of record —
  they are window means taken *inside a running player*. And any figure quoted as "a
  percentage of the measured cost" without the envelope beneath it: the sustained ceilings
  are computed from cost constants that belong to a named run, and 6b-3's model carried one
  run's sums three builds past it.
- "The geometry has X% margin" as a single number. The regimes answer differently by an
  order of magnitude: the clock's own sustained ceiling is **186.5%** of measured cost
  (1,027,946 c/frame — do not truncate `frame_cycles / PERIOD_CYCLES` = 3.657 to 3, which
  throws away 18% and produced a 152.9% figure that was quoted for a day), the shipped clip
  survives to 190.6% and a clip twice as long to 188.2%, the ring adds **4.1 points** of
  *time* rather than 38 points of throughput, and the transient answer is a different one
  again (17 periods of stall at the shipped prefill; the depth cliff is 20 chunks and the
  prefill cliff 1). Quote the regime, not the margin.
- A `.gba` hash as evidence of behaviour. Comment-only edits move the hash (the image
  embeds panic-location strings) while the run log reproduces byte-identically; nine
  successive `product-play.gba` hashes in the history behaved identically. A hash pin means
  something only next to the tree and host that produced it — cite the hash of *your own*
  build and say which recorded run you are comparing to.
- Any 32,768-Hz-declared figure as "a 32,768 Hz product": those were the shipped frames with
  the rate spelled differently, same samples and sizes. A real 32,768 Hz encode carries 2.4%
  more samples per second of playing time and whatever census its own encoder chose.
