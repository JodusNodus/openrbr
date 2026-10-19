#!/bin/bash
# A reference drive without a driver: another drive's controls (drive-tanner-corolla's by default, or the
# playback of any drive on cachy, e.g. drive-arctic-snow) played back on another stage and car. The original's
# recording of that run is the reference, whatever the car does (it may crash or leave the road). Makes
# drive-<name> the way harness/record.sh and drive-setup.sh make a recorded one, then snapshots it.
# Usage: harness/v1-drive.sh <name> "<map car weather tyre damage transmission>" [source] [last]
#   e.g. harness/v1-drive.sh v2-gravel-zr-25 "25 2 0 4 1 0" tanner-corolla 3000
#   V1_SURVEY=1 stops after step 3: the moving steps of a source, with no references or snapshots.
#   V1_REPEAT=1 (harness/matrix.py) replaces steps 3-5: the playback runs a second time (ref-<name>-2.bin, once no
#      harness game runs on cachy), diverge.py compares it with drive-<name>.bin on the static mask, then also on
#      tanner-corolla's noise mask (ref1.bin.mask; no count runs, no mask of its own) and ref-2 is deleted.
#      drive-<name>.bin stays on cachy (for the Rust check there); nothing is copied to the Mac. No snapshots.
#   1. cachy: drive-<name>.auto (the StartGame numbers) and drive-<name>.playback.bin (the first <last> steps,
#      default 3000, of drive-<source>.playback.bin; default source tanner-corolla); refuses when any
#      drive-<name>*, ref-<name>* or vcu-<name>.* exists.
#   2. one playback run (play.sh <name>-rec); its recording is drive-<name>.bin, copied to recordings/.
#   3. the snapshot steps, from that recording (drive_path, this Mac): 1000, and <impact> = the start of the
#      largest speed drop over 10 steps if it drops more than 5 m/s, else the middle of the drive.
#      Prints the moving steps (speed > 0.5 m/s).
#   4. harness/drive-setup.sh <name>: ref-<name>-1/2, the count runs and the mask; ref-1 == ref-2.
#   5. one snapshot run (play.sh vcu-<name>, RBR_SNAPSHOT=<steps>): its recording must equal ref-<name>-1;
#      the snapshots go to snaps/vcu-<name>.snap-<step>.bin, linked from /tmp/rbr-snaps.
# Needs harness/drive-setup.sh and build/rbr (its count transplant), which are in the rbr-recomp repository: run a
# new drive from an rbr-recomp checkout. V1_SURVEY=1 and V1_REPEAT=1 don't use them.
set -euo pipefail
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
# the shared recordings and snapshots by their resolved paths (the links in /tmp outlive this worktree)
mkdir -p "$repo/../recordings" "$repo/../snaps" /tmp/rbr-snaps
recordings=$(cd "$repo/../recordings" && pwd -P) snaps=$(cd "$repo/../snaps" && pwd -P)
drive_path=$repo/rust/target/release/drive_path
if [ "${V1_REPEAT:-0}" != 1 ] && [ ! -x "$drive_path" ]; then (cd rust && nice -n 10 cargo build --release -p rbr-recording --bin drive_path -j 4); fi

# 1. the drive's files on cachy (playback = the source's controls, cut to <last> steps; the numbers decide the
#    stage and car)
ssh -o BatchMode=yes cachy bash -s -- "$name" "$source" "$last" $auto <<'REMOTE1'
set -euo pipefail
cd ~/rbr-harness-runs
if compgen -G "drive-$1.*" || compgen -G "ref-$1-*" || compgen -G "vcu-$1.*" >/dev/null; then
  echo "drive-$1, ref-$1 or vcu-$1 files exist on cachy; choose another name" >&2; exit 1
fi
[ -f drive-"$2".playback.bin ] || { echo "no drive-$2.playback.bin on cachy" >&2; exit 1; }
# drive-setup.sh's trim rule: keep the records with post <= last
python3 -I -B - drive-"$2".playback.bin drive-"$1".playback.bin "$3" <<'PYEOF'
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
echo "${@:4}" > drive-"$1".auto
cat drive-"$1".auto
REMOTE1

# 2. the playback's own recording is the drive's recording
RBR_DRIVE=$name RBR_EARLYSTOP=0 harness/play.sh "$name"-rec
ssh -o BatchMode=yes cachy bash -s -- "$name" <<'REMOTE2'
set -euo pipefail
cd ~/rbr-harness-runs
mv "$1"-rec.bin drive-"$1".bin
ls -la drive-"$1".bin
REMOTE2
if [ "${V1_REPEAT:-0}" = 1 ]; then
  while ssh -o BatchMode=yes cachy "pgrep -f 'rbr-vanilla-1[.]02.*RichardBurnsRally_SSE' >/dev/null"; do
    echo "cachy: a harness game is running; waiting"; sleep 30
  done
  RBR_DRIVE=$name RBR_EARLYSTOP=0 harness/play.sh ref-"$name"-2
  scp -q harness/diverge.py cachy:/tmp/rbr-diverge-"$name".py
  ssh -o BatchMode=yes cachy bash -s -- "$name" <<'REPEAT'
set -euo pipefail
cd ~/rbr-harness-runs
# twice: on the static mask alone, then also on tanner-corolla's noise mask (ref1.bin.mask: the words its ref-2 and
# count runs differ in, the heap and stack residue of a new process)
out=$(python3 -I /tmp/rbr-diverge-"$1".py drive-"$1".bin ref-"$1"-2.bin 2>&1) || true
echo "repeat static: ${out//$'\n'/; }"
ln -sfn ref1.bin.mask drive-"$1".bin.mask
out=$(python3 -I /tmp/rbr-diverge-"$1".py drive-"$1".bin ref-"$1"-2.bin 2>&1) || true
echo "repeat noise: ${out//$'\n'/; }"
rm -f drive-"$1".bin.mask ref-"$1"-2.bin /tmp/rbr-diverge-"$1".py
REPEAT
  exit 0
fi
scp -q cachy:rbr-harness-runs/drive-"$name".bin cachy:rbr-harness-runs/drive-"$name".auto "$recordings/"

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
if [ "${V1_SURVEY:-0}" = 1 ]; then echo "survey only: $name (its files stay on cachy)"; exit 0; fi

# 4. the references (drive-setup.sh's five runs and the mask)
harness/drive-setup.sh "$name"
# the count runs only make the mask (drive-setup.sh): their recordings are not kept
ssh -o BatchMode=yes cachy bash -s -- "$name" <<'REMOTE4'
set -euo pipefail
cd ~/rbr-harness-runs
python3 -I -B - "$1" <<'PYEOF'
import os, sys
name = sys.argv[1]
for number in (1, 2, 3):
    path = "ref-%s-count%d.bin" % (name, number)
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
PYEOF
REMOTE4

# 5. the snapshot run: a snapshot does not change the run (its recording must equal ref-<name>-1)
RBR_DRIVE=$name RBR_EARLYSTOP=0 RBR_SNAPSHOT="$steps" harness/play.sh vcu-"$name"
scp -q harness/diverge.py cachy:/tmp/rbr-drive-diverge.py
# the plugin also dumps the step after each listed one (and after-call copies): keep the listed ones only
ssh -o BatchMode=yes cachy bash -s -- "$name" $steps <<'REMOTE5'
set -euo pipefail
cd ~/rbr-harness-runs
name=$1; shift
# a difference is reported, not fatal: the snapshots are kept either way
python3 -I /tmp/rbr-drive-diverge.py ref-"$name"-1.bin vcu-"$name".bin || echo "vcu-$name differs from ref-$name-1 (exit $?)" >&2
python3 -I -B - "$name" "$@" <<'PYEOF'
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
REMOTE5
for step in $steps; do
  scp -q cachy:rbr-harness-runs/vcu-"$name".snap-"$step".bin "$snaps/"
  ln -sf "$snaps/vcu-$name.snap-$step.bin" /tmp/rbr-snaps/
done
ls -la "$snaps"/vcu-"$name".snap-*.bin
