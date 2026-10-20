#!/bin/bash
# Compares the step's cost between two rbr-standalone builds: runs the boot drive (drive-tanner-corolla.bin on
# track-54_E, 13000 steps) with A and B in turn, RUNS times each (default 3), and prints per build the fewest
# instructions per step over its runs (other load on the machine only ever adds to them), the median cycles and
# stepping time, and the allocations per step, then B against A. The instruction counts need macOS (rbr-counters);
# cycles and time move with the machine's load, instructions and allocations hardly.
# Build A from the base first, e.g. `cargo build --release -p rbr-standalone` in its checkout, and copy the binary.
# DRIVE, STAGE and STEPS pick another drive in $RBR_RECORDINGS.
# Usage: scripts/perf-step.sh A_BIN B_BIN [RUNS]
set -euo pipefail
a=${1:?usage: scripts/perf-step.sh A_BIN B_BIN [RUNS]}
b=${2:?usage: scripts/perf-step.sh A_BIN B_BIN [RUNS]}
runs=${3:-3}
recordings=${RBR_RECORDINGS:?set RBR_RECORDINGS (docs/development.md "Set up")}
: "${RBR_DATA:?set RBR_DATA}" "${RBR_PATCH:?set RBR_PATCH}"
drive=${DRIVE:-drive-tanner-corolla}
stage=${STAGE:-track-54_E}
steps=${STEPS:-13000}
out=$(mktemp -d /tmp/perf-step-XXXXXX)
trap 'rm -rf "$out"' EXIT

# One run: "<stepping s> <instructions> <cycles> <allocations> <bytes>" per step, appended to $out/<name>.
run() {
    local line
    line=$("$2" --boot --recording "$recordings/$drive.bin" --stage "$stage" --steps "$steps" 2>&1 >/dev/null |
        grep ': timing: ') || { echo "$2: no timing line" >&2; exit 1; }
    line=$(sed -nE 's/.*stepping ([0-9.]+) s .*; stepping ([0-9]+) instructions, ([0-9]+) cycles, ([0-9]+) allocations \(([0-9]+) bytes\).*/\1 \2 \3 \4 \5/p' <<<"$line")
    [ -n "$line" ] || { echo "$2: no counts in the timing line (macOS only)" >&2; exit 1; }
    echo "$line" >>"$out/$1"
}

for _ in $(seq "$runs"); do
    run a "$a"
    run b "$b"
done

# "<instructions> <cycles> <stepping> <allocations> <bytes>": the fewest instructions, the median cycles and time.
summary() {
    local n=$runs
    echo "$(sort -n -k2 "$out/$1" | head -1 | cut -d' ' -f2)" \
        "$(sort -n -k3 "$out/$1" | sed -n "$(((n + 1) / 2))p" | cut -d' ' -f3)" \
        "$(sort -n -k1 "$out/$1" | sed -n "$(((n + 1) / 2))p" | cut -d' ' -f1)" \
        "$(sort -n -k4 "$out/$1" | head -1 | cut -d' ' -f4,5)"
}
read -r ai ac as aa ab <<<"$(summary a)"
read -r bi bc bs ba bb <<<"$(summary b)"
echo "per step over $runs runs of $drive ($steps steps): instructions (fewest), cycles and stepping s (median), allocations"
printf 'A %12d %10d %6.2f %6d (%d bytes)  %s\n' "$ai" "$ac" "$as" "$aa" "$ab" "$a"
printf 'B %12d %10d %6.2f %6d (%d bytes)  %s\n' "$bi" "$bc" "$bs" "$ba" "$bb" "$b"
awk -v ai="$ai" -v bi="$bi" -v ac="$ac" -v bc="$bc" -v as="$as" -v bs="$bs" -v aa="$aa" -v ba="$ba" 'BEGIN {
    printf "B vs A: instructions %+.2f %%, cycles %+.1f %%, stepping %+.1f %%, allocations %+d\n",
        (bi / ai - 1) * 100, (bc / ac - 1) * 100, (bs / as - 1) * 100, ba - aa }'
