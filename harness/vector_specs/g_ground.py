# G-ground: CCollisionSystem::Vf00 (0x4fe810), the segment query behind CGround::VfE8's host
# call (slot 0 of g_7c755c, the collision system). Its calls are E38Ops::M_DE40X (0x4fde40, the mesh ray over the tree)
# and the sample table lookup (0x520240). The direct recipe builds a small tree in the stub heap: an interior root whose
# child node has two leaf halves with faces, and a vertex table at this+0xc. The segment crosses the faces' plane near
# the origin's xy, so about half the calls hit.
SET_NAME = "g-ground"

import struct  # noqa: E402 (the recipe packs floats)


def _system_nodes(mu, a):
    """Reads of Vf00 and of M_DE40X: the system's words (root at +8, table at +0xc), the table, and the tree's halves
    (a half is a box, a face count word and a child at +0x1c; the walk follows the children M_DE40X tests)."""
    system = a[0]
    regions = [(system, 0x10), (word(mu, system + 0xC), 12 * 6)]
    todo = [word(mu, system + 8)]
    while todo:
        half = todo.pop()
        regions.append((half, 0x20))
        count = word(mu, half + 0x18) & 0x1FFFFF
        child = word(mu, half + 0x1C)
        if count:
            regions.append((child, 0x12 * count))
        elif child:
            todo += [child, child + 0x20]
    return regions


def _system_direct_fuzz(_mu, heap, rng):
    system, table, root, child = heap + 0x10000, heap + 0x20000, heap + 0x30000, heap + 0x30040
    faces0, faces1 = heap + 0x40000, heap + 0x41000
    origin, end, out = heap + 0x50000, heap + 0x50010, heap + 0x50100
    sx, sy = rng.uniform(-1.0, 1.0), rng.uniform(-1.0, 1.0)
    sz = rng.choice((-1.0, 1.0))
    ez = -sz * rng.choice((0.5, 1.0, 1.5))
    verts = [(sx + rng.uniform(-1.5, 1.5), sy + rng.uniform(-1.5, 1.5), rng.uniform(-0.2, 0.2)) for _ in range(6)]

    def box():
        return [sx + rng.uniform(-0.5, 0.5), sy + rng.uniform(-0.5, 0.5), rng.uniform(-0.5, 0.5)] + [
            rng.uniform(1.0, 3.0) for _ in range(3)]

    def faces(count):
        # A face is three vertex indices (u16) and 12 bytes of tags and barycentric nibbles: 0x12 bytes.
        return b"".join(struct.pack("<3H", *(rng.randrange(6) for _ in range(3))) + bytes(
            rng.getrandbits(8) for _ in range(12)) for _ in range(count))

    # Half-node layout: box at +0, faces word at +0x18, child at +0x1c. The second half of a node is at +0x20.
    root_node = bytearray(0x40)
    struct.pack_into("<6f", root_node, 0, *box())
    struct.pack_into("<I", root_node, 0x1C, child)
    if rng.random() < 0.5:
        # A leaf root: its faces are tested directly (the children's box tests are skipped).
        root_count = rng.choice((1, 2, 3))
        struct.pack_into("<I", root_node, 0x18, root_count)
        struct.pack_into("<I", root_node, 0x1C, faces0)
        root_faces = faces(root_count)
    else:
        root_faces = b""
    child_node = bytearray(0x40)
    count0, count1 = rng.choice((1, 2, 3)), rng.choice((0, 1, 2, 3))
    struct.pack_into("<6f", child_node, 0, *box())
    struct.pack_into("<6f", child_node, 0x20, *box())
    for half, count, faces_at in ((0, count0, faces0), (0x20, count1, faces1)):
        # A leaf half has no child: its face list is at +0x1c (+0x3c for the second half), with the count at +0x18 (+0x38).
        struct.pack_into("<I", child_node, half + 0x18, count | (0x200000 if rng.random() < 0.2 else 0))
        struct.pack_into("<I", child_node, half + 0x1C, faces_at if count else 0)
    writes = [
        (system, struct.pack("<4I", 0, 0, root, table)),
        (table, b"".join(struct.pack("<3f", *v) for v in verts)),
        (origin, struct.pack("<3f", sx, sy, sz)),
        (end, struct.pack("<3f", sx, sy, ez)),
        (out, bytes(0x40)),
        (root, bytes(root_node)),
        (child, bytes(child_node)),
        (faces0, faces(count0) if not root_faces else root_faces),
        (faces1, faces(count1)),
    ]
    return [system, origin, end, out], writes, system


SPECS = {
    # args: this (the system), the origin and end points (16-byte reads), the record (15 words: a 0x40 window).
    0x4FE810: dict(name="CCollisionSystem::Vf00", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x40)],
                   nodes=_system_nodes, direct_fuzz=_system_direct_fuzz),
    # E38Ops::M_DE40X (thiscall on the system, four stack words): the origin, the direction, the root and the hit record
    # (its +0x20 holds the ray parameter, +0x0c..+0x28 the normal, the face and the barycentric sums).
    0x4FDE40: dict(name="E38Ops::M_DE40X", popped=16, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (4, lambda a: 0x30)]),
    # The sample table lookup (thiscall on the table, three stack words: the byte, low and high sums). Writes nothing.
    0x520240: dict(name="FUN_00520240", popped=12, x87=False, thiscall=True, external=True, windows=[]),
}
