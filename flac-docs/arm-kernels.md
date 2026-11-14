# The ARM-mode kernel — measured ARMv4T facts, and how assembly is witnessed here

> **Scope: `crates/flac-lite-gba`'s `rice-arm` realization of `flac-lite`'s `RiceKernel`
> seam**, plus the groundwork in `examples/arm-kernels/` that produced it. The machine facts
> in this file are transferable to any GBA codec; the decisions are FLAC-only. The kernel's
> cycle figures are in [`measurements.md`](measurements.md); where its asm came from, and
> what the three 6a-5 rungs concluded, is in [`history.md`](history.md).

## Why the target needs ARM mode at all

`thumbv4t-none-eabi` compiles Thumb-1 only, and Thumb-1 lacks exactly the instructions the
hot codeword loop wants: no `CLZ`, no `RBIT`, no `REV`, no register-value barrel shift, no
full-word compare against an immediate. ARM32 mode buys three-operand arithmetic,
register-value shifts, rotate-immediate word compares and the single-cycle `MUL`. **CLZ is
absent in ARMv4T as well** — that correction is why a run length is counted by a *lookup*
(E2b) or a *multiply* (C1), never by `clz`.

| Substrate fact | How it is known |
|---|---|
| `.arm` assembly is reachable with the shipped toolchain via `core::arch::global_asm!` — as `agb` 0.25.0 itself does for `entrypoint.s` and `interrupt_handler.s` | measured, registry sources |
| ARMv4's rotated-immediate encoding **cannot** express `0x077CB531` (the De Bruijn constant), `0x01010101`, `0x0F0F0F0F`, `0xCCCCCCCC`, `0xAAAAAAAA`, `0x11111111`, `0x21082108` — all 32 even rotates tried. Ladder constants (`#0,#1,#2,#4,#8,#16,0x80000000`) are rotatable | measured |
| The De Bruijn index is exact with ARM's `MUL` (low word, `>> 27`, 32-entry table with distinct entries) | exhaustive over every `1 << i` plus 200,000 random words |
| `rice_unmap`'s `(n >> 1) ^ (-(n & 1))` is `AND`/`RSB`/`LSR`/`EOR` — no table, no branch, and **no flag writes**, so a composite can carry live flags across it | measured |
| The remainder mask `(1 << p) - 1` is `MOV r, #1, LSL p` + `RSBS`: a register-value shift, one cycle in ARM mode, hoistable per partition | measured |
| The register budget is tight: the target forces `r7` as a frame pointer in every build, so constant staging competes with the loop's working set. Literal pools are the escape hatch, and `nm`/`objdump` decides which side won | recorded (E7) |

## The kernel set

`examples/arm-kernels/` (archived: source of the shipped fold, no Make target) developed the
pieces outside the decode path, each proven correct before anything was integrated or timed.

| Kernel | Shape | Why it exists |
|---|---|---|
| **C1** `ctz32` (De Bruijn) | `RSBS`/`ANDS` isolate the lowest set bit, one `MUL` by the parked constant, `>> 27`, `LDRB` a 32-entry table. Flat ≈5 cycles for any q ≤ 31 | The flat replacement for the per-byte unary walk; `ctz32(0) = 32` is the "run past the window" sentinel |
| **C2** `word_skip` | Whole-word rotate-immediate zero compares (`teq`/`addeq`/`beq`, ~2 cycles per 32 run-bits), terminating word's tail by the `ctz8` byte table | Continuation past 31 becomes O(q/32) instead of O(q/8); Thumb-1 cannot compare a full word against zero, which is what starves the shipped loop |
| **composite** `sample_step` | Tier dispatch (q ≤ 7 → `ctz8`; 8–31 → C1; beyond 31 → C2 via `bl`), remainder, drain count, coded number, fold | The demonstration that the tier boundaries are table-judgeable end to end |
| **fold** `rice_unmap` | The four straight-line instructions above, shared by textual include with the composite | Zero design decisions; the no-flag property is what lets the composite carry state across it |
| **`refill_merge`** | `window \| (REV8[byte] << 24)`, the barrel shifter placing the byte in the merge cycle, plus a defensive `0xff` mask (AAPCS leaves upper `u8` bits unspecified) | The rev8 refill step; its *cost* was the probe's question, not this track's |

Contract decisions, settled before any table was generated — keep them if a kernel is ever
re-spelled:

- **Count authority is the total run from bit 0 of the slice**; the caller never adds. No
  "+32" agreement between consumer and generator can hide an off-by-32.
- **The `w == 0` handoff is the signal, never the answer's value** — C1's `32` is also a
  legitimate count, indistinguishable by design and correct either way.
- **An unterminated run is status data, and no state is saved.** Reachable only on a corrupt
  or truncated stream; the frame is dead either way. This is a *stated divergence* from the
  live loop's atomic cursor restore, witnessed at probe time and reported, never asserted to
  be zero.
- **Budget is the input slice length, no hard cap** — a fixed cap would answer a different
  question than the live path on a long run.
- **Tier 1 is `ctz8[byte]` over a bit-reversed window** (unary run at the LSB end), because
  that composes with C1 as shipped. `8` means "run past the byte" → dispatch to C1.
  Functionally equivalent to an MSB-first table at one `ldrb`; it is a convention.
- **A kernel sees a pre-assembled window**; bit offset is not an input, and this track claims
  no offset correctness (the sweep over reader alignments 0..7 belongs to the vendored seam).
- **The fold is shared by textual include, not duplicated by hand.** The assembler pastes an
  identical copy at each site, so runtime is unchanged, but the offline objdump witness ("the
  shipped fold is the tested fold") stays honest. In this repo that means `asm/fold_body.s`
  pasted through `include_str!`/`concat!` at both `global_asm!` use sites — rustc hands the
  assembler a temp file with no reliable search path, so assembler-level `.include` does not
  work.
- **Provenance.** The shipped partition body does **not** come from `examples/arm-kernels/`:
  `rice_partition.s` was never committed there. Its source of record is the E10 sample patch
  (`examples/flac_spike/experiments/e10-three-arm-probe/…patch`), from which the landed copy
  in `crates/flac-lite-gba/asm/` comes with four marked deltas. Only the *fold* is shared
  with the groundwork crate. The asm file's own header carries this.

## How ARM code is witnessed on a host with no ARM executor

This host is aarch64 and `qemu-system-arm` offers no `arm7tdmi` CPU (measured via
`-cpu help`), so **mGBA is the execution oracle** — and it is the only place in this
repository the assembled kernel runs. Every kernel is a pure word-in/word-out function with
one committed golden table (seeded-deterministic, with its census re-asserted as a test) and
two independent witnesses:

1. **Host twin** — the same algorithm as plain Rust, judged against the table, a naive
   bit-walk oracle, the builtin (`trailing_zeros`), and a third formulation where one exists
   (C2's answer re-derived over a `u128`; the composite's residual round-tripped through the
   **encoder-side** fold, which is what stops a sign-convention misreading from surviving).
2. **On-target sweep** — `make overlay-kernel-test`: mGBA's ARM7TDMI sweeps the assembly over
   the *same* committed corpus and the synthesized boundary cases, through three legs
   (incumbent, assembled kernel, model). Blue means every vector exact, plus explicit contract
   lines for the cases a sweep can hide: RFC worked example 38 → +19, the total-fold extreme,
   the `q = 31` shift-by-32 sentinel, the tier boundaries, budget 0, the unterminated run.

Two absences that are the point, and one that is not:

- **The sweep's blind spot is closed offline.** A Thumb build of the same algorithm would pass
  the sweep too — it computes the same answers — so ARM-mode-ness is an **ELF fact**:
  `make overlay-objdump` reads the four-byte encodings, ARM-only base+register `ldrb`
  addressing, and the return instruction out of the built image.
- **A host-side differential cannot convict the instruction stream**, because on the host the
  kernel's leg is a Rust *model* of it. That division of labour (model = contract, ROM =
  stream, objdump = mode) is deliberate and appears in every gate's own closing line.
- **Table addresses are never constants.** The instrument asserts at *runtime* that its three
  tables sit inside `.iwram`, because placement is a property of reachability, not of the
  `link_section` attribute: an unreachable walk's tables are dropped by the link entirely, and
  `LEADING_ONES` moved address between two builds whose source for it was identical.

## Machine facts the assembly taught us

Each of these cost an image.

| Assumption | Reality |
|---|---|
| `MUL Rd, Rm, Rs` with `Rd == Rm` is fine | **UNPREDICTABLE** on ARMv4, and clang assembles it silently. Keep `Rd`/`Rm`/`Rs` distinct |
| `pop {…, pc}` returns fine to a Thumb caller | **ARMv4T does not interwork through `LDM pc`.** A Thumb `bl` into an ARM kernel whose tail pops to PC loops forever between the linker thunk and the kernel. Only `BX`/`BLX` switch state: tails are `pop {…, lr}; bx lr` |
| `teqs` is the flag-setting spelling under `.syntax unified` | It does not exist; `teq` always writes flags. Landed shape: `teq`/`addeq`/`beq` |
| Edition-2024 `global_asm!` passes braces through | `{`/`}` are template placeholders **even in comments**; escape `{{`/`}}` (rustc unescapes before assembling) |
| `global_asm!` is a safe macro | In edition 2024 it is an **unsafe op**, and `#[allow(unsafe_code)]` on the macro item is ignored — the reviewed exception belongs on a wrapping `mod` |
| AAPCS entry alignment is SP % 4 | SP % 8 == 0 at entry |
| Assembler-level `.include` can share the fold source | No search path is provided; share through `include_str!` in the Rust source |
| Two ARM kernels can coexist in one image under their own names | **No — and it fails loudly, at the assembler.** `global_asm!` symbols are not namespaced and a crate's blocks go into one assembly unit, so a second kernel must be *renamed* (`arm_rice_partition` → `e10_rice_partition`) **and its file-local labels too** (`.Lloop` collides the same way). Verified by rebuilding the rename away: the failure is a build error, so a differential image cannot silently measure one kernel twice |
| Two `.iwram` copies of the same table in one link collide | They do not: the two legs' `CTZ8`/`REV8`/`BRUIJN_INDEX` land at distinct addresses, each kernel reads its own, and each ARM body gets its own `__Thumbv4ABSLongBXThunk_*`. Placement stays a property of reachability |
| A guard inserted into a hot asm body stays reviewable by eye | **Not reliably.** A guard insertion moved `and r12, r3, #0xFF` above the guard and left the original in place, so the shipped file carried the same line twice and the hot loop paid a **dead instruction once per codeword** — 640,000 dead executions on a 157-frame clip. No correctness gate can see it (it changes nothing) and a diff of the *guard blocks* alone misses it. What catches it is a word-for-word diff of two kernels that are supposed to be the same instruction stream |
| An instruction in this loop costs ~1 cycle | **It costs 4.000 cycles from cartridge ROM** — measured twice by deletion (one dead instruction = exactly 4 × 640,000 cycles; five more = exactly 5 × 4 × 640,000), with the saving tracking codewords *executed*. Instruction *count* is the currency here and its price is 4× what a naive model assumes, so **the register budget is a cycle budget**: a spilled bound reloaded once per codeword is not free, it is 4 cycles. And from IWRAM the same instruction costs ~1.4 — which is why the body lives there ([`history.md`](history.md), 6a-5c/6a-5d) |

## What this file does *not* claim

No cycle numbers (they are in [`measurements.md`](measurements.md), each tied to the image
that printed them). No decode-path integration for the `examples/arm-kernels/` pieces — the
shipped kernel is the overlay's `rice_arm.rs`, and the groundwork crate exists so a kernel is
proven before it is threaded. And no C3/C4 kernels: those were armed only as contingencies if
register pressure collapsed the hybrid or the refill tax measured hot, and neither happened.
