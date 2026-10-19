#!/usr/bin/env python3
"""Regenerate crates/rbr-physics/src/engine_driveline/engine_def.rs: what the engine definition's constructor
(`CEngine::CDef::CDef`, 0x50f080, 35,951 bytes of constant stores) writes, as word runs.

The constructor runs in unicorn on the retail exe's sections alone: `this` is a block of the definition's size, and
`operator new` (0x70bec2) is replaced by a bump allocator. It runs twice, on blocks filled with 0x00 and with 0xff and
with the allocator at two bases. A byte the same in both runs is written; a word that differs by the bases' distance
is a pointer to an allocated block. Every write must be a whole word, and nothing outside the blocks, the stack and
the TEB may be written. The output lists the allocations in order (their sizes), then per block (the definition, then
each allocation) its written words as runs (offset, words) and its pointer words (offset, allocation), then the words
the constructor writes differently with NGP 7.4.778's patches to it (`NGP_PATCHES`) applied.

Run from the repo root with the harness venv (unicorn):

    .venv/bin/python -I -B scripts/gen-engine-def.py [--exe PATH] [--write]

`--exe` defaults to the 1.02 executable in $RBR_PATCH (as the Rust tests), else the in-repo one. Without `--write` the script checks that its output matches the
checked-in file byte for byte and exits nonzero when it does not.
"""

import argparse
import os
import struct
import sys

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_PROT_ALL, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FS,
                               UC_X86_REG_SS, UC_X86_REG_GDTR, UC_X86_REG_MXCSR)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(os.environ.get("RBR_PATCH", os.path.join(ROOT, "original", "patch-1.02", "files", "Main_group")),
                   "RichardBurnsRally_SSE.exe")
OUT = os.path.join(ROOT, "crates", "rbr-physics", "src", "engine_driveline", "engine_def.rs")

CTOR = 0x50F080
OPERATOR_NEW = 0x70BEC2
SIZE = 0x2880  # the engine definition, +0x1990..+0x4210 of the car's
THIS = 0x20000000
HEAPS = (0x30000000, 0x38000000)
STACK, STACK_SIZE = 0x10000000, 0x100000
TEB, GDT = 0x11000000, 0x11010000
SENTINEL = 0x12345678
# NGP 7.4.778's patches to the constructor (the 1.02 bytes, then NGP's): the CReal at +0x2820 is built as a
# `CEngine::CRealPair` (0x50bc90), which adds the real at +0x2830 (`TorqueCut_NGP`), and two default stores are NOPed.
NGP_PATCHES = (
    (0x50F248, bytes.fromhex("540c0200"), bytes.fromhex("44caffff")),
    (0x5177F4, bytes.fromhex("f30f11470c"), bytes.fromhex("9090909090")),
    (0x51780B, bytes.fromhex("f30f11470c"), bytes.fromhex("9090909090")),
)


def sections(data):
    pe, = struct.unpack_from("<I", data, 0x3C)
    count, = struct.unpack_from("<H", data, pe + 6)
    optional, = struct.unpack_from("<H", data, pe + 20)
    base, = struct.unpack_from("<I", data, pe + 24 + 28)
    for i in range(count):
        at = pe + 24 + optional + 40 * i
        vsize, va, rsize, raw = struct.unpack_from("<IIII", data, at + 8)
        yield base + va, max(vsize, rsize), data[raw:raw + min(rsize, vsize or rsize)]


def desc(base, limit, access, flags=0xC):
    return struct.pack("<Q", (limit & 0xFFFF) | ((base & 0xFFFFFF) << 16) | (access << 40)
                       | (((limit >> 16) & 0xF) << 48) | (flags << 52) | ((base >> 24) << 56))


def run(exe, fill, heap):
    """One constructor run: (the definition's bytes, [(address, bytes)] of the allocations in order)."""
    mu = Uc(UC_ARCH_X86, UC_MODE_32)
    for va, size, raw in sections(exe):
        start, end = va & ~0xFFF, (va + size + 0xFFF) & ~0xFFF
        mu.mem_map(start, end - start, UC_PROT_ALL)
        mu.mem_write(va, raw)
    mu.mem_map(THIS, 0x10000, UC_PROT_ALL)
    mu.mem_write(THIS, bytes([fill]) * SIZE)
    mu.mem_map(heap, 0x100000, UC_PROT_ALL)
    mu.mem_write(heap, bytes([fill]) * 0x100000)
    mu.mem_map(STACK, STACK_SIZE, UC_PROT_ALL)
    mu.mem_map(TEB, 0x1000, UC_PROT_ALL)
    mu.mem_write(TEB, struct.pack("<I", 0xFFFFFFFF))  # an empty SEH chain
    mu.mem_map(GDT, 0x1000, UC_PROT_ALL)
    mu.mem_write(GDT, b"\0" * 8 + desc(TEB, 0xFFFFF, 0xF3) + desc(0, 0xFFFFF, 0x93))
    mu.reg_write(UC_X86_REG_GDTR, (0, GDT, 23, 0))
    mu.reg_write(UC_X86_REG_FS, 0x8 | 3)
    mu.reg_write(UC_X86_REG_SS, 0x10)
    mu.reg_write(UC_X86_REG_MXCSR, 0x1F80)
    esp = STACK + STACK_SIZE - 0x100
    mu.mem_write(esp, struct.pack("<I", SENTINEL))
    mu.reg_write(UC_X86_REG_ESP, esp)
    mu.reg_write(UC_X86_REG_ECX, THIS)
    blocks, bad = [], []

    def operator_new(mu, address, size, _):
        sp = mu.reg_read(UC_X86_REG_ESP)
        ret, n = struct.unpack("<II", mu.mem_read(sp, 8))
        at = blocks[-1][0] + ((blocks[-1][1] + 15) & ~15) if blocks else heap
        blocks.append((at, n))
        mu.reg_write(UC_X86_REG_EAX, at)
        mu.reg_write(UC_X86_REG_ESP, sp + 4)
        mu.reg_write(UC_X86_REG_EIP, ret)

    def on_write(mu, access, address, size, value, _):
        inside = (THIS <= address < THIS + SIZE or heap <= address < heap + 0x100000
                  or STACK <= address < STACK + STACK_SIZE or TEB <= address < TEB + 0x1000)
        whole = size % 4 == 0 and address % 4 == 0
        if not inside or not whole and not STACK <= address < STACK + STACK_SIZE:
            bad.append((address, size))

    mu.hook_add(UC_HOOK_CODE, operator_new, begin=OPERATOR_NEW, end=OPERATOR_NEW)
    mu.hook_add(UC_HOOK_MEM_WRITE, on_write)
    try:
        mu.emu_start(CTOR, SENTINEL, timeout=60 * 1000000)
    except UcError as e:
        sys.exit("the constructor stopped: %s at eip %#x" % (e, mu.reg_read(UC_X86_REG_EIP)))
    if bad:
        sys.exit("writes outside the blocks or not whole words: %s" % ", ".join("%#x/%d" % b for b in bad[:8]))
    return bytes(mu.mem_read(THIS, SIZE)), [(at, bytes(mu.mem_read(at, n))) for at, n in blocks]


def patched(exe, patches):
    """`exe` with `patches` applied, each checked against its 1.02 bytes."""
    out = bytearray(exe)
    pe, = struct.unpack_from("<I", exe, 0x3C)
    count, = struct.unpack_from("<H", exe, pe + 6)
    optional, = struct.unpack_from("<H", exe, pe + 20)
    base, = struct.unpack_from("<I", exe, pe + 24 + 28)
    for at, old, new in patches:
        for i in range(count):
            vsize, va, rsize, raw = struct.unpack_from("<IIII", exe, pe + 24 + optional + 40 * i + 8)
            if base + va <= at < base + va + rsize:
                o = raw + at - base - va
                if out[o:o + len(old)] != old:
                    sys.exit("the exe's bytes at %#x are not 1.02's" % at)
                out[o:o + len(new)] = new
                break
        else:
            sys.exit("%#x is not in the exe" % at)
    return bytes(out)


def images(exe):
    """[(bytes, runs, pointers)] of the definition, then each allocation, from two constructor runs."""
    a_def, a_blocks = run(exe, 0x00, HEAPS[0])
    b_def, b_blocks = run(exe, 0xFF, HEAPS[1])
    if [len(d) for _, d in a_blocks] != [len(d) for _, d in b_blocks]:
        sys.exit("the two runs allocate differently")
    blocks = [(a_def,) + words(a_def, b_def, a_blocks, b_blocks)]
    for (_, a), (_, b) in zip(a_blocks, b_blocks):
        blocks.append((a,) + words(a, b, a_blocks, b_blocks))
    return blocks


def written(runs):
    return {at + 4 * k: w for at, run in runs for k, w in enumerate(run)}


def ngp_words(stock, ngp):
    """The definition's words NGP's constructor writes differently from 1.02's or in addition: (offset, word)."""
    if [len(d) for d, _, _ in stock] != [len(d) for d, _, _ in ngp] or [p for _, _, p in stock] != [p for _, _, p in ngp]:
        sys.exit("NGP's constructor allocates differently")
    out = []
    for k, ((_, a, _), (_, b, _)) in enumerate(zip(stock, ngp)):
        a, b = written(a), written(b)
        if set(a) - set(b):
            sys.exit("NGP's constructor leaves words of block %d unwritten" % k)
        changed = [(off, w) for off, w in sorted(b.items()) if a.get(off) != w]
        if changed and k:
            sys.exit("NGP's constructor writes allocation %d differently" % (k - 1))
        out += changed
    return out


def words(a, b, a_blocks, b_blocks):
    """(written word runs, pointer words) of one block from its two runs."""
    runs, pointers, run_at, run = [], [], None, []
    for off in range(0, len(a), 4):
        x, y = struct.unpack_from("<I", a, off)[0], struct.unpack_from("<I", b, off)[0]
        target = next((k for k, ((p, _), (q, _)) in enumerate(zip(a_blocks, b_blocks)) if x == p and y == q), None)
        if target is not None:
            pointers.append((off, target))
        elif x == y:
            if run_at is None or run_at + 4 * len(run) != off:
                if run:
                    runs.append((run_at, run))
                run_at, run = off, []
            run.append(x)
            continue
        elif a[off:off + 4] != b"\0" * 4 or b[off:off + 4] != b"\xff" * 4:
            sys.exit("a word written differently by the two runs at +%#x" % off)
    if run:
        runs.append((run_at, run))
    return runs, pointers


def rust(blocks, ngp):
    out = [
        "//! What the engine definition's constructor (`CEngine::CDef::CDef`, 0x50f080) writes: the definition's words and",
        "//! the blocks it allocates. Generated by `scripts/gen-engine-def.py` from the retail exe (do not edit; run the",
        "//! script to regenerate or check); `layout::construct_engine_def` writes it.",
        "",
        "/// The sizes of the blocks the constructor allocates, in order.",
        "#[rustfmt::skip]",
        "pub const ALLOCATIONS: &[u32] = &[%s];" % ", ".join("0x%x" % len(data) for data, _, _ in blocks[1:]),
        "",
        "/// One block's written words: runs of words from an offset, and the words that point at an allocation (offset,",
        "/// index in [`ALLOCATIONS`]).",
        "pub struct Image {",
        "    pub runs: &'static [(u32, &'static [u32])],",
        "    pub pointers: &'static [(u32, usize)],",
        "}",
        "",
        "/// The definition's image, then each allocation's.",
        "#[rustfmt::skip]",
        "pub const IMAGES: &[Image] = &[",
    ]
    for _, runs, pointers in blocks:
        out.append("    Image {")
        out.append("        runs: &[")
        for at, run in runs:
            out.append("            (0x%x, &[" % at)
            for i in range(0, len(run), 8):
                out.append("                " + " ".join("0x%08x," % w for w in run[i:i + 8]))
            out.append("            ]),")
        out.append("        ],")
        out.append("        pointers: &[%s]," % ", ".join("(0x%x, %d)" % p for p in pointers))
        out.append("    },")
    out.append("];")
    out += [
        "",
        "/// The definition's words NGP 7.4.778's patched constructor writes differently or in addition: the real at",
        "/// +0x2830 (`TorqueCut_NGP`, built with +0x2820 as a `CEngine::CRealPair`) and two defaults it leaves at 0.",
        "#[rustfmt::skip]",
        "pub const NGP_WORDS: &[(u32, u32)] = &[",
    ]
    out += ["    (0x%x, 0x%08x)," % w for w in ngp]
    out.append("];")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=EXE)
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()
    with open(args.exe, "rb") as f:
        exe = f.read()
    blocks = images(exe)
    text = rust(blocks, ngp_words(blocks, images(patched(exe, NGP_PATCHES))))
    if args.write:
        with open(OUT, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print("wrote %s (%d bytes)" % (OUT, len(text)))
        return 0
    with open(OUT, encoding="utf-8") as f:
        if f.read() != text:
            sys.stderr.write("engine_def.rs differs from the generator's output; rerun with --write and review\n")
            return 1
    print("engine_def.rs matches the generator's output (%d bytes)" % len(text))
    return 0


if __name__ == "__main__":
    sys.exit(main())
