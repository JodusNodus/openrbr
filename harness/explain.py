"""Explain a diverge.py word offset: which object and field it is, and who writes it.

Usage: python3 -I harness/explain.py OFFSET... [-- ADDR... | @ADDRFILE]
  OFFSET: a word offset diverge.py printed (e.g. 0x11c0), the offset into the recorded
  frame: control (0x20 bytes), then the car (0x1700, the object at *0x8ef660), then wheels
  0-3 (0xa00 each, the objects at car+0x470). For each, prints the object and its offset,
  the lines of src/rbr that mention that offset, and the original functions that store to
  that displacement: [reg + disp] with any base register, or through a register set by
  `lea reg, [x + d]` earlier in the same function (sub-objects such as CReal at +d). Stores
  through a pointer passed to a callee (`lea; push; call`) are not found, so a field with
  no listed writer is written that way. With ADDRs (e.g. the group that diverged), the
  writers among them are listed first: the likeliest culprits.
  The store scan of the original exe is cached in /tmp/rbr-explain-stores.txt.
"""
import collections, glob, os, re, subprocess, sys, bisect

ORIG_EXE = os.environ.get("RBR_ORIG_EXE") or os.path.join(  # the 1.02 exe: RBR_PATCH's, or original/'s
    os.environ.get("RBR_PATCH") or os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                               "original", "patch-1.02", "files", "Main_group"),
    "RichardBurnsRally_SSE.exe")
OBJDUMP = os.environ.get("OBJDUMP", "/opt/homebrew/opt/llvm/bin/llvm-objdump")
CACHE = "/tmp/rbr-explain-stores.txt"
CONTROL, CAR, WHEEL = 0x20, 0x1700, 0xa00

args = sys.argv[1:]
group = []
if "--" in args:
    i = args.index("--")
    args, group = args[:i], args[i + 1:]
    if group and group[0].startswith("@"):
        group = open(group[0][1:]).read().split()
if not args:
    sys.exit(__doc__)
group = {int(a, 16) for a in group}


def where(off):
    if off < CONTROL:
        return "control", off
    if off < CONTROL + CAR:
        return "car", off - CONTROL
    w = (off - CONTROL - CAR) // WHEEL
    return "wheel %d" % w, (off - CONTROL - CAR) % WHEEL


def stores():
    """{displacement: set(function start)} for every store [reg+disp] in the original."""
    if not os.path.exists(CACHE):
        bounds = sorted(int(l.split("|")[0], 16) for l in open("data/rbr.functions.csv").readlines()[1:] if "|" in l)
        out = subprocess.run([OBJDUMP, "-d", "--no-show-raw-insn", "-M", "intel", ORIG_EXE],
                             capture_output=True, text=True, check=True).stdout
        rows = set()
        store = re.compile(r"\s*([0-9a-f]+):\s+(mov\w*|fst\w*|fist\w*|add|sub|and|or|xor|inc|dec|neg|not|"
                           r"set\w+|stos\w*)\s+\w+ ptr \[(e\w\w)(?: \+ (0x[0-9a-f]+))?\]")
        lea = re.compile(r"\s*[0-9a-f]+:\s+lea\s+(e\w\w), \[e\w\w \+ (0x[0-9a-f]+)\]$")
        write = re.compile(r"\s*[0-9a-f]+:\s+\w+\s+(e\w\w)\b")
        fn, base = None, {}  # base: register -> displacement from a lea, a sub-object pointer
        for line in out.split("\n"):
            m = re.match(r"\s*([0-9a-f]+):", line)
            if not m:
                continue
            a = int(m.group(1), 16)
            i = bisect.bisect_right(bounds, a) - 1
            if i < 0:
                continue
            if bounds[i] != fn:
                fn, base = bounds[i], {}
            # intel syntax: the destination comes first
            m = store.match(line)
            if m:
                d = int(m.group(4), 16) if m.group(4) else 0
                if d:
                    rows.add((d, fn))
                if m.group(3) in base:
                    rows.add((base[m.group(3)] + d, fn))
                continue
            m = lea.match(line)
            if m:
                base[m.group(1)] = int(m.group(2), 16)
                continue
            if re.match(r"\s*[0-9a-f]+:\s+call", line):
                for r in ("eax", "ecx", "edx"):
                    base.pop(r, None)
                continue
            m = write.match(line)
            if m and not re.match(r"\s*[0-9a-f]+:\s+(cmp|test|push)\b", line):
                base.pop(m.group(1), None)
        with open(CACHE, "w") as f:
            f.writelines("%x %x\n" % r for r in sorted(rows))
    d = collections.defaultdict(set)
    for line in open(CACHE):
        disp, fn = line.split()
        d[int(disp, 16)].add(int(fn, 16))
    return d


names = {}
for path in glob.glob("src/rbr/*.cpp"):
    pending = None
    for line in open(path, errors="ignore"):
        m = re.search(r"FUNCTION: RBR (0x[0-9a-f]+)", line)
        if m:
            pending = int(m.group(1), 16)
            continue
        if pending is not None and line.strip() and not line.lstrip().startswith("//"):
            names[pending] = "%s %s" % (os.path.basename(path), line.strip()[:70])
            pending = None

st = stores()
for a in args:
    off = int(a, 16)
    obj, rel = where(off)
    print("%s: %s +%#x" % (a, obj, rel))
    pat = re.compile(r"\b0x0*%x\b" % rel, re.I)
    hits = []
    for path in sorted(glob.glob("src/rbr/RBR_E[3-5]*.h") + glob.glob("src/rbr/RBR_E[3-5]*.cpp")):
        for k, line in enumerate(open(path, errors="ignore"), 1):
            if pat.search(line):
                hits.append("  %s:%d: %s" % (os.path.basename(path), k, line.strip()[:100]))
    print("  src mentions (%d):" % len(hits))
    print("\n".join(hits[:12]) + ("\n  ..." if len(hits) > 12 else ""))
    writers = sorted(st.get(rel, ()))
    if group:
        print("  writers in the group:", " ".join("%#x" % w for w in writers if w in group) or "none")
    print("  original functions storing to +%#x (%d):" % (rel, len(writers)))
    for w in writers[:20]:
        print("    %#x %s" % (w, names.get(w, "")))
    if len(writers) > 20:
        print("    ...")
