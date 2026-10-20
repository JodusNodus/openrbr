#!/bin/bash
# Packs the published test data (docs/development.md "Set up") from the maintainer's stores into OUT/<tag>/:
#   recordings.tar.zst  the files in RBR_RECORDINGS that data/test-data.recordings lists
#   vectors.tar.zst     RBR_VECTOR_STORE without dumps/ (snapshots) and staging/
# and OUT/<tag>.SHA256SUMS, which replaces data/test-data.SHA256SUMS. Upload the two archives as the assets of a
# GitHub release named <tag> (a new tag for every new set; old checkouts keep fetching theirs).
# Usage: scripts/pack-test-data.sh TAG OUT
set -euo pipefail
cd "$(dirname "$0")/.."
tag=${1:?usage: scripts/pack-test-data.sh TAG OUT}
out=${2:?usage: scripts/pack-test-data.sh TAG OUT}
recordings=${RBR_RECORDINGS:?set RBR_RECORDINGS}
store=${RBR_VECTOR_STORE:?set RBR_VECTOR_STORE}
mkdir -p "$out/$tag"

grep -v '^#' data/test-data.recordings > "$out/$tag/recordings.files"
(cd "$store" && find . -type f ! -name .DS_Store ! -path './dumps/*' ! -path './staging/*' | sed 's|^\./||' | sort) \
    > "$out/$tag/vectors.files"

for part in recordings vectors; do
    [ $part = recordings ] && dir=$recordings || dir=$store
    echo "$part: $(wc -l < "$out/$tag/$part.files") files"
    # No macOS metadata, so the archive unpacks cleanly with GNU tar.
    tar -C "$dir" --no-xattrs --no-mac-metadata -cf - -T "$out/$tag/$part.files" |
        zstd -q -T0 -19 --long=30 -o "$out/$tag/$part.tar.zst"
    rm "$out/$tag/$part.files"
done
(cd "$out" && shasum -a 256 "$tag/recordings.tar.zst" "$tag/vectors.tar.zst") > "$out/$tag.SHA256SUMS"
cat "$out/$tag.SHA256SUMS"
