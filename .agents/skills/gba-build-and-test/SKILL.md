---
name: gba-build-and-test
description: >-
  How to build and test the gba-sound-player GBA ROMs natively: which Make targets to
  run for thumbv4t-none-eabi (native-rom; the FLAC product path master-pack/master-play-rom,
  flac-rom, product-play-rom, spike-rom, overlay-kernel-rom; the flac-check / flac-gates /
  overlay-gates / product-play-test gate set; test-rom), what has to be installed (Rust
  nightly + rust-src + -Zbuild-std, agb-gbafix, mgba-test-runner, flac/metaflac, ffmpeg for
  master encoding), and the measured gotchas of this repo — the Tier-3 target has no
  prebuilt core, cargo's config inheritance breaks host tests, agb v0.25.0 needs aarch64
  patches, the root ROM #[test_case] suite collects zero tests, a playback ROM's PASS exits
  124, and the historical FLAC instruments (6a-3/6a-5/6a-7 arms, the 10,512 Hz rate flow,
  arm-kernels, e5_fetch) no longer have Make targets. Use when building any .gba in this
  repo, running or extending its test gates, touching crates/flac-lite or the Makefile, or
  deciding whether to use the podman-* container targets here.
---

# GBA build and test (native)

Everything in this repo has a **native** build and test path, and it is the path that
works in this agent VM. The `podman-*` twins in the Makefile are the project's portable
standard for *other* hosts; they need a container runtime, which this harness does not
have (see the `no-nested-virtualization` skill). Native here is complete, not a fallback:
every target and gate below has been run green on this host.

## Do this

```sh
# 0. one-time toolchain setup (no root needed) — idempotent
.agents/skills/gba-build-and-test/scripts/setup-native-toolchain.sh

# 1. every ROM the tree builds, natively (built + linked + fixed)
make native-rom native-flac-rom native-product-play-rom native-master-play-rom \
     native-spike-rom native-overlay-kernel-rom native-opus-rom native-uli-spike-rom
#    INPUT=track.wav is required for master-* ; both are verified green on this host.

# 2. every gate
make flac-check    # the whole FLAC surface in one command: flac-gates (every flac-lite
                   # configuration, both gafp legs) + overlay-gates (the ARM kernel's
                   # compile, host contract, objdump fact and mGBA device suite) +
                   # product-play-test + spike-test. Needs `flac` + `metaflac` on PATH.
make test          # test-rom (the root #[test_case] suite, which collects 0 cases)
                   # + flac-test + uli-test
make opus-test     # the Opus track's host gate (OPUS.md, not FLAC)

# 3. the formatting gate, after any build (builds run `cargo fmt` as a pre-step)
make check
```

`native-<target>` and `podman-<target>` are **pattern rules** over the bare target names —
`make native-flac-rom`, `make podman-flac-rom`, `make podman-overlay-gates` all work, and no
`podman-*` target has ever been run here. Archived instruments no longer have targets: the
spike's 6a-3/6a-5/6a-5b/6a-7 arms, the 10,512 Hz rate flow, `examples/arm-kernels/` and
`examples/e5_fetch/`; `flac-docs/history.md` §"Retired Make entrypoints" gives the cargo
command for each. Do **not** run `make clean` to "make sure": it drops ~5 GB of warm target
dirs (every Tier-3 build re-compiles `core`/`alloc` from source) and deletes the generated
master asset/pin.

Builds never gate on tests; tests never reformat; only `check` fails on formatting. So a
build that reformatted your tree is fine — run `make check` before committing.

## Prerequisites (all root-free, verified)

| Needed for | Install |
|---|---|
| Any `make` invocation | **GNU make ≥ 4.2** — the Makefile reads the pin out of `rust-toolchain.toml` with make's own file function (`$(file <)`), which 3.81 lacks. Linux is fine; on macOS `brew install make` and call `gmake`. The Makefile checks its own version and says so. |
| Any target build | the **pinned** channel in `rust-toolchain.toml` (`nightly-2026-09-04`) + `rustup component add rust-src rustfmt --toolchain nightly-2026-09-04`. rustup installs the pinned channel on the first build; the explicit commands are only for pre-staging. |
| `*-rom` (ROM fixing) | `cargo install agb-gbafix --version 0.25.0 --locked` (pinned: it rewrites the ROM header, so it is part of the ROM bytes records compare by sha) |
| `make test-rom`, `overlay-kernel-test`, and any `product-play`/`master-play` ROM run | `mgba-test-runner` — run the setup script above (it builds agb's test runner; it needs `cmake`, `pkg-config`, zlib, libelf headers and libclang) |
| Host gates | nothing beyond the pinned channel + `rust-src`: they run on the **host** triple, not the GBA target |
| `make flac-gates`'s `gafp-pack-check` leg, and anything that runs `scripts/gafp_pack.py` / `gen_gafp_fixtures.py` | **`flac` + `metaflac` on PATH** (libFLAC tools). Without them the leg fails at the fixture step, not in the decoder, and the failure is not a decode regression — check this before bisecting a red `flac-gates`. `gafp-check` alone (byte-comparison of the committed blob) needs only Python. |

`make help` prints the resolved toolchain; `toolchain-check` fails fast with install
instructions rather than half-running a gate.

## Gotchas this repo has actually bitten agents with

1. **Never `rustup target add thumbv4t-none-eabi`.** It is a Tier-3 `no_std` target with
   no prebuilt `core`/`alloc` on any toolchain. The build compiles `core`/`alloc` from
   source via `-Zbuild-std`, which needs nightly + `rust-src`; `.cargo/config.toml`
   already sets it. `E0463: can't find crate for core` means those, not a missing crate.
2. **Invoke the toolchain through rustup** (`rustup run <channel> cargo …`, which the
   Makefile resolves for you — it reads the pinned channel out of `rust-toolchain.toml`)
   — never `cargo +nightly`: a non-shim cargo (conda, distro) reads `+nightly` as a
   subcommand. Note the asymmetry: `rustup run <channel>` *overrides*
   `rust-toolchain.toml`, which is why the Makefile must not hard-code a channel —
   it reads it, with make's own file function and no text pipeline. `make
   toolchain-check` then asks rustup what it resolves *in this tree* and fails if
   that is not the instance the build runs: a commented-out `channel` line or a
   `RUSTUP_TOOLCHAIN` in the environment leaves rustup on the machine default with
   exit 0 while `make` still builds the pinned string, i.e. two instruments in one
   tree. Don't comment the line out to unpin — delete the file.
3. **The channel is the instrument.** `nightly` unpinned moves under `rustup update`;
   FLAC.md compares ROM shas and cycle counts across sessions, so the channel is pinned
   to a dated one. Changing `rust-toolchain.toml` is a measured change: carry a control
   build (ROM/ELF bytes + a section and symbol dump) and name the rustc instance in any
   record.
4. **Cargo config leak:** the root `.cargo/config.toml` sets `target = thumbv4t-none-eabi`
   and `build-std`, and cargo merges config by walking *up* the tree — a `[workspace]`
   boundary does not stop it. Host test gates therefore run from a **gate-neutral cwd**
   (`/tmp/gba-sound-player-host-gate`) with `--manifest-path … --target <host triple>`;
   any in-repo directory still inherits the GBA config and fails with `E0463` /
   `E0152: duplicate lang item`. Copy that shape into any new gate.
5. **Non-default configurations are only witnessed by their own gates.** Anything
   touching `crates/flac-lite` needs `make flac-gates` (`crc-check`, `scan`,
   `rice-hybrid`, their combinations, and the `thumbv7em-none-eabi` portability
   witness), not just `make test`. `flac-test`'s count deliberately excludes the
   feature-gated suites. Same rule for the overlay: `make overlay-gates` is the kernel's
   witness set, and `make flac-gates` does not and must not include it (different crate,
   whose features do not exist in the core).
6. **`mgba-test-runner` must match the `agb` pin** (`Cargo.toml`: `agb = "0.25.0"` →
   `AGB_REF=v0.25.0`), and on **aarch64 Linux** agb v0.25.0 does not compile as-is: it
   assumes `c_char == i8` and an x86 `__va_list_tag`. The three `sed` patches in the
   setup script are the same ones the project Dockerfile applies, and `cargo install
   --git` cannot carry them (`--git` cannot be combined with `--path` either).
7. **A green `make test-rom` today does not mean assertions ran.** The root crate's agb
   suite currently collects nothing: the run ends with `Running 0 tests` and exits 0. It
   proves build → link → boot-in-mGBA works. The only `#[test_case]` in the repo lives in
   `examples/tone_playback.rs`, which is not part of that test binary.
8. **No emulator or display here.** A `.gba` is validated by the compile/link/fix path and
   the test harness; screen and sound behaviour (the orange backdrop, the 440 Hz tone)
   need a human with mGBA. Never claim audio/video correctness from a green build.
9. The root `.cargo/config.toml` `runner` is an `echo` placeholder, so a bare
   `cargo test --target thumbv4t-none-eabi` "passes" without running anything. Always go
   through `make test-rom`, which sets `CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER`.

## What green looks like (re-measured 2026-11-14 on this host, after the FLAC Makefile simplification)

- Toolchain: `nightly-2026-09-04` = `rustc 1.100.0-nightly (a69a63265 2026-09-03)` +
  `rust-src`, `agb-gbafix` 0.25.0, `mgba-test-runner` from agb v0.25.0, GNU make 4.4.1,
  `flac`/`metaflac` and `ffmpeg` on PATH.
- ROMs built natively, all gitignored, `git status` clean afterwards:
  `gba-sound-player.gba` 152,984 B · `flac-integration.gba` 644,340 B ·
  `product-play.gba` 662,440 B · `master-play.gba` 649,000 B (from a 10 s clip) ·
  `flac-spike.gba` 1,042,156 B · `overlay-kernel-check.gba` 221,020 B.
  Sections at the current tree: instrument `.text` 50,132 / `.iwram` 812;
  product images `.text` 68,528–81,176 / `.iwram` 2,036 (= 812 core + 544 tables +
  680 asm body) / `.bss` 42,608 for the ring. Sizes are host- and tree-specific:
  never quote one across machines, and a comment edit alone moves a ROM hash.
- Gates, all green in one run: `flac-gates` **1,526 passed / 0 failed / 3 ignored across
  75 test binaries** (plus 9 empty `Doc-tests` lines that are not among the 75 — count the
  aggregate and the arms, not `test result:` lines); `overlay-gates` green through the
  mGBA device suite; `product-play-test` green; `spike-test` green; `make check` clean.
  The prior record at `main` (2026-11-11) was 1,498 / 3 across 72 — counts move with the
  tree and are recorded per landing, which is why no doc here treats one as a threshold.
- The happy path works end to end here: `make master-pack INPUT=…` packs (80 frames,
  162,304 samples, predictor census printed), and `master-play.gba` runs headless to
  `verdict PASSED — screen BLUE`, `und 0`, `late 0`, every period serviced, PCM and FIFO
  pins matched — **with exit 124, because a pass loops forever and the timeout kills it.**
  That is the mGBA control, not a playback claim: nothing here has been heard.
- Timing honesty: with warm target dirs a ROM rebuild is sub-second and the whole gate set
  is a couple of minutes; the first build after `make clean` pays the `-Zbuild-std`
  `core`/`alloc` compile per target per workspace. Quote warm numbers only, and say so.

Detail and the install failure modes: `references/native-toolchain-setup.md`.

## Report

Targets run + exit codes, test counts per gate, artifact list with sizes, whether the
tree stayed clean, and — explicitly — that the container path was skipped and why, plus
anything not verifiable natively (real-hardware audio/video).
