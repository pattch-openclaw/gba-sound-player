@ arm_sample_step — PR B composite sample-step kernel.
@ FLAC.md "E10 unit-test kernel track — design of record (2026-10-16)":
@ decision 2 (tier dispatch lives ONLY here: tier-1 ctz8 q <= 7 -> C1 De
@ Bruijn 8..=31 -> C2 beyond 31; the handoff signal is w == 0, never the
@ answer's value; C2 answers the total run from bit 0 — the caller never
@ adds), 5 (bit-reversed window convention pinned in the table header;
@ tier 1 is the committed ctz8 byte table whose 8 means "run past the
@ byte" -> dispatch onward), 6 (the fold arrives via the shared
@ fold_body.s source — pasted at both use sites of the single assembled
@ unit, the .include made cargo-cwd-safe), 7 (in: pre-assembled word
@ slice + p — bit offset is NOT an input; the kernel owns the remainder
@ EXACTLY as pinned there, `(terminating_word >> (q+1)) & ((1<<p) - 1)`,
@ the drain count q+1+p (bit retirement is the probe's reader, not this
@ kernel), and the fold; the register shifts carry ARMv4's defined
@ shift-by-32 semantics — result zero — pinned as the q = 31 boundary),
@ 3 (unterminated = status data, no state save), 4 (budget = input slice
@ length, passed straight to C2; no hard-coded cap).
@
@ Deliberately NOT here (recorded 2026-10-27 while wiring): any rev8
@ reversal of the remainder bits or w_{{t+1}} merge. Decision 7 pins the
@ remainder to the literal extract above; stitching the reversed-window
@ domain to spec-order FLAC streams (rev8 refill, straddle merges, reader
@ alignment) is the probe's vendored seam — the left-to-measure items the
@ design of record lists against it. The committed table carries a pinned
@ q = 31 vector whose remainder bits live in words[1] with expectation
@ rem = 0: the drift sentinel that convicts any merge-or-reversal
@ implementation of this kernel.
@
@ ABI (all args in registers r0-r3 + two AAPCS stack args; no sret —
@ outputs go through the explicit out pointer):
@   r0 = &out   {{ q, remainder, residual, drain_bits, words_consumed,
@                 status }}  (u32 x6; residual is i32 bits; a SUPERSET of
@                 decision 7's (q, remainder, words_consumed, status) —
@                 every output the kernel owns is table-judgeable)
@   r1 = words        pointer to the word slice (window domain: stream
@                     bit i = bit i%32 of words[i/32], little-endian —
@                     pinned in the table header)
@   r2 = budget_words (slice length; decision 4 semantics)
@   r3 = p            Rice remainder parameter, 0..=14 (the sweep domain;
@                     every shift here is a register op, so any p <= 30
@                     computes; p == 0 -> remainder 0 by the mask, which
@                     is exactly the live reader's order-0 skip)
@   [sp]   = &CTZ8          (committed byte->ctz table, 8 = past byte)
@   [sp+4] = &BRUIJN_INDEX  (C1's committed 32-entry table — SAME bytes)
@ Clobbers r12 + flags; preserves r4-r11 and SP; writes only
@ [out .. out+24). Interwork return: pop lr then BX LR — ARMv4T does NOT
@ interwork through an LDM-to-PC (measured 2026-10-28 on the boot-hang
@ image; only BX/BLX switch state). The internal `bl arm_word_skip` is a
@ plain ARM call: C2's bx lr lands back in ARM here.
@
@ The De Bruijn constant rides this kernel's own literal pool — the
@ algorithm plan's measured fallback ("the MUL takes it from a dedicated
@ register staged once outside the sample loop (PC-relative literal ldr
@ as the fallback)"): the isolated kernel has no loop to hoist out of;
@ the probe stages the register.
    .syntax unified
    .arm
    .text
    .code 32
    .global arm_sample_step
    .type arm_sample_step, %function
arm_sample_step:
    push    {{r4-r9, lr}}
    sub     sp, sp, #20                 @ frame: [sp] = 12 B C2 temp out,
                                        @   [sp+12] = parked remainder.
                                        @   AAPCS: SP % 8 == 0 at entry; 7
                                        @   pushes (28) -> %8 == 4; sub 20
                                        @   restores %8 == 0 at the bl to
                                        @   C2 below. (An earlier draft
                                        @   read entry as %8 == 4 and used
                                        @   16 — misaligned at the bl; the
                                        @   ARM7TDMI tolerates it, the ABI
                                        @   does not, 2026-10-28.)
    ldr     r4, [sp, #48]               @ &CTZ8         (stack arg 5)
    ldr     r5, [sp, #52]               @ &BRUIJN_INDEX (stack arg 6)
    mov     r6, r0                      @ &out
    mov     r7, r1                      @ words
    mov     r8, r2                      @ budget
    mov     r9, r3                      @ p
    @ ---- tier 1 (decision 5): ctz8[byte0]; 8 = run past the byte ----
    ldrb    r12, [r7]                   @ window byte 0 = stream bits 0..7
    ldrb    r12, [r4, r12]
    cmp     r12, #8
    blo     .Lss_have_q                 @ q < 8 done — one-lookup tier
    @ ---- w == 0 is the handoff signal to C2 (decision 2), tested once ----
    ldr     r0, [r7]
    teq     r0, #0
    beq     .Lss_c2
    @ ---- C1 tier: 8 <= q <= 31 (flat) ----
    rsbs    r2, r0, #0
    ands    r2, r0, r2                  @ lowbit(w0) != 0 here (>= 2^8)
    ldr     r12, .Lss_k                 @ literal-pool K (header note; r12
                                        @   holds tier 1's ctz8 — dead)
    mul     r0, r12, r2                 @ Rd/Rm/Rs ALL DISTINCT (ARMv4 MUL
    lsr     r0, r0, #27                 @   rule, measured pitfall: Rd==Rm
    ldrb    r12, [r5, r0]               @   UNPREDICTABLE, assembled
                                        @   silently). w0 is dead in r0 —
                                        @   C1's same free-operand choice
    b       .Lss_have_q
.Lss_c2:                                @ ---- beyond 31: C2 word-skip ----
    mov     r0, sp                      @ temp {{q, consumed, status}}
    mov     r1, r7                      @ same slice start: C2 answers the
    mov     r2, r8                      @   TOTAL run from bit 0 — the
    mov     r3, r4                      @   caller never adds (decision 2)
    bl      arm_word_skip
    ldr     r12, [sp]                   @ q (total from bit 0)
    ldr     r2, [sp, #8]                @ status
    teq     r2, #0
    beq     .Lss_unterm
.Lss_have_q:
    @ ---- remainder = (words[t] >> (s+1)) & mask — decision 7, literal ----
    mov     r0, r12, lsr #5             @ t = q / 32
    and     r1, r12, #31                @ s = q % 32
    add     r2, r7, r0, lsl #2
    ldr     r3, [r2]                    @ w_t — the TERMINATING word only;
                                        @   no w_{{t+1}} read (header note)
    add     r0, r1, #1                  @ s + 1 (1..=32)
    mov     r3, r3, lsr r0              @ register LSR; by 32 == 0, ARMv4-
                                        @   defined — the q = 31 boundary
    mov     r2, #1
    mov     r2, r2, lsl r9              @ 1 << p — register-value shift
    sub     r2, r2, #1                  @   (Thumb v4 has none); p == 0 ->
    and     r3, r3, r2                  @   mask 0 = the order-0 skip
    str     r3, [sp, #12]               @ park the remainder for out
    @ ---- drain_bits = q+1+p; words_consumed = ceil(drain/32) ----
    add     r4, r12, #1
    add     r4, r4, r9                  @ drain_bits (r4: ctz8 dead here)
    add     r0, r1, #1
    add     r0, r0, r9                  @ s+1+p — bits past w_t's top?
    mov     r5, r12, lsr #5             @ t (r5: bruijn dead here)
    cmp     r0, #32
    addls   r5, r5, #1                  @ last drained bit lands in w_t
    addhi   r5, r5, #2                  @ straddles into w_{{t+1}}
    @ ---- coded = (q << p) | rem; residual = fold(coded) (decision 6) ----
    mov     r2, r12, lsl r9             @ q << p (q <= 64, p <= 14 — no
    orr     r2, r2, r3                  @   overflow in the swept domain)
    fold_body r2, r8                    @ (n>>1) ^ -(n&1); flagless — PR A's
                                        @   carry-live-flags property; r8
                                        @   (budget) is dead past the tiers
    @ ---- out {{q, remainder, residual, drain, consumed, status = 1}} ----
    mov     r0, r12                     @ q
    ldr     r1, [sp, #12]               @ remainder (parked)
                                        @ r2 = residual (from the fold)
    mov     r3, r4                      @ drain_bits
    mov     r4, r5                      @ words_consumed
    mov     r5, #1                      @ status
    stmia   r6, {{r0-r5}}
    add     sp, sp, #20
    pop     {{r4-r9, lr}}
    bx      lr                          @ interwork return (header note)
.Lss_unterm:
    @ decision 3: status data, no state save. C2 answered q = 32 * (zero
    @ words consumed) with the whole budget consumed; remainder/residual/
    @ drain are meaningless — pinned 0 by the committed table.
    mov     r0, r12                     @ q = 32 * budget
    mov     r1, #0                      @ remainder
    mov     r2, #0                      @ residual
    mov     r3, #0                      @ drain_bits
    ldr     r4, [sp, #4]                @ words_consumed (all of budget)
    mov     r5, #0                      @ status = unterminated
    stmia   r6, {{r0-r5}}
    add     sp, sp, #20
    pop     {{r4-r9, lr}}
    bx      lr                          @ interwork return (header note)
    .align 2
.Lss_k:
    .word   0x077CB531                  @ not a rotatable immediate (the
                                        @ plan's measured substrate fact)
