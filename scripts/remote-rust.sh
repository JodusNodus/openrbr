#!/bin/bash
# Runs one of this worktree's Rust binaries on the test machine (RBR_REMOTE_HOST) instead of this one: copies the
# workspace (Cargo.toml, Cargo.lock, crates/) to $RBR_REMOTE_RUST/src/<worktree>/ there, builds the binary (release,
# its own target/) and runs it with RBR_DATA/RBR_PATCH set to $RBR_REMOTE_RUST/data/retail-install and
# data/Main_group. RBR_REMOTE_RUST is a directory relative to the remote home. It runs in the remote home, so paths in the arguments are relative to it:
# the drive recordings are in rbr-harness-runs/.
# Usage: scripts/remote-rust.sh BIN [args]   e.g. scripts/remote-rust.sh rbr-standalone --boot --recording
#        rbr-harness-runs/drive-v2-gravel-zr-25.bin --stage track-25_O --steps 2844
# When the worktree is removed: ssh $RBR_REMOTE_HOST rm -rf $RBR_REMOTE_RUST/src/<worktree>
set -euo pipefail
host=${RBR_REMOTE_HOST:?set RBR_REMOTE_HOST to the ssh host of the test machine (notes/verification.md)}
dir=${RBR_REMOTE_RUST:?set RBR_REMOTE_RUST to the Rust directory on the test machine (notes/verification.md)}
root=$(cd "$(dirname "$0")/.." && pwd)
name=$(basename "$root")
bin=$1
shift
rsync -a --delete --exclude target "$root/Cargo.toml" "$root/Cargo.lock" "$root/crates" "$host:$dir/src/$name/"
ssh "$host" "nice -n 10 ~/.cargo/bin/cargo build -q --release -j 4 --manifest-path ~/$dir/src/$name/Cargo.toml --bin $bin &&
  RBR_DATA=~/$dir/data/retail-install RBR_PATCH=~/$dir/data/Main_group nice -n 10 ~/$dir/src/$name/target/release/$bin $(printf '%q ' "$@")"
