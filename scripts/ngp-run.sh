#!/bin/bash
# One harness recording in the NGP install ($RBR_NGP_GAME, its Wine prefix
# $RBR_NGP_PREFIX, Proton $RBR_PROTON; notes/ngp.md): auto-start Tanner
# with the slot-6 car and record. Without PLAYBACK nobody drives: the game runs SECS seconds and is killed. With
# PLAYBACK (a local RBRRecompPlayback.bin, e.g. from scripts/ngp-synth-playback.py) its controls drive the car
# and the game quits by itself after the last one (killed after SECS if it hasn't). With DUMP=1 the exe and NGP.dll
# images are dumped (scripts/ngp-dump.py) to $RBR_NGP_RUNS/NAME.dump/ after SECS (idle runs only).
# SNAPSHOT="1520 2300" dumps the process at those steps (RBRRecompSnapshot.txt, as harness/play.sh's RBR_SNAPSHOT)
# to NAME.snap-<step>[-after].bin. PEEK=<local probe file> reads pointer probes after every step
# (RBRRecompPeek.txt, notes/verification.md) to NAME.peek.bin.
# The recording goes to $RBR_NGP_RUNS/NAME.bin (outside $RBR_RUNS, which play.sh prunes).
# AUTO="map car weather tyre" picks another stage start (default "54 6 0 4": Tanner, slot-6 car). REPLAY=<name> plays
# Replays/<name> in the install instead (RBRRecompReplay.txt); the game is killed after SECS. LIVE=1: a person
# drives: no auto-start (pick the stage in the menus, which loads the profile's controller bindings), the recording runs
# until the game is quit (or SECS, default 3600).
# Usage: [AUTO=...|REPLAY=name|LIVE=1] [PLAYBACK=file] [DUMP=1] [SNAPSHOT=steps] [PEEK=file] scripts/ngp-run.sh NAME [SECS]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/dev-env.sh"
game=${RBR_NGP_GAME:?set RBR_NGP_GAME to the NGP install (notes/ngp.md)}
wineprefix=${RBR_NGP_PREFIX:?set RBR_NGP_PREFIX to the Wine prefix of the NGP install (notes/ngp.md)}
proton=${RBR_PROTON:-}
runs=${RBR_NGP_RUNS:?set RBR_NGP_RUNS to the NGP runs directory (notes/verification.md)}
name=$1
live=${LIVE:-0}
secs=75; [ -n "${PLAYBACK:-}" ] && secs=600; [ "$live" = 1 ] && secs=3600
secs=${2:-$secs}
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo "bad run name" >&2; exit 1; }
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
snap=${SNAPSHOT:-}
[[ "$snap" =~ ^[0-9\ ]*$ ]] || { echo "bad SNAPSHOT" >&2; exit 1; }
snaps=$(echo $snap | tr ' ' ,); snaps=${snaps:--}
peek=0 auto=${AUTO:-54 6 0 4}
[[ "$auto" =~ ^[0-9]+(\ [0-9]+){3,5}$ ]] || { echo "bad AUTO" >&2; exit 1; }
replay=${REPLAY:-}
[[ "$replay" =~ ^[A-Za-z0-9_.-]*$ ]] || { echo "bad REPLAY" >&2; exit 1; }
# Inputs are staged under the run's name and go into the install only once the run holds the lock.
mkdir -p "$runs"
if [ -n "${PEEK:-}" ]; then cp "$PEEK" "$runs/.stage-$name.peek"; peek=1; fi
if [ -n "${PLAYBACK:-}" ]; then cp "$PLAYBACK" "$runs/.stage-$name.playback"; fi
# one of our runs at a time: the install's harness files are shared
exec 9>/tmp/rbr-ngp-run.lock
flock -w 7200 9 || { echo "another ngp-run.sh holds the lock" >&2; exit 1; }
trap 'rm -f "$runs/.stage-$name.peek" "$runs/.stage-$name.playback"' EXIT
prefix=$wineprefix game_pat="[${game:0:1}]${game:1}/RichardBurnsRally_SSE.exe"
mkdir -p "$runs"
test ! -e "$runs/$name.bin" || { echo "$runs/$name.bin exists" >&2; exit 1; }
# every process of the NGP prefix: the game (umu sets .../pfx/) and the launcher wrappers (the prefix as exported)
pids() {
  local d
  for d in /proc/[0-9]*; do
    cat "$d"/task/*/environ 2>/dev/null | tr '\0' '\n' |
      grep -qxF -e "WINEPREFIX=$prefix/pfx/" -e "WINEPREFIX=$prefix" || continue
    echo "${d#/proc/}"
  done
}
# The prefix may also be a person's own NGP install: start only once it is empty, and stop only our own processes (the
# ones with this run's token in their environment; umu's container starts sessions of its own).
token=RBR_NGP_RUN=$name.$$
idle() { for i in $(seq 30); do [ -z "$(pids)" ] && return 0; sleep 2; done; return 1; }
ours() { for p in $(pids); do cat /proc/$p/task/*/environ 2>/dev/null | tr '\0' '\n' | grep -qxF "$token" && echo "$p"; done; }
stop() { p=$(ours); [ -z "$p" ] && return; kill $p 2>/dev/null || true; sleep 3; p=$(ours); [ -z "$p" ] || kill -9 $p 2>/dev/null || true; sleep 1; }
idle || { echo "the NGP prefix has processes (the user's game?); not starting" >&2; exit 1; }
cd "$game"
rm -f RBRRecompRecord-*.bin RBRRecompSnapshot.txt RBRRecompSnap-*.bin RBRRecompPeek-*.bin
[ "$peek" = 1 ] || rm -f RBRRecompPeek.txt
[ "$snaps" = - ] || echo "${snaps//,/ }" > RBRRecompSnapshot.txt
rm -f RBRRecompPlayback.bin
[ -z "${PLAYBACK:-}" ] || cp "$runs/.stage-$name.playback" RBRRecompPlayback.bin
[ "$peek" = 1 ] && mv "$runs/.stage-$name.peek" RBRRecompPeek.txt
rm -f RBRRecompAuto.txt RBRRecompReplay.txt
if [ "$replay" != - ] && [ -n "$replay" ]; then echo "$replay" > RBRRecompReplay.txt; elif [ "$live" != 1 ]; then echo "${auto//,/ }" > RBRRecompAuto.txt; fi
touch RBRRecompRecord.on
# with the display of this machine's session
(export "$token" WINEPREFIX=$prefix ${proton:+PROTONPATH=$proton} \
   GAMEID=umu-rbr-ngp WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u)
 setsid umu-run ./RichardBurnsRally_SSE.exe >/tmp/rbr-ngp-$name.log 2>&1 &)
timeout 300 bash -c "until pgrep -f '$game_pat' >/dev/null; do sleep 2; done"
if [ -n "${PLAYBACK:-}" ] || [ "$live" = 1 ]; then
  timeout "$secs" bash -c "while pgrep -f '$game_pat' >/dev/null; do sleep 5; done" || echo "the game did not quit in $secs s; killed" >&2
else
  sleep "$secs"
  if [ "${DUMP:-0}" = 1 ]; then python3 -I "$repo/scripts/ngp-dump.py" "$runs/$name.dump" "$wineprefix"; fi
fi
stop
idle || echo "NGP prefix processes without our token remain (not ours; left alone)" >&2
rm -f RBRRecompAuto.txt RBRRecompReplay.txt RBRRecompRecord.on RBRRecompPlayback.bin RBRRecompSnapshot.txt RBRRecompPeek.txt
for f in RBRRecompPeek-*.bin; do [ -e "$f" ] && mv "$f" "$runs/$name.peek.bin"; done
for f in RBRRecompSnap-*.bin; do [ -e "$f" ] || continue; s=${f#RBRRecompSnap-}; mv "$f" "$runs/$name.snap-${s%.bin}.bin"; done
mv RBRRecompRecord-*.bin "$runs/$name.bin"
cp RBRRecompHarness.log "$runs/$name.harness.log"
ls -la "$runs/$name.bin"
