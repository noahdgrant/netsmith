# netsmith

A language and toolchain for describing circuits as text and turning them into files for CAD tools.

## Language

**Board**:
One physical PCB, described as a manufacturing process, parts, and the modules placed on it.
_Avoid_: Design, PCB

**System**:
Several boards and the joins between their ports, written `system Demo { .main: MainBoard  .io: IoBoard  .main.output ~ .io.input }`. A system holds only boards and says which ports meet, or which connectors are mated; it names no nets. Each board is still made and ordered on its own.
_Avoid_: Assembly, backplane, harness

**Mate**:
Two connectors that a system joins with `~` between their `.mating`: contact N of one meets contact N of the other (`.carrier.slot.mating ~ .card.j1.mating`, ADR 0040). The checker reads what each board has behind each contact (ground, power, a signal, or open), refuses what cannot meet (a header with a header, two outputs, ground against power, a count or pitch that differs), and joins the ports the contacts carry, so the same fact is not said twice with a port join.
_Avoid_: Connection, link, plug

**Ground group**:
The ground nets of boards that mated ground contacts join, as the system view shows them (`carrier.GND + sensor.GND`). Each board keeps its own ground net; the system never merges them, it reports which ones are one ground. A mate that carries power or a signal must carry a ground contact.
_Avoid_: Common ground, ground plane

**Order**:
A named list of boards and quantities in `netsmith.toml` (`[order.devkit.boards]`, each key a board as its folder is named, `Board` or `System/member`, a system for all its members, or a panel), made by `netsmith fab --order devkit` into `fab/orders/devkit/`: the package of each board, an `order.txt` summary and a combined bill of materials for the quantities (ADR 0035). It chooses which separate packages are ordered together; a panel's quantity counts panels.
_Avoid_: Batch, job, build set

**Panel**:
A `panel` definition: one board repeated in a grid on one fabrication panel, cut apart by V-cuts, `panel P { .cell: Board  .fab: VCutPanel { .columns &= 3  .rows &= 2  .rail &= 5mm } }` (ADR 0040). The board stays a stand-alone `board`; the panel names no nets and joins nothing. `fab` writes one Gerber set, drill file, bill of materials and placement file for the whole panel (designators carry the cell, `R1_3`), and the panel as a `.kicad_pcb`. The board house's rules for it are the `.fab` member, as a board's process is its `.process`.
_Avoid_: Array, step and repeat, panelisation (the word for the act of making one)

**Cell**:
One board of a panel's grid, counted along the rows from the top left.
_Avoid_: Slot, tile

**V-cut**:
A line cut part way through the panel from both faces, straight across the whole panel, along which the boards are broken apart. In the Gerbers the lines are on `Edge.Cuts`, marked "V-CUT" (JLCPCB's instruction); in the panel board file KiCad calls them a malformed outline.
_Avoid_: V-score, scoring

**Rail**:
The strip round a panel's grid, 5 mm to 8 mm, that the panel is held by; it carries the tooling holes and fiducials and is cut off along its own V-cut line.
_Avoid_: Frame, border, tooling edge

**Process**:
The design rules and stackup of the factory that makes a board, such as `std::mfg::jlcpcb::TwoLayer`. A board has one as its `.process` member. Each rule is the least the factory can make, written as a range the board may narrow upward; the rule used is the bottom of what is left. It is an interface with no pins.
_Avoid_: Fab house, rule set, technology

**Via in pad**:
A via whose centre is on a surface-mount pad. The fabricator fills it with epoxy and caps it with plating so that the solder stays on the pad; the process says up to what size it does that (`.via_in_pad`), and `fab-notes.txt` lists the pads that have one. The router uses one only when a pad cannot be reached any other way.

**Neck-down**:
A track drawn at the process's least width for a short run where the board's wider track does not fit, such as between two pads or for the first millimetre out of a fine-pitch pad. The router measures the finished copper of every line against the other nets' copper, and a line whose copper is too near at any width is not routed (ADR 0036).

**Side**:
The face of the board a part is on, front or back. A part on the back is its footprint seen in a mirror, with its surface-mount pads on the back copper.

**Stackup**:
The layers a board is built from: the number of copper layers, the finished thickness, the copper weight and, for a board with inner layers, the prepreg and core between them. It is a member of a process, `.process.stackup`.
_Avoid_: Layer stack

**Plane**:
An inner copper layer that is one sheet of copper on one net, such as ground. The board names the net (`.process.inner1.net &= "GND"`); a through-hole pad or a via on that net joins it where it stands, and any other pad is joined by a via. The router draws signals on the outer layers only.
_Avoid_: Copper fill, power layer (a pour is a different thing)

**Pour**:
Copper filled on an outer layer round the tracks and joined to one net, such as ground: `.process.top.net &= "GND"`. Unlike a plane it leaves room for the tracks, KiCad fills it, and vias at a pitch the board states (`.stitch_pitch`) join it to the other layers of its net.
_Avoid_: Copper fill, ground plane (that is a plane)

**Net class**:
The width the tracks of a net are drawn at. A rail may state it (`.min_track` on a `Rail`, an override; a net takes the widest of its rails). A net no rail states a width for is drawn at the width IPC-2221 needs for its current (the sum of the loads on it or returning through it, all on at once), when the solver knows every current on it; otherwise at the process's least, and a note names the net. Not a list of nets; the rail names them. A stated width is checked against the net's current by IPC-2221 (ADR 0036).
_Avoid_: Track class, width rule

**Guideline**:
Advice that a person decides about, as opposed to a limit of the fabricator. A design that goes against one gets a `warning:` and still builds; `guidelines = "error"` in `netsmith.toml`, or `--strict` for one run, makes it an error. The first is the track width against current (ADR 0036); the datasheet layout rules of ADR 0030 are the next.
_Avoid_: Lint, soft rule

**Temperature rise**:
How far above the board a loaded track may heat, `.process.max_temperature_rise` in kelvin, an upper bound. The track width check designs to it; it is 10 K when the board states none, a default of netsmith's and not a number a fabricator publishes (ADR 0036).

**Outline**:
The board edge, the board's member `.outline` (`std::pcb::Outline`): a width, a height and a corner radius. Without one the board is as big as its parts need. The origin of the board's frame is its top left corner.
_Avoid_: Board size, board shape

**DXF import**:
`netsmith dxf import <file.dxf>`, which reads the outline, the loops inside it and the circles on a holes layer from an enclosure's ASCII DXF and prints an `Outline`, `Cutout`s and `MountingHole`s as source to paste; `--write` merges their places into the layout file (ADR 0045). An outline that is not a rectangle with equal rounded corners is refused.
_Avoid_: Enclosure read

**Cutout**:
A rectangular hole inside the outline, with an optional corner radius (`std::pcb::Cutout`).
_Avoid_: Slot, notch, hole (a hole is drilled)

**Keepout**:
An area of a board, or of a part in its own frame, where tracks, vias, copper pours or parts are not allowed (`std::pcb::Keepout`), on the layers it names. It is a KiCad rule area in the board file.
_Avoid_: Exclusion zone, restricted area

**Height limit**:
The tallest a part may stand where a keepout is (`std::pcb::Keepout`'s `.max_height`, provisional, ADR 0045 section 11), on the faces the keepout's `.layers` name. A part states how tall it is with `.height`; one that does not has an unknown height, which is reported by name and never taken as zero. The check measures the part's body against the zone; the placer keeps the body, plus a small margin, out of it (owner decision 2026-11-04); a part with no fabrication outline has only its courtyard.

**Placement requirement**:
A range on where a part, or the parts of a module, may go, stated in the board's physical file on the built-in `.place` of the part or module (`.usb.place.left &= -0.5mm +/- 0.05mm`, ADR 0045, ADR 0048). The fields are `x`, `y`, `left`, `right`, `top`, `bottom`, `rotation` and `side` (on a module, `side` is a requirement on every part of it). It comes from the case, panel or carrier, not from the part, so a board states it. The placer keeps to it, and `fab` stops with the numbers if the board does not meet it (`constraints = "warn"` or `--constraints warn` lists it instead; `--constraints error` overrides the manifest, and `--allow-drc-violations` does not waive it). The solved position is not in the physical file: it stays in the layout file. The same line in a board's circuit source is an error that names the physical file.
_Avoid_: Constraint (as the name of the thing a person writes: a finding is a `constraint` finding), position (a position is what the layout file holds)

**Relation between parts**:
A requirement on two parts together, written in the physical file as the equation of two `.place` fields (`.radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm`, ADR 0045 sections 3 and 12, provisional): the measure of one part's body minus the measure of the other's, along one axis, lies in a window. The placer holds it and moves the two parts together; `fab` measures it as a `constraint` finding that names both parts, the offset, the window and the line. It is no bound on either part.
_Avoid_: Constraint between parts, pitch rule, offset constraint (an `Offset` is how the design carries it)

**Row statement**:
`row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`, in the physical file only (owner decision 2026-11-04, ADR 0048): a numbered run of parts (the first and the last name it: the same path, a number at the end of the last segment, every number between present) on one line, each neighbour at the pitch along the axis. It lowers to the _Relation between parts_ of each pair of neighbours (the pitch, and the other axis equal), so the placer and the check evaluate it with no new mechanism, and a finding names the statement's file and line and the two neighbours.
_Avoid_: Pitch rule, array, repeat, pattern (a row is the statement; a repetition construct is not built)

**Physical**:
What the case, the product and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector, antenna or sensor must be, a spacing or alignment between two parts, a module's face, a region with a height limit, a pattern of test pads, keepouts a board draws, local pours. It is requirements, never solutions. It is written in the **physical file** of a board, `<stem>.physical.nsm` beside the source, as `physical for Board { ... }` with a nested `case { ... }` section for the outline, cutouts, mounting holes, fiducials and labels (owner decisions 2026-11-04, ADR 0048). Its statements are the language of a board's body, and a path in it may cross a module boundary, only to state physical intent. The circuit source keeps what the circuit's checks consume (process, stackup, plane nets, assembly, designator letters, currents, impedance, pairs). The word `physical` is not a definition kind: `physical for` begins a unit, and `physical intent` is the thing it states.
_Avoid_: PCB constraints (it is not the circuit's constraint and not a result), sidecar (the layout file is the sidecar)

**Overhang**:
A part whose courtyard is past an edge of the outline on purpose, such as a plug that sticks out of the case. The design says it may (provisional, ADR 0045, section 10): a requirement on the part's own `.place.left`, `.right`, `.top` or `.bottom` whose whole window is at or beyond the same edge of the outline, A lock in the layout file does not say it: a locked part states every edge it is past, like any other part (owner decision, 2026-11-03). Then the courtyard past that edge is not an error and counts as clipped to the outline, also in the placer's overlap cost; pads are still checked, an edge not stated is still an error (also for a locked part), and `fab` prints each overhang as a warning and lists it among the warnings in `fab-notes.txt`. The body (`F.Fab`) is not checked.
_Avoid_: Hang-over, protrusion, outside the board (an undeclared overhang is the error `not wholly inside the board outline`)

**Body**:
The box of a part that a placement requirement measures: the outline its footprint draws on the fabrication layer (`F.Fab`), else its courtyard, after the part is turned. Not the footprint's origin, which the layout file's `at` is.
_Avoid_: Footprint box, bounding box

**Label**:
A part that prints text on the silkscreen (`std::parts::mechanical::Label`). It has no pins and is fitted by nobody.
_Avoid_: Text, annotation, legend

**Layout file**:
`<stem>.pcb.toml` beside a source file: where parts (keyed by their path in the design), keepouts and cutouts are, when the source does not say, with rotation, side and a lock. Optional, written by an editor or a placer, never by the language (ADR 0032). It holds the solution; a requirement is in the physical file (_Placement requirement_, _Physical_).
_Avoid_: Sidecar (the word for what it is, not its name), placement file (that is the pick-and-place file for the board house)

**Derived placement**:
The placer's way of choosing where parts go: from the circuit alone, with nothing written in the source. It keeps the parts of a module instance together, puts a decoupling capacitor next to the power pins it bridges, connectors at the edges and wire short, turns parts by right angles, and keeps reference text clear of other parts, all by simulated annealing with a seeded random generator (ADR 0033).
_Avoid_: Autoplace, autorouter placement

**Decoupling pair**:
A two-pad capacitor whose pads are on two different nets that one same other part has `power` pins on, together with that part. Nothing in the source says so: the netlist and the pin kinds do. Of several such parts the ones that share most of the capacitor's instance path are kept (ADR 0033).
_Avoid_: Bypass capacitor (a bypass capacitor is the datasheet's word for the same part, but only a pair the placer finds is meant here)

**Seed**:
A whole number in the layout file (`seed = 3` in a board's table) that picks which arrangement the placer finds: the same seed always gives the same board, another seed another one (ADR 0033).

**Feedback divider**:
The two resistors that set the output of an adjustable regulator, `std::parts::regulator::FeedbackDivider`: the output is `vref * (1 + rtop / rbottom)` and the solver picks resistors that keep it inside the range the board asks for, with the tolerance of the reference and of both resistors.
_Avoid_: Voltage divider (that is any divider; this one sets a regulator's output)

**Buck stage**:
Everything around a buck converter IC except the IC: `std::parts::regulator::BuckStage`, a module of the inductor, two capacitors and a feedback divider. The IC is a part with a `Buck` interface member that carries its datasheet numbers.
_Avoid_: Buck module, power stage

**Project**:
A directory with a `netsmith.toml` at its root, found from any directory below it. Its `src/` files are modules named by their path (`src/power.nsm` is `src::power`), and the commands with no file argument act on the files its manifest lists.
_Avoid_: Workspace, package, repo

**Skill**:
A directory in `.agents/skills/` with a `SKILL.md`: instructions an agent loads when its description matches the task. `netsmith-*` skills are for working in a netsmith project and `netsmith init` copies them into one; `netsmith-dev-*` skills are for working on netsmith and stay in its repository (ADR 0015).
_Avoid_: Prompt, plugin

**Manifest**:
A project's `netsmith.toml`: its name, the top-level files that hold boards, the output and fab directories, and the KiCad version. It lists what to build and the orders to make from it; it does not describe a circuit.
_Avoid_: Config, settings

**Assembly**:
Who puts a part on the board: the field `.assembly` of every part, `"fab"` (the board house places it), `"hand"` (soldered by hand) or `"none"` (not fitted at all: a hole, a fiducial). It decides the BOM, the placement file and the notes of `fab` (ADR 0024, provisional). A project may set a default for it in `netsmith.toml` (`assembly = "fab"`, ADR 0028). A board cannot reach the parts of a module, so a module that wants the choice offers a `String` value (`.assembly_mode`) and ties its parts to it with `.j.assembly &= .assembly_mode`; the board's narrowing of that value is the design's own statement for those parts (ADR 0042).
_Avoid_: Population, fitting, DNP

**Designator**:
The reference of a part on the schematic, the bill of materials and the placement file: letters and a number, `R1`, `C2`, `BZ1`. The letters are the field `.designator` of every part, stated by its kind (`Resistor` is `"R"`, ADR 0046) or narrowed by a part or board that defines a part with none, and `"U"` when nothing says; the number counts the parts with the same letters, in the order of their paths, where a number at the end of a path segment sorts as a number (`.pads.tp2` is `TP2`, and `tp10` follows `tp9`; owner decision 2026-11-04). The layout file is keyed by path, so a designator that changes moves no placement.
_Avoid_: Reference letter, prefix

**Part**:
A component with pins and quantities, such as a resistor or a regulator.
_Avoid_: Component, device

**Interface**:
A typed bundle of pins and quantities that is connected as one. A bundle of quantities with no pins, such as a process, is also an interface.
_Avoid_: Bus, bundle, group

**Module**:
A reusable sub-circuit that is placed on a board or inside another module. Only its interfaces, pins, and declared values can be used from outside; the parts and modules inside are internal.
_Avoid_: Circuit, block, sheet, subcircuit

**Rail**:
The interface for a DC supply: a positive and a negative pin and the voltage between them.
_Avoid_: Power, supply

**Draw**:
The current taken from a net: by a rail through its `pos` pin on behalf of whatever is behind it, or by a chain through its first part. Positive when it draws, negative when it supplies. A rail's current comes back at its `neg` pin and a chain's at its end, as a draw of the opposite sign. A design states the draws of loads; a supply's draw follows from the balance at its net. A capacitor draws nothing at DC.
_Avoid_: Load current (as a field name), sink, source

**Balance**:
The rule that at a net, in one board or module, the draws there (of rails and of the chains that start or end on it) add up to zero. A net with a rail on its `pos` is a supply net and is solved from; one with a rail only on `neg` is a return net, which is checked and never solved from. A module's own rails count with their sign flipped, because from inside they are the supply of what is inside. In a system the joined ports of boards balance the same way, each as its board writes it. A balance is **complete** when every part pin on the net is a term or belongs to a part that draws no DC current by its model, or is an `input` or `no_connect` pin of a part with a rail (ADR 0027); only a complete balance sets a current. The pins that are neither are its **unmodelled** pins. A current the design stated with a range on a complete balance is a **budget** (ADR 0038): what the others determine must lie inside it, and it then holds that narrower value.
_Avoid_: KCL, current sum

**Pin**:
A connection point of a part, interface, or module.

**Pad**:
The text identifier of a pin that its symbol and footprint share, such as `1`, `A1`, or `EP`. A pin can have several (a regulator's output on pads 1 and 2). The part's pin table gives them, for the package the part is in; a pin has no `.pad` field.
_Avoid_: Number, pin number

**Pin kind**:
What a pin is electrically, from a closed set: `power`, `input`, `output`, `bidirectional`, `passive` and `no_connect` (ADR 0016). The kind decides what the checker asks of the board: a `power` or `input` pin must be connected, a `no_connect` pin must not be, and two `output` pins may not share a net. `optional` and `required` move a pin from its kind's default.
_Avoid_: Pin type, direction

**Pinout**:
The pin table of a part in one package: for each pin its kind, the name the datasheet prints, its pads, and its functions. Written as a `pins "PACKAGE" { ... }` block in the part, or an unnamed `pins { ... }` for a part whose pads do not depend on the package. A pin that a package lacks does not exist in that package. It names no CAD tool (ADR 0002, ADR 0016).
_Avoid_: Pin map, pin assignment

**Alias**:
A pin with no row in the pin table that the part joins to a pin that has one (`.vin.neg ~ .vout.neg`). It is that pin, so it needs no pad of its own and adds no pin to a net in the schematic.

**Tie**:
`tie .sense ~ .vout.pos` in a part: the two pins must be on one net, which the board makes. It is skipped in a package that lacks either pin.
_Avoid_: Short, jumper

**Function**:
One of the things a pin can be, listed with `fn` in its pinout row (`fn GPIO UART1_TX`). A connected pin with more than one function needs the board to choose, `.u.pa2.function &= "UART1_TX"`; there is no default, and one function can be chosen on only one pin of a part. A function may carry its own kind after a colon (`fn GPIO:bidirectional UART1_TX:output`, ADR 0046); the net rules judge a pin by the kind of the function chosen, and the pin's kind is the default for any function without one.
_Avoid_: Alternate function, mux, mode

**Logic level**:
One of the four voltages of a `Logic` line, as a datasheet gives them and relative to the line's ground (the `neg` of its `.reference` rail): `.vih` the lowest an input reads as high, `.vil` the highest it reads as low, `.voh` the lowest an output drives a high to, `.vol` the highest it drives a low to. After the rails are solved, the solver requires `voh >= vih` and `vol <= vil` for every output and input on a net, and says "not evaluated" when a level has no value or the grounds differ (ADR 0046).
_Avoid_: Threshold, noise margin

**Absolute maximum**:
The voltages a pin survives, `.abs_min` and `.abs_max` of the pin, relative to its ground. The solver holds them against the worst-case voltage of the pin's net (ADR 0046).
_Avoid_: Rating (a rating is a field such as `.max_power`)

**Terminal**:
A pin that belongs to a part, directly or through one of its interfaces. Terminals are what an exporter writes out.

**Port**:
A pin of an interface on a module or board. A port only passes a connection along to the terminals behind it. An interface that a board declares directly (`.output: Rail`) is also the port of that board in a system, which can join it to another board's port and reach nothing else of the board.

**Net**:
All the pins joined at one electrical node, with its name. Only a board names a net; an unnamed one is called `net-` and its smallest terminal path.

**Bridge**:
The two terminals of a part that a series chain enters and leaves by.

**Chain**:
A run of series connections from one pin through bridge parts to another. A series chain passes one current, the through-current (`.current`) of each of its parts, taken from the first terminal of the part's `bridge` to the second; the first part's is drawn from the net the chain starts on (ADR 0014).

**Catalog**:
The columns of a list of real parts that a part's fields are matched against when the solver picks one. A catalog may be keyed by a column (`catalog mcu by .mpn`), and then the design names the row. The rows are a CSV file named after the catalog, in the order the solver tries them (ADR 0022); a project adds its own files in `netsmith.toml`.

**Catalog file**:
A CSV file of the rows of one catalog: a header of the catalog's columns (field paths without the leading dot) and the fixed columns `lcsc`, `mpn`, `library`, `price`, `price_breaks`, `stock`, `source`, `date`, `checked`, `name`, `label`, `notes`. `price`, `stock` and `library` are JLCPCB's numbers, entered by hand, and `date` is the day they were read; the solver does not read them and the bill of materials uses them only when no supply provider has the part (ADR 0047). `checked` is the day a refresh last verified the row against LCSC. A row that names a part must have a `source`. The standard ones are `data/jlcpcb/catalog/*.csv` (ADR 0022).

**Catalog refresh**:
`netsmith catalog refresh`, the opt-in command that checks each row of catalog files against LCSC's product record (never JLCPCB): the manufacturer's number and package. A row that agrees is VERIFIED and gets the day in its `checked` column, the one cell it writes (only if the file has the column); a row that disagrees is a CONFLICT and a part LCSC no longer lists is MISSING, and neither is changed. It never writes `price`, `stock`, `price_breaks`, `library` or `date` (JLCPCB's numbers, which LCSC's differ from), and says in a note when LCSC lists stock 0 where the row has some. `catalog age` lists the rows whose price and stock were read more than 90 days ago, the age at which the editor's bill of materials calls a price stale, and the rows whose price and stock are unknown (ADR 0039, ADR 0047; a refresh never changes `date`).

**Supply provider**:
A source of price and stock for a part number, behind one trait: given an LCSC or manufacturer's number it answers with quotes, each saying its channel (JLCPCB assembly stock or LCSC retail stock), stock, price breaks and when it was read. Asked when the bill of materials or `fab` is made, cached with a timestamp, and when it has no answer price and stock are **unknown**, never guessed and never 0. Configured in the `[supply]` table of `netsmith.toml`; credentials never go there (ADR 0047, provisional).
_Avoid_: Supplier feed, price list (a file provider reads one)

**Generic part**:
A passive or LED the solver chooses by value alone: a resistor, a ceramic capacitor, an inductor or an LED with no supplier or part number. A rating it cannot know (a capacitor's rated voltage, an inductor's saturation current and DC resistance, an LED's forward voltage) is taken from what the design states or demands. A separate step resolves it to a catalog row (JLCPCB Basic first; a capacitor only to a row with a recorded DC-bias curve), or reports it unresolved with the nearest alternatives and whether the checks pass with each (ADR 0047, provisional).
_Avoid_: Virtual part, ideal part

**Bias**:
The DC voltage across a part: a capacitor, a diode, a Zener or a TVS diode. A part with a `.bias` and a `bridge` placed between a rail's `pos` and `neg` has the rail's voltage as its `.bias` without being told; a design states `.bias` only for a part that is across no rail.

**Marked value**:
A capacitor's `.capacitance`: the value printed on the part and listed on a bill of materials, with its tolerance. A design states it where it means a part, such as a bypass capacitor.
_Avoid_: Nominal

**Effective capacitance**:
A capacitor's `.effective_capacitance`: the marked value after the DC bias across it and its tolerance, in the worst case over the range the bias can take. A ceramic capacitor holds less than its marked value as the bias rises, so a design states what it needs of the effective capacitance where the circuit depends on it, such as a regulator's output capacitor. The solver works it out from the part's published curve, and a part with no curve cannot be used for it. Temperature and ageing are not part of it yet.
_Avoid_: Derated capacitance, usable capacitance

**Footprint**:
The copper pads and outline a part has on a board, named by the CAD tool's library.

**Mapping**:
The TOML file that says which KiCad symbol and footprint a kind of part has in each package: `std/kicad.toml` for the standard library, and a `kicad.toml` at a project's root for its own parts and its own libraries. It is the only place KiCad's names appear (ADR 0021).
_Avoid_: Symbol table, library table

**Project library**:
A symbol file (`.kicad_sym`) or footprint folder (`.pretty`) of a project's own, listed in its `kicad.toml` as a `[[library]]`, so that its parts map to symbols and footprints KiCad does not ship.

**Unit**:
One of the drawings of a KiCad symbol that has several, such as the two amplifiers and the supply of a dual op-amp. All units of a part are placed in the schematic, each with the part's reference.

**Ratsnest**:
The connections a board still needs, drawn as straight lines between pads, before any track is routed.

**Fabrication package**:
The files a board house needs: Gerbers and drill files, a bill of materials, and a placement file. netsmith writes them itself from the board file (ADR 0034).

**Design rules check**:
netsmith's own check of a board's copper, holes, edge, courtyards, silkscreen, solder mask and connectivity against the process's rules (ADR 0037). It is the gate `fab` runs. A finding is data: a rule (named as KiCad names it), a severity (an error stops `fab`, a warning is listed in the notes), a message with the numbers, a place and the items involved, which the editor's Findings tab shows. _Avoid_: DRC gate (when KiCad's check is meant, say KiCad's design rules check)

**Cross-check**:
`netsmith fab --cross-check`: the board is plotted with KiCad as well and its Gerbers, drill files and placement are compared with netsmith's own (layers as bitmaps), and KiCad's design rules check is run and its findings compared with netsmith's (by rule and place). KiCad is a witness there, never a source of files or of the gate (ADR 0034, ADR 0037).

**Candidate**:
One real part in a catalog (one row of its file), with a value for each column. The solver takes the first candidate that satisfies every requirement.

**Narrowing**:
Restricting a field to the values that both its current limit and a new constraint allow. `&=` does it (ADR 0018).

**Relation**:
A constraint that mentions other fields, so it waits for the solver instead of narrowing a field now. `==` states an equation that must hold in the worst case; `>=` and `<=` state bounds. There is no plain `=` (ADR 0018).

**Editor**:
`netsmith editor`: a web app served on 127.0.0.1 by the netsmith binary, with seven sections (Schematic, Power tree, System, BOM, PCB, Build, Source), shown one at a time or two side by side; the Source section has the project tree and a source editor that shows the compiler's and solver's messages beside their lines, the others are the views of a board; and two actions in the top bar, Write KiCad files and Make fab files (ADR 0019). The schematic is shown one sheet (a module instance) at a time. A long job runs on the server while the page stays usable. It writes the source files (in its source pane), the physical file (the requirements a person draws, such as a keepout) and the layout file (the solution the PCB view moves). It reads the physical file for the PCB view and lists it in its tree; its drawn keepouts are still written to the layout file (the write path to the physical file is not built yet, issue #52). It replaces the schematic viewer, which is gone.
_Avoid_: Viewer, IDE, GUI

**View**:
One way the editor shows a board built from the source: the schematic, the power tree, the system, the bill of materials or the PCB. Each is computed from the checked and solved model, and none is edited: the source is.
_Avoid_: Page

**Power tree**:
The view of a board's rails as cards: the voltage of each net a rail draws from, the total of the currents on it, and what the part that supplies it can give, joined left to right, from the sources to the loads, by the regulators and stages that make one rail from another and by the joins of a system. The numbers are the solver's, in the worst case.
_Avoid_: Power diagram, rail map

**Netlist comparison**:
Deciding whether two designs are the same circuit by connectivity (`netsmith compare`, ADR 0017). Components are matched by value and footprint and by how their pins are wired, so net names and, unless told otherwise, designators may differ. It succeeds with the pairing of components and nets, or fails with the nets and pins that differ.
_Avoid_: Netlist diff (a diff compares names)
