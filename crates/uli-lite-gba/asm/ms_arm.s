@ uli_ms_arm — the mid/side (M/S) coefficient transform, block transfers.
@ Bit-exact with `uli_lite_gba::ms` (which is bit-exact with the core and the
@ C reference).
@
@ WHY (root-cause audit, 2026-11-16): in-frame M/S was **9,861** cycles at the
@ gate frame — 2.24x the C record's ~4,400, and the single worst kernel ratio
@ anywhere in the decoder. The Rust loop compiles to **9 instructions per
@ coefficient**:
@
@   ldr r2,[r1] ; ldr r3,[r1,#-2048] ; subs r0,#1 ; sub r6,r3,r2
@   add r2,r2,r3 ; str r6,[r1] ; str r2,[r1,#-2048] ; add r1,#4 ; bne
@
@ because it is a scalar `for i in 0..n` over two separate rows. The reference
@ has a hand-written kernel doing **4 coefficients in 14 instructions** — two
@ `ldmia`, eight ALU ops, two `stmia` — i.e. 3.5 instructions per coefficient
@ instead of 9. This is the same lever that took `pre` (11 vs 15) and `ola`
@ (26 vs 33) to within 1.00-1.23x of C; M/S was simply never disassembled.
@
@ ARITHMETIC — identical to the Rust kernel and the reference:
@   L[i] = L[i] + R[i] ; R[i] = L[i] - R[i]      (wrapping, mod 2^32)
@ The difference is derived from the sum with a shifted operand
@ (`sub r7, r3, r7, lsl #1` => (a+b) - 2b = a - b), so no second add is needed
@ and no extra register holds the old value.
@
@ CONTRACT: `r0` = L, `r1` = R, `r2` = coefficient count. Both buffers are
@ word-aligned and at least `n` i32 long; `n` is a multiple of 4 (the caller's
@ bound is `nb * BANDW`, i.e. a multiple of 16); `n == 0` writes nothing.
@
@ The multiplies this kernel does not have are the point: M/S is pure ALU, so
@ unlike the other four kernels there is no `Rs` operand-order question here.

    .syntax unified
    .arm

    .section .iwram,"ax",%progbits
    .global uli_ms_arm
    .type   uli_ms_arm, %function

uli_ms_arm:
    @ r0 = L cursor, r1 = R cursor, r2 = remaining coefficients
    push    {{r4-r10}}

.Lms_loop:
    ldmia   r0, {{r3-r6}}                @ a0..a3
    ldmia   r1, {{r7-r10}}               @ b0..b3
    add     r3, r3, r7                 @ L = a + b
    sub     r7, r3, r7, lsl #1         @ R = (a+b) - 2b = a - b
    add     r4, r4, r8
    sub     r8, r4, r8, lsl #1
    add     r5, r5, r9
    sub     r9, r5, r9, lsl #1
    add     r6, r6, r10
    sub     r10, r6, r10, lsl #1
    stmia   r0!, {{r3-r6}}
    stmia   r1!, {{r7-r10}}
    subs    r2, r2, #4
    bgt     .Lms_loop

    pop     {{r4-r10}}
    bx      lr

    .size   uli_ms_arm, .-uli_ms_arm
