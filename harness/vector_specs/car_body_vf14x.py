# Car-body damage material name (crates/rbr-physics/src/car_body/damage.rs, CCarDamageHandler::Vf14x). Run with
# `--set car_body_vf14x`. The world global (layout::WORLD) gives the manager at +0x24, whose flag word (+0x160e0) gates the
# lookup. The material tag (E45 Tag520940) is an external; GetNameOrNull (a tail jump) is read by the port, not recorded: a tail
# jump's return is logged as a call of its own.
# The popped bytes are the callee's `ret` immediates.

SET_NAME = "car_body_vf14x"

WORLD = 0x1659B70  # the world object global (g_1659b70)


def vf14x_fuzz(mu, args, rng, put):
    """The manager's flag word (the 0x1000000 gate, set or clear) and the handler's inner id (+0x3e04: 0, in range, or past
    the 0x100 materials). Every write goes back when the call returns."""
    manager = word(mu, word(mu, WORLD) + 0x24)
    put(manager + 0x160E0, rng.choice((0x1000000, 0x0, 0x1000001, 0xFFFFFFFF)), "<I")
    put(args[0] + 0x3E04, rng.choice((0, rng.randrange(1, 0x100), 0x100, 0x1234)), "<I")


SPECS = {
    # CCarDamageHandler::Vf14x() (ret 0): the material name of the handler's inner id when the gate is set; the manager
    # otherwise (the asm returns the manager pointer: the C++ says 0).
    0x4EC440: dict(name="CCarDamageHandler::Vf14x (4ec440)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x3E08)],
                   fuzz=vf14x_fuzz),
    # E45 CMaterialManager::Tag520940(id) (ret 4) on the manager at +0x18: the entry's address.
    0x520940: dict(name="E45 Tag520940 (520940)", popped=4, x87=False, thiscall=True, external=True, windows=[]),
}
