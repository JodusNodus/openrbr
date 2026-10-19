"""leaves.py SRC: the bindings whose bodies make no host call, pass no whole Ctx on and use no esp."""
import re, sys, os
src = sys.argv[1]
files = [os.path.join(d, f) for d, _, fs in os.walk(src) for f in fs if f.endswith('.rs') and '/bin' not in d and '/pure' not in d]
consts, bodies, binds = {}, {}, []
def block(s, i):
    d = 0
    for k in range(i, len(s)):
        if s[k] == '{': d += 1
        elif s[k] == '}':
            d -= 1
            if d == 0: return k + 1
for p in files:
    s = open(p).read()
    for m in re.finditer(r'pub(?:\(crate\))? const (\w+): u32 = (0x[0-9a-fA-F_]+);', s): consts[m.group(1)] = int(m.group(2).replace('_', ''), 16)
    for m in re.finditer(r'\bconst (\w+): u32 = (0x[0-9a-fA-F_]+);', s): consts.setdefault(m.group(1), int(m.group(2).replace('_', ''), 16))
    for m in re.finditer(r'fn (\w+)\((?:_?ctx|_): &mut Ctx(?:<\'_>)?, (?:r|_r|_|regs): &Regs\) -> Result<Return, Decline> \{', s):
        e = block(s, m.end() - 1)
        bodies[(p, m.group(1))] = s[m.end():e]
    for m in re.finditer(r'bind\(\s*([\w:]+),\s*"([^"]*)",\s*([\w:]+)\s*\)', s):
        binds.append((p, m.group(1), m.group(2), m.group(3)))
out = []
for p, a, name, fn in binds:
    addr = int(a, 16) if a.startswith('0x') else consts.get(a.split('::')[-1])
    fname = fn.split('::')[-1]
    cands = [(q, f) for (q, f) in bodies if f == fname]
    same = [c for c in cands if c[0] == p] or [c for c in cands if os.path.dirname(c[0]) == os.path.dirname(p)]
    key = (same or cands or [None])[0]
    if addr is None or key is None:
        continue
    b = bodies[key]
    if re.search(r'host|esp|get_f32|SignalCallees|\bctx\)|\bctx,|\(ctx\b(?!\.)|&mut \*ctx|scope\(', b):
        continue
    # A cdecl body (its arguments read, nothing popped): its pop does not say how many words it reads.
    pops = re.findall(r'Return::\w+\([^;]*?,\s*(0x[0-9a-fA-F]+|\d+)\)\)', b)
    pops += re.findall(r'pop: (0x[0-9a-fA-F]+|\d+)', b)
    if re.search(r'\b(r|regs)\.stack', b) and pops and all(int(x, 0) == 0 for x in pops):
        continue
    out.append((addr, name, fname, os.path.relpath(key[0], src)))
for addr, name, fn, p in sorted(set(out)):
    print(f'{addr:#x} {name} {fn} {p}')
