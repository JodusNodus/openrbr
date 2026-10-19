"""CColGrid::SampleB (0x4e0950, thiscall, the index and two output pointers): the sampled surface's point at the byte
+0x690+index, unblended. Its one compare is the grid's data pointer (+0x400): zero gives (0, 0) at once, which no record
has: the store's SampleB records are the inner calls of other functions (439), and none is a direct call. The direct recipe
draws the data pointer (zero, about 40% of the calls), the grid's size words and tables, and the index, so both paths come
up. (The unit test covers the empty grid too.)

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-colgrid --addr 0x4E0950 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs words)

SET_NAME = "g-colgrid"

GRID, DATA, TABLE_A, TABLE_B, OUT_1, OUT_2 = 0xB0000, 0xB1000, 0xB3000, 0xB4000, 0xB5000, 0xB5010  # heap offsets
GRID_SIZE = 0x6B0


def _sample_b_direct(mu, heap, rng):
    del mu
    grid, data, table_a, table_b = (heap + at for at in (GRID, DATA, TABLE_A, TABLE_B))
    has_data = rng.random() < 0.6
    grid_image = bytearray(GRID_SIZE)
    struct.pack_into("<I", grid_image, 0x400, data if has_data else 0)
    struct.pack_into("<2H", grid_image, 0x404, rng.randrange(1, 0x40), rng.randrange(1, 0x40))
    struct.pack_into("<2I", grid_image, 0x408, table_a, table_b)
    grid_image[0x690:0x6B0] = bytes(rng.getrandbits(8) for _ in range(0x20))
    index = rng.randrange(0x20)
    writes = [(grid, bytes(grid_image)), (heap + OUT_1, bytes(0x10)), (heap + OUT_2, bytes(0x10))]
    if has_data:
        writes += [(data, bytes(rng.getrandbits(8) for _ in range(0x2000))),
                   (table_a, bytes(rng.getrandbits(8) for _ in range(0x1000))),
                   (table_b, bytes(rng.getrandbits(8) for _ in range(0x1000)))]
    return [index, heap + OUT_1, heap + OUT_2], writes, grid


def _sample_b_nodes(mu, a):
    """The surface's data and its two tables (read on the data path only)."""
    if not word(mu, a[0] + 0x400):  # noqa: F821 (vcu_vectors' word)
        return []
    return [(word(mu, a[0] + 0x400), 0x2000), (word(mu, a[0] + 0x408), 0x1000),  # noqa: F821
            (word(mu, a[0] + 0x40C), 0x1000)]  # noqa: F821


SPECS = {
    0x4E0950: dict(name="CColGrid::SampleB", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: GRID_SIZE), (2, lambda a: 0x10), (3, lambda a: 0x10)],
                   nodes=_sample_b_nodes, direct_fuzz=_sample_b_direct),
}
