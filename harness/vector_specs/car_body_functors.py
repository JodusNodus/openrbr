# Car-body damage functors (rust/rbr-physics/src/car_body/damage_functors.rs, layout.rs dmg_*): the leaves that
# E36DamageOps::Slot1's ApplyFunctors calls through the handler's table. No car step reaches them yet (Slot1 is not
# ported), so these are direct calls on a handler in the stub heap. Run with `--set car_body_functors`.
# Each recipe fills the handler's 0x3e60 bytes with random words (zeros, small ints and floats in -0.5..1.5) and
# gives the level (0 to 3), the threshold and the ring cursor their own values. The popped bytes are `ret 0x10`.

import struct  # noqa: E402

SET_NAME = "car_body_functors"

HANDLER_WINDOW = 0x3E60  # the words the functors read and write: 0x3a00 to the ring's end


def word_bytes(r):
    """One random handler word: zero, a small int, or a float in -0.5..1.5 (the gates see all three)."""
    roll = r.random()
    if roll < 0.3:
        return 0
    if roll < 0.6:
        return struct.unpack("<I", struct.pack("<f", r.uniform(-0.5, 1.5)))[0]
    return r.randrange(1, 4)


def handler_bytes(r):
    """The handler's bytes: random words, the level (+0x3df0), the threshold (+0x3dec) and the ring cursor (+0x3e58)."""
    words = [word_bytes(r) for _ in range(HANDLER_WINDOW // 4)]
    words[0x3DF0 // 4] = r.randrange(0, 4)
    words[0x3DEC // 4] = struct.unpack("<I", struct.pack("<f", r.choice((0.0, r.uniform(0.2, 2.0)))))[0]
    words[0x3E58 // 4] = r.randrange(0, 10)
    return struct.pack("<%dI" % len(words), *words)


def float_word(r):
    return struct.unpack("<I", struct.pack("<f", r.uniform(-0.5, 1.5)))[0]


def functor_recipe(index_range):
    """A direct recipe: `a`, `b`, `c` are floats, the fourth word is the index (from `index_range`) or a random tag."""

    def recipe(mu, heap, r):
        this = heap + 0x20000
        fourth = r.getrandbits(32) if index_range is None else r.choice(index_range)
        words = [float_word(r), float_word(r), float_word(r), fourth]
        return words, [(this, handler_bytes(r))], this

    return recipe


def damage_stress_recipe(addr, index_range=None):
    """Cover gate hits, NaNs in the ordered float comparisons, state words, and signed levels where the asm checks them."""
    call_index = 0
    pair_bases = {0x4ECCB0: 0x3BA0, 0x4ECD30: 0x3B80, 0x4ECDB0: 0x3BC0, 0x4ECE30: 0x3BE0}
    state_offsets = {0x4EC720: 0x3B10, 0x4ED160: 0x3B0C}
    cases = (
        (2.0, 1.0, 0.5),
        (float("nan"), 1.0, 0.5),
        (2.0, float("nan"), 0.5),
        (2.0, 1.0, float("nan")),
        (0.0, 1.0, 0.5),
        (2.0, 1.0, 0.5),
        (1.0, 1.0, 0.5),
        (2.0, 1.0, 0.5),
    )

    def recipe(mu, heap, r):
        nonlocal call_index
        n = call_index
        call_index += 1
        this = heap + 0x20000
        a, b, c = cases[n % len(cases)]
        fourth = n % len(index_range) if index_range is not None else r.getrandbits(32)
        words = [struct.unpack("<I", struct.pack("<f", x))[0] for x in (a, b, c)] + [fourth]
        handler = bytearray(handler_bytes(r))

        if addr == 0x4ED160:
            # Negative signed levels have otherwise-passing comparisons and state, so an unsigned `jl` port would fail these.
            levels = (2, 0xFFFFFFFF, 2, 3, 0xFFFFFFFF, 2, 0x80000000, 3)
            struct.pack_into("<I", handler, 0x3DF0, levels[n % len(levels)])
            if n % len(levels) in (1, 4, 6):
                a, b, c = 2.0, 1.0, 0.5
            elif n % len(levels) == 7:
                a, b, c = float("nan"), 1.0, 0.5
            words[:3] = [struct.unpack("<I", struct.pack("<f", x))[0] for x in (a, b, c)]

        if addr in state_offsets:
            states = (0.5, 1.0, 1.25, float("nan"))
            if addr == 0x4EC720:
                states = (0.5, 0.5, 0.5, 0.5, 0.5, 1.0, 1.25, float("nan"))
            if addr == 0x4ED160:
                state = (0.5, 0.5, 0.5, 0.5, 0.5, (1.0, 1.25, float("nan"))[(n // 8) % 3], 0.5, 0.5)[n % 8]
            else:
                state = states[n % len(states)]
            struct.pack_into("<f", handler, state_offsets[addr], state)

        if addr in pair_bases:
            i = fourth
            first = 1 if n % 4 == 0 else 0
            second = 1 if n % 4 == 1 else 0
            struct.pack_into("<II", handler, pair_bases[addr] + 8 * i, first, second)

        if addr == 0x4ED0F0:
            struct.pack_into("<I", handler, 0x3AA8, 1 if n % 2 == 0 else 0)

        return words, [(this, bytes(handler))], this

    return recipe


def zero_functor_recipe(mu, heap, r):
    """E35_Zero2F0 has four ignored stack words and does not read a handler."""
    return [r.getrandbits(32) for _ in range(4)], [], heap + 0x20000


def spec(name, index_range):
    return dict(name=name, popped=0x10, x87=False, thiscall=True, windows=[(0, lambda a: HANDLER_WINDOW)],
                direct_fuzz=functor_recipe(index_range))


def head_gasket_recipe(mu, heap, r):
    """DmgCFA0: the functor recipe, with a negative level on some calls (the gate's `jl` is signed) and the indexed word
    (+0x3dd8 + 4 i) at 1 on half of them, so the pass is common."""
    words, writes, this = functor_recipe(range(0, 6))(mu, heap, r)
    handler = bytearray(writes[0][1])
    level = r.choice((0, 1, 2, 3, 2, 3, 0xFFFFFFFF, 0x80000000))
    struct.pack_into("<I", handler, 0x3DF0, level)
    if r.random() < 0.5:
        struct.pack_into("<I", handler, 0x3DD8 + 4 * words[3], 1)
    return words, [(this, bytes(handler))], this


def negative_level_recipe(addr, index_range=None):
    """A signed-gate regression input: the level is -1 and every later gate passes."""
    base = functor_recipe(index_range)

    def recipe(mu, heap, r):
        words, writes, this = base(mu, heap, r)
        words[:3] = [struct.unpack("<I", struct.pack("<f", x))[0] for x in (2.0, 1.0, 0.5)]
        if addr == 0x4ECB20:
            words[3] = 2
        handler = bytearray(writes[0][1])
        struct.pack_into("<I", handler, 0x3DF0, 0xFFFFFFFF)
        if addr == 0x4EC660:
            struct.pack_into("<f", handler, 0x3B00, 0.5)
        elif addr == 0x4EC890:
            struct.pack_into("<f", handler, 0x3B08, 0.75)
        elif addr == 0x4EC930:
            struct.pack_into("<I", handler, 0x3AF4, 1)
        elif addr == 0x4EC820:
            struct.pack_into("<I", handler, 0x3A94, 1)
        elif addr == 0x4ECB20:
            struct.pack_into("<f", handler, 0x3DEC, 1.0)
            struct.pack_into("<I", handler, 0x3D84 + 12 * words[3], 1)
        elif addr == 0x4ECEB0:
            struct.pack_into("<f", handler, 0x3A9C, 0.5)
        elif addr == 0x4ED020:
            struct.pack_into("<f", handler, 0x3AA0, 0.75)
        return words, [(this, bytes(handler))], this

    return recipe


SLOT1 = 0x4F12A0  # E36DamageOps::Slot1, the handler's vtable slot 1 (the impact dispatch; not ported)


def impact_direct_fuzz(mu, heap, r):
    """ImpactScale on a handler whose vtable (in the stub heap) has Slot1 at slot 1: the length (a4) is a float, the
    other seven words are random."""
    this, vtbl = heap + 0x20000, heap + 0x30000
    writes = [(this, struct.pack("<I", vtbl)), (vtbl + 4, struct.pack("<I", SLOT1))]
    words = [r.getrandbits(32) for _ in range(8)]
    words[3] = float_word(r)
    return words, writes, this


def slot1_result(mu, args, r, write):
    """Slot1 is not run: its eax is random and it writes nothing (the thunk passes the eax on)."""
    return r.getrandbits(32)


TAG = None  # the fourth word is a random tag (no index)
SPECS = {
    # Dmg660: the wear word (+0x3b00) at level 2 or more, capped at 0.9 (level 3) or 0.6.
    0x4EC660: dict(spec("CCarDamageHandler::Dmg660 (4ec660)", TAG),
                   direct_fuzz=negative_level_recipe(0x4EC660)),
    # DmgA00: a zone float (+0x3c08 + 0x38 i) for i below 4.
    0x4ECA00: spec("CCarDamageHandler::DmgA00 (4eca00)", range(0, 6)),
    # DmgCF20: the triple word (+0x3a30 + 0xc i) clears, flag +0x3a88.
    0x4ECF20: spec("CCarDamageHandler::DmgCF20 (4ecf20)", range(0, 28)),
    # DmgCFA0: the head gasket word (+0x3dd8 + 4 i) clears when it is 1, a > b and the level is 2 or more; returns 3.
    0x4ECFA0: dict(spec("CCarDamageHandler::DmgCFA0 (4ecfa0)", range(0, 6)), direct_fuzz=head_gasket_recipe),
    # DmgCFE0: the windshield word (+0x3dc4 + 4 i) clears when it is 1 and a > b; returns 4.
    0x4ECFE0: spec("CCarDamageHandler::DmgCFE0 (4ecfe0)", range(0, 6)),
    # Dmg9A0: the flag +0x3a98 clears.
    0x4EC9A0: spec("CCarDamageHandler::Dmg9A0 (4ec9a0)", TAG),
    # Dmg7B0: the pair word (+0x3a1c + 8 i) clears for i below 3.
    0x4EC7B0: spec("CCarDamageHandler::Dmg7B0 (4ec7b0)", range(0, 4)),
    # DmgAA0: the triple's second word (+0x3d88 + 0xc i) clears when the first is clear.
    0x4ECAA0: spec("CCarDamageHandler::DmgAA0 (4ecaa0)", range(0, 6)),
    # DmgB20: the triple's first word (+0x3d84 + 0xc i) clears, on the speed (index 2 needs level 2).
    0x4ECB20: dict(spec("CCarDamageHandler::DmgB20 (4ecb20)", range(0, 6)),
                   direct_fuzz=negative_level_recipe(0x4ECB20, range(0, 6))),
    # Dmg890: the word +0x3b08 falls at level 2 or more.
    0x4EC890: dict(spec("CCarDamageHandler::Dmg890 (4ec890)", TAG),
                   direct_fuzz=negative_level_recipe(0x4EC890)),
    # Dmg930: the flag +0x3af4 clears at level 2 or more.
    0x4EC930: dict(spec("CCarDamageHandler::Dmg930 (4ec930)", TAG),
                   direct_fuzz=negative_level_recipe(0x4EC930)),
    # Dmg820: the flag +0x3a94 clears at level 2 or more.
    0x4EC820: dict(spec("CCarDamageHandler::Dmg820 (4ec820)", TAG),
                   direct_fuzz=negative_level_recipe(0x4EC820)),
    # DmgCEB0: the word +0x3a9c caps at 0.3.
    0x4ECEB0: dict(spec("CCarDamageHandler::DmgCEB0 (4eceb0)", TAG),
                   direct_fuzz=negative_level_recipe(0x4ECEB0)),
    # DmgBB0: the pair (+0x3b60 + 8 i) clears and the flag +0x3b2c is set; returns 1.
    0x4ECBB0: spec("CCarDamageHandler::DmgBB0 (4ecbb0)", range(0, 8)),
    # DmgCC30: the pair (+0x3b40 + 8 i) clears and the flag +0x3b28 is set; returns 1.
    0x4ECC30: spec("CCarDamageHandler::DmgCC30 (4ecc30)", range(0, 8)),
    # DmgD020: the word +0x3aa0 falls at level 2 or more, the flag +0x3a88 is set; returns 5.
    0x4ED020: dict(spec("CCarDamageHandler::DmgD020 (4ed020)", TAG),
                   direct_fuzz=negative_level_recipe(0x4ED020)),
    # DmgD200: the mounted body's state (+0x3db4 + 4 i) moves to 1 or 2 on the speed.
    0x4ED200: spec("CCarDamageHandler::DmgD200 (4ed200)", range(0, 28)),
    # DmgD300: ignores the handler and stack words, always returning zero.
    0x4ED300: spec("CCarDamageHandler::DmgD300 (4ed300)", TAG),
    # The AddFunctor cases missing from the integration bindings.
    0x4EC720: dict(spec("CCarDamageHandler::Dmg720 (4ec720)", TAG),
                   direct_fuzz=damage_stress_recipe(0x4EC720)),
    0x4ECCB0: dict(spec("CCarDamageHandler::DmgCCB0 (4eccb0)", range(0, 8)),
                   direct_fuzz=damage_stress_recipe(0x4ECCB0, range(0, 8))),
    0x4ECD30: dict(spec("CCarDamageHandler::DmgCD30 (4ecd30)", range(0, 8)),
                   direct_fuzz=damage_stress_recipe(0x4ECD30, range(0, 8))),
    0x4ECDB0: dict(spec("CCarDamageHandler::DmgCDB0 (4ecdb0)", range(0, 8)),
                   direct_fuzz=damage_stress_recipe(0x4ECDB0, range(0, 8))),
    0x4ECE30: dict(spec("CCarDamageHandler::DmgCE30 (4ece30)", range(0, 8)),
                   direct_fuzz=damage_stress_recipe(0x4ECE30, range(0, 8))),
    0x4ED0F0: dict(spec("CCarDamageHandler::DmgD0F0 (4ed0f0)", TAG),
                   direct_fuzz=damage_stress_recipe(0x4ED0F0)),
    0x4ED160: dict(spec("CCarDamageHandler::DmgD160 (4ed160)", TAG),
                   direct_fuzz=damage_stress_recipe(0x4ED160)),
    0x4ED2F0: dict(name="NPhysics::E35_Zero2F0 (4ed2f0)", popped=0x10, x87=False, thiscall=True,
                   windows=[], direct_fuzz=zero_functor_recipe),
    # ImpactScale (the slot 2 thunk): scales a4 by 0.08 and calls slot 1 (Slot1, the external record below).
    0x4EC140: dict(name="CCarDamageHandler::ImpactScale (4ec140)", popped=0x20, x87=False, thiscall=True,
                   windows=[(0, lambda a: 4)], direct_fuzz=impact_direct_fuzz),
    SLOT1: dict(name="E36DamageOps::Slot1 (4f12a0)", popped=0x20, x87=False, thiscall=True, external=True,
                windows=[], result=slot1_result),
}
