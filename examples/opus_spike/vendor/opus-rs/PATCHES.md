# opus-rs vendor patch inventory

**Upstream:** `opus-rs` 0.1.34 (crates.io), <https://github.com/restsend/opus-rs>,
author jinti (shenjindi@fourz.cn). License: **BSD-3-Clause** (`COPYING` carried
alongside this file; upstream `Cargo.toml` records the license).

This is the decoder OPUS.md's survey identified as the only viable path for
`thumbv4t-none-eabi` (`core` + `alloc` only, zero FFI). The vendored copy is
the crates.io `src/` **plus exactly the patches below** — nothing else was
edited. `src/` is otherwise byte-comparable to the published crate.

## Patch 1 — `compat.rs`: `AtomicU8` via `portable_atomic` (the OPUS.md fix)

ARMv4T's `core` has no `AtomicU8` (LLVM atomics for thumbv4t cover natural
word-sized types only), so the published crate fails with:

```text
error[E0432]: unresolved import `core::sync::atomic::AtomicU8`
  --> opus-rs-0.1.34/src/compat.rs:15:26
```

`src/compat.rs` swaps the import:

```diff
- use core::sync::atomic::{AtomicU8, Ordering};
+ use portable_atomic::{AtomicU8, Ordering};
```

and `Cargo.toml` gains:

```toml
portable-atomic = { version = "1", default-features = false }
```

This is the patch OPUS.md ("Candidate: `opus-rs` 0.1.34") documented and
compile-probed in /tmp on 2026-09-27 — with **one correction found by the
first real link** (note 1a below). Vendoring it here proves the claim inside
the repo tree. On the host target the atomic is native; on thumbv4t
`unsafe-assume-single-core` (from agb) or portable-atomic's `fallback` CAS
(with `fallback` as a last resort) carries it.

### Note 1a — `critical-section` feature is INCOMPATIBLE with agb (measured 2026-09-29)

OPUS.md's sketch added `features = ["critical-section"]`. Inside this repo
that combination **cannot link**: agb 0.25.0 declares

```toml
[target.'cfg(all(target_arch = "arm", ...))'.dependencies]
portable-atomic = { version = "1.6.0", features = ["unsafe-assume-single-core", "fallback"] }
```

so every thumbv4t link that includes agb enables
`unsafe-assume-single-core`, and portable-atomic's own
`compile_error!` (src/lib.rs:582) forbids `critical-section` alongside that
cfg — the first `make native-opus-rom` died on exactly that error. Dropping
to `fallback` alone then failed the *standalone* vendor build the other way
(`no AtomicU8 in the root`): `fallback` only provides the type when some
single-core/locking assumption is present.

Two more `compile_error!`s narrowed the fix, one per direction:

- `unsafe-assume-single-core` **unconditionally** fails the host gate
  (`not supported yet on this architecture` on aarch64 — the cfg is ARM
  bare-metal-only).
- `fallback` alone provides **no `AtomicU8` on thumbv4t** without a
  single-core/locking assumption (`no AtomicU8 in the root`).

**Resolution (verified 2026-09-29 across all three builds): mirror agb's own
target-gating.** The vendor manifest declares bare `portable-atomic` for the
host (native `AtomicU8`, no feature needed) and agb's exact set —
`unsafe-assume-single-core` + `fallback` — under
`cfg(all(target_arch = "arm", target_os = "none"))`. In the ROM link this
unifies with agb's identical declaration (no conflict; the GBA *is*
single-core and agb already makes that unsafe assumption platform-wide);
standalone (no agb in the graph, OPUS.md's probe recipe) the same gate
provides `AtomicU8` for thumbv4t. The dependency family is still not new:
flac_spike's lockfile already carries `portable-atomic 1.15.0` +
`critical-section 1.2.0` (via agb's stack).

## Patch 2 — in-place decoder construction (`new_in_place` / `init_in_place`)

**Problem (measured, not assumed).** The proof ROM died at decoder
construction: the walk banner printed, packet 1 never did, mGBA ended at a
wild PC — while the identical seam passed the host gate. The
constructor-shape probe ROM (spike `src/ctor_probe.rs`, rev1–rev7;
OPUS.md 2026-10-04 status) localized it: the heap is sound (descending
capacity ladder all-clean), and *every* caller-side spelling of
"construct then `Box`" dies with SP read at 0x02FAF610 — ≈363 KB below
agb's stack top, i.e. two decoder-sized sret temporaries of frame. The
frame wraps off IWRAM's bottom (0x0300_0000) into **mapped** EWRAM: the
writes succeed silently, destroying `.bss`/heap/allocator state, until the
corrupted return jumps to garbage. Caller-side elision cannot fix it —
`Box::new(Ctor::new()?)` with `#[inline(always)]` destination propagation
is the probe's measured negative — because the by-value hops are *inside*
the constructor chain:

| By-value return | Host `size_of` (measured, libm/no-default profile) |
|---|---|
| `OpusDecoder::new` → `Self` | **178,064 B** (177,864 on target — ROM serial log) |
| `CeltDecoder::new` → `Self` | **81,952 B** |
| `SilkDecoder::new` → `Self` | 9,304 B |

**The fix: invert the construction direction.** Four additive APIs write
the state field-by-field through a destination pointer, so no `Self`
temporary materializes at any level:

- `fixedvec.rs` — `FixedVec::init_fill_at(dst, len, value)`: in-place twin
  of `from_value` (writes `len = 0` first, making the place a valid empty
  `Self`, then fills through `&mut` — same final value, zero temporary).
  All 25 `FixedVec` fields on the construction path go through it.
- `celt.rs` — `CeltDecoder::init_in_place(dst, mode, channels, sampling_rate)`:
  every field written exactly as `new()`'s literal writes it; the channel
  assert runs before any write.
- `silk/dec_api.rs` — `SilkDecoder::init_in_place(dst)`: the two
  `SilkDecoderState`s are written as `default()` values into the
  destination and `silk_init_decoder` runs on `&mut` there — exactly what
  `new()` does; the largest residual temporary is `SilkDecoderState::default()`
  at 3,992 B (20× under the 82 KB hop, 45× under the 178 KB one).
- `lib.rs` — `OpusDecoder::new_in_place(rate, channels) -> Result<Box<OpusDecoder>, &str>`:
  validates with `new()`'s exact guards **before** `Box::new_uninit()`,
  initializes through `OpusDecoder::init_in_place` (a safe wrapper over it
  carries the `unsafe` contract in doc comments), then `assume_init`.
  `OpusDecoder::init_in_place` is also exposed for callers that own the
  destination (it mirrors `new()`'s `Self` literal field-for-field,
  including the SILK `init` + `fs_api_hz` fixup).

The no_std build gains `extern crate alloc` + `use alloc::boxed::Box`
(gated `#[cfg(not(feature = "std"))]`). This is the std-decoupled heap
pattern OPUS.md gotcha 1 asked for, in miniature: the std-coupled `heap`
feature still stays **off** on target; the Box is the construction
container, not the cfg-gated field layout. The `heap`/`std` branches of
the new code mirror `new()`'s boxed fields verbatim, so the host default
profile keeps compiling (verified: a `--target aarch64-apple-darwin`
default-features build from a gate-neutral cwd is green — the repo-root
config otherwise leaks thumbv4t; the leak-target default build keeps its
documented expected-failure shape, OPUS.md survey, not a regression).

**Invariants.** `new()` is byte-for-byte unchanged; the patch is purely
additive (git diff against the pre-patch tree: insertions only) and edits
zero SILK/CELT/MDCT/range-coder/tables logic — every decode-path byte is
untouched. A future vendor sync must reproduce exactly Patch 1 + Patch 2 +
Patch 3 (this file).

**Witness (`opus_spike/tests/in_place_ctor.rs`, host gate, 3 tests).**
The patch is pure construction, so the witness proves the in-place ctor
builds *exactly the decoder `new()` builds*:

1. **Bit-identical differential** — both arms (501 sequential packets
   each) decoded side by side through the two ctor shapes, raw f32
   compared bit-for-bit. Two decoders of the same code on one machine have
   no drift to tolerate; any initialization difference diverges the state
   machine. This exercises every field the decode path reads.
2. **Committed goldens** — the in-place walk reproduces each arm's
   generator-measured `fnv_walk_fold` (the ROM decode proof's golden,
   measured through the *old* construction). Layer 1 alone could share a
   construction mistake with `new()`; this layer pins against ground truth.
3. **Contract parity** — five rates × mono/stereo construct; rejections
   return `new()`'s exact error strings (nothing allocates on the error
   path); all ten built states drop soundly.

Coverage limit, stated: the embedded arms are mono, so the decode
differential exercises mono; stereo state is written by the same
parameterized path and covered by construction + drop only, until the
gate grows stereo material.

**Scope boundary (what this patch does NOT claim).** Host witnesses prove
state equality between the two construction shapes; the on-target claim
— "the proof ROM now survives construction" — is the *predicted*
consequence (no 82 KB / 178 KB temporary can materialize because no
by-value return exists on the path) and gets its witness in the next PR:
integrate `walk_region` to `new_in_place`, rebuild the proof ROM, and
re-run the constructor-shape probe as the before/after witness
(OPUS.md 2026-10-04 designated it exactly that). No ROM was built or
booted for this PR — lib patch, host gates only, by scope.

**Upstreamability.** The pattern is what libopus itself does
(`opus_decoder_create` initializes into a user-provided buffer); the
author's `heap` feature already boxes this state, only std-coupled. This
patch is the alloc-native version of both.

## Patch 3 — in-place CELT mode construction (`get_or_init_in_place`)

**Problem (measured, not assumed).** Patch 2 removed every decoder-sized
by-value hop, yet the probe ROM (rev9, per-stage `#[inline(never)]` frames)
read `sp@P4` **inside** IWRAM and still died *inside* the `new_in_place`
call — the frame was in a callee. Disassembly of the measured image
convicted the next by-value return on the path: the **lazy CELT mode
table**. `init_in_place` starts at `modes::default_mode()`, whose cold
`OnceCell::get_slow` prologue (`add sp, r6`, literal `0xffff01bc`) is a
**65,092 B frame** — `CeltMode::new_48000_960_120` returns its `Self`
(32,296 B, host-measured in a throwaway probe crate) through
`(*storage).write(init())`, and the nested `MdctLookup::new` return
(32,072 B) materializes inside it. First construction on a 31 KB stack:
same wrap class as patch 2's original conviction.

**The fix: no temporary on the lazy path either.** Four additive APIs, same
invert-the-direction pattern:

- `compat.rs` — `OnceCell::get_or_init_in_place` (+ `#[cold] get_slow_in_place`):
  the initializer receives the storage **pointer**; the value is written
  through it, never as a `T` temporary. Same state machine (0/1/2, same
  CAS claim, same Release publish), same contract (`init_at` runs at most
  once, fully initializes, must not panic).
- `fixedvec.rs` — `FixedVec::init_empty_at`: writes `len = 0` into an
  uninitialized place, making it exactly `new()`'s empty value (buf is
  `MaybeUninit`), without a `Self` temporary.
- `mdct.rs` — `MdctLookup::init_in_place(dst, n, max_lm)`: both `FixedVec`
  fields become valid-empty via `init_empty_at`, then `new`'s push loop runs
  verbatim through `&mut` on the destination. Largest residual temporary:
  the per-level `Option<KissFftState>` (4,872 B host; 4,884 B target frame
  measured in the post-patch audit — over 13× under the removed frame).
- `modes.rs` — `CeltMode::init_in_place` mirrors `new_48000_960_120`'s
  literal field-for-field (the `mdct` field through the mdct in-place init;
  every remaining write is a `Copy` scalar or a `&'static` table reference),
  and `default_mode_in_place()` fills the **same shared cell** via
  `get_or_init_in_place`. Whichever entry point runs first wins; every later
  call — `default_mode()` in decode paths included — takes the cell's fast
  path (`state == 2`, no initializer frame ever runs there).

`lib.rs` patch 2's `init_in_place` switches its seam line from
`modes::default_mode()` to `modes::default_mode_in_place()`. That is the
**only line this patch edits**, and it is a line patch 2 itself added —
confirmed absent at `34a8870` (pre-patch-2). Upstream bytes stay
byte-comparable; the patch remains purely additive against upstream.

**Witnesses.** Host: `tests/in_place_ctor.rs` (all three layers) now drives
construction **through** this path — the bit-identical differential,
the committed `fnv_walk_fold` goldens, and contract parity all exercise the
in-place mode construction (16 green). On-target (this PR's images, proof
`b4c26633…`, probe `6eeeb25f…`): probe P4 **SURVIVED** — `sp@P4 =
0x03007DC8` inside IWRAM, decoder box `0x02008370` in EWRAM — and the SILK
arm's full 501-packet walk reproduced the golden on hardware-emulated
target. The construction path is measured clean to depth.

**Stated limit.** The CELT (music) arm still dies **after** construction —
the CELT *decode path's* stack budget (frame audit in OPUS.md 2026-10-06:
`decode_packet` 18,140 + `quant_all_bands` 8,316 + `pvq_search_fast_select`
7,132 > 32,768 B IWRAM), a different problem class this patch neither
claims nor touches. Vendor sync: reproduce Patch 1 + 2 + 3, no more.
*(The stated limit — decode-path stack — is resolved for the music hot path
by Patch 4; the mode-transition latent chain stays open, see Patch 4.)*

## Patch 4 — frame-split the mode-transition PLC bridge (`silk_plc_bridge`)

**Problem (measured, not assumed).** Patch 3's stated limit convicted the
CELT decode-path stack: nested chain `decode_packet` 18,140 +
`quant_all_bands` 8,316 + `pvq_search_fast_select` 7,132 = 33,588 B over the
~32,512 B stack (SP ≈ `0x03007F00`, IWRAM bottom `0x0300_0000`) — same wrap
class as the construction hang, and the music arm indeed died before packet 1
(jump trap). OPUS.md 2026-10-06 attributed the 18,140 B entry frame to the
CELT machinery itself (MDCT `f_buf`/`f2_buf`, band workspaces) inlined under
`lto = "fat"`. **That attribution measured false**: forcing
`mdct::forward`/`mdct::backward` out-of-line left `decode_packet`'s frame at
exactly 18,140 B (variant A, this experiment), while extracting the
**mode-transition SILK PLC bridge** — a cold branch that never runs on the
CELT-only music arm — removed exactly 15,368 B (variant B). Under `lto =
"fat"` a cold branch's locals union into the caller's single entry frame:
the bridge's `plc_i16` (`FixedVec<i16, OPUS_PCM_I16>`, 3,848 B site) and
`resampled` (`FixedVec<i16, OPUS_MAX_FRAME>`, 11,528 B site) storage —
15,376 B, less 8 B union slack — was reserved by `decode`'s `add sp, rN`
prologue on **every** packet, hot or cold.

**The fix: move the cold branch's frame behind a call boundary.** One
extraction, zero semantics: `OpusDecoder::silk_plc_bridge(&mut self,
f5_bridge)` is `decode`'s PLC-bridge block verbatim (15,368 B of frame
removed — the storages' 15,376 B less 8 B slack), marked
`#[inline(never)]`, called from the same guard. Same instructions, same call
graph — only the frame boundary moved, so the two FixedVec temporaries exist
only while the cold branch runs. (`decode` itself stays one function; the
`pub fn decode` signature and behavior are untouched.)

**Witnesses.** Frame audit (literal-pool `add sp, rN` sites, linked image):
`decode_packet` 18,140 → **2,772 B**; music hot chain 2,772 + 4,988 + 8,316
+ 7,132 = **23,208 B < 32,512 B** (was 33,588 — the wrap). On-target (proof
`ff1ab006…`): the music arm walks **501/501 packets for the first time** —
the death is gone; SILK unchanged green (`0x79E9DCEABA485BEE` MATCH). Host:
`make opus-test` 16 green — the committed `fnv_walk_fold` goldens reproduce
**bit-exactly through this path** (zero semantic change).

**New finding this patch surfaced (open, NOT fixed here).** With the walk
alive, the music arm's *values* diverge: on-target fold
`0xA781907825AB00AA` vs host golden `0x0C01DDF101FAFA3F`, first divergent
packet **61** (packets 1–60 bit-identical; stable across two ROM builds and
three host opt levels). Mechanism hypothesis (recorded, unproven): f32
multiply-add **contraction** — the host binary carries 75 fused `fmadd/fmla`
sites, all in the CELT quantizers (`alg_quant`, `quant_all_bands`,
`quant_partition*`, `renormalise_vector`, `alg_unquant`,
`decode_impl_from_rc`); zero in SILK (matching which arm matches bit-exact);
the target is soft-float with no contraction (`compiler_builtins` `__aeabi_*`
only, no FMA in the image). The host's `fnv_walk_fold` golden itself embeds
fused rounding and tolerates 127 ±1-LSB drift samples (assets.rs 2026-10-02),
so a sub-ULP class drift crossing the fold grid at packet 61 is the leading
explanation. Attempted disproofs (all measured identical host output):
`-Cllvm-args=--fp-contract=off` (accepted, zero codegen effect), LTO off,
`opt-level = 0`, vectorizer off — rustc exposes no working fp-contract knob
and no soft-float host target is installed here (Rosetta absent). The next
OPUS.md step (OPUS.md 2026-10-08) is the witness that settles it: per-packet
fold comparison on-target, and/or a soft-float host target.

**Stated limit.** The **mode-transition chain** (music packet following a
SILK/Hybrid packet) now nests `decode_packet` 2,772 + `silk_plc_bridge`
21,396 + `SilkDecoder::decode` 5,196 + `silk_plc` 4,716 = **34,080 B >
32,512 B** — latent (both embedded arms are single-mode, so no packet takes
that path; SILK's own chain 2,772 + 5,196 still fits). Same union-frame
class one level deeper; the cheap fix is boxing the two FixedVec temporaries
inside the bridge (cold path, EWRAM allocator proven live by probe P3/P4).
Vendor sync: reproduce Patch 1 + 2 + 3 + 4, no more.

## Patch 5 — gate aarch64 SIMD behind an opt-in `host-simd` feature (the packet-61 witness)

**Problem (measured, closed).** Patch 4 left the music arm alive but RED:
on-target fold `0xA781907825AB00AA` vs host golden `0x0C01DDF101FAFA3F`,
first divergent packet **61** (packets 1–60 bit-identical; 441/501 hashes
differ). OPUS.md 2026-10-08 recorded f32 mul-add contraction as the leading
hypothesis (fused-site census: 75 sites in the host binary, all CELT
quantizers, zero SILK) and named the witness that settles it: a strict
non-fused host walk matching the ROM's per-packet hashes from packet 1.
This patch **is** that witness, landed.

**Mechanism correction (measured).** The fused sites are **not LLVM
contraction** — they are the published crate's own aarch64 SIMD paths:
explicit `vfmaq_f32` (NEON fused multiply-add) inside upstream-authored
quantizer/resynth/MDCT/FFT/pitch kernels, gated behind
`#[cfg(target_arch = "aarch64")]`. That is why every compiler-flag
disproof measured inert: no flag unfuses an intrinsic. The host (hard-float
aarch64) executes those NEON paths; the GBA never can (thumbv4t:
soft-float `__aeabi_*`, no FMA, NEON never compiles). The resulting
sub-ULP drift crosses the round-half-up fold grid at packet 61 and flips
every packet hash after it.

**The fix: the host's decode path becomes an opt-in choice, default = the
product's equivalence class.** Every `target_arch = "aarch64"` cfg
predicate in `src/` (140 sites, 9 files) becomes
`all(target_arch = "aarch64", feature = "host-simd")`; the manifest gains
`host-simd = []`, **default OFF**. OFF (the consumer's pinned profile):
the host executes the crate's existing scalar f32 paths — the class the
ROM lives on. ON: upstream 0.1.34 host behavior, bit-for-bit (the
comparability witness). The 4 lint `cfg_attr`s are untouched (not code
paths). Nine cfg attributes needed 4-line rustfmt wraps, hand-edited (the
tree is never `cargo fmt`-ed); those wraps shift only embedded
panic-location line numbers in the linked image (+3/+6), never
instructions — witnessed below.

**Witnesses.**
- *Mechanism:* strict (feature-OFF) `dump_walk` of the music arm folds to
  `0xA781907825AB00AA` — **exactly the ROM's** — with **501/501
  per-packet running hashes matching the ROM log** (packet 61 included);
  the strict binary's fused-site census is **0** (control: 75). SILK is
  unchanged (`0x79E9DCEABA485BEE`, 0 residuals) — it never took a fused
  path, matching which arm stayed bit-exact.
- *Zero target effect:* proof ROM rebuilt with patch 5 alone (before the
  pin re-measure) is **byte-identical** to the patch-4 ROM (`ff1ab006…`)
  — the gating changes no thumbv4t codegen.
- *Golden re-measured through the strict path* (generator): four asset
  blobs byte-identical; only the music pins move — `fold_mismatch` 127 →
  **126** (all ±1 LSB, align_shift 312 unchanged), `fnv_walk_fold` →
  **`0xa781907825ab00aa`**. The final ROM differs from the patch-only ROM
  in exactly **50 bytes**: 9 pin-data bytes + 41 embedded panic-location
  line numbers shifted by the cfg wraps; `probe::decode_packet` still @
  `0x08025E0D` — zero instruction delta.
- *On target:* **both arms green for the first time** (proof ROM
  `fbe2a6b6…`): silk MATCH, music `0xA781907825AB00AA` MATCH, PR 3
  verdict PASSED.
- *Upstream comparability:* a `host-simd`-ON rebuild reproduces **both**
  pre-patch goldens bit-exactly (music `0x0C01DDF101FAFA3F`, silk
  unchanged).
- *Gates:* `make opus-test` 16 green (the witness re-derives the new pins
  on the strict path); `make check` clean; vendor thumbv4t standalone and
  host default-features builds clean; fmt drift unchanged at the 17-diff
  upstream baseline (per-file counts).

**Stated limit.** `host-simd` ON is a comparability mode, not the
product's class: its music golden embeds fused-NEON rounding the GBA
cannot reproduce — never repin the gate to it. Patch 4's mode-transition
chain latent stays untouched: parked by product decision (2026-10-09) —
ROM audio is packaged mode-pure, so no shipped stream takes the bridge.
Vendor sync: reproduce Patch 1 + 2 + 3 + 4 + 5, no more.

## Manifest changes (this `Cargo.toml`)

Reconstructed from the published normalized manifest with:

- dev-dependencies / examples / tests / benches stripped — the vendored copy
  is a decoder dependency, and upstream's dev-suite needs std + C interop
  crates that cannot resolve here;
- the `portable-atomic` dependency added (Patch 1);
- `[workspace]` — this crate is deliberately its own workspace root so
  OPUS.md's standalone probe recipe (`cargo +nightly build --release
  --target thumbv4t-none-eabi -Zbuild-std=core,alloc`) runs from this
  directory; the consumer crate (`../../Cargo.toml`) `exclude`s it from its
  own workspace to avoid two roots colliding.

## Build profile the consumer pins

`default-features = false, features = ["libm"]` — exactly the OPUS.md
compile-probe profile. The `heap` feature is std-coupled upstream and stays
off (gotcha 1): the field layout stays inline (~178 KB, pinned by the host
test). Patch 2 supplies the missing construction shape — the state is
built directly inside a `Box` (the EWRAM heap on target) with no by-value
hop — while wiring the production seam to it remains a tracked follow-up
in OPUS.md.

## Re-verification recipe

From this directory, with the pinned nightly:

```sh
cargo +nightly build --release --no-default-features --features libm \
  --target thumbv4t-none-eabi
# → expect: builds clean (patch applied). Reverting Patch 1 reproduces E0432.
```

## Deliberate omissions (tracked, not forgotten)

- **The std-coupled `heap` field layout is not adopted** (OPUS.md gotcha
  1): fields stay inline; Patch 2 boxes the *construction* instead, which
  is what the target actually needed. Production-seam integration of
  `new_in_place` is the tracked follow-up (OPUS.md 2026-10-05).
- **No `#![forbid(unsafe_code)]`** — upstream contains `unsafe` (SIMD probes,
  OnceCell); vendoring does not sanitize it. The spike crate itself forbids
  unsafe in its own sources.
- x86 SIMD paths exist upstream but are cfg'd off on this target; they are
  not deleted (byte-comparability with upstream is worth more than a few KB
  of unread code in a vendored copy).
