# `uli-lite-gba`

The product decoder's first realisation: the four `uli-lite` reference
kernels (`pre`, `fft`, `post`, `ola`) rewritten with **ARM-mode codegen** and
**unchecked addressing**, with their code placed in `.iwram`, plus
[`ArmKernels`] — the `uli_lite::Kernels` implementation that lets
`Decoder::<ArmKernels>` decode a full frame through them.

Since **2026-11-14** ULI2 is GBA-first: this crate is the product path, not a
portable overlay. The live plan is
[`../uli-lite/PLAN-OF-RECORD.md`](../uli-lite/PLAN-OF-RECORD.md).

```rust
use uli_lite::format::{H, M};
let mut x = [0i32; M];
let mut z = [0i32; 2 * H];
uli_lite_gba::pre(&x, &mut z);
uli_lite_gba::fft(&mut z);
```

The arithmetic is a line-for-line transcription of `uli_lite::imdct`; only the
addressing (`unsafe`, with the index bounds written down in `src/lib.rs`) and
the instruction set change. `tests/twin.rs` proves the kernels bit-exact on the
host; `tests/decoder_twin.rs` proves a **full corpus decode** through
`Decoder::<ArmKernels>` matches the reference frame for frame; and
`examples/uli_bench` proves both again on the target, while timing them.

## Why it exists

Phase 1b (`ULI2.md`) measured the portable core at 25.3x the frame period.
Two separate causes were found and separated:

- Thumb-1 has no 64-bit multiply, so the reference's `(a as i64) * (b as i64)`
  compiled to `__aeabi_lmul` (44 call sites, zero `SMULL`). ARM mode fixes
  that: 2.2x on the frame.
- Cartridge instruction fetch at WAITCNT 4/2/no-prefetch dominates ARM code.
  Moving the same kernels to `.iwram` is another 2.5x. Unchecked addressing
  adds 8% on top.

The feature `iwram-tables` additionally copies the 8,672 B synthesis tables
into `.iwram`; it is off by default because it bought only 6.3% in the
measured arm.

## Measured result (PR A, 2026-11-14)

Full frame through `Decoder::<ArmKernels>` on the committed corpus, mGBA,
WAITCNT 4/2, 262,144-cycle period:

| arm | worst | mean |
|---|---:|---:|
| core kernels (same-image control) | 2,676,986 (1021%) | 2,171,620 (828%) |
| `ArmKernels` | **1,829,247 (698%)** | **1,322,800 (505%)** |

Both arms pinned; the overlay frame equals `core_rest + 2 × overlay_seq`
within 0.02%. Kernel sequence per channel: 243,619 vs the core's 667,264.

## Measured result (PR B, 2026-11-14)

`GbaDecoder` (`src/decoder.rs`) now carries the entropy/bit-reader/dequant/M/S
hot path in ARM mode with the 32-bit cached bit window. Full frame:

| arm | worst | mean |
|---|---:|---:|
| core kernels (same-image control) | 2,681,000 (1023%) | 2,175,635 (830%) |
| `ArmKernels` | 1,677,608 (640%) | 1,171,161 (447%) |
| **`GbaDecoder` (full product path)** | **533,605 (204%)** | **463,660 (177%)** |

All pinned, `xover=0 hfnz=0`, on-target twin passed. `HotState` is placed in
EWRAM in the bench: an 8 KiB `.iwram` placement left less stack than the
≈15.0 KiB the product path needs and overflowed on target. Synthesis is now
63% of the frame; the kernels are PR C's target.

## What it is not

- Not yet the complete product decoder: `GbaDecoder` (PR B) covers the bit
  reader, Huffman, dequant, M/S and synthesis for the measured path, but
  playback, the UI and the feeder are PR D.
- Not asm. It is Rust code with ARM-mode codegen and placement; hand-written
  kernels (PR C) are a later, measured decision.
- Not portable: `.iwram`, ARM state and unchecked addressing are deliberate
  GBA choices. The host build exists for the twin tests only.

## Gates

- `make uli-overlay-test` — host differentials: the four kernels against
  `uli_lite::imdct` (full i32 range, int8/int16 OLA, overlap state), and a
  full-corpus decode through `Decoder::<ArmKernels>` against the reference
  decoder, frame for frame, including seek paths.
- `make uli-bench` — on-target twin, both full-frame arms, and per-kernel and
  sequence timings. The numbers and caveats live in
  [`../uli-lite/PLAN-OF-RECORD.md`](../uli-lite/PLAN-OF-RECORD.md).

Licensing is unresolved with the format/reference; the crate is
`publish = false`.
