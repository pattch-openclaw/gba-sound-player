# `targets/` — checked-in target specs

`thumbv4t-gba.json` is the project's GBA build target. It is **not** the built-in
`thumbv4t-none-eabi` triple: it is that triple's own `rustc --print
target-spec-json` output with exactly **one field changed**, and that one field
buys back a register in every Rust function.

This directory exists because the build target turned out to be a performance
lever, and because the choice is now deliberate and load-bearing — so it is
checked in, reviewed and witnessed instead of being a flag somebody set once.

## The one difference

| field | built-in `thumbv4t-none-eabi` | ours |
|---|---|---|
| `frame-pointer` | `"always"` | **`"may-omit"`** |

Everything else — `data-layout`, `features` (`+soft-float,+strict-align`),
`llvm-floatabi`, `relocation-model`, `linker`/`linker-flavor`,
`panic-strategy` (`abort`), `max-atomic-width` (`0`), `abi`, the asm args — is
inherited verbatim. `make target-spec-check` fails if that stops being true.

## Why

ARM7TDMI has **no hardware frame pointer**. `r11 = fp` is an ATPCS/APCS *software*
convention. In ARM mode the callee-saved set is `r4-r11` — eight registers — so
reserving r11 costs one of eight, plus a prologue `add fp, sp, #n`, an extra
push/pop slot per call, and forces some locals to be addressed through `fp`
instead of living in a register.

The built-in spec pins `frame-pointer: always`, which sets LLVM's
`frame-pointer=all` attribute on **every** function, whether or not a frame is
needed. On this project that landed on exactly the hot set: 13 of 210 functions
in the product image set up a frame, and they were `dec_chan`, `decode_frame`,
`pre`, `fft`, `post`, `ola`, `ola_s8`, `ola_s16` and `eg_slow`.

`frame-pointer` has three values, and the middle one is a trap worth recording:

- `always` — the built-in default here.
- `non-leaf` — keeps the frame in every function that *calls out*. Our hottest
  function, `dec_chan`, has ten `bl` sites to `huff_slow`/`eg_slow`, so
  `non-leaf` would have kept the frame exactly where we most wanted it gone.
  It is not the useful middle ground it looks like.
- `may-omit` — what we use. Frame only where it is actually needed (here:
  nowhere; the image ends up with **zero** functions setting up a frame).

Two facts make this defensible on this target rather than a generic gamble:

1. `panic-strategy` is `abort`. There is no unwinder in the image, no
   `core::backtrace`, no `RUST_BACKTRACE`. The stated purpose of forcing a frame
   pointer — unwinding — has no consumer here.
2. What remains is debugger walkability, and mGBA debugs from the DWARF and
   symbols we already emit (`debug = true` in the release profile), not from an
   frame-pointer chain.

It is also not a flag we could have flipped: `-Cforce-frame-pointers=no` is
**inert** on this target. Measured by rebuilding the bench ROM both ways from a
clean `target/` (cargo does not invalidate on config-content changes) — the two
ROMs are byte-identical, `579fe24dc345aaa13c7f522bc964246a`. The target spec wins,
which is why the spec is where the change had to happen.

## What it is worth (measured 2026-11-17, mGBA, WAITCNT 4/2)

| | built-in (`always`) | `may-omit` | delta |
|---|---:|---:|---:|
| 129-frame worst | 210,833 = 80.43% | **208,854 = 79.67%** | −1,979 (−0.75 pts) |
| 129-frame mean | 183,241 = 69.90% | **182,119 = 69.47%** | −1,122 (−0.43 pts) |
| album worst (f16,204) | 210,787 = 80.41% | **209,162 = 79.79%** | −1,625 (−0.62 pts) |
| album mean | 179,226 = 68.37% | **178,341 = 68.03%** | −885 (−0.34 pts) |
| `.iwram` code | 27,308 B | **26,412 B** | **−896 B** |
| functions with a frame | 13 | **0** | — |

Bit-exact: both album pins PASS, `xover=0 hfnz=0`, and the on-target kernel twin
PASSes with `fftasm=true preasm=true olaasm=true msasm=true`. Two runs per image,
identical to the cycle. Stack high-water rises 844 → 932 B, which the IWRAM budget
absorbs.

The gain is not concentrated in one function — it is the same ~0.4-0.6 points
everywhere in the decoder, because the register was being spent in every hot
function at once. That is why it is worth having as a *target* property rather
than as a per-function hack.

## Why a checked-in spec and not a flag

There is no per-function way to do this in stable Rust: no `#[no_frame_pointer]`
attribute exists, and `-Cllvm-args` cannot override rustc's per-function
attribute. The alternatives were a custom target spec (this) or hand-writing the
affected functions in asm. The spec is one file and moves every Rust function at
once; the asm route is a 1,250-instruction function, and the four hand-written
kernels are the measure of what that costs.

## Using it

Nothing to do — `.cargo/config.toml` sets `[build] target` to this file and turns
on `[unstable] json-target-spec`, so plain `cargo build --release` in any
workspace here builds for it. In the Makefile, `TARGET` is the spec's file stem
(`thumbv4t-gba`, which is what cargo names the output directory after) and
`TARGET_SPEC` is the path passed to `--target`.

Two plumbing facts that bite if you change this:

- **Rustflags are keyed by target *name*.** For a `.json` spec that name is the
  file stem, so the flags live in `[target.thumbv4t-gba]`, not
  `[target.thumbv4t-none-eabi]`. Rename the file and you must rename that table,
  or the linker script and `-Ctarget-cpu` silently stop being applied.
- **`[target.*.rustflags]` is not inherited by `--target <path>`.** Harnesses that
  pass an explicit path (e.g. a one-off experiment) must carry the flags
  themselves, or set `RUSTFLAGS`.

Host gates are unaffected: they run from `GATE_NEUTRAL_CWD`, outside the repo, so
they inherit none of this.

## Maintaining it

`targets/thumbv4t-gba.json` must stay a byte-for-byte copy of the pinned
toolchain's built-in spec except for `frame-pointer`. Regenerate when
`rust-toolchain.toml` moves:

```sh
rustup run nightly-2026-09-04 rustc --print target-spec-json -Z unstable-options \
    --target thumbv4t-none-eabi > /tmp/builtin.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/builtin.json'))
d['frame-pointer'] = 'may-omit'
json.dump(d, open('targets/thumbv4t-gba.json', 'w'), indent=2)
open('targets/thumbv4t-gba.json', 'a').write('\n')
PY
make target-spec-check
```

`make target-spec-check` dumps the built-in spec from the *pinned* rustc, diffs it
against this file, and fails on any difference other than `frame-pointer` —
including a field appearing on one side only, because a dropped field would be
filled from rustc's defaults and an added one would be missing from ours. It does
not run as a build pre-step: a stale-but-consistent spec still builds a
consistent target, so this is a check for the moment the pin moves, not a tax on
every build.

If it ever fails, **re-derive the spec and re-apply only the allowed difference**.
Do not widen the allow-list to make it pass — the fields it guards (`data-layout`,
`llvm-floatabi`, `panic-strategy`, `linker-flavor`, …) are exactly the ones that
would make every cycle figure in `crates/uli-lite/PLAN-OF-RECORD.md` describe a
different machine than the one being built.

## Scope

This is a project-wide lever, not a ULI2 one. It applies to every crate and
example built for the GBA here — the FLAC path, `gba-playback`, the album player,
the probes. Only the ULI2 product path has been re-measured so far; if another
track wants the number, it takes its own arm.

**One cross-track consequence is already known and is not a code regression.**
`flac-spike.gba`'s PR 3 verdict changes from "measured 2/2, within budget 1/2" to
"measured 0/2" under this spec, because its timing-window *calibration* fails:

| spec | calibration | verdict |
|---|---|---|
| built-in (`always`) | `raw` = 22,22,22,22,22 → `stable=true`, `nets=[0 0 0 0 0]` | measured 2/2 |
| this spec (`may-omit`) | `raw` = **20**,22,22,22,22 → `stable=false`, `nets=[0 2 2 2 2]` | measured 0/2 (numbers void) |

That harness requires all five empty-window samples to be *identical* and voids
the run otherwise, so a **2-cycle change on the first sample only** is fatal to
it. Nothing about the codec moves: `decode proof PASSED`, `pcm fnv ... [MATCH]`
and `frames=80/80 samples=162304 [OK]` all still hold, and the spread it is
rejecting is 2 cycles of net overhead (0-2 cycles, i.e. ≈0).

The honest reading is that the witness is brittle at the 2-cycle level — it is
measuring the harness's own empty window, so any codegen change anywhere in that
image can flip it. It is **not** gated by any `make` target (`spike-rom` only
builds; a human reads the verdict), and `make flac-test` — the actual FLAC gate —
passes. Re-baselining that instrument, or relaxing it to tolerate a first-sample
warm-up, belongs to the FLAC track as its own change; it is recorded here so the
next person does not spend an afternoon on an apparent regression.

Related: any ROM sha or cycle figure recorded in `flac-docs/measurements.md`
before 2026-11-17 was produced under `frame-pointer: always` and is not
reproducible under this spec. Those are records, not gates, but a track that
wants to compare against them must re-take its arm.
