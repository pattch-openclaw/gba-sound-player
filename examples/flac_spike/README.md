# flac_spike — the perf gate

The go/no-go measurement vehicle for FLAC-on-GBA. It answers one question, and it
is the arm every performance record in this repository is measured against:

> Can a 16.78 MHz ARM7TDMI decode `flac-lite` frames fast enough to keep a playback
> buffer fed, in real time, with no headroom for a second attempt?

At the product profile — 2048-sample frames at 16,384 Hz — the budget is
**2,097,500 cycles/frame ≈ 1,024 cycles/sample** on the instrument's rounded clock
(`stats::real_time_budget(2048, 16_384)`), or 2,097,152 on the GBA's exact 2²⁴ Hz timer.
The two denominators and what that gap is worth:
[`../../flac-docs/measurements.md`](../../flac-docs/measurements.md) §"The budget". (An
early version of this README stated the budget as "62,500 cycles per frame", which is
62,500 *microseconds* with the unit misapplied; the working targets inherited the error
and were ~3% of real time while claiming to be a margin. That is why budgets here are
derived from the constant, never recalled.)

Method rules, gate surface and reading rules:
[`../../flac-docs/measurements.md`](../../flac-docs/measurements.md). The verdicts this
crate produced (E1–E10 and the item ladder):
[`../../flac-docs/history.md`](../../flac-docs/history.md).

## What gets measured

1. Decode-only cost per frame for the FIXED arm (`flac -l 0`).
2. The same for the LPC arm (`flac -l 4`) — the question is the **comparison**, because
   `-l 4` emits real LPC frames and `-l 0` is "LPC disabled", not "FIXED-0"
   ([`format-notes.md`](../../flac-docs/format-notes.md), Correction 3).
3. The tail, not just the mean: min, **max with its frame index**, sum and mean. One
   over-budget frame is an audible glitch.

## The two embedded arms (generated, never hand-edited)

| Arm | Encode | Census (measured, libFLAC 1.5.0) |
|---|---|---|
| `L0_FIXED` | `flac -1 -l 0 -b 2048 -m` on the 10 s deterministic source | `fixed0 ×80`, both slots |
| `L4_LPC` | `flac -1 -l 4 -b 2048 -m`, same source | `lpc3 ×1, lpc4 ×79`, both slots |

Both are 80 frames / 162,304 samples per channel at **16,384 Hz** and must decode to
byte-identical PCM, pin `0xEB3EA1C24FE1574F` — same source, two encodes, so the witness
suite asserts the equality rather than assuming it. Regenerate everything with:

```sh
scripts/gen_spike_assets.sh    # needs flac + metaflac + python3
```

The generator (`scripts/frame_vectors.py spike_assets`) **fails closed**: the strict frame
finder's stream invariants per encode, per-frame subframe walks with the measured
footer-gap rule, and arm censuses — if a libFLAC upgrade ever makes `-l 0` not FIXED-only
or `-l 4` not majority-LPC, it refuses to write rather than emit a silently degenerate
comparison. Committed blobs are generator output; the hand-packed-vector rule applies to
whole clips.

Files: `assets/*_frames.bin` are the raw frame regions (what the ROM embeds — no `fLaC`
magic, no STREAMINFO); `assets/*_pcm.bin` are `flac -d` reference PCM (host ground truth,
never embedded); `src/assets.rs` is the generated manifest (a per-frame offset table
standing in for the GAFP manifest, sizes, FNV pins, censuses, `include_bytes!`).

## Gates

| Command | What it proves |
|---|---|
| `make spike-test` | Host witness: both embedded regions decode **bit-exact** vs `flac -d`, the hash pins reproduce (Python ↔ Rust), the ROM's shared fold (`fold_i16le_stereo`) reaches the `fnv_pcm` pins on the host, and a corrupted seek entry breaks the walk (the negative control). |
| `make spike-rom` | The ROM builds, links and is fixed for `thumbv4t-none-eabi`; it boots, verifies its region hash pins, then **decodes both arms on-target** and compares the PCM FNV-1a against the pins. |

Run the instrument headlessly and read its serial:

```sh
make spike-rom
timeout -k 5 280 mgba-test-runner flac-spike.gba | grep -E 'stats:|budget:|verdict'
```

Two things a reader should not misread. **The backdrop is validity, not budget:** the
verdict ANDs the budget check with the correctness gates and the LPC arm is over budget by
design, so blue means "these numbers come from an image whose assets, decode proofs and
calibration all passed". **And the run takes minutes** — the screen is only painted after
both clips have finished.

Correctness-before-speed: **no perf number is ever reported from an image whose decode has
not been proven correct on its own embedded bytes.**

## Archived configurations, and what they measured

This crate carries instrument arms that no longer have Make entrypoints, because the rungs
they were built for are closed. Their sources and their records stay (the numbers are in
`measurements.md`, the reasoning in `history.md`); nothing in the shipped path builds them,
and a default build of this crate has no second decoder, no extra kernel symbol and no
`deny`-vs-`forbid` relaxation.

| Feature | Was | Finding it produced |
|---|---|---|
| `item6a3-twin` / `-hybrid` / `-present` | the crate-boundary and second-instantiator probes | a `RiceKernel` body merely *present* in a link moves the default path by 26.5 M cycles and grows `.text` by 2,984 B with no symbol of it in the image |
| `item6a5-rust` / `-asm` / `-arms` | the realization reading, through the product call path | `asm − ctrl` = −584,856 c/frame (−286.9 c/sample), 157/157 frames faster; and one control's absolute moves ~200 k c/frame when a second mechanism joins the link, which is why "three arms, one image" is a paired reading and not a baseline |
| `item6a5b-vendored` | E10's archive vendored as a fourth timed leg, in one link with the landed kernel | the seam costs **zero**; +48.2 c/sample was six instructions × the difference between a ROM fetch and an IWRAM fetch |
| `item6a7-wide` (+ the 6a-7b on-screen summary) | the width marginal and the stack, three legs in one calibration | i64 costs +44.1 c/sample on FIXED; additivity reproduces to 0.06%; the first bit-exact decode claim on silicon |
| `rate-assets` (and `flac-rate-spike` / `flac-rate-play`) | the 10,512 Hz rate flow that proved the sample-rate lever and the FIFO feeder | real audible music at 10,512 Hz, correct tempo and pitch, and the +35% convert lesson that the product path inherited |

`item6a5b-vendored` is the one exception to "a probe never lands on the spike's `src/`": a
per-frame paired reading of two realizations cannot be built from a patch applied to a
scratch checkout. Two mechanics only show up when a second kernel is in the same link, and
both are recorded in [`arm-kernels.md`](../../flac-docs/arm-kernels.md): `global_asm!`
symbols are not namespaced, and a crate's `global_asm!` blocks assemble into one unit, so
`.Lloop`-style local labels collide and must be renamed too.

Standalone workspace; cargo config is **inherited** from the repo root — do not add a local
`.cargo/config.toml` (the duplicated `-Tgba.ld` double-links:
[`measurements.md`](../../flac-docs/measurements.md), "Cargo config inheritance").
