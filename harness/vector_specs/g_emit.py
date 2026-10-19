"""CMultiSignal1<CSigCheckPointTime>::Emit (0x4a0390, thiscall on the signal, one payload argument): the checkpoint signal
CTimeKeeper::Record emits at a checkpoint (lead/reports/g-emits.md, "Not done"). Its compares: the guard word (+0, must be
0), the connection table (+4, its first word is the slot count, which must be below 0x20), the slot the count names (the
list head at +8+16*count) and the secondary list (+0x208). Each list node is an object (its first word is the vtable, its
second the next node) and the receiver is the vtable's slot 1, which is a `ret 4` (0x573616, the listener's own return).
The direct recipe builds the table, the lists (each 0..2 nodes), the count and the payload, so the four outcomes (table
empty, the count's slot empty or not, the secondary list empty or not) come up. The records replay in physics_core_vectors
(`emit_checkpoint_matches`).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-emit --addr 0x4A0390 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs words)

SET_NAME = "g-emit"

SIGNAL, TABLE, VTABLE, NODES, PAYLOAD = 0xD0000, 0xD1000, 0xD1100, 0xD1200, 0xD2000  # heap offsets from the signal
RET4 = 0x573616  # a `retl $4` (the listener at 0x5735b0's own return)
NODE_SLOTS = 0x80  # 8-byte nodes in the node region


def _emit_direct(mu, heap, rng):
    del mu
    signal = heap + SIGNAL
    table, vtable, nodes = heap + SIGNAL + 0x1000, heap + SIGNAL + 0x1100, heap + SIGNAL + 0x1200
    image = bytearray(0x220)
    writes = []
    if rng.random() < 0.2:
        struct.pack_into("<I", image, 4, 0)  # no connection table: the emit returns at once
    else:
        count = rng.randrange(0x20)
        struct.pack_into("<I", image, 4, table)
        writes.append((table, struct.pack("<I", count)))
        free = iter(range(NODE_SLOTS))

        def chain(length):
            """Nodes (vtable, next) in the node region; returns the head."""
            head = 0
            for _ in range(length):
                at = nodes + 8 * next(free)
                writes.append((at, struct.pack("<2I", vtable, head)))
                head = at
            return head

        for k in range(0x20):
            struct.pack_into("<I", image, 8 + 16 * k, chain(rng.choice((0, 0, 1, 2))))
        struct.pack_into("<I", image, 0x208, chain(rng.choice((0, 1, 2))))
    writes.append((signal, bytes(image)))
    writes.append((vtable, struct.pack("<4I", 0, RET4, 0, 0)))
    writes.append((heap + PAYLOAD, bytes(rng.getrandbits(8) for _ in range(0x10))))
    return [heap + PAYLOAD], writes, signal


def _emit_nodes(mu, a):
    """The connection table, the vtable and the node region, as the emit reads them (windows at entry)."""
    base = a[0]
    return [(base + 0x1000, 8), (base + 0x1100, 0x10), (base + 0x1200, 8 * NODE_SLOTS)]


SPECS = {
    0x4A0390: dict(name="CMultiSignal1<CSigCheckPointTime>::Emit", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x220), (1, lambda a: 0x10)], nodes=_emit_nodes,
                   dispatch=lambda mu, a: [(RET4, 4)], direct_fuzz=_emit_direct),
}
