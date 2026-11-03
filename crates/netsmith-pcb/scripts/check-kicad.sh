#!/usr/bin/env bash
# Run Windows KiCad's kicad-cli from WSL on a generated board: DRC, an SVG and a 3D render.
# A board with planes has its zones filled by KiCad first, on a copy.
# Usage: crates/netsmith-pcb/scripts/check-kicad.sh out/LedExample/LedExample.kicad_pcb
# The DRC must load the file, and its "unconnected items" count is the ratsnest's line count
# (the example program prints it). Results land in out/check/<name>/.
set -euo pipefail

pcb=$(realpath "$1")
name=$(basename "$pcb" .kicad_pcb)
cli="${KICAD_CLI:-/mnt/c/Program Files/KiCad/9.0/bin/kicad-cli.exe}"
[ -x "$cli" ] || { echo "kicad-cli not found at: $cli (set KICAD_CLI)" >&2; exit 1; }

out="$(dirname "$pcb")/../check/$name"
mkdir -p "$out"
out=$(realpath "$out")

# kicad-cli.exe is happier with a Windows-local copy than a \\wsl$ path.
win_tmp=$(wslpath "$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')")/netsmith-pcb-$name
rm -rf "$win_tmp" && mkdir -p "$win_tmp"
cp "$pcb" "$win_tmp/$name.kicad_pcb"
[ -f "${pcb%.kicad_pcb}.kicad_pro" ] && cp "${pcb%.kicad_pcb}.kicad_pro" "$win_tmp/"
wdir=$(wslpath -w "$win_tmp")

# A board with planes holds zones with no fill, and KiCad's DRC does not fill them: have
# KiCad's Python fill them on this copy first (netsmith fab does the same).
if grep -q $'^\t(zone' "$pcb"; then
    mkdir "$win_tmp/filled"
    cat > "$win_tmp/fill_zones.py" <<'PY'
import sys, pcbnew
board = pcbnew.LoadBoard(sys.argv[1])
pcbnew.ZONE_FILLER(board).Fill(board.Zones())
pcbnew.SaveBoard(sys.argv[2], board)
PY
    "$(dirname "$cli")/python.exe" "$wdir\\fill_zones.py" "$wdir\\$name.kicad_pcb" \
        "$wdir\\filled\\$name.kicad_pcb"
    mv "$win_tmp/filled/$name.kicad_pcb" "$win_tmp/$name.kicad_pcb"
fi

"$cli" pcb drc --format json --severity-all --all-track-errors \
    -o "$wdir\\drc.json" "$wdir\\$name.kicad_pcb" >/dev/null
"$cli" pcb export svg --layers "F.Cu,F.Mask,F.SilkS,F.CrtYd,F.Fab,Edge.Cuts" \
    --exclude-drawing-sheet -o "$wdir\\$name.svg" "$wdir\\$name.kicad_pcb" >/dev/null 2>&1
"$cli" pcb render --side top --quality basic --width 1400 --height 800 \
    -o "$wdir\\$name.png" "$wdir\\$name.kicad_pcb" >/dev/null 2>&1
cp "$win_tmp"/drc.json "$win_tmp/$name.svg" "$win_tmp/$name.png" "$out"/
rm -rf "$win_tmp"

python3 -I - "$out/drc.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1]))
for key in ("violations", "unconnected_items"):
    for v in report.get(key, []):
        print(f"DRC {v['severity']}: {v['type']}: {v['description']}")
print(f"DRC: {len(report.get('violations', []))} violation(s), "
      f"{len(report.get('unconnected_items', []))} unconnected item(s)")
PY
echo "results in $out"
