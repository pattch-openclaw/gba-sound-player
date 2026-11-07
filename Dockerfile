# GBA ROM build + TEST image (Debian 12, Rust nightly — a PINNED dated channel,
# the same one rust-toolchain.toml pins).
#
# Target: thumbv4t-none-eabi — Tier-3, #![no_std] (core + alloc only). It ships
# NO prebuilt core/std artifacts on ANY rustup toolchain, so `rustup target add
# thumbv4t-none-eabi` will FAIL. Do not do that.
#
# Instead we compile core/alloc FROM SOURCE for the target with `-Zbuild-std`,
# which requires (all three):
#   1. a NIGHTLY toolchain (-Z flags are nightly-only),
#   2. the `rust-src` component (provides the std/core source to compile),
#   3. the build flag:  cargo build -Zbuild-std=core,alloc --target thumbv4t-none-eabi
#
#   Ref: https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html
#

# ==============================================================================
# Stage 1: Base (pure Rust)
# ==============================================================================
FROM rust:slim-bookworm AS base

# This project uses `agb`, which links with Rust's BUILT-IN linker (rust-lld)
# via the `-Clink-arg=-Tgba.ld` script — it does NOT use an ARM cross-compiler
# (no `arm-none-eabi-ld`, no `arm-none-eabi-gcc`). So no system packages are
# required beyond what the base image provides.
# git (cargo/git fetch) + make (the Makefile drives the build).
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
         git \
         make \
         ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Nightly toolchain + rust-src (needed for -Zbuild-std) + rustfmt.
#
# The channel is PINNED, and the pin lives in ONE place: rust-toolchain.toml at
# the repo root — the same file the host's rustup, cargo and rust-analyzer read.
# The image installs the channel that file names, so the container cannot drift
# from the host recipe. A dated channel resolves from the archived date-stamped
# channel manifest, so an image rebuilt months later gets the same compiler
# rather than the newest nightly; the current pin is the instance the cycle
# records in FLAC.md were measured on (its header says why).
#
# If the file is missing or names no channel, this RUN fails the image build
# loudly — better than quietly installing whatever `nightly` means that week.
# (`make` inside the container can still be overridden with TOOLCHAIN= for an
# experiment; say so in the record if you use it.)
#
# The read below is the SAME rule the host Makefile's "Toolchain resolution"
# block uses — make's own file read, plus a match on the one word wrapped
# end-to-end in double quotes, plus a count guard refusing a file that yields
# anything other than exactly one such word. No `sed`, no `head`, no regex to
# read, and no channel string anywhere in this file: host and image parse the pin
# by one rule, so they cannot drift in HOW they read it either.
# It needs GNU make 4.2+, which the apt layer above installs. There is no
# make-version guard on this side, unlike the host Makefile (which has to survive
# Apple's 3.81 at /usr/bin/make): if this image's make were ever too old, this
# RUN would fail at the read — an empty expansion plus the `test -n` below, or
# make's own unsupported-function error — rather than installing some other
# compiler. And the file arrives by COPY, so an absent pin file fails the COPY
# before the read ever runs.
#
# One gap this read shares with the host's, stated rather than papered over: a
# COMMENTED-OUT `channel` line still yields its word, so the image would install
# the commented-out channel — a stale pinned instance, not a moving one, which is
# the failure the pin exists against. The host catches that case in
# `make toolchain-check`, which asks rustup what it resolves in the tree; the
# container's build path (`make rom`) does not run that gate.
#
# rustfmt is here because every build target runs the `format` pre-step, and
# the container's CMD/entrypoint path is `make rom` -> `make build` -> `make
# format`. Without it the container silently skips formatting.
COPY rust-toolchain.toml /tmp/rust-toolchain-pinned.toml
RUN CHANNEL="$(make -f /dev/null \
        --eval='pin := $(filter "%", $(file < /tmp/rust-toolchain-pinned.toml))' \
        --eval='show:; $(if $(filter 1,$(words $(pin))),@echo $(subst ",,$(pin)),$(error expected exactly one double-quoted word, the channel; found $(words $(pin))))' \
        show)" \
    && test -n "$CHANNEL" || { echo "Dockerfile: rust-toolchain.toml yielded no double-quoted channel word" >&2; exit 1; } \
    && echo "installing the pinned Rust channel: $CHANNEL" \
    && rustup toolchain install "$CHANNEL" \
    && rustup default "$CHANNEL" \
    && rustup component add rust-src rustfmt --toolchain "$CHANNEL" \
    && rm /tmp/rust-toolchain-pinned.toml

# Install the GBA ROM fixing tool — pinned, like the mGBA runner further down.
# Not for cycles: gbafix costs no runtime. But it rewrites the ROM header, so it
# is part of the ROM BYTES every FLAC.md record compares by sha, and an unpinned
# gbafix would silently change those shas on another machine. 0.25.0 is the agb
# release this project depends on (Cargo.toml: agb = "0.25.0", AGB_REF below) and
# the version every ROM in those records was fixed with — this host's installed
# gbafix is 0.25.0, which is why the pin changes no byte here.
RUN cargo install agb-gbafix --version 0.25.0 --locked

WORKDIR /app

# ==============================================================================
# Stage 2: Builder
# Used for compiling ROMs and formatting checks. Keeps heavy C dependencies out.
# ==============================================================================
FROM base AS builder

CMD ["make", "rom"]

# ==============================================================================
# Stage 3: Tester
# Used for running the `mgba-test-runner` test suite.
# ==============================================================================
FROM base AS tester

# The TEST path is not pure Rust: mgba-test-runner links libmgba, a C
# library that mgba-sys' build script fetches (curl) and compiles with CMake,
# generating Rust bindings with bindgen (needs libclang).
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
         curl \
         tar \
         build-essential \
         cmake \
         pkg-config \
         clang \
         libclang-dev \
         libelf-dev \
         zlib1g-dev \
         libpng-dev \
         libasound2-dev \
    && rm -rf /var/lib/apt/lists/*

# Install the GBA test harness: the headless mGBA runner agb's #[test_case]
# suite needs (this is what `make test-rom` sets as CARGO_TARGET_..._RUNNER).
#
# Pinned to the agb version this project depends on (Cargo.toml: agb = "0.25.0")
# so the runner and the framework's test harness can't drift apart.
#
# We clone and patch manually instead of using `cargo install --git` because
# agb v0.25.0 has an AArch64 Linux compilation bug in `emulator/mgba`: it
# assumes `c_char` is `i8` (it is `u8` on ARM Linux) and uses an x86-specific
# `__va_list_tag`. The sed commands patch this for Apple Silicon host compat.
ARG AGB_REF=v0.25.0
RUN git clone --depth 1 --branch "${AGB_REF}" https://github.com/agbrs/agb.git /tmp/agb \
    && cd /tmp/agb \
    && sed -i 's/not(target_os = "macos")/not(any(target_os = "macos", target_arch = "aarch64"))/' emulator/mgba/src/log.rs \
    && sed -i 's/any(windows, target_os = "macos")/any(windows, target_os = "macos", target_arch = "aarch64")/' emulator/mgba/src/log.rs \
    && sed -i 's/\*const i8/*const libc::c_char/g' emulator/mgba/src/log.rs \
    && cargo install --path emulator/test-runner --locked \
    && rm -rf /tmp/agb
RUN command -v mgba-test-runner \
    && mgba-test-runner --help >/dev/null \
    && echo "mgba-test-runner OK: $(command -v mgba-test-runner)"

CMD ["make", "test"]
