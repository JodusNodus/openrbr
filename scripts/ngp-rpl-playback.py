"""Turn a game replay (.rpl) into a harness playback (RBRRecompPlayback.bin, notes/ngp.md "Replays").

A replay holds the quantized control (CLS, 9 bytes) of every physics step after the start, run-length coded in 12-byte
entries from 0x4c (u16 steps, u8 checksum, CLS); 0x44 holds the entry count, 0x48 the step count. Each step's
payload is the CLS as E16Quant::NotifyFilt (0x48d130) unpacks it. In vanilla this reproduces the live controls
exactly (test2.rpl against drive-tanner-corolla.bin: replay step k = recording step 161 + k). Under NGP the live car
gets the unquantized controls, so a playback of an NGP replay is the same drive with 8-bit controls, not the original.

Usage: python3 -I scripts/ngp-rpl-playback.py RPL OUT [T0]   (T0 = the step before the first replay step: 1520 under
NGP, 160 in vanilla)
"""
import struct, sys
import numpy as np

FRAME = 20 + 0x20 + 4 * 0xA00  # FRM1 header, control, unused wheel bytes (car bytes 0)
F = np.float32


def payload(cls):
    """NotifyFilt: throttle, brake, handbrake, clutch b4-b7 / 255, steering (signed b3) * 2/255, flags from b0, the
    gear b8 with flag 4, word 7 from b1 bit 0."""
    b0 = cls[0]
    flags = (b0 & 1) | (b0 & 2) | (0x10 if b0 & 0x20 else 0) | (0x20 if b0 & 0x40 else 0) | (4 if b0 & 4 else 0)
    floats = [F(cls[i]) * F(1 / 255) for i in (4, 5, 6, 7)] + [F(np.int8(cls[3] - 256 if cls[3] > 127 else cls[3])) * F(2 / 255)]
    return struct.pack("<5f3I", *floats, flags, cls[8] if b0 & 4 else 0, cls[1] & 1)


def main(src, dst, t0="1520"):
    d = open(src, "rb").read()
    n, steps = struct.unpack_from("<II", d, 0x44)
    s = int(t0)
    with open(dst, "wb") as f:
        for k in range(n):
            cnt, = struct.unpack_from("<H", d, 0x4C + 12 * k)
            frame = payload(d[0x4C + 12 * k + 3:0x4C + 12 * k + 12])
            for _ in range(cnt):
                f.write(struct.pack("<5I", 0x314D5246, s, s + 1, 1, 0) + frame + b"\0" * (FRAME - 52))
                s += 1
    assert s - int(t0) == steps, "entry counts don't add up to the step count"


if __name__ == "__main__":
    main(*sys.argv[1:4])
