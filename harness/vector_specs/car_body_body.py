# Car-body point scatters (rust/rbr-physics/src/car_body/body.rs): CCarBody::Vf9Cx (0x4eb820, subtract) and VfA0x (0x4eb8c0,
# add). Run with `--set car_body_body` (direct calls, a body in the stub heap). The body's +0x34 names an owner object in
# the same window (its +0x120 word is non-null: the original faults on a null base). The world's flag word gates slot 0 of the
# body (0x519950, a getter). The popped bytes are the callee's `ret` immediates.

import struct  # noqa: E402

SET_NAME = "car_body_body"

WORLD = 0x1659B70  # the world object global (g_1659b70)
CCAR_BODY_VTABLE = 0x738E9C  # CCarBody's vtable (RBR_E35_CarBody.h)
BODY_WINDOW = 0x240  # the body (0x130 bytes), the owner at +0x100 (its base at +0x130, its word at +0x120)


def scatter_direct(mu, heap, r):
    """Vf9Cx(out, n) on a body in the stub heap: the body's words random, its vtable the CCarBody's, its owner at +0x100 with a
    non-null word at +0x120, the points random (finite, a few zeros), the count 0 to 4, and the world's flag bit (0x1000) set
    about half the time (in the world's manager word +0x160f4 or the world's +0x38)."""
    body = heap + 0x20000
    out = heap + 0x40000
    n = r.randrange(0, 5)
    words = [out, n]
    block = bytearray(r.getrandbits(8) for _ in range(BODY_WINDOW))
    struct.pack_into("<I", block, 0, CCAR_BODY_VTABLE)
    struct.pack_into("<I", block, 0x34, body + 0x100)
    struct.pack_into("<I", block, 0x100 + 0x120, r.choice((1, 0x1234, 0xFFFFFFFF)))
    for at in range(0x100 + 0x130, 0x100 + 0x13C, 4):
        struct.pack_into("<f", block, at, r.uniform(-3.0, 3.0))
    points = bytearray(r.getrandbits(8) for _ in range(16 * max(n, 1)))
    for i in range(n):
        for k in range(3):
            struct.pack_into("<f", points, 16 * i + 4 * k, r.choice((r.uniform(-4.0, 4.0), 0.0)))
    writes = [(body, bytes(block)), (out, bytes(points))]
    host = word(mu, WORLD)
    manager = word(mu, host + 0x24)
    flag = r.choice((0x1000, 0, 0x2000))
    writes.append((manager + 0x160F4, struct.pack("<I", flag if r.random() < 0.5 else 0)))
    writes.append((host + 0x38, struct.pack("<I", flag if r.random() < 0.5 else 0)))
    return words, writes, body


SPECS = {
    # CCarBody::Vf9Cx(out, n) (ret 8): subtracts the base from each point's lanes 0 to 2.
    0x4EB820: dict(name="CCarBody::Vf9Cx (4eb820)", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: BODY_WINDOW), (1, lambda a: 16 * max(a[2], 1))],
                   direct_fuzz=scatter_direct),
    # CCarBody::VfA0x(out, n) (ret 8): adds the base to each point's lanes 0 to 2.
    0x4EB8C0: dict(name="CCarBody::VfA0x (4eb8c0)", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: BODY_WINDOW), (1, lambda a: 16 * max(a[2], 1))],
                   direct_fuzz=scatter_direct),
    # The body's slot 0 (CCarBody vtable, 0x519950): a getter, external when the world's flag gates it.
    0x519950: dict(name="CEntity slot 0 getter (519950)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[]),
}
