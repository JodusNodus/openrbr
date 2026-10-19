"""Records for CCar::Vf98x (0x4e6dd0), CCar::Sampler54A0 (0x4e54a0) and CDrive::M6840 (0x506840).

Vf98x runs on the snapshot's live car (--this): its countdown `+0x15e4` is drawn to 0 (the sampled path, the one the
drives reach once) or to a live value, countdown or NaN. Sampler54A0 and M6840 run on inputs in the stub heap: the
sampler's probe and answer blocks, and a drive with its definition (the gear count) and its gear block. Their callees
run in the original; the sampler's vote (0x4fe810), the car's slot 97 (0x4e5270), slot 18 (0x529dc0) and M6840 are
inner records of the Vf98x run.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-vf98 --seed S                       (Sampler54A0, M6840)
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-vf98 --addr 0x4e6dd0 --this CAR --seed S
"""

import struct

SET_NAME = "g-vf98"

NAN = float("nan")


def pf(at, value):
    return (at, struct.pack("<f", value))


def pi(at, value):
    return (at, struct.pack("<I", value & 0xFFFFFFFF))


def fuzz_vf98x(mu, args, rng, put):
    """The countdown on the live car: zero (the sampled path), NaN or a live value (the countdown path)."""
    car = args[0]
    put(car + 0x15E4, rng.choice((0.0, 0.0, NAN, rng.uniform(0.001, 0.2), -rng.uniform(0.001, 0.2))))
    put(car + 0x9F0, rng.choice((0, 0, 1)), "<I")


def fuzz_sampler(mu, heap, r):
    """Sampler54A0 (this, a, b, c): this is a zeroed car at heap+0x3000; the probe a, the answer b and the raw c."""
    car, a, b, c = heap + 0x3000, heap + 0x3100, heap + 0x3200, heap + 0x3300
    writes = [(car, bytes(0x200))]
    writes += [pf(a + 4 * k, r.uniform(-3.0, 3.0)) for k in range(4)]
    writes += [pf(b + 4 * k, r.uniform(-3.0, 3.0)) for k in range(4)]
    writes += [(c, bytes(r.randrange(256) for _ in range(0x50)))]
    return [a, b, c], writes, car


def fuzz_m6840(mu, heap, r):
    """M6840 (this, value): a drive at heap+0x3000 with its definition (+4: the gear count at +0x654) and its gear block
    (+8: the changed flag at +0x56c, the commanded gear at +0x570). The value is drawn around the count."""
    this, def_at, gears = heap + 0x3000, heap + 0x4000, heap + 0x5000
    count = r.randrange(0, 8)
    writes = [pi(this + 4, def_at), pi(this + 8, gears), pi(def_at + 0x654, count),
              pi(gears + 0x56C, r.randrange(2)), pi(gears + 0x570, r.randrange(8))]
    value = r.choice((count - 1, count, count + 1, r.randrange(-3, 10), -1))
    return [value], writes, this


def vf98x_branch(args, windows):
    """'equal' when the countdown is exactly 0 (the sampled path), else 'countdown' (NaN takes it, as the asm's jne)."""
    timer = struct.unpack_from("<f", windows[0][3], 0x15E4)[0]
    return "equal" if timer == 0.0 else "countdown"


def sampler_branch(args, windows):
    """One label: the hit or miss is the record's eax (the voted block is taken when it is 1)."""
    return "vote"


def m6840_branch(args, windows):
    """'clamped' when the value is not below the gear count (the commanded gear becomes 1), else 'kept'."""
    count = struct.unpack_from("<I", windows[2][3], 0x654)[0]
    value = args[1] - (1 << 32) if args[1] >= 1 << 31 else args[1]
    return "clamped" if value >= count else "kept"


SPECS = {
    0x4E6DD0: dict(name="CCar::Vf98x", popped=0, x87=False, thiscall=True, fuzz=fuzz_vf98x,
                   branch=vf98x_branch, windows=[(0, lambda a: 0x15F0)]),
    0x4E54A0: dict(name="CCar::Sampler54A0", popped=12, x87=False, thiscall=True, direct_fuzz=fuzz_sampler,
                   branch=sampler_branch, windows=[(1, lambda a: 16), (2, lambda a: 16), (3, lambda a: 0x50)]),
    0x506840: dict(name="CDrive::M6840", popped=4, x87=False, thiscall=True, direct_fuzz=fuzz_m6840,
                   branch=m6840_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x580, 8),
                                                 (0, lambda a: 0x658, 4)]),
    # The callees the roots call through the original (inner records only).
    0x4FE810: dict(name="CCollisionSystem::Vf00", popped=12, x87=False, thiscall=True, external=True,
                   branch=lambda a, w: "", windows=[]),
    0x4E5270: dict(name="CCar::S184", popped=8, x87=False, thiscall=True, external=True,
                   branch=lambda a, w: "", windows=[]),
    0x529DC0: dict(name="CSolid::E43Vf48", popped=0, x87=False, thiscall=True, external=True,
                   branch=lambda a, w: "", windows=[]),
}
