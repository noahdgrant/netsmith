# Glossary: C

**Candidate**:
One real part in a catalog (one row of its file), with a value for each column, or one provider of a generic in scope. The solver takes the first candidate that satisfies every requirement; for a use of a generic the candidates are the providers, in a fixed order, and the rows of the picked provider's catalog are then chosen as always.

**Carried voltage**:
The voltage of a rail on a node of a `~>` chain, as the analysis gives it: the source's, less what each part in series drops at the current it carries. The analysis writes it into the rail's `.voltage` where nothing states one, so the board does not restate the drops (ADR 0076, amendment 5).
_Avoid_: Propagated voltage, derived voltage (a derived value is any worked-out field; this one is the node's)

**Case model**:
A STEP file of the board's case, kept in the project, that a `CaseModel` member of the `case { }` section of the board's physical file names (`.shell: CaseModel { .file &= "case/box.step"  .z &= -1.6mm  .rx &= 180 }`, with the fields and the rotation order of a _STEP model_, in the _Scene_'s frame), for the board and its parts' bodies to be drawn in and checked against (ADR 0109). It is a drawing and a check, never a height, an outline or a hole: those stay statements of the `case` section. netsmith bundles and downloads none; a file that is not there leaves the fit unchecked, a warning, and an error with strict constraints.
_Avoid_: Enclosure (a solver range, above), case STEP, housing model

**Catalog**:
The columns of a list of real parts that a part's fields are matched against when the solver picks one. A catalog may be keyed by a column (`catalog mcu by .mpn`), and then the design names the row when several fit (one row that fits is taken without a name). The rows are a CSV file named after the catalog, in the order the solver tries them (ADR 0022); a project adds its own files: every `*.csv` in its `catalogs/` folder, or in another folder `catalog_dirs` names in `netsmith.toml`. A row with an empty cell is never offered, except in a column that a catalog may leave empty (`temperature_coefficient`, `rms_current`, `dielectric`, and a TVS diode's `breakdown_voltage`, ADR 0073), and then only to a design that says nothing about the field by any route (a narrowing, a relation, a join, a balance).

**Catalog file**:
A CSV file of the rows of one catalog: a header of the catalog's columns (field paths without the leading dot) and the fixed columns `lcsc`, `mpn`, `library`, `price`, `price_breaks`, `stock`, `source`, `date`, `checked`, `name`, `label`, `notes`. `price`, `stock` and `library` are JLCPCB's numbers, entered by hand, and `date` is the day they were read; the solver does not read them and the bill of materials uses them only when no supply provider has the part (ADR 0047); the build notes a picked part whose row states a `stock` of 0, or whose `lcsc` `considered.csv` records as out of stock, and never changes the pick for it. `checked` is the day a refresh last verified the row against LCSC. A row that names a part must have a `source`. The standard ones are `data/jlcpcb/catalog/*.csv` (ADR 0022). A project's are the `*.csv` files of its `catalog_dirs` folders (default `catalogs`) and the single files its `catalogs` list names (ADR 0022, amended).

**Catalog height**:
The `height` cell of a catalog row, a fixed column (ADR 0094): how tall the part's body stands above the face of the board, the greatest the manufacturer's document allows (the maximum, or the nominal plus the tolerance), with the document, its revision and the page in the row's `source`. It is the part's `.height` when the design states none; a stated `.height` wins, and no row is chosen or refused for its height. An empty cell means the height is unknown, which is never zero.
_Avoid_: Default height, typical height

**Catalog refresh**:
`netsmith catalog refresh`, the opt-in command that checks each row of catalog files against LCSC's product record (never JLCPCB): the manufacturer's number and package. A row that agrees is VERIFIED and gets the day in its `checked` column, the one cell it writes (only if the file has the column); a row that disagrees is a CONFLICT and a part LCSC no longer lists is MISSING, and neither is changed. It never writes `price`, `stock`, `price_breaks`, `library` or `date` (JLCPCB's numbers, which LCSC's differ from), and says in a note when LCSC lists stock 0 where the row has some. `catalog age` lists the rows whose price and stock were read more than 90 days ago, the age at which the editor's bill of materials calls a price stale, and the rows whose price and stock are unknown (ADR 0039, ADR 0047; a refresh never changes `date`).

**Cause**:
Why a note of a build is there: the one thing that would make it go. The tool that makes a note says it, as a typed field, never read back from the text (`netsmith_core::Cause`, sent as `Problem.cause`): _not stated_ (a check waits on a value nothing states), _part model_ (a part's definition leaves a level, a current or a limit open), _unjudged_ (a pin or net netsmith cannot judge yet), _circular_ (values that wait on each other), _supply_ (a rail nothing supplies, a load or a converter not settled), _pick_ (a provider stands in for a generic, a catalog row is out of stock), _track_ (a width that is a lower bound for a current not known) and _other_. The set is closed; the editor's Problems dock groups the notes by it, with a count for each (issue #192). An error and a warning have none.
_Avoid_: Reason, category, kind of note, note type

**Cell**:
One board of a panel's grid, counted along the rows from the top left.
_Avoid_: Slot, tile

**Chain**:
A run of series connections from one pin through bridge parts to another. A series chain passes one current, the through-current (`.current`) of each of its parts, taken from the first terminal of the part's `bridge` to the second; the first part's is drawn from the net the chain starts on (ADR 0014). A chain from a rail's `pos` to the same rail's `neg`, untapped, whose parts are each a `dc resistor` or a `dc diode` the way it runs, also has its current set by Kirchhoff's voltage law before the pick: the rail's voltage less the forward voltages, over the resistances (ADR 0113).

**Checkpoint**:
A place in a long call (the checker's instances, a trial of the solver's search, a part or a net of the design stage, a sheet or a pass of the schematic's layout, a move of the placer) that asks whether the job it belongs to was cancelled, and ends the call if so. A job is cancelled with a token (`netsmith_core::job::Cancel`) that the one who started it installs for its thread; a cancelled call unwinds to him with nothing of what it made kept, and the analysis that was cancelled leaves the last reading as it was (ADR 0062). It is the _Cancel_ button's reach: a call is stopped at its next checkpoint, not at once.
_Avoid_: Interrupt, abort, kill (a job is never killed; it is asked, and ends where it can)

**Circle of values**:
Fields that depend on each other through their equations and balances of currents while none has a value, such as the input voltage, the input current and the drop of a regulator fed through a series resistor. The build names the circle in a note and does not evaluate the checks that need it. A range stated on one of its fields starts it, and so does the range a part works in (`.vin.voltage >= 3V`, `<= 65V`) together with a value to start from (ADR 0053, proposed): the solver then grows the supply's range until the equations map it into itself and accepts it only if nothing else inside the part's range satisfies them.
_Avoid_: Cycle, loop, algebraic loop (a loop is a feedback circuit, not a set of values)

**Closed-form check**:
A relation written in the std library that gives a number from a formula (no time stepping), such as the ripple of a buck stage's output voltage, and that the solver judges in the worst case of the ranges it reads, like any other relation (`docs/research/closed-form-checks.md`; the owner's option A, 2026-11-09). It states what it assumes and when it is wrong or optimistic in the doc comment of the member. Not a simulation: it models no startup, load step or waveform, which a circuit simulator on demand would (issue #306).
_Avoid_: Simulation, transient analysis, estimate

**Coarse plan**:
A global route of every line of a board over its tiles, made before the router's detailed search: each line is routed from tile to tile, and the lines negotiate for the tiles' edges and vias until as few as possible carry more than they hold and as few lines as possible cross (a planned crossing; the header of the router's `plan.rs`). Off by default; `netsmith route --plan` turns it on for experiments.
_Avoid_: global route (the general term, for any router), floor plan (where parts go)

**Collapse**:
A corner of an island in which a regulator's input falls to nothing because its output asks for more power than the source can give through the resistance in front of it (12 V through 10 ohm gives at most 3.6 W): the circuit has no operating point there, and the analysis reports an error that names the regulator, what it asks and the states (ADR 0076, amendment 3).
_Avoid_: Brown-out, undervoltage (an input that is low but has an operating point is below the regulator's minimum, which its own relation reports), failure to start

**Connector**:
A part that is a way in and out of a board: one that provides the standard generic `Connector` (`part MolexPicoBlade provides Connector { ... }`, ADR 0068; every standard header, socket, `Coax`, `UsbC*`, `Obd2M` and `ProgrammingPads6` does). The generic states the letters `J`. The schematic layout and the placer find connectors by it, so a project's own connector is one and an IC named `HeaderBuffer` is not (owner decision 2026-11-10, issue #166).
_Avoid_: a connector by its name or its designator letters (`J`, `P`), a connector flag or kind list

**Consigned part**:
A part that the board house places and solders, and that the owner buys and ships to it (JLCPCB's consigned parts service). Stated as `AssemblyMode::Consigned` (`.assembly &= AssemblyMode::Consigned`, a module's `.assembly_mode`, the word `consigned` in a catalog row's `assembly` column or the manifest's `assembly`). It is in `cpl.csv` and in `bom.csv` with its manufacturer's number as the comment and the LCSC cell empty, needs no `lcsc`, is listed in `fab-notes.txt` under `consigned:`, and is printed as `consigned` by `build`. The stock note and the unresolved generic warning do not apply to it, since the owner supplies it (ADR 0084).
_Avoid_: Customer-supplied part, owner-supplied part, free-issue part, kitted part, supplied part (use _consigned_, as the board house does); and, for the mode, a flag on a `Fab` part

**Constant value**:
A value that no choice of parts can change: the field, and the fields joined to it, have a value now and nothing works any of them out. The stated voltage of a rail is one. Parts that read only such values and nothing else in common are searched apart (ADR 0057, proposed).
_Avoid_: Fixed value, frozen value

**Corner**:
One combination of the ends of the ranges an island has (a resistance at its least or its greatest, a supply or a load at either end, a diode's forward voltage at either end, a pin's output resistance at either end) with a state for each diode (blocking or conducting), each MOSFET (off or on) and each logic pin (high, low or high impedance). The range a current or a voltage takes is the hull of its values over the corners, which counts each value once (ADR 0076, section 3).
_Avoid_: Case, scenario, worst case (the worst case is a result, an end of the range a corner makes)

**Corridor**:
The tiles of a line's coarse plan, on either layer. A detailed search that follows the plan pays for a step outside its line's corridor only on a tile the plan crowds; it is a cost, not a wall. A line of a pour's net, of a diff pair or of a net its current widens has none.
_Avoid_: channel (the room between two rows of pads)

**Cross-check**:
`netsmith fab --cross-check`: the board is plotted with KiCad as well and its Gerbers, drill files and placement are compared with netsmith's own (layers as bitmaps), and KiCad's design rules check is run and its findings compared with netsmith's (by rule and place). KiCad is a witness there, never a source of files or of the gate (ADR 0034, ADR 0037).

**Crowding**:
How full the coarse plan leaves a tile on a layer: the most of its four edges' planned tracks over what the edge holds. A step outside a line's corridor costs nothing on a tile crowded less than 70 %, and the whole crowding cost at 100 %.
_Avoid_: congestion (the router's negotiation over cells), demand

**Crowding map**:
The router's count, for each 1 mm square of a board, of how often its searches for the nets in a line's way (paths that may cross other nets' copper, in rip-up, negotiation and the chain of rip-ups) passed through the square, over a whole run (`Routing::crowding`, written to `routing-report.json` when connections are left, and drawn as `crowding.png` by `netsmith route --stages`). A high count is where lines fought over the room.
_Avoid_: congestion map, heat map

**Cutout**:
A rectangular hole inside the outline, with an optional corner radius (`std::pcb::Cutout`).
_Avoid_: Slot, notch, hole (a hole is drilled)
