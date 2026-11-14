# Glossary: D

**3D view**:
The editor's view of a board in three dimensions, in the PCB section: the board as its stackup makes it, the copper, and each part's _Body_ extruded to its height, read-only (ADR 0105). It displays the _Scene_; it moves nothing (a part is moved in the 2D PCB view) and draws no third-party 3D model.
_Avoid_: 3D viewer, 3D model (netsmith bundles none), render (a render is a picture of the scene)

**DC element**:
What a part is in a DC analysis, stated once by a `dc` statement in the part or its generic: `dc resistor .a .b .resistance`, `dc diode .anode .cathode .forward_voltage`, `dc nmos` or `dc pmos` with a gate, a drain, a source, a threshold and an on resistance. Its pins are the pins its `bridge` joins. A provider of a generic has the generic's; a module has none, and the elements of the parts inside it make its network. The checker records it (ADR 0076, amendment 1); the DC analysis solves a resistor and a diode, and a MOSFET is recorded and not solved yet (ADR 0076, amendment 2).
_Avoid_: Model (of a part), stamp, SPICE model

**Declared kind**:
What the tools treat a part as, written `#[kind(capacitor)]` on its definition or a generic it provides (ADR 0125, `netsmith_core::PartKind`, the design file's `declared_kind`): the placer decouples with a `capacitor`, the schematic draws a `fuse` in series, the 3D view colours by it. A closed list of words; the tools read it, never the name of the part's definition (its kind, `Resistor`).
_Avoid_: Class (that is a class of pin functions, ADR 0071), category (that is the 3D view's colouring, which it gives), type

**Decoupling pair**:
A two-pad capacitor whose pads are on two different nets that one same other part has `power` pins on, together with that part. Nothing in the source says so: the netlist and the pin kinds do. Of several such parts the ones that share most of the capacitor's instance path are kept (ADR 0033). The router routes the pin to the capacitor's pad first and gives the capacitor's pad on a poured return net (ground) a short via of its own (`decouple.rs` in `netsmith-router`, issue #431).
_Avoid_: Bypass capacitor (a bypass capacitor is the datasheet's word for the same part, but only a pair the placer finds is meant here)

**Default**:
A value a module or part offers for a field and a board may replace, written `.field ?= value` (ADR 0051, proposed). It applies only when no `&=` statement states that field; a value that reaches the field through a relation or a join does not count as stated. A board's statement drops the default and need not overlap it, and two defaults for one field are an error.
_Avoid_: Initial value, preset, fallback, soft constraint, weak narrowing

**Derived figure**:
A value the checker gives a member of a module from the parts it cannot reach, by the nets and by a field's name, as it gives a part its `.bias` from the rail it is across: the bus capacitance and the least pull-up of an I2C bus from the `I2c` interfaces on the bus (ADR 0102). The board states only what is its own (the estimate of its traces) and may state the figure itself, which then stands in place of the derivation. A part that states no figure is named when a check waits for it, never skipped without a word.
_Avoid_: Computed value, inferred value, rollup

**Derived placement**:
The placer's way of choosing where parts go: from the circuit alone, with nothing written in the source. It keeps the parts of a module instance together, puts a decoupling capacitor next to the power pins it bridges, connectors at the edges and wire short, turns parts by right angles, and keeps reference text clear of other parts, all by simulated annealing with a seeded random generator (ADR 0033).
_Avoid_: Autoplace, autorouter placement

**Design rules check**:
netsmith's own check of a board's copper, holes, edge, courtyards, silkscreen, solder mask and connectivity against the process's rules (ADR 0037). It is the gate `fab` runs. A finding is data: a rule (named as KiCad names it), a severity (an error stops `fab`, a warning is listed in the notes), a message with the numbers, a place and the items involved, which the editor's Findings tab shows. _Avoid_: DRC gate (when KiCad's check is meant, say KiCad's design rules check)

**Design stage**:
The step of a build that turns a solved board into the `netsmith_core::Design` (the parts with their designators and pads, the nets, the track widths, the sourcing, the guidelines), after _check_ and _solve_ and before the exporters (`netsmith-design`; `Stage::Design`; the stage `design` in the editor's Problems dock and progress text). The stage and the type share a word, so say which is meant: _the design stage_ is the work, _the `Design`_ is what it makes. Its notes and its errors (a requirement that cannot hold, a part with no pads) are said as the stage's own.
_Avoid_: Lower, lowering, lowered (compiler words: say _the design stage_, and _designed_ of a board it has been run on)

**Designator**:
The reference of a part on the schematic, the bill of materials and the placement file: letters and a number, `R1`, `C2`, `BZ1`. The letters are the field `.designator` of every part, stated by its kind (`Resistor` is `"R"`, `Fuse` `"F"`, a diode and a TVS `"D"`, ADR 0046) or narrowed by a part or board that defines a part with none, and `"U"` when nothing says; the number counts the parts with the same letters, from 1, in the order of their full paths in the board, compared segment by segment with a number at the end of a segment, or an index, compared as a number (`tp2` before `tp10`, `decoupling[2]` before `decoupling[10]`); each board, of a system too, is numbered on its own (owner decision 2026-11-13, issue #634; it replaces the declaration order of 2026-11-09). It does not depend on the order of the statements, the layout engine, the drawing of the schematic or KiCad, so the netlist, the bill of materials and the board are the same with every layout and every order of statements; adding or renaming a part may renumber others. The layout file is keyed by path, so a designator that changes moves no placement. Whether a part is a connector is whether it provides the generic `Connector`, never its name or its designator.
_Avoid_: Reference letter, prefix

**Dielectric**:
The class of a ceramic capacitor, `C0G`, `X5R` or `X7R`: the enum `Dielectric` of `std::parts::passive`, the type of a `Capacitor`'s `.dielectric` and of the optional `dielectric` column of the capacitor catalog (ADR 0055, proposed). A design states it with `.dielectric &= Dielectric::X7R`. Read from the class that JLCPCB's record prints for the row; a generic capacitor has none.
_Avoid_: Temperature code, temperature characteristic, material

**Dielectric layer**:
One prepreg or core of a _Build-up_, a `DielectricLayer` with its kind, glass style, thickness and relative permittivity. Not a capacitor's _Dielectric_.
_Avoid_: Dielectric (that is a capacitor's class), substrate

**Diff pair**:
Two nets whose names differ only in a pair of suffixes (`dp` and `dm`, `D+` and `D-`, `_P` and `_N`, `.p` and `.n`, and the like: `netsmith_geom::PAIR_SUFFIXES`), such as the nets of the fields of `std::bus::Usb2`. Nothing in the source marks one: the names say it. When a try of the router has routed every line, it routes each net of a pair again beside the other and keeps that copper only where more of it runs coupled at no more length or vias; the route scorecard reports each pair's skew and how much of it runs coupled (issue #431).
_Avoid_: Differential pair marking, pair class

**Draw**:
The current taken from a net: by a rail through its `pos` pin on behalf of whatever is behind it, or by a chain through its first part. Positive when it draws, negative when it supplies. A rail's current comes back at its `neg` pin and a chain's at its end, as a draw of the opposite sign. A design states the draws of loads; a supply's draw follows from the balance at its net. A capacitor draws nothing at DC.
_Avoid_: Load current (as a field name), sink, source

**DXF import**:
`netsmith dxf import <file.dxf>`, which reads the outline, the loops inside it and the circles on a holes layer from an enclosure's ASCII DXF and prints an `Outline`, `Cutout`s and `MountingHole`s as source to paste; `--write` puts them in the `case` section of the physical file (ADR 0045, ADR 0048). An outline that is not a rectangle with equal rounded corners is printed as a `polygon`; a cutout of another shape than a rounded rectangle, a slot or a circle is refused.
_Avoid_: Enclosure read
