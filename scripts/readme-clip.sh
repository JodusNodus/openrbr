#!/bin/bash
# The README's cockpit clip: an NGP autopilot drive of an RSF car, drawn at a fixed frame rate and encoded as an
# animated WebP (GitHub plays it inline; it doesn't embed MP4 from the repo).
#
#   scripts/readme-clip.sh OUT.webp STAGE PACK TYRE FROM SECONDS [FPS WIDTH QUALITY]
#
# FROM is the game step the clip starts at (`viewer --at`), PACK the RSF car folder under $RBR_RSF/rsfdata/cars.
# Needs RBR_DATA, RBR_PATCH, RBR_RSF, RBR_NGP_PHYSICS, the release viewer and rbr-standalone in $BIN (default
# target/release), python3 with Pillow, and img2webp. The README's: track-45_N Audi_quattro_A2_GrpB_ngp6 0 9000 10.
set -euo pipefail
out=$1 stage=$2 pack=$3 tyre=$4 from=$5 seconds=$6 fps=${7:-24} width=${8:-960} quality=${9:-45}
bin=${BIN:-target/release}
car=$RBR_RSF/rsfdata/cars/$pack
work=$(mktemp -d /tmp/readme-clip.XXXXXX)
trap 'rm -rf "$work"' EXIT
# NGP steps at 720 Hz; the drive past the clip's end, and 10 frames before it for the temporal anti-aliasing.
warm=10 every=$((720 / fps))
steps=$((from + (seconds * fps + warm) * every))
nice -n 10 "$bin/rbr-standalone" --autopilot "$work/drive.bin" --boot --car 5 --weather 0 --tyre "$tyre" \
  --ngp-physics "$RBR_NGP_PHYSICS" --rsf-car "$car" --stage "$stage" --steps "$steps" --data "$RBR_DATA" > /dev/null
# Every `every`th frame, its substeps set so that the playback's 7 ms clock reads the real time (wind, dust, glass).
python3 -I -B - "$work/drive.bin" "$work/clip.bin" "$every" "$fps" <<'EOF'
import struct, sys
src, dst, every, fps = sys.argv[1], sys.argv[2], int(sys.argv[3]), float(sys.argv[4])
data = open(src, 'rb').read()
size = next(i for i in range(20, len(data), 4) if struct.unpack_from('<I', data, i)[0] == 0x314d5246)
out, done = bytearray(), 0
for n in range(len(data) // size // every):
    frame = bytearray(data[n * every * size:(n * every + 1) * size])
    total = round((n + 1) / fps / 0.007)
    struct.pack_into('<I', frame, 12, max(total - done, 1))
    done = total
    out += frame
open(dst, 'wb').write(out)
EOF
nice -n 10 "$bin/viewer" "$stage" --rsf-car "$car" --weather dry --view cockpit --no-hud --no-sound --size 1920x1080 \
  --enhance anisotropy,cabin,contact,hdr,particles,sky,sun,taa,water,wind --drive "$work/clip.bin" \
  --at $((from - warm * every)) --sequence $((seconds * fps + warm)) --screenshot "$work/f.png" > /dev/null
frames=("$work"/f-*.png)
python3 -I -B - "$width" "${frames[@]:$warm}" <<'EOF'
import sys
from PIL import Image
w = int(sys.argv[1])
for f in sys.argv[2:]:
    im = Image.open(f).convert('RGB')
    im.resize((w, round(im.height * w / im.width)), Image.LANCZOS).save(f, compress_level=1)
EOF
nice -n 10 img2webp -loop 0 -lossy -q "$quality" -m 4 -d $((1000 / fps)) "${frames[@]:$warm}" -o "$out"
ls -l "$out"
