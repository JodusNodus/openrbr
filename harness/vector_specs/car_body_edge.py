# Car-body grid-edge vectors (rust/rbr-physics/src/car_body, tests/car_body_vectors.rs): PushGridEdge and RefreshEdge as
# top-level records. BodyCompose is an external here (its record is the edge vector), and so are its identity S30s and
# ComposeA. Run with `--set car_body_edge`. The popped bytes are the callee's `ret` immediates (thiscall: this is ecx).

SET_NAME = "car_body_edge"


def word(mu, at):
    return struct.unpack("<I", bytes(mu.mem_read(at, 4)))[0]


def s30_car_and_body(mu, a):
    """The two identity getters of BodyCompose: slot 12 on the car, then on its body (the pointer at +0x4a0)."""
    body = word(mu, a[0] + 0x4A0)
    return [(vtarget(mu, a[0], 12), 0), (vtarget(mu, body, 12), 0)]


def edge_fuzz(mu, args, rng, put):
    """The flag (+0x16a8: 0 takes the path) and the grid pointer (+0x64: 0 returns at once)."""
    car = args[0]
    put(car + 0x16A8, rng.choice((0, 0, 1)), "<I")
    if rng.random() < 0.25:
        put(car + 0x64, 0, "<I")


SPECS = {
    # E09Vec4::Clear (ret): zeroes the 16-byte vector at `this`.
    0x465D80: dict(name="E09Vec4::Clear", popped=0, x87=False, thiscall=True, external=True, windows=[(0, lambda a: 0x10)]),
    # E33 identity getter (slot 12): returns its this.
    0x519C00: dict(name="E33 S30 identity", popped=0, x87=False, thiscall=True, external=True, windows=[]),
    # E33Mat34::ComposeA (ret 0xc): args (this, a, t, out).
    0x51B4C0: dict(name="E33Mat34::ComposeA", popped=12, x87=False, thiscall=True, external=True, windows=[]),
    # CCar::BodyCompose(a, b) (0x4e6a30, ret 8): the output b is the edge vector (windowed).
    0x4E6A30: dict(name="CCar::BodyCompose (4e6a30)", popped=8, x87=False, thiscall=True, external=True,
                   dispatch=s30_car_and_body, windows=[(1, lambda a: 0x10), (2, lambda a: 0x10)]),
    # E37GridOps::AddEdge (ret 4): ecx is the grid (car+0x64), the stack argument the edge vector (windowed).
    0x4F65B0: dict(name="E37GridOps::AddEdge", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(1, lambda a: 0x10)]),
    # E43 Vf20 (0x529200, ret 4): RefreshEdge's last call on the car; its argument is the vector `a` (windowed).
    0x529200: dict(name="CEntity::E43Vf20 (529200)", popped=4, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x10)]),
    # CCar::PushGridEdge (0x4e7270, no stack arguments): returns unless the flag at +0x16a8 is 0 and the grid (+0x64) is
    # set. Then BodyCompose(zero, edge), AddEdge(edge) on the grid, and the flag is set to 1.
    0x4E7270: dict(name="CCar::PushGridEdge", popped=0, x87=False, thiscall=True, fuzz=edge_fuzz,
                   windows=[(0, lambda a: 0x16B0)]),
    # CCar::RefreshEdge(a) (0x4e72d0, ret 4): PushGridEdge, then E43 Vf20 (0x529200) on the car with `a`.
    0x4E72D0: dict(name="CCar::RefreshEdge", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x16B0)]),
}
