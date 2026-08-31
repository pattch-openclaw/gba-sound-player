.PHONY: build rom native-rom podman-rom flac-rom native-flac-rom podman-flac-rom \
        test flac-test test-rom clean check help

# ---------------------------------------------------------------------------
# Build settings (all overridable, e.g. `make podman-rom CONTAINER=docker`)
#
# nightly is REQUIRED: -Zbuild-std (in .cargo/config.toml) is a nightly-only
# flag, used to compile core/alloc from source for this Tier-3 target. See
# Dockerfile/README. Do NOT try to `rustup target add thumbv4t-none-eabi` — it
# has no prebuilt artifacts on any toolchain.
# ---------------------------------------------------------------------------
CARGO ?= cargo +nightly
# Container runtime: podman by default, override with `make podman-rom CONTAINER=docker`.
CONTAINER ?= podman
# GBA test harness (ROM tests boot headlessly in mGBA). Override to point
# elsewhere, or set empty to see the raw cargo command.
GBA_TEST_RUNNER ?= mgba-test-runner
# Host triple, used to defeat the inherited `target = thumbv4t-none-eabi` when
# running flac-lite's host tests. See "The cargo config leak" in FLAC.md.
HOST_TRIPLE := $(shell rustc -vV | sed -n 's|host: ||p')

TARGET := thumbv4t-none-eabi
ROM := gba-sound-player.gba
FLAC_ROM := flac-integration.gba
# Standalone crate that bundles flac-lite into a ROM (the FLAC sanity build).
FLAC_CRATE_DIR := examples/flac_integration
FLAC_CRATE_BIN := target/$(TARGET)/release/flac-integration

# `rom` is the command run INSIDE the container (where agb-gbafix lives); the
# *-rom targets above it are the host-facing entrypoints. Keep `rom` = the
# native ROM build so containerized and native builds produce the same artifact.

build:
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

# Containerized: build the image, then run `make rom` inside it with the project
# dir mounted. The .gba lands back in your host dir.
podman-rom:
	$(CONTAINER) build -t gba-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app gba-builder make rom

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

flac-rom:
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET)
	agb-gbafix $(FLAC_CRATE_DIR)/$(FLAC_CRATE_BIN) -o $(FLAC_ROM)
	@echo "FLAC ROM built: $(FLAC_ROM)"

native-flac-rom: flac-rom
	@echo "native-flac-rom done: $(FLAC_ROM)"

podman-flac-rom:
	$(CONTAINER) build -t gba-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app gba-builder make flac-rom

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

# flac-lite in isolation. Both gates are load-bearing:
#  1. target check  — the gate symphonia could never pass; keeps std out
#  2. host tests    — `-Zbuild-std=` (empty) defeats the inherited core
#     recompile and `--target $(HOST_TRIPLE)` defeats the inherited GBA target.
#     Cargo MERGES config arrays up the directory tree, so an empty local
#     override alone does not clear the parent's build-std. Both flags needed.
flac-test:
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET) -Zbuild-std=core,alloc
	cd crates/flac-lite && \
	  $(CARGO) test -Zbuild-std= --target $(HOST_TRIPLE)
	@echo "flac-test passed: flac-lite compiles for $(TARGET) and passes host tests"

# Cargo's runner env var is the upper-cased target triple with dashes replaced
# by underscores: CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER.
TARGET_ENV := $(shell echo $(TARGET) | tr 'a-z-' 'A-Z_')

# ROM test suite on target (needs mgba-test-runner:
# `cargo install mgba-test-runner --git https://github.com/agbrs/agb.git`).
test-rom:
	CARGO_TARGET_$(TARGET_ENV)_RUNNER=$(GBA_TEST_RUNNER) \
	  $(CARGO) test --target $(TARGET)

# Top-level test: everything except the intentionally-broken symphonia probe.
test: test-rom flac-test
	@echo "test passed: ROM suite + flac-lite isolation gates"

# ---------------------------------------------------------------------------

clean:
	$(CARGO) clean
	cd crates/flac-lite && $(CARGO) clean
	cd $(FLAC_CRATE_DIR) && $(CARGO) clean
	rm -f $(ROM) $(FLAC_ROM)

check:
	$(CARGO) fmt --check
	cd crates/flac-lite && $(CARGO) fmt --check
	cd $(FLAC_CRATE_DIR) && $(CARGO) fmt --check

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
	@echo "  Tests:"
	@echo "    flac-test         flac-lite alone (target compile gate + host tests)"
	@echo "    test-rom          ROM #[test_case] suite in mGBA"
	@echo "    test              test-rom + flac-test"
	@echo ""
	@echo "  Other: build, rom, flac-rom, clean, check, help"
