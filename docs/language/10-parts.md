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
- A part with no catalog, such as `ProgrammingPads6`, is placed as written. A through-hole part is soldered by hand unless `.assembly` says otherwise (next section).

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
| 4a | A line of the board's `defaults` block, `Resistor.assembly ?= AssemblyMode::Hand` (issue #847, below): an offer for every part of the kind on the board, or inside its modules, that no statement on the part's own `.assembly` speaks for. It beats rows 1, 2 and 4, and gives way to any `&=` or `?=` on the part, so to rows 3 and 5. |
| 5 | The board narrowing it on a part it owns: `.assembly &= AssemblyMode::Hand`; or narrowing the value that a module offers for its parts (below). |

The manifest and the catalog files are not source text, so they write the variant as a lower-case word (`"fab"`, `"hand"`, `"none"`, `"consigned"`); in `.nsm` it is always the variant.

```toml
[project]
name = "blink"
assembly = "fab"
```

With that manifest the header of the example below is placed by the board house too, and a board that wants one part by hand still says so on the part. Rows 3 and 5 are one field, so a board that contradicts a kind gets the usual empty-intersection error. The project has one default for every board; a board that wants its own for a kind of part says it in its `defaults` block (below), and a part says otherwise on itself. A word that is not one of the four variants is refused where it is written (see Enums, below). A part marked `Hand` or `None` still counts in the solver and in the current balance.

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

board Assembly {
    /// Let the board house solder this header too.
    .j1: Header1x2 { .assembly &= AssemblyMode::Fab }
    /// This resistor goes on by hand.
    .r1: Resistor { .assembly &= AssemblyMode::Hand }
}
```

**One choice for every part of a kind.** A board that solders all its resistors by hand, or ships all its connectors to the board house, says so once in its `defaults` block (ADR 0128), as it does for a package (see _Defaults for every part of a kind_ in chapter 7). `.assembly` is a built-in member of every part and a value of the type `AssemblyMode`, and a line may offer it to a part or a generic:

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::{Led, Resistor}

board DefaultAssembly {
    defaults {
        Resistor.assembly ?= AssemblyMode::Hand
    }
    /// By hand, from the line.
    .r1: Resistor
    /// The board house, because this part says so itself.
    .r2: Resistor { .assembly &= AssemblyMode::Fab }
    /// The line is for resistors and not for LEDs.
    .d1: Led
    .j1: Header1x2
}
```

The line is an offer (ADR 0051): a `&=` or a `?=` on a part's `.assembly` replaces it for that part, as does anything a kind states of itself (`MountingHole`, `Fiducial`, `TestPoint` and the others of row 3 are fitted by nobody whatever a line says; a line that reaches only such parts is a note that it reaches nothing). It is a statement of the design, so, like a `?=` on the part, it wins over the project's default, the catalog row and the built-in rule (a through-hole header is `Hand` unless a line or a statement says otherwise). A module has no `.assembly` of its own: `Lamp.assembly ?= ...` is refused, and a module that offers `.assembly_mode` (below) takes `Lamp.assembly_mode ?= AssemblyMode::Hand`. Of the other built-in members only `.assembly` is a value a line may offer: `.mpn` and `.lcsc` name one catalog row, so a board states them on the part.

A part that the board house is to assemble from its stock (`AssemblyMode::Fab`; a consigned part is the exception) must have a supplier part number, an `lcsc` in its catalog row. `fab` checks that before it places or routes anything, and refuses with all the parts that have none in one message: each reference designator, its path, what its row says (the manufacturer's number), and what to write and where. There is no flag for it: the design says who does not assemble the part, with `.assembly &= AssemblyMode::Hand` (or `AssemblyMode::None`, or `AssemblyMode::Consigned` if you ship the part to the board house) on the part in the board, in the module's definition for a part inside a module, or in the `assembly` column of the part's own catalog row. The part is then out of `bom.csv` and `cpl.csv`, and `fab-notes.txt` lists it under `hand-solder:` and, if its catalog row has a manufacturer's number and no `lcsc`, in a line saying the board house does not assemble it and that you buy it yourself. `--allow-drc-violations` does not waive this.

**Consigned parts.** A part that JLCPCB can place but does not stock is bought by you and shipped to JLCPCB (its consigned parts service). Say so with `AssemblyMode::Consigned`, the way you say `Hand`: on the part, through a module's `.assembly_mode`, in the `assembly` column of a catalog row (`consigned`), or as the project's default. The part needs no supplier part number, its row's stock does not matter, and a consigned generic part is not looked up in a catalog:

```nsm
import std::parts::passive::Resistor

board Shipped {
    /// A resistor that you buy yourself and ship to the board house.
    .r1: Resistor { .resistance &= 330ohm +/- 1%; .assembly &= AssemblyMode::Consigned }
}
```

`build` prints it as `consigned` in the bill-of-materials column. `fab` puts it in `cpl.csv`, so the board house places it, and in `bom.csv`, with the manufacturer's number as the comment and the LCSC cell empty (or the row's number, if it has one). JLCPCB's BOM format has no column that marks a consigned part: you register the parts on its site, ship them, and pick them in the order page's component matching, so `fab-notes.txt` lists them under `consigned:` with their manufacturer's number and quantity, and says that you ship them with the order. Look at the matching step of the order preview before you pay.

A board may narrow a part inside a module (ADR 0116), but a module that wants the board to make one choice of who assembles all its parts offers the choice as a value of its own and ties its parts to it (ADR 0042). The value earns its place by tying several parts to one choice; a module with one part offers none, since the board narrows `.part.assembly` itself, and std has no module that forwards a value inward (issue #670). Declare a value of the type `AssemblyMode`, and narrow the part's `.assembly` by it inside the module:

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Led, Resistor}
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
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .header: Header1x2
    .input.pos ~ .header.p1
    .input.neg ~ .header.p2
    /// Soldered by the board house, like any part.
    .fitted: Indicator
    /// Soldered by hand: the board narrows what the module offers.
    .spare: Indicator { .assembly_mode &= AssemblyMode::Hand }
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

### A part that is not fitted until it is tuned

Some positions of a board are tuned on the bench: a shunt of an RF matching network has its footprint on the board, and its value is chosen with the board in hand (issue #1273). `Resistor`, `Capacitor` and `Inductor` say so with two members:

- `.fitted`, a `Bool` that is `true` unless the design states `.fitted &= false`. `Bool` is in the prelude, and its values are written `true` and `false`, in every file, with no import and no `Bool::` (`Bool::false` is refused).
- `.tune`, the range the value may be tuned in, of the value's own quantity: `.tune &= 1pF..10pF` on a capacitor, `.tune &= 1nH..10nH` on an inductor. It is optional: a part may be not fitted with no range.

```nsm solve
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board Match {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .feed: Resistor { .resistance &= 1kohm +/- 1% }
    .load: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .feed ~> .load ~> .supply.neg
    /// Chosen on the bench.
    .shunt: Capacitor { .fitted &= false; .tune &= 1pF..10pF; .package &= "0402" }
    .shunt.a ~ .feed.b
    .shunt.b ~ .supply.neg
    /// Starts at 100 nF, tuned from 10 nF to 1 uF.
    .trim: Capacitor { .fitted &= false; .tune &= 10nF..1uF; .capacitance &= 100nF +/- 10% }
    .trim.a ~ .feed.b
    .trim.b ~ .supply.neg
}
```

What a part that is not fitted is:

- **Marked.** The schematic writes `DNP` before its value (`DNP 1pF..10pF`), and KiCad's schematic draws its symbol crossed out (`dnp`). The board has its footprint, marked `dnp` in the KiCad board.
- **Assembled by nobody.** It is `AssemblyMode::None` whatever else says who assembles it, so it is on no placement file. Unlike a hole or a fiducial, `bom.csv` lists it: a line whose comment is `DNP`, the value it starts from if one is stated, and its range (`DNP (tune 1pF..10pF)`, `DNP 100n (tune 10nF..1uF)`), with the LCSC cell empty, so the board house buys nothing for it. `fab-notes.txt` lists the lines under `DNP`, and `board bom` and the editor's bill list them under _Not fitted_.
- **Given no catalog row unless its value is stated.** A part that is not fitted and states no `.resistance`, `.capacitance` or `.inductance` is not picked: its value is the range of `.tune`, and every relation that reads it is judged over the whole range, as any range is. A part that states its value (`.trim` above) is picked as any part is, and the row is the value to start tuning from; `bom.csv` still lists it as `DNP`, with no number. "Stated" is what _A value that may stay open_ in chapter 7 says, on the value or on a member `~` joins to it.
- **Checked for its range.** A stated value must lie in `.tune` (`Resistor`, `Capacitor` and `Inductor` state `.capacitance >= min(.tune)` and `.capacitance <= max(.tune)`, with their own value). A fitted part may state a range too: it is then picked inside it.

A part that is not fitted, states no value and has no row has no package to take a footprint from, so it states one:

```nsm fails
import std::parts::passive::Capacitor

board Shunt {
    .shunt: Capacitor { .fitted &= false; .tune &= 1pF..10pF }
}
```

```text
error: `shunt` is not fitted and states no `.capacitance`, so no catalog row is picked for it and nothing gives its footprint: state its package (`.shunt.package &= "0402"`), or the value to start tuning from (`.shunt.capacitance &= ...`), which is then picked
  --> main.nsm:4:25
  |
4 |     .shunt: Capacitor { .fitted &= false; .tune &= 1pF..10pF }
  |                         ^^^^^^^^^^^^^^^^
```

A value outside the range is refused with both:

```nsm fails
import std::parts::passive::Capacitor

board Shunt {
    .shunt: Capacitor { .fitted &= false; .tune &= 10nF..1uF; .capacitance &= 4.7uF +/- 10% }
}
```

```text
error: no `capacitor` candidate satisfies `shunt`: the closest of the 1 that fit, 4.7uF 10% 16V X5R 0603 C19666, fails because shunt.capacitance must be at most max(shunt.tune), but 4.23uF..5.17uF is above 1uF: no value of the two ranges meets it, so change one of them
    --> std/parts/passive.nsm:165:5
    |
165 |     .capacitance <= max(.tune)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
error: shunt.capacitance must be at most max(shunt.tune), but 4.23uF..5.17uF is above 1uF: no value of the two ranges meets it, so change one of them
    --> std/parts/passive.nsm:165:5
    |
165 |     .capacitance <= max(.tune)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Rules.

- What is read is the value `.fitted` has once the checker is done, which a `&=` or a `?=` of `true` or `false` gives it. A module that ties its parts' `.fitted` to a value of its own (`.shunt.fitted &= .tuned`) has the parts picked as if they were fitted, and marked DNP all the same.
- A not-fitted part counts in the solver and in the current balance, as a part marked `AssemblyMode::None` does.
- The design stage's `Design` lists the parts in `features.not_fitted`, each with its designator and its range.

### Designators

Every part has a text field `.designator` that it does not declare (declaring it again is an error), as it has `.assembly`. It holds the letters of the part's reference designator on the schematic, the bill of materials and the placement file, and the letters come from the part's kind (below). The number after them counts the parts that have the same letters, from 1, in the order of their paths (owner decision, 2026-11-13, issue #634; it replaces the order of declaration of 2026-11-09): each part's full path in the board (`power_in`, `status.led`, `buck.divider.rtop`), a module's parts with the board's own, compared segment by segment, with a number at the end of a segment compared as a number (`tp2` before `tp10`) and the index of a group's member too (`decoupling[2]` before `decoupling[10]`). So a board with the headers `.power_in` and `.enable_in` has `enable_in` as `J1` and `power_in` as `J2`, whichever it declares first, and a resistor `.buck.rtop` comes after a resistor `.audio.rbias` and before `.status.r`. The path decides it and nothing else: not the order of the statements, the order of the sheets, how the schematic is drawn, the placement, or anything KiCad does, so the netlist, the bill of materials and the board are the same with every layout and with the statements in any order. Adding, removing or renaming a part may renumber the parts that sort after it with the same letters (a layout file is keyed by path, so no placement moves). The placer and the router never read the numbers either: each net lists its pins in the order of their parts' paths, and the placer's corners take holes and fiducials in path order, so a part that is renumbered or given other letters leaves the placement and the copper as they were (owner decision, issue #1048). Each board is numbered on its own, from 1, the boards of a system too (the paths are the board's own, without the system's name for it); a panel's cells keep the board's numbers and add the cell (`R1_3`). How the numbering spans modules and the boards of a system was decided by the orchestrating agent while the owner was away (2026-11-13). A number at the end of a name (`.tp2`) is part of the path and counts only as the path sorts: `.tp2` is numbered before `.tp10`, but it is `TP1` if it is the first test point. The letters follow IEEE 315 and ASME Y14.44 usage: `K` a relay, `BZ` a buzzer, `BT` a battery, `T` a transformer, `U` an integrated circuit.

Each standard kind states its own letters, because they are always true of it, and nothing can change them: `Resistor` is `"R"`, `Capacitor` `"C"`, `Inductor` `"L"`, `FerriteBead` `"FB"`, `Led`, `Diode`, `Schottky`, `Zener` and `Tvs` `"D"`, `NMosfet` and `PMosfet` `"Q"`, `Crystal` `"Y"`, `Fuse` `"F"`, `Switch` `"SW"`, every connector (a part that provides `Connector`, see _A connector of your own_: every header, socket, `Coax`, `UsbCPower`, `UsbC2`, `UsbCReceptacle16P`, `Obd2M` and `ProgrammingPads6`) `"J"`, `TestPoint` `"TP"`, `MountingHole` and `PlatedHole` `"H"`, `Fiducial` `"FID"`, `Label` `"LBL"`, `Logo` `"LOGO"` and `LinearRegulator` `"U"`. A part you define states its own letters in its body, `.designator &= "BZ"`, or leaves them out; a part with none is `"U"`. A board may give the letters to a part of its own that states none, with the same `&=`. A test fails when a part of the standard library has no letters, so a new kind gets them on purpose.

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
    .codec: Chip { .designator &= "IC" }
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
    .mcu: Chip { .designator &= "U1" }
}
```

```text
error: Letters: `mcu` has `.designator` "U1", which is not one or more capital letters such as "R" or "IC"
```

A kind's letters are one value, so a board that contradicts them gets the usual empty-intersection error at the part: ``contradiction on `r.designator`: it is already "R", which does not overlap "Z"``. A saved placement is keyed by the part's path, not by its designator (ADR 0032), so changing letters or numbers moves no part in the layout file. A fab package that was ordered before the letters changed keeps the designators it was made with.

### Order codes and their features

A part that is sold as several order codes with different features, such as a radio module with and without a GNSS receiver, lists them in a `.variants` table: an order code in quotes to a row, and after it the names of the features that code has (issue #1311, owner decision). A board, or a module, states what it needs of the part with `.needs &=` and the name of a feature, one statement for each, and the part's `.mpn` is then the one order code that has every need. The bill of materials names that code.

```nsm
import std::parts::connector::Header1x2
import std::sources::Rail

/// A radio module sold with and without a GNSS receiver.
part Modem {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
    }
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    /// The board tracks its position: its `.mpn` is `MDM-100-G`.
    .u: Modem { .assembly &= AssemblyMode::Hand }
    .u.needs &= GNSS
    .input ~ .u.vcc
}
```

- A feature is a plain name that the table writes after a code. It is the part's own, so `.needs` takes it bare, with no import, as a pin's `.function` takes a name of its pin table. A name the table does not write is refused, with the features listed.
- Each `.needs &= F` leaves the codes that have `F`, so two statements need both features. A module that places the part may state a need of its own, and the board one from outside (`.tracker.u.needs &= GNSS`, ADR 0116: only `&=` reaches inside a module).
- The design must leave one code. With no need stated and several codes, or with needs that several codes meet, the board names the code with `.mpn &=`, and the message lists the codes with their features. A part with one code needs neither.
- A code the board names with `.mpn &=` must be a row of the table and have every feature the design needs: a code without one is refused, at the `.mpn`, with the need and the codes that have it.
- The `.needs` member is a part's only when the part states `.variants`; a part that states the table may not declare a `.needs` of its own. A part has one table, and a code is in it once.
- The code is the part's `.mpn`, so a part that also has a catalog keyed by `.mpn` (`catalog modem by .mpn { .mpn, .package }`, chapter 12) is held to the row of that code, as a hand pick holds it, and takes its supplier's number from the row. A part with no catalog has no supplier's number: the bill of materials shows the code, and the board house does not assemble it unless it is `Hand` or `Consigned` (Who assembles a part, above).

A code that lacks a feature the board needs is refused:

```nsm fails
part Modem {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
    }
    pins {
        .a passive "A" "1"
    }
}

board Tracker {
    .u: Modem { .mpn &= "MDM-100" }
    .u.needs &= GNSS
}
```

```text
error: `u.mpn` is `MDM-100`, which has no `GNSS`, and the design needs `GNSS` (main.nsm:13); the order codes that have them are `MDM-100-G`
   --> main.nsm:12:17
   |
12 |     .u: Modem { .mpn &= "MDM-100" }
   |                 ^^^^^^^^^^^^^^^^^
```

A board that leaves two codes is asked which:

```nsm fails
part Modem {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
    }
    pins {
        .a passive "A" "1"
    }
}

board Tracker {
    .u: Modem
}
```

```text
error: `u` is a `Modem`, which is sold as `MDM-100-G` (GNSS, LTE) and `MDM-100` (LTE); state what the design needs of it (`.u.needs &= GNSS`, one statement for each feature) or name the order code (`.u.mpn &= "MDM-100-G"`)
   --> main.nsm:12:5
   |
12 |     .u: Modem
   |     ^^^^^^^^^
```

### Alternates on one footprint

One footprint can take two parts: two radio modules with the same pads, a regulator sold by two makers. The board declares the part it is drawn with, and beside it, with `alternate`, each other part the footprint may take (issue #1312, owner decision). The build checks, solves and designs the board again with each alternate fitted in the member's place, and says which one fails. The bill of materials lists each alternate on the member's designator, not fitted (DNP), and `fab-notes.txt` names it; `bom.csv` and the placement file have the member only.

```nsm
import std::parts::connector::Header1x2
import std::sources::Rail

part ModuleA {
    .vcc: Rail
    .vcc.voltage <= 3.6V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

/// Another maker's module on the same pads, for 1.8 V to 5.5 V.
part ModuleB {
    .vcc: Rail
    .vcc.voltage <= 5.5V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA { .assembly &= AssemblyMode::Hand }
    /// Fitted when the first is out of stock.
    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }
    .input ~ .u.vcc
}
```

- `alternate` is followed by a declaration of the member as it would be written with the alternate in its place, and its body narrows the alternate as that declaration's would (`.assembly`, `.lcsc`, `.needs`). The member is one part declared in the same body: not a module, a group or a list. The alternate is another kind of part; a different row of the same kind is a hand pick (`.u.lcsc &= "C1234"`, chapter 12).
- A board or a module states one, never a part or a physical file. A module's alternate is checked in each place the module is used, and a member may have several, each checked alone.
- With the alternate fitted, the rest of the source is read as written: the connections and narrowings that name `.u` and its pins reach the alternate, so a pin it lacks is refused where it is used, and a supply range or a level it does not meet is refused as it would be for the member.
- The footprint is the member's: when both parts state a package it must be the same, and each pad of the alternate must be joined to the same pins of the other parts as the member's pad of that number. A pad on no net is on none for both.
- The pick lock holds the member's row; the alternate's row is picked fresh each build and is not written to the lock.
- A board that a system places is checked with the alternate fitted as the system uses it (issue #1414): the system is checked and solved again, its mates are seated again, and the power tree is read across its boards, so a supply that another board gives reaches the alternate.
- The guideline findings (an unconstrained pick, an unstated value, an open circle, a bus role, a track width) are made for the board with the alternate fitted too. Each one the board as written does not have is a warning of the board, said under the alternate's statement as the re-check's errors are (below); with `--strict` or `guidelines = "error"` it stops the build. A finding the board as written has is said once.

A finding that only the board with the alternate fitted has is said under the alternate's statement. Here nothing limits the capacitor's value:

```nsm solve warns
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::parts::passive::Resistor

board Filter {
    .j: Header1x2
    .r: Resistor { .resistance &= 10kohm +/- 5%; .package &= "0402" }
    alternate .r: Capacitor { .package &= "0402" }
    .j.p1 ~ .r.a
    .j.p2 ~ .r.b
}
```

```text
Filter: unconstrained pick guideline: with `alternate .r: Capacitor` (main.nsm:8) fitted in the place of `r`: `r` (Capacitor) was chosen with nothing limiting its capacitance, so the solver took the first row that fits, 18pF 5% 50V C0G 0402 C1549: state it, `.r.capacitance &= 100nF +/- 10%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.r.capacitance &= 10nF..10uF`
```

A value of the alternate alone is stated in the alternate's body, `alternate .r: Capacitor { .package &= "0402"; .capacitance &= 100nF +/- 10% }`, as the member's own body would state it.

An alternate that fails one of the board's checks is named with the check's own message:

```nsm fails
import std::sources::Rail

part ModuleA {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

part ModuleLow {
    .vcc: Rail
    .vcc.voltage <= 2V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .u: ModuleA { .assembly &= AssemblyMode::Hand }
    alternate .u: ModuleLow { .assembly &= AssemblyMode::Hand }
    .input ~ .u.vcc
}
```

```text
error: with `alternate .u: ModuleLow` (main.nsm:23) fitted in the place of `u`: u.vcc.voltage must always be at most 2V, but 3.135V..3.465V is not always below 2V
   --> main.nsm:13:5
   |
13 |     .vcc.voltage <= 2V
   |     ^^^^^^^^^^^^^^^^^^
```

An alternate whose pads carry other nets is refused:

```nsm fails
import std::parts::connector::Header1x2
import std::sources::Rail

part ModuleA {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

/// The same pins on the other pads.
part ModuleTurned {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "2"
        .vcc.neg power "GND" "1"
    }
}

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA { .assembly &= AssemblyMode::Hand }
    alternate .u: ModuleTurned { .assembly &= AssemblyMode::Hand }
    .input ~ .u.vcc
}
```

```text
error: with `alternate .u: ModuleTurned` (main.nsm:27) fitted in the place of `u`: pad 1 of `u` is joined to `j.1` with `ModuleA` fitted and to `j.2` with `ModuleTurned`: an alternate's pads carry the nets of the footprint it is fitted on
error: with `alternate .u: ModuleTurned` (main.nsm:27) fitted in the place of `u`: pad 2 of `u` is joined to `j.2` with `ModuleA` fitted and to `j.1` with `ModuleTurned`: an alternate's pads carry the nets of the footprint it is fitted on
```

### What the datasheet says about layout

A datasheet's layout section says where the parts around a part go: "the input capacitor close to the VIN pin", "the inductor next to SW", "the feedback resistors within 2 mm of FB". A part states it once, in a `layout` block, with the document and the section it comes from (ADR 0030, issues #829 and #994). Every board that uses the part gets it.

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::sources::Rail

/// An invented regulator whose datasheet asks for its capacitors near its pins.
part Ld9 {
    .designator &= "U"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins {
        .vin.pos  power "IN"  "3"
        .vin.neg  power "GND" "1"
        .vout.pos power "OUT" "2"
    }

    layout {
        source "LD9 datasheet, 9.1 Layout Guidelines, p.12"
        /// "Place the input capacitor within 2 mm of the IN pin."
        near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm
        /// "Place the output capacitor close to the OUT pin."
        near output_cap on .vout.pos kind capacitor to .vout.pos
    }
}

board Supply {
    .feed: Header1x2
    .load: Header1x2
    .reg: Ld9
    /// Found by `input_cap`: one pad on IN's net, one on GND's.
    .cin: Capacitor
    /// Found by `output_cap`: a capacitor with a pad on OUT's net.
    .cout: Capacitor
    .feed.p1 ~ .reg.vin.pos
    .feed.p2 ~ .reg.vin.neg
    .load.p1 ~ .reg.vout.pos
    .load.p2 ~ .reg.vin.neg
    .reg.vin.pos ~> .cin ~> .reg.vin.neg
    .reg.vout.pos ~> .cout ~> .reg.vin.neg
}
```

Rules.

- `source "..."` is mandatory, once in each block: the document (its title or number) and the section or page. A part whose guidelines come from two documents has a block for each. A `layout` block is only in a part's own definition, not in a generic (whose catalog parts each have their own datasheet), a module or a board; a catalog row of a standard generic has its block in `std/layouts.nsm` (below).
- `near NAME SELECTOR to .pin [within LENGTH]` says that parts go next to a pin. The name (`input_cap`) is what a finding calls it, and is used once in a part. The selector says which parts of the board it is about, found from the nets of the part's own pins, so the part names no other part and no net:
  - `bridging .a .b`: every other part with two pads, one on the net of each pin (a capacitor across the input);
  - `on .pin`, or `on .pin kind inductor`: every other part with a pad on the pin's net, of that kind when one is given (the words of `#[kind(...)]`, chapter _Attributes_).
- `to .pin` is the pin the parts go next to. A part found is measured from its pads on the net of that pin (all its pads, when none is on it) to the pin's pads, the nearest two, centre to centre.
- `within 2mm` is the number the datasheet gives. With none ("close to"), the placer pulls the parts and nothing is checked: the tool adds no number of its own.
- The placer pulls each part found toward the pin as it pulls a decoupling capacitor (chapter _The shape of a board_). A part that ends further than `within` is a guideline of `fab` (`warning: Supply: layout guideline: ...`, an error with `guidelines = "error"` or `--strict`), never an error of `build`.
- `///` above a statement holds the datasheet's sentence. The standard library's first block is `Lmr16006y`'s, from section 11.1 of its datasheet.

The other statements say what a datasheet says beyond "close to". Each has a name, as `near` has, and every number in them is one value, as the datasheet gives it:

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::{Capacitor, Inductor}
import std::sources::Rail

/// An invented switching regulator, in two packages.
part Ld8 {
    .designator &= "U"
    .vin: Rail
    pins "SOT-23-5" "TSOT-23-5" {
        .vin.pos power   "IN"  "5"
        .vin.neg power   "GND" "2"
        .sw      passive "SW"  "1"
        .fb      passive "FB"  "3"
    }
    .package: Package
    .package &= "TSOT-23-5"

    layout {
        source "LD8 datasheet, 10 Layout, p.21"
        /// "Keep the inductor at least 3 mm from the FB pin."
        away coil on .sw kind inductor from .fb beyond 3mm
        /// "Keep the loop of the input capacitor, IN and GND within 10 mm."
        loop hot bridging .vin.pos .vin.neg within 10mm
        /// "No vias under the IC."
        keepout under at -1.5mm, -1.5mm size 3mm, 3mm forbid vias
        /// "Route SW with a short, wide track."
        route sw_track .sw width 0.6mm length 8mm
    }
    layout "TSOT-23-5" {
        source "LD8 datasheet, 10.2 TSOT-23-5, p.22"
        /// "Use two vias at the GND pin."
        vias ground .vin.neg 2
    }
}

board Converter {
    .feed: Header1x2
    .load: Header1x2
    .reg: Ld8
    .cin: Capacitor
    .coil: Inductor
    .feed.p1 ~ .reg.vin.pos
    .feed.p2 ~ .reg.vin.neg
    .load.p1 ~ .reg.fb
    .load.p2 ~ .reg.vin.neg
    .reg.vin.pos ~> .cin ~> .reg.vin.neg
    .reg.sw ~> .coil ~> .load.p1
}
```

- `away NAME SELECTOR from .pin beyond LENGTH`: the parts the selector finds stay at least the length from the pin's pads, measured from their nearest pad. The number is mandatory: "away from" with no number gives the placer nothing to keep to. The placer pushes the parts out to the distance.
- `loop NAME bridging .a .b [within LENGTH]`: each part with a pad on the net of each pin closes a current loop with the part, from `.a` through the part found to `.b` and back. Its length, the shortest such path from pad centre to pad centre, is at most the number. The placer pulls the part found to both pins, as it pulls a decoupling capacitor.
- `keepout NAME at X, Y size W, H [forbid tracks vias copper parts]`: an area in the part's own frame (its footprint's, turned with it), as a `Keepout` member of a part has (chapter _The shape of a board_). It forbids what `forbid` names on every copper layer, all four when it names none. It is a keepout the part carries, `reg.under`, which the placer, the router and the pour keep to, as they keep to any other.
- `edge NAME .pin [within LENGTH]`: the pin's pads go at the board's edge, within the length of it. The placer pulls the part to an edge as it pulls a connector.
- `vias NAME .pin N`: the pin's pads take at least N vias of their net, each with its centre on one of them. It is checked on the routed board.
- `route NAME .pin [width LENGTH] [length LENGTH]`, one of the two at least: the tracks of the pin's net are at least the width wide (the net's least width, which the router keeps to) and at most the length long in all (checked on the routed board). A routed net longer than the length is a guideline of `fab` that names the net, its length, the limit and how far over it is (``the tracks of `XOUT` are 36.19 mm, 21.19 mm over``). It is how a datasheet that keeps a line short says so: an oscillator that keeps each line to its crystal under 15 mm, its load capacitor included, has `route xin .xin length 15mm` and `route xout .xout length 15mm`, one statement for each pin. A net that came out long against the straight line between its pads is listed in `fab-notes.txt` whether or not a part states a length (chapter _The shape of a board_, _Nets the routing made long_).
- `kelvin NAME SELECTOR to .pin`: the pin is a sense pin, a _Kelvin connection_ (issue #1148). It senses the pad of the part the selector finds on the pin's net (a shunt's terminal, say), and the router routes it from that pad alone: no other copper of the net joins its track on the way, so no load current flows in it and the sensed voltage is the pad's. The pin is one of the selector's (`bridging .in_p .in_n to .in_p`, `on .sense kind resistor to .sense`); of several pads of the part found on the net, the one nearest the pin is sensed. It has no number. On a net with a pour, a plane or a zone, the pour is carved round the sense copper, so it does not join the sense track to the rest of the net (issue #1174). `fab` warns, as for the other statements, when the statement finds no part or more than one, and when the routed copper of the pin joins any other pad of the net (chapter _The shape of a board_, _Kelvin sense connections_).
- `layout "TSOT-23-5" { }` holds the guidelines of one package of the part: a board uses them only when the part is in that package. The package is one that a pin table of the part names, when its tables name packages. A `keepout` or `vias` of a tab or an exposed pad goes in such a block.

A current-sense amplifier's datasheet asks for Kelvin connections to its shunt:

```nsm
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

/// An invented current-sense amplifier, with the pins of its inputs and supply.
part Csa9 {
    .designator &= "U"
    pins {
        .in_p input "IN+" "1"
        .in_n input "IN-" "2"
        .vs   power "VS"  "3"
        .gnd  power "GND" "4"
    }

    layout {
        source "CSA9 datasheet, 9.1 Layout Guidelines, p.18"
        /// "Connect IN+ and IN- to the pads of the shunt with Kelvin connections."
        kelvin sense_p bridging .in_p .in_n to .in_p
        kelvin sense_n bridging .in_p .in_n to .in_n
    }
}

board Monitor {
    .feed: Header1x2
    .load: Header1x2
    .supply: Header1x2
    .amp: Csa9
    /// Found by both statements: one pad on IN+'s net, one on IN-'s.
    .shunt: Resistor
    .feed.p1 ~ .shunt.a
    .shunt.a ~ .amp.in_p
    .shunt.b ~ .amp.in_n
    .shunt.b ~ .load.p1
    .feed.p2 ~ .load.p2
    .supply.p1 ~ .amp.vs
    .supply.p2 ~ .amp.gnd
    .supply.p2 ~ .load.p2
}
```

A catalog row has no definition to hold a block, and a generic such as `LinearRegulator` picks rows that each have their own datasheet. The standard library's `std/layouts.nsm` therefore has one block for each row whose datasheet has a layout section, keyed by the row's manufacturer's part number, the `mpn` of its catalog file (issue #1032):

```text
layout "XC6206P332MR-G" for LinearRegulator {
    source "Torex ETR0305, 9 Layout, p.9"
    near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm
}
```

The statements are those of a part's block and name the generic's pins. Every `LinearRegulator` is given the blocks of its rows, and a board's design keeps the guidelines of the row the solver picked (or the lock, or `.mpn &=`) and none of any other row's. A row has one package, so its block needs no `layout "PACKAGE"` of its own; a name may be used in the blocks of two rows. Only `std/layouts.nsm` may hold such a block, and `for` names a standard generic. A row whose datasheet has no layout section, or whose datasheet could not be read, has no block: `data/jlcpcb/README.md` says which. The block above is an example and not a real row's: `XC6206P332MR-G`'s datasheet has no layout section, so a board that picks it has no guideline.

A number a board does not meet is a guideline of `fab`, as for `near`. A board that does not meet one on purpose says why in its layout file, under the part's path and the guideline's name; `fab` then says it is waived, with the reason, and neither warns nor stops (an entry for a guideline the board does not have, or with no reason, is an error):

```toml
[Converter.waive."reg.hot"]
reason = "the input capacitor is on the bottom under IN and GND, with two vias each"
```

`netsmith layout summary` lists what each guideline measures on the board, with the number it asks for.

The same table waives a finding of the design rules check of a part's own footprint, under the part's path and the rule's name, with a reason, a source and the pads or the place (issue #1196; chapter _The shape of a board_, _The layout file_).

A block without its source is refused:

```nsm fails
part Ld9 {
    pins {
        .a passive "A" "1"
    }
    layout {
        near pull on .a to .a
    }
}

board B {
    .u: Ld9
}
```

```text
error: this `layout` block states no source: add `source "..."` with the document and the section that give its guidelines, such as `source "SNVSA24, 11.1 Layout Guidelines, p.17"`
  --> main.nsm:5:5
  |
5 |     layout {
  |     ^^^^^^^^
```
