---
status: accepted
---

# A manufacturing process is a member of the board

A board is made by one manufacturing process, and its design rules (least track, space, via, annular ring, edge clearance, silkscreen, solder mask) and its stackup (layers, thickness, copper weight) come from that process, not from the tool. They are written in the standard library as `std::mfg::jlcpcb::TwoLayer`, imported like any definition, and the board has it as a member: `.process: TwoLayer`. This replaces the constant `Rules::jlcpcb_2layer()` in `netsmith-pcb`.

A process is an `interface` with no pins: a typed bundle of quantities, which is what the glossary already says an interface is. We did not add a `process` keyword, because the checker, the `&=` narrowing and the import already do everything it would, and a second kind of definition for "an interface with only values" would be a second way to say it (ADR 0001). The `Stackup` it holds (`std::mfg::stackup`) is another interface, nested as `.process.stackup`. The cost is that nothing in the language says "this interface is a process": lowering reads the board's member named `process` and refuses, naming the field, one that lacks a field every process states. A different house is a different definition with the same field names.

The library states only what is always true of the process, so a rule is a range: `.min_track &= 0.1mm..1m` says the least track is at least 0.1 mm, JLCPCB's published figure. The board narrows it with `&=` as with any value (`.process.min_track &= 0.15mm..1m` for margin), and a narrowing that does not overlap the range is a contradiction naming the field, as everywhere else. Ranges are closed, so `1m`, longer than any board, stands for no upper limit; a half-open range is a language change this does not need. Lowering takes each rule as the bottom of what is left of its range and puts the resolved process in the `Design` (not in `Part`: it belongs to the board). The router, placer, `.kicad_pcb` and `.kicad_pro` writers and the fabrication notes read it from there.

The thickness is the one stackup value a board must choose (`.stackup.thickness &= 1.6mm`), because the process offers 0.4 mm to 2 mm and netsmith will not pick for the board. A board with no `process` still builds and views, as a schematic; making a PCB refuses it and says to add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`. There is no default process.

The router draws at the process's narrowest track and smallest via, so the boards are finer than the old conservative constant, which was not a published figure. A board that wants margin narrows the process. A via's copper is its drill plus the least annular ring on both sides, if that is more than the process's smallest via. Separate widths for power nets wait for net classes.

The four-layer process, its planes and the router's handling of them are in ADR 0012.
