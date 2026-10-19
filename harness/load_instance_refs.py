"""The car instance as the factories build it (the car, its four wheels, then the hinged, mounted and car bodies), from a
run's load-time dumps and its allocation log, as reference vectors for the Rust load path
(rust/rbr-physics/tests/load_defs.rs).

Usage: .venv/bin/python -I -B harness/load_instance_refs.py SNAPAT_DIR ALLOCS OUT

SNAPAT_DIR holds the dumps at the entry and the return of the car's factory (`CCar::CDef::Vf50`, 0x4eb3e0, ordinal 1),
the wheels' (`E51Wheel::CDef::Vf50`, 0x53d1c0, ordinals 1-4), the hinged bodies' (`CHingedRigidBody::CDef::Vf50`,
0x51f8e0, 1-4), the mounted bodies' (`CMountedRigidBody::CDef::Vf50`, 0x521d20, 1-3) and the car body's
(`CCarBody::CDef::Vf50x`, 0x4ebb00, 1): RBR_SNAPAT with `4eb3e0 1`, `53d1c0 4`, `51f8e0 4`, `521d20 3` and `4ebb00 1`,
RBR_ALLOCLOG=1 (harness/play.sh). ALLOCS is the same run's allocation log.

OUT gets a header line naming the inputs, then:
- `call <kind> <position> <definition> <a> <b> <result>`: each factory call (ecx and the two stack arguments at the
  entry, eax at the return). The position is the wheel's (LF, RF, LB, RB), a body's definition name, `-` for the car;
- `alloc <size> <address> <hex bytes>`: the heap calls made inside the factory calls, in order, with what the block
  held at the factory's entry (the bytes the constructors do not write keep it);
- `env - <address> <hex bytes>`: the words of the car factory's entry dump the constructors read and the test does
  not build: the physics object's pointer and its definition word, the game options' mode words, the signal object's
  pointer, the five list heads the car's connections join (and their last links' next words), the body event signal's
  words, and the root identity word;
- `<kind> <position> <address> <hex bytes> [<offset>=<hex name>...]`: the objects at their factory's return: the car,
  its exclusion set, the current car word, the list heads and last links' next words, then each wheel, then each body
  and the exclusion set it allocated. Each
  `<offset>=` field is a word holding the definition's name pointer (a token-heap copy), with the name it points to.
"""
import os
import struct
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import allocs  # noqa: E402
import replay  # noqa: E402

CAR, WHEEL = 0x4EB3E0, 0x53D1C0
CAR_SIZE, WHEEL_SIZE = 0x1D60, 0x1420
# the bodies' factories, call counts, kinds and sizes, in the game's order
BODIES = ((0x51F8E0, 4, "hinged", 0x660), (0x521D20, 3, "mounted", 0x590), (0x4EBB00, 1, "car_body", 0x130))
EXCLUSIONS = 0x2C
POSITIONS = ("LF", "RF", "LB", "RB")
PHYSICS, OPTIONS, SIGNALS, CURRENT_CAR, ROOT_IDENTITY = 0x1659B70, 0x165FA90, 0x893600, 0x8EF660, 0x7CBB00
LISTS = (0xCA8, 0xD18, 0xCD8, 0x8A20, 0x96B0)
BODY_EVENT = 0xEC98
IMAGE = range(0x400000, 0x1700000)


def c_string(snap, at):
    out = bytearray()
    while not out.endswith(b"\0"):
        out += snap.read(at + len(out), 1)
    return bytes(out)


def dump(directory, function, ordinal, after):
    name = "RBRRecompSnapAt-%08x-%d%s.bin" % (function, ordinal, "-after" if after else "")
    return replay.Snapshot(os.path.join(directory, name))


def lists(snap):
    """The list heads the car's connections join (12 bytes) and their last links' next words (4 bytes), as (address,
    size)."""
    signals = snap.u32(SIGNALS)
    out = []
    for at in LISTS:
        out.append((signals + at, 12))
        last = snap.u32(signals + at + 4)
        if last:
            out.append((last + 4, 4))
    return out


def connections(snap, first):
    """The body event's connections from `first` on (0x18 bytes) and their targets' words at +0x10."""
    out = []
    while first:
        out += [(first, 0x18), (snap.u32(first + 0x10) + 0x10, 4)]
        first = snap.u32(first + 4)
    return out


def env(snap):
    """The words the constructors read that the test does not build, as (address, size)."""
    physics, options, signals = snap.u32(PHYSICS), snap.u32(OPTIONS), snap.u32(SIGNALS)
    signal = signals + BODY_EVENT
    out = [(PHYSICS, 4), (physics + 0x24, 4), (OPTIONS, 4), (options, 4), (options + 0xCA4, 4), (SIGNALS, 4),
           (ROOT_IDENTITY, 4), (signal, 12), (signal + 0x208, 4)]
    out += connections(snap, snap.u32(signal + 0x208))
    current = snap.u32(signal + 4)
    if current:
        by_type = signal + 8 + 0x10 * snap.u32(current)
        out += [(current, 4), (by_type, 4)] + connections(snap, snap.u32(by_type))
    return out + lists(snap)


def obj(snap, kind, position, at, size, name=None):
    data = snap.read(at, size)
    names = []
    if name is not None and name not in IMAGE:
        text = c_string(snap, name).hex()
        names = ["%x=%s" % (o, text) for o in range(0, size, 4) if struct.unpack_from("<I", data, o)[0] == name]
    return " ".join([kind, position, "%#x" % at, data.hex()] + names)


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    directory, log, out = sys.argv[1:]
    records = allocs.records(log)
    calls = [(CAR, 1, "car", "-")] + [(WHEEL, k + 1, "wheel", p) for k, p in enumerate(POSITIONS)]
    for function, count, kind, _ in BODIES:
        for k in range(count):
            entry = dump(directory, function, k + 1, False)
            calls.append((function, k + 1, kind, c_string(entry, entry.u32(entry.regs["ecx"] + 4))[:-1].decode()))
    lines = ["# %s %s" % (os.path.basename(os.path.normpath(directory)), os.path.basename(log))]
    blocks = []
    for function, ordinal, kind, position in calls:
        entry, ret = dump(directory, function, ordinal, False), dump(directory, function, ordinal, True)
        a, b = struct.unpack("<II", entry.read(entry.regs["esp"] + 4, 8))
        lines.append("call %s %s %#x %#x %#x %#x" % (kind, position, entry.regs["ecx"], a, b, ret.regs["eax"]))
        # the log's dump markers: (9, thread, ordinal, version, entry address, 0), at the entry dump and the return's
        marks = [i for i, r in enumerate(records) if r[0] == 9 and r[2:5] == (ordinal, 2, function)]
        assert len(marks) == 2, (kind, position, marks)
        for r in records[marks[0] + 1:marks[1]]:
            assert r[0] == 1, r
            lines.append("alloc %#x %#x %s" % (r[3], r[5], entry.read(r[5], r[3]).hex()))
            blocks.append((r[3], r[5]))
    car_entry, car_after = dump(directory, CAR, 1, False), dump(directory, CAR, 1, True)
    for at, size in env(car_entry):
        lines.append("env - %#x %s" % (at, car_entry.read(at, size).hex()))
    car = car_after.regs["eax"]
    car_def = car_after.u32(car + 0x34)
    lines.append(obj(car_after, "car", "-", car, CAR_SIZE, car_after.u32(car_def + 4)))
    exclusions = car_after.u32(car + EXCLUSIONS)
    size = next(s for s, at in blocks if at == exclusions)
    lines.append(obj(car_after, "exclusions", "-", exclusions, size))
    lines.append(obj(car_after, "current_car", "-", CURRENT_CAR, 4))
    for at, size in lists(car_entry):
        lines.append(obj(car_after, "signal_list", "-", at, size))
    for k, position in enumerate(POSITIONS):
        after = dump(directory, WHEEL, k + 1, True)
        wheel = after.regs["eax"]
        lines.append(obj(after, "wheel", position, wheel, WHEEL_SIZE, after.u32(after.u32(wheel + 0x34) + 4)))
    for function, count, kind, body_size in BODIES:
        for k in range(count):
            after = dump(directory, function, k + 1, True)
            body, position = after.regs["eax"], next(c[3] for c in calls if c[:2] == (function, k + 1))
            lines.append(obj(after, kind, position, body, body_size, after.u32(after.u32(body + 0x34) + 4)))
            exclusions = after.u32(body + EXCLUSIONS)
            size = next((s for s, at in blocks if at == exclusions), None)
            if size is not None:
                lines.append(obj(after, "exclusions", position, exclusions, size))
    with open(out, "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
