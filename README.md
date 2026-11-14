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
   | **FLAC** | **[FLAC.md](FLAC.md)** | the lossless library: **done and accepted**. `no_std`, bit-exact against `flac -d`, and playing real audio at 16,384 Hz on two GBA units with zero underruns. Governs `crates/flac-lite`, `crates/flac-lite-gba`, `examples/flac_integration` and the `GAFP` asset, and nothing outside them. Detail in **[flac-docs/](flac-docs/)** — architecture, format notes, the measurements/method ledger, the ARM kernel, and the rulings and history. |
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
- `crates/flac-lite/` + `crates/flac-lite-gba/` + `examples/flac_integration/` — **the FLAC
  track, complete and accepted**: a `no_std`, zero-dependency, zero-allocation decoder whose
  public entry point is the manifest-driven `decoder::Decoder` facade over the table-less
  `scan` walk, witnessed bit-exact against `flac -d` on host and in the ROM; a GBA overlay
  crate holding the platform-bound `RiceKernel` realizations (the shipped one is an
  ARM-mode codeword loop resident in `.iwram`); and the product crate that embeds a `GAFP`
  asset and *plays* it at 16,384 Hz with zero underruns on real hardware. Gates:
  `make flac-check` (core configurations), `make overlay-gates` (the kernel, including its
  mGBA device suite), `make product-play-test` (the geometry's host model).
  Entry point: [FLAC.md](FLAC.md); detail in [flac-docs/](flac-docs/).
- `crates/uli-lite/` + `crates/uli-lite-gba/` + `examples/uli_spike/` + `examples/uli_bench/` — the ULI2 product-codec port. Since 2026-11-14 it is **GBA-first**: `uli-lite` is the bit-exact reference and oracle (ULA2 parsing, safe decode, host corpus), and `uli-lite-gba` is the product path (ARM-mode, unchecked kernels in `.iwram`, `ArmKernels`, evolving into the full hot-path decoder in PR B). `Decoder<K>` selects the kernel set through `uli_lite::Kernels`. `make uli-spike-rom` is the Phase 1 correctness ROM; `make uli-bench` is the measurement vehicle: it proves its own decode, times full frames through the core control, `ArmKernels`, and the full product hot path (`GbaDecoder`), and checks them bit-exact on target. PR B measured the full product hot path at **204% worst / 177% mean** of the 262,144-cycle period (533,605 / 463,660 cycles), against `ArmKernels` at 640% and the same-image core control at 1023%; the C/asm reference is 69.4%. Non-synthesis `rest` fell 837,092 → 195,484 cycles/frame, so the synthesis kernels are now the whole remaining gap (PR C). No playback or hardware claim yet. Live plan, gates and ledger: [crates/uli-lite/PLAN-OF-RECORD.md](crates/uli-lite/PLAN-OF-RECORD.md); decision record: [ULI2.md](ULI2.md); the public stock-ULC scope that is **not** currently planned: [ULC.md](ULC.md).
- `examples/flac_spike/` — the perf-gate measurement vehicle (and the `ctrl` arm of every
  performance record), with each experiment preserved as a reappliable sample under
  `examples/flac_spike/experiments/`. Its archived arms — the 6a-3/6a-5/6a-5b/6a-7
  instruments and the 10,512 Hz rate-play harness that first played real music at tempo —
  have no Make target any more; [flac-docs/history.md](flac-docs/history.md) says what each
  measured and how to rebuild one by hand.
- `examples/arm-kernels/`, `examples/e5_fetch/` — archived FLAC groundwork (the E10 kernel
  set the shipped asm came from; the emulator-vs-hardware fetch arbitration). Sources and
  records kept, no Make entrypoints.
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
| `make master-pack INPUT=track.wav` → `make master-play-rom INPUT=track.wav` | **the FLAC happy path**: resample/encode a real master to the product profile, pack the `GAFP` asset and its pins, then build `master-play.gba` — the image that *plays* it at 16,384 Hz | native (needs `ffmpeg` if the input must be resampled, `flac` to encode) |
| `make native-flac-rom` | `flac-integration.gba`: links the core + overlay, embeds the committed asset, decodes every frame through the facade, checks the pinned PCM, then times a second pass. No sound. Also the real **link** witness for the core (what `overlay-check`, an rlib, cannot be) | native or container |
| `make product-play-rom` / `native-product-play-rom` | `product-play.gba`: the same crate's second binary on the committed asset — decode → 8-bit FIFO convert → ring → sound DMA at 16,384 Hz, counting its own underflows, gating its period audit and its played-form FIFO pins, verdict to log **and** screen. Not gated: run it by hand, and a **pass ends in `loop {}`, so the timeout's exit 124 is expected** — read the log | native |
| `make product-play-test` | the product geometry's **host model**: the ROM's own `RingFeed` and `geometry` constants replayed against a credit-per-period clock, where flow control, the tolerances and the ring sizing are justified rather than asserted | native |
| `make flac-test` (+ `flac-test-wide` / `-crc` / `-scan` / `-rice`, `flac-portable-check`) | `flac-lite` **in isolation**, one gate per build configuration: GBA target compile + host tests, no agb / ROM / emulator. Each non-default feature gets its own gate because the default build does not compile what it covers | native |
| `make gafp-check` / `make gafp-pack-check` | the committed product blob still matches its generator; and the **real packer** reproduces it byte for byte, re-runs its fixture set with the pack-time **refusals**, and reads it all back through the facade (needs `flac` + `metaflac`) | native |
| `make flac-gates` | every `crates/flac-lite` configuration in one command, plus the `crc-check,scan` and `crc-check,scan,rice-hybrid` combination arms — the only builds in which a *second instantiation* is visible | native |
| `make overlay-gates` | `crates/flac-lite-gba` in one command: both configurations' compile + host witnesses, the objdump fact (the ELF really is four-byte ARM code), and the assembled kernel's device suite on mGBA's ARM7TDMI — the only place that assembly executes. Individually: `overlay-check`, `overlay-test`, `overlay-check-arm`, `overlay-test-arm`, `overlay-objdump`, `overlay-kernel-rom`, `overlay-kernel-test` | native |
| `make spike-rom` / `make spike-test` | the measurement instrument (`flac-spike.gba`) and its host bit-exactness witness. It is the `ctrl` arm of every performance record | native |
| **`make flac-check`** | everything a FLAC-track change owes: `flac-gates` + `overlay-gates` + `product-play-test` + `spike-test` | native |
| `make test-rom` | ROM `#[test_case]` suite, headless in mGBA via `mgba-test-runner` | native (needs `mgba-test-runner`) |
| `make test` | Top-level: `test-rom` + `flac-test`. Verify-only — never reformats your tree | native |
| `make podman-test` | **The same suite**, running inside the image (`make test` in there) — no host tooling | needs podman/docker |
| `make podman-test-rom` | ROM suite only, in the container | needs podman/docker |
| `make podman-flac-test` | `flac-lite` gates only, in the container (fastest container loop) | needs podman/docker |
| `make build` / `rom` / `flac-rom` | Lower-level primitives (no fixing / container); `rom` is what the container runs. All run `format` first; **none gate on tests** | — |
| `make format` | `cargo fmt` (writes) across every workspace — the automatic build pre-step. `examples/flac_integration` goes through `scripts/fmt_with_generated_stub.sh`, so a fresh clone without the generated `master_pin.rs` still formats | either |
| `make podman-check` | `cargo fmt --check` in the container (the CI formatting gate) | needs podman/docker |
| `make clean` / `clean-deep` / `check` / `help` | `clean` removes the build **outputs** — every `*.gba`/`*.sav`, the generated master asset + pin, the archived rate assets, `uli-bench*.log`, `__pycache__` — and **keeps the compiler caches**. `clean-deep` (or `make clean DEEP=1`) also drops every workspace's `target/` — the workspaces are **discovered from the `Cargo.toml`s on disk**, not listed by hand — and the next build pays the `-Zbuild-std` `core`/`alloc` compile again (measured here: 135 s and 4.5 GB against 22 s warm). `check` is `cargo fmt --check` (**verify-only**, never writes); `help` is the target list | either |
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

#### Invoking the gates

`make test` is the fast top-level suite (`test-rom` + `flac-test` + `uli-test`); the gates
that exercise a **non-default build configuration** are deliberately separate
(`flac-test-crc`, `flac-test-scan`, `flac-test-rice`, `flac-test-wide`,
`flac-portable-check`, the `overlay-*` set), because an un-run gate is an un-witnessed gate
and that is exactly the risk a by-default-OFF feature creates. The aggregate that closes the
gap is **`make flac-gates`** (every `crates/flac-lite` configuration plus the combination
arms, landed 2026-11-05 and extended since) and **`make flac-check`** for the whole FLAC
surface. The rule they exist to encode: **any commit touching `crates/flac-lite` runs
`make flac-gates` first.** `.githooks/pre-commit` runs `make test` on every commit; wiring it
to `make flac-check` for FLAC-touching commits is an open (non-FLAC-specific) convenience, and
there is no `podman-flac-gates` sibling — the aggregate is native-only.

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
`podman-*` build targets mount one named volume per workspace (`gba-cargo-target`,
`gba-flac-lite-target`, `gba-flac-lite-gba-target`, `gba-flac-integration-target`,
`gba-flac-spike-target`, `gba-opus-spike-target`, `gba-overlay-kernel-check-target`) over
the crates' build dirs. Without them every container run would rebuild `core`/`alloc` and
every dependency from scratch (`-Zbuild-std` is not cheap). They are pure caches:
`make clean-cache` drops them safely, and the next run just rebuilds slowly once. Override
the names with `CARGO_CACHE=` / `FLAC_CACHE=` / `FLACROM_CACHE=` / `SPIKE_CACHE=` /
`OPUS_CACHE=` / `OVERLAY_CACHE=` / `OVERLAY_ROM_CACHE=`, or the runtime with
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

> **Current state:** all three signals are live and separate. `flac-integration.gba`
> embeds the committed `GAFP` asset, parses its manifest and decodes every frame through the
> facade with the overlay's ARM kernel, checking the pinned PCM before timing a second pass;
> `product-play.gba` / `master-play.gba` go on to *play* it at 16,384 Hz through the ring and
> the sound DMA, counting their own underflows. The `#[used]` link-anchor scaffold this note
> described was replaced by items 6b-1…6b-3; playback closed at 7-3
> ([flac-docs/history.md](flac-docs/history.md)).

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
build and a section/symbol dump ([flac-docs/history.md](flac-docs/history.md)
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
   `agb-gbafix` on PATH. Raw cargo equivalent — through the rustup shim, with the
   channel read out of `rust-toolchain.toml`, and *not* `cargo +nightly` for the
   reason two sections down:
   ```sh
   rustup run "$(sed -n 's/.*channel = "\(.*\)"/\1/p' rust-toolchain.toml)" \
     cargo build --release --target thumbv4t-none-eabi
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
  rustup run <pinned-channel> cargo test \
    --manifest-path <repo>/crates/flac-lite/Cargo.toml \
    --target "$(rustup run <pinned-channel> rustc -vV | sed -n 's|host: ||p')"
```

Artifacts still land in `crates/flac-lite/target/` (derived from the manifest, not
the cwd). Full write-up, including the dated A/B matrix and the `-Tgba.ld`
double-pass variant that breaks linking in standalone sub-crates, lives in
**[flac-docs/measurements.md](flac-docs/measurements.md)** → "Cargo config
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
