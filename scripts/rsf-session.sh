#!/bin/bash
# Record RallySimFans drives on the test machine (notes/verification.md "RSF sessions"): per stage attempt the harness
# plugin's FRM1 recording and RSS1 session file (RBRRecompSession.on; harness/session.py), the game's audio for the
# whole session, and the settings that shape input and force feedback.
#
#   scripts/rsf-session.sh install          (Mac) build the plugin, copy it, harness/session.py and this script (as
#                                           rsf-record) to the test machine (RBR_REMOTE_HOST)
#   scripts/rsf-session.sh start NAME       take the lock, put the plugin and the .on files into the install (a
#                                           manifest lists them), meta.txt "before", then record the audio once the
#                                           game runs. Start RSF as usual (`rbr` or `rbr game`), drive any number of
#                                           stages and quit the game: that ends the session. From the Mac it installs
#                                           first.
#   scripts/rsf-session.sh stop NAME        end it while the game runs no more (--force: while it runs)
#   scripts/rsf-session.sh status           the session holding the lock
#   scripts/rsf-session.sh fetch NAME [DIR] (Mac) copy ~/rbr-rsf-runs/NAME to DIR (default $RBR_RECORDINGS/rsf/NAME)
#
# The end of a session moves stage attempt n's files to ~/rbr-rsf-runs/NAME/<n>/: physics.bin, session.bin,
# attempt.txt, files/ (the .lsp files its stage load opened) and setup.lsp (the setup its car started on); writes
# meta.txt's "after" values and harness.log, and removes exactly what the manifest added. config/ holds the controller
# profiles and RBRControls' files as the session started, devices.txt the device files the game's prefix holds. On the
# test machine the same commands run as `rsf-record ...`. Env: RSF_GAME (default Games/Richard Burns Rally), RSF_PREFIX
# (Games/prefixes/rbr), both relative to the home of the test machine; RSF_LOCK (/tmp/rbr-rsf-run.lock). For tests
# without a driver: AUTO="map car weather tyre" adds RBRRecompAuto.txt (a stage start), PLAYBACK=<file relative to
# the home> RBRRecompPlayback.bin (its controls drive; the game quits after them), FAST=N RBRRecompFastForward.txt.
set -euo pipefail
game_rel=${RSF_GAME:-Games/Richard Burns Rally}
prefix_rel=${RSF_PREFIX:-Games/prefixes/rbr}
lock=${RSF_LOCK:-/tmp/rbr-rsf-run.lock}
auto=${AUTO:-}
cmd=${1:-}
name=${2:-}
[[ "$game_rel" =~ ^[A-Za-z0-9\ ._/-]+$ && "$prefix_rel" =~ ^[A-Za-z0-9\ ._/-]+$ && "$lock" =~ ^/[A-Za-z0-9._/-]+$ ]] ||
  { echo "bad RSF_GAME, RSF_PREFIX or RSF_LOCK" >&2; exit 1; }
[[ "$auto" =~ ^([0-9]+(\ [0-9]+){3,5})?$ ]] || { echo "bad AUTO" >&2; exit 1; }
case $cmd in start|stop|fetch|_watch) [[ "$name" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "bad session name" >&2; exit 1; } ;; esac

# ---- on the Mac: build and copy, or run the command on the test machine ----
if [ ! -d "$HOME/$game_rel" ]; then
  host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
  root=$(cd "$(dirname "$0")/.." && pwd)
  remote() {
    ssh -o BatchMode=yes "$host" bash -s <<EOF
export RSF_GAME='$game_rel' RSF_PREFIX='$prefix_rel' RSF_LOCK='$lock' AUTO='$auto'
exec ~/.local/bin/rsf-record $*
EOF
  }
  install() {
    RBR_PLUGIN_SDK=${RBR_PLUGIN_SDK:-${RBR_PATCH:+$RBR_PATCH/Plugins/RBRTestPlugin}} "$root/harness/plugin/build.sh"
    local commit
    commit=$(git -C "$root" rev-parse HEAD)$(git -C "$root" diff --quiet HEAD -- harness/plugin || echo +dirty)
    ssh -o BatchMode=yes "$host" "mkdir -p rbr-rsf-plugin .local/bin && cp ${RBR_REMOTE:-rbr-recomp-test}/build/harness/RBRRecompHarness.dll rbr-rsf-plugin/ && echo $commit > rbr-rsf-plugin/commit"
    scp -q "$0" "$host:.local/bin/rsf-record"
    scp -q "$root/harness/session.py" "$host:rbr-rsf-plugin/session.py"
    echo "installed: ~/.local/bin/rsf-record and the plugin ($commit) on $host"
  }
  case $cmd in
    install) install ;;
    start) install; remote start "$name" ;;
    stop) remote stop "$name" "${3:-}" ;;
    status) remote status ;;
    fetch)
      dir=${3:-${RBR_RECORDINGS:?set RBR_RECORDINGS or give DIR}/rsf/$name}
      mkdir -p "$dir"
      rsync -a "$host:rbr-rsf-runs/$name/" "$dir/"
      echo "$dir" ;;
    *) sed -n '2,24p' "$0" >&2; exit 1 ;;
  esac
  exit
fi

# ---- on the test machine ----
game=$HOME/$game_rel
prefix=$HOME/$prefix_rel
runs=$HOME/rbr-rsf-runs
plugin=$HOME/rbr-rsf-plugin
run=$runs/$name
playback=${PLAYBACK:-}
fast=${FAST:-}
[[ "$playback" =~ ^[A-Za-z0-9_./-]*$ && "$fast" =~ ^[0-9]*$ ]] || { echo "bad PLAYBACK or FAST" >&2; exit 1; }
added=(Plugins/RBRRecompHarness.dll RBRRecompRecord.on RBRRecompSession.on)
[ -n "$auto" ] && added+=(RBRRecompAuto.txt)
[ -n "$playback" ] && added+=(RBRRecompPlayback.bin)
[ -n "$fast" ] && added+=(RBRRecompFastForward.txt)

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
    # the driven cars (RSF car ids of the attempts so far), only these keys of rallysimfans_personal.ini
    for s in $(cat "$run"/.attempt-* 2>/dev/null | sed -n 's/^rsf_car_id=//p' | sort -un); do
      ini_section "$game/rallysimfans_personal.ini" "car$s" steeringrotation forcefeedbacksensitivitytarmac \
        forcefeedbacksensitivitygravel forcefeedbacksensitivitysnow | sed "s/^/rsf.car$s./"
    done
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
  n=0
  for tick in $(ls "$game" | sed -n 's/^RBRRecomp\(Record\|Session\)-\([0-9]*\)\.bin$/\2/p' | sort -un); do
    n=$((n + 1))
    mkdir -p "$run/$n"
    [ -e "$game/RBRRecompRecord-$tick.bin" ] && mv "$game/RBRRecompRecord-$tick.bin" "$run/$n/physics.bin"
    [ -e "$game/RBRRecompSession-$tick.bin" ] && mv "$game/RBRRecompSession-$tick.bin" "$run/$n/session.bin"
    [ -e "$run/.attempt-$tick" ] && mv "$run/.attempt-$tick" "$run/$n/attempt.txt"
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
  rm -f "$run"/.attempt-* "$run/.ready" "$run/.pid"
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
  if ls "$game" | grep -q '^RBRRecomp\(Record\|Session\)-'; then
    echo "$game has RBRRecompRecord/Session files (an earlier session not ended?)" >"$run/.failed"; exit 1
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
  # the bindings: the controller profiles and RBRControls' files (the profile the game loads is also in the session
  # files, OPN1 and FIL1)
  mkdir -p "$run/config"
  cp "$game"/SavedGames/*.acm "$game/Plugins/RBRControls/RBRControls.ini" "$game/Plugins/RBRControls/CarInfo.ini" \
    "$run/config/" 2>/dev/null || true
  echo "session=$name" >"$run/meta.txt"
  meta before. >>"$run/meta.txt"
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
    [ -f "$plugin/RBRRecompHarness.dll" ] || { echo "no plugin in $plugin: run scripts/rsf-session.sh install on the Mac" >&2; exit 1; }
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
    kill -TERM "$(cat "$run/.pid")"
    for _ in $(seq 60); do [ -e "$run/.done" ] && break; sleep 1; done
    echo "session $name: $(cat "$run/.done" 2>/dev/null || echo 'not ended; see watch.log')" ;;
  status)
    for f in "$runs"/*/.pid; do [ -e "$f" ] && echo "active: $(basename "$(dirname "$f")")"; done; true ;;
  *) sed -n '2,24p' "$0" >&2; exit 1 ;;
esac
