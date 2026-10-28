#!/usr/bin/env python3
"""Packs a Poly Haven surface scan into the renderer's gravel texture (`crates/rbr-render/data/gravel.png`,
`renderer/gravel.rs`): red its brightness (Rec. 709 of the stored colour), green and blue its OpenGL normal's x and y,
alpha its height, SIZE texels a side.

    scripts/pack-gravel.py DIFF.png NOR_GL.png DISP.png SIZE OUT.png

The committed one: "Gravel Ground 01" (https://polyhaven.com/a/gravel_ground_01, CC0), the 2k PNGs, SIZE 1024.
Needs Pillow.
"""
import sys
from PIL import Image
diff, nor, disp, size, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
d = Image.open(diff).convert('RGB').resize((size, size), Image.LANCZOS)
r, g, b = d.split()
lum = Image.merge('RGB', (r, g, b)).convert('L', (0.2126, 0.7152, 0.0722, 0))
n = Image.open(nor).convert('RGB').resize((size, size), Image.LANCZOS)
h = Image.open(disp)
h = h.point(lambda v: v / 256) if h.mode.startswith('I') else h
h = h.convert('L').resize((size, size), Image.LANCZOS)
# Poly Haven's normal and height PNGs are stored sRGB-encoded (gAMA 0.45455): back to linear values.
lin = lambda v: round(255 * ((v / 255 / 12.92) if v <= 10 else ((v / 255 + 0.055) / 1.055) ** 2.4))
n = n.point(lin)
h = h.point(lin)
nx, ny, _ = n.split()
Image.merge('RGBA', (lum, nx, ny, h)).save(out, optimize=True)
