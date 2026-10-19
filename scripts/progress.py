#!/usr/bin/env python3
"""Gameplay progress scorecard (notes/rust-plan.md, notes/verification.md "Progress tracking").

  progress.py report [reccmp.txt]           print the scorecard
  progress.py mark STATUS EVIDENCE ADDR...  set the status of gameplay functions
  progress.py called DRIVE CALLS.LOG        record which functions a drive reaches
  progress.py step DRIVE REACHED.TXT...     record which functions the drive's physics step reaches
                                            (harness/step_reached.py; the files' entries are summed)
  progress.py steplist DRIVE [reccmp.txt]   list those functions with their port status

reccmp.txt defaults to data/rbr.recomp.csv (the C++ reconstruction's last report). Statuses,
best first: exact (from reccmp, computed), verified, reviewed, fixed, bad, untouched (default)."""
import csv, functools, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROG = os.path.join(ROOT, "notes", "progress")
FUNCS = os.path.join(PROG, "gameplay-functions.csv")
CALLED = os.path.join(PROG, "called.csv")
DRIVES = os.path.join(PROG, "drives.csv")
FORMATS = os.path.join(PROG, "formats.csv")
PORTS = os.path.join(ROOT, "rust", "rbr-physics", "ports.csv")
BOUND = os.path.join(ROOT, "rust", "rbr-hybrid", "bound.csv")  # the hybrid step's gate (harness/rust_step.py)
STEP = os.path.join(PROG, "step-reached.csv")
LIBRARY = os.path.join(ROOT, "data", "rbr.csv")
# The C++ reconstruction (the rbr-recomp repository), frozen: each function's source file in src/rbr and its last
# reccmp score and name.
RECOMP = os.path.join(ROOT, "data", "rbr.recomp.csv")
STATUSES = ["exact", "verified", "reviewed", "fixed", "bad", "untouched"]

# Gameplay subsystems outside the physics files, by address (notes/physics.md "Exe map").
RANGES = [
    ("loop", 0x47e1e0, 0x483000), ("loop", 0x449440, 0x449800),
    ("input", 0x4c6d00, 0x4cc000), ("input", 0x48bb70, 0x48e000),
    ("track", 0x49ff10, 0x4a50c0), ("track", 0x467630, 0x46f330), ("track", 0x4a8ee0, 0x4aa1b0),
    ("gameflow", 0x566080, 0x569000), ("gameflow", 0x4b64d0, 0x4b74f0),
    ("replay", 0x499b60, 0x49b000), ("replay", 0x609c90, 0x621330),
]
PHYSICS_FILE = re.compile(r"^RBR_E(3[3-9]|4[0-9]|5[01])_")
NOT_GAMEPLAY = re.compile(r"_Prof\.")
# Physics subsystems for the per-subsystem Rust gate (first match wins; notes/physics/*.md).
PHYSICS_SUBSYSTEMS = [
    ("collision", r"E33_|E34_Wall|E36_Collide|E37_|E38_"),
    ("car-body", r"E34_|E35_|E36_|E39_Damage"),
    ("engine-driveline", r"E39_|E40_|E41_|E42_|E43_Engine"),
    ("entities", r"E43_|E44_"),
    ("physics-core", r"E45_|E46_"),
    ("rigid-body", r"E47_"),
    ("spring-damper", r"E48_NTweak|E48_SpringDamper"),
    ("vcu", r"E49_VCU|E50_Vcu|E50_Sim"),
    ("tyre", r"E48_|E49_"),
    ("wheel", r"E50_|E51_"),
]  # the profiler; E48_TyreMenu also holds CTyre code


@functools.lru_cache(maxsize=None)
def recomp():
    """The rows of RECOMP: address, file, score, name."""
    with open(RECOMP, newline="") as f:
        return [(int(r["address"], 16), r) for r in csv.DictReader(f, delimiter="|")]


@functools.lru_cache(maxsize=None)
def annotations():
    """addr -> source file in src/rbr of the C++ reconstruction (RECOMP)."""
    return {a: r["file"] for a, r in recomp() if r["file"]}


def subsystem(addr, f):
    if NOT_GAMEPLAY.search(f):
        return None
    if PHYSICS_FILE.match(f):
        return "physics"
    for name, lo, hi in RANGES:
        if lo <= addr < hi:
            return name
    return None


def gate_excluded(addr, sc):
    """Not gate items: SampleBig's interior labels and the TyreMenu UI (E48_TyreMenu functions
    outside NPhysics, and its NTweak menu builders); same rule as the lead's gate sets."""
    name = sc.get(addr, (0, ""))[1]
    return addr in (0x50b580, 0x50b690) or ("TyreMenu" in annotations().get(addr, "") and (
        not name.startswith("NPhysics::") or "NTweak" in name))


def physics_subsystem(f):
    return next((name for name, pat in PHYSICS_SUBSYSTEMS if re.match(r"RBR_(%s)" % pat, f)), None)


def scores(path=None):
    """addr -> (score, name) from a reccmp text report, or RECOMP's by default."""
    if path is None or path == RECOMP:
        return {a: (float(r["score"]), r["name"]) for a, r in recomp() if r["score"]}
    out = {}
    pat = re.compile(r"^\s*(.*) \((0x[0-9a-f]+)\) is ([0-9.]+)%")
    for line in open(path, encoding="utf-8", errors="replace"):
        m = pat.match(re.sub(r"\x1b\[[0-9;]*m", "", line))
        if m:
            out[int(m.group(2), 16)] = (float(m.group(3)), m.group(1))
    return out


def read_csv(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def write_csv(path, rows, fields):
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, fields, lineterminator="\n")
        w.writeheader()
        w.writerows(rows)


def gameplay():
    return {a: s for a, f in annotations().items() if (s := subsystem(a, f))}


def mark(status, evidence, addrs):
    if status not in STATUSES[1:]:
        sys.exit("status must be one of " + ", ".join(STATUSES[1:]))
    known = gameplay()
    rows = {int(r["addr"], 16): r for r in read_csv(FUNCS)}
    for a in addrs:
        a = int(a, 16)
        if a not in known:
            print("skip %#x: not a gameplay function" % a)
            continue
        rows[a] = {"addr": "%#x" % a, "status": status, "evidence": evidence}
    write_csv(FUNCS, [rows[a] for a in sorted(rows)], ["addr", "status", "evidence"])


def called(drive, log):
    rows = {(r["addr"], r["drive"]) for r in read_csv(CALLED)}
    for line in open(log):
        p = line.split()
        if len(p) >= 2 and p[1].lstrip("-").isdigit() and int(p[1]) > 0:
            rows.add(("%#x" % int(p[0], 16), drive))
    write_csv(CALLED, [{"addr": a, "drive": d} for a, d in sorted(rows)], ["addr", "drive"])


def step(drive, paths):
    rows = [r for r in read_csv(STEP) if r["drive"] != drive]
    entries = {}
    for path in paths:
        for line in open(path):
            p = line.split()
            if p and p[0] == "fn":
                entries[int(p[1], 16)] = entries.get(int(p[1], 16), 0) + int(p[2])
    if not entries:
        sys.exit("no 'fn' lines in %s (harness/step_reached.py output); %s's rows are unchanged" % (" ".join(paths), drive))
    rows += [{"addr": "%#x" % a, "drive": drive, "entries": n} for a, n in entries.items()]
    write_csv(STEP, sorted(rows, key=lambda r: (r["drive"], int(r["addr"], 16))), ["addr", "drive", "entries"])


def step_status(sc):
    """addr -> port status of a function the step reaches: checked, unit or partial (ports.csv), library (data/rbr.csv:
    the pure step uses Rust's own), or unported, split by whether the C++ is exact."""
    ports = {int(r["addr"], 16): r["status"] for r in read_csv(PORTS)}
    with open(LIBRARY, newline="") as f:
        library = {int(r["address"], 16) for r in csv.DictReader(f, delimiter="|") if r["type"] == "library"}

    def status(a):
        if a in ports:
            return ports[a]
        if a in library:
            return "library"
        return "unported, C++ exact" if sc.get(a, (0,))[0] >= 100 else "unported"
    return status


STEP_STATUSES = ["checked", "unit", "partial", "unported, C++ exact", "unported", "library"]


def step_report(sc):
    """Pure-step coverage: every function a drive's physics step reaches (the C++-exact ones included) and its port."""
    status, files = step_status(sc), annotations()
    rows = read_csv(STEP)
    print("\n## Pure step coverage (notes/progress/step-reached.csv)\n")
    for drive in sorted({r["drive"] for r in rows}):
        hit = [int(r["addr"], 16) for r in rows if r["drive"] == drive]
        names = [name for name, _ in PHYSICS_SUBSYSTEMS] + ["outside physics"]
        print("| Subsystem (%s) | reached | %s |" % (drive, " | ".join(STEP_STATUSES)))
        print("|---" * (len(STEP_STATUSES) + 2) + "|")
        tot = {k: 0 for k in STEP_STATUSES}
        for n in names:
            mine = [status(a) for a in hit if (physics_subsystem(files.get(a, "")) or "outside physics") == n]
            if mine:
                for k in tot:
                    tot[k] += mine.count(k)
                print("| %s | %d | %s |" % (n, len(mine), " | ".join(str(mine.count(k)) for k in STEP_STATUSES)))
        print("| **all** | %d | %s |" % (len(hit), " | ".join(str(tot[k]) for k in STEP_STATUSES)))
        code = len(hit) - tot["library"]
        print("\nThe physics step of %s reaches %d functions, %d of them library code. Ported and checked against "
              "records: **%d of %d** (%.1f%%); %d more unit-tested or partial." % (
                  drive, len(hit), tot["library"], tot["checked"], code, 100 * tot["checked"] / max(code, 1),
                  tot["unit"] + tot["partial"]))


def step_list(drive, sc):
    status, files = step_status(sc), annotations()
    rows = [r for r in read_csv(STEP) if r["drive"] == drive]
    if not rows:
        sys.exit("no step-reached rows for %s" % drive)
    print("| Address | Name | Subsystem | Entries | Port |\n|---|---|---|---|---|")
    for r in rows:
        a = int(r["addr"], 16)
        print("| %#x | %s | %s | %s | %s |" % (a, sc.get(a, (0, "?"))[1].replace("|", "\\|"),
              physics_subsystem(files.get(a, "")) or "outside physics", r["entries"], status(a)))


def rust_report(sc):
    """The Rust port: reached non-exact physics functions per drive, and which of them ports.csv has."""
    ports = {int(r["addr"], 16): r["status"] for r in read_csv(PORTS)}
    bound = {int(r["addr"], 16): r["status"] for r in read_csv(BOUND)}
    files = annotations()
    g = gameplay()
    called_rows = read_csv(CALLED)
    print("\n## Rust port (rust/rbr-physics/ports.csv)\n")
    for drive in sorted({r["drive"] for r in called_rows}):
        hit = {int(r["addr"], 16) for r in called_rows if r["drive"] == drive}
        todo = sorted(a for a in hit if g.get(a) == "physics" and sc.get(a, (0,))[0] < 100 and not gate_excluded(a, sc))
        names = [name for name, _ in PHYSICS_SUBSYSTEMS] + ["other"]
        rows = {n: [a for a in todo if (physics_subsystem(files[a]) or "other") == n] for n in names}
        print("| Subsystem (%s) | reached, non-exact | ported | checked | unit | partial | unported | bound | "
              "bound, identical |\n|---|---|---|---|---|---|---|---|---|" % drive)
        tot = {"ported": 0, "checked": 0, "unit": 0, "partial": 0, "unported": 0, "bound": 0, "identical": 0}
        for n in names:
            mine = rows[n]
            if not mine:
                continue
            st = [ports.get(a) for a in mine]
            c = {k: st.count(k) for k in ("checked", "unit", "partial")}
            c["bound"] = sum(a in bound for a in mine)
            c["identical"] = sum(bound.get(a) == "identical" for a in mine)
            un = sum(s is None for s in st)
            for k in c:
                tot[k] += c[k]
            tot["ported"] += len(mine) - un
            tot["unported"] += un
            print("| %s | %d | %d | %d | %d | %d | %d | %d | %d |" % (n, len(mine), len(mine) - un, c["checked"],
                  c["unit"], c["partial"], un, c["bound"], c["identical"]))
        print("| **all** | %d | %d | %d | %d | %d | %d | %d | %d |" % (sum(map(len, rows.values())), tot["ported"],
              tot["checked"], tot["unit"], tot["partial"], tot["unported"], tot["bound"], tot["identical"]))
        print("\nRust physics reached by %s: **%d of %d ported** (non-exact); %d checked against records, "
              "%d unit only, %d partial." % (drive, tot["ported"], tot["ported"] + tot["unported"],
              tot["checked"], tot["unit"], tot["partial"]))
        print("\nBound in the hybrid step (rust/rbr-hybrid/bound.csv): %d, of which %d identical to the original "
              "(harness/rust_step.py)." % (tot["bound"], tot["identical"]))
        print("\nReached and unported (the integration worklist):\n")
        for n in names:
            miss = [a for a in rows[n] if a not in ports]
            if miss:
                print("- %s (%d): %s" % (n, len(miss), ", ".join("%#x %s" % (a, sc.get(a, (0, "?"))[1]) for a in miss)))


def report(reccmp):
    sc = scores(reccmp)
    status = {int(r["addr"], 16): r["status"] for r in read_csv(FUNCS)}
    reached = {int(r["addr"], 16) for r in read_csv(CALLED)}
    table = {}
    for a, sub in gameplay().items():
        st = "exact" if sc.get(a, (0,))[0] >= 100 else status.get(a, "untouched")
        row = table.setdefault(sub, {s: 0 for s in STATUSES} | {"reached": 0, "total": 0})
        row[st] += 1
        row["total"] += 1
        row["reached"] += a in reached
    order = ["physics", "loop", "input", "track", "gameflow", "replay"]
    print("## Gameplay functions\n")
    print("| Subsystem | " + " | ".join(STATUSES) + " | total | reached by a drive |")
    print("|---" * (len(STATUSES) + 3) + "|")
    tot = {k: 0 for k in STATUSES + ["reached", "total"]}
    for sub in order:
        r = table.get(sub)
        if not r:
            continue
        for k in tot:
            tot[k] += r[k]
        print("| %s | %s | %d | %d (%d%%) |" % (sub, " | ".join(str(r[s]) for s in STATUSES),
              r["total"], r["reached"], 100 * r["reached"] // max(r["total"], 1)))
    print("| **all** | %s | %d | %d (%d%%) |" % (" | ".join(str(tot[s]) for s in STATUSES),
          tot["total"], tot["reached"], 100 * tot["reached"] // max(tot["total"], 1)))
    done = tot["exact"] + tot["verified"]
    phys = table.get("physics", {s: 0 for s in STATUSES} | {"total": 0})
    print("\n**Done: %.1f%%** of gameplay functions (exact or verified; %.1f%% counting reviewed). "
          "Physics: %.1f%%." % (100 * done / max(tot["total"], 1),
          100 * (done + tot["reviewed"]) / max(tot["total"], 1),
          100 * (phys["exact"] + phys["verified"]) / max(phys["total"], 1)))
    # The Rust gate: every non-exact physics function a drive calls must be verified.
    g = gameplay()
    for drive in sorted({r["drive"] for r in read_csv(CALLED)}):
        hit = [int(r["addr"], 16) for r in read_csv(CALLED) if r["drive"] == drive]
        todo = [a for a in hit if g.get(a) == "physics" and sc.get(a, (0,))[0] < 100 and not gate_excluded(a, sc)]
        ok = sum(status.get(a) == "verified" for a in todo)
        print("Physics reached by %s: **%d of %d verified** (non-exact)." % (drive, ok, len(todo)))
        files = annotations()
        print("\n| Subsystem (%s) | reached, non-exact | verified | open | gate |\n|---|---|---|---|---|" % drive)
        for name, _ in PHYSICS_SUBSYSTEMS:
            mine = [a for a in todo if physics_subsystem(files[a]) == name]
            v = sum(status.get(a) == "verified" for a in mine)
            print("| %s | %d | %d | %d | %s |" % (name, len(mine), v, len(mine) - v, "ready" if v == len(mine) else ""))
    print("\n## End-to-end physics (all physics functions transplanted)\n")
    drives = read_csv(DRIVES)
    if drives:
        print("| Drive | Run | Date | Identical steps | Note |\n|---|---|---|---|---|")
        for d in drives:
            print("| %s | %s | %s | %s of %s | %s |" % (d["drive"], d["run"], d["date"],
                  d["identical"], d["steps"], d["note"]))
    else:
        print("(no run yet)")
    print("\n## Formats\n")
    print("| Format | Status | Retail files | Note |\n|---|---|---|---|")
    for f in read_csv(FORMATS):
        print("| %s | %s | %s | %s |" % (f["format"], f["status"], f["retail"], f["note"]))
    rust_report(sc)
    step_report(sc)


def main():
    a = sys.argv[1:]
    if not a or a[0] == "report":
        report(a[1] if len(a) > 1 else RECOMP)
    elif a[0] == "mark" and len(a) >= 4:
        mark(a[1], a[2], a[3:])
    elif a[0] == "called" and len(a) == 3:
        called(a[1], a[2])
    elif a[0] == "step" and len(a) >= 3:
        step(a[1], a[2:])
    elif a[0] == "steplist" and len(a) in (2, 3):
        step_list(a[1], scores(a[2] if len(a) > 2 else RECOMP))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
