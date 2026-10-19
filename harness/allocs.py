"""The exe's heap calls from a run's allocation log (RBR_ALLOCLOG=1 harness/play.sh: <name>.allocs.bin), named by the
function each call came from.

Usage: python3 -I -B harness/allocs.py ALLOCS [--until-snap STEP] [--from ADDR] [--to ADDR]

Prints one line per record: `<index> <kind> <caller function> <return address> <size or block> <result>`, and
`dump <version> <step or entry address> <substeps or ordinal>` where the plugin closed a dump. `--until-snap STEP`
stops at the dump of step STEP (version 1: snap-<STEP>); `--from`/`--to` keep the calls whose function lies in
[ADDR, TO). The function of a return address comes from data/rbr.functions.csv.
"""
import bisect
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KINDS = {1: "new", 2: "malloc", 3: "calloc", 4: "realloc", 5: "strdup", 6: "delete", 7: "delete[]", 8: "free",
         9: "dump"}
RECORD = struct.Struct("<6I")


def records(path):
    """The log's records: (kind, thread, ret, a, b, result)."""
    data = open(path, "rb").read()
    magic, version = struct.unpack_from("<II", data)
    if magic != 0x41524252 or version != 1:
        sys.exit("%s: not an allocation log" % path)
    return [RECORD.unpack_from(data, at) for at in range(8, len(data) - RECORD.size + 1, RECORD.size)]


def functions():
    """The original functions' starts and sizes, sorted."""
    rows = []
    with open(os.path.join(ROOT, "data", "rbr.functions.csv")) as f:
        next(f)
        for line in f:
            start, size = line.strip().split("|")
            rows.append((int(start, 16), int(size)))
    return sorted(rows)


def caller(rows, starts, ret):
    """The function holding the call that returns to `ret`, or None."""
    i = bisect.bisect_right(starts, ret - 1) - 1
    if i >= 0 and rows[i][0] < ret <= rows[i][0] + rows[i][1]:
        return rows[i][0]
    return None


def main():
    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        sys.exit(__doc__)
    path, opts = args[0], dict(zip(args[1::2], args[2::2]))
    until = int(opts["--until-snap"]) if "--until-snap" in opts else None
    lo = int(opts.get("--from", "0"), 16)
    hi = int(opts.get("--to", "ffffffff"), 16)
    rows = functions()
    starts = [r[0] for r in rows]
    for i, (kind, _thread, ret, a, b, result) in enumerate(records(path)):
        if kind == 9:
            print("%d dump %d %#x %d" % (i, a, b, ret))
            if until is not None and a == 1 and b == until:
                break
            continue
        fn = caller(rows, starts, ret)
        if fn is None or not lo <= fn < hi:
            continue
        print("%d %s %08x %08x %#x %#x" % (i, KINDS.get(kind, kind), fn, ret, a, result))


if __name__ == "__main__":
    main()
