#!/usr/bin/env python3
"""Linear RV32IMC disassembly of a raw CH59x firmware image, with helpers to
find the command dispatcher.

usage: fw_scan.py BIN dis [START END]      disassemble a range (hex addresses)
       fw_scan.py BIN imm VALUE...         list `li`/`addi x0` loads of the given immediates
       fw_scan.py BIN cluster VALUE...     functions-ish windows where many of the values appear
"""
import sys

import capstone


def disasm(code, base=0):
    md = capstone.Cs(capstone.CS_ARCH_RISCV, capstone.CS_MODE_RISCV32 | capstone.CS_MODE_RISCVC)
    md.skipdata = True
    for ins in md.disasm(code, base):
        yield ins


BRANCHES = {'j', 'jal', 'c.j', 'c.jal', 'beq', 'bne', 'blt', 'bge', 'bltu', 'bgeu',
            'beqz', 'bnez', 'c.beqz', 'c.bnez', 'bltz', 'bgez', 'blez', 'bgtz'}


def branch_note(ins):
    """Capstone prints pc-relative offsets; append the absolute target."""
    if ins.mnemonic not in BRANCHES:
        return ''
    try:
        off = int(ins.op_str.split(',')[-1], 0)
    except ValueError:
        return ''
    return '    ; -> %06x' % (ins.address + off)


def li_value(ins):
    """Immediate for `li rd, imm` / `c.li` / `addi rd, zero, imm`."""
    if ins.mnemonic in ('li', 'c.li'):
        try:
            return int(ins.op_str.split(',')[-1], 0)
        except ValueError:
            return None
    if ins.mnemonic in ('addi', 'c.addi') and ', zero, ' in ins.op_str:
        return int(ins.op_str.split(',')[-1], 0)
    return None


def main():
    path, cmd, *args = sys.argv[1:]
    code = open(path, 'rb').read()
    if cmd == 'dis':
        start = int(args[0], 16) if args else 0
        end = int(args[1], 16) if len(args) > 1 else len(code)
        for ins in disasm(code[start:end], start):
            print('%06x: %-10s %s%s' % (ins.address, ins.mnemonic, ins.op_str, branch_note(ins)))
        return
    values = {int(v, 0) for v in args}
    hits = [(ins.address, li_value(ins)) for ins in disasm(code) if li_value(ins) in values]
    if cmd == 'imm':
        for a, v in hits:
            print('%06x 0x%02x' % (a, v))
    elif cmd == 'cluster':
        window = 0x400
        best = []
        for i, (a, _) in enumerate(hits):
            seen = {v for b, v in hits if a <= b < a + window}
            best.append((len(seen), a, sorted(seen)))
        best.sort(reverse=True)
        shown = []
        for n, a, seen in best:
            if any(abs(a - s) < window for s in shown):
                continue
            shown.append(a)
            print('%06x  %2d distinct  %s' % (a, n, ' '.join('%02x' % v for v in seen)))
            if len(shown) >= 8:
                break


if __name__ == '__main__':
    main()
