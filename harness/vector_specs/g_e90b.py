"""Direct-call records for CCar::Collide6000 and the E90 segment's add helpers (AddParticle, AddSpring, AddFixed,
AddPin, AddBend). The recipes build the car's body and the segment's pools in the replay's stub heap."""

import struct

SET_NAME = "g-e90b"


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32bytes(x):
    return struct.pack("<f", x)


def u32bytes(x):
    return struct.pack("<I", x & 0xFFFFFFFF)


def fuzz_collide6000(mu, heap, rng):
    body = rng.choice((0xBF24C10, 0xBF25278, 0xBF258E0, 0xBF25F48))
    car = word(mu, body + 0x3D0)
    a, b, c = heap + 0x800, heap + 0x810, heap + 0x814
    # One call in four probes a vector below the 0.001 gate (the cleared path).
    scale = 0.0005 if rng.random() < 0.25 else 10.0
    writes = [(a + 4 * k, f32bytes(rng.uniform(-scale, scale))) for k in range(4)]
    writes += [(b, u32bytes(0)), (c, u32bytes(0))]
    return [a, b, c], writes, car


def particle(rng, at):
    lanes = [(at + 4 * k, f32bytes(rng.uniform(-2.0, 2.0))) for k in range(3)]
    return lanes + [(at + 0x24, f32bytes(rng.uniform(0.1, 2.0)))]


def pool_writes(seg, count_at, data_at, data, n):
    return [(seg + count_at, u32bytes(n)), (seg + data_at, u32bytes(data))]


def seg_block(heap, rng):
    seg = heap + 0x1000
    return seg, [(seg, bytes(0x78))]


def fuzz_add_particle(mu, heap, rng):
    seg, writes = seg_block(heap, rng)
    n = rng.randrange(0, 4)
    writes += pool_writes(seg, 0, 0xC, heap + 0x2000, n)
    pos = heap + 0x800
    writes += [(pos + 4 * k, f32bytes(rng.uniform(-5.0, 5.0))) for k in range(3)]
    return [pos, f32bits(rng.uniform(0.1, 2.0))], writes, seg


def f32bits(x):
    return struct.unpack("<I", f32bytes(x))[0]


def fuzz_add_spring(mu, heap, rng):
    seg, writes = seg_block(heap, rng)
    n = rng.randrange(0, 4)
    springs = heap + 0x4000
    ptrs = heap + 0x3000
    writes += pool_writes(seg, 0x10, 0x1C, ptrs, n)
    writes += [(ptrs + 4 * k, u32bytes(springs + 0x20 * k)) for k in range(4)]
    a, b = heap + 0x5000, heap + 0x5040
    writes += particle(rng, a) + particle(rng, b)
    if rng.random() < 0.125:
        # Both inverse masses zero: the sum is zero, so no spring and no count change.
        writes += [(a + 0x24, f32bytes(0.0)), (b + 0x24, f32bytes(0.0))]
    return [a, b, rng.randrange(0, 3), f32bits(rng.uniform(0.0, 4.0))], writes, seg


def fuzz_add_fixed(mu, heap, rng):
    seg, writes = seg_block(heap, rng)
    writes += pool_writes(seg, 0x20, 0x2C, heap + 0x3100, rng.randrange(0, 4))
    return [heap + 0x4000], writes, seg


def fuzz_add_pin(mu, heap, rng):
    seg, writes = seg_block(heap, rng)
    writes += pool_writes(seg, 0x40, 0x4C, heap + 0x3200, rng.randrange(0, 4))
    return [heap + 0x5000], writes, seg


def fuzz_add_bend(mu, heap, rng):
    seg, writes = seg_block(heap, rng)
    writes += pool_writes(seg, 0x30, 0x3C, heap + 0x3300, rng.randrange(0, 4))
    spring, a, b, p = heap + 0x4000, heap + 0x5000, heap + 0x5040, heap + 0x5080
    writes += [(spring, u32bytes(a)), (spring + 4, u32bytes(b))]
    writes += particle(rng, a) + particle(rng, b) + particle(rng, p)
    return [spring, p], writes, seg


def spring_pointers(mu, a):
    seg = a[0]
    ptrs = word(mu, seg + 0x1C)
    return [(ptrs, 0x10)] if ptrs else []


def bend_ends(mu, a):
    spring = a[1]
    return [(word(mu, spring + 4 * k), 0x28) for k in (0, 1)] if spring else []


def spring_after(mu, a, eax):
    return [(eax, 0x20)] if eax else []


def particle_after(mu, a, eax):
    return [(eax, 0x28)]


SPECS = {
    # Nested in CCar::Collide6000: its grid is the first argument (the grid getter's answer is not recorded).
    0x4F8280: dict(name="E37GridOps::EdgeEdge", popped=0x18, x87=False, thiscall=True, external=True,
                   branch=no_branch,
                   windows=[(0, lambda a: 0x6B0), (1, lambda a: 0x10), (2, lambda a: 0x10),
                            (3, lambda a: 0x10), (4, lambda a: 4), (5, lambda a: 0x10), (6, lambda a: 4)]),
    0x4E6000: dict(name="CCar::Collide6000", popped=12, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_collide6000,
                   windows=[(0, lambda a: 0x500), (1, lambda a: 0x1C), (2, lambda a: 4), (3, lambda a: 4)]),
    0x605F30: dict(name="E90Seg::AddParticle", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_add_particle, windows=[(0, lambda a: 0x78), (1, lambda a: 0xC)],
                   after=particle_after),
    0x605FB0: dict(name="E90Seg::AddSpring", popped=0x10, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_add_spring,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0x28), (2, lambda a: 0x28)],
                   nodes=spring_pointers, after=spring_after),
    0x606070: dict(name="E90Seg::AddFixed", popped=4, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_add_fixed, windows=[(0, lambda a: 0x78)]),
    0x606090: dict(name="E90Seg::AddPin", popped=4, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_add_pin, windows=[(0, lambda a: 0x78)]),
    0x6060B0: dict(name="E90Seg::AddBend", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_add_bend,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0x20), (2, lambda a: 0x28)], nodes=bend_ends),
}
