# flac_spike — the perf gate (Phase 1 step 4)

**Status: PR 1 landed — scaffold + embedded clips.** This crate is the
go/no-go measurement vehicle for FLAC-on-GBA. Full plan + validation rules:
[`../../FLAC.md`](../../FLAC.md) → "Perf gate spike — concrete plan and
validation process (2026-09-18)".

## The question

Can a 16.78 MHz ARM7TDMI decode `flac-lite` frames fast enough to keep a
playback buffer fed, in real time, with no headroom for a second attempt?

Budget per frame at the initial profile (2048 samples @ 32,768 Hz):

```text
2048 / 32768 s × 16.78 MHz ≈ 1,048,750 cycles/frame ≈ 512 cycles/sample
```

> ⚠️ This README's placeholder stated "62,500 cycles per frame" — that is the
> **microsecond** figure (62.5 ms = 62,500 µs) with its unit misapplied. The
> derived budget above is ~16.78× larger, and the placeholder's working
> targets (≤35k mean / ≤55k max) inherit the error — they are ~3% of the true
> budget. Per the FLAC.md plan, the gate's working targets are re-derived in
> PR 3 against *measured* mixer/DMA/vsync load, never against a recalled
> margin. (The plan scheduled this correction for PR 5; it came early because
> the crate landed now and a false README is worse than an early one.)

## What gets measured

1. Decode-only cost per frame for the FIXED arm (`flac -l 0`),
2. The same for the LPC arm (`flac -l 4`) — the gate's question is the
   **comparison** (`-l 4` emits real LPC frames; FLAC.md Correction 3),
3. The tail, not just the mean: one over-budget frame is an audible glitch,
   so max is reported next to min/mean.

## The two embedded arms (generated, never hand-edited)

| Arm | Encode | Census (measured, libFLAC 1.5.0) |
|---|---|---|
| `L0_FIXED` | `flac -1 -l 0 -b 2048 -m` on the 10 s deterministic source | `fixed0` ×157, both slots |
| `L4_LPC` | `flac -1 -l 4 -b 2048 -m`, same source | `lpc4` ×156 + `lpc3` ×1, both slots |

Same source, two encodes ⇒ both arms must decode to byte-identical PCM (the
witness suite asserts it). Regenerate everything:

```sh
scripts/gen_spike_assets.sh    # needs flac + metaflac + python3
```

The generator (`scripts/frame_vectors.py spike_assets`) fails closed: strict
frame-finder invariants, per-frame subframe walks with the measured footer-gap
rule, and arm censuses — if a libFLAC upgrade ever makes `-l 0` not
FIXED-only, or `-l 4` not majority-LPC, it refuses to write rather than emit a
silently degenerate comparison. Committed blobs are generator output; the
hand-packed-vector rule applies to whole clips.

Files: `assets/*_frames.bin` are the raw frame regions (what the ROM embeds —
no fLaC magic, no STREAMINFO); `assets/*_pcm.bin` are `flac -d` reference PCM
(host-test ground truth, never embedded); `src/assets.rs` is the generated
manifest (per-frame offset table standing in for the GAFP seek table, sizes,
FNV-1a pins, censuses, `include_bytes!`).

## Gates

| Command | What it proves |
|---|---|
| `make spike-test` | Host witness suite: both embedded regions decode **bit-exact** vs `flac -d`, hash pins reproduce (Python ↔ Rust), the ROM's shared fold (`fold_i16le_stereo`) reaches the `fnv_pcm` pins on the host, corrupted seek entry breaks the walk (negative control) |
| `make native-spike-rom` / `podman-spike-rom` | The spike ROM builds, links, fixes for `thumbv4t-none-eabi`; boots, verifies region hash pins, then **decodes both arms on-target** and compares the PCM FNV-1a against the pins (blue/red screen, BitReader-PoC convention) |

Standalone workspace; cargo config is **inherited** from the repo root — do
not add a local `.cargo/config.toml` (the duplicated `-Tgba.ld` leak, FLAC.md).

## PR sequence (per the FLAC.md plan)

| PR | Scope | Status |
|---|---|---|
| 1 | scaffold, clips, gates, host witness | ✅ |
| 2 | on-target decode checksum (gates every perf number) | ✅ |
| 3 | cycle harness (calibrated, overhead subtracted) | — |
| 4 | full-clip cadence test (double buffers vs playback time) | — |
| 5 | verdict table + decision rule + hardware run | — |

Correctness-before-speed: **no perf number is ever reported from a ROM image
whose decode has not been proven correct on its own embedded bytes** (PR 2).
