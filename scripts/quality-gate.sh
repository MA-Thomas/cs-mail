#!/usr/bin/env bash
# Local quality gate for cs-mail. Run from anywhere: scripts/quality-gate.sh
#
# Every check runs and reports on its own line; no check hides another. The gate
# fails if any check fails. PostgreSQL suites require CS_MAIL_TEST_DATABASE_URL
# pointing at an isolated test database. Skipping them requires --skip-postgres
# and is reported as SKIPPED, because ignored tests are not evidence.
#
# Compatible with the bash 3.2 that ships with macOS.

set -uo pipefail

usage() {
    echo "usage: scripts/quality-gate.sh [--skip-postgres]"
    echo
    echo "Checks: identity source manifest, dependency direction, rustfmt, clippy,"
    echo "workspace tests, PostgreSQL suites, and application binaries."
    echo "Set CS_MAIL_TEST_DATABASE_URL to an isolated database for the PostgreSQL suites."
}

skip_postgres=0
for arg in "$@"; do
    case "$arg" in
        --skip-postgres) skip_postgres=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; usage >&2; exit 2 ;;
    esac
done

cd "$(dirname "$0")/.." || exit 2

failed=0
summary=""

record() { # status name
    summary="${summary}$(printf '  %-8s %s' "$1" "$2")"$'\n'
    if [ "$1" = "FAIL" ]; then failed=1; fi
}

run_check() { # name command...
    local name="$1"
    shift
    echo
    echo "==> $name"
    if "$@"; then
        record PASS "$name"
    else
        record FAIL "$name"
    fi
}

# The reviewed identity-model sources in the sibling checkout match the manifest.
check_identity_sources() {
    if [ ! -d ../identity-model ]; then
        echo "missing sibling checkout ../identity-model"
        return 1
    fi
    local output
    if command -v sha256sum >/dev/null 2>&1; then
        output="$(sha256sum --check identity-source.sha256 2>&1)"
    else
        output="$(shasum -a 256 --check identity-source.sha256 2>&1)"
    fi
    local status=$?
    printf '%s\n' "$output" | grep -v ': OK$'
    if [ $status -eq 0 ]; then
        echo "all $(grep -c . identity-source.sha256) pinned files match"
    fi
    return $status
}

# Dependencies point inward: no workspace package may depend on an application.
check_dependency_direction() {
    local status=0 manifest package dependents
    for manifest in apps/*/Cargo.toml; do
        [ -f "$manifest" ] || continue
        package="$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -n 1)"
        dependents="$(cargo tree --workspace --invert "$package" --edges normal,build,dev \
            --prefix none --depth 1 2>/dev/null | sed '1d' | sed '/^$/d')"
        if [ -n "$dependents" ]; then
            echo "$package is depended on by:"
            printf '%s\n' "$dependents" | sed 's/^/    /'
            status=1
        else
            echo "$package: no dependents"
        fi
    done
    return $status
}

check_postgres() {
    if [ -z "${CS_MAIL_TEST_DATABASE_URL:-}" ]; then
        echo "CS_MAIL_TEST_DATABASE_URL is not set; use --skip-postgres to skip explicitly"
        return 1
    fi
    cargo test --workspace -- --ignored
}

# Each application binary builds and reports its version.
check_binaries() {
    local status=0 manifest package
    for manifest in apps/*/Cargo.toml; do
        [ -f "$manifest" ] || continue
        package="$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -n 1)"
        if ! cargo run --quiet --package "$package" -- --version; then
            status=1
        fi
    done
    return $status
}

run_check "identity source manifest" check_identity_sources
run_check "dependency direction" check_dependency_direction
run_check "rustfmt" cargo fmt --all -- --check
run_check "clippy" cargo clippy --workspace --all-targets -- -D warnings
run_check "workspace tests" cargo test --workspace
if [ $skip_postgres -eq 1 ]; then
    record SKIPPED "PostgreSQL suites (not evidence)"
else
    run_check "PostgreSQL suites" check_postgres
fi
run_check "application binaries" check_binaries

echo
echo "Quality gate summary:"
printf '%s' "$summary"
if [ $failed -ne 0 ]; then
    echo "Quality gate FAILED"
    exit 1
fi
echo "Quality gate passed"
