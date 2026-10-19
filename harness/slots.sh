#!/bin/bash
# Make harness slots 1..N-1 on the test machine as copies of slot 0 (the vanilla test install
# ~/$RBR_REMOTE_GAME and its prefix ~/$RBR_REMOTE_PREFIX, as play.sh), for parallel
# play.sh runs. Holds slot 0's lock while copying, so no game runs from it meanwhile.
# Usage: harness/slots.sh [N]   (default 4; existing slots are refreshed, so run it only when no game runs)
set -euo pipefail
host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
game=${RBR_REMOTE_GAME:?set RBR_REMOTE_GAME to the vanilla 1.02 test install on the test machine (notes/verification.md)}
wineprefix=${RBR_REMOTE_PREFIX:?set RBR_REMOTE_PREFIX to the Wine prefix of the test install (notes/verification.md)}
n=${1:-4}
exec lockf -t 7200 /tmp/rbr-harness.lock ssh -o BatchMode=yes "$host" bash -s -- "$n" <<'EOF2'
set -euo pipefail
cd ~/Games
for ((i = 1; i < $1; i++)); do
  rsync -a --delete --exclude 'RBRRecompRecord-*.bin' --exclude 'RBRRecomp*.log' rbr-vanilla-1.02/ rbr-vanilla-1.02-s$i/
  rsync -a --delete prefixes/rbr-vanilla/ prefixes/rbr-vanilla-s$i/
  echo "slot $i: $(du -sh rbr-vanilla-1.02-s$i | cut -f1) game, $(du -sh prefixes/rbr-vanilla-s$i | cut -f1) prefix"
done
EOF2
