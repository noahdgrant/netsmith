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
- `route NAME .pin [width LENGTH] [length LENGTH]`, one of the two at least: the tracks of the pin's net are at least the width wide (the net's least width, which the router keeps to) and at most the length long in all (checked on the routed board).
- `layout "TSOT-23-5" { }` holds the guidelines of one package of the part: a board uses them only when the part is in that package. The package is one that a pin table of the part names, when its tables name packages. A `keepout` or `vias` of a tab or an exposed pad goes in such a block.

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
