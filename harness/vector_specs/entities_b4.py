# B4, CGround (src/rbr/RBR_E43_GroundA/B/C.cpp, RBR_E44_Def.cpp, RBR_E33_Matrix/MatrixC): the ground def's ctor and
# factory (New9, the CDef ctor), CGround::CDef::Vf50 (the ground's make), E33Mat34::FetchX (the matrix copy and its
# walk) and CGround::VfE8 (the box probe). Load-time records (RBR_SNAPAT) cover the ctor, the factory and Vf50; the
# drive's snapshots cover FetchX and VfE8. See rust/rbr-physics/src/entities/README.md (git 9c28c0b8) (B4).
SET_NAME = "entities_b4"

import struct  # noqa: E402 (the fuzz hook reads floats)
from unicorn.x86_const import UC_X86_REG_ESP  # noqa: E402 (the fuzz hooks read the return address)


def matrix_nodes(mu, a):
    """The 16-byte matrix each transform argument (a, b) points at: the matrix is the transform's first word."""
    return [(word(mu, obj), 0x10) for obj in a[1:3] if obj]


def fetch_matrix(mu, a):
    """FetchX's source: the 16-byte matrix at its first word."""
    return [(word(mu, a[0]), 0x10)]


def probe_nodes(mu, a):
    """VfE8's reads: the identity object and the host globals (their words), and the two matrices."""
    return [(0x007CBB00, 4), (0x007C755C, 4)] + matrix_nodes(mu, a)


def host_s00(mu, a):
    """The box query: the host (g_7c755c) slot 0 (S00), thiscall with three pointers (its own pops: ret 0xC)."""
    host = word(mu, 0x007C755C)
    return [(vtarget(mu, host, 0), 12)]


def fuzz_gate(mu, args, rng, put):
    """The host gate's boxes (args 1 and 2, 16-byte points) for CollideInit's call: each float moves by up to 50 from the
    drive's value, so the query lands on and off the host's grid near the car (the drive's own boxes all miss)."""
    if not from_collide(mu, COLLIDE_GATE_RETURNS):
        return
    for at in (args[1], args[2]):
        for k in range(3):
            x = struct.unpack("<f", bytes(mu.mem_read(at + 4 * k, 4)))[0]
            put(at + 4 * k, x + rng.uniform(-50.0, 50.0))


# CollideInit's call sites of the loop query (0x4fd760) and the gate (0x4ff070): the return address each one leaves.
# The fuzz applies to these calls only; the wheel's query (E51Wheel::M3D520) calls the same function.
COLLIDE_QUERY_RETURNS = (0x51BE4E, 0x51C14E, 0x51C16D, 0x51C28F)
COLLIDE_GATE_RETURNS = (0x51BDAD,)


def from_collide(mu, returns):
    """Whether the call at entry (its return address is at the top of the stack) is one of CollideInit's."""
    esp = mu.reg_read(UC_X86_REG_ESP)
    return word(mu, esp) in returns


def fuzz_query(mu, args, rng, put):
    """The loop query's two boxes (args 1 and 2, 16-byte points) for CollideInit's calls: each float moves by up to 50
    from the drive's value, so the query can hit the grid's triangles (the drive's own boxes all miss)."""
    if not from_collide(mu, COLLIDE_QUERY_RETURNS):
        return
    for at in (args[1], args[2]):
        for k in range(3):
            x = struct.unpack("<f", bytes(mu.mem_read(at + 4 * k, 4)))[0]
            put(at + 4 * k, x + rng.uniform(-50.0, 50.0))


def ground_global(mu, a):
    """The ground singleton (0x1659268), which Vf50 clears and sets."""
    return [(0x01659268, 4)]


# B4b, CollideInit (0x51bd30): this = bodyB (E33Mat34), a = bodyA, b unused, c = the event sink (slot 1 = the event).
def on_a(slot, popped):
    """A call on a's vtable slot `slot` (a is the second argument: the first is this, bodyB)."""
    return lambda mu, a: [(vtarget(mu, a[1], slot), popped)]


def collide_dispatch(mu, a):
    """The vtable calls CollideInit makes at entry: a's slots 12, 70 and 72 and the sink's slot 1 (c = a[3])."""
    sink = [(vtarget(mu, a[3], 1), 4)] if a[3] else []
    return [(vtarget(mu, a[1], 12), 4), (vtarget(mu, a[1], 70), 4), (vtarget(mu, a[1], 72), 8)] + sink


def collide_nodes(mu, a):
    """CollideInit's reads outside its calls: bodyA's object (0x64 bytes, its flags at +0x60), the host (g_7c755c)
    and the identity object (g_7cbb00), whose words it passes on."""
    return [(a[1], 0x64), (0x007CBB00, 4), (0x007C755C, 4)]


SPECS_B4B = {
    0x51BD30: dict(name="E33Mat34::CollideInit", popped=12, x87=False, thiscall=True, windows=[], direct=True,
                   dispatch=collide_dispatch, nodes=collide_nodes),
    0x4F3E80: dict(name="CE36SubBlock::Init", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x80)],
                   external=True),
    0x4FF070: dict(name="E43HostBox?::query (host, a)", popped=12, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)], external=True, fuzz=fuzz_gate),
    0x4FEBC0: dict(name="host query (fallback)", popped=12, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)], external=True),
    0x4FD760: dict(name="host query (M_D760X)", popped=16, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10), (3, lambda a: 0x10)], external=True,
                   fuzz=fuzz_query),
    # M6610 (grid build, thiscall: this = the grid, source = the 0x6B0-byte grid it copies, then two stack words the
    # caller left: the two dispatch results and the identity). Pops 12.
    0x4F6610: dict(name="CColGrid::M6610 (grid build)", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6B0), (1, lambda a: 0x6B0)], external=True),
    # The two vtable calls CollideInit makes on bodyA before the gate (slots 12 and 70): thiscall with one stack word, but
    # retl (they pop nothing; M6610 pops the words). Their addresses are bodyA's vtable entries in this drive.
    0x519C00: dict(name="bodyA slot 12 (0x30)", popped=4, x87=False, thiscall=True, cdecl=True, windows=[],
                   external=True),
    0x4E11F0: dict(name="bodyA slot 70 (0x118)", popped=4, x87=False, thiscall=True, cdecl=True, windows=[],
                   external=True),
    # bodyA slot 72 (0x120): the two points the gate reads (args 1 and 2, 16 bytes each).
    0x519DB0: dict(name="bodyA slot 72 (0x120)", popped=8, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)], external=True),
    0x4F6740: dict(name="E38RayClear (grid)", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x6B0)],
                   external=True),
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)], external=True),
    0x43C6A0: dict(name="Normalize (E01Vec3?)", popped=0, x87=False, thiscall=True, windows=[(0, lambda a: 0x10)],
                   external=True),
    0x473B30: dict(name="Length (vec3)", popped=0, x87=True, thiscall=True, windows=[(0, lambda a: 0x10)],
                   external=True),
    0x473C90: dict(name="473C90 (vec3 op)", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x10), (1, lambda a: 0x10)], external=True),
    0x4E0840: dict(name="CColGrid::SampleA", popped=16, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6B0), (3, lambda a: 0x10), (4, lambda a: 0x10)], external=True),
    0x4E0950: dict(name="CColGrid::SampleB", popped=12, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x6B0), (2, lambda a: 0x10), (3, lambda a: 0x10)], external=True),
    0x4F4F10: dict(name="E36AllocEvent", popped=4, x87=False, cdecl=True, windows=[],
                   after=lambda mu, a, eax: [(eax, 0xF0)] if eax else [], external=True),
    0x4F50E0: dict(name="CCollisionEvent::InitFull", popped=24, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0xF0), (3, lambda a: 0x10), (5, lambda a: 0x10)], external=True),
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True,
                   windows=[(1, lambda a: 0x10), (3, lambda a: 0x10)], external=True),
}


SPECS = {
    # args: this (the def), name (the class name, read by the base's constructor only). Its writes are the base's (the
    # E43EntityDef constructor, 0x51a530), its vtable, and four NTweak members (CInteger x2 at +0x11c and +0x12c,
    # CUnsigned x2 at +0x13c and +0x14c), each a 16-byte block the NTweak constructors write (0x52ff20, 0x52fec0).
    0x51B990: dict(name="CGround::CDef::CDef", popped=4, x87=False, thiscall=True, windows=[(0, lambda a: 0x15C)]),
    # GroundDefNew9 (cdecl, one argument: the name): operator new(0x15c), then the def's constructor on it. Returns it.
    # The def it allocates is the returned pointer: its bytes after the call are checked (the allocation starts zeroed).
    0x51BAA0: dict(name="GroundDefNew9", popped=4, x87=False, cdecl=True, windows=[],
                   after=lambda mu, a, eax: [(eax, 0x15C)] if eax else []),
    # args: this (the transform: matrix pointer at +0, worker at +4), out (16 bytes), arg. Copies the matrix to out,
    # then the worker walks (WB320) with arg, out and 1: its one external call is the walk.
    0x51B6C0: dict(name="E33Mat34::FetchX", popped=8, x87=False, thiscall=True,
                   windows=[(0, lambda a: 8), (1, lambda a: 0x10)], nodes=fetch_matrix, direct=True),
    # RoutePairOneTwo (WB320): the worker's walk, answered from the record (its own records are in "entities").
    0x51B320: dict(name="E43PairWalk::WB320 (RoutePairOneTwo)", popped=12, x87=False, thiscall=True, windows=[],
                   external=True),
    # args: this (the ground), a, b (transforms), c, d (the 15-word record), e. Two FetchX calls (direct), the host's
    # S00 (dispatch), CopyFrom (external), then the scatter into d on a hit.
    0x51C6F0: dict(name="CGround::VfE8", popped=0x14, x87=False, thiscall=True,
                   windows=[(4, lambda a: 0x3C)], nodes=probe_nodes, direct=True, dispatch=host_s00),
    # operator new (CRT, cdecl, one size argument): the allocation the factory and Vf50 ask for.
    0x70BEC2: dict(name="operator new (CRT)", popped=4, x87=False, cdecl=True, windows=[], external=True),
    # CGround::CDef::Vf50 (this: the def; a, b): clears the singleton (deleting the old ground through slot 7 when there
    # is one), allocates a CGround (0x64) and constructs it as CEntity(def, a, b, 0x80). Returns it.
    0x51C810: dict(name="CGround::CDef::Vf50", popped=8, x87=False, thiscall=True, windows=[],
                   nodes=ground_global, after=lambda mu, a, eax: [(eax, 0x64)] if eax else []),
    # The host's box query (vtable slot 0 of g_7c755c, reached through dispatch): its out buffer (13 floats) is the
    # stack buffer VfE8 passed, so its post bytes are the host's output (recorded as an inner window).
    0x4FE810: dict(name="E43HostBox3::S00 (box query)", popped=12, x87=False, thiscall=True, external=True,
                   windows=[(3, lambda a: 0x34)]),
    # E01Vec4::CopyFrom (thiscall, one pointer; copies four floats from its argument into this).
    0x43C5B0: dict(name="E01Vec4::CopyFrom", popped=4, x87=False, thiscall=True, windows=[], external=True),
}
SPECS.update(SPECS_B4B)
