# GBA ROM build + TEST image (Debian 12, Rust nightly).
#
# This one image serves every `make` entrypoint — builds AND tests — so the
# containerized path needs no host tooling at all:
#   agb-gbafix        ROM fixing           (make rom / flac-rom)
#   mgba-test-runner  headless mGBA runner (make test-rom)
#
# The `podman-*` Makefile targets run the BARE gate targets here (`make test`,
# `make flac-test`, `make test-rom`, `make rom`), which execute directly — same
# commands, same semantics as on a host, so nothing can recurse into another
# container.
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
FROM rust:slim-bookworm

# This project uses `agb`, which links with Rust's BUILT-IN linker (rust-lld)
# via the `-Clink-arg=-Tgba.ld` script — it does NOT use an ARM cross-compiler
# (no `arm-none-eabi-ld`, no `arm-none-eabi-gcc`). So no system packages are
# required beyond what the base image provides. (Do NOT add an ARM cross-compiler
# here; it is unused by this build path and `arm-none-eabi-gcc` does not exist
# on Debian bookworm — the real package is `gcc-arm-none-eabi`, which we don't need.)
# git (cargo/git fetch) + make (the Makefile drives the build; rust:slim-bookworm
# ships neither). The ROM build itself is pure Rust (rust-lld links; no C is
# compiled) — but the TEST path is not: mgba-test-runner links libmgba, a C
# library that mgba-sys' build script fetches (curl) and compiles with CMake,
# generating Rust bindings with bindgen (needs libclang). Hence the extra
# system packages below: the same set agb's own CI installs
# (build-essential/libelf-dev/libasound-dev) plus cmake + clang/libclang +
# pkg-config for the build script. Do NOT add an ARM cross-compiler here; it is
# unused by this build path (the real Debian package is `gcc-arm-none-eabi`).
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
         git \
         make \
         ca-certificates \
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

# Nightly toolchain + rust-src (needed for -Zbuild-std) + rustfmt.
#
# rustfmt is here because every build target runs the `format` pre-step, and
# the container's CMD/entrypoint path is `make rom` -> `make build` -> `make
# format`. Without it the container silently skips formatting (the Makefile
# degrades to a notice rather than failing the build), which would let the
# standardized path produce unformatted code. It also lets CI run `make check`
# (the non-writing `cargo fmt --check` gate) inside this image.
RUN rustup toolchain install nightly \
    && rustup default nightly \
    && rustup component add rust-src rustfmt

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

# Install the GBA test harness: the headless mGBA runner agb's #[test_case]
# suite needs (this is what `make test-rom` sets as CARGO_TARGET_..._RUNNER).
#
# Pinned to the agb version this project depends on (Cargo.toml: agb = "0.25.0")
# so the runner and the framework's test harness can't drift apart. Built at
# image-build time (compiling libmgba from source takes a few minutes ONCE) so
# `make test-rom` never pays for it, and no host needs mGBA/cmake at all.
#
# `--locked` asks cargo to keep dependency versions as recorded in the source's
# lockfile. agb does NOT commit a workspace Cargo.lock, so cargo prints
# "warning: no Cargo.lock file published in mgba-test-runner" and resolves
# fresh — the warning is expected, not a failure. Kept because it becomes a
# hard pin the day agb does publish a lock, and it fails loudly if a recorded
# lock could not be honored.
#
# NOTE: `cargo install --git` does NOT accept `--path` (cargo errors: "the
# argument '--git <URL>' cannot be used with '--path <PATH>'"). With --git you
# select the crate by NAME as the positional argument; cargo resolves it inside
# the cloned repo's workspace — no --path needed (or allowed). (--path is for
# installing from a LOCAL directory; combining it with --git is what broke
# every `make podman-*` target.)
ARG AGB_REF=v0.25.0
RUN cargo install --git https://github.com/agbrs/agb.git --tag "${AGB_REF}" \
      --locked mgba-test-runner
RUN command -v mgba-test-runner \
    && mgba-test-runner --help >/dev/null \
    && echo "mgba-test-runner OK: $(command -v mgba-test-runner)"

# Set the working directory
WORKDIR /app

# By default, running the container will build the ROM
CMD ["make", "rom"]
