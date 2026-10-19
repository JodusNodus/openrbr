"""Direct runs of NGP's M3D230 rewrite (0x1005e8a0, rbr_physics::ngp::tag_blend::blend) in the emulator, for
crates/rbr-physics/tests/ngp_tag_blend.rs. The run enters at 0x1005e8f3, after the two material lookups (which go
through NGP's thread-local call of the exe's Tag520940), with the frame they leave: esi the block, ebx and eax the
two materials' entries, [ebp+8] the weight. The sines are NGP's CRT sin (0x10122149).

Usage: python3 -I scripts/ngp-tag-blend-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (f32 bits in hex): the weight, the phases (+0x10, +0x14), the two materials' period, gain, lerp a and lerp b,
then the blend (+0x54) and the total (+0x58).
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX,  # noqa: E402
                               UC_X86_REG_ESI, UC_X86_REG_ESP)

ENTER = 0x1005E8F3
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def value(rng):
    k = rng.random()
    if k < 0.05:
        return rng.choice([0.0, -0.0])
    if k < 0.07:
        return float("nan")
    return rng.uniform(-10.0, 10.0) * 10 ** rng.uniform(-2, 2)


def main(snap_path, out, count="3000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    ebp = mu.reg_read(UC_X86_REG_ESP) - 0x200
    blk, o1, o2 = m.heap_base, m.heap_base + 0x100, m.heap_base + 0x200
    put = lambda at, fmt, *v: mu.mem_write(at, struct.pack(fmt, *v))
    rng = random.Random(1)
    with open(out, "w") as f:
        for _ in range(int(count)):
            w = rng.uniform(0.0, 1.0) if rng.random() < 0.8 else value(rng)
            phases = [value(rng) for _ in range(2)]
            mats = []
            for _ in range(2):
                period = rng.choice([0.0, value(rng)]) if rng.random() < 0.3 else rng.uniform(0.05, 5.0)
                mats.append([period] + [value(rng) for _ in range(3)])
            put(blk + 0x10, "<2f", *phases)
            for at, (period, gain, la, lb) in ((o1, mats[0]), (o2, mats[1])):
                put(at + 0xD8, "<f", period)
                put(at + 0xC8, "<f", gain)
                put(at + 0x18, "<f", la)
                put(at + 0xB8, "<f", lb)
            put(ebp, "<IIf", 0, replay.SENTINEL, w)
            mu.reg_write(UC_X86_REG_EBP, ebp)
            mu.reg_write(UC_X86_REG_ESP, ebp - 0x20)
            mu.reg_write(UC_X86_REG_ESI, blk)
            mu.reg_write(UC_X86_REG_EBX, o1)
            mu.reg_write(UC_X86_REG_EAX, o2)
            mu.emu_start(ENTER - 0x10000000 + base, replay.SENTINEL, timeout=10 * 1000000)
            ins = [F(x) for x in [w] + phases + mats[0] + mats[1]]
            outs = list(struct.unpack("<2I", mu.mem_read(blk + 0x54, 8)))
            f.write(" ".join("%08x" % x for x in ins + outs) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])
