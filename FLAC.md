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
3. [x] `subframe` FIXED (orders 0–4) + `residual` partitioned Rice/Rice2 —
   the decode math the gate measures. **Done 2026-09-18** — substeps 3a–3f
   all landed; the Phase 1 decode path runs end to end
   (`decode_frame` witnessed bit-exact against `flac -d` on real frame
   runs). Phase 1's remaining work is the step 4 perf gate.
   - **Amended 2026-09-07:** if the spike clip is a `-l 4` encode, FIXED alone
     cannot decode it (measured: `lpc4` on 156/157 frames). Either step 3 grows
     low-order LPC, or the spike runs a `-l 0` clip for the FIXED arm — see step 4.
   - **Partial 2026-09-09:** the subframe *header* (`SubframeType::parse` +
     `order()`) landed — see
     [Completed: `SubframeType::parse`](#completed-subframetypeparse-2026-09-09).
   - **Substeps 3a–3f (ordered 2026-09-10; dependency-driven).** Step 3 is two
     independent subtracks that meet only at `decode_subframe`: a **bit side**
     (reader → residual samples, strictly sequential internally) and a **math
     side** (residual samples → PCM, pure array transforms with no bit-reader
     dependency). The order below gives every PR an independent witness;
     3d can run parallel to 3b/3c if capacity allows. Each substep = its own
     PR, each keeps both flac gates green, each follows the project witness
     rule — bit-layout claims pinned to encoder bytes, integer maps to spec
     text plus a differential oracle:
     - [x] **3a — leaves (done 2026-09-10, PR pending):** the three pieces
       nothing else depends on. `BitReader::read_wasted_bits` (§9.2.2 flag +
       unary, atomic cursor, all alignments vs a naive oracle),
       `residual::rice_unmap` (§9.2.7.2 folded-residual sign map — which the
       scaffold had sign-flipped; see the step 3a entry), and
       `PredictorState::new`/`fill` shaping — which required deleting the
       scaffold's "warm-up = previous frame's tail" myth, measured false
       (`scripts/measure_warmup_semantics.py`: 64/64 frames carry their OWN
       first samples, 0/64 match the previous frame's tail). See
       [Completed: step 3a leaves](#completed-step-3a-leaves--wasted-bits-reader-rice_unmap-and-the-warm-up-correction-2026-09-10).
     - [x] **3b — `decode_rice_partition` (done 2026-09-11):** the hot inner
       loop (zeros-count + remainder + `rice_unmap`), the §9.2.7.3 `i32::MIN`
       rejection, and the escape-record partition (raw two's-complement
       samples). **Witness scope, per Sam's 2026-09-11 direction:** the
       frame_vectors.py per-partition oracle extension and the on-host
       micro-benchmark are **deferred** — unit tests + an independent bit
       packer suffice for correctness now, and perf measurement is a
       dedicated later effort once more of the real implementation exists.
       If it later turns out real-world partitions differ hugely from the
       synthetic ones, that's the trigger to build the harness. See
       [Completed: step 3b](#completed-step-3b--decode_rice_partition-2026-09-11).
     - [x] **3c — `decode_residual` (done 2026-09-11):** residual header
       (coding method, partition order), per-partition sample-count rules
       (first partition loses the predictor order), escape/Rice dispatch per
       partition. **The planned "sample-size header with its `0b111` unknown
       escape" turned out to be a phantom — no such field exists in a coded
       residual** (witnessed before implementation; see
       [Completed: step 3c](#completed-step-3c--decode_residual-2026-09-11)).
       Witness: RFC 9639 Appendix D.2.7 — a real libFLAC 1.3.3 residual whose
       15 samples the RFC itself publishes (Table 39) — reused as committed
       data per Sam's 2026-09-11 direction (no new encoding), plus
       packer-oracle unit tests for the header rules.
     - [x] **3d — integrators + `PredictorState::fill` (done 2026-09-12):**
       `integrate_fixed` (orders 0–4 as nested running sums, no multiplies —
       Table 20), `integrate_lpc` (§9.2.6 dot-product + `>> shift`,
       most-recent-past coefficient order), and `fill` (warm-up read:
       `read_signed(subframe bps) × order`, `<< wasted` padding, oldest-first
       in stream / stored most-recent-first). Pure array transforms: verified
       against *synthetic* residuals + an independent Python reference — no
       bitstream dependency, no golden-vector work needed. FIXED-as-cascade
       vs FIXED-as-dot-product agreement is a free cross-check. **Done** —
       see [Completed: step 3d](#completed-step-3d--integrators--predictorstatefill-2026-09-12).
     - [x] **3e — `decode_subframe` (done 2026-09-15):** the composition
       landed — wasted read → §9.2.2 bps gate → `fill` → body dispatch
       (CONSTANT / VERBATIM / residual) → LPC fields → integrate → return
       type. The step's finding: **wasted padding applies once, at block
       exit, after prediction** (pad-before disproven on the LPC × wasted
       crossing; `lpc-wasted-256` is the committed discriminating vector).
       The VERBATIM golden-vector gap closed with the fail-closed `-l 0`
       incompressible-source pair, and per-subframe ground truth now exists
       (warm-up values, bps, `exit_bits`, reference PCM). See
       [Completed: step 3e](#completed-step-3e--decode_subframe-composition-and-the-wasted-scale-rule-2026-09-15).
     - [x] **3f — `decode_frame` + decorrelation (done 2026-09-18):** *(split
       2026-09-16; landed in two parts.)* Part 1 — `stereo::decorrelate` +
       the side-width/orientation measurements — see
       [Completed: step 3f part
       1](#completed-step-3f-part-1--stereodecorrelate-and-the-side-width-and-orientation-measurements-2026-09-16).
       Part 2 — the frame wiring: subframe loop over 1–2 slots with the side
       coded at the measured **bps + 1** (orientation derived from the
       header's channel code, never a caller hint), `decorrelate` after the
       loop, footer (`byte_align` + CRC-16 **consume** — verify stays Phase
       2), cursor contract witnessed by chaining one reader frame N → N+1,
       and the end-to-end witness: five libFLAC 1.5.0 frame runs (one per
       channel assignment, 256 + a 100-sample uncommon-blocksize tail)
       decoded twice — through the *existing* primitives first (layout
       suite: the table's ground truth pinned before any frame code existed)
       then through production `decode_frame`, **bit-exact** against
       `flac -d` PCM (pulls step 8's diff forward onto the frame layer).
       See
       [Completed: step 3f part 2](#completed-step-3f-part-2--decode_frame-and-the-frame-run-vectors-2026-09-18).
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
   - **Concrete plan + validation process (2026-09-18):** this step executes as
     five PRs — see
     [Perf gate spike — concrete plan and validation process](#perf-gate-spike--concrete-plan-and-validation-process-2026-09-18).

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

## Perf gate spike — concrete plan and validation process (2026-09-18)

**The concrete plan for Phase 1 step 4.** The decode path (`bits` →
`decode_frame`) landed 2026-09-06 → 09-18, and both measurement arms run today:
LPC ≤ 32 is implemented (3d/3e), so `-l 0` and `-l 4` clips both decode. This
section is the execution plan — five focused PRs, each with its own witness,
each keeping the existing gates green. The sketch in
`examples/flac_spike/README.md` keeps its intent ("what gets measured"), but
this plan supersedes it; the budget correction below must land in that README
via PR 5.

### Budget, derived (not recalled)

At the initial profile (2048-sample frames @ 32,768 Hz), the real-time budget
per frame is:

```text
2048 / 32768 s × 16.78 MHz ≈ 1,048,750 cycles/frame ≈ 512 cycles/sample
```

⚠️ `examples/flac_spike/README.md` states "62,500 cycles per frame" — that
figure is 62,500 **microseconds** (62.5 ms) with its unit misapplied; the same
formula one line above it yields ~1,049,000 cycles at 16.78 MHz (16.78× more).
Its working targets (≤ 35,000 mean / ≤ 55,000 max) inherit the error: they are
~3% of the true budget, so they are accidentally *stricter* than real-time,
while claiming to be a margin. PR 3 re-derives the working target against
**measured** mixer/DMA/vsync load rather than any recalled figure, and PR 5
corrects the README. Until then the gate's question is the one FLAC.md has
always asked: **does the worst frame fit the real-time budget with room left
for the playback path?**

### The five PRs

1. **Spike scaffold + assets.** **Done 2026-09-19** — see
   [Completed: spike PR 1 — scaffold + embedded
   clips](#completed-spike-pr-1--scaffold--embedded-clips-2026-09-19).
   Standalone workspace crate
   `examples/flac_spike/` — inherits the root `.cargo/config.toml`, never
   re-declares it (the duplicated `-Tgba.ld` rule). New Makefile gates
   `make native-spike-rom` / `podman-spike-rom`, mirroring the `*-flac-rom`
   pattern. Assets: extend `scripts/frame_vectors.py` (new `spike-assets`
   subcommand) to synthesize one deterministic ~10 s source (integer-only, the
   existing synthesis rule) and encode both arms — `flac -l 0 -b 2048` and
   `flac -l 4 -b 2048`, 16-bit stereo 32 kHz (the existing 157-frame `l4_stereo`
   / `l0_stereo` streams already have this shape) — emitting the raw frame
   region plus a **committed offset array** produced by the existing strict
   frame finder (the only filter proven exact on all 979 frames). No sync scan,
   no GAFP, no manifest — deliberately throwaway. ROM `include_bytes!`s both
   clips and boots showing metadata (frame count, sizes, subframe census).
   *Witness:* the existing fail-closed stream invariants before any emit;
   a census assertion (`-l 0` all-FIXED, `-l 4` majority LPC) appended as
   table comments; a host integration test that decodes the **exact embedded
   bytes** through `decode_frame` and asserts bit-exact `flac -d` PCM; ROM
   builds, links, fixes, boots.
2. **On-target decode correctness.** **Done 2026-09-20** — see
   [Completed: spike PR 2 — on-target decode
   proof](#completed-spike-pr-2--on-target-decode-proof-2026-09-20).
   The spike ROM decodes every frame of both
   embedded clips; an FNV-1a checksum of the decoded PCM is compared against
   expected values computed on the host from `flac -d` output. Screen verdict
   per clip (blue = match, red = mismatch) plus per-frame serial log — the
   BitReader PoC's convention. Buffers per the memory plan (per-channel
   `blocksize × i32` scratch + alternating halves; EWRAM has room).
   *Witness:* the embedded ROM bytes themselves — correctness is proven on the
   same image that will be measured. **This PR gates PRs 3–5: no perf number is
   trusted from a ROM image whose decode has not been proven correct on its own
   embedded bytes** (a fast wrong decode measures nothing).
3. **Cycle harness + per-frame cost.** A free-running 16.78 MHz counter around
   `decode_frame` — DIV result counter (32-bit, full-speed cycle ticks) or
   overflow-chained timer, chosen at implementation; empty-window calibration
   run first so harness overhead is subtracted and reported alongside raw.
   IRQs off during measurement windows: this is decode cost, not system cost.
   Report per clip per arm: min / max / mean cycles + worst-frame index to
   mGBA serial; screen shows worst-frame-vs-budget verdict.
   *Witness:* calibration window measures ≈ 0 net cycles; repeated runs of the
   same ROM give identical numbers (deterministic emulator, deterministic ROM —
   any variance is harness noise and gets fixed before results are trusted).
4. **Cadence test.** Full-clip loop: decode frame N into buffer half B while
   half A "plays", alternating, for the whole ~10 s of each clip, with no
   artificial waits — pass = every frame's decode stays inside the time the
   previous half would take to play. The closest available real-time simulation
   until the mixer/DMA path exists (Phase 2 step 9), where it becomes a true
   IRQ-driven test. The legitimately short final frame is inside the loop; its
   cost counts.
   *Witness:* zero over-budget frames per arm + worst-frame margin recorded;
   both arms (`-l 0` and `-l 4`) measured on the same run.
5. **Verdict + decision.** Results table into FLAC.md: arm × {mean, max, worst
   frame, % of budget} × {mGBA, hardware}. One real-hardware confirmation run
   of the exact PR-4 ROM on flash cart (the BitReader PoC proved on-cart runs
   work; mGBA stays the primary measurement — it is an emulator, and the table
   records any hardware divergence). Then execute step 4's decision rule:
   LPC fits → profile stays a preference; FIXED fits but LPC doesn't → cap the
   profile at `-l 0` (enforcement lands with Phase 2 steps 6–7, pack-time
   strict-profile validation); FIXED-only misses → offline pre-processing
   (FIXED order 0–1) or scope reduction. PR 5 also corrects the spike README's
   budget units and restates the encode-profile wording per the verdict —
   remembering Correction 3: the enforceable constraint is max predictor order
   + a fixed/LPC flag, never a "reject LPC" assumption baked into the parser.

### Validation process (applies to every PR)

- **Existing gates:** `make flac-test` (thumbv4t compile + host suites) and
  `make check` green on every PR. The root ROM crate carries **no** spike
  dependency — the baseline `.gba` never sees this code.
- **New gate from PR 1:** `make native-spike-rom` and `make podman-spike-rom`
  build the spike ROM native *and* in the container, like the integration ROM.
- **Correctness-before-speed rule:** PR 2 gates PRs 3–5. Numbers are only ever
  reported from a ROM image that has proven its decode correct (host bit-exact
  + on-target checksum) on its own embedded bytes.
- **Asset discipline:** clips come only from the deterministic synthesis
  (integer arithmetic, no floats/RNG); censuses and stream invariants fail
  closed at emit time; committed blobs are generator output, never hand-edited
  — the hand-packed-vector rule, applied to whole clips.
- **Measurement discipline:** harness overhead subtracted and reported
  alongside raw; the **same ROM bytes** used for host checks, mGBA runs, and
  the hardware run (clips live in `.text`, as the BitReader PoC's bytes do);
  the tail is first-class — one over-budget frame is an audible glitch, so max
  is reported alongside mean; repeat runs must be identical or the harness is
  wrong.
- **Claims carry witnesses:** budget math shown as arithmetic (above); working
  targets tied to *measured* mixer/DMA/IRQ load when Phase 2 step 9 makes that
  possible, never to a recalled margin; the verdict table cites ROM checksum,
  mGBA version, and run date per row.

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
  independent L/R for every frame. Left/side and side/right **do** appear,
  just rarely from `-m`-default sources — corrected 2026-09-16 by
  construction: a near-identical-channels source (R a smooth triangle, L = R
  + tiny quantized noise) makes libFLAC 1.5.0 choose **side/right on 63/63
  frames**, and an identical-quantized pair produced left/side on 63/63; the
  anti-phase source that looks like the obvious side/right bait picks
  **mid/side** instead (the encoder's per-frame cost model wins the argument,
  not intuition). The golden-vector spec still treats them as **optional**
  vectors — the *committed* vector source does not produce them — and
  `--no-mid-side` / `-B` remain the levers if forced coverage is needed. The
  corrected claim matters because "never" was load-bearing for skipping
  orientation witnesses; see step 3f part 1.
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
| Warm-up samples are "the previous frame's tail samples", the only cross-frame state the decoder retains | Warm-up lives **inside each subframe's own header**, unencoded; §9.2.5: "each subframe in FLAC is coded completely independently". Frame 0 has warm-up and no previous frame. Consequence: frames are independently decodable, `seek_frame` is safe by construction, `PredictorState` is scratch, not history | `scripts/measure_warmup_semantics.py`: over real `-l 0` and `-l 4` encodes, every frame's warm-up equals its OWN first decoded samples (64/64) and none equal the previous frame's tail (0/64). Found while implementing step 3a, before any code depended on the myth |
| `rice_unmap` scaffold doc: "odd → (n+1)/2, even → −n/2" (`negative = n & 1`) | Sign-flipped. §9.2.7.2 folds `x ≥ 0 → 2x`, `x < 0 → −2x − 1`, so the decode is **even → `n>>1`, odd → `!(n>>1)`** | The RFC's own worked example (folded 38 → +19; the scaffold mapping returns −19), pinned as a unit assertion |
| Escape partition `raw_bits` is a "4-bit field" (`PartitionHeader` doc) | §9.2.7.1: **5 bits** follow the escape code (so widths reach 31, and the 4-bit note would truncate a legal escape partition) | Reading §9.2.7.1 to implement 3b's escape branch |
| `quotient.checked_shl(order)` is a sufficient folded-value overflow guard | `checked_shl` only rejects shift *amounts* ≥ 32 and **wraps** value bits shifted out: quotient 4 at order 30 yielded `folded = 0` | The 3b rejection test asserted `InvalidField` and got `Ok(())` — the impl disagreed with the assertion, and the assertion's author was wrong about `checked_shl`, not the code |
| A coded residual carries a **sample-size header** with a `0b111` "unknown" escape, derived by scanning partitions (scaffold module doc + the original 3c plan line) | **No such field exists.** §9.2.7 is complete with method `u(2)` + partition order `u(4)` + per-partition parameter (`u(4)`/`u(5)`, escape adds raw width `u(5)`); residual width is implicit in the codewords, nothing is derived | Implementing 3c against §9.2.7 read in full + Appendix D Table 38 (real residual: header, parameter, straight into quotients) + libFLAC's `read_residual_partitioned_rice_` (reads nothing else). Caught before any code depended on it — the planned 8 phantom-bit read would have desynced every frame |
| A hand-packed byte can witness a residual-header rejection (`0b0000_0001` = "partition order 1") | MSB-first that byte is method `00` + order `0b0000` = 0 — **legal** for the test's blocksize; the test would have passed on an unrelated EOF | The 3c probe test disagreed with its own bytes; fixed structurally by routing every header probe through the `Packer` oracle so named fields are encoded fields |
| The side subframe is coded at frame **bps − 1** (3f plan line, inherited by `decode_subframe`'s "caller's `-1`" seam note: "mid/side & friends code the side at bps−1") | The side is coded at **bps + 1** — the opposite direction. §4.2: "The side channel needs one extra bit of bit depth, as the subtraction can produce sample values twice as large"; libFLAC 1.5.0 `read_subframe_` does `bps++` on the side slot (channel 0 for right-side, channel 1 for left/mid-side). A `−1` would desync every decorrelated frame and reject legal warm-ups. | Anti-phase encode (R = −L, \|L−R\| beyond 16-bit): side warm-up read at bps+1 == `flac -d`'s L−R on 24/24 frames, 12 of them values the 16-bit field **cannot hold**; the same read at bps disagrees on 24/24. `drafts/flac3f_width_probe.py` (throwaway), libFLAC 1.5.0 |
| Scaffold `stereo.rs`: "right/side: `right` = primary" (subframe 0 holds R) — flatly contradicting `format::ChannelConfig::RightSide`'s doc ("subframe 0 is the side"); two crate docs disagreed and neither was trusted | Subframe 0 is the **side**. Table 16: "`0b1001` — 2 channels: left, right; **stored as side-right** stereo"; §4.2's decode wording ("the left subblock is restored by adding the samples in the side subframe to the … right subframe"); libFLAC `undo_channel_coding` RIGHT_SIDE arm: `output[0][i] += output[1][i]`. | The rarity excuse lived in FLAC.md's own "encoder choices" bullet ("may never appear"), which is why no witness had ever existed — and it was false: the near_smooth construction (R smooth triangle, L = R + tiny quantized noise) emits **0b1001 on 63/63 frames**, and subframe 0's warm-up (read at bps+1) equals L−R from `flac -d` on 12/12 fingerprinted frames. `drafts/flac3f_width_probe.py` |

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

## Completed: step 3a leaves — wasted-bits reader, rice_unmap, and the warm-up correction (2026-09-10)

Step 3's dependency order (documented as substeps 3a–3f in the phased plan
above) starts with the three pieces nothing else depends on. What landed, and
the two scaffold claims that died on the way:

**1. `BitReader::read_wasted_bits` (§9.2.2).** The flag + unary field the
subframe body needs, closing the last gap in `bits` for the decode path
(CRCs remain, Phase 2). Bit-level and interpretation-free like the rest of the
module: the §9.2.2 rule "resulting bits per sample MUST be larger than zero"
needs frame bps, so it gates at `decode_subframe` (3e), not here. Cursor is
**atomic** (same contract as `read_utf8_coded`): an unterminated run restores
the cursor to the flag bit and returns `EndOfStream`; the slice itself bounds
the unary, so no length cap exists to get wrong. 5 unit tests: flag-clear,
hand-packed runs (k=1/3/5/8 — k=5 crosses a byte, k=8 runs 41 bits), an
over-long k=40 run, mid-slice failure restoration, and an exhaustive
alignment×outcome sweep against a naive bit-loop oracle at every start bit of
a 9-byte stream. **Encoder witness, one layer up:** `tests/subframe_header_layout.rs`
now also calls the *library* reader from `SubframeType::parse`'s exit cursor on
every golden vector and requires it to agree with both the harness's independent
oracle read and the harness cursor position — so the library reader is witnessed
by libFLAC bytes, not only by a same-file oracle. (The pre-existing harness
`read_wasted` stays: it is the independent witness, and agreement is the point.)

**2. `residual::rice_unmap` (§9.2.7.2) — and a sign-flip correction.** The
scaffold's doc claimed the residual sign map was "odd → (n+1)/2, even → −n/2"
with `negative = n & 1`. Reading §9.2.7.2 before implementing found the
opposite convention, and the RFC's own worked example settles it: a folded 38
(unary 4 ≪ 3 | binary 6, Rice parameter 3) unfolds to **+19**; the scaffold
mapping returns −19. The encoder folds `x ≥ 0 → 2x`, `x < 0 → −2x − 1`, so the
decoder's inverse is **even → `n >> 1`, odd → `!(n >> 1)`** — the same
zigzag's other side, no multiply, no divide. Had the scaffold's version
shipped, every decoded residual in every FIXED and LPC frame would have been
sign-flipped — and it would have *survived* a header-level test suite, because
nothing before step 3 touches residual values. 4 tests: the RFC worked example
pinned as an assertion (38 → 19), dense roundtrip of the spec-text fold around
zero, a differential sweep against a negate-and-subtract oracle (different
mechanism) over [0, 64Ki) and a 64Ki window at the u32::MAX end, and the
extremes — including that `0xFFFF_FFFF` maps to `i32::MIN`, the value §9.2.7.3
forbids in a stream: this pure map stays total and returns it; *rejecting* it
is `decode_rice_partition`'s job in 3b, pinned so the seam is explicit. Kept
crate-private until 3b consumes it (the crate-wide `allow(dead_code)` covers
the in-between).

**3. `PredictorState` — the warm-up myth, measured dead.** The scaffold said
warm-up samples "are the previous frame's tail samples and are the only
cross-frame state the decoder must retain", and `Decoder::seek_frame` documented
a warm-up-reconstruction hazard that follows from it. §9.2.5 says the
contradictory thing plainly — "each subframe in FLAC is coded completely
independently", warm-up "stored unencoded" *in the subframe* — and frame 0 was
the free counterexample: it has warm-up samples and no previous frame.
`scripts/measure_warmup_semantics.py` measures it over real encodes (determin-
istic PCM, `-l 0` and `-l 4`, reference samples from `flac -d`, subframe walk
by the independent harness): **64/64 frames carry their OWN first decoded
samples in their own headers; 0/64 match the previous frame's tail** (both
hypotheses checked with §9.2.2's `<< wasted` padding applied). What changed in
the code before any of it was implemented: `PredictorState` is documented as
per-subframe scratch (reused, never history), `update(decoded)` — a method
whose *purpose* was the myth — is replaced by `fill(reader, order,
subframe_bits, wasted)`, the read this module actually needs (3e wires it in),
and `seek_frame`'s hazard note becomes the reverse: seek is **safe by
construction**, which is exactly the property the GAFP manifest's O(1) seek
design has been assuming all along. The myth survived because it read like a
reasonable design note; had it been implemented as documented, the decoder
would have needed sequential-only decode and would have corrupted samples
after every seek — caught at design time by the project's own rule (any claim
about format bytes carries its witness).

**Deliberate scope cuts.** `read_wasted_bits` lands without the bps check
(layer separation); `rice_unmap` lands without §9.2.7.3 enforcement (stream
layer's job); `PredictorState::fill` lands as a corrected *signature* with
`todo!()` (its read is 3e's composition; the type and contract were the
load-bearing parts and they were wrong, so they could not wait for 3e). The
warm-up finding is recorded in "Prior assumptions that did not survive
measurement" (two rows: the myth, the sign flip) with their witnesses.

**Gates:** `make flac-test` green — thumbv4t compile gate + 54 unit (was 45:
+5 wasted-bits, +4 rice_unmap) + 11 frame-header + 5 subframe integration
tests (the subframe suite now also witnesses the library wasted reader against
the oracle on every vector); `make native-flac-rom` unchanged (build sanity);
`make check` clean.

## Completed: step 3b — `decode_rice_partition` (2026-09-11)

Phase 1 step 3b landed: the Rice partition body (unary quotient + `order`
remainder bits + `rice_unmap`) and the §9.2.7.1 escape-record partition (raw
signed two's-complement samples, incl. the legal `raw_bits == 0` =
all-zeros-stored-with-no-bits case). The function consumes the partition
**body** only — the parameter field and escape-code split stay with the
caller (3c), which keeps the inner loop testable standalone.

**Two design facts the implementation had to get right, both pinned by
unit tests:**

- **`order == 0` skips the remainder read entirely.** `read_bits(0)` is
  `InvalidField` by design (a caller bug, not a stream condition), so the
  unary-only case cannot be expressed as "read 0 bits" — it must branch.
  Conflating the two would desynchronise every subsequent codeword.
- **§9.2.7.3 rejection lives here, not in `rice_unmap`.** `folded ==
  0xFFFF_FFFF` (the unique `i32::MIN` pre-image) → `InvalidField`, and a
  quotient that cannot shift into `u32` → `InvalidField`. The seam 3a pinned
  (map stays total; the stream layer rejects) is now enforced.

**Found by test, not by review (the `checked_shl` trap).** The first draft
used `quotient.checked_shl(order)` as the overflow guard. `checked_shl` only
rejects a shift *amount* ≥ 32 — it **wraps bits shifted out of the value**.
Quotient 4 at order 30 silently yielded `folded = 0` (4 ≪ 30 mod 2³²), i.e.
a corrupt stream decoded as a valid zero residual. The test that was supposed
to prove rejection failed with `Ok(())`, and that disagreement was the bug.
Fix: guard on the quotient (`quotient > u32::MAX >> order`, with `order >= 32`
short-circuited first so the shift amount is always < 32) before shifting.

**Found by test, one level up (the hand-vector lesson, again).** The
multi-sample hand-packed vector mis-derived `fold(−2)` as 5; the spec fold
(`x < 0 → −2x − 1`) gives 3 — 5 is `fold(−3)`. The decoder faithfully returned
−3 from the bits actually packed: the *test* was wrong, the implementation
right. Same lesson as steps 1/3a, newly expensive: hand-derived bit expectations
record the author's belief. The differential sweeps (pack straight from the
spec text → decode → compare, at every Rice parameter 1..14 × every start-bit
alignment, and every escape width 1..31) are what actually witness the loop;
the hand vectors only demonstrate the RFC §9.2.7.2 worked example (folded 38 →
+19) and the order-0 / escape shapes.

**One scaffold doc claim corrected:** `PartitionHeader::raw_bits` was
documented as a "4-bit field". §9.2.7.1: "Directly following the escape code
are **5 bits** containing the number of bits with which each residual sample
is stored." Row added to the prior-assumptions table. (Latent, not live —
nothing read the field before this step — but a packer written against the
4-bit note would emit an escape partition libFLAC can't read.)

**Deliberate scope cuts (Sam's 2026-09-11 direction).** No frame_vectors.py
per-partition oracle and no on-host micro-benchmark in this PR: correctness
documented via independent packer + RFC worked example + edge-case unit tests;
perf measurement is a dedicated separate effort once more of the decoder is
real. The oracle extension returns if real-world partitions prove materially
different from synthetic ones.

**Gates:** `make flac-test` green — thumbv4t compile gate + 65 unit (was 54:
+11 for 3b) + 11 frame-header + 5 subframe integration tests; the
thumbv4t `-Zbuild-std=core,alloc` check compiles the new code for the real
target; host tests unchanged otherwise.

## Completed: step 3c — `decode_residual` (2026-09-11)

Phase 1 step 3c landed: the complete coded-residual read (§9.2.7) — coding
method `u(2)`, partition order `u(4)`, then per partition: parameter
`u(4)`/`u(5)`, escape (`0b1111`/`0b11111`) → raw width `u(5)` →
`decode_rice_partition`. Sample counts follow §9.2.7 exactly: partition 0
carries `(blocksize >> order) − predictor_order`, the rest
`blocksize >> order`; total `blocksize − order` is the `out` contract. The
three stream MUSTs (divisible blocksize, `blocksize >> order > order`, no
reserved method codes) are enforced as `InvalidField`, all division-free
(`& (partitions − 1)` mask, not a divide — ARMv4T rule).

**The phantom sample-size field, dead before it could be coded.** The 3c plan
line (and the scaffold's module doc) claimed a "sample-size header with its
`0b111` unknown escape, derived by scanning partitions." **No such field
exists in a coded residual.** §9.2.7 in full is: method, partition order,
per-partition parameter (+ escape width) — the residual sample width is
implicit in the Rice codewords; nothing is declared, derived, or scanned.
Witness, three ways: §9.2.7 read end-to-end (grep for any residual-size
wording: nothing), Appendix D Table 38 (a real residual walked bit-exactly:
2 + 4 + parameter, then straight into quotients), and libFLAC's
`read_residual_partitioned_rice_` (reads nothing else). Row added to the
prior-assumptions table. Had it been implemented as planned, the decoder
would have consumed 8 phantom bits per residual — desyncing every frame
after the header. Caught at design time, exactly like the warm-up myth: the
plan text was treated as a hypothesis until the spec confirmed it.

**Witness: committed data, zero new encoding (Sam's 2026-09-11 direction
held).** RFC 9639 Appendix D.2.7 is a fully worked decode of a real libFLAC
1.3.3 encode — Example File 2's first frame, published as hex, walked
bit-by-bit by the spec itself, with the 15 residual values in Table 39 and
the stream's MD5 verified in D.2.9. The test embeds those 37 bytes and
decodes through the full chain: `FrameHeader::parse` → `SubframeType::parse`
→ `read_wasted_bits` → `read_signed(17)` warm-up (= 4302, Table 40) →
`decode_residual` → **all 15 values equal Table 39**, cursor lands exactly
at `0xAC+0` where the RFC's subframe 1 begins (witnessed by parsing subframe
1's real type byte after it). The Python cross-check (`drafts/`, throwaway)
reproduced Table 39 from the RFC hexdump before any Rust was written — the
same oracle-first rule the bits steps used. Unit tests cover what the single
witness cannot: multi-partition counts (the first-partition penalty), mixed
Rice+escape partitions, width-0 escape in-place, 5-bit method reaching
parameter 20, and the three MUSTs (with the RFC's own 4096/order-4 → max
partition-order-9 boundary, its legal side witnessed by an EOF-beats-
rejection pass-through probe).

**Found by test (the hand-packed-vector lesson, fourth firing).** A header
rejection test hand-packed `0b0000_0001` meaning "partition order 1" — but
MSB-first that byte is method `00` + partition order `0b0000` = **0**, which
is legal for the test's blocksize. The test would have passed on the wrong
rule (asserting `InvalidField` against an EOF from a different path — a
coincidence away from a false green). Fixed structurally, not by correcting
the byte: every header-rejection probe now routes through the `Packer`
oracle, so field names in the test are the fields the bits encode. The
recurring rule earns its keep: hand-packed bytes record the author's belief;
only a packer/encoder witnesses a field layout.

**Deliberate scope cuts.** No `frame_vectors.py` whole-residual oracle
extension: the Appendix D witness is real encoder output and the header
rules are packer-witnessed; per-frame residual ground truth over the
committed streams would only pay off once 3e/3f compose the full subframe
and a PCM diff (step 3f's end-to-end) subsumes it. No profiling (3b
rule holds).

**Gates:** `make flac-test` green — thumbv4t compile gate + 74 unit (was 65:
+9 for 3c) + 11 frame-header + 5 subframe integration; `make check` clean.

## Completed: step 3d — integrators + `PredictorState::fill` (2026-09-12)

Phase 1 step 3d landed: the three pure array transforms —
`subframe::integrate_fixed`, `subframe::integrate_lpc`, and
`PredictorState::fill` — with no bitstream dependency beyond `fill`'s
`read_signed` loop. Both integrators run **in place** (residual slice in,
decoded samples out): correct because the predictor recurrence reads *decoded*
past samples, and 3e's buffer contract stays "one caller slice per subframe".

**The FIXED integrator is the forward-difference cascade, seeded with the
warm-up's differences.** Table 20's FIXED-k predictor is exactly "the k-th
forward difference of the output equals the residual", so orders 1–4 run k
running-sum accumulators + one output accumulator, no multiplies (module-doc
rule). The non-obvious part was the **seed**: the accumulators must start at
the warm-up's forward differences (`d1 = w0−w1`, `d2 = w0−2w1+w2`,
`d3 = w0−3w1+3w2−w3` — differences-of-differences, w most-recent-first), not
at the warm-up values themselves. A wrong seed agrees with the truth for at
most the first sample or two, so the vectors below pin it hard.

**Witness strategy (3d plan's rule: synthetic residuals + an independent
Python reference; no golden-vector work needed).** `drafts/flac3d_oracle.py`
(throwaway, per step 3c's drafts/ precedent) implements §9.2.5/§9.2.6
reconstruction with arbitrary-precision integers and **generates** the vectors
rather than asserting sums: a deterministic smooth signal is built, the
residual is derived by the encoder's inverse formula
(`residual = signal − predicted`), and every vector requires the decode to
recover **the signal itself**. A mis-indexed past, wrong shift, or dropped
warm-up breaks signal recovery even if oracle and impl had shared the same
summing bug — generation evades the hand-packed-vector failure mode FLAC.md
records firing four times (the oracle here is the same mechanism-vs-mechanism
differential the bits/residual steps used, one layer up).

- **FIXED orders 0–4:** 5 oracle vector sets (24 samples each; oracle's
  explicit Table 20 dot product vs the impl's cascade — different mechanism,
  different accumulator structure, Python big-int vs i64). Plus the plan's
  **free cross-check escalated**: cascade-vs-dot-product agreement is pinned
  both on the oracle vectors and on pseudo-random wide residuals per order.
- **LPC:** 4 oracle cases — `[64]>>6`, `[96,−32]>>5` (mixed signs → negative
  accumulators reach the shift: the **floor-vs-truncate seam**, pinned further
  by `(-1)>>1 == -1`), `[80,40,−20,−8]>>4`, and order-5 extreme coeffs at
  `shift 0` (order 5 also witnesses LPC orders beyond `MAX_FIXED_ORDER` —
  LPC's cap is 32, not 4). Plus an in-place-vs-separate-history differential:
  the impl aliases decoded samples into the residual slice; the reference uses
  an absolute-position history buffer (warm-up oldest-first + decoded) — a
  different indexing scheme, agreeing sample-for-sample.
- **`fill`:** 4 oracle vector cases (order × subframe-bps × wasted × cursor
  alignment: 13-bit values at bit 0, 11-bit at bit 5 with `wasted 3`, 16-bit
  at bit 3, 8-bit at bit 7 — every field straddles bytes at worst case), each
  pinning state contents (`<< wasted` padding, most-recent-first storage) and
  the cursor landing exactly at `start + order × subframe_bits`.
- **fill × integrator seam:** the 4 fill states each feed FIXED order 1 over
  a shared residual, with oracle-computed expectations — `fill`'s padding and
  the seed layout are load-bearing in the composition, not just per-unit.

**Contracts pinned (rejections write nothing, cursor untouched):**
`integrate_fixed`: `order > MAX_FIXED_ORDER` → `UnsupportedPredictorOrder`,
`warm_up.len < order` → `InvalidField` (caller bug — `fill` guarantees
`len == order`). `integrate_lpc`: **§9.2.6's "shift MUST NOT be negative"** →
`InvalidField` (the first place the rule becomes enforceable — the s(5)
coefficient field is 3e's parse, but the integrator refuses to trust it),
shift ≥ 64 rejected for the same not-relying-on-the-caller reason, `len >
MAX_LPC_ORDER` → `UnsupportedPredictorOrder`, short warm-up → `InvalidField`.
`fill`: `order > MAX_LPC_ORDER` → `UnsupportedPredictorOrder` and
`wasted ≥ 32` → `InvalidField` *before any read* (the format keeps
`subframe_bits + wasted ≤ 24`, so a wider pad is a corrupt/mis-placed read;
rejecting keeps the pad shift from being a silent `shl` wrap — the
`checked_shl` trap from 3b generalized). State is **commit-only-on-success**
(reads land in a local buffer): the EOF test shows a real partial read (one
13-bit warm-up sample consumed of three) leaving the previous state fully
intact and the cursor exactly at the failing read's start — per-read atomicity
plus module-wide state atomicity, matching the residual readers' contract.
Padding computes in `i64` then truncates (same convention as the integrators'
accumulators; the perf spike revisits intermediates, not this seam).

**No prior-assumption rows this step** — nothing the docs claimed about bytes
or semantics turned out false; the step's risk (cascade seeds, in-place
aliasing, shift flooring) was all novel implementation surface, and the
oracle-first rule meant the vectors, not a correction pass, carried the
burden.

**Deliberate scope cuts (narrow, per Sam's 2026-09-12 direction):**
`decode_subframe` stays `todo!()` — 3e's composition (wasted read → bps gate
→ `fill` → body dispatch → integrate) is the next PR; no frame_vectors.py
extension, no profiling (3b/3c rules hold; the gate's cycle counts are
step 4's dedicated effort). The `i32` truncation convention in the
accumulators is flagged, not enforced — the same "prove before trusting" note
the module already carries.

**Gates:** `make flac-test` green — thumbv4t compile gate + **90 unit** (was
74: +16 for 3d) + 11 frame-header + 5 subframe integration; `make
native-flac-rom` builds/links/fixes unchanged; `make check` clean (fmt
included).

## Completed: step 3e — `decode_subframe`: composition and the wasted-scale rule (2026-09-15)

Phase 1 step 3e landed: `subframe::decode_subframe` composes the whole
subframe — type parse → `read_wasted_bits` → §9.2.2 bps gate (`0 < frame bps
− wasted ≤ 32`, the only place the MUST can be judged) → `fill` → body
dispatch (CONSTANT one stripped value / VERBATIM `read_signed` loop /
residual) → LPC body fields (u(4) precision−1 with `0b1111` forbidden, s(5)
shift rejecting negatives *before* the coefficient reads, coefficients
most-recent-past first) → integrate → `pad_block` → return the type. Nothing
remains `todo!()` in `subframe.rs`; frame wiring (footer, side subframe
width — written `bps−1` here, measured **`bps+1`** by 3f part 1, see
"Prior assumptions", decorrelation) is 3f.

**The step's finding: §9.2.2's wasted multiply runs once, at block exit,
after prediction — not at the warm-up read.** libFLAC 1.5.0's
`read_subframe_` shifts the entire output array *after* its type dispatch
(stream_decoder.c:3028), so prediction runs entirely on the wasted-stripped
codec scale: `fill` is called with `wasted = 0`, the residual and integrators
never see padded values, and `pad_block` applies the `<< wasted` once to the
finished block. Padding before prediction is **not equivalent**: the LPC dot
product's arithmetic `>> shift` floors, and flooring does not commute with
left-shifting. Measured on the crossing vector (LPC-3 + wasted 5): 252/256
samples wrong pad-first, 0/256 pad-after. The mixed-scale variant was
witnessed wrong twice before the rule landed — `fixed1-wasted5-256` decoded
the ±20000 square wave as 20000/18750 — and the FIXED cascade provably agrees
either order (pure addition), so no FIXED vector discriminates. Both
wasted-crossing vectors are committed: `fixed1-wasted5-256` pins presence,
`lpc-wasted-256` (quantized tonal: libFLAC still picks LPC-3, wasted 5 —
quantized smooth material collapses to FIXED, unquantized tonal carries no
wasted bits, so only quantized tonal content crosses the features) pins
*order*.

**The warm-up doubles as the block's first `order` samples** (3a's finding,
now composed): FIXED/LPC write the prefix directly from `state` (reversed to
stream order) and decode `out[order..]` through residual + integrator.

**Witness strategy — real encoder bytes, two independent ground-truth
layers.** New committed table `tests/subframe_body_vectors.txt`, regenerated
by `scripts/gen_frame_vectors.sh` (`frame_vectors.py emit_bodies`): one mono
256-sample stream per subframe type (`-b 256` → exactly one frame per
stream, so the stream's kind census IS the frame's type), every expected kind
fail-closed at emit time. Field values come from a naive bit-loop `Bits`
walk — a second mechanism from the crate's accumulator reader — and
`pcm_samples` from the reference decoder (`flac -d`), so reconstruction is
witnessed by a *second decoder*, not the harness re-deriving its own sums.
Generation-time invariants (all fail-closed): footer-gap to the next frame
per the measured footer rule, warm-up == own first PCM `>> wasted`,
verbatim/constant bodies == reference PCM. Four witnesses per vector at test
time: dispatch outcome, warm-up state (coded scale, most-recent-first), exit
cursor at exactly `exit_bits` — which pins by arithmetic every field width
*below* the header (wasted run, warm-up, LPC fields, residual partitions) —
and bit-exact `flac -d` PCM. The **VERBATIM golden gap closed**: header table
vector `verbatim-first` from the incompressible Knuth-hash pair (measured:
verbatim on every frame at `-l 0`, zero verbatim at `-l 12` — `-l 0` is the
switch, census refuses to write if that ever changes).

**Rejections libFLAC can never witness** (precision `0b1111`, negative
shift) mutate ONE field of the real `lpc-256` frame at runtime, mutation
bit-positions derived from the vector's own witnessed fields (header exit +
observed widths, never constants), with a control decode of the unmutated
bytes. The first draft's probe re-parsed the *pristine* frame and every
mutation "decoded Ok" — the assert firing `Ok` was the bug; decoding the
mutated slice is now pinned by a comment. Each rejection asserts zero bits
consumed behind the rejected field and no residual samples written.

**Contracts pinned (rejections read and write nothing):** caller contracts
(`out.len() == blocksize`, `order > blocksize`) gate before the first bit;
§9.2.2's bps gate and the §9.2.6 field rejections fire before the bits
behind them; `EndOfStream` leaves the cursor at the failing read (per-read
atomicity holds through the whole composition). The bps gate also enforces
width ≤ 32 locally rather than trusting `read_signed`'s contract — same
refusal-to-trust-the-caller as the integrators' shift guard.

**Prior assumptions corrected:** 3d's `fill(wasted)` signature implied the
warm-up's `<< wasted` pad lands at read time; composition disproved that at
the LPC × wasted crossing — `fill` is now called with `wasted = 0` and the
pad moved to block exit. (The warm-up-scale assertion itself was wrong in
both directions during the step — padded-state vs stripped impl, then the
reverse — FLAC.md's "test bug, not impl bug" lesson firing twice; the
coded-scale pin now documents which side was wrong each time.) FIXED ×
wasted passing either padding order was itself a surprise: a fix passing
every committed vector can still be wrong in an unexercised feature
crossing — the crossing enumeration is what forced `lpc-wasted-256`.

**Deliberate scope cuts (narrow):** frame footer (byte-align + CRC-16
consume), subframe loop, side-subframe width (written `bps−1` here; the
seam direction was later measured **`bps+1`** — 3f part 1, "Prior
assumptions"), and decorrelation all stay 3f — the `sample_bits` parameter
already documents the caller's seam; no profiling (3b/3c/3d rules hold;
cycle counts are step 4's dedicated effort). The `i32` truncation convention carries into `pad_block` (i64 then
truncate) — flagged, not enforced.

**Gates:** `make flac-test` green — thumbv4t compile gate + **84 unit**
(unchanged: 3e's witnesses are encoder-byte integration suites, not unit
tests) + 11 frame-header + **2 subframe-body (new suite)** + 5
subframe-header; `make native-flac-rom` builds/links/fixes unchanged; `make
check` clean (fmt included).

---

## Completed: step 3f part 1 — `stereo::decorrelate`, and the side width and orientation measurements (2026-09-16)

Phase 1 step 3f split in two. Part 1 (this) lands
`stereo::decorrelate` — the three recombination transforms — and, before
any part-2 code could depend on them, measures the step's two remaining
claims about encoder bytes: the side subframe's storage width, and what
subframe 0 holds under `0b1001`. **Both measurements killed a claim**: the
plan line said side is coded at **bps − 1** (measured **bps + 1**), and the
crate contradicted itself about side/right orientation (two docs, opposite
answers; resolved toward side-first). Part 2 — `decode_frame`'s subframe
loop, footer, cursor-chaining, end-to-end PCM diff — is the open work;
`decorrelate` is not wired into a frame yet.

**What landed.** `decorrelate(config, blocksize, left, right)` recombines
the decoded subframe pair in place: left/side `(p, p − s)`; side/right
`(side + r, r)` — the left slot holds the **side** (Table 16); mid/side
`m_ext = (mid << 1) | (side & 1)`, then the exact halvings
`(m_ext ± side) >> 1`. The halvings are exact because `m_ext ± side` are
even by construction (`m_ext = L + R` recovered, `side = L − R`; same
parity is the identity `L+R − (L−R) = 2R`), so the `& 1` recovery *is* the
rounding term — no separate `+1` exists. The scaffold's `mid_side_recover`
folded into `decorrelate`: the transform is one pass, and a separate LSB
pass would touch the block twice for nothing. Length contracts gate before
any write (rejections write nothing); exactly `blocksize` samples
processed — buffers are reused across frames and the final frame is
legitimately short, so the tail beyond `blocksize` must survive untouched.
Arithmetic in `i64`, truncate at store (crate convention): `mid << 1` at
the top of the i32 range cannot wrap silently. Module docs gained the
orientation table with full witness citations.

**Witness strategy (the transform math).** Generation, not assertion:
`drafts/flac3f_stereo_oracle.py` (throwaway, `drafts/` precedent) starts
from true `(L, R)`, derives the **stored** pair with the encoder-side
formulas, and the tests require recovery of `(L, R)` — a wrong
decoder-side formula or LSB rule fails recovery, and cannot pass by
agreeing with a shared hand-transcription. 55 pairs: 16-bit-domain
extremes — including the side-saturating `(32767, −32768)`, whose stored
side `65535` is a value only a bps+1 slot could ever hold — plus an LCG
sweep. The oracle's output was inserted into `stereo.rs` mechanically and
a throwaway comparison re-verified every committed constant against the
oracle's live output before the gates ran. **That check fired
immediately**: the hand-typed insertion had diverged from the oracle on
the LCG block *and* had used truncating rather than flooring `>>1` for
negative-odd mid values (`(-1) >> 1 = -1`, not 0 — the floor-vs-truncate
seam `integrate_lpc` pinned in 3d, re-earning its keep one module over).
The lesson generalizes one level: even "copy the oracle's output" is a
transcription — verify mechanically. Plus a live brute force over
`[−16, 16]²` in all three modes, stored pairs recomputed in-test by the
encoder-side formulas (an independent spec reading from the impl's).

**The two measurements (the claims that died). Rows added to "Prior
assumptions" for both.**

1. **Side width = bps + 1, not bps − 1.** Anti-phase encode (R = −L, |L|
   high enough that |L−R| crosses 32767): the side subframe's warm-up —
   the subframe's own first decoded samples (3a's finding) — read at
   bps+1 equals `flac -d`'s L−R on **24/24** frames, 12 of them values a
   16-bit field **cannot hold**, and the same read at bps disagrees on
   **24/24**. RFC §4.2 ("the side channel needs one extra bit of bit
depth, as the subtraction can produce sample values twice as large") and
   libFLAC 1.5.0 `read_subframe_` (`bps++` on the side slot: channel 0 for
   side-right, channel 1 for left/mid-side) agree with the bytes. The
   plan's −1 would have desynchronized every decorrelated frame — a
   reader at −1 lands mid-codeword everywhere. Caught before any part-2
   code was written; `decode_subframe`'s seam doc corrected in the same PR.
2. **`0b1001` is side-first.** Scaffold `stereo.rs` claimed subframe 0
   holds R; `format::ChannelConfig::RightSide` claimed subframe 0 holds
   the side. Neither trusted. RFC Table 16 ("stored as side-right"), §4.2
   decode wording ("the left subblock is restored by **adding** the
   samples in the side subframe to the … right subframe"), and libFLAC
   `undo_channel_coding` (`output[0][i] += output[1][i]`) all say side.
   The encoder-byte witness needed libFLAC to *emit* `0b1001` — FLAC.md
   had recorded that it may never appear, which is why no orientation
   witness had ever existed. It does appear: a near-identical-channels
   construction (R a smooth triangle; L = R + tiny quantized noise → side
   cheap, R strictly more compressible) chose `0b1001` on **63/63**
   frames, and subframe 0's warm-up read at bps+1 matched L−R on **12/12**
   fingerprinted frames. The obvious bait — anti-phase — picks mid/side
   instead; the encoder's per-frame cost model wins such arguments, not
   intuition. FLAC.md's "encoder choices" bullet corrected in this PR.

**Found by re-derivation (a comment, not a test).** The mid/side extreme
test's hand-written comment claimed the no-LSB-recovery counterfactual
"lands on (32767, −32767)". Recomputing: it lands on **(32766, −32769)** —
*both* outputs off by one. A comment about a specific number the code
never computes is a claim nothing can disagree with — the exact shape that
survives to mislead later. Fixed before commit.

**Deliberate scope cuts (narrow).** `decode_frame` stays `todo!()`: the
subframe loop (with the now-measured bps+1 side seam), footer consume,
cursor-chaining and the bit-exact `flac -d` diff are part 2. The oracle
and width probe stay in `drafts/` (3c/3d precedent — committed vectors are
the oracle's *emitted output*, which is the witness; no new harness
surface this PR). `scripts/frame_vectors.py` untouched: if part 2 wants a
`0b1001` stream in the committed set, the near_smooth construction can be
graduated through the census-fail-closed generator proper. No profiling
(3b/3c/3d rules hold; cycle counts are step 4's dedicated effort).

**Gates:** `make flac-test` green — thumbv4t compile gate + **91 unit**
(was 84: +7 for `decorrelate`) + 11 frame-header + 2 subframe-body + 5
subframe-header; `make native-flac-rom` builds/links/fixes unchanged;`make
check` clean (fmt included — note `flac-lite` is a standalone workspace,
so only the crate-level `cargo fmt` reaches it; the Makefile target runs
per-crate for exactly this reason).

## Completed: step 3f part 2 — `decode_frame` and the frame-run vectors (2026-09-18)

Part 2 of the split step lands the frame layer: `frame::decode_frame`
composes the subframe loop, the measured bps+1 side seam, `decorrelate`,
and the footer consume into one driver, witnessed on **whole frame runs** —
five libFLAC 1.5.0 runs (one per channel assignment, 256 + a 100-sample
uncommon-blocksize tail each), decoded twice over the same committed
table: once through the *existing* primitives (a layout suite that proves
the table's ground truth with zero new frame code), once through the
production driver; the two agreeing, bit-exact against `flac -d`, is the
witness. With this the 3f plan line is discharged end to end — including
its "pulls a slice of step 8 forward" clause: the bit-exact `flac -d`
PCM diff now exists at the frame layer. Step 3 (and with it the Phase 1
decode path through `decode_frame`) is complete; only the step 4 perf
gate remains in Phase 1.

**What landed.** `decode_frame(reader, header, left, right, state)` —
subframe loop over 1–2 slots (`subframe_count()` from the header's own
channel code; 3–8-channel streams never reach it, rejected as
`ProfileViolation` at header parse), the side slot decoded at `bps + 1`
with the slot derived from the channel code (right/side → slot 0,
left/mid-side → slot 1) exactly as part 1 measured, `decorrelate` after
the loop, then the footer: `byte_align` + big-endian CRC-16 **consumed,
never verified** (Phase 2 step 5). Returns the sample count written.
The scaffold's `defaults: &StreamDefaults` parameter is **gone** (see
prior assumptions). Buffer contract: `len() >= blocksize` per used
channel, exactly `blocksize` written, tail untouched — playback reuses
one fixed buffer per channel and the final frame is legitimately short,
so the tail contract is load-bearing and tested with sentinels. Mono
passes a legal zero-length `right`; the driver slices the secondary
window only for two-subframe modes, and `decorrelate`'s Independent-1
arm covers `left` alone. Short-buffer rejection gates **before the
first bit**: cursor unmoved, nothing written (crate rejection rule);
past the first bit the frame is dead — re-seek from the manifest
offset, the doc comment says so.

**Witness strategy (composition pattern).** Two suites, one committed
table (`tests/frame_run_vectors.txt`, regenerated by
`scripts/gen_frame_vectors.sh` → `frame_vectors.py emit_runs`): each
run's `frame_hex` rows cover every frame **through its footer**, so the
rows concatenate into the contiguous frame region and one `BitReader`
walks frame N → N+1 exactly as a decode loop does. `frame_run_layout.rs`
replays the table through `FrameHeader::parse` + `decode_subframe` +
`decorrelate` only — it passes at this commit's *primitives*, before any
frame driver existed to hide a shared misreading. `frame_run_decode.rs`
runs production `decode_frame` on the same table; agreement is the
witness (a production-only suite could share one misreading of ground
truth with the table). Ground truth is layered twice: `slot_stored_N`
(stored subframe values — side = L−R, mid = (L+R)>>1 — derived from the
reference PCM by encoder-side formulas and re-checked against each
subframe's warm-up/verbatim/constant fields by the harness's independent
Bits walk) and `pcm_left`/`pcm_right` (`flac -d` whole-stream PCM).
The layout suite diffs **both layers** (raw slot output before
`decorrelate`, final PCM after), so orientation and transform math are
re-witnessed on real stereo bytes, not just part 1's synthetic oracle.
Chaining is pinned by cursor position — the per-frame footer gap rule
(exit on a byte boundary → bare 16-bit CRC gap, else pad + 16) is
witnessed frame by frame by the next header parse landing at the table's
byte offset.

**The seam control, generalized (the part-1 rule earned its keep again).**
The first draft of the negative control demanded bps+1 divergence on
*every* decorrelated run — and fired on a legitimate vector:
left/side and side/right here carry **FIXED-0** sides (tiny quantized
noise; no predictor helps), and a FIXED-0 subframe never consults frame
bps (no warm-up reads, width-independent Rice codewords, pad by the
wasted field), so bps+0 decodes bit-identical — divergence is
grammatically impossible there, exactly part 1's inert-feature lesson
replayed one layer up. The generator now derives `side_width_sensitive`
(true/false) from its own walk's kind/order, and the suite **checks the
flag both ways**: divergence demanded where `true` (mid-side's LPC-3
side carries the seam witness), bit-exact width-**in**variance demanded
where `false` (the crate's reader re-proving FIXED-0 inertness end to
end; a mislabeled label would diverge and fail), plus a tripwire that ≥1
observable vector exists so a libFLAC upgrade cannot silently degrade
the witness to the inert branch alone.

**The encodes (measured, libFLAC 1.5.0).** All five runs encode `-l 4
-b 256` at the **default compression level** — measured: `-1` collapses
every construction to mid/side, and the fast heuristic would have made
`0b1000`/`0b1001` unproducible no matter the audio. The side-channel
baits are the quantized-noise constructions (one channel smooth, the
other = smooth + tiny quantized noise; mirrored for left/side vs
side/right), uniform-mode censuses over both frames fail closed — the
table refuses to be written if any stream stops being the labeled mode.
The independent-stereo run uses two *uncorrelated* smooth channels: the
arm that actually compresses to `0b0001` at default level, which no
decorrelated bait can witness. `-l 4` puts LPC on the gate path through
the frame layer (mid-side frame 0's side measures as LPC-3), so
Correction 3's both-arms requirement is exercised by the runs, not just
by 3d's unit tests.

**Prior assumptions (what died this step).**

| Claim | Reality |
|---|---|
| Scaffold `decode_frame` took `&StreamDefaults` ("from stream" codes must resolve at the body) | Stream defaults never reach the frame body: `FrameHeader::parse` already resolved blocksize, bps, and channels into the header. The driver takes only the header — parameter dropped. |
| `byte_align` "asserts a falsehood" (frame module doc, step 2's correction — the *header*'s fixed fields end byte-aligned) | Header-local. The **footer** is genuinely unaligned whenever body bits miss a byte boundary; `byte_align` is the honest call there, and the run table's per-frame gap rule witnesses it. |
| The chaining witness needs the GAFP manifest ("agree with the manifest offset") | No Phase 2 surface needed: footer-through `frame_hex` rows make the table itself the offset source; the cursor lands on the next row's byte, frame after frame. |

**Deliberate scope cuts.** CRC-8/CRC-16 *verification* stays Phase 2
step 5 (both consumed here). 3–8-channel streams stay rejected at
header parse — no multi-channel decode path. No profiling (step 4 owns
cycle counts). The seam negative control and stored-slot diff live only
in the layout suite (only the primitive composition can express them);
the decode suite owns what only production can show: chaining, bit-exact
run PCM, the reused-buffer tail contract, rejection-before-the-first-bit.

**Gates:** `make flac-test` green — thumbv4t compile gate + **91 unit**
unchanged (composition adds zero unit tests — the suites that grew are
integration: **18 → 24**, two new 3-test suites `frame_run_layout` +
`frame_run_decode`; old count re-measured at HEAD in a clean worktree,
not trusted from part 1's entry). `make native-flac-rom` builds, links,
and fixes (crate API changed — signature). `make check` clean (fmt
included; one pre-existing `subframe_body_layout` paren warning from
step 3e left alone — unrelated committed test code).

## Completed: spike PR 1 — scaffold + embedded clips (2026-09-19)

Phase 1 step 4's first PR landed: the perf-gate measurement vehicle exists and
its assets are proven, but **nothing is measured yet** (PR 2 gates every perf
number, per the plan's correctness-before-speed rule). What exists now:

**Crate** `examples/flac_spike/` — standalone workspace (config inherited,
never re-declared — the `-Tgba.ld` rule), `#![no_std]` lib + GBA `[[bin]]`
behind `cfg(target_arch = "arm")` (agb is a target-gated dependency so the
host witness gate compiles the lib without agb's dependency graph; the bin is
`test = false` so `cargo test` never builds a `no_main` entry). Modules:
`assets` (generated manifest), `checksum` (FNV-1a 64, shared host/ROM),
`driver` (the seek-table frame-loop seam every later PR plugs into:
PR 2 hashes via it, PR 3 times `decode_one`'s window, PR 4 alternates buffer
halves between calls — decode logic never changes), `main` (ROM: boots, logs
arm metadata, verifies region hash pins, blue/red screen verdict).

**Assets** — generated by `scripts/gen_spike_assets.sh` → `frame_vectors.py
spike_assets` (the plan line's "spike-assets", landed with the module's
underscore convention): the committed 10 s deterministic source, both arms
`-l 0` / `-l 4` (`-b 2048 -m`, stereo 32 kHz 16-bit, **default compression
level `-1`** — the same flag pair as the existing golden-vector streams).
Per arm: raw frame region `.bin` (embedded), reference PCM `.bin` (host-test
only — the ROM pins it by hash, never carries it), and generated
`src/assets.rs` with the per-frame offset table (region-relative; the GAFP
stand-in), FNV pins, and the measured censuses:
`L0_FIXED`: `fixed0 ×157` both slots · `L4_LPC`: `lpc4 ×156 + lpc3 ×1` both
slots — Correction 3's arms, exactly. Fail-closed layers in the generator:
`check_stream` invariants, a both-slot subframe walk whose per-frame footer
gap must land on the next frame's offset (witnessing the region slicing and
the bps+1 side seam on all 314 frames), and arm censuses that refuse to write
if `-l 0` ever stops being FIXED-only or `-l 4` stops being majority LPC.
Regions are byte-stable across regenerations (sha256 pins in `assets.rs`).

**Witness suite** (`make spike-test`, host — out-of-tree `--manifest-path`
pattern, cargo config leak): 4 checksum unit tests + 6 integration — Rust
FNV-1a reproduces the Python generator's pins on both regions and both PCM
blobs; driver decode of the **exact embedded bytes** is **bit-exact vs
`flac -d`** on both arms (320,000 stereo samples each); the two arms' PCM is
byte-identical (same source ⇒ one ground truth — a free cross-arm witness);
seek-table structural invariants; and a negative control (offset +1 byte must
fail the walk, pristine control decodes) proving the driver consumes the
table rather than chaining by bytes.

**Findings this step (same failure mode, new costume — recall without a
witness):** the first draft's FNV unit-vector "published values" were wrong
from memory; the implementation disagreed and was right. The pins now in the
test were **derived** in two independent runtimes and anchored against the
32-bit definition vectors FNV publishes and everyone agrees on. Also found by
the compiler, not review, in the generated file: Python `json.dumps` escapes
non-ASCII as `\uXXXX` (valid JSON, invalid Rust — `ensure_ascii=False`), and
a const struct-literal needs `};` (a lexer error earlier in the file masked
it; generated files need compile gates too).

**Budget correction landed early:** the spike README's "62,500 cycles/frame"
units error was scheduled for PR 5, but the placeholder README became false
the moment the crate existed (it claimed "no Cargo.toml, nothing can build"),
so it was rewritten now with the derived ~1,048,750 cycles/frame budget and
the working-target caveat (re-derive in PR 3 against measured load).

**Gates:** `make flac-test` unchanged-green (flac-lite untouched), `make
spike-test` green (4 + 6), `make native-spike-rom` builds/links/fixes, `make
check` clean (spike workspace added to format/check). The root ROM crate and
the integration ROM are untouched; the baseline `.gba` never sees this code.

## Completed: spike PR 2 — on-target decode proof (2026-09-20)

Phase 1 step 4's second PR lands the gate every perf number must pass: the
spike ROM now **decodes every frame of both embedded arms on-target** and
proves the result against the generator's PCM pins. The BLUE screen changed
meaning with this PR: PR 1's blue meant asset integrity; PR 2's blue means
**decode proven on the image's own bytes** — the exact image PRs 3–5 will
time. Measured run: mGBA, ROM `flac-spike.gba`
sha256 `e81ca982692b0ede…9fb6c01`, both arms 157/157 frames, PCM FNV-1a-64
`0x54C7B356621B6E15` matched on **both** arms and cross-arm (one source, one
ground truth), verdict line `PR 2 verdict PASSED — screen BLUE`.

**What landed.** `src/main.rs`: after PR 1's asset checkpoints, a
`decode_proof` pass per arm — decode buffers, `driver::decode_clip` (the same
seek-table path the host witness walks), per-frame fold of the decoded block
into a running FNV-1a-64, walk-total cross-check against the manifest, then
final-hash-vs-`fnv_pcm`-pin; then a cross-arm hash-agreement checkpoint.
Screen stays one global verdict (BitReader-PoC convention; per-arm detail is
the serial log). Buffers: one `max_blocksize × i32` per channel (2048 × 4B
each), allocated from agb's EWRAM block-heap global allocator — the serial
log prints the raw addresses (`0x02000610`/`0x02002610`), witnessing EWRAM
placement for PR 4's buffer rotation. Alternating playback halves stay PR 4.

**The shared-fold rule (the step's design decision).** The PCM fold lives in
the **library** (`checksum::fold_i16le_stereo`), not the ROM entry: the host
witness (`make spike-test`, new test
`rom_fold_reproduces_pcm_pins_on_host`) drives the *same function* through
the *same driver* on the *same embedded bytes* and asserts the same
`fnv_pcm` pins. Host and ROM therefore cannot share a transcription mistake
of the interleave rule (L,R per pair, i16-LE) — a second hand-written fold
would be the hand-packed-vector failure with a screen verdict for an oracle.
The fold checks i16 fit *then* folds (never a silent truncation — a sample
outside the 16-bit profile means decode desync, and laundering garbage into
the proof hash is exactly what PR 2 exists to catch), commit-only-on-success:
on `Err` the caller's hash is untouched and the first offending sample is
returned.

**Verbose semantic checkpoints (Sam's direction, this PR).** Serial is the
localization record, in strict order: metadata/census/why (PR 1) → seek-table
invariants → region pins → `checkpoint: asset verification` → per arm
`decode buffers ready` (sizes + EWRAM addresses) → `decode proof: walking N
frames via seek table` → per-frame `off/bs/running-hash` (314 lines; a
mismatch localizes to the first frame whose running hash diverges from the
host's same-sequence dump — the short tail frame is visible, `bs=512`) →
`walk complete` + totals-vs-manifest (`157/157`, `320000`) → `pcm fnv:
expected/actual [MATCH]` → `cross-arm pcm … agree=true` → final verdict line
naming which layers passed. Failure modes named on serial: each
`DriverError` variant prints its evidence (`buffer too small`, `frame N
offset past region (table/blob mismatch)`, `blocksize table=… header=…
(asset corruption)`, `flac-lite rejected: <variant>`); an out-of-range
decoded sample logs per-frame with the offender and keeps walking (first
offender recorded) so the divergence point is visible; totals mismatch prints
`MISMATCH` with both sides.

**Witness strategy.** The plan line's witness, executed: the embedded ROM
bytes themselves — correctness proven on the same image that will be
measured. Layers: region pins (bytes intact, PR 1) + on-target decode of
those bytes + on-target fold == host fold == generator's Python-computed pin
(three implementations meeting: Python generator, host Rust, ROM Rust).
Cross-arm agreement is the free third witness (same source ⇒ one ground
truth). No new committed vectors needed: the committed assets and pins are
this PR's ground truth.

**Prior assumptions: none died this step** — no doc claim about bytes or
semantics turned out false (the step is composition + logging over landed
primitives). Two build facts found by the compiler, not review, in the
no_std library: the `checksum.rs` test module needed `extern crate alloc`
(`Vec` is not ambient in a `#![no_std]` test module), and `{:p}` was replaced
by explicit `as usize` hex — the allocator region address is itself evidence
PR 4 reads, an opaque placeholder isn't.

**Deliberate scope cuts (narrow).** No cycle counting (PR 3), no IRQ-off
windows, no cadence/buffer alternation (PR 4). Decode runs at boot before
the gfx loop. A failed region pin does **not** abort the decode pass — its
logs stay diagnostic while the verdict is already red (decoding drifted
bytes can still localize asset corruption). `podman-spike-rom` not run on
this host (no nested virtualization; FLAC.md build constraint) — native
gate + host witnesses carry the PR.

**Gates:** `make spike-test` green — **8 unit** (was 4: +4 fold tests —
interleave order vs hand-assembled bytes, mono window, empty-window identity,
atomic out-of-range rejection) + **7 integration** (was 6: +1 shared-fold
pin witness); old counts re-measured at HEAD in a clean worktree, not
recalled. `make flac-test` unchanged-green (91 unit + all integration
suites; flac-lite untouched). `make native-spike-rom` builds/links/fixes.
`make check` clean (fmt included). mGBA run: 338 serial lines, 0 FAIL/
MISMATCH, BLUE (mgba-test-runner still hangs as a gfx-loop ROM — expected,
PR 1's note; killed after the verdict line lands).
