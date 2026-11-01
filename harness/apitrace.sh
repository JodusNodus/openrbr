#!/bin/bash
# D3D9 capture of a playback run: plays the drive (harness/play.sh) in one slot with apitrace's
# d3d9.dll wrapper in the game folder (in front of Proton's DXVK), then keeps the trace and its text
# dump in $RBR_APITRACE and copies the dump to <out dir>.
# Usage: [RBR_SLOT=3] [RBR_DRIVE=<name>] harness/apitrace.sh <name> <out dir>
#   Needs the portable apitrace build in $RBR_APITRACE (apitrace-14.0-win32 for the wrapper,
#   apitrace-14.0-Linux for `apitrace dump`; github.com/apitrace/apitrace releases). Holds the
#   slot's lock for the whole run and always removes the wrapper again. apitrace writes the trace
#   to the prefix's Desktop. Only about the first 200 frames get saved before the game exits.
#   Summarise with scripts/d3d9states.py; notes/formats.md "Shaders and vertex layouts".
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/dev-env.sh"
game=${RBR_GAME:?set RBR_GAME to the vanilla 1.02 install (notes/verification.md)}
wineprefix=${RBR_PREFIX:?set RBR_PREFIX to the Wine prefix of the install (notes/verification.md)}
apitrace=${RBR_APITRACE:?set RBR_APITRACE to the apitrace directory (notes/verification.md)}
name=$1 out=$2
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad run name" >&2; exit 1; }
slot=${RBR_SLOT:-3}
lock=/tmp/rbr-harness.lock; [ "$slot" = 0 ] || lock=/tmp/rbr-harness-s$slot.lock
if [ -z "${RBR_APITRACE_LOCKED:-}" ]; then
  exec env RBR_APITRACE_LOCKED=1 flock -w 7200 "$lock" "$0" "$@"
fi
sfx=; [ "$slot" = 0 ] || sfx=-s$slot
game_dir=$game$sfx desktop=$wineprefix$sfx/drive_c/users/steamuser/Desktop
cp "$apitrace/apitrace-14.0-win32/lib/wrappers/d3d9.dll" "$game_dir/d3d9.dll"
rm -f "$desktop"/RichardBurnsRally_SSE*.trace
rc=0
RBR_SLOT=$slot harness/play.sh "$name" || rc=$?
rm -f "$game_dir/d3d9.dll"
trace=$(ls -t "$desktop"/RichardBurnsRally_SSE*.trace 2>/dev/null | head -1)
[ -n "$trace" ] || { echo "no trace written" >&2; exit 1; }
mv "$trace" "$apitrace/$name.trace"
"$apitrace/apitrace-14.0-Linux/bin/apitrace" dump --color=never --arg-names=no "$apitrace/$name.trace" > "$apitrace/$name.txt" 2>/dev/null || true
ls -la "$apitrace/$name.trace" "$apitrace/$name.txt"
mkdir -p "$out"
cp "$apitrace/$name.txt" "$out/"
exit $rc
