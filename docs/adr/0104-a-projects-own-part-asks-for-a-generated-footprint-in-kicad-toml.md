---
status: accepted
---

# A project's own part asks for a generated footprint in its kicad.toml

Decides the second question that ADR 0097 left open ("how a project's own part gets a footprint with no KiCad"). The owner chose on 2026-11-10 that a project may ask netsmith to generate a footprint by generator name and parameters, besides supplying `.kicad_mod` files through `[[library]]`; the owner confirmed the spelling on 2026-11-11. It meets test 1 (it is a file format: a project's `kicad.toml` and the ids of its written boards depend on the spelling) and test 3 (it rejects two obvious alternatives, below).

**Decision.** The `footprint` of a `[[part]]` row of `kicad.toml` is a `Library:Name` id, or `gen:Generator(key=value, ...)`:

```toml
footprint = "gen:QFN(pins=48, pitch=0.5mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm, lead_length=0.35-0.45mm)"
```

The parameters are the columns of the generator's table, by name, so the pads are those of the table row with the same numbers: the same code makes them, and a test compares `gen:` with `generate(<id>)` for several ids of each generator. A length carries its unit (`0.5mm`), a size is `0.25mm` or `0.2-0.3mm`, a pair is `7x7mm`, a count pair `3x3`. Eight generators are exposed: `QFN`, `DFN`, `QFP`, `SO` (SOIC, SSOP, TSSOP, MSOP: two columns of gull-wing leads), `SOT23` (3, 5 or 6 leads), `PinHeader` and `PinSocket` (2.54 mm and 2 mm), and `BGA` (a full grid). A dimension that a datasheet gives has no default; a parameter has one only where the generator's own table has a rule (`QFP` lead width at 0.4, 0.5, 0.65 and 0.8 mm, `SOT23` from its row, the paste openings of an exposed pad of at most 1.5 mm).

`Mapping::parse` reads the spelling and gives the row the footprint's id: library `gen`, and a name of the generator and its parameters in alphabetical order (`gen:QFN_body7x7mm_ep5.2x5.2mm_lead_length0.35-0.45mm_..._pitch0.5mm`). One set of numbers is one id whatever the order, spaces or zeros of the row, and the id reads back to the same parameters, so everything that passes a `Library:Name` (the board, the bill of materials, the written `.kicad_pcb`) passes this without a change, and `FootprintLibrary::load` makes the footprint from the id with no library and no KiCad. The generator checks the parameters when the project's libraries are read (`Libraries::for_project`): an unknown generator or parameter, a missing or out-of-range one, and pads that overlap are each an error naming the part, the package, the file and the line. A part with more or fewer pins than the footprint has pads is the pinout check's error (ADR 0021), which now also names the `gen:` row, its line and the pad count. A project cannot list a library named `gen`.

**Rejected: a generator name in the part definition, in the language.** `part Chip { footprint = QFN(...) }` puts a CAD fact in the circuit, which the design rules forbid (libraries state only what is always true; the mapping is the one place KiCad's names appear, ADR 0021), makes the language grow a syntax for every generator, and splits one part's package across two files. A part says what it is; `kicad.toml` says what carries it.

**Rejected: files only.** A `.kicad_mod` through `[[library]]` already works and stays. Alone it makes a person draw by hand a QFN that a generator makes from five numbers, with the IPC land pattern, and gives nothing that proves the pads from the datasheet's numbers.

**Cost.** A second spelling of a footprint in a row, so the guide says when to use which (`gen:` when a generator has the package, a file when it has not). The exposed generators' parameter names are a file format from now on. A generated footprint's name is long and carries the numbers; KiCad's own library does not have it, so "update footprint from library" finds nothing for it in KiCad, as it finds nothing for a generated KiCad name when no KiCad is installed. A QFN made from numbers is as right as the numbers: the IPC rule is the only check, with the overlap test.

**Left open.** The generators not exposed: the chips (the small-fillet rule is chosen from the size's name), diodes (SOD, SMA, SMB, SMC), crystals and the recorded pads (SOT-223, SC-70, DPAK, D2PAK, SOT-89), whose pads are a maker's drawing and no function of the dimensions, and the mechanical parts. Within the exposed ones: a QFN with no exposed pad, with different counts on its edges or with a body whose edges differ, a gull-wing with the fine-pitch fillets chosen apart from the pitch, a SOT-23 at another pitch, headers at another pitch, a BGA with balls left out. Whether the name of a generated footprint should be shorter (a hash) is the owner's.
