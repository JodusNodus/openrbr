#!/usr/bin/env python3
"""Regenerate rust/rbr-physics/src/ngp/tyre_types.rs: NGP 7.4.778's seven built-in tyre types, as its per-car init
(`0x1003f5b0`) copies them into each wheel.

The init builds the types in a zeroed 0x2eb4-byte block (seven of 0x6ac bytes) with 16 setters per type, whose
arguments are constants (`0x1003f635`..`0x10041122`), then copies the wheel's type into the wheel's tyre definition
(`0x100b79d0`, `this` the wheel definition's `+0x56c`) and three words into the wheel (`0x100b7e70`, `this` the
wheel's `+0x6b0`). Both run here in unicorn on NGP.dll's sections: the setters on a block, then each copy on a
zeroed destination per type. The output is the copies' destination offsets and, per type, their words.

Run from the repo root with the harness venv (unicorn):

    RBR_NGP=original/ngp .venv/bin/python -I -B scripts/gen-ngp-tyre-types.py [--write]

Without `--write` the script checks that its output matches the checked-in file byte for byte and exits nonzero when
it does not. The output goes through `rustfmt`.
"""

import argparse
import os
import struct
import subprocess
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_PROT_ALL, UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EBP

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "rust", "rbr-physics", "src", "ngp", "tyre_types.rs")

FILE_BASE = 0x10000000
SETTERS = (0x1003F635, 0x10041122)
COPY_DEF, COPY_WHEEL = 0x100B79D0, 0x100B7E70
TYPES, TYPE_SIZE = 7, 0x6AC
BLOCK, DEST, STACK, STACK_SIZE = 0x20000000, 0x21000000, 0x30000000, 0x10000
DEST_SIZE = 0x1000
RETURN = 0x40000000  # a `hlt` the copies return to


def sections(dll):
    pe, = struct.unpack_from("<I", dll, 0x3C)
    count, = struct.unpack_from("<H", dll, pe + 6)
    optional, = struct.unpack_from("<H", dll, pe + 20)
    for i in range(count):
        vsize, va, rsize, raw = struct.unpack_from("<IIII", dll, pe + 24 + optional + 40 * i + 8)
        yield FILE_BASE + va, max(vsize, rsize), dll[raw:raw + rsize]


def machine(dll):
    mu = Uc(UC_ARCH_X86, UC_MODE_32)
    for va, size, data in sections(dll):
        mu.mem_map(va & ~0xFFF, ((va + size + 0xFFF) & ~0xFFF) - (va & ~0xFFF), UC_PROT_ALL)
        mu.mem_write(va, data)
    for base, size in [(BLOCK, 0x4000), (DEST, DEST_SIZE), (STACK, STACK_SIZE), (RETURN, 0x1000)]:
        mu.mem_map(base, size, UC_PROT_ALL)
    mu.mem_write(RETURN, b"\xf4")
    return mu


def guard(mu, allowed):
    """Fails on a write outside the `(base, size)` regions."""
    def on_write(_mu, _access, addr, size, _value, _data):
        if not any(base <= addr and addr + size <= base + n for base, n in allowed):
            raise SystemExit("a write at %#x outside the block, the destination and the stack" % addr)
    mu.hook_add(UC_HOOK_MEM_WRITE, on_write)


def build_types(dll):
    """The setters' block: seven types."""
    mu = machine(dll)
    guard(mu, [(BLOCK, 0x4000), (STACK, STACK_SIZE)])
    mu.reg_write(UC_X86_REG_EBX, BLOCK)
    mu.reg_write(UC_X86_REG_ESP, STACK + STACK_SIZE // 2)
    mu.reg_write(UC_X86_REG_EBP, STACK + STACK_SIZE - 0x100)
    mu.emu_start(SETTERS[0], SETTERS[1])
    return bytes(mu.mem_read(BLOCK, TYPES * TYPE_SIZE))


def copy(dll, block, func, k):
    """The words `func` (thiscall, the type on the stack) writes into a zeroed destination for type `k`, as
    `{offset: word}`."""
    mu = machine(dll)
    mu.mem_write(BLOCK, block)
    written = set()

    def on_write(_mu, _access, addr, size, _value, _data):
        if DEST <= addr < DEST + DEST_SIZE:
            assert size == 4 and addr % 4 == 0, hex(addr)
            written.add(addr - DEST)
        elif not STACK <= addr < STACK + STACK_SIZE:
            raise SystemExit("%#x writes at %#x" % (func, addr))
    mu.hook_add(UC_HOOK_MEM_WRITE, on_write)
    esp = STACK + STACK_SIZE // 2
    mu.mem_write(esp, struct.pack("<II", RETURN, BLOCK + k * TYPE_SIZE))
    mu.reg_write(UC_X86_REG_ESP, esp)
    mu.reg_write(UC_X86_REG_ECX, DEST)
    mu.emu_start(func, RETURN)
    out = bytes(mu.mem_read(DEST, DEST_SIZE))
    return {o: struct.unpack_from("<I", out, o)[0] for o in sorted(written)}


def table(name, doc, base, copies):
    offsets = list(copies[0])
    assert all(list(c) == offsets for c in copies)
    lines = ["/// %s" % doc, "pub const %s_OFFSETS: [u32; %d] = [" % (name, len(offsets))]
    for i in range(0, len(offsets), 10):
        lines.append("    " + ", ".join("0x%x" % (base + o) for o in offsets[i:i + 10]) + ",")
    lines += ["];", "/// The words at [`%s_OFFSETS`], per tyre type." % name,
              "pub const %s: [[u32; %d]; %d] = [" % (name, len(offsets), TYPES)]
    for c in copies:
        lines.append("    [")
        words = [c[o] for o in offsets]
        for i in range(0, len(words), 8):
            lines.append("        " + ", ".join("0x%08x" % w for w in words[i:i + 8]) + ",")
        lines.append("    ],")
    lines.append("];")
    return lines


def render(block, dll):
    defs = [copy(dll, block, COPY_DEF, k) for k in range(TYPES)]
    wheels = [copy(dll, block, COPY_WHEEL, k) for k in range(TYPES)]
    lines = [
        "//! NGP 7.4.778's built-in tyre types (generated by `scripts/gen-ngp-tyre-types.py`; do not edit). NGP's",
        "//! per-car init (`0x1003f5b0`) builds seven types from constants and copies a wheel's type (its tyre",
        "//! definition's `+0xc`, 0 past 6) into the wheel's definition (`0x100b79d0`) and the wheel (`0x100b7e70`).",
        "",
    ]
    lines += table("DEF", "The wheel definition's words `0x100b79d0` writes (its tyre definition at `+0x56c`).",
                   0x56C, defs)
    lines.append("")
    lines += table("WHEEL", "The wheel's words `0x100b7e70` writes (at the wheel's `+0x6b0`).", 0x6B0, wheels)
    return rustfmt("\n".join(lines) + "\n")


def rustfmt(text):
    """`text` as `cargo fmt` leaves it (the workspace's `rustfmt.toml`)."""
    return subprocess.run(["rustfmt", "--edition", "2021", "--config-path", os.path.join(ROOT, "rust")],
                          input=text, capture_output=True, text=True, check=True).stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    ngp = os.environ.get("RBR_NGP", os.path.join(ROOT, "original", "ngp"))
    dll = open(os.path.join(ngp, "7.4.778", "Plugins", "NGP.dll"), "rb").read()
    text = render(build_types(dll), dll)
    if args.write:
        open(OUT, "w").write(text)
        return
    if not os.path.exists(OUT) or open(OUT).read() != text:
        sys.exit("%s is out of date; run with --write" % OUT)


if __name__ == "__main__":
    main()
