#!/usr/bin/env bash
# Run Windows KiCad's kicad-cli from WSL on a generated schematic: ERC, netlist, PDF, PNG.
# Usage: scripts/check-kicad.sh out/led/led.kicad_sch
# Results land in out/check/<name>/. Needs KiCad 9 on Windows and poppler-utils in WSL.
set -euo pipefail

sch=$(realpath "$1")
name=$(basename "$sch" .kicad_sch)
cli="${KICAD_CLI:-/mnt/c/Program Files/KiCad/9.0/bin/kicad-cli.exe}"
[ -x "$cli" ] || { echo "kicad-cli not found at: $cli (set KICAD_CLI)" >&2; exit 1; }

out="$(dirname "$sch")/../check/$name"
mkdir -p "$out"
out=$(realpath "$out")

# kicad-cli.exe is happier with a Windows-local copy than a \\wsl$ path.
win_tmp=$(wslpath "$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')")/netsmith-check-$name
rm -rf "$win_tmp" && mkdir -p "$win_tmp"
cp "$sch" "$win_tmp/$name.kicad_sch"
[ -f "${sch%.kicad_sch}.kicad_pro" ] && cp "${sch%.kicad_sch}.kicad_pro" "$win_tmp/"
wdir=$(wslpath -w "$win_tmp")

"$cli" sch erc --format json --severity-all -o "$wdir\\erc.json" "$wdir\\$name.kicad_sch" >/dev/null
"$cli" sch export netlist --format kicadsexpr -o "$wdir\\$name.net" "$wdir\\$name.kicad_sch" >/dev/null
"$cli" sch export pdf -o "$wdir\\$name.pdf" "$wdir\\$name.kicad_sch" >/dev/null
cp "$win_tmp"/erc.json "$win_tmp/$name.net" "$win_tmp/$name.pdf" "$out"/
rm -rf "$win_tmp"

pdftoppm -png -r 150 "$out/$name.pdf" "$out/$name"

python3 -I - "$out/erc.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1]))
count = 0
for sheet in report.get("sheets", []):
    for v in sheet.get("violations", []):
        count += 1
        print(f"ERC {v['severity']}: {v['type']}: {v['description']}")
print(f"ERC: {count} violation(s)")
PY
echo "results in $out"
