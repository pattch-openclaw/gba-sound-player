# `uli-bench` — ULI2 Phase 1b cycle harness

The measurement vehicle for `ULI2.md` gate **U4** (real-time decode). It
embeds the committed Phase 0 corpus and:

1. decodes it and checks the C-reference album pins (`e6e87652` /
   `b3530978`) with `xover=0 hfnz=0`, then decodes it a second time with the
   same pin check on the timed pass;
2. times every frame of that second decode in a free-running timer2→timer3
   window at 16.78 MHz, IRQs off, with a measured empty-window overhead
   subtracted;
3. times the four synthesis kernels in isolation and as one per-channel
   sequence, for both the portable core and the `uli-lite-gba` overlay, and
   checks the overlay is bit-exact with the core on the target.

It sets `WAITCNT` to `0x0000` (4/2, no prefetch) for the recorded arm; `agb`
boots at `0x4317` and the log prints both. It reports sums and counts — means
are derived host-side, because ARM7TDMI has no divide.

Build and run from the repository root:

```sh
make uli-bench-rom   # build + fix uli-bench.gba
make uli-bench       # run headless in mGBA, write uli-bench.log
```

The ROM prints `PASS`/`FAIL` and then the `mgba-test-runner` exit sentinel, so
the run ends immediately instead of waiting for a timeout. It is an mGBA
measurement of the *portable* decode path and the overlay's kernels; it is not
a player, and it makes no hardware claim.

To measure the optional IWRAM-resident tables arm:

```sh
cd examples/uli_bench && cargo build --release --target thumbv4t-none-eabi \
  --features uli-lite-gba/iwram-tables
```

The numbers, the decomposition and the plan consequences are in
`ULI2.md` → “Phase 1b — what the on-target measurement found”.
