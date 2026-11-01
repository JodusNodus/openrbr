"""Checks the emulator's x87 against this machine's CPU (the reference drives ran on x86_64).

Usage: .venv/bin/python -I harness/replay-fpu.py [SNAPSHOT...]
  Cases: random operands for every arithmetic and transcendental x87 op at 24-bit (the game's
  physics thread: control word 0x7f) and 64-bit precision, plus the operands the transcendentals
  get when each SNAPSHOT's call is replayed. Each case runs fninit; fldcw; fld b; fld a; <op>;
  fstp r0; fstp r1 in unicorn and natively (harness/replay-fpu.c, compiled into build/replay-fpu/)
  and the 80-bit results are compared. Prints mismatches per op. Needs an x86_64 CPU.
"""
import importlib.util
import os
import platform
import random
import struct
import subprocess
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_PROT_ALL

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("replay", os.path.join(HERE, "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)

OPS = ["fadd", "fsub", "fmul", "fdiv", "fsqrt", "fsin", "fcos", "fsincos", "fptan", "fpatan", "fyl2x", "f2xm1",
       "fscale", "frndint", "fprem"]
CODE = [b"\xd8\xc1", b"\xd8\xe1", b"\xd8\xc9", b"\xd8\xf1", b"\xd9\xfa", b"\xd9\xfe", b"\xd9\xff", b"\xd9\xfb",
        b"\xd9\xf2", b"\xd9\xf3", b"\xd9\xf1", b"\xd9\xf0", b"\xd9\xfd", b"\xd9\xfc", b"\xd9\xf8"]
BY_X87 = {0xFE: 5, 0xFF: 6, 0xFB: 7, 0xF2: 8, 0xF3: 9, 0xF1: 10, 0xF0: 11, 0xFD: 12, 0xFC: 13, 0xF8: 14}
BUILD = os.path.join(os.path.dirname(HERE), "build", "replay-fpu")  # the native oracle lives here


def f80(x):
    """float -> 80-bit bytes (exact for doubles)."""
    if x == 0:
        return b"\0" * 8 + struct.pack("<H", 0x8000 if str(x)[0] == "-" else 0)
    b, = struct.unpack("<Q", struct.pack("<d", x))
    s, e, m = b >> 63, (b >> 52) & 0x7FF, b & ((1 << 52) - 1)
    return struct.pack("<QH", (1 << 63) | (m << 11), (s << 15) | (e - 1023 + 16383))


def random_cases(n, rng):
    out = []
    for _ in range(n):
        op = rng.randrange(len(OPS))
        cw = rng.choice((0x7F, 0x7F, 0x37F))
        def operand():
            kind = rng.random()
            if kind < 0.6:  # a float the game would load
                x = struct.unpack("<f", struct.pack("<f", rng.uniform(-1, 1) * 10 ** rng.uniform(-6, 4)))[0]
                return f80(x)
            if kind < 0.9:  # a full 64-bit mantissa (a 64-bit precision chain)
                return struct.pack("<QH", rng.getrandbits(64) | (1 << 63), rng.randrange(16383 - 30, 16383 + 15)
                                   | (rng.getrandbits(1) << 15))
            return f80(rng.choice((0.0, -0.0, 1.0, -1.0, 0.5, 3.141592653589793, 1e-30, 1e30)))
        a, b = operand(), operand()
        if OPS[op] in ("fsqrt", "fyl2x") and a[9] & 0x80:
            a = a[:9] + bytes([a[9] & 0x7F])
        if OPS[op] == "f2xm1":  # defined on [-1, 1]
            a = f80(rng.uniform(-1, 1))
        out.append((op, cw, a, b))
    return out


def snapshot_cases(path):
    m = replay.Machine(replay.Snapshot(path), log_writes=False)
    m.fpu_log = []
    m.run()
    return [(BY_X87[op], cw, a, b) for op, a, b, cw in m.fpu_log]


def emulate(cases):
    mu = Uc(UC_ARCH_X86, UC_MODE_32)
    mu.mem_map(0x1000, 0x10000, UC_PROT_ALL)
    out = []
    for op, cw, a, b in cases:
        code = (b"\xdb\xe3" + b"\xd9\x2d" + struct.pack("<I", 0x8000) + b"\xdb\x2d" + struct.pack("<I", 0x8010)
                + b"\xdb\x2d" + struct.pack("<I", 0x8020) + CODE[op] + b"\xdb\x3d" + struct.pack("<I", 0x8100)
                + b"\xdb\x3d" + struct.pack("<I", 0x8110))
        mu.mem_write(0x8000, struct.pack("<H", cw))
        mu.mem_write(0x8010, b)
        mu.mem_write(0x8020, a)
        mu.mem_write(0x8100, b"\0" * 32)
        mu.mem_write(0x1000, code)
        mu.emu_start(0x1000, 0x1000 + len(code))
        out.append(bytes(mu.mem_read(0x8100, 10)) + bytes(mu.mem_read(0x8110, 10)))
    return out


def native(cases):
    if platform.machine() not in ("x86_64", "AMD64"):
        sys.exit("replay-fpu.py: the x87 check runs on this machine's CPU, which is not x86_64 (%s)" %
                 platform.machine())
    data = b"".join(struct.pack("<BBH", op, 0, cw) + a + b for op, cw, a, b in cases)
    os.makedirs(BUILD, exist_ok=True)
    exe = os.path.join(BUILD, "replay-fpu")
    subprocess.run(["gcc", "-O1", "-o", exe, os.path.join(HERE, "replay-fpu.c")], check=True)
    r = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [r[22 * i:22 * i + 20] for i in range(len(cases))]


def value(x):
    m, e = struct.unpack("<QH", x)
    return "%s%016x*2^%d" % ("-" if e & 0x8000 else "", m, (e & 0x7FFF) - 16383 - 63)


def main():
    rng = random.Random(1)
    cases = [("random", c) for c in random_cases(60000, rng)]
    for path in sys.argv[1:]:
        cases += [(os.path.basename(path), c) for c in snapshot_cases(path)]
    emu, nat = emulate([c for _, c in cases]), native([c for _, c in cases])
    stats = {}
    for (src, (op, cw, a, b)), e, n in zip(cases, emu, nat):
        pops = OPS[op] in ("fpatan", "fyl2x")  # r1 is an empty-stack store
        same = e[:10] == n[:10] and (pops or e[10:] == n[10:])
        key = (src if src == "random" else "replay", OPS[op], cw)
        st = stats.setdefault(key, [0, 0, None])
        st[0] += 1
        if not same:
            st[1] += 1
            st[2] = st[2] or "a %s b %s: emulator %s, cpu %s" % (value(a), value(b), value(e[:10]), value(n[:10]))
    for (src, op, cw), (n, bad, first) in sorted(stats.items()):
        print("%-7s %-8s cw %#05x: %6d cases, %5d differ%s" % (src, op, cw, n, bad, "  e.g. " + first if bad else ""))


if __name__ == "__main__":
    main()
