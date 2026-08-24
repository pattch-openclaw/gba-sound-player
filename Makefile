.PHONY: build rom clean podman-rom

# Local build settings (can be overridden)
# nightly is REQUIRED: -Zbuild-std (in .cargo/config.toml) is a nightly-only flag,
# used to compile core/alloc from source for this Tier-3 target. See Dockerfile/
# README. Do NOT try to `rustup target add thumbv4t-none-eabi` — it has no
# prebuilt artifacts on any toolchain.
CARGO ?= cargo +nightly
# Container runtime: podman by default, override with `make podman-rom CONTAINER=docker` if you prefer.
CONTAINER ?= podman

build:
	$(CARGO) build --release --target thumbv4t-none-eabi

rom: build
	agb-gbafix target/thumbv4t-none-eabi/release/gba-sound-player -o gba-sound-player.gba
	@echo "ROM built: gba-sound-player.gba"

clean:
	$(CARGO) clean
	rm -f gba-sound-player.gba

# Containerized build (podman by default)
# Builds the image, then mounts the project dir into a fresh container
# and runs `make rom` there. The .gba lands back in your host dir.
podman-rom:
	$(CONTAINER) build -t gba-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app gba-builder make rom
