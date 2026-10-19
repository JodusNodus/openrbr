#!/bin/bash
# Creates a work directory on the test machine (RBR_REMOTE_HOST) that shares toolchain/, original/ and .venv with the
# base ~/rbr-recomp-test there (notes/verification.md). Use it via `export RBR_REMOTE=<dir>`: the x87 oracle of
# harness/replay.py and the plugin build (harness/plugin/build.sh) work in it.
# Usage: scripts/remote-setup.sh <dir name>, e.g. scripts/remote-setup.sh rbr-recomp-test-a
set -e
host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
dir=$1
[ -n "$dir" ] && [ "$dir" != rbr-recomp-test ] || { echo "usage: $0 <dir name other than rbr-recomp-test>" >&2; exit 1; }
ssh -o BatchMode=yes "$host" bash -s <<SH
set -e
mkdir -p ~/$dir && cd ~/$dir
for l in toolchain original .venv; do [ -e \$l ] || ln -s ~/rbr-recomp-test/\$l \$l; done
ls -la
SH
