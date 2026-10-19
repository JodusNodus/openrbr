"""Diffs an NGP boot's image (rbr-standalone --boot --ngp-physics FILE --dump-image IMAGE) against the reference
vectors of an NGP run (harness/load_{car,wheel}_refs.py and harness/ngp_data_refs.py output, e.g. ngp-data/load40):
the car's and wheels' definitions, and NGP.dll's `.data` words, which NGP's per-car init (`0x1003f5b0`) writes.

Usage: python3 -I -B harness/ngp_init_diff.py IMAGE VECTORS [--words]

The boot's definitions are found from the car (`[0x8ef660]`): its definition at +0x34, the wheels at +0x470 and each
wheel's definition at +0x34. A word that differs counts as a pointer, and is not listed, when the reference's value is
in the snapshot's heap and the boot's in the boot heap. NGP's `.data` is read in the image at NGP.dll's runtime base
in the reference runs (`BASE`); a word the image lacks is listed as missing. The vectors list only the words that differ
from NGP.dll's, so with `$RBR_NGP` set a nonzero word of the image the vectors lack is checked against the file's
(`$RBR_NGP/7.4.778/Plugins/NGP.dll`). Without --words only the counts per object and the differing ranges are printed.
"""
import os
import struct
import sys

CAR_PTR, CAR_DEF, WHEELS = 0x8EF660, 0x34, 0x470
BASE, FILE_BASE = 0x791A0000, 0x10000000
LO, HI = 0x10171000, 0x10178000
BOOT_HEAP = (0x10000000, 0x40000000)
REF_HEAP = (0x01000000, 0x40000000)


class Image:
    def __init__(self, path):
        data = open(path, "rb").read()
        self.regions = []
        at = 0
        while at < len(data):
            base, size = struct.unpack_from("<II", data, at)
            self.regions.append((base, size, at + 8))
            at += 8 + size
        self.data = data

    def read(self, addr, n):
        for base, size, off in self.regions:
            if base <= addr and addr + n <= base + size:
                return self.data[off + addr - base:off + addr - base + n]
        return None

    def u32(self, addr):
        b = self.read(addr, 4)
        return None if b is None else struct.unpack("<I", b)[0]


def vectors(path):
    """`(kind, name) -> bytes` of a refs file."""
    out = {}
    for line in open(path):
        if line.startswith("#"):
            continue
        kind, name, _, hexbytes = line.split()[:4]
        out[(kind, name)] = bytes.fromhex(hexbytes)
    return out


def file_word(dll, addr):
    """The word NGP.dll's file holds at `addr` (0 past a section's raw data)."""
    pe, = struct.unpack_from("<I", dll, 0x3C)
    count, = struct.unpack_from("<H", dll, pe + 6)
    optional, = struct.unpack_from("<H", dll, pe + 20)
    for i in range(count):
        _, va, rsize, raw = struct.unpack_from("<IIII", dll, pe + 24 + optional + 40 * i + 8)
        if FILE_BASE + va <= addr < FILE_BASE + va + rsize:
            return struct.unpack_from("<I", dll, raw + addr - FILE_BASE - va)[0]
    return 0


def ranges(offsets):
    out = []
    for o in offsets:
        if out and out[-1][1] == o:
            out[-1][1] = o + 4
        else:
            out.append([o, o + 4])
    return out


def compare(label, ref, boot, words):
    diffs = []
    for o in range(0, len(ref) - 3, 4):
        r, = struct.unpack_from("<I", ref, o)
        b, = struct.unpack_from("<I", boot, o)
        if r != b and not (REF_HEAP[0] <= r < REF_HEAP[1] and BOOT_HEAP[0] <= b < BOOT_HEAP[1]):
            diffs.append((o, r, b))
    print("%s: %d words differ" % (label, len(diffs)))
    if words:
        for o, r, b in diffs:
            print("  +%04x ref %08x (%g) boot %08x (%g)" % (o, r, f32(r), b, f32(b)))
    else:
        for lo, hi in ranges([o for o, _, _ in diffs]):
            print("  +%04x..+%04x" % (lo, hi))
    return diffs


def f32(w):
    return struct.unpack("<f", struct.pack("<I", w))[0]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 2:
        sys.exit(__doc__)
    words = "--words" in sys.argv
    image, refs = Image(args[0]), args[1]
    car = image.u32(CAR_PTR)
    cars = vectors(os.path.join(refs, "load-car-defs.txt"))
    wheels = vectors(os.path.join(refs, "load-wheel-defs.txt"))
    ref = cars[("car_def", "0")]
    compare("car_def", ref, image.read(image.u32(car + CAR_DEF), len(ref)), words)
    for k, name in enumerate(["LF", "RF", "LB", "RB"]):
        ref = wheels[("wheel_def", name)]
        wheel = image.u32(car + WHEELS + 4 * k)
        compare("wheel_def " + name, ref, image.read(image.u32(wheel + CAR_DEF), len(ref)), words)
    listed = {}
    for line in open(os.path.join(refs, "ngp-data.txt")):
        if not line.startswith("#"):
            addr, value = (int(x, 16) for x in line.split())
            listed[addr] = value
    if os.environ.get("RBR_NGP"):
        dll = open(os.path.join(os.environ["RBR_NGP"], "7.4.778", "Plugins", "NGP.dll"), "rb").read()
        for addr in range(LO, HI, 4):
            if addr not in listed and image.u32(addr - FILE_BASE + BASE):
                listed[addr] = file_word(dll, addr)
    missing, differ = [], []
    for addr, value in sorted(listed.items()):
        have = image.u32(addr - FILE_BASE + BASE)
        if have is None:
            missing.append(addr)
        elif have != value:
            differ.append((addr, value, have))
    print("ngp .data: %d words missing, %d differ" % (len(missing), len(differ)))
    if words:
        for addr, value, have in differ:
            print("  %08x ref %08x (%g) boot %08x (%g)" % (addr, value, f32(value), have, f32(have)))


if __name__ == "__main__":
    main()
