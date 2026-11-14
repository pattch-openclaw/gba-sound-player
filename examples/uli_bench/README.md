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

PR B uses this image as its before/after control: the core arm and the
overlay arm share the bitstream/dequant/M/S/frame code, so a change there
moves both; a kernel-only change moves only the overlay arm.

The ROM prints `PASS`/`FAIL` and then the `mgba-test-runner` exit sentinel, so
the run ends immediately instead of waiting for a timeout. It is an mGBA
measurement of the *portable* decode path and the overlay's kernels; it is not
a player, and it makes no hardware claim.

To measure the optional IWRAM-resident tables arm:

```sh
cd examples/uli_bench && cargo build --release --target thumbv4t-none-eabi \
  --features uli-lite-gba/iwram-tables
```

## Measured arms

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean |
|---|---:|---:|
| core kernels (control) | 2,676,986 (1021%) | 2,171,620 (828%) |
| `ArmKernels` | 1,829,247 (698%) | 1,322,800 (505%) |

The overlay frame equals `core_rest + 2 × overlay_seq` within 0.02%, which is
why the per-kernel arms are kept. Absolute frame numbers are image-specific at
this fetch-dominated scale (the core control moved ~8% from the Phase 1b image
with an unchanged kernel sequence); compare arms **within one image**. The
numbers, the caveats and the plan consequences are in
[`../../crates/uli-lite/PLAN-OF-RECORD.md`](../../crates/uli-lite/PLAN-OF-RECORD.md)
and `ULI2.md` → “Phase 1b”.
