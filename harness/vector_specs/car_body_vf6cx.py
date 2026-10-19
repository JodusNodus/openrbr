# Car-body Vf6cx under --fuzz (set car_body_vf6cx): the brake-wall raycast hits and the wheels' flag words are random, so
# the impulse, the brake-wall emit, S90 and SimFD00 run. The set is car_body's specs with only these two hooks: the
# car_body set's other fuzz hooks (Vf64x, Sim7AD0, ...) would change the drive that the other functions' records use.
# Use: vcu_vectors.py SNAPSHOT OUT --set car_body_vf6cx --fuzz SEED

SET_NAME = "car_body_vf6cx"

def vf6cx_fuzz(mu, args, rng, put):
    """The wheels' flag words (the two words at the flag pointer of +0x650), random 0 or 1: with them, S80 is followed by
    S90 when the flag is clear, and SimFD00 runs when both are clear. The flags are restored when the call returns."""
    for i in range(4):
        wheel = struct.unpack("<I", bytes(mu.mem_read(args[0] + 0x480 + 4 * i, 4)))[0]
        flag = struct.unpack("<I", bytes(mu.mem_read(wheel + 0x650, 4)))[0] if wheel else 0
        if flag:
            put(flag, rng.choice((0, 1)), "<I")
            put(flag + 4, rng.choice((0, 1)), "<I")


def raycast_result(mu, args, rng, write):
    """RaycastEntry (4e4960, --fuzz): a miss (eax 0), or a hit (eax 1) with the output (arg 2) filled as the asm
    reads it: the pointer at +0x10 (the impulse's `p`, here the output itself), a random impulse magnitude at +0x14 and
    the contact flag (bit 0 of +0x18)."""
    if rng.random() < 0.5:
        return 0
    out = args[2]
    write(out + 0x10, out, "<I")
    write(out + 0x14, rng.uniform(-2.0, 2.0), "<f")
    write(out + 0x18, rng.choice((0, 1)), "<I")
    return 1


SPECS = {addr: {k: v for k, v in spec.items() if k not in ("fuzz", "fuzz_args", "result")} for addr, spec in SETS["car_body"].items()}
SPECS[0x4EA300]["fuzz"] = vf6cx_fuzz
SPECS[0x4E4960]["result"] = raycast_result
