# ULI2.md — product codec decision: ULI2 in a standalone `uli-lite` crate

Status: **accepted 2026-11-13; GBA-first since 2026-11-14; at near-parity with
the C + ASM reference since 2026-11-19.** ULI2 is the repository's product codec
and is in the state it needs to be used as the drop-in codec here.

Measured (mGBA, WAITCNT 4/2, 262,144-cycle period, no UI, both corpora pinned):

| | worst | mean | of period |
|---|---:|---:|---:|
| album decode wall (16,553 frames) | **194,695** (f4,998) | **169,354** | **74.27% / 64.60%** |
| gate total (decode + convert + IRQ + DMA) | **226,152** | **200,624** | **86.27% / 76.53%** |

**13.73 points of margin at the worst frame, zero starved periods over 16,552**,
and **1.070x the reference on the decode wall** (C's own record: decode 69.43%,
gate 73.63%). U2, U3 and U4 are met; U5 is met on hardware for the three player
ROMs with one open defect (the 1 Hz text redraw delays the feeder on 104 of 16,553
periods). All five hot kernels are hand-written ARM asm, each twinned bit-exactly
on the ARM7TDMI, and the product bit reader is closed-world.

Of the remaining 12.6-point gate gap against C, only **4.84 points are decode** —
the other **7.80 are the convert/IRQ/DMA half, at ~2.9x C's cost**. The output
path, not the codec, is the largest remaining difference.

The history that produced those numbers — every PR, every step, and every
falsification — is the ledger, not this file; see "Where things live" below.

## Where things live

| question | file |
|---|---|
| why ULI2, what it is, capacity, and what it is not | **this file** |
| the acceptance gates (U1–U7) and their verdicts | **this file**, §Gates |
| the risks that survive, and the decisions taken | **this file**, §Risks / §Decision log |
| the current measured state | [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md), top |
| the plan, the phases, and the order of work | [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) |
| the measurement record: arms, falsifications, the landing log | [`crates/uli-lite/RECORD.md`](crates/uli-lite/RECORD.md) |
| architecture, memory layout, the measurement *method*, the kernel table, **the encoder plan** | [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) |
| the product path's architecture and its gates | [`crates/uli-lite-gba/README.md`](crates/uli-lite-gba/README.md) |
| the reference/oracle crate's API and scope | [`crates/uli-lite/README.md`](crates/uli-lite/README.md) |
| how to run the measurement vehicle | [`examples/uli_bench/README.md`](examples/uli_bench/README.md) |

**One owner per topic.** A fact lives in exactly one of these files and the others
link to it: gates, risks and decisions belong to *this* file, measurements to the
ledger, architecture to the crate READMEs. That rule exists because the way these
docs went stale was the same gate table existing in two files and drifting apart —
the ledger's copy still said "U5: not started" long after U5 had passed on
hardware.

Companion files:

- [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) —
  **the live plan**: phases, gates, measurement ledger, PR order.
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
path**, not from the repository. The current decoder port is the first
milestone. For the time being the offline encoder is expected to be reused from
`gba-audio` (C/OpenMP), because it is the encoding reference and the source of
the measured assets. The target is to replace that encoder and any other
C-language tooling or source material with Rust, so this project does not depend
on the `gba-audio` repository or its C sources. General Python scripts and
reusable external tools such as `ffmpeg` remain acceptable; the isolation rule
is about not reusing or depending on `gba-audio`'s C implementation, not about
avoiding every non-Rust tool.

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
| **ULI2 hop 512** (product) | **32.29 / 25.78 dB** | **29.48 / 23.87 dB** | **86.27% worst gate total** at WAITCNT 4/2 in this tree; one 512-sample frame (~15.6 ms) |
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
- The reference's 73.63% is *its* gate total (decode + conversion + IRQ + DMA) at
  4/2, with a 69.43% decode wall. **This tree measures 86.27% gate / 74.27%
  decode** — the codec is at 1.070x the reference on the decode wall, and the
  remaining gate gap is mostly the convert/IRQ/DMA half (see "Where things live"
  above and §Gates). Never inherit the reference's percentage for our CPU.

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

## Capacity: how much audio fits a cartridge

The product configuration carries **12,735 B/s ≈ 101.9 kbit/s**, and that is a
measured constant rather than a target: the reference album is 33,280,736 B over
2,612.815 s (12,737 B/s) and `assets/sound/bioluminescent.wav` encoded at the same
settings is 3,293,048 B over 258.613 s (12,733 B/s). Capacity is therefore
arithmetic on one number plus the ROM's own code — **~304 KB measured**, from
`uli-play.gba`'s 3,597,068 B against its 3,293,048 B blob. That image is the
minimal one (codec, feeder, no UI); a product ROM with a screen, art and a
manifest carries more code and correspondingly less audio, so read the table as
an upper bound.

| cartridge | audio payload (after ROM code) | **play time** |
|---|---:|---:|
| 1 MiB | 744,556 B | 0:58 |
| 2 MiB | 1,793,132 B | 2:21 |
| 4 MiB | 3,890,284 B | 5:05 |
| 8 MiB | 8,084,588 B | 10:35 |
| 16 MiB | 16,473,196 B | 21:34 |
| **32 MiB** | 33,250,412 B | **43:31** |
| — per MiB | 1,048,576 B | 1:22 |

The reference project's album is 43:32 for the same reason this table's largest
row is: it was encoded with `--budget 33,292,288` (32 MiB less 256 KiB), so the
encoder *fills the cart* and "an album" is
whatever 43½ minutes at ~102 kbit/s is. **The C and Rust sides are the same
number**: there is one bitstream format and one encoder (the C encoder of record,
run off-tree to produce assets), and the Rust decoder is bit-exact, so a Rust
build cannot store more or fewer minutes than the C one. There is no Rust encoder
yet; when one lands it must honour the same rate contract, with the byte budget as
its knob.

### There is no sample-rate table, and in v1 there cannot be one

ULI2's rate is **fixed at 32,768 Hz** (decision log, "Rate flexibility — v1
fixed"): the container carries `sample_rate`, and this decoder rejects anything
but 32,768 (`format::RATE`). So capacity is a function of **bitrate**, and
bitrate is the encoder's album-wide allocation (`--budget`, `--cap`), not a rate
choice. An album longer than ~43 minutes therefore has two levers, and they are
not equivalent.

**Lever 1 — a lower bitrate budget: no format change, costs fidelity.** Fill the
same 33,250,412 B payload at a lower rate:

| target bitrate | bytes/s | **play time on 32 MiB** | what else moves |
|---|---:|---:|---|
| 101.9 kbit/s (product) | 12,735 | 43:31 | the reference album's 32.29 / 25.78 dB; this tree's worst frame 96.02% |
| 80 kbit/s | 10,000 | 55:25 | fidelity cost **unmeasured**; entropy/dequant CPU falls |
| 64 kbit/s | 8,000 | 69:16 | as above, further |
| 51 kbit/s | 6,375 | 86:56 | as above, further |

This lever *helps* part of the CPU: a coarser quantiser leaves fewer nonzero
coefficients, so the entropy, dequant and M/S work falls. The synthesis half does
not move — it was ~68% of the frame at the last attribution — so read the saving
as confined to the other third. Its cost is fidelity, and nothing in this
repository measures the trade below 102 kbit/s — the reference's own record only prices `--cap 74`
against uncapped (~0.3 dB), not a budget change. A fidelity figure for 80 or
64 kbit/s would have to be measured on the committed corpus first.

**Lever 2 — a 16,384 Hz mode: a format and encoder change, costs the top
octave.** At the same bits per stereo sample, halving the rate halves the bytes
per second (~50.9 kbit/s, 6,367 B/s), so the same 32 MiB cart holds **≈ 87:03**
— and because a frame is still 512 samples, the decoder's work per frame is
unchanged while frames arrive half as often. The frame period doubles to 524,288
cycles, so the album's measured costs become **decode 42.2% / gate total 48.0%**
of the period: half the CPU per second of audio, and roughly double the headroom
for a UI. What it costs is everything above 8 kHz
(Nyquist), all of this file's measurements would have to be re-taken (none of
them transfer), the parser's rate check and `format::RATE` change, and the
encoder needs the new rate plus a budget at the same bits/sample.

One note on the application seam, so none of this is read as a conflict. The
album player is being built out against **one** backend today — FLAC — and its
16,384 Hz product clock is *that backend's* rate: the `rate_hz` declaration and
the GAFP packer's profile refusal (`ALBUM-PLAYER.md` Q14), and the single
`RATE_LOG2` the application currently ties its geometry to. §4.2b's
one-backend-per-ROM choice already has the clock derived from whichever backend
is linked as PR-3 work (PR-2's D4), so a ULI2 backend at 32,768 Hz does not
contradict the application: it exercises the seam as designed. A 16,384 Hz ULI2
*mode* would instead coincide with the app's present constant (no geometry change
at all), while keeping 32,768 Hz keeps the band every ULI2 measurement in this
file was taken in. Decide that on capacity, CPU and the top octave — not on the
application's present constant, which is the FLAC path's.

The two levers compose, and the composition is where "long album" stops being
interesting: 16,384 Hz at ~25.5 kbit/s (half the product's bits per sample again)
fills the same cart with **≈ 174 minutes**, at a fidelity nobody has measured. The
empirical check for any of this is one corpus run — encode the same master at the
new target, `make uli-album-bench` and `make uli-gate` for the cycles, and a
fidelity run for the dB. Until that exists, none of these rows is a claim.

## What we are building

### Crates

```
crates/uli-lite/           reference + oracle (not shipped since 2026-11-14)
  src/lib.rs               public facade; `Kernels`/`PortableKernels` seam
  src/bitstream.rs         MSB-first u16 reader, EG0
  src/format.rs            ULI2/ULA2 constants
  src/tables.rs            ULA2 tables-blob parser; canonical Huffman + LUT
  src/tables_data.rs       synthesis tables generated from the reference
  src/imdct.rs             integer pre/FFT/post and window/OLA (the reference;
                           compiled in ARM mode on ARM targets since 1b)
  src/frame.rs             dequant, M/S, frame decode, int8/int16 output
  src/album.rs             ULA2 header, track/seek tables, strings
  tests/reference_corpus.rs + tests/fixtures/
  examples/gen_expected.rs frozen-vector maintenance tool

crates/uli-lite-gba/       the product decoder — every hot kernel hand-written ARM
                           asm in `.iwram`, plus the ARM-mode entropy/dequant path
  src/decoder.rs           `GbaDecoder`: closed-world reader, `HotState`, `ClassTables`
  src/lib.rs               `ArmKernels` + the kernel dispatch and `.iwram` statics
  asm/{pre,post,ola,ms}_arm.s, asm/fft_arm_v5.s   the five kernels, each twinned
                           bit-exactly against `uli_lite::imdct` on the ARM7TDMI
  tests/twin.rs            kernel differential vs the portable reference
  tests/product_twin.rs    full-frame differential vs the portable decoder
  tests/premise.rs         the closed-world premise (`rbits`/`esck`) over the
                           fixture and every generated corpus

examples/uli_bench/        the measurement vehicle: proves its own decode, then
                           times full frames, per-kernel sums and both kernel
                           realisations; also the attribution and gate arms
examples/uli_player/,      the shipped player ROM, the ring/feeder host suite
crates/uli-playback/       and the playback data path
examples/album_probe/      the album manifest probe (AP-0), not the codec
```

Both crates are **standalone workspaces** with their own `[workspace]` table,
exactly like `crates/flac-lite` and `crates/flac-lite-gba`. They inherit the
root `.cargo/config.toml` (`thumbv4t-none-eabi`, `-Zbuild-std=core,alloc`,
`-Tgba.ld`, `arm7tdmi`); do **not** add a local `.cargo/config.toml`, because
cargo concatenates target rustflags and the linker script would be passed
twice. The overlay is `publish = false`; the core can be published when its API
is stable.

### Boundaries

- **Decoder first; Rust encoder later.** The current encoder is the offline
  C/OpenMP tool from `gba-audio`. It is not part of the GBA image and is not
  ported in the current decoder milestone. It is expected to be reused for the
  time being so the decoder and playback work can use the measured assets. The
  target is to reimplement that encoder and any other C-language tooling or
  source material in Rust, removing the `gba-audio`/C-source dependency. General
  Python orchestration and reusable external tools such as `ffmpeg` stay
  allowed.
- **No C, no dependency, everything in-repo.** Arm-mode asm may be adapted from
  the reference implementation; **C source may not be incorporated,** and there
  is **no build, link or vendoring dependency on `gba-audio`** — the encoder of
  record is used *off-tree* to produce assets, and every artefact this project
  builds, tests or ships lives in this repository and is twinned against this
  repository's oracle. This is the form the isolation rule takes for the asm
  work (PR C-1) and it is unchanged for tooling.
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

### Encoder and asset generation: current vs target

**Current.** The encoder of record is still the offline C/OpenMP tool from
`gba-audio`. It produces the ULA2 blob used by the tests and measurements; it is
never linked into the GBA image. The existing prep path (source decoding,
resampling, track assembly) is external tooling for now; Python orchestration
and `ffmpeg` are acceptable and are not what this isolation rule targets.

**Target.** Reimplement any C-language tooling or source material in Rust and
remove the dependency on `gba-audio` and its C sources. That means, at minimum:

- a from-scratch Rust encoder: forward MDCT, quantisation/search, table
  training, album-wide allocation, the per-frame CPU cap, and the ULA2
  container/tables/seek/strings writer;
- Rust replacements for any other C-language asset-generation code that would
  otherwise be reused from `gba-audio`;
- deterministic, documented encoder output and its own round-trip witness.

General Python scripts remain fine: they may orchestrate source preparation,
resampling, track assembly or fixture generation, and they may call reusable
external tools such as `ffmpeg`. The rule is that **C-language tooling/source
material must not be reused or depended on**, not that every script becomes
Rust. The test tree now follows that rule too: `gen_expected.c` has been
replaced by the Rust tool `crates/uli-lite/examples/gen_expected.rs`. The
committed expected-vector file remains the frozen C-pinned reference; the Rust
tool is a maintenance/snapshot generator, not an independent oracle, so a
failing decoder test must not be "fixed" by regenerating it.

**Status (2026-11-20): the encoder exists in-repo, and what is left is acceptance,
not capability.** S1–S5 have landed in `crates/uli-enc`: the PCM boundary and the
container, the forward MDCT, the RD search with M/S, trained code tables, and album
assembly with the WAV front end and a frame-parallel search. It encodes the
43.5-minute, 12-track `cross` album inside a passed-in budget with 392 bytes to spare,
every track seeked to and decoded; a natively-32,768 Hz WAV reaches its PCM pair's exact
blob; `biomaster`'s blob is unchanged from S4's while the wall clock fell 12m16s → 3m46s
at four threads. **E-R9 is met in mechanism, not in rationale:** that album takes 40m11s
where the reference takes 14m30s on the same box, because every candidate is coded
exactly rather than estimated — a named follow-up, not a claim. What remains is **S6** (the
optional cycle-aware re-code, worth 1.48 dB to the reference at a binding cap) and
**S7**, the acceptance matrix that holds this encoder to the C oracle's bytes — the
hand-written C encoder is now the **oracle and the asset producer**, not the only
encoder in existence. Requirements, slices and gates:
[`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) -> "The
encoder — the last missing piece".

### Suggested public API

The exact names are for implementation; the shape is the decision:

The implemented shape (PR A):

```rust
pub trait Kernels {
    fn pre(x: &[i32; M], z: &mut [i32; 2 * H]);
    fn fft(z: &mut [i32; 2 * H]);
    fn post(z: &[i32; 2 * H], y: &mut [i32; M]);
    fn ola(y: &[i32; M], p: &mut [i32; H], out8: &mut [i8], out16: Option<&mut [i16]>);
}

pub struct Decoder<'a, 's, K: Kernels = PortableKernels> { /* album + caller state */ }

impl<'a, 's> Decoder<'a, 's, PortableKernels> {
    pub fn new(album: Album<'a>, state: &'s mut DecoderState) -> Result<Self, Error>;
}

impl<'a, 's, K: Kernels> Decoder<'a, 's, K> {
    pub fn with_kernels(album: Album<'a>, state: &'s mut DecoderState) -> Result<Self, Error>;
    pub fn decode_frame(/* int8 L/R, optional int16 L/R */) -> Result<FrameInfo, Error>;
}
```

`FrameInfo` reports bits consumed and the per-frame operation counts used by the
cycle model. `PortableKernels` is the reference; `uli_lite_gba::ArmKernels` is
the product's ARM/IWRAM realisation. The core keeps the reference for host
tests and as the oracle; the product is free to be GBA-shaped.

---

## Porting plan

### Implementation status (2026-11-19)

**ULI2 is at near-parity with the C + ASM reference and is the repository's
product codec.** mGBA, WAITCNT 4/2, 262,144-cycle period, both corpora pinned
(`xover=0 hfnz=0`):

| | worst | mean | of period |
|---|---:|---:|---:|
| 129-frame fixture decode | 193,317 (f98) | 172,109 | 73.75% / 65.66% |
| album decode wall (16,553 frames) | **194,695** (f4,998) | **169,354** | 74.27% / 64.60% |
| **gate total** (decode + convert + IRQ + DMA) | **226,152** | **200,624** | **86.27% / 76.53%** |

- **U2 (bit-exact host), U3 (bit-exact product) and U4 (real-time decode) are
  met**; U4's ceiling stays written down as the period (mGBA, no UI). The gate
  carries **13.73 points of margin at the worst frame, zero starved periods over
  16,552**.
- **Against the reference: 1.070x on the decode wall.** C's own record gives
  decode 69.43% and gate 73.63%; ours are 74.27% and 86.27%. Of the remaining
  12.6-point gate gap only **4.84 points are decode** — **7.80 points are the
  convert/IRQ/DMA half**, at ~2.9x C's cost. The output half, not the codec, is
  the largest remaining difference.
- **All five hot kernels are hand-written asm** and individually twinned on the
  ARM7TDMI: `pre` 1.00x C, `fft` 1.05–1.09x, `post` ~1.03x, `ola` 1.00x, M/S
  0.64x (mean). Entropy/dequant is ~35,656 mean, **at or below C's 42–48k**.
- **The product bit reader is closed-world** (`ULI2.md` decision log,
  2026-11-19): no bounds test, no error flag, a 16-bit refill contract, because
  the bake gate proves the stream before the ROM exists. `tests/premise.rs` is
  that proof's seed; `uli_lite` remains the checked oracle.
- **The player ships** (U5 met on hardware for three ROMs, with one open defect:
  the 1 Hz text redraw delays the feeder on 104 of 16,553 periods).
- **Still open:** the non-decode half of the gate (above); the dense-frame tail
  of the entropy path; the from-scratch Rust encoder/asset pipeline; and a
  licensing/publish decision (`publish = false`).

The step-by-step ladder that produced these numbers — PRs A through E, the
root-cause audit, the closed-world pass — is in
[`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md), which
is the authoritative ledger. This document is the decision record.

### Phase 1b — what the on-target measurement found (2026-11-13)

**Instrument.** `examples/uli_bench` (standalone workspace; `make
uli-bench-rom` / `make uli-bench`). It sets `WAITCNT` to `0x0000` (4/2, no
prefetch; `agb` boots at `0x4317` and the log prints both), times inside
timer2→timer3 windows at 16.78 MHz with IRQs off, subtracts a measured
empty-window overhead (35 cycles; 5-sample spread 0 in the final run), and
reports sums and counts — no divide on the measured path. It runs two full
post-pin decodes and a per-frame timed decode; the timed pass is hashed too.
All numbers are mGBA, not hardware. The overlay's kernels are proven
bit-exact on the host (`make uli-overlay-test`) *and* on-target (the image
runs both realisations on identical inputs and compares).

**Full frames** (129-frame corpus, `uli2_tiny.uli`; percentage of the
262,144-cycle frame period; synthesis = 2 × the measured per-channel
`pre+fft+post+ola` sequence):

| arm | worst | mean | synthesis / frame | non-synthesis / frame |
|---|---:|---:|---:|---:|
| portable core, Thumb-1, code in ROM (before 1b's fix) | 6,622,160 (**2526%**) | 5,897,421 (2250%) | 5,026,842 | 870,579 |
| portable core, ARM-mode kernels, code in ROM | 2,970,042 (**1133%**) | 2,245,992 (857%) | 1,334,527 | 911,465 |
| overlay kernels (projection, not a measured frame) | 2,122,748 (**810%**) | 1,398,699 (534%) | 487,233 | 911,465 |
| C/asm reference, `gba-audio` record | ~182,000 (69.4%) | ~153,000 (58.4%) | ~134,600 | ~45,000 |

The overlay row was arithmetic, not a decode: at Phase 1b `Decoder` still
called the core kernels. It is `non-synthesis + 2 × overlay_seq` from the
measured sums. **PR A (2026-11-14) replaced this projection with a measured
full frame: 1,829,247 worst / 1,322,800 mean (698% / 505%)** through
`ArmKernels`. The Phase 1b projection was 810% / 534%; the measured frame
agrees with the same-image decomposition to 0.02%.

**Kernel cost**, cycles per channel per frame (sum of 10 calls ÷ 10):

| realisation | pre | fft | post | ola | sequence | vs C |
|---|---:|---:|---:|---:|---:|---:|
| core, Thumb-1, ROM | 426,678 | 1,065,200 | 440,231 | 563,619 | 2,513,421 | 37x |
| core, ARM mode, ROM | 48,705 | 268,775 | 52,445 | 296,959 | 667,263 | 9.9x |
| overlay, ARM, unchecked, ROM | 51,779 | 249,731 | 52,446 | 259,568 | 614,115 | 9.1x |
| overlay, ARM, unchecked, **`.iwram`** | 21,260 | 85,065 | 20,640 | 116,061 | 243,617 | 3.6x |
| overlay + `iwram-tables` (8,672 B) | 17,676 | 81,537 | 16,032 | 112,477 | 228,313 | 3.4x |
| C/asm reference | ~9,700 | ~31,400 | ~10,900 | ~15,300 | ~67,300 | 1x |

**What moved the numbers, in order:**

1. **Thumb-1 has no 64-bit multiply.** The first measurement found **44
   `__aeabi_lmul` call sites and zero `SMULL`** in the image: every twiddle
   product in every kernel was a software 64-bit multiply. Compiling the four
   kernels in ARM mode (`#[instruction_set(arm::a32)]`, a source-level no-op
   that is bit-exact and host-tested) cut the frame 2.2x and synthesis 3.8x.
   Four `lmul` sites remain, all in the Thumb dequant path (1 per nonzero
   coefficient).
2. **Code location dominates instruction count.** The same overlay kernels
   move from 614,115 to 243,617 cycles/channel (**2.5x**) when their code is
   placed in `.iwram` instead of the cartridge. At `WAITCNT 4/2` with prefetch
   off, the 32-bit ARM fetch is the hot cost. This is the same reason the
   reference port describes “5 KB of ARM code” in IWRAM.
3. **Bounds checks are a small residual once the code is ARM mode.** Removing
   them in the overlay (raw pointers, audited `unsafe`) is 8% on ROM
   (667,263 → 614,115). They were a bigger multiplier only because they
   forced spill-heavy code in Thumb.
4. **The remaining kernel gap is real, and OLA is the outlier.** ARM-mode,
   unchecked, IWRAM code is still 3.6x the C/asm kernel total, and OLA alone
   is 7.6x (116,061 vs 15,300). That is the case for Phase 1d's hand-written
   asm — *after* the hot path is ARM/IWRAM, not instead of it.
5. **The entropy/bit-reader path is now the larger blocker.** It is 911,465
   cycles/frame and unchanged by any kernel work — 41% of today's portable
   frame and 65% of the projected overlay frame. It is still Thumb-1 code in
   ROM with checked slices and `Result` plumbing on the bit path. The same two
   levers (ARM mode, IWRAM) have not been applied there at all.

The encoder cycle model is not a usable proxy for this code: the worst frame
(index 72, 248 quads / 140 escapes / 616 nonzero coefficients) is modelled at
193,782 cycles and measured at 2,970,042 — **15.3x** the model. The model is a
C/asm estimate; the gap is codegen, not a model defect to “fix” by moving
constants.

**Plan consequences (the decision this measurement buys):**

- **Playback (Phase 2) was gated on U4** — at PR B's 204% worst a feeder would
  have underrun continuously. U4 was met on 2026-11-15 and the player shipped;
  see §Gates and "Where things live" above for the current position.
- **The next milestone is Phase 1c-2 (PR B) — the entropy hot path in the
  overlay — not hand-written asm.** The measurement says the dominant losses
  are codegen and placement, which the overlay can fix in portable Rust
  arithmetic; asm for four kernels cannot recover a non-kernel deficit this
  large.
- **The asm milestone moves to Phase 1d (PR C)**, and its scope is decided by
  PR B's re-measurement. The kernel table already bounds what asm can buy:
  even a perfect kernel port removes 487,233 cycles/frame from PR A's
  1,829,247, leaving ~1.34M (512%) because the non-synthesis part dominates.
- **Keep 1b's ARM-mode kernel fix in the core.** It is a pure codegen
  attribute, bit-exact on the host corpus and in the ROM, and every ARM build
  benefits. It does not make the core platform-bound: the attribute is gated on
  `cfg(target_arch = "arm")`, and the API is unchanged.
- **IWRAM budget:** the overlay's four kernels are **1,596 B** of code. The
  tables are **8,672 B** for a 6.3% kernel gain (243,617 → 228,313) and are
  **not** taken by default; the reference port spends 9.4 KB there, and this
  tree's UI/audio budget will decide later (U6).

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

### Phase 1 — GBA decode-only (landed)

1. Embed a small album, parse the `ULA2` header, decode frames, hash the int8
   output, compare with the host pin. **Landed** (that image has since been
   retired; `examples/uli_bench` does the same job and more).
2. Put the ~100 KiB Huffman LUTs in EWRAM and the hot state in IWRAM.
   **Landed** — `HotState` (8,192 B) and `ClassTables` (560 B) are `.iwram`
   statics; the code tables stay in `.ewram`.
3. Start with the portable Rust kernels and measure. **Landed as Phase 1b** —
   and the measurement reordered the plan: asm is not the next step.

Exit gate: **GBA decode hash matches the host pin**, with no overflows, no
`|X| >= 2^29` violations, and a measured worst frame. — **Met** (both pins,
`xover=0 hfnz=0`; the worst frame is measured in Phase 1b).

### Phase 1c-1 — GBA-first architecture + full-frame instrument (PR A, landed 2026-11-14)

- The `Kernels` seam and `PortableKernels`; `Decoder<K>` generic with a
  default.
- `uli_lite_gba::ArmKernels` implementing the seam with the existing
  ARM/IWRAM kernels.
- `examples/uli_bench` decodes full frames through both kernel sets; the core
  is the control; per-kernel arms and the on-target twin stay.
- Host `decoder_twin` proves the product kernel set decodes the corpus
  frame-for-frame identically.

Exit gate (met): both full-frame arms match the pins, `xover=0 hfnz=0`, no
wrapped window; **698% worst / 505% mean** for the product arm, against the
same-image core control at 1021% / 828%. No speed claim beyond the record.

### Phase 1c-2 — GBA-first hot path (PR B, landed 2026-11-14)

The 1b measurement puts the losses here, not in the absence of asm, and PR A
had measured the rest: **837,092 cycles/frame** outside the synthesis kernels.
The **entropy, bit-reader, dequant and M/S** hot path now lives in
`uli-lite-gba::GbaDecoder` (`src/decoder.rs`) as an ARM-mode realisation with a
32-bit cached bit window instead of the 64-bit slice-rebuild `peek`; sentinel
errors instead of `Result` per symbol; unchecked pointers with documented
bounds. The core stays the correctness reference and `tests/product_twin.rs`
twins it frame-for-frame.

Exit gate (met): the measured full-frame worst is **204%** of the 262,144-cycle
period — under the ≤2× gate, pins unchanged. `rest` fell from 837,092 to
**195,484 cycles/frame** (4.3×). Placement: `HotState` is EWRAM, not IWRAM — an
IWRAM placement hung on target because the product path needs ≈15.0 KiB of
stack and 8 KiB in `.iwram` left less than that.

### Phase 1d — close the gap (PR C, measured 2026-11-14)

PR C measured the worst frame before optimising it: **63% synthesis kernels /
33% entropy / 4% M/S + wrapper**, and found the codegen is not short of
instructions (6 `SMULL` + 6 `SMLAL` per butterfly, no `__aeabi_lmul`) but short
of **registers** — the radix-4 butterfly spills 66 intermediates. It split into
**C-1 (hand-written ARM asm kernels, ~194,000 cycles saved)** and **C-2 (entropy
rewrite, ~90,000 saved)**, projected ~91–94% together and **not parity** — the plan
of record carries PR C-3 for the difference. This section's original
framing follows.

The reference asm is `gba-audio/src/uli/gba/uli_imdct_arm.S` (author’s
permission required — a clean-room asm is the alternative). The kernel prize is
now the **whole** remaining delta:
PR B measured `ArmKernels` seq/channel at **169,061** against the C/asm
reference’s ~67,300 (2.51×), and synthesis is 63% of the 533,605-cycle PR B
frame. A perfect kernel port would remove at most ~2 × 98,000 = ~196,000
cycles/frame from 533,605 — which is exactly what takes a 204% frame under
100%. PR C therefore scopes to the four synthesis kernels (and OLA’s 116,061
vs 15,300 outlier first), plus tuning and block moves.

### Phase 2 — product playback

**U4 met (2026-11-15); this phase shipped.** The gate that used to hold it here
was the decode wall sitting at 204% of the period at PR B, which would have
underrun the feeder continuously. It now measures 86.27% worst on the gate total
with 13.73 points of margin and zero starved periods, and the player is in
`examples/uli_player/` with its hardware witness in U5.

1. Reuse the 32,768 Hz ring/FIFO feeder built for the FLAC product path.
   **That feeder is `crates/gba-playback` since 2026-11-15** ([`ALBUM-PLAYER.md`](ALBUM-PLAYER.md)
   §2.2 — the application line lifted it out of the `examples/flac_spike/` spike; it now also
   carries `RingFeed::truncate()`, §4.4's skip seam). The ring is `RingFeed<const RING: usize>`
   and takes its chunk size from the caller's `take_chunk` destination, so this phase's rate and
   frame size are a different const parameter rather than a fork; link the crate as a path
   dependency instead of `#[path]`-including a spike.
   ULI2 frames are 512 samples/channel; adjust the chunk/period arithmetic but
   keep the same timer/DMA/IRQ discipline.
2. Feed int8 directly (headphone profile) and the optional pre-DAC int16 path.
   The **speaker profile is in scope** and is served by the *output stage*, not
   the decoder: full stereo decode always, mono fold-down after it (see the
   plan of record, PR D). Preserve ULI2’s fused 8-bit rounding
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
| **U1 — reference** | `uli-lite` remains a bit-exact, host-testable reference and oracle (no longer a portable-product claim). | `make uli-test`; committed corpus; thumbv4t `check` |
| **U2 — bit-exact host** | The Rust core matches the C integer reference frame-for-frame on the committed corpus and rejects malformed frames without panic. | `make uli-test`; pinned hashes |
| **U3 — bit-exact product** | The product path decodes the corpus to the pinned int8/int16 frames, in the host full-frame twin and on the target. | **Met.** The product path is `GbaDecoder` with all five kernels in hand-written ARM asm (`ArmKernels` remains as a kernel overlay, not the product). `make uli-overlay-test` (host, both builds), `make uli-bench` (target pins), `make uli-kernel-twin` (asm vs Rust on the ARM7TDMI). |
| **U4 — real-time decode** | Worst decode frame fits the product budget at WAITCNT 4/2; the gate total (decode + convert + IRQ + DMA) is under the agreed ceiling, written down as the **262,144-cycle period** (mGBA, no UI — not a hardware playback claim). | **Met (2026-11-19):** album decode wall **194,695 worst / 169,354 mean = 74.27% / 64.60%**; gate total **226,152 / 200,624 = 86.27% / 76.53%** — **13.73 points of margin at the worst frame, zero starved periods over 16,552**, `frames_over_period=0`. Against the C+ASM reference's own record (decode 69.43%, gate 73.63%) that is **1.070x on the decode wall**; of the remaining 12.6-point gate gap only 4.84 points are decode, the other 7.80 being the convert/IRQ/DMA half at ~2.9x C's cost. `make uli-gate`. The ladder that got here (PR A through E, the root-cause audit, the closed-world pass) is in `crates/uli-lite/RECORD.md`. |
| **U5 — product playback** | 32,768 Hz product path runs with zero underruns and a clean period audit. | **Met on hardware (GBA SP, 2026-11-16) for the three player ROMs, two runs each, identical to the cycle**: headphone 94.52% worst / 81.39% mean, speaker 95.89% / 82.61%, two-track 94.56% / 80.92%, all with 0 underruns; the GBA 001 was unavailable. **With one open defect**: the 1 Hz text redraw delays the feeder past the DMA's grace on 104 of 16,553 periods (audible splices; `late`, not underruns), so the audio default is the end-only screen until the UI is made non-blocking. **Met in mGBA too.** `make uli-player` (headphone) and `make uli-player-speaker`: zero underruns, period audit closed (periods = served + 1 tail), ring full, three builds measured (2026-11-16). The hardware half is still owed — two units × two runs of *this* player; the manual listen that has happened is the PR E audio-only ROM, which carried no counted ledger. |
| **U6 — memory** | IWRAM/EWRAM/stack stay inside the product budget with the UI and feeder linked. | **Met for the decode/player path (2026-11-19):** `.iwram` 26,496 B of 32,768 (code + `HotState` 8,192 + `ClassTables` 560 + `q3`/`clz6` 148); stack high-water 948 B (bench) / 1,988 B (gate); code tables in `.ewram`. The UI is not in the gate arm, so this is re-checked when the UI lands. `make uli-gate`; linker map. |
| **U7 — licensing/spec** | A public crate, if published, has a permissive license and a standalone format specification; otherwise it remains `publish = false`. | **Deferred by decision 2026-11-14: `publish = false`, no license work now.** |

The U4 ceiling is not copied from `gba-audio`'s 75%: this tree's UI, feeder and
animation budget decide it, and it was written down as **the 262,144-cycle
period** — the gate total is measured, not assigned, and it now sits at **86.27%
worst with 13.73 points of margin**. Whether that margin survives the UI is the
open question, which is why U4's claim stays "the period" rather than a playback
claim on hardware; the hardware half lives in U5.

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

1. **Licensing — deferred by decision (2026-11-14).** `gba-audio` has no LICENSE
   file, so `uli-lite` stays `publish = false` and **no license work is done
   now**. What stays true if a publish decision is ever taken: a written
   permissive license (MIT/Apache-2.0/Unlicense) from the author, or a
   clean-room rewrite, plus the format name — and the adapted asm counts as
   derived work, so it is only acceptable while nothing is published.
2. **Format stability.** ULI2 is versioned (`ULA2` version 1) but not a
   standard. Pin the reference commit, write a standalone `FORMAT.md` before
   publishing, and treat bitstream changes as breaking.
3. **Tonal/sustained material — accepted for v1 (2026-11-14).** The format is
   fixed: no long-block mode, no window switching. Recorded for fidelity
   reporting, not as work. Fixed hop 512 regressed on the harpsichord
   track in `gba-audio` (24.8 dB plain vs 28.4 dB at hop 1024). ULC’s window
   switching is the alternative. Measure the repertoire before claiming ULI2 is
   universally better; a long-block ULI2 mode would be a format change.
4. **Memory.** The reference `UliDec` is ~105 KB, mostly 10-bit Huffman LUTs;
   the GBA port uses ~22.6 KB IWRAM and ~96 KB EWRAM. The LUT width is an
   implementation detail, not part of the bitstream, so an 8-bit or smaller LUT
   is possible at a CPU cost. Measure before deciding.
5. **CPU inheritance — closed (2026-11-19), and it was codegen.** 73.63% is the
   `gba-audio` gate total on its C/ARM build, not this tree's, and it must never
   be read as a transferable number. The port started at **25.3x the period** and
   the causes were diagnosed and fixed in order: Thumb-1's software 64-bit
   multiply (44 `__aeabi_lmul` sites before the ARM-mode fix), cartridge
   instruction fetch at 4/2/no-prefetch (2.5x for moving the same code to
   `.iwram`), then — the two that mattered last — the entropy path's register
   pressure and the last Rust kernels. **The product path now measures 1.070x the
   reference on the decode wall** (194,695 vs 182,007 cycles at the worst frame);
   the residual is the densest frames, not a systemic gap. What is *not* closed
   is the gate: our convert/IRQ/DMA half costs ~2.9x C's, which is 7.80 of the
   12.6 points we are still behind on the gate total.

6. **Encoder packaging and independence — v1 boundary set (2026-11-14).** For
   v1 the encoder of record is used **off-tree** to produce assets; this project
   builds nothing from it and vendors no C source. That keeps the repo free of
   C while the decoder work proceeds, and it means ULI2 is still not the
   public-reuse answer: packaging a codec needs an in-repo encoder. **The Rust
   encoder milestone is scheduled as of 2026-11-19** — it is the last missing
   piece before ULI2 can be the album player's codec — under the same rule: no C
   source, no dependency, every artefact in this repository. Requirements, slices
   and acceptance gates: `crates/uli-lite/PLAN-OF-RECORD.md` → "The encoder".
7. **Rate flexibility — v1 fixed (2026-11-14).** 32,768 Hz, hop 512, stereo, no
   long-block. Higher rates (65,536 Hz) would be a format/encoder change and are
   explicitly out of scope; the v1 target is matching the C project's fidelity
   in this repo, and their record does not reach 65 kHz either.
8. **Output-stage fairness.** Compare ULI2 against FLAC/ULC through the same
   8-bit quantiser, or preserve ULI2’s fused rounding and account for the
   difference. Half an LSB matters at this noise floor.

---

## Decision log

**Dated entries, newest first. The cycle figures inside older entries are the
numbers *of that day* and are superseded by the current state at the top of
[`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md); each
entry is kept for the decision and its reasoning, not for its numbers.**

- **2026-11-20 — S5 lands, and two small things turn out to be load-bearing: the
  CLI's defaults and the resampler's corner.** The WAV front end is a *second input shape*
  rather than a second boundary — `--album/--tracks` stays E-R2's contract, `--wav` is the
  convenience over it, and `--emit-pcm DIR` writes back the pair it assembled so "a 32,768 Hz
  WAV reaches the identical blob its PCM pair does" is a test (`tests/wav.rs`) instead of a
  sentence. Two failures are kept because they will recur:
  1. **A CLI default is behaviour, and rewriting a parser is a behaviour change.** `--fine`
  (4) and training-on moved silently while the argument parser was rewritten for `--wav`;
  on the 5-frame `corr` fixture the search emitted 18,331 bits where S4 emitted 15,664, and
  the blob hash changed. No test noticed, because the RD tests pass explicit shapes; what
  caught it was re-running S4's binary and diffing hashes. The defaults are restored and the
  equality to S4's blob is recorded for `biomaster` (`6f4f3d66…82be51`).
  2. **A resampler corner that is wrong only when downsampling.** The first version used a
  fixed 0.475-of-source-rate cutoff, right for upsampling and wrong the other way: a 20 kHz
  tone at 44,100 Hz came out at −2.2 dB and aliased straight into the band. The test that
  found it asserts the guard band, not the passband.
  **E-R9 is half-discharged and that is recorded as half.** The curves were nearly all of the
  wall clock (`--threads` reached only emit), so `Curves::compute`/`refine` are now
  frame-parallel: `biomaster` 12m16s → **3m46s** at four threads with a **byte-identical**
  blob. But the 43.5-minute album takes **40m11s** where the reference takes **14m30s** on the
  same box and flags, because `measure` codes every `(g, ms)` candidate exactly
  (`write_chan` twice) while the reference's curve pass estimates — the mechanism is met, the
  "bake is in the edit loop" rationale is not. Making the curve pass cheaper is a new step;
  S6 (cycle-aware re-code) and S7 (the acceptance matrix) remain open.

- **2026-11-19 (later) — the encoder's shape is settled before it is written: plan-
  then-emit, four determinism rules, a byte budget that is an argument, and exactness
  measured at the PCM boundary.** The encoder plan (`crates/uli-lite/PLAN-OF-RECORD.md`
  → "The encoder — the last missing piece") left four questions open; they are
  ruled here in five parts, and the reason to rule them now rather than during S1
  is that each is a structure, not a feature — retrofitting any of them means
  rewriting a component that already has tests, measurements and pins under it.

  1. **`uli-lite` stays read-only; the write side lives in `uli-enc`.** The core is
     the codec as the GBA needs it — parse, decode, nothing else. So nothing is
     added to it to make writing possible, including no "helpful" table serialiser.
     The consequence is accepted knowingly: the blob *geometry* is arithmetic inside
     `CodeTables::parse_into`, so `uli-enc` reproduces a layout it is not handed —
     tolerated because a wrong offset fails the parse, unlike a wrong bound, which
     fails silently (which is exactly the bug class PR #171 just fixed).
  2. **The E-R4 bounds get one home.** They are promoted into `uli-lite` as a
     validation over already-parsed tables (allocation-free, since the crate is
     `no_std`), called by the encoder, the existing test and eventually the bake
     gate. Validation is read-side, so (1) is not breached; a third copy of `5`
     and `2` would have been the drift this file keeps documenting.
  3. **Plan, then emit.** The encoder fixes its tables and its per-frame `g` plan,
     sizes every frame, and only then writes frames in index order. This was never
     optional in practice — the seek table needs each 256th frame's bit position and
     `g`, and `g` is coded differentially against the previous frame — but ruling it
     early also settles the parallel shape: once the `g` plan exists, frame bodies
     are independent, which is how the reference itself parallelises.
     Determinism is then four named rules (R5a–d: threads compute but never order;
     fixed-order float reductions; transform constants checked in rather than
     computed by libm at encode time; every tie names a winner), witnessed by one
     cheap check — same input at 1 and N threads, same SHA-256.
  4. **Refuse early, refuse the whole album, and take the size as an argument.**
     `--budget BYTES` (default 32 MiB − 256 KiB, the reference's own) is the
     encoder's only knowledge of size, checked against the container overhead
     *before* the first frame is encoded; a too-small budget exits nonzero citing
     needed and allowed, rather than quietly trimming tracks to squeeze in.
     **The encoder writes no ROM and never sees `cart_bytes`**, because the ROM
     holds code, art, `viz` and more than one asset producer, and by
     `ALBUM-PLAYER.md` §10.1 art is carved out of the audio budget, never the
     reverse — so the audio budget must arrive as an input from the bake, which
     already owns "refuse, with a cited number".
  5. **Byte-exactness against the C encoder is the gate, and it is measured at the
     PCM boundary** (`album.f32` + `tracks.tsv` → blob), because that is where it
     holds: both the fixture and a real 16,553-frame album were re-encoded by the
     reference to their committed SHA-256 (measurements in
     [`crates/uli-lite/RECORD.md`](crates/uli-lite/RECORD.md) → "the reference
     encoder reproduces its own corpora"). The resampler is out of the byte
     contract for a better reason than the one previously given — the reference's
     own resampling step dispatches across libraries, so identical settings do not
     guarantee identical bytes on two machines. The proof is a small matrix rather
     than a sweep, and must include one deliberately off-default cell so agreement
     cannot come from a shared lucky path. Running the reference as an oracle does
     not breach the no-C rule: E-R1 forbids build/link/vendoring dependencies, and
     `uli-album-asset` already shells out to the same binary.

- **2026-11-19 — the product ROM is closed-world: a well-formed stream is an
  input assumption, not a runtime obligation.** We own the encoder, the container
  and the bake, so the product decoder does not need to detect, reject or
  survive a malformed stream — **and does not need to prove the stream bound at
  runtime either**. Where such a proof is worth having, it belongs **before the
  ROM is baked**, in the host gate that already decodes the whole album with the
  checked decoder against the C-pinned hashes; a bad encode should fail the
  build, not a frame at 03:00 on hardware. This does not weaken the portable
  decoder: `uli_lite` keeps every guarantee it has, including U2's "rejects
  malformed frames without panic", because it is now **the oracle for a premise**
  rather than the product path — the split the tree already uses for the kernels
  (`PortableKernels` checked/host versus `ArmKernels` unchecked/product). Two
  consequences worth naming, because both were previously declined *for safety
  reasons* and are now available: the product reader can drop its bounds test,
  its error flag and its per-symbol length test, and the escape path's
  `t < 32` / `t <= valid` guards are dead on our own tables (`esck <= 2`, and
  `rbits <= 5` on every PAIR class across all five corpora). The proposals this
  unblocks, and their sizes, are in `crates/uli-lite/PLAN-OF-RECORD.md` → "The
  entropy path against the C+ASM reference (2026-11-19)". **Landed the same day:**
  P2 (`ClassTables` to IWRAM, −897 mean) and P1 + P3 (the closed-world reader,
  −5,420 mean / −9,973 worst), taking the **gate total to 226,853 worst / 201,126
  mean = 86.54% / 76.72%, 13.46 points of margin** with 0 starved periods, and
  the product `dec_chan` down 1,376 B. The premise is no longer only a comment:
  `tests/premise.rs` asserts `rbits <= 5` on every PAIR class and `esck <= 2` for
  the fixture and every generated corpus, as the seed of the bake gate.

  **Correction to this entry (2026-11-19).** It first read "`rbits == 0` on every
  PAIR class", and the test it cited passed that assertion while checking nothing
  of the kind: the test compared against a local `const CT_PAIR: u8 = 1`, which is
  `CT_QUAD`, so the bound ran over the two QUAD classes where `rbits` is never
  read, and the eleven classes that do carry a raw field (`r` =
  `0,0,0,0,0,0,1,2,3,4,5`) went unexamined. The same shadowed constant also transposed the class
  names in `PLAN-OF-RECORD.md`'s Cause 3 ("two `PAIR` … eleven `QUAD`"), and a
  second bug in that test — one `../` too many in the generated-corpus paths —
  meant the four generated corpora were never opened, so the claim "across all
  five corpora" rested on the fixture alone. **Nothing measured was unsound:** the
  16-bit contract holds because the raw pair arm refills between the two
  coefficients of a group, capping every read run at 16 bits (`r + 1 <= 6` for the
  worst raw digit), and the P1/P3 cycle figures are unaffected — they are timing
  measurements, not consequences of the bound. What changed is the justification
  and the coverage: the test now imports the format's real constants, asserts
  `rbits <= 5` (the measured maximum, and requirement E-R4), rejects a PAIR class
  at `rbits == 6` as a self-witness, and reads all five corpora — which now agree:
  identical class structure everywhere (`ctype`/`rbits`/`esck`), `max rbits == 5`,
  `max esck == 2` — with the trained length rows differing per album, as they should.

- **2026-11-18 — `post` as ARM asm: the product path is now all hand-written
  kernels, and the kernel table was re-derived with bit-exact arms.** `post` was
  the last Rust kernel in the decode path, and the only one whose excess was
  *nameable in instructions* rather than frame density: **18 per iteration against
  the reference's 14**, every extra one addressing (the `z` pair and the twiddles
  each cost an `add`+`ldr` or two `ldr` where the reference does one `ldmia`; the
  backward `Y` store cost a computed address where the reference post-indexes).
  `asm/post_arm.s` is that 14-instruction loop, and it is worth **−3,061 cycles on
  *every* frame** — worst and mean alike, because `post` always runs 512
  iterations regardless of content. Album decode wall **209,162 → 206,101 worst /
  178,341 → 175,280 mean**; **gate total 240,705 → 237,646 (91.82% → 90.65%) /
  209,721 → 206,637 (80.00% → 78.83%)**, margin **8.18 → 9.35 points**; and on the
  matched `cross` corpus — the 12-track album C's own record was measured on —
  **210,209 → 207,148 worst / 174,009 → 170,939 mean = 1.139x / 1.117x C**. Bit-
  exact: both pins on every corpus, `postasm=true` kernel twin PASS, two runs
  identical to the cycle. **The same day produced a correction that matters more
  than the win.** The by-difference attribution arms worked by *removing* a
  stage, so every later stage multiplied different data and the arm charged the
  downstream change to the part it removed — measured at **1.61x inflation for
  `pre` at the densest frame**. Three arms now *add a second identical instance*
  instead (`attrib-pre2`, `-post2`, `-entropy2`, the last replaying `dec_chan`
  from a saved `BitReader`, which is `Copy`); all three are **bit-exact**, so
  their pins PASS and nothing is confounded. The clean table: **fft 32.9%,
  entropy 29.6%, ola 14.6%, post 11.3%, pre 9.3%, ms+wrapper 2.3%**, closing to
  −2.4% of `synth` where the old table over-counted by +5.9%. **Three parts the
  record had called deficits are at or ahead of C: `pre` 1.00x worst / 0.91x mean,
  `ola` 1.00x / 0.98x, M/S 0.64x mean** — and the file's `pre 1.22x/1.23x/1.36x`
  ladder entries are artifacts. **Recorded limit:** because the audit put the data
  in `Rs`, kernel cost now varies with content by more than the gaps being judged
  (`pre` 1.20x, `post` 1.15x, `fft` 1.13x, `ola` 1.08x, entropy 26x; C's entropy
  row sits at our 43rd percentile), so single-frame ratios cannot rank deficits —
  only an all-frames per-part mean on the same corpus can. **Also corrected:** the
  file's "1.03x on the mean" was a mis-transcription (our mean divided by C's
  worst); the honest figure was 1.17x, now **1.117x**.

- **2026-11-17 — the frame pointer: the build target is now a checked-in spec.**
  The last open audit item was the register the target spec spends on a frame
  pointer in every Rust function. It is now taken, and the mechanism matters more
  than the win: **`-Cforce-frame-pointers=no` is inert** (rebuilt from a clean
  `target/` both ways, the ROMs are byte-identical), so the only lever was the
  target spec itself. `targets/thumbv4t-gba.json` is the pinned toolchain's own
  `--print target-spec-json` for `thumbv4t-none-eabi` with exactly one field
  changed — `frame-pointer: "always"` → `"may-omit"` — and `.cargo/config.toml`
  builds for it. Bit-exact (both album pins, `xover=0 hfnz=0`, `msasm=true` twin,
  two runs identical to the cycle): **gate total 242,370 → 240,705 worst (92.46% →
  91.82%) / 210,626 → 209,721 mean (80.35% → 80.00%)**, margin **7.54 → 8.18
  points**; album decode wall **210,787 → 209,162** worst / 179,226 → 178,341 mean;
  **13 → 0** of 210 functions set up a frame and `.iwram` code drops **896 B**.
  r11 becomes an ordinary data register, which is what the four hand-written asm
  kernels already did with it — the hardware register was never special, only the
  ABI's reservation. **A second assumption died with it:** `frame-pointer:
  "non-leaf"` — the plausible middle setting — would have helped **nothing**,
  because `dec_chan` is non-leaf (ten `bl` sites to `huff_slow`/`eg_slow`); only
  `may-omit` frees r11 there. And the pin's stated purpose does not apply on this
  target at all: `panic-strategy` is `abort`, there is no unwinder and no
  `RUST_BACKTRACE`, and mGBA debugs from the DWARF we emit. **Decision — the spec
  is an instrument, so it is reviewed, documented and witnessed like the toolchain
  pin**: `make target-spec-check` re-dumps the built-in spec from the pinned rustc
  and fails on any difference other than `frame-pointer` (including a field
  appearing on one side only), so a toolchain move cannot quietly change the ABI,
  data layout or linker flavour underneath every number in this file. Recorded
  with it: the one known cross-track consequence — `flac-spike`'s PR 3 verdict
  moves from `measured 2/2` to `measured 0/2` because its calibration demands five
  *identical* empty-window samples and now sees a 2-cycle first-sample difference.
  Decode is untouched there and `make flac-test` passes; no `make` target gates
  that verdict, and re-baselining it is the FLAC track's own change.

- **2026-11-16 — the loose-ends sweep landed, and it re-measured the gate.** Two of
  the three open audit items are in: the bit reader's **single `ldrh`** and
  **`zero16` as four inline `stmia`** (it had been a `bl __aeabi_memclr4` call).
  129-frame corpus **185,207 → 183,242 mean**, album **180,921 → 179,226 mean**,
  and the **gate total 251,712 → 242,370 worst (96.02% → 92.46%) / 221,245 →
  210,626 mean (84.40% → 80.35%)** with **7.54 points of margin** and zero starved
  periods — the re-measurement the audit required before U4 was quoted again.
  **The `ldrh` needed a contract, not a cast**: `include_bytes!` gives no alignment
  guarantee and the embedded albums were measured landing on an **odd address**, so
  the tree now has `uli_lite::Aligned` (13 embed sites) and
  `Error::UnalignedStream`, checked once at open and witnessed by a test that an
  odd-based blob is *rejected* rather than decoded into UB. **Three further items
  were measured and rejected**, and the reason is the finding: almost everything
  left in that category is a **branch**, ARM7TDMI charges **3 cycles for a taken
  one**, and Rust cannot emit ARM conditional instructions (no `it` on v4T) — a
  branchless clamped refill measured **+1,352 cycles/frame** because the load
  address then depends on `end`. So that category's ceiling is **~2,000**, not the
  projected 3,500-5,500, and the same limit — predication plus the register the
  target spec spends on a frame pointer — is what stands between the decode wall
  and C's number. **On the mean we are 1.17x C, not 1.03x** — that earlier figure
  was a mis-transcription (our mean divided by C's worst) corrected 2026-11-18, so
  the remaining gap is systematic rather than one worst-frame shape.
  **The frame-pointer half of that limit then landed too (2026-11-17):** the GBA
  build target is now a checked-in spec — `targets/thumbv4t-gba.json`, the pinned
  toolchain's own `thumbv4t-none-eabi` spec with one field changed,
  `frame-pointer: "may-omit"` — so r11 is no longer reserved in every Rust
  function. Bit-exact, worth a further **−1,665 worst (−0.64 pts) / −905 mean
  (−0.34 pts) on the gate total** and **−896 B of `.iwram` code**: the gate is now
  **240,705 worst / 209,721 mean = 91.82% / 80.00% with 8.18 points of margin**,
  and the album decode wall **209,162 / 178,341 = 79.79% / 68.03%**. Two things
  had only been assumed and are now measured: `-Cforce-frame-pointers=no` is
  **inert** (byte-identical ROM), and `frame-pointer: "non-leaf"` would have
  helped **nothing** because `dec_chan` is non-leaf — only `may-omit` frees the
  register where it matters. See [`targets/README.md`](targets/README.md).
  **`post` then became the last Rust kernel to go (2026-11-18):** it was the one
  kernel whose excess was nameable in instructions rather than frame density —
  **18 per iteration against the reference's 14**, every extra one addressing — and
  the reference's loop is worth **−3,061 cycles on *every* frame** (content-
  independent, because `post` always runs 512 iterations). Album decode wall
  **209,162 → 206,101 worst / 178,341 → 175,280 mean**; gate total **240,705 →
  237,646 (91.82% → 90.65%) / 209,721 → 206,637 (80.00% → 78.83%)**, margin
  **8.18 → 9.35 points**; and on the matched `cross` corpus **210,209 → 207,148
  worst / 174,009 → 170,939 mean = 1.139x / 1.117x C**. The same day, the kernel
  table was re-derived with **bit-exact** attribution arms (`attrib-pre2`,
  `-post2`, `-entropy2`), which showed that three parts previously reported as
  deficits are at or ahead of C: **`pre` 1.00x worst / 0.91x mean, `ola` 1.00x /
  0.98x, M/S 0.64x mean**. The old by-difference arms were inflated by up to 1.61x
  because removing a stage changed the data every later stage multiplied.

- **2026-11-16 — aggressive performance targeting, after a root-cause audit against
  the C decoder's code.** Six rounds of loop tuning had reached ~1% of the frame
  per round and the record read that as a ceiling. It was not: the audit compared
  our implementation against **the C source**, not against its summary table, and
  found two structural items worth more than the six rounds combined — the FFT's
  `smull`/`smlal` operands were reversed (the twiddle in `Rs`; ARM7TDMI multiply
  cost early-terminates on `Rs`, and `TW` is ~2^30, so every FFT multiply ran at
  the multiplier's slow end), and M/S had no hand-written kernel at all. Both
  landed bit-exact for **−8,896 worst / −9,282 mean**, frame **210,355 / 185,207 =
  80.2% / 70.7%**, ratio to C **1.16x**. **Decision: the decode wall is pursued
  aggressively on named, measured items rather than accepted at "close to C".**
  The remaining work is five named things (the reader's `ldrh`, the per-symbol
  contract sweep, the frame pointer, entropy arithmetic, and a gate-total
  re-measurement), not a block called "entropy". Two are recorded as
  **deficiencies in progress, not settled trades**: the target spec's
  `frame-pointer: "always"` costs one register in every Rust function relative to
  gcc (**tracked; needs a custom target JSON and an unwinding answer — not given
  up on**), and the Rust `dec_chan` carries per-symbol bounds/`bad`/validity tests
  that C pays once at stream open (**a general category** of loose ends to
  tighten, of which eight instances are already named). Recorded with the audit:
  **a ratio against a summary number is a locator, never a grade** — two of the
  four kernel ratios in this file named a cause that was not there, and one hid a
  cause that was.

- **2026-11-14 — v1 scope frozen; asm is in-repo, C is not.** Arm-mode asm may
  be adapted from the reference; C source may not be incorporated, and there is
  no build/link/vendoring dependency on `gba-audio` — every artefact lives here
  and is twinned against this repo's oracle. The speaker profile is **in
  scope**, served by an output-stage mono fold-down over a full stereo decode
  (never a one-IMDCT decoder path). The 129-frame corpus is the iteration
  instrument and the worst-frame analysis is deferred until the decoder is near
  real time. The U4 ceiling is measured, not assigned. IWRAM priority is audio
  over UI. `publish = false`; no license work. No arithmetic promise for
  malformed streams (rejection only). v1 format is fixed.
- **2026-11-14 — stereo is the product target.** ULI2 playback and the decoder
  stay stereo, aimed at the headphone output; fidelity is the project's purpose
  and is not traded for CPU headroom. The GBA speaker is mono while the headphone
  jack is stereo, so the **speaker profile is in scope but belongs to the output
  stage**: full stereo decode always, mono fold-down after it. gba-audio's
  `ULI_GBA_MONO`/`ULI_OUT_M16` one-IMDCT path is not taken, and U4 is read
  against the two-channel frame, with both regimes measured. Their 69.4% is itself a
  stereo figure, and their mono speaker profile measured 84.2% worst because the
  collapse moved into the driver. Recorded with the same decision: the C
  kernel-pair row is **134,600** (C's own per-kernel decomposition), striking an
  earlier ~146,000, so C-1's prize is ~194,000 and the C-1 + C-2 + C-3
  projection is ~193,000–213,000 (73–81%).
- **2026-11-14 — ULI2 is GBA-first.** Portability is not a product
  requirement for a fixed-rate, single-target GBA codec. The `flac-lite`
  portability constraints are retired for the product path: `uli-lite`
  becomes the bit-exact oracle and cold parser, `uli-lite-gba` the product
  decoder (ARM state, `.iwram`, unsafe/raw pointers, optional asm). Kept:
  `no_std`, zero allocation, integer-only, bit-exactness, the live oracle,
  licensing. The live plan moves to
  [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md).
- **2026-11-14 — PR A landed: the product kernel path is measured, not
  projected.** The `Kernels` seam and `ArmKernels` make a full frame
  through the product kernels measurable: 1,829,247 worst / 1,322,800 mean
  (698% / 505% of the 262,144-cycle period), both pinned, same-image core
  control 2,676,986 / 2,171,620 (1021% / 828%). The Phase 1b projection
  (810%) is retired; the measured frame agrees with the same-image
  decomposition to 0.02%. Absolute frame numbers are image-specific at this
  fetch-dominated scale; every future arm keeps its control in the same
  image.
- **2026-11-14 — PR C: the remaining gap is architectural, not tuning.**
  Measured frame 101 (worst) at 63% synthesis kernels / 33% entropy / 4% M/S +
  wrapper. The kernels are not short of instructions — the FFT butterfly emits
  the optimal 6 `SMULL` + 6 `SMLAL` — but short of registers: it spills 66
  intermediates because 14 values are live at once. Entropy is ~6.6× the C
  reference per coefficient. Four cheap levers were built and measured: dequant
  fusion neutral, full `huff` inlining +2.7% for 4,252 B of IWRAM (rejected on
  U6), split `huff` +1.6% for 896 B (kept → **200% / 174%**), frame pointers
  inert, `iwram-tables` crashes (stack 13,664 B vs ~15,040 needed). U4 stays
  open; the work is scoped as C-1 (asm kernels, ~182k cycles) and C-2 (entropy
  rewrite, ~90k), projected ~94%.
- **2026-11-14 — PR B landed: the hot path is GBA-first and the ≤2× gate is
  met.** `uli_lite_gba::GbaDecoder` (32-bit cached bit reader, unchecked
  canonical Huffman, dequant, M/S, ARM-mode `.iwram`) measures a full frame at
  **533,605 worst / 463,660 mean = 204% / 177%**, both pinned; same-image core
  control 1023% / 830%, `ArmKernels` 640% / 447%. Non-synthesis `rest` fell
  837,092 → 195,484 cycles/frame (4.3×), so synthesis (the C/asm comparison's
  2.51× gap) is now the whole remaining delta — PR C's scope. Placement record:
  `HotState` is in EWRAM because an IWRAM placement stack-overflowed on target
  (the product path needs ≈15.0 KiB of stack; the host twin was green either
  way). U4 (≤100%) is still not met.
- **2026-11-13 — accepted.** ULI2 is the product codec. Decoder-only port in
  `crates/uli-lite` + `crates/uli-lite-gba`; no new repository yet; FLAC stays
  as the reusable lossless library; ULC is documented but not planned.
- **2026-11-13 — Phase 1b: codegen is the gap, not the absence of asm.**
  `make uli-bench` measured 25.3x the frame period before the kernel codegen
  fix, 11.3x after ARM mode, and 8.1x projected with unchecked ARM kernels in
  IWRAM. Thumb-1’s `__aeabi_lmul` and cartridge instruction fetch dominate;
  bounds checks are 8%. Decision: Phase 1c is the entropy/bit-reader hot path
  in the overlay (ARM, IWRAM, 32-bit bit window), with a full-frame overlay
  decode and a ≤2× worst-frame exit gate; hand-written asm moves to Phase 1d
  and its scope is decided by 1c. Playback does not start before U4.
- **2026-11-13 — the core's synthesis kernels compile in ARM mode on ARM
  targets.** `#[instruction_set(arm::a32)]` on `pre`/`fft`/`post`/`ola` is a
  source-level no-op that lets LLVM use `SMULL` instead of `__aeabi_lmul`; it
  is bit-exact on the host corpus and in the ROM, and no API or platform
  dependency changes. Measured 2.2x on the frame, 3.8x on synthesis.
- **2026-11-13 — encoder independence target.** For the time being the
  `gba-audio` C encoder is reused for asset generation. The target is to
  reimplement that encoder and any other C-language tooling/source material in
  Rust, removing the `gba-audio`/C-source dependency. General Python scripts and
  reusable external tools such as `ffmpeg` remain acceptable; the isolation rule
  does not remove them.
- **2026-11-13 — expected-vector generator in Rust.** `gen_expected.c` was
  replaced by `crates/uli-lite/examples/gen_expected.rs`; the committed
  expected file remains the frozen C-pinned reference and the Rust tool is a
  maintenance/snapshot generator.
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
