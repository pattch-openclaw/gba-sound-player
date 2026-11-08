#!/bin/sh
# The offline half of 6a-4's kernel witness: ARM-mode-ness and the epilogue
# rules, read out of the built ELF.
#
# Why this exists: a green mGBA sweep cannot distinguish a four-byte ARM body
# from a Thumb build of the same algorithm — both compute the same answers. The
# mode is an ELF fact, and this script is the check that reads it, so "this
# kernel ships as ARM" is a gate and not a comment. `flac-docs/arm-kernels.md`
# carries the same discipline for the E10 kernel track; the two rules here are
# that track's measured ones:
#
#   (1) four-byte ARM encodings — every instruction word aligned, and the symbol
#       in ARM state (a Thumb function's `st_value` has bit 0 set);
#   (2) a leaf return of `pop {…, lr}` + `bx lr` — never `LDM`-to-PC. On ARMv4
#       only BX/BLX switch instruction set, so an `LDM …pc` return leaves CPSR in
#       whatever mode the caller was in; the E10 record measured the first pass
#       losing 3 cycles per call and 4 bytes per table to exactly that
#       (arm-kernels.md, "machine facts the assembly taught us").
#
# The extent is read from the disassembly's own symbol labels, not from the ELF
# symbol table: `global_asm!` emits no `.size` directive, so the linked ELF
# records `arm_rice_partition` with size 0 (`readelf -sW`; `nm --size-sort`
# *infers* the extent by subtracting the next symbol, which is a different
# quantity and not one this script should rest a rule on).
#
# Usage: scripts/check_arm_kernel_objdump.sh <elf> [symbol]
# Exit: 0 all rules hold, 1 a rule failed, 2 the inputs could not be read.
#
# This runs on the HOST's binutils: `objdump -i` lists elf32-littlearm, so no
# arm-none-eabi toolchain is needed (none is installed in this harness).

set -eu

ELF=${1:?usage: check_arm_kernel_objdump.sh <elf> [symbol]}
SYMBOL=${2:-arm_rice_partition}

[ -f "$ELF" ] || { echo "check_arm_kernel_objdump: no such ELF: $ELF" >&2; exit 2; }
command -v objdump >/dev/null 2>&1 || {
    echo "check_arm_kernel_objdump: objdump not found (host binutils with the arm backend is enough; no arm-none-eabi-* toolchain is needed)" >&2
    exit 2
}
objdump -i 2>/dev/null | grep -q 'elf32-littlearm' || {
    echo "check_arm_kernel_objdump: this host's objdump has no elf32-littlearm backend" >&2
    exit 2
}

DUMP=$(mktemp)
trap 'rm -f "$DUMP"' EXIT
objdump -d "$ELF" > "$DUMP" 2>/dev/null || {
    echo "check_arm_kernel_objdump: objdump failed on $ELF" >&2
    exit 2
}

python3 - "$SYMBOL" "$DUMP" <<'PY'
import re
import sys

symbol, dump_path = sys.argv[1], sys.argv[2]

label = re.compile(r'^([0-9a-f]{8}) <' + re.escape(symbol) + r'>:$')
insn = re.compile(r'^\s*([0-9a-f]+):\s*([0-9a-f]{8})\s+(\S+)(?:\s+(.*))?$')
any_label = re.compile(r'^([0-9a-f]+) <')

addr = None
words = []      # (address, 32-bit word, mnemonic, operands)
failures = []

def fail(msg):
    failures.append(msg)

with open(dump_path) as fh:
    for line in fh:
        if addr is None:
            m = label.match(line.strip())
            if m:
                addr = int(m.group(1), 16)
            continue
        other = any_label.match(line)
        if other:
            break           # the next symbol's block starts here: the extent ends
        m = insn.match(line)
        if m:
            words.append((int(m.group(1), 16), int(m.group(2), 16),
                          m.group(3), (m.group(4) or '').strip()))

if addr is None:
    print(f"[objdump] FAIL: symbol '{symbol}' not found in the disassembly of {dump_path}")
    sys.exit(2)
if not words:
    fail(f"{symbol}: the disassembly produced no instruction words")

# ---- rule 1: ARM state, four-byte encodings ------------------------------
if addr % 2:
    fail(f"{symbol}: st_value {addr:#x} has bit 0 set — that is a Thumb symbol")
if addr % 4:
    fail(f"{symbol}: st_value {addr:#x} is not four-byte aligned")
for a, w, mnemonic, operands in words:
    if a % 4:
        fail(f"instruction at {a:#x} is not four-byte aligned: {w:08x} {mnemonic} {operands}")

# A Thumb decoding of this range would step in half-words; objdump stepping in
# fours *is* the ARM-mode witness. The mnemonic text is informational only:
# objdump's ARMv4 naming of `e12fff1e` is not reliable, which is why every rule
# below is checked against the encoded word.
addrs = sorted({a for a, _, _, _ in words})
if len(addrs) < 16:
    fail(f"{symbol}: only {len(addrs)} instruction words — is this the kernel body?")
if any((b - a) % 4 for a, b in zip(addrs, addrs[1:])):
    fail(f"{symbol}: the instruction stream advances by something other than four bytes")

# ---- rule 2: the epilogue ------------------------------------------------
BX_LR = 0xE12FFF1E
bx_at = None
for a, w, _, _ in words:
    if w == BX_LR:
        bx_at = a          # the last one is the function's own return
if bx_at is None:
    fail(f"{symbol}: no `bx lr` (0x{BX_LR:08x}) in the body — on ARMv4 only BX/BLX "
         f"switch instruction set, so the return form matters")

if bx_at is not None:
    before = [w for a, w, _, _ in words if a == bx_at - 4]
    if not before:
        fail(f"{symbol}: `bx lr` at {bx_at:#x} has no preceding instruction word")
    else:
        pop = before[0]
        # LDM sp!, {...}: bits 27..25 = 100, P=0 (post-index), U=1 (increment),
        # W=1 (writeback), Rn = 13 (sp).
        is_pop = (
            (pop >> 25) & 0b111 == 0b100
            and (pop >> 24) & 1 == 0
            and (pop >> 23) & 1 == 1
            and (pop >> 21) & 1 == 1
            and (pop >> 16) & 0b1111 == 13
        )
        if not is_pop:
            fail(f"{symbol}: the instruction before `bx lr` is {pop:08x}, not a "
                 f"`pop {{...}}` — the epilogue must restore the frame it pushed")
        elif not (pop >> 14) & 1:
            fail(f"{symbol}: the `pop` before `bx lr` ({pop:08x}) does not restore lr; "
                 f"`bx lr` would then return to an unset register")
        elif (pop >> 15) & 1:
            fail(f"{symbol}: the `pop` before `bx lr` ({pop:08x}) pops PC too — "
                 f"that is the LDM-to-PC return this repo bans")

# ---- rule 2, continued: no LDM-to-PC anywhere ----------------------------
for a, w, mnemonic, operands in words:
    # Any block transfer (bits 27..25 = 100) with L=1 and PC in the register list.
    if (w >> 25) & 0b111 == 0b100 and (w >> 20) & 1 == 1 and (w >> 15) & 1 == 1:
        fail(f"{symbol}: LDM-to-PC at {a:#x}: {w:08x} {mnemonic} {operands} — "
             f"unpredictable on ARMv4 when the base is in the list, and the "
             f"mode-switching return this repo measured against (arm-kernels.md)")

# ---- informational: the ARM-only instruction shapes ----------------------
# Printable, not asserted: these are the substrate facts the assembly's header
# names, and their absence would not by itself be a bug (LLVM could re-associate
# differently), but their presence is what makes the body recognisable as the one
# the E10 patch wrote. `mul` is C1's De Bruijn; `ldrb` with a register offset is
# the table read (a Thumb build cannot encode base+register ldrb).
muls = sum(1 for a, w, m, o in words if m == 'mul' or 'mul' in m)
ldrb_reg = sum(1 for a, w, m, o in words if m.startswith('ldrb') and '[' in o and ',' in o.split(']')[0])
print(f"[objdump] {symbol}: {len(addrs)} ARM instruction words from {addr:#010x} "
      f"to {max(addrs):#010x}; mul×{muls}, base+register ldrb×{ldrb_reg}")

if failures:
    print("\n".join(f"[objdump] FAIL: {f}" for f in failures))
    sys.exit(1)
print("[objdump] ARM mode, four-byte encodings, pop {..., lr} + bx lr, no LDM-to-PC")
PY
