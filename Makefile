.PHONY: build rom clean docker-rom

# Local build settings (can be overridden)
CARGO ?= cargo +nightly

build:
	$(CARGO) build --release

rom: build
	agb-gbafix target/thumbv4t-none-eabi/release/gba-audio -o gba-audio.gba
	@echo "ROM built: gba-audio.gba"

clean:
	$(CARGO) clean
	rm -f gba-audio.gba

# Dockerized build
docker-rom:
	docker build -t gba-builder .
	docker run --rm -v "$(PWD):/app" gba-builder make rom
