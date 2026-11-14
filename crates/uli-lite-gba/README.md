# `uli-lite-gba`

The GBA overlay's first realisation of the ULI2 synthesis kernels: the four
`uli-lite` reference kernels (`pre`, `fft`, `post`, `ola`) rewritten with
**ARM-mode codegen** and **unchecked addressing**, with their code placed in
`.iwram`. It is the measurement seam between the portable core and a future
hand-written ARM kernel.

```rust
use uli_lite::format::{H, M};
let mut x = [0i32; M];
let mut z = [0i32; 2 * H];
uli_lite_gba::pre(&x, &mut z);
uli_lite_gba::fft(&mut z);
```

The arithmetic is a line-for-line transcription of `uli_lite::imdct`; only the
addressing (`unsafe`, with the index bounds written down in `src/lib.rs`) and
the instruction set change. `tests/twin.rs` proves bit-exactness with the
reference on the host, and `examples/uli_bench` proves it again on the target.

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

## What it is not

- Not the product kernel yet: `uli_lite::Decoder` still calls the core kernels.
  Wiring this crate in is Phase 1c, together with the entropy/bit-reader hot
  path.
- Not asm. It is Rust code with ARM-mode codegen and placement; hand-written
  kernels (Phase 1d) are a separate, later decision.
- Not `no_std`-portable in the placement sense: `.iwram` and the ARM
  instruction set are GBA concerns. The host build is used only for the twin
  test.

## Gates

- `make uli-overlay-test` — host differential against `uli_lite::imdct`, full
  i32 input range, int8 and int16 OLA paths, overlap state included.
- `make uli-bench` — on-target twin plus per-kernel and sequence timings, and
  the projected full-frame cost. The numbers and their caveats are in
  `ULI2.md` → “Phase 1b”.

Licensing is unresolved with the format/reference; the crate is
`publish = false`.
