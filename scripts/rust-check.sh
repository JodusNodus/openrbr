#!/bin/bash
# The Rust checks a report or merge needs, in one run: fmt --check, then clippy -D warnings, the tests and the vector
# targets against the record store (scripts/rust-vectors.sh) for the crates this change touches and the crates that
# depend on them. Niced, -j 4 (Mac load rule). Prints one line per check and a final PASS/FAIL; the full output of a
# failing check is in /tmp/rust-check-<pid>-<name>.txt (per run, so parallel runs by several workers don't overwrite
# each other).
# The change is HEAD against its merge base with integration/handoff (on integration itself: against HEAD^1, the
# merge's first parent), or against RUST_CHECK_BASE. A change to rust/Cargo.toml or rust/Cargo.lock checks every crate.
# The vector step runs the store modules of the changed rbr-physics module dirs (every module when other rbr-physics
# files change); RUST_CHECK_VECTORS="car_body tyre" names them instead, "none" skips them.
# RUST_CHECK_ALL=1 checks every crate and every vector module (the daily full run).
# The tests read RBR_DATA, RBR_PATCH and RBR_RECORDINGS (README); unset, the tests that need them skip.
# Usage: scripts/rust-check.sh
set -uo pipefail
cd "$(dirname "$0")/.."
fail=0

# The gameplay filter's and marker ticker's records live in the vector store too; their tests read them when
# RBR_FILTER_VECTORS / RBR_MARKER_VECTORS name them.
store=${RBR_VECTOR_STORE:-/Users/thomas/worktrees/rbr-recomp/vectors}
if [ -z "${RBR_FILTER_VECTORS:-}" ] && ls "$store"/gameplay_filter/*.txt >/dev/null 2>&1; then
    RBR_FILTER_VECTORS=$(ls "$store"/gameplay_filter/*.txt | paste -sd: -)
    export RBR_FILTER_VECTORS
fi
if [ -z "${RBR_MARKER_VECTORS:-}" ] && ls "$store"/gameplay_markers/*.txt >/dev/null 2>&1; then
    RBR_MARKER_VECTORS=$(ls "$store"/gameplay_markers/*.txt | paste -sd: -)
    export RBR_MARKER_VECTORS
fi

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

base=${RUST_CHECK_BASE:-$(git merge-base HEAD integration/handoff)}
[ "$base" = "$(git rev-parse HEAD)" ] && base=$(git rev-parse HEAD^1)
changed=$(git diff --name-only "$base" -- rust)
all=${RUST_CHECK_ALL:-0}
# The workspace manifest, or a lock change no crate manifest explains (a dependency update), touches every crate.
grep -qx 'rust/Cargo.toml' <<<"$changed" && all=1
grep -qx 'rust/Cargo.lock' <<<"$changed" && ! grep -qE '^rust/rbr-[a-z-]+/Cargo\.toml$' <<<"$changed" && all=1

crates=()
if [ "$all" = 1 ]; then
    crates=(--workspace)
else
    for crate in $(sed -n 's|^rust/\(rbr-[a-z-]*\)/.*|\1|p' <<<"$changed" | sort -u); do
        cargo tree --manifest-path rust/Cargo.toml -q -i "$crate" --workspace -e normal,dev --prefix none \
            --format '{p}' | cut -d' ' -f1
    done | grep '^rbr-' | sort -u > "/tmp/rust-check-$$-crates.txt"
    while read -r crate; do crates+=(-p "$crate"); done < "/tmp/rust-check-$$-crates.txt"
    rm -f "/tmp/rust-check-$$-crates.txt"
fi

if [ -n "${RUST_CHECK_VECTORS:-}" ]; then
    vectors=$RUST_CHECK_VECTORS
elif [ "$all" = 1 ]; then
    vectors=""
elif ! grep -q '^rust/rbr-physics/' <<<"$changed"; then
    vectors=none
elif grep -qE '^rust/rbr-physics/(Cargo\.toml|src/[a-z_]+\.rs|tests/[a-z_]+/)' <<<"$changed"; then
    vectors=""
else
    # A module dir's name is its store dir; a vector test names its store dir by the RBR_*_VECTORS it reads.
    vectors=$( (sed -n 's|^rust/rbr-physics/src/\([a-z_]*\)/.*|\1|p' <<<"$changed"
        for t in $(grep -E '^rust/rbr-physics/tests/[a-z_]+\.rs$' <<<"$changed"); do
            [ -f "$t" ] && grep -ohE '"RBR_[A-Z0-9_]+_VECTORS"' "$t" | tr -d '"' | sed 's/^RBR_//; s/_VECTORS$//' |
                tr A-Z a-z | sed 's/^ent$/entities/'
        done) | sort -u | tr '\n' ' ')
    [ -n "$vectors" ] || vectors=none
fi

echo "scope: ${crates[*]:-no crates} against $(git rev-parse --short "$base"); vectors: ${vectors:-all}"
check fmt cargo fmt --manifest-path rust/Cargo.toml --all --check
if [ ${#crates[@]} -gt 0 ]; then
    check clippy cargo clippy --manifest-path rust/Cargo.toml -j 4 "${crates[@]}" --all-targets -q -- -D warnings
    check test cargo test --manifest-path rust/Cargo.toml -j 4 "${crates[@]}" -q
fi
if [ "$vectors" != none ]; then
    # shellcheck disable=SC2086 # a list of module names
    check vectors scripts/rust-vectors.sh $vectors
fi

[ $fail = 0 ] && echo "rust-check: PASS" || echo "rust-check: FAIL"
exit $fail
