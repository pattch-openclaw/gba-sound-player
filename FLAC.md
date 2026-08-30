# FLAC.md — FLAC Integration Evaluation

Durable notes on FLAC-in-Rust for the GBA target. Referenced from README.md (Goal #4).

## The Constraint

Goal #4 is FLAC decoding to fit more audio on cartridge. The hard constraint is the
target itself: `thumbv4t-none-eabi` is Tier-3 bare-metal, `#![no_std]`, **`core` +
`alloc` only**. Any crate that references `std` — directly or through a transitive
dependency — cannot compile for this target.

## Rust FLAC Landscape

FLAC decoding is: parse metadata blocks → per-frame decode (constant / verbatim /
predicted subframes with LPC + Rice residual) → stereo decorrelation → PCM. The math
is cheap integer work; the problem in Rust is not algorithmic, it's ecosystem — the
mature decoders are built on `std::io` abstractions.

| Crate | Notes |
|---|---|
| `symphonia` (`symphonia-bundle-flac`) | Best-engineered pure-Rust FLAC decoder. Full `no_std` support is still upstream WIP; a community fork carries an early no-std branch. |
| `clac` | MIT, smaller surface; also not `no_std` today. |
| `ferrosintesis-flac` | Minimal, patchy API. |
| `flac-rs`, `flac-decoder` | Thin, largely unmaintained wrappers. |

## Candidate Evaluated: `symphonia-bundle-flac` (keks fork, no-std branch)

- Fork: <https://github.com/keks/Symphonia>, branch `chore/core-step-towards-nostd-0.6`
  (commit `8301d26` "chore(core): Replace std with core and alloc, where possible",
  2025-10-18 — note branch title: *step towards* no-std, not *at* no-std).
- Reproducible probe: `examples/symphonia_flac_probe/` (own `Cargo.toml` + `src/main.rs`).
- Build command (native only — this host has no nested virtualization, so no
  podman/docker):

  ```sh
  cd examples/symphonia_flac_probe
  cargo +nightly build --release --target thumbv4t-none-eabi
  ```

### Exact compile errors (verified 2026-08-29 by building the probe)

The build aborts in symphonia's **dependency graph** before symphonia's own code
compiles. Which crate aborts first depends on cargo's parallel build order; both of
these reproduce:

```
error[E0463]: can't find crate for `std`
 --> .../lazy_static-1.5.0/src/inline_lazy.rs:9:1
  |
9 | extern crate std;
  = note: the `thumbv4t-none-eabi` target may not support the standard library
error: could not compile `lazy_static` (lib) due to 1 previous error
```

```
error[E0463]: can't find crate for `std`
 --> .../num-traits-0.2.19/src/lib.rs:23:1
   |
23 | extern crate std;
   = note: the `thumbv4t-none-eabi` target may not support the standard library
error: could not compile `num-traits` (lib) due to 1 previous error
```

Dependency chains (via `cargo tree`):

```
gba-sound-player → symphonia-bundle-flac → symphonia-core → lazy_static 1.5.0
                                         ↘ symphonia-metadata → lazy_static
                           symphonia-core → num-complex 0.4.6 → num-traits 0.2.19
```

- `lazy_static` only omits `extern crate std` when its `spin_no_std` feature is
  enabled; symphonia-core does not enable it.
- `num-traits` is `std` by default; `num-complex` (from symphonia-core's DSP module)
  pulls it in with default features on.

### The failure is NOT dependency-tree-only

The keks commit only touched `symphonia-core`. `symphonia-bundle-flac/src/lib.rs` has
**no `#![no_std]` attribute at all**, and the FLAC crate's own sources still import
`std::` paths. Two classes of remaining usage:

**Trivial (pure re-export aliases — `core`/`alloc` have identical items):**

| Location | Import | `no_std` equivalent |
|---|---|---|
| `symphonia-bundle-flac/src/decoder.rs:8-10` | `std::cmp`, `std::convert::TryInto`, `std::num::Wrapping` | `core::` equivalents |
| `symphonia-bundle-flac/src/decoder.rs:278` | `std::fmt::Write` | `core::fmt::Write` |
| `symphonia-bundle-flac/src/validate.rs:8-9` | `std::mem`, `std::vec::Vec` | `core::mem`, `alloc::vec::Vec` |
| `symphonia-core/src/dsp/fft/no_simd.rs:8-9` | `std::convert::TryInto`, `std::f32` | `core::convert::TryInto`, `core::f32` |

~10 lines across the FLAC crate — sed-level fixes.

**Hard blockers (no `core` equivalent; require API redesign).** All in `symphonia-core`
(and `symphonia-metadata`), self-labeled in-source as "temporary exceptions from no_std":

1. **`errors.rs` — `Error::IoError(std::io::Error)`** (`errors.rs:11,16,48`).
   `std::io::Error` has no `core` equivalent. Every fallible call in symphonia returns
   this error type, so this one variant poisons the entire stack. Fix = replace with a
   custom error-kind enum: API-breaking upstream.
2. **The `io/` module is built on `std::io` traits** — `bit.rs:9,12`,
   `buf_reader.rs:9,12`, `media_source_stream.rs:9,14-15` (incl. `IoSliceMut`, `Read`,
   `Seek`), `monitor_stream.rs:9,11`, `scoped_stream.rs:9,12`, `mod.rs:23,27`.
   `std::io::{Read, Seek, BufRead}` do not exist in `core`. The FLAC crate inherits it:
   `demuxer.rs:8` (`std::io::{Seek, SeekFrom}`), `demuxer.rs:353` and `parser.rs:442`
   (`std::io::ErrorKind::UnexpectedEof`). Fix = own `Read`/`Seek` traits (or
   `embedded-io`), rippling through every reader signature in core + the FLAC demuxer.
3. **`registry.rs:11,16` — `std::collections::HashMap`.** No `alloc` equivalent; needs
   `hashbrown`/`BTreeMap` swap. (A fixed-codec ROM wouldn't need runtime codec
   registry at all — but the code still must compile.)
4. **`formats/probe.rs:11,14,723`** — same `std::io::{Seek, SeekFrom}` +
   `ErrorKind` coupling in the container probe layer.
5. **`symphonia-metadata`** (pulled by `symphonia-common` ← bundle-flac) — pervasive
   `std::io`, `std::collections::HashMap`, `std::sync::Arc` (e.g.
   `id3v2/frames/readers.rs:10-14`, `id3v2/mod.rs:10`, `id3v2/unsync.rs:8`).

Tally: ~27 non-trivial `std::io`/`std::collections`/`std::sync` usages across
core + common + metadata, plus the `lazy_static` (and likely `num-complex`)
dependency-graph problems.

## Upstream Status & Decision

- The no-std work has **languished for years** and has no traction in upstream
  discussions.
- **Waiting on upstream is not an option.** Only manual patches / maintaining our own
  fork would ever make symphonia usable on this target.

Options, honest version:

- **(a) Fork the keks branch.** Swap trivial imports, replace the `IoError` variant,
  introduce core-only `Read`/`Seek` traits, patch `lazy_static`/`num-complex`.
  Est. ~200–400-line diff, mechanical-to-moderate — but permanently tracking a WIP
  branch of a moving upstream.
- **(b) Evaluate `clac`** (MIT, smaller surface; also needs a no-std port).
- **(c) Write a minimal FLAC frame decoder ourselves.** The frame format is well
  documented; subframes + LPC + Rice + stereo decorrelation is ~500 lines of integer
  math. Attractive because cartridge ROM is fixed-layout and memory-mapped (seekable,
  no filesystem): we can skip the entire metadata/demuxer/seek/registry stack and
  decode straight from `static` ROM data into the mixer.

## Status (2026-08-29)

- Probe exists at `examples/symphonia_flac_probe/`; it **fails to compile by design**
  (error above). Root crate no longer carries any FLAC dependency; baseline builds
  clean.
- No FLAC integration exists yet in the shipping ROM. Decision on (a)/(b)/(c) pending.
