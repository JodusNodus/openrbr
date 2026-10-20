"""Direct-call records for CCar::AppendA980(a) (0x4ea980) on a null car (G-append-null): the asm still takes `a` out of
its list (the owner's count down, then Unlink 0x4e0020) and skips the append. The recipe builds a list of up to four
links in the replay's stub heap, `a` in it or free.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-append-null --count N --seed SEED
"""

import struct

SET_NAME = "g-append-null"

LIST = 0x1000  # heap offset: the head (first, last, count), the links at +0x20 + 0x10 k


def append_null(mu, heap, rng):
    head = heap + LIST
    slots = [head + 0x20 + 0x10 * k for k in range(4)]
    a = rng.choice(slots)
    members = [x for x in slots if x != a and rng.random() < 0.5]
    if rng.random() < 2 / 3:
        members.insert(rng.randrange(len(members) + 1), a)
    image = bytearray(0x60)
    for i, x in enumerate(members):
        struct.pack_into("<3I", image, x - head, members[i + 1] if i + 1 < len(members) else 0,
                         members[i - 1] if i > 0 else 0, head)
    struct.pack_into("<3I", image, 0, members[0] if members else 0, members[-1] if members else 0, len(members))
    return [a], [(head, bytes(image))], 0


SPECS = {
    0x4EA980: dict(name="CCar::AppendA980", popped=4, x87=False, thiscall=True, direct_fuzz=append_null,
                   windows=[(1, lambda a: 0x0C)], nodes=lambda mu, a: [(a[1] & ~0xFFF, 0x60)]),
    0x4E0020: dict(name="CBodyLink::Unlink", popped=0, x87=False, thiscall=True, external=True, windows=[]),
}
