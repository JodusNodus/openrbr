# Direct-call coverage for the E43 pair echoes MAB10, MAB20 and MAB40 (src/rbr/RBR_E43_MatrixC.cpp), which no drive
# snapshot or 300-step chain reaches. MAB30 has its own set (entities_mab30.py). Each returns its first argument.
SET_NAME = "entities_mab_rest"


def mab_fuzz(mu, heap, rng):
    del mu
    return ([rng.getrandbits(32), rng.getrandbits(32)], [], heap)


SPECS = {
    addr: dict(
        name=name,
        popped=8,
        x87=False,
        thiscall=True,
        windows=[],
        direct_fuzz=mab_fuzz,
    )
    for addr, name in [
        (0x51AB10, "E43PairWalk::MAB10"),
        (0x51AB20, "E43PairWalk::MAB20"),
        (0x51AB40, "E43PairWalk::MAB40"),
    ]
}
