"""Direct-call records for the animated replay's car state (G-snap): CCar::Snap9360 (0x4e9360) on the snapshot's car
(position, quaternion, steering and wheel words drawn, some wheels absent), with Xform8130 and the state emit as its
inner calls, and the emit itself (CMultiSignal1<CSigCarStateAnimatedReplay>::Emit, 0x4e85b0) on a heap signal object,
idle or already emitting (the busy emitter logs through E01Log and returns).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-snap --count N --seed SEED
"""

import struct

SET_NAME = "g-snap"

CAR_GLOBAL = 0x008EF660
RET4 = 0x573616  # a `retl $4`: the receivers' own return (g_retire's)
NODE_SLOTS = 0x80
PAYLOAD = 0x54  # the state's bytes: 16 unwritten, position, quaternion, steering, four wheel pairs


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def fbytes(*values):
    return struct.pack("<%df" % len(values), *values)


def snap_direct(mu, heap, rng):
    car = word(mu, CAR_GLOBAL)
    writes = [(car + 0x140, fbytes(*(rng.uniform(-500, 500) for _ in range(3)), 1.0)),
              (car + 0x100, fbytes(*(rng.uniform(-1, 1) for _ in range(4)))),
              (car + 0x16A4, fbytes(rng.uniform(-1, 1)))]
    for i in range(4):
        wheel = word(mu, car + 0x470 + 4 * i)
        if rng.random() < 0.25:
            writes.append((car + 0x470 + 4 * i, struct.pack("<I", 0)))
        elif wheel:
            writes += [(wheel + 0x5AC, fbytes(rng.uniform(-1, 1))), (wheel + 0x5BC, fbytes(rng.uniform(-0.2, 0.2)))]
    return [], writes, car


def snap_nodes(mu, a):
    car = a[0]
    wheels = [word(mu, car + 0x470 + 4 * i) for i in range(4)]
    return [(car + 0x100, 0x50), (car + 0x16A4, 4), (car + 0x470, 0x10)] + [(w + 0x5AC, 0x14) for w in wheels if w]


def sig_direct(mu, heap, rng):
    """The emit on a heap signal object: 0..2 receivers per slot list (their slot 1 is RET4), an index below 0x20 (0x20
    and up trap), and the busy flag set in one call of three."""
    signal = heap + 0x500000
    table, vtable, nodes = signal + 0x1000, signal + 0x1100, signal + 0x1200
    image = bytearray(0x220)
    struct.pack_into("<I", image, 0, 1 if rng.random() < 1 / 3 else 0)
    struct.pack_into("<I", image, 4, table)
    writes = [(table, struct.pack("<I", rng.randrange(0x20)))]
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
    payload = heap + 0x2000
    writes.append((payload, bytes(rng.getrandbits(8) for _ in range(PAYLOAD))))
    return [payload], writes, signal


def sig_nodes(mu, a):
    base = a[0]
    return [(base + 0x1000, 8), (base + 0x1100, 0x10), (base + 0x1200, 8 * NODE_SLOTS)]


SPECS = {
    0x4E9360: dict(name="CCar::Snap9360", popped=0, x87=False, thiscall=True, direct_fuzz=snap_direct,
                   windows=[], nodes=snap_nodes),
    0x4E8130: dict(name="CCar::Xform8130", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E85B0: dict(name="CMultiSignal1::Emit CSigCarStateAnimatedReplay (4e85b0)", popped=4, x87=False, thiscall=True,
                   external=True, direct_fuzz=sig_direct, nodes=sig_nodes, dispatch=lambda mu, a: [(RET4, 4)],
                   windows=[(0, lambda a: 0x220), (1, lambda a: PAYLOAD)]),
    0x43C8E0: dict(name="E01Log", popped=16, stack_words=4, x87=False, thiscall=False, cdecl=True, external=True,
                   windows=[]),
}
