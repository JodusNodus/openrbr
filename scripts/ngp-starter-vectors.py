"""Direct calls of NGP's starter 0x100456c0 (rbr_physics::ngp::starter) in the emulator, at the snapshot's x87
control word, for crates/rbr-physics/tests/ngp_starter.rs. Each call runs on a made-up drive (+8 the state, +0x170
the worker object) with its stamp (10176718) set so that the wall-clock time since it is the fuzzed hold E (us):
the timer is read once by calling NGP's 0x10111ce0, then rewound for the starter's own read.

Usage: python3 -I scripts/ngp-starter-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines: gate, ready, E (u64), then dt, band, speed, term, rate (f32 bits), then st0 (f64 bits), +0x1a0, +0x1c8, the
term, the speed (f32 bits) and 1 when the stamp was reset to the timer (0 when kept).
"""
import importlib.util, math, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP,  # noqa
                               UC_X86_REG_FPSW, UC_X86_REG_FPTAG, UC_X86_REG_FP0)

STARTER, TIMER, STAMP, BAND = 0x100456C0, 0x10111CE0, 0x10176718, 0x10171A98
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


def hold(rng, dt_us):
    k = rng.random()
    if k < 0.4:
        return rng.choice((500000, 500001, 2500000, 2500001)) + rng.choice((0, 0, -1, 1))
    return rng.randint(dt_us, 3000000)


def main(snap_path, out, count="2000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    rebase = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    drive, state, obj = m.heap_base, m.heap_base + 0x1000, m.heap_base + 0x2000
    sw, tag = mu.reg_read(UC_X86_REG_FPSW), mu.reg_read(UC_X86_REG_FPTAG)
    rng = random.Random(1)
    put = lambda a, fmt, *v: mu.mem_write(a, struct.pack(fmt, *v))
    get = lambda a: struct.unpack("<I", mu.mem_read(a, 4))[0]

    def call(fn, ecx, args):
        mu.mem_write(esp, struct.pack("<I%dI" % len(args), replay.SENTINEL, *args))
        mu.reg_write(UC_X86_REG_ESP, esp)
        mu.reg_write(UC_X86_REG_ECX, ecx)
        mu.reg_write(UC_X86_REG_FPSW, sw)
        mu.reg_write(UC_X86_REG_FPTAG, tag)
        mu.emu_start(rebase(fn), replay.SENTINEL, timeout=10 * 1000000)

    with open(out, "w") as f:
        for _ in range(int(count)):
            gate = int(rng.random() < 0.85)
            ready = int(rng.random() < 0.9)
            dt = rng.choice((0.0025, 0.0025, rng.uniform(0.0005, 0.02)))
            dt_us = math.floor(struct.unpack("<f", struct.pack("<f", dt))[0] * 1e6 + 0.5)
            e = hold(rng, dt_us)
            band = rng.uniform(60.0, 160.0)
            k = rng.random()
            speed = (0.0 if k < 0.15 else rng.uniform(-0.002, 0.002) if k < 0.25 else
                     rng.uniform(0.6, 1.1) * band if k < 0.7 else rng.uniform(-50.0, 200.0))
            term = rng.uniform(-0.1, 1.0) if rng.random() < 0.95 else float("nan")
            rate = rng.uniform(0.0, 50.0)
            mu.mem_write(drive, bytes(0x3000))
            put(drive + 8, "<I", state)
            put(drive + 0x170, "<I", obj)
            put(drive + 0x50, "<I", gate)
            put(state + 0x45C, "<f", speed)
            put(obj + 0x7C, "<I", ready)
            put(obj + 0x8C, "<f", term)
            put(obj + 0x74, "<f", rate)
            put(rebase(BAND), "<f", band)
            ticks = m.ticks
            call(TIMER, 0, [])
            now = mu.reg_read(UC_X86_REG_EAX) | mu.reg_read(UC_X86_REG_EDX) << 32
            m.ticks = ticks
            put(rebase(STAMP), "<Q", now - e)
            call(STARTER, drive, [F(dt)])
            r = struct.unpack("<Q", struct.pack("<d", st0(mu)))[0]
            stamp = struct.unpack("<Q", mu.mem_read(rebase(STAMP), 8))[0]
            assert stamp in (now, now - e)
            outs = [get(drive + 0x1A0), get(drive + 0x1C8), get(obj + 0x8C), get(state + 0x45C)]
            ins = [F(x) for x in (dt, band, speed, term, rate)]
            f.write("%d %d %x " % (gate, ready, e) + " ".join("%08x" % w for w in ins) + " %016x " % r
                    + " ".join("%08x" % w for w in outs) + " %d\n" % (stamp == now and e != 0))


if __name__ == "__main__":
    main(*sys.argv[1:])
