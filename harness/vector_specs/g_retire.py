"""The retire and outside-assistance announcers (G-retire): CCar::Test7910 (0x4e7910, the lane test), CCar::Assist8DF0
(0x4e8df0), E34_RetireA (0x4e8fa0) and the two signal emits they make (0x4e8770, the retire signal; 0x4e89b0, the
outside-assistance signal). The car is the snapshot's (its pointer is the global 0x8ef660), so its drive line, signal
objects and flow are the snapshot's own.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-retire --addr 0x4E7910 --count N --seed SEED
    (likewise 0x4E8DF0, 0x4E8FA0, 0x4E8770, 0x4E89B0)
"""

import struct  # noqa: E402 (the recipes pack words)

SET_NAME = "g-retire"

CAR_GLOBAL = 0x008EF660  # the car pointer (the snapshot's)
DRIVE_GLOBAL = 0x01659184
DRIVE_COUNT = 0x75310
FLOW_GLOBAL = 0x0165FA90
SIGNALS = 0x00893600  # the global holding the signal manager
MODE_GLOBAL = 0x007D6128  # the pointer whose +0x8e84 word is the flow mode (as in g_flow)
MODE_AT = 0x8E84
RET4 = 0x573616  # a `retl $4`: the listener's own return (the emits' receivers, from g_emit)

SIG_TABLE, SIG_VTABLE, SIG_NODES, SIG_PAYLOAD = 0xD1000, 0xD1100, 0xD1200, 0xD2000  # heap offsets (g_emit's)
NODE_SLOTS = 0x80


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def fbits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def car_of(mu):
    return word(mu, CAR_GLOBAL)


def lane_writes(mu, car, rng):
    """The car's position and lane (+0x17c, below the drive's count so the table entries it reads exist)."""
    count = word(mu, word(mu, DRIVE_GLOBAL) + DRIVE_COUNT)
    image = [(car + 0x140, struct.pack("<3f", *(rng.uniform(-30, 30) for _ in range(3)))),
             (car + 0x17C, struct.pack("<f", rng.uniform(0, max(count - 1, 1))))]
    return image


def lane_direct(mu, heap, rng):
    del heap
    car = car_of(mu)
    return [], lane_writes(mu, car, rng), car


def lane_nodes(mu, a):
    """The drive table the lane test reads (the count, the entries) and the car's words."""
    drive = word(mu, DRIVE_GLOBAL)
    count = word(mu, drive + DRIVE_COUNT)
    return [(DRIVE_GLOBAL, 4), (drive + DRIVE_COUNT, 4), (drive + 0x10, 0x30 * (count + 2))]


def assist_direct(mu, heap, rng):
    """Assist8DF0(a, b, c): a is 0 or 1; b (the 0x15e4 word) and c (the override timer) take the asm's words or random
    ones; the partial-contact time (+0x9e8) is random around 1.0 so both codes run."""
    del heap
    car = car_of(mu)
    a = rng.choice((0, 1, 1))
    b = rng.choice((fbits(0.5), fbits(30.0), rng.getrandbits(32)))
    c = rng.choice((fbits(4.0), rng.getrandbits(32)))
    writes = lane_writes(mu, car, rng)
    writes.append((car + 0x9E8, struct.pack("<f", rng.uniform(0.0, 2.0))))
    return [a, b, c], writes, car


def retire_direct(mu, heap, rng):
    """E34_RetireA (fastcall on the car): the flow object's state and mode words are random (as g_flow's check)."""
    del heap
    car = car_of(mu)
    flow = word(mu, FLOW_GLOBAL)
    host = word(mu, flow + 0xC98)
    writes = lane_writes(mu, car, rng)
    if host:
        writes.append((host, struct.pack("<H", rng.choice((0x7B, 0x7B, 0)))))
        writes.append((host + 0x17850, struct.pack("<I", rng.choice((3, 3, rng.getrandbits(3))))))
        writes.append((host + 0x17858, struct.pack("<I", rng.randrange(4))))
    return [], writes, car


def retire_nodes(mu, a):
    """The flow check's reads (the flow, its host and their state and mode words), as g_flow's check reads them."""
    flow = word(mu, FLOW_GLOBAL)
    host = word(mu, flow + 0xC98)
    daemon = word(mu, MODE_GLOBAL)
    return [(FLOW_GLOBAL, 4), (flow, 0xC9C), (host, 0x17860), (MODE_GLOBAL, 4), (daemon + MODE_AT, 4)]


def sig_direct(mu, heap, rng):
    """The emit on a heap signal object (g_emit's recipe): a connection table of 0..2 nodes per slot, the payload (4 bytes)
    the emit passes to its receivers, and the receivers' vtable (its slot 1 is RET4)."""
    signal = heap + 0x500000
    table, vtable, nodes = signal + 0x1000, signal + 0x1100, signal + 0x1200
    image = bytearray(0x220)
    writes = []
    count = rng.randrange(0x20)
    struct.pack_into("<I", image, 4, table)
    writes.append((table, struct.pack("<I", count)))
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
    writes.append((heap + SIG_PAYLOAD, bytes(rng.getrandbits(8) for _ in range(4))))
    return [heap + SIG_PAYLOAD], writes, signal


def sig_nodes(mu, a):
    """The connection table, the vtable and the node region of the emit's signal object."""
    base = a[0]
    return [(base + 0x1000, 8), (base + 0x1100, 0x10), (base + 0x1200, 8 * NODE_SLOTS)]


def sig_dispatch(mu, a):
    return [(RET4, 4)]


def emit_spec(name):
    return dict(name=name, popped=4, x87=False, thiscall=True, external=True, direct_fuzz=sig_direct, nodes=sig_nodes,
                dispatch=sig_dispatch, windows=[(0, lambda a: 0x220), (1, lambda a: 4)])


def ext(name, popped, windows=()):
    return dict(name=name, popped=popped, x87=False, thiscall=True, external=True, windows=list(windows))


SPECS = {
    # CCar::Test7910 (thiscall, no args, retl): the car's position less BlendB's point at its lane, the verdict of
    # ToWorld's answer (kept as its inner record). External: it is an inner record of Assist8DF0 too.
    0x4E7910: dict(name="CCar::Test7910", popped=0, x87=False, thiscall=True, external=True, direct_fuzz=lane_direct,
                   nodes=lane_nodes, windows=[(0, lambda a: 0x180)]),
    # CCar::Assist8DF0(a, b, c) (ret 12): the lane test, the outside emit, S158 (not kept), UpdateSeed and SetDirty.
    0x4E8DF0: dict(name="CCar::Assist8DF0", popped=12, x87=False, thiscall=True, external=True,
                   direct_fuzz=assist_direct, windows=[(0, lambda a: 0x1700)]),
    # E34_RetireA (fastcall on the car, no args): the retire emit, the flow check, Assist8DF0.
    0x4E8FA0: dict(name="E34_RetireA", popped=0, x87=False, thiscall=True, direct_fuzz=retire_direct,
                   nodes=retire_nodes, windows=[(0, lambda a: 0x180)]),
    0x4E8770: emit_spec("CMultiSignal1::Emit CSigRetireCar (4e8770)"),
    0x4E89B0: emit_spec("CMultiSignal1::Emit CSigOutsideAssistance (4e89b0)"),
    0x4E68E0: ext("CCar::ToWorld", 8, [(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E91F0: ext("CCar::UpdateSeed", 4, [(1, lambda a: 0x20)]),
    0x52F450: ext("NTweak::CBase::SetDirty", 0, []),
}
