#!/bin/bash
# The Rust checks a report or merge needs, in one run: fmt --check, then clippy -D warnings, the tests and the vector
# targets against the record store (scripts/rust-vectors.sh) for the crates this change touches and the crates that
# depend on them. Niced, -j 4. Prints one line per check and a final PASS/FAIL; the full output of a
# failing check is in /tmp/rust-check-<pid>-<name>.txt (per run, so parallel runs by several workers don't overwrite
# each other).
# The change is HEAD against its merge base with integration/handoff, or main when there is no integration branch (on
# that branch itself: against HEAD^1, the merge's first parent), or against RUST_CHECK_BASE. A change to Cargo.toml or Cargo.lock checks every crate.
# The vector step runs the store modules of the changed rbr-physics module dirs (every module when other rbr-physics
# files or a dir with no store of its own change); RUST_CHECK_VECTORS="car_body tyre" names them instead, "none" skips
# them. A base, diff or cargo tree that fails is a FAIL, not an empty change.
# RUST_CHECK_ALL=1 checks every crate and every vector module (the daily full run).
# The tests read RBR_DATA, RBR_PATCH, RBR_RECORDINGS and the others in docs/development.md "Set up"; unset, the tests
# that need them skip, and the first line names them. Without RBR_VECTOR_STORE the vector step is skipped.
# When rbr-render is checked, input_check replays every RSF session in $RBR_RECORDINGS/rsf that has a profile and car
# control calls (notes/verification.md, "Input chain check").
# Usage: scripts/rust-check.sh
set -uo pipefail
cd "$(dirname "$0")/.."
fail=0

store=${RBR_VECTOR_STORE:-}

check() {
    local name=$1
    shift
    if nice -n 10 "$@" > "/tmp/rust-check-$$-$name.txt" 2>&1; then
        echo "$name: ok"
        rm -f "/tmp/rust-check-$$-$name.txt"
    else
        echo "$name: FAIL (/tmp/rust-check-$$-$name.txt)"
        fail=1
    fi
}

if [ -z "${RUST_CHECK_BASE:-}" ]; then
    for ref in integration/handoff origin/integration/handoff main origin/main; do
        git rev-parse -q --verify "$ref^{commit}" >/dev/null && break
    done
    RUST_CHECK_BASE=$(git merge-base HEAD "$ref") ||
        { echo "rust-check: FAIL (no merge base with $ref; set RUST_CHECK_BASE)"; exit 1; }
fi
base=$RUST_CHECK_BASE
[ "$base" = "$(git rev-parse HEAD)" ] && base=$(git rev-parse HEAD^1)
changed=$(git diff --name-only "$base" -- Cargo.toml Cargo.lock crates) ||
    { echo "rust-check: FAIL (git diff against $base)"; exit 1; }
all=${RUST_CHECK_ALL:-0}
# The workspace manifest, or a lock change no crate manifest explains (a dependency update), touches every crate.
grep -qx 'Cargo.toml' <<<"$changed" && all=1
grep -qx 'Cargo.lock' <<<"$changed" && ! grep -qE '^crates/rbr-[a-z-]+/Cargo\.toml$' <<<"$changed" && all=1

crates=()
if [ "$all" = 1 ]; then
    crates=(--workspace)
else
    dependents=""
    for crate in $(sed -n 's|^crates/\(rbr-[a-z-]*\)/.*|\1|p' <<<"$changed" | sort -u); do
        tree=$(cargo tree -q -i "$crate" --workspace -e normal,dev --prefix none --format '{p}') ||
            { echo "rust-check: FAIL (cargo tree -i $crate)"; exit 1; }
        dependents+=$'\n'$tree
    done
    for crate in $(cut -d' ' -f1 <<<"$dependents" | grep '^rbr-' | sort -u); do crates+=(-p "$crate"); done
fi

if [ -n "${RUST_CHECK_VECTORS:-}" ]; then
    vectors=$RUST_CHECK_VECTORS
elif [ "$all" = 1 ]; then
    vectors=""
elif ! grep -q '^crates/rbr-physics/' <<<"$changed"; then
    vectors=none
elif grep -qE '^crates/rbr-physics/(Cargo\.toml|src/[a-z_]+\.rs|tests/[a-z_]+/)' <<<"$changed"; then
    vectors=""
else
    # A module dir's name is its store dir; a vector test names its store dir by the RBR_*_VECTORS it reads. A dir
    # with no store of its own (math, mem, store, ngp, ...) is shared by the modules: every module runs.
    stores() { grep -ohE '"RBR_[A-Z0-9_]+_VECTORS"' "$@" | tr -d '"' | sed 's/^RBR_//; s/_VECTORS$//' |
        tr A-Z a-z | sed 's/^ent$/entities/' | sort -u; }
    known=$(stores crates/rbr-physics/tests/*.rs)
    dirs=$(sed -n 's|^crates/rbr-physics/src/\([a-z_]*\)/.*|\1|p' <<<"$changed" | sort -u)
    tests=$(grep -E '^crates/rbr-physics/tests/[a-z_]+\.rs$' <<<"$changed" | while read -r t; do
        [ -f "$t" ] && echo "$t"; done)
    if [ -n "$(comm -23 <(echo "$dirs" | sed '/^$/d') <(echo "$known"))" ]; then
        vectors=""
    else
        # shellcheck disable=SC2086 # a list of test files
        vectors=$( (echo "$dirs"; [ -z "$tests" ] || stores $tests) | sed '/^$/d' | sort -u | tr '\n' ' ')
        [ -n "$vectors" ] || vectors=none
    fi
fi

if [ "$vectors" != none ] && [ -z "$store" ]; then
    vectors=none
    echo "vectors: skipped (RBR_VECTOR_STORE unset)"
fi
unset_vars=$(for v in RBR_DATA RBR_PATCH RBR_RECORDINGS RBR_NGP_PHYSICS RBR_NGP RBR_RSF RBR_SAVES; do
    [ -n "${!v:-}" ] || echo "$v"; done | paste -sd' ' -)
[ -z "$unset_vars" ] || echo "unset, their tests skip: $unset_vars"
echo "scope: ${crates[*]:-no crates} against $(git rev-parse --short "$base"); vectors: ${vectors:-all}"
check fmt cargo fmt --all --check
if [ ${#crates[@]} -gt 0 ]; then
    check clippy cargo clippy -j 4 "${crates[@]}" --all-targets -q -- -D warnings
    check test cargo test -j 4 "${crates[@]}" -q
fi
input_sessions() {
    cargo build -j 4 --release -q -p rbr-render --bin input_check || return 1
    local session status
    for session in "$RBR_RECORDINGS"/rsf/*/; do
        target/release/input_check "$session"
        status=$?
        [ $status = 0 ] || [ $status = 3 ] || return 1
    done
}
if [ -d "${RBR_RECORDINGS:-}/rsf" ] && { [ "$all" = 1 ] || [[ " ${crates[*]} " == *" rbr-render "* ]]; }; then
    export -f input_sessions
    check input_check bash -c input_sessions
fi
if [ "$vectors" != none ]; then
    # shellcheck disable=SC2086 # a list of module names
    check vectors scripts/rust-vectors.sh $vectors
fi

[ $fail = 0 ] && echo "rust-check: PASS" || echo "rust-check: FAIL"
exit $fail
