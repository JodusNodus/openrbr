#!/usr/bin/env python3
"""Test vectors for VCU functions, from the original game's own calls in a step-replay snapshot.

Runs one CPhysics::StepForward of the original (harness/replay.py, the same emulator and oracle) and
records every call of the listed functions: the arguments on entry, the bytes of the windows they
point at before and after the call, and the return value (eax, or st0 for x87 returns). Calls made
by a listed function to an "external" entry (a drive accessor, say) are kept as its "inner" records:
their arguments and return values, in call order. Our Rust port replays each record and must
reproduce the return value and every window (rust/rbr-physics, tests/vcu_vectors.rs). A call is
classified by the branch the asm takes (SPECS "branch"), so the output says which branches the step
covers.

    python3 -I harness/vcu_vectors.py SNAPSHOT OUT.txt [--set vcu|entities|tyre|spring-damper]
        [--max N] [--vcu-on | --fuzz SEED]

--vcu-on turns the launch control and the VCU on (CScratchSim+0x40 and +0x50) at each GearboxUpdate call.
The reference drive runs with both off, so the gear logic would otherwise never run. The recorded calls are
the original's behaviour on the inputs they see, with these two flags changed.

--fuzz SEED does the same and also draws the pedals, the gear and its request, the slip flag, the speeds,
the state (timer, lockout, cooldown, pedal swap), and the gear thresholds at random for each GearboxUpdate
call, so the recorded calls cover the branches the drive itself does not reach. The inputs are put back when
the call returns (after its windows are taken), so the rest of the step runs on the drive's own values.

Output format (one record per call, hex words, little-endian bytes):

    call <index> <function address> <name>
    args <word> ...                       arguments as u32 words (this first when thiscall; floats as bits)
    win <k> <address> <len> pre <hex>     window k before the call (bytes)
    win <k> <address> <len> post <hex>    the same window after the call (k matches)
    ret <eax u32> <st0 f64 bits>          eax, and st0 as f64 bits (0 when the function has no x87 return)
    inner <address> <eax> <st0 bits> <args...>   an external call made by this one, in call order
    branch <name>                         branch taken (see SPECS)
    end

--set spring-damper records the E48 spring-damper calls. Its --fuzz SEED mode also varies each
reached helper's inputs and restores the input/output footprint when that call returns.

Needs the worktree's .venv (unicorn, numpy) and the ssh oracle for x87 results (replay.py).
"""

import argparse
import math
import os
import random
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.append(HERE)  # appended: harness/bisect.py must not shadow the stdlib bisect that replay.py imports

import replay  # noqa: E402  (harness/replay.py: Snapshot, Machine, FpuOracle)
from unicorn import UC_HOOK_BLOCK, UC_HOOK_CODE  # noqa: E402
from unicorn.x86_const import (UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP,  # noqa: E402
                               UC_X86_REG_FPSW, UC_X86_REG_FP0)

F32_TWO31 = 2147483648.0


def f32(word):
    return struct.unpack("<f", struct.pack("<I", word))[0]


def f32_round(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def signed(word):
    return struct.unpack("<i", struct.pack("<I", word))[0]


def table_branch(args, windows):
    """Branch of VcuTableBlend (0x536950), from the asm: 'neg' (f < 0 or NaN, logs), 'big' (f >= 2^31),
    'i_hi' (i > n - 2), 'i_lo' (i < 0), 'interp' (normal)."""
    x, lo, hi, n = f32(args[1]), f32(args[2]), f32(args[3]), signed(args[4])
    # SSE: f = ((x - lo) * (float)(n - 1)) / (hi - lo), each op rounded to f32
    f = f32_round(f32_round(f32_round(x - lo) * float(n - 1)) / f32_round(hi - lo))
    if not (f >= 0.0):
        return "neg"
    if f >= F32_TWO31:
        return "big"
    i = int(f)
    if i > n - 2:
        return "i_hi"
    if i < 0:
        return "i_lo"
    return "interp"


def lookup_branch(args, windows):
    """Branch of LookupPairTable (0x536880), from the asm: 'flat' (table[7]), 'top' (x above the last
    record: its value at -0x04), 'unrolled' (found in the unrolled search), 'linear' (the final search
    moved past the unrolled part). Compares follow the asm's jb/jbe, so NaN is classed as the asm does.
    The table's bytes (window 0) are the record reads."""
    table, x, n = args[0], f32(args[1]), args[2]
    pre = windows[0][3]

    def fv(at):
        return f32(struct.unpack_from("<I", pre, at - table)[0])

    if fv(table + 0xC) >= x:
        return "flat"
    if x > fv(table + n * 0x20 - 0x14):
        return "top"
    u, branch = 1, "unrolled"
    if n - 1 > 3:
        e = table + 0x4C
        while True:
            if not x > fv(e - 0x20):
                break
            if not x > fv(e):
                u += 1
                break
            if not x > fv(e + 0x20):
                u += 2
                break
            if not x > fv(e + 0x40):
                u += 3
                break
            u += 4
            e += 0x80
            if u >= n - 3:
                break
    while u < n and x > fv(table + u * 0x20 + 0xC):
        u += 1
        branch = "linear"
    return branch


def ext_to_f64(parts):
    """An x87 register value (mantissa, exponent word) as a Python float (exact for 24-bit values)."""
    mant, se = parts
    sign = -1.0 if se & 0x8000 else 1.0
    e = (se & 0x7FFF)
    if e == 0 and mant == 0:
        return 0.0 * sign
    if e == 0x7FFF:  # infinity, or a NaN (the fuzzed inputs can be NaN)
        return sign * math.inf if mant & ((1 << 63) - 1) == 0 else math.copysign(math.nan, sign)
    return sign * math.ldexp(mant, e - 16383 - 63)


def no_branch(args, windows):
    """Not classified: the branch is visible in the windows, or the function has none."""
    return "-"


def diff_branch(args, windows):
    """Centre branch of DiffLocks (0x537b70), from the asm: 'zero' (handbrake at or above the release
    value), 'lf' (the left-foot maps: threshold nonzero, brake and throttle above it), 'normal'.
    Window 0 is the scratch (pedals, handbrake), window 1 the setup."""
    scratch, setup = windows[0][3], windows[1][3]
    throttle, brake, handbrake = (f32(struct.unpack_from("<I", scratch, o)[0]) for o in (0xC, 0x10, 0x14))
    release = f32(struct.unpack_from("<I", setup, 0x198)[0])
    lft = f32(struct.unpack_from("<I", setup, 0x1A8)[0])
    if not release > handbrake:
        return "zero"
    if lft != 0.0 and brake > lft and throttle > lft:
        return "lf"
    return "normal"


def _window_f32(windows, window, offset):
    return struct.unpack_from("<f", windows[window][3], offset)[0]


def _ieee_div(numerator, denominator):
    if denominator != 0.0:
        return numerator / denominator
    if numerator == 0.0 or math.isnan(numerator):
        return math.nan
    negative = math.copysign(1.0, numerator) != math.copysign(1.0, denominator)
    return -math.inf if negative else math.inf


# name, bytes popped by ret (stack words are recorded from it), x87 return, branch classifier,
# thiscall (ecx is recorded as args[0]), external (kept only as an inner record of a listed caller),
# windows: (argument index, byte length function of the arguments[, offsets: the window starts at the
# pointer stored at argument + offset, then follows each further offset the same way]).
VCU_SPECS = {
    0x536950: dict(name="E49Proxy::VcuTableBlend", popped=0x14, x87=True, branch=table_branch,
                   windows=[(0, lambda a: 16 * max(0, min(signed(a[4]), 64)))]),
    0x536880: dict(name="NPhysics::LookupPairTable", popped=0xC, x87=True, branch=lookup_branch,
                   windows=[(0, lambda a: 0x20 * min(a[2], 64) + 0x20)]),
    # args: this, t1, t2, t3, f. The pedals are read at this+0x0c and +0x10; the tables are 11 knots.
    0x5376E0: dict(name="CScratchSim::LockMax", popped=0x10, x87=True, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x20), (1, lambda a: 0x180), (2, lambda a: 0x180),
                            (3, lambda a: 0x180)]),
    # args: this. The scratch (pedals at +0x0c, +0x10, handbrake at +0x14) and the setup (CDef, 0xcac bytes,
    # at the pointer stored at this+0).
    0x537B70: dict(name="CScratchSim::DiffLocks", popped=0, x87=False, thiscall=True,
                   branch=diff_branch, windows=[(0, lambda a: 0x20), (0, lambda a: 0xCAC, 0)]),
    # args: this, x. The rev-limit cut: the engine speed comes from CDrive::Get4520, the flags go to
    # CEngine::SetState4 (inner records). Windows: the scratch, the setup (rpm limit), the state (cooldown).
    0x536EE0: dict(name="CScratchSim::RevLimitCut", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0xCAC, 0), (0, lambda a: 0x68, 4)]),
    # args: this, x. The slip gate: the scratch's vcu_enabled and the drive's sub-object flag (drive + 8, +0x55c;
    # inner record CDrive::Get08 returns that pointer).
    0x537110: dict(name="CScratchSim::CheckSlip", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x600, 0x58, 8)]),
    # drive accessors called by DiffLocks: the speed key and the lock setter (its arguments are the
    # lock before the drive clamps it to [0, 1]).
    0x537160: dict(name="CScratchSim::GearboxUpdate", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x10, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0),
                            (0, lambda a: 0x658, 0x58, 4)]),
    0x4E6770: dict(name="CCar::Get15F8", popped=0, x87=True, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x5047B0: dict(name="CDrive::Set47B0", popped=4, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x5375B0: dict(name="CScratchSim::LaunchClutch", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x10, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0)]),
    0x504820: dict(name="CDrive::Set4820", popped=4, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    # args: this, setup. The setup (the source) is read from +0x70 to +0xb6b; the scratch's CDef is at this+0.
    # the engine, drive and light setters Step makes (args: this, value; the lights: this, flag)
    0x50EB90: dict(name="CEngine::SetScratch08", popped=4, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
    0x5044A0: dict(name="CDrive::Set44A0", popped=4, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
    0x504450: dict(name="CDrive::Set30", popped=4, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
    0x4F1E40: dict(name="Lights::Set1e40", popped=4, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
    0x4F1ED0: dict(name="Lights::Set1ed0", popped=4, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
    0x537C90: dict(name="CScratchSim::CopyDef", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (1, lambda a: 0xB70), (0, lambda a: 0xCAC, 0)]),
    0x537F20: dict(name="CScratchSim::ReadPacket", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(1, lambda a: 0x20), (0, lambda a: 0x60)]),
    # args: this, dt. Windows: the scratch, the state, the drive, its D8, the CDef, the drive's definition (gear
    # count) and the gate of the diff locks (drive+0x170).
    0x537F50: dict(name="CScratchSim::Step", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x174, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0), (0, lambda a: 0x658, 0x58, 4),
                            (0, lambda a: 0x94, 0x58, 0x170)]),
    0x5380A0: dict(name="CScratchSim::StepBasic", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x174, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0), (0, lambda a: 0x658, 0x58, 4),
                            (0, lambda a: 0x94, 0x58, 0x170)]),
    0x537A80: dict(name="CScratchSim::FlagDispatch", popped=4, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x10, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0),
                            (0, lambda a: 0x968, 0x58, 4), (1, lambda a: 0x20)]),
    # the slip query (no stack args). Windows: the scratch (slip gates at +0x44, +0x48), the state (car at +0x68),
    # the drive and its D8, the CDef (rpm limit) and the drive definition's gear tables (+0x644, +0x694 per gear).
    0x536FE0: dict(name="CScratchSim::QuerySlip", popped=0, x87=False, branch=no_branch, thiscall=True,
                   windows=[(0, lambda a: 0x60), (0, lambda a: 0x70, 4), (0, lambda a: 0x10, 0x58),
                            (0, lambda a: 0x600, 0x58, 8), (0, lambda a: 0xCAC, 0), (0, lambda a: 0x968, 0x58, 4)]),
    0x504370: dict(name="CDrive::Get04", popped=0, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x506840: dict(name="CDrive::M6840", popped=4, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x504540: dict(name="CDrive::Get4540", popped=0, x87=True, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x504570: dict(name="CDrive::Set4570", popped=8, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x504520: dict(name="CDrive::Get4520", popped=0, x87=True, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x504380: dict(name="CDrive::Get08", popped=0, x87=False, branch=no_branch, thiscall=True, external=True,
                   windows=[]),
    0x50D340: dict(name="CEngine::SetState4", popped=0x10, x87=False, branch=no_branch, thiscall=True,
                   external=True, windows=[]),
}


# Entity layer (src/rbr/RBR_E43_*, RBR_E44_*; rust/rbr-physics/src/entities). Same record format.
# Frame, matrix and CGround/CHingedRigidBody leaves that make no external call yet.
ENTITY_SPECS = {
    # args: this, src. Table build: this (the 8-byte entries, count at +0x804) and src (tags, values, count
    # at +0x700; at most 256 entries, so the 0x704 bytes cover them).
    0x520190: dict(name="E44MatTable::Build", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x808), (1, lambda a: 0x704)]),
    # args: this (the 12-byte vector of the hinged body's state; written to 1, 1, 1).
    0x51CD60: dict(name="E44_FillOnes", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x0c)]),
    # constant slots: the return value (eax) is the check.
    0x51CC30: dict(name="CHingedRigidBody::CDef::Vf4c", popped=0, x87=False, thiscall=True, windows=[]),
    0x51CC40: dict(name="CHingedRigidBody::CDef::Unk20", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BA20: dict(name="CGround::CDef::Vf4c", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BB00: dict(name="CGround::CDef::Unk20", popped=0, x87=False, thiscall=True, windows=[]),
    0x520820: dict(name="CMaterial::Unk20", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BB60: dict(name="CGround::Vf88", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BB70: dict(name="CGround::Vf110", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BB80: dict(name="CGround::Vf114", popped=0, x87=False, thiscall=True, windows=[]),
    0x51BC70: dict(name="CGround::VfCC", popped=8, x87=False, thiscall=True, windows=[]),
    # args: this, val. Stores val at this+0x650.
    0x51DA50: dict(name="CHingedRigidBody::Set650", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x654)]),
    # args: this, a, b. out[i] = this[i] * a[i] for four floats (SSE single precision).
    0x51C8C0: dict(name="E44Shell::M51C8C0", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # args: this, p. The frame's vtable, p (+0x04) and depth (+0x08 = p[8] + 1, or 0 for p == 0).
    0x51AF70: dict(name="CFrame::CFrame", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x0c), (1, lambda a: 0x10 if a[1] else 0)]),
}

def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def vtarget(mu, obj, slot):
    """The function in vtable slot `slot` of the object at obj (read at entry: the target of that call)."""
    return word(mu, word(mu, obj) + 4 * slot)


# CEntityFrame / CEntityOldFrame (src/rbr/RBR_E43_Frame.h): the owner is at +0xc, the depth at +8 (0 = no hook).
# A hook calls owner slot 39..48 (CEntityFrame) or the gate (slot 32) and X or Y (OldFrame), with (a, b).
def frame_ctor_targets(slot):
    """The constructor takes the parent from `ent`'s vtable slot (Vf30 = 12, Vf34 = 13) and calls it twice."""
    return lambda mu, a: [(vtarget(mu, a[2], slot), 0)] * 2 if a[2] else []


def entity_hook_targets(slot):
    return lambda mu, a: [(vtarget(mu, word(mu, a[0] + 0xC), slot), 8)] if word(mu, a[0] + 8) else []


def old_hook_targets(x, y):
    def targets(mu, a):
        if not word(mu, a[0] + 8):
            return []
        owner = word(mu, a[0] + 0xC)
        return [(vtarget(mu, owner, 32), 0), (vtarget(mu, owner, x), 8), (vtarget(mu, owner, y), 8)]
    return targets


FRAME_WINDOW = [(0, lambda a: 0x10)]  # this: the vtable, parent, depth and owner
# The root identity object's pointer (g_7cbb00), which a frame built without an entity hangs under.
ROOT_IDENTITY_WINDOW = lambda mu, a: [(0x007CBB00, 4)]
FRAME_SPECS = {
    0x5194B0: dict(name="CEntityFrame::CEntityFrame", popped=8, x87=False, thiscall=True, windows=[(0, lambda a: 0x10), (2, lambda a: 0x10 if a[2] else 0)],
                   nodes=ROOT_IDENTITY_WINDOW, dispatch=frame_ctor_targets(12)),
    0x519650: dict(name="CEntityOldFrame::CEntityOldFrame", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (2, lambda a: 0x10 if a[2] else 0)], nodes=ROOT_IDENTITY_WINDOW,
                   dispatch=frame_ctor_targets(13)),
}
# CEntityFrame::Vf04..Vf18 forward to owner slots 39, 40, 43, 44, 47, 48
for addr, name, slot in zip((0x519530, 0x519560, 0x519590, 0x5195C0, 0x5195F0, 0x519620),
                            ("Vf04", "Vf08", "Vf0C", "Vf10", "Vf14", "Vf18"), (39, 40, 43, 44, 47, 48)):
    FRAME_SPECS[addr] = dict(name="CEntityFrame::" + name, popped=8, x87=False, thiscall=True, windows=FRAME_WINDOW,
                             dispatch=entity_hook_targets(slot))
# CEntityOldFrame::Vf04..Vf18: gate on owner slot 32, then X or Y
for addr, name, (x, y) in zip((0x5196D0, 0x519720, 0x519770, 0x5197C0, 0x519810, 0x519860),
                              ("Vf04", "Vf08", "Vf0C", "Vf10", "Vf14", "Vf18"),
                              ((39, 41), (40, 42), (43, 45), (44, 46), (47, 49), (48, 50))):
    FRAME_SPECS[addr] = dict(name="CEntityOldFrame::" + name, popped=8, x87=False, thiscall=True,
                             windows=FRAME_WINDOW, dispatch=old_hook_targets(x, y))
ENTITY_SPECS.update(FRAME_SPECS)

# E43 pair walkers (src/rbr/RBR_E43_MatrixA.cpp, MatrixB.cpp): each walks its node (this) and the node o2 through the
# +4 links of both trees, calling vtable slots 1..6 (S04..S18) of the two nodes and, directly, the walkers. A record
# keeps the walk's input: each node of the two chains (its 0x10 bytes), so the port can recurse on the same memory.
NODE_SLOTS = (1, 2, 3, 4, 5, 6)  # vtable indices of S04..S18 (E43NodeSlots)
NODE_LIMIT = 64  # nodes followed per chain


def node_slot_targets(mu, a):
    return [(vtarget(mu, obj, slot), 8) for obj in (a[0], a[1]) for slot in NODE_SLOTS]


def chains(*roots):
    """The +4 chains from the arguments at `roots`: each node's 0x10 bytes (its child link and depth)."""
    def nodes(mu, a):
        out = []
        for i in roots:
            p = a[i]
            for _ in range(NODE_LIMIT):
                if not p:
                    break
                out.append((p, 0x10))
                p = word(mu, p + 4)
        return out
    return nodes


WALK_SPECS = {}
for addr, name in ((0x51A9D0, "W9D0"), (0x51AA20, "WA20"), (0x51AA70, "WA70"), (0x51AFE0, "WFE0"), (0x51B050, "WB050"),
                   (0x51B0C0, "WB0C0"), (0x51B130, "WB130"), (0x51B1A0, "WB1A0"), (0x51B320, "WB320"), (0x51B370, "WB370"),
                   (0x51B3C0, "WB3C0"), (0x51B410, "WB410")):
    WALK_SPECS[addr] = dict(name="E43PairWalk::" + name, popped=12, x87=False, thiscall=True, windows=[], direct=True,
                            nodes=chains(0, 1), dispatch=node_slot_targets)
# MB560(a, b, c): copies the 16 bytes at a to c, then walks this against b (WB370, with c and 1).
WALK_SPECS[0x51B560] = dict(name="E43PairWalk::MB560", popped=12, x87=False, thiscall=True,
                            windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)], direct=True, nodes=chains(0, 2))
# MF8B0(name, b, cursor) (src/rbr/RBR_E43_Misc.cpp): counts the factory table at 0x7c8040 (the count loop), looks
# the name at `a` up with CDictBase::Find, appends the index to this's list (count at +0x18, list at +0x1c) and
# advances the token cursor `c`. Its three calls are direct, so they are its inner records.
WALK_SPECS[0x5198B0] = dict(name="E43PairWalk::MF8B0", popped=12, x87=False, thiscall=True,
                            windows=[(0, lambda a: 0x80), (1, lambda a: 0x20), (3, lambda a: 8)], direct=True)
TABLE_SPECS = {
    0x502050: dict(name="CEntityFactory::Vf0C (count of 0x7c8040)", popped=0, x87=False, thiscall=True,
                   windows=[], external=True),
    0x52EBF0: dict(name="NTweak::CDictBase::Find", popped=8, x87=False, thiscall=True, windows=[], external=True),
    0x52EE50: dict(name="NTweak::CTokenCursor::AdvanceIfSet", popped=0, x87=False, thiscall=True, windows=[],
                   external=True),
}
WALK_SPECS.update(TABLE_SPECS)
ENTITY_SPECS.update(WALK_SPECS)

# B3, the CEntity tree (src/rbr/RBR_E43_EntityA/B/C.cpp). The child list is at +0x48 (head), +0x4c (tail), +0x50 (count),
# the siblings link through +0x10 (next). The exclusion set is at +0x2c: [count, members...].
def tail_node(mu, a):
    """The child list's tail (the node an append links to), as a 0x14-byte window."""
    p = word(mu, a[0] + 0x4C)
    return [(p, 0x14)] if p else []


def exclusion_sets(mu, a):
    """Both exclusion sets of the entities at a[0] and a[1] (count, then members), when allocated."""
    out = []
    for obj in a[:2]:
        s = word(mu, obj + 0x2C)
        if s:
            out.append((s, 4 + 4 * word(mu, s)))
    return out


B3_SPECS = {
    # args: this, p. Appends p to the child list (the head is written only when the list was empty).
    0x51A370: dict(name="CEntity::E43Vf150 (AppendChild)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x64), (1, lambda a: 0x20)], nodes=tail_node),
    # no arguments, no windows: `xor eax, eax; ret` (a hook that answers 0).
    0x519AC0: dict(name="CEntity::Ret0 (inferred)", popped=0, x87=False, thiscall=False, windows=[]),
    # no arguments: `ret` (an empty hook).
    0x519B90: dict(name="CEntity::Nop (inferred)", popped=0, x87=False, thiscall=False, windows=[]),
    # args: this, a, b. Stores a at this+0 and b at this+4, returns this (the asm's eax = ecx).
    0x51AB90: dict(name="CEntity::SetPair (inferred)", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 8)]),
    # args: this, other. The parent's test first (a direct call), then the smaller set holds the other side.
    0x519F80: dict(name="CEntity::E43_ExclF80 (ExclusionTest)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x64), (1, lambda a: 0x64)], nodes=exclusion_sets, direct=True),
    # args: this, n. Allocates n words for the set at +0x2c, once (the CRT allocator is external).
    0x51A000: dict(name="CEntity::E43_ExclA000 (AllocExclusion)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x30)], direct=True),
    # operator new (CRT, cdecl, one size argument).
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, windows=[], external=True),
}


def child_nodes(mu, a):
    """The children of the list at a[0] (head +0x48, sibling link +0x10): each child's 0x14 bytes."""
    out = []
    c = word(mu, a[0] + 0x48)
    for _ in range(NODE_LIMIT):
        if not c:
            break
        out.append((c, 0x14))
        c = word(mu, c + 0x10)
    return out


def child_targets(slot, takes_p):
    """Each child's gate (vtable slot 32, S80) and its call (vtable index `slot`, with one pointer argument when
    takes_p)."""
    def targets(mu, a):
        out = []
        c = word(mu, a[0] + 0x48)
        for _ in range(NODE_LIMIT):
            if not c:
                break
            out.append((vtarget(mu, c, 32), 0))
            out.append((vtarget(mu, c, slot), 4 if takes_p else 0))
            c = word(mu, c + 0x10)
        return out
    return targets


# The gated walks (walk_sNN): each gated child gets one slot call, the walk's pointer or dt when the slot takes one.
# Their children are inner records, and the child walks are records of their own (direct).
GATED_WALKS = (
    (0x51A3A0, "WalkChildrenS20", 8, True),   # E43Vf20(p): vtable slot 8
    (0x51A3E0, "Vf28", 10, False),            # slot 10
    (0x51A440, "Vf48", 18, False),            # slot 18
    (0x51A250, "Vf58", 22, True),             # slot 22
    (0x51A290, "Vf5C", 23, False),            # slot 23
    (0x51A2C0, "Vf64", 25, False),            # slot 25
    (0x51A2F0, "Vf68", 26, True),             # slot 26
    (0x51A330, "Vf6C", 27, True),             # slot 27 (dt), after self slot 16 (NameWalk)
)
for addr, name, slot, takes_p in GATED_WALKS:
    B3_SPECS[addr] = dict(name="CEntity::E43" + name, popped=4 if takes_p else 0, x87=False, thiscall=True,
                          windows=[(0, lambda a: 0x64)], nodes=child_nodes, dispatch=child_targets(slot, takes_p),
                          direct=True)
# self slot 16 (Vf40, NameWalk) is called by Vf6C directly on this: a record of its own, an inner record of Vf6C.
B3_SPECS[0x51A330]["dispatch"] = lambda mu, a: [(vtarget(mu, a[0], 16), 0)] + child_targets(27, True)(mu, a)
# NameWalk (Vf40): the state's slot 17 (Unk44, external), then the pre-order walk with a stack functor.
B3_SPECS[0x51A4B0] = dict(name="CEntity::Vf40 (NameWalk)", popped=0, x87=False, thiscall=True,
                          windows=[(0, lambda a: 0x64)], direct=True,
                          dispatch=lambda mu, a: [(vtarget(mu, a[0] + 0x38, 17), 0)])
# WalkChildrenPre (E43_Walk1F0(functor)): each gated child gets the functor (its vtable slot 1, external) and the walk.
def pre_order_targets(mu, a):
    return [(vtarget(mu, a[1], 1), 4)] + [(vtarget(mu, c, 32), 0) for c, _ in child_nodes(mu, a)]


B3_SPECS[0x51A1F0] = dict(name="CEntity::E43_Walk1F0 (WalkChildrenPre)", popped=4, x87=False, thiscall=True,
                          windows=[(0, lambda a: 0x64), (1, lambda a: 8)], nodes=child_nodes, direct=True,
                          dispatch=pre_order_targets)
# E43EntityDef (the def of a CEntity): its ctor takes the name and calls CBase(name) (0x52f090; its writes to +0 and +4
# are not in the windows); the dtor releases the list at +0x14 (its count at +8) and unlinks +0xc (E43_Unlink180).
def def_list(mu, a):
    arr = word(mu, a[0] + 0x14)
    return [(arr, 0xC)] if arr else []


B3_SPECS[0x51A530] = dict(name="E43EntityDef::E43EntityDef", popped=4, x87=False, thiscall=True,
                          windows=[(0, lambda a: 4)], nodes=lambda mu, a: [(a[0] + 0xC, 0x10)], direct=True)
B3_SPECS[0x51A560] = dict(name="E43EntityDef::~E43EntityDef", popped=0, x87=False, thiscall=True,
                          windows=[(0, lambda a: 0x1C)], nodes=def_list, direct=True)
B3_SPECS[0x52F090] = dict(name="NTweak::CBase::CBase (name)", popped=4, x87=False, thiscall=True, windows=[],
                          external=True)
B3_SPECS[0x51A180] = dict(name="E43_Unlink180 (link)", popped=0, x87=False, thiscall=True, windows=[],
                          external=True)
# CEntity ctor (this, flags, x, y, def): the frame bases, the name (GetNameOrNull, fastcall on flags: an external
# call with ecx only), the state CBase, then the stores.
B3_SPECS[0x51A640] = dict(name="CEntity::CEntity", popped=16, x87=False, thiscall=True, windows=[(0, lambda a: 0x64)],
                          direct=True)
B3_SPECS[0x52F0B0] = dict(name="NTweak::CBase::GetNameOrNull (flags)", popped=0, x87=False, thiscall=True, windows=[],
                          external=True)
ENTITY_SPECS.update(B3_SPECS)

SETS = {"vcu": VCU_SPECS, "entities": ENTITY_SPECS}
FUZZ_SETS = {}  # a set's `--tyre-fuzz`-style recipe, from its spec file's FUZZ
# More sets live in harness/vector_specs/<name>.py, one file per subsystem, so ports don't edit this file: each
# defines SET_NAME and SPECS and runs with this module's names (no_branch etc.) in scope.
_SPEC_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "vector_specs")
for _f in sorted(os.listdir(_SPEC_DIR)) if os.path.isdir(_SPEC_DIR) else []:
    if _f.endswith(".py"):
        _ns = dict(globals())
        with open(os.path.join(_SPEC_DIR, _f)) as _src:
            exec(compile(_src.read(), _f, "exec"), _ns)
        SETS[_ns["SET_NAME"]] = _ns["SPECS"]
        if "FUZZ" in _ns:
            FUZZ_SETS[_ns["SET_NAME"]] = _ns["FUZZ"]
SPRING_DAMPER_ADDRS = set(SETS.get("spring-damper", {}))
SPRING_FUZZ_ADDRS = {addr for addr, spec in SETS.get("spring-damper", {}).items() if spec.get("fuzz_call")}
SPECS = VCU_SPECS  # the active set: main() picks one with --set
CODE_BEGIN, CODE_END = 0x401000, 0x72FFFF  # the exe's .text (its code, through the rdata that follows)


class Recorder:
    """Hooks the entry and the return of each listed function in one run of the step. A call to an
    external entry made while a listed function is open becomes that function's inner record."""

    def __init__(self, machine, limit, launch=False, fuzz=None):
        self.mu = machine.mu
        self.limit = limit
        self.launch = launch or fuzz is not None
        self.rng = random.Random(fuzz) if fuzz is not None else None
        self.saved = []  # bytes the fuzzed inputs replaced, put back when GearboxUpdate returns
        self.calls = []
        self.frames = []  # open listed calls, innermost last
        self.hooked_rets = set()
        self.hooked_entries = set(SPECS)
        for addr in SPECS:
            self.mu.hook_add(UC_HOOK_CODE, self.on_entry, begin=addr, end=addr)
        # Dispatch targets and return sites found during the run go through one block hook over the exe's code,
        # installed before any block is translated: a code hook added mid-run is not honoured for a block that
        # already exists (the return site of a call made from a block translated earlier).
        self.watch = {}
        self.mu.hook_add(UC_HOOK_BLOCK, self.on_block, begin=CODE_BEGIN, end=CODE_END)

    def on_block(self, mu, addr, size, _):
        fn = self.watch.get(addr)
        if fn:
            fn(mu, addr, size, _)

    def dispatch(self, mu, target, popped, spec=None):
        """A vtable target of a listed function, found at its entry: recorded as an external (SPECS keeps its
        spec, so a later run of the same target needs no new entry). `spec` replaces the default external spec (its
        windows, a fuzzed result)."""
        if target not in SPECS:
            SPECS[target] = spec or dict(name="dispatch %#x" % target, popped=popped, x87=False, thiscall=True,
                                         external=True, windows=[])
        if target not in self.hooked_entries:
            self.hooked_entries.add(target)
            self.watch[target] = self.on_entry

    def fuzz_gearbox(self, mu, this):
        """Random inputs for one GearboxUpdate call (see the module doc). The values are the ones the asm reads."""
        r = self.rng
        word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


        def put(at, value, code="<f"):
            self.saved.append((at, bytes(mu.mem_read(at, 4))))
            mu.mem_write(at, struct.pack(code, value))

        state, drive = word(this + 0x4), word(this + 0x58)
        d8, drive_def, car, pdef = word(drive + 0x8), word(drive + 0x4), word(word(this + 0x4) + 0x68), word(this)
        put(this + 0x40, 1, "<I")
        put(this + 0x50, int(r.random() < 0.8), "<I")
        put(this + 0xC, r.choice(FUZZ_LEVELS + (r.random(), 0.0, 1.0)))  # throttle
        put(this + 0x10, r.choice(FUZZ_LEVELS + (r.random(), 0.0, 1.0)))  # brake
        put(state + 0x60, r.choice((-1.0, -0.5, -0.4, -0.1, 0.0, 0.2, 0.4, 0.5, 0.6, 1.0, NAN, r.uniform(-1, 1))))  # timer
        put(state + 0x2C, r.choice((0.0, 0.0, 0.0, 0.05, 0.7, NAN, -0.1, r.random())))  # shift lockout
        put(state + 0x64, r.random() * 0.1)  # shift cooldown
        put(state + 0x58, r.randrange(2), "<I")  # pedal swap
        gear = r.randrange(6)
        put(d8 + 0x54C, gear, "<I")
        put(d8 + 0x570, gear if r.random() < 0.8 else max(0, gear + r.choice((-1, 1))), "<I")  # commanded
        put(d8 + 0x55C, r.randrange(2), "<I")  # slip flag
        put(d8 + 0x45C, r.uniform(0, 600) if r.random() < 0.5 else r.uniform(400, 700))  # engine speed (rad/s)
        put(car + 0x15F8, r.choice((0.0, r.uniform(-1, 8), r.uniform(0, 0.6))))  # car speed
        put(drive_def + 0x654, r.randrange(3, 11), "<I")  # gear count of the drive's definition
        for k in range(GEARS):
            put(pdef + 0x18 + 0x20 * k, r.choice((0.0, r.uniform(0, 6000))))  # upshift rpm
            put(pdef + 0x28 + 0x20 * k, r.choice((0.0, r.uniform(0, 6000))))  # downshift rpm
        put(pdef + 0x158, r.uniform(0, 1))  # lockout after an upshift
        put(pdef + 0x168, r.uniform(0, 1))  # lockout after a downshift

    def fuzz_clutch(self, mu, this):
        """Random inputs for one LaunchClutch call: the launch mode, the clutch and its dirty flag, the ramp
        rate, the gear fields and the engine speed. Put back on return, as for GearboxUpdate."""
        r = self.rng
        word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]

        def put(at, value, code="<f"):
            self.saved.append((at, bytes(mu.mem_read(at, 4))))
            mu.mem_write(at, struct.pack(code, value))

        state, drive, pdef = word(this + 0x4), word(this + 0x58), word(this)
        d8 = word(drive + 0x8)
        put(this + 0x4C, r.choice((1, 1, 0)), "<I")  # launch mode
        put(state + 0x4C, r.choice((0.0, 0.5, 1.0, NAN, r.random())))  # clutch
        put(state + 0x48, r.randrange(2), "<I")  # dirty flag
        put(pdef + 0x178, r.choice((0.0, r.uniform(0, 5))))  # ramp rate
        gear = r.randrange(4)
        put(d8 + 0x54C, gear, "<I")
        put(d8 + 0x570, gear if r.random() < 0.6 else r.choice((0, 2)), "<I")  # commanded
        put(d8 + 0x45C, r.choice((NAN, r.uniform(100, 350), r.uniform(0, 600))))  # engine speed (rad/s)

    def fuzz_dispatch(self, mu, this, packet):
        """Random inputs for one FlagDispatch call: the request packet, the VCU flag, the commanded gear, the
        gear count, the lockout and its thresholds. Put back on return, as for GearboxUpdate."""
        r = self.rng
        word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]

        def put(at, value, code="<f"):
            self.saved.append((at, bytes(mu.mem_read(at, 4))))
            mu.mem_write(at, struct.pack(code, value))

        state, drive, pdef = word(this + 0x4), word(this + 0x58), word(this)
        d8, drive_def = word(drive + 0x8), word(drive + 0x4)
        put(this + 0x50, r.choice((0, 1, 1, 1)), "<I")  # VCU enabled
        put(packet + 0x14, r.randrange(8), "<B")  # flags: 1 up, 2 down, 4 set
        put(packet + 0x18, r.choice((0xFFFFFFFF, r.randrange(0, 12), r.randrange(0, 12))), "<I")  # set value
        put(d8 + 0x570, r.randrange(-1, 9) & 0xFFFFFFFF, "<I")  # commanded gear
        put(drive_def + 0x654, r.randrange(3, 11), "<I")  # gear count
        put(state + 0x2C, r.choice((0.0, 0.0, 0.7, r.random())), "<f")  # shift lockout
        put(pdef + 0x158, r.uniform(0, 1))  # lockout after an upshift
        put(pdef + 0x168, r.uniform(0, 1))  # lockout after a downshift

    def fuzz_step(self, mu, this):
        """Random inputs for one StepBasic call: the pedals, the VCU and launch flags, the pedal swap, the fixed
        and launch clutch, the cooldown, the engine, drive and car speeds and the gear. Put back on return."""
        r = self.rng
        word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]

        def put(at, value, code="<f"):
            self.saved.append((at, bytes(mu.mem_read(at, 4))))
            mu.mem_write(at, struct.pack(code, value))

        state, drive, pdef = word(this + 0x4), word(this + 0x58), word(this)
        d8 = word(drive + 0x8)
        put(this + 0xC, r.choice(FUZZ_LEVELS + (r.random(),)))  # throttle
        put(this + 0x10, r.choice(FUZZ_LEVELS + (r.random(),)))  # brake
        put(this + 0x14, r.choice(FUZZ_LEVELS + (r.random(),)))  # handbrake
        put(this + 0x1C, r.uniform(-1, 1))  # fixed clutch
        put(this + 0x4C, r.choice((0, 1, 1)), "<I")  # launch mode
        put(this + 0x50, r.choice((0, 1, 1)), "<I")  # VCU enabled
        put(state + 0x58, r.randrange(2), "<I")  # pedal swap
        put(state + 0x64, r.choice((0.0, r.random() * 0.1, r.uniform(-0.1, 0.1))))  # cooldown
        put(state + 0x4C, r.choice((0.0, 0.5, 1.0, NAN, r.random())))  # launch clutch
        gear = r.randrange(4)
        put(d8 + 0x54C, gear, "<I")  # gear
        put(d8 + 0x570, gear, "<I")  # commanded gear
        put(d8 + 0x45C, r.choice((NAN, r.uniform(100, 350), r.uniform(0, 600))))  # engine speed (rad/s)
        put(d8 + 0x384, r.choice((0.0, r.uniform(0, 60), NAN)))  # drive speed (diff locks)
        put(pdef + 0x188, r.uniform(3000, 9000))  # rev limit
        put(pdef + 0x198, r.choice((0.0, r.random(), 1.0)))  # centre diff handbrake release
        put(pdef + 0x1A8, r.choice((0.0, 0.3, r.random())))  # left-foot brake threshold
        put(word(drive + 0x170) + 0x90, r.randrange(2), "<I")  # diff-lock gate
        for ch in range(3):  # diff-lock channel speeds (the record's +0xc)
            put(d8 + 0x14 * ch + 0xC, r.choice((0.0, 0.5, 1.5, 2.5, -1.2, NAN, r.uniform(-3, 3))))

    def fuzz_slip(self, mu, this):
        """Random inputs for one QuerySlip call: the slip gates, the gear, the gear tables, the rpm limit, the
        engine and the car speed. Put back on return."""
        r = self.rng
        word = lambda at: struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]

        def put(at, value, code="<f"):
            self.saved.append((at, bytes(mu.mem_read(at, 4))))
            mu.mem_write(at, struct.pack(code, value))

        state, drive, pdef = word(this + 0x4), word(this + 0x58), word(this)
        d8, car, drive_def = word(drive + 0x8), word(state + 0x68), word(drive + 0x4)
        put(this + 0x44, r.choice((0, 1, 0x80000000, r.randrange(1 << 32))), "<I")  # slip gate (rpm)
        put(this + 0x48, r.choice((0, 1, 0xFFFFFFFF, r.randrange(1 << 32))), "<I")  # slip gate (speed)
        gear = r.randrange(GEARS)
        put(d8 + 0x54C, gear, "<I")  # gear (the tables have GEARS rows)
        put(d8 + 0x45C, r.choice((NAN, r.uniform(0, 600), r.uniform(100, 350))))  # engine speed (rad/s)
        put(car + 0x15F8, r.choice((NAN, 0.0, 2.7777779, r.uniform(-4, 4), r.uniform(0, 12))))  # car speed
        put(pdef + 0x188, r.uniform(0, 9000))  # rpm limit
        for g in range(GEARS):
            put(drive_def + 0x644 + 0x50 * g, r.choice((0.0, r.uniform(0, 300), NAN)))  # lim
            put(drive_def + 0x694 + 0x50 * g, r.choice((0.0, r.uniform(0.1, 60), NAN)))  # div

    def fuzz_spring_damper(self, mu, addr, args, esp):
        """Vary a reached spring call, then restore its full input/output footprint on return."""
        spec, rng = SPECS[addr], self.rng
        resolved = []
        for win in spec["windows"]:
            at = args[win[0]]
            for off in win[2:]:
                if not at:
                    break
                at = struct.unpack("<I", bytes(mu.mem_read(at + off, 4)))[0]
            length = win[1](args)
            if at and length:
                resolved.append((at, bytes(mu.mem_read(at, length))))
        # Preserve the original windows, including outputs, so a fuzzed call cannot alter the rest
        # of the recorded drive. Any stack arguments changed below are saved the same way.
        self.saved.extend(resolved)

        def random_float():
            return rng.choice((0.0, -0.0, 0.00001, -0.00001, 0.25, -0.25, 1.0, -1.0,
                               float("nan"), float("inf"), float("-inf"), rng.uniform(-4.0, 4.0)))

        stack_float_args = {
            0x52E350: range(4), 0x52E370: range(3), 0x52E390: (0,),
            0x52E3A0: (1,), 0x52E3D0: (1,), 0x52E4C0: (1,), 0x52E4F0: (1,),
            0x52E530: (1,), 0x52E590: (0,), 0x52E5A0: (0,), 0x52E5E0: (0,),
            0x52E630: (2,), 0x52E6D0: (1, 2), 0x52E760: (1, 2, 3),
            0x52E8A0: (1, 2, 3), 0x52E090: (1, 2, 3),
        }.get(addr, ())
        stack_words = spec.get("stack_words", spec["popped"] // 4)
        args = list(args)
        for arg_index in stack_float_args:
            stack_index = arg_index - 1 if spec.get("thiscall") else arg_index
            if not 0 <= stack_index < stack_words:
                continue
            stack_at = esp + 4 + 4 * stack_index
            self.saved.append((stack_at, bytes(mu.mem_read(stack_at, 4))))
            value = random_float()
            mu.mem_write(stack_at, struct.pack("<f", value))
            args[arg_index] = struct.unpack("<I", struct.pack("<f", value))[0]

        if addr == 0x52E090 and len(resolved) > 1:
            # Select stop boundaries and damper speed thresholds so the composite Step branches
            # are reached across fuzz seeds, with occasional NaNs for unordered comparisons.
            definition = resolved[1][0]
            dword = lambda i: struct.unpack("<f", bytes(mu.mem_read(definition + 4 * i, 4)))[0]
            choices = {
                1: (dword(0x1A), dword(0x1E), dword(0x12), 0.0, 0.5, float("nan")),
                2: (dword(0x0A), dword(0x0E), dword(0x0A) - 0.1,
                    dword(0x0E) + 0.1, 0.0, float("nan")),
                3: (0.0, dword(0x3E), dword(0x3E) + 0.1, -dword(0x3A),
                    -dword(0x3A) - 0.1, -0.5, float("nan")),
            }
            for arg_index, options in choices.items():
                stack_at = esp + 4 + 4 * (arg_index - 1)
                # Save again after the general scalar fuzz above: both writes must roll back.
                self.saved.append((stack_at, bytes(mu.mem_read(stack_at, 4))))
                value = rng.choice(options)
                mu.mem_write(stack_at, struct.pack("<f", value))
                args[arg_index] = struct.unpack("<I", struct.pack("<f", value))[0]
        elif addr not in (0x52E090,):
            # All pointed-to spring helper windows are float blocks; preserve pointers and vary
            # only their scalar words (array outputs are rolled back with the saved windows).
            for at, old in resolved:
                for offset in range(0, len(old) - 3, 4):
                    mu.mem_write(at + offset, struct.pack("<f", random_float()))
        return tuple(args)

    def replace_args(self, mu, spec, new, args, esp, own):
        """A spec's "fuzz_args" hook returns {argument index: value} (or None) for this call: each value replaces that
        argument in its stack word before the record takes the arguments. A thiscall's this (index 0, in ecx) can't be
        replaced. The stack word's old bytes go back when the call returns (`own`)."""
        args = list(args)
        for k, value in (new or {}).items():
            stack_index = k - 1 if spec.get("thiscall") else k
            if stack_index < 0:
                raise ValueError("fuzz_args: this (ecx) can't be replaced")
            at = esp + 4 + 4 * stack_index
            own.append((at, bytes(mu.mem_read(at, 4))))
            args[k] = value & 0xFFFFFFFF
            mu.mem_write(at, struct.pack("<I", args[k]))
        return tuple(args)

    def on_entry(self, mu, addr, size, _):
        spec = SPECS[addr]
        esp = mu.reg_read(UC_X86_REG_ESP)
        nargs = spec.get("stack_words", spec["popped"] // 4)  # cdecl inputs are not popped by ret
        args = struct.unpack("<%dI" % nargs, bytes(mu.mem_read(esp + 4, 4 * nargs))) if nargs else ()
        if spec.get("thiscall"):
            args = (mu.reg_read(UC_X86_REG_ECX),) + args
        # The fuzz is for a call the step does not make, so it stays out of a call made inside Step or StepBasic:
        # those run on the step's own values, and the fuzzed call's inputs go back when it returns.
        spring_nested = any(fr["addr"] in SPRING_DAMPER_ADDRS for fr in self.frames)
        fuzzing = self.rng is not None and not any(fr["addr"] in (0x537F50, 0x5380A0, 0x537A80) for fr in self.frames)
        if addr == 0x537160 and fuzzing:  # GearboxUpdate: random inputs before the windows are taken
            self.fuzz_gearbox(mu, args[0])
        elif addr == 0x5375B0 and fuzzing:  # LaunchClutch
            self.fuzz_clutch(mu, args[0])
        elif addr == 0x537A80 and fuzzing:  # FlagDispatch
            self.fuzz_dispatch(mu, args[0], args[1])
        elif addr == 0x5380A0 and fuzzing:  # StepBasic
            self.fuzz_step(mu, args[0])
        elif addr == 0x536FE0 and fuzzing:  # QuerySlip
            self.fuzz_slip(mu, args[0])
        own = []  # a spec's own fuzz ("fuzz" key): this call's replaced bytes go back when it returns
        if addr in SPRING_FUZZ_ADDRS and fuzzing and not spring_nested:
            args = self.fuzz_spring_damper(mu, addr, args, esp)
        if self.rng is not None and spec.get("fuzz") and fuzzing:
            def put(at, value, code="<f"):
                own.append((at, bytes(mu.mem_read(at, struct.calcsize(code)))))
                mu.mem_write(at, struct.pack(code, value))
            spec["fuzz"](mu, args, self.rng, put)
        if self.rng is not None and spec.get("fuzz_args") and fuzzing:
            args = self.replace_args(mu, spec, spec["fuzz_args"](args, self.rng), args, esp, own)
        if addr == 0x537F50 and self.launch:  # Step: the flags are on before its windows, as for GearboxUpdate
            mu.mem_write(args[0] + 0x40, struct.pack("<I", 1))
            mu.mem_write(args[0] + 0x50, struct.pack("<I", 1))
        elif addr == 0x537160 and self.launch:  # GearboxUpdate: switch the flags on before the windows are taken
            mu.mem_write(args[0] + 0x40, struct.pack("<I", 1))
            mu.mem_write(args[0] + 0x50, struct.pack("<I", 1))
        for target in spec.get("dispatch", lambda mu, args: [])(mu, args):
            self.dispatch(mu, *target)
        windows = []
        for k, win in enumerate(spec["windows"]):
            at = args[win[0]]
            for off in win[2:]:  # each offset follows a pointer stored at the previous address
                if not at:
                    break
                at = struct.unpack("<I", bytes(mu.mem_read(at + off, 4)))[0]
            length = win[1](args) if at else 0
            windows.append((k, at, length, bytes(mu.mem_read(at, length)) if length else b""))
        for at, length in spec.get("nodes", lambda mu, a: [])(mu, args):  # (address, length) windows, by the spec
            windows.append((len(windows), at, length, bytes(mu.mem_read(at, length))))
        ret = struct.unpack("<I", bytes(mu.mem_read(esp, 4)))[0]
        fuzzed = fuzzing and (addr in (0x537160, 0x5375B0, 0x537A80, 0x5380A0, 0x536FE0) or
                              (addr in SPRING_FUZZ_ADDRS and not spring_nested))
        self.frames.append(dict(addr=addr, esp=esp, ret=ret, args=args, windows=windows, inner=[], fuzzed=fuzzed,
                                own=own,
                                branch=spec.get("branch", no_branch)(args, windows)))
        if ret not in self.hooked_rets:
            self.hooked_rets.add(ret)
            self.watch[ret] = self.on_return
        if spec.get("result") and fuzzing:
            # "result": the call is not run. Its eax and its writes come from the spec (written at once, so they
            # stay for the callers; the windows and "after" are read as usual), and it returns to its caller here.
            def write(at, value, code="<f"):
                mu.mem_write(at, struct.pack(code, value))
            # A result of None runs the call after all.
            eax = spec["result"](mu, args, self.rng, write)
            if eax is not None:
                mu.reg_write(UC_X86_REG_EAX, eax)
                mu.reg_write(UC_X86_REG_ESP, esp + 4 + pops(spec))
                mu.reg_write(UC_X86_REG_EIP, ret)

    def on_return(self, mu, addr, size, _):
        esp = mu.reg_read(UC_X86_REG_ESP)
        for i in range(len(self.frames) - 1, -1, -1):
            f = self.frames[i]
            if f["ret"] == addr and esp == f["esp"] + 4 + pops(SPECS[f["addr"]]):
                break
        else:
            return  # a return to this address from a frame we did not open
        del self.frames[i + 1:]  # callees that never returned
        # A tail jump (a thunk that jumps to its callee) leaves its own frame with the callee's entry esp and return
        # address: the callee's return ends both, so both are finished here, innermost first.
        group = [self.frames.pop()]
        while self.frames and self.frames[-1]["ret"] == addr and self.frames[-1]["esp"] == group[-1]["esp"]:
            group.append(self.frames.pop())
        for k, f in enumerate(group):
            caller = group[k + 1] if k + 1 < len(group) else (self.frames[-1] if self.frames else None)
            # the callee of a tail jump is part of its thunk's call: the thunk (the outermost) is the caller's record
            self.finish(mu, f, caller, tail_callee=k + 1 < len(group))

    def finish(self, mu, f, caller, tail_callee=False):
        """A listed call returned: its windows after, its eax, and its record in its callers (`caller` is the frame
        it was called from; a tail callee is not an external call of its caller)."""
        spec = SPECS[f["addr"]]
        f["post"] = [bytes(mu.mem_read(at, length)) if length else b"" for _, at, length, _ in f["windows"]]
        if f["fuzzed"]:
            # the fuzzed inputs go back, so the rest of the step runs on the drive's own values (the
            # pedals routed through the step reach code the emulator does not stub)
            for at, old in reversed(self.saved):
                mu.mem_write(at, old)
            self.saved = []
        for at, old in reversed(f.get("own", [])):
            mu.mem_write(at, old)
        f["eax"] = mu.reg_read(UC_X86_REG_EAX)
        # "after": windows read only on return, e.g. an object the call allocates (its returned pointer) or a stack
        # buffer a callee fills: f(mu, args, eax) -> [(address, length)]
        f["after"] = [(at, n, bytes(mu.mem_read(at, n))) for at, n in spec.get("after", lambda mu, a, e: [])(mu, f["args"], f["eax"])]
        sw = mu.reg_read(UC_X86_REG_FPSW)
        top = (sw >> 11) & 7
        f["st0"] = ext_to_f64(mu.reg_read(UC_X86_REG_FP0 + top)) if spec["x87"] else 0.0
        if spec.get("external") and not tail_callee:
            # every open listed call gets it, so a parent (Step) holds its callees' externals in call order; a
            # "direct" frame (a walker) takes only the calls made from its own body (the innermost open frame)
            innermost = self.frames[-1] if self.frames else None
            for open_frame in self.frames:
                if SPECS[open_frame["addr"]].get("external"):
                    continue
                if SPECS[open_frame["addr"]].get("direct") and open_frame is not innermost:
                    continue
                open_frame["inner"].append(f)
        else:
            if caller and SPECS[caller["addr"]].get("direct"):
                caller["inner"].append(f)  # a direct callee of a walker: kept as its inner record, and on its own
            if len(self.calls) < self.limit:
                self.calls.append(f)

GEARS = 10  # gear tables of the Def (CVehicleControlUnit::CDef)
NAN = float("nan")
FUZZ_LEVELS = (0.0, 0.3, 0.5, 0.9, 1.0, NAN)  # pedal values the asm compares against, and NaN


def run_step(snap, limit, launch=False, fuzz=None):
    """One step of the original, recorded. Reruns while x87 results are missing from the oracle."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, launch, fuzz)
        m.run()
        if not m.fpu_missing:
            if m.stop:
                sys.exit("run stopped: %s" % m.stop)
            return m, rec
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def st0_bits(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def pops(spec):
    """The bytes a return takes off the stack: the stack arguments of a stdcall or thiscall callee, none for cdecl."""
    return 0 if spec.get("cdecl") else spec["popped"]


def write(path, calls):
    with open(path, "w") as out:
        for i, c in enumerate(calls):
            out.write("call %d %#x %s\n" % (i, c["addr"], SPECS[c["addr"]]["name"]))
            out.write("args %s\n" % " ".join("%08x" % a for a in c["args"]))
            for (k, at, length, pre), post in zip(c["windows"], c["post"]):
                out.write("win %d %#x %d pre %s\n" % (k, at, length, pre.hex()))
                out.write("win %d %#x %d post %s\n" % (k, at, length, post.hex()))
            for at, n, data in c.get("after", []):
                out.write("awin %#x %d %s\n" % (at, n, data.hex()))
            out.write("ret %08x %016x\n" % (c["eax"], st0_bits(c["st0"])))
            for f in c["inner"]:
                out.write("inner %#x %08x %016x %s\n" % (f["addr"], f["eax"], st0_bits(f["st0"]),
                                                         " ".join("%08x" % a for a in f["args"])))
                # an inner call's own windows (an external spec with "windows" or "after"), right after its line
                for (k, at, length, pre), post in zip(f["windows"], f.get("post", [])):
                    out.write("iwin %d %#x %d %s %s\n" % (k, at, length, pre.hex(), post.hex()))
                for at, n, data in f.get("after", []):
                    out.write("iawin %#x %d %s\n" % (at, n, data.hex()))
            out.write("branch %s\n" % c["branch"])
            out.write("end\n")


def run_tyre_direct(snap, addr, this, seed, limit, fuzz):
    """One tyre leaf called with the fuzzed inputs of seed, started at its entry."""
    oracle = replay.FpuOracle()
    r = random.Random(seed)
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit)
        mu = m.mu
        stack, writes = fuzz(mu, addr, this, r)
        for at, data in writes:
            mu.mem_write(at, data)
        esp = mu.reg_read(UC_X86_REG_ESP)  # Machine put the return address (the sentinel) here
        for i, word in enumerate(stack):
            mu.mem_write(esp + 4 + 4 * i, struct.pack("<I", word & 0xFFFFFFFF))
        if SPECS[addr].get("thiscall"):
            mu.reg_write(UC_X86_REG_ECX, this)
        try:
            mu.emu_start(addr, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn's errors: a stop of this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


READ_PACKET = 0x537F20  # CScratchSim::ReadPacket, called by the car before the step
COPY_DEF = 0x537C90  # CScratchSim::CopyDef, called at car setup (not in a step)
STEP_BASIC = 0x5380A0  # CScratchSim::StepBasic, not called by the car (Step is)
QUERY_SLIP = 0x536FE0  # CScratchSim::QuerySlip, called by FlagDispatch's downshift (not in a step)
SETUP_WORDS = 0xB70 // 4  # the setup window CopyDef reads (its last read ends at +0xb6c)


def run_read_packet(snap, this, fields, seed, limit):
    """ReadPacket with a packet the step does not make: the call is started at its entry, with the scratch
    `this`, and the fuzzed inputs of FlagDispatch (seed). `fields` = throttle, brake, handbrake, clutch, unk18,
    flags, value."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, False, seed)
        mu = m.mu
        packet = m.heap_base + 0x100
        mu.mem_write(packet, struct.pack("<5fII", *fields))
        mu.reg_write(UC_X86_REG_ECX, this)
        esp = mu.reg_read(UC_X86_REG_ESP)  # Machine put the return address (the sentinel) here
        mu.mem_write(esp + 4, struct.pack("<I", packet))
        try:
            mu.emu_start(READ_PACKET, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn's errors: a stop of this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            # the root call returned to the sentinel, which is never executed, so close its frame here
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def run_entry(snap, limit, fuzz=None):
    """The one call a load-time entry dump starts (replay.py): run from its entry, as the original does, and
    recorded like the step's calls. The records are the original's, not ours (the dump's own run is not used).
    `fuzz` (a seed) applies the spec's "fuzz" key to that call, as --fuzz does on the step path."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, fuzz=fuzz)
        m.run()
        if mu_returned(m):
            # the return goes to the sentinel, which is never executed, so close the call's frame here
            rec.on_return(m.mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and not mu_returned(m):
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def mu_returned(m):
    return m.mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL


def run_step_basic(snap, this, dt, seed, limit):
    """StepBasic with the fuzzed inputs of seed: the call is started at its entry, with the scratch `this` and
    the step's dt (the step does not make this call)."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, False, seed)
        mu = m.mu
        mu.reg_write(UC_X86_REG_ECX, this)
        esp = mu.reg_read(UC_X86_REG_ESP)  # Machine put the return address (the sentinel) here
        mu.mem_write(esp + 4, struct.pack("<I", dt))
        try:
            mu.emu_start(STEP_BASIC, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn's errors: a stop of this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def run_query_slip(snap, this, seed, limit):
    """QuerySlip with the fuzzed inputs of seed: the call is started at its entry, with the scratch `this`."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, False, seed)
        mu = m.mu
        mu.reg_write(UC_X86_REG_ECX, this)
        try:
            mu.emu_start(QUERY_SLIP, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn's errors: a stop of this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def run_copy_def(snap, this, words, limit):
    """CopyDef with a setup the step does not make: the call is started at its entry, with the scratch
    `this` and the setup words (SETUP_WORDS of them, at the heap)."""
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = Recorder(m, limit, False, None)
        mu = m.mu
        setup = m.heap_base + 0x1000
        mu.mem_write(setup, struct.pack("<%dI" % SETUP_WORDS, *words))
        mu.reg_write(UC_X86_REG_ECX, this)
        esp = mu.reg_read(UC_X86_REG_ESP)  # Machine put the return address (the sentinel) here
        mu.mem_write(esp + 4, struct.pack("<I", setup))
        try:
            mu.emu_start(COPY_DEF, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn's errors: a stop of this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if not m.fpu_missing:
            if m.stop and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
                sys.exit("run stopped: %s" % m.stop)
            return rec.calls
        oracle.fetch(m.fpu_missing)
    sys.exit("x87 results still missing after 8 runs")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshot")
    ap.add_argument("out")
    ap.add_argument("--max", type=int, default=100000, help="top-level records per run, all listed functions")
    ap.add_argument("--vcu-on", action="store_true", help="turn the launch control and the VCU on for GearboxUpdate")
    ap.add_argument("--fuzz", type=int, default=None, metavar="SEED",
                    help="random GearboxUpdate inputs (implies --vcu-on) or root spring-damper inputs")
    ap.add_argument("--packets", type=int, default=0, metavar="N", help="N ReadPacket calls with random packets (SEED from --fuzz)")
    ap.add_argument("--copy", type=int, default=0, metavar="N", help="N CopyDef calls with random setups (SEED from --fuzz)")
    ap.add_argument("--basic", type=int, default=0, metavar="N", help="N StepBasic calls with random inputs (SEED from --fuzz)")
    ap.add_argument("--set", default="vcu",
                    help="function table to record: vcu, entities, tyre, or spring-damper; several joined with '+' "
                         "record their union (entities+rigid-body)")
    ap.add_argument("--slip", type=int, default=0, metavar="N", help="N QuerySlip calls with random inputs (SEED from --fuzz)")
    ap.add_argument("--tyre-fuzz", type=int, default=0, metavar="N", help="N calls of each tyre leaf (--set tyre) with random inputs (SEED from --fuzz)")
    ap.add_argument("--tyre-fuzz-addr", type=lambda v: int(v, 0), metavar="ADDR", help="--tyre-fuzz for this one function only (e.g. 0x5360c0)")
    a = ap.parse_args()
    global SPECS
    names = a.set.split("+")
    unknown = [n for n in names if n not in SETS]
    if unknown:
        ap.error("unknown set %s (sets: %s)" % (", ".join(unknown), ", ".join(sorted(SETS))))
    SPECS = {addr: spec for n in names for addr, spec in SETS[n].items()}
    snap = replay.Snapshot(a.snapshot)
    if snap.version == 2:  # a load-time entry dump: its one call, from the entry (any set listing that function)
        if snap.step not in SPECS:
            sys.exit("%s is a load-time entry dump of %#x, which --set %s does not list" % (a.snapshot, snap.step, a.set))
        if a.packets or a.copy or a.basic or a.slip or a.tyre_fuzz:
            sys.exit("--packets, --copy, --basic, --slip and --tyre-fuzz need a step snapshot")
        m, rec, calls = None, None, run_entry(snap, a.max, a.fuzz)
    else:
        m, rec = run_step(snap, a.max, a.vcu_on, a.fuzz)
        calls = rec.calls
    if a.packets:
        steps = [c for c in rec.calls if c["addr"] == 0x537F50]  # Step: its this is the scratch
        if not steps:
            sys.exit("no CScratchSim::Step call in the step, so no scratch to read a packet into")
        this = steps[0]["args"][0]
        rng = random.Random(a.fuzz or 0)
        calls = []
        for k in range(a.packets):
            r = rng
            fields = (r.choice(FUZZ_LEVELS + (r.random(),)), r.choice(FUZZ_LEVELS + (r.random(),)),
                      r.choice(FUZZ_LEVELS + (r.random(),)), r.uniform(0, 1), r.uniform(-1, 1),
                      r.randrange(8), r.choice((0xFFFFFFFF, r.randrange(0, 12), r.randrange(0, 12))))
            calls += run_read_packet(snap, this, fields, (a.fuzz or 0) * 1000 + k, a.max)
    if a.copy:
        steps = [c for c in rec.calls if c["addr"] == 0x537F50]  # Step: its this is the scratch
        if not steps:
            sys.exit("no CScratchSim::Step call in the step, so no scratch to copy a setup into")
        this = steps[0]["args"][0]
        rng = random.Random(a.fuzz or 0)
        calls = []
        for k in range(a.copy):
            # a quarter of the words are zero: the tail reads them as flags
            words = [0 if rng.random() < 0.25 else rng.getrandbits(32) for _ in range(SETUP_WORDS)]
            calls += run_copy_def(snap, this, words, a.max)
    if a.basic:
        steps = [c for c in rec.calls if c["addr"] == 0x537F50]  # Step: its this is the scratch, its dt the step's
        if not steps:
            sys.exit("no CScratchSim::Step call in the step, so no scratch to run StepBasic on")
        this, dt = steps[0]["args"][0], steps[0]["args"][1]
        calls = []
        for k in range(a.basic):
            calls += run_step_basic(snap, this, dt, (a.fuzz or 0) * 1000 + k, a.max)
    if a.slip:
        steps = [c for c in rec.calls if c["addr"] == 0x537F50]  # Step: its this is the scratch
        if not steps:
            sys.exit("no CScratchSim::Step call in the step, so no scratch to run QuerySlip on")
        this = steps[0]["args"][0]
        calls = []
        for k in range(a.slip):
            calls += run_query_slip(snap, this, (a.fuzz or 0) * 1000 + k, a.max)
    if a.tyre_fuzz:
        # the scratch: the first CScratch method of the step (CopyFrom's this is the material, not the scratch)
        tyre = FUZZ_SETS["tyre"]
        tyre_calls = [c for c in rec.calls if c["addr"] in tyre["scratch"]]
        if not tyre_calls:
            sys.exit("no CScratch call in the step, so no scratch to fuzz")
        this = tyre_calls[0]["args"][0]
        calls = []
        for k in range(a.tyre_fuzz):
            for addr in ([a.tyre_fuzz_addr] if a.tyre_fuzz_addr else tyre["addrs"]):
                calls += run_tyre_direct(snap, addr, this, (a.fuzz or 0) * 1000 + k, a.max, tyre["fuzz"])
    write(a.out, calls)
    counts = {}
    for c in calls:
        key = (SPECS[c["addr"]]["name"], c["branch"])
        counts[key] = counts.get(key, 0) + 1
    print("%s: step %d, %d calls recorded" % (a.snapshot, snap.step, len(calls)))
    for (name, branch), n in sorted(counts.items()):
        print("  %-28s %-8s %d" % (name, branch, n))
    seen = {c["addr"] for c in calls} | {f["addr"] for c in calls for f in c["inner"]}
    for addr in sorted(SPECS):
        if addr not in seen:
            print("  %s: not reached in this step" % SPECS[addr]["name"])


if __name__ == "__main__":
    main()
