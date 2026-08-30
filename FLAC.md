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
- No FLAC integration exists yet in the shipping ROM.

---

## Decision (2026-08-30): option (c) — write `flac-lite`

**We are going with (c): a minimal, `no_std`, zero-dependency, zero-allocation FLAC
frame decoder of our own.** Rationale:

- We control **both ends** of the pipeline. Encoding is offline and ours; the
  "filesystem" is memory-mapped, fixed-layout ROM. That deletes the entire stack that
  makes symphonia unportable — `std::io::{Read, Seek, BufRead}`, the container probe,
  the runtime codec registry, `HashMap`, `Error::IoError`. A decoder over a
  `&'static [u8]` with a cursor needs **none** of it.
- (a) means permanently tracking a WIP fork of a moving upstream for a port upstream
  has abandoned; (b) is the same porting cost as (c) but against someone else's
  architecture, which assumes `std::io` too.
- The decode math is ~500 lines of integer work (subframes + LPC + Rice + stereo
  decorrelation). All shifts/adds/multiply-accumulate — **no division**, which matters
  because ARM7TDMI has no hardware divide.

### Architecture

```
offline (host)                       ROM (no_std, core-only)
────────────────                     ────────────────────────
source.wav ──flac -l 4──▶ constrained .flac
                         │  pack script (manifest + raw frames)
                         ▼
                    .gfp blob ──▶ include_bytes!/static ROM
                                      │
                                      ├─ format::Manifest   (stream info + frame index)
                                      ├─ decoder::FrameStream (cursor over frames)
                                      └─ frame → subframe → residual → stereo
                                              │
                                              ▼
                                        double buffer → DMA/mixer
```

**Container: raw frames + an offline manifest (`GAFP` blob).** The pack script strips
STREAMINFO/seektable/Vorbis metadata and emits a manifest holding stream info
(sample rate, channels, bps, blocksize) plus a **frame-offset index**. Consequences:

- Seek is an O(1) table lookup — no `Seek`/`SeekFrom` trait anywhere.
- Decode walks a `&[u8]` cursor; the manifest is borrowed (`&'a`), so the whole decode
  path is **zero-allocation** (`alloc` is not required by the decoder at all).
- Non-conforming files are rejected at *pack* time, not at decode time.

### Constrained encode profile (our FLAC subset)

Because we own the encoder, we pin the features the decoder must support:

- 16-bit, 32kHz to start (65,536Hz later), mono or stereo
- blocksize fixed per track (1024 or 2048)
- `-l 4` → predictors capped at FIXED order 0–4 (full LPC order ≤32 still *parsed*,
  but banned by profile → see perf gate)
- Rice / Rice2 residuals; mid/side + left/right-side decorrelation
- no metadata blocks other than STREAMINFO (stripped by the packer)

### Crate layout (`crates/flac-lite/`)

| Module | Responsibility |
|---|---|
| `bits` | MSB-first bit reader over `&[u8]` (u32 accumulator + `clz`, available on ARMv4T); UTF-8-style coded numbers; byte alignment for verbatim/raw-signature subframes |
| `format` | `GAFP` manifest parse (borrowed, zero-alloc), sample-rate/blocksize tables, encode-profile validation |
| `frame` | frame header (sync `0xFFxF`, blocksize/sample-rate tables, coded frame number, CRC-8 — checked in debug, skippable in release), `decode_frame` |
| `subframe` | CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤32, precisions 0/15/16); warm-up/predictor state |
| `residual` | partitioned Rice, Rice2, and escape-record residual |
| `stereo` | mid/side, left/side, right/side decorrelation |
| `decoder` | top-level cursor API: `FrameStream` + per-frame warm-up state, decode into caller-provided channel buffers |

Memory: two channel buffers of `blocksize` × `i32` (2048 × 2 × 4B = 16KB scratch,
plus ≤32-sample warm-up per subframe) — fits IWRAM/EWRAM with room for the DMA
double-buffer. Release build can downcast accumulation to `i32` throughout.

### Playback path

One frame → fill a half of a double-buffer; DMA/timer IRQ consumes the other half.
2048 samples @ 32kHz ≈ **64ms of audio per frame**, a generous real-time budget per
IRQ boundary.

### Risk & gate (be honest about this)

The open question is not "can we write it" but **whether a 16.78MHz ARM7TDMI sustains
decode + playback in real time**. Gate before building the full decoder:

1. Spike: FIXED-only + Rice (no full LPC) and benchmark a ~10s clip on mGBA with an
   explicit frame-decode **cycle counter** (timer capture around `decode_frame`).
2. If FIXED-only fits but LPC does not, `flac -l 4` graduates from preference to
   **hard project constraint** — and the parser can then reject LPC frames outright.
3. If even FIXED-only misses the budget, fall back to frame-level offline
   pre-processing (e.g. store FIXED order 0–1 only) or reduce scope to short loops.

### Testing

- **Host:** `crates/flac-lite/tests/` (std integration harness — the lib itself is
  `#![no_std]`, tests are separate crates) decoding fixture files and asserting
  bit-exact equality with reference `flac --decode` PCM.
- **Target:** `cargo +nightly check --release --target thumbv4t-none-eabi
  -Zbuild-std=core,alloc` must stay clean — that is the compile gate that symphonia
  could never pass.

#### Cargo config leak (found 2026-08-30, verified by A/B builds)

Cargo reads `.cargo/config.toml` by **walking up parent directories** and merging
every layer; a `[workspace]` boundary does not stop it. So `crates/flac-lite` inherits
the root's GBA config, and **both** inherited values break a naive host test:

| Command (in `crates/flac-lite/`) | Result |
|---|---|
| `cargo +nightly test` | ❌ `E0463: can't find crate for 'test'` — inherited `[build] target` |
| `cargo +nightly test --target <host>` | ❌ `E0152: duplicate lang item core::sized` — inherited `build-std` recompiles `core` under host `std` |
| local `[unstable] build-std = []` | ❌ still `E0152` — cargo **merges arrays across config layers**, an empty local override does not clear the parent's |
| `cargo +nightly test -Zbuild-std= --target <host>` | ✅ both overrides required |

Canonical host gate: `cargo +nightly test -Zbuild-std= --target "$(rustc -vV | sed
-n 's|host: ||p')"`. Alternatively run from outside the repo subtree with
`--manifest-path`, where no parent config exists — which is what splitting
`flac-lite` into its own repo would give us permanently.

> `-Zbuild-std=core,alloc` on the target gate is **not** "using std": rustup ships
> no prebuilt `core` for this Tier-3 target, so `core` must be compiled from source
> (root README: `E0463: can't find crate for core`). The flag is only named after
> `std`. **`std` must never appear in a build-std list in this project** — an early
> draft here suggested `-Zbuild-std=core,alloc,std` for host tests, which was wrong:
> it masked the leak instead of fixing it. The host gate *disables* build-std.
- **ROM:** `examples/flac_spike/` (placeholder) → later a packed clip playing A/B
  against the same WAV.

### Scaffold status (2026-08-30)

Scaffolding only — **no decoding logic implemented yet**:

- `crates/flac-lite/` — `#![no_std]`, zero-dep crate; module skeleton with real type
  and function signatures and `todo!()` bodies (crate-level `allow(dead_code,
  unused_variables)` is tagged for removal as implementations land). Compiles clean
  for `thumbv4t-none-eabi` and for the host test target.
- `crates/flac-lite/README.md` — `GAFP` manifest byte layout + encode profile contract
  (the spec the packer and decoder must agree on).
- `scripts/pack_flac.sh` — documented stub; prints the intended pipeline, exits 2
  (not implemented).
- `examples/flac_spike/README.md` — perf-gate placeholder (no Cargo.toml, so nothing
  can accidentally build it).
- Root crate still carries **no** FLAC dependency; baseline ROM unaffected. `agb`
  integration (path dependency + mixer example) is deliberately deferred until the
  perf gate is settled.

Next steps, in order:

1. [ ] Implement `bits::BitReader` + host unit tests (smallest testable unit).
2. [ ] `format::Manifest` parser + `scripts/pack_flac.sh` real implementation.
3. [ ] `subframe` FIXED + `residual` Rice → **perf gate spike** in mGBA.
4. [ ] LPC + stereo decorrelation + CRC; fixture tests vs reference `flac`.
5. [ ] `examples/flac_spike` ROM: cycle counter, then mixer/DMA double-buffer playback.
