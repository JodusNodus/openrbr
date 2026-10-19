"""Wheel vector set: the CWheel leaves the reference drive reaches (exe 0x5390d0-0x53afb0 and the E51Wheel helpers).

Windows: a wheel is `this` (args[0]); its tyre tail is the pointer at this+0xf10 (`TyreTail_Get10(this+0xf00)` reads
`+0x10`), so `(0, len, 0xf10)` maps the tail object. The car (`this+0x7f4`) is the Xform6870 receiver.
"""

import struct

SET_NAME = "wheel"


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def tail_rand(mu, this, rng, put, k_offsets):
    """Random inputs for a wheel's leaves: the vectors at this+0x9e0, +0x140, +0x100 and the tail's scale words."""
    for off in (0x9E0, 0x140, 0x100):
        for k in range(4):
            put(this + off + 4 * k, rng.uniform(-2, 2))
    tail = word(mu, this + 0xF10)
    for off in k_offsets:
        put(tail + off, rng.uniform(-2, 2))


def copy100_fuzz(mu, args, rng, put):
    tail_rand(mu, args[0], rng, put, (0x6C,))


def copy100_direct_args(rng, this):
    return [this + 0x1000, this + 0x1020]  # x and out: scratch words past the leaf's own windows


def blend4_fuzz(mu, args, rng, put):
    this = args[0]
    tail_rand(mu, this, rng, put, (0xCC,))
    for base in args[1:3]:  # a and b are the input vectors (c and d are written)
        for k in range(4):
            put(base + 4 * k, rng.uniform(-2, 2))


def blend4_direct_args(rng, this):
    return [this + 0x1040, this + 0x1060, this + 0x1080, this + 0x10A0]


def unk9290_fuzz(mu, args, rng, put):
    """Random frame records (flag, vector, lanes) and the wheel's axis and contact vectors (the flag is 0 or 1)."""
    this, a, b = args[0], args[1], args[2]
    for base in (a, b):
        put(base, rng.choice((0, 1, 1, 1)), "<I")
        for k in range(4):
            put(base + 0x20 + 4 * k, rng.uniform(-1, 1))
        put(base + 0x50, rng.uniform(-0.5, 0.5))
        put(base + 0x64, rng.uniform(-0.5, 0.5))
    for off in (0xA70, 0xA80, 0xA90):
        for k in range(4):
            put(this + off + 4 * k, rng.uniform(-2, 2))


def unk9290_direct_args(rng, this):
    return [this + 0x1100, this + 0x1200, this + 0x1300]  # a, b, out: scratch records past the leaf's windows


def xform_fuzz(mu, args, rng, put):
    """The CDef's source vectors (the eight pairs' sources, +0x250..+0x310) and the car's transforms' inputs. Written
    back when the call returns."""
    d = word(mu, args[0] + 0x34)
    for base in range(0x250, 0x310, 0x10):
        for k in range(4):
            put(d + base + 4 * k, rng.uniform(-2, 2))


def f32_word(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def block_fuzz(mu, this, rng, put):
    """The tyre block (this+0x7f0) floats: the input (+0x10, sometimes 0 or out of [-1, 1]) and the accumulators."""
    block = word(mu, this + 0x7F0)
    put(block + 0x10, rng.choice((0.0, float("nan"), rng.uniform(-1.5, 1.5), rng.uniform(-0.3, 0.3))))
    put(block + 0x0, rng.uniform(-0.5, 0.5))
    put(block + 0x4, rng.uniform(-0.5, 0.5))


def axis_fuzz(mu, args, rng, put):
    """The angle (this+0x5ac), the axis base and turn vectors (+0x600, +0x610), the CDef's angle and lerp scales."""
    this = args[0]
    put(this + 0x5AC, rng.uniform(-3.2, 3.2))
    for base in (0x600, 0x610):
        for k in range(4):
            put(this + base + 4 * k, rng.uniform(-1, 1))
    block_fuzz(mu, this, rng, put)
    d = word(mu, this + 0x34)
    put(d + 0x900, rng.uniform(-3, 3))
    put(d + 0x950, rng.uniform(-2, 2))
    for off in (0x8A0, 0x8E0, 0x8F0):
        for k in range(4):
            put(d + off + 4 * k, rng.uniform(-2, 2))
    put(d + 0x910, rng.uniform(-2, 2))
    put(d + 0x920, rng.uniform(-2, 2))


def no_args(rng, this):
    return []


def unk97a0_fuzz(mu, args, rng, put):
    """The two sub-objects' gates (flags +0xd04 and +0xc94 are 0 or 1; scales +0xd5c and +0xcec are zero or not), the
    cross operands +0xa70 and +0xa80, the scale +0xa50 and +0xa60..+0xa68."""
    this = args[0]
    for flag, scale in ((0xD04, 0xD5C), (0xC94, 0xCEC)):
        put(this + flag, rng.choice((0, 1)), "<I")
        put(this + scale, rng.choice((0.0, rng.uniform(0.5, 2))))
    for off in (0xA70, 0xA74, 0xA78, 0xA80, 0xA84, 0xA88, 0xA8C):
        put(this + off, rng.uniform(-1, 1))
    put(this + 0xA50, rng.uniform(-2, 2))
    for off in (0xA60, 0xA64, 0xA68):
        put(this + off, rng.uniform(-2, 2))


def copycf30_fuzz(mu, args, rng, put):
    """The source record `a` (+0x30..+0x6c, the words CopyCF30 copies) and the CDef's +0x40c."""
    this = args[0]
    sa = args[1]
    for off in range(0x30, 0x70, 4):
        put(sa + off, rng.uniform(-2, 2))
    d = word(mu, this + 0x34)
    put(d + 0x40C, rng.uniform(-2, 2))
    # CopyDefFrom(c) reads the table pointer at c+0x2c and copies dwords of the table. The table is a zeroed block in the
    # wheel (its words are copied, not followed, but random words there fault the damper call on the copied values).
    table = this + 0x340
    put(args[3] + 0x2C, table, "<I")
    for k in range(0, 0x100, 4):
        put(table + k, 0.0)


def unk9900_direct_args(rng, this):
    """Two material tags (small indices, the snapshot's entries 0..3) and the float z."""
    return [rng.randrange(4), rng.randrange(4), f32_word(rng.uniform(-1, 1))]


def unk9900_nodes(mu, a):
    """The global, the CDef pointer and the material manager with the tags' entries (as block_nodes maps them)."""
    return block_nodes(mu, a)


def vf20_fuzz(mu, args, rng, put):
    """The two E09 vector blocks Vf20 clears (+0x190, +0x1c0): random words, so the clear shows in the windows."""
    this = args[0]
    for off in (0x190, 0x1C0):
        for k in range(4):
            put(this + off + 4 * k, rng.uniform(-2, 2))


def setter_fuzz(mu, args, rng, put):
    """The setter's bounds (+0x960 below +0x970) and the input, with the lerp's inputs (the axis fuzz)."""
    this = args[0]
    d = word(mu, this + 0x34)
    lo = rng.uniform(-1, 0)
    put(d + 0x960, lo, "<f")
    put(d + 0x970, lo + rng.uniform(0.1, 2), "<f")
    block_fuzz(mu, this, rng, put)
    put(this + 0x5AC, rng.uniform(-3.2, 3.2))
    put(d + 0x900, rng.uniform(-3, 3))
    put(d + 0x950, rng.uniform(-2, 2))


def setter_direct_args(rng, this):
    return [f32_word(rng.uniform(-2, 2))]


def walk_fuzz(mu, args, rng, put):
    """The walk's gain inputs (+0x9c0, +0x9c4: sometimes tiny), its offset (+0x9a0), the car's +0xb1c (GetB1C) and
    the frame vectors (+0x9e0, +0xa50 the spin)."""
    this = args[0]
    put(this + 0x9C0, rng.uniform(-2, 2))
    put(this + 0x9C4, rng.choice((rng.uniform(-2, 2), rng.uniform(-0.0005, 0.0005))))
    for base in (0x9A0, 0x9E0, 0xC10):
        for k in range(4):
            put(this + base + 4 * k, rng.uniform(-2, 2))
    put(this + 0xA50, rng.uniform(-30, 30))
    car = word(mu, this + 0x7F4)
    put(car + 0xB1C, rng.uniform(-2, 2))


def m3afb0_direct_args(rng, this):
    return [this + 0x1400]  # the output vector p: scratch past the wheel's windows


WORLD = 0x1659B70  # g_1659b70: the CPhysics global (RBR_E51_WheelB.cpp:151, :203)


def physics_words(mu):
    """The CDef and the material manager (cdef+0x18) the CPhysics global leads to, as the snapshot holds them now."""
    cdef = word(mu, word(mu, WORLD) + 0x24)
    return cdef, cdef + 0x18


def block_nodes(mu, a):
    """The windows M3D230 reads outside its argument: the global, the CPhysics' CDef pointer, the material manager (the
    first four material entries, 0x160 bytes each from +0xc, and their flag bytes)."""
    _, manager = physics_words(mu)
    return [(WORLD, 4), (word(mu, WORLD) + 0x24, 4), (manager, 0x58C)]


def pose_nodes(mu, a):
    """M3D460 also reads the CDef's flags word (+0x160cc) and the pair tag (+0x1614c)."""
    cdef, _ = physics_words(mu)
    return block_nodes(mu, a) + [(cdef + 0x160CC, 4), (cdef + 0x1614C, 4)]


def fuzz_pose(mu, heap, r, flags):
    """The tyre block (heap) and the snapshot's CDef: the block's phase inputs, weight, tags (0..3) and target, the
    material entries 0..3 (divisor, gain, the two lerped values, flag byte) and, when `flags`, the CDef's pair flags."""
    block = heap + 0x400
    cdef, manager = physics_words(mu)
    writes = []

    def f32_at(at, value):
        writes.append((at, struct.pack("<f", value)))

    def u32_at(at, value):
        writes.append((at, struct.pack("<I", value & 0xFFFFFFFF)))

    f32_at(block + 0x10, r.choice((0.0, float("nan"), r.uniform(-3, 3), r.uniform(-0.3, 0.3))))
    f32_at(block + 0x14, r.choice((0.0, r.uniform(-3, 3), r.uniform(-0.3, 0.3))))
    f32_at(block + 0x34, r.choice((0.0, 1.0, r.uniform(0, 1), r.uniform(-0.2, 1.2))))
    u32_at(block + 0x38, r.randrange(4))
    u32_at(block + 0x3C, r.randrange(4))
    f32_at(block + 0x40, r.uniform(-1, 1))
    for tag in range(4):
        entry = manager + 0xC + tag * 0x160
        f32_at(entry + 0xD8, r.choice((0.0, float("nan"), r.uniform(-7, 7))))
        f32_at(entry + 0xC8, r.uniform(-2, 2))
        for off in (0x18, 0xB8):
            f32_at(entry + off, r.uniform(-2, 2))
        writes.append((manager + tag * 0x160 + 0x164, bytes([r.choice((0, 1, 3))])))
    if flags:
        u32_at(cdef + 0x160CC, r.choice((0x40000, 0x40000 | 0x1, 0, 0x2)))
        u32_at(cdef + 0x1614C, r.randrange(4))
    return block, writes


def fuzz_block230(mu, heap, r):
    block, writes = fuzz_pose(mu, heap, r, flags=False)
    return [block], writes, heap + 0x800


def fuzz_block460(mu, heap, r):
    block, writes = fuzz_pose(mu, heap, r, flags=True)
    b = r.choice((r.uniform(-0.5, 0.5), 0.0, float("nan")))
    c = r.choice((r.uniform(-0.5, 0.5), 0.0, -1e9))
    return [block, f32_word(b), f32_word(c)], writes, heap + 0x800


def fuzz_object(size):
    """A random object of `size` bytes in the stub heap (the direct call's `this`): the functions that only write their
    own object are checked against its old bytes."""
    def recipe(mu, heap, r):
        this = heap + 0x800
        return [], [(this, bytes(r.randrange(256) for _ in range(size)))], this
    return recipe


def travel_nodes(mu, a):
    """The windows M3B880 reads outside the wheel: the world global, its car pointer (+0x24) and the car's flag word
    (+0x160e0; bit 0x400 gates the travel)."""
    car = word(mu, word(mu, WORLD) + 0x24)
    return [(WORLD, 4), (word(mu, WORLD) + 0x24, 4), (car + 0x160E0, 4)]


def travel_fuzz(mu, heap, r):
    """M3B880's inputs: the wheel (random bytes), its tyre block (+0x34, the float at +0x41c), the strut's damper (+0xec4; its
    m_unk4 at +0xec8 is a random block for ComputeTravel and TravelRange) and the car's flag word, with 0x400 random and bit
    2 clear (the overlay, Unk9bb0, is not recorded)."""
    this = heap + 0x800
    tyre = heap + 0x2000
    block = heap + 0x2400
    flags_at = word(mu, word(mu, 0x1659B70) + 0x24) + 0x160E0
    flags = (word(mu, flags_at) & ~0x404) | (0x400 if r.random() < 0.5 else 0)
    writes = [(this, bytes(r.randrange(256) for _ in range(0xF18))),
              (tyre, bytes(r.randrange(256) for _ in range(0x420))),
              (block, bytes(r.randrange(256) for _ in range(0x100))),
              (this + 0x34, struct.pack("<I", tyre)),
              (this + 0xEC8, struct.pack("<I", block)),
              (flags_at, struct.pack("<I", flags))]
    return [], writes, this


def tail_args(rng, this):
    """UnkA860's stack words: raw bits (the tyre block, the strut's bits, the scratch's value)."""
    return [rng.randrange(1 << 32) for _ in range(3)]


def tweak_base_fuzz(mu, heap, r):
    """NTweak::CBase(name): the name is a string in the stub heap (the header keeps the pointer)."""
    _, writes, this = fuzz_object(0x10)(mu, heap, r)
    name = heap + 0x800
    return [name], writes + [(name, b"Tweak\x00")], this


def cdef_fuzz(mu, heap, r):
    """CWheel::CDef::CDef (direct runs): the definition is a random 0x980-byte block at heap+0x800; the outer is a string in
    the stub heap (the solid base keeps its pointer)."""
    block, outer = heap + 0x800, heap + 0x2000
    return [outer], [(block, bytes(r.randrange(256) for _ in range(0x980))), (outer, b"DefWheel\x00")], block


def new_cdef_fuzz(mu, heap, r):
    """CWheel::M3B610 (direct runs): the outer is a string in the stub heap (the definition keeps its pointer); the ecx is
    ignored, so it is zero."""
    outer = heap + 0x2000
    return [outer], [(outer, b"DefWheel\x00")], 0


def wheel_ctor_fuzz(mu, heap, r):
    """E51Wheel::Ctor (direct runs): the wheel is a random 0x1318-byte block at heap+0x800; the definition is a random
    0x980-byte block at heap+0x5000 whose name word (+4) is the string "WheelName" at heap+0x6000, which is also the name
    argument. The owner is zero: with an owner the body event runs the wheel's own slot 18 (through its vtable), which the
    records do not list. The flags are random."""
    wheel, definition, name = heap + 0x800, heap + 0x5000, heap + 0x6000
    def_bytes = bytearray(r.randrange(256) for _ in range(0x980))
    def_bytes[4:8] = struct.pack("<I", name)
    wheel_bytes = bytes(r.randrange(256) for _ in range(0x1318))
    return ([definition, name, 0, r.randrange(0x100)],
            [(wheel, wheel_bytes), (definition, bytes(def_bytes)), (name, b"WheelName\x00")], wheel)


def cstate_fuzz(mu, heap, r):
    """CWheel::CState's constructor (direct runs): the owner is a body-shaped block at heap+0x3000 whose flags byte (+0x60)
    is random (bit 2 decides whether the E47 state keeps it), the name a string in the stub heap, the state a random block at
    heap+0x800. The third argument is zero."""
    owner, name, state = heap + 0x3000, heap + 0x3400, heap + 0x800
    owner_bytes = bytearray(0x64)
    owner_bytes[0x60] = r.randrange(256)
    state_bytes = bytes(r.randrange(256) for _ in range(0x400))
    return [owner, name, 0], [(owner, bytes(owner_bytes)), (name, b"State\x00"), (state, state_bytes)], state


def strut_nodes(mu, a):
    """The strut's name object (`m_name` at +4 of the strut, `this+0xec4`): its +0x28 and +0x38 floats."""
    return [(word(mu, a[0] + 0xEC4 + 4), 0x40)]


CAR_PTR = 0x8EF660  # the car object global (GetB0C's car)


def m3b900_direct_args(rng, this):
    """The strut travel x (the stack argument)."""
    return [struct.unpack("<I", struct.pack("<f", rng.uniform(-0.2, 0.2)))[0]]


def m3b900_fuzz(mu, args, rng, put):
    """The tyre tail's scale word (+0x6c, read through TyreTail_Get10) and the wheel's vectors the pose overwrites."""
    tail_rand(mu, args[0], rng, put, (0x6C,))


def m3b900_nodes(mu, a):
    """The windows M3B900 reads outside its argument: the world global and its object, the CDef's debug-flag word
    (+0x160e0, as `M3D460`'s nodes), the car global and the car's GetB0C word (+0xb0c), and the tyre tail."""
    cdef, _ = physics_words(mu)
    car = word(mu, CAR_PTR)
    tail = word(mu, a[0] + 0xF10)  # TyreTail_Get10's pointer; the refresh reads its scale (+0x6c)
    return [(WORLD, 4), (word(mu, WORLD) + 0x24, 4), (cdef + 0x160E0, 4), (CAR_PTR, 4), (car + 0xB0C, 4),
            (tail, 0x70)]


E38_GLOBAL = 0x7C755C  # g_7c755c: the E38Ops object the ray casts and M_D760X run on
SWEEP_GLOBAL = 0x7C7464  # g_7c7464: the ECX of E38_004FAD90
PROBE_INT = 0x739594  # the int M3D520 copies into +0xce0


def m3d520_direct_args(rng, this):
    """The probe's two stack pointers: the output and the second vector, both in the wheel object past its fields."""
    return [this + 0x1000, this + 0x1020]


def m3d520_fuzz(mu, args, rng, put):
    """The strut, the contact vectors and the points the probe reads, the three blocks' targets and diffs, and the tyre
    tail's scale word (TyreTail_Get14 reads the tail at +0x14 of the container, +0xf14 of the wheel)."""
    this = args[0]
    for off in (0xA20, 0xA30, 0xA40, 0xA80):
        for k in range(4):
            put(this + off + 4 * k, rng.uniform(-2, 2))
    for block in (0xC90, 0xC20, 0xD00):
        put(this + block + 0x40, rng.uniform(0, 3))
        put(this + block + 0x64, rng.uniform(-0.5, 2))
    put(word(mu, this + 0xF14) + 0x4C, rng.uniform(0, 1))


def m3d520_nodes(mu, a):
    """The globals the probe reads (the E38 object, the world, the sweep's ECX and the int it copies) and the tyre tail
    (+0x4c is its scale)."""
    tail = word(mu, a[0] + 0xF14)
    return [(E38_GLOBAL, 4), (WORLD, 4), (SWEEP_GLOBAL, 4), (PROBE_INT, 4), (tail, 0x50)]


def m3d030_fuzz(mu, args, rng, put):
    """The axes, the car transform's inputs and the strut's name floats (+0x28, +0x38)."""
    axis_fuzz(mu, args, rng, put)
    xform_fuzz(mu, args, rng, put)
    name = word(mu, args[0] + 0xEC4 + 4)
    for off in (0x28, 0x38):
        put(name + off, rng.uniform(-2, 2))


def unk9660_fuzz(mu, args, rng, put):
    """The phase at this+0x5ac (a finite angle)."""
    put(args[0] + 0x5AC, rng.uniform(-10, 100))


def unk9660_direct_args(rng, this):
    """The output words (past the leaf's windows) and the cycle length: 4 as Unk96e0 passes it, or others."""
    return [this + 0x1100, rng.choice((4, rng.randrange(1, 17), rng.randrange(1, 2**32)))]


def unk96e0_fuzz(mu, args, rng, put):
    """The phase at this+0x5ac and the tyre block's two 4-entry tables (+0x14 and +0x24) that Unk96e0 lerps."""
    this = args[0]
    put(this + 0x5AC, rng.uniform(-10, 100))
    block = word(mu, this + 0x7F0)
    for off in (0x14, 0x24):
        for k in range(4):
            put(block + off + 4 * k, rng.uniform(-1, 1))


RET4 = 0x4025F0  # a plain `ret 4` function: the Invoke of the fuzzed connections


def noise_signal_blocks(heap, base, r):
    """A CMultiSignal1 at `base` (guard clear, count pointer, 32 slots, overflow list; 0x218 bytes) with random
    connections on the overflow list and on the slot the count selects. A connection is 0x18 bytes: vtable (slot 1 is
    Invoke, RET4) at +0, the next link at +4. The count stays in range, the guard clear: the other two paths log."""
    vtable, count_at = heap + 0x1000, heap + 0x1100
    count = r.randrange(0x20)
    signal = bytearray(0x218)
    conns, nxt = [], heap + 0x1200
    for head in (0x208, 8 + 16 * count):
        links = [nxt + 0x40 * k for k in range(r.choice((0, 1, 2)))]
        nxt += 0x100
        for k, c in enumerate(links):
            conns.append((c, struct.pack("<II", vtable, links[k + 1] if k + 1 < len(links) else 0) + bytes(16)))
        if links:
            signal[head:head + 4] = struct.pack("<I", links[0])
    signal[4:8] = struct.pack("<I", count_at)
    return [(base, bytes(signal)), (vtable, struct.pack("<II", 0, RET4)), (count_at, struct.pack("<I", count))] + conns


def signal_conns(mu, signal):
    """The connections the walk visits: the overflow list, then the selected slot's."""
    count_at = word(mu, signal + 4)
    out = []
    for head in (signal + 0x208, signal + 8 + 16 * word(mu, count_at)):
        c = word(mu, head)
        while c:
            out.append(c)
            c = word(mu, c + 4)
    return out


def signal_nodes(mu, a):
    return [w for c in signal_conns(mu, a[0]) for w in ((c, 0x18), (word(mu, c), 8))]


def signal_targets(mu, a):
    return [(word(mu, word(mu, c) + 4), 4) for c in signal_conns(mu, a[0])]


def tyre_noise_emit_fuzz(mu, heap, r):
    """CMultiSignal1<CSigTyreNoise>::Emit (direct runs): the signal in the stub heap, the payload a random block."""
    signal, sig = heap + 0x800, heap + 0x4000
    writes = noise_signal_blocks(heap, signal, r) + [(sig, bytes(r.randrange(256) for _ in range(0x60)))]
    return [sig], writes, signal


def emit_noise_fuzz(mu, heap, r):
    """CScratch::EmitNoise (direct runs): the scratch is a random 0x440-byte block; g_signalManager points at a block in
    the stub heap whose tyre-noise signal (+0x6268) is built as above."""
    scratch, manager = heap + 0x4000, heap + 0x8000
    writes = noise_signal_blocks(heap, manager + 0x6268, r)
    writes += [(scratch, bytes(r.randrange(256) for _ in range(0x440))), (NOISE_MANAGER, struct.pack("<I", manager))]
    return [], writes, scratch


NOISE_MANAGER = 0x893600  # g_signalManager


def emit_noise_nodes(mu, a):
    """The signal manager word, the signal (0x218 bytes at +0x6268), its count word and the connections the walk reads."""
    signal = word(mu, NOISE_MANAGER) + 0x6268
    return [(NOISE_MANAGER, 4), (signal, 0x218), (word(mu, signal + 4), 4)] + signal_nodes(mu, [signal])


SPECS = {
    # Copy100 (thiscall: this, x, out): tmp = this+0x9e0 * k + this+0x140 (k = tail+0x6c), Xform6870(&tmp, x), then
    # the 16 bytes at this+0x100 go to out.
    0x5390D0: dict(name="CWheel::Copy100", popped=8, stack_words=2, x87=False, thiscall=True, branch=no_branch,
                   fuzz=copy100_fuzz, direct_args=copy100_direct_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0xD0, 0xF10), (1, lambda a: 16), (2, lambda a: 16)]),
    # Blend4 (thiscall: this, a, b, c, d): c = a*k + b, d = a*(0-k) + b with k = tail+0xcc * 0.5; the four vectors are
    # copied to this+0xa20 (d), this+0xa40 (c), this+0xde0 (c), this+0xe00 (d).
    0x539180: dict(name="CWheel::Blend4", popped=0x10, stack_words=4, x87=False, thiscall=True, branch=no_branch,
                   fuzz=blend4_fuzz, direct_args=blend4_direct_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0xD0, 0xF10), (1, lambda a: 16), (2, lambda a: 16),
                            (3, lambda a: 16), (4, lambda a: 16)]),
    # Car body transform (external here: its records are the inner calls of Copy100). Same windows as car_body.py.
    0x4E6870: dict(name="CCar::Xform6870", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x4A4), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # Unk9660 (thiscall: this, p, n): the pick of this+0x5ac in a cycle of n steps, written to p (three words).
    0x539660: dict(name="CWheel::Unk9660", popped=8, stack_words=2, x87=False, thiscall=True, branch=no_branch,
                   fuzz=unk9660_fuzz, direct_args=unk9660_direct_args,
                   windows=[(0, lambda a: 0x5B0), (1, lambda a: 12)]),
    # Unk96e0 (thiscall, plain ret): Init on a local (the external call below), Unk9660 with n = 4 into it, then two
    # lerps of the tyre block's tables (through this+0x7f0) into this+0xd70 and this+0xd74. The window covers the phase.
    0x5396E0: dict(name="CWheel::Unk96e0", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=unk96e0_fuzz, direct_args=no_args,
                   windows=[(0, lambda a: 0xD78), (0, lambda a: 0x40, 0x7F0)]),
    # CE36SubBlock::Init (0x4f3e80, thiscall, plain ret): zeroes the local's three words (an inner call of Unk96e0).
    0x4F3E80: dict(name="CE36SubBlock::Init (4f3e80)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 12)]),
    # Unk9290 (thiscall: this, a, b, out): frame records a and b, the axis this+0xa90, out.
    0x539290: dict(name="CWheel::Unk9290", popped=0xC, stack_words=3, x87=False, thiscall=True, branch=no_branch,
                   fuzz=unk9290_fuzz, direct_args=unk9290_direct_args,
                   windows=[(0, lambda a: 0xAA0), (1, lambda a: 0x68), (2, lambda a: 0x68), (3, lambda a: 16)]),
    # M3A8A0 (thiscall, no stack arguments, plain ret): eight car transforms into the CDef's axes. The car (this+0x44)
    # is read with its flag byte (+0x60); the CDef is the window at this+0x34. Its calls are the externals below.
    0x53A8A0: dict(name="CWheel::M3A8A0 (xform caller)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=xform_fuzz,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x980, 0x34), (0, lambda a: 0x64, 0x44)]),
    # CCar::BodyToWorldPair(a, b) (0x4e69c0, ret 8): a is read, b written (the 16-byte output).
    0x4E69C0: dict(name="CCar::BodyToWorldPair (4e69c0)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CCar::BodyCompose(a, b) (0x4e6a30, ret 8): as BodyToWorldPair.
    0x4E6A30: dict(name="CCar::BodyCompose (4e6a30)", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # M3A9E0 (thiscall, plain ret): the axis vector at +0x5f0 from the tyre block's input (block+0x10) and the angle.
    0x53A9E0: dict(name="CWheel::M3A9E0 (axis from angle)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=axis_fuzz, direct_args=no_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x14, 0x7F0)]),
    # M3AB00 (thiscall): the axis pair +0x600 and +0x610 from the angle w*def+0x950 + def+0x900.
    0x53AB00: dict(name="CWheel::M3AB00 (axis pair)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=axis_fuzz, direct_args=no_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x14, 0x7F0), (0, lambda a: 0x980, 0x34)]),
    # M3AB90 (thiscall): the lerp into +0x570 from the CDef's +0x8e0, +0x8a0, +0x8f0 and the tyre block.
    0x53AB90: dict(name="CWheel::M3AB90 (lerp 570)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=axis_fuzz, direct_args=no_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x14, 0x7F0), (0, lambda a: 0x980, 0x34)]),
    # M3AC50 (thiscall, one stack float v, ret 4): the input setter (calls M3AB00).
    0x53AC50: dict(name="CWheel::M3AC50 (input setter)", popped=4, stack_words=1, x87=False, thiscall=True,
                   branch=no_branch, fuzz=setter_fuzz, direct_args=setter_direct_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x14, 0x7F0), (0, lambda a: 0x980, 0x34)]),
    # M3AEF0 (thiscall, plain ret): the walk into +0xc10 (MB560 on the frame), plus the gain from GetB1C of the car.
    0x53AEF0: dict(name="E51Wheel::M3AEF0 (walk)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=walk_fuzz, direct_args=no_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0xB24, 0x7F4)]),
    # M3AFB0 (thiscall, one stack pointer p, ret 4): M3AEF0, then the cross product of two vectors into p.
    0x53AFB0: dict(name="E51Wheel::M3AFB0 (spin cross)", popped=4, stack_words=1, x87=False, thiscall=True,
                   branch=no_branch, fuzz=walk_fuzz, direct_args=m3afb0_direct_args,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0xB24, 0x7F4), (1, lambda a: 12)]),
    # E43PairWalk::MB560 (0x51b560, ret 0xc): args (walker, a, b, c); a and c are windowed (c is the walk's output).
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)]),
    # CRT sin and cos (__CIsin 0x53f3b0, __CIcos 0x53f3a0): x87 results of the axis angle.
    0x53F3B0: dict(name="CRT sin (__CIsin)", popped=4, x87=True, cdecl=True, windows=[], external=True),
    0x53F3A0: dict(name="CRT cos (__CIcos)", popped=4, x87=True, cdecl=True, windows=[], external=True),
    # M3D230 (thiscall, one stack pointer p = the tyre block, ret 4): the blend of the two material tags into +0x54
    # and +0x58. Its sines (0x53f3b0) and its tag lookup (0x520940) are the inner records.
    0x53D230: dict(direct_fuzz=fuzz_block230, name="E51Wheel::M3D230 (tag blend)", popped=4, stack_words=1, x87=False,
                   thiscall=True, branch=no_branch, external=True, nodes=block_nodes, windows=[(1, lambda a: 0x68)]),
    # E41Plain::InitRealLerp (0x50f010, cdecl, one buffer argument): the lerp at the buffer's +0xc (inner records only).
    0x50F010: dict(name="E41Plain::InitRealLerp", popped=0, stack_words=1, x87=False, cdecl=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # CMaterialManager::Tag520940 (thiscall, one stack word, ret 4): the material entry of a tag (inner records only).
    0x520940: dict(name="CMaterialManager::Tag520940", popped=4, stack_words=1, x87=False, thiscall=True,
                   branch=no_branch, external=True, windows=[]),
    # M3D460 (thiscall, the block and floats b, c, ret 0xc): M3D230, then the flag at +0x4 from d = target - total.
    0x53D460: dict(direct_fuzz=fuzz_block460, name="E51Wheel::M3D460 (pose flag)", popped=0xC, stack_words=3,
                   x87=False, thiscall=True, branch=no_branch, nodes=pose_nodes, windows=[(1, lambda a: 0x68)]),
    # M3D030 (thiscall, no stack words, plain ret): the per-frame update of one wheel. The car transform (M3A8A0), the
    # axes, two GetName calls on the strut, SetDirty on this+0x5b0, M3B900 and the strut's UpdateAndReset (inner records).
    0x53D030: dict(name="E51Wheel::M3D030 (strut update)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=m3d030_fuzz, direct_args=no_args, nodes=strut_nodes,
                   windows=[(0, lambda a: 0xEC4), (0, lambda a: 0x14, 0x7F0), (0, lambda a: 0x980, 0x34),
                            (0, lambda a: 0x64, 0x44)]),
    # NTweak::CBase::GetName (0x52db10, thiscall): returns m_name (inner records only).
    0x52DB10: dict(name="NTweak::CBase::GetName", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # NTweak::CBase::SetDirty (0x52f450, thiscall): the object's 16 bytes (inner records only).
    0x52F450: dict(name="NTweak::CBase::SetDirty", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # CSpringDamper::UpdateAndReset (0x52e060, thiscall): the strut to the end of the wheel. It is a tail call of M3D030,
    # so its record is its own, and M3D030's windows end below the strut (they include its writes).
    0x52E060: dict(name="CSpringDamper::UpdateAndReset", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x55C)]),
    # E51Wheel::M3B900 (0x53b900, thiscall, one float): the wheel (inner records only).
    0x53B900: dict(name="E51Wheel::M3B900", popped=4, stack_words=1, x87=False, thiscall=True, external=True,
                   fuzz=m3b900_fuzz, direct_args=m3b900_direct_args, nodes=m3b900_nodes,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x14, 0x7F0), (0, lambda a: 0x980, 0x34)]),
    # E51Wheel::M3D520 (0x53d520, thiscall, two stack pointers out and tmp, ret 8): the ground probe. Callees are inner
    # records only where a spec exists (M3D460 does); E39, E46, the sweep and the rest have none yet, so no vector test.
    0x53D520: dict(name="E51Wheel::M3D520 (ground probe)", popped=8, stack_words=2, x87=False, thiscall=True, external=True,
                   fuzz=m3d520_fuzz, direct_args=m3d520_direct_args, nodes=m3d520_nodes,
                   windows=[(0, lambda a: 0xF18), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CWheel::E43Vf20 (0x539090, slot 8, thiscall, one stack argument p): clears the vectors at +0x1c0 and +0x190,
    # calls the virtual Vf40 on the wheel, then forwards p to CSolid::E43Vf20 (0x529200 thunk).
    0x539090: dict(name="CWheel::E43Vf20", popped=4, x87=False, thiscall=True, branch=no_branch,
                   fuzz=vf20_fuzz, direct_args=lambda rng, this: [rng.getrandbits(32)],
                   windows=[(0, lambda a: 0xF18)]),
    # E51Wheel::M3B5B0 (0x53b5b0, the wheel's Vf40, vtable slot 0x40): an E47 call, then a tail jump. Inner records only.
    0x53B5B0: dict(name="E51Wheel::M3B5B0 (Vf40)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xF18)]),
    # E47_Vf16Helper (0x52b6d0), the base call of M3B5B0: inner records only.
    0x52B6D0: dict(name="E47_Vf16Helper", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xF18)]),
    # CSolid::E43Vf20 (0x529200, the jmp thunk to CEntity::E43Vf20 at 0x51a3a0), called with p: inner records only.
    0x529200: dict(name="CSolid::E43Vf20 (thunk)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xF18)]),
    # CDef::Unk4C, CDef::Unk20, E51Setup::Vf20 and Vf28 (vtable slots; thiscall, no arguments): each returns the address
    # of a static object and writes nothing.
    0x538B40: dict(name="CDef::Unk4C", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x538D70: dict(name="CDef::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x53B550: dict(name="E51Setup::Vf20", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x53B570: dict(name="E51Setup::Vf28", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    # E51WheelDamage::Zero (0x53b050, thiscall): zeroes the damage record's first 0x34 bytes.
    0x53B050: dict(direct_fuzz=fuzz_object(0x40), name="E51WheelDamage::Zero", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x34)]),
    # CScratch::Vf04 (0x538f50, thiscall): clears its vectors and zeroes +0x57c and +0x210. Clear (0x465d80) is a leaf.
    0x538F50: dict(direct_fuzz=fuzz_object(0x600), name="CScratch::Vf04", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x580)]),
    # CWheel::CScratch::CScratch (0x53b670, thiscall, returns this): the base init, the vtable word, the material at +0x2a4,
    # the sub-block at +0x404, then Vf04. The writes end before +0x580, Vf04's last store.
    0x53B670: dict(direct_fuzz=fuzz_object(0x600), name="CScratch::CScratch", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x580)]),
    # E51Wheel::M3B880 (0x53b880, thiscall, no arguments): the overlay (Unk9bb0, a host call), the strut's travel and its clamp.
    0x53B880: dict(direct_fuzz=travel_fuzz, name="E51Wheel::M3B880 (refresh travel)", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0xF18)], nodes=travel_nodes),
    # CMultiSignal1<CSigTyreNoise>::Emit (0x534f80, thiscall, ret 4: sig): the signal walk of 0x526580 on the tyre-noise
    # signal. Its window is the signal (0x218 bytes) and the count word; the connections are nodes, Invoke the dispatch.
    0x534F80: dict(direct_fuzz=tyre_noise_emit_fuzz, name="CMultiSignal1<CSigTyreNoise>::Emit", popped=4, stack_words=1,
                   x87=False, thiscall=True, branch=no_branch, windows=[(0, lambda a: 0x218), (0, lambda a: 4, 4)],
                   nodes=signal_nodes, dispatch=signal_targets),
    # CMultiSignal1<CSigTireParticles>::Emit (0x53d0d0): the same walk on the tire-particle signal.
    0x53D0D0: dict(direct_fuzz=tyre_noise_emit_fuzz, name="CMultiSignal1<CSigTireParticles>::Emit", popped=4,
                   stack_words=1, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x218), (0, lambda a: 4, 4)], nodes=signal_nodes, dispatch=signal_targets),
    # CScratch::EmitNoise (0x535890, thiscall, plain ret): emits the signal at g_signalManager+0x6268 with the scratch's
    # block (+0x420), then clears its word +0x424. The signal manager's signal is the call's window (via the Emit).
    0x535890: dict(direct_fuzz=emit_noise_fuzz, name="CScratch::EmitNoise", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x440)], nodes=emit_noise_nodes),
    # CWheel::M3B610 (0x53b610, thiscall, plain ret: the caller pops the one stack word, the outer): operator new (0x70bec2)
    # gives the block, the definition is built on it (its window is the nested CDef call's, checked by the test).
    0x53B610: dict(direct_fuzz=new_cdef_fuzz, name="CWheel::M3B610 (new CDef)", popped=0, stack_words=1, x87=False,
                   thiscall=True, external=True, branch=no_branch, windows=[]),
    # operator new (0x70bec2, cdecl): the caller pops; the size is not recorded (the block is the record's eax).
    0x70BEC2: dict(name="operator new", popped=0, x87=False, external=True, windows=[]),
    # E51Wheel::Ctor (0x53b6e0, thiscall, ret 0x10; four stack words: def, name, owner, flags). The wheel is its window
    # (0x1318 bytes: the tyre scratch at +0xf00 is the last), the definition is read (its name word, the tyre's and the
    # spring damper's pointers).
    0x53B6E0: dict(direct_fuzz=wheel_ctor_fuzz, name="E51Wheel::Ctor", popped=0x10, stack_words=4, x87=False,
                   thiscall=True, branch=no_branch, windows=[(0, lambda a: 0x1318), (1, lambda a: 0x980 if a[1] else 0)]),
    # CSolid::CSolid (0x52d480, thiscall, four stack words: def, name, owner, flags): the solid base, 0x3d0 bytes.
    0x52D480: dict(name="CSolid::CSolid", popped=0x10, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 0x3D0)]),
    # CWheel::CDef::CDef (0x5387e0, thiscall, ret 4; one stack word, the outer): the whole definition is its window.
    0x5387E0: dict(direct_fuzz=cdef_fuzz, name="CWheel::CDef::CDef", popped=4, stack_words=1, x87=False,
                   thiscall=True, branch=no_branch, windows=[(0, lambda a: 0x980)]),
    # CWheel::CState::CState (0x538dd0, thiscall, three stack words: owner, name, unused; ret 0xc): the E47 state, its
    # members and the spring damper and tyre states at +0x2a0 and +0x2e0. The state is 0x400 bytes of the wheel; the owner's
    # flags (+0x60) are read, so its window is the owner's block.
    0x538DD0: dict(direct_fuzz=cstate_fuzz, name="CWheel::CState::CState", popped=0xC, stack_words=3, x87=False,
                   thiscall=True, branch=no_branch, windows=[(0, lambda a: 0x400), (1, lambda a: 0x64)]),
    # CWheel::UnkA860 (0x53a860, thiscall, three stack words a, b, c; ret 0xc): a to the tyre block (+0x7f0), SetUnk10(b) on
    # the strut and Set414(c) on the tyre scratch (+0xf00). The scratch's word at +0x414 is past the wheel's first 0xf18 bytes.
    0x53A860: dict(name="CWheel::UnkA860", popped=0xC, stack_words=3, x87=False, thiscall=True, branch=no_branch,
                   direct_args=tail_args, windows=[(0, lambda a: 0xF18), (0, lambda a: 4, 0x1314)]),
    # NTweak tweak constructors and CBase::SetDirty, shared by the wheel's state and the mounted and body tweaks (thiscall).
    0x52F090: dict(direct_fuzz=tweak_base_fuzz, name="NTweak::CBase::CBase", popped=4, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    0x52FFF0: dict(direct_fuzz=fuzz_object(0x20), name="NTweak::CVector4::CVector4", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x20)]),
    0x52FEC0: dict(direct_fuzz=fuzz_object(0x10), name="NTweak::CUnsigned::CUnsigned", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    0x52FF80: dict(direct_fuzz=fuzz_object(0x20), name="NTweak::CVector::CVector", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x20)]),
    0x52FEA0: dict(direct_fuzz=fuzz_object(0x10), name="NTweak::CReal::CReal", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    0x52F450: dict(direct_fuzz=fuzz_object(0x10), name="NTweak::CBase::SetDirty", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    # E09Vec4::Clear (0x465d80, thiscall): the 16 zero bytes of the row at `this`.
    0x465D80: dict(direct_fuzz=fuzz_object(0x10), name="E09Vec4::Clear", popped=0, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    # CWheel::Unk97a0 (0x5397a0, thiscall, three stack arguments a, b, c; returns 1 when a sub-object is gated on).
    0x5397A0: dict(name="CWheel::Unk97a0", popped=0xC, stack_words=3, x87=False, thiscall=True, branch=no_branch,
                   fuzz=unk97a0_fuzz, direct_args=lambda rng, this: [this + 0x200, this + 0x220, this + 0x240],
                   windows=[(0, lambda a: 0xF18), (1, lambda a: 16), (2, lambda a: 16), (3, lambda a: 16)]),
    # CWheel::Unk94d0 (0x5394d0, thiscall on the sub-object, five stack arguments; the first is the vector v): inner only.
    # The sub-object (arg 1) is inside the wheel, so its window runs to the wheel's end (0xf18); the callee writes its fields.
    0x5394D0: dict(name="CWheel::Unk94d0", popped=0x14, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: a[0] + 0xF18 - a[1]), (2, lambda a: 0x10),
                            (3, lambda a: 0x10), (4, lambda a: 0x10), (5, lambda a: 0x10)]),
    # E51Wheel::CopyCF30 (0x53cf30, thiscall, three stack arguments a, b, c): CopyStateToDef(b) on the strut, CopyDefFrom(c)
    # on the tyre scratch, then copies a's words into the CDef (+0x234..+0x23c, +0x2a0 via CopyFrom, +0x414..+0x41c), SetDirty,
    # the blend +0x3fc, cos into +0x3d0 (sin dropped), and +0x3d4, +0x3d8 and +0x900.
    0x53CF30: dict(name="E51Wheel::CopyCF30", popped=0xC, stack_words=3, x87=False, thiscall=True, branch=no_branch,
                   fuzz=copycf30_fuzz, direct_args=lambda rng, this: [this + 0x200, this + 0x280, this + 0x2C0],
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x980, 0x34), (0, lambda a: 0xD0, 0xF10),
                            (1, lambda a: 0x80), (2, lambda a: 0x40), (3, lambda a: 0x40)]),
    # NTweak::CSpringDamper::CopyStateToDef (0x52dbd0, thiscall, one stack argument src): inner records only. It writes
    # the CDef through the strut's pointers: the spring block at +0x4 (CDef+0x450) and the tyre block at +0x4c (m_def).
    0x52DBD0: dict(name="CSpringDamper::CopyStateToDef", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x80), (1, lambda a: 0x80), (0, lambda a: 0x440, 0x4),
                            (0, lambda a: 0x320, 0x4C)]),
    # CTyre::CScratch::CopyDefFrom (0x534760, thiscall, one stack argument src): inner records only. Its state (the
    # scratch's m_state at +0x14, in the wheel: DamperUpdateB writes past +0x6f0) is the pointer window.
    0x534760: dict(name="CTyre::CScratch::CopyDefFrom", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x80, 0x14), (0, lambda a: 0x320, 0x10),
                            (1, lambda a: 0x40)]),
    # CWheel::E43Vf6C (0x539eb0, slot 27, thiscall, one float argument f): tail-forwards f to CSolid::E43Vf6C.
    0x539EB0: dict(name="CWheel::E43Vf6C", popped=4, x87=False, thiscall=True, branch=no_branch,
                   fuzz=lambda mu, args, rng, put: None,
                   direct_args=lambda rng, this: [f32_word(rng.uniform(-2, 2))],
                   windows=[(0, lambda a: 0xF18)]),
    # CWheel::E43Vf5C (0x539a90, slot 23, thiscall, no arguments): two virtual calls on the car (this+0x44), with
    # this+0x9f0 and this+0xa00, two E09Vec4::Clear calls on those blocks, then zero at +0xa10.
    0x539A90: dict(name="CWheel::E43Vf5C", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=lambda mu, args, rng, put: None, direct_args=no_args,
                   windows=[(0, lambda a: 0xF18)]),
    # The car's slot 28 and 29 (0x529340 `CSolid::E43Vf70`, 0x529ea0 `CSolid::E43Vf74`), each with a vector: inner records only.
    0x529340: dict(name="CCar::S70 (slot 28)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)], nodes=lambda mu, a: [(a[0] + 0x370, 0xC)]),
    0x529EA0: dict(name="CCar::S74 (slot 29)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)], nodes=lambda mu, a: [(a[0] + 0x310, 0xC)]),
    # CSolid::E43Vf6C (0x52cc30, thiscall, one float argument): inner records only.
    0x52CC30: dict(name="CSolid::E43Vf6C", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xF18)]),
    # CWheel::Unk9900 (0x539900, thiscall, stack x, y, z): Tag520940 on the two tags, BlendTable on the scratch at +0xf00,
    # E50BlendOps::Blend on this+0xaa4, then stores x, y, z to +0xc04, +0xc08, +0xc0c.
    0x539900: dict(name="CWheel::Unk9900", popped=0xC, stack_words=3, x87=False, thiscall=True, branch=no_branch,
                   fuzz=lambda mu, args, rng, put: None, direct_args=unk9900_direct_args, nodes=unk9900_nodes,
                   windows=[(0, lambda a: 0xF18), (0, lambda a: 0x40, 0xF00)]),
    # CTyre::CScratch::BlendTable (0x534e70, thiscall, stack i, j, a, b, o1, o2; returns int): inner records only.
    0x534E70: dict(name="CTyre::CScratch::BlendTable", popped=0x18, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x180), (5, lambda a: 4), (6, lambda a: 4)]),
    # E50BlendOps::Blend (0x5381f0, thiscall, stack a, b, c, d, e): inner records only.
    0x5381F0: dict(name="E50BlendOps::Blend", popped=0x14, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x100), (1, lambda a: 0x150), (2, lambda a: 0x150)]),
}
