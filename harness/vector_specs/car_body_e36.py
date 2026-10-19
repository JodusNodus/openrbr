# E36 vector and triple helpers (rust/rbr-physics/src/car_body/e36.rs): E36VecU::CrossNorm (0x4f3d40, ret 8) and
# E36Triple::MinPick (0x4f3ec0, ret 0). Run with `--set car_body_e36`. Direct calls only: no car step reaches them.
# The popped bytes are the callee's `ret` immediates.

import struct  # noqa: E402

SET_NAME = "car_body_e36"

VEC_WINDOW = 16


def float_word(r):
    """A float for a lane: mostly finite, with zeros, infinities and NaNs (the rsqrt cases)."""
    return r.choice((r.uniform(-4.0, 4.0), r.uniform(-4.0, 4.0), 0.0, -0.0, float("inf"), float("nan")))


def cross_norm_direct(mu, heap, r):
    """CrossNorm(a, out) on `this`: `this`, `a` and `out` in the stub heap; the two inputs' lanes are random floats."""
    this, a, out = heap + 0x20000, heap + 0x30000, heap + 0x40000
    writes = [(base + 4 * lane, struct.pack("<f", float_word(r))) for base in (this, a) for lane in range(4)]
    writes += [(out + 4 * lane, struct.pack("<I", r.getrandbits(32))) for lane in range(4)]
    return [a, out], writes, this


def min_pick_direct(mu, heap, r):
    """MinPick() on `this` (a triple in the stub heap): the two ints random, the float 0.5 or side of it, NaN included."""
    this = heap + 0x20000
    f = r.choice((0.0, 0.25, 0.5, 0.75, -1.0, float("nan"), float("inf")))
    writes = [(this, struct.pack("<II", r.getrandbits(32), r.getrandbits(32))),
              (this + 8, struct.pack("<f", f))]
    return [], writes, this


SPECS = {
    # E36VecU::CrossNorm(a, out) (ret 8): out = this x a, scaled by rsqrtss of its squared length.
    0x4F3D40: dict(name="E36VecU::CrossNorm (4f3d40)", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: VEC_WINDOW), (1, lambda a: VEC_WINDOW), (2, lambda a: VEC_WINDOW)],
                   direct_fuzz=cross_norm_direct),
    # E36Triple::MinPick() (ret 0): m_a when m_f < 0.5, else m_b (m_a at +0, m_b at +4, m_f at +8).
    0x4F3EC0: dict(name="E36Triple::MinPick (4f3ec0)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 12)], direct_fuzz=min_pick_direct),
}
