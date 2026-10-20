"""Direct calls for the respawn path no drive reaches but once (gap/car-proofs): CCar::Split17C (0x4e5530), CCar::Vf98x
(0x4e6dd0, the car's slot 38 countdown), CCar::S184 (0x4e5270, slot 97) and CCar::Vf180 (0x4e9660, slot 96), on the
snapshot's car (the global 0x8ef660). Their callees run in the original and are inner records with windows on the car (so a
test checks the port's bytes before each and takes the callee's after it): the vote (0x4fe810, answered as a miss without
running in a third of Vf98x's calls), M6840, slot 18, slot 17 of +0xe0, Xform8130, ClearAcc, slot 36 and the respawn emit.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-respawn --addr 0x4E5530 --count N --seed SEED
    (likewise 0x4E6DD0, 0x4E5270, 0x4E9660)
"""

import struct  # noqa: E402 (the recipes pack words)

from unicorn import UC_HOOK_CODE  # noqa: E402
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP  # noqa: E402

SET_NAME = "g-respawn"

CAR_GLOBAL = 0x008EF660

DRIVE_GLOBAL, DRIVE_COUNT = 0x01659184, 0x75310
WORLD = 0x01659B70  # the app object: its +0x24's +0x1608c is the tick Vf98x counts down by
VOTE = 0x4FE810  # the sampler's vote (the collision object's slot 0)
NAN = float("nan")


def car_of(mu):
    return struct.unpack("<I", bytes(mu.mem_read(CAR_GLOBAL, 4)))[0]


def word_at(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def drive_nodes(mu, a):
    """The drive table Split17C reads (the global, the count and the entries up to one past the count)."""
    drive = word_at(mu, DRIVE_GLOBAL)
    count = word_at(mu, drive + DRIVE_COUNT)
    return [(DRIVE_GLOBAL, 4), (drive + DRIVE_COUNT, 4), (drive + 0x10, 0x30 * (count + 2))]


def lane_value(mu, rng):
    """A lane below the drive's count: whole indices, the first and last, and fractions."""
    count = word_at(mu, word_at(mu, DRIVE_GLOBAL) + DRIVE_COUNT)
    return rng.choice((0.0, float(count - 1), float(rng.randrange(count)), rng.uniform(0, count - 1),
                       rng.uniform(count - 1.01, count - 0.001)))


def split_direct(mu, heap, rng):
    """Split17C(a, b) on the snapshot's car, its lane drawn by `lane_value`; a and b are random heap blocks."""
    car = car_of(mu)
    a, b = heap + 0xD3000, heap + 0xD3100
    return [a, b], [(car + 0x17C, struct.pack("<f", lane_value(mu, rng))),
                    (a, bytes(rng.getrandbits(8) for _ in range(16))),
                    (b, bytes(rng.getrandbits(8) for _ in range(16)))], car


def skip_hook(mu, rng, addr, popped):
    """In a third of the calls the callee at `addr` returns at once (eax 0) without running."""
    if rng.random() >= 1 / 3:
        return

    def skip(mu, at, size, _):
        esp = mu.reg_read(UC_X86_REG_ESP)
        mu.reg_write(UC_X86_REG_EAX, 0)
        mu.reg_write(UC_X86_REG_ESP, esp + 4 + popped)
        mu.reg_write(UC_X86_REG_EIP, word_at(mu, esp))

    mu.hook_add(UC_HOOK_CODE, skip, begin=addr, end=addr)


def countdown_direct(mu, heap, rng):
    """Vf98x on the snapshot's car: the countdown zero (the sampled path), NaN, or live on either side of the tick; the
    partial-contact flag (+0x9f0) random, the lane drawn by `lane_value`; a null car in a tenth of the calls."""
    del heap
    car = car_of(mu)
    tick = struct.unpack("<f", bytes(mu.mem_read(word_at(mu, word_at(mu, WORLD) + 0x24) + 0x1608C, 4)))[0]
    timer = rng.choice((0.0, 0.0, 0.0, -0.0, NAN, tick * rng.uniform(0.0, 0.999), tick * rng.uniform(1.0, 50.0),
                        -rng.uniform(0.0, 1.0)))
    skip_hook(mu, rng, VOTE, 12)  # the vote's miss
    if rng.random() < 0.1:
        return [], [], 0
    return [], [(car + 0x15E4, struct.pack("<f", timer)), (car + 0x9F0, struct.pack("<I", rng.choice((0, 1)))),
                (car + 0x17C, struct.pack("<f", lane_value(mu, rng)))], car


def countdown_nodes(mu, a):
    app = word_at(mu, WORLD)
    world = word_at(mu, app + 0x24)
    return drive_nodes(mu, a) + [(WORLD, 4), (app + 0x24, 4), (world + 0x1608C, 4)]


def slot97_direct(mu, heap, rng):
    """S184(sel, sample) on the snapshot's car (a null car in a tenth of the calls); sel random, sample a random
    direction off the vertical; the flag +0x9f0 random."""
    car = car_of(mu)
    sel, sample = heap + 0xD3000, heap + 0xD3100
    s = [rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-0.3, 0.3), rng.uniform(-1, 1)]
    writes = [(sel, struct.pack("<4f", *(rng.uniform(-500, 500) for _ in range(4)))), (sample, struct.pack("<4f", *s))]
    if rng.random() < 0.1:
        return [sel, sample], writes, 0
    return [sel, sample], writes + [(car + 0x9F0, struct.pack("<I", rng.choice((0, 1))))], car


def respawn_direct(mu, heap, rng):
    """Vf180(a, b) on the snapshot's car (a null car in a tenth of the calls): a a position near the car's, b a unit
    quaternion; the pending pointer (+0x274) null or a heap word. The car's slot 36 (0x52bf30) clears the pointer, so it
    is skipped in a third of the calls, for the pointer's branch after it."""
    car = car_of(mu)
    a, b, pending = heap + 0xD3000, heap + 0xD3100, heap + 0xD3200
    pos = struct.unpack("<4f", bytes(mu.mem_read(car + 0x140, 16)))
    q = [rng.uniform(-1, 1) for _ in range(4)]
    n = sum(x * x for x in q) ** 0.5
    writes = [(a, struct.pack("<4f", *(p + rng.uniform(-5, 5) for p in pos))), (b, struct.pack("<4f", *(x / n for x in q))),
              (pending, struct.pack("<I", rng.getrandbits(32)))]
    if rng.random() < 0.1:
        return [a, b], writes, 0
    skip_hook(mu, rng, SLOT36, 0)
    return [a, b], writes + [(car + 0x274, struct.pack("<I", rng.choice((0, pending))))], car


def respawn_nodes(mu, a):
    """The pending word, the signal global and the car's slot-36 and +0xe0 slot-17 targets (as dispatched)."""
    if not a[0]:
        return []
    pending = word_at(mu, a[0] + 0x274)
    return ([(pending, 4)] if pending else []) + [(SIGNAL_GLOBAL, 4)]


def ext_win(name, popped, windows=()):
    return dict(name=name, popped=popped, x87=False, thiscall=True, external=True, windows=list(windows))


SIGNAL_GLOBAL = 0x00893600
SLOT36 = 0x52BF30  # the car's slot 36 (CSolid::E43Vf90)
CAR_SPAN = 0x16B0  # the car's bytes (as car_body's windows)


SPECS = {
    0x4E5530: dict(name="CCar::Split17C", popped=8, x87=False, thiscall=True, direct_fuzz=split_direct,
                   nodes=drive_nodes, windows=[(0, lambda a: 0x180), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E6DD0: dict(name="CCar::Vf98x", popped=0, x87=False, thiscall=True, direct_fuzz=countdown_direct,
                   nodes=countdown_nodes, windows=[(0, lambda a: CAR_SPAN)]),
    0x4E5270: dict(name="CCar::S184", popped=8, x87=False, thiscall=True, external=True, direct_fuzz=slot97_direct,
                   windows=[(0, lambda a: CAR_SPAN), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E9660: dict(name="CCar::Vf180", popped=8, x87=False, thiscall=True, external=True, direct_fuzz=respawn_direct,
                   nodes=respawn_nodes, windows=[(0, lambda a: CAR_SPAN), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    VOTE: ext_win("CCollisionSystem::Vf00 (vote)", 12, [(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x50)]),
    0x506840: ext_win("CDrive::M6840", 4, [(0, lambda a: 0x580, 8)]),  # the gear block (the drive's +8)
    0x529DC0: ext_win("CSolid::E43Vf48 (slot 18)", 0, [(0, lambda a: CAR_SPAN)]),
    0x529A50: ext_win("E47SolidState::Vf17x (slot 17 of +0xe0)", 0, [(0, lambda a: CAR_SPAN - 0xE0)]),
    0x4E8130: ext_win("CCar::Xform8130", 8, [(0, lambda a: CAR_SPAN), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x529E60: ext_win("CSolid::E43ClearAcc", 0, [(0, lambda a: CAR_SPAN)]),
    0x52BF30: ext_win("CSolid::E43Vf90 (slot 36)", 0, [(0, lambda a: CAR_SPAN)]),
    0x4E8830: ext_win("CMultiSignal1::Emit CSigRespawn (4e8830)", 4, [(1, lambda a: 0x10)]),
}
