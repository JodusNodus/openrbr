# Direct-call coverage for CEntity::Vf114 (0x519d60, slot 69; src/rbr/RBR_E43_EntityB.cpp): `xor eax, eax; ret`. The
# tanner-damage steps reach it (CEntity::E43VfD4) but no vcu-cov snapshot does.
SET_NAME = "entities_vf114"


def vf114_fuzz(mu, heap, rng):
    del mu, rng
    return ([], [], heap)


SPECS = {
    0x519D60: dict(name="CEntity::Vf114", popped=0, x87=False, thiscall=True, windows=[], direct_fuzz=vf114_fuzz),
}
