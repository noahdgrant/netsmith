## Pins, packages and ties

A part's `pins` block is the datasheet's pin table. Each row is one pin: its path, its kind, the name the datasheet prints, its pads, and optionally its functions and whether it may float. A block named for a package applies when the part is in that package; a block with no package applies to every package, for a part whose pads do not depend on it (ADR 0016).

```nsm-fragment
pins "DFN-8-EP" {
    //  pin        kind           name     pads
    .vout.pos      power          "OUT"    "1" "2"
    .pa2           bidirectional  "PA2"    "12"  fn GPIO UART1_TX
    .boot0         input          "BOOT0"  "31"  optional
}
tie .sense ~ .vout.pos
```

A row is `.path kind "NAME" "pad" ["pad" ...] [fn FUNCTION[:kind] [class] ...] [optional | required] [abs_min VALUE] [abs_max VALUE] [from .pin]`, on one line, or ended by `;`. The names and pads are text, so they are quoted. A function is a plain name, with a kind of its own after a colon when the datasheet gives one, and `class` after it when the board may choose it on many pins (see Functions).

| Kind | Meaning | Must be connected? |
|---|---|---|
| `power` | A pin of a supply. Whether it takes or gives power is the sign of its rail's `current`. | Yes, unless `optional` |
| `input` | A signal the part reads. | Yes, unless `optional` |
| `output` | A signal the part drives. | No, unless `required` |
| `bidirectional` | A signal in both directions. | No, unless `required` |
| `passive` | A pin with no direction, such as a resistor's. | No, unless `required` |
| `no_connect` | A pin that must stay open. | Never connected |
| `tri_state` | An output that drives only while its part is selected, and is high impedance otherwise. | No, unless `required` |
| `open_drain` | An output that only pulls its line low, or lets it go (open drain, or open collector). A pull-up sets the high. | No, unless `required` |

"Connected" means that another pin is on its net: a pin of another part, or a rail the board declares, which is supplied from outside. A net made only of the part's own pins does not count.

Here is a regulator in two packages. It is invented for this guide and is not a part you can buy. The DFN has a sense pin and an exposed pad; the SOT-23-5 has neither.

```nsm module=src::ldo9
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

A board for it. `build` would stop at export, because `kicad.toml` has no row for `Ldo9`, so this block is checked and not built.

```nsm
import src::ldo9::Ldo9
import std::sources::Rail

board LdoBoard {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: Ldo9 {
        .package &= "DFN-8-EP"
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output

    .u.sense ~ .u.vout.pos     // the part ties these
    .u.en ~ .u.vin.pos         // always on
}
```

Rules.

- A row names a plain pin of the part (`.sense`), which the row declares, or a pin of one of its interfaces (`.vout.pos`). Naming a pin that is already declared with `: Pin`, or an interface, or a value, is an error.
- A part has one unnamed table or one table for each package, not both. A table that names a package needs a `.package: Package` in the part. Several packages with the same pads share one table, `pins "SOT-23" "SOT-223" { ... }` (next section); a package is named once, in one header.
- A pin has one kind, one datasheet name, one `optional` or `required` and one set of absolute limits in every package. Its pads and functions may differ. A pin that only some packages have does not exist in the others.
- Several pads on a row are one pin that the package brings out more than once: `"1" "2"`. Each becomes a pin of the net in the schematic.
- A pin that has no row but is joined inside the part to one that has (`.vout.neg` and `.vin.neg` above) is an alias and needs no pad. A pin with no row and no alias that is on a net is an error, once for the part.
- A pad belongs to one pin, unless the part joins the pins that share it.
- `optional` is for a `power` or `input` pin that may float; `required` is for an `output`, `bidirectional` or `passive` pin that may not. Neither is accepted where it changes nothing.
- `fn` lists what a pin can be. A connected pin with more than one function needs the board to choose: `.u.pa2.function &= UART1_TX`. There is no default function. One function cannot be chosen on two pins of a part, except a class (`fn GPIO class` in a row, "Functions"; or a function that a peripheral is `on`, "A peripheral on any pin"). A function may carry its own kind, `fn GPIO:bidirectional UART1_TX:output` (next sections).
- `tie .a ~ .b` says that the board must put the two pins on one net. The tie is skipped in a package that lacks either pin.
- The faults on a board are reported when it is checked. A rule that needs the package waits for the solver when the board does not name the package (a catalog row chooses it). The solver does not choose a catalog row whose package lacks a pin that the board connects.
- Two `power` pins that are both supplies on one net are not checked yet.

Messages.

A pin that must be connected and is not:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board EnLeftOpen {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vout.pos
}
```

```text
error: `u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net
  --> main.nsm:9:5
  |
9 |     .u: Ldo9 { .package &= "DFN-8-EP" }
  |     ^^^^^^^^
```

The message points at the line that declares the part, and names the pad and the datasheet name so you can find the pin on the datasheet.

A tie that the board does not make:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board SenseOnTheInput {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vin.pos
    .u.en ~ .u.vin.pos
}
```

```text
error: `u.sense` must be on the same net as `u.vout.pos` (`Ldo9` ties them), but `u.sense` is on net `VIN` with `input.pos`, `u.en`, `u.vin.pos`
   --> src/ldo9.nsm:31:5
   |
31 |     tie .sense ~ .vout.pos
   |     ^^^^^^^^^^^^^^^^^^^^^^
```

A `no_connect` pin that has something on its net:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board NoConnectUsed {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vout.pos
    .u.en ~ .u.vin.pos
    .u.nc ~ .u.vout.pos
}
```

```text
error: `u.nc` is a no_connect pin (pad 5, `NC`) but it is on net `3V3` with `output.pos`, `u.sense`, `u.vout.pos`
  --> main.nsm:9:5
  |
9 |     .u: Ldo9 { .package &= "DFN-8-EP" }
  |     ^^^^^^^^
```

A pin that the package does not have, connected:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board SenseInTheSmallPackage {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: Ldo9 { .package &= "SOT-23-5" }
    .output: Rail {
        .pos.net &= "3V3"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vout.pos
    .u.en ~ .u.vin.pos
}
```

```text
error: `u.sense` (`SENSE`) does not exist in package `SOT-23-5` of `Ldo9`, but the board connects it; the packages that have it: `DFN-8-EP`
  --> main.nsm:9:5
  |
9 |     .u: Ldo9 { .package &= "SOT-23-5" }
  |     ^^^^^^^^
```

Leave the sense pin out and the same board is fine in the SOT-23-5: the tie is skipped there. A package is text, because a package's name has dashes and parentheses (`"SOT-23-5"`, `"VSON-10(2x3)"`) and a size is a number (`"0603"`), but it is checked (ADR 0118). In a package the part has no table for, the message is at the statement, lists the tables the part has and names the nearest:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board UnknownPackage {
    .input: Rail { .pos.net &= "VIN" }
    .u: Ldo9 { .package &= "SOT23-5" }
    .input ~ .u.vin
}
```

```text
error: `u` is a `Ldo9` in package `SOT23-5`, which has no pin table; its tables are for `DFN-8-EP`, `SOT-23-5`; did you mean "SOT-23-5"?
  --> main.nsm:6:16
  |
6 |     .u: Ldo9 { .package &= "SOT23-5" }
  |                ^^^^^^^^^^^^^^^^^^^^^
```

### Several packages, and the absolute limits in the row

A header may list several packages that have the same pads: they share the table (ADR 0124). The table is the table of each of them, as if it were written once for each.

A row may end with the pin's limits from the datasheet's Absolute Maximum Ratings: `abs_max 65V`, or `abs_min -0.5V abs_max 6.5V` when the datasheet gives both (`abs_min` first, then `abs_max`). They are the pin's `.abs_min` and `.abs_max` (see "Logic levels and absolute maximums"), and the solver holds the worst-case voltage of the pin's net against them. This is the only place a limit is written (ADR 0126): a statement of it, `.x.abs_max &= 65V` or `.x.abs_min &= -0.5V`, is an error that names the row, for a `Logic` line and a bus pin as for a plain pin. A voltage is measured from a ground, and the row says which:

- `abs_max 4V from .agnd` measures the limits from the pin named after `from`, a pin of the part; it ends the row, after `abs_min` and `abs_max`.
- With no `from`, a pin that is the `.line` of a `Logic`, or a pin of a bus interface with `.levels`, is measured from that interface's ground, the `neg` of its reference rail, as it always was; `from` is refused there.
- With no `from`, any other pin is measured from the part's ground: the one row of the table that is the `neg` of a rail (`.vin.neg`). A table with none, or with the grounds of several rails, needs `from`.

A value can be any expression a narrowing takes, `abs_max .vdd.voltage + 0.3V`. A limit in a generic's row is a contract of its providers (see Generics): a provider whose row states none has the generic's, and one that states its own must be at least as strict, `abs_max` no higher and `abs_min` no lower, or the row is an error that names the generic's limit. A pin that a part joins to nothing outside it is not judged, and a pin with only a capacitor on it is a `not evaluated` note, as for any absolute maximum.

```nsm module=src::en_ldo
import std::sources::Rail

/// A fixed-output LDO with an enable pin that survives 6 V, in two packages with the same
/// pads. Illustrative; not a real part.
part EnLdo {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current

    pins "SOT-23-5" "SOT-353" {
        .vin.pos   power  "IN"   "1"
        .vin.neg   power  "GND"  "2"
        .en        input  "EN"   "3" abs_max 6V
        .nc        no_connect "NC" "4"
        .vout.pos  power  "OUT"  "5"
    }

    .package: Package
}
```

A board in either package checks:

```nsm
import src::en_ldo::EnLdo
import std::sources::Rail

board EnBoard {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .u: EnLdo { .package &= "SOT-353" }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
```

A package named twice in a header:

```nsm fails
import std::sources::Rail

part Twice {
    .vin: Rail
    pins "SOT-23-5" "SOT-23-5" {
        .vin.pos  power "IN"  "1"
        .vin.neg  power "GND" "2"
    }
    .package: Package
}
```

```text
error: package "SOT-23-5" is listed twice in this `pins` header
  --> main.nsm:5:21
  |
5 |     pins "SOT-23-5" "SOT-23-5" {
  |                     ^^^^^^^^^^
```

A limit with no rail ground in its table to measure from:

```nsm fails
part NoGround {
    pins {
        .gnd  power "GND" "1"
        .en   input "EN"  "2" abs_max 6V
    }
}
```

```text
error: the limits of `.en` are measured from the part's ground, the `neg` of its rail, and this table has no rail's `neg` to be it; name the pin they are measured from, such as `abs_max 65V from .gnd`
  --> main.nsm:4:31
  |
4 |         .en   input "EN"  "2" abs_max 6V
  |                               ^^^^^^^
```

The statement form of a limit is refused, with the row that says it:

```nsm fails
import std::logic::Logic
import std::sources::Rail

part Statement {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.line.abs_max &= 3.6V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}
```

```text
error: a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.in.line` in the `pins` table of its part with `abs_max 3.6V`, such as `.in.line input "NAME" "3" abs_max 3.6V` (ADR 0126)
  --> main.nsm:8:5
  |
8 |     .in.line.abs_max &= 3.6V
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

### Functions

A pin with several functions is shared between peripherals, and the board decides which it is. The microcontroller here is also invented.

```nsm module=src::mcu
import std::sources::Rail

/// A microcontroller with a few of its pins. Illustrative; not a real part.
part Mcu32 {
    .vdd: Rail
    .vdd.voltage >= 1.7V
    .vdd.voltage <= 3.6V
    .vdd.current &= 1mA..25mA

    pins "QFN-32" {
        .vdd.pos power         "VDD"   "1" "17" "32"
        .vdd.neg power         "VSS"   "8" "24" "33"
        .nrst    input         "NRST"  "4"
        .pa2     bidirectional "PA2"   "12" fn GPIO UART1_TX SPI1_SCK
        .pb6     bidirectional "PB6"   "14" fn GPIO UART1_TX
        .boot0   input         "BOOT0" "31" optional
    }

    .package: Package
    catalog mcu by .mpn { .mpn, .package }
}
```

```nsm
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board Uart {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pa2.function &= UART1_TX
}
```

A function is written as the pin table writes it after `fn`, a bare name with no quotes (ADR 0118): `.u.pa2.function &= UART1_TX`. A function is a name of the part's pin table, not a variant of an enum, so it needs no import. A module passes a choice on with an equation of two functions (below), never with text. A quoted function is refused, and the message says what to write:

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board QuotedFunction {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pa2.function &= "UART1_TX"
}
```

```text
error: the function of `u.pa2` is a name from the pin table of its part, not text: write `UART1_TX`, without quotes
   --> main.nsm:16:24
   |
16 |     .u.pa2.function &= "UART1_TX"
   |                        ^^^^^^^^^^
```

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board NoFunction {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
}
```

```text
error: `u.pa2` has several functions (GPIO, UART1_TX, SPI1_SCK) and is connected: choose one with `.u.pa2.function &= GPIO`
   --> main.nsm:10:5
   |
10 |     .u: Mcu32 { .package &= "QFN-32" }
   |     ^^^^^^^^^
```

A function that the pin does not offer, a misspelt one (the nearest function of the part is named), and one function on two pins:

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board WrongFunction {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pa2.function &= I2C1_SDA
}
```

```text
error: `u.pa2` cannot be `I2C1_SDA`; its functions are GPIO, UART1_TX, SPI1_SCK
   --> main.nsm:15:5
   |
15 |     .u.pa2.function &= I2C1_SDA
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board MisspeltFunction {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pa2.function &= UART1TX
}
```

```text
error: `u.pa2` cannot be `UART1TX`; its functions are GPIO, UART1_TX, SPI1_SCK; did you mean `UART1_TX`?
   --> main.nsm:16:5
   |
16 |     .u.pa2.function &= UART1TX
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board OneFunctionTwice {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .load2: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pb6 ~ .load2.a
    .u.pa2.function &= UART1_TX
    .u.pb6.function &= UART1_TX
}
```

```text
error: function `UART1_TX` is chosen on both `u.pa2` and `u.pb6`
   --> main.nsm:18:5
   |
18 |     .u.pb6.function &= UART1_TX
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A function can be chosen on any number of pins when the part says it is a **class**: the row writes `class` after the function, `fn GPIO class`. A microcontroller whose ADC inputs are also plain GPIOs lists `GPIO` on every such pin, and a board that uses all of them as GPIO chooses `GPIO` on each (a class is also what `peripheral NAME on CLASS` names, "A peripheral on any pin"; the two spellings make a function a class in the same way). A connected pin that lists `GPIO` and an ADC input still needs one of them chosen, and a function no row marks stays on one pin. A `class` right after a function's name (and its `:kind`) is always that function's marker, so a function that is itself called `class` is a name only as the first function after `fn` or right after a function that already has its marker (`fn GPIO class class`).

```nsm module=src::mcu8
/// A microcontroller with two pins that are GPIO or an ADC input. Illustrative; not a real part.
part Mcu8 {
    pins {
        .gpio0 bidirectional "GPIO0"      "1" fn GPIO class
        .gpio1 bidirectional "GPIO1_ADC0" "2" fn GPIO class ADC0:input
        .gpio2 bidirectional "GPIO2_ADC1" "3" fn GPIO class ADC1:input
    }
}
```

```nsm
import src::mcu8::Mcu8
import std::parts::passive::Resistor

board GpioOnTwoPins {
    .u: Mcu8
    .a: Resistor
    .b: Resistor
    .u.gpio1 ~ .a.a
    .u.gpio2 ~ .b.a
    .u.gpio1.function &= GPIO
    .u.gpio2.function &= GPIO
}
```

The marker is a fact of the function, so every row that lists it carries it. A row that lists the function without it, a marker on a function that `peripheral NAME on CLASS` already makes a class, and a marker on a line of a peripheral (which is on one pin at a time) are refused in the part:

```nsm fails
part Mcu8b {
    pins {
        .gpio0 bidirectional "GPIO0" "1" fn GPIO class
        .gpio1 bidirectional "GPIO1" "2" fn GPIO
    }
}
```

```text
error: `.gpio1` lists `GPIO` without `class`, which `.gpio0` marks as a class; a class is a fact of the function, so write `fn GPIO class` on every row that lists it
  --> main.nsm:4:9
  |
4 |         .gpio1 bidirectional "GPIO1" "2" fn GPIO
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A module hides the parts inside it, so a board cannot write `.hub.mcu.pa2.function`. A module that wants its board to choose a function on a pin it exports passes the choice on with the equation `.mcu.pa2.function == .scl.function`, inside the module: the part's pin takes the function that the board states on the module's pin, and every check of the part applies to it as if the board had written it.

```nsm module=src::hub
import src::mcu::Mcu32
import std::sources::Rail

/// Invented: a microcontroller with one pin exported, and the choice of its function passed on.
module Hub {
    .supply: Rail
    .scl: Pin
    .mcu: Mcu32 { .package &= "QFN-32" }
    .supply ~ .mcu.vdd
    .mcu.nrst ~ .supply.pos
    .scl ~ .mcu.pa2
    .mcu.pa2.function == .scl.function
}

/// The same without the equation: nothing passes the choice of `.scl` on.
module HubWithoutEquation {
    .supply: Rail
    .scl: Pin
    .mcu: Mcu32 { .package &= "QFN-32" }
    .supply ~ .mcu.vdd
    .mcu.nrst ~ .supply.pos
    .scl ~ .mcu.pa2
}
```

```nsm
import src::hub::Hub
import std::parts::passive::Resistor
import std::sources::Rail

board Forwarded {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .hub: Hub
    .load: Resistor
    .supply ~ .hub.supply
    .hub.scl ~ .load.a
    .hub.scl.function &= UART1_TX
}
```

`.function &=` on a pin of a module that no such equation reads has no effect, and it is an error, with the equation to write in the module:

```nsm fails
import src::hub::HubWithoutEquation
import std::parts::passive::Resistor
import std::sources::Rail

board Ignored {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .hub: HubWithoutEquation
    .load: Resistor
    .supply ~ .hub.supply
    .hub.scl ~ .load.a
    .hub.scl.function &= UART1_TX
}
```

```text
error: `hub.mcu.pa2` has several functions (GPIO, UART1_TX, SPI1_SCK) and is connected: choose one with `.mcu.pa2.function &= GPIO`
   --> src/hub.nsm:19:5
   |
19 |     .mcu: Mcu32 { .package &= "QFN-32" }
   |     ^^^^^^^^^^^
error: `hub.scl.function` is stated, but `hub.scl` is a pin of module `HubWithoutEquation` and nothing passes it to a pin of a part, so it has no effect; inside `HubWithoutEquation`, write `.mcu.pa2.function == .scl.function`
   --> main.nsm:14:5
   |
14 |     .hub.scl.function &= UART1_TX
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The first message is the part's own pin, which still has several functions to choose from because nothing chose one. The second says where the choice went: nowhere. With one pin of a part on the module's pin, the message names it; with several, it says `.<part>.<pin>.function == .scl.function`. A pin that has no row in a part's pin table has no function to choose either, and the statement is refused for the same reason. A module in a module forwards the same way, one equation at each level (`.hub.scl.function == .scl.function` in the outer module).

### A kind for each function

A pin has one kind, and a function chosen for it may be a different direction: a pin that is a GPIO is bidirectional, as a UART transmitter it is an output, as a receiver an input. Write the kind of a function after its name with a colon and no spaces, as the datasheet gives it (ADR 0046; KiCad's alternate pin definitions each have their own electrical type, and this is the same idea). The pin's own kind stays the default for the pin and for every function written without a colon, and it is what the symbol and the other pin rules use. The net rules use the kind of the function the board chose: an `output` function is an output on its net, an `input` function an input. A function's kind is one of the pin kinds (`power`, `input`, `output`, `bidirectional`, `passive`, `no_connect`).

Do not guess the kind from the function's name. Write it only where the datasheet says which way the function goes; a function with no kind has the pin's.

```nsm
/// A microcontroller with one pin. Illustrative; not a real part.
part Mcu8 {
    pins {
        .pa2 bidirectional "PA2" "1" fn GPIO:bidirectional UART1_TX:output UART1_RX:input
    }
}

board Link {
    .a: Mcu8
    .b: Mcu8
    .a.pa2 ~ .b.pa2
    .a.pa2.function &= UART1_TX
    .b.pa2.function &= UART1_RX
}
```

A function that disagrees with how its net is used is refused, naming the part, the pin, the function and its kind. Two transmitters on one net are two outputs, and two receivers on one net have nothing to drive them:

```nsm fails
/// A microcontroller with one pin. Illustrative; not a real part.
part Mcu8 {
    pins {
        .pa2 bidirectional "PA2" "1" fn GPIO:bidirectional UART1_TX:output UART1_RX:input
    }
}

board TwoTransmitters {
    .a: Mcu8
    .b: Mcu8
    .a.pa2 ~ .b.pa2
    .a.pa2.function &= UART1_TX
    .b.pa2.function &= UART1_TX
}
```

```text
error: net `net-a.pa2` has two outputs: `a.pa2` (`Mcu8` function `UART1_TX`, an output), `b.pa2` (`Mcu8` function `UART1_TX`, an output)
  --> main.nsm:9:5
  |
9 |     .a: Mcu8
  |     ^^^^^^^^
```

```nsm fails
/// A microcontroller with one pin. Illustrative; not a real part.
part Mcu8 {
    pins {
        .pa2 bidirectional "PA2" "1" fn GPIO:bidirectional UART1_TX:output UART1_RX:input
    }
}

board TwoReceivers {
    .a: Mcu8
    .b: Mcu8
    .a.pa2 ~ .b.pa2
    .a.pa2.function &= UART1_RX
    .b.pa2.function &= UART1_RX
}
```

```text
error: net `net-a.pa2` has only inputs (`a.pa2` (`Mcu8` function `UART1_RX`, an input), `b.pa2` (`Mcu8` function `UART1_RX`, an input)): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor, or choose another function for a pin whose function makes it an input
  --> main.nsm:9:5
  |
9 |     .a: Mcu8
  |     ^^^^^^^^
```

The same pins with `GPIO` chosen on either side are a bidirectional pin on the net, and are accepted. A kind that is not one of the seven is a parse error: ``unknown pin kind `tristate`; the kinds are power, input, output, bidirectional, passive, no_connect and tri_state``.

### Peripherals that share hardware

A microcontroller's serial unit runs as a UART, an SPI master or an I2C master, one at a time. The pin table cannot say so: each pin lists the functions it can be, and nothing relates two pins' functions. Two statements in the part carry it (ADR 0071).

`peripheral NAME { FUNCTION[:kind] ... }` names the functions that belong together, a UART's `TX` and `RX`. The functions are separated by spaces or line ends, each with an optional kind as in a pin row. A peripheral is **in use** when the board chooses one of its functions on a pin, and two of its lines are still one use. A function takes its kind from the peripheral that lists it, so the pin rows write the function without a kind.

`shared NAME { PERIPHERAL ... }` says that at most one of its members is in use at a time. The name is the datasheet's name for the hardware, and it is what a message prints.

The pin table stays what it was: one row for each pin, with the functions the pin can be. Every function of a peripheral must be listed by at least one row, because a function that no pin offers can never be chosen. The part below is invented; its four pins list every function.

```nsm module=src::zmcu
/// Zmcu, an invented microcontroller: two serial units, each of which runs as a UART or an
/// I2C master, one at a time.
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    peripheral TWIM0 { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    peripheral UARTE1 { UARTE1_TX:output UARTE1_RX:input }
    peripheral TWIM1 { TWIM1_SCL:bidirectional TWIM1_SDA:bidirectional }

    shared SERIAL0 { UARTE0 TWIM0 }
    shared SERIAL1 { UARTE1 TWIM1 }

    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO UARTE0_TX UARTE0_RX TWIM0_SCL TWIM0_SDA UARTE1_TX UARTE1_RX TWIM1_SCL TWIM1_SDA
        .p01 bidirectional "P0.01" "2" fn GPIO UARTE0_TX UARTE0_RX TWIM0_SCL TWIM0_SDA UARTE1_TX UARTE1_RX TWIM1_SCL TWIM1_SDA
        .p02 bidirectional "P0.02" "3" fn GPIO UARTE0_TX UARTE0_RX TWIM0_SCL TWIM0_SDA UARTE1_TX UARTE1_RX TWIM1_SCL TWIM1_SDA
        .p03 bidirectional "P0.03" "4" fn GPIO UARTE0_TX UARTE0_RX TWIM0_SCL TWIM0_SDA UARTE1_TX UARTE1_RX TWIM1_SCL TWIM1_SDA
    }
}
```

A console on one unit and a sensor on the other, each with a function chosen on each pin as before:

```nsm
import src::zmcu::Zmcu
import std::parts::connector::Header1x4

board Sensors {
    .u: Zmcu
    .j: Header1x4
    .u.p00 ~ .j.p1
    .u.p01 ~ .j.p2
    .u.p02 ~ .j.p3
    .u.p03 ~ .j.p4
    .u.p00.function &= UARTE0_TX
    .u.p01.function &= UARTE0_RX
    .u.p02.function &= TWIM1_SCL
    .u.p03.function &= TWIM1_SDA
}
```

A third interface on a unit that is in use is refused. The message names the part, the shared resource, both uses with their functions and lines, and whether another unit is free; the error points at the second use, in path order. Here the sensor went on the unit the console uses:

```nsm fails
import src::zmcu::Zmcu
import std::parts::connector::Header1x4

board Crowded {
    .u: Zmcu
    .j: Header1x4
    .u.p00 ~ .j.p1
    .u.p01 ~ .j.p2
    .u.p02 ~ .j.p3
    .u.p03 ~ .j.p4
    .u.p00.function &= UARTE0_TX
    .u.p01.function &= UARTE0_RX
    .u.p02.function &= TWIM0_SCL
    .u.p03.function &= TWIM0_SDA
}
```

```text
error: `u` (`Zmcu`) uses UARTE0 and TWIM0, which share `SERIAL0` and run one at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:11) and on `u.p01` as UARTE0_RX (main.nsm:12), TWIM0 on `u.p02` as TWIM0_SCL (main.nsm:13) and on `u.p03` as TWIM0_SDA (main.nsm:14); the other shared resources of `Zmcu`: `SERIAL1` has nothing in use
   --> main.nsm:13:5
   |
13 |     .u.p02.function &= TWIM0_SCL
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A function counts as soon as it is chosen, also on a pin that nothing is connected to: the statement is deliberate. A function that no peripheral lists (`GPIO`) is never a use of one. The rule needs no package and no catalog row; it is judged for every `Zmcu` the board has, with its full path (`hub.mcu` for a part in a module), before the solver runs.

A part's author is told about a statement that is wrong, at that statement. A member that is not a peripheral, a function or a pin of the part names the nearest name:

```nsm fails
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    peripheral TWIM0 { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    shared SERIAL0 { UARTE0 TWIM9 }
    pins {
        .p00 bidirectional "P0.00" "1" fn UARTE0_TX UARTE0_RX TWIM0_SCL TWIM0_SDA
    }
}
```

```text
error: `TWIM9` in `shared SERIAL0` is not a peripheral, a function or a pin of `Zmcu`; did you mean `TWIM0`? the peripherals are UARTE0 and TWIM0
  --> main.nsm:4:29
  |
4 |     shared SERIAL0 { UARTE0 TWIM9 }
  |                             ^^^^^
```

A function that no row lists is on no pin:

```nsm fails
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "1" fn UARTE0_TX
    }
}
```

```text
error: `peripheral UARTE0` lists `UARTE0_RX`, but no row of any `pins` table of `Zmcu` lists it, so no pin can be it; list it as `fn UARTE0_RX` on the pins that carry it
  --> main.nsm:2:42
  |
2 |     peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
  |                                          ^^^^^^^^^
```

A row that writes a kind for a function of a peripheral is refused, because the peripheral gives it its kind:

```nsm fails
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "1" fn UARTE0_TX:output UARTE0_RX
    }
}
```

```text
error: `.p00` lists `UARTE0_TX:output`, a function of `peripheral UARTE0`, which gives it its kind; write `UARTE0_TX` without a kind
  --> main.nsm:4:9
  |
4 |         .p00 bidirectional "P0.00" "1" fn UARTE0_TX:output UARTE0_RX
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A function belongs to one peripheral, and a peripheral and a function do not share a name. Statement order does not matter: a `shared` may come before the peripherals it names and the rows that list their functions. A `peripheral` or a `shared` can only be in a part: a generic states what every provider has, and a peripheral is a fact of one part's datasheet, so a part that provides the generic states its own.

### A peripheral on any pin

On many microcontrollers any pin of a core can carry any line of a peripheral. Listing every such function on every row would make the table as long as the pins times the peripherals, so a peripheral says where it goes instead: `peripheral NAME on CLASS { ... }` offers its functions on every pin whose row lists the function `CLASS`. A class is an ordinary function: the rows list it (`fn APP_GPIO`), and it is also what a board chooses for a pin that is a plain GPIO of that core.

A core that owns a pin is carried the same way. Every pin lists the cores that may own it, a peripheral of a core is `on` that core's class, and a pin that the silicon gives to one core lists one class. A peripheral with no `on` is offered only on the pins whose rows list its functions by name: a dedicated pin. Its functions each need a row that lists them; a function of a peripheral that is `on` a class does not.

```nsm module=src::zmcu2
/// Zmcu2, an invented microcontroller: an application core and a network core, a UART and
/// an I2C master that share one serial unit, a UART of the network core, and a fast SPI on
/// dedicated pins.
part Zmcu2 {
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral TWIM0 on APP_GPIO { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    peripheral NETUART on NET_GPIO { NETUART_TX:output NETUART_RX:input }
    peripheral HSSPI { HSSPI_SCK:output HSSPI_MOSI:output }

    shared SERIAL0 { UARTE0 TWIM0 }

    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO NET_GPIO
        .p01 bidirectional "P0.01" "2" fn APP_GPIO NET_GPIO
        .p02 bidirectional "P0.02" "3" fn APP_GPIO
        .p03 bidirectional "P0.03" "4" fn NET_GPIO
        .p04 bidirectional "P0.04" "5" fn APP_GPIO NET_GPIO HSSPI_SCK
        .p05 bidirectional "P0.05" "6" fn APP_GPIO NET_GPIO HSSPI_MOSI
    }
}
```

One row for each pin, and no row grows with the peripherals. The application core's console on `.p02`, the network core's on `.p03` and a fast SPI on its dedicated pins:

```nsm
import src::zmcu2::Zmcu2
import std::parts::connector::Header1x8

board Cores {
    .u: Zmcu2
    .j: Header1x8
    .u.p00 ~ .j.p1
    .u.p02 ~ .j.p2
    .u.p03 ~ .j.p3
    .u.p04 ~ .j.p4
    .u.p05 ~ .j.p5
    .u.p00.function &= UARTE0_TX
    .u.p02.function &= UARTE0_RX
    .u.p03.function &= NETUART_TX
    .u.p04.function &= HSSPI_SCK
    .u.p05.function &= HSSPI_MOSI
}
```

A function on a pin that does not offer it says where it is offered, and what the pin offers. A peripheral on a class has too many functions to list on every pin, so the message names the peripherals. A dedicated function on the wrong pin, and a network-core line on a pin that the network core does not own:

```nsm fails
import src::zmcu2::Zmcu2
import std::parts::connector::Header1x8

board WrongPin {
    .u: Zmcu2
    .j: Header1x8
    .u.p01 ~ .j.p1
    .u.p01.function &= HSSPI_SCK
}
```

```text
error: `u.p01` cannot be `HSSPI_SCK`; that function is offered only on `u.p04`; `u.p01` offers APP_GPIO, NET_GPIO and the lines of the peripherals on those pins: UARTE0, TWIM0, NETUART
  --> main.nsm:8:5
  |
8 |     .u.p01.function &= HSSPI_SCK
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import src::zmcu2::Zmcu2
import std::parts::connector::Header1x8

board WrongCore {
    .u: Zmcu2
    .j: Header1x8
    .u.p02 ~ .j.p1
    .u.p02.function &= NETUART_TX
}
```

```text
error: `u.p02` cannot be `NETUART_TX`; that function is offered on every pin that lists `NET_GPIO`; `u.p02` offers APP_GPIO and the lines of the peripherals on those pins: UARTE0, TWIM0
  --> main.nsm:8:5
  |
8 |     .u.p02.function &= NETUART_TX
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A name that no pin offers is a slip, and the message names the nearest function (`did you mean`). A connected pin that lists only a class still has several functions, because its peripherals' lines are functions of it, and needs one chosen. Shared resources count the functions a class offers: a board that puts `UARTE0` and `TWIM0` on pins of this part gets the message of the previous section, though no row lists either.

A part's author is refused a class that no row lists, and a row that lists what its class already offers:

```nsm fails
part Zmcu2 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO NET_GPIO
    }
}
```

```text
error: `peripheral UARTE0 on GPIO`: no row of any `pins` table of `Zmcu2` lists the function `GPIO`, so UARTE0 is offered on no pin; the functions that rows list are APP_GPIO and NET_GPIO
  --> main.nsm:2:26
  |
2 |     peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
  |                          ^^^^
```

```nsm fails
part Zmcu2 {
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO UARTE0_TX
    }
}
```

```text
error: `.p00` lists `UARTE0_TX`, which `peripheral UARTE0` already offers on every pin that lists `APP_GPIO`, as `.p00` does; remove it from the row
  --> main.nsm:4:9
  |
4 |         .p00 bidirectional "P0.00" "1" fn APP_GPIO UARTE0_TX
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### A pin by capability

A board that does not care which pin of a microcontroller carries a signal asks for a capability and lets the tool pick the pin (ADR 0101). A request is a declaration whose type is a function of a part: `.status: .u.GPIO` declares the pin `status` of the board, and asks `u` for any pin that offers `GPIO`. What a request can ask for is what the part already says: a function that a row lists, or a function of a peripheral that is offered on the pin (`.tx: .u.UARTE1_TX`). The part declares nothing more. A request is a pin of the board, so `~` works on it as on any pin, and it is joined and checked as the pin it got.

The part and the board are invented. The pins `p02` and `p03` list `ANALOG`, so a request for `ANALOG` has two pins to choose from.

```nsm module=src::zmcu4
/// Zmcu4, an invented microcontroller: four pins, two of them analog, and two UARTs of which
/// one at a time is in use.
part Zmcu4 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral UARTE1 on GPIO { UARTE1_TX:output UARTE1_RX:input }
    shared UARTS limit 1 { UARTE0 UARTE1 }

    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO
        .p01 bidirectional "P0.01" "4" fn GPIO
        .p02 bidirectional "P0.02" "5" fn GPIO ANALOG
        .p03 bidirectional "P0.03" "6" fn GPIO ANALOG
    }
}
```

```nsm
import src::zmcu4::Zmcu4
import std::parts::passive::Resistor

board Sensors {
    .u: Zmcu4
    .load: Resistor
    .divider: Resistor
    .console: Resistor

    .status: .u.GPIO
    .sense: .u.ANALOG
    .tx: .u.UARTE1_TX

    .status ~ .load.a
    .sense ~ .divider.a
    .tx ~ .console.a
}
```

The tool gives each request a pin before the nets are made, so the schematic, the checks and the board see the pins as if the source had named them. It serves the requests in the order of their names and gives each the first pin that offers its capability in the order of the pins' names (byte order, so `p0_10` comes before `p0_2`; the order of the rows in the part never matters). Here `sense` gets `p02`, `status` gets `p00` and `tx` gets `p01`, with `UARTE1_TX` chosen on it. A pin that the source names (connected, or with a function chosen) is never given to a request.

`build` and `fab` write the answer in the pin file beside the source, `main.pins.toml` for `main.nsm`, when a run picks a pin:

```toml
version = 1

[Sensors.pins.sense]
pin = "p02"
function = "ANALOG"
source = "auto"

[Sensors.pins.status]
pin = "p00"
function = "GPIO"
source = "auto"

[Sensors.pins.tx]
pin = "p01"
function = "UARTE1_TX"
source = "auto"
```

The file has one table for each board and one entry for each request, keyed by the request's path (`hub.scl` for a request in a module `hub`, written `[Sensors.pins."hub.scl"]`). `pin` is the pin's path in the part, not its pad. The file is the same text every time for the same entries, so a run that changes nothing leaves its bytes alone, and the pin map does not move unless the design does:

- A run never changes an entry that still holds: the request exists and asks the same function, the pin offers it, nothing else holds the pin, and the part's `shared` limits still allow it. A request with no entry is served round the pins the entries hold, so a new request never moves another request's pin.
- `lock = true` on an entry says that the pin is a promise to firmware. A locked entry that no longer holds is an error at its line in the file. An unlocked one is a note, and its request is picked again.
- An entry whose request is gone is stale: a note when unlocked, and the file drops it; an error when locked, until it is deleted.
- `netsmith build --locked` and `netsmith fab --locked` write nothing and stop, listing what would change, as `cargo --locked` does. Run with `--locked` in continuous integration.

A request that cannot be served says which of its needs fails. A capability that nothing offers:

```nsm fails
import src::zmcu4::Zmcu4
import std::parts::passive::Resistor

board NoPwm {
    .u: Zmcu4
    .load: Resistor
    .dim: .u.PWM
    .dim ~ .load.a
}
```

```text
error: `dim` asks for PWM on `u` (`Zmcu4`), and no pin of `Zmcu4` offers it; the functions its rows and peripherals offer are GPIO, ANALOG, UARTE0_TX, UARTE0_RX, UARTE1_TX and UARTE1_RX
  --> main.nsm:7:14
  |
7 |     .dim: .u.PWM
  |              ^^^
```

Every pin that offers it taken, here by the source and by the request served before it (`vbat` comes before `vin`):

```nsm fails
import src::zmcu4::Zmcu4
import std::parts::passive::Resistor

board TooManyAnalog {
    .u: Zmcu4
    .a: Resistor
    .b: Resistor
    .c: Resistor
    .vin: .u.ANALOG
    .vbat: .u.ANALOG
    .u.p02 ~ .a.a
    .u.p02.function &= GPIO
    .vin ~ .b.a
    .vbat ~ .c.a
}
```

```text
error: `vin` asks for ANALOG on `u`, and every pin that offers it is taken: `u.p02` by the source (connected to `a.a`, function GPIO chosen at main.nsm:12), `u.p03` by `vbat` (new)
  --> main.nsm:9:5
  |
9 |     .vin: .u.ANALOG
  |     ^^^^^^^^^^^^^^^
```

A request for a function of a peripheral that a `shared` resource already gives to another unit:

```nsm fails
import src::zmcu4::Zmcu4
import std::parts::passive::Resistor

board TwoUarts {
    .u: Zmcu4
    .a: Resistor
    .b: Resistor
    .console: .u.UARTE0_TX
    .modem: .u.UARTE1_TX
    .console ~ .a.a
    .modem ~ .b.a
}
```

```text
error: `modem` asks for UARTE1_TX on `u`, and no free pin can serve it: `UARTS` allows 1 of UARTE0 and UARTE1 at a time: UARTE0 is used on `u.p00` (`console`) as UARTE0_TX; UARTE1 is used on `u.p01` (this request) as UARTE1_TX
  --> main.nsm:9:5
  |
9 |     .modem: .u.UARTE1_TX
  |     ^^^^^^^^^^^^^^^^^^^^
```

The rules about a pin run on the result as they do on a pin the source names: a function that disagrees with its net is the usual "two outputs" error, once, at the request. A class (`GPIO` above, a function that rows list and a peripheral is `on`) can be chosen on any number of pins of a part.

Several requests may ask for a function that rows list on more than one pin (`ANALOG` above), and each gets a pin of its own among the pins that offer it, in the same order as for any request:

```nsm
import src::zmcu4::Zmcu4
import std::parts::passive::Resistor

board TwoSensors {
    .u: Zmcu4
    .a: Resistor
    .b: Resistor
    .temperature: .u.ANALOG
    .battery: .u.ANALOG
    .temperature ~ .a.a
    .battery ~ .b.a
}
```

Here `battery` gets `p02` and `temperature` gets `p03`, and the pin file has an entry for each with `function = "ANALOG"`. The optimizer and `--pins locked` treat them as any two requests: each stays on a pin that offers `ANALOG`, and two never share a pin. When more requests ask for the function than pins offer it, the last one in the order of the names gets the "every pin that offers it is taken" error above. Two things stay refused: two requests for a function that only one pin offers (the second has no pin left, with the same error), and two pins that the source itself gives the same function by hand (`function ANALOG is chosen on both ...`). A line of a peripheral (`UARTE1_TX`) is also on one pin at a time, though every pin of its class could carry it, so two requests for it are refused with that same message.

A request asks a part declared in its own board or module, and a part with several packages needs its package stated (`.u.package &= "QFN-32"`), as the pins differ between packages. Not built yet: a request for the end of a bus (`.dbg: .u.Uart`).

The editor shows the pin map of the selected part in the schematic's inspector (_Pin map_ section): each request with the pin it got, the pad of that pin, the function, a mark when the entry is locked, and where the pin came from (`file, hand`, `file, auto`, or `picked now` when the pin file has no entry for it yet). A request that got no pin is listed with the message the check gives, which is also in the problems list. The editor assigns in memory as it does on every analysis and reads `<stem>.pins.toml`. It writes that file in two ways and no other: the padlock of a request locks or unlocks its pin (the entry's `lock` line, written only when true, as a run writes it), and a move, by dragging the request's pin onto another pin that offers its function or by choosing the pin from the list that the row's _Move_ button opens, puts the request there (the entry's `pin` line, and `source = "hand"`, because a person chose it). Either writes the lines of that entry in place and leaves every other line, comment and blank line of the file as it was; a request the file has no entry for gets one, after the last entry of its board. The editor refuses, with the reason in a message and without writing, a move to a pin the part does not have, one that does not offer the function, one that another request or the source holds, one that would break a `shared` limit, and a move of a locked request. It never writes the pick lock (`netsmith lock --update` does), and it writes nothing when the source does not build.

#### The pin map for firmware

`netsmith fab` writes the pin map for the firmware author once the layout is final, into the board's `fab/<board>/` folder next to the other files of the package, for each board that has pin requests: a C header, `<board>.pins.h`, and a JSON map, `<board>.pins.json`. Each signal is named by its request: `.status: .u.GPIO` is the signal `status`. They are made from the pins the board was routed with (after any rounds of `--pins auto` that moved a pin: the files are those of the board files that are in `fab/`) and the part's pin table, with no date, so the same pins give the same bytes. The pins are provisional at `build`, because `--pins auto` may move them at `fab`, so `build` writes no pin map and never removes one that `fab` wrote. A board with no request writes neither file, and `fab` removes the two an earlier `fab` left in that board's folder. A board `fab` writes as INCOMPLETE (the router left connections) has them too, with the pins that board file was routed with (the board is written anyway): that package is not for ordering, and a later run may still move the pins. `--pins locked` and `--locked` write them too, from the pin file as it is. A board placed by a `system` has none yet, as its requests are the system's.

For the board above, `Sensors.pins.h` is:

```text
// Generated by netsmith from main.nsm. Do not edit: run `netsmith fab` again.
// The pins of the requests of board Sensors.
#ifndef SENSORS_PINS_H
#define SENSORS_PINS_H

// sense: pin P0.02, pad 5 of u, function ANALOG
#define SENSORS_PIN_SENSE_NAME "P0.02"
#define SENSORS_PIN_SENSE_FUNCTION "ANALOG"

// status: pin P0.00, pad 3 of u, function GPIO
#define SENSORS_PIN_STATUS_NAME "P0.00"
#define SENSORS_PIN_STATUS_FUNCTION "GPIO"

// tx: pin P0.01, pad 4 of u, function UARTE1_TX
#define SENSORS_PIN_TX_NAME "P0.01"
#define SENSORS_PIN_TX_FUNCTION "UARTE1_TX"

#endif // SENSORS_PINS_H
```

and `Sensors.pins.json`:

```text
{
  "board": "Sensors",
  "signals": {
    "sense": {
      "function": "ANALOG",
      "locked": false,
      "pad": "5",
      "part": "u",
      "pin": "P0.02"
    },
    "status": {
      "function": "GPIO",
      "locked": false,
      "pad": "3",
      "part": "u",
      "pin": "P0.00"
    },
    "tx": {
      "function": "UARTE1_TX",
      "locked": false,
      "pad": "4",
      "part": "u",
      "pin": "P0.01"
    }
  },
  "source": "main.nsm"
}
```

- **Names.** The macros are `<BOARD>_PIN_<SIGNAL>_NAME` (the pin as the part's pin table names it, a string: a name such as `P0.13` is no C token), `_FUNCTION` (the function the request asked for, a string) and `_NUMBER`. The board and the signal are upper-cased, and anything that is not a letter or a digit is `_` (`hub.scl` is `HUB_SCL`). Two requests that are the same signal after that are an error that names both, which `fab` gives with the other checks of the source before it places or writes anything.
- **`_NUMBER`** is written only when the pin's name is letters and then one number, `GPIO7` (7) or `PA2` (2). A name with no trailing number (`VDD`), with the number in two places (`P0.13`) or with something after it (`GPIO7_B`) has no `_NUMBER`: which number firmware counts by is for the firmware author, so netsmith never guesses.
- **Several parts.** The signal is the request's own name, which is its path in the board and so is unique (`hub.scl` for a request in a module `hub`), whatever part it asks. The header says the part in each comment, and the JSON in each signal's `part` (the part's path in the board).
- **The JSON.** Keys are in byte order and the file is indented by two spaces and ends with a newline. `pin` is the pin's name in the pin table and `pad` its pad number (several are joined by a comma; `null` when the part has no row for it), `function` is the function requested, and `locked` is whether the pin file's entry has `lock = true`, the promise to firmware.

#### When a board does not route

The first pin map is arbitrary with respect to routing, because pins are picked before anything is placed. When `fab` places a board and the router leaves connections, it may swap the pins of the requests and try again (ADR 0101). `--pins auto` is the default; `--pins locked` (or `pins = "locked"` under `[project]` in `netsmith.toml`, and the flag wins) moves nothing.

- **What may move.** A request whose entry is not `lock = true`, to another pin of the same part that offers its capability, or exchanging pins with another unlocked request when each pin offers what the other asks for. A pin that the source names, a locked entry, and a choice that breaks a `shared` limit of the part never move or happen. Only the `fn` lists and the request say which pins are interchangeable; a pin that differs in a way the row cannot say (5 V tolerance, noise) should state that as a function of its own (`fn GPIO LOW_NOISE`).
- **What decides.** On a copy of the placed board, each candidate puts the swapped pads on each other's nets and is scored by the ratsnest: how many pairs of lines of different nets cross, then the total length. The best change that lowers the score is made, and the search goes on from there until none does. The order of the candidates is fixed (requests and pins in the byte order of their paths) and a tie keeps the first, so the same board gives the same proposal.
- **The rounds.** Each round is a pass of the whole pipeline: the source is checked, solved and designed with the proposed map, then placed and routed again. A map is kept when the board routes, or when it leaves no more connections and no more crossings than the map before it; otherwise the earlier map stands and the board is made again with it, so that the files are its own. At most three rounds. The search and every route of the rounds together may take as long as the route timeout (`--route-timeout`), counted from the first failure: the search stops between two candidates when the time is spent, a route gets what is left after the search and the elaboration, and a round with no time left for its route is not made. When the last round was no better, the board is made again with the earlier map in what is left, or in 10 seconds (never more than the route timeout) when nothing is, so that the files are the pin file's.
- **What is written.** The pin file with the map that stands, only when a swap was made: a run that routes never changes the file. The first note of `fab-notes.txt` says that the pin map changed and lists each request's old and new pin, because a changed pin is a change of the firmware. `netsmith build` after it draws the schematic from the new map. A board that still does not route after the rounds is INCOMPLETE as before, with the best map written.
- **`--pins locked`.** Nothing is changed, the pin file included. The search runs once on the placed board, and the swaps that would help are written to `pins.proposed.toml` in the output folder, with the numbers in the error. The proposal was placed but not routed, so it is not known to route: copy it over the pin file and run again. Use it in continuous integration and after the firmware is built on the pin map. `--locked` (on `fab`) implies it: it is the flag that says the run changes no file the source's folder holds, so a board that does not route ends with the proposal in the output folder and an error, and the pin file is as it was, whatever `--pins` or the manifest say (`--locked --pins auto` is `--locked`).
- **Not done.** A board placed by a `system` is not optimized (its requests are in its own table and in the system's), `fab --order` never moves a pin, and a board that does not route for a reason no pin changes is reported as before.

### A peripheral that is an end of a bus

A processor whose SPI pins are any pin of a class has no `.spi: Spi` to state a role on: its master end is a peripheral, and a function names no bus. The peripheral says which bus it is an end of with one clause, and the lines that are its functions (ADR 0064):

```nsm-fragment
peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
    SPIM0_SCK:output  as sclk
    SPIM0_MOSI:output as mosi
    SPIM0_MISO:input  as miso
}
```

`as Spi SpiRole::Master` names the interface and the role (a bus with one role writes `as Uart`), and `as sclk` after a function names the line it is. A function that names no line (a data/command pin of a display) is not on the bus. The part is refused when the interface has no such role, when a line is not one the role has, when two functions are one line, and when the kind a function states disagrees with the line (`SPIM0_MISO:output as miso` for a master, whose `miso` is an input). The peripheral is the end of the bus (`u`, `SPIM0`), and a pin on which the board chose one of its functions is a pin of that line and role: the rules of the previous section read it as they read the pin of an interface, with the end named by the part and the peripheral. The board's `.u.p00.function &= SPIM0_SCK` is the statement that puts the pin on the bus, and a pin that the board leaves at `APP_GPIO` is a plain pin.

```nsm module=src::zmcubus
import std::bus::{Spi, SpiRole}
import std::sources::Rail

/// Zmcubus, an invented processor whose serial units reach any pin of the application core.
part Zmcubus {
    .vdd: Rail

    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK:output  as sclk
        SPIM0_MOSI:output as mosi
        SPIM0_MISO:input  as miso
    }

    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3" fn APP_GPIO
        .p01     bidirectional "P0.01" "4" fn APP_GPIO
        .p02     bidirectional "P0.02" "5" fn APP_GPIO
        .p03     bidirectional "P0.03" "6" fn APP_GPIO
    }
}
```

The board joins the peripheral to the other end as a bus, with `~` between the peripheral and an interface of its bus (ADR 0129). The peripheral is named by its part and its name, `.u.SPIM0`, as a pin request names a function. Each line of the peripheral is the pin on which the board chose its function, so the functions say which pins, and the join says which bus: `.u.SPIM0 ~ .flash.spi` joins `u.p00` to `flash.spi.sclk`, `u.p01` to `mosi` and `u.p02` to `miso`, the nets three `~` lines would make, with their names. A `Uart`'s join crosses `tx` and `rx`, as `~` between two `Uart`s does. The master joined to the flash of the previous section builds, with the select on a plain pin of the processor:

```nsm evaluate
import src::zmcubus::Zmcubus
import src::zparts::Zflash
import std::sources::Rail

board AnyPin {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Zmcubus
    .flash: Zflash
    .supply ~ .u.vdd
    .supply ~ .flash.vdd

    .u.p00.function &= SPIM0_SCK
    .u.p01.function &= SPIM0_MOSI
    .u.p02.function &= SPIM0_MISO
    .u.SPIM0 ~ .flash.spi
    .u.p03.function &= APP_GPIO
    .u.p03 ~ .flash.spi.cs
}
```

The pins are joined after every statement is read, so the functions may come after the join. The join chooses no function: a function offered on several pins would have no answer, and the board, or the pin file, says which pin. A line of the peripheral whose function no pin has chosen is an error at the join, with the pins that offer it, unless the role's `lines` makes the line `optional`: a master's `cs` then joins nothing, and a select that reaches the slave through a link is the board's to join. A line that only the interface has (the `rts` and `cts` of a `Uart`, against a peripheral of `tx` and `rx`) joins nothing. The other side is an interface of the peripheral's bus or another such peripheral; a pin, a part, an interface of another kind and a peripheral that names no bus are refused. A peripheral has no values, so the join equates none: the clock is stated on the interface at the other end. A peripheral is no member of its part, so a chain or `~>` with one, a rename map and `except` are refused.

```nsm fails
import src::zmcubus::Zmcubus
import src::zparts::Zflash
import std::sources::Rail

board NoMosi {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Zmcubus
    .flash: Zflash
    .supply ~ .u.vdd
    .supply ~ .flash.vdd

    .u.p00.function &= SPIM0_SCK
    .u.p02.function &= SPIM0_MISO
    .u.SPIM0 ~ .flash.spi
    .u.p03.function &= APP_GPIO
    .u.p03 ~ .flash.spi.cs
}
```

```text
error: `.u.SPIM0 ~ .flash.spi` joins the line `mosi` of `SPIM0`, but no pin of `u` is `SPIM0_MOSI`: choose it on a pin that offers it and has no function chosen, `u.p01` (`.u.p01.function &= SPIM0_MOSI`)
   --> main.nsm:17:5
   |
17 |     .u.SPIM0 ~ .flash.spi
   |     ^^^^^^^^^^^^^^^^^^^^^
error: `flash.spi.mosi` is an input that must be connected (pad 5, `DI`), but nothing else is on its net
   --> main.nsm:11:5
   |
11 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
```

A board that joins the pins by hand is judged the same way. With the pins of `mosi` and `miso` joined to the wrong lines of the flash, the rules find it, naming the pins:

```nsm fails
import src::zmcubus::Zmcubus
import src::zparts::Zflash
import std::sources::Rail

board Crossed {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Zmcubus
    .flash: Zflash
    .supply ~ .u.vdd
    .supply ~ .flash.vdd

    .u.p00.function &= SPIM0_SCK
    .u.p01.function &= SPIM0_MOSI
    .u.p02.function &= SPIM0_MISO
    .u.p03.function &= APP_GPIO
    .u.p00 ~ .flash.spi.sclk
    .u.p01 ~ .flash.spi.miso
    .u.p02 ~ .flash.spi.mosi
    .u.p03 ~ .flash.spi.cs
}
```

```text
error: net `net-flash.spi.miso` joins two different lines of a `Spi`: `miso` (`flash.spi.miso`) and `mosi` (`u.p01`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `u.p01` is on the net of `miso`
   --> main.nsm:11:5
   |
11 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
error: net `net-flash.spi.mosi` joins two different lines of a `Spi`: `miso` (`u.p02`) and `mosi` (`flash.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `flash.spi.mosi` is on the net of `miso`
   --> main.nsm:11:5
   |
11 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
```

A function of a peripheral that disagrees with its line is refused in the part:

```nsm fails
import std::bus::{Spi, SpiRole}

part Zmcubus {
    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK:output as sclk
        SPIM0_MISO:output as miso
    }
    pins {
        .p00 bidirectional "P0.00" "3" fn APP_GPIO
    }
}
```

```text
error: `SPIM0_MISO` is an output in `peripheral SPIM0`, but `SpiRole::Master` makes `miso` an input of `Spi`
  --> main.nsm:6:20
  |
6 |         SPIM0_MISO:output as miso
  |                    ^^^^^^
```

### Counts, functions and pins that share

`shared NAME limit N { MEMBER ... }` allows at most N of its members at once; without `limit` it allows one. A limit is a whole number of at least 1 and fewer than the number of members. A member is a peripheral, a function or a pin:

- a **peripheral** is in use as before;
- a **function** is in use when it is chosen on a pin (`shared CLOCK { TWIM0_SCL SPIM0_SCK }`: two functions of two peripherals that cannot both be used);
- a **pin**, written with its dot (`.p10`), is in use when the board chooses its function or connects it, which means that its net has a pin that is not the part's own, or the board names the net. "If you use pin X you cannot use pin Y" is about the copper.

A name may be a member of several shared resources, a name twice in one is an error, and so is a function that is a member besides its own peripheral. A function that two pins both chose is already an error of its own.

```nsm module=src::zmcu3
/// Zmcu3, an invented microcontroller: three UARTs of which only two run at once, and two pins
/// that share an analogue reference.
part Zmcu3 {
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral UARTE1 on APP_GPIO { UARTE1_TX:output UARTE1_RX:input }
    peripheral UARTE2 on APP_GPIO { UARTE2_TX:output UARTE2_RX:input }

    shared UARTS limit 2 { UARTE0 UARTE1 UARTE2 }
    shared REFERENCE { .p10 .p11 }

    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO
        .p01 bidirectional "P0.01" "2" fn APP_GPIO
        .p02 bidirectional "P0.02" "3" fn APP_GPIO
        .p10 bidirectional "P0.10" "4" fn APP_GPIO
        .p11 bidirectional "P0.11" "5" fn APP_GPIO
    }
}
```

Two UARTs and one of the reference pins is fine:

```nsm
import src::zmcu3::Zmcu3
import std::parts::connector::Header1x4

board TwoUarts {
    .u: Zmcu3
    .j: Header1x4
    .u.p00 ~ .j.p1
    .u.p01 ~ .j.p2
    .u.p10 ~ .j.p3
    .u.p00.function &= UARTE0_TX
    .u.p01.function &= UARTE1_TX
    .u.p10.function &= APP_GPIO
}
```

A third UART is more than the limit. The message counts the members in use, names each with its uses, and lists what the resource has; it points at the first use of the member that goes over:

```nsm fails
import src::zmcu3::Zmcu3
import std::parts::connector::Header1x4

board ThreeUarts {
    .u: Zmcu3
    .j: Header1x4
    .u.p00 ~ .j.p1
    .u.p01 ~ .j.p2
    .u.p02 ~ .j.p3
    .u.p00.function &= UARTE0_TX
    .u.p01.function &= UARTE1_TX
    .u.p02.function &= UARTE2_TX
}
```

```text
error: `u` (`Zmcu3`) uses 3 of the peripherals of `UARTS`, which allows 2 at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:10), UARTE1 on `u.p01` as UARTE1_TX (main.nsm:11), UARTE2 on `u.p02` as UARTE2_TX (main.nsm:12); `UARTS` lists UARTE0, UARTE1 and UARTE2
   --> main.nsm:12:5
   |
12 |     .u.p02.function &= UARTE2_TX
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Two pins that cannot both be used, each named with how it is in use:

```nsm fails
import src::zmcu3::Zmcu3
import std::parts::connector::Header1x4

board TwoReferences {
    .u: Zmcu3
    .j: Header1x4
    .u.p10 ~ .j.p1
    .u.p11 ~ .j.p2
    .u.p10.function &= APP_GPIO
    .u.p11.function &= UARTE0_TX
}
```

```text
error: `u.p10` and `u.p11` of `Zmcu3` are both in use, but they share `REFERENCE`, which allows 1 at a time: `u.p10` as APP_GPIO (main.nsm:9) and `u.p11` as UARTE0_TX (main.nsm:10)
   --> main.nsm:10:5
   |
10 |     .u.p11.function &= UARTE0_TX
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A limit of 0, a fraction or a number with a unit is a parse error; a limit that is not fewer than the members is refused where it is written:

```nsm fails
part Zmcu3 {
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output }
    peripheral UARTE1 on APP_GPIO { UARTE1_TX:output }
    shared UARTS limit 2 { UARTE0 UARTE1 }
    pins {
        .p00 bidirectional "P0.00" "1" fn APP_GPIO
    }
}
```

```text
error: `shared UARTS limit 2` allows every one of its 2 members at once, so it states no limit at all; a limit is at least 1 and fewer than the number of members: lower it, or remove it for one at a time
  --> main.nsm:4:24
  |
4 |     shared UARTS limit 2 { UARTE0 UARTE1 }
  |                        ^
```

### Outputs

Two outputs on one net would fight, so the checker refuses them. An output and a bidirectional pin may share a net.

```nsm fails
part Driver {
    pins {
        .out output "OUT" "1"
        .gnd passive "GND" "2"
    }
}

board Fight {
    .a: Driver
    .b: Driver
    .a.out ~ .b.out
}
```

```text
error: net `net-a.out` has two outputs: `a.out`, `b.out`
  --> main.nsm:9:5
  |
9 |     .a: Driver
  |     ^^^^^^^^^^
```

### A line that several parts drive

A line that two parts drive can work if every driver lets go of it while it is not selected: the data out of several SPI slaves on one `miso`, each one selected by its own chip select. The kind for that pin is `tri_state` (ADR 0064). A net whose drivers are all `tri_state` is accepted, and that at most one of them is selected at a time is a fact of the protocol, which the select lines and the firmware carry; the checker says no more. A `bidirectional` pin is never counted as a driver, as before.

A part writes `tri_state` only when its datasheet says the pin is high impedance while the part is not selected, with the page in a comment, as a rail's range and a level are written (see _A part states its own requirements_ in `docs/DESIGN-RULES.md`). A datasheet that draws the pin as an output and says no more gives `output`, which says exactly what is known, and a board that shares that line is refused:

```nsm
/// Zrom, an invented serial memory.
part Zrom {
    pins {
        // Page 8: DO is high impedance while /CS is high.
        .do  tri_state "DO"  "1"
        .cs  input     "/CS" "2"
        .gnd passive   "GND" "3"
    }
}

/// Zhost, an invented controller that reads the data line and selects each memory.
part Zhost {
    pins {
        .din input   "DIN" "1"
        .cs0 output  "CS0" "2"
        .cs1 output  "CS1" "3"
        .gnd passive "GND" "4"
    }
}

board Shared {
    .a: Zrom
    .b: Zrom
    .host: Zhost
    .a.do ~ .b.do
    .a.do ~ .host.din
    .a.cs ~ .host.cs0
    .b.cs ~ .host.cs1
}
```

Two `tri_state` outputs and a receiver on one net build. A part whose datasheet is silent writes `output`, and the board that shares its line is refused with what to read:

```nsm fails
part Zrom {
    pins {
        // Page 8: DO is high impedance while /CS is high.
        .do  tri_state "DO"  "1"
        .cs  input     "/CS" "2" optional
    }
}

part Zgyro {
    pins {
        // The datasheet draws SDO as an output and says nothing about CS.
        .do  output "SDO" "1"
        .cs  input  "CS"  "2" optional
    }
}

board Silent {
    .a: Zrom
    .b: Zgyro
    .a.do ~ .b.do
}
```

```text
error: net `net-a.do` has 2 drivers and `b.do` is not `tri_state`: `a.do` (`Zrom`, a `tri_state` output) and `b.do` (`Zgyro`, an output); it would drive the net while another driver on it is selected; the datasheet of `Zgyro` must say that the pin is high impedance while its part is not selected (then write `tri_state` for it, with the page), or give `b.do` a net of its own
   --> main.nsm:18:5
   |
18 |     .a: Zrom
   |     ^^^^^^^^
```

### Open-drain outputs

An open-drain output, or an open-collector one, only pulls its line low or lets it go: a comparator's output, a fault flag, an interrupt line. The kind for it is `open_drain` (#460). Any number of `open_drain` pins may share a line, since none of them drives it high, and a `bidirectional` pin beside them is accepted. Two rules apply to the net:

- A pin that drives the line high, an `output` or a `tri_state` output, is refused on it: it would drive the line high while an open-drain pin pulls it low.
- The line needs a pull-up: a resistor from the net to a net that holds a rail's `pos`, or a pin that can drive it high (`output`, `tri_state` or `bidirectional`, such as a GPIO behind a series resistor). A resistor to the ground is a pull-down, not a pull-up.

The pull-up sets the high of the line. The levels check (see _An open-drain line_) takes the high of an `open_drain` pin from its pull-up, whatever the pin states, and the absolute-maximum check holds the pin to the voltage of that pull-up, not to its own supply. A part writes `open_drain` when its datasheet says the output is open drain or open collector, with the page in a comment.

```nsm
import std::parts::passive::Resistor
import std::sources::Rail

/// Zcmp, an invented comparator.
part Zcmp {
    pins {
        // Page 4: OUT is an open-drain output.
        .out open_drain "OUT" "1"
    }
}

/// Zreader, an invented part that reads the line.
part Zreader {
    pins {
        .din input "DIN" "1"
    }
}

board WiredOr {
    .v3: Rail
    .a: Zcmp
    .b: Zcmp
    .r: Resistor
    .s: Zreader
    .a.out ~ .b.out
    .a.out ~ .s.din
    .v3.pos ~> .r ~> .a.out
}
```

The same line with no resistor to the rail, and with a push-pull output on it:

```nsm fails
part Zcmp {
    pins {
        .out open_drain "OUT" "1"
    }
}

part Zdrv {
    pins {
        .out output "OUT" "1"
    }
}

board Fight {
    .a: Zcmp
    .d: Zdrv
    .a.out ~ .d.out
}
```

```text
error: net `net-a.out` has an open_drain output (`a.out`) and a push-pull driver: `d.out` (`Zdrv`, an output); it drives the line high while an open_drain pin pulls it low; give `d.out` a net of its own, or write `open_drain` for it if the datasheet says the pin is open drain
   --> main.nsm:14:5
   |
14 |     .a: Zcmp
   |     ^^^^^^^^
error: net `net-a.out` has an open_drain output (`a.out`) and no pull-up: nothing takes the line high when it lets go of it; join a resistor from the net to a rail's `pos`, or to a pin that drives the line high
   --> main.nsm:14:5
   |
14 |     .a: Zcmp
   |     ^^^^^^^^
```

### A pin tied to a rail

A pin may be tied to a rail: a GPIO strapped to ground, a configuration pin to the supply, a spare input to either. Any kind but `output` may be, with no statement for it. A push-pull `output` on the net of a rail, its positive or its negative pin, drives the rail to its other level whenever it drives, which is a short, and is refused (#461). An output that is itself a pin of a rail (the VBUS a hub's table writes as `output`) is a supply the part gives, and is not this case.

```nsm fails
import std::sources::Rail

part Zdrv {
    pins {
        .out output "OUT" "1"
    }
}

board Tied {
    .gnd: Rail
    .d: Zdrv
    .d.out ~ .gnd.neg
}
```

```text
error: `d.out` (`Zdrv`) is an output on net `GND`, the net of rail `gnd` (`gnd.neg`): a push-pull output drives the rail to its other level, which is a short; give it a net of its own, or, if its datasheet says the pin may be tied, write the kind it says (`bidirectional`, `open_drain` or `passive`)
   --> main.nsm:11:5
   |
11 |     .d: Zdrv
   |     ^^^^^^^^
```

The schematic's project file (`<board>.kicad_pro` beside the `.kicad_sch` that `build` writes) holds an ERC pin map that says the same as these rules: KiCad's default map, but a bidirectional or tri-state pin on a net with a power output, and a tri-state pin on a net with a power input, are not findings. So KiCad's electrical rules check of the exported schematic stays quiet about a GPIO tied to ground, where KiCad's own default reports `pin_to_pin` against the power flag on the ground net. The map is never stricter than KiCad's: what netsmith refuses, it reports itself.

### Pads

Pads are checked after the solver has chosen the package, because they depend on it. Two pins on one pad are an error unless the part joins them, which makes them one pin:

```nsm fails
part Shared {
    pins {
        .a passive "A" "1"
        .b passive "B" "1"
    }
}

board SamePad {
    .p: Shared
    .q: Shared
    .p.a ~ .q.a
}
```

```text
error: pad 1 of `Shared` is given to both `a` and `b`; two pins share a pad only if the part joins them
  --> main.nsm:4:9
  |
4 |         .b passive "B" "1"
  |         ^^^^^^^^^^^^^^^^^^
```

A pin that is on a net and has no row, and is not joined to a pin that has one, is reported once for its part:

```nsm fails
part NoPad {
    pins {
        .a passive "A" "1"
    }
    .b: Pin
}

board Loose {
    .p: NoPad
    .q: NoPad
    .p.a ~ .q.a
    .p.b ~ .q.b
}
```

```text
error: `p` is a `NoPad` and its pins (`b`) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `NoPad`, which says which pad each pin is
  --> main.nsm:9:5
  |
9 |     .p: NoPad
  |     ^^^^^^^^^
error: `q` is a `NoPad` and its pins (`b`) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `NoPad`, which says which pad each pin is
   --> main.nsm:10:5
   |
10 |     .q: NoPad
   |     ^^^^^^^^^
```

The table is also checked against the KiCad symbol and footprint the part is exported with. `build` and `fab` stop when a pad of the table is not a pad of the footprint or a pin of the symbol, when a footprint pad or symbol pin belongs to no pin of the table, when the symbol names the pin otherwise than the table's datasheet name (after the `pin_names` of `kicad.toml`), or when the symbol's electrical type does not suit the pin's kind. Each message names the part and both sides, for example `pad 3 is the input pin `en` (`EN`) in the part, but pin 3 of the symbol `tiny:Chip` is named `ENABLE``. This is a fact about your libraries and not about the circuit, so `check` does not report it. A footprint that `kicad.toml` asks for with `gen:` (_A part of your own, with no KiCad installed_) is checked the same way, and the message adds the row, its line and the pad count.

### Mistakes in a table

A pin has one kind and one datasheet name in every package:

```nsm fails
part TwoAnswers {
    .package: Package
    pins "A" {
        .x input "X" "1"
    }
    pins "B" {
        .x output "X" "1"
    }
}

board Empty {
}
```

```text
error: `.x` is an output named `X` in `pins "B"` but an input named `X` in `pins "A"`; a pin has one kind, one datasheet name and one set of absolute limits in every package
  --> main.nsm:7:9
  |
7 |         .x output "X" "1"
  |         ^^^^^^^^^^^^^^^^^
```

A row that stops early, an unknown kind and a pad without quotes are caught by the parser, at the token that is wrong:

```nsm fails
part Short {
    pins {
        .a passive "A"
    }
}
```

```text
error: expected a pad of `.a` as a quoted string such as "3", found end of line
  --> main.nsm:3:23
  |
3 |         .a passive "A"
  |                       ^
```

```nsm fails
part UnknownKind {
    pins {
        .a tristate "A" "1"
    }
}
```

```text
error: unknown pin kind `tristate`; the kinds are power, input, output, bidirectional, passive, no_connect, tri_state and open_drain
  --> main.nsm:3:12
  |
3 |         .a tristate "A" "1"
  |            ^^^^^^^^
```

```nsm fails
part BarePad {
    pins {
        .a passive "A" 1
    }
}
```

```text
error: a pad is text: write "1" with quotes, not `1`
  --> main.nsm:3:24
  |
3 |         .a passive "A" 1
  |                        ^
```
