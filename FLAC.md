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
- blocksize fixed per track (1024 or 2048) — with the **final frame legitimately
  short**, so per-frame blocksize comes from the frame header, not the manifest
- **max predictor order ≤ 4** (encode `-l 4`), `-l 0` for FIXED-only. Predictors
  above order 4 stay *parsed* but off-profile → see perf gate.
  ⚠️ Wording corrected 2026-09-07: this line previously read "`-l 4` → predictors
  capped at FIXED order 0–4", which is not what `-l` does — `-l` is a maximum
  **LPC** order and `-l 4` emits real LPC-4 frames. See
  [Correction 3](#correction-3--l-4-does-not-mean-fixed-order-04).
- Rice / Rice2 residuals; mid/side + left/right-side decorrelation
- sample rate above the 4-bit table (65,536 Hz) travels as code `0b0000` + stream
  default, which needs `--lax` at encode time
- no metadata blocks other than STREAMINFO (stripped by the packer)

### Crate layout (`crates/flac-lite/`)

| Module | Responsibility |
|---|---|
| `bits` | MSB-first bit reader over `&[u8]` (u32 accumulator + `clz`, available on ARMv4T); UTF-8-style coded numbers; byte alignment for verbatim/raw-signature subframes |
| `format` | `GAFP` manifest parse (borrowed, zero-alloc), sample-rate/blocksize tables, encode-profile validation |
| `frame` | frame header (sync `0xFFxF`, blocksize/sample-rate tables, coded frame number, CRC-8 — checked in debug, skippable in release), `decode_frame` |
| `subframe` | CONSTANT, VERBATIM, FIXED (orders 0–4), LPC (order ≤32); warm-up/predictor state. LPC coefficient precision is a **body** field (§9.2.6 Table 22 `u(4)`, after warm-up), not part of the subframe type — the "precisions 0/15/16" note here was a phantom, see [Prior assumptions](#prior-assumptions-that-did-not-survive-measurement) |
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

1. Spike: FIXED + Rice **and** low-order LPC, benchmarking a ~10s clip on mGBA with
   an explicit frame-decode **cycle counter** (timer capture around `decode_frame`).
   Two arms, because the decision is a comparison: a `-l 0` clip (FIXED-only) and a
   `-l 4` clip (LPC ≤4) — measured, these really are different streams
   (`fixed0` ×157 vs `lpc4` ×156), so one clip cannot answer for both.
2. If FIXED fits but LPC does not, cap the profile at **max predictor order 0 via
   `-l 0`** (a stronger constraint than `-l 4`, which permits LPC) and reject
   higher-order/LPC frames in the parser. Restated 2026-09-07: the old wording
   ("`flac -l 4` becomes a hard constraint, reject LPC frames") was incoherent —
   `-l 4` *allows* LPC, so it could never have been the reject-LPC switch.
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
with the decoder in the image. It does not decode audio yet: everything past the
bit reader is still `todo!()` scaffold, so the ROM holds a `#[used]` fn-pointer
**link anchor** that pulls the decode path into the image without running it (what
the ROM does run is a live `BitReader` proof — see
`examples/flac_integration/src/main.rs`) — the build path is fully exercised while
no `todo!()` can panic on hardware. This is the intended shape of the ongoing
check: correctness lives in `flac-test`, bundling/memory/scheduling lives in
`*-flac-rom`, and the two failing independently is the diagnostic.

The root ROM crate still carries **no** FLAC dependency; the baseline is unaffected
if the experiment breaks. `agb` mixer/DMA integration stays deferred until the perf
gate is settled.

### Scaffold status (2026-08-30) — historical snapshot

> **Superseded 2026-09-06.** Accurate as of this date, false as a present-tense
> claim: `bits::BitReader` landed in PRs #25–#32 (41 host tests). Outstanding
> work lives in the **Phased plan** at the end of this file. Kept below for
> provenance — read it as history, never as status.

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

### Completed: `bits::BitReader` (2026-09-04 → 2026-09-06)

**Not a roadmap.** Outstanding work lives in the Phased plan below — the single
source of truth for this project's roadmap. This section is finished history,
kept for provenance and for the lessons baked into each step. `bits` is complete
for the decode path as of PR #32; the only `todo!()`s left in `bits.rs` are
`crc8` and `crc16`, tracked as Phase 2 step 5.

1. [x] `bits::BitReader` + host unit tests — **done 2026-09-06** (PRs #25–#32;
   41 `core`-only tests, thumbv4t compile gate green).
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
   - [x] **`read_signed` — done 2026-09-05**: delegates to `read_bits` (so
     width validation + EOF/cursor rules are identical by construction), then
     sign-extends with the shl-then-arithmetic-shr idiom — no divide, no
     subtract, and **no special case for n == 32** (both shifts are by 0 there,
     which Rust defines; the result is the plain `as i32` reinterpretation).
     7 new `core`-only tests: hand-computed fields (all three sign cases from
     one byte pattern), a deliberately-unaligned negative crossing a byte
     boundary, per-width extremes (−1 at every width over 66 bytes; the
     min-negative at every width), the n=32 boundary (−1 / i32::MIN /
     i32::MAX), width validation, EOF cursor invariants, and an exhaustive
     alignment×width differential sweep vs an independent subtract-based
     oracle (raw − 2^w, a different mechanism than the shift impl). Lesson
     baked into the tests: the first draft's failures were all *test* bugs —
     hand-computed bit fields were mis-derived and the oracle itself had
     dropped its arithmetic shift; the impl never changed. `make flac-test`
     green (31 tests + thumbv4t compile gate).
   - [x] **`read_utf8_coded` — done 2026-09-05**: FLAC's UTF-8-style coded
     number (RFC 9639 **§9.1.5** — the §5.1.4.1/§7.2 cites here were stale
     section numbers from the pre-RFC doc layout, and the RFC number itself
     was a 9629 typo; both fixed repo-wide in this PR). Lead-byte leading-ones
     prefix → total octets (1–7), lead payload + six bits per continuation,
     MSB-first, into `u64` (the format extends UTF-8 to 36-bit values, so
     stock UTF-8 decoders can't be reused). `InvalidField` for stray-
     continuation leads (prefix 1), `0xFF` (prefix 8 — the only invalid
     all-ones lead; `0xFE` is the legal 7-byte lead), and broken
     continuations; deliberately **bit-level**: no canonical-form check
     (libFLAC-identical — stream semantics belong to the frame layer).
     Composite read is **atomic**: partial failures restore the cursor.
     7 new `core`-only tests (~400 assertions): all seven form classes with
     hand-derived vectors incl. the RFC §9.1.5 worked example (51 billion),
     exhaustive 1-byte identity + 2-byte sweeps, invalid-form and truncated-
     number cursor atomicity, and an alignment×sample differential roundtrip
     vs an independent encoder. `make flac-test` green (31 tests + thumbv4t
     compile gate). Lesson baked in again: every failure during this step was
     a *test/vector* bug (mis-derived expectations, a hex-digit-group typo,
     a bit-packing harness bug) — the impl never changed; an independent
     Python oracle script caught three bad vectors before any Rust was
     written.
   - [x] **`byte_align` + `read_u8` — done 2026-09-06**: the last two
     frame-header blockers, together in one `bits`-closing PR
     (`feat/bitreader-byte-align-read-u8`; together because each alone is a
     ~5-line diff and the composite frame-header composition is the shared
     test surface). `byte_align` is the division-free `(8 - (pos & 7)) & 7`:
     infallible, idempotent at aligned, and it cannot overrun — a slice's bit
     count is a multiple of 8, so aligning any in-bounds cursor lands in
     bounds. `read_u8` delegates to `read_bits(8)` (EOF/cursor rules identical
     by construction, same delegation as `read_signed`). Design decision found
     *during* implementation, not before: `read_u8` is **bit-level, not
     alignment-enforcing**.

     > ⚠️ **Corrected 2026-09-07 — the reasoning recorded here was wrong, and
     > confidently so.** This entry justified bit-level `read_u8` by claiming the
     > frame header's fixed fields are **31 bits** ("channels 3") and that the
     > header CRC-8 therefore always lands at bit `31 + 8k`, i.e. **never**
     > byte-aligned. Channel assignment is a **4-bit** field, the fixed fields
     > are **32 bits**, and the CRC-8 is therefore **always byte-aligned**
     > (bit 40 behind a 1-octet coded number, 48 behind 2). Measured against real
     > libFLAC 1.5.0 frames and pinned by `tests/frame_header_layout.rs`. The
     > *conclusion* survives — `read_u8` must stay bit-level — but its real
     > justification is the **frame footer** (the CRC-16 read after subframe
     > padding), never the header. Authoritative layout + the full post-mortem:
     > [Frame header: measured byte layout](#frame-header-measured-byte-layout--the-31-vs-32-bit-correction-2026-09-07).

     10 new `core`-only tests: drop-width sweep at every
     alignment, exhaustive length×position no-overrun sweep, idempotence,
     `read_u8` differential vs `read_bits(8)` + naive oracle at every
     alignment, all-256-byte stream order, EOF cursor invariants, and two
     composite frame-layer patterns — a "faithful 55-bit header tail" and a
     padding→footer read with a counterfactual assertion (the unaligned read is
     a *different* byte, so skipping `byte_align` cannot pass).
     `make flac-test` green (41 tests + thumbv4t compile gate). Lesson baked
     in again: an independent Python bit packer caught two hand-packed vector
     bugs before any Rust ran — sync `0x3FF8` is not a valid sync code (the
     14-bit fixed-blocksize code is `0x3FFE`), and blocksize code 9 is 512,
     not 2048 (codes 8..13 = 256/512/1024/2048/4096/8192; 2048 = code 11).

     > The "faithful 55-bit header tail" pattern was **not** faithful: it was
     > hand-packed from the same wrong 31-bit model, and parsed correctly it is
     > not a valid frame header at all (channels code 3 → 4-channel, reserved bit
     > set). Retired to a clearly-labelled synthetic bit-reader test
     > (`unaligned_coded_number_then_byte_straddling_read`). The lesson this step
     > actually taught, one level up: **a hand-packed vector cannot witness a
     > field-width claim — it only records what the author believed.** Two
     > reviewers had read that test and the prose above and agreed. Only real
     > encoder output is a witness; see
     > [Golden vectors](#golden-vectors--how-to-regenerate-them).
   - `crc8` + `crc16` — **not part of this step.** FLAC polynomials, table-free
     by design (a 256×u16 table is 512 bytes of ROM we could spend elsewhere);
     the perf gate runs with CRC verify skipped, which the design already
     allows (checked in debug, skippable in release), so they were never a spike
     prerequisite. Tracked as **Phase 2 step 5**.

### Phased plan: PoC/perf-gate first, production pipeline after (2026-09-06)

**The roadmap — single source of truth.** The numbering below is execution order
across both phases; nothing outside these two lists is outstanding work.

Re-sequenced deliberately. The perf gate ("can a 16.78MHz ARM7TDMI decode
FIXED+Rice in real time?") is the project's critical unknown, and its answer
could invalidate downstream design choices (e.g. `flac -l 4` hardening,
buffer layout, even the encode profile). So the gate gets the *shortest
honest path*, and the production-only machinery (manifest, packer, CRC
verify, fixtures) waits until the gate resolves. The old list's "manifest
parser before spike" was the *production* build order, not the *gate* order —
the spike does not need it: it can `include_bytes!` a plain `flac -l 4`
encode and locate frames by sync-code scan (`0xFF 0xF8`) or a hand-computed
static offset table. That harness is throwaway on purpose; the GAFP packer
gets built later for the shipping path regardless.

**Phase 1 — path to the perf gate (the only active work):**

1. [x] Finish `bits` for frame-header consumption: `byte_align`, `read_u8`
   (+ host unit tests). Small, division-free, and the genuine last blockers
   in `bits`. **Done 2026-09-06** — `bits` is complete for the decode path
   (CRCs remain, Phase 2); the frame header parse below is unblocked.
2. [x] Minimal `frame` header parse: sync/code checks, blocksize & sample-rate
   tables, UTF-8 coded frame number, **consume** (do not verify) the header
   CRC-8. Enough to position the reader at the subframe data of a real
   `flac -l 4` frame. **Done 2026-09-08** — see
   [Completed: `FrameHeader::parse`](#completed-frameheaderparse-2026-09-08)
   for what landed, including three doc claims the implementation disproved.
   - Prerequisite **done 2026-09-07** (`docs/flac-frame-header-findings`): the
     header's real byte layout measured against libFLAC 1.5.0, the scaffold's
     31-bit/3-bit-channels model corrected, `StreamDefaults` added to the
     `parse` signature ("from stream" codes must resolve, and 65 kHz hits that
     path on every frame), and golden vectors from real encodes committed with a
     regeneration script. See
     [Frame header: measured byte layout](#frame-header-measured-byte-layout--the-31-vs-32-bit-correction-2026-09-07).
   - So the parse now has a witness: implement it against
     `tests/frame_header_vectors.txt` rather than against a hand-packed array.
3. [ ] `subframe` FIXED (orders 0–4) + `residual` partitioned Rice/Rice2 —
   the decode math the gate measures.
   - **Amended 2026-09-07:** if the spike clip is a `-l 4` encode, FIXED alone
     cannot decode it (measured: `lpc4` on 156/157 frames). Either step 3 grows
     low-order LPC, or the spike runs a `-l 0` clip for the FIXED arm — see step 4.
   - **Partial 2026-09-09:** the subframe *header* (`SubframeType::parse` +
     `order()`) landed — see
     [Completed: `SubframeType::parse`](#completed-subframetypeparse-2026-09-09).
     The step stays open: `decode_subframe`, warm-up, the integrators and all of
     `residual` are still `todo!()`.
4. [ ] **Perf gate spike** in `examples/flac_spike/`: ROM embeds a ~10s clip via
   `include_bytes!`; frames located by a **hand-computed offset array** (no GAFP,
   no manifest — deliberately throwaway); timer-capture cycle counter around
   `decode_frame`; decode-loop cadence vs the 64ms/frame @ 2048×32kHz real-time
   budget, measured on mGBA.
   - **Amended 2026-09-07 — sync scan dropped as the frame finder.** Measured on
     real encodes it over-matches by 1.1×–9.4× depending on the audio, and CRC-8
     alone still admits 1–2 false positives per stream; `crc8` stays parked in
     Phase 2. Numbers + reasoning:
     [Frame sync scanning: what actually filters](#frame-sync-scanning-what-actually-filters).
   - **Two clips, not one** (`-l 0` FIXED-only and `-l ≤4` LPC), because the gate's
     question is a comparison and those are measurably different streams.
   - Decision rule (from the risk gate above): FIXED fits but LPC does not → cap
     the profile at **`-l 0`** (which is what FIXED-only means) and reject
     higher-order frames. FIXED-only misses the budget → offline pre-processing
     (FIXED order 0–1) or scope reduction.

**Phase 2 — production pipeline (starts only after the gate resolves):**

5. [ ] `crc8` + `crc16` implementations; wire debug-only frame CRC-8 /
     footer CRC-16 verification into `frame`.
6. [ ] `format::Manifest` (GAFP) parser + `scripts/pack_flac.sh` real
     implementation — the shippable container: O(1) seek table, profile
     validation at pack time, replaces the spike's scan-based harness.
7. [ ] LPC (order ≤32, parse at minimum; enforce the profile per the gate's
     decision) + stereo decorrelation (mid/side, left/side, right/side).
8. [ ] Fixture tests: bit-exact vs reference `flac --decode` PCM across the
     constrained profile.
9. [ ] `agb` integration: replace the integration ROM's `#[used]` link anchor
     with a real decode loop; mixer/DMA double-buffer playback; A/B against
     the same WAV.

---

## Frame header: measured byte layout — the 31-vs-32-bit correction (2026-09-07)

Step 2 is "parse the frame header", so before writing it the header was measured
rather than recalled: real streams encoded by **libFLAC 1.5.0** (`flac` 1.5.0 via
brew on this host) and parsed byte-by-byte with an independent Python parser
cross-checked against **RFC 9639 §9.1.2–9.1.3**. 979 frames across five streams.
The scaffold was wrong in two ways that both land exactly on step 2.

### The layout (what real encoders emit)

```text
byte 0        byte 1        byte 2        byte 3        byte 4..  last
FF            F8            B8            18            00   CE   46...
└──────────────── fixed fields: 32 bits ────────────┘   └─┬─┘  └┬┘
                                                            │     └ subframe 0
 sync 14 = 0x3FFE (fixed blocksize)                          └ CRC-8 (poly 0x07)
 reserved 1 = 0        blocking 1 = 0 (fixed)
 blocksize 4 = 0xB (2048)   rate 4 = 0x8 (32 kHz)
 channels 4 = 0x1 (L/R)     size 3 = 0x4 (16-bit)   reserved 1 = 0
```

Field order and widths, authoritative (RFC 9639 Table 15/16, confirmed on
libFLAC bytes):

| Field | Bits | Notes |
|---|---|---|
| `synccode` | 14 | `0b11111111111110` — **the same 14-bit value for both blocking strategies**. RFC 9639 §9.1 defines a *15*-bit sync `0b111111111111100` followed by the strategy bit, which is why a frame starts `0xFFF8`/`0xFFF9`. Not 8 bits, and **not** 16. (Corrected 2026-09-08 while implementing step 2: this row previously read "`…11` variable", implying a second 14-bit sync code. There isn't one — `0x3FFF` is not a sync, it is a sync plus the variable-blocksize bit, which is why `parse` reads 14 bits for sync and validates the strategy separately.) |
| reserved | 1 | must be 0 |
| blocking strategy | 1 | 0 = fixed blocksize (our profile) |
| blocksize code | 4 | full table below |
| sample-rate code | 4 | full table below |
| **channel assignment** | **4** | **4 bits, not 3** — see the correction |
| sample-size code | 3 | 0 = from stream, 1 = 8, 4 = 16, `0b011` reserved |
| reserved | 1 | must be 0 |
| — fixed fields total — | **32** | lands exactly on byte 4 |
| UTF-8 coded number | 8·1..10 | whole octets, always starts byte-aligned |
| uncommon blocksize | 8 \| 16 | only when blocksize code is `0b0110`/`0b0111` |
| uncommon sample rate | 8 \| 16 | only for rate codes `0b1100..0b1110`; **never 24-bit** (corrected 2026-09-08 — the scaffold comment and this row both said `8 | 16 | 24` with no witness; §9.1.7 defines 8-bit kHz, 16-bit Hz, 16-bit Hz÷10, and libFLAC 1.5.0 emits exactly those) |
| CRC-8 | 8 | **always byte-aligned** |

**Blocksize codes** (RFC 9639 Table 14, restated correctly 2026-09-08 — the
prose here had the `144·2ᵛ` range one code too high and wrongly called the top
two codes reserved; the independent oracle in `scripts/frame_vectors.py` was
right all along, which is what pinned it):
`0b0000` reserved · `0b0001` 192 · `0b0010..0b0101` = `144·2ᵛ` →
**2:576 3:1152 4:2304 5:4608** · `0b0110` 8-bit uncommon (value + 1) ·
`0b0111` 16-bit uncommon (value + 1) · `0b1000..0b1111` = `2ᵛ` →
**8:256 9:512 10:1024 11:2048 12:4096 13:8192 14:16384 15:32768**.

**Sample-rate codes**: `0b0000` **from stream** · `0b0001` 88.2k ·
`0b0010` 176400 · `0b0011` 192000 · `0b0100` 8k · `0b0101` 16k · `0b0110` 22050 ·
`0b0111` 24k · `0b1000` **32k** · `0b1001` 44100 · `0b1010` 48k · `0b1011` 96k ·
`0b1100` = kHz as 8-bit · `0b1101` = Hz as 16-bit · `0b1110` = Hz÷10 as 16-bit ·
`0b1111` forbidden.
**There is no code for 65,536 Hz.** It can only travel as `0b0000` + stream
default — see the profile section below. (Rates that *do* have uncommon-code
representations — measured 56000 as `0b1100`, 48001 as `0b1101`, 10010 as
`0b1110` — encode **without** `--lax`, unlike 65,536, which has no code at all.
Both paths now carry golden vectors.)

**Channel assignment**: `0b0000` mono · `0b0001` L/R · `0b0010..0b0111` 3–8
channels · `0b1000` left/side · `0b1001` side/right · `0b1010` mid/side ·
`0b1011..0b1111` reserved.

### Correction 1 — channel assignment is 4 bits; fixed fields are 32, not 31

`FLAC.md` (this file, the 2026-09-06 `byte_align`/`read_u8` entry) and
`bits.rs`'s header-tail unit test both recorded the fixed fields as
`sync 14 + reserved + blocking + blocksize 4 + rate 4 + channels 3 + size 3 +
reserved` = **31 bits**, and drew a conclusion from it: that the header CRC-8
"always sits at bit `31 + 8k`, **never** byte-aligned", so `byte_align()` before
the CRC would drop the CRC's top bit.

Both the width and the conclusion are wrong.

* **Why it matters:** reading 3 bits for channels desynchronises the cursor by
  one bit for everything after it. The header still "parses" — sync matches,
  blocksize decodes to something plausible — and the damage surfaces later: the
  coded number, the CRC byte, and then the subframe type field are all read from
  the wrong offset. Debugging that means staring at subframe code, not header
  code.
* **Why 4 is forced, format-wise:** the field must encode mono, independent
  stereo, three decorrelation modes, 3–8 channel layouts, *and* reserved values.
  Three bits cannot cover mono + stereo + 3 decorrelation modes without stealing
  the 3–8-channel codes; the spec's own table (Table 16) is 4 bits.
* **Measured:** the CRC-8 lands at bit 40 (1-octet coded number) or bit 48
  (2-octet) — `% 8 == 0` on every one of the 979 frames examined. With 32 fixed
  bits, everything before the CRC is whole octets, so **byte-alignment isn't just
  observed, it's structurally guaranteed**: 32 + 8·(coded octets) + 8·(optional
  uncommon fields, which are 8/16/24 bits).
* **What survives:** `read_u8` staying bit-level. Its real justification is the
  **frame footer** — the CRC-16 read that follows subframe data and its padding
  to a byte boundary, which genuinely can end unaligned — plus verbatim /
  raw-signature samples. The header was never the reason.
* **What to delete from memory:** the idea that `byte_align()` before the header
  CRC is *dangerous*. It is a no-op there. Writing it "defensively" would be
  harmless at runtime but would encode a false claim about FLAC, which is how
  this error propagated in the first place.

### Correction 2 — there is no swapped mid/side (`side_bit` was a phantom)

`format::ChannelConfig::MidSide { side_bit }` was documented as "the swapped-pair
flag (assignment 0b101 vs 0b110)". FLAC has no swapped mid/side variant. Codes
`0b0101` and `0b0110` are **6-channel and 7-channel** layouts. The three
decorrelation codes are distinct and unambiguous, so nothing in the format could
ever set that flag. Removed: a phantom field invites a phantom branch, and
"unknown/never-taken" branches are exactly what a `strict-profile` audit trips over.

### Correction 3 — `-l 4` does not mean "FIXED order 0–4"

The profile said `-l 4` caps predictors at FIXED order 0–4. `flac --help` says:
`-l, --max-lpc-order=#   Max LPC order; 0 => only fixed predictors`. So **`-l N`
is a maximum *LPC* order**, and only `-l 0` is FIXED-only. Measured on the same
10 s source (census over every frame of each encode):

| Encode | Subframe 0 across the whole stream | Decorrelation chosen |
|---|---|---|
| `-l 0` | `fixed0` ×157 | mid-side ×157 |
| `-l 4` | `lpc4` ×156, `lpc3` ×1 | mid-side ×157 |

So a `-l 4` clip is **mostly full-LPC frames**. Three things follow, and they
cascade into the roadmap:

1. **Step 3 (the perf gate's decode math) needs more than FIXED.** If the spike
   embeds a `-l 4` clip, the decoder must decode LPC or it will reject nearly
   every frame. Either the spike embeds a **`-l 0`** clip for the FIXED arm
   *and* a `-l ≤4` clip for the LPC arm (this is now the plan — the gate compares
   FIXED vs LPC cost, which is the actual decision), or step 3 must grow LPC.
2. **The `strict-profile` reject rule changes shape.** "Reject LPC" is not the
   same switch as "reject predictor order > 4". The gate's decision rule
   ("FIXED fits, LPC doesn't → make `-l 4` a hard constraint") needs restating:
   the enforceable constraint is **max predictor order ≤ N plus an explicit
   fixed/LPC flag**, not a FIXED-only assumption.
3. **The profile wording is fixed** (see below): "max predictor order ≤ N,
   `-l 0` for FIXED-only", never "FIXED order 0–4".

### The final frame is legitimately short

A track's last frame carries a **smaller blocksize** than STREAMINFO's maximum:
320,000 samples at blocksize 2048 is 156 full frames plus one 512-sample frame
(visible in the `stereo-last` golden vector: blocksize code `0b1001` = 512 where
the track uses 2048). Anything that filters candidate frames by "blocksize must
equal the track's blocksize" therefore **silently drops the last frame of every
track**. This is not speculation — a first cut of the vector harness did exactly
that, an assertion fired, and the "bad parse" was the harness being over-strict
about a legal stream. Corollary for the packer: frame count is
`ceil(total_samples / blocksize)`, and the manifest must carry the last frame's
sample count (or the decoder reads it per-frame from the header, which is what
`FrameHeader::blocksize` is for).

### Frame sync scanning: what actually filters

The plan lists "locate frames by sync scan" as the spike's frame finder. Measured
over real encodes (`0xFF 0xF8`-shape candidates vs. the true frame count):

| Stream | real frames | sync-shape candidates | + CRC-8 only |
|---|---|---|---|
| `l4_stereo` | 157 | 214 | 157 |
| `l0_stereo` | 157 | 200 | 157 |
| `l4_mono` | 313 | 330 | 313 |
| `l4_silence` | 32 | 32 | 32 |
| `r65k` | 320 | 447 | 320 |
| **total** | **979** | **1223** | **979** |

A second, different source (sine + hash-noise material) was worse: **1476**
candidates for 157 real frames (~9.4×), because noise-like payload bytes happen
to look like sync + reserved-clear far more often. Two conclusions:

* **Sync shape alone is not a frame finder.** 1.1× on clean synthetic tones,
  up to ~10× on noisy material. It's a *recovery* mechanism (`Error::FrameSync`),
  not an index.
* **CRC-8 alone is close but not exact**: the `r65k` and second-source streams
  admit 1–2 false positives each (a payload byte sequence that both looks like a
  header and happens to CRC). What *was* exact, on all 979 frames of both
  sources: **strict RFC field validation + CRC-8**, checked as "frame numbers are
  exactly `0..N-1` in stream order". That is the filter the harness uses.

**Decision for the spike (step 4):** locate frames with a **hand-computed offset
table** generated offline (option (a) from the earlier review) — zero new decoder
code, and it matches the container's real design, where the manifest's offset
table *is* the seek mechanism. Keep `crc8` parked in Phase 2 step 5. Do not ship
sync-scan-without-a-filter.

### What the encoder actually chooses (cite this instead of guessing)

* `-l 0` → FIXED only; `-l N` → LPC up to order N (`-l 4` produced `lpc4`).
* `-m` (try mid/side per frame) chose **mid/side on every frame** of a
  mid/side-shaped source; on a source with uncorrelated channels it chose
  independent L/R for every frame. Left/side and side/right may never appear in a
  given encode — so the golden-vector spec treats them as **optional** vectors
  rather than required ones, and `--no-mid-side` / `-B` are the levers if forced
  coverage is ever needed.
* 65,536 Hz requires **`--lax`** (outside FLAC's streamable subset); every frame
  then carries sample-rate code `0b0000`, i.e. the `FromStreamDefault` path.
* Digitally silent input → **CONSTANT** subframes, and `flac` still emits a
  normal header + Rice-coded residual structure around them.

### `--force-utf8-legacy-noop` is not a real flag

Both `crates/flac-lite/README.md` and `scripts/pack_flac.sh` carry a reference
encode command ending in `--force-utf8-legacy-noop`. libFLAC 1.5.0 rejects it:
`flac: unrecognized option` (the only UTF-8-related flag is `--no-utf8-convert`,
which is about tag charsets, not frame numbers). Both places now use the plain
command. Frame numbers are UTF-8-coded regardless — that is a spec property, not
something to be configured — which is exactly why `read_utf8_coded` exists and why
the 6-byte/7-byte header pair is in the vector set.

## Golden vectors — how to regenerate them

`crates/flac-lite/tests/frame_header_vectors.txt` holds real frame headers cut
from real encodes, with expected field values derived by an **independent RFC
parser**, never by hand-packing and never by asking libFLAC to explain itself.
Consumed by `crates/flac-lite/tests/frame_header_layout.rs`.

**Why machine-generated.** The 31-bit error entered the project through a
hand-packed test vector and survived review *because* it looked authoritative:
whoever packed those bytes believed 31 bits, and the test asserted 31 bits.
A hand-packed vector cannot witness a field-width claim. Encoder output can, so
the vectors are encoder output, end to end.

### Regenerating

```sh
# from the repo root; needs `flac` + `metaflac` on PATH (brew install flac)
./scripts/gen_frame_vectors.sh [output-path]
```

That script: synthesizes the source PCM (`scripts/frame_vectors.py synth`,
integer arithmetic only — no floats, no RNG, so bytes are identical on every
platform), runs each encode profile, runs the harness's **stream invariants**
(fail-closed), and writes the vector table (default:
`crates/flac-lite/tests/frame_header_vectors.txt`). Pass a path to emit somewhere
else without touching the committed table.

Pieces:

* `scripts/frame_vectors.py` — the oracle: RFC tables, a strict frame-header
  parser, CRC-8, deterministic PCM synthesis, frame finding, stream checks,
  vector emission. Subcommands: `synth DIR`, `emit DIR OUT`, `measure DIR`.
* `scripts/gen_frame_vectors.sh` — the deterministic driver (synth → encode →
  check → emit), plus a census of what the encoder chose, appended as comments.

### The invariants that make the table trustworthy

`emit` refuses to write unless, for **every** source stream:

1. frames found == `ceil(total_samples / max_blocksize)` from STREAMINFO;
2. every frame header's **CRC-8 verifies**;
3. coded frame numbers are **exactly `0..N-1` in stream order** — this is the
   assertion that proves the parser reads every field at the right width, since
   one mis-sized field corrupts the coded number;
4. frame offsets strictly ascending;
5. every **non-final** frame's blocksize equals STREAMINFO's maximum (the final
   frame may be short — see above).

And `frame_header_layout.rs` asserts, per vector, that walking the bytes through
`bits::BitReader` reproduces every field, the coded number, the CRC-8 byte, the
exact header bit-length, and that the CRC-8 position is byte-aligned (a
regression tripwire: if that ever fails, someone changed a field width).

### Checking new source material before trusting it

`scripts/frame_vectors.py measure DIR` runs the strict-vs-lenient comparison over
any directory of `.flac` files and prints real/candidate counts per stream, and
**fails loudly** if the strict filter is not exact. Use it whenever adding a
vector source — it is how the sync-scan numbers above were produced, and how the
"final frame is short" over-rejection was caught.

Two things it also taught, both now encoded in the synthesis parameters:

* **A too-pure tonal source makes the vectors lie.** The first synthesis pass
  produced `-l 4` and `-l 0` encodes that were *both* all-FIXED, so the table's
  census line contradicted the `-l` semantics it was there to demonstrate. The
  source needed a broadband floor for LPC to win: the committed synthesis has a
  noise floor and several non-harmonically-related partials, and the emitted
  census (`lpc4` at `-l 4`, `fixed0` at `-l 0`) is the check that it still does.
* **Decorrelation must be earned.** Mid/side vectors only exist because the
  synthesis is `L = center + side`, `R = center − side`. With independent
  channels libFLAC picks plain L/R and the vector silently disappears.

### Vector set (as generated by flac 1.5.0)

| Vector | Why it's in the set | Required |
|---|---|---|
| `stereo-6byte-first` | 6-byte header, 1-octet coded number, CRC-8 at bit 40 | yes |
| `stereo-7byte-num128` | 7-byte header — frame ≥128 forces the 2-octet form | yes |
| `stereo-last` | final frame: **short blocksize** *and* a 7-byte number | yes |
| `mono-b1024-first` | mono (channels `0b0000`), blocksize 1024, `lpc4` subframe | yes |
| `r65k-rate-from-stream` | sample-rate code `0b0000` — the 65 kHz path | yes |
| `fixed-only-first` | `-l 0` encode: FIXED-only, the perf gate's FIXED arm | yes |
| `silence-constant` | CONSTANT subframes from digital silence | no |
| `stereo-midside` | channels `0b1010` | no |
| `stereo-leftside`, `stereo-sideright` | the other two decorrelation modes; **encoder-dependent**, emitted only if chosen | no |

Required vectors fail the generator if absent; optional ones are skipped with
nothing emitted, because "the encoder didn't choose left/side for this source"
is a fact about the source, not a harness bug.

## Prior assumptions that did not survive measurement

Kept deliberately, since each one looked like a reasonable note and cost real
time. The pattern is worth naming: **every one of these was a claim about a
format recorded from recall rather than from bytes**, and every one was in a file
that read like documentation, which is what made it survive review.

| Prior assumption | Reality (measured) | How it was caught |
|---|---|---|
| Frame header fixed fields = 31 bits, `channels 3` | 32 bits, `channels 4` | Python parser over 979 real frames; assertion #3 above |
| Header CRC-8 "never byte-aligned"; `byte_align` there would corrupt it | Always byte-aligned; `byte_align` is a no-op | CRC bit position `% 8` over all frames |
| `read_u8` must be bit-level *because of the header CRC* | True, because of the **footer** (post-padding CRC-16) | Layout walk; footer case checked separately |
| `MidSide { side_bit }` = "swapped pair, 0b101 vs 0b110" | No such variant; `0b0101`/`0b0110` are 6/7-channel | RFC Table 16 vs the scaffold's own comment |
| `-l 4` → "predictors capped at FIXED order 0–4" | `-l` = max **LPC** order; `-l 4` emitted `lpc4`; FIXED-only needs `-l 0` | `flac --help` + per-frame census of both encodes |
| A `-l 4` clip is a reasonable FIXED-path test asset | It is mostly LPC frames — the spike would reject ~all of it | Census of subframe types across a full encode |
| Filter candidate frames by the track's blocksize | The **final frame is legitimately short** → drops the last frame of every track | Harness assertion fired on a legal stream |
| Sync-scan needs no filter ("nearly free") | 1.1×–9.4× over-match; CRC-8 alone still admits 1–2 false positives | `measure` over two different source types |
| 65,536 Hz is reachable with a rate code | No 4-bit code exists; needs `--lax` + code `0b0000` on every frame | `flac` refused without `--lax`; all 320 frames code 0 |
| `bits_per_sample: u8` in the header struct | Code `0b000` would silently yield a bogus number → needs `StreamDefaults` | Signature review against the measured rate-code-0 case |
| `--force-utf8-legacy-noop` in reference encode commands | Not a libFLAC flag — 1.5.0 exits 1 with `unrecognized option` | Ran the command |
| Hand-packed vectors + prose notes are sufficient evidence of layout | They record the author's belief, and two reviewers agreed with a wrong one | This whole pass; vectors are now encoder-derived |
| The 14-bit sync is `0x3FFE` for fixed blocksize, `0x3FFF` for variable | §9.1's sync is **15** bits (`0b111111111111100`) plus the strategy bit; `0x3FFE` covers **both** strategies | Writing `parse` against §9.1 — a 14-bit `0x3FFF` check would reject variable-blocksize streams at the sync instead of at the strategy bit |
| Uncommon sample rate is stored as 8 \| 16 \| **24** bits | §9.1.7: 8-bit kHz, 16-bit Hz, 16-bit Hz÷10 — **never 24-bit** | §9.1.7 + libFLAC 1.5.0 encodes of 56000/48001/10010 Hz; a 24-bit read strands the cursor 8 bits short of the CRC-8 |
| Blocksize codes `0b1110..0b1111` are "reserved"; `144·2ᵛ` starts at `0b0101` | Table 14: `0b0010..0b0101` are the `144·2ᵛ` family, and `0b1110`/`0b1111` are **16384/32768** | Table 14 while writing the blocksize table test; the Python oracle had it right, the prose did not |
| Real headers are 6 or 7 bytes (asserted over the golden set) | **6 to 9** bytes: the uncommon blocksize/rate octets lengthen the header | Regenerating vectors with the tail/rate streams — the old assertion failed immediately, which is what it was for |
| LPC `precision_bits` rides in the subframe-type field (`0b00`→15-bit, `0b01`→16-bit) | The type field carries **no** precision. §9.2.6 Table 22: `u(4)` = precision−1 (0b1111 forbidden) sits in the **body**, after the warm-up samples, then `s(5)` shift, then coefficients | Reading §9.2.6 to implement `SubframeType::parse`; confirmed by walking the field at the real cursor on `-l 4` encodes |
| `SubframeType::parse` reads the type field "plus the order bits that follow" | Nothing follows. Order lives *inside* the 6-bit code: FIXED = v−8, LPC = v−31 (Table 19) | Table 19 while writing `parse` — reading trailing order bits would strand the cursor inside the wasted-bits run |
| Subframe layout `[type][warm-up][wasted bits][residual]` | `[type][wasted bits][warm-up][residual]` — wasted precedes warm-up, and must: warm-up width is `subframe bps × order`, and subframe bps = frame bps − wasted | §9.2.5/§9.2.6 width formulas while placing `parse`'s exit cursor |
| `EndOfStream` is reachable mid-field (a 1-byte slice is too short) | Degenerate: the field is 7 bits and reads are byte-granular, so any non-empty slice holds it; a 1-byte `0xFF` is a pad-bit rejection, not EOF | Writing the EOF test — the reader disagreed with the assertion, and was right |

**Rule going forward:** any claim in these docs about *format bytes* carries its
witness — an RFC section, or a measurement with the command that produced it. If
neither is cited, treat it as a hypothesis, not documentation.

## Completed: `FrameHeader::parse` (2026-09-08)

Phase 1 step 2 landed. `FrameHeader::parse` walks RFC 9639 §9.1.1–9.1.8 in field
order and leaves the cursor exactly on the first subframe bit; the 3–8-channel,
unsupported-depth, reserved-code and forbidden-value cases each return a
distinct, documented error (taxonomy in `frame.rs` module docs). Implemented
alongside it, as the parse's dependencies: `SampleRate::{hz, from_flac_code}`
and `ChannelConfig::subframe_count`. `decode_frame` below it is still `todo!()`
— step 3 is the next thing.

**What the vectors became.** Step 2's rule was "implement against
`tests/frame_header_vectors.txt`, not a hand-packed array", and the same rule
applies to the parts of the header the original set did not cover. So the
generator now encodes five more streams and the table carries **13 vectors**
(was 8): final frames of 1512 and 100 samples — lengths no table blocksize can
express, so libFLAC must emit the uncommon 16-bit and 8-bit forms — and rates
56000 / 48001 / 10010 Hz, one per uncommon sample-rate code. The independent
parser grew to resolve those appended values (blocksize minus 1; rate ×1000 /
×1 / ×10), and its stream invariants still have to hold before the generator
writes anything. Header-length coverage went 6→9 bytes, which broke the
harness's own `vec![6, 7]` assertion the moment real uncommon fields arrived —
the over-narrow assertion, not the decoder, was what was wrong.

**Rejection has no encoder witness, so it gets a mutation witness instead.
libFLAC never emits an invalid stream**, so `Error::FrameSync` /
`InvalidField` / `ProfileViolation` / `UnsupportedSampleSize` cannot be golden-
vected. `tests/frame_header_layout.rs` instead takes a real header out of the
table at runtime and mutates **one field at a time** — sync, each reserved bit,
the blocking strategy, each reserved/forbidden code, a broken coded number, the
§9.1.5 31-bit number cap, blocksize 65536, sample rate 0, an unaligned cursor —
and asserts the specific variant. Every case names its mutation; the untouched
bytes keep the field widths honest.

**Three doc claims the implementation disproved** (all in the table above with
their witnesses): the phantom `0x3FFF` "variable-blocksize sync code", the
phantom **24-bit** uncommon sample rate, and the blocksize prose that called
16384/32768 reserved. Same pattern this file already names — prose about format
bytes written from recall, surviving review because it read like documentation.
Two of the three would have compiled and been *plausibly* wrong: a 24-bit read
lands the cursor 8 bits short of the CRC-8, and a `0x3FFF` sync check rejects
legal streams at the door. Only the third would have failed loudly, by refusing
a legal 16384/32768-sample frame.

**Deferred on purpose** (module docs carry the full list): the §9.1.6 rule that
uncommon blocksize values 1–15 are final-frame-only, and strict-profile
blocksize gating. Neither is judgeable from one header — "final" needs the frame
count, and the final frame's short blocksize looks exactly like a violation to
any frame-local check. Both belong to the packer/decoder loop, not the parser.

**Gates:** `make flac-test` green — thumbv4t compile gate + 45 host unit tests
(4 new, in `format.rs`) + 11 integration tests; `make native-flac-rom` still
builds/links/fixes with the parse in the image; `make check` clean.

## Completed: `SubframeType::parse` (2026-09-09)

Phase 1 step 3's first piece: the §9.2.1 subframe-type field. `parse` reads the
leading zero pad bit + the 6-bit type code (Table 19) and leaves the cursor on
the **wasted-bits flag** — the first bit `decode_subframe` needs. `order()` is
the warm-up count the body carries.

**Two phantoms died here, both in the scaffold's own prose** (rows added to the
prior-assumptions table below; same failure mode this file already names —
format bytes described from recall). My first independent measurement script
reproduced one of them and had to be corrected before it could serve as a
witness, which is the usual value of building the oracle first: **941** real
subframe-0 headers across seven encode profiles, cross-checked against
`flac --analyze`, zero mismatches after the fix.

1. **"`precision` field maps `0b00 → 15-bit, 0b01 → 16-bit`"** (module doc +
   `Lpc { precision_bits }`). The type field carries **no** precision. §9.2.6
   Table 22 places `u(4)` = precision−1 (0b1111 forbidden) *after* the warm-up
   samples, then `s(5)` shift, then the coefficients. `Lpc` now carries `order`
   alone; precision is step 3's body work.
2. **"plus the order bits that follow for FIXED/LPC"** (`parse`'s doc).
   Nothing follows: order is *inside* the 6-bit code (FIXED = v−8, LPC = v−31).
3. Adjacent, caught while re-reading §9.2.1: the module's layout sketch listed
   `[type][warm-up][wasted bits][residual]`. The wasted-bits field comes
   **before** warm-up — and it must, since warm-up width is `subframe bps ×
   order` and subframe bps is `frame bps − wasted`. Wrong only bites at step 3,
   where that width is computed.

**Design decision: `parse` consumes 7 bits, full stop.** Wasted bits are a
property of how the body is coded, not of the predictor's identity, so they are
`decode_subframe`'s (it needs them for subframe bps), not this enum's. The
cursor contract is therefore "lands on the wasted flag", and it is *witnessed*
rather than asserted: the test reads the wasted field from parse's exit cursor
and requires the value to equal the oracle's independently derived number.
Reading too few bits strands the residual mid-header; too many eats the unary
run. `Fixed(1)` with wasted = 5 lands both.

**Witnesses.** The golden-vector table grew a second layer: `scripts/frame_vectors.py`
now derives `subframe0_kind/order/wasted/bytes` for every frame header it
already emits (same independent-parser rule — never hand-packed, never asked of
libFLAC), and a new `subframe-wasted-bits` source stream (coarse square wave,
zero LSBs by construction → FIXED-1 + wasted 5, measured) is the table's only
wasted-bits vector. Committed frame-header vectors are otherwise byte-stable:
regeneration changed nothing but additions. Coverage witnessed on real bytes:
`Fixed(0)`, `Fixed(1)`, `Lpc{3}`, `Lpc{4}`, wasted 0 **and** 5. Still **not**
witnessed, and recorded as gaps rather than coverage: `Constant` (optional
vector only — the committed `silence-constant` vector's `subframe0_*` lines do
carry it, but it is encoder-dependent and can vanish on regeneration) and
**`Verbatim` — no vector at all yet**, needs an incompressible source stream
(measured: full-range integer noise produces `verbatim` frames at `-l 0`, but
libFLAC's default `-l 12` on the same source produces `fixed1`; a future
source/flag pair must pin it). The four Table 19 *boundary* codes (`Fixed(0)`,
`Fixed(4)`, `Lpc{1}`, `Lpc{32}`) never appeared in any encode either, so they
are synthesized from the spec table — with `octet()` calibrated against a real
vector byte first, so the synthesized patterns are provably the encoder's own
encoding of those codes rather than a private invention.

**Rejection has no encoder witness** (libFLAC never emits reserved codes), so
`tests/subframe_header_layout.rs` mutates one field of a real subframe byte: pad
bit set → `InvalidField`; each reserved code range (`0b000010..0b000111`,
`0b001101..0b011111`) → `InvalidField`, at both extremes and middles.

**One lesson about EOF, learned from a test bug:** `EndOfStream` is
*degenerate* for this field. It is 7 bits and reads are byte-granular, so any
non-empty slice holds it — the only reachable error states are the pad-bit and
reserved-code rejections. My first draft asserted that a 1-byte slice was too
short; the reader disagreed, and was right. Also pinned while proving this: a
1-byte `0xFF` is *not* an EOF case either (top bit is the pad bit →
`InvalidField`), which a first draft misread as a failure. The test now names
both traps in place of asserting them wrongly.

**Profile gating stays out.** All legal LPC orders 1..=32 parse as `Lpc`; the
`order > profile.max` check happens where an `EncodeProfile` is in scope — the
same separation `FrameHeader::parse` uses for strict-profile blocksize gating.
"Reject LPC" remains the wrong switch (Correction 3).

**Gates:** `make flac-test` green — thumbv4t compile gate + 45 unit + 11
frame-header + 5 subframe integration tests; `make native-flac-rom` builds/
links/fixes/boot-checks unchanged; `make test-rom` green; `make check` clean.
