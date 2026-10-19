"""List what NGP.dll (NGP 7.4.778) changes or uses in RichardBurnsRally_SSE.exe, statically.

Usage: python3 -I scripts/ngp-scan.py PATH/TO/NGP.dll > /tmp/ngp-scan.tsv
Prints one row per reference: kind, exe address, patch length, containing exe function,
source file of that function in src/rbr (data/rbr.recomp.csv), and the rbr.csv name if any. Kinds:
  patch   byte patch: `push <exe addr>` ... `call PATCH_CTOR` (the ctor gets the new bytes and length)
  orig    lazily resolved pointer to an original exe function: `push <exe addr>` ... `call ORIG_RESOLVE`
  vtable  a named exe vftable (data/rbr.csv) the DLL code references; slot swaps go through VTABLE_SWAP
  ref     any other exe address in an instruction operand
A summary goes to stderr. Nothing in the DLL is run. The helper addresses are specific to 7.4.778.
"""
import bisect, collections, os, re, subprocess, sys

OBJDUMP = os.environ.get("OBJDUMP", "objdump")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATCH_CTOR, ORIG_RESOLVE, VTABLE_SWAP = 0x100E8B70, 0x101109D0, 0x10110C30
EXE_LO, EXE_HI = 0x401000, 0x7EC000  # .text through .data


def functions():
    rows = [l.split("|") for l in open(os.path.join(ROOT, "data/rbr.functions.csv")).read().split("\n")[1:] if "|" in l]
    return sorted((int(a, 16), int(s)) for a, s in rows)


def names():
    out = {}
    for line in open(os.path.join(ROOT, "data/rbr.csv"), errors="replace"):
        p = line.rstrip("\n").split("|")
        if len(p) >= 2 and re.fullmatch(r"[0-9a-fA-F]{8}", p[0]):
            out[int(p[0], 16)] = p[1]
    return out


def source_files():
    """addr -> source file of the C++ reconstruction (rbr-recomp), from its frozen state in data/rbr.recomp.csv."""
    rows = [l.split("|") for l in open(os.path.join(ROOT, "data/rbr.recomp.csv")).read().split("\n")[1:] if "|" in l]
    return {int(r[0], 16): r[1] for r in rows if r[1]}


def main(dll):
    asm = subprocess.run([OBJDUMP, "-d", "--x86-asm-syntax=intel", dll], capture_output=True, text=True, check=True).stdout
    lines = [l for l in asm.split("\n") if re.match(r"[0-9a-f]{8}:", l)]
    funcs, nm, src = functions(), names(), source_files()
    keys = [a for a, _ in funcs]

    def containing(a):
        i = bisect.bisect_right(keys, a) - 1
        return funcs[i][0] if i >= 0 and a < funcs[i][0] + max(funcs[i][1], 1) else None

    rows, seen = [], set()
    for i, line in enumerate(lines):
        m = re.search(r"call\t0x([0-9a-f]+)\b", line)
        target = int(m.group(1), 16) if m else None
        if target not in (PATCH_CTOR, ORIG_RESOLVE):
            continue
        # The exe address is the nearest earlier push in that range; for patches the push just
        # before the call is the length.
        length, addr = None, None
        for prev in reversed(lines[max(i - 40, 0):i]):
            p = re.search(r"push\t0x([0-9a-f]+)$", prev)
            if not p:
                continue
            v = int(p.group(1), 16)
            if target == PATCH_CTOR and length is None:
                length = v
            elif EXE_LO <= v < EXE_HI:
                addr = v
                break
        rows.append(("patch" if target == PATCH_CTOR else "orig", addr, length))
        seen.add(addr)
    for line in lines:
        for h in re.findall(r"0x([0-9a-f]{6,8})\b", line.split("\t", 1)[-1]):
            v = int(h, 16)
            if EXE_LO <= v < EXE_HI and v not in seen:
                rows.append(("vtable" if "vftable" in nm.get(v, "") else "ref", v, None))
                seen.add(v)

    summary = collections.Counter()
    for kind, addr, length in sorted(rows, key=lambda r: (r[0], r[1] or 0)):
        fn = containing(addr) if addr else None
        f = src.get(fn, "") if fn else ""
        print("\t".join([kind, f"{addr:08x}" if addr else "?", str(length or ""), f"{fn:08x}" if fn else "", f, nm.get(addr, "")]))
        m = re.match(r"RBR_E(\d+)_", f)
        summary[(kind, "physics" if m and 33 <= int(m.group(1)) <= 51 else "other")] += 1
    for k in sorted(summary):
        print(*k, summary[k], file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
