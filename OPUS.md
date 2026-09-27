# OPUS.md — Opus Integration Evaluation

Durable notes on Opus-in-Rust for the GBA target. Companion to
[FLAC.md](FLAC.md); same constraint, same methodology (compile probes against the
real target, witnesses over recollection). Opened 2026-09-27.

## Why look at Opus at all

The FLAC perf gate is not passing: measured on-target, `flac-lite` runs at
**≈219% of the real-time budget on the FIXED arm and ≈654% on LPC** (FLAC.md,
spike PR 3), and experiments E1–E7 have ruled out every cheap lever — no probe
has moved the mean meaningfully. Opus is the alternative worth pricing in:
far higher compression (less ROM per minute of audio) and *potentially* cheaper
per-sample decode for speech-like content (SILK is integer arithmetic).

The honest caveat up front: **Opus is more compressed, not obviously cheaper to
decode on this hardware.** CELT is MDCT-heavy and float-based; the ARM7TDMI has
no FPU, so every butterfly is soft-float. Any Opus path must clear the same
correctness-before-speed perf gate FLAC.md already defines.

## The Constraint (unchanged)

`thumbv4t-none-eabi`, Tier-3 bare-metal, `#![no_std]`, **`core` + `alloc` only**.
Any crate referencing `std` — directly or transitively — cannot compile. Full
argument in FLAC.md → "The Constraint".

## Rust Opus landscape (surveyed 2026-09-27)

~20 crates.io crates match "opus"; seven are pure-Rust decoders/codec ports.
All non-FFI claims below were **compile-probed**, not read:
`cargo +nightly build --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc`
against a standalone probe crate (native host; probe recipe at the end of this file).

| Crate | Verdict | Evidence |
|---|---|---|
| **`opus-rs` 0.1.34** | ✅ **builds for thumbv4t** with one patch | `#![cfg_attr(not(feature = "std"), no_std)]`, `libm` feature, zero FFI. Patch: see below. |
| `audio-codec` 0.4.7 | ⚠️ same blocker, same fix | Wraps `opus-rs` (`default-features = false, features = ["opus"]`); its own `g729-sys` dep compiles clean. Inherits opus-rs's AtomicU8 failure and its fix. |
| `opus-decoder` 0.1.1 (Rusopus) | ❌ not `no_std` | No `no_std` attribute; `thiserror` pulls std (`can't find crate for std` ×2); plus unconditional `std::fs`/`std::time` debug-trace code in `celt/mdct.rs` (dead-code-looking but compiled). Tempting crate ("no unsafe, no FFI, RFC 8251 conformant") — wrong ecosystem posture, same symphonia story in miniature. |
| `opus-pure` 0.2.1 | ❌ std-only | No `no_std` attribute; 29/82 source files use `std::`. |
| `rusty-opus` 0.9.1 | ❌ std-only | Performance fork of opus-rs (AVX2 kernels — irrelevant here); std-only. |
| `ogg-opus` 0.1.2 | ❌ std-only | Thin Ogg-Opus wrapper. |
| `opus-wave` 3.0.1 | ❌ std-only | Includes optional DNN (DRED/OSCE) PLC — the wrong direction for 256 KB WRAM. |
| `opus`, `audiopus_sys`, `libopus_sys`, `opus-head-sys`, … | ❌ C FFI | libopus bindings. Dead on arrival for the target. |

## Candidate: `opus-rs` 0.1.34 — the one viable path

Repo: <https://github.com/restsend/opus-rs>. Pure-Rust encoder+decoder, MIT/Apache.

### Verified compile (2026-09-27, this host, nightly 2026-09-03)

With `default-features = false, features = ["libm"]`, the **only** error on
`thumbv4t-none-eabi`:

```
error[E0432]: unresolved import `core::sync::atomic::AtomicU8`
  --> opus-rs-0.1.34/src/compat.rs:15:26
   |
15 | use core::sync::atomic::{AtomicU8, Ordering};
   |                          ^^^^^^^^ no `AtomicU8` in `sync::atomic`
```

ARMv4T has no `AtomicU8` in `core` (LLVM atomics for `thumbv4t` cover natural
word-sized types only; `AtomicU8` is target-gated out). The fix is one import —
`portable_atomic` (already in the repo's lockfile via `flac_spike`):

```diff
- use core::sync::atomic::{AtomicU8, Ordering};
+ use portable_atomic::{AtomicU8, Ordering};
```

plus `[dependencies] portable-atomic = { version = "1", features = ["critical-section"] }`.
With that, the probe (`OpusDecoder::new` + `decode` through a `no_std` extern
"C" shim) builds **clean** for `thumbv4t-none-eabi`.

### Gotcha 1 — the `heap` feature is hard-coupled to `std`

`opus-rs`'s `heap` feature (`heap = ["std"]`) is what `Box`es the codec state;
without it all state is inline. Enabling it for the GBA build fails immediately
(`can't find crate for std` + prelude failures — the crate stops being
`no_std`). Measured host `size_of::<OpusDecoder>()` **without** heap:
**178,064 bytes inline**. That is a large static for 256 KB WRAM (before PCM
buffers, resampler, DMA double-buffer, and the game itself). A local patch that
re-implements the `heap` pattern via `alloc::boxed::Box` **decoupled from std**
(the fields are already individually `cfg`-gated — ~38 sites) is the fix; it is
a mechanical, contained vendor patch.

### Gotcha 2 — float internals on a soft-float target

CELT paths compute in `f32`; every op is a soft-float call on ARM7TDMI. SILK
(narrowband/wideband speech) is mostly integer. The perf question therefore
splits by codec mode and must be **measured, not assumed** — see the gate below.

### Gotcha 3 — sample-rate mismatch

`OpusDecoder::new` accepts 8/12/16/24/48 kHz. The `agb` sw mixer offers
10512 / 18157 / 32768 Hz. There is **no shared rate**; the playback path needs a
resampler (48k → 32768 Hz, ratio ≈ 0.6827) or a cheap rate choice
(24k → 32768, ratio ≈ 1.3653). Integer linear/cosine-interp resampling is a
few cycles/sample; it joins the decode cost, and the witness set must pin it too.

## Probe (reproducible)

Standalone probe crate (pattern of `examples/symphonia_flac_probe/`, but this
one **compiles**):

```sh
mkdir opus-probe && cd opus-probe
cat > Cargo.toml <<'EOF'
[package]
name = "opus-probe"
version = "0.1.0"
edition = "2024"
[lib]
crate-type = ["lib"]
[dependencies]
opus-rs = { version = "0.1.34", default-features = false, features = ["libm"] }
EOF
# src/lib.rs: no_std shim calling OpusDecoder::new(48000, 1) + decode(...)
cargo +nightly build --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc
# → expect: E0432 AtomicU8 (compat.rs:15). Vendor-patch as above → builds clean.
```

Probe crates lived in `/tmp/opus_probe*` during the survey (throwaway). If this
evaluation proceeds, promote to `examples/opus_probe/` with the vendored patch,
and add Makefile gates in the `flac-test` pattern (host tests outside the repo
tree — the cargo-config-leak rule, FLAC.md; target check inside it).

## Minimal proposal — an Opus ROM, shaped like the FLAC ROM

Deliberately mirrors the `flac-lite` architecture (FLAC.md → Decision
2026-08-30): we own both ends, the "filesystem" is fixed-layout ROM, so the
entire Ogg container layer is **deleted** — Opus packets are self-delimiting
via the TOC byte, and packet boundaries come from an offline manifest.

```
offline (host)                              ROM (no_std, core+alloc)
────────────────────                        ──────────────────────────────────
source.wav ──ffmpeg/libopus──▶ .opus (ogg)
                                │  pack script: demux → raw packets
                                │  + manifest (rate, channels, TOC-pinned
                                │    frame size, per-packet offset+length)
                                ▼
                           .gob blob ──▶ include_bytes! / static ROM
                                             │
                                             ├─ Manifest      (O(1) packet seek)
                                             ├─ vendored opus-rs (patched):
                                             │    OpusDecoder state in EWRAM
                                             │    (Box-via-alloc patch)
                                             ├─ decode 20ms packet → 960×f32 @48k
                                             ├─ integer resampler 48k → 32768
                                             ▼
                                       double buffer → mixer/DMA (A/B vs WAV)
```

**Container ("GOBP" blob, sibling of GAFP):** manifest with stream info +
per-packet `(offset, length)` index. No Ogg pages, no CRC-per-segment parsing,
no `Read`/`Seek` traits — same deletion of the std-io stack that made
`flac-lite` tractable. Constrained encode profile, pinned at pack time:
mono, one fixed frame size (20 ms → 960 samples @48k), one bandwidth per
track, packets verified to match the manifest (reject at pack time, not
decode time).

**Memory plan (the tight part):** decoder state ~178 KB + resampler state +
two stereo-mono halves of 960×i16 + game code/graphics. Against 256 KB EWRAM
this only fits **mono** and only with the state boxed in EWRAM — an honest
constraint, and one reason the spike is mono first. IWRAM stays for hot code
and scratch (the E6 lesson: pinning *all* hot code to IWRAM measured slower;
pin selectively or not at all).

**Perf gate (the same gate, applied before building anything real):**

1. Budget, derived: a 20 ms frame @16.78 MHz = **335,600 cycles**, 960 samples
   → **≈350 cycles/sample** (decode + resample + mono mix), versus flac-lite's
   measured ~1,113 c/sample FIXED.
2. Encode the *same* spike clip through both arms of the Opus decision —
   **SILK speech** (nb/wb, low bitrate) and **CELT music** (fb, default) — and
   run the existing `flac_spike` cycle harness pattern (`examples/opus_spike/`,
   standalone workspace crate, inherits root `.cargo/config.toml`, never
   re-declares it — the duplicated `-Tgba.ld` rule).
3. Correctness-before-speed, unchanged: on-target FNV-1a PCM checksum against
   reference decode (reference = libopus via `ffmpeg`/`opus_demo` — the brew
   `opus` bottle is library-only here; either extract `opus_demo` from source
   or witness against ffmpeg-decoded PCM), proven on the exact embedded bytes
   before any cycle number is trusted.

**Decision rule:** if SILK fits the budget and CELT does not, an Opus ROM is a
*speech/voice* format (SILK-only profile, pack-time reject of CELT frames) —
still a genuinely different product from FLAC (music). If neither fits, Opus
joins the ruled-out list and FLAC's remaining levers (E8 sub-stage attribution)
stay the mainline.

## Status (2026-09-27)

- Survey + compile probes complete (this file is the deliverable; probes were
  throwaway in `/tmp`).
- **No Opus dependency anywhere in the repo; root ROM unaffected.** No spike
  crate yet — `examples/opus_spike/` and the vendored patched `opus-rs` land
  only if we schedule the gate above.
