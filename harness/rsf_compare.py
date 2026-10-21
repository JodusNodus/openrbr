"""rsf_compare.py SESSION_DIR [--attempt N] --out REPORT_DIR: compares each recorded RSF stage attempt with ReRBR.

Usage: python3 -I -B harness/rsf_compare.py SESSION_DIR --out REPORT_DIR [--attempt N] [--sections LIST]
           [--standalone BIN] [--sound BIN] [--input-check BIN] [--ngp-physics FILE] [--rsf-car ID|PACK] [--stage NAME]
           [--setup FILE] [--ffb-sensitivity G,T,S] [--profile pf<name>.rbr] [--damage LEVEL] [--settings BIN]
           [--camera inside|chase|trackside] [--keep-work]

An attempt's <n>/setup.lsp (the setup the game loaded) goes to the boot as --setup unless --setup names another.
The drive's force-feedback settings come from `rsf-settings --session` (rbr_formats::rsf::settings, notes/ngp.md "RSF
settings"): NGP's sensitivity per surface for the car's slot, dead zone and threshold from meta.txt, the game's
strength and direction from --profile (the session holds no profile; without one, on, strength 1, normal).
--ffb-sensitivity replaces the percentages. The damage level (the quick race's 0x1660850) is GAM1's; --damage
replaces it, and gives it for sessions recorded before GAM1 held it, which otherwise run the boot's default, safe
(drive2 runs exact on safe, drive1/2 on off).

SESSION_DIR is the session tree rec/capture fetches (notes/verification.md "Comparing RSF sessions"): <n>/physics.bin
(FRM1), <n>/session.bin (RSS1, harness/session.py), audio.wav with audio.txt, meta.txt. For each attempt it writes
REPORT_DIR/<n>/report.md, summary.json and PNG plots, and REPORT_DIR/index.md lists the attempts' headline numbers.
--sections takes any of physics,input,ffb,sound (default all). The programs default to this checkout's
target/release; RBR_DATA, RBR_PATCH, RBR_RSF and RBR_NGP_PHYSICS are read as rbr-standalone reads them.

1. physics: `rbr-standalone --boot --recording <n>/physics.bin --recorded-payload --trace-out` with the stage, car,
   weather and tyre of the attempt's GAM1 (the car an RSF car: --rsf-car, else the pack the stage load opened (OPN1),
   else attempt.txt's or the one [carNN] of meta.txt; the report says when OPN1 and attempt.txt disagree; NGP
   physics when NGP.dll was loaded, the session's force-feedback sensitivities), as the launcher's command
   (crates/rbr-launcher/src/command.rs) builds them. The equal steps, the first differing step and its words come
   from rbr-standalone's report (rbr_hybrid::pure::word_name); the curves from both FRM1s and our trace's .ext.
   The boot also takes the aids from the recording's car words, [NGP] headGasketDurability, the control input's
   earlier payloads at a step (CTL1, --extra-payloads) and NGP's starter clock (STP1, --wall-clock).
2. input: `input_check` (rbr_gameplay::poll::session): every car control call (CTL1) against the payload the ported
   device poll (POL1 through the profile's .acm, FIL1), NGP's gears, the filter and NGP's handler give; exact, pass
   when every call is equal.
3. ffb: the constant force's SetParameters magnitudes (EFF1, by STP1's QPC; sets over 0.1 s after the last
   StepForward left out) against rbr_gameplay::ffb::magnitude and dead_zone of our steering_ffb::force: exact per
   value, correlation, RMS error, lag. Also the mapping alone: NGP's curve of the recorded filtered torque (FRM1 car +0xaec, gate +0x1ec) with
   the car's S (its common.lsp ForceFeedbackSensitivity_NGP, scaled by the stage surface's percentage), so a physics
   divergence does not hide the settings.
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
DAMAGE_LEVELS = ("off", "safe", "reduced", "realistic")  # 0x1660850's words
HEADER, CONTROL, WHEEL, WHEELS = 20, 0x20, 0xA00, 4
FRM1 = 0x314D5246
STEP_102, STEP_NGP = 0.007, 1.0 / 720  # notes/verification.md "Recordings": a substep
SETTLE_SUBSTEPS = 40
FILETIME_UNIX = 116444736000000000  # 100 ns ticks from 1601 to 1970
CONSTANT_FORCE = "13541c20-8e33-11d0-9ad0-00a0c9a06e35"
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


def loaded_pack(s):
    """The RSF car pack whose files the attempt's stage load opened (OPN1 `rsfdata/cars/<pack>/` before the first
    StepForward; later opens are the next car's install), or None."""
    steps = s.tagged("STP1")
    first = steps[0]["qpc"] if steps else float("inf")
    packs = [m.group(1) for o in s.tagged("OPN1") if o["qpc"] < first
             for m in [re.search(r"rsfdata[\\/]cars[\\/]([^\\/]+)[\\/]", o["path"], re.I)] if m]
    return packs[-1] if packs else None


def pack_id(pack):
    """`pack`'s RSF car id in RBR_RSF's `rsfdata/cache/cars.json`, or None."""
    try:
        cars = json.load(open(os.path.join(os.environ.get("RBR_RSF", ""), "rsfdata", "cache", "cars.json")))
    except (OSError, ValueError):
        return None
    return next((c["id"] for c in cars if c.get("path", "").lower() == pack.lower()), None)


def race_flags(game, meta, opts, attempt, s):
    """The --boot flags the launcher builds for this attempt (command.rs viewer_args/launch_args), and notes. The RSF
    car is --rsf-car's, else the pack the stage load opened, else attempt.txt's (`Cars.ini [Car05]`, which can lag)."""
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
    named = read_keys(os.path.join(attempt, "attempt.txt"))[""]
    rsf_car, pack = opts.rsf_car or named.get("rsf_car_id"), loaded_pack(s)
    named_pack = named.get("rsf_car_physics", "").strip('"').replace("\\", "/").rstrip("/").rsplit("/", 1)[-1]
    if pack and not opts.rsf_car and pack.lower() != named_pack.lower():
        notes.append("the stage load opened %s's files, attempt.txt names %s (car %s): ran %s" % (
            pack, named_pack or "none", rsf_car, pack))
        rsf_car = pack_id(pack) or pack
    if rsf_car is None:
        cars = sorted({int(m.group(1)) for k in meta for m in [re.fullmatch(r"car(\d+)", k)] if m})
        if len(cars) == 1:
            rsf_car = str(cars[0])
        elif cars:
            notes.append("meta.txt names cars %s; give --rsf-car" % cars)
    if rsf_car and ngp:
        flags += ["--rsf-car", rsf_car]
    settings = drive_settings(os.path.dirname(attempt), rsf_car, game["car"], opts)
    notes += settings["notes"]
    if ngp and settings["percent"] != [100, 100, 100]:
        flags += ["--ffb-sensitivity", ",".join(str(p) for p in settings["percent"])]
    if ngp and settings["head_gasket_durability"] > 1:
        flags += ["--head-gasket-durability", str(settings["head_gasket_durability"])]
    damage = opts.damage or (DAMAGE_LEVELS[min(game["damage"], 3)] if "damage" in game else None)
    if damage:
        flags += ["--damage", damage]
    else:
        notes.append("damage level: the boot's default (safe); this session does not record it (--damage)")
    setup = opts.setup or next((p for p in [os.path.join(attempt, "setup.lsp")] if os.path.exists(p)), None)
    if setup:
        flags += ["--setup", setup]
    else:
        notes.append("no --setup: the boot takes the car's default setup, not the one driven (the session holds none)")
    return flags, ngp, rsf_car, notes, settings


def parse_settings(text):
    """rsf-settings' key=value lines as the settings the FFB check takes."""
    kv = dict(line.split("=", 1) for line in text.splitlines() if "=" in line)
    number = lambda key, default: float(kv[key]) if key in kv else default  # noqa: E731
    return {"percent": [int(p) for p in kv["sensitivity"].split(",")], "source": kv["sensitivity_source"].split(","),
            "dead_zone": number("dead_zone", 0.0), "dead_zone_threshold": number("dead_zone_threshold", 0.0),
            "ffb_rate": int(number("ffb_rate", 1000)), "input_rate": int(number("input_rate", 1000)),
            "head_gasket_durability": int(number("head_gasket_durability", 1)),
            "enabled": number("enabled", 1) != 0, "strength": number("strength", 1.0),
            "inverse": number("inverse", 0) != 0, "game_source": kv.get("game_source"),
            "aids": {k: kv[k] != "0" for k in ("gear_guard", "neutral_lock", "clutch_help") if k in kv}}


def drive_settings(session_dir, rsf_car, slot, opts):
    """The session's settings for the RSF car in `slot` (`rsf-settings --session`), --ffb-sensitivity over its
    percentages; `notes` says what is assumed."""
    notes = []
    command = [opts.settings, "--session", session_dir, "--car", str(rsf_car or 0), "--slot", str(slot)]
    if opts.profile:
        command += ["--profile", opts.profile]
    try:
        r = subprocess.run(command, capture_output=True, text=True)
        out = parse_settings(r.stdout) if r.returncode == 0 else None
    except OSError:
        r, out = None, None
    if out is None:
        notes.append("rsf-settings failed (%s): NGP's and the game's defaults" % (r.stderr.strip() if r else
                                                                                    opts.settings))
        out = parse_settings("sensitivity=100,100,100\nsensitivity_source=default,default,default\n")
    if out["game_source"] is None:
        notes.append("no game profile (--profile): force feedback on, strength 1, normal direction assumed")
    if opts.ffb_sensitivity:
        out["percent"], out["source"] = [int(p) for p in opts.ffb_sensitivity.split(",")], ["--ffb-sensitivity"] * 3
    out["notes"] = notes
    return out


def surface_of(map_id):
    """The stage's surface index (NGP's SetupSpec: 0 gravel, 1 tarmac, 2 snow): RSF's stages_data.json surface_id
    when RBR_RSF lists the map, else the exe's table (rbr-standalone boot::surface, E14_4a1330)."""
    path = os.path.join(os.environ.get("RBR_RSF", ""), "rsfdata", "cache", "stages_data.json")
    if os.path.exists(path):
        for row in json.load(open(path, encoding="utf-8-sig")):
            if str(row.get("id")) == str(map_id):
                return {"1": 1, "2": 0, "3": 2}.get(str(row.get("surface_id")), 0)
    if 10 <= map_id <= 18:
        return 2
    return 1 if 0x29 <= map_id <= 0x31 or map_id in (0x49, 0x55, 0x56, 0x5E) else 0


def car_sensitivity(attempt, loaded=None):
    """The car's ForceFeedbackSensitivity_NGP (S before the ini's percentage) and the file it came from: the RSF
    pack's common.lsp the attempt loaded (<n>/files), else RBR_RSF's; 100 (physics.lsp's) without one. The pack is
    `loaded` (`loaded_pack`) when given, else attempt.txt's."""
    pack = ("rsfdata/cars/" + loaded if loaded
            else read_keys(os.path.join(attempt, "attempt.txt"))[""].get("rsf_car_physics", "").strip('"'))
    pack = pack.replace("\\", "/").lower()
    candidates = []
    if pack:
        for root, _, files in os.walk(os.path.join(attempt, "files")):
            if root.lower().replace("\\", "/").endswith(pack) and "common.lsp" in files:
                candidates.append(os.path.join(root, "common.lsp"))
        rsf = os.environ.get("RBR_RSF", "")
        parts = [p for p in pack.split("/") if p]
        here = rsf
        for part in parts:  # the install's own case
            names = {n.lower(): n for n in os.listdir(here)} if os.path.isdir(here) else {}
            here = os.path.join(here, names.get(part, part))
        candidates.append(os.path.join(here, "common.lsp"))
    for path in candidates:
        if os.path.exists(path):
            m = re.search(r"ForceFeedbackSensitivity_NGP\s+([-0-9.eE]+)", open(path, errors="replace").read())
            if m:
                return float(m.group(1)), path
    return 100.0, "physics.lsp default"


def ngp_curve(torque, gated, sensitivity):
    """rbr_physics::ngp::ffb::curve in f32 (pow in double on 0.975f)."""
    t = np.asarray(torque, dtype=np.float32)
    e = (np.abs(t) * (np.float32(100.0) / np.float32(sensitivity))).astype(np.float32)
    p = np.power(np.float64(np.float32(0.975)), e.astype(np.float64)).astype(np.float32)
    return np.where(gated, np.float32(0.0), np.sign(t).astype(np.float32) * (np.float32(1.0) - p))


def scaled_sensitivity(s, percent):
    """rbr_physics::ngp::car_init::ffb_sensitivity: (percent / 100f) * S above 0 and not 100, at least 1."""
    s = np.float32(s)
    if percent > 0 and percent != 100:
        s = np.float32(np.float32(percent) / np.float32(100.0)) * s
    return float(max(s, np.float32(1.0)))


AIDS = (("auto_gears", 0x890), ("gear_guard", 0x894), ("neutral_lock", 0x898), ("clutch_help", 0x89C))


def driver_aids(frames, profile):
    """The aids flags of the recording's first frame (car +0x890..+0x89c, the race options the car's control unit
    took, `notes/gameplay.md`), and a note comparing them with the profile's (`profile`, empty without one)."""
    first = frames.words([frames.car_at(off) for _, off in AIDS])[0]
    aids = {name: bool(v) for (name, _), v in zip(AIDS, first)}
    flags = []
    for name, on in aids.items():
        flags += ["--" + name.replace("_", "-"), "on" if on else "off"]
    note = "aids from the recording (car+0x890..+0x89c): " + ", ".join(
        "%s %s" % (n.replace("_", " "), "on" if on else "off") for n, on in aids.items())
    differ = [n for n, on in profile.items() if aids[n] != on]
    if differ:
        note += "; the profile differs: " + ", ".join(differ)
    elif profile:
        note += "; the profile agrees (%s; it keeps no auto gears)" % ", ".join(profile)
    return flags, note


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


def write_extra(s, path):
    """`--extra-payloads`: per step with more than one CTL1, every payload but the last (FRM1 holds the last), as
    `u32 step` and the payload. Returns how many it wrote."""
    by_step = {}
    for c in s.tagged("CTL1"):
        by_step.setdefault(c["step"], []).append(c["payload"])
    records = [struct.pack("<I", step) + p for step, ps in sorted(by_step.items()) for p in ps[:-1]]
    if records:
        with open(path, "wb") as f:
            f.write(b"".join(records))
    return len(records)


def write_clock(s, path):
    """`--wall-clock`: each step's first STP1 as NGP's starter reads the clock (us: the QueryPerformanceCounter ticks
    times 10^6 over the frequency, which the CLK1 pairs give against FILETIME's 10 MHz). Returns how many it wrote."""
    clk = s.tagged("CLK1")
    if len(clk) < 2 or clk[-1]["filetime"] == clk[0]["filetime"]:
        return 0
    hz = round((clk[-1]["qpc"] - clk[0]["qpc"]) * 1e7 / (clk[-1]["filetime"] - clk[0]["filetime"]), -3)
    first = {}
    for r in s.tagged("STP1"):
        first.setdefault(r["pre"], r["qpc"])
    if first:
        with open(path, "wb") as f:
            f.write(b"".join(struct.pack("<IQ", pre, qpc * 1000000 // int(hz)) for pre, qpc in sorted(first.items())))
    return len(first)


def physics(attempt, s, meta, opts, out, work, env):
    games = s.tagged("GAM1")
    if not games:
        return {"error": "no GAM1 record"}, ["No GAM1 record: the stage and car are unknown."]
    game = games[0]
    flags, ngp, rsf_car, notes, settings = race_flags(game, meta, opts, attempt, s)
    rec = os.path.join(attempt, "physics.bin")
    base = ["--boot", "--recording", rec] + flags
    stage = resolve_stage(opts, game, opts.standalone, base, env)
    trace = os.path.join(work, "ours.bin")
    frames = Frames(rec)
    aids, why = driver_aids(frames, settings["aids"])
    base += aids
    notes.append(why)
    command = [opts.standalone] + base + stage + ["--steps", str(frames.n), "--recorded-payload", "--trace-out", trace]
    extra, clock = os.path.join(work, "extra.bin"), os.path.join(work, "clock.bin")
    hooks = (["--extra-payloads", extra] if write_extra(s, extra) else []) + \
        (["--wall-clock", clock] if ngp and write_clock(s, clock) else [])
    command[command.index("--trace-out"):command.index("--trace-out")] = hooks
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
                       "settings": settings, "rec": rec, "map": game["map"], "attempt": attempt, "pack": loaded_pack(s),
                       "speed": a["speed"][rows],
                       "sideways": a["sideways"][rows], "driving": a["substeps"][rows] != SETTLE_SUBSTEPS}
    summary["_sound"] = {"stage": stage, "flags": flags, "rec": rec, "frames": a, "ngp": ngp}
    if not opts.keep_work:
        for path in (trace,):
            os.remove(path)
    return summary, lines


# --- 2. input ----------------------------------------------------------------------------------------------------

INPUT_LINE = re.compile(r"attempt (\d+): equal (\d+) of (\d+) \(by time (\d+)\), bursts (\d+), moved (\d+), "
                        r"gear from recording (\d+), menu (\d+), words \[([^\]]*)\]")


def input_chain(session_dir, n, opts, env):
    """`input_check` (rbr_gameplay::poll::session) over the session: each car control call (CTL1) against the payload
    the ported poll, NGP's gears, the filter and NGP's handler give for the polls (POL1) delivered before it. It runs
    every attempt (the axes' state carries over within a game process) and reports this one."""
    lines = ["## Input chain (NGP)", ""]
    r = subprocess.run([opts.input_check, session_dir, str(n)], capture_output=True, text=True, env=env)
    out = r.stdout + r.stderr
    m = INPUT_LINE.search(out)
    if not m:
        return {"error": "input_check failed"}, lines + ["`input_check` failed (exit %d):" % r.returncode, "", "```",
                                                         out[-3000:], "```"]
    keys = ("attempt", "equal", "compared", "equal_by_time", "bursts", "moved", "gear_from_recording", "menu")
    summary = dict(zip(keys, (int(v) for v in m.groups()[:8])))
    summary["words"] = [int(v) for v in m.group(9).split(",")] if m.group(9).strip() else []
    first = re.search(r"first difference at step (\d+): ours (.*), recorded (.*)", out)
    if first:
        summary["first_diff"] = {"step": int(first.group(1)), "ours": first.group(2), "recorded": first.group(3)}
    c = summary["compared"]
    pct = lambda k: 100.0 * summary[k] / c if c else 0.0  # noqa: E731
    lines += ["Command: `%s %s %d`" % (os.path.basename(opts.input_check), session_dir, n), "",
              "- car control calls: **%d of %d equal** (%.3f %%) in all eight words; %d (%.3f %%) with the poll "
              "delivery the QPCs give" % (summary["equal"], c, pct("equal"), summary["equal_by_time"],
                                         pct("equal_by_time")),
              "- bursts of steps %d, of which %d took a neighbouring poll count (NGP delivers the polls once per burst; "
              "the moment is not recorded)" % (summary["bursts"], summary["moved"]),
              "- gear selection flag taken from the recording (the drive's commanded gear is not in the session): %d"
              % summary["gear_from_recording"],
              "- calls skipped (the pause menu's call for help or reset): %d" % summary["menu"],
              "- mismatching calls per word: %s" % summary["words"]]
    if first:
        lines.append("- first difference at step %(step)d: ours %(ours)s, recorded %(recorded)s" % summary["first_diff"])
    return summary, lines + [""]


# --- 3. FFB ------------------------------------------------------------------------------------------------------

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


def exact_ffb(set_steps, set_values, posts, ours, reach=REACH, also=None, earliest=None):
    """Each recorded magnitude against ours at the last step whose StepForward ended before it, or up to `reach`
    steps either side (NGP's 1000 Hz FFB thread reads the force EmitForce stored, which STP1's QPC brackets only
    to a couple of steps), back to `earliest` (per set, the step at the set before it: the thread reads the force
    after its previous send returns, and a send can block in the driver for a while, the steps running on; or the
    step before a catch-up burst, `burst_start`), or one of the set's row of `also` (values the thread sends outside
    the steps, NaN for none; `ffb_outside`): per value, whether ours was run there (`compared`), whether one of those
    equals it (`match`), the offset of the nearest that does (+ an earlier step; 0 where none, or past `reach`), and
    ours at the step and the step before (NaN where not run)."""
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
    if earliest is not None:
        first = np.searchsorted(posts, earliest)
        for i in np.nonzero(here & ~match & (first < pos - reach))[0]:
            match[i] = bool((ours[max(first[i] - reach, 0):pos[i] - reach] == set_values[i]).any())
    if also is not None:
        match |= here & (also == np.asarray(set_values, dtype=np.float64)[:, None]).any(axis=1)
    return here, match, offset, at, before


def exact_check(set_steps, values, posts, ours, also=None, earliest=None):
    """exact_ffb as a summary: compared, matched (`outside` of them only by `also`, `stalled` only by `earliest`),
    fraction, pass and the first mismatches."""
    compared, match, _, at, before = exact_ffb(set_steps, values, posts, ours, also=also, earliest=earliest)
    n, good = int(compared.sum()), int((match & compared).sum())
    plain = int((exact_ffb(set_steps, values, posts, ours)[1] & compared).sum())
    stalled = int((exact_ffb(set_steps, values, posts, ours, earliest=earliest)[1] & compared).sum()) - plain
    outside = good - plain - stalled
    bad = np.nonzero(compared & ~match)[0]
    return {"compared": n, "matched": good, "outside": outside, "stalled": stalled, "fraction": good / n if n else None,
            "not_run": int((~compared).sum()),
            "pass": n > 0 and good == n,
            "mismatches": [{"set": int(i), "step": int(set_steps[i]), "recorded": int(values[i]),
                            "ours": None if np.isnan(at[i]) else int(at[i]),
                            "ours_before": None if np.isnan(before[i]) else int(before[i])} for i in bad[:200]]}, \
        compared, match


def mismatch_lines(exact):
    if not exact["mismatches"]:
        return []
    lines = ["First mismatches (%d in all; summary.json keeps 200):" % (exact["compared"] - exact["matched"]), "",
             "| set | step | recorded | ours at step | ours step before |", "| --- | --- | --- | --- | --- |"]
    row = "| %(set)d | %(step)d | %(recorded)d | %(ours)s | %(ours_before)s |"
    lines += [row % m for m in exact["mismatches"][:20]]
    return lines + [""]


def recorded_mapping(data, settings, gain):
    """Ours from the recorded car (the mapping alone): NGP's curve of FRM1's filtered torque (car +0xaec) and gate
    (+0x1ec) with the car's S scaled by the stage surface's percentage, then `gain` (magnitude and dead zone with the
    recorded steering). Returns (posts, magnitudes, S, how S was found)."""
    frames = Frames(data["rec"])
    w = frames.words([8, frames.car_at(0xAEC), frames.car_at(0x1EC), HEADER + 16])
    s_car, where = car_sensitivity(data["attempt"], data.get("pack"))
    surface = surface_of(data["map"])
    s_eff = scaled_sensitivity(s_car, settings["percent"][surface])
    force = ngp_curve(w[:, 1].view("<f4"), w[:, 2] != 0, s_eff)
    ours = gain(force, w[:, 3].view("<f4"))
    order = np.argsort(w[:, 0], kind="stable")
    return w[order, 0].astype(np.int64), ours[order].astype(np.float64), s_eff, \
        "%g x %d%% (%s surface; S from %s)" % (s_car, settings["percent"][surface],
                                               ("gravel", "tarmac", "snow")[surface], where)


def burst_start(clock, frequency):
    """A function giving, for steps, the step before their burst: after a hitch the game catches up with
    StepForwards under 1 ms apart (0.08-0.65 ms; a step is 1.39 ms, and no StepForward comes 1-1.2 ms after the one
    before in drive2), and the force EmitForce stored before the burst stays until it ends (drive2 attempt 4: a
    0.4 s hitch, then ~300 steps on one magnitude)."""
    q, post = clock
    gap = np.diff(q, prepend=q[0] - frequency) >= frequency // 1000
    start = post[np.maximum.accumulate(np.where(gap, np.arange(len(q)), 0))] - 1

    def at(steps):
        i = np.clip(np.searchsorted(post, steps), 0, len(post) - 1)
        return np.where(post[i] == steps, start[i], steps)
    return at


def ffb_outside(s, q, data, st, gain):
    """What NGP's FFB thread sends outside the steps' forces (`exact_ffb`'s `also`, two candidates per set): it sends
    `floor(-10000 x)` of the force EmitForce last stored while the game's racing gate holds (778 0x1003c3eb, the exe's
    0x5403e0 case 0xb: app state 1, FFB on, the start timer done), else 0. The gate opens in the first driving
    StepForward before its EmitForce, so until that step ends (as the thread reads it: after its previous send
    returned) it resends the force the previous attempt left (its recorded car's last two steps by
    `recorded_mapping`: the last when its final step stored one); after the last StepForward the gate closes, 0."""
    stp = s.tagged("STP1")
    first = next((r["qpc"] for r in stp if r["substeps"] == 1), None)
    also = np.full((len(q), 2), np.nan)
    if first is None:
        return also
    carried = [0.0, np.nan]
    attempt = data["attempt"].rstrip("/")
    prev = os.path.join(os.path.dirname(attempt), str(int(os.path.basename(attempt)) - 1))
    if data["ngp"] and os.path.exists(os.path.join(prev, "physics.bin")):
        games = rss.read(os.path.join(prev, "session.bin")).tagged("GAM1")
        if games:
            _, mapped, _, _ = recorded_mapping({"rec": os.path.join(prev, "physics.bin"), "attempt": prev,
                                                "map": games[0]["map"]}, st, gain)
            carried = list(mapped[-2:])
    q = np.asarray(q, dtype=np.int64)
    # The thread reads the force after its previous send returns.
    also[np.concatenate([q[:1], q[:-1]]) <= first] = carried
    also[q > stp[-1]["qpc"], 0] = 0.0
    return also


def ffb(s, meta, phys, out):
    lines = ["## Force feedback", ""]
    data = phys.get("_ffb")
    if data is None:
        return {"error": "no physics trace"}, lines + ["No trace of ours (the physics section did not run)."]
    sets, directions = recorded_ffb(s)
    st = data["settings"]
    dz, thr = st["dead_zone"], st["dead_zone_threshold"]
    strength = st["strength"] if st["enabled"] else 0.0

    def gain(force, steering):
        return dead_zone(magnitude(force, strength, st["inverse"], ngp=data["ngp"]), steering, dz, thr)

    ours = gain(data["force"], data["steering"]).astype(np.float64)
    summary = {"sets": len(sets), "directions": directions, "settings": {k: v for k, v in st.items()}}
    lines += ["Ours: `rbr_gameplay::ffb::magnitude` (%s, strength %g, %s: %s) and `dead_zone` (`ffbDeadZone` %g, "
              "`ffbDeadZoneThreshold` %g) of `steering_ffb::force` after each step; sensitivity %s (%s). Recorded: "
              "each constant-force SetParameters' magnitude (negated for a negative direction; directions seen %s), "
              "held from the step of the StepForward before it to the next." % (
                  "on" if st["enabled"] else "off", st["strength"], "inverse" if st["inverse"] else "normal",
                  st["game_source"] or "no profile, assumed", dz, thr, st["percent"], "; ".join(st["source"]),
                  directions), ""]
    if not sets:
        lines.append("No constant-force SetParameters in the session.")
        return summary, lines
    clock = step_clock(s)
    q, m = zip(*sets)
    # The FFB thread runs on after the stage: sets more than 0.1 s after the last StepForward have no step to compare
    # with.
    during = np.array(q, dtype=np.int64) <= clock[0][-1] + s.qpc_frequency // 10
    summary["after_last_step"] = int((~during).sum())
    q, m = np.array(q)[during], np.array(m)[during]
    set_steps, values = steps_at(q, clock), m.astype(np.float64)
    also = ffb_outside(s, q, data, st, gain)
    earliest = np.minimum(np.concatenate([set_steps[:1], set_steps[:-1]]),
                          burst_start(clock, s.qpc_frequency)(set_steps))
    summary["exact"], compared, match = exact_check(set_steps, values, data["post"].astype(np.int64), ours, also,
                                                    earliest)
    e = summary["exact"]
    lines += ["**Exact check** (expected exact per value): each recorded magnitude against ours at the last step "
              "whose StepForward ended before it, up to %d steps either side, or outside the steps what the thread's "
              "racing gate sends (`ffb_outside`: the previous attempt's force until the first driving step ends, 0 "
              "after the last), back to the step of the set before it or before a catch-up burst. %d of %d match (%s; "
              "%d by the gate, %d held); %d sets fall outside our run, %d come over 0.1 s after the last step."
              % (REACH, e["matched"], e["compared"], "%.4f" % e["fraction"] if e["compared"] else "-", e["outside"],
                 e["stalled"], e["not_run"], summary["after_last_step"]), ""]
    lines += mismatch_lines(e)
    if data["ngp"]:
        posts, mapped, s_eff, how = recorded_mapping(data, st, gain)
        summary["mapping"], _, _ = exact_check(set_steps, values, posts, mapped, also, earliest)
        summary["mapping"]["sensitivity"] = s_eff
        mp = summary["mapping"]
        lines += ["**Mapping from the recorded car** (expected exact per value; physics divergence left out): NGP's "
                  "curve of the recorded filtered torque with S %g = %s, then the gain above. %d of %d match (%s; %d "
                  "by the gate, %d held)." % (s_eff, how, mp["matched"], mp["compared"],
                                                             "%.4f" % mp["fraction"] if mp["compared"] else "-",
                                                             mp["outside"], mp["stalled"]), ""]
        lines += mismatch_lines(mp)
    lines += ["Secondary, held per step:", ""]
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
        summary["input"], part = input_chain(session_dir, n, opts, env)
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
    p, f, i = summary.get("physics", {}), summary.get("ffb", {}), summary.get("input", {})
    exact, mapping = f.get("exact", {}), f.get("mapping", {})
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
        "ffb_mapping": {"expectation": "exact per value", "pass": mapping.get("pass"),
                        "fraction": mapping.get("fraction"), "compared": mapping.get("compared"),
                        "matched": mapping.get("matched")},
        "input": {"expectation": "exact", "pass": i.get("equal") == i.get("compared") if "compared" in i else None,
                  "equal": i.get("equal"), "compared": i.get("compared")},
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
    f = c["ffb_mapping"]
    rows.append("| FFB mapping (recorded car) | exact per value | %s | %s of %s |" % (word[f["pass"]], f["matched"],
                                                                                      f["compared"]))
    rows.append("| input | exact | %s | %s of %s car control calls equal |" % (
        word[c["input"]["pass"]], c["input"]["equal"], c["input"]["compared"]))
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
            "%s %s/%s" % (word[c["input"]["pass"]], c["input"]["equal"], c["input"]["compared"])
            if c["input"]["compared"] is not None else "-",
            "%s %s/%s" % (word[f["pass"]], f["matched"], f["compared"]) if f["compared"] is not None else "-",
            "%s %s/%s" % (word[c["ffb_mapping"]["pass"]], c["ffb_mapping"]["matched"], c["ffb_mapping"]["compared"])
            if c["ffb_mapping"]["compared"] is not None else "-",
            "%.1f dB" % so["loudness_diff_mean"] if "loudness_diff_mean" in so else "-"]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("session")
    ap.add_argument("--out", required=True)
    ap.add_argument("--attempt", type=int)
    ap.add_argument("--sections", default=",".join(SECTIONS))
    ap.add_argument("--standalone", default=os.path.join(ROOT, "target", "release", "rbr-standalone"))
    ap.add_argument("--sound", default=os.path.join(ROOT, "target", "release", "rbr-sound"))
    ap.add_argument("--input-check", default=os.path.join(ROOT, "target", "release", "input_check"))
    ap.add_argument("--ngp-physics")
    ap.add_argument("--rsf-car")
    ap.add_argument("--stage")
    ap.add_argument("--setup")
    ap.add_argument("--ffb-sensitivity")
    ap.add_argument("--profile")
    ap.add_argument("--damage", choices=DAMAGE_LEVELS)
    ap.add_argument("--settings", default=os.path.join(ROOT, "target", "release", "rsf-settings"))
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
        f.write("| attempt | stage | physics (exact): equal steps | first differing step | input (exact): equal calls | "
                "FFB (exact per value): matched | FFB mapping (recorded car): matched | loudness ours - rec "
                "(approximate) |\n")
        f.write("| --- | --- | --- | --- | --- | --- | --- | --- |\n")
        for row in rows:
            f.write("| [%s](%s/report.md) | %s |\n" % (row[0], row[0], " | ".join(row[1:])))


if __name__ == "__main__":
    main()
