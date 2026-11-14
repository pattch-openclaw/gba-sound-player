# Architecture — what the FLAC track is made of, and why it is shaped this way

> **Scope: the FLAC line of work.** This file describes `crates/flac-lite`,
> `crates/flac-lite-gba`, the `GAFP` container and the seam between the portable core and
> the GBA overlay. It is not a repository-wide architecture document: the other codec
> tracks live in [`../ULI2.md`](../ULI2.md), [`../ULC.md`](../ULC.md) and
> [`../OPUS.md`](../OPUS.md), and the baseline audio/input stack in
> [`../README.md`](../README.md). Numbers live in [`measurements.md`](measurements.md);
> byte-level format facts in [`format-notes.md`](format-notes.md); how it got here in
> [`history.md`](history.md).

## The constraint

`thumbv4t-none-eabi` is Tier-3 bare-metal: `#![no_std]`, **`core` + `alloc` only**. Any
crate that references `std`, directly or through a transitive dependency, cannot compile
for this target. `alloc` exists; the decode path does not use it. The ARM7TDMI has no
hardware divide and Thumb-1 has no `CLZ`/`RBIT`/`REV`, which together decide most of what
the hot loop looks like ([`arm-kernels.md`](arm-kernels.md)).

## Why there is a decoder of our own (2026-08-30)

The mature Rust FLAC decoders are built on `std::io`, and in symphonia's no-std fork the
blockers are structural rather than a dependency-tree accident: `Error::IoError(std::io::Error)`
as the single error type, an `io/` module on `Read`/`Seek`/`BufRead`, a `HashMap` codec
registry, `std::sync::Arc` in metadata (~27 non-trivial `std` usages across
core/common/metadata), and no `#![no_std]` on the FLAC crate at all. Waiting on upstream is
not an option — that work has languished for years.

The decision was **option (c): write a minimal `no_std`, zero-dependency, zero-allocation
FLAC frame decoder**. The alternatives were (a) fork the community no-std branch, which is
permanently tracking a WIP fork of a moving upstream for a ~200–400-line diff, and (b) port
`clac`, the same cost against someone else's `std::io` architecture. (c) was chosen because
**controlling both ends deletes the entire stack that made the port impossible**: encoding is
offline and ours, the "filesystem" is a memory-mapped fixed-layout ROM, so there is no
`Read`/`Seek` trait, no container probe, no runtime registry, no `HashMap`, no `IoError`. A
decoder over `&'static [u8]` with a cursor needs none of it, and the math is integer
shifts/adds/multiply-accumulate — **no division**, which matters on this CPU.

The reproduction of the symphonia failure is kept at `examples/symphonia_flac_probe/` (its
own `Cargo.toml`; expected to fail compilation). It is the evidence for the decision, not a
live component. The root ROM crate carries no FLAC dependency either way.

## What the tree contains

```
crates/flac-lite/          the portable core — no_std, zero deps, zero alloc on the
  bits.rs                    decode path, deny(unsafe_code)
                           MSB-first reader (u32 refill accumulator), UTF-8 coded
                           numbers, wasted bits, CRC-8/CRC-16, KernelWindow bridge
  stream.rs                fLaC magic, §8.1 metadata chain, §8.2 STREAMINFO, SeekTable,
                           the fixed-block number line, frame_data_offset
  frame.rs                 frame header parse + decode_frame, derived width bounds
  subframe.rs              CONSTANT / VERBATIM / FIXED / LPC, integrators, pad_block
  residual.rs              partitioned Rice / Rice2 / escape, and the RiceKernel seam
  stereo.rs                mid-side, left-side, right-side decorrelation
  scan.rs                  the table-less frame walk + resync        (feature `scan`)
  rice_hybrid.rs           the portable second Rice mechanism  (feature `rice-hybrid`)
  driver.rs                per-frame loop over a borrowed frame index (instrument-facing)
  decoder.rs               the facade: Decoder::new(&Manifest) + decode_next_with::<K>.
                           A shell over scan::Frames — deliberately not a third loop
  format.rs                the GAFP layer: Manifest (parse, frame_region, blocksize,
                           max_fixed_order, stream defaults), Blocksize::samples,
                           EncodeProfile::validate, StreamInfo::from_stream

crates/flac-lite-gba/      the GBA overlay — no_std, deny(unsafe_code), no `agb`
  rice_arm.rs              the shipped ARM-mode kernel, its FFI, KernelAbi, three tables
                                                              (feature `rice-arm`)
  rice_twin.rs             an out-of-crate arm forwarding to the core's twin
                                                              (feature `rice-twin`)
  asm/rice_partition.s     the assembled body, in `.iwram` (see the placement finding)

examples/flac_integration/ THE PRODUCT CRATE: a GAFP blob + manifest + facade + kernel.
                           Two binaries from one feature table, one lockfile, one set of
                           pins — `flac-integration` (decode/verify/time) and
                           `product-play` (decode → convert → ring → sound DMA, with its
                           own underflow count, period audit and verdict).
                           tests/product_play_witness.rs is the host model of the geometry.
examples/overlay-kernel-check/  the kernel's device witness — the only place the asm runs
examples/flac_spike/       the measurement instrument (the `ctrl` arm of every record)
                           + experiments/<name>/, every probe as a reappliable sample
crates/flac-lite/tests/    committed libFLAC vector tables + the layout/decode suites
scripts/                   gafp_pack.py (the packer), gen_gafp_blob.py, frame_vectors.py
                           and the gen_* asset/fixture generators

examples/arm-kernels/      ARCHIVED — E10's isolated kernel groundwork (source of the
examples/e5_fetch/         shipped asm; the fetch-ratio arbitration). No Make target.
examples/symphonia_flac_probe/  the compile-failure probe; documentation of the constraint
```

**Features, all off by default except `i32-integrator`, and the gate that covers each.**
A non-default configuration is witnessed by its own gate, because the default build does not
compile what it covers — `required-features` on the witness suites is what makes "the default
build contains no kernel" a checked statement rather than a claim.

| Feature | What it changes | Gate |
|---|---|---|
| `i32-integrator` (core, **on** by default) | i32 arithmetic width, armed per subframe by `Σ|c_j|·2^(b−1) ≤ i32::MAX`, with the i64 leg as the always-correct escape for off-profile streams. `ArithmeticWidth` reports which leg answered | `flac-test` + `flac-test-wide` (the pair *is* the differential) |
| `iwram-pin` (core) | pins the hot lookup **tables** to `.iwram` on ARM — the crate's only `unsafe` surface (placement attributes on immutable tables), off by default so a target with no `.iwram` section gets a loud link failure and not a silent one | GBA ROM builds + the `nm`/address witness |
| `crc-check` (core) | verifies header CRC-8 and footer CRC-16 instead of consuming them | `flac-test-crc` |
| `scan` (core) | compiles the table-less frame walk | `flac-test-scan`, `flac-portable-check` |
| `rice-hybrid` (core) | compiles the portable hybrid Rice twin | `flac-test-rice`, `flac-portable-check` |
| `strict-profile` (core) | holds a manifest to `format::DEFAULT_PROFILE` — implemented, and **no gate builds it**: `validate()` is tested directly and the product ROM does its own start-up profile check. The guarantee moved to **pack time**, where the packer measures the frames instead of trusting a header | none — deliberate; a runtime-strict profile is optional insurance and owes a gate leg before it is wanted |
| `rice-arm` (overlay) | assembles `asm/rice_partition.s`, declares the FFI and `KernelAbi`, owns the three tables. This is what the product links | `overlay-check-arm`, `overlay-test-arm` (contract), `overlay-kernel-test` (device), `overlay-objdump` |
| `rice-twin` (overlay) | forwards to `flac-lite/rice-hybrid`; the out-of-crate arm the seam's reachability was proven with | the core's `flac-test-rice` covers the mechanism; the overlay's own twin suite is `required-features`-gated |
| `master-asset` (product crate) | builds `product-play` against the generated `assets/master.gfp` + `src/master_pin.rs` instead of the committed asset | `master-play-rom`, and `product-play-test` on the tracked asset |

**Note which pin is *not* a feature.** The ARM kernel **body** is pinned to `.iwram`
unconditionally, and there is no configuration in which ROM residency is the code you want:
6a-5d measured the same instruction stream at **4.000 cycles per instruction fetched from
cartridge ROM against ~1.4 from IWRAM** — 2.9× on the decode, from one section directive,
with nothing else changed. E6's "instruction fetch is ruled out" does not contradict this;
it pinned a Rust path where every instruction carried many cycles of work, which is exactly
the case a 4× fetch price is invisible in.

## The seam: a whole-partition `RiceKernel`

The kernels replace the per-sample codeword step inside `residual::decode_rice_partition`,
so the core exposes a small generic trait whose default is the shipped table-count loop:

- **Whole-partition, not per-sample.** A kernel owns the byte window (`KernelWindow { w, v,
  ptr, end }`, handed out and taken back through `bits`), because that is the only shape
  that has ever produced a measured number.
- **Monomorphized, never `dyn`.** And note the measured cost of *instantiation*, not of
  dispatch: a second `RiceKernel` body merely present in a link re-decides inlining for the
  shared decode path (+2,984 B of `.text` with no symbol of it in the image, and tens of
  millions of cycles on the *default* arm).
- **Unsealed, default `TableCountKernel`** named on one line; `decode_rice_partition_with::<K>`
  is the seam. The choice is threaded up the caller-facing chain — `frame::decode_frame_with`
  → `subframe::decode_subframe_with` → `residual::decode_residual_with` — with each
  non-generic entry point a one-line delegation to `…::<TableCountKernel>`. That is what lets
  a product name an overlay kernel without the core depending on the overlay; the dependency
  arrow is overlay → core.
- **Two mechanisms stay live.** `bits::rice_quotient` and `LEADING_ONES` are not deleted even
  where a kernel replaces them: the differentials consume two live mechanisms, and
  `LEADING_ONES` is the `nm` anchor cited by half the ledger — as an anchor, not a constant
  (it relocates when a second body becomes reachable, with its bytes unchanged).
- **Correctness is the core's.** The overlay owns no decode semantics; it is judged by the
  core's witnesses — same golden tables, same `flac -d` bit-exactness.

## Why two crates

Not "GBA needs different semantics" — it needs the same semantics written in the machine's
own idiom. The core and the overlay implement the *same* Rice codeword map, and on this
platform that map measured two opposite things: the portable hybrid twin of it is
**+42.6 c/sample slower** than the incumbent, and the ARM-mode realization of the same map
is **−349 c/sample faster**. So the platform-bound realization lives in its own crate rather
than as ARM-shaped code inside a portable one, and the core's "as portable as a laptop
userspace FLAC decoder" claim stays intact.

What actually travels, measured:

- **Portable already:** the E2a `u32` refill accumulator and the E2b `LEADING_ONES` table
  count — pure Rust, zero unsafe, no platform import.
- **Does not travel: E4's i32 win.** Its safety is profile-bound and its −46.9% LPC result is
  an **ARMv4T artifact** (64-bit arithmetic is software-emulated there). Only the *guarded*
  form is portable, and it is the core's by decision: on a 64-bit host `i64` is native, so
  the narrow leg buys nothing there and might cost.
- **Does not travel: E10.** ARM-mode encodings are thumbv4t-only and asm is unsafe by
  construction.
- **The core's only GBA residue** is the `.iwram` *section name* — a link-script contract
  other bare-metal ARM targets do not define — behind `iwram-pin`, off by default.
- **Full-profile correctness is the core's job, not a precondition:** LPC ≤ 32, escaped
  partitions and wasted bits all stay correct for arbitrary FLAC. The constrained encode
  profile is enforced at the product's **pack** time.

## The container, and where frames come from

`GAFP` (byte contract in [`../crates/flac-lite/README.md`](../crates/flac-lite/README.md)):
raw FLAC frames plus a small manifest and a frame-offset table, written offline by
`scripts/gafp_pack.py`. Consequences of that shape:

- Seek is an O(1) table lookup — no `Seek`/`SeekFrom` trait anywhere.
- Decode walks a `&[u8]` cursor; the manifest is borrowed, so the decode path allocates nothing.
- **Non-conforming files are rejected at pack time, not at decode time.**
- The frame index is *one* frame source, not the decoder's contract — hence `scan` (the
  table-less walk) lives in the core and `Manifest` is product-side, with the facade a shell
  over the walk rather than a third loop.

`scripts/gen_gafp_blob.py` is not redundant with the packer and is not waiting to be
retired: it re-assembles the committed product blob from the **spike's** own committed frames,
which makes it the only gate that can say the asset the ROM plays is the asset the instrument
measured. `gafp_pack.py` reaches the same bytes by a different route and cannot say that.

## The playback path

`examples/flac_integration/src/product_play_main.rs` is the shipping path, and it is not
`agb`'s mixer: the ROM programs timer0 as the sample clock, the timer1 cascade as the period
IRQ, the sound-DMA enable and every per-period re-arm itself, and states the mixer's absence
as a safety precondition. The chain is
`Decoder` → `convert_frame` (i32 → 8-bit FIFO bytes, `sample >> 8`) → `RingFeed` (36 chunks of
560 samples per channel, prefill 16) → sound DMA/FIFO, with the push taken in `CHUNK`-sized
interrupt-masked sections. It counts its own underflows, gates its period audit (coalesced /
late / clock refusal / tail / void) and folds the **played form** — the 8-bit FIFO bytes per
channel — as well as the 16-bit decode PCM.

Two things that path learned the hard way and should not re-learn:

- **The 16-bit digests are decode witnesses and say nothing about what the speaker plays.**
  That is why `FNV_FIFO_L/R` exist. Folding them on the *push* path was tried and measured:
  the house FNV-1a is 64-bit, ARM7TDMI has no 64-bit multiply, and 4,096 FIFO bytes per frame
  costs ≈800,000 cycles — the run failed at 128% work/frame. The witness therefore runs in the
  post-audio proof pass, and the residual gap is stated: it is the converter's output for the
  same frames, not a capture of the DMA/DAC, with `RingFeed`'s byte copy as the
  host-witnessed bridge in between.
- **A critical section is measured in samples by the thing it must not delay, and in cycles
  by the code that holds it.** Holding IME off for a whole 2,048-sample frame's ring copy
  (14,680 c) overshot a 16-sample re-arm grace and made one service in 582 late,
  deterministically, on both units. `CHUNK`-sized sections cut that window to 4,014 c.

## Design rules that must survive any edit

1. **No `alloc` in the decode path.** Scratch is caller-provided and borrowed. That is what
   keeps the path free of unsafe code.
2. **No I/O traits.** `&[u8]` + cursor, forward only.
3. **No division, ever.** Shifts and adds only.
4. **Fixed-size predictor state** (`[i32; MAX_LPC_ORDER]`), never a growable buffer.
5. **Per-frame scheduling granularity.** One decode call = one 2048-sample frame = 125 ms at
   the product rate, so a caller can pin decode work to DMA IRQ boundaries.
6. **Frames are independently decodable.** Warm-up lives inside each subframe, so a manifest
   offset is a safe seek and `PredictorState` is scratch, not history.
7. **A rejection writes nothing.** Cursor and output are left exactly as they were before the
   failing read.
8. **The core makes no real-time claim; the overlay makes no correctness claim of its own.**
   Real-time belongs to the product image measured on hardware; correctness belongs to the
   core's witnesses.
9. **Sub-crates inherit the root `.cargo/config.toml` and must not re-declare it** —
   `rustflags` concatenate across config layers, so a second `-Tgba.ld` double-links.

**Memory.** Per frame: two channel buffers of `blocksize × i32` = 2048 × 2 × 4 B = **16 KiB**
of scratch, plus ≤32 warm-up samples per subframe, plus the ring (40,320 B) and the DMA
buffers. That fits EWRAM (256 KB) with room to spare and does **not** fit IWRAM (32,768 B
total) at this rate — which is a live design fact, not a preference: E3 measured the
EWRAM-write penalty at **3.482 c/B** on real hardware, and the ring's placement is the
standing target on the producer's convert leg.

## Decisions of record

| Date | Decision |
|---|---|
| 2026-08-30 | Option (c): write `flac-lite`. |
| 2026-09-07 | The encode profile is a **max LPC order plus flags**, never a "reject LPC" switch (`-l 4` emits real LPC-4 frames); frame fixed fields are 32 bits. [`format-notes.md`](format-notes.md) |
| 2026-10-11 | **Cumulative gains:** no lever substitutes for another; the rate track grows the denominator while decoder levers shrink the numerator. |
| 2026-11-01 | The two-crate split. Corrected by measurement on 2026-11-06: the portable twin stays an alternative, the overlay owns the realization. |
| 2026-11-06 | Guarded i32 becomes the core's **default** width, armed per subframe, with i64 as the escape leg. The toolchain becomes a pin, not a note. |
| 2026-11-08 | The seam is threaded to the frame and the ARM kernel is carried; the placement of the carry is the product's line, not the core's. |
| 2026-11-09 | The kernel body is **IWRAM-resident** (6a-5d: placement outranks every local optimization). |
| 2026-11-10 | The kernel ships in the product ROM (6b-1) and C1/C2 become the only definition of done. |
| 2026-11-11 | Non-conformance is refused **at pack time**, by reading frames rather than trusting headers (6b-2); and the declared-rate identity rule is ruled (Q14). |
| 2026-11-13 | The rate is a measured result, not a commitment; **16,384 Hz** is the product clock; push sections are `CHUNK`-sized; C2 is met and acceptance becomes per-path rather than per-asset. [`history.md`](history.md) |
| Throughout | **Correctness before speed**, one variable per run, and no cycle quoted across records. |
