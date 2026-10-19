"""The physics definition and the definition registry's solid, ground, car body, hinged and mounted definitions from a
snap-0 dump, as reference vectors for the Rust load path (rust/rbr-physics/tests/load_defs.rs; the wheel and car
definitions are load_wheel_refs.py's and load_car_refs.py's).

Usage: .venv/bin/python -I -B harness/load_registry_refs.py SNAPSHOT OUT

The registry (`CEntityFactory` at 0x7c8040) holds its count at +8 and one 0x14-byte entry per name from +0xc: the name
at +0, the definition at +0x10. Each definition's kind (and size) comes from its vtable. OUT gets a header line naming
the snapshot, then one line per definition: `<kind> <registry index> <address> <hex bytes> 4=<hex name>`, the name
being the NUL-terminated string the definition's name word (+4) points to.
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

REGISTRY = 0x7C8040
ENTRY = 0x14
# vtable: (kind, size)
KINDS = {
    0x73FAFC: ("solid_def", 0x1F0),
    0x73CEAC: ("ground_def", 0x15C),
    0x738DDC: ("car_body_def", 0x150),
    0x73D1AC: ("hinged_def", 0x510),
    0x73E59C: ("mounted_def", 0x240),
}
PHYS_DEF = 0x1659B6C
# (kind, offset, size) of the physics definition's parts; the materials are MATERIALS
PHYS_PARTS = (
    ("phys_def", 0x0, 0xC),
    ("def_manager", 0xC, 0xC),
    ("material_manager", 0x18, 0xC),
    ("solver_def", 0x16024, 0x4C),
    ("phys_tweaks", 0x16070, 0xF0),
)
MATERIALS = (0x24, 0x160, 0x100)
# the wheel and car definitions' vtables, cut by the other scripts
ELSEWHERE = (0x74281C, 0x7382A4)


def c_string(snap, at):
    out = bytearray()
    while not out.endswith(b"\0"):
        out += snap.read(at + len(out), 1)
    return bytes(out)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    lines = ["# %s" % os.path.basename(sys.argv[1])]
    phys = snap.u32(PHYS_DEF)
    first, stride, count = MATERIALS
    parts = [(kind, "-", phys + offset, size) for kind, offset, size in PHYS_PARTS]
    parts += [("material", str(n), phys + first + n * stride, stride) for n in range(count)]
    for kind, position, at, size in parts:
        fields = [kind, position, "%#x" % at, snap.read(at, size).hex()]
        if kind != "phys_tweaks" and snap.u32(at + 4):
            fields.append("4=%s" % c_string(snap, snap.u32(at + 4)).hex())
        lines.append(" ".join(fields))
    for i in range(snap.u32(REGISTRY + 8)):
        at = snap.u32(REGISTRY + 0xC + i * ENTRY + 0x10)
        vtable = snap.u32(at)
        if vtable in ELSEWHERE:
            continue
        kind, size = KINDS[vtable]
        name = c_string(snap, snap.u32(at + 4)).hex()
        lines.append("%s %d %#x %s 4=%s" % (kind, i, at, snap.read(at, size).hex(), name))
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
