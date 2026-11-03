#!/usr/bin/env bash
# Capture or verify the refactor baseline:
#   scripts/refactor-verify.sh init   - write baseline files under target/refactor-baseline/
#   scripts/refactor-verify.sh check  - compare the current tree with that baseline
#
# The baseline consists of the normalized `cargo test -- --list` output, the number
# of `cargo check` warnings, the clippy warning summary and the pre-existing deny
# error fingerprint.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="$ROOT/target/refactor-baseline"
MODE="${1:-check}"
EXPECTED_TESTS=701

mkdir -p "$BASE"
cd "$ROOT"

tests_list() {
    cargo test -- --list 2>&1 | tr -d '\r' > "$BASE/tests-list.raw.txt"
    grep -E '^[^ ].*: test$' "$BASE/tests-list.raw.txt" | sort > "$BASE/tests-list.txt"
    wc -l < "$BASE/tests-list.txt" | tr -d ' \r'
}

run_check() {
    cargo check --all-targets > "$BASE/check.log" 2>&1
    echo $? > "$BASE/check.exit"
    grep -c '^warning' "$BASE/check.log" | tr -d '\r'
}

run_clippy() {
    cargo clippy --all-targets > "$BASE/clippy.log" 2>&1
    echo $? > "$BASE/clippy.exit"
    grep -E 'generated [0-9]+ warnings' "$BASE/clippy.log" \
        | sed 's/^warning: //' | tr -d '\r' > "$BASE/clippy-summary.txt"
    grep -c '^error: this looks like you are trying to swap' "$BASE/clippy.log" | tr -d '\r'
}

normalize_clippy_warnings() {
    grep '^warning: ' "$1" | grep -vE 'generated [0-9]+ warnings' \
        | LC_ALL=C sort > "$2"
}

case "$MODE" in
    init)
        n_tests="$(tests_list)"
        n_check="$(run_check)"
        n_deny="$(run_clippy)"
        printf '%s\n' "$n_tests" > "$BASE/tests-count-before.txt"
        cp "$BASE/tests-list.txt" "$BASE/tests-before.txt"
        cp "$BASE/check.log" "$BASE/check-before.log"
        cp "$BASE/clippy.log" "$BASE/clippy-before.log"
        cp "$BASE/clippy-summary.txt" "$BASE/clippy-summary-before.txt"
        normalize_clippy_warnings "$BASE/clippy.log" "$BASE/clippy-warnings-before.txt"
        printf '%s\n' "$n_check" > "$BASE/check-warnings-before.txt"
        printf '%s\n' "$n_deny" > "$BASE/clippy-deny-errors-before.txt"
        printf 'baseline: tests=%s check_warnings=%s clippy_deny_errors=%s\n' \
            "$n_tests" "$n_check" "$n_deny"
        if [ "$n_tests" != "$EXPECTED_TESTS" ]; then
            printf 'FAIL baseline test count %s (expected %s)\n' "$n_tests" "$EXPECTED_TESTS" >&2
            exit 1
        fi
        ;;
    check)
        status=0
        n_tests="$(tests_list)"
        n_check="$(run_check)"
        n_deny="$(run_clippy)"

        if diff -u "$BASE/tests-before.txt" "$BASE/tests-list.txt" > "$BASE/tests-diff.txt"; then
            printf 'OK   test list: %s tests, identical\n' "$n_tests"
        else
            printf 'FAIL test list changed; see %s\n' "$BASE/tests-diff.txt"
            status=1
        fi

        want_check="$(cat "$BASE/check-warnings-before.txt")"
        if [ "$n_check" = "$want_check" ]; then
            printf 'OK   cargo check warnings: %s\n' "$n_check"
        else
            printf 'FAIL cargo check warnings: %s (baseline %s); see %s\n' \
                "$n_check" "$want_check" "$BASE/check.log"
            status=1
        fi

        normalize_clippy_warnings "$BASE/clippy.log" "$BASE/clippy-warnings.txt"
        comm -13 "$BASE/clippy-warnings-before.txt" "$BASE/clippy-warnings.txt" \
            > "$BASE/clippy-warnings-added.txt"
        comm -23 "$BASE/clippy-warnings-before.txt" "$BASE/clippy-warnings.txt" \
            > "$BASE/clippy-warnings-removed.txt"
        bin_now="$(grep 'bin "vibecodium") generated' "$BASE/clippy-summary.txt" | grep -oE 'generated [0-9]+' | grep -oE '[0-9]+')"
        test_now="$(grep 'test) generated' "$BASE/clippy-summary.txt" | grep -oE 'generated [0-9]+' | grep -oE '[0-9]+')"
        bin_base="$(grep 'bin "vibecodium") generated' "$BASE/clippy-summary-before.txt" | grep -oE 'generated [0-9]+' | grep -oE '[0-9]+')"
        test_base="$(grep 'test) generated' "$BASE/clippy-summary-before.txt" | grep -oE 'generated [0-9]+' | grep -oE '[0-9]+')"
        if [ "$bin_now" -le "$bin_base" ] && [ "$test_now" -le "$test_base" ]; then
            printf 'OK   clippy warnings: bin %s/%s, test %s/%s (decrease allowed)\n' \
                "$bin_now" "$bin_base" "$test_now" "$test_base"
        else
            printf 'FAIL clippy warnings: bin %s/%s, test %s/%s\n' \
                "$bin_now" "$bin_base" "$test_now" "$test_base"
            status=1
        fi
        printf '     clippy summary: %s\n' "$(tr '\n' ';' < "$BASE/clippy-summary.txt")"

        want_deny="$(cat "$BASE/clippy-deny-errors-before.txt")"
        if [ "$n_deny" = "$want_deny" ] && grep -q 'you are trying to swap' "$BASE/clippy.log"; then
            printf 'OK   clippy deny errors: %s (pre-existing almost_swapped)\n' "$n_deny"
        else
            printf 'FAIL clippy deny errors: %s (baseline %s); see %s\n' \
                "$n_deny" "$want_deny" "$BASE/clippy.log"
            status=1
        fi

        exit "$status"
        ;;
    *)
        echo "usage: $0 [init|check]" >&2
        exit 2
        ;;
esac
