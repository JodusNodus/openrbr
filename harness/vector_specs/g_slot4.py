"""Direct-call records for E36DamageOps::Slot4 (0x4f1000) on the snapshot car's damage ops (G-slot4): every damage
level the asm tells apart (0 and out of range take zeros, 1, 2 and 3 their fractions), and zone damage values below
0, above 1, inside and NaN for the clamp. InitTables and its AddFunctor calls run inside; the two emits are kept as
inner records.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-slot4 --count N --seed SEED   (a snapshot with damage on)
"""

import struct

SET_NAME = "g-slot4"

CAR_GLOBAL = 0x008EF660
OPS_SIZE = 0x3EC0


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def slot4_direct(mu, heap, rng):
    ops = word(mu, word(mu, CAR_GLOBAL) + 0x3CC)
    level = rng.choice((0, 1, 2, 3, 4, 0xFFFFFFFF))
    writes = [(ops + 0x3DF0, struct.pack("<I", level))]
    for zone in range(0x40):
        value = rng.choice((rng.uniform(-0.5, 0.0), rng.uniform(0.0, 1.0), rng.uniform(1.0, 3.0), float("nan"), -0.0))
        writes.append((ops + 0x2214 + 0x58 * zone, struct.pack("<f", value)))
    return [], writes, ops


SPECS = {
    0x4F1000: dict(name="E36DamageOps::Slot4", popped=0, x87=False, thiscall=True, direct_fuzz=slot4_direct,
                   windows=[(0, lambda a: OPS_SIZE)]),
    0x4EF350: dict(name="CMultiSignal1<CSigCarDamageHandlerInit>::Emit", popped=4, x87=False, thiscall=True,
                   external=True, windows=[(1, lambda a: 4)]),
    0x4EF1D0: dict(name="CSigCarDamage emit", popped=4, x87=False, thiscall=True, external=True, windows=[]),
}
