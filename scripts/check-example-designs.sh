#!/usr/bin/env bash
# Proves that every example's design file is current (issue #609): builds every board of examples/
# with `netsmith build --emit-design`, with KiCad hidden, and compares the designs with the
# committed `<stem>.design.json` beside each board's source. The design is what the design stage
# hands the placer, and the tests of the placer, the router and fab start from these files, so a
# change to the checker, the solver, the design stage, std/ or a catalog that moves a design shows
# up as a diff of the file that a person reviews, as a moved pick does in its lock (ADR 0100).
#
# The examples are found by folder, as scripts/check-example-locks.sh finds them:
#   - each `*.nsm` file of examples/ that is not a `*.physical.nsm` file is a board's source, built
#     as `netsmith build <file>.nsm` from examples/, and its design file is
#     examples/<stem>.design.json;
#   - each folder of examples/ with a netsmith.toml is a project: each file of its `top_level` list
#     is built as `netsmith build <file>` from that folder, and its design file is beside it
#     (`src/main.nsm` has `src/main.design.json`).
# A board that cannot build has no design, and is named in `no_design` below with its reason (it
# then has no design file); a name there that is no longer an example fails, so the list cannot rot.
# It fails when a design file is stale or missing, when a board does not build, when it finds no
# board at all (a changed folder layout would make the globs match nothing and pass for nothing),
# and for an orphan design file: a `*.design.json` anywhere under examples/ whose source
# `<stem>.nsm` is not beside it (the board was renamed or removed; delete the file).
# The build runs with `--locked` and writes its schematic into a temporary folder, so the check
# writes nothing in examples/. scripts/check-example-designs-test runs the script over a fake
# examples folder with a stub binary; NETSMITH_EXAMPLES_DIR names that folder (default: examples/
# of the repository).
#
# Usage: scripts/check-example-designs.sh [--update] [path/to/netsmith]
# Without a binary it builds the debug binary with `cargo build -p netsmith-cli` first. `--update`
# writes each design file that differs or is missing (and prints its name) instead of failing.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"

update=false
if [ "${1:-}" = --update ]; then
    update=true
    shift
fi

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

# The single-file boards of examples/ that cannot build, as "file name|reason": the same as the
# `skipped` list of scripts/check-no-kicad.sh. Keep it short.
no_design=(
    "carrier_and_cards.nsm|its parts are invented and have no KiCad symbol, so it is checked and solved but never built"
)

# The reason `$1` is in `no_design`, or a failure when it is not.
no_design_reason() {
    local entry
    for entry in "${no_design[@]}"; do
        if [ "${entry%%|*}" = "$1" ]; then
            echo "${entry#*|}"
            return 0
        fi
    done
    return 1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
log="$work/log"
new="$work/new.design.json"

failed=0
runs=0

# check <label> <folder> <source, relative to the folder>: build it, and compare its designs with
# the design file beside the source.
check() {
    local label="$1" dir="$2" source="$3"
    local fixture="$dir/${source%.nsm}.design.json"
    runs=$((runs + 1))
    rm -rf "$work/out" "$new"
    local code=0
    (cd "$dir" && NETSMITH_NO_KICAD=1 "$bin" build --locked --notes none --out "$work/out" \
        --emit-design "$new" "$source") > "$log" 2>&1 || code=$?
    if [ "$code" -ne 0 ] || [ ! -f "$new" ]; then
        failed=1
        echo "FAIL  $label: the board does not build (exit $code), so its design cannot be compared"
        sed 's/^/      /' "$log"
        return
    fi
    if cmp -s "$new" "$fixture"; then
        echo "ok    $label"
    elif [ "$update" = true ]; then
        cp "$new" "$fixture"
        echo "wrote ${fixture#"$examples"/}"
    elif [ ! -f "$fixture" ]; then
        failed=1
        echo "FAIL  $label: ${fixture#"$examples"/} is missing"
    else
        failed=1
        echo "FAIL  $label: ${fixture#"$examples"/} is stale; the first lines that differ:"
        diff -u "$fixture" "$new" | sed -n '3,40p' | sed 's/^/      /'
    fi
}

# The files of a project's `top_level` list (one array on one line of its netsmith.toml).
top_level() {
    sed -n 's/^[[:space:]]*top_level[[:space:]]*=[[:space:]]*\[\(.*\)\].*$/\1/p' "$1" \
        | tr ',' '\n' | sed -n 's/^[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p'
}

# The boards that are one file, and the projects.
for file in "$examples"/*.nsm; do
    [ -e "$file" ] || continue
    case "$file" in *.physical.nsm) continue ;; esac
    name="$(basename "$file")"
    if reason="$(no_design_reason "$name")"; then
        echo "skip  examples/$name: $reason"
        continue
    fi
    check "examples/$name" "$examples" "$name"
done
for entry in "${no_design[@]}"; do
    if [ ! -f "$examples/${entry%%|*}" ]; then
        echo "FAIL  examples/${entry%%|*} is named in no_design of this script and is not an example: remove the entry"
        failed=1
    fi
done
for manifest in "$examples"/*/netsmith.toml; do
    [ -e "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    tops="$(top_level "$manifest")"
    if [ -z "$tops" ]; then
        echo "FAIL  examples/$(basename "$dir"): found no file in the top_level list of its netsmith.toml (this script reads a list on one line)"
        failed=1
        continue
    fi
    while IFS= read -r top; do
        check "examples/$(basename "$dir")/$top" "$dir" "$top"
    done <<< "$tops"
done

if [ "$runs" -eq 0 ]; then
    echo "FAIL  found no board in $examples: no *.nsm file and no folder with a netsmith.toml, so there is nothing to check (did the examples move?)"
    failed=1
fi

# A design file whose board source is gone: `<dir>/<stem>.design.json` belongs to `<dir>/<stem>.nsm`.
while IFS= read -r fixture; do
    [ -n "$fixture" ] || continue
    if [ ! -f "${fixture%.design.json}.nsm" ]; then
        echo "FAIL  ${fixture#"$examples"/}: an orphan design file: ${fixture%.design.json}.nsm is not there (delete the file, or restore the source)"
        failed=1
    fi
done < <(find "$examples" -name '*.design.json' | sort)

if [ "$failed" -ne 0 ]; then
    cat >&2 <<'MSG'

check-example-designs: FAILED
An example's design file (<stem>.design.json, beside its source) does not match the design the
board builds now: the checker, the solver, the design stage, std/ or a catalog changed it, or an
example has no design file yet. A design that moves is a change to the board, so a person reviews it:
  1. from the repository's root, run
         scripts/check-example-designs.sh --update [path/to/netsmith]
     (it writes each design file that differs, and names it);
  2. read the diff of the design files, and commit them with the change that caused it.
A board that does not build is fixed first (a stale pick lock fails here too: see
scripts/check-example-locks.sh). A design file named as an orphan has no source beside it any
more: delete it. No board found means the examples are not where this script looks for them.
MSG
    exit 1
fi
if [ "$update" = true ]; then
    echo "check-example-designs: $runs boards, every design file is written"
else
    echo "check-example-designs: $runs boards, every design file is current"
fi
