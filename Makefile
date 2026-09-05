.PHONY: format build rom native-rom podman-rom flac-rom native-flac-rom podman-flac-rom \
        test test-rom flac-test podman-test podman-test-rom podman-flac-test \
        clean clean-cache check podman-check help toolchain-check

# ---------------------------------------------------------------------------
# Build settings (all overridable, e.g. `make podman-rom CONTAINER=docker`)
#
# nightly is REQUIRED: -Zbuild-std (in .cargo/config.toml) is a nightly-only
# flag, used to compile core/alloc from source for this Tier-3 target. See
# Dockerfile/README. Do NOT try to `rustup target add thumbv4t-none-eabi` — it
# has no prebuilt artifacts on any toolchain.
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
# rustup: `make check CARGO=cargo RUSTC=rustc` (needs nightly as the default).
RUSTUP ?= rustup
TOOLCHAIN ?= nightly

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
CACHE_MOUNTS := -v $(CARGO_CACHE):/app/target:Z \
                -v $(FLAC_CACHE):/app/crates/flac-lite/target:Z \
                -v $(FLACROM_CACHE):/app/examples/flac_integration/target:Z
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
ROM := gba-sound-player.gba
FLAC_ROM := flac-integration.gba
# Standalone crate that bundles flac-lite into a ROM (the FLAC sanity build).
FLAC_CRATE_DIR := examples/flac_integration
# Absolute manifest path for the flac-lite host gate (see `flac-test`: the gate
# runs from outside the repo so the root's cargo config is not inherited).
FLAC_CRATE_DIR_MANIFEST := $(abspath crates/flac-lite/Cargo.toml)
FLAC_CRATE_BIN := target/$(TARGET)/release/flac-integration

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

# Containerized: build the image, then run `make rom` inside it with the project
# dir mounted. The .gba lands back in your host dir.
podman-rom:
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make rom

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
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make flac-rom

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
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make test
	@echo "podman-test done: ROM suite + flac-lite gates ran in $(CONTAINER)."

# ROM #[test_case] suite only.
podman-test-rom:
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make test-rom
	@echo "podman-test-rom done: ROM suite ran in $(CONTAINER)."

# flac-lite alone (target compile gate + host tests) — the fastest container
# loop; no agb, no ROM, no emulator.
podman-flac-test:
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make flac-test
	@echo "podman-flac-test done: flac-lite gates ran in $(CONTAINER)."

# Formatting gate in the container too (CI-friendly; identical rustfmt).
podman-check:
	$(CONTAINER) build -t $(IMAGE) .
	$(CONTAINER) run --rm $(CACHE_MOUNTS) -v "$(PWD):/app:Z" -w /app $(IMAGE) make check
	@echo "podman-check done: formatting verified in $(CONTAINER)."

# ---------------------------------------------------------------------------

clean:
	$(CARGO) clean
	cd crates/flac-lite && $(CARGO) clean
	cd $(FLAC_CRATE_DIR) && $(CARGO) clean
	rm -f $(ROM) $(FLAC_ROM)

# Drop the container build caches (the named volumes behind $(CACHE_MOUNTS)).
# Safe: they are pure build caches — the next container run rebuilds from
# scratch, slowly once, then repopulates them. Does NOT touch ./target.
clean-cache:
	-$(CONTAINER) volume rm $(CARGO_CACHE) $(FLAC_CACHE) $(FLACROM_CACHE) 2>/dev/null || true
	@echo "container build-cache volumes removed ($(CARGO_CACHE), $(FLAC_CACHE), $(FLACROM_CACHE))."

check:
	$(CARGO) fmt --check
	cd crates/flac-lite && $(CARGO) fmt --check
	cd $(FLAC_CRATE_DIR) && $(CARGO) fmt --check

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
	@echo "  Tests — native gates (need nightly + agb-gbafix + mgba-test-runner locally):"
	@echo "    test              test-rom + flac-test (verify-only: no reformat)"
	@echo "    test-rom          ROM #[test_case] suite in mGBA"
	@echo "    flac-test         flac-lite alone (target compile gate + host tests)"
	@echo ""
	@echo "  Tests — containerized (same gates, no host tooling needed):"
	@echo "    podman-test       test-rom + flac-test in the $(CONTAINER) image"
	@echo "    podman-test-rom   ROM #[test_case] suite in the container"
	@echo "    podman-flac-test  flac-lite alone in the container"
	@echo "    podman-check      cargo fmt --check in the container"
	@echo ""
	@echo "  Formatting:"
	@echo "    format            cargo fmt all workspaces (writes; runs before builds)"
	@echo "    check             cargo fmt --check (verify-only gate; never writes)"
	@echo ""
	@echo "  Other: build, rom, flac-rom, clean, clean-cache, help"
	@echo ""
	@echo "  Builds never gate on tests; they DO auto-run 'format' first."
	@echo "  (no rustfmt for $(TOOLCHAIN)? format prints a notice and skips.)"
	@echo ""
	@echo "  Toolchain: $(RUSTC)  (override: RUSTUP= / TOOLCHAIN= / CARGO=)"
	@echo "  Container: $(CONTAINER)  image: $(IMAGE)  (override: CONTAINER=docker IMAGE=)"
	@echo "  Host triple: $(HOST_TRIPLE)"
