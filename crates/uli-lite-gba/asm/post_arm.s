@ uli_post_arm — post-twiddle (the IMDCT's second half-rotation), C's loop shape.
@ Bit-exact with `uli_lite_gba::post`, which is bit-exact with the core and with
@ the C reference.
@
@ WHY (2026-11-18): `post` was the **last Rust kernel on the product path** —
@ `pre`, `fft`, `ola` and `ms` are all hand-written now. Fresh attribution with
@ the bit-exact arms (`attrib-post2`) put it at **23,682 cycles at the gate frame
@ and 23,458 mean = 11.3% / 12.9% of the frame**, against the C record's 21,800
@ (its own per-channel row doubled) — the one kernel where the excess is
@ *nameable in instructions* rather than data:
@
@   ours (LLVM, measured)   18 instructions per iteration
@   reference (this file)   14
@
@ The four extra were all addressing, and each maps to one reference idiom:
@
@   add r6,r0,lr,lsl #3 ; ldr r6,[r6,#4]   ->  ldmia r6,{{r6,r7}}    (z pair, -2)
@   ldr sl,[r2,#-4] ; ldr r9,[r2],#8       ->  ldmia ip!,{{r8,r9}}   (twiddles, -1)
@   add r6,r1,r3 ; str r7,[r6,#2044]       ->  str r4,[r5],#-8     (backward Y, -1)
@
@ ARITHMETIC — identical to the Rust kernel and the C reference:
@   Y[2k]       = hi32(zr*wr - zi*wi)
@   Y[M-1-2k]   = -hi32(zr*wi + zi*wr)
@ with (zr, zi) = z[2*PERM[k] ..] and (wr, wi) = POST[2k ..]. The negation is
@ applied to the **high word**, after the accumulate — the reference's order, so
@ every product's rounding is unchanged.
@
@ MULTIPLY OPERAND ORDER: `smull r11,r4,r8,r6` puts the *twiddle* in Rn and the
@ *data* in Rs, because ARM7TDMI multiply cost early-terminates on Rs. Swapping
@ them is bit-identical and measurably slower — the inversion that cost the FFT
@ 3.6% of the whole frame until the 2026-11-16 audit (see `fft_arm_v5.s`).
@ Any edit here keeps data in Rs.
@
@ REGISTER MAP: four cursors, four data lanes, one multiply pair, nothing on the
@ stack except the save set. There is no spare register to spend: the loop is
@ instruction-bound, not register-bound, which is exactly why the reference's
@ shape wins.
@   r0  z (2H i32)          r1  Y forward cursor, +8
@   r3  PERM cursor, +2     r12 POST cursor, +8
@   r5  Y backward cursor, -8            lr  Y end (r1 start + 2048)
@   r6  zr, then the z-pair address      r7  zi
@   r8  wr                  r9  wi       r10 -wi
@   r11, r4  the smull lo/hi pair (r4 is the stored high word)

    .syntax unified
    .arm

    .section .iwram,"ax",%progbits
    .global uli_post_arm
    .type   uli_post_arm, %function

uli_post_arm:
    @ r0 = z, r1 = Y, r2 = PERM, r3 = POST
    push    {{r4-r11, lr}}

    mov     r12, r3                     @ twiddle cursor = POST[0]
    mov     r3, r2                      @ perm cursor = PERM[0]
    add     lr, r1, #2048               @ Y end: 2H words = 2048 bytes
    sub     r5, lr, #4                  @ &Y[M-1] = end - 4 (2044 is not an
                                        @ encodable ARM immediate; end-4 is)

.Lpost_loop:
    ldrh    r6, [r3], #2                @ k's permutation
    add     r6, r0, r6, lsl #3          @ &z[2*PERM[k]]  (8 bytes per pair)
    ldmia   r6, {{r6, r7}}              @ zr, zi
    ldmia   r12!, {{r8, r9}}            @ wr, wi
    rsb     r10, r9, #0                 @ -wi
    smull   r11, r4, r8, r6             @ wr*zr
    smlal   r11, r4, r10, r7            @ + (-wi)*zi  -> re
    str     r4, [r1], #8                @ Y[2k]
    smull   r11, r4, r9, r6             @ wi*zr
    smlal   r11, r4, r8, r7             @ + wr*zi
    rsb     r4, r4, #0                  @ negate the high word
    str     r4, [r5], #-8               @ Y[M-1-2k]
    cmp     r1, lr
    blo     .Lpost_loop

    pop     {{r4-r11, lr}}
    bx      lr

    .size   uli_post_arm, .-uli_post_arm
