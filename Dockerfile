# GBA ROM build image (Debian 12, Rust).
#
# Target: thumbv4t-none-eabi — a Tier-3, #![no_std] bare-metal target
# (supports core + alloc only). It ships NO prebuilt std artifacts on any
# rustup toolchain (stable OR nightly), so do NOT run `rustup target add` for
# it — that step will fail. We build no_std directly for the target instead.
#
#   Ref: https://doc.rust-lang.org/nightly/rustc/platform-support/armv4t-none-eabi.html
#
FROM rust:slim-bookworm

# C toolchain + git (linker / build deps)
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

# Set the working directory
WORKDIR /app

# By default, running the container will build the ROM
CMD ["make", "rom"]
