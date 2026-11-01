#!/bin/bash
# Runs every rbr-physics vector test against the record store (copies of the
# original-call records in /tmp; see the "Vector store" note in notes/physics.md "x87 and float rules").
# Usage: scripts/rust-vectors.sh [module...]
#   With modules (store subdir names, e.g. car_body), only their targets run: use that for
#   your own module; the full run is the merge check. The store root is $RBR_VECTOR_STORE (required),
#   one subdir per module.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/dev-env.sh
store=${RBR_VECTOR_STORE:?set RBR_VECTOR_STORE to the vector store (docs/development.md "Set up")}
only=" $* "
fail=0

# run_target <env var> <store subdir> <test target>...
run_target() {
    local var=$1 dir=$2
    shift 2
    local files=("$store/$dir"/*)
    if [ ! -e "${files[0]}" ]; then
        echo "$*: no records (empty $store/$dir)"
        fail=1
        return
    fi
    local paths tests=()
    paths=$(IFS=:; echo "${files[*]}")
    for t in "$@"; do tests+=(--test "$t"); done
    local out
    if out=$(env "$var=$paths" cargo test -q \
        -p rbr-physics "${tests[@]}" -- --nocapture 2>&1); then
        if grep -qE "skipping|no calls" <<<"$out"; then
            echo "$*: PASS with skips (some calls unrecorded)"
            grep -E "skipping|no calls" <<<"$out" | sort -u
        else
            echo "$*: PASS"
        fi
    else
        echo "$*: FAIL"
        grep -A2 'panicked at' <<<"$out" | cut -c1-300
        tail -n 15 <<<"$out"
        fail=1
    fi
}

# Every test target that reads an RBR_*_VECTORS variable runs, so a new target can't be skipped
# unnoticed; the store subdir is the variable's module (RBR_ENT_VECTORS is entities).
for var in $(grep -ohE '"RBR_[A-Z0-9_]+_VECTORS"' crates/rbr-physics/tests/*.rs | tr -d '"' | sort -u); do
    dir=$(tr A-Z a-z <<<"${var#RBR_}"); dir=${dir%_vectors}; [ "$dir" = ent ] && dir=entities
    [ $# = 0 ] || [[ "$only" == *" $dir "* ]] || continue
    # shellcheck disable=SC2046
    run_target "$var" "$dir" $(grep -lF "\"$var\"" crates/rbr-physics/tests/*.rs | xargs -n1 basename | sed 's/\.rs$//')
done

# A module name no vector variable has ran nothing: a FAIL, not a silent pass.
known=$(grep -ohE '"RBR_[A-Z0-9_]+_VECTORS"' crates/rbr-physics/tests/*.rs | tr -d '"' | sed 's/^RBR_//; s/_VECTORS$//' |
    tr A-Z a-z | sed 's/^ent$/entities/')
for m in "$@"; do
    grep -qx "$m" <<<"$known" || { echo "$m: FAIL (no vector module of that name)"; fail=1; }
done

[ $# = 0 ] || exit $fail

# The rest needs no records (spring-damper vectors are committed in tests/vectors/).
if out=$(env -u RBR_VCU_VECTORS -u RBR_TYRE_VECTORS -u RBR_ENT_VECTORS \
    -u RBR_PHYSICS_CORE_VECTORS -u RBR_RIGID_BODY_VECTORS -u RBR_CAR_BODY_VECTORS \
    -u RBR_ENGINE_DRIVELINE_VECTORS cargo test -q \
    -p rbr-physics --lib --test spring_damper_vectors 2>&1); then
    echo "lib + spring_damper_vectors: PASS"
else
    echo "lib + spring_damper_vectors: FAIL"
    tail -n 15 <<<"$out"
    fail=1
fi

exit $fail
