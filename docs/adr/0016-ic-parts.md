---
status: accepted
---

# An IC is a part with typed pins, a pinout per package, and a KiCad mapping read from the installed libraries

Accepted. The user chose a pin table in the part (Option B of the proposal, kept below as the decision): a part declares its pins in a `pins` table, one table for each package. The language, the checker, the solver, the lowering and the core model are implemented (steps 4, 5, 7, 8, 9 and the language part of 12); KiCad's side is built except step 10 (steps 1, 2, 3, 6, 11 and 13; ADR 0021 records what was decided in building them, among it support for multi-unit symbols and for a project's own libraries, which replace the refusal of a multi-unit symbol in step 1 and the last two "Not decided here" items below). The tested examples are in `docs/LANGUAGE.md`; the `.nsm` blocks here are the design as decided.

## The problem

Only five kinds of part export today: `Resistor`, `Capacitor`, `Led`, `LinearRegulator` and `Header1x2`. Their KiCad symbol and footprint names are a Rust table in `crates/netsmith-kicad/src/mapping.rs`, their symbols are copies of KiCad's stock files in `crates/netsmith-kicad/symbols/`, and any other `part` fails at export with "there is no KiCad symbol for a `X` yet". A real board needs multi-pin ICs. Two cases drive the design.

- A fixed-output LDO in a package with 9 pads (8 pins and an exposed pad). Two pads are the output. Its sense pin must be tied to the output. One pad is no-connect. Ground is a pad and the exposed pad.
- A microcontroller-like IC with about 30 pins. Several pins have more than one function (GPIO, UART transmit, SPI data out), and the design chooses one.

What the language can say today, tried against the current tools (see "Checked against the parser"):

- A pin has a `.pad` and a `.net` and nothing else. It has no electrical kind, no name from the datasheet and no functions.
- One pin has one pad. Several pads of one pin work only as extra pins joined inside the part with `~`. Lowering already treats two pins on one pad as one physical pin, so the nets come out right (`u.vout.pos`, `u.vout_b` and `u.sense` on one net).
- Pads that depend on the package come from catalog columns (`vin.pos.pad` in the `ldo` catalog, written in Rust in `crates/netsmith-solve/src/jlcpcb.rs`). A pad that is a package-independent truth is written in the part body (`.pad &= "1"`). That is already two ways.
- A pin that is joined to another pin inside a part but has no pad stops lowering: "its pins (`pa9.reference.neg`, `pa9.reference.pos`) have no pads". This bites the moment a part puts a `Logic` interface on a pin, and a `Rail` pair that shares a ground (`.vin.neg ~ .vout.neg`) when only one of them gets a pad.
- Supply current and operating range already work in a part: `.vdd.current &= 1mA..25mA` gives the board `rail.current = -25mA..-1mA`, and `.vdd.voltage <= 3.6V` on a 5 V rail fails with "must always be at most 3.6V, but 4.75V..5.25V is not always below 3.6V" (ADR 0007, ADR 0014).
- Nothing checks that a pin is connected, that a no-connect pin is not, or that two pins are tied.
- Footprints are already read from the installed KiCad libraries at run time (`FootprintLibrary::installed()`, `NETSMITH_KICAD_FOOTPRINTS`), and the PCB side already works in pad numbers, with several pads sharing a number (`Footprint::pads_numbered`). Symbols are not: `SymbolLib::builtin()` holds six bundled files, and the viewer and the emitter both use it.

## Decisions that do not depend on the syntax

These hold whatever the syntax for declaring pins.

**1. A part says what is true of the part; a pinout says what is true of one package.** The pin kinds, the datasheet names and the functions are the same in every package. The pad numbers are not. A pinout is a statement about a part in a package and names no CAD tool, so it is library source (ADR 0002). Pad numbers stop being catalog columns: the catalog row names the package and the pinout of that package supplies the pads. The `ldo` catalog loses its four `*.pad` columns.

**2. A pin has a kind from a closed set.** `power` (the pins of a `Rail`; whether it takes or gives power is the sign of the rail's `current`, which the part already states), `input`, `output`, `bidirectional`, `passive` and `no_connect`. These are the KiCad electrical types that mean something to a checker. `tri_state`, `open_collector` and the rest can be added when a part needs them; until then a part uses `bidirectional` or `passive`.

**3. A pin has a datasheet name.** `OUT`, `SENSE`, `PA9`. It is the name the datasheet prints, so it is true of the part in any tool, and it is what the symbol is validated against (decision 6). Case does not matter in the comparison.

**4. The checker reports pin faults on the board, before the solver runs.** None of these need a pad or a library.

| Fault | Rule | Message |
|---|---|---|
| Required pin unconnected | `power` and `input` pins must have another part's pin on their net, unless the pinout says `optional`. `output`, `bidirectional` and `passive` pins may float unless it says `required`. | `` `u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net `` |
| Tied pin not tied | `tie .a ~ .b` in a part means the board must put the two on one net. | `` `u.sense` must be on the same net as `u.vout.pos` (`Ldo9` ties them), but nothing is connected to `u.sense` ``, or, when it is on another net, `` ... but `u.sense` is on net `VIN` with `input.pos`, `u.en`, `u.vin.pos` `` |
| No-connect pin connected | A `no_connect` pin must have nothing else on its net. | `` `u.nc` is a no_connect pin (pad 5, `NC`) but it is on net `3V3` with `output.pos`, `u.sense`, `u.vout.pos` `` |
| Two outputs on one net | Two `output` pins on one net; pins that the part joins inside are one. An `output` and a `bidirectional` pin are allowed. Two power sources are not checked yet: whether a power pin gives or takes is the sign of the rail's current, which is known only after the solve (a known limit). | `` net `X` has two outputs: `a.tx`, `b.tx` `` |
| Function not chosen | A connected pin with more than one function needs `.function &=` from the board. | `` `u.pa9` has several functions (GPIO, UART1_TX, SPI1_MOSI) and is connected: choose one with `.u.pa9.function &= "GPIO"` `` (the hint names the first function listed) |
| Function not offered | `.function &=` names a function the pin does not list. | `` `u.pa9` cannot be `I2C1_SDA`; its functions are GPIO, UART1_TX, SPI1_MOSI `` |
| Function used twice | One function on two pins of one part. | `` function `UART1_TX` is chosen on both `u.pa9` and `u.pb6` `` |

"Another part's pin" means a terminal whose owning part is not this one, so a net made only of pins joined inside the part counts as unconnected. Messages point at the board's declaration of the part (`.u: Ldo9`) and name the pinout line.

An intentionally unused `optional` pin needs no statement. Intentionally leaving a `required` pin open is refused; if the datasheet allows it, the pin is not `required`. A function is chosen on the board because the board decides what the pin is for; there is no default function, since a default would be one more thing a reader must remember. Connecting by function (`.u.uart1_tx` and letting the tool pick the pin) is left open.

**5. Pads and pins are checked after the solver has chosen the package.** Two pins on one pad that are not joined inside the part is an error naming both pins (`pad 3 is given to `sense` and `en``). A pin joined inside the part to a pin that has pads needs none of its own (an alias): this removes the "have no pads" stop above and makes `.vin.neg ~ .vout.neg` need one pad line. A pin with no pads and no alias is an error in the part, once.

**6. KiCad names live in a per-tool mapping file, keyed by part and package.** Not in the part (ADR 0002), and not in Rust. The file is TOML, like the manifest (ADR 0011). `std/kicad.toml` ships with the standard library and holds today's five kinds; a project may add `kicad.toml` at its root for its own parts. A (part, package) key that is in both files is an error, not an override, so there is one answer. The Rust `ENTRIES` table goes.

```toml
# std/kicad.toml. The only place KiCad's names appear.
[[part]]
kind = "Ldo9"
package = "DFN-8-EP"
symbol = "Regulator_Linear:LDO_Sense_8"          # illustrative: not a real KiCad symbol
footprint = "Package_DFN_QFN:DFN-8-1EP_3x3mm_P0.65mm_EP1.55x2.4mm"

# Only where KiCad's symbol names a pin differently from the datasheet:
[part.pin_names]
SENSE = "SNS"
```

The footprint named above exists in KiCad 9's `Package_DFN_QFN` and has pads `1` to `8`, the exposed pad `9`, and four unnumbered paste apertures. The symbol name is made up, so the example never passes validation against the stock library.

**7. Export reads the symbols and footprints from the installed KiCad libraries.** `.kicad_sym` files are read from `NETSMITH_KICAD_SYMBOLS` (new, by the pattern of `NETSMITH_KICAD_FOOTPRINTS`), defaulting to the Windows install seen from WSL. Each `Library.kicad_sym` holds every symbol of the library, 2 to 3 MB for the larger ones, so the loader parses a library once per run and extracts the symbol asked for. A symbol that `extends` another is flattened (the parent's pins and drawing, the child's properties), since the schematic's `lib_symbols` holds whole symbols. The bundled copies in `crates/netsmith-kicad/symbols/` and the footprint fixtures in `crates/netsmith-pcb/fixtures/` go, which also settles the README licensing note. Tests write small `.kicad_sym` and `.kicad_mod` files of their own into a temporary folder; tests that want the stock libraries skip themselves when KiCad is absent, like the existing KiCad tests.

When a library is missing:

- `check` and `solve` need no CAD library and never read one.
- `build` and `fab` stop, once, with the path and the variable: `` symbol library `Regulator_Linear` not found: looked for /mnt/c/.../symbols/Regulator_Linear.kicad_sym; set NETSMITH_KICAD_SYMBOLS to the folder that holds the .kicad_sym libraries ``. A library that is present but lacks the symbol says so and names the library. Nothing falls back to a drawn box in `build`.
- `view` keeps its fallback box with the reason as the tooltip, as it does for a kind it has no symbol for today.

**8. A mismatch between the part, the symbol and the footprint is an error naming both sides.** It runs at export, when both libraries have been read, and also from a test that validates every row of `std/kicad.toml` when KiCad is installed. For each part in a package:

- Every pad in the pinout must be a numbered pad of the footprint, and every numbered pad of the footprint must be in the pinout. Unnumbered pads (paste apertures) are ignored. Several footprint pads with one number are one pad.
- Every pad in the pinout must be a pin number of the symbol, and every symbol pin must be a pad. Hidden duplicate pins at one point (how stock symbols stack `OUT` pins 1, 2 and 3) are ordinary pins here.
- The symbol pin on a pad must have the part pin's datasheet name, after the `pin_names` table. This is the check that catches two pads swapped between the part and the symbol, which the pad numbers alone do not.
- The symbol pin's electrical type must suit the part pin's kind: `power` with `power_in`, `power_out` or `passive`; `input` with `input`; `output` with `output`, `tri_state` or `open_collector`; `bidirectional` with anything but `power_*`; `no_connect` with `no_connect` or `passive`; `passive` with `passive` or `unspecified`. A symbol pin of type `passive` is always accepted, because that is how stock symbols hide stacked duplicates.

```text
error: U1 (Ldo9, package "DFN-8-EP"): pad 5 is the no-connect pin `nc` (`NC`) in the part, but pin 5 of the symbol `Regulator_Linear:LDO_Sense_8` is named `EN`
error: U1 (Ldo9, package "DFN-8-EP"): the pinout has pad 9 (`GND`, the exposed pad) but the footprint `Package_DFN_QFN:DFN-8-1EP_3x3mm_P0.65mm_EP1.55x2.4mm` has pads 1, 2, 3, 4, 5, 6, 7, 8 only
error: U1 (Ldo9, package "DFN-8-EP"): the footprint `...` has pad 9, which no pin of the part has; give it to a pin in the pinout
```

The symbol's `(alternate "TIM14_CH1" ...)` entries on microcontroller pins could be checked against a pin's functions. They are named by the vendor's peripheral numbering, which will not match a part's names in general, so this ADR does not check them. Left open.

**9. Electrical models use what exists.** A part that draws declares `.vdd: Rail` and narrows its current as ADR 0014 says, so an IC counts against the regulator that feeds it with no new construct. The part states the datasheet's envelope (`.vdd.current &= 1mA..25mA`, widest case), and a design narrows it to its use (`.u.vdd.current &= 4mA..8mA`). Operating range and absolute maximum are relations on the rail's voltage (`.vdd.voltage >= 1.7V`, `.vdd.voltage <= 3.6V`), which the checker already tests over the whole range, with the numbers in the message. A comment on the line says which table it is, and the message points at that line. Not in this ADR: logic levels (below, "Models"), a draw by a pin that has no rail, and sequencing.

**10. A design says "this exact part" by narrowing the catalog's key.** No new mechanism for choosing: the part has `.mpn: String` as a catalog column, and `.mpn &= "..."` leaves one candidate, as `.package &= "0402"` leaves some today. Not naming it lets the solver pick the first fitting row, which is right for an LDO and wrong for a microcontroller, so a catalog may be keyed: `catalog mcu by .mpn { ... }` makes an unset `.mpn` an error that lists the candidates (`` `u` is an `Mcu32`; name the part with .mpn: MCU32-A, MCU32-B ``). The parts in this ADR are invented; a real row needs its source in `data/jlcpcb/README.md` like any other.

## The decision: a pin table in the part, one table for each package

A `pins` block is the datasheet's pin table: one row for each pin with its path, kind, datasheet name and pads, then optional `fn` and `optional` or `required`. A block named for a package applies when the part is in that package; an unnamed block applies to every package, for a part whose pads do not depend on it (a resistor). A part has either one unnamed block or one block for each package, not both, so a pin never has two answers. A row names a plain pin (`.sense`), which the row declares, or a pin of an interface (`.vout.pos`), which the interface already has. Pads are text, so they are quoted. A row is whitespace-delimited tokens and ends at a newline or `;`:

```text
.path  kind  "NAME"  "pad" ["pad" ...]  [fn FUNCTION ...]  [optional | required]
```

The kinds are `power`, `input`, `output`, `bidirectional`, `passive` and `no_connect`. `tie .a ~ .b` is a statement of the part. `optional` and `required` change a pin's default (decision 4) and are refused where they change nothing (`optional` on an output, `required` on an input). Here is the LDO of the problem, in the two packages the user described:

```nsm
import std::sources::Rail

/// A fixed-output LDO. Illustrative; not a real part.
part Ldo9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg          // `vout.neg` needs no row: it is joined to `vin.neg`

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "DFN-8-EP" {
        //  pin        kind        name     pads
        .vout.pos      power       "OUT"    "1" "2"
        .sense         input       "SENSE"  "3"
        .vin.neg       power       "GND"    "4" "9"
        .nc            no_connect  "NC"     "5"
        .en            input       "EN"     "6"
        .vin.pos       power       "IN"     "7" "8"
    }
    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }
    tie .sense ~ .vout.pos

    .package: Package
    catalog ldo9 { .vout.voltage, .dropout, .max_current, .package }
}
```

The board is unchanged by any of this. The board for the LDO, which does not depend on the packages:

```nsm
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail
import std::parts::connector::Header1x2

board LdoBoard {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }

    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Ldo9 { .vout.voltage &= 3.3V +/- 5% }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output

    .u.sense ~ .u.vout.pos     // the part ties these; leave this out and `check` says so
    .u.en ~ .u.vin.pos         // always on

    .power_in: Header1x2
    .power_out: Header1x2
    .input.pos ~ .power_in.p1
    .input.neg ~ .power_in.p2
    .output.pos ~ .power_out.p1
    .output.neg ~ .power_out.p2
}
```

The pads of `U1` in the exported nets for the DFN: `3V3` has `1`, `2` and `3`; `GND` has `4` and `9`; `VIN` has `7`, `8` and `6`; pad 5 is on no net and gets a no-connect flag in the schematic.

A microcontroller, abridged, in a catalog keyed by its part number (decision 10), with the board choosing a function:

```nsm
part Mcu32 {
    .vdd: Rail
    .vdd.voltage >= 1.7V
    .vdd.voltage <= 3.6V
    .vdd.current &= 1mA..25mA

    pins "QFN-32" {
        .vdd.pos   power          "VDD"   "1" "17" "32"
        .vdd.neg   power          "VSS"   "8" "24" "33"
        .nrst      input          "NRST"  "4"
        .pa2       bidirectional  "PA2"   "12"  fn GPIO UART1_TX SPI1_SCK
        .pa3       bidirectional  "PA3"   "13"  fn GPIO UART1_RX
        .boot0     input          "BOOT0" "31"  optional
        // ... more rows
    }
    pins "TSSOP-20" {
        .vdd.pos   power          "VDD"   "4"
        .vdd.neg   power          "VSS"   "2"
        .pa2       bidirectional  "PA2"   "8"   fn GPIO UART1_TX
        // ...
    }
    .package: Package
    .mpn: String
    catalog mcu by .mpn { .mpn, .package }
}

// in the board
//   .u: Mcu32 { .mpn &= "MCU32-A" }
//   .u.pa2.function &= "UART1_TX"
```

What the user decided, beyond the sketch:

- **The same pin in two packages has one kind and one datasheet name.** The checker refuses a pin that differs, with an error that names both blocks (`` `.x` is an output named `X` in `pins "B"` but an input named `X` in `pins "A"` ``). The pads and the functions may differ. Deviation from the user's wording: `optional` and `required` must agree too, because the checks of decision 4 run on the board before the solver may know the package, and they need one answer.
- **A pin missing from the chosen package does not exist there.** A `tie` that names it is skipped for that package. A board that connects it gets an error that names the part, the pin, the package and the packages that have it (`` `u.sense` (`SENSE`) does not exist in package `SOT-23-5` of `Ldo9`, but the board connects it; the packages that have it: `DFN-8-EP` ``). Nothing is said of a pin the board does not connect, so the same board works in the smaller package.
- **The solver avoids a package the board cannot use.** A catalog row whose package lacks a pin that the board connects is not a candidate; when no row is left the message says why (`` the `ldo9` catalog has nothing for `u`, which needs ... (rows were left out because package `SOT-23-5` has no `sense`, which the board connects) ``). This is the simple case: it looks at the pins that the board connects, not at whether another package would make a tie or a required pin impossible. A rule that depends on the package waits for the solver when the board has not named the package (a catalog row chooses it); the others are reported when the board is checked.
- **A catalog may be keyed** (`catalog mcu by .mpn`): an unset key is an error that lists the candidates, and `.mpn &= "..."` selects one row. The key must be one of the catalog's columns.
- **The pad is not a field.** `.pad &= "1"` is gone (the pin has `.net` and `.function`), so there is one way to give a pad. A plain `.name: Pin` in a part has no pad of its own and is useful only as an alias for a pin that has.
- **KiCad names stay out of this phase.** They go in a TOML mapping file (decision 6), which is step 2 and not implemented.

**Rejected.** The proposal had two other options. Option A gave each pin a built-in type (`Input`, `NoConnect`) and kept pads as `.pad` fields or catalog columns, so a part in several packages would have gone back to one catalog column for each pin and pad, and a second way to give a pad stayed. Option C declared pins by type in the part and moved the pads into a new top-level `package "X" of Part { ... }` definition, which splits the datasheet's pin table in two places, needs a list-valued assignment and a fifth kind of definition, and has no honest answer for a pin that a package lacks. The pin table is the only one with one way to give a pad and one table to check against the datasheet.

## How phase 1 differs from the proposal

- A part with `pins` tables for packages and no package yet is judged twice: what the board alone settles is reported when the board is checked, and the rules that need the package wait for the solver (`pin_faults_solved` in `netsmith-check`, called by the solver). The pad rules of decision 5 run there too, for a part that has tables. A part with no table at all, whose pins are on nets, is stopped by lowering with the same wording.
- A net counts as company for a `power` or `input` pin if it has a pin that is not the part's own, including a rail the board declares: a rail that nothing on the board drives is supplied from outside (ADR 0007). A module's port the board leaves open is not seen.
- `netsmith_core::Part` has `pins` (name, datasheet name, kind, pads, chosen function) and `PinRef.pin` is still the pad, one reference for each pad. A pad that is on no net is listed in `Part::pins` only; the exporters that mark it are phase 2.
- Two power sources on one net are not checked (see decision 4).

## Models

Supply current, operating range and absolute maximum work today, as decision 9 says. Logic levels do not, and need a net-level check like the balance of ADR 0007.

`std::logic::Logic` is a line and its reference rail, with no levels. Give it `.vih`, `.vil`, `.voh` and `.vol` (voltages, ranges). A part states them relative to its rail: `.pa7.voh >= .vdd.voltage - 0.4V`, `.pa7.vih <= 0.7 * .vdd.voltage` (a threshold is a range because the rail is). On a net with an `output` or `bidirectional` pin driving and an `input` or `bidirectional` pin receiving, the checker requires `driver.voh` always at least `receiver.vih` and `driver.vol` always at most `receiver.vil`, and says both ranges. For the absolute maximum, a receiver states `.abs_max` and `.abs_min` relative to its own ground; on a net a rail's `pos` is on, the net's voltage is the rail's, and the check applies when the receiver's ground is on that rail's `neg` net. Otherwise the check is listed as not evaluated, the way the solver lists what it could not evaluate. A pin that sources current into a load (a GPIO driving an LED) draws from its reference rail, but ADR 0014 does not count a chain that starts on a signal; that stays a limit. Because the pin kinds exist, "two outputs on one net" is free in the checker (decision 4).

## Implementation steps

Sizes: S is under half a day, M one to two days, L three or more. Phase 1 is done: steps 4, 5, 7, 8 and 9 and the language part of 12. Phase 2 is KiCad's side: steps 1, 2, 3, 6, 10, 11 and 13, and the rest of 12.

| # | Crate | Step | Size |
|---|---|---|---|
| 1 | kicad | Symbol library loader for `.kicad_sym`: read a library once, find a symbol by `Library:Name`, flatten `extends`, read `hide`, the full set of electrical types, pin names and numbers, and refuse a multi-unit symbol with its unit count. `SymbolLib` keeps its methods (`get`, `pin_geometry`, `pin_type`, `field_offset`). Error for a missing library or symbol as in decision 7. | L |
| 2 | kicad | Mapping from `std/kicad.toml` and a project `kicad.toml`, keyed by (kind, package); `through_hole` read from the footprint's `attr`, not a table flag; replace `ENTRIES`. Errors for a duplicate key and for an unknown part keep their wording. | M |
| 3 | pcb / netsmith | Validation of decision 8: one function taking the part's pinout, a symbol and a footprint, in `netsmith-pcb` (it has both). Errors name both sides. A test over every row of `std/kicad.toml`, skipped without KiCad. | M |
| 4 | core | `Part` gets the facts exporters need: pins with kind, datasheet name, pads and chosen function. `PinRef.pin` stays the pad. | S |
| 5 | lower | One `PinRef` per pad; the alias rule (decision 5); carry the pin facts; take the reference letter, value and `hand_soldered` from the part and mapping instead of the `match` on `Resistor`, `Led` and `Header1x2` in `crates/netsmith-lower/src/lib.rs` (the reference letter is built: it is the part's field `.designator`, ADR 0046; the value and `hand_soldered` are still a `match`). | M |
| 6 | kicad | Emitter: read symbols from the loader; one label per distinct point (hidden duplicates share a point with their visible pin); a `no_connect` flag on a pin whose pad is on no net; pin electrical types from the symbol feed the power-flag rule as now. ERC with real KiCad on the worked example, using the `check-kicad.sh` script. | M |
| 7 | check | The pin rules of decision 4, with messages that name pins, pads and the numbers; the pad rules of decision 5 after the solve; function choice; the `tie` and `pins` data in the model. | M |
| 8 | lang | The `pins` block grammar, `tie`, `fn`, `optional`, `required`, and the migration of the five library parts. | M to L |
| 9 | solve | Catalog `by .mpn` (an unset key is an error that lists candidates); the `package` column selects the pinout; remove the four `*.pad` columns from `ldos()` and its tests. A catalog row for an IC needs its source recorded in `data/jlcpcb/`. Rows were written in Rust in `jlcpcb.rs` then; ADR 0022 moved them to CSV files. | S to M |
| 10 | pcb | Footprints with an exposed pad, fine pitch and several pads on one net: check that the placer, the ratsnest and the router connect every pad of a net (both output pads, the exposed pad) and that pads on no net (no-connect) are left alone. The router's 0.65 mm pitch against the process minimum is the risk: not tried. No thermal vias under an exposed pad; no pin-aware placement (decoupling capacitors near the supply pins). Both are limits to record. | M to L |
| 11 | view | Use the symbol loader and the mapping in place of `symbol_for` and the bundled library (`glyph.rs`, `crates/netsmith/src/view.rs`). `read_pin` ignores `hide yes`, so hidden stacked pins would draw twice at one point with overlapping names: skip hidden pins whose point and name another pin repeats. `Flow` marks only `input`/`power_in` and `output`/`power_out`; `bidirectional` and `tri_state` need a case. The fallback box lists only the pads a net uses and names them by pad; with pin facts in the `Design` it should list every pin of the package with its datasheet name and kind, power on top, ground below, inputs left, outputs right, no-connect last, and show the chosen function (`PA2 UART1_TX`). Layout tests with a 30-pin symbol. | M |
| 12 | docs | `docs/LANGUAGE.md` section and tested examples, `GLOSSARY.md` (Pin kind, Pinout, Tie, Function, Mapping), README (remove "hard-coded for five kinds", update the licensing note), `.agents/skills/netsmith-dev-add-part`, tests for every message above. | M |
| 13 | repo | Delete `crates/netsmith-kicad/symbols/*.sexp` and `crates/netsmith-pcb/fixtures/` once the tests write their own libraries; the byte-for-byte schematic tests move to those. | S |
| 14 | check / lang | Logic levels and absolute maximum on pins (the Models section): `Logic` fields, the driver and receiver check, not evaluated when the ground differs. Built: ADR 0046. | L, later |

Order: 4 and 1 and 2 first (nothing visible), then 3, 5, 6 and 11 so that the five existing kinds export through the loader with identical output (the examples must still pass ERC and DRC with 0 violations), then 7 and 8, then 9 and 10 for a real part, then 12 and 13.

## Checked against the parser, before it was implemented

With the tools at the commit that proposed this ADR (`netsmith-lang --example parse`, `netsmith-check --example check`, `netsmith build`):

- A part with the extra-pad pattern of Option A, written with `Pin` for every pin and no `tie`, parses and checks. The board's nets come out as `3V3: u.vout.pos, u.sense, u.vout_b`, `GND: u.vin.neg, u.vout.neg, u.ep` and `VIN: u.vin.pos, u.vin_b`. `build` then stops, for two reasons the ADR names: `u.vout.neg` has no pad (the alias rule is new) and `Ldo9` has no KiCad symbol.
- `.package: Package` followed by `.package &= "DFN-8-EP"` in a part body works.
- `.vdd.current &= 1mA..25mA` in a part gives the board `rail.current = -25mA..-1mA`; `.vdd.voltage <= 3.6V` on a 5 V rail fails with the numbers; on a 3.3 V rail it passes.
- A `Logic` interface inside a part stops lowering ("its pins (...) have no pads"), as the problem section says.
- New, not read by the current parser: `tie ...` (first error at the `.` after `tie`), the `pins "..." { ... }` block (first error at the string), `package "..." of ...`, the types `Input` and `NoConnect` ("unknown type `Input`"), `functions`, `fn`, `catalog ... by`, and `.name` as a field of a pin. Everything else in the examples is current syntax.
- The KiCad facts used here were read from the installed KiCad 9 libraries: stacked pins in stock symbols are separate pins sharing a name and a point, the extra ones `hide yes` and typed `passive`; pins carry `(alternate "NAME" type shape)` entries; a footprint with an exposed pad has that pad numbered and extra unnumbered paste pads; `Regulator_Linear.kicad_sym` alone has 1285 `(extends ...)` entries.

## Not decided here

- ~~Whether a catalog should be able to hold rows in `.nsm` or CSV so that adding an IC stops being a Rust edit.~~ Decided provisionally in ADR 0022: rows are CSV files, one per catalog.
- Connecting by function and letting the tool assign the pin.
- Multi-unit symbols (a microcontroller drawn as several units). The loader refuses them with a message; the emitter places unit 1 only.
- Symbols and footprints from a project's own library (a `sym-lib-table`). Only the installed KiCad libraries are read.
- Whether the KiCad version in the manifest should be checked against the version in the library file's header. A library written by a newer KiCad than the one `kicad-cli` is might not open.
