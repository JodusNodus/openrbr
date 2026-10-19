"""The E39 queries CollideInit gates and falls back on: E39_SegTest70 (0x4ff070),
E38Ops::M_EBC0 (0x4febc0) and its face test E38Ops::M_E2D0 (0x4fe2d0), directly on faces laid out in the stub heap, so
the hit paths no drive reaches are recorded. The records replay in collision_vectors.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-e39 --addr ADDR --count N --seed SEED
"""

SET_NAME = "g-e39"

import struct  # noqa: E402


def _u32(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def _vertex(rng, xy):
    return (xy[0] + rng.uniform(-1.0, 1.0), xy[1] + rng.uniform(-1.0, 1.0), rng.uniform(-0.05, 0.05))


def _point(rng, xy):
    """A point above or below the plane z = 0 by up to 0.6 (the hit window is 1e-5 to 0.5 behind the face)."""
    z = rng.choice((rng.uniform(0.0, 0.6), rng.uniform(0.0, 0.02), 0.0, 0.7))
    return (xy[0], xy[1], z * rng.choice((-1.0, 1.0)))


def _face_query_nodes(mu, a):
    """M_EBC0: the list (the collider's own when the argument is null), its records, their faces and vertex blocks."""
    this = a[0]
    list_at = a[3] or this + 0x1014
    count = min(_u32(mu, list_at), 16)
    windows = [(list_at, 4 + 4 * count)]
    for index in range(count):
        record = _u32(mu, list_at + 4 + 4 * index)
        windows.append((record, 0x20))
        word = _u32(mu, record + 0x18)
        faces = word & 0x1FFFFF
        if faces:
            windows.append((_u32(mu, record + 0x1C), 0x12 * faces))
        base = _u32(mu, this + 0xC + 4 * (word >> 22))
        if base:
            windows.append((base, 12 * 6))
    return windows


def _face_query_direct_fuzz(_mu, heap, rng):
    # A collider with one vertex block (table entry 1, at +0x10; entry 0 empty) and up to three records of up to three
    # faces round the point's xy, either winding. The list is the collider's own (+0x1014) or an explicit one.
    this, base = heap + 0x10000, heap + 0x20000
    point_at, out = heap + 0x50000, heap + 0x50100
    xy = [rng.uniform(-1.0, 1.0) for _ in range(2)]
    point = _point(rng, xy)
    writes = [(this, bytes(0x1040)), (point_at, struct.pack("<3f", *point)), (out, bytes(0x40))]
    verts = [_vertex(rng, xy) for _ in range(6)]
    writes.append((base, b"".join(struct.pack("<3f", *v) for v in verts)))
    writes.append((this + 0x10, struct.pack("<I", base)))
    records = []
    for index in range(rng.randrange(1, 4)):
        record, faces = heap + 0x30000 + 0x40 * index, heap + 0x40000 + 0x100 * index
        count = rng.choice((0, 1, 2, 3, 3))
        table = rng.choice((1, 1, 1, 0))  # entry 0 is empty: the record is skipped
        writes.append((record, bytes(0x18) + struct.pack("<II", (table << 22) | count, faces)))
        face_bytes = b""
        for _ in range(count):
            corners = [rng.randrange(6) for _ in range(3)]
            face_bytes += struct.pack("<3H2H", *corners, rng.randrange(0x10000), rng.randrange(0x10000))
            face_bytes += bytes(rng.randrange(256) for _ in range(8))
        writes.append((faces, face_bytes))
        records.append(record)
    explicit = rng.random() < 0.5
    list_at = heap + 0x28000 if explicit else this + 0x1014
    writes.append((list_at, struct.pack("<I", len(records)) + b"".join(struct.pack("<I", r) for r in records)))
    return [point_at, out, list_at if explicit else 0], writes, this


def _point_triangle_direct_fuzz(_mu, heap, rng):
    # One face round the point's xy, either winding; now and then a degenerate face or a NaN corner.
    point_at, out = heap + 0x50000, heap + 0x50100
    corners = [heap + 0x51000 + 0x10 * k for k in range(3)]
    xy = [rng.uniform(-1.0, 1.0) for _ in range(2)]
    verts = [_vertex(rng, xy) for _ in range(3)]
    mode = rng.randrange(20)
    if mode == 0:
        verts[2] = verts[1]
    elif mode == 1:
        verts[0] = (float("nan"),) + verts[0][1:]
    writes = [(point_at, struct.pack("<3f", *_point(rng, xy))), (out, bytes(0x2C))]
    writes += [(at, struct.pack("<3f", *v)) for at, v in zip(corners, verts)]
    return [point_at, *corners, out], writes, heap + 0x10000


def _segment_test_direct_fuzz(_mu, heap, rng):
    # The collider's tree: a root without faces over a leaf pair (the second of which is hidden now and then), boxes
    # round the origin; the segment's ends within two units of it. The output is the collider's own or an explicit word.
    this, root, child, a, b, out = (heap + 0x10000, heap + 0x20000, heap + 0x20040, heap + 0x50000, heap + 0x50010,
                                    heap + 0x50100)

    def box():
        return [rng.uniform(-1.5, 1.5) for _ in range(3)] + [rng.uniform(0.1, 1.0) for _ in range(3)]

    pair = bytearray(0x40)
    struct.pack_into("<6fI", pair, 0, *box(), rng.choice((1, 2)))
    struct.pack_into("<6fI", pair, 0x20, *box(), rng.choice((1, 1, 0x200001)))
    writes = [(this, bytes(0x1020)), (this + 8, struct.pack("<I", root)),
              (root, struct.pack("<6fII", *box(), 0, child) + bytes(0x20)), (child, bytes(pair)),
              (a, struct.pack("<3f", *[rng.uniform(-2.0, 2.0) for _ in range(3)])),
              (b, struct.pack("<3f", *[rng.uniform(-2.0, 2.0) for _ in range(3)])), (out, bytes(0x10))]
    return [a, b, out if rng.random() < 0.5 else 0], writes, this


SPECS = {
    0x4FEBC0: dict(name="E38Ops::M_EBC0", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x1040), (1, lambda a: 0x10), (2, lambda a: 0x40)],
                   nodes=_face_query_nodes, direct_fuzz=_face_query_direct_fuzz),
    0x4FE2D0: dict(name="E38Ops::M_E2D0", popped=20, x87=False, thiscall=True, direct=True,
                   windows=[(1, lambda a: 12), (2, lambda a: 12), (3, lambda a: 12), (4, lambda a: 12),
                            (5, lambda a: 0x2C)],
                   direct_fuzz=_point_triangle_direct_fuzz),
    0x4FF070: dict(name="E39_SegTest70", popped=12, x87=False, thiscall=True, direct=True,
                   windows=[(0, lambda a: 0x1020), (1, lambda a: 12), (2, lambda a: 12), (3, lambda a: 4)],
                   nodes=lambda mu, a: [(a[0] + 8, 4), (_u32(mu, a[0] + 8), 0x80)],
                   direct_fuzz=_segment_test_direct_fuzz),
    # The material table lookups M_EBC0 makes on a hit (thiscall on the table, three stack words).
    0x520240: dict(name="FUN_00520240", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x808)]),
}
