#!/usr/bin/env bash
# Proves that `build`, `fab`, `board3d` (with `--json` and `--glb`) and `render` need no KiCad (ADR 0097, step 4): builds and fabs every example
# that is meant to build, with KiCad's symbol and footprint libraries and `kicad-cli` hidden, and
# fails if a run exits with an error or says that KiCad is missing.
#
# The examples are listed from the examples/ folder, never by hand, so a new one is covered by
# adding it: every `*.nsm` of examples/ that is a board (not a `*.physical.nsm`, the physical file
# of a board) is built and fabbed from examples/, and every folder of examples/ is a project (it
# has a `netsmith.toml`, which is how `netsmith build` takes a folder) and is built from its own
# folder, as a person runs it, and fabbed too when it has a physical file (`*.physical.nsm`):
# without one the parts are placed on a board with no size and `fab` has nothing to say yet.
# What cannot build is named in `skipped` below, and a project that is built and not fabbed in
# `no_fab`, each with its reason; the script fails for a folder or a file that is neither built
# nor named there, and for a name there that is gone.
#
# A project may name a KiCad part of its own in `kicad.toml`, which is the one case that needs
# KiCad (ADR 0097), and then a run stops and says which part, which id and which line. No example
# does: `schematic_bench` and `enclosure_sensor` draw their symbols from the pin tables and take
# their footprints from the generators or from their own `libs/` (their `README.md` and
# `kicad.toml` say how), so every example here must build, and fab where it is fabbed.
#
# KiCad is hidden four ways, so that no one of them has to be right alone:
#   - NETSMITH_NO_KICAD=1 makes the search find nothing, `kicad-cli` and the Windows KiCad that
#     WSL would find under /mnt/c included;
#   - NETSMITH_KICAD_SYMBOLS and NETSMITH_KICAD_FOOTPRINTS name empty folders;
#   - KICAD_CLI is unset;
#   - the directories of PATH that hold a kicad-cli are removed from it.
# `fab` prints a note when it finds no kicad-cli, "KiCad is switched off" when NETSMITH_NO_KICAD is
# set (it is, here) and "KiCad is not installed" when it simply finds none; the script requires
# one of the two, as the proof that KiCad really was hidden from it.
#
# Usage: scripts/check-no-kicad.sh [path/to/netsmith]
# Without an argument it builds the debug binary with `cargo build -p netsmith` first. The examples
# are copied to a temporary folder: a run writes the pick lock beside the sources.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"

# The projects that are built here and not fabbed, as "name|reason", though they have a physical
# file: a run of this script has about a minute for each.
no_fab=(
    "rp2350_minimal|the placer takes 3 s, but the router needs 35 s on an idle machine for the seed the project's layout file names (97 s for seed 0), more than the 60 s that one run of this script may take, and a run the router has to stop exits 1 with INCOMPLETE; the test rp2350_minimal_pcb_matches_raspberry_pi (crates/netsmith/tests/rp2350_parity.rs) runs fab on the project with NETSMITH_NO_KICAD=1 and a route timeout of 20 s, and gates on the parity checker, in the test slices"
    "power_manifold_charger|89 parts on a 101.2 x 24 mm board: the placer takes 15 s, but the router does not finish (4 of 256 connections are left after its 4 tries, in 5 min in a debug build), and a run the router cannot finish exits 1 with INCOMPLETE; the test power_manifold_pcb_matches_the_original_where_it_can (crates/netsmith/tests/power_manifold_parity.rs) runs fab on the project with NETSMITH_NO_KICAD=1 and a route timeout of 20 s, and gates on the parity checker, in the test slices (README, \"The PCB\")"
)

# What of examples/ is not built, as "name|reason". A file is named by its file name, a folder
# by its name and a slash. Keep this short: each entry is something that cannot build.
skipped=(
    "carrier_and_cards.nsm|its parts are invented and have no KiCad symbol: it is checked and solved only (crates/netsmith-check/tests/mate.rs, crates/netsmith-solve/tests/mate.rs), never built"
)

if [ "$#" -ge 1 ]; then
    bin="$(realpath "$1")"
else
    (cd "$repo" && cargo build --quiet -p netsmith)
    bin="$repo/target/debug/netsmith"
fi
[ -x "$bin" ] || { echo "error: $bin is not an executable (build it, or name the binary)" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/no-symbols" "$work/no-footprints" "$work/home"
cp -R "$repo/examples" "$work/examples"

# PATH without a directory that holds a kicad-cli.
clean_path=""
IFS=: read -r -a dirs <<< "${PATH:-}"
for dir in "${dirs[@]}"; do
    if [ -e "$dir/kicad-cli" ] || [ -e "$dir/kicad-cli.exe" ]; then
        echo "hidden from PATH: $dir"
        continue
    fi
    clean_path="${clean_path:+$clean_path:}$dir"
done

# Run `netsmith` with KiCad hidden, in the folder `$1`, with the arguments after it. The output
# goes to $work/log, and is shown if the run fails.
hidden() {
    local dir="$1"
    shift
    (
        cd "$dir"
        env -u KICAD_CLI \
            PATH="$clean_path" \
            HOME="$work/home" \
            NETSMITH_NO_KICAD=1 \
            NETSMITH_KICAD_SYMBOLS="$work/no-symbols" \
            NETSMITH_KICAD_FOOTPRINTS="$work/no-footprints" \
            "$bin" "$@"
    ) > "$work/log" 2>&1
}

failed=0
runs=0

# step <label> <folder> <netsmith arguments...>: one run that must succeed and say nothing of a
# missing KiCad. A `fab` run must say that it found no kicad-cli (the proof that it was hidden).
step() {
    local label="$1" dir="$2"
    shift 2
    runs=$((runs + 1))
    if ! hidden "$dir" "$@"; then
        echo "FAIL  $label: exited with an error"
        sed 's/^/      /' "$work/log"
        failed=1
        return
    fi
    if grep -Eiq 'KiCad not found|no KiCad (symbol|footprint) libraries' "$work/log"; then
        echo "FAIL  $label: says that KiCad is missing"
        sed 's/^/      /' "$work/log"
        failed=1
        return
    fi
    if [ "$1" = fab ] && ! grep -Eq 'KiCad is (switched off|not installed)' "$work/log"; then
        echo "FAIL  $label: fab did not report that it found no kicad-cli, so KiCad was not hidden"
        sed 's/^/      /' "$work/log"
        failed=1
        return
    fi
    echo "ok    $label"
}

# Why `$1` (a name in examples/) is not built, or nothing when it is.
skip_reason() {
    local entry
    for entry in "${skipped[@]}"; do
        if [ "${entry%%|*}" = "$1" ]; then
            printf '%s' "${entry#*|}"
            return 0
        fi
    done
    return 1
}

# Why `$1` (a project of examples/) is built here and not fabbed, or nothing when it is fabbed.
no_fab_reason() {
    local entry
    for entry in "${no_fab[@]}"; do
        if [ "${entry%%|*}" = "$1" ]; then
            printf '%s' "${entry#*|}"
            return 0
        fi
    done
    return 1
}

# What is in examples/ now, so that an allow-list entry for a thing that is gone is found out.
present=" "
shopt -s nullglob

# The boards that are one file.
for file in "$repo"/examples/*.nsm; do
    base="$(basename "$file")"
    present="$present$base "
    case "$base" in *.physical.nsm) continue ;; esac
    if reason="$(skip_reason "$base")"; then
        echo "skip  examples/$base: $reason"
        continue
    fi
    name="${base%.nsm}"
    step "build examples/$base" "$work/examples" build "$base" --out "$work/out/$name"
    step "fab   examples/$base" "$work/examples" fab "$base" --out "$work/fab/$name"
done

# `board3d` places a board as `fab` does and prints its 3D scene (ADR 0105), with no KiCad either:
# a board of two layers, of four, with an outline and pours from a layout file, and one board of a
# file of several.
for args in "led.nsm" "four_layer.nsm" "outline_and_pours.nsm" "two_boards.nsm --board TwoBoards/led"; do
    # shellcheck disable=SC2086 # the arguments are words
    step "board3d examples/$args" "$work/examples" board3d $args --json
done
# And the same scene as a `.glb` file (the one the editor's 3D view shows).
step "board3d examples/four_layer.nsm --glb" "$work/examples" board3d four_layer.nsm --glb "$work/four_layer.glb"
[ -s "$work/four_layer.glb" ] || { echo "FAIL  board3d --glb wrote no file"; failed=1; }
# And the picture of it (`render`, netsmith's own rasterizer), with a part lit up.
step "render examples/led.nsm" "$work/examples" render led.nsm --view iso --highlight J1 --out "$work/led-iso.png"
[ -s "$work/led-iso.png" ] || { echo "FAIL  render wrote no file"; failed=1; }

# The projects: run from their folder, as a person does.
for dir in "$repo"/examples/*/; do
    name="$(basename "$dir")"
    present="$present$name/ "
    if reason="$(skip_reason "$name/")"; then
        echo "skip  examples/$name/: $reason"
        continue
    fi
    if [ ! -f "$dir/netsmith.toml" ]; then
        echo "FAIL  examples/$name/: has no netsmith.toml, so this script does not build it: give it one, or name it in \`skipped\` with the reason it cannot build"
        failed=1
        continue
    fi
    step "build examples/$name" "$work/examples/$name" build --out "$work/out/$name"
    # A project with no physical file has no board outline: its parts sit on a board with no size
    # and `fab` has nothing to say yet, so it is run when the file is there.
    if [ -z "$(find "$dir" -name '*.physical.nsm' -not -path '*/libs/*' -not -path '*/reference/*' -print -quit)" ]; then
        echo "note  examples/$name: no physical file, so no fab run"
    elif reason="$(no_fab_reason "$name")"; then
        echo "note  examples/$name: no fab run here: $reason"
    else
        step "fab   examples/$name" "$work/examples/$name" fab --out "$work/fab/$name"
    fi
done

# A name of an allow-list that is not in examples/ any more (`no_fab` names projects: folders).
stale() {
    case "$present" in
        *" $1 "*) ;;
        *)
            echo "FAIL  \`$2\` names $1, which is not in examples/ (remove it)"
            failed=1
            ;;
    esac
}
for entry in "${skipped[@]}"; do
    stale "${entry%%|*}" skipped
done
for entry in "${no_fab[@]}"; do
    stale "${entry%%|*}/" no_fab
done

if [ "$failed" -ne 0 ]; then
    echo "check-no-kicad: FAILED"
    exit 1
fi
echo "check-no-kicad: $runs runs with no KiCad: every example built, and fabbed where it has a physical file"
