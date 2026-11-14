# Glossary: A

**Absolute maximum**:
The voltages a pin survives, `.abs_min` and `.abs_max` of the pin, relative to its ground. The solver holds them against the worst-case voltage of the pin's net (ADR 0046), which for a net the DC analysis solved is the range of its node voltage (ADR 0076, amendment 12). A part states a pin's limits only at the end of its pin row, `abs_min -0.5V abs_max 6V` (ADR 0124, ADR 0126), never with a statement; a `Logic` line or a bus pin is measured from its interface's ground, any other pin from the part's ground or from the pin `from` names (the `Lt8641`'s `SYNC/MODE`); a pin joined to nothing outside its part is not judged.
_Avoid_: Rating (a rating is a field such as `.max_power`)

**Alias**:
A pin with no row in the pin table that the part joins to a pin that has one (`.vin.neg ~ .vout.neg`). It is that pin, so it needs no pad of its own and adds no pin to a net in the schematic.

**Assembly**:
Who puts a part on the board: the field `.assembly` of every part, of the type `AssemblyMode`: `AssemblyMode::Fab` (the board house places it), `AssemblyMode::Hand` (soldered by hand), `AssemblyMode::None` (not fitted at all: a hole, a fiducial) or `AssemblyMode::Consigned` (the board house places it, and the owner ships the part to it). It decides the BOM, the placement file and the notes of `fab` (ADR 0024, provisional). A project may set a default for it in `netsmith.toml` (`assembly = "fab"`, ADR 0028; the manifest and the catalog files write the lower-case word). A board may narrow a part inside a module (`.a.j.assembly &= AssemblyMode::Hand`, ADR 0116); a module that offers one choice for all its parts declares a value of the type `AssemblyMode` (`.assembly_mode`) and ties its parts to it with `.j.assembly &= .assembly_mode`; the board's narrowing of that value is the design's own statement for those parts (ADR 0042, ADR 0054).
_Avoid_: Population, fitting, DNP

**Assumed window**:
The range taken for the input of a regulator while the parts in series with it are picked, before the network can be solved: from the largest of what the part and the design need of it to the highest supply that reaches it. It is an assumption about a valid design, it is never kept (the picks are made again without it, from the one row each was given), and what the pass after the picks writes is the circuit's (ADR 0076, amendment 7).
_Avoid_: Seed, start window (those are written by hand and stay in the model as a claim), enclosure (an enclosure holds whatever a pick chooses; an assumed window holds for the picks that a valid design allows)

**Attribute**:
`#[name]` or `#[name(argument, ...)]` above a `generic`, a `part` or a member of one, which says what it is to the checker, the solver and the tools (ADR 0125): `#[bank]`, `#[series(E24, E96)]`, `#[kind(resistor)]`, `#[value("LED")]`, `#[assembly(hand)]` on a definition, `#[role(bias)]` on a member. The standard library states its own with the same attributes; no list in Rust names its definitions. A part has those of its definition and of the generics it provides.
_Avoid_: Annotation, decorator, marker, pragma, tag
