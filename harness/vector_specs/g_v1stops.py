"""Direct-call records for the stops the v1 drives expose (lead/reports/g-v1stops.md): E43Vf128's ring tail, the ring
append, and the mounted body's velocity term E46_Offset540. The other branches are reached by the v1 snapshots."""

import struct

SET_NAME = "g-v1stops"

RING_BYTES = 0x710  # the head (3 words), 64 vectors from +0x10, 64 float triples from +0x410
DEBUG_FLAGS = 0x160E0  # the CDef's debug flags; bit 0x1000000 turns on E43Vf128's ring tail
RING_TAIL_FLAG = 0x1000000


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32bits(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def floats(rng, n, span):
    return struct.pack("<%df" % n, *[rng.uniform(-span, span) for _ in range(n)])


def ring_writes(rng, ring):
    """A ring with random contents and a head that wraps or not; a ring that does not wrap is full now and then."""
    wraps = rng.random() < 0.5
    nxt = rng.randrange(0x40) if wraps or rng.random() < 0.8 else rng.choice((0x40, 0x41))
    head = struct.pack("<III", rng.choice((1, 0x100)) if wraps else 0, nxt, rng.randrange(0x42))
    return [(ring, bytes(rng.getrandbits(8) for _ in range(RING_BYTES))), (ring, head)]


def fuzz_ring(mu, heap, rng):
    ring, vec = heap + 0x1000, heap + 0x800
    writes = ring_writes(rng, ring) + [(vec, floats(rng, 4, 100.0))]
    return [vec] + [f32bits(rng.uniform(-10.0, 10.0)) for _ in range(3)], writes, ring


def fuzz_offset(mu, heap, rng):
    body, point, out = heap + 0x1000, heap + 0x800, heap + 0x810
    writes = [(body, bytes(0x270))]
    for at, span in ((0x140, 100.0), (0x190, 5.0), (0x250, 20.0), (0x260, 5.0)):
        writes.append((body + at, floats(rng, 4, span)))
    writes += [(point, floats(rng, 4, 100.0)), (out, floats(rng, 4, 10.0))]
    return [point, out], writes, body


def fuzz_vf128_ring(mu, heap, rng):
    """CSolid::E43Vf128 with a ring tail (rigid_body.py's fuzz_vf128 plus the ring at heap+0x3000): no hook, the CDef's
    ring flag set on most calls."""
    body, definition, ring = heap + 0x1000, heap + 0x2000, heap + 0x3000
    a, b = heap + 0x800, heap + 0x810
    cdef = word(mu, word(mu, 0x1659B70) + 0x24)
    flags = word(mu, cdef + DEBUG_FLAGS)
    flags = flags | RING_TAIL_FLAG if rng.random() < 0.8 else flags & ~RING_TAIL_FLAG
    vtable = bytes(mu.mem_read(0xBF1D7B8, 4))
    writes = [(body, vtable), (body + 0x34, struct.pack("<I", definition)),
              (body + 0x3CC, struct.pack("<I", 0)), (body + 0x3C8, struct.pack("<I", ring)),
              (definition + 0x16C, struct.pack("<f", rng.uniform(0.001, 0.1))),
              (cdef + DEBUG_FLAGS, struct.pack("<I", flags))]
    writes += ring_writes(rng, ring)
    for k in range(3):
        writes.append((body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((body + 0x190 + 4 * k, struct.pack("<f", rng.uniform(-3.0, 3.0))))
        writes.append((body + 0x1C0 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
        writes.append((definition + 0x130 + 4 * k, struct.pack("<f", rng.uniform(0.0, 0.05))))
        writes.append((a + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((b + 4 * k, struct.pack("<f", rng.uniform(-1000.0, 1000.0))))
    return [a, b, heap + 0x820, heap + 0x830, heap + 0x840, heap + 0x850, heap + 0x860, heap + 0x870], writes, body


def vf128_nodes(mu, a):
    """The ring and the flag chain the tail reads (the global, the CDef pointer, the debug flags)."""
    app = word(mu, 0x1659B70)
    cdef = word(mu, app + 0x24)
    return [(word(mu, a[0] + 0x3C8), RING_BYTES), (0x1659B70, 4), (app + 0x24, 4), (cdef + DEBUG_FLAGS, 4)]


SPECS = {
    0x529100: dict(name="E46_RingAppend", popped=0x10, x87=False, thiscall=True, direct_fuzz=fuzz_ring,
                   windows=[(0, lambda a: RING_BYTES), (1, lambda a: 0x10)]),
    0x529540: dict(name="E46_Offset540", popped=8, x87=False, thiscall=True, direct_fuzz=fuzz_offset,
                   windows=[(0, lambda a: 0x270), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x52A0E0: dict(name="CSolid::E43Vf128", popped=0x20, x87=False, thiscall=True, direct_fuzz=fuzz_vf128_ring,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10), (2, lambda a: 0x10),
                            (0, lambda a: 0x1F0, 0x34)],
                   nodes=vf128_nodes),
    0x519C00: dict(name="CSolid::S30", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x40), (3, lambda a: 0x10)]),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
}
