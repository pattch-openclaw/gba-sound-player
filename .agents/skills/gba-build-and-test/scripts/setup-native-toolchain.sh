#!/usr/bin/env bash
# Native toolchain setup for gba-sound-player: the minimum that makes every native
# Make target and test gate run on a Linux host WITHOUT root. Idempotent; run it, then
# use `make native-rom`, `make test`, `make flac-gates` as usual.
#
# Verified on Ubuntu 26.04 arm64, no passwordless sudo, 2026-11-06 (see
# references/native-toolchain-setup.md for the measured output and the failure modes
# this script exists to avoid).
set -uo pipefail

AGB_REF="${AGB_REF:-v0.25.0}"     # MUST match the `agb` version in Cargo.toml
WORK="${WORK:-/tmp/agb-runner-build}"
BINDEST="${BINDEST:-$HOME/.cargo/bin}"
TOOLCHAIN="${TOOLCHAIN:-nightly}"
BREW_PREFIX="${BREW_PREFIX:-/home/linuxbrew/.linuxbrew}"

say() { printf '== %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- 0. sanity: run from the repo so the agb pin can be read -------------------
if [ -f Cargo.toml ] && grep -q 'agb *=' Cargo.toml 2>/dev/null; then
  want=$(grep -m1 -E '^agb *=' Cargo.toml | tr -d '"' | sed -E 's/.*= *([0-9.]+).*/\1/')
  if [ -n "$want" ] && [ "v$want" != "$AGB_REF" ]; then
    echo "warning: Cargo.toml pins agb $want but AGB_REF=$AGB_REF; set AGB_REF=v$want" >&2
  fi
fi

# --- 1. Rust: nightly + rust-src (-Zbuild-std for the Tier-3 target) + rustfmt --
if have rustup; then
  say "rust: rustup present; ensuring $TOOLCHAIN + rust-src + rustfmt"
  rustup toolchain list | grep -q "^$TOOLCHAIN" || rustup toolchain install "$TOOLCHAIN"
  rustup component list --installed --toolchain "$TOOLCHAIN" | grep -qx rust-src \
    || rustup component add rust-src --toolchain "$TOOLCHAIN"
  rustup component list --installed --toolchain "$TOOLCHAIN" | grep -qx rustfmt \
    || rustup component add rustfmt --toolchain "$TOOLCHAIN"
else
  say "rust: no rustup - a nightly rustc/cargo on PATH must already provide -Zbuild-std"
fi

# --- 2. agb-gbafix (ROM fixing: `rom`, `flac-rom`, `*-rom` targets) -----------
if have agb-gbafix; then say "agb-gbafix: already present ($(command -v agb-gbafix))"
else say "agb-gbafix: cargo install agb-gbafix"; cargo install agb-gbafix || exit 1; fi

# --- 3. C build deps for libmgba (only the ROM test suite needs these) --------
# Ask functionally: what does the libmgba build actually need and is it already there?
need=()
have cmake     || need+=(cmake)
have pkg-config || have pkgconf || need+=(pkg-config)
[ -f /usr/include/zlib.h ]   || [ -f "$BREW_PREFIX/opt/zlib/include/zlib.h" ]   || need+=(zlib)
[ -f /usr/include/libelf.h ] || [ -f "$BREW_PREFIX/opt/elfutils/include/libelf.h" ] || need+=(elfutils)
if [ "${#need[@]}" -gt 0 ]; then
  if [ -x "$BREW_PREFIX/bin/brew" ] || have brew; then
    b="$BREW_PREFIX/bin/brew"; [ -x "$b" ] || b=brew
    say "libmgba deps missing (${need[*]}): brew install ${need[*]}"
    HOMEBREW_NO_AUTO_UPDATE=1 "$b" install "${need[@]}" || exit 1
  else
    say "libmgba deps missing (${need[*]}): with root use:"
    echo "  apt-get install -y build-essential cmake pkg-config zlib1g-dev libelf-dev libpng-dev"
  fi
else
  say "libmgba deps: all present"
fi
if [ -d "$BREW_PREFIX/opt" ]; then
  export PATH="$BREW_PREFIX/bin:$PATH"
  export CMAKE_PREFIX_PATH="$BREW_PREFIX/opt/zlib;$BREW_PREFIX/opt/elfutils;$BREW_PREFIX/opt/libpng"
  export PKG_CONFIG_PATH="$BREW_PREFIX/opt/zlib/lib/pkgconfig:$BREW_PREFIX/opt/elfutils/lib/pkgconfig:$BREW_PREFIX/opt/libpng/lib/pkgconfig"
  export LDFLAGS="-L$BREW_PREFIX/opt/zlib/lib -L$BREW_PREFIX/opt/elfutils/lib -L$BREW_PREFIX/opt/libpng/lib"
  export CPPFLAGS="-I$BREW_PREFIX/opt/zlib/include -I$BREW_PREFIX/opt/elfutils/include -I$BREW_PREFIX/opt/libpng/include"
fi

# --- 4. libclang for bindgen (mgba-sys generates bindings) -------------------
# Prefer a complete clang install. If only the distro *runtime* libclang exists
# (no clang-c/Index.h, no resource headers), assemble the pieces root-free.
CLANG_RES=$(ls -d /usr/lib/llvm-*/lib/clang/*/include 2>/dev/null | head -1)
if have clang || [ -n "$CLANG_RES" ]; then
  say "libclang: a complete clang install is present"
else
  runtime=$(ls /usr/lib/*/libclang-*.so.1 /usr/lib/llvm-*/lib/libclang-*.so.1 2>/dev/null | head -1)
  if [ -z "$runtime" ]; then
    echo "warning: no libclang at all. With root: apt-get install -y clang libclang-dev" >&2
  else
    say "libclang: runtime-only ($runtime); assembling a rootless bindgen host"
    ver=$(basename "$runtime" | sed -E 's/libclang-([0-9]+)\.so.*/\1/')
    root="$HOME/.local/llvm-bindgen"
    mkdir -p "$root/usr/lib/llvm-$ver/lib" "$root/lib"
    ln -sf "$runtime" "$root/lib/libclang.so"
    if [ ! -d "$root/usr/lib/llvm-$ver/lib/clang/$ver/include" ]; then
      tmp=$(mktemp -d); ( cd "$tmp" && apt-get download "libclang-common-$ver-dev" \
        && dpkg-deb -x libclang-common-$ver-dev_*.deb "$root" ) || \
        echo "warning: could not fetch libclang-common-$ver-dev; with root: apt-get install -y libclang-dev" >&2
      rm -rf "$tmp"
    fi
    export LIBCLANG_PATH="$root/lib"
    # The distro runtime libclang does not add GCC's own include dir, so glibc's
    # limits.h `#include_next` has nowhere to land: point bindgen at it explicitly.
    gccinc=$(gcc -print-file-name=include)
    export BINDGEN_EXTRA_CLANG_ARGS="-idirafter $gccinc"
    CLANG_RES="$root/usr/lib/llvm-$ver/lib/clang/$ver"
  fi
fi

# --- 5. mgba-test-runner (the headless mGBA that `make test-rom` needs) ------
if have mgba-test-runner; then
  say "mgba-test-runner: already present ($(command -v mgba-test-runner))"
else
  say "mgba-test-runner: building agb $AGB_REF's test runner"
  # agb v0.25.0 mis-builds emulator/mgba on aarch64 Linux (assumes c_char == i8 and
  # uses an x86 __va_list_tag). Same three patches the project Dockerfile applies -
  # this host is aarch64 Linux, so they are needed natively too.
  if [ ! -d "$WORK/repo/.git" ]; then
    rm -rf "$WORK/repo" && mkdir -p "$WORK" \
      && git clone --depth 1 --branch "$AGB_REF" https://github.com/agbrs/agb.git "$WORK/repo" || exit 1
  fi
  cd "$WORK/repo" || exit 1
  sed -i 's/not(target_os = "macos")/not(any(target_os = "macos", target_arch = "aarch64"))/' emulator/mgba/src/log.rs
  sed -i 's/any(windows, target_os = "macos")/any(windows, target_os = "macos", target_arch = "aarch64")/' emulator/mgba/src/log.rs
  sed -i 's/\*const i8/*const libc::c_char/g' emulator/mgba/src/log.rs
  # --git cannot be combined with --path, and --git cannot carry these patches.
  cargo install --path emulator/test-runner --locked --root "$HOME/.local/mgba-runner" || exit 1
  mkdir -p "$BINDEST"
  cp -v "$HOME/.local/mgba-runner/bin/mgba-test-runner" "$BINDEST/" || exit 1
fi

echo
say "result"
for c in rustc cargo agb-gbafix mgba-test-runner cmake pkg-config clang cc; do
  printf '  %-18s %s\n' "$c" "$(command -v "$c" 2>/dev/null || echo '-')"
done
[ -n "$CLANG_RES" ] && printf '  %-18s %s\n' "clang resource hdrs" "$CLANG_RES"
echo
echo "Next: make native-rom && make test && make flac-gates"
