# `uli-lite`

A minimal, `no_std`, zero-dependency, zero-allocation decoder for the ULI2
bitstream and the ULA2 album container.

This is the Phase 0 portable core of the ULI2 port described in
[`../../ULI2.md`](../../ULI2.md). It is a bit-exact port of the C reference in
`gba-audio/src/uli/uli_dec_ref.c` and `uli_tables.c`:

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

That ROM is a correctness witness, not a player. The cycle record is Phase 1b's
`make uli-bench` (`examples/uli_bench`): on the same corpus it measures the
worst frame at 2,970,042 cycles = 1133% of the 262,144-cycle product period,
after compiling the four synthesis kernels in ARM mode (which replaced 44
`__aeabi_lmul` software-multiply sites with `SMULL`; 25.3x before that). The
result, the kernel breakdown and the revised plan are in
[`../../ULI2.md`](../../ULI2.md) → “Phase 1b”. No real-time claim is made.

The synthesis kernels are exposed behind the off-by-default `kernels` feature
(`uli_lite::imdct`), so the GBA overlay and the bench can time and twin-test
them without widening the product facade. On ARM targets those four functions
carry `#[instruction_set(arm::a32)]`; on the host they are unchanged, and the
committed corpus pins prove the codegen change is bit-exact.

`make uli-test` runs both halves from the repository root. The host half runs
from a neutral directory with `--manifest-path`, because the repository's
`.cargo/config.toml` targets the GBA and is not meant to leak into host tests.

Not in this crate yet:

- a full-frame decode through the GBA overlay (the overlay's kernels exist and
  are bit-exact, but `Decoder` still calls the core kernels; Phase 1c);
- the entropy/bit-reader hot path in ARM/IWRAM form (Phase 1c);
- hand-written asm kernels (Phase 1d);
- a product ROM or playback integration;
- a Rust encoder (the offline C encoder remains the encoder of record);
- a public license/publish decision (the crate remains `publish = false`).

## Licensing

The ULI2 format and reference implementation come from the sibling `gba-audio`
project, which has no LICENSE file. This crate is therefore `publish = false`
until the format/reference licensing is settled. See [`../../ULI2.md`](../../ULI2.md)
and the licensing risk section there.
