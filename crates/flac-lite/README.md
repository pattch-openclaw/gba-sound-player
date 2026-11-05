# flac-lite

Minimal FLAC frame decoder: `#![no_std]`, **zero dependencies**, **zero
allocation** on the decode path — portable pure Rust. The GBA
(thumbv4t-none-eabi) is the founding platform, not the boundary: the decode
path imports nothing platform-specific, and the crate's sole unsafe surface is
one placement attribute behind `all(target_arch = "arm", feature = "iwram-pin")`
— off by default, so host builds and ARM targets without a `.iwram` link-script
definition compile with zero unsafe and zero pin surface. GBA ROM crates opt in
to the pin (`flac_spike` / `flac_integration` do). The GAFP container and
encode-profile sections below are product-side container work — a manifest is
*one* frame source, not the decoder's contract — and the build-gate/config-leak
notes are this repo's build setup. A second bare-metal target is a build
witness, not an aspiration: `make flac-portable-check` compiles this crate for
`thumbv7em-none-eabi` alongside thumbv4t + host (portability fact, no perf
claim — landed 2026-11-02).
([FLAC.md → split proposal](../../FLAC.md#flac-lite-and-flac-lite-gba--portable-core-split-proposal-2026-11-01)).

Implemented: the full Phase 1 decode path through `frame::decode_frame` —
header, subframe types, partitioned Rice residual + escape, integrators,
stereo decorrelation — bit-exact against `flac -d` on the committed libFLAC
vector tables in `tests/` — **plus the per-frame loop above it, `driver`
(landed 2026-11-04)**: `decode_one` decodes one frame at its frame-index
offset through a fresh cursor, `decode_all` walks a whole index through a
per-frame callback, and both take a borrowed `FrameSource` (frame index +
region + max blocksize + channel count + stream defaults — the E1 sample's
`ClipView` field set, no generated-asset metadata). It is a **narrow copy** of
the perf spike's proven driver, not a shared file, and both copies are run over
the same embedded clip bytes in the spike's own witness suite so the copy
cannot drift silently — see [FLAC.md → split
proposal](../../FLAC.md#flac-lite-and-flac-lite-gba--portable-core-split-proposal-2026-11-01)
for why the instrument could not simply move. **And the frame walk above that,
`scan` (landed the same day, item 4b)**: a frame source with no index in front
of it, where each frame's own fields tell the reader where it ends and the end
of the bytes is the end of the stream (`stream::parse`'s file-level entry point,
item 4a, is the half that says where the frames begin). Plus frame CRC
verification (Phase 2 step 5):
`bits::crc8` / `bits::crc16` are always compiled, and the `crc-check` feature
additionally verifies the header CRC-8 and footer CRC-16 inside
`FrameHeader::parse` / `decode_frame` (`HeaderCrc` / `FrameCrc`). Off by
default — the release path consumes both CRCs unchecked, the design's "checked
in debug, skippable in release"; gate: `make flac-test-crc`. The frame walk is
behind a gate of the same shape — `scan`, off by default, `make flac-test-scan`
the gate — but for the opposite reason from `crc-check`: not to skip work the
release path cannot afford, so that a GBA ROM does not link the walk at all
until item 6 rewires the facade onto it. Measured, not argued: with `scan` off,
the instrument's `.text`, `.rodata`, `.iwram` and `.ewram` are byte for byte
the 4a image; with it on, `.text` is +3,120 B and the image contains no walk
symbol — a second consumer of the decode path changes what the inliner does to
the code already linked. See the module docs and `../../FLAC.md`'s landing
note. Remaining
`todo!()`: the manifest-driven `decoder` facade — which will be a thin shell
over `driver`, not a second loop — and `format::Manifest`.

The file-level entry point landed next (split-ladder item 4a, 2026-11-04):
`stream::parse` reads a real `.flac` file's `fLaC` signature, the RFC 9639
§8.1 metadata-block chain and the §8.2 `STREAMINFO`, and returns
`StreamFacts` — validated stream defaults plus `frame_data_offset`, the offset
at which the frame layer can parse the first frame (`stream::frame_data_offset`
when that is all a caller wants). It materializes no block list: the crate is
core-only and allocation-minimal, so a caller that wants the blocks themselves
drives `stream::Blocks`. **The next rung is 4b** — a table-less frame source
and the end-of-stream rule. 4a deliberately stops at the offset and the
defaults, and `tests/fixtures/s5_unseekable_total_unknown.flac` is committed
exactly to record why the easy rule ("count frames from `total_samples`") is
not available: a real stream can declare no sample count, no frame sizes and
no MD5 and still carry ten frames. `src/lib.rs` carries the authoritative
implementation state.

**4b landed on top of it the same day** (`src/scan.rs`): `scan::Frames` walks a
frame region with no index in front of it — `Frames::stream` over a whole file
(starts at `frame_data_offset`, numbering anchored at 0, defaults and bounds
from that file's own `STREAMINFO`), `Frames::region` over any slice that begins
at a frame (a `FrameSource`-shaped slice, a hand-cut range, a blob), and
`.anchored(n)` for a range whose real frame number is known. Each `next_frame`
call parses one header, decodes it through the *same* `frame`-layer entry
`driver` uses, and advances by that frame's own length: the cursor is left on
the bit its own fields end on, one byte past the footer CRC. Hence no index, no
sample-count claim and no fixed byte stride — the fixture table's frame lengths
(`117 116 116 116 116 116 115 116 116 51`) admit none. What a frame's tail
actually is, measured per frame: **zero-padding of 0–7 bits plus the 16-bit
CRC**, recorded as a 16..23-bit gap (generation asserts the format's bound
rather than trusting it), with non-zero padding on 23 of the 35
frames here. The table then counts what a reader that treated the footer as
exactly 16 bits would be standing on: `footer_slip_positions_strict_headers 0`
over those 23 bit positions, and `footer_byte_truncation_strict_headers 0` over
the 30 positions a byte cursor that rounded a non-byte-aligned end *down* lands
on — so a mis-counted footer is refused rather than followed, loudly, though
*inside* a frame that had already decoded. One of the 23 positions is
sync-SHAPED (`footer_slip_positions_sync_shape 1`): two bytes that look like a
frame and fields that are not, which is the case the field leg of the frame
filter exists for, and the reason the strict filter is asked about these offsets
in `tests/scan_frame_walk_layout.rs` rather than trusted about them. A walk that skips a constant 16 bits and re-anchors to the next byte
boundary lands correctly (the footer ends on a byte boundary, so rounding up
absorbs the padding) — that is `frame`'s existing contract, and it works only
because the body's bit end came from walking the fields. `scan` measures lengths
the same way, which is why `bit_position() / 8` is a measurement here and not a
rounding, and why the walk reproduces the table's `frame_bytes` on every frame in
both feature builds. `.finish()` is the end-of-stream rule: the end of the
region is the only end every FLAC stream has, so the walk reads to it and only
then checks the walk against what the stream declared (`total_samples`, the
frame count 4a derives from a fixed block size) — in **that** order, sample claim
first, because it is the one that names how much audio is missing; and the
frame-count arm cannot fire on a self-consistent file at all (both numbers come
from the same arithmetic), it exists for a claim handed to a region it does not
belong to — a case only a staged test can build, and one this suite had not built
until a mutation showed the arm was untested — one code path for seekable
and unseekable streams, and the reason `s5`'s silence is never turned into a
zero. `scan::next_frame_start` is **recovery only** (`resync`): sync-shape
alone finds 1223 candidates for 979 real frames, so a search must run the whole
field-plus-CRC filter inside the derived width bounds, which is also why this
crate's depth limit applies to *locating* frames exactly as much as to decoding
them (the generator's independent filter finds all 10 of the 24-bit fixture's
frames; ours finds 0, and refuses the walk with `UnsupportedSampleSize`). Coded
frame numbers are the third strict leg — enforced **before a frame writes any
samples**. The two ways a walk can refuse a caller's buffers are kept apart by
the error type, because they ask the caller for different things:
`Error::DeclaredMaxExceedsBuffers` says the buffers are shorter than the block
size **the stream declares** — a claim about the stream, which §8.2 itself warns
may exceed every frame in the region, fixed by making the buffers fit the stream
or by handing the walk no bound at all — and `Error::BufferTooSmall` says they
are shorter than **this frame's own header** asked. Same position in the walk,
same "nothing read, nothing written"; `s2`'s second subframe is the per-frame
case, because whether a frame carries two subframes is that frame's channel code
rather than the stream's declared channel count. What is deliberately *not* here: `SEEKTABLE` interpretation (the
walker skips it by length; random access is item 6's `format::Manifest`), any
end-of-stream claim based on declared counts, and a rewired `Decoder` facade.
Roadmap: [`../../FLAC.md`](../../FLAC.md) → "Phased plan".

```
crates/flac-lite/
├── src/lib.rs        crate docs, Error/Result, profile constants
├── src/bits.rs       bit reader + CRC-8/CRC-16
├── src/format.rs     GAFP manifest parse, blocksize/sample-rate/channel tables, profile
├── src/stream.rs     fLaC signature, §8.1 block chain, §8.2 STREAMINFO, and
│                     where the frame region begins (`frame_data_offset`)
├── src/frame.rs      frame header + one-frame decode driver
├── src/subframe.rs   CONSTANT / VERBATIM / FIXED / LPC + predictor state
├── src/residual.rs   partitioned Rice / Rice2 / escape
├── src/stereo.rs     mid-side, left-side, right-side decorrelation
├── src/driver.rs     per-frame loop over a borrowed frame index (copied, not
│                     shared, from the perf spike's proven driver)
├── src/scan.rs       frame walk with no index: a frame ends itself, the bytes
│                     end the stream (`Frames`, `StreamBounds`, `resync`) —
│                     behind `feature = "scan"`, off by default until item 6
├── src/decoder.rs    manifest-shaped facade (scaffold: todo!())
└── tests/README.md   committed libFLAC vector tables + layout/decode suites
```

## Build gates

The gates, run from this directory (or `make flac-test` / `make flac-test-scan`
/ `make flac-test-crc` / `make flac-portable-check` from the repo root; the
root Makefile is the keeper). **All need explicit flags — this is not optional
verbosity**, see the config-leak note below.

```sh
# GATE 1 — the GBA target. The real gate: the build symphonia could never pass.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc

# GATE 2 — host tests (correctness: bit-exactness vs reference `flac`).
# Default features: the frame walk (item 4b) is NOT compiled and its witness
# suite is not built — see GATE 4.
HOST=$(rustc -vV | sed -n 's|host: ||p')
cargo +nightly test -Zbuild-std= --target "$HOST"

# GATE 3 — second bare-metal target (portability witness, no perf claim).
# Pin OFF (compiles no `.iwram` reference) and the walk ON, so that a feature a
# GBA ROM leaves off is still compiled for a second target instead of being
# silently un-covered. Verified container-portable: builds with NO prebuilt
# rust-std for the triple (core/alloc come from rust-src via -Zbuild-std, same
# as gate 1).
cargo +nightly check --release --target thumbv7em-none-eabi -Zbuild-std=core,alloc \
  --features scan

# GATE 4 — the frame walk with its feature on (deliberately NOT part of
# `check-gba-all`: the thing that must not carry the walk yet is the ROM).
# Thumbv4t compile, then the walk's own witness suite. `tests/scan_frame_walk_layout.rs`
# is `required-features = ["scan"]`, so without the feature it is not built at
# all rather than quietly passing.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc \
  --features scan
cargo +nightly test -Zbuild-std= --target "$HOST" --features scan
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

## GAFP container format (contract — product-side container work)

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
- **frame headers inside 6–9 bytes (48–72 bits)** — that is the range the
  reference command below produces, i.e. a *packer* profile fact. It is not a
  decoder limit, and since 2026-11-04 it is no longer what `FrameHeader::parse`
  asserts: the assert was an encoder observation masquerading as a bound, so it
  has been replaced by derived bounds — `frame::MIN_HEADER_BITS` (48) to
  `frame::MAX_ACCEPTED_HEADER_BITS` (120; the format's widest *spelling* is
  `MAX_HEADER_BITS` = 128, and a 7-octet coded number cannot occur in a
  fixed-blocksize stream — §9.1.5 caps a frame number at 31 bits / 6 bytes).
  Witnessed on real encoder bytes: `tests/fixtures/s3_wide_48001hz_b3000.flac`
  is a valid stream whose **first frame header is 80 bits**, and parsing it is a
  test. No width `Error` was added, and the reasoning is in `frame.rs`: every
  field value that could exceed the ceiling is already refused for its own
  reason (number cap, variable blocking, non-minimal number form). The earlier
  follow-up text ("a plain `flac -b 16` encode reaches an 80-bit header that
  trips the assert") is the bug that closed it — see FLAC.md → item 4a entry.
- **max predictor order ≤ 4** — encode with `-l 4`; higher orders stay *parsed*
  but rejected under `strict-profile`, pending the perf spike.
  ⚠️ Corrected 2026-09-07: this previously said "predictors capped at **FIXED
  order 0–4**", which misreads `-l`. `-l, --max-lpc-order` is a maximum **LPC**
  order; `-l 4` emits real LPC-4 subframes (measured: `lpc4` ×156/157 frames), and
  FIXED-only is `-l 0`. The `strict-profile` switch is therefore "order > 4",
  not "is LPC". See FLAC.md → "Correction 3".
- partitioned Rice / Rice2 residuals; escaped partitions supported
- stereo: independent **and** mid/side (`-m`); left-side, right-side
- no metadata blocks other than what the manifest replaces

Reference encode command (the packer will run this):

```sh
flac -1 -f -l 4 -b 2048 -m input.wav
```

For the FIXED-only arm of the perf gate (a materially different stream — see the
note above): `flac -1 -f -l 0 -b 2048 -m input.wav`.
For 65,536 Hz: add `--lax` (that rate is outside FLAC's streamable subset, so
every frame then carries sample-rate code `0b0000` = "from stream", which the
decoder resolves via `frame::StreamDefaults`).

An earlier revision of this command ended in `--force-utf8-legacy-noop`. That is
**not a libFLAC option** — 1.5.0 exits 1 with `unrecognized option` (verified by
running it). Frame numbers are UTF-8-coded by spec, not by flag; nothing here
needs forcing.

## Design rules that must survive implementation

1. **No `alloc` in the decode path.** Scratch is caller-provided and borrowed;
   the manifest is borrowed. That is what keeps the decode path free of unsafe
   code (`#![deny(unsafe_code)]` since 2026-10-22, one named cfg-gated pin
   attribute — see `src/lib.rs`).
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

The roadmap is a single list in **FLAC.md → "Phased plan"** (the only source of
truth for outstanding work). Phase 1's decode path through `frame::decode_frame`
is complete (`src/lib.rs` carries the per-module state), the frame loop
landed as `driver` on 2026-11-04, and the stream/metadata half of item 4
(`stream.rs`, item 4a) landed the same day: `stream::parse` → `StreamFacts` +
`frame_data_offset`. Outstanding in this ladder rung: **item 4b** — the
table-less frame source and the end-of-stream rule — plus the `decoder` facade
(a shell over `driver`) and `format::Manifest`, and the rest of the
portability ladder in
[FLAC.md → split proposal](../../FLAC.md#flac-lite-and-flac-lite-gba--portable-core-split-proposal-2026-11-01).
