"""QuerySlip's gates: the rpm gate (+0x44, gears above 2, rpm against the rev limit) and the
speed gate (+0x48, second gear only, |car speed| against 10 km/h, 0x741378). The step's records and the 300-step
downshift never reaches the speed gate's car-speed read: CCar::Get15F8 (0x4e6770) is no inner call of the 394 recorded
QuerySlip calls in the store. The direct recipe builds
the scratch, the setup (CDef, rpm limit at +0x188), the state (car at +0x68), the car (+0x15f8), the drive (+4 the drive
definition, +8 the engine) and the gear tables (+0x644 and +0x694, ten rows of 0x50) in the stub heap. Each call draws its
gates, gear, rev limit, engine speed, car speed and tables, so both sides of each compare come up within 120 calls.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-qslip --addr 0x536FE0 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs floats and words)

SET_NAME = "g-qslip"

SCRATCH, DEF, STATE, CAR = 0x80000, 0x80100, 0x80200, 0x81000  # stub heap offsets (heap + offset)
DRIVE, ENGINE, DRIVE_DEF = 0x83000, 0x83100, 0x84000
GEARS = 10  # the Def's gear rows (gear.rs)


def _query_slip_direct(mu, heap, rng):
    del mu
    gate_rpm = rng.choice((0, 1, 0x80000000))
    gate_speed = rng.choice((0, 1, 0xFFFFFFFF))
    gear = rng.choice((0, 1, 2, 2, 2, 3, 4, 5, 9))
    rpm_limit = rng.uniform(0.0, 9000.0)
    engine_speed = rng.uniform(0.0, 700.0)  # rad/s, so the rpm is about 0 to 6700
    car_speed = rng.choice((rng.uniform(-6.0, 6.0), rng.uniform(-3.0, 3.0), 2.7777779, -2.7777779))

    scratch, definition, state, car = (heap + at for at in (SCRATCH, DEF, STATE, CAR))
    drive, engine, drive_def = (heap + at for at in (DRIVE, ENGINE, DRIVE_DEF))
    scratch_image = bytearray(0x60)
    struct.pack_into("<2I", scratch_image, 0, definition, state)
    struct.pack_into("<2I", scratch_image, 0x44, gate_rpm, gate_speed)
    struct.pack_into("<I", scratch_image, 0x58, drive)
    def_image = bytearray(0x190)
    struct.pack_into("<f", def_image, 0x188, rpm_limit)
    state_image = bytearray(0x70)
    struct.pack_into("<I", state_image, 0x68, car)
    car_image = bytearray(0x1600)
    struct.pack_into("<f", car_image, 0x15F8, car_speed)
    drive_image = bytearray(0x10)
    struct.pack_into("<2I", drive_image, 4, drive_def, engine)
    engine_image = bytearray(0x600)
    struct.pack_into("<f", engine_image, 0x45C, engine_speed)
    struct.pack_into("<I", engine_image, 0x54C, gear)
    table = bytearray(0x968)
    for g in range(GEARS):
        struct.pack_into("<f", table, 0x644 + 0x50 * g, rng.uniform(0.5, 3.0))  # lim
        struct.pack_into("<f", table, 0x694 + 0x50 * g, rng.uniform(0.5, 1.5))  # div
    writes = [(scratch, bytes(scratch_image)), (definition, bytes(def_image)), (state, bytes(state_image)),
              (car, bytes(car_image)), (drive, bytes(drive_image)), (engine, bytes(engine_image)),
              (drive_def, bytes(table))]
    return [], writes, scratch


# The set is the whole vcu set (the inner getters are its specs) with the recipe on QuerySlip.
SPECS = dict(VCU_SPECS)  # noqa: F821 (vcu_vectors' names are in scope for a set file)
SPECS[0x536FE0] = dict(VCU_SPECS[0x536FE0], direct_fuzz=_query_slip_direct)  # noqa: F821
