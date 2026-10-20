"""Direct calls of NGP's engine wrapper 0x1004b8e0 (the engine's wear and death) and of its idle table's lookup
0x1004a700 in the emulator, for crates/rbr-physics/tests/ngp_engine.rs (notes/ngp.md "Engine wrapper"), and of the
1.02 functions NGP repoints at its `.data` or its code (`Sim8890`, `SimE970`, `SimECF0`, `LaunchClutch`).

Usage: python3 -I scripts/ngp-engine-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)

Wrapper lines `w`: alive, then dt, speed, the rev limit (rpm) and the health (f32 bits), then the health and the alive
flag after, and the emits (calls of NGP's strand post 0x1004a2f0 or its inline emit 0x1004a8d0). Each call runs on the
snapshot's car with the health object's +0x58 and +0x74, and the rev limit, set; the speed is mostly around 1.05
times the limit and the health around 0.101, so the wear and the death are both taken.
Lookup lines `t`: n, then v and the doubles at knot slots -1..n+1 of x, then of y (f64 bits), then st0 (f64 bits). Each
call runs on a table on the stub heap with n knots (0 to 4, mostly 0 to 2), the doubles around them random, and v
mostly on a knot.
Repointed lines `c`: the function, the fill's seed and the stack words, then `|` the words set over the fill, `|` the
words that changed (after), `|` eax and st0 (f64 bits; SimE970's, else 0), `|` Sim8890's walk (S30's eax, MB560's a
and b, and its four floats; empty without the walk). Words are `address=value` (hex). The objects are in the stub heap
(`OBJECTS`): the engine's def, state and options random (`fill`, as `harness/vector_specs/engine_driveline.py`'s
recipes), the rest 0; Sim8890's state `+0xc` is the snapshot's car, so its walk runs on the car. A `d` line first:
NGP's tuning words they read, at NGP's file base.
"""
import importlib.util, math, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn import UC_HOOK_CODE  # noqa: E402
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_ESP,  # noqa: E402
                               UC_X86_REG_FPSW, UC_X86_REG_FPTAG, UC_X86_REG_FP0)

WRAPPER, LOOKUP, POST, INLINE, TABLE_VTABLE = 0x1004B8E0, 0x1004A700, 0x1004A2F0, 0x1004A8D0, 0x10148930
F = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]
D = lambda x: struct.unpack("<Q", struct.pack("<d", x))[0]
SIM8890, SIM_E970, SIM_ECF0, LAUNCH_CLUTCH = 0x518890, 0x50E970, 0x50ECF0, 0x5375B0
OBJECTS = 0x800000  # the objects' offset in the stub heap, past what its allocations take
# The random words of the objects (offsets from OBJECTS, scale): the engine's def, state and options block.
FILLED = ((0x4000, 0x6880, 2), (0x8000, 0x8274, 1), (0xA000, 0xA080, 1))
# NGP's words they read: the stall speed, the air temperature, the clutch band (from, to, slope).
TUNING = (0x10171ABC, 0x10171ADC, 0x10171AE4, 0x10172338, 0x101767C4)


def st0(mu):
    top = (mu.reg_read(UC_X86_REG_FPSW) >> 11) & 7
    mant, se = mu.reg_read(UC_X86_REG_FP0 + top)
    e, sign = se & 0x7FFF, -1.0 if se & 0x8000 else 1.0
    if e == 0 and mant == 0:
        return 0.0 * sign
    if e == 0x7FFF:
        return sign * math.inf if mant & ((1 << 63) - 1) == 0 else math.copysign(math.nan, sign)
    assert mant & 0x7FF == 0, "more than 53 bits"
    return sign * math.ldexp(mant, e - 16383 - 63)


def main(snap_path, out, count="1000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    rebase = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    sw, tag = mu.reg_read(UC_X86_REG_FPSW), mu.reg_read(UC_X86_REG_FPTAG)
    rng = random.Random(1)
    put = lambda a, fmt, *v: mu.mem_write(a, struct.pack(fmt, *v))
    get = lambda a, fmt="<I": struct.unpack(fmt, mu.mem_read(a, struct.calcsize(fmt)))[0]
    emits = []
    mu.hook_add(UC_HOOK_CODE, lambda *_: emits.append(1), begin=rebase(POST), end=rebase(POST))
    mu.hook_add(UC_HOOK_CODE, lambda *_: emits.append(1), begin=rebase(INLINE), end=rebase(INLINE))
    # Sim8890's walk; added before any call, as a block translated before its hook is added does not run it.
    walk = []

    def on_walk(uc, at, *_):
        # After S30 (its eax), at MB560's call (a, b), after MB560 (its four floats at esp+0x20).
        if at == 0x518975:
            walk.append(uc.reg_read(UC_X86_REG_EAX))
        elif at == 0x51897D:
            walk.extend([uc.reg_read(UC_X86_REG_EBX), uc.reg_read(UC_X86_REG_EAX)])
        elif at == 0x518982:
            walk.extend(get(uc.reg_read(UC_X86_REG_ESP) + 0x20 + 4 * k) for k in range(4))
    mu.hook_add(UC_HOOK_CODE, on_walk, begin=0x518975, end=0x518982)

    def call(fn, ecx, words):
        mu.mem_write(esp, struct.pack("<I%dI" % len(words), replay.SENTINEL, *words))
        mu.reg_write(UC_X86_REG_ESP, esp)
        mu.reg_write(UC_X86_REG_ECX, ecx)
        mu.reg_write(UC_X86_REG_FPSW, sw)
        mu.reg_write(UC_X86_REG_FPTAG, tag)
        mu.emu_start(rebase(fn) if fn >= 0x10000000 else fn, replay.SENTINEL, timeout=10 * 1000000)

    car = get(0x8EF660)
    engine = car + 0x690
    health_at = get(engine + 0x48)
    rev_at = get(car + 0x850) + 0x188
    table = m.heap_base
    with open(out, "w") as f:
        for _ in range(int(count)):
            alive = int(rng.random() < 0.9)
            dt = rng.choice((1 / 720, 1 / 720, rng.uniform(0.0005, 0.05)))
            rpm = rng.uniform(5000.0, 9000.0)
            limit = rpm * 0.10471976
            speed = rng.choice((-1, 1, 1, 1)) * limit * rng.choice((1.05, rng.uniform(0.9, 1.2), rng.uniform(1.05, 1.1)))
            health = rng.choice((0.101, rng.uniform(0.0999, 0.1015), rng.uniform(0.0, 1.0)))
            put(health_at + 0x58, "<f", health)
            put(health_at + 0x74, "<I", alive)
            put(rev_at, "<f", rpm)
            emits.clear()
            ins = [F(x) for x in (dt, speed, rpm, health)]
            call(WRAPPER, engine, ins[:2])
            outs = [get(health_at + 0x58), get(health_at + 0x74), len(emits)]
            f.write("w %d " % alive + " ".join("%08x" % w for w in ins) + " %08x %d %d\n" % tuple(outs))
        for _ in range(int(count)):
            n = rng.choice((0, 1, 1, 2, 2, 3, 4))
            xs = sorted(rng.uniform(0.0, 900.0) for _ in range(n))
            slots = [[rng.uniform(-100.0, 1000.0)] + xs + [rng.uniform(-100.0, 1000.0) for _ in range(2)],
                     [rng.uniform(-1.0, 1.0) for _ in range(n + 3)]]
            if n and rng.random() < 0.2:
                slots[0][n + 1] = slots[0][n]  # a knot repeated past the end: the lerp divides by 0
            v = rng.choice(slots[0][1:n + 1] or slots[0]) if rng.random() < 0.6 else rng.uniform(-100.0, 1000.0)
            # The table: vtable, then begin/end/capacity of x at +8, of y at +0x14, each vector framed by its slots.
            x_at, y_at = table + 0x100, table + 0x200
            for at, vals in ((x_at, slots[0]), (y_at, slots[1])):
                mu.mem_write(at - 8, b"".join(struct.pack("<d", d) for d in vals))
            put(table, "<9I", rebase(TABLE_VTABLE), n, x_at, x_at + 8 * n, x_at + 8 * n, y_at, y_at + 8 * n,
                y_at + 8 * n, 0)
            lo, hi = struct.unpack("<II", struct.pack("<d", v))
            call(LOOKUP, table, [lo, hi])
            words = [D(v)] + [D(d) for d in slots[0] + slots[1]] + [D(st0(mu))]
            f.write("t %d " % n + " ".join("%016x" % w for w in words) + "\n")
        repointed(mu, call, rebase, car, m.heap_base + OBJECTS, rng, int(count) // 4, f, walk)


def fill(seed, at, scale):
    """The objects' random word at `at` (as the test's `fill`): splitmix64 of (seed, at), 24 bits as a float in
    [-scale, scale) (scale 1 or 2, so the value is exact in f32)."""
    z = ((seed << 32 | at) + 0x9E3779B97F4A7C15) & (1 << 64) - 1
    z = (z ^ z >> 30) * 0xBF58476D1CE4E5B9 & (1 << 64) - 1
    z = (z ^ z >> 27) * 0x94D049BB133111EB & (1 << 64) - 1
    return F(((z ^ z >> 31) >> 40) * scale / 8388608.0 - scale)


def repointed(mu, call, rebase, car, heap, rng, count, f, walk):
    """`d` line: NGP's tuning words these read. `c` lines: Sim8890 (half with the walk), SimE970, SimECF0 and
    LaunchClutch, `count` calls each."""
    # NGP's call in Sim8890 (0x518a32) and its constant in SimECF0 (0x50ecfe) are in the snapshot's .text.
    assert struct.unpack("<i", mu.mem_read(0x518A33, 4))[0] + 0x518A37 == rebase(0x1004D2F0)
    assert struct.unpack("<I", mu.mem_read(0x50ECFE, 4))[0] == rebase(0x10171ABC)
    get = lambda a: struct.unpack("<I", mu.mem_read(a, 4))[0]
    f.write("d " + " ".join("%x=%x" % (a, get(rebase(a))) for a in TUNING) + "\n")
    lo, hi = heap + 0x3000, heap + 0xD000
    this, def_at, state, opt = heap + 0x3000, heap + 0x4000, heap + 0x8000, heap + 0xA000
    scratch, drive, dstate, vdef, vstate = heap + 0xB000, heap + 0xB100, heap + 0xB200, heap + 0xC000, heap + 0xC400
    pick = lambda *v: rng.choice(v)
    for fn in (SIM8890, SIM_E970, SIM_ECF0, LAUNCH_CLUTCH):
        for _ in range(count):
            seed = rng.getrandbits(32)
            image = bytearray(hi - lo)
            for first, end, scale in FILLED:
                for at in range(heap + first, heap + end, 4):
                    struct.pack_into("<I", image, at - lo, fill(seed, at, scale))
            sets = {this + 4: def_at, this + 8: state, this + 0x48: opt}
            setf = lambda a, v: sets.__setitem__(a, F(v))
            words = []
            if fn == SIM8890:
                sets[state + 0xC] = car
                setf(state + 0x1C, pick(0.0, 0.0005, 0.002, 0.5, rng.uniform(-1, 1), rng.uniform(0.001, 2)))
                setf(state + 0x24, pick(0.0, 0.0005, 0.5, -1.0, 2.0, rng.uniform(-1, 2)))
                setf(state + 0x28, pick(0.0, 1.0, -1.0, 5.0, rng.uniform(-2, 5)))
                setf(state + 0x20, pick(0.0, 300.0, 350.0, rng.uniform(-1, 400)))
                setf(def_at + 0x2774, pick(0.0, 1.0, -0.5, rng.uniform(-1, 3)))
                words = [F(pick(0.02, 0.05, 1 / 720, rng.uniform(0.001, 0.1)))]
            elif fn == SIM_E970:
                setf(state + 0x28, pick(0.0, 273.15, 300.0, -40.0, rng.uniform(-400, 400)))
            elif fn == SIM_ECF0:
                setf(state + 0x188, pick(39.2699, 39.27, 39.0, 40.0, rng.uniform(0, 60)))
                setf(opt + 0x58, pick(0.0, 0.1, 0.2, rng.uniform(0, 1)))
                setf(opt + 0x5C, pick(0.0, 0.1, 0.2, rng.uniform(0, 1)))
                for o in (0x8, 0x18, 0x28, 0x38):
                    setf(opt + o, pick(0.5, 0.7, 0.8, 0.9, rng.uniform(0, 1)))
            else:
                # The scratch: def, state, launch mode (+0x4c; ClutchHelp's value with the launch flag off) and drive.
                sets.update({scratch: vdef, scratch + 4: vstate, scratch + 0x4C: pick(0, 0, 1, 2),
                             scratch + 0x58: drive, drive + 8: dstate, dstate + 0x54C: pick(0, 1, 1, 2, rng.randrange(8)),
                             dstate + 0x570: pick(0, 1, 2, 2, rng.randrange(8))})
                setf(dstate + 0x45C, pick(89.0, 89.0118, 99.4838, rng.uniform(80, 110), rng.uniform(0, 300)))
                setf(vdef + 0x178, pick(0.5, 1.0, 2.0, rng.uniform(0, 5)))
                setf(vstate + 0x4C, pick(0.0, 1.0, 0.5, rng.uniform(-0.5, 1.5)))
                words = [F(pick(1 / 720, 0.02, rng.uniform(0.0005, 0.05)))]
            for at, w in sets.items():
                struct.pack_into("<I", image, at - lo, w)
            mu.mem_write(lo, bytes(image))
            walk.clear()
            call(fn, scratch if fn == LAUNCH_CLUTCH else this, words)
            after = mu.mem_read(lo, hi - lo)
            diffs = [(lo + k, struct.unpack_from("<I", after, k)[0]) for k in range(0, hi - lo, 4)
                     if after[k:k + 4] != image[k:k + 4]]
            f.write("c %x %x %s | %s | %s | %x %016x | %s\n" % (
                fn, seed, " ".join("%x" % w for w in words), " ".join("%x=%x" % kv for kv in sets.items()),
                " ".join("%x=%x" % kv for kv in diffs), mu.reg_read(UC_X86_REG_EAX),
                D(st0(mu)) if fn == SIM_E970 else 0, " ".join("%x" % w for w in walk)))


if __name__ == "__main__":
    main(*sys.argv[1:])
