"""NGP.dll's `.data` words from a snapshot of an NGP run, as reference vectors for the Rust port of NGP's load-time
init (rust/rbr-physics/tests/load_defs.rs, `ngp_car_init_matches_the_game`).

Usage: RBR_NGP=original/ngp .venv/bin/python -I -B harness/ngp_data_refs.py SNAPSHOT OUT

Finds NGP.dll in the snapshot's loader list and writes the words of [0x10171000, 0x10178000) (its `.data`, at file
addresses) whose value is not what the file (`$RBR_NGP/7.4.778/Plugins/NGP.dll`) holds there (a relocated pointer
counts as the file's). OUT gets a header line naming the snapshot, then one line per word: `<file address> <hex word>`.
"""
import os
import struct
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

FILE_BASE = 0x10000000
LO, HI = 0x10171000, 0x10178000


def file_words(path):
    """The file's words of [LO, HI) (0 past the section's raw data)."""
    dll = open(path, "rb").read()
    pe, = struct.unpack_from("<I", dll, 0x3C)
    count, = struct.unpack_from("<H", dll, pe + 6)
    optional, = struct.unpack_from("<H", dll, pe + 20)
    out = bytearray(HI - LO)
    for i in range(count):
        _, va, rsize, raw = struct.unpack_from("<IIII", dll, pe + 24 + optional + 40 * i + 8)
        lo, hi = max(LO, FILE_BASE + va), min(HI, FILE_BASE + va + rsize)
        if lo < hi:
            out[lo - LO:hi - LO] = dll[raw + lo - FILE_BASE - va:raw + hi - FILE_BASE - va]
    return out


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    found = [(base, path) for base, _, name, path in snap.modules() if name.lower() == "ngp.dll"]
    if not found:
        sys.exit("NGP.dll is not loaded in the snapshot")
    base, _ = found[0]
    want = file_words(os.path.join(os.environ["RBR_NGP"], "7.4.778", "Plugins", "NGP.dll"))
    have = snap.read(LO - FILE_BASE + base, HI - LO)
    with open(sys.argv[2], "w") as out:
        out.write("# %s\n" % os.path.basename(sys.argv[1]))
        for o in range(0, HI - LO, 4):
            f, = struct.unpack_from("<I", want, o)
            c, = struct.unpack_from("<I", have, o)
            if c != f and (c - f) & 0xFFFFFFFF != base - FILE_BASE:
                out.write("%08x %08x\n" % (LO + o, c))


if __name__ == "__main__":
    main()
