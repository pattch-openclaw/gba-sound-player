# ULI2.md — product codec decision: ULI2 in a standalone `uli-lite` crate

Status: **accepted 2026-11-13; GBA-first since 2026-11-14.** Phase 0 (portable
core) is bit-exact on the host corpus and Phase 1 (decode-only ROM) passes its
pins. Phase 1b (2026-11-13) measured the portable decoder at 11.3x the frame
period. **PR A (2026-11-14) retired portability for the product path** and
measured a **full frame through the product's ARM/IWRAM kernels at 698% worst /
505% mean** — still ~7x over the 262,144-cycle period, so U4 is **not met** and
playback has not started. This file is the decision record (why ULI2, what it
is, the licensing and encoder risks); the live plan, gates and measurement
ledger are [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md).

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

crates/uli-lite-gba/       the product decoder (PR A: kernel seam wired)
  src/lib.rs               `ArmKernels` + ARM-mode, unchecked kernels in
                           `.iwram`; `iwram-tables` copies the 8,672 B tables
  tests/twin.rs            kernel differential vs the portable reference
  tests/decoder_twin.rs    full-frame differential vs the portable decoder
  (asm/imdct.s does not exist yet: asm is Phase 1d)

examples/uli_spike/        Phase 1 decode-only ROM
  src/main.rs              embeds the corpus, parses ULA2, decodes, hashes int8/int16
  assets/uli2_tiny.uli     the committed Phase 0 corpus

examples/uli_bench/        the measurement vehicle (Phase 1b + PR A)
  src/main.rs              proves the pins, then times full frames through both
                           kernel sets and both kernel realisations
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

This is a separate milestone after the decoder/playback path. It is not required
for the first product ROM, and it does not change the decoder work. Until it
lands, the `gba-audio` C encoder remains the encoder of record.

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

### Implementation status (2026-11-13)

- **Phase 0 — complete.** `crates/uli-lite` is a `no_std`, zero-dependency,
  zero-allocation integer decoder. `make uli-test` compiles it for
  `thumbv4t-none-eabi` and runs the committed 2-second corpus on the host:
  int8 and int16 outputs, per-frame bits/bitpos/ops/cycle model, both seek
  paths, malformed albums and short output buffers. It is bit-exact against
  the C reference.
- **Phase 1 — decode-only target witness.** `make uli-spike-rom` builds
  `examples/uli_spike`; in mGBA it decodes all 129 frames, prints
  `int8=e6e87652` and `int16=b3530978` (the C-reference album pins), reports
  `xover=0 hfnz=0`, and PASSes.
- **Phase 1b — measured (2026-11-13).** `make uli-bench` builds
  `examples/uli_bench`; the image proves its own decode (twice, both pins),
  then times every frame and both kernel realisations. Headline: **worst
  frame 2,970,042 cycles = 1133% of the 262,144-cycle frame period** with the
  portable core (25.3x before the ARM-mode kernel fix); a bounds-check-free
  ARM overlay in IWRAM projects 2,122,748 = **810%**. The C/asm reference is
  69.4%. U4 is **not met**; no playback claim is made. The measurement and
  its consequences are below, and in “Decision log”.
- **PR A — GBA-first architecture (2026-11-14).** The `Kernels` seam makes the
  product kernel set selectable from `Decoder`; `uli-lite-gba` provides
  `ArmKernels`; `make uli-bench` now decodes **full frames through both sets**
  instead of projecting. Measured product arm: **1,829,247 worst (698%) /
  1,322,800 mean (505%)**, both pinned; the same-image core control is
  2,676,986 / 2,171,620 (1021% / 828%). The Phase 1b projection is retired.
  Still ~7x the period; PR B is next.
- **Not yet:** the GBA-first entropy/bit-reader/dequant/M/S hot path (PR B,
  Phase 1c-2), hand-written asm (PR C, Phase 1d), playback, hardware runs, the
  from-scratch Rust encoder/asset pipeline, or a licensing/publish decision.

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

- **Playback (Phase 2) does not start yet.** At PR A's 698% worst (505% mean)
  a feeder would underrun continuously; building it now would only measure
  underruns. U4 is the gate.
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

1. `examples/uli_spike`: embed a small album, parse the `ULA2` header, decode
   frames, hash the int8 output, compare with the host pin. **Landed.**
2. Put `DecoderState` in EWRAM (the ~100 KiB Huffman LUTs dominate). The hot
   `X`, `buf`, `P` buffers are still inside that EWRAM state; their IWRAM
   placement is PR B (Phase 1c-2). **Partially landed.**
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

### Phase 1c-2 — GBA-first hot path (PR B, next)

The 1b measurement puts the losses here, not in the absence of asm, and PR A
has now measured the rest: **837,092 cycles/frame** outside the synthesis
kernels. Port the **entropy, bit-reader, dequant and M/S** hot path into
`uli-lite-gba` as an ARM-mode, `.iwram`-resident realisation with a 32-bit
cached bit window instead of the 64-bit slice-rebuild `peek`; sentinel errors
instead of `Result` per symbol; unchecked pointers with documented bounds.
Keep the core as the correctness reference and extend the host twins.

Exit gate: the measured full-frame worst is **≤ 2× the 262,144-cycle period**
(the point at which a 1× product path is a codegen question rather than a
rewrite), with unchanged pins. Record the `rest` number explicitly; at PR A it
is 837,092 cycles/frame and is what must move. If that gate is missed, stop and
re-plan rather than starting Phase 1d.

### Phase 1d — close the gap (PR C, scoped by 1c-2)

The reference asm is `gba-audio/src/uli/gba/uli_imdct_arm.S` (author’s
permission required). PR A already bounds the kernel prize: 243,619
cycles/channel in ARM/IWRAM Rust against ~67,300 in the reference, so a perfect
port saves at most ~176,000/channel — ~352,000/frame — while the non-synthesis
path is 837,092. The asm milestone is therefore only worth opening once PR B
has made the non-synthesis path small; if PR B’s ARM/IWRAM kernels land close
enough to the reference, PR C may be a narrow OLA-only port (116,061 vs 15,300
is the outlier) rather than four kernels, plus tuning and block moves.

### Phase 2 — product playback

**Gated on U4.** At PR A the product kernel set is 698% worst / 505% mean, so a
feeder would underrun continuously; this phase starts only after PR B/C bring
the full frame inside the budget with margin.

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
| **U1 — reference** | `uli-lite` remains a bit-exact, host-testable reference and oracle (no longer a portable-product claim). | `make uli-test`; committed corpus; thumbv4t `check` |
| **U2 — bit-exact host** | The Rust core matches the C integer reference frame-for-frame on the committed corpus and rejects malformed frames without panic. | `make uli-test`; pinned hashes |
| **U3 — bit-exact product** | The product kernel set (`ArmKernels`) decodes the corpus to the pinned int8/int16 frames, in the host full-frame twin and on the target. **Met (PR A).** | `make uli-bench` (both arms + on-target twin); `make uli-overlay-test` (host kernel + decoder twins) |
| **U4 — real-time decode** | Worst decode frame fits the product budget at WAITCNT 4/2; the gate total (decode + convert + IRQ + DMA) is under the agreed ceiling. **Not met (PR A, 2026-11-14): full frame through `ArmKernels` is 1,829,247 worst = 698% (1,322,800 mean = 505%); the same-image core control is 1021% / 828%. PR B gate ≤200%, PR C target ≤100%.** | `make uli-bench`; mGBA, then hardware |
| **U5 — product playback** | 32,768 Hz product path runs with zero underruns and a clean period audit. | `product-play` equivalent for ULI2; hardware runs |
| **U6 — memory** | IWRAM/EWRAM/stack stay inside the product budget with the UI and feeder linked. | linker map + runtime stack check |
| **U7 — licensing/spec** | A public crate, if published, has a permissive license and a standalone format specification; otherwise it remains `publish = false`. | license file + `uli-docs/format.md` |

The gate ceiling for U4 is not copied from `gba-audio`’s 75%: this tree’s UI,
feeder and animation budget decide it. Start from the measured 73.63% gate
total, add this tree’s non-audio cost, and write the number down before the
first real-time claim.

What 1b measured is the **decode wall**, not the gate total, and it is far
outside any ceiling: 1133% worst with the core kernels, 810% projected with
the IWRAM ARM overlay. The 73.63% inheritance question is therefore not open
yet — the first issue is reaching 100% at all. PR A measured the product
kernel set at 698%, so PR B/PR C own that; U5/U6 stay untouched until it
lands.

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
5. **CPU inheritance — measured, and the gap is codegen.** 73.63% is the
   `gba-audio` gate total on its C/ARM build, not this tree’s. Phase 1b
   measured the portable Rust decoder at **1133% worst** (ARM-mode kernels,
   code in ROM); **PR A measured the product kernel set at 698% worst / 505%
   mean**, with the same-image core control at 1021% / 828%. The dominant
   losses are Thumb-1’s software 64-bit multiply (44 `__aeabi_lmul` sites
   before the fix), cartridge instruction fetch at 4/2/no-prefetch (2.5x for
   moving the same code to IWRAM), and the entropy/bit-reader path, which is
   still Thumb/ROM and is now **837,092 cycles/frame** of the PR A product
   frame. Hand-written asm for four kernels cannot close a deficit that is
   mostly outside those kernels; PR B is the entropy hot path. The reference’s
   73.63% must not be read as a transferable number.
6. **Encoder packaging and independence.** A decoder-only crate is useful
   internally, but external users need an encoder. The current C encoder is
   reused from `gba-audio` and must eventually be replaced by a Rust encoder.
   General Python tooling and `ffmpeg` are not the problem; the C tooling and
   source dependency are. Until the C tooling is reimplemented, the project is
   not fully isolated and ULI2 is not yet the public-reuse answer.
7. **Rate flexibility.** ULI2 is fixed at 32,768 Hz. Goal #5 (higher rates such
   as 65,536 Hz) would require a format/encoder change. Do not block the
   current product on that; record it as future work.
8. **Output-stage fairness.** Compare ULI2 against FLAC/ULC through the same
   8-bit quantiser, or preserve ULI2’s fused rounding and account for the
   difference. Half an LSB matters at this noise floor.

---

## Decision log

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
