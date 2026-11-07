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
Advice that a person decides about, as opposed to a limit of the fabricator. A design that goes against one gets a `warning:` and still builds; `guidelines = "error"` in `netsmith.toml`, or `--strict` for one run, makes it an error. The first is the track width against current (ADR 0036); the datasheet layout rules of ADR 0030 are the next. An **Open circle** (ADR 0057), an **Unconstrained pick** and an **Unstated value** (ADR 0065) are findings of the same kind.
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

**Pin protrusion**:
How far the pins of a through-hole part protrude out of the far side of the board (`.pin_protrusion`, a length, ADR 0045 section 15; the owner chose the name on 2026-11-04), whether they were cut after soldering or bought short. The board's physical file states it for one part, narrowing it as it narrows `.height`; a part that states none has an unknown protrusion, which is not zero, and its pads stay an obstacle for a part on that face. A part on that face may stand over the pins when it is fitted by hand, states its height, and the protrusion plus that height fits the height limit there. `fab-notes.txt` and `order.txt` then say how to cut the pins (a `trim:` line).
_Avoid_: Tail, tail height, lead length, stub

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
_Avoid_: Sidecar (the word for what it is, not its name), placement file (that is the pick-and-place file for the board house). The schematic has its own, the _Schematic layout file_.

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

**Bank**:
Identical capacitors in parallel that stand in one place of the design, a `Capacitor` with a `.count` (ADR 0052, proposed). The count is one number the board may state (`.count &= 3`, `.stage.output_count &= 3`), a range the solver chooses from (`1..8`, the fewest that holds every check), or unset, which is one. Every other field is the value of one capacitor of the bank. Lowering makes one part for each, on the same nets, and the bill of materials lists the row once with the quantity; the second and later parts have the path of the first with `#2`, `#3`. Only the standard `Capacitor` is a bank (a user part with a member named `count` is one part), and a requirement on a bank's part path in the physical file holds for each of its capacitors. A bank is not a repetition construct: the language has none.
_Avoid_: Array, capacitor slots (`cap_a`, `cap_b`), parallel group, multiplier, multiplicity

**Dielectric**:
The class of a ceramic capacitor, `C0G`, `X5R` or `X7R`: the enum `Dielectric` of `std::parts::passive`, the type of a `Capacitor`'s `.dielectric` and of the optional `dielectric` column of the capacitor catalog (ADR 0055, proposed). A design states it with `.dielectric &= Dielectric::X7R`. Read from the class that JLCPCB's record prints for the row; a generic capacitor has none.
_Avoid_: Temperature code, temperature characteristic, material

**Heating current**:
The DC current at which an inductor warms by the rise its datasheet states: `Inductor`'s optional `.rms_current`, a catalog column that the standard rows leave empty because the rise differs between makers and no row has a recorded value (ADR 0055, proposed). Distinct from the saturation current, which is where the inductance falls.
_Avoid_: Rated current (a rating of what?), temperature-rise current, Irms (the symbol, not the term)

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
Who puts a part on the board: the field `.assembly` of every part, of the type `AssemblyMode`: `AssemblyMode::Fab` (the board house places it), `AssemblyMode::Hand` (soldered by hand) or `AssemblyMode::None` (not fitted at all: a hole, a fiducial). It decides the BOM, the placement file and the notes of `fab` (ADR 0024, provisional). A project may set a default for it in `netsmith.toml` (`assembly = "fab"`, ADR 0028; the manifest and the catalog files write the lower-case word). A board cannot reach the parts of a module, so a module that wants the choice offers a value of the type `AssemblyMode` (`.assembly_mode`) and ties its parts to it with `.j.assembly &= .assembly_mode`; the board's narrowing of that value is the design's own statement for those parts (ADR 0042, ADR 0054).
_Avoid_: Population, fitting, DNP

**Enum**:
A type whose values are a fixed list of named variants, declared with `enum Name { A, B }` and imported like a definition (ADR 0054, proposed). A variant is written with its enum, `Name::A`; a field of the enum type takes only its variants, with `&=`, `?=` and `==`. `AssemblyMode` is the one enum in scope in every file without an import (`std/prelude.nsm`).
_Avoid_: Option set, choice list, constant, tag, quoted word (for a value that is one of a fixed list)

**Variant**:
One value of an enum, written `Enum::Variant` (`AssemblyMode::Hand`). Not a text: `"hand"` is refused where a variant belongs, and the message names the variant it was meant to be.
_Avoid_: Case, member (a member is a `.name` of a definition), mode

**Designator**:
The reference of a part on the schematic, the bill of materials and the placement file: letters and a number, `R1`, `C2`, `BZ1`. The letters are the field `.designator` of every part, stated by its kind (`Resistor` is `"R"`, ADR 0046) or narrowed by a part or board that defines a part with none, and `"U"` when nothing says; the number counts the parts with the same letters, in the order of their paths, where a number at the end of a path segment sorts as a number (`.pads.tp2` is `TP2`, and `tp10` follows `tp9`; owner decision 2026-11-04). The layout file is keyed by path, so a designator that changes moves no placement.
_Avoid_: Reference letter, prefix

**Part**:
A component with pins and quantities, such as a resistor or a regulator.
_Avoid_: Component, device

**Interface**:
A typed bundle of pins and quantities that is connected as one. A bundle of quantities with no pins, such as a process, is also an interface. A member of an interface type is a port (`.supply: Rail`); what picks a part is a generic, not an interface.
_Avoid_: Bus (that is the name of how the schematic draws one, see _Bus drawing_), bundle, group

**Module**:
A reusable sub-circuit that is placed on a board or inside another module. Only its interfaces, pins, and declared values can be used from outside; the parts and modules inside are internal.
_Avoid_: Circuit, block, sheet, subcircuit

**Generic**:
A definition, `generic Name { ... }`, of what a board may rely on of any part or module that provides it: pins with their kinds (a `pins` table whose rows have a kind and no name and no pads), values with their types, and the relations, narrowings and defaults that hold for every provider. It has no pads, no package and no catalog, unless it is its own provider: the standard `Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal`, `Fuse`, `Schottky`, `Zener`, `Tvs`, `NMosfet` and `LinearRegulator` are, so each has a pin table with pads and a catalog, is the first candidate for its own use, and keeps its kind name (ADR 0068 stage 2). A board uses it with a member of its type, `.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }`, and the solver picks a provider (ADR 0068, proposed). Do not confuse it with a **generic part**, which is a pick, not a definition.
_Avoid_: Abstract part, trait, protocol, role, family, spec

**Provider**:
A `part` or `module` whose header says `provides Name` (`part AcmeMicroCoax provides CoaxReceptacle`), naming a generic. It has every member and statement of the generic, does not declare them again, and adds its own pin table, numbers, catalog and, for a module, parts. It is a candidate for a use of the generic when its file is imported, directly or through other files, by the board's file. A provider's name is unique among the providers of one generic, and it does not contain a use of the generic it provides (ADR 0068 section 13). A provider is never a **supply provider**, the source of price and stock; say which one is meant (ADR 0068, proposed). A part may provide a standard generic such as `Resistor`: the generic's own catalog and pads are not read into it, and the design lists the generics it provides (`Part.provides`), so that the schematic and the placer treat it as the kind it provides.
_Avoid_: Implementation, implementer, subtype, instance (of a generic)

**Use of a generic**:
A member declared with a generic's type (`.rf: CoaxReceptacle { ... }`). The solver puts one provider in its place, an instance of that part or module at the use's own path (`rf`), and the build says which and why the providers before it were passed over. A use reaches only the members the generic declares, so a board works with every provider; a board that wants exactly one writes the provider as the type (`.rf: AcmeMicroCoax`), and nothing is picked (ADR 0068, proposed).
_Avoid_: Instantiation, binding

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
A function may be listed by a peripheral instead of a row, and a **class** is a function that rows list (`fn APP_GPIO NET_GPIO`) and peripherals are `on`: the function a board chooses for a pin that a core owns names the core (ADR 0071).
A module's own pin (`.scl: Pin`) passes a choice on to a pin of a part inside with an equation, `.mcu.p04.function == .scl.function`; `.function &=` on a module's pin that no such equation reads is an error, because it has no effect (ADR 0071, stage 0).
_Avoid_: Alternate function, mux, mode

**Peripheral**:
`peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }` in a part: the functions of one part that belong together (a UART's `TX` and `RX`), each with an optional kind as in a pin row. It is **in use** when the board chooses one of its functions on a pin; two of its lines are still one use. With `on CLASS`, every pin whose row lists the function `CLASS` offers its functions (so a part where any pin can carry any line keeps one row for each pin); with no `on`, only the rows that list a function by name offer it, a dedicated pin. A function belongs to one peripheral, and a peripheral and a function have different names (ADR 0071).
_Avoid_: Group, block, role, mode, mux, unit (a unit is the hardware a `shared` resource names, not a peripheral)

**Shared resource**:
`shared UARTS limit 2 { UARTE0 UARTE1 UARTE2 .p10 }` in a part: the hardware that at most `limit` of its members use at a time (one without `limit`; a limit is at least 1 and fewer than the members). A member is a peripheral, a function (in use when chosen on a pin) or a pin (in use when its function is chosen or the board connects it). Using more members than allowed is an error that names the part, the resource, every use and its line; it is judged for each instance of the part, before the solve, from the functions the board chose (ADR 0071).
_Avoid_: Conflict set, exclusive set, group, resource (alone)

**Logic level**:
One of the four voltages of a `Logic` line, as a datasheet gives them and relative to the line's ground (the `neg` of its `.reference` rail): `.vih` the lowest an input reads as high, `.vil` the highest it reads as low, `.voh` the lowest an output drives a high to, `.vol` the highest it drives a low to. After the rails are solved, the solver requires `voh >= vih` and `vol <= vil` for every output and input on a net, and says "not evaluated" when a level has no value or the grounds differ (ADR 0046).
_Avoid_: Threshold, noise margin

**Level translator**:
A `LevelTranslator` (`std::parts::logic`, ADR 0063): a single-supply buffer whose input accepts a lower logic level than its supply and whose output swings to its supply. Its input thresholds are `.vt_plus_max` (above it the input is a high for sure) and `.vt_minus_min` (below it a low for sure). A generic with the catalog `level_translator`, which has no standard row.
_Avoid_: Level shifter, voltage translator

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
The columns of a list of real parts that a part's fields are matched against when the solver picks one. A catalog may be keyed by a column (`catalog mcu by .mpn`), and then the design names the row. The rows are a CSV file named after the catalog, in the order the solver tries them (ADR 0022); a project adds its own files in `netsmith.toml`. A row with an empty cell is never offered, except in a column that a catalog may leave empty (`temperature_coefficient`, `rms_current`, `dielectric`), and then only to a design that says nothing about the field by any route (a narrowing, a relation, a join, a balance).

**Catalog file**:
A CSV file of the rows of one catalog: a header of the catalog's columns (field paths without the leading dot) and the fixed columns `lcsc`, `mpn`, `library`, `price`, `price_breaks`, `stock`, `source`, `date`, `checked`, `name`, `label`, `notes`. `price`, `stock` and `library` are JLCPCB's numbers, entered by hand, and `date` is the day they were read; the solver does not read them and the bill of materials uses them only when no supply provider has the part (ADR 0047). `checked` is the day a refresh last verified the row against LCSC. A row that names a part must have a `source`. The standard ones are `data/jlcpcb/catalog/*.csv` (ADR 0022).

**Catalog refresh**:
`netsmith catalog refresh`, the opt-in command that checks each row of catalog files against LCSC's product record (never JLCPCB): the manufacturer's number and package. A row that agrees is VERIFIED and gets the day in its `checked` column, the one cell it writes (only if the file has the column); a row that disagrees is a CONFLICT and a part LCSC no longer lists is MISSING, and neither is changed. It never writes `price`, `stock`, `price_breaks`, `library` or `date` (JLCPCB's numbers, which LCSC's differ from), and says in a note when LCSC lists stock 0 where the row has some. `catalog age` lists the rows whose price and stock were read more than 90 days ago, the age at which the editor's bill of materials calls a price stale, and the rows whose price and stock are unknown (ADR 0039, ADR 0047; a refresh never changes `date`).

**Supply provider**:
A source of price and stock for a part number, behind one trait: given an LCSC or manufacturer's number it answers with quotes, each saying its channel (JLCPCB assembly stock or LCSC retail stock), stock, price breaks and when it was read. Asked when the bill of materials or `fab` is made, cached with a timestamp, and when it has no answer price and stock are **unknown**, never guessed and never 0. Configured in the `[supply]` table of `netsmith.toml`; credentials never go there (ADR 0047, provisional).
_Avoid_: Supplier feed, price list (a file provider reads one)

**Generic part**:
(A pick, not a definition: see **generic**.) A passive or LED the solver chooses by value alone: a resistor, a ceramic capacitor, an inductor or an LED with no supplier or part number. A rating it cannot know (a capacitor's rated voltage, an inductor's saturation current and DC resistance, an LED's forward voltage) is taken from what the design states or demands. A separate step resolves it to a catalog row (JLCPCB Basic first; a capacitor only to a row with a recorded DC-bias curve), or reports it unresolved with the nearest alternatives and whether the checks pass with each (ADR 0047, provisional).
_Avoid_: Virtual part, ideal part

**Peak voltage**:
The highest voltage a rail reaches, transients included: the `.peak` of a `Rail`, where `.voltage` is the steady range. A part that clamps a transient (a TVS diode) states it for the rail behind it, joined rails share it, and the parts on the rail are rated for it. A peak below the rail's steady voltage is refused as a mistake (where a stage checks the ratings, as `BuckStage` does).
_Avoid_: Surge voltage, transient voltage (the transient is the event; the peak is the number)

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
One of the drawings of a KiCad symbol that has several, such as the two amplifiers and the supply of a dual op-amp, or the power unit and the signal unit of a processor module (`U1A`, `U1B`). The KiCad schematic places all units of a part, each with the part's reference; the editor's schematic does the same: each unit that has pins is a box of its own in the drawing, as the library draws it, with the part's reference and the unit's letter, upright, and the decoupling capacitors stand beside the power unit (ADR 0050). A pad is in one unit. The layout's own stack of two-terminal parts in a column is a different thing and is not called a unit in the docs.
_Avoid_: Sub-part, section, gate (a unit may be a gate, an amplifier or a supply)

**Ratsnest**:
The connections a board still needs, drawn as straight lines between pads, before any track is routed.

**Fabrication package**:
The files a board house needs: Gerbers and drill files, a bill of materials, and a placement file. netsmith writes them itself from the board file (ADR 0034).

**Design rules check**:
netsmith's own check of a board's copper, holes, edge, courtyards, silkscreen, solder mask and connectivity against the process's rules (ADR 0037). It is the gate `fab` runs. A finding is data: a rule (named as KiCad names it), a severity (an error stops `fab`, a warning is listed in the notes), a message with the numbers, a place and the items involved, which the editor's Findings tab shows. _Avoid_: DRC gate (when KiCad's check is meant, say KiCad's design rules check)

**Cross-check**:
`netsmith fab --cross-check`: the board is plotted with KiCad as well and its Gerbers, drill files and placement are compared with netsmith's own (layers as bitmaps), and KiCad's design rules check is run and its findings compared with netsmith's (by rule and place). KiCad is a witness there, never a source of files or of the gate (ADR 0034, ADR 0037).

**Candidate**:
One real part in a catalog (one row of its file), with a value for each column, or one provider of a generic in scope. The solver takes the first candidate that satisfies every requirement; for a use of a generic the candidates are the providers, in a fixed order, and the rows of the picked provider's catalog are then chosen as always.

**Narrowing**:
Restricting a field to the values that both its current limit and a new constraint allow. `&=` does it (ADR 0018). A default (`?=`) is not a narrowing: it takes effect only when nothing narrows the field.

**Default**:
A value a module or part offers for a field and a board may replace, written `.field ?= value` (ADR 0051, proposed). It applies only when no `&=` statement states that field; a value that reaches the field through a relation or a join does not count as stated. A board's statement drops the default and need not overlap it, and two defaults for one field are an error.
_Avoid_: Initial value, preset, fallback, soft constraint, weak narrowing

**Unconstrained pick**:
A part of a catalog with a value column (a resistor's resistance, a capacitor's capacitance, an inductor's inductance) that was picked while nothing limited the value, so the solver took the first row that fits (ADR 0065). A value is limited by a statement (`&=`, an applied default, a range a member carries in), by a bound that reaches it, or by a check or a current budget that the pick decides and that was judged once the board was solved (a budget counts only if the pick changes whether it holds); a check that holds whatever the size of the values is, such as the output voltage of a divider, limits their ratio and not their size, and the finding says so; a bound on a rating and a relation a part's own definition states do not limit it. A guideline finding: a warning by default, an error with `--strict` or `guidelines = "error"`. A range, however wide, is how a board says that any value will do.
_Avoid_: Unset value, free pick, default pick, loose part

**Unstated value**:
A value declared in a module or a board (not in a part, an interface or a catalog column) that a check reads and that nothing states or works out, so the check is not evaluated (ADR 0065). Only the root is named: a member that a wrapper forwards to is not one, the wrapper's is. A guideline finding with the same severity as an **Unconstrained pick**.
_Avoid_: Missing value, unset member, open value

**Transferred bound**:
A bound on a member that a part's field is held to (`.top.resistance &= .balance`, `.balance >= 20kohm`), read as a bound on the field too, because `&=` puts the field inside the member and a bound on all of the member is a bound on every part of it (ADR 0065). The solver adds it as a relation of its own while it picks and checks the parts, and takes it out of the model it returns. It adds a refusal and never a value.
_Avoid_: Propagated bound, inherited bound, derived bound (ADR 0057 uses that for a bound on a value worked out from a part)

**Relation**:
A constraint that mentions other fields, so it waits for the solver instead of narrowing a field now. `==` states an equation that must hold in the worst case; `>=` and `<=` state bounds. There is no plain `=` (ADR 0018).

**Circle of values**:
Fields that depend on each other through their equations and balances of currents while none has a value, such as the input voltage, the input current and the drop of a regulator fed through a series resistor. The build names the circle in a note and does not evaluate the checks that need it. A range stated on one of its fields starts it, and so does the range a part works in (`.vin.voltage >= 3V`, `<= 65V`) together with a value to start from (ADR 0053, proposed): the solver then grows the supply's range until the equations map it into itself and accepts it only if nothing else inside the part's range satisfies them.
_Avoid_: Cycle, loop, algebraic loop (a loop is a feedback circuit, not a set of values)

**Open circle**:
A circle of values that stayed without a value after the solver's own start, with a bound (`>=` or `<=`) that reads it, or a value worked out from it, left unevaluated. The build says so as a guideline finding (rule `open circle`): a `warning:` that names the fields, the checks and a field to start from, and an error under `--strict`. A circle that also reads a value nothing states names that value instead (ADR 0057, proposed). A circle may close through the currents of a net instead of an equation: a net of more than two currents, where the solver does not work a part's current out (ADR 0026), as a divider behind a fuse or a resistor does (issue #18); the finding then names the net and its currents, and a range on the part's current or a voltage in the circle starts it. A current that is exactly zero is not a term of the net.
_Avoid_: Unstarted loop, stuck cycle

**Constant value**:
A value that no choice of parts can change: the field, and the fields joined to it, have a value now and nothing works any of them out. The stated voltage of a rail is one. Parts that read only such values and nothing else in common are searched apart (ADR 0057, proposed).
_Avoid_: Fixed value, frozen value

**Part's range**:
What a part's own relations on a field bound, written as `>=` and `<=` with values (`.vin.voltage >= 3V`, `<= 65V`): the lowest lower bound and the highest upper bound. The solver starts a circle of values only inside it; a state below it is the part switched off, which its equations do not describe (ADR 0053, proposed).
_Avoid_: Operating domain, valid range, working window

**Editor**:
`netsmith editor`: a web app served on 127.0.0.1 by the netsmith binary, with seven sections (Source, Schematic, Power tree, System, BOM, PCB, Build; the number keys 1 to 7 pick them in that order), shown one at a time or two side by side; the Source section has the project tree and a source editor that shows the compiler's and solver's messages beside their lines, the others are the views of a board; and two actions in the top bar, Write KiCad files and Make fab files (ADR 0019). The schematic is shown one module (instance) at a time. A long job runs on the server while the page stays usable. It writes the source files (in its source pane), the physical file (the requirements a person draws, such as a keepout) and the layout file (the solution the PCB view moves). It reads the physical file for the PCB view and lists it in its tree; its drawn keepouts are still written to the layout file (the write path to the physical file is not built yet, issue #52). It replaces the schematic viewer, which is gone.
_Avoid_: Viewer, IDE, GUI

**View**:
One way the editor shows a board built from the source: the schematic, the power tree, the system, the bill of materials or the PCB. Each is computed from the checked and solved model, and none is edited: the source is.
_Avoid_: Page

**Satellite**:
A small part that the editor's schematic draws beside the pin of a main part (an IC, a connector) that it serves: a decoupling capacitor, a pull-up, a bootstrap capacitor, a divider. It is wired to that pin always, and its rail pins have a stub and a power symbol of their own (see _Power symbol_). The main part and its satellites are one block.
_Avoid_: Passive (a satellite may be a transistor), helper

**Pair satellite**:
A satellite whose two pins are on two different pins of one main part, such as a crystal across `XL1` and `XL2`, a resonator, or a series element across two pins. The schematic stands it beside both pins, turned so that its two wires are short, have as few bends as can be, and cross nothing, and never writes a name on its nets. The metric `pair_crossings` (target 0) counts the crossings on those wires.
_Avoid_: Crystal rule, two-pin satellite

**Spine**:
How the editor's schematic draws the power path through a regulator: one row from left to right, found from the netlist, the pin kinds and the rails, never from what a part is called, and only where the rails are known. From the left: the chains of series parts of the module whose far end goes on to another module (a fuse and a diode, with a TVS diode on the node they end in), the input chain (a ferrite bead, from where the power comes in to the IC's input pins), the IC with its input on the left and its switching pin on the right, the output chain (the inductor) and the output node. A **series part** is a two-terminal part whose two pins are on two different nets, neither a ground, that is not across two pins of the IC (that is a _Pair satellite_); each stands on the row, turned along it, and a chain goes on through a net only while one more series part is there and no main part has a pin on it. The capacitors to ground hang from the node they serve, the smallest nearest the IC, each with a stub and a ground symbol of its own (side by side they share one wire and one symbol, see _Power symbol_); the feedback divider hangs from the output node with the feedback pin wired to its middle; a rail on the row is wired along it and has one symbol, at its outer end. The pair satellites of the IC (a bootstrap capacitor) stand beside its two pins and the row never crosses them. The metrics `spine_bends` (the bends along the longest series chain) and `detached_chain` (chains that are neither on the IC's row nor wired to it) have target 0.
_Avoid_: Power path (that is the current; the spine is its drawing), backbone, main line

**Power symbol**:
How the editor's schematic draws a rail: the negative side of a rail (where the current returns) with the ground symbol, bars that shrink away from a wire, and the positive side with the supply symbol, a stem with an arrow head and the net's name beside it. Which nets are which comes from the checked model (a `Rail`'s `pos` and `neg`), never from the names; the ground symbols carry their names only when a board has more than one ground. Each pin of a main part on a rail has a wire to its own symbol, and pins next to each other on one rail share a bus and a symbol; a satellite's rail pin has a short stub and a symbol of its own (a supply symbol up, a ground symbol down), except that satellites side by side in a row, and satellites on the same rail within 20 grid steps of each other (two pull-ups of a bus, one above the other), share a short wire and one symbol where nothing is crossed. The KiCad schematic file does not use them.
_Avoid_: Power port (a net label is the name of a signal net at the end of a stub, and a power symbol is never one)

**Flag**:
How the editor's schematic shows a net that goes on to another module: a pentagon pointing at the end of the short stub of a pin, like KiCad's hierarchical label, with the net's name in it (none for a net with a generated name). A net that goes on has a flag at the end of the stub of each group of its pins in the module (a main part and the small parts that serve its pins).
_Avoid_: Port (a `Rail`'s and a module's ports are pins), off-sheet label, sheet

**Bus drawing**:
How the editor's schematic draws the members of one interface instance (`mcu.uart0`) when three or more of them run between the same two main parts of a module, or leave the module from one part (or two or more of them run between three or more parts, a **shared bus**, or between two parts and on to another module): each member's pin stays on the part's edge and its wire is a short entry that meets one thick line, which runs to the other part (to every other part, as one line with a branch to each, for a shared bus) or ends in one flag where the members go on to another module. The members of an interface nested in the bus's interface are members of it under dotted names (`data.sclk`), an entry whose pin the part's pin table says is an output, an input or bidirectional has an arrowhead for it (a **bus arrow**), and the flag says which instance of the other sheet the members go to (`→ modem.uart: {rx, tx}`). It is named once, by the instance (`uart0`), with the list of members (`{tx, rx, rts, cts}`) and a slash with their count. The line joins nothing: each entry joins, by name, the net of that name at the other end, as a label does. Which nets are one bus comes from the interface instances the design carries (ADR 0049), never from net names. The word is for the drawing; the language's thing is an _Interface_.
_Avoid_: Bundle, harness, interface (for the drawing), bus wire

**Shared bus**:
A bus whose members have a pin on more than two main parts of a module, or on two and go on to another module: an I2C bus with several devices, an SPI bus with a clock and data lines that every device has. One thick line with a spine at each part and a branch from each spine to the line, and the members' entries as for any bus (ADR 0067). It needs three members between two places and two among three or more.
_Avoid_: Multidrop bus, tap, bus with taps, daisy chain

**Bus arrow**:
The small arrowhead on a bus entry that says which way the member's signal goes at that part, as the part's pin table says it (`output`: away from the part, `input`: at the part, `bidirectional`: one each way); a pin of any other kind has none. The thick line has no arrow, because the members of a bus go both ways (ADR 0067).
_Avoid_: Direction mark, flow arrow, signal arrow

**Schematic layout file**:
`<stem>.sch.toml` beside a source file (`src/main.nsm` has `src/main.sch.toml`): where a person moved a part in the editor's schematic, so that the drawing stays as they arranged it across edits. For each board, then each sheet (a module instance, or the board's own sheet), a part is keyed by its path from the sheet and holds an offset of its block (`dx`, `dy`, millimetres on the 1.27 mm grid) from where the automatic drawing puts it, optionally a `rotate` (0, 90, 180, 270) and a `mirror`, or a `pin` to a place from the sheet's reference part; a note is keyed `<path>#doc` or `<path>#relation` and has an offset. A part with no entry is placed automatically; an entry whose part or sheet is gone is a finding (stage `layout`, with the file's line) and is ignored, and so is an entry the drawing cannot honour (its block would lie on another part, stand too far from the drawing, or break a rule of the drawing). The engine applies the file before it routes a wire, so the wires go round what was moved. It never holds a wire, a label or a net, so it can move the drawing and never rewire it. Optional, written by the editor, editable by hand (ADR 0061). The code type is `LayoutHints` (`netsmith_view::hints`) and its entries are _hints_; the block is a main part with the _Satellites_ that serve it.
_Avoid_: Layout file (that is the PCB's), sidecar, positions file, `.layout.toml` (ADR 0019's first name for it)

**Schematic note**:
How the editor's schematic shows the source's own writing: the doc comments (`///`, `//!`) on a board, a module, a part or an interface, and every `==` relation of a board or a module, each as a framed box of wrapped text with no wire or leader line to anything. A doc comment is in the page's sans serif; a relation is in a monospace face under a small heading (`Relation`), with what the solver settled on for the field it gives a value to. The box of a part's note stands beside the part in free room, a board's or a module's stands at the top left of its sheet under the title, and a note with no room near its part goes to the margin. Which text is a note comes from the checker's model (`Design::notes`, ADR 0060), never from a `//` comment, which is not drawn. It joins nothing: `netsmith_view::check` leaves it out of the netlist and reports a note that lies on anything else (`note-overlap`) or whose text escapes its frame (`note-text-overflow`). The code type is `Note`; the word for the drawing is the schematic note, because a build's _note_ (a message that names what the solver left alone) is another thing.
_Avoid_: Annotation, comment (that is the source's `//`), callout, text box, label (that is the silkscreen part, and a _Net label_ is a net's name), title block

**Reference part**:
The part a schematic sheet's coordinates start at: the largest part of the sheet (most pins, then the largest symbol, then the first by path), put at the origin of the sheet's drawing. What grows round it when a part is added leaves it where it was, so the part and what did not change keep their coordinates. The sheet's box (`data-bounds`, the `viewBox`) may start below zero because of it. `netsmith_view::stability` compares two drawings of a sheet in the frame of the reference part they share.
_Avoid_: Anchor (the engine's word for a main part with parts round it), origin part

**Net label**:
How the editor's schematic shows a signal net that is not drawn as one wire: the net's name as plain text at the end of a short stub out of each group of its pins (a main part and the small parts that serve its pins, which stay wired to that stub), like KiCad's local label. A net is named, not wired, when it joins more than one such group and its wire would be long (over 48 grid steps), fan out to three groups over 30 steps, or cross more than one other wire; a rail is never named (a power symbol is drawn), nor is the net of a part that serves a pin. A net with a generated name is shown by the first pin of its main part (`J2.3`), as KiCad names such a net. The pieces of a net that carry the same name in a module are one net.
_Avoid_: Bare name, off-page connector (a flag is that), global label

**Stray wire**:
Wire or dot in the editor's schematic that serves nothing: an end that touches no pin, power symbol, flag, label, X or other wire of its net (a dead end, such as a rail that runs on past its last tap), two wires of one net along one line, a hook (out, across a grid step or less, and back), a wire that is a point, a dot where fewer than three arms of a net meet, three arms and no dot, a wire along a pin's line, or along the outline of a body or a flag within 0.7 mm. The drawing has none: the engine takes them off when it has routed, and `netsmith_view::check` reports any that is left.
_Avoid_: Dangling wire, spur, stub (a stub is the short wire straight out of a pin)

**Ref comparison**:
What the editor says changed in the circuit between a git ref (the base) and the files on disk (the head), or between two refs (the head is then the second ref, which is read as the first is and leaves the files on disk unread): the parts, nets, rails, ports, bill of materials and problems that differ, and what changed in the systems and on the PCB (where a part is in the layout file, the outline, the layers), each with its source line on both sides (ADR 0059). A part is the same on both sides by its path in the board, a net by its name or its pins, a rail by its net; a part that only has another designator is renumbered, not changed. It reads the ref with `git archive` and never touches the work tree. Not the _Netlist comparison_ (two designs that share no names) and not a text diff.
_Avoid_: Diff (a diff compares lines), git diff, branch diff

**Checkpoint**:
A place in a long call (the checker's instances, a trial of the solver's search, a part or a net of lowering, a sheet or a pass of the schematic's layout, a move of the placer) that asks whether the job it belongs to was cancelled, and ends the call if so. A job is cancelled with a token (`netsmith_core::job::Cancel`) that the one who started it installs for its thread; a cancelled call unwinds to him with nothing of what it made kept, and the analysis that was cancelled leaves the last reading as it was (ADR 0062). It is the _Cancel_ button's reach: a call is stopped at its next checkpoint, not at once.
_Avoid_: Interrupt, abort, kill (a job is never killed; it is asked, and ends where it can)

**Power tree**:
The view of a board's rails as cards: the voltage of each net a rail draws from, the total of the currents on it, and what the part that supplies it can give, joined left to right, from the sources to the loads, by the regulators and stages that make one rail from another and by the joins of a system. The numbers are the solver's, in the worst case.
_Avoid_: Power diagram, rail map

**Netlist comparison**:
Deciding whether two designs are the same circuit by connectivity (`netsmith compare`, ADR 0017). Components are matched by value and footprint and by how their pins are wired, so net names and, unless told otherwise, designators may differ. It succeeds with the pairing of components and nets, or fails with the nets and pins that differ.
_Avoid_: Netlist diff (a diff compares names)
