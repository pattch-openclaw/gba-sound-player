# E7 — `-Cforce-frame-pointers=yes` flag-flip probe (result: **inert**)

Menu item: FLAC.md "Perf follow-ups" E7. Full result entry: FLAC.md
"E7 result — the flag is inert: thumbv4t-none-eabi forces frame pointers
(2026-09-25)".

**What the probe does.** One-variable build-flag probe: rebuild the
identical spike source with the `-Cforce-frame-pointers=yes` line removed
from the repo-root `.cargo/config.toml`, and compare. No decode-path code
changes — the variant is purely the rustflag line.

**Measured 2026-09-25: the flag is a no-op on this target.** The control
(flag ON, committed tree) and the variant (flag removed, clean rebuild)
produce the **byte-identical ROM** — same sha256, and an mgba serial log
`cmp`-identical to the control's. The `thumbv4t-none-eabi` target spec
itself sets `"frame-pointer": "always"`, so the frame-pointer cost is not
rustflag-addressable; `-Cforce-frame-pointers=no` is equally inert (proved
on a minimal `no_std` probe crate: asm byte-identical either way, prologues
`push {…, r7, lr}` + `.setfp r7` in both). There is no measurement to
compare because there is no second image.

## Files

- `e7-frame-pointer-flag.patch` — the variant: the one-line removal from
  `.cargo/config.toml`, against `main` @ `6a6beb4`. Verify with
  `git apply --check` at that base.

## Re-run recipe

Base: `main` @ `6a6beb4` (or later until `.cargo/config.toml` moves).

1. **Control** — committed tree, flag present:
   `make native-spike-rom` →
   `shasum -a 256 flac-spike.gba` must be
   `0d97659165b2900600305ee1d7438f90c9f3e05d479068431ef7b6cc6bfeedb3`
   (bit-identical to PR 3's recorded image; rebuild-hashes-stable).
2. **Variant** — `git apply examples/flac_spike/experiments/e7-frame-pointer-flag/e7-frame-pointer-flag.patch`,
   then a clean rebuild of the bare-metal dir (cargo does not key freshness
   on config-file *content* changes at this level — do not trust an
   incremental rebuild):
   `rm -rf examples/flac_spike/target/thumbv4t-none-eabi && make native-spike-rom`.
   Prove the flag really is absent from the build:
   `cd examples/flac_spike && rustup run nightly cargo build --release --target thumbv4t-none-eabi -v 2>&1 | grep -c force-frame-pointers`
   must print `0`.
3. **Reading** — the variant sha256 comes back **identical to the control**
   (that is the result; no run needed — an identical image cannot print
   different numbers). If a future toolchain ever makes the flag effective
   (shas differ), *then* run both images:
   `mgba-test-runner flac-spike.gba > log 2>&1 &`, poll for the
   `PR 3 verdict` line, kill (the runner hangs on the gfx loop — expected),
   `grep -E "stats:|budget:|verdict" log`; two headless runs `cmp`-identical
   per image; net each by its own calibration.

## Values a faithful re-run reproduces (2026-09-25, this host)

- Control ROM sha256 `0d97659165b2900600305ee1d7438f90c9f3e05d479068431ef7b6cc6bfeedb3`;
  variant (flag removed) sha256: **identical**.
- Decode proof both arms: pcm fnv `0x54C7B356621B6E15` [MATCH].
- Stats (net, overhead 22): FIXED
  `count=157 min=585301@156 max=2314473@60 sum=361095055`; LPC
  `count=157 min=1506681@156 max=6901384@136 sum=1077175030` — exact
  reproduction of PR 3's recorded numbers (same image bits).
- Mechanism: `rustc --target thumbv4t-none-eabi --print target-spec-json
  -Z unstable-options | grep frame` → `"frame-pointer": "always"`.

## Provenance

Rust nightly 1.100.0-nightly `a69a63265` (2026-09-03, LLVM 23.1.1) via
rustup; agb 0.25.0 `mgba-test-runner` (mGBA 0.10.5); runs 2026-09-25;
control log 675 lines, two runs `cmp`-identical; variant-built image
`cmp`-identical to the control log.
