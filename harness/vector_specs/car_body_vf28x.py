# Car-body Vf28x (0x4ea020) and its externals (set car_body_vf28x): the car_body specs plus the Vf28x entries. The
# externals that only Vf28x reaches are listed here, so the car_body set and its records stay as they were. Use:
# vcu_vectors.py SNAPSHOT OUT --set car_body_vf28x (the load-time entry dumps of 0x4ea020).

SET_NAME = "car_body_vf28x"


FRAME_LEN = 0x180  # the locals below the frame base: Vf28x's esp-relative slots (up to +0x180)


def vf28x_nodes(mu, a):
    """The words Vf28x reads outside its arguments: the world global (0x1659b70) and its object (+0x24 is the keeper),
    the keeper's flag word (+0x160e0), the frame and signal globals (0x7cbb00, 0x893600), the damage and drive objects
    and their vtables (the virtual calls read a slot of each), the car's vtable, and the frame: the locals Vf28x's
    esp-relative slots address. Its base is the entry esp (return address at esp): push ebp, and esp,-16, sub 0x174,
    three pushes."""
    def word(at):
        return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    car = a[0]
    world = word(0x1659B70)
    keeper = word(world + 0x24)
    out = [(0x1659B70, 4), (world, 0x48), (keeper + 0x160E0, 4), (0x7CBB00, 4), (0x893600, 4)]
    for obj, size in ((word(car + 0x3CC), 0x20), (word(car + 0x4A0), 0x40)):
        out += [(obj, 4), (word(obj), size)]
    out.append((word(car), 0x40))
    esp = mu.reg_read(UC_X86_REG_ESP)
    out.append((((esp - 4) & ~15) - 0x180, FRAME_LEN))
    return out


def vf28x_dispatch(mu, a):
    """The virtual targets: the car's slot 12 (the keeper branch's getter), the slot 12 of the drive at +0x4a0, and the
    damage object's slot 5 (+0x14)."""
    car = a[0]
    drive = struct.unpack("<I", bytes(mu.mem_read(car + 0x4A0, 4)))[0]
    damage = struct.unpack("<I", bytes(mu.mem_read(car + 0x3CC, 4)))[0]
    return [(vtarget(mu, car, 12), 0), (vtarget(mu, drive, 12), 0), (vtarget(mu, damage, 5), 0)]


def vf28x_fuzz(mu, args, rng, put):
    """The def (car+0x30: null takes the short path, which skips the copies and the two signal emits), the keeper's flag
    bit 8 (set: the keeper branch's two slot-12 calls and ComposeA), and each body's test word: the body test (0x521920)
    is true when the word at body+0x580 points at a 1, so each body points at a scratch word below the stack."""
    car = args[0]
    if rng.random() < 0.3:
        put(car + 0x30, 0, "<I")
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    flags = struct.unpack("<I", bytes(mu.mem_read(keeper + 0x160E0, 4)))[0]
    put(keeper + 0x160E0, (flags | 8) if rng.random() < 0.5 else (flags & ~8), "<I")
    esp = mu.reg_read(UC_X86_REG_ESP)
    for i in range(4):
        body = struct.unpack("<I", bytes(mu.mem_read(car + 0x490 + 4 * i, 4)))[0]
        if body and rng.random() < 0.7:
            scratch = esp - 0x1000 + 4 * i
            put(body + 0x580, scratch, "<I")
            put(scratch, rng.choice((1, 1, 2, 0)), "<I")


SPECS = {addr: dict(spec) for addr, spec in SETS["car_body"].items()}
# Xform8130 and Sample9800 are listed in car_body, so they would be top-level records; here they are Vf28x's externals
# (inner records), so the test answers each Vf28x call from one list.
SPECS[0x4E8130]["external"] = True
SPECS[0x4E9800]["external"] = True
# ComposeA writes its out matrix (arg 3, a 3x4 float matrix: 0x30 bytes); the shared spec has no window for it.
SPECS[0x51B4C0] = dict(SPECS[0x51B4C0], windows=[(3, lambda a: 0x30)])
SPECS.update({
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)]),
    0x51A3E0: dict(name="CEntity::E43Vf28", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x560)],
                   external=True),
    0x529D30: dict(name="CSolid::Step77", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 4), (2, lambda a: 4)],
                   dispatch=lambda mu, a: [(vtarget(mu, word(mu, 0x7C755C), 0), 12)], external=True),
    0x4EA020: dict(name="CCar::Vf28x", popped=0, x87=False, thiscall=True, nodes=vf28x_nodes, dispatch=vf28x_dispatch,
                   fuzz=vf28x_fuzz,
                   windows=[(0, lambda a: 0x16B0), (0, lambda a: 0x44B4, 0x34)]),
    0x508480: dict(name="E?::Drive 508480 (508480)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x1200)]),
    0x523210: dict(name="E?::Passenger 523210 (523210)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xE00)]),
    0x518CF0: dict(name="E?::Vf690 (518cf0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x1020)]),
    0x4F2150: dict(name="CCarLightSystem::E4F2150 (4f2150)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xE78)]),
    0x522000: dict(name="E?::Rigid 522000 (522000)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x5390D0: dict(name="E?::Wheel 5390d0 (5390d0)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x50E9A0: dict(name="E?::Engine E9A0 (50e9a0)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E83F0: dict(name="CMultiSignal1::Emit 83f0 (4e83f0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
    0x4E8330: dict(name="CMultiSignal1<SolidGeneral>::Emit (4e8330)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 88)]),
    0x4E84B0: dict(name="CMultiSignal1::Emit 84b0 (4e84b0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x8)]),
    0x4E81B0: dict(name="CMultiSignal1::Emit 81b0 (4e81b0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
    0x4E5CF0: dict(name="CCar::GetDir (4e5cf0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
})
