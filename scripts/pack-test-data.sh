#!/bin/bash
# Packs the published test data (docs/development.md "Set up") from the maintainer's stores into OUT/<tag>/:
#   recordings.tar.zst  the files in RBR_RECORDINGS that data/test-data.recordings lists
#   vectors.tar.zst     RBR_VECTOR_STORE without dumps/ (snapshots) and staging/
#   snapshots.tar.zst   the snapshots data/test-data.snapshots lists (dumps/ from RBR_VECTOR_STORE, the rest from
#                       RBR_SNAPS) and the x87 results cache (RBR_REPLAY_FPU_CACHE, default build/replay-fpu-cache.bin)
# and OUT/<tag>.SHA256SUMS, which replaces data/test-data.SHA256SUMS. Upload the archives as the assets of a GitHub
# release named <tag> (a new tag for every new set; old checkouts keep fetching theirs).
# Usage: scripts/pack-test-data.sh TAG OUT
set -euo pipefail
cd "$(dirname "$0")/.."
tag=${1:?usage: scripts/pack-test-data.sh TAG OUT}
out=${2:?usage: scripts/pack-test-data.sh TAG OUT}
recordings=${RBR_RECORDINGS:?set RBR_RECORDINGS}
store=${RBR_VECTOR_STORE:?set RBR_VECTOR_STORE}
snaps=${RBR_SNAPS:?set RBR_SNAPS}
cache=${RBR_REPLAY_FPU_CACHE:-build/replay-fpu-cache.bin}
mkdir -p "$out/$tag"

# Each part is a staging folder of hard links (copies across file systems), laid out as it unpacks.
stage() {
    mkdir -p "$out/$tag/$1/$(dirname "$3")"
    ln -f "$2" "$out/$tag/$1/$3" 2>/dev/null || cp "$2" "$out/$tag/$1/$3"
}
grep -v '^#' data/test-data.recordings | while read -r f; do stage recordings "$recordings/$f" "$f"; done
(cd "$store" && find . -type f ! -name .DS_Store ! -path './dumps/*' ! -path './staging/*' | sed 's|^\./||') |
    while read -r f; do stage vectors "$store/$f" "$f"; done
grep -v '^#' data/test-data.snapshots | while read -r f; do
    case $f in dumps/*) stage snapshots "$store/$f" "$f" ;; *) stage snapshots "$snaps/$f" "$f" ;; esac
done
stage snapshots "$cache" replay-fpu-cache.bin

for part in recordings vectors snapshots; do
    echo "$part: $(find "$out/$tag/$part" -type f | wc -l) files"
    # Sorted, without macOS metadata, so the archive unpacks cleanly with GNU tar.
    (cd "$out/$tag/$part" && find . -type f | sed 's|^\./||' | sort |
        tar --no-xattrs --no-mac-metadata -cf - -T -) | zstd -q -T0 -19 --long=31 -o "$out/$tag/$part.tar.zst"
    rm -r "${out:?}/$tag/$part"
done
(cd "$out" && shasum -a 256 "$tag"/{recordings,vectors,snapshots}.tar.zst) > "$out/$tag.SHA256SUMS"
cat "$out/$tag.SHA256SUMS"
