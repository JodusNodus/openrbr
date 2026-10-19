"""Direct-call records for CMountedRigidBody::Vf6c (lead/reports/g-mounted.md): a mounted body of the snapshot (--this,
one with an owner) with its velocities, its position around the mount point (so each clamp bound is hit) and its
event word (+0x584) drawn at random. The emits are the original's, kept as inner records with their payloads."""

import struct

SET_NAME = "g-mounted"

BODY_BYTES = 0x590
MOUNT_POINT = 0x220


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32(mu, at):
    return struct.unpack("<f", bytes(mu.mem_read(at, 4)))[0]


def vf6c_args(rng, this):
    return [struct.unpack("<I", struct.pack("<f", rng.choice((0.007, rng.uniform(0.001, 0.02)))))[0]]


def fuzz_vf6c(mu, a, rng, put):
    body = a[0]
    mount = word(mu, body + 0x34) + MOUNT_POINT
    for k in range(4):
        put(body + 0x1C0 + 4 * k, rng.uniform(-2.0, 2.0))
        put(body + 0x230 + 4 * k, rng.uniform(-50.0, 50.0))
    for k in range(3):
        put(body + 0x140 + 4 * k, f32(mu, mount + 4 * k) + rng.uniform(-0.06, 0.06))
    put(body + 0x14C, rng.uniform(-1.0, 1.0))
    put(body + 0x584, rng.choice((0, 1)), "<I")


def vf6c_branch(args, windows):
    """unowned, or the body event's first step (event) or not (state), read from the body window."""
    pre = windows[0][3]
    if struct.unpack_from("<I", pre, 0x3D0)[0] == 0:
        return "unowned"
    return "state" if struct.unpack_from("<I", pre, 0x584)[0] else "event"


SPECS = {
    0x521D90: dict(name="CMountedRigidBody::Vf6c", popped=4, x87=False, thiscall=True, direct_args=vf6c_args,
                   fuzz=fuzz_vf6c, branch=vf6c_branch,
                   windows=[(0, lambda a: BODY_BYTES), (0, lambda a: MOUNT_POINT + 0x10, 0x34)],
                   nodes=lambda mu, a: [(0x893600, 4)]),
    0x51F7C0: dict(name="CMultiSignal1<CSigMrRBodyEvent>::Emit", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x50)]),
    0x51F700: dict(name="CMultiSignal1<CSigMrRBodyState>::Emit", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x30)]),
    0x52CC30: dict(name="CSolid::E43Vf6C", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: BODY_BYTES)]),
}
