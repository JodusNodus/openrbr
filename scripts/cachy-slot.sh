#!/bin/bash
# Runs a command in one of RBR_BUILD_SLOTS (default 4) build slots, so parallel builds and
# reccmp runs don't starve the harness games on cachy. Waits for a free slot.
# Usage: scripts/cachy-slot.sh <command...>
for ((t = 0; t < 7200; t += 3)); do
  for ((i = 0; i < ${RBR_BUILD_SLOTS:-4}; i++)); do
    # lockf -t 0 exits 75 when the slot is busy; anything else is the command's own status.
    rc=0; lockf -t 0 "/tmp/rbr-build-s$i.lock" "$@" || rc=$?
    [ $rc = 75 ] || exit $rc
  done
  sleep 3
done
echo "cachy-slot.sh: no free build slot in 2 h" >&2; exit 1
