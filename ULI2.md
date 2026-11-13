# ULI2.md — product codec decision: ULI2 in a standalone `uli-lite` crate

Status: **accepted 2026-11-13**. Implementation has not started. This file is the
entry point for the decision; once the crate exists, the live plan, gates and
measurement ledger move to `uli-docs/` in the same shape as
[`FLAC.md`](FLAC.md) / [`flac-docs/`](flac-docs/).

Companion files:

- [`FLAC.md`](FLAC.md) — the reusable lossless library. It stays.
- [`ULC.md`](ULC.md) — the public ULC codec. It is **not currently planned**;
  that file records its scope and intent only.

---

## Decision in one paragraph

Adopt **ULI2** — the MDCT codec shipped by the sibling `gba-audio` project
(called `ulc-inspired` in its earlier prototype) — as the product codec for
album-length lossy playback on the GBA. Implement the **decoder** as a
standalone, `no_std`, pure-Rust, zero-dependency, zero-allocation crate at
`crates/uli-lite`, with a separate `crates/uli-lite-gba` overlay for the
ARM-mode kernels and memory placement. Do **not** open a new repository yet: the
crate boundary is the extraction seam, exactly as it is for `flac-lite`. Keep
`flac-lite` as the reusable lossless library; remove FLAC from the **product
path**, not from the repository. Port only the decoder — the encoder stays
offline in C.

The first target is the shape the measurements were taken at: **32,768 Hz,
stereo, hop 512 (N = 1024), about 102 kbit/s for a full album**. Higher rates
and a long-block mode are future format questions, not part of this decision.

---

## Why ULI2

The decision is driven by measured output on the target, not by format
popularity. The comparable rows from the `gba-audio` codec shootout and the
Phase 2 build are:

| Arm | Codec-only SNR mean / worst | Plain 8-bit DAC SNR mean / worst | CPU / latency |
|---|---:|---:|---|
| **ULI2 hop 512** (product) | **32.29 / 25.78 dB** | **29.48 / 23.87 dB** | 73.63% worst gate total at WAITCNT 4/2; one 512-sample frame (~15.6 ms) |
| `ulc-inspired` hop 1024 (ULI2's prototype) | 32.87 / 30.79 dB | 29.87 / 24.56 dB | ~82% worst estimated; ~31 ms; not the production form |
| ULC-L (gba-audio's ULC reimplementation) | 25.28 / 21.78 dB | 24.71 / 20.25 dB | 87% mean / 95.6% worst in its 2048-block model |
| ULC2 MSE encoder | 21.52 / 20.02 dB | — | — |
| ULC2 stock-like encoder | 18.02 / 15.29 dB | — | — |
| FLAC lossy prequant (current FLAC product experiment) | 24.55 / 24.29 dB | 24.02 / 22.01 dB | already in this tree |

Sources: `gba-audio/proto/judge/RESULTS.md` (ranked shootout),
`gba-audio/docs/phase2/1-codec.md` (ULI2 construction and quality),
`gba-audio/docs/phase2/2-gba-decoder.md` (bit-exact GBA port and CPU), and
`gba-audio/proto/ulc/REPORT.md` (the ULC prototype). These are external
references from the sibling `gba-audio` checkout, not files in this repository.

The ULC-format rows are gba-audio’s **own reimplementation**, not a measurement
of stock ULC: their harness blocked building the stock codec. Treat them as
strong evidence about the companded-ULC format and as the reason ULC needs its
own spike before it is compared with ULI2. See [`ULC.md`](ULC.md).

The important reading:

- **ULI2 is the best measured real-time result after the GBA's 8-bit DAC.**
  It is about **5 dB** ahead of the ULC-L arm and about **5.5 dB** ahead of the
  lossy-FLAC experiment on mean plain-DAC SNR.
- The lossless-to-plain-DAC ceiling is about 35.2 dB. ULI2 is within roughly
  **5.7 dB** of that ceiling; ULC-L is roughly 10.5 dB below it. The DAC, not
  the codec, is the dominant limit from here.
- The gain is not an encoder accident. ULI2 uses a **linear quantiser** with
  per-band classes, per-album-trained Huffman tables, a DAC-floor-weighted
  album-wide allocation, and an encoder-side **per-frame CPU cap**. Stock ULC's
  companded quantiser is the weaker format; gba-audio's ULC-L linear variant
  gained about 3.8 dB over its MSE ULC2 encoder and about 7 dB over the
  stock-like encoder, which is the same lesson in the other direction.
- ULI2 gives up about **0.4 dB mean** against the hop-1024 `ulc-inspired`
  prototype, and buys back one-frame latency, a simpler fixed-hop decoder, and
  a real-time CPU model that fits the product. That is the right trade for a
  sound player.
- The 73.63% figure is the **gate total** (decode + conversion + IRQ + DMA) at
  4/2; the decode wall alone was 69.43% worst. Re-measure both in this tree —
  see “Gates” below — before inheriting the claim.

## What ULI2 is, and what it is not

**It is:**

- A fixed-point, integer-only MDCT codec. The normative C reference,
  `gba-audio/src/uli/uli_dec_ref.h`, is explicitly “portable integer C (no
  float, no malloc, no libc beyond memset/memcpy).”
- A decoder that writes the GBA's DAC samples directly: int8 L/R, with an
  optional pre-DAC int16 path. Frame f produces 512 samples per channel.
- A complete album container (`ULA2`): header, track table, seek table, strings,
  and the bitstream. Header fields include `sample_rate`, `hop`, `n_frames`,
  `n_tracks`, `seek_interval`, and `est_max_cycles`.
- A codec with existing **bit-exact test vectors and hashes** from the host
  reference and the GBA port. That is the strongest accelerator for a Rust port.

**It is not:**

- **Stock ULC.** ULI2 is a derivative with a different quantiser, entropy coding,
  fixed hop, and container. Do not use the names interchangeably. See
  [`ULC.md`](ULC.md).
- **Hardware-coupled.** The GBA-specific parts are the ARM asm kernels, the
  `.iwram`/`.ewram` placement, and the DMA/timer player. The codec core is
  portable C and can be a portable Rust crate.
- **A public standard.** It has one encoder, one GBA player, no packaged
  encoder library, no standalone format specification outside the source
  comments, and no LICENSE file in the `gba-audio` repository. Publishing a
  crate is a separate packaging/licensing decision, not an automatic consequence
  of porting the decoder.
- **A general-purpose codec.** The current format is fixed to 32,768 Hz stereo,
  hop 512, 32 bands of 16 coefficients, and no block switching. It is a
  specialised GBA album codec, not a replacement for FLAC/Opus/ULC outside that
  use case.

---

## What we are building

### Crates

```
crates/uli-lite/           portable core: no_std, zero deps, zero alloc
  src/lib.rs               public facade
  src/bitstream.rs         MSB-first u16 reader, EG0, canonical Huffman + LUT
  src/tables.rs            ULA2 tables-blob parser; Huffman LUT construction
  src/dequant.rs           band classes, pair/quad symbols, escapes, dequant
  src/imdct.rs             integer pre/FFT/post and window/OLA
  src/frame.rs             frame decode: 512 samples/channel, int8/int16 output
  src/album.rs             ULA2 header, track/seek tables, strings (optional)
  tests/                   host vectors, malformed streams, width/overflow tests

crates/uli-lite-gba/       GBA overlay: platform-bound realizations
  src/lib.rs               safe wrapper, feature-gated kernels
  asm/imdct.s              ARM-mode pre/FFT/post/OLA kernels (ported with permission)
  src/placement.rs         .iwram/.ewram tables and hot state
  tests/                   out-of-crate witnesses; no agb dependency
```

Both crates are **standalone workspaces** with their own `[workspace]` table,
exactly like `crates/flac-lite` and `crates/flac-lite-gba`. They inherit the
root `.cargo/config.toml` (`thumbv4t-none-eabi`, `-Zbuild-std=core,alloc`,
`-Tgba.ld`, `arm7tdmi`); do **not** add a local `.cargo/config.toml`, because
cargo concatenates target rustflags and the linker script would be passed
twice. The overlay is `publish = false`; the core can be published when its API
is stable.

### Boundaries

- **Decoder only.** The encoder is the offline C/OpenMP tool from `gba-audio`.
  It is not part of the GBA image and is not ported in this effort. A Rust
  encoder, if ever wanted, is a separate project.
- **No `agb` in the core.** The core is platform-free; the product ROM composes
  it with the playback layer. This is what makes host tests and future reuse
  possible.
- **No allocation.** The caller owns the decoder state and the output buffers.
  The `UliDec` state is large (~105 KB, mostly the 10-bit Huffman LUTs) and
  belongs in EWRAM on the GBA; the core crate must not hard-code that placement.
- **No float.** Integer/fixed-point only. The ARM7TDMI has no FPU, and the
  fixed-point reference already exists.
- **Codec core separate from the `ULA2` container.** A consumer that has raw
  frames or a different container can use `frame` without `album`.
- **Unsafe is confined to the overlay.** The core can be `#![deny(unsafe_code)]`
  (or an audited, tiny budget for slice split/placement); the overlay’s asm/FFI
  carries the SAFETY blocks, following `flac-lite-gba`’s pattern.

### Suggested public API

The exact names are for implementation; the shape is the decision:

```rust
pub struct Decoder<S> { /* caller-provided state, no alloc */ }

impl<S> Decoder<S> {
    pub fn new_in_place(state: &mut S, tables: &[u8]) -> Result<Self, Error>;
    pub fn decode_frame(
        &mut self,
        src: &[u8],
        out8l: &mut [i8],
        out8r: &mut [i8],
        out16l: Option<&mut [i16]>,
        out16r: Option<&mut [i16]>,
    ) -> Result<FrameInfo, Error>;
}
```

`FrameInfo` reports bits consumed and the per-frame operation counts used by the
cycle model. The GBA overlay swaps the four IMDCT kernels behind the same
facade; the core keeps a portable Rust implementation for host tests and for
platforms without asm.

---

## Porting plan

### Phase 0 — reference and vectors (host)

1. Port `uli_tables.c`, `uli_dec_ref.c` and the container/header parsing from
   `uli_format.h` into `uli-lite`.
2. Compile the C reference as a **host-only test oracle** (`cc` in a dev
   helper, never in the GBA build) and compare frame output bit-for-bit.
3. Import the committed `gba-audio` vectors/hashes as fixtures, or regenerate
   them from the same reference and pin the hashes in `tests/`.
4. Exercise every syntax element: pair/quad/escape, zero bands, M/S, raw LSBs,
   long Huffman codes, seek entries, malformed lengths.

Exit gate: **host decode is bit-exact against the C reference** on the committed
corpus, including error paths that must reject rather than panic.

### Phase 1 — GBA decode-only

1. Add a `uli_spike` / `uli_decode` example ROM: embed a small album, parse the
   `ULA2` header, decode frames, hash the int8 output, compare with the host
   pin.
2. Put `UliDec` in EWRAM and the hot `X`, `buf`, `P` buffers plus the IMDCT
   tables in IWRAM, as the reference port does. Measure section sizes and stack.
3. Start with the portable Rust kernels; measure. If the frame budget is not
   met, port the three asm kernels (`pre`, `fft`, `post`) and the OLA from
   `gba-audio/src/uli/gba/uli_imdct_arm.S` with the author’s permission. The
   reference ARM build is 6.3 KB of code and 9.4 KB of tables; the Rust/asm
   image will be its own measured number, not that one.

Exit gate: **GBA decode hash matches the host pin**, with no overflows, no
`|X| >= 2^29` violations, and a measured worst frame.

### Phase 2 — product playback

1. Reuse the 32,768 Hz ring/FIFO feeder built for the FLAC product path.
   ULI2 frames are 512 samples/channel; adjust the chunk/period arithmetic but
   keep the same timer/DMA/IRQ discipline.
2. Feed int8 directly (headphone profile) and the optional int16 path to the
   speaker profile. Preserve ULI2’s fused 8-bit rounding
   (`(acc + 2^39) >> 40`); do not silently replace it with the FLAC path’s
   truncation.
3. Drive the UI from the `ULA2` track/seek/strings tables. The existing GAFP
   manifest is FLAC-specific; do not force ULI2 into it.
4. Make the playback layer codec-agnostic with a small `PcmSource` trait, so
   FLAC remains a compile-time alternative and an A/B control rather than dead
   code.

Exit gate: **product path at 32,768 Hz, underruns = 0, period audit clean**, in
mGBA first, then on hardware.

### Phase 3 — hardware and retirement

Run both GBA 001 and GBA SP, two runs each, with underruns counted. Only then
does the real-time claim inherit the FLAC gate’s wording. After the hardware
run, switch the product default to ULI2 and mark the FLAC product path as an
alternate/archived backend. The `flac-lite` crate and its tests stay in the
tree.

---

## Gates

These are the ULI2 equivalents of the FLAC C1/C2 acceptance criteria. They are
claims to be measured, not inherited from `gba-audio`.

| Gate | Claim | Witness |
|---|---|---|
| **U1 — standalone core** | `uli-lite` is `no_std`, zero-dependency, zero-allocation, no `agb`, no GBA registers; a stranger can drive the facade. | `make uli-check`; host tests; `cargo build --target thumbv4t-none-eabi -Zbuild-std=core,alloc` |
| **U2 — bit-exact host** | The Rust core matches the C integer reference frame-for-frame on the committed corpus and rejects malformed frames without panic. | `make uli-test`; pinned hashes |
| **U3 — bit-exact GBA** | The GBA overlay produces the same int8/int16 frames as the host pin. | `make uli-rom-test`; mGBA hash |
| **U4 — real-time decode** | Worst decode frame fits the product budget at WAITCNT 4/2; the gate total (decode + convert + IRQ + DMA) is under the agreed ceiling. | instrument ROM; mGBA, then hardware |
| **U5 — product playback** | 32,768 Hz product path runs with zero underruns and a clean period audit. | `product-play` equivalent for ULI2; hardware runs |
| **U6 — memory** | IWRAM/EWRAM/stack stay inside the product budget with the UI and feeder linked. | linker map + runtime stack check |
| **U7 — licensing/spec** | A public crate, if published, has a permissive license and a standalone format specification; otherwise it remains `publish = false`. | license file + `uli-docs/format.md` |

The gate ceiling for U4 is not copied from `gba-audio`’s 75%: this tree’s UI,
feeder and animation budget decide it. Start from the measured 73.63% gate
total, add this tree’s non-audio cost, and write the number down before the
first real-time claim.

---

## Keeping FLAC

FLAC and ULI2 answer different questions:

- **FLAC is lossless and standard.** Where an asset fits, it is the highest
  fidelity the 8-bit DAC can produce. `flac-lite` is also a genuinely reusable
  `no_std` Rust library, which was the point of C1.
- **ULI2 is lossy and fits a full album.** At 32 MiB and ~43 minutes, it is the
  best measured codec in this tree’s target class.

Therefore:

- The **product default** moves to ULI2 for album-length music.
- The **`flac-lite` crate stays** as a standalone library and an optional
  backend. Its C1 acceptance is unchanged.
- The FLAC **product example/gates** may be archived or kept as an A/B arm;
  they are not deleted. The root crate already has no FLAC dependency, so the
  baseline ROM is unaffected either way.
- Do **not** link both decoders in a shipping ROM. Use mutually exclusive
  features; the memory and code budgets are tight.

This preserves the reusable lossless library and adds the high-fidelity lossy
product codec without pretending one codec covers both jobs.

---

## Risks and open questions

1. **Licensing.** `gba-audio` has no LICENSE file. Before publishing `uli-lite`,
   get a written permissive license (MIT/Apache-2.0/Unlicense) from the author,
   or keep the crate `publish = false`. A clean-room Rust implementation does
   not by itself grant rights to the encoder or the format name.
2. **Format stability.** ULI2 is versioned (`ULA2` version 1) but not a
   standard. Pin the reference commit, write a standalone `FORMAT.md` before
   publishing, and treat bitstream changes as breaking.
3. **Tonal/sustained material.** Fixed hop 512 regressed on the harpsichord
   track in `gba-audio` (24.8 dB plain vs 28.4 dB at hop 1024). ULC’s window
   switching is the alternative. Measure the repertoire before claiming ULI2 is
   universally better; a long-block ULI2 mode would be a format change.
4. **Memory.** The reference `UliDec` is ~105 KB, mostly 10-bit Huffman LUTs;
   the GBA port uses ~22.6 KB IWRAM and ~96 KB EWRAM. The LUT width is an
   implementation detail, not part of the bitstream, so an 8-bit or smaller LUT
   is possible at a CPU cost. Measure before deciding.
5. **CPU inheritance.** 73.63% is the `gba-audio` gate total on its driver, not
   this tree’s. The Rust port may be slower or faster than the C/asm original;
   the ARM asm kernels are likely necessary for the real-time gate.
6. **Encoder packaging.** A decoder-only crate is useful internally, but
   external users need an encoder. Publishing ULI2 as a reusable codec means
   packaging the C encoder or writing a Rust encoder; that is a separate scope
   and a major reason ULI2 is not the public-reuse answer.
7. **Rate flexibility.** ULI2 is fixed at 32,768 Hz. Goal #5 (higher rates such
   as 65,536 Hz) would require a format/encoder change. Do not block the
   current product on that; record it as future work.
8. **Output-stage fairness.** Compare ULI2 against FLAC/ULC through the same
   8-bit quantiser, or preserve ULI2’s fused rounding and account for the
   difference. Half an LSB matters at this noise floor.

---

## Decision log

- **2026-11-13 — accepted.** ULI2 is the product codec. Decoder-only port in
  `crates/uli-lite` + `crates/uli-lite-gba`; no new repository yet; FLAC stays
  as the reusable lossless library; ULC is documented but not planned.
- Alternatives considered and rejected for the product:
  - **Stock ULC** — public and reusable, but lower measured fidelity in the
    gba-audio ULC-format arms, a float reference with no integer oracle, a
    2,403-line asm player, and 2–3 frame latency at its default block size.
    Stock ULC still needs its own spike. See [`ULC.md`](ULC.md).
  - **Opus** — more compression, but float/CELT cost on a no-FPU target and a
    std-heavy Rust ecosystem; see [`OPUS.md`](OPUS.md).
  - **Lossy-prequant + FLAC** — keeps the FLAC container but measured about
    5.5 dB behind ULI2 after the DAC.
  - **New repository first** — rejected as premature; the crate boundary is
    the extraction seam, and cross-repo version skew would slow the decoder /
    player co-evolution.
