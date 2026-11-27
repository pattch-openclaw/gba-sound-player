# Source this from the repo root (or anywhere):  . .agents/env.sh
#
# It puts the pinned, root-free toolchain on PATH for this shell. Nothing here
# changes the repo or writes outside $HOME.
#
# The setup that creates these paths is:
#   .agents/skills/gba-build-and-test/scripts/setup-native-toolchain.sh

# --- PATH: rustup shims, our wrappers/static tools, then the Python venv -------
# The venv is first so `python3` is the one with numpy/scipy/soundfile and
# fonttools/pillow. `make album-bake` calls plain `python3` and needs fontTools.
export PATH="$HOME/.local/venvs/gba-audio/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

# --- project paths -------------------------------------------------------------
# Optional: the sibling oracle checkout, used only by `make uli-accept`,
# `make uli-oracle-check` and the probe/fixture instruments. No asset target
# needs it; `make uli-album-asset` and its siblings run entirely in-tree.
if [ -d "$HOME/gba-audio/encoder" ]; then
  export GBA_AUDIO="${GBA_AUDIO:-$HOME/gba-audio}"
fi
# The Makefile's album target honours this; album-bake itself uses `python3`,
# which the PATH line above already points at the same interpreter.
export ULI_ALBUM_PY="${ULI_ALBUM_PY:-$HOME/.local/venvs/gba-audio/bin/python3}"

# --- rootless C headers/libs (only needed to rebuild mgba-test-runner) ---------
# Deliberately NOT exported globally: PKG_CONFIG_SYSROOT_DIR/LD_LIBRARY_PATH
# would then apply to every build in the session. The setup script sets them
# locally while it builds the runner. These are here for a manual rebuild:
#
#   export GBA_ROOTLESS="$HOME/.local/rootless"
#   export PKG_CONFIG_PATH="$GBA_ROOTLESS/usr/lib/aarch64-linux-gnu/pkgconfig"
#   export PKG_CONFIG_SYSROOT_DIR="$GBA_ROOTLESS"
#   export LIBCLANG_PATH="$GBA_ROOTLESS/usr/lib/aarch64-linux-gnu"
#   export BINDGEN_EXTRA_CLANG_ARGS="-resource-dir $GBA_ROOTLESS/usr/lib/llvm-18/lib/clang/18 -idirafter $(gcc -print-file-name=include)"
export GBA_ROOTLESS="${GBA_ROOTLESS:-$HOME/.local/rootless}"
