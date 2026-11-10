---
status: superseded by ADR-0006
---

# Emit schematics, and leave the PCB layout to the CAD tool

Status notes: Superseded by ADR-0006 for boards netsmith generates.

The first exporter writes a KiCad schematic and never writes or reads a `.kicad_pcb`. KiCad's own "Update PCB from Schematic" keeps footprints and routing in place when the schematic is regenerated, as long as symbol UUIDs derive from names. atopile instead writes the PCB directly, which allows layout reuse but means preserving hand routing ourselves; we chose to avoid that risk until we have an in-house editor, at the cost of a schematic that is correct but plainly laid out.
