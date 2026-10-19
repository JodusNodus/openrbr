"""Spring-damper vector set for the E48 helper and controller path."""

SET_NAME = "spring-damper"

def spring_leaf_branch(addr, args, windows):
    """Branch labels for the small spring-damper array helpers."""
    if addr == 0x52E400:  # DamperClamp(a[4], b[2])
        a3, b1 = _window_f32(windows, 0, 12), _window_f32(windows, 1, 4)
        return "clamp" if a3 <= b1 and b1 != a3 else "pass"
    if addr == 0x52E460:  # DamperMerge(a[2], b[2])
        a0, b0 = _window_f32(windows, 0, 0), _window_f32(windows, 1, 0)
        total = f32_round(a0 + b0)
        if total > 0.000001:
            return "weighted"
        return "take_a" if b0 <= a0 and a0 != b0 else "sum"
    if addr == 0x52E4C0:
        return "update" if _window_f32(windows, 0, 8) != 0.0 else "zero"
    if addr == 0x52E4F0:
        return "update" if _window_f32(windows, 0, 4) != 0.0 else "zero"
    if addr == 0x52E530:
        return "update" if _window_f32(windows, 0, 0) != 0.0 else "zero"
    if addr == 0x52E5A0:
        return "positive" if not (f32(args[0]) <= 0.0) else "nonpositive"
    if addr == 0x52E5E0:
        return "below_one" if args and f32(args[0]) < 1.0 else "at_or_above_one"
    if addr == 0x52E6D0:
        return "relax" if f32(args[1]) < _window_f32(windows, 0, 0) else "reset"
    if addr == 0x52E760:
        state = windows[0][3]
        if not f32(args[2]) < _window_f32(windows, 0, 8):
            return "idle"
        ratio = _ieee_div(_window_f32(windows, 0, 8), f32(args[2]))
        if ratio >= 1.0:
            reciprocal = _ieee_div(1.0, ratio)
            ratio = _ieee_div(1.0, (1.28571427 - reciprocal * 0.285714298) * reciprocal)
        else:
            ratio = (1.28571427 - ratio * 0.285714298) * ratio
        w = (1.0 - _ieee_div(1.0, ratio)) * struct.unpack_from("<f", state)[0]
        if f32(args[1]) < w:
            return "release" if f32(args[1]) < struct.unpack_from("<f", state)[0] else "clear"
        return "relax" if w > 0.0 else "idle"
    return no_branch(args, windows)


def spring_step_branch(args, windows):
    """Summarize the travel, damper-speed and stop-rate paths selected by CSpringDamper::Step."""
    x, y, z = (f32(v) for v in args[1:4])
    definition = windows[1][3]

    def d(i):
        return struct.unpack_from("<f", definition, 4 * i)[0]

    travel = f32_round(f32_round(d(0x12) - f32_round(x - d(0x1A))) * d(0x16))
    threshold = f32_round(f32_round(d(0x1E) - d(0x1A)) * d(0x22))
    travel_mode = "direct" if threshold <= travel else "compound"
    if 0.0 <= z:
        velocity = "positive_fast" if d(0x3E) <= z and z != d(0x3E) and d(0x3E) != 0.0 else "positive"
    else:
        low_speed_edge = f32_round(0.0 - d(0x3A))
        velocity = "negative_fast" if z < low_speed_edge and d(0x3A) != 0.0 else "negative"
    if y < d(0x0A):
        stop = "droop_positive" if 0.0 <= z else "droop_negative"
    elif d(0x0E) < y:
        stop = "bump_positive" if 0.0 < z else "bump_nonpositive"
    else:
        stop = "within_stops"
    return "%s_%s_%s" % (travel_mode, velocity, stop)


def spring_branch_for(addr):
    if addr == 0x52E090:
        return spring_step_branch
    if addr in (0x52E400, 0x52E460, 0x52E4C0, 0x52E4F0, 0x52E530, 0x52E5A0,
                0x52E5E0, 0x52E6D0, 0x52E760):
        return lambda args, windows: spring_leaf_branch(addr, args, windows)
    return no_branch


SPECS = {
    # cdecl math leaves: stack_words is the number of input words; popped is the ret cleanup.
    0x52E350: dict(name="DamperStep1", popped=0, stack_words=4, x87=True, branch=no_branch, windows=[]),
    0x52E370: dict(name="DamperStep2", popped=0, stack_words=3, x87=True, branch=no_branch, windows=[]),
    0x52E390: dict(name="DamperBiasSub", popped=0, stack_words=1, x87=True, branch=no_branch, windows=[]),
    0x52E3A0: dict(name="DamperAccumA", popped=0, stack_words=3, x87=False, branch=no_branch,
                   windows=[(0, lambda a: 8), (2, lambda a: 4)]),
    0x52E3D0: dict(name="DamperAccumB", popped=0, stack_words=3, x87=False, branch=no_branch,
                   windows=[(0, lambda a: 4), (2, lambda a: 8)]),
    0x52E400: dict(name="DamperClamp", popped=0, stack_words=2, x87=True, branch=spring_branch_for(0x52E400),
                   windows=[(0, lambda a: 16), (1, lambda a: 8)]),
    0x52E460: dict(name="DamperMerge", popped=0, stack_words=2, x87=False, branch=spring_branch_for(0x52E460),
                   windows=[(0, lambda a: 8), (1, lambda a: 8)]),
    0x52E4C0: dict(name="DamperUpdateA", popped=0, stack_words=2, x87=False,
                   branch=spring_branch_for(0x52E4C0), windows=[(0, lambda a: 16)]),
    0x52E4F0: dict(name="DamperUpdateB", popped=0, stack_words=2, x87=False,
                   branch=spring_branch_for(0x52E4F0), windows=[(0, lambda a: 16)]),
    0x52E530: dict(name="DamperUpdateC", popped=0, stack_words=2, x87=False,
                   branch=spring_branch_for(0x52E530), windows=[(0, lambda a: 16)]),
    0x52E590: dict(name="DamperScale", popped=0, stack_words=1, x87=True, branch=no_branch, windows=[]),
    0x52E5A0: dict(name="DamperSqrt4", popped=0, stack_words=1, x87=True,
                   branch=spring_branch_for(0x52E5A0), windows=[]),
    0x52E5E0: dict(name="DamperCurve", popped=0, stack_words=1, x87=True,
                   branch=spring_branch_for(0x52E5E0), windows=[]),
    0x52E630: dict(name="DamperCombine", popped=0, stack_words=3, x87=False, branch=no_branch,
                   windows=[(0, lambda a: 16), (1, lambda a: 16)]),
    0x52E6D0: dict(name="DamperRelax", popped=0, stack_words=3, x87=False,
                   branch=spring_branch_for(0x52E6D0), windows=[(0, lambda a: 16)]),
    0x52E760: dict(name="DamperSolve", popped=0, stack_words=4, x87=True,
                   branch=spring_branch_for(0x52E760), windows=[(0, lambda a: 16)]),
    0x52E8A0: dict(name="DamperForce", popped=0, stack_words=4, x87=False, branch=no_branch,
                   windows=[(0, lambda a: 16)]),
    # Ctor/accessor calls are recorded from RBR_SNAPAT load-time entry dumps.
    0x52D710: dict(name="CSetupOptions::Unk1C", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 4)]),
    0x52D730: dict(name="CDef::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch, windows=[]),
    0x52D750: dict(name="CDef::CDef", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x11C)]),
    0x52D9E0: dict(name="CSetup::Unk20", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 4)]),
    0x52DA40: dict(name="CState::SetUnk10", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x3C)]),
    0x52DA50: dict(name="CState::CState", popped=0xC, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x3C)]),
    0x52DBD0: dict(name="CSpringDamper::CopyStateToDef", popped=4, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x30), (0, lambda a: 0x11C, 4), (1, lambda a: 0x9C)]),
    0x52DD70: dict(name="CSpringDamper::UpdateForces", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x30), (0, lambda a: 0x11C, 4), (0, lambda a: 0x3C, 8),
                            (0, lambda a: 8, 0x10)]),
    0x52DF20: dict(name="CSpringDamper::CSpringDamper", popped=0xC, x87=False, thiscall=True,
                   branch=no_branch, windows=[(0, lambda a: 0x30), (1, lambda a: 0x11C if a[1] else 0),
                                              (2, lambda a: 0x3C if a[2] else 0)]),
    0x52E010: dict(name="CSpringDamper::ResetOutputs", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 0x30), (0, lambda a: 0x3C, 8), (0, lambda a: 0xEB8, 8, 0x18),
                            (0, lambda a: 0x14, 0x1C)]),
    0x52E090: dict(name="CSpringDamper::Step", popped=0x18, x87=False, thiscall=True,
                   branch=spring_step_branch,
                   windows=[(0, lambda a: 0x30), (0, lambda a: 0x11C, 4), (0, lambda a: 0x3C, 8),
                            (0, lambda a: 8, 0x10), (4, lambda a: 4), (5, lambda a: 4), (6, lambda a: 4)]),
    0x52D960: dict(name="CU32Value::Clear", popped=0, x87=False, thiscall=True, branch=no_branch,
                   windows=[(0, lambda a: 8)]),
    # Emit is nested by ResetOutputs; keep its return and arguments in that function's inner records.
    0x52DE60: dict(name="CDamperSignal::Emit", popped=4, x87=False, thiscall=True, external=True,
                   branch=no_branch, windows=[]),
}

SPRING_FUZZ_ADDRS = {
    0x52E350, 0x52E370, 0x52E390, 0x52E3A0, 0x52E3D0, 0x52E400, 0x52E460,
    0x52E4C0, 0x52E4F0, 0x52E530, 0x52E590, 0x52E5A0, 0x52E5E0, 0x52E630,
    0x52E6D0, 0x52E760, 0x52E8A0, 0x52E090,
}
for _address in SPRING_FUZZ_ADDRS:
    SPECS[_address]["fuzz_call"] = True
