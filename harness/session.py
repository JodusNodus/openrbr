"""session.py FILE...: summary of RSS1 session files (record counts per tag, devices, effects, callers, files, step
range, time span). session.py --files DIR FILE: write the FIL1 files' contents to DIR (by base name).
Importable: read(path) -> Session (header and records, each record's payload parsed per tag; an unknown tag's payload
is kept as "raw" bytes).

RSS1 is the harness plugin's session file (RBRRecompSession.on, notes/verification.md "RSF sessions"): one per stage
attempt, RBRRecompSession-<tick>.bin next to the FRM1 recording RBRRecompRecord-<tick>.bin of the same tick.
Little-endian. Header: 'RSS1', u32 version (1), u64 QPC frequency. Then records to EOF: u32 tag, u32 payload length,
payload. Every payload starts with u64 QPC (QueryPerformanceCounter) and u32 thread id (GetCurrentThreadId); then:

  CLK1  u64 FILETIME (GetSystemTimeAsFileTime, UTC); at the start and once a second
  STP1  u32 pre, u32 post (the FRM1 step counters), u32 substeps; one per StepForward call, after it
  GAM1  u32 map, u32 car slot, u32 weather, u32 tyre, u32 NGP.dll image base, u32 image size, u32 n + n bytes of
        NGP.dll's version string (empty if none); at stage start. Fields are only ever added at the end.
  DEV1  u32 device, GUID instance, GUID product, u32 n + n bytes name (ANSI); per device, on CreateDevice or first use
  FMT1  u32 device, u32 data size, u32 count, count x (GUID or 16 zero bytes, u32 ofs, u32 type, u32 flags)
  PRP1  u32 device, u32 op (0 Set, 1 GetProperty), u32 property (MAKEDIPROP value, 0xffffffff for a GUID one), u32 how,
        u32 obj, u32 hresult, u32 n + n bytes after the DIPROPHEADER
  POL1  u32 device, u32 hresult, u32 size, GetDeviceState bytes
  BUF1  u32 device, u32 hresult, u32 count, count x DIDEVICEOBJECTDATA (ofs, data, timestamp, sequence, app data)
  EFF1  u32 device, u32 effect, u32 op (1 create, 2 SetParameters, 3 Start, 4 Stop, 5 Download, 6 Unload,
        7 SendForceFeedbackCommand, 8 Release), u32 arg (DIEP flags, Start iterations or the command), u32 hresult,
        GUID type (create, else zeros), u32 has_effect, then when 1: u32 flags, duration, sample period, gain,
        trigger button, trigger repeat, axes, axes x u32 axis ids, axes x i32 directions, u32 start delay,
        u32 n + n type-specific bytes, u32 has_envelope and when 1 the 4 DIENVELOPE u32s after dwSize (attack level,
        attack time, fade level, fade time)
  STA1  u32 post, u32 count, count x (u32 id, u32 n, n bytes); after each STP1. The state the car's sound reads, which
        FRM1 does not hold: 1 the engine state (*(CCar +0x690 +8), 0x274 bytes: speed +0x188), 2 the drive state
        (*(CCar +0x4b0 +8), 0x580 bytes: gear +0x54c), 3-6 wheel k's tyre-noise block (wheel +0xf00 +0x420, 0x60
        bytes: |slip ratio| at +0x20); n is 0 when the pointer could not be read
  CTL1  u32 step counter when taken (0xffffffff before the first step), 0x20 bytes: the payload the car's control input
        took (NGP's call, or the 1.02 Emit), the one FRM1 stores for the next step
  MOD1  u32 count, count x (u32 base, u32 size, u32 n + n bytes module name, u32 kinds, kinds x u32 calls): calls per
        calling module since the attempt started, kinds in MOD_KINDS order; every 10 s and before END1 (the last one
        holds the attempt's totals)
  OPN1  u32 access, u32 opened, u32 n + n bytes path: a .lsp or .acm file opened by a module in the game folder
  FIL1  u32 n + n bytes path, u32 n + n bytes contents (at most 1 MiB): an .acm's when opened, and at the start of an
        attempt the .lsp files its stage load opened
  END1  end of the stage attempt

Device numbers count the devices the plugin has seen in the process, effect numbers the effects it has created.
A device's first record may be its DEV1 from "first use" (made before the hooks); an effect seen first on use has
device 0xffffffff. Setup records (DEV1, FMT1, PRP1 sets, EFF1 creates, the .acm's OPN1 and FIL1) are repeated at the
start of every later attempt's file, and a stage load's OPN1 at the start of its attempt's, with their own QPC."""
import struct
import sys
import uuid
from collections import Counter

MOD_KINDS = ("POL1", "BUF1", "EFF1", "PRP1", "FMT1", "DEV1", "ReadFile", "WriteFile", "DeviceIoControl",
             "GetRawInputData", "SendInput", "XInputGetState", "XInputSetState")
EFFECT_OPS = {1: "create", 2: "set", 3: "start", 4: "stop", 5: "download", 6: "unload", 7: "command", 8: "release"}
# GUID_ConstantForce etc. (dinput.h)
EFFECT_GUIDS = {
    "13541c20-8e33-11d0-9ad0-00a0c9a06e35": "ConstantForce",
    "13541c21-8e33-11d0-9ad0-00a0c9a06e35": "RampForce",
    "13541c22-8e33-11d0-9ad0-00a0c9a06e35": "Square",
    "13541c23-8e33-11d0-9ad0-00a0c9a06e35": "Sine",
    "13541c24-8e33-11d0-9ad0-00a0c9a06e35": "Triangle",
    "13541c25-8e33-11d0-9ad0-00a0c9a06e35": "SawtoothUp",
    "13541c26-8e33-11d0-9ad0-00a0c9a06e35": "SawtoothDown",
    "13541c27-8e33-11d0-9ad0-00a0c9a06e35": "Spring",
    "13541c28-8e33-11d0-9ad0-00a0c9a06e35": "Damper",
    "13541c29-8e33-11d0-9ad0-00a0c9a06e35": "Inertia",
    "13541c2a-8e33-11d0-9ad0-00a0c9a06e35": "Friction",
    "13541c2b-8e33-11d0-9ad0-00a0c9a06e35": "CustomForce",
}


class Reader:
    def __init__(self, data, at=0):
        self.data, self.at = data, at

    def u32(self, n=None):
        if n is not None:
            return [self.u32() for _ in range(n)]
        (v,) = struct.unpack_from("<I", self.data, self.at)
        self.at += 4
        return v

    def i32(self, n):
        v = list(struct.unpack_from("<%di" % n, self.data, self.at))
        self.at += 4 * n
        return v

    def u64(self):
        (v,) = struct.unpack_from("<Q", self.data, self.at)
        self.at += 8
        return v

    def guid(self):
        g = uuid.UUID(bytes_le=bytes(self.data[self.at:self.at + 16]))
        self.at += 16
        return str(g)

    def bytes(self, n):
        b = bytes(self.data[self.at:self.at + n])
        self.at += n
        return b

    def blob(self):
        return self.bytes(self.u32())


def parse(tag, r):
    """The fields of one payload after its QPC and thread id."""
    if tag == "CLK1":
        return {"filetime": r.u64()}
    if tag == "STP1":
        pre, post, substeps = r.u32(3)
        return {"pre": pre, "post": post, "substeps": substeps}
    if tag == "GAM1":
        names = ("map", "car", "weather", "tyre", "ngp_base", "ngp_size")
        d = dict(zip(names, r.u32(len(names))))
        d["ngp_version"] = r.blob().decode("latin-1")
        return d
    if tag == "DEV1":
        return {"device": r.u32(), "instance": r.guid(), "product": r.guid(), "name": r.blob().decode("latin-1")}
    if tag == "FMT1":
        device, size, count = r.u32(3)
        objects = []
        for _ in range(count):
            g = r.guid()
            ofs, typ, flags = r.u32(3)
            objects.append({"guid": g, "ofs": ofs, "type": typ, "flags": flags})
        return {"device": device, "size": size, "objects": objects}
    if tag == "PRP1":
        names = ("device", "op", "property", "how", "obj", "hresult")
        d = dict(zip(names, r.u32(len(names))))
        d["data"] = r.blob()
        return d
    if tag == "POL1":
        device, hresult = r.u32(2)
        return {"device": device, "hresult": hresult, "state": r.blob()}
    if tag == "BUF1":
        device, hresult, count = r.u32(3)
        return {"device": device, "hresult": hresult, "events": [tuple(r.u32(5)) for _ in range(count)]}
    if tag == "EFF1":
        names = ("device", "effect", "op", "arg", "hresult")
        d = dict(zip(names, r.u32(len(names))))
        d["type"] = r.guid()
        d["params"] = None
        if r.u32():
            names = ("flags", "duration", "sample_period", "gain", "trigger_button", "trigger_repeat")
            p = dict(zip(names, r.u32(len(names))))
            axes = r.u32()
            p["axes"] = r.u32(axes)
            p["directions"] = r.i32(axes)
            p["start_delay"] = r.u32()
            p["specific"] = r.blob()
            p["envelope"] = r.u32(4) if r.u32() else None
            d["params"] = p
        return d
    if tag == "STA1":
        post, count = r.u32(2)
        regions = {}
        for _ in range(count):
            region = r.u32()
            regions[region] = r.blob()
        return {"post": post, "regions": regions}
    if tag == "CTL1":
        return {"step": r.u32(), "payload": r.bytes(0x20)}
    if tag == "MOD1":
        modules = []
        for _ in range(r.u32()):
            base, size = r.u32(2)
            name = r.blob().decode("latin-1")
            calls = r.u32(r.u32())
            modules.append({"base": base, "size": size, "name": name, "calls": dict(zip(MOD_KINDS, calls))})
        return {"modules": modules}
    if tag == "OPN1":
        access, opened = r.u32(2)
        return {"access": access, "opened": opened, "path": r.blob().decode("latin-1")}
    if tag == "FIL1":
        return {"path": r.blob().decode("latin-1"), "contents": r.blob()}
    return {"raw": r.bytes(len(r.data) - r.at)}


class Session:
    def __init__(self, version, qpc_frequency, records):
        self.version, self.qpc_frequency, self.records = version, qpc_frequency, records

    def tagged(self, tag):
        return [r for r in self.records if r["tag"] == tag]

    def seconds(self, qpc):
        return qpc / self.qpc_frequency


def read(path):
    """The session at path; a record cut off at the end (a killed game) is dropped."""
    data = open(path, "rb").read()
    if data[:4] != b"RSS1":
        raise ValueError("%s: not an RSS1 file" % path)
    version, frequency = struct.unpack_from("<IQ", data, 4)
    at, records = 16, []
    while at + 8 <= len(data):
        tag, n = struct.unpack_from("<4sI", data, at)
        if at + 8 + n > len(data):
            break
        r = Reader(data[at + 8:at + 8 + n])
        rec = {"tag": tag.decode("latin-1"), "qpc": r.u64(), "thread": r.u32()}
        rec.update(parse(rec["tag"], r))
        records.append(rec)
        at += 8 + n
    return Session(version, frequency, records)


def summary(s):
    lines = ["RSS1 version %d, QPC %d Hz, %d records" % (s.version, s.qpc_frequency, len(s.records))]
    counts = Counter(r["tag"] for r in s.records)
    lines.append("tags: " + ", ".join("%s %d" % kv for kv in sorted(counts.items())))
    if s.records:
        t0, t1 = s.records[0]["qpc"], s.records[-1]["qpc"]
        lines.append("time: %.3f s (QPC %d .. %d)" % (s.seconds(t1 - t0), t0, t1))
    clocks = s.tagged("CLK1")
    if clocks:
        unix = (clocks[0]["filetime"] - 116444736000000000) / 1e7
        lines.append("clock: %d CLK1, first at unix %.3f" % (len(clocks), unix))
    for g in s.tagged("GAM1"):
        lines.append("game: map %(map)d car %(car)d weather %(weather)d tyre %(tyre)d, NGP.dll %(ngp_base)08x"
                     " size %(ngp_size)x version %(ngp_version)r" % g)
    steps = s.tagged("STP1")
    if steps:
        sub = Counter(r["substeps"] for r in steps)
        lines.append("steps: %d calls, pre %d .. post %d, substeps %s" % (
            len(steps), steps[0]["pre"], steps[-1]["post"], dict(sorted(sub.items()))))
    threads = Counter(r["thread"] for r in s.records)
    lines.append("threads: " + ", ".join("%d (%d)" % kv for kv in threads.most_common()))
    for d in s.tagged("DEV1"):
        tags = Counter(r["tag"] for r in s.records if r.get("device") == d["device"])
        lines.append("device %d: %r instance %s product %s; %s" % (
            d["device"], d["name"], d["instance"], d["product"], ", ".join("%s %d" % kv for kv in sorted(tags.items()))))
    effects = {}
    for e in s.tagged("EFF1"):
        effects.setdefault(e["effect"], []).append(e)
    for n, recs in sorted(effects.items()):
        ops = Counter(EFFECT_OPS.get(e["op"], str(e["op"])) for e in recs)
        kind = next((EFFECT_GUIDS.get(e["type"], e["type"]) for e in recs if e["op"] == 1), "?")
        failed = sum(1 for e in recs if e["hresult"] & 0x80000000)
        lines.append("effect %d (device %d, %s): %s; %d failed" % (
            n, recs[0]["device"], kind, ", ".join("%s %d" % kv for kv in sorted(ops.items())), failed))
    mods = s.tagged("MOD1")
    for m in mods[-1]["modules"] if mods else []:
        calls = ", ".join("%s %d" % kv for kv in m["calls"].items() if kv[1])
        if calls:
            lines.append("caller %s: %s" % (m["name"], calls))
    controls = s.tagged("CTL1")
    if controls:
        lines.append("controls: %d CTL1, %d distinct payloads, threads %s" % (
            len(controls), len(set(c["payload"] for c in controls)), sorted(set(c["thread"] for c in controls))))
    for f in s.tagged("FIL1"):
        lines.append("file: %s (%d bytes)" % (f["path"], len(f["contents"])))
    opened = Counter(o["path"] for o in s.tagged("OPN1"))
    if opened:
        lines.append("opened: %d .lsp/.acm opens of %d files" % (sum(opened.values()), len(opened)))
    lines.append("ended: %s" % ("yes" if counts["END1"] else "no END1"))
    return "\n".join(lines)


def write_files(s, out):
    """Each FIL1's contents to out/<base name> (the last of a name wins); the names written."""
    import os
    os.makedirs(out, exist_ok=True)
    names = []
    for f in s.tagged("FIL1"):
        name = f["path"].replace("\\", "/").rsplit("/", 1)[-1]
        with open(os.path.join(out, name), "wb") as w:
            w.write(f["contents"])
        names.append(name)
    return names


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__.split("\n")[0])
    if sys.argv[1] == "--files":
        for path in sys.argv[3:]:
            print("\n".join(write_files(read(path), sys.argv[2])))
        sys.exit()
    for path in sys.argv[1:]:
        if len(sys.argv) > 2:
            print("==", path)
        print(summary(read(path)))
