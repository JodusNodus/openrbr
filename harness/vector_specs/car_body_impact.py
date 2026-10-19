# Car-body impact path (crates/rbr-physics/src/car_body/layout.rs zone_scaled, apply_functors, engine_gate, ring_store,
# damage_rand): the callees of E36DamageOps::Slot1 (0x4f12a0) whose branches the impact chains do not all reach (the
# engine gate's and the speech ring's open paths, the spread links). Direct calls on a handler in the stub heap. Run
# with `--set car_body_impact`. The functor in ApplyFunctors' tables is DmgD020 (an external record here, its writes the
# run after it), and the two emits are external records whose emitter has no source (direct calls run externals: the
# emit returns at once). The popped bytes are the callee's `ret` immediates.

import struct  # noqa: E402

SET_NAME = "car_body_impact"

HANDLER_WINDOW = 0x3E70  # the zones, the functor tables, the floats and the tail up to the hit
FUNCTOR = 0x4ED020  # CCarDamageHandler::DmgD020 (writes +0x3a88, +0x3aa0 and the ring; returns 5 or 0)
SIGNALS = 0x893600
ENGINE_FIRE_EMIT = 0x4EF290
SPEECH_EMIT = 0x4AE0D0
LCG_MUL, LCG_ADD = 0x2000081, 0xE623F41D
LCG_INV = pow(LCG_MUL, -1, 1 << 32)


def f32(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def float_word(r, lo=-0.5, hi=1.5):
    return f32(r.choice((r.uniform(lo, hi), r.uniform(lo, hi), r.uniform(lo, hi), 0.0, float("nan"))))


def seed_word(r):
    """The handler's seed: random, or one whose next draw is below 5000 (E35_Rand below 0.05, the fire path)."""
    if r.random() < 0.4:
        target = r.randrange(0, 5000) + 100001 * r.randrange(0, 42000)
        return ((target - LCG_ADD) * LCG_INV) & 0xFFFFFFFF
    return r.getrandbits(32)


def handler_words(r):
    """The handler's words: zones with their scales, levels and links, functor tables of the stand-in, the floats."""
    words = [r.getrandbits(32) if r.random() < 0.2 else 0 for _ in range(HANDLER_WINDOW // 4)]
    for z in range(0x40):
        base = (0x2208 + 0x58 * z) // 4
        words[base] = float_word(r)  # the engine share
        words[base + 2] = float_word(r, 0.0, 2.0)  # the scale
        words[base + 3] = float_word(r, 0.0, 1.5)  # the level
        for k in range(4):
            words[base + 6 + 2 * k] = float_word(r, 0.0, 1.5)  # the spread factor
            words[base + 7 + 2 * k] = r.choice((0, r.randrange(1, 0x40), r.randrange(1, 0x40)))
            words[base + 14 + 2 * k] = float_word(r, 0.0, 1.5)  # the functor link's factor
            words[base + 15 + 2 * k] = r.choice((0, r.randrange(1, 0x40), 0x40))
        words[(0x3808 + 4 * z) // 4] = float_word(r, 0.0, 1.2)
    for t in range(0x40):
        table = (8 + 0x88 * t) // 4
        count = r.randrange(0, 5)
        words[table + 0x20] = count
        for k in range(4):
            entry = table + 8 * k
            words[entry] = FUNCTOR
            words[entry + 1] = 0  # the functor's this is the handler
            words[entry + 4] = float_word(r)
            words[entry + 5] = float_word(r)  # the weight
            words[entry + 6] = r.getrandbits(32)  # the tag
    words[0x3DE8 // 4] = seed_word(r)
    words[0x3DF0 // 4] = r.choice((0, 1, 2, 3, 3, 4, 0xFFFFFFFF))
    words[0x3A90 // 4] = r.choice((0, 1))
    words[0x3B20 // 4] = r.choice((0, 1, 1))
    words[0x3E58 // 4] = r.choice((r.randrange(0, 10), r.randrange(0, 10), 12))
    return struct.pack("<%dI" % len(words), *words)


def zone_scaled_fuzz(mu, heap, r):
    this = heap + 0x20000
    return [r.randrange(1, 0x40), float_word(r, 0.0, 3.0)], [(this, handler_words(r))], this


def apply_functors_fuzz(mu, heap, r):
    """ApplyFunctors(idx, scale, best): the tables' count is 0 to 4 (the window covers tables 0 to 63), best in the heap."""
    this, best = heap + 0x20000, heap + 0x40000
    writes = [(this, handler_words(r)), (best, struct.pack("<I", float_word(r)))]
    return [r.randrange(0, 0x40), float_word(r, 0.0, 3.0), best], writes, this


def silent_emitter(mu, offset):
    """The emitter at `offset` on the signal object, its source pointer (+4) cleared: the emit returns at once."""
    signals = struct.unpack("<I", bytes(mu.mem_read(SIGNALS, 4)))[0]
    return signals + offset + 4, struct.pack("<I", 0)


def engine_gate_fuzz(mu, heap, r):
    """EngineGate(a, 11000, 1.0, 0): `a` around the limit."""
    this = heap + 0x20000
    a = f32(r.choice((r.uniform(0, 22000), r.uniform(11000, 30000), 11000.0, float("nan"))))
    b = r.choice((f32(11000.0), float_word(r, 0, 20000)))
    return [a, b, f32(1.0), 0], [(this, handler_words(r)), silent_emitter(mu, 0xB840)], this


def ring_store_fuzz(mu, heap, r):
    this = heap + 0x20000
    writes = [(this, handler_words(r)), silent_emitter(mu, 0xCF48)]
    return [float_word(r), float_word(r), float_word(r), r.getrandbits(32)], writes, this


def rand_fuzz(mu, heap, r):
    """E35_Rand(lo, hi, seed) (cdecl): the seed word in the heap."""
    seed = heap + 0x40000
    lo = r.choice((0.0, r.uniform(-2, 2)))
    hi = r.choice((1.0, r.uniform(-2, 4)))
    return [f32(lo), f32(hi), seed], [(seed, struct.pack("<I", seed_word(r)))], 0


def functor_after(mu, args, eax):
    """DmgD020's writes: +0x3a88 to +0x3e60 of its handler (+0x3a88, +0x3aa0, the ring and its index)."""
    return [(args[0] + 0x3A80, 0x3E0)]


def direct(name, popped, recipe, windows, x87=False, thiscall=True, **extra):
    return dict(name=name, popped=popped, x87=x87, thiscall=thiscall, windows=windows, direct_fuzz=recipe, **extra)


HANDLER = [(0, lambda a: HANDLER_WINDOW)]
SPECS = {
    0x4EDEE0: direct("CCarDamageHandler::ZoneScaled (4edee0)", 8, zone_scaled_fuzz, HANDLER),
    0x4EC560: direct("CCarDamageHandler::ApplyFunctors (4ec560)", 0xC, apply_functors_fuzz,
                     HANDLER + [(3, lambda a: 4)]),
    0x4EF680: direct("E36DamageOps::EngineGate (4ef680)", 0x10, engine_gate_fuzz, HANDLER),
    0x4EF5F0: direct("E36DamageOps::RingStore (4ef5f0)", 0x10, ring_store_fuzz, HANDLER),
    0x4EBD70: direct("E35_Rand (4ebd70)", 0, rand_fuzz, [(2, lambda a: 4)], x87=True, thiscall=False, cdecl=True,
                     stack_words=3),
    FUNCTOR: dict(name="CCarDamageHandler::DmgD020 (4ed020)", popped=0x10, x87=False, thiscall=True, external=True,
                  windows=[], after=functor_after),
    ENGINE_FIRE_EMIT: dict(name="CSigEngineFire emit (4ef290)", popped=4, x87=False, thiscall=True, external=True,
                           windows=[]),
    SPEECH_EMIT: dict(name="CSigCarDamageSpeech emit (4ae0d0)", popped=4, x87=False, thiscall=True, external=True,
                      windows=[]),
}
