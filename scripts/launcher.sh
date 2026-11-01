#!/bin/bash
# Builds the viewer, rbr-standalone and the launcher (release) and starts the launcher (crates/rbr-launcher).
# When they exist, RBR_DATA defaults to the retail install in the main checkout (original/ is not in git, so worktrees
# lack it), RBR_PATCH to its 1.02 patch files (the launcher always starts from the game's boot weather),
# RBR_NGP_PHYSICS to original/ngp/physics.rbz (NGP 7.4.778's archive with its own cars) and RBR_RSF to original/rsf (a
# RallySimFans folder).
# These override the install folder of the launcher's Settings page (docs/development.md). Arguments go to the launcher.
# Usage: scripts/launcher.sh [--data DIR] [--viewer PATH] [--standalone PATH] [--list]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/dev-env.sh"
root=$(cd "$(dirname "$0")/.." && pwd)
nice -n 10 cargo build -j 4 --release --manifest-path "$root/Cargo.toml"
main=$(dirname "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)")
[ -d "$main/original/retail-1.0/install" ] && export RBR_DATA=${RBR_DATA:-$main/original/retail-1.0/install}
[ -d "$main/original/patch-1.02/files/Main_group" ] && export RBR_PATCH=${RBR_PATCH:-$main/original/patch-1.02/files/Main_group}
[ -f "$main/original/ngp/7.4.778/Plugins/NGP.dll" ] && export RBR_NGP_DLL=${RBR_NGP_DLL:-$main/original/ngp/7.4.778/Plugins/NGP.dll}
[ -f "$main/original/ngp/physics.rbz" ] && export RBR_NGP_PHYSICS=${RBR_NGP_PHYSICS:-$main/original/ngp/physics.rbz}
[ -d "$main/original/rsf" ] && export RBR_RSF=${RBR_RSF:-$main/original/rsf}
exec "$root/target/release/rerbr" "$@"
