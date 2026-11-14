# FLAC on the GBA — `flac-lite`

Goal #4 of this project: decode FLAC on a Game Boy Advance so more audio fits on a
cartridge. This file is the entry point — the constraint, what exists, how to build
a ROM, what has been measured, and where the detail lives. It is deliberately
short. The work is **done and accepted**: `C1` (a standalone `no_std` crate) and
`C2` (real-time playback on hardware) both closed on 2026-11-13, so this directory
holds a description of a shipped thing rather than a plan. The history of how it got
here, and the rulings that shaped it, are in
[`flac-docs/history.md`](flac-docs/history.md).

> **Scope: this file and [`flac-docs/`](flac-docs/) are the FLAC line of work, and
> nothing else.** They govern `crates/flac-lite`, `crates/flac-lite-gba`,
> `examples/flac_integration`, `examples/flac_spike`, the `GAFP` asset and the
> `flac-*`/`product-play`/`gafp`/`overlay` Make targets. They are **not** a
> repository-wide spec and they decide nothing outside the FLAC track: "the product"
> and "the product path" here mean the **FLAC** product path
> (`examples/flac_integration`, `product-play.gba`). The sibling codec tracks keep
> their own entry points and ledgers — [`ULI2.md`](ULI2.md) (accepted 2026-11-13 as
> the repository's product codec), [`ULC.md`](ULC.md), [`OPUS.md`](OPUS.md) — and
> their own senses of the word "product". Do not carry a decision across that
> boundary in either direction. Where the tracks genuinely share ground (the
> ring/FIFO feeder, the geometry, the measurement method) it is named as shared.
>
> One unreconciled difference, recorded on both sides: FLAC's C2 asks for the highest
> rate this hardware achieved and settled on **16,384 Hz**, while
> [`ULI2.md`](ULI2.md)'s playback gate is written at 32,768 Hz. FLAC's evidence for
> that number is in [`flac-docs/history.md`](flac-docs/history.md) §"The rate
> ruling" and is re-usable; the decision is the product-codec owner's, not this
> file's.

## The constraint

`thumbv4t-none-eabi` is Tier-3 bare-metal: `#![no_std]`, **`core` + `alloc` only**.
Anything that references `std`, directly or transitively, cannot compile for this
target. That is why this project has its own decoder: the mature Rust FLAC decoders
are built on `std::io`, and in symphonia's no-std fork the blockers are structural
(`Error::IoError(std::io::Error)` as the one error type, an `io/` module on
`Read`/`Seek`/`BufRead`, a `HashMap` registry, `std::sync::Arc` in metadata), not a
dependency-tree accident. On 2026-08-30 the project wrote a minimal `no_std`,
zero-dependency, zero-allocation frame decoder instead — `flac-lite` — because
controlling both ends (encoding is offline and ours; the "filesystem" is a
memory-mapped ROM) deletes the whole stack that made the port impossible. The failed
port is kept as evidence at `examples/symphonia_flac_probe/` (it is expected to fail
compilation). Full reasoning: [`flac-docs/architecture.md`](flac-docs/architecture.md).

## Making a ROM

```sh
make master-pack     INPUT=track.wav    # resample + encode + pack a GAFP asset + its pins
make master-play-rom INPUT=track.wav    # …and a playable ROM from it  -> master-play.gba
```

`master-play.gba` is the thing that makes sound: it plays the asset end to end on
real hardware. The two images built from the tracked asset instead of a fresh
master are `product-play.gba` (`make product-play-rom`, the same binary on the
committed clip) and `flac-integration.gba` (`make flac-rom`, decode/verify/time —
no sound, and the real link witness for the core). The measurement instrument is
`make spike-rom`. Every target also runs as `podman-<target>` in the container and
`native-<target>` on the host; `make help` lists the set.

Two things about playback ROMs that bite the first time:

- A passing run ends in `loop {}`, so under `mgba-test-runner` the timeout's
  **exit 124 is the pass**. Read the `[pp-rom]` log and the screen, not the status.
- Nothing here has been heard from an emulator. `make master-play-rom` plus a
  headless run is the **mGBA control**; the claim comes from a unit.

Host toolchain: the pinned nightly in `rust-toolchain.toml` plus `rust-src`,
`agb-gbafix` 0.25.0 for ROM fixing, `mgba-test-runner` for the ROM suites, and
`flac`/`metaflac` for the packer's gate. `.agents/skills/gba-build-and-test/` has the
setup script and this repo's build gotchas.

## What the product is

| | |
|---|---|
| Rate | **16,384 Hz** — `2²⁴/1024`, so the timer reload is an integer and the clock plays exactly the rate the stream declares |
| Depth | 16-bit in the stream, **8-bit out**: the GBA's two direct-sound channels are 8-bit signed FIFO channels fed by DMA, so the only freedom is the refill rate. The ~48 dB that 8-bit truncation implies, with no dither, is the product's floor rather than a defect item 7 left open |
| Frames | 2048 samples, fixed blocksize, stereo (mono is not implemented) |
| Predictor | **FIXED-0** is the measured product shape; the master path also accepts CONSTANT/VERBATIM/FIXED-1..4, which have no measured row of their own. **LPC is refused at pack time** |
| Geometry | 1,024 ticks/sample · 2,097,152 cycles per frame · `CHUNK` 560 samples = 573,440 cycles per period · ring 36 chunks = 20,160 samples/channel · prefill 16 chunks · re-arm grace 16 samples |
| Container | `GAFP` — raw FLAC frames plus a manifest and an offset table. Byte contract in [`crates/flac-lite/README.md`](crates/flac-lite/README.md) |
| Playback | no `agb` mixer: the ROM programs timer0, the timer1 cascade, the sound DMA and every per-period re-arm itself, and states that as a safety precondition |

## Rules this track does not renegotiate

These are settled by measurement or by the owner, and each one exists because
breaking it was demonstrated to be cheap-looking and wrong. Details and dates:
[`flac-docs/history.md`](flac-docs/history.md).

1. **A stream is encoded at the rate it is played at** (Q14, ruled 2026-11-11). The
   player does not reinterpret a declared rate to fit a clock, and there is no
   tolerance anywhere: `RATE_BRIDGE` is `None`, `playback_rate_for_stream` is an
   equality, `EncodeProfile::allowed_sample_rates_hz` names exactly one rate, the
   packer refuses a foreign-declaring stream before it encodes, and the ROM checks
   its own embedded blob at start-up. The failure mode being excluded is audio
   coming back pitch-shifted: a 32,000 Hz asset on a 32,768 Hz clock is 1.024× fast,
   **41.06 cents**, and *every digest in this repository is blind to it* because the
   samples are the same bytes. Note the trap in the format: FLAC's four-bit rate
   table has 16,000 and 32,000 and neither power of two, so `16,384/16,000` is the
   same 1.024.
2. **The rates this architecture can express exactly** are the ones dividing 2²⁴ by
   an integer: 65,536 / 32,768 / 16,384 / 8,192. `agb`'s 10,512 / 18,157 / 32,768
   menu is not a dependency here — no FLAC ROM instantiates its mixer — and 18,157
   is not exact (924 ticks plays 18,157.16 Hz).
3. **FIXED-0 is what has a measured row.** `-l 0` disables LPC; it does **not** force
   FIXED order 0. The packer censuses the subframes it actually got and refuses what
   it has not priced; `--master` is the deliberate exception for real material, and
   `--allow-lpc`/`--allow-unmeasured` mark their output NON-PRODUCT.
4. **The compiler instance is the instrument.** `rust-toolchain.toml` pins
   `nightly-2026-09-04`; editing it is a codegen change that owes a control build,
   and the Makefile reads the pin rather than copying it. ROM shas and cycle counts
   are only comparable between the same build path on the same machine — never quote
   one across hosts.
5. **mGBA figures for memory-heavy legs are derated, not trusted.** Hardware runs
   consistently above the model: decode ~+7%, and the producer's convert leg
   **+35%**, with no probe yet putting a mechanism to that gap. Where a hardware
   figure exists, use it.
6. **Acceptance is per path, not per asset** (owner's amendment, 2026-11-13): the
   code path was validated once end to end with real audio on both units; subsequent
   encodes are judged by the build-time gates and the mGBA control. The packer's
   structural refusals stay useful as preflight, not as the gate.

## Gates

```sh
make flac-check     # everything a FLAC change owes, in one command
```

which is `flac-gates` (every `crates/flac-lite` configuration + both `gafp` legs) +
`overlay-gates` (both `crates/flac-lite-gba` configurations, the objdump fact, and
the ARM kernel's device suite in mGBA) + `product-play-test` (the host model of the
product geometry) + `spike-test` (the instrument's bit-exactness witness).
`make check` is the only formatting gate; builds auto-format and never gate on
tests. The per-gate detail, and why each non-default configuration has its own
command: [`flac-docs/measurements.md`](flac-docs/measurements.md) §"Gates".

Anything touching `crates/flac-lite` owes `make flac-gates`, not just
`make test` — the feature-gated suites are not in the default build, and a
configuration nobody runs is a configuration nobody witnesses.

## What exists

| Piece | What it is |
|---|---|
| `crates/flac-lite` | The portable core: `no_std`, zero deps, zero allocation on the decode path, `#![deny(unsafe_code)]` plus named `cfg`-gated `.iwram` pin attributes. Frame decode is complete (`bits` → `stream` → `frame` → `subframe` → `residual` → `stereo`), bit-exact against `flac -d` on host and in the ROM. Facade: `decoder::Decoder` over `scan::Frames`, driven by `format::Manifest`; `EncodeProfile::validate` is the pack-time guard. No `todo!()` remains in `src/`. |
| `crates/flac-lite-gba` | The GBA overlay: the `RiceKernel` realizations that are platform-bound. `rice-arm` is the shipped one — an ARM-mode codeword loop in `asm/rice_partition.s`, resident in `.iwram`, with its FFI and three pinned tables. A **default build of this crate contains no kernel, no asm, no FFI and no `unsafe`**. |
| `examples/flac_integration` | The product crate: the decode/verify image and the playback image, both over the embedded `GAFP` asset, plus the host model of the geometry in `tests/`. |
| `examples/overlay-kernel-check` | The kernel's device witness — the only place the assembled ARM code executes (this host has no ARM executor, so mGBA's ARM7TDMI is the oracle). The repo's first ROM suite with real assertions in it. |
| `examples/flac_spike` | The measurement instrument and the `ctrl` arm of every performance record, plus every experiment as a reappliable sample under `experiments/`. |
| `scripts/` | `gafp_pack.py` (the real packer), `gen_gafp_blob.py` (the committed blob's generator/`gafp-check`), `frame_vectors.py` + the `gen_*` scripts (golden vectors, fixtures, assets). `pack_flac.sh` was the unwritten front-end; `gafp_pack.py` and `make master-pack` are what shipped instead. |

## Where performance stands

Full ledger, with every number tied to the image that produced it:
[`flac-docs/measurements.md`](flac-docs/measurements.md). The rows that matter now:

| Path | Result | Where |
|---|---|---|
| **Real audio through the master path, hardware** — 16,384 Hz, GBA 001 ×2 + GBA SP ×2, audible, BLUE | work **32.208%** / worst frame **35.831%** · decode 23.656% · `attr 527,339` c/frame · 289/289 periods · `und 0` · `late 0` · lead floor 9,120 samples (0.557 s) | the C2 acceptance run |
| Committed synthetic (FIXED-0) asset, hardware, chunked push | work **28.600%** / worst **29.781%** · decode 19.961% · `attr 446,292` | 7-2 |
| Decode alone, kernel in a decode slot, hardware | **396,066 / 398,255 c/frame** = 193.4 c/sample | 6b-2 pt 2, the decode row of record |
| Same, core's default `TableCountKernel` | 1,737,135 / 1,745,843 c/frame — the ARM kernel is worth **4.39×** on the product path | 6b-2 pt 2 control |
| LPC (`-l 4`) decode alone, hardware | 173.23% / 174.14% of a 32,768 Hz frame = **86.6% / 87.1%** of a 16,384 Hz one | refused at pack time |

So the shipped path has roughly **3× headroom** on real material at 16,384 Hz, the
latency defect that refused the first 32 kHz runs is closed by `CHUNK`-sized
critical sections (4,014 c ≈ 3.92 samples against a 16-sample grace, a 4.08× margin),
and the ceiling is the decode cost of predicted frames, not the feeder. 8,192 Hz is
the only rung at which LPC projects inside a frame.

## Documentation map

| File | Answers |
|---|---|
| [`flac-docs/architecture.md`](flac-docs/architecture.md) | What exists and why it is shaped this way: the constraint, the two crates, the `RiceKernel` seam, the container, the feature/gate map, the design rules that must survive any edit. |
| [`flac-docs/format-notes.md`](flac-docs/format-notes.md) | Every measured FLAC byte fact, with its witness — frame header layout and its three corrections, subframe/residual/stereo/footer rules, and the table of prior assumptions that measurement killed. |
| [`flac-docs/measurements.md`](flac-docs/measurements.md) | The numbers of record and how to read one: budget arithmetic, the derating rules, the gate surface, golden-vector regeneration, the toolchain pin, cargo config inheritance, what the committed corpus cannot see, and what must not be quoted. |
| [`flac-docs/arm-kernels.md`](flac-docs/arm-kernels.md) | The ARM-mode kernel track: measured ARMv4T machine facts, the kernel set, how assembly is witnessed on a host with no ARM executor, and the pitfalls that each cost an image. |
| [`flac-docs/history.md`](flac-docs/history.md) | How it got here: the acceptance criteria and their closure, the rate ruling, the rung choice, the E-series verdicts, the ladder of items, the questions that were asked and answered, and the closed list not to re-litigate. |
| [`crates/flac-lite/README.md`](crates/flac-lite/README.md) | The crate's own contract: the `GAFP` byte layout, the encode profile, the gates and the config-leak note. |
| [`examples/flac_spike/README.md`](examples/flac_spike/README.md) | The instrument: embedded arms, censuses, gates, how to regenerate the assets. |

## Standing caveats

- Every `podman-*` target has **never been run** in the harness that writes these
  records: rootless podman cannot start there (no `newuidmap`, no podman machine, no
  nested virtualization). The native path is the witnessed one.
- `make test-rom` runs, exits 0, and **collects zero tests** — the root crate's
  `#[test_case]` suite is empty. A green `test-rom` means build → link → boot, not
  that assertions ran. The suite with real cases is `make overlay-kernel-test`.
- `flac-portable-check` is a compile gate: no non-GBA performance claim exists in this
  tree, in either direction.
- The Rice corpus behind the kernel differentials has **0 escaped partitions, no
  sample that folds to `u32::MAX`, and no codeword that overflows the §9.2.7.3
  guard**; mutation arms over it are green *correctly*. See
  [`flac-docs/measurements.md`](flac-docs/measurements.md).
- Archived instruments are still in the tree but no longer have Make entrypoints:
  the spike's 6a-3/6a-5/6a-5b/6a-7 arms and its 10,512 Hz rate flow,
  `examples/arm-kernels/` (E10 groundwork), `examples/e5_fetch/` (the fetch-ratio
  arbitration), and `flac-integration`'s `--no-default-features` A/B control. Their
  sources and their records stand; [`flac-docs/history.md`](flac-docs/history.md)
  §"Retired Make entrypoints" lists each one, what it measured, and the command that
  rebuilds it by hand.
- Comments and older notes cite `FLAC.md` section names from when this file was one
  ~9,000-line notebook. [`flac-docs/history.md`](flac-docs/history.md) §"Where an old
  citation resolves" maps them to the files above.
