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
  bit-exact equality with reference `flac --decode` PCM. Run via `make flac-test`,
  which invokes cargo from **outside the repo** (see "Cargo config leak" — the
  in-tree flag override went stale on nightly drift).
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
| `cargo +nightly test -Zbuild-std= --target <host>` | ⚠️ worked up to nightly ~2026-06-01, **rots** (below) |
| `CARGO_UNSTABLE_BUILD_STD= cargo +nightly test --target <host>` | ❌ still `E0152` (nightly 2026-09-03) |
| `cargo +nightly test --config 'unstable.build-std=[]' --target <host>` | ❌ still `E0152` (nightly 2026-09-03) |
| **cwd outside the repo** + `cargo +nightly test --manifest-path <crate>/Cargo.toml --target <host>` | ✅ non-inheritance is positional, not flag precedence |

##### The flag override rotted; only leaving the tree is durable (found 2026-09-04)

The 2026-08-30 conclusion — `-Zbuild-std=` (empty) + `--target <host>`, "both
overrides required" — held against nightly of that era and is now **false**. On
nightly 2026-09-03 (`rustc 1.100.0-nightly a69a63265`) the inherited
`[unstable] build-std` wins over *every* in-tree override we tried:

| Variant, run in `crates/flac-lite/` (nightly 2026-09-03) | Result |
|---|---|
| `cargo test -Zbuild-std= --target <host>` (the old canonical gate) | ❌ `E0152` while compiling `std` from source |
| `CARGO_UNSTABLE_BUILD_STD= cargo test --target <host>` | ❌ `E0152` compiling `flac-lite` |
| `cargo test --config 'unstable.build-std=[]' --target <host>` | ❌ `E0152` |
| `cargo test --config 'build.target="<host>"' -Zbuild-std=` | ❌ `E0152` |
| same crate, toolchain pinned to `nightly-2026-06-01` | ✅ passes |
| cwd `=/tmp/...` (outside repo) + `--manifest-path`, no flags | ✅ passes |

Reproduce with `drafts/flac-host-gate-matrix.sh` (workspace, not committed). The
`E0152` message names *which* `core` lost: the first definition comes from
`target/<host>/debug/build/core/*/out/libcore-*.rmeta` — a build-std `core` baked
*inside the crate's own target dir* — colliding with the toolchain's prebuilt host
`core`. So stale artifacts poison subsequent runs too; every row above was measured
after `rm -rf target/<host>`.

**Canonical host gate (Makefile `flac-test`, 2026-09-04):** run cargo from a cwd
outside the repo so the config walk terminates immediately, and name the crate with
`--manifest-path`:

```sh
REPO=$(git rev-parse --show-toplevel)   # resolve BEFORE leaving the tree
mkdir -p /tmp/gba-sound-player-host-gate && cd /tmp/gba-sound-player-host-gate && \
  cargo +nightly test \
    --manifest-path "$REPO/crates/flac-lite/Cargo.toml" \
    --target "$(rustc -vV | sed -n 's|host: ||p')"
```

(The manifest path must be absolute and resolved *before* the `cd` — the Makefile
uses `$(abspath crates/flac-lite/Cargo.toml)` for exactly this reason.)

Nothing is inherited, so there is nothing to override — no `-Zbuild-std=` dance, and
no exposure to whatever nightly decides about flag-vs-config precedence. The build
dir is derived from the *manifest*, not the cwd, so artifacts still land in
`crates/flac-lite/target/` (the container cache volumes keep working). This is the
same effect splitting `flac-lite` into its own repo would give us, without the split.

> `-Zbuild-std=core,alloc` on the target gate is **not** "using std": rustup ships
> no prebuilt `core` for this Tier-3 target, so `core` must be compiled from source
> (root README: `E0463: can't find crate for core`). The flag is only named after
> `std`. **`std` must never appear in a build-std list in this project** — an early
> draft here suggested `-Zbuild-std=core,alloc,std` for host tests, which was wrong:
> it masked the leak instead of fixing it. The host gate *disables* build-std.

##### Second leak variant: duplicated `-Tgba.ld` (found 2026-08-30, by building)

The same parent-directory walk bites `rustflags`, and the failure mode is
*linking*, not type-checking. `cargo` **concatenates** `target.<triple>.rustflags`
across config layers, so a standalone sub-crate that re-declares the root's config
locally gets the linker script **twice**:

```
error: linking with `rust-lld` failed
  = note: rust-lld: error: gba.ld:15: region 'ewram' already defined
          >>>     ewram (w!x) : ORIGIN = 0x02000000, LENGTH = 256K
  note: "-Tgba.ld" "-Tgba.ld"
```

(The `agb` build script only adds `cargo:rustc-link-search`, not the `-T` arg —
the `-Tgba.ld` in `.cargo/config.toml` is the sole source, so declaring it twice
is purely a config-layer merge artefact.)

**Rule: standalone sub-crates must NOT re-declare the root `.cargo/config.toml` —
they inherit it.** `crates/flac-lite/` follows this (no local config); so does
`examples/flac_integration/`. `examples/symphonia_flac_probe/` predates the finding
and carries a redundant local copy — harmless there only because the probe dies
long before linking. If a future sub-crate needs to *run* cargo for the GBA target,
keep config inherited and pass extra flags on the command line.
- **ROM:** `examples/flac_spike/` (placeholder) → later a packed clip playing A/B
  against the same WAV.

### Build gates are now Makefile targets (2026-08-30)

The gates above stopped being ad hoc commands and are now standardized entrypoints
in the root `Makefile` (full table in README → "Build Process (Standardized)"):

| Make target | Gate |
|---|---|
| `make flac-test` | **both flac-lite gates above** (thumbv4t check + host tests), library in isolation |
| `make native-flac-rom` / `make podman-flac-rom` | new: `flac-lite` bundled into a bootable ROM via `examples/flac_integration/` — the *integration* sanity check |
| `make test` | `test-rom` (agb `#[test_case]` suite in mGBA) + `flac-test` |
| `make native-rom` / `make podman-rom` | baseline ROM, one per compute environment |

`examples/flac_integration/` is a standalone workspace ROM crate (`agb` +
path-dependency on `crates/flac-lite`) that **compiles, links, fixes, and boots**
with the decoder in the image. It does not decode yet: `flac-lite` is scaffold, so
the ROM holds a `#[used]` fn-pointer **link anchor** that references the decode
path without ever calling it — the build path is fully exercised while no `todo!()`
can panic on hardware. This is the intended shape of the ongoing check: correctness
lives in `flac-test`, bundling/memory/scheduling lives in `*-flac-rom`, and the two
failing independently is the diagnostic.

The root ROM crate still carries **no** FLAC dependency; the baseline is unaffected
if the experiment breaks. `agb` mixer/DMA integration stays deferred until the perf
gate is settled.

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
   **Not done**: the read path is implemented and hardware-validated, but 4
   reader methods and both CRC helpers are still `todo!()` scaffold (listed
   below).
   - [x] **Core read path — done 2026-09-04** (PR #25): `new`, `read_bits`,
     `bit_position`, `bits_remaining` on the **position-only** design — cursor
     is a plain bit offset, no refill accumulator. All byte touching lives in
     the private `peek_at(pos, n)`. 12 `core`-only unit tests (hand-computed
     patterns, exhaustive alignment×width sweep, differential bit-by-bit vs
     wide-read vs independent oracle, EOF/cursor invariants). Both gates green.
   - [x] **On-hardware PoC — done 2026-09-05** (PR #25, validated on mGBA *and*
     real hardware): the FLAC integration ROM (`make native-flac-rom` /
     `podman-flac-rom`) embeds a hard-coded 10-byte test vector, reads it back
     through `BitReader` in the decoder's representative field pattern
     (unaligned widths, byte-crossing reads, cursor + EOF checks), logs
     expected-vs-actual per field over mGBA serial, and shows the verdict on
     screen: **blue = all matched, red = mismatch** (purple = proof never
     completed). Expectations are hand-computed from the bit layout, not from
     the library — so the ROM is an independent oracle, and the colour report
     works on cart with no cable.
   - [x] **`peek_bits` — done 2026-09-05**: direct wrap of the existing
     `peek_at(pos, n)` helper (takes `&self` — peeking needs no cursor
     mutation), so width validation (`InvalidField` for 0/>32) and EOF
     semantics are identical to `read_bits` by construction, cursor never
     moves. 5 new `core`-only tests: peek==read agreement + idempotence,
     width validation, EOF cursor invariants, the off=3/n=32 five-byte
     worst case via peek, and an exhaustive alignment×width sweep against
     the independent oracle. `make flac-test` green (17 tests + thumbv4t
     compile gate).
   - [ ] `read_signed` — two's-complement sign extension after `read_bits`.
   - [ ] `read_utf8_coded` — FLAC's zero-padded prefix numbers (RFC 9629
     §5.1.4.1 / §7.2), used for frame/sample numbers and channel assignments.
   - [ ] `byte_align` — discard to the next byte boundary, returning bits
     dropped (0..7); division-free (`& 7`).
   - [ ] `read_u8` — byte-aligned single byte (CRC-8 / padding).
   - [ ] `crc8` + `crc16` — FLAC polynomials, table-free for now; revisit a
     256×u16 table only if the perf spike shows it pays for itself.
2. [ ] `format::Manifest` parser + `scripts/pack_flac.sh` real implementation.
3. [ ] `subframe` FIXED + `residual` Rice → **perf gate spike** in mGBA.
4. [ ] LPC + stereo decorrelation + CRC; fixture tests vs reference `flac`.
5. [ ] `examples/flac_spike` ROM: cycle counter, then mixer/DMA double-buffer playback.
