#!/bin/bash
# Device traces of the wheel's force feedback on the test machine (RBR_REMOTE_HOST), without ptrace or root:
# harness/ffb_trace.c preloaded into the process that writes to the wheel's /dev/input/event* (Proton's winedevice
# for RSF, rbr-render's ffb_play example for ours) logs every EVIOCSFF, EVIOCRMFF and EV_FF write
# (notes/ngp.md "Force feedback"; harness/ffb_trace.py reads the logs). The wheel turns by itself: keep runs short.
#
#   harness/ffb_trace.sh rsf NAME SECS AUTO [PLAYBACK] RSF starts AUTO ("map car weather tyre"), driven by PLAYBACK
#                                                      (an FRM1 relative to the remote home; else no one) for SECS from the game's
#                                                      start, in an RSF session (scripts/rsf-session.sh, which holds
#                                                      /tmp/rbr-rsf-run.lock), then is stopped; FFB=1 binds the profile so
#                                                      NGP sends force feedback, FAST=1 plays at the game's pace (rsf-session.sh)
#   harness/ffb_trace.sh probe NAME FILE               Proton's dinput plays FILE as NGP calls it (harness/probes/ffb_replay.c)
#   harness/ffb_trace.sh ours NAME FILE                rbr_render::ffb::Wheel plays FILE (`seconds magnitude` lines,
#                                                      ffb_trace.py magnitudes) under the same lock
#   harness/ffb_trace.sh fetch NAME DIR                the trace (and an RSF run's session.bin) to DIR
#
# Remote: ~/rbr-ffb-trace/NAME/ffb-<pid>.txt; the RSF session in ~/rbr-rsf-runs/ffbtrace-NAME. ours builds in
# $RBR_REMOTE_RUST/src/<worktree> (scripts/remote-rust.sh).
set -euo pipefail
host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
root=$(cd "$(dirname "$0")/.." && pwd)
cmd=${1:-}
name=${2:-}
[[ "$name" =~ ^[A-Za-z0-9_-]+$ ]] || { sed -n '2,18p' "$0" >&2; exit 1; }
proton=${RBR_REMOTE_PROTON:-/usr/share/steam/compatibilitytools.d/proton-cachyos-slr}

build() {
  ssh -o BatchMode=yes "$host" mkdir -p rbr-ffb-trace
  scp -q "$root/harness/ffb_trace.c" "$host:rbr-ffb-trace/"
  ssh -o BatchMode=yes "$host" "cd rbr-ffb-trace && cc -O2 -Wall -shared -fPIC -fno-stack-protector \
    -U_FORTIFY_SOURCE -o ffb_trace.so ffb_trace.c"
}

case $cmd in
  rsf)
    secs=$3 auto=$4 playback=${5:-}
    build
    ssh -o BatchMode=yes "$host" "AUTO='$auto' PLAYBACK='$playback' FFB='${FFB:-}' FAST='${FAST:-}' ~/.local/bin/rsf-record start ffbtrace-$name"
    ssh -o BatchMode=yes "$host" bash -s -- "$name" "$secs" "$proton" <<'EOF'
set -u
token=RBR_FFB_TRACE_RUN=$1.$$
out=$HOME/rbr-ffb-trace/$1
rm -rf "$out" && mkdir -p "$out"
cd "$HOME/Games/Richard Burns Rally"
# as ~/.local/bin/rbr starts the game, with the display of the test machine's session and the trace preloaded
(export "$token" WINEPREFIX="$HOME/Games/prefixes/rbr" PROTONPATH="$3" GAMEID=umu-rbr \
   WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
   WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u) \
   LD_PRELOAD=$HOME/rbr-ffb-trace/ffb_trace.so FFB_TRACE_DIR="$out"
 setsid umu-run ./RichardBurnsRally_SSE.exe >"$out/proton.log" 2>&1 &)
sleep "$2"
# stop only the processes with this run's token
ours() { for p in $(pgrep -u "$(id -u)" .); do { tr '\0' '\n' <"/proc/$p/environ"; } 2>/dev/null | grep -qxF "$token" && echo "$p"; done; }
p=$(ours); [ -n "$p" ] && kill $p 2>/dev/null; sleep 5
p=$(ours); [ -n "$p" ] && kill -9 $p 2>/dev/null; sleep 5
for _ in $(seq 30); do [ -e "$HOME/rbr-rsf-runs/ffbtrace-$1/.done" ] && break; sleep 2; done
cat "$HOME/rbr-rsf-runs/ffbtrace-$1/.done"
wc -l "$out"/ffb-*.txt
EOF
    ;;
  probe)
    file=$3
    remote=${RBR_REMOTE:-rbr-recomp-test}
    build
    ssh -o BatchMode=yes "$host" "mkdir -p $remote/build/ffbprobe"
    scp -q "$root/harness/probes/ffb_replay.c" "$host:$remote/build/ffbprobe/"
    scp -q "$file" "$host:rbr-ffb-trace/$name.magnitudes"
    ssh -o BatchMode=yes "$host" bash -s -- "$name" "$remote" "$proton" <<'EOF'
set -eu
out=$HOME/rbr-ffb-trace/$1
rm -rf "$out" && mkdir -p "$out"
cd ~/$2 && WINEPREFIX=$HOME/$2/wineprefix WINEDEBUG=-all wine toolchain/msvc71/bin/cl.exe /nologo /O2 \
  build/ffbprobe/ffb_replay.c /Fobuild/ffbprobe/ /Febuild/ffbprobe/ffb_replay.exe /link /LIBPATH:toolchain/msvc71/lib \
  /ENTRY:start /SUBSYSTEM:WINDOWS kernel32.lib user32.lib dinput8.lib dxguid.lib msvcrt.lib >"$out/cl.txt" ||
  { cat "$out/cl.txt"; exit 1; }
cp build/ffbprobe/ffb_replay.exe "$out/" && cp "$HOME/rbr-ffb-trace/$1.magnitudes" "$out/ffb_replay.txt"
exec 9>/tmp/rbr-rsf-run.lock
flock -n 9 || { echo "another run holds /tmp/rbr-rsf-run.lock" >&2; exit 1; }
# the RSF install's Proton in a prefix of its own (made on first use)
cd "$out" && WINEPREFIX=$HOME/rbr-ffb-trace/prefix PROTONPATH="$3" GAMEID=umu-rbr-ffb-trace \
  WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u) \
  LD_PRELOAD=$HOME/rbr-ffb-trace/ffb_trace.so FFB_TRACE_DIR="$out" umu-run ./ffb_replay.exe >"$out/proton.log" 2>&1
cat "$out/ffb_replay.log"
wc -l "$out"/ffb-*.txt
EOF
    ;;
  ours)
    file=$3
    dir=${RBR_REMOTE_RUST:-rbr-rust}
    build
    rsync -a --delete --exclude target "$root/Cargo.toml" "$root/Cargo.lock" "$root/crates" "$host:$dir/src/$(basename "$root")/"
    ssh -o BatchMode=yes "$host" mkdir -p "$dir/src/$(basename "$root")/scripts"
    rsync -a "$root/scripts/packaging" "$host:$dir/src/$(basename "$root")/scripts/"  # the window icon rbr-render embeds
    scp -q "$file" "$host:rbr-ffb-trace/$name.magnitudes"
    ssh -o BatchMode=yes "$host" bash -s -- "$name" "$dir/src/$(basename "$root")" <<'EOF'
set -eu
out=$HOME/rbr-ffb-trace/$1
rm -rf "$out" && mkdir -p "$out"
nice -n 10 ~/.cargo/bin/cargo build -q --release -j 4 --manifest-path ~/$2/Cargo.toml -p rbr-render --example ffb_play
exec 9>/tmp/rbr-rsf-run.lock
flock -n 9 || { echo "another run holds /tmp/rbr-rsf-run.lock" >&2; exit 1; }
LD_PRELOAD=$HOME/rbr-ffb-trace/ffb_trace.so FFB_TRACE_DIR="$out" ~/$2/target/release/examples/ffb_play \
  "$HOME/rbr-ffb-trace/$1.magnitudes" --device G923
wc -l "$out"/ffb-*.txt
EOF
    ;;
  fetch)
    dir=$3
    mkdir -p "$dir"
    rsync -a "$host:rbr-ffb-trace/$name/" "$dir/"
    rsync -a "$host:rbr-rsf-runs/ffbtrace-$name/1/session.bin" "$dir/" 2>/dev/null || true
    echo "$dir" ;;
  *) sed -n '2,18p' "$0" >&2; exit 1 ;;
esac
