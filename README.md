# gba-sound-player

A Rust-based Game Boy Advance audio project. The goal is to push the limits of
the GBA's audio hardware — both the hardware PSG (programmable sound generation)
channels and the PCM (sample-based) playback channels — and to build reusable
libraries that other GBA ROMs can build on top of.

## Goals

1. **Audio fidelity.** Explore what the GBA APU can actually do, from simple PSG
   square/noise/wavetable tones up to high-quality sample playback driven by a
   custom software mixer. Long-term the focus is on the sample-based (PCM)
   channels, since that is where the most headroom for quality lives.

2. **Reusability.** The audio stack and the input stack are each designed to be
   extracted into their own crate/repository so they can be shared across any
   number of top-level GBA ROMs.

3. **Instrument input.** Support real-time instrument input (MIDI or equivalent)
   so a ROM can be used as a playable instrument, not just a fixed playback
   device.

4. **Compressed audio (codecs).** Support compressed audio decoding so more audio fits on the
   cartridge. This goal is split into **separate codec tracks, each with its own entry point,
   plan of record, gates and measurement ledger — none of them a repository-wide spec:**

   | Track | Entry point | Status |
   |---|---|---|
   | **ULI2** | **[ULI2.md](ULI2.md)** + **[crates/uli-lite/PLAN-OF-RECORD.md](crates/uli-lite/PLAN-OF-RECORD.md)** | **accepted 2026-11-13 as the product codec; GBA-first since 2026-11-14.** Phase 0/1 are bit-exact; Phase 1b measured the portable decoder; **PR A (2026-11-14)** wired the product kernel set into `Decoder` and measured a full frame at **698% worst / 505% mean**; **PR B (2026-11-14)** moved the entropy/bit-reader/dequant/M/S hot path into `uli-lite-gba` and measured **204% worst / 177% mean**; **PR C (2026-11-14)** attributed the frame — **63% synthesis kernels / 33% entropy / 4% M/S + wrapper** — and reached **200% / 174%** with a split-`huff` fast path. **U4 is not met** and playback has not started; the remainder is scoped as two rewrites (asm kernels, entropy), not tuning. The live plan and ledger are the plan of record; `ULI2.md` is the decision record. |
   | **FLAC** | **[FLAC.md](FLAC.md)** | the lossless library, built and bit-exact; its live plan is **[flac-docs/plan-of-record.md](flac-docs/plan-of-record.md)** — **the only live plan *for FLAC***, governing `crates/flac-lite`, `crates/flac-lite-gba`, `examples/flac_integration` and the `GAFP` asset, and nothing outside them. Detail split into **[flac-docs/](flac-docs/)** (architecture, format notes, measurements, experiments, method, plan of record, open questions). |
   | **ULC** | **[ULC.md](ULC.md)** | scope and intent recorded; **not currently planned**. |
   | **Opus** | **[OPUS.md](OPUS.md)** | an evaluation opened 2026-09-27 while the FLAC perf gate was failing; not a build plan. |

   The FLAC documents use "the product" and "the product path" in the **FLAC** sense (the
   `product-play.gba` image and its asset). Read that as track-local: the repo's product-codec
   choice is recorded in **[ULI2.md](ULI2.md)**, and the two tracks share ground only where a
   file says so (the ring/FIFO feeder, the geometry, the measurement method).

5. **Upstream higher sample rates.** Add support for higher sample rates (like 65kHz) to the upstream `agb` library to push past the current 32kHz software mixer limit.

## Project ideas (top-level ROMs)

The top-level behaviour of this ROM is intentionally minimal for now. Future
ROMs built on the shared libraries could include:

- **A high-fidelity song player** — play a full arrangement using as much of the
  APU's capability as the real-time budget allows.
- **A playable instrument** — accept MIDI (or similar) input over a cable and
  trigger notes in real time, turning the GBA into a soft-synth / tracker.
- **A tracker / step-sequencer** — an LSDJ-style interface for composing and
  performing live, backed by the same audio library.

## Architecture

The codebase is intended to be layered, with each layer isolatable into its own
repository:

```
┌─────────────────────────────────────────────────────────────┐
│  Top-level ROM (this crate)                                  │
│  - entry point, game loop, UI                                │
│  - composes the audio + input libraries                      │
├─────────────────────────────────────────────────────────────┤
│  Audio library  (planned, separate repo)                     │
│  - PSG channel control (ch 1-4)                              │
│  - PCM / sample playback (ch 5-6)                            │
│  - software mixer, DMA, timer-driven sample clock            │
│  - note/event API (frequency, volume, pan, loop)             │
├─────────────────────────────────────────────────────────────┤
│  Input library  (planned, separate repo)                     │
│  - GBA button polling (agb input)                            │
│  - MIDI / instrument input over the link/serial port         │
│  - event queue consumed by the ROM                           │
└─────────────────────────────────────────────────────────────┘
```

For now the audio and input logic live inline in this crate. As they grow they
will be split out into dedicated crates (`gba-sound-player-lib`, `gba-input-lib`) that
the top-level ROM depends on.

### Current state

- `examples/tone_playback.rs` — minimal ROM that sets the hardware backdrop colour to orange, emits milestone logs via `agb::eprintln!`, and successfully plays a 440 Hz square wave tone using PSG Channel 1.
- `examples/pcm_playback.rs` — demonstrates sample-based (PCM) audio playback using `agb`'s native software mixer and a pre-converted 32kHz `.wav` file.
- `src/main.rs` is a clean slate baseline (no FLAC dependency, builds clean).
- `examples/symphonia_flac_probe/` — standalone FLAC compile probe (`symphonia-bundle-flac` from the keks no-std fork, with its own `Cargo.toml`). **Expected to fail compilation**; it is the evidence for the decision to write our own decoder — see [flac-docs/architecture.md](flac-docs/architecture.md) → "Why we wrote our own decoder".
- `crates/flac-lite/` — our own `no_std`, zero-dependency, zero-allocation FLAC frame decoder, now **portable** (the GBA is the founding platform, not the boundary). The Phase-1 decode path is **complete** through `frame::decode_frame`, plus the stream layer, the per-frame `driver`, CRC verification (`crc-check`), the table-less `scan` walk (feature) and the `RiceKernel` seam; witnessed bit-exact vs `flac -d` on host and in the ROM image. **The `todo!()` list is closed** (6b-1, 2026-11-10): the public entry point is now the manifest-driven `decoder::Decoder` facade over the table-less `scan` walk, with `format::Manifest` parsing real `GAFP`/`STREAM` bytes, `Blocksize::samples`, `EncodeProfile::validate` and the typed `SampleRate` — and a re-grep of `crates/*/src/` for `todo!`/`unimplemented!` returns nothing (re-checked 2026-11-11). Plan in [flac-docs/plan-of-record.md](flac-docs/plan-of-record.md). Decision + design in [FLAC.md](FLAC.md) and [flac-docs/architecture.md](flac-docs/architecture.md); byte-level `GAFP` contract in [`crates/flac-lite/README.md`](crates/flac-lite/README.md).
- `crates/flac-lite-gba/` — **the GBA overlay for `flac-lite`, and since 6b-1 the crate the product ROM links.** Created landed-empty by 6a-3 (2026-11-07); 6a-4 added the ARM-mode Rice kernel (`global_asm!` + the FFI contract); 6a-5d moved it from `.text` to `.iwram` on a measured 4.000 vs ~1.4 cycles/instruction; **6b-1 (2026-11-10) made it the kernel `flac-integration.gba` actually selects by default**, with `make native-flac-rom-default-kernel` as the same-source A/B control (same ROM, `arm-kernel` off, so the decode runs through the core's `TableCountKernel`). `no_std`, `#![deny(unsafe_code)]` (the unsafe budget: 0 items in a default build, 3 with `rice-arm`, 6 with `iwram-pin` as well — placement attributes and the FFI blocks), a path dependency on the core, no `agb` dependency. What it is *for*: the core's portable Rice realization and the GBA/ARM realization of the same map have **opposite** measured performance (+42.6 vs −349 cycles/sample), so the platform-bound one belongs over the core's `residual::RiceKernel` seam in its own crate rather than as ARM-shaped code inside the portable core — and the core's portability claim is untouched. 6a-4 adds the ARM kernels, the `global_asm!`/FFI boundary and its `SAFETY` blocks, and turns on `iwram-pin`. Plan of record: [flac-docs/plan-of-record.md](flac-docs/plan-of-record.md). Gates: `make overlay-check` (overlay **and the core it depends on** type-check for `thumbv4t-none-eabi` — a compile witness; an rlib has no link step), `make overlay-test`, `make overlay-test-twin`.
- `crates/uli-lite/` + `crates/uli-lite-gba/` + `examples/uli_spike/` + `examples/uli_bench/` — the ULI2 product-codec port. Since 2026-11-14 it is **GBA-first**: `uli-lite` is the bit-exact reference and oracle (ULA2 parsing, safe decode, host corpus), and `uli-lite-gba` is the product path (ARM-mode, unchecked kernels in `.iwram`, `ArmKernels`, evolving into the full hot-path decoder in PR B). `Decoder<K>` selects the kernel set through `uli_lite::Kernels`. `make uli-spike-rom` is the Phase 1 correctness ROM; `make uli-bench` is the measurement vehicle: it proves its own decode, times full frames through the core control, `ArmKernels`, and the full product hot path (`GbaDecoder`), and checks them bit-exact on target. PR B measured the full product hot path at **204% worst / 177% mean** of the 262,144-cycle period (533,605 / 463,660 cycles), against `ArmKernels` at 640% and the same-image core control at 1023%; the C/asm reference is 69.4%. Non-synthesis `rest` fell 837,092 → 195,484 cycles/frame, so the synthesis kernels are now the whole remaining gap (PR C). No playback or hardware claim yet. Live plan, gates and ledger: [crates/uli-lite/PLAN-OF-RECORD.md](crates/uli-lite/PLAN-OF-RECORD.md); decision record: [ULI2.md](ULI2.md); the public stock-ULC scope that is **not** currently planned: [ULC.md](ULC.md).
- `examples/flac_spike/` — the perf-gate measurement vehicle (and the `ctrl` arm of every performance record), with each experiment preserved as a reappliable sample under `examples/flac_spike/experiments/`. It also carries the rate-play harness that plays real music at 10,512 Hz. Results: [flac-docs/measurements.md](flac-docs/measurements.md) and [flac-docs/perf-results.md](flac-docs/perf-results.md).
- `examples/flac_integration/` — **the product path** (6b-1/6b-2): a bootable ROM that embeds the committed `GAFP` asset, parses its manifest, and decodes it end to end through `flac-lite`'s public facade with `flac-lite-gba`'s ARM kernel, verifying against the pinned reference PCM before timing a second pass (`FLAC-VERIFY`, `FLAC-DECODE`, `FLAC-IMG-BUDGET`). It is also the consumer that makes the kernel swap attributable — `make native-flac-rom-default-kernel` builds the same source with `flac-lite-gba`'s `rice-arm` off, decoding through the core's table-counting kernel instead. Still **no mixer and no sound output**: that is item 7.
- The project also includes a basic `#[test_case]` suite runnable via `mgba-test-runner`.
- `Makefile` — standardized build/test entrypoints, see **[Build Process](#build-process-standardized)**.
- `Dockerfile` — the containerized build (Debian 12 / `rust:slim-bookworm`,
  nightly + `rust-src` + `rustfmt`, `agb-gbafix`). `rustfmt` is in the image so
  the `format` build pre-step runs (and CI can gate with `make check`) inside
  the container too.

### Build and Test status (as of 2026-08-24)

**Baseline audio and video verified.**

Through a series of recent refactors and debugging sessions, we:
1. Rewrote the app as a simple "hello world" baseline that sets an orange screen.
2. Added debug logging statements (`agb::eprintln!`).
3. Validated that the ROM builds correctly, prints debug statements to the mGBA terminal, and successfully displays the orange screen in an emulator.
4. Identified and fixed a missing "Master Sound Enable" initialization step, successfully activating PSG Channel 1 to play a continuous 440 Hz square wave tone.
5. Implemented a 3-channel diagnostic loop (Sweep/Square, Square, Noise) synchronized with screen colors (Red, Green, Blue) to isolate emulator-vs-hardware audio quirks.

**Debugging Workflow:**
1. **Milestone Logging:** The ROM uses `agb::eprintln!` to log its progress to the terminal running mGBA.
2. **Automated Testing:** We use agb's `#[test_case]` harness.
   - The test runner boots the ROM headlessly in mGBA and captures assertion output.
   - **Recommended: run tests in the container** — no host setup at all. The
     containerized path uses a **multi-stage Dockerfile** to keep the environments
     specialized:
     - The `tester` stage ships nightly + `rust-src` + `agb-gbafix` **plus** the C
       toolchain and `mgba-test-runner` required to boot mGBA.
     - The `builder` stage is pure Rust (used for standard ROM builds and isolation
       gates), keeping test-emulator dependencies out of the everyday build path.
     
     ```sh
     make podman-test         # ROM suite + flac-lite isolation gates (tester + builder stages)
     make podman-test-rom     # ROM #[test_case] suite only (tester stage)
     make podman-flac-test    # flac-lite alone (builder stage, fastest loop)
     ```
   - **Native test setup (optional, faster inner loop):** running `make test-rom`
     on the host needs the pinned nightly toolchain, plus the toolchain that
     builds libmgba (a C library) for `mgba-test-runner`. The channel is pinned in
     [`rust-toolchain.toml`](rust-toolchain.toml) — rustup installs it on the first
     build — so these commands only matter if you want it pre-staged:
     ```sh
     rustup toolchain install nightly-2026-09-04      # the pinned channel
     rustup component add rust-src --toolchain nightly-2026-09-04
     brew install cmake pkg-config libelf make   # the last one for the pin read: use gmake
     cargo install agb-gbafix --version 0.25.0 --locked
     cargo install --git https://github.com/agbrs/agb.git --tag v0.25.0 --locked mgba-test-runner
     ```
   - **Running Tests natively** (the Makefile sets this runner itself; shown for
     a raw `cargo` invocation — through rustup, so it works even when a non-shim
     cargo shadows one on PATH, and naming the pinned channel rather than asking
     for "the newest nightly", which is not the instrument the records use):
     ```sh
     CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER=mgba-test-runner \
       rustup run nightly-2026-09-04 cargo test --target thumbv4t-none-eabi
     ```

1. **Native build** — `make native-rom` compiles, links, and fixes the ROM using
   the host toolchain (the pinned nightly + `rust-src`, per
   [`rust-toolchain.toml`](rust-toolchain.toml), plus `agb-gbafix` 0.25.0
   installed locally).
   Produces the same runnable `gba-sound-player.gba` as the containerized path.
2. **Containerized build** — `make podman-rom` (podman by default, docker
   also works) successfully:
   1. builds the `builder` stage of the image (`rust:slim-bookworm` + nightly +
      `rust-src` + `rustfmt` + `git` + `make` + `agb-gbafix`). It avoids the C
      toolchain and emulator dependencies which are isolated to the `tester` stage.
   2. compiles the ROM (`#![no_std]` + `-Zbuild-std` for `thumbv4t-none-eabi`),
   3. links it with `rust-lld` (via the `agb`-supplied `gba.ld` linker script),
   4. fixes it into a loadable ROM, and
   5. outputs `gba-sound-player.gba` to the host directory.

Both paths produce the same runnable `.gba`. The container is the **standardized**,
reproducible path (identical toolchain everywhere, no host setup needed) and the
only one that works on a host without the native tooling installed; the native
path is the fast local iteration loop.

**Tests follow the same native/container convention** (`make podman-test` ⇄
`make test`), so a machine that can run `make podman-rom` can also run the whole
test suite — including the mGBA `#[test_case]` suite — without installing
anything. See [Build Process → Containerized testing](#containerized-testing).

### Architecture: agb framework baseline

We commit to **agb as the framework** (`#[agb::entry]`, agb display, agb
DMA/timing). We have successfully verified the runtime pipeline: the baseline ROM acquires the graphics controller, changes the backdrop colour to orange, and plays a test tone.

### PCM Audio and Sample Rates

Currently, this project uses the native `agb` software mixer for PCM playback. The `agb` framework enforces a maximum hardware timer frequency of **32,768 Hz** (`Frequency::Hz32768`). It does this because arbitrary sample rates might fail to divide cleanly into the GBA's ~16.78 MHz master clock, resulting in severe CPU and timing overhead.

**Future Goal:** We aim to eventually address this limitation and increase the target audio frequency to a **~65kHz sample rate** (specifically 65,536 Hz, which is a clean divisor of the master clock and was common in high-fidelity commercial GBA titles). This will likely require bypassing `agb`'s built-in `MixerController` and writing a custom DMA/Timer-driven audio mixer in the future.

## Audio Initialization (Lessons Learned)

During the process of bringing up the audio hardware, we identified a critical sequence of operations necessary for the GBA to emit sound. The hardware will completely ignore writes to audio registers if the sound circuit isn't explicitly enabled.

The critical initialization order for GBA audio is:

1. **Master Sound Enable (`SOUNDCNT_X` at `0x04000084`)**
   - **Crucial:** Bit 7 MUST be set to `1` (`0x0080`) before writing to *any* other audio registers. If this bit is `0`, all writes to PSG channels are ignored and forced to `0`.
2. **Master Volume & Panning (`SOUNDCNT_L` at `0x04000080`)**
   - Set left/right master volume (bits 0-2 and 4-6) to maximum (`7`).
   - Enable the specific channels (e.g., Channel 1) on both the left (bit 8) and right (bit 12) outputs. (e.g., `0x1177`).
3. **Sound Output Ratio (`SOUNDCNT_H` at `0x04000082`)**
   - Set the PSG volume ratio to 100% (bits 0-1 set to `2`).
4. **Channel-Specific Configuration (e.g., Channel 1)**
   - **Sweep (`SOUND1CNT_L` - `0x04000060`):** Set to `0` to disable frequency sweep.
   - **Duty/Envelope (`SOUND1CNT_H` - `0x04000062`):** Set duty cycle (e.g., 50% = bit 6-7) and initial volume (bits 12-15).
   - **Frequency/Trigger (`SOUND1CNT_X` - `0x04000064`):** Write the 11-bit frequency value and set the highest bit (bit 15, `0x8000`) to trigger the note.

### Hardware-Specific Quirks (Emulator vs. Real Silicon)

While mGBA is highly accurate, we discovered two specific audio behaviors that only manifest on real GBA hardware:

1. **Envelope Step Time of `0` Mutes Audio:** If you configure a *decreasing* envelope with a step time of `0` (e.g., `0xF080`), mGBA treats this as "constant volume" and plays the tone. Real hardware treats this as an instant drop to `0` volume, resulting in complete silence. Always use a non-zero envelope step (e.g., `4`) if you want the note to fade out, or configure an *increasing* envelope for sustained notes.
2. **Sweep Unit "First Trigger" Bug:** On real hardware, the very first time you trigger Channel 1 (the only channel with a sweep unit), the sound may not play. The uninitialized sweep hardware consumes the first trigger pulse just to reset its internal state machine, failing to restart the oscillator. **Workaround:** "Double-trigger" the channel on its very first play by writing the frequency and trigger bit (`0x8000`) to `SOUND1CNT_X` twice in rapid succession.

*Note: Many channel frequency/trigger registers are write-only. Reading from them will usually return `0` or normal hardware fallback values.*

## Build Process (Standardized)

All building and testing goes through the **`Makefile`** — one set of entrypoints,
two compute environments. `make help` lists everything.

The split exists because the two environments differ only in *virtualization*:
containerized hosts can run podman; some hosts (e.g. this agent's VM) cannot nest
virtualization, so podman/docker is unavailable there. Every target therefore has
a native twin that produces the **same artifact** from the same toolchain
requirements (nightly + `rust-src`, plus `agb-gbafix` for ROM fixing).

| Target | What it does | Environment |
|---|---|---|
| `make native-rom` | Build + link + fix `gba-sound-player.gba` with the host toolchain | no virtualization needed |
| `make podman-rom` | Build the `gba-builder` image and run the same ROM build inside it; `CONTAINER=docker` works | needs podman/docker |
| `make native-flac-rom` | **EXPERIMENTAL** — build `flac-integration.gba`: a bootable ROM that links `crates/flac-lite` in | no virtualization needed |
| `make podman-flac-rom` | Same FLAC integration ROM, built in the container | needs podman/docker |
| `make native-flac-rom-default-kernel` | 6b-1's A/B control for the image above: the same source with `flac-lite-gba`'s `rice-arm` off, decoding through the core's `TableCountKernel` (`flac-integration-default-kernel.gba`) | no virtualization needed |
| `make product-play-rom` / `native-product-play-rom` | **EXPERIMENTAL, and the only image built to play at the 32,768 Hz target** (6b-3; **never heard** — mGBA headless only so far): `product-play.gba`, a second binary of the same product crate driving decode → 8-bit FIFO convert → ring → sound DMA at 32,768 Hz, counting its own underflows, gating its period audit (coalesced / late / clock / tail / void) and the played-form FIFO pins, and printing a verdict to log and screen. The ROM image is not gated: build it and run it by hand (its geometry model is gated by `make product-play-test`); a **pass ends in `loop {}`, so the timeout's exit 124 is expected** — read the log. Its asset is **not** a product asset yet (`Q14`) | no virtualization needed |
| `make product-play-test` | 6b-3's **host model** (12 tests): the ROM's own `RingFeed` and `geometry` constants replayed on the host against a credit-per-period clock — where the flow control, the two tolerances and the buffer sizing are justified rather than asserted | no virtualization needed |
| `make flac-test` | `flac-lite` **in isolation**: GBA target compile gate + host unit tests. No agb, no ROM, no emulator. Default features — the frame walk (`scan`) and the portable hybrid Rice kernel (`rice-hybrid`) are **off**, so no ROM links either. `make flac-test-crc` is the same shape with `crc-check` on | native |
| `make flac-test-scan` | The same two steps with `--features scan` on: the walk's thumbv4t compile plus its witness suite (`tests/scan_frame_walk_layout.rs` is `required-features = ["scan"]`, so it is not built at all without the feature). Deliberately **not** in `check-gba-all` — the thing that must not carry the walk yet is the ROM. `flac-portable-check` runs with `scan` (and `rice-hybrid`) **on** so the second bare-metal target keeps compiling them | native |
| `make flac-test-rice` | Same shape again with `--features rice-hybrid` on: the portable hybrid Rice kernel's thumbv4t compile plus the **two-mechanism Rice differential** (`tests/rice_two_mechanism_differential.rs` is `required-features = ["rice-hybrid"]`). The gate exists because the module's cost is measured, not assumed: with the feature off the ROM-links image is byte-for-byte the pre-PR-3 image; with it on the calibration instrument grows **+2,984 B of `.text` while `nm` reports no hybrid symbol in the image**. Also deliberately **not** in `check-gba-all` — no ROM links the twin, and the default flip was gated on an on-target cycle reading and **declined** ([flac-docs/measurements.md](flac-docs/measurements.md)) | native |
| `make flac-gates` | **Every `crates/flac-lite` configuration in one command**: `flac-test` + `flac-test-wide` + `flac-test-crc` + `flac-test-scan` + `flac-test-rice` + `flac-portable-check` + `gafp-check` + `gafp-pack-check`, plus the combination arms — `crc-check,scan`, and `crc-check,scan,rice-hybrid` (the one build where the walk, the CRC verification and **both** Rice mechanisms compile at once). Adds no new witness: it is the aggregate the TODO below asked for, by the name it gave it, landed 2026-11-05, extended with the third configuration on 2026-11-06, with the no-default-features arm (`flac-test-wide`) on 2026-11-07 and with the two `gafp` legs on 2026-11-11. The count moves with the tree, so it is recorded per landing, not here — see the per-arm table in [flac-docs/plan-of-record.md](flac-docs/plan-of-record.md) | native |
| `make overlay-check` | **`crates/flac-lite-gba` and the core it depends on both type-check for the GBA target** (item 6 PR 6a-1's gate). A *compile* witness only: an rlib has no link step, it claims no codegen charge. Since 6b-1 the product image **does** link this overlay, and the link witness moved to `make flac-rom` + `make native-flac-rom` plus the `nm` `.iwram` witness in 6a-5d — this target remains the type-check of the crate on its own. Deliberately not in `flac-gates` — that aggregate is every configuration **of the core**, and this is a different crate | native |
| `make overlay-test` | the overlay crate's own host witness, run out-of-tree (the cargo-config rule) — in 6a-1 that is one no-op test by design, and it is the live-edge witness for the path dependency. Also not in `test` or `flac-gates`: plumbing gates, hand-invoked like `flac-test-crc/-scan/-rice` | native |
| `make test-rom` | ROM `#[test_case]` suite, headless in mGBA via `mgba-test-runner` | native (needs `mgba-test-runner`) |
| `make test` | Top-level: `test-rom` + `flac-test`. Verify-only — never reformats your tree | native |
| `make podman-test` | **The same suite**, running inside the image (`make test` in there) — no host tooling | needs podman/docker |
| `make podman-test-rom` | ROM suite only, in the container | needs podman/docker |
| `make podman-flac-test` | `flac-lite` gates only, in the container (fastest container loop) | needs podman/docker |
| `make build` / `rom` / `flac-rom` | Lower-level primitives (no fixing / container); `rom` is what the container runs. All run `format` first; **none gate on tests** | — |
| `make format` | `cargo fmt` (writes) across all three workspaces — the automatic build pre-step | either |
| `make podman-check` | `cargo fmt --check` in the container (the CI formatting gate) | needs podman/docker |
| `make clean` / `check` / `help` | Clean all workspaces incl. sub-crates; `cargo fmt --check` (**verify-only**, never writes); target list | either |
| `make clean-cache` | Remove the container build-cache volumes (see [Containerized testing](#containerized-testing)) | needs podman/docker |

#### Formatting: builds auto-format; nothing but `check` gates

Every **build** runs **`make format`** (`cargo fmt`, writing, all three
workspaces) as a pre-step — including inside the container, whose bind mount
means `make podman-rom` formats your working tree. So the routine loop is just:

```sh
make native-rom        # formats, then builds — code lands rustfmt-clean
```

Two deliberate separations:

- **Builds never depend on `test`** — experimental, not-yet-passing code must
  stay buildable. Run gates explicitly (`make test`) or in CI.
- **Tests never reformat** — `make test` must not rewrite your working tree as
  a side effect, and a CI `make test` passing must never mask drift that
  `make check` exists to fail on.

| | Writes files? | Fails on unformatted? | Fails on test failures? |
|---|---|---|---|
| `make build` / `*-rom` | ✅ (`format` first) | no — fixes them | no |
| `make test` / `test-rom` / `flac-test` / `flac-gates` | never | no | ✅ |
| `make check` | never | ✅ | no |

If a build reformats your tree, run `make check` before committing to confirm
the result is gate-green. If `rustfmt` is missing for the pinned toolchain,
`format` prints a notice and the build continues — formatting is a convenience,
never a hard dependency of building.

#### TODO: one aggregate for the non-default-configuration gates

`make test` runs `test-rom` + `flac-test`, and the repo has no CI workflows. The
gates that exercise a **non-default build configuration** are therefore manual by
omission: `flac-test-crc` (frame CRC verification ON), `flac-test-scan` (the frame
walk ON), `flac-test-rice` (the portable hybrid Rice kernel ON) and
`flac-portable-check` (second bare-metal target) run only when someone
remembers them — and an un-run gate is an un-witnessed gate, which is exactly the
risk a by-default-OFF feature like `crc-check`, `scan` or `rice-hybrid` creates.
`scan` and `rice-hybrid` make the point twice over: a module no default build
compiles is witnessed by `flac-test-scan` / `flac-test-rice` (their rules, and the
GBA-target compile) and by `flac-portable-check`'s `--features scan,rice-hybrid`
(the second target) — nothing else builds them at all. Tracked here so it is not forgotten; when written it was deliberately
not part of any feature PR, since each gate lands with its own PR's gate work —
that part changed on 2026-11-05, when the aggregate landed inside split-ladder
item 5 PR 2 (the PR where the *instantiation count* changes rather than the code,
which is the natural home for a feature *combination*). The invocation hook below
is still open.

- ✅ **Landed 2026-11-05** — a single `flac-gates` aggregate: `flac-test` + `flac-test-crc` +
  `flac-test-scan` + `flac-portable-check`, **plus** the two `--features
  crc-check,scan` arms (thumbv4t `cargo check`, and the host test run from the
  gate-neutral cwd) that no single-feature gate runs — one command covering every
  `crates/flac-lite` configuration. Measured on the `crc-check` landing tree with warm caches: the
  extra thumbv4t `cargo check` and host test run complete in hundredths of a
  second; cold caches pay the usual `-Zbuild-std` cost, once. Measured on item 5
  PR 2's landing host, twice, with every cache warm and everything already built:
  **0.72 s** and **0.75 s** for the aggregate, of which the two combination arms
  are **0.18–0.21 s** — the four suite runs dominate, exactly as the sentence
  above predicts. (The first run after a source edit costs more — 2.95 s on the
  re-run following this PR's own doc-text change — because "warm" is not the state
  a tree is in immediately after an edit.)
- ✅ **Extended 2026-11-06 (split-ladder item 5 PR 3)** — the aggregate gained a
  third feature configuration and a second combination arm: `flac-test-rice`
  (`--features rice-hybrid`, the portable hybrid Rice kernel) and the
  `crc-check,scan,rice-hybrid` pair, which is the only build in which the frame
  walk, CRC verification and **both** Rice mechanisms compile together. `flac-portable-check`
  now compiles the twin for the second bare-metal target too. The counts are what
  a feature gate is supposed to produce: **178 / 181 / 198 / 194 / 201 / 217, one
  ignored each** — the default configuration's number **unchanged** by a PR that
  adds 16 tests, because those tests are not in the build the ROM links (1,169
  passed + 6 ignored across the six configurations). Warm timings moved the way
  adding two arms predicts: **1.00 s** warm for the aggregate (PR 2's four
  configurations were 0.72–0.75 s; a run taken straight after a source edit is
  2.15 s and is not a warm number), `flac-test-rice` itself 0.17 s.
- Something to *invoke* it: with no CI, the cheap honest hook is local (a
  pre-commit/pre-push hook, or a documented rule that any commit touching
  `crates/flac-lite` runs `make flac-gates` first). [flac-docs/method-and-gates.md](flac-docs/method-and-gates.md)
  names the gate for each landed feature — only the automation is missing.
- Still open alongside it, left out of the aggregate's own PR on purpose: a
  `podman-flac-gates` sibling, so the container path covers the same set. Today
  `make podman-flac-test` covers the **default** configuration only — the
  non-default configurations have no containerized path, which is the same
  omission this section was written about.

### Containerized testing

Tests follow the same convention as ROM builds, and for the same reason: the
container is the environment-independent path. **`make podman-rom` working on a
host now means `make podman-test` works on that host too** — the `gba-builder`
image ships `agb-gbafix` *and* `mgba-test-runner` (a headless mGBA built from
source at image-build time), so nothing about mGBA, `cmake`, `libelf`, or the
exact nightly has to be true on the host.

```sh
make podman-test         # ROM #[test_case] suite + flac-lite isolation gates
make podman-flac-test    # flac-lite alone — fastest loop, no agb/emulator
make podman-check        # formatting gate, same rustfmt as CI
```

How it works: each `podman-*` target builds the image, then runs the **bare**
gate target (`make test`, `make flac-test`, `make test-rom`, `make check`) *inside*
the container with the project dir bind-mounted. The bare targets are unaware of
containers, exactly like `rom` — so the commands, flags, and gate semantics are
identical on both paths, and the artifacts/reports land back in your working tree.

**Build caches.** The bind mount shadows the image's `/app/target`, so the
targets mount three named volumes (`gba-cargo-target`, `gba-flac-lite-target`,
`gba-flac-integration-target`) over the crates' build dirs. Without them every
container run would rebuild `core`/`alloc` and every dependency from scratch
(`-Zbuild-std` is not cheap). They are pure caches: `make clean-cache` drops them
safely, and the next run just rebuilds slowly once. Override the names with
`CARGO_CACHE=` / `FLAC_CACHE=` / `FLACROM_CACHE=` / `SPIKE_CACHE=` /
`OPUS_CACHE=` / `KERNEL_CACHE=` / `OVERLAY_CACHE=`, or the runtime with
`CONTAINER=docker`.

**Native still wins for iteration speed** when the host has the tooling — no
image build, no container startup, warm incremental `target/`. Use it as the
inner loop, keep `podman-test` as the truth.

### The FLAC sanity-check triangle

Goal #4 development keeps three independent signals separate, so a failure is
always diagnosable without guesswork:

1. **`make flac-test`** — is the decoder *correct and `no_std`-clean* on its own?
   Runs both load-bearing gates for `crates/flac-lite` (see below).
2. **`make native-flac-rom`** / **`make podman-flac-rom`** — does the decoder
   still *bundle into a ROM*: compile, link (`rust-lld` + `gba.ld`), get fixed,
   and boot?
3. **`make test`** — does the whole ROM (baseline + audio stack + tests) still
   pass?

If (1) passes but (2) fails, the problem is **integration** — linking, memory
budget, scheduling — not the decoder. That isolation is the whole point of (1);
the FLAC library is deliberately *not* a dependency of the root ROM crate, so a
broken experiment can never take the baseline down with it.

> **Current state of the FLAC ROM build:** `flac-integration.gba` compiles,
> links, fixes, and boots (purple backdrop, logs the linked anchor address).
> The decoder behind it is now complete and bit-exact, but this ROM deliberately
> still runs only a live `BitReader` proof: it references the decode path through
> a `#[used]` link anchor that is never called, so the image exercises the full
> build path without running a decode loop. Replacing the anchor with a real
> decode loop plus mixer/DMA playback is Phase 2 step 9 in
> [flac-docs/plan-of-record.md](flac-docs/plan-of-record.md).

### Toolchain resolution (conda / non-rustup `cargo` on PATH)

The Makefile does **not** use `cargo +nightly`. That syntax only works when the
`cargo` found on `PATH` is the **rustup shim**; under conda (the `(base)` prompt)
or some Homebrew/distro setups a *plain* cargo comes first, and it reads
`+nightly` as a subcommand:

```
error: no such command: `+nightly`
help: invoke `cargo` through `rustup` to handle `+toolchain` directives
```

Instead the Makefile detects rustup and pins the toolchain through it, so the
same targets work in a conda shell, a bare shell, and inside the container:

```make
HAS_RUSTUP := $(shell command -v $(RUSTUP) >/dev/null 2>&1 && echo 1)
# rustup present   -> `rustup run <pinned channel> cargo …` (immune to a shadowed shim)
# no rustup at all -> plain `cargo`/`rustc`, which must already be the pinned nightly
```

#### Which nightly, exactly (the channel is pinned)

`nightly` is a channel, not an instrument: `rustup update` replaces what it
points at silently. This repo compares ROM shas, section hashes and cycle counts
across sessions and hosts ([flac-docs/measurements.md](flac-docs/measurements.md)),
so the channel is pinned to a
dated one — `nightly-2026-09-04`, i.e. `rustc 1.100.0-nightly (a69a63265
2026-09-03)`, the instance the existing records name — in
[`rust-toolchain.toml`](rust-toolchain.toml). Dated channels resolve from the
archived date-stamped channel manifests, which is what makes the toolchain
re-installable months later on a machine nobody has.

Three things about that file that are not obvious:

- A `rust-toolchain.toml` at the repo root also governs the standalone workspaces
  under it (`crates/*`, `examples/*`): rustup walks **up the directory tree**, not
  the workspace graph. A file in a subdirectory would shadow it.
- `rustup run <channel>` **selects** a toolchain explicitly and therefore
  overrides the file. That is exactly the form the Makefile uses, so it reads the
  channel out of the file (`PINNED_TOOLCHAIN` → `TOOLCHAIN ?=`) instead of
  hard-coding a second copy of it. A pin that lived only in the file would not
  pin `make`.
- The read is done with **make's own file function** (`$(file <)`) and a
  quoted-word match — no `sed`, no pipeline, no second copy of the string. That
  function needs **GNU make 4.2 or newer**: Linux makes are fine, Apple still ships
  3.81 at `/usr/bin/make`, so on macOS use `brew install make` and run `gmake`. The
  Makefile checks its own make version and names that fix, rather than reporting a
  malformed pin file.

`Dockerfile` installs the channel `rust-toolchain.toml` names: it copies the file
and reads it with the **same rule** the Makefile uses (`make --eval` + the
quoted-word match), so there is neither a second copy of the string nor a second
rule for it to drift — and that side installs `make` itself, so it carries no
version guard; a too-old make there would fail the image build at the read, not
silently install a different compiler. It pins
`agb-gbafix` at `0.25.0` for the same reason it pins the mGBA runner to
`v0.25.0`: gbafix rewrites the ROM header, so it is part of the ROM bytes the
records compare by sha. Changing the pin changes the instrument — carry a control
build and a section/symbol dump ([flac-docs/plan-of-record.md](flac-docs/plan-of-record.md)
→ "Baselines to hold").

Overrides: `TOOLCHAIN=`, `RUSTUP=`, `CARGO=`, `RUSTC=`. `make help` prints the
resolved toolchain and host triple; `toolchain-check` (a prerequisite of the
test gates) fails fast with install instructions rather than half-running a
gate — and it also asks **rustup** what it resolves in this tree, stopping if that
is not the instance the build runs. That second half is what catches a pin file
rustup is quietly ignoring: a commented-out `channel` line, or a `RUSTUP_TOOLCHAIN`
in the environment, leaves rustup on the machine default with exit 0 while the
Makefile still builds the pinned instance — two instruments in one tree, which the
pin exists to prevent. Prerequisites for a native setup:

```sh
# GNU make 4.2+: the Makefile reads rust-toolchain.toml with make's own file
# function. Linux makes are fine; on macOS: brew install make, then run gmake.
rustup toolchain install nightly-2026-09-04                     # pinned channel
rustup component add rust-src --toolchain nightly-2026-09-04   # -Zbuild-std
rustup component add rustfmt  --toolchain nightly-2026-09-04   # make check + build's format pre-step
cargo install agb-gbafix --version 0.25.0 --locked              # ROM fixing
cargo install --git https://github.com/agbrs/agb.git --tag v0.25.0 --locked mgba-test-runner  # make test-rom
```

### Requirement: nightly + `rust-src` + `-Zbuild-std` (no prebuilt `core` for this target)

The GBA target `thumbv4t-none-eabi` is a **Tier-3, `#![no_std]`** bare-metal
target. Per the [Rust platform-support docs](https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html),
its library support is **`core` and `alloc` only** — no `std`, and rustup ships
**no prebuilt `core`/`alloc` artifacts** for it on *any* toolchain (stable *or*
nightly). So `rustup target add thumbv4t-none-eabi` fails by design and is the
wrong approach.

A `#![no_std]` build still needs **`core`** (it is mandatory — `#![no_std]` only
means *no `std`*). Since there are no prebuilt `core` artifacts for this target,
we compile `core`/`alloc` **from source for `thumbv4t`** using `-Zbuild-std`
(the compiler's own suggestion for `E0463: can't find crate for core`). That
requires all three:

- **a nightly toolchain** — `-Zbuild-std` is a nightly-only flag;
- **the `rust-src` component** — provides the `core`/`alloc` source to compile;
- **`build-std` enabled** — already set in `.cargo/config.toml`
  (`[unstable] build-std = ["core", "alloc"]`), so a plain `cargo build
  --target thumbv4t-none-eabi` picks it up automatically.

This is a toolchain/platform reality, independent of host OS. It is **not** a
"missing dependency" and it is **not** fixable by dropping nightly — nightly is
required *because* of `-Zbuild-std`. Do not try to install a std target for this
target; compile `core`/`alloc` from source instead.

> The earlier "drop nightly / build no_std directly" attempt failed with
> `error[E0463]: can't find crate for core` — this target needs `core` built
> for it, which is exactly what `-Zbuild-std` (on nightly) provides.

You have two main ways to build and develop:

1. **Native build (local development)**
   ```sh
   make native-rom
   ```
   Compiles, links, and fixes the ROM for `thumbv4t` (via `-Zbuild-std`) using the
   host toolchain — the fast iteration loop. Requires `nightly` + `rust-src` +
   `agb-gbafix` on PATH. Raw cargo equivalent:
   ```sh
   cargo +nightly build --release --target thumbv4t-none-eabi
   ```

   On macOS you may need the toolchain's `lib` on the dynamic loader path so
   `rust-lld` finds `libLLVM.dylib`:
   ```sh
   export DYLD_FALLBACK_LIBRARY_PATH="$HOME/.rustup/toolchains/nightly-<arch>/lib"
   ```

2. **Container runtime (CLI) — the standardized, reproducible build**
   The default runtime is **podman**; override with docker if you prefer.
   Run:
   ```sh
   make podman-rom
   ```
   This builds the image, mounts your local directory into the container,
   compiles the ROM (`#![no_std]` + `-Zbuild-std`, target `thumbv4t-none-eabi`),
   fixes it, and outputs `gba-sound-player.gba` directly to your host directory.
   Prefer docker? `make podman-rom CONTAINER=docker`.

3. **Using VS Code Dev Containers (IDE)**
   The repository includes a `.devcontainer` configuration. Open this folder in
   VS Code and click "Reopen in Container" to run your IDE and `rust-analyzer`
   inside the Debian environment with full GBA target autocomplete.

Run `gba-sound-player.gba` in any GBA emulator (mGBA recommended) on a host that
has an emulator, to see the title screen and hear the 440 Hz tone.

### Note: cargo config inheritance (why the flac gates carry extra flags)

Cargo reads `.cargo/config.toml` by walking **up** parent directories and merging
every layer — a `[workspace]` boundary does not stop it. `crates/flac-lite`
therefore inherits the root's GBA settings, and both inherited values break a
naive host test: the `target` default gives `E0463: can't find crate for 'test'`,
and the inherited `build-std` gives `E0152: duplicate lang item`. Because cargo
**merges arrays** across config layers, an empty local `build-std = []` does not
clear the parent's — and as of nightly 2026-09-03 the inherited value also beats
every *in-tree* override (`-Zbuild-std=`, the empty `CARGO_UNSTABLE_BUILD_STD=`,
`--config unstable.build-std=[]`), so the flag-based workaround we used through
2026-06 no longer holds. The durable fix is positional: run cargo from outside the
repo so the config walk finds nothing, and point it at the crate explicitly. The
`flac-test` target encodes it:

```sh
mkdir -p /tmp/gba-sound-player-host-gate && cd /tmp/gba-sound-player-host-gate && \
  cargo +nightly test --manifest-path <repo>/crates/flac-lite/Cargo.toml \
    --target "$(rustc -vV | sed -n 's|host: ||p')"
```

Artifacts still land in `crates/flac-lite/target/` (derived from the manifest, not
the cwd). Full write-up, including the dated A/B matrix and the `-Tgba.ld`
double-pass variant that breaks linking in standalone sub-crates, lives in
**[flac-docs/method-and-gates.md](flac-docs/method-and-gates.md)** → "Cargo config
inheritance".

## References

- <https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html> — `thumbv4t-none-eabi` is Tier 3, `#![no_std]` (`core` + `alloc` only, no prebuilt std)
- <https://gbadev.net/> — GBA hardware reference
- <https://gbadev.net/gbadoc/audio/registers.html> — GBA audio hardware registers reference
- <https://gbadev.net/gbadoc/audio/sound1.html> — GBA audio Sound Channel 1 reference
- <https://github.com/agbrs/agb> — the `agb` Rust library this project builds on
- [RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html) — the FLAC format specification the decoder is written against
- <https://jsgroth.dev/blog/posts/gba-audio/> — detailed GBA audio hardware
  analysis (mixing, DMA, timers)
- <https://www.littlesounddj.com/lsd/index.php> — LSDJ, the reference
  GBA tracker/instrument this project is loosely inspired by
