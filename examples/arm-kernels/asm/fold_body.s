@ fold_body nreg, tmp — the §9.2.7.2 rice_unmap fold body: PR B decision 6's
@ single committed source for the fold's four instructions. It is textually
@ pasted at both shipped use sites (standalone `arm_rice_unmap` and the
@ composite sample-step, via include_str!/concat! — the assembler's
@ .include, made cargo-cwd-safe: rustc gives the assembler no reliable
@ search path, so the paste happens in the Rust source that ships it).
@ The include guard makes the double paste safe inside one assembly unit:
@ the macro is DEFINED once, INVOKED at each use site; the built binary
@ carries the fold's instructions twice either way (decision 6's measured
@ fact), with one source to review.
@
@ Destination-parameterized because the two use sites hold different
@ registers live at the fold; the sequence itself is byte-identical at
@ both. arm_rice_unmap's use (nreg r0, tmp r1) must reproduce PR A's
@ recorded encodings exactly — that is the no-drift witness (objdump of
@ the built ELF, FLAC.md PR A entry).
@
@ Writes NO flags (no S bit anywhere): callers keep live condition-code
@ state across the fold (PR A's recorded property).
@ Identity (measured; step-3a corrected convention):
@   (n >> 1) ^ (-(n & 1)) — even -> n >> 1, odd -> !(n >> 1).
.ifndef FOLD_BODY_INCLUDED
.set FOLD_BODY_INCLUDED, 1
.macro fold_body nreg, tmp
    and     \tmp, \nreg, #1     @ parity of n
    rsb     \tmp, \tmp, #0      @ -(n & 1): 0 (even) or 0xFFFFFFFF (odd)
    lsr     \nreg, \nreg, #1    @ strip the folded LSB
    eor     \nreg, \nreg, \tmp  @ even: unchanged; odd: bitwise-not
.endm
.endif
