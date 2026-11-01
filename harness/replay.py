"""Replay one StepForward call of the original game in an x86 emulator from a snapshot.

Usage: .venv/bin/python -I harness/replay.py SNAPSHOT [--check AFTER_SNAPSHOT RECORDING] [--after AFTER_SNAPSHOT]
  SNAPSHOT is a plugin dump (RBR_SNAPSHOT=... harness/play.sh; notes/verification.md), or a load-time
  entry dump (RBRRecompSnapAt-<addr>-<k>.bin, harness/plugin): the call runs from the function's
  entry to its return. --after compares that run with the dump at its return (-after.bin).
  Run A runs the original code; harness/rust_step.py runs the Rust ports against it.
  --check compares run A with the real call: the car + wheel bytes of RECORDING's frame for
  it, and every byte run A wrote against AFTER_SNAPSHOT (<name>.snap-<step>-after.bin).
  --after compares the registers and every byte run A wrote against AFTER_SNAPSHOT.
  Bytes that depend on the stubbed Windows calls (timers, their stack garbage) and the stack
  below the call are left out of every comparison.

Needs unicorn and numpy in the worktree's .venv; new x87 results come from this machine's CPU (harness/replay-fpu.c,
compiled at first use).
"""
import argparse
import bisect
import ctypes
import mmap
import os
import platform
import struct
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dev_env  # noqa: E402  (dev.env: RBR_DATA, the harness folders)

from unicorn import (Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_PROT_ALL, UC_PROT_READ, UC_PROT_EXEC, UC_HOOK_MEM_WRITE,
                     UC_HOOK_BLOCK, UC_HOOK_MEM_UNMAPPED, UC_HOOK_MEM_PROT, UC_HOOK_INTR, UC_HOOK_CODE)
from unicorn.unicorn_py3.unicorn import uclib  # the loaded libunicorn, for raw_hook
from unicorn.x86_const import *

STEP_FORWARD = 0x5273A0


def system_module(path):
    """Windows code, stubbed. The Universal CRT NGP.dll links (ucrtbase pow, sin, ...) runs emulated like the game's
    own: its maths decides physics results (notes/ngp.md)."""
    path = path.lower()
    return "\\windows\\" in path and not path.endswith(("\\ucrtbase.dll", "\\vcruntime140.dll"))


ORIG_EXE = os.environ.get("RBR_ORIG_EXE") or os.path.join(  # the 1.02 exe: RBR_PATCH's, or original/'s
    os.environ.get("RBR_PATCH") or os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                               "original", "patch-1.02", "files", "Main_group"),
    "RichardBurnsRally_SSE.exe")
REGISTERS = (("eax", UC_X86_REG_EAX), ("ecx", UC_X86_REG_ECX), ("edx", UC_X86_REG_EDX), ("ebx", UC_X86_REG_EBX),
             ("esp", UC_X86_REG_ESP), ("ebp", UC_X86_REG_EBP), ("esi", UC_X86_REG_ESI), ("edi", UC_X86_REG_EDI))
CAR_PTR, CAR_BYTES, WHEEL_BYTES, CONTROL_BYTES = 0x8EF660, 0x1700, 0xA00, 0x20
SENTINEL = 0xFEED0000  # return address of the replayed call; never mapped
HEAP_SIZE = 0x1000000  # the stub heap
MAGIC = 0x53524252  # 'RBRS'


class Snapshot:
    """A plugin dump: regions (base, size, file offset, protect, type, allocation base), the
    registers at the call, fxsave and the stack as the call saw it."""

    def __init__(self, path):
        f = open(path, "rb")
        self.data = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        d = self.data
        toff, magic = struct.unpack_from("<II", d, len(d) - 8)
        if magic != MAGIC:
            raise ValueError("%s: not a snapshot" % path)
        h = struct.unpack_from("<17I", d, toff)
        if h[0] != MAGIC or h[1] not in (1, 2):
            raise ValueError("%s: unknown snapshot version" % path)
        self.step, self.substeps, self.thread, self.teb = h[2:6]
        self.regs = dict(zip(("eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"), h[6:14]))
        self.stack_lo, nstack, n = h[14], h[15], h[16]
        # version 2 is a load-time dump (step = the function's entry, substeps = its ordinal): the entry
        # dump has the stack, the dump at the return (returns) has none
        self.path, self.version, self.returns = path, h[1], h[1] == 2 and nstack == 0
        p = toff + 17 * 4
        self.fx = bytes(d[p:p + 512])
        self.stack = bytes(d[p + 512:p + 512 + nstack])
        p += 512 + nstack
        self.regions = sorted(struct.unpack_from("<6I", d, p + 24 * i) for i in range(n))
        self.bases = [r[0] for r in self.regions]

    def read(self, addr, n):
        """Bytes at addr as the snapshot holds them (the call's stack included); may span regions."""
        out = bytearray()
        while len(out) < n:
            at = addr + len(out)
            i = bisect.bisect_right(self.bases, at) - 1
            base, size, off = self.regions[i][:3] if i >= 0 else (0, 0, 0)
            if not base <= at < base + size:
                raise KeyError("%#x not in snapshot" % at)
            k = min(n - len(out), base + size - at)
            out += self.data[off + at - base:off + at - base + k]
        lo, hi = max(addr, self.stack_lo), min(addr + n, self.stack_lo + len(self.stack))
        if lo < hi:
            out[lo - addr:hi - addr] = self.stack[lo - self.stack_lo:hi - self.stack_lo]
        return bytes(out)

    def u32(self, addr):
        return struct.unpack("<I", self.read(addr, 4))[0]

    def has(self, addr):
        i = bisect.bisect_right(self.bases, addr) - 1
        return i >= 0 and addr < self.regions[i][0] + self.regions[i][1]

    def modules(self):
        """(base, size, name, path) from the PEB loader list."""
        peb = self.u32(self.teb + 0x30)
        ldr = self.u32(peb + 0x0C)
        head = ldr + 0x0C
        out, e = [], self.u32(head)
        while e != head and len(out) < 512:
            base, size = self.u32(e + 0x18), self.u32(e + 0x20)
            def ustr(at):
                n, _, buf = struct.unpack("<HHI", self.read(at, 8))
                return self.read(buf, n).decode("utf-16-le", "replace")
            out.append((base, size, ustr(e + 0x2C), ustr(e + 0x24)))
            e = self.u32(e)
        return out

    def exports(self, base):
        """{address: name} of a loaded module's exports."""
        pe = base + self.u32(base + 0x3C)
        rva, size = self.u32(pe + 0x78), self.u32(pe + 0x7C)
        if not rva:
            return {}
        _, _, _, nfun, nnames, funcs, names, ords = struct.unpack("<IIIIIIII", self.read(base + rva + 8, 32))
        out = {}
        for k in range(nnames):
            name_at = base + self.u32(base + names + 4 * k)
            name = self.read(name_at, 64).split(b"\0")[0].decode()
            o, = struct.unpack("<H", self.read(base + ords + 2 * k, 2))
            f = self.u32(base + funcs + 4 * o)
            if not rva <= f < rva + size:  # forwarders point at a string
                out[base + f] = name
        return out


# Hardware results for the x87 ops the emulator gets wrong (it computes transcendentals through
# host doubles: 53 of 64 mantissa bits; fscale at 24-bit precision is rounded; harness/replay-fpu.py
# measures it). They come from this machine's CPU: harness/replay-fpu.c, compiled into build/replay-fpu/
# at first use, keyed by (op, control word, st0, st1) and cached in build/replay-fpu-cache.bin.
FPU_OPS = {0xFE: (5, 0), 0xFF: (6, 0), 0xFB: (7, 1), 0xF2: (8, 1), 0xF3: (9, -1), 0xF1: (10, -1), 0xF0: (11, 0),
           0xFD: (12, 0), 0xF8: (14, 0), 0xF5: None, 0xF9: None, 0xFC: None}  # op -> (replay-fpu.c case, stack change)
FPU_UNARY = {0xFE, 0xFF, 0xFB, 0xF2, 0xF0}  # st1 is no input
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FPU_CACHE = os.environ.get("RBR_REPLAY_FPU_CACHE", os.path.join(REPO, "build", "replay-fpu-cache.bin"))
FPU_SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "replay-fpu.c")
FPU_EXE = os.path.join(REPO, "build", "replay-fpu", "replay-fpu")  # compiled at first use; needs an x86_64 CPU


def original_text():
    """(address, bytes) of the original exe's .text at its preferred base. A load-time dump has the
    transplant's jmps and the plugin's two hooks in .text (the only bytes that differ from the file
    there), so the replay restores the file's bytes and runs the original code."""
    d = open(ORIG_EXE, "rb").read()
    pe = struct.unpack_from("<I", d, 0x3C)[0]
    nsec, = struct.unpack_from("<H", d, pe + 6)
    optsz, = struct.unpack_from("<H", d, pe + 20)
    base, = struct.unpack_from("<I", d, pe + 52)
    for i in range(nsec):
        s = pe + 24 + optsz + 40 * i
        if d[s:s + 8].rstrip(b"\0") == b".text":
            vsize, va, rsize, raw = struct.unpack_from("<IIII", d, s + 8)
            return base + va, d[raw:raw + min(vsize, rsize)]
    raise ValueError("%s: no .text section" % ORIG_EXE)


class FpuOracle:
    def __init__(self, path=FPU_CACHE):
        self.path, self.known = path, {}
        if os.path.exists(path):
            d = open(path, "rb").read()
            for k in range(0, len(d) - 45, 46):
                self.known[d[k:k + 24]] = d[k + 24:k + 46]

    def fetch(self, keys):
        """Runs the cases natively on this machine and caches the results."""
        keys = sorted(set(keys) - set(self.known))
        if not keys:
            return
        if platform.machine() not in ("x86_64", "AMD64"):
            sys.exit("replay.py: the x87 oracle runs on this machine's CPU, which is not x86_64 (%s); "
                     "no new x87 results can be computed" % platform.machine())
        if not os.path.exists(FPU_EXE) or os.path.getmtime(FPU_SRC) > os.path.getmtime(FPU_EXE):
            os.makedirs(os.path.dirname(FPU_EXE), exist_ok=True)
            subprocess.run(["gcc", "-O1", "-o", FPU_EXE, FPU_SRC], check=True)
        out = subprocess.run([FPU_EXE], input=b"".join(keys), capture_output=True, check=True).stdout
        if len(out) != 22 * len(keys):
            raise RuntimeError("replay-fpu returned %d bytes for %d cases" % (len(out), len(keys)))
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        with open(self.path, "ab") as f:
            for i, k in enumerate(keys):
                self.known[k] = out[22 * i:22 * i + 22]
                f.write(k + self.known[k])


def desc(base, limit, access, flags=0xC):
    """A GDT descriptor."""
    return struct.pack("<Q", (limit & 0xFFFF) | ((base & 0xFFFFFF) << 16) | (access << 40)
                       | (((limit >> 16) & 0xF) << 48) | (flags << 52) | ((base >> 24) << 56))


GPR = (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EBX, UC_X86_REG_ESP, UC_X86_REG_EBP,
       UC_X86_REG_ESI, UC_X86_REG_EDI)
XMM = tuple(UC_X86_REG_XMM0 + i for i in range(8))


def modrm(mu, code):
    """Decode a 32-bit ModRM operand from code (starting at the ModRM byte): (reg field, rm register
    number or None, effective address or None, bytes used)."""
    m = code[0]
    mod, reg, rm = m >> 6, (m >> 3) & 7, m & 7
    if mod == 3:
        return reg, rm, None, 1
    n, base = 1, None
    if rm == 4:
        sib = code[1]
        n = 2
        scale, index, b = 1 << (sib >> 6), (sib >> 3) & 7, sib & 7
        ea = 0 if index == 4 else mu.reg_read(GPR[index]) * scale
        if b == 5 and mod == 0:
            ea += struct.unpack_from("<i", code, n)[0]
            n += 4
        else:
            ea += mu.reg_read(GPR[b])
    elif rm == 5 and mod == 0:
        ea = struct.unpack_from("<I", code, 1)[0]
        n = 5
    else:
        ea = mu.reg_read(GPR[rm])
    if mod == 1:
        ea += struct.unpack_from("<b", code, n)[0]
        n += 1
    elif mod == 2:
        ea += struct.unpack_from("<i", code, n)[0]
        n += 4
    return reg, None, ea & 0xFFFFFFFF, n


# rsqrtss/rcpss approximate in hardware and are exact in the emulator. The tables of the CPU the
# references ran on (harness/replay-approx.c) give the hardware's results; checked on 400k random
# inputs over the whole float range, zero, denormals, infinities and NaNs included.
_tables = struct.unpack("<4096I", open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                    "replay-approx.bin"), "rb").read())
RSQRT, RCP = _tables[:2048], _tables[2048:]


def hw_rsqrt(x):
    s, e, m = x >> 31, (x >> 23) & 0xFF, x & 0x7FFFFF
    if e == 0:
        return 0xFF800000 if s else 0x7F800000
    if e == 255:
        return (0xFFC00000 if s else 0) if m == 0 else x | 0x400000
    if s:
        return 0xFFC00000
    odd = (e - 127) & 1
    return RSQRT[(odd << 10) | (m >> 13)] - (((e - 127 - odd) // 2) << 23)


def hw_rcp(x):
    s, e, m = x >> 31, (x >> 23) & 0xFF, x & 0x7FFFFF
    if e == 0:
        return (s << 31) | 0x7F800000
    if e == 255:
        return s << 31 if m == 0 else x | 0x400000
    t = RCP[m >> 12]
    te = ((t >> 23) & 0xFF) - (e - 127)
    return s << 31 if te <= 0 else (s << 31) | (te << 23) | (t & 0x7FFFFF)


X87_SPECIAL = {0xFE: "fsin", 0xFF: "fcos", 0xFB: "fsincos", 0xF2: "fptan", 0xF3: "fpatan", 0xF1: "fyl2x",
               0xF9: "fyl2xp1", 0xF0: "f2xm1", 0xFD: "fscale", 0xF8: "fprem", 0xF5: "fprem1", 0xFC: "frndint"}


def special_sites(code, base):
    """Candidate addresses of rsqrt/rcp (ss and ps) and x87 transcendentals in code: byte matches;
    a hook only fires if execution reaches one, so false matches cost nothing."""
    out = []
    for op in (b"\x0f\x52", b"\x0f\x53"):
        i = code.find(op)
        while i >= 0:
            out += [base + i - 1, base + i] if i and code[i - 1] == 0xF3 else [base + i]
            i = code.find(op, i + 1)
    i = code.find(b"\xd9")
    while i >= 0:
        if i + 1 < len(code) and code[i + 1] in X87_SPECIAL:
            out.append(base + i)
        i = code.find(b"\xd9", i + 1)
    return out


class Machine:
    """One emulator loaded with a snapshot, ready to run the step call."""

    def __init__(self, snap, log_writes=True, timer_base=1 << 40, timer_step=1000, scribble=None, oracle=None):
        if snap.returns:
            raise ValueError("%s: a load-time dump at a return; replay its entry dump" % snap.path)
        self.snap = snap
        mu = self.mu = Uc(UC_ARCH_X86, UC_MODE_32)
        self.views = []  # (base, memory): the writable regions and the stub heap, in memory of ours (differing)
        for base, size, off, protect, *_ in snap.regions:
            # read-only stays read-only, so a store there faults as in the game
            if protect & 0xCC:
                self.map_ours(base, size)
            else:
                mu.mem_map(base, size, UC_PROT_READ | UC_PROT_EXEC)
            mu.mem_write(base, snap.data[off:off + size])
        mu.mem_write(snap.stack_lo, snap.stack)
        if snap.version == 2:
            mu.mem_write(*original_text())
        # fs -> TEB through a one-entry GDT on a free page
        gdt = next(a for a in range(0x7FF00000, 0x7FFF0000, 0x10000) if not snap.has(a))
        mu.mem_map(gdt, 0x1000, UC_PROT_ALL)
        mu.mem_write(gdt, b"\0" * 8 + desc(snap.teb, 0xFFFFF, 0xF3) + desc(0, 0xFFFFF, 0x93))
        mu.reg_write(UC_X86_REG_GDTR, (0, gdt, 23, 0))
        mu.reg_write(UC_X86_REG_FS, 0x8 | 3)
        mu.reg_write(UC_X86_REG_SS, 0x10)  # flat 32-bit stack
        r = snap.regs
        for name, reg in (("eax", UC_X86_REG_EAX), ("ecx", UC_X86_REG_ECX), ("edx", UC_X86_REG_EDX),
                          ("ebx", UC_X86_REG_EBX), ("ebp", UC_X86_REG_EBP), ("esi", UC_X86_REG_ESI),
                          ("edi", UC_X86_REG_EDI)):
            mu.reg_write(reg, r[name])
        # the plugin's esp points at the pushed substeps; the call pushes its return address. A load-time
        # entry dump's esp already points at the function's return address.
        slot = r["esp"] if snap.version == 2 else r["esp"] - 4
        mu.reg_write(UC_X86_REG_ESP, slot)
        mu.mem_write(slot, struct.pack("<I", SENTINEL))
        self.start = snap.step if snap.version == 2 else STEP_FORWARD
        fcw, = struct.unpack_from("<H", snap.fx, 0)
        fcw = int(os.environ.get("RBR_REPLAY_FCW", "0"), 16) or fcw  # another x87 control word than the snapshot's
        mxcsr, = struct.unpack_from("<I", snap.fx, 24)
        mu.reg_write(UC_X86_REG_FPCW, fcw)
        mu.reg_write(UC_X86_REG_MXCSR, mxcsr)
        for i in range(8):
            mu.reg_write(UC_X86_REG_XMM0 + i, int.from_bytes(snap.fx[160 + 16 * i:176 + 16 * i], "little"))
        # code outside the game's own modules (Windows/Wine) never runs: calls into it are stubbed
        self.names, self.system = {}, []
        for base, size, name, path in snap.modules():
            if system_module(path):
                self.system.append((base, size, name))
                for a, sym in snap.exports(base).items():
                    self.names[a] = "%s!%s" % (name, sym)
        heap = next(a for a in range(0x60000000, 0x7F000000, HEAP_SIZE)
                    if not any(snap.has(a + k) for k in range(0, HEAP_SIZE, 0x10000)))
        self.map_ours(heap, HEAP_SIZE)
        self.heap_base = heap
        self.heap_next, self.heap_sizes, self.ticks, self.tick_step = heap, {}, timer_base, timer_step
        self.scribble = scribble  # a byte the stubs leave below esp (perturbed runs; Windows leaves its frames there)
        self.writes = []
        self.stubbed = {}
        self.sound_vtables, self.sound_objects = {}, {}  # the DirectSound objects (_ds_object)
        self.d3d_vtable, self.d3d_buffers = None, {}  # the Direct3D vertex buffers (_d3d_create_vb)
        self.stop = None
        if log_writes:
            self.eip = reg_reader(mu, UC_X86_REG_EIP)
            raw_hook(self, UC_HOOK_MEM_WRITE, self.on_write)
        for base, size, _ in self.system:
            mu.hook_add(UC_HOOK_BLOCK, self.on_system, begin=base, end=base + size - 1)
        self.special, self.fpu_log, self.special_at, self.hooked = {}, None, set(), set()
        self.oracle, self.pending, self.fpu_missing, self.from_cpu = oracle, None, set(), 0
        # the game's modules, and executable memory outside any module
        mods = snap.modules()
        for rbase, rsize, _, protect, *_ in snap.regions:
            mod = next((m for m in mods if m[0] <= rbase < m[0] + m[1]), None)
            if mod and not system_module(mod[3]) or not mod and protect & 0xF0:
                self.hook_special(rbase, self.read(rbase, rsize))
        mu.hook_add(UC_HOOK_MEM_UNMAPPED | UC_HOOK_MEM_PROT, self.on_unmapped)
        mu.hook_add(UC_HOOK_INTR, self.on_intr)
        self.chain = None

    def map_ours(self, base, size):
        """Maps [base, base + size) read-write over memory of ours, so differing() compares it without copies."""
        mem = mmap.mmap(-1, size)
        self.mu.mem_map_ptr(base, size, UC_PROT_ALL, ctypes.addressof(ctypes.c_char.from_buffer(mem)))
        self.views.append((base, mem))

    def hook_special(self, base, code):
        """Hooks the special instructions in code, and the instruction after each x87 one (where the
        CPU's result replaces the emulator's)."""
        for a in special_sites(code, base):
            self.special_at.add(a)
            hooks = (a, a + 2) if code[a - base] == 0xD9 else (a,)
            for h in hooks:
                if h not in self.hooked:
                    self.hooked.add(h)
                    self.mu.hook_add(UC_HOOK_CODE, self.on_special, begin=h, end=h)

    def on_special(self, mu, addr, size, _):
        code = bytes(mu.mem_read(addr, 16))
        if self.pending and self.pending[0] == addr:  # the CPU's result of the x87 op just executed
            _, change, res, sw_before = self.pending
            self.pending = None
            sw = mu.reg_read(UC_X86_REG_FPSW)
            top = (sw >> 11) & 7
            mu.reg_write(UC_X86_REG_FP0 + top, struct.unpack("<QH", res[:10]))
            if change == 1:
                mu.reg_write(UC_X86_REG_FP0 + ((top + 1) & 7), struct.unpack("<QH", res[10:20]))
            hw, = struct.unpack("<H", res[20:])
            mu.reg_write(UC_X86_REG_FPSW, (sw & ~0x473F) | (hw & 0x4700) | ((sw_before | hw) & 0x3F))
            self.from_cpu += 1
        if addr not in self.special_at:
            return
        if code[0] == 0xD9:
            name = X87_SPECIAL[code[1]]
            self.special[name] = self.special.get(name, 0) + 1
            sw = mu.reg_read(UC_X86_REG_FPSW)
            top = (sw >> 11) & 7
            st = [struct.pack("<QH", *mu.reg_read(UC_X86_REG_FP0 + ((top + i) & 7))) for i in range(2)]
            if code[1] in FPU_UNARY:
                st[1] = bytes(10)
            cw = mu.reg_read(UC_X86_REG_FPCW)
            if self.fpu_log is not None:  # operands (harness/replay-fpu.py)
                self.fpu_log.append((code[1], st[0], st[1], cw))
            if FPU_OPS[code[1]] and self.oracle:
                case, change = FPU_OPS[code[1]]
                key = struct.pack("<BBH", case, 0, cw) + st[0] + st[1]
                res = self.oracle.known.get(key)
                if res:
                    self.pending = (addr + 2, change, res, sw)
                else:
                    self.fpu_missing.add(key)
            return
        scalar = code[0] == 0xF3
        op = code[2 if scalar else 1]
        reg, rm, ea, n = modrm(mu, code[3 if scalar else 2:])
        lanes = 1 if scalar else 4
        src = (mu.reg_read(XMM[rm]).to_bytes(16, "little")[:4 * lanes] if ea is None
               else bytes(mu.mem_read(ea, 4 * lanes)))
        fn = hw_rsqrt if op == 0x52 else hw_rcp
        dst = bytearray(mu.reg_read(XMM[reg]).to_bytes(16, "little"))
        for k in range(lanes):
            dst[4 * k:4 * k + 4] = struct.pack("<I", fn(struct.unpack_from("<I", src, 4 * k)[0]))
        mu.reg_write(XMM[reg], int.from_bytes(dst, "little"))
        name = ("rsqrt" if op == 0x52 else "rcp") + ("ss" if scalar else "ps")
        self.special[name] = self.special.get(name, 0) + 1
        mu.reg_write(UC_X86_REG_EIP, addr + (3 if scalar else 2) + n)

    def on_write(self, addr, size, value):
        self.writes.append((addr, size, value & ((1 << 8 * size) - 1), self.eip()))

    def return_addresses(self):
        """The call chain's return addresses, innermost first: every value on the stack up to the replayed
        call that follows a call instruction (stale ones included)."""
        mu = self.mu
        esp = mu.reg_read(UC_X86_REG_ESP)
        top = self.snap.regs["esp"]
        words = struct.unpack("<%dI" % ((top - esp) // 4), mu.mem_read(esp, top - esp)) if top > esp else ()
        out = []
        for v in words:
            try:
                before = bytes(mu.mem_read(v - 7, 7))
            except UcError:
                continue
            if (before[2] == 0xE8 or before[1] == 0xFF and before[2] & 0xF8 in (0x10, 0x90)
                    or before[5] == 0xFF and 0xD0 <= before[6] <= 0xD7
                    or before[5] == 0xFF and before[6] & 0xF8 == 0x10
                    or before[4] == 0xFF and before[5] & 0xF8 in (0x10, 0x50)):
                out.append(v)
        return out

    def on_unmapped(self, mu, access, addr, size, value, _):
        what = {19: "read of unmapped", 20: "write to unmapped", 21: "fetch from unmapped", 22: "write to read-only",
                23: "read of protected", 24: "fetch from non-executable"}.get(access, "access %d at" % access)
        self.stop = "fault: %s %#x at eip %#x" % (what, addr, mu.reg_read(UC_X86_REG_EIP))
        self.chain = [mu.reg_read(UC_X86_REG_EIP)] + self.return_addresses()
        return False

    def on_intr(self, mu, intno, _):
        self.stop = "%s at eip %#x" % ({13: "general protection fault", 6: "invalid opcode", 0: "divide error"}.get(
            intno, "interrupt %d" % intno), mu.reg_read(UC_X86_REG_EIP))
        self.chain = [mu.reg_read(UC_X86_REG_EIP)] + self.return_addresses()
        mu.emu_stop()

    def on_system(self, mu, addr, size, _):
        name = self.names.get(addr, "%#x" % addr)
        esp = mu.reg_read(UC_X86_REG_ESP)
        ret, = struct.unpack("<I", mu.mem_read(esp, 4))
        stub = STUBS.get(name.split("!")[1] if "!" in name else name)
        if not stub:
            self.stop = "unstubbed call to %s from %#x" % (name, ret)
            self.chain = [ret] + self.return_addresses()
            mu.emu_stop()
            return
        nargs, fn = stub
        args = struct.unpack("<%dI" % nargs, mu.mem_read(esp + 4, 4 * nargs)) if nargs else ()
        self.stubbed[name] = self.stubbed.get(name, 0) + 1
        eax = fn(self, *args)
        if self.scribble is not None:
            mu.mem_write(esp - 0x400, bytes([self.scribble]) * 0x400)
        mu.reg_write(UC_X86_REG_EAX, (eax or 0) & 0xFFFFFFFF)
        mu.reg_write(UC_X86_REG_ESP, esp + 4 + 4 * nargs)
        mu.reg_write(UC_X86_REG_EIP, ret)

    def run(self, timeout=60):
        t = time.time()
        try:
            self.mu.emu_start(self.start, SENTINEL, timeout=int(timeout * 1e6))
        except UcError as e:
            if not self.stop:
                self.stop = "%s at eip %#x" % (e, self.mu.reg_read(UC_X86_REG_EIP))
                self.chain = [self.mu.reg_read(UC_X86_REG_EIP)] + self.return_addresses()
        self.seconds = time.time() - t
        if not self.stop and self.mu.reg_read(UC_X86_REG_EIP) != SENTINEL:
            self.stop = "timeout at eip %#x" % self.mu.reg_read(UC_X86_REG_EIP)
        return self.stop

    def read(self, addr, n):
        return bytes(self.mu.mem_read(addr, n))

    def poke(self, addr, data):
        """A stub's memory write, logged like the code's own."""
        self.mu.mem_write(addr, data)
        eip = struct.unpack("<I", self.read(self.mu.reg_read(UC_X86_REG_ESP), 4))[0]
        for k in range(0, len(data), 4):
            chunk = data[k:k + 4]
            self.writes.append((addr + k, len(chunk), int.from_bytes(chunk, "little"), eip))

    def frame(self):
        """car + 4 wheels, as harness recordings store them after the control."""
        car, = struct.unpack("<I", self.read(CAR_PTR, 4))
        out = self.read(car, CAR_BYTES)
        for w in struct.unpack("<4I", self.read(car + 0x470, 16)):
            out += self.read(w, WHEEL_BYTES) if w else b"\0" * WHEEL_BYTES
        return out


# Windows/Wine calls: the same stand-in in every run. name -> (stdcall argument count, fn(machine, *args)).
# Timers count up from a fixed value; heap blocks come from a bump allocator in fresh memory.
def _qpc(m, out):
    m.ticks += m.tick_step
    m.poke(out, struct.pack("<Q", m.ticks))
    return 1


def _qpf(m, out):
    m.poke(out, struct.pack("<Q", 10000000))
    return 1


def _alloc(m, heap, flags, size):
    a = m.heap_next
    m.heap_next += (size + 15) & ~15
    m.heap_sizes[a] = size
    if flags & 8:  # HEAP_ZERO_MEMORY
        m.poke(a, b"\0" * size)
    return a


def _realloc(m, heap, flags, block, size):
    a = _alloc(m, heap, flags, size)
    m.poke(a, m.read(block, min(size, m.heap_sizes.get(block, size))))
    return a


def _tls(m, index):
    return struct.unpack("<I", m.read(m.snap.teb + 0xE10 + 4 * index, 4))[0] if index < 64 else \
        struct.unpack("<I", m.read(struct.unpack("<I", m.read(m.snap.teb + 0xF94, 4))[0] + 4 * (index - 64), 4))[0]


def _fls(m, index):
    """Wine's FLS layout: TEB+0xfb4 -> { LIST_ENTRY, chunks[8] }, chunk k holds 0x10 << k slots
    after one header word."""
    k = 0
    while index >= 0x10 << k:
        index -= 0x10 << k
        k += 1
    u32 = lambda a: struct.unpack("<I", m.read(a, 4))[0]
    chunk = u32(u32(m.snap.teb + 0xFB4) + 8 + 4 * k)
    return u32(chunk + 4 * (index + 1)) if chunk else 0


def _interlocked(delta):
    def fn(m, at):
        v = (struct.unpack("<i", m.read(at, 4))[0] + delta) & 0xFFFFFFFF
        m.poke(at, struct.pack("<I", v))
        return v
    return fn


# DirectSound buffers, made by CreateSoundBuffer (E30DXSound::Vf48). A game object's vtable is ours (_ds_object):
# each entry is a stub address in DSOUND.dll's range, DS_STUB_BASE + 0x100 * kind + slot, where slot is the byte
# offset the game calls (E30_CC). Kind 0 is the buffer (Lock, Unlock, Play, Stop, volume, pan, frequency), kind 1 its
# IDirectSound3DBuffer (QueryInterface with IID_DS3DBuffer). A stub pops the arguments its caller pushed and writes
# only its out-pointers: sound is not physics, so no game state depends on it.
DS_STUB_BASE = 0x7b0d0000
DS3D_IID_DATA1 = 0x279AFA86  # IID_IDirectSound3DBuffer, the first word of the GUID the game passes


def _ds_ok(m, *args):
    return 0


def _ds_qi(m, this, iid, out):
    d3 = struct.unpack("<I", m.read(iid, 4))[0] == DS3D_IID_DATA1
    m.poke(out, struct.pack("<I", m.sound_objects[this]["d3"] if d3 else this))
    return 0


def _ds_lock(m, this, off, nbytes, p1, n1, p2, n2, flags):
    """A block of the buffer's memory from off (the whole rest for nbytes 0)."""
    buf = m.sound_objects[this]
    m.poke(p1, struct.pack("<I", buf["data"] + off))
    m.poke(n1, struct.pack("<I", nbytes or buf["size"] - off))
    if p2:
        m.poke(p2, b"\0" * 4)
    if n2:
        m.poke(n2, b"\0" * 4)
    return 0


DS_VTABLES = {
    0: {0x00: (3, _ds_qi), 0x04: (1, _ds_ok), 0x08: (1, _ds_ok), 0x10: (3, _ds_ok), 0x2c: (8, _ds_lock),
        0x30: (4, _ds_ok), 0x34: (2, _ds_ok), 0x3c: (2, _ds_ok), 0x40: (2, _ds_ok), 0x44: (2, _ds_ok),
        0x48: (1, _ds_ok), 0x4c: (5, _ds_ok)},
    1: {0x04: (1, _ds_ok), 0x08: (1, _ds_ok), 0x34: (4, _ds_ok), 0x38: (5, _ds_ok), 0x3c: (3, _ds_ok),
        0x44: (3, _ds_ok), 0x48: (3, _ds_ok), 0x4c: (5, _ds_ok), 0x50: (5, _ds_ok)},
}


def _ds_object(m, kind):
    """A new object of this kind; its vtable is made once per run, at the stub addresses."""
    if kind not in m.sound_vtables:
        vt = _alloc(m, 0, 0, 0x54)
        for slot in DS_VTABLES[kind]:
            m.poke(vt + slot, struct.pack("<I", DS_STUB_BASE + 0x100 * kind + slot))
        m.sound_vtables[kind] = vt
    obj = _alloc(m, 0, 0, 4)
    m.poke(obj, struct.pack("<I", m.sound_vtables[kind]))
    return obj


def _ds_create_buffer(m, this, desc, out, unk):
    """IDirectSound::CreateSoundBuffer: the buffer with dwBufferBytes of memory (DSBUFFERDESC + 8) and its 3D object."""
    size, = struct.unpack("<I", m.read(desc + 8, 4))
    obj = _ds_object(m, 0)
    m.sound_objects[obj] = {"data": _alloc(m, 0, 0, size), "size": size, "d3": _ds_object(m, 1)}
    m.poke(out, struct.pack("<I", obj))
    return 0


# Direct3D 9 vertex buffers, made by IDirect3DDevice9::CreateVertexBuffer (device vtable slot 26). The device is the
# snapshot's own object, and slot 26 is d3d9's internal 0x7a8bdd80 (no export, so it is named by its address). It is
# called from HedgeHog3D's CHH_Car::ProjectUVs (0x100165d0), reached from the exe's CSigCarDamage emit loop at 0x4ef23b
# through the signal's Invoke thunk (HedgeHog3D 0x1006c870). The buffer's vtable is ours, with stub addresses in d3d9's
# range (D3D_STUB_BASE + slot). Lock hands out a block of the buffer's length; the light buffer is only drawn by d3d9,
# so no game state depends on it. The stubs pop their caller's arguments and write only their out-pointers.
D3D_CREATE_VB = 0x7a8bdd80
D3D_STUB_BASE = 0x7a8f0000


def _d3d_create_vb(m, this, length, usage, fvf, pool, out, shared):
    if m.d3d_vtable is None:
        m.d3d_vtable = _alloc(m, 0, 0, 0x54)
        for slot in D3D_VB_VTABLE:
            m.poke(m.d3d_vtable + slot, struct.pack("<I", D3D_STUB_BASE + slot))
    obj = _alloc(m, 0, 0, 4)
    m.poke(obj, struct.pack("<I", m.d3d_vtable))
    m.d3d_buffers[obj] = {"data": _alloc(m, 0, 0, length), "size": length}
    m.poke(out, struct.pack("<I", obj))
    return 0


def _d3d_lock(m, this, off, nbytes, out, flags):
    """A block of the buffer's memory from off (the whole rest for nbytes 0)."""
    buf = m.d3d_buffers[this]
    m.poke(out, struct.pack("<I", buf["data"] + off))
    return 0


# IDirect3DVertexBuffer9: Release (slot 2), Lock (slot 11, four arguments and this), Unlock (slot 12)
D3D_VB_VTABLE = {0x08: (1, lambda m, *args: 0), 0x2c: (5, _d3d_lock), 0x30: (1, lambda m, *args: 0)}


STUBS = {
    "QueryPerformanceCounter": (1, _qpc), "RtlQueryPerformanceCounter": (1, _qpc),
    "QueryPerformanceFrequency": (1, _qpf), "RtlQueryPerformanceFrequency": (1, _qpf),
    "GetTickCount": (0, lambda m: 1000000), "timeGetTime": (0, lambda m: 1000000),
    "EnterCriticalSection": (1, lambda m, cs: 0), "RtlEnterCriticalSection": (1, lambda m, cs: 0),
    "LeaveCriticalSection": (1, lambda m, cs: 0), "RtlLeaveCriticalSection": (1, lambda m, cs: 0),
    # NGP's CRT locks and notifies its worker queue (std::mutex, std::condition_variable); the waiter is another thread
    "RtlAcquireSRWLockExclusive": (1, lambda m, lock: 0), "RtlReleaseSRWLockExclusive": (1, lambda m, lock: 0),
    "RtlWakeAllConditionVariable": (1, lambda m, cv: 0), "RtlWakeConditionVariable": (1, lambda m, cv: 0),
    "TlsGetValue": (1, _tls), "FlsGetValue": (1, _fls),
    # the TEB's thread id (the allocation log's hooks, RBR_ALLOCLOG, tag their records with it)
    "GetCurrentThreadId": (0, lambda m: struct.unpack("<I", m.read(m.snap.teb + 0x24, 4))[0]),
    "GetLastError": (0, lambda m: struct.unpack("<I", m.read(m.snap.teb + 0x34, 4))[0]),
    "SetLastError": (1, lambda m, e: m.poke(m.snap.teb + 0x34, struct.pack("<I", e))),
    "HeapAlloc": (3, _alloc), "RtlAllocateHeap": (3, _alloc),
    "HeapFree": (3, lambda m, heap, flags, block: 1), "RtlFreeHeap": (3, lambda m, heap, flags, block: 1),
    "HeapReAlloc": (4, _realloc), "RtlReAllocateHeap": (4, _realloc),
    "HeapSize": (3, lambda m, heap, flags, block: m.heap_sizes.get(block, 0)),
    "RtlSizeHeap": (3, lambda m, heap, flags, block: m.heap_sizes.get(block, 0)),
    "InterlockedIncrement": (1, _interlocked(1)), "InterlockedDecrement": (1, _interlocked(-1)),
    # the debugger sink of the game's log helper (E01Log, reached from VcuTableBlend's NaN branch); no game state
    "OutputDebugStringA": (1, lambda m, text: 0),
    # NGP's signal emits hand their listeners (sound) to its asio thread (notes/ngp.md); the handler runs there, not
    # in the step
    "PostQueuedCompletionStatus": (4, lambda m, port, n, key, overlapped: 1),
    # DirectSound 3D buffer calls from the sound module (out of scope; they take no car state): SetVelocity (slot 19,
    # E30DXSound::SetVel, 0x4d29a0) and SetPos (slot 20, 0x4d2a4b).
    "0x7b0cdb10": (5, lambda m, *args: 0),
    "0x7b0cdc80": (5, lambda m, *args: 0),
    # IDirectSound::CreateSoundBuffer (slot 3 of the IDirectSound vtable)
    "0x7b0cf5b0": (4, _ds_create_buffer),
}
STUBS.update({"%#x" % (DS_STUB_BASE + 0x100 * kind + slot): stub
              for kind, table in DS_VTABLES.items() for slot, stub in table.items()})
STUBS.update({"%#x" % (D3D_STUB_BASE + slot): stub for slot, stub in D3D_VB_VTABLE.items()})
STUBS["%#x" % D3D_CREATE_VB] = (7, _d3d_create_vb)


def run(snap, oracle, **kw):
    """A finished Machine; reruns while x87 cases are missing from the oracle (new operands can
    appear once earlier results change)."""
    for _ in range(8):
        m = Machine(snap, oracle=oracle, **kw)
        m.run()
        if not m.fpu_missing:
            return m
        oracle.fetch(m.fpu_missing)
    raise RuntimeError("x87 results still missing after 8 runs")


class Recording:
    """A harness recording, mapped: frames[pre] = (offset of the frame's control, its end, post step), the first
    frame of each pre step. Built once per path (recording())."""

    def __init__(self, path):
        with open(path, "rb") as f:
            self.data = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        self.frames, p = {}, 0
        while p + 20 <= len(self.data):
            _, pre, post, _, cs = struct.unpack_from("<5I", self.data, p)
            end = p + 20 + CONTROL_BYTES + cs + 4 * WHEEL_BYTES
            self.frames.setdefault(pre, (p + 20, end, post))
            p = end


_recordings = {}


def recording(path):
    if path not in _recordings:
        _recordings[path] = Recording(path)
    return _recordings[path]


def reg_reader(mu, reg):
    """A function returning a 32-bit register of mu, called through libunicorn directly (Uc.reg_read costs a few us)."""
    out = ctypes.c_uint32()
    ref, read, handle = ctypes.byref(out), uclib.uc_reg_read, mu._uch

    def get():
        read(handle, reg, ref)
        return out.value
    return get


MEM_HOOK = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_int, ctypes.c_uint64, ctypes.c_int, ctypes.c_int64,
                            ctypes.c_void_p)
CODE_HOOK = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_uint64, ctypes.c_uint32, ctypes.c_void_p)


def raw_hook(m, kind, fn, begin=1, end=0):
    """Hooks fn(addr, size, value) (memory accesses) or fn(addr, size) (UC_HOOK_CODE, UC_HOOK_BLOCK) into machine m's
    engine through libunicorn directly: Uc.hook_add's wrapper costs about 1 us a call. An exception in fn stops the
    run with m.stop."""
    def stop(e):
        m.stop = m.stop or "harness hook: %r" % e
        uclib.uc_emu_stop(m.mu._uch)
    if kind in (UC_HOOK_CODE, UC_HOOK_BLOCK):
        def cb(_, addr, size, __):
            try:
                fn(addr, size)
            except BaseException as e:
                stop(e)
        cb = CODE_HOOK(cb)
    else:
        def cb(_, access, addr, size, value, __):
            try:
                fn(addr, size, value)
            except BaseException as e:
                stop(e)
        cb = MEM_HOOK(cb)
    handle = ctypes.c_size_t()
    if uclib.uc_hook_add(m.mu._uch, ctypes.byref(handle), kind, ctypes.cast(cb, ctypes.c_void_p), None,
                         ctypes.c_uint64(begin), ctypes.c_uint64(end)):
        raise RuntimeError("uc_hook_add failed")
    m.__dict__.setdefault("raw_hooks", []).append(cb)  # alive as long as the machine


def recording_frame(path, pre):
    """The car + wheel bytes of the recording's frame for the call starting at step `pre`."""
    r = recording(path)
    if pre not in r.frames:
        raise KeyError("no frame for step %d in %s" % (pre, path))
    at, end, post = r.frames[pre]
    return r.data[at + CONTROL_BYTES:end], post


def byte_map(writes):
    """addr -> index of the last write covering it."""
    last = {}
    for i, (a, size, _, _) in enumerate(writes):
        for k in range(size):
            last[a + k] = i
    return last


def changed_bytes(a, b):
    """Addresses whose bytes differ between two snapshots (regions both hold at the same place)."""
    import numpy as np
    out = set()
    for base, size, off, *_ in a.regions:
        i = bisect.bisect_right(b.bases, base) - 1
        if i < 0 or b.regions[i][0] != base or b.regions[i][1] != size:
            continue
        x = np.frombuffer(a.data, np.uint8, size, off)
        y = np.frombuffer(b.data, np.uint8, size, b.regions[i][2])
        out.update(base + int(k) for k in np.nonzero(x != y)[0])
    return out


def differing(m1, m2):
    """Addresses whose final bytes differ between two runs of a snapshot: its writable regions and
    the stub heap."""
    import numpy as np
    out = []
    for (base, x), (base2, y) in zip(m1.views, m2.views):
        assert base == base2
        x64, y64 = np.frombuffer(x, np.uint64), np.frombuffer(y, np.uint64)
        words = np.flatnonzero(x64 != y64)
        if len(words):
            x8, y8 = np.frombuffer(x, np.uint8).reshape(-1, 8), np.frombuffer(y, np.uint8).reshape(-1, 8)
            hit = x8[words] != y8[words]
            out.extend(base + int(k) for k in (8 * words[:, None] + np.arange(8))[hit])
    return out


def stub_dependent(m):
    """Bytes whose value depends on the stubs (timer values, the stack garbage real Windows calls
    leave): they differ in runs with other timer values and stack garbage. Whole qwords."""
    out = set()
    for base, step, garbage in ((0x123456789ABC, 0x9876543, 0x5A), (0xFEDCBA987654, 0x1234567, 0xC3)):
        m2 = run(m.snap, m.oracle, log_writes=False, timer_base=base, timer_step=step, scribble=garbage)
        out |= {a & ~7 | k for a in differing(m, m2) for k in range(8)}  # a byte may agree by chance
    return out


def stack_below(snap):
    """The stack under the replayed call: the step's own frames, garbage once it returns."""
    i = bisect.bisect_right(snap.bases, snap.regs["esp"]) - 1
    alloc = snap.regions[i][5]
    return range(min(r[0] for r in snap.regions if r[5] == alloc), snap.regs["esp"])


def check(m, after, rec):
    """Run A against the real call: the recording's frame and the dump right after the call."""
    ok = True
    ref, post = recording_frame(rec, m.snap.step)
    got = m.frame()
    diff = [i for i in range(len(ref)) if ref[i] != got[i]]
    print("frame vs %s, call %d -> %d: %s" % (rec, m.snap.step, post,
          "bit-exact (%d bytes)" % len(ref) if not diff else "%d bytes differ, first at frame offset %#x" % (
              len(diff), diff[0])))
    ok &= not diff
    return ok and not compare_writes(m, Snapshot(after))


def check_return(m, after):
    """Run A from a load-time entry dump against the dump at its return: the registers and every byte
    run A wrote (check() does the same for a recorded step)."""
    asnap = Snapshot(after)
    diff = [(n, m.mu.reg_read(r), asnap.regs[n]) for n, r in REGISTERS if m.mu.reg_read(r) != asnap.regs[n]]
    print("registers at return: %s" % ("match the real call's" if not diff else ", ".join(
        "%s %08x, real %08x" % d for d in diff)))
    bad = compare_writes(m, asnap)
    return not diff and not bad


def compare_writes(m, asnap):
    """Run A's writes against the dump after the real call; prints the counts and returns the bad
    addresses (a byte that depends on the stubs or the stack below the call is not compared)."""
    written = byte_map(m.writes)
    addrs = sorted(written)
    timed = stub_dependent(m)
    stack = stack_below(m.snap)
    bad, gone, deeper, stubby = [], 0, 0, 0
    for a in addrs:
        if not asnap.has(a):
            gone += 1
        elif a in stack:
            deeper += 1
        elif a in timed:
            stubby += 1
        elif m.read(a, 1) != asnap.read(a, 1):
            bad.append(a)
    others = [a for a in changed_bytes(m.snap, asnap) if a not in written and a not in stack]
    print("run A wrote %d bytes (%d writes): %d differ from the real call's result; not compared: %d that depend "
          "on the stubbed calls (timers, their stack garbage), %d on the stack below the call, %d outside the dump" % (
              len(addrs), len(m.writes), len(bad), stubby, deeper, gone))
    print("%d bytes changed during the real call that run A didn't write (other threads)" % len(others))
    for a in bad[:20]:
        w = m.writes[written[a]]
        print("  %#x: run A %02x, real %02x (last write eip %#x)" % (a, m.read(a, 1)[0], asnap.read(a, 1)[0], w[3]))
    return bad


def where(m, addr):
    """What an address is: car/wheel offset, or the region kind."""
    car = struct.unpack("<I", m.read(CAR_PTR, 4))[0]  # 0 before the car is made (load-time dumps)
    if car and car <= addr < car + CAR_BYTES:
        return "car+%#x" % (addr - car)
    for i, w in enumerate(struct.unpack("<4I", m.read(car + 0x470, 16)) if car else ()):
        if w and w <= addr < w + 0x1428:  # the wheels lie 0x1428 apart; recordings keep the first 0xa00
            return "wheel%d+%#x%s" % (i, addr - w, "" if addr < w + WHEEL_BYTES else ", not recorded")
    if 0x400000 <= addr < 0x1800000:
        return "exe global"
    return "heap"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshot")
    ap.add_argument("--check", nargs=2, metavar=("AFTER_SNAPSHOT", "RECORDING"))
    ap.add_argument("--after", metavar="AFTER_SNAPSHOT", help="a load-time entry dump: compare run A with the dump "
                    "at its return")
    a = ap.parse_args()
    snap = Snapshot(a.snapshot)
    if snap.returns:
        ap.error("%s is a load-time dump at a return: replay its entry dump" % a.snapshot)
    if a.after and snap.version != 2:
        ap.error("--after needs a load-time entry dump")
    oracle = FpuOracle()
    if snap.version == 2:
        print("%s: load-time entry of %#x, call %d" % (a.snapshot, snap.step, snap.substeps))
    else:
        print("%s: call at step %d, %d substeps" % (a.snapshot, snap.step, snap.substeps))
    m = run(snap, oracle, log_writes=False)
    print("run A: %s, %.1f s; stubbed: %s; emulated as the CPU does: %s (%d x87 results from the CPU)" % (
        m.stop or "returned", m.seconds, m.stubbed or "nothing", m.special or "nothing", m.from_cpu))
    if m.stop:
        sys.exit(1)
    logged = []

    def logged_a():
        if not logged:
            logged.append(run(snap, oracle))
        return logged[0]

    if a.check and not check(logged_a(), *a.check):
        sys.exit(1)
    if a.after and not check_return(logged_a(), a.after):
        sys.exit(1)


if __name__ == "__main__":
    main()
