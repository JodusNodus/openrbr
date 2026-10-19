"""The stage world's objects from a snap-0 dump, as reference vectors for the Rust load path
(rust/rbr-physics/tests/load_stage.rs, step 5 of the load path).

Usage: .venv/bin/python -I -B harness/load_stage_refs.py SNAPSHOT OUT COL MAT

COL and MAT are the stage's retail .col and .mat files (their lengths give the images' lengths). The objects are found
from the game's static roots: the collision system ([0x7c755c]) and its .col image (+4), the brake-wall manager
([0x7c63c8]) and its wall (+4), the cell grid ([0x8eb794]), the mesh array (0x1657030) and its waters, the E44
material table (0x16592b8) and the .mat image (the track state's +0x148, from the race host [0x7ea678] +0x70); the
physics state's entity lists ([[0x1659b70] +0x20] +0x450..+0x490) and the ground object filed before the wall; the
drive line ([0x1659184]); and the .trk collision meshes: the track state's mesh list (+0x360) and per mesh its
descriptor, name, shape record, vertex and face arrays, and the collision grid of its id (0x8f7030 + id * 0x6b0).
The placements' objects follow the waters in the physics state's list +0x460: per solid (`CSolid`, 0x3d0 bytes) the
FNV-1a digest of its bytes with SOLID_MASK zeroed (`solid`), its entity link (`solid_link`, +0x10), its cell link
(`solid_cell`, +0x160) and a nonzero sort key (`solid_key`, +0x30); the link blocks they reach first
(`placement_block`, in that order) and the mask itself (`solid_mask`: 0xff for each masked byte). The load's inputs
for them: the physics object (`physics`), its definition's debug words (`physics_flags`), its parent stack's top
(`parent_top`), the definition registry (`registry`), the first 8 bytes of each solid's definition (`solid_def`), and
the body event signal's words its emit reads (`env`: the signal object's pointer, the signal's busy word, current type
and lists, their connections and the connections' targets' words at +0x10), as snap-0 has them.

OUT gets a header line naming the snapshot, then one line per object: `<kind> <position> <address> <payload>`. The
payload is the object's bytes in hex, or for the two file images `fnv:<length>:<digest>,...`: the FNV-1a 64 digest of
each 4 KiB page; the drive line's entries past its count come as `drive_tail` digests of the same kind, with each
entry's tag and the 8 bytes after it zeroed (SetDriveLine leaves them). The cell table is cut sparse: one `cell_slot` line per nonzero entry. The link blocks the waters
hang in come as `link_block` lines, in the order the waters first reach them (the order the Rust build allocates
them).
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

COLLISION_SYSTEM = 0x7C755C
WALL_MANAGER = 0x7C63C8
CELL_GRID = 0x8EB794
MESH_ARRAY = 0x1657030
MAT_TABLE = 0x16592B8
RACE_HOST = 0x7EA678
PHYSICS = 0x1659B70
DRIVE_LINE = 0x1659184
GRID_BANK = 0x8F7030
GRID_SIZE = 0x6B0
PAGE = 0x1000
LINK_BLOCK = 0x644
REGISTRY = 0x7C8040
SOLID_VTABLE = 0x73FC54
SOLID_SIZE = 0x3D0
SIGNALS = 0x893600
BODY_EVENT = 0xEC98
# The solid's bytes left out of its digest: the links (in their own lines), the bounds' w lanes (+0x8c, +0x9c: SyncGrid's
# min and max of the transformed edges' w, which the matrix's unset tenth word, +0x134, reaches) and the bytes the load
# leaves unset (the Rust build's 0x00/0xff builds differ there).
SOLID_MASK = [(0x10, 0x1C), (0x160, 0x16C), (0x8C, 0x90), (0x9C, 0xA0),
              (0x68, 0x70), (0x7C, 0x80), (0xB4, 0xC0), (0xC4, 0xD0), (0xD4, 0xE0), (0xEC, 0xF0), (0xFC, 0x100),
              (0x134, 0x140), (0x14C, 0x150), (0x18C, 0x190), (0x1A4, 0x1B0), (0x1BC, 0x1C0), (0x1FC, 0x200),
              (0x21C, 0x220), (0x278, 0x280), (0x284, 0x290), (0x294, 0x2A0), (0x2B0, 0x2E0), (0x394, 0x3A0),
              (0x3B8, 0x3C0), (0x3C4, 0x3C8)]


def fnv(data):
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def pages(snap, at, n):
    data = snap.read(at, n)
    return "fnv:%d:%s" % (n, ",".join("%016x" % fnv(data[i:i + PAGE]) for i in range(0, n, PAGE)))


def connections(snap, first):
    """The body event's connections from `first` on (0x18 bytes) and their targets' words at +0x10, as (address, size)."""
    out = []
    while first:
        out += [(first, 0x18), (snap.u32(first + 0x10) + 0x10, 4)]
        first = snap.u32(first + 4)
    return out


def body_event(snap):
    """The words the body event's emit reads, as (address, size)."""
    signal = snap.u32(SIGNALS) + BODY_EVENT
    out = [(SIGNALS, 4), (signal, 12), (signal + 0x208, 4)] + connections(snap, snap.u32(signal + 0x208))
    current = snap.u32(signal + 4)
    if current:
        by_type = signal + 8 + 0x10 * snap.u32(current)
        out += [(current, 4), (by_type, 4)] + connections(snap, snap.u32(by_type))
    return out


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    col_len, mat_len = (os.path.getsize(p) for p in sys.argv[3:5])
    lines = ["# %s" % os.path.basename(sys.argv[1])]

    def obj(kind, position, at, n):
        lines.append("%s %s %#x %s" % (kind, position, at, snap.read(at, n).hex()))

    system = snap.u32(COLLISION_SYSTEM)
    obj("col_system", 0, system + 4, 0x1010)
    lines.append("col_image 0 %#x %s" % (snap.u32(system + 4), pages(snap, snap.u32(system + 4), col_len)))
    manager = snap.u32(WALL_MANAGER)
    obj("wall_manager", 0, manager + 4, 4)
    obj("brake_wall", 0, snap.u32(manager + 4), 0x21C)

    grid = snap.u32(CELL_GRID)
    obj("cell_grid", 0, grid, 0x14)
    table = snap.u32(grid + 0x10)
    obj("cell_table", 0, table - 4, 4)
    blocks = {}
    for i in range(snap.u32(table - 4)):
        block = snap.u32(table + 4 * i)
        if block:
            obj("cell_slot", i, table + 4 * i, 4)
            blocks[block] = i

    obj("mesh_array", 0, MESH_ARRAY, 0x804)
    seen = []
    for k in range(snap.u32(MESH_ARRAY + 0x800)):
        water = snap.u32(MESH_ARRAY + 4 * k)
        obj("water", k, water, 0xC0)
        owner = snap.u32(water + 0x98)
        block = next(b for b in blocks if b <= owner < b + LINK_BLOCK - 4)
        if block not in seen:
            seen.append(block)
            obj("link_block", len(seen) - 1, block - 4, LINK_BLOCK)

    obj("mat_table", 0, MAT_TABLE, 0x808)
    track = snap.u32(snap.u32(RACE_HOST) + 0x70)
    obj("track_mat", 0, track + 0x148, 4)
    image = snap.u32(track + 0x148)
    lines.append("mat_image 0 %#x %s" % (image, pages(snap, image, mat_len)))

    state = snap.u32(snap.u32(PHYSICS) + 0x20)
    obj("physics_state", 0, snap.u32(PHYSICS) + 0x20, 4)
    obj("entity_lists", 0, state + 0x450, 0x40)
    ground = snap.u32(snap.u32(manager + 4) + 0x14)
    obj("ground_link", 0, ground + 0x10, 0xC)

    line = snap.u32(DRIVE_LINE)
    count = snap.u32(line + 0x75310)
    obj("drive_count", 0, line + 0x75310, 4)
    for k in range(count):
        obj("drive_entry", k, line + 0x30 * k + 0x10, 0x28)
    tail = bytearray(snap.read(line + 0x30 * count + 0x10, 0x30 * (10000 - count)))
    for k in range(10000 - count):
        tail[0x30 * k + 0x24:0x30 * k + 0x30] = bytes(12)
    lines.append("drive_tail %d %#x fnv:%d:%s" % (count, line + 0x30 * count + 0x10, len(tail), ",".join(
        "%016x" % fnv(tail[i:i + PAGE]) for i in range(0, len(tail), PAGE))))

    physics = snap.u32(PHYSICS)
    obj("physics", 0, physics, 0x40)
    obj("physics_flags", 0, snap.u32(physics + 0x24) + 0x160CC, 0x2C)
    obj("parent_top", 0, snap.u32(physics + 0x28) + 0xE888, 4)
    obj("registry", 0, REGISTRY, 0x1C + 0x14 * snap.u32(REGISTRY + 8))
    solids = []
    at = snap.u32(state + 0x460)
    while at:
        if snap.u32(at) == SOLID_VTABLE:
            solids.append(at)
        at = snap.u32(at + 0x10)
    for k, d in enumerate(sorted({snap.u32(o + 0x34) for o in solids})):
        obj("solid_def", k, d, 8)
    for at, size in body_event(snap):
        obj("env", 0, at, size)
    mask = bytearray(SOLID_SIZE)
    for lo, hi in SOLID_MASK:
        mask[lo:hi] = b"\xff" * (hi - lo)
    lines.append("solid_mask 0 0 %s" % mask.hex())
    for k, o in enumerate(solids):
        data = bytes(0 if m else b for b, m in zip(snap.read(o, SOLID_SIZE), mask))
        lines.append("solid %d %#x fnv:%d:%016x" % (k, o, SOLID_SIZE, fnv(data)))
        obj("solid_link", k, o + 0x10, 0xC)
        obj("solid_cell", k, o + 0x160, 0xC)
        if snap.u32(o + 0x30):
            obj("solid_key", k, o + 0x30, 4)
        owner = snap.u32(o + 0x168)
        block = next(b for b in blocks if b <= owner < b + LINK_BLOCK - 4)
        if block not in seen:
            seen.append(block)
            obj("placement_block", len(seen) - 1, block - 4, LINK_BLOCK)

    first, last = snap.u32(track + 0x364), snap.u32(track + 0x368)
    obj("mesh_list", 0, first, last - first)
    for k in range(0, (last - first) // 4):
        desc = snap.u32(first + 4 * k)
        obj("mesh_desc", k, desc, 0x34)
        name = snap.u32(desc + 8)
        obj("mesh_name", k, name, snap.read(name, 0x200).index(0) + 1)
        flags, shape = snap.u32(desc + 4), snap.u32(desc + 0x2C)
        if shape:
            obj("mesh_shape", k, shape, 0x50 if flags & 0x20000 else 0x20)
        counts = snap.read(desc + 0xC, 4)
        obj("mesh_vertices", k, snap.u32(desc + 0x10), 16 * (counts[0] | counts[1] << 8))
        obj("mesh_faces", k, snap.u32(desc + 0x14), 7 * (counts[2] | counts[3] << 8))
        if flags & 2 == 0 and snap.u32(desc) != 0:
            obj("col_grid", snap.u32(desc), GRID_BANK + GRID_SIZE * snap.u32(desc), GRID_SIZE)
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
