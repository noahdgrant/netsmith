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
- Ranges are `1.8V..2.2V` and `3.3V +/- 5%`. Prefixes: `p n u m k M G`. `m` is milli and `M` is mega.
- `a ~ b` puts pins on one net, or joins two interfaces of one kind. `a ~> part ~> b` runs a chain through parts that have a `bridge`.
- Only a board names nets (`.pos.net &= "VCC"`). Modules never do.
- A module is set only through its rails, pins and declared values.
- A part's pins are its `pins "PACKAGE" { .pin kind "NAME" "pad" ... }` table, with `tie`, `fn`, `optional`; the board connects them and chooses a pin's function (`.u.pa2.function &= "UART1_TX"`). A function may carry its own direction after a colon with no spaces, `fn GPIO:bidirectional UART1_TX:output UART1_RX:input`; write it only where the datasheet says which way the function goes. A pin has no `.pad`.
- Every part has `.designator`, the letters of its reference (`R1`, `BZ1`): a standard kind states them; a part you define states its own (`.designator &= "BZ"`) or is `U`; a board may narrow them on a part of its own that has none. Capital letters only.
- A digital line is a `Logic` (`.line` pin, `.reference` rail); a part states `.vih`, `.vil`, `.voh`, `.vol` and `.line.abs_min`/`.abs_max` from its datasheet. Join lines pin to pin (`.a.out.line ~ .b.in.line`) and each part's `.reference` to its own supply. Never invent a level: a missing one is "not evaluated".
- A board needs `.process: TwoLayer { .stackup.thickness &= 1.6mm }` to become a PCB.
- A board's shape is members and fields, not new syntax: `.outline: Outline { .width &= 60mm .height &= 40mm .corner_radius &= 2mm }`, `Cutout`, `Keepout` (`std::pcb`), `.min_track` on a `Rail`, `.process.top.net &= "GND"` for an outer pour, `Label` for silkscreen text. A keepout forbids `.tracks`, `.vias`, `.copper` and `.parts` unless a field says `"allow"`. No coordinates of parts in the source: they are in `<stem>.pcb.toml`. From an enclosure's DXF: `netsmith dxf import case.dxf --layer EDGE --holes-layer HOLES` prints the `Outline`, `Cutout` and `MountingHole` members to paste and `--write board.nsm` merges their places into the layout file (ADR 0045).
- A system joins ports of boards with `~` and nothing else.
- Statement order does not matter. Separate statements with a newline or `;`.
- Comments: `//`, `///` for the next declaration, `//!` for the file or definition.
- Use `&=` for constants and `==` for equations. Do not write `.x = 5V`: there is no `=`.

## Standard library

`std::parts::passive` (Resistor, Capacitor, Led, Inductor, FerriteBead, Crystal, Fuse, Switch), `std::parts::diode` (Diode, Schottky, Zener, Tvs), `std::parts::transistor` (NMosfet, PMosfet), `std::parts::regulator::LinearRegulator`, `std::parts::connector` (headers, sockets, Coax, UsbCPower, UsbC2, ProgrammingPads6), `std::parts::mechanical` (MountingHole, PlatedHole, Fiducial, TestPoint, Label), `std::pcb` (Outline, Cutout, Keepout), `std::bus` (Uart, Spi, I2c, Swd, Usb2, I2cPullups), `std::sources::Rail`, `std::logic::Logic`, `std::mfg::jlcpcb::{TwoLayer, FourLayer}`. Fields and pins of each are in the guide's catalogue.

## Catalogs

A part with `catalog X { .a, .b }` gets its real part from the rows of `X.csv`, first fitting row first. Your project adds files in `netsmith.toml` (`catalogs = ["catalogs/ldo.csv"]`, named after the catalog). A header of the field names without the dot plus `lcsc`, `mpn`, `library`, `price`, `stock`, `date`, `source`; cells like `3.3V +/- 2%`, text bare. A row that names a part needs a `source`. A keyed catalog (`by .mpn`) is chosen with `.mpn &= "..."`. See the guide's Catalog files.

## Capacitors

Say what you need after derating: `.effective_capacitance >= 4.7uF`. Use `.capacitance &= 100nF +/- 20%` only where you mean the marked part. A capacitor between a rail's `pos` and `neg` gets its bias from the rail.

## Loop

Write a little, run `netsmith build`, read the first error, fix it. See `netsmith-read-errors`.
