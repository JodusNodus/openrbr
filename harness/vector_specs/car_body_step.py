# Car-body step vectors: the per-step functions as top-level records (rust/rbr-physics/src/car_body, tests/car_body_vectors.rs).
# The car_body set keeps the same functions as externals (inner records of the step's callers); this set records them
# on their own, with their externals (the identity S30 getters, MB560, ComposeA) as inner records. Run with
# `--set car_body_step`. The popped bytes are the callee's `ret` immediates (thiscall: this is ecx, not popped).

SET_NAME = "car_body_step"


def s30_car(mu, a):
    """The identity getter (slot 12) on the car: the call ToWorld and WorldCompose make first."""
    return [(vtarget(mu, a[0], 12), 0)]


def s30_car_and_body(mu, a):
    """The two identity getters of BodyToWorldPair and BodyCompose: slot 12 on the car, then on its body (the pointer
    at +0x4a0). Both return their `this`; the first leaves its pushed buffer (popped 0)."""
    body = word(mu, a[0] + 0x4A0)
    return [(vtarget(mu, a[0], 12), 0), (vtarget(mu, body, 12), 0)]


def world_direct_args(rng, this):
    """Direct calls: a and b are two scratch blocks in the car (outside the windows the step uses)."""
    return [this + 0x300, this + 0x320]


def world_fuzz(mu, args, rng, put):
    """The input vector a (random lanes)."""
    for k in range(4):
        put(args[1] + 4 * k, rng.uniform(-2, 2))


def get_dir_nodes(mu, a):
    """The direction vectors of the wheels at car+0x470 and +0x474 (+0x9e0, four words); an empty slot is skipped."""
    return [(word(mu, a[0] + slot) + 0x9E0, 0x10) for slot in (0x470, 0x474) if word(mu, a[0] + slot)]


def get_dir_direct_args(rng, this):
    """Direct calls: `out` is a scratch vector in the car (outside the windows the step uses)."""
    return [this + 0x300]


def get_dir_fuzz(mu, args, rng, put):
    """The wheels: each slot is empty now and then (the fallback needs both sides short), and a set wheel's direction
    lanes are random, with zeros and small values so the length falls under 0.01."""
    car = args[0]
    pool = (0.0, 1e-3, -1.0, 0.5, -0.25)
    for slot in (0x470, 0x474):
        wheel = word(mu, car + slot)
        if not wheel:
            continue
        if rng.random() < 0.2:
            put(car + slot, 0, "<I")
            continue
        for off in (0x9E0, 0x9E4, 0x9E8, 0x9EC):
            put(wheel + off, rng.choice(pool) if rng.random() < 0.4 else rng.uniform(-2, 2))


def ffb_norm_direct_args(rng, this):
    """Direct calls take no stack arguments."""
    return []


def ffb_norm_fuzz(mu, args, rng, put):
    """The gate (the car's word at +0x1ec, read by S8C), the filtered steering torque (+0xaec) and MaxFFTorque (def+0x44a0): the
    clamp's cases, zero and NaN included."""
    car = args[0]
    nan = float("nan")
    put(car + 0x1EC, rng.choice((0, 0, 1)), "<I")
    put(car + 0xAEC, rng.choice((0.0, 0.5, -3.0, 3.0, nan, rng.uniform(-2, 2))))
    scale_at = word(mu, car + 0x34) + 0x44A0
    put(scale_at, rng.choice((0.0, 1.0, 10.0, -2.0, rng.uniform(0.1, 20.0))))


SPECS = {
    # E33 identity getter (slot 12): returns its this.
    0x519C00: dict(name="E33 S30 identity", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # E33Mat34::ComposeA (ret 0xc): args (this, a, t, out). The walk is the entities port's; the record answers it.
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    # E43PairWalk::MB560 (ret 0xc): args (this, a, b, c). Window 0 is a, window 1 is c (the walk's output).
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    # CCar::ToWorld(a, b) (0x4e68e0, ret 8): MB560(S30(car), a, frame, b), frame = the global 0x7cbb00. Returns b.
    0x4E68E0: dict(name="CCar::ToWorld", popped=8, x87=False, thiscall=True, dispatch=s30_car,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CCar::WorldCompose(a, b) (0x4e6950, ret 8): ComposeA(S30(car), a, frame, b). Returns b.
    0x4E6950: dict(name="CCar::WorldCompose", popped=8, x87=False, thiscall=True, dispatch=s30_car,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x1C)]),
    # CCar::BodyToWorldPair(a, b) (0x4e69c0, ret 8): MB560(S30(body), a, S30(car), b). Returns b.
    0x4E69C0: dict(name="CCar::BodyToWorldPair", popped=8, x87=False, thiscall=True, dispatch=s30_car_and_body,
                   direct_args=world_direct_args, fuzz=world_fuzz,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CCar::BodyCompose(a, b) (0x4e6a30, ret 8): ComposeA(S30(body), a, S30(car), b). Returns b.
    0x4E6A30: dict(name="CCar::BodyCompose", popped=8, x87=False, thiscall=True, dispatch=s30_car_and_body,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x1C)]),
    # CCar::GetDir(out) (0x4e5cf0, ret 4): the wheel directions at car+0x470 and +0x474 (the nodes), summed into a
    # vector and turned into the heading. The car window holds the wheel pointers; `out` is windowed.
    0x4E5CF0: dict(name="CCar::GetDir", popped=4, x87=False, thiscall=True, nodes=get_dir_nodes,
                   direct_args=get_dir_direct_args, fuzz=get_dir_fuzz,
                   windows=[(0, lambda a: 0x480), (1, lambda a: 0x10)]),
    # CCar::FfbNorm (0x4e76c0, ret): the gate (slot 35, S8C) then the filtered steering torque over MaxFFTorque at def+0x44a0,
    # clamped to [-1, 1]. Returns in st0 (flds). The def is the car's +0x34 pointer, one hop.
    0x4E76C0: dict(name="CCar::FfbNorm", popped=0, x87=True, thiscall=True,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 35), 0)],
                   direct_args=ffb_norm_direct_args, fuzz=ffb_norm_fuzz,
                   windows=[(0, lambda a: 0xAF0), (0, lambda a: 0x44A4, 0x34)]),
}
