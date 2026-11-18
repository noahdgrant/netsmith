# Glossary: L

**Label**:
A part that prints text on the silkscreen (`std::parts::mechanical::Label`). It has no pins and is fitted by nobody. A part's own silkscreen text is not a label (_Silkscreen text_).
_Avoid_: Text, annotation, legend

**Layout block**:
`layout { source "..." near ... }` in a part: what its datasheet says about where the parts around it go, with the document and section in a mandatory `source` (ADR 0030, issues #829 and #994). Its statements name the part's own pins and find the board's parts from their nets (`bridging .a .b`, `on .pin kind K`): `near` (next to a pin), `away` (at least a distance from one), `loop` (a short current loop through a part across two pins), `keepout` (an area of the part's frame that forbids tracks, vias, copper or parts), `edge` (a pin at the board's edge), `vias` (vias in a pin's pads) and `route` (the width and length of a pin's net's tracks). The placer and the router work towards them, and `fab` gives a _Guideline_ when the board does not meet a number the datasheet gives, unless the _Layout file_ waives it with a reason. `layout "SOT-223" { }` holds the guidelines of one package of the part. A catalog row of a standard generic has its block in `std/layouts.nsm`, `layout "MPN" for LinearRegulator { }`, keyed by the row's manufacturer's part number and kept in the design when the solver picks that row. It is knowledge of the part, not a decision of the design: a design's positions are in the _Layout file_, and a requirement of the board is in its physical file (_Placement requirement_).
_Avoid_: Layout hint, layout constraint, layout rule (a rule of the fabricator is a design rule)

**Layout file**:
`<stem>.pcb.toml` beside a source file: where parts (keyed by their path in the design), keepouts and cutouts are, when the source does not say, with rotation, side and a lock, and the _Waiver_ of a _Layout block_'s guideline. Optional, written by an editor or a placer, never by the language (ADR 0032); `netsmith layout` shows, sets and checks its entries. It holds the solution; a requirement is in the physical file (_Placement requirement_, _Physical_).
_Avoid_: Sidecar (the word for what it is, not its name), placement file (that is the pick-and-place file for the board house). The schematic has its own, the _Schematic layout file_.

**Leakage (`Ii`)**:
The most current a logic pin takes or gives when it is high impedance, at any voltage between its ground and its supply, stated by a part as the optional member `.ii` of its `Logic` or `Levels` (`.io.ii &= 1uA`; ADR 0076, amendment 8). The analysis takes it as a resistance from the pin to its supply and to its ground, the greatest supply voltage over the current, each a state of the pin. A pin that states none leaks nothing. The output resistances `.roh` and `.rol` are the other optional members.
_Avoid_: Input current (a part's `.current` is its draw), bias current, input impedance

**Level translator**:
A `LevelTranslator` (`std::parts::logic`, ADR 0063, ADR 0066): a single-supply buffer whose input accepts a lower logic level than its supply and whose output swings to its supply. Its input is a `Logic` line whose `.vih` is `.vt_plus_max` (above it the input is a high for sure) and whose `.vil` is `.vt_minus_min` (below it a low for sure), so the solver judges the driver without a statement of the design; its output is a `Logic` line that states no level. Its supply draw is the row's own: `0A .. .max_supply_current`. A generic with the catalog `level_translator`, which has no standard row.
_Avoid_: Level shifter, voltage translator

**Line**:
A pin of a bus interface as a role's `lines` table gives it: its kind, and the words `single` (one interface of the role on a net), `crosses` (joined to the other end's other line, a `Uart`'s `tx` to `rx`), `optional` and `required` (whether it must be connected), and `limit N` on the role (ADR 0064).
_Avoid_: Signal (what a net carries), channel

**Line class**:
How important a connection of the router is, derived from the board: escape (to a plane, or deep in a fine-pitch grid), decoupling (a capacitor's pad to the pin it decouples), pair (a net of a diff pair), clock (a crystal's or a clock-named net), power (at least 100 mA), and the rest, in that fixed order. From the router's second try of a board the lines are routed class by class, and the copper of a decoupling, pair or clock line is never ripped up for a line of a later class (`Class` in `netsmith_router::route`, issue #416).
_Avoid_: Net class (KiCad's, the width and clearance the `.kicad_pro` gives a net), priority, importance (alone)

**Link**:
How the editor's schematic draws a chain of series parts between two main parts, with no IC: the fuse and the reverse-polarity diode between a vehicle connector and a header. It is the spine's row without the regulator (ADR 0072): from left to right the **source** (the main part whose end of the chain has no parts to ground on it, so the connector the power comes in at), the series parts turned along the row, and the **load** (the main part at the other end), standing level with the row so that the chain runs into its pin as one wire. The parts to ground on a node (a TVS diode) hang from it. A chain is a link only when a supply rail is at each end of it and no rail on it feeds other series parts as well (a rail with pull-ups is not one). The net at the source's end is a plain wire with no power symbol (a symbol that joins nothing else on the sheet reads as a port to somewhere); the net at the load's end has the one symbol, with its name. A source and its load are always in one row.
_Avoid_: Feed, bridge, jumper (a jumper is a part)

**Linter**:
`netsmith project lint` (issue #1104): what reads a source that checks and finds what is worth changing in it as a source, one rule for each kind of finding (`unused_import`, `unused_definition`, `redundant`, `naming`), each with the code `lint.` and its name. It never changes a file. A rule is turned off for a project by `[lint] off` in `netsmith.toml` and for one line by `// lint: off <rule>`. Not the checker, whose errors say a source is wrong, and not a _Guideline_, which judges a design against engineering advice.
_Avoid_: Style check, guideline (a guideline judges the design), warning (as a name: a finding is a warning unless `--strict` makes it an error)

**Load group**:
`shared HEATERS limit 1 { .heater_a .heater_b }` in a board or a module: loads (rails that draw, by path) of which at most `limit` are on at once, one without `limit` (issue #806). A load that is off draws the low end of its range, so the members on one net add up to their low ends plus the `limit` largest spans; the balance of the net, a budget, the DC analysis, the track widths and the power view read that sum, and a message about the net's currents names the group. A load is in one group. It is the statement of a part's shared resource, with loads as its members.
_Avoid_: Duty group, exclusive loads, load set

**Logic level**:
One of the four voltages of a `Logic` line, or of the `Levels` of a bus interface (`.spi.levels`, ADR 0064) or of a part, for its plain pins (`.levels`, issue #1190), as a datasheet gives them and relative to the line's ground (the `neg` of its `.reference` rail): `.vih` the lowest an input reads as high, `.vil` the highest it reads as low, `.voh` the lowest an output drives a high to, `.vol` the highest it drives a low to. After the rails are solved, the solver requires `voh >= vih` and `vol <= vil` for every output and input on a net, and says "not evaluated" when a level has no value or the grounds differ (ADR 0046).
_Avoid_: Threshold, noise margin

**Logo**:
A part that prints a drawing from an SVG or DXF file beside the source on the silkscreen, or etches it in copper (`std::parts::mechanical::Logo`). It has no pins and is fitted by nobody; `.file` names the drawing, `.size` the longer side of its box and `.layer` where it is made. Text is a _Label_.
_Avoid_: Image, picture, artwork, bitmap (a raster file is not read)

**Long net**:
A net no zone pours whose routed track is more than twice its _Straight length_ and at least 2 mm over it (`netsmith_fab::NetRoutes`, issue #1194). `fab-notes.txt`, `board drc` and the _Route scorecard_ list each with its length, the straight length, the ratio and its vias (`routed long:`), beside the nets with the most vias (`most vias:`). A note and never a check: a net that must be short states its limit in a part's `layout` block (`route NAME .pin length LENGTH`), which `fab` checks.
_Avoid_: Detour (the scorecard's ratio of all routed nets), meander (a length a diff pair adds on purpose), long track
