#!/usr/bin/env python3
"""Lighting sheet: render a fixed set of views at fixed hours and weathers with one or two viewer binaries and
compose one sheet PNG per view (rows hours, columns weather), with a per-view a/b difference summary.

  python3 -I scripts/light-sheet.py --a target/release/viewer [--b OTHER] --out DIR [--quick] [--root CHECKOUT]
                                    [--drive RECORDING] [--views forest,chase] [--jobs 3] [--reuse]

Raw renders go to DIR/raw/{a,b}/VIEW_HHMM_WEATHER.png, sheets to DIR/VIEW.png, the summary to DIR/summary.txt.
--root is the checkout holding original/ (RBR_DATA, RBR_PATCH, RBR_RSF, RBR_NGP_PHYSICS default to it; default
$RBR_ROOT, else this checkout). The chase and cockpit views need --drive, a 45O drive recording; without it they are
left out."""
import argparse, os, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

SIZE = "1920x1080"
TIMEOUT = 300
HOURS = ["07:00", "10:00", "13:00", "17:00", "18:00", "23:00"]
QUICK_HOURS = ["10:00", "18:00", "23:00"]
# column name -> viewer flags
WEATHERS = {"dry": ["--weather", "dry"], "wet": ["--weather", "wet"], "fog": ["--weather", "dry", "--fog", "heavy"]}
RSF = "original/rsf/Maps/"
# key: (label, stage under --root or retail name, extra flags). All use the stage's start camera; a fixed
# "--camera EX,EY,EZ,FX,FY,FZ" (eye, then look direction) can go in the flags.
VIEWS = {
    "forest": ("forest, retail 41M start", "track-41_M", []),
    "open": ("open ground, RSF 394M Verkiai", RSF + "394-Verkiai 2010/track-394_M", []),
    "cliff": ("cliff and cutting, RSF 460M Valstagna", RSF + "460-PS-Valstagna/track-460_M", []),
    "crowd": ("crowd under the start tent, RSF 176M Semetin", RSF + "176-Semetin 2009/track-176_M", []),
    "chase": ("chase, retail 45O Xsara", "track-45_O", ["--car", "XSARA", "--drive", "{drive}", "--view", "chase"]),
    "cockpit": ("cockpit, retail 45O Xsara", "track-45_O", ["--car", "XSARA", "--drive", "{drive}", "--view", "cockpit"]),
    "unbaked": ("no light bake, RSF 369M Capo Di Feno", RSF + "369-Capo Di Feno/track-369_M", []),
}


def render(job):
    viewer, stage, flags, out, env = job
    if out.exists():
        return job, None
    cmd = [viewer, stage, "--enhance", "all", "--screenshot", str(out), "--size", SIZE, "--no-sound", *flags]
    try:
        r = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return job, f"timeout after {TIMEOUT} s"
    if r.returncode != 0 or not out.exists():
        return job, f"exit {r.returncode}: {(r.stderr or r.stdout).strip().splitlines()[-1:]}"
    return job, None


def font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:
        return ImageFont.load_default()


def load(path):
    return Image.open(path).convert("RGB") if path.exists() else None


def compose(key, label, hours, weathers, variants, raw, out_dir, width=2400):
    """One sheet: rows hours, columns weather, per cell one image per variant (a, b)."""
    n = len(variants)
    cw = min(960, width // (len(weathers) * n))
    ch = cw * 9 // 16
    f, left, top = font(max(14, cw // 28)), 130, 80
    sheet = Image.new("RGB", (left + cw * n * len(weathers), top + ch * len(hours)), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((8, 6), label, fill=(255, 255, 255), font=f)
    for ci, w in enumerate(weathers):
        d.text((left + ci * cw * n + 8, top - 34), w + ("   (a | b)" if n == 2 else ""), fill=(255, 255, 255), font=f)
    for ri, h in enumerate(hours):
        d.text((8, top + ri * ch + ch // 2 - 8), h, fill=(255, 255, 255), font=f)
        for ci, w in enumerate(weathers):
            for vi, v in enumerate(variants):
                im = load(raw / v / f"{key}_{h.replace(':', '')}_{w}.png")
                x, y = left + (ci * n + vi) * cw, top + ri * ch
                if im is None:
                    d.rectangle([x, y, x + cw - 1, y + ch - 1], outline=(160, 0, 0))
                    d.text((x + 8, y + 8), "failed", fill=(255, 80, 80), font=f)
                else:
                    sheet.paste(im.resize((cw, ch), Image.LANCZOS), (x, y))
    sheet.save(out_dir / f"{key}.png")


def diff_stats(key, hours, weathers, raw):
    """Pixel differences a against b over all rendered cells of a view; channels are 0..255."""
    n = total = s_abs = s8 = s32 = 0
    mx = 0
    for h in hours:
        for w in weathers:
            name = f"{key}_{h.replace(':', '')}_{w}.png"
            a, b = load(raw / "a" / name), load(raw / "b" / name)
            if a is None or b is None:
                continue
            dd = np.abs(np.asarray(a, np.int16) - np.asarray(b, np.int16))
            px = dd.max(axis=2)
            s_abs += dd.mean(); mx = max(mx, int(dd.max()))
            s8 += (px > 8).mean(); s32 += (px > 32).mean(); n += 1
    if not n:
        return f"{key:8} no complete a/b pair"
    return f"{key:8} cells {n:3}  mean {s_abs / n:7.3f}  max {mx:3}  >8 {100 * s8 / n:6.3f}%  >32 {100 * s32 / n:6.3f}%"


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--a", required=True); p.add_argument("--b"); p.add_argument("--out", required=True)
    p.add_argument("--quick", action="store_true", help="10:00, 18:00, 23:00 and dry only")
    p.add_argument("--root", default=os.environ.get("RBR_ROOT", str(Path(__file__).resolve().parents[1])))
    p.add_argument("--drive", help="a 45O drive recording for the chase and cockpit views")
    p.add_argument("--views", help="comma list of: " + ",".join(VIEWS))
    p.add_argument("--jobs", type=int, default=3); p.add_argument("--reuse", action="store_true",
                   help="keep renders already in DIR/raw")
    a = p.parse_args()
    t0 = time.time()
    root = Path(a.root)
    env = dict(os.environ)
    env.setdefault("RBR_DATA", str(root / "original/retail-1.0/install"))
    env.setdefault("RBR_PATCH", str(root / "original/rsf-1.02"))
    env.setdefault("RBR_RSF", str(root / "original/rsf"))
    env.setdefault("RBR_NGP_PHYSICS", str(root / "original/ngp/physics.rbz"))
    keys = a.views.split(",") if a.views else list(VIEWS)
    if not a.drive:
        keys = [k for k in keys if "{drive}" not in VIEWS[k][2]]
    hours = QUICK_HOURS if a.quick else HOURS
    weathers = ["dry"] if a.quick else list(WEATHERS)
    variants = {"a": a.a, **({"b": a.b} if a.b else {})}
    out_dir = Path(a.out); raw = out_dir / "raw"
    jobs = []
    for v, viewer in variants.items():
        (raw / v).mkdir(parents=True, exist_ok=True)
        for k in keys:
            _, stage, extra = VIEWS[k]
            stage = stage if stage.startswith("track-") else str(root / stage)
            for h in hours:
                for w in weathers:
                    out = raw / v / f"{k}_{h.replace(':', '')}_{w}.png"
                    if not a.reuse:
                        out.unlink(missing_ok=True)
                    flags = [x.format(drive=a.drive) for x in extra] + ["--clock", h, *WEATHERS[w]]
                    jobs.append((viewer, stage, flags, out, env))
    print(f"{len(jobs)} renders, {a.jobs} at a time", flush=True)
    failed = []
    with ThreadPoolExecutor(a.jobs) as ex:
        for i, (job, err) in enumerate(ex.map(render, jobs), 1):
            if err:
                failed.append(f"{job[3].relative_to(out_dir)}: {err}")
                print("FAILED", failed[-1], flush=True)
            if i % 10 == 0:
                print(f"{i}/{len(jobs)} {time.time() - t0:.0f} s", flush=True)
    lines = []
    for k in keys:
        compose(k, VIEWS[k][0], hours, weathers, list(variants), raw, out_dir)
        if a.b:
            lines.append(diff_stats(k, hours, weathers, raw))
    lines.append(f"a {a.a}" + (f"\nb {a.b}" if a.b else ""))
    lines += [f"FAILED {f}" for f in failed]
    lines.append(f"{len(jobs) - len(failed)}/{len(jobs)} renders ok, total {time.time() - t0:.0f} s")
    (out_dir / "summary.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    sys.exit(main())
