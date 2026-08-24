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

# C toolchain + git (build deps), plus a GNU ARM cross toolchain as a linker
# safety net (the gba.ld script + rust-lld are the primary linker path)
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    arm-none-eabi-gcc \
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
