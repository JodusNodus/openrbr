#!/bin/bash
# Downloads the published test data that data/test-data.SHA256SUMS names (docs/development.md "Set up"), checks it and
# unpacks each part to DIR/<tag>/<part>, then prints its exports (`eval "$(scripts/fetch-test-data.sh DIR)"`):
#   recordings  RBR_RECORDINGS                       the drives the tests read
#   vectors     RBR_VECTOR_STORE                     the per-call records
#   snapshots   RBR_SNAPS, RBR_REPLAY_FPU_CACHE      the hybrid check's and the vector tools' snapshots, the x87 cache
# PARTS default to recordings and vectors. A part already unpacked is kept; a new tag unpacks next to the old one.
# RBR_TEST_DATA_URL replaces the releases URL (a mirror, or file:// for a set packed here by scripts/pack-test-data.sh).
# Needs curl, tar and zstd.
# Usage: scripts/fetch-test-data.sh DIR [PART...]
set -euo pipefail
dir=${1:?usage: scripts/fetch-test-data.sh DIR [PART...]}
shift
parts=" ${*:-recordings vectors} "
sums=$(cd "$(dirname "$0")/.." && pwd)/data/test-data.SHA256SUMS
url=${RBR_TEST_DATA_URL:-https://github.com/JodusNodus/rerbr/releases/download}
command -v shasum >/dev/null && sha="shasum -a 256" || sha=sha256sum
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)

while read -r hash asset; do
    part=$(basename "$asset" .tar.zst)
    [[ $parts == *" $part "* ]] || continue
    to=$dir/$(dirname "$asset")/$part
    if [ "$(cat "$to/.sha256" 2>/dev/null)" != "$hash" ]; then
        echo "fetching $asset" >&2
        mkdir -p "$dir/$(dirname "$asset")"
        curl -fL --retry 3 --progress-bar -o "$dir/$asset" "$url/$asset"
        (cd "$dir" && echo "$hash  $asset" | $sha -c - >&2)
        rm -rf "$to"
        mkdir "$to"
        zstd -dc --long=31 "$dir/$asset" | tar -C "$to" -xf -
        rm "$dir/$asset"
        echo "$hash" > "$to/.sha256"
    fi
    case $part in
    recordings) echo "export RBR_RECORDINGS=$to" ;;
    vectors) echo "export RBR_VECTOR_STORE=$to" ;;
    snapshots) echo "export RBR_SNAPS=$to"; echo "export RBR_REPLAY_FPU_CACHE=$to/replay-fpu-cache.bin" ;;
    esac
done < "$sums"
