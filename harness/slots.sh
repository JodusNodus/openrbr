#!/bin/bash
# Make harness slots 1..N-1 on cachy as copies of slot 0 (the vanilla test install
# ~/Games/rbr-vanilla-1.02 and its prefix ~/Games/prefixes/rbr-vanilla), for parallel
# play.sh runs. Holds slot 0's lock while copying, so no game runs from it meanwhile.
# Usage: harness/slots.sh [N]   (default 4; existing slots are refreshed, so run it only when no game runs)
set -euo pipefail
n=${1:-4}
exec lockf -t 7200 /tmp/rbr-harness.lock ssh -o BatchMode=yes cachy bash -s -- "$n" <<'EOF2'
set -euo pipefail
cd ~/Games
for ((i = 1; i < $1; i++)); do
  rsync -a --delete --exclude 'RBRRecompRecord-*.bin' --exclude 'RBRRecomp*.log' rbr-vanilla-1.02/ rbr-vanilla-1.02-s$i/
  rsync -a --delete prefixes/rbr-vanilla/ prefixes/rbr-vanilla-s$i/
  echo "slot $i: $(du -sh rbr-vanilla-1.02-s$i | cut -f1) game, $(du -sh prefixes/rbr-vanilla-s$i | cut -f1) prefix"
done
EOF2
