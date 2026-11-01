#!/bin/bash
# Make harness slots 1..N-1 as copies of slot 0 (the vanilla install
# $RBR_GAME and its prefix $RBR_PREFIX, as play.sh), for parallel
# play.sh runs. Holds slot 0's lock while copying, so no game runs from it meanwhile.
# Usage: harness/slots.sh [N]   (default 4; existing slots are refreshed, so run it only when no game runs)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/dev-env.sh"
game=${RBR_GAME:?set RBR_GAME to the vanilla 1.02 install (notes/verification.md)}
wineprefix=${RBR_PREFIX:?set RBR_PREFIX to the Wine prefix of the install (notes/verification.md)}
n=${1:-4}
exec 9>/tmp/rbr-harness.lock
flock -w 7200 9 || { echo "slot 0 is busy" >&2; exit 1; }
for ((i = 1; i < n; i++)); do
  rsync -a --delete --exclude 'RBRRecompRecord-*.bin' --exclude 'RBRRecomp*.log' "$game/" "$game-s$i/"
  rsync -a --delete "$wineprefix/" "$wineprefix-s$i/"
  echo "slot $i: $(du -sh "$game-s$i" | cut -f1) game, $(du -sh "$wineprefix-s$i" | cut -f1) prefix"
done
