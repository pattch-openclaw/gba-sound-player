# Architecture — what exists, and why it is shaped this way

This is the design file: the constraint, the decision that produced the decoder,
what the tree actually contains today, the seam between the portable core and the
GBA overlay, and the rules that must survive implementation. Measurements live in
[`measurements.md`](measurements.md); the live work plan in
[`plan-of-record.md`](plan-of-record.md).

## The constraint

Goal #4 is FLAC decoding to fit more audio on a cartridge. The hard constraint is
the target: `thumbv4t-none-eabi` is Tier-3 bare-metal, `#![no_std]`, **`core` +
`alloc` only**. Any crate that references `std` — directly or through a transitive
dependency — cannot compile for this target. `alloc` exists but the decode path
does not use it.

The sibling evaluation for the alternative codec is [`../OPUS.md`](../OPUS.md);
same constraint, same methodology.

## Why we wrote our own decoder (decision of 2026-08-30)

The mature Rust FLAC decoders are built on `std::io` abstractions, and the port is
not a dependency-tree accident: in symphonia's no-std fork the blockers are
structural — `Error::IoError(std::io::Error)` as the single error type, an `io/`
module built on `Read`/`Seek`/`BufRead`, `HashMap` in the codec registry, and
`std::sync::Arc` in `symphonia-metadata`. Waiting on upstream is not an option;
the no-std work has languished for years.

The decision was **option (c): write a minimal, `no_std`, zero-dependency,
zero-allocation FLAC frame decoder ourselves.** Rationale, still the reason the
design is small:

- We control **both ends**. Encoding is offline and ours; the "filesystem" is a
  memory-mapped fixed-layout ROM. That deletes the entire stack that makes
  symphonia unportable — no `Read`/`Seek` traits, no container probe, no runtime
  codec registry, no `HashMap`, no `IoError`.
- A decoder over `&'static [u8]` with a cursor needs none of it.
- The decode math is integer shifts/adds/multiply-accumulate — **no division**,
  which matters because ARM7TDMI has no hardware divide.

The reproduction of the symphonia failure is kept at
`examples/symphonia_flac_probe/` (own `Cargo.toml`; expected to fail compilation).
It is the evidence for the decision, not a live component. The root ROM carries no
FLAC dependency.

## Alternatives considered (surveyed 2026-08-29)

The mature Rust FLAC decoders were `symphonia` (`symphonia-bundle-flac` — the
best-engineered pure-Rust decoder; full `no_std` still upstream WIP), `clac` (MIT,
smaller surface), `ferrosintesis-flac` (minimal, patchy API), and the thin,
largely unmaintained `flac-rs`/`flac-decoder` wrappers. None was `no_std` in a
usable form at the time.

The candidate probed was the community no-std fork — keks `Symphonia`, branch
`chore/core-step-towards-nostd-0.6`, commit `8301d26` (2025-10-18), *a step
towards* no-std, not *at* it. `examples/symphonia_flac_probe/` fails before
symphonia's own code compiles: `E0463: can't find crate for std` from
`lazy_static` and `num-traits` in the dependency graph (which crate aborts first
depends on cargo's build order). The FLAC crate carries no `#![no_std]` at all,
and the hard blockers are structural: `Error::IoError(std::io::Error)` as the
single error type, an `io/` module built on `Read`/`Seek`/`BufRead`, a `HashMap`
codec registry, and `std::sync::Arc` in `symphonia-metadata` — ~27 non-trivial
`std` usages across core/common/metadata.

| Option | Cost |
|---|---|
| (a) Fork the keks branch | ~200–400-line diff, mechanical-to-moderate, but permanently tracking a WIP fork of a moving upstream. |
| (b) Port `clac` | The same porting cost as (c), against someone else's architecture, which assumes `std::io` too. |
| (c) Write a minimal decoder | ~500 lines of integer math; deletes the metadata/demuxer/seek/registry stack because we own both ends. **Chosen.** |

## What the tree contains today

```
crates/flac-lite/          portable core: no_std, zero deps, zero alloc
  bits.rs                  MSB-first reader (u32 refill accumulator), UTF-8 coded
                           numbers, wasted bits, CRC-8/CRC-16, KernelWindow bridge
  stream.rs                fLaC magic, §8.1 metadata chain, §8.2 streaminfo
  frame.rs                 frame header parse, decode_frame, derived width bounds
  subframe.rs              CONSTANT / VERBATIM / FIXED / LPC, integrators, pad_block
  residual.rs              partitioned Rice/Rice2/escape, RiceKernel seam
  stereo.rs                mid-side, left-side, right-side decorrelation
  driver.rs                per-frame loop over a borrowed frame index
  decoder.rs               the facade: `Decoder::new(&Manifest)` + `decode_next_with::<K>`;
                           a shell over `scan::Frames`, deliberately not a third loop
                           (6b-1, 2026-11-10 — the facade `todo!()`s are closed)
  format.rs                the `GAFP`/manifest layer: `Manifest` (parse, `frame_region`,
                           `blocksize`, `max_fixed_order`, stream defaults, bounds),
                           `Blocksize::samples`, `EncodeProfile::validate`,
                           `StreamInfo::from_stream` — all landed (6b-1/6b-2)
  scan.rs                  table-less frame walk + resync (feature `scan`)
  rice_hybrid.rs           portable hybrid Rice twin (feature `rice-hybrid`)

crates/flac-lite-gba/      GBA overlay: no_std, deny(unsafe_code)
  rice_twin.rs             an out-of-crate RiceKernel arm (feature `rice-twin`)
  rice_arm.rs              the ARM-mode kernel, FFI, KernelAbi, three tables
                           (feature `rice-arm`); 0 unsafe in a default build,
                           3 items with rice-arm, 6 with iwram-pin as well
  asm/rice_partition.s     the assembled body (E10's, plus the four 6a-4 deltas),
                           placed in `.iwram` rather than `.text` since 6a-5d —
                           measured 4.000 cycles/instruction in ROM vs ~1.4 in
                           IWRAM, i.e. 48.3% of the 32 kHz frame instead of 109.8%

examples/overlay-kernel-check/  the kernel-check ROM (`rice-arm` + `iwram-pin`):
                           corpus + synthesized boundaries through incumbent,
                           assembled kernel and model, on mGBA's ARM7TDMI
examples/flac_spike/       the measurement instrument (the `ctrl` arm of every E
                           record) + experiments/<name>/ reappliable samples
  src/e10.rs + asm/        6a-5b: one archived experiment re-applied as a live
                           *measurement leg* (feature `item6a5b-vendored`), with
                           its kernel symbol and local labels renamed so both
                           kernels link, and `tests/e10_witness.rs` as its gate
examples/flac_integration/ standalone ROM that links the decoder (build sanity)
examples/arm-kernels/      ARM-mode kernels + dual witness, outside the decode path
examples/symphonia_flac_probe/  the compile-failure probe (documentation)
scripts/                   vector/asset generators, pack_flac.sh (stub)
```

**Features, all off by default, and the gate that covers each.** Most defer a
measured cost or a configuration; the gate is how the configuration stays
covered. `strict-profile` is the exception — it is declared and unimplemented.

| Feature | What it changes | Gate |
|---|---|---|
| `iwram-pin` | pins the hot lookup **tables** to `.iwram` on ARM targets (the crate's only `unsafe` surface: placement attributes on immutable tables). The ARM kernel **body** is pinned to `.iwram` unconditionally and is *not* this feature's business — 6a-5d measured a ROM-resident body at 4.000 cycles/instruction against ~1.4 in IWRAM, and there is no configuration in which that is the code you want | GBA ROM builds + `nm` witness |
| `crc-check` | verifies header CRC-8 and footer CRC-16 instead of consuming them | `flac-test-crc` |
| `scan` | compiles the table-less frame walk | `flac-test-scan` |
| `rice-hybrid` | compiles the portable hybrid Rice twin | `flac-test-rice` |
| `strict-profile` | Holds a manifest to `format::DEFAULT_PROFILE` — blocksize, bits per sample, and the recorded maximum FIXED order — plus the per-frame order refusal that stays in `subframe`. **Implemented**: `EncodeProfile::validate` has a body and `Decoder::new` calls it under `cfg!(feature = "strict-profile")`. | **no gate builds the feature** — `validate()` itself is tested directly (`tests/decoder_manifest_walk.rs`, `tests/gafp_asset_facts.rs`), but the `cfg`'d call site inside `Decoder::new` has never been compiled by any gate, and the product ROM does not enable it (`flac_integration` builds `iwram-pin, scan` + `arm-kernel`). That is deliberate per 6b-2: the profile guarantee moved to pack time, where the packer *measures* the frames instead of trusting a header, and a profile-strict runtime re-check is optional insurance. If one is ever wanted, a gate leg is owed first. |
| `rice-twin` (overlay) | forwards to `flac-lite/rice-hybrid`; exposes the out-of-crate arm | `overlay-test-twin` |
| `rice-arm` (overlay) | assembles `asm/rice_partition.s`, declares the FFI and `KernelAbi`, owns the three tables — correct-only, nothing names it as a default | `overlay-test-arm` (host contract, 15 arms) + `overlay-kernel-test` (8 device arms) + `overlay-objdump` |
| `iwram-pin` (overlay) | forwards to `flac-lite/iwram-pin`; pins the overlay's three tables and the core's `LEADING_ONES` | `overlay-kernel-rom`, whose own test asserts the runtime addresses are inside `.iwram` |

**These are landed, not outstanding.** The `decoder` facade and the `format` items
above went in with 6b-1 (2026-11-10) and the packer/seek-table work with 6b-2
(2026-11-11); the crate's `todo!()` list is closed and `crates/flac-lite/README.md`
plus `src/lib.rs` carry what is genuinely still open inside it. What remains in
this area: `scripts/pack_flac.sh` is still the documented stub (its header records
what landed around it), the *spike* still builds on a generated per-frame offset
table as its stand-in, and `gen_gafp_blob.py` is still the hand-authoring generator
that `gafp-check` pins byte for byte — with `gafp_pack.py` now being the real
packer, retiring that generator is a safe but unwritten step. The product path no
longer depends on any of the stand-ins: `examples/flac_integration` reads the
manifest out of the embedded blob and walks the frames from it. The open product
work in this area is item **6b-3** — 32,768 Hz rate-play through that manifest —
not the facade.

**One experiment runs inside the tree, and it is the exception that proves the
`experiments/` rule.** 6a-5b needed the *same image* to contain both the landed ARM
realization and the probe's vendored one, which no patch applied to a scratch
checkout can give a per-frame paired reading. So E10's module, its asm body and its
witness were vendored into the spike crate behind `item6a5b-vendored` — a feature
that implies `item6a5-asm`, because a differential leg without the landed leg in the
link measures nothing. It is **not** a `RiceKernel` arm, deliberately: a leg that
goes through the seam prices the seam, and the point was to price what the seam does
not cover. Two mechanics that only show up when a second kernel is in the same link:
`global_asm!` symbols are not namespaced (`arm_rice_partition` →
`e10_rice_partition`), and a crate's `global_asm!` blocks assemble into one unit, so
`.Lloop`-style local labels collide and must be renamed too. The default build
doesn't have the module at all: it stays `forbid(unsafe_code)` outside 6a-5b
builds, and the E10 witness is `required-features`-gated so a green cannot name
nothing.

**The seam now reaches the frame.** 6a-5 threaded the kernel choice up the
caller-facing chain — `frame::decode_frame_with<K>` → `subframe::decode_subframe_with<K>`
→ `residual::decode_residual_with<K>` → the partition seam — with each non-generic
entry point a one-line delegation to `…::<TableCountKernel>` (`decode_residual`'s
line is where the default lives now; the retired private `decode_rice_partition`
held it while nothing above it took a kernel, and that function's own docs deferred
the threading to item 6). This is what lets a *product* name an overlay kernel
without the core depending on the overlay, and it is why 6a-5's reading is a
product-path reading rather than a probe.

**One thing links `flac-lite-gba` outside the instrument features: the correctness
instrument**
(`examples/overlay-kernel-check/`), and only with `rice-arm` on. No shipped decode
path, no spike build and no product crate names it. That is the item-6a shape: the
overlay's rungs exist so the build plumbing, the seam's reachability and the
kernel's correctness are settled before any cycle is read (6a-5) or any default is
moved (6a-6).

## The seam: a whole-partition `RiceKernel`

The kernels replace the per-sample codeword step inside
`residual::decode_rice_partition`, so the core exposes a small generic trait whose
default is the shipped table-count loop:

- **Whole-partition, not per-sample.** The kernel owns the byte window
  (`KernelWindow { w, v, ptr, end }`, handed out and taken back through
  `bits`), because that is the only shape that has ever produced a measured
  number. `examples/arm-kernels/src/sample_step.rs` deliberately excludes the
  rev8 reversal and the `w_{t+1}` straddle merge; stitching those in is the
  vendored seam's job.
- **Monomorphized, never `dyn`.** The E2b lesson: landing codegen coupling is
  where measured samples die.
- **Unsealed, default `TableCountKernel`.** `decode_rice_partition` names the
  default on one line; `decode_rice_partition_with::<K>` is the seam.
- **Two mechanisms in every differential.** `bits::rice_quotient` and
  `LEADING_ONES` are not deleted even when a kernel replaces them: the
  differentials consume two live mechanisms, and `LEADING_ONES @ 0x0300022c` is
  the `nm` anchor cited by E2b, E10, the `iwram-pin` witness and item 5.
- **Correctness is the core's.** The overlay owns no decode semantics; it is
  judged by the core's witnesses (same golden tables, same `flac -d`
  bit-exactness).

## Why two crates: the map is not the finding

**What actually travels, measured.** It is worth stating plainly, because the
menu's headline numbers suggest more portability than exists:

- **In the core and portable already:** the E2a `u32` refill accumulator and the
  E2b `LEADING_ONES` table count — pure Rust, zero unsafe, no platform import.
- **Does not travel: E4's i32 win.** Its safety is profile-bound (precision ≤ 11,
  `|coeff| ≤ 751`; profile precision 15 is *not* safe), and its −46.9% LPC result
  is an **ARMv4T artifact** — 64-bit arithmetic is software-emulated there. On
  64-bit hosts `i64` is native, so the narrow leg is expected to buy little. Only
  the *guarded* form (item 6a-6) is portable, and it is the core's by decision.
- **Does not travel: E10.** ARM-mode encodings are thumbv4t-only, and asm is
  unsafe by construction. Overlay material.
- **Closed/inert/negative and not extractable:** E1, E3, E6, E7.
- **The core's only GBA residue** is the `.iwram` *section name* — a link-script
  contract other ARM bare-metal targets do not define (a loud link failure, not a
  silent one) — and it sits behind `iwram-pin`, off by default.
- **Full-profile correctness is the core's job, not a profile precondition:** LPC
  ≤ 32, escaped partitions and wasted bits stay correct for arbitrary FLAC; the
  constrained encode profile is enforced at the product's pack time.

The core and the overlay implement the *same* Rice codeword map, and on the
founding platform that map measured two opposite things:

| Realization | Reading |
|---|---|
| Core's incumbent (`TableCountKernel`, E2a reader + E2b table) | the live baseline |
| Core's portable hybrid twin, same map, same tables pinned | **+42.60 / +42.72 cycles per decoded sample slower** (314/314 frames) |
| E10's ARM-mode asm realization, same map | **−349 / −348 cycles per sample** |

So the split is not "GBA needs different semantics" — it needs the same semantics
written in the machine's own idiom. That is why the platform-bound realization
lives in its own crate instead of as ARM-shaped code inside the portable core,
and why the core's "as portable as a laptop userspace FLAC decoder" claim stays
intact. Full numbers and the `nm`/section attribution: [`measurements.md`](measurements.md).

## The container: where frames come from (status: specified, not built)

The product plans to ship `GAFP` blobs: raw FLAC frames plus a small manifest
(stream info + a frame-offset index), produced offline by `scripts/pack_flac.sh`.
Consequences of that shape:

- Seek is an O(1) table lookup — no `Seek`/`SeekFrom` trait anywhere.
- Decode walks a `&[u8]` cursor; the manifest is borrowed, so the decode path is
  zero-allocation.
- Non-conforming files are rejected at **pack** time, not decode time.
- The frame index is one frame source, not the decoder's contract — hence
  `scan` (the table-less walk) lives in the core, and `Manifest` is product-side.

Today the spike embeds the same shape as a generated offset table (`assets.rs`),
which is why the decode path has needed no manifest to be measured.

## Playback path

The measured playback harness (`flac-rate-play`) is a ring buffer + FIFO8
DMA/timer feed: one decoded frame fills half a double buffer, the FIFO consumes
the other half, with a word-copy push and a branch-free convert fast path. At
2048 samples / 32 kHz that is ≈64 ms of audio per frame. The shipping
`agb` mixer/DMA integration is still Phase 2 step 9; the harness exists to prove
the seam, not to be the mixer. Measurements: [`playback-results.md`](playback-results.md).

## Design rules that must survive implementation

1. **No `alloc` in the decode path.** Scratch is caller-provided and borrowed.
   That is what keeps the path free of unsafe code (`#![deny(unsafe_code)]`, one
   named `cfg`-gated pin attribute).
2. **No I/O traits.** `&[u8]` + cursor, forward only.
3. **No division, ever.** ARM7TDMI has no hardware divide: shifts and adds only.
4. **Fixed-size predictor state** (`[i32; MAX_LPC_ORDER]`), never a growable
   buffer.
5. **Per-frame scheduling granularity.** One decode call = one frame ≈ 64 ms at
   2048/32 kHz, so the caller can pin decode work to DMA IRQ boundaries.
6. **Frames are independently decodable.** Warm-up lives inside each subframe, so
   a manifest offset is a safe seek and `PredictorState` is scratch, not history.
7. **A rejection writes nothing.** Cursor and output are left exactly as they were
   before the failing read.
8. **The core makes no real-time claim; the overlay makes no correctness claim of
   its own.** Real-time belongs to the overlay measured on mGBA, pending hardware
   arbitration.

**Memory.** Per frame: two channel buffers of `blocksize × i32` — 2048 × 2 × 4 B =
16 KiB scratch, plus ≤32 warm-up samples per subframe, plus the DMA double buffer.
Fits EWRAM (256 KB) with room to spare. IWRAM is 32 KB minus a 256 B BIOS
reservation; item 5 measured that pinning *data* tables helps and pinning *code*
hurts (E6).

## The portability ladder — delivered

| Rung | Status |
|---|---|
| 1. Metadata + docs reframed (portable core; GBA is the founding platform) | landed 2026-11-02 |
| 2. `iwram-pin` feature gate (pin surface off by default) | landed 2026-11-02 |
| 3. Second-target build witness (`thumbv7em-none-eabi`) | landed 2026-11-02 |
| 4a. Stream layer (`fLaC`, metadata chain, streaminfo) | landed 2026-11-04 |
| 4b. Per-frame loop (`driver`) | landed 2026-11-04 |
| 4c. Table-less frame walk (`scan`) | landed 2026-11-04, feature-gated |
| 5. `crc8`/`crc16` + verification behind `crc-check` | landed 2026-11-03 |
| 6. `RiceKernel` seam + window bridge + portable twin + the flip reading | landed 2026-11-05/06; the default **did not flip** (measured negative) |
| 7. Overlay crate, seam passable from outside | item 6a-1 and 6a-3 landed 2026-11-06/07 |
| 8. Guarded i32 default width | item 6a-6 **landed** 2026-11-09 (default `i32-integrator`, per-subframe guard, i64 escape leg) |
| 9. ARM kernel over the seam | item 6a-4 **landed** 2026-11-08 (correctness only, no default moved); item 6a-5 **measured** it through the seam at −584,856 c/frame (FIXED), fork resolved to *carry* — the one-line placement lands with the product path (6b) |
| 10. Manifest + packer + facade rewired onto `scan` | item 6b, planned |

## Decisions of record

- **2026-08-30 — option (c):** write `flac-lite`. (This file, top.)
- **2026-09-07 — the encode profile is a *max LPC order* plus flags**, never a
  "reject LPC" switch; `-l 4` emits real LPC-4 frames. See
  [`format-notes.md`](format-notes.md) Correction 3.
- **2026-10-11 — cumulative gains:** no lever substitutes for another; the rate
  track grows the denominator while decoder levers shrink the numerator, and the
  product is what clears. See [`plan-of-record.md`](plan-of-record.md).
- **2026-11-01 — the two-crate split**, corrected by measurement on 2026-11-06:
  the portable twin stays an alternative, the overlay owns the realization.
- **2026-11-06 — i32 becomes the core's default arithmetic width**, feature-gated
  and armed per **subframe** by a bound computed from values the parse already
  has, with i64 kept as the always-correct escape leg for off-profile streams
  (landed in item 6a-6; the frame reports the OR of its subframes). The width
  needs no seam.
- **2026-11-06 — the toolchain is a pin, not a note** (`nightly-2026-09-04`).
- **2026-11-08 — the seam is threaded to the frame, and the ARM kernel is carried**
  (item 6a-5). The `…_with<K>` chain is monomorphized (no `dyn`), the default stays
  one line, and the placement of the carry is the product's line, not the core's:
  the dependency arrow is overlay → core. Numbers:
  [`measurements.md`](measurements.md).
- **Unchanged throughout — correctness before speed**, one variable per run, and
  no cycle quoted across records.
