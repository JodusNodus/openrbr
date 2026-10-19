"""The VfDC group (lead/reports/g-vfdc.md): CBody::VfDC (0x4e2850) inside a step, its EdgePair hits drawn with --fuzz,
and CEntity::VfE8 (0x519970, `xor eax, eax; ret 0x14`) called directly. The records replay in collision_vectors
(`body_vf_dc_matches_original_calls`) and entities_vectors (`empty_hooks_match_original_calls`).

    harness/vcu_vectors.py SNAPSHOT OUT.txt --set g-vfdc --fuzz SEED
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-vfdc --addr 0x519970 --count N --seed SEED
"""

SET_NAME = "g-vfdc"

import struct  # noqa: E402


def _word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


VFDC = 0x4E2850
VFDC_END = 0x4E2CCF
BODY_VF118 = 0x4E11F0


def _vfdc_frame(esp):
    """VfDC's frame (the port's VfDcFrame) from its entry esp: after `push ebp`, the alignment, the exception record,
    0x14c4 bytes of locals and three saved registers."""
    return ((esp - 4) & ~0xF) - 0x14E0


def _vfdc_nodes(mu, args):
    """This body's grid (its flag bytes +0x688, +0x689 and the counts +0x68c, +0x68d), the frame global and the frame
    (last: the stale fourth lanes of the box's centre and half extent are read from it)."""
    from unicorn.x86_const import UC_X86_REG_ESP
    nodes = [(0x007CBB00, 4)]
    grid = _word(mu, args[0] + 0x64)
    if grid:
        nodes.insert(0, (grid + 0x688, 8))
    return nodes + [(_vfdc_frame(mu.reg_read(UC_X86_REG_ESP)), 0x14E0)]


def _vfdc_fuzz(mu, args, rng, put):
    """The grid's two flag bytes, which a hit copies into the event and clears. When the other entity is a body
    (its slot 0x118 is CBody::Vf118, `mov eax, [ecx+0x64]`), its grid's box and points are scaled up on 70% of the
    calls, so the overlap box takes in this body's edges and BoxHit passes them to EdgePair (the drive's bodies are
    apart)."""
    grid = _word(mu, args[0] + 0x64)
    if grid:
        put(grid + 0x688, rng.randrange(256), "<B")
        put(grid + 0x689, rng.randrange(256), "<B")
    other = args[3]
    if _word(mu, _word(mu, other) + 0x118) != BODY_VF118 or rng.random() >= 0.7:
        return
    src = _word(mu, other + 0x64)
    if not src:
        return
    scale = rng.uniform(5.0, 50.0)
    points = bytes(mu.mem_read(src + 0x68D, 1))[0]
    for at in [src, src + 0x10] + [src + 0x40 + 0x10 * k for k in range(points)]:
        for lane in range(3):
            value = struct.unpack("<f", bytes(mu.mem_read(at + 4 * lane, 4)))[0]
            put(at + 4 * lane, value * scale)


def _vfdc_called(mu):
    from unicorn.x86_const import UC_X86_REG_ESP
    return VFDC <= _word(mu, mu.reg_read(UC_X86_REG_ESP)) < VFDC_END


def _edge_pair_result(mu, args, rng, write):
    """EdgePair called by VfDC: half of the calls run; the others are answered, a hit on 70% of them with the hit, t and
    normal drawn."""
    if not _vfdc_called(mu) or rng.random() < 0.5:
        return None
    if rng.random() >= 0.7:
        return 0
    for k in range(4):
        write(args[5] + 4 * k, rng.uniform(-3.0, 3.0))
        write(args[7] + 4 * k, rng.uniform(-1.0, 1.0))
    write(args[6], rng.uniform(0.0, 1.0))
    return 1


def _vfdc_dispatch(mu, args):
    """The slots VfDC calls: the other entity's 0x118 (its grid; the counts read after), 0x34 and 0x30, this body's
    0x30, and the list's slot 1 (Append; the event as it is appended)."""
    def slot(obj, offset):
        return _word(mu, _word(mu, obj) + offset)
    this, other, lst = args[0], args[3], args[5]
    grid = dict(name="VfDC other slot 0x118", popped=0, x87=False, thiscall=True, external=True, windows=[],
                after=lambda mu, a, eax: [(eax + 0x688, 8)] if eax else [])
    append = dict(name="VfDC list slot 1 (Append)", popped=4, x87=False, thiscall=True, external=True,
                  windows=[(1, lambda a: 0xB0)])
    return ([(slot(other, 0x118), 0, grid)] + [(slot(other, k), 0) for k in (0x34, 0x30)] +
            [(slot(this, 0x30), 0), (slot(lst, 4), 4, append)])


def _vfe8_fuzz(mu, heap, rng):
    del mu
    return [rng.getrandbits(32) for _ in range(5)], [], heap


SPECS = {
    VFDC: dict(name="CBody::VfDC", popped=0x14, x87=False, thiscall=True, direct=True,
               windows=[(0, lambda a: 0x68)], nodes=_vfdc_nodes, dispatch=_vfdc_dispatch, fuzz=_vfdc_fuzz),
    0x519970: dict(name="CEntity::VfE8", popped=0x14, x87=False, thiscall=True, windows=[], direct_fuzz=_vfe8_fuzz),
    # VfDC's callees. Inputs first, then outputs (the replay checks the inputs the port leaves).
    0x4F6550: dict(name="E38RaySeeds", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x4F6740: dict(name="E38RayClear", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x51B780: dict(name="E33Mat34::FetchY", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 0x6B0)]),
    0x4F6610: dict(name="E37GridOps::M6610", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    # EdgePair(a0, a1, b0, b1, hit, t, normal, scratch, grid, 0) on the other grid: the grid and the four edge ends in,
    # the hit, t and normal out (the grid's flag bytes too, in its window).
    0x4F8560: dict(name="E37GridOps::EdgePair", popped=0x28, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0), (1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10),
                            (4, lambda a: 0x10), (5, lambda a: 0x10), (6, lambda a: 4), (7, lambda a: 0x10)],
                   result=_edge_pair_result),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x51B600: dict(name="E33Mat34::ComposeB", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x4E0D00: dict(name="E33MakeEvent", popped=36, x87=False, cdecl=True, external=True,
                   windows=[(1, lambda a: 0x10), (5, lambda a: 0x10), (7, lambda a: 0x10)],
                   after=lambda mu, a, eax: [(eax, 0xB0)] if eax else []),
}
