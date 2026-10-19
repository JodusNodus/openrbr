"""Tyre vector set: the CScratch step and its leaves (exe 0x528980 and 0x531ba0-0x5360c0, the tyre tail getters and
the NTweak name getter), recorded by harness/vcu_vectors.py with `--set tyre`.

Each leaf has a spec (windows, popped bytes, ABI), and `FUZZ` gives `--tyre-fuzz` its scratch and the fuzzed inputs of
each tyre leaf the drive does not reach. `vcu_vectors.py` loads this file with its helpers in scope.
"""

import struct

SET_NAME = "tyre"

NAN = float("nan")


# Tyre leaves (src/rbr/RBR_E49_TyreSim.cpp, RBR_E49_TyreStep.cpp, RBR_E48_TyreMenu.cpp). The popped bytes are the
# callee's `ret` immediates. Windows: this (CScratch, 0x600 bytes), the state at this+0x14 (CState) and the tyre
# parameters at this+0x10 (CDef); each is followed by its pointer chain as the VCU sets do.
TYRE_SOLVE_WINDOWS = [(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (1, lambda a: 0x1C),
                     (2, lambda a: 0xC), (3, lambda a: 0x20)]
# Friction (0x531c70) and the four helpers it calls: E01Vec4 CopyFrom, CFloat3 Length and Scale, E16 Sign.
TYRE_SPECS_FRICTION = {
    0x531C70: dict(name="CScratch::Friction", popped=4, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14)]),
    0x473B30: dict(name="E01Vec4::Length", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x473CD0: dict(name="E01Vec4::Scale", popped=8, x87=False, thiscall=True, external=True, windows=[]),
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x48A440: dict(name="E16_Sign", popped=0, x87=False, external=True, windows=[]),  # cdecl: returns eax
}
# HeatStep (src/rbr/RBR_E49_TyreSim.cpp) and the two spring-damper helpers it calls: DamperStep1 (cdecl, st0) and
# DamperAccumA (cdecl). The wear array is the pointer at this+0x414; one word per tyre index.
TYRE_SPECS_HEAT = {
    # The heat lerps read the CDef tables up to +0x244, so the def window is 0x400 (as Step's).
    0x534090: dict(name="CScratch::HeatStep", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (0, lambda a: 0x40, 0x414)]),  # the wear: the tyre index reaches 5 in the drive
    0x52E350: dict(name="NSpringDamper::DamperStep1", popped=0, x87=True, external=True, windows=[]),
    0x52E3A0: dict(name="NSpringDamper::DamperAccumA", popped=0, x87=False, external=True, windows=[]),
}
# TableStep (idx, x) and RelaxStep (dt), src/rbr/RBR_E49_TyreSim.cpp, and the spring-damper helpers they call that
# the HeatStep specs do not list: DamperStep2 (cdecl, st0) and DamperForce (cdecl). The state's table and damper
# blocks are windows as for HeatStep.
TYRE_SPECS_RELAX = {
    # BlendTable (thiscall, six stack words: the two indices, the two fractions, the two out pointers; ret 0x18, eax 1).
    0x534E70: dict(name="CScratch::BlendTable", popped=0x18, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (0, lambda a: 0x40, 0x414), (5, lambda a: 4), (6, lambda a: 4)]),
    # RelaxCount (thiscall, the step's dt as its stack word): RelaxStep, then the counter at state+0x1c and its dirty flag.
    0x5358C0: dict(name="CScratch::RelaxCount", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x200, 0x10)]),
    0x534480: dict(name="CScratch::TableStep", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x200, 0x10)]),
    0x5345A0: dict(name="CScratch::RelaxStep", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x200, 0x10)]),
    0x52E370: dict(name="NSpringDamper::DamperStep2", popped=0, x87=True, external=True, windows=[]),
    0x52E8A0: dict(name="NSpringDamper::DamperForce", popped=0, x87=False, external=True, windows=[]),
}
# CScratch::Step (thiscall, 16 stack words: the spin, three floats, then the pointers). The stack words are args[1] to
# args[16]: args[5] the CTyreMatData source, args[6] the pair, args[7] and args[9] the Align vectors, args[10] the outA
# vector, args[11] the copied vector, args[12] and args[13] the cleared vectors, args[14] and args[15] the float outputs,
# args[16] the WriteOutputs struct. The CDef tables are read up to +0x244, so its window is 0x400 (as ComputeOutputs).
# The game flag (g_1659b70 -> +0x24 -> +0x160e0) is a node window: its words are the chain the asm reads.
def step_flag_windows(mu, a):
    g = word(mu, 0x1659B70)
    if not g:
        return [(0x1659B70, 4)]
    p = word(mu, g + 0x24)
    return [(0x1659B70, 4), (g + 0x24, 4)] + ([(p + 0x160E0, 4)] if p else [])


TYRE_SPECS_STEP = {
    0x5360C0: dict(name="CScratch::Step", popped=0x40, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (0, lambda a: 0x40, 0x414), (0, lambda a: 0x5B0, 0x410),
                            (5, lambda a: 0x100), (6, lambda a: 12), (7, lambda a: 16), (9, lambda a: 16),
                            (10, lambda a: 16), (11, lambda a: 16), (12, lambda a: 16), (13, lambda a: 16),
                            (14, lambda a: 4), (15, lambda a: 4), (16, lambda a: 0xB0)],
                   nodes=step_flag_windows),
}

TYRE_SPECS = {
    0x531BA0: dict(name="CScratch::Sign", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0x600)]),
    0x5322E0: dict(name="TyreClamp", popped=0x10, x87=False, windows=[(3, lambda a: 4)]),
    0x532A30: dict(name="CScratch::ComputeSlip", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (1, lambda a: 12)]),
    0x533640: dict(name="CScratch::SolveDirs", popped=0x18, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (4, lambda a: 4), (5, lambda a: 4),
                            (6, lambda a: 4)]),
    0x5338A0: dict(name="CScratch::Accumulate", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10)]),
    0x535040: dict(name="CTyreMatData::CopyFrom", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x100), (1, lambda a: 0x100)]),  # the copy reaches word 0x3e (+0xf8)
    # TyreFrictionCurve (13 stack words: args 0-8 floats, 9 and 10 out pointers, 11 the table, 12 its count) and
    # FloorS32 (x, table, count): the table holds count+2 words (FloorS32 reads a[k+1] for k <= count).
    0x5334D0: dict(name="TyreFrictionCurve", popped=0x34, x87=True,
                   windows=[(9, lambda a: 4), (10, lambda a: 4), (11, lambda a: 4 * (min(a[12], 64) + 2))]),
    0x5325F0: dict(name="FloorS32", popped=0xC, x87=True, windows=[(1, lambda a: 4 * (min(a[2], 64) + 2))]),
    0x43C8E0: dict(name="log (FloorS32's index error)", popped=0, x87=False, external=True, windows=[]),  # cdecl
    # ComputeOutputs (thiscall, 17 stack words: 7 and 8 are the raw copy pointers, 9 to 16 the float outputs). The
    # src pointer (stack word 6) holds the two copied dwords at +0x36c and +0x370; the CDef tables run to +0x2a4+4*idx.
    # StepInner (thiscall, 6 stack words: args[1] the record, args[2] a 3-float output, args[3] the vector it zeroes
    # (E09Vec4::Clear, 0x465d80), args[4] to args[6] float outputs). The record's words read here are at +0x340..+0x390.
    0x535310: dict(name="CScratch::StepInner", popped=0x18, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (1, lambda a: 0x390), (2, lambda a: 12), (3, lambda a: 16), (4, lambda a: 4),
                            (5, lambda a: 4), (6, lambda a: 4)]),
    # ContactPatch (thiscall, one stack word: the out float). The record at this+0x410 and the table at this+0x414
    # are read here; SolveDispatch and the SetDirty stores fall in the this window.
    0x5358F0: dict(name="CScratch::ContactPatch", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (0, lambda a: 0x5B0, 0x410), (0, lambda a: 32, 0x414), (1, lambda a: 4)]),
    # SolvePatch (thiscall, no stack words): the results at this+0x3b8 to +0x4c4. Its StepInner call takes the
    # record at this+0x20 and is recorded on its own.
    0x535D60: dict(name="CScratch::SolvePatch", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10)]),
    # Locate (cdecl: the table, x, the count n and the out struct, 0x528980): a hinted search of a sorted table.
    # cdecl: the caller pops the four argument words, which popped gives the recorder.
    # SplitSlip (thiscall, no stack words): the slip speed at this+0x360 splits into the weights at +0x3dc and +0x3f0.
    0x5346F0: dict(name="CScratch::SplitSlip", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x600)]),
    # FillOutputs (thiscall, the src vector pointer, the pair pointer and the divisor as stack words): the output block.
    0x532F10: dict(name="CScratch::FillOutputs", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x100, 0x10),
                            (1, lambda a: 16), (2, lambda a: 12)]),
    # WriteOutputs (thiscall, the output struct pointer as its stack word): the output block from the contact vectors.
    0x533080: dict(name="CScratch::WriteOutputs", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x100, 0x10),
                            (1, lambda a: 0xB0)]),
    # SolveForces (thiscall, the friction's k and two output vectors as stack words): the contact forces.
    0x532B60: dict(name="CScratch::SolveForces", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (2, lambda a: 16), (3, lambda a: 16)]),
    # SolvePair (thiscall): the grip clamp of the pair when this+0x3f0 > 0.
    0x5352A0: dict(name="CScratch::SolvePair", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14)]),
    # ClampGrip (thiscall, seven stack words: five floats, then the out pointers at this+0x3fc and +0x400).
    0x533760: dict(name="CScratch::ClampGrip", popped=0x1C, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14)]),
    # Align (thiscall, two vector pointers): the cross of the two, normalised when long enough; returns eax.
    0x531FC0: dict(name="CScratch::Align", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (1, lambda a: 16), (2, lambda a: 16)]),
    0x528980: dict(name="Locate", popped=0x10, x87=False, cdecl=True, windows=[(0, lambda a: 4 * min(a[2], 64)), (3, lambda a: 12)],
                   nodes=lambda mu, a: [(a[0] - 4, 4)]),  # the word below the table: a NaN input walks to t(-1)
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x5320C0: dict(name="CScratch::ComputeOutputs", popped=0x44, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10),
                            (7, lambda a: 0x374)] + [(k, lambda a: 4) for k in range(8, 18)]),
    0x532470: dict(name="CScratch::SetStateField", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14)]),
    0x532490: dict(name="CScratch::Set414", popped=4, x87=False, thiscall=True, windows=[(0, lambda a: 0x600)]),
    0x533490: dict(name="CScratch::SetState48", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10)]),
    # The CTyre tail getters (`mov eax, [ecx+0x10]` and `[ecx+0x14]`) and NTweak::CBase::GetName (`[ecx+4]`).
    0x5326B0: dict(name="TyreTail_Get10", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x20)]),
    0x5326C0: dict(name="TyreTail_Get14", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x20)]),
    0x52DB10: dict(name="NTweak::CBase::GetName", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x8)]),
    0x534000: dict(name="CScratch::ClearBias", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: 0x400, 0x10)]),
    # the contact solvers (src/rbr/RBR_E49_CTyre.cpp). args: this, p (7 words), qb (3 words), r (7 words), idx.
    # SolveDispatch: this, p, qb, r, a4, a5, a6. Their recursion is recorded as its own records.
    0x533AE0: dict(name="CScratch::SolveA", popped=0x10, x87=False, thiscall=True,
                   windows=TYRE_SOLVE_WINDOWS),
    0x533BF0: dict(name="CScratch::SolveB", popped=0x10, x87=False, thiscall=True,
                   windows=TYRE_SOLVE_WINDOWS),
    0x533D40: dict(name="CScratch::SolveC", popped=0x10, x87=False, thiscall=True,
                   windows=TYRE_SOLVE_WINDOWS),
    0x533E80: dict(name="CScratch::SolveD", popped=0x10, x87=False, thiscall=True,
                   windows=TYRE_SOLVE_WINDOWS),
    0x5351A0: dict(name="CScratch::SolveDispatch", popped=0x18, x87=False, thiscall=True,
                   windows=TYRE_SOLVE_WINDOWS),
    **TYRE_SPECS_FRICTION,
    **TYRE_SPECS_HEAT,
    **TYRE_SPECS_RELAX,
    **TYRE_SPECS_STEP,
}


# Tyre leaves called directly with fuzzed inputs (--tyre-fuzz): the branches the drive does not take. The
# scratch is the step's CScratch (the first tyre call's this); each call starts at the function's entry with
# the inputs written first, so its windows are taken from them.
TYRE_FUZZ = (0x5360C0, 0x531BA0, 0x5322E0, 0x532A30, 0x533640, 0x5338A0, 0x535040, 0x532490,
             0x533AE0, 0x533BF0, 0x533D40, 0x533E80, 0x5351A0, 0x531C70, 0x534090, 0x534480, 0x5345A0,
             0x5334D0, 0x5325F0, 0x5320C0, 0x535310, 0x535D60, 0x5358F0, 0x528980, 0x5346F0, 0x531FC0, 0x5352A0, 0x533760, 0x532B60, 0x532F10, 0x533080,
             0x532470, 0x533490, 0x534000, 0x534E70, 0x5358C0, 0x5326B0, 0x5326C0, 0x52DB10)
TYRE_FUZZ_SCRATCH = (0x5360C0, 0x531BA0, 0x532A30, 0x533640, 0x5338A0, 0x532490, 0x533AE0, 0x533BF0, 0x533D40, 0x533E80,
                     0x5351A0, 0x531C70, 0x534090, 0x534480, 0x5345A0, 0x5320C0, 0x535310, 0x535D60, 0x5358F0,
                     0x5346F0, 0x531FC0, 0x5352A0, 0x533760, 0x532B60, 0x532F10, 0x533080,
                     0x532470, 0x533490, 0x534000, 0x534E70, 0x5358C0)  # thiscall on the scratch
TYRE_FUZZ_POOL = (0.0, 0.0005, 0.001, 0.0015, -0.0005, -0.001, -1.0, 1.0, 0.5, 0.2, 0.9)


def tyre_pool(r, scale):
    """A random float for a fuzzed tyre input: a pool value (the branch edges), or a random one of `scale`."""
    return r.choice(TYRE_FUZZ_POOL + (NAN, r.uniform(-scale, scale), r.uniform(-scale, scale)))


def fuzz_grip(put, state, r):
    """The state's grip fields: the k40, k44 and k4c that ClampGrip divides by, and the flag at +0x5c."""
    put(state + 0x40, tyre_pool(r, 2.0))
    put(state + 0x44, tyre_pool(r, 2.0))
    put(state + 0x4C, r.choice((0.31, 0.3, 0.01, 1.0, r.uniform(0.2, 0.4))))
    put(state + 0x5C, r.choice((0, 1)), "<I")


def fuzz_tyre(mu, addr, this, r):
    """The fuzzed inputs of one tyre call: (stack words after this, the writes (address, bytes))."""
    word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]
    state, pdef = word(this + 0x14), word(this + 0x10)
    writes = []

    def put(at, value, code="<f"):
        writes.append((at, struct.pack(code, value)))

    def common():
        put(state + 0x4C, r.choice((0.31, 0.3, 0.315, 0.01, 1.0, r.uniform(0.2, 0.4))))  # R
        put(state + 0x58, r.choice((0.0, 0.5, 1.0, r.random())))  # lerp weight
        put(pdef + 0xCC, r.choice((0.0, 0.1, 1.0, 0.155, r.uniform(0.0, 0.3))))  # setup width (+0xcc)

    if addr == 0x5360C0:  # Step: the spin and the three floats as stack words, the pointers into a scratch block below
        # the stack (the windows are taken from it), the ComputeSlip matrix and the ContactPatch inputs, the tables
        # the Locate calls search, and the game flag (bit 0x20 of the word at g_1659b70 -> +0x24 -> +0x160e0).
        bits = lambda v: struct.unpack("<I", struct.pack("<f", v))[0]
        S = mu.reg_read(UC_X86_REG_ESP) - 0x800
        for k in range(0x40):
            put(S + 4 * k, tyre_pool(r, 2.0))  # e: the CTyreMatData words (0x100 bytes)
        speed_scale = r.choice((1.0, 15.0, 40.0))  # the outA vector's size: the slip speed reaches the weight's knee
        for k in range(3):
            put(S + 0x100 + 4 * k, tyre_pool(r, 2.0))  # f: the pair
            put(S + 0x120 + 4 * k, r.uniform(-1.0, 1.0))  # i: the align vector
            put(S + 0x130 + 4 * k, r.uniform(-speed_scale, speed_scale))  # j: the outA vector
        for k in range(4):
            put(S + 0x110 + 4 * k, r.uniform(-1.0, 1.0))  # g: the align vector
            put(S + 0x140 + 4 * k, tyre_pool(r, 1.0))  # k: the copied vector
            put(S + 0x150 + 4 * k, tyre_pool(r, 1.0))  # l
            put(S + 0x160 + 4 * k, tyre_pool(r, 1.0))  # m
        if r.random() < 0.1:  # equal align vectors: the cross product is zero, the no-contact path of Align
            for k in range(3):
                put(S + 0x120 + 4 * k, 0.5 * k)
                put(S + 0x110 + 4 * k, 0.5 * k)
        for k in range(0x2C // 4):
            put(S + 0x180 + 4 * k, tyre_pool(r, 2.0))  # p: the WriteOutputs struct
        for off in (0x170, 0x174):
            put(S + off, 0, "<I")
        for off in range(0x170, 0x1A0, 4):
            put(this + off, tyre_pool(r, 2.0))
        for k in range(3):
            put(this + 0x5E0 + 4 * k, tyre_pool(r, 2.0))
        put(this + 0x288, r.choice((0.0, 0.5, 1.0, tyre_pool(r, 2.0))))  # F[0xA2]: the wear gate
        put(this + 0x378, r.choice((0.0, 1.0, -1.0, r.uniform(-30, 30), r.uniform(-1, 1))))  # the spin
        for off in (0x30C, 0x298, 0x314, 0x3DC, 0x360):
            put(this + off, tyre_pool(r, 4.0))
        put(this + 0x3AC, r.choice((0.0, 0.0005, -0.2, 0.7, r.uniform(-1.0, 1.0))))
        common()
        record, table = word(this + 0x410), word(this + 0x414)
        put(record + 0x5AC, r.uniform(-20.0, 20.0))  # the sector's angle (ContactPatch)
        for k in range(8):
            put(table + 4 * k, r.uniform(0.0, 1.0))  # the wear table
        put(this + 0x228, r.choice((0.0, 1e-11, 0.5, 1.0, r.uniform(0.1, 3.0))))  # G
        put(this + 0x258, r.choice((0.0, 0.1, r.uniform(-1.0, 3.0))))  # H
        for off in (0x3A8, 0x1E8, 0x208, 0x218, 0x1F8):
            put(this + off, r.uniform(-1.0, 1.0))
        put(state + 0x40, r.uniform(0.1, 2.0))
        put(state + 0x50, r.uniform(-1.0, 1.0))
        put(state + 0x54, r.uniform(-1.0, 1.0))
        put(state + 0x68, r.uniform(0.0, 1.0))  # the lerp input of the first table
        for off in (0x2C, 0x7C, 0x8C, 0x9C, 0xBC, 0xCC):
            put(pdef + off, r.uniform(-1.0, 2.0))
        put(pdef + 0xEC, r.uniform(0.1, 2.0))
        for off in (0x70, 0x74):
            put(state + off, r.choice((0, 1)), "<I")
        for base, n in ((0x134, 4), (0x154, 6), (0x1E4, 8), (0x184, 8)):  # the sorted tables Locate searches
            values = sorted(r.uniform(0.0, 1.0) for _ in range(n))
            for k, v in enumerate(values):
                put(pdef + base + 4 * k, v)
        for off in range(0x144, 0x164, 4):
            put(pdef + off, tyre_pool(r, 1.0))
        for off in range(0x204, 0x244, 4):
            put(pdef + off, tyre_pool(r, 2.0))
        for off in range(0x1A4, 0x1C4, 4):
            put(pdef + off, tyre_pool(r, 1.0))
        for off in (0x4C, 0x5C):  # the carcass stiffnesses
            put(pdef + off, tyre_pool(r, 2.0))
        for off in (0x90, 0x9C, 0xA8, 0xB4, 0xC0):  # the Locate hints, in range of each table
            put(state + off, r.randrange(4), "<I")
        for off in (0x98, 0xA4, 0xB0, 0xBC):
            put(state + off, r.uniform(0.0, 1.0))
        for off in range(0xD0, 0x104, 4):
            put(state + off, tyre_pool(r, 1.0))
        g = word(0x1659B70)
        if g and word(g + 0x24):
            put(word(g + 0x24) + 0x160E0, r.choice((0, 0x20, 0x21)), "<I")  # the game flag gating the f copy
        stack = [bits(r.choice((0.0, 1.0, -1.0, r.uniform(-30, 30), r.uniform(-1, 1)))), bits(r.uniform(0.001, 0.02)),
                 bits(tyre_pool(r, 2.0)), bits(tyre_pool(r, 2.0)), S, S + 0x100, S + 0x110, 0, S + 0x120, S + 0x130,
                 S + 0x140, S + 0x150, S + 0x160, S + 0x170, S + 0x174, S + 0x180]
        return stack, writes
    if addr == 0x531BA0:  # Sign: the factors and the spin
        for off in (0x30C, 0x298, 0x314, 0x3DC):
            put(this + off, tyre_pool(r, 4.0))
        put(this + 0x378, r.choice((0.0, 1.0, -1.0, r.uniform(-30, 30), r.uniform(-1, 1))))
        common()
        return [], writes
    if addr == 0x5322E0:  # TyreClamp (stdcall a, b, c, out): out is a scratch word
        common()
        # the branches are a sign test on a, a compare of a with t = -b*c, and the gate on b: drawn so each reaches
        a = r.choice((-1.0, -0.5, 0.5, 1.0, r.uniform(-2, 2)))
        b = r.choice((-1.0, -0.5, 0.0005, 0.001, 0.5, 1.0, r.uniform(-1, 1)))
        c = r.choice((-1.0, 0.0, 0.5, 1.0, r.uniform(-3, 3)))
        return [struct.unpack("<I", struct.pack("<f", v))[0] for v in (a, b, c)] + [this + 0x5F8], writes
    if addr == 0x532A30:  # ComputeSlip (dir, spin): the matrix, the direction and the spin
        for off in range(0x170, 0x1A0, 4):
            put(this + off, tyre_pool(r, 2.0))
        for k in range(3):
            put(this + 0x5E0 + 4 * k, tyre_pool(r, 2.0))
        common()
        spin = struct.unpack("<I", struct.pack("<f", tyre_pool(r, 30.0)))[0]
        return [this + 0x5E0, spin], writes
    if addr == 0x533640:  # SolveDirs (a, b, c, o1, o2, o3): the outputs are scratch words
        common()
        return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0],
                struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0],
                struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0],
                this + 0x5D0, this + 0x5D4, this + 0x5D8], writes
    if addr == 0x5338A0:  # Accumulate: the scratch words it reads, and the state and setup
        for off in (0x340, 0x344, 0x30C, 0x314, 0x238, 0x368, 0x36C, 0x374, 0x300, 0x3B0, 0x504, 0x508, 0x50C):
            put(this + off, tyre_pool(r, 2.0))
        common()
        return [], writes
    if addr == 0x535040:  # CTyreMatData::CopyFrom (src): the material block at this+0x1a0, the source at +0x4f0
        for k in range(0x100 // 4):
            put(this + 0x4F0 + 4 * k, r.getrandbits(32), "<I")
        return [this + 0x4F0], writes
    if addr == 0x532490:  # Set414 (v)
        return [r.getrandbits(32)], writes
    if addr in (0x533AE0, 0x533BF0, 0x533D40, 0x533E80, 0x5351A0):  # the contact solvers: p, qb and r in the scratch
        P, QB, R = this + 0x540, this + 0x580, this + 0x5A0
        put(this + 0x258, tyre_pool(r, 2.0))  # f96: the contact offset (CScratch word 0x96)
        for k in range(7):
            put(P + 4 * k, tyre_pool(r, 2.0))
        for k in range(8):
            put(R + 4 * k, r.choice((0.315, 0.3, 0.2, 1.0, 0.0, r.uniform(0.2, 0.4), r.uniform(-2, 2))))
        for k in range(3):
            put(QB + 4 * k, tyre_pool(r, 2.0))
        if addr == 0x5351A0:  # SolveDispatch (p, qb, r, a4, a5, a6): the two flags and a6 (the solver gets a6 + 1)
            return [P, QB, R, r.randrange(2), r.randrange(2), r.choice((-1, 0, 1, 2, 3, 4))], writes
        return [P, QB, R, r.choice((-1, 0, 1, 2, 3, 4, 7))], writes
    if addr == 0x531C70:  # Friction (k): the gate, speed and factors, the three vectors and the state's vector
        put(this + 0x3F0, r.choice((1.0, 0.5, 2.0, 0.0, -1.0, NAN, r.uniform(0.0, 2.0))))
        put(this + 0x360, tyre_pool(r, 150.0))
        put(this + 0x314, tyre_pool(r, 300.0))
        put(this + 0x1F8, tyre_pool(r, 300.0))
        for base in (0x170, 0x180, 0x190):
            for k in range(4):
                put(this + base + 4 * k, tyre_pool(r, 1.0))
        put(state + 0xC, r.choice((1, 1, 1, 0)), "<I")
        for k in range(4):
            put(state + 0x80 + 4 * k, tyre_pool(r, 1.0))
        return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0]], writes
    if addr == 0x534090:  # HeatStep (a, b, c): the heat source and factors, the state's table, weights and damper blocks
        wear = this + 0x5C0  # m_wear: the pointer at this+0x414; one word per tyre index (0..2)
        put(this + 0x414, wear, "<I")
        for off in (0x288, 0x368, 0x374, 0x378, 0x3C8, 0x310, 0x388, 0x308):
            put(this + off, tyre_pool(r, 2.0))
        put(this + 0x360, r.uniform(0.0, 30.0))  # the speed
        put(this + 0x3C0, r.choice((0, 1, 2, 5)), "<I")  # the tyre index (5 in the drive)
        for off in (0xA8, 0xAC, 0xC0, 0xC4):
            put(state + off, r.choice((0, 1, 2)), "<I")
        put(state + 0xB0, r.uniform(0.0, 1.0))  # the blend weights
        put(state + 0xC8, r.uniform(0.0, 1.0))
        for off in range(0xD0, 0xE0, 4):  # the table entries
            put(state + off, tyre_pool(r, 2.0))
        put(state + 0x12C, tyre_pool(r, 2.0))
        put(state + 0x130, tyre_pool(r, 2.0))
        put(pdef + 0x118, r.uniform(0.5, 3.0))  # the time constant (a divisor)
        for off in range(0x110, 0x130, 4):  # the damper blocks (divisors of s0 * s1)
            put(pdef + off, r.uniform(0.5, 3.0))
        for off in range(0x16C, 0x178, 4):
            put(pdef + off, r.uniform(0.0, 1.0))
        for off in range(0x1C4, 0x1D8, 4):
            put(pdef + off, r.uniform(0.0, 1.0))
        for k in range(6):
            put(wear + 4 * k, r.uniform(0.0, 1.2))
        return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0] for _ in range(3)], writes
    if addr in (0x534480, 0x5345A0, 0x534E70, 0x5358C0):  # TableStep, RelaxStep, BlendTable, RelaxCount: the state's table
        # and damper words (the same inputs for the four: BlendTable and RelaxCount read the same blocks)
        put(state + 0x1C, r.choice((0, 1, 2, 3, 5, 7, 10)), "<I")  # the tyre index is this word & 7
        for off in range(0xD0, 0xE0, 4):
            put(state + off, tyre_pool(r, 2.0))
        for off in (0x12C, 0x130, 0x64, 0x60, 0x68):
            put(state + off, tyre_pool(r, 2.0))
        put(state + 0x6C, r.uniform(0.5, 3.0))  # DamperForce divides by s3
        put(this + 0x360, r.uniform(0.0, 30.0))  # the speed
        for off in range(0x110, 0x130, 4):  # the damper blocks divide by s0 * s1
            put(pdef + off, r.uniform(0.5, 3.0))
        put(pdef + 0x118, r.uniform(0.5, 3.0))
        for off in (0x7C, 0xAC, 0x124):
            put(pdef + off, tyre_pool(r, 2.0))
        if addr == 0x534480:  # TableStep (idx, x)
            return [r.choice((0, 1, 2, 3)), struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0]], writes
        if addr == 0x534E70:  # BlendTable (i, j, a, b, o1, o2): the outputs at this+0x5B0 and +0x5B4
            floats = [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 1.0)))[0] for _ in range(2)]
            return [r.choice((0, 1, 2, 3)), r.choice((0, 1, 2, 3))] + floats + [this + 0x5B0, this + 0x5B4], writes
        return [struct.unpack("<I", struct.pack("<f", r.uniform(0.0, 0.1)))[0]], writes  # RelaxStep, RelaxCount (dt)
    if addr == 0x532470:  # SetStateField (flag): the state's and the scalars it reads are the scratch's own
        return [r.getrandbits(32)], writes
    if addr == 0x533490:  # SetState48 (v): the def's +0x7c and +0x8c, which the record's def window holds
        put(pdef + 0x7C, tyre_pool(r, 2.0))
        put(pdef + 0x8C, tyre_pool(r, 2.0))
        return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0]], writes
    if addr == 0x534000:  # ClearBias: no inputs
        return [], writes
    if addr in (0x5326B0, 0x5326C0, 0x52DB10):  # TyreTail_Get10 (+0x10), TyreTail_Get14 (+0x14), GetName (+4): the word read
        put(this + {0x5326B0: 0x10, 0x5326C0: 0x14, 0x52DB10: 0x4}[addr], r.getrandbits(32), "<I")
        return [], writes
    if addr == 0x535310:  # StepInner: the record is this (its words are fuzzed in place), the outputs at this+0x5c0
        for off in (0x30C, 0x340, 0x348, 0x34C, 0x358, 0x1C8, 0x1D8, 0x2F4, 0x36C, 0x370, 0x368, 0x38C):
            put(this + off, tyre_pool(r, 2.0))
        put(state + 0x4C, tyre_pool(r, 2.0))  # the effective radius
        for off in (0x9C, 0xA0):
            put(state + off, r.choice((0, 1, 2, 3, 5)), "<I")
        put(state + 0xA4, tyre_pool(r, 1.0))
        put(pdef + 0x3C, tyre_pool(r, 2.0))
        for base in (0x244, 0x264, 0x284, 0x2A4):
            for i in range(8):
                put(pdef + base + 4 * i, tyre_pool(r, 2.0))
        return [this, this + 0x5C0, this + 0x5D0, this + 0x5E0, this + 0x5E4, this + 0x5E8], writes
    if addr == 0x5346F0:  # SplitSlip: the slip speed around the thresholds 1 and 2 (and NaN)
        put(this + 0x360, r.choice((0.0, 0.5, 1.0, 1.5, 2.0, 2.5, NAN, r.uniform(-1.0, 3.0))))
        return [], writes
    if addr == 0x533080:  # WriteOutputs: the def's factor, the contact vectors and scalars, the out struct at this+0x4d0
        put(pdef + 0xAC, tyre_pool(r, 2.0))
        for off in (0x30C, 0x340, 0x35C, 0x360, 0x364, 0x368, 0x36C, 0x374, 0x3D8, 0x404):
            put(this + off, tyre_pool(r, 2.0))
        put(this + 0x388, r.randrange(1 << 32), "<I")
        for off in (0x160, 0x170, 0x180, 0x190):
            for k in range(4):
                put(this + off + 4 * k, tyre_pool(r, 1.0))
        return [this + 0x4D0], writes
    if addr == 0x532F10:  # FillOutputs: the def's fields, the state and the scalars, the src vector and the pair
        for off in (0x1C, 0x7C, 0x8C):
            put(pdef + off, tyre_pool(r, 2.0))
        put(state + 0x48, tyre_pool(r, 2.0))
        for off in (0x3E0, 0x3E4, 0x3A8, 0x35C, 0x358, 0x354, 0x36C, 0x374, 0x37C, 0x424):
            put(this + off, tyre_pool(r, 2.0))
        for k in range(4):
            put(this + 0x5C0 + 4 * k, r.uniform(-1.0, 1.0))
        for k in range(3):
            put(this + 0x5D0 + 4 * k, r.randrange(1 << 32), "<I")
        return [this + 0x5C0, this + 0x5D0, struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0]], writes
    if addr == 0x532B60:  # SolveForces: the state's grip factors, the contact vectors, the scalars, k and the outputs
        fuzz_grip(put, state, r)
        for off in (0x394, 0x3B0, 0x398, 0x314, 0x3F0, 0x3DC, 0x368, 0x37C, 0x36C, 0x3B8, 0x3AC, 0x3D0):
            put(this + off, tyre_pool(r, 2.0))
        for off in (0x170, 0x180, 0x190):
            for k in range(4):
                put(this + off + 4 * k, tyre_pool(r, 1.0))
        for k in range(4):
            put(this + 0x5C0 + 4 * k, r.uniform(-1.0, 1.0))
            put(this + 0x5D0 + 4 * k, r.uniform(-1.0, 1.0))
        return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0], this + 0x5C0, this + 0x5D0], writes
    if addr == 0x5352A0:  # SolvePair: the pair's words; the clamp runs when this+0x3f0 > 0
        put(this + 0x3F0, tyre_pool(r, 1.0))
        for off in (0x314, 0x1E8, 0x1F8, 0x394, 0x398):
            put(this + off, tyre_pool(r, 4.0))
        fuzz_grip(put, state, r)
        return [], writes
    if addr == 0x533760:  # ClampGrip: the grip fields, five float words, the out pointers at this+0x3fc and +0x400
        fuzz_grip(put, state, r)
        floats = [tyre_pool(r, 4.0) for _ in range(5)]
        return [struct.unpack("<I", struct.pack("<f", v))[0] for v in floats] + [this + 0x3FC, this + 0x400], writes
    if addr == 0x531FC0:  # Align: two vectors at this+0x5c0 and +0x5d0, a parallel pair and the length edge 0.1
        scale = r.choice((0.02, 0.1, 0.3, 1.0))
        a = [r.uniform(-scale, scale) for _ in range(3)]
        b = [k * r.uniform(-1.0, 1.0) for k in a] if r.random() < 0.3 else [r.uniform(-scale, scale) for _ in range(3)]
        for k, v in enumerate(a + [r.random()]):
            put(this + 0x5C0 + 4 * k, v)
        for k, v in enumerate(b + [r.random()]):
            put(this + 0x5D0 + 4 * k, v)
        return [this + 0x5C0, this + 0x5D0], writes
    if addr == 0x528980:  # Locate (table, x, n, out): a sorted table at this+0x5c0, the out struct at this+0x5f0
        n = r.choice((4, 6, 8))
        table = this + 0x5C0
        values = sorted(r.uniform(0.0, 10.0) for _ in range(n))
        for k, v in enumerate(values):
            put(table + 4 * k, v)
        put(this + 0x5F0, r.randrange(n), "<I")  # the hint (the out struct's lo; clear of the table)
        x = r.choice((values[0] - 1.0, values[-1] + 1.0, values[r.randrange(n)], r.uniform(values[0], values[-1])))
        return [table, struct.unpack("<I", struct.pack("<f", x))[0], n, this + 0x5F0], writes
    if addr == 0x5358F0:  # ContactPatch (thiscall, the out float at this+0x5f0): the sector record and the solve
        record, table = word(this + 0x410), word(this + 0x414)
        put(record + 0x5AC, r.uniform(-20.0, 20.0))  # the sector's angle
        for k in range(8):
            put(table + 4 * k, r.uniform(0.0, 1.0))
        put(this + 0x228, r.choice((0.0, 1e-11, 0.5, 1.0, r.uniform(0.1, 3.0))))  # G
        put(this + 0x258, r.choice((0.0, 0.1, r.uniform(-1.0, 3.0))))  # H
        put(this + 0x3AC, r.choice((0.0, 0.0005, -0.2, 0.7, r.uniform(-1.0, 1.0))))  # T
        for off in (0x3A8, 0x1E8, 0x208, 0x218, 0x1F8):
            put(this + off, r.uniform(-1.0, 1.0))
        put(state + 0x40, r.uniform(0.1, 2.0))
        put(state + 0x50, r.uniform(-1.0, 1.0))
        put(state + 0x54, r.uniform(-1.0, 1.0))
        put(state + 0x68, r.uniform(-1.0, 2.0))
        for off in (0x2C, 0x7C, 0x8C, 0x9C, 0xBC, 0xCC):
            put(pdef + off, r.uniform(-1.0, 2.0))
        put(pdef + 0xEC, r.uniform(0.1, 2.0))
        for off in (0x70, 0x74):
            put(state + off, r.choice((0, 1)), "<I")
        for k in range(3):  # the qb and p words the solve reads before it writes them
            put(this + 0x314 + 4 * k, r.uniform(-1.0, 1.0))
        return [this + 0x5F0], writes
    if addr == 0x535D60:  # SolvePatch: the StepInner inputs (the record at this+0x20), then the clamp inputs
        put(this + 0x3DC, r.choice((0.0, tyre_pool(r, 2.0))))
        for off in (0x30C, 0x340, 0x368, 0x36C, 0x34C, 0x350, 0x394, 0x3B0):
            put(this + off, tyre_pool(r, 2.0))
        put(this + 0x398, r.getrandbits(32), "<I")  # the clamp's b is this dword, read as a float
        for off in (0x340, 0x348, 0x34C, 0x358, 0x1C8, 0x1D8, 0x2F4, 0x36C, 0x370, 0x38C):
            put(this + 0x20 + off, tyre_pool(r, 2.0))
        put(state + 0x40, tyre_pool(r, 2.0))
        put(state + 0x44, tyre_pool(r, 2.0))
        put(state + 0x4C, tyre_pool(r, 2.0))
        for off in (0x9C, 0xA0):
            put(state + off, r.choice((0, 1, 2, 3, 5)), "<I")
        put(state + 0xA4, tyre_pool(r, 1.0))
        put(pdef + 0x3C, tyre_pool(r, 2.0))
        for base in (0x244, 0x264, 0x284, 0x2A4):
            for i in range(8):
                put(pdef + base + 4 * i, tyre_pool(r, 2.0))
        return [], writes
    if addr == 0x5320C0:  # ComputeOutputs: the state's indices and weight, the CDef tables, src = this
        for off in (0x9C, 0xA0):
            put(state + off, r.choice((0, 1, 2, 3, 5)), "<I")
        put(state + 0xA4, tyre_pool(r, 1.0))
        for base in (0x244, 0x264, 0x284, 0x2A4):
            for i in range(8):
                put(pdef + base + 4 * i, tyre_pool(r, 2.0))
        put(this + 0x36C, r.getrandbits(32), "<I")
        put(this + 0x370, r.getrandbits(32), "<I")
        scalar = lambda: struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0]
        outs = [this + 0x5C0 + 4 * i for i in range(9)]  # stack words 7 to 15; word 16 is this+0x5e8
        return [scalar(), scalar(), scalar(), scalar(), scalar(), scalar(), this] + outs + [this + 0x5E8], writes
    if addr in (0x5334D0, 0x5325F0):  # TyreFrictionCurve, FloorS32: the table at this+0x5C0 (count+2 words)
        table, count = this + 0x5C0, r.choice((0, 1, 2, 3, 4))
        for k in range(count + 2):
            put(table + 4 * k, tyre_pool(r, 2.0))
        if addr == 0x5325F0:  # FloorS32 (x, table, count)
            return [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 4.0)))[0], table, count], writes
        words = [struct.unpack("<I", struct.pack("<f", tyre_pool(r, 2.0)))[0] for _ in range(9)]
        return words + [this + 0x5B0, this + 0x5B4, table, count], writes  # the outputs at this+0x5B0 and +0x5B4
    raise ValueError("no fuzz for %#x" % addr)


# Lifecycle (direct calls, `harness/direct_calls.py`; not reached by a step). The CDef is 0x120 bytes and its second
# block (this[1]) follows it, so the def window is 0x400 as the step's. CopyDefFrom's source is the setup (arg 1), whose
# block at +0x2c the copy reads up to +0x6b0. The recipes build the objects in the stub heap, every word random first.
DEF_WINDOW = 0x400
SETUP_WINDOW = 0x6B0


def random_block(base, size, r):
    return [(base + at, struct.pack("<I", r.getrandbits(32))) for at in range(0, size, 4)]


def cdef_direct(mu, heap, r):
    """CTyre::CDef::CDef on `this`: a def in the stub heap, every word random (the constructor writes its defaults)."""
    this = heap + 0x20000
    return [], random_block(this, DEF_WINDOW, r), this


def copy_def_direct(mu, heap, r):
    """CTyre::CScratch::CopyDefFrom(src) on `this`: the scratch (its +0x10 def and +0x14 state), the def, the state and
    the source (its +0x2c setup block), random words first and the pointers after them."""
    this, pdef, state, src, setup = heap + 0x20000, heap + 0x30000, heap + 0x40000, heap + 0x60000, heap + 0x70000
    writes = random_block(this, 0x600, r) + random_block(pdef, DEF_WINDOW, r) + random_block(state, 0x200, r)
    writes += random_block(src, 0x30, r) + random_block(setup, SETUP_WINDOW, r)
    writes += [(this + 0x10, struct.pack("<I", pdef)), (this + 0x14, struct.pack("<I", state)),
               (src + 0x2C, struct.pack("<I", setup))]
    return [src], writes, this


TYRE_SPECS_LIFECYCLE = {
    0x530C50: dict(name="NPhysics::CTyre::CDef::CDef", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: DEF_WINDOW)], direct_fuzz=cdef_direct),
    0x534760: dict(name="NPhysics::CTyre::CScratch::CopyDefFrom", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: DEF_WINDOW, 0x10),
                            (1, lambda a: 0x30), (1, lambda a: SETUP_WINDOW, 0x2C)],
                   direct_fuzz=copy_def_direct),
}

CSTATE_WINDOW = 0x400
SCRATCH_WINDOW = 0x600


def cstate_direct(mu, heap, r):
    """NPhysics::CTyre::CState::CState(this, a, b, c) on `this`: a state in the stub heap, every word random; the three
    stack words (the original returns with `ret 0xC`) are pointers to blocks in the heap, unread by the constructor."""
    this = heap + 0x20000
    words = [heap + 0x30000, heap + 0x31000, heap + 0x32000]
    return words, random_block(this, CSTATE_WINDOW, r), this


def scratch_init_direct(mu, heap, r):
    """NPhysics::CTyre::CScratch::Init on `this`: a scratch in the stub heap, every word random."""
    this = heap + 0x20000
    return [], random_block(this, SCRATCH_WINDOW, r), this


def scratch_ctor_direct(mu, heap, r):
    """NPhysics::CTyre::CScratch::CScratch(def, state, o) on `this`: a scratch in the stub heap, random; the def is a
    block in the heap (the constructor reads its name word), the state and o are pointers it stores."""
    this, pdef, state, o = heap + 0x20000, heap + 0x30000, heap + 0x40000, heap + 0x50000
    writes = random_block(this, SCRATCH_WINDOW, r) + random_block(pdef, 0x10, r)
    return [pdef, state, o], writes, this


TYRE_SPECS_LIFECYCLE_2 = {
    0x531500: dict(name="NPhysics::CTyre::CState::CState", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: CSTATE_WINDOW)], direct_fuzz=cstate_direct),
    0x5327D0: dict(name="NPhysics::CTyre::CScratch::Init", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: SCRATCH_WINDOW)], direct_fuzz=scratch_init_direct),
    0x5328D0: dict(name="NPhysics::CTyre::CScratch::CScratch", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: SCRATCH_WINDOW), (1, lambda a: 0x10)], direct_fuzz=scratch_ctor_direct),
}

SCRATCH_SIZE = 0x3F0


def create_scratch_direct(mu, heap, r):
    """NPhysics::CTyre::CDef::CreateScratch on `this` (the def, unread): the call's block is the one operator new returns."""
    this = heap + 0x20000
    return [], [], this


def create_scratch_after(mu, args, eax):
    """The block the call allocated (its eax, as `operator new` returns it) and the scratch Init writes into it."""
    return [(eax, SCRATCH_SIZE)] if eax else []


TYRE_SPECS_LIFECYCLE_3 = {
    0x535220: dict(name="NPhysics::CTyre::CDef::CreateScratch", popped=0, x87=False, thiscall=True,
                   windows=[], after=create_scratch_after, direct_fuzz=create_scratch_direct),
    0x70BEC2: dict(name="operator new (CRT)", popped=0, x87=False, cdecl=True, external=True, windows=[]),
}

DTOR_WINDOW = 0x200


# The material's flags (a CBitField) hold a pointer the destructor calls through when it is not null, and the offset of
# the word depends on the caller's path. The records take the null case: the words at +0x2b0..+0x300 are zeroed (the
# flags' pointer lies there in the material and its sub-block), so the destructors never call through a random pointer.
FLAGS_ZERO = [(at, struct.pack("<I", 0)) for at in range(0x2B0, 0x300, 4)]


def block_direct(mu, heap, r):
    """A destructor on `this` (a block in the stub heap, every word random, the flags zeroed)."""
    this = heap + 0x20000
    return [], random_block(this, DTOR_WINDOW, r) + [(this + at, b) for at, b in FLAGS_ZERO], this


def scratch_dtor_direct(mu, heap, r):
    """~CScratch / DeleteScratch / DeleteContainer on `this`: the scratch in the stub heap, random. Its def is null or a
    CDef (its vtable word is CDef's, so the release call reads the table; the slot is a no-op) with a random state."""
    this, pdef, state = heap + 0x20000, heap + 0x30000, heap + 0x40000
    writes = random_block(this, 0x600, r) + random_block(pdef, DEF_WINDOW, r) + random_block(state, 0x200, r)
    writes += [(this + at, b) for at, b in FLAGS_ZERO]
    null = r.random() < 0.5
    if not null:
        writes += [(pdef, struct.pack("<I", 0x740B04)), (this + 0x10, struct.pack("<I", pdef)),
                   (this + 0x14, struct.pack("<I", state))]
    else:
        writes += [(this + 0x10, struct.pack("<I", 0)), (this + 0x14, struct.pack("<I", state))]
    return [r.getrandbits(3)], writes, this


def scratch_body_direct(mu, heap, r):
    """The destructor body (0x532570) on `this`: a scratch in the stub heap, random, the flags zeroed."""
    this = heap + 0x20000
    return [], random_block(this, 0x600, r) + [(this + at, b) for at, b in FLAGS_ZERO], this


def scratch_dtor_fuzz_free(args, rng):
    return {0: rng.choice((0, 1, 2, 3))}


TYRE_SPECS_LIFECYCLE_4 = {
    0x5324A0: dict(name="NTweak CBase block x17 (5324a0)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: DTOR_WINDOW)], direct_fuzz=block_direct),
    0x532550: dict(name="NTweak CBase block x3 (532550)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: DTOR_WINDOW)], direct_fuzz=block_direct),
    0x532570: dict(name="NPhysics::CTyre::CScratch::CScratch (532570, the destructor body)", popped=0, x87=False,
                   thiscall=True, windows=[(0, lambda a: 0x600)], direct_fuzz=scratch_body_direct),
    0x5329A0: dict(name="NPhysics::CTyre::CScratch::~CScratch", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: DEF_WINDOW, 0x10)],
                   direct_fuzz=scratch_dtor_direct),
    0x5328B0: dict(name="NPhysics::CTyre::CScratch::DeleteScratch", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: DEF_WINDOW, 0x10)],
                   direct_fuzz=scratch_dtor_direct),
    0x535280: dict(name="NPhysics::CTyre::CDef::DeleteContainer (535280)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x600), (0, lambda a: 0x200, 0x14), (0, lambda a: DEF_WINDOW, 0x10)],
                   direct_fuzz=scratch_dtor_direct),
}

SPECS = {**TYRE_SPECS, **TYRE_SPECS_LIFECYCLE, **TYRE_SPECS_LIFECYCLE_2, **TYRE_SPECS_LIFECYCLE_3, **TYRE_SPECS_LIFECYCLE_4}
# `--tyre-fuzz`: the leaves it calls with fuzzed inputs, the scratch the first of them is called on, and the fuzzer.
FUZZ = dict(addrs=TYRE_FUZZ, scratch=TYRE_FUZZ_SCRATCH, fuzz=fuzz_tyre)
