"""Records for the pair worker CDrive::M96C0 (0x5096c0, the wheel worker of a drive of DriveType 3) and its callees
(lead/reports/g-drive96.md): the step's own calls on the tarmac-zr snapshots, and direct calls with fuzzed drives
for the branches the step does not reach. All seven run in the original (no external calls)."""

import struct

SET_NAME = "g-drive96"

NAN = float("nan")


def u32(b, at):
    return struct.unpack_from("<I", b, at)[0]


def i32(b, at):
    return struct.unpack_from("<i", b, at)[0]


def f32(b, at):
    return struct.unpack_from("<f", b, at)[0]


def pf(at, value):
    return (at, struct.pack("<f", value))


def pi(at, value):
    return (at, struct.pack("<i", value))


def some(r, lo, hi, nan=0.05):
    """A float in [lo, hi), now and then NaN or zero."""
    x = r.random()
    return NAN if x < nan else 0.0 if x < nan + 0.05 else r.uniform(lo, hi)


def pair_objects(heap, r):
    """A CDrive at heap+0x3000 with its definition (this+4), state (this+8) and worker object (this+0x170): random
    floats in the drive's words, the state and the definition, then the indices, gates, signs and flags the pair
    worker reads, in their ranges. Returns (this, def, state, obj, writes)."""
    this, def_at, state_at, obj = heap + 0x3000, heap + 0x4000, heap + 0x8000, heap + 0xB000
    writes = [(this + 0x80, struct.pack("<92f", *[r.uniform(-3.0, 3.0) for _ in range(92)])),
              (def_at, struct.pack("<664f", *[r.uniform(-2.0, 2.0) for _ in range(664)])),
              (state_at, struct.pack("<368f", *[r.uniform(-2.0, 2.0) for _ in range(368)])),
              (this + 4, struct.pack("<I", def_at)), (this + 8, struct.pack("<I", state_at)),
              (this + 0x170, struct.pack("<I", obj))]
    for k in range(4):
        a = def_at + 0x60 * k
        writes += [pi(a + 0x18, r.randrange(-3, 4)), pi(a + 0x28, r.randrange(4)), pi(a + 0x38, r.randrange(4)),
                   pf(a + 0x48, some(r, 0.3, 3.0)), pi(a + 0x68, r.randrange(4))]
        w = def_at + 0xF0 * k
        writes += [pf(w + 0x148, some(r, 0.1, 2.0)), pi(w + 0x158, r.randrange(4))]
        b = state_at + 200 * k
        writes += [pi(b + 0xA4, r.choice((-1, 0, 0, 1))), pi(b + 0x74, r.choice((-1, 0, 0, 1))),
                   pf(b + 0x84, some(r, 0.05, 5.0)), pf(b + 0xF8, some(r, 0.0, 2.0)), pf(b + 0xB4, some(r, 0.1, 3.0)),
                   pf(b + 0x94, r.choice((0.0, 0.0, NAN, r.uniform(-2.0, 2.0)))),
                   pi(state_at + 0x14 + 0x14 * k, r.choice((-1, 0, 0, 1))),
                   pi(state_at + 0x4F0 + 4 * k, r.choice((0, 1))),
                   pi(obj + 0x18 + 0xC * k, r.choice((0, 1)))]
        writes.append(pf(this + 0xF4 + 0x10 * k, some(r, -3.0, 3.0)))
    for k in range(2):
        writes += [pi(state_at + 0x520 + 4 * k, r.choice((0, 1))),
                   pf(this + 0xD0 + 4 * k, r.choice((0.0, 0.0, r.uniform(-1.0, 1.0))))]
    for at in (0x5A4, 0x5A8, 0x5AC, 0x5B0):
        writes.append(pi(state_at + at, r.randrange(100)))
    writes += [pi(state_at + 0x5BC, r.choice((0, 0, 1))), pf(state_at + 0x5A0, r.uniform(0.0, 300.0))]
    return this, def_at, state_at, obj, writes


def fuzz_leaf(mu, heap, r):
    """M5B00, M5F10 and M60D0 (this, x): a random drive and x in 0..3."""
    this, _, _, _, writes = pair_objects(heap, r)
    return [r.randrange(4)], writes, this


def fuzz_plain(mu, heap, r):
    """M6540 and M7AA0 (this): a random drive."""
    this, _, _, _, writes = pair_objects(heap, r)
    return [], writes, this


def dt_word(r):
    return struct.unpack("<I", struct.pack("<f", r.choice((NAN, 0.0, r.uniform(0.001, 0.05), r.uniform(0.001, 0.05)))))[0]


def fuzz_step3870(mu, heap, r):
    """E39_Step3870 (this, i, dt): a random drive, the wheel i in 0..3 and dt (NaN and zero too)."""
    this, _, _, _, writes = pair_objects(heap, r)
    return [r.randrange(4), dt_word(r)], writes, this


def fuzz_m96c0(mu, heap, r):
    """M96C0 (this, dt): a random drive. The solve loop repeats while term 1's active flag is 1 and its MatVec output
    (this+0xa0, which nothing in the loop writes) is beyond term 1's scale, so in the original that state never ends:
    the output is drawn inside the scale when the flag is 1."""
    this, def_at, state_at, _, writes = pair_objects(heap, r)
    gi = r.randrange(4)
    scale = r.uniform(0.5, 2.0)
    writes += [pi(def_at + 0x60 + 0x68, gi), pf(state_at + 0x2C, scale),
               pf(this + 0xA0 + 4 * gi, r.uniform(-0.9, 0.9) * scale)]
    if r.random() < 0.3:  # the ungated branch
        writes += [pi(state_at + 0xA4, 0), pi(state_at + 0xA4 + 200, 0)]
    return [dt_word(r)], writes, this


def gated(windows, x):
    return i32(windows[1][3], 0xA4 + 200 * x) != 0


def m96c0_branch(args, windows):
    """'zero' (both blocks ungated) or 'pair', with '-reset' when the pending word is set."""
    s = windows[1][3]
    kind = "pair" if gated(windows, 0) or gated(windows, 1) else "zero"
    return kind + ("-reset" if u32(s, 0x5BC) else "")


def step3870_branch(args, windows):
    """'latched', or with the latch clear 'free-set' (the offset beyond the bound) and 'free-none'."""
    i, s, t = args[1], windows[1][3], windows[0][3]
    if i32(s, 0x74 + 200 * i) != 0:
        return "latched"
    v, bound = f32(t, 0xF4 + 0x10 * i), f32(s, 0xF8 + 200 * i)
    return "free-set" if (v if v > 0 else 0.0 - v) > bound else "free-none"


def leaf_branch(args, windows):
    """'gated', or the wheel's enable word ('enabled', 'disabled')."""
    x = args[1]
    if gated(windows, x):
        return "gated"
    return "enabled" if u32(windows[3][3], 0x18 + 0xC * x) else "disabled"


def m5b00_branch(args, windows):
    """The gate and the pair's second flag."""
    return ("gated" if gated(windows, args[1]) else "free") + ("-active" if u32(windows[1][3], 0x524) else "-idle")


def m6540_branch(args, windows):
    """'zero' (the forces' sum is zero: the gates give the sign) or 'sign'."""
    s, d = windows[1][3], windows[2][3]
    a, b = u32(d, 0x88), u32(d, 0x98)
    total = f32(s, 0x94 + 200 * b) + f32(s, 0x94 + 200 * a)
    return "zero" if total == 0.0 else "sign"


def m7aa0_branch(args, windows):
    """'active' when the pair's second flag comes out set, else 'idle'."""
    s, d = windows[1][3], windows[2][3]
    a, b = u32(d, 0x88), u32(d, 0x98)
    on = i32(s, 0x28) == 0 and (gated(windows, a) or gated(windows, b))
    return "active" if on else "idle"


# Windows: this (to +0x1d0), the state (this+8), the definition (this+4) and the worker object (this+0x170).
WINDOWS = [(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8), (0, lambda a: 0xA60, 4), (0, lambda a: 0x60, 0x170)]

SPECS = {
    0x5096C0: dict(name="CDrive::M96C0", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz_m96c0,
                   branch=m96c0_branch, windows=WINDOWS),
    0x503870: dict(name="E39_Step3870", popped=8, x87=False, thiscall=True, direct_fuzz=fuzz_step3870,
                   branch=step3870_branch, windows=WINDOWS),
    0x505F10: dict(name="CDrive::M5F10", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz_leaf,
                   branch=leaf_branch, windows=WINDOWS),
    0x5060D0: dict(name="CDrive::M60D0", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz_leaf,
                   branch=leaf_branch, windows=WINDOWS),
    0x506540: dict(name="CDrive::M6540", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_plain,
                   branch=m6540_branch, windows=WINDOWS),
    0x507AA0: dict(name="CDrive::M7AA0", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_plain,
                   branch=m7aa0_branch, windows=WINDOWS),
    0x505B00: dict(name="CDrive::M5B00", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz_leaf,
                   branch=m5b00_branch, windows=WINDOWS),
}
