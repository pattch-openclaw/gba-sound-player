@ E5 fetch arbitration body.
@
@ ONE synthetic instruction stream, assembled twice: once into `.text`
@ (cartridge ROM) and once into `.iwram`. Same bytes, same semantics; the only
@ variable is the address the instruction fetch reads from. That is the
@ arbitration E5 exists to make: on mGBA a 32-bit ARM instruction costs ~4
@ cycles from cartridge ROM and ~1.4 from IWRAM; this image prices the ratio on
@ real hardware with both copies in one link.
@
@ Per call: 4096 dependent `add r0, r0, #1` (16 x 256) plus the loop's
@ `subs`/`bne`. The chain is dependent so the result is a single value the caller
@ verifies; the work is long enough that the ~33-cycle timer window overhead is
@ noise. `r0`/`r1` are caller-saved in the C ABI, so no registers need saving.
.syntax unified
.arm
.code 32

.macro e5_body name
    .global \name
    .type \name, %function
\name:
    mov     r1, #16
1:
    .rept 256
    add     r0, r0, #1
    .endr
    subs    r1, r1, #1
    bne     1b
    bx      lr
    .size \name, . - \name
.endm

.section .text,"ax",%progbits
e5_body e5_rom_body

.section .iwram,"ax",%progbits
e5_body e5_iwram_body
