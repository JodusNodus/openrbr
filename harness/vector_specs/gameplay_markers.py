"""Direct-call records for E12's marker ticker and the E25 pace-note word maps."""

import random
import struct
from unicorn.x86_const import UC_X86_REG_EDX


SET_NAME = "gameplay_markers"
FIRED = 0x80000000


def _word(mu, address):
    return struct.unpack("<I", bytes(mu.mem_read(address, 4)))[0]


def _f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value):
    return struct.unpack("<I", struct.pack("<f", _f32(value)))[0]


def _ticker_nodes(mu, args):
    this = args[0]
    count, keys = _word(mu, this + 0x20), _word(mu, this + 0x24)
    app = _word(mu, 0x7EAC48)
    host = _word(mu, 0x0165FC68)
    return [
        (keys, count * 0x0C),
        (0x01660B74, 4),
        (0x01660B80, 4),
        (app + 0x728, 4),
        (host + 0x140, 4),
        (host + 0x144, 4),
        (host + 0x258, 4),
        (host + 0x25C, 4),
        (host + 0x260, 4),
    ]


def _ticker_dispatch(mu, _args):
    """Capture the one virtual start-line action when the event-state gate is 7."""
    holder = _word(mu, 0x007EA990)
    obj = _word(mu, holder + 0x40)
    target = _word(mu, _word(mu, obj) + 0xA0)
    return [(target, 8)] if target else []


def _dot150_nodes(mu, args):
    """Keep Dot150's sampled driveline records and the sampler/car lanes together in its direct record."""
    sampler = args[0]
    drive = _word(mu, 0x01659184)
    count = _word(mu, drive + 0x75310)
    x = struct.unpack("<f", bytes(mu.mem_read(sampler + 0x140, 4)))[0]
    if x < 0.0:
        first = 0
    else:
        first = min(int(x), count - 1)
    rows = 1 if first == count - 1 else 2
    return [(sampler + 0x140, 0x1C), (drive + 0x10 + first * 0x30, rows * 0x30)]


def _sample_fuzz(mu, heap, rng):
    sampler = _word(mu, 0x7EABA8)
    drive = _word(mu, 0x01659184)
    count = _word(mu, drive + 0x75310)
    if count < 2:
        x = 0.0
    else:
        points = (-2.0, -0.5, 0.0, 0.25, 1.0, count - 1.0, count - 0.5, count + 1.0)
        x = _f32(rng.choice(points) if rng.randrange(2) else rng.uniform(0.0, count - 1.0))
    return [heap + 0x10000], [(sampler + 0x140, struct.pack("<f", x))], sampler


def _sample_nodes(_mu, args):
    return [(args[0] + 0x140, 4)]


def _dot150_fuzz(mu, heap, rng):
    _, writes, sampler = _sample_fuzz(mu, heap, rng)
    return [], writes, sampler


def _ticker_fuzz(mu, heap, rng):
    sampler = _word(mu, 0x7EABA8)
    stage_set = _word(mu, sampler + 0x10)
    count, source_keys = _word(mu, stage_set + 0x20), _word(mu, stage_set + 0x24)
    source = bytearray(bytes(mu.mem_read(source_keys, count * 0x0C)))
    this, keys = heap + 0x10000, heap + 0x11000
    distances = [struct.unpack_from("<f", source, i * 0x0C + 8)[0] for i in range(count)]
    for i in range(count):
        # Start and split handling dispatch through live race-host state which is not fully
        # initialized in a physics-step snapshot. Keep the marker distances and array shape,
        # but make these isolated calls exercise their ordinary-key path.
        tag = struct.unpack_from("<I", source, i * 0x0C)[0]
        if tag in (0x15, 0x17):
            struct.pack_into("<I", source, i * 0x0C, 0x14)
        word1 = struct.unpack_from("<I", source, i * 0x0C + 4)[0]
        struct.pack_into("<I", source, i * 0x0C + 4, word1 & ~FIRED)

    scenario = rng.randrange(12)
    special_tag = {0: 0x16, 1: 0x18}.get(scenario)
    candidates = [
        i for i in range(max(0, count - 1))
        if struct.unpack_from("<I", source, i * 0x0C)[0] not in (0x15, 0x16, 0x17, 0x18)
        and distances[i] < distances[i + 1]
    ]
    if special_tag is not None and candidates:
        index = rng.choice(candidates)
        distance = distances[index]
        gap = distances[index + 1] - distance
        delta = min(max(0.02, abs(distance) * 0.00002), gap / 4.0)
        x = _f32(distance + delta)
        struct.pack_into("<I", source, index * 0x0C, special_tag)
    elif distances:
        index = rng.randrange(count)
        distance = distances[index]
        delta = max(0.02, abs(distance) * 0.00002)
        mode = rng.randrange(4)
        if mode == 0:
            x = distance
        elif mode == 1:
            x = _f32(distance - delta)
        elif mode == 2:
            x = _f32(distance + delta)
        else:
            x = _f32(rng.uniform(distances[0], distances[-1]))
    else:
        x = 0.0

    # Preserve the E12 Emit calls and their payloads, but disconnect their snapshot listeners. A pace-note listener
    # reaches audio/file APIs outside this ticker's contract, while the callout and EndRace records remain exact.
    signals = _word(mu, 0x00893600)
    writes = [
        (this, bytes(0x60)),
        (this + 0x20, struct.pack("<II", count, keys)),
        (keys, bytes(source)),
        (signals + 0x6ACC, struct.pack("<I", 0)),
        (signals + 0x7974, struct.pack("<I", 0)),
        (0x01660B74, struct.pack("<f", rng.choice((0.0, 0.05, 0.25, 1.0)))),
        (0x01660B80, struct.pack("<f", rng.choice((-1.0, 0.0, 2.0, 10.0)))),
    ]
    return [_f32_bits(x)], writes, this


def _slot_fuzz(_mu, _heap, rng):
    return [rng.choice(tuple(range(0x22)) + (0xFFFFFFFF,))], [], 0


def _flag_fuzz(_mu, _heap, rng):
    bits = rng.choice((0, 1, 2, 4, 0x10, 0x20, 0x40, 0x80, 0x400, 0x800, 0x1000,
                       0x2000, 0x400 | FIRED, rng.getrandbits(32)))
    return [bits], [], 0


def _handler_nodes(mu, args):
    signal = mu.reg_read(UC_X86_REG_EDX)
    key = _word(mu, signal + 4) if signal else 0
    return [(args[0], 0x58), (signal, 0x0C), (key, 0x08)] if signal else [(args[0], 0x58)]


SPECS = {
    # `this` is the sampler stored at g_e14Aba8. Each fresh call starts with all key fired bits cleared and x is
    # selected around a real stage marker, so the captured key bytes and event payloads cover both ticker passes.
    0x4783B0: dict(direct_fuzz=_ticker_fuzz, name="E12_AnimSet::Load3b0", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x28, 0x10)], nodes=_ticker_nodes, dispatch=_ticker_dispatch),
    0x4762A0: dict(direct_fuzz=_sample_fuzz, name="E12_Sampler::Sample", popped=4, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10)], nodes=_sample_nodes),
    0x43C6A0: dict(name="E01Vec4::Normalize", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    0x476450: dict(direct_fuzz=_dot150_fuzz, name="E12_Sampler::Dot150", popped=0, x87=True, thiscall=True, external=True, windows=[],
                   nodes=_dot150_nodes),
    0x476BF0: dict(name="E12_SeekD", popped=0, x87=True, thiscall=True, external=True, windows=[]),
    0x477860: dict(name="PaceNoteCallout::Emit", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x0C), (1, lambda a: 0x08, 4)]),
    0x477920: dict(name="EndRace::Emit", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x14)]),
    0x4B65B0: dict(name="E25PaceHandler", popped=0, x87=False, thiscall=True, external=True,
                   windows=[], nodes=_handler_nodes),
    0x4B55E0: dict(direct_fuzz=_slot_fuzz, name="E25_SlotId", popped=4, x87=False, windows=[]),
    0x4B5540: dict(direct_fuzz=_flag_fuzz, name="E25_BitId", popped=4, x87=False, windows=[]),
    0x00566240: dict(name="E12_GameHost::M66240", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x00572FC0: dict(name="E62_M572fc0", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x00579430: dict(name="E12_GameHost::M79430", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x00572DA0: dict(name="E12_GameHost::M72da0", popped=4, x87=False, thiscall=True, external=True, windows=[]),
    0x00573530: dict(name="E62GameMenu::M573530", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x00565470: dict(name="E59Host90::M00565470", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    0x00565E80: dict(name="E59_00565e80", popped=0, x87=False, cdecl=True, external=True, windows=[]),
}
