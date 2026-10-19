"""The CollideInit group: E33Mat34::CollideInit (0x51bd30) and CBrakeWall::VfE8 (0x4e3a10)
inside a step, with --fuzz answering their queries so the hit tails no drive reaches are recorded, and, directly, the
grid pair walkers (0x51b410 and the three it routes to). The records replay in entities_collide, collision_vectors and
entities_vectors.

    harness/vcu_vectors.py SNAPSHOT OUT.txt --set g-collide [--fuzz SEED]
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-collide --addr ADDR --count N --seed SEED
"""

SET_NAME = "g-collide"

import math  # noqa: E402
import struct  # noqa: E402


def _word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def _float(mu, at):
    return struct.unpack("<f", bytes(mu.mem_read(at, 4)))[0]


def _ret(mu):
    from unicorn.x86_const import UC_X86_REG_ESP
    return _word(mu, mu.reg_read(UC_X86_REG_ESP))


# CollideInit (0x51bd30): its frame (the port's CollideFrame) from the entry esp, the globals it reads, bodyA's flags.
# With --fuzz bodyA's second-pass bit is drawn, the gate passes on 80% of the calls, and every query CollideInit makes
# (0x4fd760, 0x4febc0) is answered: a hit on half of them, its output drawn.
COLLIDE, COLLIDE_END = 0x51BD30, 0x51C656
HOST_GLOBAL, IDENTITY_GLOBAL = 0x007C755C, 0x007CBB00


def _collide_frame(esp):
    return ((esp - 4) & ~0xF) - 0x860


def _collide_nodes(mu, _args):
    from unicorn.x86_const import UC_X86_REG_ESP
    return [(HOST_GLOBAL, 4), (IDENTITY_GLOBAL, 4), (_collide_frame(mu.reg_read(UC_X86_REG_ESP)), 0x860)]


def _collide_fuzz(mu, args, rng, put):
    flags = bytes(mu.mem_read(args[1] + 0x60, 1))[0]
    put(args[1] + 0x60, rng.choice((flags | 8, flags & ~8)), "<B")


def _from_collide(mu):
    return COLLIDE <= _ret(mu) < COLLIDE_END


def _query_hit(out):
    def result(mu, args, rng, write):
        if not _from_collide(mu):
            return None
        if rng.random() >= 0.5:
            return 0
        for k in range(4):
            write(args[out] + 4 * k, rng.uniform(-3.0, 3.0))
        n = [rng.uniform(-1.0, 1.0) for _ in range(3)]
        norm = math.sqrt(sum(x * x for x in n)) or 1.0
        for k in range(3):
            write(args[out] + 0x10 + 4 * k, n[k] / norm)
        write(args[out] + 0x1C, 0.0)
        for k in range(8, 13):
            write(args[out] + 4 * k, rng.uniform(0.0, 1.0))
        return 1
    return result


def _gate(mu, args, rng, write):
    if not _from_collide(mu):
        return None
    return 1 if rng.random() < 0.8 else None


def _collide_dispatch(mu, args):
    def slot(obj, offset):
        return _word(mu, _word(mu, obj) + offset)
    body, sink = args[1], args[3]
    gate = dict(name="CollideInit bodyA slot 0x120", popped=8, x87=False, thiscall=True, external=True,
                windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)])
    return [(slot(body, 0x120), 8, gate), (slot(body, 0x30), 0), (slot(body, 0x118), 0), (slot(sink, 4), 4)]


# VfE8 (0x4e3a10): the wall's leaf list, its segments, their index tables and the vertices they index, the identity
# global. With --fuzz, 70% of its FetchX pairs are answered: two points on either side of one of its edges, so the
# crossing (and the latch and bit-14 paths) is recorded.
VFE8, VFE8_END = 0x4E3A10, 0x4E3DBE
VFE8_FIRST_FETCH = 0x4E3A4D


def _wall_edges(mu, wall):
    """The (index word, its address, vertex address) of every leaf edge of `wall`."""
    edges = []
    mesh = _word(mu, wall + 0x218)
    for i in range(min(_word(mu, wall + 0x20C), 100)):
        seg = _word(mu, wall + 0x7C + 4 * i)
        table = _word(mu, seg + 0x14)
        for j in range(min(_word(mu, seg + 0x10), 256)):
            word = struct.unpack("<H", bytes(mu.mem_read(table + 2 * j, 2)))[0]
            edges.append((word, table + 2 * j, mesh + 8 * (word & 0x3FFF)))
    return edges


def _vf_e8_nodes(mu, args):
    wall = args[0]
    nodes = [(IDENTITY_GLOBAL, 4), (wall, 4), (wall + 0x7C, 0x19C), (wall + 0x218, 4)]
    for i in range(min(_word(mu, wall + 0x20C), 100)):
        seg = _word(mu, wall + 0x7C + 4 * i)
        nodes += [(seg + 0x10, 8), (_word(mu, seg + 0x14), 2 * min(_word(mu, seg + 0x10), 256))]
    nodes += [(v, 0x20) for _, _, v in _wall_edges(mu, wall)]
    return nodes


_PENDING = {}


def _fetch_x(mu, args, rng, write):
    from unicorn.x86_const import UC_X86_REG_EDI
    ret = _ret(mu)
    if not VFE8 <= ret < VFE8_END:
        return None
    wall = mu.reg_read(UC_X86_REG_EDI)
    if ret == VFE8_FIRST_FETCH:
        _PENDING.pop(wall, None)
        edges = _wall_edges(mu, wall)
        if not edges or rng.random() >= 0.7:
            return None
        _, _, v = rng.choice(edges)
        a = (_float(mu, v + 8), _float(mu, v + 0xC))
        b = (_float(mu, v + 0x18), _float(mu, v + 0x1C))
        e = (b[0] - a[0], b[1] - a[1])
        length = math.hypot(*e) or 1.0
        n = (e[1] / length, -e[0] / length)
        u = rng.uniform(0.05, 0.95)
        p = (a[0] + u * e[0], a[1] + u * e[1])
        d0, d1 = rng.uniform(0.2, 3.0), rng.uniform(0.2, 3.0)
        z = rng.uniform(-2.0, 2.0)
        _PENDING[wall] = (p[0] - n[0] * d1, p[1] - n[1] * d1, z + rng.uniform(-1.0, 1.0))
        point = (p[0] + n[0] * d0, p[1] + n[1] * d0, z)
    else:
        point = _PENDING.pop(wall, None)
        if point is None:
            return None
    for k in range(3):
        write(args[1] + 4 * k, point[k])
    return args[1]


# The grid walkers, directly: two chains of nodes (vtable, child link +4, depth +8) to a shared root and up to two
# contact grids in the stub heap; every hook slot (1..4) is E43PairWalk::MAB10 (0x51ab10, `ret 8`).
HOOK = 0x51AB10
VTABLE, ROOT, CHAIN_A, CHAIN_B, GRIDS = 0x100, 0x200, 0x300, 0x600, 0x1000
GRID_SIZE = 0x6B0


def _chains(heap, rng, la, lb):
    writes = [(heap + VTABLE, struct.pack("<7I", 0, *[HOOK] * 6))]
    depth = rng.randint(0, 3)
    writes.append((heap + ROOT, struct.pack("<4I", heap + VTABLE, 0, depth, 0)))
    ends = []
    for base, n in ((CHAIN_A, la), (CHAIN_B, lb)):
        parent = heap + ROOT
        for k in range(n):
            at = heap + base + 0x10 * k
            writes.append((at, struct.pack("<4I", heap + VTABLE, parent, depth + k + 1, 0)))
            parent = at
        ends.append(parent)
    return writes, ends


def _walk_fuzz(depths):
    def fuzz(mu, heap, rng):
        del mu
        la, lb = depths(rng)
        writes, (a, b) = _chains(heap, rng, la, lb)
        n = rng.randint(0, 2)
        writes.append((heap + GRIDS + 0x68D, bytes([rng.randint(0, 12)])))
        return [b, heap + GRIDS, n], writes, a
    return fuzz


def _equal(rng):
    n = rng.randint(0, 4)
    return n, n


def _deeper(rng):
    lb = rng.randint(0, 3)
    return lb + rng.randint(0, 3), lb


def _shallower(rng):
    la = rng.randint(0, 3)
    return la, la + rng.randint(0, 3)


def _any(rng):
    return rng.randint(0, 4), rng.randint(0, 4)


def _nodes(mu, a):
    out = [(a[2] + 0x68D, 1)]
    for i in (0, 1):
        p = a[i]
        for _ in range(64):
            if not p:
                break
            out.append((p, 0x10))
            p = _word(mu, p + 4)
    return out


def _walker(name, depths):
    return dict(name="E43PairWalk::" + name, popped=12, x87=False, thiscall=True, windows=[], direct=True,
                nodes=_nodes, dispatch=lambda mu, a: [(HOOK, 8)], direct_fuzz=_walk_fuzz(depths))


SPECS = {
    COLLIDE: dict(name="E33Mat34::CollideInit", popped=12, x87=False, thiscall=True, direct=True,
                  windows=[(1, lambda a: 0x64)], nodes=_collide_nodes, dispatch=_collide_dispatch, fuzz=_collide_fuzz),
    VFE8: dict(name="CBrakeWall::VfE8", popped=20, x87=False, thiscall=True, direct=True,
               windows=[(4, lambda a: 0x3C)], nodes=_vf_e8_nodes),
    0x51B6C0: dict(name="E33Mat34::FetchX", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)], result=_fetch_x),
    # CollideInit's callees, inputs first (the replay checks the frame bytes the port leaves before each call).
    0x4F3E80: dict(name="NTweak::CE36SubBlock::Init", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x80)]),
    0x4FF070: dict(name="E39 segment gate", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)], result=_gate),
    0x4F6610: dict(name="E37GridOps::M6610", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (0, lambda a: 0x10)]),
    0x4FD760: dict(name="E38Ops::M_D760X", popped=16, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x34)], result=_query_hit(3)),
    0x4FEBC0: dict(name="E38Ops::M_EBC0", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x34)], result=_query_hit(2)),
    0x4F6740: dict(name="E38RayClear", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x4F4F10: dict(name="E36AllocEvent", popped=4, x87=False, cdecl=True, external=True, windows=[]),
    0x4F50E0: dict(name="CCollisionEvent::InitFull", popped=24, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 0x50), (5, lambda a: 0x10), (0, lambda a: 0xF0)]),
    0x4E0950: dict(name="CColGrid::SampleB", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 4), (3, lambda a: 4)]),
    0x4E0840: dict(name="CColGrid::SampleA", popped=16, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 4), (4, lambda a: 4)]),
    0x473C90: dict(name="E11_VecLerp", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x43C6A0: dict(name="E01Vec4::Normalize", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x473B30: dict(name="CFloat3::Length", popped=0, x87=True, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xC)]),
    0x51B410: _walker("WB410", _any),
    0x51AAC0: _walker("WAC0", _equal),
    0x51B280: _walker("WB280", _deeper),
    0x51B2D0: _walker("WB2D0", _shallower),
}
