"""Object bytes from a load-time dump, as reference vectors for the Rust load path (rust/rbr-physics/tests/).

Usage: .venv/bin/python -I -B harness/load_refs.py SNAPSHOT OUT SPEC...

Each SPEC is ADDR:LEN (LEN bytes at ADDR) or *ADDR (the NUL-terminated string the word at ADDR points to, NUL
included). OUT gets a header line naming the snapshot, then one line per SPEC: `<spec> <address> <hex bytes>`.
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402


def window(snap, spec):
    if spec.startswith("*"):
        at = snap.u32(int(spec[1:], 0))
        out = bytearray()
        while not out.endswith(b"\0"):
            out += snap.read(at + len(out), 1)
        return at, bytes(out)
    at, n = (int(x, 0) for x in spec.split(":"))
    return at, snap.read(at, n)


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    snap = replay.Snapshot(sys.argv[1])
    lines = ["# %s" % os.path.basename(sys.argv[1])]
    for spec in sys.argv[3:]:
        at, data = window(snap, spec)
        lines.append("%s %#x %s" % (spec, at, data.hex()))
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
