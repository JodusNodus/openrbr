"""Engine and driveline vector set (rust/rbr-physics/src/engine_driveline, tests/engine_driveline_vectors.rs).

Run: .venv/bin/python -I harness/vcu_vectors.py SNAPSHOT OUT.txt --set engine-driveline
Each entry is one function of src/rbr RBR_E39-E43; the plan and the groups are in
rust/rbr-physics/src/engine_driveline/README.md (git 9c28c0b8). Windows follow the rules in vcu_vectors.py's docstring.
"""

SET_NAME = "engine-driveline"

# The windows of a CEngine call: the engine's own fields (this+0x14 pedal, this+0x194), the definition, the state
# (the whole of layout::EngineState) and the option block. Pointers: this+4 def, this+8 state, this+0x48 options.
ENGINE_WINDOWS = [(0, lambda a: 0x198), (0, lambda a: 0x2830, 4), (0, lambda a: 0x274, 8), (0, lambda a: 0x80, 0x48)]
# SimE660 reads the def's threshold at +0x286c, past the 0x2830 bytes the other kernels read.
DEF_END = 0x2880
ENGINE_WINDOWS_660 = [(0, lambda a: 0x198), (0, lambda a: DEF_END, 4), (0, lambda a: 0x274, 8),
                      (0, lambda a: 0x80, 0x48)]

NAN = float("nan")
SIGNALS = 0x893600  # the global signal table; its [SIGNALS] + 0x3cb8 is the engine's emit object
INF = float("inf")


def table_lerp_branch(args, windows):
    """E41_TableLerp (0x50d580): 'neg' (f < 0), 'big' (f >= 2^31), 'nan' (f is NaN: the x86 cvttss2si result
    0x80000000 takes the 'i_lo' path), 'i_hi' (i > n - 2), 'i_lo' (i < 0), 'interp'. The f32 operation order is
    ((x - lo) * (n - 1)) / (hi - lo), each step rounded to f32, as in table_branch."""
    x, lo, hi, n = f32(args[1]), f32(args[2]), f32(args[3]), signed(args[4])
    f = f32_round(f32_round(f32_round(x - lo) * float(n - 1)) / f32_round(hi - lo))
    if f != f:
        return "nan"
    if f < 0.0:
        return "neg"
    if f >= F32_TWO31:
        return "big"
    i = int(f)
    if i > n - 2:
        return "i_hi"
    if i < 0:
        return "i_lo"
    return "interp"


def unkd6f0_branch(args, windows):
    """E41_UnkD6F0 (0x50d6f0): 'zero' (x == 0), 'high' (x > 0.5), 'low'."""
    x = f32(args[0])
    if x == 0.0:
        return "zero"
    if 0.5 < x:
        return "high"
    return "low"


def blend_w_branch(args, windows):
    """E41_BlendW (0x50d220): 'short' (two knots or fewer, no search), else the search's exit: 'hit' (x <= knot i),
    'end' (no knot took x; i = count - 1). The weights: 'in' (0 <= h <= 1), 'below' (h < 0), 'above' (h > 1), 'nan'."""
    count = struct.unpack_from("<I", windows[0][3], 0)[0]
    keys = [struct.unpack_from("<f", windows[1][3], 0xC * k)[0] for k in range(count)] if count else []
    x = f32(args[1])
    if count <= 2:
        search = "short"
        i = 1
    else:
        i = 1
        while True:
            if x <= keys[i] or x != x:
                break
            i += 1
            if i >= count - 1:
                break
        search = "hit" if x <= keys[i] or x != x else "end"
    m = i - 1
    g = f32_round(1.0 - f32_round(f32_round(x - keys[m]) / f32_round(keys[i] - keys[m])))
    h = f32_round(1.0 - g)
    weight = "nan" if h != h else ("above" if h > 1.0 else ("below" if h < 0.0 else "in"))
    return search + "/" + weight


def sim410_branch(args, windows):
    """CEngine::Sim410 (0x50d410): 'gate' (the option gate is set and the speed is above 0: the value is returned),
    'off' (returns 0). The stored value is written either way. Windows 2 and 3 are the state and the options."""
    speed = struct.unpack_from("<f", windows[2][3], 0x188)[0]
    gate = struct.unpack_from("<f", windows[3][3], 0x58)[0]
    return "gate" if gate != 0.0 and 0.0 < speed else "off"


def curve_eta_branch(args, windows):
    """E41_CurveEta (0x50d7a0): 'below' (x < the first key), 'above' (x past the last key), 'search' (between).
    Window 0 is the curve (records of 0x20 bytes, keys at +0x0c), so the keys come from its bytes."""
    t, x, n = args[0], f32(args[1]), signed(args[2])
    curve = windows[0][3]
    key = lambda k: struct.unpack_from("<f", curve, 0x20 * k + 0x0C)[0]
    if x < key(0):
        return "below"
    last = key(n - 1)
    if last <= x and x != last:
        return "above"
    return "search"


def sime660_branch(args, windows):
    """CEngine::SimE660 (0x50e660): the load gate (abs value above 1e-5), the speed against 100 rad/s, and the rate's
    sign ('zero' and 'nan' take the zero branch of the result)."""
    state = windows[2][3]
    f = lambda off: struct.unpack_from("<f", state, off)[0]
    rate = f(0x184)
    sign = "nan" if rate != rate else ("pos" if rate > 0.0 else ("neg" if rate < 0.0 else "zero"))
    gate = "gate" if f(0x24C) > 0.00001 else "nogate"
    return gate + "/" + ("fast" if 100.0 < f(0x188) else "slow") + "/" + sign


def bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def fuzz_pool(r, scale):
    """A fuzzed float: a branch edge (0, the NaN, an infinity) or a random one of `scale`."""
    return r.choice((0.0, 1.0, -1.0, 0.5, NAN, INF, r.uniform(-scale, scale), r.uniform(-scale, scale)))


def words_f32(values):
    return [bits(v) for v in values]


# Direct-call recipes ("direct_fuzz", see harness/direct_calls.py): recipe(mu, heap, r) -> (stack words,
# [(address, bytes)] to write first, this). The heap is the replay's stub heap (mapped, writable, free), so the
# tables and objects are placed there. this is None for a stdcall function.
def fuzz_table_lerp(mu, heap, r):
    n = r.randrange(2, 12)
    t = heap + 0x100
    table = [fuzz_pool(r, 10.0) for _ in range(n)]
    span = r.choice((r.uniform(-5, 10), 1.0, r.uniform(0.1, 3.0), 1e-9))
    # a 1e-9 span is the 'big' edge: near zero, where it is still representable in f32 (at |lo| ~ 10 it rounds to
    # an exact zero span, and the emulator's division stops with no x87 result)
    lo = r.uniform(-1e-3, 1e-3) if span == 1e-9 else r.uniform(-10, 10)
    hi = lo + span
    # the fraction of the span: below lo (neg), inside (interp), past the last knot (i_hi), big, or a pool edge
    x = r.choice((lo + r.uniform(-0.2, 1.0) * (hi - lo), lo + r.uniform(-0.2, 1.0) * (hi - lo),
                  lo + r.uniform(0.0, 1.0) * (hi - lo) * 1e9, lo - 1.0, hi + 1.0, fuzz_pool(r, 10.0)))
    writes = [(t + 4 * k, struct.pack("<f", v)) for k, v in enumerate(table)]
    return [t, bits(x), bits(lo), bits(hi), n], writes, None


def fuzz_unkd6f0(mu, heap, r):
    x = r.choice((0.0, 0.25, 0.5, 0.75, 1.0, -0.3, NAN, r.uniform(-1, 2)))
    return [bits(x), heap + 0x400], [], None


def fuzz_curve_eta(mu, heap, r):
    n = r.randrange(2, 12)
    t = heap + 0x800
    key, writes = 0.0, []
    for k in range(n + 1):
        key += r.uniform(0.1, 2.0)
        writes.append((t + 0x20 * k + 0x0C, struct.pack("<f", key)))
        writes.append((t + 0x20 * k + 0x1C, struct.pack("<f", r.uniform(-5, 5))))
        writes.append((t + 0x20 * k + 0x00, struct.pack("<f", r.uniform(-5, 5))))
    x = r.choice((-1.0, key + 1.0, r.uniform(0, key), fuzz_pool(r, key)))
    return [t, bits(x), n], writes, None


def fuzz_blend_w(mu, heap, r):
    count = r.randrange(2, 9)
    t, recs, out = heap + 0x1000, heap + 0x1100, heap + 0x1200
    keys = sorted(r.uniform(-5, 5) for _ in range(count))
    x = r.choice((keys[0] - 1.0, keys[-1] + 1.0, r.uniform(keys[0], keys[-1]), keys[r.randrange(count)],
                  fuzz_pool(r, 5.0)))
    writes = [(t, struct.pack("<I", count)), (t + 4, struct.pack("<I", recs))]
    for k, key in enumerate(keys):
        writes.append((recs + 0xC * k, struct.pack("<f", key)))
        writes.append((recs + 0xC * k + 4, struct.pack("<f", r.uniform(-1, 1))))
        writes.append((recs + 0xC * k + 8, struct.pack("<f", r.uniform(-1, 1))))
    return [t, bits(x), out, out + 4, out + 8, out + 12], writes, None


def fuzz_engine(mu, heap, r):
    """A CEngine with its three objects (this+4 def, this+8 state, this+0x48 options) in the stub heap. The
    fields the ported kernels read are drawn from the edges of their branches."""
    this, def_at, state_at, opt_at = heap + 0x3000, heap + 0x4000, heap + 0x8000, heap + 0xA000
    writes = [(this + 4, struct.pack("<I", def_at)), (this + 8, struct.pack("<I", state_at)),
              (this + 0x48, struct.pack("<I", opt_at))]
    for at in range(def_at, def_at + 0x2830, 4):
        writes.append((at, struct.pack("<f", r.uniform(-2, 2))))
    for at in range(state_at, state_at + 0x274, 4):
        writes.append((at, struct.pack("<f", r.uniform(-1, 1))))
    for at in range(opt_at, opt_at + 0x80, 4):
        writes.append((at, struct.pack("<f", r.uniform(-1, 1))))
    writes.append((def_at + 0x282C, struct.pack("<f", r.choice((0.0, -0.0, 1.0, r.uniform(-2, 2), NAN)))))
    writes.append((state_at + 0x188, struct.pack("<f", r.choice((0.0, -0.0, 1.0, -30.0, 120.0, NAN, r.uniform(-50, 300))))))
    writes.append((opt_at + 0x58, struct.pack("<f", r.choice((0.0, 0.0, 1.0, -2.0)))))
    return [], writes, this


def drive_fuzz_objects(heap, r):
    """A CDrive with its definition (this+4, the gear count at +0x654) and state (this+8, the commanded gear at +0x570,
    the gear at +0x54c) in the stub heap. Returns (drive, writes)."""
    drive, def_at, state_at = heap + 0x3000, heap + 0x4000, heap + 0x8000
    count = r.randrange(1, 12)
    writes = [(drive + 4, struct.pack("<I", def_at)), (drive + 8, struct.pack("<I", state_at)),
              (def_at + 0x654, struct.pack("<I", count)),
              (state_at + 0x54C, struct.pack("<i", r.randrange(-1, 12))),
              (state_at + 0x570, struct.pack("<i", r.choice((0, r.randrange(-2, 12))))),
              (state_at + 0x564 + 8, struct.pack("<I", r.randrange(2)))]
    return drive, writes


def fuzz_set47b0(mu, heap, r):
    """CDrive::Set47B0 (v): the commanded gear moves by v and is clamped to 0..count-1."""
    drive, writes = drive_fuzz_objects(heap, r)
    return [r.randrange(-4, 5) & 0xFFFFFFFF], writes, drive


def fuzz_get4530(mu, heap, r):
    """CDrive::Get4530: the gear (the state's +0x54c), returned as an int."""
    drive, writes = drive_fuzz_objects(heap, r)
    return [], writes, drive


def fuzz_set_two(mu, heap, r):
    """CDrive::SetTwo (a, x, y): entry a (0..3, 0x10 bytes apart at this), its +0xe8 and +0x128 sub-objects dirtied and
    given x and y at +0xc."""
    values = [r.choice((NAN, 0.0, -0.0, r.uniform(-4, 4))) for _ in range(2)]
    return [r.randrange(4)] + [bits(v) for v in values], [], heap + 0x3000


def fuzz_set4570(mu, heap, r):
    """CDrive::Set4570 (channel, value): the channel's lock record (ratio at +0xc, lock at +0x18), its maximum torque in
    the def (+0x58 of the 0x60-byte entry) and the worker's enable word (+0x90 of the object at this+0x170, zero about
    one call in four). The ratios cover the band edges and NaN, the values the clamp edges and NaN."""
    drive, writes = drive_fuzz_objects(heap, r)
    worker = heap + 0x9000
    writes.append((drive + 0x170, struct.pack("<I", worker)))
    writes.append((worker + 0x90, struct.pack("<I", r.choice((0, 0, 0, r.randrange(1, 4))))))
    for k in range(3):
        put_f32(writes, heap + 0x4000 + 0x60 * k + 0x58, r.choice((0.0, r.uniform(-50, 300), NAN)))
        put_f32(writes, heap + 0x8000 + 0x14 * k + 0xC,
                r.choice((-2.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, NAN, r.uniform(-3, 3))))
    value = r.choice((-0.5, 0.0, 0.3, 1.0, 1.5, NAN, r.uniform(-1, 2)))
    return [r.randrange(3), bits(value)], writes, drive


def fuzz_drive_record(mu, heap, r):
    """CDrive::M6DB0 and CDrive::M6DD0 (a): the floats at +0x64 and +0x54 of record a (0xc8 bytes) of the state (this+8)."""
    drive, writes = drive_fuzz_objects(heap, r)
    for k in range(4):
        for at in (0x54, 0x64):
            put_f32(writes, heap + 0x8000 + 0xC8 * k + at, r.choice((NAN, 0.0, -0.0, r.uniform(-4, 4))))
    return [r.randrange(4)], writes, drive


def fuzz_test4840(mu, heap, r):
    """CDrive::Test4840: the gear scales from the second gear (against 0.2, NaN too), the worker's enable, ready and speed
    term (against 0.5) and torque gain, and the car's engine (SimE980 reads its state's float at +0x188 against 40)."""
    drive, writes = drive_fuzz_objects(heap, r)
    state, worker, car, engine_state = heap + 0x8000, heap + 0x9000, heap + 0xC000, heap + 0xD000
    writes += [(drive + 0x170, struct.pack("<I", worker)), (state + 0x580, struct.pack("<I", car)),
               (car + 0x698, struct.pack("<I", engine_state)),
               (worker + 0x78, struct.pack("<I", r.choice((0, 1, 1)))),
               (worker + 0x7C, struct.pack("<I", r.choice((0, 1, 1))))]
    for i in range(12):
        put_f32(writes, worker + 0x48 + 4 * i, r.choice((0.0, 0.2, 0.6, 1.0, NAN, r.uniform(-1, 1))))
    put_f32(writes, worker + 0x84, r.choice((0.0, 0.2, 0.5, -1.0, NAN)))
    put_f32(writes, worker + 0x8C, r.choice((0.0, 0.4, 0.5, 1.0, -1.0, NAN)))
    put_f32(writes, engine_state + 0x188, r.choice((30.0, 50.0, NAN)))
    return [], writes, drive


def fuzz_sim8cf0(mu, heap, r):
    """E43EngineSim::Sim8CF0: the state's float at +0x200, the engine's +0x184, and the options block's four words (the
    half-sum crosses 0 and 1 and NaN). The global signal object (0x893600) points at a stub object whose +4 is zero, so
    its emit returns at once, with eax 0."""
    this, writes = engine_objects(heap, r)
    put_f32(writes, this + 0x184, r.uniform(-1, 1))
    signal_objects = heap + 0x20000
    writes += [(SIGNALS, struct.pack("<I", signal_objects)), (signal_objects + 0x3CB8 + 4, struct.pack("<I", 0))]
    return [], writes, this


def fuzz_mgr_b_word8(mu, heap, r):
    """E39_MgrB 0x501fa0: the word at this+8."""
    this = heap + 0x3000
    return [], [(this + 8, struct.pack("<I", r.getrandbits(32)))], this


def fuzz_mgr_b_idx(mu, heap, r):
    """E39_MgrB 0x501fb0 and 0x502060 (idx): the table pointer at this+0xc, any index (the entry is only addressed)."""
    this = heap + 0x3000
    return [r.choice((0, 1, 2, 7, r.getrandbits(32)))], [(this + 0xC, struct.pack("<I", r.getrandbits(32)))], this


def fuzz_copy_row(mu, heap, r):
    """E39_MgrD 0x502460 (idx, out): a table of sixteen rows of four random words at this, and the output at heap+0x6000."""
    table, out = heap + 0x3000, heap + 0x6000
    writes = [(table + 4 * k, struct.pack("<I", r.getrandbits(32))) for k in range(64)]
    return [r.randrange(16), out], writes, table


def engine_objects(heap, r):
    """A CEngine with its def, state and option block in the stub heap, every field the kernels read drawn at random
    (the state, the options and the first DEF_END bytes of the def). Returns (this, writes)."""
    this, def_at, state_at, opt_at = heap + 0x3000, heap + 0x4000, heap + 0x8000, heap + 0xA000
    writes = [(this + 4, struct.pack("<I", def_at)), (this + 8, struct.pack("<I", state_at)),
              (this + 0x48, struct.pack("<I", opt_at))]
    for at in range(def_at, def_at + DEF_END, 4):
        writes.append((at, struct.pack("<f", r.uniform(-2, 2))))
    for at in range(state_at, state_at + 0x274, 4):
        writes.append((at, struct.pack("<f", r.uniform(-1, 1))))
    for at in range(opt_at, opt_at + 0x80, 4):
        writes.append((at, struct.pack("<f", r.uniform(-1, 1))))
    writes.append((this + 0x14 + 8, struct.pack("<I", r.randrange(2))))
    writes.append((this + 0x194, struct.pack("<f", r.uniform(-1, 1))))
    return this, writes


def put_f32(writes, at, value):
    writes.append((at, struct.pack("<f", value)))


def fuzz_set_scratch08(mu, heap, r):
    this, writes = engine_objects(heap, r)
    return [r.getrandbits(32)], writes, this


def fuzz_crank_tick(mu, heap, r):
    """SimE370 (a): the crank angle near the wrap, and a small or a large step, either sign."""
    this, writes = engine_objects(heap, r)
    state = heap + 0x8000
    put_f32(writes, state + 0x178, r.choice((0.0, 6.0, 12.5, -0.5, r.uniform(0, 12.6), r.uniform(-1, 13))))
    writes.append((state + 0x17C, struct.pack("<I", r.randrange(256))))
    writes.append((state + 0x180, struct.pack("<I", r.randrange(256))))
    a = r.choice((0.0, 0.05, -0.05, 1.0, -1.0, 6.3, -6.3, r.uniform(-7, 7), r.uniform(-0.2, 0.2)))
    b_at, c_at = heap + 0x9000, heap + 0x9010
    return [bits(a), b_at, c_at], writes, this


def fuzz_rev_gate(mu, heap, r):
    """SimE470: the speed (rad/s) around 500 rpm and 3500 rpm, the pedal and the gate."""
    this, writes = engine_objects(heap, r)
    state, opt = heap + 0x8000, heap + 0xA000
    put_f32(writes, state + 0x188, r.choice((0.0, 52.4, 366.5, 500.0, 1000.0, r.uniform(0, 1000), NAN)))
    put_f32(writes, this + 0x20, r.choice((0.0, 0.5, 1.0, -0.2, 1.2, r.uniform(-1, 2))))
    writes.append((opt + 0x74, struct.pack("<I", r.randrange(2))))
    return [], writes, this


def fuzz_compliance(mu, heap, r):
    """SimE9F0: the stage fractions, the enable flag, the stage ends."""
    this, writes = engine_objects(heap, r)
    def_at, opt = heap + 0x4000, heap + 0xA000
    for k in range(4):
        put_f32(writes, opt + (0xC, 0x1C, 0x2C, 0x3C)[k], r.choice((0.0, 0.5, 1.0, r.uniform(0, 1))))
        lo = r.uniform(0.5, 5)
        put_f32(writes, def_at + (0x118, 0x158, 0x198, 0x1D8)[k], lo)
        put_f32(writes, def_at + (0x128, 0x168, 0x1A8, 0x1E8)[k], lo + r.uniform(0.1, 5))
    put_f32(writes, opt + 0x70, r.choice((0.0, 1.0, r.uniform(0, 1))))
    put_f32(writes, def_at + 0xD8, r.uniform(0.5, 5))
    put_f32(writes, def_at + 0xE8, r.uniform(0.5, 5))
    writes.append((def_at + 0x26F8, struct.pack("<I", r.randrange(2))))
    return [], writes, this


def fuzz_exhaust(mu, heap, r):
    """SimEC40 (i, q): the row, and the inputs q (12 floats; q[2] picks the first form)."""
    this, writes = engine_objects(heap, r)
    q = heap + 0x9100
    for k in range(12):
        writes.append((q + 4 * k, struct.pack("<f", r.choice((0.0, 1.0, r.uniform(-2, 2))) if k != 2
                                              else r.choice((0.0, 1.0)))))
    return [r.randrange(8), q], writes, this


def fuzz_stall(mu, heap, r):
    """SimECF0: the speed around 40 rad/s, the two gates and the four terms around 0.7."""
    this, writes = engine_objects(heap, r)
    state, opt = heap + 0x8000, heap + 0xA000
    put_f32(writes, state + 0x188, r.choice((30.0, 40.0, 45.0, r.uniform(0, 60))))
    put_f32(writes, opt + 0x58, r.choice((0.0, 0.1, 0.2, r.uniform(0, 1))))
    put_f32(writes, opt + 0x5C, r.choice((0.0, 0.1, 0.2, r.uniform(0, 1))))
    for off in (0x8, 0x18, 0x28, 0x38):
        put_f32(writes, opt + off, r.choice((0.5, 0.7, 0.8, 0.9, r.uniform(0, 1))))
    return [], writes, this


def fuzz_torque(mu, heap, r):
    """SimED70 (x, u): x, and the q words that pick the curve (q[2], q[3]) and scale it (q[4], q[5])."""
    this, writes = engine_objects(heap, r)
    q = heap + 0x9200
    for k in range(8):
        writes.append((q + 4 * k, struct.pack("<f", r.uniform(-2, 2))))
    writes.append((q + 8, struct.pack("<f", r.choice((0.0, 1.0)))))
    writes.append((q + 12, struct.pack("<f", r.choice((0.0, 1.0)))))
    return [bits(r.uniform(-3, 3)), q], writes, this


def fuzz_sime660(mu, heap, r):
    """SimE660 (a): the rate's sign, the speed on both sides of 100 rad/s, the loads over and under their denominators
    (the zero denominator too), and the threshold term's value on both sides of its threshold."""
    this, writes = engine_objects(heap, r)
    state, opt, dfn = heap + 0x8000, heap + 0xA000, heap + 0x4000
    threshold = r.uniform(-1, 1)
    put_f32(writes, state + 0x184, r.choice((0.0, -0.0, 1.0, -1.0, NAN, r.uniform(-3, 3))))
    put_f32(writes, state + 0x24C, r.choice((0.0, 0.000005, 0.5, 2.0, NAN, r.uniform(0, 5))))
    put_f32(writes, state + 0x188, r.choice((0.0, 50.0, 100.0, 100.5, 400.0, NAN, r.uniform(0, 500))))
    for off in (0xBC, 0xF4, 0x12C, 0x164):
        put_f32(writes, state + off, r.choice((0.0, 1.0, -1.0, 40.0, NAN, r.uniform(-200, 200))))
    for off in (0x138, 0x178, 0x1B8, 0x1F8):
        put_f32(writes, dfn + off, r.choice((0.0, 1.0, 40.0, -5.0, NAN, r.uniform(-100, 100))))
    put_f32(writes, dfn + 0x284C, r.choice((0.0, 1.0, -2.0, r.uniform(-10, 10))))
    put_f32(writes, dfn + 0x285C, r.choice((0.0, 0.5, -1.0, r.uniform(-10, 10))))
    put_f32(writes, dfn + 0x286C, threshold)
    put_f32(writes, state + 0x23C, r.choice((threshold, threshold + 0.5, threshold - 0.5, NAN, r.uniform(-2, 2))))
    for k in range(4):
        put_f32(writes, opt + 0x14 + 0x10 * k, r.choice((0.0, 0.5, -0.5, r.uniform(-1, 1))))
    return [bits(r.choice((0.0, 0.01, 0.05, r.uniform(0, 0.1))))], writes, this


def sime_d870_branch(args, windows):
    """CEngine::SimD870 (0x50d870): the roll's gate (options +4) and whether the slot's stall term is open to a roll
    (below 1, and not exactly 1)."""
    opt = windows[2][3]
    gate = struct.unpack_from("<I", opt, 4)[0]
    term = struct.unpack_from("<f", opt, 8 + 0x10 * args[1])[0]
    if not gate:
        return "gate-off"
    return "open" if term <= 1.0 and term != 1.0 else "closed"


def fuzz_d870(mu, heap, r):
    """SimD870 (i < 4): the gate, the slot's stall term at the roll's edges, the threshold and the slot loads, the
    table entry and the LCG word (the options block points at it)."""
    i = r.randrange(4)
    this, writes = engine_objects(heap, r)
    state, opt, seed = heap + 0x8000, heap + 0xA000, heap + 0xB000
    writes.append((opt, struct.pack("<I", seed)))
    writes.append((seed, struct.pack("<I", r.getrandbits(32))))
    writes.append((opt + 4, struct.pack("<I", 1 if r.random() < 0.9 else 0)))
    # the stall term at 0.1 or more makes the fraction's denominator 3 * base; with slot_b large the fraction is 1
    put_f32(writes, opt + 8 + 0x10 * i, r.choice((0.0, 0.05, 0.5, 0.95, 0.5, 0.95, 1.0, 1.02, NAN, r.uniform(-0.2, 1.5))))
    put_f32(writes, state + 0x23C, r.choice((600.0, 500.0, 700.0, 300.0, NAN, r.uniform(0, 1000))))
    put_f32(writes, state + 0x94 + 0x38 * i, r.choice((1.0, 1.0, 0.0, 3.0, -1.0, NAN, r.uniform(-1, 3))))
    # a large slot_b saturates the fraction at 1, so the draw hits whenever the stall term is open
    put_f32(writes, state + 0xAC + 0x38 * i, r.choice((0.0, 1.0, 1e5, 3e5, r.uniform(1e5, 1e6), r.uniform(-1e5, 1e6))))
    put_f32(writes, 0x7CA668 + 4 * i, r.choice((1.0, 0.0, -1.0, r.uniform(-1, 1))))
    if r.random() < 0.4:
        # a roll that hits: the fraction saturates at 1 and the draw is below it, so the term is raised
        put_f32(writes, opt + 8 + 0x10 * i, r.choice((0.5, 0.95)))
        put_f32(writes, state + 0x23C, 600.0)
        put_f32(writes, state + 0x94 + 0x38 * i, 1.0)
        put_f32(writes, state + 0xAC + 0x38 * i, r.choice((3e5, 1e6)))
    return [i], writes, this


def fuzz_sim80d0(mu, heap, r):
    """Sim80D0 (i < 4): the engine's objects, the LCG word the options block points at, and the entry's words drawn
    from the edges of its branches: the damper's fp6, the rate and speed, the rev gate, the option gate and the
    entry's flag."""
    i = r.randrange(4)
    this, writes = engine_objects(heap, r)
    def_at, state, opt, seed = heap + 0x4000, heap + 0x8000, heap + 0xA000, heap + 0xB000
    writes.append((opt, struct.pack("<I", seed)))
    writes.append((seed, struct.pack("<I", r.getrandbits(32))))
    writes.append((opt + 0x74, struct.pack("<I", 1 if r.random() < 0.9 else 0)))
    writes.append((def_at + 0x26F8, struct.pack("<I", 1 if r.random() < 0.8 else 0)))
    writes.append((state + 0x168 + 4 * i, struct.pack("<I", 1 if r.random() < 0.8 else 0)))
    put_f32(writes, state + 0x184, r.choice((0.0, 0.005, 0.02, -0.02, 0.5, -5.0, NAN, r.uniform(-20, 20))))
    put_f32(writes, state + 0x188, r.choice((0.0, 50.0, 80.0, 120.0, -100.0, r.uniform(-50, 300))))
    put_f32(writes, state + 0x1A8, r.choice((300.0, 293.14, r.uniform(200, 400))))
    put_f32(writes, state + 0x1B8, r.choice((101300.0, r.uniform(0, 2e5))))
    put_f32(writes, state + 0x6C, r.choice((1.0, 2.0, r.uniform(0.5, 2.0))))
    put_f32(writes, state + 0x78, r.choice((1.0, 0.0, r.uniform(0, 3000))))
    put_f32(writes, state + 0x88 + 0x38 * i + 0x18, r.choice((0.0, 0.5, -0.5, 1e3, NAN, r.uniform(-1, 3))))
    put_f32(writes, opt + 0x10 * i + 0x8, r.choice((0.0, 0.5, 1.0, r.uniform(-2, 2))))
    put_f32(writes, opt + 0x10 * i + 0x10, r.choice((0.0, 0.5, 1.0)))
    return [i], writes, this


def sim7d30_nodes(mu, a):
    """The two blend tables the definition holds at +0x9c and +0xa4: each descriptor, its records and their sub-tables."""
    d = u32_at(mu, a[0] + 4)
    out = []
    for table in (d + 0x9C, d + 0xA4):
        count, records = u32_at(mu, table), u32_at(mu, table + 4)
        out.append((table, 8))
        if count:
            out.append((records, 0xC * count))
        for k in range(count):
            sub_count, sub = u32_at(mu, records + 0xC * k + 4), u32_at(mu, records + 0xC * k + 8)
            if sub_count and sub:
                out.append((sub, 0xC * sub_count))
    return out


def fuzz_sim7d30(mu, heap, r):
    """Sim7D30 (x): the engine's objects, and the two blend tables the definition holds at +0x9c and +0xa4 (their
    descriptors written into the definition, the records and sub-tables in the heap)."""
    this, writes = engine_objects(heap, r)
    def_at = heap + 0x4000
    for k, at in enumerate((def_at + 0x9C, def_at + 0xA4)):
        _, _, tw = two_level_table(heap + 0x20000 * (k + 1), r)
        writes += tw + [(at, tw[0][1]), (at + 4, tw[1][1])]
    x = r.choice((0.0, 0.5, -0.5, 1.0, NAN, r.uniform(-2, 2)))
    return [bits(x)], writes, this


def fuzz_sim8610(mu, heap, r):
    """Sim8610 (x): the option gate, the damper's zero and sign edges (`v54`), the rate, the limits and the weights.
    The lerp reads the definition's +0x27ac and +0x27bc, so both are drawn with the rest."""
    this, writes = engine_objects(heap, r)
    def_at, state, opt = heap + 0x4000, heap + 0x8000, heap + 0xA000
    writes.append((opt + 0x48, struct.pack("<I", 1 if r.random() < 0.9 else 0)))
    put_f32(writes, opt + 0x54, r.choice((0.0, 0.5, -0.5, 1.0, 2.0, NAN, r.uniform(-1, 2))))
    put_f32(writes, def_at + 0x2784, r.choice((1.0, 0.0, 10.0, r.uniform(0, 50))))
    put_f32(writes, def_at + 0x2764, r.choice((0.0, 5.0, 100.0, r.uniform(0, 50))))
    put_f32(writes, def_at + 0x279C, r.choice((5.0, 0.0, r.uniform(0, 10))))
    put_f32(writes, state + 0x1C, r.choice((0.0, 0.5, 2.0, -1.0, NAN, r.uniform(-1, 5))))
    put_f32(writes, state + 0x24, r.choice((0.0, 0.5, 2.0, -1.0, r.uniform(-1, 5))))
    put_f32(writes, this + 0x58, r.choice((0.0, 0.5, -0.5, r.uniform(-1, 3))))
    x = r.choice((0.02, 0.05, 1.0, r.uniform(0.01, 0.1)))
    return [bits(x)], writes, this


def fuzz_sim8b20(mu, heap, r):
    """Sim8B20 (x): the divisors (kept away from zero), the flag at the definition's +0x26f8 and the options' ratios."""
    this, writes = engine_objects(heap, r)
    def_at, state, opt = heap + 0x4000, heap + 0x8000, heap + 0xA000
    writes.append((def_at + 0x26F8, struct.pack("<I", 1 if r.random() < 0.6 else 0)))
    for off in (0xB8, 0xF0, 0x128, 0x160, 0x80):
        put_f32(writes, state + off, r.choice((1.0, 2.0, r.uniform(0.5, 3.0))))
    put_f32(writes, state + 0x26C, r.choice((0.0, 1.0, r.uniform(-1, 3))))
    put_f32(writes, def_at + 0x281C, r.choice((1.0, 2.0, r.uniform(0.5, 3.0))))
    put_f32(writes, opt + 0x58, r.choice((0.0, 0.5, 1.0, -0.5, r.uniform(-1, 2))))
    x = r.choice((0.02, 0.5, 1.0, r.uniform(0.01, 0.2)))
    return [bits(x)], writes, this


def fuzz_sim8890(mu, heap, r):
    """Sim8890 (x) with the car's walk off: `state+0x1c` at or below the walk's threshold, so the car's slot 12 and
    `MB560` (host calls) do not run. The two damper steps, their accumulators, the clamps and the outputs are fuzzed."""
    this, writes = engine_objects(heap, r)
    def_at, state = heap + 0x4000, heap + 0x8000
    put_f32(writes, state + 0x1C, r.choice((0.0, -0.5, -1.0, 0.0005, NAN, r.uniform(-1, 0.001))))
    put_f32(writes, state + 0x24, r.choice((0.0, 0.0005, 0.5, -1.0, 2.0, r.uniform(-1, 2))))
    put_f32(writes, state + 0x28, r.choice((0.0, 1.0, -1.0, 5.0, NAN, r.uniform(-2, 5))))
    put_f32(writes, state + 0x238, r.choice((0.0, 0.5, -0.5, r.uniform(-2, 2))))
    put_f32(writes, def_at + 0x2774, r.choice((0.0, 1.0, -0.5, NAN, r.uniform(-1, 3))))
    x = r.choice((0.02, 0.05, 1.0, r.uniform(0.01, 0.1)))
    return [bits(x)], writes, this


def fuzz_set48(mu, heap, r):
    """CEngine::Set48 (v): the value the options pointer (+0x48) takes."""
    this, writes = engine_objects(heap, r)
    return [r.getrandbits(32)], writes, this


def fuzz_set_state4(mu, heap, r):
    """CEngine::SetState4 (a, b, c, d): the four words at the state's +0x168..+0x174."""
    this, writes = engine_objects(heap, r)
    return [r.getrandbits(32) for _ in range(4)], writes, this


def fuzz_get48_58(mu, heap, r):
    """CEngine::Get48_58: the options block's float at +0x58, or the constant 0 when the block's pointer is 0 (a third
    of the calls)."""
    this, writes = engine_objects(heap, r)
    if r.random() < 0.33:
        writes.append((this + 0x48, struct.pack("<I", 0)))
    return [], writes, this


def fuzz_sim_e970(mu, heap, r):
    """SimE970: the state's float at +0x28 (the damper bias subtracts 273.15 from it)."""
    this, writes = engine_objects(heap, r)
    put_f32(writes, heap + 0x8000 + 0x28, r.choice((0.0, 273.15, 300.0, -40.0, NAN, r.uniform(-400, 400))))
    return [], writes, this


def fuzz_sim_e980(mu, heap, r):
    """SimE980: the state's float at +0x188 around the threshold 40, and NaN."""
    this, writes = engine_objects(heap, r)
    put_f32(writes, heap + 0x8000 + 0x188, r.choice((40.0, 40.0001, 39.9999, NAN, 0.0, r.uniform(-100, 100))))
    return [], writes, this


def sim_e9a0_nodes(mu, a):
    """The world object the flag reads: the global that holds it, its keeper pointer (+0x24) and the keeper's flag word
    (+0x160e0)."""
    world = u32_at(mu, 0x1659B70)
    keeper = u32_at(mu, world + 0x24)
    return [(0x1659B70, 4), (world, 0x28), (keeper + 0x160E0, 4)]


def fuzz_sim_e9a0(mu, heap, r):
    """SimE9A0: the keeper's flag word with the bit 0x80000 on or off (half the calls)."""
    this, writes = engine_objects(heap, r)
    world, keeper = heap + 0x60000, heap + 0x62000
    flags = r.getrandbits(32) & ~0x80000 | (0x80000 if r.random() < 0.5 else 0)
    writes += [(0x1659B70, struct.pack("<I", world)), (world + 0x24, struct.pack("<I", keeper)),
               (keeper + 0x160E0, struct.pack("<I", flags))]
    return [], writes, this


def fuzz_sim_ee60(mu, heap, r):
    """SimEE60: the definition's word at +0x26f8 (on or off) and the state's float at +0x68."""
    this, writes = engine_objects(heap, r)
    writes.append((heap + 0x4000 + 0x26F8, struct.pack("<I", r.randrange(2))))
    put_f32(writes, heap + 0x8000 + 0x68, r.uniform(-3, 3))
    return [], writes, this


def fuzz_get_state188(mu, heap, r):
    """GetState188: the state's float at +0x188."""
    this, writes = engine_objects(heap, r)
    put_f32(writes, heap + 0x8000 + 0x188, r.choice((0.0, 1.0, 50.0, NAN, r.uniform(-1, 1))))
    return [], writes, this


def graph_writes(at, ring_at, r):
    """A CGraph1D (the sink CGraph1D::Push writes into): its count (+0x14, 1 to 3), its push counter (+0x44) below it,
    and its float ring (cap 8, buffer after the ring)."""
    e = r.randrange(1, 4)
    buf = ring_at + 0x100
    return [(at + 0x14, struct.pack("<i", e)), (at + 0x44, struct.pack("<I", r.randrange(0, e))),
            (at + 0x18, struct.pack("<I", ring_at)), (ring_at, struct.pack("<I", buf)),
            (ring_at + 4, struct.pack("<i", 8)), (ring_at + 8, struct.pack("<I", buf))]


# The state's graph pointers (+0x274, +0x278) lie past the 0x274 bytes the other kernels read; the misfire roll's seed
# word (the option block's first pointer) is read by SimE520 and Sim80D0.
SIM8E20_WINDOWS = [(0, lambda a: 0x198), (0, lambda a: DEF_END, 4), (0, lambda a: 0x280, 8),
                   (0, lambda a: 0x80, 0x48), (0, lambda a: 4, 0x48, 0)]


def sim8e20_nodes(mu, a):
    """The world object the graph branch reads (the global that holds it, its keeper's flag word at +0x160e0, and +0x58),
    and Sim7D30's tables when the definition's flag (+0x26f8) turns them on."""
    world = u32_at(mu, 0x1659B70)
    keeper = u32_at(mu, world + 0x24)
    out = [(0x1659B70, 4), (world, 0x5C), (keeper + 0x160E0, 4)]
    if u32_at(mu, u32_at(mu, a[0] + 4) + 0x26F8):
        out += sim7d30_nodes(mu, a)
    return out


def fuzz_sim8e20(mu, heap, r):
    """Sim8E20 (a, b): the engine's objects, the cycle counter and the loop count (the definition's +0x2698, small),
    the throttle, and the globals the sinks read: the world object (0x1659b70) with its keeper's flag word (+0x160e0)
    and its +0x58, and the sound manager (0x893600). The definition's Sim7D30 flag (+0x26f8) is drawn with its two tables.
    The car's walk stays off: `state+0x1c` is at or below the walk's threshold."""
    this, writes = engine_objects(heap, r)
    def_at, state, opt, seed = heap + 0x4000, heap + 0x8000, heap + 0xA000, heap + 0xB000
    world, keeper, manager = heap + 0x60000, heap + 0x62000, heap + 0x64000
    writes += [
        (def_at + 0x2698, struct.pack("<I", r.randrange(0, 4))),
        (state + 0x234, struct.pack("<I", r.randrange(0, 60))),
        (state + 0x274, struct.pack("<I", heap + 0x66000)),
        (state + 0x278, struct.pack("<I", heap + 0x68000)),
        (0x1659B70, struct.pack("<I", world)),
        (world + 0x24, struct.pack("<I", keeper)),
        (world + 0x58, struct.pack("<I", 0 if r.random() < 0.7 else 1)),
        (keeper + 0x160E0, struct.pack("<I", 0x200 if r.random() < 0.7 else 0)),
        (0x893600, struct.pack("<I", manager)),
        # The misfire roll (SimE520) and Sim80D0 run inside this call with no fuzz of their own: the option block's
        # first word points at a seed word.
        (opt, struct.pack("<I", seed)),
        (seed, struct.pack("<I", r.getrandbits(32))),
    ]
    writes += graph_writes(heap + 0x66000, heap + 0x66100, r) + graph_writes(heap + 0x68000, heap + 0x68100, r)
    if r.random() < 0.5:
        writes.append((def_at + 0x26F8, struct.pack("<I", 1)))
        for k, at in enumerate((def_at + 0x9C, def_at + 0xA4)):
            _, _, tw = two_level_table(heap + 0x80000 + 0x20000 * k, r)
            writes += tw + [(at, tw[0][1]), (at + 4, tw[1][1])]
    else:
        writes.append((def_at + 0x26F8, struct.pack("<I", 0)))
    put_f32(writes, state + 0x1C, r.choice((0.0, -0.5, 0.0005, NAN, r.uniform(-1, 0.001))))
    put_f32(writes, state + 0x210, r.choice((-3e9, NAN, 0.0, r.uniform(-1e9, 1e9), r.uniform(-2, 2))))
    put_f32(writes, state + 0x220, r.choice((-3e9, 0.0, r.uniform(-2, 2))))
    a = r.choice((0.02, 0.05, 1.0, r.uniform(0.005, 0.1)))
    b = r.choice((0.0, 1.0, -1.0, -1151.917, 1151.917, 2000.0, r.uniform(-500, 1500)))
    return [bits(a), bits(b)], writes, this


def blend_tables_fuzz(heap, r):
    """Two blend tables (count 1 to 6; records of key, y1, y2 at 0xc bytes) with the combined keys b * d + c * e, and
    the search's key a at a record, between two of them, outside them, or NaN. Returns (a, d, e, b, c, writes)."""
    count = r.choice((1, 2, 2, 3, 4, 5, 6))
    b_desc, c_desc, b_tab, c_tab = heap + 0x1000, heap + 0x1100, heap + 0x1200, heap + 0x1300
    d = r.choice((1.0, 0.5, -1.0, r.uniform(-2, 2)))
    e = r.choice((1.0, 0.5, -1.0, r.uniform(-2, 2)))
    bkeys = sorted(r.uniform(-5, 5) for _ in range(count))
    ckeys = [r.uniform(-1, 1) for _ in range(count)]
    writes = [(b_desc + 4, struct.pack("<I", count)), (b_desc + 8, struct.pack("<I", b_tab)),
              (c_desc + 8, struct.pack("<I", c_tab))]
    for k in range(count):
        writes.append((b_tab + 0xC * k, struct.pack("<f", bkeys[k])))
        writes.append((c_tab + 0xC * k, struct.pack("<f", ckeys[k])))
        for off in (4, 8):
            writes.append((b_tab + 0xC * k + off, struct.pack("<f", r.uniform(-1, 1))))
            writes.append((c_tab + 0xC * k + off, struct.pack("<f", r.uniform(-1, 1))))
    keys = [bkeys[k] * d + ckeys[k] * e for k in range(count)]
    a = r.choice((keys[0] - 1.0, keys[-1] + 1.0, r.uniform(keys[0], keys[-1]), keys[r.randrange(count)],
                  NAN, fuzz_pool(r, 5.0)))
    return a, d, e, b_desc, c_desc, writes


def fuzz_blend_x(mu, heap, r):
    """E41_BlendX (a, b, c, d, e, f, g, h): the tables above, outputs at the heap's scratch."""
    a, d, e, b_desc, c_desc, writes = blend_tables_fuzz(heap, r)
    out = heap + 0x1400
    return [bits(a), b_desc, c_desc, bits(d), bits(e), out, out + 4, out + 8], writes, None


def fuzz_blend_y(mu, heap, r):
    """E41_BlendY (a, b, c, d, e, f, g): the tables above, outputs at the heap's scratch."""
    a, d, e, b_desc, c_desc, writes = blend_tables_fuzz(heap, r)
    out = heap + 0x1400
    return [bits(a), b_desc, c_desc, bits(d), bits(e), out, out + 4], writes, None


def fuzz_misfire(mu, heap, r):
    """SimE520 (a): the misfire rate (0 turns the roll off), the two counters (their order picks the roll) and the
    LCG word (the option block's first word points at it)."""
    this, writes = engine_objects(heap, r)
    state, opt, seed = heap + 0x8000, heap + 0xA000, heap + 0xB000
    put_f32(writes, state + 0x10, r.choice((0.0, 0.0, 0.5, 0.9, 1.0, r.uniform(0, 1))))
    put_f32(writes, state + 0x14, r.choice((0.0, 0.02, 0.05, r.uniform(0, 0.1))))
    put_f32(writes, state + 0x18, r.choice((0.0, 0.01, 0.2, r.uniform(0, 0.3))))
    writes.append((opt, struct.pack("<I", seed)))
    writes.append((seed, struct.pack("<I", r.getrandbits(32))))
    return [bits(r.choice((0.0, 0.01, 0.05, r.uniform(0, 0.1))))], writes, this



def u32_at(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def sub_tables(index):
    """The windows of a two-level blend table (argument `index`): the outer records' sub-tables. Each outer record is
    {key, count, records}; BlendX/BlendY read the record as their descriptor, so its records are windowed too."""
    def nodes(mu, a):
        table = a[index]
        records = u32_at(mu, table + 4)
        out = []
        for k in range(u32_at(mu, table)):
            count, sub = u32_at(mu, records + 0xC * k + 4), u32_at(mu, records + 0xC * k + 8)
            if count and sub:
                out.append((sub, 0xC * count))
        return out
    return nodes


def two_level_table(heap, r):
    """An outer blend table (count at +0, records at +4; records of key, count, records) and its sub-tables (records
    of key, y1, y2). Every sub-table has the same count, as BlendX needs. Returns (table, keys, writes)."""
    # Two knots at least: BlendW reads the knot after the last one of a single-knot table (past its end).
    count, sub_count = r.randrange(2, 5), r.randrange(1, 5)
    table, recs = heap + 0x1000, heap + 0x1100
    keys = sorted(r.uniform(-1.0, 1.0) for _ in range(count))
    writes = [(table, struct.pack("<I", count)), (table + 4, struct.pack("<I", recs))]
    for k, key in enumerate(keys):
        sub = heap + 0x1400 + 0x40 * k
        writes.append((recs + 0xC * k, struct.pack("<f", key)))
        writes.append((recs + 0xC * k + 4, struct.pack("<I", sub_count)))
        writes.append((recs + 0xC * k + 8, struct.pack("<I", sub)))
        sub_keys = sorted(r.uniform(0.0, 2.0) for _ in range(sub_count))
        for j, sub_key in enumerate(sub_keys):
            writes.append((sub + 0xC * j, struct.pack("<f", sub_key)))
            for off in (4, 8):
                writes.append((sub + 0xC * j + off, struct.pack("<f", r.uniform(-1, 1))))
    return table, keys, writes


def fuzz_smooth(mu, heap, r):
    """SimE060 (a, b, c, d, e, f, g): the outer table and its sub-tables, a (the outer search key), b (the sub-table
    key, above or at 1 for the two branches), c, e, the f block (the damper's four words) and the g output."""
    this, writes = engine_objects(heap, r)
    table, keys, tw = two_level_table(heap, r)
    writes += tw
    x = r.choice((keys[0] - 0.5, keys[-1] + 0.5, r.uniform(keys[0], keys[-1]), keys[r.randrange(len(keys))], NAN))
    b = r.choice((0.5, 1.0, 1.5, 2.0, -0.5, NAN, r.uniform(-1, 3)))
    blk, out = heap + 0x1800, heap + 0x1820
    for k in range(4):
        put_f32(writes, blk + 4 * k, r.choice((0.0, 1.0, r.uniform(-2, 2), r.uniform(0, 2))))
    return [bits(x), bits(b), bits(r.choice((293.14, r.uniform(0.5, 400)))), table,
            bits(r.choice((0.01, 0.5, 1.0, r.uniform(0, 1)))), blk, out], writes, this


def fuzz_solve(mu, heap, r):
    """SimE1F0 (a, b, c, d, e, f): the outer table as in SimE060, a, b, the dt d, and the solve block e (the damper's
    four words, its accumulated word at +4 is the second term) with the f output."""
    this, writes = engine_objects(heap, r)
    table, keys, tw = two_level_table(heap, r)
    writes += tw
    x = r.choice((keys[0] - 0.5, keys[-1] + 0.5, r.uniform(keys[0], keys[-1]), keys[r.randrange(len(keys))], NAN))
    b = r.choice((0.5, 1.0, 1.5, 2.0, -0.5, NAN, r.uniform(-1, 3)))
    solve_at, out = heap + 0x1880, heap + 0x18A0
    for k in range(4):
        put_f32(writes, solve_at + 4 * k, r.choice((0.0, 1.0, r.uniform(0, 2), r.uniform(-1, 2))))
    put_f32(writes, heap + 0x8000 + 0x200, r.uniform(-1, 1))
    return [bits(x), bits(b), table, bits(r.choice((0.5, 1.0, r.uniform(0, 2)))), solve_at, out], writes, this



def blend_w_index(keys, x):
    """E41_BlendW's knot index (the search of blend_w_branch, without its branch names)."""
    count = len(keys)
    if count <= 2:
        return 1
    i = 1
    while True:
        if x <= keys[i] or x != x:
            break
        i += 1
        if i >= count - 1:
            break
    return i


def two_level_branch(args, windows, bracket):
    """SimE060 (bracket) and SimE1F0 (not): the ratio's side of 1 ('gt1' takes the second branch; NaN takes the first),
    and for SimE060 whether BlendX found a bracket: its first combined key is above the ratio, with a sub-table of
    more than one record (the window of each sub-table follows the outer header and records, in knot order)."""
    count = struct.unpack_from("<I", windows[4][3], 0)[0]
    recs = windows[5][3]
    fl = lambda at: struct.unpack_from("<f", recs, at)[0]
    keys = [fl(0xC * k) for k in range(count)]
    x, ratio = f32(args[1]), f32(args[2])
    side = "gt1" if ratio > 1.0 else "le1"
    if not bracket:
        return side
    sub = {}
    j = 8
    for k in range(count):
        sub_count, sub_at = struct.unpack_from("<II", recs, 0xC * k + 4)
        if sub_count and sub_at:
            sub[k] = windows[j][3]
            j += 1
    hi = blend_w_index(keys, x)
    lo = hi - 1
    g = f32_round(1.0 - f32_round(f32_round(x - keys[lo]) / f32_round(keys[hi] - keys[lo])))
    h = f32_round(1.0 - g)
    if h != h or h <= 1.0:
        d, e = (0.0, 1.0) if h < 0.0 else (h, g)
    else:
        d, e = 1.0, 0.0
    sub_hi, sub_lo = sub.get(hi), sub.get(lo)
    if sub_hi is None or sub_lo is None or struct.unpack_from("<I", sub_hi, 4)[0] == 1:
        return side + "/none"
    key0 = f32_round(f32_round(struct.unpack_from("<f", sub_hi, 0)[0] * d) + f32_round(struct.unpack_from("<f", sub_lo, 0)[0] * e))
    return side + ("/bracket" if key0 > ratio else "/none")


def sime060_branch(args, windows):
    return two_level_branch(args, windows, True)


def sime1f0_branch(args, windows):
    return two_level_branch(args, windows, False)


# CDriveLine (drive_line.rs): the table at `this` (entries of 0x30 from this; entry k's fields at this+0x30k+0x10:
# q vec4, v vec4, s float, tag dword), the count at this+0x75310. The fuzzed objects sit in the stub heap.
DRIVE_LINE_COUNT = 0x75310
DRIVE_SLOTS = (0, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9999)


def drive_entry_at(this, k):
    return (this + 0x30 * k + 0x10) & 0xFFFFFFFF


def drive_entry_bytes(r):
    """One entry's 0x28 bytes: q, v and s as finite floats, then the tag."""
    vals = [r.uniform(-4.0, 4.0) for _ in range(9)]
    return struct.pack("<9fI", *vals, r.getrandbits(32))


def drive_junk(this, r, slots):
    """Random entries at the given indices of the table at this (so an unwritten word shows)."""
    return [(drive_entry_at(this, k), drive_entry_bytes(r)) for k in slots]


def drive_fuzz_object(heap, r, count):
    """A CDriveLine at heap+0x100000 with `count` entries plus one past them, and the count."""
    this = heap + 0x100000
    writes = drive_junk(this, r, range(count + 2))
    writes.append((this + DRIVE_LINE_COUNT, struct.pack("<I", count)))
    return this, writes


def fuzz_blend_b(mu, heap, r):
    """CDriveLine::BlendB (this, idx, t, out): the entries idx and idx+1 of a table of 2 to 12 entries."""
    count = r.randrange(2, 13)
    this, writes = drive_fuzz_object(heap, r, count)
    out = heap + 0x1400
    return [r.randrange(0, count), bits(r.choice((0.0, 1.0, 0.5, -0.25, r.uniform(-2.0, 2.0), r.uniform(0, 1)))),
            out], writes, this


def fuzz_last(mu, heap, r):
    """CDriveLine::Last (this): the table's last entry, with one or more entries (the empty table traps)."""
    count = r.randrange(1, 13)
    this, writes = drive_fuzz_object(heap, r, count)
    return [], writes, this


def fuzz_thunk(mu, heap, r):
    """CDriveLine::Thunk4F0 (this, id): the id is E12_SampleX's float argument. E12 lerps the s values of the drive
    line at the global g_e19Ptr59184 (0x1659184), so that global points at a fuzzed table; the thunk's ecx word (the
    global at 0x7eaba8) is random and unused by E12."""
    count = r.randrange(2, 13)
    this, writes = drive_fuzz_object(heap, r, count)
    writes.append((0x1659184, struct.pack("<I", this)))
    writes.append((0x7EABA8, struct.pack("<I", r.getrandbits(32))))
    return [bits(r.uniform(0.0, count - 1.0))], writes, this


def thunk_nodes(mu, a):
    """E12_SampleX's reads: the drive line at the global, its count and the s of entries floor(x) and the next."""
    this = u32_at(mu, 0x1659184)
    count = u32_at(mu, this + DRIVE_LINE_COUNT)
    i = math.floor(struct.unpack("<f", struct.pack("<I", a[1]))[0])
    out = [(0x7EABA8, 4), (0x1659184, 4), (this + DRIVE_LINE_COUNT, 4), (drive_entry_at(this, i) + 0x20, 4)]
    if i + 1 < count:
        out.append((drive_entry_at(this, i + 1) + 0x20, 4))
    return out


def clamp_x(r):
    """ClampMix's x: the branch edges (2^31, -2^31, the NaN), negatives, zero and the interpolated range."""
    return r.choice((NAN, -3.0e9, -2147483648.0, -5.0, -1.0, -0.5, 0.0, 0.25, 3.7, 1000.0, 2147483648.0, 3.0e9,
                     r.uniform(-10.0, 10.0), r.uniform(0.0, 1.0e4)))


def fuzz_clamp_mix(mu, heap, r):
    """CDriveLine::ClampMix (this, x, out): x picks the index and the fraction (see clamp_index)."""
    count = r.randrange(2, 13)
    this, writes = drive_fuzz_object(heap, r, count)
    # x at or above 2^31 reads the entry before the table (the wrapped index 0x7fffffff, at this-0x30).
    writes.append((drive_entry_at(this, 0xFFFFFFFF), drive_entry_bytes(r)))
    return [bits(clamp_x(r)), heap + 0x1400], writes, this


def clamp_index(x):
    """The index ClampMix blends at (0 for NaN and below -2^31; 0x7fffffff at or above 2^31)."""
    if x >= 2147483648.0:
        return 0x7FFFFFFF
    if not x >= -2147483648.0:
        return 0
    return max(0, int(x))


def clamp_branch(args, windows):
    """'above' (x >= 2^31: index 0x7fffffff), 'log' (NaN or below -2^31: index 0), 'negative' (integer part below 0),
    'interp' (0 <= x < 2^31)."""
    x = struct.unpack("<f", struct.pack("<I", args[1]))[0]
    if x >= 2147483648.0:
        return "above"
    if not x >= -2147483648.0:
        return "log"
    return "negative" if int(x) < 0 else "interp"


def blend_b_nodes(mu, a):
    """The entries BlendB reads: idx's q and v, and the next entry's (0x50 bytes from the q)."""
    return [(drive_entry_at(a[0], a[1]), 0x50)]


def clamp_nodes(mu, a):
    return [(drive_entry_at(a[0], clamp_index(struct.unpack("<f", struct.pack("<I", a[1]))[0])), 0x50)]


def last_nodes(mu, a):
    """The count and the last entry's s (the float at the next entry's base)."""
    count = u32_at(mu, a[0] + DRIVE_LINE_COUNT)
    return [(a[0] + DRIVE_LINE_COUNT, 4), (drive_entry_at(a[0], (count - 1) & 0xFFFFFFFF) + 0x20, 4)]


def set_drive_line_slots(n):
    """The entries SetDriveLine's windows cover: the copied entries and one past them, then sampled far entries."""
    return sorted(set(range(n + 2)) | set(DRIVE_SLOTS))


def fuzz_set_drive_line(mu, heap, r):
    """CDriveLine::SetDriveLine (this, sig): a signal of 0 to 40 entries (the table is 10000 long, so the far entries
    are sampled), with random entries in the table beforehand."""
    this = heap + 0x100000
    n = r.randrange(0, 41)
    sig, base = heap + 0x10, heap + 0x200
    writes = [(sig, struct.pack("<I", n)), (sig + 4, struct.pack("<I", base))]
    writes += [(base + 0x30 * k, drive_entry_bytes(r) + bytes(8)) for k in range(n)]
    writes += drive_junk(this, r, set_drive_line_slots(n))
    writes.append((this + DRIVE_LINE_COUNT, struct.pack("<I", r.getrandbits(32))))
    return [sig], writes, this


def fuzz_sample_big(mu, heap, r):
    """CDriveLine::SampleBig (this, pos, x): a table of 2 to 12 entries and a point near entry i (1 <= i < count, so the
    bisection's index i - 1 is in the table and the point's distance keeps the step k at 1). x is i plus a fraction."""
    count = r.randrange(2, 13)
    this, writes = drive_fuzz_object(heap, r, count)
    i = r.randrange(1, count)
    entry = dict(writes)[drive_entry_at(this, i)]
    q = struct.unpack_from("<3f", entry)
    pos = heap + 0x1400
    point = [c + r.uniform(-1.0, 1.0) for c in q]
    writes.append((pos, struct.pack("<3f", *point) + bytes(4)))
    return [pos, bits(i + r.uniform(0.0, 0.999))], writes, this


def sample_big_nodes(mu, a):
    """The count and the entries from one before the table to one past the count (the bisection reads them)."""
    this = a[0]
    count = u32_at(mu, this + DRIVE_LINE_COUNT)
    return [(this + DRIVE_LINE_COUNT, 4)] + [(drive_entry_at(this, k), 0x30) for k in range(-1, count + 1)]


def set_drive_line_nodes(mu, a):
    """The signal's entries, the count and the table's windows (the copied entries, one past them, and the samples)."""
    this, sig = a[0], a[1]
    n = u32_at(mu, sig)
    base = u32_at(mu, sig + 4)
    out = [(base, 0x30 * n)] if n else []
    out += [(drive_entry_at(this, k), 0x28) for k in set_drive_line_slots(n)]
    return out + [(this + DRIVE_LINE_COUNT, 4)]


# E40 drive leaves (drive_leaf.rs): CDrive objects with the definition at this+4 and the state at this+8.
def float_words(at, nbytes, r, lo=-2.0, hi=2.0):
    """Finite random floats over nbytes from at (one write per dword)."""
    return [(at + k, struct.pack("<f", r.uniform(lo, hi))) for k in range(0, nbytes, 4)]


def drive_leaf_objects(heap, r):
    """A CDrive at heap+0x3000 with random floats in its own 0x84 bytes, its definition (0xa60 bytes) and state
    (0x560 bytes). Returns (this, def, state, writes)."""
    this, def_at, state_at = heap + 0x3000, heap + 0x4000, heap + 0x8000
    writes = [(this + 4, struct.pack("<I", def_at)), (this + 8, struct.pack("<I", state_at))]
    writes += float_words(this + 0x40, 0x44, r)
    writes += float_words(def_at, 0xA60, r)
    writes += float_words(state_at, 0x560, r)
    return this, def_at, state_at, writes


def fuzz_m5970(mu, heap, r):
    """CDrive::M5970 (this, x): x is the lane scale (a float at +8) and a word at +0x1c."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    x = heap + 0x5000
    writes += float_words(x, 0x20, r)
    writes.append((x + 0x1C, struct.pack("<I", r.getrandbits(32))))
    return [x], writes, this


def fuzz_m5a10(mu, heap, r):
    """CDrive::M5A10 (this): the gear's ratio at def+0x694+0x50*gear, the state's gear at +0x54c and its flag at
    +0x55c. The ratio is zero, NaN or a random value of either sign."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    gear = r.randrange(-1, 12)
    ratio = r.choice((0.0, -0.0, NAN, r.uniform(0.2, 2.0), r.uniform(-2.0, -0.2)))
    writes += [(state_at + 0x54C, struct.pack("<i", gear)), (state_at + 0x55C, struct.pack("<I", r.choice((0, 0, 1))))]
    writes.append((def_at + 0x694 + 0x50 * gear, struct.pack("<f", ratio)))
    return [], writes, this


def m5a10_branch(args, windows):
    """'busy' (the state's flag +0x55c is set), 'nan' and 'zero' (the ratio), 'compute'. Windows 1 and 2 are the
    state and the definition (the windows hop from this)."""
    state, definition = windows[1][3], windows[2][3]
    if struct.unpack_from("<I", state, 0x55C)[0] != 0:
        return "busy"
    gear = struct.unpack_from("<i", state, 0x54C)[0]
    ratio = struct.unpack_from("<f", definition, 0x694 + 0x50 * gear)[0]
    if ratio != ratio:
        return "nan"
    return "zero" if ratio == 0.0 else "compute"


def fuzz_m5d50(mu, heap, r):
    """CDrive::M5D50 (this): the definition's indices (0..3) and the state's gates (0, 1 or 2) and pair gates."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    for at in (0x68, 0x88, 0x98, 0xC8, 0xE8, 0xF8, 0x128):
        writes.append((def_at + at, struct.pack("<I", r.randrange(4))))
    for at in range(0, 0x5C0, 4):
        writes.append((state_at + at, struct.pack("<I", r.choice((0, 0, 1, 2)))))
    # the four gates of the branch: all clear, all set, or random (the three of them and the 2fc flag)
    mode = r.choice(("none", "all", "random"))
    for at in (0x14, 0x28, 0x3C):
        writes.append((state_at + at, struct.pack("<I", r.choice((0, 0, 1, 2)))))
    for at in (0xA4, 0x16C, 0x234, 0x2FC):
        if mode == "none":
            value = 0
        elif mode == "all":
            value = r.randrange(1, 3)
        else:
            value = r.choice((0, 0, 1, 2))
        writes.append((state_at + at, struct.pack("<I", value)))
    for k in range(4):  # pair k's gate (k = 0 is the +0xa4 gate above)
        value = 0 if mode == "none" else r.randrange(1, 3) if mode == "all" else r.choice((0, 0, 1))
        writes.append((state_at + 200 * k + 0xA4, struct.pack("<I", value)))
    return [], writes, this


def m5d50_branch(args, windows):
    """'none' (the gate is clear and the three gates are zero), 'all' (the gate is set and the three are nonzero),
    'other' (the rest). Windows 1 and 2 are the state and the definition."""
    state = windows[1][3]
    word = lambda at: struct.unpack_from("<I", state, at)[0]
    b3, b4, b5 = word(0xA4) != 0, word(0x16C) != 0, word(0x234) != 0
    if word(0x2FC) == 0 and not (b3 or b4 or b5):
        return "none"
    if word(0x2FC) != 0 and b3 and b4 and b5:
        return "all"
    return "other"


def fuzz_m5fe0(mu, heap, r):
    """CDrive::M5FE0 (this, x): wheel x's gate (the state's +0x5c word in its 200-byte block), the definition's wheel
    block (c at +0x2c), and the flag array the pointer at this+0x170 names (entry 3x+6 gates the drive term)."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    flags = heap + 0x5000
    x = r.randrange(0, 4)
    writes += float_words(this + 0x10, 0x170, r)  # from +0x10: the def and state pointers are at +4 and +8
    writes += float_words(def_at, 0x1000, r)
    writes += float_words(state_at, 0x800, r)
    writes += [(flags + 4 * k, struct.pack("<I", r.choice((0, 1, 2)))) for k in range(0x10)]
    writes.append((def_at + 0x12C + 0xF0 * x + 0x2C, struct.pack("<I", r.randrange(4))))
    writes.append((state_at + 200 * x + 0x48 + 0x5C, struct.pack("<I", r.choice((0, 1, 2)))))
    writes.append((flags + 4 * (3 * x + 6), struct.pack("<I", r.choice((0, 1)))))
    writes.append((this + 0x170, struct.pack("<I", flags)))
    return [x], writes, this


def m5fe0_branch(args, windows):
    """'idle' (wheel x's gate is zero, the output is 0), 'drive' (the flag entry 3x+6 is set), 'coast' (it is not).
    Windows 1 and 3 are the flags and the state (the windows hop from this)."""
    x = args[1]
    if struct.unpack_from("<I", windows[3][3], 200 * x + 0x48 + 0x5C)[0] == 0:
        return "idle"
    return "drive" if struct.unpack_from("<I", windows[1][3], 4 * (3 * x + 6))[0] else "coast"


def fuzz_m5be0(mu, heap, r):
    """CDrive::M5BE0 (this, x): wheel x's block (c at def+0x12c+0xf0x+0x2c picks the pack, the gate at +0xa4 and the
    load at +0x6c), the pack's index words (def+0x68 and def+0x60c+0x68) and the state's flags at +0x4f0."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    x = r.randrange(0, 4)
    writes.append((def_at + 0x12C + 0xF0 * x + 0x2C, struct.pack("<I", r.randrange(4))))
    writes.append((def_at + 0x68, struct.pack("<I", r.randrange(4))))
    for k in range(4):
        writes.append((def_at + 0x60 * k + 0x18, struct.pack("<i", r.randrange(1, 6))))
        writes.append((def_at + 0x60 * k + 0x68, struct.pack("<I", r.randrange(4))))
    writes.append((state_at + 200 * x + 0xA4, struct.pack("<I", r.choice((0, 1)))))
    for k in range(4):
        writes.append((state_at + 0x4F0 + 4 * k, struct.pack("<I", r.choice((0, 1)))))
    return [x], writes, this


def m5be0_branch(args, windows):
    """The gate of wheel x (g1 set, g0 clear), and the two flag words at +0x4f0 for the pack c and the pack c2 (c0 or
    c1 by whether the flag is zero)."""
    x = args[1]
    state, definition = windows[1][3], windows[2][3]
    gate = "g1" if struct.unpack_from("<I", state, 200 * x + 0xA4)[0] else "g0"
    c = struct.unpack_from("<I", definition, 0x68)[0]
    c2 = struct.unpack_from("<I", definition, 0x60 * struct.unpack_from("<I", definition, 0x12C + 0xF0 * x + 0x2C)[0] + 0x68)[0]
    flag = lambda k: "c0" if struct.unpack_from("<I", state, 0x4F0 + 4 * k)[0] == 0 else "c1"
    return gate + flag(c) + flag(c2)


def fuzz_m6260(mu, heap, r):
    """CDrive::M6260 (this, x): wheel x's latch. The block's gate (+0xa4) is mostly clear, the flag entry 3x+6 is
    random, and the definition's record index c (at +0x158 of wheel x) is 0..3 so the record stays in the window."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    flags = heap + 0x5000
    x = r.randrange(0, 4)
    writes += float_words(this + 0x10, 0x170, r)  # from +0x10: the def and state pointers are at +4 and +8
    writes += [(flags + 4 * k, struct.pack("<I", r.choice((0, 1, 2)))) for k in range(0x10)]
    writes.append((def_at + 0xF0 * x + 0x158, struct.pack("<I", r.randrange(4))))
    writes.append((state_at + 200 * x + 0xA4, struct.pack("<I", r.choice((0, 0, 0, 1)))))
    writes.append((flags + 4 * (3 * x + 6), struct.pack("<I", r.choice((0, 1)))))
    writes.append((this + 0x170, struct.pack("<I", flags)))
    if r.random() < 0.5:  # the drive and slip terms are zero, so the drive value is 0 and the latch band is tested
        writes += [(this + 0x90, struct.pack("<f", 0.0)), (state_at + 0x37C, struct.pack("<f", 0.0))]
        writes += [(state_at + 0x1C + 0x14 * j, struct.pack("<f", 0.0)) for j in range(4)]
    return [x], writes, this


def m6260_branch(args, windows):
    """'gated' (the block's gate +0xa4 is set). Otherwise the flag entry 3x+6 ('flag0' or 'flag1') and the latch: 'up'
    (the drive is above the clamp), 'down' (below), or 'none'. Windows 0 to 3 are this, the flags, the definition and
    the state (the windows hop from this)."""
    x = args[1]
    this, flags, definition, state = (windows[k][3] for k in range(4))
    if struct.unpack_from("<I", state, 200 * x + 0xA4)[0]:
        return "gated"
    fv = lambda buf, at: struct.unpack_from("<f", buf, at)[0]
    a = 0xF0 * x
    c = struct.unpack_from("<I", definition, a + 0x158)[0]
    e = 0x60 * c
    g = f32_round(fv(definition, e + 0x48) * fv(definition, a + 0x148))
    d18 = f32_round(float(struct.unpack_from("<i", definition, e + 0x18)[0]))
    drive = f32_round(f32_round(fv(state, 0x37C) - f32_round(fv(state, 0x460) * fv(this, 0x90)))
                * f32_round(f32_round(f32_round(d18 * fv(definition, 0x48)) * g) * 0.25))
    slip = f32_round(f32_round(f32_round(g * 0.5) * fv(state, 0x1C)) + fv(state, 0x1C + 0x14 * c))
    f = f32_round(drive - slip)
    if struct.unpack_from("<I", flags, 4 * (3 * x + 6))[0] == 0:
        flag, k, h = "flag0", 0.0, 0.0
    else:
        flag, k, h = "flag1", fv(this, 0x10 * x + 0xF4), fv(state, 200 * x + 0xF8)
    hk = f32_round(h - k)
    neg = f32_round(0.0 - f32_round(h + k))
    latch = "up" if f > hk else "down" if neg > f else "none"
    return f"{flag}/{latch}"

def sign_class(f):
    return "n" if f != f else "p" if f > 0.0 else "m" if f < 0.0 else "z"


def fuzz_m6630(mu, heap, r):
    """CDrive::M6630 (this): two pairs of blocks. The pair indices are at def+0x88/0x98 and +0xf8/0xe8, the blocks'
    sum words at +0x94 and the gate words at +0xa4 are random. In some calls a pair's sums cancel, so its gate decides
    the sign, and sometimes a block's sum is NaN."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    idx = {off: r.randrange(4) for off in (0x88, 0x98, 0xE8, 0xF8)}
    writes += [(def_at + off, struct.pack("<I", i)) for off, i in idx.items()]
    for k in range(4):
        writes.append((state_at + 200 * k + 0xA4, struct.pack("<i", r.randrange(-1, 2))))
    for i0_at, i1_at in ((0x88, 0x98), (0xF8, 0xE8)):
        i0, i1 = idx[i0_at], idx[i1_at]
        if r.random() < 0.4:
            g = r.uniform(-2.0, 2.0) if i0 != i1 else 0.0
            writes += [(state_at + 200 * i0 + 0x94, struct.pack("<f", g)),
                       (state_at + 200 * i1 + 0x94, struct.pack("<f", -g))]
        if r.random() < 0.15:
            writes.append((state_at + 200 * i0 + 0x94, struct.pack("<f", NAN)))
    return [], writes, this


def m6630_stage(state, definition, i0_at, i1_at, scale_at):
    """One stage of M6630 on the pre-call bytes: the weighted sum and the class of the pair's sum ('p', 'm', 'n', or
    'z' with the gate's sign 'g+', 'g-', 'g0')."""
    i0 = struct.unpack_from("<I", definition, i0_at)[0]
    i1 = struct.unpack_from("<I", definition, i1_at)[0]
    b0, b1 = 200 * i0, 200 * i1
    g0 = struct.unpack_from("<f", state, b0 + 0x94)[0]
    g1 = struct.unpack_from("<f", state, b1 + 0x94)[0]
    w0 = struct.unpack_from("<f", definition, 0x148 + 0xF0 * i0)[0]
    w1 = struct.unpack_from("<f", definition, 0x148 + 0xF0 * i1)[0]
    scale = f32_round(struct.unpack_from("<f", definition, scale_at)[0] * 0.5)
    weighted = f32_round(f32_round(f32_round(w0 * g0) + f32_round(w1 * g1)) * scale)
    total = f32_round(g0 + g1)
    if total == 0.0:
        gate = struct.unpack_from("<i", state, b0 + 0xA4)[0] + struct.unpack_from("<i", state, b1 + 0xA4)[0]
        return weighted, "g+" if gate > 0 else "g-" if gate < 0 else "g0"
    return weighted, sign_class(total)


def m6630_branch(args, windows):
    """The class of each stage's pair sum, then the sign of the total at +0x10 (the two weighted sums). Windows 1 and 2
    are the state and the definition."""
    state, definition = windows[1][3], windows[2][3]
    w1, c1 = m6630_stage(state, definition, 0x88, 0x98, 0xA8)
    w2, c2 = m6630_stage(state, definition, 0xF8, 0xE8, 0x108)
    return f"{c1}/{c2}/{sign_class(f32_round(w2 + w1))}"

def fuzz_m6a00(mu, heap, r):
    """CDrive::M6A00 (this, x): wheel x's record (at def+0x60x: the block index k at +0x28 and the divisor at +0x48),
    block k's gate (+0xa4) and the state's entry x (at +0x14x). The divisor is never zero."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    x = r.randrange(0, 4)
    k = r.randrange(0, 4)
    writes.append((def_at + 0x60 * x + 0x28, struct.pack("<I", k)))
    writes.append((def_at + 0x60 * x + 0x48, struct.pack("<f", r.choice((-1.0, 1.0)) * r.uniform(0.5, 2.0))))
    writes.append((state_at + 200 * k + 0xA4, struct.pack("<I", r.choice((0, 1)))))
    return [x], writes, this


def m6a00_branch(args, windows):
    """'gate1' (block k's gate +0xa4 is set, the lane is written) or 'gate0'. Windows 1 and 2 are the state and the
    definition (the windows hop from this)."""
    state, definition = windows[1][3], windows[2][3]
    k = struct.unpack_from("<I", definition, 0x60 * args[1] + 0x28)[0]
    return "gate1" if struct.unpack_from("<I", state, 200 * k + 0xA4)[0] else "gate0"

def fuzz_m6b20(mu, heap, r):
    """CDrive::M6B20 (this, x): wheel x's record (block indices k at +0x28 and m at +0x38), the blocks' gates (+0xa4,
    often zero), the entry x's word at +0x14 (P) and the outputs this+0xc0+4i, which are often zero."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    x = r.randrange(0, 4)
    k, m = r.randrange(0, 4), r.randrange(0, 4)
    writes += [(def_at + 0x60 * x + 0x28, struct.pack("<I", k)), (def_at + 0x60 * x + 0x38, struct.pack("<I", m))]
    for i in range(4):
        writes.append((state_at + 200 * i + 0xA4, struct.pack("<I", r.choice((0, 0, 1, 2)))))
        writes.append((this + 0xC0 + 4 * i, struct.pack("<f", r.choice((0.0, -0.0, 0.0, NAN, r.uniform(-1, 1))))))
    writes.append((state_at + 0x14 * x + 0x14, struct.pack("<I", r.choice((0, 0, 3)))))
    return [x], writes, this


def m6b20_branch(args, windows):
    """'zeros0' or 'zeros2' (no write, or the floats and P[+0x14] cleared), 'hold' (one gate zero and P[+0x14] set: no
    write), 'call' (one gate zero, P[+0x14] clear, both outputs zero), 'store' (the same with an output not zero).
    Windows 0 to 2 are this, the state and the definition (the windows hop from this)."""
    this, state, definition = (windows[k][3] for k in range(3))
    x = args[1]
    k = struct.unpack_from("<I", definition, 0x60 * x + 0x28)[0]
    m = struct.unpack_from("<I", definition, 0x60 * x + 0x38)[0]
    zeros = (struct.unpack_from("<I", state, 200 * k + 0xA4)[0] == 0) + (struct.unpack_from("<I", state, 200 * m + 0xA4)[0] == 0)
    if zeros != 1:
        return f"zeros{zeros}"
    if struct.unpack_from("<I", state, 0x14 * x + 0x14)[0]:
        return "hold"
    out = lambda i: struct.unpack_from("<f", this, 0xC0 + 4 * i)[0]
    return "call" if out(k) == 0.0 and out(m) == 0.0 else "store"


def fuzz_m6c90(mu, heap, r):
    """CDrive::M6C90 (this, a), x87 result: the state's speed (+0x45c) and its +0x54c, the drive's +0x50 and +0x2c, and
    the object at this+0x170 (its words +0x74, +0x7c and +0x8c). The speed covers the zero, low and high cases."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    obj = heap + 0x5000
    writes += float_words(obj, 0x90, r)
    writes.append((obj + 0x7C, struct.pack("<I", r.choice((0, 1, 2)))))
    writes.append((this + 0x170, struct.pack("<I", obj)))
    writes.append((this + 0x50, struct.pack("<I", r.choice((0, 0, 1)))))
    writes.append((this + 0x2C, struct.pack("<f", r.choice((0.3, 0.7, 1.0, r.uniform(-1, 2))))))
    writes.append((state_at + 0x54C, struct.pack("<i", r.choice((0, 1, 1, 2)))))
    speed = r.choice((0.0, 250.0, 300.0, 350.0, 399.0, 401.0, 500.0, NAN, r.uniform(-50, 450)))
    writes.append((state_at + 0x45C, struct.pack("<f", speed)))
    a = r.uniform(-2.0, 2.0)
    return [struct.unpack("<I", struct.pack("<f", a))[0]], writes, this


def m6c90_branch(args, windows):
    """'zero_speed' (the speed is not below 400, NaN included), 'zero_gate' (the gate is off), 'zero_obj' (the object's
    +0x7c is zero), 'low' (speed at or below 300) or 'high'. Windows 0 to 2 are this, the state and the object."""
    this, state, obj = (windows[k][3] for k in range(3))
    speed = struct.unpack_from("<f", state, 0x45C)[0]
    if not 400.0 > speed:
        return "zero_speed"
    gated = struct.unpack_from("<I", this, 0x50)[0] != 0 or (
        struct.unpack_from("<i", state, 0x54C)[0] == 1 and struct.unpack_from("<f", this, 0x2C)[0] > 0.5)
    if not gated:
        return "zero_gate"
    if struct.unpack_from("<I", obj, 0x7C)[0] == 0:
        return "zero_obj"
    return "low" if speed <= 300.0 else "high"


def fuzz_m79b0(mu, heap, r):
    """CDrive::M79B0 (this, x): the state's gear (+0x54c), its flag (+0x55c) and the car (+0x580), whose word at +0x30 is
    the Get30 answer. The car object is in the stub heap."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    car = heap + 0x7000
    writes.append((car + 0x30, struct.pack("<I", r.getrandbits(32))))
    writes.append((state_at + 0x580, struct.pack("<I", car)))
    writes.append((state_at + 0x54C, struct.pack("<i", r.randrange(-1, 12))))
    writes.append((state_at + 0x55C, struct.pack("<I", r.getrandbits(32))))
    return [r.getrandbits(32)], writes, this


def fuzz_m9120(mu, heap, r):
    """CDrive::M9120 (this, dt), int result: the CDef's flags (+0x160cc; the gate 0x80000 on about three calls in four),
    the state's gear (+0x54c), the definition's band for it (+0x688 + 0x50 gear), the object at this+0x170 (its
    words +0x88 and +0x48 + 4 gear) and the car M79B0 reads (+0x580). dt is NaN or a random float."""
    this, def_at, state_at, writes = drive_leaf_objects(heap, r)
    obj, physics, cdef = heap + 0x6000, heap + 0x9000, heap + 0xA000
    writes += float_words(obj, 0x90, r)
    writes.append((this + 0x170, struct.pack("<I", obj)))
    writes.append((state_at + 0x54C, struct.pack("<i", r.randrange(-1, 12))))
    car = heap + 0x7000
    writes.append((car + 0x30, struct.pack("<I", r.getrandbits(32))))
    writes.append((state_at + 0x580, struct.pack("<I", car)))
    writes.append((0x1659B70, struct.pack("<I", physics)))
    writes.append((physics + 0x24, struct.pack("<I", cdef)))
    gate = 0x80000 if r.random() < 0.75 else 0
    writes.append((cdef + 0x160CC, struct.pack("<I", (r.getrandbits(32) & ~0x80000) | gate)))
    dt = r.choice((NAN, r.uniform(-2.0, 2.0), r.uniform(-20.0, 20.0)))
    return [bits(dt)], writes, this


def m9120_nodes(mu, a):
    """CDrive::M9120's reads outside its windows: the physics global (g_1659b70), its CDef (+0x24) and the flags."""
    physics = u32_at(mu, 0x1659B70)
    cdef = u32_at(mu, physics + 0x24)
    return [(0x1659B70, 4), (physics + 0x24, 4), (cdef + 0x160CC, 4)]


def m9120_branch(args, windows):
    """'off' (the gate is clear), 'lower' or 'upper' (dt is outside the gear's band) or 'band' (dt is inside it, NaN too).
    Windows 1 to 3 are the state, the definition and the object; window 6 is the CDef's flags."""
    if not struct.unpack_from("<I", windows[6][3])[0] & 0x80000:
        return "off"
    dt = struct.unpack("<f", struct.pack("<I", args[1]))[0]
    state, dfn, obj = (windows[k][3] for k in (1, 2, 3))
    idx = struct.unpack_from("<i", state, 0x54C)[0]
    band = 0x688 + idx * 0x50
    scale = struct.unpack_from("<f", obj, 0x48 + 4 * idx)[0]
    base = struct.unpack_from("<f", obj, 0x88)[0]
    lower = f32_round(f32_round(base * struct.unpack_from("<f", dfn, band + 0x3C)[0]) * scale)
    upper = f32_round(f32_round(base * struct.unpack_from("<f", dfn, band + 0x4C)[0]) * scale)
    if dt < lower:
        return "lower"
    return "upper" if upper < dt else "band"


def fuzz_m9210(mu, heap, r):
    """CDrive::M9210 (this, dt), void: the gear objects of M9120 (the flags, the state, the band, the object at
    this+0x170, the car) and the state's RNG word (+0x57c). dt is NaN or a random float, up to 60 in size so that the
    speed term reaches its cap (0.8)."""
    _, writes, this = fuzz_m9120(mu, heap, r)
    writes.append((heap + 0x8000 + 0x57C, struct.pack("<I", r.getrandbits(32))))
    dt = r.choice((NAN, r.uniform(-2.0, 2.0), r.uniform(-60.0, 60.0)))
    return [bits(dt)], writes, this


def m9210_branch(args, windows):
    """'off' (the gate is clear) or 'gate' (the RNG step runs), with '-capped' when the speed term is above 0.8; 'nan'
    for a NaN dt (see m9120_branch for the windows)."""
    dt = struct.unpack("<f", struct.pack("<I", args[1]))[0]
    if dt != dt:
        return "nan"
    gate = "gate" if struct.unpack_from("<I", windows[6][3])[0] & 0x80000 else "off"
    return gate + ("-capped" if f32_round(abs(dt) * f32_round(0.033333335)) > 0.8 else "")


def fuzz_m8570(mu, heap, r):
    """CDrive::M8570 (this, dt), void: the M9120 objects, the gear and its target (equal about half the time), the
    object's word +0x78 (zero a quarter of the time), the lane's slope and reach (+0xa74, +0xa84), the trim accumulator
    (+0x574), the trim (+0x560) and the flag (+0x55c). NaN appears in some of them."""
    _, writes, this = fuzz_m9120(mu, heap, r)
    state_at, def_at, obj = heap + 0x8000, heap + 0x4000, heap + 0x6000
    gear = r.randrange(-1, 12)
    target = gear if r.random() < 0.5 else r.randrange(-1, 12)
    writes += [
        (state_at + 0x54C, struct.pack("<i", gear)),
        (state_at + 0x570, struct.pack("<i", target)),
        (obj + 0x78, struct.pack("<I", r.choice((0, 1, 2)))),
        (def_at + 0xA74, struct.pack("<f", r.choice((0.0, -1.0, 0.5, r.uniform(-2.0, 2.0))))),
        (def_at + 0xA84, struct.pack("<f", r.choice((0.0, 0.5, -1.0, r.uniform(-1.0, 2.0))))),
        (state_at + 0x574, struct.pack("<f", r.choice((NAN, -1.0, 0.0, 0.5, r.uniform(-2.0, 2.0))))),
        (state_at + 0x560, struct.pack("<f", r.choice((0.0, 0.5, 1.0, 3.0, NAN, r.uniform(-1.0, 5.0))))),
        (state_at + 0x55C, struct.pack("<I", r.choice((0, 0, 1)))),
    ]
    dt = r.choice((NAN, r.uniform(-1.0, 1.0), r.uniform(0.0, 0.2)))
    return [bits(dt)], writes, this


def m8570_branch(args, windows):
    """'hold' or 'step' (the trim is off: the flag holds it or steps it), or on the trim path 'skip' (the slope is above
    the accumulator), 'trip' (the reach is not above it, NaN included), 'relay' (the flag set: M79B0 runs) or
    'accumulate'. Windows 1 to 3 are the state, the definition and the object."""
    state, dfn, obj = (windows[k][3] for k in (1, 2, 3))
    flag = struct.unpack_from("<I", state, 0x55C)[0]
    if struct.unpack_from("<I", state, 0x54C)[0] == struct.unpack_from("<I", state, 0x570)[0] or not struct.unpack_from("<I", obj, 0x78)[0]:
        return "hold" if flag else "step"
    limit = struct.unpack_from("<f", state, 0x574)[0]
    slope = struct.unpack_from("<f", dfn, 0xA74)[0]
    if slope > limit:
        return "skip"
    reach = f32_round(struct.unpack_from("<f", dfn, 0xA84)[0] + slope)
    if not reach > limit:
        return "trip"
    return "relay" if flag else "accumulate"


def fuzz_m93a0(mu, heap, r):
    """CDrive::M93A0 (this, a, out, b, c, d), void: the M9120 objects, the gear's ratio (zero a fifth of the time), the
    flag +0x55c, the word +0x368 (-1, 0 or 1), the trigger +0x578, the lane's words +0x664 and +0x674 and the out float.
    a is NaN or random; b, c and d are positive."""
    _, writes, this = fuzz_m9120(mu, heap, r)
    state_at, def_at, out = heap + 0x8000, heap + 0x4000, heap + 0xB000
    gear = r.randrange(-1, 12)
    writes.append((state_at + 0x54C, struct.pack("<i", gear)))
    ratio = 0.0 if r.random() < 0.2 else r.uniform(-2.0, 2.0)
    writes.append((def_at + 0x694 + 0x50 * gear, struct.pack("<f", ratio)))
    writes += [
        (state_at + 0x55C, struct.pack("<I", r.choice((0, 1, 1)))),
        (state_at + 0x368, struct.pack("<i", r.choice((-1, 0, 0, 1)))),
        (state_at + 0x578, struct.pack("<I", r.choice((0, 1)))),
        (def_at + 0x664, struct.pack("<f", r.uniform(0.5, 2.0))),
        (def_at + 0x674, struct.pack("<f", r.uniform(-1.0, 2.0))),
        (out, struct.pack("<f", r.uniform(-1.0, 1.0))),
    ]
    a = r.choice((NAN, r.uniform(-2.0, 2.0), r.uniform(-0.5, 0.5)))
    return [bits(a), out, bits(r.uniform(0.0, 1.5)), bits(r.uniform(0.5, 2.0)), bits(r.uniform(0.5, 2.0))], writes, this


def m93a0_branch(args, windows):
    """'A1a' and 'A1b' (the first block, trigger clear: |a| not above b, or above), 'A2a' and 'A2b' (trigger set), 'B-set'
    and 'B-clear' (the flag with the word +0x368 set or clear), or 'tail'. Windows 1 and 2 are the state and the
    definition."""
    state, dfn = windows[1][3], windows[2][3]
    gear = struct.unpack_from("<i", state, 0x54C)[0]
    ratio = struct.unpack_from("<f", dfn, 0x694 + 0x50 * gear)[0]
    flag = struct.unpack_from("<I", state, 0x55C)[0]
    word = struct.unpack_from("<i", state, 0x368)[0]
    a, b, c, d = (struct.unpack("<f", struct.pack("<I", x))[0] for x in (args[1], args[3], args[4], args[5]))
    if ratio != 0.0 and flag and word == 0:
        if not struct.unpack_from("<I", state, 0x578)[0]:
            return "A1b" if abs(a) > b else "A1a"
        f7 = f32_round(struct.unpack_from("<f", state, 0x384)[0] * c)
        f5 = f32_round(f32_round(struct.unpack_from("<f", dfn, 0x674)[0] + struct.unpack_from("<f", dfn, 0x664)[0])
                       * f32_round(f7 - struct.unpack_from("<f", state, 0x45C)[0]))
        f6 = f32_round(a - f32_round(f5 / d))
        return "A2b" if abs(f6) > b else "A2a"
    if ratio != 0.0 and flag:
        return "B-set" if word else "B-clear"
    return "tail"


def fuzz_m7c10(mu, heap, r):
    """CDrive::M7C10 (this), void: the M5D50 and M5BE0 inputs (the indices and gates, as in their recipes), the counter
    +0x5a0 and the word +0x5bc. The matrix at +0x470 is random; E39_Step4990 (an inner call) writes its inverse at +0x4b0."""
    _, writes, this = fuzz_m5d50(mu, heap, r)
    def_at, state_at = heap + 0x4000, heap + 0x8000
    for x in range(4):
        writes.append((def_at + 0x12C + 0xF0 * x + 0x2C, struct.pack("<I", r.randrange(4))))
        writes.append((def_at + 0x60 * x + 0x18, struct.pack("<i", r.randrange(1, 6))))
        writes.append((def_at + 0x60 * x + 0x68, struct.pack("<I", r.randrange(4))))
        writes.append((state_at + 200 * x + 0xA4, struct.pack("<I", r.choice((0, 1)))))
        writes.append((state_at + 0x4F0 + 4 * x, struct.pack("<I", r.choice((0, 1)))))
    writes.append((state_at + 0x5A0, struct.pack("<f", r.uniform(-3.0, 300.0))))
    writes.append((state_at + 0x5BC, struct.pack("<I", r.getrandbits(32))))
    return [], writes, this


def fuzz_m7d60(mu, heap, r):
    """CDrive::M7D60 (this), int result: the M5D50 inputs, the lanes (pack kinds at +0x158 + 0xf0 k, the index words at
    +0x88, +0x98, +0xe8 and +0xf8, the scales), and the state's wheel words (gates at +0xa4 + 200 k, values at +0x94 +
    200 k, the signs at +0x14) with the packs' outputs (this+0xc0 + 4 k) zero half the time."""
    _, writes, this = fuzz_m5d50(mu, heap, r)
    def_at, state_at = heap + 0x4000, heap + 0x8000
    for k in range(4):
        writes.append((def_at + 0x158 + 0xF0 * k, struct.pack("<I", r.choice((0, 1, 2)))))
        writes.append((state_at + 0xA4 + 200 * k, struct.pack("<i", r.choice((-1, 0, 0, 1)))))
        writes.append((state_at + 0x94 + 200 * k, struct.pack("<f", r.choice((-1.0, 1.0, 0.0, r.uniform(-2.0, 2.0))))))
        writes.append((this + 0xC0 + 4 * k, struct.pack("<f", r.choice((0.0, 0.0, 0.5, -0.5)))))
    for at in (0x88, 0x98, 0xE8, 0xF8):
        writes.append((def_at + at, struct.pack("<I", r.randrange(4))))
    for at in (0x48, 0xA8, 0x108):
        writes.append((def_at + at, struct.pack("<f", r.uniform(0.5, 2.0))))
    for at in (0x78, 0xD8):
        writes.append((def_at + at, struct.pack("<i", r.randrange(-3, 4))))
    writes.append((state_at + 0x14, struct.pack("<i", r.choice((-1, 0, 1)))))
    writes.append((state_at + 0x28, struct.pack("<i", r.choice((0, 1, 2)))))
    writes.append((state_at + 0x3C, struct.pack("<i", r.choice((0, 1, 2)))))
    return [], writes, this


def fuzz_m9bc0(mu, heap, r):
    """CDrive::M9BC0 (this, dt), void: the M7D60 inputs (the lanes, gates and wheel values), the state's branch word
    (+0x5b8), the multipliers and thresholds (+0x18, +0x2c, +0x40), the wheel values +0x64 and +0x94 (by 200 k), this's
    +0xa0..0xac, and the object at this+0x170 (its words +0x18, +0x24, +0x30 and +0x3c pick E39_Step3870 when zero)."""
    _, writes, this = fuzz_m7d60(mu, heap, r)
    def_at, state_at = heap + 0x4000, heap + 0x8000
    obj = heap + 0xB000
    writes.append((this + 0x170, struct.pack("<I", obj)))
    for at in (0x18, 0x18 + 0x14, 0x18 + 0x28):
        writes.append((state_at + at, struct.pack("<f", r.uniform(0.1, 3.0))))
    writes.append((state_at + 0x5B8, struct.pack("<I", r.choice((0, 1, 1)))))
    for k in range(4):
        writes.append((state_at + 0x64 + 200 * k, struct.pack("<f", r.uniform(-2.0, 2.0))))
        writes.append((this + 0xA0 + 4 * k, struct.pack("<f", r.uniform(-3.0, 3.0))))
    for at in (0x0C, 0x10, 0x24, 0x38):
        writes.append((state_at + at, struct.pack("<f", r.uniform(-2.0, 2.0))))
    for at in (0x18, 0x24, 0x30, 0x3C):
        writes.append((obj + at, struct.pack("<I", r.choice((0, 1)))))
    dt = r.choice((NAN, r.uniform(0.001, 0.05), r.uniform(0.001, 0.05)))
    return [bits(dt)], writes, this


def fuzz_ratio_ca0(mu, heap, r):
    """E39_RatioCA0 (v, p), stdcall with an x87 result: the ring at p (the count 1 to 8, the index below the count, the
    values) and v, a random float or NaN."""
    p = heap + 0x3000
    n = r.randrange(1, 9)
    writes = [(p, struct.pack("<I", n)), (p + 4, struct.pack("<I", r.randrange(n)))]
    writes += [(p + 8 + 4 * k, struct.pack("<f", r.uniform(-2.0, 2.0))) for k in range(8)]
    v = r.choice((NAN, r.uniform(-2.0, 2.0), r.uniform(-2.0, 2.0)))
    return [bits(v), p], writes, 0


def fuzz_tab_e00(mu, heap, r):
    """E39_TabE00 (table, a, b, c, n), stdcall, x87 result: the lerp at (a - b) * n / (c - b) of the table (ten floats,
    the top knot reads table[9]). The callers keep b <= a < c, so the index stays below the last knot."""
    t = heap + 0x100
    b = r.randrange(0, 2000)
    c = b + r.randrange(1, 2000)
    a = r.choice((b, c - 1, b + r.randrange(0, c - b)))
    writes = [(t + 4 * k, struct.pack("<f", fuzz_pool(r, 10.0))) for k in range(10)]
    return [t, a, b, c, 9], writes, None


def tab_e00_branch(args, windows):
    """E39_TabE00: 'top' when the index is the last knot (9 - 1 = 8) or past it, else 'interp'."""
    a, b, c, n = signed(args[1]), signed(args[2]), signed(args[3]), signed(args[4])
    return "top" if int((a - b) * n / (c - b)) >= n - 1 else "interp"


def fuzz_worker_row(r, base):
    """A row of the array (0xb0 bytes): the breaks B0 < B2 < B1 (+0xc, +0x2c, +0x1c) and B3 < B1 (+0x3c), so both
    lerps are reachable, the values at +0x6c..+0x9c, the gain (+0x4c), the rate (+0x5c) and the counter's limit (+0xac)."""
    b0, b2, b1 = sorted(r.sample(range(0, 2000), 3))
    writes = [(base + 0xC, struct.pack("<I", b0)), (base + 0x2C, struct.pack("<I", b2)),
              (base + 0x1C, struct.pack("<I", b1)), (base + 0x3C, struct.pack("<I", r.randrange(0, b1)))]
    writes += [(base + 0x6C + 0x10 * k, struct.pack("<f", fuzz_pool(r, 10.0))) for k in range(4)]
    writes += [(base + 0x4C, struct.pack("<f", r.uniform(-2.0, 2.0))),
               (base + 0x5C, struct.pack("<f", r.uniform(-2.0, 2.0))),
               (base + 0xAC, struct.pack("<I", r.randrange(0, 4)))]
    return writes


def fuzz_tables(r, n):
    """The curve tables of the array (N): the three ten-knot tables at +0x59c, +0x5c4 and +0x5ec (the last reads
    +0x610 as a value too), so the whole run from +0x59c to +0x614 is fuzzed."""
    return [(n + 0x59C + 4 * k, struct.pack("<f", fuzz_pool(r, 10.0))) for k in range(30)]


def fuzz_worker_f60(mu, heap, r):
    """E39_WorkerF60 (dt, a, b, c, n, s), stdcall, int result: the row a, the state b (the position at +0x7c, the
    counter at +0xc4 against the row's limit) and the block c (+0x6c) and the array n (the tables and values)."""
    row, b, c, n = heap + 0x2000, heap + 0x8000, heap + 0xA000, heap + 0x4000
    writes = fuzz_worker_row(r, row) + fuzz_tables(r, n)
    writes += [(b + 0x7C, struct.pack("<I", r.randrange(0, 2200))),
               (b + 0x1C, struct.pack("<f", r.uniform(-3.0, 3.0))),
               (b + 0xC4, struct.pack("<I", r.randrange(0, 6)))]
    writes += [(c + 0x6C, struct.pack("<f", r.uniform(-2.0, 2.0)))]
    dt, s = r.uniform(-1.0, 2.0), r.uniform(-2.0, 2.0)
    return [bits(dt), row, b, c, n, bits(s)], writes, None


def fuzz_worker_4060(mu, heap, r):
    """E39_Worker4060 (dt, b, c, n, s), stdcall, x87 result: the row the car block's index (+0xdc of c) picks in the
    array n (at +0x4ec, 0xb0 each), the state b (its position at +0x7c, the counter at +0xc4) and the tables."""
    b, c, n = heap + 0x8000, heap + 0xA000, heap + 0x4000
    index = r.randrange(0, 3)
    row = n + 0x4EC + 0xB0 * index
    writes = fuzz_worker_row(r, row) + fuzz_tables(r, n)
    writes += [(c + 0xDC, struct.pack("<I", index)),
               (b + 0x7C, struct.pack("<I", r.randrange(0, 2200))),
               (b + 0x1C, struct.pack("<f", r.uniform(-3.0, 3.0))),
               (b + 0xC4, struct.pack("<I", r.randrange(0, 6)))]
    dt, s = r.uniform(-1.0, 2.0), r.uniform(-2.0, 2.0)
    return [bits(dt), b, c, n, bits(s)], writes, None


def fuzz_hook_4160(mu, heap, r):
    """E39_Hook4160 (a1, c, n, x, s), with `this` in ecx (the edx is unused), stdcall-style (ret 0x14), float result. `this`
    holds the drive object at +0x170 (its +0x80 is the gate). a1 is the state (the position and the lane words are
    IntLanes at +0x70 and +0x80), c the block, n the def and x the input block. The block's denominators stay away from
    zero, so the ratios are finite most of the time."""
    this, obj, a1, c, n, x = heap + 0x9000, heap + 0xB000, heap + 0x8000, heap + 0xA000, heap + 0x4000, heap + 0x2000
    index = r.randrange(0, 3)
    writes = fuzz_worker_row(r, n + 0x4EC + 0xB0 * index) + fuzz_tables(r, n)
    writes += [(this + 0x170, struct.pack("<I", obj)), (obj + 0x80, struct.pack("<I", r.choice((0, 1, 2))))]
    writes += [(c + 0xDC, struct.pack("<I", index)),
               (c + 0x3C, struct.pack("<f", r.uniform(0.5, 3.0))),
               (c + 0x7C, struct.pack("<f", r.uniform(0.5, 3.0)))]
    writes += [(c + off, struct.pack("<f", r.uniform(-2.0, 2.0)))
               for off in (0x6C, 0x8C, 0x9C, 0xBC, 0xCC, 0xEC)]
    writes += [(a1 + 0x7C, struct.pack("<I", r.randrange(0, 2200))),
               (a1 + 0x8C, struct.pack("<I", r.randrange(0, 2200))),
               (a1 + 0x1C, struct.pack("<f", r.uniform(-3.0, 3.0))),
               (a1 + 0xAC, struct.pack("<f", r.uniform(-3.0, 3.0))),
               (a1 + 0xC4, struct.pack("<I", r.randrange(0, 6)))]
    writes += [(x + 0x60, struct.pack("<f", r.uniform(-2.0, 2.0))),
               (x + 0x70, struct.pack("<f", r.uniform(-2.0, 2.0)))]
    return [a1, c, n, x, bits(r.uniform(-2.0, 2.0))], writes, this


def hook_4160_branch(args, windows):
    """E39_Hook4160: the gate (the object's +0x80) and the sign of the slope (a1's +0x1c)."""
    gate = struct.unpack_from("<I", windows[1][3], 0x80)[0] != 0
    slope = struct.unpack_from("<f", windows[2][3], 0x1C)[0]
    return ("gate" if gate else "nogate") + "/" + ("pos" if slope > 0.0 else "nonpos")


SPECS = {
    # args: t, x, lo, hi, n (stdcall, 0x14 bytes). The table is n floats at t (n clamped to 64 for the window).
    0x50D580: dict(direct_fuzz=fuzz_table_lerp, name="E41_TableLerp", popped=0x14, x87=True, branch=table_lerp_branch,
                   windows=[(0, lambda a: 4 * max(0, min(signed(a[4]), 64)))]),
    # args: x, o (stdcall, 8 bytes). Writes the record at o (0x18 bytes: +0x08, +0x0c, +0x10, +0x14).
    0x50D6F0: dict(direct_fuzz=fuzz_unkd6f0, name="E41_UnkD6F0", popped=8, x87=False, branch=unkd6f0_branch,
                   windows=[(1, lambda a: 0x18)]),
    # args: t, x, i, j, o1, o2 (stdcall, 0x18 bytes). The table: count at t, the records' pointer at t + 4, the
    # keys at +0 of the 0xc-byte records. Outputs: the knot index i, j = i - 1, and the two weights.
    0x50D220: dict(direct_fuzz=fuzz_blend_w, name="E41_BlendW", popped=0x18, x87=False, branch=blend_w_branch,
                   windows=[(0, lambda a: 8), (0, lambda a: 0x300, 4), (2, lambda a: 4), (3, lambda a: 4),
                            (4, lambda a: 4), (5, lambda a: 4)]),
    # args: none; this (thiscall). The engine's def (this+4) holds abs_scale at +0x282c, the state (this+8) the speed
    # at +0x188 and the value (CReal: dirty at +0x248, value at +0x24c), the option block (this+0x48) the gate at +0x58.
    0x50D410: dict(direct_fuzz=fuzz_engine, name="CEngine::Sim410", popped=0, x87=True, thiscall=True, branch=sim410_branch,
                   windows=ENGINE_WINDOWS),
    # Drive accessors (args: this, then the stack words). The drive's lanes are NTweak values: dirty at +8, value at
    # +0xc. Set30 scales its value by the definition's +0x620 into the lane at +0x64. Windows: the drive (its first
    # 0xc bytes hold the pointers to the definition, this+4, and the state, this+8), the state and the definition.
    0x504450: dict(name="CDrive::Set30", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    0x5044A0: dict(name="CDrive::Set44A0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    # no arguments: clears the lanes at this+0x10 and this+0x44 (dirty, then zero).
    0x502C00: dict(name="E39_ClearLanes", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x80)]),
    0x5047B0: dict(direct_fuzz=fuzz_set47b0, name="CDrive::Set47B0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    # no arguments; this (thiscall). The engine's +0x184 becomes 0.7 times its state's float at +0x200 plus 0.3 times
    # itself, +0x190 the options block's sum (+8, +0x18, +0x28, +0x38) halved and clamped to 0..1, then the emit (0x50ee90)
    # runs on the signal object at [0x893600] + 0x3cb8 and +0x194 is cleared. Windows: the engine, its state (+0x200) and
    # the options block.
    0x518CF0: dict(direct_fuzz=fuzz_sim8cf0, name="E43EngineSim::Sim8CF0", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x198), (0, lambda a: 0x204, 8), (0, lambda a: 0x40, 0x48)]),
    # E39_MgrB / E39_MgrD pointer accessors and copies (names ours).
    0x501FA0: dict(direct_fuzz=fuzz_mgr_b_word8, name="E39_MgrB_Word8", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)]),
    0x501FB0: dict(direct_fuzz=fuzz_mgr_b_idx, name="E39_MgrB_Entry", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)]),
    0x502060: dict(direct_fuzz=fuzz_mgr_b_idx, name="E39_MgrB_FieldAt", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)]),
    # args: this (the table), idx, out. Copies the 16-byte row idx to out.
    0x502460: dict(direct_fuzz=fuzz_copy_row, name="E39_MgrD_CopyRow", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x100), (2, lambda a: 0x10)]),
    0x504820: dict(name="CDrive::Set4820", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    # no arguments; eax: the gate. Windows: the drive (to +0x174, the worker pointer at +0x170), the worker object, the
    # state (to +0x584, the car pointer at +0x580), the def (the gear count), the car (its engine at +0x690, whose
    # state pointer is at +0x698) and that state (the float at +0x188).
    0x504840: dict(direct_fuzz=fuzz_test4840, name="CDrive::Test4840", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x174), (0, lambda a: 0x94, 0x170), (0, lambda a: 0x584, 8),
                            (0, lambda a: 0x658, 4), (0, lambda a: 0x6A0, 8, 0x580),
                            (0, lambda a: 0x190, 8, 0x580, 0x698)]),
    0x504520: dict(name="CDrive::Get4520", popped=0, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    0x504530: dict(direct_fuzz=fuzz_get4530, name="CDrive::Get4530", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    0x504540: dict(name="CDrive::Get4540", popped=0, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8), (0, lambda a: 0x658, 4)]),
    # args: this, i, a, b. The lanes i * 0x10 + 0xe8 and + 0x128 (dirty, then value at +0xc).
    # args: this, a, x, y. Four entries at this + 0x10 a, so the window covers 0x170.
    0x5043C0: dict(direct_fuzz=fuzz_set_two, name="CDrive::SetTwo", popped=0xC, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x170)]),
    # args: this, channel, value. Windows: the drive (to +0x174, which holds the worker pointer at +0x170), the worker
    # object (the enable word at +0x90), the state (the lock records) and the def (the maximum torque).
    0x504570: dict(direct_fuzz=fuzz_set4570, name="CDrive::Set4570", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x174), (0, lambda a: 0x94, 0x170), (0, lambda a: 0x574, 8),
                            (0, lambda a: 0x658, 4)]),
    # args: this, a. The floats of record a at the state (this+8): +0x64 (M6DB0) and +0x54 (M6DD0), returned as st0.
    0x506DB0: dict(direct_fuzz=fuzz_drive_record, name="CDrive::M6DB0", popped=4, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8)]),
    0x506DD0: dict(direct_fuzz=fuzz_drive_record, name="CDrive::M6DD0", popped=4, x87=True, thiscall=True,
                   windows=[(0, lambda a: 0x80), (0, lambda a: 0x574, 8)]),
    # CEngine kernels (thiscall; ENGINE_WINDOWS: this, def, state, options). The pedal is the CReal at this+0x14.
    0x50EB90: dict(direct_fuzz=fuzz_set_scratch08, name="CEngine::SetScratch08", popped=4, x87=False, thiscall=True, windows=ENGINE_WINDOWS),
    # args: this, a, b, c. The crank (state +0x178, +0x17c, +0x180) and the tick delta (*b) and its angle (*c).
    0x50E370: dict(direct_fuzz=fuzz_crank_tick, name="CEngine::SimE370", popped=0xC, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS + [(2, lambda a: 4), (3, lambda a: 4)]),
    0x50E470: dict(direct_fuzz=fuzz_rev_gate, name="CEngine::SimE470", popped=0, x87=True, thiscall=True, windows=ENGINE_WINDOWS),
    0x50E9F0: dict(direct_fuzz=fuzz_compliance, name="CEngine::SimE9F0", popped=0, x87=False, thiscall=True, windows=ENGINE_WINDOWS),
    # args: this, i, q. The exhaust row i of the definition; q: 12 floats.
    # args: this, x, y (x87 return). Entry x takes the key y; Sim80D0 (external) resets it first when the key is lower.
    0x518D90: dict(name="E43EngineSim::Sim8D90", popped=8, x87=True, thiscall=True, windows=ENGINE_WINDOWS),
    0x5180D0: dict(direct_fuzz=fuzz_sim80d0, name="E43EngineSim::Sim80D0", popped=4, x87=False, thiscall=True,
                   external=True, windows=ENGINE_WINDOWS + [(0, lambda a: 4, 0x48, 0)],
                   nodes=lambda mu, a: [(0x7CA668, 0x10)]),  # the roll table SimD870 scales
    # args: this, x. The damper and load stage: two DamperSolves, SimE060 and SimE1F0 (the blend tables at +0x9c, +0xa4).
    0x517D30: dict(direct_fuzz=fuzz_sim7d30, name="E43EngineSim::Sim7D30", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS, nodes=sim7d30_nodes),
    # args: this, x. The brake-side damper step: DamperMerge twice, the lerp of the definition's +0x27ac, +0x27bc.
    0x518610: dict(direct_fuzz=fuzz_sim8610, name="E43EngineSim::Sim8610", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    # args: this, x. The fuel and air terms: Sim410 and SimE9F0 on the engine, then the ratios and the clamp of opts+0x58.
    0x518B20: dict(direct_fuzz=fuzz_sim8b20, name="E43EngineSim::Sim8B20", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    # args: this, x. The car's damper step: two spring-damper steps and the car's walk (`S30` and `MB560`, host calls,
    # not fuzzed: the records keep the walk off), then Sim8610 and the clamps of both slots.
    0x518890: dict(direct_fuzz=fuzz_sim8890, name="E43EngineSim::Sim8890", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    # args: this, a, b. One engine step: the crank and the cycle terms, the sum returned in st0 (the asm's g9 + b),
    # Emit, SimE660 and the averages, the graphs and the FloorS32 log (host sinks), and Sim8890 and Sim8B20 on the cycle.
    # args: this, v. The options pointer (+0x48) takes v; eax is v.
    0x50D310: dict(direct_fuzz=fuzz_set48, name="CEngine::Set48", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    # args: this, a, b, c, d. The four words at the state's +0x168..+0x174; eax is the state.
    0x50D340: dict(direct_fuzz=fuzz_set_state4, name="CEngine::SetState4", popped=0x10, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    0x50D380: dict(direct_fuzz=fuzz_get48_58, name="CEngine::Get48_58", popped=0, x87=True, thiscall=True,
                   windows=ENGINE_WINDOWS),
    0x50E970: dict(direct_fuzz=fuzz_sim_e970, name="CEngine::SimE970", popped=0, x87=True, thiscall=True,
                   windows=ENGINE_WINDOWS),
    0x50E980: dict(direct_fuzz=fuzz_sim_e980, name="CEngine::SimE980", popped=0, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS),
    0x50E9A0: dict(direct_fuzz=fuzz_sim_e9a0, name="CEngine::SimE9A0", popped=0, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS, nodes=sim_e9a0_nodes),
    0x50EE60: dict(direct_fuzz=fuzz_sim_ee60, name="CEngine::SimEE60", popped=0, x87=True, thiscall=True,
                   windows=ENGINE_WINDOWS_660),
    0x50EB80: dict(direct_fuzz=fuzz_get_state188, name="CEngine::GetState188", popped=0, x87=True, thiscall=True,
                   windows=ENGINE_WINDOWS),
    0x518E20: dict(direct_fuzz=fuzz_sim8e20, name="E43EngineSim::Sim8E20", popped=8, x87=True, thiscall=True,
                   windows=SIM8E20_WINDOWS, nodes=sim8e20_nodes),
    # args: this (a), b, s, out. The lerp of two vec4 (`Vec4::lerp`): out = a + (b - a) * s.
    0x473C90: dict(name="E11_VecLerp", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10), (3, lambda a: 0x10)]),
    # args: this. The lane at +0x24 takes 1 - cos(angle) of the engine value at +0x20 (see engine::ebb0_angle).
    0x50EBB0: dict(name="CEngine::SimEBB0", popped=0, x87=False, thiscall=True, windows=ENGINE_WINDOWS),
    0x50EC40: dict(direct_fuzz=fuzz_exhaust, name="CEngine::SimEC40", popped=8, x87=True, thiscall=True,
                   windows=ENGINE_WINDOWS + [(2, lambda a: 0x30)]),
    0x50ECF0: dict(direct_fuzz=fuzz_stall, name="CEngine::SimECF0", popped=0, x87=False, thiscall=True, windows=ENGINE_WINDOWS),
    # args: this, x, u. The torque curve; the record u (8 floats) gets q[7].
    0x50ED70: dict(direct_fuzz=fuzz_torque, name="CEngine::SimED70", popped=8, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS + [(2, lambda a: 0x20)]),
    # args: this, a. The misfire roll: the seed word (option block +0 points at it), the level at this+0x194.
    0x50E520: dict(direct_fuzz=fuzz_misfire, name="CEngine::SimE520", popped=4, x87=False, thiscall=True,
                   windows=ENGINE_WINDOWS + [(0, lambda a: 4, 0x48, 0)]),
    # args: this, i. The slot's roll (eax: the quotient, the stall term, the options block, or 0). The table entry at
    # 0x7ca668 + 4 i is the one global it touches (a nodes window, 0x10 bytes for i < 4).
    0x50D870: dict(direct_fuzz=fuzz_d870, name="CEngine::SimD870", popped=4, x87=False, thiscall=True,
                   branch=sime_d870_branch,
                   windows=[(0, lambda a: 0x198), (0, lambda a: 0x274, 8), (0, lambda a: 0x88, 0x48),
                            (0, lambda a: 4, 0x48, 0)],
                   nodes=lambda mu, a: [(0x7CA668, 0x10)]),
    # args: a, b, c, d, e, f, g (stdcall). b and c are blend tables (count at b+4, records at +8); the outputs f, g
    # and h are floats and an int. The records' keys are in b and c, the weights d and e.
    0x50CED0: dict(direct_fuzz=fuzz_blend_x, name="E41_BlendX", popped=0x20, x87=False,
                   windows=[(1, lambda a: 0x10), (1, lambda a: 0x60, 8), (2, lambda a: 0x10), (2, lambda a: 0x60, 8),
                            (5, lambda a: 4), (6, lambda a: 4), (7, lambda a: 4)]),
    0x50D090: dict(direct_fuzz=fuzz_blend_y, name="E41_BlendY", popped=0x1C, x87=False,
                   windows=[(1, lambda a: 0x10), (1, lambda a: 0x60, 8), (2, lambda a: 0x10), (2, lambda a: 0x60, 8),
                            (5, lambda a: 4), (6, lambda a: 4)]),
    # args: this, a. The torque at a (x87 result); the def's load ratios and threshold, the state's loads and speed.
    0x50E660: dict(direct_fuzz=fuzz_sime660, name="CEngine::SimE660", popped=4, x87=True, thiscall=True,
                   branch=sime660_branch, windows=ENGINE_WINDOWS_660),
    # args: t, x, n (stdcall, 0xc bytes). The curve is n records of 0x20 bytes; the search reads one record past
    # the last (the value at +0x1c of the record n).
    0x50D7A0: dict(direct_fuzz=fuzz_curve_eta, name="E41_CurveEta", popped=0xC, x87=True, branch=curve_eta_branch,
                   windows=[(0, lambda a: 0x20 * (max(0, min(signed(a[2]), 64)) + 1))]),
    # args: this, a, b, c, d, e, f, g. The smoothed damper: BlendW on the outer table (argument 4, its records
    # windowed through their sub-tables), BlendX on the two sub-tables, DamperSqrt4/Scale/UpdateA/Combine. The f block
    # (argument 6) and the g output (argument 7) are written; the state's smoothed value at +0x1e4 (dirty +0x1ec).
    0x50E060: dict(direct_fuzz=fuzz_smooth, name="CEngine::SimE060", popped=0x1C, x87=False, thiscall=True,
                   branch=sime060_branch,
                   windows=ENGINE_WINDOWS + [(4, lambda a: 8), (4, lambda a: 0x300, 4), (6, lambda a: 0x10),
                                             (7, lambda a: 4)],
                   nodes=sub_tables(4)),
    # args: this, a, b, c, d, e, f. The solve damper: BlendW and BlendY on the outer table (argument 3), DamperSolve on
    # the solve block (argument 5, four words), the f output (argument 6); the state's solved value at +0x200.
    0x50E1F0: dict(direct_fuzz=fuzz_solve, name="CEngine::SimE1F0", popped=0x18, x87=False, thiscall=True,
                   branch=sime1f0_branch,
                   windows=ENGINE_WINDOWS + [(3, lambda a: 8), (3, lambda a: 0x300, 4), (5, lambda a: 0x10),
                                             (6, lambda a: 4)],
                   nodes=sub_tables(3)),
    # args: this, idx, t, out. The cubic Hermite between entries idx and idx+1 (their q and v, 0x50 bytes).
    0x50A650: dict(direct_fuzz=fuzz_blend_b, name="CDriveLine::BlendB", popped=0xC, x87=False, thiscall=True,
                   windows=[(3, lambda a: 0x10)], nodes=blend_b_nodes),
    # args: this. The arc length of the last entry (a float at st0); the count and that entry's s.
    0x50AD40: dict(direct_fuzz=fuzz_last, name="CDriveLine::Last", popped=0, x87=True, thiscall=True,
                   windows=[], nodes=last_nodes),
    # args: this, id. The naked trampoline: ecx from 0x7eaba8, then E12_SampleX(id), whose st0 is the result
    # (see thunk_nodes).
    0x50A4F0: dict(direct_fuzz=fuzz_thunk, name="CDriveLine::Thunk4F0", popped=4, x87=True, thiscall=True,
                   windows=[], nodes=thunk_nodes),
    # args: this, x, out. BlendB at the integer part of x and the fraction; the branches are clamp_branch's.
    0x50B0F0: dict(direct_fuzz=fuzz_clamp_mix, name="CDriveLine::ClampMix", popped=8, x87=False, thiscall=True,
                   branch=clamp_branch, windows=[(2, lambda a: 0x10)], nodes=clamp_nodes),
    # args: this, sig. Copies the signal's entries, sets the count and fills the rest of the table (see
    # set_drive_line_nodes for the windows: the far entries are sampled).
    # args: this, pos, x (st0). The arc position of pos (three floats) bisected from x; the table is read in full.
    0x50B300: dict(direct_fuzz=fuzz_sample_big, name="CDriveLine::SampleBig", popped=8, x87=True, thiscall=True,
                   windows=[(1, lambda a: 0x10)], nodes=sample_big_nodes),
    0x50AEC0: dict(direct_fuzz=fuzz_set_drive_line, name="CDriveLine::SetDriveLine", popped=4, x87=False,
                   thiscall=True, windows=[(1, lambda a: 8)], nodes=set_drive_line_nodes),
    # args: this, x. The lane scale and the word (see drive_leaf.rs). Window 1 is the definition (this+4).
    0x505970: dict(direct_fuzz=fuzz_m5970, name="CDrive::M5970", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x84), (0, lambda a: 0x640, 4), (1, lambda a: 0x20)]),
    # args: this. The speed over the gear's ratio less the definition's +0xa54 times the state's +0x384 (x87).
    0x505A10: dict(direct_fuzz=fuzz_m5a10, name="CDrive::M5A10", popped=0, x87=True, thiscall=True,
                   branch=m5a10_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x560, 8), (0, lambda a: 0xA60, 4)]),
    # args: this. The flag matrix at the state's +0x4f0 (see drive_leaf.rs); windows 1 and 2 are the state and the
    # definition, with the state's pair entries (200 bytes each, gates at +0xa4) inside the first 0x5c0 bytes.
    0x505D50: dict(direct_fuzz=fuzz_m5d50, name="CDrive::M5D50", popped=0, x87=False, thiscall=True,
                   branch=m5d50_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x5C0, 8), (0, lambda a: 0x140, 4)]),
    # args: this, x. The slip value of wheel x (see drive_leaf.rs): windows 1 and 3 are the flags the pointer at
    # +0x170 names and the state; window 2 is the definition.
    0x505FE0: dict(direct_fuzz=fuzz_m5fe0, name="CDrive::M5FE0", popped=4, x87=False, thiscall=True,
                   branch=m5fe0_branch,
                   windows=[(0, lambda a: 0x180), (0, lambda a: 0x40, 0x170), (0, lambda a: 0x1000, 4),
                            (0, lambda a: 0x800, 8)]),
    # args: this, x. The clutch-pack values of wheel x (see drive_leaf.rs). Window 1 is the state, window 2 the def.
    0x505BE0: dict(direct_fuzz=fuzz_m5be0, name="CDrive::M5BE0", popped=4, x87=False, thiscall=True,
                   branch=m5be0_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x500, 8), (0, lambda a: 0x800, 4)]),
    # args: this, x. The clamp-and-latch of wheel x (see drive_leaf.rs). Windows 1 to 3 are the flags the pointer at
    # +0x170 names, the definition and the state.
    0x506260: dict(direct_fuzz=fuzz_m6260, name="CDrive::M6260", popped=4, x87=False, thiscall=True,
                   branch=m6260_branch,
                   windows=[(0, lambda a: 0x180), (0, lambda a: 0x40, 0x170), (0, lambda a: 0xA60, 4),
                            (0, lambda a: 0x5C0, 8)]),
    # args: this. Two axle pairs (see drive_leaf.rs): the pairs' indices are in the definition, the sums and gates in the
    # state. Windows 1 and 2 are the state and the definition.
    0x506630: dict(direct_fuzz=fuzz_m6630, name="CDrive::M6630", popped=0, x87=False, thiscall=True,
                   branch=m6630_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x560, 8), (0, lambda a: 0xA60, 4)]),
    # args: this, x. Wheel x's block k gets the lane value (see drive_leaf.rs). Windows 1 and 2 are the state and the
    # definition.
    0x506A00: dict(direct_fuzz=fuzz_m6a00, name="CDrive::M6A00", popped=4, x87=False, thiscall=True,
                   branch=m6a00_branch, windows=[(0, lambda a: 0x10), (0, lambda a: 0x560, 8), (0, lambda a: 0xA60, 4)]),
    # args: this, x. Wheel x's blocks k and m (see drive_leaf.rs). Windows 0 to 2 are this (with the outputs), the state and
    # the definition.
    0x506B20: dict(direct_fuzz=fuzz_m6b20, name="CDrive::M6B20", popped=4, x87=False, thiscall=True,
                   branch=m6b20_branch, windows=[(0, lambda a: 0x180), (0, lambda a: 0x560, 8), (0, lambda a: 0xA60, 4)]),
    # args: this, a (a float, on the stack; the result is on x87). The clamp of the speed term and the object update (see
    # drive_leaf.rs). Windows 0 to 2 are this, the state and the object at this+0x170.
    0x506C90: dict(direct_fuzz=fuzz_m6c90, name="CDrive::M6C90", popped=4, x87=True, thiscall=True,
                   branch=m6c90_branch, windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x560, 8), (0, lambda a: 0x90, 0x170)]),
    # args: this, x. The gear-change emit (see drive_worker.rs). Get30 and the emit are external: the emit's inner record
    # keeps its 16-byte payload (argument 1).
    0x5079B0: dict(direct_fuzz=fuzz_m79b0, name="CDrive::M79B0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (0, lambda a: 0x590, 8)]),
    # args: this, dt (a float, on the stack; the result is an int). The gear guard (see drive_worker.rs). Windows 0 to 3
    # are this (with +0x170), the state, the definition and the object at this+0x170; 4 to 6 are the physics global, its
    # CDef and the flags. M79B0 runs inside it (its Get30 and emit are the external calls).
    0x509120: dict(direct_fuzz=fuzz_m9120, name="CDrive::M9120", popped=4, x87=False, thiscall=True,
                   branch=m9120_branch, nodes=m9120_nodes,
                   windows=[(0, lambda a: 0x174), (0, lambda a: 0x590, 8), (0, lambda a: 0xA60, 4), (0, lambda a: 0x90, 0x170)]),
    # args: this, dt (a float, on the stack; void). The gear-logic worker (see drive_worker.rs); the same windows as M9120,
    # and the state's RNG word is in the state window.
    0x509210: dict(direct_fuzz=fuzz_m9210, name="CDrive::M9210", popped=4, x87=False, thiscall=True,
                   branch=m9210_branch, nodes=m9120_nodes,
                   windows=[(0, lambda a: 0x1C8), (0, lambda a: 0x590, 8), (0, lambda a: 0xA60, 4), (0, lambda a: 0x90, 0x170)]),
    # args: this, dt (a float, on the stack; void). The gear trim (see drive_worker.rs). Windows 0 to 3 as in M9210, with
    # the definition's slope words (+0xa74, +0xa84) in window 2 and the object's word +0x78 in window 3.
    0x508570: dict(direct_fuzz=fuzz_m8570, name="CDrive::M8570", popped=4, x87=False, thiscall=True,
                   branch=m8570_branch,
                   windows=[(0, lambda a: 0x1C0), (0, lambda a: 0x590, 8), (0, lambda a: 0xA90, 4), (0, lambda a: 0x80, 0x170)]),
    # args: this, a, out, b, c, d (five stack words; void). The clutch-torque worker (see drive_worker.rs). Windows 0 to 3
    # as in M9210; window 4 is the out float (args[2]); the physics global chain is read by M9120 (nodes).
    0x5093A0: dict(direct_fuzz=fuzz_m93a0, name="CDrive::M93A0", popped=0x14, x87=False, thiscall=True,
                   branch=m93a0_branch, nodes=m9120_nodes,
                   windows=[(0, lambda a: 0x1C8), (0, lambda a: 0x590, 8), (0, lambda a: 0xA90, 4), (0, lambda a: 0x90, 0x170),
                            (2, lambda a: 4)]),
    # args: this (the reset worker, see drive_worker.rs). Windows 0 to 2: this (with +0xc0 and the lanes), the state (the
    # matrix at +0x470, +0x4b0, the counter +0x5a0 and the word +0x5bc) and the definition. E39_Step4990 (group 7) is
    # external: its matrix and out windows are the inner call's windows.
    0x507C10: dict(direct_fuzz=fuzz_m7c10, name="CDrive::M7C10", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8), (0, lambda a: 0xA60, 4)]),
    0x504990: dict(name="E39_Step4990", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x40), (1, lambda a: 0x40)]),
    # args: this (the clutch-state machine, see drive_worker.rs). Windows 0 to 2: this (with the packs' outputs), the state
    # and the definition. M6A00 and M6B20 are ports and run inside it.
    0x507D60: dict(direct_fuzz=fuzz_m7d60, name="CDrive::M7D60", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8), (0, lambda a: 0xA60, 4)]),
    # args: this, dt (a float, on the stack; void). The wheel worker of the drive (see drive_worker.rs). M7C10 and M7D60 run
    # inside it; E39_MatVec and E39_Step3870 are external, their windows are the inner call's windows.
    # CDrive::Step1E0 (ecx = the drive, dt on the stack; ret 4): the drive's step. Its external calls (the car
    # forwarder, and the gear and wheel callees made by the listed workers) are its inner records, in call order.
    0x50A1E0: dict(name="CDrive::Step1E0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8), (0, lambda a: 0xA60, 4), (0, lambda a: 0x90, 0x170)]),
    # CCar::Fwd18E20 (0x4e6650: this += 0x690, then E43's Sim8E20, ret 8): the car forwarder, st0.
    0x4E6650: dict(name="CCar::Fwd18E20", popped=8, x87=True, thiscall=True, external=True, windows=[]),
    0x509BC0: dict(direct_fuzz=fuzz_m9bc0, name="CDrive::M9BC0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8), (0, lambda a: 0xA60, 4), (0, lambda a: 0x60, 0x170)]),
    # E39_MatVec (ecx = m, then v and out on the stack; ret 8): the 4x4 matrix, the vector and the output.
    0x502400: dict(name="E39_MatVec", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x40), (1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # E39_Step3870 (ecx = self, then the lane index and dt on the stack; ret 8): the drive's lane step.
    0x503870: dict(name="E39_Step3870", popped=8, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x1D0), (0, lambda a: 0x5C0, 8)]),
    # args: v, p (stdcall, the ring for the x87 result; see drive_worker.rs ratio_ca0). The ring is the window at p.
    # E39_TabE00 (table, a, b, c, n; stdcall, ret 0x14): the table lerp at (a - b) * n / (c - b) on the x87 (the x87 result).
    0x503E00: dict(direct_fuzz=fuzz_tab_e00, name="E39_TabE00", popped=0x14, x87=True, thiscall=False,
                   branch=tab_e00_branch, windows=[(0, lambda a: 0x28)]),
    # E39_WorkerF60 (dt, a, b, c, n, s; stdcall, ret 0x18): the counter step of a row (int result).
    0x503F60: dict(direct_fuzz=fuzz_worker_f60, name="E39_WorkerF60", popped=0x18, x87=False, thiscall=False,
                   windows=[(1, lambda a: 0xB0), (2, lambda a: 0xC8), (3, lambda a: 0x70), (4, lambda a: 0x620)]),
    # E39_Worker4060 (dt, b, c, n, s; stdcall, ret 0x14): the row's value at the state's position (x87 result).
    0x504060: dict(direct_fuzz=fuzz_worker_4060, name="E39_Worker4060", popped=0x14, x87=True, thiscall=False,
                   windows=[(1, lambda a: 0xC8), (2, lambda a: 0xE0), (3, lambda a: 0x800)]),
    # E39_Hook4160 (self, a1, c, n, x, s; ret 0x14, fastcall with an unused edx): the car's speed term (float result).
    0x504160: dict(direct_fuzz=fuzz_hook_4160, name="E39_Hook4160", popped=0x14, x87=True, thiscall=True,
                   branch=hook_4160_branch,
                   windows=[(0, lambda a: 0x174), (0, lambda a: 0x84, 0x170), (1, lambda a: 0xC8),
                            (2, lambda a: 0xF0), (3, lambda a: 0x800), (4, lambda a: 0x74)]),
    0x503CA0: dict(direct_fuzz=fuzz_ratio_ca0, name="E39_RatioCA0", popped=8, x87=True, thiscall=False,
                   windows=[(1, lambda a: 0x28)]),
    0x4E67F0: dict(name="CCar::Get30", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x506F20: dict(name="CMultiSignal1<CSigMrRCarGearChange>::Emit", popped=4, x87=False, thiscall=True,
                   external=True, windows=[(1, lambda a: 16)]),
}
