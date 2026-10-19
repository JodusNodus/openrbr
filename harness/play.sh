#!/bin/bash
# Unattended playback run on the vanilla 1.02 test install on the test machine (RBR_REMOTE_HOST): slot 0 is
# ~/$RBR_REMOTE_GAME (Wine prefix ~/$RBR_REMOTE_PREFIX, Proton $RBR_REMOTE_PROTON), slot N > 0 is
# ~/$RBR_REMOTE_GAME-sN (prefix ~/$RBR_REMOTE_PREFIX-sN; made by harness/slots.sh). Paths relative to the remote home.
# Usage: [RBR_DRIVE=<name>] [RBR_WRITEWATCH=1] harness/play.sh <name>
#   Plays the drive's controls back (RBR_DRIVE, default tanner-corolla: the plugin starts
#   the stage from the drive's auto-start file and quits when the controls run out),
#   and keeps the recording as ~/rbr-harness-runs/<name>.bin on the test machine, with the
#   plugin's log next to it (<name>.harness.log).
#   The drive's files live in ~/rbr-harness-runs (harness/record.sh, harness/v1-drive.sh):
#   drive-<drive>.playback.bin and drive-<drive>.auto are copied into the slot's
#   RBRRecompPlayback.bin / RBRRecompAuto.txt when they differ.
#   The plugin's transplant, shadow, count and capture runs and load-time dumps take a
#   transplant file built from the C++ reconstruction: run them from an rbr-recomp checkout.
#   Runs take the first free slot (RBR_SLOTS, default 4; one lock per slot on this machine)
#   and wait while all are busy, so up to RBR_SLOTS games run at once.
#   Each run installs its plugin into the slot first: RBR_HARNESS_PLUGIN=<path on the test machine>, or
#   ~/rbr-harness-plugin/RBRRecompHarness.dll (harness/plugin/build.sh --install).
#   RBR_PEEK=<local file> copies a probe list into the slot and saves the plugin's
#   RBRRecompPeek-<tick>.bin as ~/rbr-harness-runs/<name>.peek.bin.
#   RBR_SNAPSHOT="40 572" lists steps to snapshot (RBRRecompSnapshot.txt): before the step call
#   starting at each listed step and the call after it the plugin dumps the process
#   (harness/replay.py), and again right after a listed step's call; kept as
#   ~/rbr-harness-runs/<name>.snap-<step>.bin and <name>.snap-<step>-after.bin.
#   RBR_WRITEWATCH=1 arms one-word write watches for 0x01660b74/0x01660b80 and logs the marker-tick reads through frame 5000.
#   RBR_WATCHLIST=<local file> (with RBR_WRITEWATCH=1) replaces those watches: the file lists up to 4 hex word addresses
#   (one per line) and optionally "steps N" to quit at step N; each hit logs whether it came inside StepForward.
#   RBR_FASTFORWARD=N plays N physics steps per step the frame asks for (RBRRecompFastForward.txt; default 8,
#   1 steps at the game's own pace as a live drive does).
#   RBR_CAMERA=N switches the camera to mode N in the race (RBRRecompCamera.txt: 1 chase, 3 Cam_bonnet, 4 Cam_bonnet2,
#   the game's default, 5 Cam_internal; rbr-recomp notes/cameras.md 2a), as the CH_CAM key would.
#   RBR_VOLUMES="e c f m t r" writes the volume options (engine, co-driver, effects, music, tyres, replay music; a
#   number or - to keep the game's) every race frame (RBRRecompVolumes.txt), to hear the original one group at a time.
#   RBR_POPS=1 logs the exhaust pops' misfire values and the 16-sound limit: the count, and each caller's made and
#   refused sounds (RBRRecompPops.txt; rbr-recomp notes/sound.md 2.5).
#   RBR_FPS=N caps the game at N frames a second (RBRRecompFps.txt), as the retrace wait does on a display.
#   RBR_ALLOCLOG=1 logs the exe's heap calls (RBRRecompAllocLog.on, see the plugin), kept as
#   ~/rbr-harness-runs/<name>.allocs.bin (harness/allocs.py reads it).
#   A game still running after RBR_TIMEOUT seconds (default 600) counts as hung and is killed.
set -euo pipefail
host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
game=${RBR_REMOTE_GAME:?set RBR_REMOTE_GAME to the vanilla 1.02 test install on the test machine (notes/verification.md)}
wineprefix=${RBR_REMOTE_PREFIX:?set RBR_REMOTE_PREFIX to the Wine prefix of the test install (notes/verification.md)}
proton=${RBR_REMOTE_PROTON:?set RBR_REMOTE_PROTON to the Proton build umu-run uses on the test machine (notes/verification.md)}
if [ -n "${RBR_PEEK:-}" ] && [ ! -f "$RBR_PEEK" ]; then
  echo "RBR_PEEK file not found: $RBR_PEEK" >&2; exit 1
fi
if [ -z "${RBR_SLOT:-}" ]; then
  # lockf -t 0 exits 75 when the slot is busy; anything else is the run's own status.
  until [ $SECONDS -gt 7200 ]; do
    for ((i = 0; i < ${RBR_SLOTS:-4}; i++)); do
      lock=/tmp/rbr-harness.lock; [ $i = 0 ] || lock=/tmp/rbr-harness-s$i.lock
      rc=0; RBR_SLOT=$i lockf -t 0 "$lock" "$0" "$@" || rc=$?
      [ $rc = 75 ] || exit $rc
    done
    sleep 3
  done
  echo "no free harness slot in 2 h" >&2; exit 1
fi
sfx=; [ "$RBR_SLOT" = 0 ] || sfx=-s$RBR_SLOT
game_dir=$game$sfx prefix=$wineprefix$sfx
name=$1
drive=${RBR_DRIVE:-tanner-corolla}
[[ "$drive" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad RBR_DRIVE" >&2; exit 1; }
peek=${RBR_PEEK:-}
snap=${RBR_SNAPSHOT:-}
writewatch=${RBR_WRITEWATCH:-0}
snaparg=${snap// /,}
[ -n "$snaparg" ] || snaparg=-
[[ "$snap" =~ ^[0-9\ ]*$ ]] || { echo "bad RBR_SNAPSHOT" >&2; exit 1; }
[[ "$writewatch" = 0 || "$writewatch" = 1 ]] || { echo "bad RBR_WRITEWATCH" >&2; exit 1; }
watchlist=${RBR_WATCHLIST:-}
[ -z "$watchlist" ] || [ "$writewatch" = 1 ] || { echo "RBR_WATCHLIST needs RBR_WRITEWATCH=1" >&2; exit 1; }
[ -z "$watchlist" ] || [ -f "$watchlist" ] || { echo "RBR_WATCHLIST file not found: $watchlist" >&2; exit 1; }
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad run name" >&2; exit 1; }

# Kill what a hung run leaves behind: every process of the vanilla prefix (wineserver
# children, a zombie game whose threads live on). Matched by the WINEPREFIX Proton sets
# for it, read through any live thread; never another prefix's, nor RSF_Launcher.
kill_vanilla() {
ssh -o BatchMode=yes "$host" bash -s -- "$prefix" <<'EOF'
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
EOF
}

kill_vanilla
ssh -o BatchMode=yes "$host" bash -s -- "$game_dir" "$drive" "${RBR_ALLOCLOG:-0}" "$snaparg" "$writewatch" <<'EOF'
set -euo pipefail
mkdir -p ~/rbr-harness-runs && cd ~/"$1"
drive=${2:-tanner-corolla}
pb=~/rbr-harness-runs/drive-$drive.playback.bin; au=~/rbr-harness-runs/drive-$drive.auto
[ -f "$pb" ] && [ -f "$au" ] || { echo "no drive-$drive.playback.bin/auto in ~/rbr-harness-runs" >&2; exit 1; }
cmp -s "$pb" RBRRecompPlayback.bin || cp "$pb" RBRRecompPlayback.bin
cmp -s "$au" RBRRecompAuto.txt || cp "$au" RBRRecompAuto.txt
# Recordings are ~195 MB per full run: drop those older than 6 h (user, 2026-10-14). The
# references (ref*, drive-*: the recorded drives), step-replay snapshots and every log stay.
if [ "$5" != 1 ]; then
  find ~/rbr-harness-runs -maxdepth 1 -type f -name '*.bin' -mmin +360 ! -name 'ref*' ! -name 'drive-*' \
    ! -name 'y24-drive-meta.bin' ! -name '*.snap-*' -delete
fi
test -f RBRRecompPlayback.bin && test -f RBRRecompAuto.txt
! pgrep -f "[${1:0:1}]${1:1}/RichardBurnsRally_SSE.exe" >/dev/null
rm -f RBRRecompTransplant.bin RBRRecompRecord-*.bin RBRRecompHarness.log RBRRecompCalls.log RBRRecompShadow.log RBRRecompShadow.on \
  RBRRecompCount.on RBRRecompTrace.on RBRRecompTrace-*.bin RBRRecompPeek.txt RBRRecompPeek-*.bin RBRRecompEarlyStop.txt \
  RBRRecompSnapshot.txt RBRRecompSnap-*.bin RBRRecompSnapAt.txt RBRRecompSnapAt-*.bin RBRRecompAllocLog.on \
  RBRRecompAllocs.bin RBRRecompWriteWatch.on RBRRecompFastForward.txt RBRRecompCamera.txt RBRRecompVolumes.txt \
  RBRRecompPops.txt RBRRecompFps.txt
touch RBRRecompRecord.on
if [ "$3" = 1 ]; then touch RBRRecompAllocLog.on; fi
if [ "$5" = 1 ]; then touch RBRRecompWriteWatch.on; fi
snap=${4:-}
[ "$snap" = - ] && snap=
if [ -n "$snap" ]; then echo "${snap//,/ }" > RBRRecompSnapshot.txt; fi
EOF
if [ -n "$peek" ]; then scp -q "$peek" "$host:$game_dir/RBRRecompPeek.txt"; fi
if [ -n "${RBR_FASTFORWARD:-}" ]; then
  [[ "$RBR_FASTFORWARD" =~ ^[0-9]+$ ]] || { echo "bad RBR_FASTFORWARD" >&2; exit 1; }
  echo "$RBR_FASTFORWARD" | ssh -o BatchMode=yes "$host" "cat > $game_dir/RBRRecompFastForward.txt"
fi
if [ -n "${RBR_CAMERA:-}" ]; then
  [[ "$RBR_CAMERA" =~ ^[1345]$ ]] || { echo "bad RBR_CAMERA" >&2; exit 1; }
  echo "$RBR_CAMERA" | ssh -o BatchMode=yes "$host" "cat > $game_dir/RBRRecompCamera.txt"
fi
if [ -n "${RBR_VOLUMES:-}" ]; then
  [[ "$RBR_VOLUMES" =~ ^([0-9.]+|-)( ([0-9.]+|-)){5}$ ]] || { echo "bad RBR_VOLUMES" >&2; exit 1; }
  echo "$RBR_VOLUMES" | ssh -o BatchMode=yes "$host" "cat > $game_dir/RBRRecompVolumes.txt"
fi
if [ -n "${RBR_FPS:-}" ]; then
  [[ "$RBR_FPS" =~ ^[0-9]+$ ]] || { echo "bad RBR_FPS" >&2; exit 1; }
  echo "$RBR_FPS" | ssh -o BatchMode=yes "$host" "cat > $game_dir/RBRRecompFps.txt"
fi
if [ "${RBR_POPS:-}" = 1 ]; then ssh -o BatchMode=yes "$host" "touch $game_dir/RBRRecompPops.txt"; fi
if [ -n "$watchlist" ]; then scp -q "$watchlist" "$host:$game_dir/RBRRecompWriteWatch.on"; fi
# Every run installs its plugin (no run inherits another run's build): RBR_HARNESS_PLUGIN, or
# the current one in ~/rbr-harness-plugin (harness/plugin/build.sh --install).
ssh -o BatchMode=yes "$host" cp "${RBR_HARNESS_PLUGIN:-rbr-harness-plugin/RBRRecompHarness.dll}" "$game_dir/Plugins/RBRRecompHarness.dll"
ssh -o BatchMode=yes "$host" bash -s -- "$name" "${RBR_TIMEOUT:-600}" "$game_dir" "$prefix" "$proton" <<'EOF'
set -euo pipefail
cd ~/"$3"
# as ~/.local/bin/rbr-vanilla, with this slot's game folder and prefix
(export WINEPREFIX=$HOME/$4 PROTONPATH=$5 \
   GAMEID=umu-${4##*/} WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-vanilla-run-${4##*/}.log 2>&1 &)
game="[${3:0:1}]${3:1}/RichardBurnsRally_SSE.exe"
# the launcher may update its runtime first
timeout 300 bash -c "until pgrep -f '$game' >/dev/null; do sleep 2; done" || echo "the game did not start" >&2
timeout "$2" bash -c "while pgrep -f '$game' >/dev/null; do sleep 5; done" || echo "the game did not quit in $2 s (hung); killed" >&2
EOF
kill_vanilla
ssh -o BatchMode=yes "$host" bash -s -- "$name" "$game_dir" "$([ -n "$peek" ] && echo 1 || echo 0)" <<'EOF'
set -euo pipefail
cd ~/"$2"
cat RBRRecompHarness.log 2>/dev/null || true
for log in Harness Calls Shadow; do
  if [ -f RBRRecomp$log.log ]; then cp RBRRecomp$log.log ~/rbr-harness-runs/"$1".${log,,}.log; fi
done
rm -f RBRRecompTransplant.bin
rec=$(ls -t RBRRecompRecord-*.bin 2>/dev/null | head -1)
[ -n "$rec" ] || { echo "no recording" >&2; exit 1; }
mv "$rec" ~/rbr-harness-runs/"$1".bin
if [ "$3" = 1 ]; then
  peek=$(ls -t RBRRecompPeek-*.bin 2>/dev/null | head -1)
  [ -n "$peek" ] || { echo "no peek output" >&2; exit 1; }
  mv "$peek" ~/rbr-harness-runs/"$1".peek.bin
else
  rm -f ~/rbr-harness-runs/"$1".peek.bin
fi
rm -f RBRRecompPeek.txt RBRRecompPeek-*.bin RBRRecompFastForward.txt RBRRecompCamera.txt RBRRecompVolumes.txt \
  RBRRecompPops.txt RBRRecompFps.txt
rm -f RBRRecompWriteWatch.on
for f in RBRRecompSnap-*.bin; do
  [ -e "$f" ] || continue
  step=${f#RBRRecompSnap-}; mv "$f" ~/rbr-harness-runs/"$1".snap-${step%.bin}.bin
  ls -la ~/rbr-harness-runs/"$1".snap-${step%.bin}.bin
done
rm -f RBRRecompSnapshot.txt
if [ -f RBRRecompAllocs.bin ]; then
  mv RBRRecompAllocs.bin ~/rbr-harness-runs/"$1".allocs.bin
  ls -la ~/rbr-harness-runs/"$1".allocs.bin
fi
rm -f RBRRecompAllocLog.on
echo "slot: $2"
ls -la ~/rbr-harness-runs/"$1".bin
if [ "$3" = 1 ]; then ls -la ~/rbr-harness-runs/"$1".peek.bin; fi
EOF
