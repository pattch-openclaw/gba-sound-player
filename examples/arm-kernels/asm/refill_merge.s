@ arm_refill_merge — PR B's optional `rev8` refill step (the design of
@ record's "a table lookup plus placement, trivially table-witnessable";
@ PR A's scope cut: "lands [in PR B], where decision 6's `.include` sharing
@ between the fold's two call sites also becomes real").
@
@ The ratified merge shape (left-to-measure item 3's *shape*, ratified
@ item 3: "rev8 refill + ORR ..., LSL #24 merge — the barrel shifter places
@ the byte in the merge cycle"). The COST of the reversal (one extra ldrb
@ per 8 bits in the shared refill path) is explicitly the probe's question
@ (left-to-measure item 4) — this kernel witnesses the shape only; it
@ measures nothing.
@
@ ABI: r0 = window (top byte must be zero — caller contract; the ORR
@ merges, it does not mask: overlapping bits would corrupt silently),
@ r1 = raw byte (0..=255), r2 = &REV8 (the committed 256-entry byte->
@ bit-reversed table). Returns the merged window in r0. Clobbers r1 +
@ flags; writes no memory. Interwork-safe return (`BX lr`).
@
@ The bit-reversed placement: REV8[byte] puts the byte's bit 7 at bit 24
@ (nearest the LSB end of the stream's future position at the top of the
@ window... the convention is decision 5's pinned bit-reversed window: a
@ byte arriving at the window top enters with its FIRST bit at position
@ 24, so REV8[byte] << 24 ORs the byte in stream order).
    .syntax unified
    .arm
    .text
    .code 32
    .global arm_refill_merge
    .type arm_refill_merge, %function
arm_refill_merge:
    and     r1, r1, #0xff               @ defensive byte mask: the wrapper
                                        @   zero-extends, but the kernel
                                        @   does not trust it (AAPCS leaves
                                        @   u8 upper bits unspecified);
                                        @   one cycle buys the bound.
    ldrb    r1, [r2, r1]                @ reversed byte (base+register
                                        @   LDRB — ARM-only addressing;
                                        @   zero-wait IWRAM at the probe's
                                        @   pin, .rodata in this crate)
    orr     r0, r0, r1, lsl #24         @ barrel-shifter merge — one cycle
    bx      lr
