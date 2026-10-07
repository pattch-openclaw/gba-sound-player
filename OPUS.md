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

### "But does Opus support higher rates?" — no: 48 kHz is the ceiling (2026-09-27)

Researched because 65,536 Hz is Goal #5 territory: **the Opus format never
exceeds 48 kHz.** Internally the codec always operates at 48 kHz (RFC 6716 §2);
the lower rates are SILK-native input/output modes, and libopus resamples
internally for anything else. The API rates are exactly **{8, 12, 16, 24, 48}
kHz — nothing arbitrary, nothing higher**. What reads as "higher rates" is
almost certainly the *audio bandwidth* ladder (NB 4 kHz … FB 20 kHz "fullband")
— bandwidth ≤ 20 kHz, Nyquist ≤ 24 kHz, never a 48 k-plus sample rate.

opus-rs matches libopus exactly here, source-verified: both constructors reject
everything else with `"Invalid sampling rate"` (`OpusEncoder::new` lib.rs:350,
`OpusDecoder::new` lib.rs:1199; the list `[8000, 12000, 16000, 24000, 48000]`,
no 44100 anywhere in the crate). **Highest supported rate: 48,000 Hz.**

Consequences for a ~65 kHz GBA:

- **Feasibility:** 48k → 65,536 is pure *upsampling* (512/375 ≈ 1.3653×).
  Upsampling cannot alias — the reconstruction filter just removes images — so
  it is strictly easier to do correctly than the 48k → 32768 downsample.
- **Cost:** resampler work scales with *output* samples: 65536 produces ~2× the
  output samples per decoded packet vs 32768, so ≈2× resampler cost per second
  — plus the mixer running at 65k, which is precisely the un-built part of
  Goal #5 (agb's sw mixer stops at 32768). Decode cost is unchanged either way:
  decoding happens on 48 k frames regardless of the playback rate.
- **What it buys:** at 32768 (Nyquist 16,384 Hz) the 16.4–20 kHz band of a
  fullband encode is paid for in bits and then low-passed away. Two coherent
  profiles instead: **32768 playback** ⇒ cap encoder bandwidth at superwideband
  (12 kHz content, safely under Nyquist — stop paying for unrecoverable highs);
  **65536 playback** (if Goal #5 lands) ⇒ fullband survives at ~2× mixer/resampler cost.
- **Cheapest path overall stays 16k → 32768:** ratio 2.048 ≈ 2×, near-trivial
  interpolation with a small filter — the speech-first profile's best friend.

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

### Measurement plan — start here (added 2026-09-29)

The concrete PR order, mirroring the flac gate step for step:

1. **`examples/opus_spike/`** — standalone workspace crate exactly like
   `examples/flac_spike/`: cargo config **inherited** from the repo root
   (never re-declare `.cargo/config.toml` — the `-Tgba.ld` double-link rule),
   dual target (thumbv4t ROM + host witness tests), vendored patched
   `opus-rs` living in `vendor/opus-rs/` — **not** in the root ROM.
2. **Two embedded arms, same clip, modes recorded not assumed:** encode the
   same deterministic 10 s source twice — a **SILK speech arm**
   (`-application voip`, API rate 16 kHz ⇒ SILK-only WB) and a **music arm**
   (`-application audio`, API rate 48 kHz ⇒ what libopus actually picks at
   FB/20 ms — *hybrid*, verified by the TOC census below, since TOC has no
   pure-CELT 20 ms config: CELT-only tops out at 5 ms, RFC 6716 Table). A
   pack script demuxes Ogg → raw packets + `(offset, length)` manifest →
   `include_bytes!` blobs; fail-closed censuses (TOC mode, 20 ms pin, mono,
   strict Ogg pages with CRC-32) refuse to write a degenerate arm. Generator
   output, never hand-packed.
3. **Driver seam like flac_spike's:** a `decode_clip` walking the manifest,
   one packet per callback. One deliberate difference from FLAC: Opus decode
   is a **state machine** (no per-packet independent decode), so the walk is
   sequential with a fresh decoder per clip, and the offset table is the
   product-shape manifest, not a seek-independence witness.
4. **PR order copied from the FLAC gate (correctness-before-speed):** host
   witness first — decode embedded packets with patched `opus-rs`, fold the
   PCM, compare bit-exactly against the ffmpeg/libopus reference (the Opus
   decoder is bit-exact by spec; any float-path divergence is a measured
   finding with a recorded tolerance, never assumed away); then the ROM
   decode proof; only then the cycle harness.
5. **Reuse the harness wholesale when it arrives:** timer2/timer3 cascade at
   /1, empty-window calibration subtracted, IRQs off in the window,
   min/max/sum to mGBA serial, division-free with the mean derived host-side.
   Time **decode + resample together** per packet — the decoder-internal
   resampler is part of the product cost.
6. **Budget derived, not recalled:** 20 ms @ 16.78 MHz = **335,600 cycles /
   960 samples ≈ 350 c/sample** (vs flac-lite's measured ~1,113 c/sample
   FIXED) — worst frame reported next to mean, since one over-budget frame
   is an audible glitch.

**The cheap first move is steps 1+2:** vendored patched `opus-rs` compiling
inside `examples/opus_spike/` with the two embedded arms and the host witness
green — no ROM cycle time spent, and the whole pipeline is proven before any
cycle counting.

## Status (2026-09-27)

- Survey + compile probes complete (this file is the deliverable; probes were
  throwaway in `/tmp`).
- **No Opus dependency anywhere in the repo; root ROM unaffected.** No spike
  crate yet — `examples/opus_spike/` and the vendored patched `opus-rs` land
  only if we schedule the gate above.

## Status (2026-09-29): step 1 landed — `examples/opus_spike/` PR 1 (scaffold + vendored decoder)

- **The spike crate exists** with the vendored patched `opus-rs`; per Sam's
  scope decision this PR carries **step 1 only** — step 2's arms + pack
  script + decode witness are the next PR. The gates:
  - `make native-opus-rom` — real thumbv4t build/link of the scaffold ROM
    embedding the vendored decoder (the compile gate proving the vendored
    `opus-rs` **links** for the target, not just `check`). The ROM logs a
    decode-path link witness (`probe::decode_packet`'s address in the ROM
    mapping — survives `lto = "fat"`) and the inline state size bounded
    against the 256 KB EWRAM argument.
  - `make opus-test` — out-of-tree host gate (same cargo-config-leak pattern
    as `spike-test`): the constructor contract (five rates × mono/stereo,
    44.1 kHz rejected), the derived 335,600-cycle budget arithmetic, and the
    inline state size pinned to the measured 178,064 bytes (gotcha 1's
    number — now a named test, not prose).
  - Headless mGBA boot witness (2026-09-29, ROM sha256
    `17ef387b7cc22e8104522437a0816b4155f5fefb097dc60c61831830a211ef7e`):
    decode-path link `probe::decode_packet @ 0x08023EB9` inside the ROM
    mapping (the vendored decoder survives `lto = "fat"`), and the state
    size logged on-target: **177,864 bytes** — 200 below the host figure,
    `size_of` being per-target; both readings carry the same ≈178 KB EWRAM
    argument. BLUE verdict; the run log is the thumbv4t number's witness.
- Vendor patch on `opus-rs` 0.1.34 is the one documented above (`compat.rs`:
  `AtomicU8` via `portable_atomic`), applied to a vendored copy under
  `examples/opus_spike/vendor/opus-rs/` (BSD-3, license + provenance +
  re-verification recipe in `PATCHES.md`). **The vendored source is copied
  as-is** (diff-audited 2026-09-30, `diff -rq src <crates.io source>`): all
  67 `.rs` files byte-identical to the published crate except `compat.rs`,
  whose single changed line is that import (plus the comment recording it)
  — zero edits to any SILK/CELT/MDCT/range-coder/tables logic. The target
  fit comes from build configuration, not tampering: feature flags
  (`libm`, `heap` off), the dependency substitution, and target-gated
  `portable-atomic` features; only the manifest is reconstructed (dev-
  deps/tests stripped). Any future vendor sync should reproduce exactly
  this one-file diff — more or less than it is drift to explain.
  **Correction to the sketch here** (found by the first real ROM link,
  2026-09-29): the `critical-section`
  feature **cannot coexist with agb** — agb 0.25 enables portable-atomic's
  `unsafe-assume-single-core` in every thumbv4t link, and portable-atomic
  `compile_error!`s when the two combine. The vendor manifest therefore
  mirrors agb's own feature set (`unsafe-assume-single-core` + `fallback`),
  verified for both the ROM link and the standalone vendor build.
  PATCHES.md note 1a carries the detail. No heap patch yet: the decoder
  state stays inline (`heap` is std-coupled) — fine for the scaffold; the
  EWRAM Box-via-`alloc` story is a later PR's tracked omission.
- A seam contract measured from vendored source while wiring `probe.rs`:
  `OpusDecoder::decode` **rejects `frame_size = 0`** ("frame_size too small")
  — the TOC decides the actual decode geometry, but the caller must still
  carry ≥ that many samples; there is no "let the TOC decide" zero
  convention.
- Still open (later PRs, per the plan above): step 2 (two embedded arms via
  the pack script + fail-closed TOC censuses, host witness vs the
  ffmpeg/libopus reference), then ROM decode proof, then the cycle harness
  reusing flac_spike PR 3's counter discipline. A prepared measurement note
  for step 2: ffmpeg 9.0.1's reference decode removes the front pre-skip and
  trims the tail to source duration — the raw packet walk from sample 0 is
  the reference's *prefix* by `pre_skip`; the generator must witness that
  alignment, not assume it.

## Status (2026-10-02): step 2 landed — two embedded arms + host witness (PR 2)

- **The arms exist and the host witness proves the decode.** `scripts/gen_opus_assets.sh`
  + `scripts/opus_assets.py` generate the two arms of the FLAC gate's deterministic
  10 s source (silk: `-application voip` 12 kbps → census-pure config 9 SILK-only WB
  ×501; music: `-application audio` 96 kbps → census-pure config 31 CELT-only FB 20 ms
  ×501 — both at the 48 kHz API rate, the deviation from the 2026-09-29 sketch recorded
  in the script header). `probe::decode_clip` walks the manifest sequentially (fresh
  decoder per clip — the state-machine difference from flac_spike, plan item 3);
  `tests/opus_witness.rs` proves the walk over the exact embedded bytes meets the
  ffmpeg/libopus float reference. Gates: `make opus-test` 12 green (4 witness +
  4 checksum unit + 4 scaffold pins), `make native-opus-rom` links the assets into the
  ROM image, `make check` clean.
- **Two prior claims this step measured FALSE (the correction is the finding):**
  1. *"the raw packet walk from sample 0 is the reference's prefix by `pre_skip`"*
     (the 2026-09-29 note above). False for the SILK arm: the vendored walk meets the
     reference at **pre_skip − 3 = 309** (0 grid mismatches there; 478,439/480,000 when
     compared at the header's 312). The CELT arm aligns exactly at pre_skip = 312.
     Alignment is now a **per-arm measured pin** (`align_shift` in assets.rs): the
     generator discovers it fail-closed by driving the crate's own
     `probe::walk_region` through `examples/dump_walk.rs` (the production seam, not a
     second decode loop), and the witness **re-derives it in Rust** over the
     production walk's bytes — the two discovery implementations must agree with the
     pin. Bands the generator refuses to write outside of: shift within ±16 of
     pre_skip, every residual ≤ 1 LSB, residual count < 1% of samples.
  2. *"the i16 fold is their measured equivalence class, 0 mismatches per arm"*
     (draft, 2026-10-01, truncate-toward-zero — the vendored decoder's own output
     convention). Not reproducible: on the CELT arm truncate disagrees with the
     reference on **240,397/480,000** samples by exactly ±1 LSB — the port's
     soft-float drift (max |Δ| ≈ 0.76 LSB) amplified by truncation's grid-offset
     boundary. The honest grid is **round half up** (`checksum::fold_f32_to_i16`,
     no_std-safe integer rounding, half-up pinned over half-away at exact .5):
     residuals become SILK **0**, CELT **127 of 480,000**, every one ±1 LSB — soft
     drift crossing grid boundaries, not decode divergence. The 127 ride as the
     music arm's measured `fold_mismatch` pin: the witness asserts the count
     exactly and bounds every delta at ≤ 1 LSB. Zero is not the equivalence class;
     the measured residual is.
- **Driver guard corrected.** The draft's per-entry `packet.len() != entry.len` check
  was tautological (the slice is built from `entry.len`) — the length-drift negative
  control caught it by returning `Ok`. The walk now verifies manifest **tiling** before
  touching the decoder (start 0, contiguous, non-empty, exact cover; each violation a
  named error at its index) and the witness carries **six** corrupt-manifest controls
  (past-region / gap / overlap / **in-region lying length** / truncated table / empty
  packet) plus an identity positive control so the guard can't pass by rejecting
  everything. The decoder cannot be the guard: measured 2026-10-02 against the vendored
  decoder, a first packet truncated 30 B → 14 B still returns `Ok(960)` — a lying
  length inside the region decodes silently, which is why manifest integrity is the
  driver's contract and content corruption is the decode-vs-reference layer's net.
- **Deliberate scope cuts:** no ROM decode proof and no cycle counting (correctness-
  before-speed orders them next, flac_spike PR 2→3); `dump_walk` is `host-tools`
  feature-gated so it never enters the ROM build or the host gate; reference blobs
  stay host-only (the ROM pins them by hash); the EWRAM Box story stays a tracked
  omission. Asset sha256s after regeneration: silk packets
  `76fa0525…`, music packets `7e26c8d6…`, refs unchanged (`bf4adc61…` / `4136341a…` —
  encode + reference decode reproduce bit-identically; the generator's walk-dump step
  is measurement-only and cannot move the blobs).

## Status (2026-10-04): step 3 in progress — ROM decode proof lands RED, hang mechanism proven (PR 3 WIP + constructor-shape probe)

- **What lands:** the ROM decode proof (`src/main.rs`: region FNV → production-
  seam walk → per-packet running hash → `fnv_walk_fold` golden → EWRAM
  placement), `decode_clip`/`walk_region` returning `WalkStats` (totals +
  decoder/window addresses — the placement witness and the manifest cross-
  check), the generator's `fnv_walk_fold` pins (host witness re-derives them;
  13 host tests green, 12 → 13), and the **constructor-shape probe ROM**
  (`src/ctor_probe.rs`, `[[bin]] ctor-probe`, built beside the proof by
  `make opus-rom`). **The proof image stays red on-target by measurement, not
  by neglect** — this PR is the representative in-progress landing Sam asked
  for: the harness that localizes the failure, with the fix as the next PR.
- **The mechanism, measured end to end (probe ROM rev1–rev7).** The 2026-10-03/
  04 hang was decoder construction: the walk banner prints, packet 1 never
  does, mGBA dies at a wild PC (`F901F900` proof ROM; `48084808` probe).
  Seven probe revisions, each death named by the last banner printed:
  1. **Heap capacity: measured OUT.** The descending ladder (200,000 → 4,096 B,
     raw `alloc::alloc`, 3-point writes, readback, dealloc) round-trips
     **every rung clean** at 0x02008370 on the 229,664 B heap — allocator
     call, write range, free-list reuse and split at scale all sound. (An
     earlier ascending ladder got NULLs ≥ 131,072 B — correct allocator
     behavior after fragmenting its own heap; the ordering was the mistake,
     recorded because it confounded rev2–rev3's readings.)
  2. **Construction shape: convicted.** Three spellings of "construct then
     Box" all die before packet 1 on that same healthy heap: the production
     `Box::new(new_decoder(..)?)` (probe P2), the repro `.map(|d| Box::new(d))`
     (TAIL), and the candidate caller-side fix — `new_decoder_boxed`, an
     `#[inline(always)]` wrapper applying `Box::new` directly to the ctor
     result (P4). Caller-side destination propagation does **not** eliminate
     the temporary: intermediate by-value returns *inside* the vendored ctor
     (sub-decoder constructors building the `Self` literal) survive every
     caller spelling. `new_decoder_boxed` stays in the seam as the measured
     negative.
  3. **The SP witness closes it.** `read_sp` reads SP by asm at stage entry
     (valid: the `thumbv4t-none-eabi` target spec pins `"frame-pointer":
     "always"`, so SP is never scratch — the compiler even refuses `r7` as
     an inline-asm operand). P4's frame: **SP = 0x02FAF610 — 362,736 B below
     agb's stack top (0x0300_7F00) and 330,224 B below IWRAM's bottom
     (0x0300_0000)**. A frame that size cannot hold one 177,864 B temporary;
     the arithmetic fits two (2 × 177,864 = 355,728 ≤ 362,736 — the outer
     sret plus the ctor's own by-value temporaries inlined into the caller's
     frame). The frame wraps off IWRAM's bottom into **mapped** EWRAM: the
     writes succeed silently, destroying `.bss`/heap/allocator state, until
     the corrupted return path jumps to garbage — which is exactly why the
     failure was silent, and why the old honesty note's "fails loudly rather
     than silently" prediction was measured FALSE (corrected in place).
- **Prior-assumption corrections this step:** (1) the honesty note's
  fail-loudly claim — false on mapped-RAM substrates; (2) the plan's implicit
  assumption that `Box::new(ctor_result)` + `lto = "fat"` elides the large
  return — false here, and not fixable at the call site; (3) the 2026-10-03
  note's `F901F900` vs silence treated as two fault classes — one class
  (corrupted return), landing-dependent.
- **Gates & witnesses.** `make opus-test` 13 green (witness suite grows
  4 → 5 with `walk_fold_hash_reproduces_the_rom_decode_golden`; the ROM's
  golden is the seam's own folded output, never the reference — lossy-codec
  rule). `make native-opus-rom` builds both images; sha256 witnesses for
  this landing: proof `09f5de90…` (boots clean, region FNV MATCH, dies at
  construction — status quo red), probe `0a358968…` (ladder all-clean,
  `sp@entry=0x02FAF610 … STACK TEMPORARY CONFIRMED`, jump trap). The probe
  is **build-only, never a test gate**: a dead ROM wedges
  `mgba-test-runner`, so the deliberately-red image is captured by hand
  (sha + serial tail), like the FLAC gate's boot checks but inverted.
  Regeneration tripwire re-run green: all four asset blobs byte-identical
  from a scratch crate dir (`assets.rs` differs only by post-generation fmt
  reflow — token-stream equal, all scalar pins identical).
- **Deliberate scope cuts (this PR):** the constructor fix does NOT land
  here — the next PR adds an in-place construction path to the vendored
  seam (no by-value hop anywhere: `Box::new_uninit`-style in-place init or a
  boxed ctor), which is expected to turn the proof ROM green and re-runs
  this probe as its before/after witness. The probe's P3 stage (rev1's
  fused `vec![0u8; 200_000]` death) sits behind TAIL's expected death and
  stays unmeasured; cycle counting stays ordered behind a green proof
  (correctness-before-speed).

## Status (2026-10-05): vendor patch 2 landed — in-place decoder construction (lib-only PR; ROM integration is the next PR)

- **What lands:** the constructor fix the 2026-10-04 entry ordered, as
  **vendor patch 2** — a narrow lib-only PR: no ROM was built or booted
  (scope: vendor patch + host unit witnesses). Four additive APIs in
  `vendor/opus-rs/` initialize the decoder state **directly through a
  destination pointer**, so no `Self` temporary materializes anywhere on
  the construction path: `FixedVec::init_fill_at` (in-place twin of
  `from_value`; carries all 25 `FixedVec` fields),
  `CeltDecoder::init_in_place` (removes the 81,952 B by-value hop),
  `SilkDecoder::init_in_place` (removes the 9,304 B hop; the largest
  residual temporary on that path is the 3,992 B
  `SilkDecoderState::default()` written into the destination), and
  `OpusDecoder::new_in_place -> Box<OpusDecoder>` +
  `OpusDecoder::init_in_place` (validation with `new()`'s exact guards
  **before** `Box::new_uninit`, so a rejection allocates nothing). `new()`
  is byte-for-byte unchanged; the vendor diff against the published crate
  is now exactly patch 1 + patch 2, purely additive, zero decode-path
  bytes touched (PATCHES.md patch 2 carries the full inventory, sizes,
  unsafe contracts, and the upstreamability note).
- **How this fixes the ROM decode hang (mechanism → fix).** The probe
  convicted the **by-value construction frame**: `Box::new(new(..)?)`
  materialized sret temporaries sized like the decoder — SP read at
  0x02FAF610, ≈363 KB below agb's stack top, two decoder-sized frames —
  wrapping off IWRAM's bottom into mapped EWRAM and silently destroying
  `.bss`/heap/control flow (2026-10-04 entry). Patch 2 removes the frame
  at its source: with no by-value `Self` return left on the path
  (178,064 B host / 177,864 B target), there is nothing for the stack to
  materialize; the only allocation is the `Box` itself, landing in
  EWRAM's heap — the substrate the ladder measured clean. This is the
  shape libopus itself uses (`opus_decoder_create` initializes into the
  caller's memory rather than returning the state by value). **The
  on-target claim stays predicted until the next PR:** it wires
  `probe::walk_region` to `new_in_place`, rebuilds the proof ROM, and
  re-runs the constructor-shape probe as the designated before/after
  witness (expected: candidate stage survives, proof ROM prints packet
  1). `new_decoder_boxed` stays in the seam as the measured negative.
- **Witnesses (host only, by scope).** `tests/in_place_ctor.rs`, 3 tests:
  (1) **bit-identical differential** — both embedded arms (501 sequential
  packets each) decoded side by side through `new()` and
  `new_in_place()`, raw f32 compared bit-for-bit; same code, same
  machine, so there is no drift to tolerate — any initialization
  difference diverges the state machine, and the sequential walk
  exercises every field the decode path reads; (2) **committed goldens**
  — the in-place walk reproduces each arm's generator-measured
  `fnv_walk_fold` (the ROM proof's golden, measured through the OLD
  construction), pinning both ctor shapes against ground truth so layer
  1 cannot pass on a shared construction mistake; (3) **contract
  parity** — five rates × mono/stereo construct, every rejection returns
  `new()`'s exact error string, and all built states drop soundly
  (in-place-initialized memory drops through `FixedVec`'s existing
  length-tracking Drop). Stated coverage limit: the arms are mono, so
  the differential exercises mono decode; stereo is covered by
  construction + drop only, until the gate grows stereo material.
- **Gates.** `make opus-test` **16 green (13 → 16)**; `make check` clean;
  the vendor standalone thumbv4t build (`--no-default-features --features
  libm`, PATCHES.md re-verification recipe) compiles clean with patch 2;
  the vendor default-features host build compiles clean from a
  gate-neutral cwd with an explicit host `--target` (proves the
  `heap`/`std` branches of the new code; the repo-root-config-leaked
  default-features build keeps its documented expected-failure shape —
  same std-coupling error class as the pre-patch baseline, OPUS.md
  survey); vendor `cargo fmt --check` drift is unchanged from the
  pre-patch baseline (16 pre-existing diffs in upstream-authored code
  only — the patched files add zero).
- **Prior-assumption corrections this step:** none — no documented claim
  measured false. Every size cited in the patch docs was re-measured on
  this tree (host `size_of` through a throwaway probe crate: 178,064 /
  81,952 / 9,304 / 3,992 B), not recalled.

## Status (2026-10-06): seam wired to the in-place ctor + vendor patch 3 — construction measured CLEAN on target; SILK arm green, CELT arm's death moves to the decode path

- **What lands:** (1) `walk_region` now constructs via
  `probe::new_decoder_in_place` → `OpusDecoder::new_in_place` (vendor patch
  2), replacing the `Box::new(new_decoder(..)?)` shape; `new_decoder_boxed`
  stays as the recorded measured negative, `new_decoder` stays for the host
  differential. (2) The constructor-shape probe ROM at **rev9**: P4 carries
  the landed fix shape, every stage is `#[inline(never)]`, and TAIL (the
  by-value repro control) moved after the verdict so every
  information-bearing stage prints before the expected death. (3) **Vendor
  patch 3** — in-place CELT mode construction (`OnceCell::get_or_init_in_place`,
  `CeltMode::init_in_place`, `MdctLookup::init_in_place`,
  `FixedVec::init_empty_at`; PATCHES.md patch 3 carries the inventory).
  Vendor diff against the published crate is now exactly patches 1+2+3.
- **New finding (the reason patch 3 exists).** Patch 2 alone did NOT clear
  construction. Rev9 pinned per-stage frames and read `sp@P4 = 0x03007DC8`
  **inside** IWRAM — P4's own frame is `sub sp, #0xb4` (disassembly) — yet
  the ROM still died *inside* the `new_in_place` call: the frame was in a
  callee. Disassembly of the literal-pool `add sp, rN` prologues convicted
  the lazy CELT mode table: `OnceCell<CeltMode>::get_slow` carries a
  **65,092 B frame** (literal `0xffff01bc`; `CeltMode` 32,296 B + nested
  `MdctLookup::new` return 32,072 B + slack — host sizes measured in a
  throwaway probe crate, arithmetic derived). Rev8's reading had been
  convicted by an LTO-union entry frame — the per-stage `inline(never)` is
  what made the witness separate the stages; the wrapped entry frame also
  *aliased into EWRAM*, so rev8's P4 death was aliasing, not the candidate.
- **Measured on-target (post-patch-3 images: proof `b4c26633…`, probe
  `6eeeb25f…`):**
  - Probe: **P4 SURVIVED** — `sp@P4` inside IWRAM, decoder box `0x02008370`
    in EWRAM, `dropped ok`. The construction-shape hang is closed.
  - **SILK arm green on-target for the first time** (probe P2 and the proof
    ROM, same golden): region FNV MATCH, full 501/501 packet walk, totals
    OK (480,960 samples), heap placement OK, and the walk-fold golden
    `fnv_walk_fold = 0x79E9DCEABA485BEE` **MATCH** — the three-link chain
    (region hash → target decode = host walk → host witness vs reference)
    is now complete for SILK.
  - **CELT (music) arm still dies after construction**, before packet 1
    (proof ROM: jump trap `042BBC88`). This is a different problem class:
    the decode path's **stack budget**, not construction. Measured frame
    audit of the same linked image (literal-pool decode of every
    `add sp, rN` / `sub sp, #imm` site ≥ 1024 B):
    `probe::decode_packet` **18,140** (CELT machinery inlined into it),
    `bands::quant_all_bands` **8,316**,
    `pvq::pvq_search_fast_select` **7,132**,
    `CeltDecoder::decode_impl_from_rc` 4,988, `SilkDecoder::decode` 5,196.
    Nested CELT chain 18,140 + 8,316 + 7,132 = 33,588 B **exceeds the
    ~32,512 B stack** (agb sets SP ≈ 0x03007F00; IWRAM bottom 0x0300_0000)
    — same wrap class, different frame. SILK's chain (18,140 + 5,196)
    fits, matching which arm survives.
- **Gates.** `make opus-test` **16 green** (the in-place differential, the
  committed goldens, and contract parity now all drive construction through
  patch 3's path); `make check` clean; vendor standalone thumbv4t build
  (`--no-default-features --features libm`) clean; vendor `cargo fmt --check`
  drift unchanged from the 16-diff pre-patch baseline (all upstream-authored;
  the patch 3 files add zero).
- **Prior-assumption corrections this step:** patch 2's PATCHES.md scope
  note predicted the proof ROM “now survives construction” once wired —
  **measured false until patch 3**: construction survived only after the
  mode-table frame was removed too (the patch inventory grew 1+2 → 1+2+3
  as a result). And the frame audit shows the by-value-return survey is not
  complete even now: the decode path's large `MaybeUninit` stack buffers
  (MDCT `f_buf`/`f2_buf`, band workspaces) materialize as frames under
  `lto = "fat"` regardless of construction shape.
- **Next PR (named, correctness-before-speed order holds):** the CELT
  decode-path stack. First move is a frame-reduction experiment, not a
  vendor surgery guess: the CELT hot chain's big stack buffers live in
  `mdct::forward` (`f_buf`/`f2_buf` ≈ 7.7 KB combined, inlined into
  `decode_packet`'s 18,140 B frame) and in `quant_all_bands` /
  `pvq_search_fast_select`; candidates are `#[inline(never)]` boundaries
  (cheap, non-semantic, splits frames — but nesting still sums) versus
  scratch-buffer ownership (thread a caller-owned scratch through the CELT
  decode, patch-shaped, EWRAM-backed). Witness already exists: probe P2's
  music walk printing packet 1, then the proof ROM's music-arm golden
  `fnv_walk_fold = 0x0C01DDF101FAFA3F` MATCH — the same two checkpoints
  that turned SILK green here. Cycle counting stays ordered behind both arms green.
