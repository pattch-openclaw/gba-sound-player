@ uli_ola_s8 / uli_ola_s16 — window + overlap-add with the output rounding fused.
@ Bit-exact with `uli_lite_gba::ola_s8` / `ola_s16`, which are bit-exact with the
@ core and the C reference.
@
@ WHY: in-frame `ola` was 34,375 at the gate frame = 17,187/channel = 1.12x the
@ C row. The disassembly showed the loop at **33 instructions** per iteration
@ against the reference's **26**, and the excess was in two places:
@
@   1. THE CLAMP — 8 instructions per output where the reference needs 6. LLVM
@      saturates straight to int8 (cmn / mvn / asrgt / cmp / movge); the
@      reference saturates to int16 with the three-instruction idiom
@      `mov t, x, asr #15 ; teq t, x, asr #31 ; eorne x, 0x7FFF, x, asr #31`,
@      then shifts right 8 and stores a byte. `sat8(x >> 8) == (sat16(x) >> 8)`
@      for every i32, so the shorter form is the same function.
@   2. THE ADDRESSING — 7 instructions of index arithmetic (two `add lsl #2` +
@      `ldr [r, #-imm]` pairs plus indexed loads/stores) where the reference keeps
@      five POST-INDEXED CURSORS (a, b, wa, wb, out-forward, out-backward) and
@      reads the overlap word with `ldr r9, [b_cursor, Y_minus_P]`.
@
@ The four multiplies are the algorithm's own and are not touched.
@
@ ARITHMETIC — identical to the Rust kernels and the reference:
@   acc1 = a*wa - b*wb  -> out[i]        a = Y[h+i], b = P[h-1-i]
@   acc2 = -a*wb - b*wa -> out[M-1-i]
@   int8  = sat8(hi32(acc + 2^39) >> 8)  == ((hi32(acc) + 0x80) >> 8) clamped
@   int16 = sat16(hi32(acc + 2^31))
@   and P[h-1-i] = Y[h-1-i] afterwards.
@ The `+2^39` becomes `+0x80` on the HIGH word because 2^39's low 32 bits are
@ zero and acc's are below 2^32, so nothing carries into the high word — the
@ reference's own identity, which is why working on the high word is exact.
@
@ REGISTER MAP: six cursors, four lanes, one multiply pair.
@   r1 a cursor, +4       r2 b cursor, -4      r3 wa cursor, +4
@   r4 wb cursor, -4      r5 out forward       r6 out backward
@   r7 Y - P (bytes, constant, for the overlap load)
@   r8 0x7FFF (the saturation constant)
@   r9 a    r10 b    r11 wa    r12 -wb / -wa
@   lr, r0  the multiply's lo, hi

    .syntax unified
    .arm

    .section .iwram,"ax",%progbits

@ Round, clamp and store one output. `mode` 0 = int8, 2 = int16.
.macro OLA_OUT mode, out, step
  .if \mode == 2
    adds    lr, lr, #0x80000000        @ acc += 2^31, carrying into the high word
    adc     r0, r0, #0
  .else
    add     r0, r0, #0x80              @ the +2^39 rounding, on the high word
  .endif
    mov     lr, r0, asr #15            @ sat16(r0), the reference's idiom:
    teq     lr, r0, asr #31            @   if (r0 >> 15) != (r0 >> 31) then
    eorne   r0, r8, r0, asr #31        @   r0 = 0x7FFF ^ sign_mask
  .if \mode == 0
    mov     r0, r0, asr #8
    strb    r0, [\out], #\step
  .else
    strh    r0, [\out], #(2*\step)
  .endif
.endm

.macro OLA_FN name, mode
    .global \name
    .type   \name, %function
\name:
    @ r0 = Y (M i32), r1 = P (H i32), r2 = out, r3 = WIN (M i32)
    push    {{r4-r11, lr}}

    mov     r5, r2                     @ out forward cursor
  .if \mode == 0
    add     r6, r2, #512               @ &out[M-1]  (M = 512, NOT H = 256);
    sub     r6, r6, #1                 @   511 is not one ARM immediate
  .else
    add     r6, r2, #1024              @ &out[M-1] as int16 = 2*(M-1)
    sub     r6, r6, #2
  .endif
    mov     r7, r0
    sub     r7, r7, r1                 @ Y - P (bytes): the overlap read below
    add     r2, r1, #1020              @ b cursor = &P[H-1]
    add     r1, r0, #1024              @ a cursor = &Y[H]
    add     r4, r3, #2048              @ wb cursor = &WIN[M-1], indexed by M-1-i
    sub     r4, r4, #4
    mov     r8, #0x7F00
    orr     r8, r8, #0xFF              @ 0x7FFF

1:
    ldr     r9, [r1], #4               @ a = Y[h+i]
    ldr     r10, [r2]                  @ b = P[h-1-i]
    ldr     r11, [r3], #4              @ wa = WIN[i]
    ldr     r12, [r4], #-4             @ wb = WIN[M-1-i]
    rsb     r12, r12, #0               @ -wb
    smull   lr, r0, r11, r9            @ wa*a
    smlal   lr, r0, r12, r10           @ + (-wb)*b  -> acc1
    OLA_OUT \mode, r5, 1
    rsb     r11, r11, #0               @ -wa
    smull   lr, r0, r12, r9            @ -wb*a
    smlal   lr, r0, r11, r10           @ + (-wa)*b  -> acc2
    OLA_OUT \mode, r6, -1
    ldr     r9, [r2, r7]               @ Y[h-1-i]
    str     r9, [r2], #-4              @ P[h-1-i] = Y[h-1-i]
    cmp     r5, r6
    blo     1b

    pop     {{r4-r11, lr}}
    bx      lr
    .size \name, .-\name
.endm

    OLA_FN uli_ola_s8, 0
    OLA_FN uli_ola_s16, 2
