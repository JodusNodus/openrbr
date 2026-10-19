# Car-body damage handler (rust/rbr-physics/src/car_body, CCarDamageHandler): the handler is embedded in the car at +0x9f4,
# so `this` is `car + 0x9f4`. Run with `--set car_body_damage`. The popped bytes are the callee's `ret` immediates.

import struct  # noqa: E402

SET_NAME = "car_body_damage"

DAMAGE_AT = 0x9F4  # the handler inside CCar (m_damage, RBR_E34_Car.h:614)
HANDLER_WINDOW = 0x3E00  # covers the level, threshold and counter (+0x3a14, +0x3dec, +0x3df0)


def set3fb0_direct_args(rng, this):
    """The level word: the switch's cases 0 to 3, the default (any other value, negative included)."""
    return [rng.choice((0, 1, 2, 3, 4, 5, 0xFFFFFFFF, 0x7FFFFFFF, 0x80000000, 7))]


def copy_from_fuzz(mu, heap, r):
    """CCarDamageHandler::CopyFrom(src) on `this`: two handlers in the stub heap, the source's words at random."""
    dst, src = heap + 0x20000, heap + 0x60000
    writes = [(src + at, struct.pack("<I", r.getrandbits(32))) for at in range(0, HANDLER_WINDOW, 4)]
    writes += [(dst + at, struct.pack("<I", r.getrandbits(32))) for at in range(0, HANDLER_WINDOW, 4)]
    return [src], writes, dst


def test3b20_fuzz(mu, args, rng, put):
    """The gate's inputs on the handler: the enable word (+0x3b20), the level float (+0x3aa0: 0.0, NaN, or a value) and
    the four words that count (+0x3a30, +0x3a3c, +0x3a48, +0x3a54: zero or not)."""
    h = args[0]
    put(h + 0x3B20, rng.choice((0, 1, 7)), "<I")
    put(h + 0x3AA0, rng.choice((0.0, float("nan"), 1.5, -0.25)), "<f")
    for at in (0x3A30, 0x3A3C, 0x3A48, 0x3A54):
        put(h + at, rng.choice((0, 1, 0x20)), "<I")


def add_functor_direct_args(rng, this):
    """The index (-2 to 30: the switch's cases, the two left out, and the out-of-range words) and the three words."""
    return [rng.randrange(-2, 31) & 0xFFFFFFFF, rng.getrandbits(32), rng.getrandbits(32), rng.getrandbits(32)]


def add_functor_fuzz(mu, args, rng, put):
    """The functor count (+0x80, 0 to 5: 4 or more is full)."""
    put(args[0] + 0x80, rng.randrange(0, 6), "<I")


SPECS = {
    # CCarDamageHandler::Set3FB0(level) (ret 4): counter, level and threshold float (0.0, 0.2, 0.7 or 1.0).
    0x4EBFB0: dict(name="CCarDamageHandler::Set3FB0 (4ebfb0)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: HANDLER_WINDOW)], direct_args=set3fb0_direct_args),
    # E36DamageOps::AddFunctor(idx, a, b, c) (ret 0x10): the functor entry at count * 0x20 (fn, zeros, b, a, c).
    0x4EF720: dict(name="E36DamageOps::AddFunctor (4ef720)", popped=0x10, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x84)], direct_args=add_functor_direct_args, fuzz=add_functor_fuzz),
    # CCarDamageHandler::Test3B20() (ret 0): the windshield gate; the enable word, the level and four counted words.
    0x4EC0D0: dict(name="CCarDamageHandler::Test3B20 (4ec0d0)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: HANDLER_WINDOW)], fuzz=test3b20_fuzz),
    # CCarDamageHandler::CopyFrom(src) (ret 4): the zones and floats of eight groups, then the tail (0xfa words, 0x3a0c on).
    0x4EC470: dict(name="CCarDamageHandler::CopyFrom (4ec470)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: HANDLER_WINDOW), (1, lambda a: HANDLER_WINDOW)], direct_fuzz=copy_from_fuzz),
}
