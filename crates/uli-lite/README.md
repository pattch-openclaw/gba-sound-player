# `uli-lite`

A minimal, `no_std`, zero-dependency, zero-allocation decoder for the ULI2
bitstream and the ULA2 album container.

Since **2026-11-14 this crate is the reference and oracle**, not the shipped
decoder: ULI2 is a GBA product codec, so the product path lives in
[`../uli-lite-gba`](../uli-lite-gba) and is allowed to be ARM-mode, unchecked
and `.iwram`-resident. The constraints listed below are kept here because they
make this crate a cheap, host-testable witness — not because the product must
be portable. The live plan is
[`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md).

This is the Phase 0 core described in [`../../ULI2.md`](../../ULI2.md). It is a
bit-exact port of the C reference in `gba-audio/src/uli/uli_dec_ref.c` and
`uli_tables.c`:

- integer/fixed-point only, no float;
- no allocation; caller-owned [`DecoderState`] and output slices;
- no `agb`, no GBA registers, no platform-bound assembly;
- the ULA2 container parser is a separate module, so raw-frame consumers can
  use the codec without the album layer.

The GBA overlay (`uli-lite-gba`) will provide the ARM kernels and the
`.iwram`/`.ewram` placement. The portable kernels in this crate are the
correctness reference and the fallback for platforms without the overlay.

## API shape

```rust,ignore
use uli_lite::{Album, Decoder, DecoderState};

let album = Album::parse(blob)?;
let mut state = Box::new(DecoderState::new()); // on the GBA: a static in EWRAM
let mut decoder = Decoder::new(album, &mut state)?;

let mut out8l = [0i8; 512];
let mut out8r = [0i8; 512];
let info = decoder.decode_frame(&mut out8l, &mut out8r, None, None)?;
```

`decode_frame` fills the GBA DAC samples directly (int8 L/R) and can also fill
the optional pre-DAC PCM16 path for the speaker profile. `seek_track` decodes
the track's preroll frame and returns the samples to drop; `seek_entry` resets
to a seek-table entry without a preroll.

## State and memory

`DecoderState` is large (roughly 100 KiB, dominated by the 10-bit Huffman LUTs).
On the GBA it belongs in EWRAM; the overlay can pin the hot transform/lapping
buffers in IWRAM. The LUT width is an implementation detail, not bitstream, so a
future product can trade memory for the long-code slow path.

`DecoderState::new()` is `const`, so a GBA product can put it in a `static`
instead of constructing a 100 KiB value on the stack.

## Status and gates

Phase 0 is complete, and the Phase 1 decode-only ROM passes in mGBA:

- the thumbv4t target compiles with `core` only;
- the committed 2-second corpus decodes bit-exactly against the C reference for
  both int8 and int16 outputs, including per-frame operation counts and the
  cycle model;
- track/seek paths and malformed-album/output-size cases are covered;
- `examples/uli_spike` decodes the corpus on the target and matches the
  album-wide int8/int16 pins (`e6e87652` / `b3530978`), with `xover=0 hfnz=0`.

That ROM is a correctness witness, not a player. The cycle record is
`make uli-bench` (`examples/uli_bench`):

- **Phase 1b (2026-11-13)**: worst frame 2,970,042 cycles = 1133% of the
  262,144-cycle product period with the core kernels in ARM mode (25.3x before
  that codegen fix; 37x on the kernel sequence).
- **PR A (2026-11-14)**: the `Kernels` seam lets `Decoder` run a full frame
  through the product's `ArmKernels`. Measured **1,829,247 worst (698%) /
  1,322,800 mean (505%)**, both pinned; the same-image core control is
  2,676,986 (1021%) / 2,171,620 (828%). The overlay frame is exactly
  `core_rest + 2 × overlay_seq` within 0.02%.

Absolute frame numbers are image-specific at this fetch-dominated scale (the
core control moved ~8% between the Phase 1b and PR A images with an unchanged
kernel sequence), so arms are compared within one image. The numbers, caveats
and revised plan are in [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md) and
[`../../ULI2.md`](../../ULI2.md). No real-time claim is made.

`Decoder` is generic over `uli_lite::Kernels` and defaults to
`PortableKernels`; `uli_lite_gba::ArmKernels` is the product realisation. The
`kernels` feature still exposes the four portable functions
(`uli_lite::imdct`) for the bench and kernel twins. On ARM targets those four
functions carry `#[instruction_set(arm::a32)]`; on the host they are unchanged,
and the committed corpus pins prove the codegen change is bit-exact.

`make uli-test` runs both halves from the repository root. The host half runs
from a neutral directory with `--manifest-path`, because the repository's
`.cargo/config.toml` targets the GBA and is not meant to leak into host tests.

Not in this crate (and not supposed to be):

- the product hot path: entropy, bit reader, dequant and M/S move to
  `uli-lite-gba` in PR B;
- hand-written asm kernels (PR C, scoped by PR B);
- a product ROM or playback integration;
- a public license/publish decision (the crate remains `publish = false`).

The core is kept as the oracle on purpose: it is the thing the fast product is
twinned against.

## Licensing

The ULI2 format and reference implementation come from the sibling `gba-audio`
project, which has no LICENSE file. This crate is therefore `publish = false`
until the format/reference licensing is settled. See [`../../ULI2.md`](../../ULI2.md)
and the licensing risk section there.
