#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage: harness/cachy.sh <name> <script> <args...>

Runs a Python script from harness/ on cachy after syncing this worktree's
harness/ and rust/ sources and building rbr-hybrid in release mode.
EOF
    exit 2
}

[[ $# -ge 2 ]] || usage
name=$1
script_arg=$2
shift 2

[[ $name =~ ^[A-Za-z0-9._-]+$ && $name != . && $name != .. ]] || {
    echo "cachy.sh: name must contain only letters, digits, dot, underscore, or hyphen" >&2
    exit 2
}

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
host=${RBR_CACHY_HOST:-cachy}
started_at=$(date +%s)

if [[ -f $script_arg ]]; then
    script_file=$(cd "$(dirname "$script_arg")" && pwd -P)/$(basename "$script_arg")
elif [[ -f $repo/harness/$script_arg ]]; then
    script_file=$repo/harness/$script_arg
elif [[ -f $repo/$script_arg ]]; then
    script_file=$repo/$script_arg
else
    echo "cachy.sh: script not found: $script_arg" >&2
    exit 2
fi

case $script_file in
    "$repo"/harness/*.py) script_rel=${script_file#"$repo"/} ;;
    *)
        echo "cachy.sh: script must be a Python file inside this worktree's harness/" >&2
        exit 2
        ;;
esac

check_disk() {
    local output used
    output=$(ssh "$host" 'df -P /') || {
        echo "cachy.sh: cannot read disk use on $host" >&2
        return 1
    }
    used=$(awk 'NR == 2 { gsub(/%/, "", $5); print $5 }' <<<"$output")
    [[ $used =~ ^[0-9]+$ ]] || {
        echo "cachy.sh: could not parse disk use on $host: $output" >&2
        return 1
    }
    if (( used > 90 )); then
        echo "cachy.sh: refusing to start; $host disk use is ${used}% (limit 90%)" >&2
        return 1
    fi
}

check_disk
remote_home=$(ssh "$host" 'echo $HOME')
[[ $remote_home == /* ]] || {
    echo "cachy.sh: could not resolve the home directory on $host" >&2
    exit 1
}
remote_root=$remote_home/rbr-emu

ssh "$host" "mkdir -p $remote_root/src/$name $remote_root/snaps $remote_root/recordings $remote_root/cache"
rsync -a --delete --exclude target/ --exclude .venv/ --exclude .git/ \
    "$repo/harness/" "$host:$remote_root/src/$name/harness/"
rsync -a --delete --exclude target/ --exclude .venv/ --exclude .git/ \
    "$repo/rust/" "$host:$remote_root/src/$name/rust/"
# long_run.py and worklist.py import scripts/progress.py, which reads data/.
for dir in scripts data; do
    rsync -a --delete "$repo/$dir/" "$host:$remote_root/src/$name/$dir/"
done

sync_lock=${TMPDIR:-/tmp}/rbr-emu-cache-sync.lock
sync_file() {
    local path=$1 remote_dir=$2
    lockf -k "$sync_lock" /bin/bash -c '
        if [[ -L $1 || -d $1 ]]; then
            rsync -aL --delay-updates "$1" "$2/"
        else
            rsync -a --checksum --delay-updates "$1" "$2/"
        fi
    ' cachy-cache-sync "$path" "$host:$remote_dir"
}

snap_dir=${RBR_SNAPS:-/tmp/rbr-snaps}
recording_dir=${RBR_RECORDINGS:-$repo/../recordings}
remote_args=()
recording_value=0
for arg in "$@"; do
    if (( recording_value )); then
        if [[ ! -f $arg ]]; then
            arg=$recording_dir/$(basename "$arg")
        fi
        if [[ ! -f $arg ]]; then
            echo "cachy.sh: --recording file not found locally: $arg" >&2
            exit 2
        fi
        sync_file "$arg" "$remote_root/recordings"
        remote_args+=("$remote_root/recordings/$(basename "$arg")")
        recording_value=0
        continue
    fi

    if [[ $arg == --recording ]]; then
        remote_args+=("$arg")
        recording_value=1
        continue
    fi

    if [[ $arg == *snap*.bin || $arg == *snapshot*.bin ]]; then
        local_path=$arg
        [[ -f $local_path ]] || local_path=$repo/$arg
        [[ -f $local_path ]] || local_path=$snap_dir/$(basename "$arg")
        if [[ ! -f $local_path ]]; then
            echo "cachy.sh: snapshot file not found locally: $arg" >&2
            exit 2
        fi
        sync_file "$local_path" "$remote_root/snaps"
        remote_args+=("/tmp/rbr-snaps/$(basename "$local_path")")
        continue
    fi

    if [[ -f $arg && ( $arg == */recordings/* || $(basename "$arg") == drive-*.bin ) ]]; then
        sync_file "$arg" "$remote_root/recordings"
        remote_args+=("$remote_root/recordings/$(basename "$arg")")
        continue
    fi

    remote_args+=("$arg")
done
if (( recording_value )); then
    echo "cachy.sh: --recording needs a path" >&2
    exit 2
fi

if [[ $script_rel == harness/long_run.py ]]; then
    [[ -d $snap_dir ]] || {
        echo "cachy.sh: snapshot directory not found: $snap_dir" >&2
        exit 2
    }
    drive=tanner-corolla
    for ((i = 0; i < ${#remote_args[@]} - 1; i++)); do
        if [[ ${remote_args[i]} == --drive ]]; then
            drive=${remote_args[i + 1]}
        fi
    done
    # Only the drive's own chain (long_run.py's DRIVES patterns), not the whole snapshot directory.
    prefix=vcu-$drive
    [[ $drive == tanner-corolla ]] && prefix=vcu-cov
    for snap in "$snap_dir"/"$prefix".snap-*.bin; do
        [[ -f $snap ]] && sync_file "$snap" "$remote_root/snaps"
    done
    drive_recording=$recording_dir/drive-$drive.bin
    if [[ -f $drive_recording ]]; then
        sync_file "$drive_recording" "$remote_root/recordings"
    fi
fi

fpu_cache=${RBR_REPLAY_FPU_CACHE:-}
if [[ -z $fpu_cache ]]; then
    for candidate in "$repo/build/replay-fpu-cache.bin" \
                     "$(dirname "$repo")/rust-integration/build/replay-fpu-cache.bin"; do
        if [[ -f $candidate ]]; then
            fpu_cache=$candidate
            break
        fi
    done
fi
if [[ -n $fpu_cache && -f $fpu_cache ]]; then
    sync_file "$fpu_cache" "$remote_root/cache"
fi

check_disk
encoded_args=$(printf '%s\0' "${remote_args[@]}" | base64 | tr -d '\n')
set +e
ssh "$host" "bash $remote_root/src/$name/harness/cachy-runner.sh run $name $script_rel $encoded_args"
status=$?
set -e
elapsed=$(( $(date +%s) - started_at ))
echo "cachy.sh: $name wall time ${elapsed}s (exit $status)" >&2
exit "$status"
