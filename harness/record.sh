#!/bin/bash
# Record a live reference drive: the user drives, the plugin records.
# The game shows on this machine's display.
# Usage: harness/record.sh <name> "<map> <car> <weather> <tyre>"
#   e.g. harness/record.sh snow-japan "12 3 2 5"
#   Sets up $RBR_GAME-rec (prefix $RBR_PREFIX-rec; play.sh has the variables) as a
#   copy of slot 0 (holding slot 0's lock while copying, like harness/slots.sh), starts
#   the stage from the 4 or 5 numbers (RBRRecompAuto.txt, no menus; the 5th is the damage setting, 0 off, 1 safe, 2 reduced, 3 realistic; the 6th the transmission, 0 manual, 1 automatic), and launches the game.
#   Drive the stage to the end or crash, then quit to desktop (ESC): the script waits
#   until the game quits, then keeps the recording as $RBR_RUNS/drive-<name>.bin
#   with the numbers in $RBR_RUNS/drive-<name>.auto.
#   The rec install never collides with the harness slots; slot 0's lock is released
#   before the game starts, so harness runs continue while the user drives (the drive
#   may stutter while slots render; the recording only holds inputs and state, so it
#   stays valid).
#   RBR_REC_MENUS=1: no auto-start; start the stage yourself through the menus, with the same
#   map, car, weather, tyre, damage and transmission as the numbers (they still go to
#   drive-<name>.auto for the playback). The auto-start skips the profile, so the menu route
#   is the one that loads your controller bindings (the Tanner reference was recorded so).
#   Refuses to overwrite an existing drive-<name>.bin. Every recording installs its plugin:
#   $RBR_HARNESS_PLUGIN (harness/plugin/build.sh --install).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/dev-env.sh"
game=${RBR_GAME:?set RBR_GAME to the vanilla 1.02 install (notes/verification.md)}
wineprefix=${RBR_PREFIX:?set RBR_PREFIX to the Wine prefix of the install (notes/verification.md)}
proton=${RBR_PROTON:-}
plugin=${RBR_HARNESS_PLUGIN:?set RBR_HARNESS_PLUGIN to the harness plugin (harness/plugin/build.sh --install)}
runs=${RBR_RUNS:?set RBR_RUNS to the harness runs directory (notes/verification.md)}
if [ $# != 2 ]; then echo "usage: harness/record.sh <name> \"<map> <car> <weather> <tyre>\"" >&2; exit 1; fi
name=$1 auto=$2
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad drive name" >&2; exit 1; }
[[ "$auto" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+(\ [0-3](\ [01])?)?$ ]] || { echo "bad auto-start numbers (want \"map car weather tyre [damage [transmission]]\", damage 0 off .. 3 realistic, transmission 0 manual 1 automatic)" >&2; exit 1; }
game_dir=$game-rec prefix=$wineprefix-rec

test ! -e "$runs/drive-$name.bin" \
  || { echo "$runs/drive-$name.bin exists; delete it first to re-record" >&2; exit 1; }

# Setup under slot 0's lock (slots.sh pattern): copy the rec install from slot 0, strip
# every unattended-run file, arm the recorder. No RBRRecompPlayback.bin: with it present
# the plugin would drop the player's inputs and drive the old recording instead.
exec 9>/tmp/rbr-harness.lock
flock -w 7200 9 || { echo "slot 0 is busy" >&2; exit 1; }
mkdir -p "$runs"
rsync -a --delete --exclude 'RBRRecompRecord-*.bin' --exclude 'RBRRecomp*.log' "$game/" "$game_dir/"
rsync -a --delete "$wineprefix/" "$prefix/"
cd "$game_dir"
rm -f RBRRecompPlayback.bin RBRRecompTransplant.bin RBRRecompRecord-*.bin RBRRecompHarness.log \
  RBRRecompCalls.log RBRRecompShadow.log RBRRecompShadow.on RBRRecompCount.on RBRRecompTrace.on \
  RBRRecompTrace-*.bin RBRRecompPeek.txt RBRRecompPeek-*.bin RBRRecompEarlyStop.txt \
  RBRRecompSnapshot.txt RBRRecompSnap-*.bin
touch RBRRecompRecord.on
echo "$auto" > RBRRecompAuto.txt
ls -la RBRRecompRecord.on RBRRecompAuto.txt
cat RBRRecompAuto.txt
exec 9>&-  # released before the game starts, so harness runs continue while the user drives
if [ "${RBR_REC_MENUS:-}" = 1 ]; then
  rm -f "$game_dir/RBRRecompAuto.txt" && echo "no auto-start: pick the stage in the menus"
fi

# Every recording installs its plugin (no recording inherits another run's build)
cp "$plugin" "$game_dir/Plugins/RBRRecompHarness.dll"

# Leftover processes of the rec prefix (play.sh's kill, for this prefix only).
want="WINEPREFIX=$prefix/pfx/"
pids() {
  for d in /proc/[0-9]*; do
    cat "$d"/task/*/environ 2>/dev/null | tr '\0' '\n' | grep -qxF "$want" || continue
    cat "$d"/task/*/cmdline 2>/dev/null | tr '\0' ' ' | grep -q RSF_Launcher && continue
    echo "${d#/proc/}"
  done
}
p=$(pids)
if [ -n "$p" ]; then
  echo "killing leftover processes of the rec prefix:" $p >&2
  kill $p 2>/dev/null || true
  sleep 3
  p=$(pids)
  [ -z "$p" ] || kill -9 $p 2>/dev/null || true
fi

echo "starting the game: drive the stage to the end or crash, then quit to desktop"
# as ~/.local/bin/rbr-vanilla, with the rec game folder and prefix; no timeout kill
(cd "$game_dir"
 export WINEPREFIX=$prefix ${proton:+PROTONPATH=$proton} \
   GAMEID=umu-rec WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-vanilla-run-rec.log 2>&1 &)
game_pat="[${game_dir:0:1}]${game_dir:1}/RichardBurnsRally_SSE.exe"
timeout 300 bash -c "until pgrep -f '$game_pat' >/dev/null; do sleep 2; done" || { echo "the game did not start" >&2; exit 1; }
echo "game running; waiting until it quits"
while pgrep -f "$game_pat" >/dev/null; do sleep 5; done
rec=$(ls -t "$game_dir"/RBRRecompRecord-*.bin 2>/dev/null | head -1 || true)
[ -n "$rec" ] || { echo "no recording" >&2; exit 1; }
mv "$rec" "$runs/drive-$name.bin"
echo "$auto" > "$runs/drive-$name.auto"
grep -E "auto:|stage started|playback|recorder" "$game_dir/RBRRecompHarness.log" 2>/dev/null || true
ls -la "$runs/drive-$name.bin" "$runs/drive-$name.auto"
