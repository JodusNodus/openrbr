# Car-body seed applier (crates/rbr-physics/src/car_body/setup.rs, CCar::UpdateA720). Run with `--set car_body_a720`
# (direct calls from the snapshot's car, 0xbf1d7b8). The car's +0x34 is set to the car itself, so the frame stores
# (+0x224 to +0x4470) land in the car window. The externals are the drive's M8770, NopD300, CopyDef and the four wheels'
# CopyCF30; UpdateMrr (0x4e9c70) is external here too, with the car window and the wheels' def windows. The popped bytes are the callee's `ret` immediates.

SET_NAME = "car_body_a720"

CAR = 0x0BF1D7B8  # the step snapshot's car (the car_body set's `--this`)
CAR_WINDOW = 0x4A00  # the car's words the seed applier and UpdateMrr write (up to +0x4470, and the handler at +0x9f4)
SEED_WINDOW = 0x140  # the seed block: its frame pointer (+4) is at +0x100, the call words at +8 to +0x34
SEED = 0x60000  # stub heap offsets: the seed block, and the blocks its pointers name
BLOCKS = 0x80000
BLOCK = 0x200
WHEEL_LIST = 0x470  # the car's four wheel pointers (+0x470 to +0x47c)
WHEEL_WINDOW = 0x1000  # a wheel: CopyCF30 writes its subobjects at +0xec4 and +0xf00
DEF_WINDOW = 0x904  # a wheel's def (the pointer at wheel+0x34): CopyCF30 writes its words up to +0x900


def wheel_windows(k):
    """The windows of wheel k (the car's list entry) and of its def, followed from the car (the first argument)."""
    at = WHEEL_LIST + 4 * k
    return [(0, lambda a: WHEEL_WINDOW, at), (0, lambda a: DEF_WINDOW, at, 0x34)]


def update_a720_direct(mu, heap, r):
    """UpdateA720(seed) on the snapshot's car: the seed block random, its frame pointer at +0x100, its call words (+8 to
    +0x34) pointing at random blocks in the stub heap. The car's +0x34 is the car itself (see the module comment)."""
    seed = heap + SEED
    writes = [(seed, bytes(r.getrandbits(8) for _ in range(SEED_WINDOW)))]
    writes.append((seed + 4, (seed + 0x100).to_bytes(4, "little")))
    # The call words (+8 to +0x40): each names a block (the M8770 source, NopD300, CopyDef, and the wheels' a, b and c).
    for k, at in enumerate(range(0x8, 0x44, 4)):
        writes.append((seed + at, (heap + BLOCKS + BLOCK * k).to_bytes(4, "little")))
    for k in range(15):
        block = bytearray(BLOCK)
        at = heap + BLOCKS + BLOCK * k
        # The callees read the argument's words (+0x2c points at a second block, zeroed, at +0x80)
        block[0x2C:0x30] = (at + 0x80).to_bytes(4, "little")
        writes.append((at, bytes(block)))
    writes.append((CAR + 0x34, CAR.to_bytes(4, "little")))
    return [seed], writes, CAR


SPECS = {
    # CCar::UpdateA720(seed) (ret 4): the externals in order, the frame stores, then UpdateMrr.
    0x4EA720: dict(name="CCar::UpdateA720 (4ea720)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: CAR_WINDOW), (1, lambda a: SEED_WINDOW)]
                   + [w for k in range(4) for w in wheel_windows(k)], direct_fuzz=update_a720_direct),
    # CDrive::M8770(src) on car+0x4b0 (ret 4): the drive's state snapshot; its window is the car from +0x4b0.
    0x508770: dict(name="CDrive::M8770 (508770)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: CAR_WINDOW - 0x4B0)]),
    # E41Plain::NopD300(p) on car+0x690 (ret 4): empty.
    0x50D300: dict(name="E41Plain::NopD300 (50d300)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    # CVehicleControlUnit::CScratchSim::CopyDef(src) on car+0x850 (ret 4): its window is the car from +0x850.
    0x537C90: dict(name="CScratchSim::CopyDef (537c90)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: CAR_WINDOW - 0x850)]),
    # E51Wheel::CopyCF30(a, b, c) on a wheel (ret 0xc): its effects are the wheel's subobjects and the wheel's def.
    0x53CF30: dict(name="E51Wheel::CopyCF30 (53cf30)", popped=0xC, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: WHEEL_WINDOW), (0, lambda a: DEF_WINDOW, 0x34)]),
    # CCar::UpdateMrr() (ret 0), external here: its effects are the car and the wheels' defs (the wheel windows).
    0x4E9C70: dict(name="CCar::UpdateMrr (4e9c70)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: CAR_WINDOW)] + [w for k in range(4) for w in wheel_windows(k)]),
}
