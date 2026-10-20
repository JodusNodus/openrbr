#!/bin/bash
# Downloads the published test data that data/test-data.SHA256SUMS names (docs/development.md "Set up"), checks it and
# unpacks it to DIR/<tag>/recordings and DIR/<tag>/vectors, then prints the exports for RBR_RECORDINGS and
# RBR_VECTOR_STORE (`eval "$(scripts/fetch-test-data.sh DIR)"`). A set already unpacked is kept; a new tag unpacks
# next to the old one. RBR_TEST_DATA_URL replaces the releases URL (a mirror, or file:// for a set packed here by
# scripts/pack-test-data.sh). Needs curl, tar and zstd.
# Usage: scripts/fetch-test-data.sh DIR
set -euo pipefail
dir=${1:?usage: scripts/fetch-test-data.sh DIR}
sums=$(cd "$(dirname "$0")/.." && pwd)/data/test-data.SHA256SUMS
url=${RBR_TEST_DATA_URL:-https://github.com/JodusNodus/rerbr/releases/download}
command -v shasum >/dev/null && sha="shasum -a 256" || sha=sha256sum
mkdir -p "$dir"
dir=$(cd "$dir" && pwd)

while read -r hash asset; do
    part=$(basename "$asset" .tar.zst)
    tag=$(dirname "$asset")
    [ -f "$dir/$tag/$part/.sha256" ] && [ "$(cat "$dir/$tag/$part/.sha256")" = "$hash" ] && continue
    mkdir -p "$dir/$tag"
    echo "fetching $asset" >&2
    curl -fL --retry 3 --progress-bar -o "$dir/$asset" "$url/$asset"
    (cd "$dir" && echo "$hash  $asset" | $sha -c - >&2)
    rm -rf "$dir/$tag/$part"
    mkdir "$dir/$tag/$part"
    zstd -dc --long=30 "$dir/$asset" | tar -C "$dir/$tag/$part" -xf -
    rm "$dir/$asset"
    echo "$hash" > "$dir/$tag/$part/.sha256"
done < "$sums"

echo "export RBR_RECORDINGS=$dir/$tag/recordings"
echo "export RBR_VECTOR_STORE=$dir/$tag/vectors"
