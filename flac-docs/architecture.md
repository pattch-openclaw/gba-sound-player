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
  decoder.rs               facade shell — still todo!() (needs a frame source)
  format.rs                sample-rate/channel helpers; Manifest, Blocksize::samples
                           and EncodeProfile::validate still todo!()
  scan.rs                  table-less frame walk + resync (feature `scan`)
  rice_hybrid.rs           portable hybrid Rice twin (feature `rice-hybrid`)

crates/flac-lite-gba/      GBA overlay: no_std, deny(unsafe_code), unsafe budget 0
  rice_twin.rs             an out-of-crate RiceKernel arm (feature `rice-twin`)

examples/flac_spike/       the measurement instrument (the `ctrl` arm of every E
                           record) + experiments/<name>/ reappliable samples
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
| `iwram-pin` | pins the hot lookup tables to `.iwram` on ARM targets (the crate's only `unsafe` surface: placement attributes on immutable tables) | GBA ROM builds + `nm` witness |
| `crc-check` | verifies header CRC-8 and footer CRC-16 instead of consuming them | `flac-test-crc` |
| `scan` | compiles the table-less frame walk | `flac-test-scan` |
| `rice-hybrid` | compiles the portable hybrid Rice twin | `flac-test-rice` |
| `strict-profile` | *declared*: rejects frames outside the constrained encode profile | **not implemented** — `format::EncodeProfile::validate` is still `todo!()`, and no gate builds the feature. Product-side work (6b-2) |
| `rice-twin` (overlay) | forwards to `flac-lite/rice-hybrid`; exposes the out-of-crate arm | `overlay-test-twin` |

The `decoder` facade and the `format` items above are the remaining `todo!()`s; the
`GAFP` byte contract is specified in [`../crates/flac-lite/README.md`](../crates/flac-lite/README.md)
and `scripts/pack_flac.sh` is still a documented stub. The spike's generated
per-frame offset table is the deliberate stand-in.

**Nothing links `flac-lite-gba` yet.** No ROM, no example, no other crate. That is
the item-6a shape, not an oversight: the overlay's first rung exists so that the
build plumbing and the seam's reachability are settled before any asm arrives.

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
| 8. Guarded i32 default width | item 6a-6, planned |
| 9. ARM kernels over the seam | item 6a-4/6a-5, planned |
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
  and armed per frame by a bound computed from values the parse already has, with
  i64 kept as the always-correct escape leg for off-profile streams (lands in
  item 6a-6). The width needs no seam.
- **2026-11-06 — the toolchain is a pin, not a note** (`nightly-2026-09-04`).
- **Unchanged throughout — correctness before speed**, one variable per run, and
  no cycle quoted across records.
