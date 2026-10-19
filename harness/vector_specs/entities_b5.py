# B5, the hinged body's limit and rotation sims (src/rbr/RBR_E44_SimA.cpp): SimC960 (the rotation block, from the hinge
# angle through the CRT sin and cos), SimCF10 (one hinge limit's residual) and SimEA80 (the sum over the two limits, which
# calls SimCF10 directly). Records from the drive's snapshots (steps 40, 572, 1295). See
# rust/rbr-physics/src/entities/README.md (B5).
SET_NAME = "entities_b5"
from unicorn.x86_const import UC_X86_REG_ESP  # noqa: E402 (the fuzz hooks read the return address)


def hinge_def_nodes(mu, a):
    """The hinged body's definition (its pointer at +0x34): the limit blocks (0x1f0..0x2b0) and their values."""
    return [(word(mu, a[0] + 0x34), 0x300)]


HINGE_CODE = (0x51C000, 0x520000)  # the hinge sims' code: a call from here is one the parent sim makes


def called_from_hinge(mu):
    """Whether the call at entry returns into the hinge sims: its inputs are then the parent's values, so no fuzz."""
    return HINGE_CODE[0] <= word(mu, mu.reg_read(UC_X86_REG_ESP)) < HINGE_CODE[1]


SIMD160_SIMC960_RETURN = 0x51D1C3  # the return address of SimD160's call of SimC960 (0x51d1be)


def fuzz_angle(mu, args, rng, put):
    """SimC960's hinge angle: a random angle over a few turns, so the sine and cosine see more than the drive's angles. Not
    when SimD160 calls it: the angle is SimD160's input, which its own record has at entry."""
    if word(mu, mu.reg_read(UC_X86_REG_ESP)) == SIMD160_SIMC960_RETURN:
        return
    put(args[0] + 0x3FC, rng.uniform(-3.2, 3.2), "<f")


def fuzz_hinge(mu, args, rng, put):
    """SimEA80's inputs at its entry, before its windows are taken: the limit states, the hinge angle and rate, and each
    limit's reference, step clamp, stop and damping coefficients, so the sum takes the residual's branches. Set at entry
    (not inside its calls), so its port and its inner SimCF10 calls read the same values."""
    if called_from_hinge(mu):
        return
    this = args[0]
    limits = word(mu, this + 0x34) + 0x1F0
    for idx in range(2):
        put(this + 0x3E8 + 4 * idx, rng.uniform(-0.5, 0.5), "<f")
        block = limits + 0x60 * idx
        put(block + 0x5C, rng.uniform(-0.5, 0.5), "<f")
        put(block + 0x4C, rng.uniform(0.0005, 0.1), "<f")
        put(block + 0x2C, rng.uniform(-100.0, 100.0), "<f")
        put(block + 0x3C, rng.uniform(-5.0, 5.0), "<f")
    put(this + 0x3FC, rng.uniform(-0.5, 0.5), "<f")
    put(this + 0x40C, rng.uniform(-2.0, 2.0), "<f")


def slip_nodes(mu, a):
    """SimCFF0's reads outside its window: the identity global (0x7cbb00, the second walk's receiver), the definition (the axis at +0x360, the scale, friction and anchor at +0x2c0..
    +0x34c and +0x320) and the car's state word at +0xc00 (the state object is car+0xa00; the vector at +0xc10 is read by the
    copy inside the window of +0xc00..+0xc20)."""
    return [(word(mu, a[0] + 0x34), 0x380), (word(mu, a[0] + 0x3D0) + 0xA00 + 0xC00, 0x20), (0x7CBB00, 4)]


def fuzz_slip(mu, args, rng, put):
    """SimCFF0's inputs at its entry, before its walks: the friction (0 in the drive, which zeroes the term), the scale, and
    the car's +0xc00 word, so the term takes both signs and the clamped-friction branch. The axis object (+0x360, the first
    walk's a: zero in the drive, which zeroes the dot product) is fuzzed too."""
    if called_from_hinge(mu):
        return
    this = args[0]
    definition = word(mu, this + 0x34)
    state = word(mu, this + 0x3D0) + 0xA00
    put(definition + 0x34C, rng.choice((0.0, rng.uniform(-2.0, 2.0))), "<f")
    for q in range(3):
        put(definition + 0x2C0 + 4 * q, rng.uniform(-3.0, 3.0), "<f")
    for q in range(4):
        put(definition + 0x360 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    put(state + 0xC00, rng.uniform(-1.0, 1.0), "<f")


def solve_nodes(mu, a):
    """SimEAD0's reads outside its window: the definition (+0x16c..+0x4dc, the solve's terms and scale), and the object
    the hinge's +0x650 word points to (its first word is the early-return flag)."""
    return [(word(mu, a[0] + 0x34), 0x500), (word(mu, a[0] + 0x650), 4)]


def fuzz_solve(mu, args, rng, put):
    """SimEAD0's inputs at its entry: the hinge angle, rate and settle term, the definition's mass and b0 (the zero cases
    take their defaults), its 3x3 block, the vectors, the scalars and the rotation block, so the solve takes both rate
    signs and the zero-rate branch, and half the calls unowned. Not the vote: its hit is the original's own decision."""
    if called_from_hinge(mu):
        return
    this = args[0]
    definition = word(mu, this + 0x34)
    put(this + 0x40C, rng.choice((0.0, rng.uniform(-2.0, 2.0))), "<f")
    put(this + 0x3FC, rng.uniform(-0.5, 0.5), "<f")
    put(this + 0x620, rng.uniform(-1.0, 1.0), "<f")
    put(definition + 0x16C, rng.choice((0.0, rng.uniform(0.5, 3.0))), "<f")
    put(definition + 0x4BC, rng.choice((0.0, rng.uniform(0.05, 1.0))), "<f")
    for q in range(9):
        put(definition + 0x3E0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(this + 0x598 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    for q in range(3):
        put(this + 0x600 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(definition + 0x3D0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(definition + 0x2C0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    put(definition + 0x49C, rng.uniform(-0.5, 0.5), "<f")
    put(definition + 0x4CC, rng.uniform(-3.0, 3.0), "<f")
    put(definition + 0x4DC, rng.uniform(-3.0, 3.0), "<f")
    # half the calls unowned (the owner word at +0x3d0 zero): the force pass instead of the solve
    if rng.random() < 0.5:
        put(this + 0x3D0, 0, "<I")


def fb40_nodes(mu, a):
    """SimFB40's reads outside its window: the definition's block (the pose at +0x300, the context, the reference at
    +0x49c and the raw word at +0x4fc), the identity global (0x7cbb00, the ComposeA's `t`) and the hinge's link pointer
    (+0x650) with the word it points to (the link flag)."""
    return [(word(mu, a[0] + 0x34), 0x500), (0x7CBB00, 4), (word(mu, a[0] + 0x650), 4), (a[0] + 0x650, 4)]


def fuzz_fb40(mu, args, rng, put):
    """SimFB40's inputs at its entry: the key at +0x30 (zero takes the ComposeA branch, nonzero the ToQuat branch, and the
    owner car decides too), the angle, the definition's reference and raw word, the link flag, and the four vectors the
    branches read (+0x100 rotation, +0x190, +0x1c0 and the definition's pose at +0x300)."""
    this = args[0]
    definition = word(mu, this + 0x34)
    link = word(mu, this + 0x650)
    put(this + 0x30, rng.choice((0, rng.randrange(1, 1 << 16))), "<I")
    put(this + 0x3FC, rng.uniform(-0.5, 0.5), "<f")
    put(definition + 0x49C, rng.uniform(-0.5, 0.5), "<f")
    put(definition + 0x4FC, rng.randrange(0, 1 << 32), "<I")
    put(link, rng.choice((0, 1)), "<I")
    for q in range(4):
        put(this + 0x100 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(this + 0x190 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(this + 0x1C0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(definition + 0x300 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    for q in range(9):
        put(this + 0x574 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(this + 0x598 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(definition + 0x3E0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    for q in range(3):
        put(definition + 0x3D0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(definition + 0x360 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
    # SimDB40's guard: the CDef's debug flag (0x400000 at +0x160e0), set or clear (the drive has it clear)
    cdef = word(mu, word(mu, 0x1659B70) + 0x24)
    flags = word(mu, cdef + 0x160E0)
    put(cdef + 0x160E0, (flags & ~0x400000) | rng.choice((0, 0x400000)), "<I")


def db40_nodes(mu, a):
    """SimDB40's reads outside its window: the definition's block up to +0x420 (the pose, the axis, the vector and the
    matrix), the identity global (0x7cbb00) and the debug flag's chain: the CPhysics global, its CDef pointer and the
    CDef's flags word (+0x160e0)."""
    definition = word(mu, a[0] + 0x34)
    physics = word(mu, 0x1659B70)
    cdef = word(mu, physics + 0x24)
    return [(definition, 0x420), (0x7CBB00, 4), (0x1659B70, 4), (physics + 0x24, 4), (cdef + 0x160E0, 4)]


def d860_nodes(mu, a):
    """SimD860's reads outside its window: the definition's 3x3 at +0x3e0 (nine floats) and the identity global (0x7cbb00,
    the walk's `b`)."""
    definition = word(mu, a[0] + 0x34)
    return [(definition + 0x3E0, 36), (0x7CBB00, 4)]


SIMEFE0_SIMD860_RETURN = 0x51F1FD  # the return address of SimEFE0's call of SimD860 (0x51f1f8)


def fuzz_d860(mu, args, rng, put):
    """SimD860's inputs at its entry: the rotation row at +0x5a4 (the walk's vector is its product with the definition's
    matrix), the base vector at +0x5c0 (the output's offset), and the definition's matrix. Not when SimEFE0 calls it: its
    inputs are then the parent's values (SimEFE0's own fuzz sets them)."""
    if word(mu, mu.reg_read(UC_X86_REG_ESP)) == SIMEFE0_SIMD860_RETURN:
        return
    this = args[0]
    definition = word(mu, this + 0x34)
    for q in range(3):
        put(this + 0x5A4 + 4 * q, rng.uniform(-1.0, 1.0), "<f")
        put(this + 0x5C0 + 4 * q, rng.uniform(-2.0, 2.0), "<f")
    for q in range(9):
        put(definition + 0x3E0 + 4 * q, rng.uniform(-1.0, 1.0), "<f")


def efe0_nodes(mu, a):
    """SimEFE0's reads outside its window: the definition's block up to +0x500 (the flags, the vectors and the matrices it
    passes on), and the flag word the link at +0x650 points to (its zero test and its clear)."""
    definition = word(mu, a[0] + 0x34)
    return [(definition, 0x500), (word(mu, a[0] + 0x650), 4)]


def fuzz_efe0(mu, args, rng, put):
    """SimEFE0's branches at its entry: the definition's gate (the slot-36 call or the dirty flag), the link flag (the angle
    branch runs when it is zero), the angle words the factor reads (+0x47c, +0x48c, +0x49c), and the grid (AddEdge runs only
    when it is set). Set before the windows are taken, so the port reads the same values."""
    this = args[0]
    definition = word(mu, this + 0x34)
    put(definition + 0x4EC, rng.randrange(2), "<I")
    put(word(mu, this + 0x650), rng.randrange(2), "<I")
    for offset in (0x47C, 0x48C, 0x49C):
        put(definition + offset, rng.uniform(-2.0, 2.0), "<f")
    if rng.random() < 0.5:
        put(this + 0x64, 0, "<I")


def e1d0_nodes(mu, a):
    """SimE1D0's reads outside its window: the car's reference vector (+0x250..+0x26f: the reference words, the height at
    +0x258 and the walk's vector at +0x260), the definition's settle projection (pf, 12 bytes at +0x2c0) and the identity
    global at 0x7cbb00 (the walks' receiver)."""
    car = word(mu, a[0] + 0x3D0)
    definition = word(mu, a[0] + 0x34)
    return [(car + 0x250, 0x20), (definition + 0x2C0, 12), (0x7CBB00, 4)]


SIMEFE0_SIMD160_RETURN = 0x51F1AC  # the return address of SimEFE0's call of SimD160 (0x51f1a7)


def fuzz_d160(mu, args, rng, put):
    """SimD160's inputs at its entry: the definition's frame block (the pose, the walk vectors, the matrices and the scale
    row, +0x320..+0x448) and the hinge angle (+0x3fc, which SimC960 turns into the rotation block). Not when SimEFE0 calls it:
    these are SimEFE0's inputs, and its record has them at entry."""
    if word(mu, mu.reg_read(UC_X86_REG_ESP)) == SIMEFE0_SIMD160_RETURN:
        return
    definition = word(mu, args[0] + 0x34)
    for offset in range(0x320, 0x44C, 4):
        put(definition + offset, rng.uniform(-2.0, 2.0), "<f")
    put(args[0] + 0x3FC, rng.uniform(-3.2, 3.2), "<f")


def d160_nodes(mu, a):
    """SimD160's reads outside its window: the definition's block up to +0x500 (the matrices, the vectors and the pose it
    walks and composes), and the identity global at 0x7cbb00 (the value MB560 takes)."""
    return [(word(mu, a[0] + 0x34), 0x500), (0x7CBB00, 4)]


SPECS = {
    # SimC960 (thiscall, void): s = sin(angle) then c = cos(angle), both CRT calls (x87, cdecl) on the angle at +0x3fc;
    # writes the rotation block at +0x598..+0x5b8. The window covers the angle and the block.
    0x51C960: dict(name="CHingedRigidBody::SimC960", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x5BC)],
                   direct=True, fuzz=fuzz_angle),
    # CRT sin and cos (__CIsin 0x53f3b0, __CIcos 0x53f3a0: flds arg; fsin/fcos; retl). cdecl, one argument (the angle
    # bits), the result in st0 (recorded as st0 bits).
    0x53F3B0: dict(name="CRT sin (__CIsin)", popped=4, x87=True, cdecl=True, windows=[], external=True),
    0x53F3A0: dict(name="CRT cos (__CIcos)", popped=4, x87=True, cdecl=True, windows=[], external=True),
    # SimCF10(idx) (thiscall, one stack argument, ret 4, the residual in st0): this's hinge angle idx (+0x3e8), the hinge
    # angle (+0x3fc) and the rate (+0x40c), the definition's limit block idx. Writes this's angle idx when the limit moves
    # it. The windows cover this (0x410 bytes) and the definition's limit blocks (`nodes`).
    0x51CF10: dict(name="CHingedRigidBody::SimCF10", popped=4, x87=True, thiscall=True, windows=[(0, lambda a: 0x410)],
                   nodes=hinge_def_nodes, direct=True),
    # SimEA80 (thiscall, returns the sum in st0): SimCF10(0) and SimCF10(1) for the limits whose mode is 2, added in f32.
    0x51EA80: dict(name="CHingedRigidBody::SimEA80", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0x410)],
                   nodes=hinge_def_nodes, direct=True, fuzz=fuzz_hinge),
    # SimCFF0 (thiscall, no stack argument, the slip term in st0): two walks (MB560 below) and the car's state word; reads the
    # body's position and the definition's anchor, scale and friction. Direct, from the snapshot.
    0x51CFF0: dict(name="CHingedRigidBody::SimCFF0", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0x410)],
                   nodes=slip_nodes, direct=True, fuzz=fuzz_slip),
    # E43PairWalk::MB560 (0x51b560), recorded inside SimCFF0: a is window 1 and the output c window 3 (16 bytes each).
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 16), (3, lambda a: 16)]),
    # SimEAD0 (thiscall, void): the hinge solve; writes this+0x624. Direct, from the snapshot.
    0x51EAD0: dict(name="CHingedRigidBody::SimEAD0", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x660)],
                   nodes=solve_nodes, direct=True, fuzz=fuzz_solve),
    # CHingedRigidBody::SimE1D0 (0x51e1d0), recorded inside SimEAD0 (its writes are in this's window); a direct walker,
    # so its walks and ComposeA are its inner records.
    0x51E1D0: dict(name="CHingedRigidBody::SimE1D0", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x660)],
                   nodes=e1d0_nodes, direct=True),
    # CSolid::Vf81x (0x52c510, the contact vote): (v90, v90b, 3.0f) -> hit; the two vectors are windows 1 and 2.
    0x52C510: dict(name="E47Solid::Vf81x (vote)", popped=12, x87=False, thiscall=True,
                   windows=[(1, lambda a: 16), (2, lambda a: 16)], external=True),
    # CSolid::E43Vf64 (0x52cb20, the force pass an unowned SimEAD0 tail-calls): its writes to the body are read on return.
    0x52CB20: dict(name="CSolid::E43Vf64 (force pass)", popped=0, x87=False, thiscall=True, windows=[], external=True,
                   after=lambda mu, a, eax: [(a[0], 0x660)]),
    # SimFB40 (thiscall, void): SimDB40, then the solid state and the hinged body's emits (signals), then E43Vf28. Direct.
    0x51FB40: dict(name="CHingedRigidBody::SimFB40", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x560)],
                   nodes=fb40_nodes, direct=True, fuzz=fuzz_fb40),
    # SimDB40 (thiscall, void): the car's walks over the rows and the transform, then the hinged body's three basis walks.
    # Direct: its walks and CCar::Xform8130 are its inner records. Its own record too (SimFB40 makes the call).
    0x51DB40: dict(name="CHingedRigidBody::SimDB40", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x5C0)],
                   nodes=db40_nodes, direct=True),
    # CCar::Xform8130 (the transform SimDB40 makes; its output is not used)
    # CCar::Xform8130 (SimDB40's transform, whose output it does not use; SimD160's, which is in SimD160's window)
    0x4E8130: dict(name="CCar::Xform8130", popped=8, x87=False, thiscall=True, windows=[(1, lambda a: 16), (2, lambda a: 16)],
                   external=True),
    # SimEFE0 (thiscall, void): the hinged body's frame update. Direct: its calls below and SimD860 are its inner records.
    0x51EFE0: dict(name="CHingedRigidBody::SimEFE0", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x660)],
                   nodes=efe0_nodes, direct=True, fuzz=fuzz_efe0),
    # the calls SimEFE0 makes that we don't port yet (inner records of SimEFE0)
    0x52BF30: dict(name="CHingedRigidBody::E47D36y (vtable slot 36)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x660)], external=True),
    0x51E070: dict(name="CHingedRigidBody::VfE070", popped=24, x87=False, thiscall=True,
                   windows=[(q, lambda a: 64) for q in range(1, 7)], external=True),
    0x51D160: dict(name="CHingedRigidBody::SimD160", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x660)],
                   external=True),
    0x4E6A30: dict(name="CCar::BodyCompose", popped=8, x87=False, thiscall=True, windows=[(1, lambda a: 16), (2, lambda a: 48)],
                   external=True),
    0x4F65B0: dict(name="E37GridOps::AddEdge", popped=4, x87=False, thiscall=True, windows=[(1, lambda a: 16)], external=True),
    # SimD860 (thiscall, void out[3]): the walk over the definition's matrix product with the rotation row, plus the
    # hinged body's base vector. Direct: its walk is its inner record. Reached through SimEFE0 (0x51efe0) on the drive.
    0x51D860: dict(name="CHingedRigidBody::SimD860", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x5D0), (1, lambda a: 12)], nodes=d860_nodes, direct=True, fuzz=fuzz_d860),
    # SimD160 (thiscall, void): the body's rotation block (SimC960), the walks over the definition's matrices and the body's
    # vectors, the rotation quaternion (ToQuat) and the translation (stored inline; its CopyFrom is not recorded). Direct: its
    # calls below are its inner records.
    0x51D160: dict(name="CHingedRigidBody::SimD160", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x660)],
                   nodes=d160_nodes, direct=True, fuzz=fuzz_d160),
    # E33Mat34::ComposeA (the compose under CCar::BodyCompose, called by SimE1D0; and by SimFB40)
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, windows=[(3, lambda a: 16)],
                   external=True),
    # E34Quat::ToQuat (the 3x3 at this, 36 bytes, to the quaternion out); SimD160 calls it on the body's rotation matrix
    0x4E4D10: dict(name="E34Quat::ToQuat", popped=4, x87=False, thiscall=True, windows=[(0, lambda a: 36), (1, lambda a: 16)],
                   external=True),
    # CMultiSignal1<const CSigSolidGeneralState&>::Emit (the payload is 22 words, passed by pointer)
    0x4E8330: dict(name="CMultiSignal1<SolidGeneral>::Emit", popped=4, x87=False, thiscall=True,
                   windows=[(1, lambda a: 88)], external=True),
    # CMultiSignal1<const CSigHingedBody&>::Emit (the payload is three words)
    0x51F640: dict(name="CMultiSignal1<HingedBody>::Emit", popped=4, x87=False, thiscall=True,
                   windows=[(1, lambda a: 12)], external=True),
    0x51A3E0: dict(name="CEntity::E43Vf28", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x560)],
                   external=True),
}
