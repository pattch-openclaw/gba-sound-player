# GBA ROM build image (Debian 12, Rust nightly).
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
# ships neither). We do NOT need build-essential — this is a pure-Rust build
# (rust-lld links; no C is compiled).
RUN apt-get update \
    && apt-get install -y --no-install-recommends git make \
    && rm -rf /var/lib/apt/lists/*

# Nightly toolchain + rust-src (needed for -Zbuild-std)
RUN rustup toolchain install nightly \
    && rustup default nightly \
    && rustup component add rust-src

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

# Set the working directory
WORKDIR /app

# By default, running the container will build the ROM
CMD ["make", "rom"]
