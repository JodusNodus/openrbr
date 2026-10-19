"""The car's definition from a snap-0 dump, with the heap blocks its engine definition allocates, as reference vectors
for the Rust load path (rust/rbr-physics/tests/load_defs.rs).

Usage: .venv/bin/python -I -B harness/load_car_refs.py SNAPSHOT OUT

Reads the player car (*0x8ef660) and its definition (car+0x34, 0x44e0 bytes: `CCar::CDef`, with the control unit,
drive, engine, light system and passenger definitions inside). The engine definition (+0x1990) points at two arrays
(+0xa0, 10 elements; +0xa8, 3 elements; 0xc bytes each), and each element at a 0x54-byte block (+8): the 15 blocks
follow the definition, in the order the constructor allocates them (an array, then its elements' blocks).
OUT gets a header line naming the snapshot, then one line per object: `<kind> <index> <address> <hex bytes>
[<offset>=<hex name>...]`, where each `<offset>=` field is a name word of a definition that points outside the exe
image (a token-heap copy the walk made) and the NUL-terminated name it points to.
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

DEF_SIZE = 0x44E0
# the car definition and the definitions inside it, each with its name word at +4
NAMED = (0x0, 0x250, 0xEFC, 0x1990, 0x4210, 0x440C, 0x4428)
IMAGE = range(0x400000, 0x1700000)
ENGINE = 0x1990
# (count word, array word) of the engine definition's two arrays
ARRAYS = ((0x9C, 0xA0), (0xA4, 0xA8))
ELEMENT, BLOCK = 0xC, 0x54


def c_string(snap, at):
    out = bytearray()
    while not out.endswith(b"\0"):
        out += snap.read(at + len(out), 1)
    return bytes(out)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    car = snap.u32(0x8EF660)
    at = snap.u32(car + 0x34)
    names = ["%x=%s" % (o + 4, c_string(snap, snap.u32(at + o + 4)).hex()) for o in NAMED
             if snap.u32(at + o + 4) not in IMAGE]
    lines = ["# %s" % os.path.basename(sys.argv[1]),
             " ".join(["car_def", "0", "%#x" % at, snap.read(at, DEF_SIZE).hex()] + names)]
    k = 0
    for count, array in ARRAYS:
        n, base = snap.u32(at + ENGINE + count), snap.u32(at + ENGINE + array)
        blocks = [(base, n * ELEMENT)] + [(snap.u32(base + i * ELEMENT + 8), BLOCK) for i in range(n)]
        for block, size in blocks:
            lines.append("engine_block %d %#x %s" % (k, block, snap.read(block, size).hex()))
            k += 1
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
