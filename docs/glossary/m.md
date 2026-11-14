# Glossary: M

**Manifest**:
A project's `netsmith.toml`: its name, the top-level files that hold boards, the output and fab directories, and the KiCad version. It lists what to build and the orders to make from it; it does not describe a circuit.
_Avoid_: Config, settings

**Mapping**:
The TOML file that says which KiCad symbol and footprint a kind of part has in each package: `std/kicad.toml` for the standard library, and a `kicad.toml` at a project's root for its own parts and its own libraries. It is the only place KiCad's names appear (ADR 0021).
_Avoid_: Symbol table, library table

**Marked value**:
A capacitor's `.capacitance`: the value printed on the part and listed on a bill of materials, with its tolerance. A design states it where it means a part, such as a bypass capacitor.
_Avoid_: Nominal

**Mate**:
Two connectors that a system joins with `~` between their `.mating`: contact N of one meets contact N of the other (`.carrier.slot.mating ~ .card.j1.mating`, ADR 0040). The checker reads what each board has behind each contact (ground, power, a signal, or open), refuses what cannot meet (a header with a header, two outputs, ground against power, a count or pitch that differs), and joins the ports the contacts carry, so the same fact is not said twice with a port join.
_Avoid_: Connection, link, plug

**Mated height**:
The distance between the faces of the two boards of a _Mate_: the two connectors' _Seats_ added, with both tolerances (ADR 0110), so the std 2.54 mm header in the std socket gives 10.65 mm to 11.35 mm. The design stage gives it for each mate of a system (`netsmith_design::system_mates`); a mate with an unknown seat has none.
_Avoid_: Stack height, board spacing, standoff height (a standoff is another part)

**Meander**:
A serpentine of 45 degree bends that the router draws on a straight run of the shorter net of a diff pair, so that the pair's two nets are as long as each other (issue #431). Its bumps stand on one side of the run, away from the partner, with an edge gap of three track widths (or the clearance, if larger) between neighbouring legs; it goes only where the run is not coupled to the partner and where it keeps every space the design rules check asks of a track (`netsmith_router::meander`).
_Avoid_: Serpentine, trombone, tuning (for the copper itself)

**Member role**:
What a member is to the checker and the solver, written `#[role(bias)]` above it (ADR 0125): `bias`, `bridge_voltage`, `current`, `supply_peak`, `supply_reverse_peak`, `limit_current`, `supply_fault_current` or `count`. The role, not the member's name, gives the member its value. Not a **Role** of a bus end (`SpiRole::Slave`).
_Avoid_: Magic name, special member, well-known field

**Minimum on-time**:
The shortest time a buck IC's high-side switch stays on, and its twin the minimum off-time: `.min_on_time` and `.min_off_time` of `Buck`, `optional` members that a part states at the largest its datasheet gives (ADR 0093). `BuckStage` checks that its `.on_time`, `vout / (vin * fsw)`, and its `.off_time`, `(vin - vout) / (vin * fsw)`, are not below them in the worst case the ranges give: the minimum on-time bounds the highest input for an output and a frequency, the minimum off-time the least headroom of the input over the output (the IC's dropout). A linear regulator's dropout is its own member, `.dropout`.
_Avoid_: Minimum pulse width, minimum duty cycle, tON limit

**Module**:
A reusable sub-circuit that is placed on a board or inside another module. Its interfaces, pins, and declared values are joined and set from outside; a value of a part or module inside it can only be narrowed (`&=`, ADR 0116), and the pins of the IC its user named for a type parameter joined (`.reg.ic.pg` of `Buck<Lt8641>`, ADR 0120).
_Avoid_: Circuit, block, sheet, subcircuit
