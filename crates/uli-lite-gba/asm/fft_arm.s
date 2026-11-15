@ uli_fft_arm — the 256-point radix-4 DIF FFT, hand-written ARM mode,
@ bit-exact with `uli_lite::imdct::fft`, placed in `.iwram`.
@
@ PROVENANCE: written for this repository against our own oracle
@ (`crates/uli-lite/src/imdct.rs`); not adapted from `gba-audio`. Twinned
@ bit-exactly against the Rust reference on the host and on target. PR C-1,
@ PLAN-OF-RECORD.md.
@
@ WHY (PR C attribution): the Rust ARM-mode `fft` already emits the optimal
@ 6 SMULL + 6 SMLAL per butterfly; it is short of REGISTERS, not
@ instructions, and LLVM spills 66 intermediates. This file is a hand
@ allocation that spills FOUR words per butterfly (t0 and t2, the two sums
@ only out2 needs) instead of everything LLVM cannot keep live.
@
@ ARITHMETIC — must equal the Rust reference bit-for-bit:
@   hi32(v)      = v >> 32
@   mul32(a,b)   = SMULL lo,hi,a,b
@   hi32(a*b + c*d) = SMULL lo,hi,a,b ; SMLAL lo,hi,c,d ; take hi
@       (full 64-bit accumulation keeps low-word carries; hence SMLAL, never
@        a high-word add)
@   cmul(Yr,Yi,wr,wi) = ( hi32(Yr*wr - Yi*wi), hi32(Yr*wi + Yi*wr) )
@   The real part's `-wi` is realised by computing the IMAG part first with
@   +wi, then negating the wi register once (rsb) for the real part.
@
@ STRUCTURE (n = H = 256, buffer = 2n = 512 i32 words):
@   General stages l = 256, 64, 16 (q = l/4 = 64,16,4; stride = 2l words;
@   t = 2q words between p, p1, p2, p3):
@     for j in 0..q:
@       load the 3 twiddle pairs from TW (6 words per j; 84 over 3 stages)
@       for p = 2j; p < 512; p += stride: butterfly at (p, p+t, p+2t, p+3t)
@   Tail stage l = 4 (q = 1, stride = 8 words, NO twiddles):
@     for p = 0; p < 512; p += 8: butterfly, every output ASR #2.
@
@ BUTTERFLY (general stage):
@   t0 = A+C ; t1 = A-C ; t2 = B+D ; t3 = B-D     (componentwise)
@   x[p ] = t0 + t2                 (no twiddle)
@   x[p1] = cmul(t1r+t3i, t1i-t3r, w1)
@   x[p2] = cmul(t0r-t2r, t0i-t2i, w2)
@   x[p3] = cmul(t1r-t3i, t1i+t3r, w3)
@   Tail (l==4): x[p+k] = the four combinations >> 2 (arithmetic shift).
@
@ REGISTER MAP (the allocation this file exists to make):
@   r0,r1   Yr,Yi  — the current output's operands (formed from t-values)
@   r2,r3   the current output's twiddle pair (wr,wi)
@   r4,r5   t1r,t1i             (needed by out1 and out3)
@   r6,r7   t3r,t3i             (needed by out1 and out3)
@   r8,r9   SMULL/SMLAL lo,hi
@   r10,r11 x[p..] store scratch / load use
@   r12     x base (byte address)
@   lr      store-address scratch
@   Stack frame (push r4-r11,lr; then 40 bytes):
@     [sp,#0]  tw pointer (byte address into TW)
@     [sp,#4]  stride (words = 2l)
@     [sp,#8]  bound (words) = 512
@     [sp,#12] q
@     [sp,#16] l
@     [sp,#20] p (words)
@     [sp,#24] t = 2q (words)
@     [sp,#28] j
@     [sp,#32] t0r   (spilled; only out2 uses t0)
@     [sp,#36] t0i
@     [sp,#40] t2r   (only out2 uses t2)
@     [sp,#44] t2i
@   (frame is 48 bytes; sp 8-aligned via the push of 9 regs = 36 + 48 = 84? we
@    only need alignment for the AAPCS stack; the FPU is absent so 4-byte
@    alignment suffices on ARM7TDMI. We keep 4-byte alignment.)

    .syntax unified
    .arm

    @ BUDGET (C-3, 2026-11-15): v1 is a superseded comparison arm and v1
    @ lives in .text (cartridge ROM) so it does not consume IWRAM. Its
    @ recorded cycle numbers were taken while it was in .iwram; the IWRAM
    @ budget now goes to the shipping kernel (v4) and the hot state.
    .section .text,"ax",%progbits
    .global uli_fft_arm
    .type   uli_fft_arm, %function

    .equ    F_TW,   0
    .equ    F_STR,  4
    .equ    F_BND,  8
    .equ    F_Q,    12
    .equ    F_L,    16
    .equ    F_P,    20
    .equ    F_T2Q,  24
    .equ    F_J,    28
    .equ    F_T0R,  32
    .equ    F_T0I,  36
    .equ    F_T2R,  40
    .equ    F_T2I,  44
    .equ    FRAME,  48

uli_fft_arm:
    @ r0 = x (i32*, 2*H words) ; r1 = TW (i32*, 504 words)
    push    {{r4-r11, lr}}
    sub     sp, sp, #FRAME

    mov     r12, r0                    @ x base (byte address)
    str     r1, [sp, #F_TW]            @ tw pointer (byte address)

    mov     r1, #512
    str     r1, [sp, #F_BND]

    mov     r1, #256
    str     r1, [sp, #F_L]             @ l = 256 (kept ONLY in the frame; r9
                                       @ is the SMULL hi in the butterfly)

@ ==========================================================================
@ General-stage loop: l = 256, 64, 16.
@ ==========================================================================
.Lstage:
    ldr     r1, [sp, #F_L]             @ l
    mov     r1, r1, lsr #2             @ q = l/4
    str     r1, [sp, #F_Q]
    add     r1, r1, r1                 @ t = 2q (words)
    str     r1, [sp, #F_T2Q]
    ldr     r1, [sp, #F_L]
    mov     r1, r1, lsl #1             @ stride = 2l (words)
    str     r1, [sp, #F_STR]

    mov     r1, #0
    str     r1, [sp, #F_J]             @ j = 0

.Lj_loop:
    ldr     r1, [sp, #F_J]
    ldr     r2, [sp, #F_Q]
    cmp     r1, r2
    bge     .Lstage_next

    mov     r1, r1, lsl #1             @ p = 2j
    str     r1, [sp, #F_P]

.Lk_loop:
    ldr     r2, [sp, #F_P]             @ p (words)
    ldr     r3, [sp, #F_BND]
    cmp     r2, r3
    bge     .Lj_next

    @ ---- load A=(r4,r5) B=(r6,r7) C=(r10,r11) D=(lr,r0) ----
    ldr     r1, [sp, #F_T2Q]           @ t (words)
    add     r3, r2, r1                 @ p1 = p + t
    add     lr, r12, r2, lsl #2        @ &x[p]
    ldr     r4, [lr, #0]               @ ar
    ldr     r5, [lr, #4]               @ ai

    add     lr, r12, r3, lsl #2        @ &x[p1]
    add     r3, r3, r1                 @ p2 = p1 + t
    ldr     r6, [lr, #0]               @ br
    ldr     r7, [lr, #4]               @ bi

    add     r2, r12, r3, lsl #2        @ &x[p2]
    add     r3, r3, r1                 @ p3 = p2 + t
    ldr     r10, [r2, #0]              @ cr
    ldr     r11, [r2, #4]              @ ci
    str     r10, [sp, #F_T0R]          @ stash C (cr) for the t-computes
    str     r11, [sp, #F_T0I]

    add     r2, r12, r3, lsl #2        @ &x[p3]
    ldr     lr, [r2, #0]               @ dr
    ldr     r0, [r2, #4]               @ di

    @ ---- compute the four sums ----
    @ t1 = A - C  -> r4,r5
    @ t3 = B - D  -> r6,r7
    @ t0 = A + C  -> spill F_T0R/F_T0I
    @ t2 = B + D  -> r10,r11 then spill to F_T2R/F_T2I
    ldr     r2, [sp, #F_T0R]           @ cr
    ldr     r3, [sp, #F_T0I]           @ ci
    add     r10, r4, r2                @ t0r = ar+cr
    add     r11, r5, r3                @ t0i = ai+ci
    str     r10, [sp, #F_T0R]
    str     r11, [sp, #F_T0I]
    sub     r4, r4, r2                 @ t1r = ar-cr
    sub     r5, r5, r3                 @ t1i = ai-ci

    add     r10, r6, lr                @ t2r = br+dr
    add     r11, r7, r0                @ t2i = bi+di
    str     r10, [sp, #F_T2R]
    str     r11, [sp, #F_T2I]
    sub     r6, r6, lr                 @ t3r = br-dr
    sub     r7, r7, r0                 @ t3i = bi-di

    @ ---------------- out0 = t0 + t2 (no twiddle) ----------------
    ldr     r2, [sp, #F_T0R]
    ldr     r3, [sp, #F_T0I]
    ldr     r0, [sp, #F_T2R]
    ldr     r1, [sp, #F_T2I]
    ldr     lr, [sp, #F_P]
    add     lr, r12, lr, lsl #2        @ &x[p]
    add     r2, r2, r0                 @ t0r+t2r
    mov     r2, r2, asr #2             @ x[p]   = (t0r+t2r) >> 2
    str     r2, [lr, #0]
    add     r3, r3, r1                 @ t0i+t2i
    mov     r3, r3, asr #2             @ x[p+1] = (t0i+t2i) >> 2
    str     r3, [lr, #4]

    @ ---------------- out1 = cmul(t1r+t3i, t1i-t3r, w1) ----------------
    ldr     r10, [sp, #F_TW]
    ldr     r2, [r10, #0]              @ w1r
    ldr     r3, [r10, #4]              @ w1i
    add     r0, r4, r7                 @ Yr = t1r + t3i
    sub     r1, r5, r6                 @ Yi = t1i - t3r
    @ imag = hi32(Yr*wi + Yi*wr)   (uses +wi)
    smull   r8, r9, r0, r3             @ Yr*wi
    smlal   r8, r9, r1, r2             @ + Yi*wr
    ldr     r10, [sp, #F_T2Q]
    ldr     lr, [sp, #F_P]
    add     r10, lr, r10               @ p1 = p + t
    add     lr, r12, r10, lsl #2       @ &x[p1]
    str     r9, [lr, #4]               @ x[p1+1] = imag
    @ real = hi32(Yr*wr + Yi*(-wi))  (negate wi once)
    rsb     r3, r3, #0
    smull   r8, r9, r0, r2             @ Yr*wr
    smlal   r8, r9, r1, r3             @ + Yi*(-wi)
    str     r9, [lr, #0]               @ x[p1] = real

    @ ---------------- out2 = cmul(t0r-t2r, t0i-t2i, w2) ----------------
    ldr     r10, [sp, #F_TW]
    ldr     r2, [r10, #8]              @ w2r
    ldr     r3, [r10, #12]             @ w2i
    ldr     r0, [sp, #F_T0R]
    ldr     r1, [sp, #F_T0I]
    ldr     r10, [sp, #F_T2R]
    ldr     lr, [sp, #F_T2I]
    sub     r0, r0, r10                @ Yr = t0r - t2r
    sub     r1, r1, lr                 @ Yi = t0i - t2i
    smull   r8, r9, r0, r3             @ Yr*wi
    smlal   r8, r9, r1, r2             @ + Yi*wr
    ldr     r10, [sp, #F_T2Q]          @ t
    add     r10, r10, r10              @ 2t
    ldr     lr, [sp, #F_P]             @ p
    add     r10, lr, r10               @ p2 = p + 2t
    add     lr, r12, r10, lsl #2
    str     r9, [lr, #4]               @ x[p2+1] = imag
    rsb     r3, r3, #0
    smull   r8, r9, r0, r2
    smlal   r8, r9, r1, r3
    str     r9, [lr, #0]               @ x[p2] = real

    @ ---------------- out3 = cmul(t1r-t3i, t1i+t3r, w3) ----------------
    ldr     r10, [sp, #F_TW]
    ldr     r2, [r10, #16]             @ w3r
    ldr     r3, [r10, #20]             @ w3i
    sub     r0, r4, r7                 @ Yr = t1r - t3i
    add     r1, r5, r6                 @ Yi = t1i + t3r
    smull   r8, r9, r0, r3
    smlal   r8, r9, r1, r2
    ldr     lr, [sp, #F_T2Q]           @ t
    add     r10, lr, lr                @ 2t
    add     r10, r10, lr               @ 3t
    ldr     lr, [sp, #F_P]             @ p
    add     r10, lr, r10               @ p3 = p + 3t
    add     lr, r12, r10, lsl #2
    str     r9, [lr, #4]               @ x[p3+1] = imag
    rsb     r3, r3, #0
    smull   r8, r9, r0, r2
    smlal   r8, r9, r1, r3
    str     r9, [lr, #0]               @ x[p3] = real

    @ ---- advance p by stride ----
    ldr     r2, [sp, #F_P]
    ldr     r3, [sp, #F_STR]
    add     r2, r2, r3
    str     r2, [sp, #F_P]
    b       .Lk_loop

.Lj_next:
    ldr     r1, [sp, #F_J]
    ldr     r2, [sp, #F_TW]
    add     r2, r2, #24                @ TW += 6 words
    str     r2, [sp, #F_TW]
    add     r1, r1, #1
    str     r1, [sp, #F_J]
    b       .Lj_loop

.Lstage_next:
    ldr     r1, [sp, #F_L]
    mov     r1, r1, lsr #2             @ l >>= 2
    str     r1, [sp, #F_L]
    cmp     r1, #16
    bge     .Lstage

@ ==========================================================================
@ Tail stage l = 4: p = 0, 8, 16, ... < 512. No twiddles; every output the
@ four t-combinations >> 2. Same butterfly sums, applied 8 words at a time.
@ ==========================================================================
    mov     r2, #0
    str     r2, [sp, #F_P]
.Ltail_loop:
    ldr     r2, [sp, #F_P]
    ldr     r3, [sp, #F_BND]
    cmp     r2, r3
    bge     .Ldone

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
    add     r8, r4, r10                @ t0r
    add     r9, r5, r11                @ t0i
    sub     r4, r4, r10                @ t1r
    sub     r5, r5, r11                @ t1i
    add     r10, r6, r0                @ t2r
    add     r11, r7, r1                @ t2i
    sub     r6, r6, r0                 @ t3r
    sub     r7, r7, r1                 @ t3i

    @ x[p  ] = (t0+t2)>>2 ; x[p+1] = (t0i+t2i)>>2
    add     r0, r8, r10
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #0]
    str     r1, [lr, #4]
    @ x[p+2] = (t1r+t3i)>>2 ; x[p+3] = (t1i-t3r)>>2
    add     r0, r4, r7
    sub     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #8]
    str     r1, [lr, #12]
    @ x[p+4] = (t0-t2)>>2 ; x[p+5] = (t0i-t2i)>>2
    sub     r0, r8, r10
    sub     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #16]
    str     r1, [lr, #20]
    @ x[p+6] = (t1r-t3i)>>2 ; x[p+7] = (t1i+t3r)>>2
    sub     r0, r4, r7
    add     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [lr, #24]
    str     r1, [lr, #28]

    ldr     r2, [sp, #F_P]
    add     r2, r2, #8
    str     r2, [sp, #F_P]
    b       .Ltail_loop

.Ldone:
    add     sp, sp, #FRAME
    pop     {{r4-r11, lr}}
    bx      lr

    .size   uli_fft_arm, .-uli_fft_arm
