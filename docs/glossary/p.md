# Glossary: P

**Package shape**:
The solids a part's body is drawn as in the _Scene_ where a footprint generator made its footprint: leads, terminals, balls, pins and marks made from the generator's own numbers (the body, the span, the lead width and length, the exposed pad), built to the part's known height and never giving one (`crates/netsmith-pcb/src/body3d.rs`, ADR 0105). A footprint read from a file whose name and pads are those of a vertical pin header or socket (`PinHeader_2x18_P2.54mm...`, a full grid of round plated holes at that pitch) is drawn as the generated header is; another footprint, or a generated one not shaped, is the box of its _Body_; a part of unknown height is its flat outline.
_Avoid_: 3D model (KiCad's or a maker's file), parametric model, mesh

**Pad**:
The text identifier of a pin that its symbol and footprint share, such as `1`, `A1`, or `EP`. A pin can have several (a regulator's output on pads 1 and 2). The part's pin table gives them, for the package the part is in; a pin has no `.pad` field.
_Avoid_: Number, pin number

**Pair satellite**:
A satellite whose two pins are on two different pins of one main part, such as a crystal across `XL1` and `XL2`, a resonator, or a series element across two pins. The schematic stands it beside both pins, turned so that its two wires are short, have as few bends as can be, and cross nothing, and never writes a name on its nets. The metric `pair_crossings` (target 0) counts the crossings on those wires.
_Avoid_: Crystal rule, two-pin satellite

**Panel**:
A `panel` definition: one board repeated in a grid on one fabrication panel, cut apart by V-cuts, `panel P { .cell: Board  .fab: VCutPanel { .columns &= 3  .rows &= 2  .rail &= 5mm } }` (ADR 0040). The board stays a stand-alone `board`; the panel names no nets and joins nothing. `fab` writes one Gerber set, drill file, bill of materials and placement file for the whole panel (designators carry the cell, `R1_3`), and the panel as a `.kicad_pcb`. The board house's rules for it are the `.fab` member, as a board's process is its `.process`.
_Avoid_: Array, step and repeat, panelisation (the word for the act of making one)

**Part**:
A component on a board, with pins and quantities, a designator and a line in the bill of materials. A `part` definition makes one (`.u1: Lt8641`), and so does a generic that is its own provider (`.r1: Resistor`). The part is the member `.r1`; `Resistor` is a **generic**, a definition and not a part. A generic with no pads (`CoaxReceptacle`) is placed through a provider, and the provider is the part. A checker message says "part" of a definition that is placed as one, `Resistor` included.
_Avoid_: Component, device

**Part's range**:
What a part's own relations on a field bound, written as `>=` and `<=` with values (`.vin.voltage >= 3V`, `<= 65V`): the lowest lower bound and the highest upper bound. The solver starts a circle of values only inside it; a state below it is the part switched off, which its equations do not describe (ADR 0053, proposed).
_Avoid_: Operating domain, valid range, working window

**Parts file**:
`reference/parts.toml` of a ported board: the original's reference designator of each part of the design, by the part's member path (`[parts]`, `"usb_uart.receptacle" = "J1"`, #456). The nets proof of the port's tests names the design's parts as the original does by it, and `netsmith parity` pairs the original's parts with the board `fab` wrote by it and the member path `fab` writes on each footprint as a hidden property (`netsmith_path`). A member renamed in the source is renamed in the file; nothing else maps designators.
_Avoid_: designator map, `designators.toml` (the map of the fab board's designators that it replaced, which went stale when a member was renamed or a part was added ahead of another)

**Pass-through**:
A role that an interface has no `lines` for, such as `UsbRole::Passthrough`, which a receptacle (`std::parts::connector::UsbC2`) states: the bus goes through the part, so it is not an end of the bus and the role is the one of the end behind it. It has no table to judge, counts toward no limit, satisfies the `bus role` guideline, and the guideline and the note name the end behind it that states no role, never the pass-through (ADR 0064, section 10). A **Role** that is an end of the bus has `lines`.
_Avoid_: Transparent end, bypass, receptacle role, none (there is no role `none`)

**Peak voltage**:
The highest voltage a rail reaches, transients included: the `.peak` of a `Rail`, where `.voltage` is the steady range. A part that clamps a transient (a TVS diode) states it for the rail behind it, joined rails share it, and the parts on the rail are rated for it. A peak below the rail's steady voltage is refused as a mistake (where a stage checks the ratings, as `BuckStage` does).
_Avoid_: Surge voltage, transient voltage (the transient is the event; the peak is the number)

**Peripheral**:
`peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }` in a part: the functions of one part that belong together (a UART's `TX` and `RX`), each with an optional kind as in a pin row. It is **in use** when the board chooses one of its functions on a pin; two of its lines are still one use. With `on CLASS`, every pin whose row lists the function `CLASS` offers its functions (so a part where any pin can carry any line keeps one row for each pin); with no `on`, only the rows that list a function by name offer it, a dedicated pin. A function belongs to one peripheral, and a peripheral and a function have different names (ADR 0071). A peripheral may be an end of a bus, `peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master { SPIM0_SCK:output as sclk ... }`: it has the role, and a pin on which the board chose one of its functions is a line of the bus (ADR 0064).
_Avoid_: Group, block, role (the **Role** of a peripheral that is a bus end is the one it names after `as`), mode, mux, unit (a unit is the hardware a `shared` resource names, not a peripheral)

**Physical**:
What the case, the product and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector, antenna or sensor must be, a spacing or alignment between two parts, a module's face, a region with a height limit, a pattern of test pads, keepouts a board draws, local pours. It is requirements, never solutions. It is written in the **physical file** of a board, `<stem>.physical.nsm` beside the source, as `physical for Board { ... }` with a nested `case { ... }` section for the outline, cutouts, mounting holes, fiducials and labels (owner decisions 2026-11-04, ADR 0048). Its statements are the language of a board's body, and a path in it may cross a module boundary, only to state physical intent. The circuit source keeps what the circuit's checks consume (process, stackup, plane nets, assembly, designator letters, currents, impedance, pairs). The word `physical` is not a definition kind: `physical for` begins a unit, and `physical intent` is the thing it states.
_Avoid_: PCB constraints (it is not the circuit's constraint and not a result), sidecar (the layout file is the sidecar)

**Pick lock**:
`<stem>.lock.toml` beside a source file: the catalog row each part of each board of the file was picked as, by the part's path, with the row's LCSC and manufacturer numbers as a check against a renamed row, and the provider each use of a generic with more than one provider in scope was picked as, by the use's path (ADR 0113; a locked use is never varied). `build`, `fab` and `fab --order` (for each source file that builds a board of the order) write it when it does not exist and read it when it does: the solver offers a locked part only its locked row, judged by everything the design says, and does not fall back to another row if it fails (an error that names the row and the relation). A part with no entry, or an entry for a part that is gone, is told and is an error with `--locked`; an existing lock is never rewritten by `build`, `fab` or an order, only by `netsmith lock --update` (which solves again with no holding and prints the old row and the new one of each part that changed), and `netsmith lock --check` says whether it is current. It holds no stock, no price and no solved value, and it is not the solver's result: it records one choice among rows (ADR 0100). A committed file, so a change to a catalog or to the solver that would move a pick shows as a change to it. The editor shows whether it exists and whether it is current, read only: it never writes it.
_Avoid_: Lockfile (Cargo's word for a file of versions), solved-design file (a different, unbuilt file that would hold the `Design`), pin file and pinned (a _Pin_ is a part's lead; `Catalogs::with_pin` holds a part to one row to test an alternative, and drops the design's numbers for it)

**Pick-time analysis**:
The DC analysis run inside the search of picks, with the last part of a group chosen: the island of the network that has a part of the group is solved with the picks in it, cut at the rails whose voltage an earlier pass worked out, and the rails on it that nothing gave get its voltage, so the checks that read them judge the pick. It is what a hand-written `.vout.voltage == .vin.voltage - .r.drop` did for a series resistor in front of a clamp. It solves only an island with no regulator in it (ADR 0076, amendment 10). The pass after the picks is the analysis that every pick is finally judged on.
_Avoid_: Trial analysis, inner analysis, solver loop

**Pin**:
A connection point of a part, interface, or module.

**Pin file**:
`<stem>.pins.toml` beside the top-level source: for each request of each board the pin it got (`pin`, `function`, `lock`, `source`). `build` and `fab` write it when a run picks a pin, `--locked` stops a run that would change it, and a run never changes an entry that still holds. It holds a solution and the requirements are in the source, as in the _Layout file_ (ADR 0032, ADR 0101).
_Avoid_: Pin map file, pinout file (a _Pinout_ is a part's table), pin lock file

**Pin kind**:
What a pin is electrically, from a closed set: `power`, `input`, `output`, `bidirectional`, `passive`, `no_connect`, `tri_state` and `open_drain` (ADR 0016, ADR 0064, #460). The kind decides what the checker asks of the board: a `power` or `input` pin must be connected, a `no_connect` pin must not be, and two `output` pins may not share a net. A `tri_state` pin is an output that drives only while its part is selected: any number of them may share a net, and one `output` among them is refused. An `open_drain` pin only pulls its line low or lets it go (open drain or open collector): any number of them may share a net, an `output` or `tri_state` pin among them is refused, and the net needs a pull-up. `optional` and `required` move a pin from its kind's default.
_Avoid_: Pin type, direction

**Pin map**:
The pins that the requests of a board got, as the _Pin file_ lists them. The editor shows the map of the selected part (the pin, its pad, the function, whether the entry is locked, whether it came from the file or this run), and writes the `lock` line of an entry (a padlock) and its `pin` and `source` lines (a move to another pin that offers the function), and no other line of the file. The firmware export will list these with the pins the source names (issue #111).
_Avoid_: Pinout (a part's table), pin assignment, pin table

**Pin optimizer**:
What `fab` does when the router leaves connections of a board that has _Pin requests_ (`--pins auto`, the default): it swaps the pins of requests whose entries are not locked with the pins that offer the same capability, scored on the placed board by the ratsnest (crossings, then length), and goes round the pipeline again with the new _Pin map_, at most three rounds, writing the _Pin file_ only when it swapped. With `--pins locked` it moves nothing and writes `pins.proposed.toml` in the output folder (ADR 0101).
_Avoid_: Pin autorouter, pin shuffler, pin swapper (a swap is one move of it), auto-assign (the first assignment is made by the check, before any layout)

**Pin protrusion**:
How far the pins of a through-hole part protrude out of the far side of the board (`.pin_protrusion`, a length, ADR 0045, header of `crates/netsmith-pcb/src/protrusion.rs`; the owner chose the name on 2026-11-04), whether they were cut after soldering or bought short. The board's physical file states it for one part, narrowing it as it narrows `.height`; a part that states none has an unknown protrusion, which is not zero, and its pads stay an obstacle for a part on that face. A part on that face may stand over the pins when it is fitted by hand, states its height, and the protrusion plus that height fits the height limit there. `fab-notes.txt` and `order.txt` then say how to cut the pins (a `trim:` line).
_Avoid_: Tail, tail height, lead length, stub

**Pin request**:
`.status: .u.GPIO` in a board or a module: a pin of its own whose type is a capability of a part, a function that a row of the part lists or a peripheral offers (`.tx: .u.UARTE1_TX`). It asks for any pin of `u` that offers the capability, and the tool picks it before the nets are made and keeps the answer in the _Pin file_. The request's path is its key. It is joined and checked as the pin it got, and a pin that the source names is never given to one (ADR 0101).
_Avoid_: Pin assignment (the word for choosing providers is _Assignment_), pin constraint, pin hint, wildcard pin

**Pinout**:
The pin table of a part in one package: for each pin its kind, the name the datasheet prints, its pads, and its functions. Written as a `pins "PACKAGE" { ... }` block in the part, `pins "SOT-23" "SOT-223" { ... }` for packages that share it (ADR 0124), or an unnamed `pins { ... }` for a part whose pads do not depend on the package. A row may end with the pin's **absolute maximum**, `abs_min -0.5V abs_max 65V` (ADR 0126). A pin that a package lacks does not exist in that package. It names no CAD tool (ADR 0002, ADR 0016).
_Avoid_: Pin table, pin map (the pin map is the pins that requests got, _Pin map_)

**Placed board**:
A board as `fab` placed it, just before routing, saved as `<board>.placed.json` in the board's output folder by `fab --save-placed` (the `netsmith_pcb::Board` with the track margin `fab` routes with; `crates/netsmith-fabricate/src/placed.rs`). `netsmith route` routes it again alone, with no checking, solving or placing, for experiments on the router. A file of a working tool, not of the design: it is not committed, a `fab` without the flag removes it, and a file of another version of netsmith is refused.
_Avoid_: Layout (the layout file holds positions a person or the editor keeps), snapshot, checkpoint

**Placement requirement**:
A range on where a part, or the parts of a module, may go, stated in the board's physical file on the built-in `.place` of the part or module (`.usb.place.left &= -0.5mm +/- 0.05mm`, ADR 0045, ADR 0048). The fields are `x`, `y`, `left`, `right`, `top`, `bottom`, `rotation` and `side` (on a module, `side` is a requirement on every part of it). It comes from the case, panel or carrier, not from the part, so a board states it. The placer keeps to it, and `fab` stops with the numbers if the board does not meet it (`constraints = "warn"` or `--constraints warn` lists it instead; `--constraints error` overrides the manifest, and `--allow-drc-violations` does not waive it). The solved position is not in the physical file: it stays in the layout file. The same line in a board's circuit source is an error that names the physical file.
_Avoid_: Constraint (as the name of the thing a person writes: a finding is a `constraint` finding), position (a position is what the layout file holds)

**Plane**:
An inner copper layer that is one sheet of copper on one net, such as ground. The board names the net (`.process.inner1.net &= "GND"`); a through-hole pad or a via on that net joins it where it stands, and any other pad is joined by a via. KiCad's type for its layer is `power`.
_Avoid_: Copper fill, power layer (a pour is a different thing)

**Planned crossing**:
Two lines of different nets that the coarse plan takes through one tile on one layer with their places on the tile's edges alternating round it, so that one must change layer to pass the other. Each line takes a numbered place on every tile edge it crosses, in one order along the edge from both tiles. The plan pays for a crossing more than the two vias that avoid it, so a plan with none plans those vias instead.
_Avoid_: intersection (of two tracks' copper, a short)

**Port**:
A pin of an interface on a module or board. A port only passes a connection along to the terminals behind it. An interface that a board declares directly (`.output: Rail`) is also the port of that board in a system, which can join it to another board's port and reach nothing else of the board.

**Pour**:
Copper filled on an outer layer round the tracks and joined to one net, such as ground: `.process.top.net &= "GND"`. Unlike a plane it leaves room for the tracks, KiCad fills it, and vias at a pitch the board states (`.stitch_pitch`) join it to the other layers of its net.
_Avoid_: Copper fill, ground plane (that is a plane)

**Power symbol**:
How the editor's schematic draws a rail: the negative side of a rail (where the current returns) with the ground symbol, bars that shrink away from a wire, and the positive side with the supply symbol, a stem with an arrow head and the net's name beside it. Which nets are which comes from the checked model (a `Rail`'s `pos` and `neg`), never from the names; the ground symbols carry their names only when a board has more than one ground. Each pin of a main part on a rail has a wire to its own symbol, and pins next to each other on one rail share a bus and a symbol; a satellite's rail pin has a short stub and a symbol of its own (a supply symbol up, a ground symbol down), except that satellites side by side in a row, and satellites on the same rail within 20 grid steps of each other (two pull-ups of a bus, one above the other), share a short wire and one symbol where nothing is crossed. A supply whose pins on the sheet are on two or more parts that stand close together (stub ends within 16 grid steps, four pins or fewer), none of them a small part to ground (a decoupling or bulk capacitor, a TVS diode), has no symbol: it is wired, as a signal net is, with its name on a short stem up (else down) from the wire, because a symbol would only join what the wire joins already and would read as a port to somewhere (owner decision 2026-11-07). A supply with one pin on the sheet, or far-apart pins, keeps its symbol; grounds are always drawn with theirs. The KiCad schematic file does not use them.
_Avoid_: Power port (a net label is the name of a signal net at the end of a stub, and a power symbol is never one)

**Power tree**:
The view of a board's rails as cards: the voltage of each net a rail draws from, the total of the currents on it, and what the part that supplies it can give, joined left to right, from the sources to the loads, by the regulators and stages that make one rail from another and by the joins of a system. The numbers are the solver's, in the worst case.
_Avoid_: Power diagram, rail map

**Process**:
The design rules and stackup of the factory that makes a board, such as `std::mfg::jlcpcb::TwoLayer`. A board has one as its `.process` member. Each rule is the least the factory can make, written as a range the board may narrow upward; the rule used is the bottom of what is left. It is an interface with no pins, or a generic whose providers are interfaces: `TwoLayer` is a generic, and each copper weight is a provider (`JlcTwoLayer1oz`, `JlcTwoLayer2oz`) that the board picks by narrowing `.stackup.copper_weight` (ADR 0113).
_Avoid_: Fab house, rule set, technology

**Project**:
A directory with a `netsmith.toml` at its root, found from any directory below it. Its `src/` files are modules named by their path (`src/power.nsm` is `src::power`), and the commands with no file argument act on the files its manifest lists.
_Avoid_: Workspace, package, repo

**Project library**:
A symbol file (`.kicad_sym`) or footprint folder (`.pretty`) of a project's own, listed in its `kicad.toml` as a `[[library]]`, so that its parts map to symbols and footprints KiCad does not ship.

**Provider**:
A `part`, `module` or `interface` whose header says `provides Name` (`part AcmeMicroCoax provides CoaxReceptacle`), naming a generic; an interface provides a generic with no pins and no catalog, such as a process (ADR 0113). It has every member and statement of the generic, does not declare them again, and adds its own pin table, numbers, catalog and, for a module, parts. It is a candidate for a use of the generic when its file is imported, directly or through other files, by the board's file. A provider's name is unique among the providers of one generic, and it does not contain a use of the generic it provides (ADR 0068 section 13). A provider is never a **supply provider**, the source of price and stock; say which one is meant (ADR 0068). A part may provide a standard generic such as `Resistor`: the generic's own catalog and pads are not read into it, and the design lists the generics it provides (`Part.provides`), so that the schematic and the placer treat it as the kind it provides.
_Avoid_: Implementation, implementer, subtype, instance (of a generic)
