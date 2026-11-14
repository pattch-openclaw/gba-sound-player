# Native toolchain setup for this repo, measured

Host: Ubuntu 26.04.1 arm64 guest (Parallels), **no passwordless sudo**, Linuxbrew at
`/home/linuxbrew/.linuxbrew`. Date: 2026-11-06. `scripts/setup-native-toolchain.sh`
automates everything below and is idempotent.

## What each gate needs

| Gate | Mechanism | Extra tooling |
|---|---|---|
| `make native-rom` / `native-flac-rom` / `native-product-play-rom` / `native-spike-rom` / `native-overlay-kernel-rom` / `native-opus-rom` | `cargo build --release --target thumbv4t-none-eabi` (`-Zbuild-std` from `.cargo/config.toml`) then `agb-gbafix` | nightly + `rust-src` + `agb-gbafix` |
| `make flac-test`, `flac-test-wide`, `flac-test-crc`, `flac-test-scan`, `flac-test-rice`, `flac-gates`, `overlay-*` gates, `product-play-test` | an in-tree `cargo check --release --target …` arm **plus** a host-triple `cargo test --manifest-path …` arm run from a gate-neutral cwd | nightly + `rust-src` |
| `make flac-portable-check` | `cargo check --release --target thumbv7em-none-eabi` | nothing else — `rust-src` supplies `core` for that triple too, so no `rustup target add` |
| `make test-rom` | builds the target test binary and runs it through `CARGO_TARGET_THUMBV4T_NONE_EABI_RUNNER=mgba-test-runner` | `mgba-test-runner` (libmgba: cmake + C compiler + zlib + libelf + libclang/bindgen) |
| `make master-pack` / `master-play-rom INPUT=…` | `scripts/gafp_pack.py`, which needs **ffmpeg** (only when the input must be resampled to 16,384 Hz) and **flac**; the generated `assets/master.gfp` + `src/master_pin.rs` are gitignored and removed by `make clean` |
| `make gafp-pack-check` | **flac + metaflac** — the only FLAC gate that runs the reference encoder; a red here without them is a missing tool, not a decode regression |
| archived instruments (`item6a*`, `item6a7*`, `rate-*`, `kernel-*`, `e5-fetch`) | **no Make target as of 2026-11-14** — build them by hand with `cargo build --release --target thumbv4t-none-eabi --features <arm>` in the crate that holds them; `flac-docs/history.md` §"Retired Make entrypoints" maps each name |

## The one expensive install: `mgba-test-runner`

`agb`'s headless mGBA runner is the only non-Rust-shaped dependency. Steps that worked:

```sh
# 1. C build deps for libmgba, root-free
brew install cmake pkg-config zlib elfutils libpng

# 2. libclang for bindgen (mgba-sys generates its bindings at build time)
#    A complete clang install (`apt-get install -y clang libclang-dev` with root, or
#    brew llvm) is the clean route. Without one, this host had only the distro RUNTIME
#    libclang (/usr/lib/aarch64-linux-gnu/libclang-21.so.1) — no clang-c/Index.h and no
#    clang resource headers. Rootless assembly that was measured to work:
mkdir -p ~/.local/llvm-bindgen/lib
ln -sf /usr/lib/aarch64-linux-gnu/libclang-21.so.1 ~/.local/llvm-bindgen/lib/libclang.so
apt-get download libclang-common-21-dev && dpkg-deb -x libclang-common-21-dev_*.deb ~/.local/llvm-bindgen
export LIBCLANG_PATH="$HOME/.local/llvm-bindgen/lib"
export BINDGEN_EXTRA_CLANG_ARGS="-idirafter $(gcc -print-file-name=include)"

# 3. CMake needs to be told where the brew prefixes are
export CMAKE_PREFIX_PATH="/home/linuxbrew/.linuxbrew/opt/zlib;/home/linuxbrew/.linuxbrew/opt/elfutils;/home/linuxbrew/.linuxbrew/opt/libpng"
export PKG_CONFIG_PATH="/home/linuxbrew/.linuxbrew/opt/zlib/lib/pkgconfig:/home/linuxbrew/.linuxbrew/opt/elfutils/lib/pkgconfig"
export LDFLAGS="-L/home/linuxbrew/.linuxbrew/opt/zlib/lib -L/home/linuxbrew/.linuxbrew/opt/elfutils/lib"
export CPPFLAGS="-I/home/linuxbrew/.linuxbrew/opt/zlib/include -I/home/linuxbrew/.linuxbrew/opt/elfutils/include"

# 4. agb v0.25.0 (the version pinned in Cargo.toml) — patch, then install by path
git clone --depth 1 --branch v0.25.0 https://github.com/agbrs/agb.git /tmp/agb
cd /tmp/agb
sed -i 's/not(target_os = "macos")/not(any(target_os = "macos", target_arch = "aarch64"))/' emulator/mgba/src/log.rs
sed -i 's/any(windows, target_os = "macos")/any(windows, target_os = "macos", target_arch = "aarch64")/' emulator/mgba/src/log.rs
sed -i 's/\*const i8/*const libc::c_char/g'                                            emulator/mgba/src/log.rs
cargo install --path emulator/test-runner --locked        # binary lands in ~/.cargo/bin
# keep it out of the toolchain bins instead:
cargo install --path emulator/test-runner --locked --root ~/.local/mgba-runner
cp ~/.local/mgba-runner/bin/mgba-test-runner ~/.cargo/bin/
```

Result on this host: libmgba 0.10.5 (static, `LIBMGBA_ONLY=1 M_CORE_GBA=1
M_CORE_GB=0 USE_DEBUGGERS=1`) linked, runner installed, `make test-rom` exits 0.

## Failure messages and what each one actually meant

| Message | Cause | Fix |
|---|---|---|
| `error[E0463]: can't find crate for 'core'` | Tier-3 target has no prebuilt core; `-Zbuild-std` was not in play | nightly + `rustup component add rust-src`; never `rustup target add thumbv4t-none-eabi` |
| `E0152: duplicate lang item in crate 'core'` while running host tests | inherited `build-std` from the parent `.cargo/config.toml` recompiled core under the prebuilt host core | run the host test from outside the repo (`cd /tmp/gba-sound-player-host-gate`) with `--manifest-path` and `--target <host triple>` |
| `error: no such command: '+nightly'` | a non-rustup cargo earlier on PATH | call the toolchain through rustup (`rustup run nightly cargo …`), which the Makefile already resolves |
| `fatal error: 'limits.h' file not found` at `/usr/include/limits.h:124` during bindgen | the distro runtime libclang does not add GCC's own include dir, so glibc's `#include_next <limits.h>` had nowhere to land | `BINDGEN_EXTRA_CLANG_ARGS="-idirafter $(gcc -print-file-name=include)"` (a complete clang install does this itself) |
| `could not find libclang` | only the runtime `libclang-21.so.1` present, no `libclang.so` name to dlopen | symlink `libclang.so` → the versioned runtime lib and set `LIBCLANG_PATH` |
| `Could NOT find ZLIB` / missing libelf during the libmgba CMake build | no `zlib1g-dev`/`libelf-dev` on the system | `brew install zlib elfutils` + `CMAKE_PREFIX_PATH`/`CPPFLAGS`/`LDFLAGS` |
| `error: cannot install with --git ... --path` / a build-script compile error in `emulator/mgba/src/log.rs` | `--git` can't carry local patches (and can't be combined with `--path`); agb v0.25.0 assumes `c_char == i8` and an x86 `__va_list_tag` on aarch64 Linux | clone at the tag, apply the three `sed` patches, `cargo install --path …  --locked` |
| `Running 0 tests` then success | the root crate has no `#[test_case]`s today | expected; do not report the ROM suite as asserting anything |

## Cold vs warm

Warm (the state a normal session ends in): ROM rebuilds finish in hundredths of a second,
`make flac-gates` ~1 s of test time, `make test-rom` ~26 s (the test profile is its own
artifact set), the whole verified set above runs in a few minutes.

Cold (`make clean-deep`, or a fresh checkout — plain `make clean` is shallow and
leaves these caches alone): every workspace×target pair pays the
`-Zbuild-std` compile of `core`/`alloc` plus all dependencies — minutes per workspace, and
the target dirs reach gigabytes. Don't clean a warm tree just to measure; if a cold
number is genuinely wanted, say so first and expect several minutes.
