@ arm_word_skip — PR B kernel: C2 word-wide continuation skip.
@ FLAC.md "E10 unit-test kernel track — design of record (2026-10-16)",
@ decision 1 (standalone — mechanically separate from C1: the terminating
@ word's tail is counted by the committed ctz8 byte table, not De Bruijn),
@ decision 2 (answers the TOTAL run from bit 0 of the slice — the caller
@ never adds), decision 3 (unterminated = status data, no state save),
@ decision 4 (budget = input slice length; no hard-coded cap; the count is
@ the walk's answer on any budget).
@
@ ABI (sret — three-word output struct; all args in r0-r3):
@   r0 = &out   {{ q, words_consumed, status }}  (u32 x3)
@   r1 = words  pointer to the word slice (bit i of the run = bit i%32 of
@               words[i/32], little-endian words — the kernel's table
@               header pins the exact convention)
@   r2 = budget_words (0..=slice length; may exceed what the run needs)
@   r3 = ctz8   the committed 256-entry byte->ctz table (8 = past the byte)
@ Clobbers r12 and flags; preserves r4-r11 and SP; writes only
@ [out .. out+12). Interwork return: pop lr then BX LR — ARMv4T does NOT
@ interwork through an LDM-to-PC (measured 2026-10-28: a Thumb caller's
@ `bl` + a `pop {{..., pc}}` ARM tail loops silently between the linker
@ thunk's literal and this entry — the boot hang that convicted this
@ shape; only BX/BLX switch state on this core).
@
@ The O(q/32) claim (the algorithm plan's C2): every accumulated word is
@ tested as a WHOLE with the rotate-immediate compare `teq` — ARM mode;
@ Thumb v4 cannot compare a full word against zero, which is exactly what
@ starves the shipped loop into its per-byte continuation. An empty word
@ adds 32 via the predicated `addeq` while the compare's Z flag survives
@ it (addeq writes flags only when suffixed — the S-flag chain):
@     teq / addeq / beq     ~2 cyc per 32 bits of run
@ The terminating word's tail (<= 31 bits) costs <= 4 table lookups.
    .syntax unified
    .arm
    .text
    .code 32
    .global arm_word_skip
    .type arm_word_skip, %function
arm_word_skip:
    push    {{r4-r7, lr}}
    mov     r4, r0                  @ &out
    mov     r6, r2                  @ budget0 (consumed = budget0 - r2)
    mov     r0, #0                  @ q
.Lws_loop:
    cmp     r2, #0
    beq     .Lws_unterm             @ budget exhausted before a 1 (decision 3)
    ldr     r12, [r1], #4           @ word k; cursor advances
    subs    r2, r2, #1              @ budget--
    teq     r12, #0                 @ the whole-word rotate-immediate
                                    @ compare. ARM's TEQ ALWAYS writes
                                    @ flags: under .syntax unified the
                                    @ 'teqs' spelling is rejected outright
                                    @ (clang: "'teq' can not set flags",
                                    @ measured 2026-10-27) — the pre-UAL
                                    @ mnemonic IS the S-flag form.
    addeq   r0, r0, #32             @ empty word: +32, table untouched
    beq     .Lws_loop               @ addeq wrote no flags: Z is teq's
    @ ---- terminating word at r1-4: count its tail, <= 4 ctz8 lookups ----
    @ (deliberately NOT the De Bruijn shape — decision 1's implementation
    @ isolation keeps C2's witness and later attribution clean)
    ldrb    r5, [r1, #-4]
    ldrb    r5, [r3, r5]
    cmp     r5, #8
    addlo   r0, r0, r5
    blo     .Lws_done
    add     r0, r0, #8
    ldrb    r5, [r1, #-3]
    ldrb    r5, [r3, r5]
    cmp     r5, #8
    addlo   r0, r0, r5
    blo     .Lws_done
    add     r0, r0, #8
    ldrb    r5, [r1, #-2]
    ldrb    r5, [r3, r5]
    cmp     r5, #8
    addlo   r0, r0, r5
    blo     .Lws_done
    add     r0, r0, #8
    ldrb    r5, [r1, #-1]
    ldrb    r5, [r3, r5]
    add     r0, r0, r5              @ word != 0: the walk always lands
                                    @ before this lookup can return 8
.Lws_done:
    mov     r7, #1
    b       .Lws_out
.Lws_unterm:
    mov     r7, #0                  @ status data; q = 32 * zero words
                                    @ consumed — the walk's answer, never a
                                    @ cap artifact (decision 4)
.Lws_out:
    sub     r2, r6, r2              @ words_consumed
    stmia   r4, {{r0, r2, r7}}        @ {{ q, words_consumed, status }}
    pop     {{r4-r7, lr}}
    bx      lr                      @ interwork return (see header note)
