"""The contact hit-path group (lead/reports/g-hit.md): CEntity::VfF0 (0x519eb0, `xor eax, eax; ret 0x20`, the other
entity's slot 0xf0 in CBody::VfD0), called directly: no 30-step chain reaches it. The records replay in entities_vectors
(`empty_hooks_match_original_calls`).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-hit --addr 0x519eb0 --count N --seed SEED
"""

SET_NAME = "g-hit"


def _vff0_fuzz(mu, heap, rng):
    del mu
    return [rng.getrandbits(32) for _ in range(8)], [], heap


SPECS = {
    0x519EB0: dict(name="CEntity::VfF0", popped=0x20, x87=False, thiscall=True, windows=[], direct_fuzz=_vff0_fuzz),
}
