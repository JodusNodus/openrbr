"""The functions the original physics step reaches and who calls them, over chains of the original (run A of
rust_step.py --steps: the recording's control, then StepForward(1)), for the pure-step coverage
(scripts/progress.py step, lead/reports/pure-step.md).

Usage: .venv/bin/python -I -B harness/step_reached.py SNAPSHOT... --recording DRIVE.bin --steps N --out REACHED.txt
                                                       [--until STEP]

  Each snapshot's chain runs N steps, or up to the next snapshot's step (or --until) if that comes first (give the
  snapshots of one drive in step order to cover it once).

Output, entries summed over the snapshots' chains:

    fn ADDR ENTRIES                 a function (data/rbr.functions.csv) entered ENTRIES times
    edge CALLER CALLEE ENTRIES      CALLER holds the return address (0 for the chain's roots: StepForward and the
                                    control's Emit); a function reached by a tail jump counts for its caller's caller
"""

import argparse
import bisect
import collections
import os
import struct
import sys

from unicorn import UC_HOOK_BLOCK
from unicorn.x86_const import UC_X86_REG_ESP

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402
import rust_step  # noqa: E402
import worklist  # noqa: E402


def chain(path, recording, steps, o, starts):
    """({fn: entries}, {(caller, callee): entries}, the machine, the stop or None) of one chain; a stopped chain's
    counts run up to the stop."""
    snap = replay.Snapshot(path)
    controls = rust_step.recorded_controls(recording)
    m = rust_step.machine(snap, o)
    entries, edges, known = collections.Counter(), collections.Counter(), set(starts)

    def block(mu, addr, size, _):
        if addr not in known:
            return
        ret = struct.unpack("<I", mu.mem_read(mu.reg_read(UC_X86_REG_ESP), 4))[0]
        k = bisect.bisect_right(starts, ret) - 1
        caller = starts[k] if k >= 0 and ret != replay.SENTINEL else 0
        entries[addr] += 1
        edges[caller, addr] += 1
    m.mu.hook_add(UC_HOOK_BLOCK, block, begin=starts[0], end=starts[-1])
    for k in range(steps):
        control = controls.get(snap.step + k) if k else b""
        if control is None:
            return entries, edges, m, "no recorded control for step %d" % (snap.step + k)
        stop = rust_step.advance(m, snap, k, control)
        if stop:
            return entries, edges, m, "step %d: %s" % (snap.step + k, stop)
    return entries, edges, m, None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshots", nargs="+")
    ap.add_argument("--recording", required=True)
    ap.add_argument("--steps", type=int, required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--until", type=int, help="the last chain stops before this step")
    a = ap.parse_args()
    starts, o = sorted(worklist.starts()), rust_step.oracle()
    entries, edges, stops = collections.Counter(), collections.Counter(), []
    steps = [replay.Snapshot(p).step for p in a.snapshots] + [a.until]
    for k, path in enumerate(a.snapshots):
        n = a.steps if steps[k + 1] is None else min(a.steps, steps[k + 1] - steps[k])
        for _ in range(8):  # as rust_step.py: missing x87 results are fetched from cachy, then the run repeated
            o.known.used, o.known.modelled = 0, set()
            e, g, m, stop = chain(path, a.recording, n, o, starts)
            missing = set(m.fpu_missing) | o.known.modelled
            if not missing or not replay.REMOTE:
                break
            o.fetch(missing)
        if stop:
            stops.append("%s stopped: %s" % (os.path.basename(path), stop))
            print(stops[-1], flush=True)
        entries.update(e)
        edges.update(g)
        print("%s: %d steps, %d functions" % (path, n, len(e)), flush=True)
    with open(a.out, "w") as f:
        f.write("# harness/step_reached.py %s --recording %s --steps %d\n" % (
            " ".join(os.path.basename(p) for p in a.snapshots), os.path.basename(a.recording), a.steps))
        f.writelines("# %s\n" % line for line in stops)
        f.writelines("fn %08x %d\n" % (fn, n) for fn, n in sorted(entries.items()))
        f.writelines("edge %08x %08x %d\n" % (c, fn, n) for (c, fn), n in sorted(edges.items()))
    print("%s: %d functions, %d edges" % (a.out, len(entries), len(edges)))


if __name__ == "__main__":
    main()
