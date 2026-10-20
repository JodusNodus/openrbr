"""Dump the exe image and NGP.dll's image from the running NGP test game (run on the test machine by scripts/ngp-run.sh;
rbr-recomp's scripts/ngp-memdiff.py reads them).

Usage (on the test machine, the game running in the Wine prefix ~/PREFIX): python3 -I ngp-dump.py OUT_DIR PREFIX
Writes OUT_DIR/exe.bin (0x400000..0x7ec000) and OUT_DIR/ngp.bin, and prints NGP.dll's load address.
"""
import glob, os, sys


def game_pid():
    want = "WINEPREFIX=" + os.path.join(os.path.expanduser("~"), sys.argv[2], "pfx/")
    for d in glob.glob("/proc/[0-9]*"):
        try:
            env = open(d + "/environ", "rb").read().decode(errors="replace").split("\0")
            if want in env and "RichardBurnsRally_SSE" in open(d + "/cmdline", "rb").read().decode(errors="replace") \
                    and "ngp.dll" in open(d + "/maps").read().lower():
                return d[6:]
        except OSError:
            pass
    sys.exit("no running NGP game")


def read(pid, lo, hi):
    out = bytearray()
    with open(f"/proc/{pid}/mem", "rb", 0) as m:
        for a in range(lo, hi, 0x10000):
            n = min(0x10000, hi - a)
            try:
                m.seek(a)
                out += m.read(n)
            except OSError:
                out += b"\0" * n
    return bytes(out)


def main(out):
    pid = game_pid()
    os.makedirs(out, exist_ok=True)
    open(f"{out}/exe.bin", "wb").write(read(pid, 0x400000, 0x7EC000))
    base = min(int(l.split("-")[0], 16) for l in open(f"/proc/{pid}/maps") if l.rstrip().lower().endswith("ngp.dll"))
    head = read(pid, base, base + 0x1000)
    pe = int.from_bytes(head[0x3C:0x40], "little")
    open(f"{out}/ngp.bin", "wb").write(read(pid, base, base + int.from_bytes(head[pe + 0x50:pe + 0x54], "little")))
    print(f"pid {pid}, NGP.dll at {base:08x}")


if __name__ == "__main__":
    main(sys.argv[1])
