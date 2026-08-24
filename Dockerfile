# Use the official Rust image based on Debian 12
FROM rust:slim-bookworm

# Install required C toolchain and git
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install nightly and the thumbv4t target
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
