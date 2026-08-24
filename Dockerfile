# Use the official Rust image based on Debian 12.
#
# WHY NIGHTLY: `thumbv4t-none-eabi` (ARMv4T) is a low-tier target. Stable rustup
# has NO prebuilt std artifacts for it, so `rustup target add thumbv4t-none-eabi`
# fails on stable ("has no prebuilt artifacts available for target"). The
# practical way to build std for this bare-metal target is a nightly toolchain
# + the `rust-src` component (std is compiled from source for the target).
# This is a hard Rust limitation, not a project preference — do not "simplify"
# this to stable; the build will break at the `rustup target add` step.
FROM rust:slim-bookworm

# Install required C toolchain and git
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install nightly + the thumbv4t target + std sources
RUN rustup toolchain install nightly \
    && rustup default nightly \
    && rustup component add rust-src \
    && rustup target add thumbv4t-none-eabi

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

# Set the working directory
WORKDIR /app

# By default, running the container will build the ROM
CMD ["make", "rom"]
