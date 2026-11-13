# `uli-spike` — ULI2 Phase 1 decode-only ROM

This image is the first on-target witness for the ULI2 Rust port. It embeds the
committed 2-second corpus, parses the ULA2 container, decodes all 129 frames
through the portable [`uli-lite`](../../crates/uli-lite) core, and folds the
int8 and int16 outputs into the same FNV-1a pins the C reference produced.

It is **not** a player: no DMA, no FIFO, no mixer, no ARM kernels, no cycle
measurement. The portable Rust kernels are slow (tens of seconds of emulated
time for two seconds of audio), so this image makes no real-time claim.

## Build

From the repository root:

```sh
make uli-spike-rom
```

This builds `examples/uli_spike/target/thumbv4t-none-eabi/release/uli-spike`
and fixes it to `uli-spike.gba`. The decoder state is a `.ewram` static
(roughly 100 KiB, dominated by the Huffman LUTs); the fixture is embedded in
ROM.

## Run

Open `uli-spike.gba` in mGBA, or use a headless mGBA runner. The serial log
ends with:

```text
[uli-spike] frames=129 int8=e6e87652 (want e6e87652) int16=b3530978 (want b3530978) xover=0 hfnz=0
[uli-spike] PASS
```

The background is blue on PASS and red on failure.

## What it proves, and what it does not

Proves:

- the ULA2 parser, table parser, bitstream, dequantisation, IMDCT, OLA and both
  output paths run on the target;
- the target output is bit-exact to the C reference for both int8 and int16 on
  the committed corpus;
- no coefficient-bound (`xover`) or coded-band (`hfnz`) format violations occur.

Does not prove:

- real-time performance (the portable kernels are not the overlay's ARM
  kernels, and no cycle counter is run);
- playback through the 32,768 Hz FIFO/DMA path;
- hardware behaviour (this is an mGBA witness).
