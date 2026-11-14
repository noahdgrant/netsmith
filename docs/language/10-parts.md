## Parts

A part has pins, quantities, relations between them, a bridge for chains, and a catalog.

```nsm
/// A resistor with a through-current and voltage drop tied together by Ohm's law.
part SimpleResistor {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }

    .resistance: Resistance
    /// Through-current, a to b.
    #[role(current)]
    .current: Current
    /// Voltage at a relative to b.
    .drop: Voltage

    .drop == .current * .resistance
    bridge .a ~> .b

    .package: Package
    catalog resistor { .resistance, .package }
}
```

Rules.

- A pin of a part is a row of its `pins` table (next section), which also gives its pads: the identifiers that its symbol and footprint share, such as `"1"`, `"A1"` or `"EP"`. A pin has no `.pad` field. Its `.net` is the net name, set by a board, and its `.function` is what a board chooses for a pin that has several.
- A pin declared as `.name: Pin` has no row, so it has no pad. It is for a pin that the part joins to one that has pads (an alias, see below), and in a module or interface, where a pin passes a connection along.
- A part states only what is always true of it. It does not fix a value a design might change, and it names no KiCad symbol or footprint (ADR 0002).
- `bridge .a ~> .b` names the two terminals a chain enters and leaves by. A part has at most one.
- `catalog name { .field, ... }` lists the fields the solver fills from a catalog (next section). A column may end with `optional` (`catalog tvs { .standoff_voltage, .breakdown_voltage optional }`, ADR 0078): a file may leave it out and a row may leave its cell empty ("An optional column" in the next section). The standard catalogs are `resistor`, `capacitor`, `led`, `ldo`, `schottky`, `zener`, `tvs`, `ferrite_bead`, `inductor`, `fuse`, `n_mosfet`, `crystal` and `header` (the contact rating of a pin header or socket, ADR 0040). A part naming another is reported as "not chosen: there is no `x` catalog to choose from".
- A part with no catalog, such as `Header2x5`, is placed as written. A through-hole part is soldered by hand unless `.assembly` says otherwise (next section).

Misuse.

```nsm fails
part Bad {
    .a: Pin
    .b: Pin
    bridge .a ~> .a
}
```

```text
error: a bridge needs two different terminals
  --> main.nsm:4:5
  |
4 |     bridge .a ~> .a
  |     ^^^^^^^^^^^^^^^
```

### Who assembles a part

Every part has a field `.assembly`, of the type `AssemblyMode`, that it does not declare (declaring it again is an error). `AssemblyMode` is an enum (see Enums, below) that every file knows without an import. It says who puts the part on the board, and `fab` honours it (ADR 0024, provisional; the type is ADR 0054):

| Value | Meaning |
|---|---|
| `AssemblyMode::Fab` | The board house places and solders it: a BOM line and a placement row. |
| `AssemblyMode::Hand` | Soldered by hand: not in the BOM or the placement file, and listed in `fab-notes.txt` as `hand-solder:`. |
| `AssemblyMode::None` | Not fitted by anyone, such as a hole or a fiducial: in neither file, and listed in `fab-notes.txt` as `not fitted`. |
| `AssemblyMode::Consigned` | The board house places and solders it, and you ship the part to the board house: a BOM line and a placement row, no supplier part number needed, and listed in `fab-notes.txt` as `consigned:` (ADR 0084). See Consigned parts, below. |

Where the value comes from, weakest first (ADR 0028 amends ADR 0024):

| | Says |
|---|---|
| 1 | The built-in rule: a through-hole part is `Hand` and every other part is `Fab`. |
| 2 | The project's default, `assembly = "fab"`, `"hand"`, `"none"` or `"consigned"` under `[project]` in `netsmith.toml`. It is for every part of every board that nothing below speaks for. |
| 3 | The part kind's own statement: `MountingHole`, `PlatedHole`, `Fiducial`, `TestPoint` and `ProgrammingPads6` are `None`, and neither the project nor a board can change that. |
| 4 | The catalog file's optional `assembly` column: the picked row's value. |
| 5 | The board narrowing it on a part it owns: `.assembly &= AssemblyMode::Hand`; or narrowing the value that a module offers for its parts (below). |

The manifest and the catalog files are not source text, so they write the variant as a lower-case word (`"fab"`, `"hand"`, `"none"`, `"consigned"`); in `.nsm` it is always the variant.

```toml
[project]
name = "blink"
assembly = "fab"
```

With that manifest the header of the example below is placed by the board house too, and a board that wants one part by hand still says so on the part. Rows 3 and 5 are one field, so a board that contradicts a kind gets the usual empty-intersection error. There is no per-board default: the project has one, and a part says otherwise on itself. A word that is not one of the four variants is refused where it is written (see Enums, below). A part marked `Hand` or `None` still counts in the solver and in the current balance.

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

board Assembly {
    /// Let the board house solder this header too.
    .j1: Header1x2 {
        .assembly &= AssemblyMode::Fab
    }
    /// This resistor goes on by hand.
    .r1: Resistor {
        .assembly &= AssemblyMode::Hand
    }
}
```

A part that the board house is to assemble from its stock (`AssemblyMode::Fab`; a consigned part is the exception) must have a supplier part number, an `lcsc` in its catalog row. `fab` checks that before it places or routes anything, and refuses with all the parts that have none in one message: each reference designator, its path, what its row says (the manufacturer's number), and what to write and where. There is no flag for it: the design says who does not assemble the part, with `.assembly &= AssemblyMode::Hand` (or `AssemblyMode::None`, or `AssemblyMode::Consigned` if you ship the part to the board house) on the part in the board, in the module's definition for a part inside a module, or in the `assembly` column of the part's own catalog row. The part is then out of `bom.csv` and `cpl.csv`, and `fab-notes.txt` lists it under `hand-solder:` and, if its catalog row has a manufacturer's number and no `lcsc`, in a line saying the board house does not assemble it and that you buy it yourself. `--allow-drc-violations` does not waive this.

**Consigned parts.** A part that JLCPCB can place but does not stock is bought by you and shipped to JLCPCB (its consigned parts service). Say so with `AssemblyMode::Consigned`, the way you say `Hand`: on the part, through a module's `.assembly_mode`, in the `assembly` column of a catalog row (`consigned`), or as the project's default. The part needs no supplier part number, its row's stock does not matter, and a consigned generic part is not looked up in a catalog:

```nsm
import std::parts::passive::Resistor

board Shipped {
    /// A resistor that you buy yourself and ship to the board house.
    .r1: Resistor {
        .resistance &= 330ohm +/- 1%
        .assembly &= AssemblyMode::Consigned
    }
}
```

`build` prints it as `consigned` in the bill-of-materials column. `fab` puts it in `cpl.csv`, so the board house places it, and in `bom.csv`, with the manufacturer's number as the comment and the LCSC cell empty (or the row's number, if it has one). JLCPCB's BOM format has no column that marks a consigned part: you register the parts on its site, ship them, and pick them in the order page's component matching, so `fab-notes.txt` lists them under `consigned:` with their manufacturer's number and quantity, and says that you ship them with the order. Look at the matching step of the order preview before you pay.

A board may narrow a part inside a module (ADR 0116), but a module that wants the board to make one choice of who assembles all its parts offers the choice as a value of its own and ties its parts to it (ADR 0042). The value earns its place by tying several parts to one choice; a module with one part offers none, since the board narrows `.part.assembly` itself, and std has no module that forwards a value inward (issue #670). Declare a value of the type `AssemblyMode`, and narrow the part's `.assembly` by it inside the module:

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with its resistor. The board chooses who solders them.
module Indicator {
    .supply: Rail
    .assembly_mode: AssemblyMode
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor
    .led.assembly &= .assembly_mode
    .r1.assembly &= .assembly_mode
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Lamps {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .header: Header1x2
    .input.pos ~ .header.p1
    .input.neg ~ .header.p2
    /// Soldered by the board house, like any part.
    .fitted: Indicator
    /// Soldered by hand: the board narrows what the module offers.
    .spare: Indicator {
        .assembly_mode &= AssemblyMode::Hand
    }
    .input ~ .fitted.supply
    .input ~ .spare.supply
}
```

- The tie is a relation between two `AssemblyMode` values inside the module, `.led.assembly &= .assembly_mode`. It counts as the design's own `.assembly &=` on that part (row 5 of the table above): the board's narrowing of the module's value is the last word, and the project's default, the catalog row and the built-in rule only speak when it is not set.
- A board that leaves `.assembly_mode` alone has said nothing: the tie narrows nothing, no note is printed for it, and the parts are assembled as if they stood on the board. Two instances of the module are separate, and each has its own value.
- A variant that `AssemblyMode` does not have is refused where it is written, listing the variants (see Enums, below). A kind that states `AssemblyMode::None` (`Fiducial`, `MountingHole`) cannot be tied to a different variant: the usual empty-intersection error names the part, as ``contradiction on `mark.fiducial.assembly`: it is already AssemblyMode::None, which does not overlap AssemblyMode::Fab``.
- A module may pass the value on to a module inside it, `.inner.assembly_mode &= .assembly_mode`. Call the value `.assembly_mode` in every module that offers it, so that boards read alike.
- The board may also narrow one part inside the module itself, `.spare.led.assembly &= AssemblyMode::Hand` (ADR 0116), but only with `&=`: a default from outside (`?=`) does not reach inside a module.

```nsm fails
import std::parts::connector::Header1x2

module Block {
    .j: Header1x2
}

board Reach {
    .block: Block
    .block.j.assembly ?= AssemblyMode::Hand
}
```

```text
error: `j` is inside `Block` and is not part of its interface: only narrowing (`&=`) reaches inside a module; narrow a value inside it with `&=`, or use a rail, a pin, or a value that `Block` declares
  --> main.nsm:9:12
  |
9 |     .block.j.assembly ?= AssemblyMode::Hand
  |            ^
```

### Designators

Every part has a text field `.designator` that it does not declare (declaring it again is an error), as it has `.assembly`. It holds the letters of the part's reference designator on the schematic, the bill of materials and the placement file, and the letters come from the part's kind (below). The number after them counts the parts that have the same letters, from 1, in the order of their paths (owner decision, 2026-11-13, issue #634; it replaces the order of declaration of 2026-11-09): each part's full path in the board (`power_in`, `status.led`, `buck.divider.rtop`), a module's parts with the board's own, compared segment by segment, with a number at the end of a segment compared as a number (`tp2` before `tp10`) and the index of a group's member too (`decoupling[2]` before `decoupling[10]`). So a board with the headers `.power_in` and `.enable_in` has `enable_in` as `J1` and `power_in` as `J2`, whichever it declares first, and a resistor `.buck.rtop` comes after a resistor `.audio.rbias` and before `.status.r`. The path decides it and nothing else: not the order of the statements, the order of the sheets, how the schematic is drawn, the placement, or anything KiCad does, so the netlist, the bill of materials and the board are the same with every layout and with the statements in any order. Adding, removing or renaming a part may renumber the parts that sort after it with the same letters (a layout file is keyed by path, so no placement moves). Each board is numbered on its own, from 1, the boards of a system too (the paths are the board's own, without the system's name for it); a panel's cells keep the board's numbers and add the cell (`R1_3`). How the numbering spans modules and the boards of a system was decided by the orchestrating agent while the owner was away (2026-11-13). A number at the end of a name (`.tp2`) is part of the path and counts only as the path sorts: `.tp2` is numbered before `.tp10`, but it is `TP1` if it is the first test point. The letters follow IEEE 315 and ASME Y14.44 usage: `K` a relay, `BZ` a buzzer, `BT` a battery, `T` a transformer, `U` an integrated circuit.

Each standard kind states its own letters, because they are always true of it, and nothing can change them: `Resistor` is `"R"`, `Capacitor` `"C"`, `Inductor` `"L"`, `FerriteBead` `"FB"`, `Led`, `Diode`, `Schottky`, `Zener` and `Tvs` `"D"`, `NMosfet` and `PMosfet` `"Q"`, `Crystal` `"Y"`, `Fuse` `"F"`, `Switch` `"SW"`, every connector (a part that provides `Connector`, see _A connector of your own_: every header, socket, `Coax`, `UsbCPower`, `UsbC2`, `UsbCReceptacle16P`, `Obd2M` and `ProgrammingPads6`) `"J"`, `TestPoint` `"TP"`, `MountingHole` and `PlatedHole` `"H"`, `Fiducial` `"FID"`, `Label` `"LBL"` and `LinearRegulator` `"U"`. A part you define states its own letters in its body, `.designator &= "BZ"`, or leaves them out; a part with none is `"U"`. A board may give the letters to a part of its own that states none, with the same `&=`. A test fails when a part of the standard library has no letters, so a new kind gets them on purpose.

```nsm
part Buzzer {
    .designator &= "BZ"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

part Chip {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Letters {
    /// `BZ1`: its kind says so.
    .alarm: Buzzer
    /// `U1`: a part that says nothing is an integrated circuit.
    .mcu: Chip
    /// `IC1`: the board says so, because the part does not.
    .codec: Chip {
        .designator &= "IC"
    }
}
```

Each part is numbered among the parts with its letters, in the order of their paths. Here `clamp` sorts before `rectifier`, so the TVS is `D1` and the diode `D2`, though the diode is declared first; the fuse is `F1` and the header, a second kind with its own count, `J1`:

```nsm
import std::parts::connector::Header1x2
import std::parts::diode::{Diode, Tvs}
import std::parts::passive::Fuse

board Order {
    /// `D2`: `rectifier` sorts after `clamp`.
    .rectifier: Diode
    /// `J1`, the one part with the letters `J`.
    .zz_in: Header1x2
    /// `D1`.
    .clamp: Tvs
    /// `F1`.
    .fuse: Fuse
}
```

Letters that are not one or more capital letters are refused when the board is built, naming the part, because the number is added after them:

```nsm fails
part Chip {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Letters {
    .mcu: Chip {
        .designator &= "U1"
    }
}
```

```text
error: Letters: `mcu` has `.designator` "U1", which is not one or more capital letters such as "R" or "IC"
```

A kind's letters are one value, so a board that contradicts them gets the usual empty-intersection error at the part: ``contradiction on `r.designator`: it is already "R", which does not overlap "Z"``. A saved placement is keyed by the part's path, not by its designator (ADR 0032), so changing letters or numbers moves no part in the layout file. A fab package that was ordered before the letters changed keeps the designators it was made with.
