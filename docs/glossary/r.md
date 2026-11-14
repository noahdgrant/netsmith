# Glossary: R

**Rail**:
The strip round a panel's grid, 5 mm to 8 mm, that the panel is held by; it carries the tooling holes and fiducials and is cut off along its own V-cut line.
_Avoid_: Frame, border, tooling edge

**Rail**:
The interface for a DC supply: a positive and a negative pin and the voltage between them.
_Avoid_: Power, supply

**Range end**:
`min(.f)` and `max(.f)` (ADR 0098, proposed): the least and the greatest value of the range of the field `.f`, each as one number of the field's own kind, written in a `<=` or `>=` statement (`min(.bias) <= max(.zener_voltage)`). The comparison is then made on two numbers, so it says "can hold for some value", which a plain `<=` ("for every value") cannot. A range end is read and never solved for: it narrows nothing, and a relation that holds one waits, listed as not evaluated, while the field has no value. Not the smaller of two values, as `min(a, b)` is in other languages: it takes one field.
_Avoid_: Bound function, lo and hi (the names the owner may choose instead), a range with values at its ends (that is `lo..hi` with values for `lo` and `hi`, a different thing)

**Range with values at its ends**:
A range whose ends are values, `0A .. .max_supply_current` (ADR 0066): a member or an expression over members, read when both have a value, as the range from the low end's lowest value to the high end's highest. While an end has no value the statement claims nothing and is listed as not evaluated. It is how a part states its own draw from a catalog column. A tolerance (`+/-`) still needs numbers.
_Avoid_: Computed range, dynamic range

**Ratsnest**:
The connections a board still needs, drawn as straight lines between pads, before any track is routed.

**Ref comparison**:
What the editor says changed in the circuit between a git ref (the base) and the files on disk (the head), or between two refs (the head is then the second ref, which is read as the first is and leaves the files on disk unread): the parts, nets, rails, ports, bill of materials and problems that differ, and what changed in the systems and on the PCB (where a part is in the layout file, the outline, the layers), each with its source line on both sides (ADR 0059). A part is the same on both sides by its path in the board, a net by its name or its pins, a rail by its net; a part that only has another designator is renumbered, not changed. It reads the ref with `git archive` and never touches the work tree. Not the _Netlist comparison_ (two designs that share no names) and not a text diff.
_Avoid_: Diff (a diff compares lines), git diff, branch diff

**Reference part**:
The part a schematic sheet's coordinates start at: the largest part of the sheet (most pins, then the largest symbol, then the first by path), put at the origin of the sheet's drawing. What grows round it when a part is added leaves it where it was, so the part and what did not change keep their coordinates. The sheet's box (`data-bounds`, the `viewBox`) may start below zero because of it. `netsmith_view::stability` compares two drawings of a sheet in the frame of the reference part they share.
_Avoid_: Anchor (the engine's word for a main part with parts round it), origin part

**Regulator (at DC)**:
A part that states `dc linear_regulator` or `dc switching_regulator`: an ideal source on the net of its output rail, over the range of the rail's voltage, and an input rail that draws what the output asks (what it supplies and its **quiescent current** for a linear one, `Pout / (efficiency * Vin)` for a switching one, whose efficiency is a fraction above 0 and at most 1: anything else is an error). It cuts the network into the island behind it and the island in front of it (ADR 0076, amendment 3).
_Avoid_: Converter model, power stage (a `BuckStage` is the module around the IC, and the IC is the regulator)

**Relation**:
A constraint that mentions other fields, so it waits for the solver instead of narrowing a field now. `==` states an equation that must hold in the worst case; `>=` and `<=` state bounds. There is no plain `=` (ADR 0018).

**Relation between parts**:
A requirement on two parts together, written in the physical file as the equation of two `.place` fields (`.radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm`, ADR 0045; header of `crates/netsmith-pcb/src/constraints.rs`): the measure of one part's body minus the measure of the other's, along one axis, lies in a window. The placer holds it and moves the two parts together; `fab` measures it as a `constraint` finding that names both parts, the offset, the window and the line. It is no bound on either part.
_Avoid_: Constraint between parts, pitch rule, offset constraint (an `Offset` is how the design carries it)

**Reproduction**:
A port's board drawn by `fab` from the routes file and the layout file that `netsmith routes import` writes from the original's KiCad board, with any other file it needs (a physical file that lets a part hang over an edge), kept in `reference/agent/` of the example and not in `src/` (issue #599). Its copper and its zones are the original's, which `crates/netsmith-cli/tests/reproduce.rs` checks shape by shape and zone by zone; what it lacks (a pad netsmith's footprint draws otherwise) is named in the example's README. The example's own build, with no routes file, is the **automatic board**: the placer's and the router's, compared with the reproduction and the original as they improve.
_Avoid_: agent board, golden board, copy (alone)

**Reverse peak**:
The `.reverse_peak` of a `Rail`: the highest voltage the rail reaches the wrong way round, a positive number, which a design states for its input (ADR 0091). The first part of a `~>` chain from the rail that has a `.supply_reverse_peak` (a `Diode`, a `Schottky` or a `Tvs`) is given it and checked against it: a reverse-polarity diode must block it, and a unidirectional TVS diode conducts it. A rail that states none checks nothing and lists nothing.
_Avoid_: Negative peak, reverse transient

**Role**:
A part's end of a bus: a variant of the bus's enum (`SpiRole::Slave`), stated with `&=` on the interface by the part or by the board (`.spi.role &= SpiRole::Slave`). The checker compares the part's pin table with the lines the role has, and counts the role against its `limit`. A part that states none is judged by the kinds of its table alone (a connector that the bus goes through states a **Pass-through**), and an interface of a bus with roles that is joined to another end and states none is a guideline finding, `bus role` (ADR 0064, ADR 0030). `~` between two interfaces does not equate it.
_Avoid_: Mode, direction, master/slave (they are the variants of `SpiRole`, not the name of the construct)

**Rounded bend**:
A bend of a track drawn as an arc tangent to the two straight tracks it joins, in place of the 45 degree corner (issue #431). The router draws one once a board is routed, of the largest radius up to four track widths that keeps every space, and leaves the corner where none fits; the board file writes it as KiCad's `(arc (start) (mid) (end))`.
_Avoid_: Curved track, fillet (a teardrop's old name), mitre (a corner cut straight)

**Route robustness**:
Whether a board routes for more than one lucky set of the router's costs: its placed board routed again in one try with no time limit at the turn costs 24, 25 (the router's own), 26 and 27, and the connections each leaves (`netsmith route --tries 1 --route-timeout 0 --turn T`). `scripts/route-scorecard` prints it for each ported board beside the route scorecard, and a change to the router is judged by all four, since one unit of a cost re-rolls where the tracks go (issue #431, S3b).
_Avoid_: Stability (the placer's), determinism (the same input always gives the same copper; that is a separate property), variance

**Route scorecard**:
The numbers that say how good a board's routing is, read from its `.kicad_pcb` and `.kicad_pro` (`netsmith_fab::Scorecard`), so that a board netsmith routed and one a person routed in KiCad are measured by the same code: connections left, track length against the shortest tree between each routed net's pads, vias, bends by angle, track narrower than its net class, how much other nets' track cuts each pour and the pieces it falls into, decoupling track length and the skew of diff pairs. A measurement and never a check: `scripts/route-scorecard` prints it for the example boards beside their references (issue #431).
_Avoid_: Score (alone), quality (the placer's `Quality` is a measure of a placement), benchmark

**Routes file**:
`<stem>.routes.toml` beside a source file and its _Layout file_: copper a person, an agent or an import fixed, one record for each net of a board with its tracks (straight or arcs, on the outer layers), its vias, its _Zones_, a hash of its pads and a lock (issue #599; the format ADR 0030 sketched). `fab`, the editor's PCB view and `netsmith route` draw it on the placed board before the router, which keeps it and routes only what it leaves; a locked net gets no copper from the router. A record whose pads moved, or for a net the board does not have, is a warning and is not drawn (its locked shapes aside). Optional: with none, every board is routed as before. Written by `netsmith routes` (`import`, `add-track`, `add-via`, `delete`, `lock`, `unlock`) and by the editor's PCB view (which draws and drags tracks and vias with the same code), never by `fab`.
_Avoid_: routes cache (ADR 0030's sketch: the file holds decisions, not a cache), fixed routes file, track file

**Row statement**:
`row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`, in the physical file only (owner decision 2026-11-04, ADR 0048): a numbered run of parts (the first and the last name it: the same path, a number at the end of the last segment, every number between present) on one line, each neighbour at the pitch along the axis. It lowers to the _Relation between parts_ of each pair of neighbours (the pitch, and the other axis equal), so the placer and the check evaluate it with no new mechanism, and a finding names the statement's file and line and the two neighbours.
_Avoid_: Pitch rule, array, repeat, pattern (a row is the statement; a repetition construct is not built)
