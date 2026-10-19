"""The integration worklist: the original physics functions the hybrid step still runs in the emulator, with every
rbr-hybrid binding installed (harness/rust_step.py), and what each one waits for.

Usage: .venv/bin/python -I -B harness/worklist.py SNAPSHOT... [--reccmp RECCMP.txt] [--out WORKLIST.md]

  Runs one StepForward per snapshot with all bindings and counts the entries of the original's functions
  (data/rbr.functions.csv) the emulator runs. A function is listed under its physics subsystem
  (scripts/progress.py's PHYSICS_SUBSYSTEMS, from its source file), in the binding order of the integration,
  with its blocker: not ported (crates/rbr-physics/ports.csv), ported but not
  bound yet (the integration's), or bound and declined (the binding's reason). Library code (data/rbr.csv) and
  the code outside the physics files are counted, not listed: they leave when their physics callers are bound.
  Names come from a reccmp report (scripts/progress.py's default, or --reccmp).
"""
import argparse
import collections
import csv
import os
import sys

from unicorn import UC_HOOK_CODE

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
sys.path.append(os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts"))
import progress  # noqa: E402
import replay  # noqa: E402
import rust_step  # noqa: E402

ORDER = ["vcu", "spring-damper", "tyre", "wheel", "engine-driveline", "rigid-body", "entities", "car-body",
         "physics-core", "collision"]


def starts():
    with open(os.path.join(replay.REPO, "data", "rbr.functions.csv")) as f:
        return {int(line.split("|")[0], 16) for line in f.read().split("\n")[1:] if line}


def library():
    with open(os.path.join(replay.REPO, "data", "rbr.csv")) as f:
        return {int(r["address"], 16) for r in csv.DictReader(f, delimiter="|") if r["type"] == "library"}


def reached(path, o, hybrid, entries):
    """{function: entries} of one hybrid step, and the bindings' declines {addr: {reason: count}}."""
    m = rust_step.machine(replay.Snapshot(path), o, hybrid)
    count = collections.Counter()

    def hook(mu, addr, size, _):
        if addr in entries:
            count[addr] += 1
    m.mu.hook_add(UC_HOOK_CODE, hook, begin=min(entries), end=max(entries))
    stop = rust_step.finish(m)
    if stop and not m.fpu_missing:
        sys.exit("%s: %s" % (path, stop))
    return count, m.declined


def listing(total, seen, declined, hybrid, names, by_entries=False):
    """The worklist's tables: one per physics subsystem in ORDER, a row per function the emulator ran (total
    {addr: entries}, seen {addr: snapshots or chains}) with its blocker, by address or by_entries; and the closing
    line on what is not listed. Returns (lines, functions listed, their entries)."""
    files = progress.annotations()
    ports = {int(r["addr"], 16): r["status"] for r in progress.read_csv(progress.PORTS)}
    lib = library()
    rows = collections.defaultdict(list)
    library_calls = outside = 0
    for addr in sorted(total):
        sub = progress.physics_subsystem(files.get(addr, ""))
        if addr in hybrid.bound and addr not in declined:
            continue  # the entry hook saw the binding's address before it ran
        if addr in lib:
            library_calls += 1
        elif sub is None:
            outside += 1
        elif addr in declined:
            rows[sub].append((addr, "bound, declined: %s" % "; ".join(sorted(declined[addr]))))
        elif addr in ports:
            rows[sub].append((addr, "ported (%s), not bound yet" % ports[addr]))
        else:
            rows[sub].append((addr, "not ported"))
    out = []
    for sub in ORDER + sorted(set(rows) - set(ORDER)):
        if not rows[sub]:
            continue
        if by_entries:
            rows[sub].sort(key=lambda row: -total[row[0]])
        out += ["## %s (%d)" % (sub, len(rows[sub])), "", "| Address | Name | Calls | Snapshots | Waits for |",
                "|---|---|---|---|---|"]
        out += ["| %#x | %s | %d | %d | %s |" % (addr, names.get(addr, (0, "?"))[1].replace("|", "\\|"), total[addr],
                                               seen[addr], why) for addr, why in rows[sub]]
        out.append("")
    out.append("Not listed: %d library functions (data/rbr.csv) and %d functions outside the physics files that the "
               "emulated physics calls." % (library_calls, outside))
    return out, sum(len(r) for r in rows.values()), sum(total[addr] for r in rows.values() for addr, _ in r)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshots", nargs="+")
    ap.add_argument("--reccmp", default=progress.RECOMP)
    ap.add_argument("--out")
    a = ap.parse_args()
    names = progress.scores(a.reccmp) if os.path.exists(a.reccmp) else {}
    hybrid, o, entries = rust_step.Hybrid(), rust_step.oracle(), starts()
    total, seen, declined = collections.Counter(), collections.Counter(), {}
    for path in a.snapshots:
        count, why = reached(path, o, hybrid, entries)
        total.update(count)
        seen.update(count.keys())
        for addr, d in why.items():
            declined.setdefault(addr, set()).update(d)
    out = ["# Integration worklist", "",
           "The original physics functions a hybrid step still runs in the emulator with every rbr-hybrid binding "
           "installed, by subsystem in the binding order (`harness/worklist.py` on %d snapshots of %s). Calls are "
           "per step, summed over the snapshots. Porting a function here moves its subtree into Rust once its callers "
           "are bound; a function whose callers are all bound already is a leaf to port next." % (
               len(a.snapshots), ", ".join(sorted({os.path.basename(p).split(".snap")[0] for p in a.snapshots}))), ""]
    out += listing(total, seen, declined, hybrid, names)[0]
    text = "\n".join(out) + "\n"
    if a.out:
        with open(a.out, "w") as f:
            f.write(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
