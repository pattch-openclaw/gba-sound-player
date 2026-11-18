.PHONY: format build rom flac-rom product-play-rom master-pack master-play-rom \
        spike-rom spike-test playback-test \
        overlay-check overlay-test overlay-check-arm overlay-test-arm \
        overlay-objdump overlay-kernel-rom overlay-kernel-test overlay-gates \
        test test-rom flac-test flac-test-wide flac-test-crc flac-test-scan \
        flac-test-rice flac-gates flac-portable-check gafp-check gafp-pack-check \
        product-play-test opus-test opus-rom uli-test uli-check uli-fixture \
        uli-bench-rom uli-bench uli-attrib uli-attrib-subframe uli-frame-ops \
        uli-album-asset uli-album-bench uli-album-bench-rom uli-gate uli-gate-rom uli-gate-fixture uli-gate-fixture-rom uli-play-rom \
        uli-overlay-test uli-kernel-twin \
        album-probe-art album-probe-test album-probe-rom album-probe \
        album-bake album-pack-check album-seam-test album-rom album-player-rom album-test \
        uli-spike-rom clean clean-cache check help toolchain-check

# ===========================================================================
# Build settings
# ===========================================================================
#
# nightly is REQUIRED: `-Zbuild-std` (in .cargo/config.toml) is nightly-only and
# is how `core`/`alloc` get compiled for this Tier-3 target. Never run
# `rustup target add thumbv4t-none-eabi` — no toolchain ships prebuilt core for
# it. See README.md and flac-docs/measurements.md ("Cargo config inheritance").
#
# The channel is pinned in ONE place, rust-toolchain.toml, and read here — it is
# not duplicated as a string. Why the file alone is not enough: `rustup run
# <name>` SELECTS a toolchain and therefore OVERRIDES the pin file, so a channel
# that lived only in the file would not pin `make` at all. The read uses make's
# own file function (the Dockerfile evaluates this same expression, so host and
# image share one *rule* for reading it, not merely one string), and three layers
# close the gaps:
#   - a GNU make version guard, because `$(file <)` needs make 4.2 and Apple
#     still ships 3.81 at /usr/bin/make;
#   - a match-count guard at parse time, because this is a word-pattern match and
#     not a TOML parse — a pin file that yields anything other than exactly one
#     quoted word stops the build rather than silently falling back;
#   - `toolchain-check`, which asks rustup what it resolves *in this tree* and
#     compares it to the instance this build runs. That is the gap a word match
#     cannot see: rustup ignores a commented-out `channel` line and falls back to
#     the ambient default silently, exit 0.
# The `nightly` fallback below is for a MISSING pin file only (partial checkout).
#
# The instance is the instrument: flac-docs/measurements.md compares ROM shas and
# cycle counts across sessions, so changing rust-toolchain.toml is a measured change
# that owes a control build ("The toolchain pin", and history.md's landing record).
# Try a different instance for one command with `make <target>
# TOOLCHAIN=nightly-2026-09-11` — and say so in the record.

RUSTUP ?= rustup
RUST_TOOLCHAIN_FILE := $(dir $(lastword $(MAKEFILE_LIST)))rust-toolchain.toml

ifndef MAKE_VERSION
$(error reading the pinned channel uses make's own file function, $$(file <), which needs GNU make 4.2 or newer. This make is $(MAKE) and reports no MAKE_VERSION, so it is not GNU make. On GNU/Linux the system make is fine. On macOS install GNU make from Homebrew and run gmake, or build in the container with make podman-rom.)
endif
ifneq ($(filter 3.% 4.0 4.1,$(MAKE_VERSION)),)
$(error reading the pinned channel uses make's own file function, $$(file <), which needs GNU make 4.2 or newer — this is GNU make $(MAKE_VERSION). Linux system makes are normally new enough. macOS ships 3.81 at /usr/bin/make: install GNU make from Homebrew and run gmake, or build in the container with make podman-rom.)
endif

# The channel is the one word in the pin file wrapped end-to-end in double
# quotes, so `$(filter "%", …)` selects exactly it and `$(subst)` drops the
# quotes. No sed, no pipeline, no second copy of the string that could drift.
PINNED_QUOTED := $(filter "%", $(file < $(RUST_TOOLCHAIN_FILE)))
PINNED_TOOLCHAIN := $(subst ",,$(PINNED_QUOTED))
TOOLCHAIN ?= $(if $(PINNED_TOOLCHAIN),$(PINNED_TOOLCHAIN),nightly)

ifneq ($(words $(PINNED_QUOTED)),1)
ifneq ($(wildcard $(RUST_TOOLCHAIN_FILE)),)
$(error $(RUST_TOOLCHAIN_FILE) — expected exactly one double-quoted word, the channel, and found $(words $(PINNED_QUOTED))$(if $(PINNED_QUOTED), which were $(PINNED_QUOTED),). Either the channel line is missing or unquoted, or the prose in that file contains a quoted single word. Refusing to build on a moving channel.)
endif
endif

# Invoke the toolchain through rustup when rustup is present: a *plain* cargo on
# PATH (conda, some distro/Homebrew setups) reads `+nightly` as a subcommand and
# dies, and a shadowed shim would silently build a different compiler. With no
# rustup at all, fall back to a cargo that must already BE the pinned nightly
# (override with CARGO= / RUSTC= / TOOLCHAIN=).
HAS_RUSTUP := $(shell command -v $(RUSTUP) >/dev/null 2>&1 && echo 1)
ifeq ($(HAS_RUSTUP),1)
CARGO ?= $(RUSTUP) run $(TOOLCHAIN) cargo
RUSTC ?= $(RUSTUP) run $(TOOLCHAIN) rustc
else
CARGO ?= cargo
RUSTC ?= rustc
endif

TARGET := thumbv4t-none-eabi
# Second bare-metal triple for the core's portability witness (a build fact, no
# performance claim attached). Override with `SECOND_TARGET=<triple>`.
SECOND_TARGET ?= thumbv7em-none-eabi
# Cargo's runner env var is the target triple upper-cased with `-` → `_`:
# CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER.
TARGET_ENV := $(shell echo $(TARGET) | tr 'a-z-' 'A-Z_')
GBA_TEST_RUNNER ?= mgba-test-runner

# Host triple, used to defeat the inherited `target = thumbv4t-none-eabi` when a
# host gate runs. Queried through the SAME toolchain the gate builds with, so a
# stray rustc elsewhere on PATH cannot hand us the wrong triple.
HOST_TRIPLE := $(shell $(RUSTC) -vV 2>/dev/null | sed -n 's|host: ||p')

# Where every host gate runs: a directory OUTSIDE the repo, so cargo's upward
# `.cargo/config.toml` walk finds nothing to inherit. This is not cosmetic — see
# flac-docs/measurements.md, "Cargo config inheritance". The build dir still
# derives from the manifest path, so the container cache volumes apply.
GATE_NEUTRAL_CWD ?= /tmp/gba-sound-player-host-gate

# Is rustfmt installed for the pinned toolchain? `format` degrades to a notice
# rather than failing a build when it is absent: formatting is a convenience
# pre-step to builds and is only ever *gated* by `make check`.
HAS_RUSTFMT := $(shell $(CARGO) fmt --version >/dev/null 2>&1 && echo 1)

# --- container path (the portable build for hosts without this toolchain) -----
# Every bare target runs wherever it is invoked: natively, or inside the image
# when a `podman-<target>` wrapper re-invokes it there. Nothing branches on "am I
# in a container?". NOTE: no `podman-*` target has ever been run in the harness
# that writes most of these records (rootless podman cannot start there) — the
# native path is the witnessed one.
CONTAINER ?= podman
IMAGE ?= gba-builder
# One image serves builds AND tests: it carries nightly + rust-src + rustfmt +
# agb-gbafix + mgba-test-runner, so the containerized path needs no host tooling.
# `builder` is the lighter stage (no emulator); `tester` adds mgba-test-runner.
PODMAN_STAGE ?= builder
# Named volumes for the per-workspace cargo dirs: the project bind mount shadows
# the image's /app/target, so without these every container run rebuilds
# core/alloc and every dependency from scratch. `:Z` is inert on macOS.
CARGO_CACHE   ?= gba-cargo-target
FLAC_CACHE    ?= gba-flac-lite-target
FLACROM_CACHE ?= gba-flac-integration-target
SPIKE_CACHE   ?= gba-flac-spike-target
OPUS_CACHE    ?= gba-opus-spike-target
OVERLAY_CACHE ?= gba-flac-lite-gba-target
OVERLAY_ROM_CACHE ?= gba-overlay-kernel-check-target
CACHE_MOUNTS := -v $(CARGO_CACHE):/app/target:Z \
                -v $(FLAC_CACHE):/app/crates/flac-lite/target:Z \
                -v $(OVERLAY_CACHE):/app/crates/flac-lite-gba/target:Z \
                -v $(FLACROM_CACHE):/app/examples/flac_integration/target:Z \
                -v $(SPIKE_CACHE):/app/examples/flac_spike/target:Z \
                -v $(OPUS_CACHE):/app/examples/opus_spike/target:Z \
                -v $(OVERLAY_ROM_CACHE):/app/examples/overlay-kernel-check/target:Z

# ===========================================================================
# Where things live
# ===========================================================================

ROM := gba-sound-player.gba

# --- the FLAC product path (FLAC.md) -----------------------------------------
# examples/flac_integration is the product crate: the `GAFP` asset, the manifest
# facade over flac-lite's frame walk, the overlay's ARM kernel, and two binaries
# built from one feature table, one lockfile and one set of pins —
#   flac-integration  decode + verify + time (no sound)
#   product-play      decode → convert → ring → sound DMA, and its own judge
FLAC_CRATE_DIR := examples/flac_integration
FLAC_INTEGRATION_MANIFEST := $(abspath examples/flac_integration/Cargo.toml)
FLAC_CRATE_BIN := $(FLAC_CRATE_DIR)/target/$(TARGET)/release/flac-integration
FLAC_ROM := flac-integration.gba
PRODUCT_PLAY_ROM := product-play.gba
PRODUCT_PLAY_BIN := $(FLAC_CRATE_DIR)/target/$(TARGET)/release/product-play
# The master-audio path: a source master is resampled/encoded to the product
# rate, packed into a GAFP asset, and the SAME product-play binary is rebuilt
# against that asset and its generated pin. Both generated files are build
# artifacts (gitignored), so the tracked product asset is never overwritten.
MASTER_ASSET := $(FLAC_CRATE_DIR)/assets/master.gfp
MASTER_PIN := $(FLAC_CRATE_DIR)/src/master_pin.rs
MASTER_PLAY_ROM := master-play.gba

# --- the Album Player's pack step (ALBUM-PLAYER.md §9.0, A3a) ----------------
# The master-audio path generalised to N tracks: `album.toml` is the tracked
# input, and the per-track GAFP blobs, `ALBM1` (the app's manifest) and the
# generated `src/assets.rs` are build output, gitignored the way the master asset
# already is (§2.3). Phase 1 packs; nothing plays audio yet (§11).
ALBUM_CRATE_DIR := examples/album_player
ALBUM_MANIFEST := $(abspath examples/album_player/Cargo.toml)
ALBUM ?= examples/album_player/album.toml
ALBUM_ASSETS_DIR := $(ALBUM_CRATE_DIR)/assets
ALBUM_BLOB := $(ALBUM_ASSETS_DIR)/albm1.bin
ALBUM_ASSETS_RS := $(ALBUM_CRATE_DIR)/src/assets.rs
ALBUM_GATE_LOG := album-pack-check.log
ALBUM_SEAM_GATE_LOG := album-seam-test.log
# The playback ROM (PR-3a step iii): the binary cargo writes, and the fixed image
# `agb-gbafix` makes of it. Both are build output — `*.gba` at the root is already
# cleaned by `clean`'s output pass.
ALBUM_ROM_BIN := $(ALBUM_CRATE_DIR)/target/$(TARGET)/release/album-play
ALBUM_ROM := album-player.gba
# The unaudited artifact: the same bin built without the `deep-audit` feature. It plays the album but
# does not verify decoded content, which is what takes the ~75 s of CPU out of a run a human sits in
# front of. `*.gba` in `clean-outputs` covers this name — checked, not assumed.
ALBUM_FAST_ROM := album-player-fast.gba
ALBUM_ROM_RUN_LOG := album-rom-run.log
# AP-1's gate arm (PR-3b-iii-B): the same audited bin with `input-replay`'s compiled-in key script,
# because a headless run cannot press a key. Its own image and log, so `album-rom`'s stay untouched.
ALBUM_REPLAY_ROM := album-player-replay.gba
ALBUM_TEST_LOG := album-test.log

# --- the core and its overlay -------------------------------------------------
FLAC_CRATE_DIR_MANIFEST := $(abspath crates/flac-lite/Cargo.toml)
OVERLAY_CRATE_DIR := crates/flac-lite-gba
OVERLAY_CRATE_MANIFEST := $(abspath crates/flac-lite-gba/Cargo.toml)
# The overlay is a LIBRARY: no `agb`, so `overlay-check`/`overlay-test` have no
# link step to witness. examples/overlay-kernel-check is the ROM that supplies
# one, and it is the only place in the tree the assembled ARM kernel executes.
OVERLAY_ROM := overlay-kernel-check.gba
OVERLAY_ROM_DIR := examples/overlay-kernel-check
OVERLAY_ROM_MANIFEST := $(abspath examples/overlay-kernel-check/Cargo.toml)
OVERLAY_ROM_BIN := target/$(TARGET)/release/overlay-kernel-check

# --- the measurement instrument ----------------------------------------------
# examples/flac_spike is the `ctrl` arm of every performance record in this
# repository and the calibration anchor for the ledgers. Its feature-gated
# instrument arms (6a-3/6a-5/6a-5b/6a-7) and its 10,512 Hz rate flow are
# ARCHIVED: the sources stay for their records, their Make entrypoints are gone
# (flac-docs/history.md names the runs they produced).
SPIKE_CRATE_DIR := examples/flac_spike
SPIKE_CRATE_MANIFEST := $(abspath examples/flac_spike/Cargo.toml)
SPIKE_ROM := flac-spike.gba
SPIKE_CRATE_BIN := $(SPIKE_CRATE_DIR)/target/$(TARGET)/release/flac-spike

# The playback data path, lifted out of the instrument into a crate of its own
# (ALBUM-PLAYER.md §2.2). The instrument, the product ROM and the album probe
# all depend on it, so its host suite is where the conversion, the ring
# accounting, `truncate()` and the FNV folds are witnessed — once, for every
# consumer.
PLAYBACK_MANIFEST := $(abspath crates/gba-playback/Cargo.toml)

# --- other codec tracks (their own entry points; not FLAC) --------------------
ULI_CRATE_MANIFEST := $(abspath crates/uli-lite/Cargo.toml)
ULI_SPIKE_DIR := examples/uli_spike
ULI_SPIKE_BIN := $(ULI_SPIKE_DIR)/target/$(TARGET)/release/uli-spike
ULI_SPIKE_ROM := uli-spike.gba
ULI_OVERLAY_MANIFEST := $(abspath crates/uli-lite-gba/Cargo.toml)
# The ULI2 measurement vehicle (examples/uli_bench/): prove the corpus pins on the
# measured image, then time full frames through both kernel sets. An mGBA
# measurement; no playback or hardware claim. ULI2's plan, not FLAC's.
ULI_BENCH_DIR := examples/uli_bench
ULI_BENCH_BIN := $(ULI_BENCH_DIR)/target/$(TARGET)/release/uli-bench
ULI_BENCH_ROM := uli-bench.gba
ULI_BENCH_LOG ?= uli-bench.log
OPUS_CRATE_DIR := examples/opus_spike
OPUS_CRATE_MANIFEST := $(abspath examples/opus_spike/Cargo.toml)
OPUS_ROM := opus-spike.gba
OPUS_PROBE_ROM := ctor-probe.gba
OPUS_CRATE_BIN := $(OPUS_CRATE_DIR)/target/$(TARGET)/release/opus-spike

# --- the Album Player's Phase 0 probes (ALBUM-PLAYER.md §11, §13 AP-0) -------
# Three throwaway probe ROMs and one host gate, all from examples/album_probe/
# (its own workspace, same pattern). The shared modules are `#[path]` includes of
# flac_integration's geometry and flac_spike's playback/checksum — one
# implementation, two consumers — so nothing in crates/ or the other examples/
# moves for them. The ART1 blob and its pin are TRACKED because they are development
# instruments: this host gate reads those exact bytes, and scripts/album_probe_art.py
# is deterministic and in-tree. ALBUM-PLAYER.md §2.3 sorts assets that way —
# instruments are tracked, per-album and per-ROM artifacts are build output — so
# Phase 1's GAFP blobs, ALBM1 and src/assets.rs are generated and gitignored, the
# way `master-pack` already treats its own (§2.3, §9.0).
ALBUM_PROBE_DIR := examples/album_probe
ALBUM_PROBE_MANIFEST := $(abspath examples/album_probe/Cargo.toml)
ALBUM_PROBE_TARGET_DIR := $(ALBUM_PROBE_DIR)/target/$(TARGET)/release
ALBUM_PROBE_ART := $(ALBUM_PROBE_DIR)/assets/probe_art1.bin
ALBUM_PROBE_ART_PIN := $(ALBUM_PROBE_DIR)/src/art_pin.rs
ALBUM_PROBE_ART_TOOL := scripts/album_probe_art.py

# ===========================================================================
# format / build / baseline ROM
# ===========================================================================
#
# Builds auto-format (all workspaces) so experimental code lands rustfmt-clean;
# `check` is the strict non-writing gate. Builds never depend on tests, and tests
# never reformat — a CI `make test` must not mask drift that `make check` exists
# to fail on.

WORKSPACES := . crates/flac-lite crates/flac-lite-gba crates/gba-playback \
              crates/gba-audio-hw \
              examples/flac_spike \
              examples/overlay-kernel-check examples/opus_spike \
              crates/uli-lite crates/uli-lite-gba examples/uli_spike examples/uli_bench \
              examples/album_probe

# examples/album_player is deliberately NOT in WORKSPACES: its `src/lib.rs`
# declares `pub mod assets;` from a GENERATED, gitignored `src/assets.rs` that
# only `make album-bake` writes, so it cannot be a plain `cargo fmt` / `cargo
# clean` target on a fresh clone — rustfmt resolves `mod` statically and aborts.
# Its fmt coverage comes from the `fmt_with_generated_stub.sh` call below (which
# writes a comment-only stub with GENERATED=<file>, formats, and removes it), and
# its caches are still covered by `clean-caches`, whose WORKSPACE_DIRS is derived
# from the manifests on disk rather than from this list.

# Every cargo workspace root in the tree, derived from the manifests on disk (see
# `clean`). Pruned so a warm `target/`'s vendored manifests are never mistaken for
# workspace roots of their own.
WORKSPACE_DIRS := $(sort $(patsubst %/,%,$(dir $(shell find . \
        \( -path ./target -o -path '*/target' -o -path ./.git \) -prune -o \
        -name Cargo.toml -print 2>/dev/null))))

# flac_integration is formatted through scripts/fmt_with_generated_stub.sh, NOT a
# bare `cargo fmt`: its src/lib.rs declares a feature-gated module from a GENERATED,
# gitignored file (`src/master_pin.rs`, written by `make master-pack`), and rustfmt
# resolves `#[path]` modules statically regardless of `cfg` — so a bare `cargo fmt`
# aborts on every tree where the master asset has never been packed, i.e. a fresh
# clone. The helper writes a parse-only stub when the pin is absent, formats, and
# removes it; a real pin is formatted in place and never removed. It takes the crate
# dir as an argument and never changes the cwd (its own header says why), and it
# exits with rustfmt's status, so `check` stays a real gate.

ifeq ($(HAS_RUSTFMT),1)
format:
	@for d in $(WORKSPACES); do (cd $$d && $(CARGO) fmt) || exit 1; done
	CARGO='$(CARGO)' sh scripts/fmt_with_generated_stub.sh $(FLAC_CRATE_DIR)
	CARGO='$(CARGO)' GENERATED='$(abspath $(ALBUM_ASSETS_RS))' sh scripts/fmt_with_generated_stub.sh $(ALBUM_CRATE_DIR)
else
format:
	@echo "format: skipping — no rustfmt for '$(TOOLCHAIN)' (build continues)."
	@echo "  enable with: rustup component add rustfmt --toolchain $(TOOLCHAIN)"
endif

build: format
	$(CARGO) build --release --target $(TARGET)

# Link + fix the baseline ROM. Needs agb-gbafix on PATH (the image installs it;
# natively `cargo install agb-gbafix --version 0.25.0 --locked` — pinned because
# it rewrites the header every recorded ROM sha is computed over).
rom: build
	agb-gbafix target/$(TARGET)/release/gba-sound-player -o $(ROM)
	@echo "ROM built: $(ROM)"

# ===========================================================================
# FLAC — the product path
# ===========================================================================
#
# The happy path, end to end, is two commands:
#
#   make master-pack     INPUT=track.wav     # resample + encode + pack the asset
#   make master-play-rom INPUT=track.wav     # …and build a playable ROM from it
#
# Product geometry (all of it derived from these numbers — see FLAC.md):
# 16,384 Hz, 16-bit stereo, 2048-sample frames, 8-bit FIFO output, one 2^24 Hz
# timer as the sample clock. `gafp_pack.py` refuses anything else, and the ROM
# refuses an asset that declares a rate it does not play at (Q14).

# The decode/verify image: embeds the committed GAFP asset, parses its manifest,
# decodes every frame through the facade with the overlay's ARM kernel, checks
# the pinned PCM, then times a second pass. No sound — that is `product-play-rom`.
# It is also the real LINK witness for the core (what `overlay-check`, an rlib,
# structurally cannot be) and for the `scan` walk the facade is built on.
# `gafp-check` is a prerequisite: no product image is built over a blob that no
# longer matches its generator.
flac-rom: gafp-check format
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(FLAC_CRATE_BIN) -o $(FLAC_ROM)
	@echo "FLAC ROM built: $(FLAC_ROM) (manifest facade + ArmRiceKernel; decode-only)"

# The playback image on the committed product asset. It drives
# asset → manifest → Decoder → convert → ring → sound DMA/FIFO at 16,384 Hz,
# counts its own underflows, gates its period audit and its played-form FIFO
# pins, and prints a verdict to the log AND the screen. The image is not gated by
# `make test`: build it, then run it by hand.
product-play-rom: gafp-check format
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --bin product-play
	agb-gbafix $(PRODUCT_PLAY_BIN) -o $(PRODUCT_PLAY_ROM)
	@echo "product-play ROM built: $(PRODUCT_PLAY_ROM)"
	@echo "  run: timeout -k 5 120 $(GBA_TEST_RUNNER) $(PRODUCT_PLAY_ROM)   and read the [pp-rom] log"
	@echo "  a PASS ends in 'verdict PASSED — screen BLUE' and then loops forever: the"
	@echo "  timeout kills it, so exit 124 IS the passing result. Read the log, not the status."

# Encode an arbitrary source master into a playable GAFP asset at the product
# rate/depth. `--master` is the path for real music: libFLAC chooses
# CONSTANT/FIXED-1..4 from a real signal, so those shapes are accepted without a
# measured product row — LPC is still refused, and 16,384 Hz / 16-bit /
# 2048-sample frames are still enforced. `--emit-pin` writes the pins the ROM
# checks its own embedded blob against.
master-pack: toolchain-check
	@test -n "$(INPUT)" || { echo "usage: make master-pack INPUT=source.wav"; exit 1; }
	python3 scripts/gafp_pack.py --master --emit-pin "$(MASTER_PIN)" "$(INPUT)" "$(MASTER_ASSET)"
	@echo "master-pack done: $(MASTER_ASSET) + $(MASTER_PIN)"

# The same product-play binary built against that master asset. This is the image
# C2 was accepted on (flac-docs/history.md): real audio, 16,384 Hz, both units.
master-play-rom: master-pack format
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --bin product-play --features master-asset
	agb-gbafix $(PRODUCT_PLAY_BIN) -o $(MASTER_PLAY_ROM)
	@echo "master-play ROM built: $(MASTER_PLAY_ROM)"
	@echo "  run: timeout -k 5 120 $(GBA_TEST_RUNNER) $(MASTER_PLAY_ROM)   (exit 124 = pass)"

# ===========================================================================
# FLAC — gates
# ===========================================================================
#
# Two crates, deliberately separate gates, so a decoder bug is never confused
# with an integration failure:
#
#   flac-test   flac-lite ALONE — the thumbv4t compile gate plus the host suites.
#               No agb, no ROM, no emulator.
#   test-rom    the root ROM's #[test_case] suite in headless mGBA.
#
# The core's non-default configurations each get their own gate because the
# default build does not compile what they cover (`required-features` on the
# suites makes that a checked fact, not a convention). `flac-gates` runs all of
# them plus the combination arms in one command; anything touching
# crates/flac-lite owes it.

# Gate 1 is `cargo check --target $(TARGET) -Zbuild-std=core,alloc` run IN-TREE,
# where inheriting the root config is the whole point: it is the gate symphonia
# could never pass, because it keeps `std` out. Gate 2 is the host suite run from
# $(GATE_NEUTRAL_CWD) with --manifest-path, so the root's GBA `target` AND
# `build-std` are never inherited at all (an in-tree override is not durable:
# cargo merges config arrays across layers). Every host gate below takes this
# same shape; copy it, do not re-derive it.
flac-test: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features instrument
	@echo "flac-test passed: flac-lite compiles for $(TARGET) and passes host tests"

# The audited i64 reference build (`--no-default-features`, i32-integrator off).
# `flac-test` runs the narrow default, so this is the companion half of the width
# differential: both configurations are pinned to the same external oracle
# (`flac -d` PCM), which is what makes the pair a differential rather than two
# greens earned by the same arithmetic.
flac-test-wide: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --no-default-features
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --no-default-features
	@echo "flac-test-wide passed: the i64 reference leg compiles for $(TARGET) and its suite is green"

# Frame CRC verification ON. Verification is OFF in the default build by design
# (release consumes the CRCs unchecked), so this gate is what keeps the OFF
# default honest about the ON arm.
flac-test-crc: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check
	@echo "flac-test-crc passed: CRC verification compiles for $(TARGET) and its witness is green"

# The table-less frame walk ON — the code the product facade is built on, so this
# is the walk's thumbv4t compile plus its layout suite.
flac-test-scan: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features scan,instrument
	@echo "flac-test-scan passed: the frame walk compiles for $(TARGET) and its witness suite is green"

# The portable hybrid Rice kernel ON: the second `RiceKernel` mechanism and the
# two-mechanism differential. It stays a feature (and not the default) because
# the on-target reading declined the flip: +42.6 c/sample against the incumbent.
flac-test-rice: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features rice-hybrid
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features rice-hybrid
	@echo "flac-test-rice passed: the portable hybrid kernel compiles for $(TARGET) and its differential is green"

# The portability witness: the core compiles for a SECOND bare-metal ARM target.
# A build fact with no performance claim attached — nothing in this tree measures
# a non-GBA target. `scan` and `rice-hybrid` are on deliberately: a feature-gated
# module no gate builds for a second target has an asserted, not measured,
# portability claim. `iwram-pin` stays off — no non-GBA target defines that section.
flac-portable-check: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(SECOND_TARGET) -Zbuild-std=core,alloc --features scan,rice-hybrid
	@echo "flac-portable-check passed: flac-lite with the walk and the twin on compiles for $(SECOND_TARGET) (no perf claim)"

# The committed product blob's tripwire: rebuild it in memory and compare byte
# for byte. No encoder needed. Its reason for existing alongside the real packer:
# it reads the *spike's* own committed frames, so it is the only gate that says
# the asset the ROM plays is the asset the instrument measured.
gafp-check: toolchain-check
	python3 scripts/gen_gafp_blob.py --check
	@echo "gafp-check passed: the committed GAFP blob still matches its generator"

# The real packer's gate, three legs: (1) `gafp_pack.py` reproduces the committed
# product blob byte for byte; (2) `gen_gafp_fixtures.py --check` regenerates the
# fixture set AND re-runs the pack-time refusals (real LPC at the product
# profile, an off-target rate, 8-bit), which are C2's guard and therefore
# witnesses in their own right; (3) `gafp_asset_facts.rs` reads all of it back
# through `Manifest` / `scan::Frames` / `Decoder` against the `flac -d` pins.
# This is the only FLAC leg that runs libFLAC: it needs `flac` + `metaflac` on
# PATH, so a red here in an image without them is a missing tool, not a decoder
# regression — check that before bisecting.
gafp-pack-check: toolchain-check
	@command -v flac >/dev/null && command -v metaflac >/dev/null || \
	  { echo "gafp-pack-check needs flac and metaflac on PATH (the reference encoder, decoder and block lister)"; exit 1; }
	@d=$$(mktemp -d /tmp/gafp-pack.XXXXXX); \
	 python3 scripts/frame_vectors.py synth "$$d" >/dev/null && \
	 flac -1 -f -s -l 0 -b 2048 -m -o "$$d/l0_stereo.flac" "$$d/stereo.wav" && \
	 python3 scripts/gafp_pack.py "$$d/l0_stereo.flac" "$$d/l0.gfp" \
	   --expect examples/flac_integration/assets/spike_l0.gfp; \
	 rc=$$?; rm -rf "$$d"; exit $$rc
	python3 scripts/gen_gafp_fixtures.py --check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features scan,instrument --test gafp_asset_facts
	@echo "gafp-pack-check passed: the packer writes the committed blob byte for byte, the fixtures and their refusals still match their generator, and the crate reads it all back bit-exactly"

# One command for every `crates/flac-lite` configuration. It adds nothing the
# named gates do not witness; the two combination arms at the end are the only
# builds that were not otherwise runnable, and a *combination* is the only way to
# see a second instantiation. Counts move with the tree: they are recorded in
# flac-docs/measurements.md, and a green run's value is its exit status.
# Not parallel-safe by design — `make -j flac-gates` interleaves output.
flac-gates: flac-test flac-test-wide flac-test-crc flac-test-scan flac-test-rice flac-portable-check gafp-check gafp-pack-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check,scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check,scan
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check,scan,rice-hybrid
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check,scan,rice-hybrid
	@echo "flac-gates passed: every flac-lite configuration is green (default, wide, crc, scan, rice, both, all three, the second target, and both gafp legs)"

# The product geometry's host model: it replays the shipped manifest's real frame
# lengths through the SAME `RingFeed` and the SAME `geometry` constants the ROM
# compiles (both `#[path]`-shared from src/lib.rs) against a credit-per-period
# clock. This is where flow control, the two tolerances and the ring sizing are
# *justified* rather than asserted — so the geometry's comments are not the only
# witness. It fails closed if the model file is missing: a geometry with no host
# model is a geometry nobody checked.
product-play-test:
	@test $$(find $(FLAC_CRATE_DIR)/tests -name 'product_play_witness.rs' | wc -l) -eq 1 \
	  || { echo "product-play-test: examples/flac_integration/tests/product_play_witness.rs is missing — the host model for this geometry has to exist before the geometry can be claimed"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(FLAC_INTEGRATION_MANIFEST) --target $(HOST_TRIPLE) --test product_play_witness
	@echo "product-play-test passed: the product geometry's ring, flow control and sizing are modelled on the host against the shipped manifest"

# ===========================================================================
# FLAC — the GBA overlay and its ARM kernel (crates/flac-lite-gba)
# ===========================================================================
#
# The overlay holds the platform-bound `RiceKernel` realizations, each behind its
# own feature: a default build of it contains no kernel, no asm, no FFI and no
# `unsafe`. Like the core's configurations, a non-default configuration is
# witnessed by its OWN gate — which is also what keeps a green default honest
# about what the default does not contain.
#
# The crate has NO local .cargo/config.toml and must not get one: cargo
# CONCATENATES `target.<triple>.rustflags` across config layers, so a local
# re-declaration links `-Tgba.ld` twice (`region 'ewram' already defined`).

# Compile witnesses. An rlib has no link step, so neither of these claims a codegen
# charge or a link; the link witness is `overlay-kernel-rom` (and, for the product
# configuration, `flac-rom`).
overlay-check: toolchain-check
	cd $(OVERLAY_CRATE_DIR) && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	@echo "overlay-check passed: the overlay AND the core it depends on both type-check for $(TARGET)"

overlay-check-arm: toolchain-check
	cd $(OVERLAY_CRATE_DIR) && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features rice-arm
	@echo "overlay-check-arm passed: rice-arm type-checks for $(TARGET) — asm assembled, FFI and KernelAbi declared"

# Host witnesses. `overlay-test-arm` runs the boundary differential (q = 31, the
# w == 0 handoff, p = 0, both §9.2.7.3 guards, the end-of-stream shapes) — but on
# the host that differential runs a Rust MODEL of the asm. It witnesses the
# contract; the instruction stream is the ROM's and objdump's.
overlay-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(OVERLAY_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "overlay-test passed: the overlay's DEFAULT configuration is green (no kernel, asm, FFI or unsafe exists in it)"

overlay-test-arm: toolchain-check
	cd $(OVERLAY_CRATE_DIR) && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features rice-arm
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(OVERLAY_CRATE_MANIFEST) --target $(HOST_TRIPLE) --features rice-arm
	@echo "overlay-test-arm passed: the rice-arm contract witness is green (the asm itself runs only in the kernel-check ROM)"

# The ELF fact a correctness sweep cannot show: a Thumb build of the same
# algorithm computes the same answers, so ARM mode, the four-byte encodings and
# the `pop {…, lr}` + `bx lr` return are read out of the bytes. Host binutils is
# enough; no arm-none-eabi toolchain needed. Rules and provenance: the script.
overlay-objdump: toolchain-check
	sh scripts/check_arm_kernel_objdump.sh $(OVERLAY_ROM_DIR)/$(OVERLAY_ROM_BIN)
	@echo "overlay-objdump passed: arm_rice_partition is four-byte ARM code with a pop {…, lr} + bx lr return and no LDM-to-PC"

# The device witness: the only place the assembled kernel executes (this host has
# no ARM executor, so mGBA's ARM7TDMI is the oracle). Debug profile on purpose —
# overflow checks and the model's debug_assert domain guard are part of what is
# being witnessed. This is a correctness gate, not a timing one.
overlay-kernel-rom: format
	cd $(OVERLAY_ROM_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(OVERLAY_ROM_DIR)/$(OVERLAY_ROM_BIN) -o $(OVERLAY_ROM)
	sh scripts/check_arm_kernel_objdump.sh $(OVERLAY_ROM_DIR)/$(OVERLAY_ROM_BIN)
	@echo "Kernel-check ROM built: $(OVERLAY_ROM) (its own test asserts the tables' .iwram addresses at runtime)"

overlay-kernel-test: toolchain-check
	cd $(OVERLAY_ROM_DIR) && \
	  CARGO_TARGET_$(TARGET_ENV)_RUNNER=$(GBA_TEST_RUNNER) \
	  $(CARGO) test --target $(TARGET)
	@echo "overlay-kernel-test passed: the assembled ARM kernel, the model and the core's incumbent agree on the committed corpus and the synthesized boundaries, on mGBA's ARM7TDMI"

# One command for the kernel the product links.
overlay-gates: overlay-check overlay-test overlay-check-arm overlay-test-arm overlay-kernel-rom overlay-kernel-test
	@echo "overlay-gates passed: both overlay configurations compile for $(TARGET), their host contracts are green, and the assembled kernel is witnessed on mGBA's ARM7TDMI"

# ===========================================================================
# FLAC — the measurement instrument (examples/flac_spike)
# ===========================================================================
#
# The host witness over the EXACT embedded clips, then the on-target decode proof
# and the cycle harness. Keep the default arm's bytes honest: cross-session cycle
# records are anchored on this image (flac-docs/measurements.md).

spike-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(SPIKE_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "spike-test passed: spike clips decode bit-exactly on the host"

# The lifted playback data path (crates/gba-playback). Fails closed the way
# `album-probe-test` does: a cargo invocation that fails or a log with no
# `test result: ok.` line is reported as the suite not passing, and a green that
# collected (nearly) nothing is refused too — the count is read from the log only
# after the suite has passed. `--lib` on purpose: the whole suite lives in `src/`,
# so exactly one result line exists and the count cannot pick up a doc-test line.
playback-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(PLAYBACK_MANIFEST) --target $(HOST_TRIPLE) --lib > $(abspath playback-test.log) 2>&1; \
	  status=$$?; \
	  cat $(abspath playback-test.log); \
	  [ $$status -eq 0 ] && grep -q '^test result: ok\.' $(abspath playback-test.log) \
	    || { echo "playback-test: the library suite did not pass — see playback-test.log"; exit 1; }; \
	  n=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath playback-test.log)); \
	  [ -n "$$n" ] && [ "$$n" -ge 25 ] || { echo "playback-test: collected '$$n' tests, needs >= 25 — a green that names nothing is not a green"; exit 1; }
	@echo "playback-test passed: FIFO conversion, ring accounting, truncate()'s ledger, both FNV folds, and §6's one-divide census"

spike-rom: format
	cd $(SPIKE_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(SPIKE_CRATE_BIN) -o $(SPIKE_ROM)
	@echo "Spike ROM built: $(SPIKE_ROM)"
	@echo "  run: timeout -k 5 280 $(GBA_TEST_RUNNER) $(SPIKE_ROM) | grep -E 'stats:|budget:|verdict'"

# ===========================================================================
# Other codec tracks (see OPUS.md / ULI2.md — not the FLAC path)
# ===========================================================================

opus-rom: format
	cd $(OPUS_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(OPUS_CRATE_DIR)/target/$(TARGET)/release/opus-spike -o $(OPUS_ROM)
	agb-gbafix $(OPUS_CRATE_DIR)/target/$(TARGET)/release/ctor-probe -o $(OPUS_PROBE_ROM)
	@echo "Opus spike ROM built: $(OPUS_ROM) (+ constructor probe $(OPUS_PROBE_ROM))"
	@echo "  the ctor-probe image is a deliberately-red compile/link witness; never run it in a test gate"

opus-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(OPUS_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "opus-test passed: opus spike scaffold pins hold on the host"

uli-test: toolchain-check
	cd crates/uli-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "uli-test passed: uli-lite compiles for $(TARGET) and is bit-exact on the host corpus"

uli-check: toolchain-check
	cd crates/uli-lite && $(CARGO) fmt --check
	cd crates/uli-lite-gba && $(CARGO) fmt --check
	cd $(ULI_BENCH_DIR) && $(CARGO) fmt --check

# Phase 1 decode-only ROM: a correctness image, not a player and not a cycle record.
uli-spike-rom: format
	cd $(ULI_SPIKE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(ULI_SPIKE_BIN) -o $(ULI_SPIKE_ROM)
	@echo "ULI2 spike ROM built: $(ULI_SPIKE_ROM)"

uli-bench-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_BENCH_ROM)
	@echo "ULI2 bench ROM built: $(ULI_BENCH_ROM)"

# Run it headlessly and keep the log. The ROM prints the runner's exit sentinel
# after its verdict, so this returns in seconds; the timeout is a leash for a hung
# image, not the normal path. The gate is the log's PASS/FAIL and the numbers in it,
# NOT the exit status (contrast the FLAC playback ROMs, which loop forever on a pass).
uli-bench: uli-bench-rom
	timeout -k 5 300 $(GBA_TEST_RUNNER) $(ULI_BENCH_ROM) 2>&1 | tee $(ULI_BENCH_LOG) || true
	@grep -q '^\[uli-bench\] PASS' $(ULI_BENCH_LOG) || { echo "uli-bench: harness/pin failure — see $(ULI_BENCH_LOG)"; exit 1; }
	@echo "uli-bench done: $(ULI_BENCH_LOG)"

# --- PR E: the album-length corpus (a witness, never a gate input) -----------
#
# PLAN-OF-RECORD.md "PR E" needs a corpus large enough to be a worst-case
# witness. It is the *tracked* reference master at the **product
# configuration** — 32,768 Hz, hop 512, `--cap 74`, budget scaled to the
# product's ~102 kbit/s — because density, not duration, is what the tail
# measures, and because the reference album's own 43-minute blob does not fit a
# 32 MiB cart beside this instrument (33,729,148 B against 33,554,432 B).
#
# Everything this target generates is build output (ALBUM-PLAYER.md §2.3's
# division): `examples/uli_bench/assets/` and `src/album_pin.rs` are gitignored,
# the tracked input is the WAV, and the pin is C-sourced — `scripts/
# uli_album_pin.py` takes the C reference decoder's own `hash_of_frame_hashes`
# and re-verifies its published checkpoints against the decoder's 8-bit track
# output before it will emit a pin. Needs the sibling `gba-audio` checkout and
# ffmpeg; it therefore never runs in `make test`.
#
# `gba-audio/encoder/prep.py` needs numpy, scipy and soundfile, which a bare
# system python often lacks (PEP 668). Point `ULI_ALBUM_PY` at a venv:
#   python3 -m venv /tmp/uli-venv && /tmp/uli-venv/bin/pip install numpy scipy soundfile
#   make uli-album-asset ULI_ALBUM_PY=/tmp/uli-venv/bin/python3
GBA_AUDIO ?= $(abspath ../gba-audio)
ULI_ALBUM_WAV ?= $(abspath assets/sound/bioluminescent.wav)
ULI_ALBUM_DIR := $(ULI_BENCH_DIR)/assets
ULI_ALBUM_BLOB := $(ULI_ALBUM_DIR)/biomaster.uli
ULI_ALBUM_REF := $(ULI_ALBUM_DIR)/ref
ULI_ALBUM_PIN := $(ULI_BENCH_DIR)/src/album_pin.rs
ULI_ALBUM_BENCH_ROM := uli-album-bench.gba
ULI_ALBUM_BENCH_LOG ?= uli-album-bench.log
# The reference album's budget (33,292,288 B) over its 2,612.815 s is 12,742 B/s;
# scaled to this master's 258.61 s, so the stream lands at the product's rate.
ULI_ALBUM_BUDGET ?= 3294000
ULI_ALBUM_THREADS ?= 8
ULI_ALBUM_PY ?= python3

uli-album-asset:
	@test -f $(ULI_ALBUM_WAV) || { echo "uli-album-asset: missing $(ULI_ALBUM_WAV)"; exit 1; }
	@test -x $(GBA_AUDIO)/build/uli_enc -o -f $(GBA_AUDIO)/encoder/Makefile || \
	  { echo "uli-album-asset: no gba-audio encoder at $(GBA_AUDIO) (set GBA_AUDIO=)"; exit 1; }
	mkdir -p $(ULI_ALBUM_DIR) $(ULI_ALBUM_REF)
	$(ULI_ALBUM_PY) $(GBA_AUDIO)/encoder/prep.py $(ULI_ALBUM_WAV) --out $(ULI_ALBUM_DIR)
	$(MAKE) -C $(GBA_AUDIO)/encoder
	$(GBA_AUDIO)/build/uli_enc --album $(ULI_ALBUM_DIR)/album.f32 \
	  --tracks $(ULI_ALBUM_DIR)/tracks.tsv --out $(ULI_ALBUM_BLOB) \
	  --budget $(ULI_ALBUM_BUDGET) --cap 74 --threads $(ULI_ALBUM_THREADS)
	$(GBA_AUDIO)/build/uli_dec $(ULI_ALBUM_BLOB) $(ULI_ALBUM_REF) --seektest
	$(ULI_ALBUM_PY) scripts/uli_album_pin.py --blob $(ULI_ALBUM_BLOB) \
	  --ref $(ULI_ALBUM_REF) --rs $(ULI_ALBUM_PIN)
	@echo "uli-album-asset: $(ULI_ALBUM_BLOB) and $(ULI_ALBUM_PIN) (both gitignored)"

uli-album-bench-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features album-corpus
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_ALBUM_BENCH_ROM)
	@echo "ULI2 album-corpus bench ROM built: $(ULI_ALBUM_BENCH_ROM)"

# The tail witness: worst frame **with its index**, mean, min, sum, on the album
# corpus, decoded by the product build. Gate on PASS; the numbers are in the log.
uli-album-bench: uli-album-bench-rom
	timeout -k 5 600 $(GBA_TEST_RUNNER) $(ULI_ALBUM_BENCH_ROM) 2>&1 | tee $(ULI_ALBUM_BENCH_LOG) || true
	@grep -q '^\[uli-bench\] PASS' $(ULI_ALBUM_BENCH_LOG) \
	  || { echo "uli-album-bench: harness/pin failure — see $(ULI_ALBUM_BENCH_LOG)"; exit 1; }
	@grep -E '^\[uli-bench\] (corpus|decode frames|frame prod8 |prod8 pin )' $(ULI_ALBUM_BENCH_LOG) || true
	@echo "uli-album-bench done: $(ULI_ALBUM_BENCH_LOG)"

# PR E's second deliverable: the **gate total** — decode + convert + IRQ + DMA in
# one window, with the shipping `RingFeed` and the real sound timebase (timer0 =
# one sample, timer1 = one 512-sample feeder period, DMA1/2 on the FIFOs). The
# arm is `src/gate.rs`; `uli-album-bench` is the decode wall on the same corpus,
# so the pair separates "decode" from "decode + output stage" in one image.
#
# The default arm is the **player model**: it hashes nothing in the paced loop
# (a player does not), drains the ring before stopping, and therefore runs for
# exactly the album's length. `--features gate-hash` is the attribution arm that
# verifies in-loop instead: +~5,000 cycles/frame on the windows *and* 12 s of
# starvation, because in a paced pass untimed producer work is not free.
#
# The verdict gates on the pins and on the counter's integrity, **not** on
# underruns: a starved period is the finding this arm exists to produce.
ULI_GATE_ROM := uli-gate.gba
ULI_GATE_LOG ?= uli-gate.log
uli-gate-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features gate,album-corpus
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_GATE_ROM)
	@echo "ULI2 gate-total ROM built: $(ULI_GATE_ROM)"

# The same gate arm on the **129-frame fixture**: a ~30 s hardware run whose
# audio is the synthetic fixture (two-tone stereo, a silent gap, a noisy band,
# high-frequency clicks) rather than music. It is the cheapest witness that the
# whole chain — timer0/1, DMA1/2 on the FIFOs, the feeder IRQ, the ring — works on
# silicon, and the fixture is C-pinned on both outputs, so a blue screen is a real
# pass. Use it before flashing the 3.7 MB album build.
# The **audio-only** ROM: one paced playback pass over the album corpus and
# nothing else, so its runtime is the album's own length (~4.3 min) and the only
# outputs are sound and a screen colour. This is the image to flash for a manual
# listen; every number still comes from `uli-gate`.
ULI_PLAY_ROM := uli-play.gba
uli-play-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features play-only
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_PLAY_ROM)
	@echo "ULI2 audio-only ROM built: $(ULI_PLAY_ROM)"
	@stat -c '  %s bytes' $(ULI_PLAY_ROM)

ULI_GATE_FIXTURE_ROM := uli-gate-fixture.gba
uli-gate-fixture-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features gate
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_GATE_FIXTURE_ROM)
	@echo "ULI2 gate-total ROM (fixture corpus) built: $(ULI_GATE_FIXTURE_ROM)"

uli-gate-fixture: uli-gate-fixture-rom
	timeout -k 5 180 $(GBA_TEST_RUNNER) $(ULI_GATE_FIXTURE_ROM) 2>&1 | tee uli-gate-fixture.log || true
	@grep -q '^\[uli-bench\] PASS' uli-gate-fixture.log \
	  || { echo "uli-gate-fixture: harness/pin failure — see uli-gate-fixture.log"; exit 1; }
	@grep -E '^\[uli-bench\] (corpus|decode frames|frame prod8 |frame gate |gate feeder|gate pin)' uli-gate-fixture.log || true

uli-gate: uli-gate-rom
	timeout -k 5 900 $(GBA_TEST_RUNNER) $(ULI_GATE_ROM) 2>&1 | tee $(ULI_GATE_LOG) || true
	@grep -q '^\[uli-bench\] PASS' $(ULI_GATE_LOG) \
	  || { echo "uli-gate: harness/pin failure — see $(ULI_GATE_LOG)"; exit 1; }
	@grep -E '^\[uli-bench\] (corpus|decode frames|frame prod8 |frame gate |gate feeder|gate pin|calib)' $(ULI_GATE_LOG) || true
	@echo "uli-gate done: $(ULI_GATE_LOG)"

# --- ULI2 by-difference attribution (PLAN-OF-RECORD.md, critical path Step 0) ---
#
# Each arm rebuilds the bench with exactly one part of the product frame removed
# and dumps the per-frame window (`--features attrib-dump`), so the frame can be
# attributed by difference at a *given* frame index rather than only in the mean.
# These are TIMING-ONLY builds: the album pins fail by construction in every arm,
# so this target does NOT gate on the log's PASS. Read the `frame prod8` line and
# the `pfc` rows, never the pin verdict. `attrib-dump` costs a constant +16
# cycles per window in this image (control 274,214 vs the product 274,198) —
# deltas are unaffected; quote absolutes from the product arm.
ULI_ATTRIB_ARMS := control synth pre fft post ola ms entropy synth-entropy

# --- Sub-frame entropy attribution (Step 0b) --------------------------------
#
# The entropy/dequant path owns 69% of the worst frame's distance to the C shape,
# so it is attributed one level down before its rewrite: `attrib-entropy-{reader,
# nolut,nodequant,nowrite}` remove one layer each (`attrib-count` adds the
# `huff_slow` hit counter). Same method and same caveats as `uli-attrib`: these
# are TIMING-ONLY images whose pins fail by construction, and `attrib-dump` costs
# a constant +16 cycles per window. Quote `frame prod8` / `pfc`, never the pins.
ULI_SUBFRAME_ARMS := s-base s-reader s-nolut s-nodequant s-nowrite s-count
uli-attrib-subframe: toolchain-check
	@for arm in $(ULI_SUBFRAME_ARMS); do \
	  case $$arm in \
	    s-base) feats="attrib-dump,attrib-synth,attrib-entropy" ;; \
	    s-reader) feats="attrib-dump,attrib-synth,attrib-entropy-reader" ;; \
	    s-count) feats="attrib-dump,attrib-synth,attrib-count" ;; \
	    *) feats="attrib-dump,attrib-synth,attrib-entropy-$${arm#s-}" ;; \
	  esac; \
	  echo "uli-attrib-subframe: $$arm ($$feats)"; \
	  ( cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features "$$feats" ) || exit 1; \
	  agb-gbafix $(ULI_BENCH_BIN) -o uli-subframe-$$arm.gba || exit 1; \
	  timeout -k 5 300 $(GBA_TEST_RUNNER) uli-subframe-$$arm.gba > uli-subframe-$$arm.log 2>&1 || true; \
	  grep -E '^\[uli-bench\] frame prod8' uli-subframe-$$arm.log || exit 1; \
	  grep -E '^\[uli-bench\] (attrib|prod8 slow_hits)' uli-subframe-$$arm.log || true; \
	done
	@echo "uli-attrib-subframe done: uli-subframe-<arm>.log (timing-only arms; pins fail by design)"
uli-attrib: toolchain-check
	@for arm in $(ULI_ATTRIB_ARMS); do \
	  feats="attrib-dump"; \
	  case $$arm in \
	    control) : ;; \
	    synth-entropy) feats="attrib-dump,attrib-synth,attrib-entropy" ;; \
	    *) feats="attrib-dump,attrib-$$arm" ;; \
	  esac; \
	  echo "uli-attrib: $$arm ($$feats)"; \
	  ( cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features "$$feats" ) || exit 1; \
	  agb-gbafix $(ULI_BENCH_BIN) -o uli-attrib-$$arm.gba || exit 1; \
	  timeout -k 5 300 $(GBA_TEST_RUNNER) uli-attrib-$$arm.gba > uli-attrib-$$arm.log 2>&1 || true; \
	  grep -E '^\[uli-bench\] frame prod8' uli-attrib-$$arm.log || exit 1; \
	done
	@echo "uli-attrib done: uli-attrib-<arm>.log (timing-only arms; pins fail by design)"

# --- The on-target asm KERNEL twin (a real gate, added with FFT v5) ----------
#
# The bench's `ovl twin ... fftasm=` line lives behind the bench's `asm-fft`
# feature, so `make uli-bench` — which builds without it — reported
# `fftasm=true` vacuously while the product decoder was in fact calling the asm
# kernel. The album pins were the only witness. This target builds the image
# that actually runs the twin, and gates on it.
#
# It fits because the twiddle-table placement arm (`tw-iwram-arm`, 2,016 B of
# `.iwram`) is off by default: with it on, this build measured 30,284 B of a
# 30,320 B region and had no room for the 828 B stack.
ULI_FFT_TWIN_LOG := uli-kernel-twin.log
uli-kernel-twin: toolchain-check
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET) --features asm-fft,asm-pre,asm-ola
	agb-gbafix $(ULI_BENCH_BIN) -o uli-kernel-twin.gba
	timeout -k 5 300 $(GBA_TEST_RUNNER) uli-kernel-twin.gba > $(ULI_FFT_TWIN_LOG) 2>&1 || true
	@grep -E '^\[uli-bench\] asmfft5 ' $(ULI_FFT_TWIN_LOG) || { echo "uli-fft-twin: no asmfft5 arm in the log"; exit 1; }
	@grep -q 'ovl twin .* fftasm=true preasm=true olaasm=true \[PASS\]' $(ULI_FFT_TWIN_LOG) || { echo "uli-kernel-twin: an asm kernel twin failed — see $(ULI_FFT_TWIN_LOG)"; exit 1; }
	@echo "uli-kernel-twin passed: the asm FFTs (v1..v5), the asm pre-twiddle and the asm overlap-add are bit-exact on the ARM7TDMI"

# The overlay's host twin: the ARM-mode, unchecked kernels must be bit-exact with
# uli-lite's portable reference. This is what licenses the bench's overlay arm to be
# called a realisation of the same arithmetic.
uli-overlay-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_OVERLAY_MANIFEST) --target $(HOST_TRIPLE)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_OVERLAY_MANIFEST) --target $(HOST_TRIPLE) --features verify
	@echo "uli-overlay-test passed: overlay kernels and the full decoder are bit-exact with the portable reference, in both the product and the verify build"

# Per-frame operation counts from the portable decoder (host). The entropy
# attribution's instrument: symbol counts are a bitstream property, so they need
# no emulator build and no `verify` image. Pair the rows with a timed arm from
# `make uli-attrib`. Frames may be named: `make uli-frame-ops FRAMES="98 72"`.
ULI_FRAME_OPS_ALBUM ?= $(abspath crates/uli-lite/tests/fixtures/uli2_tiny.uli)
uli-frame-ops: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) run --manifest-path $(ULI_CRATE_MANIFEST) --target $(HOST_TRIPLE) --example frame_ops -- \
	    $(ULI_FRAME_OPS_ALBUM) $(FRAMES)

# Regenerate the committed expected-vector file. This WRITES a fixture, so it is
# a maintenance command and never part of `make test`.
uli-fixture: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) run --manifest-path $(ULI_CRATE_MANIFEST) --target $(HOST_TRIPLE) --example gen_expected -- \
	    $(abspath crates/uli-lite/tests/fixtures/uli2_tiny.uli) \
	    $(abspath crates/uli-lite/tests/fixtures/uli2_tiny_expected.txt)

# ===========================================================================
# Album Player — Phase 0 (AP-0): probes only, never gate inputs
# ===========================================================================
#
# The point of AP-0 is to replace ALBUM-PLAYER.md §10's target percentages with
# measurements, so the artifact is a log: three throwaway probe ROMs (UI, art,
# skip), one host gate, and the logs the ROM leg leaves behind. A probe is a
# witness, never a gate input (the ctor-probe rule): nothing here is wired into
# `flac-check` or `test`, and `album-check` (AP-0…AP-9) does not exist yet.

# Regenerate the tracked ART1 blob + pin, then audit the tool itself. Writes
# tracked files, so this is a maintenance command and never part of `make test`.
album-probe-art: toolchain-check
	python3 $(ALBUM_PROBE_ART_TOOL) --out $(ALBUM_PROBE_ART) --pin $(ALBUM_PROBE_ART_PIN)
	python3 $(ALBUM_PROBE_ART_TOOL) --self-test

# The AP-0 host gate: the ART1 round-trip and every refusal ART1.md names, §6's
# exactness with the one tagged bake-time divide, the session clock's invariants,
# and the ring's rules at both clocks. Three legs fail closed: a missing suite is
# refused; a cargo invocation that fails or a log with no `test result: ok.` line
# is reported as the suite not passing (whatever the log's tail says); and a green
# that collected (nearly) nothing is refused too — the count is read from the log
# only once the suite has passed, because a suite that names no test is not a green.
album-probe-test: toolchain-check
	@test $$(find $(ALBUM_PROBE_DIR)/tests -name 'album_probe_host.rs' | wc -l) -eq 1 \
	  || { echo "album-probe-test: $(ALBUM_PROBE_DIR)/tests/album_probe_host.rs is missing — the AP-0 host half has to exist before the probes can be believed"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_PROBE_MANIFEST) --target $(HOST_TRIPLE) --test album_probe_host > $(abspath album-probe-test.log) 2>&1; \
	  status=$$?; \
	  cat $(abspath album-probe-test.log); \
	  [ $$status -eq 0 ] && grep -q '^test result: ok\.' $(abspath album-probe-test.log) \
	    || { echo "album-probe-test: the host suite did not pass — see album-probe-test.log"; exit 1; }; \
	  n=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath album-probe-test.log)); \
	  [ -n "$$n" ] && [ "$$n" -gt 40 ] || { echo "album-probe-test: collected '$$n' tests, needs > 40 — a green that names nothing is not a green"; exit 1; }
	@tracked=$$(sha256sum $(ALBUM_PROBE_ART) | cut -d' ' -f1); \
	  tmp=$$(mktemp -d); \
	  python3 $(ALBUM_PROBE_ART_TOOL) --out $$tmp/probe_art1.bin --pin $$tmp/art_pin.rs >/dev/null \
	    || { rm -rf $$tmp; exit 1; }; \
	  regenerated=$$(sha256sum $$tmp/probe_art1.bin | cut -d' ' -f1); \
	  rm -rf $$tmp; \
	  [ "$$tracked" = "$$regenerated" ] || { echo "album-probe-test: the art tool is not deterministic — tracked $$tracked vs regenerated $$regenerated"; exit 1; }
	@echo "album-probe-test passed: ART1 round-trip + every refusal, §6 exactness, the session clock, both clocks' geometry, and a byte-identical regeneration"

# The three probe images. `cargo build` in the standalone workspace builds the
# library and every [[bin]] (ui-probe, art-probe, skip-probe), then each raw image
# is header-fixed for the cartridge — the uli-bench shape. Run one with
# `timeout -k 5 300 $(GBA_TEST_RUNNER) album-probe-<name>.gba` and read the
# `[album-probe]` lines. Each image prints its verdict, its `Tests finished
# successfully` sentinel, and then loops forever on the verdict colour: the runner
# returns as soon as it sees the sentinel, and a run that never printed one ends at
# the leash (124). So the log is the verdict — `make album-probe` greps it and does
# not trust an exit status — and the ROMs loop rather than return. Section sizes are
# printed because AP-0 has to record .text/.iwram/.bss for each probe, not estimate
# them.
album-probe-rom: format
	cd $(ALBUM_PROBE_DIR) && $(CARGO) build --release --target $(TARGET) --features probe-roms
	agb-gbafix $(ALBUM_PROBE_TARGET_DIR)/ui-probe -o album-probe-ui.gba
	agb-gbafix $(ALBUM_PROBE_TARGET_DIR)/art-probe -o album-probe-art.gba
	agb-gbafix $(ALBUM_PROBE_TARGET_DIR)/skip-probe -o album-probe-skip.gba
	@ls -l album-probe-ui.gba album-probe-art.gba album-probe-skip.gba
	@size -A $(ALBUM_PROBE_TARGET_DIR)/ui-probe $(ALBUM_PROBE_TARGET_DIR)/art-probe \
	          $(ALBUM_PROBE_TARGET_DIR)/skip-probe | grep -E '^(File|\.text|\.iwram|\.bss|\.rodata)'
	@echo "  run: timeout -k 5 300 $(GBA_TEST_RUNNER) album-probe-<ui|art|skip>.gba"
	@echo "  the verdict is the log's '[album-probe] PASS' line, never the exit status"

# Run all three headless and keep one log each. The gate is the log, not the exit
# status (the uli-bench model): the PASS line names the measurement, and the
# sentinel proves the run reached its end rather than a timeout. A log that ends
# at the leash is a FAIL even if it printed a stale PASS earlier.
album-probe: album-probe-rom
	@for probe in ui art skip; do \
	  timeout -k 5 300 $(GBA_TEST_RUNNER) album-probe-$$probe.gba > album-probe-$$probe.log 2>&1 || true; \
	  grep -q '^\[album-probe\] PASS' album-probe-$$probe.log \
	    || { echo "album-probe: album-probe-$$probe.gba printed no PASS verdict — see album-probe-$$probe.log"; exit 1; }; \
	  grep -q '^Tests finished successfully' album-probe-$$probe.log \
	    || { echo "album-probe: album-probe-$$probe.gba never reached the exit sentinel (a timeout, not a pass) — see album-probe-$$probe.log"; exit 1; }; \
	done
	@echo "album-probe done: album-probe-{ui,art,skip}.log hold the tables and the verdicts"

# ===========================================================================
# Album Player — Phase 1 (A3a): the pack step, and the four refusals
# ===========================================================================
#
# Nothing here plays audio. This is §9.0's "the bake step is a verdict" at its
# Phase-1 scope: `album.toml` (tracked) → one `GAFP` blob per distinct source →
# `ALBM1` (the app's manifest) → the generated `src/assets.rs`. The gate is
# named `album-pack-check` on purpose — `album-bake-check` is AP-5/Phase 3's
# budget-and-art gate (§9.1), not this one.

# Bake the tracked fixture (or ALBUM=your.toml). A maintenance command: it
# writes generated, gitignored files, so it is never part of `make test`.
album-bake: toolchain-check format
	@test -f "$(ALBUM)" || { echo "album-bake: $(ALBUM) does not exist — pass ALBUM=<config> or check the tracked fixture is present"; exit 1; }
	python3 scripts/album_bake.py "$(ALBUM)"

# The Phase-1 pack gate. Five legs, all fail-closed:
#   1. the inputs a clone must have (the tracked config, the host suite) and the
#      tools the bake shells to (flac, metaflac, ffmpeg, python3) are checked by
#      name, so a missing dependency is one line instead of a traceback;
#   2. bake once, then bake again and compare the asset directory's whole
#      listing and every generated byte — §9.0's determinism leg, "pack twice,
#      compare bytes", the thing that makes "generated" mean "regenerable";
#      the listing half is what catches a stale or leftover blob. The comparison
#      is also witnessed *negatively*: one flipped byte in a copy must be
#      reported, so a leg that has only ever passed is not mistaken for proof;
#   3. the bake's own `--self-test` drives all four refusals with no repo state;
#   4. the host suite runs from $(GATE_NEUTRAL_CWD) with --manifest-path and an
#      explicit --target, the flac-test shape, so the root's GBA `target` and
#      `build-std` are never inherited;
#   5. a green that collected (nearly) nothing is refused: the count is read from
#      the log only after the suite passed.
album-pack-check: toolchain-check
	@test -f "$(ALBUM)" || { echo "album-pack-check: $(ALBUM) is missing — the tracked fixture config is this gate's input"; exit 1; }
	@test $$(find $(ALBUM_CRATE_DIR)/tests -name 'album_pack_host.rs' | wc -l) -eq 1 \
	  || { echo "album-pack-check: $(ALBUM_CRATE_DIR)/tests/album_pack_host.rs is missing — the ALBM1 host half has to exist before a blob can be believed"; exit 1; }
	@for tool in flac metaflac ffmpeg python3; do \
	  command -v $$tool >/dev/null 2>&1 \
	    || { echo "album-pack-check: '$$tool' is not on PATH — the bake (or its host gate) shells to it"; exit 1; }; \
	done
	python3 scripts/album_bake.py "$(ALBUM)"
	@first=$$(mktemp -d); \
	  cp -r $(ALBUM_ASSETS_DIR) $$first/assets || exit 1; \
	  cp $(ALBUM_ASSETS_RS) $$first/assets.rs || exit 1; \
	  ls $(ALBUM_ASSETS_DIR) > $$first/assets.list || exit 1; \
	  python3 scripts/album_bake.py "$(ALBUM)" >/dev/null || { rm -rf $$first; exit 1; }; \
	  ls $(ALBUM_ASSETS_DIR) > $$first/assets.after.list || exit 1; \
	  cmp -s $$first/assets.list $$first/assets.after.list \
	    || { echo "album-pack-check: the bake is not deterministic — the asset directories differ:"; \
	         echo "  first:  $$(tr '\n' ' ' < $$first/assets.list)"; \
	         echo "  second: $$(tr '\n' ' ' < $$first/assets.after.list)"; rm -rf $$first; exit 1; }; \
	  for f in $$first/assets/*; do \
	    cmp -s "$$f" "$(ALBUM_ASSETS_DIR)/$$(basename $$f)" \
	      || { echo "album-pack-check: the bake is not deterministic — $$(basename $$f) differs between two runs"; rm -rf $$first; exit 1; }; \
	  done; \
	  cmp -s $$first/assets.rs $(ALBUM_ASSETS_RS) \
	    || { echo "album-pack-check: the bake is not deterministic — src/assets.rs differs between two runs"; rm -rf $$first; exit 1; }; \
	  cp $$first/assets.rs $$first/assets.mutant.rs; \
	  python3 -c 'import sys; p = sys.argv[1]; d = bytearray(open(p, "rb").read()); d[0] ^= 0xFF; open(p, "wb").write(d)' $$first/assets.mutant.rs; \
	  cmp -s $$first/assets.mutant.rs $(ALBUM_ASSETS_RS) \
	    && { echo "album-pack-check: the determinism comparison passed two files that differ by one byte — the byte leg is not witnessing anything"; rm -rf $$first; exit 1; } \
	    || echo "  determinism leg: one flipped byte is reported as a difference (negative control)"; \
	  rm -rf $$first
	python3 scripts/album_bake.py --self-test
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_pack_host > $(abspath $(ALBUM_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  cat $(abspath $(ALBUM_GATE_LOG)); \
	  [ $$status -eq 0 ] && grep -q '^test result: ok\.' $(abspath $(ALBUM_GATE_LOG)) \
	    || { echo "album-pack-check: the host suite did not pass — see $(ALBUM_GATE_LOG)"; exit 1; }; \
	  n=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_GATE_LOG))); \
	  [ -n "$$n" ] && [ "$$n" -ge 48 ] || { echo "album-pack-check: collected '$$n' tests, needs >= 48 — a green that names nothing is not a green"; exit 1; }
	@echo "album-pack-check passed: the four §9.0 Phase-1 refusals fired — (a) declared rate != product clock,"
	@echo "  (b) frames outside the encode profile (inherited from gafp_pack.py), (c) ALBM1 n_samples vs the"
	@echo "  container's total_samples, (d) the writer's output re-read through meta.rs — plus two bakes"
	@echo "  byte-identical and the ALBM1 host suite green."

# The app seam's host gate (PR-3a steps ii and iii-A, extended by PR-3b-i): the derived geometry
# pinned to the shipped FLAC figures, the manifest/pin cross-check for every track, both backend arms
# through `PcmSource`, the FLAC arm decoding the fixture's track to its committed `fnv_pcm` pin, and
# the ROM's whole path for that track (`pull_frame` -> i32 -> `convert_frame` -> `fold_bytes`)
# reproducing the bake's `fnv_fifo_l` / `fnv_fifo_r` — the sample-scale proof the ROM step rests on.
#
# PR-3b-i adds the two halves that have nothing to do with audio being improved: the **input seam**
# (edges, two-frame debounce, the autorepeat's exact first-repeat/interval frames, §5.3's control map,
# the BIOS chord being absent by design, and the REPLAY script landing each entry on its own frame and
# no adjacent one) and the **session clock's map** (§4.4's three invariants plus the saturating
# `committed - LEAD_SAMPLES`). Those tests are host tests of the same machine the ROM drives; the
# REPLAY script is replayed at the keys layer because `mgba-test-runner` cannot press a key, so a
# green here is never evidence that a physical press reaches a cartridge (§13 AP-1, AP-7).
#
# PR-3b-ii-B2a adds the **hand-off gain and the pause contract**: a model that drives the real
# `RingFeed`, the real `HandOffGain` and the real `Transport` through scripted pause/resume commands,
# asserting in periods and samples that a fade is monotone to zero and back, that it never touches the
# ring, that a pause freezes the session clock and the ring head, that a resume drops or repeats no
# sample, that three scripted pauses book no underflow, and that a frozen ring refuses a push.
#
# PR-3b-fix1 adds the **post-gain stream witness**, which is the transport's own: the model records
# every byte handed to `rearm_dma` after the gain, and asserts that across a pause/resume the stream is
# the source's, contiguous, differing only inside the two declared ramp windows (and the all-zero
# play-out period the freeze cuts), and that the chunk carrying the ramp is *served* — played out —
# before the freeze. That test fails if the deferral is removed; the ROM's `pcm`/`fifo` folds are a
# decoder/content witness (a cold re-decode) and cannot see any of it, so they are never cited here
# for the transport.
#
# PR-3b-ii-B2b adds **§4.4's skip**: the same machine through `begin_reprime` + `truncate()` + the
# cover, read the way the ROM's wire watch reads it — `to_new = prime + 1` at every prime the probe
# swept (2/4/8/16 → 3/5/9/17 periods), `old = 0` and `underflow_delta = 0` across five skips at a
# nearly-full queue, no starved period inside the truncate window (with the bare truncate shown to book
# one), whole-chunk cover pushes, §4.4's three position invariants across a skip, and the two waits the
# skip log reports: the track's own `head_silence` and the frame loop's `take_to_handoff_bound`. The
# verdict-words leg covers every string the screen can draw (`PASSED`, `FAILED <TERM>` for each of
# `text::TERMS`, and each with the `PAUSED` suffix).
#
# Same shape as `album-pack-check`: neutral cwd, explicit host target, a log, and a collected-test
# floor — a green that names nothing is not a green. The second leg is the stub-only build, which is
# what makes "a stub image links no codec at all" an observed fact rather than a claim about one test
# binary compiled with the default features.
album-seam-test: toolchain-check
	@test $$(find $(ALBUM_CRATE_DIR)/tests -name 'album_seam_host.rs' | wc -l) -eq 1 \
	  || { echo "album-seam-test: $(ALBUM_CRATE_DIR)/tests/album_seam_host.rs is missing — the seam's host half has to exist before a backend arm can be believed"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_seam_host > $(abspath $(ALBUM_SEAM_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  cat $(abspath $(ALBUM_SEAM_GATE_LOG)); \
	  [ $$status -eq 0 ] && grep -q '^test result: ok\.' $(abspath $(ALBUM_SEAM_GATE_LOG)) \
	    || { echo "album-seam-test: the host suite did not pass — see $(ALBUM_SEAM_GATE_LOG)"; exit 1; }; \
	  n=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_SEAM_GATE_LOG))); \
	  [ -n "$$n" ] && [ "$$n" -ge 58 ] || { echo "album-seam-test: collected '$$n' tests, needs >= 58 — a green that names nothing is not a green"; exit 1; }
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) build --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) \
	    --no-default-features --features backend-stub >> $(abspath $(ALBUM_SEAM_GATE_LOG)) 2>&1 \
	    || { cat $(abspath $(ALBUM_SEAM_GATE_LOG)); echo "album-seam-test: the stub-only build failed — a stub image must link no codec at all (--no-default-features --features backend-stub)"; exit 1; }
	@echo "album-seam-test passed: the derived geometry matches the shipped FLAC figures, the manifest"
	@echo "  and its generated pins agree for every track, both backend arms satisfy PcmSource, the shipped"
	@echo "  track decodes to its fnv_pcm pin on the host, its FIFO bytes fold to the bake's fnv_fifo_l/fnv_fifo_r"
	@echo "  as the ROM will push them, a stub-only build links no decoder, the input seam's edges/debounce/"
	@echo "  autorepeat/control-map/chord rules hold and the REPLAY script lands on its own frames, the"
	@echo "  painted position obeys §4.4's three invariants with a saturating lead, the hand-off gain"
	@echo "  and §4.4's pause contract hold: a fade is monotone to silence and back, applied to the DMA slot"
	@echo "  and never the ring, and a pause/resume keeps the position exact with no underflow, and §4.4's"
	@echo "  skip lands the new track at prime+1 periods with no stale audio and no underflow, with the"
	@echo "  hand-off's waits split into the track's own lead-in and the frame loop's bounded share, and a"
	@echo "  track's end advances through the same hand-off minus the truncate while the album's last track"
	@echo "  stops and starts nothing. The deferral is witnessed on the post-gain stream: the ramped chunk is"
	@echo "  served (played out) before the freeze, and across a pause/resume the bytes handed to rearm_dma are"
	@echo "  the source's own, contiguous, with only the two declared ramp windows differing."

# The playback ROM (PR-3a step iii): `ALBM1` + the packed blobs → the app's own chain at 16,384 Hz.
# It boots, verifies its own album (§D-C), plays track 1 end to end through the shipped feeder, and
# prints its `[album-rom]` ledger; like every playback ROM here it ends in `loop {}`.
#
# `album-bake` is a **prerequisite on purpose** (R11, §2.3(4)): a build must not be able to skip the
# pack step and link yesterday's blob, so a stale `albm1.bin`/`assets.rs` cannot reach an image.
# Run it and read the log: `timeout -k 5 600 $(GBA_TEST_RUNNER) $(ALBUM_ROM) > $(ALBUM_ROM_RUN_LOG)`,
# and remember that exit 124 IS the passing result (the ROM reached its `loop {}`). The old 240 s
# was a guess with no recorded headroom — a 30 s clip plus a full cold re-decode and two folds — and
# is now 600 s. The ROM times itself in **CPU cycles** (`gba_audio_hw::CycleCounter`, read before
# the audio chain arms and printed as `boot: parse=…c open=…c gate=…c prefill=…c total=…c`), so the
# image can say what a human waited for; ÷ 16,777,216 is seconds on any GBA. The 600 s leash is a
# backstop for the post-verdict `loop {}`, not a budget line.
#
# This is the **audited** artifact and the one AP-1 runs: the default features include `deep-audit`,
# which is the post-audio content proof (~817,429,485 c) plus the full asset fold (~439,233,721 c) =
# ~75 s of CPU. That is the price of a gate artifact. `make album-player-rom` drops the flag when
# what you want is something to sit in front of, and its verdict line says the terms were SKIPPED.
album-rom: album-bake
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --bin album-play
	agb-gbafix $(ALBUM_ROM_BIN) -o $(ALBUM_ROM)
	@ls -l $(ALBUM_ROM)
	@size -A $(ALBUM_ROM_BIN) | grep -E '^(File|\.text|\.iwram|\.bss)'
	@echo "  run: timeout -k 5 600 $(GBA_TEST_RUNNER) $(ALBUM_ROM) > $(ALBUM_ROM_RUN_LOG) 2>&1"
	@echo "  judge by the log's '[album-rom] verdict' line, never by the exit status: a PASS loops"
	@echo "  forever, so the timeout's exit 124 IS the passing result."

# The same ROM, built without `deep-audit`: a **player** artifact, not a gate artifact. It plays the
# album and runs every always-on witness (the ALBM1 structure fold, the per-track tripwires, the pin
# cross-check, the FLAC boot gate, the live period audit) but skips the cold re-decode's pcm/fifo
# folds and the asset-region fold — ~75 s of CPU that is right for `album-rom` and wrong for a cart
# a human is watching. It says so twice rather than looking like a pass: a `[album-rom] NOTE` line at
# boot, and `pcm=SKIPPED, fifo=SKIPPED, assets=SKIPPED — built without deep-audit` in the verdict.
# `album-bake` stays a prerequisite for the same R11 reason as `album-rom`.
album-player-rom: album-bake
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --bin album-play \
	  --no-default-features --features "backend-flac arm-kernel"
	agb-gbafix $(ALBUM_ROM_BIN) -o $(ALBUM_FAST_ROM)
	@ls -l $(ALBUM_FAST_ROM)
	@size -A $(ALBUM_ROM_BIN) | grep -E '^(File|\.text|\.iwram|\.bss)'
	@echo "  run: timeout -k 5 600 $(GBA_TEST_RUNNER) $(ALBUM_FAST_ROM) 2>&1 | grep '^\[album-rom\]'"
	@echo "  this build does not verify decoded content: pcm/fifo/assets are SKIPPED, by design."

# AP-1's gate (PR-3b-iii-B): the album plays **through** its tracks — two or more visits, every skip,
# every pause, and every track-end advance — and the run's own log is the verdict.
#
# The audited `album-rom` is a HARDWARE build, and a headless run cannot press a key, so the only
# image that can witness a skip, a pause or an advance is the REPLAY arm: the same ROM with
# `input-replay`'s compiled-in key script. It is built WITH `deep-audit` (named explicitly, so a
# change to the crate's default features cannot silently turn this gate into an unaudited one): the
# content proofs are part of the verdict here, not an optional extra.
#
# The verdict is the LOG, never the exit status. A PASS prints its verdict and then loops forever, so
# the leash is what ends the run and **exit 124 IS this gate's pass path** — the run's first
# assertion says so out loud. The leash has real headroom: the measured time-to-verdict for this arm
# is **17.2 s** (18.3 s on the run before it) against 120 s, ~7x slack for a slower host.
#
# The fixture starts on track 1 (`START_TRACK`): track 0 is the 4.3-minute master, and starting there
# would make this run ~1.8x longer for the same proof. The ROM prints `start_track=1` at boot so the
# choice is never read as the album's own start.
#
# Every assertion is a named failure rather than a bare grep, and `content_at=5 periods (expected 5)`
# is `SKIP_PRIME_CHUNKS + 1` = 4 + 1, the cover's depth — written here as the gate's own arithmetic
# instead of read back from the run it is gating. The log's `advance:` rows carry the boundary gap:
# 4 of its periods are the `SKIP_PRIME_CHUNKS` design constant, the rest is counted, so it is
# **derived**, not measured — the echo says so and no comment here calls it measured.
#
# The rows and the latency terms are read, not only the summaries (PR-3b-fix2): every `pause:` row
# must show `gain=0` at the freeze and `dropped=0` across the pause, every latency leg in the
# `input:`/`latency:` lines and in the skip rows is compared with its bound, the ISR bracket must be
# under the 16-sample grace, and the set of `proof track N:` lines is compared with the set of
# `feed: … track=N` visits — a visited track with no proof row, or a proof row for a track nobody
# visited, fails. Each is its own named assertion, so a doctored log names the term it doctored.
#
# **Capacity is a print constraint, not a verdict (R13).** The gate compares a printed row count with
# the census line's `printed=`, never with `occurred=`: a long interactive session can cross more
# visits/skips/boundaries than the log holds, and the rows it kept are each their own witness. The
# three census lines (`visits:`, `skips:`, `advances:`) are required to be present.
#
# **`dropped=0 (ring empty at mark)` is a pass, a bare `dropped=0` is not.** A press taken on the pass
# where a visit's audio has already drained truncates an empty ring and legitimately drops nothing;
# the ROM records that it *performed* the truncate and prints the ring-empty annotation, so the gate
# can tell that from a missing truncate. A track-end advance still requires plain `dropped=0`.
#
# **The pass bound is this run's own measurement, in cycles.** The `input: pass worst=` line's bound
# is checked here to equal the run's worst producer frame + one period (573,440) + the margin
# (280,896), and the skip summary's `take→hand-off<=` to equal this run's measured pass in periods
# plus one. The old fixed 3-period / 6-frame ceilings were emulator measurements (R3) and are gone.
album-test: toolchain-check album-bake
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --bin album-play --features "input-replay deep-audit"
	agb-gbafix $(ALBUM_ROM_BIN) -o $(ALBUM_REPLAY_ROM)
	@status=0; \
	  timeout -k 5 120 $(GBA_TEST_RUNNER) $(ALBUM_REPLAY_ROM) > $(ALBUM_TEST_LOG) 2>&1 || status=$$?; \
	  [ $$status -eq 124 ] \
	    && echo "album-test: the runner ended at the leash (exit 124), which is the pass path — a PASS loops forever" \
	    || echo "album-test: the runner exited $$status, not 124; the log is still the verdict (a refused boot exits early)"; \
	  grep -q '^\[album-rom\] verdict PASSED' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: no 'verdict PASSED' line — the run did not finish, or a gated term failed; see $(ALBUM_TEST_LOG)"; exit 1; }; \
	  grep -q 'ledger=true, audit=true, handoffs=true, pause=true, skip=true, advance=true, cadence=true, isr=true, reaction=true, pcm=true, fifo=true, assets=true' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: a gated verdict term is not true — the verdict line names it; see $(ALBUM_TEST_LOG)"; exit 1; }; \
	  grep -q 'built without deep-audit' $(ALBUM_TEST_LOG) \
	    && { echo "album-test: the gate arm was built without deep-audit — its content proofs were SKIPPED, not passed"; exit 1; } || true; \
	  grep -q 'feed: .*underflows=0' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the ring booked an underflow — the feed line's underflows= is not 0"; exit 1; }; \
	  grep -q 'audit: hw_periods=[0-9]* coalesced=0 late=0' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the period audit refuses the run — coalesced or late is not 0"; exit 1; }; \
	  visits=$$(sed -n 's/.*traversal: visits=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$visits" ] && [ "$$visits" -ge 2 ] \
	    || { echo "album-test: traversal read '$$visits' visits, needs >= 2 — a one-track run never reached a boundary"; exit 1; }; \
	  vprint=$$(sed -n 's/^\[album-rom\] visits: occurred=[0-9][0-9]* printed=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$vprint" ] \
	    || { echo "album-test: the visits census line is missing — the print log's depth is not a verdict and the run must say occurred= vs printed="; exit 1; }; \
	  grep -q '^\[album-rom\] traversal: .* end=ALBUM_END' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the traversal did not reach end=ALBUM_END — it wrapped or stopped early"; exit 1; }; \
	  skips=$$(sed -n 's/.*skip: skips=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$skips" ] && [ "$$skips" -ge 1 ] \
	    || { echo "album-test: skip: skips=$$skips, needs >= 1 — the REPLAY script's skips did not run"; exit 1; }; \
	  grep -q '^\[album-rom\] skip: skips=[1-9][0-9]* .*expected_content_at=5 .*\[PASS\]' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the skip summary is not [PASS] (or expected_content_at is not 5 = SKIP_PRIME_CHUNKS + 1)"; exit 1; }; \
	  soccurred=$$(sed -n 's/^\[album-rom\] skips: occurred=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  sprinted=$$(sed -n 's/^\[album-rom\] skips: occurred=[0-9][0-9]* printed=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$soccurred" ] && [ -n "$$sprinted" ] && [ "$$soccurred" -eq "$$skips" ] \
	    || { echo "album-test: the skips census (occurred='$$soccurred', printed='$$sprinted') disagrees with skips=$$skips — every skip must be counted even when its row does not fit"; exit 1; }; \
	  rows=$$(grep -c '^\[album-rom\] skip: →track' $(ALBUM_TEST_LOG) || true); \
	  [ "$$rows" -eq "$$sprinted" ] \
	    || { echo "album-test: $$rows skip rows for printed=$$sprinted (skips=$$skips occurred) — the log is truncated or a printed row is missing"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] skip: →track' $(ALBUM_TEST_LOG) | grep -Evc 'content_at=5 periods \(expected 5\) old=0 .*underflow_delta=0 .*take→hand-off=[0-9][0-9]* periods \(bound [0-9][0-9]*\) .*dropped=(0 \(ring empty at mark\)|[1-9][0-9]*)' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad skip row(s) with content_at != 5, stale audio (old != 0), an underflow delta, no take->hand-off number with its bound, or a bare dropped=0 — 0 is legal only with the ring-empty annotation, because the gate is that the hand-off performed its truncate"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] skip: →track' $(ALBUM_TEST_LOG) | grep -vc '\[PASS\]' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad skip row(s) are not [PASS]"; exit 1; }; \
	  pairs=$$(sed -n 's/.*pause: pairs=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$pairs" ] && [ "$$pairs" -ge 1 ] \
	    || { echo "album-test: pause: pairs=$$pairs, needs >= 1 — the pause half of the script did not run"; exit 1; }; \
	  grep -q '^\[album-rom\] pause: pairs=[1-9][0-9]* .*\[PASS\]' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the pause summary is not [PASS]"; exit 1; }; \
	  prow=$$(grep -c '^\[album-rom\] pause [0-9]' $(ALBUM_TEST_LOG) || true); \
	  [ "$$prow" -eq "$$pairs" ] \
	    || { echo "album-test: $$prow pause row(s) for pairs=$$pairs — a pair printed no row, or a row is not a pair"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] pause [0-9]' $(ALBUM_TEST_LOG) | grep -Evc 'fade_periods=[0-9][0-9]* gain=0 committed=[0-9][0-9]* ; resume@frame=[0-9][0-9]* committed=[0-9][0-9]* dropped=0 \[PASS\]$$' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad pause row(s) did not reach gain=0 before the freeze or booked dropped!=0 across the pause — the row, not only the summary, is read here"; exit 1; }; \
	  detect=$$(sed -n 's/.*detect_latency max=\([0-9][0-9]*\) frames.*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$detect" ] && [ "$$detect" -le 1 ] \
	    || { echo "album-test: detect_latency max='$$detect' frames, must be <= 1 (DEBOUNCE_FRAMES - 1) — the press->detect leg is outside its bound or the line is missing"; exit 1; }; \
	  take=$$(sed -n 's/.*take_latency max=\([0-9][0-9]*\) frames.*/\1/p' $(ALBUM_TEST_LOG)); \
	  tbound=$$(sed -n 's/.*take_latency max=[0-9][0-9]* frames (bound \([0-9][0-9]*\) = this run.s worst main-loop pass by latch).*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$take" ] && [ -n "$$tbound" ] \
	    || { echo "album-test: the take_latency line is missing a number (max='$$take', bound='$$tbound') — the ISR->take leg is not measured"; exit 1; }; \
	  [ "$$take" -le "$$tbound" ] \
	    || { echo "album-test: take_latency max=$$take frames is past its bound of $$tbound (the run's own worst main-loop pass by latch) — a press waited more than one pass"; exit 1; }; \
	  pworst=$$(sed -n 's/.*input: pass worst=\([0-9][0-9]*\)c .*/\1/p' $(ALBUM_TEST_LOG)); \
	  pframe=$$(sed -n 's/.*producer frame worst=\([0-9][0-9]*\)c .*/\1/p' $(ALBUM_TEST_LOG)); \
	  pbound=$$(sed -n 's/.*bound=\([0-9][0-9]*\)c .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$pworst" ] && [ -n "$$pframe" ] && [ -n "$$pbound" ] \
	    || { echo "album-test: the pass-bound line is missing a number (pass='$$pworst', frame='$$pframe', bound='$$pbound') — the emulator-derived ceiling must be replaced by a measured one, not dropped"; exit 1; }; \
	  [ "$$pbound" -eq $$((pframe + 573440 + 280896)) ] \
	    || { echo "album-test: the pass bound $$pbound c is not this run's own producer frame $$pframe c + one period (573440) + the margin (280896) — a bound with an emulator constant in it is exactly what R13 removed"; exit 1; }; \
	  [ "$$pworst" -le "$$pbound" ] \
	    || { echo "album-test: the worst pass $$pworst c is past its own measured bound $$pbound c"; exit 1; }; \
	  pp=$$(( (pworst + 573439) / 573440 )); [ "$$pp" -ge 1 ] || pp=1; \
	  sbound=$$(sed -n 's/^\[album-rom\] skip: skips=[0-9][0-9]* .*take→hand-off<=\([0-9][0-9]*\) periods.*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$sbound" ] && [ "$$sbound" -eq $$((pp + 1)) ] \
	    || { echo "album-test: the skip bound $$sbound periods is not this run's measured pass ($$pp periods) + 1 — a fixed bound is the emulator gate this round removed"; exit 1; }; \
	  bnd=$$(sed -n 's/.*action@next-period max=\([0-9][0-9]*\) frames.*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$bnd" ] && [ "$$bnd" -le 3 ] \
	    || { echo "album-test: action@next-period max='$$bnd' frames, must be <= 3 (one period is 2.04 video frames)"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] skip: →track' $(ALBUM_TEST_LOG) | grep -Evc 'take→hand-off=[0-9][0-9]* periods \(bound [0-9][0-9]*\)' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad skip row(s) whose row does not carry both its take->hand-off and the run's own bound"; exit 1; }; \
	  isr=$$(sed -n 's/.*vblank ISR worst=\([0-9][0-9]*\) samples.*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$isr" ] && [ "$$isr" -lt 16 ] \
	    || { echo "album-test: the ISR line's worst='$$isr' samples, must be < 16 (the 16-sample re-arm grace) — a handler that long delays the feeder past the DMA's reload"; exit 1; }; \
	  grep -q '^\[album-rom\] input: vblank ISR worst=[0-9][0-9]* samples = [0-9][0-9]*c over [0-9][0-9]* frames (grace limit 16 samples) \[PASS\]' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the ISR bracket line is missing or not [PASS] against its 16-sample grace"; exit 1; }; \
	  visited=$$(sed -n 's/^\[album-rom\] feed: visit=[0-9][0-9]* track=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG) | sort -n -u | tr '\n' ' '); \
	  proved=$$(sed -n 's/^\[album-rom\] proof track \([0-9][0-9]*\): .*/\1/p' $(ALBUM_TEST_LOG) | sort -n -u | tr '\n' ' '); \
	  [ -n "$$visited" ] && [ "$$visited" = "$$proved" ] \
	    || { echo "album-test: the proof's track set '$$proved' != the visited set '$$visited' — a visited track has no proof row (MATCH, an aliased blob, or the named SKIPPED) or a never-visited track has one"; exit 1; }; \
	  vrows=$$(grep -c '^\[album-rom\] feed: visit=' $(ALBUM_TEST_LOG) || true); \
	  [ "$$vrows" -eq "$$vprint" ] \
	    || { echo "album-test: $$vrows feed visit row(s) for printed=$$vprint (visits=$$visits occurred) — a printed visit is missing from the ledger the proof's track set is compared with"; exit 1; }; \
	  advances=$$(sed -n 's/.*advance: advances=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  [ -n "$$advances" ] && [ "$$advances" -ge 1 ] \
	    || { echo "album-test: advance: advances=$$advances, needs >= 1 — the album never crossed a track boundary"; exit 1; }; \
	  aprint=$$(sed -n 's/^\[album-rom\] advances: occurred=[0-9][0-9]* printed=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  arows=$$(grep -c '^\[album-rom\] advance: →track' $(ALBUM_TEST_LOG) || true); \
	  [ -n "$$aprint" ] && [ "$$arows" -eq "$$aprint" ] \
	    || { echo "album-test: $$arows advance rows for printed=$$aprint (advances=$$advances occurred) — a printed boundary has no row"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] advance: →track' $(ALBUM_TEST_LOG) | grep -Evc 'content_at=5 periods old=0 .*underflow_delta=0 .*dropped=0 ' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad advance row(s) with content_at != 5 (= SKIP_PRIME_CHUNKS + 1), stale audio (old != 0), an underflow delta, or a truncate (dropped != 0 — a drained ring has nothing to drop)"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] advance: →track' $(ALBUM_TEST_LOG) | grep -vc '\[PASS\]' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad advance row(s) are not [PASS]"; exit 1; }; \
	  grep -q 'pcm_fnv=.*\[MATCH\]' $(ALBUM_TEST_LOG) && grep -q 'fifo L=.*\[MATCH\]' $(ALBUM_TEST_LOG) \
	    && grep -q 'audit: assets=[0-9]*c \[MATCH\]' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: a content proof is not MATCH (pcm, fifo or the asset fold)"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] proof' $(ALBUM_TEST_LOG) | grep -v '\[MATCH\]\|same packed blob\|redecode=\|^\[album-rom\] proof: tracks folded=' | grep -vc 'SKIPPED — 2069 frames is past the audited fold' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad proof line(s) are neither MATCH, an aliased blob, nor the named track-2 fold cap"; exit 1; }; \
	  skipped=$$(grep -c 'SKIPPED' $(ALBUM_TEST_LOG) || true); \
	  [ "$$skipped" -eq 1 ] \
	    || { echo "album-test: the log holds $$skipped SKIPPED line(s), expected exactly the track-2 fold cap — a leg was skipped, not passed"; exit 1; }; \
	  grep -q "^\[album-rom\] proof track 2: SKIPPED — 2069 frames is past the audited fold's cap" $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the one permitted SKIPPED is not the track-2 fold-cap leg — a new leg is passing unnoticed"; exit 1; }; \
	  n=$$(grep -c '^\[album-rom\]' $(ALBUM_TEST_LOG) || true); \
	  [ "$$n" -ge 60 ] \
	    || { echo "album-test: the log holds $$n '[album-rom]' lines, needs >= 60 — an empty or truncated log is not a pass"; exit 1; }
	@grep -E '^\[album-rom\] (traversal|visits: |skip: skips|skips: |pause: pairs|pause [0-9]|advance: |advances: |input: edges|input: pass|input: take_latency|latency: |input: vblank ISR)' $(ALBUM_TEST_LOG)
	@echo "album-test passed: the album played through to its end with every gated term true — underflows=0,"
	@echo "  coalesced=0, late=0, every skip's cover 5 periods with old=0 (the weaker witness: at zero gain"
	@echo "  a stale chunk would also read 0) and the performed truncate as its own witness (dropped=0 is legal only"
	@echo "  with the ring-empty annotation), every track-end advance at SKIP_PRIME_CHUNKS+1 with old=0, dropped=0 and"
	@echo "  underflow_delta=0, every content proof MATCH (the only"
	@echo "  SKIPPED leg is the track-2 fold cap, named in the gate), and the proof's track set equal to the visited set."
	@echo "  Every pause ROW is read, not only its summary: gain=0 at the freeze and dropped=0 across the pause — both"
	@echo "  arms of a clock-delta tautology, so they show the chain stopped, not that no sample was lost at the wire."
	@echo "  Every latency leg is read and inside its bound: press->detect <= 1 frame, detect->take <= the run's own"
	@echo "  worst main-loop pass (by latch), action@next-period <= 3 frames, the pass bound = this run's measured"
	@echo "  producer frame + one period + one vblank, a skip's take->hand-off <= this run's measured pass + 1 period,"
	@echo "  and the ISR bracket < 16 samples of the audio clock. Printed-row counts are checked against the census"
	@echo "  lines' printed=, not against occurred= — the log's depth is not a verdict (R13). The advance rows"
	@echo "  above carry the derived track-boundary gap (4 of its periods are the SKIP_PRIME_CHUNKS constant; the rest is"
	@echo "  counted). Verdict: $(ALBUM_TEST_LOG)"

# ===========================================================================
# Aggregates
# ===========================================================================

# The ROM's own #[test_case] suite. KNOWN STATE, not this run's bug: the root
# crate currently COLLECTS ZERO CASES and exits 0, so a green `test-rom` is
# evidence that build → link → boot works, nothing more. (The repo's first ROM
# suite with assertions in it is `make overlay-kernel-test`.)
test-rom: toolchain-check
	CARGO_TARGET_$(TARGET_ENV)_RUNNER=$(GBA_TEST_RUNNER) \
	  $(CARGO) test --target $(TARGET)

test: test-rom flac-test uli-test uli-overlay-test
	@echo "test passed: ROM suite + flac-lite isolation gates + uli-lite host corpus + uli-lite-gba twin"

# Everything a FLAC-track change owes. `flac-gates` is the core, `overlay-gates`
# is the kernel, `product-play-test` is the geometry the playback claim stands on,
# `playback-test` is the lifted ring/conversion suite (§2.2), and `spike-test` is
# the instrument.
flac-check: flac-gates overlay-gates product-play-test playback-test spike-test
	@echo "flac-check passed: core configurations, overlay/kernel witnesses, product geometry, playback data path, instrument"

# ===========================================================================
# Environment aliases
# ===========================================================================
#
# `native-<target>` says "build <target> on this host" out loud; `podman-<target>`
# re-invokes the same bare target inside the image, which carries the whole
# toolchain (nightly + rust-src + rustfmt + agb-gbafix + mgba-test-runner) so the
# host needs nothing. Both produce the same artifact; nothing here branches on
# which environment it is in.
# The two containerized suites that need the emulator get the `tester` stage.
podman-test podman-test-rom: PODMAN_STAGE := tester

native-%: %
	@echo "native-$*: done"

podman-%:
	$(CONTAINER) build --target $(PODMAN_STAGE) -t $(IMAGE)-$(PODMAN_STAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-$(PODMAN_STAGE) make $*
	@echo "podman-$*: done in $(CONTAINER) ($(PODMAN_STAGE) stage)"

# ===========================================================================
# Housekeeping
# ===========================================================================

# Clean, in two depths, because the two things a build leaves behind have
# completely different prices:
#
#   * build OUTPUTS — ROMs, saves, generated assets, run logs, `__pycache__`. Cheap
#     to reproduce, and the thing you actually mean by "tidy the tree".
#   * compiler CACHES — the fifteen workspaces' `target/` dirs, ~4.5 GB here. Not
#     free to lose: the Tier-3 target has no prebuilt `core`, so every one of them
#     re-runs `-Zbuild-std`. Measured on this host for the full verified set
#     (`check` + seven ROMs + `flac-check`): 135 s cold against 22 s warm.
#
# So `clean` takes the outputs and keeps the caches, and `clean-deep` (or
# `make clean DEEP=1`) takes both. Splitting them is also what makes the two
# hand-written lists this target used to be unnecessary:
#
#  1. **Workspaces are discovered, not listed.** The old `clean` named six crate
#     directories while the tree held fifteen workspaces, so nine `target/` dirs
#     survived a "clean" (~2 GB of the 4.5). That is the failure `.gitignore`
#     argues about for ROM names — "a rule that has to be extended for each new
#     build is a rule that will be missed" — so the list comes from the manifests
#     on disk. `cargo clean` in a workspace *member* resolves to its root's target
#     dir, so a member in this list is a harmless no-op, not a second clean.
#  2. **ROMs and saves go by pattern.** Name lists are exactly what left the
#     retired instruments' images (`flac-spike-6a7.gba`, `flac-rate-*.gba`,
#     `arm-kernels.gba`, `e5-fetch.gba`, …) in the repository root after a "clean".
#     No `.gba`/`.sav` is tracked (`.gitignore` says why), so removing them all
#     cannot destroy source.
#
# Nothing here touches tracked generator output — `examples/flac_spike/assets/*.bin`
# and `src/assets.rs`, the committed `GAFP` blob, the test fixtures — which the
# build reads as input and regenerates only through their own generators. The Album
# Player's assets are the other class, not this one: they belong to whichever album
# was built, so they are generated from a tracked fixture config and gitignored
# (ALBUM-PLAYER.md §2.3) — `albm1.bin`, `assets/*.gfp` and `src/assets.rs` are
# removed below, and `album.toml` stays because it is tracked source.
clean: clean-outputs
	@if [ -n "$(DEEP)" ]; then \
	  echo "clean: DEEP is set — dropping every workspace's target/ as well"; \
	  $(MAKE) -s clean-caches; \
	else \
	  echo "clean: build outputs removed, compile caches KEPT (~4.5 GB here)."; \
	  echo "       full rebuild cost if you do want them gone: 'make clean-deep'"; \
	fi

# The outputs: every ROM and emulator save, the generated master asset + its pin
# (regenerate with `make master-pack INPUT=…`), the archived 10,512 Hz rate flow's
# staged clips and modules, the ULI2 harness's per-run log (`make uli-bench` rewrites
# it; the numbers that matter live in ULI2.md), and python's bytecode caches.
clean-outputs:
	rm -f *.gba *.sav
	@rm -f $(MASTER_ASSET) $(MASTER_PIN)
	@rm -f $(SPIKE_CRATE_DIR)/src/rate_assets.rs $(SPIKE_CRATE_DIR)/src/rate_pcm.rs
	@rm -f $(SPIKE_CRATE_DIR)/assets/rate_*.bin
	@rm -f uli-bench*.log uli-attrib*.log
	@rm -f album-probe*.log
	@rm -f $(ALBUM_BLOB) $(ALBUM_ASSETS_DIR)/*.gfp $(ALBUM_ASSETS_RS) $(ALBUM_GATE_LOG) $(ALBUM_SEAM_GATE_LOG) $(ALBUM_ROM_RUN_LOG) $(ALBUM_TEST_LOG)
	@find . -name __pycache__ -type d -not -path './.git/*' -prune -exec rm -rf {} + 2>/dev/null || true

# The caches: the fifteen workspaces' `target/` dirs, discovered from the tree.
clean-caches:
	@for d in $(WORKSPACE_DIRS); do (cd $$d && $(CARGO) clean) || exit 1; done

clean-deep: clean-outputs clean-caches
	@echo "clean-deep: outputs AND every workspace's target/ are gone; the next build"
	@echo "            pays the -Zbuild-std core/alloc compile again (135 s / 4.5 GB here)."

# Drop the container build-cache volumes. Safe: they are pure build caches, so
# the next container run rebuilds slowly once and repopulates them. Does NOT
# touch ./target.
clean-cache:
	-$(CONTAINER) volume rm $(CARGO_CACHE) $(FLAC_CACHE) $(FLACROM_CACHE) $(SPIKE_CACHE) $(OPUS_CACHE) $(OVERLAY_CACHE) $(OVERLAY_ROM_CACHE) 2>/dev/null || true
	@echo "container build-cache volumes removed."

check:
	@for d in $(WORKSPACES); do (cd $$d && $(CARGO) fmt --check) || exit 1; done
	CARGO='$(CARGO)' sh scripts/fmt_with_generated_stub.sh $(FLAC_CRATE_DIR) --check
	CARGO='$(CARGO)' GENERATED='$(abspath $(ALBUM_ASSETS_RS))' sh scripts/fmt_with_generated_stub.sh $(ALBUM_CRATE_DIR) --check
	@echo "check: all workspaces rustfmt-clean"

# Fail early and legibly when the pinned toolchain is not reachable, instead of
# half-running a gate (e.g. `--target ''` when the host triple cannot be probed).
# The second half is the one this file cannot answer for itself: is the instance
# this build runs ALSO the one rustup hands to a bare cargo, an editor or
# rust-analyzer in this tree? Two instruments in one tree is the exact failure the
# pin exists to prevent, so it is checked by asking rustup, not by pattern-guessing
# at the pin file's format.
toolchain-check:
	@$(RUSTC) -V >/dev/null 2>&1 || { \
	  echo "error: cannot run '$(RUSTC)'. This build needs rustup + the '$(TOOLCHAIN)' toolchain:"; \
	  echo "  rustup toolchain install $(TOOLCHAIN)"; \
	  echo "  rustup component add rust-src rustfmt --toolchain $(TOOLCHAIN)"; \
	  echo "(no rustup at all? override: make $(MAKECMDGOALS) CARGO=cargo RUSTC=rustc)"; \
	  exit 1; }
	@test -n "$(HOST_TRIPLE)" || { \
	  echo "error: could not read the host triple from '$(RUSTC) -vV'"; exit 1; }
ifeq ($(HAS_RUSTUP),1)
ifneq ($(wildcard $(RUST_TOOLCHAIN_FILE)),)
ifeq ($(TOOLCHAIN),$(PINNED_TOOLCHAIN))
	@bld=$$($(RUSTC) -V 2>/dev/null); \
	actfull=$$($(RUSTUP) show active-toolchain 2>/dev/null | head -1); \
	act=$${actfull%% *}; got=$$($(RUSTUP) run "$$act" rustc -V 2>/dev/null); \
	if [ -n "$$act" ] && [ -n "$$got" ] && [ "$$bld" != "$$got" ]; then \
	  echo "error: this build runs '$$bld' (from $(TOOLCHAIN), read out of $(RUST_TOOLCHAIN_FILE)),"; \
	  echo "  but rustup resolves this tree to '$$act', which runs '$$got'."; \
	  echo "  rustup is not taking a channel from the pin file — a commented-out or mis-sectioned channel line makes it fall back to the ambient default silently; a RUSTUP_TOOLCHAIN environment override does the same."; \
	  echo "  fix the tree, not the build: the instance cargo and your editor see must be the string that builds the ROM."; \
	  exit 1; \
	fi
else
	@echo "notice: TOOLCHAIN=$(TOOLCHAIN) overrides the pinned instance ($(PINNED_TOOLCHAIN)); this build is not the instrument $(notdir $(RUST_TOOLCHAIN_FILE)) declares, so the rustup cross-check is skipped. Say so in the record."
endif
endif
endif

help:
	@echo "gba-sound-player — build targets"
	@echo ""
	@echo "  Baseline ROM:          native-rom | podman-rom   -> $(ROM)"
	@echo ""
	@echo "  FLAC happy path (16,384 Hz, FIXED/16-bit/2048-sample frames):"
	@echo "    make master-pack     INPUT=track.wav     -> $(MASTER_ASSET) + generated pin"
	@echo "    make master-play-rom INPUT=track.wav     -> $(MASTER_PLAY_ROM) (playable)"
	@echo "    make native-product-play-rom               -> $(PRODUCT_PLAY_ROM) (committed asset)"
	@echo "    make native-flac-rom                       -> $(FLAC_ROM) (decode/verify only)"
	@echo "    make native-spike-rom                      -> $(SPIKE_ROM) (measurement instrument)"
	@echo "  A playback ROM's run ends in loop {}: with mgba-test-runner, exit 124 IS the pass."
	@echo ""
	@echo "  FLAC gates:"
	@echo "    flac-check           EVERYTHING a FLAC change owes (the five below)"
	@echo "    flac-gates           every crates/flac-lite configuration + gafp-check + gafp-pack-check"
	@echo "    overlay-gates        crates/flac-lite-gba + the ARM kernel's mGBA witness"
	@echo "    product-play-test    the product geometry's host model"
	@echo "    playback-test        the playback data path's host suite (ring, truncate, FNV folds)"
	@echo "    spike-test           the instrument's host witness (bit-exact vs flac -d)"
	@echo "    (individually: flac-test, flac-test-wide/-crc/-scan/-rice, flac-portable-check,"
	@echo "     gafp-check, gafp-pack-check, overlay-check/-test/-check-arm/-test-arm,"
	@echo "     overlay-objdump, overlay-kernel-rom, overlay-kernel-test)"
	@echo ""
	@echo "  Other tracks:"
	@echo "    ULI2 (ULI2.md — the repository's product codec): uli-test, uli-overlay-test,"
	@echo "      uli-check, uli-fixture, native-uli-spike-rom, native-uli-bench-rom,"
	@echo "      uli-bench (headless run + PASS/FAIL read from the log), uli-kernel-twin"
	@echo "    ULI2 attribution (timing-only; pins fail by design): uli-attrib,"
	@echo "      uli-attrib-subframe, uli-frame-ops"
	@echo "    ULI2 PR E (album corpus, witness only — needs ffmpeg + the sibling gba-audio checkout):"
	@echo "      uli-album-asset (blob + C-sourced pin, both gitignored), uli-album-bench (the tail),"
	@echo "      uli-gate (the gate total: decode + convert + IRQ + DMA)"
	@echo "    Opus (OPUS.md): native-opus-rom, opus-test"
	@echo "    Album Player, Phase 0 (AP-0 — the probe aggregate; nothing here is"
	@echo "      wired into flac-check or test): album-probe-rom (three probe ROMs),"
	@echo "      album-probe (run all three headless; PASS read from each log),"
	@echo "      album-probe-art (regenerate the tracked ART1 blob + pin),"
	@echo "      album-probe-test (the AP-0 host gate)."
	@echo "    Album Player, Phase 1 (pack step + the app seam; ALBUM-PLAYER.md §9.0 A3a, §4.2b):"
	@echo "      album-bake (album.toml -> assets/*.gfp + ALBM1 + src/assets.rs),"
	@echo "      album-pack-check (two byte-identical bakes, the four refusals, the ALBM1 host gate),"
	@echo "      album-seam-test (the seam's host gate: derived geometry vs the shipped figures, both"
	@echo "      backend arms, the FLAC arm's fnv_pcm and fnv_fifo_l/fnv_fifo_r pins, and a stub-only"
	@echo "      build with no decoder linked),"
	@echo "      album-rom -> $(ALBUM_ROM) (bake + the audited ROM: all five verdict terms true; run it"
	@echo "      with mgba-test-runner and read the [album-rom] ledger — exit 124 IS the pass),"
	@echo "      album-player-rom -> $(ALBUM_FAST_ROM) (the same ROM built without deep-audit: no cold"
	@echo "      re-decode, no asset fold, and the verdict says pcm/fifo/assets=SKIPPED),"
	@echo "      album-test -> $(ALBUM_REPLAY_ROM) (AP-1's gate: the REPLAY arm run headless to the album's"
	@echo "      end; every gated term, skip, pause and track-end advance asserted from $(ALBUM_TEST_LOG),"
	@echo "      exit 124 being the pass path)."
	@echo "  Tests/format:  test (test-rom + flac-test + uli-test), test-rom, check, format"
	@echo "  Housekeeping:  clean (outputs only; caches kept), clean-deep (+ every"
	@echo "                 workspace's target/), clean-cache (container volumes),"
	@echo "                 toolchain-check, help"
	@echo ""
	@echo "  Any target above also runs as podman-<target> (in the image; needs a"
	@echo "  container runtime) and native-<target> (this host). Builds run 'format'"
	@echo "  first and never gate on tests; tests never reformat; only 'check' fails"
	@echo "  on formatting."
	@echo ""
	@echo "  Toolchain: $(TOOLCHAIN) via $(RUSTC)"
	@echo "  Container: $(CONTAINER)  image: $(IMAGE)  Host triple: $(HOST_TRIPLE)"
