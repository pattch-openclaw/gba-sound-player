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
  ~731 Hz square wave on PSG channel 1 for ~2 seconds on boot.
- `assets/fonts/` — a small pixel font used for the title screen.
- `Makefile` — `make build`, `make rom`, `make clean`.

## Building and Development

To ensure a consistent toolchain across Linux and macOS hosts, this project uses
a containerized development environment based on Debian 12. The default runtime
is **podman** (daemonless, rootless — ideal on a minimal Linux host), but any
OCI-compatible runtime (e.g. docker) works.

### Requirement: `#![no_std]`, and *no* std target for `thumbv4t-none-eabi`

The GBA target `thumbv4t-none-eabi` is a **Tier-3, `#![no_std]`** bare-metal
target. Per the [Rust platform-support docs](https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html),
its library support is **`core` and `alloc` only** — it ships **no `std`**, and
rustup has **no prebuilt std artifacts** for it on *any* toolchain (stable or
nightly). Consequently:

- The crate is written `#![no_std]` (see `src/main.rs`) and relies on `core`/
  `alloc` via `agb`. This is the supported model, not a workaround.
- **Do not** run `rustup target add thumbv4t-none-eabi` or try to install a std
  target for it — that step fails by design. We build `no_std` directly for the
target instead (the target spec itself is built into rustc; no target install
is needed).
- This is a toolchain/platform reality, independent of host OS. If it looks like
a "missing dependency" or "add nightly" issue, it isn't — the target simply has
no std.

You have two main ways to build and develop:

1. **Using a container runtime (CLI)**
   The default runtime is **podman**; override with docker if you prefer.
   Run:
   ```sh
   make podman-rom
   ```
   This builds the image, mounts your local directory into the container,
   compiles the ROM (`no_std`, target `thumbv4t-none-eabi`), and outputs
   `gba-sound-player.gba` directly to your host directory. Prefer docker?
   `make podman-rom CONTAINER=docker`.

2. **Using VS Code Dev Containers (IDE)**
   The repository includes a `.devcontainer` configuration. Open this folder in
   VS Code and click "Reopen in Container" to run your IDE and `rust-analyzer`
   inside the Debian environment with full GBA target autocomplete.

*(If you prefer to build natively on your host — a side quest for us — install
the nightly Rust toolchain and `agb-gbafix`. You do **not** need to add a std
target for `thumbv4t-none-eabi`; the `no_std` build works directly. `make rom`
will still work then.)*

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
