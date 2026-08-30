# flac-lite

Minimal FLAC frame decoder for GBA: `#![no_std]`, **zero dependencies**, **zero
allocation** on the decode path. Scaffold only — every function body is `todo!()`.
Decision record and rationale: [`../../FLAC.md`](../../FLAC.md).

```
crates/flac-lite/
├── src/lib.rs        crate docs, Error/Result, profile constants
├── src/bits.rs       bit reader + CRC-8/CRC-16
├── src/format.rs     GAFP manifest parse, blocksize/sample-rate/channel tables, profile
├── src/frame.rs      frame header + one-frame decode driver
├── src/subframe.rs   CONSTANT / VERBATIM / FIXED / LPC + predictor state
├── src/residual.rs   partitioned Rice / Rice2 / escape
├── src/stereo.rs     mid-side, left-side, right-side decorrelation
├── src/decoder.rs    top-level cursor API (what a ROM actually calls)
└── tests/README.md   fixture plan (no fixtures yet)
```

## Build gates

Two gates, run from this directory. **Both need explicit flags — this is not
optional verbosity**, see the config-leak note below.

```sh
# GATE 1 — the GBA target. The real gate: the build symphonia could never pass.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc

# GATE 2 — host tests (correctness: bit-exactness vs reference `flac`).
HOST=$(rustc -vV | sed -n 's|host: ||p')
cargo +nightly test -Zbuild-std= --target "$HOST"
```

### ⚠️ The `.cargo/config.toml` leak (verified 2026-08-30)

Cargo discovers `.cargo/config.toml` by **walking up parent directories**, and
merges every layer it finds. A `[workspace]` boundary does **not** stop this. So
this crate inherits the repo root's GBA settings, and both inherited values break
the host build:

| Command | Failure |
|---|---|
| `cargo test` (no flags) | `error[E0463]: can't find crate for 'test'` — inherited `[build] target = thumbv4t-none-eabi` sends the host test build to the GBA target |
| `cargo test --target <host>` | `error[E0152]: duplicate lang item in crate core: 'sized'` — inherited `[unstable] build-std = ["core","alloc"]` recompiles `core` from source, which collides with the host's prebuilt `core` underneath `std` |
| adding a local `[unstable] build-std = []` | still `E0152` — cargo **merges arrays across config layers**, so the parent's `["core","alloc"]` survives an empty local override |
| `cargo test -Zbuild-std= --target <host>` | ✅ — CLI overrides beat the file merge; `-Zbuild-std=` (empty) disables the recompile, `--target` defeats the default |

So **both** overrides are required, and neither alone is enough. Alternative that
sidesteps the whole problem: run from *outside* the repo subtree, where no parent
config exists —

```sh
cd /tmp && cargo +nightly test \
  --manifest-path "$PWD/crates/flac-lite/Cargo.toml"
```

This needs no flags, and it is also what extracting this crate into its own repo
(per README.md's library-split plan) would give us permanently.

### Why `-Zbuild-std` appears at all (it is not "using std")

`thumbv4t-none-eabi` is Tier-3 and rustup ships **no prebuilt `core`/`alloc`** for
it. A `#![no_std]` build still requires `core`, so it must be compiled from source —
that is the entire job of `-Zbuild-std=core,alloc`, and the reason nightly +
`rust-src` are mandatory here (see the root README: `E0463: can't find crate for
core`). The flag is only *named* after `std`; the values we pass are `core,alloc`.
**`std` must never appear in any build-std list in this project** — an earlier
revision of this README suggested `-Zbuild-std=core,alloc,std` for host tests, and
that was wrong (it was papering over the leak above). Gate 2 disables build-std
instead of extending it.

Zero dependencies is not cosmetic. On this target any dependency that references
`std` breaks the build — precisely the symphonia failure documented in FLAC.md.
Adding a dependency requires re-verifying gate #1.

## GAFP container format (contract)

`GAFP` = **G**BA **F**LAC **P**ack. One blob per track, `include_bytes!`-able as a
`&'static [u8]`, produced offline by `scripts/pack_flac.sh`. All integers
**little-endian** (GBA-native; the manifest is our header, not FLAC's).

```
offset  size   field
------  -----  -------------------------------------------------------------
0x00    4      magic  "GAFP"
0x04    1      version (0x01)
0x05    1      channels (1 | 2)
0x06    1      bits per sample (16; 8 accepted for experiments)
0x07    1      max fixed predictor order used (0..4)
0x08    2      block size, samples per subframe (1024 | 2048)
0x0A    4      sample rate, Hz (32000 initially; 65536 later)
0x0E    8      total samples in track (u64)
0x16    2      frame count N
0x18    4      byte offset of frame data (header + table end)
0x1C    4*N    frame offset table: u32 offsets, relative to blob start, ascending
        —      raw FLAC frames, concatenated, in order
```

Deliberately **absent**, versus a standard `.flac`:

- No `fLaC` magic, no metadata-block framing, no VORBIS/PADDING/SEEKTABLE.
- STREAMINFO is reduced to the ten fields above (the ones a decoder needs).
- The frame offset table replaces `Seek`/seektable: `seek_frame(i)` is
  `offsets[i]`, an array lookup with no I/O trait involved. That single property
  is what makes a `no_std` decoder tractable here.
- Frame data is verbatim FLAC frames as emitted by the encoder — the decoder
  parses FLAC, not a custom codec. Frame bytes for frame `i` are
  `[offsets[i] .. offsets[i+1]]` (last frame runs to end of blob), so the decoder
  learns each frame's exact extent instead of sniffing for the next sync code.

### Encode profile (what the packer guarantees)

The decoder only needs to handle what our encoder is allowed to produce:

- 16-bit PCM, 32 kHz (65,536 Hz later), mono or stereo
- block size fixed per track: 1024 or 2048 samples
- fixed block size (no blocking strategy / variable frames)
- predictors capped at **FIXED order 0–4** — encode with `-l 4`; full LPC stays
  *parsed* but rejected under `strict-profile`, pending the perf spike
- partitioned Rice / Rice2 residuals; escaped partitions supported
- stereo: independent **and** mid/side (`-m`); left-side, right-side
- no metadata blocks other than what the manifest replaces

Reference encode command (the packer will run this):

```sh
flac -1 -f -l 4 -b 2048 -m --force-utf8-legacy-noop input.wav
```

## Design rules that must survive implementation

1. **No `alloc` in the decode path.** Scratch is caller-provided and borrowed;
   the manifest is borrowed. That is what lets the crate stay `#![forbid(unsafe_code)]`.
2. **No I/O traits.** `&[u8]` + cursor, forward only.
3. **No division, ever** — ARM7TDMI has no hardware divide. Shifts and adds only.
4. **Fixed-size predictor state** (`[i32; MAX_LPC_ORDER]`), never a growable buffer.
5. **Per-frame scheduling granularity.** One `decode_next` call = one frame ≈ 64 ms
   of audio at 2048/32 kHz, so the caller can pin decode work to DMA IRQ boundaries.

## Memory

Per frame: two channel buffers of `blocksize × i32`. At 2048 samples that is
2048 × 2 × 4 B = **16 KB** scratch, plus ≤ 32 warm-up samples per subframe, plus
the DMA double buffer. Fits EWRAM (256 KB) with room to spare; `i32` intermediates
can be revisited for `i16` later if we need the space.

## Next steps

See FLAC.md → "Next steps". `bits::BitReader` first: smallest unit that can be
tested against hand-computed bit patterns.
