#!/usr/bin/env python3
"""Minimal D3D9 shader model 1.x bytecode disassembler (vs_1_1 / ps_1_x).

Usage: python3 -I scripts/d3d9dis.py FILE...

Prints the instructions in the D3DX assembler's syntax (as apitrace shows them too); skips comment
blocks (the retail files carry a `DBUG` block with source paths). Notes: notes/formats.md "Shaders and vertex layouts".
"""

import struct
import sys

OPS = {0: "nop", 1: "mov", 2: "add", 3: "sub", 4: "mad", 5: "mul", 6: "rcp", 7: "rsq", 8: "dp3", 9: "dp4",
       10: "min", 11: "max", 12: "slt", 13: "sge", 14: "exp", 15: "log", 16: "lit", 17: "dst", 18: "lrp",
       19: "frc", 20: "m4x4", 21: "m4x3", 22: "m3x4", 23: "m3x3", 24: "m3x2", 31: "dcl", 64: "texcoord",
       65: "texkill", 66: "tex", 67: "texbem", 68: "texbeml", 69: "texreg2ar", 70: "texreg2gb",
       71: "texm3x2pad", 72: "texm3x2tex", 73: "texm3x3pad", 74: "texm3x3tex", 76: "texm3x3spec",
       77: "texm3x3vspec", 78: "expp", 79: "logp", 80: "cnd", 81: "def", 82: "texreg2rgb", 83: "texdp3tex",
       84: "texm3x2depth", 85: "texdp3", 86: "texm3x3", 87: "texdepth", 88: "cmp", 89: "bem", 0xFFFD: "phase"}
USAGE = ["position", "blendweight", "blendindices", "normal", "psize", "texcoord", "tangent", "binormal",
         "tessfactor", "positiont", "color", "fog", "depth", "sample"]
SRCMOD = ["{}", "-{}", "{}_bias", "-{}_bias", "{}_bx2", "-{}_bx2", "1-{}", "{}_x2", "-{}_x2", "{}_dz", "{}_dw",
          "{}_abs", "-{}_abs", "!{}"]
SHIFT = {1: "_x2", 2: "_x4", 3: "_x8", 15: "_d2", 14: "_d4", 13: "_d8"}


def reg(tok, pixel):
    t = ((tok >> 28) & 7) | ((tok >> 8) & 0x18)
    n = tok & 0x7FF
    if t == 0:
        return "r%d" % n
    if t == 1:
        return "v%d" % n
    if t == 2:
        return "c[a0.x + %d]" % n if tok & 0x2000 else "c%d" % n
    if t == 3:
        return "t%d" % n if pixel else "a%d" % n
    if t == 4:
        return ["oPos", "oFog", "oPts"][n]
    if t == 5:
        return "oD%d" % n
    if t == 6:
        return "oT%d" % n
    return "?%d_%d" % (t, n)


def dst(tok, pixel):
    mask = (tok >> 16) & 0xF
    mod = "_sat" if (tok >> 20) & 1 else ""
    shift = SHIFT.get((tok >> 24) & 0xF, "")
    m = "" if mask == 0xF else "." + "".join(c for i, c in enumerate("xyzw") if mask >> i & 1)
    return reg(tok, pixel) + m, shift + mod


def src(tok, pixel):
    sw = (tok >> 16) & 0xFF
    comps = "".join("xyzw"[(sw >> (2 * i)) & 3] for i in range(4))
    s = "" if comps == "xyzw" else "." + (comps[0] if len(set(comps)) == 1 else comps)
    return SRCMOD[(tok >> 24) & 0xF].format(reg(tok, pixel) + s)


def disasm(data):
    toks = struct.unpack("<%dI" % (len(data) // 4), data)
    ver = toks[0]
    pixel = ver >> 16 == 0xFFFF
    out = ["%s_%d_%d" % ("ps" if pixel else "vs", (ver >> 8) & 0xFF, ver & 0xFF)]
    i = 1
    while i < len(toks):
        t = toks[i]
        op = t & 0xFFFF
        if op == 0xFFFF:
            break
        if op == 0xFFFE:
            i += 1 + ((t >> 16) & 0x7FFF)
            continue
        i += 1
        params = []
        if op == 81:  # def: dst + 4 floats
            params = [toks[i]]
            vals = struct.unpack("<4f", struct.pack("<4I", *toks[i + 1:i + 5]))
            out.append("def %s, %s" % (reg(toks[i], pixel), ", ".join("%g" % v for v in vals)))
            i += 5
            continue
        while i < len(toks) and toks[i] & 0x80000000:
            params.append(toks[i])
            i += 1
        name = OPS.get(op, "op%d" % op)
        coissue = "+" if t & 0x40000000 else ""
        if op == 31:
            u = params[0]
            usage = USAGE[u & 0x1F] + (str((u >> 16) & 0xF) if (u >> 16) & 0xF else "")
            d, _ = dst(params[1], pixel)
            out.append("dcl_%s %s" % (usage, d))
            continue
        if not params:
            out.append(coissue + name)
            continue
        d, mod = dst(params[0], pixel)
        out.append("%s%s%s %s" % (coissue, name, mod, ", ".join([d] + [src(p, pixel) for p in params[1:]])))
    return out


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print("// " + path)
        print("\n".join(disasm(open(path, "rb").read())))
        print()
