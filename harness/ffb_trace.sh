#!/bin/bash
# Device traces of the wheel's force feedback, without ptrace or root:
# harness/ffb_trace.c preloaded into the process that writes to the wheel's /dev/input/event* (Proton's winedevice
# for RSF, rbr-render's ffb_play example for ours) logs every EVIOCSFF, EVIOCRMFF and EV_FF write
# (notes/ngp.md "Force feedback"; harness/ffb_trace.py reads the logs). The wheel turns by itself: keep runs short.
#
#   harness/ffb_trace.sh rsf NAME SECS AUTO [PLAYBACK] RSF starts AUTO ("map car weather tyre"), driven by PLAYBACK
#                                                      (an FRM1 relative to the home; else no one) for SECS from the game's
#                                                      start, in an RSF session (scripts/rsf-session.sh, which holds
#                                                      /tmp/rbr-rsf-run.lock), then is stopped; FFB=1 binds the profile so
#                                                      NGP sends force feedback, FAST=1 plays at the game's pace (rsf-session.sh)
#   harness/ffb_trace.sh probe NAME FILE                Proton's dinput plays FILE as NGP calls it (harness/probes/ffb_replay.c)
#   harness/ffb_trace.sh ours NAME FILE                 rbr_render::ffb::Wheel plays FILE (`seconds magnitude` lines,
#                                                      ffb_trace.py magnitudes) under the same lock
#   harness/ffb_trace.sh fetch NAME DIR                 the trace (and an RSF run's session.bin) to DIR
#
# Traces: $RBR_FFB_TRACE/NAME/ffb-<pid>.txt; the RSF session in $RBR_RSF_RUNS/ffbtrace-NAME.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/dev-env.sh"
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cmd=${1:-}
name=${2:-}
[[ "$name" =~ ^[A-Za-z0-9_-]+$ ]] || { sed -n '2,18p' "$0" >&2; exit 1; }
proton=${RBR_PROTON:-}
# as scripts/rsf-session.sh takes them: the RSF install and its prefix, and the lock every wheel run holds
game=${RBR_RSF:?set RBR_RSF to the RSF install (notes/verification.md)}
prefix=${RBR_RSF_PREFIX:?set RBR_RSF_PREFIX to the Wine prefix the RSF game runs in (notes/verification.md)}
runs=${RBR_RSF_RUNS:?set RBR_RSF_RUNS to the RSF runs directory (notes/verification.md)}
trace=${RBR_FFB_TRACE:?set RBR_FFB_TRACE to the FFB trace directory (notes/verification.md)}
toolchain=${RBR_TOOLCHAIN:?set RBR_TOOLCHAIN to the MSVC 7.1 toolchain (notes/verification.md)}
lock=${RSF_LOCK:-/tmp/rbr-rsf-run.lock}
[[ "$game" =~ ^/[A-Za-z0-9\ ._/-]+$ && "$prefix" =~ ^/[A-Za-z0-9\ ._/-]+$ && "$lock" =~ ^/[A-Za-z0-9._/-]+$ &&
   "$proton" =~ ^(/[A-Za-z0-9._/-]+)?$ ]] || { echo "bad RBR_RSF, RBR_RSF_PREFIX, RSF_LOCK or RBR_PROTON" >&2; exit 1; }

build() {
  mkdir -p "$trace"
  (cd "$trace" && cc -O2 -Wall -shared -fPIC -fno-stack-protector \
    -U_FORTIFY_SOURCE -o ffb_trace.so "$root/harness/ffb_trace.c")
}

case $cmd in
  rsf)
    secs=${3:-} auto=${4:-} playback=${5:-} ffb=${FFB:-} fast=${FAST:-}
    # passed on to rsf-session.sh
    [[ "$secs" =~ ^[0-9]+$ && "$auto" =~ ^[0-9]+(\ [0-9]+){3,5}$ && "$playback" =~ ^[A-Za-z0-9_./-]*$ &&
       "$ffb" =~ ^[A-Za-z0-9_]*$ && "$fast" =~ ^[0-9]*$ ]] || { echo "bad SECS, AUTO, PLAYBACK, FFB or FAST" >&2; exit 1; }
    build
    RBR_RSF=$game RBR_RSF_PREFIX=$prefix RSF_LOCK=$lock AUTO="$auto" \
      PLAYBACK="$playback" FFB="$ffb" FAST="$fast" "$root/scripts/rsf-session.sh" start ffbtrace-"$name"
    out=$trace/$name
    rm -rf "$out" && mkdir -p "$out"
    # CLOCK_MONOTONIC - CLOCK_MONOTONIC_RAW in ns (ffb_trace.py steps' --raw-offset; it drifts ~1 ms in 10 min)
    python3 -c 'import time; print(time.clock_gettime_ns(time.CLOCK_MONOTONIC) - time.clock_gettime_ns(time.CLOCK_MONOTONIC_RAW))' \
      >"$out/raw_offset"
    token=RBR_FFB_TRACE_RUN=$name.$$
    # as ~/.local/bin/rbr starts the game, with the display of this machine's session and the trace preloaded
    (cd "$game" && export "$token" WINEPREFIX="$prefix" ${proton:+PROTONPATH=$proton} GAMEID=umu-rbr \
       WINEDLLOVERRIDES="mfc70,mfc71,msvci70,msvcp70,msvcp71,msvcr70,msvcr71=n" \
       WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u) \
       LD_PRELOAD=$trace/ffb_trace.so FFB_TRACE_DIR="$out"
     setsid umu-run ./RichardBurnsRally_SSE.exe >"$out/proton.log" 2>&1 &)
    sleep "$secs"
    # stop only the processes with this run's token
    ours() { for p in $(pgrep -u "$(id -u)" .); do { tr '\0' '\n' <"/proc/$p/environ"; } 2>/dev/null | grep -qxF "$token" && echo "$p"; done; }
    p=$(ours); [ -n "$p" ] && kill $p 2>/dev/null; sleep 5
    p=$(ours); [ -n "$p" ] && kill -9 $p 2>/dev/null; sleep 5
    for _ in $(seq 30); do [ -e "$runs/ffbtrace-$name/.done" ] && break; sleep 2; done
    cat "$runs/ffbtrace-$name/.done"
    wc -l "$out"/ffb-*.txt
    ;;
  probe)
    file=$3
    build
    out=$trace/$name
    rm -rf "$out" && mkdir -p "$out"
    export WINEPREFIX=$toolchain/wineprefix WINEDEBUG=-all
    wine "$toolchain/msvc71/bin/cl.exe" /nologo /O2 "$root/harness/probes/ffb_replay.c" \
      /Fo"$out/" /Fe"$out/ffb_replay.exe" /link "/LIBPATH:$toolchain/msvc71/lib" \
      /ENTRY:start /SUBSYSTEM:WINDOWS kernel32.lib user32.lib dinput8.lib dxguid.lib msvcrt.lib >"$out/cl.txt" ||
      { cat "$out/cl.txt"; exit 1; }
    cp "$file" "$out/ffb_replay.txt"
    exec 9>"$lock"
    flock -n 9 || { echo "another run holds $lock" >&2; exit 1; }
    # the RSF install's Proton in a prefix of its own (made on first use)
    (cd "$out" && WINEPREFIX=$trace/prefix ${proton:+PROTONPATH=$proton} GAMEID=umu-rbr-ffb-trace \
      WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/$(id -u) \
      LD_PRELOAD=$trace/ffb_trace.so FFB_TRACE_DIR="$out" umu-run ./ffb_replay.exe >"$out/proton.log" 2>&1)
    cat "$out/ffb_replay.log"
    wc -l "$out"/ffb-*.txt
    ;;
  ours)
    file=$3
    build
    out=$trace/$name
    rm -rf "$out" && mkdir -p "$out"
    nice -n 10 cargo build -q --release -j 4 --manifest-path "$root/Cargo.toml" -p rbr-render --example ffb_play
    cp "$file" "$trace/$name.magnitudes"
    exec 9>"$lock"
    flock -n 9 || { echo "another run holds $lock" >&2; exit 1; }
    LD_PRELOAD=$trace/ffb_trace.so FFB_TRACE_DIR="$out" "$root/target/release/examples/ffb_play" \
      "$trace/$name.magnitudes" --device G923
    wc -l "$out"/ffb-*.txt
    ;;
  fetch)
    dir=$3
    mkdir -p "$dir"
    cp -a "$trace/$name/." "$dir/"
    cp -a "$runs/ffbtrace-$name/1/session.bin" "$dir/" 2>/dev/null || true
    echo "$dir" ;;
  *) sed -n '2,18p' "$0" >&2; exit 1 ;;
esac
