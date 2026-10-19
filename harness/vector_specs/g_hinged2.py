"""Direct-call records for the second hinged-body group."""

import random
import struct

SET_NAME = "g-hinged2"


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32bits(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def vtarget(mu, this, slot):
    return word(mu, word(mu, this) + slot)


def fuzz_e44(mu, heap, rng):
    body, point, out = heap + 0x1000, heap + 0x800, heap + 0x810
    vtable = bytes(mu.mem_read(0xBF1D7B8, 4))
    writes = [(body, vtable), (body + 0x44, struct.pack("<I", 0))]
    for k in range(3):
        writes.extend([
            (body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-50.0, 50.0))),
            (body + 0x190 + 4 * k, struct.pack("<f", rng.uniform(-3.0, 3.0))),
            (body + 0x1C0 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))),
            (point + 4 * k, struct.pack("<f", rng.uniform(-50.0, 50.0))),
            (out + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))),
        ])
    return [point, out], writes, body


def fuzz_e4f0(mu, heap, rng):
    body = rng.choice((0xBF24C10, 0xBF25278, 0xBF258E0, 0xBF25F48))
    a, b, c = heap + 0x800, heap + 0x810, heap + 0x820
    writes = []
    for base in (a, b, c):
        for k in range(4):
            writes.append((base + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
    return [a, b, c, a, b, c], writes, body


def e4f0_dispatch(mu, a):
    body = a[0]
    car = word(mu, body + 0x44)
    return [
        (vtarget(mu, car, 0x30), 0, dict(name="slot 12", popped=0, stack_words=2, x87=False,
                                         thiscall=True, external=True, windows=[])),
        (vtarget(mu, body, 0x30), 0, dict(name="slot 12", popped=0, stack_words=2, x87=False,
                                          thiscall=True, external=True, windows=[])),
        (0x51B4C0, 12),
        (0x51B560, 12),
        (vtarget(mu, car, 0x138), 0x18, dict(
            name="car slot 0x138",
            popped=0x18,
            x87=False,
            thiscall=True,
            external=True,
            windows=[(i, lambda a: 0x10) for i in range(1, 7)],
        )),
    ]


def fuzz_f350(mu, heap, rng):
    body = rng.choice((0xBF24C10, 0xBF25278, 0xBF258E0, 0xBF25F48))
    point, impulse = heap + 0x800, heap + 0x810
    writes = []
    scale = rng.choice((0.05, 3500.0, 5000.0))
    for k in range(3):
        writes.extend([
            (point + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))),
            (impulse + 4 * k, struct.pack("<f", rng.uniform(-scale, scale))),
        ])
    args = [point, impulse, 0, 0, point, impulse, 0, impulse]
    return args, writes, body


def fuzz_clear480(mu, heap, rng):
    body = rng.choice((0xBF24C10, 0xBF25278, 0xBF258E0, 0xBF25F48))
    car = word(mu, body + 0x3D0)
    slot = car + 0x480 + 4 * rng.randrange(4)
    return [body], [(slot, struct.pack("<I", body))], car


def fuzz_vf128x(mu, heap, rng):
    body = rng.choice((0xBF24C10, 0xBF25278, 0xBF258E0, 0xBF25F48))
    car = word(mu, body + 0x3D0)
    a, b = heap + 0x800, heap + 0x810
    writes = []
    for base in (a, b):
        for k in range(4):
            writes.append((base + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
    return [a, b, 0, 0, a, 0, 0, b], writes, car


def fuzz_excl_a060(mu, heap, rng):
    this, array = heap + 0x1000, heap + 0x2000
    value = rng.randrange(1, 6)
    items = [1, value, 3, value, 5]
    writes = [(this + 0x2C, struct.pack("<I", array)),
              (array, struct.pack("<I", len(items)))]
    writes.extend((array + 4 * (i + 1), struct.pack("<I", item)) for i, item in enumerate(items))
    return [value], writes, this


def fuzz_e90_seg_pool_init(mu, heap, rng):
    pool, block = heap + 0x800, heap + 0x1000
    return [rng.randrange(1, 6), block], [(pool, bytes(0x10)), (block, bytes(0x1000))], pool


def e90_helper_nodes(mu, a):
    # Both builders receive the dummy as an argument and read its owning group and the fence type row.
    dummy = a[2] if len(a) == 4 else a[4]
    group = word(mu, dummy + 0xB0)
    nodes = [(group, 0x50), (0x7D81A0, 0x2C)]
    for off in (0x64, 0x68):
        adjacent = word(mu, dummy + off)
        if adjacent:
            nodes.append((adjacent, 0xB4))
    return nodes


def e90_init_nodes(mu, a):
    dummy = a[1]
    return [(at, 0xB4) for at in (word(mu, dummy + 0x64), word(mu, dummy + 0x68)) if at]


def e90_particle_node(mu, a):
    seg = a[0]
    return [(word(mu, seg + 0x0C) + word(mu, seg) * 0x28, 0x28)]


def e90_spring_nodes(mu, a):
    seg = a[0]
    ptrs = word(mu, seg + 0x1C)
    block = word(mu, ptrs + word(mu, seg + 0x10) * 4)
    return [(block, 0x20)]


def e90_fixed_node(mu, a):
    seg = a[0]
    return [(word(mu, seg + 0x2C) + word(mu, seg + 0x20) * 4, 4)]


def e90_pin_node(mu, a):
    seg = a[0]
    return [(word(mu, seg + 0x4C) + word(mu, seg + 0x40) * 4, 4)]


def e90_bend_node(mu, a):
    seg = a[0]
    return [(word(mu, seg + 0x3C) + word(mu, seg + 0x30) * 0x0C, 0x0C)]


def fuzz_e90_post(mu, heap, rng):
    post, dummy, group, block = heap + 0x1000, heap + 0x2000, heap + 0x3000, heap + 0x4000
    typ = 0x7D81A0
    writes = [
        (post, bytes(0x78)), (dummy, bytes(0xB4)), (group, bytes(0x50)), (block, bytes(0x1000)),
        (dummy + 0xB0, struct.pack("<I", group)), (dummy + 0x6C, struct.pack("<fff", 1.0, 2.0, -1.0)),
        (group + 0x30, struct.pack("<I", 0)), (typ + 0x0C, struct.pack("<I", 4)),
        (typ + 0x10, struct.pack("<f", 1.5)), (typ + 0x24, struct.pack("<I", 2)),
        (typ + 0x28, struct.pack("<f", 0.2)),
    ]
    return [post, dummy, block], writes, 0x167EA90


def fuzz_e90_span(mu, heap, rng):
    span, prev, next_seg = heap + 0x1000, heap + 0x1100, heap + 0x1200
    dummy, next_dummy, group, block = heap + 0x2000, heap + 0x2200, heap + 0x3000, heap + 0x4000
    prev_parts, next_parts = heap + 0x6000, heap + 0x6100
    typ = 0x7D81A0
    writes = [
        (span, bytes(0x78)), (prev, bytes(0x78)), (next_seg, bytes(0x78)),
        (dummy, bytes(0xB4)), (next_dummy, bytes(0xB4)), (group, bytes(0x50)),
        (block, bytes(0x3000)), (prev_parts, bytes(4 * 0x28)), (next_parts, bytes(4 * 0x28)),
        (dummy + 0xB0, struct.pack("<I", group)), (dummy + 0x64, struct.pack("<I", next_dummy)),
        (dummy + 0x6C, struct.pack("<fff", 0.0, 2.0, 0.0)),
        (next_dummy + 0x6C, struct.pack("<fff", 4.0, 2.5, 1.0)),
        (prev + 0x0C, struct.pack("<I", prev_parts)), (next_seg + 0x0C, struct.pack("<I", next_parts)),
        (group + 0x2C, struct.pack("<I", 0)), (typ + 0x04, struct.pack("<I", 2)),
        (typ + 0x08, struct.pack("<I", 4)), (typ + 0x0C, struct.pack("<I", 4)),
        (typ + 0x10, struct.pack("<f", 1.5)), (typ + 0x24, struct.pack("<I", 1)),
        (typ + 0x28, struct.pack("<f", 0.2)),
    ]
    for at in (prev_parts, next_parts):
        for i in range(4):
            writes.append((at + i * 0x28 + 0x24, struct.pack("<f", 0.5)))
    return [span, prev, next_seg, dummy, block], writes, 0x167EA90


def f350_nodes(mu, a):
    body = a[0]
    car = word(mu, body + 0x3D0)
    link = word(mu, body + 0x650)
    result = [(car, 0x500), (word(mu, car), 0x200), (link, 8), (0x731D00, 4), (0x73D3BC, 4)]
    excluded = word(mu, body + 0x2C)
    if excluded:
        count = word(mu, excluded)
        result.append((excluded, 4 + min(count, 64) * 4))
    return result


def f350_dispatch(mu, a):
    body = a[0]
    definition = word(mu, body + 0x34)
    if word(mu, definition + 0x50C) == 0:
        return [(vtarget(mu, word(mu, body + 0x3D0), 0x128), 0x20, dict(
            name="car slot 0x128",
            popped=0x20,
            x87=False,
            thiscall=True,
            external=True,
            windows=[(0, lambda a: 0x500), (1, lambda a: 0x10), (2, lambda a: 0x10),
                     (5, lambda a: 0x10), (8, lambda a: 0x10)],
        ))]
    return []


def group_candidates(mu):
    pool = 0x167EA90 + 0x720
    count, data = word(mu, pool), word(mu, pool + 0xC)
    groups = []
    for i in range(min(count, 64)):
        group = data + i * 0x50
        n = word(mu, group)
        if n > 1 and n < 64 and word(mu, group + 0x3C) == 0:
            groups.append(group)
    return groups


def fuzz_e90(mu, heap, rng):
    groups = group_candidates(mu)
    if not groups:
        raise RuntimeError("no unbuilt E90Group in this snapshot")
    return [], [], rng.choice(groups)


def e90_nodes(mu, a):
    group = a[0]
    n, data = word(mu, group), word(mu, group + 0xC)
    typ = 0x7D81A0 + word(mu, group + 0x2C) * 0x2C
    return [(data, n * 0xB4), (typ, 0x2C)]


def e90_size(mu, group):
    n = word(mu, group)
    typ = 0x7D81A0 + word(mu, group + 0x2C) * 0x2C
    cols, rows, layers = word(mu, typ + 8), word(mu, typ + 0xC), word(mu, typ + 0x24)
    spans = (n - 1) & 0xFFFFFFFF
    a = (((cols - 2) & 0xFFFFFFFF) * 0x2C + 0x24) & 0xFFFFFFFF
    b = ((cols - 4) + ((rows - 1) * (cols - 1) * 4)) & 0xFFFFFFFF
    b = (b * 0x24) & 0xFFFFFFFF
    span = ((a * rows + b) * layers + 0x78) & 0xFFFFFFFF
    post = (layers * rows * 0x34 + 0x140) & 0xFFFFFFFF
    return (span * spans + post * n) & 0xFFFFFFFF


def e90_after(mu, a, eax):
    group = a[0]
    size = e90_size(mu, group)
    allocation = word(mu, group + 0x3C)
    return [(allocation, size)] if allocation and size else []


SPECS = {
    0x51E5C0: dict(name="E44_JmpA570", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_e44, windows=[(0, lambda a: 0x3D0), (1, lambda a: 0xC), (2, lambda a: 0xC)],
                   after=lambda mu, a, eax: [(a[2], 0xC)]),
    0x51E4F0: dict(name="CHingedRigidBody::SimE4F0", popped=0x18, x87=False, thiscall=True,
                   direct_fuzz=fuzz_e4f0, windows=[(0, lambda a: 0x660), (1, lambda a: 0x10),
                                                   (2, lambda a: 0x10), (3, lambda a: 0x10)],
                   dispatch=e4f0_dispatch),
    0x51F350: dict(name="CHingedRigidBody::SimF350", popped=0x20, x87=False, thiscall=True,
                   direct_fuzz=fuzz_f350, windows=[(0, lambda a: 0x660), (0, lambda a: 0x510, 0x34),
                                                  (1, lambda a: 0x10), (2, lambda a: 0x10)],
                   nodes=f350_nodes, dispatch=f350_dispatch),
    0x601470: dict(name="E90Group::Build", popped=0, x87=False, thiscall=True,
                   direct_fuzz=fuzz_e90, windows=[(0, lambda a: 0x50)], nodes=e90_nodes,
                   after=e90_after),
    # The helpers remain native. Recording their return values also verifies the operation order inside Build.
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, cdecl=True, external=True, windows=[]),
    0x601270: dict(name="E90SegPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_e90_seg_pool_init, windows=[(0, lambda a: 0x10),
                                                               (2, lambda a: a[1] * 0x78)]),
    0x602FE0: dict(name="E90FenceHandler::BuildPost", popped=0xC, stack_words=3, x87=False,
                   thiscall=True, external=True, direct_fuzz=fuzz_e90_post, nodes=e90_helper_nodes,
                   windows=[(1, lambda a: 0x78), (2, lambda a: 0xB4), (3, lambda a: 0x1000)]),
    0x6029B0: dict(name="E90FenceHandler::BuildSpan", popped=0x14, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_e90_span, nodes=e90_helper_nodes,
                   windows=[(1, lambda a: 0x78), (2, lambda a: 0x78), (3, lambda a: 0x78),
                            (4, lambda a: 0xB4), (5, lambda a: 0x3000)]),
    0x4E58F0: dict(name="CCar::Clear480", popped=4, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_clear480,
                   windows=[(0, lambda a: 0x500), (1, lambda a: 0x660)]),
    0x4E6100: dict(name="CCar::Vf128x", popped=0x20, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_vf128x,
                   windows=[(0, lambda a: 0x500), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x51A060: dict(name="CEntity::E43_ExclA060", popped=4, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_excl_a060,
                   windows=[(0, lambda a: 0x660)],
                   nodes=lambda mu, a: [(word(mu, a[0] + 0x2C), 4 + min(word(mu, word(mu, a[0] + 0x2C)), 64) * 4)]
                   if word(mu, a[0] + 0x2C) else []),
    0x605B60: dict(name="E90Seg::Init", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0xB4)], nodes=e90_init_nodes),
    0x6026A0: dict(name="E90PartPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: a[1] * 0x28)]),
    0x6027A0: dict(name="E90PtrTab::Init", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: a[1] * 0x24)]),
    0x6028B0: dict(name="E90BendPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: a[1] * 0x0C)]),
    0x602930: dict(name="E90PinPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: a[1] * 4)]),
    0x605F30: dict(name="E90Seg::AddParticle", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0x0C)], nodes=e90_particle_node),
    0x606090: dict(name="E90Seg::AddPin", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78)], nodes=e90_pin_node),
    0x605FB0: dict(name="E90Seg::AddSpring", popped=0x10, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0x0C), (2, lambda a: 0x0C)],
                   nodes=e90_spring_nodes),
    0x606070: dict(name="E90Seg::AddFixed", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78)], nodes=e90_fixed_node),
    0x6060B0: dict(name="E90Seg::AddBend", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78)], nodes=e90_bend_node),
    0x51FD00: dict(name="CHingedRigidBody::SimFD00", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x660)]),
    0x4E6000: dict(name="CCar::Collide6000", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x500), (1, lambda a: 0x1C), (2, lambda a: 4), (3, lambda a: 4)]),
    0x52A0E0: dict(name="CSolid::E43Vf128", popped=0x20, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x660), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E68E0: dict(name="CCar::Xform68E0", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x400), (1, lambda a: 0xC), (2, lambda a: 0x10)]),
    0x52F090: dict(name="NTweak::CBase::ctor", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x473CD0: dict(name="NTweak::CFloat3::Scale", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 0x10)]),
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x40), (3, lambda a: 0x10)]),
}
