"""Retime a harness playback (RBRRecompPlayback.bin) from one step rate to another, e.g. a vanilla drive (100 Hz,
first step 160) for the NGP test install (720 Hz, first step 1520; notes/ngp.md). The car leaves the drive's line
sooner or later (other physics), so this is a long moving input for determinism runs, not a reference.

Usage: python3 -I scripts/ngp-retime-playback.py IN OUT [IN_T0 IN_HZ OUT_T0 OUT_HZ]   (default 160 100 1520 720)
Each output step takes the control of the input step at the same time. One-step events (flags: gear up/down,
gear select) are kept on the first output step of their input step only, so a shift happens once.
"""
import struct, sys

FRAME = 20 + 0x20 + 4 * 0xA00  # FRM1 header, control, unused wheel bytes (car bytes 0)


def controls(path):
    d, out, off = open(path, "rb").read(), {}, 0
    while off + 20 <= len(d):
        magic, pre, post, sub, cb = struct.unpack_from("<5I", d, off)
        assert magic == 0x314D5246, "not an FRM1 playback"
        out[pre] = d[off + 20:off + 52]
        off += 20 + 0x20 + cb + 4 * 0xA00
    return out


def main(src, dst, in_t0="160", in_hz="100", out_t0="1520", out_hz="720"):
    in_t0, in_hz, out_t0, out_hz = int(in_t0), int(in_hz), int(out_t0), int(out_hz)
    ctl = controls(src)
    last = max(ctl)
    with open(dst, "wb") as f:
        s, prev = out_t0, None
        while True:
            v = in_t0 + (s - out_t0) * in_hz // out_hz
            if v > last:
                break
            c = bytearray(ctl.get(v, b"\0" * 0x20))
            if v == prev:
                c[20:24] = b"\0" * 4  # flags only once per input step
            f.write(struct.pack("<5I", 0x314D5246, s, s + 1, 1, 0) + bytes(c) + b"\0" * (FRAME - 52))
            prev, s = v, s + 1


if __name__ == "__main__":
    main(*sys.argv[1:7])
