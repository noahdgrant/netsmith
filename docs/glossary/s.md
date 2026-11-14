# Glossary: S

**Satellite**:
A small part that the editor's schematic draws beside the pin of a main part (an IC, a connector) that it serves: a decoupling capacitor, a pull-up, a bootstrap capacitor, a divider. It is wired to that pin always, and its rail pins have a stub and a power symbol of their own (see _Power symbol_). The main part and its satellites are one block.
_Avoid_: Passive (a satellite may be a transistor), helper

**Scene**:
The one three-dimensional description of a board that the Rust side builds and writes as glTF binary (`.glb`): the board, the copper and the part bodies, each node with stable ids in its `extras` (part path, designator, net, finding code, _Height provenance_). The 3D view displays it, netsmith's own rasterizer draws PNG renders of it (ADR 0105), and it is written as a _STEP file_ for CAD (ADR 0107).
_Avoid_: Model, mesh (a mesh is one piece of it), 3D export

**Schematic layout file**:
`<stem>.sch.toml` beside a source file (`src/main.nsm` has `src/main.sch.toml`): where a person moved a part in the editor's schematic, so that the drawing stays as they arranged it across edits. For each board, then each sheet (a module instance, or the board's own sheet), a part is keyed by its path from the sheet and holds an offset of its block (`dx`, `dy`, millimetres on the 1.27 mm grid) from where the automatic drawing puts it, optionally a `rotate` (0, 90, 180, 270) and a `mirror`, or a `pin` to a place from the sheet's reference part; a note is keyed `<path>#doc` or `<path>#relation` and has an offset. A part with no entry is placed automatically; an entry whose part or sheet is gone is a finding (stage `layout`, with the file's line) and is ignored, and so is an entry the drawing cannot honour (its block would lie on another part, stand too far from the drawing, or break a rule of the drawing). The engine applies the file before it routes a wire, so the wires go round what was moved. It never holds a wire, a label or a net, so it can move the drawing and never rewire it. Optional, written by the editor, editable by hand (ADR 0061). The code type is `LayoutHints` (`netsmith_view::hints`) and its entries are _hints_; the block is a main part with the _Satellites_ that serve it.
_Avoid_: Layout file (that is the PCB's), sidecar, positions file, `.layout.toml` (ADR 0019's first name for it)

**Schematic note**:
How the editor's schematic shows the source's own writing: the doc comments (`///`, `//!`) on a board, a module, a part or an interface, and every `==` relation of a board or a module, each as a framed box of wrapped text with no wire or leader line to anything. A doc comment is in the page's sans serif; a relation is in a monospace face under a small heading (`Relation`), with what the solver settled on for the field it gives a value to. The box of a part's note stands beside the part in free room, a board's or a module's stands at the top left of its sheet under the title, and a note with no room near its part goes to the margin. Which text is a note comes from the checker's model (`Design::notes`, ADR 0060), never from a `//` comment, which is not drawn. It joins nothing: `netsmith_view::check` leaves it out of the netlist and reports a note that lies on anything else (`note-overlap`) or whose text escapes its frame (`note-text-overflow`). The code type is `Note`; the word for the drawing is the schematic note, because a build's _note_ (a message that names what the solver left alone) is another thing.
_Avoid_: Annotation, comment (that is the source's `//`), callout, text box, label (that is the silkscreen part, and a _Net label_ is a net's name), title block

**Seat**:
How far above the face of its board a connector's partner rests when the two are mated, `.mating.seat` (ADR 0110): a header's plastic top, a socket's body top. It is a fact of one part, true whatever it mates with, a range with the maker's tolerance, and comes from the `header` catalog's `mating.seat` column with the drawing in the row's `source` (2.50 +/- 0.20 mm for the std 2.54 mm headers, 8.50 +/- 0.15 mm for the sockets). Unset means unknown, never a default, and never the part's height, which is its tallest point.
_Avoid_: Stack height (that is the _Mated height_), seating plane, insertion depth

**Seed**:
A whole number in the layout file (`seed = 3` in a board's table) that picks which arrangement the placer finds: the same seed always gives the same board, another seed another one (ADR 0033). When it does not place the board, the placer tries the seeds after it, up to `place_seeds` in `netsmith.toml` (8 by default), and `fab` writes the seed that places it to the file; a seed the file states that no longer places the board is stale, and is told as a stale pick lock is (#458).

**Shared bus**:
A bus whose members have a pin on more than two main parts of a module, or on two and go on to another module: an I2C bus with several devices, an SPI bus with a clock and data lines that every device has. One thick line with a spine at each part and a branch from each spine to the line, and the members' entries as for any bus (ADR 0067). It needs three members between two places and two among three or more.
_Avoid_: Multidrop bus, tap, bus with taps, daisy chain

**Shared resource**:
`shared UARTS limit 2 { UARTE0 UARTE1 UARTE2 .p10 }` in a part: the hardware that at most `limit` of its members use at a time (one without `limit`; a limit is at least 1 and fewer than the members). A member is a peripheral, a function (in use when chosen on a pin) or a pin (in use when its function is chosen or the board connects it). Using more members than allowed is an error that names the part, the resource, every use and its line; it is judged for each instance of the part, before the solve, from the functions the board chose (ADR 0071).
_Avoid_: Conflict set, exclusive set, group, resource (alone)

**Side**:
The face of the board a part is on, front or back. A part on the back is its footprint seen in a mirror, with its surface-mount pads on the back copper.

**Signal layer**:
An inner layer for tracks, which names no net: `.process.inner2.use &= "signal"`. The router draws on the outer layers only, so it holds only the copper of through-hole pads for now; it is plotted and checked like the outer layers.
_Avoid_: Routing layer, mixed layer

**Sink current**:
The current a driver can sink while it holds a line at its low level, as a datasheet gives it with the `VOL` it guarantees: `.sink_current` of an `I2c` interface, an **Optional member** that `~` does not equate between two parts (the datasheet's `IOL`, at the part's `.levels.vol`). A driver sets a least pull-up resistance, `(supply.voltage - low_level) / sink_current` (NXP UM10204 Rev. 7.0, section 7.1), a **Closed-form check**: the checker states it for each part on the bus with the part's own figures (ADR 0102, a **Derived figure**), and a board may state the optional `.sink_current` and `.low_level` of `I2cPullups` itself instead. A part with no figure beside parts that have one is named. Not the current a pull-up draws (`.current`) and not the current a supply can give.
_Avoid_: IOL (as a name), sink (alone), drive current

**Sink, source and dual-role receptacle**:
The three generics of a USB Type-C receptacle in `std::parts::connector`, one for each way power goes through the port (#463). A **sink** receptacle (`UsbCSinkReceptacle`) takes power from the cable: its VBUS is a source of the board (`dc source .vbus`). A **source** receptacle (`UsbCSourceReceptacle`) gives power: the board drives VBUS and the device on the cable draws, so `.vbus.current >= 0A`. A **dual-role** receptacle (`UsbCDualRoleReceptacle`) does either, and the board states a signed range for `.vbus.current`. A receptacle that carries no power role is a `Connector` with a plain `Rail`.
_Avoid_: host port, device port (for the power direction), DRP

**Skeleton**:
The first source of a port, which `netsmith port skeleton <original.kicad_pcb>` writes with the port's _Parts file_ (#452): one member for each footprint of the original, named by its designator, and one join for each net, with the original's net names. A footprint that is not a plain resistor, capacitor, inductor or LED is a **gap** (marked `GAP`) until the project has its part file, so the skeleton does not build until each has one.
_Avoid_: scaffold (the word for `netsmith init`), template, stub

**Skill**:
A directory in `.agents/skills/` with a `SKILL.md`: instructions an agent loads when its description matches the task. `netsmith-*` skills are for working in a netsmith project and `netsmith init` copies them into one; `netsmith-dev-*` skills are for working on netsmith and stay in its repository (ADR 0015).
_Avoid_: Prompt, plugin

**Source (at DC)**:
A rail that holds its nets over the range of its voltage in the analysis: a `Rail` that the board declares itself, or the rail that a part gives with `dc source` (a connector's supply, ADR 0076, amendment 5). Its `pos` net is held over the range the design states for `.voltage` and its `neg` net at zero; the network on either side of it is an island.
_Avoid_: Supply part, power source part (a source is a rail, and the part that gives it states `dc source` for it)

**Spine**:
How the editor's schematic draws the power path through a regulator: one row from left to right, found from the netlist, the pin kinds and the rails, never from what a part is called, and only where the rails are known. From the left: the chains of series parts of the module whose far end goes on to another module (a fuse and a diode, with a TVS diode on the node they end in), the input chain (a ferrite bead, from where the power comes in to the IC's input pins), the IC with its input on the left and its switching pin on the right, the output chain (the inductor) and the output node. A **series part** is a two-terminal part whose two pins are on two different nets, neither a ground, that is not across two pins of the IC (that is a _Pair satellite_); each stands on the row, turned along it, and a chain goes on through a net only while one more series part is there and no main part has a pin on it. The capacitors to ground hang from the node they serve, the smallest nearest the IC, each with a stub and a ground symbol of its own (side by side they share one wire and one symbol, see _Power symbol_); the feedback divider hangs from the output node with the feedback pin wired to its middle; a rail on the row is wired along it and has one symbol, at its outer end. The pair satellites of the IC (a bootstrap capacitor) stand beside its two pins and the row never crosses them. The metrics `spine_bends` (the bends along the longest series chain) and `detached_chain` (chains that are neither on the IC's row nor wired to it) have target 0.
_Avoid_: Power path (that is the current; the spine is its drawing), backbone, main line

**Stackup**:
The layers a board is built from: the number of copper layers, the finished thickness, the copper weight and, for a board with inner layers, its _Build-up_. It is a member of a process, `.process.stackup`.
_Avoid_: Layer stack

**Stage picture**:
A PNG of one stage of the router's winning try (`stage-1-first-pass.png` to `stage-4-final.png`, written by `netsmith route --stages`): that stage's copper, front and back in two colours, the pads and vias, and the lines still left drawn bright on top; `crowding.png` beside them draws the crowding map. A picture for an agent to open, not a file of the fab package (`netsmith_pcb::picture`).
_Avoid_: Screenshot, render (`netsmith render` draws the 3D scene), snapshot (the router's data a picture is drawn from)

**Stage timings**:
One line on stderr for each stage of a run (`timing: solve LedExample 12.3 ms`), printed when the environment variable `NETSMITH_TIMINGS` is set to anything but empty or `0`, and nothing otherwise (`netsmith_core::timings`). A stage inside another (`solve.dc` in `solve`) is counted in the one around it. The numbers are wall time of one run, taken on whatever the machine was doing.
_Avoid_: Profile, trace (a `Trace` is what a build knows about its source), benchmark

**Standard footprint library**:
netsmith's own `.kicad_mod` footprints and `.kicad_sym` symbols in `std/libs/`, carried by the binary under the library name `netsmith`, for the packages and parts that KiCad's libraries lack and the generators do not make (ADR 0087). A row of `std/kicad.toml` names one as `netsmith:Name`; a board that uses one gets the library written beside its `.kicad_pro` and listed in KiCad's library tables. Each footprint is drawn from a manufacturer's package drawing, with the page and what was measured recorded.
_Avoid_: Bundled library, built-in footprints (the generators of ADR 0043 are the generated footprints)

**Standard height**:
The greatest body height a package's standard allows (the JEDEC outline's `A` maximum, `SOIC-8` 1.75 mm by MS-012), read from a maker's drawing that cites it, for a generated footprint listed in `crates/netsmith-pcb/src/standard_height.rs` (ADR 0106). It stands in for a part with no stated and no catalog height, with the provenance `standard`: a limit it fits is a pass said to rest on the standard, a limit it exceeds leaves the part unchecked. It is never written as the part's `.height`.
_Avoid_: Nominal height (a nominal is the middle of a tolerance), default height, typical height

**Stated rail**:
A rail whose `.voltage`, or the `.voltage` of any rail joined to it by `~`, a statement of the design narrowed (`.stage.vin.voltage &= 4.2V..5.5V`, a module port's seed or a window), on a node the DC analysis solved. The range it states has to enclose the range the analysis found, and a statement that does is kept as it is; one that does not is an error that names the field, both ranges and the states at the end that goes outside (ADR 0076, amendment 17). Not checked: a rail past a regulator's output, and a node a source holds (a source's voltage is an input).
_Avoid_: Seed (the start window of a circle, which is one use of a stated rail), claim, assumed window (it is never kept, and is not stated)

**STEP file**:
The board and its parts as exact solids for mechanical CAD (ISO 10303-21, AP214), that `netsmith board3d --step` writes from the _Scene_ and `netsmith fab` writes into the fab package as `<board>.step`: an assembly with the board and one product per part, named by its designator; a part of unknown height is its outline only, never a solid (ADR 0107). A part drawn from its _STEP model_ holds that model's own solids, copied in (ADR 0108).
_Avoid_: STEP model (a maker's or KiCad's file of one part, the next term), 3D export, MCAD export

**STEP model**:
A STEP file of one part, kept in the project, that a part names with its built-in `.model` (`.usb.model.file &= "models/usb4105.step"`, with an offset in mm and a rotation in degrees, of the prelude's `StepModel`), drawn in the _Scene_ in place of its _Package shape_ (ADR 0108): placed in KiCad's frame for a model, so a footprint's `(model ...)` numbers can be copied in, read by netsmith's own reader and copied exactly into the _STEP file_. It is a drawing and never a height. netsmith bundles and downloads none; a file that is not there is a warning, and the part keeps its generated shape.
_Avoid_: 3D model (KiCad's word, which also names VRML files), user model, CAD model

**Stray wire**:
Wire or dot in the editor's schematic that serves nothing: an end that touches no pin, power symbol, flag, label, X or other wire of its net (a dead end, such as a rail that runs on past its last tap), two wires of one net along one line, a hook (out, across a grid step or less, and back), a wire that is a point, a dot where fewer than three arms of a net meet, three arms and no dot, a wire along a pin's line, or along the outline of a body or a flag within 0.7 mm. The drawing has none: the engine takes them off when it has routed, and `netsmith_view::check` reports any that is left.
_Avoid_: Dangling wire, spur, stub (a stub is the short wire straight out of a pin)

**Supply budget**:
What a source rail that states its own `.current` gives at most, the magnitude of the low end of the stated range (`.supply.current &= -1A..0A` is 1 A): the DC analysis finds a fault when even the least the loads on the rail's net draw is more than the budget (ADR 0076, amendment 13). A rail that states no `.current` has no budget.
_Avoid_: Current limit (a part's own relation), Load rating

**Supply peak**:
The `.supply_peak` of a part in a `~>` chain, a `Fuse`: the `.peak` of the rail whose `pos` the chain starts on, which the checker gives the part without being told (ADR 0086). The chain carries it as it carries the current, so a part in series, which is across no rail and has no `.bias`, is checked against the highest voltage it has to interrupt. A part in no chain from a rail has none, and a design states it.
_Avoid_: Chain voltage, input peak

**Supply provider**:
A source of price and stock for a part number, behind one trait: given an LCSC or manufacturer's number it answers with quotes, each saying its channel (JLCPCB assembly stock or LCSC retail stock), stock, price breaks and when it was read. Asked when the bill of materials or `fab` is made, cached with a timestamp, and when it has no answer price and stock are **unknown**, never guessed and never 0. Configured in the `[supply]` table of `netsmith.toml`; credentials never go there (ADR 0047).
_Avoid_: Supplier feed, price list (a file provider reads one)

**Support section**:
The `support { ... }` section of a part: the parts and connections its datasheet's application circuit puts around it (a bootstrap capacitor, the resistor on `RT`), which come with the part wherever it is placed, at the path below it (`reg.ic.boost`, ADR 0115). It is how a part that provides a generic carries what not every provider needs.
_Avoid_: Application module, companion parts, external components, satellite parts

**Swing**:
The range of voltage a `Logic` line can have, `.swing` of the `Logic` or of the `Levels` of a bus interface (ADR 0081), relative to the line's ground (the `neg` of its `.reference` rail) and read as written, so `-0.3V..1.8V` goes 0.3 V below the ground. A part states it when its outputs swing to a supply that has no pad. The absolute-maximum check holds a receiver's limits against it; a line that states none swings from its ground to the top of its reference rail.
_Avoid_: Output range, internal rail (a rail has pads)

**Synthesized provider**:
The second provider of a standard generic whose catalog is `resistor`, `capacitor`, `inductor` or `led`: the generic parts of an E-series (ADR 0047), offered after the rows of the generic's own catalog (an LED has no series: its generic parts differ only in the package, and the design supplies the color and the forward voltage). It is Rust, not a definition, because the language has no way to say how a value is generated (ADR 0068 open question 10), and there is one for each of the four catalogs `resistor`, `capacitor`, `inductor` and `led`, described by one table of the solver (`standard.rs`: the columns it fills, the value columns of ADR 0065, and the columns a file of rows may leave out, which are the ones it leaves empty). A part or provider that a project writes gets the synthesized parts and the omittable columns when it lists one of the four catalogs, and none otherwise, whatever it provides (ADR 0068 section 14, item 8).
_Avoid_: Series provider, virtual provider

**System**:
Several boards and the joins between their ports, written `system Demo { .main: MainBoard  .io: IoBoard  .main.output ~ .io.input }`. A system holds only boards and says which ports meet, or which connectors are mated; it names no nets. Each board is still made and ordered on its own.
_Avoid_: Assembly, backplane, harness

**System scene**:
The boards of a _System_ placed against each other by their _Mates_, in one scene (ADR 0110): each member's _Scene_ as it is alone, moved by one rigid motion (a turn about z, a flip when it faces the other way, an offset) so that each mated connector's pad centres land on its partner's and the boards' faces are the least _Mated height_ apart, in the frame of the first member. A member no mate places is drawn apart with its reason, never at a guessed height. `netsmith board3d --board System` reports it, with the findings across boards (`system_collision`, `system_unchecked`, `mate_misaligned`, `system_apart`).
_Avoid_: Assembly (that is `.assembly`, who solders the parts), stack, system model
