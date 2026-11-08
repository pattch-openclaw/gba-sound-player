# FINDINGS.md — Cart Fidelity Findings & Codec Direction

Durable findings from the 2026-11-08 comparison against the sister project
**gba-audio** (<https://github.com/pattch-openclaw/gba-audio>, commit `7ce74e3`,
2026-11-08): a C implementation of the same end goal — one 32 MiB GBA ROM, whole
album, highest fidelity on the GBA's 8-bit DirectSound output. Their repo is a
measured-codec-shootout resource we can mine instead of re-deriving.

Claim tags follow FLAC.md convention: **[measured]** = run by us and
reproducible; **[external]** = measured in gba-audio by their tooling (their
DAC model, their C decoders — not yet re-verified on this stack); **[derived]**
= arithmetic shown; **[est]** = estimate.

## 1. The 8-bit encode target for FLAC [measured 2026-11-08]

**The playback surface is 8-bit.** The GBA FIFO takes 8-bit samples; a 16-bit
master's extra 8 bits are unreproducible. Encoding FLAC against a 16-bit master
(as our current perf-gate profile does, FLAC.md "initial profile") therefore
spends ~3× the ROM for zero fidelity at the speaker. The correct lossless
target is **8-bit / 32,768 Hz / stereo**, encoded: resample-in-float → quantize
once (plain rounding, no dither) → FLAC.

Pipeline and numbers, run on `assets/sound/bioluminescent.wav`
(96 kHz / 24-bit stereo, 4:18.6):

```sh
ffmpeg -i in.wav -af aresample=32768:precision=28 -c:a pcm_f32le master.wav
ffmpeg -i master.wav -c:a pcm_s8 -f s8 q8.raw        # plain rounding, no dither
flac --force-raw-format --endian=little --sign=signed --bps=8 \
     --sample-rate=32768 --channels=2 -8 -p -e q8.raw
flac -d --force-raw-format --endian=little --sign=signed -o out.raw q8.flac
cmp q8.raw out.raw                                    # BIT-EXACT ✓
```

- 8-bit raw: 524 kbit/s → FLAC: **195 kbit/s (2.69:1)** [measured; 6,303,072 B]
- 16-bit FLAC for contrast: 661 kbit/s — 3.4× the ROM, no playback benefit [measured]
- Cart fit (33,423,360 B stream budget, [derived]): **≈ 23 min/cart** at true
  8-bit lossless. One track measured; expect ~18–24 min across denser material.
- ffmpeg caveat: the WAV muxer rejects `pcm_s8`; pipe raw + `--force-raw-format` [measured]
- FIFO wants unsigned (u8, 0x80 center); constant offset, identical compression [derived]

**Why 2.7:1 is the ceiling:** after quantizing to 8 bits the incompressible
payload *is* the quantization noise; FLAC's predictor cannot code what is
fundamentally unpredictable. This is a property of the signal, not the encoder.

## 2. The fidelity frontier at cart scale [external: gba-audio proto/judge/RESULTS.md]

All SNRs are mean per-track after their 8-bit DAC model (SOUNDBIAS mode 1),
43-min CC-licensed test album, same 32 MiB budget (~100 kbit/s):

| path | fit | SNR after DAC | CPU worst frame |
|---|---|---|---|
| Lossless 8-bit FLAC (this file, §1) | ~23 min | DAC floor only, ~35 dB [derived] | open (see §4) |
| MDCT "ulc-inspired", hop 512 [external] | 43 min | ~29.9 dB | ~74% [est], gate ≤75% |
| best alternative MDCT (blockswitch) [external] | 43 min | ~29.5 dB | 81% — over gate |
| **FLAC of requantized master** (gba-audio `proto/flac2`) [external] | 43 min | **~24.0 dB** | ~95% [external] |
| raw 8-bit WAV, no codec | ~8.5 min [derived] | DAC floor | ~0 |

Two non-obvious results, both confirmed by the seven-codec shootout:

1. **FLAC-of-a-quantized-master is strictly dominated.** To fill a cart at
   album length, FLAC must be fed a pre-degraded master (~5 effective bits via
   power-of-two wasted-bit shifts, some tracks at 16 kHz). That is a *uniform,
   spectrally-blind* lossy quantizer — a constrained special case of lossy
   encoding — and it lands ~6 dB below an MDCT codec with per-band bit
   allocation (reverse water-filling) at identical bytes. "Lossless" applies
   only to the pipe after the loss has already been taken.
2. **The DAC, not the codec, sets the gap.** The 8-bit ceiling is ~35.2 dB
   [external]; the best full-album lossy result is ~29.9 dB. A bit-exact 8-bit
   master sits at the floor: **≤23 min, FLAC-8bit is the fidelity optimum**,
   ~2–4 dB above what any lossy codec achieves at the same cart size [est,
   power-addition of codec + DAC noise]. Above ~23 min it cannot fit without
   pre-degradation, and the lossy codecs win at equal length.

So the honest shape: FLAC's role here is **delivery of samples already at the
hardware's resolution floor**, not compression. It wins the quality corner of
the frontier; an MDCT wins the length corner. Both are 32 kHz/8-bit playback.

## 3. Decision (Sam, 2026-11-08): pursue a ULC codec implementation in Rust

Given gba-audio's measurements, the **ulc-inspired** format is the winner at
full-album length (SPEC round 3: hop 512, N=1024 IMDCT, fused plain rounding;
~29.9 dB after DAC at ~100 kbit/s, est ~74% worst-frame CPU pending their
mGBA gate). Direction: **port ULC to Rust (`no_std`) as our album-length
codec**, grounding development in their numbers rather than re-running a
shootout.

Why this is tractable where flac-lite's perf gate struggles:

- ULC's decoder is deliberately simple: **plain Huffman** entropy (no rANS /
  adaptive context), small tables (~7–30 KB), one N=1024 DCT-IV IMDCT, fixed
  windows, no block switching, no TNS/SBR. Fewer load-bearing parts to get
  real-time on ARM7TDMI.
- Prior art exists in C: `ulc-codec` and `ulcplayer-gba` by Aikku93 —
  **Unlicense** (no license obligations; credit in README). gba-audio used
  them as baseline/fallback and ported their asm ideas.
- The integer-precision choices (16-bit internal, bit-exact reference) can
  follow gba-audio's "normative C" pattern: a host reference decoder that is
  bit-identical to the GBA path, tested against it on real frames — the same
  witness methodology this repo already uses in FLAC.md.

Suggested first gates (mirroring this repo's methodology, not new invention):
1. compile probe + host round-trip of a vendored/portable ulc encode→decode;
2. cycle spike on target (reuse the flac_spike harness and budget arithmetic:
   at hop 512 / 32768 Hz the frame budget is 512 cyc/sample, same as FLAC.md §
   "Budget, derived" — a 512-sample frame ≈ 262 k cycles);
3. same-clip SNR comparison against gba-audio's numbers as an external anchor.

## 4. Standing caveats

- **CPU is unchanged by bit depth.** FLAC decode cost is dominated by LPC +
  Rice per sample, not by sample width; 8-bit payload does not lift flac-lite's
  perf-gate failure (219%/654% of budget, FIXED/LPC). The §1 fidelity win is
  only real once real-time decode exists — gba-audio's C FLAC prototype
  measured ~95% worst frame, so this is marginal even in C [external].
  The ULC decision (§3) is partly an escape from this dead end.
- **[external] ≠ verified here.** All gba-audio SNR/CPU figures come from their
  C decoders and their DAC model on their harness; mGBA does not model UPWM
  nonlinearity and nothing is hardware-verified in either repo. Re-verify any
  number this project commits to, per the usual witness rule.
- SNR ranking is trustworthy at the ~6 dB gaps used here, not at the sub-dB
  near-ties among their top codecs (their judge report says the aggregation
  choice flipped one winner). Last mile is listening on real hardware.
- Noise shaping measured **worse** on SNR in headphone mode (no room to push
  shaped noise: audio band ≈ full Nyquist at 32 kHz). Plain rounding is the
  measured optimum [external]; keep dither out of the §1 pipeline for SNR
  objectives.

## Pointers

- gba-audio: `docs/SPEC.md` (requirements + round-2/3 decisions),
  `docs/DESIGN.md`, `proto/judge/RESULTS.md` (7-codec shootout),
  `proto/flac2/REPORT.md` (the FLAC-prequant post-mortem),
  `tests/results/latest/summary.md` (full-suite evidence).
- Upstream ULC: ulc-codec / ulcplayer-gba (Aikku93, Unlicense).
- This repo: perf-gate methodology FLAC.md → "Perf gate spike"; harness
  precedent `examples/flac_spike/`; Opus survey (rejected, 49× over budget on
  CELT) OPUS.md.
