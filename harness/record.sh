#!/bin/bash
# Record a live reference drive on cachy: the user drives, the plugin records.
# Run from the Mac; the game shows on cachy's display.
# Usage: harness/record.sh <name> "<map> <car> <weather> <tyre>"
#   e.g. harness/record.sh snow-japan "12 3 2 5"
#   Sets up ~/Games/rbr-vanilla-1.02-rec (prefix ~/Games/prefixes/rbr-vanilla-rec) as a
#   copy of slot 0 (holding slot 0's lock while copying, like harness/slots.sh), starts
#   the stage from the 4 or 5 numbers (RBRRecompAuto.txt, no menus; the 5th is the damage setting, 0 off, 1 safe, 2 reduced, 3 realistic; the 6th the transmission, 0 manual, 1 automatic), and launches the game.
#   Drive the stage to the end or crash, then quit to desktop (ESC): the script waits
#   until the game quits, then keeps the recording as ~/rbr-harness-runs/drive-<name>.bin
#   with the numbers in ~/rbr-harness-runs/drive-<name>.auto.
#   The rec install never collides with the harness slots; slot 0's lock is released
#   before the game starts, so harness runs continue while the user drives (the drive
#   may stutter while slots render; the recording only holds inputs and state, so it
#   stays valid). Never touches ~/Games/Richard Burns Rally or ~/Games/prefixes/rbr.
#   RBR_REC_MENUS=1: no auto-start; start the stage yourself through the menus, with the same
#   map, car, weather, tyre, damage and transmission as the numbers (they still go to
#   drive-<name>.auto for the playback). The auto-start skips the profile, so the menu route
#   is the one that loads your controller bindings (the Tanner reference was recorded so).
#   Refuses to overwrite an existing drive-<name>.bin. Every run installs its plugin:
#   RBR_HARNESS_PLUGIN, or ~/rbr-harness-plugin on cachy (harness/plugin/build.sh --install).
set -euo pipefail
if [ $# != 2 ]; then echo "usage: harness/record.sh <name> \"<map> <car> <weather> <tyre>\"" >&2; exit 1; fi
name=$1 auto=$2
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad drive name" >&2; exit 1; }
[[ "$auto" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+(\ [0-3](\ [01])?)?$ ]] || { echo "bad auto-start numbers (want \"map car weather tyre [damage [transmission]]\", damage 0 off .. 3 realistic, transmission 0 manual 1 automatic)" >&2; exit 1; }
game_dir=Games/rbr-vanilla-1.02-rec prefix=Games/prefixes/rbr-vanilla-rec

ssh -o BatchMode=yes cachy bash -s -- "$name" <<'EOF'
test ! -e rbr-harness-runs/drive-"$1".bin \
  || { echo "rbr-harness-runs/drive-$1.bin exists; delete it on cachy first to re-record" >&2; exit 1; }
EOF

# Setup under slot 0's lock (slots.sh pattern): rsync the rec install from slot 0, strip
# every unattended-run file, arm the recorder. No RBRRecompPlayback.bin: with it present
# the plugin would drop the player's inputs and drive the old recording instead.
# NOTE: ssh joins its command args with spaces and the remote shell re-splits, so no
# argument may contain spaces (play.sh comma-encodes RBR_SNAPSHOT for the same reason).
# $auto is word-split locally into its 4 or 5 numbers, arriving as $3 onwards.
lockf -t 7200 /tmp/rbr-harness.lock ssh -o BatchMode=yes cachy bash -s -- "$game_dir" "$prefix" $auto <<'EOF2'
set -euo pipefail
cd ~/Games
game_dest=${1##*/} prefix_dest=${2##*/}
rsync -a --delete --exclude 'RBRRecompRecord-*.bin' --exclude 'RBRRecomp*.log' rbr-vanilla-1.02/ "$game_dest"/
rsync -a --delete prefixes/rbr-vanilla/ prefixes/"$prefix_dest"/
cd ~/"$1"
rm -f RBRRecompPlayback.bin RBRRecompTransplant.bin RBRRecompRecord-*.bin RBRRecompHarness.log \
  RBRRecompCalls.log RBRRecompShadow.log RBRRecompShadow.on RBRRecompCount.on RBRRecompTrace.on \
  RBRRecompTrace-*.bin RBRRecompPeek.txt RBRRecompPeek-*.bin RBRRecompEarlyStop.txt \
  RBRRecompSnapshot.txt RBRRecompSnap-*.bin
touch RBRRecompRecord.on
echo "${*:3}" > RBRRecompAuto.txt
ls -la RBRRecompRecord.on RBRRecompAuto.txt
cat RBRRecompAuto.txt
EOF2
if [ "${RBR_REC_MENUS:-}" = 1 ]; then
  ssh -o BatchMode=yes cachy bash -s -- "$game_dir" <<'EOF'
rm -f ~/"$1"/RBRRecompAuto.txt && echo "no auto-start: pick the stage in the menus"
EOF
fi

# Every recording installs its plugin (no recording inherits another run's build):
# RBR_HARNESS_PLUGIN, or the current one in ~/rbr-harness-plugin on cachy.
ssh -o BatchMode=yes cachy bash -s -- "${RBR_HARNESS_PLUGIN:-rbr-harness-plugin/RBRRecompHarness.dll}" "$game_dir" <<'EOF'
cp "$1" "$2/Plugins/RBRRecompHarness.dll"
EOF

# Leftover processes of the rec prefix (play.sh's kill, for this prefix only).
ssh -o BatchMode=yes cachy bash -s -- "$prefix" <<'EOF2'
want=WINEPREFIX=$HOME/$1/pfx/
pids() {
  for d in /proc/[0-9]*; do
    cat "$d"/task/*/environ 2>/dev/null | tr '\0' '\n' | grep -qxF "$want" || continue
    cat "$d"/task/*/cmdline 2>/dev/null | tr '\0' ' ' | grep -q RSF_Launcher && continue
    echo "${d#/proc/}"
  done
}
p=$(pids)
[ -z "$p" ] && exit 0
echo "killing leftover processes of $1:" $p >&2
kill $p 2>/dev/null || true
sleep 3
p=$(pids)
[ -z "$p" ] || kill -9 $p 2>/dev/null || true
EOF2

echo "starting the game on cachy: drive the stage to the end or crash, then quit to desktop"
ssh -o BatchMode=yes cachy bash -s -- "$name" "$game_dir" "$prefix" $auto <<'EOF2'
set -euo pipefail
cd ~/"$2"
# as ~/.local/bin/rbr-vanilla, with the rec game folder and prefix; no timeout kill
(export WINEPREFIX=$HOME/$3 PROTONPATH=/usr/share/steam/compatibilitytools.d/proton-cachyos-slr \
   GAMEID=umu-${3##*/} WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-vanilla-run-rec.log 2>&1 &)
game="[${2:0:1}]${2:1}/RichardBurnsRally_SSE.exe"
timeout 300 bash -c "until pgrep -f '$game' >/dev/null; do sleep 2; done" || { echo "the game did not start" >&2; exit 1; }
echo "game running; waiting until it quits"
while pgrep -f "$game" >/dev/null; do sleep 5; done
rec=$(ls -t RBRRecompRecord-*.bin 2>/dev/null | head -1 || true)
[ -n "$rec" ] || { echo "no recording" >&2; exit 1; }
mv "$rec" ~/rbr-harness-runs/drive-"$1".bin
echo "${*:4}" > ~/rbr-harness-runs/drive-"$1".auto
cat RBRRecompHarness.log 2>/dev/null | grep -E "auto:|stage started|playback|recorder" || true
ls -la ~/rbr-harness-runs/drive-"$1".bin ~/rbr-harness-runs/drive-"$1".auto
EOF2
