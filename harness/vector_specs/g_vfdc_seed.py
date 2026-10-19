"""CCar::UpdateSeed (0x4e91f0) called directly on the step snapshot's car, with car_body's fuzz: its flags word draws the
0x20 path (S8C, then S158) and the 0x10 path (MotionProbe and its emit), which no snapshot window reaches
(lead/reports/g-vfdc.md). The step does not call UpdateSeed (the control emit does), so the records are direct. The
seed buffer is in the stub heap. MotionProbe runs inside UpdateSeed's record (its externals are UpdateSeed's). The
records replay in car_body_vectors (`update_seed_matches_original_calls`).

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-vfdc-seed --addr 0x4e91f0 --count N --seed SEED
"""

SET_NAME = "g-vfdc-seed"

UPDATE_SEED = 0x4E91F0
CAR = 0x0BF1D7B8  # the step snapshot's car (car_body_a720.py)
BUFFER = 0x1000  # stub heap offset of the seed buffer (0x20 bytes)


def _update_seed_direct(mu, heap, rng):
    del mu, rng
    return [heap + BUFFER], [(heap + BUFFER, bytes(0x20))], CAR


SPECS = {addr: spec for addr, spec in SETS["car_body"].items() if spec.get("external")}
SPECS[UPDATE_SEED] = dict(SETS["car_body"][UPDATE_SEED], direct_fuzz=_update_seed_direct)
# Keep the slot-86 call and its own layout nodes in this caller's nested record tree.
SPECS[0x4E5EE0] = dict(SETS["car_body"][0x4E5EE0], external=True)
