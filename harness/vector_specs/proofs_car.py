"""Direct calls for the car-body and entity bindings no drive reaches (gap/car-proofs).

The emits CSigEngineFire (0x4ef290), CSigCarDamageSpeech (0x4ae0d0), CSigRespawn (0x4e8830), CSigRetireCar (0x4e8770)
and CSigOutsideAssistance (0x4e89b0), the CMultiSignal1 template of 0x4e8b30: g_retire's heap signal object, plus a null source (the emit returns at once) and a busy emitter (the
recursive-emission log) in a tenth of the calls each.

Dmg720 (0x4ec720) and DmgD160 (0x4ed160): car_body_functors' handler with the gates open (level 2 or 3, a > b, the
state below 1) and an increment that leaves 0..1 on either side, for the two clamps the functor set's cases miss.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-car-proofs --addr 0x4EF290 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipes pack words)

from unicorn import UC_HOOK_CODE  # noqa: E402
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_ECX,  # noqa: E402
                               UC_X86_REG_EDI, UC_X86_REG_EDX, UC_X86_REG_ESI, UC_X86_REG_ESP)

SET_NAME = "g-car-proofs"

RET4 = 0x573616  # a `retl $4`: the listener's own return (g_emit's)
NODE_SLOTS = 0x80


def sig_direct(mu, heap, rng):
    """The emit on a heap signal object: a connection table (its index below 0x20), 0..2 nodes per list, the payload (4
    bytes) the receivers get and their vtable (slot 1 is RET4). A null source or a set busy flag now and then."""
    del mu
    signal = heap + 0x500000
    table, vtable, nodes = signal + 0x1000, signal + 0x1100, signal + 0x1200
    image = bytearray(0x220)
    writes = []
    shape = rng.random()
    struct.pack_into("<I", image, 4, 0 if shape < 0.1 else table)
    struct.pack_into("<I", image, 0, 1 if 0.1 <= shape < 0.2 else 0)
    writes.append((table, struct.pack("<I", rng.randrange(0x20))))
    free = iter(range(NODE_SLOTS))

    def chain(length):
        head = 0
        for _ in range(length):
            at = nodes + 8 * next(free)
            writes.append((at, struct.pack("<2I", vtable, head)))
            head = at
        return head

    for k in range(0x20):
        struct.pack_into("<I", image, 8 + 16 * k, chain(rng.choice((0, 0, 1, 2))))
    struct.pack_into("<I", image, 0x208, chain(rng.choice((0, 1, 2))))
    writes.append((signal, bytes(image)))
    writes.append((vtable, struct.pack("<4I", 0, RET4, 0, 0)))
    writes.append((heap + 0xD2000, bytes(rng.getrandbits(8) for _ in range(4))))
    return [heap + 0xD2000], writes, signal


def sig_nodes(mu, a):
    base = a[0]
    return [(base + 0x1000, 8), (base + 0x1100, 0x10), (base + 0x1200, 8 * NODE_SLOTS)]


def emit_spec(name):
    return dict(name=name, popped=4, x87=False, thiscall=True, direct_fuzz=sig_direct, nodes=sig_nodes,
                dispatch=lambda mu, a: [(RET4, 4)], windows=[(0, lambda a: 0x220), (1, lambda a: 4)])


FUNCTORS = SETS["car_body_functors"]  # noqa: F821 (vcu_vectors' sets)


def clamp_recipe(state_at):
    """Dmg720 / DmgD160 with the gates open: the state (`state_at`) in -1..1 and `(a - b) * c` in -4..4, so the sum
    falls below 0, inside, and above 1."""
    def recipe(mu, heap, r):
        del mu
        this = heap + 0x20000
        handler = bytearray(handler_bytes(r))
        struct.pack_into("<I", handler, 0x3DF0, r.choice((2, 3)))
        struct.pack_into("<f", handler, state_at, r.uniform(-1.0, 0.999))
        b = r.uniform(-0.5, 1.5)
        words = [fbits(b + r.uniform(0.01, 2.0)), fbits(b), fbits(r.uniform(-4.0, 4.0)), r.getrandbits(32)]
        return words, [(this, bytes(handler))], this

    return recipe


def fbits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def handler_bytes(r):
    return FUNCTORS[0x4EC720]["direct_fuzz"].__globals__["handler_bytes"](r)


RETIRE = SETS["g-retire"]  # noqa: F821
CAR_GLOBAL = 0x008EF660


REGS = (UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESI, UC_X86_REG_EDI,
        UC_X86_REG_EBP, UC_X86_REG_ESP)
LANE_POINT_AT = 0x4E7969  # Test7910 after BlendB: the lane point at [esp+0x30]


def lane_point(mu, car):
    """The lane point Test7910 measures from (its own start, stopped once BlendB has answered; registers back after)."""
    saved = [mu.reg_read(r) for r in REGS]
    ret = struct.unpack("<I", bytes(mu.mem_read(saved[-1], 4)))[0]
    point = []

    def stop(mu, addr, size, _):
        esp = mu.reg_read(UC_X86_REG_ESP)
        point.extend(struct.unpack("<3f", bytes(mu.mem_read(esp + 0x30, 12))))
        mu.emu_stop()

    hook = mu.hook_add(UC_HOOK_CODE, stop, begin=LANE_POINT_AT, end=LANE_POINT_AT)
    mu.reg_write(UC_X86_REG_ECX, car)
    # Its first instruction (push ebp) by hand, so the recorder's entry hook does not see this run.
    mu.mem_write(saved[-1] - 4, struct.pack("<I", saved[-2]))
    mu.reg_write(UC_X86_REG_ESP, saved[-1] - 4)
    mu.emu_start(0x4E7911, ret)
    mu.hook_del(hook)
    for r, value in zip(REGS, saved):
        mu.reg_write(r, value)
    return point


def near_lane_writes(mu, rng):
    """The car around its lane point: the lane (+0x17c) the car's own, the position (+0x140) the lane point moved by
    up to 4 on each axis, so the distance falls on both sides of 2 and the offset points either way."""
    car = struct.unpack("<I", bytes(mu.mem_read(CAR_GLOBAL, 4)))[0]
    point = lane_point(mu, car)
    scale = rng.choice((0.5, 1.5, 4.0))
    return car, [(car + 0x140, struct.pack("<3f", *(p + rng.uniform(-scale, scale) for p in point)))]


def near_lane_direct(mu, heap, rng):
    del heap
    car, writes = near_lane_writes(mu, rng)
    return [], writes, car


def speed_direct(mu, heap, rng):
    """Get15F8 on the snapshot's car, its +0x15f8 a random float (zero, signed, huge, NaN, infinite or raw bits)."""
    del heap
    car = struct.unpack("<I", bytes(mu.mem_read(CAR_GLOBAL, 4)))[0]
    value = rng.choice((struct.pack("<f", rng.uniform(-60, 60)), struct.pack("<f", 0.0), struct.pack("<f", -0.0),
                        struct.pack("<f", float("nan")), struct.pack("<f", float("inf")),
                        struct.pack("<I", rng.getrandbits(32))))
    return [], [(car + 0x15F8, value)], car


def ctor_direct(mu, heap, rng):
    """E36DamageOps::Ctor on a heap block of random words (a third zero), so the table scans of InitTables (0x4ed370) see
    free and taken slots in the bytes the constructor does not write."""
    del mu
    this = heap + 0x20000
    words = [rng.choice((0, rng.getrandbits(32), rng.randrange(1, 16))) for _ in range(0x3EB0 // 4)]
    return [], [(this, struct.pack("<%dI" % len(words), *words))], this


SPECS = {
    0x4EF410: dict(name="E36DamageOps::Ctor", popped=0, x87=False, thiscall=True, direct_fuzz=ctor_direct,
                   windows=[(0, lambda a: 0x3EB0)]),
    0x4E6770: dict(name="CCar::Get15F8", popped=0, x87=True, thiscall=True, direct_fuzz=speed_direct, windows=[],
                   nodes=lambda mu, a: [(a[0] + 0x15F8, 4)]),
    0x4E7910: dict(RETIRE[0x4E7910], direct_fuzz=near_lane_direct),
    0x4E68E0: RETIRE[0x4E68E0],
    0x4EC720: dict(FUNCTORS[0x4EC720], direct_fuzz=clamp_recipe(0x3B10)),
    0x4ED160: dict(FUNCTORS[0x4ED160], direct_fuzz=clamp_recipe(0x3B0C)),
    0x4EF290: emit_spec("CMultiSignal1::Emit CSigEngineFire (4ef290)"),
    0x4AE0D0: emit_spec("CMultiSignal1::Emit CSigCarDamageSpeech (4ae0d0)"),
    0x4E8830: emit_spec("CMultiSignal1::Emit CSigRespawn (4e8830)"),
    0x4E8770: emit_spec("CMultiSignal1::Emit CSigRetireCar (4e8770)"),
    0x4E89B0: emit_spec("CMultiSignal1::Emit CSigOutsideAssistance (4e89b0)"),
    0x43C8E0: dict(name="E01Log", popped=16, stack_words=4, x87=False, thiscall=False, cdecl=True, external=True,
                   windows=[]),
}
