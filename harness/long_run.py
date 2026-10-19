"""The long hybrid run, the main check of the Rust step (lead/reports/pure-step.md, 2a): run B (every rbr-hybrid
binding) against run A (the original) and run A against the drive's recording, over a drive's whole snapshot chain;
then the worklist, the original functions run B still runs in the emulator over the chain, ranked by entries.

Usage: .venv/bin/python -I -B harness/long_run.py --drive tanner-corolla [--from STEP] [--to STEP] [-j N]
                                                   [--out REPORT.md] [--reccmp RECCMP.txt]
  Needs `cargo build --release -p rbr-hybrid` in rust/, as rust_step.py.

  Each segment runs from its snapshot to the next one (the last to the end of the recording) as rust_step.py --steps,
  with its rules: run B against run A after every step under both stack fills, every byte of the writable memory and
  the stub heap, its read windows and exclusions (classify); run A against the recording's car and wheels after every
  step. A segment keeps going after a divergence and reports the first: for run B, the step where the first of the
  bytes that fail began to differ; for run A, the first frame. It stops only where the emulator does (a fault, a call
  with no stub, x87 results missing from the cache). x87 results as rust_step.py: with RBR_REMOTE the missing and
  modelled ones are fetched from cachy and the segment repeated.
  --from and --to keep the segments that overlap steps [FROM, TO); a segment always starts at its snapshot, TO cuts
  the last one. -j runs that many segments at once, niced, the longest first. The report (stdout, and --out) holds
  each segment's rust_step.py lines, a table of verdicts, first divergences and runtimes, and the worklist
  (harness/worklist.py's tables, functions ranked by entries over the chain; "Snapshots" counts the segments that
  ran them). Each segment prints a progress line on stderr every PROGRESS steps.

Drives (DRIVES): the recording and the snapshots of the same game run, from RBR_RECORDINGS (default: recordings/ next
to the worktree) and RBR_SNAPS (default /tmp/rbr-snaps).
"""
import argparse
import collections
import contextlib
import glob
import io
import multiprocessing
import os
import re
import sys
import time

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
sys.path.append(os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts"))
import progress  # noqa: E402
import replay  # noqa: E402
import rust_step  # noqa: E402
import worklist  # noqa: E402

RECORDINGS = os.environ.get("RBR_RECORDINGS", os.path.join(os.path.dirname(replay.REPO), "recordings"))
SNAPS = os.environ.get("RBR_SNAPS", "/tmp/rbr-snaps")
PROGRESS = 1000  # a line on stderr every this many steps of a segment
DRIVES = {"tanner-corolla": ("drive-tanner-corolla.bin", r"vcu-cov\.snap-\d+\.bin"),
          "tanner-damage": ("drive-tanner-damage.bin", r"vcu-tanner-damage\.snap-\d+\.bin"),
          "tarmac-lancer": ("drive-tarmac-lancer.bin", r"vcu-tarmac-lancer\.snap-\d+\.bin"),
          "arctic-snow": ("drive-arctic-snow.bin", r"vcu-arctic-snow\.snap-\d+\.bin")}


def segments(drive, lo, hi):
    """[(snapshot, first step, steps)] of the drive's chain that overlap [lo, hi)."""
    recording, pattern = DRIVES[drive]
    snaps = sorted((replay.Snapshot(p).step, p) for p in glob.glob(os.path.join(SNAPS, "*"))
                   if re.fullmatch(pattern, os.path.basename(p)))
    end = max(replay.recording(os.path.join(RECORDINGS, recording)).frames) + 1
    out = []
    for k, (step, path) in enumerate(snaps):
        stop = min(snaps[k + 1][0] if k + 1 < len(snaps) else end, hi)
        if step < stop and stop > lo:
            out.append((path, step, stop - step))
    return out


_hybrid = None


def segment(job):
    """One segment's chain: (its rust_step.py lines, chain()'s result, run B's entries, seconds)."""
    global _hybrid
    path, recording, steps = job
    _hybrid = _hybrid or rust_step.Hybrid()
    o = rust_step.oracle()
    a = argparse.Namespace(recording=recording, steps=steps, only=None, perturb=None, full_fill=False,
                           excluded=False, progress=PROGRESS)
    t = time.time()
    for _ in range(8):  # as rust_step.py: missing x87 results are fetched from cachy, then the segment repeated
        o.known.used, o.known.modelled = 0, set()
        count, out = collections.Counter(), io.StringIO()
        with contextlib.redirect_stdout(out):
            res = rust_step.chain(path, a, _hybrid, o, count)
        missing = res["fpu_missing"] | o.known.modelled
        if not missing or not replay.REMOTE:
            break
        o.fetch(missing)
    return out.getvalue(), res, count, time.time() - t


def nice():
    os.nice(10)


def verdicts(results):
    """The table of segment verdicts."""
    rows = ["| Segment | Steps | Run B vs run A | First run B divergence | Run A vs recording | Runtime | Steps/s |",
            "|---|---|---|---|---|---|---|"]
    for (path, start, steps), (_, res, _, secs) in results:
        b = "equal" if not res["failures"] else "%d bytes" % res["failures"]
        if res["halt"]:
            b = ("%s up to the stop; " % b if res["compared"] else "") + "stopped at step %d (%s)" % res["halt"]
        first = "step %d (%#x, read by %#x)" % res["first_b"] if res["first_b"] else "-"
        a = "bit-exact" if res["a_bad"] is None else "diverges at frame %d (%d words)" % res["a_bad"]
        if res["unverified"]:
            a += "; %d steps with modelled x87" % res["unverified"]
        rows.append("| %d-%d | %d | %s | %s | %s | %.0f s | %.1f |" % (
            start, start + steps - 1, res["steps"], b, first, a, secs, res["steps"] / secs))
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--drive", required=True, choices=sorted(DRIVES))
    ap.add_argument("--from", dest="lo", type=int, default=0, metavar="STEP")
    ap.add_argument("--to", dest="hi", type=int, default=1 << 31, metavar="STEP")
    ap.add_argument("-j", type=int, default=1, help="segments at once (niced)")
    ap.add_argument("--out", help="also write the report here")
    ap.add_argument("--reccmp", default=progress.RECOMP,
                    help="function names (a reccmp text report; default the C++ reconstruction's)")
    a = ap.parse_args()
    recording = os.path.join(RECORDINGS, DRIVES[a.drive][0])
    jobs = segments(a.drive, a.lo, a.hi)
    if not jobs:
        sys.exit("long_run.py: no segment of %s overlaps [%d, %d)" % (a.drive, a.lo, a.hi))
    hybrid, t = rust_step.Hybrid(), time.time()  # checks the library before anything runs
    work = [(p, recording, n) for p, _, n in jobs]
    if a.j > 1:
        order = sorted(range(len(work)), key=lambda k: -work[k][2])
        with multiprocessing.get_context("spawn").Pool(a.j, initializer=nice) as pool:
            done = dict(zip(order, pool.map(segment, [work[k] for k in order], chunksize=1)))
        results = [done[k] for k in range(len(work))]
    else:
        results = []
        for job in work:
            results.append(segment(job))
            print(results[-1][0], end="", flush=True)
    wall = time.time() - t
    total, seen, declined = collections.Counter(), collections.Counter(), {}
    for _, _, count, _ in results:
        total.update(count)
        seen.update(count.keys())
    for _, res, _, _ in results:
        for addr, d in res["declined"].items():
            declined.setdefault(addr, set()).update(d)
    names = progress.scores(a.reccmp) if os.path.exists(a.reccmp) else {}
    tables, listed, entries = worklist.listing(total, seen, declined, hybrid, names, by_entries=True)
    steps = sum(res["steps"] for _, res, _, _ in results)
    ok = all(res["ok"] for _, res, _, _ in results)
    out = ["# Long run: %s, steps %d-%d" % (a.drive, jobs[0][1], jobs[-1][1] + jobs[-1][2] - 1), "",
           "`harness/long_run.py --drive %s%s%s%s`: %d segments, %d steps in %.0f s (%.1f steps/s, -j %d); %s." % (
               a.drive, " --from %d" % a.lo if a.lo else "", " --to %d" % a.hi if a.hi < 1 << 31 else "",
               " -j %d" % a.j if a.j > 1 else "", len(jobs), steps, wall, steps / wall, a.j,
               "every segment passes" if ok else "NOT every segment passes"), ""]
    out += [line for line in verdicts(zip(jobs, results))] + [""]
    out += ["## Worklist", "",
            "The original physics functions run B (every binding) still runs in the emulator over the chain: "
            "**%d functions, %d entries**. By subsystem, ranked by entries over the chain." % (listed, entries), ""]
    out += tables
    if a.j > 1:
        print("".join(text for text, _, _, _ in results), end="")
    report = "\n".join(out) + "\n"
    print("\n" + report, end="")
    if a.out:
        with open(a.out, "w") as f:
            f.write("```\n%s```\n\n%s" % ("".join(text for text, _, _, _ in results), report))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
