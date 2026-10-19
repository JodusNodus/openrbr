#!/usr/bin/env bash
# The test machine's side of harness/remote.sh: builds rbr-hybrid and runs one harness script there.
set -euo pipefail

mode=${1:-}
name=${2:-}
script=${3:-}
encoded_args=${4:-}
[[ $mode == run ]] || {
    echo "remote-runner.sh: expected run mode" >&2
    exit 2
}
[[ $name =~ ^[A-Za-z0-9._-]+$ && $name != . && $name != .. ]] || {
    echo "remote-runner.sh: invalid run name" >&2
    exit 2
}
[[ $script =~ ^harness/[A-Za-z0-9_./-]+\.py$ && $script != *..* ]] || {
    echo "remote-runner.sh: invalid harness script" >&2
    exit 2
}

root=$HOME/rbr-emu
source_root=$root/src/$name
orig_exe=$HOME/rbr-recomp-test/original/RichardBurnsRally_SSE.exe
cargo_home=$root/cargo
rustup_home=$root/rustup
fpu_dir=$root/fpu/$name
cache_dir=$root/cache
run_fpu_cache=$cache_dir/$name-replay-fpu-cache.bin
mkdir -p "$root/locks" "$root/snaps" "$root/recordings" "$cache_dir" "$fpu_dir"

check_disk() {
    local used
    used=$(df -P / | awk 'NR == 2 { gsub(/%/, "", $5); print $5 }')
    if [[ ! $used =~ ^[0-9]+$ ]] || (( used > 90 )); then
        echo "remote-runner.sh: refusing to start; disk use is ${used:-unknown}% (limit 90%)" >&2
        exit 75
    fi
}
check_disk

if [[ ! -e /tmp/rbr-snaps ]]; then
    ln -s "$root/snaps" /tmp/rbr-snaps 2>/dev/null || true
fi
if [[ ! -L /tmp/rbr-snaps || $(readlink -f /tmp/rbr-snaps) != "$root/snaps" ]]; then
    echo "remote-runner.sh: /tmp/rbr-snaps exists and is not the shared cache; refusing to replace it" >&2
    exit 1
fi

slot=
while [[ -z $slot ]]; do
    for candidate in 1 2 3 4; do
        exec 9>"$root/locks/wrapper-$candidate.lock"
        if flock -n 9; then
            slot=$candidate
            break
        fi
        exec 9>&-
    done
    [[ -n $slot ]] || sleep 1
done
trap 'exec 9>&-' EXIT

check_disk
[[ -x $root/.venv/bin/python3 ]] || {
    echo "remote-runner.sh: missing emulator Python at $root/.venv/bin/python3" >&2
    exit 1
}
[[ -f $orig_exe ]] || {
    echo "remote-runner.sh: original exe not found: $orig_exe" >&2
    exit 1
}

shared_fpu_cache=$cache_dir/replay-fpu-cache.bin
if [[ -f $shared_fpu_cache ]]; then
    cp "$shared_fpu_cache" "$run_fpu_cache"
else
    rm -f "$run_fpu_cache"
fi

export CARGO_HOME=$cargo_home RUSTUP_HOME=$rustup_home
export PATH=$cargo_home/bin:$PATH
echo "remote run $name: slot $slot; disk $(df -P / | awk 'NR == 2 {print $5}'); building rbr-hybrid" >&2
nice -n 10 cargo build --manifest-path "$source_root/Cargo.toml" \
    --release -j 4 -p rbr-hybrid >&2

args=()
if [[ -n $encoded_args ]]; then
    mapfile -d '' -t args < <(printf '%s' "$encoded_args" | base64 -d)
fi
export RBR_ORIG_EXE=$orig_exe
export RBR_REMOTE_HOST=
export RBR_REMOTE=rbr-emu/fpu/$name
export RBR_REPLAY_FPU_CACHE=$run_fpu_cache
export RBR_SNAPS=/tmp/rbr-snaps
export RBR_RECORDINGS=$root/recordings

echo "remote run $name: running $script" >&2
cd "$source_root"
set +e
nice -n 10 "$root/.venv/bin/python3" -I -B -u "$source_root/$script" "${args[@]}"
status=$?
set -e
echo "remote run $name: exit $status" >&2
exit "$status"
