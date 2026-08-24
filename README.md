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
  ~731 Hz square wave on PSG channel 1 on boot.
- `assets/fonts/` — a small pixel font used for the title screen.
- `Makefile` — `make build`, `make rom`, `make clean`, `make podman-rom`.
- `Dockerfile` — the containerized build (Debian 12 / `rust:slim-bookworm`,
  nightly + `rust-src`, `agb-gbafix`).

### Build status (as of 2026-08-24)

**The build is verified working** on Linux using containerization with `podman`
(the default runtime; docker also works). `make podman-rom` successfully:

1. builds the `gba-builder` image (`rust:slim-bookworm` + nightly + `rust-src`
   + `git` + `make` + `agb-gbafix`),
2. compiles the ROM (`#![no_std]` + `-Zbuild-std` for `thumbv4t-none-eabi`),
3. links it with `rust-lld` (via the `agb`-supplied `gba.ld` linker script),
4. fixes it into a loadable ROM, and
5. outputs `gba-sound-player.gba` to the host directory.

This is the current known-good build path.

### ⚠️ Runtime status: ROM loads, but no video or audio

**The ROM builds and loads in an emulator without crashing, but it currently
produces no visible output (black screen) and no audible output (silent).**
This is a *runtime* issue, not a build issue — the compiler, linker, and ROM
fixing steps all succeed.

**Suspected root cause (needs investigation):** `src/main.rs` mixes two
incompatible GBA frameworks. It uses `agb`'s `#[agb::entry]` (which means agb
owns the hardware — its own sound system, its own display/DMA, its own timing),
*but* it also writes to the GBA's sound and display registers **directly via
raw MMIO pointers** (the `psg` module, and `gfx`/`bg`/`frame` calls), bypassing
agb's abstraction. agb's sound/display subsystems are not the ones driving the
hardware when raw registers are written underneath them, so the tone and the
title screen never actually reach the GBA. This is an architecture mismatch
(HAL + raw MMIO) and is the most likely explanation for the silent/black
ROM.

Next steps (not yet done): pick one framework and commit to it — either
(a) use agb's `sound::` and `display::` APIs exclusively (drop the raw `psg`
module), or (b) drop agb and drive the hardware purely via raw MMIO (a
gba-rs-style `no_std` crate). Do not mix the two.

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

1. **Using a container runtime (CLI)**
   The default runtime is **podman**; override with docker if you prefer.
   Run:
   ```sh
   make podman-rom
   ```
   This builds the image, mounts your local directory into the container,
   compiles the ROM (`#![no_std]` + `-Zbuild-std`, target `thumbv4t-none-eabi`),
   and outputs `gba-sound-player.gba` directly to your host directory. Prefer
   docker? `make podman-rom CONTAINER=docker`.

2. **Using VS Code Dev Containers (IDE)**
   The repository includes a `.devcontainer` configuration. Open this folder in
   VS Code and click "Reopen in Container" to run your IDE and `rust-analyzer`
   inside the Debian environment with full GBA target autocomplete.

*(If you prefer to build natively on your host — a side quest for us — install
the nightly Rust toolchain, the `rust-src` component, and `agb-gbafix`. You do
**not** need to add a std target for `thumbv4t-none-eabi`; `.cargo/config.toml`
enables `-Zbuild-std`, so `cargo build` compiles `core`/`alloc` for the target
automatically. `make rom` will still work then.)*

Run `gba-sound-player.gba` in any GBA emulator (mGBA recommended) to see the title
screen and hear the tone.

## References

- <https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html> — `thumbv4t-none-eabi` is Tier 3, `#![no_std]` (`core` + `alloc` only, no prebuilt std)
- <https://gbadev.net/> — GBA hardware reference
- <https://github.com/agbrs/agb> — the `agb` Rust library this project builds on
- <https://jsgroth.dev/blog/posts/gba-audio/> — detailed GBA audio hardware
  analysis (mixing, DMA, timers)
- <https://www.littlesounddj.com/lsd/index.php> — LSDJ, the reference
  GBA tracker/instrument this project is loosely inspired by
