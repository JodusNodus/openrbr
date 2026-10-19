# Car-body vectors (crates/rbr-physics/src/car_body, tests/car_body_vectors.rs). Helpers from vcu_vectors.py are in scope.
# Batch A: the quaternion kernel ToQuat, the car accessors and the dirty-flag setters. The popped bytes are the
# callee's `ret` immediates (thiscall: this is ecx, not popped).

from unicorn import UcError  # noqa: E402 (wheel_flag_nodes reads objects that may not map the field)

SET_NAME = "car_body"


def quat_branch(args, windows):
    """ToQuat's branch (E34Quat::ToQuat, 0x4e4d10): 'pos' when the trace t = (v0 + v8) + v4 is above 0, else
    'diagN', the diagonal index i the asm picks (NaN takes the diagonal path, as the asm's jbe does)."""
    v = [f32(struct.unpack_from("<I", windows[0][3], 4 * k)[0]) for k in range(9)]
    t = f32_round(f32_round(v[0] + v[8]) + v[4])
    if t > 0.0:
        return "pos"
    i = 1 if v[0] < v[4] else 0
    if v[8] > v[i * 4]:
        i = 2
    return "diag%d" % i


def quat_fuzz(mu, args, rng, put):
    """Random 3x3 matrices: the trace sign and the diagonal order decide the branch, so draw the diagonal and the
    off-diagonal from a pool that reaches each (and NaN now and then)."""
    pool = (0.0, 0.5, -0.5, 1.0, -1.0, 0.25, -0.25, 2.0, -2.0, float("nan"))
    for k in range(9):
        put(args[0] + 4 * k, rng.choice(pool) if rng.random() < 0.3 else rng.uniform(-2, 2))


def ffb_torque_branch(args, windows):
    """UpdateFfbTorque (0x4e7ff0): 'timer' (the override timer above 0), else the speed factor's clamp: 'low'
    (f3 at 0), 'high' (at 1), 'mid' (between, and NaN)."""
    car = windows[0][3]
    timer = f32(struct.unpack_from("<I", car, 0x1670)[0])
    if timer > 0.0:
        return "timer"
    speed = f32(struct.unpack_from("<I", car, 0x15FC)[0])
    f3 = f32_round(f32_round(speed - 7.0) * f32_round(1.0 / 6.0))
    if f3 < 0.0:
        return "low"
    if 1.0 < f3:
        return "high"
    return "mid"


def ffb_torque_fuzz(mu, args, rng, put):
    """UpdateFfbTorque's inputs: the start hold, the speed (the clamp's cases), SteeringRackRatio (at def+0x23c, the
    def is at this+0x34), the steering torque sum and the filtered torque."""
    this = args[0]
    nan = float("nan")
    put(this + 0x1670, rng.choice((0.0, 0.5, -0.2, nan, rng.uniform(-1, 1))))
    put(this + 0x15FC, rng.choice((nan, 0.0, 7.0, 13.0, 20.0, -3.0, rng.uniform(-2, 25))))
    gain_at = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0] + 0x23C
    put(gain_at, rng.choice((0.0, 1.0, 0.5, nan, rng.uniform(-3, 3))))
    put(this + 0xADC, rng.choice((0.0, 1.0, nan, rng.uniform(-500, 500))))
    put(this + 0xAEC, rng.choice((0.0, nan, rng.uniform(-500, 500))))


def xform_direct_args(rng, this):
    """Direct calls of Xform6870 (a, b): two 16-byte scratch vectors in the car (outside the windows the step uses)."""
    return [this + 0x300, this + 0x320]


def xform_fuzz(mu, args, rng, put):
    """The two vectors a and b (their words are random; b is the copy target)."""
    for base in (args[1], args[2]):
        for k in range(4):
            put(base + 4 * k, rng.uniform(-2, 2))


def drag_nodes(mu, a):
    """The time keeper's words the gate reads: the world object (at the global 0x1659b70), its +0x24 pointer
    (the keeper) and the keeper's word at +0x160f4; the world's +0x38 word is inside the first window."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    return [(0x1659B70, 4), (world, 0x40), (keeper + 0x160F4, 4)]


def drag_fuzz(mu, args, rng, put):
    """The velocity and the angular velocity (the walk's input, so the spin damping's over-5 branch is reached), the
    drag setup (Cd and the frontal area, in the def at this+0x34) and the gate's bit (the world's +0x38 word)."""
    this = args[0]
    for k in range(3):
        put(this + 0x1C0 + 4 * k, rng.uniform(-40, 40))
        put(this + 0x190 + 4 * k, rng.uniform(-8, 8))
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    put(world + 0x38, 0x4000 if rng.random() < 0.5 else 0, "<I")
    pdef = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    put(pdef + 0x4490, rng.uniform(0.3, 0.5))
    put(pdef + 0x4480, rng.uniform(1.5, 2.5))


def update9af0_nodes(mu, a):
    """The world object (at the global 0x1659b70): its words at +0x20 and +0x40 are read (the timer and its object), and
    the globals of the signal objects (0x893600) and the flow object (0x165fa90)."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    return [(0x1659B70, 4), (world, 0x48), (0x893600, 4), (0x165FA90, 4)]


def sample9800_fuzz(mu, args, rng, put):
    """The E39 block's count (+0x80, signed) and scale (+0x8c), the filtered steering torque (+0xaec) and the position value
    (+0x15f4), random. The block pointer is the car's +0x620."""
    this = args[0]
    block = struct.unpack("<I", bytes(mu.mem_read(this + 0x620, 4)))[0]
    put(block + 0x80, rng.choice((rng.randrange(0, 1 << 16), rng.randrange(-(1 << 31), 0))), "<i")
    put(block + 0x8C, rng.uniform(-3.0, 3.0))
    put(this + 0xAEC, rng.uniform(-50.0, 50.0))
    put(this + 0x15F4, rng.uniform(-30.0, 30.0))


def sample9800_nodes(mu, a):
    """The world object (at the global 0x1659b70): its words at +0x20 and +0x40 are read (the timer card and the keeper,
    which the timer gap reads)."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    return [(0x1659B70, 4), (world, 0x48)]


def update_seed_nodes(mu, a):
    """The world object (at the global 0x1659b70), its timer card (world+0x20, read at +0x18), the E19 object (the
    global 0x893634) and the signal objects (0x893600)."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    card = struct.unpack("<I", bytes(mu.mem_read(world + 0x20, 4)))[0]
    return [(0x1659B70, 4), (world, 0x48), (card, 0x28), (0x893634, 4), (0x893600, 4)]


def update_seed_fuzz(mu, args, rng, put):
    """The override (+0x1670: positive takes the local seed), the seed words, the argument's flags word (+0x14: bits 0x20
    and 0x10 pick the gate's branch) and the step counter (+0x950: multiples of 1000 store the card). E19IsState2 holds
    when the object at the global 0x893634 has 2 at +0x10; S8C returns the car's word at +0x1ec. MotionProbe's inputs
    are its own fuzz (the car is its argument too)."""
    this, src = args[0], args[1]
    put(this + 0x1670, rng.uniform(-1, 2))
    for i in range(5):
        put(src + 4 * i, rng.uniform(-1, 1))
    put(src + 0x14, rng.randrange(1 << 8), "<I")
    put(this + 0x950, rng.choice((0, 1000, 2000, 3000, 4000, rng.randrange(1 << 16))), "<I")
    motion_fuzz(mu, args, rng, put)
    e19 = struct.unpack("<I", bytes(mu.mem_read(0x893634, 4)))[0]
    put(e19 + 0x10, 2 if rng.random() < 0.5 else rng.randrange(4), "<I")
    put(this + 0x1EC, 0 if rng.random() < 0.6 else rng.randrange(1, 4), "<I")


# The seed buffer the direct calls pass: a 0x20-byte block in the entry dump's heap (the first UpdateSeed call's `p`),
# which the dump maps; the fuzz writes its words.
UPDATE_SEED_BUFFER = 0x1A4FCAC


def update_seed_direct_args(rng, this):
    """The seed buffer (the argument `p`), at the address the entry dump maps."""
    return [UPDATE_SEED_BUFFER]


def vf158_nodes(mu, args):
    car = args[0]
    vtable = struct.unpack("<I", bytes(mu.mem_read(car, 4)))[0]
    parent = struct.unpack("<I", bytes(mu.mem_read(car + 0x44, 4)))[0]
    nodes = [(0x01659B68, 4), (vtable, 0x15C)]
    if parent:
        parent_vtable = struct.unpack("<I", bytes(mu.mem_read(parent, 4)))[0]
        parent_slot = parent_vtable + 0x158
        target = struct.unpack("<I", bytes(mu.mem_read(parent_slot, 4)))[0]
        nodes.extend(((parent, 4), (parent_slot, 4), (target, 1)))
    return nodes


def vf158_direct(mu, heap, rng):
    """A car-like solid with a parent: slot 0 and slot 86 are known, and its parent's slot 86 returns directly."""
    del mu
    car, car_vtable = heap + 0x90000, heap + 0x91000
    parent, parent_vtable, ret = heap + 0x92000, heap + 0x93000, heap + 0x94000
    image = bytearray(0x3D0)
    struct.pack_into("<I", image, 0, car_vtable)
    struct.pack_into("<I", image, 0x30, rng.getrandbits(32))
    struct.pack_into("<I", image, 0x44, parent)
    struct.pack_into("<I", image, 0x1DC, rng.getrandbits(32))
    struct.pack_into("<I", image, 0x1EC, rng.getrandbits(32))
    struct.pack_into("<I", image, 0x274, rng.getrandbits(32))
    for k in range(3):
        struct.pack_into("<f", image, 0x140 + 4 * k, rng.uniform(-100.0, 100.0))
    for k in range(4):
        struct.pack_into("<f", image, 0x100 + 4 * k, rng.uniform(-1.0, 1.0))
    vt = bytearray(0x15C)
    struct.pack_into("<I", vt, 0, 0x519950)
    struct.pack_into("<I", vt, 0x158, 0x4E5EE0)
    parent_vt = bytearray(0x15C)
    struct.pack_into("<I", parent_vt, 0x158, ret)
    writes = [(car, bytes(image)), (car_vtable, bytes(vt)),
              (parent, struct.pack("<I", parent_vtable)), (parent_vtable, bytes(parent_vt)), (ret, b"\xc3")]
    return [], writes, car


def vf158_solid_direct(mu, heap, rng):
    """Use the same solid fixture to call E43Vf158 directly, separate from Vf158x's tail-call wrapper."""
    return vf158_direct(mu, heap, rng)


def update_mrr_nodes(mu, a):
    """The damage object at car+0x3cc: its first word (the vtable) and the vtable's slot 4 (S10), and the signal global."""
    damage = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x3CC, 4)))[0]
    vtable = struct.unpack("<I", bytes(mu.mem_read(damage, 4)))[0]
    return [(damage, 4), (vtable + 0x10, 4), (0x893600, 4)]


def update_mrr_dispatch(mu, a):
    """The damage object's slot 4 (S10), called on the object itself."""
    damage = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x3CC, 4)))[0]
    return [(vtarget(mu, damage, 4), 0)]


def update_mrr_fuzz(mu, args, rng, put):
    """Each entry of the lists at +0x480 and +0x490 is cleared at random (the calls on a null entry are skipped); the
    other entries keep the objects the dump holds."""
    this = args[0]
    for base in (0x480, 0x490):
        for i in range(4):
            if rng.random() < 0.5:
                put(this + base + 4 * i, 0, "<I")


def xform8130_nodes(mu, a):
    """The frame global (0x7cbb00), which ComposeA takes as its target."""
    return [(0x7CBB00, 4)]


def xform8130_direct_args(rng, this):
    """Direct calls: a and b are two scratch blocks in the car (outside the windows the step uses)."""
    return [this + 0x300, this + 0x320]


def xform8130_fuzz(mu, args, rng, put):
    """The input block a (the body's words, random)."""
    for i in range(4):
        put(args[1] + 4 * i, rng.uniform(-1, 1))


def update9af0_direct_args(rng, this):
    """The argument: 0 (the early return), 1 to 5 (the flow path and the probe path below 6). Larger values were
    not run: the original stops in its log path (CreateFileA from 0x401c34) with them, so they are out of its range."""
    return [rng.choice([0, 1, 2, 3, 4, 5, 6, 7])]


def update9af0_fuzz(mu, args, rng, put):
    """The car's words the function reads: +0x1668 (zero or not), the float at +0x9ec (>= 1 takes the flow path), +0x9e8
    (the probe's gate). The timer: the object at world+0x40 (its pointer at +4 holds doubles at +0x38 and +0x50, and
    the word at +0x58 chooses which) and the double at world+0x20's pointer + 0x20 (the argument's gap)."""
    this = args[0]
    put(this + 0x1668, 0, "<I")
    put(this + 0x9EC, rng.uniform(-1, 2))
    put(this + 0x9E8, rng.uniform(-1, 12))
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    obj = struct.unpack("<I", bytes(mu.mem_read(world + 0x40, 4)))[0]
    inner = struct.unpack("<I", bytes(mu.mem_read(obj + 4, 4)))[0]
    put(inner + 0x38, rng.uniform(0, 10), "<d")
    put(inner + 0x50, rng.uniform(0, 20), "<d")
    put(inner + 0x58, 0 if rng.random() < 0.5 else 1, "<I")
    at = struct.unpack("<I", bytes(mu.mem_read(world + 0x20, 4)))[0]
    put(at + 0x20, rng.uniform(0, 20), "<d")


def step_direct_args(rng, this):
    """The frame time dt (its bits are the stack argument)."""
    return [struct.unpack("<I", struct.pack("<f", rng.choice([1 / 60, 1 / 100, 0.02, 0.1])))[0]]


def vf6cx_nodes(mu, a):
    """The words Vf6cx reads of its wheels: +0xebc of the wheels at +0x470 (the count), and +0x650 of the wheels at +0x480
    (the flag pointer, and the two words it points to). Also the world global (0x1659b70), the world object (+0x20 and
    +0x40 are read) and the timer card (+0x18 and the double at +0x20)."""
    out = [(0x1659B70, 4)]
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    out.append((world, 0x48))
    card = struct.unpack("<I", bytes(mu.mem_read(world + 0x20, 4)))[0]
    if card:
        out.append((card, 0x28))
    for i in range(4):
        counted = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x470 + 4 * i, 4)))[0]
        if counted:
            out.append((counted + 0xEBC, 4))
        wheel = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x480 + 4 * i, 4)))[0]
        if wheel:
            flag = struct.unpack("<I", bytes(mu.mem_read(wheel + 0x650, 4)))[0]
            out.append((wheel + 0x650, 4))
            if flag:
                out.append((flag, 8))
    return out


def wheel_flag_nodes(mu, a):
    """The flag words of the wheel (`a[0]`): the two words at its flag pointer (+0x650), when the pointer is set and maps.
    S80 is also the solids' E43Vf80, so an object without the field (an unmapped +0x650 or flag pointer) has no window."""
    try:
        flag = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x650, 4)))[0]
        if flag:
            mu.mem_read(flag, 8)
    except UcError:
        return []
    return [(flag, 8)] if flag else []


def vf6cx_dispatch(mu, a):
    """The vtable targets of the four wheels at +0x480 that Vf6cx calls: S80 (slot 32) and S90 (slot 36)."""
    out = []
    for i in range(4):
        wheel = struct.unpack("<I", bytes(mu.mem_read(a[0] + 0x480 + 4 * i, 4)))[0]
        if wheel:
            out += [(vtarget(mu, wheel, 32), 0), (vtarget(mu, wheel, 36), 0)]
    return out


def step_fuzz(mu, args, rng, put):
    """The target seed (+0x16a4), the slew (+0xafc) and the steering target (+0xb0c), and the def's two gains (+0x22c
    and +0x23c, nonzero). The snapshot flag (+0x9e4) stays 0: its call, Snap9360, is not run by the direct calls."""
    this = args[0]
    put(this + 0x16A4, rng.choice([0.0, rng.uniform(-3, 3), rng.uniform(-30, 30)]))
    put(this + 0xAFC, rng.uniform(-2, 2))
    put(this + 0xB0C, rng.uniform(-1, 1))
    pdef = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    put(pdef + 0x22C, rng.uniform(0.5, 2.0))
    put(pdef + 0x23C, rng.uniform(0.5, 2.0))


def damp_fuzz(mu, args, rng, put):
    """The two damper cells' first and fifth lanes (their difference is the force) and the setup's two gains in the
    def (at +0x4460 and +0x4470)."""
    this = args[0]
    for i in range(2):
        cell = this + 0xA28 + 0x40 * i
        put(cell, rng.uniform(-3, 3))
        put(cell + 0x10, rng.uniform(-3, 3))
    pdef = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    put(pdef + 0x4460, rng.uniform(0.5, 2.0))
    put(pdef + 0x4470, rng.uniform(0.5, 2.0))


def vf64x_nodes(mu, a):
    """The world, and the keeper's words from +0x160b0 (the wind, the time keeper's gate at +0x160f4 and the flag bit 0x10 at
    +0x160e0)."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    return [(0x1659B70, 4), (world, 0x40), (keeper + 0x160B0, 0x48)]


def vf64x_fuzz(mu, args, rng, put):
    """The drag's and the aero gust's inputs (drag_fuzz and sim7_fuzz), the vector at +0x15d0 and its scalar at +0x15d4,
    the flag at +0x16ac, the keeper's bit 0x10, and the def's three inertia words at +0x130 (nonzero divisors)."""
    this = args[0]
    drag_fuzz(mu, args, rng, put)
    sim7_fuzz(mu, args, rng, put)
    for k in range(3):
        put(this + 0x15D0 + 4 * k, rng.uniform(-2, 2))
    put(this + 0x15D4, rng.uniform(-2, 2))
    # +0x16ac is the object 0x495090 takes as this (the asm leaves it in ecx): zero, or a scratch object in the car.
    put(this + 0x16AC, 0 if rng.random() < 0.5 else this + 0x1700, "<I")
    for k in range(0x40 // 4):
        put(this + 0x1700 + 4 * k, rng.uniform(-2, 2))
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    put(keeper + 0x160E0, 0x10 if rng.random() < 0.5 else 0, "<I")
    pdef = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    for k in range(3):
        put(pdef + 0x130 + 4 * k, rng.uniform(0.5, 2.0))


def sim7_nodes(mu, a):
    """The time keeper's world object, its keeper (the pointer at world+0x24) and the wind (the keeper's words at
    +0x160a0 + 0x10, the gust vector)."""
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    return [(0x1659B70, 4), (world, 0x40), (keeper + 0x160B0, 0x10)]


def sim7_fuzz(mu, args, rng, put):
    """The brake-wall penetration (the gate and the wall weight), the velocity (the speed; one call in ten is nearly
    still, under the 0.001 gate), the seed (the accumulator at +0x370; its z decides the gate), the spin (the angular
    velocity's z), the wind, and the inverse mass in the def (its inverse is the wind's scale)."""
    this = args[0]
    put(this + 0x166C, rng.uniform(-0.5, 1.5))
    still = rng.random() < 0.1
    for k in range(3):
        put(this + 0x1C0 + 4 * k, rng.uniform(-5e-4, 5e-4) if still else rng.uniform(-30, 30))
        put(this + 0x370 + 4 * k, rng.uniform(-50, 50))
    put(this + 0x198, rng.uniform(-1, 1))
    world = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    keeper = struct.unpack("<I", bytes(mu.mem_read(world + 0x24, 4)))[0]
    for k in range(3):
        put(keeper + 0x160B0 + 4 * k, rng.uniform(-3, 3))
    pdef = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    put(pdef + 0x16C, rng.uniform(0.5, 2.0))


def append_nodes(mu, a):
    """The members of the head's list, walked from its first link at entry (the drive's neighbours can lie outside the
    head window): each member's 12 bytes."""
    head = a[0]
    link = struct.unpack("<I", bytes(mu.mem_read(head, 4)))[0]
    count = struct.unpack("<I", bytes(mu.mem_read(head + 8, 4)))[0]
    nodes = []
    for _ in range(min(count, 8)):
        if not link:
            break
        nodes.append((link, 0x0C))
        link = struct.unpack("<I", bytes(mu.mem_read(link, 4)))[0]
    return nodes


def motion_direct_args(rng, this):
    """MotionProbe's out word: a scratch word in the car, outside the windows the step uses."""
    return [this + 0x300]


def motion_fuzz(mu, args, rng, put):
    """The velocity, from slow (the probe goes on) to fast (the speed gate, 1.5 m/s): each lane is drawn at a random
    scale, so the length crosses 1.5 on both sides. The gate word (+0x1ec, S8C) is set one call in four. M5666b0 returns the flow's word at +0x17854 (when the flow's pointer
    at +0xc98 is tagged 0x7b) or the global at 0x166084c; both are zeroed half the time, so the probe goes on."""
    scale = rng.choice([0.5, 1.0, 3.0])
    for k in range(3):
        put(args[0] + 0x1C0 + 4 * k, rng.uniform(-scale, scale))
    # S8C (0x5293e0) returns the word at +0x1ec: nonzero stops the probe at once.
    put(args[0] + 0x1EC, 1 if rng.random() < 0.25 else 0, "<I")
    if rng.random() < 0.5:
        put(0x166084C, 0, "<I")
        flow = struct.unpack("<I", bytes(mu.mem_read(0x165FA90, 4)))[0]
        target = struct.unpack("<I", bytes(mu.mem_read(flow + 0xC98, 4)))[0]
        if bytes(mu.mem_read(target, 2)) == b"\x7b\x00":
            put(target + 0x17854, 0, "<I")


def append_direct_args(rng, this):
    """AppendA980's link `a`: one of the four link slots in the head's window (the fuzz links it or leaves it out)."""
    return [this + 0x20 + 0x10 * rng.randrange(4)]


def append_fuzz(mu, args, rng, put):
    """A valid state for the two lists in the head window and the link `a`: the head list (at this) and a second list
    (at this+0x80) take random subsets of the other slots in random order; `a` is in the head list, in the second one,
    or in neither. Each list has its first, last and count words; a member's owner is its list's head address."""
    this, a = args[0], args[1]
    slots = [this + 0x20 + 0x10 * k for k in range(4)]
    others = [x for x in slots if x != a]
    lists = {this: [], this + 0x80: []}
    where = rng.choice([this, this + 0x80, None])
    for x in others:
        target = rng.choice([this, this + 0x80, None])
        if target is not None:
            lists[target].append(x)
    if where is not None:
        lists[where].append(a)
    for head, members in lists.items():
        rng.shuffle(members)
        for i, x in enumerate(members):
            put(x, members[i + 1] if i + 1 < len(members) else 0, "<I")
            put(x + 4, members[i - 1] if i > 0 else 0, "<I")
            put(x + 8, head, "<I")
        put(head, members[0] if members else 0, "<I")
        put(head + 4, members[-1] if members else 0, "<I")
        put(head + 8, len(members), "<I")
    for x in others + [a]:
        if all(x not in m for m in lists.values()):
            for k in (0, 4, 8):
                put(x + k, 0, "<I")


def neg_branch(args, windows):
    return "neg" if args[2] else "pos"


SPECS = {
    # args: this, out (out is the 4-float quaternion; eax is out). this is the 3x3 matrix, nine floats.
    0x4E4D10: dict(name="E34Quat::ToQuat", popped=4, x87=False, thiscall=True, branch=quat_branch,
                   fuzz=quat_fuzz, windows=[(0, lambda a: 0x24), (1, lambda a: 0x10)]),
    # args: this, i, neg. The lane's first word at +0xa48 + 0x40 i, negated when neg (x87 fchs).
    0x4E6730: dict(name="CCar::GetA48", popped=8, x87=True, thiscall=True, branch=neg_branch,
                   windows=[(0, lambda a: 0xB10)]),
    # no stack args; eax is this + 0xa00 (the state object).
    0x4E65B0: dict(name="CCar::StateAtb", popped=0, x87=False, thiscall=True, windows=[]),
    # no stack args; st0 is the word at +0xb0c, and +0xb1c for GetB1C.
    0x4E65E0: dict(name="CCar::GetB0C", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0xB20)]),
    0x4E65F0: dict(name="CCar::GetB1C", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0xB20)]),
    # args: this, v. Sets the dirty flag of the CReal at +0xad0 (external SetDirty), then +0xadc += v (at its +0xc).
    0x4E6660: dict(name="CCar::AddADC", popped=4, x87=False, thiscall=True, windows=[(0, lambda a: 0xAE0)]),
    # args: this, i, j, v. SetDirty on the CReal at (j + 4i) * 0x10 + 0xa1c (external), then its value is v.
    0x4E66A0: dict(name="CCar::SetA1C", popped=0xC, x87=False, thiscall=True, windows=[(0, lambda a: 0xB20)]),
    # no stack args. The timer, speed and demand; the filtered torque's CReal at +0xae0 (dirty flag then value); the setup
    # word at def+0x23c (the def is at this+0x34, its window is the pointer chain).
    0x4E7FF0: dict(name="CCar::UpdateFfbTorque", popped=0, x87=False, thiscall=True, branch=ffb_torque_branch,
                   fuzz=ffb_torque_fuzz, windows=[(0, lambda a: 0x1680), (0, lambda a: 0x240, 0x34)]),
    # args: this, a, b (16-byte vectors). The body at +0x4a0 is read. The asm (0x4e6870): S30 of the body (slot 12, the
    # identity 0x519c00) with b pushed, S30 of this with t and a pushed, then ComposeA(a, t, b) on this (0x51b4c0). The
    # C++ has ComposeA(b, t, b): the first argument differs.
    0x4E6870: dict(name="CCar::Xform6870", popped=8, x87=False, thiscall=True, fuzz=xform_fuzz,
                   direct_args=xform_direct_args, windows=[(0, lambda a: 0x4A4), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # E33 identity getter (slot 12): returns its this. Callers push arguments it does not pop (args: this).
    0x519C00: dict(name="E33 S30 identity", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # E33Mat34::ComposeA (args: this, a, b, out; ret 0xc): copies a to out, then walks. Xform6870 calls it as
    # ComposeA(a, t, b) on the car; the outer window holds its effect on b.
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    # E43PairWalk::MB560 (args: receiver this, a, b, c; ret 0xc): c = a, then the walk. Its effect is the outer window;
    # a (args[1]) and c (args[3]) are also recorded: DragAndSpinDamp walks its vectors through it.
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    # args: this (the list head: first, last, count at +0, +4, +8), a (the link: next, prev, owner at +0, +4, +8). Appends
    # a to this. When a is linked to an owner, it is unlinked first (CBodyLink::Unlink, 0x4e0020, external).
    # CCar::AppendA980(a) on the list head `this` (E34LinkList10: first, last, count). The head window holds the links
    # the fuzz builds (slots at this+0x20 + 0x10 * k, a second list at this+0x80) so every branch is reachable; `a` is
    # the drive's own link (outside that window) and is windowed on its own.
    0x4EA980: dict(name="CCar::AppendA980", popped=4, x87=False, thiscall=True,
                   direct_args=append_direct_args, fuzz=append_fuzz, nodes=append_nodes,
                   windows=[(0, lambda a: 0x100), (1, lambda a: 0x0C)]),
    # CCar::MotionProbe(out) (0x4e7730, `ret 4`): the gate (S8C, slot 35), the speed (velocity length, 1.5), E60Flow's
    # M5666b0, ToWorld((0, 0, 1)) for the sign of the dot, and the sampler (0x4e54a0) whose verdict words (big+0x40,
    # big+0x44) decide the result. Its out word is windowed.
    0x4E7730: dict(name="CCar::MotionProbe", popped=4, x87=False, thiscall=True,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 35), 0)], direct_args=motion_direct_args,
                   fuzz=motion_fuzz, windows=[(0, lambda a: 0x1D0), (1, lambda a: 4)]),
    0x5666B0: dict(name="E60Flow::M5666b0", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E68E0: dict(name="CCar::ToWorld", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E54A0: dict(name="CCar::Sampler54A0", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 0x50), (3, lambda a: 0x10)]),
    0x4E0020: dict(name="CBodyLink::Unlink", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x0C)]),
    # DragAndSpinDamp (Sim7DC0, 0x4e7dc0): the drag force and the spin damping, sent to S70 and S74 (slots 28, 29). The
    # externals: the gate (S00 when the time keeper's flags set bit 0x4000), E09Vec4::Clear, Length, Scale, the S30
    # identity (slot 12, which the asm calls with a pushed buffer it does not pop), MB560, and the cross product.
    0x4E7DC0: dict(name="CCar::DragAndSpinDamp", popped=0, x87=False, thiscall=True,
                   nodes=lambda mu, a: drag_nodes(mu, a), fuzz=drag_fuzz,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 0), 0), (vtarget(mu, a[0], 12), 0),
                                           (vtarget(mu, a[0], 28), 4), (vtarget(mu, a[0], 29), 4)],
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x4498, 0x34)]),
    # CCar::Update9AF0(arg) (0x4e9af0, ret 4): returns on arg 0 or when the timer gap (0x4a0290, a double) is below 5.
    # Then the flow (0x566640 on the global 0x165fa90) and the S8C gate (slot 35) pick the emits; otherwise the probe
    # (MotionProbe, 0x4e7730) decides the code. Its emits (0x4e8770, 0x4e88f0 on the signal objects at 0x893600) and
    # 0x4e8df0 are external. The world's words at +0x20 and +0x40 are read.
    # CCar::Vf6cx (0x4ea300, ret 4): the car's per-step composite. Its externals (the passenger, scratch, drive and solid
    # steps, the brake-wall raycast and impulse, the wheel and rigid-body loops, the drive line's thunk, the timekeeper,
    # the position and state emits) are answered from the records; the wheels' virtual targets come from `dispatch`.
    0x4EA300: dict(name="CCar::Vf6cx", popped=4, x87=False, thiscall=True, nodes=vf6cx_nodes, dispatch=vf6cx_dispatch,
                   windows=[(0, lambda a: 0x16B0), (0, lambda a: 0x44B4, 0x34)]),
    0x523160: dict(name="E?::CPassenger::Step (523160)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x536F70: dict(name="E?::CScratchSim::NopA (536f70)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x5043B0: dict(name="CDrive::Nop3B0 (5043b0)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x52CC30: dict(name="CSolid::E43Vf6C (52cc30)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x16B0)]),
    0x4E4960: dict(name="CBrakeWallManager::RaycastEntry (4e4960)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 0x1C)]),
    0x529640: dict(name="E46Impulse::Impulse640 (529640)", popped=0x10, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x16B0), (1, lambda a: 0x1C)]),
    0x4E4490: dict(name="CMultiSignal1::Emit brake wall (4e4490)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
    # The wheel's virtual targets (S80, S90) and SimFD00 change the wheel's flag words (at the pointer at +0x650), which
    # Vf6cx reads after the calls: the node window carries them (none when the pointer is null).
    0x5293D0: dict(name="CWheel::S80 (5293d0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[], nodes=wheel_flag_nodes),
    0x52BF30: dict(name="CWheel::S90 (52bf30)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[], nodes=wheel_flag_nodes),
    0x51FD00: dict(name="CWheel::SimFD00 (51fd00)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[], nodes=wheel_flag_nodes),
    0x521D90: dict(name="E?::Vf6c (521d90)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x521940: dict(name="E?::Test M1940 (521940)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x522100: dict(name="E?::StepBig (522100)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x51A060: dict(name="E?::ExclA060 (51a060)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x50A4F0: dict(name="CDriveLine::Thunk4F0 (50a4f0, jumps to 475ea0)", popped=4, x87=True, thiscall=True, external=True,
                   windows=[]),
    0x4A0540: dict(name="NRegulation::CTimeKeeper::Record (4a0540)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 8)]),
    0x4A0320: dict(name="NRegulation::CTimeKeeper::Update (4a0320)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[]),
    0x504540: dict(name="CDrive::Get4540 (504540)", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x4E8B30: dict(name="CMultiSignal1::Emit position (4e8b30)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x14)]),
    0x4E8BF0: dict(name="CMultiSignal1::Emit MrR (4e8bf0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0xC)]),
    # CCar::StepEngine (0x4e9930, ret 4): the steering cells (slew +0xaf0, steering target +0xb00, its rate +0xb10, the
    # demand +0xad0, each dirtied first), then the scratch sim, drive and entity steps, Damp66D0, UpdateFfbTorque and,
    # when +0x9e4 is set, Snap9360. Only its SetDirty calls (0x52f450) are recorded; the other externals are not.
    0x4E9930: dict(name="CCar::StepEngine", popped=4, x87=False, thiscall=True, fuzz=step_fuzz,
                   direct_args=step_direct_args, windows=[(0, lambda a: 0x16B0), (0, lambda a: 0x4498, 0x34)]),
    0x4E9AF0: dict(name="CCar::Update9AF0", popped=4, x87=False, thiscall=True,
                   nodes=update9af0_nodes, fuzz=update9af0_fuzz, direct_args=update9af0_direct_args,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 35), 0)],
                   windows=[(0, lambda a: 0x1680)]),
    # CCar::UpdateSeed(p) (0x4e91f0, ret 4): the seed (p, or a local one when +0x1670 is positive) goes to ReadPacket,
    # NopD570 and M5970. Then E19IsState2 gates the flags word (p+0x14): bit 0x20 runs S8C (slot 35) and, when that
    # returns 0, S158 (slot 86); otherwise bit 0x10 runs MotionProbe and one emit (0x4e8a70 with (out, 1), or Assist8DF0
    # when the probe returns nonzero). Last, the step counter (+0x950) stores the timer card's word and p+0x10 for the
    # first four thousand-step marks, then counts up.
    0x4E91F0: dict(name="CCar::UpdateSeed", popped=4, x87=False, thiscall=True, nodes=update_seed_nodes,
                   fuzz=update_seed_fuzz, direct_args=update_seed_direct_args,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 35), 0), (vtarget(mu, a[0], 86), 0)],
                   windows=[(0, lambda a: 0x16B0), (1, lambda a: 0x20)]),
    # CCar::Vf158x (slot 86): its slot 0, app-root flag and solid's slot-86 event path.
    0x4E5EE0: dict(name="CCar::Vf158x", popped=0, x87=False, thiscall=True, nodes=vf158_nodes,
                   direct_fuzz=vf158_direct, windows=[(0, lambda a: 0x3D0)]),
    0x519950: dict(name="CEntity::Vf00 (519950)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x52C030: dict(name="CSolid::E43Vf158 (52c030)", popped=0, x87=False, thiscall=True, external=True,
                   direct_fuzz=vf158_solid_direct, nodes=vf158_nodes, windows=[(0, lambda a: 0x3D0)]),
    0x519AA0: dict(name="CEntity::E43Vf158 (519aa0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[]),
    0x51F7C0: dict(name="CMultiSignal1::Emit<CSigMrRBodyEvent>", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x50)]),
    0x537F20: dict(name="CScratchSim::ReadPacket (537f20)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x20)]),
    0x50D570: dict(name="E41Plain::NopD570 (50d570)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x20)]),
    0x505970: dict(name="CDrive::M5970 (505970)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x20)]),
    0x499900: dict(name="E19IsState2 (499900)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E8A70: dict(name="CMultiSignal1::Emit car problem (4e8a70)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 8)]),
    # CCar::UpdateMrr (0x4e9c70, ret 0): wires the car to its damage object (S10, vtable slot 4, on the object at +0x3cc),
    # then the light system, the drive and the rigid bodies (setters and E45 calls on the lists at +0x470, +0x480 and
    # +0x490), UpdateSeed on a local block, and the emit 0x4e8cb0 with the pair (the car, the word at +0x30). Its own write is
    # +0x9f4 (the damage object).
    0x4E9C70: dict(name="CCar::UpdateMrr", popped=0, x87=False, thiscall=True, nodes=update_mrr_nodes,
                   dispatch=update_mrr_dispatch, fuzz=update_mrr_fuzz, windows=[(0, lambda a: 0x16B0)]),
    0x503AB0: dict(name="CDrive::Set170 (503ab0)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x50D310: dict(name="E41Plain::Set048 (50d310)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x4F1E00: dict(name="CCarLightSystem::Attach (4f1e00)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x53A860: dict(name="E?::Vf860 (53a860)", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    0x53D030: dict(name="E?::Vf030 (53d030)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x51DA50: dict(name="E?::Set650 (51da50)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x51EFE0: dict(name="E?::Vfe0 (51efe0)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x521910: dict(name="E45Smalls::M1910 (521910)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x521980: dict(name="CMountedRigidBody::E45_M1980 (521980)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[]),
    0x503AA0: dict(name="CDrive::Ret3AA0 (503aa0)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E8CB0: dict(name="CMultiSignal1::Emit (4e8cb0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 8)]),
    # CCar::Xform8130(a, b) (0x4e8130, ret 8): BodyCompose(a, m1) into a 28-byte local, the identity S30 (slot 12) on the
    # car, then ComposeA(m1, frame, b) on what S30 returns (the frame global 0x7cbb00). Returns b.
    0x4E8130: dict(name="CCar::Xform8130", popped=8, x87=False, thiscall=True, nodes=xform8130_nodes,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 12), 0)], direct_args=xform8130_direct_args,
                   fuzz=xform8130_fuzz, windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x4E6A30: dict(name="CCar::BodyCompose (4e6a30)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x1C)]),
    # CCar::Sample9800 (0x4e9800, ret 0): the sensor sampler, called from Vf28x. Its engine and drive getters are
    # externals (0x50eb80, 0x504530, 0x504540, 0x50ee60, 0x50e970, 0x50d380, 0x50e980); the emit (0x4e8270) takes the
    # payload (16 words, 0x40 bytes).
    0x4E9800: dict(name="CCar::Sample9800 (4e9800)", popped=0, x87=False, thiscall=True, nodes=sample9800_nodes,
                   fuzz=sample9800_fuzz,
                   windows=[(0, lambda a: 0x16B0), (0, lambda a: 0x44B4, 0x34), (0, lambda a: 0x90, 0x620)]),
    0x50EB80: dict(name="E?::Engine GetState188 (50eb80)", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x504530: dict(name="CDrive::Get4530 (504530)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x50EE60: dict(name="E?::Engine SimEE60 (50ee60)", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x50E970: dict(name="E?::Engine SimE970 (50e970)", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x50D380: dict(name="E?::Engine Get48_58 (50d380)", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x50E980: dict(name="E?::Engine SimE980 (50e980)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E8270: dict(name="CMultiSignal1::Emit sensor (4e8270)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x40)]),
    0x4A0290: dict(name="E?::TimerGap (4a0290)", popped=4, x87=True, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x60), (1, lambda a: 0x8)]),
    0x566640: dict(name="E60Flow::Check (566640)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E8DF0: dict(name="CCar::Assist8DF0 (4e8df0)", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    0x4E8770: dict(name="CMultiSignal::Emit (4e8770)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x4)]),
    0x4E88F0: dict(name="CMultiSignal::Emit (4e88f0)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x8)]),
    0x4EC0D0: dict(name="CCar::Test3B20 (4ec0d0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x40)]),
    0x504840: dict(name="E?::Test4B0 (504840)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x50ECF0: dict(name="E?::Test690 (50ecf0)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CCar::Vf64x (0x4e9020, no stack arguments): the drag and the aero gust (both listed, their externals recorded), the
    # walk of the angular velocity (MB560) crossed with the def's inertia words and sent to S74; the externals 0x5224f0
    # (car+0x8b0), 0x52cb20 (it writes the vector at +0x380, which the walk reads), the walk of +0x380, the blend of
    # +0x15d0, 0x495090 (the keeper's bit 0x10 and +0x16ac), and the four objects at +0x490 (0x521920 test, 0x521740
    # action). The identity S30 (slot 12) between them is not modelled: it leaves the car as it is.
    0x4E9020: dict(name="CCar::Vf64x", popped=0, x87=False, thiscall=True,
                   nodes=vf64x_nodes, fuzz=vf64x_fuzz,
                   windows=[(0, lambda a: 0x16B0), (0, lambda a: 0x4498, 0x34)]),
    0x5224F0: dict(name="CCar::Vf64x member 8B0 (5224f0)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x52CB20: dict(name="CCar::Vf64x (52cb20)", popped=0, x87=False, thiscall=True, external=True,
                   nodes=lambda mu, a: [(a[0] + 0x380, 0x10)], windows=[]),
    0x495090: dict(name="E?::Vf64x send (495090)", popped=8, x87=False, thiscall=True, external=True, windows=[]),
    0x521920: dict(name="E?::Test580 (521920)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x521740: dict(name="E?::Act (521740)", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CCar::Damp66D0 (0x4e66d0, no stack arguments): two dampers at +0xa28 (stride 0x40). Each CReal at +0x14 of the
    # cell gets 0 - (lane0 - lane4) * gain, gain at def + 0x4460 + 0x10 i (SetDirty on the CReal is the external call).
    0x4E66D0: dict(name="CCar::Damp66D0", popped=0, x87=False, thiscall=True, fuzz=damp_fuzz,
                   windows=[(0, lambda a: 0xA90), (0, lambda a: 0x4484, 0x34)]),
    # CCar::Sim7AD0 (0x4e7ad0, no stack arguments): the wheel aero gust. It returns unless the brake-wall penetration
    # (+0x166c) is above 0, the seed's z is not below 0 and the horizontal speed is above 0.001. Then ToWorld(seed),
    # two MB560 walks, and the S70 and S74 sends (slots 28 and 29). The wind is the world keeper's gust vector.
    0x4E7AD0: dict(name="CCar::Sim7AD0", popped=0, x87=False, thiscall=True,
                   nodes=sim7_nodes, fuzz=sim7_fuzz,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 28), 4), (vtarget(mu, a[0], 29), 4)],
                   windows=[(0, lambda a: 0x1670), (0, lambda a: 0x170, 0x34)]),
    # The slot 28 and 29 targets (S70 and S74) take a vector: they add it to the car's accumulators (+0x370, +0x310).
    # The node window after the vector is the accumulator after the call (read by Vf64x's test, which feeds it on).
    0x529340: dict(name="CCar::S70 (slot 28)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)], nodes=lambda mu, a: [(a[0] + 0x370, 0xC)]),
    0x529EA0: dict(name="CCar::S74 (slot 29)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)], nodes=lambda mu, a: [(a[0] + 0x310, 0xC)]),
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x473B30: dict(name="E01Vec4::Length", popped=0, x87=True, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x473CD0: dict(name="E01Vec4::Scale", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: 0x10)]),
    0x43C630: dict(name="E01VecSetA::Cross", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CReal::SetDirty: the flag at +8 is set to 1 (no stack args, thiscall on the CReal).
    0x52F450: dict(name="NTweak::CBase::SetDirty", popped=0, x87=False, thiscall=True, external=True, windows=[]),
}
