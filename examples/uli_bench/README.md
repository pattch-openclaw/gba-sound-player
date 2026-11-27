# `uli-bench` — ULI2 Phase 1b cycle harness

The measurement vehicle for the U4 gate (real-time decode) and for every
hot-path PR that follows. It embeds the committed Phase 0 corpus and:

1. decodes it and checks the C-reference album pins (`e6e87652` / `b3530978`)
   with `xover=0 hfnz=0`;
2. times a full album decode through **both** kernel sets — the portable
   reference (`PortableKernels`, the control) and the product's
   `ArmKernels` — frame by frame in free-running timer2→timer3 windows at
   16.78 MHz, IRQs off, with a measured empty-window overhead subtracted.
   Both timed passes are pinned on their own samples;
3. times the four synthesis kernels in isolation and as one per-channel
   sequence, for both realisations, so the kernel share and the non-synthesis
   rest can be separated;
4. runs the reference and overlay kernels on identical inputs and compares
   every sample on target (the host twin is `make uli-overlay-test`).

It sets `WAITCNT` to `0x0000` (4/2, no prefetch) for the recorded arm; `agb`
boots at `0x4317` and the log prints both. It reports sums and counts — means
are derived host-side, because ARM7TDMI has no divide.

Build and run from the repository root:

```sh
make uli-bench-rom   # build + fix uli-bench.gba
make uli-bench       # run headless in mGBA, write uli-bench.log
```

The image is its own control: the core arm and the
overlay arm share the bitstream/dequant/M/S/frame code, so a change there
moves both; a kernel-only change moves only the overlay arm. The product arm
(`GbaDecoder`, the product hot path) is timed in the same image.

The ROM prints `PASS`/`FAIL` and then the `mgba-test-runner` exit sentinel, so
the run ends immediately instead of waiting for a timeout. It is an mGBA
measurement of the *portable* decode path and the overlay's kernels; it is not
a player, and it makes no hardware claim.

To measure the optional IWRAM-resident tables arm:

```sh
cd examples/uli_bench && cargo build --release --target thumbv4t-none-eabi \
  --features uli-lite-gba/iwram-tables
```

## The corpus: `assets/` is generated, never committed

The tracked input is the master audio; the blob, the `album.f32` intermediate and
the pin module are build output (`.gitignore`). The recipe is
`make uli-album-asset`, which needs ffmpeg and the venv `python3` (numpy/scipy), and
runs:

1. `scripts/uli_prep.py assets/sound/bioluminescent.wav --out examples/uli_bench/assets`
   — the default master is **96 kHz / 24-bit stereo, 258.6 s**; the in-repo script
   resamples it to 32,768 Hz through `scripts/uli_resample.py` and writes `album.f32`
   (f32 interleaved stereo at PCM16 scale) and `tracks.tsv`. The
   `bioluminescent-32768hz-8bit-stereo.wav` beside the master is a listening
   derivative, not this target's input.
2. `crates/uli-enc` → `assets/biomaster.uli`;
3. `crates/uli-lite/examples/gen_album_pin.rs` → `src/album_pin.rs` (the Rust
   decoder's FNV pin).

No step needs the `gba-audio` checkout; the C oracle is used only by
`make uli-accept`, and then only when explicitly pointed at.

`make uli-accept` encodes the **PCM pair** for its `biomaster` and `off-default` cells;
`make uli-bench` / `uli-gate` / `uli-album-bench` use the blob and/or the pin. The other
corpora in the same `assets/` tree:

| target | corpus | used by |
|---|---|---|
| `make uli-cross-asset` | `assets/cross/` — the 12-track CC-licensed album, downloaded/trimmed by `scripts/uli_cross_album.py` then `scripts/uli_prep.py` | the `cross60` cell, album-scale bench/gate |
| `make uli-adversarial-asset` | `assets/adv/` — synthetic 10 s density set, offline | the matrix's fourth slot when present (replaces `cross60`) |
| `make uli-two-track-asset` | `assets/two/` — the master twice, for the seek witness | `uli-player-two-track` |

The full explanation, including the local `uli_prep.py` and where the pins come from, is
in [`../../ULI2.md`](../../ULI2.md) -> "The measurement corpus — how
`examples/uli_bench/assets` is generated and used".

## Measured arms

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period. **Current
(2026-11-19):**

| arm | worst | mean |
|---|---:|---:|
| **`GbaDecoder` (the product path)** | **193,317 (73.75%)** | **172,109 (65.66%)** |

The album decode wall and the gate total come from `make uli-gate`:
**194,695 / 169,354** (74.27% / 64.60%) and **226,152 / 200,624** (86.27% /
76.53%) over 16,553 frames, with 13.73 points of margin and 0 starved periods.

The `kern` and `ovl` lines in the log are **same-image controls**, not product
numbers: `kern` is the portable reference kernel set and `ovl` the `ArmKernels`
overlay, both several times the product's cost, kept so a kernel regression shows
up as a delta inside one image. The per-kernel sums (`pre`/`fft`/`post`/`ola`,
10 calls each) are what the kernel work is judged on.

Absolute frame numbers are image-specific at
this fetch-dominated scale (the core control moved ~8% from the Phase 1b image
with an unchanged kernel sequence); compare arms **within one image**. The
numbers, the caveats and the plan consequences are in
[`../../crates/uli-lite/PLAN-OF-RECORD.md`](../../crates/uli-lite/PLAN-OF-RECORD.md)
and `ULI2.md` → “Phase 1b”.
