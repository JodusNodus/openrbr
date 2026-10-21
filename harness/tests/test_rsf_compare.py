"""Tests of harness/rsf_compare.py: the parsing and alignment on synthetic RSS1 files, and the whole tool on a
synthetic session made of an existing drive's first frames with a made-up RSS1 beside it (needs RBR_DATA, RBR_PATCH,
RBR_RECORDINGS and the release rbr-standalone and rbr-sound; skipped without them).

Usage: .venv/bin/python -I -B harness/tests/test_rsf_compare.py [-v]
"""
import os
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import uuid
import zlib

import numpy as np

HARNESS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HARNESS)
import rsf_compare as rc  # noqa: E402
import session as rss  # noqa: E402

FREQ = 10_000_000
UNIX0 = 1_760_000_000.0


class Writer:
    """An RSS1 file in memory, as the plugin writes it (harness/session.py's docstring)."""

    def __init__(self):
        self.out = bytearray(b"RSS1" + struct.pack("<IQ", 1, FREQ))

    def record(self, tag, qpc, body, thread=7):
        payload = struct.pack("<QI", qpc, thread) + body
        self.out += tag.encode() + struct.pack("<I", len(payload)) + payload

    def clk(self, qpc, unix):
        self.record("CLK1", qpc, struct.pack("<Q", int(round(unix * 1e7)) + rc.FILETIME_UNIX))

    def stp(self, qpc, pre, post, substeps):
        self.record("STP1", qpc, struct.pack("<3I", pre, post, substeps))

    def gam(self, qpc, game, car, weather, tyre, base=0, size=0, version=b""):
        self.record("GAM1", qpc, struct.pack("<7I", game, car, weather, tyre, base, size, len(version)) + version)

    def dev(self, qpc, device, name):
        self.record("DEV1", qpc, struct.pack("<I", device) + uuid.uuid4().bytes_le + uuid.uuid4().bytes_le +
                    struct.pack("<I", len(name)) + name)

    def fmt(self, qpc, device, size, objects):
        body = struct.pack("<3I", device, size, len(objects))
        for ofs, typ in objects:
            body += bytes(16) + struct.pack("<3I", ofs, typ, 0)
        self.record("FMT1", qpc, body)

    def range(self, qpc, device, how, obj, lo, hi):
        data = struct.pack("<ii", lo, hi)
        self.record("PRP1", qpc, struct.pack("<7I", device, 0, rc.DIPROP_RANGE, how, obj, 0, len(data)) + data)

    def pol(self, qpc, device, state):
        self.record("POL1", qpc, struct.pack("<3I", device, 0, len(state)) + state)

    def buf(self, qpc, device, events):
        self.record("BUF1", qpc, struct.pack("<3I", device, 0, len(events)) +
                    b"".join(struct.pack("<5I", ofs, data & 0xFFFFFFFF, 0, 0, 0) for ofs, data in events))

    def eff(self, qpc, effect, op, magnitude=None, direction=1, guid=b""):
        body = struct.pack("<5I", 0, effect, op, 0, 0) + (guid or bytes(16))
        if magnitude is None:
            body += struct.pack("<I", 0)
        else:  # flags, duration, sample period, gain, trigger button and repeat; one axis, its direction; no envelope
            body += struct.pack("<I6I", 1, 0x10, 0xFFFFFFFF, 0, 10000, 0xFFFFFFFF, 0)
            body += struct.pack("<IIi", 1, 0, direction) + struct.pack("<II", 0, 4) + struct.pack("<i", magnitude)
            body += struct.pack("<I", 0)
        self.record("EFF1", qpc, body)

    def save(self, path):
        open(path, "wb").write(bytes(self.out))


CONSTANT = uuid.UUID(rc.CONSTANT_FORCE).bytes_le


def frames_bytes(path, n):
    with open(path, "rb") as f:
        head = f.read(rc.HEADER)
    car = struct.unpack_from("<I", head, 16)[0]
    with open(path, "rb") as f:
        return f.read(n * (rc.HEADER + rc.CONTROL + car + rc.WHEELS * rc.WHEEL))


def write_wav(path, samples, rate, float32=False):
    """Stereo WAV of mono `samples` in [-1, 1]."""
    x = np.repeat(np.asarray(samples)[:, None], 2, axis=1)
    data = x.astype("<f4").tobytes() if float32 else (np.clip(x, -1, 1) * 32767).astype("<i2").tobytes()
    tag, bits = (3, 32) if float32 else (1, 16)
    fmt = struct.pack("<HHIIHH", tag, 2, rate, rate * 2 * bits // 8, 2 * bits // 8, bits)
    body = b"WAVE" + b"fmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(data)) + data
    open(path, "wb").write(b"RIFF" + struct.pack("<I", len(body)) + body)


def synthetic_session(root, recording, n, lag=7, lead=2.0, ours=None, wrong=None):
    """A session from `recording`'s first `n` frames: STP1 at 7 ms a substep, CLK1 once a second, a wheel polled each
    frame from the payload (X steering over -32768..32767, Y throttle and Rz brake over 0..65535), audio.wav from `lead`
    s before the first step. The constant force is set after each step to 3000 x steering `lag` steps before, or with
    `ours` (a --trace-out .ext) to our magnitude at that step, at the step before on every fifth (NGP's FFB thread
    racing the step), and off by one after frame `wrong`."""
    os.makedirs(os.path.join(root, "1"))
    open(os.path.join(root, "1", "physics.bin"), "wb").write(frames_bytes(recording, n))
    f = rc.fields(rc.Frames(os.path.join(root, "1", "physics.bin")))
    w = Writer()
    q0 = 50 * FREQ
    w.clk(q0, UNIX0)
    w.gam(q0, 54, 6, 0, 4)
    w.dev(q0, 0, b"Synthetic wheel")
    w.fmt(q0, 0, 80, [(0, 0x00000102), (4, 0x00000202), (20, 0x00000502), (48, 0x0000000C)])
    w.range(q0, 0, 1, 0, -32768, 32767)
    w.range(q0, 0, 1, 4, 0, 65535)
    w.range(q0, 0, 1, 20, 0, 65535)
    w.eff(q0, 0, 1, guid=CONSTANT)
    t, next_clock = 0.0, 1.0
    control = f["control"].view("<f4")
    if ours is not None:
        ext = rc.read_ext(ours)
        mine = dict(zip(ext["post"].tolist(), rc.magnitude(ext["force"], ngp=False).tolist()))
    for i in range(len(f["post"])):
        q = q0 + int(t * FREQ)
        state = bytearray(80)
        struct.pack_into("<iii", state, 0, int(control[i, 4] * 32767), int(control[i, 0] * 65535), 0)
        struct.pack_into("<i", state, 20, int(control[i, 1] * 65535))
        w.pol(q + 10, 0, bytes(state))
        t += rc.STEP_102 * int(f["substeps"][i])
        q = q0 + int(t * FREQ)
        w.stp(q, int(f["pre"][i]), int(f["post"][i]), int(f["substeps"][i]))
        if ours is None:
            value = int(3000 * control[max(0, i - lag), 4])
        else:
            post = int(f["post"][i - 1 if i % 5 == 4 and i > 4 else i])
            value = mine.get(post, 0) + (1 if i == wrong else 0)
        w.eff(q + 5, 0, 2, value)
        if t >= next_clock:
            w.clk(q, UNIX0 + t)
            next_clock += 1.0
    w.record("END1", q0 + int(t * FREQ), b"")
    w.save(os.path.join(root, "1", "session.bin"))
    rate = 22050
    env = np.interp(np.arange(int((t + lead + 1) * rate)) / rate - lead, np.cumsum(f["substeps"]) * rc.STEP_102,
                    f["speed"] / max(f["speed"].max(), 1))
    rng = np.random.default_rng(1)
    write_wav(os.path.join(root, "audio.wav"), 0.3 * env * rng.standard_normal(len(env)), rate, float32=True)
    with open(os.path.join(root, "audio.txt"), "w") as fh:
        fh.write("start_unix_ns=%d\nrate=%d\nchannels=2\nformat=f32\nnode=synthetic\n" % ((UNIX0 - lead) * 1e9, rate))
    with open(os.path.join(root, "meta.txt"), "w") as fh:
        fh.write("date=synthetic\n[NGP]\nffbDeadZone=0\nffbDeadZoneThreshold=0\n")
    return f


class Parsing(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.path = os.path.join(self.dir, "s.bin")

    def test_clocks_steps_and_the_held_force(self):
        w = Writer()
        w.clk(1000, UNIX0)
        w.eff(1000, 3, 1, guid=CONSTANT)
        for k in range(10):
            w.stp(2000 + 100 * k, k, k + 1, 1)
        w.eff(2150, 3, 2, 500)  # after the StepForward that ended at step 2
        w.eff(2550, 3, 2, 700, direction=-1)  # after step 6's, a negative direction
        w.eff(2560, 4, 2, 900)  # not a constant force's effect
        w.clk(1000 + FREQ, UNIX0 + 1.0)
        w.save(self.path)
        s = rss.read(self.path)
        clock = rc.step_clock(s)
        self.assertEqual(list(rc.steps_at([1999, 2000, 2150, 2999], clock)), [-1, 1, 2, 10])
        sets, directions = rc.recorded_ffb(s)
        self.assertEqual(sets, [(2150, 500), (2550, -700)])
        self.assertEqual(directions, [-1, 1])
        q, m = zip(*sets)
        held = rc.held(rc.steps_at(q, clock), m, np.arange(1, 11))
        np.testing.assert_array_equal(held[:1], [np.nan])
        np.testing.assert_array_equal(held[1:], [500] * 4 + [-700] * 5)
        to_unix = rc.unix_of_qpc(s)
        self.assertAlmostEqual(float(to_unix(1000 + FREQ // 2)), UNIX0 + 0.5, places=6)

    def test_the_exact_ffb_rule(self):
        posts = np.array([10, 11, 12, 13])
        ours = np.array([100.0, 200.0, 300.0, 400.0])
        compared, match, offset, _, _ = rc.exact_ffb(np.array([11, 12, 12, 13, 14, 9, 10]),
                                                      np.array([200, 200, 250, 400, 0, 0, 300]), posts, ours)
        self.assertEqual(compared.tolist(), [True, True, True, True, False, False, True])
        self.assertEqual((match & compared).tolist(), [True, True, False, True, False, False, True])
        self.assertEqual(offset[match].tolist(), [0, 1, 0, -2])
        _, match, _, _, _ = rc.exact_ffb(np.array([10]), np.array([300]), posts, ours, reach=1)
        self.assertFalse(match[0])

    def test_axes_from_polls_buffers_and_ranges(self):
        w = Writer()
        w.dev(0, 2, b"Wheel")
        w.fmt(0, 2, 16, [(0, 0x102), (4, 0x202), (12, 0xC)])
        w.range(0, 2, 0, 0, -100, 100)  # every axis
        w.range(0, 2, 1, 4, 0, 1000)  # then Y alone
        for k in range(4):
            w.stp(100 * (k + 1), k, k + 1, 1)
        w.pol(50, 2, struct.pack("<iiii", -100, 10, 0, 0))
        w.pol(250, 2, struct.pack("<iiii", 50, 20, 0, 0))
        w.buf(260, 2, [(0, -7), (12, 128)])
        w.save(self.path)
        s = rss.read(self.path)
        devices = rc.device_axes(s)
        self.assertEqual(devices[2]["name"], "Wheel")
        self.assertEqual({o: a[1] for o, a in devices[2]["axes"].items()}, {0: (-100, 100), 4: (0, 1000)})
        values = rc.axis_values(s, devices, np.arange(0, 4), rc.step_clock(s))
        np.testing.assert_array_equal(values[(2, 0)], [-100, -100, -7, -7])
        np.testing.assert_array_equal(values[(2, 4)], [10, 10, 20, 20])

    def test_magnitude_and_dead_zone_are_the_rust_ones(self):
        # rbr_gameplay::ffb's tests.
        self.assertEqual(list(rc.magnitude([0.5, -1.0, 0.12345, 3.0], ngp=False)), [-5000, 10000, -1234, -10000])
        self.assertEqual(int(rc.magnitude([0.12345], ngp=True)[0]), -1235)
        self.assertEqual(int(rc.magnitude([0.5], strength=0.5, inverse=True, ngp=False)[0]), 2500)
        self.assertEqual(int(rc.magnitude([np.nan])[0]), 0)
        m = np.array([900, 1000, 900])
        self.assertEqual(list(rc.dead_zone(m, np.array([0.01, 0.01, 0.05]), 0.04, 0.1)), [0, 1000, 900])

    def test_lag_and_alignment(self):
        rng = np.random.default_rng(3)
        a = np.convolve(rng.standard_normal(3000), np.ones(20) / 20, "same")
        self.assertEqual(rc.lag_of(a, np.roll(a, 7), 50), 7)
        self.assertEqual(rc.lag_of(np.roll(a, 7), a, 50), -7)
        env = np.abs(np.convolve(rng.standard_normal(2000), np.ones(30) / 30, "same")) + 0.01
        recorded = np.concatenate([np.full(345, 0.001), env, np.full(100, 0.001)])
        offset, _ = rc.align(env, recorded, 100, 3.2, 1.0)
        self.assertAlmostEqual(offset, 3.45)

    def test_wav_png_and_the_report(self):
        x = np.sin(np.arange(1000) / 10) * 0.5
        for f32 in (False, True):
            path = os.path.join(self.dir, "a%d.wav" % f32)
            write_wav(path, x, 8000, float32=f32)
            y, rate = rc.read_wav(path)
            self.assertEqual((rate, y.shape), (8000, (1000, 2)))
            np.testing.assert_allclose(y[:, 0], x, atol=1e-4)
        png = os.path.join(self.dir, "p.png")
        ranges = rc.plot(png, np.arange(100), [(np.arange(100) ** 2, rc.BLUE)], mark=50, width=40, height=10)
        self.assertEqual(ranges, (0.0, 99.0, 0.0, 9801.0))
        with open(png, "rb") as f:
            d = f.read()
        self.assertEqual(d[:8], b"\x89PNG\r\n\x1a\n")
        w, h = struct.unpack_from(">II", d, 16)
        idat = d[d.index(b"IDAT") + 4:d.index(b"IEND") - 8]
        self.assertEqual((w, h, len(zlib.decompress(idat))), (40, 10, 10 * (1 + 40 * 3)))
        text = ("boot: from step 0, 3000 of 3000 steps run (Game::frame + pure physics), 2900 equal to the recording, "
                "1 snapshot pointer words\n  differs from the recording after step 1777: 3 words (car+0x144, "
                "wheel2+0x5bc, car+0x1c0)\n  words that differ at some step: 40 (car+0x144 (from step 1777, 3 steps))\n"
                "  stopped: the recording ends\n")
        r = rc.parse_report(text)
        self.assertEqual((r["run"], r["equal"], r["first_diff"], r["first_diff_words"], r["differing_words"]),
                         (3000, 2900, 1777, 3, 40))
        self.assertEqual(r["differing"], [("car+0x144", 1777, 3)])
        self.assertEqual([rc.friendly(n) for n in r["first_diff_names"]],
                         ["car+0x144 (position y)", "wheel2+0x5bc (strut travel x)", "car+0x1c0 (velocity x)"])

    def test_settings_and_the_mapping(self):
        st = rc.parse_settings("sensitivity=280,250,280\nsensitivity_source=[NGP],[NGP],[NGP]\ndead_zone=0.1\n"
                               "dead_zone_threshold=0\nffb_rate=1000\ninput_rate=500\nhead_gasket_durability=10\n"
                               "enabled=1\nstrength=0.5\ninverse=1\ngame_source=pfX.rbr\n")
        self.assertEqual((st["percent"], st["dead_zone"], st["input_rate"], st["strength"], st["inverse"]),
                         ([280, 250, 280], 0.1, 500, 0.5, True))
        st = rc.parse_settings("sensitivity=100,100,100\nsensitivity_source=default,default,default\n")
        self.assertEqual((st["enabled"], st["strength"], st["inverse"], st["game_source"]), (True, 1.0, False, None))
        # car_init::ffb_sensitivity: 110 at 250 % is 275; 100 and 0 leave S, and S stays at least 1.
        self.assertEqual([rc.scaled_sensitivity(110.0, p) for p in (250, 100, 0)], [275.0, 110.0, 110.0])
        self.assertEqual(rc.scaled_sensitivity(1.5, 10), 1.0)
        # ngp::ffb::curve: half force near |T| = 27.37 at S = 100, gated 0, the torque's sign.
        x = rc.ngp_curve([27.37, -90.9, 50.0, 0.0], np.array([False, False, True, False]), 100.0)
        self.assertLess(abs(x[0] - 0.5), 1e-3)
        self.assertLess(abs(x[1] + 0.9), 1e-3)
        self.assertEqual((x[2], x[3]), (0.0, 0.0))
        # A map RSF's list lacks: the exe's table.
        with mock.patch.dict(os.environ, {"RBR_RSF": ""}):
            self.assertEqual([rc.surface_of(m) for m in (10, 0x29, 54)], [2, 1, 0])


def s_post(session, i):
    return int(rc.fields(rc.Frames(os.path.join(session, "1", "physics.bin")))["post"][i])


class EndToEnd(unittest.TestCase):
    def test_a_synthetic_session(self):
        recordings = os.environ.get("RBR_RECORDINGS")
        tools = [os.path.join(rc.ROOT, "target", "release", p) for p in ("rbr-standalone", "rbr-sound")]
        if not (recordings and os.environ.get("RBR_DATA") and os.environ.get("RBR_PATCH") and
                all(map(os.path.exists, tools))):
            self.skipTest("RBR_DATA, RBR_PATCH, RBR_RECORDINGS or the release rbr-standalone/rbr-sound missing")
        root = tempfile.mkdtemp()
        session, out = os.path.join(root, "session"), os.path.join(root, "report")
        drive = os.path.join(recordings, "drive-tanner-corolla.bin")
        # Our force first, which the made-up FFB sets take.
        head = os.path.join(root, "head.bin")
        with open(head, "wb") as f:
            f.write(frames_bytes(drive, 3000))
        trace = os.path.join(root, "ours.bin")
        subprocess.run([tools[0], "--boot", "--recording", head, "--stage", "track-54_E", "--car", "6", "--weather",
                        "0", "--tyre", "4", "--steps", "3000", "--trace-out", trace], capture_output=True)  # exits 1: the head ends
        synthetic_session(session, drive, 3000, ours=trace + ".ext", wrong=1500)
        rc.main([session, "--out", out])
        import json
        s = json.load(open(os.path.join(out, "1", "summary.json")))
        p = s["physics"]
        self.assertEqual(p["stage"], ["--stage", "track-54_E"])
        self.assertEqual(p["position_error_max"], 0.0)
        self.assertEqual((p["run"], p["equal"], p["frames"]), (3000, 3000, 3000))
        self.assertNotIn("first_diff", p)
        self.assertGreater(s["input"]["channels"]["steering"]["correlation"], 0.999)
        self.assertEqual(s["input"]["channels"]["steering"]["ofs"], 0)
        self.assertEqual(s["input"]["channels"]["throttle"]["ofs"], 4)
        self.assertEqual(s["ffb"]["sets"], 3000)
        exact = s["ffb"]["exact"]
        self.assertEqual((exact["compared"], exact["matched"], exact["pass"]), (3000, 2999, False))
        self.assertEqual([m["step"] for m in exact["mismatches"]], [s_post(session, 1500)])
        self.assertEqual([x["name"] for x in s["ffb"]["sections"]], ["start", "impact", "slide"])
        c = s["checks"]
        self.assertEqual((c["physics"]["pass"], c["ffb"]["pass"], c["input"]["pass"], c["sound"]["expectation"]),
                         (True, False, None, "approximate"))
        self.assertGreater(s["sound"]["seconds"], 10)
        self.assertTrue(os.path.exists(os.path.join(out, "1", "spectrogram-recorded.png")))
        self.assertIn("track-54_E", open(os.path.join(out, "index.md")).read())


if __name__ == "__main__":
    unittest.main()
