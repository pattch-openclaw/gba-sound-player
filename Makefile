.PHONY: format build rom native-rom podman-rom flac-rom native-flac-rom podman-flac-rom \
        spike-rom native-spike-rom podman-spike-rom \
        rate-assets rate-test rate-rom native-rate-rom podman-rate-rom \
        rate-play-rom native-rate-play-rom \
        opus-rom native-opus-rom podman-opus-rom \
        kernel-rom native-kernel-rom podman-kernel-rom \
        test test-rom flac-test flac-test-crc flac-test-scan flac-test-rice flac-gates flac-portable-check spike-test opus-test kernel-test overlay-check overlay-test \
        podman-test podman-test-rom podman-flac-test podman-opus-test podman-kernel-test podman-overlay-test \
        clean clean-cache check podman-check help toolchain-check

# ---------------------------------------------------------------------------
# Build settings (all overridable, e.g. `make podman-rom CONTAINER=docker`)
#
# nightly is REQUIRED: -Zbuild-std (in .cargo/config.toml) is a nightly-only
# flag, used to compile core/alloc from source for this Tier-3 target. See
# Dockerfile/README. Do NOT try to `rustup target add thumbv4t-none-eabi` — it
# has no prebuilt artifacts on any toolchain.
#
# WHICH nightly is pinned in rust-toolchain.toml. Read the block below: the file
# alone does NOT pin `make`, so the channel is read out of that file here.
# ---------------------------------------------------------------------------
# Toolchain resolution — deliberately NOT `cargo +nightly`.
#
# `cargo +nightly` only works when the `cargo` on PATH is the **rustup shim**.
# Conda (the `(base)` prompt) and some distro/Homebrew setups put a *plain*
# cargo first on PATH; that binary treats `+nightly` as a subcommand and dies:
#   error: no such command: `+nightly`
#   help: invoke `cargo` through `rustup` to handle `+toolchain` directives
#
# `rustup run <toolchain> cargo` pins the toolchain through rustup no matter
# which cargo (or none) is on PATH — so the same Makefile works in a conda
# shell, a bare shell, and inside the container. Override if you truly have no
# rustup: `make check CARGO=cargo RUSTC=rustc` (needs the pinned nightly as the
# default — see rust-toolchain.toml).
#
# ---------------------------------------------------------------------------
# The channel is PINNED in ONE place — rust-toolchain.toml — and read here.
#
# Why the file is not sufficient on its own: a toolchain file is consulted when a
# bare `cargo`/`rustc` runs in the tree, but `rustup run <name>` SELECTS a
# toolchain explicitly and therefore OVERRIDES it. Measured on this host: inside a
# directory whose rust-toolchain.toml says nightly-2026-09-04, plain `rustc -V`
# follows the file, while `rustup run stable rustc -V` prints stable 1.99.1. This
# Makefile drives every build through `rustup run $(TOOLCHAIN)`, so a channel that
# lived only in the file would not pin `make` at all.
#
# So: rust-toolchain.toml is the pin and the thing that pins bare cargo/rustc,
# rust-analyzer and editors; TOOLCHAIN below is read out of it so that `make`
# honours the same string rather than a second copy of it. `nightly` is the
# fallback only if that file is absent (e.g. a partial checkout) — a fallback that
# is deliberately loud: `make help` prints the resolved toolchain, and
# `toolchain-check` fails fast naming the channel it wants.
#
# The read itself uses make's own file function rather than a sed pipeline (the
# Dockerfile runs this same expression through `make --eval`, so host and image
# do not merely share a string — they share one rule for reading it). Three
# layers, each doing what only it can: a version guard and a match-count guard at
# parse time, on every target (below), and a cross-check against rustup's own
# resolution in `toolchain-check`, which every measurement-bearing target already
# depends on — because the file's *format* is not the property that matters; "the
# instance this build runs" and "the instance rustup/cargo/your editor pick in this
# tree" being the same instance is.
# ---------------------------------------------------------------------------
RUSTUP ?= rustup
RUST_TOOLCHAIN_FILE := $(dir $(lastword $(MAKEFILE_LIST)))rust-toolchain.toml

# Reading the pin with no shell in the path: `$(file < X)` is make's own file read,
# and the channel is the one word in that file wrapped end-to-end in double quotes,
# so `$(filter "%", …)` selects exactly it and `$(subst)` drops the quotes. No sed,
# no pipeline, no second copy of the string that could drift.
#
# What this read is NOT, and where each gap is closed:
#   - It is a word-pattern match, not a TOML parse. Multi-word double-quoted phrases
#     in that file's prose are safe (their words are only half-wrapped), but a
#     single-word quoted term would match too — so the number of matches is counted,
#     never trusted, and a wrong count stops the build naming what it found. (The
#     canonical form of the pin file is load-bearing here: a space before the `]` of
#     its `components` array would make `"rustfmt"` a standalone quoted word.)
#   - It cannot tell a comment from a line. rustup can, and is silent about it:
#     measured, `# channel = "nightly-2026-09-04"` makes rustup resolve the tree to
#     the ambient default (exit 0) while this read still returns 2026-09-04. That
#     one is caught by the rustup cross-check in `toolchain-check`, not by guessing
#     at comment spellings here.
#   - `$(file <)` does not exist before GNU make 4.2, and Apple still ships 3.81 at
#     /usr/bin/make. Guarded by name below, so an old make is told which make to use
#     rather than being told its pin file is malformed.
ifndef MAKE_VERSION
$(error reading the pinned channel uses make's own file function, $$(file <), which needs GNU make 4.2 or newer. This make is $(MAKE) and reports no MAKE_VERSION, so it is not GNU make. On GNU/Linux the system make is fine. On macOS install GNU make from Homebrew and run gmake, or build in the container with make podman-rom.)
endif
ifneq ($(filter 3.% 4.0 4.1,$(MAKE_VERSION)),)
$(error reading the pinned channel uses make's own file function, $$(file <), which needs GNU make 4.2 or newer — this is GNU make $(MAKE_VERSION). Linux system makes are normally new enough. macOS ships 3.81 at /usr/bin/make: install GNU make from Homebrew and run gmake, or build in the container with make podman-rom.)
endif

PINNED_QUOTED := $(filter "%", $(file < $(RUST_TOOLCHAIN_FILE)))
PINNED_TOOLCHAIN := $(subst ",,$(PINNED_QUOTED))
# Try a different instance for one command: `make flac-test TOOLCHAIN=nightly-2026-09-11`
# (then say so in the record — FLAC.md "Baselines to hold", the instance rule).
TOOLCHAIN ?= $(if $(PINNED_TOOLCHAIN),$(PINNED_TOOLCHAIN),nightly)

# A pin file that EXISTS but does not yield exactly one quoted value is a silent
# instrument downgrade — the failure mode this file exists to prevent. Refuse rather
# than fall back: the `nightly` fallback below is for a MISSING file (a partial
# checkout, or a tool invoked outside the tree). These guards fire on any `make`,
# including `make help`, so a malformed pin cannot be missed.
ifneq ($(words $(PINNED_QUOTED)),1)
ifneq ($(wildcard $(RUST_TOOLCHAIN_FILE)),)
$(error $(RUST_TOOLCHAIN_FILE) — expected exactly one double-quoted word, the channel, and found $(words $(PINNED_QUOTED))$(if $(PINNED_QUOTED), which were $(PINNED_QUOTED),). Either the channel line is missing or unquoted, or the prose in that file contains a quoted single word. Refusing to build on a moving channel.)
endif
endif

# Detect rustup instead of assuming the `cargo` on PATH is the rustup shim:
#   - rustup present  -> pin through rustup (correct even when a non-shim cargo
#                        from conda/Homebrew shadows it on PATH)
#   - no rustup at all -> plain cargo/rustc, which must already BE nightly
#                        (override with CARGO=/ RUSTC=/ TOOLCHAIN= as needed)
HAS_RUSTUP := $(shell command -v $(RUSTUP) >/dev/null 2>&1 && echo 1)
ifeq ($(HAS_RUSTUP),1)
CARGO ?= $(RUSTUP) run $(TOOLCHAIN) cargo
RUSTC ?= $(RUSTUP) run $(TOOLCHAIN) rustc
else
CARGO ?= cargo
RUSTC ?= rustc
endif
# Container runtime: podman by default, override with `make podman-rom CONTAINER=docker`.
CONTAINER ?= podman
# Container image. ONE image serves builds AND tests: it carries nightly +
# rust-src + rustfmt + agb-gbafix + mgba-test-runner, so the containerized
# path needs no host tooling whatsoever (see Dockerfile).
IMAGE ?= gba-builder
# Persistent volumes for the three cargo build dirs. The project bind mount
# shadows the image's /app/target, so without these every container run would
# recompile core/alloc + every dependency from scratch. Named volumes are
# auto-created by podman/docker on first run. `:Z` matches the project mount
# (private SELinux label); it is inert on macOS, where podman runs no SELinux.
CARGO_CACHE   ?= gba-cargo-target
FLAC_CACHE    ?= gba-flac-lite-target
FLACROM_CACHE ?= gba-flac-integration-target
SPIKE_CACHE   ?= gba-flac-spike-target
OPUS_CACHE    ?= gba-opus-spike-target
KERNEL_CACHE ?= gba-arm-kernels-target
OVERLAY_CACHE ?= gba-flac-lite-gba-target
CACHE_MOUNTS := -v $(CARGO_CACHE):/app/target:Z \
                -v $(FLAC_CACHE):/app/crates/flac-lite/target:Z \
                -v $(OVERLAY_CACHE):/app/crates/flac-lite-gba/target:Z \
                -v $(FLACROM_CACHE):/app/examples/flac_integration/target:Z \
                -v $(SPIKE_CACHE):/app/examples/flac_spike/target:Z \
                -v $(OPUS_CACHE):/app/examples/opus_spike/target:Z \
                -v $(KERNEL_CACHE):/app/examples/arm-kernels/target:Z
# Note, not fixed here: KERNEL_CACHE has been in $(CACHE_MOUNTS) since that
# crate landed but was never added to `clean-cache` below. `clean-cache` now
# lists $(OVERLAY_CACHE) because this change introduces it; the kernel volume's
# absence is named so it is not mistaken for a deliberate exclusion.
# GBA test harness (ROM tests boot headlessly in mGBA). Override to point
# elsewhere, or set empty to see the raw cargo command.
GBA_TEST_RUNNER ?= mgba-test-runner
# Host triple, used to defeat the inherited `target = thumbv4t-none-eabi` when
# running flac-lite's host tests. Queried through the SAME toolchain the tests
# build with, so a stray rustc elsewhere on PATH can't hand us the wrong triple.
# See "Cargo config leak" in FLAC.md.
HOST_TRIPLE := $(shell $(RUSTC) -vV 2>/dev/null | sed -n 's|host: ||p')

# Is rustfmt installed for the pinned toolchain? Asked the same way the
# toolchain is resolved (through $(CARGO)), so a shadowed cargo can't confuse
# it. `format` degrades to a notice instead of failing a build when it is
# absent — formatting is a convenience pre-step, never a build gate. The
# Dockerfile installs rustfmt so the standardized container path always
# formats.
HAS_RUSTFMT := $(shell $(CARGO) fmt --version >/dev/null 2>&1 && echo 1)

TARGET := thumbv4t-none-eabi
# The `crc-check` witness (FLAC.md Phase 2 step 5, 2026-11-03): the same
# flac-test shape with the feature ON — thumbv4t compile
# half plus the full host suite, which under the feature also runs
# tests/crc_check.rs (pass witness on the committed real frames + CRC
# mutations). Verification is OFF in the default build by design (release
# consumes the CRCs unchecked; "checked in debug, skippable in release"), so
# this gate exists precisely to keep the OFF default honest about the ON arm.
# It is invoked by hand: `test`/`podman-test` do not include it — see the
# gate-aggregation TODO in README.md.
flac-test-crc: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check
	@echo "flac-test-crc passed: frame CRC verification compiles for $(TARGET) and the host witness is green with crc-check on"

# The `scan` gate (4b's frame walk): same shape as flac-test-crc and for the same
# reason. With the feature off the module and its suite do not exist in a default
# build (`required-features = ["scan"]` in the crate manifest), so `flac-test`
# cannot go green on the walk's witness by accident. Keep the counts straight when
# reading logs: `flac-test` runs the crate WITHOUT the walk (its total is 4b's
# total minus the walk's tests), `flac-test-scan` runs it WITH them. Like
# flac-test-crc, this is not in `check-gba-all`: a GBA ROM does not link the walk
# until item 6 rewires the facade onto it, and crates/flac-lite/Cargo.toml records
# the measured code-size cost of doing that.
flac-test-scan: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features scan
	@echo "flac-test-scan passed: the frame walk compiles for $(TARGET) and its witness suite is green with the scan feature on"

# The `rice-hybrid` gate (split-ladder item 5 PR 3's portable hybrid Rice
# kernel): the same shape again, and here the gate is the finding rather than a
# convenience. With the feature off, the module, its unit tests and its
# out-of-crate differential do not exist in a default build
# (`required-features = ["rice-hybrid"]`), so `flac-test` cannot go green on the
# twin's witness by accident — and the image is byte-for-byte the pre-PR-3 image.
# With it on, the calibration instrument grows **+2,984 B of .text while `nm`
# reports no hybrid symbol at all**: a second implementor of the `RiceKernel`
# seam re-decides how the inliner treats the decode path beside it. That is the
# 4b `scan` result one rung over, measured in full in crates/flac-lite/Cargo.toml
# and FLAC.md, and it is why this is a feature: the default-flip decision has to
# stay attributable to the swapped mechanism alone. Like the other two, this is
# not in `check-gba-all`: no ROM links the twin, and the flip that would is
# gated on an on-target cycle reading.
# Counts when reading logs: `flac-test` runs the crate WITHOUT the twin, this
# gate runs it WITH the twin and its differential.
flac-test-rice: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features rice-hybrid
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features rice-hybrid
	@echo "flac-test-rice passed: the portable hybrid kernel compiles for $(TARGET) and its two-mechanism differential is green with rice-hybrid on"

# Second bare-metal target for the portability witness (`flac-portable-check`,
# FLAC.md split-proposal ladder step 3, 2026-11-02). Cortex-M4F: a canonical
# non-thumbv4t bare-metal ARM triple. Override with `SECOND_TARGET=<triple>`.
SECOND_TARGET ?= thumbv7em-none-eabi
ROM := gba-sound-player.gba
FLAC_ROM := flac-integration.gba
# Standalone crate that bundles flac-lite into a ROM (the FLAC sanity build).
FLAC_CRATE_DIR := examples/flac_integration
# Absolute manifest path for the flac-lite host gate (see `flac-test`: the gate
# runs from outside the repo so the root's cargo config is not inherited).
FLAC_CRATE_DIR_MANIFEST := $(abspath crates/flac-lite/Cargo.toml)
FLAC_CRATE_BIN := target/$(TARGET)/release/flac-integration
# The perf-gate spike ROM crate (examples/flac_spike/) — Phase 1 step 4, see
# FLAC.md "Perf gate spike". Its host witness gate runs the same out-of-tree
# --manifest-path way as flac-test (cargo config leak; FLAC.md).
SPIKE_ROM := flac-spike.gba
SPIKE_CRATE_DIR := examples/flac_spike
SPIKE_CRATE_MANIFEST := $(abspath examples/flac_spike/Cargo.toml)
SPIKE_CRATE_BIN := target/$(TARGET)/release/flac-spike
# The Opus perf-gate spike crate (examples/opus_spike/) — OPUS.md "Measurement
# plan — start here". Same standalone-workspace + inherited-config pattern as
# the FLAC spike; its host gate runs out-of-tree the same way (cargo config
# leak). PR 1 is the scaffold/compile-link gate; the decode witness grows into
# `opus-test` with step 2's embedded arms.
OPUS_ROM := opus-spike.gba
# The constructor-shape probe ROM (deliberately-red repro control, PR 3
# harness — src/ctor_probe.rs): built beside the proof ROM by `opus-rom`
# because `cargo build` compiles both bins anyway; its expected on-target
# death must never enter a test gate (a dead ROM wedges mgba-test-runner),
# so it is a build-only artifact captured by hand (sha + serial tail).
OPUS_PROBE_ROM := ctor-probe.gba
OPUS_CRATE_DIR := examples/opus_spike
OPUS_CRATE_MANIFEST := $(abspath examples/opus_spike/Cargo.toml)
OPUS_CRATE_BIN := target/$(TARGET)/release/opus-spike

# The E10 ARM-kernels crate (examples/arm-kernels/) — FLAC.md E10 groundwork:
# isolated ARM-mode kernels, each with a host twin and an on-target sweep of
# one committed golden table (no decode-path integration, no timing).
# Standalone workspace, same inherited-config pattern as the spikes; its host
# gate runs out-of-tree the same way (cargo config leak; FLAC.md).
KERNEL_ROM := arm-kernels.gba
KERNEL_CRATE_DIR := examples/arm-kernels
KERNEL_CRATE_MANIFEST := $(abspath examples/arm-kernels/Cargo.toml)
KERNEL_CRATE_BIN := target/$(TARGET)/release/arm-kernels

# The GBA OVERLAY crate (crates/flac-lite-gba/) — FLAC.md split-ladder item 6,
# 6a: the platform-bound Rice-kernel realizations written over flac-lite's
# `residual::RiceKernel` seam. A LIBRARY, not a ROM: no agb, so there is no ROM
# to build and no link step to witness (the crate's own docs carry that boundary
# and its `unsafe` budget). Standalone workspace with the root's config
# INHERITED on purpose — do NOT add a local .cargo/config.toml (cargo
# concatenates target.<triple>.rustflags, so a local re-declaration links
# `-Tgba.ld` twice; FLAC.md's double-link leak).
OVERLAY_CRATE_DIR := crates/flac-lite-gba
OVERLAY_CRATE_MANIFEST := $(abspath crates/flac-lite-gba/Cargo.toml)

# `rom` is the command run INSIDE the container (where agb-gbafix lives); the
# *-rom targets above it are the host-facing entrypoints. Keep `rom` = the
# native ROM build so containerized and native builds produce the same artifact.

# ---------------------------------------------------------------------------
# format — the build pre-step (writes files; all three workspaces).
#
# Builds auto-format, so experimental code lands formatted without a separate
# `make check` round trip. `check` stays the strict NON-writing gate
# (`cargo fmt --check`) that humans/CI run to verify; `format` is the writer.
#
# `podman-rom` gets this too for free (the container runs `make rom` ->
# `make build`), and the container mounts the project dir, so containerized
# formats write back to your working tree.
# ---------------------------------------------------------------------------
ifeq ($(HAS_RUSTFMT),1)
format:
	$(CARGO) fmt
	cd crates/flac-lite && $(CARGO) fmt
	cd $(FLAC_CRATE_DIR) && $(CARGO) fmt
	cd $(SPIKE_CRATE_DIR) && $(CARGO) fmt
	cd $(OPUS_CRATE_DIR) && $(CARGO) fmt
	cd $(KERNEL_CRATE_DIR) && $(CARGO) fmt
	cd $(OVERLAY_CRATE_DIR) && $(CARGO) fmt
else
format:
	@echo "format: skipping — no rustfmt for '$(TOOLCHAIN)' (build continues)."
	@echo "  enable with: rustup component add rustfmt --toolchain $(TOOLCHAIN)"
endif

# NOTE: `build` deliberately does NOT depend on `test`. Builds never gate on
# tests — experimental/untested code must stay buildable. Gates are what
# `make check` and `make test` are for, run explicitly (or in CI).
build: format
	$(CARGO) build --release --target $(TARGET)

# Link + fix the baseline ROM. Needs agb-gbafix on PATH (the container image
# installs it; natively: `cargo install agb-gbafix`).
rom: build
	agb-gbafix target/$(TARGET)/release/gba-sound-player -o $(ROM)
	@echo "ROM built: $(ROM)"

# ---------------------------------------------------------------------------
# ROM build entrypoints — one per compute environment.
#
#   native-rom    no virtualization required; uses this host's toolchain.
#   podman-rom    reproducible containerized build (the standardized path).
#
# Both produce the same $(ROM) artifact.
# ---------------------------------------------------------------------------

# Native: build + fix here. Requires nightly + rust-src + agb-gbafix locally.
native-rom: rom
	@echo "native-rom done: $(ROM)"

# Containerized: build the image (builder stage only), then run `make rom`.
podman-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make rom

# ---------------------------------------------------------------------------
# EXPERIMENTAL: flac-lite bundled into a ROM (sanity check for FLAC.md option
# (c) — our own decoder must keep fitting the GBA target end to end).
#
#   native-flac-rom     build it here
#   podman-flac-rom     build it in the container
#
# It compiles + links + boots; it does NOT play audio yet (flac-lite is still
# scaffold). The root ROM crate stays FLAC-free either way.
# ---------------------------------------------------------------------------

flac-rom: format
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(FLAC_CRATE_DIR)/$(FLAC_CRATE_BIN) -o $(FLAC_ROM)
	@echo "FLAC ROM built: $(FLAC_ROM)"

native-flac-rom: flac-rom
	@echo "native-flac-rom done: $(FLAC_ROM)"

podman-flac-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make flac-rom

# ---------------------------------------------------------------------------
# PERF-GATE SPIKE ROM (Phase 1 step 4 — FLAC.md "Perf gate spike").
#
#   spike-rom           build + fix the spike ROM (bare name: runs anywhere)
#   native-spike-rom    build it here
#   podman-spike-rom    build it in the container
#
# The measurement vehicle for the go/no-go perf gate. The root ROM crate and
# the flac_integration sanity ROM are untouched by it.
# ---------------------------------------------------------------------------

spike-rom: format
	cd $(SPIKE_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(SPIKE_CRATE_DIR)/$(SPIKE_CRATE_BIN) -o $(SPIKE_ROM)
	@echo "Spike ROM built: $(SPIKE_ROM)"

native-spike-rom: spike-rom
	@echo "native-spike-rom done: $(SPIKE_ROM)"

podman-spike-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make spike-rom

# ---------------------------------------------------------------------------
# SAMPLE-RATE ROM (FLAC.md "Sample-rate reduction — tooling and profiling
# proposal (2026-10-11)", Part 1 steps 1+2+3): real audio normalized to a
# mixer-domain rate, embedded TEMPORARILY, decode measured against the
# budget derived at that rate.
#
#   make rate-assets INPUT=path/to.wav RATE=10512   stage the clips (host tools:
#                                                   ffmpeg + flac + python3)
#   make rate-test                                  host witness on the staged
#                                                   bytes (bit-exact vs flac -d)
#   make native-rate-rom                            build $(RATE_ROM)
#
# COEXISTENCE: the committed spike assets (assets/spike_*.bin, src/assets.rs)
# and the default flac-spike ROM are the E-menu's calibration anchor and are
# NEVER touched by this flow — the rate bin/test are `required-features =
# ["rate-assets"]` targets, invisible to spike-rom/spike-test. Everything the
# rate flow writes is generated + gitignored + removed by `make clean`.
# ---------------------------------------------------------------------------

RATE_ROM := flac-rate-spike.gba
# The rate-PLAY ROM (decode into audible playback via the custom IRQ
# feeder, FLAC.md rate-flow playback entry) — same assets, same gate.
RATE_PLAY_ROM := flac-rate-play.gba
# The generated module whose presence means `make rate-assets` has run.
RATE_CRATE_ASSETS := $(SPIKE_CRATE_DIR)/src/rate_assets.rs
RATE_BIN := $(SPIKE_CRATE_DIR)/target/$(TARGET)/release/flac-rate-spike
RATE_PLAY_BIN := $(SPIKE_CRATE_DIR)/target/$(TARGET)/release/flac-rate-play

rate-assets:
	@test -n "$(INPUT)" || { echo "usage: make rate-assets INPUT=path/to.wav RATE={10512|18157|32768}"; exit 1; }
	@test -n "$(RATE)" || { echo "usage: make rate-assets INPUT=path/to.wav RATE={10512|18157|32768}"; exit 1; }
	bash scripts/gen_rate_assets.sh "$(INPUT)" "$(RATE)"

# The rate flow's host gate: same out-of-tree --manifest-path pattern as
# spike-test (cargo config leak), plus --features rate-assets. Fails closed
# when the generated modules are absent — stage them first.
rate-test: toolchain-check
	@test -f $(RATE_CRATE_ASSETS) || { echo "rate-test: no generated rate assets — run 'make rate-assets INPUT=… RATE=…' first"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(SPIKE_CRATE_MANIFEST) --target $(HOST_TRIPLE) --features rate-assets
	@echo "rate-test passed: the staged rate clips decode bit-exactly on the host"

rate-rom: format
	@test -f $(RATE_CRATE_ASSETS) || { echo "rate-rom: no generated rate assets — run 'make rate-assets INPUT=… RATE=…' first"; exit 1; }
	cd $(SPIKE_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --features rate-assets
	agb-gbafix $(RATE_BIN) -o $(RATE_ROM)
	@echo "Rate ROM built: $(RATE_ROM)"

native-rate-rom: rate-rom
	@echo "native-rate-rom done: $(RATE_ROM)"

# The playback ROM: identical gate to rate-rom (staged assets required).
rate-play-rom: format
	@test -f $(RATE_CRATE_ASSETS) || { echo "rate-play-rom: no generated rate assets — run 'make rate-assets INPUT=… RATE=…' first"; exit 1; }
	cd $(SPIKE_CRATE_DIR) && $(CARGO) build --release --target $(TARGET) --features rate-assets --bin flac-rate-play
	agb-gbafix $(RATE_PLAY_BIN) -o $(RATE_PLAY_ROM)
	@echo "Rate-play ROM built: $(RATE_PLAY_ROM)"

native-rate-play-rom: rate-play-rom
	@echo "native-rate-play-rom done: $(RATE_PLAY_ROM)"

podman-rate-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make rate-rom

# ---------------------------------------------------------------------------
# OPUS PERF-GATE SPIKE ROM (OPUS.md "Measurement plan — start here").
#
#   opus-rom            build + fix the scaffold ROM (bare name: runs anywhere)
#   native-opus-rom     build it here
#   podman-opus-rom     build it in the container
#
# PR 1: the compile/link gate — the vendored patched opus-rs (see the crate's
# vendor/opus-rs/PATCHES.md) LINKS into a bootable ROM image for $(TARGET),
# the real link rather than a bare check. The root ROM crate stays Opus-free.
# ---------------------------------------------------------------------------

opus-rom: format
	cd $(OPUS_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(OPUS_CRATE_DIR)/$(OPUS_CRATE_BIN) -o $(OPUS_ROM)
	agb-gbafix $(OPUS_CRATE_DIR)/target/$(TARGET)/release/ctor-probe -o $(OPUS_PROBE_ROM)
	@echo "Opus spike ROM built: $(OPUS_ROM) (+ constructor probe $(OPUS_PROBE_ROM))"

native-opus-rom: opus-rom
	@echo "native-opus-rom done: $(OPUS_ROM)"

podman-opus-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make opus-rom

# ---------------------------------------------------------------------------
# E10 GROUNDWORK: ARM kernel-check ROM (FLAC.md "E10 — ARM-mode codeword
# loop"; examples/arm-kernels/).
#
#   kernel-rom          build + fix the kernel-check ROM (bare name: runs anywhere)
#   native-kernel-rom   build it here
#   podman-kernel-rom   build it in the container
#
# Isolated ARM-mode kernels, unit-testable in isolation: the ROM sweeps the
# assembly over the committed golden table on the emulator's ARM7TDMI — the
# execution oracle, since the host has no ARM executor. No decode-path
# integration and no cycle windows: correctness groundwork only; measurement
# stays gated on the E10 probe plan (FLAC.md).
# ---------------------------------------------------------------------------

kernel-rom: format
	cd $(KERNEL_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(KERNEL_CRATE_DIR)/$(KERNEL_CRATE_BIN) -o $(KERNEL_ROM)
	@echo "Kernel-check ROM built: $(KERNEL_ROM)"

native-kernel-rom: kernel-rom
	@echo "native-kernel-rom done: $(KERNEL_ROM)"

podman-kernel-rom:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make kernel-rom

# ---------------------------------------------------------------------------
# Containerized testing.
#
# Same convention as the ROM builds: the BARE names (`rom`, `flac-rom`, `test`,
# `test-rom`, `flac-test`) run wherever they are invoked — natively on a host,
# or inside the container when the `podman-*` wrapper re-invokes them there.
# The `podman-*` names are the host-facing containerized entrypoints. Nothing
# branches on "am I in a container?", so no recursion and no surprise: a host
# with the toolchain installed uses the bare names; a host without them (no
# agb-gbafix, no mgba-test-runner, an unexpected nightly) uses `podman-*`.
#   make podman-test        ROM suite + flac-lite gates, in the image
#   make podman-flac-test   flac-lite alone, in the image
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Tests. Deliberately separated so a decoder bug is never confused with a ROM
# integration failure:
#
#   flac-test   flac-lite ALONE — host unit/integration tests + the GBA target
#               compile gate. No agb, no ROM, no emulator.
#   test        the ROM test suite (agb #[test_case] harness in mGBA).
#
# If flac-test passes but test/native-flac-rom fails, the problem is the
# integration (linking, memory, mixer/scheduling), not the decoder.
# ---------------------------------------------------------------------------

# Where the host gate runs: a directory OUTSIDE the repo, so cargo's upward
# `.cargo/config.toml` walk finds nothing to inherit. Must live outside the
# tree — any in-repo directory still walks up into the root's GBA config.
GATE_NEUTRAL_CWD ?= /tmp/gba-sound-player-host-gate

# flac-lite in isolation. Both gates are load-bearing:
#  1. target check  — the gate symphonia could never pass; keeps std out.
#     Run in-tree, where inheriting the root config is exactly what we want.
#  2. host tests    — run from $(GATE_NEUTRAL_CWD) with --manifest-path, so the
#     root's GBA `target` AND `build-std` are never inherited at all. `--target
#     $(HOST_TRIPLE)` stays explicit so the gate documents its own intent.
#
# Why not the old `-Zbuild-std=` (empty) + `cd` form: it held until nightly
# ~2026-06-01, then rotted. On nightly 2026-09-03 the inherited build-std wins
# over EVERY in-tree override — CLI `-Zbuild-std=`, the empty
# `CARGO_UNSTABLE_BUILD_STD=` env var, and `--config unstable.build-std=[]` all
# still recompile `core` from source under the prebuilt host `core`, giving
# `E0152: duplicate lang item core::sized` (verified 2026-09-04; full A/B table
# in FLAC.md → "Cargo config leak"). Config non-inheritance is positional, so it
# cannot drift with nightly the way flag-precedence does. The build dir stays
# `crates/flac-lite/target` (derived from the manifest, not the cwd), so the
# container cache volumes still apply.
flac-test: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE)
	@echo "flac-test passed: flac-lite compiles for $(TARGET) and passes host tests"

# Second-target build witness (FLAC.md split-proposal ladder step 3, 2026-11-02):
# records the PORTABILITY claim — flac-lite compiles for a non-thumbv4t
# bare-metal ARM target — as a build fact, with NO performance claim attached
# (the split's reading rules). Same in-tree compile-gate shape as flac-test's
# thumbv4t half: `-Zbuild-std=core,alloc` compiles core/alloc from rust-src, so
# the triple needs no prebuilt `rustup` rust-std — verified with the component
# REMOVED, which is the container image's shape (nightly + rust-src only;
# no Dockerfile change). Scope: rlib compile, not final link — a link wants a
# board link script, same boundary as the thumbv4t `check` half (whose real
# link is `make flac-rom`). Pin OFF: no `.iwram` reference compiles in on
# non-GBA targets (step 2's gate made that the shape). `scan` is turned ON here on
# purpose (review decision, 2026-11-05): the walk is `no_std` + `alloc` code whose
# portability no other gate proves, and a feature-gated module that no gate builds
# for a second target is a module whose portability claim is asserted rather than
# measured. This is the only gate that compiles the walk for a non-thumbv4t
# bare-metal target; `flac-test-scan` covers the thumbv4t half. `rice-hybrid`
# joins it on the same argument, and it matters more here: PR 3's own title is
# "the *portable*-twin default", and the one claim of the twin's that no other
# gate proves is that the hybrid walk builds for a second bare-metal ARM target.
flac-portable-check: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(SECOND_TARGET) -Zbuild-std=core,alloc --features scan,rice-hybrid
	@echo "flac-portable-check passed: flac-lite with the frame walk on compiles for $(SECOND_TARGET) (portability witness, no perf claim)"

# The spike's assets witness: the host tests (examples/flac_spike/tests/) decode
# the EXACT embedded clips bit-exactly against flac -d reference PCM. Same
# out-of-tree pattern as flac-test's host half — the spike crate inherits the
# root's GBA config otherwise (cargo config leak, FLAC.md). The thumbv4t half
# of the spike's compile gate is `make spike-rom` itself (it links the real
# ROM, so a bare `check` would understate it).
spike-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(SPIKE_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "spike-test passed: spike clips decode bit-exactly on the host"

# The Opus spike's host gate: same out-of-tree --manifest-path pattern (cargo
# config leak). PR 1 pins the scaffold facts (constructor contract, derived
# budget, state size, seam forwarding); step 2 grows it into the decode
# witness against the ffmpeg/libopus reference. The thumbv4t half of the
# compile gate is `make opus-rom` itself (it links the real image).
opus-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(OPUS_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "opus-test passed: opus spike scaffold pins hold on the host"

podman-opus-test:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make opus-test

# Host witness for the E10 ARM kernels: kernel twins vs the naive bit-walk
# oracle and the committed golden table. Same out-of-tree --manifest-path
# pattern (cargo config leak). The thumbv4t half of the compile gate is
# `make kernel-rom` itself (the real link, not a bare check).
kernel-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(KERNEL_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "kernel-test passed: E10 kernel twins match the oracle and the committed table"

podman-kernel-test:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make kernel-test

# ---------------------------------------------------------------------------
# THE GBA OVERLAY (FLAC.md split-ladder item 6, 6a; crates/flac-lite-gba/).
#
#   overlay-check        the crate COMPILES for $(TARGET) and flac-lite resolves
#                        through the path dependency, in a release check
#   overlay-test         the crate's host witness, run out-of-tree (config leak)
#
# What neither one witnesses, stated plainly because both absences are the point
# of 6a-1: no link step exists in an rlib, so there is no link witness and no
# codegen charge claimed here (6a-1 builds no ROM — the crate carries no `agb`),
# no cycle is claimed, and no ROM in this repo links the overlay yet. The
# thumbv4t half is `cargo check` rather than a ROM build precisely because there
# is no ROM to build; the real link witness for this crate arrives with 6a-3, as
# an arm of an existing instrument. And the rule the crate manifest follows
# because of this file: the overlay has NO local .cargo/config.toml, because
# cargo CONCATENATES `target.<triple>.rustflags` across config layers, so a local
# re-declaration would double-link `-Tgba.ld` (README, agb-build gotcha 2 and its
# two variants). 6a-1 cannot convict that leak either way — there is no link step
# in an rlib — so the rule is inherited from the tree rather than tested here; the
# first rung in this item with a link step next to asm is 6a-4, in an instrument.
#
# One limit of `overlay-check`, found by 6a-1's own mutation witness (FLAC.md, the
# 6a-1 record): `cargo check` does not compile `#[cfg(test)]` source, so an
# `unsafe` inside a test is convicted by `overlay-test` and never here; and
# deleting the path dependency outright leaves `overlay-check` GREEN — it asserts
# nothing about the edge being USED. What it witnesses is that cargo resolves
# `flac-lite` through this manifest and type-checks it for $(TARGET), which is the
# `Checking flac-lite` line in its own log. The live-edge witness is
# `overlay-test`.
#
# Deliberately NOT part of `flac-gates`: that aggregate is every configuration of
# `crates/flac-lite`, and this is a different crate whose features do not exist
# yet. Deliberately NOT in `test` either, and hand-invoked like
# flac-test-crc/-scan/-rice, for the same reason: nothing links this crate, so
# these are plumbing gates. There is no $(SECOND_TARGET) arm here on purpose — a
# portability witness for a crate whose whole purpose is being platform-bound
# would be a claim this item never makes; portability stays witnessed by
# `flac-portable-check`, on the core.
# ---------------------------------------------------------------------------

overlay-check: toolchain-check
	cd $(OVERLAY_CRATE_DIR) && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	@echo "overlay-check passed: the overlay AND the core it depends on both type-check for $(TARGET) (compile witness only: an rlib has no link step, and the live-edge witness is overlay-test)"

overlay-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(OVERLAY_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	@echo "overlay-test passed: the overlay's host witness is green (6a-1: one no-op test, by design)"

podman-overlay-test:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make overlay-test
	@echo "podman-overlay-test done: overlay host witness ran in $(CONTAINER)."

# Cargo's runner env var is the upper-cased target triple with dashes replaced
# by underscores: CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER.
TARGET_ENV := $(shell echo $(TARGET) | tr 'a-z-' 'A-Z_')

# ROM test suite on target. Needs mgba-test-runner on PATH:
#   natively    -> `cargo install --git https://github.com/agbrs/agb.git --tag v0.25.0 --locked mgba-test-runner`
#                  (also needs cmake + clang/libclang + libelf to build libmgba)
#   in the image -> preinstalled by the Dockerfile; use `make podman-test-rom`
#                  and none of that host setup applies.
test-rom: toolchain-check
	CARGO_TARGET_$(TARGET_ENV)_RUNNER=$(GBA_TEST_RUNNER) \
	  $(CARGO) test --target $(TARGET)

# Top-level test: everything except the intentionally-broken symphonia probe.
# Deliberately does NOT depend on `format`: a test run must never rewrite your
# working tree as a side effect (that would also let CI's `make test` silently
# fix drift that `make check` is supposed to fail on). Builds format; tests
# verify. `check` is the formatting gate.
test: test-rom flac-test
	@echo "test passed: ROM suite + flac-lite isolation gates"

# The aggregate README's "TODO: one aggregate for the non-default-configuration
# gates" asked for by name, landed with split-ladder item 5 PR 2 (2026-11-05):
# one command covering every `crates/flac-lite` configuration. It adds nothing
# that the named gates do not already witness — it is the same commands, and the
# two combination arms at the end (both features on: the thumbv4t `cargo check`
# and the host witness) are the only arms that were not already runnable, added
# because a *feature combination* is the only way to see a second instantiation,
# and PR 2 is the PR where the instantiation count changes rather than the code.
# PR 3 (2026-11-06) added the `rice-hybrid` configuration and a second
# combination arm (`crc-check,scan,rice-hybrid`): the one build in which the
# walk, the CRC verification and BOTH Rice mechanisms are compiled at once —
# which is also the shape a future default flip would ship in.
# Nothing here is parallel-safe-by-design: like `test:`, it is a plain
# prerequisite aggregate, so `make -j flac-gates` interleaves output.
flac-gates: flac-test flac-test-crc flac-test-scan flac-test-rice flac-portable-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check,scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check,scan
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc --features crc-check,scan,rice-hybrid
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check,scan,rice-hybrid
	@echo "flac-gates passed: every crates/flac-lite configuration is green — default, crc-check, scan, rice-hybrid, both, all three, and the second bare-metal target"

# ---------------------------------------------------------------------------
# Containerized test entrypoints (the portable path — the image ships
# mgba-test-runner + agb-gbafix + nightly + rust-src, so NO host tooling is
# needed). These build the image, then run the same bare gate targets inside,
# with the project dir mounted so artifacts and reports land back on the host.
#
# Use these when the native path can't run (missing toolchain pieces, an
# unexpected nightly, conda, etc.) — same commands, same gates, same result.
# ---------------------------------------------------------------------------

# Full suite: ROM #[test_case] suite in headless mGBA + flac-lite gates.
podman-test:
	$(CONTAINER) build --target tester -t $(IMAGE)-tester .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-tester make test
	@echo "podman-test done: ROM suite + flac-lite gates ran in $(CONTAINER)."

# ROM #[test_case] suite only.
podman-test-rom:
	$(CONTAINER) build --target tester -t $(IMAGE)-tester .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-tester make test-rom
	@echo "podman-test-rom done: ROM suite ran in $(CONTAINER)."

# flac-lite alone (target compile gate + host tests) — the fastest container
# loop; no agb, no ROM, no emulator. Runs in the lighter builder stage.
podman-flac-test:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make flac-test
	@echo "podman-flac-test done: flac-lite gates ran in $(CONTAINER)."

# Formatting gate in the container too (CI-friendly; identical rustfmt).
podman-check:
	$(CONTAINER) build --target builder -t $(IMAGE)-builder .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE)-builder make check
	@echo "podman-check done: formatting verified in $(CONTAINER)."

# ---------------------------------------------------------------------------

clean:
	$(CARGO) clean
	cd crates/flac-lite && $(CARGO) clean
	cd $(FLAC_CRATE_DIR) && $(CARGO) clean
	cd $(SPIKE_CRATE_DIR) && $(CARGO) clean
	cd $(KERNEL_CRATE_DIR) && $(CARGO) clean
	cd $(OVERLAY_CRATE_DIR) && $(CARGO) clean
	rm -f $(ROM) $(FLAC_ROM) $(SPIKE_ROM) $(OPUS_ROM) $(OPUS_PROBE_ROM) $(KERNEL_ROM) $(RATE_ROM) $(RATE_PLAY_ROM)
# The rate flow's generated assets are TEMPORARY (gitignored, like ROMs —
# FLAC.md "Sample-rate reduction"); they regenerate from
# `make rate-assets INPUT=… RATE=…`, so they leave with `make clean`.
	@rm -f $(SPIKE_CRATE_DIR)/src/rate_assets.rs $(SPIKE_CRATE_DIR)/src/rate_pcm.rs
	@rm -f $(SPIKE_CRATE_DIR)/assets/rate_*.bin
# mGBA drops a .sav beside each ROM on local runs — emulator state,
# remove it with the ROMs it belongs to.
	@rm -f $(ROM:.gba=.sav) $(FLAC_ROM:.gba=.sav) *.sav

# Drop the container build caches (the named volumes behind $(CACHE_MOUNTS)).
# Safe: they are pure build caches — the next container run rebuilds from
# scratch, slowly once, then repopulates them. Does NOT touch ./target.
clean-cache:
	-$(CONTAINER) volume rm $(CARGO_CACHE) $(FLAC_CACHE) $(FLACROM_CACHE) $(SPIKE_CACHE) $(OPUS_CACHE) $(OVERLAY_CACHE) 2>/dev/null || true
	@echo "container build-cache volumes removed ($(CARGO_CACHE), $(FLAC_CACHE), $(FLACROM_CACHE), $(SPIKE_CACHE), $(OPUS_CACHE), $(OVERLAY_CACHE))."

check:
	$(CARGO) fmt --check
	cd crates/flac-lite && $(CARGO) fmt --check
	cd $(FLAC_CRATE_DIR) && $(CARGO) fmt --check
	cd $(SPIKE_CRATE_DIR) && $(CARGO) fmt --check
	cd $(OPUS_CRATE_DIR) && $(CARGO) fmt --check
	cd $(KERNEL_CRATE_DIR) && $(CARGO) fmt --check
	cd $(OVERLAY_CRATE_DIR) && $(CARGO) fmt --check

# Fail early and legibly when the pinned toolchain isn't reachable, instead of
# half-running a gate (e.g. `--target ''` when the host triple can't be probed).
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
	@# The other half of the pin. Everything above asks "does the instance the
	@# Makefile chose run?"; this asks the question the Makefile cannot answer by
	@# itself: is that instance also the one rustup hands to a bare cargo, an
	@# editor, or rust-analyzer in this tree? rustup skips comment lines and says
	@# nothing about a [toolchain] table that has no channel — measured: with
	@# `# channel = "nightly-2026-09-04"` in the pin file, `rustup show
	@# active-toolchain` reports the ambient DEFAULT (stable 1.99.1, exit 0, and it
	@# happily installs components for it) while a word-level read of that same file
	@# still returns 2026-09-04. Two instruments in one tree is the exact failure the
	@# pin exists to prevent, so it is checked by asking rustup what it resolves and
	@# comparing the rustc it would run to the rustc this build runs — not by
	@# pattern-guessing at the file's format.
	@bld=$$($(RUSTC) -V 2>/dev/null); \
	actfull=$$($(RUSTUP) show active-toolchain 2>/dev/null | head -1); \
	act=$${actfull%% *}; got=$$($(RUSTUP) run "$$act" rustc -V 2>/dev/null); \
	if [ -n "$$act" ] && [ -n "$$got" ] && [ "$$bld" != "$$got" ]; then \
	  echo "error: this build runs '$$bld' (from $(TOOLCHAIN), read out of $(RUST_TOOLCHAIN_FILE)),"; \
	  echo "  but rustup resolves this tree to '$$act', which runs '$$got'."; \
	  echo "  rustup is not taking a channel from the pin file — a commented-out or mis-sectioned channel line makes it fall back to the ambient default silently; a RUSTUP_TOOLCHAIN environment override does the same."; \
	  echo "  fix the tree, not the build: the instance cargo and your editor see must be the string that builds the ROM (FLAC.md, the instance rule)."; \
	  exit 1; \
	fi
else
	@echo "notice: TOOLCHAIN=$(TOOLCHAIN) overrides the pinned instance ($(PINNED_TOOLCHAIN)); this build is not the instrument $(notdir $(RUST_TOOLCHAIN_FILE)) declares, so the rustup cross-check is skipped. Say so in the record."
endif
endif
endif

help:
	@echo "gba-sound-player build targets"
	@echo ""
	@echo "  ROMs (same artifact, two environments):"
	@echo "    native-rom        build $(ROM) with this host's toolchain"
	@echo "    podman-rom        build $(ROM) in the $(CONTAINER) image (standardized)"
	@echo ""
	@echo "  EXPERIMENTAL FLAC ROMs (flac-lite bundled for $(TARGET)):"
	@echo "    native-flac-rom   build $(FLAC_ROM) natively"
	@echo "    podman-flac-rom   build $(FLAC_ROM) in the container"
	@echo ""
	@echo "  PERF-GATE SPIKE (FLAC.md step 4):"
	@echo "    native-spike-rom  build $(SPIKE_ROM) natively"
	@echo "    podman-spike-rom  build $(SPIKE_ROM) in the container"
	@echo "    spike-test        spike asset witness tests (host, bit-exact)"
	@echo ""
	@echo "  SAMPLE-RATE ROM (FLAC.md 'Sample-rate reduction', Part 1):"
	@echo "    rate-assets       stage TEMPORARY clips from a real WAV (INPUT=… RATE={10512|18157|32768})"
	@echo "    rate-test         host witness on the staged bytes (bit-exact vs flac -d)"
	@echo "    native-rate-rom   build $(RATE_ROM) from the staged assets"
	@echo "    native-rate-play-rom build $(RATE_PLAY_ROM) — decode→playback via the custom timer-IRQ feeder"
	@echo "    podman-rate-rom   build it in the container (assets must be staged on the host)"
	@echo "    flow: make rate-assets INPUT=clip.wav RATE=10512 && make rate-test && make native-rate-rom"
	@echo ""
	@echo "  OPUS PERF-GATE SPIKE (OPUS.md):"
	@echo "    native-opus-rom   build $(OPUS_ROM) + $(OPUS_PROBE_ROM) natively (vendored opus-rs link gate)"
	@echo "    podman-opus-rom   build $(OPUS_ROM) in the container"
	@echo "    opus-test         opus spike host gate (scaffold pins; step-2 witness grows in)"
	@echo ""
	@echo "  E10 ARM KERNELS (FLAC.md — isolated-kernel groundwork):"
	@echo "    native-kernel-rom build $(KERNEL_ROM) natively (on-target kernel sweep ROM)"
	@echo "    podman-kernel-rom build $(KERNEL_ROM) in the container"
	@echo "    kernel-test       kernel twins vs oracle + committed table (host)"
	@echo ""
	@echo "  GBA OVERLAY (FLAC.md split-ladder item 6a; crates/flac-lite-gba):"
	@echo "    overlay-check     the overlay COMPILES for $(TARGET) against flac-lite (compile only: an rlib has no link step)"
	@echo "    overlay-test      the overlay's host witness (6a-1: one no-op test by design; hand-invoked, not in test/flac-gates)"
	@echo "    podman-overlay-test  that host witness in the container"
	@echo ""
	@echo "  Tests — native gates (need nightly + agb-gbafix + mgba-test-runner locally):"
	@echo "    test              test-rom + flac-test (verify-only: no reformat)"
	@echo "    test-rom          ROM #[test_case] suite in mGBA"
	@echo "    flac-test         flac-lite alone (target compile gate + host tests)"
	@echo "    flac-portable-check  flac-lite second-target build witness ($(SECOND_TARGET); with the frame walk on: portability fact, no perf claim)"
	@echo "    flac-test-crc     flac-lite host suite + thumbv4t compile with crc-check ON (hand-invoked; see README gate-aggregation TODO)"
	@echo "    flac-test-scan    flac-lite host suite + thumbv4t compile with the frame walk (\`scan\`) ON (hand-invoked; see README gate-aggregation TODO)"
	@echo "    flac-test-rice    flac-lite host suite + thumbv4t compile with the portable hybrid Rice kernel (\`rice-hybrid\`) ON (hand-invoked; see README gate-aggregation TODO)"
	@echo "    flac-gates        EVERY flac-lite configuration in one command, plus the combination arms"
	@echo ""
	@echo "  Tests — containerized (same gates, no host tooling needed):"
	@echo "    podman-test       test-rom + flac-test in the $(CONTAINER) image"
	@echo "    podman-test-rom   ROM #[test_case] suite in the container"
	@echo "    podman-flac-test  flac-lite alone in the container"
	@echo "    podman-kernel-test E10 kernel host gate in the container"
	@echo "    podman-check      cargo fmt --check in the container"
	@echo ""
	@echo "  Formatting:"
	@echo "    format            cargo fmt all workspaces (writes; runs before builds)"
	@echo "    check             cargo fmt --check (verify-only gate; never writes)"
	@echo ""
	@echo "  Other: build, rom, flac-rom, spike-rom, clean, clean-cache, help"
	@echo ""
	@echo "  Builds never gate on tests; they DO auto-run 'format' first."
	@echo "  (no rustfmt for $(TOOLCHAIN)? format prints a notice and skips.)"
	@echo ""
	@echo "  Toolchain: $(RUSTC)  (override: RUSTUP= / TOOLCHAIN= / CARGO=)"
	@echo "  Container: $(CONTAINER)  image: $(IMAGE)  (override: CONTAINER=docker IMAGE=)"
	@echo "  Host triple: $(HOST_TRIPLE)"
