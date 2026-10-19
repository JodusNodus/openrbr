"""CEntityFrame::Vf14 (0x5195f0), the frame's hook (slot 0xbc of its owner, with the two arguments). Its depth word (+8)
is the gate: at 0 the call returns its first argument at once, without the owner; otherwise it calls the owner's slot 47
(vtable +0xbc, the owner at +0xc). The step's records reach the hook (the damage chain's frames), not the depth-0 return;
the direct recipe draws the depth (0, or a hook depth) and the owner, whose slot 47 is CEntity::E43Vf140 (0x519e20, which
clears its first argument and returns it, the inner call of the hook). Lead/reports/g-records.md.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-frame --addr 0x5195F0 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs words)

SET_NAME = "g-frame"

FRAME, OWNER, OWNER_VTABLE, ARG_A, ARG_B = 0xC0000, 0xC1000, 0xC2000, 0xC3000, 0xC3100  # stub heap offsets
E43VF140 = 0x519E20


def _frame_direct(mu, heap, rng):
    del mu
    frame, owner, vtable = heap + FRAME, heap + OWNER, heap + OWNER_VTABLE
    depth = rng.choice((0, 0, 1, 2, 5))
    frame_image = bytearray(0x10)
    struct.pack_into("<2I", frame_image, 8, depth, owner)
    owner_image = bytearray(0x10)
    struct.pack_into("<I", owner_image, 0, vtable)
    vtable_image = bytearray(0x100)
    struct.pack_into("<I", vtable_image, 0xBC, E43VF140)
    writes = [(frame, bytes(frame_image)), (owner, bytes(owner_image)), (vtable, bytes(vtable_image)),
              (heap + ARG_A, bytes(0x10)), (heap + ARG_B, bytes(0x10))]
    return [heap + ARG_A, heap + ARG_B], writes, frame


# The whole entity set (the hook's callee is one of its specs), with the recipe on Vf14.
SPECS = dict(ENTITY_SPECS)  # noqa: F821 (vcu_vectors' names are in scope for a set file)
SPECS[0x5195F0] = dict(ENTITY_SPECS[0x5195F0], direct_fuzz=_frame_direct)  # noqa: F821
