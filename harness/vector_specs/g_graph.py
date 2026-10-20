"""Direct-call records for the debug graphs' pushes (G-graph): CGraph1D::Push (0x494cf0) and CGraph2D::Push (0x495090).
The recipe builds a graph in the replay's stub heap: a period of 1..5 (0 faults in the original's div), a push counter
below or past it, and a ring of 1..6 samples with its cursor anywhere in it (the last slot wraps).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-graph --count N --seed SEED
"""

import struct

SET_NAME = "g-graph"

GRAPH, RING, SAMPLES = 0x1000, 0x1100, 0x2000  # heap offsets


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def fuzz(stride, counter_at, words):
    def recipe(mu, heap, rng):
        graph, ring, samples = heap + GRAPH, heap + RING, heap + SAMPLES
        period = rng.choice((1, 2, 3, 5))
        counter = rng.choice((period - 1, rng.randrange(0, 2 * period + 2), rng.randrange(0, 2 * period + 2), 0xFFFFFFFF))
        count = rng.randrange(1, 7)
        cursor = samples + stride * rng.choice((count - 1, rng.randrange(count)))
        image = bytearray(rng.getrandbits(8) for _ in range(0x48))
        struct.pack_into("<II", image, 0x14, period, ring)
        struct.pack_into("<I", image, counter_at, counter)
        writes = [(graph, bytes(image)), (ring, struct.pack("<3I", samples, count, cursor)),
                  (samples, bytes(rng.getrandbits(8) for _ in range(stride * count)))]
        return [rng.getrandbits(32) for _ in range(words)], writes, graph
    return recipe


def nodes(stride):
    def ring_nodes(mu, a):
        ring = word(mu, a[0] + 0x18)
        return [(ring, 0xC), (word(mu, ring), stride * word(mu, ring + 4))]
    return ring_nodes


SPECS = {
    0x494CF0: dict(name="CGraph1D::Push", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz(4, 0x44, 1),
                   windows=[(0, lambda a: 0x48)], nodes=nodes(4)),
    0x495090: dict(name="CGraph2D::Push", popped=8, x87=False, thiscall=True, direct_fuzz=fuzz(8, 0x40, 2),
                   windows=[(0, lambda a: 0x48)], nodes=nodes(8)),
}
