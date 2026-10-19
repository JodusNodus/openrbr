"""Every change NGP made to the running exe, from a memory dump (scripts/ngp-dump.py on the test machine).

Usage: python3 -I scripts/ngp-memdiff.py EXE_DUMP NGP_DUMP NGP_BASE > /tmp/ngp-patches.tsv
  EXE_DUMP  the exe image from 0x400000 (exe.bin), NGP_DUMP NGP.dll's image (ngp.bin),
  NGP_BASE  where NGP.dll was loaded (hex, printed by ngp-dump.py).
Compares .text and .rdata with the original exe (RBR_ORIG_EXE, as scripts/const-check.py) and prints one row per changed run:
kind, address, length, src file, function, detail. NGP addresses are rebased to its 0x10000000 image base, so they
match the Ghidra project and scripts/ngp-scan.py. Kinds:
  call    a call/jmp rel32 that now lands in NGP code
  const   a 4-byte operand now pointing at an NGP float (value now, and the exe value it replaced)
  vtable  a changed .rdata dword (the import table filled in at load is skipped)
  code    anything else (NOPs, branch changes, flag masks, partial rel32 edits), raw old -> new bytes
  harness the harness plugin's own patches (from its log), listed so they aren't mistaken for NGP's
A summary goes to stderr.
"""
import bisect, collections, os, re, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.environ.get("RBR_ORIG_EXE") or os.path.join(  # the 1.02 exe: RBR_PATCH's, or original/'s
    os.environ.get("RBR_PATCH") or os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                               "original", "patch-1.02", "files", "Main_group"),
    "RichardBurnsRally_SSE.exe")
NGP_SIZE, NGP_IMAGE = 0x13E3000, 0x10000000
# The harness plugin's own patches: the FFB fix (RBRRecompHarness.log "applied SetFFBValue ...") and, in record or
# playback mode, its control hook in SteerSig::Emit and the CPhysics::StepForward vtable slot.
HARNESS = {0x4C6BAB, 0x4C6BC2, 0x48CD90, 0x73F994}
TEXT, RDATA = (0x401000, 0x32774B), (0x729000, 0x969AD)  # VA, size; file offset == RVA for both


def functions():
    rows = [l.split("|") for l in open(os.path.join(ROOT, "data/rbr.functions.csv")).read().split("\n")[1:] if "|" in l]
    return sorted((int(a, 16), int(s)) for a, s in rows)


def annotations():
    """addr -> (source file, name) of the C++ reconstruction (rbr-recomp), from its frozen state in data/rbr.recomp.csv."""
    rows = [l.split("|") for l in open(os.path.join(ROOT, "data/rbr.recomp.csv")).read().split("\n")[1:] if "|" in l]
    return {int(r[0], 16): (r[1], r[3]) for r in rows if r[1]}


def runs(orig, dump, va, size):
    """Changed byte runs; a single unchanged byte between changes is merged in."""
    lo = va - 0x400000
    f, g = orig[lo:lo + size], dump[lo:lo + size]
    i = 0
    while i < size:
        if f[i] == g[i]:
            i += 1
            continue
        j = i
        while j < size and (f[j] != g[j] or (j + 1 < size and f[j + 1] != g[j + 1])):
            j += 1
        yield va + i, f[i:j], g[i:j]
        i = j


def main(exe_dump, ngp_dump, ngp_base):
    orig, dump, ngp = open(EXE, "rb").read(), open(exe_dump, "rb").read(), open(ngp_dump, "rb").read()
    base = int(ngp_base, 16)
    in_ngp = lambda v: base <= v < base + NGP_SIZE
    rebase = lambda v: v - base + NGP_IMAGE
    pe = struct.unpack_from("<I", orig, 0x3C)[0]
    iat, iat_size = struct.unpack_from("<II", orig, pe + 24 + 96 + 12 * 8)  # data directory 12: the import address table
    iat_end = 0x400000 + iat + iat_size
    funcs, ann = functions(), annotations()
    keys = [a for a, _ in funcs]

    def where(a):
        i = bisect.bisect_right(keys, a) - 1
        fn = funcs[i][0] if i >= 0 and a < funcs[i][0] + max(funcs[i][1], 1) else None
        return ann.get(fn, ("", f"{fn:08x}" if fn else ""))

    summary = collections.Counter()
    for a, old, new in list(runs(orig, dump, *TEXT)) + list(runs(orig, dump, *RDATA)):
        if RDATA[0] <= a < iat_end:
            continue
        f, name = where(a)
        u32 = lambda b: struct.unpack("<I", b)[0]
        if a in HARNESS or a - 1 in HARNESS or a - 2 in HARNESS:
            kind, detail = "harness", f"{old.hex()} -> {new.hex()}"
        elif a >= RDATA[0]:
            lo, hi = a & ~3, (a + len(old) + 3) & ~3
            words = [(u32(orig[o - 0x400000:o - 0x3FFFFC]), u32(dump[o - 0x400000:o - 0x3FFFFC])) for o in range(lo, hi, 4)]
            kind = "vtable"
            detail = " ".join(f"{o:08x}->" + (f"ngp:{rebase(n):08x}" if in_ngp(n) else f"{n:08x}") for o, n in words if o != n)
        elif len(new) == 4 and in_ngp(u32(new)):
            ptr, was = rebase(u32(new)), u32(old)
            now = struct.unpack_from("<f", ngp, ptr - NGP_IMAGE)[0]
            prev = f"{struct.unpack_from('<f', orig, was - 0x400000)[0]:g} @{was:08x}" if RDATA[0] <= was < 0x7EC000 else f"ptr {was:08x}"
            kind, detail = "const", f"ngp:{ptr:08x} = {now:g} (was {prev})"
        elif len(new) == 4 and in_ngp((a + 4 + struct.unpack("<i", new)[0]) & 0xFFFFFFFF):
            kind, detail = "call", f"ngp:{rebase((a + 4 + struct.unpack('<i', new)[0]) & 0xFFFFFFFF):08x}"
        else:
            kind, detail = "code", f"{old.hex()} -> {new.hex()}"
        m = re.match(r"RBR_E(\d+)_", f)
        summary[(kind, "physics" if m and 33 <= int(m.group(1)) <= 51 else "other")] += 1
        print("\t".join([kind, f"{a:08x}", str(len(old)), f, name, detail]))
    for k in sorted(summary):
        print(*k, summary[k], file=sys.stderr)


if __name__ == "__main__":
    main(*sys.argv[1:4])
