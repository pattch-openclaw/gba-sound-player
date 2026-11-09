@ arm_rice_partition — the whole Rice partition sample loop in ARM mode:
@ split-ladder 6a-4's landing of the E10 probe kernel through the
@ `RiceKernel` seam (flac-docs/plan-of-record.md 6a-4, flac-docs/arm-kernels.md
@ "The kernels"). Tier pick as E10 measured it: tier-1 ctz8 q <= 7, C1 De
@ Bruijn 8..=31 flat, C2 word-skip beyond 31; bit-reversed window, rev8
@ refill (FLAC.md "E10 — ARM-mode codeword loop: algorithm plan of action
@ (2026-10-14)").
@
@ PROVENANCE — lifted from
@ examples/flac_spike/experiments/e10-three-arm-probe/e10-three-arm-probe.patch,
@ the E10 sample patch's copy of `examples/flac_spike/asm/rice_partition.s`.
@ That path exists only inside the patch: the kernel was never committed on
@ main, which is why 6a-4's "lifted from examples/arm-kernels/" wording
@ points at the wrong tree for THIS file — `fold_body.s` IS a committed
@ main-tree file (examples/arm-kernels/asm/), `rice_partition.s` is not,
@ and the patch is its only source of record. Every line below is the
@ patch's except the deltas marked `[6a-4 …]` in the body and listed here:
@   [6a-4 FRAME] frame is 12 bytes, not 8: [sp,#8] holds the §9.2.7.3
@                guard limit hoisted once per partition; the three stack
@                arguments therefore sit one word higher.
@   [6a-4 G1]    §9.2.7.3's first guard (quotient > MAX >> p) is TESTED,
@                not wrapped. The probe left it wrapped on purpose — the
@                divergence paragraph below is kept because it is the
@                finding, even though the landing no longer diverges.
@   [6a-4 G2]    §9.2.7.3's second guard (folded == u32::MAX, the
@                i32::MIN pre-image) is tested before the store.
@   [6a-4 ST]    a third status value, 2 = guard rejection.
@ [6a-5d] PLACEMENT. The body lives in `.iwram`, not `.text`, because the price
@ of an instruction here is 4.000 cycles fetched from cartridge ROM (6a-5c
@ measured it by deleting executed instructions: exactly 4 x 640,000 and
@ 5 x 4 x 640,000 cycles) and about 1.4 from IWRAM (6a-5d relocated this same
@ stream: 1,151,236 -> 506,413 cycles/frame on the FIXED clip). Plain
@ `.iwram` — `.text_iwram` is swallowed by `.text : {{ *(.text .text*) }}` in the
@ linker script (E6's recorded trap). The 4-cycle figure is also why the two
@ guards cost what they cost: at 4 cycles per instruction, an instruction
@ deleted from this loop is worth 1.56 budget points at 32,768 Hz.
@
@ The fold is NOT vendored here. This unit gets `fold_body.s` from the one
@ committed source (examples/arm-kernels/asm/fold_body.s), pasted through
@ the Rust `include_str!`/`concat!` in src/rice_arm.rs — decision 6's
@ sharing rule: "shared by textual include, not duplicated by hand", and
@ rustc gives the assembler no reliable search path, so the paste happens in
@ the Rust source that ships it.
@
@ Window discipline (the ratified refill shape, ratified item 3 + decision
@ 5's bit-reversed convention): w holds stream bits [P, P+v) with bit 0 of
@ w = the NEXT stream bit (P = absolute bit cursor); bits >= v of w are
@ ALWAYS zero. Invariant: ptr == (P + v) / 8 exactly (byte-aligned read
@ pointer) — maintained by construction: consumes move P and v oppositely,
@ refills move ptr and v together. Refill merges bytes bit-reversed:
@ w |= REV8[byte] << v (register-value shift; the committed
@ arm_refill_merge kernel is this merge's v == 24 specialization).
@
@ THE STITCHING THE PLAN LEFT TO THE PROBE (probe-time finding 2026-10-29,
@ before any measurement): the algorithm plan's C1 consume formula
@ `(w >> (q+1)) & ((1<<p)-1)` is window-domain; FLAC's Rice remainder is
@ MSB-first natural binary (spec 9.2.7.2), so in the bit-reversed window
@ the extracted p bits are BIT-REVERSED vs the coded number the unmap
@ expects. The kernel therefore un-reverses the remainder: y = REV8[lo] << 8
@ | REV8[hi] (16-bit reverse of the window-domain extract, lo/hi bytewise),
@ spec_rem = y >> (16 - p). ~8 extra cycles per sample when p > 0 (and
@ p == 0 lands on the same path for free: m == 0 -> y == 0 -> rem == 0).
@ This is a real cost of the reversed-window shape the plan priced as only
@ "one extra ldrb per 8 bits" in the refill — the probe measures the whole
@ tax, refill + remainder, as the arm's honest price. The host twin mirrors
@ the reversal exactly; bit-exactness vs `flac -d` is the witness.
@
@ Status — the state struct's fifth word is the exit kind, and the three
@ exits differ in what they save:
@   0  every sample written; state = the final window (w, v, ptr, end)
@   1  unterminated unary OR a truncated remainder: status data, restores
@      NOTHING (ratified item 1) — (w, v, ptr) stay exactly where the walk
@      stopped. Reachable only on corrupt/truncated streams; the committed
@      clips never reach it (bit-exact witnesses host + ROM). This is the
@      probe's ONE picked cursor contract (design of record amendment
@      2026-10-16); it diverges from the live loop's per-read atomicity
@      and from e2b's snapshot restore, on a dead frame either way. The
@      wrapper reproduces the incumbent's `Error::EndOfStream` here.
@   2  [6a-4 ST] a §9.2.7.3 guard fired: G1 after the terminating 1 and
@      before the remainder, G2 after the remainder and before the store. The
@      kernel writes the advanced (w, v, ptr) at exactly those two points, and
@      the wrapper HANDS THEM BACK, because the incumbent is not atomic there
@      either: it reads the whole codeword and only then asks §9.2.7.3, leaving
@      its reader past the codeword. The two mechanisms' reject points are the
@      same two points, so a rejected codeword leaves both cursors identical
@      (measured: order 16, 65,553 bits, both paths). This is NOT the status-1
@      contract: on status 1 the walk's position is a coincidence, on status 2
@      it is the position the incumbent would have stopped at.
@
@ A second recorded divergence (probe-time; CLOSED by [6a-4 G1/G2]): the
@ live reader rejects a coded number that cannot fit 32 bits (quotient >
@ MAX >> order) and rejects the u32::MAX fold (the i32::MIN pre-image) —
@ RFC 9639 / FLAC §9.2.7.3. E10 wrapped instead, on the argument that both
@ are reachable only on corrupt streams (q >= 2^17 needs ~8192 all-zero
@ words) — dead frame either way. The landing tests both guards instead,
@ because the seam's contract is the incumbent's: `TableCountKernel`
@ answers `Error::InvalidField` for those inputs and a kernel through the
@ seam may not answer a different sample. The cost is one hoisted `mvn +
@ lsr + str` per partition and `ldr + cmp + bhi` plus `adds + beq` per
@ sample — 3 and 2 instructions in a tail that is ~40 cycles today
@ (E10's −348 cyc/sample is measured on a build with neither guard; 6a-5
@ prices them, and a cheaper realization of G1 — testing at the
@ accumulation points instead of the shared tail — is 6a-5's to try).
@ Guard-reachable inputs are NOT dead for correctness: they are legal at
@ the profile's edges (order 16 with a 65,536-long run is 8 KB of zeros),
@ and 6a-4's boundary list names them.
@
@ Tier-3 realization at the byte seam: the kernel track's arm_word_skip
@ tests pre-staged words (~2 cyc/32 bits); here continuation words are
@ assembled from the raw byte buffer (4 ldrb + 3 orr, ~10 cyc/word). A raw
@ word is zero iff its reversal is, so the zero test pays NO rev8; only the
@ terminating word pays rev8 (its bits enter the window). The host twin
@ carries per-tier counters — the plan's "per-clip quotient distribution
@ decides how much" question, answered on the exact embedded bytes.
@
@ ABI (r0-r3 + three AAPCS stack args, the sample_step kernel's proven
@ shape; the Rust side of the same contract, with the offset/address
@ translation and the const asserts, is src/rice_arm.rs):
@   r0 = &state {{ w, v, ptr, end, status }}  (u32 x5; offsets: 0, 4, 8,
@        12, 16 — the last is the exit kind, see Status. ptr/end are byte
@        ADDRESSES inside the frame-region slice, not slice-relative
@        offsets; the Rust window holds offsets and the bridge translates.)
@   r1 = out          (i32 residuals, n of them)
@   r2 = n            (samples; the E10 probe assumed n <= 2048 = the
@        profile blocksize cap, because out_end is computed as `4n` with a
@        register-shift-free `lsl #2` — the Rust side checks the
@        byte-length instead, so the kernel's own bound is that out_end
@        not wrap)
@   r3 = p            (Rice order. m = (1<<p)-1 hoisted — the plan's C1
@        fact: p is constant within a partition. DOMAIN: p <= 16. The
@        remainder leg builds a 16-bit reversal (rev8 of two bytes) and
@        shifts by 16-p, so p > 16 is outside what this realization
@        computes; the Rust wrapper delegates wider orders to the core's
@        always-correct kernel rather than answering a wrong sample. p == 0
@        is IN range and rides the same leg (m == 0 -> y == 0 -> rem == 0).)
@   args 5..7 (caller's frame, in order): &CTZ8, &REV8, &BRUIJN_INDEX
@        CTZ8 is 256 B byte->ctz (8 = run past the byte), REV8 is 256 B
@        byte->bit-reversed, BRUIJN_INDEX is the 32-entry C1 table.
@        After push {{r4-r11, lr}} (36 B) + sub sp, #12 they sit at
@        [sp+48], [sp+52], [sp+56]; the frame's own words are [sp] = the
@        saved &state, [sp+4] = the C1 base stash, [sp+8] = the
@        [6a-4 G1] limit. (Measured, not assumed: the first landed cut
@        used 56/60/64 — the 12-byte frame's +4 added to the probe's
@        offsets as if it were +8 — and the kernel-check ROM convicted it
@        on the first end-of-stream arm, which is the arm whose region is
@        short enough that a mis-pointed CTZ8 read stops mattering only
@        after it has already produced a wrong status.)
@ Leaf kernel (no bl): SP restoration is exact.
@
@ Register map (the E7-forced r7 costs one — the loop still fits with the
@ hot tables in registers and a single literal-pool entry):
@   r0 CTZ8   r1 REV8    r2 q_total (loop-local)   r3 scratch
@   r4 w      r5 v       r6 ptr                   r7 end
@   r8 out    r9 out_end r10 p                    r11 m    r12 scratch
    .syntax unified
    .arm
    @ [6a-5d] The body lives in `.iwram`, not `.text` — measured at 1,151,236 ->
    @   506,413 cycles/frame on the FIXED clip (109.77% -> 48.29% of the 32,768 Hz
    @   budget), with nothing else changed. Plain `.iwram`, never `.text_iwram`:
    @   the linker script's `.text : {{ *(.text .text*) }}` comes first and would
    @   swallow that name (E6's recorded trap), whereas `.iwram` is collected by
    @   `*(.iwram .iwram.*)` into an output section marked `> iwram AT>rom`, which
    @   the boot code copies down.
    .section .iwram,"ax",%progbits
    .code 32
    .global arm_rice_partition
    .type arm_rice_partition, %function
arm_rice_partition:
    push    {{r4-r11, lr}}              @ 9 pushes = 36 B
    sub     sp, sp, #12                  @ frame: [sp] = &state stash,
                                        @   [sp+4] = C1 base stash,
                                        @   [sp+8] = [6a-4 G1] MAX >> p
    str     r0, [sp]
    mov     r8, r1                      @ out — saved BEFORE r1 reloads
    mov     r9, r2, lsl #2              @ 4n (n <= 2048: no wrap)
    add     r9, r8, r9                  @ out_end
    ldr     r4, [r0]                    @ w
    ldr     r5, [r0, #4]                @ v
    ldr     r6, [r0, #8]                @ ptr
    ldr     r7, [r0, #12]               @ end
    ldr     r0, [sp, #48]               @ &CTZ8
    ldr     r1, [sp, #52]               @ &REV8  (BRUIJN fetched cold at
                                        @   .Llzc from [sp, #56])
    mov     r11, #1
    mov     r11, r11, lsl r3            @ 1 << p — register shift (ARMv4
    sub     r11, r11, #1                @   has no Thumb equivalent)
    mov     r10, r3                     @ p
    @ [6a-4 G1] the §9.2.7.3 "must fit in 32 bits" bound, hoisted once per
    @   partition (p is constant here — the plan's C1 fact). MAX >> p, by
    @   register shift because p is in a register; p <= 16 reaches this
    @   kernel (wider orders are delegated by the Rust side), so the shift
    @   is never by 32 and never by 0 mod 32 in disguise.
    mvn     r12, #0
    lsr     r12, r10
    str     r12, [sp, #8]
    cmp     r8, r9
    beq     .Ldone                      @ n == 0
.Lloop:
    cmp     r5, #25
    blo     .Lrefill1
.Ltier1:                                @ ---- tier 1: ctz8[low byte] ----
                                        @ INVARIANT here: v >= 25 — the
                                        @ only entries are the fallthrough
                                        @ and refill1's v>=25 exit; the
                                        @ byte-gone exit goes .Lslow.
    and     r12, r4, #0xFF
    ldrb    r2, [r0, r12]               @ 8 = run past the byte
    cmp     r2, #8
    bhs     .Ltier23
    @ tier-1 hit: q = r2 <= 7. v' = v - (q+1) >= 25 - 8 = 17 >= 14 >= p —
    @ the remainder provably needs no refill (unconditional by the
    @ invariant above; this removes a cmp+branch from the hot path).
    add     r3, r2, #1
    mov     r4, r4, lsr r3              @ drain q+1 (per-byte-shift cost is
    sub     r5, r5, r3                  @   part of what this probe measures)
    b       .Lrem_have
.Lrefill1:                              @ ---- rev8 refill to v >= 25 ----
    cmp     r6, r7
    bhs     .Lslow                      @ bytes gone with v < 25: cold tail
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]                @ REV8 — the plan's one extra ldrb
                                        @   per 8 bits (left-to-measure 4)
    orr     r4, r4, r3, lsl r5          @ ratified merge shape (v < 25 so
    add     r5, r5, #8                  @   the shift is < 32 by entry rule)
    cmp     r5, #25
    blo     .Lrefill1
    b       .Ltier1
.Ltier23:
    teq     r4, #0
    beq     .Ltier3                     @ w == 0: the handoff signal
                                        @   (decision 2), never an answer
    mov     r2, #0                      @ C1: base run 0
    b       .Llzc
.Llzc:                                  @ ---- C1 De Bruijn, base in r2 ----
    str     r2, [sp, #4]                @ stash base
    rsbs    r3, r4, #0
    ands    r3, r4, r3                  @ lowbit(w); w != 0 on every entry
                                        @   path into .Llzc
    ldr     r12, .Lk                    @ 0x077CB531 — not a rotatable
                                        @   immediate (plan's measured
                                        @   substrate fact)
    mul     r2, r3, r12                 @ Rd distinct from both sources
                                        @   (ARMv4 MUL rule, PR B pitfall)
    lsr     r2, r2, #27
    ldr     r12, [sp, #56]              @ &BRUIJN_INDEX
    ldrb    r2, [r12, r2]               @ qdb = ctz(w)  (0..31)
    mov     r12, r2                     @ window-relative q for the drain
    ldr     r3, [sp, #4]
    add     r2, r2, r3                  @ q_total = base + qdb
    @ w != 0 => a set bit sits below v => qdb <= v-1 => drain qdb+1 <= v.
    add     r3, r12, #1
    mov     r4, r4, lsr r3              @ drain qdb+1 (shift by 32 = 0,
    sub     r5, r5, r3                  @   ARMv4-defined: the qdb = 31 leg)
    cmp     r5, r10
    blo     .Lrefill2                   @ v' < p: bytes may be needed
    b       .Lrem_have
.Lrem_have:                             @ ---- shared per-sample tail ----
                                        @ r2 = q_total, w/v current; the
                                        @ remainder's p bits are valid.
    and     r3, r4, r11                 @ rem_w = w & m (window-domain;
                                        @   p == 0 -> m == 0 -> rem 0)
    mov     r4, r4, lsr r10             @ drain p
    sub     r5, r5, r10
    @ [6a-4 G1] reject where the incumbent rejects: it reads the run AND the
    @   remainder and only then asks §9.2.7.3, so a rejected codeword leaves
    @   its reader past the whole codeword. Testing G1 before the drain — as
    @   an earlier cut of this file did — stopped this path 16 bits short of
    @   the incumbent on an order-16 rejection, which is the kind of difference
    @   a caller can see. r12 is scratch until the reversal starts.
    ldr     r12, [sp, #8]               @   the hoisted MAX >> p
    cmp     r2, r12
    bhi     .Lvalerr
    @ [6a-5c L1] the guard insertion above used to leave its own copy of the
    @   `and` behind, so this line appeared twice in a row (6a-5b measured the
    @   consequence, and the two are adjacent in the emitted code because
    @   nothing between them touches r3 or the flags). One copy is dead; the
    @   original is kept, below, after the stitching comment.
    @ Un-reverse the window-domain remainder (header: THE STITCHING).
    and     r12, r3, #0xFF              @ lo = rem_w & 0xFF
    ldrb    r12, [r1, r12]              @ REV8[lo]
    mov     r3, r3, lsr #8              @ hi = rem_w >> 8 (<= 0x3F)
    ldrb    r3, [r1, r3]                @ REV8[hi]
    orr     r3, r3, r12, lsl #8         @ y = 16-bit reverse of rem_w
    rsb     r12, r10, #16               @ 16 - p
    mov     r3, r3, lsr r12             @ rem_spec = y >> (16 - p)
    mov     r12, r2, lsl r10            @ coded = (q_total << p) | rem_spec
    orr     r12, r12, r3
    fold_body r12, r3                   @ rice_unmap (shared fold_body.s)
    adds    r3, r12, #0x80000000        @ [6a-4 G2] the fold is a bijection
    beq     .Lvalerr                    @   u32 -> i32 whose unique pre-image
                                        @   of i32::MIN is the coded number
                                        @   u32::MAX, §9.2.7.3's second
                                        @   guard — tested where it is
                                        @   cheapest instead of re-forming
                                        @   the 32-bit constant
    str     r12, [r8], #4
    cmp     r8, r9
    blt     .Lloop
    b       .Ldone
.Lrefill2:                              @ cold refill until v >= 25
    cmp     r6, r7
    bhs     .Lrf2eof
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    orr     r4, r4, r3, lsl r5
    add     r5, r5, #8
    cmp     r5, #25
    blo     .Lrefill2
    b       .Lrem_have
.Lrf2eof:                               @ bytes gone: rem must ride the
    cmp     r5, r10                     @   window's valid bits
    blo     .Lunterm
    b       .Lrem_have
.Ltier3:                                @ ---- C2 word-skip (w == 0) ----
    cmp     r6, r7
    bhs     .Lunterm                    @ zero window AND no bytes: EOF
    mov     r2, r5                      @ base = v (all valid bits were 0)
    mov     r5, #0                      @ window drained
.Lt3skip:                               @ whole-word zero tests, RAW bytes
    add     r12, r6, #4                 @   (zero iff reversed zero — the
    cmp     r12, r7                     @   zero test pays no rev8)
    bhi     .Lwslow
    ldrb    r3, [r6], #1
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #8
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #16
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #24
    teq     r3, #0
    bne     .Lt3term
    add     r2, r2, #32
    b       .Lt3skip
.Lt3term:                               @ terminating word: rev8-assemble
    sub     r6, r6, #4
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    mov     r4, r3
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    orr     r4, r4, r3, lsl #8
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    orr     r4, r4, r3, lsl #16
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    orr     r4, r4, r3, lsl #24
    mov     r5, #32
    b       .Llzc                       @ q_total = base + ctz(word)
.Lwslow:                                @ < 4 bytes ahead: per-byte walk
    cmp     r6, r7
    bhs     .Lunterm
    ldrb    r3, [r6], #1
    teq     r3, #0
    addeq   r2, r2, #8                  @ zero byte: run += 8 — teq's Z
    beq     .Lwslow                     @   survives the unsuffixed add
                                        @   (PR B's measured S-flag chain)
    ldrb    r3, [r1, r3]
    mov     r4, r3                      @ terminating byte at window bit 0
    mov     r5, #8
    b       .Llzc
.Lslow:                                 @ cold tail: bytes gone, v < 25.
    mov     r12, #0                     @   per-bit run with full guards —
.Lslowrun:                              @   runs only on the final samples
    teq     r5, #0                      @   of a slice tail; gives the
    beq     .Lunterm                    @   uniform EOF/status contract
    ands    r3, r4, #1                  @   without touching the hot path
    bne     .Lslowterm
    add     r12, r12, #1
    mov     r4, r4, lsr #1
    sub     r5, r5, #1
    b       .Lslowrun
.Lslowterm:
    mov     r4, r4, lsr #1              @ consume the terminating 1
    sub     r5, r5, #1
    cmp     r5, r10
    blo     .Lunterm                    @ truncated remainder
    mov     r2, r12                     @ q_total = the zero RUN (q < 25 on
                                        @   this path); the terminator adds
                                        @   no count — it is consumed only
    b       .Lrem_have
.Lunterm:                               @ ratified item 1: status data,
    mov     r2, #1                      @   restore NOTHING — (w, v, ptr)
    b       .Lepi                       @   stay exactly where the walk
                                        @   stopped
.Lvalerr:                               @ [6a-4 ST] a §9.2.7.3 guard fired:
    mov     r2, #2                      @   no sample is written, and the state
    b       .Lepi                       @   the epilogue stores is the position
                                        @   the wrapper hands back on purpose —
                                        @   the incumbent consumes the codeword
                                        @   before rejecting too, so this is the
                                        @   position its reader is left at. Not
                                        @   a dead store: see the status table.
.Ldone:
    mov     r2, #0
.Lepi:
    ldr     r0, [sp]
    str     r4, [r0]
    str     r5, [r0, #4]
    str     r6, [r0, #8]
    str     r2, [r0, #16]               @ status
    add     sp, sp, #12
    pop     {{r4-r11, lr}}
    bx      lr                          @ interwork return (PR B: ARMv4T
                                        @   does NOT interwork through an
                                        @   LDM-to-PC; only BX/BLX switch)
    .align 2
.Lk:
    .word   0x077CB531
