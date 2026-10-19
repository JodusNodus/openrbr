"""Direct calls of NGP's CDrive::M93A0 rewrite (0x10047300, rbr_physics::engine_driveline::drive_worker::clutch_torque
under Variant::Ngp778) in the emulator, for rust/rbr-physics/tests/ngp_clutch_torque.rs. Only the paths without
callees are fuzzed (the sign word set, or the gear's ratio or the gear flag zero): they hold NGP's changes, its sign
and the zeroed speed at a sign change; the others call the exe's M9120/M9210 through NGP's thread-local lookup.
At most one input is NaN and no divisor or factor of d is zero, so no NaN meets another and none is made: which one
an x86 add keeps, and the sign of a made one, the emulator and arm64 don't model.
The worker: this+4 the definition, this+8 the state, gear 0.

Usage: python3 -I scripts/ngp-clutch-torque-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (hex words): ratio, gear flag, sign word, trigger, speed, key, slip a, slip b, a, b, c, d, then the outputs:
*out (0xdeadbeef when unwritten), speed, sign word, trigger.
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP  # noqa: E402

M93A0 = 0x10047300
UNWRITTEN = 0xDEADBEEF
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]


def value(rng):
    k = rng.random()
    if k < 0.06:
        return rng.choice([0.0, -0.0])
    if k < 0.09:
        return rng.choice([float("nan"), -float("nan")])
    return rng.uniform(-50.0, 50.0) * 10 ** rng.uniform(-3, 2)


def main(snap_path, out, count="3000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    h = m.heap_base
    this, cdef, state, res = h, h + 0x1000, h + 0x3000, h + 0x5000
    put = lambda at, fmt, *v: mu.mem_write(at, struct.pack(fmt, *v))
    put(this + 4, "<II", cdef, state)
    put(state + 0x54C, "<I", 0)
    rng = random.Random(1)
    with open(out, "w") as f:
        for _ in range(int(count)):
            ratio = rng.choice([0.0, 1.0, 2.5, float("nan")]) if rng.random() < 0.3 else rng.uniform(0.5, 4.0)
            flag = 0 if rng.random() < 0.2 else 1
            word = rng.choice([-1, 1]) if ratio != 0.0 and flag else rng.choice([-1, 0, 1])
            trigger = rng.choice([0, 1])
            while True:
                speed, key, a, b, c = (value(rng) for _ in range(5))
                if rng.random() < 0.4:
                    speed = rng.uniform(-1.0, 1.0)  # near zero: sign changes
                slip_a, slip_b, d = (rng.uniform(0.01, 10.0) if rng.random() < 0.9 else value(rng) for _ in range(3))
                xs = (speed, key, a, b, c, slip_a, slip_b, d)
                if sum(x != x for x in xs) <= 1 and 0.0 not in (slip_a, slip_a + slip_b, d):
                    break
            put(cdef + 0x694, "<f", ratio)
            put(cdef + 0x664, "<f", slip_a)
            put(cdef + 0x674, "<f", slip_b)
            put(state + 0x55C, "<I", flag)
            put(state + 0x368, "<i", word)
            put(state + 0x578, "<I", trigger)
            put(state + 0x45C, "<f", speed)
            put(state + 0x384, "<f", key)
            put(res, "<I", UNWRITTEN)
            put(esp, "<IfIfff", replay.SENTINEL, a, res, b, c, d)
            mu.reg_write(UC_X86_REG_ESP, esp)
            mu.reg_write(UC_X86_REG_ECX, this)
            mu.emu_start(M93A0 - 0x10000000 + base, replay.SENTINEL, timeout=10 * 1000000)
            ins = [F(ratio), flag, word & 0xFFFFFFFF, trigger] + [F(x) for x in (speed, key, slip_a, slip_b, a, b, c, d)]
            outs = [struct.unpack("<I", mu.mem_read(at, 4))[0] for at in (res, state + 0x45C, state + 0x368, state + 0x578)]
            f.write(" ".join("%08x" % w for w in ins + outs) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])
