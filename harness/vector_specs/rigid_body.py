# Vector set for the rigid-body port (E47, crates/rbr-physics/src/rigid_body). Names are the C++ names in
# src/rbr/RBR_E47_*.cpp; the Rust side names them by meaning (rust/rbr-physics/src/rigid_body/README.md (git 9c28c0b8)).
# Window layout: CSolid is 0x3d0 bytes at `this`; its CDef (0x1f0 bytes) is at the pointer stored at this+0x34.
SET_NAME = "rigid-body"


def sleep_branch(args, windows):
    """Branch of CSolid::Vf24x (0x52b740): 'off' (the def's Sleepable flag is 0), 'reset' (not below both
    thresholds: the timer is cleared), 'accum' (below both: the timer grows), 'trip' (the timer reaches
    SleepDelay with the armed flag: velocities cleared, S37 called)."""
    this, d = windows[0][3], windows[1][3]
    if struct.unpack_from("<I", d, 0x1AC)[0] == 0:
        return "off"
    k1, k2 = f32(struct.unpack_from("<I", d, 0x1CC)[0]), f32(struct.unpack_from("<I", d, 0x1DC)[0])
    v = [f32(struct.unpack_from("<I", this, 0x1C0 + 4 * i)[0]) for i in range(3)]
    w = [f32(struct.unpack_from("<I", this, 0x190 + 4 * i)[0]) for i in range(3)]
    if not (f32_round(k1 * k1) > f32_round(f32_round(f32_round(v[2] * v[2]) + f32_round(v[1] * v[1])) + f32_round(v[0] * v[0]))) \
            or not (f32_round(k2 * k2) > f32_round(f32_round(f32_round(w[2] * w[2]) + f32_round(w[1] * w[1])) + f32_round(w[0] * w[0]))):
        return "reset"
    t = f32_round(f32(struct.unpack_from("<I", this, 0x270)[0]) + f32(args[1]))
    k3 = f32(struct.unpack_from("<I", d, 0x1BC)[0])
    if struct.unpack_from("<I", this, 0x274)[0] != 0 and k3 <= t and t != k3:
        return "trip"
    return "accum"


def fuzz_sleep(mu, args, rng, put):
    """Random sleep inputs for Vf24x: Sleepable, the thresholds and delay, the velocities near their thresholds, the
    timer (near the delay, so the trip and its equal-time case are reached) and the armed flag."""
    this = args[0]
    d = word(mu, this + 0x34)
    k1, k2 = rng.uniform(0.0, 0.5), rng.uniform(0.0, 0.5)
    delay = rng.choice((0.0, 0.01, 0.2, 1.0))
    put(d + 0x1AC, rng.choice((0, 1, 1, 1)), "<I")
    put(d + 0x1BC, delay)
    put(d + 0x1CC, k1)
    put(d + 0x1DC, k2)
    # Each vector's three components share one ratio s to its threshold (each k/sqrt 3), so the squared speed is
    # s*s*k*k: below the threshold for s < 1, above it for s > 1. The ratios sit near 1 so both sides get drawn.
    s1, s2 = rng.choice((0.0, 0.5, 0.9, 0.99, 1.01, 1.2)), rng.choice((0.0, 0.5, 0.9, 0.99, 1.01, 1.2))
    for k in range(3):
        put(this + 0x1C0 + 4 * k, s1 * k1 / math.sqrt(3.0))
        put(this + 0x190 + 4 * k, s2 * k2 / math.sqrt(3.0))
    put(this + 0x270, rng.choice((0.0, 0.5 * delay, delay, delay - 0.005, 2.0 * delay)))
    put(this + 0x274, rng.choice((0, 0, 0, 1)), "<I")


def parent_window(mu, a):
    """The parent's first 0x64 bytes when the body has a parent (its flags byte at +0x60: the asm tests bit 0x4 there)."""
    parent = word(mu, a[0] + 0x44)
    return [(parent, 0x64)] if parent else []


def fuzz_pose(mu, args, rng, put):
    """Random pose inputs for VfC8: the body flags (bit 0x8 takes the sample), the orientation, the velocities, the
    position and the sample. The arguments (dt and the two flags) are the caller's and stay as they are."""
    this = args[0]
    put(this + 0x60, rng.choice((0x00, 0x08, 0x0C, 0x01, 0x09)), "<B")
    for k in range(4):
        put(this + 0x100 + 4 * k, rng.uniform(-1.0, 1.0))
    for k in range(3):
        put(this + 0x190 + 4 * k, rng.uniform(-4.0, 4.0))
        put(this + 0x1C0 + 4 * k, rng.uniform(-30.0, 30.0))
    for k in range(4):
        put(this + 0x140 + 4 * k, rng.uniform(-1000.0, 1000.0))
    put(this + 0x17C, rng.uniform(-1.0, 1.0))


def advance_branch(args, windows):
    """Branch of CSolid::VfEx (0x52d110): 'done' (the window is already at 1.0), else 'advance' with the cell
    registration ('root') or the parent ('child') and the state refresh."""
    this = windows[0][3]
    if not struct.unpack_from("<f", this, 0x290)[0] < 1.0:
        return "done"
    return "advance,%s" % ("root" if struct.unpack_from("<I", this, 0x44)[0] == 0 else "child")


def back_branch(args, windows):
    """Branch of CSolid::VfDx (0x52cfd0): 'done' (the fraction is not below the window's progress), else 'back' with the
    cell registration ('root') or the parent ('child') and the state refresh."""
    this = windows[0][3]
    if not f32(args[1]) < struct.unpack_from("<f", this, 0x290)[0]:
        return "done"
    return "back,%s" % ("root" if struct.unpack_from("<I", this, 0x44)[0] == 0 else "child")


def f32_bits(value):
    """The 32-bit pattern of a float (a fuzz_args value replaces the argument's stack word)."""
    return struct.unpack("<I", struct.pack("<f", value))[0]


def fuzz_window(mu, args, rng, put):
    """Random window inputs for VfEx and VfDx: half the bodies lose their parent (the root path, which registers the
    body in its cell), the fraction taken (progress), the orientation, the start pose and the velocities. The argument
    `a` is set by the spec's fuzz_args."""
    this = args[0]
    if rng.random() < 0.5:
        put(this + 0x44, 0, "<I")
    put(this + 0x290, rng.choice((0.0, rng.uniform(0.0, 1.0), 1.0)))
    for k in range(4):
        put(this + 0x100 + 4 * k, rng.uniform(-1.0, 1.0))
        put(this + 0x140 + 4 * k, rng.uniform(-1000.0, 1000.0))
        put(this + 0x2A0 + 4 * k, rng.uniform(-1.0, 1.0))
        put(this + 0x330 + 4 * k, rng.uniform(-1000.0, 1000.0))
    for k in range(3):
        put(this + 0x190 + 4 * k, rng.uniform(-4.0, 4.0))
        put(this + 0x1C0 + 4 * k, rng.uniform(-30.0, 30.0))


def fuzz_big(mu, args, rng, put):
    """Random contact inputs for BigA: the body's flags (bit 0x8 takes the sample) and the def's gain (+0x16c), the
    contact's two gains (+0x10, +0x14), the window's gains (+0x18, +0x1c), the point (+0x20), the normal (+0x30) and
    the vector (+0x40), and the tuning float (`g_1659b70 -> +0x24 -> +0x1608c`)."""
    this, e = args[0], args[5]
    put(this + 0x60, rng.choice((0x00, 0x08)), "<B")
    put(word(mu, this + 0x34) + 0x16C, rng.uniform(0.5, 2.0))
    put(e + 0x10, rng.choice((0.0, rng.uniform(0.0, 1.0), rng.uniform(0.0, 1.0), rng.uniform(0.0, 1.0))))
    put(e + 0x14, rng.choice((0.0, rng.uniform(-1.0, 0.0), rng.uniform(0.0, 1.0), rng.uniform(0.0, 1.0))))
    put(e + 0x18, rng.uniform(0.0, 1.0))
    put(e + 0x1C, rng.uniform(0.0, 1.0))
    for k in range(3):
        put(e + 0x20 + 4 * k, rng.uniform(-1.0, 1.0))
        put(e + 0x30 + 4 * k, rng.uniform(-1.0, 1.0))
        put(e + 0x40 + 4 * k, rng.uniform(-1.0, 1.0))
    put(word(mu, word(mu, 0x1659B70) + 0x24) + 0x1608C, rng.uniform(0.5, 2.0))


def tuning_node(mu, a):
    """The float BigA's damping reads: `g_1659b70 -> +0x24 -> +0x1608c` (a tuning value, not in the body)."""
    return [(word(mu, word(mu, 0x1659B70) + 0x24) + 0x1608C, 4)]


def pose_branch(args, windows):
    """Branch of CSolid::VfC8 (0x52c8f0): 'early' (a <= 0; 'early-neg' when a < 0 sets the flag), else 'pose', with the
    quaternion step (c), the position step (b), the parent case (this+0x44), and the contact sample (flags & 8)."""
    this = windows[0][3]
    a = f32(args[1])
    if not a > 0.0:
        return "early-neg" if a < 0.0 else "early"
    parent = struct.unpack_from("<I", this, 0x44)[0]
    flags = this[0x60]
    return "pose,c%d,b%d,%s,f8=%d" % (args[3] != 0, args[2] != 0, "parent" if parent else "root", (flags & 8) != 0)


def collect_after(mu, a, eax):
    """The contacts E36Collect returns (eax != 0): `count` records of 0x40 bytes at the list pointer. The list pointer
    is the second stack argument (`a[2]`) and the count the third (`a[3]`); the original's source names them the other way."""
    if not eax:
        return []
    lst, count = word(mu, a[2]), word(mu, a[3])
    return [(lst, 0x40 * count)] if lst and count else []


def fuzz_list_body(mu, args, rng, put):
    """Random body flags for BigB: bit 0x8 retires the kind-200 records (and takes the sample on the other calls)."""
    put(args[0] + 0x60, rng.choice((0x00, 0x08, 0x0C)), "<B")


def collect_result(mu, args, rng, write):
    """E36Collect with random contacts (--fuzz): none (eax 0), or one to three records in the collector's own list (its
    object, `args[0]`, + 0x90, as the original's E36Collect sets it: `*out = p + 0x90`). The list pointer (`args[2]`)
    and the count (`args[3]`) are written. The records' points are near the body (`args[1]`), with random directions,
    aux vectors, weights (negative, zero and positive), kinds (200 retires) and factors."""
    this = args[1]
    n = rng.choice((0, 1, 2, 3))
    base = args[0] + 0x90
    pos = [struct.unpack("<f", bytes(mu.mem_read(this + 0x140 + 4 * k, 4)))[0] for k in range(3)]
    for i in range(n):
        rec = base + 0x40 * i
        for k in range(3):
            write(rec + 4 * k, pos[k] + rng.uniform(-0.5, 0.5))
            write(rec + 0x10 + 4 * k, rng.uniform(-1.0, 1.0))
            write(rec + 0x20 + 4 * k, rng.uniform(-1.0, 1.0))
        write(rec + 0x20 + 12, rng.uniform(-1.0, 1.0))
        write(rec + 0x30, rng.choice((-0.5, 0.0, 0.5, 2.0)))
        write(rec + 0x34, rng.choice((200, 200, 7)), "<I")
        write(rec + 0x38, rng.choice((0.0, 0.3, 1.0, 2.5, -1.0)))
    write(args[2], base, "<I")
    write(args[3], n, "<I")
    return 1 if n else 0


def vote_result(mu, args, rng, write):
    """The vote (S00 of the global at 0x7c755c, --fuzz): votes for a random index and scale, or not (eax 0). The output
    is the caller's local buffer (`args[3]`, 56 bytes): the int at +0x20 and the float at +0x30."""
    votes = rng.choice((0, 1))
    if votes:
        write(args[3] + 0x20, rng.randrange(0, 8), "<I")
        write(args[3] + 0x30, rng.uniform(-2.0, 2.0), "<f")
    return votes


def gate_nodes(mu, a):
    """The two words E43Vf74 reads from the app globals: `app+0x24 -> +0x160f4` (q) and `app+0x38` (b). Their OR's bit
    0x4000 makes the call flush through slot 0."""
    p24 = word(mu, word(mu, 0x1659B70) + 0x24)
    return [(p24 + 0x160F4, 4), (word(mu, 0x1659B70) + 0x38, 4)]


def flush_gate(mu):
    """Bit 0x4000 of the OR of the two gate words (see gate_nodes): E43Vf74 and E43Vf70 then flush through slot 0."""
    return (word(mu, word(mu, 0x1659B70) + 0x38) | word(mu, word(mu, word(mu, 0x1659B70) + 0x24) + 0x160F4)) & 0x4000


def fuzz_gate(mu, args, rng, put):
    """Random gate words for E43Vf74 (--fuzz): q and b take one of 0, 1, 0x4000, 0x4001, so the flush (bit 0x4000 of
    their OR) runs on about three calls in four. With --fuzz this changes step 40's path for some seeds (see README)."""
    p24 = word(mu, word(mu, 0x1659B70) + 0x24)
    put(p24 + 0x160F4, rng.choice((0, 1, 0x4000, 0x4001)), "<I")
    put(word(mu, 0x1659B70) + 0x38, rng.choice((0, 1, 0x4000, 0x4001)), "<I")


def velocity_after(this, dt, at, acc):
    """The velocity at `at` after `+= acc * dt` (the asm's mulps, then addps: the product rounds, then the sum)."""
    return [f32_round(struct.unpack_from("<f", this, at + 4 * k)[0] + f32_round(struct.unpack_from("<f", this, acc + 4 * k)[0] * dt))
            for k in range(3)]


def squared_speed(v):
    """z*z + y*y + x*x in the asm's order, rounded to single precision at each step."""
    return f32_round(f32_round(f32_round(v[2] * v[2]) + f32_round(v[1] * v[1])) + f32_round(v[0] * v[0]))


def dt_choice(rng):
    """A step length for E43Vf6C (the game's 0.007 and a few longer ones; a zero or denormal would make 1/dt infinite)."""
    return rng.choice((0.007, 0.01, 0.02, 0.05))


# The clear threshold of E43Vf6C's squared speeds (0x795e8c, 1e-5 as a float).
CLEAR_SPEED_SQ = f32(0x3727C5AC)


def integrate_branch(args, windows):
    """Branch of CSolid::E43Vf6C (0x52cc30): 'no-f8' (the body's flag 0x8 is clear, so nothing is cleared), else 'keep'
    (the linear speed is not below the threshold), 'clear-lin' (it is, the angular speed is not) or 'clear-both'. The
    speeds are the ones after `+= accel * dt`, as the asm tests them."""
    this = windows[0][3]
    if not this[0x60] & 0x8:
        return "no-f8"
    dt = f32(args[1])
    if not squared_speed(velocity_after(this, dt, 0x1C0, 0x380)) < CLEAR_SPEED_SQ:
        return "keep"
    if squared_speed(velocity_after(this, dt, 0x190, 0x320)) < CLEAR_SPEED_SQ:
        return "clear-both"
    return "clear-lin"


def fuzz_integrate(mu, args, rng, put):
    """Random body inputs for E43Vf6C (--fuzz, and the direct runs). Flag 0x8 takes the clears, and its pose step takes the
    contact sample at the position (the x87 oracle has the drive's sample, not new ones), so those calls get velocities
    below the clear threshold and no accelerations: the position stays the drive's (`v * dt` is zero), and both the
    linear and the angular clears are drawn. Flag 0 calls get the sleep step's inputs (fuzz_sleep: the timer, thresholds,
    armed flag, so the trip is drawn) and random accelerations, the history and the pose."""
    this = args[0]
    flags = rng.choice((0x00, 0x08))
    put(this + 0x60, flags, "<B")
    if flags & 0x8:
        for k in range(3):
            put(this + 0x1C0 + 4 * k, rng.choice((0.0, rng.uniform(-0.001, 0.001))))
            put(this + 0x190 + 4 * k, rng.choice((0.0, rng.uniform(-0.003, 0.003), rng.uniform(-4.0, 4.0))))
            put(this + 0x380 + 4 * k, 0.0)
            put(this + 0x320 + 4 * k, 0.0)
            for at in (0x200, 0x220, 0x250, 0x260):
                put(this + at + 4 * k, rng.uniform(-1.0, 1.0))
        return
    fuzz_sleep(mu, args, rng, put)
    for k in range(3):
        put(this + 0x380 + 4 * k, rng.choice((0.0, rng.uniform(-0.1, 0.1), rng.uniform(-100.0, 100.0))))
        put(this + 0x320 + 4 * k, rng.choice((0.0, rng.uniform(-0.1, 0.1), rng.uniform(-10.0, 10.0))))
        for at in (0x200, 0x220, 0x250, 0x260):
            put(this + at + 4 * k, rng.uniform(-1.0, 1.0))
    for k in range(4):
        put(this + 0x100 + 4 * k, rng.uniform(-1.0, 1.0))


CONTACTS_BELOW_ESP = 0x30000  # where fuzz_contacts builds its list: far below every frame of the call


def fuzz_contacts(mu, this, rng, put):
    """The vote's contact list (the word at this+0x274; the count at +4, 0x60-byte records from +0x10): none, or one to
    three records built on the stack `CONTACTS_BELOW_ESP` below the call (the snapshot's own list sits among live
    objects). Each record is a BigA contact: the word at +0x04 (the sample hook's), the gains as fuzz_big draws them, the
    point within 1.5 of the body's position (so the depth takes both signs), a random normal and vector."""
    n = rng.choice((0, 1, 2, 3))
    if not n:
        put(this + 0x274, 0, "<I")
        return
    lst = (mu.reg_read(UC_X86_REG_ESP) - CONTACTS_BELOW_ESP) & ~0xF
    put(lst, this, "<I")
    put(lst + 4, n, "<I")
    pos = [struct.unpack("<f", bytes(mu.mem_read(this + 0x140 + 4 * k, 4)))[0] for k in range(3)]
    for i in range(n):
        e = lst + 0x10 + 0x60 * i
        mu.mem_write(e, bytes(0x60))
        put(e + 0x04, rng.choice((0, rng.randrange(1, 0x100))), "<I")
        put(e + 0x10, rng.choice((0.0, rng.uniform(0.0, 1.0), rng.uniform(0.0, 1.0))))
        put(e + 0x14, rng.choice((0.0, rng.uniform(-1.0, 0.0), rng.uniform(0.0, 1.0))))
        put(e + 0x18, rng.uniform(0.0, 1.0))
        put(e + 0x1C, rng.uniform(0.0, 1.0))
        for k in range(3):
            put(e + 0x20 + 4 * k, pos[k] + rng.uniform(-1.5, 1.5))
            put(e + 0x30 + 4 * k, rng.uniform(-1.0, 1.0))
            put(e + 0x40 + 4 * k, rng.uniform(-1.0, 1.0))
    put(this + 0x274, lst, "<I")


def fuzz_vote(mu, args, rng, put):
    """Random inputs for the vote (Vf81x, direct runs): the contact list (fuzz_contacts) and the body's flag 8 (BigA's
    sample, with the hook at this+0x3cc), the other flags kept."""
    this = args[0]
    flags = bytes(mu.mem_read(this + 0x60, 1))[0]
    put(this + 0x60, flags & ~0x08 | rng.choice((0x00, 0x08)), "<B")
    fuzz_contacts(mu, this, rng, put)


def direct_vote(mu, heap, rng):
    """Direct vote runs from an entry dump of a body call (BigA's, 0x52b9f0): the body is the dump's ecx, the two outputs
    are stub-heap vectors, and the range is the force pass's 1.0 or the hinge's 3.0."""
    a, b = heap + 0x800, heap + 0x810
    return [a, b, f32_bits(rng.choice((1.0, 3.0)))], [(a, bytes(0x20))], mu.reg_read(UC_X86_REG_ECX)


def contact_list_node(mu, a):
    """The vote's contact list: the header and the records (`count` of 0x60 bytes from +0x10), when there is one."""
    lst = word(mu, a[0] + 0x274)
    return [(lst, 0x10 + 0x60 * word(mu, lst + 4))] if lst else []


def fuzz_force(mu, args, rng, put):
    """Random body inputs for E43Vf64 (--fuzz, and the direct runs): flag 0x8 or none, the contact list (fuzz_contacts),
    the force and the scratch torque, and the definition's inverse mass and inverse inertia diagonal (the force pass
    scales by them)."""
    this = args[0]
    put(this + 0x60, rng.choice((0x00, 0x08)), "<B")
    fuzz_contacts(mu, this, rng, put)
    for k in range(4):
        put(this + 0x370 + 4 * k, rng.choice((0.0, rng.uniform(-10.0, 10.0), rng.uniform(-1000.0, 1000.0))))
        put(this + 0x310 + 4 * k, rng.choice((0.0, rng.uniform(-10.0, 10.0))))
    definition = struct.unpack("<I", bytes(mu.mem_read(this + 0x34, 4)))[0]
    put(definition + 0x16C, rng.uniform(0.0001, 0.1))
    for k in range(4):
        put(definition + 0x130 + 4 * k, rng.uniform(0.0, 0.05))


def fuzz_emit(mu, args, rng, put):
    """Random body inputs for CSolid::E43Vf90 and E43Vf94 (direct runs): the owner word, the pose the event copies, the armed
    word and the active flag (E43Vf90 clears the armed word and sets the active flag, E43Vf94 clears both)."""
    this = args[0]
    put(this + 0x30, rng.getrandbits(32), "<I")
    put(this + 0x274, rng.choice((0, 1)), "<I")
    put(this + 0x1DC, rng.choice((0, 1)), "<I")
    for k in range(3):
        put(this + 0x140 + 4 * k, rng.uniform(-100.0, 100.0))
    for k in range(4):
        put(this + 0x100 + 4 * k, rng.uniform(-1.0, 1.0))


def fuzz_vf140(mu, heap, rng):
    """E43Vf140 without a parent (direct runs): a body object in the stub heap with no parent at +0x44, so the call takes its
    own path. Random position, angular and linear velocities; the point `a` and the output `b` (random, the result replaces
    it) are in the heap too."""
    body = heap + 0x1000
    a, b = heap + 0x800, heap + 0x810
    writes = [(body + 0x44, struct.pack("<I", 0))]
    for k in range(3):
        writes.append((body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((body + 0x190 + 4 * k, struct.pack("<f", rng.uniform(-3.0, 3.0))))
        writes.append((body + 0x1C0 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
        writes.append((a + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((b + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
    return [a, b], writes, body


def fuzz_vf128(mu, heap, rng):
    """CSolid::E43Vf128 (direct runs): a body object in the stub heap with its definition (inverse mass and inverse inertia
    diagonal) at heap+0x2000, the point at heap+0x800, the impulse at heap+0x810; the body's hook (+0x3cc) and ring tail
    (+0x3c8) are null, so the call takes its arithmetic path only."""
    body, definition = heap + 0x1000, heap + 0x2000
    a, b = heap + 0x800, heap + 0x810
    # The body's vtable (the slot-12 call reads it): the snapshot's CSolid at 0xbf1d7b8 has the class's.
    vtable = bytes(mu.mem_read(0xBF1D7B8, 4))
    writes = [(body, vtable), (body + 0x34, struct.pack("<I", definition)),
              (body + 0x3CC, struct.pack("<I", 0)), (body + 0x3C8, struct.pack("<I", 0)),
              (definition + 0x16C, struct.pack("<f", rng.uniform(0.001, 0.1)))]
    for k in range(3):
        writes.append((body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((body + 0x190 + 4 * k, struct.pack("<f", rng.uniform(-3.0, 3.0))))
        writes.append((body + 0x1C0 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
        writes.append((definition + 0x130 + 4 * k, struct.pack("<f", rng.uniform(0.0, 0.05))))
        writes.append((a + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((b + 4 * k, struct.pack("<f", rng.uniform(-1000.0, 1000.0))))
    return [a, b, heap + 0x820, heap + 0x830, heap + 0x840, heap + 0x850, heap + 0x860, heap + 0x870], writes, body


def fuzz_impulse640(mu, heap, rng):
    """E46Impulse::Impulse640: an accumulator object, a four-lane direction (lane 3 can be stack residue), and
    penetration/friction inputs; tune the globals to cover the clamp and no-op paths."""
    body, direction = heap + 0x1000, heap + 0x800
    app = word(mu, 0x1659B70)
    settings = word(mu, app + 0x24)
    writes = [(app + 0x24, struct.pack("<I", settings))]
    for k in range(4):
        writes.append((body + 0x1C0 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))))
        writes.append((direction + 4 * k, struct.pack("<f", rng.uniform(-2.0, 2.0))))
    writes += [
        (settings + 0x1610C, struct.pack("<f", rng.choice((0.0, 0.01, 0.1)))),
        (settings + 0x1611C, struct.pack("<f", rng.uniform(0.1, 2.0))),
    ]
    penetration = rng.choice((0.005, 0.02, 0.1, 1.0))
    mu_value = rng.choice((0.0, 0.1, 0.5, 2.0))
    return [direction, f32_bits(penetration), 0, f32_bits(mu_value)], writes, body


def impulse640_nodes(mu, args):
    app = word(mu, 0x1659B70)
    settings = word(mu, app + 0x24)
    return [(0x1659B70, 4), (app + 0x24, 4), (settings + 0x1610C, 0x14)]


def fuzz_vfa0(mu, heap, rng):
    """CSolid::E43VfA0: random body origin and 3x3 matrix with one to four rows, including lane 3 subtraction."""
    body, rows = 0xBF1D7B8, heap + 0x800
    count = rng.randint(1, 4)
    writes = [(body + 0x110 + 4 * k, struct.pack("<f", rng.uniform(-2.0, 2.0))) for k in range(9)]
    writes += [(body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))) for k in range(4)]
    writes += [(rows + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))) for k in range(4 * count)]
    return [rows, count], writes, body


def fuzz_inverse_mass(mu, heap, rng):
    body, definition = heap + 0x1000, heap + 0x2000
    writes = [(body + 0x34, struct.pack("<I", definition)),
              (definition + 0x16C, struct.pack("<f", rng.uniform(0.001, 1.0)))]
    return [], writes, body


def fuzz_accum340(mu, heap, rng):
    """CSolid::E43Vf70 (Accum340) on the snapshot's body (direct runs): the scale at +0xd0, the force at +0x370 and the vector
    at heap+0x800 random, and the gate words as in fuzz_gate, so the flush runs on about three calls in four."""
    body = 0xBF1D7B8
    app = word(mu, 0x1659B70)
    writes = [(body + 0xD0, struct.pack("<f", rng.uniform(-2.0, 2.0)))]
    for k in range(4):
        writes.append((body + 0x370 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))))
        writes.append((heap + 0x800 + 4 * k, struct.pack("<f", rng.uniform(-1000.0, 1000.0))))
    writes.append((word(mu, app + 0x24) + 0x160F4, struct.pack("<I", rng.choice((0, 1, 0x4000, 0x4001)))))
    writes.append((app + 0x38, struct.pack("<I", rng.choice((0, 1, 0x4000, 0x4001)))))
    return [heap + 0x800], writes, body


def fuzz_block(mu, args, rng, put):
    """E47SolidScratch::Vf04 (direct runs): the scratch block's 0x150 bytes random, so the cleared words and the kept ones
    both differ from zero."""
    for at in range(0, 0x150, 4):
        put(args[0] + at, rng.uniform(-100.0, 100.0))


def fuzz_quat_mat(mu, heap, rng):
    """E01Mat3::SetFromQuat (direct runs): the quaternion at heap+0x800 has four random components (not normalised; the
    arithmetic is the same), the matrix at heap+0x900 starts random."""
    writes = [(heap + 0x800 + 4 * k, struct.pack("<f", rng.uniform(-1.0, 1.0))) for k in range(4)]
    writes += [(heap + 0x900 + 4 * k, struct.pack("<f", rng.uniform(-2.0, 2.0))) for k in range(9)]
    return [heap + 0x800], writes, heap + 0x900


def fuzz_rows(mu, heap, rng, count):
    """Row loops (direct runs): the body's matrix at +0x110 (ten words, +0x134 the fourth lane's) and `count` rows at
    heap+0x800 are random. Returns the stack words (rows, count) and the body."""
    body = 0xBF1D7B8
    writes = [(body + 0x110 + 4 * k, struct.pack("<f", rng.uniform(-2.0, 2.0))) for k in range(10)]
    writes += [(heap + 0x800 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))) for k in range(4 * count)]
    return [heap + 0x800, count], writes, body


def fuzz_acb(mu, heap, rng):
    return fuzz_rows(mu, heap, rng, rng.randint(1, 4))


def fuzz_quat_rows(mu, heap, rng):
    """Quaternion loops (direct runs): the body's start orientation at +0x2a0 and the vector at +0x330 random, then `count`
    random rows at heap+0x800 (as fuzz_acb)."""
    body = 0xBF1D7B8
    count = rng.randint(1, 4)
    writes = [(body + 0x2A0 + 4 * k, struct.pack("<f", rng.uniform(-1.0, 1.0))) for k in range(4)]
    writes += [(body + 0x330 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))) for k in range(3)]
    writes += [(heap + 0x800 + 4 * k, struct.pack("<f", rng.uniform(-100.0, 100.0))) for k in range(4 * count)]
    return [heap + 0x800, count], writes, body


def fuzz_state(mu, args, rng, put):
    """E47SolidState::Vf17x (direct runs, this = the state at body+0xe0): the orientation at +0x20 random, the gate at +0x7c
    positive on about half the calls, and the diagonal at +0x70..0x78 random (its three words)."""
    this = args[0]
    for k in range(4):
        put(this + 0x20 + 4 * k, rng.uniform(-1.0, 1.0))
    put(this + 0x7C, rng.uniform(-1.0, 2.0))
    for k in range(3):
        put(this + 0x70 + 4 * k, rng.uniform(0.5, 2.0))


def fuzz_body_block(mu, args, rng, put):
    """The body's scratch block at +0x280 random, but its vtable word (the first) kept: the helpers jump through it."""
    for at in range(0x284, 0x280 + 0x150, 4):
        put(args[0] + at, rng.uniform(-100.0, 100.0))


def fuzz_vf39(mu, heap, rng):
    """E47Solid::Vf39x (direct runs; this = the snapshot's body): the matrix (+0x110) and origin (+0x140) random, then
    `n` random rows at heap+0x800 (the stack words are the rows and their count)."""
    words, writes, body = fuzz_rows(mu, heap, rng, rng.randint(1, 4))
    writes += [(body + 0x140 + 4 * k, struct.pack("<f", rng.uniform(-10.0, 10.0))) for k in range(4)]
    return [heap + 0x800, words[1]], writes, body


def fuzz_vf16(mu, heap, rng):
    """E47_Vf16Helper (direct runs): a body in the stub heap with no parent and no children, its CDef a zeroed block, and
    the state at +0xe0 (E47SolidState's vtable, 0x73fc04) with fuzz_state's orientation, gate and diagonal. The tail
    (CBody::Vf40 and the entity walk) calls slot 17 of the embedded objects at +0x38 and +0x70 (their vtable words) and
    slot 12 of their owner (a bare ret, 0x401040) through the owner pointer at +0xa0. The object at +0x38 has a bare ret
    in slot 17 (its Unk44 would write the walk's child pointer at +0x48)."""
    body = heap + 0x1000
    state = body + 0xE0
    definition, owner = heap + 0x2000, heap + 0x3000
    vtable_unk, vtable_owner, vtable_ret = heap + 0x3100, heap + 0x3200, heap + 0x3400
    vtable = bytes(mu.mem_read(0xBF1D7B8, 4))
    writes = [(body, vtable), (body + 0x34, struct.pack("<I", definition)), (definition, bytes(0x1F0)),
              (body + 0x44, struct.pack("<I", 0)), (body + 0x48, struct.pack("<I", 0)),
              (body + 0x38, struct.pack("<I", vtable_ret)), (vtable_ret, bytes(0x80)), (vtable_ret + 0x44, struct.pack("<I", 0x401040)),
              (body + 0x70, struct.pack("<I", vtable_unk)), (body + 0xA0, struct.pack("<I", owner)),
              (vtable_unk, bytes(0x80)), (vtable_unk + 0x44, struct.pack("<I", 0x4E33E0)),
              (vtable_owner, bytes(0x80)), (vtable_owner + 0x30, struct.pack("<I", 0x401040)),
              (owner, bytes(0x80)), (owner, struct.pack("<I", vtable_owner)),
              (state, struct.pack("<I", 0x73FC04))]
    writes += [(state + 0x20 + 4 * k, struct.pack("<f", rng.uniform(-1.0, 1.0))) for k in range(4)]
    writes.append((state + 0x7C, struct.pack("<f", rng.uniform(-1.0, 2.0))))
    writes += [(state + 0x70 + 4 * k, struct.pack("<f", rng.uniform(0.5, 2.0))) for k in range(3)]
    return [], writes, body


def fuzz_newdef(mu, heap, rng):
    """CSolid::NewDef (direct runs): the name argument is a zeroed-tail "DefSolid" string in the stub heap (the base
    constructor keeps the pointer)."""
    name = heap + 0x800
    return [name], [(name, b"DefSolid\x00")], 0


def fuzz_state_ctor(mu, heap, rng):
    """E47SolidState's constructor (direct runs): the owner is a body-shaped block at heap+0x3000 whose flags byte (+0x60)
    is random (bit 2 decides whether the state keeps it); the name is a string in the stub heap; the state is a random
    block at heap+0x3800. The unused third argument is zero."""
    owner, name, state = heap + 0x3000, heap + 0x3400, heap + 0x3800
    owner_bytes = bytearray(0x64)
    owner_bytes[0x60] = rng.randrange(256)
    state_bytes = bytes(rng.randrange(256) for _ in range(0x198))
    return [owner, name, 0], [(owner, bytes(owner_bytes)), (name, b"State\x00"), (state, state_bytes)], state


def fuzz_cdef(mu, heap, rng):
    """CSolid::CDef (direct runs): the block is a random 0x1f0-byte definition at heap+0x800; the name is a string in the
    stub heap (the body definition's base keeps the pointer)."""
    block, name = heap + 0x800, heap + 0x2000
    return [name], [(block, bytes(rng.randrange(256) for _ in range(0x1F0))), (name, b"DefSolid\x00")], block


SPECS = {
    # args: this, dt. Returns 1 when the body went to sleep (velocities cleared), else 0.
    0x52B740: dict(name="CSolid::Vf24x", popped=4, x87=False, branch=sleep_branch, thiscall=True,
                   windows=[(0, lambda a: 0x3D0), (0, lambda a: 0x1F0, 0x34)], fuzz=fuzz_sleep,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 37), 0)]),  # S37 on a trip (a vtable call)
    # Pose step (VfC8): this, a = dt, b = update position, c = update orientation. Windows: the body.
    # The parent's first 0x64 bytes are a window when the body has a parent (this+0x44); the parent's slot 0x30 (S30, three
    # stack arguments) is the parent's transform, which the parent case calls.
    0x52C8F0: dict(name="CSolid::VfC8", popped=12, x87=False, branch=pose_branch, thiscall=True,
                   windows=[(0, lambda a: 0x3D0)], nodes=parent_window, fuzz=fuzz_pose,
                   dispatch=lambda mu, a: [(vtarget(mu, word(mu, a[0] + 0x44), 12), 12)] if word(mu, a[0] + 0x44) else []),
    # Back step to a fraction of the window (VfDx): this, a = fraction. Windows: the body; S16 refreshes the state.
    0x52CFD0: dict(name="CSolid::VfDx", popped=4, x87=False, branch=back_branch, thiscall=True,
                   windows=[(0, lambda a: 0x3D0)], dispatch=lambda mu, a: [(vtarget(mu, a[0], 16), 0)],
                   fuzz=fuzz_window, fuzz_args=lambda args, rng: {1: f32_bits(rng.uniform(0.0, 1.0))}),
    # Advance the rest of the window (VfEx): this, a = dt. Windows: the body. S16 (vtable slot 16) refreshes the state.
    0x52D110: dict(name="CSolid::VfEx", popped=4, x87=False, branch=advance_branch, thiscall=True,
                   windows=[(0, lambda a: 0x3D0)], dispatch=lambda mu, a: [(vtarget(mu, a[0], 16), 0)],
                   fuzz=fuzz_window, fuzz_args=lambda args, rng: {1: f32_bits(rng.uniform(0.0, 0.05))}),
    # Contact vote (Vf81x): this, a and b are the two output vectors (cleared, then summed over the contacts), c the
    # contact range. The contacts are a list at this+0x274 (count at +4, 0x60-byte records from +0x10).
    # Node window 3: the list (contact_list_node). The direct runs fuzz it (fuzz_vote).
    0x52C510: dict(name="CSolid::Vf81x", popped=12, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10), (2, lambda a: 0x10)], nodes=contact_list_node,
                   fuzz=fuzz_vote, direct_fuzz=direct_vote),
    # The cell grid lookup (CCellGrid::LookupTwin, 0x4e03b0) and the car's cell append (CCar::AppendA980, 0x4ea980).
    0x4E03B0: dict(name="CCellGrid::LookupTwin", popped=8, x87=False, thiscall=True, external=True, windows=[]),
    0x4EA980: dict(name="CCar::AppendA980", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    # The contact sample (CDriveLine::SampleBig, 0x50b300): this = the drive line, pos, x; returns a float (st0).
    0x50B300: dict(name="CDriveLine::SampleBig", popped=8, x87=True, thiscall=True, external=True, windows=[]),
    # The matrix step (E33Mat34::ComposeA, 0x51b4c0): ecx = this, then three stack words; the last is the result
    # (16 bytes), as BigA uses it. The parent case (VfC8) passes the same three words.
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True, windows=[],
                   after=lambda mu, a, eax: [(a[3], 0x10)]),
    # Quaternion product out = p * q (E15Shell::M83DD0): this = p, a = q, b = out (returns out).
    0x483DD0: dict(name="E15Shell::M83DD0", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # Unit quaternion, in place (E18_QuatNormalize, fastcall: ecx = q; returns q).
    0x4921A0: dict(name="E18_QuatNormalize", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x10)]),
    # Unit quaternion to an output (E34Quat::Norm: this = v, a = out; returns out).
    0x4E4C70: dict(name="E34Quat::Norm", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)]),
    # The contact worker (CSolid::BigA, 0x52b9f0): this, then a, b, c, d, e (pointers) and f (a float). Returns the
    # contact flag (eax). Its callees are listed below; the S30/S80/S32 virtuals and 0x4e6000 are the body's own.
    0x52B9F0: dict(name="CSolid::BigA", popped=0x18, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x30), (2, lambda a: 0x10), (3, lambda a: 4),
                            (4, lambda a: 4), (5, lambda a: 0x50), (0, lambda a: 0x1F0, 0x34)],
                   nodes=tuning_node, fuzz=fuzz_big,
                   dispatch=lambda mu, a: [(vtarget(mu, word(mu, a[0] + 0x3CC), 2), 0x20)] if word(mu, a[0] + 0x3CC) else []),
    # The body's slot 80 (S80, popped 8): fills the contact's three-float vector at arg 2 from arg 1 (the matrix step).
    0x51E5C0: dict(name="CSolid::S80", popped=8, x87=False, thiscall=True, external=True,
                   windows=[], after=lambda mu, a, eax: [(a[2], 0xC)]),
    # CSolid::E43Vf128 (slot 74, 0x52a0e0; the impulse, `Vf74x`): the point at arg 1, the impulse at arg 2; args 3..8 are the
    # hook and tail arguments (not read on the arithmetic path). Popped 0x20.
    0x529640: dict(name="E46Impulse::Impulse640", popped=16, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_impulse640, windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10)],
                   nodes=impulse640_nodes),
    # The inverse-mass accessor returns the definition's +0x16c as an x87-promoted f32.
    0x5297D0: dict(name="CSolid::E43Vf13C", popped=0, x87=True, thiscall=True, external=True,
                   direct_fuzz=fuzz_inverse_mass, windows=[(0, lambda a: 0x3D0), (0, lambda a: 0x1F0, 0x34)]),
    0x52A0E0: dict(name="CSolid::E43Vf128", popped=0x20, x87=False, thiscall=True, direct_fuzz=fuzz_vf128,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10), (2, lambda a: 0x10), (0, lambda a: 0x1F0, 0x34)]),
    # CSolid::E43Vf140, the body's slot 80 in the classes BigB's dumps have (0x51e5c0 is a thunk to it, the one BigA's
    # dumps call). Fills the point at arg 2 (12 bytes) from arg 1. A child body's call goes on to its parent's slot 80.
    0x52A570: dict(name="CSolid::E43Vf140", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_vf140, windows=[(0, lambda a: 0x3D0), (1, lambda a: 0xC), (2, lambda a: 0xC)],
                   after=lambda mu, a, eax: [(a[2], 0xC)]),
    # CSolid::E43VfA0 (0x52ad10): subtracts the body's origin from every four-lane row, then transforms xyz by its 3x3 matrix.
    0x52AD10: dict(name="CSolid::E43VfA0", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_vfa0, windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))]),
    # The body's slot 12 (S30, popped 0 here: the pushed argument stays for MB560): returns the body itself.
    0x519C00: dict(name="CSolid::S30", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CFloat3::Length (0x473b30): the 3-component length of the vector at ecx (returns st0).
    0x473B30: dict(name="NTweak::CFloat3::Length", popped=0, x87=True, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # CFloat3::Scale (0x473cd0): out = s * v (four lanes); ecx = v, then s and out.
    0x473CD0: dict(name="NTweak::CFloat3::Scale", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: 0x10)]),
    # E43PairWalk::MB560 (0x51b560): a transform of the vector at arg 1 by the matrix at arg 2, into arg 3.
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x40), (3, lambda a: 0x10)]),
    # The cross product (0x43c630): out = this x a, ecx = this, a, then out.
    0x43C630: dict(name="E01V3::Cross", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # The contact sample sink (0x4e6000): this, then the body's vector and two output pointers (the same one).
    0x4E6000: dict(name="CSolid::Vf4000", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 4)]),
    # CSolid::Step77 (0x529d30): the vote for the position `this+0x140` shifted by `*b` on z. The vote is slot 0 of the
    # global at 0x7c755c (S00, three stack words: the position, the shifted point, the output); when it votes, `*a` is
    # the output's int at +0x20 and `*b` is scaled by its float at +0x30. Otherwise `*a` is 1.0 and eax 0.
    0x529D30: dict(name="CSolid::Step77", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 4), (2, lambda a: 4)],
                   dispatch=lambda mu, a: [(vtarget(mu, word(mu, 0x7C755C), 0), 12)]),
    # CSolid::E43Vf74 (slot 29, the torque apply): acc (this+0x310, window 0's offset) += pp * m_fD0 (this+0xd0), then
    # a slot-0 flush of this when the gate word has bit 0x4000. Windows: the body, pp; node windows 2 and 3 are the gate
    # words (q at app+0x24 -> +0x160f4, b at app+0x38).
    0x529EA0: dict(name="CSolid::E43Vf74", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10)], nodes=gate_nodes, fuzz=fuzz_gate,
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 0), 0)] if flush_gate(mu) else []),
    # CSolid::Dot60 (0x52a500, x87 double): this, a (an E01VecSetA, 0x30 bytes), b (E01V3), c (three floats). Windows:
    # a, b, c, and the def's 0x130 vector (`Mul` takes it as its receiver). The frame's stale temporaries (the words the
    # Mul and the second Cross read before they write them) are the entry stack, not a window.
    0x52A500: dict(name="CSolid::Dot60", popped=12, x87=True, thiscall=True,
                   windows=[(1, lambda a: 0x30), (2, lambda a: 0xC), (3, lambda a: 0xC)],
                   nodes=lambda mu, a: [(word(mu, a[0] + 0x34) + 0x130, 0x10)]),
    # Mul (0x51c8c0): ecx = the def's 0x130 vector, then b and out (16 bytes each); the out is the frame's temporary.
    0x51C8C0: dict(name="E01Vec4::Mul", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # CSolid::E43Vf138 (slot 78, popped 24): this, a (E01VecSetA), b, c, the int d, the float pointers e and f. Calls
    # Dot60 twice. Windows: a, b, c, e, f; the def's 0x130 vector (Dot60's scale) and its +0x16c float (`k`).
    # Fuzzed `dp` (argument 4) is 0 or 1: dp == 0 skips the `f` term.
    0x52B990: dict(name="CSolid::E43Vf138", popped=24, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x30), (2, lambda a: 0xC), (3, lambda a: 0xC), (5, lambda a: 4), (6, lambda a: 4)],
                   nodes=lambda mu, a: [(word(mu, a[0] + 0x34) + 0x130, 0x10), (word(mu, a[0] + 0x34) + 0x16C, 4)],
                   fuzz_args=lambda args, rng: {4: rng.choice((0, 1))}),
    # CWheel::E43Vf28 (0x53f230, slot 10 of a wheel): the caller of Step77 (0x53f36f). Listed so that an entry dump of
    # it carries the Step77 call (and its vote). No windows: the wheel is not a CSolid window.
    0x53F230: dict(name="CWheel::E43Vf28", popped=0, x87=False, thiscall=True, windows=[]),
    # The vote (S00 of the global at 0x7c755c, 0x4fe810; the caller is Step77): this, the position (window 1), the point
    # (window 2) and the 56-byte output (window 3; its int at +0x20 and float at +0x30 are read after the call).
    0x4FE810: dict(name="E34Call0_3::S00 (vote)", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x38)],
                   after=lambda mu, a, eax: [(a[3], 0x38)], result=vote_result),
    # CEntity clear (E09Vec4::Clear, 0x465d80): called by Vf24x on a trip. Returns this.
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CSolid::BigB (0x52c220): the contact list step. Windows: the body. Its calls outside the body are E36Collect (the
    # list), E34_RetireA (kind 200 on a body with flag 8), the body's slot 80 and slot 12 (MB560 transforms), and the
    # slot 28 and 29 applies (dispatch below), then Emit (DampingCollision, 0x52be70) on the event.
    # Node window 1: the global E43PairWalk receiver (g_7cbb00) that the MB560 calls take in ecx (a child's MB560 takes the body).
    0x52C220: dict(name="CSolid::BigB", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x3D0)],
                   fuzz=fuzz_list_body, nodes=lambda mu, a: [(0x7CBB00, 4)],
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 28), 4), (vtarget(mu, a[0], 29), 4)]),
    # The contact collector (0x4f3950): ecx = the collision world, then the body, the list pointer cell and the count
    # cell (windows 0 and 1 of the call).
    0x4F3950: dict(name="E36Collect", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 4), (3, lambda a: 4)], after=collect_after, result=collect_result),
    # The kind-200 retire (0x4e8fa0): ecx = the body, no stack arguments. Its writes are the car's (not compared).
    0x4E8FA0: dict(name="E34_RetireA", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # The list record copies (E01Vec4::CopyFrom, 0x43c5b0): ecx = the destination, then the source.
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)]),
    # DampingCollision Emit (0x52be70): ecx = the signal object, then the event (0x44 bytes; its callees are the game's).
    0x52BE70: dict(name="CMultiSignal1::Emit<DampingCollision>", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x44)]),
    # CSolid::E43Vf6C (slot 27, 0x52cc30; Vf27x): dt (popped 4). The integrator pass: the velocities take accel * dt, the
    # flag-8 clears, VfC8 (listed), the accel estimates and filters, the entity walk (0x51a330), Vf24x (listed), then the
    # state signal. Windows: the body and the def (the sleep step's). The parent's first 0x64 bytes when there is one.
    0x52CC30: dict(name="CSolid::E43Vf6C", popped=4, x87=False, thiscall=True, branch=integrate_branch,
                   windows=[(0, lambda a: 0x3D0), (0, lambda a: 0x1F0, 0x34)], nodes=parent_window, fuzz=fuzz_integrate,
                   fuzz_args=lambda args, rng: {1: f32_bits(dt_choice(rng))},
                   direct_args=lambda rng, this: [f32_bits(dt_choice(rng))]),
    # The entity walk E43Vf6C makes on the body (CEntity::E43Vf6C, 0x51a330): dt on the stack (popped 4). Its own calls
    # are not listed; the externals it makes land in the caller's inner records.
    0x51A330: dict(name="CEntity::E43Vf6C", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    # CSolid::E43Vf64 (slot 25, 0x52cb20; Vf25x): no arguments. The force pass: the entity walk (0x51a2c0), the contact vote
    # (Vf81x, listed), BigB when the body's flag 8 is set (its applies write the accumulators at +0x310 and +0x370), then the
    # two MB560 transforms of the accumulators through the body (the walker global is node window 2).
    0x52CB20: dict(name="CSolid::E43Vf64", popped=0, x87=False, thiscall=True, fuzz=fuzz_force,
                   windows=[(0, lambda a: 0x3D0), (0, lambda a: 0x1F0, 0x34)], nodes=lambda mu, a: [(0x7CBB00, 4)]),
    # The gated entity walk E43Vf64 makes on the body (CEntity::E43Vf64, 0x51a2c0): no arguments. Its children's calls
    # come before it in the inner records; the body's own calls after it.
    0x51A2C0: dict(name="CEntity::E43Vf64", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CSolid::E43Vf70 (Accum340, slot 28): the force applied to the body (+0x370 += v * scale). Windows: the body and v.
    0x529340: dict(name="CSolid::E43Vf70 (E46_Accum340)", popped=4, x87=False, thiscall=True, external=True, direct_fuzz=fuzz_accum340,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 0x10)],
                   dispatch=lambda mu, a: [(vtarget(mu, a[0], 0), 0)] if flush_gate(mu) else []),
    # CSolid::E47SolidScratch::Vf04 (slot 1 of the scratch block, the body's +0x280; ecx = the block, 0x529af0). Direct runs
    # only: the block is random (fuzz_block) and the post window checks all of it.
    0x529AF0: dict(name="E47SolidScratch::Vf04", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x150)], fuzz=fuzz_block),
    # E47_ScratchBaseInit (0x52b930, the scratch's base; ecx = the block): the vtable word, 1.0 at +0x10, two clears, then
    # Vf04. Direct runs only (fuzz_block); the physics-core spec has the same call with no window.
    0x52B930: dict(name="E47_ScratchBaseInit", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x150)], fuzz=fuzz_block),
    # E01Mat3::SetFromQuat (0x43c6e0; ecx = the matrix, the quaternion on the stack; the matrix is stored row-major, out[3*row+col]).
    # Shared by E47SolidState::Vf17x and the integrator-side loops. Direct runs only (fuzz_quat_mat).
    0x43C6E0: dict(name="E01Mat3::SetFromQuat", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 36), (1, lambda a: 16)], direct_fuzz=fuzz_quat_mat),
    # CSolid::E43VfAC (0x52ad40, the packed row loop over the +0x110 matrix; ecx = the body, args (rows, count)). Direct runs
    # only (fuzz_acb). The matrix is the body's; the rows are the window at args[1], 16 bytes each.
    0x52AD40: dict(name="CSolid::E43VfAC", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_acb),
    # CSolid::E43VfB0 (0x52add0, the 3-wide row loop over the +0x110 matrix; args (rows, count) like E43VfAC). Direct runs only
    # (fuzz_acb). Stores three lanes; the fourth is left as it was.
    0x52ADD0: dict(name="CSolid::E43VfB0", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_acb),
    # CSolid::E43VfBC (0x52aec0, the origin-folded row loop; args (rows, count)). Direct runs only (fuzz_acb). Reads the
    # matrix and the origin (+0x140..0x148) from the body window.
    0x52AEC0: dict(name="CSolid::E43VfBC", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_acb),
    # CSolid::E43VfC0 (0x52afd0, the m[9] fold of the same shape; args (rows, count)). Direct runs only (fuzz_acb).
    0x52AFD0: dict(name="CSolid::E43VfC0", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_acb),
    # CSolid::E43VfA4 (0x52b110): per row, CopyFrom, the quaternion at +0x2a0 as a matrix, and the matrix times the row plus the
    # vector at +0x330. Direct runs only (fuzz_quat_rows).
    0x52B110: dict(name="CSolid::E43VfA4", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # CSolid::E43VfA8 (0x52b1f0): per row, the quaternion's matrix (SetFromQuat) applied to the row less the vector at +0x330.
    0x52B1F0: dict(name="CSolid::E43VfA8", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # CSolid::E43VfB4 (0x52b2e0): per row, the quaternion's matrix applied to the row (no offset).
    0x52B2E0: dict(name="CSolid::E43VfB4", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # CSolid::E43VfB8 (0x52b380): per row, SetFromQuat then the three sums. Direct runs only (fuzz_quat_rows).
    0x52B380: dict(name="CSolid::E43VfB8", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # CSolid::E43VfC4 (0x52b460): SetFromQuat once, then per row the sums and the offset fold.
    0x52B460: dict(name="CSolid::E43VfC4", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # CSolid::E43VfC8 (0x52b580): as E43VfC4, with the other sums.
    0x52B580: dict(name="CSolid::E43VfC8", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0), (1, lambda a: 16 * min(a[2], 64))], direct_fuzz=fuzz_quat_rows),
    # E47SolidState::Vf17x (slot 17, 0x529a50; ecx = the state at body+0xe0): the matrix at +0x30 (body+0x110) from the
    # orientation at +0x20 (body+0x100); when the gate at +0x7c is positive, times diag(+0x70, +0x74, +0x78) by E46Mat::Mul.
    # Direct runs only (fuzz_state, with --this = the state); the gate is off on the drive's steps.
    0x529A50: dict(name="E47SolidState::Vf17x", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x80)], fuzz=fuzz_state),
    # E46Mat::Mul (0x528c20; ecx = the matrix (its nine words; the tenth is unused), args (out, b)). The child of Vf17x.
    0x528C20: dict(name="E46Mat::Mul", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 40), (1, lambda a: 36), (2, lambda a: 36)]),
    # E47_Vf54Helper (0x52b6b0; ecx = the body): calls E43_EntityVf54Fwd (0x519ca0, a tail-jump into the body's +0x5c object's
    # slot 1), then tail-jumps to the scratch's slot 1 (the reset, E43Vf04 0x529af0). Direct runs (fuzz_block on the body's
    # scratch block at +0x280 via --this).
    0x52B6B0: dict(name="E47_Vf54Helper", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0)], fuzz=fuzz_body_block),
    # CMultiSignal1<CSigMrRBodyState>::Emit (0x51f700): ecx = the signal, then the payload (0x30 bytes, the stack buffer).
    # CSolid::E43Vf90 (slot 36, 0x52bf30; body_emit_a): no arguments. Its own writes (the dirty words, the armed word) and the
    # event's copies are the port's; the slot-18 call, the host notify and the event Emit are the externals below.
    0x52BF30: dict(name="CSolid::E43Vf90", popped=0, x87=False, thiscall=True, fuzz=fuzz_emit,
                   windows=[(0, lambda a: 0x3D0)]),
    # CSolid::E43Vf94 (slot 37, 0x52c130; the sleep, `layout::solid_sleep`): no arguments, the sleep trip's call. Its in-object
    # writes (the dirty words, the active and armed words, the very-soft word) are the port's, as E43Vf90's; the host notify
    # (CEntity::E43Vf94, 0x519a80) and the SLEEP event's Emit (0x51f7c0, listed) are the externals.
    0x52C130: dict(name="CSolid::E43Vf94", popped=0, x87=False, thiscall=True, fuzz=fuzz_emit,
                   windows=[(0, lambda a: 0x3D0)]),
    # CEntity::E43Vf94 (0x519a80): the host notify, as CEntity::E43Vf90 (WallRel unless the body has a parent).
    0x519A80: dict(name="CEntity::E43Vf94", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CSolid::S18 (slot 18, 0x529dc0): copies the pose into the window start (+0x330, +0x2a0, ...). Its own calls are not listed.
    0x529DC0: dict(name="CSolid::S18", popped=0, x87=False, thiscall=True, external=True, windows=[(0, lambda a: 0x3D0)]),
    # CEntity::E43Vf90 (0x519a60): the host notify, made when the body has no parent (the host checks that).
    0x519A60: dict(name="CEntity::E43Vf90", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # CMultiSignal1<const CSigMrRBodyEvent&>::Emit (0x51f7c0): the event payload (0x50 bytes, the stack buffer).
    0x51F7C0: dict(name="CMultiSignal1::Emit<CSigMrRBodyEvent>", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x50)]),
    0x51F700: dict(name="CMultiSignal1::Emit<CSigMrRBodyState>", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x30)]),
    # E47Solid::Vf39x (0x52ac80; ecx = the body, args (rows, n)): E46Mat::ScaleRows on the matrix (+0x110) and E46Vec::Add
    # on the origin (+0x140), over the same rows. Both children are recorded below (the port's host replays them).
    0x52AC80: dict(name="E47Solid::Vf39x", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x150), (1, lambda a: 16 * a[2])], direct_fuzz=fuzz_vf39),
    # E46Mat::ScaleRows (0x528dd0; ecx = the matrix's ten words, args (dst, n)): the child of Vf39x.
    0x528DD0: dict(name="E46Mat::ScaleRows", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 40), (1, lambda a: 16 * a[2])]),
    # E46Vec::Add (0x528b00; ecx = the origin, args (dst, n)): the child of Vf39x.
    0x528B00: dict(name="E46Vec::Add", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 16), (1, lambda a: 16 * a[2])]),
    # E47_Vf16Helper (0x52b6d0; ecx = the body): Vf17x on the state at +0xe0, twice, then the tail jump to CBody::Vf40
    # (0x4e1120), whose two calls are recorded below. Direct runs (fuzz_vf16 on the state).
    0x52B6D0: dict(name="E47_Vf16Helper", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x3D0)], direct_fuzz=fuzz_vf16),
    # CBody::Vf40 (0x4e1120), the tail of Vf16Helper, and its two calls (as in collision.py): CBody::CState::Unk44 (0x4e33e0)
    # and CEntity::Vf40 (0x51a4b0).
    0x4E1120: dict(name="CBody::Vf40", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xD0)]),
    0x4E33E0: dict(name="CBody::CState::Unk44", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x44), (0, lambda a: 0xD0, 0x30)]),
    0x51A4B0: dict(name="CEntity::Vf40", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x70)]),
    # CSolid::NewDef (0x52b8b0; cdecl, one argument, the name): operator new, then the CDef constructor on the block.
    0x52B8B0: dict(name="CSolid::NewDef", popped=0, x87=False, external=True, windows=[], direct_fuzz=fuzz_newdef),
    # operator new (0x70bec2, cdecl): the caller pops; the size is not recorded (the block is the record's eax).
    0x70BEC2: dict(name="operator new", popped=0, x87=False, external=True, windows=[]),
    # E47SolidState::Ctor (0x52d280, ret 0xc): owner, name, unused. The state at +0 (0x198 bytes), the owner's flags at +0x60.
    0x52D280: dict(name="E47SolidState::ctor", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x198), (1, lambda a: 0x64)], direct_fuzz=fuzz_state_ctor),
    # NTweak base (0x52f090, name), CVector4 (0x52fff0), CReal (0x52fea0), CVector (0x52ff80), CUnsigned (0x52fec0):
    # the state's members, built in the original's order; each window is its own bytes.
    0x52F090: dict(name="NTweak::CBase::CBase (name)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x198)]),
    0x52FFF0: dict(name="NTweak::CVector4::CVector4", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x40)]),
    0x52FEA0: dict(name="NTweak::CReal::CReal", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x52FF80: dict(name="NTweak::CVector::CVector", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x20)]),
    0x52FEC0: dict(name="NTweak::CUnsigned::CUnsigned", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # CSolid::CDef::CDef (0x529840; ecx = the block, the name): its window is the block.
    0x529840: dict(name="CSolid::CDef::CDef", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x1F0)], direct_fuzz=fuzz_cdef),
}
