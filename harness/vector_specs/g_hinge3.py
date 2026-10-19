"""Direct-call records for G-hinge3: E43Vf128's hook and ring tail, the ring append, and SimFD00 (the detach) with
its callees."""

import struct

SET_NAME = "g-hinge3"

HINGED_VTABLE = 0x73D23C
PHYSICS = 0x1659B70
# The hinged bodies of the tanner-damage snapshots (11820, 11827, 12000; the first is free in 12000), on the car's
# list at car+0x48 while owned.
BODIES = (0xC3494A8, 0xC349B10, 0xC34A178, 0xC34A7E0)


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def vtarget(mu, this, slot):
    return word(mu, word(mu, this) + slot)


def floats(at, values):
    return [(at + 4 * k, struct.pack("<f", v)) for k, v in enumerate(values)]


def bodies(mu, owned=True):
    """The snapshot's hinged bodies: the owned ones, or all with `owned=None`."""
    found = [b for b in BODIES if word(mu, b) == HINGED_VTABLE and owned in (None, word(mu, b + 0x3D0) != 0)]
    if not found:
        raise RuntimeError("no hinged body in this snapshot")
    return found


def ring_writes(ring, rng):
    """A ring tail: wrap flag, index and count, sometimes full or at the wrap."""
    wraps = rng.random() < 0.5
    index = rng.choice((0, 1, 5, 0x3E, 0x3F, 0x40, rng.randrange(0x40)))
    count = rng.choice((index, index + 1, 0x40, rng.randrange(0x41)))
    return [(ring, bytes(0x720)), (ring, struct.pack("<III", int(wraps), index, count))]


def fuzz_ring(mu, heap, rng):
    ring, src = heap + 0x1000, heap + 0x800
    writes = ring_writes(ring, rng) + floats(src, [rng.uniform(-50, 50) for _ in range(4)])
    words = [src, rng.getrandbits(32), struct.unpack("<I", struct.pack("<f", rng.uniform(-2, 2)))[0], rng.getrandbits(32)]
    return words, writes, ring


def fuzz_vf128(mu, heap, rng):
    """A hooked body (the hinged bodies share the car's damage hook), the impulse at a point; the hook dropped or a ring
    tail added (with the physics global's ring bit) on some calls."""
    body = rng.choice(bodies(mu, owned=None))
    point, impulse, pair, ring = heap + 0x800, heap + 0x810, heap + 0x820, heap + 0x1000
    position = struct.unpack("<3f", bytes(mu.mem_read(body + 0x140, 12)))
    writes = floats(point, [v + rng.uniform(-1, 1) for v in position] + [0.0])
    writes += floats(impulse, [rng.uniform(-3000, 3000) for _ in range(3)] + [0.0])
    writes += [(pair, struct.pack("<II", rng.getrandbits(32), rng.getrandbits(32))),
               (pair + 8, struct.pack("<f", rng.choice((0.0, 0.25, 0.5, 0.75, 1.0, float("nan")))))]
    shape = rng.random()
    debug = word(mu, word(mu, PHYSICS) + 0x24) + 0x160E0
    bit = word(mu, debug)
    if shape < 0.25:
        writes.append((body + 0x3CC, struct.pack("<I", 0)))
    if shape < 0.5 or shape > 0.85:
        writes += ring_writes(ring, rng) + [(body + 0x3C8, struct.pack("<I", ring))]
        bit = bit | 0x1000000 if rng.random() < 0.8 else bit & ~0x1000000
    writes.append((debug, struct.pack("<I", bit)))
    return [point, impulse, 0, 0, pair, struct.unpack("<I", struct.pack("<f", rng.uniform(0, 20)))[0], 0, impulse], \
        writes, body


def vf128_dispatch(mu, a):
    body = a[0]
    out = [(vtarget(mu, body, 0x30), 0)]
    hook = word(mu, body + 0x3CC)
    if hook:
        out.append((vtarget(mu, hook, 4), 0x20))
    return out


def vf128_nodes(mu, a):
    debug = word(mu, word(mu, PHYSICS) + 0x24)
    return [(PHYSICS, 4), (word(mu, PHYSICS) + 0x24, 4), (debug + 0x160E0, 4), (0x795E18, 4), (0x7CBB00, 4),
            (a[5], 0xC)]


def fuzz_fd00(mu, heap, rng):
    """An owned body detaches; its pose and velocities moved. Some calls have no parent or no list (no detach)."""
    body = rng.choice(bodies(mu))
    writes = []
    for at, spread in ((0x1C0, 20.0), (0x190, 5.0), (0x140, 3.0)):
        for k in range(3):
            v = struct.unpack("<f", bytes(mu.mem_read(body + at + 4 * k, 4)))[0] if at == 0x140 else 0.0
            writes += floats(body + at + 4 * k, [v + rng.uniform(-spread, spread)])
    r = rng.random()
    if r < 0.1:
        writes.append((body + 0x44, struct.pack("<I", 0)))
    elif r < 0.2:
        writes.append((body + 0x18, struct.pack("<I", 0)))
    return [], writes, body


def fd00_nodes(mu, a):
    body = a[0]
    car, lst = word(mu, body + 0x3D0), word(mu, body + 0x18)
    out = [(0x7CBB00, 4), (0x8EB794, 4), (0x893600, 4), (PHYSICS, 4), (word(mu, PHYSICS) + 0x20, 4)]
    if car:
        out.append((car + 0x190, 0x40))
    if lst:
        out.append((lst, 12))
    return out


def fuzz_da60(mu, heap, rng):
    body = rng.choice(bodies(mu))
    writes = []
    for at in (0x140, 0x100):
        writes += floats(body + at, [rng.uniform(-1, 1) for _ in range(4)])
    return [], writes, body


def fuzz_init_bd0(mu, heap, rng):
    body = heap + 0x1000
    return [], [(body, bytes(0x30)), (word(mu, 0x7CBB00) + 8, struct.pack("<I", rng.getrandbits(16)))], body


def fuzz_clear_acc(mu, heap, rng):
    body = heap + 0x1000
    return [], [(body, bytes(rng.getrandbits(8) for _ in range(0x400)))], body


def list_writes(heap, rng):
    """A list (head, tail, count) of 0 to 4 items with links at +0x10 (next, previous, list)."""
    lst, items = heap + 0x800, [heap + 0x1000 + 0x40 * k for k in range(rng.randrange(5))]
    writes = [(lst, struct.pack("<III", items[0] if items else 0, items[-1] if items else 0, len(items)))]
    for k, it in enumerate(items):
        nxt = items[k + 1] if k + 1 < len(items) else 0
        prv = items[k - 1] if k else 0
        writes.append((it, bytes(0x40)))
        writes.append((it + 0x10, struct.pack("<III", nxt, prv, lst)))
    return lst, items, writes


def fuzz_unlink(mu, heap, rng):
    lst, items, writes = list_writes(heap, rng)
    if not items or rng.random() < 0.15:
        free = heap + 0x2000
        return [], writes + [(free, bytes(0x40)), (free + 0x10, struct.pack("<III", 0, 0, 0))], free + 0x10
    return [], writes, rng.choice(items) + 0x10


def fuzz_push(mu, heap, rng):
    lst, items, writes = list_writes(heap, rng)
    item = heap + 0x2000
    return [item], writes + [(item, bytes(0x40))], lst


def unlink_nodes(mu, a):
    lst = word(mu, a[0] + 8)
    return ([(lst, 12)] if lst else []) + [(word(mu, a[0] + k) + 0x10, 8) for k in (0, 4) if word(mu, a[0] + k)]


def push_nodes(mu, a):
    tail = word(mu, a[0] + 4)
    return [(a[0], 12), (a[1] + 0x10, 12)] + ([(tail + 0x10, 4)] if tail else [])


def fuzz_move_cell(mu, heap, rng):
    """A cell link (next, previous, cell) moved to another cell (head, tail, count), or out of one (no cell)."""
    old, new = heap + 0x800, heap + 0x900
    links = [heap + 0x1000 + 0x10 * k for k in range(4)]
    writes = [(heap + 0x1000, bytes(0x40)), (old, bytes(12)), (new, bytes(12))]
    in_old = links[:rng.randrange(4)]
    link = rng.choice(in_old) if in_old and rng.random() < 0.85 else links[3]
    for k, ln in enumerate(in_old):
        writes.append((ln, struct.pack("<III", in_old[k + 1] if k + 1 < len(in_old) else 0, in_old[k - 1] if k else 0, old)))
    if in_old:
        writes.append((old, struct.pack("<III", in_old[0], in_old[-1], len(in_old))))
    head = heap + 0x1100 if rng.random() < 0.5 else 0
    if head:
        writes += [(head, struct.pack("<III", 0, 0, new)), (new, struct.pack("<III", head, head, 1))]
    return [link, new if rng.random() < 0.85 else 0], writes, None


def move_nodes(mu, a):
    link = a[0]
    out = [(link, 16)]
    old = word(mu, link + 8)
    if old:
        out.append((old, 12))
    for k in (0, 4):
        n = word(mu, link + k)
        if n:
            out.append((n, 8))
    if a[1]:
        out.append((a[1], 12))
        head = word(mu, a[1])
        if head:
            out.append((head, 8))
    return out


def fuzz_reg_wall(mu, heap, rng):
    """A free hinged body (out of every list) filed in the physics state: its active and very-soft words."""
    body = rng.choice(bodies(mu))
    state = word(mu, word(mu, PHYSICS) + 0x20)
    writes = [(body + 0x10, bytes(12)), (body + 0x1DC, struct.pack("<I", rng.randrange(2))),
              (body + 0x1EC, struct.pack("<I", rng.randrange(2)))]
    return [body], writes, state


def reg_wall_dispatch(mu, a):
    return [(vtarget(mu, a[1], slot), 0) for slot in (0x8C, 0x80, 0x88)]


def reg_wall_nodes(mu, a):
    state = a[0]
    out = [(state + 0x450, 0x40), (a[1] + 0x10, 12)]
    for at in (0x450, 0x460, 0x470, 0x480):
        tail = word(mu, state + at + 4)
        if tail:
            out.append((tail + 0x10, 4))
    return out


def body(n=0x660):
    return [(0, lambda a: n)]


def ext(name, popped, windows=(), **kw):
    return dict(name=name, popped=popped, x87=False, thiscall=True, external=True, windows=list(windows), **kw)


SPECS = {
    0x52A0E0: dict(name="CSolid::E43Vf128", popped=0x20, x87=False, thiscall=True, direct_fuzz=fuzz_vf128,
                   windows=[(0, lambda a: 0x660), (1, lambda a: 0x10), (2, lambda a: 0x10), (0, lambda a: 0x1F0, 0x34)],
                   nodes=vf128_nodes, dispatch=vf128_dispatch, direct=True),
    0x51B560: ext("E43PairWalk::MB560", 12, [(1, lambda a: 0x10), (2, lambda a: 0x40), (3, lambda a: 0x10)]),
    0x51B4C0: ext("E33Mat34::ComposeA", 12, [(3, lambda a: 0x10)]),
    0x529100: ext("E46_RingAppend", 0x10, [(0, lambda a: 0x720), (1, lambda a: 0x10)], direct_fuzz=fuzz_ring),
    0x51FD00: dict(name="CHingedRigidBody::SimFD00", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_fd00,
                   windows=body(), nodes=fd00_nodes, direct=True,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], s), 0, ext("slot %#x" % s, 0, body())) for s in (0x54, 0x90, 0x48)]),
    0x51DA60: ext("CHingedRigidBody::SimDA60", 0, body(), direct_fuzz=fuzz_da60, direct=True,
                  nodes=lambda mu, a: [(0x7CBB00, 4), (word(mu, a[0] + 0x3D0) + 0x100, 0x10)]),
    0x483DD0: ext("E15Shell::M83DD0", 8, [(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E68E0: ext("CCar::ToWorld", 8, [(1, lambda a: 0xC), (2, lambda a: 0x10)]),
    0x529E60: ext("CSolid::E43ClearAcc", 0, [(0, lambda a: 0x400)], direct_fuzz=fuzz_clear_acc),
    0x4E41A0: ext("E34Link10::UnlinkA", 0, [(0, lambda a: 12)], direct_fuzz=fuzz_unlink, nodes=unlink_nodes),
    0x519BD0: ext("CEntity::E43_InitBD0", 0, [(0, lambda a: 0x30)], direct_fuzz=fuzz_init_bd0,
                  nodes=lambda mu, a: [(0x7CBB00, 4), (word(mu, 0x7CBB00) + 8, 4)]),
    0x4E03B0: ext("CCellGrid::LookupTwin", 8),
    0x4FAEB0: dict(name="CBodyLink::MoveToCell", popped=0, stack_words=2, x87=False, cdecl=True, external=True,
                   windows=[], direct_fuzz=fuzz_move_cell, nodes=move_nodes),
    0x5263D0: ext("CPhysics::CState::RegWall", 4, direct_fuzz=fuzz_reg_wall, nodes=reg_wall_nodes, direct=True,
                  dispatch=reg_wall_dispatch),
    0x4E6AF0: ext("E34LinkList10::PushItem", 4, direct_fuzz=fuzz_push, nodes=push_nodes),
    0x51F580: ext("CMultiSignal1<CSigDetachFromParent>::Emit", 4, [(1, lambda a: 4)]),
    0x51F7C0: ext("CMultiSignal1<CSigMrRBodyEvent>::Emit", 4, [(1, lambda a: 0x50)]),
}
