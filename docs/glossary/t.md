# Glossary: T

**Teardrop**:
Copper that widens a track where it meets a pad or a via, so that a drill that wanders or an etch that undercuts does not cut the track off (issue #431). The router adds one to each such joint where it keeps every space the design rules check asks of a track, and the board file holds it as KiCad 9's teardrop zone, filled.
_Avoid_: Fillet (KiCad's old name for it), pad flare

**Temperature range**:
The lowest and the highest temperature a part works at: its built-in `.temp_min` and `.temp_max`, which its catalog row gives in the fixed columns `temp_min` and `temp_max` when the design states none, each one temperature as the datasheet prints it. A row that states none is offered and named, never passed (ADR 0099, provisional), unless it names a dielectric class that has a cited range, which then judges the ends its cells leave empty: that is the _class's_ range and not the part's, and the check's messages say "from the class X7R". A figure that is a junction range (a diode's `TJ`) is accepted as the part's range and the part is judged by it; the part's own heating is not computed, which is the limit of the check. It is not the part's rating at 25 C (`.max_power`), and not the board's `.environment.temperature`, which the part's range must cover.
_Avoid_: Operating range, thermal limits, rating (for the range)

**Temperature rise**:
How far above the board a loaded track may heat, `.process.max_temperature_rise` in kelvin, an upper bound. The track width check designs to it; it is 10 K when the board states none, a default of netsmith's and not a number a fabricator publishes (ADR 0036).

**Temperature shift**:
How far the temperature of a design goes from the 25 C a datasheet states, in kelvin, as a range (`.z.temperature_shift &= -65K..100K` is -40 C to 125 C). The member `.temperature_shift: Temperature optional` of a `Zener` (ADR 0080): a design that states it has `.voltage_over_temperature`, the Zener's voltage at the hot and the cold corner, from the picked row's temperature coefficient; a design that states none gets the shift of its board's `.environment.temperature` (less the 25 C) when it bounds `.voltage_over_temperature`, and one that does neither has no such relation. Not an absolute temperature, and not the rise of a track or a part (`.max_temperature_rise`).
_Avoid_: Absolute temperature, delta T (the range of temperatures a board's surroundings reach is its _Environment_, and a part's is its _Temperature range_)

**Terminal**:
A pin that belongs to a part, directly or through one of its interfaces. Terminals are what an exporter writes out.

**Terminal pad**:
A pad of a footprint on a copper layer, where a _Terminal_ can be soldered: what a count of pads, a pitch, an escape or a distance to a pad measures. A **paste aperture** (a pad with paste and no copper, such as a window of an exposed pad's paste) is none; the paste layers, their Gerbers and the drawings keep it (`netsmith_geom::is_terminal_pad`, issue #1693).

**Termination**:
A part across the two nets of a _Diff pair_ that matches the line's impedance at its end, so that a level does not reflect: a part with two pads, no connector, one pad on each net (a `std::bus::CanTermination`'s 120 ohm resistor across a CAN bus's `canh` and `canl`). Derived from the netlist, as a decoupling capacitor is: the placer pulls it to the pads of the connectors that have a pad on both nets, where the line leaves the board (`netsmith_pcb::termination_pairs`, issue #1193).
_Avoid_: Terminator, terminal (a pin of a part is a _Terminal_)

**Test current**:
The current a logic level was measured at, written after the value: `.voh &= .vdd.voltage - 0.4V at 2mA`. The statement narrows or constrains the level as it does without `at`; the checker records the current beside the level, and a later analysis derives the pin's output resistance from it (ADR 0076, amendment 1). Only `voh` and `vol` take one, and a level has one.
_Avoid_: Load current (a part's `.current` is its draw), drive current

**Thermistor window**:
The cell temperatures a charger charges between, read by the thermistor on its temperature-sense pin: `Charger.thermistor_window` (issue #1262), which must lie within the charge window of the cell it charges (`Cell.charge_temperature`).
_Avoid_: NTC window, JEITA range

**Tie**:
`tie .sense ~ .vout.pos` in a part: the two pins must be on one net, which the board makes. It is skipped in a package that lacks either pin, and the solver does not choose a package that has both pins of a tie the board does not make.
_Avoid_: Short, jumper

**Tile**:
A square of the board, 1 mm a side (ten cells of the router's grid), that the coarse plan routes over. Each edge between two tiles holds as many tracks on each layer as the pads, keepouts and the board edge leave room for, and each tile as many vias. Not a cell of a panel.
_Avoid_: region, bin, gcell

**Track drop**:
The voltage a power net's routed tracks drop: the most any of its pads sees from a source pad, each run of track at the current the pins past it draw, the net's worst-case current where a pin of no known current is among them (issues #652 and #1541). `fab` measures it once the board is routed (and `board nets` and `board drc` on the copper a board has, issue #1540) and warns, as the guideline `track drop`, when it is more than 1 % of the voltage of the net's rail.
_Avoid_: IR drop, voltage loss, track loss

**Track impedance**:
The characteristic impedance of a track on one layer of a board with a _Build-up_, worked out from the thickness and relative permittivity of each _Dielectric layer_ and the copper's thickness by closed forms (`netsmith_pcb::impedance`, issues #759 and #1007): microstrip on an outer layer, under the solder mask where the process states JLCPCB's calculator parameters, and stripline on an inner one; for two tracks side by side (an edge-coupled pair), the even and odd modes, and the differential impedance as twice the odd mode's. `fab-notes.txt` lists it for each layer a track may be on: the width of a 50 ohm track and the impedance of the router's width, and for each _Impedance target_ the design states, the width that gives it and the impedance of the width the router draws.
_Avoid_: Trace impedance; controlled impedance and impedance control for the figure (those are the fabricator's service of making a board to an impedance and testing it)

**Track loss**:
What the tracks of a _Track path_ lose at its `.frequency`, in dB (issue #1405): the copper's skin-effect loss and the dielectric's, from the build-up's loss tangent (`.stackup.loss_tangent`), worked out on the routed board for each track and added up (`netsmith_pcb::impedance::loss_per_mm`). The parts in series are not counted. A build-up with no loss tangent recorded, which is every one of the standard library, has none.
_Avoid_: Insertion loss (it counts the parts), attenuation

**Track overflow**:
The placer's estimate of how far a placement asks more track of the board than it holds (issue #820): the board cut into square bins of 1 mm or more, each net of ten pads or fewer that is not a plane spreading its half perimeter of track evenly over the box round its pads, each pad taking the room of its copper, and every bin holding track on each signal layer (the two outer layers and every inner layer that is not a plane) at the process's track pitch over half its area; the overflow is the sum, over the bins, of what each is asked beyond that, in mm of track. The annealer weighs it only when its caller asks (`PlaceOptions::track_overflow`, 0 by default), because on the boards it was measured on it did not leave the router clearly fewer lines (`anneal/overflow.rs`).
_Avoid_: congestion (the router's negotiation over cells), crowding (the coarse plan's measure of a tile), demand

**Track path**:
One line of one impedance from a pin to a pin through the parts in series on it, a `std::sources::TrackPath` (issue #1275): an antenna line through a DC-blocking capacitor is one path of two nets. Each net of its chain takes its _Impedance target_ and is an _RF net_; its length budget and its _Track loss_ budget are over the whole chain, and it may state mitered corners and a fence of ground vias.
_Avoid_: RF chain, transmission path

**Transferred bound**:
A bound on a member that a part's field is held to (`.top.resistance &= .balance`, `.balance >= 20kohm`), read as a bound on the field too, because `&=` puts the field inside the member and a bound on all of the member is a bound on every part of it (ADR 0065). The solver adds it as a relation of its own while it picks and checks the parts, and takes it out of the model it returns. It adds a refusal and never a value.
_Avoid_: Propagated bound, inherited bound, derived bound (ADR 0057 uses that for a bound on a value worked out from a part)

**Turn step**:
The step in degrees by which the placer may turn a part, its `.place.turn_step` in the board's physical file (issue #1046): 90 unless stated, so a part takes 0, 90, 180 or 270 degrees from the turn it starts at, or 45 (`.r1.place.turn_step &= 45`), so the annealer tries the eight turns and a part that fits the outline in no right angle starts at the first that fits. It is a part's statement, never a module's or a footprint's; a requirement on the rotation still limits the turns, and a part the layout file locks keeps its own.
_Avoid_: rotation step, angle grid, snap angle

**Type parameter**:
A part that a module takes, written after its name with the generic that bounds it: `module Buck<Ic: BuckIc>` (ADR 0115). Inside the module it is the type of a member (`.ic: Ic`) or the argument of another module (`Buck<Ic>`), and a path through such a member reaches only what the bound declares. A board names the part, `Buck<Lt8641>` (a **type argument**), or uses a generic the module provides, and then each provider of the bound is a candidate (`Buck<Lt8641>`, `Buck<Zbk2>`).
_Avoid_: Template, generic module (a _Generic_ is a definition, not a module with a parameter), wrapper per IC
