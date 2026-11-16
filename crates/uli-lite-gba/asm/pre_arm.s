@ uli_pre_arm — pre-twiddle (IMDCT pre-rotation), the reference's cursor shape.
@ Bit-exact with `uli_lite_gba::pre` (which is bit-exact with the core and the
@ C reference).
@
@ WHY: in-frame `pre` was 26,458 at the gate frame = 13,229/channel = 1.36x the
@ C row, the worst kernel ratio left. The disassembly showed it is NOT
@ address-rebuilt and NOT spilling — it is 15 instructions per output pair with
@ zero stack traffic and folded addressing, i.e. LLVM did well — but the
@ reference's own loop is **10**. The difference is bookkeeping, not arithmetic:
@
@   ours (LLVM)                          reference (braces written out)
@   add  r7, r0, r3        <- stream 2   ldr  r4, [r1], #8       <- cursor
@   ldr  ip, [r0, -r3]     <- stream 1   ldr  r5, [r2], #-8      <- cursor
@   ldr  lr, [r2, #-4]     <- pre 1/2    ldmia ip! (r6, r7)      <- cursor
@   ldr  r8, [r2], #8      <- pre 2/2    ...
@   sub  r3, r3, #8        <- index      ...
@   ldr  r7, [r7, #2044]   <- stream 2   ...
@   cmn  r3, #2048         <- bound      ...
@   ...                                  ...
@   str  r6, [r1, #-4]     <- store 1/2  stmia r3! (r10, r11)   <- one store
@   str  r5, [r1], #8      <- store 2/2  ...
@   bne                                   cmp r3, lr ; blo
@
@ Four streams, four post-indexed cursors, one `stmia` per output pair and a
@ pointer bound: **11 instructions per pair against 15**, with no address
@ arithmetic at all. The four multiplies are the algorithm's own and are
@ irreducible — a complex product is four `smull`/`smlal` chains, and on an
@ ARM7TDMI a multiply blocks the pipeline rather than overlapping, so the only
@ lever here is the instruction count around it.
@
@ ARITHMETIC — identical to the Rust kernel and the C reference:
@   re = hi32(xr*wr - xi*wi) ; im = hi32(xr*wi + xi*wr)
@   with xr = X[2k] forward and xi = X[M-1-2k] backward, and (wr, wi) = PRE[2k..]
@   Q30/Q31 pairs. Imag first with +wi, then `rsb` negates wi for the real part —
@   the reference's ordering, so every product's rounding is unchanged.
@
@ REGISTER MAP: four cursors and four lanes, nothing else.
@   r0  X cursor, forward, +8          r1  &X[M-1] cursor, backward, -8
@   r2  PRE cursor, +8                 r3  z cursor, +8
@   lr  z end (z + 2048 bytes)
@   r4  xr    r5  xi    r6  wr    r7  wi    r8  -wi
@   r9  multiply lo (garbage)          r10, r11  the two results (hi words)

    .syntax unified
    .arm

    .section .iwram,"ax",%progbits
    .global uli_pre_arm
    .type   uli_pre_arm, %function

uli_pre_arm:
    @ r0 = X (M i32), r1 = z (2H i32), r2 = PRE (Q30 pairs)
    push    {{r4-r11, lr}}

    mov     r3, r1                     @ z cursor
    mov     r1, r0
    add     r1, r1, #2048              @ one past X[M-1]
    sub     r1, r1, #4                 @ &X[M-1]
    add     lr, r3, #2048              @ z end = z + 512 words

.Lpre_loop:
    ldr     r4, [r0], #8               @ xr = X[2k]
    ldr     r5, [r1], #-8              @ xi = X[M-1-2k]
    ldmia   r2!, {{r6, r7}}            @ wr, wi
    rsb     r8, r7, #0                 @ -wi
    smull   r9, r10, r6, r4            @ wr*xr
    smlal   r9, r10, r8, r5            @ + (-wi)*xi  -> re
    smull   r9, r11, r7, r4            @ wi*xr
    smlal   r9, r11, r6, r5            @ + wr*xi     -> im
    stmia   r3!, {{r10, r11}}          @ z[2k] = re, z[2k+1] = im
    cmp     r3, lr
    blo     .Lpre_loop

    pop     {{r4-r11, lr}}
    bx      lr

    .size   uli_pre_arm, .-uli_pre_arm
