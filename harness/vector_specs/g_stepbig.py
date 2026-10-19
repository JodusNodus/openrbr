"""Direct-call records for CMountedRigidBody::StepBig (0x522100, lead/reports/g-stops6.md), the mounted body's detach:
an owned mounted body of the V2 detach snapshots (zr-25 1991, zr-42 1731, impreza03-33 1943, impreza00-63 2730) with
its pose, its velocities and its car's velocities drawn at random; some calls have no parent or no list (no detach).
The callees are external with SimFD00's windows (g_hinge3.py's specs, loaded before this file), plus WorldCompose and
Norm.

    harness/direct_calls.py SNAPSHOT OUT --set g-stepbig --addr 0x522100 --count N --seed S
"""

import struct

from unicorn import UcError

SET_NAME = "g-stepbig"

MOUNTED_VTABLE = 0x73E62C
PHYSICS = 0x1659B70
BODY_BYTES = 0x590
# The mounted bodies of the four snapshots (each has three, all owned; other snapshots map none of them).
BODIES = (0xC41F8B8, 0xC41FE50, 0xD885798, 0xBED96A0, 0xBED9C38, 0xBEDA1D0, 0xBF536A0, 0xBF53C38, 0xBF541D0,
          0xC337448, 0xC3379E0, 0xC337F78)
HINGE3 = SETS["g-hinge3"]  # noqa: F821 (vcu_vectors' SETS: the spec files load in name order)


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32(mu, at):
    return struct.unpack("<f", bytes(mu.mem_read(at, 4)))[0]


def vtarget(mu, this, slot):
    return word(mu, word(mu, this) + slot)


def floats(at, values):
    return [(at + 4 * k, struct.pack("<f", v)) for k, v in enumerate(values)]


def owned_bodies(mu):
    found = []
    for b in BODIES:
        try:
            if word(mu, b) == MOUNTED_VTABLE and word(mu, b + 0x3D0):
                found.append(b)
        except UcError:
            pass
    if not found:
        raise RuntimeError("no owned mounted body in this snapshot")
    return found


def fuzz_step_big(mu, heap, rng):
    """An owned body detaches: its position moved, its orientation a random (not unit) quaternion near its own, its
    velocities and its car's velocities random."""
    body = rng.choice(owned_bodies(mu))
    car = word(mu, body + 0x3D0)
    writes = floats(body + 0x140, [f32(mu, body + 0x140 + 4 * k) + rng.uniform(-3, 3) for k in range(3)])
    writes += floats(body + 0x100, [f32(mu, body + 0x100 + 4 * k) + rng.uniform(-0.3, 0.3) for k in range(4)])
    for at, spread in ((0x1C0, 20.0), (0x190, 5.0)):
        writes += floats(body + at, [rng.uniform(-spread, spread) for _ in range(3)])
        writes += floats(car + at, [rng.uniform(-spread, spread) for _ in range(3)])
    r = rng.random()
    if r < 0.1:
        writes.append((body + 0x44, struct.pack("<I", 0)))
    elif r < 0.2:
        writes.append((body + 0x18, struct.pack("<I", 0)))
    return [], writes, body


def step_big_nodes(mu, a):
    body = a[0]
    car, lst = word(mu, body + 0x3D0), word(mu, body + 0x18)
    out = [(0x8EB794, 4), (0x893600, 4), (PHYSICS, 4), (word(mu, PHYSICS) + 0x20, 4)]
    if car:
        out.append((car + 0x190, 0x40))
    if lst:
        out.append((lst, 12))
    return out


def body():
    return [(0, lambda a: BODY_BYTES)]


def ext(name, popped, windows=()):
    return dict(name=name, popped=popped, x87=False, thiscall=True, external=True, windows=list(windows))


SPECS = {addr: HINGE3[addr] for addr in (0x483DD0, 0x4E68E0, 0x529E60, 0x4E41A0, 0x519BD0, 0x4E03B0, 0x4FAEB0,
                                         0x5263D0, 0x51F580, 0x51F7C0)}
SPECS.update({
    0x522100: dict(name="CMountedRigidBody::StepBig", popped=0, x87=False, thiscall=True, direct_fuzz=fuzz_step_big,
                   windows=body(), nodes=step_big_nodes, direct=True,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], s), 0, ext("slot %#x" % s, 0, body()))
                                           for s in (0x54, 0x90, 0x48)]),
    0x4E6950: ext("CCar::WorldCompose", 8, [(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E4C70: ext("E34Quat::Norm", 4, [(0, lambda a: 0x10), (1, lambda a: 0x10)]),
})
