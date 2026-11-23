.PHONY: format build rom flac-rom product-play-rom master-pack master-play-rom \
        spike-rom spike-test playback-test \
        overlay-check overlay-test overlay-check-arm overlay-test-arm \
        overlay-objdump overlay-kernel-rom overlay-kernel-test overlay-gates \
        test test-rom flac-test flac-test-wide flac-test-crc flac-test-scan \
        flac-test-rice flac-gates flac-portable-check gafp-check gafp-pack-check \
        product-play-test opus-test opus-rom uli-test uli-check uli-fixture \
        uli-default-tables uli-mdct-fixture uli-band-rdo-fixture uli-rd-ab uli-accept \
        uli-bench-rom uli-bench uli-attrib uli-attrib-subframe uli-frame-ops \
        uli-album-asset uli-album-bench uli-album-bench-rom uli-gate uli-gate-rom uli-gate-fixture uli-gate-fixture-rom uli-play-rom \
        uli-playback-test uli-player uli-player-rom uli-player-speaker uli-player-speaker-rom \
        uli-adversarial-asset uli-cross-asset uli-two-track-asset uli-playback-track-pin uli-player-two-track \
        uli-player-cross uli-player-cross-rom \
        uli-overlay-test uli-kernel-twin \
        album-probe-art album-probe-test album-probe-rom album-probe \
        album-bake album-pack-check album-seam-test album-layout-test album-glyphs-test album-rom album-player-rom album-test \
        album-preview \
        clean clean-cache check help toolchain-check target-spec-check

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

# The build target is a **checked-in target spec**, not a built-in triple
# (2026-11-17). `targets/thumbv4t-gba.json` is `rustc --print target-spec-json`
# for `thumbv4t-none-eabi` with exactly one field changed — `frame-pointer`
# `may-omit` instead of `always` — which frees r11 in every Rust function. See
# `targets/README.md` for the provenance, the measured effect and the
# `make target-spec-check` witness.
#
# `TARGET` is the spec's **file stem**, which is what cargo names the output
# directory after: builds land in `target/$(TARGET)/`. `TARGET_SPEC` is the path
# actually passed to `--target`, because cargo only resolves a bare name to
# `<name>.json` in the current directory, and most recipes `cd` first.
TARGET := thumbv4t-gba
TARGET_SPEC := $(abspath targets/$(TARGET).json)
# Second bare-metal triple for the core's portability witness (a build fact, no
# performance claim attached). Override with `SECOND_TARGET=<triple>`. This one
# stays a *built-in* triple on purpose: the witness is that the core builds for
# a target we do not ship, so it must not depend on a spec of ours.
SECOND_TARGET ?= thumbv7em-none-eabi
# Cargo's runner env var is the target name upper-cased with `-` -> `_`:
# CARGO_TARGET_THUMBV4T_GBA_RUNNER.
TARGET_ENV := $(shell echo $(TARGET) | tr 'a-z-' 'A-Z_')
GBA_TEST_RUNNER ?= mgba-test-runner

# Host triple, used to defeat the inherited GBA `[build] target` when a
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
# 2a's second generated module: the layout the selected variant declares
# (§5, §9.0's D9 row). Same class as assets.rs — a fresh clone has neither, and
# `make album-bake` writes both — so it is gitignored, cleaned, and reached
# through the same fmt stub, and `make album-layout-test` is its own gate.
ALBUM_LAYOUT_RS := $(ALBUM_CRATE_DIR)/src/layout.rs
ALBUM_LAYOUT_SPEC := $(ALBUM_CRATE_DIR)/layout_spec.toml
ALBUM_LAYOUT_GATE_LOG := album-layout-test.log
# §5.5a's third generated module: the glyph atlas of the one face the selected variant declares.
# Same class as the two above — gitignored, cleaned, reached through the same fmt stub, written by
# `make album-bake` — and `make album-glyphs-test` is its gate.
#
# It is a SEPARATE emitter rather than another table inside `album_bake.py` (re-ruled at the start
# of PR-2, against §5.5a step 5's wording; the reasoning is recorded there). Two things make the
# split safe rather than a second source of truth, and both are machine-checked, not assumed:
#   - it *imports* the face's numbers from `album_bake` (`FaceMetrics`, `layout_for`), so there is
#     one parser of §5.0's `[face]` block and one rasteriser of the line box, not two; and
#   - the module it writes re-asserts the layout module's face id, raster, line box, tile rows and
#     glyph/atlas counts as Rust `const _: () = assert!` items — so any drift between the two
#   generators is a compile error, which is a claim no Python leg can make.
ALBUM_GLYPHS_RS := $(ALBUM_CRATE_DIR)/src/glyphs.rs
ALBUM_GLYPHS_GATE_LOG := album-glyphs-test.log
# §9.1's `make album-preview`: the surface as pictures, without a cart (2a PR-(3)). The prefix the
# 1x and 4x PNGs and their `.txt` grids are written under.
ALBUM_PREVIEW := album-preview
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
ULI_OVERLAY_MANIFEST := $(abspath crates/uli-lite-gba/Cargo.toml)
# The host-side encoder (crates/uli-enc/). Its own cargo workspace, and a host
# build either way — it never compiles for the GBA, and the GBA never depends on
# it. Its gate is a round-trip through `uli-lite`: the decoder, not the encoder,
# says whether the blob is real.
ULI_ENC_MANIFEST := $(abspath crates/uli-enc/Cargo.toml)
# The acceptance harness is told which binary to compare instead of globbing the target
# tree: with a second host triple built, the lexicographic first is not necessarily
# the one this recipe just built.
ULI_ENC_BIN ?= crates/uli-enc/target/$(HOST_TRIPLE)/release/uli-enc
# How many `uli-enc` tests must run for the gate to count as having run anything.
# Raise it when tests are added; the gate refuses to pass below it, so a suite that
# fails to compile, is renamed out of `tests/`, or is filtered away cannot quietly
# vanish from the run. A count, not a per-suite shape check: the binary target and
# the doc-tests legitimately report zero.
ULI_ENC_MIN_TESTS := 89
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
              crates/uli-lite crates/uli-lite-gba examples/uli_bench \
              crates/uli-playback examples/uli_player \
              examples/album_probe

# examples/album_player is deliberately NOT in WORKSPACES: its `src/lib.rs`
# declares `pub mod assets;` from a GENERATED, gitignored `src/assets.rs` and
# (from 2a) `pub mod layout;` and (§5.5a) `pub mod glyphs;` from GENERATED modules, all of which
# only `make album-bake` writes, so it cannot be a plain `cargo fmt` / `cargo
# clean` target on a fresh clone — rustfmt resolves `mod` statically and aborts.
# Its fmt coverage comes from the `fmt_with_generated_stub.sh` call below (which
# writes a comment-only stub for each GENERATED=<file> it is given, formats, and
# removes them), and
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
	CARGO='$(CARGO)' GENERATED='$(abspath $(ALBUM_ASSETS_RS)):$(abspath $(ALBUM_LAYOUT_RS)):$(abspath $(ALBUM_GLYPHS_RS))' sh scripts/fmt_with_generated_stub.sh $(ALBUM_CRATE_DIR)
else
format:
	@echo "format: skipping — no rustfmt for '$(TOOLCHAIN)' (build continues)."
	@echo "  enable with: rustup component add rustfmt --toolchain $(TOOLCHAIN)"
endif

build: format
	$(CARGO) build --release --target $(TARGET_SPEC)

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
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
	agb-gbafix $(FLAC_CRATE_BIN) -o $(FLAC_ROM)
	@echo "FLAC ROM built: $(FLAC_ROM) (manifest facade + ArmRiceKernel; decode-only)"

# The playback image on the committed product asset. It drives
# asset → manifest → Decoder → convert → ring → sound DMA/FIFO at 16,384 Hz,
# counts its own underflows, gates its period audit and its played-form FIFO
# pins, and prints a verdict to the log AND the screen. The image is not gated by
# `make test`: build it, then run it by hand.
product-play-rom: gafp-check format
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --bin product-play
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
	cd $(FLAC_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --bin product-play --features master-asset
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

# Gate 1 is `cargo check --target $(TARGET_SPEC) -Zbuild-std=core,alloc` run IN-TREE,
# where inheriting the root config is the whole point: it is the gate symphonia
# could never pass, because it keeps `std` out. Gate 2 is the host suite run from
# $(GATE_NEUTRAL_CWD) with --manifest-path, so the root's GBA `target` AND
# `build-std` are never inherited at all (an in-tree override is not durable:
# cargo merges config arrays across layers). Every host gate below takes this
# same shape; copy it, do not re-derive it.
flac-test: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc
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
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --no-default-features
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --no-default-features
	@echo "flac-test-wide passed: the i64 reference leg compiles for $(TARGET) and its suite is green"

# Frame CRC verification ON. Verification is OFF in the default build by design
# (release consumes the CRCs unchecked), so this gate is what keeps the OFF
# default honest about the ON arm.
flac-test-crc: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features crc-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check
	@echo "flac-test-crc passed: CRC verification compiles for $(TARGET) and its witness is green"

# The table-less frame walk ON — the code the product facade is built on, so this
# is the walk's thumbv4t compile plus its layout suite.
flac-test-scan: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features scan,instrument
	@echo "flac-test-scan passed: the frame walk compiles for $(TARGET) and its witness suite is green"

# The portable hybrid Rice kernel ON: the second `RiceKernel` mechanism and the
# two-mechanism differential. It stays a feature (and not the default) because
# the on-target reading declined the flip: +42.6 c/sample against the incumbent.
flac-test-rice: toolchain-check
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features rice-hybrid
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
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features crc-check,scan
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(FLAC_CRATE_DIR_MANIFEST) --target $(HOST_TRIPLE) --features crc-check,scan
	cd crates/flac-lite && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features crc-check,scan,rice-hybrid
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
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc
	@echo "overlay-check passed: the overlay AND the core it depends on both type-check for $(TARGET)"

overlay-check-arm: toolchain-check
	cd $(OVERLAY_CRATE_DIR) && \
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features rice-arm
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
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc --features rice-arm
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
	cd $(OVERLAY_ROM_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
	agb-gbafix $(OVERLAY_ROM_DIR)/$(OVERLAY_ROM_BIN) -o $(OVERLAY_ROM)
	sh scripts/check_arm_kernel_objdump.sh $(OVERLAY_ROM_DIR)/$(OVERLAY_ROM_BIN)
	@echo "Kernel-check ROM built: $(OVERLAY_ROM) (its own test asserts the tables' .iwram addresses at runtime)"

overlay-kernel-test: toolchain-check
	cd $(OVERLAY_ROM_DIR) && \
	  CARGO_TARGET_$(TARGET_ENV)_RUNNER=$(GBA_TEST_RUNNER) \
	  $(CARGO) test --target $(TARGET_SPEC)
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
	cd $(SPIKE_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
	agb-gbafix $(SPIKE_CRATE_BIN) -o $(SPIKE_ROM)
	@echo "Spike ROM built: $(SPIKE_ROM)"
	@echo "  run: timeout -k 5 280 $(GBA_TEST_RUNNER) $(SPIKE_ROM) | grep -E 'stats:|budget:|verdict'"

# ===========================================================================
# Other codec tracks (see OPUS.md / ULI2.md — not the FLAC path)
# ===========================================================================

opus-rom: format
	cd $(OPUS_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
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
	  $(CARGO) check --release --target $(TARGET_SPEC) -Zbuild-std=core,alloc
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_CRATE_MANIFEST) --target $(HOST_TRIPLE)
	# The encoder's own suite, which S1's exit gate requires (PLAN-OF-RECORD.md).
	# It was added to `uli-check`'s fmt list and to nothing else, so the round-trip
	# — the one thing S1 exists to provide — ran only when someone remembered to.
	# Same neutral-cwd shape as every host-only workspace above: `crates/uli-enc`
	# is its own cargo workspace, but cargo discovers the root `.cargo/config.toml`
	# from the *cwd*, so building in place inherits the GBA target and `build-std`
	# and fails with ~150 errors that say nothing about the code.
	# Count the suites that actually reported a result, and refuse to pass below the
	# expected number. A `cargo test` whose test binaries fail to *build* can still
	# read as green through a pipe or a grep, which is how a broken S3 suite looked
	# fine twice: the difference between "nothing failed" and "something ran" has to
	# be asserted, not inferred from silence.
	@out=$$(cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_ENC_MANIFEST) --target $(HOST_TRIPLE) 2>&1); \
	status=$$?; \
	printf '%s\n' "$$out" | grep -E 'test result|^error' | sed 's/^/  /'; \
	if [ $$status -ne 0 ]; then echo "uli-enc test run failed (exit $$status)"; exit 1; fi; \
	suites=$$(printf '%s\n' "$$out" | grep -c 'test result: ok'); \
	failed=$$(printf '%s\n' "$$out" | grep -c 'test result: FAILED'); \
	if [ "$$failed" != 0 ]; then echo "uli-enc: $$failed suite(s) FAILED"; exit 1; fi; \
	total=$$(printf '%s\n' "$$out" | sed -n 's/.*test result: ok\. \([0-9]*\) passed.*/\1/p' | awk '{t+=$$1} END {print t+0}'); \
	total=$${total:-0}; \
	if [ "$$total" -lt $(ULI_ENC_MIN_TESTS) ]; then \
	  echo "uli-enc: $$total tests ran, expected >= $(ULI_ENC_MIN_TESTS)"; \
	  echo "  a suite that fails to compile reports nothing, and nothing is not green"; exit 1; fi; \
	echo "  uli-enc: $$suites suites, $$total tests, all passing"
	@echo "uli-test passed: uli-lite compiles for $(TARGET) and is bit-exact on the host corpus; uli-enc round-trips through it"

# Regenerate the reference's default tables blob — the byte-exactness witness the
# encoder's `to_blob` is pinned to. Off-tree by design: this is the only ULI2
# target that compiles anything from `gba-audio`, it is never a prerequisite of a
# gate, and E-R1's prohibition is on the dependency, not on the oracle (the same
# posture as `uli-album-asset`). Run it when the length model moves, read the diff,
# and let `uli-test` be the thing that decides whether the new bytes are right.
uli-default-tables:
	GBA_AUDIO=$(GBA_AUDIO) scripts/gen_uli_default_tables.sh

# The forward-MDCT golden: the reference's `mdct_frame` coefficients for the
# committed test album. Same posture as `uli-default-tables` — oracle off-tree,
# bytes in the tree, no gate builds C.
uli-mdct-fixture:
	GBA_AUDIO=$(GBA_AUDIO) python3 scripts/gen_uli_mdct_fixture.py
	@echo
# The band_rdo differential: 2400 frozen (coefficients, class, step) -> (magnitudes,
# bits, distortion) cases from the reference, so a dropped guard or a re-ordered
# comparison in the quantiser's inner loop shows up as a failing test instead of as a
# whisper in the bitstream. Same off-tree posture as the two goldens above.
uli-band-rdo-fixture:
	GBA_AUDIO=$(GBA_AUDIO) python3 scripts/gen_uli_band_rdo_fixture.py

# S3's acceptance harness: our --rd encoder against the reference's, same PCM, same
# budgets: bytes, fit, kbit/s, SNR, chosen g. Exits nonzero if any row overshoots.
# Deliberately NOT on any gate's path: it needs the off-tree reference binary, and
# E-R1 keeps that off `make uli-test`. Run it when the search changes and record the
# distillation in PLAN-OF-RECORD.md -> S3.
ULI_RD_ALBUM ?= $(ULI_BENCH_DIR)/assets/album.f32
ULI_RD_TRACKS ?= $(ULI_BENCH_DIR)/assets/tracks.tsv
ULI_RD_BUDGETS ?= 400000,120000,40000
# The S7 matrix's corpus. An env var of the same name is imported by make and so wins
# over the `?=`, and an explicit `make uli-accept ULI_ACCEPT_ASSETS=...` wins outright.
ULI_ACCEPT_ASSETS ?= $(ULI_BENCH_DIR)/assets
uli-rd-ab:
	@test -f $(ULI_RD_ALBUM) || { echo "uli-rd-ab: no album at $(ULI_RD_ALBUM) (set ULI_RD_ALBUM=)"; exit 1; }
	@test -x $(GBA_AUDIO)/build/uli_enc || { echo "uli-rd-ab: no reference encoder (make -C $(GBA_AUDIO)/encoder)"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) build --release --manifest-path $(ULI_ENC_MANIFEST) --target $(HOST_TRIPLE) --bins
	GBA_AUDIO=$(GBA_AUDIO) python3 scripts/uli_rd_ab.py \
	  --album $(ULI_RD_ALBUM) --tracks $(ULI_RD_TRACKS) --budgets $(ULI_RD_BUDGETS)
	@echo "now: cd $(GATE_NEUTRAL_CWD) && $(CARGO) test --manifest-path $(ULI_ENC_MANIFEST) --target $(HOST_TRIPLE)"

# S7's acceptance matrix: our --rd encoder and the reference on identical PCM and the
# same flags, across four cells and one oracle-only `--dacw 1` row. Per cell: MATCH or
# DIFF, both blobs' SHA-256 and size, the two header cycle fields, the oracle's codec
# proxy, our mean and worst time-domain SNR, and the searched g range. A DIFF is
# localised to a container region, and a stream DIFF to its first disagreeing seek
# entry. Exits 2 on DIFF, 1 on a harness error.
#
# Deliberately NOT on any gate's path: it runs the off-tree reference binary, and E-R1
# keeps that dependency out of `make uli-test`. It is a host measurement run — take it
# when the search or the container changes, and record the verdict in
# PLAN-OF-RECORD.md -> S7. The corpus defaults to examples/uli_bench/assets; pass
# ULI_ACCEPT_ASSETS= when it lives elsewhere.
uli-accept:
	@test -x $(GBA_AUDIO)/build/uli_enc || { echo "uli-accept: no reference encoder (make -C $(GBA_AUDIO)/encoder)"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) build --release --manifest-path $(ULI_ENC_MANIFEST) --target $(HOST_TRIPLE) --bins
	GBA_AUDIO=$(GBA_AUDIO) ULI_ACCEPT_ASSETS="$(ULI_ACCEPT_ASSETS)" \
	  python3 scripts/uli_accept.py --ours $(ULI_ENC_BIN)

uli-check: toolchain-check
	cd crates/uli-lite && $(CARGO) fmt --check
	cd crates/uli-lite-gba && $(CARGO) fmt --check
	cd crates/uli-enc && $(CARGO) fmt --check
	cd $(ULI_BENCH_DIR) && $(CARGO) fmt --check

uli-bench-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
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
# RECORD.md "PR E" needs a corpus large enough to be a worst-case
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
# Which corpus the bench/gate arms measure. `album-corpus` is PR E's master,
# `adversarial-corpus` is E3-a's synthetic density set, `cross-corpus` is E3-b's
# 12-track album. All three are generated, gitignored assets.
ULI_CORPUS_FEATURE ?= album-corpus
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
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features $(ULI_CORPUS_FEATURE)
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
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features gate,$(ULI_CORPUS_FEATURE)
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
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features play-only
	agb-gbafix $(ULI_BENCH_BIN) -o $(ULI_PLAY_ROM)
	@echo "ULI2 audio-only ROM built: $(ULI_PLAY_ROM)"
	@stat -c '  %s bytes' $(ULI_PLAY_ROM)

ULI_GATE_FIXTURE_ROM := uli-gate-fixture.gba
uli-gate-fixture-rom: format
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features gate
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

# --- PR D: the product player (ULI2.md Phase 2, RECORD.md "PR D") -----
#
# `examples/uli_player` is one paced decode-and-play loop over the
# `uli-playback` seam: the 32,768 Hz timer/DMA/FIFO chain, a live text UI read
# from the ULA2 track table, and the underrun/period ledger U5 is gated on.
# The headphone build ships stereo untouched; `--features speaker` decodes full
# stereo and folds to mono in the output stage (both are measured).
ULI_PLAYER_DIR := examples/uli_player
ULI_PLAYER_BIN := $(ULI_PLAYER_DIR)/target/$(TARGET)/release/uli-player
ULI_PLAYER_ROM := uli-player.gba
ULI_PLAYER_SPK_ROM := uli-player-speaker.gba
ULI_PLAYER_LOG ?= uli-player.log
ULI_PLAYER_SPK_LOG ?= uli-player-speaker.log

uli-player-rom: format
	cd $(ULI_PLAYER_DIR) && $(CARGO) build --release --target $(TARGET_SPEC)
	agb-gbafix $(ULI_PLAYER_BIN) -o $(ULI_PLAYER_ROM)
	@echo "ULI2 player ROM (headphone profile) built: $(ULI_PLAYER_ROM)"

uli-player-speaker-rom: format
	cd $(ULI_PLAYER_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features speaker
	agb-gbafix $(ULI_PLAYER_BIN) -o $(ULI_PLAYER_SPK_ROM)
	@echo "ULI2 player ROM (speaker profile, folded) built: $(ULI_PLAYER_SPK_ROM)"

# A playback ROM loops forever once it passes, so the runner is leashed and the
# verdict is read from the log: PASS requires zero underruns, a clean period
# audit and a worst frame inside the period.
uli-player: uli-player-rom
	timeout -k 5 900 $(GBA_TEST_RUNNER) $(ULI_PLAYER_ROM) 2>&1 | tee $(ULI_PLAYER_LOG) || true
	@grep -q '^\[uli-player\] PASS' $(ULI_PLAYER_LOG) \
	  || { echo "uli-player: not a pass — see $(ULI_PLAYER_LOG)"; exit 1; }
	@grep -E '^\[uli-player\] (profile|played frames|feeder)' $(ULI_PLAYER_LOG) || true
	@echo "uli-player done: $(ULI_PLAYER_LOG)"

uli-player-speaker: uli-player-speaker-rom
	timeout -k 5 900 $(GBA_TEST_RUNNER) $(ULI_PLAYER_SPK_ROM) 2>&1 | tee $(ULI_PLAYER_SPK_LOG) || true
	@grep -q '^\[uli-player\] PASS' $(ULI_PLAYER_SPK_LOG) \
	  || { echo "uli-player-speaker: not a pass — see $(ULI_PLAYER_SPK_LOG)"; exit 1; }
	@grep -E '^\[uli-player\] (profile|played frames|feeder)' $(ULI_PLAYER_SPK_LOG) || true
	@echo "uli-player-speaker done: $(ULI_PLAYER_SPK_LOG)"

# The seam's own host suite (crates/uli-playback): frame-for-frame pins against
# the C reference, the two speaker folds, and the ring ledger's classification.
ULI_PLAYBACK_MANIFEST := $(abspath crates/uli-playback/Cargo.toml)
uli-playback-test: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --manifest-path $(ULI_PLAYBACK_MANIFEST) --target $(HOST_TRIPLE) \
	    > $(abspath uli-playback-test.log) 2>&1; \
	  status=$$?; cat $(abspath uli-playback-test.log); [ $$status -eq 0 ] \
	    || { echo "uli-playback-test: the seam suite did not pass — see uli-playback-test.log"; exit 1; }
	@echo "uli-playback-test passed: seam transparency, fold-down, ring ledger"

# --- PR E's E3: the two pre-hardware corpora, and PR D's witnesses -------------
#
# E3-a: the **adversarial density set** — synthetic content chosen to carry the
# quad-heavy class the real master does not produce. Offline, no network, ten
# seconds, and the pin is C-sourced like every other corpus's.
ULI_ADV_DIR := $(ULI_BENCH_DIR)/assets
ULI_ADV_BLOB := $(ULI_ADV_DIR)/adv.uli
ULI_ADV_REF := $(ULI_ADV_DIR)/adv
ULI_ADV_PIN := $(ULI_BENCH_DIR)/src/album_pin_adv.rs
# 10 s at the product's 12,742 B/s.
ULI_ADV_BUDGET ?= 127500

uli-adversarial-asset:
	mkdir -p $(ULI_ADV_DIR) $(ULI_ADV_REF)
	$(ULI_ALBUM_PY) scripts/gen_adversarial_corpus.py $(ULI_ADV_DIR)/adv 10
	$(GBA_AUDIO)/build/uli_enc --album $(ULI_ADV_DIR)/adv/album.f32 \
	  --tracks $(ULI_ADV_DIR)/adv/tracks.tsv --out $(ULI_ADV_BLOB) \
	  --budget $(ULI_ADV_BUDGET) --cap 74 --threads 4
	$(GBA_AUDIO)/build/uli_dec $(ULI_ADV_BLOB) $(ULI_ADV_REF) --seektest
	$(ULI_ALBUM_PY) scripts/uli_album_pin.py --blob $(ULI_ADV_BLOB) \
	  --ref $(ULI_ADV_REF) --rs $(ULI_ADV_PIN)
	@echo "uli-adversarial-asset: $(ULI_ADV_BLOB) and $(ULI_ADV_PIN) (both gitignored)"

# E3-b: the **sibling project's 12-track album** — 43.5 minutes, 167,222 frames,
# CC-licensed material fetched from the Internet Archive with the sibling's own
# trim recipe (`scripts/uli_cross_album.py`). No blob is committed; the download
# cache lives outside the tree by default.
ULI_CROSS_DIR := $(ULI_BENCH_DIR)/assets/cross
ULI_CROSS_BLOB := $(ULI_CROSS_DIR)/cross.uli
ULI_CROSS_REF := $(ULI_CROSS_DIR)/ref
ULI_CROSS_PIN := $(ULI_BENCH_DIR)/src/album_pin_cross.rs
# 32 MiB less the ~448 KB this instrument's image needs; the sibling's own record
# used 33,292,288, which does not fit beside the instrument (174,716 B over).
ULI_CROSS_BUDGET ?= 33100000
ULI_CROSS_CACHE ?= /tmp/uli-cross-src

uli-cross-asset:
	@test -f $(GBA_AUDIO)/music/tracklist.json || \
	  { echo "uli-cross-asset: no $(GBA_AUDIO)/music/tracklist.json"; exit 1; }
	mkdir -p $(ULI_CROSS_DIR) $(ULI_CROSS_REF)
	$(ULI_ALBUM_PY) scripts/uli_cross_album.py $(GBA_AUDIO)/music/tracklist.json \
	  $(ULI_CROSS_DIR) --cache $(ULI_CROSS_CACHE)
	$(GBA_AUDIO)/build/uli_enc --album $(ULI_CROSS_DIR)/album.f32 \
	  --tracks $(ULI_CROSS_DIR)/tracks.tsv --out $(ULI_CROSS_BLOB) \
	  --budget $(ULI_CROSS_BUDGET) --cap 74 --threads $(ULI_ALBUM_THREADS)
	$(GBA_AUDIO)/build/uli_dec $(ULI_CROSS_BLOB) $(ULI_CROSS_REF) --seektest
	$(ULI_ALBUM_PY) scripts/uli_album_pin.py --blob $(ULI_CROSS_BLOB) \
	  --ref $(ULI_CROSS_REF) --rs $(ULI_CROSS_PIN)
	@echo "uli-cross-asset: $(ULI_CROSS_BLOB) and $(ULI_CROSS_PIN) (both gitignored)"

# PR D's seek witness: every track's `seek_track` reproduces the continuous
# stream, including the `skip` of a track that does not start on a frame boundary.
ULI_TWO_BLOB := $(ULI_BENCH_DIR)/assets/biotwo.uli
uli-two-track-asset:
	mkdir -p $(ULI_BENCH_DIR)/assets/two $(ULI_BENCH_DIR)/assets/two/ref
	$(ULI_ALBUM_PY) $(GBA_AUDIO)/encoder/prep.py $(ULI_ALBUM_WAV) $(ULI_ALBUM_WAV) \
	  --out $(ULI_BENCH_DIR)/assets/two
	$(GBA_AUDIO)/build/uli_enc --album $(ULI_BENCH_DIR)/assets/two/album.f32 \
	  --tracks $(ULI_BENCH_DIR)/assets/two/tracks.tsv --out $(ULI_TWO_BLOB) \
	  --budget 6588000 --cap 74 --threads $(ULI_ALBUM_THREADS)
	$(GBA_AUDIO)/build/uli_dec $(ULI_TWO_BLOB) $(ULI_BENCH_DIR)/assets/two/ref --seektest
	$(ULI_ALBUM_PY) scripts/uli_album_pin.py --blob $(ULI_TWO_BLOB) \
	  --ref $(ULI_BENCH_DIR)/assets/two/ref --rs $(ULI_BENCH_DIR)/src/album_pin_two.rs
	@echo "uli-two-track-asset: $(ULI_TWO_BLOB) (gitignored)"

uli-playback-track-pin:
	@test -f $(ULI_TWO_BLOB) || { echo "uli-playback-track-pin: run make uli-two-track-asset first"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) run --manifest-path $(ULI_PLAYBACK_MANIFEST) --target $(HOST_TRIPLE) \
	    --example track_seek_pin -- $(abspath $(ULI_TWO_BLOB))

# The **full-length album** build: E3-b's twelve tracks (43.5 min, 167,222 frames,
# 33,088,280 B) inside one ROM. This is the only image that tests the cartridge's
# real capacity rather than its stated one — 33.4 MB of a 32 MiB cart, ~160 KB of
# headroom — and it is meant to be run once on hardware, read off the final
# screen (frames, worst/mean, periods, underruns, tails).
uli-player-cross-rom: format
	cd $(ULI_PLAYER_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features cross-corpus
	agb-gbafix $(ULI_PLAYER_BIN) -o uli-player-cross.gba
	@echo "ULI2 player ROM (full-length album) built: uli-player-cross.gba"
	@stat -c '  %s bytes of a 33554432-byte cart' uli-player-cross.gba

uli-player-cross: uli-player-cross-rom
	timeout -k 5 1200 $(GBA_TEST_RUNNER) uli-player-cross.gba > uli-player-cross.log 2>&1 || true
	@grep -q '^\[uli-player\] PASS' uli-player-cross.log \
	  || { echo "uli-player-cross: not a pass — see uli-player-cross.log"; exit 1; }
	@grep -E '^\[uli-player\] (profile|played frames|ui redraws|feeder)' uli-player-cross.log || true

# The player exercising a real track change: two tracks, and a deterministic skip
# partway through track 1 (the runner has no input script).
uli-player-two-track:
	cd $(ULI_PLAYER_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features two-track,autoskip
	agb-gbafix $(ULI_PLAYER_BIN) -o uli-player-two-track.gba
	timeout -k 5 900 $(GBA_TEST_RUNNER) uli-player-two-track.gba > uli-player-two-track.log 2>&1 || true
	@grep -q '^\[uli-player\] PASS' uli-player-two-track.log \
	  || { echo "uli-player-two-track: not a pass — see uli-player-two-track.log"; exit 1; }
	@grep -E '^\[uli-player\] (played frames|ui redraws|feeder)' uli-player-two-track.log || true

# --- ULI2 by-difference attribution (RECORD.md, critical path Step 0) ---
#
# Each arm rebuilds the bench with exactly one part of the product frame changed —
# removed, or (the `*2` arms) called a second time — and dumps the per-frame window
# (`--features attrib-dump`), so the frame can be attributed by difference at a
# *given* frame index rather than only in the mean.
# The REMOVAL arms are TIMING-ONLY: the album pins fail by construction in every
# one of them, so this target does NOT gate on the log's PASS. Read the `frame
# prod8` line and the `pfc` rows, never the pin verdict. `attrib-dump` costs a
# constant +16 cycles per window in this image (control 274,214 vs the product
# 274,198) — deltas are unaffected; quote absolutes from the product arm.
#
# The `*2` arms are the exception and are strictly better: they call a stage a
# **second time** instead of removing it, so they are BIT-EXACT and their pins
# PASS. `pre2`, `post2` and `entropy2` are one call each; `fft2` is `pre`+`fft`,
# because a second `fft` needs its input restored (`fft` is in place). So read
# `fft = delta(fft2) - delta(pre2)`. These are the arms the kernel table is built
# from — the removal arms are contaminated by the downstream data change, measured
# at 1.61x for `pre` at the densest frame.
ULI_ATTRIB_ARMS := control synth pre pre2 fft fft2 post post2 ola ms entropy entropy2 synth-entropy

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
	  ( cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features "$$feats" ) || exit 1; \
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
	  ( cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features "$$feats" ) || exit 1; \
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
	cd $(ULI_BENCH_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features asm-fft,asm-pre,asm-ola,asm-post,asm-ms
	agb-gbafix $(ULI_BENCH_BIN) -o uli-kernel-twin.gba
	timeout -k 5 300 $(GBA_TEST_RUNNER) uli-kernel-twin.gba > $(ULI_FFT_TWIN_LOG) 2>&1 || true
	@grep -E '^\[uli-bench\] asmfft5 ' $(ULI_FFT_TWIN_LOG) || { echo "uli-fft-twin: no asmfft5 arm in the log"; exit 1; }
	@grep -q 'ovl twin .* fftasm=true preasm=true olaasm=true msasm=true postasm=true \[PASS\]' $(ULI_FFT_TWIN_LOG) || { echo "uli-kernel-twin: an asm kernel twin failed — see $(ULI_FFT_TWIN_LOG)"; exit 1; }
	@echo "uli-kernel-twin passed: the asm FFTs (v1..v5), the asm pre-twiddle, the asm post-twiddle, the asm overlap-add and the asm M/S transform are bit-exact on the ARM7TDMI"

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
	cd $(ALBUM_PROBE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --features probe-roms
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
#
# It writes THREE generated modules now: `album_bake.py` writes the blobs, `ALBM1` and
# `src/assets.rs` + `src/layout.rs`, and `album_glyphs.py` writes `src/glyphs.rs`. The atlas runs
# after the layout on purpose (§5.5a step 5): it resolves the face through the variant the bake just
# selected, so `make album-bake ALBUM=<config>` gives one variant to both emitters and the pair
# cannot be baked against different selections.
album-bake: toolchain-check format
	@test -f "$(ALBUM)" || { echo "album-bake: $(ALBUM) does not exist — pass ALBUM=<config> or check the tracked fixture is present"; exit 1; }
	python3 scripts/album_bake.py "$(ALBUM)"
	python3 scripts/album_glyphs.py

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
#
# Note on the third generated module: `src/glyphs.rs` is NOT compared here, because this target
# calls `scripts/album_bake.py` directly, twice, and that emitter does not write the atlas — a copy
# taken around a run that never writes the file is a `cmp` of a file against itself, which is the
# exact vacuity the layout leg below was rewritten to avoid. The atlas's own reproducibility leg is
# `album-glyphs-test`.
#
# Note on the two generated modules: both `assets.rs` and `layout.rs` are snapshotted into
# `$$first` BEFORE the second bake runs, on purpose. A copy taken after it would be the second
# run's own bytes, and `cmp` of a file against itself is green whatever the bake does — the
# layout leg was written that way for one revision and reported a determinism it never
# measured. Both are compared run-1 against run-2, and each comparison is witnessed negatively.
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
	  cp $(ALBUM_LAYOUT_RS) $$first/layout.rs || { echo "album-pack-check: $(ALBUM_LAYOUT_RS) is missing — the bake writes the layout module next to the pin module, and a pack gate that cannot see it cannot call it reproducible"; rm -rf $$first; exit 1; }; \
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
	  cmp -s $$first/layout.rs $(ALBUM_LAYOUT_RS) \
	    || { echo "album-pack-check: the bake is not deterministic — src/layout.rs differs between two runs of the same album.toml"; rm -rf $$first; exit 1; }; \
	  cp $$first/assets.rs $$first/assets.mutant.rs; \
	  python3 -c 'import sys; p = sys.argv[1]; d = bytearray(open(p, "rb").read()); d[0] ^= 0xFF; open(p, "wb").write(d)' $$first/assets.mutant.rs; \
	  cmp -s $$first/assets.mutant.rs $(ALBUM_ASSETS_RS) \
	    && { echo "album-pack-check: the determinism comparison passed two files that differ by one byte — the byte leg is not witnessing anything"; rm -rf $$first; exit 1; } \
	    || echo "  determinism leg: one flipped byte is reported as a difference (negative control)"; \
	  cp $$first/layout.rs $$first/layout.mutant.rs; \
	  python3 -c 'import sys; p = sys.argv[1]; d = bytearray(open(p, "rb").read()); d[0] ^= 0xFF; open(p, "wb").write(d)' $$first/layout.mutant.rs; \
	  cmp -s $$first/layout.mutant.rs $(ALBUM_LAYOUT_RS) \
	    && { echo "album-pack-check: the layout comparison passed two files that differ by one byte — that leg is not witnessing anything"; rm -rf $$first; exit 1; } \
	    || echo "  layout determinism leg: one flipped byte is reported as a difference (negative control)"; \
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
	  [ -n "$$n" ] && [ "$$n" -ge 51 ] || { echo "album-pack-check: collected '$$n' tests, needs >= 51 — a green that names nothing is not a green (the suite is 54 since 2a PR-(2) added the elided-title cross-check; three may vanish before this floor speaks, the same margin the 48 → 50 raise used)"; exit 1; }
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
# The #160 review round adds **§4.4's pause leg**, which had been recorded and never bounded: the model's
# own pause continuation is now held to `pause_to_freeze_bound` rather than to the literal 4 it capped itself
# with, and a second witness drives `tick()` at the cadence of a 1/2/3/8-period pass and reads the leg as
# `max(pass, FADE_PERIODS)` — the later of the deferral's floor and the main loop's turn — inside that bound.
# The pass term is the measured quantity, so the bound moves with the machine instead of against it (R13), and
# the equality test is what shows that term is load-bearing rather than slack.
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
	  [ -n "$$n" ] && [ "$$n" -ge 68 ] || { echo "album-seam-test: collected '$$n' tests, needs >= 68 — a green that names nothing is not a green"; exit 1; }
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
	@echo "  the source's own, contiguous, with only the two declared ramp windows differing. PR-3c's starved"
	@echo "  resume is tested on the host because no ROM arm this repo has measured has ever needed it: the press"
	@echo "  is routed by what the ring answers (its verdict compared against take_chunk at every boundary depth),"
	@echo "  a ring that cannot prime refills to PREFILL_SAMPLES in PREFILL_FRAMES passes with no period served,"
	@echo "  no clock movement and no dropped sample, its stream still the source's byte for byte, a ring that"
	@echo "  can prime pays nothing for that branch, and the wait holds inside resume_to_audio_bound at four pass"
	@echo "  lengths while the state that feeds the ring is kept apart from the state a pass may be timed on."

# The layout generator's own gate (2a PR-1: §11's 2a row, §9.0's register, D9). `album-layout-gen`
# is a step inside `scripts/album_bake.py` and `make album-bake` already runs it, so this target
# exists to make its *test* reachable by name — it is host-only, needs no audio and no ROM, and it
# is deliberately not a prerequisite of `album-rom`: a ROM build gets the layout through the bake,
# and this is the half that says the generator answers for its own numbers. Three legs:
#   1. the tracked spec exists exactly once, so the gate cannot be green against a spec that went
#      missing or was duplicated (the same shape as the two host gates' input legs);
#   2. `--layout-self-test` exits 0 AND the log names what it proved: every §9.0 register row fired
#      by id (9 of 9), no failed summary line, and a floor on the reproduction legs — because an
#      exit status alone would be satisfied by a run that measured nothing. Each leg of that run
#      prints where its expected number comes from (`plan` / `spec` / `arithmetic`), which is what
#      D9 asks for; a leg whose two sides are the same path is not evidence;
#   3. the generated module is byte-reproducible, the same way `album-pack-check` holds assets.rs:
#      emit twice, compare, and witness the comparison negatively with one flipped byte;
#   4. the Rust half — the three claims a Python tool cannot make: that `meta::parse` refuses a
#      manifest whose grid is not the one the generated module declares (the boot backstop, run
#      against the real baked blob), that the fixture's titles are measured against the declared
#      box from the generated advance table rather than counted, that none of them is drawn with
#      §5.5's elision marker at that box while a synthetic over-long string is (both measurement
#      paths, agreeing on the same strings), and that the two generated modules name one variant
#      (`assets::LAYOUT_VARIANT` ↔ `layout::LAYOUT_ID`) — the staleness a
#      compiling build cannot see, and the one `assets.rs`'s own comment promises a host test
#      asserts. The filters are named so a leg that vanished from the suite fails here rather than
#      passing for lack of a `--filter` match.
album-layout-test: toolchain-check album-bake
	@test $$(find $(ALBUM_CRATE_DIR) -maxdepth 1 -name 'layout_spec.toml' | wc -l) -eq 1 \
	  || { echo "album-layout-test: $(ALBUM_LAYOUT_SPEC) is missing or duplicated — one tracked layout spec, or this gate is measuring nothing"; exit 1; }
	@command -v python3 >/dev/null 2>&1 \
	  || { echo "album-layout-test: 'python3' is not on PATH — the generator and its self-test are host tools"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && python3 $(abspath scripts/album_bake.py) --layout-self-test > $(abspath $(ALBUM_LAYOUT_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  tail -n 25 $(abspath $(ALBUM_LAYOUT_GATE_LOG)); \
	  [ $$status -eq 0 ] \
	    || { echo "album-layout-test: the layout self-test refused — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  grep -q '^  register rows fired: 9 of 9$$' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) \
	    || { echo "album-layout-test: §9.0's register did not fire all of its 2a rows by id — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  ! grep -q 'SELF-TEST FAILED' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) \
	    || { echo "album-layout-test: a reproduction leg failed — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  n=$$(grep -c '  ok   <-' $(abspath $(ALBUM_LAYOUT_GATE_LOG))); \
	  [ "$$n" -ge 100 ] || { echo "album-layout-test: '$$n' legs passed, needs >= 100 — a green that names nothing is not a green"; exit 1; }
	@first=$$(mktemp -d); \
	  python3 scripts/album_bake.py --emit-layout >/dev/null || { rm -rf $$first; exit 1; }; \
	  cp $(ALBUM_LAYOUT_RS) $$first/layout.rs || { rm -rf $$first; exit 1; }; \
	  python3 scripts/album_bake.py --emit-layout >/dev/null || { rm -rf $$first; exit 1; }; \
	  cmp -s $$first/layout.rs $(ALBUM_LAYOUT_RS) \
	    || { echo "album-layout-test: the generated src/layout.rs differs between two runs — it is not reproducible"; rm -rf $$first; exit 1; }; \
	  cp $$first/layout.rs $$first/layout.mutant.rs; \
	  python3 -c 'import sys; p = sys.argv[1]; d = bytearray(open(p, "rb").read()); d[0] ^= 0xFF; open(p, "wb").write(d)' $$first/layout.mutant.rs; \
	  cmp -s $$first/layout.mutant.rs $(ALBUM_LAYOUT_RS) \
	    && { echo "album-layout-test: the layout comparison passed two files that differ by one byte — the byte leg is not witnessing anything"; rm -rf $$first; exit 1; } \
	    || echo "  layout reproducibility leg: one flipped byte is reported as a difference (negative control)"; \
	  rm -rf $$first
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_pack_host -- bar >> $(abspath $(ALBUM_LAYOUT_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] && grep -q '^test result: ok\.' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) \
	    || { tail -n 40 $(abspath $(ALBUM_LAYOUT_GATE_LOG)); echo "album-layout-test: the bar-grid host legs did not pass — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  n=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) | tail -1); \
	  [ -n "$$n" ] && [ "$$n" -ge 4 ] || { echo "album-layout-test: collected '$$n' bar-grid tests, needs >= 4 — the boot backstop's legs are not in the suite any more"; exit 1; }
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_pack_host -- title >> $(abspath $(ALBUM_LAYOUT_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] \
	    || { tail -n 40 $(abspath $(ALBUM_LAYOUT_GATE_LOG)); echo "album-layout-test: the measured-title host legs did not pass — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  m=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) | tail -1); \
	  [ -n "$$m" ] && [ "$$m" -ge 4 ] || { echo "album-layout-test: collected '$$m' title tests, needs >= 4 — a filter that matched nothing is a green that names nothing"; exit 1; }
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_pack_host -- variant >> $(abspath $(ALBUM_LAYOUT_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] \
	    || { tail -n 40 $(abspath $(ALBUM_LAYOUT_GATE_LOG)); echo "album-layout-test: the pinned-variant host leg did not pass — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  v=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) | tail -1); \
	  [ -n "$$v" ] && [ "$$v" -ge 1 ] || { echo "album-layout-test: collected '$$v' variant-agreement tests, needs >= 1 — the generated assets.rs names this leg in its own `LAYOUT_VARIANT` comment, so it may not vanish quietly"; exit 1; }
	@cd $(GATE_NEUTRAL_CWD) && \
	  $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --test album_surface_host >> $(abspath $(ALBUM_LAYOUT_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] \
	    || { tail -n 60 $(abspath $(ALBUM_LAYOUT_GATE_LOG)); echo "album-layout-test: the surface's host legs did not pass — see $(ALBUM_LAYOUT_GATE_LOG)"; exit 1; }; \
	  s=$$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' $(abspath $(ALBUM_LAYOUT_GATE_LOG)) | tail -1); \
	  [ -n "$$s" ] && [ "$$s" -ge 11 ] || { echo "album-layout-test: collected '$$s' surface tests, needs >= 11 — the boot frame, the redraw rule, the clock, the bar, the line cut, the tile indices, the tiles, the palettes and the two rectangles are the whole of what 2a draws"; exit 1; }
	@echo "album-layout-test passed: §9.0's nine fireable 2a refusals each fired by id on a spec written"
	@echo "  to be wrong about it, the measured face/interaction geometry reproduces from the tracked files"
	@echo "  with every expected number labelled by its source, the declared bar grid reaches the ALBM1 header"
	@echo "  and the generated module and the three agree (and disagree by name when they do not), the two"
	@echo "  generated modules name one variant, and none of the four retired character-count names survives"
	@echo "  in the product or in its tests (the verdict line's measured \`text::COLUMNS = 30\` is a different"
	@echo "  bound, and §5.5 says so). Every declared face is tracked with its licence, and src/layout.rs is"
	@echo "  byte-identical across two runs, the comparison witnessed negatively. The surface's own legs read"
	@echo "  the map writes a frame produces: the boot frame is the declared band stack with the title, the"
	@echo "  clock, the state word and five icons in their slots; a still frame writes nothing and a track"
	@echo "  change writes one element; the clock bumps on whole seconds and restarts on a new visit; every"
	@echo "  bar fill position draws its own pixel count; and no drawn line is longer than its box."
	@echo "  See $(ALBUM_LAYOUT_GATE_LOG) for the legs and their sources."

# The glyph atlas's own gate (§5.5a PR-2: its steps 2-5, and the §9.0 register rows this tool adds).
# Deliberately a target of its own rather than more legs inside `album-layout-test`, and the reason
# is §9.0's own rule — a gate's critical path may not run through a tool it has not built. The
# layout gate's inputs are all in-tree TOML and its own emitter; nothing there needs fontTools, and
# adding a font dependency to it would make a layout refusal cost a font install. The atlas is a
# second emitter over the same tracked face, so what it has to prove is not that the face resolves
# (the layout gate proves that) but that the atlas and the layout table cannot drift. Four legs:
#   1. the inputs are present and single: exactly one tracked `layout_spec.toml` (the atlas resolves
#      the face through `album_bake.layout_for`, so that file is this gate's input too), and
#      python3 with fontTools importable — reported by name, as everywhere else here;
#   2. `--self-test` exits 0 AND says what it proved: the leg count is read out of the log and
#      floored, no line may report FAIL, and the summary must report zero failures — an exit status
#      alone would be satisfied by a run that measured nothing. Four of this tool's refusal ids are
#      fired on fixture specs written to be wrong about them (a variant reaching for a second face in
#      a static atlas; a raster off the tile grid; a contour that does not fit its cell; a bitmap
#      face with no outlines to scanline); a fifth, `atlas-ink-not-matching-outline`, is the tool's
#      own bug path and is printed as named-but-not-fired rather than hidden. The elision half of
#      §5.5 is legged here too, on the things only this tool can see: that the px path (`measure`,
#      summing the slots the pixels come from) and the font-unit path (`album_bake.FaceMetrics`) are
#      one number on this face because every advance lands on the pixel grid; that the case table it
#      writes into the module fits, is maximal, is labelled with the declared element each box belongs
#      to, and actually reaches the box of the element that draws titles rather than only the widest
#      text box in the variant; that a wider declared box never draws a shorter line, scanned 1 px at
#      a time down to the point where the bake would refuse the box instead; and that no box this
#      variant declares is that narrow.
#   3. `--check` passes against the module `make album-bake` just wrote (§5.5a step 5's named
#      check), and the module is byte-reproducible across two emits — copied BEFORE the second
#      emit, compared after it, and witnessed negatively with one flipped byte;
#   4. the Rust half, which no Python leg can make: the generated module COMPILES for the host, so
#      its `const _: () = assert!` items run in the compiler — the atlas's own invariants (every
#      slot index in range, the atlas length equal to cells × tiles-per-row × 8 words, the declared
#      codepoints strictly increasing, the blank sentinel exactly at U+0020) and the six it restates
#      from `src/layout.rs` (`FACES[0]`'s id and raster, `LINE_PX`, `TILE_ROWS`, `OUTLINED_GLYPHS`,
#      `ATLAS_TILES`), plus the four text elements' pitch-vs-line-box check. A generator that
#      changed the face, the cut or the line box and left the other module behind is a build
#      failure, not a review finding. The ARM half of the module (`vram::tile_set`, the
#      `TileSet::new` static-lifetime contract) is compiled by `album-rom`, where agb is in scope.
#   5. the module's OWN suite runs, filtered to `glyphs::` with a floor on the count. The filter is
#      what the floor is for: the generator writes `#[cfg(test)] mod tests` into the artifact,
#      twelve of them, and a filter that matched none of them is green (§9.0's rule). They check the
#      module's API, which is what a caller in surface.rs will rely on: that the fixture the 4bpp
#      bit order is read through really is asymmetric (its mirror must NOT match), that no two
#      glyphs share a cell, that the blank sentinel is agb's and that only space uses it, that a
#      character outside the cut is a `None` and a count rather than a zero-width drawing, that
#      `measure` and `tilemap_px` are different numbers for a proportional face, and — the §5.5
#      half — that `elide` cuts exactly where `album_bake.FaceMetrics.elide` said it would on every
#      pinned case (three real boxes: the title element's, the widest and the narrowest text box in
#      the variant), that the cut is at the last whole glyph that fits rather than an earlier one,
#      that the marker is three periods this cut carries and not a U+2026 this cut does not, that no
#      declared box produces the degenerate answer, and that `fit` keeps "unwritten" and "overflows"
#      two separate numbers.
ALBUM_GLYPHS_TEST_FLOOR ?= 10
album-glyphs-test: toolchain-check album-bake
	@test $$(find $(ALBUM_CRATE_DIR) -maxdepth 1 -name 'layout_spec.toml' | wc -l) -eq 1 \
	  || { echo "album-glyphs-test: $(ALBUM_LAYOUT_SPEC) is missing or duplicated — the atlas resolves its face through the same tracked spec, so this gate is measuring nothing without it"; exit 1; }
	@command -v python3 >/dev/null 2>&1 \
	  || { echo "album-glyphs-test: 'python3' is not on PATH — the atlas generator and its self-test are host tools"; exit 1; }
	@python3 -c 'import fontTools' >/dev/null 2>&1 \
	  || { echo "album-glyphs-test: fontTools is not importable — §5.5a step 1, and the generator reads the tracked .ttf through it"; exit 1; }
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@cd $(GATE_NEUTRAL_CWD) && python3 $(abspath scripts/album_glyphs.py) --self-test > $(abspath $(ALBUM_GLYPHS_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  tail -n 30 $(abspath $(ALBUM_GLYPHS_GATE_LOG)); \
	  [ $$status -eq 0 ] \
	    || { echo "album-glyphs-test: the atlas generator's self-test refused — see $(ALBUM_GLYPHS_GATE_LOG)"; exit 1; }; \
	  ! grep -q 'FAIL' $(abspath $(ALBUM_GLYPHS_GATE_LOG)) \
	    || { echo "album-glyphs-test: a self-test leg reported FAIL — see $(ALBUM_GLYPHS_GATE_LOG)"; exit 1; }; \
	  n=$$(sed -n 's/^legs: \([0-9][0-9]*\), failures: 0$$/\1/p' $(abspath $(ALBUM_GLYPHS_GATE_LOG)) | tail -1); \
	  [ -n "$$n" ] && [ "$$n" -ge 30 ] \
	    || { echo "album-glyphs-test: '$$n' legs passed, needs >= 30 — a green that names nothing is not a green"; exit 1; }
	@cd $(GATE_NEUTRAL_CWD) && python3 $(abspath scripts/album_glyphs.py) --check >> $(abspath $(ALBUM_GLYPHS_GATE_LOG)) 2>&1 \
	  || { tail -n 20 $(abspath $(ALBUM_GLYPHS_GATE_LOG)); echo "album-glyphs-test: $(ALBUM_GLYPHS_RS) is not what this tree derives from its tracked face — §5.5a step 5's check"; exit 1; }
	@first=$$(mktemp -d); \
	  python3 scripts/album_glyphs.py --emit >/dev/null || { rm -rf $$first; exit 1; }; \
	  cp $(ALBUM_GLYPHS_RS) $$first/glyphs.rs || { rm -rf $$first; exit 1; }; \
	  python3 scripts/album_glyphs.py --emit >/dev/null || { rm -rf $$first; exit 1; }; \
	  cmp -s $$first/glyphs.rs $(ALBUM_GLYPHS_RS) \
	    || { echo "album-glyphs-test: the generated src/glyphs.rs differs between two runs — it is not reproducible"; rm -rf $$first; exit 1; }; \
	  cp $$first/glyphs.rs $$first/glyphs.mutant.rs; \
	  python3 -c 'import sys; p = sys.argv[1]; d = bytearray(open(p, "rb").read()); d[0] ^= 0xFF; open(p, "wb").write(d)' $$first/glyphs.mutant.rs; \
	  cmp -s $$first/glyphs.mutant.rs $(ALBUM_GLYPHS_RS) \
	    && { echo "album-glyphs-test: the atlas comparison passed two files that differ by one byte — the byte leg is not witnessing anything"; rm -rf $$first; exit 1; } \
	    || echo "  atlas reproducibility leg: one flipped byte is reported as a difference (negative control)"; \
	  rm -rf $$first
	@cd $(GATE_NEUTRAL_CWD) && $(CARGO) build --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) >> $(abspath $(ALBUM_GLYPHS_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] \
	    || { tail -n 40 $(abspath $(ALBUM_GLYPHS_GATE_LOG)); echo "album-glyphs-test: the generated atlas does not compile for the host, so its const asserts against src/layout.rs never ran — see $(ALBUM_GLYPHS_GATE_LOG)"; exit 1; } \
	    || echo "  cross-module const asserts: compiled, against the src/layout.rs this bake wrote"
	@cd $(GATE_NEUTRAL_CWD) && $(CARGO) test --release --manifest-path $(ALBUM_MANIFEST) --target $(HOST_TRIPLE) --lib -- glyphs:: >> $(abspath $(ALBUM_GLYPHS_GATE_LOG)) 2>&1; \
	  status=$$?; \
	  [ $$status -eq 0 ] \
	    || { tail -n 40 $(abspath $(ALBUM_GLYPHS_GATE_LOG)); echo "album-glyphs-test: the generated atlas's own suite failed — see $(ALBUM_GLYPHS_GATE_LOG)"; exit 1; }; \
	  n=$$(grep -c '^test glyphs::tests::' $(abspath $(ALBUM_GLYPHS_GATE_LOG)) 2>/dev/null || echo 0); \
	  [ "$$n" -ge $(ALBUM_GLYPHS_TEST_FLOOR) ] \
	    || { echo "album-glyphs-test: $$n tests from src/glyphs.rs ran, needs >= $(ALBUM_GLYPHS_TEST_FLOOR) — the filter matched nothing"; exit 1; }; \
	  echo "  the module's own suite: $$n tests, floor $(ALBUM_GLYPHS_TEST_FLOOR) — the bit-order fixture proved asymmetric, one cell per glyph, agb's blank tile, no slot outside the cut, measure against tilemap_px, and §5.5's elide cut where the bake's own elide cut"
	@echo "album-glyphs-test passed: the atlas generator's self-test is green, and the log names every leg it"
	@echo "  ran with the source of its expected number — the fontTools preamble, the scanline raster against a"
	@echo "  hand-computed square at a scale no float would agree with, the 8×8 bit order both ends of the wire"
	@echo "  read, the 4bpp word assembly, the even-odd/non-zero fill rules measured to agree on the tracked cut,"
	@echo "  the pool term as max/present/absent, and four of §9.0's register ids fired on specs written to be"
	@echo "  wrong about them. src/glyphs.rs is byte-identical across two emits, the comparison witnessed"
	@echo "  negatively, and it matches what this tree's tracked face derives. The module compiles, which is"
	@echo "  where its counts are tied to src/layout.rs's: one face id, one raster, one line box, one outlined"
	@echo "  count, or the build fails. The module's own tests then run: the fixture the 4bpp bit"
	@echo "  order is read through is named, and proved asymmetric by the mirror of it failing to match; and"
	@echo "  §5.5's elision cuts where the bake's Python copy of the rule said, at the last whole glyph,"
	@echo "  with a marker this cut carries. No cycle figure is claimed by this gate: the atlas's cost is"
	@echo "  the 6,016 bytes of tile data its own length asserts, and a draw cost would be a measurement"
	@echo "  of a draw path that PR-(3) has not written yet."
	@echo "  See $(ALBUM_GLYPHS_GATE_LOG) for the legs and their sources."

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
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --bin album-play
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
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --bin album-play \
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
	cd $(ALBUM_CRATE_DIR) && $(CARGO) build --release --target $(TARGET_SPEC) --bin album-play --features "input-replay deep-audit"
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
	  grep -Eq '^\[album-rom\] pause: pairs=[1-9][0-9]* toggles=[0-9][0-9]* ignored=[0-9][0-9]* refills=[0-9][0-9]* limit=[0-9][0-9]* bound=[0-9][0-9]* resume bound=([0-9][0-9]*|not measured on this arm) \[PASS\]$$' $(ALBUM_TEST_LOG) \
	    || { echo "album-test: the pause summary is not [PASS], or is missing the refills=/resume bound= terms PR-3c prints — an image whose ledger cannot say whether a resume had to wait is not a PR-3c image"; exit 1; }; \
	  refills=$$(sed -n 's/.*pause: pairs=[0-9][0-9]* toggles=[0-9][0-9]* ignored=[0-9][0-9]* refills=\([0-9][0-9]*\) .*/\1/p' $(ALBUM_TEST_LOG)); \
	  short=$$(grep -c '^\[album-rom\] pause [0-9].*primable=no' $(ALBUM_TEST_LOG) || true); \
	  [ -n "$$refills" ] && [ "$$refills" -eq "$$short" ] \
	    || { echo "album-test: the pause summary counts $$refills starved resume(s) while $$short row(s) print primable=no — the count and the rows have to be one story, because a branch this ledger never prints is not a branch that never happened"; exit 1; }; \
	  prow=$$(grep -c '^\[album-rom\] pause [0-9]' $(ALBUM_TEST_LOG) || true); \
	  [ "$$prow" -eq "$$pairs" ] \
	    || { echo "album-test: $$prow pause row(s) for pairs=$$pairs — a pair printed no row, or a row is not a pair"; exit 1; }; \
	  bad=$$(grep '^\[album-rom\] pause [0-9]' $(ALBUM_TEST_LOG) | grep -Evc 'action→freeze=[0-9][0-9]* periods \(bound [0-9][0-9]*\) gain=0 committed=[0-9][0-9]* ; resume@frame=[0-9][0-9]* committed=[0-9][0-9]* dropped=0 (primable=yes resume→arm=0 frames|primable=no resume→arm=[0-9][0-9]* frames) \(bound ([0-9][0-9]*|not measured on this arm)\) \[PASS\]$$' || true); \
	  [ "$$bad" -eq 0 ] \
	    || { echo "album-test: $$bad pause row(s) did not reach gain=0 before the freeze, booked dropped!=0 across the pause, or did not print its primable= and resume→arm= pair against a bound — the row, not only the summary, is read here"; exit 1; }; \
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
	@echo "  Every pause ROW is read, not only its summary: gain=0 at the freeze, dropped=0 across the pause, and the"
	@echo "  action->freeze leg inside pause_to_freeze_bound — the deferral's own periods, one period of boundary"
	@echo "  alignment, and one main-loop pass, because a pause is frozen by the loop rather than by the period ISR."
	@echo "  PR-3c's half of the row is read as well: primable=yes|no on every row, refills= in the summary, and the"
	@echo "  two cross-checked against each other — a branch this ledger never prints is not a branch that never"
	@echo "  happened. Each row's resume→arm is held against resume_to_audio_bound (the prefill's own decode at this"
	@echo "  run's measured pass price), and a direct arm — primable=yes — is required to take zero frames for it. On"
	@echo "  the legs recorded so far no resume has needed the refill, and refills=0 is how the gate says that out loud."
	@echo "  The first two were arms of a clock-delta tautology: they showed the chain had stopped, not that no sample"
	@echo "  was lost at the wire, and the leg they left with no bound at all is bounded by this run's own measurement."
	@echo "  Every latency leg is read and inside its bound: press->detect <= 1 frame, detect->take <= the run's own"
	@echo "  worst main-loop pass (by latch), action@next-period <= 3 frames, the pass bound = this run's measured"
	@echo "  producer frame + one period + one vblank, a skip's take->hand-off <= this run's measured pass + 1 period,"
	@echo "  a pause's action->freeze <= that bound + FADE_PERIODS, and the ISR bracket < 16 samples of the audio"
	@echo "  clock. Printed-row counts are checked against the census"
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
	  $(CARGO) test --target $(TARGET_SPEC)

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
	@rm -f $(ALBUM_BLOB) $(ALBUM_ASSETS_DIR)/*.gfp $(ALBUM_ASSETS_RS) $(ALBUM_LAYOUT_RS) $(ALBUM_GLYPHS_RS) $(ALBUM_GATE_LOG) $(ALBUM_SEAM_GATE_LOG) $(ALBUM_LAYOUT_GATE_LOG) $(ALBUM_GLYPHS_GATE_LOG) $(ALBUM_ROM_RUN_LOG) $(ALBUM_TEST_LOG)
	@rm -f $(ALBUM_PREVIEW)-*.png $(ALBUM_PREVIEW)-*.png.txt
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
	CARGO='$(CARGO)' GENERATED='$(abspath $(ALBUM_ASSETS_RS)):$(abspath $(ALBUM_LAYOUT_RS)):$(abspath $(ALBUM_GLYPHS_RS))' sh scripts/fmt_with_generated_stub.sh $(ALBUM_CRATE_DIR) --check
	@echo "check: all workspaces rustfmt-clean"

# The target spec is an *instrument*, so it gets a witness like the toolchain pin
# does. `targets/$(TARGET).json` must stay a byte-for-byte copy of the pinned
# toolchain's own `--print target-spec-json` output for `thumbv4t-none-eabi`
# except for the fields listed here; anything else drifting means a toolchain
# move changed the ABI, the data layout or the linker flavour underneath us and
# the spec is now lying about the target it claims to be.
#
# This does NOT run as a build pre-step: it costs a rustc invocation, and a
# stale-but-consistent spec still builds a consistent (if unintended) target.
# Run it when the pin moves — `make check` covers formatting, this covers the
# target. See `targets/README.md`.
SPEC_ALLOWED_DIFFS := frame-pointer
TARGET_SPEC_CHECK := scripts/check_target_spec.py
target-spec-check: toolchain-check
	@mkdir -p $(GATE_NEUTRAL_CWD)
	@$(RUSTC) --print target-spec-json -Z unstable-options --target thumbv4t-none-eabi \
	  > $(GATE_NEUTRAL_CWD)/builtin-$(TARGET).json 2>/dev/null \
	  || { echo "target-spec-check: the pinned rustc cannot dump the built-in spec"; exit 1; }
	python3 $(TARGET_SPEC_CHECK) $(GATE_NEUTRAL_CWD)/builtin-$(TARGET).json \
	  targets/$(TARGET).json --allow-diff $(SPEC_ALLOWED_DIFFS)
	@echo "target-spec-check passed: targets/$(TARGET).json differs from the pinned built-in spec only in [$(SPEC_ALLOWED_DIFFS)]"

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
	@echo "    ULI2 (ULI2.md — the repository's product codec). CI-shaped gates, cheapest first:"
	@echo "      uli-check            rustfmt on the four ULI2 workspaces"
	@echo "      uli-test             the core is bit-exact on the host corpus (U2), and"
	@echo "                           uli-enc's blob round-trips through it"
	@echo "      uli-default-tables   regenerate the reference default-tables golden"
	@echo "      uli-mdct-fixture     regenerate the reference forward-MDCT coefficients"
	@echo "      uli-band-rdo-fixture regenerate the reference band_rdo differential"
	@echo "      uli-rd-ab            S3 gate: ours vs the reference, same PCM and budgets"
	@echo "      uli-accept           S7 matrix: four cells + a --dacw 1 row; MATCH, or DIFF"
	@echo "                           localised to a container region (needs the off-tree oracle)"
	@echo "      uli-overlay-test     product path vs the portable reference, both builds, + premise"
	@echo "      uli-fixture          regenerate the committed expected-vector file"
	@echo "      uli-bench            build + run + read PASS: 129-frame cycle/witness image"
	@echo "      uli-kernel-twin      the asm kernels are bit-exact on the ARM7TDMI"
	@echo "      uli-gate-fixture     the gate total on the fixture (~2 s); uli-gate is the album (~2 min)"
	@echo "    ULI2 player (the shipped path): uli-player (headphone), uli-player-speaker (folded),"
	@echo "      uli-playback-test (the ring/feeder host suite), uli-play-rom (audio-only listen image)"
	@echo "    ULI2 album-scale witnesses (need ffmpeg + the sibling gba-audio checkout; assets are"
	@echo "      gitignored build output): uli-album-asset, uli-adversarial-asset, uli-cross-asset,"
	@echo "      uli-two-track-asset, uli-playback-track-pin; then uli-album-bench (the tail),"
	@echo "      uli-gate (the gate total: decode + convert + IRQ + DMA), uli-player-cross,"
	@echo "      uli-player-two-track. ULI_CORPUS_FEATURE= selects the corpus for the bench/gate."
	@echo "    ULI2 attribution (measurement instruments, NOT gates — pins fail by design; read the"
	@echo "      timed window, never the verdict): uli-attrib, uli-attrib-subframe, uli-frame-ops"
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
	@echo "      album-layout-test (2a's layout gate: the generator's own self-test with §9.0's nine"
	@echo "      fireable refusals fired by id and the [meta] pool guard asked at its own error case, the"
	@echo "      generated src/layout.rs byte-identical across two emits, and the Rust half — meta.rs's"
	@echo "      boot backstop, the measured title legs and the elision legs, and the two generated modules naming one variant;"
	@echo "      host-only, and not a prerequisite of album-rom, which gets its layout through album-bake),"
	@echo "      album-glyphs-test (§5.5a's atlas gate: the generator's own self-test, four of §9.0's tool-"
	@echo "      local refusal ids fired by name — a second face in a static atlas, an off-grid raster, a"
	@echo "      contour outside its cell, a bitmap face — the generated src/glyphs.rs byte-identical across two"
	@echo "      emits and matching this tree's tracked face, the module compiling so its const asserts tie"
	@echo "      it to src/layout.rs (one face, one raster, one line box, one outlined count), and the module's"
	@echo "      own floored suite, which is where §5.5's one-line elision is checked against the bake's copy of"
	@echo "      the rule; host-only, and not"
	@echo "      a prerequisite of album-rom, which gets its atlas through album-bake),"
	@echo "      album-rom -> $(ALBUM_ROM) (bake + the audited ROM: all five verdict terms true; run it"
	@echo "      with mgba-test-runner and read the [album-rom] ledger — exit 124 IS the pass),"
	@echo "      album-player-rom -> $(ALBUM_FAST_ROM) (the same ROM built without deep-audit: no cold"
	@echo "      re-decode, no asset fold, and the verdict says pcm/fifo/assets=SKIPPED),"
	@echo "      album-test -> $(ALBUM_REPLAY_ROM) (AP-1's gate: the REPLAY arm run headless to the album's"
	@echo "      end; every gated term, skip, pause and track-end advance asserted from $(ALBUM_TEST_LOG),"
	@echo "      exit 124 being the pass path)."
	@echo "      album-preview -> $(ALBUM_PREVIEW)-1x.png + -4x.png (the surface composited on the host: band"
	@echo "      stack, title, clock, bar, the blank region and the five authored icons, from the same Write"
	@echo "      list the ROM's map gets. An instrument for §5.6's human witness, not a gate),"
	@echo "      and the same again as a 30x20 character grid in the .txt beside each."
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

# §9.1's `make album-preview LAYOUT=… TITLE=…`: one variant's surface, composited on the host at 1x
# and 4x, as PNG plus the 30x20 character grid beside each.
#
# **Why it exists now, in this phase** (2a PR-(3)): §5.6 makes the legibility witness a person, and
# the first hardware run of this phase found a bug a picture would have shown in a second — every
# glyph lost its bottom tile row, and the map sent glyph indices to the chrome tileset, so the screen
# held correct positions with garbage glyphs. Neither was visible in a log. This target renders the
# same `Write` list the ROM's map gets, from the same tiles and the same palette table, so a layout
# mistake costs a host run rather than a flashing session.
#
# It is an *instrument*, not a witness: §5.6's rule is judged on a real screen by a colourblind
# reader, and AP-3's host snapshot remains the geometry leg. What it shares with the ROM is the code
# that decides the cells, so a picture that is wrong here is wrong on hardware for the same reason.
album-preview: album-bake
	@cd $(GATE_NEUTRAL_CWD) && $(CARGO) run --quiet --release --manifest-path $(ALBUM_MANIFEST) \
	  --target $(HOST_TRIPLE) --bin album-preview -- $(abspath $(ALBUM_PREVIEW))
	@ls -l $(abspath $(ALBUM_PREVIEW))-1x.png $(abspath $(ALBUM_PREVIEW))-4x.png
	@echo "  look: 1x is the screen as a person sees it (240x160); 4x is the same frame for judging"
	@echo "  a glyph. The .txt beside each is the map, one character per cell. Not tracked, not gated."
