"""Converts a capture trace (RBR_TRACE, `RBRRecompTrace-<tick>.bin`) into vcu_vectors text records, for the
listed functions (shadow-eligible leaves only: the capture records nothing else). Each record keeps the thiscall
arguments (this, then the popped stack words), the windows before and after, and eax.

Usage: python3 -I -B harness/trace_to_vectors.py <trace.bin> <out.txt> <function address>...
Read by rust/rbr-physics/tests (RBR_ENT_VECTORS=<out.txt>).
"""
import struct, sys

def u32(d, o): return struct.unpack_from("<I", d, o)[0]

def records(d):
    off = 12
    while off + 4 <= len(d):
        assert u32(d, off) == 0x314C4143, hex(off)
        addr, index = u32(d, off + 4), u32(d, off + 8)
        entry = struct.unpack_from("<7I", d, off + 20)
        stack = struct.unpack_from("<16I", d, off + 48)
        exit_ = struct.unpack_from("<7I", d, off + 112)
        p = off + 140
        popped = u32(d, p); p += 20
        n = u32(d, p); p += 4
        regions = []
        for _ in range(n):
            base, at, size, nl = u32(d, p), u32(d, p + 4), u32(d, p + 8), u32(d, p + 12); p += 16
            name = d[p:p + nl].decode("latin1"); p += nl
            pre = d[p:p + size]; p += size
            post = d[p:p + size]; p += size
            regions.append((name, at, size, pre, post))
        yield dict(addr=addr, index=index, ecx=entry[2], stack=stack, eax=exit_[0], popped=popped, regions=regions)
        off = p

def main(src, dst, addrs):
    d = open(src, "rb").read()
    out = open(dst, "w")
    n = 0
    for r in records(d):
        if r["addr"] not in addrs:
            continue
        nargs = r["popped"] // 4
        args = [r["ecx"]] + list(r["stack"][:nargs])
        out.write("call %d %#x %s\n" % (n, r["addr"], "trace"))
        out.write("args %s\n" % " ".join("%08x" % a for a in args))
        for k, (name, at, size, pre, post) in enumerate(r["regions"]):
            out.write("win %d %#x %d pre %s\n" % (k, at, size, pre.hex()))
            out.write("win %d %#x %d post %s\n" % (k, at, size, post.hex()))
        out.write("ret %08x %016x\n" % (r["eax"], 0))
        out.write("branch -\n")
        out.write("end\n")
        n += 1
    out.close()
    print("%d records" % n)

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], {int(a, 16) for a in sys.argv[3:]})
