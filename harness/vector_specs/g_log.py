"""E01Log (0x43c8e0, cdecl `(channel, level, fmt, ...)`): the game's logger, whose GameLog (0x490f50) picks the sink
of the channel and level and, for the debugger's sink (0x4dedb0), writes the text with msvcr71's vsprintf into the
buffer at 0x7ece58, then copies it to a new block for OutputDebugStringA and frees it. The direct recipe draws a
channel and level (mostly the physics' (6, 0), (7, 0) and (6, 5); also the others, which reach sinks 0 and 1) and a
format from the exe's log lines and a mix of conversions, flags, widths and precisions, with five argument words: the
doubles are special values, wide exponents, rounding edges and random bits; the strings sit in the heap. The records
replay in physics_core_vectors (`e01_log_matches`).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-log --addr 0x43C8E0 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs words)

SET_NAME = "g-log"

BUFFER, BUFFER_LEN = 0x7ECE58, 0x200
FMT, STRINGS = 0xE0000, 0xE0400  # heap offsets: the format, then the strings %s takes
STRING_TEXT = [b"void __thiscall CSignalManager::CMultiSignal1<class CSigCarControl const &>::Emit(...)", b"",
               b"x", b"ApplicationState"]

FORMATS = [
    b"FloorS32: Invalid argument %f\n",
    b"Recursive emission of %s\n",
    b"%s -- ApplicationState (%d) out of range\n",
    b"%f %f\n",
    b"%.3f|%.1f\n",
    b"%10.2f|%-10.3f|%010.1f|%+.0f|% f\n",
    b"%#.0f|%.12f|%.0f\n",
    b"%.20f|%.17f\n",
    b"%d %u %x %X %#x %c %p %%\n",
    b"%3d|%-5d|%05d|%+d|% d|%.3d|%.0d|%ld|%hd\n",
    b"%s|%.2s|%8s|%-8s|%*d|%.*d\n",
]


def _double(rng):
    pick = rng.randrange(10)
    if pick == 0:
        return rng.choice([0x7FF0000000000000, 0xFFF0000000000000, 0xFFF8000000000000, 0x7FF8000000000000,
                           0x7FF4000000000000, 0xFFF8000000000001, 0, 1 << 63, 1, 0x000FFFFFFFFFFFFF])
    if pick == 1:  # f32 values widened, as FloorS32 passes them
        return struct.unpack("<Q", struct.pack("<d", struct.unpack("<f", struct.pack("<I", rng.getrandbits(32)))[0]))[0]
    if pick == 2:  # rounding edges at the precision
        v = rng.randrange(-10**7, 10**7) / 10**rng.randrange(0, 4) + rng.choice([5e-7, 5e-4, 5e-2, 0.5, 4.9999999e-4])
        return struct.unpack("<Q", struct.pack("<d", v))[0]
    if pick == 3:  # wide exponents
        v = rng.uniform(1, 10) * 10.0 ** rng.randrange(-30, 300)
        return struct.unpack("<Q", struct.pack("<d", -v if rng.random() < 0.5 else v))[0]
    if pick == 4:  # out of the int range FloorS32 takes
        v = rng.choice([2147483648.0, -2147483649.0, 4294967296.0, 1e10, -3e9, 2.5e9 + 0.5])
        return struct.unpack("<Q", struct.pack("<d", v))[0]
    if pick == 5:  # 17 digits and nines
        v = rng.choice([0.1, 1 / 3, 2 / 3, 999999.9999995, 9.9999995, 0.9999995, 123456789.123456789, 1e16 + 2])
        return struct.unpack("<Q", struct.pack("<d", v))[0]
    if pick == 6:  # small magnitudes of both signs, which round to zero or carry
        v = rng.choice([1e-7, 1e-4, 1e-2, 0.6, 1.0]) * rng.uniform(-1, 1)
        return struct.unpack("<Q", struct.pack("<d", v))[0]
    return rng.getrandbits(64)


def _log_direct(mu, heap, rng):
    del mu
    channel, level = rng.choice([(6, 0), (7, 0), (6, 5)] * 3 + [(rng.randrange(8), rng.randrange(9))])
    fmt = rng.choice(FORMATS)
    writes = [(heap + FMT, fmt + b"\0")]
    strings = []
    at = heap + STRINGS
    for text in STRING_TEXT:
        writes.append((at, text + b"\0"))
        strings.append(at)
        at += len(text) + 1
    words = []
    i = 0
    while i < len(fmt) and len(words) < 12:
        if fmt[i:i + 1] != b"%":
            i += 1
            continue
        j = i + 1
        while j < len(fmt) and fmt[j:j + 1] not in b"diuxXcspf%":
            if fmt[j:j + 1] == b"*":
                words.append(rng.choice([0, 3, 7, 12, 0xFFFFFFFE]))
            j += 1
        conv = fmt[j:j + 1]
        if conv == b"f":
            d = _double(rng)
            words += [d & 0xFFFFFFFF, d >> 32]
        elif conv == b"s":
            words.append(rng.choice(strings + [0]))
        elif conv in b"diuxXcp" and conv:
            words.append(rng.choice([0, 1, 0xFFFFFFFF, 0x80000000, 0x7FFFFFFF, rng.getrandbits(32),
                                     rng.randrange(-200, 200) & 0xFFFFFFFF, 0x41]))
        i = j + 1
    words += [rng.getrandbits(32) for _ in range(12 - len(words))]
    return [channel, level, heap + FMT] + words, writes, None


def _log_windows(mu, a):
    """The buffer, the format and the strings, as the call reads and writes them, and what GameLog reads: the console
    flag, the sink index table, the sinks, their vtable and the log file."""
    return [(BUFFER, BUFFER_LEN), (a[2], 0x400 + 0x80), (0x7C18F8, 4), (0x7C29D8, 0x120), (0x7C615C, 0x40),
            (0x737AF8, 8), (0x8EB76C, 4)]


SPECS = {
    0x43C8E0: dict(name="E01Log", popped=4 * 15, cdecl=True, x87=False, thiscall=False, branch=lambda a, w: "log",
                   windows=[], nodes=_log_windows, direct_fuzz=_log_direct),
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=lambda a, w: "", windows=[]),
    0x70C260: dict(name="operator delete (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=lambda a, w: "", windows=[]),
}
