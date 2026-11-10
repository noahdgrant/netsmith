#!/usr/bin/env bash
# Proves that `build` and `fab` need no KiCad (ADR 0097, step 4): builds and fabs every example
# that is meant to build, with KiCad's symbol and footprint libraries and `kicad-cli` hidden, and
# fails if a run exits with an error or says that KiCad is missing.
#
# The examples are the ones the tests use (crates/netsmith/tests/erc.rs, schematic_invariants.rs):
# the boards of examples/ and the three projects. `carrier_and_cards.nsm` is left out: its parts
# are invented and have no KiCad symbol, so it is checked and solved only, never built.
#
# Two projects still name a KiCad part of their own in `kicad.toml`, which is the one case that
# needs KiCad (ADR 0097): `schematic_bench` maps two parts to stock KiCad symbols (`LM358` and
# `24LC16`, drawn in KiCad's shape on purpose, it is the bench of the schematic drawing) and
# `enclosure_sensor` maps its chip antenna to KiCad's `Johanson_2450AT43F0100` footprint. With
# KiCad hidden they must stop, and say which part, which id and which line of `kicad.toml`, and
# the three ways out. The script checks that they do. If one of them is changed to need no KiCad,
# this says so: move it to the loop that must build.
#
# KiCad is hidden four ways, so that no one of them has to be right alone:
#   - NETSMITH_NO_KICAD=1 makes the search find nothing, `kicad-cli` and the Windows KiCad that
#     WSL would find under /mnt/c included;
#   - NETSMITH_KICAD_SYMBOLS and NETSMITH_KICAD_FOOTPRINTS name empty folders;
#   - KICAD_CLI is unset;
#   - the directories of PATH that hold a kicad-cli are removed from it.
# `fab` prints a note when it finds no kicad-cli; the script requires that note, as the proof that
# KiCad really was hidden from it.
#
# Usage: scripts/check-no-kicad.sh [path/to/netsmith]
# Without an argument it builds the debug binary with `cargo build -p netsmith` first. The examples
# are copied to a temporary folder: a run writes the pick lock beside the sources.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"

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
    if [ "$1" = fab ] && ! grep -q 'KiCad is not installed' "$work/log"; then
        echo "FAIL  $label: fab did not report that it found no kicad-cli, so KiCad was not hidden"
        sed 's/^/      /' "$work/log"
        failed=1
        return
    fi
    echo "ok    $label"
}

# The boards that are one file.
for name in led led_panel linear_regulator status_indicator four_layer two_boards outline_and_pours; do
    step "build examples/$name.nsm" "$work/examples" build "$name.nsm" --out "$work/out/$name"
    step "fab   examples/$name.nsm" "$work/examples" fab "$name.nsm" --out "$work/fab/$name"
done

# The projects: run from their folder, as a person does.
for project in rp2350_minimal; do
    step "build examples/$project" "$work/examples/$project" build --out "$work/out/$project"
done
# `fab` on rp2350_minimal waits for the board's physical file (outline, mounting holes, the
# places of the connectors; a later item of epic #308): without them the 39 parts are placed
# on a board with no size, and the router stops at its 300 s timeout with connections left.
# Add `fab examples/rp2350_minimal` back here when that item merges.

# stops <label> <folder> <KiCad id> <netsmith arguments...>: a run that names a KiCad part of the
# project's own must stop, and its message must name the part, the id and the line of kicad.toml
# and say the three ways out (and not the editor's "KiCad not found").
stops() {
    local label="$1" dir="$2" id="$3"
    shift 3
    runs=$((runs + 1))
    if hidden "$dir" "$@"; then
        echo "FAIL  $label: built with no KiCad, but it names KiCad's \`$id\`: move it to the loop that must build"
        failed=1
        return
    fi
    for want in "needs KiCad's" "\`$id\`" "(kicad.toml line " "Three ways out"; do
        if ! grep -qF -- "$want" "$work/log"; then
            echo "FAIL  $label: the message lacks: $want"
            sed 's/^/      /' "$work/log"
            failed=1
            return
        fi
    done
    if grep -qi 'KiCad not found' "$work/log"; then
        echo "FAIL  $label: says \"KiCad not found\""
        failed=1
        return
    fi
    echo "ok    $label: stops, as it must, at KiCad's \`$id\` of its own kicad.toml"
}

stops "build examples/schematic_bench" "$work/examples/schematic_bench" \
    "Amplifier_Operational:LM358" build --out "$work/out/schematic_bench"
stops "fab   examples/schematic_bench" "$work/examples/schematic_bench" \
    "Amplifier_Operational:LM358" fab --out "$work/fab/schematic_bench"
stops "build examples/enclosure_sensor" "$work/examples/enclosure_sensor" \
    "RF_Antenna:Johanson_2450AT43F0100" build --out "$work/out/enclosure_sensor"
stops "fab   examples/enclosure_sensor" "$work/examples/enclosure_sensor" \
    "RF_Antenna:Johanson_2450AT43F0100" fab --out "$work/fab/enclosure_sensor"

if [ "$failed" -ne 0 ]; then
    echo "check-no-kicad: FAILED"
    exit 1
fi
echo "check-no-kicad: $runs runs with no KiCad: every example without a KiCad part of its own built and fabbed, the two with one stopped at it"
