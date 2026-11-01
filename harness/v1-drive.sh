#!/bin/bash
# A reference drive without a driver: another drive's controls (drive-tanner-corolla's by default, or the
# playback of any drive, e.g. drive-arctic-snow) played back on another stage and car. The original's
# recording of that run is the reference, whatever the car does (it may crash or leave the road). Makes
# drive-<name> the way harness/record.sh makes a recorded one, then snapshots it.
# Usage: harness/v1-drive.sh <name> "<map car weather tyre damage transmission>" [source] [last]
#   e.g. harness/v1-drive.sh v2-gravel-zr-25 "25 2 0 4 1 0" tanner-corolla 3000
#   V1_SURVEY=1 stops after step 3: the moving steps of a source, with no snapshots.
#   V1_REPEAT=1 (harness/matrix.py) replaces steps 3-4: the playback runs a second time (ref-<name>-2.bin, once no
#      harness game runs), diverge.py compares it with drive-<name>.bin on the static mask, then also on
#      tanner-corolla's noise mask (ref1.bin.mask; no count runs, no mask of its own) and ref-2 is deleted.
#      drive-<name>.bin stays in $RBR_RUNS (for the Rust check); nothing is copied to the recordings. No snapshots.
#   1. the drive's files (drive-<name>.auto and drive-<name>.playback.bin, the first <last> steps,
#      default 3000, of drive-<source>.playback.bin; default source tanner-corolla); refuses when any
#      drive-<name>*, ref-<name>* or vcu-<name>.* exists.
#   2. one playback run (play.sh <name>-rec); its recording is drive-<name>.bin, copied to recordings/.
#   3. the snapshot steps, from that recording (drive_path): 1000, and <impact> = the start of the
#      largest speed drop over 10 steps if it drops more than 5 m/s, else the middle of the drive.
#      Prints the moving steps (speed > 0.5 m/s).
#   4. one snapshot run (play.sh vcu-<name>, RBR_SNAPSHOT=<steps>): its recording must equal drive-<name>.bin;
#      the snapshots go to $RBR_SNAPS (default /tmp/rbr-snaps).
# The references and noise mask of rbr-recomp's drive-setup.sh (count runs with the C++ build's transplant) are not
# made here: run that from an rbr-recomp checkout when a drive needs them. Recordings go to $RBR_RECORDINGS.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/dev-env.sh"
if [ $# -lt 2 ] || [ $# -gt 4 ]; then
  echo "usage: harness/v1-drive.sh <name> \"<map car weather tyre damage transmission>\" [source] [last]" >&2; exit 1
fi
name=$1 auto=$2 source=${3:-tanner-corolla} last=${4:-3000}
[[ "$name" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "bad drive name (lower-case letters, digits, hyphens)" >&2; exit 1; }
[[ "$source" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "bad source drive name" >&2; exit 1; }
[[ "$last" =~ ^[1-9][0-9]*$ ]] || { echo "bad last step (a positive number)" >&2; exit 1; }
[[ "$auto" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+\ [0-3]\ [01]$ ]] || { echo "bad numbers (want \"map car weather tyre damage transmission\", damage 0..3, transmission 0 or 1)" >&2; exit 1; }
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo"
recordings=${RBR_RECORDINGS:?set RBR_RECORDINGS to the recordings directory (notes/verification.md)}
runs=${RBR_RUNS:?set RBR_RUNS to the harness runs directory (notes/verification.md)}
snaps=${RBR_SNAPS:-/tmp/rbr-snaps}
mkdir -p "$recordings" "$snaps" "$runs"
drive_path=$repo/target/release/drive_path
if [ "${V1_REPEAT:-0}" != 1 ] && [ ! -x "$drive_path" ]; then (nice -n 10 cargo build --release -p rbr-recording --bin drive_path -j 4); fi

# 1. the drive's files (playback = the source's controls, cut to <last> steps; the numbers decide the
#    stage and car)
(
cd "$runs"
if compgen -G "drive-$name.*" || compgen -G "ref-$name-*" || compgen -G "vcu-$name.*" >/dev/null; then
  echo "drive-$name, ref-$name or vcu-$name files exist; choose another name" >&2; exit 1
fi
[ -f drive-"$source".playback.bin ] || { echo "no drive-$source.playback.bin" >&2; exit 1; }
# drive-setup.sh's trim rule: keep the records with post <= last
python3 -I -B - drive-"$source".playback.bin drive-"$name".playback.bin "$last" <<'PYEOF'
import struct, sys
d = open(sys.argv[1], "rb").read()
last = int(sys.argv[3])
p = 0
while p + 20 <= len(d):
    m, pre, post, sub, cs = struct.unpack_from("<IIIII", d, p)
    assert m == 0x314d5246, "bad magic at offset %d" % p
    if post > last:
        break
    p += 20 + 0x20 + cs + 4 * 0xa00
assert p > 0, "no records kept"
open(sys.argv[2], "wb").write(d[:p])
print("%s: %d bytes (of %d)" % (sys.argv[2], p, len(d)))
PYEOF
echo "$auto" > drive-"$name".auto
cat drive-"$name".auto
)

# 2. the playback's own recording is the drive's recording
RBR_DRIVE=$name harness/play.sh "$name"-rec
mv "$runs/$name-rec.bin" "$runs/drive-$name.bin"
ls -la "$runs/drive-$name.bin"
if [ "${V1_REPEAT:-0}" = 1 ]; then
  while pgrep -f '[R]ichardBurnsRally_SSE\.exe' >/dev/null; do
    echo "a harness game is running; waiting"; sleep 30
  done
  RBR_DRIVE=$name harness/play.sh ref-"$name"-2
  (
  cd "$runs"
  # twice: on the static mask alone, then also on tanner-corolla's noise mask (ref1.bin.mask: the words its ref-2 and
  # count runs differ in, the heap and stack residue of a new process)
  out=$(python3 -I "$repo/harness/diverge.py" drive-"$name".bin ref-"$name"-2.bin 2>&1) || true
  echo "repeat static: ${out//$'\n'/; }"
  ln -sfn ref1.bin.mask drive-"$name".bin.mask
  out=$(python3 -I "$repo/harness/diverge.py" drive-"$name".bin ref-"$name"-2.bin 2>&1) || true
  echo "repeat noise: ${out//$'\n'/; }"
  rm -f drive-"$name".bin.mask ref-"$name"-2.bin
  )
  exit 0
fi
cp "$runs/drive-$name.bin" "$runs/drive-$name.auto" "$recordings/"

# 3. the snapshot steps (drive_path: one row per physics step; speed in the CSV)
csv=$(mktemp /tmp/v1-drive-path.XXXXXX.csv)
"$drive_path" "$recordings/drive-$name.bin" --every 1 --csv "$csv" >/dev/null
steps=$(python3 -I -B - "$csv" <<'PY'
import csv, sys
rows = {int(r["step"]): float(r["speed"]) for r in csv.DictReader(open(sys.argv[1]))}
last = max(rows)
drop, impact = max((rows[s] - rows[s + 10], s) for s in rows if s + 10 in rows)
print("last %d" % last, file=sys.stderr)
print("moving steps (speed > 0.5): %d" % sum(v > 0.5 for v in rows.values()), file=sys.stderr)
print("largest speed drop over 10 steps: %.2f at step %d" % (drop, impact), file=sys.stderr)
picks = [1000, impact if drop > 5 else last // 2]
print(" ".join(str(s) for s in sorted(set(picks))))
PY
)
rm -f "$csv"
echo "snapshot steps: $steps"
# V1_SURVEY=1: the moving steps of this source only (steps 1-3); no references, no snapshots
if [ "${V1_SURVEY:-0}" = 1 ]; then echo "survey only: $name (its files stay in $runs)"; exit 0; fi

# 4. the snapshot run: a snapshot does not change the run (its recording must equal the drive's)
RBR_DRIVE=$name RBR_SNAPSHOT="$steps" harness/play.sh vcu-"$name"
# the plugin also dumps the step after each listed one (and after-call copies): keep the listed ones only
(
cd "$runs"
# a difference is reported, not fatal: the snapshots are kept either way
python3 -I "$repo/harness/diverge.py" drive-"$name".bin vcu-"$name".bin || echo "vcu-$name differs from drive-$name (exit $?)" >&2
python3 -I -B - "$name" $steps <<'PYEOF'
import os, sys
name = sys.argv[1]
steps = {int(value) for value in sys.argv[2:]}
paths = ["vcu-%s.bin" % name]
for step in steps:
    paths.append("vcu-%s.snap-%d-after.bin" % (name, step))
    following = step + 1
    if following not in steps:
        paths.append("vcu-%s.snap-%d.bin" % (name, following))
for path in paths:
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
PYEOF
)
for step in $steps; do
  cp "$runs/vcu-$name.snap-$step.bin" "$snaps/"
done
ls -la "$snaps"/vcu-"$name".snap-*.bin
