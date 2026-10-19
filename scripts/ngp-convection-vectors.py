"""Direct calls of NGP's tyre convection term (0x1004d410, rbr_physics::ngp::damper::convection) in the emulator, for
rust/rbr-physics/tests/ngp_convection.rs. The scratch is in esi: +0x10 the tyre, +0x410 the wheel (+0x7f4 its car),
+0x480 the boost flag, +0x1c8 the boost. The CRT pow is NGP's.

Usage: python3 -I scripts/ngp-convection-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (hex words): a, b, c, w7c, w8c, wec, kind, spin, car speed, has boost, boost, factor 0/1, factor 2/3,
temperature, then st0 (f64 bits).
"""
import importlib.util, math, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_FPSW, UC_X86_REG_FP0  # noqa: E402

CONVECTION = 0x1004D410
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def st0(mu):
    top = (mu.reg_read(UC_X86_REG_FPSW) >> 11) & 7
    mant, se = mu.reg_read(UC_X86_REG_FP0 + top)
    e, sign = se & 0x7FFF, -1.0 if se & 0x8000 else 1.0
    if e == 0 and mant == 0:
        return 0.0 * sign
    if e == 0x7FFF:
        return sign * math.inf if mant & ((1 << 63) - 1) == 0 else math.copysign(math.nan, sign)
    return sign * math.ldexp(mant, e - 16383 - 63)


def main(snap_path, out, count="3000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    data = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    h = m.heap_base
    scratch, tyre, wheel, car = h, h + 0x1000, h + 0x2000, h + 0x4000
    put = lambda at, fmt, *v: mu.mem_write(at, struct.pack(fmt, *v))
    put(scratch + 0x10, "<I", tyre)
    put(scratch + 0x410, "<I", wheel)
    put(wheel + 0x7F4, "<I", car)
    rng = random.Random(1)
    with open(out, "w") as f:
        for _ in range(int(count)):
            a, b = rng.uniform(250.0, 450.0), rng.uniform(280.0, 310.0)
            c = rng.uniform(0.0, 50.0)
            r = rng.uniform(0.25, 0.4)
            w8c, wec = rng.uniform(-0.05, 0.05), rng.uniform(0.0, 0.2)
            kind = rng.choice([0, 1, 2, 3, 4])
            spin = rng.uniform(-200.0, 200.0) if rng.random() < 0.9 else rng.choice([0.0, -0.0])
            speed = rng.uniform(-60.0, 60.0) if rng.random() < 0.9 else rng.choice([0.0, -0.0])
            has = rng.choice([0, 1])
            boost = rng.uniform(-0.5, 1.5)
            factors = [rng.uniform(1.0, 12.0), rng.uniform(1.0, 12.0)]
            temp = rng.uniform(260.0, 310.0)
            put(tyre + 0x7C, "<f", r)
            put(tyre + 0x8C, "<f", w8c)
            put(tyre + 0xEC, "<f", wec)
            put(tyre + 0x1C, "<I", kind)
            put(wheel + 0xEA0, "<f", spin)
            put(car + 0x15F8, "<f", speed)
            put(scratch + 0x480, "<I", has)
            put(scratch + 0x1C8, "<f", boost)
            put(data(0x10171470), "<2f", *factors)
            put(data(0x10171ADC), "<f", temp)
            put(esp, "<I4f", replay.SENTINEL, a, b, c, 0.54)
            mu.reg_write(UC_X86_REG_ESP, esp)
            mu.reg_write(UC_X86_REG_ESI, scratch)
            mu.emu_start(data(CONVECTION), replay.SENTINEL, timeout=10 * 1000000)
            ins = [F(a), F(b), F(c), F(r), F(w8c), F(wec), kind, F(spin), F(speed), has, F(boost)]
            ins += [F(x) for x in factors] + [F(temp)]
            f.write(" ".join("%08x" % w for w in ins) + " %016x\n" % struct.unpack("<Q", struct.pack("<d", st0(mu)))[0])


if __name__ == "__main__":
    main(*sys.argv[1:])
