#!/usr/bin/env bash
# Proves that every example and the standard library are in the layout `netsmith project fmt` writes
# (issue #1103): runs `netsmith project fmt --check` over every board of examples/ and every file of
# std/, with KiCad hidden (the formatter reads nothing but the source).
#
# The examples are found by folder, as scripts/check-example-lint.sh finds them:
#   - the `*.nsm` files of examples/ (the boards' sources and their physical files) are checked as
#     `netsmith project fmt --check <files>` from examples/;
#   - each folder of examples/ with a netsmith.toml is a project, checked as
#     `netsmith project fmt --check` from that folder (every file of the project);
#   - the `*.nsm` files of std/ are checked as `netsmith project fmt --check <files>`.
# It also fails when it finds no board at all (a changed folder layout would make the globs match
# nothing and pass for nothing). scripts/check-example-fmt-test runs the script over a fake examples
# folder with a stub binary; NETSMITH_EXAMPLES_DIR and NETSMITH_STD_DIR name those folders (default:
# examples/ and std/ of the repository).
#
# Usage: scripts/check-example-fmt.sh [path/to/netsmith]
# Without an argument it builds the debug binary with `cargo build -p netsmith-cli` first. Nothing is
# written: `--check` never writes.
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
std="${NETSMITH_STD_DIR:-$repo/std}"
[ -d "$std" ] || { echo "error: $std is not a folder" >&2; exit 2; }
std="$(cd "$std" && pwd)"

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

failed=0
runs=0

# check <label> <folder> <netsmith arguments...>: one `project fmt --check` that must exit 0.
check() {
    local label="$1" dir="$2"
    shift 2
    runs=$((runs + 1))
    local code=0
    (cd "$dir" && NETSMITH_NO_KICAD=1 "$bin" project fmt --check "$@") > "$log" 2>&1 || code=$?
    if [ "$code" -eq 0 ]; then
        echo "ok    $label"
        return
    fi
    failed=1
    if [ "$code" -eq 1 ]; then
        echo "FAIL  $label: files are not formatted"
    else
        echo "FAIL  $label: the formatter could not run (exit $code)"
    fi
    sed 's/^/      /' "$log"
}

files=()
for file in "$examples"/*.nsm; do
    [ -e "$file" ] || continue
    files+=("$(basename "$file")")
done
[ "${#files[@]}" -eq 0 ] || check "examples/*.nsm" "$examples" "${files[@]}"
for manifest in "$examples"/*/netsmith.toml; do
    [ -e "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    check "examples/$(basename "$dir")" "$dir"
done
boards="$runs"
std_files=()
while IFS= read -r file; do
    std_files+=("$file")
done < <(cd "$std" && find . -name '*.nsm' | sed 's|^\./||' | sort)
[ "${#std_files[@]}" -eq 0 ] || check "std/" "$std" "${std_files[@]}"

if [ "$boards" -eq 0 ]; then
    echo "FAIL  found no board in $examples: no *.nsm file and no folder with a netsmith.toml, so there is nothing to check (did the examples move?)"
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    cat >&2 <<'MSG'

check-example-fmt: FAILED
An example or a file of the standard library is not in the layout `netsmith project fmt` writes, or
does not parse. Run `netsmith project fmt` in the example's folder (or `netsmith project fmt <files>`
for the single-file examples and std/) and commit the result.
MSG
    exit 1
fi
echo "check-example-fmt: $runs runs, every file formatted"
