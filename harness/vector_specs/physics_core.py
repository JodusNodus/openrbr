"""Vector set for the physics core port (crates/rbr-physics/src/physics_core): E46 math leaves (group A of its README).

Run: .venv/bin/python -I harness/vcu_vectors.py SNAPSHOT OUT --set physics_core
Each record is one call of the original with its arguments and windows (see vcu_vectors.py).
"""

SET_NAME = "physics_core"


def idx_branch(args, windows):
    """Branch of E46_IdxWrite (0x528980): 'first' (value <= t[0]), 'down' (value <= t[hint], walks down),
    'last' (value above t[count - 1]), 'up' (walks up from the hint). Comparisons as the C++ writes them."""
    table, value, count = windows[0][3], f32(args[1]), signed(args[2])
    lo = signed(struct.unpack_from("<I", windows[1][3], 0)[0])

    def t(i):
        return f32(struct.unpack_from("<I", table, 4 * i)[0])

    if value <= t(lo):
        return "first" if value <= t(0) else "down"
    if t(count - 1) < value:
        return "last"
    return "up"


def idx_fuzz(mu, args, rng, put):
    """Table and hint for E46_IdxWrite: the value (a stack word, so it can't be changed here) is kept, and the table
    is rewritten around it so the call takes the first, last or walk branch. The hint is a valid index."""
    table, value, count = args[0], f32(args[1]), signed(args[2])
    span = max(abs(value), 1.0)
    mode = rng.choice(("first", "last", "walk"))
    if mode == "first":  # every entry at or above the value: value <= t[0]
        entries = sorted(value + span * rng.random() for _ in range(count))
    elif mode == "last":  # every entry below the value: t[n-1] < value
        entries = sorted(value - span * (0.1 + rng.random()) for _ in range(count))
    else:  # entries around the value: the walk goes down or up from the hint
        entries = sorted(value + span * (2 * rng.random() - 1) for _ in range(count))
    for i, e in enumerate(entries):
        put(table + 4 * i, e, "<f")
    put(args[3], rng.randrange(count), "<I")  # the hint lo


def vf17x_fuzz(mu, args, rng, put):
    """E47SolidState::Vf17x: its gate (+0x7c) is set positive, so it multiplies its matrix by the diagonal (E46Mat::Mul).
    The drive does not take the gate on the recorded steps."""
    put(args[0] + 0x7C, rng.uniform(0.5, 2.0), "<f")
    for off in (0x70, 0x74, 0x78):
        put(args[0] + off, rng.uniform(-2.0, 2.0), "<f")


def app_init_nodes(mu, a):
    """AppInit's globals (the subsystem pointers, the registrar nodes and the callbacks) and the signal manager's two
    registrar lists, with the previous tails the links write through."""
    rd = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    out = [(0x893600, 4), (0x7C63C8, 4), (0x7C7464, 4), (0x7C755C, 4), (0x8EF7E0, 16), (0x165A338, 16),
           (0x165A348, 4), (0x165A3AC, 4), (0x165A39C, 16)]
    mgr = rd(0x893600)
    if mgr:
        out += [(mgr + 0x2E00, 12), (mgr + 0x3660, 12)]
        for tail in (rd(mgr + 0x2E04), rd(mgr + 0x3664)):
            if tail:
                out.append((tail + 4, 4))
    return out


def wall_rel_nodes(mu, a):
    """WallRel's state word (the physics at +0x20), the relay target word (+0x18) and the target list's count (+0x08),
    which the release lowers."""
    relay = a[1]
    target = struct.unpack("<I", bytes(mu.mem_read(relay + 0x18, 4)))[0]
    out = [(a[0] + 0x20, 4), (relay + 0x18, 4)]
    if target:
        out.append((target + 8, 4))
    return out


def shake_nodes(mu, a):
    """E45_P3210: the signal manager's pointer (the incar shake signal is at its +0xc6e8). The Emit's own windows are
    its record's."""
    return [(0x893600, 4)]





def spring_fuzz(mu, args, rng, put):
    """The passenger's spring (M29E0; this = args[0]): the host's words (+0x20..+0x90) over the lanes' ranges, and the
    velocity (this+0x30)."""
    host = word(mu, args[0] + 0x14)
    for off in range(0x20, 0xA0, 4):
        put(host + off, rng.uniform(-2.0, 2.0), "<f")
    for off in (0x30, 0x34, 0x38):
        put(args[0] + off, rng.uniform(-10.0, 10.0), "<f")


def passenger_flag_fuzz(mu, args, rng, put):
    """E45_P33F0 (this = args[0]): the object's E37 flag (+0x67c, the external's answer) is zero or set."""
    put(args[0] + 0x67C, rng.choice((0, 1, rng.randrange(1, 1 << 32))), "<I")


def shake_fuzz(mu, args, rng, put):
    """E45_P3210: the host words the payload copies (+0x70..+0xc0) drawn over a range, so the payload's words differ."""
    host = word(mu, args[0] + 0x14)
    for off in range(0x70, 0xC0, 4):
        put(host + off, rng.uniform(-100.0, 100.0), "<f")


def time_args(args, rng):
    """E45PSmalls::M3360 (this, the double v): v is drawn over several decades, both signs; its two words are the stack
    arguments 1 and 2 (the low word first)."""
    v = rng.uniform(-1.0, 1.0) * 10 ** rng.uniform(-3.0, 7.0)
    lo, hi = struct.unpack("<II", struct.pack("<d", v))
    return {1: lo, 2: hi}


def tag_nodes(mu, a):
    """Tag520940: the flag byte (+0x164) of the entry of the tag, read for a tag in range other than 0 (the only byte
    the port reads of the manager)."""
    tag = a[1]
    return [(a[0] + tag * 0x160 + 0x164, 1)] if 0 < tag < 0x100 else []


def tag_args(args, rng):
    """CMaterialManager::Tag520940(tag): the tag is the stack argument, from 0 to 0x11f (0x100 and up are out of range)."""
    return {1: rng.randrange(0x120)}


def wall_rel_fuzz(mu, args, rng, put):
    """E43HostRel::WallRel: half the calls get no target (the relay's +0x18 is zero), the early return the drive does not
    take (the recorded calls all have a target)."""
    if rng.random() < 0.5:
        put(args[1] + 0x18, 0, "<I")


def physics_gate(mu, a):
    """The CPhysics global, its CDef, and the CDef words the accumulators read: the flags of the debug hook and the
    gravity block (CDef +0x160a0)."""
    g = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    cdef = struct.unpack("<I", bytes(mu.mem_read(g + 0x24, 4)))[0]
    return [(0x1659B70, 4), (g + 0x38, 4), (cdef + 0x160F4, 4), (cdef + 0x160A0, 0x20)]


def vf10_flags(mu, a):
    """CPhysics::Vf10 reads the CDef's debug flags (+0x160e0 of the CDef the CPhysics global points to)."""
    g = struct.unpack("<I", bytes(mu.mem_read(0x1659B70, 4)))[0]
    cdef = struct.unpack("<I", bytes(mu.mem_read(g + 0x24, 4)))[0]
    return [(cdef + 0x160E0, 4)]


def accum250_fuzz(mu, args, rng, put):
    """E46_Accum250: half the calls get a zero inverse mass (the object at +0x34, +0x16c), the case with no walk and no
    accumulate. The drive has a non-zero mass on every recorded call."""
    if rng.random() < 0.5:
        obj = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0x34, 4)))[0]
        put(obj + 0x16C, 0.0, "<f")


def local_time_conns(mu, this):
    """The connections Emit's walk calls, in order: the overflow list, then the count's slot list (see Emit). Nothing
    while the guard is set, the count pointer is null or the count is out of range."""
    count_at = word(mu, this + 4)
    if word(mu, this) or not count_at or word(mu, count_at) >= 0x20:
        return []
    out = []
    for head in (this + 0x208, this + 8 + 16 * word(mu, count_at)):
        c = word(mu, head)
        while c:
            out.append(c)
            c = word(mu, c + 4)
    return out


def local_time_targets(mu, a):
    """Emit's dispatch targets: Invoke (vtable slot 1) of each connection of the walk."""
    return [(vtarget(mu, c, 1), 4) for c in local_time_conns(mu, a[0])]


def local_time_nodes(mu, a):
    """The connections the walk reads (0x18 bytes each: the next link at +4) and their vtables (slot 1 is Invoke)."""
    return [w for c in local_time_conns(mu, a[0]) for w in ((c, 0x18), (word(mu, c), 8))]


def table_nodes(addr, count):
    """A flag class's pointer table: `count` words at `addr` (the names or the details), read by its GetBit accessors."""
    return lambda mu, a: [(addr, 4 * count)]


def mounted_flag_nodes(mu, a):
    """A mounted body's enable flag: the pointer at +0x580 and the word it points at (its state, 0, 1 or 2)."""
    return [(word(mu, a[0] + 0x580), 4)]


SPECS = {
    # cdecl: args (table, value bits, count, out); out holds the hint lo, then hi and frac.
    0x528980: dict(name="E46_IdxWrite", popped=16, x87=False, cdecl=True, branch=idx_branch, fuzz=idx_fuzz,
                   windows=[(0, lambda a: 4 * min(signed(a[2]), 64)), (3, lambda a: 12)]),
    # thiscall: this = E46Vec (4 floats); args (dst, n): n is a count of 4-float vectors.
    0x528B00: dict(name="E46Vec::Add", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 16), (1, lambda a: 16 * min(a[2], 64))]),
    0x528B90: dict(name="E46Vec::Sub", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 16), (1, lambda a: 16 * min(a[2], 64))]),
    # thiscall: this = E46Mat (10 floats); args (out, b): b is three row vectors of 3 floats, out 9 floats.
    # external: recorded as an inner call of E47SolidState::Vf17x (below).
    0x528C20: dict(name="E46Mat::Mul", popped=8, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 40), (1, lambda a: 36), (2, lambda a: 36)]),
    # E47SolidState::Vf17x (0x529a50), the caller of Mul; listed so its gate can be fuzzed (physics-core's Mul has no
    # other caller on the recorded steps). Only its inner Mul call is used by the tests.
    0x529A50: dict(name="E47SolidState::Vf17x", popped=0, x87=False, thiscall=True, branch=no_branch,
                   fuzz=vf17x_fuzz, windows=[]),
    # thiscall: args (dst, n): n 4-float rows, each transformed in place.
    0x528DD0: dict(name="E46Mat::ScaleRows", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 40), (1, lambda a: 16 * min(a[2], 64))]),
    # thiscall: args (base, n): n rows, the xyz of each 4-float row is transformed in place.
    0x528FA0: dict(name="E46Mat::XformRows", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 40), (1, lambda a: 16 * min(a[2], 64))]),
    # E46_Accum250 = CSolid::E43Vf60 (0x529250): listed; its walk (MB560) and E43Vf70 (below) are inner records.
    0x529250: dict(name="CSolid::E43Vf60 (E46_Accum250)", popped=0, x87=False, thiscall=True, branch=no_branch,
                   nodes=physics_gate, fuzz=accum250_fuzz, windows=[(0, lambda a: 0x40), (0, lambda a: 0x170, 0x34)]),
    # E43Vf70 (Accum340): the virtual accumulate, recorded inside Accum250.
    0x529340: dict(name="CSolid::E43Vf70 (E46_Accum340)", popped=4, x87=False, thiscall=True, external=True,
                   branch=no_branch, nodes=physics_gate, windows=[(0, lambda a: 0x380), (1, lambda a: 16)]),
    # E43Vf80 / E43Vf8C: pointer getters.
    0x5293D0: dict(name="CSolid::E43Vf80", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x1E0)]),
    0x5293E0: dict(name="CSolid::E43Vf8C", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x1F0)]),
    # E43PairWalk::MB560 (0x51b560), the walk Accum250 makes on the gravity: inner record, out is window 1 (c).
    0x51B560: dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(1, lambda a: 16), (3, lambda a: 16)]),
    # group C (load-time: no StepForward call). The objects they build are read back through "after" (the object
    # the call returns) or the windows of their own argument. Entry dumps: RBR_SNAPAT, then vcu_vectors on the dump.
    0x5270C0: dict(name="CPhysics::CState::CState", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x4B8)]),
    0x5278D0: dict(name="CPhysics::CDef::Vf50", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[], after=lambda mu, a, eax: [(eax, 0x4B8)]),
    0x526760: dict(name="CPhysics::CDef::Vf54", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], after=lambda mu, a, eax: [(eax, 0xE890)]),
    0x5284F0: dict(name="CPhysics::CDef::Vf4c", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[], after=lambda mu, a, eax: [(eax, 0xF8)]),
    0x524E80: dict(name="CPhysics::CScratch::CScratch", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0xE890)]),
    # CScratch::Vf04: the solver's S08, then the +0xe840 block; the constructor calls it last.
    0x523C50: dict(name="CPhysics::CScratch::Vf04", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0xE890)]),
    # the solver calls of CScratch's constructor (inner records): CCollisionSolver::CtorSolver (E38) and its slot 2
    # (E45 Vf04 calls it on the solver at +0x10). Their windows are the solver's bytes they write.
    0x4FC930: dict(name="CCollisionSolver::CtorSolver", popped=4, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[(0, lambda a: 0xC440)]),
    0x4FC0B0: dict(name="CCollisionSolver::slot2 (E45S08::S08)", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[(0, lambda a: 0xC440)]),
    # CPhysics ctor's calls: the signal links are not calls (the caller's writes to g_signalManager, not recorded).
    0x4F5340: dict(name="E37Prefill5340", popped=0, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=no_branch, windows=[]),
    0x52F0B0: dict(name="NTweak::CBase::GetNameOrNull", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x4A0150: dict(name="E21Stub::Hook", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=no_branch, windows=[]),
    0x527C60: dict(name="CPhysics::CPhysics", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0xF8)]),
    # CDef ctor's calls: the def manager (E39), the material manager (E45), the solver's def (E38), the debug flags (E45).
    0x501D90: dict(name="E39_DefMgrCtor", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x520B30: dict(name="CMaterialManager::CMaterialManager", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x4FB350: dict(name="CCollisionSolver::CDef::CDef", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x523800: dict(name="CPhysics::CDef::CtorDbgFlags", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x524C70: dict(name="CPhysics::CDef::CDef", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x16160)]),
    # Load-time setup (group D): the factory, the global init, the app init and the host's wall release.
    0x526660: dict(name="E46_CreateDef", popped=4, x87=False, thiscall=False, cdecl=True, branch=no_branch,
                   windows=[], nodes=lambda mu, a: [(0x165B200, 4)]),
    0x5266D0: dict(name="E46_InitPhysics", popped=4, x87=False, thiscall=False, cdecl=True, branch=no_branch,
                   windows=[], nodes=lambda mu, a: [(0x1659B6C, 4), (0x1659B70, 4), (0x165B200, 4)]),
    # InitPhysics' externals: the script's comment strip (cdecl) and the token cursor's calls.
    0x52EBA0: dict(name="NTweak::StripComments", popped=4, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=no_branch, windows=[]),
    0x52EC80: dict(name="NTweak::CTokenCursor::Tokenize", popped=8, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x52EE10: dict(name="NTweak::CTokenCursor::FreeTokens", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x526FD0: dict(name="E46_AppInit", popped=0, x87=False, thiscall=False, cdecl=True, branch=no_branch,
                   windows=[], nodes=app_init_nodes),
    # CreateDef's externals: E45_PhysUnk34 (fastcall, this only) and the def's slot 2 (the StepForward it casts to).
    0x523A20: dict(name="E45_PhysUnk34", popped=0, x87=False, thiscall=True, external=True, branch=no_branch, windows=[]),
    0x52F0C0: dict(name="CDef slot 2 (StepForward)", popped=4, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    # AppInit's externals: the wall clear, the collision clear, the water table reset, the vector clear, the token
    # scratch init and the signal init.
    0x4E40D0: dict(name="CBrakeWallManager::ClearWall", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
    0x4FAD10: dict(name="E38_004FAD10", popped=0, x87=False, thiscall=True, external=True, branch=no_branch, windows=[]),
    0x4FFBA0: dict(name="E39WaterTable::Reset", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x52EB20: dict(name="NTweak::InitTokenScratch", popped=0, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=no_branch, windows=[]),
    0x4FA800: dict(name="E37SigInit800", popped=0, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=no_branch, windows=[]),
    # WallRel's externals: the relay's link release (E34Link10::UnlinkA) and the state's RegWall.
    0x4E41A0: dict(name="E34Link10::UnlinkA", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x5263D0: dict(name="CState::RegWall", popped=4, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x5276A0: dict(name="E43HostRel::WallRel", popped=4, x87=False, thiscall=True, branch=no_branch, windows=[], nodes=wall_rel_nodes,
                   fuzz=wall_rel_fuzz),
    0x524F80: dict(name="CPhysics::Vf10", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0xF8)], nodes=vf10_flags),
    # CMultiSignal1<const CSigLocalTime&>::Emit: the guard (+0), the count pointer (+4), 32 slots (+8), overflow (+0x208)
    0x526580: dict(name="CMultiSignal1<CSigLocalTime>::Emit", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x218), (0, lambda a: 4, 4)], nodes=local_time_nodes,
                   dispatch=local_time_targets),
    # The flag classes (src/rbr/RBR_E45_PhysA.cpp, RBR_E45_Material.cpp): GetBitName / GetBitDetail index a table of
    # words, GetBitCount is a constant. CDef::CFlags 0x14 bits, CDbgFlags 0x1b, CLogFlags 0x14, CMaterial::CFlags 4.
    0x523B90: dict(name="CPhysics::CDef::CFlags::GetBitName", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBC10, 0x14)),
    0x523BA0: dict(name="CPhysics::CDef::CFlags::GetBitDetail", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBC60, 0x14)),
    0x523BB0: dict(name="CPhysics::CDef::CFlags::GetBitCount", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    0x523BC0: dict(name="CPhysics::CDef::CDbgFlags::GetBitName", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBCB0, 0x1B)),
    0x523BD0: dict(name="CPhysics::CDef::CDbgFlags::GetBitDetail", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBD20, 0x1B)),
    0x523BE0: dict(name="CPhysics::CDef::CDbgFlags::GetBitCount", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    0x523BF0: dict(name="CPhysics::CDef::CLogFlags::GetBitName", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBD90, 0x14)),
    0x523C00: dict(name="CPhysics::CDef::CLogFlags::GetBitDetail", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CBDE0, 0x14)),
    0x523C10: dict(name="CPhysics::CDef::CLogFlags::GetBitCount", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    0x520840: dict(name="CMaterial::CFlags::GetBitName", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=table_nodes(0x7CAF18, 4)),
    0x520860: dict(name="CMaterial::CFlags::GetBitCount", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    # The accessors that return the address of a global (CDef, CState, CMaterialManager, the mounted body's CDef).
    0x523B10: dict(name="CPhysics::CDef::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x523B20: dict(name="CPhysics::CDef::Unk1C", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x5208D0: dict(name="CMaterialManager::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x521550: dict(name="CMountedRigidBody::CDef::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    # The mounted body's enable flag (+0x580, a pointer to the state word).
    0x521640: dict(name="E45_M1640", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[(0, lambda a: 0x584)]),
    0x521910: dict(name="E45Smalls::M1910", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x584)]),
    0x521920: dict(name="E45_M1920", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x584)], nodes=mounted_flag_nodes),
    0x521940: dict(name="E45_M1940", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x584)], nodes=mounted_flag_nodes),
    # E46FuncConn::NotifyPad / NotifyExitGame (src/rbr/RBR_E46_Conn.cpp): the connection's owner (+0x10) is called
    # cdecl with the signal. The connection's 0x14 bytes are the window.
    0x527B40: dict(name="E46FuncConn::NotifyPad", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x14)]),
    0x527B90: dict(name="E46FuncConn::NotifyExitGame", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x14)]),
    # CMaterial's constructor (src/rbr/RBR_E45_Material.cpp), built 256 times by the material manager's constructor.
    0x520980: dict(name="CMaterial::CMaterial", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x160)]),
    # CMaterialManager::Tag520940 (src/rbr/RBR_E45_Material.cpp): the material entry for a tag (this+1 of the manager).
    0x520940: dict(name="CMaterialManager::Tag520940", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[], nodes=tag_nodes, fuzz_args=tag_args),
    # The mounted body's constructors (src/rbr/RBR_E45_MountedA.cpp). CtorA builds a CDef: the solid def's constructor
    # (E47 0x529840, external), then CUnsigned (+0x1f0), CVector (+0x200) and CUnsigned (+0x230) through the NTweak
    # constructors (externals). CState::Ctor builds three CReal at +4, +0x14, +0x24. NewDef allocates 0x240 bytes and
    # calls CtorA; its block is read on return.
    0x529840: dict(name="CSolid::CDef::CDef", popped=4, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 0x240)]),
    0x52FEC0: dict(name="NTweak::CUnsigned::CUnsigned", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[(0, lambda a: 0x10)]),
    0x52FF80: dict(name="NTweak::CVector::CVector", popped=0, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[(0, lambda a: 0x20)]),
    0x52FEA0: dict(name="NTweak::CReal::CReal", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 0x10)]),
    0x521480: dict(name="CMountedRigidBody::CDef::CtorA", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x240)]),
    0x5215A0: dict(name="CMountedRigidBody::CState::Ctor", popped=12, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x34)]),
    0x521B50: dict(name="CMountedRigidBody::NewDef", popped=4, x87=False, thiscall=False, cdecl=True,
                   branch=no_branch, windows=[], after=lambda mu, a, eax: [(eax, 0x240)] if eax else []),
    0x521500: dict(name="CMountedRigidBody::CDef::Vf4c", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[]),
    # CtorBig (0x521c30) builds the body on the solid base (E47 0x52d480, external), the name (GetNameOrNull, external),
    # CState::Ctor (0x5215a0), the scratch base (E47_ScratchBaseInit 0x52b930, external) and E43_ExclA000(2) (shared
    # spec). Vf50 (0x521d20) allocates 0x590 bytes and calls CtorBig(this, a, b, 0); its block is read on return.
    0x52D480: dict(name="CSolid::CSolid", popped=0x10, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x52B930: dict(name="E47_ScratchBaseInit", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    # E43_ExclA000 (AllocExclusion, one allocation inside). The owner window (argument c) is read for its flags (+0x60).
    0x51A000: dict(name="CEntity::E43_ExclA000", popped=4, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    0x521C30: dict(name="CMountedRigidBody::CtorBig", popped=0x10, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x590), (3, lambda a: 0x64)]),
    0x521D20: dict(name="CMountedRigidBody::CDef::Vf50", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(2, lambda a: 0x64)], after=lambda mu, a, eax: [(eax, 0x590)] if eax else []),
    # The pose update (E45_M1980): the base's SetDirty, CCar::BodyCompose on the owner (its own calls are not the body's),
    # and E01Vec4::CopyFrom twice. The state word (through the body's +0x580 pointer) is read first.
    0x52F450: dict(name="NTweak::CBase::SetDirty", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x4E6A30: dict(name="CCar::BodyCompose", popped=8, x87=False, thiscall=True, external=True, windows=[]),
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x521980: dict(name="CMountedRigidBody::E45_M1980", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x590), (0, lambda a: 4, 0x580)]),
    # Passenger constructors (RBR_E45_PassengerA/B). CBase(name) (0x52f090) and the scratch's Vf04 (vtable slot 1, 0x73e888
    # = E45_P24E0, a no-op) are externals. CDef::CtorB is pure stores (no calls).
    0x52F090: dict(name="NTweak::CBase::CBase (name)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x5224E0: dict(name="CPassenger::CScratch::Vf04 (E45_P24E0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[]),
    0x522450: dict(name="CPassenger::CDef::CtorA", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x40)]),
    0x5224A0: dict(name="CPassenger::CDef::CtorB", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x40)]),
    0x522550: dict(name="CPassenger::CState::CtorB", popped=0xC, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0xC0)]),
    # CPassenger's flag test, shake, time and spring (src/rbr/RBR_E45_PassengerB.cpp). P33F0 (cdecl, one stack argument)
    # tests the object's E37 flag (external 0x4f7e40); the incar shake Emit (0x523010) is the signal walk; P3210 clears
    # its payload (Clear6, external), fills it and emits it; M3360 decodes a time (CRT _ftol2, external); M29E0 is the
    # spring. The payload is the Emit's window 2 (0x64 bytes).
    0x5233F0: dict(name="E45_P33F0", popped=4, x87=False, thiscall=False, cdecl=True, branch=no_branch, windows=[],
                   fuzz=passenger_flag_fuzz),
    0x4F7E40: dict(name="E37Get67C", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x523010: dict(name="CIncarShake::Emit", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x218), (0, lambda a: 4, 4), (1, lambda a: 0x64)], nodes=local_time_nodes,
                   dispatch=local_time_targets),
    0x523210: dict(name="E45_P3210", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x18), (0, lambda a: 0xC0, 0x14)], nodes=shake_nodes, fuzz=shake_fuzz),
    0x49FEA0: dict(name="E20Clear6::Clear6", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x60)]),
    0x523360: dict(name="E45PSmalls::M3360", popped=8, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x14)], fuzz_args=time_args),
    0x70C2C4: dict(name="CRT _ftol2", popped=0, x87=False, thiscall=False, external=True, windows=[]),
    # CPassenger::Step (0x523160) is the entry of the spring's dump (its records run the spring): its calls are not ported.
    0x523160: dict(name="CPassenger::Step", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x44)]),
    0x5229E0: dict(name="E45PSmalls::M29E0", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x40), (0, lambda a: 0xA0, 0x14)], fuzz=spring_fuzz),
    0x5230D0: dict(name="CPassenger::Ctor", popped=0xC, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x44)]),
}

# Group E (detectors and kernels, RBR_E45_PhysB.cpp, RBR_E46_Kernels.cpp) and F (contact step, RBR_E46_Physics.cpp,
# RBR_E46_Small.cpp, RBR_E46_Detectors.cpp). Only the functions the snapshots reach are specified; see the README.
CELL_GRID_GLOBAL = 0x8EB794


def cell_nodes(mu, a):
    """E45_M40C0: the physics global (g_1659b70), its CDef (+0x24), the CDef's flags word (+0x160cc) and the cell grid
    (g_8eb794), which the lookups are made on."""
    physics = word(mu, 0x1659B70)
    cdef = word(mu, physics + 0x24)
    return [(0x1659B70, 4), (physics + 0x24, 4), (cdef + 0x160CC, 4), (CELL_GRID_GLOBAL, 4)]


def cell_branch(args, windows):
    """E45_M40C0's branch on the CDef flag 0x2000 (window 2 is the flags word, after the three argument windows)."""
    flags = struct.unpack("<I", windows[5][3])[0]
    return "widened" if flags & 0x2000 else "plain"


def edge_list_nodes(mu, a):
    """E45_M46A0: the list's count and its ids (the pair objects are the callee's own; the port reads only these)."""
    c = a[3]
    return [(c, 4 + 4 * min(word(mu, c), 64))] if c else []


def edge_list_branch(args, windows):
    """E45_M46A0's gate: 'null' (no list), 'empty' (count 0, the result is cleared only) or 'swept'."""
    if not args[3]:
        return "null"
    return "swept" if struct.unpack("<I", windows[3][3][:4])[0] else "empty"


def cell_fuzz(mu, args, rng, put):
    """E45_M40C0: the CDef's flags word (+0x160cc) with the widening bit 0x2000 set or clear, so both branches are
    recorded (the drive takes the widened one on every recorded call)."""
    cdef = word(mu, word(mu, 0x1659B70) + 0x24)
    flags = word(mu, cdef + 0x160CC)
    put(cdef + 0x160CC, (flags | 0x2000) if rng.random() < 0.5 else (flags & ~0x2000) & 0xFFFFFFFF, "<I")


def edge_list_fuzz(mu, args, rng, put):
    """E45_M46A0: the list's count is 0..4 (the drive's lists are all empty). The ids stay as the drive has them (zero):
    a nonzero id is returned to the wheel code, which uses it as a pointer, and the fuzzed step then faults."""
    if args[3]:
        put(args[3], rng.randrange(5), "<I")


def edge_list_args(args, rng):
    """E45_M46A0: the list argument (c, args[3]) is null on a quarter of the calls: the early return."""
    return {3: 0} if rng.random() < 0.25 else None


def edge_edge_result(mu, args, rng, write):
    """E37GridOps::EdgeEdge (this, a, b, the hit point, the distance, the normal, the hit word) on the fuzzed sweeps: a
    hit (eax 1) on half the calls, with the outputs drawn over ranges that include the sweep's 1.01 start. The call is
    not run (its pair object is the list's, which the fuzzed count can point at garbage)."""
    point, dist, normal, word_out = args[3], args[4], args[5], args[6]
    for k in range(4):
        write(point + 4 * k, rng.uniform(-100.0, 100.0), "<f")
        write(normal + 4 * k, rng.uniform(-1.0, 1.0), "<f")
    write(dist, rng.uniform(0.0, 2.0), "<f")
    write(word_out, rng.randrange(1 << 32), "<I")
    return 1 if rng.random() < 0.5 else 0


SPECS.update({
    # cdecl, three stack args: the midpoint and the corner cells of a box, written to out (four words).
    0x5240C0: dict(name="E45_M40C0", popped=0, cdecl=True, stack_words=3, x87=False, branch=cell_branch,
                   windows=[(0, lambda a: 8), (1, lambda a: 8), (2, lambda a: 16)], nodes=cell_nodes,
                   fuzz=cell_fuzz),
    # thiscall (ecx = self, unused by the port), four stack args: the edge-pair sweep over the list (a, b, list, out).
    0x5246A0: dict(name="E45_M46A0", popped=0x10, x87=False, thiscall=True, branch=edge_list_branch,
                   windows=[(1, lambda a: 16), (2, lambda a: 16), (4, lambda a: 0x3C)], nodes=edge_list_nodes,
                   fuzz=edge_list_fuzz, fuzz_args=edge_list_args),
    # externals of M40C0: CCellGrid::LookupTwin (thiscall on the grid, two float stack args).
    0x4E03B0: dict(name="CCellGrid::LookupTwin", popped=8, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[]),
    # externals of M46A0: E37GridOps::EdgeEdge (thiscall, six stack pointers: a, b, the hit point, the distance, the
    # normal, the hit word); its outputs are windows 3..6 of the inner record.
    0x4F8280: dict(name="E37GridOps::EdgeEdge", popped=0x18, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(3, lambda a: 16), (4, lambda a: 4), (5, lambda a: 16), (6, lambda a: 4)],
                   result=edge_edge_result),
})


# Direct calls (harness/direct_calls.py): functions the recorded steps do not reach. The recipe builds the objects and
# inputs in the replay's heap (the object's garbage bytes first, so the port must write every word it owns).
def ctor_direct(mu, heap, r):
    """CL2PenetrationDetector::Ctor (this, a owner, b and c vectors, d a float, e an int): random vectors, the object
    filled with garbage."""
    this, b, c = heap + 0x4000, heap + 0x4800, heap + 0x4810
    writes = [(this, bytes(r.getrandbits(8) for _ in range(0x700))),
              (b, struct.pack("<4f", *[r.uniform(-500, 500) for _ in range(4)])),
              (c, struct.pack("<4f", *[r.uniform(-500, 500) for _ in range(4)]))]
    d = r.uniform(-10.0, 10.0)
    return [heap + 0x4900, b, c, struct.unpack("<I", struct.pack("<f", d))[0], r.getrandbits(32)], writes, this


# The CBrakeWallManager at 0x8eb7b0 (the QueryWall object): its +4 is the wall object pointer.
WALL_OBJECT_PTR = 0x8EB7B4


def grid_build_direct(mu, heap, r):
    """E45_MD60 (no arguments): the grid global is clear (the build) or already set (the early return), on half the
    calls each. On a build, the wall object's pointer (+4 of the manager at 0x8eb7b0) is null a third of the time: then
    QueryWall answers 0 and the grid takes the default bounds."""
    grid = 0 if r.random() < 0.5 else 0x0C0FFEE0
    writes = [(CELL_GRID_GLOBAL, struct.pack("<I", grid))]
    if grid == 0 and r.random() < 1 / 3:
        writes.append((WALL_OBJECT_PTR, struct.pack("<I", 0)))
    return [], writes, None


def grid_nodes(mu, a):
    """E45_MD60: the cell grid global, the wall manager global (the QueryWall object) and the wall object's pointer."""
    return [(CELL_GRID_GLOBAL, 4), (0x7C63C8, 4), (WALL_OBJECT_PTR, 4)]


def grid_branch(args, windows):
    """E45_MD60's gate: 'present' (the grid global is set, the early return) or 'built'."""
    return "present" if struct.unpack("<I", windows[0][3])[0] else "built"


SPECS.update({
    # thiscall, five stack args (a owner, b and c vectors, d float, e int); popped 0x14. Its record's window 0 is the
    # object, and the sub-object at +0x30 is the external E38RaySeeds' (its own window below).
    0x5235B0: dict(name="CL2PenetrationDetector::Ctor", popped=0x14, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x700), (2, lambda a: 16), (3, lambda a: 16)], direct_fuzz=ctor_direct),
    # E38RaySeeds (0x4f6550, thiscall, no stack args): clears the sub-object at +0x30 (0x6b0 bytes).
    0x4F6550: dict(name="E38RaySeeds", popped=0, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 0x6B0)]),
    # cdecl, no args: builds the cell grid from the wall manager's bounds (E45_MD60, 0x523d60).
    0x523D60: dict(name="E45_MD60 (grid build)", popped=0, cdecl=True, stack_words=0, x87=False, branch=grid_branch,
                   windows=[], nodes=grid_nodes, direct_fuzz=grid_build_direct),
    # CBrakeWallManager::QueryWall (thiscall on the wall manager, two vec2 out pointers).
    0x4E4140: dict(name="CBrakeWallManager::QueryWall", popped=8, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[(1, lambda a: 8), (2, lambda a: 8)]),
    # CCellGrid::CCellGrid (thiscall on the new object, four float stack args: min x, min y, width, height).
    0x4E0550: dict(name="CCellGrid::CCellGrid", popped=0x10, x87=False, thiscall=True, external=True, branch=no_branch,
                   windows=[(0, lambda a: 0x14)]),
})


def work5500_chain(mu, this):
    """Work5500's contact nodes: the owner's list (the owner at this+0x20, the head at its +0x450, the links at +0x10)."""
    out = []
    c = word(mu, word(mu, this + 0x20) + 0x450)
    while c and len(out) < 64:
        out.append(c)
        c = word(mu, c + 0x10)
    return out


def work5500_nodes(mu, a):
    """Work5500: the owner's list head word (its 0x454 bytes) and each node's link (0x14 bytes each)."""
    return [(word(mu, a[0] + 0x20), 0x454)] + [(c, 0x14) for c in work5500_chain(mu, a[0])]


def work5500_targets(mu, a):
    """Work5500's node calls at entry, in the asm's order: pass 1 slot 0x5c (slot 23), pass 2 slot 0x64 (slot 25), and
    pass 3 for each node slot 0x6c (slot 27) then slot 0x58 (slot 22), both with the stack word (popped 4)."""
    nodes = work5500_chain(mu, a[0])
    third = [t for c in nodes for t in ((vtarget(mu, c, 27), 4), (vtarget(mu, c, 22), 4))]
    return [(vtarget(mu, c, 23), 0) for c in nodes] + [(vtarget(mu, c, 25), 0) for c in nodes] + third


SPECS.update({
    # thiscall, one stack word (p): the three passes over the owner's contact list (see work5500_targets).
    0x525500: dict(name="Work5500", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x24)], nodes=work5500_nodes, dispatch=work5500_targets),
})
