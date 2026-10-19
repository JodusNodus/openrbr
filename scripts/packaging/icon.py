"""ReRBR's icon (ours): a stage road winding to the horizon at dusk, on a rounded square.

Writes the 1024x1024 PNG the packages use: python3 -I -B scripts/packaging/icon.py scripts/packaging/icon.png (needs numpy),
and with a .ico path (scripts/packaging/icon.ico) the Windows icon rerbr.exe embeds: the PNG at 256 down to 16 pixels.
"""

import struct
import sys
import zlib

import numpy as np

SIZE = 1024
SS = 4  # samples per pixel along each axis
HORIZON = 0.40


def mix(a, b, t):
    return a + (b - a) * t[..., None]


def render():
    n = SIZE * SS
    v, u = np.mgrid[0:n, 0:n].astype(np.float32)
    u, v = (u + 0.5) / n, (v + 0.5) / n

    # Sky: night blue at the top to an orange glow at the horizon; ground: dark green-grey.
    sky = mix(np.array([0.06, 0.09, 0.20]), np.array([0.98, 0.52, 0.18]), np.clip(v / HORIZON, 0, 1) ** 2.2)
    ground = mix(np.array([0.10, 0.14, 0.12]), np.array([0.05, 0.07, 0.07]), np.clip((v - HORIZON) / (1 - HORIZON), 0, 1))
    rgb = np.where((v < HORIZON)[..., None], sky, ground)

    # The sun, half set.
    sun = np.clip((0.13 - np.hypot(u - 0.68, v - HORIZON)) * n / 2, 0, 1) * (v < HORIZON)
    rgb = mix(rgb, np.array([1.0, 0.82, 0.45]), sun)

    # The road: t runs from the bottom edge (0) to the horizon (1); centre and width shrink with the distance.
    t = np.clip((1 - v) / (1 - HORIZON), 0, 1)
    depth = 1 - (1 - t) ** 1.6
    centre = 0.5 + 0.13 * np.sin(depth * 2.4 * np.pi) * (1 - depth) ** 0.5
    half = 0.24 * (1 - depth) ** 1.3 + 0.004
    off = np.abs(u - centre)
    road = np.clip((half - off) * n / 2, 0, 1) * (v > HORIZON)
    rgb = mix(rgb, np.array([0.84, 0.78, 0.66]) * (0.75 + 0.25 * (1 - depth))[..., None], road)

    # Its centre line, dashed in the distance.
    dash = (np.mod(depth * 9, 1) < 0.55) & (v > HORIZON)
    line = np.clip((half * 0.07 - off) * n / 2, 0, 1) * dash
    rgb = mix(rgb, np.array([0.98, 0.45, 0.10]), line)

    # The rounded square (macOS's grid: 824 of 1024, corner radius 185).
    inset, radius = 100 / SIZE, 185 / SIZE
    du = np.maximum(np.abs(u - 0.5) - (0.5 - inset - radius), 0)
    dv = np.maximum(np.abs(v - 0.5) - (0.5 - inset - radius), 0)
    alpha = np.clip((radius - np.hypot(du, dv)) * n / 2, 0, 1)

    rgba = np.concatenate([rgb * alpha[..., None], alpha[..., None]], axis=-1)
    rgba = rgba.reshape(SIZE, SS, SIZE, SS, 4).mean(axis=(1, 3))
    # Straight (not premultiplied) colour, as PNG stores it.
    a = rgba[..., 3:4]
    rgba[..., :3] = np.where(a > 0, rgba[..., :3] / np.maximum(a, 1e-6), 0)
    return (np.clip(rgba, 0, 1) * 255 + 0.5).astype(np.uint8)


def png(pixels):
    height, width, _ = pixels.shape
    raw = b"".join(b"\0" + pixels[y].tobytes() for y in range(height))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def ico(pixels):
    """An ICO of PNG images, box-filtered from `pixels` (1024 = 256 * 4)."""
    images = []
    for size in (256, 128, 64, 32, 16):
        k = SIZE // size
        small = pixels.astype(np.float32).reshape(size, k, size, k, 4)
        # Averaged with the colour weighted by alpha, so the transparent corners do not darken the edge.
        a = small[..., 3:4]
        rgb = (small[..., :3] * a).sum(axis=(1, 3)) / np.maximum(a.sum(axis=(1, 3)), 1e-6)
        rgba = np.concatenate([rgb, a.mean(axis=(1, 3))], axis=-1)
        images.append((size, png((rgba + 0.5).astype(np.uint8))))
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in images:
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(data for _, data in images)


if __name__ == "__main__":
    pixels = render()
    with open(sys.argv[1], "wb") as out:
        out.write(ico(pixels) if sys.argv[1].endswith(".ico") else png(pixels))
