#!/usr/bin/env python3
"""Packs a Poly Haven surface scan into one of the renderer's road scans (`crates/rbr-render/data/`,
`renderer/gravel.rs`): red its brightness (Rec. 709 of the stored colour), green and blue its OpenGL normal's x and y,
alpha its height, SIZE texels a side.

    scripts/pack-scan.py DIFF.png NOR_GL.png DISP.png SIZE OUT.png

The committed ones, from the 2k PNGs at SIZE 1024: `gravel.png` "Gravel Ground 01"
(https://polyhaven.com/a/gravel_ground_01, CC0), `asphalt.png` "Asphalt Track" (https://polyhaven.com/a/asphalt_track,
CC0), `snow.png` "Snow 02" (https://polyhaven.com/a/snow_02, CC0). Needs Pillow.
"""
import sys
from PIL import Image, ImageStat
diff, nor, disp, size, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
d = Image.open(diff).convert('RGB').resize((size, size), Image.LANCZOS)
r, g, b = d.split()
lum = Image.merge('RGB', (r, g, b)).convert('L', (0.2126, 0.7152, 0.0722, 0))
n = Image.open(nor).convert('RGB').resize((size, size), Image.LANCZOS)
h = Image.open(disp)
h = h.point(lambda v: v / 256) if h.mode.startswith('I') else h
h = h.convert('L').resize((size, size), Image.LANCZOS)
# Some of Poly Haven's scans store the normal and height sRGB-encoded (Gravel Ground 01: the normal's x and y average
# 0.72), others linear (the asphalts: 0.50), whatever their gAMA chunk says: the normal's mean tells, and the height
# shares its encoding.
if abs(ImageStat.Stat(n).mean[0] / 255 - 0.5) > 0.1:
    lin = lambda v: round(255 * ((v / 255 / 12.92) if v <= 10 else ((v / 255 + 0.055) / 1.055) ** 2.4))
    n = n.point(lin)
    h = h.point(lin)
nx, ny, _ = n.split()
Image.merge('RGBA', (lum, nx, ny, h)).save(out, optimize=True)
