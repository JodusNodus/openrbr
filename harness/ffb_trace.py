"""ffb_trace.py: the wheel's force-feedback device traces (harness/ffb_trace.c, run by harness/ffb_trace.sh).

  ffb_trace.py summary DIR [SESSION.bin|FILE] what a trace sent: gain and autocentre writes, effects, update rate,
                                              levels; with the RSF run's session (or the probe's magnitudes FILE),
                                              every SetParameters magnitude against the level that followed it and
                                              Wine's level for it
  ffb_trace.py magnitudes SESSION.bin [--from S] [--seconds N]
                                              `seconds magnitude` lines of the constant force's SetParameters (EFF1)
                                              from S seconds after the first non-zero one minus 1 s, for ffb_play
  ffb_trace.py ours SESSION.bin EXT [--last STEP]
                                              `seconds magnitude` lines for ffb_play of ours: our magnitude of each
                                              driving step (rsf_compare's trace, EXT = ours.bin.ext) at its STP1 time,
                                              on `magnitudes`' time base, up to STEP (the physics parting)
  ffb_trace.py steps SESSION.bin EXT RSF_DIR OURS_DIR [--raw-offset NS] [--last STEP]
                                              RSF's level at the wheel for each set against Proton's level of the
                                              magnitudes rsf_compare's matching allows it (ours at its step or the
                                              step before its frame, the gate's 0, ...; per class, misses listed),
                                              the device chain (the level of the set's own magnitude), ours
                                              (`ffb_trace.sh ours` of the `ours` lines) against the line due, and the
                                              delay from step end (RSF: STP1; ours: the line's time) to the wheel.
                                              NS: CLOCK_MONOTONIC - CLOCK_MONOTONIC_RAW at the run (Wine's QPC is
                                              RAW / 100, the trace MONOTONIC; RSF's delays only; default
                                              RSF_DIR/raw_offset, written by ffb_trace.sh rsf, else left out)
  ffb_trace.py compare RSF_DIR OURS_DIR... --out PNG [--seconds N]
                                              the level over time of each trace (aligned at their first non-zero
                                              level), sampled every 1 ms: equal share, largest difference; a PNG
                                              of RSF (blue) and each of ours (orange) above their differences

A trace line (ffb_trace.c): `ns tid path op ret ...`; op sff (EVIOCSFF: id type dir length delay level, envelope),
rmff (EVIOCRMFF: id), ev (EV_FF write: code value; code 96 FF_GAIN, 97 FF_AUTOCENTER, else the effect id played).
"""
import argparse
import glob
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsf_compare as rc  # noqa: E402
import session as rss  # noqa: E402
from rsf_compare import BLUE, ORANGE, plot  # noqa: E402

FF_GAIN, FF_AUTOCENTER, FF_CONSTANT = 96, 97, 0x52


def read(directory):
    """The trace's events in time order: (ns, op, fields as ints), the device's path dropped."""
    events = []
    for path in glob.glob(os.path.join(directory, "ffb-*.txt")):
        with open(path) as f:
            for line in f:
                w = line.split()
                if len(w) >= 5 and w[3] in ("sff", "rmff", "ev"):
                    events.append((int(w[0]), w[3], [int(v) for v in w[4:]]))
    events.sort(key=lambda e: e[0])
    return events


def levels(events):
    """(ns, level) of each EVIOCSFF of a constant force."""
    out = [(t, f[6]) for t, op, f in events if op == "sff" and f[2] == FF_CONSTANT]
    return np.array([t for t, _ in out], dtype=np.int64), np.array([v for _, v in out], dtype=np.int64)


def wine_level(m):
    """The evdev level Proton gives a DirectInput constant force's magnitude: dinput's scale_to_logical_value
    (joystick_hid.c) onto winebus's PID magnitude (logical -32768..32767 for physical -10000..10000, hid.c), times
    the effect gain (100 %) in bus_sdl.c's sdl_device_physical_effect_update."""
    m = np.clip(np.asarray(m, dtype=np.int64), -10000, 10000)
    return -32768 + (m + 10000) * 65535 // 20000


def constant_sets(s):
    """(qpc, magnitude) arrays of the constant force's SetParameters, as rsf_compare.recorded_ffb takes them."""
    sets, _ = rc.recorded_ffb(s)
    return np.array([q for q, _ in sets], dtype=np.int64), np.array([m for _, m in sets], dtype=np.int64)


def summary(directory, session_path=None):
    events = read(directory)
    if not events:
        return "%s: no events" % directory
    lines = ["%s: %d events over %.2f s" % (directory, len(events), (events[-1][0] - events[0][0]) / 1e9)]
    for t, op, f in events:
        if op == "ev" and f[1] in (FF_GAIN, FF_AUTOCENTER):
            lines.append("  %.3f s %s %d (ret %d)" % ((t - events[0][0]) / 1e9,
                                                     "FF_GAIN" if f[1] == FF_GAIN else "FF_AUTOCENTER", f[2], f[0]))
        elif op == "rmff":
            lines.append("  %.3f s EVIOCRMFF %d (ret %d)" % ((t - events[0][0]) / 1e9, f[1], f[0]))
    sff = [f for _, op, f in events if op == "sff"]
    plays = [f for _, op, f in events if op == "ev" and f[1] not in (FF_GAIN, FF_AUTOCENTER)]
    shapes = sorted({(f[1], f[2], f[3], f[4], f[5], tuple(f[7:])) for f in sff})
    lines.append("  EVIOCSFF %d (failed %d), plays %d (values %s), effects (id type dir length delay envelope) %s"
                 % (len(sff), sum(f[0] < 0 for f in sff), len(plays), sorted({f[2] for f in plays}), shapes))
    t, v = levels(events)
    nz = np.nonzero(v)[0]
    if len(nz):
        a = nz[0]
        d = np.diff(t[a:]) / 1e6
        lines.append("  from the first non-zero level: %d EVIOCSFF, interval ms p10/p50/p90 %s, %d level changes, "
                     "levels %d to %d" % (len(t) - a, np.round(np.percentile(d, [10, 50, 90]), 3) if len(d) else "-",
                                          int(np.count_nonzero(np.diff(v[a:]))), v[a:].min(), v[a:].max()))
    if session_path:
        if session_path.endswith(".bin"):
            m = constant_sets(rss.read(session_path))[1]
        else:
            m = np.loadtxt(session_path, ndmin=2)[:, 1].astype(np.int64)
        lines += check_calls(events, m)
    return "\n".join(lines)


def check_calls(events, m):
    """Each SetParameters magnitude m (in order) against the plays: NGP's SetParameters(DIEP_START) is one play each
    in Wine (Start(1)), so the k-th play follows the k-th call; the last EVIOCSFF before that play holds its level."""
    seq = [(op, f) for _, op, f in events if op == "sff" or (op == "ev" and f[1] not in (FF_GAIN, FF_AUTOCENTER))]
    after, last, sffs_per = [], None, []
    n = 0
    for op, f in seq:
        if op == "sff":
            last = f[6]
            n += 1
        else:
            after.append(last)
            sffs_per.append(n)
            n = 0
    k = min(len(after), len(m))
    if not k:
        return ["  calls: %d SetParameters, %d plays: nothing to pair" % (len(m), len(after))]
    got, want = np.array(after[:k], dtype=np.int64), wine_level(m[:k])
    changed = np.r_[True, np.diff(m[:k]) != 0]
    per = np.array(sffs_per[:k])
    return ["  calls: %d SetParameters, %d plays; paired %d: level == Wine's level for %d (%.4f), m == 0 -> %s; "
            "EVIOCSFF per call: %s when the magnitude changed, %s when not" % (
                len(m), len(after), k, int((got == want).sum()), float((got == want).mean()),
                sorted(set(got[m[:k] == 0].tolist())), sorted(set(per[changed].tolist())),
                sorted(set(per[~changed].tolist())))]


def magnitudes(session_path, start, seconds):
    s = rss.read(session_path)
    q, m = constant_sets(s)
    nz = np.nonzero(m)[0]
    if not len(nz):
        raise SystemExit("%s: no non-zero magnitude" % session_path)
    t = (q - q[nz[0]]) / s.qpc_frequency
    t0 = start - 1.0
    keep = (t >= t0) & (t < t0 + seconds)
    return "".join("%.6f %d\n" % (a - t0, b) for a, b in zip(t[keep], m[keep]))


def our_steps(s, ext_path, last):
    """(seconds, magnitude, step) of each driving step (substeps 1) up to `last`: what our wheel is sent, from
    rsf_compare's `.ext` (the magnitude, 0 outside the racing gate), at its StepForward's time on `magnitudes`' base
    (1 s before RSF's first non-zero set)."""
    ext = rc.read_ext(ext_path)
    sent = np.where(ext["state"] == rc.DRIVING, ext["magnitude"], 0)
    m = dict(zip(ext["post"].tolist(), sent.tolist()))
    q, sets = constant_sets(s)
    q0 = q[np.nonzero(sets)[0][0]]
    return [((r["qpc"] - q0) / s.qpc_frequency + 1.0, m[r["post"]], r["post"]) for r in s.tagged("STP1")
            if r["substeps"] == 1 and r["post"] in m and r["post"] <= last]


def ours_lines(session_path, ext_path, last):
    return "0.000000 0\n" + "".join("%.6f %d\n" % (t, v) for t, v, _ in
                                     our_steps(rss.read(session_path), ext_path, last) if t >= 0)


def pct(d):
    return "p10/p50/p90/p99 %s ms, max %.2f" % ("/".join("%.2f" % v for v in np.percentile(d, [10, 50, 90, 99])),
                                                  max(d))


def steps(session_path, ext_path, rsf_dir, ours_dir, raw_offset, last):
    """RSF's wheel writes against ours per step, and ours (ffb_play) against its lines. RSF: the k-th play follows the
    k-th SetParameters (check_calls), so each write carries a set; its level must be Proton's level of one of the
    magnitudes rsf_compare's matching allows that set (`ffb_candidates`: ours at the set's step or the step before
    its frame, the racing gate's 0, ...), each counted by class; a write whose set falls past `last` or our run is
    not compared, and counted."""
    s = rss.read(session_path)
    if raw_offset is None and os.path.exists(os.path.join(rsf_dir, "raw_offset")):  # ffb_trace.sh rsf's, at the start
        raw_offset = float(open(os.path.join(rsf_dir, "raw_offset")).read())
    ext = rc.read_ext(ext_path)
    keep = ext["post"] <= last
    posts, state, ours = ext["post"][keep], ext["state"][keep], ext["magnitude"][keep]
    clock = rc.step_clock(s)
    q, sets = constant_sets(s)
    plays, level = [], None
    for t, op, f in read(rsf_dir):
        if op == "sff":
            level = f[6]
        elif op == "ev" and f[1] not in (FF_GAIN, FF_AUTOCENTER):
            plays.append((t, level))
    n = min(len(plays), len(q))
    first = next((r["qpc"] for r in s.tagged("STP1") if r["substeps"] != rc.SETTLE_SUBSTEPS), None)
    cls, rows, literal, window = rc.ffb_candidates(q[:n], clock, s.qpc_frequency, posts, state, first)
    got = np.array([lv if lv is not None else -1 << 40 for _, lv in plays[:n]], dtype=np.int64)
    # the candidates as levels; a write matches when its level is one of them (NaN candidates never match)
    level_of = lambda v: np.where(np.isnan(v), np.nan, wine_level(np.nan_to_num(v)).astype(np.float64))  # noqa: E731
    cand = np.concatenate([np.where(rows >= 0, ours[np.maximum(rows, 0)], np.nan), literal], axis=1)
    ok = (level_of(cand) == got[:, None]).any(axis=1)
    run = cls != "not_run"
    chain = int((got == wine_level(sets[:n])).sum())
    lines = ["RSF: %d writes paired with SetParameters (%d sets, %d plays); level == Proton's level of the set's own "
             "magnitude %d (the device chain)" % (n, len(q), len(plays), chain),
             "RSF vs ours: %d of %d writes at Proton's level of a magnitude ours allows (%s); %d not compared "
             "(past the steps run%s)" % (int((ok & run).sum()), int(run.sum()), ", ".join(
                 "%s %d/%d" % (c, int((ok & (cls == c)).sum()), int((cls == c).sum())) for c in rc.CLASSES[:-1]),
                 int((~run).sum()), " or --last %d" % last if last < 2 ** 32 else "")]
    misses = np.nonzero(run & ~ok)[0]
    steps_ = rc.steps_at(q[:n], clock)
    for i in misses[:10]:
        lines.append("  miss: write %d at step %d (%s): level %d, ours %s" % (
            i, steps_[i], cls[i], got[i], [int(ours[r]) for r in rows[i] if r >= 0] +
            [int(v) for v in literal[i] if not np.isnan(v)]))
    # the delay from a step's end (STP1, on CLOCK_MONOTONIC by raw_offset) to its first write
    if raw_offset is None:
        lines.append("RSF: step end -> wheel not measured (give --raw-offset)")
    else:
        end = {r["post"]: r["qpc"] * 100 + raw_offset for r in s.tagged("STP1")}
        first_write = {}
        for i in np.nonzero(run & ok & (cls == "plain"))[0]:
            first_write.setdefault(int(steps_[i]), plays[i][0])
        delay = [(t - end[p]) / 1e6 for p, t in first_write.items() if p in end]
        if delay:
            lines.append("RSF: step end -> wheel %s (%d steps)" % (pct(delay), len(delay)))
    # ours: ffb_play's start is not traced; take the shift at which the most writes hold the line due
    lo = our_steps(s, ext_path, last)
    lt = np.array([t for t, _, _ in lo if t >= 0])
    lw = wine_level([v for t, v, _ in lo if t >= 0])
    lp = [p for t, _, p in lo if t >= 0]
    ot, ol = levels(read(ours_dir))
    if not len(ot):
        return "\n".join(lines + ["ours: no writes in %s" % ours_dir])
    start = np.nonzero((ol != 0) & (ol != -1))[0]
    base = ot[start[0]] - lt[np.nonzero(lw != -1)[0][0]] * 1e9

    def due(b):
        i = np.searchsorted(lt * 1e9 + b, ot, side="right") - 1
        return (i >= 0) & (lw[np.maximum(i, 0)] == ol), i, b

    hit, idx, b = max((due(b) for b in base + np.arange(-3e6, 0.5e6, 2e4)), key=lambda x: int(x[0].sum()))
    during = (idx >= 0) & (idx < len(lt) - 1)
    ours_level, delay = {}, []
    for j in np.nonzero(hit & during)[0]:
        if lp[idx[j]] not in ours_level:
            ours_level[lp[idx[j]]] = ol[j]
            delay.append((ot[j] - (lt[idx[j]] * 1e9 + b)) / 1e6)
    lines.append("ours: %d writes while the lines ran, %d at the level of the line due, %d not; %d steps reached the "
                 "wheel, line time -> wheel %s" % (during.sum(), (hit & during).sum(), (~hit & during).sum(),
                                                  len(ours_level), pct(delay) if delay else "-"))
    return "\n".join(lines)


def held_levels(directory, seconds):
    """The level the device holds on a 1 ms grid from 1 s before the first non-zero level, for `seconds`."""
    t, v = levels(read(directory))
    nz = np.nonzero(v)[0]
    if not len(nz):
        raise SystemExit("%s: no non-zero level" % directory)
    grid = t[nz[0]] - 1_000_000_000 + np.arange(int(seconds * 1000)) * 1_000_000
    i = np.searchsorted(t, grid, side="right") - 1
    return np.where(i >= 0, v[np.maximum(i, 0)], 0)


def distinct(directory):
    """The levels the device was given, consecutive repeats dropped (from the first non-zero one)."""
    _, v = levels(read(directory))
    v = v[np.nonzero(v)[0][0]:]
    return v[np.r_[True, np.diff(v) != 0]]


def in_order(ours, ref):
    """How many of `ours` appear in `ref` in the same order (greedy)."""
    j, n = 0, 0
    for x in ours:
        k = j
        while k < len(ref) and ref[k] != x:
            k += 1
        if k < len(ref):
            n, j = n + 1, k + 1
    return n


def compare(rsf_dir, ours_dirs, out, seconds):
    ref = held_levels(rsf_dir, seconds)
    ref_seq = distinct(rsf_dir)
    x = np.arange(len(ref)) / 1000.0
    lines, panels = [], []
    for d in ours_dirs:
        got = held_levels(d, seconds)
        n = min(len(ref), len(got))
        # the shift (whole ms, -5 to 5) at which the two held levels agree most: two replays start independently
        lag = max(range(-5, 6), key=lambda k: int((np.roll(got[:n], k) == ref[:n])[5:-5].sum()))
        diff = (np.roll(got[:n], lag) - ref[:n])[5:-5]
        seq = distinct(d)
        lines.append("%s vs %s over %.1f s (1 ms grid, shifted %+d ms): equal %.4f, |diff| max %d, mean %.2f; "
                     "levels given %d vs %d, %d of ours in RSF's order" % (
                         d, rsf_dir, n / 1000.0, lag, float((diff == 0).mean()), int(np.abs(diff).max()),
                         float(np.abs(diff).mean()), len(seq), len(ref_seq), in_order(seq[:len(ref_seq) + 50], ref_seq)))
        panels.append((np.roll(got[:n], lag), np.r_[np.zeros(5), diff, np.zeros(5)]))
    if out:
        base = os.path.splitext(out)[0]
        for k, (got, diff) in enumerate(panels):
            suffix = "" if len(panels) == 1 else "-%d" % (k + 1)
            n = len(got)
            plot(base + suffix + ".png", x[:n], [(ref[:n], BLUE), (got, ORANGE)])
            plot(base + suffix + "-diff.png", x[:n], [(diff, ORANGE)])
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("summary")
    a.add_argument("dir")
    a.add_argument("session", nargs="?")
    a = sub.add_parser("magnitudes")
    a.add_argument("session")
    a.add_argument("--from", dest="start", type=float, default=0.0)
    a.add_argument("--seconds", type=float, default=10.0)
    a = sub.add_parser("ours")
    a.add_argument("session")
    a.add_argument("ext")
    a.add_argument("--last", type=int, default=2**32)
    a = sub.add_parser("steps")
    for name in ("session", "ext", "rsf", "ours"):
        a.add_argument(name)
    a.add_argument("--raw-offset", type=float)
    a.add_argument("--last", type=int, default=2**32)
    a = sub.add_parser("compare")
    a.add_argument("rsf")
    a.add_argument("ours", nargs="+")
    a.add_argument("--out")
    a.add_argument("--seconds", type=float, default=10.0)
    o = ap.parse_args()
    if o.cmd == "summary":
        print(summary(o.dir, o.session))
    elif o.cmd == "magnitudes":
        sys.stdout.write(magnitudes(o.session, o.start, o.seconds))
    elif o.cmd == "ours":
        sys.stdout.write(ours_lines(o.session, o.ext, o.last))
    elif o.cmd == "steps":
        print(steps(o.session, o.ext, o.rsf, o.ours, o.raw_offset, o.last))
    else:
        print(compare(o.rsf, o.ours, o.out, o.seconds))


if __name__ == "__main__":
    main()
