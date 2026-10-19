#!/usr/bin/env python3
"""Direct calls of one listed function, from a snapshot, for functions a step does not call (or calls on values the
step does not produce). Each call starts at the function's entry, as the step's own call would. A spec with a
"direct_fuzz" recipe (mu, heap, rng) -> (stack words, [(address, bytes)] to write first, this) builds its own inputs
and object in the replay's stub heap, and the recipe's this is ecx. Otherwise the object is --this (the car, say, in
the snapshot's memory), the stack arguments come from "direct_args" (rng, this) -> [words], and the inputs from "fuzz"
(mu, args, rng, put) (the same hook --fuzz uses inside a step). The records are the original's, in the vcu_vectors.py
format, so a test replays them like a step's records.

    python3 -I harness/direct_calls.py SNAPSHOT OUT.txt --set SET [--addr 0xADDR]... [--this HEX] [--count N] [--seed S]

--addr defaults to every function of the set with a "direct_fuzz" recipe. --this is needed for the others. Needs the
worktree's .venv (unicorn, numpy) and RBR_REMOTE (the x87 oracle, see harness/README.md).
"""

import argparse
import random
import struct
import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.append(HERE)  # appended, as in vcu_vectors.py: harness/bisect.py must not shadow the stdlib

import vcu_vectors as v  # noqa: E402
import replay  # noqa: E402
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_EIP  # noqa: E402


def run_once(snap, oracle, addr, spec, this, rng, limit):
    """One call. Returns (records, missing): missing is the x87 results the oracle must fetch before a rerun."""
    root_spec = v.SPECS[addr]
    # External entries normally appear only inside their listed caller's `inner` records. When
    # direct_calls invokes one as the root, record that root itself while leaving its callees external.
    if root_spec.get("external"):
        v.SPECS[addr] = dict(root_spec, external=False)
    try:
        m = v.replay.Machine(snap, oracle=oracle, log_writes=False)
        rec = v.Recorder(m, limit)
        mu = m.mu
        esp = mu.reg_read(UC_X86_REG_ESP)
        if "direct_fuzz" in spec:
            words, writes, this = spec["direct_fuzz"](mu, m.heap_base, rng)
            for at, data in writes:
                mu.mem_write(at, data)
        else:
            words = list(spec["direct_args"](rng, this)) if "direct_args" in spec else []
        for i, word in enumerate(words):
            mu.mem_write(esp + 4 + 4 * i, struct.pack("<I", word & 0xFFFFFFFF))
        if spec.get("thiscall"):
            mu.reg_write(UC_X86_REG_ECX, this)
        if "fuzz" in spec:
            def put(at, value, code="<f"):
                mu.mem_write(at, struct.pack(code, value))
            spec["fuzz"](mu, (this,) + tuple(words), rng, put)
        try:
            mu.emu_start(addr, replay.SENTINEL, timeout=60 * 1000000)
        except Exception as e:  # unicorn stops this run
            m.stop = m.stop or str(e)
        if mu.reg_read(UC_X86_REG_EIP) == replay.SENTINEL:
            rec.on_return(mu, replay.SENTINEL, 0, None)
        if m.stop and not m.fpu_missing and mu.reg_read(UC_X86_REG_EIP) != replay.SENTINEL:
            sys.exit("run stopped: %s" % m.stop)
        return rec.calls, m.fpu_missing
    finally:
        if root_spec.get("external"):
            v.SPECS[addr] = root_spec


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshot")
    ap.add_argument("out")
    ap.add_argument("--set", required=True, choices=sorted(v.SETS), help="the vector set (SPECS) the function is in")
    ap.add_argument("--addr", action="append", type=lambda s: int(s, 0),
                    help="a listed function's address (repeatable); default: each with a direct_fuzz recipe")
    ap.add_argument("--this", type=lambda s: int(s, 16), help="the object (hex) for ecx, without a direct_fuzz recipe")
    ap.add_argument("--count", type=int, default=40, help="calls (each with new random inputs)")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--max", type=int, default=100000)
    a = ap.parse_args()
    v.SPECS = v.SETS[a.set]
    addrs = a.addr or sorted(addr for addr, spec in v.SPECS.items() if "direct_fuzz" in spec)
    for addr in addrs:
        if addr not in v.SPECS:
            sys.exit("%#x is not in the %s set" % (addr, a.set))
        if "direct_fuzz" not in v.SPECS[addr] and a.this is None:
            sys.exit("%#x has no direct_fuzz recipe: give --this" % addr)
    snap = replay.Snapshot(a.snapshot)
    oracle = replay.FpuOracle()
    rng = random.Random(a.seed)
    calls = []
    for addr in addrs:
        for _ in range(a.count):
            # A rerun takes the same inputs: the oracle fetches what the run missed, so new inputs would miss again.
            state = rng.getstate()
            for _ in range(64):
                rng.setstate(state)
                got, missing = run_once(snap, oracle, addr, v.SPECS[addr], a.this, rng, a.max)
                if not missing:
                    calls += got
                    break
                oracle.fetch(missing)
            else:
                sys.exit("x87 results still missing after 64 runs")
    v.write(a.out, calls)
    counts = {}
    for c in calls:
        key = (v.SPECS[c["addr"]]["name"], c["branch"])
        counts[key] = counts.get(key, 0) + 1
    print("%s: %d calls recorded" % (a.snapshot, len(calls)))
    for (name, branch), n in sorted(counts.items()):
        print("  %-28s %-8s %d" % (name, branch, n))


if __name__ == "__main__":
    main()
