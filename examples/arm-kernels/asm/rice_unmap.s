@ arm_rice_unmap — PR A's standalone fold kernel. PR B decision 6 makes
@ the .include sharing real: this standalone site and the composite
@ sample_step both draw the fold's four instructions from the single
@ committed source asm/fold_body.s (pasted into one global_asm! unit by
@ include_str!/concat! in src/asm.rs — the assembler-level .include made
@ cargo-cwd-safe). Each use site assembles its own copy; the binary
@ carries the fold twice either way (decision 6: zero runtime difference
@ versus duplication, one source to review).
@
@ NO-DRIFT WITNESS: objdump of the built kernel ELF must keep PR A's
@ recorded encodings at this entry — e2001001 (and) / e2611000 (rsb) /
@ e1a000a0 (lsr) / e0200001 (eor) (FLAC.md PR A entry; verified against
@ host clang on 2026-10-28 when the body moved to the shared source).
@
@ r0 = n (the folded residual value). Returns the unmapped i32 in r0.
@ Clobbers r1 only — and writes NO flags (the fold_body property).
    .syntax unified
    .arm
    .text
    .code 32
    .global arm_rice_unmap
    .type arm_rice_unmap, %function
arm_rice_unmap:
    fold_body r0, r1
    bx        lr
