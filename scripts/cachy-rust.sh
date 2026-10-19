#!/bin/bash
# Runs one of this worktree's Rust binaries on cachy (x86_64, 16 cores) instead of the Mac: copies rust/ (without
# target/) to cachy:~/rbr-rust/src/<worktree>/, builds the binary there (release, its own target/) and runs it with
# RBR_DATA/RBR_PATCH set to cachy's copies of the retail data. It runs in the cachy home, so paths in the arguments are
# cachy paths relative to it: the drive recordings are in rbr-harness-runs/.
# Usage: scripts/cachy-rust.sh BIN [args]   e.g. scripts/cachy-rust.sh rbr-standalone --boot --recording
#        rbr-harness-runs/drive-v2-gravel-zr-25.bin --stage track-25_O --steps 2844
# When the worktree is removed: ssh cachy rm -rf rbr-rust/src/<worktree>
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
name=$(basename "$root")
bin=$1
shift
rsync -a --delete --exclude target "$root/rust/" "cachy:rbr-rust/src/$name/"
ssh cachy "nice -n 10 ~/.cargo/bin/cargo build -q --release -j 4 --manifest-path ~/rbr-rust/src/$name/Cargo.toml --bin $bin &&
  RBR_DATA=~/rbr-rust/data/retail-install RBR_PATCH=~/rbr-rust/data/Main_group nice -n 10 ~/rbr-rust/src/$name/target/release/$bin $(printf '%q ' "$@")"
