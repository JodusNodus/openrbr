"""CBody::VfD0 (0x4e1eb0) and E37EvOps::PlaneDiff (0x4f40f0) with their callees, all external: a --fuzz step records
only VfD0's calls (the collision set's own fuzz recipes stay off), and PlaneDiff is called directly. The records replay
in collision_vectors (`body_vf_d0_matches_original_calls`, `plane_diff_matches_original_calls`).

    harness/vcu_vectors.py SNAPSHOT OUT.txt --set body-vfd0 --fuzz SEED
    harness/direct_calls.py SNAPSHOT OUT.txt --set body-vfd0 --addr 0x4f40f0 --count N --seed SEED
"""

SET_NAME = "body-vfd0"

import struct  # noqa: E402


def _word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


# CBody::VfD0 (0x4e1eb0), recorded inside a step with --fuzz: the CDef's contact bits (+0x160cc, 4 and 8) and the other
# entity's 0x40 flag are drawn at random, and EdgeEdge and the other's slot 0xf0, when VfD0 calls them, are not run but
# answered (_vfd0_hit), so the hit paths the drive does not reach are recorded. The inputs go back when VfD0 returns.
VFD0 = 0x4E1EB0
VFD0_END = 0x4E284B


def _vfd0_frame(esp):
    """VfD0's frame (the port's VfD0Frame) from its entry esp: after `push ebp`, the alignment, the exception record,
    0xe78 bytes of locals and three saved registers."""
    return ((esp - 4) & ~0xF) - 0xE90


def _vfd0_nodes(mu, _args):
    from unicorn.x86_const import UC_X86_REG_ESP
    world = _word(mu, 0x01659B70)
    frame = _vfd0_frame(mu.reg_read(UC_X86_REG_ESP))
    return [(0x01659B70, 4), (world + 0x24, 4), (_word(mu, world + 0x24) + 0x160CC, 4), (0x007CBB00, 4),
            (frame, 0xE90)]


def _vfd0_fuzz(mu, args, rng, put):
    flags_at = _word(mu, _word(mu, 0x01659B70) + 0x24) + 0x160CC
    put(flags_at, (_word(mu, flags_at) & ~0xC) | rng.choice((0, 4, 8, 0xC, 0xC, 0xC)), "<I")
    flag = bytes(mu.mem_read(args[2] + 0x60, 1))[0]
    if flag < 0x80:
        put(args[2] + 0x60, rng.choice((flag, flag | 0x40, flag & ~0x40)), "<B")


def _vfd0_called(mu):
    from unicorn.x86_const import UC_X86_REG_ESP
    return VFD0 <= _word(mu, mu.reg_read(UC_X86_REG_ESP)) < VFD0_END


def _vfd0_hit(hit, normal, scalars):
    """A result for EdgeEdge or slot 0xf0 called by VfD0 (args index: the hit and normal vectors, the scalar outputs):
    a hit on 60% of the calls, its point and normal drawn; other callers run the call."""
    def result(mu, args, rng, write):
        if not _vfd0_called(mu):
            return None
        for k in range(4):
            write(args[hit] + 4 * k, rng.uniform(-3.0, 3.0))
            write(args[normal] + 4 * k, rng.uniform(-1.0, 1.0))
        for k in scalars:
            write(args[k], rng.uniform(0.0, 1.0))
        return 1 if rng.random() < 0.6 else 0
    return result


def _vfd0_inactive(mu, _args, rng, _write):
    """The other's slot 0x110 called by VfD0: 0 (no active grid) on a tenth of the calls; else it runs."""
    return 0 if _vfd0_called(mu) and rng.random() < 0.1 else None


def _vfd0_dispatch(mu, args):
    """The slots VfD0 calls: the other entity's 0x30, 0x110 (_vfd0_inactive), 0x118, 0x80 and 0xf0 (its point test,
    _vfd0_hit), this body's 0x30 and 0x80, and the list's slot 1."""
    def slot(obj, offset):
        return _word(mu, _word(mu, obj) + offset)
    this, other, lst = args[0], args[2], args[3]
    f0 = dict(name="VfD0 other slot 0xf0", popped=32, x87=False, thiscall=True, external=True,
              windows=[(1, lambda a: 8), (2, lambda a: 8), (3, lambda a: 0x10), (4, lambda a: 4), (5, lambda a: 0x10),
                       (6, lambda a: 4), (7, lambda a: 4)],
              result=_vfd0_hit(3, 5, (4, 6)))
    active = dict(name="VfD0 other slot 0x110", popped=0, x87=False, thiscall=True, external=True, windows=[],
                  result=_vfd0_inactive)
    return ([(slot(other, k), 0) for k in (0x30, 0x118, 0x80)] + [(slot(other, 0x110), 0, active)] +
            [(slot(other, 0xF0), 32, f0)] +
            [(slot(this, k), 0) for k in (0x30, 0x80)] + [(slot(lst, 4), 4)])


def _vec_op_windows():
    """`out = op(this, b)`, thiscall with (out, b): the two inputs, then the output."""
    return [(0, lambda a: 0x10), (2, lambda a: 0x10), (1, lambda a: 0x10)]


# E37EvOps::PlaneDiff (0x4f40f0), direct (harness/direct_calls.py --set body-vfd0 --addr 0x4f40f0): the first event of
# the collision set's M_C1A0 recipe, one side missing on a fifth of the calls, `out` drawn (its w must stay).
PLANE_DIFF_OUT = 0x8000  # heap offset, clear of the E38 recipe's objects


def _plane_diff_direct_fuzz(mu, heap, rng):
    words, writes, _solver = SETS["collision"][0x4FC1A0]["direct_fuzz"](mu, heap, rng)
    event = words[0]
    if rng.random() < 0.2:
        writes.append((event + rng.choice((0x98, 0xA0)), struct.pack("<I", 0)))
    out = heap + PLANE_DIFF_OUT
    writes.append((out, struct.pack("<4f", *[rng.uniform(-5.0, 5.0) for _ in range(4)])))
    return [out], writes, event


def _plane_diff_sides(mu, args):
    return [body for body in (_word(mu, args[0] + 0xA0), _word(mu, args[0] + 0x98)) if body]


def _plane_diff_nodes(mu, args):
    nodes = []
    for body in _plane_diff_sides(mu, args):
        nodes += [(body, 4), (_word(mu, body) + 0x140, 4)]
    return nodes


def _plane_diff_dispatch(mu, args):
    """Each side's slot 0x140 (`point`, `out`): the point read, the velocity written."""
    spec = dict(name="PlaneDiff side slot 0x140", popped=8, x87=False, thiscall=True, external=True,
                windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)])
    return [(_word(mu, _word(mu, body) + 0x140), 8, spec) for body in _plane_diff_sides(mu, args)]


SPECS = {
    0x4F40F0: dict(name="E37EvOps::PlaneDiff", popped=4, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x100), (1, lambda a: 0x10)], nodes=_plane_diff_nodes,
                   dispatch=_plane_diff_dispatch, direct_fuzz=_plane_diff_direct_fuzz),
    VFD0: dict(name="CBody::VfD0", popped=12, x87=False, thiscall=True, direct=True,
               windows=[(2, lambda a: 0x64)], nodes=_vfd0_nodes, dispatch=_vfd0_dispatch, fuzz=_vfd0_fuzz),
    # VfD0's callees. Inputs first, then outputs (the replay checks the inputs the port leaves in its frame).
    0x4F6550: dict(name="E38RaySeeds", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x4F6740: dict(name="E38RayClear", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x4E0D00: dict(name="E33MakeEvent", popped=36, x87=False, cdecl=True, external=True,
                   windows=[(1, lambda a: 0x10), (5, lambda a: 0x10), (7, lambda a: 0x10)]),
    0x4E0840: dict(name="CColGrid::SampleA", popped=16, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 4), (4, lambda a: 4)]),
    0x51B6C0: dict(name="E33Mat34::FetchX", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 0x10)]),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x51BD30: dict(name="E33Mat34::CollideInit", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    0x51B780: dict(name="E33Mat34::FetchY", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 0x6B0)]),
    0x51AB90: dict(name="E33Mat34::Combine", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (0, lambda a: 8)]),
    0x51AB70: dict(name="E33Mat34::CombineTwin", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (0, lambda a: 8)]),
    0x51B600: dict(name="E33Mat34::ComposeB", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    0x4F6610: dict(name="E37GridOps::M6610", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x6B0)]),
    0x4F8280: dict(name="E37GridOps::EdgeEdge", popped=0x18, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10), (4, lambda a: 4),
                            (5, lambda a: 0x10), (6, lambda a: 4)],
                   result=_vfd0_hit(3, 5, (4,))),
    0x43C6A0: dict(name="E01Vec4::Normalize", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x4A19E0: dict(name="E21Vec3::Add", popped=8, x87=False, thiscall=True, external=True, windows=_vec_op_windows()),
    0x465DA0: dict(name="E09Vec4::Sub", popped=8, x87=False, thiscall=True, external=True, windows=_vec_op_windows()),
    0x4E0950: dict(name="CColGrid::SampleB", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 4), (3, lambda a: 4)]),
}
