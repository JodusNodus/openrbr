"""Which code reads a memory range during one physics step? Replays each snapshot's CPhysics::StepForward (as
replay.py does, see README.md) with a read hook on the ranges, and prints the reads per address and reading eip.

Usage: RBR_REMOTE=<build dir on cachy> python3 -I harness/read_hook.py [--object NAME=HEX]... RANGE... -- SNAPSHOT...
  RANGE is HEX:LEN (an absolute range) or NAME+HEX:LEN (LEN bytes at object NAME plus HEX, e.g. car+0x31c:4).
  --object NAME=HEX gives the address of an object (e.g. car=0xbf1d7b8). Each read prints its address, size and
  the eip of the instruction that read it; the step's stop reason is printed too.
"""
import argparse
import collections
import os
import sys

HARNESS = os.path.dirname(os.path.abspath(__file__))
sys.path.append(HARNESS)  # appended: harness/bisect.py must not shadow the stdlib
import replay  # noqa: E402
from unicorn import UC_HOOK_MEM_READ  # noqa: E402
from unicorn.x86_const import UC_X86_REG_EIP  # noqa: E402


def parse_range(text, objects):
    where, _, length = text.partition(":")
    if "+" in where:
        name, _, offset = where.partition("+")
        base = objects[name] + int(offset, 16)
    else:
        base = int(where, 16)
    return base, int(length, 0)


def step(path, ranges):
    """The first replay of `path` whose FPU oracle settles: the machine and the reads (addr, eip, size)."""
    snap = replay.Snapshot(path)
    oracle = replay.FpuOracle()
    for _ in range(8):
        m = replay.Machine(snap, oracle=oracle, log_writes=False)
        reads = []

        def on_read(mu, access, addr, size, value, _, reads=reads):
            reads.append((addr, mu.reg_read(UC_X86_REG_EIP), size))

        for at, length in ranges:
            m.mu.hook_add(UC_HOOK_MEM_READ, on_read, begin=at, end=at + length - 1)
        m.run()
        if not m.fpu_missing:
            return m, reads
    raise SystemExit("oracle did not settle")


def main():
    ap = argparse.ArgumentParser(description="Print the eips that read memory ranges during one physics step.")
    ap.add_argument("--object", action="append", default=[], metavar="NAME=HEX")
    ap.add_argument("ranges", nargs="+", metavar="RANGE")
    argv = sys.argv[1:]
    if "--" not in argv or argv.index("--") == len(argv) - 1:
        raise SystemExit("usage: read_hook.py [--object NAME=HEX] RANGE... -- SNAPSHOT...")
    split = argv.index("--")
    args = ap.parse_args(argv[:split])
    snapshots = argv[split + 1 :]
    objects = {}
    for item in args.object:
        name, _, value = item.partition("=")
        objects[name] = int(value, 16)
    ranges = [parse_range(t, objects) for t in args.ranges]
    for path in snapshots:
        m, reads = step(path, ranges)
        by = collections.Counter((hex(a), hex(eip), size) for a, eip, size in reads)
        print(os.path.basename(path), "stop:", m.stop, "reads:", len(reads))
        for (a, eip, size), n in sorted(by.items()):
            print("   ", a, "size", size, "read at eip", eip, "x", n)


if __name__ == "__main__":
    main()
