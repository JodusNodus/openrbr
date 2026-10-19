"""Direct runs of NGP's CDrive::M5FE0 and M6260 rewrites (0x100458c0, 0x10045b80; rbr_physics::ngp::drive) in the
emulator, for crates/rbr-physics/tests/ngp_drive.rs. Each run enters past the function's call of 0x10013cd0 (a
thread-local lookup whose result they drop), with the frame it leaves: M5FE0 at 0x100458e2, M6260 at 0x10045bd1
(its path with the block's gate clear). The worker: this+4 the definition, this+8 the state, this+0x170 the object.

Usage: python3 -I scripts/ngp-drive-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (hex words): the function, x, the split, the wheel's pack, weight, the axle's direction, scale, the torque
scale, the gate, the latch bound, enabled, the wheel offset, terms[0] and terms[pack] speeds, the drive gain, the
pack gain, loc0, pending, latch count; then step_vec[x], pack_out[x], the gate, the dirty word, pending, latch count.
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import (UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EDI,  # noqa: E402
                               UC_X86_REG_ESI, UC_X86_REG_ESP)

M5FE0, M6260 = 0x100458C0, 0x10045B80
ENTER = {M5FE0: 0x100458E2, M6260: 0x10045BD1}
SPLIT = 0x10171A90
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def value(rng):
    k = rng.random()
    if k < 0.05:
        return rng.choice([0.0, -0.0])
    if k < 0.07:
        return float("nan")
    return rng.uniform(-10.0, 10.0) * 10 ** rng.uniform(-2, 2)


def main(snap_path, out, count="1500"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    ebp = mu.reg_read(UC_X86_REG_ESP) - 0x200
    h = m.heap_base
    this, cdef, state, obj = h, h + 0x1000, h + 0x3000, h + 0x5000
    put = lambda at, fmt, *v: mu.mem_write(at, struct.pack(fmt, *v))
    put(this + 4, "<II", cdef, state)
    put(this + 0x170, "<I", obj)
    rng = random.Random(1)
    with open(out, "w") as f:
        for fn in (M5FE0, M6260):
            for _ in range(int(count)):
                x, pack = rng.randrange(4), rng.randrange(3)
                split = 0.5 if rng.random() < 0.5 else rng.uniform(0.0, 1.0)
                gate = rng.choice([-1, 1]) if fn == M5FE0 and rng.random() < 0.9 else 0
                ints = {"dir": rng.choice([-1, 0, 1, 1]), "enabled": rng.choice([0, 1, 1]),
                        "pending": rng.choice([0, 1]), "count": rng.randrange(5)}
                fl = {k: value(rng) for k in ("weight", "scale", "torque", "bound", "offset", "speed0", "speedp",
                                               "drive", "pack", "loc0")}
                wheel, axle, blk = cdef + 0xF0 * x, cdef + 0x60 * pack, state + 0xC8 * x
                put(base + SPLIT - 0x10000000, "<f", split)
                put(wheel + 0x148, "<f", fl["weight"])
                put(wheel + 0x158, "<I", pack)
                put(axle + 0x18, "<i", ints["dir"])
                put(axle + 0x48, "<f", fl["scale"])
                put(cdef + 0x48, "<f", fl["torque"])
                put(blk + 0xA0, "<I", 0)
                put(blk + 0xA4, "<i", gate)
                put(blk + 0xF8, "<f", fl["bound"])
                put(obj + 0x18 + 12 * x, "<I", ints["enabled"])
                put(this + 0xF4 + 16 * x, "<f", fl["offset"])
                put(state + 0x1C, "<f", fl["speed0"])
                if pack:
                    put(state + 0x1C + 20 * pack, "<f", fl["speedp"])
                put(state + 0x37C, "<f", fl["drive"])
                put(state + 0x460, "<f", fl["pack"])
                put(this + 0x90, "<f", fl["loc0"])
                put(state + 0x5BC, "<I", ints["pending"])
                put(state + 0x5A4, "<I", ints["count"])
                put(state + 0x530 + 4 * x, "<I", 0xDEADBEEF)
                put(this + 0xC0 + 4 * x, "<I", 0xDEADBEEF)
                if fn == M5FE0:
                    put(ebp - 0xC, "<I", obj)
                    put(ebp - 4, "<I", cdef)
                    put(ebp, "<III", 0, replay.SENTINEL, x)
                    mu.reg_write(UC_X86_REG_EBX, state)
                else:
                    put(ebp - 0x14, "<I", obj)
                    put(ebp - 0xC, "<III", cdef, wheel, blk)
                    put(ebp, "<III", 0, replay.SENTINEL, pack)
                    mu.reg_write(UC_X86_REG_EBX, x)
                    mu.reg_write(UC_X86_REG_ESI, state)
                mu.reg_write(UC_X86_REG_EDI, this)
                mu.reg_write(UC_X86_REG_EBP, ebp)
                mu.reg_write(UC_X86_REG_ESP, ebp - 0x20)
                mu.emu_start(ENTER[fn] - 0x10000000 + base, replay.SENTINEL, timeout=10 * 1000000)
                speedp = fl["speed0"] if pack == 0 else fl["speedp"]
                ins = [fn, x, F(split), pack, F(fl["weight"]), ints["dir"] & 0xFFFFFFFF, F(fl["scale"]),
                       F(fl["torque"]), gate & 0xFFFFFFFF, F(fl["bound"]), ints["enabled"], F(fl["offset"]),
                       F(fl["speed0"]), F(speedp), F(fl["drive"]), F(fl["pack"]), F(fl["loc0"]),
                       ints["pending"], ints["count"]]
                rd = lambda at: struct.unpack("<I", mu.mem_read(at, 4))[0]
                outs = [rd(state + 0x530 + 4 * x), rd(this + 0xC0 + 4 * x), rd(blk + 0xA4), rd(blk + 0xA0),
                        rd(state + 0x5BC), rd(state + 0x5A4)]
                f.write(" ".join("%08x" % w for w in ins + outs) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])
