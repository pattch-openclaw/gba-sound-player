# `uli-lite-gba`

The **product decoder**: the `uli-lite` reference realised for the GBA — every
hot kernel hand-written as ARM asm in `.iwram`, the entropy/dequant path in
ARM-mode Rust with a closed-world bit reader, and `GbaDecoder`, the full-frame
decoder the player ROMs call.

ULI2 is **GBA-first**: `crates/uli-lite` is the bit-exact reference and oracle
(host, checked), and this crate is what ships. The plan, gates and the full
measurement ledger live in
[`../uli-lite/PLAN-OF-RECORD.md`](../uli-lite/PLAN-OF-RECORD.md).

## Current state (2026-11-19)

mGBA at WAITCNT 4/2, no UI, 262,144-cycle period. Both corpora pinned; the
numbers below are the ones the gates assert.

| | worst | mean | of period |
|---|---:|---:|---:|
| 129-frame fixture decode | **193,317** (f98) | **172,109** | 73.75% / 65.66% |
| album decode wall (16,553 frames) | **194,695** (f4,998) | **169,354** | 74.27% / 64.60% |
| **gate total** (decode + convert + IRQ + DMA) | **226,152** | **200,624** | **86.27% / 76.53%** |

**13.73 points of margin at the worst frame, zero starved periods over 16,552.**
Against the C + ASM reference's own record (decode 69.43%, gate 73.63%) that is
**1.070x C on the decode wall**. Of the remaining gate gap, only 4.84 points are
decode — 7.80 points are the convert/IRQ/DMA half, which costs ~2.9x C's.

`.iwram`: 26,496 B of 32,768 used; stack high-water 948 B (fixture) / 1,988 B
(gate). Bit-exactness is asserted on every arm (`xover=0 hfnz=0`), and the host
twin compares frame-for-frame against `uli_lite`.

## How it is built

**Kernels — hand-written ARM asm**, each twinned bit-exactly against
`uli_lite::imdct` on the ARM7TDMI (`make uli-kernel-twin`):

| kernel | file | vs C |
|---|---|---|
| pre-twiddle | `asm/pre_arm.s` | 1.00x |
| FFT (radix-4, v5) | `asm/fft_arm_v5.s` | 1.05–1.09x |
| post-twiddle | `asm/post_arm.s` | ~1.03x |
| window + overlap-add | `asm/ola_arm.s` | 1.00x |
| mid/side | `asm/ms_arm.s` | 0.64x (mean) |

**Entropy/dequant — ARM-mode Rust** (`src/decoder.rs`), with the data in `Rs` so
the ARM7TDMI multiplier early-terminates, and the per-class tables resident in
IWRAM (`ClassTables`, 560 B) beside `HotState`.

**The bit reader is closed-world.** We own the encoder, the container and the
bake, so the product build carries **no bounds test, no error flag and a 16-bit
refill contract**; a malformed stream is meant to fail the build, not a frame.
The guarantees that makes sound are properties of the *tables in the blob* and
are asserted before the ROM exists — `tests/premise.rs` checks **`rbits <= 5` on
every PAIR class** (measured `0,0,0,0,0,0,1,2,3,4,5` on all five corpora; the raw
digit costs `rbits + 1` bits after the group's refill) and **`esck <= 2`** (escape
field `t = 2z + 1 + esck <= 13`), and `uli_lite` remains the checked oracle. See
`ULI2.md`'s decision log (2026-11-19) and `PLAN-OF-RECORD.md` → "The entropy path
against the C+ASM reference".

## Using it

```rust
// Caller-owned storage: `CodeTables` belongs in EWRAM, `HotState` +
// `ClassTables` in IWRAM.
let mut dec = GbaDecoder::new(album, &mut tables, &mut class, &mut hot)?;
dec.decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))?;
```

`Decoder::<ArmKernels>` is still available when a caller wants the kernel
overlay without the GBA-first hot path; `GbaDecoder` is the product decoder.

## What it is not

- **Not portable.** `.iwram`, ARM state, unchecked addressing and the
  closed-world reader are deliberate GBA choices. The host build exists for the
  twin tests.
- **Not defended against malformed input** — by policy (see above). A
  third-party or re-encoded blob must pass `tests/premise.rs` first.
- **Not a playback claim.** The gate total is measured in mGBA; the hardware
  witness for the player ROMs is separate (`ULI2.md` → U5).

## Gates

- `make uli-test` — the core is bit-exact on the host corpus, and compiles for
  the target.
- `make uli-overlay-test` — host differentials: kernels vs `uli_lite::imdct`,
  a full-corpus decode through the product path vs the reference, both builds,
  plus the closed-world premise.
- `make uli-bench` — on-target: pins, the full product frame, the per-kernel
  and sequence timings, and the kernel twin.
- `make uli-kernel-twin` — the asm kernels bit-exact on the ARM7TDMI.
- `make uli-gate` — the gate total (decode + convert + IRQ + DMA) on the album,
  with the period audit and the starvation count.

Licensing is unresolved with the format/reference; the crate is
`publish = false`.
