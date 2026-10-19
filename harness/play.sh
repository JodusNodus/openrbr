#!/bin/bash
# Unattended playback run on a vanilla test install on cachy: slot 0 is ~/Games/rbr-vanilla-1.02
# (prefix ~/Games/prefixes/rbr-vanilla), slot N > 0 is ~/Games/rbr-vanilla-1.02-sN (prefix
# ~/Games/prefixes/rbr-vanilla-sN; made by harness/slots.sh).
# Usage: [RBR_SHADOW=1 | RBR_COUNT=1 | RBR_TRACE=N] [RBR_DRIVE=<name>] [RBR_WRITEWATCH=1] harness/play.sh <name> [transplant.bin]
#   Plays the drive's controls back (RBR_DRIVE, default tanner-corolla: the plugin starts
#   the stage from the drive's auto-start file and quits when the controls run out),
#   with our functions from transplant.bin if
#   given, and keeps the recording as ~/rbr-harness-runs/<name>.bin on cachy, with the
#   plugin's logs next to it (<name>.harness.log, .calls.log, .shadow.log).
#   The drive's files live in ~/rbr-harness-runs (harness/drive-setup.sh):
#   drive-<drive>.playback.bin and drive-<drive>.auto are copied into the slot's
#   RBRRecompPlayback.bin / RBRRecompAuto.txt when they differ (tanner-corolla's are
#   today's files); a plain transplant run early-stops against ref-<drive>-1.bin
#   (tanner-corolla: ref1.bin, with its .mask).
#   Runs take the first free slot (RBR_SLOTS, default 4; one lock per slot on this machine)
#   and wait while all are busy, so up to RBR_SLOTS games run at once.
#   RBR_SHADOW=1 makes it a shadow run (RBRRecompShadow.on); every other run is a plain
#   transplant run, whatever an earlier run left in the game folder. RBR_COUNT=1 makes it a
#   count run (RBRRecompCount.on): every function with a trampoline in transplant.bin
#   (`transplant.py --count`) is counted and runs the original; ours never runs.
#   RBR_TRACE=N makes it a capture run (RBRRecompTrace.on holds N): the eligible functions in
#   transplant.bin run the original only, and their first N calls are logged to
#   ~/rbr-harness-runs/<name>.trace.bin (see harness/README.md at git 9c28c0b8). Takes precedence over RBR_SHADOW.
#   A plain transplant run stops at its first divergence from the drive's reference
#   (RBR_DRIVE=tanner-corolla: ~/rbr-harness-runs/ref1.bin; other drives:
#   ~/rbr-harness-runs/ref-<drive>-1.bin)
#   (RBRRecompEarlyStop.txt; needs ref1.bin.mask from `diverge.py --mask ref1.bin ref2.bin`),
#   so a failing run ends early and diverge.py reports the same step; RBR_EARLYSTOP=0 runs
#   to the end anyway, RBR_EARLYSTOP=<path on cachy> compares with another reference.
#   Capture runs never early-stop.
#   Each run installs its plugin into the slot first: RBR_HARNESS_PLUGIN=<path on cachy>, or
#   ~/rbr-harness-plugin/RBRRecompHarness.dll (harness/plugin/build.sh --install).
#   RBR_PEEK=<local file> copies a probe list into the slot and saves the plugin's
#   RBRRecompPeek-<tick>.bin as ~/rbr-harness-runs/<name>.peek.bin.
#   RBR_SNAPSHOT="40 572" lists steps to snapshot (RBRRecompSnapshot.txt): before the step call
#   starting at each listed step and the call after it the plugin dumps the process
#   (harness/replay.py), and again right after a listed step's call; kept as
#   ~/rbr-harness-runs/<name>.snap-<step>.bin and <name>.snap-<step>-after.bin.
#   RBR_SNAPAT=<local file> copies a load-time dump list (RBRRecompSnapAt.txt, see the plugin): the
#   listed transplanted functions are dumped at entry and on return, kept in
#   ~/rbr-harness-runs/<name>.snapat/ (RBRRecompSnapAt-<addr>-<k>[-after].bin).
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
game_dir=Games/rbr-vanilla-1.02$sfx prefix=Games/prefixes/rbr-vanilla$sfx
name=$1 transplant=${2:-}
drive=${RBR_DRIVE:-tanner-corolla}
[[ "$drive" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad RBR_DRIVE" >&2; exit 1; }
# tanner-corolla keeps today's ref1.bin/ref1.bin.mask; other drives use their own refs.
peek=${RBR_PEEK:-}
snapat=${RBR_SNAPAT:-}
[ -z "$snapat" ] || [ -f "$snapat" ] || { echo "RBR_SNAPAT file not found: $snapat" >&2; exit 1; }
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
# for it, read through any live thread; never the user's own prefix or RSF_Launcher.
kill_vanilla() {
ssh -o BatchMode=yes cachy bash -s -- "$prefix" <<'EOF'
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
early=${RBR_EARLYSTOP:-rbr-harness-runs/ref1.bin}
[ "$drive" = tanner-corolla ] || early=${RBR_EARLYSTOP:-rbr-harness-runs/ref-$drive-1.bin}
[ -n "$transplant" ] && [ "${RBR_SHADOW:-0}" = 0 ] && [ "${RBR_COUNT:-0}" = 0 ] && [ "${RBR_TRACE:-0}" = 0 ] || early=0
ssh -o BatchMode=yes cachy bash -s -- "${RBR_SHADOW:-0}" "${RBR_COUNT:-0}" "$game_dir" "$early" "${RBR_TRACE:-0}" \
  "$drive" "${RBR_ALLOCLOG:-0}" "$snaparg" "$writewatch" <<'EOF'
set -euo pipefail
mkdir -p ~/rbr-harness-runs && cd ~/"$3"
drive=${6:-tanner-corolla}
pb=~/rbr-harness-runs/drive-$drive.playback.bin; au=~/rbr-harness-runs/drive-$drive.auto
[ -f "$pb" ] && [ -f "$au" ] || { echo "no drive-$drive.playback.bin/auto in ~/rbr-harness-runs (harness/drive-setup.sh)" >&2; exit 1; }
cmp -s "$pb" RBRRecompPlayback.bin || cp "$pb" RBRRecompPlayback.bin
cmp -s "$au" RBRRecompAuto.txt || cp "$au" RBRRecompAuto.txt
# Recordings are ~195 MB per full run: drop those older than 6 h (user, 2026-10-14). The
# references (ref*, drive-*: the recorded drives), step-replay snapshots and every log stay.
if [ "$9" != 1 ]; then
  find ~/rbr-harness-runs -maxdepth 1 -type f -name '*.bin' -mmin +360 ! -name 'ref*' ! -name 'drive-*' \
    ! -name 'y24-drive-meta.bin' ! -name '*.snap-*' -delete
fi
test -f RBRRecompPlayback.bin && test -f RBRRecompAuto.txt
! pgrep -f "[${3:0:1}]${3:1}/RichardBurnsRally_SSE.exe" >/dev/null
rm -f RBRRecompTransplant.bin RBRRecompRecord-*.bin RBRRecompHarness.log RBRRecompCalls.log RBRRecompShadow.log RBRRecompShadow.on \
  RBRRecompCount.on RBRRecompTrace.on RBRRecompTrace-*.bin RBRRecompPeek.txt RBRRecompPeek-*.bin RBRRecompEarlyStop.txt \
  RBRRecompSnapshot.txt RBRRecompSnap-*.bin RBRRecompSnapAt.txt RBRRecompSnapAt-*.bin RBRRecompAllocLog.on \
  RBRRecompAllocs.bin RBRRecompWriteWatch.on RBRRecompFastForward.txt RBRRecompCamera.txt RBRRecompVolumes.txt \
  RBRRecompPops.txt RBRRecompFps.txt
touch RBRRecompRecord.on
if [ "$7" = 1 ]; then touch RBRRecompAllocLog.on; fi
if [ "$9" = 1 ]; then touch RBRRecompWriteWatch.on; fi
snap=${8:-}
[ "$snap" = - ] && snap=
if [ -n "$snap" ]; then echo "${snap//,/ }" > RBRRecompSnapshot.txt; fi
if [ "$1" = 1 ]; then touch RBRRecompShadow.on; fi
if [ "$2" = 1 ]; then touch RBRRecompCount.on; fi
if [ "$4" != 0 ]; then
  ref=$4; [ "${ref#/}" != "$ref" ] || ref=$HOME/$ref
  [ -f "$ref" ] && [ -f "$ref.mask" ] || { echo "early stop: no $ref or $ref.mask (diverge.py --mask)" >&2; exit 1; }
  echo "Z:${ref//\//\\}" > RBRRecompEarlyStop.txt
fi
if [ "$5" != 0 ]; then echo "$5" > RBRRecompTrace.on; fi
EOF
if [ -n "$peek" ]; then scp -q "$peek" "cachy:$game_dir/RBRRecompPeek.txt"; fi
if [ -n "${RBR_FASTFORWARD:-}" ]; then
  [[ "$RBR_FASTFORWARD" =~ ^[0-9]+$ ]] || { echo "bad RBR_FASTFORWARD" >&2; exit 1; }
  echo "$RBR_FASTFORWARD" | ssh -o BatchMode=yes cachy "cat > $game_dir/RBRRecompFastForward.txt"
fi
if [ -n "${RBR_CAMERA:-}" ]; then
  [[ "$RBR_CAMERA" =~ ^[1345]$ ]] || { echo "bad RBR_CAMERA" >&2; exit 1; }
  echo "$RBR_CAMERA" | ssh -o BatchMode=yes cachy "cat > $game_dir/RBRRecompCamera.txt"
fi
if [ -n "${RBR_VOLUMES:-}" ]; then
  [[ "$RBR_VOLUMES" =~ ^([0-9.]+|-)( ([0-9.]+|-)){5}$ ]] || { echo "bad RBR_VOLUMES" >&2; exit 1; }
  echo "$RBR_VOLUMES" | ssh -o BatchMode=yes cachy "cat > $game_dir/RBRRecompVolumes.txt"
fi
if [ -n "${RBR_FPS:-}" ]; then
  [[ "$RBR_FPS" =~ ^[0-9]+$ ]] || { echo "bad RBR_FPS" >&2; exit 1; }
  echo "$RBR_FPS" | ssh -o BatchMode=yes cachy "cat > $game_dir/RBRRecompFps.txt"
fi
if [ "${RBR_POPS:-}" = 1 ]; then ssh -o BatchMode=yes cachy "touch $game_dir/RBRRecompPops.txt"; fi
if [ -n "$snapat" ]; then scp -q "$snapat" "cachy:$game_dir/RBRRecompSnapAt.txt"; fi
if [ -n "$watchlist" ]; then scp -q "$watchlist" "cachy:$game_dir/RBRRecompWriteWatch.on"; fi
# Every run installs its plugin (no run inherits another run's build): RBR_HARNESS_PLUGIN, or
# the current one in ~/rbr-harness-plugin (harness/plugin/build.sh --install).
ssh -o BatchMode=yes cachy cp "${RBR_HARNESS_PLUGIN:-rbr-harness-plugin/RBRRecompHarness.dll}" "$game_dir/Plugins/RBRRecompHarness.dll"
if [ -n "$transplant" ]; then scp -q "$transplant" "cachy:$game_dir/RBRRecompTransplant.bin"; fi
ssh -o BatchMode=yes cachy bash -s -- "$name" "${RBR_TIMEOUT:-600}" "$game_dir" "$prefix" <<'EOF'
set -euo pipefail
cd ~/"$3"
# as ~/.local/bin/rbr-vanilla, with this slot's game folder and prefix
(export WINEPREFIX=$HOME/$4 PROTONPATH=/usr/share/steam/compatibilitytools.d/proton-cachyos-slr \
   GAMEID=umu-${4##*/} WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-vanilla-run-${4##*/}.log 2>&1 &)
game="[${3:0:1}]${3:1}/RichardBurnsRally_SSE.exe"
# the launcher may update its runtime first
timeout 300 bash -c "until pgrep -f '$game' >/dev/null; do sleep 2; done" || echo "the game did not start" >&2
timeout "$2" bash -c "while pgrep -f '$game' >/dev/null; do sleep 5; done" || echo "the game did not quit in $2 s (hung); killed" >&2
EOF
kill_vanilla
ssh -o BatchMode=yes cachy bash -s -- "$name" "$game_dir" "$([ -n "$peek" ] && echo 1 || echo 0)" "${RBR_TRACE:-0}" <<'EOF'
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
if [ "$4" != 0 ]; then
  trace=$(ls -t RBRRecompTrace-*.bin 2>/dev/null | head -1)
  [ -n "$trace" ] || { echo "no trace output" >&2; exit 1; }
  mv "$trace" ~/rbr-harness-runs/"$1".trace.bin
fi
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
if ls RBRRecompSnapAt-*.bin >/dev/null 2>&1 || [ -f RBRRecompSnapAt.txt ]; then
  mkdir -p ~/rbr-harness-runs/"$1".snapat
  for f in RBRRecompSnapAt-*.bin; do
    [ -e "$f" ] || continue
    mv "$f" ~/rbr-harness-runs/"$1".snapat/"$f"
  done
  rm -f RBRRecompSnapAt.txt
  ls ~/rbr-harness-runs/"$1".snapat | wc -l | sed 's/^/snapat dumps: /'
fi
echo "slot: $2"
ls -la ~/rbr-harness-runs/"$1".bin
if [ "$3" = 1 ]; then ls -la ~/rbr-harness-runs/"$1".peek.bin; fi
EOF
