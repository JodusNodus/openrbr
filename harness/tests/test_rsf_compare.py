"""Tests of harness/rsf_compare.py: the parsing and alignment on synthetic RSS1 files, and the whole tool on a
synthetic session made of an existing drive's first frames with a made-up RSS1 beside it (needs RBR_DATA, RBR_PATCH,
RBR_RECORDINGS and the release rbr-standalone and rbr-sound; skipped without them), and drive2's attempt 5 under NGP
(also RBR_NGP_PHYSICS and RBR_RSF).

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


DIPROP_RANGE = 4  # DirectInput's MAKEDIPROP(4)


class Writer:
    """An RSS1 file in memory, as the plugin writes it (harness/session.py's docstring)."""

    def __init__(self):
        self.out = bytearray(b"RSS1" + struct.pack("<IQ", 1, FREQ))

    def record(self, tag, qpc, body, thread=7):
        payload = struct.pack("<QI", qpc, thread) + body
        self.out += tag.encode() + struct.pack("<I", len(payload)) + payload

    def clk(self, qpc, unix):
        self.record("CLK1", qpc, struct.pack("<Q", int(round(unix * 1e7)) + rss.FILETIME_UNIX))

    def stp(self, qpc, pre, post, substeps):
        self.record("STP1", qpc, struct.pack("<3I", pre, post, substeps))

    def gam(self, qpc, game, car, weather, tyre, base=0, size=0, version=b"", damage=None):
        body = struct.pack("<7I", game, car, weather, tyre, base, size, len(version)) + version
        self.record("GAM1", qpc, body + (struct.pack("<I", damage) if damage is not None else b""))

    def opn(self, qpc, path):
        self.record("OPN1", qpc, struct.pack("<3I", 0, 1, len(path)) + path)

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
        self.record("PRP1", qpc, struct.pack("<7I", device, 0, DIPROP_RANGE, how, obj, 0, len(data)) + data)

    def pol(self, qpc, device, state):
        self.record("POL1", qpc, struct.pack("<3I", device, 0, len(state)) + state)

    def buf(self, qpc, device, events):
        self.record("BUF1", qpc, struct.pack("<3I", device, 0, len(events)) +
                    b"".join(struct.pack("<5I", ofs, data & 0xFFFFFFFF, 0, 0, 0) for ofs, data in events))

    def eff(self, qpc, effect, op, magnitude=None, direction=1, guid=b"", arg=0x141, gain=10000, call=None):
        """`arg` the DIEP flags of a SetParameters (NGP's: TYPESPECIFICPARAMS, DIRECTION, DURATION); `call` the
        call's QPC (sessions before it have none)."""
        body = struct.pack("<5I", 0, effect, op, arg if op == 2 else 0, 0) + (guid or bytes(16))
        if magnitude is None:
            body += struct.pack("<I", 0)
        else:  # flags, duration, sample period, gain, trigger button and repeat; one axis, its direction; no envelope
            body += struct.pack("<I6I", 1, 0x10, 0xFFFFFFFF, 0, gain, 0xFFFFFFFF, 0)
            body += struct.pack("<IIi", 1, 0, direction) + struct.pack("<II", 0, 4) + struct.pack("<i", magnitude)
            body += struct.pack("<I", 0)
        if call is not None:
            body += struct.pack("<Q", call)
        self.record("EFF1", qpc, body)

    def sta(self, qpc, post, words):
        """STA1 of the .ext's u32 words (engine, gear, slip x4) at their offsets (rc.STATE_WORDS)."""
        body = struct.pack("<2I", post, len(rc.STATE_WORDS))
        for (_, region, off), v in zip(rc.STATE_WORDS, words):
            body += struct.pack("<2I", region, off + 4) + bytes(off) + struct.pack("<I", v)
        self.record("STA1", qpc, body)

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


def synthetic_session(root, recording, n, lag=7, lead=2.0, ours=None, wrong=None, sta_wrong=None):
    """A session from `recording`'s first `n` frames: STP1 at 7 ms a substep, CLK1 once a second, a wheel polled each
    frame from the payload (X steering over -32768..32767, Y throttle and Rz brake over 0..65535), audio.wav from `lead`
    s before the first step. The constant force is set after each step to 3000 x steering `lag` steps before, or with
    `ours` (a --trace-out .ext) to our magnitude at that step (0 outside the racing gate), at the step before on every
    fifth when that is a step of ours (NGP's FFB thread racing the step), and off by one after frame `wrong`; with
    `ours` STA1 holds its state words, one bit off after frame `sta_wrong`."""
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
        gated = np.where(ext["state"] == rc.DRIVING, ext["magnitude"], 0)
        mine = dict(zip(ext["post"].tolist(), gated.tolist()))
        words = dict(zip(ext["post"].tolist(), ext["words"][:, 2:8].tolist()))
    for i in range(len(f["post"])):
        q = q0 + int(t * FREQ)
        state = bytearray(80)
        struct.pack_into("<iii", state, 0, int(control[i, 4] * 32767), int(control[i, 0] * 65535), 0)
        struct.pack_into("<i", state, 20, int(control[i, 1] * 65535))
        w.pol(q + 10, 0, bytes(state))
        t += rc.STEP_102 * int(f["substeps"][i])
        q = q0 + int(t * FREQ)
        w.stp(q, int(f["pre"][i]), int(f["post"][i]), int(f["substeps"][i]))
        if ours is not None and int(f["post"][i]) in words:
            state = list(words[int(f["post"][i])])
            state[5] ^= 1 if i == sta_wrong else 0
            w.sta(q + 1, int(f["post"][i]), state)
        if ours is None:
            value = int(3000 * control[max(0, i - lag), 4])
        else:
            post = int(f["post"][i])
            if i % 5 == 4 and post - 1 in mine:
                post -= 1
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

    def test_the_damage_level_when_recorded(self):
        w = Writer()
        w.gam(1000, 54, 6, 0, 4, version=b"7.5")
        w.gam(2000, 54, 6, 0, 4, version=b"7.5", damage=3)
        w.save(self.path)
        old, new = rss.read(self.path).tagged("GAM1")
        self.assertNotIn("damage", old)
        self.assertEqual((new["ngp_version"], new["damage"]), ("7.5", 3))

    def test_the_pack_the_stage_load_opened(self):
        w = Writer()
        w.opn(1000, b"Games\\Richard Burns Rally\\rsfdata\\cars\\Evo_IX_ngp6\\setups\\d_gravel.lsp")
        w.opn(1500, b"SavedGames\\Evo_IX_ngp6\\a.lsp")
        w.stp(2000, 0, 40, 40)
        w.opn(3000, b"rsfdata/cars/Next_Car_ngp6/setups/d_tarmac.lsp")  # the next car's install, after the attempt
        w.save(self.path)
        self.assertEqual(rc.loaded_pack(rss.read(self.path)), "Evo_IX_ngp6")

    def test_the_diep_flags_and_the_gain(self):
        w = Writer()
        w.eff(1000, 3, 1, 0, guid=CONSTANT)
        w.eff(2000, 3, 2, 500, arg=0x40, direction=-1)  # direction only: the magnitude held (0), negated
        w.eff(3000, 3, 2, 700, arg=0x100, direction=1)  # magnitude only: the direction held (negative)
        w.eff(4000, 3, 2, 900, call=3500)  # both, at the call's QPC
        w.eff(5000, 3, 2, 100, arg=0x8141, gain=10000)
        w.save(self.path)
        sets, directions = rc.recorded_ffb(rss.read(self.path))
        self.assertEqual(sets, [(2000, 0), (3000, -700), (3500, 900), (5000, 100)])
        self.assertEqual(directions, [-1, 1])
        w.eff(6000, 3, 2, 100, arg=0x145, gain=5000)  # DIEP_GAIN to half: not what reaches the wheel
        w.save(self.path)
        with self.assertRaisesRegex(rc.Failure, "gain is 5000"):
            rc.recorded_ffb(rss.read(self.path))

    def test_the_ffb_classes(self):
        # STP1s (QPC, post): a settle step, three single steps 1.4 ms apart, a catch-up burst of three 0.3 ms apart,
        # a single step, then the end. Our run: posts 40-48, not driving at 40.
        ms = FREQ // 1000
        qs = np.array([0, 20 * ms, 21.4 * ms, 22.8 * ms, 30 * ms, 30.3 * ms, 30.6 * ms, 32 * ms], dtype=np.int64)
        posts_rec = np.array([40, 41, 42, 43, 44, 45, 46, 47], dtype=np.int64)
        clock = (qs, posts_rec)
        posts = np.arange(40, 49)
        state = np.array([0] + [rc.DRIVING] * 8)
        ours = np.array([0, 410, 420, 430, 440, 450, 460, 470, 480], dtype=np.float64)
        first_driving = int(qs[1])
        sets = [  # (QPC, value, class, matches)
            (10 * ms, 0, "gate", True),             # after the settle: the gate's 0
            (19 * ms, 77, "gate", True),            # the carried force, in the window before the first driving step
            (19.5 * ms, 0, "gate", False),          # 0 again after it: the gate does not close
            (int(20.1 * ms), 410, "plain", True),   # ours at its step
            (int(20.2 * ms), 77, "plain", True),    # the first driving step's step before: the carried force
            (int(21.5 * ms), 410, "plain", True),   # the step before
            (int(22.9 * ms), 400, "plain", False),  # a value no step near has
            (int(22.9 * ms), 410, "plain", False),  # three steps... two before: too far
            (int(30.1 * ms), 430, "burst", True),   # inside a burst: the force stored before it
            (int(30.1 * ms), 440, "burst", False),  # not the burst step's own
            (int(30.7 * ms), 460, "burst", True),   # the burst's last step: its own
            (int(30.7 * ms), 430, "burst", True),   # or the one before the burst
            (int(33 * ms), 0, "after", True),       # after the last step: 0
            (int(33 * ms), 470, "after", True),     # or the last step's
        ]
        q = np.array([v[0] for v in sets], dtype=np.int64)
        values = np.array([v[1] for v in sets], dtype=np.float64)
        cls, rows, literal, window = rc.ffb_candidates(q, clock, FREQ, posts, state, first_driving)
        self.assertEqual(list(cls), [v[2] for v in sets])
        match = rc.match_sets(values, rows, literal, window, ours, [77.0, 66.0])
        self.assertEqual(match.tolist(), [v[3] for v in sets])
        # The carried force unknown: only the gate's 0 there.
        match = rc.match_sets(values, rows, literal, window, ours, None)
        self.assertEqual(match[:3].tolist(), [True, False, True])
        # Our state not driving: the gate's 0, not our magnitude.
        cls2, rows2, literal2, window2 = rc.ffb_candidates(q[3:4], clock, FREQ, posts,
                                                           np.zeros(len(posts), dtype=int), first_driving)
        self.assertEqual(list(cls2), ["gate"])
        self.assertEqual(rc.match_sets([410.0], rows2, literal2, window2, ours, [0.0]).tolist(), [False])
        self.assertEqual(rc.match_sets([0.0], rows2, literal2, window2, ours, [0.0]).tolist(), [True])
        # Ours shifted by three steps fails everywhere the force moves.
        shifted = np.r_[ours[3:], [0, 0, 0]]
        good = np.array([v[3] for v in sets])
        self.assertFalse(rc.match_sets(values, rows, literal, window, shifted, [77.0])[good][3:-2].all())
        # A set past our run is not_run, and the whole check fails with its count.
        cls3, rows3, literal3, window3 = rc.ffb_candidates(q[3:4], clock, FREQ, posts[:1], state[:1], first_driving)
        self.assertEqual(list(cls3), ["not_run"])
        e, _ = rc.class_check(q[3:4], values[3:4], cls3, rows3, literal3, window3, ours[:1], [0.0], q[3:4])
        self.assertFalse(e["pass"])
        self.assertIn("1 sets past the steps we ran", e["reason"])
        e, _ = rc.class_check(q, values, cls, rows, literal, window, ours, [77.0], q)
        self.assertEqual((e["compared"], e["matched"], e["pass"]), (14, 10, False))
        self.assertEqual((e["gate"]["matched"], e["plain"]["matched"], e["burst"]["matched"]), (2, 3, 3))
        keep = good
        e, _ = rc.class_check(q[keep], values[keep], cls[keep], rows[keep], literal[keep], window[keep], ours,
                              [77.0], q[keep])
        self.assertTrue(e["pass"])
        e, _ = rc.class_check(q[:0], values[:0], cls[:0], rows[:0], literal[:0], window[:0], ours, [0.0], q[:0])
        self.assertEqual((e["pass"], e["reason"]), (False, "no set compared"))

    def test_the_carried_force(self):
        out = os.path.join(self.dir, "report")
        os.makedirs(os.path.join(out, "2"))
        self.assertEqual(rc.carried_force(os.path.join(out, "1"), "magnitude"), [0.0])
        self.assertIsNone(rc.carried_force(os.path.join(out, "2"), "magnitude"))  # attempt 1 not compared
        os.makedirs(os.path.join(out, "1"))
        with open(os.path.join(out, "1", "summary.json"), "w") as f:
            f.write('{"ffb": {"last": {"magnitude": [-1586, -1590]}}}')
        self.assertEqual(rc.carried_force(os.path.join(out, "2"), "magnitude"), [-1586.0, -1590.0])

    def test_failures_and_reasons(self):
        rep = {"run": 3000, "equal": 3000}
        self.assertEqual(rc.run_reasons(0, rep, 3000), [])
        self.assertEqual(rc.run_reasons(1, dict(rep, control_diff="step 5 word 4"), 3000),
                         ["rbr-standalone exited 1 (control diff step 5 word 4)"])
        self.assertIn("ended after 2000 of 3000", rc.run_reasons(0, dict(rep, run=2000, ended="finished"), 3000)[0])
        self.assertEqual(rc.run_reasons(0, dict(rep, run=2000), 3000), ["ran 2000 of 3000 frames"])
        # A section's error is its FAIL with the reason; one not run is a FAIL too.
        summary, lines = rc.section("Force feedback", lambda: rc.ffb(None, {}, self.dir))
        self.assertIn("no trace of ours", summary["error"])
        c = rc.checks({"attempt": 1, "physics": {"error": "no GAM1 record"}, "ffb": summary},
                      ("physics", "ffb", "sound"))
        self.assertEqual((c["physics"]["pass"], c["physics"]["reason"]), (False, "no GAM1 record"))
        self.assertEqual((c["sta1"]["pass"], c["sta1"]["reason"]), (False, "no GAM1 record"))
        self.assertFalse(c["ffb"]["pass"])
        self.assertIn("no trace of ours", c["ffb"]["reason"])
        self.assertEqual((c["input"]["pass"], c["input"]["reason"]), (False, "not run (--sections)"))
        # The physics pass needs its own pass and STA1's.
        ok = {"pass": True, "reasons": [], "sta1": {"pass": True}}
        self.assertTrue(rc.checks({"attempt": 1, "physics": ok}, ("physics",))["physics"]["pass"])
        bad = dict(ok, **{"pass": False, "reasons": ["STA1: 1 of 9 steps differ"]})
        self.assertEqual(rc.checks({"attempt": 1, "physics": bad}, ("physics",))["physics"]["reason"],
                         "STA1: 1 of 9 steps differ")
        # Input: PASS on every DLV1 delivery equal, PASS-BY-SEARCH without DLV1 (shown), FAIL else.
        calls = {"compared": 100, "equal": 100, "equal_by_time": 97, "dlv1": 0}
        self.assertEqual(rc.input_state(calls)[0], "PASS-BY-SEARCH")
        self.assertEqual(rc.input_state(dict(calls, equal=99)),
                         ("FAIL", "no DLV1, and 1 of 100 calls differ after the delivery search"))
        dlv = dict(calls, equal=3, dlv1=120, deliveries={"equal": 120, "compared": 120})
        self.assertEqual(rc.input_state(dlv), ("PASS", None))
        self.assertEqual(rc.input_state(dict(dlv, deliveries={"equal": 119, "compared": 120})),
                         ("FAIL", "1 of 120 deliveries differ"))
        self.assertEqual(rc.input_state(dict(dlv, deliveries=None))[0], "FAIL")  # input_check without the pairing
        c = rc.checks({"attempt": 1, "input": dict(calls, state="PASS-BY-SEARCH", **{"pass": True})},
                      ("input",))
        self.assertEqual((c["input"]["pass"], rc.verdict(c["input"]), c["input"]["counts"]),
                         (True, "PASS-BY-SEARCH", "100/100 calls by search, 97 by QPC"))
        # A pack with no RSF id is a FAIL.
        with self.assertRaises(rc.Failure):
            rc.pack_id("No_Such_Car_ngp6")

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
                        "0", "--tyre", "4", "--steps", "3000", "--trace-out", trace], capture_output=True)
        synthetic_session(session, drive, 3000, ours=trace + ".ext", wrong=1500)
        rc.main([session, "--out", out])
        import json
        s = json.load(open(os.path.join(out, "1", "summary.json")))
        p = s["physics"]
        self.assertEqual(p["stage"], ["--stage", "track-54_E"])
        self.assertEqual(p["position_error_max"], 0.0)
        self.assertEqual((p["run"], p["equal"], p["frames"], p["exit"]), (3000, 3000, 3000, 0))
        self.assertNotIn("first_diff", p)
        self.assertEqual((p["sta1"]["pass"], p["sta1"]["compared"] > 2900), (True, True))
        self.assertEqual(s["ffb"]["sets"], 3000)
        exact = s["ffb"]["exact"]
        self.assertEqual((exact["compared"], exact["matched"], exact["not_run"], exact["pass"]), (3000, 2999, 0, False))
        self.assertEqual([m["step"] for m in exact["mismatches"]], [s_post(session, 1500)])
        self.assertEqual([x["name"] for x in s["ffb"]["sections"]], ["start", "impact", "slide"])
        c = s["checks"]
        self.assertEqual((c["physics"]["pass"], c["sta1"]["pass"], c["ffb"]["pass"], c["ffb_mapping"]["pass"],
                          c["sound"]["expectation"]), (True, True, False, False, "approximate"))
        self.assertIn("1 of 3000 sets differ (plain 1)", c["ffb"]["reason"])
        self.assertIn("no NGP", c["ffb_mapping"]["reason"])
        self.assertFalse(c["input"]["pass"])  # 1.02: no NGP input chain to check
        self.assertTrue(c["input"]["reason"])
        self.assertGreater(s["sound"]["seconds"], 10)
        self.assertTrue(os.path.exists(os.path.join(out, "1", "spectrogram-recorded.png")))
        self.assertIn("track-54_E", open(os.path.join(out, "index.md")).read())
        # One STA1 bit off: STA1 and so physics FAIL, at that step.
        session2, out2 = os.path.join(root, "session2"), os.path.join(root, "report2")
        synthetic_session(session2, drive, 3000, ours=trace + ".ext", sta_wrong=2000)
        rc.main([session2, "--out", out2, "--sections", "physics"])
        c = json.load(open(os.path.join(out2, "1", "summary.json")))["checks"]
        self.assertEqual((c["physics"]["pass"], c["sta1"]["pass"]), (False, False))
        self.assertIn("1 of", c["sta1"]["reason"])
        self.assertIn("(slip3)", c["sta1"]["reason"])
        self.assertIn("STA1:", c["physics"]["reason"])

    def test_an_ngp_session(self):
        # drive2 attempt 5 (NGP 7.5.779, snow): every step equal, the recorded car's force at every set with our S
        # the files'; STA1 lacks the engine and drive regions, and attempt 4 is not compared, so its carried force is
        # unknown: those FAIL with their reasons.
        recordings = os.environ.get("RBR_RECORDINGS")
        session = os.path.join(recordings or "", "rsf", "drive2")
        tool = os.path.join(rc.ROOT, "target", "release", "rbr-standalone")
        if not (os.path.exists(os.path.join(session, "5", "session.bin")) and os.environ.get("RBR_NGP_PHYSICS") and
                os.environ.get("RBR_RSF") and os.path.exists(tool)):
            self.skipTest("RBR_RECORDINGS's rsf/drive2, RBR_NGP_PHYSICS, RBR_RSF or the release rbr-standalone missing")
        out = tempfile.mkdtemp()
        rc.main([session, "--out", out, "--attempt", "5", "--sections", "physics,ffb", "--damage", "safe"])
        import json
        s = json.load(open(os.path.join(out, "5", "summary.json")))
        c = s["checks"]
        self.assertEqual((s["physics"]["exit"], s["physics"]["equal"], s["physics"]["frames"]), (0, 84084, 84084))
        self.assertEqual((c["physics"]["pass"], c["sta1"]["pass"]), (False, False))
        self.assertIn("never holds engine, gear", c["sta1"]["reason"])
        mapping = s["ffb"]["mapping"]
        self.assertEqual((c["ffb_mapping"]["pass"], mapping["sensitivity_ours"]), (False, [mapping["sensitivity"]]))
        self.assertIn("previous attempt's force is unknown", c["ffb_mapping"]["reason"])
        self.assertEqual(mapping["compared"] - mapping["matched"], mapping["gate"]["compared"] -
                         mapping["gate"]["matched"])


if __name__ == "__main__":
    unittest.main()
