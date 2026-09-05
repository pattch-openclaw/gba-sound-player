# GBA ROM build + TEST image (Debian 12, Rust nightly).
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
# rustfmt is here because every build target runs the `format` pre-step, and
# the container's CMD/entrypoint path is `make rom` -> `make build` -> `make
# format`. Without it the container silently skips formatting.
RUN rustup toolchain install nightly \
    && rustup default nightly \
    && rustup component add rust-src rustfmt

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

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
