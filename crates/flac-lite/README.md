# flac-lite

Minimal FLAC frame decoder: `#![no_std]`, **zero dependencies**, **zero
allocation** on the decode path — portable pure Rust. The GBA
(thumbv4t-none-eabi) is the founding platform, not the boundary: the decode
path imports nothing platform-specific, and the crate's sole unsafe surface is
placement attributes on immutable run-length tables, behind
`all(target_arch = "arm", feature = "iwram-pin")` — off by default, so host
builds and ARM targets without a `.iwram` link-script definition compile with zero
unsafe and zero pin surface. **How many such attributes exist depends on the
configuration**: a default build carries exactly one (`bits::LEADING_ONES`), a
`rice-hybrid` build carries four — the same attribute kind on the hybrid kernel's
three hot tables, added by split-ladder item 5 PR 3 and recorded in
[`../../flac-docs/architecture.md`](../../flac-docs/architecture.md) and
[`../../flac-docs/measurements.md`](../../flac-docs/measurements.md) rather than
assumed ratified silently. GBA ROM crates opt in to
the pin (`flac_spike` / `flac_integration` do). The GAFP container and
encode-profile sections below are product-side container work — a manifest is
*one* frame source, not the decoder's contract — and the build-gate/config-leak
notes are this repo's build setup. A second bare-metal target is a build
witness, not an aspiration: `make flac-portable-check` compiles this crate for
`thumbv7em-none-eabi` alongside thumbv4t + host (portability fact, no perf
claim — landed 2026-11-02).
([flac-docs/architecture.md](../../flac-docs/architecture.md)).

**Arithmetic width (item 6a-6, default):** `i32-integrator` is on — the ring
sites run in `i32`, the LPC dot product is guarded per subframe by
`subframe::lpc_accumulator_fits_i32` (`Σ|c_j|·2^(b−1) ≤ i32::MAX`), and a frame
whose bound fails takes the i64 leg (`ArithmeticWidth` reports it). The
feature-off build is the audited i64 reference; `make flac-test-wide` is its gate.

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
cannot drift silently — see [flac-docs/architecture.md](../../flac-docs/architecture.md)
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
until a product path opts in (6b-1 did; the cost stayed measured). Measured, not
argued: with `scan` off,
the instrument's `.text`, `.rodata`, `.iwram` and `.ewram` are byte for byte
the 4a image; with it on, `.text` is +3,120 B and the image contains no walk
symbol — a second consumer of the decode path changes what the inliner does to
the code already linked. See the module docs and
[`../../flac-docs/measurements.md`](../../flac-docs/measurements.md).

**And the second Rice mechanism, `rice_hybrid` (landed 2026-11-06, split-ladder
item 5 PR 3)** — behind the `rice-hybrid` feature, off by default, gate
`make flac-test-rice`. `RiceKernel` (PR 2) has one implementor that the decode
path uses (`TableCountKernel`, today's loop) and one that it does not:
`HybridRiceKernel`, the pure-Rust form of the three-tier codeword walk the E10
probe wrote in ARM assembly (CTZ8 tier 1 / De Bruijn tier 2 / word-skip tier 3 /
REV8 refills), owning the bit window across a whole partition through
`BitReader::kernel_window` — the first caller of that bridge. **Decoding is
unchanged in every configuration**: the one line that names the default still
names `TableCountKernel`, and a test asserts it. Whether to flip was gated on an
on-target cycle reading this host could not take when PR 3 landed; **PR 4 took it
(2026-11-06) and it declined the flip** — the hybrid runs **+42.6 / +42.7 cycles
per decoded sample slower** than the incumbent on the same instrument (+9.71% FIXED
/ +2.64% LPC mean per frame, 314/314 frames slower, 0 inside the ~9–13 cyc/frame
layout-noise band), with its tables pinned in `.iwram` at the very slots the
incumbent's occupy, and both images producing bit-identical PCM. `flac-docs/measurements.md`, item 5
PR 4, carries the table and the reading. So the module stays exactly where PR 3 put
it — the portable alternative, and the thing the ARM realization supersedes —
because the algorithm's win belongs to the ARM realization (E10: −349/−348
cyc/sample), not to a portable Rust form of it.
Why a feature rather than a module: measured on the same instrument, with
`rice-hybrid` off the ROM-links image is **byte-for-byte the pre-PR-3 image**
(`.text` 46,008 B, same sha, `nm` identical); with it on `.text` is **+2,984 B**
and **no hybrid symbol appears in the image at all**. The growth is attributed
symbol by symbol to the decode path's own functions plus one new `read_bits`
specialisation, and probe arms settle the trigger: an `impl` block whose body is
empty costs nothing, an `impl` block whose body reaches the walk costs the whole
amount even though nothing ever calls the kernel. That is the same
second-instantiator effect 4b measured for `scan`, one rung closer to the
decoder, and it is why the twin's equivalence witness (a four-way differential
over real libFLAC partitions, `tests/rice_two_mechanism_differential.rs`) is the
thing that carries this PR rather than a perf claim: this PR makes none.
That
`todo!()` list closed on 2026-11-10 (item 6b-1): the manifest-driven `decoder`
facade and `format::Manifest` both landed, the facade is a shell over the walk
rather than a second loop, and the walk's measured cost is what keeps it a
feature. `format::Blocksize::samples` and `EncodeProfile::validate` landed with
them. Item 6b-2's second half (2026-11-11) then landed `stream::SeekTable`, the
fixed-block number line (`FixedBlockNumbering`) and `Frames::anchored(n)`. None
of that is on the GBA product path: the ROM reads a manifest and has no random
access to give.

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
rather than the stream's declared channel count. What is deliberately *not* here:
`SEEKTABLE` reading and frame-number arithmetic — those live in
`stream::SeekTable` and `stream::FixedBlockNumbering`; this walk consumes a
point's offset and frame number when a caller anchors with one. Also not here: an
end-of-stream claim based on declared counts, or a second per-frame loop (the
manifest facade is a shell over this walk).
Roadmap: [`../../flac-docs/plan-of-record.md`](../../flac-docs/plan-of-record.md).

```
crates/flac-lite/
├── src/lib.rs        crate docs, Error/Result, profile constants
├── src/bits.rs       bit reader + CRC-8/CRC-16
├── src/format.rs     GAFP manifest parse, blocksize/sample-rate/channel tables, profile
├── src/stream.rs     fLaC signature, §8.1 block chain, §8.2 STREAMINFO, and
│                     where the frame region begins (`frame_data_offset`)
├── src/frame.rs      frame header + one-frame decode driver
├── src/subframe.rs   CONSTANT / VERBATIM / FIXED / LPC + predictor state
├── src/residual.rs   partitioned Rice / Rice2 / escape (+ the `RiceKernel` seam)
├── src/rice_hybrid.rs the second Rice mechanism: three-tier codeword walk +
│                     CTZ8/REV8/BRUIJN tables, owning the window across a
│                     partition — behind `feature = "rice-hybrid"`, off by
│                     default and not the default mechanism in any configuration
├── src/stereo.rs     mid-side, left-side, right-side decorrelation
├── src/driver.rs     per-frame loop over a borrowed frame index (copied, not
│                     shared, from the perf spike's proven driver)
├── src/scan.rs       frame walk with no index: a frame ends itself, the bytes
│                     end the stream (`Frames`, `StreamBounds`, `resync`) —
│                     behind `feature = "scan"`, off by default in this crate
│                     because the walk's `.text` cost is measured; the item 6b-1
│                     product ROM opts in, and so does the facade below
├── src/decoder.rs    manifest-shaped facade: a shell over `scan::Frames`, one
│                     loop (exists only where `scan` does)
└── tests/README.md   committed libFLAC vector tables + layout/decode suites
```

## Build gates

The gates, run from this directory (or `make flac-test` / `make flac-test-wide`
/ `make flac-test-scan` / `make flac-test-crc` / `make flac-test-rice` /
`make flac-gates` / `make flac-portable-check` / `make gafp-check` /
`make gafp-pack-check` from the repo root; the root
Makefile is the keeper).
**All need explicit flags — this is not optional verbosity**, see the config-leak
note below.

```sh
# GATE 1 — the GBA target. The real gate: the build symphonia could never pass.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc

# GATE 2 — host tests (correctness: bit-exactness vs reference `flac`).
# Default features: the frame walk (item 4b) and the second Rice mechanism
# (item 5 PR 3) are NOT compiled, and neither witness suite is built — see
# GATE 4 and GATE 5.
HOST=$(rustc -vV | sed -n 's|host: ||p')
cargo +nightly test -Zbuild-std= --target "$HOST"

# GATE 3 — second bare-metal target (portability witness, no perf claim).
# Pin OFF (compiles no `.iwram` reference) and the gated modules ON, so that a
# feature a GBA ROM leaves off is still compiled for a second target instead of
# being silently un-covered. Verified container-portable: builds with NO prebuilt
# rust-std for the triple (core/alloc come from rust-src via -Zbuild-std, same
# as gate 1).
cargo +nightly check --release --target thumbv7em-none-eabi -Zbuild-std=core,alloc \
  --features scan,rice-hybrid

# GATE 4 — the frame walk with its feature on (deliberately NOT part of
# `check-gba-all`: the thing that must not carry the walk yet is the ROM).
# Thumbv4t compile, then the walk's own witness suite. `tests/scan_frame_walk_layout.rs`
# is `required-features = ["scan"]`, so without the feature it is not built at
# all rather than quietly passing.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc \
  --features scan
cargo +nightly test -Zbuild-std= --target "$HOST" --features scan

# GATE 5 — the second Rice mechanism with its feature on (`make flac-test-rice`;
# same reason as GATE 4, plus the measured one: with `rice-hybrid` off the
# ROM-links image is byte-identical to the pre-PR-3 image, with it on it is
# +2,984 B with no hybrid symbol in it — `../../flac-docs/measurements.md`, split-ladder item 5
# PR 3). `tests/rice_two_mechanism_differential.rs` is
# `required-features = ["rice-hybrid"]`.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc \
  --features rice-hybrid
cargo +nightly test -Zbuild-std= --target "$HOST" --features rice-hybrid

# GATE 6 — the i64 reference build (`make flac-test-wide`): `--no-default-features`
# turns `i32-integrator` off and runs the whole suite against the same external
# oracle. `tests/width_two_mechanism_differential.rs` is
# `required-features = ["i32-integrator"]`, so it is not built here — the two
# configurations are the differential, not one suite.
cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc \
  --no-default-features
cargo +nightly test -Zbuild-std= --target "$HOST" --no-default-features

# GATE 7 — the packed container, from the packer's own side (`make gafp-pack-check`,
# item 6b-2). Unlike every gate above, this one *runs* libFLAC: it re-synthesizes
# and re-encodes the reference clip, packs it with `scripts/gafp_pack.py`, and
# requires the result to be byte-identical to the committed product blob
# (`examples/flac_integration/assets/spike_l0.gfp`, the blob `gafp-check` guards
# and `flac-rom` embeds) — a comparison, not a write. Then it re-runs the crate's
# own fixture generator in `--check` mode (`scripts/gen_gafp_fixtures.py`, which
# writes `tests/gfp/*.gfp` and `tests/gafp_asset_vectors.txt`) including the three
# pack-time refusal legs, and finally runs `tests/gafp_asset_facts.rs`, which is
# `required-features = ["scan"]`, over those bytes through `Manifest`,
# `scan::Frames` and `Decoder`. Nothing in it is generated by the crate.
cd ../.. && make gafp-pack-check
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
`std` breaks the build — precisely the symphonia failure documented in flac-docs/architecture.md.
Adding a dependency requires re-verifying gate #1.

## GAFP container format (contract — product-side container work)

`GAFP` = **G**BA **F**LAC **P**ack. One blob per track, `include_bytes!`-able as a
`&'static [u8]`, written offline by `scripts/gafp_pack.py` (the real writer, item
6b-2, 2026-11-11); `scripts/pack_flac.sh` is still the stub whose six-step plan
that script implements. Until 6b-2 the committed product blob came from the 6b-1
stand-in generator, and the two agree **byte for byte** on that blob — which is
what keeps this from being a format with two definitions. All integers
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
0x0A    4      sample rate, Hz (32768 — the device rate; the encode profile
                      below refuses a manifest declaring any other)
0x0E    8      total samples in track (u64)
0x16    2      frame count N
0x18    4      byte offset of frame data (header + table end)
0x1C    4*N    frame offset table: u32 offsets, relative to blob start, ascending
        —      raw FLAC frames, concatenated, in order
```

Two of those fields are **records, not guards**, and item 6b-2 measured why that
matters (2026-11-11):

- `bits per sample` is validated by the *parser* as `8 | 16`, while the *encode
  profile* allows `[16]`. Those are different checks on purpose: the container
  states what it is, the profile states what may be packed. Today the packer
  refuses an 8-bit encode — and the half of that refusal worth knowing is that a
  `-l 0` 8-bit stream lands on **VERBATIM** subframes, a predictor kind this
  target has never measured. A new depth is not a header change; it is an
  unmeasured cost.
- `max fixed predictor order used` is the maximum over the FIXED subframes the
  encode actually emitted, derived from the packer's frame walk — never from the
  `-l` flag or from STREAMINFO. An all-LPC stream reads `0` there, correctly. The
  crate's own fixture set makes the consequence concrete
  (`tests/gfp/x1_lpc_stereo_32768_b2048.gfp`, described in `tests/README.md`): that
  all-LPC asset and two FIXED-0 assets that decode to the *same PCM* all carry
  `0`, and `EncodeProfile::validate` accepts all three. C2's real guard is the
  packer's subframe census plus the measured cost of the kinds it finds, not this
  byte.

The **`sample rate`** field is the one place where the container's statement and
the profile's agreement have to be the same check, and plan item 7-0 (Q14, ruled
2026-11-11: a stream is encoded at the rate it is played at) is what put it there.
`EncodeProfile::allowed_sample_rates_hz` names exactly one rate —
`PRODUCT_SAMPLE_RATE_HZ`, 32,768 Hz, the clock `examples/flac_integration/src/
geometry.rs` drives the mixer at, with a host test asserting the two stay equal —
and `validate` asks it **first**, before block size or depth or order: a stream at
a rate this build does not play at has no frame period for any other clause's
budget to be stated against.

Why a header check and not a runtime one, measured in `src/format.rs`'s own tests
and in `crates/flac-lite/tests/gafp_asset_facts.rs`: respell a shipped product
blob's rate field from 32,768 to 32,000 Hz — a completely legal FLAC stream rate,
the table's `0b1000` — and the manifest still parses, the decoder still runs, and
the first frame decodes to **identical samples**. The frames of a 32,768 Hz encode
carry their own rate (`0b1101` plus a 16-bit Hz value, never the table code), and
the samples are the samples whatever the header claims. What changed is what the
sample axis *means*: played against a 32,768 Hz timer, a stream that declares
32,000 Hz runs 1.024x fast, **41.06 cents sharp**. No digest in this project — PCM
FNV, FIFO FNV, the played bytes on hardware — can tell those two blobs apart, so
the refusal belongs in the profile, which is the one object every path that
validates a manifest reads (`Manifest::profile`, `Decoder::new` under
`strict-profile`, the product ROM's start-up check). Note what it is **not**: not a
lookup and not a cost key. Cost is keyed structurally in `scripts/gafp_pack.py`'
`MEASURED_DECODE`, by **block size and predictor kind/order**; the residual envelope
a row was measured on travels as a stated caveat on that row rather than as a key
component, and the rate is a validity check asked once. One provenance point belongs
here because 7-0 is what makes it live: the only measured row for the product's shape
(`fixed0` at 2048-sample frames, 42.48% mean / **42.72% worst** of a 1,048,576-cycle
frame period, on hardware) was measured on frames an earlier encode produced, not on
the bytes this asset is — and because the table is keyed by shape, the new asset packs
**cleanly** against it, with no refusal and no `--allow-unmeasured` flag. That is the
cost model behaving as designed and it is also the failure mode the re-keying brings,
so the packer says it out loud on every pack today:

```
gafp_pack: warning: the measured decode figures this budget is stated against were
           measured on different frames than the ones being packed. … the region
           being packed is not the region those cycles were measured on, and no
           instrument in this repository has ever decoded these particular bytes on
           hardware. The match is structural, which is the point of re-keying the
           table: cost does not care what the stream calls itself.
```

A row measured on the frames this asset is made of is plan item 7-1's.

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

- 16-bit PCM at **the device's own rate, 32,768 Hz** — one value, named once in
  the profile (`PRODUCT_SAMPLE_RATE_HZ`). Before Q14 was ruled this line read
  "32 kHz (65,536 Hz later)"; plan item 7-0 made the rate a profile clause with a
  single admissible value rather than a target to grow into, because the mixer has
  one clock (see the rate note above). Mono or stereo.
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
  trips the assert") is the bug that closed it — see flac-docs/format-notes.md → "Header width, coded numbers, and stream layer".
- **max predictor order ≤ 4** — encode with `-l 4`; higher orders stay *parsed*
  but rejected under `strict-profile`, pending the perf spike.
  ⚠️ Corrected 2026-09-07: this previously said "predictors capped at **FIXED
  order 0–4**", which misreads `-l`. `-l, --max-lpc-order` is a maximum **LPC**
  order; `-l 4` emits real LPC-4 subframes (measured: `lpc4` ×156/157 frames), and
  FIXED-only is `-l 0`. The `strict-profile` switch is therefore "order > 4",
  not "is LPC". See flac-docs/format-notes.md → "Correction 3".
  ⚠️ Corrected 2026-11-11: **`-l 0` is not FIXED-0.** It disables LPC; libFLAC
  still chooses FIXED orders 1–4 from the signal, and only FIXED-0 has a measured
  row in `scripts/gafp_pack.py::MEASURED_DECODE`. The packer's product guard is
  therefore FIXED-0-only in practice and refuses FIXED-1..4 (and CONSTANT/
  VERBATIM) as unmeasured; say that, rather than calling the product profile
  "FIXED-only".
- partitioned Rice / Rice2 residuals; escaped partitions supported
- stereo: independent **and** mid/side (`-m`); left-side, right-side
- no metadata blocks other than what the manifest replaces

Reference encode command. Since 6b-2 the packer **does** run this shape: `-l`,
`-b` and the mid/side switch come from the profile it was given, `-s` and an
`flac -t` self-check wrap the call, a profile it knows it cannot honour is refused
before it encodes, and an off-profile *result* is refused before any `.gfp` byte
is written. Note which arm the packer picks by default: the command below is the
order-ceiling arm, while the packer's default is the **LPC-disabled** one (`-l 0`).
That is not the same as **FIXED-0**: only FIXED-0 has a measured product row, so a
`-l 0` encode that libFLAC decides to code as FIXED-1..4 is refused as unmeasured
until someone measures it. `-l 4` at this target measures 174.14% of the frame on
hardware — `--allow-lpc` is what writes an LPC asset at all, and it marks the
result NON-PRODUCT:

```sh
flac -1 -f -l 4 -b 2048 -m input.wav
```

For the LPC-disabled arm of the perf gate (a materially different stream — see the
note above): `flac -1 -f -l 0 -b 2048 -m input.wav`. Its product acceptability still
depends on the resulting FIXED order being 0 and the residual shape staying inside
the measured envelope (one partition, no wasted bits, no escape partitions, Rice
parameter ≤ 10).
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
5. **Per-frame scheduling granularity.** One `decode_next` call = one frame = one
   2048-sample block = **62.5 ms** of audio at 2048 samples / 32,768 Hz — exactly
   1/16 s, and exactly 2**20 cycles of the GBA's 2**24 Hz clock — so the caller can
   pin decode work to DMA IRQ boundaries.

## Memory

Per frame: two channel buffers of `blocksize × i32`. At 2048 samples that is
2048 × 2 × 4 B = **16 KB** scratch, plus ≤ 32 warm-up samples per subframe, plus
the DMA double buffer. Fits EWRAM (256 KB) with room to spare; `i32` intermediates
can be revisited for `i16` later if we need the space.

## Next steps

The roadmap is a single list in
**[flac-docs/plan-of-record.md](../../flac-docs/plan-of-record.md)** (the only
source of truth for outstanding work). Phase 1's decode path through
`frame::decode_frame` is complete (`src/lib.rs` carries the per-module state),
the frame loop landed as `driver` on 2026-11-04, the stream layer landed the same
day (`stream::parse` → `StreamFacts` + `frame_data_offset`), the table-less
`scan` walk is in behind its feature, the `decoder` facade and `format::Manifest`
landed with item 6b-1 on 2026-11-10 (the facade is a shell over the walk; there
is no third loop), and the real packer landed with 6b-2's first half on
2026-11-11, followed by the seek table, fixed-block number line and anchored
walk. Outstanding in this crate, from that plan: the now-safe retirement of the
stand-in generator, the fixture axes
`tests/README.md` names, and the crate-wide `dead_code` allow probed on
2026-11-11 — five lib-side items it currently silences, plus one unused variable
in `tests/scan_frame_walk_layout.rs` that it has *never* silenced (an integration
test is its own crate, so a crate-level allow never reached `tests/`). The full
list is in `src/lib.rs`. The product-side remainder — 32,768 Hz rate-play
through a real manifest, and playback — is item 6b-3 and item 7. Architecture and the delivered
portability ladder: [flac-docs/architecture.md](../../flac-docs/architecture.md).
