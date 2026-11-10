# flac_spike — the perf gate (Phase 1 step 4)

**Status: PR 1 landed — scaffold + embedded clips.** This crate is the
go/no-go measurement vehicle for FLAC-on-GBA. Full plan + validation rules:
[`../flac-docs/method-and-gates.md`](../../flac-docs/method-and-gates.md) and
[`../flac-docs/perf-results.md`](../../flac-docs/perf-results.md).

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
> budget. Per the project's method, the gate's working targets are re-derived in
> PR 3 against *measured* mixer/DMA/vsync load, never against a recalled
> margin. (The plan scheduled this correction for PR 5; it came early because
> the crate landed now and a false README is worse than an early one.)

## What gets measured

1. Decode-only cost per frame for the FIXED arm (`flac -l 0`),
2. The same for the LPC arm (`flac -l 4`) — the gate's question is the
   **comparison** (`-l 4` emits real LPC frames; flac-docs/format-notes.md, Correction 3),
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
| `make item6a7-test` / `item6a7-rom` | 6a-7's width leg: `src/wide.rs` (the pre-6a-6 i64 path, vendored so both widths are in one link) decodes to the reference PCM and agrees with the i32 control frame by frame; the ROM times `ctrl` (i32), `wide` (i64) and `asm` (kernel+i32) in one calibration. Since 6a-7b the same image also renders the ten-line **on-screen summary** (`src/summary.rs` + `src/item6a7b.rs`) and logs those ten lines verbatim, so a stock GBA with an EverDrive and an mGBA serial capture read the same string |

## The 6a-7b on-screen summary (`item6a7-wide` only)

`agb::println!` reaches mGBA and nothing else, and 6a-7b's rung is a reading taken
on a **GBA 001 and a GBA SP**. So the 6a-7 image renders its own result: `ctrl`,
`wide` and `asm` means per clip, the stack `S = ctrl − asm`, the width marginal
`Wd = wide − ctrl`, and the worst frame as a percent of the 32,768 Hz budget. Ten
lines at this font's `line_height() == 12` out of a hard 12-line budget, with the
fit re-derived from the real font metrics and logged (`lines=… fits=…`) rather than
assumed.

```
6A7B STACK ON HW
C=ctrl W=wide A=asm
S=C-A Wd=W-C X=worst%
l0 157f C1662797 W1752714
   A416821 S1245976
   Wd89917 X39.961
l4 157f C2987762 W6573107
   A1751108 S1236654
   Wd3585345 X167.854
HARNESS OK
```

Two things a reader should not misread. **The backdrop is validity, not budget:**
the spike's verdict ANDs the budget check and the LPC clip is 167% by design, so
blue here means "the numbers come from an image whose assets, decode proofs and
calibration all passed" — the budget verdict stays on serial. And **purple means
still running**: the screen is only painted after both clips have finished, which
takes minutes.

Whole-image sha is the wrong control for "did this change the ROM": the screen
moved two `core::panic::Location` line numbers in `.rodata` and nothing in
`.text` (`flac-docs/plan-of-record.md`, 6a-7b, the control build).

**And the ROM must stop presenting after the screen is drawn.** Under
`item6a7-wide` it ends in a bare `loop {}`, not the frame-commit loop the
colour-only verdict uses. `agb`'s `TiledGraphicsFrame::commit` computes DISPCNT's
background-enable mask from the backgrounds *shown into that frame*, so a commit
from a fresh frame writes `enabled_backgrounds = 0` and repaints the backdrop
alone. That is not theory: the first hardware run (GBA 001, twice, 2026-11-10)
showed exactly that — blue backdrop, no text — because the present loop erased
what `show()` had just committed. A headless emulator cannot see it.

Standalone workspace; cargo config is **inherited** from the repo root — do
not add a local `.cargo/config.toml` (the duplicated `-Tgba.ld` leak, flac-docs/method-and-gates.md).

## PR sequence (PRs 1–3 landed; 4–5 remain open; the live plan is flac-docs/plan-of-record.md)

| PR | Scope | Status |
|---|---|---|
| 1 | scaffold, clips, gates, host witness | ✅ |
| 2 | on-target decode checksum (gates every perf number) | ✅ |
| 3 | cycle harness (calibrated, overhead subtracted) | ✅ |
| 4 | full-clip cadence test (double buffers vs playback time) | — |
| 5 | verdict table + decision rule + hardware run | — |

Correctness-before-speed: **no perf number is ever reported from a ROM image
whose decode has not been proven correct on its own embedded bytes** (PR 2).
