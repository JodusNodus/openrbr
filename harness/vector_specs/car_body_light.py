# Car-body light system (crates/rbr-physics/src/car_body/light.rs): the E36 light block and its sub-block, the CDef's
# static table getter, and the per-light emit (EmitLights, Update). Run with `--set car_body_light`. The popped bytes are
# the callee's `ret` immediates.

import struct  # noqa: E402

SET_NAME = "car_body_light"

BLOCK_WINDOW = 0xE0  # the block's words (+0x1c to +0xa4) and the flag word at +0xdc


def sub_block_fuzz(mu, args, rng, put):
    """The block's three words, random (the init zeroes them)."""
    for at in (0, 4, 8):
        put(args[0] + at, rng.getrandbits(32), "<I")


def block_reset_fuzz(mu, args, rng, put):
    """The block's words, random (the reset writes its groups and the flag word at +0xdc)."""
    for at in range(0, BLOCK_WINDOW, 4):
        put(args[0] + at, rng.getrandbits(32), "<I")


def load_binary_fuzz(mu, heap, r):
    """NTweak::CFloat3::LoadBinary(data) on `this`: the four words at `data` (in the stub heap) are random."""
    data, this = heap + 0x30000, heap + 0x40000
    writes = [(data + at, struct.pack("<I", r.getrandbits(32))) for at in range(0, 0x10, 4)]
    return [data], writes, this


LIGHT_BYTES = 0x1000  # Update's object: the system at `this`, its lights at +0x800, its def at +0x300, its flags at +0x100


def light_system_direct(mu, heap, r):
    """Update() on a light system in the stub heap: the words random, with the three pointers (+4 lights, +8 def, +0x14 flags)
    set into the block, the flag words zero about half the time, the def's scale words and the mult small, and the lights'
    heads zero about half the time."""
    this = heap + 0x20000
    block = bytearray(r.getrandbits(8) for _ in range(LIGHT_BYTES))
    struct.pack_into("<III", block, 4, this + 0x800, this + 0x300, 0)
    struct.pack_into("<I", block, 0x14, this + 0x100)
    for at in range(0x100, 0x200, 4):
        struct.pack_into("<I", block, at, r.choice((0, 0, 1, 2, 0xFFFFFFFF)))
    struct.pack_into("<I", block, 0x1DC, r.choice((0, 1, 2, 0xFFFFFFFF, 0x80000000)))
    for at in range(0x300 + 0x2C, 0x300 + 0x80, 4):
        struct.pack_into("<I", block, at, r.choice((0, 1, 3, 0xFFFFFFF0, 0x7FFFFFFF)))
    for light in range(4):
        for base in (0x800 + 0x13C, 0x800 + 0x07C, 0x800 + 0x0BC, 0x800 + 0x0FC, 0x800 + 0x17C, 0x800 + 0x1BC):
            at = base + 0xC + 0x10 * light
            struct.pack_into("<I", block, at, r.choice((0, 0x1234)))
    return [], [(this, bytes(block))], this


def emit_lights_direct(mu, heap, r):
    """EmitLights(kind, lights, flags, scaled) (ret 0x10; `this` is not read): the lights' heads and the flag words random
    (zero about half the time), the kind 1 to 6, and the scale zero or not."""
    lights, flags = heap + 0x30000, heap + 0x40000
    lights_block = bytearray(r.getrandbits(8) for _ in range(0x40))
    flags_block = bytearray(r.getrandbits(8) for _ in range(0x40))
    for light in range(4):
        struct.pack_into("<I", lights_block, 0xC + 0x10 * light, r.choice((0, 0x1234)))
        struct.pack_into("<I", flags_block, 8 * light, r.choice((0, 0, 1, 0xFFFFFFFF)))
        struct.pack_into("<I", flags_block, 4 + 8 * light, r.choice((0, 0, 1)))
    kind = r.randrange(1, 7)
    scaled = r.choice((0, 1, 0xFFFFFFFD))
    return [kind, lights, flags, scaled], [(lights, bytes(lights_block)), (flags, bytes(flags_block))], heap + 0x20000


SPECS = {
    # CE36SubBlock::Init() (no stack argument): zeroes the two words and the float at +8.
    0x4F3E80: dict(name="CE36SubBlock::Init (4f3e80)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10)], fuzz=sub_block_fuzz),
    # E36LightBlock::Reset() (no stack argument): the four groups set to 1, then the flag word at +0xdc.
    0x4F19B0: dict(name="E36LightBlock::Reset (4f19b0)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: BLOCK_WINDOW)], fuzz=block_reset_fuzz),
    # NTweak::CFloat3::LoadBinary(data) (ret 4): copies the four words at `data` to `this`; returns `this`.
    0x4F3D20: dict(name="NTweak::CFloat3::LoadBinary (4f3d20)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)], direct_fuzz=load_binary_fuzz),
    # CCarLightSystem::CDef::Vf20x() (no stack argument): returns the static Def table (0x8ef7b4).
    0x4F17E0: dict(name="CCarLightSystem::CDef::Vf20x (4f17e0)", popped=0, x87=False, thiscall=True, windows=[]),
    # CCarLightSystem::Update() (ret 0): six gates, each emitting its lights (the scale is the def word times the mult).
    0x4F2150: dict(name="CCarLightSystem::Update (4f2150)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: LIGHT_BYTES)], direct_fuzz=light_system_direct),
    # CCarLightSystem::EmitLights(kind, lights, flags, scaled) (ret 0x10): the four lights with heads set.
    0x4F20C0: dict(name="CCarLightSystem::EmitLights (4f20c0)", popped=0x10, x87=False, thiscall=True, external=True,
                   windows=[(2, lambda a: 0x40), (3, lambda a: 0x40)], direct_fuzz=emit_lights_direct),
    # CMultiSignal1<const CSigCarLight&>::Emit(signal) on the light emitter (ret 4): the payload's five words are the window.
    0x4F2000: dict(name="CMultiSignal1<CSigCarLight>::Emit (4f2000)", popped=4, x87=False, thiscall=True,
                   external=True, windows=[(1, lambda a: 20)]),
}
