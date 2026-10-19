#!/bin/bash
# One harness recording in the NGP test install on cachy (~/Games/rbr-ngp-7.4.778, notes/ngp.md): auto-start Tanner
# with the slot-6 car and record. Without PLAYBACK nobody drives: the game runs SECS seconds and is killed. With
# PLAYBACK (a local RBRRecompPlayback.bin, e.g. from scripts/ngp-synth-playback.py) or PLAYBACK_REMOTE (one on cachy,
# relative to its home: harness/matrix.py --ngp) its controls drive the car and the game quits by itself after the last
# one (killed after SECS if it hasn't). With DUMP=1 the exe and NGP.dll images are
# dumped (scripts/ngp-dump.py) to ~/rbr-ngp-runs/NAME.dump/ after SECS (idle runs only). SNAPSHOT="1520 2300" dumps
# the process at those steps (RBRRecompSnapshot.txt, as harness/play.sh's RBR_SNAPSHOT) to NAME.snap-<step>[-after].bin.
# PEEK=<local probe file> reads pointer probes after every step (RBRRecompPeek.txt, harness/README.md) to NAME.peek.bin.
# The recording goes to cachy:~/rbr-ngp-runs/NAME.bin (outside ~/rbr-harness-runs, which play.sh prunes).
# AUTO="map car weather tyre" picks another stage start (default "54 6 0 4": Tanner, slot-6 car). REPLAY=<name> plays
# Replays/<name> on the test install instead (RBRRecompReplay.txt); the game is killed after SECS. LIVE=1: a person
# drives: no auto-start (pick the stage in the menus, which loads the profile's controller bindings), the recording runs
# until the game is quit (or SECS, default 3600).
# Usage: [AUTO=...|REPLAY=name|LIVE=1] [PLAYBACK=file|PLAYBACK_REMOTE=path] [DUMP=1] [SNAPSHOT=steps] [PEEK=file] scripts/ngp-run.sh NAME [SECS]
set -euo pipefail
name=$1
live=${LIVE:-0}
secs=75; [ -n "${PLAYBACK:-}${PLAYBACK_REMOTE:-}" ] && secs=600; [ "$live" = 1 ] && secs=3600
secs=${2:-$secs}
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad run name" >&2; exit 1; }
scp -q "$(dirname "$0")/ngp-dump.py" cachy:/tmp/ngp-dump.py
snap=${SNAPSHOT:-}
[[ "$snap" =~ ^[0-9\ ]*$ ]] || { echo "bad SNAPSHOT" >&2; exit 1; }
snaps=$(echo $snap | tr ' ' ,); snaps=${snaps:--}
peek=0 auto=${AUTO:-54 6 0 4}
[[ "$auto" =~ ^[0-9]+(\ [0-9]+){3,5}$ ]] || { echo "bad AUTO" >&2; exit 1; }
replay=${REPLAY:-}
[[ "$replay" =~ ^[A-Za-z0-9_.-]*$ ]] || { echo "bad REPLAY" >&2; exit 1; }
# Inputs are staged under the run's name and go into the install only once the run holds the lock.
play=-
ssh -o BatchMode=yes cachy mkdir -p rbr-ngp-runs
if [ -n "${PEEK:-}" ]; then scp -q "$PEEK" "cachy:rbr-ngp-runs/.stage-$name.peek"; peek=1; fi
if [ -n "${PLAYBACK:-}" ]; then scp -q "$PLAYBACK" "cachy:rbr-ngp-runs/.stage-$name.playback"; play=rbr-ngp-runs/.stage-$name.playback; fi
if [ -n "${PLAYBACK_REMOTE:-}" ]; then
  [[ "$PLAYBACK_REMOTE" =~ ^[A-Za-z0-9_./-]+$ ]] || { echo "bad PLAYBACK_REMOTE" >&2; exit 1; }
  play=$PLAYBACK_REMOTE
fi
ssh -o BatchMode=yes cachy bash -s -- "$name" "$secs" "${DUMP:-0}" "$play" "$peek" "${auto// /,}" "$snaps" "${replay:--}" "$live" <<'EOF'
set -euo pipefail
# one of our runs at a time: the install's harness files are shared
exec 9>/tmp/rbr-ngp-run.lock
flock -w 7200 9 || { echo "another ngp-run.sh holds the lock" >&2; exit 1; }
trap 'rm -f ~/rbr-ngp-runs/.stage-$1.peek ~/rbr-ngp-runs/.stage-$1.playback' EXIT
cd ~/Games/rbr-ngp-7.4.778
mkdir -p ~/rbr-ngp-runs
test ! -e ~/rbr-ngp-runs/$1.bin || { echo "~/rbr-ngp-runs/$1.bin exists" >&2; exit 1; }
# every process of the NGP prefix: the game (umu sets .../pfx/) and the launcher wrappers (the prefix as exported)
pids() {
  for d in /proc/[0-9]*; do
    cat "$d"/task/*/environ 2>/dev/null | tr '\0' '\n' |
      grep -qxF -e "WINEPREFIX=$HOME/Games/prefixes/rbr-ngp/pfx/" -e "WINEPREFIX=$HOME/Games/prefixes/rbr-ngp" || continue
    echo "${d#/proc/}"
  done
}
# The prefix is also the user's NGP install: start only once it is empty, and stop only our own processes (the ones
# with this run's token in their environment; umu's container starts sessions of its own).
token=RBR_NGP_RUN=$1.$$
idle() { for i in $(seq 30); do [ -z "$(pids)" ] && return 0; sleep 2; done; return 1; }
ours() { for p in $(pids); do cat /proc/$p/task/*/environ 2>/dev/null | tr '\0' '\n' | grep -qxF "$token" && echo "$p"; done; }
stop() { p=$(ours); [ -z "$p" ] && return; kill $p 2>/dev/null || true; sleep 3; p=$(ours); [ -z "$p" ] || kill -9 $p 2>/dev/null || true; sleep 1; }
idle || { echo "the NGP prefix has processes (the user's game?); not starting" >&2; exit 1; }
rm -f RBRRecompRecord-*.bin RBRRecompSnapshot.txt RBRRecompSnap-*.bin RBRRecompPeek-*.bin
[ "$5" = 1 ] || rm -f RBRRecompPeek.txt
[ "$7" = - ] || echo "${7//,/ }" > RBRRecompSnapshot.txt  # ssh drops empty arguments: - is none
rm -f RBRRecompPlayback.bin
[ "$4" = - ] || cp ~/"$4" RBRRecompPlayback.bin
[ "$5" = 1 ] && mv ~/rbr-ngp-runs/.stage-$1.peek RBRRecompPeek.txt
rm -f RBRRecompAuto.txt RBRRecompReplay.txt
if [ "$8" != - ]; then echo "$8" > RBRRecompReplay.txt; elif [ "$9" != 1 ]; then echo "${6//,/ }" > RBRRecompAuto.txt; fi
touch RBRRecompRecord.on
# as ~/.local/bin/rbr-ngp, with the display of cachy's session
(export "$token" WINEPREFIX=$HOME/Games/prefixes/rbr-ngp PROTONPATH=/usr/share/steam/compatibilitytools.d/proton-cachyos-slr \
   GAMEID=umu-rbr-ngp WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-ngp-$1.log 2>&1 &)
timeout 300 bash -c "until pgrep -f '[r]br-ngp-7.4.778/RichardBurnsRally_SSE.exe' >/dev/null; do sleep 2; done"
if [ "$4" != - ] || [ "$9" = 1 ]; then
  timeout "$2" bash -c "while pgrep -f '[r]br-ngp-7.4.778/RichardBurnsRally_SSE.exe' >/dev/null; do sleep 5; done" || echo "the game did not quit in $2 s; killed" >&2
else
  sleep "$2"
  if [ "$3" = 1 ]; then python3 -I /tmp/ngp-dump.py ~/rbr-ngp-runs/$1.dump; fi
fi
stop
idle || echo "NGP prefix processes without our token remain (not ours; left alone)" >&2
rm -f RBRRecompAuto.txt RBRRecompReplay.txt RBRRecompRecord.on RBRRecompPlayback.bin RBRRecompSnapshot.txt RBRRecompPeek.txt
for f in RBRRecompPeek-*.bin; do [ -e "$f" ] && mv "$f" ~/rbr-ngp-runs/$1.peek.bin; done
for f in RBRRecompSnap-*.bin; do [ -e "$f" ] || continue; s=${f#RBRRecompSnap-}; mv "$f" ~/rbr-ngp-runs/$1.snap-${s%.bin}.bin; done
mv RBRRecompRecord-*.bin ~/rbr-ngp-runs/$1.bin
cp RBRRecompHarness.log ~/rbr-ngp-runs/$1.harness.log
ls -la ~/rbr-ngp-runs/$1.bin
EOF
