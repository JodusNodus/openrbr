"""Direct calls of NGP's downforce (0x10047040, rbr_physics::ngp::downforce) in the emulator, with fuzzed inputs in
the replay's stub heap, for rust/rbr-physics/tests/ngp_downforce.rs. The argument p leads to the car through
*(*(p+8)+0x580); the car's definition is at car+0x34, its damage object at car+0x9f4 (or 0), its wheels at
car+0x470..0x47c.

Usage: python3 -I scripts/ngp-downforce-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (hex words): has damage, the two damage states, front, rear, density, speed, the four wheel arms, the two sums,
then the outputs: car+0x4a4, +0x4a8, +0x4ac, +0x378, +0x310, 1017147c and eax (8: no downforce; the speed's
direction; else the wheel pointer at car+0x474, written here as 0xffff0474).
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP  # noqa: E402

DOWNFORCE = 0x10047040
DENSITY, DRAG = 0x10171AE0, 0x1017147C
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def value(rng):
    """Mostly plausible floats, some zeros, negatives and NaN."""
    k = rng.random()
    if k < 0.08:
        return rng.choice([0.0, -0.0])
    if k < 0.1:
        return float("nan")
    return rng.uniform(-3000.0, 3000.0) if rng.random() < 0.3 else rng.uniform(0.0, 3000.0)


def main(snap_path, out, count="3000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    rebase = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    h = m.heap_base
    p, owner, car, cdef, dmg, wheels = h, h + 0x100, h + 0x1000, h + 0x3000, h + 0x4000, h + 0x9000
    put = lambda at, fmt, *v: mu.mem_write(at, struct.pack(fmt, *v))
    put(p + 8, "<I", owner)
    put(owner + 0x580, "<I", car)
    put(car + 0x34, "<I", cdef)
    for k in range(4):
        put(car + 0x470 + 4 * k, "<I", wheels + 0x1000 * k)
    rng = random.Random(1)
    with open(out, "w") as f:
        for _ in range(int(count)):
            has_dmg = rng.random() < 0.3
            states = [rng.choice([0, 1, 2, 3]) for _ in range(2)]
            front, rear = (value(rng) if rng.random() < 0.9 else 0.0 for _ in range(2))
            density = rng.uniform(0.9, 1.3)
            speed = value(rng) * (0.02 if rng.random() < 0.7 else 1.0)
            if rng.random() < 0.3:
                speed = -abs(speed)
            arms = [rng.uniform(-2.0, 2.0) for _ in range(4)]
            sums = [rng.uniform(-5000.0, 5000.0) for _ in range(2)]
            put(car + 0x9F4, "<I", dmg if has_dmg else 0)
            put(dmg + 0x3DB4, "<I", states[0])
            put(dmg + 0x3DBC, "<I", states[1])
            put(cdef + 0x1FC, "<f", front)
            put(cdef + 0x20C, "<f", rear)
            put(rebase(DENSITY), "<f", density)
            put(car + 0x15F8, "<f", speed)
            for k in range(4):
                put(wheels + 0x1000 * k + 0x8B4, "<f", arms[k])
            put(car + 0x378, "<f", sums[0])
            put(car + 0x310, "<f", sums[1])
            put(esp, "<II", replay.SENTINEL, p)
            mu.reg_write(UC_X86_REG_ESP, esp)
            mu.emu_start(rebase(DOWNFORCE), replay.SENTINEL, timeout=10 * 1000000)
            outs = [car + 0x4A4, car + 0x4A8, car + 0x4AC, car + 0x378, car + 0x310, rebase(DRAG)]
            words = [int(has_dmg)] + states + [F(x) for x in [front, rear, density, speed] + arms + sums]
            words += [struct.unpack("<I", mu.mem_read(a, 4))[0] for a in outs]
            eax = mu.reg_read(UC_X86_REG_EAX)
            words.append(0xFFFF0474 if eax == wheels + 0x1000 else eax)
            f.write(" ".join("%08x" % w for w in words) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])
