"""Direct-call records for the fence segment array (G-e90seg): E90Seg::E90Seg (0x600ff0), E90Seg::~E90Seg (0x601030)
and E90SegPool::Init (0x601270) on its owned-array path (operator new, the eh vector constructor and destructor
iterators, operator delete), with pools from an earlier init: none, an external block, or an owned array whose
segments own storage of their own. The recipes build the pool and the segments in the replay's stub heap.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-e90seg --count N --seed SEED
"""

import struct

SET_NAME = "g-e90seg"

SEG = 0x78
ARRAY, BLOCK, STORAGE = 0x3000, 0x8000, 0xA000  # heap offsets: the earlier owned array, an external block, storage


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def u32bytes(x):
    return struct.pack("<I", x & 0xFFFFFFFF)


def segment_image(heap, rng, k):
    """A segment's 0x78 bytes: random words, each of its five pools external, empty or owning storage."""
    image = bytearray(rng.getrandbits(8) for _ in range(SEG))
    for n, pool in enumerate((0x00, 0x20, 0x30, 0x40)):
        kind = rng.choice(("owned", "owned", "external", "empty"))
        struct.pack_into("<I", image, pool + 0xC, 0 if kind == "empty" else heap + STORAGE + 0x400 * (4 * k + n))
        image[pool + 8] = 1 if kind == "external" else 0
    return bytes(image)


def fuzz_ctor(mu, heap, rng):
    seg = heap + 0x1000
    return [], [(seg, bytes(rng.getrandbits(8) for _ in range(SEG)))], seg


def fuzz_dtor(mu, heap, rng):
    seg = heap + 0x1000
    return [], [(seg, segment_image(heap, rng, 0))], seg


def fuzz_pool_init(mu, heap, rng):
    """The pool's earlier state (an owned array of 1..3 segments, an external block, or nothing), then 0..4 segments
    from an external block or an owned array."""
    pool = heap + 0x800
    writes = [(pool, bytes(0x10))]
    before = rng.choice(("owned", "owned", "external", "none"))
    if before == "owned":
        k = rng.randrange(1, 4)
        array = heap + ARRAY
        writes.append((array, u32bytes(k) + b"".join(segment_image(heap, rng, i) for i in range(k))))
        writes.append((pool, u32bytes(rng.getrandbits(32)) + u32bytes(k) + bytes([0, 0, 0, 0]) + u32bytes(array + 4)))
    elif before == "external":
        writes.append((pool, u32bytes(2) + u32bytes(2) + bytes([1, 0, 0, 0]) + u32bytes(heap + BLOCK + 0x800)))
    count = rng.randrange(0, 5)
    block = heap + BLOCK if rng.random() < 0.3 else 0
    if block:
        writes.append((block, bytes(SEG * 4)))
    return [count, block], writes, pool


def pool_nodes(mu, a):
    """The earlier owned array (its count word and segments), when the pool has one."""
    pool = a[0]
    data = word(mu, pool + 0xC)
    if mu.mem_read(pool + 8, 1)[0] or not data:
        return []
    return [(data - 4, 4 + SEG * word(mu, data - 4))]


def pool_after(mu, a, eax):
    """The owned array the call made: its count word and the constructed segments."""
    pool, count, block = a
    data = word(mu, pool + 0xC)
    if block or not count or not data:
        return []
    return [(data - 4, 4 + SEG * count)]


def ext(name, popped, cdecl=False):
    return dict(name=name, popped=popped, x87=False, thiscall=not cdecl, cdecl=cdecl, external=True, windows=[])


SPECS = {
    0x600FF0: dict(name="E90Seg::E90Seg", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_ctor,
                   windows=[(0, lambda a: SEG)]),
    0x601030: dict(name="E90Seg::~E90Seg", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_dtor,
                   windows=[(0, lambda a: SEG)]),
    0x601270: dict(name="E90SegPool::Init", popped=8, x87=False, thiscall=True, direct_fuzz=fuzz_pool_init,
                   windows=[(0, lambda a: 0x10), (2, lambda a: SEG * a[1] if a[2] else 0)], nodes=pool_nodes,
                   after=pool_after),
    0x70BEC2: ext("operator new (CRT)", 4, cdecl=True),
    0x70C260: ext("operator delete (CRT)", 4, cdecl=True),
    0x70C408: dict(ext("eh vector constructor iterator", 20), thiscall=False),
    0x70C4DA: dict(ext("eh vector destructor iterator", 16), thiscall=False),
}
