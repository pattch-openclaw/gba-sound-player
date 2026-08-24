# Use the official Rust image based on Debian 12.
# We pin STABLE (not nightly): agb 0.25 + thumbv4t-none-eabi builds fine under
# stable as long as `rust-src` is present (std is built from source for the
# bare-metal target). If a future dependency truly requires nightly, bump the
# toolchain line below and the corresponding rust-toolchain.toml together.
FROM rust:slim-bookworm

# Install required C toolchain and git
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install stable + the thumbv4t target + std sources
RUN rustup component add rust-src \
    && rustup target add thumbv4t-none-eabi

# Install the GBA ROM fixing tool
RUN cargo install agb-gbafix

# Set the working directory
WORKDIR /app

# By default, running the container will build the ROM
CMD ["make", "rom"]
