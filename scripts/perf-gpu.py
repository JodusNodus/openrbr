#!/usr/bin/env python3
"""GPU time per pass on macOS: runs the viewer's `--frames N` timing under Instruments' Metal System Trace (`xcrun
xctrace`) and prints, per pass label, the GPU milliseconds per frame (vertex, fragment and compute summed) over the
frames it draws: 3N + 2, 4N + 2 with `--profile` (`viewer` main.rs), from the first stage pass on, so that the load's
mips and uploads stay out. Metal's own intervals, so unlike `viewer --profile` it works on Apple's tile GPUs, and only
the viewer's work counts (the window server and other apps share the GPU). Still a time, and the sum counts a pass's
vertex and fragment work even where they overlap. The GPU's clock moves it: on an M3 Pro the whole frame varied by
30 % between runs of one build, so it shows which passes cost most, not small differences between builds. A template
saved from Instruments with the GPU's performance state fixed (PERF_GPU_TEMPLATE) takes the clock out.
Usage: python3 -I scripts/perf-gpu.py VIEWER_ARGS...   e.g. track-54_E --car COROLLA --no-sound --screenshot
       /tmp/shot.png --frames 100 (the viewer is target/release/viewer, VIEWER picks another; RBR_DATA, RBR_PATCH)
"""

import collections
import os
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


def rows(path):
    """The rows of one exported table as dicts of elements, the exporter's `ref`s resolved."""
    ids, cols = {}, None
    for _, el in ET.iterparse(path, events=("end",)):
        if el.tag == "schema":
            cols = [c.findtext("mnemonic") for c in el.findall("col")]
        elif el.tag == "row":
            vals = []
            for c in el:
                for sub in c.iter():
                    if "id" in sub.attrib:
                        ids[sub.attrib["id"]] = sub
                vals.append(ids.get(c.attrib["ref"]) if "ref" in c.attrib else c)
            yield dict(zip(cols, vals))
            el.clear()


def shown(e):
    return "" if e is None else e.attrib.get("fmt", e.text or "")


def export(trace, schema, out):
    xpath = f'/trace-toc/run[@number="1"]/data/table[@schema="{schema}"]'
    with open(out, "w") as f:
        subprocess.run(["xcrun", "xctrace", "export", "--input", trace, "--xpath", xpath], stdout=f, check=True)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    args = sys.argv[1:]
    if "--frames" not in args:
        sys.exit("perf-gpu: needs --frames N")
    n = int(args[args.index("--frames") + 1])
    frames = (4 if "--profile" in args else 3) * n + 2
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    viewer = os.environ.get("VIEWER", os.path.join(root, "target/release/viewer"))
    with tempfile.TemporaryDirectory(prefix="perf-gpu-") as out:
        trace = os.path.join(out, "run.trace")
        env = [a for k in ("RBR_DATA", "RBR_PATCH") if k in os.environ for a in ("--env", f"{k}={os.environ[k]}")]
        with open(os.path.join(out, "record.log"), "w") as log:
            template = os.environ.get("PERF_GPU_TEMPLATE", "Metal System Trace")
            subprocess.run(["xcrun", "xctrace", "record", "--no-prompt", "--template", template,
                            "--output", trace, *env, "--launch", "--", viewer, *args],
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        gpu, encoders = os.path.join(out, "gpu.xml"), os.path.join(out, "encoders.xml")
        export(trace, "metal-gpu-intervals", gpu)
        export(trace, "metal-application-encoders-list", encoders)

        name = os.path.basename(viewer)
        labels = {}
        for r in rows(encoders):
            if shown(r["process"]).startswith(name):
                labels[r["encoder-id"].text] = shown(r["encoder-label"])
        intervals = [(int(r["start"].text), int(r["duration"].text) / 1e6, labels[r["encoder-id"].text])
                     for r in rows(gpu)
                     if shown(r["process"]).startswith(name) and r["encoder-id"].text in labels]

    first = min(start for start, _, label in intervals if label == "stage")
    total, runs = collections.defaultdict(float), collections.Counter()
    for start, ms, label in intervals:
        if start >= first:
            total[label] += ms
            runs[label] += 1
    print(f"GPU ms per frame over {frames} frames (runs of the pass per frame):")
    for label in sorted(total, key=lambda label: -total[label]):
        print(f"  {label:40s} {total[label] / frames:7.3f}  ({runs[label] / frames:.2f})")
    print(f"  {'all':40s} {sum(total.values()) / frames:7.3f}")


if __name__ == "__main__":
    main()
