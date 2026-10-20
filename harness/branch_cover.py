#!/usr/bin/env python3
"""Branch coverage of direct calls: which way each conditional jump of the listed functions went, over the calls
harness/direct_calls.py makes with the same arguments (the same recipe and seed give the same inputs). No records are
written. Function bounds come from rbr-recomp's data/rbr.functions.csv (RBR_FUNCTIONS), the asm from llvm-objdump.

    python3 -I harness/branch_cover.py FUNC[,FUNC...] SNAPSHOT --set SET --addr A [--count N] [--seed S]

FUNC is a hex address (the called function and any callee to report). A jump never taken or never fallen through is
marked; a function that is not entered is listed with no jumps reached.
"""

import collections
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.append(HERE)

import direct_calls  # noqa: E402
import replay  # noqa: E402
import vcu_vectors  # noqa: E402
from unicorn import UC_HOOK_CODE  # noqa: E402

EXE = os.environ.get("RBR_ORIG_EXE") or os.path.join(os.environ.get("RBR_PATCH", ""), "RichardBurnsRally_SSE.exe")
FUNCTIONS = os.environ.get("RBR_FUNCTIONS", os.path.expanduser("~/Projects/rbr-recomp-cpp/data/rbr.functions.csv"))
OBJDUMP = os.environ.get("LLVM_OBJDUMP", "llvm-objdump")


def sizes():
    out = {}
    for line in open(FUNCTIONS):
        addr, _, size = line.strip().partition("|")
        if size.isdigit():
            out[int(addr, 16)] = int(size)
    return out


def jumps(start, size):
    """{jump address: (mnemonic, target, fall-through)} of the conditional jumps in [start, start + size)."""
    text = subprocess.run([OBJDUMP, "-d", "--no-show-raw-insn", "-M", "intel", "--start-address=%d" % start,
                           "--stop-address=%d" % (start + size), EXE], capture_output=True, text=True, check=True).stdout
    ins = [(int(m.group(1), 16), m.group(2), m.group(3)) for m in
           (re.match(r"\s+([0-9a-f]+):\s+(\S+)\s*(.*)", line) for line in text.splitlines()) if m]
    return {a: (op, int(rest.split()[0], 16), ins[i + 1][0]) for i, (a, op, rest) in enumerate(ins[:-1])
            if op.startswith("j") and op != "jmp"}


def main():
    funcs = [int(x, 16) for x in sys.argv[1].split(",")]
    bounds = sizes()
    tables = {f: jumps(f, bounds[f]) for f in funcs}
    every = set().union(*tables.values())
    seen, entries, prev = collections.Counter(), collections.Counter(), [None]

    def hook(mu, addr, size, _):
        if prev[0] is not None:
            seen[(prev[0], addr)] += 1
        prev[0] = addr if addr in every else None
        entries[addr] += addr in tables

    init = replay.Machine.__init__

    def machine(self, *a, **k):
        init(self, *a, **k)
        prev[0] = None
        for f in funcs:
            self.mu.hook_add(UC_HOOK_CODE, hook, begin=f, end=f + bounds[f] - 1)

    replay.Machine.__init__ = machine
    vcu_vectors.write = lambda path, calls: None
    sys.argv = [sys.argv[0], sys.argv[2], os.devnull] + sys.argv[3:]
    direct_calls.main()
    for f in funcs:
        print("%#x entered %d" % (f, entries[f]))
        for a, (op, target, fall) in sorted(tables[f].items()):
            taken, fell = seen[(a, target)], seen[(a, fall)]
            note = "" if taken and fell else "  <-- " + ("not reached" if not (taken or fell) else
                                                        "never taken" if not taken else "never falls through")
            print("  %x %-4s taken %5d  falls %5d%s" % (a, op, taken, fell, note))


if __name__ == "__main__":
    main()
