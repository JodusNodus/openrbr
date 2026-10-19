# Car-body wheel attach (rust/rbr-physics/src/car_body, Vf150x): top-level records. Its E43 calls are externals.
# Run with `--set car_body_wheel`. The popped bytes are the callee's `ret` immediates (thiscall: this is ecx).

SET_NAME = "car_body_wheel"


def wheel_targets(mu, a):
    """The slot-59 target on the wheel (the stack argument of Vf150x), popped 4 (one stack argument: the car)."""
    return [(vtarget(mu, a[1], 59), 4)]


def wheel_direct_args(rng, this):
    """Direct calls: the wheel is the car's first wheel (its pointer at +0x470, as in the snapshot)."""
    return [0x0BF1F520]


def wheel_fuzz(mu, args, rng, put):
    """The wheel's flags (+0x60: the attach bits 0x20, 0x10, 0x400, 0x800 in random combinations), its def's two indices
    (+0x4fc and +0x23c, the slot numbers 0 to 5), and the car's slots emptied now and then (the slot search and the
    slot stores). Every write goes back when the call returns."""
    car, wheel = args[0], args[1]
    put(wheel + 0x60, rng.choice((0x20, 0x30, 0x420, 0x820, 0x430, 0x830, 0x20, 0x0, 0x10)), "<I")
    # The slot-59 append on the wheel reads its counted array at +0x2c, which the snapshot's wheel leaves null:
    # give it the car's array (the appends write there; those writes are not in the windows).
    put(wheel + 0x2C, word(mu, car + 0x2C), "<I")
    d = word(mu, wheel + 0x34)
    put(d + 0x4FC, rng.randrange(0, 6), "<I")
    put(d + 0x23C, rng.randrange(0, 6), "<I")
    for k in range(4):
        if rng.random() < 0.3:
            put(car + 0x470 + 4 * k, 0, "<I")
        if rng.random() < 0.3:
            put(car + 0x480 + 4 * k, 0, "<I")
        if rng.random() < 0.3:
            put(car + 0x490 + 4 * k, 0, "<I")


SPECS = {
    # CEntity::E43VfEC called directly on the car with the wheel (ret 4).
    0x51A040: dict(name="CEntity::E43VfEC direct (51a040)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    # CEntity::E43Vf150 called directly on the car with the wheel (ret 4); returns eax. It appends the wheel to the car's
    # list (head +0x48, tail +0x4c, count +0x50) and links the wheel (+0x10, +0x14, +0x18): both windows are its effect.
    0x51A370: dict(name="CEntity::E43Vf150 (51a370)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x60), (1, lambda a: 0x20)]),
    # CCar::Vf150x(wheel) (0x4e5f20, ret 4): attaches the wheel (m_wheels, the body, the +0x480 and +0x490 slots).
    0x4E5F20: dict(name="CCar::Vf150x", popped=4, x87=False, thiscall=True, dispatch=wheel_targets,
                   direct_args=wheel_direct_args, fuzz=wheel_fuzz,
                   windows=[(0, lambda a: 0x16B0), (1, lambda a: 0xEB8), (1, lambda a: 0x500, 0x34)]),
}
