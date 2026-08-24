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

- `src/main.rs` — minimal ROM: renders a title screen with text and plays a
  440 Hz (A4) square wave on PSG channel 1 on boot, looping indefinitely.
- `assets/fonts/` — a small pixel font used for the title screen.
- `Makefile` — `make build`, `make rom`, `make clean`, `make podman-rom`.
- `Dockerfile` — the containerized build (Debian 12 / `rust:slim-bookworm`,
  nightly + `rust-src`, `agb-gbafix`).

### Build status (as of 2026-08-24)

**The build is verified working two ways:**

1. **Native macOS build** — `cargo +nightly build --release --target
   thumbv4t-none-eabi` compiles cleanly on this host (see "Native build" below).
   This is the path to use for local development; it only *compiles*, it does
   not fix or run the ROM.
2. **Containerized Linux build** — `make podman-rom` (podman by default, docker
   also works) successfully:
   1. builds the `gba-builder` image (`rust:slim-bookworm` + nightly + `rust-src`
      + `git` + `make` + `agb-gbafix`),
   2. compiles the ROM (`#![no_std]` + `-Zbuild-std` for `thumbv4t-none-eabi`),
   3. links it with `rust-lld` (via the `agb`-supplied `gba.ld` linker script),
   4. fixes it into a loadable ROM, and
   5. outputs `gba-sound-player.gba` to the host directory.

The containerized path is the one that produces the runnable `.gba`. The native
path stops at the linked binary (it has no `agb-gbafix` / no emulator here).

### Architecture: agb framework + raw PSG MMIO

We commit to **agb as the framework** (`#[agb::entry]`, agb display, agb
DMA/timing). agb 0.25 has **no PSG (channel 1–4) API** — its `mixer` only
drives the PCM channels (5–6) via DMA. So the 440 Hz square wave is produced by
driving the GBA's `SOUND1` registers **directly via raw MMIO** in the `psg`
module of `src/main.rs`.

This is safe and is *not* the "mixed-frameworks" trap: agb never touches PSG
channels 1–4, so raw writes to `SOUND1` do not fight any agb subsystem. agb owns
display/DMA/timing; we own the PSG registers. Do **not** start writing raw MMIO
for display or PCM channels — that would be the conflict.

### ⚠️ Runtime status: unverified on hardware/emulator

**The ROM compiles cleanly, but has not yet been run in an emulator on this
host** (no mGBA here). The previous "black screen + silent" report is expected
to be resolved by the fix above — the tone and title screen now go through the
correct paths. **Verify in mGBA on a host that has it** before treating the
runtime as confirmed.

## Building and Development

To ensure a consistent toolchain across Linux and macOS hosts, this project uses
a containerized development environment based on Debian 12. The default runtime
is **podman** (daemonless, rootless — ideal on a minimal Linux host), but any
OCI-compatible runtime (e.g. docker) works.

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

1. **Native build (local development — the default for this host)**
   ```sh
   cargo +nightly build --release --target thumbv4t-none-eabi
   ```
   This compiles and links the ROM for `thumbv4t` (via `-Zbuild-std`). It is
   the fast iteration loop. **It only compiles — there is no emulator and no
   `agb-gbafix` on this host, so do not try to run the ROM here.** To produce
   the runnable `.gba`, use the containerized path (below) on a host that has it.

   On macOS you may need the toolchain's `lib` on the dynamic loader path so
   `rust-lld` finds `libLLVM.dylib`:
   ```sh
   export DYLD_FALLBACK_LIBRARY_PATH="$HOME/.rustup/toolchains/nightly-<arch>/lib"
   ```

2. **Container runtime (CLI) — produces the runnable `.gba`**
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

## References

- <https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html> — `thumbv4t-none-eabi` is Tier 3, `#![no_std]` (`core` + `alloc` only, no prebuilt std)
- <https://gbadev.net/> — GBA hardware reference
- <https://github.com/agbrs/agb> — the `agb` Rust library this project builds on
- <https://jsgroth.dev/blog/posts/gba-audio/> — detailed GBA audio hardware
  analysis (mixing, DMA, timers)
- <https://www.littlesounddj.com/lsd/index.php> — LSDJ, the reference
  GBA tracker/instrument this project is loosely inspired by
