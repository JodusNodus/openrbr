"""Diffs the --boot image after CCar::UpdateA720 (rbr-standalone --boot --dump-image FILE) against a step-0 snapshot
(the composed snap-0), on the words the snapshot's run read before writing them (rbr-standalone --read-set: the
snapshot's .reads).

Usage: python3 -I -B harness/boot_diff.py SNAPSHOT READS IMAGE [--allocs TEXTLOG --dump INDEX] [--src SRC]

The boot's heap blocks (IMAGE.blocks) are placed on the snapshot's by the pointers that reach them: from the exe's
words (same address in both), each boot pointer into a boot block, read at the same place in the snapshot, gives the
block's snapshot address; the walk follows the blocks found. A word then compares equal when its bytes are equal, or
when the boot word points into a placed block and the snapshot's is the placed address (a placement). Each read word
is one of:
  equal          equal bytes
  placement      a pointer, equal as a placement
  differs        a placed (or exe) word whose bytes differ: listed by object, with both values
  unplaced       a heap word in no placed boot block (an object the boot does not build, or one no pointer reached)
  stack/module   the snapshot's stack or a DLL image (not compared)
With --frame-words, the car's and the wheels' words as a recording frame holds them (pure::frame_bytes: the car at
[0x8ef660], 0x1700 bytes, each wheel of +0x470, 0xa00 bytes) are compared the same way, read or not, as car+off /
wheelK+off (pure::word_name), for the compare's run-noise words.
Objects are named by the snapshot's block: the allocating function (--allocs: rbr-recomp's harness/allocs.py text
output up to the dump record INDEX) and the RTTI class of its first word.
"""
import argparse
import bisect
import collections
import os
import re
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.append(os.path.join(ROOT, "harness"))
import replay  # noqa: E402

EXE_LO, EXE_HI = 0x400000, 0x1700000  # the exe image (sections and .data's tail)
STACK = (0x100000, 0x200000)  # the boot's stack and scratch, compared at their own address
SCRATCH = (0x300000, 0x310000)
HEAP_LO, HEAP_HI = 0x10000000, 0x40000000  # the boot heap
CAR_PTR, CAR_BYTES, WHEEL_LIST, WHEEL_BYTES = 0x8EF660, 0x1700, 0x470, 0xA00


class Image:
    """The boot image: regions and heap blocks."""

    def __init__(self, path):
        d = open(path, "rb").read()
        self.regions, at = [], 0
        while at < len(d):
            base, size = struct.unpack_from("<II", d, at)
            self.regions.append((base, size, d[at + 8:at + 8 + size]))
            at += 8 + size
        self.regions.sort()
        self.bases = [r[0] for r in self.regions]
        self.blocks = sorted((int(a, 16), int(n, 16)) for a, n in
                             (line.split() for line in open(os.path.splitext(path)[0] + ".blocks")))
        self.bstarts = [b[0] for b in self.blocks]

    def read(self, at, n):
        i = bisect.bisect_right(self.bases, at) - 1
        if i < 0:
            return None
        base, size, data = self.regions[i]
        return data[at - base:at - base + n] if at + n <= base + size else None

    def u32(self, at):
        b = self.read(at, 4)
        return None if b is None else struct.unpack("<I", b)[0]

    def block(self, at):
        """(index, base, size) of the boot block holding `at` (its end included: a pointer one past it)."""
        i = bisect.bisect_right(self.bstarts, at) - 1
        if i >= 0 and at <= self.blocks[i][0] + max(self.blocks[i][1], 1):
            return i, self.blocks[i][0], self.blocks[i][1]
        return None


def snap_u32(snap, at):
    try:
        return snap.u32(at)
    except (KeyError, struct.error):
        return None


def place(image, snap):
    """{boot block index: delta} (snapshot address = boot address + delta), and {block: its proposals}.
    Each round collects every pointer's proposal from the exe's words and the blocks placed so far, and places each
    block by its most common one, until nothing changes (a list the boot builds shorter would otherwise misplace)."""
    pointers = []  # (where, boot value, target block, source block or None)
    for base, size, data in image.regions:
        root = base < EXE_HI and base + size > EXE_LO or STACK[0] <= base < SCRATCH[1]
        for k, bv in enumerate(struct.unpack_from("<%dI" % (size // 4), data)):
            b = image.block(bv) if HEAP_LO <= bv < HEAP_HI else None
            if b is None:
                continue
            src = None if root else image.block(base + 4 * k)
            if root or src is not None:
                pointers.append((base + 4 * k, bv, b[0], src[0] if src else None))
    deltas, votes = {}, {}
    for _ in range(30):
        votes = collections.defaultdict(collections.Counter)
        for at, bv, target, src in pointers:
            if src is not None and src not in deltas:
                continue
            sv = snap_u32(snap, (at + (deltas[src] if src is not None else 0)) & 0xFFFFFFFF)
            # an equal word is data, not a pointer (the boot heap's range overlaps the snapshot's)
            if sv is not None and sv != bv and not EXE_LO <= sv < EXE_HI and snap.has(sv):
                votes[target][(sv - bv) & 0xFFFFFFFF] += 1
        new = {i: best(image, snap, i, c) for i, c in votes.items()}
        if new == deltas:
            break
        deltas = new
    return deltas, votes


def best(image, snap, i, votes):
    """The placement of boot block `i` among its proposals: the most common one, unless the block's words equal another
    candidate's on more words (a list the boot builds shorter points its tail at another object)."""
    if len(votes) == 1:
        return next(iter(votes))
    base, size = image.blocks[i]
    words = image.read(base, min(size, 0x400) & ~3)

    def score(delta):
        try:
            theirs = snap.read((base + delta) & 0xFFFFFFFF, len(words))
        except KeyError:
            return -1
        return sum(words[k:k + 4] == theirs[k:k + 4] for k in range(0, len(words), 4))

    return max(votes, key=lambda d: (score(d), votes[d]))


def names(src):
    """{function address: name} from the C++ annotations."""
    out = {}
    for root, _, files in os.walk(src):
        for f in files:
            if f.endswith((".cpp", ".h")):
                lines = open(os.path.join(root, f), errors="replace").read().split("\n")
                for i, line in enumerate(lines):
                    m = re.match(r"// (?:FUNCTION|STUB): RBR 0x([0-9a-f]+)", line)
                    nxt = next((x for x in lines[i + 1:i + 4] if x and not x.startswith("//")), "") if m else ""
                    found = [w for w in re.findall(r"([A-Za-z_][\w:~]*)\s*\(", nxt) if not w.startswith("__")]
                    if found:
                        out.setdefault(int(m.group(1), 16), found[0])
    return out


def live_blocks(path, until):
    """The allocations live at record `until` of allocs.py's text output: sorted [(block, size or 0, function)]."""
    live = {}
    for line in open(path):
        f = line.split()
        if int(f[0]) >= until:
            break
        kind = f[1]
        if kind == "dump":
            continue
        fn, a, res = int(f[2], 16), int(f[4], 16), int(f[5], 16)
        if kind in ("new", "malloc") and res:
            live[res] = (a, fn)
        elif kind in ("calloc", "strdup") and res:
            live[res] = (0, fn)
        elif kind == "realloc":
            live.pop(a, None)
            if res:
                live[res] = (0, fn)
        elif kind in ("delete", "delete[]", "free"):
            live.pop(a, None)
    return sorted((b, s, fn) for b, (s, fn) in live.items())


def rtti(snap, vtable):
    try:
        td = snap.u32(snap.u32(vtable - 4) + 12)
        raw = snap.read(td + 8, 96).split(b"\0")[0].decode()
    except (KeyError, struct.error, UnicodeDecodeError):
        return None
    return raw[4:].rstrip("@") if raw.startswith(".?A") else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("snapshot")
    ap.add_argument("reads")
    ap.add_argument("image")
    ap.add_argument("--allocs")
    ap.add_argument("--dump", type=int)
    ap.add_argument("--src", default=os.path.join(ROOT, "src"))
    ap.add_argument("--votes", action="store_true", help="list the blocks whose pointers disagree")
    ap.add_argument("--frame-words", action="store_true", help="compare the car's and the wheels' words too")
    ap.add_argument("--whole", metavar="I-J", help="compare every word of boot blocks #I..#J, read or not")
    args = ap.parse_args()

    snap, image = replay.Snapshot(args.snapshot), Image(args.image)
    deltas, votes = place(image, snap)
    conflicts = [i for i, c in votes.items() if len(c) > 1]
    placed = sorted(((image.blocks[i][0] + d) & 0xFFFFFFFF, image.blocks[i][1], i) for i, d in deltas.items())
    pstarts = [p[0] for p in placed]
    fn = names(args.src) if os.path.isdir(args.src) else {}
    live = live_blocks(args.allocs, args.dump) if args.allocs else []
    lstarts = [b[0] for b in live]
    mods = [(b, s, name) for b, s, name, _ in snap.modules()]
    sbase = next(r[5] for r in snap.regions if r[0] <= snap.stack_lo < r[0] + r[1])
    stack = [(r[0], r[0] + r[1]) for r in snap.regions if r[5] == sbase]
    stack_lo, stack_hi = min(s for s, _ in stack), max(e for _, e in stack)

    def to_boot(at):
        """(boot address, boot block index or None), or None when no placed block holds `at`."""
        if EXE_LO <= at < EXE_HI:
            return at, None
        i = bisect.bisect_right(pstarts, at) - 1
        if i >= 0 and at < placed[i][0] + max(placed[i][1], 1):
            return (at - deltas[placed[i][2]]) & 0xFFFFFFFF, placed[i][2]
        return None

    def owner(at):
        """The snapshot object holding `at`: (base, label)."""
        if EXE_LO <= at < EXE_HI:
            return 0, "exe"
        i = bisect.bisect_right(lstarts, at) - 1
        if i >= 0 and (live[i][1] == 0 or at < live[i][0] + live[i][1]):
            b, size, f = live[i]
            vt = snap_u32(snap, b)
            cls = rtti(snap, vt) if vt and EXE_LO <= vt < EXE_HI else None
            return b, "%s size %#x by %#x %s" % (cls or "-", size, f, fn.get(f, "?"))
        return at & ~0xFFF, "no logged block"

    words = collections.defaultdict(int)  # aligned word -> mask of the bytes read
    for line in open(args.reads):
        at, n, _reader, _ = line.split()
        at, n = int(at, 16), int(n, 16)
        for k in range(at, at + n):
            words[k & ~3] |= 1 << (k & 3)

    fmt = lambda v: "-" if v is None else "%#010x" % v  # noqa: E731

    def same(sv, bv):
        """Equal, or a pointer equal as a placement."""
        if sv == bv:
            return True
        b = image.block(bv) if bv is not None else None
        return b is not None and b[0] in deltas and (bv + deltas[b[0]]) & 0xFFFFFFFF == sv

    if args.frame_words:
        car_s, car_b = snap.u32(CAR_PTR), image.u32(CAR_PTR)
        objs = [("car", car_s, car_b, CAR_BYTES)] + [
            ("wheel%d" % k, snap.u32(car_s + WHEEL_LIST + 4 * k), image.u32(car_b + WHEEL_LIST + 4 * k), WHEEL_BYTES)
            for k in range(4)]
        rows = [(name, off, snap_u32(snap, s0 + off), image.u32(b0 + off))
                for name, s0, b0, n in objs for off in range(0, n, 4)]
        bad = [r for r in rows if not same(r[2], r[3])]
        print("# frame words: %d of %d differ (snapshot / boot)" % (len(bad), len(rows)))
        for name, off, sv, bv in bad:
            print("  %s+%#x: %#010x / %#010x" % (name, off, sv, bv))

    if args.whole:
        lo, hi = map(int, args.whole.split("-"))
        print("# boot blocks #%d..#%d, every word (offset: snapshot / boot)" % (lo, hi))
        for i in range(lo, hi + 1):
            base, size = image.blocks[i]
            if i not in deltas:
                print("  #%d %#x size %#x: not placed" % (i, base, size))
                continue
            at = (base + deltas[i]) & 0xFFFFFFFF
            bad = [(off, snap_u32(snap, at + off), image.u32(base + off)) for off in range(0, size & ~3, 4)]
            bad = [b for b in bad if not same(b[1], b[2])]
            print("  #%d %#x size %#x at %#x: %d of %d words differ%s" % (
                i, base, size, at, len(bad), size // 4,
                "".join(" +%#x %s/%s" % (o, fmt(sv), fmt(bv)) for o, sv, bv in bad[:8])))

    totals = collections.Counter()
    diffs = collections.defaultdict(list)  # owner -> [(offset, snapshot, boot, boot block)]
    unplaced = collections.defaultdict(set)
    for w, m in sorted(words.items()):
        if stack_lo <= w < stack_hi or any(b <= w < b + s for b, s, _ in mods if not EXE_LO <= w < EXE_HI):
            totals["stack/module"] += 1
            continue
        t = to_boot(w)
        if t is None:
            totals["unplaced"] += 1
            unplaced[owner(w)].add(w)
            continue
        sv, bv = snap_u32(snap, w), image.u32(t[0])
        if bv is None and t[1] is None:
            totals["exe code (stand-ins by address)"] += 1
            continue
        bytes_differ = sv is None or bv is None or any((sv ^ bv) >> 8 * k & 0xFF for k in range(4) if m >> k & 1)
        if not bytes_differ or sv == bv:
            totals["equal"] += 1
            continue
        b = image.block(bv) if bv is not None else None
        if b is not None and b[0] in deltas and (bv + deltas[b[0]]) & 0xFFFFFFFF == sv:
            totals["placement"] += 1
            continue
        totals["differs"] += 1
        o = owner(w)
        diffs[o].append((w - o[0], sv, bv, t[1]))

    print("# %s vs %s on %s" % (os.path.basename(args.image), os.path.basename(args.snapshot),
                                os.path.basename(args.reads)))
    print("boot blocks placed: %d of %d (%d with conflicting pointers)" % (len(deltas), len(image.blocks),
                                                                          len(conflicts)))
    print("read words: " + ", ".join("%s %d" % kv for kv in sorted(totals.items())))
    if args.votes:
        for i in sorted(conflicts):
            print("  boot block #%d %#x size %#x: %s" % (i, image.blocks[i][0], image.blocks[i][1], ", ".join(
                "%#x x%d" % ((image.blocks[i][0] + d) & 0xFFFFFFFF, n) for d, n in votes[i].most_common(4))))
    print("\n# differing words by snapshot object (offset: snapshot / boot)")
    for (base, label), rows in sorted(diffs.items(), key=lambda kv: -len(kv[1])):
        blk = {r[3] for r in rows} - {None}
        print("%#010x %s: %d words%s" % (base, label, len(rows),
                                         " (boot block #%s)" % ",".join(map(str, sorted(blk))) if blk else ""))
        for off, sv, bv, _ in rows:
            print("    +%#x: %s / %s" % (off, fmt(sv), fmt(bv)))
    print("\n# unplaced words by snapshot object")
    for (base, label), ws in sorted(unplaced.items(), key=lambda kv: -len(kv[1])):
        offs = sorted(w - base for w in ws)
        print("%#010x %s: %d words at %s%s" % (base, label, len(ws), " ".join("+%#x" % o for o in offs[:16]),
                                                " ..." if len(offs) > 16 else ""))


if __name__ == "__main__":
    main()
