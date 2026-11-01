#!/bin/bash
# Record RallySimFans drives (notes/verification.md "RSF sessions"): per stage attempt the harness
# plugin's FRM1 recording and RSS1 session file (RBRRecompSession.on; harness/session.py), the game's audio for the
# whole session, and the settings that shape input and force feedback.
#
#   scripts/rsf-session.sh install          build the plugin into $RBR_RSF_PLUGIN (with harness/session.py)
#   scripts/rsf-session.sh start NAME       take the lock, put the plugin and the .on files into the install (a
#                                           manifest lists them), meta.txt "before", then record the audio once the
#                                           game runs. Start RSF as usual (`rbr` or `rbr game`), drive any number of
#                                           stages and quit the game: that ends the session.
#   scripts/rsf-session.sh stop NAME        end it while the game runs no more (--force: while it runs); a session
#                                           whose watcher died is ended from its manifest
#   scripts/rsf-session.sh status           the session holding the lock
#   scripts/rsf-session.sh fetch NAME [DIR] copy $RBR_RSF_RUNS/NAME to DIR (default $RBR_RECORDINGS/rsf/NAME)
#
# The end of a session moves stage attempt n's files to $RBR_RSF_RUNS/NAME/<n>/: physics.bin, session.bin,
# attempt.txt, files/ (the .lsp files its stage load opened) and setup.lsp (the setup its car started on); writes
# meta.txt's "after" values and harness.log, and removes exactly what the manifest added. config/ holds the controller
# profiles (.acm, .rbr), RBRControls' files, NGPCarMenu.ini and RSF's cars_data.json as the session started, devices.txt the device files the game's prefix holds.
# Env: RBR_RSF (the RSF install), RBR_RSF_PREFIX (the Wine prefix the game runs in),
# RSF_LOCK (/tmp/rbr-rsf-run.lock). For tests
# without a driver: AUTO="map car weather tyre" adds RBRRecompAuto.txt (a stage start), PLAYBACK=<file relative to
# the home> RBRRecompPlayback.bin (its controls drive; the game quits after them), FAST=N RBRRecompFastForward.txt,
# FFB=1 RBRRecompFfb.on (binds the newest SavedGames/pf*.acm, as the menus would, so force feedback reaches the
# wheel in an AUTO start, playback too; FFB=<name> binds pf<name>.acm), SNAPS=1 RBRRecompSessionSnaps.on (each
# attempt's start-state snapshots, about 960 MB).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/dev-env.sh"
game=${RBR_RSF:?set RBR_RSF to the RSF install (notes/verification.md)}
prefix=${RBR_RSF_PREFIX:?set RBR_RSF_PREFIX to the Wine prefix the RSF game runs in (notes/verification.md)}
plugin=${RBR_RSF_PLUGIN:?set RBR_RSF_PLUGIN to the RSF plugin directory (notes/verification.md)}
runs=${RBR_RSF_RUNS:?set RBR_RSF_RUNS to the RSF runs directory (notes/verification.md)}
lock=${RSF_LOCK:-/tmp/rbr-rsf-run.lock}
auto=${AUTO:-}
cmd=${1:-}
name=${2:-}
[[ "$game" =~ ^/[A-Za-z0-9\ ._/-]+$ && "$prefix" =~ ^/[A-Za-z0-9\ ._/-]+$ && "$lock" =~ ^/[A-Za-z0-9._/-]+$ ]] ||
  { echo "bad RBR_RSF, RBR_RSF_PREFIX or RSF_LOCK" >&2; exit 1; }
[[ "$auto" =~ ^([0-9]+(\ [0-9]+){3,5})?$ ]] || { echo "bad AUTO" >&2; exit 1; }
case $cmd in start|stop|fetch|_watch) [[ "$name" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "bad session name" >&2; exit 1; } ;; esac

root=$(cd "$(dirname "$0")/.." && pwd)
install() {
  RBR_PLUGIN_SDK=${RBR_PLUGIN_SDK:-${RBR_PATCH:+$RBR_PATCH/Plugins/RBRTestPlugin}} "$root/harness/plugin/build.sh"
  local commit
  commit=$(git -C "$root" rev-parse HEAD)$(git -C "$root" diff --quiet HEAD -- harness/plugin || echo +dirty)
  mkdir -p "$plugin"
  cp "$root/build/harness/RBRRecompHarness.dll" "$plugin/RBRRecompHarness.dll.tmp" &&
    mv "$plugin/RBRRecompHarness.dll.tmp" "$plugin/RBRRecompHarness.dll"
  cp "$root/harness/session.py" "$plugin/session.py"
  echo "$commit" > "$plugin/commit"
  echo "installed: the plugin ($commit) and session.py in $plugin"
}
run=$runs/$name
case $cmd in
  install) install; exit ;;
  fetch)
      dir=${3:-${RBR_RECORDINGS:?set RBR_RECORDINGS or give DIR}/rsf/$name}
      mkdir -p "$dir"
      cp -a "$run/." "$dir/"
      echo "$dir"; exit ;;
  start|stop|status|_watch) ;;  # below
  *) sed -n '2,28p' "$0" >&2; exit 1 ;;
esac

playback=${PLAYBACK:-}
fast=${FAST:-}
[[ "$playback" =~ ^[A-Za-z0-9_./-]*$ && "$fast" =~ ^[0-9]*$ ]] || { echo "bad PLAYBACK or FAST" >&2; exit 1; }
added=(Plugins/RBRRecompHarness.dll RBRRecompRecord.on RBRRecompSession.on)
[ -n "$auto" ] && added+=(RBRRecompAuto.txt)
[ -n "$playback" ] && added+=(RBRRecompPlayback.bin)
[ -n "$fast" ] && added+=(RBRRecompFastForward.txt)
[ -n "${FFB:-}" ] && added+=(RBRRecompFfb.on)
[ -n "${SNAPS:-}" ] && added+=(RBRRecompSessionSnaps.on)

# The game's processes: RichardBurnsRally_SSE.exe in any case (the RSF launcher starts X:\...\richardburnsrally_sse.exe)
# in this install's prefix (umu sets .../pfx/).
game_pids() {
  local d
  for d in /proc/[0-9]*; do
    grep -qai 'RichardBurnsRally_SSE\.exe' "$d/cmdline" 2>/dev/null || continue
    tr '\0' '\n' 2>/dev/null <"$d/environ" |
      grep -qxF -e "WINEPREFIX=$prefix" -e "WINEPREFIX=$prefix/" -e "WINEPREFIX=$prefix/pfx" -e "WINEPREFIX=$prefix/pfx/" || continue
    echo "${d#/proc/}"
  done
}

# key=value lines of an ini section (CRLF and BOM stripped); with keys, only those (exact names, case kept).
ini_section() {
  local file=$1 section=$2
  shift 2
  [ -f "$file" ] || return 0
  awk -v want="[$section]" -v keys="$*" '
    BEGIN { n = split(keys, k, " "); for (i = 1; i <= n; i++) only[k[i]] = 1 }
    { sub(/\r$/, ""); sub(/^\xef\xbb\xbf/, "") }
    /^\[/ { inside = ($0 == want); next }
    inside && /=/ {
      key = $0; sub(/[ \t]*=.*/, "", key); value = $0; sub(/^[^=]*=[ \t]*/, "", value)
      if (!n || (key in only)) print key "=" value
    }' "$file"
}

# The file version of a PE file (VS_FIXEDFILEINFO).
pe_version() {
  [ -f "$1" ] || return 0
  python3 -I -c '
import struct, sys
d = open(sys.argv[1], "rb").read()
i = d.find(struct.pack("<I", 0xfeef04bd))
if i >= 0:
    ms, ls = struct.unpack_from("<II", d, i + 8)
    print("%d.%d.%d.%d" % (ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff))' "$1"
}

# The settings meta.txt keeps, each line prefixed with $1 ("before." or "after.").
meta() {
  local p=$1 f s
  {
    echo "date=$(date -Is)"
    echo "rsf_launcher_version=$(pe_version "$game/rsf_launcher/RSF_Launcher.dll")"
    echo "ngp_dll_version=$(pe_version "$game/Plugins/NGP.dll")"
    echo "ngp_dll_sha256=$(sha256sum "$game/Plugins/NGP.dll" 2>/dev/null | cut -d' ' -f1)"
    echo "exe_sha256=$(sha256sum "$game/RichardBurnsRally_SSE.exe" | cut -d' ' -f1)"
    echo "plugin_commit=$(cat "$plugin/commit" 2>/dev/null)"
    ini_section "$game/RichardBurnsRally.ini" NGP | sed 's/^/ini.NGP./'
    for s in $(grep -o '^\[[^]]*\]' "$game/Plugins/RBRControls/RBRControls.ini" 2>/dev/null | tr -d '[]\r' | sed 's/^\xef\xbb\xbf//'); do
      ini_section "$game/Plugins/RBRControls/RBRControls.ini" "$s" | sed "s/^/rbrcontrols.$s./"
    done
    # the driven cars' keys of rallysimfans_personal.ini (finish sets $cars)
    for s in ${cars:-}; do car_keys | sed -n "s/^car$s\./rsf.car$s./p"; done
    # the wheel and the shifter (Logitech, Thrustmaster): kernel driver and the lg4ff sysfs settings
    for f in /sys/bus/hid/devices/*:046D:* /sys/bus/hid/devices/*:044F:*; do
      [ -e "$f" ] || continue
      s=hid.$(basename "$f")
      grep -q '^HID_NAME=.*\(Wheel\|Shift\|Racing\|T500\)' "$f/uevent" || continue
      echo "$s.name=$(sed -n 's/^HID_NAME=//p' "$f/uevent")"
      echo "$s.driver=$([ -L "$f/driver" ] && basename "$(readlink -f "$f/driver")")"
      for k in range autocenter gain combine_pedals; do if [ -r "$f/$k" ]; then echo "$s.$k=$(cat "$f/$k")"; fi; done
    done
  } | sed "s/^/$p/" || true
}

# Only these keys of rallysimfans_personal.ini, of every [car<id>] section, as car<id>.<key>=value lines (the file is
# the player's: never copied whole).
car_keys() {
  [ -f "$game/rallysimfans_personal.ini" ] || return 0
  awk '
    { sub(/\r$/, ""); sub(/^\xef\xbb\xbf/, "") }
    /^\[/ { car = tolower($0) ~ /^\[car[0-9]+\]$/ ? tolower(substr($0, 2, length($0) - 2)) : ""; next }
    car != "" && /=/ {
      key = $0; sub(/[ \t]*=.*/, "", key); value = $0; sub(/^[^=]*=[ \t]*/, "", value)
      if (tolower(key) ~ /^(steeringrotation|forcefeedbacksensitivity(tarmac|gravel|snow))$/) print car "." key "=" value
    }' "$game/rallysimfans_personal.ini"
}

# The car the attempt's slot holds: RSF writes [Car05] of Cars.ini (RSFCarID, RSFCarPhysics, CarName) when it installs
# a car; the slot itself comes from the session file's GAM1.
note_attempts() {
  local f tick
  for f in "$game"/RBRRecompSession-*.bin; do
    [ -e "$f" ] || continue
    tick=${f##*-}; tick=${tick%.bin}
    [ -e "$run/.attempt-$tick" ] && continue
    { echo "tick=$tick"; ini_section "$game/Cars/Cars.ini" Car05 RSFCarID RSFCarPhysics CarName |
      sed 's/^RSFCarID=/rsf_car_id=/; s/^RSFCarPhysics=/rsf_car_physics=/; s/^CarName=/car_name=/'; } >"$run/.attempt-$tick"
  done
}

# The game's PipeWire output streams (Stream/Output/Audio of the game's processes): "serial rate channels name" lines.
audio_nodes() {
  pw-dump 2>/dev/null | python3 -I -c '
import json, sys
pids = set(sys.argv[1:])
for o in json.load(sys.stdin):
    info = o.get("info") or {}
    p = info.get("props") or {}
    if o.get("type") != "PipeWire:Interface:Node" or p.get("media.class") != "Stream/Output/Audio":
        continue
    who = " ".join(str(p.get(k, "")) for k in ("application.process.binary", "application.name", "node.name"))
    if str(p.get("application.process.id")) not in pids and "richardburnsrally" not in who.lower():
        continue
    rate, channels = 48000, 2
    for f in (info.get("params") or {}).get("Format") or []:
        rate, channels = f.get("rate", rate), f.get("channels", channels)
    print(p.get("object.serial", o["id"]), rate, channels, p.get("node.name", "?").replace(" ", "_"))' $(game_pids)
}

# Recorded streams: one pw-record per game output node, audio.wav for the first, audio-<k>.wav for more. A rough
# baseline: start_unix_ns is when pw-record started, and a paused stream leaves no samples.
audio_pids=()
audio_seen=" "
record_audio() {
  local serial rate channels node base k
  while read -r serial rate channels node; do
    [[ "$audio_seen" == *" $serial "* ]] && continue
    k=${#audio_pids[@]}
    if [ "$k" = 0 ]; then base=audio; else base=audio-$((k + 1)); fi
    printf 'start_unix_ns=%s\nrate=%s\nchannels=%s\nformat=f32\nnode=%s\nserial=%s\n' "$(date +%s%N)" "$rate" \
      "$channels" "$node" "$serial" >"$run/$base.txt"
    pw-record --target "$serial" --rate "$rate" --channels "$channels" --format f32 -P node.dont-reconnect=true \
      "$run/$base.wav" 2>>"$run/watch.log" &
    audio_pids+=($!)
    audio_seen+="$serial "
    echo "audio: recording node $serial ($node, $rate Hz, $channels ch) to $base.wav" >>"$run/watch.log"
  done < <(audio_nodes)
}

# The device files the game's prefix holds open (hidraw: Wine talks HID to the wheel itself; input/event: evdev, the
# kernel driver makes the reports), to devices.txt.
note_devices() {
  local d f
  for d in /proc/[0-9]*; do
    tr '\0' '\n' 2>/dev/null <"$d/environ" |
      grep -qxF -e "WINEPREFIX=$prefix" -e "WINEPREFIX=$prefix/" -e "WINEPREFIX=$prefix/pfx" -e "WINEPREFIX=$prefix/pfx/" || continue
    for f in "$d"/fd/*; do
      f=$(readlink "$f" 2>/dev/null) || continue
      case $f in /dev/hidraw*|/dev/input/*|/dev/bus/usb/*|/dev/uinput) echo "$(tr '\0' ' ' <"$d/cmdline" | cut -c1-80) $f" ;; esac
    done
  done | sort -u >"$run/devices.txt"
  for f in $(grep -o '/dev/hidraw[0-9]*\|/dev/input/event[0-9]*' "$run/devices.txt" | sort -u); do
    echo "$f: $(cat "/sys/class/${f#/dev/}/device/uevent" 2>/dev/null | grep -m1 -e HID_NAME -e NAME | cut -d= -f2-)$(cat "/sys/class/${f#/dev/}/device/name" 2>/dev/null)"
  done >>"$run/devices.txt"
}

# End of the session: audio, attempts, meta, the manifest's files.
finish() {
  trap - EXIT TERM INT
  set +e  # put the install back whatever fails here
  local p f n tick
  for p in "${audio_pids[@]}"; do kill -INT "$p" 2>/dev/null || true; done
  for p in "${audio_pids[@]}"; do wait "$p" 2>/dev/null || true; done
  note_attempts
  # the driven cars: their keys as the session began (start's .cars), the reader's before.rsf.car<id>.<key>
  cars=$(cat "$run"/.attempt-* 2>/dev/null | sed -n 's/^rsf_car_id=//p' | tr -d '\r' | sort -un)
  for f in $cars; do sed -n "s/^car$f\./before.rsf.car$f./p" "$run/.cars" 2>/dev/null; done >>"$run/meta.txt"
  n=0
  for tick in $(ls "$game" | sed -n 's/^RBRRecomp\(Record\|Session\)-\([0-9]*\)\.bin$/\2/p' | sort -un); do
    n=$((n + 1))
    mkdir -p "$run/$n"
    [ -e "$game/RBRRecompRecord-$tick.bin" ] && mv "$game/RBRRecompRecord-$tick.bin" "$run/$n/physics.bin"
    [ -e "$game/RBRRecompSession-$tick.bin" ] && mv "$game/RBRRecompSession-$tick.bin" "$run/$n/session.bin"
    [ -e "$run/.attempt-$tick" ] && mv "$run/.attempt-$tick" "$run/$n/attempt.txt"
    # its start-state snapshots (before its first step, and before step 1520), for rbr-standalone --snapshot
    for f in "$game"/RBRRecompSnap-"$tick"-*.bin; do
      [ -e "$f" ] && mv "$f" "$run/$n/snap-${f##*-}"
    done
    # the files the attempt's stage load opened; setup.lsp the setup its car started on (session.py --setup)
    if [ -e "$run/$n/session.bin" ]; then
      python3 -I -B "$plugin/session.py" --files "$run/$n/files" "$run/$n/session.bin" >/dev/null 2>>"$run/watch.log"
      f=$(python3 -I -B "$plugin/session.py" --setup "$run/$n/session.bin" "$game/rsfdata/cache/stages_data.json" \
        2>>"$run/watch.log")
      [ -n "$f" ] && cp "$run/$n/files/$f" "$run/$n/setup.lsp" && echo "setup=$f" >>"$run/$n/attempt.txt"
    fi
  done
  [ -e "$game/RBRRecompHarness.log" ] && mv "$game/RBRRecompHarness.log" "$run/harness.log"
  meta after. >>"$run/meta.txt"
  while read -r how f; do
    case $how in
      added) rm -f "$game/$f" ;;
      replaced) mv "$run/.replaced/$(basename "$f")" "$game/$f" ;;
    esac
  done <"$run/.manifest"
  rmdir "$run/.replaced" 2>/dev/null || true
  rm -f "$run"/.attempt-* "$run/.ready" "$run/.pid" "$run/.cars"
  echo "$n stage attempts" >"$run/.done"
  echo "session $name ended: $n stage attempts in $run" >>"$run/watch.log"
  exit 0
}

# The session, detached: holds the lock until the game has run and exited, or until stop.
watch() {
  exec 9>"$lock"
  flock -n 9 || { echo "another session holds $lock" >"$run/.failed"; exit 1; }
  echo $$ >"$run/.pid"
  local f
  for f in "${added[@]}"; do
    [ "$f" = Plugins/RBRRecompHarness.dll ] && continue  # a test install's own copy is put back at the end
    [ ! -e "$game/$f" ] || { echo "$game/$f exists (an earlier session not ended?)" >"$run/.failed"; exit 1; }
  done
  if ls "$game" | grep -q '^RBRRecomp\(Record\|Session\|Snap\)-'; then
    echo "$game has RBRRecompRecord/Session/Snap files (an earlier session not ended?)" >"$run/.failed"; exit 1
  fi
  [ -z "$(game_pids)" ] || { echo "the game already runs: quit it, then start the session" >"$run/.failed"; exit 1; }
  : >"$run/.manifest"
  trap finish EXIT TERM INT
  # the harness's own files a test install may hold: set aside, put back at the end
  for f in Plugins/RBRRecompHarness.dll RBRRecompHarness.log; do
    if [ -e "$game/$f" ]; then
      mkdir -p "$run/.replaced"
      mv "$game/$f" "$run/.replaced/"
      echo "replaced $f" >>"$run/.manifest"
    elif [ "$f" = Plugins/RBRRecompHarness.dll ]; then
      echo "added $f" >>"$run/.manifest"
    fi
  done
  cp "$plugin/RBRRecompHarness.dll" "$game/Plugins/RBRRecompHarness.dll"
  for f in RBRRecompRecord.on RBRRecompSession.on; do touch "$game/$f"; echo "added $f" >>"$run/.manifest"; done
  if [ -n "$auto" ]; then echo "$auto" >"$game/RBRRecompAuto.txt"; echo "added RBRRecompAuto.txt" >>"$run/.manifest"; fi
  if [ -n "$playback" ]; then cp "$HOME/$playback" "$game/RBRRecompPlayback.bin"; echo "added RBRRecompPlayback.bin" >>"$run/.manifest"; fi
  if [ -n "$fast" ]; then echo "$fast" >"$game/RBRRecompFastForward.txt"; echo "added RBRRecompFastForward.txt" >>"$run/.manifest"; fi
  if [ -n "${FFB:-}" ]; then
    if [ "$FFB" = 1 ]; then acm=$(cd "$game" && ls -t SavedGames/pf*.acm | head -n 1); else acm=SavedGames/pf$FFB.acm; fi
    echo "./$acm" >"$game/RBRRecompFfb.on"; echo "added RBRRecompFfb.on" >>"$run/.manifest"
  fi
  if [ -n "${SNAPS:-}" ]; then
    touch "$game/RBRRecompSessionSnaps.on"; echo "added RBRRecompSessionSnaps.on" >>"$run/.manifest"
  fi
  # the settings that decide bindings, FFB, aids and rotation: the controller profiles and the game profiles (the .acm
  # the game loads is also in the session files, OPN1 and FIL1), RBRControls' files, NGPCarMenu.ini, RSF's car data
  mkdir -p "$run/config"
  cp "$game"/SavedGames/*.acm "$game"/SavedGames/pf*.rbr "$game/Plugins/RBRControls/RBRControls.ini" \
    "$game/Plugins/RBRControls/CarInfo.ini" "$game/Plugins/NGPCarMenu.ini" "$game/rsfdata/cache/cars_data.json" \
    "$run/config/" 2>/dev/null || true
  echo "session=$name" >"$run/meta.txt"
  meta before. >>"$run/meta.txt"
  car_keys >"$run/.cars"
  touch "$run/.ready"
  echo "waiting for the game" >>"$run/watch.log"
  until [ -n "$(game_pids)" ]; do sleep 2 & wait $!; done
  echo "game started" >>"$run/watch.log"
  local seconds=0
  while [ -n "$(game_pids)" ]; do
    record_audio
    note_attempts
    [ "$seconds" = 30 ] && note_devices
    sleep 2 & wait $!
    seconds=$((seconds + 2))
  done
  echo "game exited" >>"$run/watch.log"
  finish
}

case $cmd in
  start)
    [ -f "$plugin/RBRRecompHarness.dll" ] || { echo "no plugin in $plugin: run scripts/rsf-session.sh install" >&2; exit 1; }
    [ ! -e "$run" ] || { echo "$run exists" >&2; exit 1; }
    mkdir -p "$run"
    setsid "$0" _watch "$name" >>"$run/watch.log" 2>&1 </dev/null &
    for _ in $(seq 30); do
      [ -e "$run/.ready" ] && { echo "session $name: recording; start RSF (rbr, or rbr game) and drive. Quitting the game ends the session."; exit 0; }
      [ -e "$run/.failed" ] && { cat "$run/.failed" >&2; rm -rf "$run"; exit 1; }
      sleep 1
    done
    echo "the session did not start; see $run/watch.log" >&2; exit 1 ;;
  _watch) run=$run watch ;;
  stop)
    [ -e "$run/.done" ] && { echo "session $name: $(cat "$run/.done") (already ended)"; exit 0; }
    [ -e "$run/.pid" ] || { echo "no session $name" >&2; exit 1; }
    if [ -n "$(game_pids)" ] && [ "${3:-}" != --force ]; then
      echo "the game still runs: quit it (the session then ends by itself), or stop $name --force" >&2; exit 1
    fi
    pid=$(cat "$run/.pid")
    if [[ "$pid" =~ ^[0-9]+$ ]] && tr '\0' ' ' 2>/dev/null <"/proc/$pid/cmdline" | grep -q " _watch $name $"; then
      kill -TERM "$pid"
      for _ in $(seq 60); do [ -e "$run/.done" ] && break; sleep 1; done
      echo "session $name: $(cat "$run/.done" 2>/dev/null || echo 'not ended; see watch.log')"
    else
      # a stale session (its watcher died): end it here from the manifest, as the watcher would
      [ -z "$(game_pids)" ] || { echo "the game still runs: quit it, then stop $name" >&2; exit 1; }
      exec 9>"$lock"
      flock -n 9 || { echo "another session holds $lock" >&2; exit 1; }
      echo "session $name: watcher $pid is gone; ending it from the manifest" | tee -a "$run/watch.log"
      finish
    fi ;;
  status)
    for f in "$runs"/*/.pid; do [ -e "$f" ] && echo "active: $(basename "$(dirname "$f")")"; done; true ;;
  *) sed -n '2,28p' "$0" >&2; exit 1 ;;
esac
