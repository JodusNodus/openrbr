#!/bin/bash
# Which StartGame numbers reproduce a menu-started drive? Plays drive-<drive>.bin back with the
# candidate numbers (a throwaway drive name <drive>-<tag> sharing the playback) and prints, per
# sampled step, how many words of the run differ from the live recording. Right numbers: the count
# stays at the heap-noise baseline; wrong ones: it grows within a few hundred steps.
# Usage: harness/drive-trial.sh <drive> "<map car weather tyre damage trans>" <tag>
set -euo pipefail
drive=$1 auto=$2 tag=$3 t=$1-$3
ssh -o BatchMode=yes cachy bash -s -- "$drive" "$t" $auto <<'EOF'
set -euo pipefail
cd ~/rbr-harness-runs
[ -f drive-"$1".playback.bin ] || python3 - "$1" <<'PY'
import struct, sys
d = open("drive-%s.bin" % sys.argv[1], "rb").read(); p = 0
while p + 20 <= len(d):
    m, pre, post, sub, cs = struct.unpack_from("<IIIII", d, p)
    n = 20 + 0x20 + cs + 4 * 0xa00
    if m != 0x314d5246 or p + n > len(d): break
    p += n
open("drive-%s.playback.bin" % sys.argv[1], "wb").write(d[:p])
PY
ln -f drive-"$1".playback.bin drive-"$2".playback.bin
echo "${*:3}" > drive-"$2".auto
EOF
RBR_DRIVE=$t RBR_EARLYSTOP=0 harness/play.sh trial-$t >/tmp/drive-trial-$t.log 2>&1 || true
ssh -o BatchMode=yes cachy bash -s -- "$drive" "$t" <<'EOF'
cd ~/rbr-harness-runs
python3 - drive-"$1".bin trial-"$2".bin <<'PY'
import struct, sys
import numpy as np
def load(path):
    d = open(path, "rb").read(); p = 0; out = {}
    while p + 20 <= len(d):
        m, pre, post, sub, cs = struct.unpack_from("<IIIII", d, p)
        n = 0x20 + cs + 4 * 0xa00
        if p + 20 + n > len(d): break
        out[post] = np.frombuffer(d, np.uint32, n // 4, p + 20); p += 20 + n
    return out
a, b = load(sys.argv[1]), load(sys.argv[2])
common = sorted(set(a) & set(b))
if not common: print("no common steps (run failed?)"); sys.exit()
picks = [s for s in common if s in (41, 45, 50, 60, 80, 100, 300, 1000, 3000, 6000, 10000, 15000, 20000)] + [common[-1]]
print("steps %d..%d (%d common; drive has %d)" % (common[0], common[-1], len(common), len(a)))
print("  ".join("%d:%d" % (s, int((a[s] != b[s]).sum())) for s in picks))
PY
rm -f drive-"$2".playback.bin drive-"$2".auto
EOF
