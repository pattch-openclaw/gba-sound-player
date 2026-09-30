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
off (gotcha 1): decoder state is inline (~178 KB, pinned by the host test);
the Box-via-`alloc` EWRAM patch is a later PR, tracked in OPUS.md.

## Re-verification recipe

From this directory, with the pinned nightly:

```sh
cargo +nightly build --release --no-default-features --features libm \
  --target thumbv4t-none-eabi
# → expect: builds clean (patch applied). Reverting Patch 1 reproduces E0432.
```

## Deliberate omissions (tracked, not forgotten)

- **No heap/EWRAM patch yet** (state is inline; OPUS.md gotcha 1).
- **No `#![forbid(unsafe_code)]`** — upstream contains `unsafe` (SIMD probes,
  OnceCell); vendoring does not sanitize it. The spike crate itself forbids
  unsafe in its own sources.
- x86 SIMD paths exist upstream but are cfg'd off on this target; they are
  not deleted (byte-comparability with upstream is worth more than a few KB
  of unread code in a vendored copy).
