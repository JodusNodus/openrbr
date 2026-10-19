#!/bin/bash
# Creates an isolated build directory on cachy for one worker, sharing toolchain, originals and reccmp
# with the base ~/rbr-recomp-test. Use it via `export RBR_REMOTE=<dir>` before build/reccmp/match scripts.
# Usage: scripts/remote-setup.sh <dir name>, e.g. scripts/remote-setup.sh rbr-recomp-test-a
set -e
dir=$1
[ -n "$dir" ] && [ "$dir" != rbr-recomp-test ] || { echo "usage: $0 <dir name other than rbr-recomp-test>" >&2; exit 1; }
ssh -o BatchMode=yes cachy bash -s <<SH
set -e
mkdir -p ~/$dir && cd ~/$dir
for l in toolchain original .venv; do [ -e \$l ] || ln -s ~/rbr-recomp-test/\$l \$l; done
ls -la
SH
# The per-module static D3DX libs live in the shared toolchain every remote dir links
# above (see notes/toolchain.md); upload them when this checkout carries them so a new
# remote dir never needs manual steps. toolchain/ itself stays untracked.
cd "$(dirname "$0")/.."
for l in d3dx9_4091126.lib; do
  if [ -f "toolchain/msvc71/lib/$l" ]; then rsync -a "toolchain/msvc71/lib/$l" "cachy:~/rbr-recomp-test/toolchain/msvc71/lib/$l"; fi
done
ssh -o BatchMode=yes cachy "test -f ~/rbr-recomp-test/toolchain/msvc71/lib/d3dx9_4091126.lib" || { echo "remote-setup.sh: d3dx9_4091126.lib missing on cachy; copy toolchain/sdk/dx9-4.09.00.1126/Lib/d3dx9.lib to toolchain/msvc71/lib/d3dx9_4091126.lib (see notes/toolchain.md)" >&2; exit 1; }
