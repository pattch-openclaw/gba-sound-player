# E6 sample — hot decode code pinned to IWRAM (measured 2026-09-26, **not integrated**)

The reappliable sample code for E6 in FLAC.md's "Perf follow-ups —
experiment menu" (code placement — the one dimension E1–E4 structurally
could not touch). **Result: pinning the hot decode path to IWRAM is
decisively the wrong direction** — the FIXED arm is **17.8% slower**
(217.4% → 256.1% of the 1,048,750-cycle budget, 1,113 → 1,312 c/sample)
and the LPC arm **9.0% slower** (652.3% → 711.0%). The menu's reading rule
demotes E6: instruction-fetch location is not the cost center, and code
placement joins the ruled-out list. Full findings, witnesses, and the
mechanism correction (`.text_iwram` is a trap — plain `.iwram` is the
working pin target):
[`../../../../FLAC.md`](../../../../FLAC.md) →
"E6 result — IWRAM code pinning is slower on both arms (2026-09-27)".

This directory is **sample code only**: the probe never lands on `main`'s
spike sources. The live `src/` stays exactly as PR 3 shipped it.

## What the probe does

Two IRQ-off measurement windows per frame, on one image (the E2a/E4
shape), with **code placement as the only source-level variable**:

1. **ctrl** — PR 3's `driver::decode_one` (the verbatim i64 path on the
   position-only `BitReader`), the same-run control;
2. **variant** — `e6::decode_one`: a **verbatim vendored copy** of the hot
   path (`e6.rs`: reader, header/subframe/residual decode, Rice loop, both
   integrators — same arithmetic, same unary loop, same rejection rules)
   whose hot functions carry
   `#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]`:
   - **pinned spine** (`inline(never)`): `decode_frame`, `decode_subframe`,
     `decode_residual`, `decode_rice_partition`, `integrate_fixed`,
     `integrate_lpc` — 4,732 B total in IWRAM;
   - **pinned inline leaves** (`inline(always)`): the reader primitives +
     `rice_unmap`/`pad_block`/`fill_state`, so the per-bit loop carries no
     extra call vs the LTO-inlined control;
   - **unpinned** per-frame plumbing (header parse, `decode_one` wrapper,
     the shared `stereo::decorrelate` — a real shared call, E2a's vendor
     boundary) stays in ROM.

**Measured correction (the menu named the wrong section):** gba.ld's
`.text : { *(.text .text*) } > rom` matches `.text_iwram` before the
`.iwram` output section can claim it (GNU ld assigns an input section to
the first matching output section) — with the menu's attribute the symbols
stayed at `0800xxxx`. Plain `.iwram` is agb's own IWRAM-code section and
lands correctly under `> iwram AT>rom`, copied at boot by `CommonInit`'s
BIOS `CPUSet`.

Witnesses before any number counts:

- **Host** (`tests/e6_witness.rs`, runs under `make spike-test`): the
  vendored path decodes the exact embedded clips bit-exactly vs `flac -d`,
  and differentially frame-by-frame against the production
  `driver::decode_one`.
- **ROM**: per-frame variant-vs-control **sample-equality** — 314/314
  frame-windows equal (`eq=1`), checksum delta 0; first offender named.
- **Link witness**: `nm` on the built ELF — the six pinned spine
  functions from `decode_frame` (`0x0300_022c`) through
  `decode_rice_partition` (`0x0300_1178`, size `0x330`); `.iwram`
  section total `0x1540` = 5,440 B (pinned code 4,732 B + agb's own
  IWRAM helpers + interworking thunks) of the 32,512 B budget; boot log
  prints `pinned_rice=0x03001179`.
- **Calibration**: E4's verified shape in both passes; overhead 24,
  stable, nets 0.

## Files

| File | What |
|---|---|
| `e6-iwram-code-pinning.patch` | `git diff` of the probe against `main` @ `7d8b412` (E7 docs merge), touching `examples/flac_spike/src/{e6.rs (new), lib.rs, main.rs}` + `tests/e6_witness.rs` (new). Byte-identical to `git diff 7d8b412 bd48b04` (the measurement commit). Verified `git apply --check` clean at that commit. |

## Re-applying

```sh
# from the repo root, on a tree at (or near) the recorded base
git apply examples/flac_spike/experiments/e6-iwram-code-pinning/e6-iwram-code-pinning.patch
make spike-test && make native-spike-rom
# two headless runs must be byte-identical (determinism witness):
for i in 1 2; do (mgba-test-runner flac-spike.gba > /tmp/e6_$i.log 2>&1 &
  P=$!; until grep -q verdict /tmp/e6_$i.log; do sleep 1; done; kill $P 2>/dev/null); done
cmp /tmp/e6_1.log /tmp/e6_2.log && echo IDENTICAL
grep -E "e6 (ctrl|variant|summary)|e6 witness" /tmp/e6_1.log | head
# undo:
git apply -R examples/flac_spike/experiments/e6-iwram-code-pinning/e6-iwram-code-pinning.patch
```

If the patch no longer applies cleanly (the harness moved — PR 4/5 touch
`main.rs`), take the **two-window + equality-witness + unified-calibration**
shape from the patch rather than forcing the hunks; `e6.rs` itself is a
standalone module and should port verbatim. **Build the control from a
separate worktree at the base commit** (`git worktree add`) and require its
stats to reproduce the recorded baseline exactly — across trees the stats
witness judges the control, never the ROM sha.

## Measured values (what a faithful re-run should reproduce, modulo toolchain)

mGBA `mgba-test-runner` 0.10.5, runs 2026-09-26 on the same host runner as
PR 3 / E1 / E2a / E4 / E7, Rust nightly 1.100.0-nightly (a69a63265
2026-09-03). Variant ROM sha256
`ff3a2f56a84aa77197dc2d4c1b355bb59a0e87bbbd0d59f6d8b85a8ab3ea30f9` (two
runs `cmp`-identical, 1006 lines). Control ROM built from a base worktree
@ `7d8b412`, sha256
`65cd0c36e9e204ebbaf6a9376f20fb81ecc3936ffc617179fa908482b0c41e84` (two
runs `cmp`-identical, 675 lines; per the cross-tree rule its **stats**
reproduce the baseline exactly — serial log `cmp`-identical to the E7
control run — while the sha differs per build tree).

| arm | ctrl sum | variant sum | delta | ctrl mean %budget | variant mean %budget | variant worst %budget |
|---|---|---|---|---|---|---|
| FIXED (`l0_stereo`) | 357,908,871 | 421,705,665 | **+17.8% (slower)** | 217.4% | 256.1% | 257.8% |
| LPC (`l4_stereo`) | 1,073,988,846 | 1,170,737,712 | **+9.0% (slower)** | 652.3% | 711.0% | 715.7% |

Net of this image's own calibration (overhead 24, stable, nets 0, both
passes). 157 frames / 320,000 samples per arm; budget 1,048,750
cycles/frame ≈ 512 c/sample — variant c/sample: FIXED 1,311.5 (ctrl
1,113.1), LPC 3,641.1 (ctrl 3,340.2). Control reproduces the recorded
baseline shape exactly: worst-frame indices **exact** (FIXED @60, LPC @136,
min @156 both arms); this image's PR 3-shape pass printed FIXED
sum=357,906,830 max=2,294,068@60 and LPC sum=1,073,986,805
max=6,880,979@136 — the e6 ctrl arm sits a constant +13 cyc/frame above it
(+2,041 sum, code layout), the same offset E2a recorded on its image. The
variant's worst frame moves index (FIXED @60 → @4, LPC @136 → @145,
deterministic across both runs) — the mean, not the worst, is the reading.
Decode proof matched the PCM pin `0x54C7B356621B6E15` both arms before any
timing.

Gates green on the patched tree: `make spike-test` (host tests 23 → 25:
lib 15 unchanged, +2 `tests/e6_witness.rs`, `spike_witness` 8 unchanged),
`make flac-test` (115 passed, 1 ignored — unchanged; `crates/flac-lite/`
untouched by the patch), `make check`, `make native-spike-rom`.
