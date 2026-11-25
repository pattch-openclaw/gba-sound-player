# gba-sound-player — agent instructions

This is a GBA audio project (FLAC product path + the ULI2 codec). The build is
native and root-free; containers are not needed and are unavailable in this
harness. The detailed build/test guide is
[`.agents/skills/gba-build-and-test/SKILL.md`](.agents/skills/gba-build-and-test/SKILL.md).

## Start here, every session

```sh
. .agents/env.sh          # PATH: pinned Rust, ffmpeg, flac 1.5.0, venv python3
make toolchain-check
```

The harness shell does not persist `PATH` between commands, so source
`.agents/env.sh` in every fresh shell (or prefix commands with it). It exports
`GBA_AUDIO` when the sibling oracle exists and `ULI_ALBUM_PY`.

## Reproduce the toolchain from scratch

```sh
.agents/skills/gba-build-and-test/scripts/setup-native-toolchain.sh
. .agents/env.sh
make toolchain-check
make test
```

The script is idempotent and installs everything under `$HOME` without sudo.
It was verified on Ubuntu 24.04 arm64 on 2026-11-25.

## What is installed, and where

| tool | version / pin | path |
|---|---|---|
| Rust | `nightly-2026-09-04` = `rustc 1.100.0-nightly (a69a63265 2026-09-03)`; `rust-src` + `rustfmt` | `~/.cargo/bin` (rustup) |
| `agb-gbafix` | **0.25.0** (must match the `agb` pin; it rewrites ROM bytes) | `~/.cargo/bin/agb-gbafix` |
| `ffmpeg` / `ffprobe` | **7.0.2** static | `~/.local/bin` → `~/.local/opt/ffmpeg` |
| `flac` / `metaflac` | **1.5.0** (must be 1.5.0 — see gotchas) | `~/.local/bin` → `~/.local/opt/flac-1.5.0` |
| `mgba-test-runner` | agb **v0.25.0**, aarch64 patches | `~/.local/bin` → `~/.local/mgba-runner/bin` |
| Python (album/prep) | 3.12 venv: `numpy 2.5.5`, `scipy 1.18.1`, `soundfile 0.14.0`, `fonttools 4.68.0`, `pillow 13.0.0` | `~/.local/venvs/gba-audio` |
| rootless C headers/libs | Ubuntu noble arm64 debs: zlib, libelf, libpng, libzstd, libclang-18, libllvm18 | `~/.local/rootless` |
| oracle checkout | `gba-audio` (default `../gba-audio`, here `/home/pi/gba-audio`) | `$GBA_AUDIO` |

Rust is invoked through `rustup run` by the Makefile, which reads the pinned
channel out of `rust-toolchain.toml`. Never use a bare `cargo +nightly`; never
`rustup target add thumbv4t-none-eabi` (Tier-3 has no prebuilt core — the build
uses `-Zbuild-std` with `rust-src`).

## Verified gates (2026-11-25, this host)

Run with `. .agents/env.sh`:

| command | result |
|---|---|
| `make toolchain-check` | pass |
| `make test-rom` | build + link + boot in mGBA, 0 tests collected (expected) |
| `make check` | all workspaces rustfmt-clean |
| `make uli-check` | pass |
| `make uli-test` | uli-lite corpus 6 tests; uli-enc **17 suites / 94 tests** |
| `make uli-overlay-test` | pass (product + `verify` builds) |
| `make uli-probe` | 2,400 `band_rdo` cases byte-for-byte; MDCT golden matches |
| `make uli-frame-diff` | **exits 2 by design** while it has findings (PR #192/#193 work) |
| `make uli-plan-diff A=… B=…` | exit 0 identical / 2 different, as documented |
| `make uli-bench` | ROM built and ran headless: `[uli-bench] PASS` |
| `make flac-check` | pass (gates + overlay + product + playback + spike) |
| `make gafp-pack-check` | 4 fixtures byte-identical, 9 host facts |
| `make album-bake` then `make album-pack-check` | bake writes assets; gate passes 54 tests |
| `make uli-album-asset ULI_ALBUM_PY="$ULI_ALBUM_PY"` | available (runs `prep.py` + the C oracle encoder; not run here) |

`make uli-accept` (the S7 matrix) and the album-level ULI2 numbers still need
the generated corpus (`make uli-album-asset`) plus the reference encoder; the
encoder is built at `$GBA_AUDIO/build/uli_enc`, the corpus is not generated here.

## Gotchas found the hard way

1. **flac/metaflac must be 1.5.0.** Ubuntu 24.04 ships 1.4.3, which regenerates
   `crates/flac-lite/tests/gfp/p2_fixed0_stereo_16384_b2048.gfp` as different
   bytes (13,577 vs 13,898). `make gafp-pack-check` then fails inside
   `gen_gafp_fixtures.py` (which also has a pre-existing formatting bug that
   turns the mismatch into a `TypeError` instead of a message). The committed
   fixtures are libFLAC 1.5.0 bytes; do not "fix" them by regenerating with 1.4.3.
2. **`python3` must be the venv for album work.** `make album-bake` calls plain
   `python3`, and `album_glyphs.py` needs `fontTools`. `.agents/env.sh` puts the
   venv first. `gba-audio/encoder/prep.py` needs `numpy` (and ffmpeg).
3. **`make album-pack-check` on a fresh clone needs `make album-bake` first.**
   The crate has an unconditional `pub mod glyphs;`, but `album-pack-check` runs
   only `album_bake.py` (which writes `assets.rs` and `layout.rs`), not
   `album_glyphs.py` (which writes `glyphs.rs`). `album-glyphs-test` depends on
   `album-bake`; the pack gate does not. Bake once, then the pack gate compiles.
4. **Host gates run from `/tmp/gba-sound-player-host-gate`.** The root
   `.cargo/config.toml` sets the GBA target and `build-std`; cargo merges config
   upward, so host tests must run with `--target <host triple>` from outside the
   repo. The Makefile already does this — copy the shape for new gates.
5. **mGBA runner must match the `agb` pin.** `agb = "0.25.0"` → `AGB_REF=v0.25.0`;
   agb v0.25.0 needs the three aarch64 patches in the setup script (same as the
   Dockerfile). A mismatched runner is a confusing boot failure, not a build error.
6. **The ULI2 S8 instruments are off the gate path on purpose.** `uli-probe`,
   `uli-frame-diff`, and `uli-accept` compile/run the off-tree oracle; E-R1
   forbids the dependency, not the oracle. `uli-frame-diff` exits 2 while it has
   findings; that is not a broken harness.
7. **`make clean` is shallow.** It removes ROMs/assets/logs but keeps every
   workspace's `target/`. `make clean-deep` drops ~4.5 GB of caches and makes the
   next full build ~135 s (cold) instead of ~22 s (warm). Don't deep-clean to
   "make sure".

## Where to look

- Build/test guide: `.agents/skills/gba-build-and-test/SKILL.md`
- Product codec decision, gates, risks: `ULI2.md`
- The live plan, S8 sub-steps, divergence inventory: `crates/uli-lite/PLAN-OF-RECORD.md`
- Measurement ledger: `crates/uli-lite/RECORD.md`
- FLAC path: `FLAC.md`, `flac-docs/`
- Album player: `ALBUM-PLAYER.md`
- Encoder crate docs: `crates/uli-enc/README.md`
