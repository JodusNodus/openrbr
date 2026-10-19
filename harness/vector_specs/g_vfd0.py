"""The VfD0 group (lead/reports/g-vfd0.md): E33Mat34::FetchY (0x51b780), CBrakeWall::VfF0 (0x4e3970) and
E33Mat34::CombineTwin (0x51ab70) inside a step (snapH 1295 reaches VfD0 in its first step), and, directly, the Five-Six
pair walkers and E33Mat34::ComposeB (0x51b600), which no step window reaches. The records replay in entities_vectors
and collision_vectors.

    harness/vcu_vectors.py SNAPSHOT OUT.txt --set g-vfd0 [--fuzz SEED]
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-vfd0 --addr ADDR --count N --seed SEED
"""

SET_NAME = "g-vfd0"

import struct  # noqa: E402


def _word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


# The direct walks: two chains of nodes (vtable, child link +4, depth +8) up to a shared root, in the stub heap. Every
# hook slot (1..6) of the vtable is E43PairWalk::MAB10 (0x51ab10, `ret 8`), so a hook is one external call.
HOOK = 0x51AB10
VTABLE, ROOT, CHAIN_A, CHAIN_B, MATRIX, OUT = 0x100, 0x200, 0x300, 0x600, 0x900, 0x980


def _chains(heap, rng, la, lb):
    """The vtable, the root and the two chains (la and lb nodes deep): returns (writes, the deepest of each)."""
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
    """A direct walk of the deepest node of chain A against the deepest of chain B, their depths from `depths`."""
    def fuzz(mu, heap, rng):
        del mu
        la, lb = depths(rng)
        writes, (a, b) = _chains(heap, rng, la, lb)
        return [b, rng.getrandbits(32), rng.getrandbits(32)], writes, a
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


def _compose_b_fuzz(mu, heap, rng):
    """ComposeB(matrix, o2, out) on the worker: a drawn matrix, a drawn `out` and the two chains."""
    writes, (worker, o2) = _chains(heap, rng, *_any(rng))
    writes.append((heap + MATRIX, struct.pack("<4f", *[rng.uniform(-5.0, 5.0) for _ in range(4)])))
    writes.append((heap + OUT, struct.pack("<4f", *[rng.uniform(-5.0, 5.0) for _ in range(4)])))
    return [heap + MATRIX, o2, heap + OUT], writes, worker


def _nodes(*roots):
    """The +4 chains from the arguments at `roots` (each node's 0x10 bytes)."""
    def nodes(mu, a):
        out = []
        for i in roots:
            p = a[i]
            for _ in range(64):
                if not p:
                    break
                out.append((p, 0x10))
                p = _word(mu, p + 4)
        return out
    return nodes


def _hooks(mu, a):
    return [(HOOK, 8)]


def _walker(name, depths):
    return dict(name="E43PairWalk::" + name, popped=12, x87=False, thiscall=True, windows=[], direct=True,
                nodes=_nodes(0, 1), dispatch=_hooks, direct_fuzz=_walk_fuzz(depths))


# CBrakeWall::VfF0's slot 0xe8 (VfE8, `(a, b, 0, out, 1)`): with --fuzz, 60% of its calls are answered as hits with
# four drawn vectors in `out`, the rest run.
def _vf_e8_hit(mu, args, rng, write):
    if rng.random() >= 0.6:
        return None
    for k in range(16):
        write(args[4] + 4 * k, rng.uniform(-3.0, 3.0))
    return 1


def _vf_e8(mu, args):
    spec = dict(name="CBrakeWall slot 0xe8 (VfE8)", popped=20, x87=False, thiscall=True, external=True,
                windows=[(4, lambda a: 0x40)], result=_vf_e8_hit)
    return [(_word(mu, _word(mu, args[0]) + 0xE8), 20, spec)]


SPECS = {
    0x51AA70: _walker("WA70", _equal),
    0x51B1A0: _walker("WB1A0", _deeper),
    0x51B210: _walker("WB210", _shallower),
    0x51B3C0: _walker("WB3C0", _any),
    0x51B600: dict(name="E33Mat34::ComposeB", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)], nodes=_nodes(0, 2), dispatch=_hooks,
                   direct_fuzz=_compose_b_fuzz),
    0x51AB70: dict(name="E33Mat34::CombineTwin", popped=8, x87=False, thiscall=True, windows=[(0, lambda a: 8)]),
    # FetchY(out, arg) on a transform whose matrix is a contact grid: the grid (0x6b0 bytes) copied to out, then the
    # grid walk (0x51b410, external: its writes to out are its window).
    0x51B780: dict(name="E33Mat34::FetchY", popped=8, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 0x6B0)],
                   nodes=lambda mu, a: [(_word(mu, a[0]), 0x6B0)]),
    0x51B410: dict(name="E43PairWalk::WB410 (grid walk)", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 0x6B0)]),
    # VfF0(a..h): VfE8 into its frame, then the copies to c (16 bytes), d (4), e (16), f, g and h (4 each).
    0x4E3970: dict(name="CBrakeWall::VfF0", popped=32, x87=False, thiscall=True, direct=True,
                   windows=[(3, lambda a: 0x10), (4, lambda a: 4), (5, lambda a: 0x10), (6, lambda a: 4),
                            (7, lambda a: 4), (8, lambda a: 4)],
                   dispatch=_vf_e8),
}
