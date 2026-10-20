"""Direct calls for entity bindings no drive reaches (gap/car-proofs).

E37GridOps::Copy6770 (0x4f6770, thiscall (from), ret 4): a random grid (0x6b0 bytes) copied onto random bytes, and a
null `this` in a tenth of the calls (nothing copied, eax 0).

CMountedRigidBody::Vf138 (0x521a50, thiscall six words, ret 0x18; the asm of CHingedRigidBody::SimE4F0 apart from
addresses) on the mounted bodies of vcu-v2-gravel-impreza00-63.snap-2730 (the objects with the vtable 0x73e62c): the
owner-car path, and with the owner (+0x3d0) cleared in a third of the calls, the forward of the six words to
CSolid::E43Vf138 (0x52b990, an inner record with the body's window and the six words' vectors; the owned path calls it on
the car).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-ent-proofs --addr 0x4F6770 --count N --seed SEED
    harness/direct_calls.py vcu-v2-gravel-impreza00-63.snap-2730.bin OUT.txt --set g-ent-proofs --addr 0x521A50 ...
"""

import struct  # noqa: E402 (the recipes pack words)

SET_NAME = "g-ent-proofs"

GRID = 0x6B0  # a contact grid's bytes (0x1ac words)
MOUNTED = (0xC337448, 0xC3379E0, 0xC337F78)  # the snapshot's mounted bodies
HINGED2 = SETS["g-hinged2"]  # noqa: F821 (vcu_vectors' sets)


def copy_direct(mu, heap, rng):
    del mu
    to, src = heap + 0xD4000, heap + 0xD5000
    writes = [(to, bytes(rng.getrandbits(8) for _ in range(GRID))), (src, bytes(rng.getrandbits(8) for _ in range(GRID)))]
    return [src], writes, 0 if rng.random() < 0.1 else to


def mounted_direct(mu, heap, rng):
    """SimE4F0's recipe (g_hinged2) on a mounted body, its owner cleared in a third of the calls."""
    words, writes, _ = HINGED2[0x51E4F0]["direct_fuzz"](mu, heap, rng)
    body = rng.choice(MOUNTED)
    if rng.random() < 1 / 3:
        writes.append((body + 0x3D0, struct.pack("<I", 0)))
    return words, writes, body


def mounted_dispatch(mu, a):
    """SimE4F0's dispatch (the car's and the body's slot 12, ComposeA, the walk, the car's slot 0x138) for an owned body."""
    if not struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x3D0, 4)))[0]:
        return []
    return HINGED2[0x51E4F0]["dispatch"](mu, a)


SPECS = {
    0x4F6770: dict(name="E37GridOps::Copy6770", popped=4, x87=False, thiscall=True, direct_fuzz=copy_direct,
                   windows=[(0, lambda a: GRID), (1, lambda a: GRID)]),
    0x521A50: dict(HINGED2[0x51E4F0], name="CMountedRigidBody::Vf138", direct_fuzz=mounted_direct,
                   dispatch=mounted_dispatch),
    0x52B990: dict(name="CSolid::E43Vf138", popped=0x18, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x660)] + [(i, lambda a: 0x10) for i in range(1, 7)]),
}
