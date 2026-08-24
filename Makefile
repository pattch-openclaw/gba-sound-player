.PHONY: build rom clean podman-rom

# Local build settings (can be overridden)
# NOTE: nightly is REQUIRED. `thumbv4t-none-eabi` is a low-tier target with no
# prebuilt std artifacts on stable, so stable can't build std for it. (See Dockerfile.)
CARGO ?= cargo +nightly
# Container runtime: podman by default, override with `make podman-rom CONTAINER=docker` if you prefer.
CONTAINER ?= podman

build:
	$(CARGO) build --release

rom: build
	agb-gbafix target/thumbv4t-none-eabi/release/gba-sound-player -o gba-sound-player.gba
	@echo "ROM built: gba-sound-player.gba"

clean:
	$(CARGO) clean
	rm -f gba-sound-player.gba

# Containerized build (podman by default)
# Builds the image, then mounts the project dir into a fresh container
# and runs `make rom` there. The .gba lands back in your host dir.
# The image sets nightly as the default toolchain, so the in-container build is nightly.
podman-rom:
	$(CONTAINER) build -t gba-builder .
	$(CONTAINER) run --rm -v "$(PWD):/app:Z" -w /app gba-builder make rom
