# tests — fixture plan (no fixtures yet)

Scaffold placeholder. Integration tests live here because the library itself is
`#![no_std]`: an integration test is compiled as a **separate crate with `std`**,
so tests can use normal file I/O while the lib never touches it.

## Gate 1 — bit-exactness vs the reference decoder

Decode each packed fixture with `flac-lite` and assert byte-for-byte equality with
`flac --decode` output for the same source. FLAC is lossless: any mismatch is a bug,
never "close enough".

Planned fixtures (generate once, commit small ones; large ones fetched by a script):

| Fixture | Profile axis under test |
|---|---|
| `mono_32k_b2048_f2.gfp` | mono, FIXED low order |
| `stereo_32k_b2048_midside.gfp` | mid/side decorrelation |
| `stereo_32k_b1024_indep.gfp` | independent stereo, smaller block |
| `const_silence.gfp` | CONSTANT subframes (leading/trailing silence) |
| `verbatim_burst.gfp` | VERBATIM subframes (incompressible burst) |
| `escape_partition.gfp` | residual escape records (order 0b1111) |
| `lpc_order14.gfp` | full LPC — decoder-correctness only, not in target profile |
| `8bit.gfp` | 8-bit streams (experimental path) |

Also: hand-built bit patterns for `bits::BitReader` (unit tests inside `src/bits.rs`
can stay `#[cfg(test)]` — `std` is available to unit tests only via the host harness,
so keep them `core`-only and assert on explicit values).

## Gate 2 — negative / robustness cases

Truncated frame, corrupt sync, reserved field values (`0b1111` sample rate,
reserved subframe types), unsupported sample size, non-monotonic manifest offsets,
offsets pointing outside the blob, wrong magic/version. Each must yield the
specific `Error` variant — never a panic, never an infinite loop. (On a ROM a panic
is a frozen game; fuzzing this later is cheap and worth it.)

## Gate 3 — target build

Not a test file: `cargo +nightly check --target thumbv4t-none-eabi
-Zbuild-std=core,alloc` staying green is itself the regression test that keeps
`std` out of the crate.
