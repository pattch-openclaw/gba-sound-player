.PHONY: format build rom flac-rom product-play-rom master-pack master-play-rom \
        spike-rom spike-test \
        overlay-check overlay-test overlay-check-arm overlay-test-arm \
        overlay-objdump overlay-kernel-rom overlay-kernel-test overlay-gates \
        test test-rom flac-test flac-test-wide flac-test-crc flac-test-scan \
        flac-test-rice flac-gates flac-portable-check gafp-check gafp-pack-check \
        product-play-test opus-test opus-rom uli-test uli-check uli-fixture \
        uli-bench-rom uli-bench uli-overlay-test \
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

# ===========================================================================
# format / build / baseline ROM
# ===========================================================================
#
# Builds auto-format (all workspaces) so experimental code lands rustfmt-clean;
# `check` is the strict non-writing gate. Builds never depend on tests, and tests
# never reformat — a CI `make test` must not mask drift that `make check` exists
# to fail on.

WORKSPACES := . crates/flac-lite crates/flac-lite-gba examples/flac_spike \
              examples/overlay-kernel-check examples/opus_spike \
              crates/uli-lite crates/uli-lite-gba examples/uli_spike examples/uli_bench

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

# The overlay's host twin: the ARM-mode, unchecked kernels must be bit-exact with
# uli-lite's portable reference. This is what licenses the bench's overlay arm to be
# called a realisation of the same arithmetic.
uli-overlay-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_OVERLAY_MANIFEST) --target $(HOST_TRIPLE)
	@echo "uli-overlay-test passed: overlay kernels and the full decoder are bit-exact with the portable reference"

# Regenerate the committed expected-vector file. This WRITES a fixture, so it is
# a maintenance command and never part of `make test`.
uli-fixture: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) run --manifest-path $(ULI_CRATE_MANIFEST) --target $(HOST_TRIPLE) --example gen_expected -- \
	    $(abspath crates/uli-lite/tests/fixtures/uli2_tiny.uli) \
	    $(abspath crates/uli-lite/tests/fixtures/uli2_tiny_expected.txt)

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
# is the kernel, `product-play-test` is the geometry the playback claim stands on.
flac-check: flac-gates overlay-gates product-play-test spike-test
	@echo "flac-check passed: core configurations, overlay/kernel witnesses, product geometry, instrument"

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
# build reads as input and regenerates only through their own generators.
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
	@rm -f uli-bench*.log
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
	@echo "    flac-check           EVERYTHING a FLAC change owes (the four below)"
	@echo "    flac-gates           every crates/flac-lite configuration + gafp-check + gafp-pack-check"
	@echo "    overlay-gates        crates/flac-lite-gba + the ARM kernel's mGBA witness"
	@echo "    product-play-test    the product geometry's host model"
	@echo "    spike-test           the instrument's host witness (bit-exact vs flac -d)"
	@echo "    (individually: flac-test, flac-test-wide/-crc/-scan/-rice, flac-portable-check,"
	@echo "     gafp-check, gafp-pack-check, overlay-check/-test/-check-arm/-test-arm,"
	@echo "     overlay-objdump, overlay-kernel-rom, overlay-kernel-test)"
	@echo ""
	@echo "  Other tracks:"
	@echo "    ULI2 (ULI2.md — the repository's product codec): uli-test, uli-overlay-test,"
	@echo "      uli-check, uli-fixture, native-uli-spike-rom, native-uli-bench-rom,"
	@echo "      uli-bench (headless run + PASS/FAIL read from the log)"
	@echo "    Opus (OPUS.md): native-opus-rom, opus-test"
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
