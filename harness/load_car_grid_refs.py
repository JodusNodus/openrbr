"""The car's collision grids from load-time dumps, as reference vectors for the Rust load path
(crates/rbr-physics/tests/load_car_grids.rs).

Usage: .venv/bin/python -I -B harness/load_car_grid_refs.py OUT LABEL=CAR=ENTRY=SNAP0...

CAR is the car's directory names, `<Cars dir>/<sgc name>/<physics dir>` (e.g. `COROLLA/corolla/t_coroll`), ENTRY a
`CCar::UpdateA720` (0x4ea720) entry dump (the grids as the car loader emitted them) and SNAP0 the same run's snap-0
(after UpdateA720 moved the hinged bodies' grids). Per run, OUT gets:
  car <label> <Cars dir> <sgc name> <physics dir> <HedgeHog3D's base>
  emit <label> <id> <count> <x y z bits>...           grid 1-9's vertices (+0x40, count at +0x68d) at ENTRY
  grid <label> <id> <0x6b0 bytes>                      grid 1-9 (0x8f7030 + id * 0x6b0) at SNAP0
  desc <label> <id> <address> <0x34 bytes>             the loader's CSigCollisionMesh of grids 1-8 (the block whose
                                                       id and faces pointer are the grid's)
  block <label> <id> <kind> <address> <length> <bytes>  what it points to: vertices (16 bytes each), faces (7 bytes
                                                       each), samples (2 bytes each); map_a and map_b (+0x24, +0x28)
                                                       as `fnv:<FNV-1a 64>`
HedgeHog3D's base is grid 1's name pointer less its offset (0x1aca74, "MrCarCollision").
"""
import os
import re
import struct
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

GRIDS = 0x8F7030
GRID_SIZE = 0x6B0
DESC_SIZE = 0x34
BODY_NAME = 0x1ACA74


def fnv64(data):
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "fnv:%016x" % h


def descriptors(snap, faces):
    """{id: address} of the 0x34-byte blocks with the grid's id at +0 and its faces pointer at +0x14."""
    want = {struct.pack("<I", f): gid for gid, f in faces.items() if f}
    found = {}
    pattern = re.compile(b"|".join(re.escape(k) for k in want))
    for base, size, off, *_ in snap.regions:
        blob = snap.data[off:off + size]
        for m in pattern.finditer(blob):
            at = base + m.start() - 0x14
            gid = want[m.group()]
            if m.start() >= 0x14 and struct.unpack_from("<I", blob, m.start() - 0x14)[0] == gid:
                assert gid not in found, "two descriptors of grid %d" % gid
                found[gid] = at
    return found


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    lines = []
    for arg in sys.argv[2:]:
        label, car, entry_path, snap_path = arg.split("=")
        cars_dir, sgc, physics = car.split("/")
        entry, snap = replay.Snapshot(entry_path), replay.Snapshot(snap_path)
        grid = lambda gid: GRIDS + gid * GRID_SIZE  # noqa: E731
        hh = snap.u32(grid(1) + 0x684) - BODY_NAME
        lines.append("car %s %s %s %s %08x" % (label, cars_dir, sgc, physics, hh))
        for gid in range(1, 10):
            n = entry.read(grid(gid) + 0x68D, 1)[0]
            words = [w for k in range(n) for w in struct.unpack("<3I", entry.read(grid(gid) + 0x40 + 16 * k, 12))]
            lines.append(" ".join(["emit", label, str(gid), str(n)] + ["%08x" % w for w in words]))
        for gid in range(1, 10):
            lines.append("grid %s %d %s" % (label, gid, snap.read(grid(gid), GRID_SIZE).hex()))
        descs = descriptors(snap, {gid: snap.u32(grid(gid) + 0x410) for gid in range(1, 9)})
        for gid, at in sorted(descs.items()):
            d = snap.read(at, DESC_SIZE)
            lines.append("desc %s %d %08x %s" % (label, gid, at, d.hex()))
            word = lambda o: struct.unpack_from("<I", d, o)[0]  # noqa: E731
            verts, faces, samples, map_a, map_b = (word(o) for o in (0x10, 0x14, 0x1C, 0x24, 0x28))
            nv, nf = struct.unpack_from("<2H", d, 0xC)
            ns = word(0x18)
            width, height = struct.unpack_from("<2H", d, 0x20)
            for kind, at2, length, digest in [
                    ("vertices", verts, 16 * nv, False), ("faces", faces, 7 * nf, False),
                    ("samples", samples, 2 * ns, False), ("map_a", map_a, width * width, True),
                    ("map_b", map_b, height * height, True)]:
                if not at2 or not length:
                    continue
                data = snap.read(at2, length)
                lines.append("block %s %d %s %08x %d %s" % (
                    label, gid, kind, at2, length, fnv64(data) if digest else data.hex()))
        print("%s: %s, %s (%d descriptors)" % (label, entry_path, snap_path, len(descs)))
    with open(sys.argv[1], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
