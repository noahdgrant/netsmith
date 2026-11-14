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

**Test current**:
The current a logic level was measured at, written after the value: `.voh &= .vdd.voltage - 0.4V at 2mA`. The statement narrows or constrains the level as it does without `at`; the checker records the current beside the level, and a later analysis derives the pin's output resistance from it (ADR 0076, amendment 1). Only `voh` and `vol` take one, and a level has one.
_Avoid_: Load current (a part's `.current` is its draw), drive current

**Tie**:
`tie .sense ~ .vout.pos` in a part: the two pins must be on one net, which the board makes. It is skipped in a package that lacks either pin.
_Avoid_: Short, jumper

**Tile**:
A square of the board, 1 mm a side (ten cells of the router's grid), that the coarse plan routes over. Each edge between two tiles holds as many tracks on each layer as the pads, keepouts and the board edge leave room for, and each tile as many vias. Not a cell of a panel.
_Avoid_: region, bin, gcell

**Transferred bound**:
A bound on a member that a part's field is held to (`.top.resistance &= .balance`, `.balance >= 20kohm`), read as a bound on the field too, because `&=` puts the field inside the member and a bound on all of the member is a bound on every part of it (ADR 0065). The solver adds it as a relation of its own while it picks and checks the parts, and takes it out of the model it returns. It adds a refusal and never a value.
_Avoid_: Propagated bound, inherited bound, derived bound (ADR 0057 uses that for a bound on a value worked out from a part)

**Type parameter**:
A part that a module takes, written after its name with the generic that bounds it: `module Buck<Ic: BuckIc>` (ADR 0115). Inside the module it is the type of a member (`.ic: Ic`) or the argument of another module (`Buck<Ic>`), and a path through such a member reaches only what the bound declares. A board names the part, `Buck<Lt8641>` (a **type argument**), or uses a generic the module provides, and then each provider of the bound is a candidate (`Buck<Lt8641>`, `Buck<Zbk2>`).
_Avoid_: Template, generic module (a _Generic_ is a definition, not a module with a parameter), wrapper per IC
