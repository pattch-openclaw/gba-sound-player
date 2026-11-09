# The ARM-mode kernel track — measured ARMv4T facts and the kernel set

The E10 work lives in `examples/arm-kernels/` and is deliberately outside the
decode path: kernels are proven correct there before anything is integrated or
timed. This file keeps what that track established about the machine and about
testing ARM code on this host; the cycle reading is in
[`perf-results.md`](perf-results.md) (E10, E9), the landing plan in
[`plan-of-record.md`](plan-of-record.md) (6a-4…6a-7).

## Why the target needs this at all

`thumbv4t-none-eabi` compiles Thumb-1 only, and Thumb-1 lacks the instructions the
hot codeword loop wants: no `CLZ`, no `RBIT`, no `REV`, no register-value barrel
shift, no full-word compare against an immediate. ARM32 mode buys three-operand
arithmetic, register-value shifts, rotate-immediate word compares and the
single-cycle `MUL`. CLZ is absent in ARMv4T as well — that correction is why the
count is a *lookup* (E2b) or a *multiply* (C1), never `clz`.

The substrate facts below were verified by repo/registry inspection and host
arithmetic; each is marked with what makes it true.

| Fact | Status |
|---|---|
| `.arm` assembly is reachable with the shipped toolchain (`core::arch::global_asm!`, as agb 0.25.0 itself does for `entrypoint.s`, `interrupt_handler.s` and the sample mixer) | measured, registry sources |
| ARMv4's rotated-immediate encoding cannot express the De Bruijn constant `0x077CB531`, `0x01010101`, `0x0F0F0F0F`, `0xCCCCCCCC`, `0xAAAAAAAA`, `0x11111111`, `0x21082108` — all 32 even rotates tried. Ladder constants (`#0,#1,#2,#4,#8,#16,0x80000000`) are rotatable | measured |
| The De Bruijn index is exact with ARM's `MUL` (low word, `>> 27`, 32-entry table with distinct entries) — exhaustive over every `1 << i` plus 200,000 random words | measured |
| `rice_unmap` identity `(n >> 1) ^ (-(n & 1))` is `AND`/`RSB`/`LSR`/`EOR`, no table, no branch, **no flag writes** — so a composite can carry live flags across it | measured |
| The remainder mask `(1 << p) - 1` is `MOV r, #1, LSL p` + `RSBS`: a register-value shift, one cycle in ARM mode, and hoistable per partition | measured |
| Register budget is tight: the target forces `r7` as frame pointer in every build, so constant staging competes with the loop's working set. Literal pools are the escape hatch; `nm`/`objdump` decides which side won | recorded (E7) |

## The kernels

| Kernel | Shape | Why it exists |
|---|---|---|
| **C1** `ctz32` (De Bruijn) | `RSBS`/`ANDS` isolate the lowest set bit, one `MUL` by the parked constant, `>> 27`, `LDRB` a 32-entry table. Flat ≈5 cycles for any q ≤ 31 | The flat replacement for the per-byte unary walk; `ctz32(0) = 32` is the "run past the window" sentinel |
| **C2** `word_skip` | Whole-word rotate-immediate zero compares (`teq`/`addeq`/`beq`, ~2 cycles per 32 run-bits), terminating word's tail by the `ctz8` byte table (≤4 lookups) | Continuation past 31 becomes O(q/32) instead of O(q/8); Thumb-1 cannot compare a full word against zero, which is what starves the shipped loop |
| **composite** `sample_step` | Tier dispatch (q ≤ 7 `ctz8` byte table → 8–31 C1 → beyond 31 C2 via `bl`), remainder, drain count, coded number, fold | The track's comprehensive demonstration: the tier boundaries are table-judgeable end to end |
| **fold** `rice_unmap` | The four straight-line instructions above, shared by textual include with the composite | Zero design decisions; the property that it writes no flags is what lets the composite carry state across it |
| **`refill_merge`** | `window \| (REV8[byte] << 24)`, the barrel shifter placing the byte in the merge cycle, plus a defensive `0xff` mask (AAPCS leaves upper `u8` bits unspecified) | The rev8 refill step; its *cost* is the probe's question, not this track's |

Handoff and contract decisions of record (settled before any table was generated):

- **C2 is standalone**, slice-in/slice-out, so the tier logic never has to share a
  PR with its contract.
- **Count authority is the total run from bit 0 of the slice**; the caller never
  adds. No "+32" agreement between consumer and generator can hide an off-by-32.
- **The `w == 0` handoff is the signal, never the answer's value** — C1's `32` is
  also a legitimate count, indistinguishable by design and correct either way.
- **An unterminated run is status data, and no state is saved.** It is reachable
  only on a corrupt or truncated stream; the frame is dead either way. This is a
  stated divergence from the live loop's atomic cursor restore, witnessed at probe
  time.
- **Budget is the input slice length, no hard cap.** A fixed cap would answer a
  different question than the live path on a long run.
- **The tier-1 table is `ctz8[byte]` over a bit-reversed window** (unary run at the
  LSB end), because that composes with C1 as shipped. `8` means "run past the
  byte" → dispatch to C1. Functionally equivalent to an MSB-first table at one
  `ldrb`; the choice is convention.
- **Provenance correction (6a-4, measured):** 6a-4's plan line said the
  partition kernel would be "lifted from `examples/arm-kernels/`". That is right
  about the **fold** (`asm/fold_body.s` is a committed main-tree file and is pasted
  by `include_str!`) and wrong about the **partition body**: `rice_partition.s` was
  never committed on main — its only source of record is the E10 sample patch
  (`examples/flac_spike/experiments/e10-three-arm-probe/…patch`), and the landed
  copy in `crates/flac-lite-gba/asm/` comes from there, with four marked deltas.
  The asm file's own header carries the correction; this line exists so the plan
  text does not keep sending readers to the wrong tree.
- **The fold is shared by textual include, not duplicated by hand.** The assembler
  pastes an identical copy at each site, so runtime is unchanged, but the offline
  `objdump` witness ("the shipped fold is the tested fold") stays honest. In this
  repo that means `asm/fold_body.s` pasted through `include_str!`/`concat!` at both
  `global_asm!` use sites — rustc hands the assembler a temp file with no reliable
  search path, so assembler-level `.include` does not work.
- **The composite sees a pre-assembled window**; bit offset is not an input. The
  sweep over reader alignments 0..7 belongs to the probe's vendored seam, and the
  composite claims no offset correctness.

## How ARM code is witnessed on a host with no ARM executor

Every kernel is a pure word-in/word-out function with **one committed golden
table** (`src/*_vectors.rs`, recipe in the header, seeded-deterministic,
fail-closed census re-asserted as a test) and two independent witnesses:

1. **Host twin** (`make kernel-test`, out-of-tree `--manifest-path`): the same
   algorithm as plain Rust, judged against the table, a naive bit-walk oracle, the
   builtin (`trailing_zeros`), and a third formulation where one exists — e.g. C2's
   answer re-derived over a `u128`, the composite's residual round-tripped through
   the **encoder-side** fold. That anchor is what stops a sign-convention
   misreading from surviving.
2. **On-target sweep** (`make native-kernel-rom`): mGBA's ARM7TDMI sweeps the
   assembly over the same table. **mGBA is the execution oracle because the host has
   none** (aarch64; `qemu-system-arm` 11.1.2 offers no `arm7tdmi` CPU — measured
   via `-cpu help`). Blue = every vector exact, plus explicit contract lines for
   the cases a sweep can hide (RFC worked example 38 → +19, the total-fold
   extreme, the q = 31 shift-by-32 sentinel, tier boundaries q 7/8/32, budget 0,
   unterminated).

**The sweep's blind spot is closed offline.** A Thumb build of the same algorithm
would pass the sweep too, so ARM-mode-ness is an **ELF fact**: `objdump -d` shows
4-byte ARM encodings, ARM-only base+register `ldrb` addressing, and register-value
shifts.

Vector counts at the end of the track: C1 346, PR A fold 839, `word_skip` 241,
`sample_step` 456, `refill_merge` 470; host tests 5 → 9 → 20 across the three
landings. Every image was rebuilt and run twice with `cmp`-identical output, and
each new kernel's image re-printed the untouched kernels' own sweep counts — the
in-image control that keeps a cross-session comparison honest.

## Machine facts the assembly taught us (each one cost an image)

| Assumption | Reality |
|---|---|
| `MUL Rd, Rm, Rs` with `Rd == Rm` is fine | **UNPREDICTABLE** on ARMv4, and clang assembles it silently. Keep Rd/Rm/Rs distinct (`w` is dead in `r0`, so reuse `r0`) |
| `pop {…, pc}` returns fine to a Thumb caller | **ARMv4T does not interwork through `LDM pc`.** A Thumb `bl` into an ARM kernel whose tail pops to PC loops forever between the linker thunk and the kernel. Only `BX`/`BLX` switch state: tails are `pop {…, lr}; bx lr` |
| `teqs` is the flag-setting spelling under `.syntax unified` | It does not exist; `teq` always writes flags. Landed shape: `teq`/`addeq`/`beq` |
| Edition-2024 `global_asm!` passes braces through | `{`/`}` are template placeholders **even in comments**; escape `{{`/`}}` (rustc unescapes before assembling) |
| `global_asm!` is a safe macro | In edition 2024 it is an **unsafe op**, and `#[allow(unsafe_code)]` on the macro item is ignored — the reviewed exception belongs on a wrapping `mod` |
| AAPCS entry alignment is SP % 4 | SP % 8 == 0 at entry; caught in the frame before any measurement |
| Assembler-level `.include` can share the fold source | No search path is provided; share through `include_str!` in the Rust source |
| Two ARM kernels can coexist in one image under their own names | **No — and it fails loudly, at the assembler, not silently at the link.** `global_asm!` symbols are not namespaced, and a crate's blocks go into one assembly unit: restoring the vendored copy's original symbol while the landed kernel is also in the crate gives `error: symbol 'arm_rice_partition' is already defined` / `note: instantiated into assembly here` and the build stops. The second kernel must be *renamed* (`arm_rice_partition → e10_rice_partition`). Verified by rebuilding the rename away, 2026-11-09 — the failure mode is a build error, so a differential image cannot silently measure one kernel twice |
| …and that its file-local labels are private | **Also no.** rustc assembles a crate's `global_asm!` blocks into one assembly unit, so `.Lloop`, `.Ltier1`, `.Lrefill1`, … collide between two kernels and the assembler fails with `symbol '.Lloop' is already defined`. Rename the vendored copy's labels (`.Le10*`) as part of the same mechanical edit — labels *and* symbols, or it does not link |
| Two `.iwram` copies of the same table in one link are a collision | They are not: the two legs' `CTZ8`/`REV8`/`BRUIJN_INDEX` land at distinct addresses (`0x0300032c/34c/44c` and `0x0300054c/56c/66c` in one image), each kernel reads its own, and each ARM body gets its own `__Thumbv4ABSLongBXThunk_*`. Table placement stays a property of reachability, never a constant |
| A guard inserted into a hot asm body stays reviewable by eye | **Not reliably.** The [6a-4 G1] insertion moved `and r12, r3, #0xFF` above the guard and left the original in place, so the shipped `rice_partition.s` carries the same line twice (256 and 258) and the hot loop pays a **dead instruction once per codeword** — 640,000 dead executions on a 157-frame clip. No correctness gate can see it (it changes nothing), and a source diff of the *guard blocks* alone misses it. What caught it is a word-for-word diff of two kernels that are supposed to be the same instruction stream; that diff is now part of how a realization differential is read |

## What this track does *not* claim

No cycle numbers (the three-arm probe measured those separately, and 6a-5 will
re-measure on the shipped path). No decode-path integration. No `.iwram` pinning —
the plan's 544 B table arithmetic (tier-1 256 + `rev8` 256 + Bruijn 32) is pricing,
not placement; the probe's `nm`/`objdump` measures what actually pins, and at the
overlay default the incumbent `LEADING_ONES` stays live, making the honest overlay
figure **+512 B pinnable**, not +256 B. No bit-offset correctness. No C3/C4: they
are contingencies, armed only if register pressure collapses the hybrid or the
refill tax measures hot, never measured first.
