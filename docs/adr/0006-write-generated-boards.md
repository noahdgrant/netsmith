---
status: accepted
---

# Write schematics and generated boards; do not read layout

netsmith writes a KiCad schematic and, for a generated board, a `.kicad_pcb` with footprints placed, nets assigned and the tracks routed. Each run makes a new board, so hand layout is not preserved and a `.kicad_pcb` is never read; this replaces ADR 0003's rule that netsmith never writes one, which assumed KiCad would own the layout. Footprints come from the installed KiCad libraries through the export mapping, and the Gerbers are made by KiCad's own exporter from that file. Preserving a hand-edited layout across regeneration waits for an in-house editor.
