# ULC.md — stock ULC: scope and intent (not currently planned)

Status: **not planned as of 2026-11-13**. This file does not commit the project to
a ULC implementation. It records what a ULC crate would be for, what its scope
would be, and what would have to change before it becomes worth doing.

Companion files:

- [`ULI2.md`](ULI2.md) — the accepted product codec decision. ULI2 is the codec
  the product path is being built around.
- [`FLAC.md`](FLAC.md) — the reusable lossless library. It stays.

---

## Why this file exists

The product decision is **ULI2**, not ULC. But ULC has a property ULI2 does not:
it is a public, documented, Unlicense codec with an existing encoder and
players on several platforms. A `no_std`, pure-Rust ULC decoder would be a
generally reusable library, not just a GBA album codec.

That is a genuinely valuable outcome, and it is worth recording precisely so it
is not lost or confused with the product decision. A 2026-11-13 survey of
crates.io, GitHub repositories and Sourcegraph found no published `no_std`
pure-Rust ULC decoder; the existing implementations are C, assembly and Python.
It is **not** the current plan: this file is a scope document for a possible
later project.

---

## What ULC is, and how it differs from ULI2

**Stock ULC** is Aikku93’s Ultra-Low-Complexity MDCT audio codec:

- Public format specification: `ulc-codec/FormatSpecs.md`.
- Unlicense (public domain); `ulc-codec` and `ulcplayer-gba` are the reference
  C/asm implementations.
- A C encoder/decoder tool (`ulcencodetool`, `ulcdecodetool`), plus GBA, NDS,
  N64 and Python players.
- Variable block size (256–32768 coefficients), any sample rate, mono or
  stereo, window switching with overlap scaling, a nybble-based syntax, a
  companded quantiser, and noise fill.

**ULI2** is the codec shipped by the sibling `gba-audio` project. It began as
`ulc-inspired`, but it is a different format: fixed 32,768 Hz stereo, hop 512,
32 bands of 16 coefficients, no block switching, a linear quantiser, per-album
Huffman tables, and an `ULA2` album container. Its measured fidelity is
substantially higher than the ULC-format arms in the same shootout. Those
ULC-format numbers are gba-audio’s own reimplementation, not a build of the
stock codec; a stock-ULC spike would be required before comparing them
directly with ULI2.

Do not use the names interchangeably:

| | Stock ULC | ULI2 |
|---|---|---|
| Quantiser | Companded `v*abs(v)` | Linear, per-band step |
| Entropy coding | Fixed nybble syntax | Per-album canonical Huffman tables |
| Block size / rate | 256–32768, any rate (GBA player ≤2048) | Fixed hop 512, 32,768 Hz |
| Window switching | Yes, with overlap scaling | No |
| Noise fill | Yes | Not in the current format |
| Container | Simple `ULC2` file header | `ULA2` album container |
| Ecosystem | Public spec, encoder, multiple players | One encoder, one GBA player, custom |
| License | Unlicense | No LICENSE in `gba-audio`; coordinate |

The gba-audio ULC experiment also included **ULC-L**, a linear-quantiser variant
of the ULC format. ULC-L is a format change, not stock ULC: it is not compatible
with the stock encoder/players. It gained fidelity, but a public ULC crate that
wanted compatibility should implement **stock ULC**, not ULC-L.

---

## Intended scope if pursued

The goal would be a **generally reusable `no_std` Rust implementation of stock
ULC**, usable on the GBA and on other bare-metal or embedded targets. It would
not be a GBA-only decoder and it would not be the product codec.

### Crates

```
crates/ulc-lite/           portable core: no_std, zero deps, zero alloc
  src/lib.rs               public decoder facade
  src/bitstream.rs         nybble reader, window control, quantiser, runs, noise
  src/dequant.rs           coefficient reconstruction
  src/imdct.rs             fixed-point IMDCT / window / overlap-add
  src/frame.rs             block decode and lapping state
  src/container.rs         ULC2 file header (optional; separate from the codec)
  tests/                   host vectors, malformed streams, precision tests

crates/ulc-lite-gba/       optional GBA overlay
  src/lib.rs               safe wrapper, feature-gated kernels
  asm/*.s                  ARM-mode DCT-IV/FFT/OLA kernels
  src/placement.rs         .iwram/.ewram tables and hot state
```

The core would be standalone and platform-free, in the same shape as
`flac-lite`: `#![no_std]`, zero dependencies, no allocation, caller-provided
state and output buffers. The GBA overlay would be a separate, `publish = false`
crate, so the reusable core does not inherit a linker script or a GBA memory
map.

### Decoder only, initially

The encoder would remain the existing C tool. `ulcencodetool` is already public,
cross-platform and Unlicense, and it is what makes a decoder crate useful to
other people: they can produce streams without waiting for a Rust encoder. A
Rust encoder is out of scope for the first version. If the crate is later
published, users need the encoder documented and easy to build; the C tool
already is.

### Fixed-point synthesis

The GBA has no FPU, so the shipped path must be integer/fixed-point. The stock
C decoder is float-based (`ulcDecoder.c` plus `libfourier`), while the GBA
player is hand-written fixed-point assembly. A Rust implementation therefore has
to define a **normative integer path** rather than transliterate the float
reference. That is the main technical risk and the main reason this is not a
small port.

The intended shape:

- a portable fixed-point IMDCT (DCT-IV via a complex FFT), windowing,
  overlap-add, and the lapping buffer;
- explicit Q-format and rounding/saturation rules, documented and pinned;
- no float in the core; a host-only float reference may be used in tests but is
  not the shipped path;
- a deterministic noise-fill RNG in decoder state (the C reference’s static
  global seed is not a good API for a reusable library, and the chosen
  semantics must be documented).

### Tests and witnesses

A ULC crate would need more verification work than ULI2 because there is no
bit-exact integer reference to compare against:

- host tests against the C float decoder with a documented tolerance;
- crafted frames covering every window-control pattern, overlap-scale value,
  quantiser change, zero run, noise-fill case and stop code;
- malformed-stream tests that reject without panic or overflow;
- a pinned integer output hash for the fixed-point path;
- a GBA overlay witness against the existing `ulcplayer-gba` assembly, or
  against a pinned Rust integer decode if the assembly is the only fixed-point
  oracle.

### Public API shape

The exact names are for implementation; the intent is a small, allocation-free
facade:

```rust
pub struct Decoder<S> { /* caller-owned state */ }

impl<S> Decoder<S> {
    pub fn new_in_place(state: &mut S, config: Config) -> Result<Self, Error>;
    pub fn decode_block(
        &mut self,
        src: &[u8],
        out: &mut [i16],
    ) -> Result<BlockInfo, Error>;
}
```

The core would expose block decoding; a container parser would be a separate
module so a consumer can use raw blocks without adopting the `ULC2` file header.
The GBA overlay would swap the fixed-point kernels behind the same facade.

### Reusability and licensing

This is the strongest argument for ULC over ULI2 as a public crate:

- The format is public and documented, not defined by one project’s encoder.
- The encoder exists and is cross-platform.
- The code is Unlicense, so a Rust port can be published under MIT/Apache-2.0
  with credit to Aikku93; there is no permission gate like the one around
  `gba-audio`.
- The codec supports multiple rates, block sizes and channel counts, so the
  crate is useful beyond the GBA album use case.
- There is currently **no published `no_std` pure-Rust ULC decoder**; the Rust
  implementation would fill a real gap.

The tradeoff is fidelity: stock ULC is the lower-fidelity format, and the
gba-audio result belongs to ULI2. A public ULC crate would be valuable for
reuse and interoperability, not for beating ULI2 on the GBA.

---

## Why it is not currently planned

1. **The product goal is highest fidelity on the GBA.** ULI2 is the measured
   winner there. The gba-audio ULC-format arms are well behind, though those
   numbers are for gba-audio’s reimplementation rather than stock ULC; a stock
   ULC spike would still be required. ULC-L would be a format fork that loses
   the public-ecosystem advantage.
2. **The port is materially harder.** ULC has a float reference, a fixed-point
   assembly player, window switching/overlap scaling, and no integer oracle.
   The GBA player is about 2,403 lines of assembly; ULI2’s GBA decoder is much
   smaller and its C reference is already integer and bit-exact.
3. **Integration is slower.** ULC’s default block size is 2048 and the GBA
   player decodes over 2–3 frames (roughly 125–187 ms at 32,768 Hz). ULI2’s
   hop 512 gives a one-frame decoder path and a clean `uli_dec_frame` API that
   matches the existing 32,768 Hz feeder.
4. **One codec at a time.** The product path already has a large, careful FLAC
   effort and a new ULI2 port to land. Adding a second lossy decoder now would
   split the measurement and maintenance effort.
5. **The reusable-library need is already covered for lossless audio.**
   `flac-lite` is the standard, reusable `no_std` library. ULC would be the
   reusable *lossy* library, which is a separate deliverable, not a prerequisite
   for the product.

---

## What would change the decision

A ULC implementation becomes worth scheduling if one or more of these becomes
true:

- A stock-ULC spike measures fidelity close to ULI2 at an acceptable CPU and
  latency on the target — for example, a stock-ULC encode at the same byte
  budget that lands near 29 dB plain-DAC SNR with a worst frame inside the
  product budget.
- A **generally reusable public lossy codec crate** becomes a first-class
  deliverable, independent of the GBA product. In that case ULC is the better
  candidate than ULI2 because of the public format, encoder and Unlicense.
- A future product direction needs **rate flexibility** (for example the 65,536
  Hz goal) or channel counts/block sizes that ULI2’s fixed 32,768 Hz stereo
  format cannot express.
- ULI2’s licensing or format stability becomes untenable and the product needs
  a public fallback.
- A contributor or downstream project specifically wants a Rust ULC decoder and
  is willing to own the fixed-point verification burden.

## What would not change it

- **A ULC-L port.** ULC-L is a format fork; if the goal is a public reusable
  ULC crate, it must implement stock ULC. ULC-L would need its own format name,
  encoder and compatibility story.
- **A GBA-only ULC decoder.** The point of ULC is general reuse. A GBA-only
  implementation would be a product codec, and ULI2 already wins that job.
- **Replacing FLAC.** FLAC is lossless and standard; ULC is lossy and
  GBA-oriented. They are not substitutes.

---

## Non-goals for this file

- It does not schedule ULC work.
- It does not change the product codec decision in [`ULI2.md`](ULI2.md).
- It does not propose a ULC-L format fork.
- It does not require a new repository; if ULC is pursued, the same
  in-repo-crate-first rule applies, with extraction later if the crate proves
  reusable.
- It does not port the encoder.

---

## References

External, from the sibling `gba-audio` checkout and the public ULC projects:

- `ulc-codec/FormatSpecs.md` — the stock bitstream and inverse-transform spec.
- `ulc-codec/libulc/ulcDecoder.c` — the float reference decoder.
- `ulc-codec/tools/ulcEncodeTool.c` — the public encoder tool.
- `ulcplayer-gba/README.md` — the GBA player, ~65% CPU at 128 kbps and its
  2–3-frame block latency.
- `gba-audio/proto/ulc/REPORT.md` — gba-audio’s ULC/ULC-L reimplementation and
  its measured limits.
- `gba-audio/proto/judge/RESULTS.md` — the ULC, ULC-L and ULI2 comparison.
- `gba-audio/docs/phase2/1-codec.md` and `2-gba-decoder.md` — the ULI2
  production reference, for contrast.

For the accepted product decision, see [`ULI2.md`](ULI2.md). For the reusable
lossless library, see [`FLAC.md`](FLAC.md).
