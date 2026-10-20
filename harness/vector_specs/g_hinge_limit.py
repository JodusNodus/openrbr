"""Direct-call records for CHingedRigidBody::SimCF10(idx) (0x51cf10) at any limit index (G-hinge-limit). Its only caller,
SimEA80, passes 0 and 1; the asm takes any index (the state word at +0x3e8 + 4 idx, the definition's block at
+0x1f0 + 0x60 idx). The recipe builds the body and its definition in the replay's stub heap, with indices 0..4 and
angles that reach each branch (above and below the limit, clamped to the step, settled, a zero correction).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-hinge-limit --count N --seed SEED
"""

import struct

SET_NAME = "g-hinge-limit"

BODY, DEF = 0x1000, 0x2000  # heap offsets
INDICES = 5


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def fuzz_cf10(mu, heap, rng):
    body, definition = heap + BODY, heap + DEF
    idx = rng.choice((2, 3, 4, rng.randrange(INDICES)))
    image = bytearray(rng.getrandbits(8) for _ in range(0x410))
    angle = rng.uniform(-1.0, 1.0)
    struct.pack_into("<I", image, 0x34, definition)
    struct.pack_into("<f", image, 0x3FC, angle)
    struct.pack_into("<f", image, 0x40C, rng.uniform(-5.0, 5.0))
    blocks = bytearray(rng.getrandbits(8) for _ in range(0x1F0 + 0x60 * INDICES))
    for k in range(INDICES):
        block = 0x1F0 + 0x60 * k
        ref = rng.uniform(0.0, 0.6)
        struct.pack_into("<f", blocks, block + 0x2C, rng.uniform(0.0, 2000.0))
        struct.pack_into("<f", blocks, block + 0x3C, rng.uniform(0.0, 50.0))
        struct.pack_into("<f", blocks, block + 0x4C, rng.choice((0.01, 0.05, 1.0)))
        struct.pack_into("<f", blocks, block + 0x5C, ref)
        # The state: inside, past either side by a little or a lot, or exactly at the limit (a zero correction).
        state = rng.choice((angle + ref + rng.uniform(-0.2, 0.2), angle - ref + rng.uniform(-0.2, 0.2),
                            angle + rng.uniform(-0.1, 0.1), angle + ref))
        struct.pack_into("<f", image, 0x3E8 + 4 * k, state)
    return [idx], [(body, bytes(image)), (definition, bytes(blocks))], body


def def_nodes(mu, a):
    return [(word(mu, a[0] + 0x34) + 0x1F0, 0x60 * INDICES)]


SPECS = {
    0x51CF10: dict(name="CHingedRigidBody::SimCF10", popped=4, x87=True, thiscall=True, windows=[(0, lambda a: 0x410)],
                   nodes=def_nodes, direct_fuzz=fuzz_cf10),
}
