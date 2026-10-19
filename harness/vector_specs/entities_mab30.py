# Direct-call coverage for the E43 pair echo the drive snapshots do not reach.
SET_NAME = "entities_mab30"


def mab30_fuzz(mu, heap, rng):
    del mu
    return ([rng.getrandbits(32), rng.getrandbits(32)], [], heap)


SPECS = {
    0x51AB30: dict(
        name="E43PairWalk::MAB30",
        popped=8,
        x87=False,
        thiscall=True,
        windows=[],
        direct_fuzz=mab30_fuzz,
    ),
}
