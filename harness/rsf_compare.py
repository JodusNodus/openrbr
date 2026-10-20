"""rsf_compare.py SESSION_DIR [--attempt N] --out REPORT_DIR: compares each recorded RSF stage attempt with ReRBR.

Usage: python3 -I -B harness/rsf_compare.py SESSION_DIR --out REPORT_DIR [--attempt N] [--sections LIST]
           [--standalone BIN] [--sound BIN] [--ngp-physics FILE] [--rsf-car ID|PACK] [--stage NAME] [--setup FILE]
           [--ffb-sensitivity G,T,S] [--camera inside|chase|trackside] [--keep-work]

An attempt's <n>/setup.lsp (the setup the game loaded) goes to the boot as --setup unless --setup names another;
--ffb-sensitivity replaces meta.txt's percentages.

SESSION_DIR is the session tree rec/capture fetches (notes/verification.md "Comparing RSF sessions"): <n>/physics.bin
(FRM1), <n>/session.bin (RSS1, harness/session.py), audio.wav with audio.txt, meta.txt. For each attempt it writes
REPORT_DIR/<n>/report.md, summary.json and PNG plots, and REPORT_DIR/index.md lists the attempts' headline numbers.
--sections takes any of physics,input,ffb,sound (default all). The programs default to this checkout's
target/release; RBR_DATA, RBR_PATCH, RBR_RSF and RBR_NGP_PHYSICS are read as rbr-standalone reads them.

1. physics: `rbr-standalone --boot --recording <n>/physics.bin --recorded-payload --trace-out` with the stage, car,
   weather and tyre of the attempt's GAM1 (the car an RSF car with --rsf-car or the one [carNN] of meta.txt, NGP
   physics when NGP.dll was loaded, the force-feedback sensitivities of meta.txt's [NGP]), as the launcher's command
   (crates/rbr-launcher/src/command.rs) builds them. The equal steps, the first differing step and its words come
   from rbr-standalone's report (rbr_hybrid::pure::word_name); the curves from both FRM1s and our trace's .ext.
2. input: the raw polls (POL1/BUF1 decoded by FMT1, ranges from PRP1) per step against the payload channels. A
   diagnostic only: the session holds no bindings and the device poll is not ported (the report says so).
3. ffb: the constant force's SetParameters magnitudes (EFF1, by STP1's QPC) against rbr_gameplay::ffb::magnitude and
   dead_zone of our steering_ffb::force: correlation, RMS error, lag.
4. sound (approximate, a rough baseline): `rbr-sound` renders the attempt (chase camera by default), aligned to
   audio.wav by CLK1 and audio.txt's start_unix_ns, then by the loudness envelopes: loudness per second and the
   spectrograms side by side.
"""
import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import zlib

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import session as rss  # noqa: E402

SECTIONS = ("physics", "input", "ffb", "sound")
HEADER, CONTROL, WHEEL, WHEELS = 20, 0x20, 0xA00, 4
FRM1 = 0x314D5246
STEP_102, STEP_NGP = 0.007, 1.0 / 720  # notes/verification.md "Recordings": a substep
SETTLE_SUBSTEPS = 40
FILETIME_UNIX = 116444736000000000  # 100 ns ticks from 1601 to 1970
CONSTANT_FORCE = "13541c20-8e33-11d0-9ad0-00a0c9a06e35"
DIPROP_RANGE = 4
CHANNELS = ("throttle", "brake", "handbrake", "clutch", "steering")  # payload words 0-4
# Frame words with a name (crates/rbr-recording/src/pose.rs); the rest keep rbr_hybrid::pure::word_name's car+off.
CAR_NAMES = {0x100: "orientation", 0x140: "position", 0x17C: "driveline parameter", 0x1C0: "velocity",
             0x15FC: "speed"}
WHEEL_NAMES = {0x100: "orientation", 0x140: "anchor", 0x5AC: "roll angle", 0x5BC: "strut travel"}
BLUE, ORANGE, RED, GREY = (31, 119, 180), (255, 127, 14), (214, 39, 40), (225, 225, 225)


# --- files -------------------------------------------------------------------------------------------------------

def read_keys(path):
    """A key=value file as {section: {key: value}} (keys lower case; lines before any [section] under "")."""
    out, section = {"": {}}, ""
    if not os.path.exists(path):
        return out
    for line in open(path, encoding="utf-8", errors="replace"):
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip().lower()
            out.setdefault(section, {})
        elif "=" in line and not line.startswith((";", "#")):
            k, v = line.split("=", 1)
            out.setdefault(section, {})[k.strip().lower()] = v.strip()
    return out


def read_meta(path):
    """meta.txt as read_keys does; the capture's flat `before.ini.<Section>.<Key>=` lines (the ini as the session
    began) also go under their section, where an `[section]` line holds none of the key."""
    meta = read_keys(path)
    for key, value in meta[""].items():
        if key.startswith("before.ini.") and key.count(".") >= 3:
            section, name = key[len("before.ini."):].split(".", 1)
            meta.setdefault(section, {}).setdefault(name, value)
    return meta


class Frames:
    """An FRM1 recording, memory-mapped: rows of `size` bytes, the car `car` bytes."""

    def __init__(self, path):
        with open(path, "rb") as f:
            magic, _, _, _, car = struct.unpack("<5I", f.read(HEADER))
        if magic != FRM1:
            raise ValueError("%s: not FRM1" % path)
        self.car, self.size = car, HEADER + CONTROL + car + WHEELS * WHEEL
        n = os.path.getsize(path) // self.size
        self.raw = np.memmap(path, dtype=np.uint8, mode="r", shape=(n, self.size)) if n else np.zeros((0, self.size),
                                                                                                        np.uint8)
        self.n = n

    def words(self, offsets, chunk=4096):
        """The u32 at each frame offset, one column per offset, in one pass over the file."""
        out = np.empty((self.n, len(offsets)), dtype=np.uint32)
        cols = np.array([[o + i for i in range(4)] for o in offsets]).ravel()
        for a in range(0, self.n, chunk):
            out[a:a + chunk] = np.ascontiguousarray(self.raw[a:a + chunk][:, cols]).view("<u4")
        return out

    def car_at(self, off):
        return HEADER + CONTROL + off

    def wheel_at(self, k, off):
        return HEADER + CONTROL + self.car + k * WHEEL + off


def fields(frames):
    """Per frame: pre, post, substeps, control words, position, velocity, strut travel (FRM1 offsets)."""
    names = ["pre", "post", "substeps"] + ["c%d" % i for i in range(8)] + ["px", "py", "pz", "vx", "vy", "vz"] + \
        ["travel%d" % k for k in range(WHEELS)] + ["qx", "qy", "qz", "qw"]
    offs = [4, 8, 12] + [HEADER + 4 * i for i in range(8)] + [frames.car_at(0x140 + 4 * i) for i in range(3)] + \
        [frames.car_at(0x1C0 + 4 * i) for i in range(3)] + [frames.wheel_at(k, 0x5BC) for k in range(WHEELS)] + \
        [frames.car_at(0x100 + 4 * i) for i in range(4)]
    w = frames.words(offs)
    f = {n: w[:, i] for i, n in enumerate(names)}
    as_f = lambda n: w[:, names.index(n)].view("<f4").astype(np.float64)  # noqa: E731
    f["position"] = np.stack([as_f(n) for n in ("px", "py", "pz")], axis=1)
    v = np.stack([as_f(n) for n in ("vx", "vy", "vz")], axis=1)
    f["speed"] = np.linalg.norm(v, axis=1)
    # The sideways speed: the velocity along body +X (pose.rs: active rotation of [x, y, z, w], forward -Y).
    x, y, z, w_ = (as_f(n) for n in ("qx", "qy", "qz", "qw"))
    right = np.stack([1 - 2 * (y * y + z * z), 2 * (x * y + z * w_), 2 * (x * z - y * w_)], axis=1)
    f["sideways"] = np.abs(np.sum(v * right, axis=1))
    f["steering"] = as_f("c4")
    f["travel"] = np.stack([as_f("travel%d" % k) for k in range(WHEELS)], axis=1)
    f["control"] = w[:, 3:11]
    return f


def recorded_state(s, posts):
    """STA1 at each post step: engine speed (region 1, the engine state's +0x2c, EngineNoise p[0] as our .ext),
    gear (region 2, +0x54c), |slip ratio| (regions 3-6, +0x20); None without STA1, NaN where a step has none."""
    sta = s.tagged("STA1")
    if not sta:
        return None
    at = {r["post"]: r["regions"] for r in sta}

    def word(regions, region, off, kind):
        b = regions.get(region, b"") if regions else b""
        return struct.unpack_from(kind, b, off)[0] if len(b) >= off + 4 else np.nan

    rows = [at.get(int(p)) for p in posts]
    return {"engine": np.array([word(r, 1, 0x2C, "<f") for r in rows], dtype=np.float64),
            "gear": np.array([word(r, 2, 0x54C, "<I") for r in rows], dtype=np.float64),
            "slip": np.array([[word(r, 3 + k, 0x20, "<f") for k in range(WHEELS)] for r in rows],
                             dtype=np.float64).reshape(-1, WHEELS)}


def read_ext(path):
    """Our trace's .ext (headless/trace.rs): post, force, engine speed, gear, slip[4], MaxFFTorque per step."""
    a = np.fromfile(path, dtype="<u4").reshape(-1, 9)
    return {"post": a[:, 0], "force": a[:, 1].view("<f4"), "engine": a[:, 2].view("<f4").astype(np.float64),
            "gear": a[:, 3].astype(np.int64), "slip": a[:, 4:8].view("<f4").astype(np.float64),
            "max_ff_torque": a[:, 8].view("<f4")}


def read_wav(path, start=0.0, seconds=None):
    """(samples float64 [n, channels], rate) from `start` s for `seconds` (default to the end): PCM 16/24/32 or IEEE
    float 32/64 WAV, by its chunks, the data memory-mapped (a session's audio is hundreds of MB)."""
    with open(path, "rb") as f:
        head = f.read(12)
        if head[:4] != b"RIFF" or head[8:12] != b"WAVE":
            raise ValueError("%s: not a WAV" % path)
        fmt, data = None, None
        while True:
            chunk = f.read(8)
            if len(chunk) < 8:
                break
            cid, n = struct.unpack("<4sI", chunk)
            if cid == b"data":
                data = (f.tell(), n)
                break
            body = f.read(n + (n & 1))
            if cid == b"fmt ":
                fmt = struct.unpack_from("<HHIIHH", body)
                if fmt[0] == 0xFFFE:  # WAVE_FORMAT_EXTENSIBLE: the sub-format's first word
                    fmt = (struct.unpack_from("<H", body, 24)[0],) + fmt[1:]
    tag, channels, rate, _, align, bits = fmt
    at, n = data
    n = min(n, os.path.getsize(path) - at) // align
    first = max(0, min(n, int(start * rate)))
    last = n if seconds is None else max(first, min(n, first + int(seconds * rate)))
    raw = np.memmap(path, dtype=np.uint8, mode="r", offset=at, shape=(n * align,))[first * align:last * align]
    if tag == 3:
        x = raw.view("<f%d" % (bits // 8)).astype(np.float64)
    elif bits == 24:
        b = raw.reshape(-1, 3).astype(np.int32)
        x = ((b[:, 0] | b[:, 1] << 8 | b[:, 2] << 16) << 8 >> 8) / 8388608.0
    else:
        x = raw.view("<i%d" % (bits // 8)).astype(np.float64) / float(1 << (bits - 1))
    return x.reshape(-1, channels), rate


# --- the session's clocks ----------------------------------------------------------------------------------------

def step_clock(s):
    """STP1 as arrays: the QPC after each StepForward and its post step."""
    stp = s.tagged("STP1")
    return np.array([r["qpc"] for r in stp], dtype=np.int64), np.array([r["post"] for r in stp], dtype=np.int64)


def steps_at(qpcs, clock):
    """The post step of the last StepForward done by each QPC (-1 before the first)."""
    q, post = clock
    i = np.searchsorted(q, np.asarray(qpcs, dtype=np.int64), side="right") - 1
    return np.where(i >= 0, post[np.maximum(i, 0)], -1)


def unix_of_qpc(s):
    """QPC -> Unix seconds from CLK1 (a line through every pair; one pair takes the QPC frequency)."""
    clk = s.tagged("CLK1")
    if not clk:
        return None
    q = np.array([r["qpc"] for r in clk], dtype=np.float64)
    t = (np.array([r["filetime"] for r in clk], dtype=np.float64) - FILETIME_UNIX) / 1e7
    if len(clk) > 1 and np.ptp(q) > 0:
        a, b = np.polyfit(q - q[0], t, 1)
        return lambda qpc: a * (np.asarray(qpc, dtype=np.float64) - q[0]) + b
    return lambda qpc: t[0] + (np.asarray(qpc, dtype=np.float64) - q[0]) / s.qpc_frequency


def held(at_steps, values, posts):
    """Each post step's value: the last value set at or before it (NaN before the first)."""
    order = np.argsort(at_steps, kind="stable")
    at_steps, values = np.asarray(at_steps)[order], np.asarray(values, dtype=np.float64)[order]
    i = np.searchsorted(at_steps, posts, side="right") - 1
    return np.where(i >= 0, values[np.maximum(i, 0)], np.nan)


# --- PNG and plots -----------------------------------------------------------------------------------------------

def write_png(path, rgb):
    """An RGB uint8 image [h, w, 3] as PNG (zlib, struct)."""
    h, w, _ = rgb.shape
    raw = b"".join(b"\0" + rgb[y].tobytes() for y in range(h))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) +
                chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def plot(path, x, series, mark=None, width=1000, height=200):
    """Lines of (y, colour) over x, each pixel column drawn from its samples' min to max; grey quarter lines, a red
    line at x = mark. Returns the axes' ranges for the caption."""
    img = np.full((height, width, 3), 255, dtype=np.uint8)
    x = np.asarray(x, dtype=np.float64)
    ys = [np.asarray(y, dtype=np.float64) for y, _ in series]
    finite = np.concatenate([y[np.isfinite(y)] for y in ys] + [np.zeros(0)])
    if not len(x) or not len(finite):
        write_png(path, img)
        return None
    x0, x1 = float(x.min()), float(x.max())
    y0, y1 = float(finite.min()), float(finite.max())
    if y1 - y0 < 1e-12:
        y0, y1 = y0 - 1, y1 + 1
    for q in (1, 2, 3):
        img[height * q // 4, :] = GREY
        img[:, width * q // 4] = GREY
    col = np.clip(((x - x0) / max(x1 - x0, 1e-12) * (width - 1)).astype(int), 0, width - 1)
    for y, (_, colour) in zip(ys, series):
        ok = np.isfinite(y)
        row = np.clip(((y1 - y[ok]) / (y1 - y0) * (height - 1)).astype(int), 0, height - 1)
        c = col[ok]
        lo, hi = np.full(width, height, int), np.full(width, -1, int)
        np.minimum.at(lo, c, row)
        np.maximum.at(hi, c, row)
        for px in np.nonzero(hi >= 0)[0]:
            img[lo[px]:hi[px] + 1, px] = colour
    if mark is not None and x0 <= mark <= x1:
        img[:, int((mark - x0) / max(x1 - x0, 1e-12) * (width - 1))] = RED
    write_png(path, img)
    return x0, x1, y0, y1


def caption(ranges, xunit="step"):
    if ranges is None:
        return "(no data)"
    x0, x1, y0, y1 = ranges
    return "%s %g to %g, y %.4g to %.4g" % (xunit, x0, x1, y0, y1)


def colour_map(v):
    """[0, 1] -> RGB, dark blue to yellow."""
    stops = np.array([[0, 0, 40], [60, 20, 120], [200, 40, 80], [250, 150, 30], [255, 255, 160]], dtype=np.float64)
    p = np.clip(v, 0, 1) * (len(stops) - 1)
    i = np.minimum(p.astype(int), len(stops) - 2)
    t = (p - i)[..., None]
    return (stops[i] * (1 - t) + stops[i + 1] * t).astype(np.uint8)


# --- 1. physics --------------------------------------------------------------------------------------------------

def stage_candidates(data, number):
    maps = os.path.join(data, "Maps")
    names = sorted({f.split(".")[0] for f in os.listdir(maps) if re.fullmatch(r"track-%d_[A-Z]\.trk" % number, f)}) \
        if os.path.isdir(maps) else []
    return names


def race_flags(game, meta, opts, attempt):
    """The --boot flags the launcher builds for this attempt (command.rs viewer_args/launch_args), and notes."""
    notes, flags = [], ["--weather", str(game["weather"]), "--tyre", str(game["tyre"]), "--car", str(game["car"])]
    ngp = game["ngp_base"] != 0
    if ngp:
        physics = opts.ngp_physics or os.environ.get("RBR_NGP_PHYSICS")
        if physics:
            flags += ["--ngp-physics", physics]
        else:
            notes.append("NGP.dll was loaded but no NGP physics.rbz is given (--ngp-physics, RBR_NGP_PHYSICS): 1.02 "
                         "physics")
            ngp = False
    rsf_car = opts.rsf_car or read_keys(os.path.join(attempt, "attempt.txt"))[""].get("rsf_car_id")
    if rsf_car is None:
        cars = sorted({int(m.group(1)) for k in meta for m in [re.fullmatch(r"car(\d+)", k)] if m})
        if len(cars) == 1:
            rsf_car = str(cars[0])
        elif cars:
            notes.append("meta.txt names cars %s; give --rsf-car" % cars)
    if rsf_car and ngp:
        flags += ["--rsf-car", rsf_car]
    sens = ffb_sensitivity(meta, game["car"], rsf_car, opts.ffb_sensitivity)
    if ngp and sens["percent"] != [100, 100, 100]:
        flags += ["--ffb-sensitivity", ",".join(str(p) for p in sens["percent"])]
    setup = opts.setup or next((p for p in [os.path.join(attempt, "setup.lsp")] if os.path.exists(p)), None)
    if setup:
        flags += ["--setup", setup]
    else:
        notes.append("no --setup: the boot takes the car's default setup, not the one driven (the session holds none)")
    return flags, ngp, rsf_car, notes


def ffb_sensitivity(meta, slot, rsf_car, given=None):
    """NGP's ForceFeedbackSensitivity<Surface>[_Car<slot>] from meta.txt's [NGP] (notes/ngp.md), else the RSF car's
    forcefeedbacksensitivity<surface>; percent per gravel, tarmac, snow, and where each came from."""
    if given:
        return {"percent": [int(p) for p in given.split(",")], "source": ["--ffb-sensitivity"] * 3}
    ngp, car = meta.get("ngp", {}), meta.get("car%s" % rsf_car, {}) if rsf_car else {}
    percent, source = [], []
    for surface in ("gravel", "tarmac", "snow"):
        for where, table, key in (("[NGP]", ngp, "forcefeedbacksensitivity%s_car%d" % (surface, slot)),
                                  ("[NGP]", ngp, "forcefeedbacksensitivity%s" % surface),
                                  ("[car%s]" % rsf_car, car, "forcefeedbacksensitivity%s" % surface)):
            try:
                value = float(table[key])
            except (KeyError, ValueError):
                continue
            if value <= 0:  # unset: NGP falls back to the next key
                continue
            percent.append(int(round(value)))
            source.append("%s %s" % (where, key))
            break
        else:
            percent.append(100)
            source.append("default")
    return {"percent": percent, "source": source}


def driver_aids(frames):
    """The aids flags the recording shows (the session holds no race options): the VCU state's launch clutch (car
    +0xb60, a CReal, its dirty word +0xb68) runs, and sets the word, whenever AutoGears or ClutchHelp is on; never set
    while driving, both were off. Else the boot's defaults (auto gears off, clutch help on); gear guard and neutral lock
    leave no such mark."""
    driving = frames.words([12])[:, 0] != SETTLE_SUBSTEPS
    dirty = frames.words([HEADER + CONTROL + 0xB68])[driving, 0]
    if len(dirty) and not dirty.any():
        return ["--auto-gears", "off", "--clutch-help", "off"], \
            "aids: auto gears and clutch help off (the launch clutch's car+0xb68 stays 0 while driving)"
    return [], "aids: the boot's defaults (the launch clutch ran, car+0xb68 set: auto gears or clutch help on)"


def resolve_stage(opts, game, standalone, base, env):
    """The stage flags: --stage NAME (a retail map: the variant the weather loads, which rbr-standalone names), else
    --rsf-stage ID for a community stage."""
    if opts.stage:
        return ["--stage", opts.stage]
    data = env.get("RBR_DATA", "")
    names = stage_candidates(data, game["map"])
    if not names:
        return ["--rsf-stage", str(game["map"])]
    r = subprocess.run([standalone] + base + ["--stage", names[0], "--steps", "1"], capture_output=True, text=True,
                       env=env)
    m = re.search(r"the race's weather loads (\S+), not", r.stdout + r.stderr)
    return ["--stage", m.group(1) if m else names[0]]


def parse_report(text):
    """rbr-standalone's headless report lines (headless/report.rs)."""
    out = {"text": text}
    m = re.search(r"(\d+) of (\d+) steps run .*?, (\d+) equal to the recording", text)
    if m:
        out["run"], out["asked"], out["equal"] = int(m.group(1)), int(m.group(2)), int(m.group(3))
    m = re.search(r"differs from the recording after step (\d+): (\d+) words \(([^)]*)\)", text)
    if m:
        out["first_diff"], out["first_diff_words"] = int(m.group(1)), int(m.group(2))
        out["first_diff_names"] = [n.strip() for n in m.group(3).split(",")]
    m = re.search(r"first-difference values: (.*)", text)
    if m:
        out["first_diff_values"] = m.group(1)
    m = re.search(r"words that differ at some step: (\d+)(?: \((.*)\))?", text)
    if m:
        out["differing_words"] = int(m.group(1))
        # The first 16 by their first step: (name, first step, steps it differs).
        out["differing"] = [(w.strip(), int(a), int(n)) for w, a, n in
                            re.findall(r"([^,(]+?) \(from step (\d+), (\d+) steps\)", m.group(2) or "")]
    for key, pattern in (("stopped", r"stopped: (.*)"), ("ended", r"the flow: (.*)"),
                         ("control_diff", r"Game's control differs from the recorded payload at (.*)"),
                         ("result", r"stage result: (.*)")):
        m = re.search(pattern, text)
        if m:
            out[key] = m.group(1)
    return out


def friendly(name):
    """car+0x140 -> car+0x140 (position x), by CAR_NAMES/WHEEL_NAMES."""
    m = re.fullmatch(r"(car|wheel\d)\+0x([0-9a-f]+)", name)
    if not m:
        return name
    off = int(m.group(2), 16)
    table = CAR_NAMES if m.group(1) == "car" else WHEEL_NAMES
    for base, what in table.items():
        if base <= off < base + 16:
            return "%s (%s %s)" % (name, what, "xyzw"[(off - base) // 4])
    return name


def physics(attempt, s, meta, opts, out, work, env):
    games = s.tagged("GAM1")
    if not games:
        return {"error": "no GAM1 record"}, ["No GAM1 record: the stage and car are unknown."]
    game = games[0]
    flags, ngp, rsf_car, notes = race_flags(game, meta, opts, attempt)
    rec = os.path.join(attempt, "physics.bin")
    base = ["--boot", "--recording", rec] + flags
    stage = resolve_stage(opts, game, opts.standalone, base, env)
    trace = os.path.join(work, "ours.bin")
    frames = Frames(rec)
    aids, why = driver_aids(frames)
    base += aids
    notes.append(why)
    command = [opts.standalone] + base + stage + ["--steps", str(frames.n), "--recorded-payload", "--trace-out", trace]
    r = subprocess.run(command, capture_output=True, text=True, env=env)
    rep = parse_report(r.stdout)
    summary = {"command": command, "stage": stage, "ngp": ngp, "rsf_car": rsf_car, "frames": frames.n,
               "exit": r.returncode, "notes": notes}
    summary.update({k: v for k, v in rep.items() if k != "text"})
    lines = ["## Physics", "", "Command: `%s`" % " ".join(command[1:]), ""]
    lines += ["- %s" % n for n in notes]
    if not os.path.exists(trace + ".ext"):
        lines += ["", "rbr-standalone failed (exit %d):" % r.returncode, "", "```", (r.stdout + r.stderr)[-3000:],
                  "```"]
        summary["error"] = "no trace"
        return summary, lines
    ours, ext = Frames(trace), read_ext(trace + ".ext")
    a, b = fields(frames), fields(ours)
    index = {int(p): i for i, p in enumerate(a["post"])}
    rows = np.array([index.get(int(p), -1) for p in b["post"]])
    keep = rows >= 0
    rows, x = rows[keep], b["post"][keep].astype(np.int64)
    div = rep.get("first_diff")
    lines += ["", "- steps run %s of %s frames, **%s equal** to the recording" % (rep.get("run"), frames.n,
                                                                                  rep.get("equal"))]
    if div is not None:
        lines.append("- first differing step **%d**: %d words, first %s" % (
            div, rep["first_diff_words"], ", ".join(friendly(n) for n in rep["first_diff_names"])))
        lines.append("- values there: %s" % rep.get("first_diff_values", "?"))
        lines.append("- words that differ at some step: %s" % rep.get("differing_words"))
        # Words that differ from the first step at every step (a value set once, the state the step reads) hide
        # where the motion parts: the first step another word differs.
        standing = [w for w, a, n in rep.get("differing", []) if a == div and n == rep.get("run")]
        rest = [(a, w) for w, a, n in rep.get("differing", []) if w not in standing]
        if standing:
            summary["standing_words"] = standing
            summary["moving_diff"] = rest[0][0] if rest else None
            summary["moving_diff_names"] = [w for a, w in rest if a == rest[0][0]] if rest else []
            lines.append("- differing at every step from %d: %s; the first step another word differs: **%s** (%s)" % (
                div, ", ".join(friendly(w) for w in standing), rest[0][0] if rest else "none of the first 16",
                ", ".join(friendly(w) for w in summary["moving_diff_names"])))
    else:
        lines.append("- no step differs")
    for key in ("control_diff", "stopped", "ended", "result"):
        if key in rep:
            lines.append("- %s: %s" % (key.replace("_", " "), rep[key]))
    err = np.linalg.norm(b["position"][keep] - a["position"][rows], axis=1)
    summary["position_error_max"] = float(np.nanmax(err)) if len(err) else None
    summary["speed_error_max"] = float(np.nanmax(np.abs(b["speed"][keep] - a["speed"][rows]))) if len(err) else None
    lines += ["- position error max %.4g m, speed error max %.4g m/s (ours - recorded, every step run)" % (
        summary["position_error_max"] or 0, summary["speed_error_max"] or 0), "",
        "Curves: blue recorded, orange ours, red line the first differing step. Engine speed, gear and tyre slip "
        "are recorded by STA1 when the session has it, else ours only (FRM1 holds neither); the settle steps' "
        "recorded state is left out (not yet set).", ""]
    ext_rows = np.searchsorted(ext["post"], x)
    ext_rows = np.minimum(ext_rows, len(ext["post"]) - 1)
    sta, empty = recorded_state(s, x), []
    if sta is not None:
        settle = a["substeps"][rows] == SETTLE_SUBSTEPS
        for key in sta:
            sta[key][settle] = np.nan
        empty = [key for key in sta if np.isnan(sta[key]).all()]
        if empty:
            lines[-1:-1] = ["STA1 holds no %s at any step (its region was not read), so those curves are ours only." %
                            " or ".join(empty)]
            summary["sta1_empty"] = empty
    rec_curve = lambda key: [(sta[key], BLUE)] if sta is not None and key not in empty else []  # noqa: E731
    curves = [("speed", "speed, m/s", [(a["speed"][rows], BLUE), (b["speed"][keep], ORANGE)]),
              ("position-error", "position error, m", [(err, ORANGE)]),
              ("steering", "payload steering", [(a["steering"][rows], BLUE), (b["steering"][keep], ORANGE)]),
              ("engine", "engine speed (EngineNoise p[0], state +0x2c), rad/s",
               rec_curve("engine") + [(ext["engine"][ext_rows], ORANGE)]),
              ("gear", "gear (0 R, 1 N, 2 first)", rec_curve("gear") + [(ext["gear"][ext_rows], ORANGE)])]
    for k in range(WHEELS):
        curves.append(("travel%d" % k, "wheel %d strut travel, m" % k,
                       [(a["travel"][rows, k], BLUE), (b["travel"][keep, k], ORANGE)]))
    for k in range(WHEELS):
        curves.append(("slip%d" % k, "wheel %d tyre |slip ratio|" % k,
                       ([(sta["slip"][:, k], BLUE)] if rec_curve("slip") else []) + [(ext["slip"][ext_rows, k], ORANGE)]))
    for name, title, series in curves:
        png = "physics-%s.png" % name
        ranges = plot(os.path.join(out, png), x, series, mark=div)
        lines += ["**%s** (%s)" % (title, caption(ranges)), "", "![%s](%s)" % (name, png), ""]
    # The ours-only and recorded curves FFB needs, kept for section 3.
    # The FFB torque (+0xaec, the value of the CReal at +0xae0) and its gate (+0x1ec) of both cars, for the force
    # of the recorded car's state.
    torque = [HEADER + CONTROL + 0xAEC, HEADER + CONTROL + 0x1EC]
    rec_t, our_t = frames.words(torque)[rows], ours.words(torque)[keep]
    summary["_ffb"] = {"post": x, "force": ext["force"][ext_rows], "steering": a["steering"][rows], "ngp": ngp,
                       "torque": rec_t[:, 0].view("<f4"), "gate": rec_t[:, 1],
                       "our_torque": our_t[:, 0].view("<f4"), "our_gate": our_t[:, 1],
                       "max_ff_torque": ext["max_ff_torque"][ext_rows],
                       "sensitivity": ffb_sensitivity(meta, game["car"], rsf_car, opts.ffb_sensitivity),
                       "speed": a["speed"][rows],
                       "sideways": a["sideways"][rows], "driving": a["substeps"][rows] != SETTLE_SUBSTEPS}
    summary["_sound"] = {"stage": stage, "flags": flags, "rec": rec, "frames": a, "ngp": ngp}
    if not opts.keep_work:
        for path in (trace,):
            os.remove(path)
    return summary, lines


# --- 2. input ----------------------------------------------------------------------------------------------------

def device_axes(s):
    """Per device: its name and axes {ofs: (label, (min, max) or None)} from DEV1, FMT1 and PRP1 ranges."""
    devices = {d["device"]: {"name": d["name"], "axes": {}} for d in s.tagged("DEV1")}
    for f in s.tagged("FMT1"):
        dev = devices.setdefault(f["device"], {"name": "?", "axes": {}})
        dev["size"] = f["size"]
        for o in f["objects"]:
            if o["type"] & 3:  # DIDFT_RELAXIS | DIDFT_ABSAXIS; the instance in bits 8..24 (0xffff: any)
                dev["axes"][o["ofs"]] = ["axis@%d" % o["ofs"], None, (o["type"] >> 8) & 0xFFFF]
    for p in s.tagged("PRP1"):
        dev = devices.get(p["device"])
        if dev is None or p["property"] != DIPROP_RANGE or p["hresult"] & 0x80000000 or len(p["data"]) < 8:
            continue
        lo, hi = struct.unpack_from("<ii", p["data"])
        targets = dev["axes"].keys() if p["how"] == 0 else [p["obj"]] if p["how"] == 1 else []
        for ofs in targets:
            if ofs in dev["axes"]:
                dev["axes"][ofs][1] = (lo, hi)
    return devices


def axis_values(s, devices, pres, clock):
    """Each device axis's raw value for the frames starting at `pres`: the last POL1 state, with BUF1 events applied
    after it, read after the StepForward that ended at the frame's pre step (the frame's payload is the next poll's)."""
    out = {}
    for n, dev in devices.items():
        events = []  # (qpc, ofs, value) from both kinds, in order
        for r in s.records:
            if r["tag"] not in ("POL1", "BUF1") or r["device"] != n or r["hresult"] & 0x80000000:
                continue
            if r["tag"] == "POL1":
                st = r["state"]
                for ofs in dev["axes"]:
                    if ofs + 4 <= len(st):
                        events.append((r["qpc"], ofs, struct.unpack_from("<i", st, ofs)[0]))
            else:
                for ofs, data, _, _, _ in r["events"]:
                    if ofs in dev["axes"]:
                        events.append((r["qpc"], ofs, struct.unpack("<i", struct.pack("<I", data))[0]))
        for ofs in dev["axes"]:
            ev = [(q, v) for q, o, v in events if o == ofs]
            if ev:
                q, v = zip(*ev)
                out[(n, ofs)] = held(steps_at(q, clock), v, pres)
    return out


DEFAULT_RANGE = (0, 65535)
ACM_ACTIONS = {0: "steering", 3: "throttle", 5: "brake", 6: "handbrake", 11: "clutch"}  # action_map::ACTIONS
INPUT_KINDS = ("POL1", "BUF1", "DeviceIoControl", "GetRawInputData", "XInputGetState", "ReadFile")


def read_acm(path):
    """A controller map (`rbr_formats::saves::action_map`): per binding its action, device instance GUID, object kind
    ("axis", "button", "pov") and instance."""
    with open(path, "rb") as f:
        r = rss.Reader(f.read())
    if r.bytes(4) != b"PAMA" or r.u32() != 0x300:
        raise ValueError("%s: not a 0x300 PAMA map" % path)
    out = []
    for _ in range(r.u32()):
        devices, bindings = r.u32(2)
        guids = [r.guid() for _ in range(devices)]
        for _ in range(bindings):
            device, obj, action = r.u32(3)
            kind = "axis" if obj & 3 else "button" if obj & 0xC else "pov" if obj & 0x10 else "other"
            out.append({"action": action, "device": guids[device] if device < len(guids) else None, "kind": kind,
                        "instance": (obj >> 8) & 0xFFFF})
    return out


def find_acm(session_dir, attempt):
    """The attempt's controller map: the copy the game opened (files/savedgames), else the session's config/."""
    for d in (os.path.join(attempt, "files", "savedgames"), os.path.join(session_dir, "config")):
        if os.path.isdir(d):
            for f in sorted(os.listdir(d)):
                if f.lower().endswith(".acm"):
                    return os.path.join(d, f)
    return None


def tied_polls(s, devices):
    """Per CTL1 (the payload the car's control input took): its step, its words, and each device axis's raw value
    from the device's last POL1 before it."""
    ctl = s.tagged("CTL1")
    qc = np.array([c["qpc"] for c in ctl], dtype=np.int64)
    words = np.array([struct.unpack("<8I", c["payload"]) for c in ctl], dtype=np.uint32).reshape(-1, 8)
    values = {}
    for n, dev in devices.items():
        pols = [r for r in s.tagged("POL1") if r["device"] == n and not r["hresult"] & 0x80000000]
        if not pols or not dev["axes"]:
            continue
        qp = np.array([r["qpc"] for r in pols], dtype=np.int64)
        i = np.searchsorted(qp, qc, side="left") - 1
        for ofs in dev["axes"]:
            v = np.array([struct.unpack_from("<i", r["state"], ofs)[0] if ofs + 4 <= len(r["state"]) else 0
                          for r in pols], dtype=np.float64)
            values[(n, ofs)] = np.where(i >= 0, v[np.maximum(i, 0)], np.nan)
    return np.array([c["step"] for c in ctl], dtype=np.int64), words, values


def callers(s):
    """Who fed the payload: the CTL1 threads, the POL1 threads per device, and the last MOD1's input calls by
    module."""
    out = {"ctl1_threads": {}, "pol1_threads": {}, "modules": {}}
    for c in s.tagged("CTL1"):
        out["ctl1_threads"][c["thread"]] = out["ctl1_threads"].get(c["thread"], 0) + 1
    for r in s.tagged("POL1"):
        key = "%d@%d" % (r["device"], r["thread"])
        out["pol1_threads"][key] = out["pol1_threads"].get(key, 0) + 1
    mods = s.tagged("MOD1")
    for m in mods[-1]["modules"] if mods else []:
        calls = {k: v for k, v in m["calls"].items() if v and k in INPUT_KINDS + ("EFF1",)}
        if calls:
            out["modules"][m["name"]] = calls
    return out


def mapping(v, y, rng):
    """The payload `y` as a function of the raw axis `v`: linear and cubic fits over the range-normalised axis, their
    residuals, and the axis span over which the payload rests."""
    ok = np.isfinite(v) & np.isfinite(y)
    lo, hi = rng if rng else (np.nanmin(v[ok]), np.nanmax(v[ok]))
    norm = (v - lo) / max(hi - lo, 1e-12)
    coef = np.polyfit(norm[ok], y[ok], 1)
    e = np.where(ok, np.polyval(coef, norm) - y, np.nan)
    cubic = np.polyfit(norm[ok], y[ok], 3)
    rest = float(np.median(y[ok][np.abs(y[ok]) == np.min(np.abs(y[ok]))]))
    flat = norm[ok][y[ok] == rest]
    return {"correlation": float(np.corrcoef(v[ok], y[ok])[0, 1]), "fit": coef.tolist(),
            "max_error": float(np.nanmax(np.abs(e))), "rms_error": float(np.sqrt(np.nanmean(e ** 2))),
            "cubic": cubic.tolist(),
            "cubic_rms_error": float(np.sqrt(np.mean((np.polyval(cubic, norm[ok]) - y[ok]) ** 2))),
            "rest": rest, "rest_band": [float(flat.min()), float(flat.max())] if len(flat) else None,
            "_line": np.polyval(coef, norm), "_off": np.abs(np.nan_to_num(e)) > 1 / 255}


def input_chain(s, frames, out, session_dir, attempt):
    lines = ["## Steering input", "",
             "Expected exact once the poll is ported: the device poll that turns raw axes into channels "
             "(`USInputManagerImpl::Tic` 0x4cbf50 under NGP's 1000 Hz input thread, with RBRControls' processing) is "
             "not, so `rbr_gameplay::filter`, `control` and `control::ngp_direct_gears` are not run. For now each "
             "payload channel against its raw axis: a linear fit (a x + b over the range-normalised axis), its "
             "residual, a cubic fit's residual (curvature), and the axis span over which the payload rests (a dead "
             "zone).", ""]
    devices = device_axes(s)
    summary = {"devices": {str(n): {"name": d["name"], "axes": len(d["axes"])} for n, d in devices.items()},
               "channels": {}, "callers": callers(s)}
    c = summary["callers"]
    lines += ["Devices: " + (", ".join("%d %r (%d axes)" % (n, d["name"], len(d["axes"]))
                                       for n, d in sorted(devices.items()) if d["axes"]) or "none"), "",
              "Who fed the payload: CTL1 (the car's control input) on threads %s; POL1 (device@thread) %s; input "
              "calls by module (last MOD1): %s." % (
                  c["ctl1_threads"] or "none", c["pol1_threads"] or "none",
                  "; ".join("%s %s" % (m, ", ".join("%s %d" % kv for kv in k.items()))
                            for m, k in c["modules"].items()) or "none"), ""]
    if s.tagged("CTL1"):
        steps, words, values = tied_polls(s, devices)
        ok = steps != 0xFFFFFFFF
        steps, words = steps[ok], words[ok]
        values = {k: v[ok] for k, v in values.items()}
        tie = "each CTL1 payload against each device's last POL1 before it"
        # The CTL1 payload is FRM1's for the frame starting at its step.
        frame = {int(p): k for k, p in enumerate(frames["pre"])}
        rows = np.array([frame.get(int(t), -1) for t in steps])
        same = (rows >= 0) & np.all(words == frames["control"][np.maximum(rows, 0)], axis=1)
        summary["ctl1_equals_frm1"] = [int(same.sum()), int((rows >= 0).sum())]
        lines.append("CTL1 payloads equal to FRM1's for the same step: %d of %d." % tuple(summary["ctl1_equals_frm1"]))
    else:
        ok = frames["substeps"] != SETTLE_SUBSTEPS
        steps, words = frames["post"].astype(np.int64)[ok], frames["control"][ok]
        values = {k: v[ok] for k, v in axis_values(s, devices, frames["pre"].astype(np.int64), step_clock(s)).items()}
        tie = "each FRM1 payload against the last POL1 after the step before (no CTL1 in this session)"
    acm = find_acm(session_dir, attempt)
    bound = {}
    if acm:
        # The device numbers with axes, by instance GUID (a device's DEV1 repeats; the one with FMT1 axes reads).
        instances = {r["instance"]: r["device"] for r in s.tagged("DEV1") if devices[r["device"]]["axes"]}
        for b in read_acm(acm):
            name = ACM_ACTIONS.get(b["action"])
            if name and b["kind"] == "axis":
                # The axis instance's offset in the device's data format (FMT1's object of that instance).
                n = instances.get(b["device"])
                ofs = next((o for o, a in devices[n]["axes"].items() if a[2] == b["instance"]), None) \
                    if n is not None else None
                bound[name] = (n, ofs, b["instance"], b["device"])
        lines.append("Bindings from %s: %s." % (os.path.relpath(acm, session_dir), ", ".join(
            "%s axis instance %d of %s (%s)" % (k, inst, guid, "device %d axis@%d" % (n, ofs) if ofs is not None else
                                                "no FMT1 axis of that device and instance")
            for k, (n, ofs, inst, guid) in bound.items()) or "no axis bindings"))
    summary["tie"] = tie
    lines += ["Tie: %s. Without a PRP1 range an axis is taken as 0..65535 (a 16-bit HID axis's DirectInput default)." % tie, ""]
    if not values:
        lines.append("No raw axis values (no POL1 with an FMT1 axis).")
        return summary, lines
    lines += ["| channel | axis | bound | correlation | linear a, b | max error | RMS error | cubic RMS | rest band | "
              "first step off by > 1/255 |", "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
    for k, name in enumerate(CHANNELS):
        y = words[:, k].view("<f4").astype(np.float64)
        best = None
        for key, v in values.items():
            okv = np.isfinite(v)
            if okv.sum() < 10 or np.std(v[okv]) == 0 or np.std(y[okv]) == 0:
                continue
            r = abs(float(np.corrcoef(v[okv], y[okv])[0, 1]))
            if best is None or r > best[1]:
                best = (key, r)
        b = bound.get(name)
        key = (b[0], b[1]) if b and (b[0], b[1]) in values else best[0] if best else None
        if key is None:
            lines.append("| %s | none varies with it | | | | | | | | |" % name)
            continue
        n, ofs = key
        rng = devices[n]["axes"][ofs][1]
        v = values[key]
        seen = [int(np.nanmin(v)), int(np.nanmax(v))] if np.isfinite(v).any() else None
        m = mapping(v, y, rng or DEFAULT_RANGE)
        off = np.nonzero(m.pop("_off"))[0]
        line = m.pop("_line")
        m.update({"device": n, "ofs": ofs, "range": rng, "seen": seen, "bound": b is not None and key == (b[0], b[1]),
                  "best_correlated": None if best is None else list(best[0]),
                  "first_off": int(steps[off[0]]) if len(off) else None})
        summary["channels"][name] = m
        band = "%.4f to %.4f at %g" % (m["rest_band"][0], m["rest_band"][1], m["rest"]) if m["rest_band"] else "-"
        lines.append("| %s | device %d %s, range %s%s | %s | %.4f | %.4g, %.4g | %.4g | %.4g | %.4g | %s | %s |" % (
            name, n, devices[n]["axes"][ofs][0], rng or "0..65535 assumed, seen %s..%s" % tuple(seen),
            "" if best is None or best[0] == key else " (best correlated: device %d axis@%d)" % best[0],
            "yes" if m["bound"] else "no", m["correlation"], m["fit"][0], m["fit"][1], m["max_error"],
            m["rms_error"], m["cubic_rms_error"], band, m["first_off"]))
        png = "input-%s.png" % name
        ranges = plot(os.path.join(out, png), steps, [(y, BLUE), (line, ORANGE)])
        lines += ["", "**%s**: payload blue, the linear fit of the axis orange (%s)" % (name, caption(ranges)), "",
                  "![%s](%s)" % (name, png), ""]
        lines.append("")
    return summary, lines


# --- 3. FFB ------------------------------------------------------------------------------------------------------

def car_force(torque, gate, max_ff_torque, ngp=True):
    """steering_ffb::force from the car's words in f32: NGP's curve sign(T) (1 - 0.975^(|T| 100 / S)), the power in
    double (ngp::ffb::curve), 1.02's FfbNorm T / S clamped to [-1, 1]; 0 while the gate (+0x1ec) is set."""
    t = np.asarray(torque, dtype=np.float32)
    s = np.asarray(max_ff_torque, dtype=np.float32)
    with np.errstate(all="ignore"):
        if ngp:
            exponent = np.abs(t) * (np.float32(100.0) / s)
            p = np.power(np.float64(np.float32(0.975)), exponent.astype(np.float64)).astype(np.float32)
            f = np.sign(t).astype(np.float32) * (np.float32(1.0) - p)
        else:
            f = t / s
            f = np.where(f < -1, np.float32(-1), np.where(f > 1, np.float32(1), f))
    return np.where(np.asarray(gate) != 0, np.float32(0.0), f).astype(np.float32)


def magnitude(force, strength=1.0, inverse=False, ngp=True):
    """rbr_gameplay::ffb::magnitude in f32: x = force * strength (negated when inverse), x * -10000 floored under NGP
    (truncated in 1.02), clamped to 10000; NaN gives 0."""
    f = np.asarray(force, dtype=np.float32)
    x = f * np.float32(strength)
    if inverse:
        x = -x
    scaled = (x * np.float32(-10000.0)).astype(np.float64)
    m = np.floor(scaled) if ngp else np.trunc(scaled)
    m = np.clip(np.nan_to_num(m, nan=0.0), -10000, 10000).astype(np.int64)
    return np.where(np.isnan(f), 0, m)


def dead_zone(m, steering, dead_zone=0.0, threshold=0.0):
    """rbr_gameplay::ffb::dead_zone: 0 where |steering| < dead_zone and |m| < int(threshold * 10000)."""
    t = int(np.float32(threshold) * np.float32(10000))
    return np.where((np.abs(steering) < dead_zone) & (np.abs(m) < t), 0, m)


def recorded_ffb(s):
    """The constant-force SetParameters: (QPC, signed magnitude) and the directions seen."""
    constant = {e["effect"] for e in s.tagged("EFF1") if e["op"] == 1 and e["type"] == CONSTANT_FORCE}
    out, directions = [], set()
    for e in s.tagged("EFF1"):
        p = e["params"]
        if e["op"] != 2 or e["effect"] not in constant or p is None or len(p["specific"]) < 4 or \
                e["hresult"] & 0x80000000:
            continue
        m = struct.unpack_from("<i", p["specific"])[0]
        d = p["directions"][0] if p["directions"] else 0
        directions.add(d)
        out.append((e["qpc"], -m if d < 0 else m))
    return out, sorted(directions)


def lag_of(a, b, most):
    """The shift k (steps) maximising the correlation of a[t] with b[t + k], |k| <= most."""
    a, b = a - a.mean(), b - b.mean()
    n = len(a)
    size = 1 << int(np.ceil(np.log2(2 * n)))
    c = np.fft.irfft(np.conj(np.fft.rfft(a, size)) * np.fft.rfft(b, size), size)
    ks = np.arange(-most, most + 1)
    return int(ks[np.argmax(c[ks % size])])


REACH = 2  # steps either side of the STP1 association a recorded FFB set may come from


def exact_ffb(set_steps, set_values, posts, ours, reach=REACH):
    """Each recorded magnitude against ours at the last step whose StepForward ended before it, or up to `reach`
    steps either side (NGP's 1000 Hz FFB thread reads the force EmitForce stored, which STP1's QPC brackets only
    to a couple of steps): per value, whether ours was run there (`compared`), whether one of those equals it
    (`match`), the offset of the nearest that does (+ an earlier step; 0 where none), and ours at the step and the
    step before (NaN where not run)."""
    pos = np.searchsorted(posts, set_steps)
    last = len(posts) - 1
    here = np.where(pos <= last, posts[np.minimum(pos, last)], -1) == set_steps
    at = np.where(here, ours[np.minimum(pos, last)], np.nan)
    before = np.where(here & (pos > 0), ours[np.maximum(pos - 1, 0)], np.nan)
    match = np.zeros(len(pos), dtype=bool)
    offset = np.zeros(len(pos), dtype=np.int64)
    for k in sorted(range(-reach, reach + 1), key=lambda k: (abs(k), -k)):
        rows = pos - k
        valid = here & (rows >= 0) & (rows <= last)
        hit = valid & ~match & (ours[np.clip(rows, 0, last)] == set_values)
        offset[hit] = k
        match |= hit
    return here, match, offset, at, before


def ffb(s, meta, phys, out):
    lines = ["## Force feedback", ""]
    data = phys.get("_ffb")
    if data is None:
        return {"error": "no physics trace"}, lines + ["No trace of ours (the physics section did not run)."]
    sets, directions = recorded_ffb(s)
    ngp_ini = meta.get("ngp", {})

    def number(key, default):
        try:
            return float(ngp_ini[key.lower()])
        except (KeyError, ValueError):
            return default

    dz, thr = number("ffbDeadZone", 0.0), number("ffbDeadZoneThreshold", 0.0)
    ours = dead_zone(magnitude(data["force"], ngp=data["ngp"]), data["steering"], dz, thr).astype(np.float64)
    sens = data["sensitivity"]
    summary = {"sets": len(sets), "directions": directions, "dead_zone": dz, "dead_zone_threshold": thr,
               "sensitivity": sens}
    lines += ["Ours: `rbr_gameplay::ffb::magnitude` (strength 1, not inverse: the game's options are not in the "
              "session) and `dead_zone` (`ffbDeadZone` %g, `ffbDeadZoneThreshold` %g from meta.txt's [NGP]) of "
              "`steering_ffb::force` after each step; sensitivity %s (%s). Recorded: each constant-force "
              "SetParameters' magnitude (negated for a negative direction; directions seen %s), held from the step "
              "of the StepForward before it to the next." % (dz, thr, sens["percent"], "; ".join(sens["source"]),
                                                             directions), ""]
    if not sets:
        lines.append("No constant-force SetParameters in the session.")
        return summary, lines
    clock = step_clock(s)
    q, m = zip(*sets)
    set_steps, values = steps_at(q, clock), np.array(m, dtype=np.float64)
    # A set after the last StepForward has no step of its own: the race has ended and the game's gate zeroes it.
    after = np.asarray(q, dtype=np.int64) > clock[0][-1]
    compared, match, offset, at, before = exact_ffb(set_steps, values, data["post"].astype(np.int64), ours)
    compared &= ~after
    n, good = int(compared.sum()), int((match & compared).sum())
    bad = np.nonzero(compared & ~match)[0]
    summary["exact"] = {"compared": n, "matched": good, "fraction": good / n if n else None,
                        "not_run": int((~compared).sum()), "after_the_last_step": int(after.sum()),
                        "pass": n > 0 and good == n,
                        "mismatches": [{"qpc": int(q[i]), "step": int(set_steps[i]), "recorded": int(values[i]),
                                        "ours": None if np.isnan(at[i]) else int(at[i]),
                                        "ours_before": None if np.isnan(before[i]) else int(before[i])}
                                       for i in bad[:200]]}
    lines += ["**Exact check** (expected exact per value): each recorded magnitude against ours at the last step "
              "whose StepForward ended before it, or up to %d steps either side. %d of %d match (%s); %d sets fall "
              "outside our run, %d of them after the last StepForward (the race over)." % (
                  REACH, good, n, "%.4f" % (good / n) if n else "-", int((~compared).sum()), int(after.sum())), ""]
    # The mapping apart from the physics: the force of the recorded car's torque and gate, with our S. The formula
    # is checked first against our own trace's force.
    own = car_force(data["our_torque"], data["our_gate"], data["max_ff_torque"], data["ngp"])
    same = int((own.view("<u4") == np.asarray(data["force"], dtype=np.float32).view("<u4")).sum())
    from_rec = dead_zone(magnitude(car_force(data["torque"], data["gate"], data["max_ff_torque"], data["ngp"]),
                                   ngp=data["ngp"]), data["steering"], dz, thr).astype(np.float64)
    compared_r, match_r, offset_r, at_r, before_r = exact_ffb(set_steps, values, data["post"].astype(np.int64),
                                                              from_rec)
    compared_r &= ~after
    offsets = {int(k): int(((offset_r == k) & match_r & compared_r).sum()) for k in range(-REACH, REACH + 1)}
    n_r, good_r = int(compared_r.sum()), int((match_r & compared_r).sum())
    bad_r = np.nonzero(compared_r & ~match_r)[0]
    s_values = np.unique(data["max_ff_torque"])
    summary["exact_recorded_car"] = {
        "compared": n_r, "matched": good_r, "fraction": good_r / n_r if n_r else None,
        "pass": n_r > 0 and good_r == n_r, "max_ff_torque": [float(v) for v in s_values[:4]], "offsets": offsets,
        "formula_reproduces_trace": [same, len(own)],
        "mismatches": [{"step": int(set_steps[i]), "recorded": int(values[i]),
                        "ours": None if np.isnan(at_r[i]) else int(at_r[i]),
                        "ours_before": None if np.isnan(before_r[i]) else int(before_r[i])} for i in bad_r[:200]]}
    lines += ["**From the recorded car** (the mapping alone): the same check with the force of the recorded car's FFB "
              "torque (car+0xaec) and gate (car+0x1ec) through our `MaxFFTorque` %s (the formula gives our trace's "
              "force bit for bit at %d of %d steps). %d of %d match (%s)." % (
                  ", ".join("%g" % v for v in s_values[:4]), same, len(own), good_r, n_r,
                  "%.4f" % (good_r / n_r) if n_r else "-"),
              "", "Matched sets by the step they equal (+ earlier than the STP1 association): %s." % ", ".join(
                  "%+d: %d" % kv for kv in offsets.items()), ""]
    if len(bad_r):
        lines += ["| set | step | recorded | from the recorded car at step | step before |",
                  "| --- | --- | --- | --- | --- |"]
        lines += ["| %d | %d | %d | %s | %s |" % (i, set_steps[i], values[i], at_r[i], before_r[i])
                  for i in bad_r[:10]]
        lines.append("")
    if len(bad):
        lines += ["First mismatches (%d in all; summary.json keeps 200):" % len(bad), "",
                  "| set | step | recorded | ours at step | ours step before |", "| --- | --- | --- | --- | --- |"]
        lines += ["| %d | %d | %d | %s | %s |" % (i, set_steps[i], values[i], at[i], before[i]) for i in bad[:20]]
        lines.append("")
    lines += ["Secondary, held per step (ours from our run):", ""]
    rec = held(set_steps, m, data["post"])
    ok = np.isfinite(rec)
    x = data["post"]
    div = phys.get("first_diff")
    windows = [("every step", ok)]
    if div is not None:
        windows.append(("before the divergence", ok & (x < div)))
    lines += ["| steps | n | correlation | RMS error | lag (steps, + ours later) |", "| --- | --- | --- | --- | --- |"]
    for name, w in windows:
        if w.sum() < 2:
            continue
        a, b = rec[w], ours[w]
        r = float(np.corrcoef(a, b)[0, 1]) if np.std(a) > 0 and np.std(b) > 0 else float("nan")
        rms = float(np.sqrt(np.mean((a - b) ** 2)))
        lag = lag_of(a, b, min(2000, int(w.sum()) - 1))
        summary[name] = {"n": int(w.sum()), "correlation": r, "rms": rms, "lag": lag}
        lines.append("| %s | %d | %.4f | %.1f | %d |" % (name, w.sum(), r, rms, lag))
    ranges = plot(os.path.join(out, "ffb.png"), x, [(rec, BLUE), (ours, ORANGE)], mark=div)
    dt = STEP_NGP if data["ngp"] else STEP_102
    lines += ["", "Magnitude over the drive, recorded blue, ours orange, red the first physics difference (%s; a step "
              "is %.4g s)" % (caption(ranges), dt), "", "![ffb](ffb.png)", ""]
    summary["sections"] = []
    for name, why, at in hard_sections(data, dt):
        w = np.zeros(len(x), dtype=bool)
        half = int(1.5 / dt)
        w[max(0, at - half):at + half] = True
        w &= ok
        if w.sum() < 4:
            continue
        a, b = rec[w], ours[w]
        lag = lag_of(a, b, min(int(0.5 / dt), int(w.sum()) - 1))
        exact_w = compared & (set_steps >= x[w][0]) & (set_steps <= x[w][-1])
        matched = int((match & exact_w).sum())
        png = "ffb-%s.png" % name
        ranges = plot(os.path.join(out, png), x[w], [(a, BLUE), (b, ORANGE)], mark=div, height=240)
        summary["sections"].append({"name": name, "step": int(x[at]), "lag": lag, "matched": matched,
                                    "compared": int(exact_w.sum()),
                                    "rms": float(np.sqrt(np.mean((a - b) ** 2)))})
        lines += ["**%s** at step %d (%s): lag %d steps (%.0f ms, + ours later), %d of %d sets exact, RMS %.0f (%s)"
                  % (name, x[at], why, lag, lag * dt * 1000, matched, exact_w.sum(),
                     summary["sections"][-1]["rms"], caption(ranges)), "", "![%s](%s)" % (name, png), ""]
    return summary, lines


def hard_sections(data, dt):
    """3 s windows where the wheel's feel is hardest, by the recorded car: the start (driving's first steps), the
    biggest impact (the largest speed drop over 0.1 s) and the biggest slide (the largest sideways speed). Each:
    (name, why, row of its centre)."""
    driving = np.nonzero(data["driving"])[0]
    if not len(driving):
        return []
    start = driving[0]
    out = [("start", "the first driving step", min(start + int(1.5 / dt), len(data["post"]) - 1))]
    k = max(1, int(0.1 / dt))
    speed = data["speed"]
    if len(speed) > k:
        drop = np.where(data["driving"][k:], speed[:-k] - speed[k:], -np.inf)
        i = int(np.argmax(drop))
        out.append(("impact", "speed fell %.1f m/s in 0.1 s" % drop[i], i + k))
    side = np.where(data["driving"], data["sideways"], -np.inf)
    i = int(np.argmax(side))
    out.append(("slide", "sideways %.1f m/s at %.1f m/s" % (side[i], speed[i]), i))
    return out


# --- 4. sound ----------------------------------------------------------------------------------------------------

def envelope(x, rate, hz=100):
    """RMS per 1/hz s of a mono signal."""
    n = int(rate // hz)
    m = len(x) // n
    return np.sqrt(np.mean(x[:m * n].reshape(m, n) ** 2, axis=1))


def align(ours, recorded, hz, coarse, most):
    """The offset (s) of ours' start in the recording: the coarse one moved by the envelopes' best match within
    +-most s (log envelopes, normalised)."""
    a = np.log10(ours + 1e-5)
    b = np.log10(recorded + 1e-5)
    a, b = (a - a.mean()) / (a.std() or 1), (b - b.mean()) / (b.std() or 1)
    start = int(round(coarse * hz))
    span = int(most * hz)
    best, score = 0, -np.inf
    for k in range(-span, span + 1):
        lo = start + k
        i0, j0 = max(0, -lo), max(0, lo)
        n = min(len(a) - i0, len(b) - j0)
        if n < hz:
            continue
        c = float(np.dot(a[i0:i0 + n], b[j0:j0 + n]) / n)
        if c > score:
            best, score = k, c
    return (start + best) / hz, score


def loudness(x, rate):
    """RMS dBFS per whole second."""
    n = len(x) // rate
    return 20 * np.log10(np.sqrt(np.mean(x[:n * rate].reshape(n, rate) ** 2, axis=1)) + 1e-9)


def spectrogram(path, x, rate, width=1000, height=256, top=11025.0):
    nfft = 2048
    hop = max(1, (len(x) - nfft) // width)
    cols = []
    window = np.hanning(nfft)
    for i in range(min(width, max(0, (len(x) - nfft) // hop + 1))):
        cols.append(np.abs(np.fft.rfft(x[i * hop:i * hop + nfft] * window)) * 2 / window.sum())
    if not cols:
        write_png(path, np.zeros((height, width, 3), np.uint8))
        return
    s = 20 * np.log10(np.array(cols).T + 1e-9)
    freqs = np.fft.rfftfreq(nfft, 1 / rate)
    rows = np.searchsorted(freqs, np.linspace(top, 0, height))
    img = s[np.minimum(rows, len(freqs) - 1)]
    write_png(path, colour_map((img + 120) / 110))


def settle_frames(frames):
    """The leading load-settle frames (StepForward(40)): 4 in 1.02, more under NGP's step."""
    sub = np.asarray(frames["substeps"])
    return int(np.argmax(sub != SETTLE_SUBSTEPS)) if (sub != SETTLE_SUBSTEPS).any() else len(sub)


def resampled(frames, ngp, path):
    """An FRM1 for rbr-sound, which feeds one payload per 7 ms frame: four settle frames, then the driving frames,
    under NGP's 1/720 s step the one at each 7 ms boundary."""
    raw = frames.raw
    substeps = frames.words([12])[:, 0]
    settle = settle_frames({"substeps": substeps})
    picks = np.arange(settle, frames.n)
    if ngp:
        t = np.arange(0, (frames.n - settle) * STEP_NGP, STEP_102)
        picks = settle + np.minimum(np.round(t / STEP_NGP).astype(int), frames.n - settle - 1)
    with open(path, "wb") as f:
        # rbr-sound skips its first four frames (SETTLE_FRAMES), which its live boot settles by itself.
        for i in list(range(min(4, settle))) + list(picks):
            f.write(raw[i].tobytes())
    return len(picks)


def sound(session_dir, s, meta, phys, opts, out, work, env):
    lines = ["## Sound", ""]
    data = phys.get("_sound")
    wav = os.path.join(session_dir, "audio.wav")
    if data is None or not os.path.exists(wav):
        return {"error": "no physics run or no audio.wav"}, lines + ["Not run: needs the physics section and audio.wav."]
    if data["stage"][0] != "--stage":
        return {"error": "community stage"}, lines + [
            "Not run: rbr-sound plays retail stages only (its stage sounds come from the 1.02 maps), map %s is an RSF "
            "stage." % data["stage"][1]]
    audio = read_keys(os.path.join(session_dir, "audio.txt"))[""]
    game = s.tagged("GAM1")[0]
    rec = os.path.join(work, "sound.bin")
    driven = resampled(Frames(data["rec"]), data["ngp"], rec)
    with open(os.path.join(work, "sound.auto"), "w") as f:
        f.write("%d %d %d %d\n" % (game["map"], game["car"], game["weather"], game["tyre"]))
    ours_wav = os.path.join(work, "ours.wav")
    flags, i = [], 0
    while i < len(data["flags"]):  # rbr-sound takes --ngp-physics and --rsf-car of the boot's flags
        if data["flags"][i] in ("--ngp-physics", "--rsf-car"):
            flags += data["flags"][i:i + 2]
        i += 2 if data["flags"][i].startswith("--") else 1
    stage = data["stage"][1] if data["stage"][0] == "--stage" else None
    command = [opts.sound, "--recording", rec, "--out", ours_wav, "--seconds", str(driven * STEP_102 + 1),
               "--camera", opts.camera, "--standalone", opts.standalone] + (["--stage", stage] if stage else []) + flags
    r = subprocess.run(command, capture_output=True, text=True, env=env)
    summary = {"command": command, "exit": r.returncode, "camera": opts.camera}
    lines += ["Command: `%s`" % " ".join(command[1:]), "",
              "Approximate only, a rough baseline of how it should sound: no check. Ours: rbr-sound's %s camera (%s), the payloads %s. Recorded: the game's whole PipeWire output, so it "
              "also holds the camera the player used, its menus, the co-driver and every sound the game played; "
              "only the car and stage sounds are ours to match." % (
                  opts.camera, {"chase": "6 m behind and 2 m above the car", "inside": "in the car",
                                "trackside": "trackside cameras"}[opts.camera],
                  "picked at each 7 ms from NGP's 1/720 s steps and rounded to the replay's steps by the live "
                  "boot" if data["ngp"] else "as recorded"), ""]
    if r.returncode != 0 or not os.path.exists(ours_wav):
        summary["error"] = "rbr-sound failed"
        return summary, lines + ["rbr-sound failed:", "", "```", r.stderr[-3000:], "```"]
    a, rate = read_wav(ours_wav)
    a = a.mean(axis=1)
    clock = step_clock(s)
    to_unix = unix_of_qpc(s)
    frames = data["frames"]
    # Ours starts after the settle frames, with the countdown, whose frames step nothing: at the last settle step.
    first = int(frames["post"][max(0, min(settle_frames(frames) - 1, len(frames["post"]) - 1))])
    coarse = None
    if to_unix is not None and "start_unix_ns" in audio and len(clock[0]):
        q = clock[0][min(np.searchsorted(clock[1], first), len(clock[0]) - 1)]
        coarse = float(to_unix(q)) - int(audio["start_unix_ns"]) / 1e9
    # The recording's part around ours (the whole file without a clock), resampled to our rate.
    margin = 5.0
    start = max(0.0, coarse - margin) if coarse is not None else 0.0
    b, rate_b = read_wav(wav, start, len(a) / rate + 2 * margin if coarse is not None else None)
    b = b.mean(axis=1)
    if rate_b != rate:
        t = np.arange(int(len(b) * rate / rate_b)) / rate
        b = np.interp(t, np.arange(len(b)) / rate_b, b)
    hz = 100
    ea, eb = envelope(a, rate, hz), envelope(b, rate, hz)
    offset, score = align(ea, eb, hz, coarse - start if coarse is not None else 0.0,
                          margin if coarse is not None else len(eb) / hz)
    summary.update({"coarse": coarse, "offset": offset + start, "match": score})
    shift = int(round(offset * rate))
    lo = max(0, -shift)
    n = min(len(a) - lo, len(b) - shift - lo)
    if n < rate:
        summary["error"] = "no overlap"
        return summary, lines + ["No second of overlap after the alignment (offset %.3f s)." % offset]
    a, b = a[lo:lo + n], b[shift + lo:shift + lo + n]
    la, lb = loudness(a, rate), loudness(b, rate)
    summary.update({"seconds": len(la), "loudness_ours": la.tolist(), "loudness_recorded": lb.tolist(),
                    "loudness_diff_mean": float(np.mean(la - lb))})
    lines += ["- alignment in audio.wav: coarse %s s (CLK1, audio.txt), refined %.3f s (envelope match %.3f)" % (
        "%.3f" % coarse if coarse is not None else "unknown", offset + start, score),
        "- %d s side by side; loudness ours - recorded: mean %.1f dB" % (len(la), summary["loudness_diff_mean"]), ""]
    ranges = plot(os.path.join(out, "sound-loudness.png"), np.arange(len(la)), [(lb, BLUE), (la, ORANGE)])
    lines += ["**loudness, dBFS per second**: recorded blue, ours orange (%s)" % caption(ranges, "second"), "",
              "![loudness](sound-loudness.png)", ""]
    spectrogram(os.path.join(out, "spectrogram-ours.png"), a, rate)
    spectrogram(os.path.join(out, "spectrogram-recorded.png"), b, rate)
    lines += ["Spectrograms, 0 to 11 kHz, -120 to -10 dB, %d s:" % len(la), "", "| ours | recorded |", "| --- | --- |",
              "| ![ours](spectrogram-ours.png) | ![recorded](spectrogram-recorded.png) |", ""]
    return summary, lines


# --- the session -------------------------------------------------------------------------------------------------

def attempts(session_dir):
    return sorted(int(d) for d in os.listdir(session_dir)
                  if d.isdigit() and os.path.exists(os.path.join(session_dir, d, "physics.bin")))


def compare(session_dir, n, opts, env):
    attempt = os.path.join(session_dir, str(n))
    out = os.path.join(opts.out, str(n))
    work = os.path.join(out, "work")
    os.makedirs(work, exist_ok=True)
    meta = read_meta(os.path.join(session_dir, "meta.txt"))
    s = rss.read(os.path.join(attempt, "session.bin"))
    lines = ["# Attempt %d" % n, "", "```", rss.summary(s), "```", ""]
    summary = {"attempt": n}
    phys = {}
    if "physics" in opts.sections:
        phys, part = physics(attempt, s, meta, opts, out, work, env)
        lines += part
    if "input" in opts.sections:
        frames = phys["_sound"]["frames"] if "_sound" in phys else fields(Frames(os.path.join(attempt, "physics.bin")))
        summary["input"], part = input_chain(s, frames, out, session_dir, attempt)
        lines += part
    if "ffb" in opts.sections:
        summary["ffb"], part = ffb(s, meta, phys, out)
        lines += part
    if "sound" in opts.sections:
        summary["sound"], part = sound(session_dir, s, meta, phys, opts, out, work, env)
        lines += part
    summary["physics"] = {k: v for k, v in phys.items() if not k.startswith("_")}
    summary["checks"] = checks(summary)
    lines[6:6] = check_lines(summary["checks"])
    with open(os.path.join(out, "report.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(out, "summary.json"), "w") as f:
        json.dump(summary, f, indent=1, default=lambda v: v.tolist() if hasattr(v, "tolist") else str(v))
    if not opts.keep_work:
        shutil.rmtree(work, ignore_errors=True)
    return summary


def checks(summary):
    """Per area: what the capture is to us (exact or approximate) and whether this attempt passed, for regression runs
    (None: not checked)."""
    p, f = summary.get("physics", {}), summary.get("ffb", {})
    exact = f.get("exact", {})
    physics_pass = None
    if "equal" in p:  # every step equal, to the recording's end or the race's
        physics_pass = p["equal"] == p["run"] and "first_diff" not in p and (p["run"] == p["frames"] or "ended" in p)
    return {
        "physics": {"expectation": "exact", "pass": physics_pass, "equal": p.get("equal"), "run": p.get("run"),
                    "frames": p.get("frames"), "first_diff": p.get("first_diff"),
                    "first_diff_words": p.get("first_diff_names"), "standing_words": p.get("standing_words"),
                    "moving_diff": p.get("moving_diff"), "moving_diff_words": p.get("moving_diff_names")},
        "ffb": {"expectation": "exact per value", "pass": exact.get("pass"), "fraction": exact.get("fraction"),
                "compared": exact.get("compared"), "matched": exact.get("matched")},
        "ffb_recorded_car": {"expectation": "exact per value", "pass": f.get("exact_recorded_car", {}).get("pass"),
                             "fraction": f.get("exact_recorded_car", {}).get("fraction"),
                             "compared": f.get("exact_recorded_car", {}).get("compared"),
                             "matched": f.get("exact_recorded_car", {}).get("matched")},
        "input": {"expectation": "exact once the poll is ported", "pass": None,
                  "why": "the raw-to-channel poll (0x4cbf50, NGP's input thread) is not ported" +
                         ("" if summary.get("input", {}).get("tie", "").startswith("each CTL1") else
                          "; no CTL1 ties the payloads to their polls")},
        "sound": {"expectation": "approximate", "pass": None,
                  "loudness_diff_mean": summary.get("sound", {}).get("loudness_diff_mean")},
    }


def check_lines(c):
    word = {True: "PASS", False: "FAIL", None: "not checked"}
    rows = ["## Checks", "", "| area | expectation | result | |", "| --- | --- | --- | --- |"]
    p, f = c["physics"], c["ffb"]
    rows.append("| physics | exact | %s | %s/%s equal, first difference %s |" % (
        word[p["pass"]], p["equal"], p["frames"], "none" if p["first_diff"] is None else p["first_diff"]))
    rows.append("| FFB | exact per value | %s | %s of %s |" % (word[f["pass"]], f["matched"], f["compared"]))
    r = c["ffb_recorded_car"]
    rows.append("| FFB of the recorded car | exact per value | %s | %s of %s |" % (word[r["pass"]], r["matched"],
                                                                                    r["compared"]))
    rows.append("| input | exact once the poll is ported | not checked | %s |" % c["input"]["why"])
    rows.append("| sound | approximate | not checked | loudness ours - recorded %s dB |" % (
        "%.1f" % c["sound"]["loudness_diff_mean"] if c["sound"]["loudness_diff_mean"] is not None else "-"))
    return rows + [""]


def headline(summary):
    p, so, c = summary.get("physics", {}), summary.get("sound", {}), summary["checks"]
    word = {True: "PASS", False: "FAIL", None: "-"}
    f = c["ffb"]
    return [str(summary["attempt"]), p.get("stage", ["", "-"])[1],
            "%s %s/%s" % (word[c["physics"]["pass"]], p.get("equal", "-"), p.get("frames", "-")),
            str(p.get("first_diff", "none")) + (" (others from %s)" % p["moving_diff"] if p.get("standing_words") else ""),
            "%s %s/%s" % (word[f["pass"]], f["matched"], f["compared"]) if f["compared"] is not None else "-",
            "%s/%s" % (c["ffb_recorded_car"]["matched"], c["ffb_recorded_car"]["compared"]),
            "%.1f dB" % so["loudness_diff_mean"] if "loudness_diff_mean" in so else "-"]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("session")
    ap.add_argument("--out", required=True)
    ap.add_argument("--attempt", type=int)
    ap.add_argument("--sections", default=",".join(SECTIONS))
    ap.add_argument("--standalone", default=os.path.join(ROOT, "target", "release", "rbr-standalone"))
    ap.add_argument("--sound", default=os.path.join(ROOT, "target", "release", "rbr-sound"))
    ap.add_argument("--ngp-physics")
    ap.add_argument("--rsf-car")
    ap.add_argument("--stage")
    ap.add_argument("--setup")
    ap.add_argument("--ffb-sensitivity")
    ap.add_argument("--camera", default="chase", choices=("inside", "chase", "trackside"))
    ap.add_argument("--keep-work", action="store_true")
    opts = ap.parse_args(argv)
    opts.sections = set(opts.sections.split(","))
    if opts.sections - set(SECTIONS):
        ap.error("--sections: %s" % ",".join(SECTIONS))
    env = dict(os.environ)
    ns = [opts.attempt] if opts.attempt else attempts(opts.session)
    os.makedirs(opts.out, exist_ok=True)
    rows = []
    for n in ns:
        summary = compare(opts.session, n, opts, env)
        rows.append(headline(summary))
        print("attempt %d: %s" % (n, " | ".join(rows[-1][1:])))
    with open(os.path.join(opts.out, "index.md"), "w") as f:
        f.write("# %s\n\n" % os.path.abspath(opts.session))
        f.write("| attempt | stage | physics (exact): equal steps | first differing step | FFB (exact per value): matched | FFB of the recorded car | loudness ours - rec (approximate) |\n")
        f.write("| --- | --- | --- | --- | --- | --- | --- |\n")
        for row in rows:
            f.write("| [%s](%s/report.md) | %s |\n" % (row[0], row[0], " | ".join(row[1:])))


if __name__ == "__main__":
    main()
