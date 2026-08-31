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

4. **Compressed audio (FLAC).** Support FLAC decoding in order to make use of compressed audio files, allowing us to fit more songs (or a full album) onto the cartridge. Status and library evaluation live in **[FLAC.md](FLAC.md)**.

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
- `examples/symphonia_flac_probe/` — standalone FLAC compile probe (`symphonia-bundle-flac` from the keks no-std fork, with its own `Cargo.toml`). **Expected to fail compilation**; the failure analysis is documented in **[FLAC.md](FLAC.md)**.
- `crates/flac-lite/` — our own `no_std`, zero-dependency, zero-allocation FLAC frame decoder (**scaffold only**: real signatures, `todo!()` bodies). Decision + design in **[FLAC.md](FLAC.md)**; byte-level `GAFP` contract in [`crates/flac-lite/README.md`](crates/flac-lite/README.md).
- `examples/flac_integration/` — **EXPERIMENTAL** standalone ROM crate that links `flac-lite` into a bootable GBA ROM. This is the ongoing build sanity check that the decoder keeps fitting the target end to end (see `make native-flac-rom` below). It builds, links, and boots; it does not play audio yet.
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
   - **macOS Native Test Setup:** To run tests locally, you need the nightly toolchain, a C++ compiler for the emulator bindings, and the emulator itself:
     ```sh
     rustup toolchain install nightly
     rustup component add rust-src --toolchain nightly
     brew install cmake mgba
     cargo install agb-gbafix
     cargo install mgba-test-runner --git https://github.com/agbrs/agb.git
     ```
   - **Running Tests:**
     ```sh
     CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER=mgba-test-runner cargo +nightly test --target thumbv4t-none-eabi
     ```

1. **Native build** — `make native-rom` compiles, links, and fixes the ROM using
   the host toolchain (nightly + `rust-src` + `agb-gbafix` installed locally).
   Produces the same runnable `gba-sound-player.gba` as the containerized path.
2. **Containerized build** — `make podman-rom` (podman by default, docker
   also works) successfully:
   1. builds the `gba-builder` image (`rust:slim-bookworm` + nightly + `rust-src`
      + `rustfmt` + `git` + `make` + `agb-gbafix`),
   2. compiles the ROM (`#![no_std]` + `-Zbuild-std` for `thumbv4t-none-eabi`),
   3. links it with `rust-lld` (via the `agb`-supplied `gba.ld` linker script),
   4. fixes it into a loadable ROM, and
   5. outputs `gba-sound-player.gba` to the host directory.

Both paths produce the same runnable `.gba`. The container is the **standardized**,
reproducible path (identical toolchain everywhere, no host setup needed) and the
only one that works on a host without virtualization-independent tooling; the
native path is the fast local iteration loop. Emulation/testing still happens on
hosts that have mGBA.

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
| `make flac-test` | `flac-lite` **in isolation**: GBA target compile gate + host unit tests. No agb, no ROM, no emulator | either |
| `make test-rom` | ROM `#[test_case]` suite, headless in mGBA via `mgba-test-runner` | needs mGBA |
| `make test` | Top-level: `test-rom` + `flac-test`. Verify-only — never reformats your tree | either |
| `make build` / `rom` / `flac-rom` | Lower-level primitives (no fixing / container); `rom` is what the container runs. All run `format` first; **none gate on tests** | — |
| `make format` | `cargo fmt` (writes) across all three workspaces — the automatic build pre-step | either |
| `make clean` / `check` / `help` | Clean all workspaces incl. sub-crates; `cargo fmt --check` (**verify-only**, never writes); target list | either |

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
| `make test` / `test-rom` / `flac-test` | never | no | ✅ |
| `make check` | never | ✅ | no |

If a build reformats your tree, run `make check` before committing to confirm
the result is gate-green. If `rustfmt` is missing for the pinned toolchain,
`format` prints a notice and the build continues — formatting is a convenience,
never a hard dependency of building.

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
> It does **not** decode audio yet — `flac-lite` is scaffold (`todo!()` bodies),
> and the ROM references the decoder via a `#[used]` link anchor that is never
> called, so the image exercises the full build path without ever hitting a
> `todo!()` panic on hardware. Replace the anchor with a real decode loop once
> decoding lands (see [FLAC.md](FLAC.md) → next steps).

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
# rustup present   -> `rustup run nightly cargo …` (immune to a shadowed shim)
# no rustup at all -> plain `cargo`/`rustc`, which must already be nightly
```

Overrides: `TOOLCHAIN=`, `RUSTUP=`, `CARGO=`, `RUSTC=`. `make help` prints the
resolved toolchain and host triple; `toolchain-check` (a prerequisite of the
test gates) fails fast with install instructions rather than half-running a
gate. Prerequisites for a native setup:

```sh
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly    # -Zbuild-std
rustup component add rustfmt  --toolchain nightly    # make check + build's format pre-step
cargo install agb-gbafix                             # ROM fixing
cargo install mgba-test-runner --git https://github.com/agbrs/agb.git  # make test-rom
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
clear the parent's. The `flac-test` target encodes the fix:

```sh
cargo +nightly test -Zbuild-std= --target "$(rustc -vV | sed -n 's|host: ||p')"
```

Both overrides are load-bearing. Full write-up (including the `-Tgba.ld`
double-pass variant that breaks linking in standalone sub-crates) lives in
**[FLAC.md](FLAC.md)** → "Cargo config leak".

## References

- <https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html> — `thumbv4t-none-eabi` is Tier 3, `#![no_std]` (`core` + `alloc` only, no prebuilt std)
- <https://gbadev.net/> — GBA hardware reference
- <https://gbadev.net/gbadoc/audio/registers.html> — GBA audio hardware registers reference
- <https://gbadev.net/gbadoc/audio/sound1.html> — GBA audio Sound Channel 1 reference
- <https://github.com/agbrs/agb> — the `agb` Rust library this project builds on
- <https://jsgroth.dev/blog/posts/gba-audio/> — detailed GBA audio hardware
  analysis (mixing, DMA, timers)
- <https://www.littlesounddj.com/lsd/index.php> — LSDJ, the reference
  GBA tracker/instrument this project is loosely inspired by
