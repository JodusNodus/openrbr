# G-de40: E38Ops::M_DE40X (0x4fde40), the mesh ray over the collision tree that
# CCollisionSystem::Vf00 (0x4fe810) calls. Thiscall on the system, four stack words: the origin, the direction (not the
# end point: M_DE40X takes the half direction and the direction itself), the root and the hit record. The direct recipe
# builds a small tree in the stub heap (a root, a child with two halves, faces per half, two vertex tables at this+0xc
# and this+0x10) and a segment that crosses the faces' plane near the origin's xy, so about half the calls hit.
SET_NAME = "g-de40"

import struct  # noqa: E402 (the recipe packs floats)


def _mesh_nodes(mu, a):
    """Reads of M_DE40X: the system's table pointers (+0xc and +0x10), the two tables (6 vertices), and the tree below
    the root argument (a half is a box, a face count word and a child at +0x1c; the walk follows the children that the
    worklist tests, the same as the system recipe's)."""
    system = a[0]
    regions = [(system, 0x14)]
    for slot in (0xC, 0x10):
        regions.append((word(mu, system + slot), 12 * 6))
    todo = [a[3]]
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


def _mesh_direct_fuzz(_mu, heap, rng):
    system, table0, table1 = heap + 0x10000, heap + 0x20000, heap + 0x21000
    root, child = heap + 0x30000, heap + 0x30040
    faces0, faces1 = heap + 0x40000, heap + 0x41000
    origin, direction, out = heap + 0x50000, heap + 0x50010, heap + 0x50100
    sx, sy = rng.uniform(-1.0, 1.0), rng.uniform(-1.0, 1.0)
    sz = rng.choice((-1.0, 1.0))
    direction_xyz = (rng.uniform(-0.5, 0.5), rng.uniform(-0.5, 0.5), -sz * rng.choice((0.5, 1.0, 1.5)))
    tables = [[(sx + rng.uniform(-1.5, 1.5), sy + rng.uniform(-1.5, 1.5), rng.uniform(-0.2, 0.2)) for _ in range(6)]
              for _ in range(2)]
    # The hit record's ray parameter starts at the value the caller pre-seeds (1.1), or at a random one below it.
    pre = 1.1 if rng.random() < 0.5 else rng.uniform(0.0, 1.5)

    def box():
        return [sx + rng.uniform(-0.5, 0.5), sy + rng.uniform(-0.5, 0.5), rng.uniform(-0.5, 0.5)] + [
            rng.uniform(1.0, 3.0) for _ in range(3)]

    def faces(count):
        # A face is three vertex indices (u16) and 12 bytes of tags and barycentric nibbles: 0x12 bytes.
        return b"".join(struct.pack("<3H", *(rng.randrange(6) for _ in range(3))) + bytes(
            rng.getrandbits(8) for _ in range(12)) for _ in range(count))

    def word_of(count, index):
        # A face count word with its table index in bits 22 and up, and sometimes the skip flag (bit 21).
        return count | (index << 22) | (0x200000 if rng.random() < 0.2 else 0)

    # Half-node layout: box at +0, faces word at +0x18, child at +0x1c. The second half of a node is at +0x20.
    root_node = bytearray(0x40)
    struct.pack_into("<6f", root_node, 0, *box())
    struct.pack_into("<I", root_node, 0x1C, child)
    root_faces = b""
    if rng.random() < 0.5:
        # A leaf root: its faces are tested directly (the children's box tests are skipped).
        root_count = rng.choice((1, 2, 3))
        struct.pack_into("<I", root_node, 0x18, word_of(root_count, rng.randrange(2)) & ~0x200000)
        struct.pack_into("<I", root_node, 0x1C, faces0)
        root_faces = faces(root_count)
    child_node = bytearray(0x40)
    count0, count1 = rng.choice((1, 2, 3)), rng.choice((0, 1, 2, 3))
    struct.pack_into("<6f", child_node, 0, *box())
    struct.pack_into("<6f", child_node, 0x20, *box())
    for half, count, faces_at in ((0, count0, faces0), (0x20, count1, faces1)):
        # A leaf half has no child: its face list is at +0x1c (+0x3c for the second half), with the count at +0x18 (+0x38).
        struct.pack_into("<I", child_node, half + 0x18, word_of(count, rng.randrange(2)))
        struct.pack_into("<I", child_node, half + 0x1C, faces_at if count else 0)
    writes = [
        (system, struct.pack("<4I", 0, 0, root, table0) + struct.pack("<I", table1)),
        (table0, b"".join(struct.pack("<3f", *v) for v in tables[0])),
        (table1, b"".join(struct.pack("<3f", *v) for v in tables[1])),
        (origin, struct.pack("<3f", sx, sy, sz)),
        (direction, struct.pack("<3f", *direction_xyz)),
        (out, struct.pack("<8I", *([0] * 8)) + struct.pack("<f", pre) + bytes(0x10)),
        (root, bytes(root_node)),
        (child, bytes(child_node)),
        (faces0, faces(count0) if not root_faces else root_faces),
        (faces1, faces(count1)),
    ]
    return [origin, direction, root, out], writes, system


SPECS = {
    # args: this (the system), the origin and the direction (12-byte reads), the root (its tree is in `nodes`) and the hit
    # record (0x30 bytes: the normal at +0x0c, the ray parameter at +0x20, the face at +0x18 and the table at +0x1c).
    0x4FDE40: dict(name="E38Ops::M_DE40X", popped=16, x87=False, thiscall=True, direct=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (4, lambda a: 0x30)],
                   nodes=_mesh_nodes, direct_fuzz=_mesh_direct_fuzz),
}
