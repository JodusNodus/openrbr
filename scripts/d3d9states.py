#!/usr/bin/env python3
"""Per-draw D3D9 state summary from an apitrace text dump (harness/apitrace.sh).

Usage: python3 -I scripts/d3d9states.py DUMP SHADER_DIR... FIRST_FRAME LAST_FRAME

DUMP is `apitrace dump --color=never --arg-names=no` output. Names each shader object after the
.vso/.pso file under SHADER_DIRs with the same instructions (later directories win, so list the
install before the 1.02 patch), tracks render and sampler state through the whole trace, and prints
the distinct states used by the draws in frames FIRST..LAST, grouped by shader pair. Shaders that
HedgeHog3D assembles from text have no file and show as `Vertex?`/`Pixel?`.
Notes: notes/formats/shaders.md.
"""

import collections
import os
import pathlib
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import d3d9dis as dis

dump, shader_dirs = sys.argv[1], [pathlib.Path(p) for p in sys.argv[2:-2]]
first, last = int(sys.argv[-2]), int(sys.argv[-1])


def ops(lines):
    """Instructions with registers, without swizzles, masks or co-issue marks (formats differ)."""
    out = []
    for l in lines[1:]:
        l = l.strip().lstrip("+").strip()
        if l and not l.startswith(("dcl", "def")):
            out.append(re.sub(r"\.[xyzw]+", "", re.sub(r"\s+", " ", l)))
    return tuple(out)


known = {}
for d in shader_dirs:
    for f in sorted(d.rglob("*.?so")):
        known.setdefault(ops(dis.disasm(f.read_bytes())), set()).add(f.name)

RS = {"D3DRS_ALPHATESTENABLE", "D3DRS_ALPHAREF", "D3DRS_ALPHAFUNC", "D3DRS_ALPHABLENDENABLE", "D3DRS_SRCBLEND",
      "D3DRS_DESTBLEND", "D3DRS_CULLMODE", "D3DRS_ZWRITEENABLE", "D3DRS_ZENABLE", "D3DRS_FOGENABLE"}
SS = {"D3DSAMP_ADDRESSU", "D3DSAMP_ADDRESSV", "D3DSAMP_MAGFILTER", "D3DSAMP_MINFILTER", "D3DSAMP_MIPFILTER"}
names, rs, ss, tex = {}, {}, {}, {}
vs = ps = None
frame = 0
combos = collections.Counter()
pending = None
for line in open(dump, errors="replace"):
    if pending is not None:
        if line.startswith('", &0x'):
            ptr = re.match(r'", &(0x[0-9a-f]+)', line).group(1)
            kind, body = pending
            cands = known.get(ops(body))
            names[ptr] = ",".join(sorted(cands)) if cands else "%s?(%d ops)" % (kind, len(ops(body)))
            pending = None
        else:
            pending[1].append(line.strip())
        continue
    m = re.match(r'\d+ IDirect3DDevice9::(\w+)\(0x[0-9a-f]+, (.*)\)( = .*)?$', line.rstrip())
    if not m:
        m2 = re.match(r'\d+ IDirect3DDevice9::Create(Vertex|Pixel)Shader\(0x[0-9a-f]+, "(.*)', line.rstrip())
        if m2:
            pending = (m2.group(1), [m2.group(2)])
        continue
    fn, args = m.group(1), m.group(2)
    a = [x.strip() for x in args.split(",")]
    if fn == "Present":
        frame += 1
    elif fn == "SetVertexShader":
        vs = a[0]
    elif fn == "SetPixelShader":
        ps = a[0]
    elif fn == "SetRenderState" and a[0] in RS:
        rs[a[0][6:]] = a[1]
    elif fn == "SetSamplerState" and a[1] in SS:
        ss[(int(a[0]), a[1][8:])] = a[2]
    elif fn == "SetTexture":
        tex[int(a[0])] = a[1] not in ("NULL", "0x0")
    elif fn.startswith("Draw") and first <= frame <= last:
        stages = tuple(s for s in range(4) if tex.get(s))
        samp = tuple("%d:%s/%s" % (s, ss.get((s, "ADDRESSU"), "WRAP").replace("D3DTADDRESS_", ""), ss.get((s, "ADDRESSV"), "WRAP").replace("D3DTADDRESS_", "")) for s in stages)
        key = (names.get(vs, vs), names.get(ps, ps), tuple(sorted(rs.items())), samp)
        combos[key] += 1

by_pair = collections.defaultdict(list)
for (v, p, r, s), n in combos.items():
    by_pair[(v, p)].append((n, r, s))
for (v, p), rows in sorted(by_pair.items(), key=lambda kv: -sum(n for n, _, _ in kv[1])):
    print("== VS %s | PS %s  (%d draws)" % (v, p, sum(n for n, _, _ in rows)))
    for n, r, s in sorted(rows, reverse=True):
        print("   %5d  %s  tex %s" % (n, " ".join("%s=%s" % (k, v.replace("D3D", "")) for k, v in r), " ".join(s)))
