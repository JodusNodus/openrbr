"""The EdgePair group (lead/reports/g-edgepair.md): E37GridOps::EdgePair (0x4f8560) and its callees Walk5FF0 (0x4f5ff0),
M74A0 (0x4f74a0) and Store5570 (0x4f5570), called directly on fuzzed grids and segments. The callees are recorded as
the inner calls of each EdgePair run too. Branches the impact snapshots do not reach: the no-hit returns, NaN lanes,
the n == 0 and sampled-surface paths, and M74A0's sign flip.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-edgepair --count N --seed SEED
"""

SET_NAME = "g-edgepair"

import struct  # noqa: E402

EDGE_PAIR = 0x4F8560
WALK_5FF0 = 0x4F5FF0
M74A0 = 0x4F74A0
STORE_5570 = 0x4F5570

GRID = 0x800  # the window of a grid: the rows (+0x484, six bytes each) and the fields up to +0x690 + the slots
NAN = 0x7FC00000


def _float_bytes(rng, nan):
    if rng.random() < nan:
        return struct.pack("<I", NAN)
    return struct.pack("<f", rng.uniform(-1.5, 1.5))


def _vec(rng, nan=0.0):
    return b"".join(_float_bytes(rng, nan) for _ in range(4))


def _seg_vec(rng, xyz):
    """A segment end: the point's three lanes and a random w."""
    return struct.pack("<3f", *xyz) + _float_bytes(rng, 0.0)


def _grid_writes(rng, heap, nan):
    """A grid at heap+0x10000 with up to 0x50 rows over 2-40 vertices, and an optional sample atlas (70%): the pair table
    at heap+0x12000, the two byte tables at heap+0x13000 and heap+0x13400, with the grid's pointers and sizes. With an
    atlas the vertices are finite and each row's two slots differ: a NaN or a zero-length edge gives a sample index
    outside the atlas, which the original reads as a fault."""
    grid, data, table_a, table_b = heap + 0x10000, heap + 0x12000, heap + 0x13000, heap + 0x13400
    atlas = rng.random() < 0.7
    block = bytearray(rng.randrange(256) for _ in range(GRID))
    slots = rng.choice([2, 3, 8, 24, 40])
    for slot in range(slots):
        block[0x40 + 16 * slot:0x50 + 16 * slot] = _vec(rng, 0.0 if atlas else nan)
    rows = rng.choice([0, 1, 2, 3, 5, 8, rng.randrange(1, 0x50)])
    block[0x68C] = rows
    for index in range(rows):
        first = rng.randrange(slots)
        second = rng.randrange(slots - 1) if atlas else rng.randrange(slots)
        block[0x484 + 6 * index] = first
        block[0x485 + 6 * index] = second + (atlas and second >= first)
    for slot in range(slots):
        block[0x690 + slot] = rng.randrange(8)
    block[0x400:0x404] = struct.pack("<I", data if atlas else 0)
    if atlas:
        block[0x404:0x406] = struct.pack("<H", rng.randrange(1, 17))
        block[0x406:0x408] = struct.pack("<H", rng.randrange(1, 17))
        block[0x408:0x40C] = struct.pack("<I", table_a)
        block[0x40C:0x410] = struct.pack("<I", table_b)
    writes = [(grid, bytes(block)), (data, bytes(rng.randrange(256) for _ in range(16))),
              (table_a, bytes(rng.randrange(256) for _ in range(0x400))),
              (table_b, bytes(rng.randrange(256) for _ in range(0x400)))]
    edges = [(struct.unpack_from("<3f", block, 0x40 + 16 * block[0x484 + 6 * k]),
              struct.unpack_from("<3f", block, 0x40 + 16 * block[0x485 + 6 * k])) for k in range(rows)]
    return grid, writes, edges


def _segments_near_edge(rng, edges):
    """Four points for two segments (p2-p3 and p4-p5) that pass near a row's edge, so the walk's gates often pass.
    Without a row, random points."""
    if not edges or rng.random() < 0.2:
        return [tuple(rng.uniform(-1.5, 1.5) for _ in range(3)) for _ in range(4)]
    (ax, ay, az), (bx, by, bz) = edges[rng.randrange(len(edges))]
    base = []
    for _ in range(4):
        s = rng.uniform(-0.2, 1.2)
        base.append(tuple(a + s * (b - a) + rng.gauss(0, 0.3) for a, b in zip((ax, ay, az), (bx, by, bz))))
    return base


def _sample_nodes(mu, args):
    """The windows EdgePair reads through the grid's sample pointers: the pair table, and the two byte tables with room
    for indices outside 0x400 (a ratio beyond 0..1 moves them)."""
    grid = args[0]
    word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    if not word(grid + 0x400):
        return []
    windows = [(word(grid + 0x400), 0x200)]
    for at in (word(grid + 0x408), word(grid + 0x40C)):
        windows.append((at - 0x800, 0x1000))
    return windows


def _edge_pair_direct_fuzz(_mu, heap, rng):
    # The grid, the two segments (p2-p3, p4-p5), the outputs (hit, t, normal) and the scratch word, with the grid again
    # and 0 (the original's ten words). Segments are random, so some pairs straddle the frame and some do not.
    grid, writes, edges = _grid_writes(rng, heap, 0.03)
    seg = heap + 0x15000
    points = [seg + 0x10 * k for k in range(4)]
    writes += [(p, _seg_vec(rng, c)) for p, c in zip(points, _segments_near_edge(rng, edges))]
    hit, t, normal, scratch = heap + 0x16000, heap + 0x16010, heap + 0x16020, heap + 0x16030
    words = points + [hit, t, normal, scratch, grid, 0]
    return words, writes, grid


def _walk_direct_fuzz(_mu, heap, rng):
    grid, writes, edges = _grid_writes(rng, heap, 0.03)
    seg = heap + 0x15000
    points = [seg + 0x10 * k for k in range(4)]
    writes += [(p, _seg_vec(rng, c)) for p, c in zip(points, _segments_near_edge(rng, edges))]
    index = rng.randrange(0x50)
    outs = [heap + 0x16000 + 0x10 * k for k in range(3)]
    return [index] + points + outs, writes, grid


def _m74a0_direct_fuzz(_mu, heap, rng):
    grid, writes, _edges = _grid_writes(rng, heap, 0.03)
    seg = heap + 0x15000
    points = [seg, seg + 0x10]
    writes += [(p, _vec(rng, 0.03)) for p in points]
    index = rng.randrange(0x50)
    return [index] + points + [heap + 0x16000], writes, grid


def _store_direct_fuzz(_mu, heap, rng):
    this, seg = heap + 0x15000, heap + 0x15010
    points = [seg + 0x10 * k for k in range(3)]
    writes = [(p, _vec(rng, 0.03)) for p in [this] + points]
    return points, writes, this


SPECS = {
    # The ten stack words are (p2, p3, p4, p5, hit, t, normal, scratch, grid, 0). The windows index this and the stack
    # words: 1-4 the segments, 5 the hit, 6 t, 7 the normal. The grid's sample windows are read at entry (nodes).
    EDGE_PAIR: dict(name="E37GridOps::EdgePair", popped=0x28, x87=False, thiscall=True, direct=True,
                    windows=[(0, lambda a: GRID), (1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10), (4, lambda a: 0x10), (5, lambda a: 0x10), (6, lambda a: 4), (7, lambda a: 0x10)],
                    nodes=_sample_nodes, direct_fuzz=_edge_pair_direct_fuzz),
    WALK_5FF0: dict(name="E37GridOps::Walk5FF0", popped=0x20, x87=True, thiscall=True, direct=True,
                    windows=[(0, lambda a: GRID), (2, lambda a: 0x10), (3, lambda a: 0x10), (4, lambda a: 0x10), (5, lambda a: 0x10), (6, lambda a: 0x10), (7, lambda a: 0x10), (8, lambda a: 0x10)],
                    direct_fuzz=_walk_direct_fuzz),
    M74A0: dict(name="E37GridOps::M74A0", popped=0x10, x87=True, thiscall=True, direct=True,
                windows=[(0, lambda a: GRID), (2, lambda a: 0x10), (3, lambda a: 0x10), (4, lambda a: 0x10)], direct_fuzz=_m74a0_direct_fuzz),
    STORE_5570: dict(name="E37VecSet::Store5570", popped=0xC, x87=True, thiscall=True, direct=True,
                     windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10)], direct_fuzz=_store_direct_fuzz),
}
