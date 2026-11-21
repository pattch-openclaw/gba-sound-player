# ULI2 measurement record

The **chronological** half of the ULI2 plan: the critical-path steps, every PR and its arms, the measurement ledger, the landing log, and the risks as they were understood at the time.

- Current state, architecture, memory layout and the measurement *method*: [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md).
- The decisions, the gates and the live risks: [`../../ULI2.md`](../../ULI2.md).

**This file is append-mostly and is allowed to be long.** It exists because the project’s rule is that a claim is worthless without its measured arm and a falsification is worth more than a projection — so the arms stay, including the ones that lost. Where a passage was contradicted outright it is marked *Superseded* in place rather than deleted; the numbers in an older entry are the numbers *of that day*.

---

## Decision (2026-11-14): ULI2 is GBA-first

ULI2 is a GBA product codec by definition: one fixed rate (32,768 Hz), one
hop (512), one target (ARM7TDMI), one player. The portability constraints
inherited from the `flac-lite` split are therefore **retired for the product
path**. The portable core is not deleted — it becomes the bit-exact reference
and the test oracle — but it is no longer the thing that has to ship, and the
product is no longer required to be safe, portable or platform-free.

### Constraints retired, and constraints kept

| Constraint | Product path | Why |
|---|---|---|
| `#![deny(unsafe_code)]` | **retired** | Raw pointers, unchecked indexing, block moves and asm are the performance work. |
| Portable facade / no platform codegen | **retired** | ARM state, `.iwram` placement and GBA-shaped layout are allowed. |
| The core *is* the product | **retired** | The product decoder lives in `uli-lite-gba`; the core is the oracle. |
| No `agb` in the codec | **retired for the product**; keep the decoder itself `agb`-free | The decoder needs no MMIO; keeping it `agb`-free is what lets the host twin tests run. `agb` belongs to the player. |
| `no_std` | **kept** | `agb` is `no_std`; the GBA has no OS. Not a portability tax. |
| Zero allocation, caller-owned state | **kept** | Real-time and memory constraints, not portability. |
| Integer/fixed-point only | **kept** | No FPU; the format's arithmetic. |
| Bit-exactness against the C-pinned corpus | **kept** | The correctness witness; every optimization is twin-tested. |
| A live reference implementation | **kept** | The oracle is what keeps a fast unsafe decoder honest. |
| Licensing / `publish = false` | **kept** | Unresolved; unchanged by this decision. |
| Rust encoder later | **kept**, *scheduled 2026-11-19* | Separate milestone; the C encoder remains of record until it lands. Now the last missing piece — see [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md) → "The encoder — the last missing piece". |

---

## Critical path to real time (2026-11-15)

**This is the first thing to read if the question is "what next".** Status lines
above are history; this section is the live order of work.

> **Superseded as "what next" (2026-11-19).** Real time was met (U4 with 13.73
> points of margin), so the live order of work is no longer this critical path but
> the remaining milestone: [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md) → "The encoder
> — the last missing piece", whose phases, gates and decisions this file's header
> already assigns to that file. What follows stays the record of how the decode
> wall was brought under the period, arm by arm — history, and the provenance of
> the numbers quoted elsewhere.

### Where we are

**The decode wall on the album corpus is 206,101 worst (f16,204) / 175,280
mean = 78.62% / 66.86%**, and the **gate total (decode + convert + IRQ + DMA) is
237,646 worst / 206,637 mean = 90.65% / 78.83%**, with **24,498 cycles (9.35
points) of margin at the worst frame** and zero starved periods over 16,552.
Re-measured 2026-11-18 after the asm `post` kernel (the last Rust kernel on the
product path). The 129-frame iteration corpus reads 205,793 / f83 / 179,058 mean
= 78.50% / 68.31%. **On the `cross` corpus — the sibling 12-track album, the one
the C record's own numbers were measured on — the decode wall is 207,148 worst
(f27,693) / 170,939 mean = 79.02% / 65.21%, against C's 181,928 / ~153,000, i.e.
1.139x worst / 1.117x mean.**

Against the C record (181,928 = 69.4% worst, ~153,000 = 58.4% mean) the decode
wall is **1.15x** on the worst frame and **1.17x** on the mean. **CORRECTION
2026-11-18:** an earlier revision of this paragraph read "1.03x on the mean" —
that number was wrong: it came from dividing our *mean* by C's *worst*
(178,341/181,928 = 0.98) and then mis-transcribing the quotient. The like-for-like
mean ratio is 178,341/153,000 = **1.17x**. The gap is systematic (~15-17% across
the distribution), not a single bad frame, and every "beat C" target should be
sized from the corrected pair. (Both C figures remain a diagnostic: different
corpus, different image — see "Caveats".) `.iwram` headroom is ~2,196 B (27,3xx B of
30,320 B usable at the 844 B measured stack).

Two different targets, not one:

- **to work** — the decode wall must fit with room for the gate total (DMA, IRQ,
  convert) and this tree's UI: target **<= ~85%**. **MET twice over (80.4%
  decode, 92.5% gate total).** The gate total is now measured, not assigned, and
  it leaves 7.54 points at the worst frame.
- **to match C** — cut a further **~25,200 worst / ~17,900 mean**, measured on the
  matched `cross` corpus, where our mean and C's cover the *same* 167,222 frames
  (so this is density-free). Not a requirement; C has 31% slack by design. We are
  **1.139x worst / 1.117x mean**, and the gap is systematic rather than one bad
  frame. Where it lives: `post` was the only *instruction-named* deficit and it is
  now closed; `fft` (32.9% of the frame, but at instruction parity) and entropy
  (29.6%, the predication-gated path) are what remain, and single-frame ratios
  against C cannot rank them — only an all-frames per-part mean on the same
  corpus can. See "The kernel table, re-derived" in
[`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md).

**92.46% gate total is not "real time" either.** It is measured in mGBA with the
shipping `RingFeed` and the real timebase, which does not model the cart prefetch
buffer or DMA/code-fetch arbitration, and this tree's UI is not in it — the
hardware run recorded 104 *late re-arms* from a 1 Hz text redraw alone. So U4's
ceiling stays written down as the period. What has changed is that the margin is
now **7.54 points at the worst frame** instead of 3.98, and the UI's measured
1.6-1.8 period redraw is the thing that consumes it.

### Root-cause audit against the C product decoder (2026-11-16)

**Why this exists.** Seven optimisation steps had brought the marginal return to
~1% of the frame per round, and the record's own conclusion was that "the
remaining gap is the entropy path" plus a list of ceilings it had never priced
against the C source. This audit went to the C implementation directly —
`gba-audio/src/uli/gba/uli_gba_dec.iwram.c`, `uli_imdct_arm.S`,
`uli_gba_int.h` and `docs/phase2/2-gba-decoder.md` — read it line by line,
disassembled our own `.iwram` from the ELF, and measured. It found more in one
pass than the previous six steps combined. **The lesson is not "look harder": it
is that a per-kernel ratio against a **summary table** is not a comparison. The
C record's `pre 9.7k / fft 31.4k / post 10.9k / ola 15.3k` is a *decomposition*,
not a contract; the code behind it was never read.**

**Result, measured in two arms, two runs each, identical to the cycle, both
pins PASS and the on-target kernel twin PASS in both:**

| | worst | mean | of period | vs C |
|---|---:|---:|---|---:|
| HEAD (before the audit) | 219,251 (f125) | 194,489 | 83.64% / 74.19% | 1.205x |
| **A — FFT multiply operands** | 214,691 (f83) | 187,496 | 81.90% / 71.52% | 1.180x |
| **B — M/S kernel (above)** | **210,355 (f83)** | **185,207** | **80.24% / 70.65%** | **1.156x** |

A is **-4,560 worst / -6,993 mean / -902,149 sums over 129 frames**; B is
**-4,336 / -2,289 / -295,275**. Together **-8,896 worst (-4.06%), -9,282 mean
(-4.77%)**, and `.iwram` +28 B net (`uli_ms_arm` is 68 B; the Rust `ms` control
left `.iwram` when the kernel arrived, which pays for most of it).

The five findings, ranked by what they were worth, are below. **Two are landed;
three are open work with a named shape.**

#### A — the FFT multiplies had their operands reversed (**landed, -6,993 mean**)

ARM7TDMI multiply cost depends on **Rs**, the *second* source operand: it
early-terminates on Rs's significant bytes. The C source states the convention
explicitly (`uli_imdct_arm.S`):

> *"the multiplier operands are swapped so that the data value is Rs (ARM7
> multiply early termination by Rs: zero / small coefficients cost fewer cycles);
> products are identical."*

…and their stage-2 record prices exactly that change: **78.1% → 73.1%**, five
points of the frame from operand order alone.

Our kernels split on this line, and the split is legible in the disassembly:

| kernel | our operand order | C's | measured vs C |
|---|---|---|---:|
| `pre_arm.s` | `smull r9,r10,r6,r4` = Rn=twiddle, **Rs=data** ✅ | same | 1.23x |
| `ola_arm.s` | `smull lr,r0,r11,r9` = Rn=window, **Rs=data** ✅ | same | **1.00x** |
| `post` (Rust) | LLVM: Rn=twiddle, **Rs=z** ✅ | — | 1.03x |
| `fft_arm_v5.s` | `smull r0,r1,r8,r11` = Rn=data, **Rs=twiddle** ❌ | Rn=twiddle, **Rs=data** | **1.14x** |

Our `TW` entries are ~2^30 (`1073741824`), i.e. full width, so **every FFT
multiply ran at the slow end of the multiplier's range, unconditionally**. The
FFT is 33% of the frame, which is why this one line of operand order is a 3.6%
frame.

**How it happened, and why the record missed it.** `v5`'s header says
*"ARITHMETIC — identical to v1/v2/v3/v4"*, and that was true of the products but
not of the timing. v1 was written clean-room in PR C-1, before the reference asm
was adopted; `pre` (Step 1c) and `ola` (Step 1d) were written later by copying
the reference's loop shape, which brought the correct operand order with it. The
FFT was the one kernel that never got it — **all of v1-v4 are reversed**. The
record then quoted the FFT's `1.33x → 1.29x → 1.14x` ladder as if each step were
structural progress; part of what it was measuring was this constant.

#### B — M/S had no hand-written kernel at all (**landed, -2,289 mean**)

C ships `uli_ms_asm`: two `ldmia`, eight ALU ops, two `stmia` — **4 coefficients
in 14 instructions**. We had a scalar `for i in 0..n` loop, which LLVM compiles
to **9 instructions per coefficient**:

```asm
ldr r2,[r1] ; ldr r3,[r1,#-2048] ; subs r0,#1 ; sub r6,r3,r2
add r2,r2,r3 ; str r6,[r1] ; str r2,[r1,#-2048] ; add r1,#4 ; bne
```

`asm/ms_arm.s` is that kernel, landed as `asm-ms` (on by default) with the Rust
loop kept as the in-image control and a new on-target twin
(`ovl twin ... msasm=true`) that compares the two on the ARM7TDMI over a
pattern covering zero, the int32 rails, and wrapping sums.

**The record's ceiling for this item was wrong in the informative direction.**
"Handing off" said M/S was *"the only hot loop in the decoder nobody has
disassembled … already ~1.0x C per coefficient … ceiling ~1,500-2,500"* and
explained the 9,861-vs-4,400 difference as *"frame density, not a slow loop"*. It
was a slow loop, C's was asm, and the loop is worth ~3,300 cycles by instruction
count. The measured frame delta is smaller (-2,289 mean) because the by-difference
`attrib-ms` arm over-states it: removing M/S changes the values the synthesis
kernels multiply, and those multiplies are data-dependent (**see the method note
below**).

#### C — the target pinned a frame pointer in every Rust function (**LANDED 2026-11-17**)

`rustc --print target-spec-json --target thumbv4t-none-eabi` reports
`"frame-pointer": "always"`. That is why the FLAC E7 probe and PR C's
`-Cforce-frame-pointers=no` arm both measured **inert** — the conclusion drawn
from them ("no cost, move on") was the wrong half of the observation.

**Yes: relative to C + asm we have one fewer register, everywhere, permanently.**
Gcc at `-O2 -marm` omits the frame pointer; ARM-mode Rust on this target cannot.
In `dec_chan` — the register-starved hot loop this file has spent three steps
on — r11 is pinned:

```asm
push {r4, r5, r6, r7, r8, r9, sl, fp, lr}
add  fp, sp, #28
```

117 fp-relative loads/stores, and **259 stack references in 1,394 instructions
(18.6%)**; the by-value `BitReader` argument is re-read through `fp`.

This retro-explains the record's own negative result: **"register relief in
`dec_chan` does not pay"** (1e-ii) — there was no spare register to relieve
*with*, because the toolchain had already spent r11. That experiment was testing
a hypothesis its instrument had disabled.

**This is not given up on, and it is not a small thing.** It is one of thirteen
usable registers in every Rust kernel and in `dec_chan`, and it is the last
structural difference between our codegen and gcc's. What it needs is dedicated
consideration, not another micro-step:

- the only real lever is a **custom target JSON** derived from
  `thumbv4t-none-eabi` with `frame-pointer: "non-leaf"` (or `"may-omit"`). That
  changes the instrument, so it carries a control build and a section/symbol dump
  per this file's baselines-to-hold rule;
- it needs a **stack-unwinding answer first**: the reason bare-metal ARM targets
  pin a frame pointer is unwinding and debugger walkability. This tree measures
  stack high-water with a canary (`844 B`), not an unwinder, so the constraint
  may already be unnecessary here — *to be established, not assumed*;
- it should be measured as one arm (whole image, both twins, all pins) because
  it moves every Rust function at once, and it is not attributable by difference.

Recorded as **U6-adjacent and open**, not as a ceiling accepted.

**LANDED 2026-11-17 — the frame pointer is gone, and the lever is now a
checked-in target spec.** `targets/thumbv4t-gba.json` is the pinned toolchain's
own `--print target-spec-json` for `thumbv4t-none-eabi` with exactly one field
changed, `frame-pointer: "may-omit"`; `.cargo/config.toml` builds for it and
`make target-spec-check` fails if anything else drifts. Measured, bit-exact (both
album pins PASS, `xover=0 hfnz=0`, `msasm=true` twin PASS, two runs identical to
the cycle):

| | built-in (`always`) | `may-omit` | delta |
|---|---:|---:|---:|
| 129-frame worst | 210,833 = 80.43% | **208,854 = 79.67%** | −1,979 (−0.75 pts) |
| 129-frame mean | 183,241 = 69.90% | **182,119 = 69.47%** | −1,122 (−0.43 pts) |
| album worst (f16,204) | 210,787 = 80.41% | **209,162 = 79.79%** | −1,625 (−0.62 pts) |
| album mean | 179,226 = 68.37% | **178,341 = 68.03%** | −885 (−0.34 pts) |
| **gate total worst** | 242,370 = 92.46% | **240,705 = 91.82%** | −1,665 (−0.64 pts) |
| **gate total mean** | 210,626 = 80.35% | **209,721 = 80.00%** | −905 (−0.34 pts) |
| margin at worst frame | 7.54 pts | **8.18 pts (21,439 cyc)** | +0.64 pts |
| `.iwram` code | 27,308 B | **26,412 B** | −896 B |
| functions setting up a frame | 13 of 210 | **0** | — |

Three things this settles, each of which had to be measured rather than reasoned:

1. **The rustflag was never the lever.** `-Cforce-frame-pointers=no` rebuilt from
   a clean `target/` produces a **byte-identical ROM**
   (`579fe24dc345aaa13c7f522bc964246a` both ways). Cargo does not invalidate on
   config-content changes, which is why the earlier probes needed the clean
   rebuild to be meaningful. The target spec's value wins, so the spec is where
   the change had to happen.
2. **`non-leaf` would not have helped.** `dec_chan` has ten `bl` sites to
   `huff_slow`/`eg_slow`, so `frame-pointer: "non-leaf"` — the plausible middle
   setting — keeps the frame in exactly the function we most wanted it out of.
   Only `may-omit` frees r11 there. This is not visible from the flag name and it
   is the difference between a ~0.5-point win and nothing.
3. **The stated reason for the pin does not apply here.** The target is
   `panic-strategy: "abort"` — no unwinder, no `core::backtrace`, no
   `RUST_BACKTRACE` — so what remains is debugger walkability, and mGBA debugs
   from the DWARF we already emit. That is the justification; it is recorded
   because a future reader will otherwise assume the pin was load-bearing.

`dec_chan` keeps `fp` in its push list, but as an ordinary **data** register now
(`fp_uses` 40 → 59, stack references 165 → 155, instructions 1,258 → 1,250) —
which is the whole point: the four hand-written kernels already used r11 as data,
so the register was never special, only the Rust ABI's reservation of it.

Maintenance: the spec must stay the built-in one plus that single field.
`make target-spec-check` re-dumps the built-in spec from the *pinned* rustc and
fails on any other difference, including a field appearing on one side only.
`targets/README.md` has the provenance, the regeneration recipe, the two plumbing
traps (rustflags are keyed by the spec's file stem; `[target.*.rustflags]` is not
inherited by `--target <path>`) and the one known cross-track consequence.

#### D — the bit reader loads halfwords as two bytes (**open, planned**)

C: `*(b)->p++` on a `const uint16_t*` — **one `ldrh`**. Ours
(`BitReader::load`) is `*p | (*p.add(1) << 8)`, deliberately, because the stream
is a `&[u8]` and ARM7TDMI has no unaligned halfword access. LLVM therefore emits:

```asm
ldrb r3,[r1] ; ldrb r5,[r1,#1] ; orr r3,r3,r5,lsl #8 ; orr r7,r7,r3,lsl r2
```

**The stream is provably halfword-aligned at every refill**: `set_pos` sets
`next = stream + 2*(bitpos>>4)` and every refill advances by 2, so the only open
question is the base. The fix is an alignment contract on `Album::stream()`
(assert `stream_ptr % 2 == 0` once at construction, or carry a `*const u16` the
way C does) and then a single `ldrh`. **Bounded**: the `s-reader` arm prices the
whole reader's *mechanics* at 4,801 cycles (5.5% of the entropy path), so expect
~1,500-2,000, not more. Planned, and it also removes two instructions of
per-refill shape from every arm that carries a refill.

**LANDED 2026-11-16 — but the projection was wrong about where the win was, and
the alignment contract is a real cost.** Measured: **−755 mean / +820 worst** on
the 129-frame corpus (sum 23,891,692 → 23,794,249), and it shrank `dec_chan` from
1,394 to 1,240 instructions and its stack references from 259 to 174.

What it took, which the projection did not anticipate: `include_bytes!` gives
`&[u8]` **no alignment guarantee**, and the embedded albums were measured landing
on an **odd address** (`0x8010093` in the bench ROM). Odd bases are therefore the
*normal* case here, not an edge case, so a single `ldrh` needed an explicit
contract rather than a cast:

- `uli_lite::Aligned<const N: usize>` (`#[repr(align(8))]`) — a blob wrapper, one
  line at each of the **13** embed sites, because the format's stream is a
  halfword sequence and the C reference reads it through a `const uint16_t *`.
- `Error::UnalignedStream`, checked **once** in `GbaDecoder::new` rather than per
  load, and witnessed by `product_decoder_rejects_an_unaligned_blob` — an
  odd-based blob is rejected by name, not decoded into UB, so a future embedder
  that forgets the wrapper gets a diagnosable error instead of a silent codegen
  regression.

The *reader* change is one line: `(p as *const u16).read()` instead of two
`ldrb` + `orr` + `lsl`. That is the whole code delta; the rest of the work is the
precondition that makes it sound.

#### E — loose ends: checks and shapes C does not pay for (**open, a category**)

This is a **general category, not a single bug**, and it is where the rest of the
entropy gap lives. The instances found so far, each verified in the disassembly:

| instance | ours | C |
|---|---|---|
| `refill` validity test | `if valid < 17` **plus** a nested `if valid < 17` | one `cmp n,#16` |
| `refill` bounds test | `next + 2 > end` **on every refill** | **none** — the stream is length-checked once at open |
| `refill` error path | `bad = true` store | a global only on the rare real error |
| `huff_consume!` | per-symbol `len == 0` compare + `bad` store path | `SKIP(b, e & 31)` unconditional |
| `esc!` fast-path guard | `z < 6 && t < 32 && t <= valid` (3 compares) | `z < 6` (1) |
| `esck` in the escape path | reloaded from the stack per escape | held in a register per band |
| `zero16` | **`bl __aeabi_memclr4`** (3 call sites, ARM→Thumb veneer) | 4 inline `stmia` |
| `huff_at` result | `(u16, u32)` tuple, length re-tested at the call site | packed `(sym << 5) \| len` |

The gate on this category is the same one the record already applies to kernels
("every hot loop gets its own arm and a same-image control"), applied to
**contracts** instead: each of these is a check that exists for a reason in the
abstract — malformed streams, short buffers, unaligned bases — and the question
is whether the product path can pay it **once at open** instead of per symbol.
The answer is almost certainly yes for the bounds and `bad` bookkeeping, because
the album is parsed and length-validated before the first frame.

**Expect this category to be worth more than any remaining kernel item.** It is
not a rewrite: it is a series of small, individually measurable removals.

**LANDED / MEASURED 2026-11-16 — the category is real but its shape is not the
one this table implies.** What the sweep actually returned, in order:

| item | outcome |
|---|---|
| `zero16` → `bl __aeabi_memclr4` | **landed, −1,170 mean.** Four inline `stmia` in asm (the reference's own shape). This was the largest single item in the category and the easiest. |
| reader `ldrh` (D above) | **landed, −755 mean / +820 worst.** |
| `refill` bounds test | **measured, KEPT.** Three shapes were built. Removing the branch via a *branchless clamped load* (`min(next, end-2)` with a predicated move, truncation detected once per channel) **cost +1,352 cycles/frame**: the load address then depends on `end`, which puts an extra link in the critical path ahead of every load and outweighs the 3-cycle taken branch it removes. Folding `bad` into a per-refill `bad \|= over` was worse again (the flag becomes live across the loop). The branch stays, and the three measured shapes are recorded at the macro. |
| `huff_consume!` length test | **not taken.** Removing it needs a failure signal that is not a per-symbol branch; every option tried in design either forces `bad` into memory (the Step-1a trap) or lets a malformed stream spin. `ands`+`beq` is 2 instructions on a not-taken branch. The remaining prize is ~1 cycle/symbol. |
| `esc` guard, `esck` reload | **not taken.** ~2 compares per *escape* (140 on the worst frame), not per symbol; the parser's `esck <= 24` bound makes `t < 32` a live check, so tightening it is a format/parser change, not a codegen one. |

**So: this category delivered ~1,925 mean (1.27% of the frame, 0.75 points of the
period) and its ceiling is now ~2,000 more, not the 3,500-5,500 projected.** The
reason is instructive and matches what 1e-ii found from the other direction:
**almost every item here is a *branch*, and ARM7TDMI's cost model is 1 cycle for
the compare and 3 for the taken branch** — so the win is not in counting
instructions, it is in whether a shape can be made branch-free. Rust cannot emit
ARM conditional instructions (`ldrhne`, `addne` — no `it` block on v4T), so every
predicated C idiom becomes a branch, and the two shapes that *are* expressible
(predicated move, select) cost more in the address chain than the branch saved.
**That is the frame pointer's problem wearing different clothes**: what is left
in this category is bounded by codegen shape, not by slack in our source.

The one item that did pay in the predicted way, `zero16`, paid because it moved
the work from a *call* to straight-line instructions — no branch to remove, just
a call frame and a loop to delete.

#### The method note this audit earns

**By-difference attribution over-states any arm that changes `X`, because the
synthesis kernels' cost is data-dependent.** Removing `pre` feeds the FFT a stale
`buf`; removing M/S changes the coefficients the FFT multiplies. That was already
visible in the record ("the interaction is the FFT's data-dependent
trivial-butterfly path") and this audit gives it a mechanism: `SMULL`/`SMLAL`
cost varies with `Rs`, and a 3.6% frame change came from nothing but reordering
operands. So the per-kernel table ranks work, and the **frame delta of a real
change** grades it — never the other way round. Finding A also makes this
*stronger*: now that data is in `Rs` for the FFT, its cost is genuinely
data-dependent by design (which is the point — quiet frames are cheaper).

### Step 0 done (2026-11-15): the frame, attributed by difference

The per-kernel table this section used to carry chose the next kernel from
numbers taken **before C-3** — while `x`/`buf`/`p` were still in EWRAM — and from
the isolated `ovl` arm, which times the kernels on the bench's **EWRAM** buffers
(`KX`/`KZ`/`KY`/`KP`), not on the product's IWRAM `HotState`. Those readings
overstated every kernel by 21–33%, and C-3's 87,040-cycle gain was never
attributed. It is now.

**Method** (PR C's, one image, `make uli-attrib`): time the product arm, then the
same frames with exactly one part removed, and difference the per-frame windows
at the same frame index. The arms are timing-only cargo features on
`uli-lite-gba` (`attrib-pre`/`-fft`/`-post`/`-ola`/`-synth`/`-entropy`/`-ms`);
the pins fail by construction there and are never quoted. `attrib-dump` costs a
constant **+16 cycles per window**, so the absolutes below are the product arm's
and every delta is a frame-aligned control-minus-arm.

**Re-attributed after Step 1e** (control 215,726 at f98 in the dump image; the
product is 215,710 there and 219,251 at its own worst frame f125). Only the
entropy row moved in 1e — the kernel rows are identical to the 1d measurement,
which is itself the check that the change was confined to `dec_chan`. The rows are
the same instrument re-run in the shipping image each time, not a rescaling of an
earlier table — entropy changed twice and the FFT once, so the shares did:

| part | at f98 | share | C/asm | ratio |
|---|---:|---:|---:|---:|
| `pre` (asm, 1c) | 23,912 | 11.1% | 19,400 | **1.23x** |
| `fft` (asm v5) | **71,520** | **33.2%** | 62,800 | **1.14x** |
| `post` (Rust) | 22,376 | 10.4% | 21,800 | 1.03x |
| `ola` (asm, 1d) | 30,539 | 14.2% | 30,600 | **1.00x** |
| synthesis, all 8 calls | 147,074 | 68.3% | 134,600 | 1.09x |
| entropy + dequant | **58,359** | **27.1%** | ~42,600 | ~1.4x |
| M/S | 9,861 | 4.4% | ~4,400 | ~1.0x |
| wrapper / glue | 432 | 0.2% | — | — |

The sub-frame entropy partition is **unchanged and needs no re-run**: those arms
all carry `attrib-synth`, so they are measured with synthesis removed and v5
cannot move them.

The C column is the record's own per-channel decomposition doubled (pre 9,700 +
fft 31,400 + post 10,900 + ola 15,300) plus its entropy/dequant/M-S row. Those
rows sum to **181,600, within 0.2% of the record's worst frame (181,928)**, so
they are best read as *C's worst-frame shape* — which is the comparison the gate
actually needs. It is still a different corpus and image: a diagnostic, never an
acceptance criterion.

**The two-part split of the gap to C has moved three times, and that is the
point of re-attributing.** Step 0 put 69% of the 92,286-cycle distance in the
entropy path and 31% in synthesis; Step 1 moved it to 58/42; Step 1a to 42/57;
**after Step 1b it was 52/47, after Step 1c 55/44; **after Step 1d it is 62%
entropy (20,932: entropy+dequant+M/S 67,932 against C's ~47,000) / 37% synthesis
(12,474: 147,074 against 134,600)**, out of a total distance of **33,510** at
f98. Three of the four kernels are now between 1.00x and 1.23x C, and the
synthesis block as a whole is 1.09x — so **the remaining deficit is the entropy
path**, and that is where the next item goes. The two halves are now within 10% of each other,
which is the real reading: **neither is "the" critical path any more**, and the
order of work below is about size-per-risk rather than about a dominant block.
Part of the entropy row is frame density, not a slow loop: M/S is 9,861 here
against C's ~4,400 because the gate frame transforms more coefficients.

#### The `fft` row is now bit-exact, and it was **5.5% too high** (2026-11-19)

The re-derived table above left **one** row *derived*: `fft` is in place on `z`, so
the `*2` trick that made `pre`/`post`/`dec_chan` bit-exact does not apply — a
second call would transform already-transformed data. It is now measured, with a
new arm rather than a new method: **`attrib-fft2` restores the input first** (a
second `pre`, which is idempotent into `z`) and then calls `fft` again, so every
later stage sees the bytes it would have. Its delta is `pre` + `fft`;
**`fft = delta(attrib-fft2) - delta(attrib-pre2)`**, and both terms come from
bit-exact images whose pins PASS. `make uli-attrib` now builds `pre2`/`fft2`/
`post2`/`entropy2` alongside the removal arms, and the `*2` arms now set their own
`mask` bits — without that a `-fft2` image printed `mask=0x0` and was
indistinguishable from the control in its own log, which is the same shape of
vacuity as the old `fftasm=true` line.

| | fixture (129) | album (16,553) |
|---|---:|---:|
| `fft` pair, mean | **66,068** | **67,995** |
| min / max | 61,013 (f35) / 68,744 (f70) | 60,983 (f15) / 69,019 (f7,301) |
| at the corpus's worst frame | 67,569 (f98) | 68,375 (f16,204) |
| vs C's 62,800 shape | **1.076x** | **1.083x** (1.089x at f16,204) |

**Two consequences, in order of size.**

1. **The removal arm charged `fft` 71,520 at f98; the honest number is 67,569 —
   a 5.5% overstatement**, and it is the last contaminated row in the table. The
   mechanism is the one the audit named for `pre`: `attrib-fft` leaves an
   *untransformed* `z` for `post` to read, and `post`'s cost is data-dependent
   (1.15x across our own frames), so part of what the arm removed was a change in
   `post`. Pins PASS on both `fft2` and `pre2`, so nothing here is confounded.
2. **The FFT is 1.05x-1.09x C, not 1.14x.** The whole remaining prize to C
   parity is therefore **5,195 on the album mean / 5,575 at f16,204** — about 3%
   of the frame, not the ~8,700 the old ratio implied, and a quarter of the
   ~18,200 the record projected for this block two revisions ago.

**Why instruction work will not close it.** Both loops were counted: ours is ~60
instructions per radix-4 butterfly against the reference's ~61, i.e. genuine
parity (`LOAD_T`+`FORM_Y_P`+three `cmul`s is the same budget we spend on loads,
sum formation and three multiply pairs). At N = 256 complex, 4 radix-4 stages = 256
butterflies per call, and 34,000 cycles per call over 60-instruction butterflies is
**~133 cycles per butterfly** — so the residual is data and memory cycles, not
instructions. The two that are identifiable are loop control: `G_END` is reloaded
from the stack on every butterfly (every general-purpose register is spoken for),
and the k-loop pays a taken `b` at the bottom *in addition to* its head test.
Together that is ~5 cycles x 256 ≈ 4% of the call. **Loop rotation alone was
costed and rejected: it recovers ~256 cycles/call, a tenth of the frame, below
the instrument's noise.** Removing the `G_END` reload needs a register, and the
only place to get one is the reference's register-offset stores
(`str r12,[r0,r1]`), which would release `lr` for the end pointer — that is the
one structural change left in this kernel, and it is worth ~2-4%, not 14%.

**And the register-offset route does not survive cycle accounting either**
(2026-11-19, before writing it). The k-loop names `r0`-`r12`, `lr` and `sp`: all
fifteen are live, so `lr` cannot simply be reassigned — PR #155's freed register
went to **Rust** codegen (`r11`/`sl` in `dec_chan`), and hand-written asm never
had a frame pointer to omit. The reference gets its store addresses from the
**`smull` low word** (dead after the multiply) and pays for it with
register-offset addressing. That is the trade, and on this part it is a wash:
`ldr rd,[rn,rm]`/`str rd,[rn,rm]` each cost **one cycle more** than the
immediate-offset form on ARM7TDMI, and freeing `lr` takes three such stores per
butterfly (+3) to remove one `ldr` at the loop head (-3). **Net zero, with
bit-exactness risk attached.** So the residual 8% against C is multiply and
memory cost on the same instruction sequence, not loop control, and this kernel
is correctly called **done**.

**What this re-prices.** Restating f98 with the honest `fft` row moves the
synthesis block from 147,074 to **143,123, i.e. 1.09x C -> 1.063x**, and leaves
every kernel on the product path at or within 9% of C. Entropy/dequant is then the
only row with a double-digit deficit, and it is worth ~19,000 — roughly four times
the FFT's entire remainder. **The order for "meet or beat C" is entropy first; the
FFT is a 5k item and it is nearly done.**

The FFT is the story of the kernel ratio: **1.33x → 1.29x → 1.14x** C across
Steps 0, 1 and 1b, on a `pre` that is 1.36x and an `ola` at 1.12x. The remaining
synthesis gap is now mostly `pre` (7,058) and `ola` (3,786), i.e. two Rust
kernels, not the asm one.

> **Superseded (2026-11-19).** The paragraph above is the Step 1b-era reading and
> three of its claims are now retired: the ladder ends at **1.08x** (the 1.14x was
> a contaminated row), `pre` and `ola` both measure **1.00x** C in the clean table,
> and neither is a Rust kernel any more. Kept for the record of what the sequence
> of measurements said at the time.

**What it corrects, in order of consequence:**

1. **Entropy/dequant is no longer the single dominant gap** — Step 1 took
   111,227 → 86,826 at f98 (−22%), which is what took the decode wall under the
   period. What remains is ~1.8x C and is **not** in the dequant arithmetic
   (13% of the path) or the reader *mechanics* (5.5%); it is symbol resolution
   and the per-coefficient work around it. The sub-frame arms and the diagnosis
   are in "Step 1" below.
2. **Synthesis is 1.21x C and the FFT is the largest kernel.** Per-kernel rows
   are ±3% (see caveats), so read the sequence row; the FFT's f98 prize is
   ~18,200/frame against C's shape.
3. **`pre` is now the worst kernel ratio (1.36x, up from 1.22x).** Two
   re-measurements a day apart on the same corpus disagree by ~10%; treat any
   single-kernel ratio as a locator, not a grade, and size asm work from the
   sequence row.
4. **The kernels did not change** — synthesis is 162,979 in both attributions
   (162,987 before Step 1). The movement in the shares is entirely entropy.
5. **M/S is closed** (9,844 at f98 vs C's ~4,400 for the pair-dense shape; the
   mean row moved with the frame mix, not with the loop).

### The order of work

Every item is a projection until it lands as its own measured arm, with a
same-image control, twinned bit-exactly (the discipline that has held).
**After the 2026-11-16 root-cause audit the order is re-cut: the remaining work
is five named items with a shape, not a block called "entropy".**

**Landed, in order:**

1. ~~**The asm FFT, v5.**~~ **DONE (2026-11-15): 81,035 → 71,519 at f98, 1.14x
   C, −9,516 on the frame.** See "Step 1b".
2. ~~**`pre` asm.**~~ **DONE (2026-11-15): 26,458 → 23,889 at f98, 1.36x → 1.23x
   C, −2,569 on the frame.** See "Step 1c". The instrument falsified the
   projection: `pre` was *not* address-rebuilt (LLVM's loop was already 15
   instructions with no spills), and the win came from the reference's cursor
   shape — 10 instructions — not from the arithmetic, which is irreducible.
3. ~~**`ola` asm.**~~ **DONE (2026-11-15): 34,375 → 30,539, 1.00x C.** Step 1d.
4. ~~**M/S.**~~ **DONE (2026-11-16): `asm/ms_arm.s`, −2,289 mean / −4,336 worst,
   on-target twin `msasm=true`.** The record had this item sized as ~1,500-2,500
   cycles of "whatever LLVM left"; it was a missing kernel worth ~3,300 by
   instruction count. See "Root-cause audit", B.
5. ~~**The FFT multiply operands.**~~ **DONE (2026-11-16): −6,993 mean / −4,560
   worst.** Not on this list before the audit, and the largest single win since
   Step 1a. See "Root-cause audit", A.
6. ~~**`post` asm.**~~ **DONE (2026-11-18): 18 → 14 instructions per iteration,
   −3,061 cycles on *every* frame** (129-frame 208,854 → 205,793; gate total
   240,705 → 237,646; `cross` 210,209 → 207,148, i.e. 1.139x / 1.117x C). The last
   Rust kernel on the product path, and the only kernel whose excess was nameable
   in instructions. See the measurement ledger.
7. ~~**The kernel table, re-derived.**~~ **DONE (2026-11-18): bit-exact arms
   (`attrib-pre2`/`-post2`/`-entropy2`) replace the contaminated removals.**
   `pre`'s delta was 1.61x too large; the clean table has **`pre` at 1.00x C and
   `ola` at 1.00x, both ahead of C on the mean**, and `post` was the only real
   instruction deficit. Fresh table in the measurement ledger.

**Open, in the order they should be taken:**

6. ~~**The reader's `ldrh` + the "loose ends" contract sweep.**~~ **DONE
   (2026-11-16): −1,925 mean / −1,965 on the 129-frame corpus, −1,695 mean on the
   album, −9,342 worst / −10,619 mean on the gate total.** Two items landed (the
   single `ldrh` behind a new `uli_lite::Aligned` alignment contract, and `zero16`
   as four inline `stmia`); three were **measured and kept or dropped** (the
   `refill` bounds branch, `huff_consume`'s length test, the `esc` guard). The
   projection of 3,500-5,500 was wrong in an informative way — see "Root-cause
   audit", E: **almost every item here is a branch, and ARM7TDMI charges 3 cycles
   for a taken one**, so what looked like instruction slack is really the target's
   lack of predication. Remaining ceiling ~2,000.
7. **The frame pointer** (audit C). One fewer register than gcc in every Rust
   function. Needs a custom target JSON, an answer on unwinding, and a
   whole-image control build. **Dedicated consideration, not a micro-step** — see
   the audit section, and do not size it before the sweep in item 6 has shown how
   much of the entropy gap was contract slack rather than register pressure.
8. **The rest of entropy's arithmetic.** After Step 1d the gap to C is 62%
   entropy / 37% synthesis. The sub-frame arms already say the dequant is 13% of
   the path and the reader mechanics 5.5%, which bounds the arithmetic lever at
   ~2,000/frame on its own — take it last, and after item 6 has re-priced the
   path. **Re-priced 2026-11-19:** the 2026-11-19 examination in "The entropy
   path against the C+ASM reference" replaces this estimate with P1-P5; the
   lever is register pressure and table placement, not arithmetic, and it is
   worth several times 2,000. Read that section instead of this item.

**Maintenance, not optimisation:**

9. ~~**PR E — the album-corpus tail and the gate total.**~~ **DONE
   (2026-11-15): 16,553 frames at the product configuration, worst frame f16,204
   at 220,062 = 83.94% (mean 189,780 = 72.40%) — the 129-frame tail was not an
   underestimate (+0.3 points over 128× the frames, mean *lower*) — and the gate
   total (decode + convert + push + IRQ + DMA) at 251,712 = **96.02% worst /
   221,245 = 84.40% mean** in the player model; the same arm verifying in-loop
   reads 98.33% and starves 12.06 s, and that difference is the harness, not the
   codec. U4's ceiling is the period itself, with 10,432 cycles (3.98 points) of
   margin at the worst frame.** See "PR E" below. **The audit's −9,282 mean must
   be re-taken through this arm** (`make uli-gate`) before U4 is quoted again:
   the 3.98 points of margin were bought at 74.2% mean and are now bought at
   70.7%, so the gate total is expected to fall by roughly the same 3.5 points.
10. ~~**PR D — playback.**~~ **DONE (2026-11-16, mGBA): the seam, the player and
    both output profiles; zero underruns and a clean period audit in each of the
    three builds.** See "Landed 2026-11-16" below. What it leaves: U5 on hardware,
    a multi-track fixture, and the UI redraw's own cost (`late` re-arms — the
    1 Hz text redraw costs 1.6-1.8 periods inside agb's display path).

The arithmetic on the two targets, from these numbers: **<=100% is met (80.2%
worst decode) and the <=85% headroom target is met on the decode wall with
~12,600 cycles of margin** — up from 2,783 before the audit. What remains is
the honest one: the *gate total* was measured at 96.0% worst / 84.4% mean before
these two steps and has to be re-measured (item 9), this tree's UI and art are
not in it, and mGBA does not model the cart prefetch buffer or DMA/code-fetch
arbitration — so U4's ceiling stays written down as the period, not claimed as a
playback result.

### Step 1 — the entropy/dequant rewrite (2026-11-15)

**Two halves, in the order the plan required: the instrument, then the code.**

#### The instrument (`make uli-attrib-subframe`)

Five timing-only features on `uli-lite-gba`, same by-difference method as Step 0
(`attrib-dump` + `attrib-synth` on every arm, so the data-dependent synthesis
cost cannot contaminate a delta):

- `attrib-entropy-reader` — the band loop is replaced by a replay of the frame's
  **true** bit count (`GbaDecoder::replay_bits`, fed from the portable decoder's
  `FrameInfo::bits` in the same image), one `refill`+`take` per 16 bits. This is
  the reader's mechanics at the real bit volume with no symbols at all.
- `attrib-entropy-nolut` — every **coefficient** symbol becomes a fixed 5-bit
  read. Deliberately not applied to the band-class symbol: the first version
  substituted `huff` outright, every band collapsed to zero and the whole path
  measured 1,359 cycles.
- `attrib-entropy-nodequant` — the `rq` load and the `step` multiply removed, the
  sign bit still read (bit stream unchanged).
- `attrib-entropy-nowrite` — the store removed, `black_box` keeps the value.
- `attrib-count` — a `huff_slow` hit counter (`static mut` with volatile access:
  ARMv4T has no `AtomicU32`).

All five share a new `attrib-timing` feature that only relaxes **frame
rejection**: a removed layer desynchronises the stream, and a rejected frame
would abort the run instead of measuring it (a bad frame resynchronises 256 bits
on and continues). Inert in the product build, and it is what let the new arms
run 129 frames each on the first try.

**Partition of the entropy path at f98** (all deltas against `s-base` = 432, the
wrapper, with synthesis removed from every arm):

| arm | f98 | delta | what it prices |
|---|---:|---:|---|
| `s-base` (no `dec_chan`, no synthesis) | 432 | — | wrapper |
| `s-reader` (true bit count replayed) | 5,233 | 4,801 | reader mechanics at the real bit volume — **5.5%** of the path |
| `s-nolut` (fixed 5-bit coefficient symbol) | 15,663 | 15,231 | the whole path with symbol resolution removed |
| `s-nodequant` (no `rq`/`multiply`) | 75,718 | — | dequant arithmetic = **11,540 (13.3%)** |
| `s-nowrite` (no store) | 90,064 | — | **uninformative** — `black_box` costs more than the store it replaces |
| `s-count` (full path + hit counter) | 87,258 | — | the path = **86,826**; the counter costs ~430 |

**The finding that directed the rewrite — and the one after it.** Dequant
arithmetic is 13% and reader *mechanics* 5.5%, so the cost is the per-symbol
work and the traffic around it: the difference to `s-nolut` (~71,600 with ~7,200
of it fewer nonzero coefficients, so ~64,000 at the symbol level) is **~76% of
the path and ~26% of the frame**, i.e. ~154 cycles per Huffman symbol. `huff`
itself is `lsr; ldrh; cmp; shift` — so those cycles are not the instruction
count, they are memory traffic and spills. `dec_chan`'s disassembly confirmed it:
108 stack/structural accesses in 1,491 instructions, and the 24-byte `BitReader`
addressed through one base register, so **every `refill` and `take` stores
`next`/`hw`/`valid`/`bad` back to memory**, with the per-band hoists (`rq`,
`step`, `dq15`) spilled alongside.

`huff_slow` hits: **2,168 over the corpus**, 13.4% of f98's symbols.

#### The rewrite

1. **The reader goes in and out by value.** `dec_chan` takes `BitReader` and
   returns it, the long-escape path takes a copy and hands it back, and
   `huff_slow` no longer touches the reader **at all** — it resolves the code
   from the buffered window. Nothing takes `&mut BitReader` across an
   out-of-line call boundary. (The window still did not get promoted — see
   below — but the `&mut` escape was the *reported* store site and it is gone.)
2. **The refill contract.** `refill` now guarantees **`valid >= 17`** (two
   halfwords at most, the second only when the window was empty) instead of
   looping `while valid <= 16`, and `skip` became **branchless** (`hw <<= n;
   valid -= n`) on the hot path: every `huff`, `esc` and raw-field read is
   preceded by a `refill`, so `n <= valid` always holds. 17 rather than 16
   because a group reads `rbits` raw bits and then a sign bit — `take(16)`
   followed by `take(1)` must not underflow. The resynchronising `skip`/`peek`/
   `get` stay for cold paths and the reader fuzz tests, which still pass.
3. **The long-code search reads the window.** It was `peek`ing up to sixteen
   times and each `peek` could refill, bounds-check and re-derive the window.
   Now it is a pure function of `hw` — the biggest single change for the 13.4%
   of symbols that take it.
4. **The sign comes off the top bit**: `(hw as i32) < 0` then one `take(1)`, where
   it was a `peek` plus a `skip` (two branch-bearing window operations).
5. **A smaller per-band live set**: one `&Huffman` per active table plus its LUT
   pointer (`Huffman::lut_ptr`, new in `uli-lite` under `product`) instead of a
   five-slice `HuffmanParts` value — **ten live words per table** across the loop,
   which was the largest single source of pressure. Dropping it took the
   `dec_chan` stack accesses **218 → 108**.
6. **`rbits == 0` and `rbits != 0` are separate loops** (a const-generic
   `coef::<RAW>`), so no per-coefficient branch on a loop-invariant value.

#### Measured result

Same corpus, same pins, both twin builds bit-exact:

| | worst | mean | of period |
|---|---:|---:|---|
| before Step 1 | 274,198 (f98) | 232,090 | 105% / 89% |
| after Step 1 | **249,835 (f109)** | **220,757** | **95.3% / 84.2%** |
| delta | **−24,363 (−8.9%)** | **−11,333 (−4.9%)** | |

Entropy path at f98: **111,227 → 86,826 (−22%)**. Ratio to C: **1.51x → 1.37x**.
`.iwram` 28,708 B (1,612 B free above the 820 B measured stack); stack
high-water 768 → 820 B.

#### Three things that were tried and did not work (recorded so they are not
repeated)

1. **The reader ABI.** By-value in/out (249,835) vs `&mut` (252,768 worst, +0.2%
   mean) — no difference: a `&mut` handed to an out-of-line function pins the
   window in memory, and a by-value parameter plus a by-value return makes LLVM
   coalesce one alloca and keep every field in memory anyway.
2. **`let mut br = br;`** — a local copy to force promotion. No cycles, and
   **+788 B of `.iwram`**, which left 732 B for an 820 B stack and corrupted the
   `ovl` pin (caught by the bench, not by the host twins, which were green
   throughout). Withdrawn; the reason it failed is recorded at the call site.
3. **Inlining `dec_chan` into `decode_frame` to make the window a promotable
   local.** `#[inline(always)]` together with `instruction_set(a32)` and
   `link_section(".iwram")` gets **outlined into `.text`**, i.e. cartridges —
   758,912 cycles, **3.0x** worse. A reminder that placement, not inlining, is
   the dominant term at this fetch ratio.

4. **The C-style `m == 1` hoist (`rq[1] * step`).** Measured: mean 219,747 with
   it vs 220,757 without (−0.46% for the hoist), but its own worst frame was
   **250,945** against **249,835** without. The hoist's two operands are spilled,
   so the branch costs more than the EWRAM `rq` load it avoids. **Not taken**;
   the simpler rule stands and `r1` was removed entirely.

#### What this left as the next lever — and Step 1a took it

The reference gets its window into registers because `REFILL`/`SKIP` are
**macros over `let mut` locals inside `dec_chan`** — its `BR` is three words and
every helper is `always_inline`, so the whole thing is straight-line code over
locals. Any Rust restructure that keeps the reader in a *struct* keeps it in an
alloca (measured three ways above). That was item 1a.

### Step 1a — the window into registers (2026-11-15)

**What changed.** `dec_chan` destructures the incoming `BitReader` into five
`let mut` scalars (`next`, `end`, `hw`, `valid`, `bad`) and `refill`, `take`,
`top`, `esc`, the symbol consume and the whole per-coefficient body are **macros
over those scalars** — the reference's `REFILL`/`SKIP` shape, in Rust. The pure
decoders (`huff_at`, `coef_huff_at`, `huff_slow`) now take `hw` by value instead
of the reader, so nothing on the hot path forms a `&mut BitReader`; the long
escape hands a *copy* of the struct to the out-of-line `eg_slow` and takes it
back. The general `peek`/`skip`/`get` remain for cold paths and the reader fuzz
tests, which pass unchanged.

**Measured.** Same corpus, same pins, both twin builds bit-exact:

| | worst | mean | of period |
|---|---:|---:|---|
| after Step 1 | 249,835 (f109) | 220,757 | 95.3% / 84.2% |
| after Step 1a | **237,252 (f109)** | **211,071** | **90.5% / 80.5%** |
| delta | **−12,583 (−5.0%)** | **−9,686 (−4.4%)** | |

Cumulative from the Step 0 baseline (274,198 / 232,090): **−36,946 (−13.5%)** and
**−21,019 (−9.1%)**. Ratio to C: **1.37x → 1.30x** worst. Entropy path at f98:
86,826 → **69,364** (−20% here, −37.6% over both steps).

Confirmed in the disassembly, which is the whole point: `hw` now lives in a
register (`lsr r7, r8, r7` / `orr r8, r1, r8` in the pair body) and the `bad`
byte is stored 38 → 6 times. `strb`-per-refill is gone.

**What did not move, and what that means for the instrument.** The sub-frame
`attrib-entropy-reader` floor is **unchanged at 4,801**, because the replay arm
composes the same `refill`/`take` primitives and was *already* register-resident.
So that arm is a floor on the mechanics, **not** a measure of the struct-ABI
penalty — the 12,583 came from removing the store-back traffic in the *real*
path, which no arm in this set isolates. Read it that way, and note it for the
next instrument.

**Also measured, and one thing not taken:**

- **`eg_slow` was 1,820 B of `.iwram`** — more than a fifth of the audio code
  budget — because it inlined `peek`, `skip`, `set_pos` and `pos` at every step.
  Rewritten to walk bits with the hot primitives: **968 B**, and the frame moved
  237,084 (at f93) → 237,252 (at f109), i.e. ~57 cycles of mean cost. Kept.
- **Moving `eg_slow` to the cartridge instead** (it is the coldest code) measured
  **+22,657 cycles on the worst frame**. A long escape prefix is rare but not
  cheap; it stays in IWRAM. Recorded because "cold path, move it to ROM" is the
  intuitive move and it is wrong here by a factor of ~10.
- `.iwram` is now **28,452 B with 1,040 B free** above the 828 B measured stack
  (it was 1,612 B free before Step 1 and 244 B free mid-1a). `dec_chan` is
  6,432 B and `decode_frame` 1,500 B. The `verify` image overruns the region
  (0x77c4 against the 0x7670 limit), which is the pre-existing blocked instrument
  from Step 0b made worse; it is memory, not arithmetic.

#### What this leaves as the next lever

Synthesis. After two entropy steps the deficit is 42% entropy / 57% synthesis,
and the **FFT alone is 35% of the frame** at 1.29x C — the only single block
large enough to close the ~14,400 that <=85% needs. Inside entropy, the largest
remaining pieces are the dequant arithmetic (13,129, 19% of the path) and the
per-band hoists that still spill beside the window (`rqp`, `d15`, `step` reload
per coefficient in the disassembly) — real, but together worth less than the FFT
prize.

### Step 1b — FFT v5 (2026-11-15)

**Target.** In-frame the FFT was **81,035 at f98 = 40,518/channel = 1.29x C's
31,400**, 35% of the frame and the largest single item. Two differences from the
reference were named before the work started: the **A..D / output traffic** and
the **sum formation**. Both turned out to be address-and-move problems, not
multiply problems — the multiply core was already identical (`2x SMULL/SMLAL` +
one `rsb` per complex product, both sides).

**What changed in `asm/fft_arm_v5.s`:**

1. **An absolute byte cursor** (`r2`) and the element stride in bytes (`r3`)
   replace v4's word index rebuilt from `(p, t, x)`. The four complexes are one
   `ldmia` at the cursor plus three `add r0, r0, r3` steps — **7 instructions for
   the loads where v4 needed 12** — and each store address is one or two `add`s
   from `(cursor, stride)` with no shift-per-word conversion.
2. **The destructive sum formation** from the reference: form the sum into one
   operand's register and derive the difference from the sum with a shifted
   operand (`rsb t1, t0, a, lsl #1` ⇒ `2a - t0 = a - c`). **8 instructions with
   no moves, where v4 needs 12 with four.** Both half-B Y values come from the
   same trick (4 instructions where v4 needs 6).
3. **The twiddle pointer lives in a register for the whole k-loop** (`r12`).
   That is possible only because the cursor is *absolute*, so the buffer base is
   not needed inside the loops and frees the register; v4 reloaded `F_TWJ` from
   the stack once per output (3 loads per butterfly) and indexed from it.
4. The trivial-butterfly path (j == 0, identity twiddles) and the block transfers
   are kept from v4; the tail stage becomes one `ldmia`/8 `str` on a single base.

**Measured.** Same corpus, same pins, both twin builds bit-exact:

| | worst | mean | of period |
|---|---:|---:|---|
| Step 1a | 237,252 (f109) | 211,071 | 90.5% / 80.5% |
| **Step 1b** | **227,736 (f109)** | **201,554** | **86.9% / 76.9%** |
| delta | **−9,516 (−4.0%)** | **−9,517 (−4.5%)** | |

FFT at f98: **81,035 → 71,519 (−9,516, exactly the frame delta — the change is
confined to the kernel)**, i.e. **35,760/channel = 1.14x C**, the closest any
kernel has come. Cumulative from the Step 0 baseline: **−46,462 (−16.9%)** on the
worst frame, −30,536 mean. Ratio to C: **1.30x → 1.25x**. `asm/fft_arm_v5.s` is
**764 B**, 192 B *smaller* than v4's 956 B despite doing more per instruction.

**The bug worth recording: `smull` writes a register pair.** The first v5 stored
each output's address in `r0`, then did the real half of the complex multiply as
`smull r0, r1, ...` — destroying the address and writing the real part to a
pointer built from a product's low word. It corrupted memory on target
(`BadFrame`, 486 of 512 coefficients wrong) and the signature was diagnostic:
**every imaginary part was right and every real part wrong**, because only the
second `smull` of each product clobbered the address. v4 keeps its store address
in `lr` for exactly this reason; v5 now does too, at the cost of one bound `ldr`
per butterfly (every other register is spoken for). Found by transliterating the
kernel into a host test and comparing against the reference — the transliteration
reproduced the target's first divergent index exactly, which turned a guess into
a one-line fix.

**Memory — and the instrument gap it exposed.** `.iwram` is **28,276 B with
2,044 B free above the 828 B stack (1,216 B of headroom)**, slightly better than
before v5 because v5 is smaller than v4. v4 is **demoted to `.text`** as a
comparison arm, exactly as v1/v2/v3 already were — which means its cycle number
in that image is a fetch-polluted ROM reading: measured, the same kernel at
`asmfft4` reads **179,694 cycles/call from ROM against 57,094 for v5 in
`.iwram`**, a 3.1x placement penalty and *not* a v4-vs-v5 comparison. Do not
quote it as one.

Two things about the instrument were wrong and are now fixed:

- **`ovl twin ... fftasm=true` was vacuous in `make uli-bench`.** The twin lives
  behind the bench's `asm-fft` feature and `make uli-bench` does not enable it,
  so from PR C-1 until now that line reported `true` without comparing anything
  — while the product decoder was calling the asm kernel. The album pins were the
  only real witness. **`make uli-fft-twin`** now builds the image that runs the
  twin and gates on it.
- **That image could not fit until `TW_IWRAM` was gated.** The twiddle-placement
  arm allocated a 2,016 B `.iwram` copy of `TW`, which put the build at
  **30,284 B of a 30,320 B region** — no room for the 828 B stack, so the twin
  could not run at all. Since `iwram-tables` became the default, `crate::TW` is
  *already* in `.iwram`, so that copy was comparing the table with itself; it is
  now behind `tw-iwram-arm` (off by default). This is the second time a
  measurement-only allocation has silently eaten the budget.

**What it leaves.** The two halves of the gap to C are now within 10% of each
other (52% entropy / 47% synthesis) and **~4,900 cycles buys the <=85%
target**. `pre` (1.36x) is the largest remaining kernel and the cheapest thing
to try; the rest of entropy (dequant 13,129, plus the spilled per-band hoists) is
the bigger prize but has already had three passes.

### Step 1c — the `pre` pre-twiddle (2026-11-15)

**Target.** `pre` was **26,458 at f98 = 13,229/channel = 1.36x C's 19,400** —
the worst kernel ratio in the table and the largest kernel not already in asm.

**The instrument changed the plan, which is the point of running it first.** The
prediction was an addressing win, on the FFT's evidence. The disassembly said
otherwise:

- `pre` is **15 instructions per output pair, 96 B, with zero stack traffic**,
  and its addressing is already folded (`ldr ip, [r0, -r3]`, `ldr r8, [r2], #8`,
  `str r5, [r1], #8`). LLVM did well. There was no spill to remove.
- The **reference's** loop is **10 instructions**, and the difference is pure
  bookkeeping: it keeps **four post-indexed cursors** (forward X, backward X,
  PRE, z) and stores a whole output pair with one `stmia`, where LLVM computes
  the backward address from an index (`add` + indexed `ldr`), splits the store
  into a pre-indexed `str` and a post-indexed `str`, and runs a 3-instruction
  loop tail (`sub`/`cmn`/`bne`) instead of `cmp`/`bcc` on a pointer.
- The four multiplies are the algorithm's own — a complex product is four
  `smull`/`smlal` chains, and on an ARM7TDMI a multiply blocks the pipeline
  rather than overlapping, so there is nothing to hide behind it. **The
  instruction count around the multiplies was the only lever**, and the record
  should say so plainly: this is not the FFT's story.

**What changed** (`asm/pre_arm.s`, new `asm-pre` feature, on by default): the
reference's shape — four cursors, `ldmia` for the twiddle pair, one `stmia` per
output pair, pointer bound. **10 instructions per pair, 76 B.** `pre_kernel`
dispatches to it exactly as `fft_kernel` does, and the Rust ARM-mode `pre` stays
in `.iwram` as the in-image control.

**Measured.** Same corpus, same pins, both twin builds bit-exact:

| | worst | mean | of period |
|---|---:|---:|---|
| Step 1b | 227,736 (f109) | 201,554 | 86.9% / 76.9% |
| **Step 1c** | **225,168 (f93)** | **198,992** | **85.9% / 75.9%** |
| delta | **−2,568 (−1.1%)** | **−2,562 (−1.3%)** | |

`pre` at f98: **26,458 → 23,889 (−2,569, exactly the frame delta)**, i.e.
**11,945/channel, 1.36x → 1.23x** C. Cumulative from the Step 0 baseline:
**−49,030 (−17.9%)** worst, −33,098 mean. Ratio to C: **1.25x → 1.24x**.
`.iwram` 28,208 B with 1,968 B free above the 828 B stack (1,140 B of headroom);
the asm `pre` costs 76 B and the Rust control stays.

**The honest note on the C row.** `pre` is 1.23x C now and its *instruction*
shape matches the reference's, so the residual is not code structure. A 37.9
cycle/pair C figure cannot be reached by this instruction mix on an ARM7TDMI
(4 multiplies alone are ~20 cycles, plus 4 loads, 2 stores and a branch), which
means the C row in the table — like every per-kernel row — is a *shape*, not a
comparable measurement: different corpus, different image, ±3% at best and up to
10% between re-measurements. Do not read 1.23x as "23% of headroom left".

**The gate, generalised.** Adding a second asm kernel exposed that the twin gate
had to cover kernels, not one kernel: `make uli-fft-twin` is now
**`make uli-kernel-twin`**, builds with `asm-fft,asm-pre`, and requires
`fftasm=true preasm=true` in the `ovl twin` line. That is the second time this
session that a witness turned out narrower than the claim it backed.

### Step 1d — the overlap-add (2026-11-15)

**Target.** `ola` was **34,375 at f98 = 17,187/channel = 1.12x C's 30,600** — the
third kernel and the last one on the product path that was still compiler
output.

**The instrument, and what it showed.** Our fused int8 loop was **33
instructions** per iteration; the reference's is **26**. The excess was in two
places, neither of them the arithmetic:

1. **The clamp: 8 instructions per output against 6.** LLVM saturates straight to
   int8 (`cmn`/`mvn`/`asrgt`/`cmp`/`movge`). The reference saturates to int16 with
   a three-instruction idiom — `mov t, x, asr #15 ; teq t, x, asr #31 ; eorne x,
   0x7FFF, x, asr #31` — then shifts right 8 and stores a byte. That is the same
   function: `sat8(x >> 8) == sat16(x) >> 8` for every i32, because
   `32767 >> 8 == 127` and `-32768 >> 8 == -128`.
2. **The addressing: 7 instructions of index arithmetic against ~1.** The
   reference keeps **six post-indexed cursors** (a forward, b backward, wa
   forward, wb backward, out forward, out backward) and reads the overlap word
   with `ldr r9, [b_cursor, Y_minus_P]`, where `Y_minus_P` is a byte constant
   computed once at entry. LLVM rebuilt four of the addresses per iteration from
   the loop index.

The four multiplies are the algorithm's own and are untouched — the same
`smull`/`smlal` pair on the same operands, including the reference's
`+0x80`-on-the-high-word form of the fused `(acc + 2^39) >> 40` rounding, which
is exact because 2^39's low word is zero and the accumulator's cannot carry into
the high word.

**What changed** (`asm/ola_arm.s`, new `asm-ola` feature, on by default): a
`OLA_FN mode` macro in the reference's shape generating **both** output formats —
`uli_ola_s8` (the shipping headphone path) and `uli_ola_s16` (the pre-DAC
profile) — at **160 B each, 26 instructions per iteration**. `ola_s8_kernel`
dispatches exactly as `fft_kernel`/`pre_kernel` do; the Rust kernels stay in
`.iwram` as the in-image controls.

**Measured.** Same corpus, same pins, both twin builds bit-exact:

| | worst | mean | of period |
|---|---:|---:|---|
| Step 1c | 225,168 (f93) | 198,992 | 85.9% / 75.9% |
| **Step 1d** | **221,332 (f93)** | **195,154** | **84.4% / 74.4%** |
| delta | **−3,836 (−1.7%)** | **−3,838 (−1.9%)** | |

`ola` at f98: **34,375 → 30,539 (−3,836, exactly the frame delta)** =
**15,270/channel = 1.00x C** — the first kernel at parity. Cumulative from the
Step 0 baseline: **−52,866 (−19.3%)** worst, −36,936 mean. Ratio to C:
**1.24x → 1.22x**. `.iwram` 28,632 B with 1,664 B of region above the 836 B
measured stack (828 B of headroom).

**Two bugs, both mine, both caught before any number was quoted.** The first
draft used `H` where `M` belongs: the output cursors start at `out[M-1]` (= 511
for int8, 1022 for int16) and the window cursor at `WIN[M-1]`, but I wrote `H`'s
half-frame values, so the loop ran **128 iterations instead of 256** and wrote
half a frame. The bench's album pin caught it (`cb98d597` vs `e6e87652`) on the
first run, and the frame *looked* 18,900 cycles faster — which is exactly the
kind of number a wrong loop bound produces. The second was an ARM immediate:
`add r6, r2, #511` is not encodable (511 needs `#512 ; sub #1`), and the
assembler rejected it rather than silently assembling something else.

**The gate, again.** `make uli-kernel-twin` now builds
`asm-fft,asm-pre,asm-ola` and requires `fftasm=true preasm=true olaasm=true`.
The `olaasm` twin replays both formats from the same saved overlap state, because
OLA *mutates* `p` — comparing only the samples would miss a wrong `P` write.

### Step 1e — budget work: memory and a falsified hypothesis (2026-11-15)

Three arms, each measured on its own, taken because the remaining cycles are
incremental and the memory was worth banking while it was cheap.

#### 1e-i — the Rust control kernels left `.iwram` (−496 B, zero cycles)

`pre`, `ola_s8` and `ola_s16` are the in-image **controls** now that asm kernels
exist, so their placement became conditional on the asm feature being *off* —
exactly the gate `fft` has carried since C-1 on `asm-fft`, and the closure of a
line I drew in 1b/1d but never followed up. **496 B freed and the frame is
byte-identical**, because nothing on the gate path calls them.

`post` and the dual `ola` were **left in `.iwram`** and the reason is recorded:
`post` *is* the product kernel (`decoder.rs` calls it directly; there is no asm
`post`), and the dual `ola` is the int16/pre-DAC profile's kernel. Moving either
would have bought ~456 B more by making a shipping path 2-3x slower.

#### 1e-ii — register relief in `dec_chan`: measured, rejected

The 1c/1d lesson was "stop touching memory per element", and `dec_chan`'s
disassembly showed `rqp` reloaded from the stack on **every coefficient** and
`d15` on every escape — so the obvious next move was register relief.

- Folding `bad` into a `valid` sentinel looked like a clear win: it deleted the
  byte stores and **shrank `dec_chan` from 1,610 to 1,192 instructions** (−1,672 B
  of code). Measured: **+1,450 cycles mean.** A −1,672 B / +1,450 cycle trade.
- Adding the intended relief on top — dropping `esck` and `dq15` as live values
  and deriving them inside the rare branches — made it **worse still: +3,351
  cycles mean**, for −1,472 B.

**The reason is worth carrying forward: indexing `tables` inside those branches
resurrects `tables` as a live value across the whole loop.** It had been dead
after the per-band loads; the "relief" made it live again, which is precisely the
pressure the change was meant to remove. Both variants were reverted, and the
register-pressure hypothesis for this kernel is **falsified** — the second such
falsification in this record after the FFT's v1, and the same lesson: the
register count is not the lever that the *code shape* is.

#### 1e-iii — the cursor rewrite (−856 B, −665 cycles mean)

`dec_chan`'s three coefficient loops and the band loop now advance **pointers**
and compare them against an end pointer, rather than carrying `i`/`b` counters
and computing `xp.add(i)` / `x.add(b * BANDW)`. The verify-only `hfnz` format
rule became a pointer compare against `x + NCODED`, which is what let the
counters go at all.

| | worst | mean | `.iwram` |
|---|---:|---:|---:|
| after 1d | 221,332 (f93) | 195,154 | 28,136 B |
| **after 1e** | **219,251 (f125)** | **194,489** | **27,280 B** |
| delta | −2,081 | **−665** | **−856 B** |

**The mean is the honest comparator** — the worst frame moved from f93 to f125,
so worst-vs-worst is not like-for-like, while the sum over 129 frames is. And
665 is a *third* of what `pre` (2,569) and `ola` (3,836) got from the same shape:
`dec_chan`'s coefficient stores were already index-scaled by LLVM
(`str r0, [r3, r4, lsl #2]`), so unlike those kernels there were no rebuilt
addresses to remove — only the counters.

#### Net, and the state of the memory budget

`.iwram` **28,632 → 27,280 B (−1,352)**; product headroom **828 → 2,196 B**.
Code is now 9,860 B and data 16,864 B (`HotState` 8,192 + tables 8,672), so for
the first time **data is the majority of the audio budget** and code is no longer
the binding constraint.

The `verify` image — the build that witnesses `xover`/`hfnz` and per-op counts
for the product decoder — is still blocked, and by less than before:
**29,716 B of `.iwram` against a 30,320 B region, with an 836 B stack, i.e. 232 B
short.** It needs one more decision rather than more work: the dual `ola`
(344 B) would clear it, at the cost of making the int16/pre-DAC profile pay a
cartridge fetch. That trade is deliberately **not** taken here.

#### Handing off — what is left, and what to do first (2026-11-15, revised 2026-11-16)

> **Superseded in its ceilings by the "Root-cause audit" above (2026-11-16).**
> Read the order of work there first. This section is kept because its *measured*
> table and its checklist still hold; its *projections* were three named items
> short and one of them (M/S) was wrong by 2x in the wrong direction. Where the
> two disagree, the audit wins.

**The marginal return, per step, on the mean — which is the honest comparator,
because the worst-frame index moved six times (f98 → f109 → f93 → f125 → f83) and
a worst-vs-worst delta across different frames is not like-for-like:**

| step | what | mean cycles saved |
|---|---|---:|
| 1 | entropy/dequant rewrite | 11,333 |
| 1a | the bit window into registers | 9,686 |
| 1b | FFT v5 (cursor + sum formation) | 9,517 |
| 1c | `pre` in the reference's cursor shape | 2,562 |
| 1d | `ola` in the reference's cursor shape | 3,838 |
| 1e | budget work (controls to ROM, cursor rewrite) | 665 |
| **A** | **FFT multiply operands (audit, 2026-11-16)** | **6,993** |
| **B** | **M/S block-transfer kernel (audit, 2026-11-16)** | **2,289** |

**The "flat curve" reading was wrong, and that is the audit's headline.** Six
micro-steps returned 665 to 9,686 each and were trending to ~1%; the audit's two
items returned 6,993 and 2,289 in one pass, because they were *structural*
comparisons against the C source rather than further tuning of code that already
matched. The curve is flat only while the question is "what else can I tighten
in this loop"; it is not flat when the question is "what does the reference do
that I do not".

**What is left, ranked, with the ceiling each one is bounded by:**

1. ~~**The reader `ldrh` + the loose-ends contract sweep.**~~ **DONE 2026-11-16:
   −1,925 mean, −9,342 worst on the gate total.** Audit D and E. The projection
   (3,500-5,500) was right about the category and wrong about the mechanism: the
   win was `zero16`'s deleted *call* and the deleted *loads*, not the deleted
   branches — every branch-removal shape measured **worse**. Remaining ceiling
   ~2,000, and it needs predication the target does not have.
2. **The frame pointer — unsized, deliberately.** Audit C: one fewer register
   than gcc in every Rust function, pinned by the target spec. Needs a custom
   target JSON, an unwinding answer, and a whole-image control. **Do not size it
   before item 1 lands** — item 1 may show that the entropy gap was contract
   slack, not register pressure, and the two levers overlap (`dec_chan`'s
   validity/bounds tests are exactly what occupies the registers r11 cannot).
3. **Entropy's dequant arithmetic — 13,129 (19% of the entropy path), ceiling
   ~2,000.** Bounded by the sub-frame arms: the reference does the same four
   multiplies. What is left is the two-compare `mm < 16 / >= 16` split and the
   per-band `rqp` reload — exactly what 1e-ii tried and failed on. Take it last,
   after item 1 re-prices the path.
4. ~~**M/S — ceiling ~1,500-2,500.**~~ **DONE 2026-11-16 at −2,289 mean / −4,336
   worst.** The old text is struck because it was wrong in an instructive way: it
   called M/S "the only hot loop nobody has disassembled", sized it at ~1.0x C per
   coefficient, and blamed the 9,861-vs-4,400 difference on "frame density, not a
   slow loop". It was a slow loop, C's was hand-written asm, and the missing
   kernel was worth ~3,300 by instruction count.
5. **`pre`'s residual — 4,512.** Best read as *measurement*, not headroom: four
   multiplies are ~20 of its ~47 cycles, so a 37.9 cycle/pair C row is not
   reachable by this instruction mix. Expect **~0** — and note the audit's
   explanation for the apparent 1.23x: the loop is **instruction-for-instruction
   identical to C's**, and Rs is the data in both, so the ratio is corpus and
   multiply-magnitude, not code.

Realistic total left: **~5,500-7,500 cycles, i.e. 70.7% → ~68-68.5% on the
mean**, *before* the frame-pointer item — which is exactly the wrong shape for a
ceiling and the reason it is listed unsized rather than guessed.

**Two negative results worth not repeating:**
- **`huff_slow` is already clean.** It was flagged in this file as likely building
  five fat pointers on the stack per long code (13.4% of the worst frame's
  symbols). Disassembled: LLVM folds `parts()` entirely into **constant offsets
  from the struct base** (`ldrh r2, [r1, #0xb16]`), so the prologue is
  `push {fp, lr}` and the function is 212 B. There is no slice construction to
  remove, and the LUT-geometry lever remains the only one there.
- **Register relief in `dec_chan` does not pay** (1e-ii): −1,672 B of code for
  +1,450 cycles, and worse with derived per-band values, because indexing
  `tables` makes it live again. **But the audit re-opens the *conclusion***: the
  experiment tested for a free register in a function the toolchain had already
  spent r11 on (audit C). The measurement is sound; the hypothesis was disabled
  before it was run. Do not cite 1e-ii as "registers are not the lever" until
  the frame-pointer arm has been run.

**The transferable checklist**, earned across these steps and worth applying
to the feeder and UI when they are written:

1. Every hot loop gets its own arm and a **same-image control**, or its number is
   a guess. Two of six steps changed their plan *because* the instrument was run
   first, and one was rejected outright by its own measurement.
2. **Cursors, not indices.** If LLVM already folded the index (`dec_chan`), expect
   a third of the prize the kernels got.
3. **One pointer, not ten words.** A `HuffmanParts`-shaped value live across a
   loop was the largest single pressure source found.
4. **`smull` writes a register pair**, so a store address in `r0` dies mid
   multiply. Keep it in `lr`.
5. **Cold-but-per-frame code stays in `.iwram`**: `eg_slow` from the cartridge
   measured **+22,657**.
6. **Measurement-only code never gets a default feature** — the `TW_IWRAM` copy
   and the vacuous `fftasm=true` both cost real understanding, and the second was
   a *witness* that had never compared anything.
7. **Read the reference's *code*, not its summary table** (added 2026-11-16). The
   audit's two items were invisible to six steps of §*attribution* and obvious
   from one reading of `uli_gba_dec.iwram.c` and `uli_imdct_arm.S`.
8. **A ratio against a summary number is a locator, never a grade.** Two of the
   four kernel ratios in this file (M/S and `pre`) named a cause that was not
   there; one (the FFT) hid one that was.

**And the one thing that should gate further decoder work:** the feeder's
convert + IRQ + DMA cost must be **re-measured** before U4 is quoted again —
PR E's 96.02% / 84.40% gate total was taken at 74.2% mean, and the decode wall is
now 70.7% mean (−9,282 cycles), so the gate total is expected to fall by
~3.5 points to roughly 92.5% worst / 81% mean. That re-measurement (`make
uli-gate`) is cheaper than the next kernel and is the only thing that turns these
numbers into a real-time claim. The budget now has **19.8% of the frame and
~2,196 B of IWRAM** spare, both unverified against what they reserve margin *for*.

## Phases and PRs

### PR A — GBA-first architecture + full-frame instrument (landed 2026-11-14)

- The decision above, recorded here and in `ULI2.md`.
- The `uli_lite::Kernels` seam and `PortableKernels`; `Decoder<K>` generic with
  a default.
- `uli_lite_gba::ArmKernels` implementing the seam with the existing ARM/IWRAM
  kernels.
- `examples/uli_bench` decodes **full frames through both kernel sets** and
  keeps the core as the control; the per-kernel arms and the on-target twin
  stay.
- Host twin: `uli-lite-gba` decodes the committed corpus frame-for-frame
  through `ArmKernels` and compares every sample with the core.
- Gate: both full-frame arms match the corpus pins, `xover=0 hfnz=0`, no
  wrapped window. No speed claim beyond the recorded number.

Result: **PR A measurement** (full-frame worst / mean, 129-frame corpus,
262,144-cycle period):

| arm | worst | mean |
|---|---:|---:|
| core kernels (control, same image) | 2,676,986 (1021%) | 2,171,620 (828%) |
| overlay kernels, full frame | 1,829,247 (**698%**) | 1,322,800 (**505%**) |

The within-image A/B is the valid one: **1.46x worst / 1.64x mean**. The
overlay full frame is exactly `core_rest + 2 × overlay_seq` within 0.02%,
which confirms the Phase 1b decomposition. The core control in this image is
~8% faster than Phase 1b's image (2,970,042 → 2,676,986 worst) although the
kernel sequence is unchanged (667,264 both times); the difference is code
layout and instruction fetch, not arithmetic. **Absolute frame numbers are
image-specific; the control must live in the same image as the arm.**

The Phase 1b projection (810% / 534%) is therefore replaced by a measurement:
**698% worst / 505% mean** for the product kernel set. Still ~7x over the
frame period; PR B's gate is ≤2x.

### PR B — GBA-first hot path (landed 2026-11-14)

This is where the retired constraints are exercised.

- Move the hot bit reader, Huffman decode, dequant and M/S into
  `uli-lite-gba`: `GbaDecoder` + `HotState` in `src/decoder.rs`.
- 32-bit cached bit window (no 64-bit shift helpers), sentinel `error` instead
  of `Result` per symbol, unchecked pointers with documented bounds, ARM
  state, `.iwram` code, cold paths out of line.
- Keep the core as the oracle: frame-for-frame host twinning over the corpus
  (`tests/product_twin.rs`), and the album pins on target.
- Gate: **full-frame worst ≤2× the 262,144-cycle period**, pins unchanged.
  **Met: 204% worst / 177% mean.**

Result: **PR B measurement** (full frame, mGBA, WAITCNT 4/2, 129-frame corpus,
262,144-cycle period):

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control, same image) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path (`GbaDecoder`)** | **533,605 (204%)** | **463,660 (177%)** | 351,556 |

All three arms pinned `e6e87652` / `b3530978`, `xover=0 hfnz=0`; the on-target
twin passed. The product frame is `rest + 2 × overlay_seq` within 0.1%:
overlay seq/channel 169,061, so **`rest` (non-synthesis) fell from PR A's
837,092 to 195,484 cycles/frame — a 4.3× cut**, and synthesis is now 63% of the
frame (it was 27% at PR A). The ladder: 2526% (Thumb core) → 1133% (ARM core)
→ 698% (PR A kernels) → **204%** (PR B hot path).

**Placement finding (recorded because it cost a hang).** PR B's first on-target
run hung with `pc=0x00000004` (the undefined-instruction vector). Root cause:
`HotState` (8 KiB) placed in `.iwram` left only `0x03007f00 − __iwram_end`
(≈14 KiB) for the stack, and the product path needs **≈15.0 KiB** (measured
stack pointer floor `0x03004440`, i.e. the stack overflowed past `__iwram_end`
into `PRODUCT_HOT` and the ARM/Thumb thunks, then branched into uninitialised
IWRAM). The host twin was green throughout, so this was purely a target
placement defect, not arithmetic. **`HotState` lives in EWRAM in the bench**;
its IWRAM placement is a U6 decision that must first budget stack, not just
code+data. Weathervane: the ARM build was 1,236 B larger in `.iwram` than the
Thumb one, and the handful of bytes between `__iwram_end` and the stack floor is
all the margin there was.

Open item this buys: the remaining synthesis gap. `ArmKernels` seq/channel is
169,061 against the C/asm reference's ~67,300 (2.51×); the OLA is still the
outlier. PR C is now scoped to kernels, not to the entropy path.

### PR C — real-time: attribution first, then the two rewrites (in progress)

PR C opened by measuring where the 523,267-cycle worst frame actually goes,
because at 2.0× the period a guessed optimisation is as likely to move code
into a worse register allocation as to help. The attribution below is the
substance of this PR; the code change it produced is small and the remaining
gap is scoped as two named rewrites.

**Attribution, worst frame (frame 101), 523,267 cycles, mGBA 4/2:**

| part | cycles | share | of which |
|---|---:|---:|---|
| synthesis kernels (2 × pre+fft+post+ola) | 328,502 | 63% | fft 170,130, ola 76,932, pre 42,478, post 41,238 |
| entropy + dequant (`dec_chan` × 2) | ~175,000 | 33% | huff, bit reader, dequant |
| M/S + frame wrapper | ~20,000 | 4% | |

Measured by difference on one image: total frame, then the same frame with
the eight kernel calls removed (194,765), then with `dec_chan` removed too
(~20,600). The kernel figure agrees with `2 × overlay_seq` to 3%.

The two numbers that set the work:

- **Kernels are 2.4× the C/asm reference** (328,502 vs the C record's
  **134,600** for the pair: 67,300 per channel, from its own stage-2
  decomposition `pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k`). The FFT
  butterfly alone spills 66 intermediates to the
  stack because 14 values are live at once against 16 architectural registers;
  the multiply codegen is already optimal (6 `SMULL` + 6 `SMLAL`, no
  `__aeabi_lmul`). This is a register-allocation problem, and Rust cannot
  express the allocation: **the kernels need hand-written ARM asm** (the
  milestone ULI2.md always placed after the hot path).
- **Entropy is ~6.6× the C reference per coefficient** (~272 c/coef against
  the C bench's measured ~41 for entropy+dequant). This is *not* explained by
  the obvious suspects — see “ruled out” below — so it needs a redesign of the
  bit-reader protocol, not tuning.

**Cheap levers, measured and ruled out** (each built and run on target):

| lever | result |
|---|---|
| fuse dequant into the coefficient loop, drop the `m[16]` scratch array | **neutral** (535,941 vs 533,605) — LLVM already handled the two-pass form |
| inline all of `huff` at the call sites | +2.7% (519,397) but **+4,252 B of IWRAM** — poor trade where U6 is binding |
| inline only the `huff` LUT fast path, keep the long search out of line | **+1.6% (523,267) for +896 B** — kept |
| `-Cforce-frame-pointers=no` | inert (identical cycles), matches the FLAC track's E7 |
| synthesis tables in IWRAM (`iwram-tables`) | **superseded** — recorded as *crashes* on the then-believed ~15,040 B stack. The stack is now measured at ~2,960 B total and `iwram-tables` runs and wins (148% -> see "Memory layout" in
[`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md)). Kept here only as a record of what was believed at the time. |

Gate: **≤1×** worst. **Not met by this PR** (523,267 = 200%); it is a
measurement, one small win, and the scope statement for the two rewrites
below. U4 stays open.

#### PR C-1 — asm kernels (the 63%)

Hand-written ARM kernels in `.iwram`, twinned bit-exactly against the Rust
reference (the existing `tests/twin.rs` and `tests/decoder_twin.rs` are the
witness). **Provenance decided 2026-11-14:** ARM-mode asm may be adapted from
the reference implementation; **C source may not be incorporated**, and there
must be **no build, link or vendoring dependency on `gba-audio`** — every
artefact this project uses lives in this repository. The asm is ours to write
and maintain (`global_asm!`), and it is twinned against our own oracle, not
theirs. Target the C record's own decomposition: **~134,600 cycles/frame for
the pair**, i.e. pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k per channel.
Prize: **~194,000 cycles/frame**. Its per-kernel rows are the useful part — our
FFT is 170,130 for the pair against 62,800, and our OLA is 76,932 against 30,600
— so FFT carries ~107,000 of the prize and OLA ~46,000.

#### PR C-2 — entropy rewrite (the 33%) — **landed as C-3a, 2026-11-14**

Redesign the bit-reader protocol. The 32-bit cached window from PR B is in
place, so this is not “more of the same”: candidate directions are holding the
reader state in registers across a whole band instead of behind `&mut`, a
byte-oriented refill path, and folding `huff`+`skip`+sign into one symbol
routine. Prize: ~90,000 cycles/frame.

**Landed as C-3a plus C-3c**, with the reader as a `Copy` local whose position
is derived from a stream cursor, packed `huff`+`skip`, the inline `clz6` escape
path, `q3`/`rq` tables, direct coefficient writes with no pre-zero pass, and
`nb`-bounded M/S. Measured: **−35,000 cycles/frame**, not the projected 90,000.
See “PR C-2 / C-3a” in the measurement ledger. The rest of this section is the
pre-measurement text and stays for context.

*Step 0b (2026-11-15) adds the caveat this paragraph implies but does not
say: the symbol-level packing landed, the **reader contract did not**. `dec_chan`
still takes `br: &mut BitReader`, so the struct's address escapes into
`huff_slow`/`eg_slow` and every refill stores the window back to memory; `skip`
still branches on validity, and the sign is still a `peek` plus a `skip`. The C
decoder's `REFILL`-once/branchless-`SKIP` protocol is the part of "the reader
in registers" that is still open — see "Step 0b".*

Together C-1 and C-2 project to ~239,000 cycles (~91%) once the C kernel row is
pinned at 134,600 (PR C recorded ~94% against the earlier ~146,000). Neither is
a tuning change, which is why PR C splits into them rather than closing here.

**They are not sufficient for the target, and ~91% is not C-class.** The gap to
the C/asm record is 523,267 → ~182,000 = **341,267 cycles**; C-1 (~194,000) +
C-2 (~90,000) account for **284,000**, leaving **~57,000 unidentified by the
plan**. A parity audit against the C product decoder
(`../../gba-audio/src/uli/gba/uli_gba_dec.iwram.c`, 287 lines) found every
remaining differential as a named item, listed as C-3 below. 94% would pass a
bare `≤100%` read of U4 and then fail the gate total (decode + convert + IRQ +
DMA): the C record's 69.4% decode wall sits inside a 73.63% gate total, i.e.
~4 points of headroom for everything that is not the decoder. **The plan
therefore targets parity, not the period.**

#### PR C-3 — differential port against the C product decoder (proposed 2026-11-14)

Reference: `../../gba-audio/src/uli/gba/uli_gba_dec.iwram.c` (the C product
decoder: 287 lines, ARM-in-IWRAM, bit-exact against its own reference) and
`../../gba-audio/src/uli/gba/uli_imdct_arm.S`. Reading it against
[`../../crates/uli-lite-gba/src/decoder.rs`](decoder.rs) and
[`lib.rs`](lib.rs) yields the items below; each is a differential to be removed,
not a redesign. Same discipline as everything else in this file: one arm, same
image, same corpus, one measurement each.

**C-3a — `dec_chan` structure (the content C-2 lacked).** C-2 named a prize but
not a design; the C decoder *is* the design, and every per-coefficient cost left
in our hot path has a named counterpart there.

| `uli-lite-gba` today | C reference | scale (frame 101) |
|---|---|---|
| `[s / 27, (s / 9) % 3, (s / 3) % 3, s % 3]` per quad | `uli_g_q3[81]` byte table: one load, shift by `6 - 2k` | 248 quads |
| clamp compare + `rq as i64` range dance per nonzero coefficient | `rq[16]` per-class table of `16m − dq`, `(m << 4) − dq15` for escapes, then one `*= step` | 616 nz |
| `m: [0i32; BANDW]` scratch written then re-read per band | direct writes into `X` | 512 extra ld/st per channel |
| pre-zero fill of all 512 coefficients, then zero bands again | `zero16()` = 4 × `STM`; EOB tail `STM`-filled; no memset at all | 512 stores per channel |
| `huff` returns a symbol, then `br.skip(len)`; `br.bad` checked per band | `huff` returns the LUT entry: symbol packed with its code length in the low 5 bits, already consumed | per symbol |
| `eg()` is `#[inline(never)]` | `esc()` with a `clz6[64]` fast path inline; out of line only for z ≥ 6 | 140 escapes |
| `&mut BitReader { stream, bitpos, .. }` across the whole path | local `BR { w, n, p }` copied in and out **once per channel** | everything |

Prize: the bulk of C-2's ~90,000, made auditable item by item. **C-2 should be
executed as C-3a rather than as an independent rewrite** — otherwise the same
work is specified twice.

**C-3b — stats-free product build.** The C product decoder keeps **no
counters**: no `nz`/`zc`/`pairs`/`quads`/`escs`/`raws`, no per-coefficient
`xover`/`hfnz` compares; its stage profiling is behind `ULI_GBA_PROFILE` and
compiled out of the shipping build. Ours are inside the timed loop. Put `Ops`
and `Diag` behind a `verify` feature (the twin and corpus tests keep them), so
the product arm measures the product. This also repairs the fairness of every
future `×%` comparison against the C record.

**C-3c — malformed-stream exactness off the valid path (approved 2026-11-14).**
The i64 range fallback in the dequant step exists to keep escaped magnitudes
exact on malformed streams; every *valid* coefficient pays for it. This project
controls its own encoded data and **promises nothing about the arithmetic of
malformed streams**, so the fallback is not a product requirement. Keep it in
the verifying build (it is still what makes the oracle bit-exact) and let the
product use the 32-bit `SMULL` path. Rejection-only behaviour stays: a corrupt
stream must set a sentinel and return, never panic, hang or corrupt state.

**C-3d — OLA output fusion, one format per arm.** C ships three kernels
(`uli_ola_s8_asm`, `uli_ola_s8w_asm`, `uli_ola_s16_asm`) with the rounding
`(acc + 2^39) >> 40` and the store fused in, and the format selected **outside**
the sample loop. Our `ola` branches on `Option<&mut [i16]>` per sample, and
`examples/uli_bench` times `Some(l16)` + `Some(r16)`, so the measured arm writes
**both** int8 and int16 for every sample — a configuration the C record never
timed. OLA is the 2.5× outlier (76,932 vs ~15,300), so this is a measurable part
of C-1, not decoration.

**C-3e — `nb`-limited M/S.** C's `dec_chan` returns the bands-before-EOB count
and `uli_gba_frame` runs `uli_ms_asm` over `nb * BANDW`, not all 512. We track
the same value and ignore it for M/S.

**Revised budget** (the ledger's worst frame, 523,267 → the C record's
~182,000). Cumulative, because the items overlap: the C-1 target row already
contains C's OLA, so C-3d only counts the part of our dual-output overhead that
C's row never had.

| after | frame | % of period | basis |
|---|---:|---:|---|
| PR C | 523,267 | 200% | measured |
| **+ C-3a/C-3c** | **488,685** | **186%** | **measured** — 35,000 saved, not 90,000 |
| C-1: kernels → C's 134,600 | ~295,000 | ~112% | C's per-kernel rows; re-based on the measured frame |
| C-3a: entropy/dequant → ~85,000 | ~239,000 | 91% | C-3a table; still ~2× C's 42–48k |
| + C-3b/C-3c (counters, i64 gating) | ~224,000–234,000 | 85–89% | not yet measured |
| + C-3d/C-3e (OLA single-format, `nb`-limited M/S) | ~202,000–222,000 | 77–85% | not yet measured |
| + C-1's stack cut → `iwram-tables` | **~193,000–213,000** | **73–81%** | 6.3% of kernels |
| C/asm reference | ~182,000 | 69.4% | gba-audio record |

The last two rows are the point of the audit: C-class parity needs the
**structural** items (C-3a, C-3d) and the **build-shape** items (C-3b, C-3c),
and it needs C-1 to be specified as *asm that also cuts the stack*, because
that is what buys the IWRAM table budget back (see U6). These projections are no
more trustworthy than PR C's levers were; they are ranges to be replaced by
measurements, one arm at a time.

**Read the range honestly — and it moved when C-3a was measured.** The rows
above were projections; the C-3a/C-3c row is now a measurement, and it delivered
**35,000 of the projected 90,000**. Re-based on that, the item list projects
**~250,000–280,000 (95%–107%)**, i.e. the period at best, **not the ±5% band
this file calls parity**. Two consequences are recorded rather than smoothed
over: (1) C-3b and the diagnostics are the largest *measured* lever left
(~11,000 for the stats alone, ~30,000 with the diagnostics) and are worth doing
next; (2) parity now needs something the list does not contain — a per-stage
attribution of the ~140,000 cycles entropy and dequant still cost (5.3× the C
record per coefficient, down from 6.6×), because the structural port did not
move that ratio nearly enough to close the gap. Do not let the table become a
forecast.

**Proposed order.** Each step is small enough to twin and measure on its own,
and each changes the frame the next one is measured against — so every arm keeps
its control in the same image (PR A's rule).

1. **C-3b (+ C-3c, landed 2026-11-14)** — the counters and format-rule
   diagnostics now sit behind `verify`; measured **−30,822 worst / −18,427 mean**
   (the attribution predicted −30,482 / ~20,000). Gate unchanged: `verify` is off
   for the product arm, on for the twins and for the arm that witnesses the
   product decoder's own `xover/hfnz`.
2. ~~**Coefficient buffers to IWRAM**~~ — **DONE 2026-11-15 (C-3): −87,040
   cycles/frame, 138% → 105%.** The block never existed (it was the stale stack
   figure) and the prize was ~6x the 10,000–15,000 projection, because it
   captures all coefficient traffic through `x`/`buf`/`p`, not just M/S. See the
   C-3 section below.
3. ~~**C-1** — asm kernels~~ — **one of four landed** (the FFT, v1→v4). The other
   three (`pre`, `post`, `ola`) are still Rust and are now the critical path:
   see **"Critical path to real time"** at the top of this file for the ordered
   list and the measured headroom. (This item's original justification — that
   `iwram-tables` crashes on a ~15,040 B stack — was disproven by measurement.)
4. ~~**C-3d**~~ — **DONE 2026-11-15**: OLA output fused per format; production
   OLA is 1.40x C and the int8-only frame dropped 148% → 138%.
5. ~~**C-2 = C-3a (landed 2026-11-14, −35,000 measured)**~~ — done.
6. **Re-measure and take the U4 verdict** against the **real-time budget**, not
   against a percentage borrowed from C, and not against the 129-frame corpus
   alone (PR E is the witness).

**The target is the real-time budget, not a percentage borrowed from C
(clarified 2026-11-15).** The C record's 69.4% is **not a benchmark to match** —
it is a *compute-budget existence proof*. C+asm runs this codec in real time on
this exact hardware (ARM7TDMI, 16.78 MHz, 262,144 cycles per 512-sample frame),
which settles the only question that matters: **the work fits the machine.** The
figure is therefore used for what it is — evidence, and a coarse sanity
reference for where a competent implementation lands — and *not* as a target to
be "within 5% of". Two consequences follow, and they replace the earlier
"C-class parity" framing:

- **The goal is U4/U5: real time with headroom**, i.e. the decode wall under the
  period with room for the gate total (convert + IRQ + DMA) and this tree's UI.
  The ceiling is written down from a measured near-real-time arm, not assigned.
- **"C-class" is retired as a gate.** There is no reason a Rust+asm stack should
  be slower than a C+asm one on the same hardware; if C proves the budget is
  reachable, Rust+asm is expected to reach it too. The comparison to C is kept
  only as a diagnostic for *where* time goes, never as an acceptance criterion.

The C record's 69.4% was taken on gba-audio's whole album (167,222 frames, worst
frame 27806) with its own toolchain and image, and absolute frame numbers here
are image-specific (the same kernel sequence measured 2,970,042 in one image and
2,676,986 in another), so no `×` against it is like-for-like until PR E. Its
per-kernel decomposition (`pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k` per
channel = 134,600/pair) remains useful as a **rough shape of a solved
implementation**, which is how this file now uses it.

### PR D — playback (after U4)

- Reuse the 32,768 Hz ring/FIFO feeder; int8 direct and the optional pre-DAC
  int16 path; UI from the ULA2 tables; `PcmSource` seam.
  - **That feeder is `crates/gba-playback` since 2026-11-15** (the application line lifted it
    out of `examples/flac_spike/`; item 1 of "Cross-track facts PR D's owner needs" below, and
    [`../../ALBUM-PLAYER.md`](../../ALBUM-PLAYER.md) §2.2). Link it as a path dependency at
    this phase's own `RING` const rather than `#[path]`-including the spike.
- **Two output regimes, one decoder (decided 2026-11-14).** The speaker profile
  is **in scope**, not an afterthought: the decoder always produces full stereo
  (two channels synthesised, stereo sample pairs out) and the mono collapse
  happens in the **output stage**, after the decoder — never inside it, and
  never by shortening synthesis to one channel. Both regimes are measured: the
  headphone path at the stereo decode wall, and the speaker path including its
  fold-down in the gate total.
- Fold-down method is a PR D call, to be measured there: `(L+R)/2` (the mono mix
  gba-audio's driver uses, ~13.65%/frame in their stage) or single-channel
  (`L` only). **They are not equivalent** — single-channel output is not a
  fold-down: hard-panned content vanishes, and on an M/S frame `L = M + S`
  reproduces mid+side rather than mid. Single-channel is cheaper and is
  acceptable for the speaker if the measurement supports it; record which one
  ships and what it costs.
- Gate: U5 (zero underruns, clean period audit) in mGBA, then hardware, with
  both regimes reported.

#### Landed 2026-11-16: the seam, the player, and both output profiles

Three pieces, each with an owner that does not know about the others:

* **`crates/uli-playback`** — the codec-agnostic `PcmSource` the player owns, the
  ULI2 adapter (`UliSource`) over `GbaDecoder` + the `ULA2` track table, the
  **output stage** (`fold_into`: headphone stereo, or speaker mono by
  `(L+R)>>1` / `L`-only), and `Player<const RING>` over the shipping
  `gba_playback::playback::RingFeed`. Host suite: **10 tests**, including every
  emitted frame pinned **one-for-one against the C reference decoder's hashes**
  (a stronger statement than an album total), the two folds' non-equivalence, the
  int8-vs-int16 rounding rule, and the ledger's served/underrun/tail split.
* **`examples/uli_player`** — the product player: one paced
  decode→fold→push loop, timer0 = one sample, timer1 = one 512-sample feeder
  period with its IRQ, DMA1/DMA2 on the FIFOs from a two-slot ping-pong, 16-frame
  (250 ms) ring, 8-frame prefill, drain before the final screen. Keys: **A**
  pause/resume, **L/R** previous/next track. The UI is text drawn from the
  container's strings and the seam's position — title, `n/N`, elapsed/remaining —
  redrawn about once a second, with the background re-`show`n before every
  commit (the FLAC ROM's one-shot-present finding).
* **Measurements below**, both profiles, zero underruns.

**The fold choice: `(L+R)>>1` ships.** `L`-only is implemented because the plan
required the comparison, and measured because a claim about it should not be
assumed: it is *cheaper* than the fold-down (−2,114 cycles/frame mean here) and
that cheapness is exactly its disqualification — it is not a fold-down (hard-panned
content vanishes; on an M/S frame `L = M + S` is mid+side, not mid). Recorded
costs are cross-image and therefore indicative, not paired:
`(L+R)>>1` ≈ **+6.3 k cycles/frame** over the stereo build (≈ 12 cycles/sample for
512 samples × load-pair/add/shift/store-both), `L`-only ≈ −2.1 k.

**U5 in mGBA: met for both profiles.** Zero underruns, the period audit closes
(`periods = served + tails`, one tail at the end of the album), and the ring ran
full (8,192 samples of lead) throughout. Explicitly **not** the hardware half of
U5, which still wants two units × two runs.

**The full-length album build (2026-11-16).** `make uli-player-cross` embeds E3-b's
twelve tracks — 43.5 min, 167,222 frames, 33,088,280 B — in one ROM: **33,407,264 B
of a 33,554,432 B cart, 147,168 B of headroom (0.44%)**. That is the image that
tests the cartridge's *real* capacity rather than its stated one, and its mGBA
run is the album-scale player number:

| build | worst (non-boundary) | mean | underruns | periods | boundary frames |
|---|---|---:|---:|---|---|
| full-length album, headphone | 245,671 (f11,531) = 93.72% | 205,763 = 78.50% | **0** | 167,232 / 167,232 served | 12, worst 343,152 |

**A gapless track advance costs two decodes in one window**, and the player now
reports it as its own number instead of folding it into the worst frame: the pull
that crosses a boundary also decodes the next track's preroll, so it costs ~1.3
periods (343,152 cycles here, 12 times on this album). The 250 ms ring absorbs it
— the ledger shows zero underruns and zero tails across 167,232 periods — and a
gapless join is the design, so the verdict judges the non-boundary worst and
prints the boundary figure separately. `skipped_samples = 2,769` is the sum of the
twelve tracks' non-frame-aligned `skip` fields, all accounted.

Three things this first increment does **not** do, stated so they are not read as
done:

1. **The UI redraw is outside the measured window.** The per-frame cost below is
   decode+fold+push; the ~1 Hz redraw sits between windows. It did not cause an
   underrun in either run (the ledger is clean), but its own cost is unmeasured.
2. **Multi-track seek is implemented and unexercised.** `UliSource` advances and
   seeks through the `ULA2` table and `UliPlayer` binds L/R to it, but the corpus
   is a single track, so a track change has never run. A two-track blob (the same
   master twice, Q15-style) is the missing fixture.
3. **Pause drains rather than stopping instantly.** Pause stops feeding and lets
   the 250 ms ring run out; the ledger classifies those periods as tails, not
   underruns, which is the correct accounting but not an instant silence.

#### Cross-track facts PR D's owner needs (reported by the application track, 2026-11-15)

Not codec rulings and not claims on U4 — three things decided above the seam that change
what PR D reuses. Authority: [`../../ALBUM-PLAYER.md`](../../ALBUM-PLAYER.md) §2.2, §2.3,
§4.3, §9.0.

1. **The feeder PR D says it will reuse has moved — it landed 2026-11-15** (this note read
   future tense when the application track wrote it; PR-1 of the album line is the event).
   `RingFeed` + its `checksum` pair are now **`crates/gba-playback`**, a path dependency that
   `examples/flac_spike`, `examples/flac_integration` and `examples/album_probe` all share and
   re-export, and `truncate()` is on the lifted type (§4.4, Q13b) — **measured, not derived**:
   the skip probe's truncate arm reads `SKIP_PRIME_CHUNKS + 1` periods (3/5/9/17 at prime
   2/4/8/16, `min == max`, `underflows = 0`) against the 39–53 periods drain and
   consume-and-discard still cost at the ring's 36-chunk ceiling. `flac-check` gained
   `playback-test` (24 tests, fails closed below the floor) beside `product-play-test`, and the
   shipped images' bytes are re-baselined in [`../../flac-docs/measurements.md`](../../flac-docs/measurements.md).
   **What this changes for PR D, and what it does not:** the ask is now a path dependency on
   the crate (`gba-playback = { path = "../../crates/gba-playback" }` from an `examples/`
   workspace, `../gba-playback` from a `crates/` one) instead of a `#[path]` of
   a spike directory, and it needs **no new mechanism** for the 32,768 Hz geometry — the ring
   is `RingFeed<const RING: usize>` and takes its chunk size from the caller's `take_chunk`
   destination, so a different rate and frame size are a different const parameter, not a fork
   (the product host model already instantiates arbitrary `R`). Nothing in `crates/uli-lite*`
   or `examples/uli_*` referenced the spike pair, so **no ULI work in flight needed
   refactoring**; `make uli-test` is green across the move, and no ULI image compiles these
   modules, so no ULI hash was re-baselined.
2. **"UI from the ULA2 tables" overlaps the application manifest (Q13).** The app rules its UI
   reads `ALBM1` — its own per-track map of ROM locations, album-sample boundaries, titles and
   the progress reciprocal (§4.3) — because the features A1/A6/A10/A11 must exist on both
   backends and `ULA2`'s tables are one backend's. The resolution keeps both statements true:
   the adapter reads `ULA2`'s tables to position the decoder and **cross-checks them against
   the manifest at bake time, refusing on mismatch**, so "driven from `ULA2`" and "driven from
   `ALBM1`" are the same sentence wherever they agree and a build failure where they do not.
   What still needs a PR D ruling is where the output stage stops and the player begins — that
   decides where U5's gate gets written.
3. **The non-audio addend in the ≤ ~85% target is partly measured, and it is the app's to
   hand across.** AP-0 (`examples/album_probe/README.md`) measured the basic UI at **5,306
   cycles per video frame** = 10,832 of the FLAC 573,440-cycle period (**1.89%** of wall
   time), feeder 0.92%. Because that cost is VBlank-paced rather than per audio period, the
   share travels as a fraction of *time*: against ULI2's 262,144-cycle period the same
   arithmetic reads ≈4,950 cycles/period (**≈1.9%**). That is a conversion, not a ULI2
   measurement — the app re-measures it on the `BACKEND=uli2` arm (AP-6/AP-9, Phase 5) before
   anyone spends it, and viz + game cost are still unmeasured targets.

Also for PR D's assets: the album line's policy is that **no album audio is committed** — the
track list is tracked and every blob is generated at build time and pinned (§2.3). For the
lossy backend that composes with Q3 (encoder used off-tree): the bake hands the adapter
*already-encoded asset bytes plus a tracklist*, decodes nothing, and performs the same
manifest/viz/cost work as the lossless path.

---

### PR E — the album-corpus tail and the gate total (landed 2026-11-15)

**Two deliverables, one corpus, two images: the tail of the product frame over a
16,553-frame album, and the gate total (decode + convert + IRQ + DMA) measured
in one window.** Method is [`../../flac-docs/measurements.md`](../../flac-docs/measurements.md)'s:
the tail is first-class (worst **with its frame index**, min, sum, mean), and
every absolute is image-specific.

#### The corpus decision: the tracked master, not the 43-minute album

The plan of record previously said "full-album corpus (~70 s, 167,222 frames)".
That was written before two facts existed, and both now decide against it:

1. **It does not fit.** `gba-audio`'s album is 33,280,736 B; this bench image is
   487,720 B with a 39,308 B corpus inside (448,412 B of non-album bytes). The
   sum is 33,729,148 B against a 32 MiB cart's 33,554,432 B — **174,716 B over**.
   A full-album arm would need the album trimmed, at which point it is no longer
   the reference corpus and its pins no longer apply.
2. **Density, not duration, is what a tail measures.** The reference album's
   measured worst frames were concentrated in two of twelve tracks, and its own
   encoder-model-ranked "heavy" frames were *not* the measured heaviest. Repeating
   one master (Q15's album-length build) cannot strengthen a tail at all.

So PR E's corpus is **`assets/sound/bioluminescent.wav`** — the tracked reference
master, 258.61 s, 96 kHz/24-bit — encoded at the **product configuration**:
`prep.py`'s fft resample to 32,768 Hz stereo, then the C encoder of record with
`--cap 74` and `--budget 3,294,000` (the reference album's 12,742 B/s scaled to
this master's 258.61 s). The result is **3,293,048 B = 101.686 kbit/s,
16,553 frames** — the product rate, one track, and it fits a 32 MiB cart with
the instrument at **3,741,460 B (11%)**.

The corpus is a **witness, never a gate input**: the blob, the f32 intermediate,
the C decode directory and the pin module are all gitignored
(`ALBUM-PLAYER.md` §2.3's division), generated by `make uli-album-asset` from the
tracked WAV. The pin is C-sourced: `scripts/uli_album_pin.py` takes the C
reference decoder's own `hash_of_frame_hashes` and re-verifies its published
per-frame checkpoints against the C decoder's 8-bit track output before it will
emit a pin (67 checkpoints matched, the preroll frame excluded by construction).

**The density screen ran before any ROM was built** (`make uli-frame-ops`, the
host symbol census). Against the 129-frame corpus's gate frame — 64 band symbols,
248 quads, 140 escapes, 616 nz — the album reaches **max 710 nz, 182 escapes, 257
Huffman symbols, 16 long codes, 64 band symbols**, with **M/S in 87.1% of frames**
(the fixture's gate frame has M/S off). It does not reach the quad-heavy mix
(max 228 quads against 248; pairs are this master's dominant group, max 392).
So the trigger set in advance is **met** in every dimension that drives the
entropy path and in the M/S class, and **partly unmet** in the quad class — which
is why a small synthetic adversarial set and the `gba-audio` album cross-check
stay on the books as PR E's E3, gated rather than dropped.

#### E1 — the tail (product build, decode only)

`make uli-album-bench` (`--features album-corpus`), one image, product decoder,
**int8-only** — the shipping path (C-3d). Pins: `int8=55e710af` PASS,
`xover=0 hfnz=0`.

| arm | frames | min (frame) | **worst (frame)** | mean | sum |
|---|---:|---:|---:|---:|---:|
| product `prod8`, album corpus | 16,553 | 144,725 (f15) | **220,062 (f16,204) = 83.94%** | **189,780 = 72.40%** | 3,141,376,572 |
| product `prod8`, 129-frame fixture, same instrument | 129 | 153,436 (f35) | 219,261 (f125) = 83.65% | 194,499 = 74.19% | 25,090,379 |

Three readings, in order of consequence:

1. **The 129-frame tail was not an underestimate.** 128× more frames move the
   worst frame by **+801 cycles (+0.3 points)** and the mean *down* by 4,709. The
   deferred worry — that 129 frames understate the tail — does not hold on this
   material: the fixture's synthetic clicks/noise are a harsher per-frame load
   than real music at product settings.
2. **The fixture's absolute moved +10 cycles/frame** (219,251 → 219,261,
   194,489 → 194,499) and that is the recompile offset the FLAC ledger warns
   about, not a regression: the arithmetic that changed is the counter's
   zeroing cadence, the paired deltas are unaffected, and every number above is
   from its own image.
3. **The cycle-worst frame is not the densest frame.** f16,204 carries 63 band
   symbols / 212 quads / 97 escapes / 570 nz / 4 long codes **and M/S**; the
   bit- and symbol-maxima (f10,908: 3,744 bits, 249 symbols, 710 nz; f13,930: 257
   symbols) are *not* the cycle worst. The plan's standing warning about proxies
   holds inside a single real album, not only across the corpus.

**An instrument bug the long corpus exposed, now fixed and recorded.** The cycle
counter (timer2/timer3) is 32 bits at 16.78 MHz, so it wraps every **256 s of
emulated time** — and one album pass is ~217 s (product) to ~1,280 s (portable
core). Before this, only the *between-pass* case was guarded: the first album run
reported windows of 4,294,967,260 cycles in 4,546 frames, and `wrapped` stayed
false because a stale 16-bit high word reads as a *valid* difference. Fixes: the
counter is zeroed every **1,024 windows** (no window can span a wrap — a window
never crosses a zeroing), and any window above `SUSPECT_CYCLES` (2²⁴, a
counter-integrity bound, not a speed one, because the control arms are
legitimately several periods per frame) is counted, printed and **fails the
verdict**. The 129-frame corpus never reached either limit, which is why it took
a 16,553-frame corpus to find.

#### E2 — the gate total (decode + convert + IRQ + DMA)

`make uli-gate` (`--features gate,album-corpus`). Same decoder, same frames,
int8-only, with the *shipping* `gba_playback::RingFeed` and the real timebase:
timer0 = one sample (512 ticks), timer1 cascaded = one feeder period **= one ULI2
frame** (64 periods/s) with its overflow as the feeder interrupt, DMA1→FIFO A and
DMA2→FIFO B in repeat mode from a two-slot ping-pong, `SOUNDCNT_H` 100/100,
`SOUNDBIAS` 8-bit. The measured window is **decode → convert → push with
interrupts live**, so the feeder IRQ and the FIFO DMA are inside it; the
producer's *wait for ring space* is outside (a real player is clock-paced, and
charging the wait would measure the hardware clock).

| arm (one image) | worst (frame) | mean | percent (worst / mean) |
|---|---:|---:|---:|
| `prod8` — decode only | 220,102 (f16,204) | 189,817 | 83.97% / 72.41% |
| **`gate` — decode + convert + push + IRQ + DMA (player model)** | **251,712 (f16,204)** | **221,245** | **96.02% / 84.40%** |
| gate overhead (paired, same frame) | **+31,610** | **+31,428** | +12.06 / +11.99 points |
| `gate-hash` — the same arm with in-loop verification | 257,759 (f16,204) | 226,320 | 98.33% / 86.33% |
| verification cost (paired) | +6,047 | +5,075 | +2.31 / +1.94 points **plus 12.06 s of starvation** |

- **ULI2's "convert" is a byte copy, not the FLAC path's PCM16 `>> 8`** — the
  fused int8 path (C-3d) is what makes the output stage this cheap. The
  reference project's own output stage is 3.02% *plus* a driver conversion; this
  is the same job after ULI2 stopped paying for int16.
- **Feeder ledger, player model: `pushed = consumed = 8,475,136`, ring = 0,
  `serviced = full_takes = 16,552`, `starved_entries = 0`.** The pass ran for
  16,552 periods = **258.63 s, exactly the album's length**, and ended with the
  queue drained rather than cut. That equality is the check worth keeping: a
  paced pass whose runtime is not the audio's is either cutting its tail (no
  drain) or losing periods (starvation), and the first version of this arm did
  both.
- **The 772 starved entries are solved, and they were the harness.** The first
  version hashed every decoded frame *outside* the window. In a paced pass that
  work is not free: ~12,200 cycles/frame (4.7% of the period) made the producer
  slower than the audio clock, emptied the ring and starved **772 periods =
  12.06 s** of silence across the album, while the ring refilled later so the
  ledger still closed. `gate-hash` keeps that shape as a labelled attribution
  arm; the default is the **player model**, which hashes nothing and whose
  runtime is exactly the album. The hash also moves the windows (+5,075
  cycles/frame): the traffic shares the bus and shifts where the feeder IRQ lands
  inside the window, so the harness shows up twice — once in the windows, once in
  the pacing.
- **A drain was missing and is now in.** The producer runs ~14% ahead, so it
  finishes with the ring full; tearing the chain down there cut the last ~250 ms.
  The pass waits for `ready() == 0` before stopping, which is what a player does.
- **0 frames' measured window exceeded one period** (worst 96.02%). The window
  that would have is the mean + a burst, which is the player's problem.

#### The U4 ceiling, written down

U4's ceiling is **the period itself (100%)**: the chip is spending this to decode
and produce a frame; the UI is the only remaining consumer, and IWRAM priority
stays audio-first. On the album corpus, in mGBA, at WAITCNT 4/2, no UI:

- **decode wall: 83.94% worst / 72.40% mean** — the ≤85% headroom target is met
  with 2,783 cycles of margin at the worst frame;
- **gate total: 96.02% worst / 84.40% mean** — inside the period with **10,432
  cycles (3.98 points) of margin at the worst frame** and 15.6 points at the mean.

Read the margin honestly: **4 points at the worst frame is not much**, and this arm
pays no UI, no art, no manifest walk, mGBA does not model the cart prefetch
buffer or DMA/code-fetch arbitration (the FLAC ledger's R3), and the producer's
pacing wait hides at most one IRQ per frame from the window. Those are the reasons
the number is a **ceiling written down**, not a playback claim: U5 stays open, and
PR D is where the burst behaviour and the real underrun ledger live.

#### The audio-only ROM, and a manual hardware pass (2026-11-16)

The instrument is a witness, and a witness that spends 90 minutes decoding the
same album five extra ways is not something to flash to answer "does it play".
`make uli-play-rom` builds the **audio-only** image from the same arm:
`--features play-only` runs **one paced pass** — decode → int8 → ring → DMA1/DMA2
on the FIFOs at 32,768 Hz — with no pins pass, no control arms, no attribution and
no per-frame hashing, so its runtime is the album's own length.

**Result, on real hardware, manual listen, one pass:** the album played through its
entirety with **no glitches and no silent gaps**. That is the observation the
hardware run contributes; the screen colour was not recorded, so the numeric
verdict above is the **headless** run of the same image (`starved=0`,
`pushed = consumed`, ring drained, 258.63 s) and the two must not be conflated.
Recorded as the first silicon witness of **ULI2's** chain at 32,768 Hz: timer0
sample clock, timer1 feeder period and IRQ, both FIFOs, the ring, and the fused
int8 output.

Its own numbers, with the image-specific caveat stated up front:

| image | worst (f16,204) | mean | runtime |
|---|---:|---:|---:|
| `uli-play` (audio-only, no instrument code) | 226,542 = 86.43% | 196,073 = 74.80% | 258.63 s |
| `uli-gate` (the same arm inside the instrument) | 251,712 = 96.02% | 221,245 = 84.40% | 258.63 s |

The 9.6-point spread is consistent with the instrument's own presence — code
layout, the passes that run before it, its counters — rather than with the
decoder, but no mechanism has been isolated and this file does not claim one. By
its own rule absolutes are image-specific and pairing is within an image, so
**96.02% stays the gate total of record** (deliberately the pessimistic of the two) and 86.4% is
recorded as what a shipping-shaped image compiles to.

**What this does not establish.** It is not U5: one unit, one run, no counted
underrun ledger, no period audit, no UI, and the judgement is by ear. It removes
the specific doubt PR E could not answer — whether the measured chain actually
produces clean audio on silicon at 32,768 Hz — and leaves U5 where it was.

#### E3 — the two pre-hardware corpora (landed 2026-11-16)

Both are generated, gitignored assets with C-sourced pins, and both are now
measured on the same arms as PR E.

**E3-a, the adversarial density set** (`make uli-adversarial-asset`,
`scripts/gen_adversarial_corpus.py`). Ten seconds of synthetic content chosen to
invite the **quad-heavy** class the real master does not produce — 4-sample-block
noise, impulse trains at four rates, hard-clipped harmonic-rich tones — at product
settings (127,432 B, 641 frames). Its census covers the fixture's gate-frame class
outright: **max 252 quads** (fixture 248; album 228), 197 escapes, 721 nonzero
coefficients, 264 Huffman symbols, 64 band symbols. Measured:

| arm | worst (frame) | mean | % of 262,144 |
|---|---|---:|---:|
| decode wall (`prod8`) | 213,783 (f64) | 184,896 | 81.55 / 70.54 |
| gate total | 251,431 (f64) | 216,753 | 95.91 / 82.68 |

**The class is not the worst class.** Quad codes carry four coefficients per
Huffman symbol, so a quad-dense frame is *cheaper* per frame than the album's
mixed pair+quad+M/S frames: the adversarial arm's worst is **281 cycles below**
the album's gate total (251,431 against 251,712) and its mean is 2% lower. That
is the bound E3-a owed: the corpus that produces the missing class does not beat
the master's tail, so PR E's worst frame stands.

**E3-b, the sibling project's 12-track album** (`make uli-cross-asset`,
`scripts/uli_cross_album.py`). The repertoire the reference project measured on:
twelve CC-licensed tracks fetched from the Internet Archive, trimmed and faded by
the sibling's own documented recipe, resampled to the codec rate, and encoded at
**101.277 kbit/s** — **167,222 frames**, the same count as their album, in
33,088,280 B (the budget is 33,100,000, *not* their 33,292,288, which is 174,716 B
over a 32 MiB cart beside this instrument). C-sourced pin `51fcf846`, 103
checkpoints verified. Measured:

| arm | worst (frame) | mean | % of 262,144 |
|---|---|---:|---:|
| decode wall (`prod8`) | 220,864 (f11,552) | 185,749 | 84.25 / 70.86 |
| gate total | **252,490 (f11,552)** | 217,119 | **96.32 / 82.83** |

- **The album-scale corpus has a marginally worse tail than the single master**:
  96.32% against 96.02% (+782 cycles, +0.30 points), with 3.68 points of margin.
  That is the first time a corpus has produced a worse frame than PR E's, and it
  is the reason E3 existed.
- Frame 11,552 of this bitstream, not the sibling's 27,806: different encode
  (different budget, different processing), so the indices are not comparable and
  are not compared.
- Against the sibling's own C/asm record on the same repertoire (decode 69.43%
  worst, gate total 73.63%), our decode wall is **1.21x** — the ratio this file has
  carried since Step 0 — and the distance is the Rust/C gap plus the output stage.
- The feeder closed the ledger exactly: 167,221 periods, 167,221 full takes, **0
  starved**, ring drained.

**What E3 does not settle.** The pins are our encoder's, not the sibling's
bitstream: reproducing *their* blob would need their exact budget and processing,
and it is 174,716 B too large for this cart anyway. So E3-b is repertoire-and-scale
equivalence, not bitstream equivalence — which is what the hardware claim needs
(the tail over a real 43-minute album), and it is recorded as that.

#### What PR E leaves open (E3, gated)

1. **The quad-heavy class** the album does not produce (max 228 quads vs the
   fixture's 248, 0 pairs there): a small synthetic adversarial set at product
   settings, plus the `gba-audio` 12-track album as the album-scale cross-check
   that re-validates its 104/64/12 pins. Both are required *before hardware*, and
   neither is needed to write the ceiling down above.
2. **The handler's double-dispatch question** (772 starved entries with a full
   ring at the end) — PR D's, because the player owns that chain.
3. **A real album-length run** (zero underruns, period audit, UI linked) — U5 and
   PR D, not U4.

---

## Measurement ledger

### Root-cause audit (2026-11-16, mGBA, WAITCNT 4/2)

The audit's two landed items, each its own image, 129-frame corpus
(`uli2_tiny.uli`), product feature set (`asm-fft,asm-pre,asm-ola,asm-ms,
iwram-tables`). Every arm: both album pins PASS, `xover=0 hfnz=0`, and the
on-target kernel twin PASS with `fftasm=true preasm=true olaasm=true
msasm=true`. **Two runs per image, identical to the cycle.**

| arm | worst (frame) | mean | sum/129 | of period | vs C |
|---|---:|---:|---:|---|---:|
| HEAD before the audit | 219,251 (f125) | 194,489 | 25,089,116 | 83.64% / 74.19% | 1.205x |
| **A** FFT multiply operands (twiddle→Rn, data→Rs) | 214,691 (f83) | 187,496 | 24,186,967 | 81.90% / 71.52% | 1.180x |
| **B** `asm/ms_arm.s` M/S block transfer | **210,355 (f83)** | **185,207** | **23,891,692** | **80.24% / 70.65%** | **1.156x** |

| step | Δ worst | Δ mean | Δ sum |
|---|---:|---:|---:|
| A — FFT operands | −4,560 | **−6,993** | −902,149 |
| B — M/S kernel | −4,336 | **−2,289** | −295,275 |
| **both** | **−8,896 (−4.06%)** | **−9,282 (−4.77%)** | **−1,197,424** |

**Disassembly facts** (the audit's evidence; image- and toolchain-specific):

- **The isolated kernel arm confirms A independently**: `asmfft5 n=10` moved
  **57,094 → 51,064 cycles per call** in `uli-kernel-twin.log` (the arm times the
  kernel on the bench's EWRAM `KX`/`KZ` buffers, so it is not the same data as the
  product frame and its delta is larger than the frame's — read the direction, not
  the magnitude).
- `uli_fft_arm_v5` before A: 62 instructions per general radix-4 butterfly
  against C's 61, with all six `smull`/`smlal` carrying the **twiddle in Rs**.
  `TW` entries are ~2^30, so every multiply ran at the multiplier's slow end.
  `pre_arm.s` and `ola_arm.s` already had data in Rs; `post` got it from LLVM by
  accident of source operand order.
- `dec_chan` (product, ARM, `.iwram`): 1,394 instructions, **259 stack/frame
  references (18.6%)**, prologue `push {r4-r9, sl, fp, lr}` + `add fp, sp, #28`.
  `zero16` compiles to `bl __aeabi_memclr4` at 3 sites. `BitReader::load` is two
  `ldrb` + `orr`/`lsl` per refill. Escape fast path is 12+ instructions with two
  compares C does not have.
- M/S before B: 9 instructions per coefficient (2 `ldr` + `add` + `sub` + 2
  `str` + index + branch); C's kernel is 14 per **four**.
- `.iwram` after B: **27,308 B** (`uli_ms_arm` is 68 B; the Rust `ms` control left
  `.iwram` when the kernel became the product path). Stack high-water 844 B.
- `rustc --print target-spec-json --target thumbv4t-none-eabi` reports
  `"frame-pointer": "always"`, which is why `-Cforce-frame-pointers=no` is inert
  (FLAC E7, PR C) and why r11 is unavailable in every Rust function.

**Notes on reading these rows.** The by-difference per-kernel table in "Step 0"
was **not** re-run for this ledger and its M/S and `pre` rows are now known to be
biased (see the audit's method note): removing an arm that changes `X` also
changes the synthesis kernels' cost, which is data-dependent through `Rs`.
Frame deltas of real changes are quoted above; per-kernel rows stay locators.

### `post` as ARM asm (2026-11-18, mGBA, WAITCNT 4/2)

`post` was the **last Rust kernel on the product path** and the one kernel whose
excess is nameable in instructions rather than density: **18 per iteration
against the reference's 14**, all four extra being addressing (the `z` pair via
`add`+`ldr` instead of one `ldmia`; the twiddles via two `ldr` instead of one
`ldmia`; the backward `Y` store via a computed address instead of a
post-indexed cursor). `asm/post_arm.s` is the reference's 14-instruction loop.

| arm | worst (frame) | mean | of period |
|---|---:|---:|---|
| 129-frame, before | 208,854 (f83) | 182,119 | 79.67% / 69.47% |
| 129-frame, **asm `post`** | **205,793 (f83)** | **179,058** | **78.50% / 68.31%** |
| album, before | 209,162 (f16,204) | 178,341 | 79.79% / 68.03% |
| album, **asm `post`** | **206,101 (f16,204)** | **175,280** | **78.62% / 66.86%** |
| **gate total**, before | 240,705 | 209,721 | 91.82% / 80.00% |
| **gate total**, **asm `post`** | **237,646** | **206,637** | **90.65% / 78.83%** |
| `cross` (167,222), before | 210,209 (f27,693) | 174,009 | 80.19% / 66.38% |
| `cross`, **asm `post`** | **207,148 (f27,693)** | **170,939** | **79.02% / 65.21%** |

**−3,061 cycles on *every* frame, worst and mean alike.** That uniformity is the
win's signature: `post` runs a fixed 256 iterations per channel regardless of
content, so removing 4 of 18 instructions is a *content-independent* saving —
unlike the multiply-bound kernels, whose cost tracks the data. Gate margin
**8.18 → 9.35 points**; ratio to C on the matched corpus **1.155x → 1.139x worst,
1.137x → 1.117x mean**.

**Witnesses:** both album pins PASS on every corpus (`e6e87652`/`b3530978`
fixture, `55e710af` album, `51fcf846` cross), `xover=0 hfnz=0`, the on-target
kernel twin PASSes with **`postasm=true`** (`make uli-kernel-twin`),
`make uli-test` and `make uli-overlay-test` green,
`gate feeder ... starved_entries=0 frames_over_period=0`, and two runs identical
to the cycle.

**Multiply operand order is part of the contract**, and the kernel header says so:
`smull r11,r4,r8,r6` puts the *twiddle* in `Rn` and the **data in `Rs`**, which is
the only ordering the ARM7TDMI early-terminates on. The inversion of that cost
the FFT 3.6% of the whole frame until the 2026-11-16 audit.

### Target spec: `frame-pointer: may-omit` (2026-11-17, mGBA, WAITCNT 4/2)

The build target is now a checked-in spec, `targets/thumbv4t-gba.json` — the
pinned toolchain's built-in `thumbv4t-none-eabi` spec with **one** field changed,
`frame-pointer: "always"` → `"may-omit"` — and `.cargo/config.toml` builds for it.
Control and variant are the same tree with the same compiler; only the spec
differs.

| arm | worst (frame) | mean | of period |
|---|---:|---:|---|
| 129-frame, built-in `always` | 210,833 (f83) | 183,241 | 80.43% / 69.90% |
| 129-frame, `may-omit` | **208,854 (f83)** | **182,119** | **79.67% / 69.47%** |
| album, built-in | 210,787 (f16,204) | 179,226 | 80.41% / 68.37% |
| album, `may-omit` | **209,162 (f16,204)** | **178,341** | **79.79% / 68.03%** |
| gate total, built-in | 242,370 | 210,626 | 92.46% / 80.35% |
| **gate total, `may-omit`** | **240,705** | **209,721** | **91.82% / 80.00%** |

`gate feeder ... starved_entries=0 pushed=8475136 consumed=8475136
frames_over_period=0 worst_over=0`; margin at the worst gate frame **7.54 → 8.18
points (21,439 cycles)**.

Code shape: **13 → 0** of 210 functions set up a frame; `.iwram` code 27,308 →
**26,412 B** (−896 B); `dec_chan` 1,258 → 1,250 instructions, stack/frame
references 165 → 155, `fp` uses 40 → 59 (r11 becomes data). Stack high-water
844 → 932 B.

**Witnesses:** both album pins PASS (`e6e87652`/`b3530978` fixture, `55e710af`
album), `xover=0 hfnz=0`, `msasm=true` twin PASS, `make uli-test` /
`uli-overlay-test` / `uli-kernel-twin` green, two runs identical to the cycle.

**Two negative results, recorded so they are not repeated:**

- `-Cforce-frame-pointers=no` is **inert**: rebuilt from a clean `target/` both
  ways, the ROMs are **byte-identical** (`579fe24d…`). Cargo does not invalidate
  on config-content changes, so a naive rebuild of the variant is a no-op that
  looks like a null result.
- `frame-pointer: "non-leaf"` — the plausible middle setting — would have helped
  **nothing**, because `dec_chan` is non-leaf (ten `bl` sites to
  `huff_slow`/`eg_slow`). Only `may-omit` frees r11 in the hot function.

**Known cross-track consequence (not a regression):** `flac-spike.gba`'s PR 3
verdict goes from `measured 2/2` to `measured 0/2` because its timing-window
calibration requires five *identical* empty-window samples and now reads
`raw = 20,22,22,22,22` instead of `22,22,22,22,22`. Decode is unchanged
(`pcm fnv ... [MATCH]`, `decode proof PASSED`); the rejected spread is 2 cycles of
net overhead. Not gated by any `make` target (`spike-rom` only builds), and
`make flac-test` passes. Re-baselining that instrument belongs to the FLAC track —
see `targets/README.md`.

### Loose-ends sweep (2026-11-16, mGBA, WAITCNT 4/2)

Audit items D and E: the single `ldrh` behind a new `uli_lite::Aligned`
alignment contract, and `zero16` as four inline `stmia`. Every arm: both album
pins PASS (129-frame) / the album pin PASS (album corpus), `xover=0 hfnz=0`,
`msasm=true` twin PASS, **two runs identical to the cycle**.

**129-frame iteration corpus** (sums over 129):

| arm | worst (frame) | mean | sum | of period |
|---|---:|---:|---:|---|
| HEAD (after #152) | 210,355 (f83) | 185,207 | 23,891,692 | 80.24% / 70.65% |
| + `ldrh` | 211,175 (f83) | 184,451 | 23,794,249 | 80.55% / 70.36% |
| + `zero16` asm | **210,833 (f83)** | **183,242** | **23,638,173** | **80.43% / 69.90%** |
| delta | **+478** | **−1,965 (−0.75 pts)** | −253,519 | |

**Album corpus** (16,553 frames, `assets/biomaster.uli`) — the tail instrument:

| arm | worst (frame) | mean | of period |
|---|---:|---:|---|
| HEAD (after #152) | 211,050 (f13,698) | 180,921 | 80.51% / 69.02% |
| **sweep** | **210,787 (f16,204)** | **179,226** | **80.41% / 68.37%** |
| delta | −263 | **−1,695 (−0.65 pts)** | |

**Gate total** (decode + convert + push + IRQ + DMA, shipping `RingFeed`, real
32,768 Hz timebase, player model — 16,545 timed windows, 16,552 periods):

| arm | worst | mean | margin at worst |
|---|---:|---:|---|
| PR E (2026-11-15, pre-#152) | 251,712 = 96.02% | 221,245 = 84.40% | 3.98 pts |
| **now** | **242,370 = 92.46%** | **210,626 = 80.35%** | **7.54 pts (19,774 cyc)** |

`gate feeder ... starved_entries=0 pushed=8475136 consumed=8475136
frames_over_period=0`. This is the re-measurement the audit's item 9 required.

**Code-shape facts** (image-specific):

- `dec_chan` 1,394 → 1,240 instructions, stack/frame references 259 → 174.
- `__aeabi_memclr4` is gone from `dec_chan` (13 `stm` in its place).
- `uli_ms_arm` 68 B; `.iwram` 27,3xx B with an 844 B measured stack.

**Read the worst-frame row with care.** On the 129-frame corpus the worst frame
(f83) is **+478 cycles worse** while every frame in aggregate is 1,965 better;
on the album the worst frame is −263 and the mean −1,695. The 129-frame delta is a
layout/register-allocation artifact on one quad-heavy frame, and it is exactly the
±3% warning this file already carries — the album corpus is the instrument that
decides, and both of its rows improve.

### Phase 1b baseline (2026-11-13, mGBA, WAITCNT 4/2)

Worst / mean frame, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean |
|---|---:|---:|
| portable core, Thumb-1, ROM | 6,622,160 (2526%) | 5,897,421 (2250%) |
| core kernels ARM-mode, ROM | 2,970,042 (1133%) | 2,245,992 (857%) |
| overlay kernels ARM/IWRAM (projection) | 2,122,748 (810%) | 1,398,699 (534%) |
| C/asm reference (`gba-audio`) | ~182,000 (69.4%) | ~153,000 (58.4%) |

Kernel sequence per channel: core Thumb/ROM 2,513,421; core ARM/ROM 667,263;
overlay ARM/IWRAM 243,617; overlay + IWRAM tables 228,313; C/asm ~67,300.
The projection is now replaced by PR A's measured full frame.

### PR A (2026-11-14)

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,676,986 (1021%) | 2,171,620 (828%) | 1,372,892 |
| overlay kernels (`ArmKernels`) | 1,829,247 (698%) | 1,322,800 (505%) | 523,102 |

Both arms: pins `e6e87652` / `b3530978`, `xover=0 hfnz=0`, no wrapped window;
calibration overhead 38 cycles (spread 3). Kernel sequence per channel:
core 667,264; overlay 243,619. Core full-frame rest (non-synthesis):
837,092 cycles; the overlay frame equals that plus 2 × 243,619 within 0.02%.
The on-target overlay twin passed for all four kernels.

Recorded consequence: PR A is a **measured** full-frame product kernel path at
~7x the period, down from the ~11x core arm in the same image and the 25x
Phase 1b starting point. It is not yet a product decoder: the bit reader,
Huffman, dequant and M/S are still the portable Thumb/ROM code, and they are
the 837,092-cycle rest that PR B attacks.

### PR B (2026-11-14)

Full frame, mGBA, WAITCNT 4/2, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path (`GbaDecoder`)** | **533,605 (204%)** | **463,660 (177%)** | 351,556 |

All arms pinned, `xover=0 hfnz=0`, on-target twin passed. Overlay
seq/channel 169,061; **`rest` = 195,484 cycles/frame (from 837,092 at PR A),
so synthesis is back to 63% of the frame.** The PR B gate (≤2×) is met at
204%. The remaining gap to the C/asm reference is now the kernels: 169,061 vs
~67,300 per channel (2.51×), with OLA the outlier.

Placement: `HotState` (8 KiB) is in EWRAM, not IWRAM — an IWRAM placement
stack-overflowed on target (≈15.0 KiB of stack needed; see PR B above). The
host twin was green either way, so this was a target placement defect only.

### PR C-3b (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period, same-environment baseline.

| arm | worst | mean | min |
|---|---:|---:|---:|
| C-3a + C-3c (before) | 488,685 (186%) | 429,339 (164%) | 353,340 |
| **C-3b product build** (`verify` off) | **457,863 (175%)** | **410,912 (157%)** | 352,940 |
| C-3b `verify` build (counters on) | 488,682 (186%) | 429,336 (164%) | 353,337 |
| core kernels (control, same image) | 2,643,897 (1009%) | 2,152,958 (821%) | 1,376,966 |
| overlay kernels (`ArmKernels`) | 1,640,505 (626%) | 1,148,486 (438%) | 371,523 |

**−30,822 worst / −18,427 mean**, against a prediction of −30,482 / ~20,000. The
`verify` build reproduces the previous arm to within 3 cycles (488,682 vs
488,685), which is the layout noise this file records elsewhere — so the feature
is a switch, not a re-implementation. Both builds pin `e6e87652` / `b3530978`,
`xover=0 hfnz=0`, no wrapped window, overlay twin PASS.

Design: `uli-lite-gba`'s `verify` feature gates the per-symbol counters (a
no-op call per increment otherwise) and the two diagnostic compares per nonzero
coefficient; `Diag` is empty and `FrameInfo::ops/cycles/xover/hfnz` are zeroed in
the product build. The format rules are **not** unwitnessed: `examples/uli_bench`
reads `xover/hfnz` from the core decoder's correctness pass, and
`tests/product_twin.rs` compares full `FrameInfo` under `--features verify` plus
a samples/bits/bitpos twin without it; `make uli-overlay-test` runs both. A guard
test fails if anything re-enables the counters outside the gate.

### PR C-2 / C-3a (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period. The baseline was rebuilt
from `main` in this environment immediately before the change (its numbers
reproduce PR C's record exactly, which is what makes the arms comparable).

| arm | worst | mean | min |
|---|---:|---:|---:|
| baseline (PR C, `main`) | 523,267 (200%) | 456,187 (174%) | 351,446 |
| **C-3a + C-3c** | **488,685 (186%)** | **429,339 (164%)** | 353,340 |
| core kernels (control, same image) | 2,643,897 (1009%) | 2,152,958 (821%) | 1,376,966 |
| overlay kernels (`ArmKernels`) | 1,640,505 (626%) | 1,148,486 (438%) | 371,523 |

All arms pinned `e6e87652` / `b3530978`, `xover=0 hfnz=0`, no wrapped window,
overlay twin PASS; calibration overhead 35 cycles, spread 0. The worst frame
moved 101 → 106, so the **mean is the comparable number** (−5.9%); worst is
−6.6%. The controls moved too (1009% vs PR C's 1023%), which is the recorded
image-specificity effect, not a kernel change.

**What landed**: reader as a `Copy` local over a stream cursor with the bit
position *derived* rather than maintained; `huff` returning the packed
`(sym << 5) | len` already consumed; the inline `clz6` escape path; `q3` and
`clz6` generated in-repo; `rq`/`dq15` precomputed at album parse; coefficients
written straight into `x` with no pre-zero pass and an EOB tail fill; M/S
bounded by `nb`; and C-3c's 32-bit dequant (approved: no arithmetic promise on
malformed streams). Host twins, the in-file bit-reader fuzz (4,000 random
windows × 64 steps) and the on-target pins are all green.

**Measured decomposition of what is left** — each arm built and run on target,
same image, varying only the counters and diagnostics:

| variant | worst | mean | vs landed |
|---|---:|---:|---:|
| C-3a + C-3c (landed) | 488,685 | 429,339 | — |
| …counters hoisted into locals | 509,888 | 439,098 | **worse, +21,000** |
| …`Ops` counters removed (C-3b) | 477,370 | 422,173 | −11,315 |
| …`Ops` and `Diag` removed | 458,203 | 411,253 | −30,482 |

Two findings worth keeping. **Hoisting the counters into locals costs 21,000
cycles** — register pressure displaces the reader, the same failure mode that
made PR C's dequant fusion neutral; recorded so it is not attempted again. And
**C-3b is worth ~11,000 alone, ~30,000 with the diagnostics** — the largest
measured lever left, and still open because gating it behind `verify` changes
the twin's comparison basis and the bench's on-target `xover/hfnz` witness.

Entropy + dequant is now ~140,000 cycles/frame against the C record's ~45,000
(≈5.3× per coefficient, down from 6.6×). **The structural port did not close
that ratio**, so the next entropy step has to be a per-stage attribution at
sub-frame granularity (PR C's by-difference method one level down), not another
structural guess.

### PR C-2 attribution: where the remaining ~140,000 cycles go (2026-11-14)

Two instruments, both run rather than reasoned about:

1. **Per-frame dump.** `examples/uli_bench --features per-frame` prints one row
   per frame with the measured window and the decoder's operation counts
   (`make uli-bench` is unchanged; the feature is off by default). The dump
   build reproduces the gate arm exactly — worst 488,685, sum 55,384,726 — so
   the instrument does not perturb what it measures.
2. **By-difference arms.** Each rebuild removes exactly one part and re-runs the
   bench. These are **timing-only**: the pins fail by construction (their output
   is no longer the album), and only the window is quoted.

Fitting the 129 dumped rows with a per-event model

    cycles = c + Σ (events × cycles_per_event)

explains the corpus at **R² = 0.9994** with a residual sd of **1,170 cycles**.
Per-event costs (cycles, ± the fit's standard error): **pair symbol ≈ 302 ± 19**,
**quad symbol ≈ 341 ± 23**, **escape ≈ 134 ± 9**, **raw field ≈ 57 ± 4**,
**long code ≈ 221 ± 16**, **M/S frame ≈ 18,124 ± 440**, nonzero-over-zero
coefficient ≈ +25 ± 9. The band-symbol term is **not identifiable** and must not
be quoted: `coded = pairs/8 + quads/4` exactly (`nz + zc = 16 × coded`), so the
band term is a collinear proxy — it fits negative, which is the tell that it is
absorbing something else, not that zero bands are free.

By-difference arms against the landed arm (worst / mean):

| part removed | worst | mean | Δ worst | Δ mean |
|---|---:|---:|---:|---:|
| — (landed C-3a + C-3c) | 488,685 | 429,339 | — | — |
| M/S loop | 472,618 | 419,852 | 16,067 | 9,487 |
| dequant arithmetic | 465,655 | 414,434 | 23,030 | 14,905 |
| zero-band + EOB fills | 486,874 | 421,937 | 1,811 | 7,402 |
| `Ops` + `Diag` (measured earlier) | 458,203 | 411,253 | 30,482 | ~20,000 |

**What that says, and the harvest order it implies:**

1. **The diagnostics are the biggest single item: ~30,000 cycles.** They are two
   compares plus a counter per coefficient/symbol — work the C product build
   does not do at all. **C-3b is therefore 3× the harvest of the M/S loop.**
2. **M/S costs 16,067 but its loop shape is not the problem.** Rewriting it as
   four-coefficient blocks — the unit the reference's `uli_ms_asm` moves — was
   built and measured: it recovers only 1,776 worst / 896 mean. The cost is
   **memory traffic**: our `HotState` (and therefore `x`) is in EWRAM, while the
   reference's `uli_g_X`/`uli_g_P` live in IWRAM (`uli_gba_dec.iwram.c`), where
   the same M/S is ~4,400. Every coefficient written or transformed pays 16-bit
   EWRAM wait states. **The lever is coefficient buffers in IWRAM**, which is
   blocked on the ≈15.0 KiB stack — and C-1's asm is what removes the spills that
   need it. This is now the second-largest identified item and it is *not* an
   entropy rewrite.
3. **The dequant arithmetic is only ~4,000.** Of the 23,030 in its row, ~19,200
   is the `xover` diagnostic compare that the `Ops`+`Diag` arm also captures. The
   32-bit `rq` path is already near its floor; do not spend more here.
4. **Fills cost ~7,400 mean and are not removable.** Zero bands and the post-EOB
   tail must be written because `pre`/`post` read all `M` coefficients. They do
   set the floor: sparse frames drop from 353,340 to 325,109 without them.
5. **Per event, escapes (134) and long codes (221) are the most expensive things
   after the symbol itself** — 74 escapes and 8 long codes in the mean frame,
   ≈11,700 cycles. Worth a look once 1 and 2 are done; not before.

**Conclusion.** The huff/field path is ~200 cycles per symbol, which is the same
order as the C record's whole non-synthesis budget per symbol. The remaining gap
is therefore **not** an unexplained per-symbol inefficiency: it is diagnostics
(~30,000) plus **EWRAM traffic on the coefficient buffers**, and both of those
are understood. Re-based: C-3b alone puts the frame at ~458,000 (175%); C-3b plus
IWRAM coefficient buffers is the path to ~170% before C-1 lands, after which the
kernel prize (~194,000) is what closes the budget.

### PR C-1 probe: hand-written ARM FFT, measured (2026-11-14, mGBA, WAITCNT 4/2)

**This is the keystone experiment the C-1 projection rested on, and it does NOT
confirm the projection.** A complete hand-written radix-4 DIF FFT
(`crates/uli-lite-gba/asm/fft_arm.s`, ARM mode, `.iwram`, `global_asm!`) was
written against our own oracle, twinned bit-exactly, and measured in isolation
against the Rust ARM-mode `fft` in the same image. Instrument:
`examples/uli_asm_probe` (on-target twin; **since retired** — `make uli-kernel-twin`
is the current witness) and the bench's `asmfft` arm
(`make uli-bench` with `--features asm-fft`).

| arm | cycles / FFT call | vs Rust | vs C record (~31,400) |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (`overlay`, control) | **85,065** | 100% | 2.71× |
| **hand-written asm FFT** | **88,205** | **103.7%** | **2.81×** |
| C/asm reference (per channel) | ~31,400 | — | 1× |

Correctness: the asm FFT is **bit-exact** with the reference on the ARM7TDMI
(`[asm-probe] FFT matches the reference`; bench `fftasm=true` on a 512-sample
buffer and across the corpus). It assembles, links into `.iwram`, and its own
frame is 84 B (push + 48-byte frame).

**Design, as the allocation PR C said Rust could not express:** t0..t3 live in
r4–r11 (8 registers held across all four outputs), one twiddle pair at a time in
r2/r3, SMULL/SMLAL scratch in r8/r9, x base in r12; the stage bookkeeping and
out2's two sums (t0, t2) spill to a 48-byte stack frame. The inner butterfly
emits **exactly 6 SMULL + 6 SMLAL** (the irreducible complex-multiply count)
and **no `__aeabi_lmul`** — the same codegen quality PR C recorded for the Rust
version, now guaranteed by hand.

**The finding: hand asm did not move the FFT.** ~3.7% *slower* than the Rust ARM
kernel, still ~2.8× the C record. The register-pressure hypothesis ("LLVM spills
66 intermediates; a hand allocation will recover the gap") is **falsified as the
primary lever**: the same arithmetic with a hand allocation lands in the same
place. Per-butterfly it is 12 multiplies + ~40 LDR + ~17 STR — the memory
traffic (twiddle reloads and the t0/t2 stack spill per output) is what remains,
and the 12-multiply core is an arithmetic floor of roughly ~30–38k cycles for
64 butterflies at ~4–5 cycles per SMULL/SMLAL, i.e. **the C number is near the
multiply floor, not a register-allocation gap we can close for free.**

**Consequence for the plan, stated before the follow-up work:** C-1's ~194,000
prize is **not** available from FFT asm as scoped. Either (a) the remaining FFT
gap is paid by cutting the memory traffic toward the multiply floor (fewer stack
reloads, twiddles kept resident, possibly unrolling the whole stage) — a
smaller, different prize than ~194k — or (b) the 63%-synthesis half of the
parity plan does not close and the target must be re-based. The **stack** half
of C-1's premise is also not delivered by this kernel: its own frame is 84 B,
but the ~15.0 KiB product-path stack that blocks IWRAM coefficient buffers is
not addressed by the FFT, so C-3's "coefficient buffers to IWRAM" stays blocked
until the real stack consumers are found. Recorded here so the ~73–81% parity
row is not read as reachable by asm alone.

Feature gating: `asm-fft` (off by default). `ArmKernels::fft` uses the asm when
the feature is on; `overlay::fft` (the Rust kernel) stays the control and every
other arm keeps its baseline in the same image. The product decoder
(`GbaDecoder`) does not route through `ArmKernels`, so `prod` is unchanged in
the `asm-fft` build — a separate wiring fact, not a result.

#### PR C-1 traffic follow-up: the memory-traffic hypothesis, measured

Because v1 landed at the same cost as the Rust kernel, the residual was
identified as **memory traffic**, not spills. A second kernel (`fft_arm_v2.s`)
removes exactly that traffic and nothing else — no t-value is spilled, `p` and
the twiddle base stay resident, and the loop-invariant stride/bound stop being
reloaded per butterfly — and was measured in the same image, bit-exact.

| arm | cycles / FFT call | % of Rust | × C record (~31,400) |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (control) | 85,065 | 100% | 2.71× |
| asm v1 (spill-heavy) | 88,205 | 103.7% | 2.81× |
| **asm v2 (traffic-reduced + TWJ hoist + stride/bound in-place)** | **73,363** | **86.2%** | **2.34×** |

**v2 is 16.8% faster than v1 and 13.8% faster than the Rust ARM kernel**, bit-
exact on the ARM7TDMI (probe `v2 matches`; bench twin `fftasm=true` for both
arms). The experiment therefore **confirms the memory-traffic hypothesis as a
real lever**: the ~15,000 cycles/call v1 lost to stack spill and reload are a
true cost, and a hand allocation that deletes them pays. The kernel is still
2.3× the C record, so the FFT is **not** closed by this alone, but the gap is
now demonstrably traffic-bound and stepping down monotonically:
85,065 → 88,205 (spill) → **73,363** (traffic removed).

The remaining ~42,000 over the C record is the multiply core plus the
irreducible load/store of A..D (8 loads) and the four outputs (8 stores) plus
six twiddle words per butterfly. Whether that can be cut further is the next
question; the instrument (probe + bench arms, same image, same twin) is in place
to answer it incrementally.

#### PR C-1 traffic follow-up 2: block transfers, and why our traffic > C's

A third bit-exact kernel (`fft_arm_v3.s`) converts the general-stage loads and
the out0 store to `ldmia`/`stmia` (2-word block transfers) — the structural
difference the C reference has and we did not. Measured in the same image:

| arm | cycles / FFT call | % of Rust | × C record |
|---|---:|---:|---:|
| Rust ARM-mode `fft` (control) | 85,065 | 100% | 2.71× |
| asm v1 (spill-heavy) | 88,205 | 103.7% | 2.81× |
| asm v2 (traffic-reduced) | 73,363 | 86.2% | 2.34× |
| **asm v3 (v2 + `ldmia`/`stmia`)** | **71,635** | **84.2%** | **2.28×** |

**Why our memory traffic was worse than the C reference** — read from
`../../gba-audio/src/uli/gba/uli_imdct_arm.S` (analysis only; nothing copied into
this tree, the asm here is written against our own oracle):

1. **Block transfers.** C loads the complex operands with `ldmia r0,{r3,r4}` /
   `add …; ldmia …` — two words per instruction — and stores y0 with `stmia`.
   v1/v2 emitted **separate `ldr`/`str` per word** (16 load/store instructions
   per butterfly vs C's ~8). On a 4-cycle-fetch ARM7TDMI that halves the fetch
   cost of the operand traffic. This is v3, and it bought **−2.4%** on its own.
2. **A trivial-butterfly path.** C runs `TRIV_BFLY_P` for the identity-twiddle
   butterfly (j == 0 of each stage, where `TW[0..6] = (2^30,0)*3`, i.e. exactly
   1.0 in Q30): pure adds + `asr #2`, **no multiplies and no twiddle loads**.
   We still run the general path there. This is ~1.5% of the butterflies (3 of
   192 radix-4) — a small, not-yet-taken win.
3. **Macro-fused form-y.** C's `FORM_Y_P` computes the four t-sums, does y0's
   `asr #2` and store inline, and reuses registers so the sums are never
   spilled — the same idea v2 implements by hand (the outputs overwrite the
   t-registers they consume). v2 matches this structurally.

The ladder — one lever at a time, same image, all bit-exact: **85,065 (Rust) →
88,205 (v1 spill) → 73,363 (v2 traffic) → 71,635 (v3 block transfers)**, i.e.
v1→v3 is **−18.8%** and v3 is **−15.8% vs the Rust kernel**. The two structural
differences from C are now closed (block transfers) or quantified (trivial
path ~1.5%); the residual to the C *record* is the multiply core plus the
remaining genuine A..D loads / output stores / twiddle loads, and that record
was measured on a different corpus and image, so `2.28×` is not a like-for-like
statement — it is the honest gap to the number of record.

Instruments (**since retired**; `make uli-kernel-twin` replaces it): `examples/uli_asm_probe` (all three kernels twinned on the
ARM7TDMI) and the bench's `asmfft`/`asmfft2`/`asmfft3` arms + `fftasm` twin.
Note for future arms: the twin's scratch buffers had to move from `run()`'s
stack to EWRAM statics — three 512-word kernels plus the ~15 KiB product-path
stack overflowed `.iwram` once the asm grew it (the same stack class PR B and
the `iwram-tables` crash recorded).

#### PR C-1 landed: wired into the product, and where to look next

The kernels were then wired into the **product hot path**: `GbaDecoder`'s
synthesis calls `crate::fft_kernel`, which dispatches to the best asm variant
(v4 since 2026-11-15) when `asm-fft` is on (the `#[cfg]` lives in one helper).
Before this, the asm only reached `ArmKernels`, so `prod` was unchanged; now it
moves.

| arm | worst | mean | of period (worst) |
|---|---:|---:|---:|
| product frame, Rust FFT (`asm-fft` off) | 457,863 | 410,912 | 175% |
| product frame, asm FFT v3 (`asm-fft` on) | 430,997 | 384,046 | 164% |
| **product frame, asm FFT v4 (+ trivial butterfly)** | **425,795** | **378,845** | **162%** |

**v3: −26,866 / −26,866 (−5.9% / −6.5%)**; **v4: a further −5,202 / −5,201**,
i.e. **−32,068 / −32,067 vs the Rust-FFT product build (−7.0% / −7.8%)**. Both
pins `e6e87652` / `b3530978` PASS, `xover=0 hfnz=0`, on-target twin PASS. The
product-frame delta is exactly `2 ×` the per-FFT saving (two channels). Ladder
now **2526% → … → 175% → 164% → 162%** worst (145% mean). U4 (≤100%) is still
not met.

**Why this Rust+asm stack still loses to C+asm, and the research order to close
it** (2.20× the C record's ~31,400 per FFT — and, since C's number is from a
different album/image, the sub-goal is to get under 31,400 *in this tree's
instrument*, not to match a foreign percentage):

1. **~~Trivial-butterfly path~~ — DONE as v4 (`fft_arm_v4.s`), 2026-11-15.**
   C's `TRIV_BFLY_P` handles the identity twiddle (j==0 of each stage, where
   `TW[base..base+6] = (2^30,0)*3`, i.e. 1.0 in Q30, verified for l=256/64/16,
   and `cmul(y,1.0) == y>>2`) with pure adds + `asr #2`: no multiplies, no
   twiddle loads. v4 takes that path and measures **69,034 cycles/call**, i.e.
   **−2,601 (−3.6%) vs v3** — larger than the ~1,000 projected, because the path
   also skips the per-output twiddle-pointer load and the six twiddle loads.
   It covers `1 + 4 + 16 = 21` of the 192 radix-4 butterflies (an earlier note
   said 3/192 by counting j-iterations, not butterflies). **This closes the last
   structural difference to C.**
2. **~~Twiddle loads~~ — RESOLVED 2026-11-15, and not by `ldmia`.** The
   `ldr`->`ldmia` idea was worth ~1-2%. The real cost was that `TW` sat in
   cartridge ROM (`.rodata` @ `0x0800d640`) while C keeps tables in IWRAM: a
   same-image A/B measures **−10.4%** for relocating the table, delivered by the
   opt-in `iwram-tw` feature (see "follow-up 3" below). With the table in IWRAM
   the read is ~1 cycle, so `ldmia` would now save only instruction count (~1%)
   — still worth one measured arm, but it is no longer where the money is.
3. **The multiply core is the floor.** 12 SMULL/SMLAL per butterfly (6+6) ×
   192 radix-4 + tail is ~the C kernel's own arithmetic. If 1 and 2 land and
   the FFT is still over, the gap is the per-butterfly load/store of A..D (4
   `ldmia`), the four output stores, and the twiddle loads — the genuinely
   irreducible memory traffic — not codegen.

4. **Corpus/image.** PR E's worst-frame program (deferred) is what makes the
   gap to C like-for-like. Until then every `×` is against a foreign number.

**C-3d LANDED 2026-11-15 — the OLA outlier was mostly a measurement artifact.**
`ola` fused the format selection per sample and computed **both** int8 and int16
whenever the caller passed an int16 buffer, and the bench passed one (to pin both
hashes) — so the arm the plan was reading (34,780/call, 2.27x C) was a
configuration the C record never timed. C ships one kernel per format
(`uli_ola_s8_asm` / `uli_ola_s8w_asm` / `uli_ola_s16_asm`) and selects the format
outside the loop.

Two fused kernels now do the same (`ola_s8`, `ola_s16`, format fixed at compile
time, no per-sample branch, no arithmetic for the format not being written), and
`ola_kernel` resolves the `Option` once so the product path gets the int8 kernel
when it has no int16 buffer. Measured, same image, same kernel family:

| OLA arm | cycles/call | vs C (15,300) |
|---|---:|---:|
| dual (what the plan was reading) | 34,780 | 2.27x |
| **s8 — the production headphone path** | **21,418** | **1.40x** |
| s16 — pre-DAC profile | 23,220 | 1.52x |

**OLA's headroom against C shrank ~69%** (19,480 -> 6,118 per channel), so OLA
asm is now a small item, not the second-biggest kernel prize the earlier table
suggested. The earlier `2.28x` row is superseded.

**And it is a real frame win, not just a fairer measurement.** The product was
writing int16 it never uses: an int8-only product pass (`decode_frame(.., None,
None)`, fused OLA, identical int8 pin `e6e87652`) measures

| product frame | worst | mean | of period |
|---|---:|---:|---:|
| dual output (previous default) | 387,878 | 340,928 | 148% |
| **int8-only (production)** | **361,238** | **314,288** | **138%** |

**-26,640 worst / -26,640 mean (-6.9%)**, `1.99x` the C decode wall. Ladder:
2526% -> ... -> 175% -> 148% -> **138%**. The int16 profile keeps its own arm.

**C-3 LANDED 2026-11-15 — `HotState` in IWRAM: the largest single win so far.**
The decoder's hot working set (`x` 4,096 + `buf` 2,048 + `p` 2,048 = **8,192 B**,
the same three buffers C keeps as `uli_g_X`/`uli_g_buf`/`uli_g_P` in IWRAM) moved
from EWRAM to IWRAM. EWRAM charges 16-bit wait states on a 32-bit bus, so every
coefficient write, every M/S read/write, and every pre/post/OLA pass over the
buffers was paying ~5 extra cycles per access.

| product frame | worst | mean | of period | vs C |
|---|---:|---:|---:|---:|
| before C-3 (int8-only) | 361,238 | 314,288 | 138% | 1.99x |
| **after C-3 (int8-only, production)** | **274,198** | **232,090** | **105%** | **1.51x** |
| after C-3 (dual output arm) | 300,838 | — | 115% | 1.65x |

**−87,040 cycles/frame (−24.1%)**, both pins `e6e87652` / `b3530978` PASS, all
twins PASS. **The plan projected 10,000–15,000 for this item; it delivered
87,040 — about 6x** — because the projection counted only the M/S pass (16,067)
while the win captures *all* coefficient traffic through `x`/`buf`/`p`. Mean is
now **under** the period (89%); worst is **12,054 cycles (4.6%) from real time**.

**How it was paid for in IWRAM** (the binding constraint). `.iwram` went from
22,436 B to 28,852 B of 32,512. Two principled savings made the 8,192 B fit:
1. **The Rust `fft` moved to ROM** when `asm-fft` is on (1,056 B). It is not on
   the product path — `fft_kernel` dispatches to v4 — so IWRAM should not pay for
   it. With `--no-default-features` it *is* the product kernel and stays in
   IWRAM; the placement is gated on the feature.
2. **v3 moved to ROM** (708 B): superseded by v4 (v4 = v3 + trivial butterfly).
   v1/v2 went the same way in the same change. All three remain callable
   comparison arms; their recorded numbers were taken in `.iwram`.

Stack: usable from `0x03007F00` is 3,660 B, high-water ≈2,960 B, leaving **~700 B**
— usable for this image, below C's own ~1 KiB assert, and the real product ROM
(UI + feeder) must re-check it before shipping. `HotState` size is pinned by a
compile-time assert so the budget cannot drift silently.

**PLANNED NEXT — `pre`/`post`/`ola` as asm (the rest of C-1), 2026-11-15.**
*(Live version, with the step-0 re-measurement caveat, is the "Critical path to
real time" section at the top of this file. The numbers below predate C-3 —
buffers were still in EWRAM — so they bound the ratios, not today's absolutes.)*
The FFT is now near its multiply floor; the next synthesis target is the *other
three kernels*, which are still the **Rust ARM-mode** kernels. Measured per channel
with tables in IWRAM, against the C record (`ovl` arm of `uli_bench`):

| kernel | ours (Rust ARM) | C/asm record | ratio | headroom vs C |
|---|---:|---:|---:|---:|
| pre | 17,654 | 9,700 | 1.82× | 7,954 |
| fft (v4, asm) | 61,852 | 31,400 | 1.97× | 30,452 |
| post | 16,010 | 10,900 | **1.47×** | 5,110 |
| ola (dual, superseded) | 34,780 | 15,300 | 2.27× | 19,480 |
| **ola (s8, production)** | **21,418** | **15,300** | **1.40×** | **6,118** |
| pair | 260,797 | 134,600 | 1.94× | 126,197 |

**OLA's contribution to this list is much smaller than it first looked** (C-3d,
below: the production int8 kernel is 1.40x C, not 2.28x). What remains of the
list is `pre` (1.82x) and `post` (1.47x), which together are the larger half
now. Historical note on the ordering:
1. ~~**C-3d first (measurement fairness)**~~ — **DONE 2026-11-15**, above: the
   format is now selected outside the loop and the production arm is 1.40× C.
2. **Then OLA asm** on the *s8* kernel — but note the prize is now ~6,100/channel
   (~12,200/frame), not the ~39,000 the dual-output arm implied, so `pre`/`post`
   are the larger half of what is left.

`pre` and `post` follow with one asm kernel each (a twiddle-table walk with a
single `SMULL`/`SMLAL` pair per output — structurally simpler than the FFT, so
they should come cheap). Together the three are now **~38,000 cycles/frame** of
identified headroom per the pre-C-3 kernel table, and `pre` is the largest single
item. **Note after C-3 (105%):** only **12,054 cycles** separate the production
frame from the period, so the *next* synthesis arm is not chosen by kernel ratio
any more — `pre` asm alone (~16,000/frame) would clear the period if it lands at
C's number. That makes `pre` the single most valuable next kernel.

#### PR C-1 follow-up 3: the twiddle table is the real "twiddle load" cost

**The planned item was `ldr` -> `ldmia` for the three twiddle pairs (~1-2%). The
measurement found the dominant cost is PLACEMENT, not instruction count.** With
`iwram-tables` off, `TW` is a plain `const` and therefore lives in `.rodata` —
which the linker map puts at `0x0800d640`, i.e. **cartridge ROM**. Every
twiddle load in the hot butterfly was therefore a slow ROM read (~6 cycles at
WAITCNT 4/2 with no prefetch), the same class of problem as ROM-resident code.
The C reference keeps its tables in IWRAM (~9.4 KB); we did not.

Measured as a **same-image A/B** (one build, one kernel, only the table pointer
differs — `fft_v4_tw(x, tw)` timed against the ROM `TW` and against an `.iwram`
copy):

| arm (v4, 69,034 with ROM twiddles) | cycles/call | vs ROM |
|---|---:|---:|
| twiddle table in `.rodata` (ROM) | 69,034 | — |
| **twiddle table in `.iwram`** | **61,852** | **−7,182 (−10.4%)** |

That is **~6.2 cycles saved per twiddle load**, exactly the ROM wait-state cost
over ~1,152 loads/call — the mechanism confirms itself. Product frame, with the
new `iwram-tw` feature moving `crate::TW` into `.iwram` (the product path picks
it up through the same `tw` argument, no asm change):

| arm | worst | mean | of period (worst) |
|---|---:|---:|---:|
| product, v4, TW in ROM (`asm-fft`) | 425,795 | 378,845 | 162% |
| **product, v4, TW in IWRAM (`asm-fft,iwram-tw`)** | **411,431** | **364,481** | **157%** |

**−14,364 worst / −14,364 mean**, exactly `2 ×` the per-FFT saving (two
channels). The FFT is now **61,852 vs the C record's ~31,400 = 1.97x** — under
2x for the first time. Ladder: 85,065 (Rust) → 88,205 → 73,363 → 71,635 →
69,034 → **61,852** (IWRAM twiddles).

**Budget, stated because it is the binding constraint.** `iwram-tw` costs
**2,016 B** (TW is 504 words). `.iwram` goes 14,860 → 16,876 B of 32,512,
leaving **~600 B** against the recorded ~15 KiB product stack — viable in the
bench image (it runs, all twins/pins PASS) but **not** comfortably safe for the
full product ROM, which adds UI and a feeder. Two ways to pay for it, both
deferred to the U6 budget decision: drop the superseded v1/v2 kernels from the
unit (frees 1,520 B; their numbers are recorded here), or accept a smaller
stack margin. The feature is therefore **opt-in**, exactly like `iwram-tables`.

**Consequence for the research order.** Item 2 was "twiddle `ldmia`". With the
table in IWRAM the read cost collapses, so `ldmia` would now save only the
instruction-count difference (~1%), not a wait-state penalty — it is worth
measuring once, but the 10.4% was in the table's location. This also revises
the "why are we slower than C" answer: **a large part of it was table
placement**, not kernel codegen. The remaining gap is now the multiply core,
the A..D/output traffic, and a like-for-like corpus (PR E).

### PR C (2026-11-14, mGBA, WAITCNT 4/2)

Full frame, 129-frame corpus, 262,144-cycle period:

| arm | worst | mean | min |
|---|---:|---:|---:|
| core kernels (control) | 2,681,000 (1023%) | 2,175,635 (830%) | 1,376,906 |
| overlay kernels (`ArmKernels`) | 1,677,608 (640%) | 1,171,161 (447%) | 371,463 |
| **product hot path, split `huff`** | **523,267 (200%)** | **456,187 (174%)** | 351,446 |

All arms pinned, `xover=0 hfnz=0`, on-target twin passed. PR B was 533,605 /
463,660; the split-`huff` change is the 1.6% in between (IWRAM +896 B).

Frame-101 attribution of the 523,267 (by difference, one image): kernels
328,502 (63%), `dec_chan` × 2 ≈175,000 (33%), M/S + wrapper ≈20,000 (4%).
Kernels against the C record's 134,600 are 2.44×; entropy per
coefficient against the C bench's entropy+dequant (~41 c/coef) is ~6.6×.

### Hardware, GBA SP (2026-11-16) — the three player ROMs, and a late-re-arm finding

**Every run below passed**, and the two runs of each ROM are **identical to the
cycle** — the determinism the FLAC ledger demands as a witness, now on this
target. The GBA 001 was unavailable (hardware fault), so this is one unit.

| ROM | worst (frame) | mean | margin at worst | periods | underruns | tails | skips |
|---|---|---:|---:|---|---:|---:|---:|
| `uli-player` (headphone) | 247,789 (f13,905) = **94.52%** | 213,362 = 81.39% | 5.48 pts | 16,552 | 0 | 0 | 0 |
| `uli-player-speaker` | 251,365 (f16,196) = **95.89%** | 216,547 = 82.61% | 4.11 pts | 16,552 | 0 | 0 | 0 |
| `uli-player-two-track` | 247,876 (f17,158) = **94.56%** | 212,133 = 80.92% | 5.44 pts | 20,546 | 0 | 1 | 1 |

- Against mGBA on the same images: **+1.11 / +1.31 points** on the headphone build
  (247,789 vs 244,879; 213,362 vs 209,937) — the silicon penalty is ~1 point here,
  far smaller than the LPC row the FLAC ledger recorded, and the worst frame still
  leaves 4–5.5 points of the period.
- The two-track run exercises a real track change on silicon: `tails 1 skips 1`,
  zero underruns — the skip's bounded gap accounted as a tail, as designed.

**The finding: audible drops the ledger cannot see.** The listening report is
"brief drops, frequently" during the runs, while every ledger field says clean.
That is a **late re-arm**, and it is now measured: at each feeder entry the ROM
reads the period timer and counts entries that arrive more than the DMA's grace
(the FIFO's 16 samples) past the overflow.

| image | redraw | `late` entries | `max_late` | underruns |
|---|---|---:|---:|---:|
| `uli-player` (1 Hz live text) | 268 redraws | **104** | 34 samples | 0 |
| `uli-player-noui` (`--features ui-off`) | none | **1** | 17 samples | 0 |

**The 1 Hz text redraw causes 104 of the 105 late entries.** A late re-arm is
audible (the DMA replays the previous slot and splices mid-waveform) but it is not
a starved period, so the underrun ledger is blind to it by construction — the
player's PASS was true and the listening report is also true. The measured cost of
the redraw is ~410–470 k cycles (1.6–1.8 periods), inside agb's display path, and
that is longer than the 488 µs grace.

Consequence for the product, and it is the plan's own rule ("the UI is the slack;
audio has priority"): **the audio-critical default is the end-only screen**
(`ui-off`), and a live UI has to be built so it does not block the feeder — a
digit-tile clock rather than a full background rebuild per second, or rendering
sliced across periods. `late` is the gate for that work, and it is on the final
screen next to the ledger.

### PR D, album-scale (2026-11-16, mGBA, WAITCNT 4/2) — the full-length player build

`make uli-player-cross`: E3-b's twelve tracks in one player ROM, 33,407,264 B of a
33,554,432 B cart (**147,168 B headroom**), headphone profile, ring 8,192 samples.

| quantity | value |
|---|---|
| frames played | 167,224 (album frames + per-track padding frames) |
| worst non-boundary frame | **245,671 (f11,531) = 93.72%** |
| mean | **205,763 = 78.50%** |
| track-change frames | **12, worst 343,152** (each also decodes the next track's preroll) |
| feeder | 167,232 periods, 167,232 served, **0 underruns, 0 tails** |
| ring high water | 8,192 samples (full) |
| `skipped_samples` | 2,769 (the twelve tracks' `skip` fields, all accounted) |
| UI redraws | 2,622, mean 377,163 cycles, worst 524,392 (outside the frame window) |

- The boundary cost is *by design* and is what the ring is for: 343,152 cycles is
  1.31 periods, and the queue never ran dry in 167,232 periods.
- Image-specific: the instrument's gate arm on the same corpus reads 96.32%, this
  player image reads 93.72% for its worst non-boundary frame. Different images,
  so they are not compared; the player number is the product one.

### E3 (2026-11-16, mGBA, WAITCNT 4/2) — the two pre-hardware corpora

| corpus | frames | bytes | decode worst / mean | gate worst / mean | feeder |
|---|---:|---:|---|---|---|
| E3-a adversarial density | 641 | 127,432 | 213,783 (81.55%) / 184,896 (70.54%) | 251,431 (95.91%) / 216,753 (82.68%) | 640/640, 0 starved |
| E3-b sibling 12-track album | **167,222** | 33,088,280 | 220,864 (84.25%) / 185,749 (70.86%) | **252,490 (96.32%)** / 217,119 (82.83%) | 167,221/167,221, 0 starved |
| (PR E master, for comparison) | 16,553 | 3,293,048 | 220,062 (83.94%) / 189,780 (72.40%) | 251,712 (96.02%) / 221,245 (84.40%) | 16,552/16,552, 0 starved |

- **The album-scale repertoire's tail is 0.30 points worse** than the single
  master's (252,490 against 251,712): the first corpus to beat PR E's worst frame.
- **The quad-heavy class is cheaper, not dearer** (251,431): quads carry four
  coefficients per symbol, so the class the master lacks is not the class that
  would have hurt.
- Pins: E3-a `88738a70` (63 checkpoints), E3-b `51fcf846` (103 checkpoints), both
  C-sourced from the C decoder's own `hash_of_frame_hashes`; both albums decode
  with `xover=0 hfnz=0` and are bit-exact against the C reference.
- E3-b's encode took 14.4 min at `--threads 8`; its fetch+prep 1.3 min (649 MB of
  CC-licensed FLAC, cached outside the tree), and the measurement arm 19 min.
- ROM sizes: the E3-b gate image is 33,544,188 B of a 33,554,432 B cart — **10,244
  B of headroom**, which is what forces the smaller budget.

### PR D (2026-11-16, mGBA, WAITCNT 4/2) — the player, both output profiles

Corpus: the same generated album (`3,293,048 B`, 16,553 frames, 32,768 Hz), the
same blob PR E measured. Ring 8,192 samples (16 frames), prefill 8 frames, one
feeder period = one frame (64/s).

| build | worst (frame) | mean | % of 262,144 | underruns | periods (served / tails) | max ring |
|---|---|---:|---:|---:|---|---:|
| headphone (stereo as decoded) | 244,879 (f16,196) | 209,937 | 93.42 / 80.08 | **0** | 16,553 (16,552 / 1) | 8,192 |
| speaker, `(L+R)>>1` | 250,473 (f3,206) | 216,254 | 95.55 / 82.49 | **0** | 16,553 (16,552 / 1) | 8,192 |
| speaker, `L`-only | 242,755 (f16,196) | 207,823 | 92.60 / 79.28 | **0** | 16,553 (16,552 / 1) | 8,192 |

- Frames played 16,544 = 16,553 − 8 prefill − 1 preroll; `frames_fed` = 16,552.
- The window is decode → fold → push with IRQs live; the ~1 Hz UI redraw is
  outside it. The three builds are **different images**, so the fold deltas are
  indicative: `(L+R)>>1` ≈ +6.3 k, `L`-only ≈ −2.1 k cycles/frame against the
  stereo build. Both are consistent with a per-sample cost of a few cycles
  (512 samples per frame).
- The speaker profiles are **not** a stereo-shortcut: both decode two channels
  (the decoder is untouched) and fold after it, which is why their worst frames
  sit where a stereo frame does.
- Instrument note: the 32-bit counter wraps every 256 s and this album is 258 s,
  so the player zeroes it every 1,024 frames; a window above 2²⁴ is counted as a
  suspect and fails the verdict rather than entering the mean.

### PR E (2026-11-15, mGBA, WAITCNT 4/2) — album corpus, tail and gate total

Corpus: `assets/sound/bioluminescent.wav` (258.61 s, tracked) → 32,768 Hz stereo
via `prep.py` → C encoder of record, `--cap 74`, `--budget 3,294,000` (the
reference album's 12,742 B/s, scaled) ⇒ **3,293,048 B, 101.686 kbit/s, 16,553
frames**, K=14, M/S in 87.1% of frames, 315.2 nonzero coefficients/frame, 30.7
coded bands/frame, 42,760 long Huffman codes, encoder verify `|X|` overflows 0.
C-sourced pin: `hash_of_frame_hashes 55e710af`; 67 published checkpoints
re-verified by `scripts/uli_album_pin.py` from the C decoder's own `s8/` output.

| arm | image | frames | min (frame) | worst (frame) | mean | sum |
|---|---|---:|---:|---:|---:|---:|
| `prod8` decode wall | `make uli-album-bench` | 16,553 | 144,725 (f15) | **220,062 (f16,204) = 83.94%** | **189,780 = 72.40%** | 3,141,376,572 |
| `prod8` decode wall | `make uli-gate` | 16,553 | 144,765 (f15) | 220,102 (f16,204) = 83.97% | 189,817 = 72.41% | 3,142,038,692 |
| **`gate`, player model** | `make uli-gate` | 16,545 | 175,824 (f18) | **251,712 (f16,204) = 96.02%** | **221,245 = 84.40%** | 3,660,489,032 |
| `gate-hash`, verification in the loop | `--features gate-hash` | 16,545 | 175,855 (f16,414) | 257,759 (f16,204) = 98.33% | 226,320 = 86.33% | 3,744,468,460 |
| `prod8`, 129-frame fixture (control) | `make uli-bench` | 129 | 153,436 (f35) | 219,261 (f125) = 83.65% | 194,499 = 74.19% | 25,090,379 |
| `gate`, 129-frame fixture (player model) | `make uli-gate-fixture` | 121 | 184,619 (f40) | 250,925 (f125) = 95.72% | 228,188 = 87.05% | 27,610,750 |

- **Gate overhead, paired in the same image at the same frame: +31,610 cycles at
  f16,204; +31,428 mean (12.06 / 11.99 points).**
- **Feeder ledger, player model:** `pushed = consumed = 8,475,136`, ring `0`,
  `serviced = full_takes = 16,552`, `starved_entries = 0` ⇒ the pass ran
  **258.63 s, exactly the album**, with the queue drained.
- **The 772 starved entries reported in this file's first revision were the
  harness.** In-loop per-frame verification (the FNV hash, run *outside* the
  window) costs ~12,200 cycles/frame; a paced pass pays it in wall time, so the
  producer ran 4.7% slower than the audio clock and the ring starved for
  **12.06 s** across the album (`gate-hash`: `serviced 17,324`,
  `starved_entries 772`, runtime 270.7 s). A player hashes nothing, so the
  default arm does not either; the pin is witnessed by the untimed passes in the
  same image. The hash *also* moves the windows (+5,075 cycles/frame mean — shared
  bus traffic and a shifted IRQ phase), so the harness cost appears twice.
- **A drain was missing and is now in:** the producer finishes ~14% ahead with the
  ring full, so teardown was cutting the last ~250 ms; the pass now waits for
  `ready() == 0`. With the hash and the drain both out of the picture, the arm's
  runtime *is* the album's length — the invariant worth keeping.
- **The fixture's `gate` arm is not an exact overhead match** (+33,649 mean vs the
  album's +31,428): the fixture is encoded at 156 kbit/s against the product's
  102, so it is a denser corpus. Read the pair as "a 31–34 k cycle per-frame
  producer cost", which is the property being checked: it is a fixed per-frame
  cost, not a corpus-dependent one.
- The fixture's `prod8` +10 cycles/frame (219,251 → 219,261) is the recompile
  offset, not a regression.
- Instrument finding: the 32-bit cycle counter wraps every 256 s of emulated time
  and one album pass is 217–1,280 s. The first album run read windows of
  4,294,967,260 cycles in 4,546 frames with `wrapped=false` (a stale high word is
  a *valid* difference). Fixed by zeroing the counter every 1,024 windows and by
  a `SUSPECT_CYCLES` (2²⁴) bound that counts, prints and fails. The 129-frame
  corpus never reached either limit.

### Step 0 attribution — the product frame by difference (2026-11-15, mGBA, WAITCNT 4/2)

Instrument: `make uli-attrib` (target added with this measurement). Nine builds,
one image each, `--features attrib-dump` on every arm so the per-frame windows
can be differenced at the same frame index. The arms are **timing-only** cargo
features on `uli-lite-gba` that remove exactly one part of `GbaDecoder`'s frame;
the album pins fail by construction in all of them and no pin is quoted from
this section. The dump costs a constant **+16 cycles per window** (control
274,214 vs the product arm's 274,198; Σ = 16 × 129), which is why every absolute
below is the product arm's and every delta is control-minus-arm.

**Control (this instrument, 129 frames):** 274,214 worst at frame 98 /
232,106 mean; the product arm in the same tree without the dump is **274,198 /
232,090 (105% / 89%)** — the recorded number, reproduced exactly. Nothing in the
instrumentation perturbed the frame.

| arm (part removed) | worst | mean | Δmean | Δ f98 |
|---|---:|---:|---:|---:|
| — control | 274,214 | 232,106 | — | — |
| `attrib-synth` (all 8 kernel calls) | 112,860 (f101) | 71,838 | 160,268 | 162,987 |
| `attrib-pre` | 249,383 (f101) | 208,359 | 23,747 | 26,464 |
| `attrib-fft` | 193,180 (f98) | 148,765 | 83,341 | 81,034 |
| `attrib-post` | 251,836 (f98) | 208,224 | 23,882 | 22,378 |
| `attrib-ola` | 239,827 (f98) | 198,118 | 33,988 | 34,387 |
| `attrib-ms` | 266,252 (f0) | 227,692 | 4,413 | 9,619 |
| `attrib-entropy` (not quotable, see below) | 156,277 | 156,016 | 76,090 | 118,193 |
| `attrib-synth,attrib-entropy` | 742 | 481 | 231,625 | 273,728 |

**Decomposition (product frame):**

| part | mean | share | f98 | share |
|---|---:|---:|---:|---:|
| synthesis, 8 calls | 160,268 | 69.0% | 162,987 | 59.4% |
| └ `fft` (asm v4) | 83,341 | 35.9% | 81,034 | 29.6% |
| └ `ola` (Rust, fused s8) | 33,988 | 14.6% | 34,387 | 12.5% |
| └ `post` (Rust) | 23,882 | 10.3% | 22,378 | 8.2% |
| └ `pre` (Rust) | 23,747 | 10.2% | 26,464 | 9.7% |
| entropy + dequant | 66,943 | 28.8% | 101,122 | 36.9% |
| M/S | 4,413 | 1.9% | 9,619 | 3.5% |
| wrapper / glue | 481 | 0.2% | 486 | 0.2% |

**Why each number is quoted the way it is:**

- **Synthesis** is `control − attrib-synth` (the sequence, not the sum of the
  singles). The singles sum to 164,958 mean / 164,263 at f98, i.e. +2.9% / +0.8%
  over the sequence; the interaction is the FFT's data-dependent trivial-butterfly
  path, whose input changes when `pre` is removed.
- **Entropy + dequant** is `attrib-synth − (attrib-synth,attrib-entropy)`:
  71,838 − 481 = 66,943 mean. `attrib-entropy` on its own is **not quotable** —
  removing `dec_chan` freezes the bit position, so every frame re-synthesises one
  stale frame (156,016 mean against the 160,749 = synthesis + wrapper the arm
  should measure), and the 6.6× figure in the PR C section above is superseded by
  this one.
- **M/S** is `control − attrib-ms`. It is ~C-class in the mean (4,413 vs the
  record's ~4,400) and 9,619 on the dense worst frame, so the earlier "16,067 for
  the M/S loop" (PR C-2, pre-C-3, EWRAM `x`) is retired: what that arm measured
  was mostly EWRAM traffic, and C-3 fixed it.
- **Wrapper** is the `synth,entropy` arm: 481 cycles. There is no frame glue left
  to win.

**Per-channel in-frame kernel costs, against the C record:**

| kernel | ours | C/asm | ratio | prize/frame (mean) |
|---|---:|---:|---:|---:|
| `pre` | 11,874 | 9,700 | 1.22x | 4,347 |
| `fft` (asm v4) | 41,671 | 31,400 | 1.33x | 20,541 |
| `post` | 11,941 | 10,900 | 1.10x | 2,082 |
| `ola` (fused s8) | 16,994 | 15,300 | 1.11x | 3,388 |

These replace the pre-C-3 table (which read `pre` 17,654 and `post` 16,010 from
the isolated `ovl` arm, `fft` 61,852 from the asm-v4 + `iwram-tw` A/B and `ola`
21,418 from `ola_s8` — all timed on the bench's **EWRAM** buffers). The in-frame
numbers are 21–33% lower because C-3 put `x`/`buf`/`p` in IWRAM; the FFT's 61,852 c/call reading that made it "1.97x C, near its floor" was
the same artifact. **In-frame the FFT is 1.33x C and carries 20,541 of the
25,668-cycle synthesis gap.**

**The C rows are a worst-frame shape, and that is how the gate should read them.**
The record's decomposition (134,600 synthesis/pair + ~47,000 entropy/dequant/M-S)
sums to 181,600, within 0.2% of its recorded worst frame of 181,928 — so on the
worst frame the comparison is ours 274,198 against 181,928. The f98 components
are the dump image's, so they sum to 274,214 — 16 cycles (the constant) above
the product frame:

| | ours f98 (dump image) | C worst-frame shape | gap |
|---|---:|---:|---:|
| synthesis | 162,987 | 134,600 | 28,387 |
| entropy + dequant + M/S | 110,741 | ~47,000 | 63,741 |
| wrapper | 486 | — | — |
| **total** | **274,214** (product: 274,198) | **181,928** | **92,286** |

Entropy/dequant/M/S is therefore **69% of the distance to C on the gate frame**
and synthesis 31%. That inverts the previous order of work, which had entropy
fourth behind three kernels worth 9,817/frame together. It also re-answers the
old question "can asm alone close it?": no — asm on all four kernels at C-class
recovers 25,668 mean / 28,387 at f98, which clears the 12,054-cycle deficit to
100% but nowhere near the ~85% working target.


### Step 0b — entropy pre-analysis: where the 101,122 cycles go (2026-11-15)

Step 0 named the entropy/dequant path as the gate-critical item and left one
question open: *inside* it, what costs the cycles? This section answers the
cheap half of that with counts and code, before any rewrite. It is the input to
the sub-frame attribution PR, not a substitute for it.

**Method.** Symbol counts are a property of the bitstream, so they are read on
the host with `make uli-frame-ops` (`crates/uli-lite/examples/frame_ops.rs`)
instead of from a `verify` emulator build. Nothing here derives cycles; it
counts operations and pairs them with the Step 0 timed arms.

**The worst frame (f98) versus the frame the cycle model was tuned on (f72):**

| | f98 (gate frame) | f72 (old "worst") |
|---|---:|---:|
| bits in frame | 3,639 | 2,274 |
| Huffman symbols (`band + pair/2 + quad/4`) | **238** | 126 |
| band-class symbols | 61 | 64 |
| pair groups / quad groups | **352 / 4** | 0 / 248 |
| coefficients written (nz + zc) | 720 | 992 |
| nonzero / zero | 666 / 54 | 616 / 376 |
| raw-bit fields (`rbits`) | **128** | 0 |
| escapes | 21 | 140 |
| long codes (`huff_slow`) | **32 (13.4%)** | 0 |
| M/S frame | **yes** | no |

**The timed gate frame is not the busiest frame, and this is the trap to avoid.**
`make uli-frame-ops` names frame 125 as the most bits (4,225 vs f98's 3,639),
frame 90 as the most symbols (250 vs 238) and frame 32 as the most nonzero
coefficients (726 vs 666) — yet f98 is the worst *timed* frame. Its mix is what
costs: 352 pair groups, 128 raw-bit fields, 32 long codes, an M/S frame. Cost is
not a function of bits or of symbol count, so any attribution must be taken on
the timed frame rather than a proxy for density.

Reading it with Step 0's 101,122 cycles for the path: **~140 cycles per
coefficient slot** (720) and **~425 per Huffman symbol** (238). Per *nonzero*
coefficient it is ~152. The honest comparison stays the frame-level **2.36x**
(110,741 vs C's ~47,000); the per-coefficient form is for sizing levers, not for
a ratio claim. **Retired:** the "~272 cycles/coefficient against C's ~41" figure
carried by the PR C ledger — it divides our per-symbol work by the C bench's
per-coefficient work on a different frame and is not derivable.

**What this changes about the target.** f98 is **pair-class and M/S**: 352 pair
groups, 4 quads, 128 raw-bit fields. The quad path — the one the corpus model
and PR C-3a's `q3` expansion were built around, and the shape of the old worst
frame f72 — is almost absent from the gate frame. The hot path at f98 is
`CT_PAIR`: one Huffman symbol per 2 coefficients, two `get(rbits)` raw reads,
two escapes, two dequant+sign+store.

**What the code costs today** (`crates/uli-lite-gba/src/decoder.rs`; disassembly
of `dec_chan` in `.iwram`, 1,155 instructions = 4,620 B, 188-byte frame, 86
stack accesses):

- **The reader is addressed through memory.** `dec_chan` takes `br: &mut
  BitReader` (`:507`), and the out-of-line `huff_slow` (`:408`) and `eg_slow`
  (`:362`) take the same `&mut`, so the struct's address escapes: every refill
  and every slow call stores `next`/`hw`/`valid`/`bad` back into it
  (`str r1,[r5,#12]`, `str …, [r5,#16]`, `strb …, [r5,#20]`).
- **The `huff` fast path is 9 instructions with two conditional branches**
  (`lsr; add; ldrh; cmp; beq` for the LUT miss, then `ands; beq` for the
  `len == 0` sentinel the C decoder does not have), **plus a third branch in
  `skip`** (`cmp valid,len; bls` to the slow path at `:307`).
- **Refill is a loop with an end-pointer check and two byte loads**
  (`:261` + `:221`: `ldrb r4,[r3]; ldrb r6,[r3,#1]`, four in `set_pos`).
- **The sign bit costs a `peek` and a `skip`** (`put_coef`, `:454`, via `get`),
  i.e. two more branch-bearing window operations, where C tests the window's
  sign bit and shifts.

**What C does instead** (structural comparison; external reference, no source
incorporated — `gba-audio/src/uli/gba/uli_gba_dec.iwram.c` and
`uli_gba_int.h`):

- `REFILL(b)` runs once per symbol/group and *guarantees* n >= 16, so
  `huff` is `lut[w >> 22]` with one unlikely branch, and **`SKIP(b,k)` is
  unconditionally `w <<= k; n -= k`** — two instructions, no branch, no refill
  check.
- The sign comes from `((int32_t)b->w < 0)`, then `SKIP(b,1)` — no separate
  peek.
- Slow paths copy the reader in and out (`BR t = *b; …; *b = t`) so **only the
  temporary escapes to memory** — their comment says exactly this. The hot
  reader stays in registers.
- Per band they hoist what our loop reloads per coefficient: `r1 = rq[1]*step`,
  `dq15 = 16*15 - rq[15]`, and a **single** table pointer (`huff_slow` recovers
  the struct base from the LUT pointer), where our `HuffmanParts` keeps five
  slices live across the loop.
- They carry no bounds check on the refill path at all (malformed codes set an
  error flag instead); ours checks `next + 2 > end` per refill and per peek
  fallback.

**The levers, in expected-value order** (each its own measured arm):

1. **Take the reader off the stack** — by-value cursor, everything
   `#[inline(always)]`, slow paths take and return a copy. Biggest item, and the
   precondition for (2).
2. **Adopt the refill contract** — one refill per symbol/group guaranteeing
   >= 16 bits, then `huff`, `skip` and the sign are branchless. That removes most
   of the taken branches a pair iteration pays — one or more per `peek`, `skip`,
   sign, refill and escape — and on ARM7TDMI a taken branch is a 3-cycle pipeline
   flush with no predictor to hide it.
3. **Drop the hot-path bounds/`bad` checks** (validate at album parse) and
   refill with **one `ldrh`** instead of two `ldrb` (needs a 2-byte-alignment
   assertion on the ULA2 stream, which the format already assumes).
4. **Shrink the per-band live set** — one table pointer per band, hoist
   `rq[1]*step` and `dq15`, and skip the EWRAM `rq` load for the common `m == 1`
   digit. Register pressure is this project's measured failure mode (the FFT).
5. **LUT geometry last** — 32/238 symbols (13.4%) take `huff_slow` at f98;
   `LUTB` 11-12 or a two-level LUT, but only once the split says it matters.

**Do not retry** (each already measured, PR C): dequant fusion (neutral), full
`huff` inlining (+2.7% for 4,252 B of IWRAM), `Ops`/`Diag` on the hot path
(removed in C-3b).

**Next instrument — the sub-frame arms.** `attrib-entropy-*` cargo features on
`uli-lite-gba`, in the Step 0 style, all timing-only: `reader-only` (consume the
frame's bit count, no Huffman), `no-lut` (fixed-bit skip), `no-dequant` (store
the magnitude, no `rq`/multiply/sign), `symbols-only` (no coefficient writes),
plus a `huff_slow` hit counter. That partitions the 101,122 before the rewrite
lands. The arithmetic that keeps it honest: 12,054 cycles (to 100%) is ~12% of
this path, so levers 1-2 plausibly clear the period on their own; the ~51,000
needed for <= 85% wants the whole list.

**Blocked instrument, recorded so it is not silently relied on.** The bench's
`verify` build — which `per-frame` implies, and which the file's own wording
calls the ops witness — **fails the `ovl` pin on target at HEAD**
(`b3613428`/`b607b02e` with `--features verify`; `1b4f486e`/`28d04b52` with
`per-frame`), and the run stops before the product arms. The differing hashes
across two builds point at corruption rather than an arithmetic difference; the
leading hypothesis is `.iwram` headroom (29,688 B of 32,512 in the verify image,
i.e. 2,824 B below the stack top against a 768 B measured product high-water),
not yet demonstrated. `make uli-overlay-test` (host twins, `verify`) is green.
This is pre-existing, unrelated to the attribution work, and it is why the
counts above come from the host.

---

## Risks as recorded during the port (historical)

**The risks that still stand live in [`../../ULI2.md`](../../ULI2.md) §Risks.**
What is kept here is the reasoning that *drove* the work — chiefly the
codegen diagnosis that justified hand-written asm — because it is the argument
behind five kernels and it would be expensive to re-derive.

1. **LLVM's ARM7TDMI codegen vs gcc — now diagnosed, and it is register
   allocation.** PR C measured that the codegen is *not* missing instructions:
   the FFT butterfly carries 6 `SMULL` + 6 `SMLAL` and zero `__aeabi_lmul`
   (the PR B fix held). What it does instead is **spill 66 intermediates to the
   stack**: the radix-4 butterfly has 14 values live at once against 16
   architectural registers, and the 32-bit `i64` accumulation widens every live
   range. That is a decision Rust gives to the allocator and cannot express, so
   the kernels need hand-written ARM asm (`global_asm!`, already used by
   `flac-lite-gba`) — PR C-1. **Superseded in part by Step 0 (2026-11-15):** with placement fixed,
  `pre`/`post`/`ola` are 1.10–1.22x C, so this risk now sizes only the FFT
  (1.33x) and the entropy path, not the three Rust kernels.
  **Corrected again by the 2026-11-16 root-cause audit:** the FFT's 1.14x was
  *not* register allocation — it was the multiply **operand order** (twiddle in
  `Rs` instead of the data), worth −6,993 mean when fixed, with the loop already
  at C's instruction count. And the "register allocation" framing has a cause
  that is not LLVM's allocator: the target spec pins a frame pointer (item 11).
2. **IWRAM budget — the stack figure here is stale; see U6.** The reference
   spends ~22.6 KB (17.6 KB tables/buffers + 5 KB ARM code) of 32 KB. The
   ≈15.0 KiB stack quoted in earlier arms of this file was an artifact of
   stack-local twin buffers and was **withdrawn 2026-11-15**: the measured
   product stack high-water is **768 B**, `.iwram` sits at **25,548 B of
   32,512**, and `HotState` is therefore in IWRAM (C-3). Any further placement
   move still budgets stack first, but the budget is 768 B, not 15 KiB.
3. **Oracle drift.** The core must stay bit-exact and host-testable; every
   fast-path change is twinned. Do not “optimize” the oracle.
4. **Licensing — deferred, not resolved.** `gba-audio` has no LICENSE and the crate stays
   `publish = false` by decision (2026-11-14): no license outreach now. Note what
   this leaves standing: asm adapted from the reference is derived work from a
   repository with no license, which is acceptable only while nothing is
   published. A publish decision re-opens both this and `FORMAT.md`.
5. **Encoder independence — v1 boundary set.** The encoder of record stays the
   off-tree C tool for v1; this project builds nothing from it and vendors no
   part of it. The isolation rule is unchanged and now explicit: **no C source
   incorporated, no build/link/vendoring dependency on `gba-audio`, every
   artefact in this repo.** The Rust encoder is deferred, and v1's format is
   fixed (32,768 Hz, hop 512, stereo, no long-block) by decision 2026-11-14:
   the goal is matching the C project's fidelity here, not adding rates.
   **Superseded in part (2026-11-19):** the encoder is *scheduled*, not deferred —
   it is the last missing piece before ULI2 can be the album player's codec. The
   no-C/no-dependency rule and the fixed v1 format both stand, unchanged; see
   [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md) → "The encoder — the last missing piece".
6. **Playback is not implied by decode.** A fast decoder is not a player;
   U4 must pass with margin before Phase D starts, and U5/U6 are separate.
7. **The C comparison is not yet like-for-like.** Step 0 narrowed the two
   instrument asymmetries that used to inflate our frame: the hot loop no longer
   carries the `Ops`/`Diag` bookkeeping the C product build compiles out (C-3b),
   and the frame quoted is the **int8-only** `prod8` pass the product actually
   writes, not a dual int8+int16 pass (C-3d). What remains asymmetric is the
   corpus and image, so every `×` against 69.4% stays a diagnostic, and a
   pessimistic estimate must not be used to justify a work item.
8. **Worst-frame and corpus mismatch — narrowed.** The C record's 69.4% is
   frame 27806 of its whole album (167,222 frames); ours is frame 98 (Step 0;
   frame 101 before C-3) of a 129-frame corpus. The kernel-row disagreement (134,600 vs 146,000) is
   **resolved in favour of 134,600** by C's own stage-2 decomposition, which
   also removes ~11,400 from C-1's prize (it is ~194,000, not ~182,000). What
   remains open is the comparison itself: pin the reference row in this tree's
   instrument before writing C-1's numeric gate, and treat "C-class" as ±5% in
   this tree, not an inherited percentage. **Corpus decision 2026-11-14:** the
   129-frame corpus stays the iteration instrument — at multiples over budget,
   reaching a realistic regime matters more than worst-case fidelity, and
   full-album runs would slow the loop. The worst-frame program is PR E,
   deferred until the decoder is near real time; no claim may rest on the
   129-frame number alone.
9. **Stereo is the product target; the speaker is served by the output stage
   — decided 2026-11-14.** The decoder and the playback path are stereo, aimed
   at the headphone output: fidelity on this hardware is the point of the
   project and is not traded for headroom. The GBA speaker is physically mono
   and the headphone jack is stereo, so the **speaker profile is in scope but
   is not a decoder mode** — full stereo decode always, mono fold-down after it
   (PR D), with both regimes measured. C's `ULI_GBA_MONO` / `ULI_OUT_M16` (one
   IMDCT per frame) is **explicitly not taken**, and U4 is sized against the
   two-channel frame. Worth recording, since it looks like a free lever: C's
   69.4% is already the *stereo* wall (its synthesis loop runs per channel), and
   their mono attempt landed at 84.2% worst / 77.3% mean because the collapse
   lived in the driver at 13.65%/frame — over their own target, and that output
   was never hash-checked. Single-channel output is **not** equivalent to a
   fold-down (hard-panned content vanishes; on an M/S frame `L = M + S`), so if
   it is adopted for cost it must be recorded as a fidelity trade, not as the
   same feature.
11. **The Rust toolchain costs us a register relative to C + asm — open, and not
   accepted (2026-11-16).** `thumbv4t-none-eabi`'s target spec sets
   `"frame-pointer": "always"`, so r11 is pinned in **every** Rust function on
   this target; gcc at `-O2 -marm` omits the frame pointer, so the C product
   decoder has it. In ARM-mode hot code that is one of ~13 usable registers lost
   before the allocator starts, plus fp-relative addressing — `dec_chan` shows
   `push {r4-r9, sl, fp, lr}` / `add fp, sp, #28` and 259 stack/frame references
   in 1,394 instructions. **This is a big deficiency and it is being tracked, not
   written off.** What it needs: (a) a custom target JSON with
   `frame-pointer: "non-leaf"`, which changes the instrument and so carries a
   control build and a section/symbol dump per this file's baselines-to-hold
   rule; (b) an answer on stack unwinding *first* — the pin exists for
   unwinding/debugger walkability, and this tree measures stack high-water with a
   canary (844 B), not an unwinder, so the constraint may be unnecessary here
   (to be established, not assumed); (c) measurement as one whole-image arm,
   since it moves every Rust function at once and is not attributable by
   difference. It is deliberately **not sized** before the entropy contract sweep
   lands: the two levers overlap, because the per-symbol validity/bounds tests
   are exactly what occupies the registers r11 cannot. For the same reason,
   **1e-ii ("register relief in `dec_chan` does not pay") must not be cited as
   "registers are not the lever"** until this arm has run — that experiment's
   hypothesis was disabled by the toolchain before it was tested.

---

## Landing log (measurement milestones)

**Decisions live in [`../../ULI2.md`](../../ULI2.md) §Decision log** — that file is
the decision record. This is the ledger's own chronological index of what landed
and what it measured, at a finer grain than the decisions (one entry per step,
not one per decision). Where the two overlap, the decision record's wording is
authoritative and this one carries the numbers.

- **2026-11-20 — S6: the decoder-cycle price, and the high-rate gap it turns out to
  be all of.** An over-cap candidate is no longer refused: it is re-coded under
  `mu · (band symbol + decoded band work)` until the reader's cycle model fits
  `--cap`, and the final encode re-codes at the price that bought it (the
  reference's `point()`/`band_cyc`/stored-`mu` shape). `--musearch 1` is the
  shipped default, `0` the A/B arm.

  20 s of `bioluminescent` (655,360 samples, 1,281 frames), `--rd --budget 400000
  --cap 74 --threads 4`, shipped `--stride 8 --fine 4`:

  | | codec SNR | stream bits | blob | est. cycles max | SHA-256 |
  |---|---|---:|---:|---:|---|
  | ours `--musearch 0` | 39.57 dB | 3,154,706 | 399,736 B | 74.0% | `b32614c3…ab771d` |
  | ours `--musearch 1` | **41.01** dB | 3,154,594 | 399,724 B | 74.0% | `7a63311f…b00ff42` |
  | reference `--musearch 0` | 39.54 dB | 3,154,854 | 399,756 B | 74.0% (verify 74.2%) | `f994a199…e4c139` |
  | reference `--musearch 1` | **41.02** dB | 3,156,681 | 399,984 B | 74.0% (verify 74.3%) | `a3e949fd…9965c1` |

  The mechanism is worth **1.44 dB** here against the oracle's 1.48 dB, and our
  re-coded output sits where the gate asks it to — between our own rate-only output
  and the oracle's (39.57 < 41.01 < 41.02). 278 of 1,281 frames (22%) were
  re-coded, and the `g` range moved the same direction as the oracle's (8..112 →
  8..108). The residual at this budget is **0.01 dB**, but the honest reading is
  not "S6 closed 1.15 dB": on this slice the rate-only search was already 0.03 dB
  *ahead* of the oracle's (39.57 vs 39.54), so the re-code is a mechanism both
  encoders either have or lack, worth ~1.45 dB to either.

  **It does not touch the two lower budgets, and that was measured rather than
  assumed.** `--budget 40000 --cap 74`: the two arms are **byte-identical**
  (39,732 B, `80ab819a…572e51`, 0 frames re-coded) — the cap does not bind at all.
  `--budget 120000 --cap 74`: it binds on **9** frames, the blob moves 4 bytes
  (`17ed982e…6dd87` vs `4a0a224d…0dc11`) and the codec SNR does not — **31.19 dB
  both ways**, which is exactly S4's number for that row. So the 0.16 dB residual
  stays "0.16 dB, unattributed".

  Whole album, `biomaster` (16,553 frames), `--budget 3294000 --cap 74 --threads
  4`: `--musearch 0` → **31.43 dB, 3,293,656 B** (`6f4f3d66…82be51`), `--musearch 1`
  → **31.89 dB, 3,293,696 B** (`f4f3a52a…e095c8`), 1,766 frames (10.7%) re-coded,
  cap 74.0% max in both. The blob **does** change and quality rises 0.46 dB — a
  third of the slice's gain, because the cap binds on a tenth of the frames at this
  rate instead of a fifth. The `--musearch 0` hash is S5's pinned blob, so the
  price is a no-op when it is off, which is the assertion the `mu = 0` guard exists
  for — and which means **S5's gate numbers, `cross` included, are the
  `--musearch 0` arm**; none were re-pinned at the shipped default.

  **It costs 4.1× on a cap-binding album.** Same 20 s slice: 18.1 s rate-only
  against 73.2 s with the price, where the reference goes 7.5 s → 13.9 s (1.9×).
  Every over-cap candidate is re-quantised up to 22 times in both domains, and at a
  binding cap most fine-step candidates are over it. `biomaster`'s 3m46s rate-only
  therefore predicts ~15 min with the price on, and `cross` predicts over two
  hours. That is the same shape of debt as the exact-curve pass already recorded
  under E-R9 ("met in mechanism, not in rationale"), and it is a named follow-up,
  not a claim.

  Four new tests hold the mechanism: the mirrored `CY_*` constants reconstruct
  `Ops::cycle_model()` exactly for a real channel pair, a rising `mu` never costs
  more cycles (the ladder the bisection assumes), an over-cap candidate comes back
  under the cap with a positive `mu`, and the 40,000-budget case is inert. Suite
  and gate: `make uli-test` green at **88 tests**, `ULI_ENC_MIN_TESTS` moved 84 →
  88, `cargo fmt --check` clean. The CLI's extra line — `musearch: N of M frames
  re-coded under a cycle price (mu > 0)` — exists because "musearch on" and
  "musearch did something" are two claims.

- **2026-11-20 — S5: the 43.5-minute album fits a passed-in budget, and the search
  is frame-parallel.** The album is the `cross` corpus (12 tracks, 167,222 frames,
  85,616,716 samples = 2612.8 s), run at `--rd --budget 33292288 --cap 74
  --threads 4` with training on and the shipped `--stride 8 --fine 4`. Both encoders
  were run on the same box, same inputs, same flags:

  | | this encoder | reference (C, `--threads 4`) |
  |---|---|---|
  | wall clock | **40m11s** | 14m30s |
  | user time | 122m | 52m |
  | peak RSS | 1.69 GB | 1.43 GB |
  | blob | **33,291,896 B** (392 unused) | 33,280,720 B (11,568 unused) |
  | SHA-256 | `69911924…e656b8` | `b3c63657…89ae2a` |
  | per-track SNR mean pcm16 / int8 | 31.75 / 28.56 dB | 32.16 / 28.97 dB |
  | pooled codec SNR (ours) | 29.43 dB | — |
  | `g` range | 8..132 | 72..142 |
  | M/S frames | 78.1% | 77.4% |
  | est. cycles mean / max | 67.3% / 74.0% | 67.2% / 74.3% |

  What this establishes: the blob **fits the passed-in budget with 392 bytes to
  spare**, all 12 tracks decode — the verify pass seeks to each track's own preroll,
  so that is a claim about the track table, not only about a linear walk — and
  nothing overflows (`xover 0`, `hfnz 0`). The 0.41 dB pcm16 shortfall against the
  reference is a quality gap, not a fit or validity gap, and it is on a different
  album from the S4 tables' 0.16 dB result, so the two must not be quoted as one.

  What it does **not** establish, recorded rather than smoothed: 40 minutes is 2.8x
  the reference's 14.5 on the same box, so E-R9's *rationale* ("the bake is in the
  edit loop") is not met for an album of this length even though its mechanism
  (frame-parallel compute, ordered emit) now is. The cause is named rather than
  guessed: `measure` codes the frame for **every** candidate — `write_chan` twice,
  the exact-bitstream approach S7's byte gate wants — while the reference's curve
  pass is an estimate. Ours also offers ~54 `(g, ms)` candidates per frame against
  the reference's 28. Making the curve pass cheaper is a new step, not an S5 task.

  Same-day, same flags, on `biomaster` (16,553 frames, `--budget 3294000 --cap 74`):
  moving `Curves::compute`/`refine` from the caller's thread to `--threads` took
  **12m16s → 3m46s** at four threads with a **byte-identical blob**
  (`6f4f3d66…82be51`), CPU time 11m18s → 12m24s. The S4-blob equality is the
  assertion; the CLI's `--fine` default (4) and training default (on) had to be
  restored to get it, and a rewritten parser that quietly moved them changed the
  fixture's blob from 15,664 to 18,331 bits without failing any test.

- **2026-11-19 — the reference encoder reproduces its own corpora, so the
  encoder's acceptance gate is a measurement and not a hope.** Before sizing E1,
  the off-tree reference at `gba-audio/encoder` was built and pointed at the two
  corpora whose bytes this tree already depends on. Both came back **identical**.

  | corpus | command (off-tree) | result |
  |---|---|---|
  | fixture `uli2_tiny` (129 frames, 39,308 B) | `gen_fixture.py` → `uli_enc --album tiny.f32 --tracks tiny.tsv --budget 300000 --threads 1` | `68dca43a…756d` = committed SHA-256 |
  | `biomaster` (16,553 frames, 3,293,048 B) | `uli_enc --album album.f32 --tracks tracks.tsv --budget 3294000 --cap 74 --threads 8` | `990da225…8a73` = committed SHA-256 |

  Both inputs (`tiny.f32`/`tiny.tsv`, and the 67 MB `album.f32` + `tracks.tsv` the
  bake produced on 2026-11-15) were already on disk, so the reproduction needed no
  new tooling — and the second line re-encodes a **real album**, not a fixture:
  16,553 frames, `g range 89..135`, M/S on 87.1% of frames, est. cycles mean 67.9%
  / max 74.0%, 952 bytes of the 3,294,000 budget unused. Its retrain delta was
  −0.03%, against the fixture's −8.05% — the fixture is 2 s of synthetic content,
  so its first-pass tables are far from the trained ones, and a number quoted from
  one corpus does not transfer to the other.

  **Three findings that size the work, in order of usefulness.**
  1. *The gate is reachable.* Byte-for-byte reproduction of the reference on the
     PCM→blob boundary is now an executable check on both a synthetic and a real
     corpus, which is why the fallback in `PLAN-OF-RECORD.md` was demoted from
     "gate" to "a cell of the matrix".
  2. *The reference is thread-stable where we most feared it.* `--threads 1` and
     `--threads 8` produced byte-identical blobs on the fixture, despite the
     reference's `schedule(dynamic, 64)` and a `reduction(+:tot)` over doubles.
     One corpus is not a proof — an unordered float reduction flips only on a
     near-tie, and dense albums have more of those — but it does say R5b is
     insurance, not a rewrite. **Re-run on `adv`/`cross` before trusting it.**
  3. *Two things the earlier plan had wrong.* `musearch` printed no activity on
     either corpus (est. cycles mean 67.9% / max 74.0%, i.e. the `g` allocation
     alone met the cap), so E6 is off the byte-exactness critical path for these
     two — not for albums in general. And the resampler exemption in the plan was
     justified by the wrong mechanism: `prep.py` does not use `ffmpeg`'s resampler
     but `gba_dac_model.resample`, which dispatches on FFT / `soxr`-if-installed /
     `scipy`, so byte-exactness across machines depends on which library answers.
     Hence the gate now sits at the PCM boundary, where it was measured.

  **Measured in passing, and it corrects a claim made earlier today.** Comparing
  the whole tables region across the five corpora: it is **5,212 bytes in every
  one**, with **five different hashes** (FNV-1a `0f5e1b96…` fixture, `89c4eb31…`
  biomaster, `10ac765c…` biotwo, `64809502…` adv, `93ef18f5…` cross). So
  "identical tables across corpora" — phrased that way in `premise.rs`'s header and
  in PLAN-OF-RECORD before this entry — is wrong: what is identical is the **class
  structure**, and what differs is the trained length rows. Against the reference's
  `tables_default` the structure matches on `ctype` and `rbits` in all 14 classes,
  and differs in **one cell**: `esck[2]` is `2` in the default and `0` in every
  blob, i.e. training removed an escape the default declared. That is the safe
  direction, and it is the evidence for a rule the encoder needs: `esck` is
  **derived**, so E-R4 must be checked against the final trained tables rather than
  assumed satisfied because the defaults obey it.

  **Caveat, stated once here so it does not have to be repeated:** the reference
  compiles with `-O2 -march=native`, so these hashes are host-qualified — they are
  reproducible *on this machine*, which is the machine that made them and the one
  the gate runs on. It is not a cross-platform claim about the C encoder, and the
  Rust encoder keeps libm out of its own path for exactly that reason (R5c).

- **2026-11-18 — `post` as ARM asm: the last Rust kernel on the product path is
  now hand-written, for −3,061 cycles on every frame.** Fresh attribution with the
  new **bit-exact** arms put `post` at 23,682 cycles at the gate frame / 23,458
  mean (11.3% / 12.9% of the frame) against the C record's 21,800 — and it was the
  one kernel whose excess was *nameable in instructions*: **18 per iteration
  against the reference's 14**, every extra instruction addressing (the `z` pair
  and the twiddles each cost an `add`+`ldr` or two `ldr` where the reference does
  one `ldmia`; the backward `Y` store cost a computed address where the reference
  post-indexes). `asm/post_arm.s` is that 14-instruction loop, landed as `asm-post`
  (default on) with the Rust `post` kept as the in-image control and de-placed
  from `.iwram` per the 1e-i rule. Measured, bit-exact (both album pins on every
  corpus, `postasm=true` twin PASS, two runs identical to the cycle): **129-frame
  208,854 → 205,793 worst / 182,119 → 179,058 mean**, **album 209,162 → 206,101 /
  178,341 → 175,280**, **gate total 240,705 → 237,646 (91.82% → 90.65%) /
  209,721 → 206,637 (80.00% → 78.83%)**, margin **8.18 → 9.35 points**, and on the
  `cross` corpus — the one C's own record was measured on — **210,209 → 207,148
  worst / 174,009 → 170,939 mean, i.e. 1.155x → 1.139x worst and 1.137x → 1.117x
  mean against C**. The win is **−3,061 on every frame, worst and mean alike**,
  and that uniformity is the point: `post` runs a fixed 512 iterations per frame
  regardless of content, so this is a *content-independent* saving, unlike the
  multiply-bound kernels whose cost tracks the data.
- **2026-11-18 — the kernel table is re-derived with bit-exact arms, and three
  "deficits" turn out to be measurement artifacts.** Every by-difference arm
  worked by *removing* a stage, so every later stage multiplied different data and
  the arm charged the downstream change to the part it removed. Measured directly:
  `pre`'s delta was **1.61x too large at the densest frame** (31,150 vs a true
  19,336), and the inflation tracked frame density exactly. Three arms now *add a
  second identical instance* instead of removing one — `attrib-pre2` (idempotent),
  `attrib-post2` (idempotent), `attrib-entropy2` (replay `dec_chan` from a saved
  `BitReader`, which is `Copy`) — so downstream data is unperturbed and the pins
  still PASS. Clean result at f83: **fft 68,694 (32.9%, derived) / entropy 61,751
  (29.6%) / ola 30,521 (14.6%) / post 23,682 (11.3%) / pre 19,336 (9.3%) / ms+wrapper
  4,883 (2.3%)**, closing to −2.4% of `synth` (the old table over-counted by
  +5.9%). **Corrections: `pre` is 1.00x C at the gate frame and 0.91x on the mean;
  `ola` is 1.00x / 0.98x; ms is 0.64x on the mean — three of six parts are at or
  ahead of C.** Retired with it: the file's `pre 1.22x/1.23x/1.36x/1.61x` ladder
  entries and the claim that `pre` was "the worst kernel ratio left". **A limit
  the numbers impose, recorded so no one re-derives it the hard way:** because the
  audit put the data in `Rs`, kernel cost now varies with content by *more* than
  the gaps being judged (`pre` 1.20x, `post` 1.15x, `fft` 1.13x, `ola` 1.08x,
  entropy 26x across our own frames; C's entropy row sits at our 43rd percentile).
  Single-frame ratios cannot separate a deficit from a frame-mix difference — only
  an all-frames mean on the same corpus can, which is why the `cross` row is the
  one to quote.

- **2026-11-17 — the frame pointer is gone: the GBA build target is now a
  checked-in spec, and it is worth 0.6 points of the frame.**
  `targets/thumbv4t-gba.json` is the pinned toolchain's own
  `--print target-spec-json` for
  `thumbv4t-none-eabi` with exactly one field changed, `frame-pointer` `always` →
  `may-omit`; `.cargo/config.toml` builds for it, `make target-spec-check` fails
  if any other field drifts (including a field appearing on one side only), and
  `targets/README.md` carries the provenance, the regeneration recipe and the
  plumbing traps. Measured bit-exact: **gate total 242,370 → 240,705 worst
  (92.46% → 91.82%) / 210,626 → 209,721 mean (80.35% → 80.00%), margin 7.54 →
  8.18 points**; album decode wall **210,787 → 209,162** worst and 179,226 →
  178,341 mean; 129-frame 210,833 → 208,854 / 183,241 → 182,119. **13 → 0** of 210
  functions set up a frame and `.iwram` code drops 896 B; r11 becomes an ordinary
  data register (the four asm kernels always used it as one). **Decision — the
  spec is the instrument, so it is reviewed and witnessed like the toolchain
  pin**, not flipped by a flag: `-Cforce-frame-pointers=no` is **inert**
  (byte-identical ROM rebuilt from a clean `target/` both ways), which is why the
  change could not be a flag in the first place. Two negative results recorded:
  the rustflag does nothing, and `frame-pointer: "non-leaf"` — the plausible
  middle setting — would have helped **nothing** because `dec_chan` is non-leaf
  (ten `bl` sites), so only `may-omit` frees r11 where it matters. The pin's
  stated purpose does not apply on this target (`panic-strategy: abort`, no
  unwinder, no `RUST_BACKTRACE`; mGBA debugs from DWARF), which is the
  justification and is now written down. **Known cross-track consequence, recorded
  not hidden:** `flac-spike`'s PR 3 verdict moves from `measured 2/2` to
  `measured 0/2` because its calibration witness demands five *identical*
  empty-window samples and now sees a 2-cycle first-sample difference
  (`22,22,22,22,22` → `20,22,22,22,22`). Decode is untouched and `make flac-test`
  passes; no `make` target gates that verdict. Re-baselining it is the FLAC
  track's own change.

- **2026-11-16 — loose-ends sweep landed (audit D + E): the gate total is now
  92.46% worst / 80.35% mean, and the category's ceiling is lower than projected.**
  Two items landed: the reader's **single `ldrh`** (previously two `ldrb` + `orr`
  + `lsl` on the most-executed path in the decoder) and **`zero16` as four inline
  `stmia`** (previously a `bl __aeabi_memclr4` call, ARM→Thumb veneer included).
  129-frame corpus **185,207 → 183,242 mean (−1,965, −0.75 pts)**, album corpus
  **180,921 → 179,226 mean (−1,695)** with worst 211,050 → 210,787, and the **gate
  total 251,712 → 242,370 worst (96.02% → 92.46%) / 221,245 → 210,626 mean (84.40%
  → 80.35%), margin 3.98 → 7.54 points**, zero starved periods. `dec_chan` 1,394
  → 1,240 instructions, stack references 259 → 174. **With it, the `ldrh` needed
  a real contract, not a cast**: `include_bytes!` gives `&[u8]` no alignment
  guarantee and the embedded albums were measured landing on an **odd address**,
  so odd bases are the normal case — hence `uli_lite::Aligned` (13 embed sites)
  and `Error::UnalignedStream`, checked once at open and witnessed by a test that
  an odd-based blob is *rejected*, not decoded into UB. **Decision — three items
  measured and NOT taken, recorded so they are not re-attempted:** (i) the
  `refill` bounds branch, removed via a branchless clamped load, measured
  **+1,352 cycles/frame** — the load address then depends on `end`, which costs
  more in the critical path than the 3-cycle taken branch it saves; folding `bad`
  into a per-refill OR was worse again. (ii) `huff_consume`'s length test — every
  failure-signal design either forces `bad` into memory (the Step-1a trap) or lets
  a malformed stream spin, for ~1 cycle/symbol. (iii) the `esc` guard, which is
  per *escape* and needs a parser bound change. **The general lesson, and it
  changes how this file should read the C-vs-us gap: almost every item in the
  "loose ends" category is a *branch*, ARM7TDMI charges 3 cycles for a taken one,
  and Rust cannot emit ARM conditionals (no `it` on v4T) — so what looked like
  instruction slack is the target's lack of predication.** The category delivered
  1,925 mean and has ~2,000 left, not the 3,500-5,500 projected.
- **2026-11-16 — root-cause audit against the C product decoder; the FFT
  multiply operands and the M/S kernel landed for −9,282 mean / −8,896 worst.**
  The audit read `gba-audio`'s decoder and asm line by line instead of comparing
  against its summary table, disassembled our `.iwram` from the ELF, and
  measured. Frame **219,251/194,489 (83.64%/74.19%) → 210,355/185,207
  (80.24%/70.65%)**, ratio to C **1.205x → 1.156x**, both pins PASS and
  `msasm=true` twin PASS, two runs identical to the cycle, `.iwram` 27,308 B with
  an 844 B measured stack. **(A)** The FFT's six `smull`/`smlal` had the twiddle
  in **Rs** and the data in Rn — the reverse of the convention the C source
  spells out (multiply cost early-terminates on Rs) and prices at 78.1% → 73.1%
  in its own record; with `TW` ~2^30 every FFT multiply ran at the slow end. All
  of v1-v4 were reversed, and v5's "arithmetic identical to v1..v4" header was
  true of the products and false of the timing. Swapping the operands is
  bit-exact by construction and worth **−6,993 mean**. **(B)** M/S had **no
  hand-written kernel at all**; C's is two `ldmia` + 8 ALU + two `stmia` per four
  coefficients (14 per 4) against our scalar loop's 9 per 1. `asm/ms_arm.s`
  landed as `asm-ms` (default on) with the Rust `ms` kept as the in-image control
  and a new on-target twin: **−2,289 mean**. **Decision — aggressive targeting,
  re-cut order.** The audit's two items returned more than the previous six
  micro-steps combined, and the record's own "the curve is flat, ~1%/round"
  conclusion is withdrawn as a statement about the *decoder*: it was a statement
  about tuning loops that already matched the reference. The order of work is
  now five named items (audit A-E) instead of a block called "entropy", the
  M/S ceiling of "~1,500-2,500" is corrected (missing kernel, ~3,300 by
  instruction count), and the frame pointer is recorded as a dedicated open item
  rather than an accepted cost. The gate total **must be re-measured** (`make
  uli-gate`) before U4 is quoted again: PR E's 96.02%/84.40% was taken at a
  74.2% mean decode and the expected figure is ~92.5%/81%.
- **2026-11-16 — the frame pointer is a known deficiency, not a settled trade.**
  `"frame-pointer": "always"` in the `thumbv4t-none-eabi` target spec pins r11 in
  every Rust function; gcc at `-O2 -marm` omits it. That is **one fewer register
  than C + asm everywhere**, plus fp-relative addressing, and it is why 1e-ii's
  "register relief in `dec_chan` does not pay" is now read as an experiment whose
  hypothesis the toolchain had already disabled. **Not given up on**: the lever
  is a custom target JSON (`frame-pointer: "non-leaf"`), which changes the
  instrument and therefore needs a control build, a section/symbol dump, and an
  answer on stack unwinding first — this tree measures stack high-water with a
  canary (844 B), not an unwinder, so the constraint may be unnecessary here.
  Explicitly **not sized** until the entropy contract sweep lands, because the
  two levers overlap. Tracked with U6.
- **2026-11-15 — Step 1e landed: budget work, and the register hypothesis
  falsified.** Three arms. (i) The Rust control kernels (`pre`, `ola_s8`,
  `ola_s16`) left `.iwram` — their placement is now conditional on the asm
  feature being *off*, the gate `fft` has carried since C-1 — for **−496 B and a
  byte-identical frame**; `post` and the dual `ola` stayed because they *are*
  shipping paths (there is no asm `post`, and the dual `ola` is the int16
  profile). (ii) Register relief in `dec_chan` was **tried and rejected**: a
  `bad`-into-`valid` sentinel cut `dec_chan` from 1,610 to 1,192 instructions
  (−1,672 B) and cost **+1,450 cycles mean**, and adding derived `esck`/`d15`
  made it **+3,351** — because indexing `tables` in those branches resurrects it
  as a live value across the whole loop, the exact pressure the change was meant
  to relieve. Reverted; the register-pressure hypothesis is falsified for this
  kernel, the second such falsification after the FFT's v1. (iii) The cursor
  rewrite landed: pointer loops and a `x + NCODED` compare in place of counters
  for **−856 B and −665 cycles mean / −2,081 worst**. The mean is the honest
  comparator (worst frame moved f93 → f125), and 665 is a third of what `pre` and
  `ola` got from the same shape because `dec_chan`'s stores were already
  index-scaled. Net: **`.iwram` 28,632 → 27,280 B, headroom 828 → 2,196 B**, frame
  **221,332 → 219,251 worst / 195,154 → 194,489 mean = 83.6% / 74.2%**, ratio to C
  **1.22x → 1.21x**. Data (16,864 B) is now the majority of the audio budget for
  the first time. The `verify` image is 232 B short of fitting — that is a
  decision (the dual `ola`, 344 B) and not taken here.
- **2026-11-15 — Step 1d landed: `ola` at C parity, and the <=85% headroom
  target is met.** Our fused int8 loop was **33 instructions** per iteration
  against the reference's **26**, and the excess was the clamp (8 per output
  against 6 — the reference saturates to int16 with `mov`/`teq`/`eorne` and then
  shifts right 8, which is the same function as `sat8` because
  `sat8(x >> 8) == sat16(x) >> 8`) and the addressing (7 instructions of index
  arithmetic against six post-indexed cursors plus a `Y_minus_P` constant for the
  overlap read). Rewritten in that shape (`asm/ola_arm.s`, a `OLA_FN mode` macro
  generating both output formats, 160 B each, **26 instructions**): **34,375 →
  30,539 at f98** (−3,836, exactly the frame delta) = **15,270/channel = 1.00x
  C**, the first kernel at parity. Product frame **225,168 → 221,332 worst (85.9%
  → 84.4%)** / **198,992 → 195,154 mean (75.9% → 74.4%)**, ratio to C **1.24x →
  1.22x**, cumulative **−52,866 (−19.3%)**. With three of four kernels at 1.00x-
  1.23x the gap to C is now 62% entropy / 37% synthesis, so the next item is
  inside entropy. Two self-inflicted bugs recorded because both were caught by
  witnesses before any number was quoted: using `H` where `M` belongs ran the
  loop 128 iterations instead of 256 and made the frame look 18,900 cycles
  faster (the album pin caught it), and `add r6, r2, #511` is not an encodable
  ARM immediate. `make uli-kernel-twin` now builds `asm-fft,asm-pre,asm-ola` and
  requires `fftasm=true preasm=true olaasm=true`, with the `ola` twin replaying
  both formats from the same saved overlap state because OLA *mutates* `p`.
- **2026-11-15 — Step 1c landed: `pre` in the reference's cursor shape, and the
  instrument falsified its own projection.** The plan said `pre` (1.36x C, the
  worst kernel ratio) was an addressing win like the FFT's. It was not: LLVM's
  loop was **15 instructions per output pair with zero spills and folded
  addressing** — nothing to remove — while the reference's is **10**, because it
  keeps four post-indexed cursors and stores a whole pair with one `stmia`
  instead of a computed backward index, a split pre/post store pair and a
  3-instruction loop tail. Rewritten in that shape (`asm/pre_arm.s`, 76 B, 10
  instructions, no address arithmetic): **26,458 → 23,889 at f98** (−2,569,
  exactly the frame delta), **11,945/channel, 1.36x → 1.23x** C, and the product
  frame **227,736 → 225,168 worst (86.9% → 85.9%)** / **201,554 → 198,992 mean
  (76.9% → 75.9%)**, ratio to C **1.25x → 1.24x**. Recorded with it: the four
  multiplies are irreducible and non-overlappable on an ARM7TDMI, so the
  instruction count *around* them was the only lever — and once the shape
  matches the reference's, the residual 1.23x is not code structure, so the C
  row must not be read as 23% of available headroom. `make uli-fft-twin` became
  **`make uli-kernel-twin`** (builds `asm-fft,asm-pre`, requires
  `fftasm=true preasm=true`): a second asm kernel showed the witness had been
  narrower than the claim, the second time that happened this session.
- **2026-11-15 — Step 1b landed: FFT v5, 1.14x C, and the frame at 86.9%.** The
  kernel took the reference's address shape — an **absolute byte cursor** with
  the stride in a register, a **register-held twiddle pointer** (possible only
  because an absolute cursor does not need the buffer base in the loops), and the
  **destructive sum formation** that derives the difference from the sum with a
  shifted operand, so 8 instructions with no moves instead of 12 with four.
  Measured **81,035 → 71,519 at f98** (−9,516, exactly the frame delta),
  **35,760/channel = 1.14x C**, and the product frame **237,252 → 227,736 worst
  (90.5% → 86.9%)** / **211,071 → 201,554 mean (80.5% → 76.9%)**, ratio to C
  **1.30x → 1.25x**, pins unchanged. v5 is **764 B of `.iwram`**, 192 B smaller
  than v4, and v4 is demoted to `.text` as a comparison arm — where it measures
  179,694 cycles/call against v5's 57,094, a 3.1x placement penalty that must
  **not** be quoted as a v4-vs-v5 comparison. Two instrument defects found and
  fixed on the way: `ovl twin ... fftasm=true` had been **vacuous** in
  `make uli-bench` since PR C-1 (the twin is behind the bench's `asm-fft`
  feature, which that target does not enable), and the image that *does* run it
  could not fit until `TW_IWRAM` (2,016 B, comparing the table with itself since
  `iwram-tables` became the default) was gated. `make uli-fft-twin` is the new
  gate. Recorded for the next asm: **`smull` writes a register pair**, so an
  output address held in `r0` is destroyed by the real half of a complex
  multiply — the first v5 draft did that and stored every real part to a pointer
  built from a product's low word (486 of 512 coefficients wrong, and
  diagnostically *only* the real parts). v4 holds its address in `lr`; so does
  v5. Found by transliterating the kernel into a host test that reproduced the
  target's first divergent index exactly.
- **2026-11-15 — Step 1a landed: the bit window is in registers, and synthesis
  becomes the critical path.** `dec_chan` now destructures the reader into five
  `let mut` scalars and `refill`/`take`/`top`/`esc`/`coef` are **macros over
  them** — the C reference's `REFILL`/`SKIP` shape, and the only Rust shape that
  did not put the 24-byte struct back in an alloca (three ABI/promotion attempts
  had failed). The pure decoders take `hw` by value, so no `&mut BitReader` is
  formed on the hot path. Measured: **249,835 → 237,252 worst (95.3% → 90.5%)**
  and **220,757 → 211,071 mean (84.2% → 80.5%)**, −5.0% / −4.4%, pins unchanged,
  both twins bit-exact; **1.37x → 1.30x** the C record, and **−36,946** off the
  worst frame across Steps 1 + 1a. The disassembly confirms the point: `hw` is in
  a register and the `bad` byte is stored 38 → 6 times. Two side findings
  recorded: `eg_slow` was 1,820 B of `.iwram` (it inlined `peek`/`skip`/
  `set_pos` at every step) and is 968 B when it walks bits with the hot
  primitives — moving it to the cartridge instead measured **+22,657**, so "cold
  path, put it in ROM" is wrong here by ~10x. And the `attrib-entropy-reader`
  floor is unchanged at 4,801, because that arm was already register-resident:
  it is a floor on the mechanics, **not** a measure of the struct-ABI penalty.
  With the deficit now 42% entropy / 57% synthesis, **FFT v5 becomes item 1**.
- **2026-11-15 — Step 1 landed: the entropy rewrite takes the decode wall under
  the frame period.** Instrument first, as the plan required: five timing-only
  arms (`make uli-attrib-subframe`) partition the entropy path at the gate frame,
  and they said the dequant arithmetic is **13%** and the reader *mechanics*
  **5.5%** — so the cost is symbol resolution and the per-coefficient traffic
  around it, at ~154 cycles per Huffman symbol for an `lsr; ldrh; cmp; shift`.
  The rewrite then: reader by value in and out of `dec_chan`, a refill contract
  guaranteeing **17** valid bits (so `skip` is branchless), a long-code search
  that reads the window instead of `peek`ing up to sixteen times, the sign off
  the window's top bit, and one `&Huffman` + LUT pointer per table instead of a
  ten-word `HuffmanParts` (`dec_chan`'s stack accesses 218 → 108). Measured:
  entropy path **111,227 → 86,826 at f98 (−22%)**, product frame **274,198 →
  249,835 worst (105% → 95.3%)** and **232,090 → 220,757 mean (89% → 84.2%)**,
  ratio to C 1.51x → 1.37x, pins unchanged. `iwram-tables` untouched; `.iwram`
  28,708 B with 1,612 B free above the 820 B stack. Recorded with it: the
  attribution's shares moved (synthesis is 65% of the frame and the gap to C is
  now ~58/42 entropy/synthesis, not 69/31), three failed experiments (the reader
  ABI, a local-copy promotion attempt that cost 788 B and overflowed the stack,
  and inlining `dec_chan` which got outlined into ROM for 3.0x worse), and the
  C-style `m == 1` hoist which measured worse on the gate metric and was not
  taken. **The reader is still in memory** — the reference gets its window into
  registers because `REFILL`/`SKIP` are macros over locals, so that is item 1a.
- **2026-11-15 — Step 0 done: the frame is attributed, and it inverts the order
  of work.** `make uli-attrib` measured the product frame by difference in one
  image, per-frame aligned: synthesis **160,268** mean / 162,987 at the worst
  frame (f98), entropy+dequant **66,943** / 101,122, M/S 4,413 / 9,619, wrapper
  481. In-frame the kernels are **1.10–1.33x C**, not 2.4x — the pre-C-3
  readings were taken on the bench's EWRAM buffers, not the product's IWRAM
  `HotState`, and overstated every kernel by 21–33%. The already-asm FFT is
  **41,671/channel (1.33x C)** and carries 20,541 of the 25,668-cycle synthesis
  gap; `pre`+`post`+`ola` are 9,817 together. On the gate frame,
  entropy/dequant/M/S is **110,741 = 40% of the frame and 69% of the 92,286-cycle
  distance to the C worst-frame shape**, so the entropy rewrite is now item 1
  and the three Rust kernels are item 3. Retired: "`pre` asm is the largest
  single headroom (~15,900)", "the FFT is 1.97x and near its floor", "the M/S
  loop costs 16,067". Method recorded in the ledger: timing-only cargo features
  (`attrib-*`), pins fail by construction, `attrib-dump` costs a constant +16
  cycles per window, and `attrib-entropy` alone is not quotable because the bit
  position freezes.

- **2026-11-14 — PR C-1 landed: hand-written FFT, measured as a ladder, wired
  into the product.** The FFT started as v1 (a careful hand allocation) at
  88,205 c/call — *slower* than the Rust ARM kernel's 85,065. That falsified
  the register-pressure premise: it is not spills but **memory traffic**.
  Removing the spill/reload traffic (v2, 73,363) and adding `ldmia`/`stmia`
  block transfers (v3, **71,635**) delivered **−18.8% from v1 and −15.8% vs
  Rust**, all bit-exact. Routing `GbaDecoder`'s synthesis through the asm FFT
  took the **full product frame 457,863 → 430,997 (175% → 164%)**, i.e. the
  gains are now real product-path gains, not just `ArmKernels`. Method kept:
  one lever per arm, same image, always a Rust control, always twinned on the
  ARM7TDMI. Still 2.28× the C record; the next research order (trivial
  butterfly, twiddle `ldmia`, then the multiply/memory floor, then a
  like-for-like corpus) is written in the “PR C-1 landed” ledger section. The
  remaining stack gap to C is not accepted as inherent: nothing about Rust+asm
  forces it, so it is scoped as continued work.

- **2026-11-15 — C-3 landed: `HotState` in IWRAM, the largest single win.**
  Moving the decoder's 8,192 B hot working set (`x`/`buf`/`p`, exactly the three
  buffers C keeps in IWRAM) out of EWRAM took the production frame **361,238 →
  274,198 (138% → 105%)**, **−87,040 (−24.1%)**, mean 232,090 (89%, now under the
  period), and **1.99× → 1.51×** the C decode wall. **The projection was
  10,000–15,000; the measurement is 87,040 — ~6x** — because the projection
  counted only the M/S pass while every coefficient access through those buffers
  was paying EWRAM wait states. Paid for by moving the Rust `fft` to ROM when
  `asm-fft` is on (it is not on the product path) and the superseded v1/v2/v3 FFT
  kernels to ROM, which is the general rule this work keeps teaching: **spend
  IWRAM on what the product actually executes.** `.iwram` 28,852 B of 32,512;
  usable stack from the BIOS top is 3,660 B against a ~2,960 B high-water, so
  ~700 B margin — usable here, below C's own ~1 KiB assert, and the full product
  ROM must re-check it. Also corrected the stack record: the earlier "768 B" was
  the descent below `run()`'s frame only, not the total from the stack top.

- **2026-11-15 — C-3d landed: OLA was measured in a configuration C never ran.**
  The `ola` kernel selected its output format *per sample* and computed both
  formats whenever an int16 buffer was passed — and the bench passed one, to pin
  both hashes. So the 2.28× OLA ratio the plan carried was partly a measurement
  artifact. Fused per-format kernels (`ola_s8`, `ola_s16`, format fixed outside
  the loop, C's own structure) plus an `ola_kernel` dispatcher measure the
  production int8 kernel at **21,418/call = 1.40× C** (dual 34,780 = 2.27×;
  s16 23,220 = 1.52×), shrinking OLA's headroom ~69%. It also found a real win:
  the product was writing int16 it never uses, so the int8-only pass takes the
  frame **387,878 → 361,238 (148% → 138%)**, mean 340,928 → 314,288, with the
  same int8 pin (`e6e87652`). Ladder: 175% → 148% → 138%, **1.99× C**. Recorded
  with the lesson that recurred three times in this work: *measure the
  configuration that ships*, not the one that is convenient to pin.

- **2026-11-15 — the fast path is the DEFAULT, and "C-class parity" is retired.**
  Two decisions. (1) `uli-lite-gba` now defaults to `["asm-fft",
  "iwram-tables"]`: the measured wins (v4 FFT + all synthesis tables in IWRAM,
  the C layout) are the product path rather than opt-in flags, so the **default
  build is 148%** (387,879) instead of 175%, with 8,212 B of `.iwram` headroom
  after the 768 B measured stack. The control is not lost: `frame core` (the
  portable/Thumb decoder) and the isolated `kern`/`ovl` arms still measure the
  Rust kernels in the same image, and `--no-default-features` gives the
  pure-Rust-kernel build. (2) **The 69.4% figure is re-read as a compute-budget
  existence proof, not a comparison to match.** C+asm runs this codec in real
  time on this exact hardware, which proves the *work fits the machine*; that is
  the fact we need. "C-class parity" is therefore retired as a gate — there is no
  reason Rust+asm should lose to C+asm here, so the target is U4/U5 (real time
  with gate/UI headroom), with the C figure kept only as a diagnostic for where
  time goes and as a rough shape of a solved implementation.

- **2026-11-15 — memory layout is read from C, and the stack figure was stale.**
  Placement is now derived, not guessed: C's linker script puts plain statics in
  **IWRAM** (`.data`/`.bss` both) and only explicit `.ewram` in EWRAM — the
  opposite of our agb default — and C's decoder uses ~no stack because every
  working buffer is a static. Profiling C's working set (`uli_g_X/buf/P` 8,192 B,
  five tables 8,672 B, state ~600 B, ~5 KB ARM code, ~100 KB Huffman in EWRAM)
  gave the target directly. Measuring our own stack with a sentinel probe found
  **768 B, not the ≈15.0 KiB this file had carried since PR B** — that figure was
  an artifact of stack-local twin buffers, already fixed in PR C-1 but never
  re-measured, and it had been *blocking* the tables-in-IWRAM item. With the real
  number, `iwram-tables` (C's own table layout) runs and measures the product
  frame at **148%** (387,879 / 340,929) with 6,196 B of `.iwram` headroom, and
  C-3 (`HotState` → IWRAM) is unblocked (fits once the superseded v1/v2 kernels
  are dropped). Lesson recorded next to the placement findings: a stale number in
  a decision record is a blocked work item.

- **2026-11-15 — PR C-1 follow-up: the twiddle table's location dominates.**
  Investigating the planned "twiddle `ldmia`" step revealed the actual cost: with
  `iwram-tables` off, `TW` lives in `.rodata` = cartridge ROM, so every hot
  twiddle load paid ~6 ROM wait-state cycles. A same-image A/B (same kernel,
  only the table pointer changed) measures **69,034 → 61,852 cycles/call
  (−10.4%)** for moving the table to IWRAM; the `iwram-tw` feature delivers it
  and takes the product frame **425,795 → 411,431 (162% → 157%)**. FFT now
  **1.97× the C record** — under 2× for the first time. Recorded because it
  revises the "why slower than C" answer: table placement, not codegen, was a
  large share. `iwram-tw` is opt-in: it costs 2,016 B of `.iwram` (margin ~600 B
  vs the ~15 KiB product stack), and making it the default waits on the U6
  budget (or dropping the superseded v1/v2 kernels to free 1,520 B).

- **2026-11-15 — PR C-1 follow-up landed: the trivial butterfly (v4).** C's
  `TRIV_BFLY_P` was the last structural difference in the FFT, and it is now
  taken. At j==0 of every stage the twiddle triple is exactly (2^30,0)×3
  (verified for l=256/64/16), so `cmul(y,1.0) == y>>2`: the three twiddled
  outputs are sums shifted right 2, with no multiplies and no twiddle loads.
  `fft_arm_v4.s` measures **69,034 cycles/call — −2,601 (−3.6%) vs v3**,
  larger than the ~1,000 the ledger projected, because the path also drops the
  per-output twiddle-pointer load and all six twiddle loads. It covers 21 of
  the 192 radix-4 butterflies (an earlier note's "3/192" counted j-iterations,
  not butterflies). Wired into `fft_kernel`, the product frame goes
  **430,997 → 425,795 worst (164% → 162%)**, the delta being exactly 2× the
  per-FFT saving. Ladder: 85,065 → 88,205 → 73,363 → 71,635 → **69,034**,
  2.20× the C record. U4 still open; next is twiddle `ldmia`, then the
  multiply/memory floor, then a like-for-like corpus (PR E).

- **2026-11-14 — C-3b landed; the prediction held.** The counters and format-rule
  diagnostics moved behind `uli-lite-gba`'s `verify` feature: the product build
  measures **457,863 worst / 410,912 mean (175% / 157%)**, **−30,822 / −18,427**
  against the attribution's predicted −30,482 / ~20,000 — the first projection in
  this file to land on its number. The `verify` build reproduces the previous arm
  within 3 cycles, so it is a switch rather than a rewrite. The product decoder is
  no longer the arm that witnesses `xover=0 hfnz=0`: that stays with the core
  decoder's pass in the bench and with the `verify` twins
  (`make uli-overlay-test` now runs both builds, and a guard test fails if the
  counters come back outside the gate). Ladder: 2526% → 1133% → 698% → 204% →
  186% → **175%**. Next is coefficient buffers to IWRAM with C-1.

- **2026-11-14 — the attribution is built, run, and it reorders the work.** Two
  instruments: a per-frame dump in the bench (`--features per-frame`, which
  reproduces the gate arm exactly, so it does not perturb the measurement) and
  by-difference arms. The 129 rows fit a per-event model at R² = 0.9994 (residual
  sd 1,170 cycles); the notable coefficients are pair 302, quad 341, escape 134,
  raw field 57, long code 221, M/S frame 18,124. Findings: **the diagnostics are
  the biggest item (~30,000)**; **M/S costs 16,067 but a four-coefficient block
  rewrite recovers only 1,776**, so the cost is EWRAM traffic, not loop shape —
  the reference's coefficient buffers are in IWRAM; the dequant arithmetic is
  only ~4,000 (its row is mostly the `xover` compare); and the fills (~7,400
  mean) are required because `pre`/`post` read all `M` coefficients. The
  band-symbol term is **not identifiable** (collinear) and is not quoted. Work
  order becomes C-3b → IWRAM coefficient buffers (with C-1) → C-1 → OLA, and the
  remaining gap is now explained rather than mysterious: diagnostics plus
  memory placement, not an unexplained per-symbol cost.

- **2026-11-14 — C-2 landed as C-3a (+C-3c), and it was measured, not assumed.**
  The structural port and the 32-bit dequant measured **488,685 worst / 429,339
  mean (186% / 164%)** against a re-measured `main` baseline of 523,267 /
  456,187 — **35,000 saved where the plan projected 90,000**. Consequences
  recorded: the counter-hoist idea was built, measured **worse** (+21,000,
  register pressure) and rejected; C-3b is the largest measured lever left
  (−11,000 alone, −30,000 with the diagnostics); and the item list now projects
  95–107%, so **parity needs a per-stage attribution of the remaining ~140,000
  cycles**, which the plan did not previously contain. All twins and pins green
  (`xover=0 hfnz=0`, overlay twin PASS).

- **2026-11-14 — the eight open questions are answered; scope is frozen for v1.**
  Recorded as one block because they interact:

  - **asm provenance.** ARM-mode asm may be adapted from the reference; **C
    source may not be incorporated**, and there is **no build, link or vendoring
    dependency on `gba-audio`** — every artefact lives in this repo and is
    twinned against our own oracle. This unblocks C-1; the license consequence
    stays deferred with `publish = false`.
  - **U4 ceiling.** Not an inherited or assigned number: it is **measured** at
    the first near-real-time arm, from the measured gate total plus this tree's
    non-audio cost. Until then 73–81% is a projection, not a grade.
  - **Corpus.** `uli2_tiny` (129 frames) is the iteration instrument, deliberately:
    at multiples over budget, reaching a realistic regime beats worst-case
    fidelity, and full-album runs slow the loop. The comprehensive worst-frame
    analysis is **PR E**, deferred to the real-time regime, FLAC-style.
  - **Output regimes.** Speaker profile **in scope**; the decoder is always
    stereo and the mono collapse happens in the output stage (never one IMDCT).
    Fold-down method — `(L+R)/2` vs single-channel — is priced at PR D; they are
    not equivalent, and single-channel is accepted as the cheap option if the
    measurement supports it.
  - **U6 priority.** Audio first: IWRAM goes to code, hot state and stack, then
    synthesis tables if the stack allows; UI affordances are the slack.
  - **U7 licensing.** `publish = false`; no license work now.
  - **Malformed streams.** No promise: this project controls its own encodes.
    Rejection-only stays (sentinel, no panic/hang); C-3c's i64 path moves to the
    verifying build.
  - **Format scope.** v1 is fixed (32,768 Hz, hop 512, stereo, no long-block);
    the target is matching the C project's fidelity here, not new rates. The
    Rust encoder stays deferred with the same no-C/no-dependency rule.
    **Superseded in part (2026-11-19):** the encoder is *scheduled*; the fixed v1
    format and the no-C/no-dependency rule stand. See
    [`PLAN-OF-RECORD.md`](PLAN-OF-RECORD.md) → "The encoder — the last missing piece".

- **2026-11-14 — stereo is the product target (mono risk closed).** The decoder
  and the playback path are stereo, aimed at the headphone output; fidelity is
  the project's purpose and is not spent to buy CPU headroom. Hardware note:
  the GBA speaker is mono, the headphone jack is stereo, so the speaker profile
  is served by an **output-stage** mono fold-down over a full stereo decode —
  the decoder never collapses, and C's `ULI_GBA_MONO`/`ULI_OUT_M16` one-IMDCT
  path is not taken. U4 is read against the two-channel frame, and both output
  regimes are measured. Supporting evidence: C's headline 69.4% decode
  wall is itself stereo (its synthesis loop runs twice per frame); the mono
  speaker profile measured 84.2% worst / 77.3% mean because the collapse lived
  in the driver (13.65%/frame), and that output is driver-defined and was never
  hash-checked. Same day, the C kernel-pair row was pinned at **134,600** (C's
  own `pre 9.7k + fft 31.4k + post 10.9k + ola 15.3k` per channel), striking PR
  C's ~146,000; C-1's prize is therefore ~194,000 and the revised projection is
  **~193,000–213,000 (73–81%)** — parity plausible at the optimistic end, not
  proven.
- **2026-11-14 — C-1 + C-2 are insufficient for the target; PR C-3 proposed.** A
  parity audit read the C product decoder
  (`gba-audio/src/uli/gba/uli_gba_dec.iwram.c`) against our hot path and found
  ten differentials the plan had not scoped: a `q3[81]` quad-expansion table
  (we divide per quad), a per-class `rq[16]` dequant table plus one multiply (we
  clamp-compare and carry an i64 exactness path per coefficient), direct writes
  into `X` (we write a `m[16]` scratch), zero bands and the EOB tail as `STM`
  fills (we pre-zero all 512 and zero again per band), a packed
  `(sym << 5) | len` `huff` (we pair it with a separate `skip` and a per-band
  `bad` check), an inline `clz6` escape fast path (ours is `inline(never)`), a
  register-resident `{w,n,p}` reader copied once per channel (ours is
  `&mut BitReader` throughout), no stats counters in the product build, and
  format-fused OLA kernels with M/S limited to `nb * BANDW`. Recorded
  consequence: the ~94% projection (now ~91% once the C row is pinned) is the
  period, not parity (69.4%); C-3 is
  split into C-3a (executed *as* C-2, which lacked a design), C-3b/C-3c (build
  shape), C-3d/C-3e (output stage and wrapper), with C-1 re-specified as asm
  that also cuts the ~15.0 KiB stack — that cut is what makes `iwram-tables`
  legal again. Revised projection ~193,000–213,000 (73–81%); the U4 gate is
  re-read as C-class,
  since the gate total adds convert + IRQ + DMA to the decoder.
- **2026-11-14 — PR C: measured the gap instead of guessing at it.** Frame 101
  (the worst) is **63% synthesis kernels / 33% entropy (`dec_chan`) / 4% M/S +
  wrapper**. Four cheap levers were built and run on target: fusing dequant was
  neutral, full `huff` inlining bought 2.7% for 4,252 B of IWRAM (rejected on
  U6), a split `huff` LUT fast path bought 1.6% for 896 B (kept), frame
  pointers were inert, and `iwram-tables` crashed because 8,672 B of tables
  leave less stack than the ~15,040 B the path needs. Consequence: the product
  frame is **523,267 worst / 456,187 mean (200% / 174%)**, and U4 stays open
  with the remaining work scoped as PR C-1 (asm kernels, ~194,000 cycles — the
  ~182,000 figure was struck when the C row was pinned at 134,600) and
  PR C-2 (entropy rewrite, ~90,000 cycles) — projected ~94% together. Recorded
  because the plan had assumed tuning might close this; measurement says the
  gap is architectural.

- **2026-11-14 — PR B landed: the hot path is GBA-first and the ≤2× gate is met.**
  `uli_lite_gba::GbaDecoder` (32-bit cached bit reader, unchecked canonical
  Huffman, dequant, M/S, ARM-mode `.iwram`) measures a full frame at **533,605
  worst / 463,660 mean = 204% / 177%** of the period, both pinned, against the
  same-image core control at 1023% / 830% and `ArmKernels` at 640% / 447%.
  Non-synthesis `rest` fell 837,092 → 195,484 cycles/frame (4.3×). PR B's gate
  (≤200% worst) is met; U4 is not (204%). The remaining gap is the synthesis
  kernels (2.51× the C/asm reference, OLA the outlier) — that is PR C's scope,
  and it is now the whole remaining delta, not a part of it.
  Placement record: the first on-target run **hung** (`pc=0x00000004`) because
  an 8 KiB `HotState` in `.iwram` left less stack than the ≈15.0 KiB the
  product path needs; the stack overflowed past `__iwram_end` into the hot
  buffers and the ARM/Thumb thunks. The host twin was green throughout, so this
  was a target placement defect, not arithmetic. `HotState` is EWRAM in the
  bench; the IWRAM/stack budget is now U6's binding constraint.

- **2026-11-14 — ULI2 is GBA-first.** Portability constraints retired for the
  product path; `uli-lite` becomes the oracle, `uli-lite-gba` the product
  decoder. PR A lands the kernel seam and the full-frame instrument; PR B
  rewrites the hot path; PR C closes the gap; PR D is playback.
- **2026-11-14 — PR A landed.** The `Kernels` seam and full-frame instrument
  make the product kernel set measurable end to end: 1,829,247 worst / 1,322,800
  mean (698% / 505% of the period), both pinned. The Phase 1b projection is
  retired. Recorded caveat: absolute frame numbers are image-specific at this
  fetch-dominated scale; every future arm keeps the control in the same image.
- **2026-11-14 — plan of record moves here.** `ULI2.md` stays the decision
  record; phases, gates and the measurement ledger live in this file.