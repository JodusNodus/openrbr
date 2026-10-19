"""Direct-call records for the E90 fence init group: E90Seg::Init (its D3DX normalise calls, through the thunk at
0x627235), the pool inits E90PartPool, E90PtrTab, E90BendPool and E90PinPool (their operator new[] and delete[] calls
and the part constructor). The recipes build the segment, the dummies and the pools in the replay's stub heap."""

import struct

SET_NAME = "g-e90c"

UP = 0x16FC524  # the up vector (x, y, z), with bit 0 of the dword at UP + 0xc marking it stored
UP_GUARD = 0x16FC530


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def f32bytes(x):
    return struct.pack("<f", x)


def u32bytes(x):
    return struct.pack("<I", x & 0xFFFFFFFF)


def place(obj, rng, spread, base):
    """The dummy's position (+0x6c) near `base`, three floats."""
    return [(obj + 0x6C + 4 * k, f32bytes(base[k] + rng.uniform(-spread, spread))) for k in range(3)]


def fuzz_seg_init(mu, heap, rng):
    seg, dummy, prev, nxt = heap + 0x1000, heap + 0x2000, heap + 0x2100, heap + 0x2200
    writes = [(seg, bytes(0x78)), (dummy, bytes(0xB4)), (prev, bytes(0xB4)), (nxt, bytes(0xB4))]
    base = [rng.uniform(-5.0, 5.0) for _ in range(3)]
    # One call in eight has no neighbours (the span is a point: the zero direction), one in eight a tiny span.
    lone = rng.random() < 0.125
    spread = 1e-8 if rng.random() < 0.125 else 2.0
    writes += place(dummy, rng, spread, base)
    links = {0x64: 0, 0x68: 0}
    if not lone:
        if rng.random() < 0.85:
            links[0x68] = prev
            writes += place(prev, rng, spread, base)
        if rng.random() < 0.85:
            links[0x64] = nxt
            writes += place(nxt, rng, spread, base)
    writes += [(dummy + at, u32bytes(p)) for at, p in links.items()]
    # The up vector is stored once (guard bit set); a call that finds it clear stores it itself.
    guard_set = rng.random() < 0.5
    writes += [(UP_GUARD, u32bytes(1 if guard_set else 0))]
    writes += [(UP + 4 * k, f32bytes(rng.uniform(-1.0, 1.0))) for k in range(3)]
    return [dummy], writes, seg


def seg_init_nodes(mu, a):
    dummy = a[1]
    out = [(UP, 0x10)]
    for at in (0x64, 0x68):
        p = word(mu, dummy + at)
        if p:
            out.append((p + 0x6C, 0xC))
    return out


def fuzz_pool(n_range, block_range, ext_pre, owned_pre):
    def fuzz(mu, heap, rng):
        pool = heap + 0x1000
        n = rng.randrange(*n_range)
        block = heap + 0x2000 if rng.random() < block_range else 0
        writes = [(pool, bytes(0x10))]
        if ext_pre:
            # Storage from an earlier init: owned (delete[] first) or external (dropped, not freed).
            owned = rng.random() < 0.5
            writes += [(pool + 0x8, bytes([0 if owned else 1, 0, 0, 0])), (pool + 0xC, u32bytes(heap + 0x3000))]
            writes += [(pool + 0x4, u32bytes(4))]
        return [n, block], writes, pool

    return fuzz


fuzz_part_pool = fuzz_pool((0, 4), 0.5, True, False)
fuzz_ptr_tab = fuzz_pool((0, 4), 0.5, False, False)
fuzz_bend_pool = fuzz_pool((0, 4), 0.5, True, False)
fuzz_pin_pool = fuzz_pool((0, 4), 0.5, True, False)


def pool_window(a):
    return 0x10


def block_window(elem_size):
    return lambda a: elem_size * a[1] if a[2] else 0


SPECS = {
    # Nested in the fence builders: its span setup calls _D3DXVec3Normalize@8 twice, on the dummies' positions.
    0x6027A0: dict(name="E90PtrTab::Init", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_ptr_tab,
                   windows=[(0, pool_window)]),
    0x605B60: dict(name="E90Seg::Init", popped=4, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_seg_init,
                   windows=[(0, lambda a: 0x78), (1, lambda a: 0xB4)], nodes=seg_init_nodes),
    0x6026A0: dict(name="E90PartPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_part_pool,
                   windows=[(0, pool_window), (2, block_window(0x28))]),
    0x6028B0: dict(name="E90BendPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_bend_pool,
                   windows=[(0, pool_window), (2, block_window(0xC))]),
    0x602930: dict(name="E90PinPool::Init", popped=8, x87=False, thiscall=True, external=True,
                   direct_fuzz=fuzz_pin_pool,
                   windows=[(0, pool_window), (2, block_window(4))]),
    # Inner calls: the normalise thunk's target, operator new[] and delete[], and the part constructor.
    0x627235: dict(name="_D3DXVec3Normalize@8", popped=8, x87=False, external=True,
                   windows=[(0, lambda a: 12), (1, lambda a: 12)]),
    0x70BF84: dict(name="operator new[] (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, windows=[],
                   external=True),
    0x70C27E: dict(name="operator delete[] (CRT)", popped=4, x87=False, thiscall=False, cdecl=True, windows=[],
                   external=True),
    0x602220: dict(name="E90_PartCtor", popped=0, x87=False, thiscall=True, windows=[], external=True),
}
