# flac_spike — performance gate (placeholder)

**Status: placeholder. No code, no `Cargo.toml` — deliberately not wired into the
build** (the root crate uses `autoexamples = false` and declares examples
explicitly, so nothing here can be compiled by accident).

This directory is where the go/no-go measurement for FLAC-on-GBA happens, before
any real decoder work beyond the minimum needed to measure. Full rationale:
[`../../FLAC.md`](../../FLAC.md) → "Risk & gate".

## The question

Can a 16.78 MHz ARM7TDMI decode FLAC frames fast enough to keep a DMA-fed PCM
buffer full, in real time, with no headroom for a second attempt?

Budget per frame at the initial target (2048 samples @ 32768 Hz):

```
62,500 cycles per frame  (2048 / 32768 s × 16.78 MHz)
```

Leaving a sane margin for the mixer, DMA IRQs and vsync work, the working target
is **≤ ~35,000 cycles/frame average, and no single frame above ~55,000**.

## What gets measured

1. Decode-only cost per frame, FIXED predictors (orders 0–4) + partitioned Rice —
   the constrained profile with full LPC banned (`flac -l 4`).
2. Same, with full LPC subframes allowed — to learn what LPC costs and whether
   `-l 4` must become a hard constraint.
3. Cost split: bit reading vs residual decode vs predictor integration.

## How

- ROM example: `include_bytes!` a packed clip, decode N frames in a loop, bracket
  each `decode_frame` with a free-running timer counter (or `agb`'s cycle-ish
  timing facilities), and report min/max/mean cycles per frame via
  `agb::eprintln!` so mGBA's log is the measurement output.
- Sanity-check against a host build of the same code for correctness first — a
  fast wrong decode measures nothing.
- Watch the tail, not just the mean: one over-budget frame is an audible glitch.

## Outcomes

| Result | Consequence |
|---|---|
| FIXED-only fits, LPC fits | Keep the full decoder; profile stays a preference. |
| FIXED-only fits, LPC doesn't | `flac -l 4` becomes a **hard project constraint**; reject LPC frames at pack time. |
| Neither fits | Reduce scope: shorter loops, lower rate, order 0–1 predictors, or offline pre-processing. FLAC may not be viable on this hardware and that is a valid answer. |

## Prerequisites (in order, per FLAC.md)

1. `bits::BitReader` + tests
2. `format::Manifest` + real `scripts/pack_flac.sh`
3. `subframe` FIXED + `residual` Rice → **this spike**
