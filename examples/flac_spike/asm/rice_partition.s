@ e10_rice_partition — E10 probe kernel: the whole Rice partition sample
@ loop in ARM mode (FLAC.md "E10 — ARM-mode codeword loop: algorithm plan
@ of action (2026-10-14)"; hybrid pick: tier-1 ctz8 q <= 7, C1 De Bruijn
@ 8..=31 flat, C2 word-skip beyond 31; bit-reversed window, rev8 refill).
@
@ [6a-5b PORT] (item 6 PR 6a-5b, 2026-11-09): this file is the E10 archive's
@   `asm/rice_partition.s` verbatim EXCEPT that its exported symbol is renamed
@   `arm_rice_partition` -> `e10_rice_partition`. The rename is mechanical, not
@   semantic: 6a-5b links this vendored leg and the landed overlay kernel
@   (crates/flac-lite-gba/asm/rice_partition.s) into ONE image, and both would
@   otherwise define the same `.global` — non-namespaced `global_asm!` symbols
@   collide at link time. No encoding, register, label or ordering in the body
@   changed, so a cycle reading from this leg still means "E10's kernel". The
@   diff between this body and the landed one is exactly the [6a-4 …] delta list
@   (5 instructions per sample — G1's `ldr+cmp+bhi` and G2's `adds+beq` — plus 3
@   per partition), which is the quantity 6a-5b's reading is about.
@
@ Vendored seam module (examples/flac_spike/src/e10.rs); probe code, never
@ lands on main's spike sources, never wired into crates/flac-lite. The
@ fold rides the committed fold_body.s source (decision 6's sharing — the
@ probe vendor's verbatim copy, pinned byte-identical by an e10.rs host
@ test).
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
@ EOF contract (uniform by construction): unterminated unary AND a
@ truncated remainder both write status 1 and restore NOTHING (ratified
@ item 1) — the state stays exactly where the walk stopped. Reachable only
@ on corrupt/truncated streams; the committed clips never reach it
@ (bit-exact witnesses host + ROM). This is the probe's ONE picked cursor
@ contract (design of record amendment 2026-10-16); it diverges from the
@ live loop's per-read atomicity and from e2b's snapshot restore, on a dead
@ frame either way.
@
@ A second recorded divergence: the live reader rejects a folded value
@ overflowing u32 (quotient > MAX >> order -> InvalidField); this kernel
@ computes (q << p) | rem with plain 32-bit wrap, reachable only on corrupt
@ streams (q >= 2^17 needs ~8192 all-zero words) — dead frame again.
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
@ shape):
@   r0 = &state {{ w, v, ptr, end, status }}  (u32 x5; ptr/end are byte
@        ADDRESSES into the frame-region slice)
@   r1 = out          (i32 residuals, n of them)
@   r2 = n            (samples)
@   r3 = p            (Rice order 0..=14; m = (1<<p)-1 hoisted — the plan's
@        C1 fact: p is constant within a partition)
@   [sp]   = &CTZ8          (256 B byte->ctz, 8 = run past the byte)
@   [sp+4] = &REV8          (256 B byte->bit-reversed)
@   [sp+8] = &BRUIJN_INDEX  (32-entry C1 table)
@ After push {{r4-r11, lr}} (36 B) + sub sp, #8, the stack args sit at
@ [sp+44], [sp+48], [sp+52]. Leaf kernel (no bl): SP restoration is exact.
@
@ Register map (the E7-forced r7 costs one — the loop still fits with the
@ hot tables in registers and a single literal-pool entry):
@   r0 CTZ8   r1 REV8    r2 q_total (loop-local)   r3 scratch
@   r4 w      r5 v       r6 ptr                   r7 end
@   r8 out    r9 out_end r10 p                    r11 m    r12 scratch
    .syntax unified
    .arm
    .text
    .code 32
    .global e10_rice_partition
    .type e10_rice_partition, %function
e10_rice_partition:
    push    {{r4-r11, lr}}              @ 9 pushes = 36 B
    sub     sp, sp, #8                  @ frame: [sp] = &state stash,
                                        @   [sp+4] = C1 base stash
    str     r0, [sp]
    mov     r8, r1                      @ out — saved BEFORE r1 reloads
    mov     r9, r2, lsl #2              @ 4n (n <= 2048: no wrap)
    add     r9, r8, r9                  @ out_end
    ldr     r4, [r0]                    @ w
    ldr     r5, [r0, #4]                @ v
    ldr     r6, [r0, #8]                @ ptr
    ldr     r7, [r0, #12]               @ end
    ldr     r0, [sp, #44]               @ &CTZ8
    ldr     r1, [sp, #48]               @ &REV8  (BRUIJN fetched cold at
                                        @   .Le10lzc from [sp, #52])
    mov     r11, #1
    mov     r11, r11, lsl r3            @ 1 << p — register shift (ARMv4
    sub     r11, r11, #1                @   has no Thumb equivalent)
    mov     r10, r3                     @ p
    cmp     r8, r9
    beq     .Le10done                      @ n == 0
.Le10loop:
    cmp     r5, #25
    blo     .Le10refill1
.Le10tier1:                                @ ---- tier 1: ctz8[low byte] ----
                                        @ INVARIANT here: v >= 25 — the
                                        @ only entries are the fallthrough
                                        @ and refill1's v>=25 exit; the
                                        @ byte-gone exit goes .Le10slow.
    and     r12, r4, #0xFF
    ldrb    r2, [r0, r12]               @ 8 = run past the byte
    cmp     r2, #8
    bhs     .Le10tier23
    @ tier-1 hit: q = r2 <= 7. v' = v - (q+1) >= 25 - 8 = 17 >= 14 >= p —
    @ the remainder provably needs no refill (unconditional by the
    @ invariant above; this removes a cmp+branch from the hot path).
    add     r3, r2, #1
    mov     r4, r4, lsr r3              @ drain q+1 (per-byte-shift cost is
    sub     r5, r5, r3                  @   part of what this probe measures)
    b       .Le10rem_have
.Le10refill1:                              @ ---- rev8 refill to v >= 25 ----
    cmp     r6, r7
    bhs     .Le10slow                      @ bytes gone with v < 25: cold tail
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]                @ REV8 — the plan's one extra ldrb
                                        @   per 8 bits (left-to-measure 4)
    orr     r4, r4, r3, lsl r5          @ ratified merge shape (v < 25 so
    add     r5, r5, #8                  @   the shift is < 32 by entry rule)
    cmp     r5, #25
    blo     .Le10refill1
    b       .Le10tier1
.Le10tier23:
    teq     r4, #0
    beq     .Le10tier3                     @ w == 0: the handoff signal
                                        @   (decision 2), never an answer
    mov     r2, #0                      @ C1: base run 0
    b       .Le10lzc
.Le10lzc:                                  @ ---- C1 De Bruijn, base in r2 ----
    str     r2, [sp, #4]                @ stash base
    rsbs    r3, r4, #0
    ands    r3, r4, r3                  @ lowbit(w); w != 0 on every entry
                                        @   path into .Le10lzc
    ldr     r12, .Le10k                    @ 0x077CB531 — not a rotatable
                                        @   immediate (plan's measured
                                        @   substrate fact)
    mul     r2, r3, r12                 @ Rd distinct from both sources
                                        @   (ARMv4 MUL rule, PR B pitfall)
    lsr     r2, r2, #27
    ldr     r12, [sp, #52]              @ &BRUIJN_INDEX
    ldrb    r2, [r12, r2]               @ qdb = ctz(w)  (0..31)
    mov     r12, r2                     @ window-relative q for the drain
    ldr     r3, [sp, #4]
    add     r2, r2, r3                  @ q_total = base + qdb
    @ w != 0 => a set bit sits below v => qdb <= v-1 => drain qdb+1 <= v.
    add     r3, r12, #1
    mov     r4, r4, lsr r3              @ drain qdb+1 (shift by 32 = 0,
    sub     r5, r5, r3                  @   ARMv4-defined: the qdb = 31 leg)
    cmp     r5, r10
    blo     .Le10refill2                   @ v' < p: bytes may be needed
    b       .Le10rem_have
.Le10rem_have:                             @ ---- shared per-sample tail ----
                                        @ r2 = q_total, w/v current; the
                                        @ remainder's p bits are valid.
    and     r3, r4, r11                 @ rem_w = w & m (window-domain;
                                        @   p == 0 -> m == 0 -> rem 0)
    mov     r4, r4, lsr r10             @ drain p
    sub     r5, r5, r10
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
    str     r12, [r8], #4
    cmp     r8, r9
    blt     .Le10loop
    b       .Le10done
.Le10refill2:                              @ cold refill until v >= 25
    cmp     r6, r7
    bhs     .Le10rf2eof
    ldrb    r3, [r6], #1
    ldrb    r3, [r1, r3]
    orr     r4, r4, r3, lsl r5
    add     r5, r5, #8
    cmp     r5, #25
    blo     .Le10refill2
    b       .Le10rem_have
.Le10rf2eof:                               @ bytes gone: rem must ride the
    cmp     r5, r10                     @   window's valid bits
    blo     .Le10unterm
    b       .Le10rem_have
.Le10tier3:                                @ ---- C2 word-skip (w == 0) ----
    cmp     r6, r7
    bhs     .Le10unterm                    @ zero window AND no bytes: EOF
    mov     r2, r5                      @ base = v (all valid bits were 0)
    mov     r5, #0                      @ window drained
.Le10t3skip:                               @ whole-word zero tests, RAW bytes
    add     r12, r6, #4                 @   (zero iff reversed zero — the
    cmp     r12, r7                     @   zero test pays no rev8)
    bhi     .Le10wslow
    ldrb    r3, [r6], #1
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #8
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #16
    ldrb    r12, [r6], #1
    orr     r3, r3, r12, lsl #24
    teq     r3, #0
    bne     .Le10t3term
    add     r2, r2, #32
    b       .Le10t3skip
.Le10t3term:                               @ terminating word: rev8-assemble
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
    b       .Le10lzc                       @ q_total = base + ctz(word)
.Le10wslow:                                @ < 4 bytes ahead: per-byte walk
    cmp     r6, r7
    bhs     .Le10unterm
    ldrb    r3, [r6], #1
    teq     r3, #0
    addeq   r2, r2, #8                  @ zero byte: run += 8 — teq's Z
    beq     .Le10wslow                     @   survives the unsuffixed add
                                        @   (PR B's measured S-flag chain)
    ldrb    r3, [r1, r3]
    mov     r4, r3                      @ terminating byte at window bit 0
    mov     r5, #8
    b       .Le10lzc
.Le10slow:                                 @ cold tail: bytes gone, v < 25.
    mov     r12, #0                     @   per-bit run with full guards —
.Le10slowrun:                              @   runs only on the final samples
    teq     r5, #0                      @   of a slice tail; gives the
    beq     .Le10unterm                    @   uniform EOF/status contract
    ands    r3, r4, #1                  @   without touching the hot path
    bne     .Le10slowterm
    add     r12, r12, #1
    mov     r4, r4, lsr #1
    sub     r5, r5, #1
    b       .Le10slowrun
.Le10slowterm:
    mov     r4, r4, lsr #1              @ consume the terminating 1
    sub     r5, r5, #1
    cmp     r5, r10
    blo     .Le10unterm                    @ truncated remainder
    mov     r2, r12                     @ q_total = the zero RUN (q < 25 on
                                        @   this path); the terminator adds
                                        @   no count — it is consumed only
    b       .Le10rem_have
.Le10unterm:                               @ ratified item 1: status data,
    mov     r2, #1                      @   restore NOTHING — (w, v, ptr)
    b       .Le10epi                       @   stay exactly where the walk
                                        @   stopped
.Le10done:
    mov     r2, #0
.Le10epi:
    ldr     r0, [sp]
    str     r4, [r0]
    str     r5, [r0, #4]
    str     r6, [r0, #8]
    str     r2, [r0, #16]               @ status
    add     sp, sp, #8
    pop     {{r4-r11, lr}}
    bx      lr                          @ interwork return (PR B: ARMv4T
                                        @   does NOT interwork through an
                                        @   LDM-to-PC; only BX/BLX switch)
    .align 2
.Le10k:
    .word   0x077CB531
