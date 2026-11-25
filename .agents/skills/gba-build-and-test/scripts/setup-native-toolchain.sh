#!/usr/bin/env bash
# One-shot, idempotent, root-free agent toolchain for gba-sound-player.
#
# Installs everything the native Make targets and gates need on a Linux host
# without sudo:
#   * the pinned Rust channel from rust-toolchain.toml, with rust-src + rustfmt
#   * agb-gbafix 0.25.0 (part of the ROM bytes records compare by sha)
#   * ffmpeg 7.0.2 (static build) for album/master packing
#   * flac + metaflac 1.5.0 (the version the committed fixtures were made with;
#     Ubuntu 24.04's 1.4.3 regenerates different bytes and fails gafp-pack-check)
#   * a Python 3 venv with numpy/scipy/soundfile (gba-audio prep.py) and
#     fonttools/pillow (album glyphs)
#   * rootless zlib/libelf/libpng/libclang/libllvm headers + libs, and
#     mgba-test-runner built from agb v0.25.0 with the aarch64 patches
#
# Everything lands under $HOME: ~/.cargo, ~/.local/opt, ~/.local/rootless,
# ~/.local/venvs, ~/.local/mgba-runner, ~/.local/bin. After it finishes:
#
#   . .agents/env.sh          # from the repo root
#   make toolchain-check
#   make test
#
# Verified on Ubuntu 24.04 arm64, 2026-11-25. The script is safe to re-run.
set -uo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
cd "$REPO_ROOT" || exit 1

HOME_PREFIX="${HOME_PREFIX:-$HOME/.local}"
BIN="$HOME_PREFIX/bin"
OPT="$HOME_PREFIX/opt"
VENV="${VENV:-$HOME/.local/venvs/gba-audio}"
ROOTLESS="${ROOTLESS:-$HOME/.local/rootless}"
MGBA_ROOT="${MGBA_ROOT:-$HOME/.local/mgba-runner}"
WORK="${WORK:-/tmp/agb-runner-build}"

AGB_REF="${AGB_REF:-v0.25.0}"
FLAC_VER="${FLAC_VER:-1.5.0}"
FFMPEG_VER="${FFMPEG_VER:-7.0.2}"

PY_PINS="${PY_PINS:-numpy==2.5.5 scipy==1.18.1 soundfile==0.14.0 fonttools==4.68.0 pillow==13.0.0}"

say() { printf '== %s\n' "$*"; }
warn() { printf '!! %s\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

mkdir -p "$BIN" "$OPT" "$ROOTLESS"
export PATH="$HOME/.cargo/bin:$BIN:$PATH"

# ---------------------------------------------------------------------------
# 0. sanity: GNU make >= 4.2 (the Makefile reads rust-toolchain.toml with $(file))
# ---------------------------------------------------------------------------
if have make; then
  v=$(make --version | sed -n '1s/.* //p')
  case "$v" in
    4.[2-9]*|[5-9].*) ;;
    *) warn "GNU make $v is too old; 4.2+ is required (macOS: brew install make, call gmake)" ;;
  esac
fi

# ---------------------------------------------------------------------------
# 1. Rust: the pinned dated channel + rust-src (-Zbuild-std on Tier-3) + rustfmt
# ---------------------------------------------------------------------------
TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml 2>/dev/null | head -1)
TOOLCHAIN="${TOOLCHAIN:-nightly-2026-09-04}"
if ! have rustup; then
  say "rust: installing rustup (no default toolchain)"
  curl -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh || exit 1
  sh /tmp/rustup-init.sh -y --no-modify-path --default-toolchain none --profile minimal || exit 1
fi
export PATH="$HOME/.cargo/bin:$BIN:$PATH"
if ! rustup toolchain list | grep -q "^$TOOLCHAIN"; then
  say "rust: installing $TOOLCHAIN"
  rustup toolchain install "$TOOLCHAIN" --profile minimal || exit 1
fi
for c in rust-src rustfmt; do
  rustup component list --installed --toolchain "$TOOLCHAIN" | grep -qx "$c" \
    || rustup component add "$c" --toolchain "$TOOLCHAIN" || exit 1
done
say "rust: $(rustup run "$TOOLCHAIN" rustc -V)"

# ---------------------------------------------------------------------------
# 2. agb-gbafix 0.25.0 (must match the agb pin; it rewrites the ROM header)
# ---------------------------------------------------------------------------
if ! cargo install --list 2>/dev/null | grep -q "^agb-gbafix v0.25.0:"; then
  say "agb-gbafix 0.25.0: cargo install"
  cargo install agb-gbafix --version 0.25.0 --locked || exit 1
fi

# ---------------------------------------------------------------------------
# 3. ffmpeg (static) — album/master packing and gba-audio prep.py
# ---------------------------------------------------------------------------
ffmpeg_ok() { have ffmpeg && ffmpeg -version 2>/dev/null | head -1 | grep -q "version $FFMPEG_VER"; }
if ! ffmpeg_ok; then
  case "$(uname -m)" in
    aarch64|arm64) ff_arch=arm64 ;;
    x86_64|amd64)  ff_arch=amd64 ;;
    *) warn "no static ffmpeg mapping for $(uname -m); install ffmpeg manually"; ff_arch= ;;
  esac
  if [ -n "$ff_arch" ]; then
    say "ffmpeg $FFMPEG_VER: downloading static build ($ff_arch)"
    tmp=$(mktemp -d)
    curl -sSL "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-${ff_arch}-static.tar.xz" \
      -o "$tmp/ffmpeg.tar.xz" || exit 1
    mkdir -p "$OPT/ffmpeg"
    tar -xf "$tmp/ffmpeg.tar.xz" -C "$OPT/ffmpeg" --strip-components=1 || exit 1
    rm -rf "$tmp"
    ln -sf "$OPT/ffmpeg/ffmpeg" "$BIN/ffmpeg"
    ln -sf "$OPT/ffmpeg/ffprobe" "$BIN/ffprobe"
  fi
fi
have ffmpeg && say "ffmpeg: $(ffmpeg -version | head -1)"

# ---------------------------------------------------------------------------
# 4. flac + metaflac 1.5.0 — pinned: the committed fixtures are its bytes
# ---------------------------------------------------------------------------
flac_ok() { have flac && flac --version 2>/dev/null | grep -q "^flac $FLAC_VER"; }
if ! flac_ok; then
  say "flac $FLAC_VER: building from source (--disable-ogg, static)"
  tmp=$(mktemp -d)
  curl -sSL "https://downloads.xiph.org/releases/flac/flac-${FLAC_VER}.tar.xz" \
    -o "$tmp/flac.tar.xz" || exit 1
  tar -xf "$tmp/flac.tar.xz" -C "$tmp" || exit 1
  (
    cd "$tmp/flac-${FLAC_VER}" || exit 1
    ./configure --prefix="$OPT/flac-$FLAC_VER" \
      --disable-shared --enable-static --disable-ogg \
      --disable-doxygen-docs --disable-examples --disable-cpplibs >/dev/null || exit 1
    make -j"$(nproc 2>/dev/null || echo 2)" >/dev/null || exit 1
    make install >/dev/null || exit 1
  ) || exit 1
  rm -rf "$tmp"
  for b in flac metaflac; do
    printf '#!/bin/sh\nexec "%s/opt/flac-%s/bin/%s" "$@"\n' "$HOME_PREFIX" "$FLAC_VER" "$b" > "$BIN/$b"
    chmod +x "$BIN/$b"
  done
fi
have flac && say "flac: $(flac --version) (metaflac: $(metaflac --version 2>/dev/null || echo missing))"

# ---------------------------------------------------------------------------
# 5. Python 3 venv: numpy/scipy/soundfile for gba-audio prep; fonttools/pillow
#    for album_glyphs.py. `python3` on PATH must be this venv for album-bake.
# ---------------------------------------------------------------------------
if [ ! -x "$VENV/bin/python" ]; then
  say "python: creating $VENV"
  mkdir -p "$(dirname "$VENV")"
  if python3 -m venv "$VENV" >/dev/null 2>&1; then
    :
  else
    # Debian/Ubuntu without python3-venv: virtualenv bundles pip/ensurepip.
    python3 -m pip install --user --break-system-packages --quiet virtualenv || exit 1
    python3 -m virtualenv "$VENV" >/dev/null || exit 1
  fi
fi
"$VENV/bin/python" -m pip install --quiet --upgrade pip || exit 1
# shellcheck disable=SC2086
"$VENV/bin/python" -m pip install --quiet $PY_PINS || exit 1
"$VENV/bin/python" - <<'PY' || exit 1
import fontTools, numpy, PIL, scipy, soundfile
print("python:", "numpy", numpy.__version__, "scipy", scipy.__version__,
      "soundfile", soundfile.__version__, "fonttools", fontTools.version,
      "pillow", PIL.__version__)
PY

# ---------------------------------------------------------------------------
# 6. Rootless C development headers/libs for mgba-test-runner (Ubuntu arm64)
#    Uses the ports.ubuntu.com package index when the host has no -dev packages
#    and no root. Harmless on hosts that already have them.
# ---------------------------------------------------------------------------
need_rootless=0
[ -f /usr/include/zlib.h ] || need_rootless=1
[ -f /usr/include/libelf.h ] || need_rootless=1
[ -f /usr/include/png.h ] || need_rootless=1
[ -n "$(ls /usr/lib/*/libclang-*.so.1 /usr/lib/llvm-*/lib/libclang-*.so.1 2>/dev/null | head -1)" ] || need_rootless=1

if [ "$need_rootless" = 1 ] && ! have mgba-test-runner; then
  . /etc/os-release 2>/dev/null || true
  codename="${VERSION_CODENAME:-noble}"
  if [ "$(uname -m)" = aarch64 ] || [ "$(uname -m)" = arm64 ]; then
    say "rootless headers: fetching Ubuntu $codename arm64 -dev packages into $ROOTLESS"
    cache="$ROOTLESS/.cache"
    mkdir -p "$cache/debs" "$cache/lists"
    base="https://ports.ubuntu.com/ubuntu-ports"
    for comp in main universe; do
      idx="$cache/lists/$codename-$comp.gz"
      [ -s "$idx" ] || curl -sSL "$base/dists/$codename/$comp/binary-arm64/Packages.gz" -o "$idx" || exit 1
    done
    # package -> filename, parsed once from the two indexes
    python3 - "$cache/lists" "$codename" > "$cache/packages.tsv" <<'PY'
import gzip, os, sys
want = {
    "zlib1g-dev", "libelf-dev", "libelf1t64", "libpng-dev", "libpng16-16t64",
    "libzstd-dev", "libclang1-18", "libclang-common-18-dev", "libllvm18",
}
d = sys.argv[1]
for comp in ("main", "universe"):
    p = os.path.join(d, "%s-%s.gz" % (sys.argv[2], comp))
    if not os.path.exists(p):
        continue
    with gzip.open(p, "rt") as fh:
        for para in fh.read().split("\n\n"):
            fields = {}
            for line in para.splitlines():
                if ": " in line:
                    k, v = line.split(": ", 1)
                    fields.setdefault(k, v)
            if fields.get("Package") in want:
                print("%s\t%s" % (fields["Package"], fields["Filename"]))
PY
    while IFS=$'\t' read -r pkg path; do
      [ -n "$pkg" ] || continue
      f="$cache/debs/$(basename "$path")"
      [ -s "$f" ] || curl -sSL "$base/$path" -o "$f" || exit 1
      dpkg-deb -x "$f" "$ROOTLESS" || exit 1
    done < "$cache/packages.tsv"
    # clang-sys looks for libclang.so; the runtime package ships the versioned one.
    clanglib=$(ls "$ROOTLESS"/usr/lib/*/libclang-*.so.1 2>/dev/null | head -1)
    [ -n "$clanglib" ] && ln -sf "$clanglib" "$(dirname "$clanglib")/libclang.so"
    say "rootless headers: $(find "$ROOTLESS/usr/include" -maxdepth 1 -type f 2>/dev/null | wc -l) headers, libs in $ROOTLESS/usr/lib"
  else
    warn "rootless C deps only scripted for Ubuntu arm64; install zlib/libelf/libpng/libclang dev packages by hand"
  fi
fi

# ---------------------------------------------------------------------------
# 7. mgba-test-runner from agb v0.25.0 (the agb pin), patched for aarch64
# ---------------------------------------------------------------------------
if ! have mgba-test-runner && [ ! -x "$MGBA_ROOT/bin/mgba-test-runner" ]; then
  say "mgba-test-runner: building agb $AGB_REF's test runner"
  if [ ! -d "$WORK/repo/.git" ]; then
    rm -rf "$WORK/repo" && mkdir -p "$WORK"
    git clone --depth 1 --branch "$AGB_REF" https://github.com/agbrs/agb.git "$WORK/repo" || exit 1
  fi
  (
    cd "$WORK/repo" || exit 1
    # agb v0.25.0 assumes c_char == i8 and an x86 __va_list_tag; same patches as
    # the project Dockerfile. Only needed on aarch64, harmless elsewhere.
    sed -i 's/not(target_os = "macos")/not(any(target_os = "macos", target_arch = "aarch64"))/' emulator/mgba/src/log.rs
    sed -i 's/any(windows, target_os = "macos")/any(windows, target_os = "macos", target_arch = "aarch64")/' emulator/mgba/src/log.rs
    sed -i 's/\*const i8/*const libc::c_char/g' emulator/mgba/src/log.rs
    export PKG_CONFIG_PATH="$ROOTLESS/usr/lib/aarch64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export PKG_CONFIG_SYSROOT_DIR="$ROOTLESS"
    export CMAKE_PREFIX_PATH="$ROOTLESS/usr"
    export CFLAGS="-I$ROOTLESS/usr/include${CFLAGS:+ $CFLAGS}"
    export CXXFLAGS="$CFLAGS"
    export LDFLAGS="-L$ROOTLESS/usr/lib/aarch64-linux-gnu -Wl,-rpath,$ROOTLESS/usr/lib/aarch64-linux-gnu${LDFLAGS:+ $LDFLAGS}"
    if [ -n "$(ls "$ROOTLESS"/usr/lib/*/libclang-*.so.1 2>/dev/null | head -1)" ]; then
      export LIBCLANG_PATH="$(dirname "$(ls "$ROOTLESS"/usr/lib/*/libclang-*.so.1 | head -1)")"
      res=$(ls -d "$ROOTLESS"/usr/lib/llvm-*/lib/clang/*/ 2>/dev/null | head -1)
      [ -n "$res" ] && export BINDGEN_EXTRA_CLANG_ARGS="-resource-dir ${res%/} -idirafter $(gcc -print-file-name=include)"
    fi
    cargo install --path emulator/test-runner --locked --force --root "$MGBA_ROOT" || exit 1
  ) || exit 1
  ln -sf "$MGBA_ROOT/bin/mgba-test-runner" "$BIN/mgba-test-runner"
fi
if [ -x "$MGBA_ROOT/bin/mgba-test-runner" ]; then
  ln -sf "$MGBA_ROOT/bin/mgba-test-runner" "$BIN/mgba-test-runner"
  say "mgba-test-runner: $MGBA_ROOT/bin/mgba-test-runner"
fi

# ---------------------------------------------------------------------------
# 8. the sibling oracle (optional): build the reference encoder/decoder
# ---------------------------------------------------------------------------
if [ -f "$REPO_ROOT/../gba-audio/encoder/Makefile" ]; then
  say "gba-audio oracle: building build/uli_enc and build/uli_dec"
  make -C "$REPO_ROOT/../gba-audio/encoder" >/dev/null 2>&1 \
    && say "gba-audio oracle: ready" \
    || warn "gba-audio oracle: build failed (run make -C ../gba-audio/encoder to see it)"
fi

# ---------------------------------------------------------------------------
# result
# ---------------------------------------------------------------------------
echo
say "result"
for c in rustc cargo agb-gbafix ffmpeg ffprobe flac metaflac mgba-test-runner; do
  printf '  %-18s %s\n' "$c" "$(command -v "$c" 2>/dev/null || echo '-')"
done
printf '  %-18s %s\n' "python (album)" "$VENV/bin/python"
printf '  %-18s %s\n' "rootless libs" "$ROOTLESS"
echo
echo "Next:  . .agents/env.sh"
echo "       make toolchain-check && make test"
