#!/usr/bin/env bash
# Proves that every example's pick lock is current (ADR 0100): runs `netsmith lock --check` over
# every board of examples/, with KiCad hidden (the lock is the solver's picks and needs no KiCad).
# It fails when a lock is stale (a catalog, `std/` or the solver would now pick another row) or
# missing (a new example with no lock), and says how to fix it.
#
# The examples are found by folder, as scripts/check-no-kicad.sh finds them:
#   - each `*.nsm` file of examples/ that is not a `*.physical.nsm` file is a board's source, and
#     is checked as `netsmith lock --check <file>.nsm` from examples/;
#   - each folder of examples/ with a netsmith.toml is a project, checked as `netsmith lock --check`
#     from that folder (the project's boards, as the manifest lists them).
# A board whose parts are all concrete still has a lock, an empty one: `lock --update` writes it.
# It also fails when it finds no board at all (a changed folder layout would make the globs match
# nothing and pass for nothing), and for an orphan lock: a `*.lock.toml` anywhere under examples/
# whose source `<stem>.nsm` is not beside it (the board was renamed or removed; delete the lock).
# scripts/check-example-locks-test runs the script over a fake examples folder with a stub binary;
# NETSMITH_EXAMPLES_DIR names that folder (default: examples/ of the repository).
#
# Usage: scripts/check-example-locks.sh [path/to/netsmith]
# Without an argument it builds the debug binary with `cargo build -p netsmith-cli` first. Nothing is
# written: `lock --check` never writes.
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

# check <label> <folder> <netsmith arguments...>: one `lock --check` that must exit 0.
check() {
    local label="$1" dir="$2"
    shift 2
    runs=$((runs + 1))
    local code=0
    (cd "$dir" && NETSMITH_NO_KICAD=1 "$bin" lock --check "$@") > "$log" 2>&1 || code=$?
    if [ "$code" -eq 0 ]; then
        echo "ok    $label"
        return
    fi
    failed=1
    if [ "$code" -eq 1 ]; then
        echo "FAIL  $label: the pick lock is stale or missing"
    else
        echo "FAIL  $label: lock --check could not tell (exit $code)"
    fi
    sed 's/^/      /' "$log"
}

# The boards that are one file, and the projects.
for file in "$examples"/*.nsm; do
    [ -e "$file" ] || continue
    case "$file" in *.physical.nsm) continue ;; esac
    name="$(basename "$file")"
    check "examples/$name" "$examples" "$name"
done
for manifest in "$examples"/*/netsmith.toml; do
    [ -e "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    check "examples/$(basename "$dir")" "$dir"
done

if [ "$runs" -eq 0 ]; then
    echo "FAIL  found no board in $examples: no *.nsm file and no folder with a netsmith.toml, so there is nothing to check (did the examples move?)"
    failed=1
fi

# A lock whose board source is gone: `<dir>/<stem>.lock.toml` belongs to `<dir>/<stem>.nsm`.
while IFS= read -r lock; do
    [ -n "$lock" ] || continue
    if [ ! -f "${lock%.lock.toml}.nsm" ]; then
        echo "FAIL  ${lock#"$examples"/}: an orphan pick lock: ${lock%.lock.toml}.nsm is not there (delete the lock, or restore the source)"
        failed=1
    fi
done < <(find "$examples" -name '*.lock.toml' | sort)

if [ "$failed" -ne 0 ]; then
    cat >&2 <<'MSG'

check-example-locks: FAILED
An example's pick lock (<stem>.lock.toml, beside its source) does not match what the solver picks
now: a catalog, std/ or the solver changed a pick, or an example has no lock yet. A pick that moves
is a change to the board, so a person reviews it:
  1. in the example's folder (examples/ for a one-file board), run
         netsmith lock --update [<file.nsm>]
     (it prints each part that changed, the old row and the new one);
  2. read the diff of the lock file, and commit it with the change that caused it.
A lock named as an orphan has no source beside it any more: delete it. No board found means the
examples are not where this script looks for them.
MSG
    exit 1
fi
echo "check-example-locks: $runs boards, every pick lock is current"
