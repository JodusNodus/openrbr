"""Direct calls of NGP's damper helpers (rbr_physics::ngp::damper) in the emulator, at the snapshot's x87 control
word (0x27f under NGP), for rust/rbr-physics/tests/ngp_damper.rs:
  0x1004d2f0 DamperStep1 (a, b, c, d), with the air temperature 10171adc;
  0x1004d3c0 (a, b, c), esi = the scratch: DamperStep1 with the scratch factor 0x1004d340 as d;
  0x1004d380 DamperStep2 (a, b, c), esi = the scratch.
The scratch's words are at *(scratch+0x10) +0x7c, +0x8c, +0xac.

Usage: python3 -I scripts/ngp-damper-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines: the function's address, a, b, c, d, the temperature, the three scratch words (f32 bits), then st0 (f64 bits;
the x87 value has at most 53 significant bits at this control word).
"""
import importlib.util, math, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_FPSW, UC_X86_REG_FPTAG, UC_X86_REG_FP0  # noqa

FUNCS = (0x1004D2F0, 0x1004D3C0, 0x1004D380)
TEMPERATURE = 0x10171ADC
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def st0(mu):
    top = (mu.reg_read(UC_X86_REG_FPSW) >> 11) & 7
    mant, se = mu.reg_read(UC_X86_REG_FP0 + top)
    e = se & 0x7FFF
    sign = -1.0 if se & 0x8000 else 1.0
    if e == 0 and mant == 0:
        return 0.0 * sign
    if e == 0x7FFF:
        return sign * math.inf if mant & ((1 << 63) - 1) == 0 else math.copysign(math.nan, sign)
    assert mant & 0x7FF == 0, "more than 53 bits"
    return sign * math.ldexp(mant, e - 16383 - 63)


def value(rng):
    k = rng.random()
    if k < 0.05:
        return 0.0
    if k < 0.07:
        return float("nan")
    return rng.uniform(-1e4, 1e4) * 10 ** rng.uniform(-4, 0)


def main(snap_path, out, count="1000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    rebase = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    scratch, inner = m.heap_base, m.heap_base + 0x100
    mu.mem_write(scratch + 0x10, struct.pack("<I", inner))
    sw, tag = mu.reg_read(UC_X86_REG_FPSW), mu.reg_read(UC_X86_REG_FPTAG)
    rng = random.Random(1)
    with open(out, "w") as f:
        for fn in FUNCS:
            for _ in range(int(count)):
                a, b, c, d = (value(rng) for _ in range(4))
                if fn != 0x1004D380 and rng.random() < 0.5:
                    b = rng.uniform(250.0, 400.0)  # a temperature
                temp = rng.uniform(270.0, 300.0)
                words = [value(rng), value(rng), rng.uniform(0.0, 2.0)]
                mu.mem_write(rebase(TEMPERATURE), struct.pack("<f", temp))
                mu.mem_write(inner + 0x7C, struct.pack("<f", words[0]))
                mu.mem_write(inner + 0x8C, struct.pack("<f", words[1]))
                mu.mem_write(inner + 0xAC, struct.pack("<f", words[2]))
                mu.mem_write(esp, struct.pack("<I4f", replay.SENTINEL, a, b, c, d))
                mu.reg_write(UC_X86_REG_ESP, esp)
                mu.reg_write(UC_X86_REG_ESI, scratch)
                mu.reg_write(UC_X86_REG_FPSW, sw)
                mu.reg_write(UC_X86_REG_FPTAG, tag)
                mu.emu_start(rebase(fn), replay.SENTINEL, timeout=10 * 1000000)
                r = struct.unpack("<Q", struct.pack("<d", st0(mu)))[0]
                ins = [F(x) for x in (a, b, c, d, temp)] + [F(x) for x in words]
                f.write("%08x " % fn + " ".join("%08x" % w for w in ins) + " %016x\n" % r)


if __name__ == "__main__":
    main(*sys.argv[1:])
