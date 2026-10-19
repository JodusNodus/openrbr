"""Writes the Rust load's built objects over a snapshot, as a new snapshot of the same format (level 3 of
lead/reports/rust-load-path.md, "Composed load").

Usage: python3 -I -B harness/compose_load.py BASE OUT PATCH... [--written-only] [--poison-later]

The patches are the load tests' builds (rust/rbr-physics/tests/common/patch.rs, with RBR_LOAD_OUT set:
`definitions`, `car-instance` from load_defs.rs, `stage-world` from load_stage.rs, `car-grids` from load_car_grids.rs).
A patch file is a run of objects:
u32 address, u32 length n, u8 kind length, the kind, then n bytes each: ours (the build on 0x00), the class of each
byte, and the game's bytes it was compared with. The classes:
  w  written by the build (equal to the game's)            -> ours
  u  written by nothing in the build (the fill)           -> ours (0x00), unless --written-only
  W  as w, at a factory's return (the car instance)       -> ours where BASE still holds the game's byte there
  U  as u, at a factory's return                          -> as W, unless --written-only
  n  a name word (points into the test's names)           -> BASE's
  g  the game's bytes, uninitialised in the original      -> BASE's
  x  compared in another object (a solid's links)          -> BASE's
  l  written by load code not built (after the build)     -> BASE's, or 0xcd with --poison-later
A W or U byte in an aligned word where BASE differs from the game's at the factory's return was written by load code
between that and BASE (E46_Notify67d0, UpdateA720, the start pose): BASE's is kept, and counted. Prints, per kind and class, the bytes
written, those of them that change BASE, and the bytes kept.
"""
import collections
import os
import shutil
import struct
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402

POISON = 0xCD


def objects(path):
    """(kind, address, ours, class, game) of each object of a patch file."""
    d = open(path, "rb").read()
    p = 0
    while p < len(d):
        at, n, k = struct.unpack_from("<IIB", d, p)
        kind = d[p + 9:p + 9 + k].decode()
        q = p + 9 + k
        yield kind, at, d[q:q + n], d[q + n:q + 2 * n], d[q + 2 * n:q + 3 * n]
        p = q + 3 * n


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    written_only, poison = "--written-only" in sys.argv, "--poison-later" in sys.argv
    if len(args) < 3:
        sys.exit(__doc__)
    base_path, out_path, patches = args[0], args[1], args[2:]
    base = replay.Snapshot(base_path)
    shutil.copyfile(base_path, out_path)
    counts = collections.defaultdict(lambda: [0, 0, 0])  # (kind, class): written, changed, kept
    with open(out_path, "r+b") as f:
        for path in patches:
            for kind, at, ours, cls, game in objects(path):
                have = base.read(at, len(ours))
                new = bytearray(have)
                # by aligned word: later code that rewrote a word can leave a byte of it as it was
                still = lambda i: all(have[j - at] == game[j - at] for j in range(  # noqa: E731
                    max(at, (at + i) & ~3), min(at + len(ours), ((at + i) & ~3) + 4)))
                for i, c in enumerate(cls):
                    c = chr(c)
                    if c in "wu" and not (written_only and c == "u"):
                        new[i] = ours[i]
                    elif c in "WU" and not (written_only and c == "U") and still(i):
                        new[i] = ours[i]
                    elif c == "l" and poison:
                        new[i] = POISON
                    else:
                        counts[kind, c][2] += 1
                        continue
                    counts[kind, c][0] += 1
                    counts[kind, c][1] += new[i] != have[i]
                write(f, base, at, bytes(new))
    total = [0, 0, 0]
    for (kind, c), n in sorted(counts.items()):
        print("%-18s %s: %8d written, %7d of them change the base; %7d kept" % (kind, c, *n))
        total = [a + b for a, b in zip(total, n)]
    print("total: %d written, %d of them change the base; %d kept" % tuple(total))
    print("wrote", out_path)


def write(f, snap, at, data):
    """data at address at, into the file of snap's layout (the regions' file offsets)."""
    done = 0
    while done < len(data):
        a = at + done
        i = replay.bisect.bisect_right(snap.bases, a) - 1
        base, size, off = snap.regions[i][:3]
        if not base <= a < base + size:
            raise KeyError("%#x not in the snapshot" % a)
        assert not snap.stack_lo <= a < snap.stack_lo + len(snap.stack), "an object in the dump's stack"
        k = min(len(data) - done, base + size - a)
        f.seek(off + a - base)
        f.write(data[done:done + k])
        done += k


if __name__ == "__main__":
    main()
