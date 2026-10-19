"""The car's wheel definitions and the setups from a snap-0 dump, as reference vectors for the Rust load path
(rust/rbr-physics/tests/load_defs.rs).

Usage: .venv/bin/python -I -B harness/load_wheel_refs.py SNAPSHOT OUT

Reads the player car (*0x8ef660), its four wheels (car+0x470) and their definitions (wheel+0x34, 0x980 bytes each:
the spring damper definition at +0x450 and the tyre definition at +0x56c), CCompleteCarSetup (*0x7ecd00: its header,
the Car, Drive, Engine and VehicleControlUnit setups, then the WheelLF..RB, SpringDamperLF..RB and TyreLF..RB setups)
and CCompleteCarSetupOptions (*0x7ecd04: its header, the Car, Drive, Control and Engine options, the spring damper,
tyre and wheel options, then TyreType00..06).
OUT gets a header line naming the snapshot, then one line per object:
`<kind> <position> <address> <hex bytes> [<offset>=<hex name>...]`, where each `<offset>=` field is a name word of
the object (a token-heap pointer) and the NUL-terminated name it points to.
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

POSITIONS = ("LF", "RF", "LB", "RB")
WHEEL_DEF = (0x980, (0x4, 0x454, 0x570))
# (kind, offset, size) of the single objects in CCompleteCarSetup and CCompleteCarSetupOptions; each has its name
# word at +4
SINGLE_SETUPS = (
    ("complete_setup", 0x0, 0xC),
    ("car_setup", 0xC, 0x3C),
    ("drive_setup", 0x48, 0x63C),
    ("engine_setup", 0x684, 0x1C),
    ("vcu_setup", 0x6A0, 0xB6C),
)
SINGLE_OPTIONS = (
    ("setup_options", 0x0, 0xC),
    ("car_options", 0x10, 0x210),
    ("drive_options", 0x220, 0x10BC),
    ("vcu_options", 0x12DC, 0xCC),
    ("engine_options", 0x13A8, 0x3C),
)
# (kind, offset of the LF object, stride = size); each has its name word at +4
SETUPS = (
    ("wheel_setup", 0x1210, 0x70),
    ("spring_setup", 0x13D0, 0x9C),
    ("tyre_setup", 0x1640, 0x30),
)
OPTIONS = (
    ("spring_options", 0x13E4, 0x18C),
    ("tyre_options", 0x1A14, 0x3C),
    ("wheel_options", 0x1B10, 0x320),
)
TYRE_TYPES = (0x2790, 0x6AC, 7)


def c_string(snap, at):
    out = bytearray()
    while not out.endswith(b"\0"):
        out += snap.read(at + len(out), 1)
    return bytes(out)


def line(snap, kind, position, at, size, names):
    fields = [kind, position, "%#x" % at, snap.read(at, size).hex()]
    fields += ["%x=%s" % (o, c_string(snap, snap.u32(at + o)).hex()) for o in names if snap.u32(at + o)]
    return " ".join(fields)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    car = snap.u32(0x8ef660)
    lines = ["# %s" % os.path.basename(sys.argv[1])]
    for i, position in enumerate(POSITIONS):
        wheel = snap.u32(car + 0x470 + 4 * i)
        size, names = WHEEL_DEF
        lines.append(line(snap, "wheel_def", position, snap.u32(wheel + 0x34), size, names))
    complete, options = snap.u32(0x7ecd00), snap.u32(0x7ecd04)
    for owner, singles, arrays in ((complete, SINGLE_SETUPS, SETUPS), (options, SINGLE_OPTIONS, OPTIONS)):
        for kind, offset, size in singles:
            lines.append(line(snap, kind, "-", owner + offset, size, (0x4,)))
        for kind, first, stride in arrays:
            for i, position in enumerate(POSITIONS):
                lines.append(line(snap, kind, position, owner + first + i * stride, stride, (0x4,)))
    first, stride, count = TYRE_TYPES
    for n in range(count):
        lines.append(line(snap, "tyre_type", "%02d" % n, options + first + n * stride, stride, (0x4,)))
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
