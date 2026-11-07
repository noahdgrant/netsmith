---
name: netsmith-write-nsm
description: A cheat sheet for writing correct .nsm source files, with pointers to the language guide and the standard library. Use when writing or editing any .nsm file in a netsmith project.
---

# Write `.nsm`

Read the language guide (`docs/netsmith/LANGUAGE.md`) for any construct you are unsure of. It has the syntax, an example and the real error for each construct, and a catalogue of the standard library at the end.

## Shape of a file

```nsm-fragment
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board Name {
    .supply: Rail { .voltage &= 5V +/- 5% }   // a member with values set
    .r1: Resistor {}                           // the solver picks it
    .supply.pos ~> .r1 ~> .supply.neg          // a series chain
}
```

## Rules

- A member is `.name: Type`. `.name` is local. A bare `name` looks outward through enclosing blocks.
- `&=` narrows a field to a constant or range. `==` is an equation that must hold in the worst case. `>=` and `<=` are bounds, checked in the worst case. There is no `=`.
- `.f ?= value` is a default a module or part offers for a field: it applies only if no `&=` states `.f` (a value reached through `==` or `~` does not count), a board's `&=` drops it without needing to overlap it, and two `?=` on one field are an error.
- State what a part's value or a declared value needs. A resistor, capacitor or inductor picked with nothing limiting its value is an `unconstrained pick` warning (an error with `--strict`), and a value a module or board declares that a check reads and nothing states is an `unstated value` warning (ADR 0065). Write the value (`.r.resistance &= 4.7kohm +/- 5%`), bound what the part decides (a current or voltage check), or say that any value will do with a range (`&= 10kohm..100kohm`). A bound on a member that a part is held to by `&=` reaches the part. A `?=` default in a module is its statement.
- An `enum` is a type with a fixed list of variants (`enum Mode { Run, Sleep }`), and a variant is written with its enum, `Mode::Sleep`; a member takes it with `.m: Mode` and `&=`, `?=` or `==`. Who assembles a part is the built-in `.assembly` of every part, of the enum `AssemblyMode` (`Fab`, `Hand`, `None`, in scope in every file with no import): `.j1.assembly &= AssemblyMode::Hand`. Never write it as text (`"hand"`): that is an error that names the variant. A module offers the choice as `.assembly_mode: AssemblyMode` and ties its parts with `.j.assembly &= .assembly_mode`. The manifest and catalog files (not source) still write the word in lower case.
- Ranges are `1.8V..2.2V` and `3.3V +/- 5%`. Prefixes: `p n u m k M G`. `m` is milli and `M` is mega.
- `a ~ b` puts pins on one net, or joins two interfaces of one kind. `a ~> part ~> b` runs a chain through parts that have a `bridge`.
- Only a board names nets (`.pos.net &= "VCC"`). Modules never do.
- A module is set only through its rails, pins and declared values.
- A part's pins are its `pins "PACKAGE" { .pin kind "NAME" "pad" ... }` table, with `tie`, `fn`, `optional`; the board connects them and chooses a pin's function (`.u.pa2.function &= "UART1_TX"`). A function may carry its own direction after a colon with no spaces, `fn GPIO:bidirectional UART1_TX:output UART1_RX:input`; write it only where the datasheet says which way the function goes. A kind is one of `power`, `input`, `output`, `bidirectional`, `passive`, `no_connect`, `tri_state`; write `tri_state` (an output that is high impedance while its part is not selected, so several may share a net) only where the datasheet says so, with the page in a comment, and `output` for a pin it draws and is silent about. A pin has no `.pad`.
- A part states which functions belong together with `peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }` and which hardware peripherals share with `shared SERIAL0 { UARTE0 TWIM0 }` (`shared UARTS limit 2 { ... }` allows two; a member may also be a function or a pin `.p10`): at most one member is in use at a time, and the functions a board chooses are what is counted (ADR 0062). A peripheral `on APP_GPIO` is offered on every pin whose row lists `APP_GPIO` (a class is a function rows list; a core that owns a pin is a class), so the table keeps one row for each pin; a peripheral with no `on` needs each function listed by the rows of its dedicated pins, written without a kind. Both are only in a part, never in a generic, module, interface or board.
- Every part has `.designator`, the letters of its reference (`R1`, `BZ1`): a standard kind states them; a part you define states its own (`.designator &= "BZ"`) or is `U`; a board may narrow them on a part of its own that has none. Capital letters only.
- A digital line is a `Logic` (`.line` pin, `.reference` rail); a part states `.vih`, `.vil`, `.voh`, `.vol` and `.line.abs_min`/`.abs_max` from its datasheet. Join lines pin to pin (`.a.out.line ~ .b.in.line`) and each part's `.reference` to its own supply. Never invent a level: a missing one is "not evaluated".
- A board needs `.process: TwoLayer { .stackup.thickness &= 1.6mm }` to become a PCB.
- A board's circuit source holds `.min_track` on a `Rail` and `.process.top.net &= "GND"` for an outer pour. Physical intent is in the board's **physical file**, `<stem>.physical.nsm` beside the source (ADR 0048): `physical for Board { case { .outline: Outline { .width &= 60mm; .height &= 40mm } } .usb.place.left &= 1mm +/- 0.1mm }`. `case { }` holds the `Outline`, `Cutout`, `MountingHole`, `Fiducial` and `Label` (`std::pcb`, `std::parts::mechanical`), a `Keepout` may be anywhere in the unit, and a path may reach a part inside a module (`.radio.row_a.place.left`) only to state `.place`, `.height` or `.pin_protrusion` (how far the pins of a through-hole part protrude out of the far side, ADR 0045 section 15). A keepout forbids `.tracks`, `.vias`, `.copper` and `.parts` unless a field says `"allow"`. The same statements in the board's own source are an error that names the file. No solved coordinates anywhere: they are in `<stem>.pcb.toml`. From an enclosure's DXF: `netsmith dxf import case.dxf --layer EDGE --holes-layer HOLES` prints the members for `case` and `--write board.nsm` puts them, and the holes' places, in the physical file (ADR 0045).
- A row of numbered parts on a pitch is one statement in the physical file, `row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm` (every number between the two names must be a part of one module; the row statement is not valid in the circuit source). Numbers at the end of a path segment sort as numbers, so `.pads.tp2` is `TP2`.
- A `generic G { ... }` states what any provider must have (values, `pins { .a passive required }` with no names or pads, checks); a `part X provides G` or `module X provides G` stands for it and does not declare G's members again (narrow them). A board writes `.rf: G { .max_frequency >= 1.61GHz }` and the solver picks the first provider, in the files the board's file imports, for which the design holds; `.rf: G { .mpn &= "..." }` picks by part number, `.rf: X` forces X with no pick. A use reaches only what G declares (ADR 0059).
- `Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal`, `Fuse`, `Schottky`, `Zener`, `Tvs`, `NMosfet`, `PMosfet`, `LevelTranslator` and `LinearRegulator` are generics that are their own first provider, so `.r: Resistor { ... }` is read and picked as ever. To add a part that a use of one can pick, write `part X provides Resistor` with its own `pins` table (a row of the generic's kind for each of its pins), its own one-row `catalog`, and no `.package: Package` of its own (the generic declares it); import its file from the board's file. The standard rows and series are tried first. `PMosfet` and `LevelTranslator` have a standard catalog with no rows: a project adds rows in `p_mosfet.csv` or `level_translator.csv`, or writes a provider.
- A bus interface (`std::bus::Spi`, `Uart`, `I2c`, `Swd`, `Usb2`) has a role: a part states it (`.spi.role &= SpiRole::Slave`, import `SpiRole`), its pin table is what its datasheet says (a slave's data out is `tri_state` only if the datasheet says so, else `output`), and `~` joins two interfaces (a `Uart`'s `~` crosses `tx` to `rx`). A part's bus clock is a bound (`.spi.frequency <= 10MHz`), the board states the clock (`.cpu.spi.frequency &= 8MHz`). A bus interface also has `.levels` like a `Logic`: `.spi.levels.reference ~ .vdd`, `.spi.levels.vih &= 0.7 * .vdd.voltage` (vil, voh, vol), and an input's `.spi.sclk.abs_max`, each from the datasheet; `~` between two interfaces joins none of them.
- A resistor from a rail to one line (a reset, an enable, an open-drain output) is `std::bus::PullUp`, not a plain `Resistor` on the rail's `pos`: the module states what it draws, so the net is solved and the checks on it run. State `.pull.resistance &= 10kohm +/- 5%`, join `.pull.supply ~ .rail` and `.pull.line ~ .u.reset_n`; `.pull.line_low &= 0..0.1` narrows the share of time the line is low.
- A system joins ports of boards with `~` and nothing else.
- Statement order does not matter. Separate statements with a newline or `;`.
- Comments: `//`, `///` for the next declaration, `//!` for the file or definition.
- Use `&=` for constants and `==` for equations. Do not write `.x = 5V`: there is no `=`.

## Standard library

`std::parts::passive` (Resistor, Capacitor, Led, Inductor, FerriteBead, Crystal, Fuse, Switch), `std::parts::diode` (Diode, Schottky, Zener, Tvs), `std::parts::transistor` (NMosfet, PMosfet), `std::parts::logic` (LevelTranslator, Sn74lvc1g04), `std::parts::regulator::LinearRegulator`, `std::parts::connector` (headers, sockets, Coax, UsbCPower, UsbC2, Obd2M, ProgrammingPads6), `std::parts::mechanical` (MountingHole, PlatedHole, Fiducial, TestPoint, Label), `std::pcb` (Outline, Cutout, Keepout), `std::bus` (Uart, Spi, I2c, Swd, Usb2, PullUp, I2cPullups), `std::sources::Rail`, `std::logic::Logic`, `std::mfg::jlcpcb::{TwoLayer, FourLayer}`. Fields and pins of each are in the guide's catalogue.

## Catalogs

A part with `catalog X { .a, .b }` gets its real part from the rows of `X.csv`, first fitting row first. Your project adds files in `netsmith.toml` (`catalogs = ["catalogs/ldo.csv"]`, named after the catalog). A header of the field names without the dot plus `lcsc`, `mpn`, `library`, `price`, `stock`, `date`, `source`; cells like `3.3V +/- 2%`, text bare. A row that names a part needs a `source`. A keyed catalog (`by .mpn`) is chosen with `.mpn &= "..."`. See the guide's Catalog files.

## Capacitors

Say what you need after derating: `.effective_capacitance >= 4.7uF`. Use `.capacitance &= 100nF +/- 20%` only where you mean the marked part. A capacitor between a rail's `pos` and `neg` gets its bias from the rail.

When no single capacitor meets a requirement, a `Capacitor` takes `.count`, the number of identical capacitors in parallel (a bank): `.count &= 1..8` lets the solver choose the fewest, and a requirement on the whole bank multiplies, `.count * .effective_capacitance >= 47uF`, while a rating (`.max_voltage`) is per capacitor. `BuckStage` does this for each side: read or state `.stage.output_count` and `.stage.input_count` (`.stage.output_count &= 2`). Never write capacitors side by side to get a larger value.

## Loop

Write a little, run `netsmith build`, read the first error, fix it. See `netsmith-read-errors`.
