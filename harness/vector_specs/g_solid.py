"""CSolid::Step76 (0x529c80) and CSolid::Vf85 (0x52b870), the two rigid-body functions the wheel's slot-96 step and the
ground probe reach: no 30-step chain calls either. Step76 is the velocity of a point,
(p - pos) x w + v, xyz only (the cross product is the host call 0x43c630, recorded as its inner record). Vf85 answers
whether the definition's word at +0x1ec is nonzero (the compare of the one branch-free test; both answers come up in
the records). The direct recipes build the solid (pos +0x140, w +0x190, v +0x1c0, the definition at +0x34) and the
definition in the stub heap.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-solid --addr 0x529C80 --addr 0x52B870 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipes pack floats and words)

SET_NAME = "g-solid"

SOLID, POINT, OUT, DEF = 0xA0000, 0xA2000, 0xA2100, 0xA1000  # stub heap offsets (heap + offset)


def _solid_image(rng, definition):
    image = bytearray(0x200)
    struct.pack_into("<I", image, 0x34, definition)
    for at in (0x140, 0x190, 0x1C0):  # pos, w, v: three floats each
        struct.pack_into("<3f", image, at, *(rng.uniform(-10.0, 10.0) for _ in range(3)))
    return image


def _step76_direct(mu, heap, rng):
    del mu
    solid, point, out = heap + SOLID, heap + POINT, heap + OUT
    writes = [(solid, bytes(_solid_image(rng, heap + DEF))), (heap + DEF, bytes(0x1F0)),
              (point, struct.pack("<3f", *(rng.uniform(-10.0, 10.0) for _ in range(3)))),
              (out, struct.pack("<3f", *(rng.uniform(-10.0, 10.0) for _ in range(3))))]
    return [point, out], writes, solid


def _vf85_direct(mu, heap, rng):
    del mu
    definition = heap + DEF
    def_image = bytearray(0x1F0)
    struct.pack_into("<I", def_image, 0x1EC, rng.choice((0, 1, 0x80000000, rng.getrandbits(32))))
    writes = [(heap + SOLID, bytes(_solid_image(rng, definition))), (definition, bytes(def_image))]
    return [], writes, heap + SOLID


SPECS = {
    0x529C80: dict(name="CSolid::Step76", popped=8, x87=False, thiscall=True, external=False,
                   windows=[(0, lambda a: 0x1D0), (1, lambda a: 12), (2, lambda a: 12)], direct_fuzz=_step76_direct),
    # The cross product (rigid_body.py's spec): an inner call of Step76.
    0x43C630: dict(name="E01V3::Cross", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x52B870: dict(name="CSolid::Vf85", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x40), (0, lambda a: 0x1F0, 0x34)], direct_fuzz=_vf85_direct),
}
