#!/usr/bin/env bash
# Proves that every example lints with no finding (issue #1104): runs `netsmith project lint --strict`
# over every board of examples/, with KiCad hidden (the linter reads the sources and the catalogs and
# needs no KiCad). `--strict` makes a finding an error, so the run fails on one and prints it.
#
# The examples are found by folder, as scripts/check-example-locks.sh finds them:
#   - each `*.nsm` file of examples/ that is not a `*.physical.nsm` file is a board's source, linted
#     as `netsmith project lint --strict <file>.nsm` from examples/ (with its physical file);
#   - each folder of examples/ with a netsmith.toml is a project, linted as
#     `netsmith project lint --strict` from that folder (every file of the project).
# A finding that is to stay is turned off where it is (`// lint: off <rule>` on its line, or
# `[lint] off` in the project's netsmith.toml), so the exception is listed beside the source.
# It also fails when it finds no board at all (a changed folder layout would make the globs match
# nothing and pass for nothing). scripts/check-example-lint-test runs the script over a fake
# examples folder with a stub binary; NETSMITH_EXAMPLES_DIR names that folder (default: examples/
# of the repository).
#
# Usage: scripts/check-example-lint.sh [path/to/netsmith]
# Without an argument it builds the debug binary with `cargo build -p netsmith-cli` first. Nothing is
# written: the linter never writes.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"

if [ "$#" -ge 1 ]; then
    bin="$(realpath "$1")"
else
    (cd "$repo" && cargo build --quiet -p netsmith-cli)
    bin="$repo/target/debug/netsmith"
fi
[ -x "$bin" ] || { echo "error: $bin is not an executable (build it, or name the binary)" >&2; exit 2; }

examples="${NETSMITH_EXAMPLES_DIR:-$repo/examples}"
[ -d "$examples" ] || { echo "error: $examples is not a folder" >&2; exit 2; }
examples="$(cd "$examples" && pwd)"

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

failed=0
runs=0

# check <label> <folder> <netsmith arguments...>: one `project lint --strict` that must exit 0.
check() {
    local label="$1" dir="$2"
    shift 2
    runs=$((runs + 1))
    local code=0
    (cd "$dir" && NETSMITH_NO_KICAD=1 "$bin" project lint --strict "$@") > "$log" 2>&1 || code=$?
    if [ "$code" -eq 0 ]; then
        echo "ok    $label"
        return
    fi
    failed=1
    if [ "$code" -eq 1 ]; then
        echo "FAIL  $label: the linter has findings"
    else
        echo "FAIL  $label: the linter could not run (exit $code)"
    fi
    sed 's/^/      /' "$log"
}

for file in "$examples"/*.nsm; do
    [ -e "$file" ] || continue
    case "$file" in *.physical.nsm) continue ;; esac
    check "examples/$(basename "$file")" "$examples" "$(basename "$file")"
done
for manifest in "$examples"/*/netsmith.toml; do
    [ -e "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    check "examples/$(basename "$dir")" "$dir"
done

if [ "$runs" -eq 0 ]; then
    echo "FAIL  found no board in $examples: no *.nsm file and no folder with a netsmith.toml, so there is nothing to lint (did the examples move?)"
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    cat >&2 <<'MSG'

check-example-lint: FAILED
An example has a finding of `netsmith project lint` (an unused import or definition, a redundant
statement, a name against the guide), or could not be linted (it does not check). Change the source
as the finding says. A finding that is to stay is turned off where it is, with `// lint: off <rule>`
on its line (or alone on the line above it), or for a project with `[lint] off = ["<rule>"]` in its
netsmith.toml. To see the findings, run `netsmith project lint` in the example's folder.
MSG
    exit 1
fi
echo "check-example-lint: $runs boards and projects, no finding"
