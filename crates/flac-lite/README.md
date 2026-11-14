# flac-lite

A minimal FLAC frame decoder: `#![no_std]`, **zero dependencies**, **zero allocation on
the decode path**, `#![deny(unsafe_code)]` with a single named `cfg`-gated exception.
The GBA (`thumbv4t-none-eabi`) is the **founding platform, not the boundary** — the decode
path imports nothing platform-specific, and everything GBA-shaped sits behind an
off-by-default feature or in the sibling overlay crate
([`../flac-lite-gba/`](../flac-lite-gba/)).

It decodes what a FLAC encoder actually emits — CONSTANT / VERBATIM / FIXED / LPC
subframes, partitioned Rice and Rice2 residuals with escape partitions, wasted-stripped
bits, mid/side, left-side and right-side decorrelation, and the i64 escape leg for streams
outside the product profile — and it is witnessed **bit-exact against `flac -d`** on
committed reference vectors, on the host and inside a ROM image.

```rust
use flac_lite::{Decoder, FrameBuffers, Manifest};

let manifest = Manifest::parse(blob)?;                 // the GAFP container (or your own)
let mut decoder = Decoder::new(&manifest)?;
let mut bufs = FrameBuffers::new(&mut left, &mut right); // caller-provided i32 scratch
while let Some(frame) = decoder.decode_next(&mut bufs)? {
    // frame.left / frame.right: the decoded, decorrelated block; frame.header, frame.index
}
```

`Decoder::new` borrows the manifest, so nothing here allocates. `Ok(None)` is end of stream;
an `Err` leaves the cursor un-advanced for a recoverable frame error, which is what lets a
caller skip a corrupt frame with `Decoder::seek_frame` and continue.

`Decoder` is a **shell over `scan::Frames`**, the table-less frame walk: each frame's own
fields say where it ends and the end of the bytes is the end of the stream. There is
deliberately no second per-frame loop in the crate. To choose a Rice kernel by type (what
the GBA product does), call `decode_next_with::<K>`; `driver` is the instrument-facing loop
over a borrowed frame index.

- Architecture, the seam, the design rules: [`../../flac-docs/architecture.md`](../../flac-docs/architecture.md)
- Measured byte facts about the format: [`../../flac-docs/format-notes.md`](../../flac-docs/format-notes.md)
- Numbers, method, gates: [`../../flac-docs/measurements.md`](../../flac-docs/measurements.md)
- Decisions, rulings, closed questions: [`../../flac-docs/history.md`](../../flac-docs/history.md)
- The committed vectors and where each expectation comes from: [`tests/README.md`](tests/README.md)

## Guarantees

1. **No `alloc` on the decode path.** Scratch is caller-provided and borrowed; so is the
   manifest.
2. **No I/O traits.** `&[u8]` plus a cursor, forward only. No `Read`, no `Seek`, no
   registry, no metadata probe.
3. **No division, ever.** ARM7TDMI has no hardware divide: shifts, adds and multiplies only.
4. **Fixed-size predictor state** (`[i32; MAX_LPC_ORDER]`), never a growable buffer.
5. **Frames are independently decodable** — warm-up lives inside each subframe, so a
   manifest offset is a safe seek and `PredictorState` is scratch, not history.
6. **A rejection writes nothing.** Cursor and output are left exactly as they were before the
   failing read; every refusal yields a named `Error` variant, never a panic and never an
   infinite loop (on a ROM a panic is a frozen game).
7. **The core makes no real-time claim.** Timings belong to the product image measured on
   hardware; the core's claim is bit-exactness.

**Arithmetic width.** `i32-integrator` is on by default: the ring sites run in i32 and the
LPC dot product is guarded **per subframe** by `Σ|c_j|·2^(b−1) ≤ i32::MAX`, computed from
values the header just parsed. A subframe whose bound fails takes the i64 leg — an
off-profile stream's escape hatch is the other width, not an error — and `ArithmeticWidth`
reports which answered. `--no-default-features` is the audited i64 reference build, and the
two configurations are the width differential.

**Unsafe.** A default build carries exactly one `unsafe` *surface*: placement attributes on
immutable tables behind `all(target_arch = "arm", feature = "iwram-pin")` — off by default,
so host builds and ARM targets without a `.iwram` section compile with zero unsafe and zero
pin surface. How many such attributes exist depends on the configuration (a `rice-hybrid`
build has four), which is recorded rather than assumed. GBA ROM crates opt in.

## Features, and the gate that covers each

All off by default except `i32-integrator`. Each non-default configuration is witnessed by
its **own** Make target, because the default build does not compile what it covers —
`required-features` on the witness suites makes a green default a statement about what the
default contains.

| Feature | Effect | Gate |
|---|---|---|
| `i32-integrator` (**on**) | i32 arithmetic with the per-subframe guard, i64 escape leg | `flac-test` / `flac-test-wide` |
| `crc-check` | verifies header CRC-8 and footer CRC-16 instead of consuming them | `flac-test-crc` |
| `scan` | the table-less frame walk (`scan::Frames`, `resync`) and the `decoder` facade | `flac-test-scan` |
| `rice-hybrid` | `HybridRiceKernel`, the portable three-tier codeword walk | `flac-test-rice` |
| `iwram-pin` | `.iwram` placement attributes on the hot tables (ARM only) | the ROM builds + the address witness |
| `strict-profile` | holds a manifest to `format::DEFAULT_PROFILE` via `EncodeProfile::validate` | tested directly; **no gate builds the `cfg`'d call site**, by decision — the guarantee lives at pack time |
| `instrument` | exposes `driver` for the measurement ROM | `flac-test` runs the host suite with it on |

`rice-hybrid` stays non-default because the on-target reading said so, not because nobody
got round to it: the portable twin is **+42.6 / +42.7 cycles per decoded sample slower** than
the incumbent, 314/314 frames, with both images producing bit-identical PCM. A test asserts
at source level that the one line naming the default still names `TableCountKernel`, so
either direction of drift is attributable. (`measurements.md`, [`history.md`](../../flac-docs/history.md).)

## Build gates

Run them from the repository root: `make flac-test`, `flac-test-wide`, `flac-test-crc`,
`flac-test-scan`, `flac-test-rice`, `flac-portable-check`, `gafp-check`, `gafp-pack-check`,
or all of them with `make flac-gates`. Each gate has two halves and both are load-bearing:

```sh
# 1. the target compile — the gate a std-based decoder can never pass. Run IN-TREE,
#    where inheriting the root .cargo/config.toml is exactly the point.
rustup run nightly-2026-09-04 cargo check --release --target thumbv4t-none-eabi \
  -Zbuild-std=core,alloc

# 2. the host suite — run from OUTSIDE the repo, with an explicit --manifest-path.
mkdir -p /tmp/gate && cd /tmp/gate && \
  rustup run nightly-2026-09-04 cargo test \
    --manifest-path "$OLDPWD/Cargo.toml" --target "$(rustc -vV | sed -n 's|host: ||p')"
```

### ⚠️ The `.cargo/config.toml` leak

Cargo discovers `.cargo/config.toml` by **walking up parent directories** and merging every
layer. A `[workspace]` boundary does **not** stop it, and the repository root's config sets a
GBA target and `build-std`. Measured on this tree:

| Attempt | Result |
|---|---|
| `cargo test` in this directory | `E0463: can't find crate for 'test'` — the inherited `target` sends the host build to the GBA |
| `cargo test --target <host>` in this directory | `E0152: duplicate lang item 'sized'` — the inherited `build-std` recompiles `core` from source under the host's prebuilt `core` |
| `-Zbuild-std=` (empty) on the command line | **still `E0152`** — re-verified on the pinned `nightly-2026-09-04`. CLI `-Zbuild-std=`, `CARGO_UNSTABLE_BUILD_STD=` and `--config unstable.build-std=[]` have all failed at one nightly or another: the inherited value wins, because non-inheritance is **positional**, not flag precedence |
| an out-of-tree cwd + `--manifest-path` (shape 2 above) | ✅ — and this is what every host gate in the Makefile does |

So do not "fix" a host gate with a flag or a local config override: run it from outside the
tree. (This is also what extracting the crate into its own repository would give us
permanently.) Conversely, sub-crates must not re-declare the root config either — cargo
concatenates `target.<triple>.rustflags` across layers, so a second `-Tgba.ld` double-links
with `region 'ewram' already defined`.

`-Zbuild-std=core,alloc` is not "using std": rustup ships no prebuilt `core`/`alloc` for this
Tier-3 target, so they are compiled from `rust-src`. That is why nightly + `rust-src` are
mandatory and why `rustup target add thumbv4t-none-eabi` is a no-op to be avoided. **`std`
must never appear in a build-std list in this project.**

Zero dependencies is not cosmetic either: any dependency that references `std` breaks gate 1.
Adding one requires re-running it. `make flac-portable-check` records the portability claim —
the crate compiles for a second bare-metal ARM target — as a **build fact with no performance
claim attached**; nothing in this tree measures a non-GBA target.

## The `GAFP` container (product-side; a manifest is *one* frame source, not the decoder's contract)

`GAFP` = **G**BA **F**LAC **P**ack. One blob per track, `include_bytes!`-able as a
`&'static [u8]`, written offline by `scripts/gafp_pack.py` (`make master-pack INPUT=…` is the
operator-facing entry). All integers **little-endian** — the manifest is our header, not
FLAC's.

```
offset  size   field
------  -----  -------------------------------------------------------------
0x00    4      magic  "GAFP"
0x04    1      version (0x01)
0x05    1      channels (1 | 2)
0x06    1      bits per sample (16; 8 accepted for experiments)
0x07    1      max FIXED predictor order used (0..4)
0x08    2      block size, samples per subframe (1024 | 2048)
0x0A    4      sample rate, Hz (16384 — the device rate)
0x0E    8      total samples in track (u64)
0x16    2      frame count N
0x18    4      byte offset of frame data (header + table end)
0x1C    4*N    frame offset table: u32 offsets, relative to blob start, ascending
        —      raw FLAC frames, concatenated, in order
```

Deliberately **absent** versus a standard `.flac`: no `fLaC` magic, no metadata-block
framing, no VORBIS/PADDING/SEEKTABLE, and STREAMINFO reduced to the fields a decoder needs.
The offset table replaces `Seek`: `seek_frame(i)` is `offsets[i]`. Frame data is verbatim
FLAC frames as the encoder emitted them — the decoder parses FLAC, not a custom codec — and
frame `i` is `[offsets[i] .. offsets[i+1]]`, so it learns each frame's exact extent instead
of sniffing for the next sync code.

Two header fields are **records, not guards**, and knowing which is which matters:

- `bits per sample` is validated by the *parser* as `8 | 16` while the *encode profile*
  allows `[16]`. Different checks, on purpose: the container states what it is, the profile
  states what may be packed. The packer refuses an 8-bit encode, and the half of that worth
  knowing is that a `-l 0` 8-bit stream lands on **VERBATIM** subframes — a predictor kind
  this target has never measured. A new depth is not a header change; it is an unmeasured cost.
- `max fixed predictor order used` is the maximum over the FIXED subframes the encode
  actually emitted, derived from the packer's frame walk — never from the `-l` flag and never
  from STREAMINFO. An all-LPC stream reads `0` there, correctly, and `EncodeProfile::validate`
  accepts it: nothing on this side of the line can tell the measured asset from the 4× more
  expensive one. The real guard is the packer's subframe census.

**The rate field is a validity check, and it is asked first.** `EncodeProfile` names exactly
one admissible rate (`PRODUCT_SAMPLE_RATE_HZ`, 16,384 Hz) and a stream at any other rate has
no frame period for any other clause's budget to be stated against. Why a header check and not
a runtime one, measured: respell a product blob's rate field from 16,384 to 16,000 Hz — a
completely legal FLAC table rate — and the manifest still parses, the decoder still runs, and
the first frame decodes to **identical samples**. Played against a 16,384 Hz timer it runs
1.024× fast, **41.06 cents sharp**, and no digest in this project can tell the two blobs
apart. That is [`history.md`](../../flac-docs/history.md)'s Q14 ruling, and it is the reason
the sample rate is not a lookup dimension anywhere in the toolchain: decode cost is keyed
structurally (blocksize, predictor kind and order) in `gafp_pack.py`'s measured table, and the
rate is checked once, by every path that validates a manifest — `Manifest::profile`,
`Decoder::new` under `strict-profile`, and the product ROM against its own embedded blob.

### Encode profile (what the packer guarantees)

- 16-bit PCM at **16,384 Hz** — one value, named once. Mono or stereo.
- Block size fixed per track: 1024 or 2048 samples. Fixed blocksize, no variable frames.
- **Max predictor order ≤ 4** — and read `-l` correctly: `--max-lpc-order` caps **LPC**, so
  `-l 4` emits real LPC-4 subframes and FIXED-only is `-l 0`. The `strict-profile` switch is
  therefore "order > 4", not "is LPC".
- **And `-l 0` is not FIXED-0.** It disables LPC; libFLAC still chooses FIXED orders 1–4 from
  the signal, and only FIXED-0 has a measured decode row. The product guard is a subframe
  census, not a flag.
- Frame headers inside 6–9 bytes (48–72 bits) is a **packer** observation about what the
  reference command produces, not a decoder limit. `FrameHeader::parse` asserts derived
  bounds instead — `frame::MIN_HEADER_BITS` (48) to `MAX_ACCEPTED_HEADER_BITS` (120) — because
  the earlier fixed width was an encoder observation masquerading as a bound; a committed
  fixture with an 80-bit first header is parsed in a test.
- Partitioned Rice / Rice2 residuals; escaped partitions supported; stereo independent,
  mid/side, left-side, right-side.

Reference encode command, in the shape `gafp_pack.py` drives (it takes `-l`, `-b` and `-m`
from the profile it was given, wraps the call with `-s` and an `flac -t` self-check, refuses a
profile it cannot honour **before** encoding, and refuses an off-profile *result* before
writing any `.gfp` byte):

```sh
flac -1 -f -l 0 -b 2048 -m input.wav     # the LPC-disabled product arm (NOT a FIXED-0 guarantee)
flac -1 -f -l 4 -b 2048 -m input.wav     # the order-ceiling arm; LPC at the target is refused
```

At 65,536 Hz add `--lax` (that rate is outside FLAC's streamable subset, so every frame then
carries rate code `0b0000` = "from stream", which `frame::StreamDefaults` resolves). And for
the record: `--force-utf8-legacy-noop`, which an earlier revision of this file listed, **is
not a libFLAC option** — 1.5.0 exits 1 with `unrecognized option`. Frame numbers are
UTF-8-coded by spec, not by flag.

## Caveats worth stating rather than discovering

- The crate builds with a crate-level `#![allow(dead_code, unused_variables)]`
  (`src/lib.rs`). That is a stated allowance, not a lint-clean build — and it never reaches
  `tests/`, because an integration test is its own crate.
- The committed Rice corpus is blind in three measured ways (no escaped partition, no sample
  that folds to `u32::MAX`, no codeword overflowing the §9.2.7.3 guard), so mutation arms over
  it are green *correctly*. [`measurements.md`](../../flac-docs/measurements.md) has the detail
  and the standing mitigation.
- 24-bit streams are parsed and then **refused at the capability boundary**
  (`UnsupportedSampleSize`), not decoded. The `s4` fixture exists to pin that refusal, and the
  walk's suites include it in framing tests and exclude it from PCM comparisons by name.
- `SEEKTABLE` reading (`stream::SeekTable`, `Frames::anchored(n)`) exists and is witnessed, but
  the product path has no random access to give it: the ROM reads a manifest and plays forward.
