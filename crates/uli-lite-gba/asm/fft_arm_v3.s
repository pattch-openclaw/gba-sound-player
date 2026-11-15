@ uli_fft_arm_v3 — block-transfer variant (v2 + ldmia/stmia) of the hand-written ARM FFT.
@ Bit-exact with `fft_arm.s` (v1) and the Rust reference; same arithmetic.
@
@ WHY (PR C-1 probe follow-up): v1 measured 88,205 cycles/call, 3.7% slower than
@ the Rust ARM kernel (85,065) and 2.8x the C record. The register-pressure
@ hypothesis was falsified as the primary lever; the residual was identified as
@ MEMORY TRAFFIC — v1 spills the t0/t2 sums to the stack every butterfly and
@ reloads the loop variables (p, t), the TW pointer and those sums repeatedly
@ (~40 LDR / ~17 STR per butterfly). v2 removes that traffic:
@
@   1. p lives in r2 and t (=2q) in r3 across the whole inner k-loop; neither is
@      reloaded per butterfly.
@   2. NO t-value is spilled. The four sums occupy r4-r11 (t1=(r4,r5),
@      t3=(r6,r7), t0=(r8,r9), t2=(r10,r11)). Outputs are ordered so each pair
@      of sums is consumed and then its own registers become that output's
@      operands:  out0 then out2 use t0,t2 (r8-r11);  out1 then out3 use
@      t1,t3 (r4-r7). SMULL/SMLAL lo,hi are r0,r1; lr is the store address.
@   3. The per-j TW base is derived once per j; each output loads its two
@      twiddle words from it. For l=64 and l=16 v1 reloaded the SAME six words
@      every butterfly AND a per-butterfly TW pointer; v2 computes the pointer
@      once per j and reloads only the two words it needs per output.
@
@ ARITHMETIC — identical to v1 / the Rust reference:
@   hi32(a*b + c*d) = SMULL lo,hi,a,b ; SMLAL lo,hi,c,d ; take hi
@   cmul(Yr,Yi,wr,wi) = ( hi32(Yr*wr - Yi*wi), hi32(Yr*wi + Yi*wr) ), imag first
@   with +wi, then rsb negates wi for the real part.
@   General stage: out0 = (t0+t2)>>2 ; out1 = cmul(t1r+t3i, t1i-t3r, w1) ;
@   out2 = cmul(t0r-t2r, t0i-t2i, w2) ; out3 = cmul(t1r-t3i, t1i+t3r, w3).
@   Tail (l==4): the four t-combinations >> 2, no twiddles.
@
@ REGISTER MAP:
@   r0,r1   SMULL/SMLAL lo,hi (and scalar temps)
@   r2      p (i32 words) — live across the k-loop
@   r3      t = 2q (i32 words) — live across the k-loop
@   r4,r5   t1  -> out1 Y, then out3 Y
@   r6,r7   t3  -> out1 w, then out3 w
@   r8,r9   t0  -> out2 Y
@   r10,r11 t2  -> out2 w
@   r12     x base (byte address)
@   lr      store-address scratch
@   Stack (push r4-r11,lr; 32 bytes):
@     [sp,#0]  bound (words) = 512
@     [sp,#4]  l
@     [sp,#8]  q
@     [sp,#12] j
@     [sp,#16] TW base for the current stage (bytes)
@     [sp,#20] stride (words = 2l)
@     [sp,#24] running TW pointer (bytes), advanced by q*24 at stage end

    .syntax unified
    .arm

    @ BUDGET (C-3, 2026-11-15): v3 is superseded by v4 (v4 = v3 + the trivial
    @ butterfly) and lives in .text so IWRAM goes to the hot state instead.
    @ Its recorded numbers were taken in .iwram.
    .section .text,"ax",%progbits
    .global uli_fft_arm_v3
    .type   uli_fft_arm_v3, %function

    .equ    F_BND,   0
    .equ    F_L,     4
    .equ    F_Q,     8
    .equ    F_J,     12
    .equ    F_TWB,   16
    .equ    F_STR,   20
    .equ    F_TWRUN, 24
    .equ    F_TWJ,   28
    .equ    FRAME,   40

uli_fft_arm_v3:
    @ r0 = x (i32*, 2*H words) ; r1 = TW (i32*, 504 words)
    push    {{r4-r11, lr}}
    sub     sp, sp, #FRAME

    mov     r12, r0                    @ x base
    str     r1, [sp, #F_TWRUN]

    mov     r0, #512
    str     r0, [sp, #F_BND]
    mov     r0, #256
    str     r0, [sp, #F_L]

@ ==========================================================================
@ General-stage loop: l = 256, 64, 16.
@ ==========================================================================
.Lstage_v3:
    ldr     r0, [sp, #F_L]
    mov     r0, r0, lsr #2             @ q = l/4
    str     r0, [sp, #F_Q]
    ldr     r0, [sp, #F_L]
    mov     r0, r0, lsl #1             @ stride = 2l (words)
    str     r0, [sp, #F_STR]

    ldr     r0, [sp, #F_TWRUN]
    str     r0, [sp, #F_TWB]           @ stage base TW (bytes)

    mov     r0, #0
    str     r0, [sp, #F_J]

.Lj_loop_v3:
    ldr     r0, [sp, #F_J]
    ldr     r1, [sp, #F_Q]
    cmp     r0, r1
    bge     .Lstage_next_v3

    mov     r2, r0, lsl #1             @ r2 = p = 2j
    ldr     r3, [sp, #F_Q]
    mov     r3, r3, lsl #1             @ r3 = t = 2q
    add     r1, r0, r0, lsl #1         @ 3j
    ldr     r2, [sp, #F_TWB]           @ (r2 reloaded to p just below)
    add     r2, r2, r1, lsl #3         @ stage base + j*24 bytes
    str     r2, [sp, #F_TWJ]
    mov     r2, r0, lsl #1             @ restore r2 = p

    @ Per-j twiddle base = stage base + j*24, computed ONCE here so each output
    @ is a plain ldr from [sp,#F_TWJ] rather than a 3j/shift/add chain.

.Lk_loop_v3:
    cmp     r2, #512                   @ p < 512 ?  (512 = 0x200, encodable)
    bge     .Lj_next_v3

    @ ---- load A=(r4,r5) B=(r6,r7) C=(r8,r9) D=(r10,r11) via block loads ----
    add     lr, r12, r2, lsl #2        @ &x[p]
    ldmia   lr, {{r4, r5}}               @ a = (ar, ai)

    add     r1, r2, r3                 @ p+t
    add     lr, r12, r1, lsl #2        @ &x[p+t]
    ldmia   lr, {{r6, r7}}               @ b = (br, bi)

    add     r0, r1, r3                 @ p+2t
    add     lr, r12, r0, lsl #2        @ &x[p+2t]
    ldmia   lr, {{r8, r9}}               @ c = (cr, ci)

    add     r1, r0, r3                 @ p+3t
    add     lr, r12, r1, lsl #2        @ &x[p+3t]
    ldmia   lr, {{r10, r11}}             @ d = (dr, di)

    @ ---- sums ----
    add     r0, r4, r8                 @ t0r
    sub     r4, r4, r8                 @ t1r
    add     r1, r5, r9                 @ t0i
    sub     r5, r5, r9                 @ t1i
    mov     r8, r0                     @ t0r -> r8
    mov     r9, r1                     @ t0i -> r9
    add     r0, r6, r10                @ t2r
    sub     r6, r6, r10                @ t3r
    add     r1, r7, r11                @ t2i
    sub     r7, r7, r11                @ t3i
    mov     r10, r0                    @ t2r -> r10
    mov     r11, r1                    @ t2i -> r11
    @ t0=(r8,r9) t1=(r4,r5) t2=(r10,r11) t3=(r6,r7)

    @ ---------------- out0 = (t0+t2) >> 2 ----------------
    add     r0, r8, r10
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    add     lr, r12, r2, lsl #2
    stmia   lr, {{r0, r1}}               @ x[p], x[p+1]

    @ ---------------- out2 = cmul(t0r-t2r, t0i-t2i, w2) ----------------
    @ Y into r8,r9 (t0 dead); w2 into r10,r11 (t2 dead).
    sub     r8, r8, r10                @ Yr = t0r - t2r
    sub     r9, r9, r11                @ Yi = t0i - t2i
    ldr     r1, [sp, #F_TWJ]
    ldr     r10, [r1, #8]              @ w2r
    ldr     r11, [r1, #12]             @ w2i
    smull   r0, r1, r8, r11            @ Yr*wi
    smlal   r0, r1, r9, r10            @ + Yi*wr  -> imag
    add     lr, r2, r3                 @ p+t
    add     lr, lr, r3                 @ p+2t
    add     lr, r12, lr, lsl #2        @ &x[p+2t]
    str     r1, [lr, #4]               @ x[p2+1] = imag
    rsb     r11, r11, #0               @ -wi
    smull   r0, r1, r8, r10
    smlal   r0, r1, r9, r11
    str     r1, [lr, #0]               @ x[p2] = real

    @ ---------------- half B: out1 and out3 both need t1,t3 -------------
    @ Form BOTH Y before either is overwritten. t1=(r4,r5) t3=(r6,r7). The
    @ t0/t2 half's registers r8-r11 are dead now, so Y3 lands there.
    add     r0, r4, r7                 @ Y1r = t1r + t3i   (temp)
    sub     r1, r5, r6                 @ Y1i = t1i - t3r   (temp)
    sub     r8, r4, r7                 @ Y3r = t1r - t3i
    add     r9, r5, r6                 @ Y3i = t1i + t3r
    mov     r4, r0                     @ Y1r -> r4
    mov     r5, r1                     @ Y1i -> r5
    @ Y1=(r4,r5) Y3=(r8,r9); t3=(r6,r7) and t2-regs (r10,r11) free for twiddles.

    @ ---------------- out1 = cmul(Y1, w1) ----------------
    ldr     r1, [sp, #F_TWJ]
    ldr     r6, [r1, #0]               @ w1r
    ldr     r7, [r1, #4]               @ w1i
    smull   r0, r1, r4, r7             @ Y1r*wi
    smlal   r0, r1, r5, r6             @ + Y1i*wr  -> imag
    add     lr, r2, r3                 @ p+t
    add     lr, r12, lr, lsl #2        @ &x[p+t]
    str     r1, [lr, #4]
    rsb     r7, r7, #0                 @ -wi
    smull   r0, r1, r4, r6
    smlal   r0, r1, r5, r7             @ real
    str     r1, [lr, #0]

    @ ---------------- out3 = cmul(Y3, w3) ----------------
    ldr     r1, [sp, #F_TWJ]
    ldr     r10, [r1, #16]             @ w3r
    ldr     r11, [r1, #20]             @ w3i
    smull   r0, r1, r8, r11            @ Y3r*wi
    smlal   r0, r1, r9, r10            @ + Y3i*wr -> imag
    add     lr, r2, r3                 @ p+t
    add     lr, lr, r3                 @ p+2t
    add     lr, lr, r3                 @ p+3t
    add     lr, r12, lr, lsl #2        @ &x[p+3t]
    str     r1, [lr, #4]
    rsb     r11, r11, #0
    smull   r0, r1, r8, r10
    smlal   r0, r1, r9, r11
    str     r1, [lr, #0]

    @ ---- advance p by stride = 4t (t=2q=l/2, stride=2l=4t); no frame load ----
    add     r2, r2, r3, lsl #2
    b       .Lk_loop_v3

.Lj_next_v3:
    ldr     r0, [sp, #F_J]
    add     r0, r0, #1
    str     r0, [sp, #F_J]
    b       .Lj_loop_v3

.Lstage_next_v3:
    ldr     r0, [sp, #F_L]
    mov     r0, r0, lsr #2
    str     r0, [sp, #F_L]
    ldr     r0, [sp, #F_Q]
    add     r0, r0, r0, lsl #1         @ 3q
    mov     r0, r0, lsl #3             @ q*24
    ldr     r1, [sp, #F_TWRUN]
    add     r1, r1, r0
    str     r1, [sp, #F_TWRUN]
    ldr     r0, [sp, #F_L]
    cmp     r0, #16
    bge     .Lstage_v3
    b       .Ltail_v3

.Ltail_v3:
    @ ==========================================================================
    @ Tail stage l = 4: p = 0, 8, 16, ... < 512. No twiddles; every output is the
    @ four t-combinations >> 2. p in r2, x base in r12.
    @ ==========================================================================
    mov     r2, #0
.Ltail_loop_v3:
    ldr     r0, [sp, #F_BND]
    cmp     r2, r0
    bge     .Ldone_v3

    add     lr, r12, r2, lsl #2        @ &x[p]
    ldr     r4, [lr, #0]               @ ar
    ldr     r5, [lr, #4]               @ ai
    ldr     r6, [lr, #8]               @ br
    ldr     r7, [lr, #12]              @ bi
    ldr     r10, [lr, #16]             @ cr
    ldr     r11, [lr, #20]             @ ci
    ldr     r0, [lr, #24]              @ dr
    ldr     r1, [lr, #28]              @ di

    @ t0=A+C -> r8,r9 ; t1=A-C -> r4,r5 ; t2=B+D -> r10,r11 ; t3=B-D -> r6,r7
    add     r8, r4, r10
    add     r9, r5, r11
    sub     r4, r4, r10
    sub     r5, r5, r11
    add     r10, r6, r0
    add     r11, r7, r1
    sub     r6, r6, r0
    sub     r7, r7, r1

    add     r0, r8, r10                @ t0+t2 -> x[p],x[p+1]
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #0]
    str     r1, [lr, #4]
    add     r0, r4, r7                 @ t1r+t3i -> x[p+2],x[p+3]
    sub     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #8]
    str     r1, [lr, #12]
    sub     r0, r8, r10                @ t0-t2 -> x[p+4],x[p+5]
    sub     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #16]
    str     r1, [lr, #20]
    sub     r0, r4, r7                 @ t1r-t3i -> x[p+6],x[p+7]
    add     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #24]
    str     r1, [lr, #28]

    add     r2, r2, #8
    b       .Ltail_loop_v3

.Ldone_v3:
    add     sp, sp, #FRAME
    pop     {{r4-r11, lr}}
    bx      lr

    .size   uli_fft_arm_v3, .-uli_fft_arm_v3
