# Method and gates — how a number gets taken here

`FLAC.md` used to be a lab notebook, so its method was spread across every record
that had ever used it. This file collects the rules once. Results live in
[`perf-results.md`](perf-results.md), [`playback-results.md`](playback-results.md)
and [`measurements.md`](measurements.md); this is the discipline that makes them
worth quoting.

The one rule everything else is a special case of:

> **A claim carries its witness.** For a format fact that is an RFC section or
> encoder bytes. For a performance fact that is a same-image control, a named
> ROM sha, a named toolchain instance, and the arithmetic that turns the raw
> counter into the number printed. For a correctness fact it is a witness the
> implementation could disagree with — ideally a second mechanism, not a
> transcription of the first.

## The real-time budget (derived, not recalled)

At the initial profile — 2048-sample frames at 32,768 Hz — the budget per frame is

```text
2048 / 32768 s × 16,780,000  = 1,048,750 cycles/frame = 512.0849609375 c/sample   ← the ledger's
2048 / 32768 s × 2^24        = 1,048,576 cycles/frame = 512 c/sample               ← the exact one
```

**Two clocks are in use in this tree, and the row above is the difference.** The
instrument budgets with `stats.rs::GBA_CLOCK_HZ = 16_780_000` — the rounded "16.78
MHz" — so 1,048,750 is what `real_time_budget()` computes, what every `verdict:`
line in every ROM compares against, and what the `perf-results.md` /
`measurements.md` tables divide by; that arithmetic is itself under test
(`real_time_budget_matches_the_plan_arithmetic`). `scripts/gafp_pack.py` budgets
with `GBA_CYCLES_PER_SECOND = 16_777_216` = 2^24, which is the GBA's actual clock
and what an `Hz32768` timer really divides: 512 cycles per sample, 2^20 per
2048-sample frame. `examples/flac_spike/src/stats.rs` does not carry 2^24, so a
reader who takes the instrument's constant to the hardware gets a budget 174 cycles
too generous (0.0166%). Nothing decided changes — every bar here is two decimal
places away from a decision — but it does mean a share quoted to three decimals is
denominator-dependent (398,255 c is 37.974% of the rounded budget and 37.981% of
the exact one), and the packer's 0.1 pp row-consistency tolerance is wider than
that gap. Item 7-1 puts new shares on the same table; pick one clock before then
and the record's correction in `plan-of-record.md` (2026-11-11) is where the two
are currently reconciled in prose.

The budget scales 1-for-1 with sample rate while per-frame decode work barely
moves, which is why the rate track exists at all:

| rate (Hz) | c/sample budget | budget/frame (2048) | mixer domain |
|---|---|---|---|
| 32,768 | 512.08 | 1,048,750 | `Hz32768` |
| 18,157 | 924.16 | 1,892,682 | `Hz18157` |
| 10,512 | 1,596.27 | 3,269,162 | `Hz10512` |

`examples/flac_spike/src/stats.rs::real_time_budget(blocksize, rate)` is the one
implementation; the ROM evaluates it at compile time.

**A units error worth keeping in the record:** the spike README once said "62,500
cycles per frame". That is 62,500 **microseconds** with the unit misapplied — the
formula gives ~1,049,000 cycles at 16.78 MHz, 16.78× larger. Its working targets
(≤ 35,000 mean / ≤ 55,000 max) inherited the error and were ~3% of the true
budget, i.e. accidentally stricter than real time while claiming to be a margin.
Working targets are derived against measured mixer/DMA load, never recalled.

## Rules every run follows

- **Correctness before speed.** No number is reported from an image whose decode
  has not been proven correct on its own embedded bytes (host bit-exact vs
  `flac -d`, plus on-target checksum). A fast wrong decode measures nothing.
- **One variable per run.** A "speed change that keeps `make spike-test` green"
  changed *how*, never *what*.
- **Measure the net, report the raw.** Every window is net of a calibration taken
  in the same image and the same shape. Calibration windows must be byte-identical
  to each other: the first draft's asymmetric bookkeeping measured itself (raw 24
  then 22) and the stability check refused the run. Fixed shape: raw 22 stable,
  all nets 0.
- **All bookkeeping and logging outside the window.** A serial write inside a
  window measures the logger. IRQs off (`critical_section` = IME save/disable/
  restore), IME witnessed 0 inside.
- **Determinism is a witness, not an assumption.** Two headless runs `cmp`
  byte-identical; the `***` cycle lines identical across runs. A fixed image
  repeats to the cycle — the docs-only control in item 5 PR 4 proved exactly that
  (instruction stream unchanged → same sums, mins, maxes, indices).
- **The tail is first-class.** One over-budget frame is an audible glitch: report
  min, max with its frame index, sum, and mean; never mean alone.
- **Paired per-frame deltas are the number of record.** Percentages are
  denominator-dependent; absolute cycles/frame and cycles/sample survive a changed
  denominator. (Item 5 PR 4's FIXED +9.71% and LPC +2.64% are the *same*
  measurement: +174 K cycles/frame.)
- **Say what the command varied, not only what it printed.** A "measured" claim
  that omits the variable is unfalsifiable, and a zero-returning throwaway script
  can report a fact that merely agrees with the prose being checked.
- **A second mechanism, not a second transcription.** Host and ROM run the same
  fold/driver over the same bytes (the shared-fold rule); oracles are written
  independently (naive bit-walk vs accumulator reader; Python big-int vs i64).
  "Copy the oracle's output" is still a transcription — the hand-typed stereo
  oracle diverged on one block and was caught by a mechanical re-check.
- **Rejection paths need mutation witnesses.** libFLAC never emits an invalid
  stream, so every rejection test mutates one field of a *real* vector at runtime
  and asserts the specific error variant; the untouched bytes keep widths honest.
- **Probes live in `examples/flac_spike/experiments/<name>/`.** The spike's `src/`
  is the cross-session instrument; a probe never lands on it. Each archive has a
  README with re-application notes and its measured values.
- **No cycles quoted across records** (see [`measurements.md`](measurements.md)).
  Same-image pairs only, and every arm names its own control in its own session.

**The false-green catalogue** (each cost real time, each is why the rules exist):
a hand-packed vector records the author's belief, not the format (it fired four
times, twice surviving review); `checked_shl` guards the shift amount and wraps
the value; a hand-computed "published" FNV vector was wrong from memory; a test
that re-parsed the pristine frame asserted `Ok` from the mutation; a generated
`\uXXXX` escape is valid JSON and invalid Rust; a gate that "runs" can collect
zero tests; and the mirror image of all of those — a *negative* composed by adding
one record's paired marginal to another record's absolute, on the page that states
the rule against it (6a-5's 103.83% stack projection, re-opened by 6a-5b). A
projection is a claim about two images; the same-image pair is the measurement, and
the plan's sequencing decisions inherit whichever one they were taken from.

**The rule's other half, which 6a-5b measured (2026-11-09).** That projection came
out right — to 0.4% — and it is now on the record *why*: the two absolute arms it
composed reproduce across images (landed `asm` 183,288,727 / 183,231,371 /
183,304,113, a 0.04% spread; E10's vendored `arm` 167,911,108 → 167,872,486,
−0.02%), while the control moved 280,848,260 → 275,111,047 → 249,051,451 across the
same three links. So the rule is not "never compose a marginal with an absolute"; it
is *name the image each number came from, and know whether the absolutes reproduce*.
An arm that reproduces across named images, to a stated spread, is quotable across
records as a paired marginal — but the only way to know that is to measure the pair
in one image, which is what the rule asks for anyway. A projection that has had that
check is a projection with a bound on it; one that has not is what this catalogue
already calls it. Two corollaries, both measured on the 6a-5b image: a fixed image
re-runs byte-identically (664 `***` lines, zero differences over two runs), and the
~9–13 cyc/frame recompile band is a band on **paired deltas** — adding a fourth
timed leg to the same source moved every absolute leg of both clips by exactly
+15,386 cycles (+98 c/frame on all 314 frame-legs, mins and maxes alike) while every
pair stayed cycle-identical. The offset is a property of the image, not of any arm
(“why” is unmeasured; that it is fixed, deterministic and cancelling is not): pairing
does not merely reduce that error, it cancels it, and an absolute arm compared across
two images can be off by a fixed per-frame amount with no link mechanism behind it.

## Measurement entry points (spike crate)

- `timing_pass()` — the per-frame loop; the closure handed to `timed()` is exactly
  what is measured.
- `timed()` — window mechanics (IME-off critical section, composite timer reads);
  `CALIBRATION_SAMPLES` just above it.
- `BUDGET_CYCLES_PER_FRAME` — the `real_time_budget(2048, 32_768)` constant.
- Counter: timer2 at /1 cascading into timer3, read as a 32-bit composite with a
  low–high–low stability recheck. A window that outlives the composite saturates
  to `stats::WORST_WRAPPED` and fails loudly rather than reporting a modulo.
- Never touch `src/assets.rs` or the committed `.bin`s — region pins gate every
  layer above.

Run loop: `make native-spike-rom` → `timeout -k 5 280 mgba-test-runner
flac-spike.gba` with serial captured to a log (kill after the verdict line — the
runner hangs on gfx-loop ROMs, expected) → `grep -E "stats:|budget:|verdict"`.
The 6a-5 arm images use the same loop on `flac-spike-6a5-arms.gba` /
`flac-spike-6a5-asm.gba`, grepping `6a5`; the arm pass has its own calibration
(3 discarded + 5 reported, E4's shape) and its own paired-delta summary lines.

**Paired deltas are the number of record; absolute sums are not.** 6a-5 measured
this twice over: the arm images' absolute sums moved by **+2 cycles/frame** between
two harness layouts while every *paired* delta reproduced to the cycle, and moving
`ctrl` between links moves it by **−36,543 / −202,529 cycles/frame** depending on
how many `RiceKernel` bodies the link carries. Same-image pairs, or nothing.

*Provenance:* the exploration menu that produced E1–E10 was retired with the
results; its method rules are the ones above, and its per-variant loop was:
`make native-spike-rom` → run under `mgba-test-runner` with serial logged →
`grep -E "stats:|budget:|verdict"`. Probes are kept as reappliable samples
under `examples/flac_spike/experiments/<name>/`.

## Gate surface (the Makefile is the contract)

| Gate | What it proves |
|---|---|
| `make flac-test` | The real gate: `thumbv4t-none-eabi` compile (`-Zbuild-std=core,alloc`) + host unit/integration suites, default features (`i32-integrator` on). Run from a cwd outside the repo (see below). |
| `make flac-test-wide` | Same, `--no-default-features`: the audited i64 reference build, end to end. Pairs with `flac-test` against one external oracle (`flac -d` PCM); the width differential suite is `required-features = ["i32-integrator"]`, so it is not built here. |
| `make flac-test-crc` | Same, `--features crc-check`: header CRC-8 + footer CRC-16 verification, both accept and reject paths. |
| `make flac-test-scan` | Same, `--features scan`: the frame walk + `tests/scan_frame_walk_layout.rs` (`required-features = ["scan"]`). |
| `make flac-test-rice` | Same, `--features rice-hybrid`: the two-mechanism differential. |
| `make native-flac-rom-default-kernel` | 6b-1's A/B control: the same ROM source built `--no-default-features`, i.e. `arm-kernel` off, so the identical decode runs through the core's `TableCountKernel`. It is what makes the kernel swap a same-source pair rather than two unrelated ROMs (`flac-integration-default-kernel.gba`). |
| `make flac-portable-check` | A **second** bare-metal target (`thumbv7em-none-eabi`) compiles, `--features scan,rice-hybrid`, `iwram-pin` off. Compile only — **no non-GBA performance claim exists**. |
| `make gafp-check` | The committed product blob's tripwire: `scripts/gen_gafp_blob.py --check` re-assembles it in memory and compares byte for byte with `examples/flac_integration/assets/spike_l0.gfp`. No encoder needed. It also runs as a prerequisite of `flac-rom`, so no product image is built over a blob that no longer matches its generator. |
| `make gafp-pack-check` | 6b-2's writer gate, three legs: `scripts/gafp_pack.py` reproduces the committed product blob **byte for byte**; `gen_gafp_fixtures.py --check` regenerates the fixture set and its three **refusal** legs (LPC at the product profile, 44.1 kHz at a 32 kHz target, 8-bit) and asserts each one refuses; and `crates/flac-lite/tests/gafp_asset_facts.rs` (`--features scan,instrument --test gafp_asset_facts`, 8 tests) reads every fact row back through the facade — 2 tests per row, one of them the negative, the row contradicting the header it came from. Needs `flac` + `metaflac` on PATH: this is the leg that fails in a CI image without libFLAC tools, and it is not a decoder regression. |
| `make flac-gates` | The aggregate: `flac-test` + `flac-test-wide` + `flac-test-crc` + `flac-test-scan` + `flac-test-rice` + `flac-portable-check` + `gafp-check` + `gafp-pack-check`, plus the two combination arms (`crc-check,scan` and `crc-check,scan,rice-hybrid`). Counts move with the tree, so they are recorded per landing rather than here: at `main` (2026-11-11) it is **1,498 passed / 0 failed / 3 ignored across 72 test binaries** — the per-arm table is in `plan-of-record.md`, "Baselines to hold". It is the only aggregate that needs `flac`/`metaflac` on PATH, because the `gafp-pack-check` leg really runs the packer. |
| `make native-flac-rom` | **The product image** (`examples/flac_integration/` → `flac-integration.gba`): embeds the committed `GAFP` asset, parses its manifest, decodes through the facade with `flac-lite-gba`'s `ArmRiceKernel`, verifies against the pinned PCM, then times a second pass. Pairs with `native-flac-rom-default-kernel` above. **Builds and verifies; it makes no sound** — the sound is the second binary below (6b-3). |
| `make product-play-rom` / `native-product-play-rom` | **The playback image** (`product-play.gba`, 6b-3): a second binary of the *same* product crate — same blob, same kernel, same pins, same lockfile — that drives asset → manifest → `Decoder` → `convert_frame` → `RingFeed` → sound DMA/FIFO, with timer0 as the sample clock, timer1's period IRQ as the feeder and timer2 as an independent period witness. It is **built to play** at 32,768 Hz (headless mGBA only so far — **never heard**), **counts its own underflows**, gates its **period audit** (coalesced / late / clock refusal / tail / void) and the **played-form FIFO pins**, and prints a verdict to the log *and* the screen (`verdict PASSED — screen BLUE`). The ROM image is not gated: build it here and run it by hand (`timeout -k 5 120 $(GBA_TEST_RUNNER) product-play.gba`), and because a passing run ends in `loop {}` the timeout's **exit 124 is the expected result of a pass** — read the log, not the status. Its geometry model *is* gated (`make product-play-test`). Since 7-0 (2026-11-13) the asset it plays **is** a product asset — it declares 32,768 Hz, the rate the mixer runs at, and the ROM's own start-up profile check refuses any other; `Q14` was ruled 2026-11-11 and implemented by that item, and the `TODO(Q14, BLOCKING)` marker is gone. What still keeps this off the playback-claim list is that it has never run on hardware (7-1, then 7-3). |
| `make product-play-test` | 6b-3's host model (14 tests, host — 12 as landed by 6b-3, plus 7-0's rate-contract guard and the pitch instrument): replays the real manifest's frame lengths through the **same** `RingFeed` and the **same** `geometry` constants the ROM compiles (both `#[path]`-shared through `src/lib.rs`) against a credit-per-period clock, and is where the flow-control rule, the two tolerances and the buffer sizing are *justified* rather than asserted. Runs out-of-tree for the cargo-config reason, like the other host gates. Its cost envelope is transcribed from one named run of the ROM's own printed `stages:`/`attribution:` lines; `the_envelope_is_the_runs_own_sums` locks those numbers to each other and to the image's own `isolated_estimate`, but it cannot read a log, so re-reading them after a build is a record-keeping act and not a gate — 6b-3's report-hygiene pass is what that looks like when it is missed. Pinning the printed lines as a fixture is written against 7-1. |
| `make spike-test` / `native-spike-rom` | The measurement instrument: host witness over the exact embedded bytes, then on-target decode proof and cycle harness. |
| `make item6a5-test` / `item6a5-arms-rom` / `item6a5-asm-rom` | Item 6a-5's arms: every kernel (default, Rust mechanism, ARM embodiment) decodes the committed clips to the pinned PCM and agrees with the control frame window by frame window on the host, and the two arm images time them through the product call path (three-way and product-shape). Arm features are off in every other build. |
| `make item6a7-test` / `item6a7-rom` | Item 6a-7's width + stack image: the vendored pre-6a-6 i64 path (`src/wide.rs`) decodes the committed clips to the reference PCM and agrees with the i32 control frame by frame on the host; the ROM times `ctrl` (i32), `wide` (i64) and `asm` (kernel+i32) in one calibration. `item6a7-test` 26; the default `spike-test` and image are unchanged. |

| `make e5-fetch-rom` / `native-e5-fetch-rom` | E5's hardware arbitration artifact (`examples/e5_fetch/`): one synthetic ARM stream assembled into `.text` and `.iwram` and timed in one image, with `WAITCNT`/prefetch on/off and the ratio rendered on screen for a stock GBA; measured 2026-11-10 on a GBA 001 and a GBA SP: ROM 4.085 / IWRAM 1.037 cycles/add, ratio 3.939, mGBA reproduced within 2 cycles/call. |
| `make rate-assets` / `rate-test` / `native-rate-rom` / `native-rate-play-rom` | The rate flow (10,512 / 18,157 / 32,768 Hz) and FIFO playback harness. |
| `make kernel-test` / `native-kernel-rom` | The ARM kernels: host twin + on-target mGBA sweep over one committed golden table each. |
| `make overlay-check` / `overlay-test` / `overlay-test-twin` | `flac-lite-gba`: thumbv4t compile witness, its own host test, and the out-of-crate Rice differential. |
| `make test-rom` | The root ROM's `#[test_case]` suite under `mgba-test-runner` — which currently **collects 0 cases** and prints `Running 0 tests`, so its exit 0 is a build-and-boot check, not evidence of anything (see the note below; 6a-5 owns the suite). |
| `make check` | Formatting + workspace checks (the only style gate; builds auto-format). |

**Standing caveats, so nobody reads a defined target as a tested one:**

- Every `podman-*` target has **never been run** in this harness (rootless podman
  cannot start here: no `newuidmap`, no podman machine, no nested virtualization).
- `make test-rom` runs, exits 0, and prints `Running 0 tests` — the root ROM suite
  collects nothing. A non-empty ROM suite is a prerequisite of item 6a-5, not a
  result of it.
- `rate-test` is not runnable when the generated rate assets are gitignored
  (several records say so).
- `flac-portable-check` is a compile gate. No host-side performance claim of any
  kind is available, in either direction.

## Golden vectors — how to regenerate them

`crates/flac-lite/tests/frame_header_vectors.txt` holds real frame headers cut
from real encodes, with expected field values derived by an **independent RFC
parser**, never by hand-packing and never by asking libFLAC to explain itself.
Consumed by `crates/flac-lite/tests/frame_header_layout.rs`.

**Why machine-generated.** The 31-bit error entered the project through a
hand-packed test vector and survived review *because* it looked authoritative:
whoever packed those bytes believed 31 bits, and the test asserted 31 bits.
A hand-packed vector cannot witness a field-width claim. Encoder output can, so
the vectors are encoder output, end to end.

### Regenerating

```sh
# from the repo root; needs `flac` + `metaflac` on PATH (brew install flac)
./scripts/gen_frame_vectors.sh [output-path]
```

That script: synthesizes the source PCM (`scripts/frame_vectors.py synth`,
integer arithmetic only — no floats, no RNG, so bytes are identical on every
platform), runs each encode profile, runs the harness's **stream invariants**
(fail-closed), and writes the vector table (default:
`crates/flac-lite/tests/frame_header_vectors.txt`). Pass a path to emit somewhere
else without touching the committed table.

Pieces:

* `scripts/frame_vectors.py` — the oracle: RFC tables, a strict frame-header
  parser, CRC-8, deterministic PCM synthesis, frame finding, stream checks,
  vector emission. Subcommands: `synth DIR`, `emit DIR OUT`, `measure DIR`.
* `scripts/gen_frame_vectors.sh` — the deterministic driver (synth → encode →
  check → emit), plus a census of what the encoder chose, appended as comments.

### The invariants that make the table trustworthy

`emit` refuses to write unless, for **every** source stream:

1. frames found == `ceil(total_samples / max_blocksize)` from STREAMINFO;
2. every frame header's **CRC-8 verifies**;
3. coded frame numbers are **exactly `0..N-1` in stream order** — this is the
   assertion that proves the parser reads every field at the right width, since
   one mis-sized field corrupts the coded number;
4. frame offsets strictly ascending;
5. every **non-final** frame's blocksize equals STREAMINFO's maximum (the final
   frame may be short — see above).

And `frame_header_layout.rs` asserts, per vector, that walking the bytes through
`bits::BitReader` reproduces every field, the coded number, the CRC-8 byte, the
exact header bit-length, and that the CRC-8 position is byte-aligned (a
regression tripwire: if that ever fails, someone changed a field width).

### Checking new source material before trusting it

`scripts/frame_vectors.py measure DIR` runs the strict-vs-lenient comparison over
any directory of `.flac` files and prints real/candidate counts per stream, and
**fails loudly** if the strict filter is not exact. Use it whenever adding a
vector source — it is how the sync-scan numbers above were produced, and how the
"final frame is short" over-rejection was caught.

Two things it also taught, both now encoded in the synthesis parameters:

* **A too-pure tonal source makes the vectors lie.** The first synthesis pass
  produced `-l 4` and `-l 0` encodes that were *both* all-FIXED, so the table's
  census line contradicted the `-l` semantics it was there to demonstrate. The
  source needed a broadband floor for LPC to win: the committed synthesis has a
  noise floor and several non-harmonically-related partials, and the emitted
  census (`lpc4` at `-l 4`, `fixed0` at `-l 0`) is the check that it still does.
* **Decorrelation must be earned.** Mid/side vectors only exist because the
  synthesis is `L = center + side`, `R = center − side`. With independent
  channels libFLAC picks plain L/R and the vector silently disappears.

### Vector set (as generated by flac 1.5.0)

| Vector | Why it's in the set | Required |
|---|---|---|
| `stereo-6byte-first` | 6-byte header, 1-octet coded number, CRC-8 at bit 40 | yes |
| `stereo-7byte-num128` | 7-byte header — frame ≥128 forces the 2-octet form | yes |
| `stereo-last` | final frame: **short blocksize** *and* a 7-byte number | yes |
| `mono-b1024-first` | mono (channels `0b0000`), blocksize 1024, `lpc4` subframe | yes |
| `r65k-rate-from-stream` | sample-rate code `0b0000` — the 65 kHz path | yes |
| `fixed-only-first` | `-l 0` encode: FIXED-only, the perf gate's FIXED arm | yes |
| `silence-constant` | CONSTANT subframes from digital silence | no |
| `stereo-midside` | channels `0b1010` | no |
| `stereo-leftside`, `stereo-sideright` | the other two decorrelation modes; **encoder-dependent**, emitted only if chosen | no |

Required vectors fail the generator if absent; optional ones are skipped with
nothing emitted, because "the encoder didn't choose left/side for this source"
is a fact about the source, not a harness bug.

---

## Toolchain pin — a version string does not pin an instrument

Since 2026-11-06 the compiler instance every cycle record was measured with is a
string in `rust-toolchain.toml`: `nightly-2026-09-04`, i.e.
`rustc 1.100.0-nightly (a69a63265 2026-09-03)`, LLVM 23.1.1. The Makefile reads
that file with make's own function rather than parsing it twice.

Why it is pinned rather than documented:

- `agb v0.25.0` enables `agb_hashmap/allocator_api`; newer nightlies split
  `core::alloc::Allocator` and `agb_hashmap` fails with `E0277` (`ALLOCATOR:
  AllocatorNightly` unsatisfied). Pinning resolves the build *and* matches the
  comparator across the measurement history.
- Editing the pin is a **codegen change**. It carries the four-arm control
  (warm/clean × before/after) and a run log, like any other change that can move
  an instruction stream.
- `agb-gbafix` is pinned at 0.25.0 `--locked`. Byte-neutral here; never verified
  on another machine, and it rewrites the header every ROM sha in these docs is
  computed over.

## Cargo config inheritance (why the host gates run from outside the tree)

Cargo reads `.cargo/config.toml` by **walking up parent directories** and merging
every layer; a `[workspace]` boundary does not stop it. Two consequences, both
measured, both still live:

1. **The host test gate must run from a cwd outside the repo.** Otherwise the
   root's `[build] target = thumbv4t-none-eabi` and `[unstable] build-std` leak in
   and the host test fails with `E0463`/`E0152`. No in-tree override is durable:
   `-Zbuild-std=`, `CARGO_UNSTABLE_BUILD_STD=`, and
   `--config 'unstable.build-std=[]'` all worked or failed depending on the
   nightly, because non-inheritance is *positional*, not flag precedence. The
   canonical gate resolves the manifest path with `$(abspath …)` **before**
   `cd`-ing to a scratch dir outside the tree; artifacts still land in the
   crate's `target/`.
2. **Standalone sub-crates must not re-declare the root config.** `rustflags` are
   concatenated across layers, so a second local copy of `-Tgba.ld` produces
   `region 'ewram' already defined` at link time. `flac-lite`, `flac-lite-gba`,
   `flac_spike`, `arm-kernels` and `flac_integration` inherit; only the
   pre-scaffold `symphonia_flac_probe` still carries a redundant copy, harmless
   because it dies before linking.

`-Zbuild-std=core,alloc` on the target gate is **not** "using std": rustup ships
no prebuilt `core` for this Tier-3 target, so `core` must be compiled from source.
`std` must never appear in a build-std list in this project.

## The committed corpus, and what it cannot see

The Rice differentials in this tree run over **4 vectors, 11 partitions, 1,015
samples, parameters {0, 2, 3, 6, 8}**, and it is blind in three measured ways:

- **0 escaped partitions** — the escape path is witnessed only by spec-text
  comparisons (and RFC Appendix D's worked residual, elsewhere).
- **No sample that folds to `u32::MAX`** — the §9.2.7.3 `i32::MIN` rejection is
  witnessed only in-crate.
- **No codeword that overflows the §9.2.7.3 guard** — same.

Mutation arms that "pass" over this corpus are green *correctly*. Worse, nothing in
the corpus can tell one kernel from another (mutation arms M3/M5), so a
differential can keep itself honest by comparing a kernel with a copy of itself —
which is why both source-level pins exist. The 5-bit
parameter remainder leg (`order` 17…31) is reached only by a spec-text sweep, not
by real streams. The cursor-restore difference between the incumbent and a
spec-order kernel on an unterminated final codeword is permitted by the trait
contract and recorded as **untested**, not passed. The standing fix is a
kernel-check ROM sweeping the **same** committed golden tables (shared-consumer
rule — never a second transcription of an expectation).
