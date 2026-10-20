"""NTweak's tokenizer and the token heap's allocator, the two callees of `NTweak::CBase::Parse` (0x52f0c0) that other
loaders call too (the setup, the tyres, the physics init, the season).

CTokenCursor::Tokenize (0x52ec80, thiscall on the cursor `(text, 0)`, ret 8): the recipe writes a text of atoms, quoted
strings (with whitespace and parentheses inside), groups (nested, with quoted parentheses), numbers and whitespace runs,
with or without the leading `(`, and sometimes a stray `)` between tokens, a `)` stuck to an atom, an unterminated string
or group, an empty text or a null one. Its `operator new(0x4000)` is an external entry; the table it returns is read
after the call (up to its terminator).

E11_StackAlloc (0x473a70, cdecl `(size, heap, align)`): the heap record (base, used, capacity) with a block that fits
(the original logs a full heap and traps, so a full one is not drawn), over the alignments the game uses and others.

The records replay in ntweak_vectors.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-ntweak --addr 0x52EC80 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipes pack words)

SET_NAME = "g-ntweak"

CURSOR, TEXT, HEAP_RECORD = 0xF0000, 0xF0100, 0xF1000  # heap offsets
TEXT_LEN = 0x400
EMPTY_TOKENS = 0x165F1A8
SPACES = [b" ", b"\t", b"\r\n", b"  ", b"\t\t\t", b"\n"]
ATOM_CHARS = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789+-."


def _atom(rng):
    return bytes(rng.choice(ATOM_CHARS) for _ in range(rng.randrange(1, 12)))


def _quoted(rng, closed=True):
    inner = bytes(rng.choice(ATOM_CHARS + b" ()\t") for _ in range(rng.randrange(0, 10)))
    return b'"' + inner + (b'"' if closed else b"")


def _group(rng, depth):
    items = [_quoted(rng)] if rng.random() < 0.7 else []
    for _ in range(rng.randrange(0, 4)):
        items.append(_item(rng, depth + 1))
    sep = lambda: rng.choice(SPACES)  # noqa: E731
    body = b"".join(sep() + i for i in items) if rng.random() < 0.8 else b"".join(items)
    return b"(" + body + (sep() if rng.random() < 0.3 else b"") + b")"


def _item(rng, depth=0):
    pick = rng.randrange(6 if depth < 3 else 3)
    if pick == 0:
        return _atom(rng)
    if pick == 1:
        return _quoted(rng)
    if pick == 2:
        return rng.choice([b"0.5", b"-2.47", b"+3", b"12", b"1.2.3", b"5.", b"+"])
    return _group(rng, depth)


def _text(rng):
    pick = rng.random()
    if pick < 0.03:
        return b""
    parts = [rng.choice([b"", b" ", b"\t\r\n"]), b"(" if rng.random() < 0.8 else b""]
    for _ in range(rng.randrange(1, 20)):
        parts.append(rng.choice(SPACES) if parts[-1] and rng.random() < 0.9 else b"")
        parts.append(_item(rng))
        if rng.random() < 0.05:
            parts.append(b")")  # stuck to the atom, or a stray one after a space
    if rng.random() < 0.05:
        parts.append(_quoted(rng, closed=False))
    elif rng.random() < 0.05:
        parts.append(b"(" + _atom(rng) + b" " + _atom(rng))
    else:
        parts.append(rng.choice([b")", b" )", b"\r\n)", b""]))
    return b"".join(parts)[:TEXT_LEN - 1]


def _tokenize_direct(mu, heap, rng):
    del mu
    cursor = heap + CURSOR
    text = _text(rng)
    writes = [(cursor, struct.pack("<2I", rng.getrandbits(32), rng.getrandbits(32))),
              (heap + TEXT, text + b"\0" + bytes(rng.getrandbits(8) for _ in range(TEXT_LEN - len(text) - 1)))]
    at = 0 if rng.random() < 0.02 else heap + TEXT
    return [at, 0], writes, cursor


def _table_after(mu, a, eax):
    """The table the cursor (eax) holds after the call, up to its terminator."""
    table = struct.unpack("<I", bytes(mu.mem_read(eax, 4)))[0]
    n = 0
    while struct.unpack("<I", bytes(mu.mem_read(table + 4 * n, 4)))[0]:
        n += 1
    return [(table, 4 * (n + 1))]


def _stack_alloc_direct(mu, heap, rng):
    del mu
    record = heap + HEAP_RECORD
    align = rng.choice([0x10, 0x10, 0x10, 4, 8, 1, 2, 0x20, 0x100])
    cap = rng.choice([0x1000, 0x10000, 0x200000, rng.randrange(0x40, 0x80000)])
    used = rng.randrange(0, cap)
    off = (used + align - 1) & ~(align - 1)
    size = rng.choice([0x20, 0, 1, 7, 0x10, rng.randrange(0, 0x400)])
    if off + size >= cap:  # a full heap traps: draw a block that fits
        used, off, size = 0, 0, min(size, cap - 1)
    writes = [(record, struct.pack("<3I", rng.getrandbits(32) & ~0xF, used, cap))]
    return [size, record, align], writes, None


SPECS = {
    0x52EC80: dict(name="NTweak::CTokenCursor::Tokenize", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 8), (1, lambda a: TEXT_LEN if a[1] else 0)],
                   nodes=lambda mu, a: [(EMPTY_TOKENS, 4)], after=_table_after, direct_fuzz=_tokenize_direct),
    0x473A70: dict(name="E11_StackAlloc", popped=12, x87=False, thiscall=False, cdecl=True,
                   windows=[(1, lambda a: 12)], direct_fuzz=_stack_alloc_direct),
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, external=True,
                   branch=lambda a, w: "", windows=[]),
}
