.PHONY: build rom clean

# GBA toolchain (installed via rustup)
export PATH := /opt/homebrew/Cellar/rustup/1.29.0_2/bin:/tmp/ch/bin:$(PATH)
export RUSTUP_HOME := /tmp/rt
export CARGO_HOME := /tmp/ch

# Nightly is required for `-Z build-std=core,alloc` (thumbv4t std build).
NIGHTLY = cargo +nightly

build:
	$(NIGHTLY) build --release

rom: build
	agb-gbafix target/thumbv4t-none-eabi/release/gba-audio -o gba-audio.gba
	@echo "ROM: gba-audio.gba"

clean:
	rm -rf target gba-audio.gba
