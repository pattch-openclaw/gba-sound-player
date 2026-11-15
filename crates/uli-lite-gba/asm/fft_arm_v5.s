@ uli_fft_arm_v5 — v4 (block transfers + trivial butterfly) with the address
@ arithmetic and the sum formation taken from the reference's shape.
@ Bit-exact with v1/v2/v3/v4 and the Rust reference.
@
@ WHY (item 1 of the critical path, after Step 1a): in-frame the FFT is
@ 40,518 cycles/channel — 35% of the frame and 1.29x the C record's 31,400 — and
@ it is the only single block large enough to close the ~14,400 cycles the <=85%
@ headroom target still needs. Two measured differences remained:
@
@   1. A..D / OUTPUT TRAFFIC. v4 keeps a WORD index p and rebuilds every address
@      from (p, t, x) with a shift chain: `add lr,r2,r3 ; add lr,lr,r3 ;
@      add lr,r12,lr,lsl #2` — 2 to 4 ALU ops per output, 12 for the four loads
@      plus 17 for the four stores. v5 keeps an ABSOLUTE BYTE CURSOR in r2 and
@      the element stride T in r3, so a group is one `ldmia` at r2 and three
@      `add r0,r0,r3` steps, and each store address is one or two `add`s from
@      (r2, r3) with no shift-per-word conversion. 12 loads + 17 store-address
@      ops -> 7 + 11.
@   2. SUM FORMATION. v4 forms each of the eight t values with a separate
@      `add`/`sub` plus a `mov` into its final register: 12 instructions, four
@      of them moves. The reference instead forms the sum destructively into
@      one operand and derives the difference from the sum with a shifted
@      operand (`sub d, sum, e, lsl #1`), which needs no move: 8 instructions.
@      v5 uses that, and forms both half-B Y values from the same trick (4
@      instructions where v4 needs 6).
@
@ Total per radix-4 butterfly: **80 -> 60 instructions**, and every twiddle load
@ is now `[r12, #imm]` off a register-held pointer instead of `ldr [sp,#F_TWJ]`
@ followed by an indexed load (r12 holds TWJ for the whole k-loop; the cursor is
@ absolute, so the x base is not needed inside the loops and frees the register).
@
@ ARITHMETIC — identical to v1/v2/v3/v4 and the Rust reference:
@   hi32(a*b + c*d) = SMULL lo,hi,a,b ; SMLAL lo,hi,c,d ; take hi
@   cmul(Yr,Yi,wr,wi) = ( hi32(Yr*wr - Yi*wi), hi32(Yr*wi + Yi*wr) ), imag first,
@   then rsb negates wi for the real part (the C reference's ordering, so the
@   rounding of every product is unchanged).
@   t0=A+C, t1=A-C, t2=B+D, t3=B-D  (derived as above; t0r = ar+cr into r8 with
@   ar still live, then t1r = 2*ar - t0r as `rsb r4, r8, r4, lsl #1`)
@   out0 = (t0+t2)>>2 ; out1 = cmul(t1r+t3i, t1i-t3r, w1) ;
@   out2 = cmul(t0r-t2r, t0i-t2i, w2) ; out3 = cmul(t1r-t3i, t1i+t3r, w3).
@   Tail (l==4): the four t-combinations >> 2, no twiddles.
@   Trivial (j==0, twiddle == (2^30,0)x3): every output is a sum/difference >> 2.
@
@ REGISTER MAP:
@   r2      P — absolute BYTE cursor of the current group; live across the k-loop
@   r3      T — element stride in BYTES (T = 2l); live across the k-loop
@   r12     TWJ — absolute bytes of this j's twiddle pair; live across the k-loop
@   lr      ADDRESS SCRATCH for the stores. This is deliberate: `smull` writes a
@           register PAIR, so an address held in r0 or r1 is destroyed by the
@           second `smull` of a complex multiply. v4 holds its store address in
@           lr for exactly this reason; an earlier v5 draft used r0 and stored
@           every real part to a pointer built from a multiply's low word.
@   r0      SMULL lo (garbage), and the bound load at each loop head
@   r1      SMULL hi — the stored result
@   r4,r5   A -> t1 -> Y1
@   r6,r7   B -> t3 -> w1 temps
@   r8,r9   C -> t0 -> Y2/Y3
@   r10,r11 D -> t2 -> w2/w3 temps
@   Stack (push r4-r11,lr; 32 bytes): G_L, G_Q, G_T, G_TWB, G_J, G_TWRUN, G_X
@   are touched once per stage or per j; G_END (x + 2048) is reloaded once per
@   butterfly because every general-purpose register is spoken for above.

    .syntax unified
    .arm

    .section .iwram,"ax",%progbits
    .global uli_fft_arm_v5
    .type   uli_fft_arm_v5, %function

    .equ    G_L,     0
    .equ    G_Q,     4
    .equ    G_T,     8
    .equ    G_TWB,   12
    .equ    G_J,     16
    .equ    G_TWRUN, 20
    .equ    G_X,     24
    .equ    G_END,   28
    .equ    GFRAME,  32

uli_fft_arm_v5:
    @ r0 = x (i32*, 2*H words) ; r1 = TW (i32*, 504 words)
    push    {{r4-r11, lr}}
    sub     sp, sp, #GFRAME

    str     r1, [sp, #G_TWRUN]         @ running stage twiddle base
    add     r1, r0, #2048              @ end = x + 512 words (bytes)
    str     r1, [sp, #G_END]
    str     r0, [sp, #G_X]

    mov     r0, #256
    str     r0, [sp, #G_L]

@ ==========================================================================
@ General-stage loop: l = 256, 64, 16.
@ ==========================================================================
.Lstage5:
    ldr     r0, [sp, #G_L]
    mov     r0, r0, lsr #2             @ q = l/4
    str     r0, [sp, #G_Q]
    ldr     r0, [sp, #G_L]
    mov     r0, r0, lsl #1             @ T = 2l bytes
    str     r0, [sp, #G_T]

    ldr     r0, [sp, #G_TWRUN]
    str     r0, [sp, #G_TWB]           @ stage base TW (bytes)

    mov     r0, #0
    str     r0, [sp, #G_J]

.Ljloop5:
    ldr     r0, [sp, #G_J]
    ldr     r1, [sp, #G_Q]
    cmp     r0, r1
    bge     .Lstage_next5

    ldr     r3, [sp, #G_T]             @ T — live for this j's k-loop

    @ TWJ = stage base + 24*j ; P = x + 8*j (8 bytes per coefficient pair, and
    @ p = 2j words).
    add     r1, r0, r0, lsl #1         @ 3j
    ldr     r2, [sp, #G_TWB]
    add     r12, r2, r1, lsl #3        @ TWJ = TWB + j*24
    ldr     r2, [sp, #G_X]
    add     r2, r2, r0, lsl #3         @ P = x + 8j

    @ j == 0 of every stage has the identity twiddle (TW[base..base+6] =
    @ (2^30,0) x3), so every output is a plain sum/difference >> 2 — no
    @ multiplies and no twiddle loads.
    cmp     r0, #0
    beq     .Ltriv5

.Lkloop5:
    ldr     r0, [sp, #G_END]
    cmp     r2, r0                     @ P < end ?
    bge     .Lj_next5

    @ ---- load A=(r4,r5) B=(r6,r7) C=(r8,r9) D=(r10,r11) ----
    @ One base from the cursor, then three stride steps: 7 instructions for the
    @ four complexes (v4: 12, because it rebuilt each address from a word index).
    ldmia   r2, {{r4, r5}}             @ A at P
    add     r0, r2, r3                 @ &B
    ldmia   r0, {{r6, r7}}
    add     r0, r0, r3                 @ &C
    ldmia   r0, {{r8, r9}}
    add     r0, r0, r3                 @ &D
    ldmia   r0, {{r10, r11}}

    @ ---- sums, formed destructively (8 instructions, no moves) ----
    @ t0 = A+C into C's registers with A still live, then t1 = 2*A - t0.
    add     r8, r8, r4                 @ t0r = cr + ar
    add     r9, r9, r5                 @ t0i = ci + ai
    rsb     r4, r8, r4, lsl #1         @ t1r = 2*ar - t0r = ar - cr
    rsb     r5, r9, r5, lsl #1         @ t1i = 2*ai - t0i = ai - ci
    add     r10, r10, r6               @ t2r = dr + br
    add     r11, r11, r7               @ t2i = di + bi
    rsb     r6, r10, r6, lsl #1        @ t3r = 2*br - t2r = br - dr
    rsb     r7, r11, r7, lsl #1        @ t3i = 2*bi - t2i = bi - di
    @ t0=(r8,r9) t1=(r4,r5) t2=(r10,r11) t3=(r6,r7)

    @ ---------------- out0 = (t0+t2) >> 2 -> x[P] ----------------
    add     r0, r8, r10
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    stmia   r2, {{r0, r1}}

    @ ---------------- out2 = cmul(t0-t2, w2) -> x[P+2T] ----------------
    @ Y2 into r8,r9 (t0 dead); w2 into r10,r11 (t2 dead); the lo half of the
    @ multiply doubles as the address scratch after the operands are read.
    sub     r8, r8, r10                @ Y2r
    sub     r9, r9, r11                @ Y2i
    ldr     r10, [r12, #8]             @ w2r
    ldr     r11, [r12, #12]            @ w2i
    smull   r0, r1, r8, r11            @ Y2r*w2i
    smlal   r0, r1, r9, r10            @ + Y2i*w2r -> imag
    add     lr, r2, r3, lsl #1         @ &x[P+2T] — lr, not r0: smull writes r0
    str     r1, [lr, #4]
    rsb     r11, r11, #0               @ -w2i
    smull   r0, r1, r8, r10
    smlal   r0, r1, r9, r11            @ real
    str     r1, [lr]

    @ ---------------- half B: Y1 and Y3 from t1,t3 ----------------
    @ Y1 = (t1r+t3i, t1i-t3r) ; Y3 = (t1r-t3i, t1i+t3r). Y1 goes into t1's
    @ registers and Y3 into t0's (dead), derived from Y1 with a shifted operand.
    add     r4, r4, r7                 @ Y1r = t1r + t3i
    sub     r5, r5, r6                 @ Y1i = t1i - t3r
    sub     r8, r4, r7, lsl #1         @ Y3r = 2*t1r - Y1r = t1r - t3i
    add     r9, r5, r6, lsl #1         @ Y3i = 2*t1i - Y1i = t1i + t3r

    @ ---------------- out1 = cmul(Y1, w1) -> x[P+T] ----------------
    ldr     r6, [r12]                  @ w1r
    ldr     r7, [r12, #4]              @ w1i
    smull   r0, r1, r4, r7             @ Y1r*w1i
    smlal   r0, r1, r5, r6             @ + Y1i*w1r -> imag
    add     lr, r2, r3                 @ &x[P+T]
    str     r1, [lr, #4]
    rsb     r7, r7, #0                 @ -w1i
    smull   r0, r1, r4, r6
    smlal   r0, r1, r5, r7             @ real
    str     r1, [lr]

    @ ---------------- out3 = cmul(Y3, w3) -> x[P+3T] ----------------
    ldr     r10, [r12, #16]            @ w3r
    ldr     r11, [r12, #20]            @ w3i
    smull   r0, r1, r8, r11            @ Y3r*w3i
    smlal   r0, r1, r9, r10            @ + Y3i*w3r -> imag
    add     lr, r2, r3                 @ p+t
    add     lr, lr, r3, lsl #1         @ p+3t
    str     r1, [lr, #4]
    rsb     r11, r11, #0               @ -w3i
    smull   r0, r1, r8, r10
    smlal   r0, r1, r9, r11            @ real
    str     r1, [lr]

    add     r2, r2, r3, lsl #2         @ P += 4T (next group of this j)
    b       .Lkloop5

.Ltriv5:
    @ Identity-twiddle butterfly (j == 0): w = (2^30, 0) x3, so cmul(y, 1.0) ==
    @ y >> 2 and every output is a sum/difference of t values shifted right 2.
    @ P starts at x and advances by 4T; r12 is free here and holds the imag temp.
.Lk_triv5:
    ldr     r0, [sp, #G_END]
    cmp     r2, r0
    bge     .Lj_next5

    ldmia   r2, {{r4, r5}}             @ A
    add     r0, r2, r3
    ldmia   r0, {{r6, r7}}             @ B
    add     r0, r0, r3
    ldmia   r0, {{r8, r9}}             @ C
    add     r0, r0, r3
    ldmia   r0, {{r10, r11}}           @ D

    add     r8, r8, r4                 @ t0r
    add     r9, r9, r5                 @ t0i
    rsb     r4, r8, r4, lsl #1         @ t1r
    rsb     r5, r9, r5, lsl #1         @ t1i
    add     r10, r10, r6               @ t2r
    add     r11, r11, r7               @ t2i
    rsb     r6, r10, r6, lsl #1        @ t3r
    rsb     r7, r11, r7, lsl #1        @ t3i

    @ out0 = (t0+t2) >> 2
    add     r0, r8, r10
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    stmia   r2, {{r0, r1}}

    @ out1 = (t1r+t3i, t1i-t3r) >> 2
    add     r0, r4, r7
    sub     r12, r5, r6
    mov     r0, r0, asr #2
    mov     r12, r12, asr #2
    add     r1, r2, r3
    stmia   r1, {{r0, r12}}

    @ out2 = (t0r-t2r, t0i-t2i) >> 2
    sub     r0, r8, r10
    sub     r12, r9, r11
    mov     r0, r0, asr #2
    mov     r12, r12, asr #2
    add     r1, r2, r3, lsl #1
    stmia   r1, {{r0, r12}}

    @ out3 = (t1r-t3i, t1i+t3r) >> 2
    sub     r0, r4, r7
    add     r12, r5, r6
    mov     r0, r0, asr #2
    mov     r12, r12, asr #2
    add     r1, r2, r3
    add     r1, r1, r3, lsl #1
    stmia   r1, {{r0, r12}}

    add     r2, r2, r3, lsl #2         @ P += 4T
    b       .Lk_triv5

.Lj_next5:
    ldr     r0, [sp, #G_J]
    add     r0, r0, #1
    str     r0, [sp, #G_J]
    b       .Ljloop5

.Lstage_next5:
    ldr     r0, [sp, #G_L]
    mov     r0, r0, lsr #2
    str     r0, [sp, #G_L]
    ldr     r0, [sp, #G_Q]
    add     r0, r0, r0, lsl #1         @ 3q
    mov     r0, r0, lsl #3             @ q*24
    ldr     r1, [sp, #G_TWRUN]
    add     r1, r1, r0
    str     r1, [sp, #G_TWRUN]
    ldr     r0, [sp, #G_L]
    cmp     r0, #16
    bge     .Lstage5
    b       .Ltail5

.Ltail5:
    @ ==========================================================================
    @ Tail stage l = 4: the four complexes are adjacent (T = 8 bytes), so one
    @ `ldmia` of eight words and one base address serve the whole group. No
    @ twiddles; every output is a t-combination >> 2.
    @ ==========================================================================
    ldr     r2, [sp, #G_X]             @ P = x
.Ltail_loop5:
    ldr     r0, [sp, #G_END]
    cmp     r2, r0
    bge     .Ldone5

    ldmia   r2, {{r4-r11}}             @ A=(r4,r5) B=(r6,r7) C=(r8,r9) D=(r10,r11)

    add     r8, r8, r4                 @ t0r
    add     r9, r9, r5                 @ t0i
    rsb     r4, r8, r4, lsl #1         @ t1r
    rsb     r5, r9, r5, lsl #1         @ t1i
    add     r10, r10, r6               @ t2r
    add     r11, r11, r7               @ t2i
    rsb     r6, r10, r6, lsl #1        @ t3r
    rsb     r7, r11, r7, lsl #1        @ t3i

    add     r0, r8, r10                @ (t0+t2) >> 2 -> x[P], x[P+1]
    add     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [r2]
    str     r1, [r2, #4]

    add     r0, r4, r7                 @ (t1r+t3i, t1i-t3r) >> 2 -> x[P+2..3]
    sub     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [r2, #8]
    str     r1, [r2, #12]

    sub     r0, r8, r10                @ (t0-t2) >> 2 -> x[P+4..5]
    sub     r1, r9, r11
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [r2, #16]
    str     r1, [r2, #20]

    sub     r0, r4, r7                 @ (t1r-t3i, t1i+t3r) >> 2 -> x[P+6..7]
    add     r1, r5, r6
    mov     r0, r0, asr #2
    mov     r1, r1, asr #2
    str     r0, [r2, #24]
    str     r1, [r2, #28]

    add     r2, r2, #32
    b       .Ltail_loop5

.Ldone5:
    add     sp, sp, #GFRAME
    pop     {{r4-r11, lr}}
    bx      lr

    .size   uli_fft_arm_v5, .-uli_fft_arm_v5
