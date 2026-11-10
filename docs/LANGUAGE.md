# The netsmith language

netsmith reads `.nsm` files. A file describes boards: the parts on them, the nets that join the pins, and the requirements the parts must meet. netsmith checks the file, picks real parts that meet the requirements in the worst case, and writes files for KiCad and for a board house.

This guide has a quick start, then one section for each construct of the language. Each section gives what the construct means, its syntax, a complete example, its rules, and the message you get when you misuse it. The last section lists the standard library.

`netsmith init` copies this guide into a project as `docs/netsmith/LANGUAGE.md`. Vocabulary is in `GLOSSARY.md` and the reasons for the design are in `docs/adr/`, in netsmith's source repository.

## How to read the examples

Every block marked `nsm` in this guide is a complete file. The test `crates/netsmith/tests/guide.rs` checks each one, solves those marked `solve`, and compares every message with what the tool prints. A block marked `nsm-fragment` is a piece of a file and is not run.

To run an example, save it and use `netsmith build`:

```sh
netsmith build main.nsm --out out
```

`build` checks the file, solves it, and writes a KiCad schematic. In the netsmith source repository, `cargo run -p netsmith -- build main.nsm` does the same, and `cargo run -p netsmith-check --example check -- main.nsm` and `cargo run -p netsmith-solve --example solve -- main.nsm` print what the checker and the solver worked out.

## Quick start

`examples/led.nsm` is an LED on a 5 V supply with a resistor that sets its current. The solver picks the resistor.

```nsm solve report file=examples/led.nsm
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board LedExample {
    /// Made by JLCPCB's two-layer process, on a board 1.6 mm thick.
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }

    .led: Led {
        .current &= 5mA..15mA
    }

    .r1: Resistor {}

    /// The supply comes in through this header.
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2

    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance

    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
board LedExample
  D1   Led             KT-0603R        LED_SMD:LED_0603_1608Metric                                C2286 Basic
  J1   Header1x2       Conn_01x02      Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical hand-soldered
  R1   Resistor        330             Resistor_SMD:R_0402_1005Metric                             C25104 Basic
  net GND: D1.1, J1.2
  net VCC: J1.1, R1.1
  net net-led.anode: D1.2, R1.2
```

Read it from the top.

- `import` brings in definitions from the standard library.
- `board LedExample { ... }` defines one board.
- `.process: TwoLayer { ... }` gives the board a member named `process` of type `TwoLayer`. It says which factory makes the board. `.stackup.thickness &= 1.6mm` chooses a thickness.
- `.supply: Rail { ... }` is a member named `supply`, a `Rail`. `.voltage &= 5V +/- 5%` narrows its voltage to 4.75 V to 5.25 V. `.pos.net &= "VCC"` names the net on its positive pin.
- `.led: Led { .current &= 5mA..15mA }` is an LED that must carry 5 mA to 15 mA.
- `.r1: Resistor {}` is a resistor with nothing stated. The solver picks its value.
- `.supply.pos ~ .power_in.p1` puts two pins on one net.
- `.led.current == ...` is a relation. It ties the LED current to the resistor the solver will pick.
- `.supply.pos ~> .r1 ~> .led ~> .supply.neg` is a series chain: positive pin, resistor, LED, negative pin.

The report shows what the solver chose: a 330 ohm resistor and a red LED, both from JLCPCB's Basic library. The relation decides it. With the supply at 4.75 V to 5.25 V, a forward voltage of 1.8 V to 2.4 V and a 1% 330 ohm resistor, the current stays between 5 mA and 15 mA (about 7.05 mA to 10.56 mA, in fact). The resistor is a 0402 part: its 62.5 mW rating holds for the current it really carries (see Resistor power).

The rest of this guide explains each construct. Statement order never matters to what a design means, so you may write the statements of a block in any order. The one thing it shows in is the number of a part's designator (see Designators).

## Definitions: board, part, interface, module, system, panel

A definition is a named, reusable description. There are eight kinds.

| Keyword | Meaning |
|---|---|
| `board` | One physical board: a process, parts and modules. It is built and ordered. |
| `part` | A component with pins and quantities, such as a resistor. |
| `interface` | A typed set of pins and quantities that is connected as one, such as a `Rail`. |
| `module` | A reusable sub-circuit placed on a board or in another module. |
| `enum` | A type with a fixed list of named values (see Enums). |
| `generic` | What a board may rely on of any part or module that provides it: a part or module says `provides` it, and a board that uses the generic gets the first provider that works (see Generics and providers). |
| `system` | Several boards and the joins between their ports (see Systems). |
| `panel` | One board repeated in a grid on one fabrication panel (see Panels). |

Syntax:

```nsm-fragment
kind Name {
    ...members and statements...
}
```

A file holds any number of definitions. Names are case sensitive. By convention definitions are `CamelCase` and members are `snake_case`.

```nsm
import std::parts::passive::Resistor

/// A two-pin screw terminal.
part Terminal2 {
    pins {
        .p1 passive "P1" "1"
        .p2 passive "P2" "2"
    }
}

/// A signal line and the ground it is measured against.
interface Signal {
    .line: Pin
    .ground: Pin
    .level: Voltage
}

/// Holds a signal line low until something drives it.
module Pulldown {
    .input: Signal
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .input.line ~> .r ~> .input.ground
}

board Probe {
    .sig: Signal { .level &= 3.3V }
    .hold: Pulldown
    .sig ~ .hold.input
    .terminal: Terminal2
    .sig.line ~ .terminal.p1
    .sig.ground ~ .terminal.p2
}
```

Rules.

- A definition holds members (`.name: Type`) and statements (constraints, connections, `bridge`, `catalog`).
- A `part` is the only definition that may hold `bridge` and `catalog`.
- A board is placed only in a system or a panel, never in another board or a module.
- Today a part can be built into a schematic only if the exporter knows its name: `Resistor`, `Capacitor`, `Led`, `LinearRegulator` and `Header1x2`. Another part checks and solves, and `build` stops with the message below.

Misuse: a board inside a board.

```nsm fails
import std::sources::Rail

board Inner {
    .a: Rail
}

board Outer {
    .inner: Inner
}
```

```text
error: a board cannot be placed inside a board; boards are placed only in a system or a panel
  --> main.nsm:8:13
  |
8 |     .inner: Inner
  |             ^^^^^
```

Misuse: a part the exporter does not know.

```nsm fails
part Chip {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Uses {
    .u: Chip
}
```

```text
error: Uses: U1: there is no KiCad symbol for a `Chip` yet
```

## Members

A member is a named thing inside a definition: a pin, a value, a part, a module or an interface. You declare one with `.name: Type`. A declaration may have a block that sets members of the new thing.

```nsm
import std::parts::passive::Capacitor
import std::sources::Rail

board Members {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .bypass: Capacitor {
        .effective_capacitance >= 1uF
        .max_voltage >= supply.voltage * 2
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

Rules.

- `.name` is a member of the block you are writing in. A path continues with dots: `.supply.pos.net`.
- A name without the leading dot is looked up outward. `supply.voltage` inside the `.bypass` block finds the board's `supply`. The search goes through the enclosing blocks of the same definition, innermost first.
- At the top level of a definition a bare name finds nothing, because nothing encloses it. Write the dot.
- A bare name never leaves its definition. A module cannot see the board that uses it.
- A declaration needs a single name: `.name: Type`. A name is declared once per block.
- Statements end at a newline or at `;`.
- A name that is not a plain identifier is written in single quotes: `.'1'`.

Types you can declare: `Pin`; the quantities `Number`, `Voltage`, `Current`, `Resistance`, `Capacitance`, `Inductance`, `Frequency`, `Time`, `Length`, `Power`, `Charge`, `Temperature` (kelvin, `10K`; a rise is a difference, so it is kelvin too); the text types `String`, `Package`, `Color`; an `enum` that is in scope (Enums, below); and any definition you can name.

Misuse: a typo, a bare name where the dot belongs, and a name declared twice.

```nsm fails
import std::parts::passive::Resistor

board Typo {
    .r1: Resistor { .resitance &= 1kohm +/- 1% }
}
```

```text
error: `resitance` is not a member of this Resistor
  --> main.nsm:4:22
  |
4 |     .r1: Resistor { .resitance &= 1kohm +/- 1% }
  |                      ^^^^^^^^^
```

```nsm fails
import std::parts::passive::Resistor

board Bare {
    .r1: Resistor
    r1.resistance &= 1kohm +/- 1%
}
```

```text
error: `r1` is not defined in any enclosing block; write `.r1` for a member of this block
  --> main.nsm:5:5
  |
5 |     r1.resistance &= 1kohm +/- 1%
  |     ^^
```

```nsm fails
import std::parts::passive::Resistor

board Twice {
    .r: Resistor
    .r: Resistor
}
```

```text
error: `r` is already declared in this block
  --> main.nsm:5:5
  |
5 |     .r: Resistor
  |     ^^
```

## Values: units, ranges and tolerances

Every value is a quantity with a dimension, and it is always a range in the worst case. A single number is a range with equal ends.

Literals.

| Form | Meaning |
|---|---|
| `3.3V`, `10kohm`, `100nF`, `5mA` | A number and a unit, with an SI prefix. |
| `1.8V..2.2V` | A range, low end first. |
| `3.3V +/- 5%` | A value and a relative tolerance: 3.135 V to 3.465 V. |
| `100ohm +/- 5ohm` | A value and an absolute tolerance, in the value's unit. |
| `20ppm` | A relative fraction, like `%`. |
| `"VCC"` | Text. |

Units: `m`, `g`, `s`, `A`, `K`, `V`, `ohm` (or `Ω`), `W`, `F`, `H`, `Hz`, `S`, `C`. Prefixes: `f p n u (or µ) m k M G T`. Lowercase `m` is milli and uppercase `M` is mega, so `1mohm` is a milliohm and `1Mohm` is a megohm. Arithmetic uses `+ - * /` and parentheses. A sum or difference needs the same dimension. A product or quotient makes a new one.

```nsm solve
import std::parts::passive::{Resistor, Capacitor}
import std::sources::Rail

board Units {
    .rail: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 4.7kohm +/- 1% }
    .r2: Resistor { .resistance &= 100ohm +/- 5ohm }
    .r3: Resistor { .resistance &= 900ohm..1.1kohm }
    .c1: Capacitor { .capacitance &= 100nF +/- 20% }
    .rail.pos ~> .r1 ~> .r2 ~> .r3 ~> .c1 ~> .rail.neg
}
```

Rules.

- A tolerance needs fixed numbers on both sides: `.x &= .y +/- 1%` is an error. A range may have a value at an end (next section).
- A value has a dimension and a field accepts only its own: a `Voltage` field takes volts.
- A divisor whose range includes zero is an error.

Misuse.

```nsm fails
import std::parts::passive::Resistor

board WrongUnit {
    .r1: Resistor { .resistance &= 5mA }
}
```

```text
error: `r1.resistance` holds Ω, but this value is A
  --> main.nsm:4:21
  |
4 |     .r1: Resistor { .resistance &= 5mA }
  |                     ^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::sources::Rail

board Mixed {
    .a: Rail { .voltage &= 3V + 2mA }
}
```

```text
error: cannot add V and A
  --> main.nsm:4:28
  |
4 |     .a: Rail { .voltage &= 3V + 2mA }
  |                            ^^^^^^^^
```

```nsm fails
import std::sources::Rail

board Unknown {
    .a: Rail { .voltage &= 3parsec }
}
```

```text
error: unknown unit `parsec`
  --> main.nsm:4:28
  |
4 |     .a: Rail { .voltage &= 3parsec }
  |                            ^^^^^^^
```

```nsm fails
import std::sources::Rail

board Backwards {
    .a: Rail { .voltage &= 3V..1V }
}
```

```text
error: the low end of the range is above the high end
  --> main.nsm:4:28
  |
4 |     .a: Rail { .voltage &= 3V..1V }
  |                            ^^^^^^
```

### A range whose end is a value

The ends of a range may be values, not only numbers (ADR 0066): a member, or an expression over members. The range `0A .. .max_supply_current` is the current from nothing up to the member `.max_supply_current`, whatever that turns out to be. It is the syntax that was always there with a name at an end, and nothing else is added. Write a space after `..` before a name that starts with a dot, so that the text is not read as three dots.

```nsm-fragment
part Gate {
    .vcc: Rail
    .max_supply_current: Current
    .vcc.current &= 0A .. .max_supply_current
}
```

The statement waits for its ends, as any statement that names a value does, and then it means what the same range with the numbers written in would mean. The range holds the lowest value of the low end and the highest of the high end, so an end that is itself a range (a value with a tolerance, a cell that holds a range) is never trusted to be narrower than it says. While an end has no value, the statement claims nothing: it narrows nothing, refuses nothing, and the build lists it as not evaluated, with the range as written, `gate.vcc.current (0A..gate.max_supply_current)`. A low end above the high end is an error, with the two ends and their values.

This is how a part states its own draw when the draw is a catalog column: the row's maximum is the range's high end, and the rail the part is on carries the range as soon as the row is picked (the translator below). Only a range has this: a tolerance, `.x +/- 5%`, still needs a number at both ends.

```nsm evaluate
import std::sources::Rail

/// Invented: a gate whose datasheet gives the largest current its supply pin draws.
part Gate {
    .designator &= "U"
    .vcc: Rail
    .max_supply_current: Current
    .vcc.current &= 0A .. .max_supply_current
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Counted {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= -1uA..0A
    }
    .gate: Gate { .max_supply_current &= 750nA }
    .supply ~ .gate.vcc
}
```

The gate draws 0 A to 750 nA, which fits the 1 uA that the supply is allowed to give (a supply gives, so its current is the draws negated). A supply that is allowed 500 nA does not, and the build says so:

```nsm fails
import std::sources::Rail

/// Invented: a gate whose datasheet gives the largest current its supply pin draws.
part Gate {
    .designator &= "U"
    .vcc: Rail
    .max_supply_current: Current
    .vcc.current &= 0A .. .max_supply_current
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Counted {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= -500nA..0A
    }
    .gate: Gate { .max_supply_current &= 750nA }
    .supply ~ .gate.vcc
}
```

```text
error: the currents on net `3V3` do not add up: `supply.current` must stay within -500nA..0A, but the others on it work out to -750nA..0A (all of them: `gate.vcc.current` is 0A..750nA, `supply.current` is -500nA..0A)
   --> main.nsm:20:9
   |
20 |         .current &= -500nA..0A
   |         ^^^^^^^^^^^^^^^^^^^^^^
```

## Enums: a value from a fixed list

An `enum` names a type whose values are a fixed list of variants (ADR 0054, proposed). It is declared at the top of a file, like a definition, and a variant is written `Enum::Variant`:

```nsm
import std::parts::connector::Header1x2
import std::sources::Rail

/// How a lamp is driven.
enum Drive {
    /// Always on.
    Steady,
    Dimmed,
}

module Lamp {
    .supply: Rail
    .drive: Drive
    .j: Header1x2
    .supply.pos ~ .j.p1
    .supply.neg ~ .j.p2
}

board Lamps {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp {
        .drive &= Drive::Dimmed
    }
    .input ~ .lamp.supply
}
```

- A member has an enum as its type: `.drive: Drive`. An enum that another file declares is imported like a definition (`import src::lamp::Drive`).
- A value is stated with `&=` or given as a default with `?=` (ADR 0051), and two statements of one field must name the same variant, or the usual empty-intersection error names both: ``contradiction on `lamp.drive`: it is already Drive::Dimmed, which does not overlap Drive::Steady``.
- `==` compares two values of one enum, a field and a variant or a field and a field, and is checked like any other relation: `.drive == Drive::Steady` fails when the field is `Drive::Dimmed`. Only `==` compares variants; they have no order.
- `&=`, `?=` and `==` take only a variant of the field's own enum. Text is not one: `.drive &= "Dimmed"` is refused and the message says what to write.
- A variant is written with its enum's name, so two enums may have a variant of one name, and a variant is never confused with a member or a net name. A file imports the enum, not its variants.
- `AssemblyMode` (variants `Fab`, `Hand`, `None` and `Consigned`) is the one enum that every file knows without an import, because every part has a built-in `.assembly` of that type (Who assembles a part, above). It is defined in `std/prelude.nsm`.

A variant the enum does not have is refused where it is written, with the variants listed and the nearest one named:

```nsm fails
import std::parts::passive::Resistor

board Solder {
    .r1: Resistor {
        .assembly &= AssemblyMode::Hnad
    }
}
```

```text
error: `AssemblyMode` has no variant `Hnad`; its variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`; did you mean `AssemblyMode::Hand`?
  --> main.nsm:5:36
  |
5 |         .assembly &= AssemblyMode::Hnad
  |                                    ^^^^
```

Text where an enum belongs is refused, and says which variant it was meant to be (this is what the quoted words of earlier versions, `.assembly &= "hand"`, now report):

```nsm fails
import std::parts::passive::Resistor

board Quoted {
    .r1: Resistor {
        .assembly &= "hand"
    }
}
```

```text
error: `r1.assembly` holds a variant of `AssemblyMode`, but this value is text; a variant of `AssemblyMode` is not written as text: write `AssemblyMode::Hand` (the variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`)
  --> main.nsm:5:9
  |
5 |         .assembly &= "hand"
  |         ^^^^^^^^^^^^^^^^^^^
```

## Narrowing and relations

Five operators put requirements on values. There is no plain `=`.

| Operator | Meaning |
|---|---|
| `&=` | Narrowing. Restrict a field to the values that its current range and the new constant, range, tolerance or expression all allow. |
| `?=` | A default. Give a field this value if no `&=` states the field (next section). |
| `==` | An equation between quantities. It must hold in the worst case. |
| `>=` and `<=` | A bound. It must hold for every value the two sides can take. |

A `&=` with a constant on the right narrows the field at once. A statement that mentions another field is a relation: the solver evaluates it once it has the numbers.

```nsm solve
import std::parts::passive::Resistor
import std::sources::Rail

board Divider {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .top: Resistor {}
    .bottom: Resistor { .resistance &= 10kohm +/- 1% }
    .out: Rail { .voltage &= 3.3V +/- 7% }
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .top.b ~ .out.pos
    .supply.neg ~ .out.neg
    .out.voltage == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
}
```

Here `.bottom.resistance` narrows at once. `.out.voltage == ...` waits for the solver, which picks `.top` so that the divider output always stays within 3.3 V +/- 7%.

Rules.

- Use `&=` to say what a field may be, and a constant on its right.
- Use `==` for an equation whose right side mentions other fields. When the left field has a range, the computed range must lie inside it. When it has none yet, it takes the computed one. When it has a range and this is the only equation on it, it then takes the computed range too, since the equation says what the value is: a window of current that a resistor's value fixes becomes the current that resistor lets through, and what depends on the current (a power) is worked out from that.
- Use `>=` and `<=` for limits: `.output_cap.max_voltage >= .vout.voltage * 1.5` holds only if the lowest rating is above the highest voltage times 1.5.
- Narrowing is an intersection. Two narrowings that do not overlap are a contradiction, whatever their order.
- A narrowed value cannot be loosened later. A library therefore states only what is always true, and a board narrows the rest. A value a board may want to change is offered with `?=` instead (next section).
- `=` is not an operator. `.x = 5V` and `.x = <expression>` are errors that name the two operators above.

### The ends of a range

`>=`, `<=` and `==` hold for every value in the ranges. Some rules are of the other kind, "can hold for some value", and the language says them with `min` and `max` (ADR 0098, proposed): `min(.f)` is the least value of the range of the field `.f`, and `max(.f)` the greatest, each as one number of the field's own kind. The comparison is then made on two numbers: `min(.a.voltage) <= max(.b.voltage)` holds when the lowest `a` can be is at most the highest `b` can be, which is so for a 4 V to 6 V rail and a 4.7 V to 5.5 V one, where `.a.voltage <= .b.voltage` is not.

```nsm solve
import std::sources::Rail

board Ends {
    .a: Rail {
        .voltage &= 4V..6V
        .pos.net &= "A"
        .neg.net &= "GND"
    }
    .b: Rail {
        .voltage &= 4.7V..5.5V
        .pos.net &= "B"
        .neg.net &= "GND"
    }
    min(.a.voltage) <= max(.b.voltage)
}
```

Rules.

- The argument is a field, written as a member path (`min(.bias)`, `max(.z.zener_voltage)`). It is not an expression (`min(.a + .b)`), a value (`max(5V)`) or text, and `min` and `max` take one argument: `min(.a, .b)` is not the smaller of two values, it is an error. The only functions are `min` and `max`; any other name is an error.
- They are read in a `<=` or `>=` statement, on either side and inside arithmetic (`min(.a.voltage) + 1V <= max(.b.voltage)`). A `==`, `&=` or `?=` is refused them: nothing is worked out from the end of a range, and no field takes a value from one.
- They never narrow. The solver reads the end of the field's range as it is when the check is made and gives nothing back: the ranges of the fields in a relation with `min` or `max` are the ranges they had. A relation of a part also decides which catalog rows are offered, as every relation of a part does (a row that fails it is not picked); that is a check on each candidate, not a narrowing.
- A field with no value yet has no end to read. The relation waits and is listed as not evaluated, like any relation with an unknown value, and is no error.
- An end is one number, so it carries no correlation with the field read whole elsewhere in the same statement: `max(.a.voltage) <= .a.voltage` is an error (6 V is not under 4 V), as `.a.voltage <= .a.voltage` is.
- A failed relation says what was compared: `min(c.voltage) must be at most max(b.voltage), but 12V is above 5.5V: no value of the two ranges meets it, so change one of them`.

A rail that is surely above the other, and the mistakes in the call:

```nsm fails
import std::sources::Rail

board Ends {
    .a: Rail {
        .voltage &= 12V
        .pos.net &= "A"
        .neg.net &= "GND"
    }
    .b: Rail {
        .voltage &= 4.7V..5.5V
        .pos.net &= "B"
        .neg.net &= "GND"
    }
    min(.a.voltage) <= max(.b.voltage)
}
```

```text
error: min(a.voltage) must be at most max(b.voltage), but 12V is above 5.5V: no value of the two ranges meets it, so change one of them
   --> main.nsm:14:5
   |
14 |     min(.a.voltage) <= max(.b.voltage)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::sources::Rail

board Ends {
    .a: Rail { .voltage &= 4V..6V }
    min(.a.voltage, .a.voltage) <= 5V
    min(.a.voltage + 1V) <= 5V
    .a.voltage == max(.a.voltage)
    abs(.a.voltage) <= 5V
}
```

```text
error: `min` takes one field, such as `min(.bias)`, but this has 2 arguments: it reads the least value of one field's range, and is not the smaller of several values
  --> main.nsm:5:5
  |
5 |     min(.a.voltage, .a.voltage) <= 5V
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: the argument of `min` is a field, such as `min(.bias)`, not `.a.voltage + 1V`: it reads one end of a field's range
  --> main.nsm:6:9
  |
6 |     min(.a.voltage + 1V) <= 5V
  |         ^^^^^^^^^^^^^^^
error: `max(.a.voltage)` can only be used in a `<=` or `>=` statement: it reads one end of a range, and nothing is worked out from it
  --> main.nsm:7:19
  |
7 |     .a.voltage == max(.a.voltage)
  |                   ^^^^^^^^^^^^^^^
error: `abs` is not a function; the functions are `min(.field)` and `max(.field)`, the least and the greatest value of a field's range
  --> main.nsm:8:5
  |
8 |     abs(.a.voltage) <= 5V
  |     ^^^
```

### Defaults a board can replace

A module or a part may give a field a value that a board can replace. It writes `.field ?= value`. `?=` is not a weaker `&=`: a narrowing only restricts, so a library that wants a value a board may change cannot say it with `&=`, and `&=` with a range the board then cannot leave would be a requirement. A default is an offer. The board accepts it by saying nothing, or replaces it by stating the field.

```nsm solve report
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with the resistor that sets its current.
module Lamp {
    .supply: Rail
    /// The current through the LED. A board that needs another states it with `&=`.
    .current: Current
    .current ?= 2mA..3mA
    .led: Led
    .r1: Resistor
    .led.current &= .current
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Dim {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp
    .input ~ .lamp.supply
}

board Bright {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp { .current &= 6mA..9mA }
    .input ~ .lamp.supply
}
```

```text
board Dim
  D1   Led             XL-1608SURC-06  LED_SMD:LED_0603_1608Metric                                C965799 Extended
  R1   Resistor        1.2k            Resistor_SMD:R_0603_1608Metric                             C22765 Basic
  net GND: D1.1
  net VCC: R1.1
  net net-lamp.led.anode: D1.2, R1.2
board Bright
  D1   Led             XL-1608SURC-06  LED_SMD:LED_0603_1608Metric                                C965799 Extended
  R1   Resistor        390             Resistor_SMD:R_0603_1608Metric                             C23151 Basic
  net GND: D1.1
  net VCC: R1.1
  net net-lamp.led.anode: D1.2, R1.2
```

`Dim` leaves `.lamp.current`, so the lamp draws the 2 mA to 3 mA that `Lamp` offers and the solver picks the 1.2 kohm resistor. `Bright` states `.lamp.current &= 6mA..9mA`, and the resistor is 390 ohm.

Rules.

- `.field ?= value` gives the field the value only if no other statement states the field. A field is stated when an `&=` statement on that same field exists anywhere that can state it: the board, a module around the field, or the part itself. Two `?=` on one field are not "stated": they are an error (below).
- A value that only reaches the field through a relation (`==`, `>=`, `<=`) or a join (`~`) does not state it. The default stays, and the solver then checks it against what reaches the field, as it checks any value. The example below shows what that costs.
- When a board states the field, the default is dropped. It is not intersected with the board's value, so the board's value need not overlap it: `Bright` asks for 6 mA to 9 mA and the default was 2 mA to 3 mA.
- The order of statements never matters. `.field &= v` before or after `.field ?= w` gives `v` either way.
- A field has at most one default. Two `?=` on one field, in the same module or one in a module and one in the board, are an error that names both lines, even when a `&=` would have replaced them both.
- The right side of `?=` is what the right side of `&=` can be: a number with a unit, a range, `x +/- y%`, text, or an expression over other fields. `?=` on a name that is not declared, or on a field of another type, is an error as it is for `&=`.

Misuse: two defaults.

```nsm fails
import std::sources::Rail

module Source {
    .supply: Rail
    .supply.voltage ?= 3.3V +/- 5%
}

board Twice {
    .source: Source
    .source.supply.voltage ?= 5V +/- 5%
}
```

```text
error: `source.supply.voltage` has 2 defaults, at main.nsm:5 and main.nsm:10: a field has one default; keep one `?=`
   --> main.nsm:10:5
   |
10 |     .source.supply.voltage ?= 5V +/- 5%
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Keep one default and write the other as a `&=` on the board. Misuse: a join is not a statement. The sensor below offers 3.3 V, the board joins its supply to a 5 V rail and states nothing on the sensor, so the sensor's supply is 3.3 V and 5 V at once:

```nsm fails
import std::sources::Rail

/// A sensor that runs from 3.3 V unless its board says otherwise.
module Sensor {
    .supply: Rail
    .supply.voltage ?= 3.3V +/- 5%
}

board Joined {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .sensor: Sensor
    .input ~ .sensor.supply
}
```

```text
error: connected values disagree: `input.voltage` is 4.75V..5.25V but `sensor.supply.voltage` is 3.135V..3.465V
   --> main.nsm:11:9
   |
11 |         .voltage &= 5V +/- 5%
   |         ^^^^^^^^^^^^^^^^^^^^^
```

State the voltage on the sensor's supply, `.sensor.supply.voltage &= 5V +/- 5%`, and the default is dropped.

### A value that nobody stated

A board that leaves a number to the solver builds. The solver takes the first row of a catalog that fits, and a check that reads a value that nothing gives is listed as not evaluated. Two 330 ohm resistors that each drew 8 mA were once picked for a balancing resistance that nobody had stated, and the only trace was a note. Two findings say so (ADR 0065). They are guidelines, like the track width one (ADR 0036): a `warning:` line by default and an error with `--strict` or `guidelines = "error"` in `netsmith.toml`, because the design may be right and a board in progress should build.

An **unconstrained pick** is a part of a catalog with a value column that was picked while nothing limited the value: a `Resistor`'s `.resistance`, a `Capacitor`'s `.capacitance` and `.effective_capacitance`, an `Inductor`'s `.inductance`. The finding names the part, the row the solver took, the member of a module or board that the value is held to when there is one (two parts that hang from one member are one finding), and what to write.

```nsm solve warns
import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Plain {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .pull: Resistor
    .supply.pos ~> .pull ~> .supply.neg
}
```

```text
Plain: unconstrained pick guideline: `lamp.top` and `lamp.bottom` (Resistor) were chosen with nothing limiting their resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104 for each; both are held to `lamp.balance_resistance` (`&=` at main.nsm:9 and 10), which nothing states: state it, `.lamp.balance_resistance &= 4.7kohm +/- 5%`, or say that any value will do, `.lamp.balance_resistance &= 10kohm..100kohm`
Plain: unconstrained pick guideline: `pull` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104: state it, `.pull.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.pull.resistance &= 10kohm..100kohm`
```

Say what the value is, bound a value that it decides, or say that any value will do. A range is the way to say "any": the finding cannot tell a wide range from a narrow one, and no keyword is added for it.

```nsm solve
import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Stated {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .lamp.balance_resistance &= 10kohm..100kohm
    .pull: Resistor { .resistance &= 4.7kohm +/- 5% }
}
```

A pick is limited when at least one of these holds, and the checks it names are judged (they pass or fail) once the board is solved:

- the value, or a member it is held to or a field a `~` joins it to, has a value, a range or a tolerance: a `&=`, a `?=` default that applied, or a range that a member carries in. Any range counts, however wide;
- a bound (`>=` or `<=`) has the value, or such a member, alone on one side. A bound on a member that a part is held to counts once it reaches the part (see the end of "Catalogs and how the solver picks parts");
- a check that the pick decides: a bound, or an equation or `&=` whose left side has a range of its own, that names a value worked out from the pick (an LED's stated current in `.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance`);
- a current stated on a net that a value worked out from the pick is a term of (a budget, ADR 0038), when the pick changes whether the budget holds: a load that the module states on the same supply, and that the pick does not feed, is not a limit on the pick.

What does not limit a pick: a bound on a rating (`.r.max_power >= 0.25W`, a `max_voltage`, the package); a relation that a part's own definition states (a resistor's `drop * current <= max_power`, a diode's `current >= 0mA`); and the catalog being filtered, because a filter on the package says nothing about the value. A bound that waits for a value that nothing sets is not judged, so it does not limit the pick, and the finding is made unless an open circle finding already names that check (one finding for one cause). A part with no row for its requirements is reported by its own message and not by this one.

A check that holds whatever the size of the values is, does not limit their size. The output of a divider, `.out == .supply.voltage * .bottom.resistance / .total_resistance` with `.out >= 1.5V` and `.out <= 1.8V`, fixes the ratio of the two resistors and says nothing of whether they are 330 ohm or 330 kohm; if both are unstated, both are unconstrained in size. The solver tells it by scaling the unlimited picks together by one factor and expanding each check through the equations that define what it reads: a check whose two sides are unchanged by the scaling limits a ratio. The finding says so. A range on one resistor, on the total, or a bound on a current that the divider draws (`.draw <= 200uA`) limits the size, and ends it. What the solver cannot expand (a value worked out by a join, by a balance of currents or by two equations) it does not call a ratio, so the finding is then missed and never made wrongly.

```nsm solve warns
import std::parts::passive::Resistor
import std::sources::Rail

module Divider {
    .supply: Rail
    .total_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .total_resistance == .top.resistance + .bottom.resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .out: Voltage
    .out == .supply.voltage * .bottom.resistance / .total_resistance
    .out >= 1.5V
    .out <= 1.8V
}

board RatioOnly {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .divider: Divider
    .divider.supply ~ .supply
}
```

```text
RatioOnly: unconstrained pick guideline: `divider.top` and `divider.bottom` (Resistor) were chosen with nothing limiting their resistance in size, so the solver took the first row that fits, 330Ω 1% 0402 C25104 for each; their resistance is limited as a ratio, not in size, by `.out >= 1.5V` and `.out <= 1.8V` (main.nsm:13 and 14), which hold whatever the size is: state one of them, `.divider.top.resistance &= 4.7kohm +/- 5%`, or bound a value that changes with the size, not only with the ratio (a current or a voltage the design checks), or say that any value will do, `.divider.top.resistance &= 10kohm..100kohm`
```

An **unstated value** is a value declared in a module or a board that a check reads, which was not evaluated because nothing states the value or works it out. It is for what is not a pick: a voltage that a board states for a check, an enum, a text.

```nsm solve warns
module Receiver {
    .line: Pin
    .vih: Voltage
    .vil: Voltage
    .driver_high: Voltage
    .driver_high >= .vih
    .driver_high <= 5.5V
    .vil &= 0.3V
}

board Probe {
    .receiver: Receiver
}
```

```text
Probe: unstated value guideline: `receiver.vih` and `receiver.driver_high` are declared by `Receiver` and nothing states or works them out, so the checks `receiver.driver_high >= receiver.vih` and `receiver.driver_high <= 5.5V` are not evaluated; state them, for example `.receiver.vih &= 3.3V +/- 5%`
```

State the values, and the checks are made against them:

```nsm solve
module Receiver {
    .line: Pin
    .vih: Voltage
    .vil: Voltage
    .driver_high: Voltage
    .driver_high >= .vih
    .driver_high <= 5.5V
    .vil &= 0.3V
}

board Probe {
    .receiver: Receiver { .vih &= 2V; .driver_high &= 3.3V +/- 5% }
}
```

Rules.

- **Only a root is named.** A value is reported when it is declared in a module or a board (not in a part, not in an interface such as a rail, not a catalog column), has no value after the solve, is not the left side of an equation or `&=` that works it out, and is not joined or a term of a balance of currents. A member that a wrapper forwards to (`.inner.driver_high == .driver_high`) is not a root; the wrapper's member is, and it is the one named, because it is the one the board can state. A part's unset datasheet value is a gap in the library (ADR 0046), and an interface's optional values (marked `optional`, below) are offered open on purpose, so neither is reported.
- **A check reads it.** A member that no check reads is not reported: a value that nothing checks and nothing picks from changes nothing. A bound on a member that reached a part was judged there, and is the way the board states the member. A definition that waits (`.x == .y * 2`, `.part.field &= .member`) is not a check.
- **A member that an unconstrained pick names is named once,** by the pick finding, and so is a value that an open circle finding already asks to be stated.
- **A module checked alone is not judged.** Both findings are made on a solved board, where the use is known. A module that no board uses builds.
- **A default is a statement of the library.** `.balance_resistance ?= 4.7kohm +/- 5%` in the module that offers it limits the pick, so a board that says nothing gets no finding, and a board that states its own value replaces it (see "Defaults a board can replace").

### A value that may stay open

Some numbers are on a datasheet only sometimes: an IC may name the largest DC resistance of the inductor it works with, or leave it out. A check against such a number is made when the IC states it and means nothing when it does not. A member says so with the word `optional` after its type (ADR 0074, provisional), the word that a `pins` row and a catalog column already use for "may be absent":

```nsm solve
interface Limits {
    /// The largest DC resistance, when the datasheet gives one.
    .max_dcr: Resistance optional
}

module Coil {
    .limits: Limits
    .dcr: Resistance
    .dcr <= .limits.max_dcr
}

board Quiet {
    .coil: Coil { .dcr &= 0.08ohm }
}
```

`Quiet` states nothing about `.coil.limits.max_dcr`, so the relation `.dcr <= .limits.max_dcr` is skipped: it is not checked, the build does not list it as not evaluated, and no finding is made for it. Without `optional` the same board would list the relation as not evaluated, as it does for any value that nothing states. When something states the member, the relation is checked as any other. Here the limit is stated on the other side of a join, as an IC states it, and the coil is refused with the numbers:

```nsm fails
interface Limits {
    /// The largest DC resistance, when the datasheet gives one.
    .max_dcr: Resistance optional
}

module Coil {
    .limits: Limits
    .dcr: Resistance
    .dcr <= .limits.max_dcr
}

module Ic {
    .limits: Limits
}

board Strict {
    .coil: Coil { .dcr &= 0.08ohm }
    .ic: Ic { .limits.max_dcr &= 0.05ohm }
    .coil.limits ~ .ic.limits
}
```

```text
error: coil.dcr must always be at most coil.limits.max_dcr, but 80mΩ is not always below 50mΩ
  --> main.nsm:9:5
  |
9 |     .dcr <= .limits.max_dcr
  |     ^^^^^^^^^^^^^^^^^^^^^^^
```

Rules.

- **Stated is what is written.** A member is stated when a statement of the design names it, as ADR 0069 had it for `required`, and not when the solver gave it a value. The statements that count are an `&=`, a `?=` default that applies, an equation (`==`) with the member alone on its left whose right side has no unstated optional member, and a bound against numbers alone (`.max_dcr <= 0.5ohm`). A statement anywhere that can state the member counts: the board, a module around it, the part.
- **A join carries it.** `~` between two interfaces makes their members one value, so a limit that the IC states on its side is stated on the stage's side, and a limit that neither side states is stated on neither.
- **The member alone is guarded.** A relation is skipped because an optional member in it is unstated, never because another value in it is. The relation `.dcr <= .limits.max_dcr` is itself a bound with the member alone on one side, and it does not state the member. A relation over an optional member that is stated and a value that nothing states is listed as not evaluated, as before.
- **A bound that gives no value is not silent.** `.limits.max_dcr <= 0.5ohm` states the member and gives it no value, so the relation is kept and listed as not evaluated, which says the check was not made.
- **Where it is allowed.** On a value (a quantity, text or an enum) declared in a part, a module, an interface or a generic. It is an error on a pin (a pin that need not be connected says so in its `pins` row), on a member whose type is an interface, a part or a module (mark the values inside it), and in a board, a system, a panel or a physical file, which have no user above them.
- **Not in the definition that declares it.** A statement of an optional member in the interface or module that declares it (`&=`, `?=`, `==`, or a bound against numbers) would state it for every use, so the relation would never be skipped: it is an error that names the two places. A part that has the interface as a member may state it, as that is its own datasheet.

```nsm fails
interface Limits {
    .max_dcr: Resistance optional
    .max_dcr &= 0.1ohm
}

board Misuse {
    .limits: Limits
}
```

```text
error: `max_dcr` is `optional` (at main.nsm:2) and is stated here with `&=`, in the definition that declares it: a statement here would state it for every use, so the checks that read it would never be skipped; keep `optional` and let the design state it, or remove `optional` and keep the statement
  --> main.nsm:3:5
  |
3 |     .max_dcr &= 0.1ohm
  |     ^^^^^^^^^^^^^^^^^^
```

### A function the board does not use

A datasheet states some limits for a function whether or not a design uses it: a microcontroller's datasheet limits the voltage on its USB VBUS pin to 4.35 V to 5.5 V, and a board that has no USB port leaves the pin open. The part states the limit once and it is true for every board, so it stays in the part (libraries state only what is always true). A board that never connects the port was left with two notes it could do nothing about: `not evaluated: processor.usb.vbus.voltage >= 4.35V` (and its twin for 5.5 V), because nothing gives the pin a voltage, and `processor.usb.vbus.pos has no source: nothing supplies it` in the power view. The board says that nothing on it uses the interface, with a statement of its own (ADR 0083, provisional):

```nsm-fragment
unused .processor.usb
```

The word is `unused`, then the path of one interface of a part or a module (`.processor.usb`, `.modem.usb`). It is a statement of a board, written anywhere in it, also in the body of the part's declaration (`.processor: Processor { unused .usb }`). The parts below are invented.

```nsm module=src::processor
import std::bus::Usb2
import std::sources::Rail

/// A processor with a USB port. Its datasheet limits VBUS to 4.35 V to 5.5 V whether or not a
/// board uses the port. Illustrative; not a real part.
part Processor {
    .vdd: Rail
    .usb: Usb2
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.voltage >= 4.35V
    .usb.vbus.voltage <= 5.5V

    pins {
        .vdd.pos      power         "VDD"  "1"
        .vdd.neg      power         "VSS"  "2"
        .usb.vbus.pos power         "VBUS" "3"
        .usb.dp       bidirectional "DP"   "4"
        .usb.dm       bidirectional "DM"   "5"
    }
}

/// A test pad.
part Pad {
    pins {
        .p passive "P" "1"
    }
}

/// A USB receptacle that the bus goes through.
part Receptacle {
    .usb: Usb2

    pins {
        .usb.vbus.pos power         "VBUS" "1" optional
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}
```

A board with no USB port says so, and builds with neither note:

```nsm evaluate
import src::processor::{Pad, Processor}
import std::sources::Rail

board Sensor {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .processor: Processor
    .supply ~ .processor.vdd
    unused .processor.usb
}
```

Without the `unused` line the same board builds too, and its notes are the two above. The pin `.usb.vbus.pos` is a `power` pin without `optional`, which must be connected; the statement is also why `Sensor` has no error for it.

What the statement does, for the interface it names and everything inside it:

- **The part's limits for it are not evaluated.** A relation that reads only values of the interface (`.usb.vbus.voltage >= 4.35V`) is taken out before the solve: it is not checked and not listed as not evaluated. A relation that reads another value as well (`.usb.vbus.voltage >= .vdd.voltage - 2V`) is kept: it says something about the rest of the part.
- **Its pins need no source and no connection.** The power view has no net for the rails of the interface (so no `has no source` note), the width of the tracks of a net is not worked out for them, and a pin of the interface that its table says must be connected is not asked to be.
- **Its levels do not run.** A pin of it is not compared with another as a driver or a receiver, and its absolute maximum is not read against a net.
- **Nothing else changes.** Another interface of the part keeps every check, and a board that does not write the statement gets what it got before.

The statement says that nothing on the board uses the interface. A board that also uses it contradicts itself, and the error shows both lines: the connection, and the line of the `unused` statement. A pin of the interface that is on a net with a pin of another part, a series fuse, resistor or ferrite bead that leads from it to such a pin (below), an interface joined to another with `~`, and a function chosen on a pin of it are all uses:

```nsm fails
import src::processor::{Processor, Receptacle}
import std::sources::Rail

board Contradiction {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .processor: Processor
    .jack: Receptacle
    .supply ~ .processor.vdd
    .jack.usb.vbus.neg ~ .supply.neg
    .processor.usb ~ .jack.usb
    unused .processor.usb
}
```

```text
error: `processor.usb.dm` is connected here to `jack.usb.dm` (net `net-jack.usb.dm`), but `processor.usb` is stated `unused` at main.nsm:15: the interface is either used or not; remove the connection, or remove `unused`
   --> main.nsm:14:5
   |
14 |     .processor.usb ~ .jack.usb
   |     ^^^^^^^^^^^^^^
```

A test pad does not use the interface. A part with passive pins only (a test pad, a header) brings a net out and uses nothing, and neither does a return to ground: **unused means that nothing on the board uses the interface**, not that nothing is connected to it. A module's own USB pads, which the board names and brings out to unpopulated test pads, can be stated unused, and the power view then has no net that nothing supplies for them:

```nsm evaluate
import src::processor::{Pad, Processor}
import std::sources::Rail

board Modem {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .processor: Processor
    .supply ~ .processor.vdd
    .tp_dp: Pad
    .tp_vbus: Pad
    .processor.usb.dp ~ .tp_dp.p
    .processor.usb.vbus.pos.net &= "USB_VBUS"
    .tp_vbus.p.net &= "USB_VBUS"
    .processor.usb.vbus.neg ~ .supply.neg
    unused .processor.usb
}
```

A fuse, a resistor or a ferrite bead in series from the interface to the rest of the board is a use, through however many of them: the part still feeds the board from the interface, and the interface's limits and the draw of its rails belong in the checks. The rest of the board is a net that has a pin of another part that is not `passive`. The error names the parts the use goes through and the pin it reaches:

```nsm fails
import src::processor::{Processor, Receptacle}
import std::parts::passive::Fuse
import std::sources::Rail

board Behind {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .processor: Processor
    .jack: Receptacle
    .fuse: Fuse
    .supply ~ .processor.vdd
    .jack.usb.vbus.neg ~ .supply.neg
    .processor.usb.vbus.pos ~> .fuse ~> .jack.usb.vbus.pos
    unused .processor.usb
}
```

```text
error: `processor.usb.vbus.pos` is connected here through `fuse` (a `Fuse`) to `jack.usb.vbus.pos` (net `net-fuse.b`), but `processor.usb` is stated `unused` at main.nsm:17: the interface is either used or not; remove the connection, or remove `unused`
   --> main.nsm:16:5
   |
16 |     .processor.usb.vbus.pos ~> .fuse ~> .jack.usb.vbus.pos
   |     ^^^^^^^^^^^^^^^^^^^^^^^
```

A series part that leads only to test pads, to a return (a pull-down to ground proves nothing), or to another interface that is stated unused, brings no use. Only a `Resistor`, a `Fuse` and a `FerriteBead` are series parts for this rule; a capacitor, an inductor, a diode or a switch in series is not followed (it is not yet decided whether one is a use).

Rules.

- **A board, an interface.** Only a board states it. A part, a module, a system or a panel cannot say that a function of its own is unused, and a physical file states where things are and not what the circuit is. The path must name an interface (a pin, a value, a whole part and a whole module are errors that say so), and naming one twice is an error.
- **A use is a net or a choice.** An interface is used when one of its pins (not the return of a supply: every ground is one net) shares a net with a pin of another part that is not `passive` (also through series fuses, resistors and ferrite beads: the net at the far end of such a part counts, unless it is a return), when it is joined to another interface with `~` (a rail of the board that supplies it is one), or when the board states a function for one of its pins. The pins a part's own definition joins inside it are the part's business, and a connection made inside the module that holds the interface is the module's own wiring.
- **Order does not matter.** The statement acts after every statement of the board is read.

### A generic part that no row can be ordered for

The solver keeps a generic part (a resistor, capacitor, inductor or LED of a value that no catalog row has, ADR 0047) when no row meets the design, and records what the design asks of it: for an inductor in a converter, a saturation current of at least 6.05 A. The bill of materials and `fab` then look for a row that can be ordered for it, and `fab` refuses the board when there is none. A third finding says so when the board is built, for every assembled generic part and not only those on a power path (issue #182). It is a guideline like the two above: a `warning:` line by default and an error with `--strict` or `guidelines = "error"` in `netsmith.toml`.

```nsm solve warns generics
import std::parts::passive::Capacitor
import std::sources::Rail

board Odd {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .c: Capacitor {
        .capacitance &= 130nF +/- 10%
        .max_voltage >= 12V
    }
    .supply.pos ~ .c.a
    .supply.neg ~ .c.b
}
```

```text
Odd: unresolved generic guideline: C1 (c): the solver kept a generic part because no row of the `capacitor` catalog can be ordered for 130 nF 10% 0402, rated voltage at least 12 V (E24, needs a recorded DC-bias curve). Nearest in the catalog: 100nF 10% 16V X7R 0402 C1525 (-23.1%, Basic), 100nF 10% 50V X7R 0402 C307331 (-23.1%, Basic), 220nF 10% 16V X7R 0402 C16772 (+69.2%, Basic). `fab` will refuse the board until a row meets it: add one to the project's `capacitor` catalog (`catalogs/capacitor.csv`), or change the design so that a row in the catalogs does.
```

The finding names the part (its reference designator and its path), what the solver kept (value, tolerance, package) and the need it recorded, the rows nearest the need and what the design does with each in the part's place, and what to add: a row that meets it, in the project's catalog (ADR 0022). The rows are ranked by how near they come to the need that fails, not by value, so a project row that nearly meets it (a 5.3 A inductor for a 6.05 A need) is listed before a standard row of the right value that falls far short. `fab` prints the same list in its refusal.

- **A part that is not assembled is not reported.** A part with `.assembly &= AssemblyMode::Hand` or `AssemblyMode::None`, or a through-hole part that is soldered by hand, needs no row for the board house; `fab` does not refuse it either.
- **A generic that a row can be ordered for is not reported.** The check is `fab`'s own (the stock a provider reports is read as it is for `fab`), so the finding appears exactly when `fab` would refuse the part.
- **It is made after the other findings.** With `--strict`, a board that has an unconstrained pick or an open circle is refused for those first, and this one is listed once they are fixed.

### Values that wait on each other

The solver sets a field from an equation only when every field on the right has a value. If equations feed each other in a circle and no field of the circle has a value, none can be set first: every relation in the circle, and every check after it, stays unevaluated. A load that draws constant power through a series resistor is such a circle. Its voltage needs the drop across the resistor, the drop needs the current, and the current needs the voltage.

```nsm solve warns circular
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Circle {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 10ohm +/- 1% }
    .load: Rail {
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos

    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage
}
```

The build succeeds, lists each relation as not evaluated, and says so twice. The circle has a check inside it that cannot be made (the resistor's power rating needs the current), so there is a warning that names the check and a field to start from (ADR 0057):

```text
Circle: open circle guideline: these values depend on each other and none has a value: `load.current`, `load.voltage`, `r1.current`, `r1.drop`, `supply.current`; so the checks `(r1.drop * r1.current) <= r1.max_power` are not evaluated, and the parts they would judge are chosen without them; state a range on one of them with `&=`, for example `load.voltage`, that holds every value it can take: the checks are then made against it, and a range that is too small makes them fail
```

and one note for the circle that names its fields:

```text
these values depend on each other and none has a value, so the checks that use them are not evaluated: `load.current`, `load.voltage`, `r1.current`, `r1.drop`, `supply.current`; state a range on one of them with `&=` to start the solver, or state the range the part works in (`>=` and `<=` on the voltage) so that the solver can start it
```

- Start the solver by stating a range on one field of the circle with `&=`, here `.load.voltage &= 5V..16V`. It is a starting point and not an answer: once the other values are worked out, `.load.voltage` is checked against its own equation, and a range that is too tight is an error. A range that is too wide stays as wide as it was stated: the equation narrows a stated range only when the value it gives lies wholly inside it, so a start taken from the whole range a part works in, such as `3V..65V` for a regulator, is never narrowed once the first pass reaches below it, and every value worked out from its low end stays loose with it (the equation's check then says the range cannot be shown).
- Or let the load say where it works, and the solver starts the circle itself (ADR 0053). That is the next section.
- The note is only a note: no design passes or fails differently because of it. The warning is a guideline finding, like the track width one: it is a `warning:` line by default, an error with `--strict` or `guidelines = "error"` in `netsmith.toml`, and it is given only when a bound (`>=` or `<=`) that reads a value of the circle, or a value worked out from one, is not evaluated. A circle with no such check in it (a part with no rating) gets the note alone, and no build is refused for it.
- A field that is unset only because nothing states it is not a circle and gets no note. A circle that also waits on such a field gets no note either, because a range on the circle would not start it; if a check inside it is not evaluated it gets the warning, which then names the value to state first (`a range on them alone would not start them, because they also read ..., which nothing states: state it first`). The value it names is one that nothing states and nothing works out: when the circle reads a value that another equation works out from an unstated one, it is the unstated one that is named, in path order, with `and N more` after ten.

#### A circle through the currents of a net

A divider on a supply behind a part in series has no circle of equations, and its checks still wait. The divider's current needs its input voltage, the voltage needs the drop across the series part, and the drop needs that part's current: what the net after it carries to the divider and to whatever else is on it. The solver works a part's current out only from a net of two currents (ADR 0026, because the residual of several ranges says little about one part), so the part's current is never set, and the draw limit below is never judged: the divider is picked without it (issue #18).

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

module Divider {
    .vin: Rail
    .mid: Pin
    .max_draw: Current
    .top: Resistor
    .bottom: Resistor
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
    .top.current <= .max_draw
}

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 14V..22V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop

    .divider: Divider { .max_draw &= 40uA }
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
}
```

The build picks 330 ohm and 330 ohm, which draw far more than 40 uA at 14 V, and says so with a warning (the same `open circle` finding, with the same severity):

```text
Behind: open circle guideline: these values depend on each other and none has a value: `after.current`, `after.voltage`, `divider.top.current`, `divider.vin.current`, `divider.vin.voltage`, `series.current`, `series.drop`, `supply.current`; so the checks `divider.top.current <= divider.max_draw`, `(series.drop * series.current) <= series.max_power`, `(divider.top.drop * divider.top.current) <= divider.top.max_power` and 1 more are not evaluated, and the parts they would judge are chosen without them; net `AFTER` carries 3 currents (`after.current`, `divider.vin.current`, `series.current`), none of which has a value, and the solver works a part's current out only from a net of two currents (ADR 0026), so what the checks wait on is never set: state a range on `series.current` (the part's current: the most and the least that the net's loads draw through it), or on a voltage in the circle, for example `after.voltage`; if nothing else draws from the net, `after.current &= 0A` makes it a circle of equations, which a voltage starts; the checks are then made against what you state, and nothing compares it with the net's other currents
```

- A range on the part's current starts it: `.series.current &= 0A..4mA` is a claim about what the loads behind the part draw at most and at least. So does a range on a voltage in the circle, `.divider.vin.voltage &= 0V..22V`, as for any circle. The checks are then made against what you state; nothing compares a stated current with the net's other currents, because one of them has no value.
- A range on another term of the net (`.after.current &= 0A..1mA`) does not start it: the net still has three currents and the part's is one of them.
- A current that is exactly zero is not a term. `.after.current &= 0A` says nothing else draws from the net, so the part's current is the divider's, and what is left is a circle of equations, which a voltage starts as above (and the finding then says so in its first form).
- The finding is given when a bound (`>=` or `<=`) that reads a value in the circle is not evaluated, and the circle would close through the currents of a net of more than two terms that has no value. A design whose load simply has no stated current (a fuse in front of a load) is no circle and gets the notes alone; so does a net that has pins the solver cannot account for (an IC's signal pin), because nothing on it is determined by the others even in principle.

#### What starts it: the rail that supplies the net, with its sign

Two statements look alike and only one starts the circle. Take the board above and let a lamp, a `Rail` with its own range, share the net `AFTER` (`.after ~ .lamp`). The lamp's `.current &= 0A..4mA` is a load, so it counts positive (see _The sign of a current_ under Load current). It does not start the circle: it is a fourth term on a net whose other three currents still have no value, and the finding is the same:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

module Divider {
    .vin: Rail
    .mid: Pin
    .max_draw: Current
    .top: Resistor
    .bottom: Resistor
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
    .top.current <= .max_draw
}

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 14V..22V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop

    .divider: Divider { .max_draw &= 40uA }
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
    .lamp: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
        .current &= 0A..4mA
    }
    .after ~ .lamp
}
```

```text
Behind: open circle guideline: these values depend on each other and none has a value: `after.current`, `after.voltage`, `divider.top.current`, `divider.vin.current`, `divider.vin.voltage`, `lamp.voltage`, `series.current`, `series.drop`, `supply.current`; so the checks `divider.top.current <= divider.max_draw`, `(series.drop * series.current) <= series.max_power`, `(divider.top.drop * divider.top.current) <= divider.top.max_power` and 1 more are not evaluated, and the parts they would judge are chosen without them; net `AFTER` carries 3 currents (`after.current`, `divider.vin.current`, `series.current`), none of which has a value, and the solver works a part's current out only from a net of two currents (ADR 0026), so what the checks wait on is never set: state a range on `series.current` (the part's current: the most and the least that the net's loads draw through it), or on a voltage in the circle, for example `after.voltage`; if nothing else draws from the net, `after.current &= 0A` makes it a circle of equations, which a voltage starts; the checks are then made against what you state, and nothing compares it with the net's other currents
```

The rail that supplies the net starts it. Here that is `.supply`, which supplies the net `VIN` and whose current counts negative: `.supply.current &= -8mA..0A` says it supplies at most 8 mA. The net `VIN` has two currents, the supply's and the series part's, so the series part's current follows from the supply's, and the circle is started. The finding goes and the divider is picked against its 40 uA (1M, not 330 ohm):

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

module Divider {
    .vin: Rail
    .mid: Pin
    .max_draw: Current
    .top: Resistor
    .bottom: Resistor
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
    .top.current <= .max_draw
}

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 14V..22V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop

    .divider: Divider { .max_draw &= 40uA }
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
    .lamp: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
        .current &= 0A..4mA
    }
    .after ~ .lamp
    .supply.current &= -8mA..0A
}
```

- Write the supplier's own current, negative: `-8mA..0A`, not `0A..8mA`. A load states its draw positive, a rail that supplies states what it supplies negative.
- A range on a load joined with `~` onto a net that has other terms without a value (`.lamp.current &= 0A..4mA` here) does not start a circle; a range on the supplier's current does.

#### A circle the solver starts from the supply

A load that states the range it works in (`.load.voltage >= 4V` and `<= 30V`, as a regulator IC states its input range) lets the solver start the circle itself. It takes the range of a value the equation reads that already has one, the supply's `9V..16V`, and grows it until the equation maps it into itself, the way a stated range would be checked. That is the range the load's voltage is in, here 8.4 V to 15.7 V, and the current and the drop follow from it.

```nsm solve started
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Started {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 10ohm +/- 1% }
    .load: Rail {
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos

    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage
    .load.voltage >= 4V
    .load.voltage <= 30V
}
```

The build lists nothing as not evaluated, and says what it did:

```text
`load.voltage` depends on itself through its own equations and nothing stated a range for it, so the solver started it from `supply.voltage` and worked out 8.398V..15.69V, the range its equations map into itself; no other value inside 4V..30V, the range its part works in, satisfies them (state a range on it with `&=` to replace this)
```

The start is a claim, and the solver accepts it only when it can show that nothing else is possible. A constant-power load has a second working point near 0 V (the load draws more as the voltage falls, and the resistor drops more), so the equations alone never leave one value. What rules it out is the range the part works in: the lowest lower bound and the highest upper bound that its relations put on the field (`>=` and `<=` with a value). The solver holds the field to the part of that range below the claim, and to the part above it, and propagates: if the equation leaves nothing of either, the claim is the only value the part can be in. The rules:

- A range stated on a field of the circle (with `&=`) is used as stated, and nothing is started.
- Nothing is started without a range on the part, on both ends, or without a value to start from. The relations stay not evaluated, and the note above is the only one.
- A second value that the equations allow inside the part's range is a reason not to start: at 36 ohm behind 9 V the load can sit at 5.9 V and at 3.1 V, and with a least of 3 V the circle stays unstarted and a check that needs it is not evaluated. With a least of 4 V, 3.1 V is not a state of the load, and the circle is started from 5.9 V.
- A state below the part's range is the part switched off, which the equations do not describe, so the checks pass for the states inside the range. A design that must also hold when the load is not working says so with its own relation.
- A circle that cannot be solved at all (100 ohm cannot give 0.5 W at 9 V) is not started and its relations are not evaluated.

Misuse: a plain `=`.

```nsm fails
import std::sources::Rail

board Plain {
    .a: Rail { .voltage = 5V }
}
```

```text
error: there is no `=`: use `&=` to narrow a value, or `==` for an equation that must hold
  --> main.nsm:4:25
  |
4 |     .a: Rail { .voltage = 5V }
  |                         ^
```

Misuse: two narrowings that do not overlap, and a bound that cannot hold.

```nsm fails
import std::parts::passive::Resistor

board Contradiction {
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .r1.resistance &= 2kohm +/- 1%
}
```

```text
error: contradiction on `r1.resistance`: it is already 990Ω..1.01kΩ, which does not overlap 1.98kΩ..2.02kΩ
  --> main.nsm:5:5
  |
5 |     .r1.resistance &= 2kohm +/- 1%
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::sources::Rail

board Bound {
    .a: Rail { .voltage &= 3V }
    .b: Rail { .voltage &= 5V }
    .b.voltage <= .a.voltage
}
```

```text
error: b.voltage must always be at most a.voltage, but 5V is not always below 3V
  --> main.nsm:6:5
  |
6 |     .b.voltage <= .a.voltage
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

## Connections: `~` and `~>`

`~` puts two pins on one net, or joins two interfaces of the same kind: their pins pair by name and their values become equal. `~>` runs a series chain from a pin through parts to another pin. Each part in a chain needs a `bridge`.

```nsm
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board Chains {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .a: Resistor
    .b: Resistor
    .led: Led

    // These two lines make the same nets.
    .supply.pos ~> .a ~> .b ~> .led ~> .supply.neg
    // .supply.pos ~ .a.a
    // .a.b ~ .b.a
    // .b.b ~ .led.anode
    // .led.cathode ~ .supply.neg
}
```

A part declares its bridge: `bridge .a ~> .b` in `Resistor` says that a chain enters at pin `a` and leaves at pin `b`. `Led` enters at `anode` and leaves at `cathode`.

Rules.

- `~` joins pin to pin, or interface to interface when both are the same definition. It does not join a pin and an interface.
- `~>` starts at a pin. A part in the chain is entered at the first pin of its bridge and left at the second. Chains may continue from a part, `.a ~> .b`, and may end at a pin.
- `a ~> pin ~> b` puts `a`, the pin and `b` on one net.
- A chain is a statement about nets. It does not model the physics of the series current. Write the equation yourself, as `examples/led.nsm` does.
- A system joins ports with `~` only (see Systems).

Misuse.

```nsm fails
import std::parts::connector::Header1x2
import std::sources::Rail

board NoBridge {
    .supply: Rail
    .header: Header1x2
    .supply.pos ~> .header ~> .supply.neg
}
```

```text
error: `.header` cannot be used in a `~>` chain: `Header1x2` has no `bridge`
  --> main.nsm:7:20
  |
7 |     .supply.pos ~> .header ~> .supply.neg
  |                    ^^^^^^^
```

```nsm fails
import std::sources::Rail
import std::logic::Logic

board Mismatch {
    .supply: Rail
    .signal: Logic
    .supply ~ .signal
}
```

```text
error: cannot connect a `Rail` to a `Logic`
  --> main.nsm:7:15
  |
7 |     .supply ~ .signal
  |               ^^^^^^^
```

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

board Kinds {
    .supply: Rail
    .r1: Resistor
    .supply ~ .r1.a
}
```

```text
error: `~` joins two pins or two interfaces of the same kind
  --> main.nsm:7:15
  |
7 |     .supply ~ .r1.a
  |               ^^^^^
```

## Nets and net names

A net is all the pins joined at one node. Only a board names a net (ADR 0005). Naming is a narrowing of the `net` field of a pin: `.supply.pos.net &= "VCC"`. Every pin that asks for `VCC` is on the net `VCC`. A net nobody names is called `net-` and its smallest terminal path, for example `net-led.anode`.

```nsm
import std::parts::passive::Resistor
import std::sources::Rail

board Named {
    .supply: Rail {
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .r1: Resistor
    .supply.pos ~> .r1 ~> .supply.neg
}
```

Rules.

- A module, part or interface that wrote `.net &=` would give every instance of itself the same net name, so the checker refuses it outside a board. A board names a module's nets through the module's rails and pins: `.stage.vout.pos.net &= "3V3"`.
- The rule covers any field called `net`, such as the net of a plane in a manufacturing process.
- Pins that are joined and name their net differently are an error.
- Two boards of a system may both have `GND`. They are different nets.

Misuse.

```nsm fails
import std::sources::Rail

module Stage {
    .supply: Rail
    .supply.pos.net &= "VCC"
}

board Uses {
    .stage: Stage
}
```

```text
error: `supply.pos.net` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name
  --> main.nsm:5:5
  |
5 |     .supply.pos.net &= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
error: `stage.supply.pos.net` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name
  --> main.nsm:5:5
  |
5 |     .supply.pos.net &= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::sources::Rail
import std::parts::passive::Resistor

board Disagree {
    .supply: Rail { .pos.net &= "A" }
    .r: Resistor
    .supply.pos ~ .r.a
    .r.a.net &= "B"
}
```

```text
error: these pins are connected but name their net differently: `A` on `supply.pos`, `B` on `r.a`
  --> main.nsm:8:5
  |
8 |     .r.a.net &= "B"
  |     ^^^^^^^^^^^^^^^
```

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

Every part has a field `.assembly`, of the type `AssemblyMode`, that it does not declare (declaring it again is an error). `AssemblyMode` is an enum (see Enums, below) that every file knows without an import. It says who puts the part on the board, and `fab` honours it (ADR 0024, provisional; the type is ADR 0054, proposed):

| Value | Meaning |
|---|---|
| `AssemblyMode::Fab` | The board house places and solders it: a BOM line and a placement row. |
| `AssemblyMode::Hand` | Soldered by hand: not in the BOM or the placement file, and listed in `fab-notes.txt` as `hand-solder:`. |
| `AssemblyMode::None` | Not fitted by anyone, such as a hole or a fiducial: in neither file, and listed in `fab-notes.txt` as `not fitted`. |
| `AssemblyMode::Consigned` | The board house places and solders it, and you ship the part to the board house: a BOM line and a placement row, no supplier part number needed, and listed in `fab-notes.txt` as `consigned:` (ADR 0084, provisional). See Consigned parts, below. |

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

A part inside a module is not the board's to name (ADR 0004), so a module that wants the board to choose who assembles its parts offers the choice as a value of its own and ties its parts to it (ADR 0042). Declare a value of the type `AssemblyMode`, and narrow the part's `.assembly` by it inside the module:

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
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Lamps {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
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
- The board still cannot write `.spare.led.assembly`; it sees only what the module offers.

```nsm fails
import std::parts::connector::Header1x2

module Block {
    .j: Header1x2
}

board Reach {
    .block: Block
    .block.j.assembly &= AssemblyMode::Hand
}
```

```text
error: `j` is inside `Block` and is not part of its interface; set it through a rail, a pin, or a value that `Block` declares
  --> main.nsm:9:12
  |
9 |     .block.j.assembly &= AssemblyMode::Hand
  |            ^
```

### Designators

Every part has a text field `.designator` that it does not declare (declaring it again is an error), as it has `.assembly`. It holds the letters of the part's reference designator on the schematic, the bill of materials and the placement file, and the letters come from the part's kind (below). The number after them counts the parts that have the same letters, from 1, in the order the design declares the parts (owner decision, 2026-11-09, issue #166): the parts of a board in the order of its statements, and a module's parts, and those of a module inside it, where the module is declared, one module after another and each module's parts in its own order, depth first. So a board that declares `.power_in` and then `.enable_in`, two headers, has `J1` and `J2`, and a resistor in a module declared between two resistors of the board is the second. It is the order of the source text and nothing else: not the order of the sheets, how the schematic is drawn, the placement, or anything KiCad does, so the netlist, the bill of materials and the board are the same with every layout. It is also the one place where the order of statements shows: moving a part's declaration above another part of the same letters swaps their numbers (a layout file is keyed by path, so no placement moves). A number at the end of a name (`.tp2`) is part of the name and counts for nothing. The letters follow IEEE 315 and ASME Y14.44 usage: `K` a relay, `BZ` a buzzer, `BT` a battery, `T` a transformer, `U` an integrated circuit.

Each standard kind states its own letters, because they are always true of it, and nothing can change them: `Resistor` is `"R"`, `Capacitor` `"C"`, `Inductor` `"L"`, `FerriteBead` `"FB"`, `Led`, `Diode`, `Schottky`, `Zener` and `Tvs` `"D"`, `NMosfet` and `PMosfet` `"Q"`, `Crystal` `"Y"`, `Fuse` `"F"`, `Switch` `"SW"`, every header, socket, `Coax`, `UsbCPower`, `UsbC2`, `Obd2M` and `ProgrammingPads6` `"J"`, `TestPoint` `"TP"`, `MountingHole` and `PlatedHole` `"H"`, `Fiducial` `"FID"`, `Label` `"LBL"` and `LinearRegulator` `"U"`. A part you define states its own letters in its body, `.designator &= "BZ"`, or leaves them out; a part with none is `"U"`. A board may give the letters to a part of its own that states none, with the same `&=` (a part inside a module is the module's, ADR 0004). A test fails when a part of the standard library has no letters, so a new kind gets them on purpose.

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

Each part is numbered among the parts with its letters, in declaration order. Here the TVS is declared first, so it is `D1` and the diode `D2`; the fuse is `F1` and the header, a second kind with its own count, `J1`:

```nsm
import std::parts::connector::Header1x2
import std::parts::diode::{Diode, Tvs}
import std::parts::passive::Fuse

board Order {
    /// `J1`, though `.zz_in` would sort after the others.
    .zz_in: Header1x2
    /// `D1`.
    .clamp: Tvs
    /// `D2`.
    .rectifier: Diode
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

A row is `.path kind "NAME" "pad" ["pad" ...] [fn FUNCTION[:kind] ...] [optional | required]`, on one line, or ended by `;`. The names and pads are text, so they are quoted. A function is a plain name, with a kind of its own after a colon when the datasheet gives one (see Functions).

| Kind | Meaning | Must be connected? |
|---|---|---|
| `power` | A pin of a supply. Whether it takes or gives power is the sign of its rail's `current`. | Yes, unless `optional` |
| `input` | A signal the part reads. | Yes, unless `optional` |
| `output` | A signal the part drives. | No, unless `required` |
| `bidirectional` | A signal in both directions. | No, unless `required` |
| `passive` | A pin with no direction, such as a resistor's. | No, unless `required` |
| `no_connect` | A pin that must stay open. | Never connected |
| `tri_state` | An output that drives only while its part is selected, and is high impedance otherwise. | No, unless `required` |

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

A board for it. `build` would stop at export, because there is no KiCad symbol for an `Ldo9` yet, so this block is checked and not built.

```nsm
import src::ldo9::Ldo9
import std::sources::Rail

board LdoBoard {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Ldo9 {
        .package &= "DFN-8-EP"
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
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
- A part has one unnamed table or one table for each package, not both. A table that names a package needs a `.package: Package` in the part.
- A pin has one kind, one datasheet name and one `optional` or `required` in every package. Its pads and functions may differ. A pin that only some packages have does not exist in the others.
- Several pads on a row are one pin that the package brings out more than once: `"1" "2"`. Each becomes a pin of the net in the schematic.
- A pin that has no row but is joined inside the part to one that has (`.vout.neg` and `.vin.neg` above) is an alias and needs no pad. A pin with no row and no alias that is on a net is an error, once for the part.
- A pad belongs to one pin, unless the part joins the pins that share it.
- `optional` is for a `power` or `input` pin that may float; `required` is for an `output`, `bidirectional` or `passive` pin that may not. Neither is accepted where it changes nothing.
- `fn` lists what a pin can be. A connected pin with more than one function needs the board to choose: `.u.pa2.function &= "UART1_TX"`. There is no default function. One function cannot be chosen on two pins of a part. A function may carry its own kind, `fn GPIO:bidirectional UART1_TX:output` (next sections).
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
        .neg.net &= "GND"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vout.pos
}
```

```text
error: `u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net
   --> main.nsm:10:5
   |
10 |     .u: Ldo9 { .package &= "DFN-8-EP" }
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
        .neg.net &= "GND"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
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
        .neg.net &= "GND"
    }
    .u: Ldo9 { .package &= "DFN-8-EP" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
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
   --> main.nsm:10:5
   |
10 |     .u: Ldo9 { .package &= "DFN-8-EP" }
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
        .neg.net &= "GND"
    }
    .u: Ldo9 { .package &= "SOT-23-5" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.sense ~ .u.vout.pos
    .u.en ~ .u.vin.pos
}
```

```text
error: `u.sense` (`SENSE`) does not exist in package `SOT-23-5` of `Ldo9`, but the board connects it; the packages that have it: `DFN-8-EP`
   --> main.nsm:10:5
   |
10 |     .u: Ldo9 { .package &= "SOT-23-5" }
   |     ^^^^^^^^
```

Leave the sense pin out and the same board is fine in the SOT-23-5: the tie is skipped there. In a package the part has no table for, the message lists the tables it has:

```nsm fails
import src::ldo9::Ldo9
import std::sources::Rail

board UnknownPackage {
    .input: Rail { .pos.net &= "VIN" }
    .u: Ldo9 { .package &= "QFN-9" }
    .input ~ .u.vin
}
```

```text
error: `u` is a `Ldo9` in package `QFN-9`, which has no pin table; its tables are for `DFN-8-EP`, `SOT-23-5`
  --> main.nsm:6:5
  |
6 |     .u: Ldo9 { .package &= "QFN-9" }
  |     ^^^^^^^^
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
    .mpn: String
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

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board NoFunction {
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
}
```

```text
error: `u.pa2` has several functions (GPIO, UART1_TX, SPI1_SCK) and is connected: choose one with `.u.pa2.function &= "GPIO"`
   --> main.nsm:11:5
   |
11 |     .u: Mcu32 { .package &= "QFN-32" }
   |     ^^^^^^^^^
```

A function that the pin does not offer, and one function on two pins:

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board WrongFunction {
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
    .u.pa2.function &= "I2C1_SDA"
}
```

```text
error: `u.pa2` cannot be `I2C1_SDA`; its functions are GPIO, UART1_TX, SPI1_SCK
   --> main.nsm:16:5
   |
16 |     .u.pa2.function &= "I2C1_SDA"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import src::mcu::Mcu32
import std::parts::passive::Resistor
import std::sources::Rail

board OneFunctionTwice {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .load: Resistor
    .load2: Resistor
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
    .u.pa2 ~ .load.a
    .u.pb6 ~ .load2.a
    .u.pa2.function &= "UART1_TX"
    .u.pb6.function &= "UART1_TX"
}
```

```text
error: function `UART1_TX` is chosen on both `u.pa2` and `u.pb6`
   --> main.nsm:19:5
   |
19 |     .u.pb6.function &= "UART1_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
        .neg.net &= "GND"
    }
    .hub: Hub
    .load: Resistor
    .supply ~ .hub.supply
    .hub.scl ~ .load.a
    .hub.scl.function &= "UART1_TX"
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
        .neg.net &= "GND"
    }
    .hub: HubWithoutEquation
    .load: Resistor
    .supply ~ .hub.supply
    .hub.scl ~ .load.a
    .hub.scl.function &= "UART1_TX"
}
```

```text
error: `hub.mcu.pa2` has several functions (GPIO, UART1_TX, SPI1_SCK) and is connected: choose one with `.mcu.pa2.function &= "GPIO"`
   --> src/hub.nsm:19:5
   |
19 |     .mcu: Mcu32 { .package &= "QFN-32" }
   |     ^^^^^^^^^^^
error: `hub.scl.function` is stated, but `hub.scl` is a pin of module `HubWithoutEquation` and nothing passes it to a pin of a part, so it has no effect; inside `HubWithoutEquation`, write `.mcu.pa2.function == .scl.function`
   --> main.nsm:15:5
   |
15 |     .hub.scl.function &= "UART1_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
    .a.pa2.function &= "UART1_TX"
    .b.pa2.function &= "UART1_RX"
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
    .a.pa2.function &= "UART1_TX"
    .b.pa2.function &= "UART1_TX"
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
    .a.pa2.function &= "UART1_RX"
    .b.pa2.function &= "UART1_RX"
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
    .u.p00.function &= "UARTE0_TX"
    .u.p01.function &= "UARTE0_RX"
    .u.p02.function &= "TWIM1_SCL"
    .u.p03.function &= "TWIM1_SDA"
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
    .u.p00.function &= "UARTE0_TX"
    .u.p01.function &= "UARTE0_RX"
    .u.p02.function &= "TWIM0_SCL"
    .u.p03.function &= "TWIM0_SDA"
}
```

```text
error: `u` (`Zmcu`) uses UARTE0 and TWIM0, which share `SERIAL0` and run one at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:11) and on `u.p01` as UARTE0_RX (main.nsm:12), TWIM0 on `u.p02` as TWIM0_SCL (main.nsm:13) and on `u.p03` as TWIM0_SDA (main.nsm:14); the other shared resources of `Zmcu`: `SERIAL1` has nothing in use
   --> main.nsm:13:5
   |
13 |     .u.p02.function &= "TWIM0_SCL"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
    .u.p00.function &= "UARTE0_TX"
    .u.p02.function &= "UARTE0_RX"
    .u.p03.function &= "NETUART_TX"
    .u.p04.function &= "HSSPI_SCK"
    .u.p05.function &= "HSSPI_MOSI"
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
    .u.p01.function &= "HSSPI_SCK"
}
```

```text
error: `u.p01` cannot be `HSSPI_SCK`; that function is offered only on `u.p04`; `u.p01` offers APP_GPIO, NET_GPIO and the lines of the peripherals on those pins: UARTE0, TWIM0, NETUART
  --> main.nsm:8:5
  |
8 |     .u.p01.function &= "HSSPI_SCK"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import src::zmcu2::Zmcu2
import std::parts::connector::Header1x8

board WrongCore {
    .u: Zmcu2
    .j: Header1x8
    .u.p02 ~ .j.p1
    .u.p02.function &= "NETUART_TX"
}
```

```text
error: `u.p02` cannot be `NETUART_TX`; that function is offered on every pin that lists `NET_GPIO`; `u.p02` offers APP_GPIO and the lines of the peripherals on those pins: UARTE0, TWIM0
  --> main.nsm:8:5
  |
8 |     .u.p02.function &= "NETUART_TX"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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

### A peripheral that is an end of a bus

A processor whose SPI pins are any pin of a class has no `.spi: Spi` to state a role on: its master end is a peripheral, and a function names no bus. The peripheral says which bus it is an end of with one clause, and the lines that are its functions (ADR 0064):

```nsm-fragment
peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
    SPIM0_SCK:output  as sclk
    SPIM0_MOSI:output as mosi
    SPIM0_MISO:input  as miso
}
```

`as Spi SpiRole::Master` names the interface and the role (a bus with one role writes `as Uart`), and `as sclk` after a function names the line it is. A function that names no line (a data/command pin of a display) is not on the bus. The part is refused when the interface has no such role, when a line is not one the role has, when two functions are one line, and when the kind a function states disagrees with the line (`SPIM0_MISO:output as miso` for a master, whose `miso` is an input). The peripheral is the end of the bus (`u`, `SPIM0`), and a pin on which the board chose one of its functions is a pin of that line and role: the rules of the previous section read it as they read the pin of an interface, with the end named by the part and the peripheral. The board's `.u.p00.function &= "SPIM0_SCK"` is the statement that puts the pin on the bus, and a pin that the board leaves at `APP_GPIO` is a plain pin.

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

The master joined to the flash of the previous section builds, with the select on a plain pin of the processor:

```nsm evaluate
import src::zmcubus::Zmcubus
import src::zparts::Zflash
import std::sources::Rail

board AnyPin {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Zmcubus
    .flash: Zflash
    .supply ~ .u.vdd
    .supply ~ .flash.vdd

    .u.p00.function &= "SPIM0_SCK"
    .u.p01.function &= "SPIM0_MOSI"
    .u.p02.function &= "SPIM0_MISO"
    .u.p03.function &= "APP_GPIO"
    .u.p00 ~ .flash.spi.sclk
    .u.p01 ~ .flash.spi.mosi
    .u.p02 ~ .flash.spi.miso
    .u.p03 ~ .flash.spi.cs
}
```

With the pins of `mosi` and `miso` joined to the wrong lines of the flash, the rules find it, naming the pins:

```nsm fails
import src::zmcubus::Zmcubus
import src::zparts::Zflash
import std::sources::Rail

board Crossed {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Zmcubus
    .flash: Zflash
    .supply ~ .u.vdd
    .supply ~ .flash.vdd

    .u.p00.function &= "SPIM0_SCK"
    .u.p01.function &= "SPIM0_MOSI"
    .u.p02.function &= "SPIM0_MISO"
    .u.p03.function &= "APP_GPIO"
    .u.p00 ~ .flash.spi.sclk
    .u.p01 ~ .flash.spi.miso
    .u.p02 ~ .flash.spi.mosi
    .u.p03 ~ .flash.spi.cs
}
```

```text
error: net `net-flash.spi.miso` joins two different lines of a `Spi`: `miso` (`flash.spi.miso`) and `mosi` (`u.p01`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `u.p01` is on the net of `miso`
   --> main.nsm:12:5
   |
12 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
error: net `net-flash.spi.mosi` joins two different lines of a `Spi`: `miso` (`u.p02`) and `mosi` (`flash.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `flash.spi.mosi` is on the net of `miso`
   --> main.nsm:12:5
   |
12 |     .flash: Zflash
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
    .u.p00.function &= "UARTE0_TX"
    .u.p01.function &= "UARTE1_TX"
    .u.p10.function &= "APP_GPIO"
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
    .u.p00.function &= "UARTE0_TX"
    .u.p01.function &= "UARTE1_TX"
    .u.p02.function &= "UARTE2_TX"
}
```

```text
error: `u` (`Zmcu3`) uses 3 of the peripherals of `UARTS`, which allows 2 at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:10), UARTE1 on `u.p01` as UARTE1_TX (main.nsm:11), UARTE2 on `u.p02` as UARTE2_TX (main.nsm:12); `UARTS` lists UARTE0, UARTE1 and UARTE2
   --> main.nsm:12:5
   |
12 |     .u.p02.function &= "UARTE2_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
    .u.p10.function &= "APP_GPIO"
    .u.p11.function &= "UARTE0_TX"
}
```

```text
error: `u.p10` and `u.p11` of `Zmcu3` are both in use, but they share `REFERENCE`, which allows 1 at a time: `u.p10` as APP_GPIO (main.nsm:9) and `u.p11` as UARTE0_TX (main.nsm:10)
   --> main.nsm:10:5
   |
10 |     .u.p11.function &= "UARTE0_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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

The table is also checked against the KiCad symbol and footprint the part is exported with. `build` and `fab` stop when a pad of the table is not a pad of the footprint or a pin of the symbol, when a footprint pad or symbol pin belongs to no pin of the table, when the symbol names the pin otherwise than the table's datasheet name (after the `pin_names` of `kicad.toml`), or when the symbol's electrical type does not suit the pin's kind. Each message names the part and both sides, for example `pad 3 is the input pin `en` (`EN`) in the part, but pin 3 of the symbol `tiny:Chip` is named `ENABLE``. This is a fact about your libraries and not about the circuit, so `check` does not report it.

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
error: `.x` is an output named `X` in `pins "B"` but an input named `X` in `pins "A"`; a pin has one kind and one datasheet name in every package
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
error: unknown pin kind `tristate`; the kinds are power, input, output, bidirectional, passive, no_connect and tri_state
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

## Catalogs and how the solver picks parts

A catalog is the list of real parts that a part's fields are matched against. Each real part is a candidate with one value per column. `catalog resistor { .resistance, .max_power, .package }` makes `resistance`, `max_power` and `package` the columns of a resistor.

The solver works in four steps.

1. Propagate. Connected interfaces hold one value, so the solver narrows them to what all of them allow. A field that an equation defines takes the computed value.
2. Pick. For each part with an unset catalog column, take candidates in catalog order. A candidate fits when each column value lies within the range the design allows. Use the first candidate for which every relation that mentions the part holds, and every relation that mentions a value worked out from it (a named ratio, a module's declared current) once the parts that value also comes from are chosen (ADR 0057).
3. Verify. Check every relation again with the picks in place.
4. Report. Whatever fails is reported with the numbers. Whatever could not be evaluated is listed as not evaluated.

Rules.

- Order is the policy. The solver takes the first candidate that works, so the order of the rows in the catalog's file decides which part you get, and nothing sorts it. The JLCPCB catalogs list Basic parts first (no feeder fee), then ascending price, then ascending value. The result is the cheapest Basic part that works.
- The check is the worst case. A requirement on a result must contain the whole range the inputs can produce, not a typical value.
- Parts that share a relation are searched together, depth first, and a failed choice backtracks. The search goes back to the nearest part that a failure depends on (the parts whose values the failed relation is worked out from), not to the part just before it, and when it fails it names the parts the failure is made of and the relation that cannot be met, `no combination of candidates satisfies `a` and `z` together, whatever the other parts of the group are: the closest, ... fails because ...`, with no word about parts that played no part in it (ADR 0057). The search stops after 250,000 combinations, or after as much work as a board of its size allows (a trial of a large board costs more than one of a small board, so a large one stops sooner: both are counts, never seconds, so the answer is the same on every machine), and says so: `gave up after trying 250000 combinations of candidates for `a` and `m` and `z`; a bound reads values of all of them, so no part could be blamed alone. State a value for one of them, narrow a window that a bound on them reads, or split the group so that fewer parts share a bound`. It names the parts it was choosing together (the first twelve, and how many more), and a stop for work adds that it was the work and not the number of combinations. It only reaches that when the failure depends on every part (ADR 0057, 2026-11-07 amendment, issue #177). Parts that read only the same stated value (a rail whose voltage the board states) and nothing else in common are searched apart, so a part that fails does not undo the picks of the others. Before a group is searched, a part that no candidate can serve whatever the others are (every candidate that fits is refused by a check that only its own choice decides; for a bank, at every count it may have, and a check on what a capacitor holds at its voltage counts as decided by the part that sets that voltage too, so it does not blame the bank) is named at once: `no resistor candidate satisfies t10: the closest of the 343 that fit, ... fails because ...` (ADR 0057).
- A column the design leaves alone takes the candidate's value, so `.r1: Resistor {}` works.
- A part is picked after the parts whose picks decide what it needs, never by the order of names (below).
- Narrow only what the design needs. A tighter range removes candidates.
- A part number is in a catalog only with a recorded source: a row that names a part has a `source` cell, or the file does not load (next section).
- A catalog may be keyed by one of its columns: `catalog mcu by .mpn { .mpn, .package }`. The solver does not choose a row of a keyed catalog for you, because the first fitting row is right for a regulator and wrong for a microcontroller. Name the part with `.u.mpn &= "..."`, which leaves one row. If you do not, the message lists the candidates: `` `u` is an `Mcu32`; name the part with .mpn: MCU32-A, MCU32-B ``.
- A catalog's `package` column picks the part's pin table. The pads are in the table (previous section), not in the catalog. A row whose package lacks a pin that the board connects is skipped, and the message says so when nothing else is left.

A bound on a member that a part is held to reaches the part (ADR 0065). A module that writes `.r.resistance &= .balance` holds the resistor to the member `.balance`, and a board that writes `.lamp.balance >= 20kohm` has bounded the member, which is not a value: nothing flows from it into the resistor. The solver reads the bound as a bound on the resistor too, since `&=` puts the resistor's value inside the member's, and a bound on all of the member is a bound on every part of it. It does so for a member that has no value yet, along a chain of such links, and for `==` only when the left side has no range of its own (an equation puts its right side inside the left side's allowance, so a range on the left says nothing about the right). The pick then takes the first row that meets the bound, tolerance included (`20kohm +/- 1%` is not always at least 20 kohm), and a part that is not a pick is checked after the picks, with the numbers. The solver adds nothing but this refusal: no range and no value changes, so a design that meets what it states is never refused.

```nsm solve report
import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Bounded {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .lamp.balance_resistance >= 20kohm
}
```

```text
board Bounded
  R2   Resistor        22k             Resistor_SMD:R_0402_1005Metric                             C25768 Basic
  R1   Resistor        22k             Resistor_SMD:R_0402_1005Metric                             C25768 Basic
  net GND: R2.2
  net VCC: R1.1
  net net-lamp.bottom.a: R1.2, R2.1
```

When no row meets the bound, the message says which statements make it a bound on the part:

```nsm fails
import std::parts::passive::Resistor

module Fixed {
    .value: Resistance
    .r: Resistor
    .r.resistance &= .value
}

board Unreachable {
    .fixed: Fixed
    .fixed.value >= 20kohm
    .fixed.value <= 10kohm
}
```

```text
error: no `resistor` candidate satisfies `fixed.r`: the closest of the 219 that fit, 20kΩ 1% 0402 C25765, fails because fixed.r.resistance must always be at least 20kΩ, but 19.8kΩ..20.2kΩ is not always above 20kΩ: it is held to `fixed.value` (`.r.resistance &= .value`, main.nsm:6), and `.fixed.value >= 20kohm` (main.nsm:11) bounds it
   --> main.nsm:11:5
   |
11 |     .fixed.value >= 20kohm
   |     ^^^^^^^^^^^^^^^^^^^^^^
```

A value that two equations state is held to a window that the board stated, whatever range the part's own member has (ADR 0065). The frequency of an oscillator is worked out from its timing resistor, and the board states the window it wants; the module links the two with `.osc.fsw == .fsw`. Both lines are equations on `.osc.fsw`, and neither is its allowance: a stated range on `.fsw`, which no pick changes, is, and what the circuit works out must lie inside it. A range that the oscillator's own member `.rt_resistance` has makes the frequency a range before any resistor is picked, and the window is then compared with the pick as it is without that line: a resistor outside the window is refused where it is picked, and a stated resistor outside it fails the build with the numbers. Two values that the picks change and that nothing bounds only have to share a value.

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

module Oscillator {
    .rt_resistance: Resistance
    .rt_resistance &= 10kohm..230kohm
    .fsw_nominal: Frequency
    .fsw_nominal == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)
    .fsw_tolerance: Number
    .fsw_tolerance &= 0.925..1.075
    .fsw: Frequency
    .fsw == .fsw_nominal * .fsw_tolerance
}

module Timer {
    .fsw: Frequency
    .supply: Rail
    .osc: Oscillator
    .timing: Resistor { .resistance &= 15kohm +/- 1% }
    .supply.pos ~> .timing ~> .supply.neg
    .osc.rt_resistance == .timing.resistance
    .osc.fsw == .fsw
}

board Probe {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .timer: Timer
    .timer.supply ~ .supply
    .timer.fsw &= 0.9MHz..1.1MHz
}
```

```text
error: `timer.osc.fsw` must stay within 900kHz..1.1MHz, but (timer.osc.fsw_nominal * timer.osc.fsw_tolerance) works out to 2.113MHz..2.494MHz
   --> main.nsm:12:5
   |
12 |     .fsw == .fsw_nominal * .fsw_tolerance
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A part is picked after the parts whose picks decide what it needs. A capacitor behind a diode that the solver picks has the bias of the rail it sits on, which is the supply's voltage less the diode's forward voltage, and that is known only once the diode is chosen. The solver therefore chooses the diode first, and the board below builds with its parts named in either order. A part that needs a value that no pick can set is not delayed: it fails as it would alone, with a message that says which field to state. Ties go by path, and the same inputs give the same board.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer
import std::parts::diode::Schottky
import std::parts::passive::Capacitor
import std::sources::Rail

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .supply.neg ~ .load.neg
    .z_diode: Schottky {}
    .supply.pos ~> .z_diode ~> .load.pos
    .load.voltage == .supply.voltage - .z_diode.drop
    .a_cap: Capacitor { .effective_capacitance >= 1uF }
    .load.pos ~> .a_cap ~> .load.neg
}
```

Two groups of parts that read each other's values through a bound (a window that the tap of one divider must stay inside, set by another divider's output) are searched one after the other, in path order, and the one that comes first is picked while the bound waits for the other. Its first fit can leave nothing for the other, and the board would then build or fail by what its instances are called. When a group fails and an earlier group shares a bound with it, the solver searches them again, the failed one first, so that the others are judged by the bound against its picks. If that does not find parts for all of them the first failure and its message stand. Parts that were found this way are the parts that the other order of the names gives (ADR 0065).

A bound that still waits for a part that is chosen later is not judged, but it turns a candidate away when no value in the ranges as they stand can meet it: the parts chosen later only narrow the ranges, so they cannot meet it either. A divider that draws far more than a quiescent bound allows is refused for it even while the input voltage of the stage waits for another group. A candidate that the bound could still hold for, with a narrower range, is kept, and the check after the pick is the judge (ADR 0065, 9.5). Among the candidates that are kept, one for which the bound holds over the whole range as it stands is chosen before one for which it holds only for some values, and within each of the two the first in catalog order is taken; if every candidate holds only for some values, the first of them is taken (ADR 0065, 9.6).

The JLCPCB catalogs hold 1% resistors (0402 E24 from 10 ohm to 1 Mohm, Basic 0603, 250 mW 0805), Basic 0402 and 0603 ceramic capacitors, LEDs in five colors, one fixed-output linear regulator, and a few Schottky, Zener and TVS diodes, ferrite beads, power inductors, resettable fuses, N-channel MOSFETs and crystals (ADR 0025). A resistor's rating, `max_power`, is a column: a row with an empty cell is never offered, because the resistor's own check uses it. An empty cell is no obstacle for a column the design never mentions (`temperature_coefficient` is empty in every standard row), and a design that bounds the column is offered only the rows that state it. `data/jlcpcb/README.md` lists every row and its source.

### Catalog files

The rows of a catalog are data, not code. A catalog is one CSV file named after it: `resistor.csv` is the `resistor` catalog. The standard catalogs are `data/jlcpcb/catalog/*.csv` in netsmith's source and ship inside the program. A project adds files of its own, which is how it gets a part netsmith does not list. Every `*.csv` file in the project's `catalogs/` folder is one, named by its file name: `catalogs/mcu.csv` is the catalog `mcu` (ADR 0022). `netsmith.toml` needs no line for it, because the key `catalog_dirs` has the default `["catalogs"]` (`netsmith init` writes it out, so that a reader sees it):

```toml
[project]
name = "blink"
top_level = ["src/main.nsm"]
catalog_dirs = ["catalogs"]
```

- `catalog_dirs` lists folders, relative to the folder of `netsmith.toml`; `..` is allowed, so projects can share one (`catalog_dirs = ["catalogs", "../shared"]`). `catalog_dirs = []` reads no folder. A folder that does not exist holds no catalogs.
- Only the files directly in a folder are read; **sub-folders are not**. Files that do not end in `.csv` are ignored. The files are read in file-name order, folders in the order the list gives, so the result does not depend on the file system.
- A `.csv` file in a folder whose name is not a catalog name (letters, digits and `_`, not starting with a digit: `my-parts.csv`, `2026.csv`) is an error that names the file, not a file that is skipped: a data file that is not a catalog goes in another folder. The check is on the name only; a `.csv` that is not a catalog but is named like one is read as a catalog and fails with the line of the first thing wrong (an unknown column, a missing `source`).
- `catalogs = ["extra/ldo.csv"]` still lists single files, from any folder inside the project, each named after its catalog, for a file that is not in a `catalog_dirs` folder. A file that a folder and the list both name, and two files of one catalog (`catalogs/ldo.csv` and `../shared/ldo.csv`), are errors that name both files: a catalog is one file. A project that listed `catalogs/x.csv` before this key existed removes the entry (or writes `catalog_dirs = []`).
- A running editor reads the folders again with each build, so a file that is added to a folder, or removed, is noticed without a restart. A change to `catalog_dirs` itself in `netsmith.toml` is read when the editor starts.

The first line of a file names the columns. Two kinds of column can be there, in any order.

- The columns of the part: the paths of the fields in its `catalog X { ... }` declaration, without the leading dot (`resistance`, `package`, `vout.voltage`, `mpn`). A cell of a quantity column is a value as the language writes it, with its unit: `10kohm +/- 1%`, `1.8V..5.5V`, `0.3V`. It must be in the dimension of the field. A cell of a text column is its text as it is, without quotes: `0402`, `SOT-23-5`. An empty cell means the row has no value there: the row fits no design that asks for one, and, for any column a design needs (the package, the resistance, a rating), the row is never offered at all. Only a column that the part's `catalog` statement marks `optional` (ADR 0078, below) may be empty in a row that is offered. The standard parts mark the four that were added after the files were written: `temperature_coefficient` of `resistor`, `rms_current` of `inductor`, `dielectric` of `capacitor` and `breakdown_voltage` of `tvs`. Such a row is offered to a design that says nothing about the field, and a design says something by narrowing it, by a relation that names it, by a `~` that joins it or by a balance of currents that sums it.
- The fixed columns, all optional except as noted: `lcsc` (the supplier's number), `mpn` (the manufacturer's; also a column of the part if its catalog lists `.mpn`), `library` (`Basic`, `Extended` or `Preferred`), `height` (how tall the part's body stands above the board, one length such as `2.62mm`, the greatest the manufacturer's document allows; the part's `.height` is it when the design states none, ADR 0094, and an empty cell means the height is unknown; see Height limits), `price` and `stock` (JLCPCB's snapshot, entered by hand; the solver does not read them, so a row is offered whatever its `stock` says, and the build notes a pick whose `stock` is 0; the bill of materials shows them only when no supply provider has the part, ADR 0047), `price_breaks` (`1:0.0019 1000:0.0015`, the price for one piece from each quantity; entered by hand and read by nothing), `date` (`2026-10-31`, the day the `price` and `stock` were read), `checked` (`2026-11-02`, the day `catalog refresh` last found the row's `mpn` and `package` equal to LCSC's record; empty if never, read by nothing), `name` and `label` (what a message calls the row, and what the schematic's value field shows; both default to the `mpn`, then the `lcsc`), `notes`, and `source`. **A row with an `lcsc` or an `mpn` must have a `source`**: the URL or text that says where its numbers were read. A row with neither is a made-up part and needs a `name`.

`price`, `stock` and `library` are JLCPCB's numbers and `date` is the day they were read: they are a hand-entered snapshot (the standard catalogs that ship in netsmith leave `price`, `stock`, `date` and `checked` empty: read a part's price and stock from the supplier before ordering), and `netsmith catalog refresh` never writes them (it asks LCSC, never JLCPCB, and only verifies the row and records the day in `checked`; see Commands). `netsmith catalog age` lists the rows whose price and stock were read long ago and those whose price and stock are unknown. The price and stock of a part come from a supply provider (the `[supply]` table of `netsmith.toml`, ADR 0047, provisional): `provider = "file"` with `file = "supply/quotes.csv"`, a CSV with the columns `lcsc` or `mpn`, `channel` (`jlcpcb-assembly` or `lcsc-retail`: the two stocks differ, and the answer says which it is), `stock`, `price_breaks` (`1:0.0100 100:0.0050`), `currency` and `date`; `channel` and `cache_hours` choose the stock shown and how long a cached answer is used. With no provider, or one that does not list the part, price and stock are unknown in the bill of materials, never guessed and never 0. The manifest has no field for a key or a password. The solver never asks; it reads the rows and no price or stock.

A file named like a standard catalog adds its rows after the standard ones, so the standard parts are tried first. A part, by `lcsc` or by `mpn`, is in a catalog once; a file that repeats a standard part is refused. A capacitor's `effective_capacitance` cell is its curve of bias (ADR 0013): `derate capacitance by bias: 0:1 5:0.5` is the `capacitance` of the row after the part keeps 1 at 0 V and 0.5 at 5 V, and nothing after the colon means that no curve is published for the part.

**Generic parts** (ADR 0047, provisional). If no row of a catalog works, the solver may pick a generic part, which has no supplier or part number. The rows are tried first, so a design that built keeps its parts. The series is `series = "E24"` (the default), `"E12"`, `"E96"` or `"none"` in the `[project]` table of `netsmith.toml`; capacitors and inductors use it too, except that E96 becomes E24 for them.

- A **resistor** is a value of the series with a 1% tolerance in a 0402, 0603 or 0805 package at that package's usual power rating (62.5, 100 and 125 mW).
- A **ceramic capacitor** is a value of the series from 1 pF to 100 uF, a tolerance of 20%, 10% or 5%, and a 0402, 0603 or 0805 package. It has **no bias curve**, so its effective capacitance is unknown: a generic capacitor is never used where the design asks for `.effective_capacitance`, and it resolves only to a row that has a recorded curve (owner decision, 2026-11-03: no conservative curve and no derating rule). Its rated voltage is the least that a relation of the design demands (`.max_voltage >= .bias * 1.5`).
- An **inductor** is a value of the series from 1 nH to 1 mH, a tolerance of 30%, 20% or 10%, and a 0402, 0603, 0805 or 1210 package. Its saturation current is the least that any relation demands, the stage's included (`.inductor.current + .ripple / 2 <= .inductor.saturation_current` is worked out after the part's marked inductance has set the ripple, ADR 0057), so it is a requirement on the row that will stand for the part; its DC resistance the most that a relation allows. A rating that nothing bounds is left unknown, and the check that mentions it is listed as not evaluated.
- An **LED** is a package (0402, 0603 or 0805) with the colour and the forward voltage the design states, and is used only where the design states the forward voltage, since other values are worked out from it.

When the bill of materials or `fab` is made, each generic part is resolved to the row of the catalog with the same package and value, a tolerance no wider and every rating it was given no lower (and, for a capacitor, a recorded bias curve): JLCPCB Basic before Preferred before Extended, then the order of the file, passing over a row that a supply provider says has no stock. A generic part no row stands for is *unresolved*: the bill of materials lists it with the nearest rows and `fab` refuses it by name. The message also says, for each nearest row, whether the board's checks pass with that row in the part's place (the part's own stated value left out, every relation kept), at most 30 solves for a board; it ends "Change the value in the design and build again", because a new value is checked against every relation by the build. The nearest rows are ranked by how near they come to the rating that fails (a project row at 5.3 A for a need of 6.05 A before a standard row at 3.6 A), then by package and value. `build` says the same about every assembled generic part that is unresolved, as a guideline finding (see "A generic part that no row can be ordered for").

A project's file, `catalogs/resistor.csv`, adds a 316 ohm resistor to the standard resistors, and a board asks for it:

```csv file=catalogs/resistor.csv set=own
mpn,resistance,max_power,package,library,price,stock,date,source
ACME-R316,316ohm +/- 1%,62.5mW,0402,Basic,0.0011,5000,2026-10-31,https://example.invalid/acme-r316.pdf
```

```nsm solve report catalogs=own
import std::parts::passive::Resistor

board OwnPart {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
board OwnPart
  R1   Resistor        ACME-R316       Resistor_SMD:R_0402_1005Metric                             no LCSC number
```

The solver does not read `stock`, so a row that states a stock of 0 is still picked when it is the first that fits. The build says so in a note, and the editor shows the same line among its problems: the part's designator and path, the row's LCSC number and manufacturer's number, the file and line of the row, and what to do (choose another row, or state a different value). A part that the standard `data/jlcpcb/considered.csv` records as out of stock when it was read gets the same note when a project's row offers it and states no stock. An empty `stock` cell states nothing and gets no note, and a positive stock gets none. The note is not a guideline and never stops a build, and the pick is the same with or without it.

```csv file=catalogs/resistor.csv set=sold_out
lcsc,mpn,resistance,max_power,package,library,stock,source
C900001,ACME-R316,316ohm +/- 1%,62.5mW,0402,Extended,0,https://example.invalid/acme-r316.pdf
```

```nsm solve stock catalogs=sold_out
import std::parts::passive::Resistor

board SoldOut {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
R1 (`r1`) is C900001 (ACME-R316), and its catalog row (catalogs/resistor.csv:2) states a stock of 0: the solver does not read stock, so it picked the part; choose another row of the catalog or state a different value, and read the supplier's stock before ordering
```

A resistor's `temperature_coefficient` is a column too: how far the resistance moves per kelvin, as a fraction (`0.00005` is 50 ppm/K; take the datasheet's largest figure). The standard rows leave it empty, because no value goes into a standard catalog without a recorded source. A design that does not mention the field picks rows as before. A design that limits it, `.r1.temperature_coefficient <= 0.0001`, is offered only the rows that state it, so it needs a row of a project's file (a file that leaves the column out of its first line is read as empty there, which is why the file above needs nothing new):

```csv file=catalogs/resistor.csv set=tempco
mpn,resistance,max_power,package,temperature_coefficient,library,source
ACME-R100T,100ohm +/- 1%,62.5mW,0402,0.00005,Basic,https://example.invalid/acme-r100t.pdf
```

```nsm solve report catalogs=tempco
import std::parts::passive::Resistor

board Stable {
    .r1: Resistor {
        .resistance &= 100ohm +/- 1%
        .temperature_coefficient <= 0.0001
    }
}
```

```text
board Stable
  R1   Resistor        ACME-R100T      Resistor_SMD:R_0402_1005Metric                             no LCSC number
```

With only the standard rows, the same limit has nothing to pick, and the message says why:

```nsm fails
import std::parts::passive::Resistor

board Unstated {
    .r1: Resistor {
        .resistance &= 100ohm +/- 1%
        .temperature_coefficient <= 0.0001
    }
}
```

```text
error: the `resistor` catalog has nothing for `r1`, which needs resistance within 99Ω..101Ω (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 219 for `temperature_coefficient`)
  --> main.nsm:4:5
  |
4 |     .r1: Resistor {
  |     ^^^^^^^^^^^^^
```

A part that is an IC is named by its key. The catalog of the `Mcu32` above is `catalogs/mcu.csv`:

```csv file=catalogs/mcu.csv set=ic
mpn,package,library,price,stock,date,source
MCU32-A,QFN-32,Extended,1.20,300,2026-10-31,https://example.invalid/mcu32-a.pdf
MCU32-B,QFN-32,Extended,1.45,120,2026-10-31,https://example.invalid/mcu32-b.pdf
```

```nsm fails catalogs=ic
import src::mcu::Mcu32
import std::sources::Rail

board WhichMcu {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
}
```

```text
error: `u` is an `Mcu32`; name the part with .mpn: MCU32-A, MCU32-B
   --> main.nsm:10:5
   |
10 |     .u: Mcu32 { .package &= "QFN-32" }
   |     ^^^^^^^^^
```

#### An optional column

A column that a row may leave empty says so with the word `optional` after it in the `catalog` statement (ADR 0078, provisional), the word that a member (ADR 0074) and a `pins` row already use for "may be absent". The standard library does it for `temperature_coefficient`, `dielectric`, `rms_current` and a TVS diode's `breakdown_voltage`, and a project's own part does it for a column of its own catalog. A fuse that records a surge rating only for some parts (invented, like the file below):

```nsm module=src::zfuse
/// A fuse with an optional surge rating. Illustrative; not a real part.
part Zfuse {
    .designator &= "F"
    .rated_current: Current
    .surge_current: Current
    pins "0805" {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    bridge .a ~> .b
    .package: Package
    catalog zfuse { .rated_current, .surge_current optional, .package }
}
```

The file leaves the cell empty for the row that does not record it, and a file written before the column existed may leave the whole column out:

```csv file=catalogs/zfuse.csv set=surge
mpn,rated_current,surge_current,package,library,source
ZF-1,1A,,0805,Extended,https://example.invalid/zf-1.pdf
ZF-2,1A,8A,0805,Extended,https://example.invalid/zf-2.pdf
```

A design that says nothing about the column is offered every row, the empty one too. This file has only that row, and the design builds:

```csv file=catalogs/zfuse.csv set=unrated
mpn,rated_current,package,library,source
ZF-1,1A,0805,Extended,https://example.invalid/zf-1.pdf
```

```nsm evaluate catalogs=unrated
import src::zfuse::Zfuse

board Plain {
    .f1: Zfuse { .rated_current &= 1A }
}
```

A design that states the column, by a bound, an `&=`, a `~` or a balance of currents, is offered only the rows that record it (ADR 0047, ADR 0058). With the first file it gets `ZF-2`:

```nsm evaluate catalogs=surge
import src::zfuse::Zfuse

board Surge {
    .f1: Zfuse {
        .rated_current &= 1A
        .surge_current >= 5A
    }
}
```

With the file that has no `surge_current` column it has nothing to pick, and the message counts the rows:

```nsm fails catalogs=unrated
import src::zfuse::Zfuse

board Surge {
    .f1: Zfuse {
        .rated_current &= 1A
        .surge_current >= 5A
    }
}
```

```text
error: the `zfuse` catalog has nothing for `f1`, which needs rated_current within 1A (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 1 for `surge_current`)
  --> main.nsm:4:5
  |
4 |     .f1: Zfuse {
  |     ^^^^^^^^^^
```

The columns the statement does not mark are as they were: a row with an empty cell is never offered, and the file must have the column. The marker is refused on a column the part does not have (the same message as for any column), and on the key of a keyed catalog (`catalog mcu by .mpn { .mpn optional }`), because a row with no key could not be chosen.

A file is read when it is used, and a mistake names the file and line. A row with a part number and no source, and a part twice:

```csv fails file=catalogs/resistor.csv
mpn,resistance,package
ACME-R316,316ohm +/- 1%,0402
```

```text
catalogs/resistor.csv:2: the row for mpn ACME-R316 has no source; every part in a catalog needs the URL or text it was read from in its `source` column
```

```csv fails file=catalogs/resistor.csv
mpn,resistance,package,source
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/a
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/b
```

```text
catalogs/resistor.csv:3: the mpn `ACME-R316` is already a row at catalogs/resistor.csv:2; a part is in a catalog once
```

A column the part does not list, and a value in the wrong unit, are found when a board uses the catalog, because only then is it known what the columns hold:

```csv file=catalogs/resistor.csv set=typo
mpn,resistence,package,source
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=typo
import std::parts::passive::Resistor

board Typo {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:1: `resistence` is not a column of `catalog resistor`, which lists .resistance, .max_power, .package, .temperature_coefficient
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

```csv file=catalogs/resistor.csv set=unit
mpn,resistance,max_power,package,source
ACME-R316,316V +/- 1%,62.5mW,0402,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=unit
import std::parts::passive::Resistor

board Unit {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:2: `resistance` of `catalog resistor`: `316V +/- 1%` is in V, but this column holds Ω
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

Messages.

When no candidate fits the constraints, the catalog has nothing:

```nsm fails
import std::parts::passive::Resistor

board NoValue {
    .r1: Resistor { .resistance &= 12.34kohm +/- 0.1% }
}
```

```text
error: the `resistor` catalog has nothing for `r1`, which needs resistance within 12.327kΩ..12.353kΩ
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 12.34kohm +/- 0.1% }
  |     ^^^^^^^^^^^^^
```

The message points at the line that declares the part in your board, and names it by its path (`r1`, or `stage.regulator` when it is inside a module). When several parts are picked together and one of them has no candidate, that part is the one named, whatever the others could do.

When candidates fit but none keeps every relation true, the message names the closest one and the relation that failed:

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

board TooTight {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .top: Resistor {}
    .bottom: Resistor { .resistance &= 10kohm +/- 1% }
    .out: Rail { .voltage &= 3.3V +/- 5% }
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .top.b ~ .out.pos
    .supply.neg ~ .out.neg
    .out.voltage == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
}
```

```text
error: no combination of candidates satisfies `bottom` and `top` together: the closest, bottom = 10kΩ 1% 0603 C25804 and top = 5.1kΩ 1% 0603 C23186, fails because `out.voltage` must stay within 3.135V..3.465V, but ((supply.voltage * bottom.resistance) / (top.resistance + bottom.resistance)) works out to 3.124V..3.501V
   --> main.nsm:16:5
   |
16 |     .out.voltage == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Read it this way. The closest choice was 10 kohm over 5.1 kohm. Its output spans 3.124 V to 3.500 V, wider than 3.135 V to 3.465 V. To fix it, loosen the requirement, use tighter parts, or change the circuit. The solver will not pick a part that only works at the typical value.

## Interfaces

An interface is a typed set of pins and quantities that is connected as one. `std::sources::Rail` is one: a positive pin, a negative pin, a voltage, a current and a peak voltage. Two interfaces of one definition join with `~`.

```nsm
import std::parts::passive::Resistor

/// A pair of wires that carry a differential signal.
interface Pair {
    .p: Pin
    .n: Pin
    .swing: Voltage
}

module Termination {
    .line: Pair
    .r: Resistor { .resistance &= 100ohm +/- 1% }
    .line.p ~> .r ~> .line.n
}

board Link {
    .bus: Pair { .swing &= 400mV }
    .term: Termination
    .bus ~ .term.line
}
```

Rules.

- An interface may hold pins, quantities and other interfaces. It may hold no parts.
- An interface with no pins is a bundle of quantities. A manufacturing process is one.
- `a ~ b` joins the pins that have the same name and makes equal the values that have the same name. It does not equate `current` fields, except for a module's own rail and the rail of a part inside it (see Load current).
- Joining two different interfaces is an error (shown in Connections).

## Generics and providers

A `part` is exact: `.rf: AcmeMicroCoax` is that connector and no other. Often a board does not care which part it is, only what the part must be: a coaxial receptacle that reaches 1.61 GHz. A **generic** states that, and the parts and modules that **provide** it say they can stand in its place (ADR 0068). The board writes a **use**, `.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }`, and the solver picks the first provider for which the whole design holds, and names it.

The layers are part, then generic, then module: a board can name one part, name a generic and put requirements on it, or use a module built from either. Each is complete on its own.

### A generic

A generic is written like a part, with what is always true of every provider and nothing that only one has: pins with their kinds, values with their types, and the checks that hold for all of them. It has no pads and no package.

```nsm module=src::rf
/// A coaxial receptacle that carries an antenna lead off the board.
generic CoaxReceptacle {
    .designator &= "J"

    pins {
        .signal passive required
        .shield passive required
    }

    /// The characteristic impedance. Every receptacle of this generic is 50 ohm.
    .impedance: Resistance
    .impedance &= 50ohm
    /// The highest frequency it is specified for.
    .max_frequency: Frequency
    /// The manufacturer's number, so that a design can name the part.
    .mpn: String
}
```

Rules.

- A generic holds values, pins and interfaces, and statements (`&=`, `?=`, `==`, `>=`, `<=`). It holds no parts and no modules, and no connections between parts.
- Its `pins` table has rows of a kind and no datasheet name and no pads: `.signal passive required`. The kind is what every provider's row must say, because the kind decides what the checker asks of the board. `required` and `optional` hold for the provider's row too, and a provider may leave the word out.
- A relation or a narrowing in a generic holds for every provider and for every use. The solver judges it with the values of whichever provider is tried.
- `.mpn: String` is an ordinary value. A provider whose catalog lists `.mpn` as a column has the number in its row, and a board that states `.mpn &=` finds that provider.
- A doc comment (`///`) goes before a declaration, not before a statement; use `//` there.

### A provider

`part X provides G` (or `module X provides G`) says that `X` can stand wherever `G` is used. `X` is a part as written today: its own pin table, its own numbers, its own one-row catalog. It has every member and every statement of the generic and does not declare them again.

```nsm module=src::parts::acme_micro_coax
import src::rf::CoaxReceptacle

/// ACME-MC-1, a surface-mount micro coaxial receptacle, DC to 6 GHz. Invented.
part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        //  pin     kind     name      pads
        .signal     passive  "SIGNAL"  "1"
        .shield     passive  "GND"     "2" "3"
    }
    .max_frequency &= 6GHz

    .package: Package
    catalog acme_micro_coax { .mpn, .package }
}
```

```csv file=catalogs/acme_micro_coax.csv set=coax
mpn,package,library,source
ACME-MC-1,ACME_MC_1,Extended,https://example.invalid/acme-mc-1.pdf
```

A second provider, a plainer part that stops at 1 GHz:

```nsm module=src::parts::acme_basic_coax
import src::rf::CoaxReceptacle

/// ACME-BC-1, a through-hole receptacle, DC to 1 GHz. Invented.
part AcmeBasicCoax provides CoaxReceptacle {
    pins {
        .signal     passive  "SIGNAL"  "1"
        .shield     passive  "GND"     "2" "3"
    }
    .max_frequency &= 1GHz

    .package: Package
    catalog acme_basic_coax { .mpn, .package }
}
```

```csv file=catalogs/acme_basic_coax.csv set=coax
mpn,package,library,source
ACME-BC-1,ACME_BC_1,Extended,https://example.invalid/acme-bc-1.pdf
```

The project has both catalog files in its `catalogs/` folder and lists both parts in `kicad.toml`, as for any hand-written part. A provider whose own names differ from the generic's states the generic's names with the aliases and equations the language already has, and says nothing more:

```nsm module=src::parts::other_coax
import src::rf::CoaxReceptacle

/// A receptacle whose pin and top frequency have other names. Invented.
part OtherCoax provides CoaxReceptacle {
    pins {
        .rf      passive  "RF"   "1"
        .shield  passive  "GND"  "2"
    }
    .signal ~ .rf               // the generic's name for the pin this table calls .rf
    .fmax: Frequency
    .fmax &= 3GHz
    .max_frequency == .fmax     // the generic's name for the value this part calls .fmax

    .package: Package
    catalog other_coax { .mpn, .package }
}
```

Rules.

- A part or module may provide several generics: `part AcmeMicroCoax provides CoaxReceptacle, RfConnector`. Only a `generic` is provided; `board`, `system`, `panel`, `interface` and `enum` cannot say `provides`.
- Each pin of the generic has a row of the same kind in every `pins` table of the part, or is an alias of a pin that has a row of that kind: the row the alias is joined to is what the checker reads for the net, so it is the one compared (`.signal ~ .rf` with `.rf output` is refused where the generic says `.signal passive`, at the row of `.rf`).
- The providers of one generic are all parts or all modules: a generic is placed as the kind its providers have. A generic that has a `pins` table is provided by parts.
- A provider is checked where it is written, as any part is, so an unused provider's mistakes show.
- No two providers of one generic have the same name, in whatever files they are: a part is known by its name in the bill of materials, in the `[[part]]` entries of `kicad.toml` and in the notes, so two would share a kind and a mapping. The later one (by the order below) is refused at its own name, with both module paths.
- A provider does not contain a use of the generic it provides, directly or through the parts and modules inside it: a pick that could take the provider being made would make it again inside itself. A provider that wraps `BuckConverter` in `.inner: BuckConverter` is refused at that line with the cycle named (`AFilteredBuck > AFilteredBuck`), whatever its name sorts as; wrap a part, or a module that does not provide the generic. Nor does any definition contain itself, directly or through another.

### A use

A board or module declares a member with the generic's type and puts requirements on it. The import of a provider's file is what makes the provider a candidate: it is not named in the board.

```nsm evaluate picks catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax   // not used by name: brings its file's providers in scope
import src::parts::acme_micro_coax::AcmeMicroCoax   // the same

module AntennaFeed {
    .signal: Pin
    .gnd: Pin

    // Any receptacle specified up to the radio's top frequency. The solver picks one.
    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}

board Radio {
    .feed: AntennaFeed
}
```

```text
feed.rf is a CoaxReceptacle: picked AcmeMicroCoax; passed over AcmeBasicCoax (no `acme_basic_coax` candidate satisfies `feed.rf`: the closest of the 1 that fit, ACME-BC-1, fails because feed.rf.max_frequency must always be at least 1.61GHz, but 1GHz is not always above 1.61GHz)
```

Which providers are candidates, and in what order:

- A candidate is a provider in a file that the board's file imports, directly or through other files, or in the board's own file. The import list is the list: no second list is kept. An import used only to bring providers into scope is a plain import, and a comment says so.
- The order is fixed: the generic itself first when it is its own provider (it has pads or a catalog), then the providers of the standard library, then the project's, each by module path and then by name. It never depends on where a definition is written in a file, so moving one changes nothing. The module path of a file is its path under the project's root (`src/main.nsm` is `src::main`) whichever way its name is typed (`./src/main.nsm`, an absolute path), and a file that imports the file being built finds that file, held once, not a second copy of it.
- The pick is the first candidate for which every check of the whole design holds. Nothing compares candidates: a design that cares states a requirement or names the type. A provider that leaves a value open that the use puts a bound on (`.rf: G { .max_frequency >= 1GHz }` and a provider that never states `.max_frequency`) cannot be judged, which is not holding: it is passed over for one that can be, and if none can, the first stands and the build names the bound as `not evaluated`. What cannot be judged is: a bound or an equation that waits on a value inside the use that nothing states (of the use itself, of an interface or a part inside it, such as the datasheet number of the IC inside a module provider), and a logic level or an absolute maximum that could not be checked because a pin of the use states no value for it. A value the use shares with the board (the board's rail that `.vin` is joined to, whose `.peak` the board leaves open) is the design's, and is no reason to try another provider.
- The pick depends on the sources, the catalog files and `netsmith.toml` only: not on the KiCad libraries, the mapping, stock or the time. A provider with no entry in `kicad.toml` is not skipped; the build fails at export and names it.
- A failing design is tried with the other providers only for the uses that matter: a use is varied alone first, and one whose other providers change nothing about what fails is left at its first. At most 64 combinations are tried. When the search stops at that bound without an answer, the build prints the errors of the design as it stands first (the first solve's), and then `gave up after trying 64 provider combinations for ...`, naming the first six uses it did not reach and counting the rest: the bound is never the only thing said about a design that has a mistake of its own.

The three ways to say what part the board wants:

```nsm-fragment
.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }   // a requirement: the solver picks
.rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }          // the part's number: the solver finds its provider
.rf: AcmeMicroCoax                                   // the provider itself: nothing is picked
```

The last line changes only the type. Every other line that mentions `.rf` still holds, because a provider has every member of the generic with the same names. A use reaches only the members the generic declares, so that a board works with every provider; a path to anything else is an error (below).

A part number picks the provider whose catalog has the row, and passes over the ones that do not:

```nsm evaluate picks catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}
```

```text
rf is a CoaxReceptacle: picked AcmeMicroCoax; passed over AcmeBasicCoax (the `acme_basic_coax` catalog has nothing for `rf`, which needs mpn within "ACME-MC-1")
```

When no provider works, one error at the line of the use names the closest provider (the trial with the fewest failures; ties in candidate order) and why each of the others fails:

```nsm fails catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle { .max_frequency >= 10GHz }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}
```

```text
error: no CoaxReceptacle provider satisfies rf: the closest of the 2 in scope, AcmeBasicCoax, fails because no `acme_basic_coax` candidate satisfies `rf`: the closest of the 1 that fit, ACME-BC-1, fails because rf.max_frequency must always be at least 10GHz, but 1GHz is not always above 10GHz; the others: AcmeMicroCoax (no `acme_micro_coax` candidate satisfies `rf`: the closest of the 1 that fit, ACME-MC-1, fails because rf.max_frequency must always be at least 10GHz, but 6GHz is not always above 10GHz)
  --> main.nsm:8:5
  |
8 |     .rf: CoaxReceptacle { .max_frequency >= 10GHz }
  |     ^^^^^^^^^^^^^^^^^^^
```

With one provider in scope the message is the one the part gives when it is written by its type, exactly as before.

When nothing provides the generic, the error is at the use and says how to bring a provider in scope:

```nsm fails
import src::rf::CoaxReceptacle

board Radio {
    .rf: CoaxReceptacle
}
```

```text
error: `rf` is a `CoaxReceptacle`, and no part or module in the files this file imports provides it; import the file of a provider, or name one as the type of `rf` (a part or module declared with `provides CoaxReceptacle`)
  --> main.nsm:4:5
  |
4 |     .rf: CoaxReceptacle
  |     ^^^^^^^^^^^^^^^^^^^
```

A use reaches only what the generic declares:

```nsm fails catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
    .rf.package &= "ACME_MC_1"
}
```

```text
error: `package` is not declared by `CoaxReceptacle`; a use of a generic reaches only the members the generic declares, so that it holds for every provider (declare it in `CoaxReceptacle`, or name a provider, `AcmeMicroCoax`, as the type)
   --> main.nsm:10:9
   |
10 |     .rf.package &= "ACME_MC_1"
   |         ^^^^^^^
```

A provider may not declare a member of the generic again. A narrowing or a relation is how it states its own number:

```nsm fails
generic Pad {
    .span: Length
    pins {
        .a passive
        .b passive
    }
}

part WidePad provides Pad {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .span: Length
    .span &= 24mm
}

board Plate {
    .p: Pad
}
```

```text
error: `span` is declared by `Pad`, which this part provides; do not declare it again: narrow it with `.span &= ...`, or state it with a relation
   --> main.nsm:14:5
   |
14 |     .span: Length
   |     ^^^^^
```

A provider's pin row has the generic's kind, and every pin has a row:

```nsm fails
generic Pad {
    pins {
        .a passive
        .b passive
    }
}

part ShortPad provides Pad {
    pins {
        .a input "A" "1"
    }
}
```

```text
error: `.a` is a passive pin in `Pad`, which `ShortPad` provides, but this row says an input; a provider's row has the generic's kind, because the kind decides what the checker asks of the board
   --> main.nsm:10:9
   |
10 |         .a input "A" "1"
   |         ^^^^^^^^^^^^^^^^
error: `pins` of `ShortPad` has no row for `.b`, which `Pad` declares as a passive pin; add `.b passive "NAME" "pad"`, or join it to a pin that has a row (`.b ~ .other`)
  --> main.nsm:9:5
  |
9 |     pins {
  |     ^^^^^^
```

Only a part or a module provides a generic:

```nsm fails
generic Pad {
    .span: Length
}

board Plate provides Pad {
}
```

```text
error: only a part or a module provides a generic; a board does not (`part X provides G`, `module X provides G`)
  --> main.nsm:5:13
  |
5 | board Plate provides Pad {
  |             ^^^^^^^^
```

### A module that provides a generic

An IC alone cannot stand for a regulator: its enable, boot and feedback pins need circuit that only it knows. A module can provide a module-shaped generic, which has values and interfaces and no `pins` table. The generic for a buck converter is next to `Buck` (the numbers an IC states for `BuckStage`), which is unchanged and is used inside the providers:

```nsm module=src::buck
import std::sources::Rail

/// A buck regulator with everything around it. A board states the input, the output and the
/// load; the provider is an IC with its stage.
generic BuckConverter {
    .vin: Rail
    .vout: Rail
    /// The highest voltage the input pin survives.
    .vin_abs_max: Voltage
    // A transient never goes past what the input survives.
    .vin.peak <= .vin_abs_max
}
```

`Zbk2` is this guide's invented IC (4.5 V to 6 V, 2 A), above. `Zbk9` is another, for 4.5 V to 60 V and 3.5 A:

```nsm module=src::zbk9
import std::parts::regulator::Buck
import std::sources::Rail

/// A 3.5 A buck converter for 4.5 V to 60 V. Illustrative; not a real part.
part Zbk9 {
    .vin: Rail
    .buck: Buck {
        .vref &= 0.8V +/- 1.5%
        .fsw &= 1.2MHz +/- 15%
        .min_input_voltage &= 4.5V
        .max_input_voltage &= 60V
        .max_output_current &= 3.5A
        .efficiency &= 0.85..0.95
        .min_inductance &= 1.5uH
        .min_inductor_saturation &= 3A
        .max_inductor_dcr &= 0.1ohm
        .min_input_capacitance &= 2uF
        .min_output_capacitance &= 10uF
    }
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)

    pins {
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .buck.sw.pos   power   "SW"   "4" "5"
        .buck.fb       input   "FB"   "6"
        .en            input   "EN"   "7"
    }
}
```

The providers, in one file, each an IC with its stage:

```nsm module=src::buck_ics
import std::parts::regulator::BuckStage
import src::buck::BuckConverter
import src::zbk2::Zbk2
import src::zbk9::Zbk9

/// Zbk2 with its stage.
module Zbk2Buck provides BuckConverter {
    .vin_abs_max &= 7V

    .controller: Zbk2
    .stage: BuckStage
    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
    .controller.en ~ .vin.pos
}

/// Zbk9 with its stage.
module Zbk9Buck provides BuckConverter {
    .vin_abs_max &= 65V

    .controller: Zbk9
    .stage: BuckStage
    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
    .controller.en ~ .vin.pos
}
```

A board that picks by requirement. The input is 12 V, which `Zbk2` cannot take, so the pick is `Zbk9Buck`:

```nsm evaluate picks
import src::buck::BuckConverter
import src::buck_ics::Zbk9Buck    // not used by name: brings every provider of the file into scope
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }

    .reg: BuckConverter {
        .vout.voltage &= 3.3V +/- 4%
        .vin_abs_max >= 40V
    }
    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
reg is a BuckConverter: picked Zbk9Buck; passed over Zbk2Buck (reg.stage.vin.voltage must always be at most reg.stage.buck.max_input_voltage, but 11.4V..12.6V is not always below 6V)
```

Forcing the choice, with no pick: `.reg: Zbk9Buck { .vout.voltage &= 3.3V +/- 4% }`. The inside of a module provider stays private: a board reaches `.reg.vin`, `.reg.vout` and `.reg.vin_abs_max`, never `.reg.stage`.

### The standard passives are generics

`Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal` and `Fuse` (`std::parts::passive`), `Schottky`, `Zener`, `Tvs` and `TvsBidirectional` (`std::parts::diode`), `NMosfet` and `PMosfet` (`std::parts::transistor`), `LinearRegulator` (`std::parts::regulator`) and `LevelTranslator` (`std::parts::logic`) are generics (ADR 0068, stage 2; ADR 0063 for the last two and `PMosfet`). Each is its own first provider: it has a pin table with pads and a `catalog`, so it stands for itself, with the same members, the same relations, the same catalog rows and, for a resistor, a capacitor, an inductor or an LED, the same series of values that have no row yet (ADR 0047). A design that adds no provider is read and solved exactly as when they were parts: the same picks, the same effort, the same kind names (`Resistor`) in the bill of materials and in `kicad.toml`. The parts of the standard library that have no catalog (`Diode`, `Switch`, the connectors) are not generics. The standard catalogs of `PMosfet` and `LevelTranslator` have no rows (see _Transistors_ and _Logic levels_): a project that uses either adds its own rows.

A project adds a provider of one of them as it adds any provider: a part that says `provides Resistor`, in a file that the board's file imports. The generic's catalog and pads are its own and are not read into a provider: the provider has its own pin table and its own one-row catalog, and what the generic declares (`.resistance`, `.max_power`, `.temperature_coefficient`, `.current`, `.drop`, `.package`), what it states (`.designator &= "R"`, Ohm's law, the power rating, `bridge .a ~> .b`) and the kinds of its pins. The provider does not declare `.package` again, which a part written by hand would.

```nsm module=src::parts::precision_thin_film
import std::parts::passive::Resistor

/// PTF-10K, an invented 10 kohm thin-film resistor with a tolerance of 0.1 %.
part PrecisionThinFilm provides Resistor {
    pins {
        .a  passive  "A"  "1"
        .b  passive  "B"  "2"
    }
    catalog precision_thin_film { .resistance, .max_power, .package }
}
```

```csv file=catalogs/precision_thin_film.csv set=thin
mpn,resistance,max_power,package,library,source
PTF-10K,10kohm +/- 0.1%,125mW,0402,Extended,https://example.invalid/ptf-10k.pdf
```

The standard rows and series come first, because the generic is first in the order. A design that they serve never reaches the provider, and one that they cannot serve, here a resistor good to 0.1 %, is tried with it:

```nsm evaluate picks catalogs=thin
import src::parts::precision_thin_film::PrecisionThinFilm   // not used by name: brings the provider in scope
import std::parts::passive::Resistor

board Reference {
    .r1: Resistor { .resistance &= 10kohm +/- 0.1% }
    .r2: Resistor { .resistance &= 10kohm +/- 1% }
}
```

```text
r1 is a Resistor: picked PrecisionThinFilm; passed over Resistor (the `resistor` catalog has nothing for `r1`, which needs resistance within 9.99kΩ..10.01kΩ)
```

`r2` is served by a standard row, so it says nothing. The provider needs the same three things as a part written by hand: its catalog file in `catalogs/`, its entry in `kicad.toml` (`kind = "PrecisionThinFilm"`), and the recorded source of its row. The other tools see what it provides, not only its name: it is drawn in series as an inductor is when it provides `Inductor`, `FerriteBead` or `Fuse`, and the placer takes it for a decoupling capacitor when it provides `Capacitor`, and a provider of `Capacitor` is a bank when its `.count` says so. The rules of a provider apply (above), and two more follow from the generic being a part with its own pads:

- A provider's pin table has a row of the generic's kind for each of the generic's pins: `.a` and `.b` for a resistor, `.anode` and `.cathode` for an LED or a diode, `.gate`, `.source` and `.drain` for a MOSFET, `.vin.pos`, `.vin.neg` and `.vout.pos` for a linear regulator.
- The generic's statements hold for the provider, so a provider cannot state other designator letters than the generic's (`.designator &= "RN"` is a contradiction of `"R"`).
- Only a part provides a standard generic: a module that says `provides Resistor` is refused, because the generic has a `pins` table.

The series of values that have no row (ADR 0047) and the columns that a file of rows may leave out (`.temperature_coefficient` of a resistor, `.dielectric` of a capacitor, `.rms_current` of an inductor) belong to the **catalogs** `resistor`, `capacitor`, `inductor` and `led` (an LED has no series, only its packages), which are the standard generics' own: one table of the solver names them, and it is the only place that does (ADR 0068, stage 3). A catalog is one name that every part listing it shares, so a part or a provider that lists `catalog resistor` has the series and the omitted columns of that catalog, whatever the generic it provides. A column may be left out of a file when the part's `catalog` statement marks it `optional` (ADR 0078), also in a catalog that no generic synthesizes: `.breakdown_voltage` of a TVS diode, in the `tvs` catalog (ADR 0073), is marked so in `std`. A provider whose catalog has another name has no series, and its file has every column it lists, unless its statement marks the column `optional`.

## Modules

A module is a reusable sub-circuit. Its interfaces, pins and declared values are its interface to the outside. The parts and modules inside it are private (ADR 0004).

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with the resistor that sets its current.
module Indicator {
    .supply: Rail
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Lamp {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Indicator
    .input ~ .lamp.supply
    .header: Header1x2
    .input.pos ~ .header.p1
    .input.neg ~ .header.p2
}
```

Rules.

- A board sets a module through the module's rails, pins and declared values: `.lamp.supply.voltage`. It cannot write `.lamp.led.current`.
- A module author exposes anything a user may need to set, as a rail or a declared value.
- A declared value with a sensible value is a default (`?=`). One without is left for the board to state: a board that forgets it gets a finding when a pick or a check waits for it (see "A value that nobody stated"), and a bound the module writes on it (`.balance_resistance >= 1kohm`) reaches the parts it is held to.
- Two instances of a module are separate. Their internal nets get names from their instance paths, so they never collide.
- A module may hold modules and parts. It names no nets.

Misuse.

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

module Stage {
    .supply: Rail
    .r1: Resistor
    .supply.pos ~> .r1 ~> .supply.neg
}

board Reach {
    .stage: Stage
    .stage.r1.resistance &= 1kohm +/- 1%
}
```

```text
error: `r1` is inside `Stage` and is not part of its interface; set it through a rail, a pin, or a value that `Stage` declares
   --> main.nsm:12:12
   |
12 |     .stage.r1.resistance &= 1kohm +/- 1%
   |            ^^
```

## Systems and ports

A system holds boards and says which of their ports meet. A port is an interface that a board declares directly, such as `.output: Rail`. Everything else of a board is private (ADR 0010).

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

/// Turns the 5 V it is given into 3.3 V for another board.
board Source {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
    .in_header: Header1x2
    .out_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    .output.pos ~ .out_header.p1
    .output.neg ~ .out_header.p2
}

board Lamp {
    .input: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .led: Led { .current &= 5mA..20mA }
    .r1: Resistor
    .in_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    .led.current == (.input.voltage - .led.forward_voltage) / .r1.resistance
    // The LED is a part, so the board states what it takes from its input.
    .input.current == -.led.current
    .input.pos ~> .r1 ~> .led ~> .input.neg
}

system Demo {
    .power: Source
    .lamp: Lamp
    .power.output ~ .lamp.input
}
```

Rules.

- A system holds boards and nothing else. The same board may be placed twice, each as its own member.
- `~` in a system joins two ports of the same kind on different boards. It equates their values and balances their currents. It does not put pins on a net. A connector that carries a port is mated instead, which joins the ports it carries (below).
- A system narrows a board through the board's ports and declared values: `.power.output.voltage &= 3.3V +/- 5%`.
- A system names no nets.
- `build` and `fab` write each board of a system to `<out>/<System>/<member>/`.

Misuse.

```nsm fails
import std::sources::Rail

board A {
    .output: Rail
}

board B {
    .input: Rail
}

system S {
    .a: A
    .b: B
    .a.output.pos.net &= "X"
    .a.output ~ .b.input.pos
}
```

```text
error: `a.output.pos.net` names a net, and a system does not: each board names its own nets, and a system only says which ports meet
   --> main.nsm:14:5
   |
14 |     .a.output.pos.net &= "X"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
error: a system joins two ports of the same kind; a pin is not a port
   --> main.nsm:15:17
   |
15 |     .a.output ~ .b.input.pos
   |                 ^^^^^^^^^^^^
```

### Connectors that mate

Boards that plug into each other do it through connectors, and a system says which. `~` between the `.mating` of two connectors says they are mated, contact N to contact N (ADR 0040). Every pin header and socket of the standard library states its `.mating` (`std::parts::connector::Mating`): its `kind`, `gender`, `pitch`, `rows` and `positions`, and, when its datasheet rates it, the current of one contact, `pin_current`. A system can reach a connector's `.mating` and nothing else of it.

The system names no pin and no net. After the nets of the boards are known, the checker reads what each board has behind each contact, a role: ground (the net is the `neg` of a rail), power (the `pos` of a rail), a signal (a part's `input`, `output` or `bidirectional` pin is on it) or open. Then:

- The two connectors must mate: the same kind, rows, positions and pitch, and a header with a socket. A header does not mate with a header.
- Contact N of one board meets contact N of the other, with the rules a board already has on a net: two outputs are refused, an `input` with nothing that drives it on either board is refused, ground against power or a signal is a short. A power contact against a power contact equates their voltages, as `~` of rails does, and an open contact is fine against anything but an input.
- A mate that carries power or a signal must carry a ground: a contact that is ground on both boards.
- The ports the mate carries are joined by it: a port of one board whose pins are all on contacts that a port of the same kind on the other board has its pins on is joined to it, as an explicit `~` of the two would join them (values equated, currents balanced). A `~` between ports that the mate carries is refused with a message naming the mate, so a join has one spelling. A port no connector carries (a soldered lead) is still joined with `~`. If a board has two ports on the same contacts, the mate cannot say which is joined, and says so.
- Each board keeps its own ground net. The system reports the ground groups: which boards share a ground through the mated ground contacts.
- The contacts carry the current. The rated current of one contact (`pin_current`) times the contacts that carry a rail's `pos`, or the ground the rails return through, is at least the current of the rail, in either direction. A connector that is rated is picked from the `header` catalog (`data/jlcpcb/catalog/header.csv`, one row for each series and pin count from a public datasheet), so the solver chooses a row that works. A connector with no rating is not checked, and the check is listed as not evaluated.
- A pinout that also works with one connector turned end for end, and that neither connector keys (`.mating.keyed`), is a guideline finding (`mating`): the plug-in can go in the wrong way round and the checker cannot tell.

A carrier with two sockets and two plug-in cards, with power, a UART and an SPI bus over the mates. The parts are invented, so the example is checked and solved and not exported:

```nsm evaluate file=examples/carrier_and_cards.nsm
//! A carrier board with two sockets and two plug-in cards, mated in a system.
//!
//! Every board is a PCB of its own with its own `GND` and `3V3` nets. The carrier has a controller
//! and two 1x10 sockets; the sensor card plugs into one and the serial card into the other. A
//! card has a 1x10 header wired to what it takes from the carrier: power on one card and an SPI
//! bus on the other. The system mates each socket with a header, and the checker does the rest:
//! it pairs contact N with contact N, finds what each board has behind each contact, joins the
//! rails the mates carry (so there is no `~` of the ports to write), checks that a signal has
//! one driver and that every mate carries a ground, and reports which grounds are one.
//!
//! The controller and the two chips are invented parts, written here to keep the example
//! whole; they have no KiCad symbol, so this file is checked and solved but not exported.

import std::bus::{Spi, SpiRole, Uart}
import std::parts::connector::{Header1x10, Socket1x10}
import std::parts::passive::Fuse
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A controller with a UART and an SPI master. Invented; not a real part.
part Controller {
    .vdd: Rail
    .uart: Uart
    .spi: Spi
    // Its table drives `sclk`, `mosi` and `cs` and reads `miso`: the master's end of the bus.
    .spi.role &= SpiRole::Master

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .uart.tx  output "TX"   "3"
        .uart.rx  input  "RX"   "4"
        .spi.sclk output "SCK"  "5"
        .spi.mosi output "MOSI" "6"
        .spi.miso input  "MISO" "7"
        .spi.cs   output "CS"   "8"
    }
}

/// An SPI sensor. Invented.
part SpiSensor {
    .vdd: Rail
    .spi: Spi
    // Its table reads `sclk`, `mosi` and the select and drives `miso`: a slave's end of the bus.
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "GND"  "2"
        .spi.sclk input  "SCLK" "3"
        .spi.mosi input  "SDI"  "4"
        .spi.miso output "SDO"  "5"
        .spi.cs   input  "CSN"  "6"
    }
}

/// A UART transceiver. Invented.
part SerialChip {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .uart.tx  output "TX"   "3"
        .uart.rx  input  "RX"   "4"
    }
}

/// The carrier: it takes 3.3 V from outside, gives it to the cards through its sockets, and
/// brings the controller's UART and SPI to them.
///
/// Both sockets have the same pinout, 1 GND, 2 3V3, 3 to 6 the SPI bus (the card is the
/// slave), 7 and 8 the UART (7 is the controller's `tx`), 9 3V3 and 10 GND.
board Carrier {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    /// Supplied from outside the system.
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .peak &= 3.6V
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    /// What the carrier gives to its cards, through a fuse: up to 500 mA.
    .cards: Rail {
        .pos.net &= "3V3_CARDS"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
        .current &= 0mA..500mA
    }
    .fuse: Fuse
    .supply.pos ~> .fuse ~> .cards.pos
    .supply.neg ~ .cards.neg
    // The fuse drops a little of the supply; the cards get what is left.
    .cards.voltage == .supply.voltage - .fuse.drop

    .mcu: Controller {
        .vdd.current &= 20mA..60mA
    }
    .supply ~ .mcu.vdd

    .slot_a: Socket1x10
    .slot_b: Socket1x10

    .cards.neg ~ .slot_a.p1
    .cards.pos ~ .slot_a.p2
    .mcu.spi.sclk ~ .slot_a.p3
    .mcu.spi.mosi ~ .slot_a.p4
    .mcu.spi.miso ~ .slot_a.p5
    .mcu.spi.cs ~ .slot_a.p6
    .cards.pos ~ .slot_a.p9
    .cards.neg ~ .slot_a.p10

    .cards.neg ~ .slot_b.p1
    .cards.pos ~ .slot_b.p2
    .mcu.uart.tx ~ .slot_b.p7
    .mcu.uart.rx ~ .slot_b.p8
    .cards.pos ~ .slot_b.p9
    .cards.neg ~ .slot_b.p10
}

/// A card with a sensor on SPI. Its header goes into the carrier's first socket.
board SensorCard {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .pos.net &= "3V3_CARD"
        .neg.net &= "GND"
        .current &= -30mA..-5mA
    }
    .sensor: SpiSensor
    .supply ~ .sensor.vdd

    .j1: Header1x10
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
    .sensor.spi.sclk ~ .j1.p3
    .sensor.spi.mosi ~ .j1.p4
    .sensor.spi.miso ~ .j1.p5
    .sensor.spi.cs ~ .j1.p6
    .supply.pos ~ .j1.p9
    .supply.neg ~ .j1.p10
}

/// A card with a UART chip. Its header goes into the carrier's second socket. The UART
/// crosses: the card's `tx` is on contact 8, where the controller has its `rx`.
board SerialCard {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .pos.net &= "3V3_CARD"
        .neg.net &= "GND"
        .current &= -60mA..-10mA
    }
    .chip: SerialChip
    .supply ~ .chip.vdd

    .j1: Header1x10
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
    .chip.uart.rx ~ .j1.p7
    .chip.uart.tx ~ .j1.p8
    .supply.pos ~ .j1.p9
    .supply.neg ~ .j1.p10
}

system CarrierAndCards {
    .carrier: Carrier
    .sensor: SensorCard
    .serial: SerialCard

    // The sensor card's header goes into socket A, the serial card's into socket B. A mate
    // joins contact N to contact N and derives the rails it carries, so no port is joined here.
    .carrier.slot_a.mating ~ .sensor.j1.mating
    .carrier.slot_b.mating ~ .serial.j1.mating
}
```

Mistakes in a mate. Some invented parts for the examples:

```nsm module=src::kit
import std::parts::connector::Mating

/// A part with one output pin. Invented.
part Out {
    pins {
        .y output "Y" "1"
    }
}

/// A part with one input pin. Invented.
part In {
    pins {
        .a input "A" "1"
    }
}

/// A 1x4 header at 2 mm pitch, which does not fit a 2.54 mm socket. Invented.
part Header1x4Fine {
    pins {
        .p1 passive "P1" "1"
        .p2 passive "P2" "2"
        .p3 passive "P3" "3"
        .p4 passive "P4" "4"
    }

    .mating: Mating {
        .kind &= "pin_header"
        .gender &= "header"
        .pitch &= 2mm
        .rows &= 1
        .positions &= 4
    }
}
```

A header does not go into a header:

```nsm fails
import std::parts::connector::Header1x4
import std::sources::Rail

board Carrier {
    .gnd: Rail { .neg.net &= "GND" }
    .slot: Header1x4
    .gnd.neg ~ .slot.p1
}

board Card {
    .gnd: Rail { .neg.net &= "GND" }
    .j1: Header1x4
    .gnd.neg ~ .j1.p1
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: `.carrier.slot` and `.card.j1` are both headers (Header1x4 and Header1x4); a header mates with a socket
   --> main.nsm:19:5
   |
19 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The count and the pitch must agree:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x4}
import src::kit::Header1x4Fine
import std::sources::Rail

board Carrier {
    .gnd: Rail { .neg.net &= "GND" }
    .slot_a: Socket1x4
    .slot_b: Socket1x4
    .gnd.neg ~ .slot_a.p1
    .gnd.neg ~ .slot_b.p1
}

board Short {
    .gnd: Rail { .neg.net &= "GND" }
    .j1: Header1x2
    .gnd.neg ~ .j1.p1
}

board Fine {
    .gnd: Rail { .neg.net &= "GND" }
    .j1: Header1x4Fine
    .gnd.neg ~ .j1.p1
}

system Kit {
    .carrier: Carrier
    .short: Short
    .fine: Fine
    .carrier.slot_a.mating ~ .short.j1.mating
    .carrier.slot_b.mating ~ .fine.j1.mating
}
```

```text
error: `.carrier.slot_a` (Socket1x4) has 4 positions and `.short.j1` (Header1x2) has 2; a mate joins contact N to contact N, so both need the same number
   --> main.nsm:29:5
   |
29 |     .carrier.slot_a.mating ~ .short.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: `.carrier.slot_b` (Socket1x4) has a pitch of 2.54mm and `.fine.j1` (Header1x4Fine) has 2mm; they do not fit together
   --> main.nsm:30:5
   |
30 |     .carrier.slot_b.mating ~ .fine.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A signal needs one driver. Two outputs on one contact, or an input that nothing drives, are refused:

```nsm fails
import src::kit::{In, Out}
import std::parts::connector::{Header1x4, Socket1x4}
import std::sources::Rail

board Carrier {
    .gnd: Rail { .neg.net &= "GND" }
    .tx: Out
    .rx: In
    .slot: Socket1x4
    .gnd.neg ~ .slot.p1
    .tx.y ~ .slot.p2
    .rx.a ~ .slot.p3
}

board Card {
    .gnd: Rail { .neg.net &= "GND" }
    .tx: Out
    .rx: In
    .j1: Header1x4
    .gnd.neg ~ .j1.p1
    .tx.y ~ .j1.p2
    .rx.a ~ .j1.p3
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: contact 2 of the mated connectors is an output on both boards, and two outputs cannot share a net: `.carrier.slot` pin 2 is on net `net-slot.p2`, a signal (output `carrier.tx.y`); `.card.j1` pin 2 is on net `net-j1.p2`, a signal (output `card.tx.y`); a mate joins contact N to contact N, so if one is meant to be turned end for end, its pin 2 would meet pin 3
   --> main.nsm:28:5
   |
28 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: contact 3 of the mated connectors has an input and nothing that drives it on either board: `.carrier.slot` pin 3 is on net `net-rx.a`, a signal (input `carrier.rx.a`); `.card.j1` pin 3 is on net `net-j1.p3`, a signal (input `card.rx.a`); a mate joins contact N to contact N, so if one is meant to be turned end for end, its pin 3 would meet pin 2
   --> main.nsm:28:5
   |
28 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A mate that carries a signal and no ground is refused, because the two boards would share no reference:

```nsm fails
import src::kit::{In, Out}
import std::parts::connector::{Header1x4, Socket1x4}

board Carrier {
    .tx: Out
    .slot: Socket1x4
    .tx.y ~ .slot.p2
}

board Card {
    .rx: In
    .j1: Header1x4
    .rx.a ~ .j1.p2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: the mate of `.carrier.slot` and `.card.j1` carries power or signals (contact 2 is a signal on `.carrier.slot` and a signal on `.card.j1`) but no ground: no contact is ground on both boards, so the two boards share no ground
   --> main.nsm:19:5
   |
19 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A port that a mate carries is not joined again with `~`:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x2}
import std::sources::Rail

board Carrier {
    .out: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .slot: Socket1x2
    .out.neg ~ .slot.p1
    .out.pos ~ .slot.p2
}

board Card {
    .supply: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= -50mA..-10mA
    }
    .j1: Header1x2
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
    .carrier.out ~ .card.supply
}
```

```text
error: `.carrier.out` and `.card.supply` are already joined by the mate of `.carrier.slot` and `.card.j1` at contacts 1, 2; the mate derives this join, so remove this one (a port join is for ports that no connector carries)
   --> main.nsm:31:5
   |
31 |     .carrier.out ~ .card.supply
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A system reaches a connector's `.mating` and nothing else of it:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x2}
import std::sources::Rail

board Carrier {
    .slot: Socket1x2
}

board Card {
    .supply: Rail
    .j1: Header1x2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.p1 ~ .card.supply.pos
}
```

```text
error: `p1` is inside `slot`, a `Socket1x2`; a system can reach only the `.mating` of a connector
   --> main.nsm:16:19
   |
16 |     .carrier.slot.p1 ~ .card.supply.pos
   |                   ^^
```

A pinout that reads the same from both ends is insertable the wrong way round, and the two rails here would swap. The finding is a guideline (ADR 0036): `warning:` on the command line, an error with `--strict`. A keyed connector (`.slot: Socket1x4 { .mating.keyed &= "polarised" }`) goes in one way and is not reported:

```nsm solve warns
import std::parts::connector::{Header1x4, Socket1x4}
import std::sources::Rail

board Carrier {
    .a: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "RAIL_A"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    .b: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "RAIL_B"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    .slot: Socket1x4
    .a.neg ~ .slot.p1
    .a.pos ~ .slot.p2
    .b.pos ~ .slot.p3
    .b.neg ~ .slot.p4
}

board Card {
    .a: Rail {
        .pos.net &= "RAIL_A"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    .b: Rail {
        .pos.net &= "RAIL_B"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    .j1: Header1x4
    .a.neg ~ .j1.p1
    .a.pos ~ .j1.p2
    .b.pos ~ .j1.p3
    .b.neg ~ .j1.p4
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
Kit/carrier: mating guideline: the pinout of the mate of `.carrier.slot` and `.card.j1` also works with one connector turned end for end, and neither is keyed: contact 2 (RAIL_A on `.carrier.slot`, RAIL_A on `.card.j1`) and contact 3 (RAIL_B on `.carrier.slot`, RAIL_B on `.card.j1`) would swap places and every contact would still be accepted; only a keyed connector (`.mating.keyed`) goes in one way
```

## Panels

A panel repeats one board in a grid on one fabrication panel, cut apart by V-cuts (ADR 0040). The board stays a stand-alone `board`: the panel only says how many copies there are and in what grid, and the board house's rules for such a panel come from a standard-library interface, as a board's process does. A panel names no nets and joins nothing.

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::{TwoLayer, VCutPanel}

board Tile {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        // Copper at least 0.4 mm from the edge, as a V-cut needs.
        .min_edge_clearance &= 0.4mm..1m
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

panel TilePanel {
    .cell: Tile
    .fab: VCutPanel {
        .columns &= 3
        .rows &= 2
        .rail &= 5mm
    }
}
```

The board's outline is in the board's physical file, `main.physical.nsm` here. A V-cut runs straight across the panel, so the outline is a rectangle with square corners. The panel reads the outline of its cell from the board with its physical file, so the file is found before the panel is checked.

```nsm-physical
import std::pcb::Outline

physical for Tile {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
            .corner_radius &= 0mm
        }
    }
}
```

Rules.

- `.cell: Board` names the board that is repeated. It is a `board` defined in the same file and placed in no system; a board a system places is made as the system's, not alone.
- `.fab: VCutPanel { ... }` is the board house's V-cut panel (`std::mfg::jlcpcb::VCutPanel`). `.columns` and `.rows` say the grid and have no default. `.rail` is the width of the handling rail round the grid, `0mm` (the default) for none; a rail carries the tooling holes and fiducials that assembly needs, and is 5 mm to 8 mm. The boards sit side by side with no space between them. Every other field is a limit of the board house with its source in `std/mfg/jlcpcb.nsm`.
- A panel holds that board and its rules and nothing else. It joins nothing, names no nets, and configures nothing inside its board.
- `netsmith fab` makes the board's own package in `fab/<Panel>/cell/<Board>/` and the panel's in `fab/<Panel>/`: one Gerber set for the whole panel (the boards' layers moved to their cells, one outline, the V-cut lines on `Edge.Cuts` marked "V-CUT", the drills merged, the tooling holes and fiducials of the rails), a bill of materials and a placement file for all the cells, `fab-notes.txt`, the panel as a `.kicad_pcb` (which KiCad can plot; `--cross-check` compares), and `panel.svg`. Designators in `bom.csv` and `cpl.csv` carry the cell (`R1_3` is `R1` of the third cell, counted along the rows from the top left).
- An order may name a panel, and its quantity then counts panels: `LedPanel = 3` in `[order.run.boards]` is three panels (see Orders).
- `fab` refuses a panel the board house would refuse, with the numbers: a panel outside 70 x 70 mm to 475 x 475 mm (250 x 250 mm if it is assembled), more than 25 lines along a side, lines closer than 2 mm, a board thinner than 0.6 mm, copper nearer the edge than 0.4 mm, an assembled panel without rails, a rail under 5 mm. Rounded corners are refused too.

Misuse: no board house rules.

```nsm fails
board Tile {
}

panel P {
    .cell: Tile
}
```

```text
error: panel P: it does not say who makes it, and a panel needs the board house's rules: add `import std::mfg::jlcpcb::VCutPanel` and `.fab: VCutPanel { .columns &= 3; .rows &= 2 }`
```

Misuse: the grid is not stated.

```nsm fails
import std::mfg::jlcpcb::VCutPanel

board Tile {
}

panel P {
    .cell: Tile
    .fab: VCutPanel {
        .rows &= 2
    }
}
```

```text
error: panel P: `fab` is a `VCutPanel` and has no value for `columns`: say how many, for example `.fab: VCutPanel { .columns &= 3 }`
```

Misuse: a panel holds boards, not parts or joins.

```nsm fails
import std::mfg::jlcpcb::VCutPanel
import std::parts::passive::Resistor
import std::sources::Rail

board Tile {
    .supply: Rail
}

panel P {
    .cell: Tile
    .fab: VCutPanel {
        .columns &= 2
        .rows &= 2
    }
    .r1: Resistor {}
    .cell.supply ~ .cell.supply
}
```

```text
error: a panel holds the board it repeats and its fabricator's panel rules, but `Resistor` is a part
   --> main.nsm:15:10
   |
15 |     .r1: Resistor {}
   |          ^^^^^^^^
error: a panel joins nothing: it repeats one board in a grid, and the boards stay apart
   --> main.nsm:16:5
   |
16 |     .cell.supply ~ .cell.supply
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## Load current

A `Rail` has a `current` field: the current drawn from the net through the rail's `pos` pin by whatever is behind the rail. It is positive when the thing draws and negative when it supplies (ADR 0007; see _The sign of a current_ at the end of this section). A design states the consuming side.

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

board Supply {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
    .in_header: Header1x2
    .out_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    .output.pos ~ .out_header.p1
    .output.neg ~ .out_header.p2
}
```

The checker records one balance for each net and scope: the currents of the rails there add up to zero. A scope is a board or a module. The solver uses the balances while it propagates. When one current on a net is unknown, it is the negative of the rest. Here it works out that the regulator supplies 0 to 50 mA, and that the board's input draws the same.

Rules.

- `~` between rails equates every field except `current`. A rail can feed several loads, so equal currents would be wrong. The one exception is a module's own rail joined to the rail of a part inside it: there the two are one current (below, ADR 0077).
- A module's own rail is a port. From inside, it supplies what is inside, so it counts with its sign flipped.
- `LinearRegulator` states that its input draws what its output supplies and its own quiescent current (`.quiescent_current`, none unless a part states it from its datasheet: `.quiescent_current &= 0A..140uA`, ADR 0076, amendment 9, provisional), that the output current is at most `.max_current`, and that `(vin - vout) * current + vin * quiescent` is at most `.max_power`.
- The first part of a `~>` chain that starts on a rail's net adds its through-current to that net's sum (ADR 0014). The solver sets the current of a rail from the rest of the net; a part's current is set from a net only when the net has two terms (a fuse in series with a load), and is otherwise verified, never solved by subtraction (ADR 0026). If its own relations do not determine it, the relations that use it are `not evaluated`. A balance is set or verified only when it is complete (ADR 0027): every part pin on the net is a term or belongs to a part that draws nothing at DC (one that states no `current`, has no rail and has only `passive` pins: a capacitor, a header, a test point). An `input` or `no_connect` pin of a part that has a `Rail` also draws nothing (its leakage is ignored; the rail is the part's supply). A pin that is none of these, such as an inductor joined with `~`, an IC's output or bidirectional pin, or an input of a part with no rail, leaves the balance incomplete, and the build lists those pins in a note.
- A net with two or more unknown currents stays unsolved without a message.
- A range stated on a current is a budget (ADR 0038), like the range of a field an equation states: what the rest of a complete net determines must lie inside it, and need not fill it. The stated current then holds the narrower value. Where two or more currents are stated, or one is worked out from the net, the currents only have to be able to add up to zero. A range on a current inside a part (the draw of a part's supply pin, `.u.vcc.current &= 0A..750nA`, whoever writes it, the part or the board) is the part's claim about what it draws and never a budget: a supply that is stated as allowed less than the claim is refused, however the claim is written (`0A..750nA` and `0A .. .max_draw` give the same verdict).

A module that draws through a divider, with the board stating the most it may draw:

```nsm solve
import std::parts::passive::Resistor
import std::sources::Rail

module Monitor {
    .vin: Rail
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }
    .r_top.current == .vin.voltage / (.r_top.resistance + .r_bottom.resistance)
    .vin.pos ~> .r_top ~> .r_bottom ~> .vin.neg
}

board Sensor {
    .supply: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .monitor: Monitor { .vin.current &= 0A..20uA }
    .supply ~ .monitor.vin
}
```

The divider draws 15.84 uA to 17.17 uA, which is inside the 20 uA, so the build passes and `monitor.vin.current` is that range. With `.vin.current &= 0A..10uA` the circuit draws more than the budget and the build fails:

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

module Monitor {
    .vin: Rail
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }
    .r_top.current == .vin.voltage / (.r_top.resistance + .r_bottom.resistance)
    .vin.pos ~> .r_top ~> .r_bottom ~> .vin.neg
}

board Sensor {
    .supply: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .monitor: Monitor { .vin.current &= 0A..10uA }
    .supply ~ .monitor.vin
}
```

```text
error: the currents on net `3V3` do not add up: `-monitor.vin.current` must stay within -10uA..0A, but the others on it work out to -17.17uA..-15.84uA (all of them: `monitor.r_top.current` is 15.84uA..17.17uA, `-monitor.vin.current` is -10uA..0A)
   --> main.nsm:18:25
   |
18 |     .monitor: Monitor { .vin.current &= 0A..10uA }
   |                         ^^^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a load larger than the regulator can give.

```nsm fails
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

board Overload {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50A
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
    .in_header: Header1x2
    .out_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    .output.pos ~ .out_header.p1
    .output.neg ~ .out_header.p2
}
```

```text
error: no `ldo` candidate satisfies `stage.regulator`: the closest of the 1 that fit, XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446, fails because -stage.regulator.vout.current must always be at most stage.regulator.max_current, but 0A..50A is not always below 100mA (the currents on net `3V3` in `stage`: stage.regulator.vout.current (-50A..0A) + -stage.vout.current (-50A..0A) add up to zero; net `3V3`: output.current (0A..50A) + stage.vout.current (-50A..0A) add up to zero)
   --> std/parts/regulator.nsm:40:5
   |
40 |     -.vout.current <= .max_current
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: two boards that disagree about the current at a join.

```nsm fails
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

board Source {
    .input: Rail
    .stage: Stage
    .output: Rail { .current &= 0mA..50mA }
    .input ~ .stage.vin
    .stage.vout ~ .output
}

board Sink {
    .input: Rail { .current &= -100mA }
}

system Demo {
    .power: Source
    .load: Sink
    .power.output ~ .load.input
}
```

```text
error: the currents at the join `load.input ~ power.output` cannot add up to zero: `load.input.current` (board `Sink`) is -100mA and `power.output.current` (board `Source`) is 0A..50mA, which add up to -100mA..-50mA
   --> main.nsm:21:20
   |
21 |     .input: Rail { .current &= -100mA }
   |                    ^^^^^^^^^^^^^^^^^^
```

### The sign of a current

Every `current` has one sign convention (ADR 0007), and the rest of this guide uses it. A current is the draw on a net: it is positive when the rail or part takes current from the net, and negative when it supplies the net. A load counts positive and a rail that supplies counts negative.

- A load states its own draw as a positive range: `.output: Rail { .current &= 0mA..50mA }` draws up to 50 mA.
- The output of whatever supplies a net is negative: a board's `.input` rail, a regulator's `.vout`. `.regulator.vout.current &= -1.2A..0A` says the regulator supplies up to 1.2 A. A regulator's `.vin.current` is positive and equals `-.vout.current`.
- A range with the wrong sign is a different claim: `0A..1.2A` on a `.vout` says that the output draws.
- A rail of a module is a port, and from inside the module it counts with its sign flipped (see _Load current_ above).

Which statement starts a circle that runs through the currents of a net depends on this sign: see _A circle through the currents of a net_.

### A wrapper module's rail

A module that wraps a part (an INA228 with its bypass capacitor, a flash with its decoupling) has a rail of its own and joins it to the rail of the part inside with `~`. The two are one current (ADR 0077): the module's rail draws what the part draws, so a supply current the part states (`.vs.current &= 0A..1.1mA`, the part's claim) is the current of the module's rail, and a sheet that has several such modules on one rail adds them up. The module writes nothing for it; it does not state `.vs.current == .sensor.vs.current`.

```nsm evaluate
import std::logic::Logic
import std::parts::passive::Capacitor
import std::sources::Rail

/// An invented current monitor: its supply and an alert line that names the supply as its
/// reference.
part Sensor {
    .designator &= "U"
    .vs: Rail
    .alert: Logic
    .alert.reference ~ .vs
    .vs.current &= 0A..1.1mA
}

/// An invented flash.
part Flash {
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 0A..10mA
}

module Monitor {
    .vdd: Rail
    .sensor: Sensor
    .bypass: Capacitor
    .vdd ~ .sensor.vs
    .vdd.pos ~> .bypass ~> .vdd.neg
}

module Storage {
    .vcc: Rail
    .flash: Flash
    .vcc ~ .flash.vcc
}

module Sheet {
    .vdd: Rail
    .monitor: Monitor
    .storage: Storage
    .vdd ~ .monitor.vdd
    .vdd ~ .storage.vcc
    .vdd.current == .monitor.vdd.current + .storage.vcc.current
}

board Wrapped {
    .supply: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .sheet: Sheet
    .supply ~ .sheet.vdd
}
```

The monitor's rail takes 0 to 1.1 mA and the flash's 0 to 10 mA, the sheet's 0 to 11.1 mA, and the supply's current is -11.1 mA to 0 A. Before ADR 0077 the sheet's equation was not evaluated and the build said, for this board:

```text
these values depend on each other and none has a value, so the checks that use them are not evaluated: `sheet.monitor.sensor.alert.reference.current`, `sheet.monitor.vdd.current`, `sheet.vdd.current`, `supply.current`; state a range on one of them with `&=` to start the solver, or state the range the part works in (`>=` and `<=` on the voltage) so that the solver can start it
```

The alert line's reference rail is on the monitor's net and no one says what it draws, so the monitor's balance had two unknown currents, and the rail of the module had no value. The reference rail of a logic line or of a bus's levels carries nothing (ADR 0066), so it is not a term of the wrapper's net.

The rule is narrow, and applies to a `~` written inside a module that joins one of the module's own rails to a rail that is a member of a part inside that module (a part, not another module). Of the rails on the module's net inside it, only the two may draw: a rail that is also joined to another part, a chain that starts on the net (a pull-up from `.vdd.pos`), another rail of the module that others draw from, or a second part's rail on the net, is a share of the module's current that the part's does not have, and the module's rail stays what the balance of the net says. A board's rails, two sibling parts, a module's port joined to the port of an inner module and every `~` outside a module keep their meaning: `~` does not equate currents.

A budget stated at the module's rail (`.monitor.vdd.current &= 0A..500uA` in the sheet) is judged as before against what the part says it may draw, and is refused when the part may draw more (ADR 0038): the two rails are one current and the part's range is not narrowed to fit the budget (ADR 0066).

## Capacitors: marked value, effective capacitance and bias

A `Capacitor` has two capacitances because they are two quantities (ADR 0013).

- `.capacitance` is the marked value with its tolerance, as on a bill of materials. Use it for a bypass capacitor where you mean a part.
- `.effective_capacitance` is what the circuit can use: the marked value after the DC bias across the part and after the tolerance, in the worst case over the range the bias can take. Use it where a datasheet asks for capacitance in use, such as a regulator's output capacitor.
- `.bias` is the DC voltage across the capacitor. A capacitor placed between a rail's `pos` and `neg` takes its bias from that rail. State `.bias` only for a capacitor that is across no rail.

```nsm solve report
import std::parts::passive::Capacitor
import std::sources::Rail

board Caps {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .bypass: Capacitor { .capacitance &= 100nF +/- 20% }
    .bulk: Capacitor { .effective_capacitance >= 1uF }
    .supply.pos ~> .bypass ~> .supply.neg
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
board Caps
  C2   Capacitor       4.7u            Capacitor_SMD:C_0402_1005Metric                            C23733 Basic
  C1   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  net 3V3: C1.1, C2.1
  net GND: C1.2, C2.2
```

The solver picks the cheapest Basic part whose effective capacitance stays at or above 1 uF. A marked 1 uF part does not qualify: the 25 V one keeps about 0.7 of its value at 3.465 V, before the tolerance.

Rules.

- The solver evaluates the effective capacitance from the part's published curve (the `effective_capacitance` cell of `data/jlcpcb/catalog/capacitor.csv`). A part with no curve is refused, but only when the design constrains `.effective_capacitance`.
- Temperature and ageing are not modelled. The curves are typical data at 25 C.
- State what you need after derating (`.effective_capacitance >= 4.7uF`), not the marked value.

Misuse: a capacitor in a chain that is across no rail, and a requirement no part meets.

```nsm fails
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board NoBias {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .filter: Capacitor { .effective_capacitance >= 1uF }
    .supply.pos ~> .r1 ~> .filter ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `filter`, which needs nothing in particular (`filter.bias` is not known, so `filter.effective_capacitance` cannot be worked out: place `filter` between a rail's pos and neg, whose voltage is known, or state `filter.bias`)
   --> main.nsm:11:5
   |
11 |     .filter: Capacitor { .effective_capacitance >= 1uF }
   |     ^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Capacitor
import std::sources::Rail

board TooBig {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .bypass: Capacitor { .effective_capacitance >= 10mF }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
error: no `capacitor` candidate satisfies `bypass`: the closest of the 34 that fit, 47uF 20% 6.3V X5R 0805 C16780, fails because bypass.effective_capacitance must always be at least 10mF, but 16.09uF..26.7uF is not always above 10mF (`bypass.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
   --> main.nsm:10:26
   |
10 |     .bypass: Capacitor { .effective_capacitance >= 10mF }
   |                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### Dielectric and package size

A ceramic capacitor's class (`C0G`, `X5R`, `X7R`) and its size (`"0402"`, `"0603"`) are two more things a design may ask for (ADR 0055, proposed).

- `.dielectric` has the enum type `Dielectric` of `std::parts::passive` (import it with the part). A design states it with `&=`, `.dielectric &= Dielectric::X7R`, or compares it with `==`. Every row of the standard capacitor catalog has one, read from the class that JLCPCB's record prints for the row; a generic capacitor (the project states an E-series) has none, so a design that states a class is given a row, never a generic part. A design that does not mention it picks as before.
- `.package` is the size as text, `.package &= "0603"`. It narrows to the rows of that package, and a generic capacitor of that size. It names one size: a design that would take 0402 or 0603 leaves `.package` open and lets the catalog's order decide (Basic parts first, then the file's order), or states `.package` on the capacitors where the size matters.
- A column is optional in a project's catalog file: a file that has no `dielectric` in its first line is read as empty there, and a design that states a class is offered no row of it.

```nsm solve report
import std::parts::passive::{Capacitor, Dielectric}
import std::sources::Rail

board Classes {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .bypass: Capacitor {
        .capacitance &= 100nF +/- 20%
        .dielectric &= Dielectric::X7R
        .package &= "0402"
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
board Classes
  C1   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  net 3V3: C1.1
  net GND: C1.2
```

A class that no row of the catalog has for the value is reported as any requirement no part meets:

```nsm fails
import std::parts::passive::{Capacitor, Dielectric}
import std::sources::Rail

board NoSuchClass {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .bypass: Capacitor {
        .capacitance &= 100nF +/- 20%
        .dielectric &= Dielectric::C0G
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `bypass`, which needs capacitance within 80nF..120nF and dielectric within Dielectric::C0G
   --> main.nsm:10:5
   |
10 |     .bypass: Capacitor {
   |     ^^^^^^^^^^^^^^^^^^
```

An `Inductor` has an optional `.rms_current`, the heating (RMS) current, as a catalog column. The standard rows leave it empty (see `data/jlcpcb/README.md`), and nothing in the library checks it, so `BuckStage` does not either: a design that needs a heating rating states `.stage.inductor.rms_current >= 2A` itself and gets a row from a project's catalog file that records one.

### A bank of capacitors in parallel

A requirement that no single capacitor meets is met by several in parallel (ADR 0052). `.count` is how many identical capacitors stand in this place. Only the standard `Capacitor` has this meaning: a part of your own with a member named `count` (the segments of an LED bar, say) is one part, and its `.count` is a plain number. Left unset it is one, and nothing changes. A whole number (`.count &= 3`) is that many. A range (`.count &= 1..8`) lets the solver choose: it takes the fewest of the range for which every check that mentions the part holds, together with the catalog row, trying one capacitor of every row before two of any, so a part that one capacitor can serve is picked as before. Every other field of the part is the value of one capacitor, so a requirement on the whole bank multiplies by the count (`.count * .effective_capacitance`), and a rating (`.max_voltage`) holds for each.

```nsm solve report
import std::parts::passive::Capacitor
import std::sources::Rail

board Bank {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "5V"
        .neg.net &= "GND"
    }
    .bulk: Capacitor { .count &= 1..8 }
    .bulk.count * .bulk.effective_capacitance >= 60uF
    .bulk.max_voltage >= 6V
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
board Bank
  C1   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C2   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C3   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C4   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C5   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C6   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  net 5V: C1.1, C2.1, C3.1, C4.1, C5.1, C6.1
  net GND: C1.2, C2.2, C3.2, C4.2, C5.2, C6.2
```

The design has one part for each capacitor of the bank, numbered one after the other, all on the same nets, and the bill of materials lists the row once with the quantity. The first capacitor has the path of the instance (`bulk`, the key of a layout file); the others add `#2`, `#3` (`bulk#2`). The schematic, the KiCad files and the placer see real parts. The placer treats each as any other capacitor, so a bank is not kept in a row. A requirement in the physical file on the part's path (`.bulk.place.top &= 3mm..6mm`, `.bulk.place.side &= "bottom"`, a `.height`) holds for every capacitor of the bank: the design has one requirement for each, the check measures each, and a finding names the one it is about (`bulk#2.place.top`, with its designator). A relation between the place of the bank and that of another part (`.bulk.place.x - .r1.place.x == 5mm +/- 3mm`) holds for each pair, and the build lists a note that says so (`` `bulk` is a bank of 4 capacitors ``). A requirement on the module the bank is in reaches them all, as for any part.

When no count in the range is enough, the message names the count that came nearest:

```nsm fails
import std::parts::passive::Capacitor
import std::sources::Rail

board Short {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "5V"
        .neg.net &= "GND"
    }
    .bulk: Capacitor { .count &= 1..2 }
    .bulk.count * .bulk.effective_capacitance >= 10mF
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
error: no `capacitor` candidate satisfies `bulk`: the closest of the 34 that fit, 2 x 47uF 20% 6.3V X5R 0805 C16780, fails because (bulk.count * bulk.effective_capacitance) must always be at least 10mF, but 21.86uF..32.81uF is not always above 10mF (`bulk.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
   --> main.nsm:11:5
   |
11 |     .bulk.count * .bulk.effective_capacitance >= 10mF
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A bank is one choice: the capacitors of a bank are identical, and the ripple current each takes is not modelled (the catalog has no ripple rating), so it is not divided between them.

## The process of a board

A board is made by one manufacturing process. Its design rules and stackup come from a definition in the standard library, and the board has it as a member named `process` (ADR 0008, ADR 0012).

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Bare {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_track &= 0.15mm..1m
    }
    .header: Header1x2
}
```

Rules.

- Each rule is the least the factory can make, written as a range. `.min_track &= 0.1mm..1m` says the least track is at least 0.1 mm. A board narrows it upward for margin. The rule used is the bottom of what is left.
- Ranges are closed, so `1m` stands for no upper limit.
- A board must choose `.stackup.thickness`. The process offers 0.4 mm to 2 mm, and netsmith will not pick.
- A board with no `process` builds and views as a schematic. `fab` stops and says to add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`. There is no default.
- `.max_temperature_rise` is the rise in kelvin that the track width check designs to (see Track width against current). The process leaves it unstated and it is 10 K when the board does too: `.process.max_temperature_rise &= 20K`.
- `std::mfg::jlcpcb::FourLayer` has two inner planes. The board names the net of each: `.process.inner1.net &= "GND"`. See `examples/four_layer.nsm`. A plane on a net the board does not have is an error. `FourLayer` offers 1.6 mm only.

`netsmith fab` on a board without one prints (this block is not run by the guide test):

```text
error: LedExample: `LedExample` does not say how it is made, and a PCB needs design rules: add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer` to the board
```

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Thick {
    .process: TwoLayer {
        .stackup.thickness &= 3mm
    }
}
```

```text
error: contradiction on `process.stackup.thickness`: it is already 400um..2mm, which does not overlap 3mm
  --> main.nsm:5:9
  |
5 |         .stackup.thickness &= 3mm
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::FourLayer

board Planes {
    .process: FourLayer {
        .stackup.thickness &= 1.6mm
        .inner1.net &= "VCC"
        .inner2.net &= "GND"
    }
    .supply: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

```text
error: Planes: `process.inner1` is a plane on the net `VCC`, but the board has no net of that name (its nets: `5V`, `GND`)
```

## The shape of a board

Beyond its parts and nets, a board can say how wide its power tracks are, how big it is and what is cut out of it, where copper is poured, and where tracks, vias, copper or parts are not allowed (ADR 0020, ADR 0029). Each is a member of the board, or a field of one, in the standard library: `std::pcb` has `Outline`, `Cutout` and `Keepout`, `std::mfg::stackup` has `Pour`, and a `Rail` has `.min_track`. The track widths and the pours are what the circuit's checks consume, and they stay in the board. The outline, the cutouts, the keepouts a board draws, the mounting holes, the fiducials, the labels and every requirement on where a part goes are **physical intent**, and a board states them in its physical file (ADR 0048), described first. A board that says none of it is laid out as before.

Lengths are in the board's own frame: the origin is the outline's top left corner, x grows to the right and y downward. A board states how big a shape is. Where a shape is, when the board does not say, is in the layout file (see below).

### The physical file

Physical intent is what the product, the case and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector must be, a fixed spacing between two parts, the face of a module, a keepout. It is requirements, never solutions. The circuit source stays strictly the circuit, and a case revision is a diff of its own file (ADR 0048).

It is in `<stem>.physical.nsm` beside the source, found by stem as the layout file is: `board.nsm` has `board.physical.nsm`. The file is optional and never needed for a board that states no physical intent. It holds one unit `physical for Board { ... }` for each board it speaks for (the file may hold the units of several boards), and the statements in a unit are the language of a board's body: declarations, `&=`, `==`, `>=` and `<=`. The units are put with their boards before anything else is checked, so a `panel` or a `system` that places the board sees them too.

```nsm solve place
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::{Cutout, Keepout, Outline}

physical for Cased {
    // The case: numbers from the case drawing.
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
            .corner_radius &= 1mm
        }
        .window: Cutout {
            .x &= 30mm
            .y &= 2mm
            .width &= 6mm
            .height &= 3mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, centred.
    .power.place.left &= 1.5mm +/- 0.1mm
    .power.place.y >= .outline.height / 2 - 0.1mm
    .power.place.y <= .outline.height / 2 + 0.1mm
    // A part inside a module is named by its path: the sensor's header stays right of 20 mm.
    .sensor.j.place.left >= 20mm
    // The sensor block is on the bottom face, every part of it.
    .sensor.place.side &= "bottom"

    .rib: Keepout {
        .x &= 12mm
        .y &= 0mm
        .width &= 6mm
        .height &= 24mm
        .layers &= "top"
        .max_height &= 4mm
    }
}
```

- **`case { ... }`** is the section for what the case fixes: the outline, the cutouts, the mounting holes (`MountingHole`), the fiducials (`Fiducial`) and the labels (`Label`) of the board. It holds the same statements as the rest of the unit, and lifts out unchanged if a case is ever shared. These five belong in it; a keepout may be anywhere in the unit.
- **A path may cross a module boundary here and nowhere else.** `.sensor.j.place.left` names the header inside the `Sensor` module. The circuit source still cannot reach into a module (ADR 0004); the physical file can, to state physical intent about the part. A statement may set or compare only a part's `.place`, a part's `.height`, and what the unit declares itself (the fields of its `.outline`, `.window`, `.rib`). Anything that changes what a part is, or a connection, is refused.
- **A relation between two parts** is the equation of _Where a part goes_: `.b.place.x - .a.place.x == 17.5mm +/- 0.05mm`. The placer holds it and `fab` measures it (_A relation between two parts_).
- **`.place.side` on a module** is a requirement on every part of it, at any depth. A module with an exception states the faces of its parts one by one and none for itself; two lines that disagree are an error that names both.
- The case numbers are the maintainer's file to review: put the source of each (a drawing, a sheet) in a comment above the line, and never invent one.

`examples/enclosure_sensor/` is a complete invented board that states all of this in its physical file, and each statement has a comment that says what it asks for: a rounded outline with a cutout, mounting holes and fiducials, a 40 mm plug that hangs over an edge, an antenna and a coax connector on one line, two socket rows a fixed distance apart, a module on the back with one part on the front, a row of twelve test pads on a pitch, a height limit, and a flat cell on the back across the sockets whose pins protrude 0.2 mm (_Pin protrusion_). It is a project, because it defines four invented parts and maps them to KiCad's stock footprints and a footprint of its own in its `kicad.toml`; `crates/netsmith/tests/enclosure_sensor.rs` runs `fab` on it and checks that every requirement is met.

The placer keeps to every requirement, `fab` measures them on the board and stops on one not met, and the editor draws the file's outline and keepouts. A requirement that is a single value fixes the part, like a lock in the layout file.

Misuse: the old place.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
import std::pcb::Outline

board Old {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .outline: Outline {
        .width &= 40mm
        .height &= 24mm
    }
    .j: Header1x2
    .j.place.left &= 1.5mm
}
```

```text
error: `.outline: Outline` is an outline, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in the `case { }` section of `physical for Old` in `main.physical.nsm`
  --> main.nsm:9:15
  |
9 |     .outline: Outline {
  |               ^^^^^^^
error: `j.place.left` is a placement requirement, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for Old` in `main.physical.nsm`
   --> main.nsm:14:5
   |
14 |     .j.place.left &= 1.5mm
   |     ^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a physical file that changes what a part is.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Block {
    .j: Header1x2
}

board Wrong {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .b: Block
}
```

```nsm-physical
physical for Wrong {
    .b.j.assembly &= AssemblyMode::Hand
    .b.j.p1 ~ .b.j.p2
}
```

```text
error: `b.j.assembly` is not physical intent: a physical file states where a part is (its `.place`), how tall it is (`.height`), how far its pins protrude (`.pin_protrusion`) and the outline, cutouts, keepouts, holes, fiducials and labels it declares, never what a part is; put this in the circuit source
  --> main.physical.nsm:2:5
  |
2 |     .b.j.assembly &= AssemblyMode::Hand
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: a physical file states where things are, and a connection with `~` changes the circuit; put it in the circuit source
  --> main.physical.nsm:3:5
  |
3 |     .b.j.p1 ~ .b.j.p2
  |     ^^^^^^^^^^^^^^^^^
```

Misuse: a unit for a board that is not there.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Right {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
physical for Left {
}
```

```text
error: `physical for Left` names no board of the source it is paired with; its boards are `Right`
  --> main.physical.nsm:1:14
  |
1 | physical for Left {
  |              ^^^^
```

### Track widths by rail

A rail's `.min_track` is the narrowest track its nets are drawn with. It is a lower bound, written as a range like a process rule, and the bottom of what is left is the width. Both pins of the rail get it. A net takes the widest `.min_track` of the rails on it, so `GND` below is 0.8 mm without anyone saying so. `~` between rails equates the width too, so a module's rail carries it.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

board Widths {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .pos.net &= "VIN"
        .neg.net &= "GND"
        .min_track &= 0.8mm..1m
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
    .in: Header1x2
    .out: Header1x2
    .input.pos ~ .in.p1
    .input.neg ~ .in.p2
    .output.pos ~ .out.p1
    .output.neg ~ .out.p2
}
```

The router draws `VIN` and `GND` at 0.8 mm and `3V3` at 0.5 mm, and the `.kicad_pro` has a net class for each width. A width under the process's least track is the process's least. Signals have no rail: widen them all with `.process.min_track`. The width is checked against the rail's current, below.

### Track width against current

A track that carries current heats. The solver knows the worst-case current each rail draws (`.current`, see Load current) and, through the series parts of a chain, the current of the nets between them, so netsmith draws every net at a width that can carry it and checks any width you state yourself (ADR 0036). It uses the IPC-2221 formula, `I = k * dT^0.44 * A^0.725`, where `I` is the current in amperes, `dT` the temperature rise in kelvin, `A` the track's cross-section in square mils (its width times the copper's thickness), and `k` is 0.048 for a track on an outer layer and 0.024 on an inner layer. netsmith draws tracks on the outer layers only, so it uses 0.048 and the outer copper of the process (`.stackup.copper_weight` ounces, 0.035 mm each).

A net carries the current that Kirchhoff's current law says it must, the way an EE adds it up (ADR 0036, PROVISIONAL): the sum of the currents of the loads that draw from it (a supply net) or return through it (a ground, or any shared return), with every load on at the same time as the worst case. Two loads of 2 A and 1 A on separate supplies that share a `GND` make `GND` a 3 A net, where each supply net carries its own. The things that count are the rails with a pin on the net and the series parts (`~>` chains) whose bridge ends on it, so the nets between the parts of a chain take the chain's current. A regulator is a load on its input net and a supply on its output net, each with the current the solver worked out: a linear regulator draws what its output supplies, and a switching regulator draws `Pout / efficiency / Vin`, from the efficiency its part model states, so its input net is sized for that and not for the output current. A part model that states no efficiency leaves the input current unknown, and the net is sized only for what is known (below). The sum is pessimistic where a current is counted twice: a ground that the load of a regulator's output and the regulator's own input both return through carries the sum of the two, though the load's return goes to the regulator and only the input's goes on to the source. You do not have to state a width for it. A net no rail gives a `.min_track` is drawn at the width IPC-2221 needs for that current (rounded up to 0.01 mm): a 2 A node between a fuse and a diode is drawn at 0.79 mm with nothing written, and the check has nothing to say. A `.min_track` on a rail is an override: the net is drawn at the widest one, never narrower than the process's least track, and the check warns when IPC-2221 needs more. The temperature rise is `.process.max_temperature_rise`, in kelvin, an upper bound like `via_in_pad`; it is 10 K when the board states none, and the finding says so. 10 K is a conservative default of netsmith's and is not a number from a fabricator. The planes of a four-layer board and the nets of a pour are areas and not tracks, and are not sized or checked.

When a current on the net is not known (the solver leaves the currents of a net with two or more unknown ones unsolved), netsmith does not guess it, and does not ignore what is known either. The currents that are known are a lower bound on the net's current, because loads only add up: a current that is not known can make the sum larger and never smaller. So the net is drawn at least as wide as the known currents need (the process's least track or a stated `.min_track` still decides when it is wider), and `build` prints a note that says the width is a lower bound and names the net and the carriers whose current is missing. A net with no known current at all stays at the process's least track, and the note says that. A stated `.min_track` that is narrower than the known current needs is still a finding, worded "carries at least" and naming the carriers whose current is missing. The note appears while a current is missing and goes away when you state it. A net whose every current is known is sized as above and has no note.

```nsm solve notes
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Unsized {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .inlet: Header1x2
    .outlet: Header1x2
    .supply.pos ~> .r ~> .load.pos
    .supply.neg ~ .load.neg
    .inlet.p1 ~ .supply.pos
    .inlet.p2 ~ .supply.neg
    .outlet.p1 ~ .load.pos
    .outlet.p2 ~ .load.neg
}
```

```text
the current of rail `supply`, rail `load` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `GND` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `supply`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VIN` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `load`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VOUT` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
```

Here a fan on the output is known to draw up to 1.2 A, but the supply's and the load's currents are not. `GND` carries at least the fan's 1.2 A, so it is drawn at the 0.39 mm that 1.2 A needs and not at the process's least; `VIN` has nothing known and stays at the least:

```nsm solve notes
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Partial {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }
    .fan: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
        .current &= 0A..1.2A
    }
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .inlet: Header1x2
    .outlet: Header1x2
    .supply.pos ~> .r ~> .load.pos
    .supply.neg ~ .load.neg
    .inlet.p1 ~ .supply.pos
    .inlet.p2 ~ .supply.neg
    .outlet.p1 ~ .load.pos
    .outlet.p2 ~ .load.neg
}
```

```text
the current of rail `supply`, rail `load` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `GND` are 0.39 mm, a lower bound: sized for the 1.2 A that is known, not for the whole current; state that current, or give a rail on the net a `.min_track`
the current of rail `supply`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VIN` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `load`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VOUT` are 0.39 mm, a lower bound: sized for the 1.2 A that is known, not for the whole current; state that current, or give a rail on the net a `.min_track`
```

Where the DC analysis solved the net (see "DC analysis": every pin on it is one the analysis models, which no net with a connector, an unmodelled pin or an open island is), the current is the one the analysis found into it (ADR 0076, amendment 8, provisional): at each corner of the ranges, the sum of what the elements put into the net, and the largest of those over the corners. It is complete, so a net the analysis solved has no note, and it is not larger than the sum above, which takes every load at its largest at the same time, and smaller where the loads follow one voltage: a regulator that draws most when the supply is low next to a resistor that draws most when it is high is never both at once, and the net is sized for the corner that carries the most. It is used for the nets between two elements that no rail holds (the nets of a chain, a regulator's input behind a fuse), which one island contains; a supply, a ground or a regulator's output is on many islands and keeps the sum above. A net with an open-drain pin that was only inferred keeps it too. A finding that a stated width is too narrow says `the most the DC analysis finds into it over every corner of its ranges` and the sum it replaces.

A current that is not known can still be bounded from above, by Ohm's law for a resistor (see "Resistor power"). The width does not change with the bound. The note goes only when the width the net is drawn at carries the known current and every bound on the net together, in whichever direction they flow, which is so for the pull-down of a logic line (a few hundred microamperes, which the process's least track carries many times over) and not for a part whose bound is amperes. A net with a current that has no bound, a rail's or a transistor's, keeps its note.

The block below states a width that is too narrow for its rail, so it is a finding:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 0A..2A
        .min_track &= 0.2mm..1m
    }
    .source: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= -2A..0A
    }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
```

```text
Heater: IPC-2221 guideline: net `5V` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 745 mA and IPC-2221 needs 0.79 mm: give a rail on the net `.min_track &= 0.79mm..1m`, or allow a higher `.process.max_temperature_rise`
Heater: IPC-2221 guideline: net `GND` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 745 mA and IPC-2221 needs 0.79 mm: give a rail on the net `.min_track &= 0.79mm..1m`, or allow a higher `.process.max_temperature_rise`
```

A ground is sized for the sum of what returns through it. Here two supplies share `GND`: the 5 V one carries 2 A, the 3.3 V one 1 A, and the `GND` track that the rail `load_a` states at 1 mm carries 2.39 A at 10 K, enough for the 2 A of `5V` but not for the 3 A that return together:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Shared {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .source_a: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= -2A..0A
    }
    .source_b: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= -1A..0A
    }
    .load_a: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 0A..2A
        .min_track &= 1mm..1m
    }
    .load_b: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A..1A
    }
    .in_a: Header1x2
    .in_b: Header1x2
    .out_a: Header1x2
    .out_b: Header1x2
    .in_a.p1 ~ .source_a.pos
    .in_a.p2 ~ .source_a.neg
    .in_b.p1 ~ .source_b.pos
    .in_b.p2 ~ .source_b.neg
    .out_a.p1 ~ .load_a.pos
    .out_a.p2 ~ .load_a.neg
    .out_b.p1 ~ .load_b.pos
    .out_b.p2 ~ .load_b.neg
}
```

```text
Shared: IPC-2221 guideline: net `GND` carries up to 3 A (the sum of the currents on it with every load on at once: rail `load_a` 2 A + rail `load_b` 1 A) but its tracks are 1 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 2.39 A and IPC-2221 needs 1.37 mm: give a rail on the net `.min_track &= 1.37mm..1m`, or allow a higher `.process.max_temperature_rise`
```

Rules.

- This is a guideline and not a limit of the fabricator (ADR 0030): `build` and `fab` print each finding as `warning: ...`, `fab` also writes it in `fab-notes.txt`, and the editor lists it as a warning at the rail or part that carries the current. The board still builds.
- To make a finding stop the build, put `guidelines = "error"` in the `[project]` table of `netsmith.toml`, or pass `--strict` to `build` or `fab` for one run. The message is then an error at the same line.
- IPC-2221 is a conservative design guide, and IPC-2152 is more accurate (it counts the board's thickness and the copper next to the track). netsmith implements IPC-2221 only. A track the check passes is not therefore good for a certain temperature, and one it flags may be fine; read the number as a prompt to decide.
- The router may draw a track narrower than its net's width where there is no room (the stub at a pad is the net's width, and the pad's own copper carries the rest). The check is of the width asked for.

### The outline and cutouts

The member `.outline` of a board's physical file (in its `case` section) is the board edge: a rectangle with rounded corners. A range is a limit, and the placer takes the smallest board that holds the parts inside it: `.width &= 0mm..60mm` is at most 60 mm. A single value is exactly that size. Without an `.outline` the board is as big as its parts need. A `Cutout` is a rectangular hole inside the outline, with an optional corner radius. Its corner nearest the origin is `.x`, `.y`, or the layout file's entry for its name; stating both is an error.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Shaped {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::{Cutout, Outline}

physical for Shaped {
    case {
        .outline: Outline {
            .width &= 60mm
            .height &= 0mm..40mm
            .corner_radius &= 3mm
        }
        .slot: Cutout {
            .x &= 5mm
            .y &= 8mm
            .width &= 3mm
            .height &= 12mm
            .corner_radius &= 1mm
        }
    }
}
```

When the parts do not fit, `fab` stops and says which ones (the margin round the parts is the process's copper to edge space plus one track and its space, 0.4 mm for JLCPCB's, the same as for a board sized to fit; this block is not run by the guide test):

```text
error: Shaped: 3 part(s) do not fit the outline of 60.0 x 40.0 mm: outside the outline or inside its margin, by mm past what is kept clear: J1 1.30, R4 0.20; courtyards overlapping, by mm2: R2/R3 1.20; placing them in the outline anyway ended with 1 pair(s) of courtyards overlapping, 2 part(s) outside the outline or inside its 0.4 mm margin
```

(The numbers are invented for the example.) The placer packs and turns the parts in the outline (a part wider than the board is turned, unless its requirements state a rotation) and refuses only if that ends with courtyards over each other or parts outside, so the message leads with the parts that are wrong: how many, every part that is outside the outline or inside its margin with how far, and every pair of courtyards that overlap with its area. It does not give the size the parts need, except when the courtyards alone add up to more than both faces of the outline hold, which no arrangement can fix (then it says so, with the areas). The message then lists the ten pairs of courtyards that overlap the most (the names, the shared area in mm2 and its size, where it is from the outline's top left corner and the side of the outline it is nearest) and the ten parts furthest out (each with how far outside which edge, or inside the margin, or into which cutout), and names all the others. `fab` also writes every part's final position, turn, face and state to `placement-failure.txt` in the board's output folder and names the file in the message. There is no `--json` output for it yet.

When the enclosure is a DXF, `netsmith dxf import` writes the outline, the cutouts and the mounting holes for you (see Commands): the members for the `case` section, the holes as `MountingHole` parts, and their places, which come from the case and not from the placer, as requirements in the same file (`.hole1.place.x &= 5mm`). A hole whose place is stated is not put in a corner.

Mounting holes, plated holes and fiducials go in the corners of a fixed outline, holes first, clockwise from the top left, and clear of a rounded corner. A board with no outline lays them out with the other parts. A part in a rounded corner, over a cutout or outside the outline is found by the placement check and named.

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Tiny {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::pcb::Outline

physical for Tiny {
    case {
        .outline: Outline {
            .width &= 3mm
            .height &= 40mm
            .corner_radius &= 2mm
        }
    }
}
```

```text
error: Tiny: `outline.corner_radius` is 2 mm, more than half of the shape's shorter side (3 mm)
```

### Pours

The process has a pour on each outer layer, `.top` and `.bottom`. A pour with a net is copper filled round the tracks, joined to its net with KiCad's thermal reliefs; a pour with no net is no pour. `.stitch_pitch` is the most space between the vias that join the pour to the other layers of its net (an inner plane, or the pour on the other side): the tool uses the top of the range and puts the vias on a grid after routing, in what the routing leaves free.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::FourLayer

board Poured {
    .process: FourLayer {
        .inner1.net &= "GND"
        .inner2.net &= "3V3"
        .top.net &= "GND"
        .bottom.net &= "GND"
        .top.stitch_pitch &= 0mm..8mm
        .bottom.stitch_pitch &= 0mm..8mm
    }
    .in: Header1x2
    .out: Header1x2
    .in.p1 ~ .out.p1
    .in.p1.net &= "3V3"
    .in.p2 ~ .out.p2
    .in.p2.net &= "GND"
}
```

A pour on a net the board does not have is an error that lists the nets, as for a plane. The pads of the pour's net keep the via the router puts beside them; the pour is KiCad's to fill (`fab` has it fill the zones before it checks and plots the board).

### Keepouts

A `Keepout` is an area where something is not allowed. Four fields say what, each `"forbid"` or `"allow"`, and one that states none forbids all four (the language has no boolean, so the words stand for one): `.tracks`, `.vias`, `.copper` (the pours are not filled there) and `.parts` (no part's courtyard, and no pad). `.layers` is `"all"` (the default) or `"top"`, `"bottom"`, `"inner1"`, `"inner2"` separated by commas. The router, the stitching vias and the placement check keep to it, and the board file has it as a KiCad rule area, so KiCad's check and fill do too.

A keepout is a member of a board's physical file, with its place in the board's frame, or a member of a part, in the part's own frame (the footprint's, turned with the part), so that a part carries its own no-copper zone. A part states it relative to itself and nothing else; the keepouts a board draws are physical intent and are in its physical file (a keepout in a board's circuit source is an error that says so).

```nsm evaluate
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
import std::pcb::Keepout

part Radio {
    .package: Package
    .antenna: Keepout {
        .x &= -1mm
        .y &= -9mm
        .width &= 12mm
        .height &= 8mm
        .layers &= "top"
        .vias &= "allow"
    }
}

board Wireless {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .radio: Radio
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::Keepout

physical for Wireless {
    .no_copper: Keepout {
        .x &= 40mm
        .y &= 0mm
        .width &= 20mm
        .height &= 6mm
        .tracks &= "allow"
    }
}
```

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Open {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::pcb::Keepout

physical for Open {
    .k: Keepout {
        .width &= 5mm
        .height &= 5mm
        .tracks &= "allow"
        .vias &= "allow"
        .copper &= "allow"
        .parts &= "allow"
    }
}
```

```text
error: Open: `k` allows tracks, vias, copper and parts, and forbids nothing
```

### Height limits

**Provisional: to be reviewed by the maintainer (ADR 0045, "Height limits").** A board that fits in plan view can still hit a lid, a bracket or a neighbouring board. A part states how tall its body stands with the built-in member `.height` (one value, from the face of the board it is on to the top of its body; the board's physical file may narrow it for the board), and a `Keepout` in the physical file states the tallest a part may stand where it is with `.max_height`. A keepout that states `.max_height` and none of `.tracks`, `.vias`, `.copper` and `.parts` forbids nothing else: it only limits height. `.layers` says which face it limits (`"top"` for the parts on the top face, `"bottom"` for the parts on the bottom, `"all"` for both), so a part on the back has its own limit: two keepouts over the same area, one for each face.

A part that states no `.height` has the `height` cell of the catalog row it was picked as, if the row has one (provisional, ADR 0094): a catalog file's `height` is one length that a datasheet or a drawing gives, the greatest it allows (the maximum, or the nominal plus the tolerance), with the document, its revision and the page in the row's `source`. A height that the design states wins, whatever the row says, and the row's height is never an input of the pick: no row is chosen or refused for it. For a board that has a height limit, `fab-notes.txt` says how many parts have a known height and names the row each catalog height was read from. A part with neither has no height: it is not taken as flat and not guessed. Where a limit covers its body, `fab` and the editor print a warning that names it (`height of R3 (`r3`) is unchecked ...`), and `fab` writes it in `fab-notes.txt`. With `constraints = "error"` in `netsmith.toml`, or `fab --constraints error`, it is an error that stops `fab` instead (`--constraints warn` wins over the manifest, and `--allow-drc-violations` does not waive it). A part that does state one is kept out from under a zone it is too tall for by the placer, and the placement check names it, with the numbers, if it stands there anyway. A zone a part carries (a `Keepout` member of a part, in the part's own frame) is kept clear of the same way, wherever the part ends up and however it is turned: the placer keeps the other parts out of it, and the check measures it against every part but the one that carries it. The footprint's 3D model is not read.

```nsm solve place
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Ribbed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .tall: Resistor { .resistance &= 15kohm +/- 1% }
    .flat: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .tall ~> .rail.neg
    .rail.pos ~> .flat ~> .rail.neg
    // The datasheet heights.
    .tall.height &= 9mm
    .flat.height &= 0.5mm
}
```

```nsm-physical
import std::pcb::{Outline, Keepout}

physical for Ribbed {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
        }
    }
    // The case's lid has ribs over the top 18 mm: nothing taller than 3 mm there.
    .ribs: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= "top"
        .max_height &= 3mm
    }
}
```

The placer put `.tall` in the part of the board the ribs leave free. Placed by hand under the ribs (a layout file that locks it there), the check says:

```text
error: Ribbed: R2 (`tall`) stands 9.00 mm tall on the top face, and the keepout `ribs` allows 3.00 mm there (6.00 mm too tall), where its body is, around (20.0, 6.0) mm from the board's top left corner
```

Misuse: a height is one value, above or at zero, and a limit is above zero.

```nsm fails
import std::parts::passive::Resistor
import std::mfg::jlcpcb::TwoLayer

board Vague {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .r: Resistor { .resistance &= 15kohm +/- 1% }
    .r.height &= 2mm..5mm
}
```

```text
error: Vague: `r.height` is 2mm..5mm, a range, and a position, a size or a height is one value: say it with `&= 2mm`
```

A catalog row's `height` is one length too, and a cell that is not one is found when a board uses the catalog, with its file and line:

```csv file=catalogs/resistor.csv set=range
mpn,resistance,max_power,package,height,source
ACME-R316,316ohm +/- 1%,62.5mW,0402,0.4mm..0.6mm,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=range
import std::parts::passive::Resistor

board Range {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:2: `height`: `0.4mm..0.6mm` is a range, and a height is one value: write the datasheet's maximum, such as `0.6mm`
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

### Pin protrusion

**Provisional readings (ADR 0045, section 15); the name `.pin_protrusion` and the rule were decided by the owner on 2026-11-04.** On a tight board a flat part sits on one face over the pins of a through-hole part that come through from the other face. The pins are cut after soldering, so what protrudes is lower than the flat part's underside. A part has the built-in member `.pin_protrusion` (a length): how far the pins of a through-hole part protrude out of the far side of the board, whether they were cut or bought short. The board's physical file states it for one part, as it states `.height`, and a path may reach a part inside a module (`.radio.row_a.pin_protrusion &= 0.2mm`). A part that states none has an unknown protrusion, which is not zero: its pads stay an obstacle for a part on that face, as before.

A part on that face may stand over the pins when all of these hold, and `fab` says which does not:

- it is fitted by hand, after the pins are cut (`.assembly` is `"hand"` or `"none"`; a part the board house assembles is on the board before any pin is soldered);
- it states its `.height`;
- the protrusion plus its height fits the height limit that a keepout puts on that face where its body is (the pins protrude `t`, the part rests on them, so `t + height` must not be more than `.max_height`). With no limit there, nothing is compared, and `fab` warns of that.

Its own pads still keep the process's clearance from the pads of the through-hole part. Here the socket's pins protrude 0.2 mm, the regulator stands 1.8 mm tall and the lid leaves 2.2 mm, so it places over them:

```nsm solve place
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Stacked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .reg: LinearRegulator
    .reg.vin ~ .input
    .reg.vout ~ .output
    .socket: Header1x2
    .input.pos ~ .socket.p1
    .input.neg ~ .socket.p2
    // The regulator is fitted by hand, after the socket's pins are cut.
    .reg.assembly &= AssemblyMode::Hand
    // Datasheet: it stands 1.8 mm tall.
    .reg.height &= 1.8mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Stacked {
    case {
        .outline: Outline {
            .width &= 24mm
            .height &= 16mm
        }
    }
    // Case drawing (invented): the lid leaves 2.2 mm under the back of the board.
    .back_room: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 24mm
        .height &= 16mm
        .layers &= "bottom"
        .max_height &= 2.2mm
    }
    // The regulator lies on the back across the socket's two pads, a hair apart in x.
    .socket.place.side &= "top"
    .socket.place.rotation &= 0
    .socket.place.x &= 12mm +/- 0.02mm
    .socket.place.y &= 8mm +/- 0.02mm
    .reg.place.side &= "bottom"
    .reg.place.rotation &= 0
    .reg.place.x &= 14.75mm +/- 0.02mm
    .reg.place.y &= 8mm +/- 0.5mm
    // The socket's pins are cut after soldering to protrude 0.2mm.
    .socket.pin_protrusion &= 0.2mm
}
```

`fab` then writes a step for a person next to `hand-solder:` in `fab-notes.txt` and `order.txt`: `trim: J1 (socket): after soldering, cut the pins to protrude 0.20 mm from the bottom face, before U1 (reg) is fitted (stacked.physical.nsm:26); measure one protruding pin`. The files cannot carry it, so measure one protruding pin before the covering part goes on. Never state a protrusion to make a finding go away: cutting the pins is a hand step, and it is stated only when it is done.

With pins that protrude 0.6 mm the stack is 2.4 mm. The lines pin both parts over each other, so `fab` refuses before it places anything, naming the lines and the protrusion that would fit (a placement that is checked afterwards says `U1 (`reg`) lies over the pins of J1 (`socket`), whose pins protrude 0.60 mm ...`):

```nsm solve place refused
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Stacked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .reg: LinearRegulator
    .reg.vin ~ .input
    .reg.vout ~ .output
    .socket: Header1x2
    .input.pos ~ .socket.p1
    .input.neg ~ .socket.p2
    // The regulator is fitted by hand, after the socket's pins are cut.
    .reg.assembly &= AssemblyMode::Hand
    // Datasheet: it stands 1.8 mm tall.
    .reg.height &= 1.8mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Stacked {
    case {
        .outline: Outline {
            .width &= 24mm
            .height &= 16mm
        }
    }
    // Case drawing (invented): the lid leaves 2.2 mm under the back of the board.
    .back_room: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 24mm
        .height &= 16mm
        .layers &= "bottom"
        .max_height &= 2.2mm
    }
    // The regulator lies on the back across the socket's two pads, a hair apart in x.
    .socket.place.side &= "top"
    .socket.place.rotation &= 0
    .socket.place.x &= 12mm +/- 0.02mm
    .socket.place.y &= 8mm +/- 0.02mm
    .reg.place.side &= "bottom"
    .reg.place.rotation &= 0
    .reg.place.x &= 14.75mm +/- 0.02mm
    .reg.place.y &= 8mm +/- 0.5mm
    // The socket's pins are cut after soldering to protrude 0.6mm.
    .socket.pin_protrusion &= 0.6mm
}
```

```text
error: Stacked: no placement meets every requirement of the physical file:
  these lines cannot both hold: main.physical.nsm:22 `.socket.place.x &= 12mm +/- 0.02mm`; main.physical.nsm:23 `.socket.place.y &= 8mm +/- 0.02mm`; main.physical.nsm:21 `.socket.place.rotation &= 0`; main.physical.nsm:26 `.reg.place.x &= 14.75mm +/- 0.02mm`; main.physical.nsm:27 `.reg.place.y &= 8mm +/- 0.5mm`; main.physical.nsm:25 `.reg.place.rotation &= 0`; main.physical.nsm:24 `.reg.place.side &= "bottom"`; main.physical.nsm:29 `.socket.pin_protrusion &= 0.6mm`: they keep the courtyard of U1 (`reg`) within x 12.83 to 16.67, y 5.8 to 10.2 mm, over a pad of J1 (`socket`) (pins protruding 0.6 mm) by at least 0.02 x 0.775 mm wherever it is put, and the protrusion does not excuse that: U1 stands 1.80 mm tall on pins that protrude 0.60 mm: 2.40 mm from the bottom face, and the keepout `back_room` allows 2.20 mm there (0.20 mm too much); cut the pins to protrude 0.40 mm or less, or use a lower part
```

The same refusal is made, with its own sentence at the end, when the covering part states no height (`state .reg.height`) or when the board house assembles it (`make U1 hand`). A part over the pads of a through-hole part that states no protrusion is the plain overlap, with a sentence that names the member to state. A part kind may bound the protrusion, and a board that states more breaks that claim, which is refused with the line of the bound:

```nsm fails
import std::mfg::jlcpcb::TwoLayer

part ShortPins {
    .designator &= "J"
    pins {
        .p1 passive "P1" "1"
    }
    .pin_protrusion <= 3mm
    .package: Package
}

board TooLong {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: ShortPins
    .j.pin_protrusion &= 5mm
}
```

```text
error: j.pin_protrusion must always be at most 3mm, but 5mm is not always below 3mm
  --> main.nsm:8:5
  |
8 |     .pin_protrusion <= 3mm
  |     ^^^^^^^^^^^^^^^^^^^^^^
```

### Labels

A `Label` is silkscreen text: a part with no pins that is fitted by nobody, placed like any other part, and declared in the `case` section of the board's physical file. `.text` is what it prints and `.text_height` how tall (it was `.height` before the part height of _Height limits_ took that name); unset, the process's least text height, and never less. The reference designator of a label is `LBL1`, and it is not in the schematic.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer

board Marked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::parts::mechanical::Label

physical for Marked {
    case {
        .revision: Label {
            .text &= "REV A"
            .text_height &= 1.2mm
        }
    }
}
```

### Where a part goes

A case, a panel or a carrier fixes where some parts must be: a connector's mouth at a slot, a button under its cap. Every part and every module has a built-in member `.place` (ADR 0045) that holds the requirements. It is narrowed with the ordinary operators, like `.assembly`: `&=` for a value or a range (`2mm +/- 0.1mm`, `1mm..2mm`), `>=` and `<=` for a bound or an expression, such as half the outline's height. A tolerance needs a constant on both sides, so a window round an expression is two bounds. Declaring `.place` yourself is an error. Order never matters.

The lines are in the board's physical file (ADR 0048), in the unit `physical for Board`: a board's circuit source that narrows a `.place` is an error that says so. A part inside a module is named by its path, `.sensor.j.place.y`. All the examples below are two blocks, the circuit source and its physical file.

| Field | Meaning |
|---|---|
| `.x`, `.y` | the centre of the part's body |
| `.left`, `.right`, `.top`, `.bottom` | the edges of the body's box, after the part is turned |
| `.rotation` | degrees counterclockwise on screen, as in the layout file; a plain number, a multiple of 90 |
| `.side` | `"top"` or `"bottom"`; on a module it is a requirement on every part of it |

Positions are lengths in the board's frame (the outline's top left corner is the origin, x right, y down), and they are measured from the **body**: the outline the footprint draws on its fabrication layer (`F.Fab`), else its courtyard. The case states the shell of a connector, not the origin the footprint's author chose, so a requirement survives a change of package. The layout file's `at` stays the footprint's origin; the check converts, and every finding says which drawing it used. A field the board leaves alone is unconstrained.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::Outline

physical for Cased {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, turned, and
    // centred along it.
    .power.place.left     &= 1.5mm +/- 0.1mm
    .power.place.y        >= .outline.height / 2 - 0.1mm
    .power.place.y        <= .outline.height / 2 + 0.1mm
    .power.place.rotation &= 90
    .power.place.side     &= "top"

    // The whole sensor block stays right of 20 mm.
    .sensor.place.left >= 20mm
    // The sensor's header, a part inside the module, is named by its path.
    .sensor.j.place.y &= 8mm +/- 0.2mm
}
```

The requirements are hard. A requirement that leaves one place and one turn (every field a single value) fixes the part like `lock = true` in the layout file. Any other puts the part in a window the placer keeps to, and it gives the module's parts the half of the module's requirements that keeps them in (a least `.left` or `.top`, a most `.right` or `.bottom`). A part the layout file locks is checked against them before anything is placed; a part it merely moves is measured afterwards. The layout file keeps the solution; the physical file keeps only the requirement, never a solved position.

`netsmith build` refuses what can be seen without a placement: bounds that leave nothing, a rotation that is not a multiple of 90, a side that is neither `"top"` nor `"bottom"`, and a requirement measured from an outline that has no fixed size.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Squeezed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x2
}
```

```nsm-physical
physical for Squeezed {
    .j.place.y >= 12mm
    .j.place.y <= 3mm
}
```

```text
error: the requirements on `j.place.y` cannot all hold: it must be at least 12mm (`j.place.y >= 12mm`) and at most 3mm (`j.place.y <= 3mm`)
  --> main.physical.nsm:3:5
  |
3 |     .j.place.y <= 3mm
  |     ^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Stretchy {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Stretchy {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 20mm..30mm
        }
    }
    .j.place.y >= .outline.height / 2
}
```

```text
error: Stretchy: `j.place.y` is measured from the outline's top left corner, and `outline.height` does not state one size, so the board has no fixed outline; give `outline.height` a single value (`&= 30mm`), or state the position without it
```

#### A relation between two parts

Some requirements are between two parts and fix neither: two rows of sockets a datasheet puts 17.5 mm apart, an antenna header that is on the line of its cable. They are written as the equation of the fields, in the physical file, and they may reach into modules by path, at any depth. A relation is the measure of one part's body minus the measure of another's, along one axis, in a window. `==` with a range is the window (`17.5mm +/- 0.05mm`, or `.a.place.y == .b.place.y` for a window of zero), and `>=` and `<=` give a one-sided window.

```nsm solve place
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Radio {
    .row_a: Header1x2
    .row_b: Header1x2
}

board Paired {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .radio: Radio
    .antenna: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Paired {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 30mm
        }
    }

    // Datasheet figure 4: the two rows are 17.5 mm +/- 0.05 mm apart, centre to centre.
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    // Cable drawing: the antenna header is on the line of row A.
    .antenna.place.y == .radio.row_a.place.y
}
```

The solver does not evaluate a relation between two parts, because placement is not a solver output; the placer and the check do (ADR 0045, section 12; provisional). The placer treats the relation as a hard requirement like the others: it moves the two parts together until the measure is in the window, and a window narrower than its 0.05 mm grid is met exactly by putting one of the parts off the grid by a few hundredths of a millimetre. The same inputs give the same board. A relation is one equation of two fields of two different parts, each with the coefficient 1 and -1 (a sum, a product, a turn, a module's own `.place` and two fields of one part stay in the file unevaluated, and the build lists them as `not evaluated`), along one axis (`x`, `left` and `right`, or `y`, `top` and `bottom`), with everything else a constant, such as `.outline.width / 2`. A window that can never be held is an error of `build`; two parts that the requirements or the layout file's locks have already fixed, or one fixed and one whose own window cannot reach the relation, are refused before placing with the numbers. A relation that the placed board does not meet is a finding, as any other requirement is, and stops `fab`:

```text
error [constraint] constraint radio.row_b.place.x - radio.row_a.place.x (paired.physical.nsm:11) is violated: the centre of the body of J2 (`radio.row_b`, F.Fab) minus the centre of the body of J1 (`radio.row_a`, F.Fab) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)
```

#### A row of parts

A row of test pads, or of the pins of a bank of switches, is a numbered run of parts on one line, each a fixed distance from the next. The `row` statement says it once, in the physical file only (owner decision, 2026-11-04, ADR 0048): `row .pads.tp1 .. .pads.tp4 along x pitch 2.54mm +/- 0.05mm`. The first and the last part name the run: the same path up to the last segment, then a name that ends in a number, and every number between them is a part (`tp1`, `tp2`, `tp3`, `tp4`). `along x` or `along y` is the axis of the pitch. The statement puts the parts on one line (the other axis equal) and each one at the pitch from the one before it, within the window; a pitch without a window is held exactly, like a relation without one. A negative pitch runs the row the other way. The pitch is a length.

```nsm solve place
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp3: Header1x2
    .tp4: Header1x2
}

board Probed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .pads: Pads
}
```

```nsm-physical
import std::pcb::Outline

physical for Probed {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 30mm
        }
    }

    // Test fixture drawing: the four pads are 5 mm apart on one line near the bottom edge.
    .pads.tp1.place.y >= .outline.height - 8mm
    row .pads.tp1 .. .pads.tp4 along x pitch 5mm +/- 0.05mm
}
```

The statement is the same as the equations it stands for: for each pair of neighbours `.pads.tp2.place.x - .pads.tp1.place.x == 5mm +/- 0.05mm` and `.pads.tp2.place.y == .pads.tp1.place.y`, so the placer holds it and `fab` measures it with no new mechanism. The distance of the whole row from the edge, or its face, is stated on a part or a module as before (`.pads.place.side &= "top"` puts every pad on the top face). A finding names the row's file and line, and the two neighbours it is about:

```text
error [constraint] constraint pads.tp3.place.x - pads.tp2.place.x (probed.physical.nsm:13) is violated: the centre of the body of J3 (`pads.tp3`, F.Fab) minus the centre of the body of J2 (`pads.tp2`, F.Fab) is 7 mm along x, and it allows 4.95 to 5.05 mm (1.95 mm over)
```

The parts of a row are numbered in natural order, so the names and the labels agree: `.pads.tp2` is `J2` and `tp10` follows `tp9` (see _Designators_). A run is refused, with the statement quoted, when it is not one: an end that does not end in a number, ends with different names or in different modules, a first number above the last, a run of one part, a part missing between the ends (a gap), an end that is not a part (a module, a value), or a pitch that is not a length. A number written with leading zeros keeps the width of the first (`tp01 .. tp12`). A `row` in the circuit source is an error that says it belongs in the physical file.

Misuse: a gap in the run.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp4: Header1x2
}

board Gappy {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .pads: Pads
}
```

```nsm-physical
physical for Gappy {
    row .pads.tp1 .. .pads.tp4 along x pitch 5mm
}
```

```text
error: `row .pads.tp1 .. .pads.tp4` has a gap: `pads` has no member `tp3`, and a row is every number from the first to the last
  --> main.physical.nsm:2:5
  |
2 |     row .pads.tp1 .. .pads.tp4 along x pitch 5mm
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### A part that hangs over the board edge

A plug that sticks out of the case has part of its courtyard outside the outline. That is an error ("not wholly inside the board outline") unless the design says the part may hang over that edge. **The owner decided the rule on 2026-11-03 (ADR 0045, section 10); how a module states an overhang for its parts is still open.** The design says it with a requirement it can already write: a window for an edge of the **body** that lies at or beyond the same edge of the outline. `.j.place.left &= -0.5mm +/- 0.1mm` puts the body's left edge 0.5 mm past the board's left edge, and `.plug.place.bottom >= .outline.height` puts its bottom edge on the board's bottom edge or past it (flush counts). Only the part's own `left`, `right`, `top` and `bottom` say it, and only when the whole window is at or beyond the edge. A window on `.x` or `.y`, a bound a module states for its parts, or an entry of the layout file puts the part somewhere but does not say it may hang over, so the courtyard past the edge is the error it always was. That holds for a part the layout file locks (`lock = true`) too: the lock fixes where the part is, and the part must still state every edge its courtyard is past, like any other (owner decision, 2026-11-03). A locked part past an edge nobody stated gets the error below, naming the edge, the distance and the line that would allow it.

```nsm solve place
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot C: the header's body sticks 0.5 mm out of the left edge.
    .j.place.left &= -0.5mm +/- 0.1mm
}
```

For a part that may hang over an edge, the placer and the check do this, and nothing else:

- **The room it takes** is its courtyard clipped to the outline, with the margin (0.4 mm for JLCPCB's process) kept on the sides that are on the board and none on the side that hangs over. A connector wider than the board no longer stops the placer before it starts, and a locked part that states its overhang counts as the part that is inside.
- **Overlap** with the courtyards of other parts, keepouts and cutouts is measured on the clipped courtyard, so two parts hanging over the same edge do not conflict beyond it, in the placer's cost as in the check.
- **Copper is not waived.** Every pad is still inside the outline and the process's copper to edge space from it, so the placer keeps them there and a pad outside is an error with its distance.
- **The body's centre may be off the board** in the direction an edge field states (provisional, to be reviewed by the maintainer). A part whose `.bottom` window starts 25 mm past a 60 mm board may be required with its 40 mm body, and so its centre, 5 mm below it, as long as its pads stay inside. A window on `.x` or `.y` keeps the old rule: a centre that is outside the outline is refused with the numbers, and so is a centre outside by an edge the part's edge fields do not state.
- **A direction nobody stated is an error**, and the finding says how far past which edge, which edges are stated, and the line that would allow it.
- **Not checked:** the body (`F.Fab`) may be anywhere, the part of the courtyard outside the outline is not measured, and neither is how far the part hangs beyond what its window says. Silkscreen and text outside the board still get the design rules check's silk to edge clearance warning, as in KiCad.

`fab` prints each overhang as a warning as it places and lists it among the warnings in `fab-notes.txt`: `warning: J1 hangs over the left edge: its courtyard is 0.50 mm past it (`j.place.left (main.physical.nsm:10)` allows -0.6 to -0.4 mm)`. The variants below run the real checks. The first states the position by the body's centre, which is not an overhang.

```nsm solve place refused
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.x &= 1.3mm
}
```

```text
error: Plugged: J1 is not wholly inside the board outline: its courtyard is at least 0.50 mm past the left edge wherever its requirements put it, and the design does not say it may hang over an edge; to allow it past the left edge, require the body to reach it, for example `.j.place.left <= 0mm`; the lines that place it: main.physical.nsm:10 `.j.place.x &= 1.3mm`
```

The next states an overhang that is too much: the header's pads end up 0.05 mm from the board edge, nearer than the process allows.

```nsm solve place refused
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.left &= -1mm +/- 0.1mm
}
```

```text
error: Plugged: a pad of J1 is 0.05 mm from the board edge
```

The last states the left edge, and its centre along it puts it past the top edge too:

```nsm solve place refused
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.left &= -0.5mm +/- 0.1mm
    .j.place.y &= 5.1mm
}
```

```text
error: Plugged: J1 is not wholly inside the board outline: its courtyard is 0.52 mm past the top edge, and the design says it may hang over only the left edge (`j.place.left (main.physical.nsm:10)` allows -0.6 to -0.4 mm); to allow it past the top edge, require the body to reach it, for example `.j.place.top <= 0mm`
```

`netsmith fab` measures every requirement on the board it made and prints a finding for each one not met, with the measured value, the window allowed and how far over it is. A violated requirement is an error that stops `fab`, like a design rules error; it is not a guideline, so `guidelines` and `--strict` do not govern it. `constraints = "warn"` under `[project]` in `netsmith.toml`, or `--constraints warn` for one run, lists them as warnings instead (`"error"` is the default). `fab --constraints error|warn` wins over the manifest, so `--constraints error` stops `fab` even when the manifest says `warn`. `--allow-drc-violations` waives design rules, not requirements: with it, an unmet requirement still stops `fab` ("does not waive them"). The notes of the package count the requirements met, and say how many were not and that they are only warnings. Output of the check for a header moved by hand (this block is not run by the guide test):

```text
error [constraint] constraint power.place.left (cased.physical.nsm:13) is violated: the left edge of the body of J1 (F.Fab) is 28.73 mm from the left edge of the outline, and it allows 1.4 to 1.6 mm (27.13 mm over)
```

The editor lists the same findings with the placement's, a relation between two parts among them (_A relation between two parts_); it does not yet draw the windows or a line between the two parts (ADR 0045, C4). Height limits are in _Height limits_ above.

### The layout file

Where parts, keepouts and cutouts are, when the physical file does not say, is in `<stem>.pcb.toml` beside the source file: `board.nsm` has `board.pcb.toml`. The file is optional and never needed to build. An editor or a placer writes it, and `fab` and the editor read it. It holds the solution: the physical file holds the requirements on positions (see _Where a part goes_), never a solved position, and the layout file never a requirement.

```toml
version = 1

# One table for each board of the file, keyed by the board's name.
[OutlineAndPours]
seed = 3              # another arrangement of the parts the file does not place; absent is 0

[OutlineAndPours.parts."stage.output_cap"]
at = [31.5, 22.0]     # mm from the outline's top left corner, y down
rotation = 90         # degrees, counterclockwise on screen
side = "top"          # "top" or "bottom"
lock = true           # the placer leaves it where it is
source = "hand"       # "hand", "auto" or "regen": who wrote it

[OutlineAndPours.keepouts.antenna]   # a Keepout or Cutout that states no `.x`, `.y` in the physical file
at = [18.0, 2.0]
```

Parts are keyed by their path in the design, which a renumbered reference designator does not change. A part with no entry is placed by the placer (ADR 0033): it derives a placement from the circuit alone, keeping the parts of a module together, a decoupling capacitor next to the power pins it bridges, connectors at the edges and wire short, and places them round the parts the file places. The same circuit, file and `seed` always give the same board; another `seed` gives another arrangement. A build puts every part that has an entry where the file says; the placer places the others round the parts that are locked (`lock = true`) and does not react to the others, so moving a part by hand moves nothing else. `place_subset` places some parts and leaves the rest, but no command asks for it yet. An entry for a part, keepout or cutout that the board does not have, or a table for a board the file does not have, is an error that names it and lists what exists. A keepout or cutout that states `.x` and `.y` in the physical file and has an entry as well is an error: a thing has one place. `side = "bottom"` puts a part on the back. `examples/outline_and_pours.nsm`, its physical file `examples/outline_and_pours.physical.nsm` and `examples/outline_and_pours.pcb.toml` use all of this.

## Imports

`import` brings definitions into a file. Paths start at `std` for the standard library or `src` for the files of your project.

```nsm-fragment
import std::parts::passive::Resistor
import std::parts::passive::{Resistor, Led}
import src::power::Bypassed
```

A project file `src/power.nsm` is the module `src::power`:

```nsm module=src::power
//! Power stages.

import std::parts::passive::Capacitor
import std::sources::Rail

/// A rail with a bypass capacitor across it.
module Bypassed {
    .rail: Rail
    .cap: Capacitor { .capacitance &= 100nF +/- 20% }
    .rail.pos ~> .cap ~> .rail.neg
}
```

```nsm solve
import src::power::Bypassed
import std::sources::Rail

board Filtered {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .filter: Bypassed
    .supply ~ .filter.rail
}
```

Rules.

- An import names a module path and a definition. The last segment is the definition. There is no import of a whole module.
- A name is imported once. Importing a name that the file defines or already imports is an error.
- A type is written as a bare name. `A::B` in a type position is not supported.

Misuse.

```nsm fails
import std::parts::passive::Resistors

board Empty {
}
```

```text
error: module `std::parts::passive` has no definition `Resistors`
  --> main.nsm:1:29
  |
1 | import std::parts::passive::Resistors
  |                             ^^^^^^^^^
```

```nsm fails
import src::powr::Bypassed

board Empty {
}
```

```text
error: unknown module `src::powr`
  --> main.nsm:1:1
  |
1 | import src::powr::Bypassed
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## Comments

| Form | Meaning |
|---|---|
| `// text` | A comment to the end of the line. |
| `/// text` | A doc comment on the declaration or definition that follows. |
| `//! text` | A doc comment on the file (at its top) or on the definition it is inside (at the start of its body). |

```nsm
//! A file with every comment form.

import std::parts::passive::Resistor

/// A resistor on its own.
board Documented {
    //! The board has one resistor.

    // Not a doc comment.
    /// The only part.
    .r1: Resistor
}
```

Rules.

- A `///` line attaches to a declaration or a definition. On a statement it is an error.
- Inside a part, `///` goes only on a declaration (`.resistance: Resistance`) or on a definition. A `pins` block, a row of a pin table, `catalog`, `tie`, `bridge`, a relation and a connection are statements: put a `//` comment on them. The message is the one below (`doc comments can only be attached to declarations`, and for a pin table row `a pin table takes `//` notes, not doc comments`).
- `////` is a plain comment.
- There are no block comments: only `//`, `///` and `//!` exist. `/*` is an error.
- Doc comments are kept in the syntax tree and printed back. The editor's schematic draws the ones on a board, a module, a part or an interface, and every `==` relation, as notes beside what they are about (ADR 0060); a `//` comment is never drawn.

Misuse.

```nsm fails
import std::parts::passive::Resistor

board Stray {
    .r1: Resistor
    /// about nothing
    .r1.resistance &= 1kohm +/- 1%
}
```

```text
error: doc comments can only be attached to declarations
  --> main.nsm:5:5
  |
5 |     /// about nothing
  |     ^^^^^^^^^^^^^^^^^
```

A block comment is the other misuse.

```nsm fails
import std::parts::passive::Resistor

board Block {
    .r1: Resistor /* the only part */
}
```

```text
error: block comments do not exist; use `//`
  --> main.nsm:4:19
  |
4 |     .r1: Resistor /* the only part */
  |                   ^^
```

## Projects and netsmith.toml

A project is a directory with a `netsmith.toml` at its root (ADR 0011). `netsmith init` makes one in the current directory. It writes `netsmith.toml`, `src/main.nsm` (a working LED board), `.gitignore` and `README.md`, and for the agents that work in the project an `AGENTS.md`, the skills in `.agents/skills/` (with `.claude/skills` linking to them) and this guide as `docs/netsmith/LANGUAGE.md` (ADR 0015). It never overwrites a file; an `AGENTS.md` that exists is left alone and `init` says so.

```toml
[project]
name = "blink"
top_level = ["src/main.nsm"]
out = "out"
fab = "fab"
kicad = 9
catalog_dirs = ["catalogs"]
```

Only `name` is required.

| Key | Meaning |
|---|---|
| `name` | The project name. |
| `top_level` | The files that hold boards or systems. `top_level = []` is a project of modules only: `build` and `fab` with no file then say `error: this project has no boards`. |
| `out` | Where `netsmith build` writes. |
| `fab` | Where `netsmith fab` writes. |
| `kicad` | The KiCad version of the files. Only 9 is accepted. |
| `catalog_dirs` | The folders whose `*.csv` files are the project's own catalogs, each named by its file name (`catalogs/acme_ldo.csv` is the catalog `acme_ldo`), in addition to the standard ones. Default `["catalogs"]`; `init` writes it out. See Catalog files. |
| `catalogs` | Optional. Single catalog files to add that are not in a `catalog_dirs` folder, each named by its catalog. See Catalog files. |
| `assembly` | Optional. Who assembles a part nothing else speaks for: `"fab"`, `"hand"`, `"none"` or `"consigned"`. See Who assembles a part. |
| `guidelines` | Optional. What a design that goes against a guideline gets: `"warn"` (the default) reports it and builds, `"error"` refuses to build. `--strict` asks for `"error"` for one run. See Track width against current. |

### Orders

An order says which boards go into one fabrication order and how many of each (ADR 0035). It is a table `[order.<name>.boards]` beside `[project]`: each key is a board as its output folder is named (`Board`, or `System/member`), or a system's name for all its members, and the value is the quantity.

```toml
[project]
name = "product"
top_level = ["src/devkit/devkit.nsm", "src/unit/unit.nsm"]

[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10

[order.spares.boards]
DevKit = 5
```

`netsmith fab --order` lists the orders. `netsmith fab --order devkit` makes only those boards, in `fab/orders/devkit/<board>/`, and writes `order.txt` (each board's layers, size, thickness, hand-solder parts and notes, the combined bill of materials, the unverified items) and `order-bom.csv` (each LCSC number with copies x parts per board, no spares). A plain `netsmith fab` still makes every board. See `docs/PROJECT-LAYOUT.md`.

- An order name is letters, digits, `-` and `_`. A quantity is a whole number of 1 or more.
- A board name that is not in the project, or a board named twice (`DevKit` and `"DevKit/carrier"` in one order), is an error with the manifest line, listing the boards that exist.
- A key may be a panel (ADR 0040), and its quantity then counts panels: `LedPanel = 3` orders three panels, so 18 boards if the panel holds six. `order.txt` says so, and `order-bom.csv` multiplies the panel's parts by the quantity. A board may be ordered loose and in a panel; the two are summed in the bill of materials.

Rules.

- The commands look for `netsmith.toml` in the current directory and then in each parent.
- With no file argument, `netsmith build`, `fab` and `editor` act on `top_level`. With a file they act on that file and still see the project's modules.
- Every other file under `src/` is a module: `src/parts/status.nsm` is `src::parts::status`.
- Paths in the manifest stay inside the project, and a catalog file is named for its catalog. An unknown key is refused with its line.
- A file name that cannot be written in an import is refused, naming the file.
- A `.csv` file in a `catalog_dirs` folder that is not named like a catalog is refused, naming the file.

Misuse.

```text
$ netsmith build        # netsmith.toml has `topl = [...]`
/path/to/project: netsmith.toml: TOML parse error at line 3, column 1
  |
3 | topl = ["src/main.nsm"]
  | ^^^^
unknown field `topl`, expected one of `name`, `top_level`, `out`, `fab`, `kicad`, `catalog_dirs`, `catalogs`
```

```text
$ netsmith fab --order devkit        # "Unitt" is not a board of the project
netsmith.toml:8: order `devkit` names `Unitt`, but the project has no such board, panel or system; the boards and panels are: DevKit/carrier, DevKit/power_card, Unit
```

## Commands

```sh
netsmith init [--name <name>]          # a project in the current directory
netsmith build [file.nsm] [--out dir] [--strict] # check, solve, write a KiCad schematic
netsmith editor [file.nsm] [--port n]  # edit and view in a browser (default port 8787)
netsmith fab [file.nsm] [--out dir] [--route-timeout secs] [--allow-drc-violations] [--cross-check] [--strict] [--constraints warn|error] # files for a board house (boards and panels), after a design rules check
netsmith fab --order [name] [--out dir] # a named order of boards and quantities; no name lists the orders
netsmith compare <a> <b> [options]     # are two designs the same circuit?
netsmith kicad-libs import <sym-lib-table> [fp-lib-table]  # the [[library]] stanzas for kicad.toml, from a KiCad project's library tables (ADR 0021)
netsmith catalog refresh [path...] [--dry-run] [--offline] [--only <catalog>] [--lcsc <C123>] [--delay <ms>] [--cache-ttl <hours>] [--no-cache]
netsmith catalog age [path...] [--days <n>] [--only <catalog>]  # rows whose date is older than n days (default 90)
netsmith dxf import <file.dxf> [--layer <name>]... [--holes-layer <name>]... [--units mm|cm|m|inch] [--board <name>] [--write <file.nsm>]  # an enclosure's outline, cutouts and holes as source to paste
```

`editor` serves the editor on `127.0.0.1` and prints the address to open: a project tree, the source with the compiler's and solver's messages beside the lines they are about, and views of the selected board (schematic, power tree, system, bill of materials, PCB) with buttons for `build` and `fab` (ADR 0019). A board of more than 16 parts is drawn in a box for each module instance. A rail is drawn with the ground and supply symbols at its pins. Errors and warnings are listed first, and the notes (relations not evaluated, balances left incomplete) in a closed section. It checks the text as you type without writing it, saves with `Ctrl+S`, reads a file again when it changes on disk, and with no file acts on the project like `build`.

`--strict` (on `build` and `fab`) makes a finding against a guideline an error for this run, like `guidelines = "error"` in `netsmith.toml`; without it a finding is a warning.

The power model's findings come from `netsmith`, not from the editor's page: `build`, the editor's analysis (its `power` stage, the Problems dock's `Power` group) and the power tree it draws all carry the same ones. A rail that can draw more than the supply that states what it can give (`max_current`) is an error that stops the build, where the solver had no relation to check it against; `--strict` is not needed. The notes do not stop it, with or without `--strict`: a rail nothing supplies (`V5 has no source: nothing supplies it`), a rail with loads whose current the solver has not settled, and a converter whose efficiency it has not settled. The Problems dock lists the notes of a board grouped by their cause, with a count for each (the server says it for every note: `not_stated`, `part_model`, `unjudged`, `circular`, `supply`, `pick`, `track`, `other`), so a long list of notes reads as the few things to do about them: a rail nothing supplies and a load not settled are `supply`, a level or an input current a part's definition leaves out is `part_model`, a value nothing states is `not_stated`.

`fab` writes the Gerbers, drill files and placement file itself and runs netsmith's own design rules check (ADR 0037), so it does not need KiCad. An error of the check (a clearance, a short, a missing connection, a hole, the edge, a courtyard, a keepout) stops `fab` unless you pass `--allow-drc-violations` (which never waives a `[constraint]` finding: a requirement on where a part goes is relaxed only by `constraints = "warn"` or `--constraints warn`), and `fab-notes.txt` then says the board is not clean; its warnings (silkscreen on a pad, a narrow mask web, a via in a pad) are listed in the notes. `--cross-check` compares the files and the findings with KiCad's own plot and design rules check of the same board (it needs KiCad's command-line tool: `$KICAD_CLI`, then `kicad-cli` on `PATH`, then Windows KiCad 9 under WSL). It writes, for each board, a Gerber zip, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the `.kicad_pcb` and `.kicad_pro`, and an `.svg`. Read `fab-notes.txt` before you order.

`catalog refresh` is the one command that uses the network, and it is opt-in (ADR 0039): nothing else, and never the solver, asks anyone anything. **JLCPCB data is not fetched**: the command asks LCSC only (ADR 0039, the 2026-11-02 decision). For each row with an `lcsc` number it reads LCSC's record of that part and checks the manufacturer's number and the package against the row. A row that agrees is `VERIFIED`, and if the file has a `checked` column it gets the day there: that is the only cell the command ever writes. It **never writes `price`, `stock`, `price_breaks`, `library` or `date`**: the board is assembled from JLCPCB's parts library, those cells are JLCPCB's numbers entered by hand, and LCSC's price (often at a first break above one piece) and stock differ from them. It may say, as a note and not a finding, that LCSC lists stock 0 where the row has some (or the reverse), so you read JLCPCB's parts page by hand; the age of a row's own numbers is still its `date`, so a refresh never makes a stale row fresh. It keeps the order, quoting and line endings of the file, so the result is a small diff. With no path it acts on the project's catalog files (the `*.csv` of its `catalog_dirs` folders and the files under `catalogs`) and, in the netsmith repository, on `data/jlcpcb/catalog/`; a path is a catalog file or a directory of them (the standard catalogs inside the binary cannot be written, so they are checked in the repository and arrive with a release). A row is reported, not changed, when the records disagree with it: `CONFLICT` if the manufacturer's number or the package is not the row's, `MISSING` if LCSC no longer lists the number (a row is never deleted); `SKIPPED` is a row `--offline` has no kept answer for. `--dry-run` prints the diff, cell by cell, and writes nothing; `--only` and `--lcsc` choose catalogs and rows. The exit code is 0, 1 if any row is `CONFLICT` or `MISSING`, and 2 when the run had to stop (the endpoint refused, or answered in another shape: nothing is written then). LCSC's endpoint is unofficial and LCSC's terms of use have not been read: a person must read them before the command is advertised, and official APIs (JLCAPI, the LCSC API; they need an account) are future work (ADR 0039). Meanwhile the command is polite: one request at a time, `--delay` milliseconds apart (default 500), a User-Agent that says what it is, answers kept for `--cache-ttl` hours (default 24; `--no-cache` keeps none) and retried with a growing wait. `--offline` asks nothing and uses the kept answers of any age. `catalog age` prints the rows whose price and stock were read more than `--days` days ago (the supply provider's answer, else the row's `date`) and, apart, the rows whose price and stock are unknown because no provider has them and the row has no `date` (unknown is not old and not 0); it reads the `[supply]` table of the project that contains the directory. The editor's bill of materials marks the old lines, says `unknown` for the rest, and has a "Check parts against LCSC" button for the project's own catalogs.

`dxf import` reads the board outline of an enclosure from an ASCII DXF file (ADR 0045) and prints, to paste into the `case { }` section of the board's physical file, the `import` lines, an `.outline: Outline`, a `Cutout` with its `.x` and `.y` for each loop inside it, and a `MountingHole` for each circle on the holes layer with the requirement on the centre of its body (`.hole1.place.x &= 5mm`). It writes nothing by default. `--write <file.nsm>` edits `<stem>.physical.nsm` beside that file (ADR 0048): it makes the file, the unit or the `case` section if they are not there, replaces the members of an earlier import (`outline`, `cutout1`, `hole1`...) and the lines that state their places, keeps every other line and every comment, and prints a diff; `--board` names the board when the file has more than one. The circuit source and the layout file are never written. The reader takes `LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE` (closed flag, bulges), `POLYLINE` with `VERTEX`, `SPLINE` and `ELLIPSE` (as chords within 0.005 mm), and `INSERT` of blocks (translate, rotate, scale, mirror); text, dimensions and the like are not read and are counted in a warning. Binary DXF is refused. Units come from `$INSUNITS` (millimetre, centimetre, metre, inch); a file without them needs `--units`. Edges whose ends are within 0.001 mm join into closed loops; a gap is listed with its size and place. The one loop that is not inside another is the outline and the loops inside it are cutouts; two outer loops, a loop that crosses itself, a loop that crosses a cutout or only touches the outline, or a loop inside a cutout is an error. A loop that crosses the outline (a window in a case wall that is partly beyond the board) is an *opening*: the import leaves it out and prints a note with its layer, size and place on the board, and imports the rest (provisional, issue #90). `--layer` names the layer of the board edge and `--holes-layer` the layer of the holes (both may be repeated); a layer that the file's layer table declares is accepted when nothing is drawn on it, so a case with no holes can still pass `--holes-layer`, and a name that is nowhere in the file is an error that lists the layers the file has; with neither, the one layer that has lines or arcs is used, and if there are several the error lists them with their entity counts. A circle on the outline layer is a round `Cutout`; on the holes layer it is a hole, which must be a `MountingHole` package, `M2` (2.2 mm) or `M3` (3.2 mm); a hole of another diameter is refused with the nearest package and the difference, because the language cannot state it yet (issue #89). The board's frame is the outline's top left corner with y down, and the output says which point of the drawing that is. The outline must be a rectangle with equal rounded corners and each cutout a rounded rectangle, a slot or a circle: any other shape is refused with its size and edge count, because the board model has no polygon outline yet (ADR 0045 step C8).

`compare` checks a design against another by connectivity, for example a port against the KiCad design it came from. `<a>` and `<b>` are each a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (the XML of `kicad-cli sch export netlist --format kicadxml`) or a `.nsm` file, which is built as `build` does; `--board <name>` picks the board of a `.nsm` file that has several (`Name`, or `System/member`). Net names and, by default, designators may differ between the two; components are matched by value, footprint (without its library) and how their pins are wired. It prints `match` with the designators and net names that correspond, or each difference: a component with no counterpart, a pair that differs in value or footprint, a net whose pins differ (each pin in its own side's designators), a net with no counterpart. The exit code is 0 for a match, 1 for a difference and 2 for a mistake or when the comparison could not be decided (many interchangeable parts and a search past its bound). Options:

- `--by-refdes`: designators are identity; a component matches only the component with the same designator.
- `--map <file>`: lines `A_ref B_ref` fix pairs of designators; the rest are matched by wiring. Blank lines and `#` comments are ignored. Not with `--by-refdes`.
- `--ignore-unconnected`: leave out nets of a single pin on both sides.
- `--loose-values`: values that read as the same number agree (`4k7` and `4.7k`, `100n` and `0.1uF`).

Power ports and flags (`#PWR`, `#FLG`) are not parts. A pin joined to nothing is on no net, so it differs only from a pin that is joined to something. Reading a `.kicad_sch` needs KiCad's command-line tool, found as for `fab`.

## Standard parts

The library has the parts a two-sided four-layer board is made of, and the interfaces they talk over (ADR 0023). They state what a datasheet says. Each rating is a field that the part leaves open and a relation that is always true of it: a design narrows the field with `&=`, as it narrows a resistance, and the solver checks the relation in the worst case. A rating the design never states is listed as not evaluated, and is no error. Most of them have a small catalog of real parts (ADR 0025): a design that states no rating gets the first row that works, and a rating it does state must be a range that contains the row's (`.zener_voltage &= 4.7V..5.5V` takes a row that reads `4.8V..5.4V`). The guide's examples state what the board needs and let the catalog supply the rest. `build` stops at the export of any of them that has no KiCad symbol yet (see Parts), so the examples here are checked and solved and not built (the guide marks them `evaluate`).

### Voltage across a part

A diode, a Zener, a TVS diode and a capacitor have a `.bias`: the DC voltage across them. A part placed between a rail's `pos` and `neg` takes it from that rail (ADR 0013), so a rating on the voltage across the part needs no line of its own. A part across no rail has no bias until the design states `.bias`.

A capacitor is the same either way round, so its bias is the rail's voltage and the sign is not looked at. A diode is not: its `.bias` is the voltage in the direction it blocks, the cathode relative to the anode, and the checker gives a diode across a rail the rail's voltage with the sign of the way it is wired (ADR 0088, provisional). It does so through `.bridge_voltage`, a field of `Diode`, `Schottky`, `Zener` and `Tvs`: the voltage from the first pin of the part's `bridge` to the second (the anode relative to the cathode for a `Diode` or a `Schottky`, the cathode relative to the anode for a `Zener` or a `Tvs`, whose bridge is the way they conduct when they clamp), `+` the rail's voltage when the first pin is on the rail's `pos` and its negative when it is on the `neg`. A diode with its cathode on `pos` blocks the rail and has the rail's voltage as its bias. A diode wired the other way round conducts across the rail, so its bias is negative and the part's own relation names it: a `Diode` or a `Schottky` must have `.bias >= -.forward_voltage` (it never has more than a forward drop the wrong way), and a `Zener` or a `Tvs` must have `.bias >= 0V`, the way round it is a clamp. `.bridge_voltage` is an `optional` member (ADR 0074): a diode across no rail has none, and the relations that read it are neither checked nor listed as not evaluated. A design does not write it; the ratings of the table below follow from it.

| Part | What is always true | What the design states |
|---|---|---|
| `std::parts::diode::Diode`, `std::parts::diode::Schottky` | `.bias <= .max_reverse_voltage`, `.current <= .max_forward_current`, `.bias >= -.forward_voltage`, `.supply_fault_current <= .max_forward_current`, `.supply_reverse_peak <= .max_reverse_voltage` | The ratings; the last two are from the chain (ADR 0091). |
| `std::parts::diode::Zener` | `.bias * .current <= .max_power`, `.bias >= 0V`, `min(.bias) <= max(.zener_voltage)` | `.zener_voltage` with its tolerance. For a clamp, the line `.z.bias <= .z.zener_voltage`; over a temperature range, `.temperature_shift` (below). Catalog `zener`. |
| `std::parts::diode::Tvs` | `.bias <= .standoff_voltage`, `.clamping_voltage <= .protected_limit`, `.bias >= 0V`, `-.supply_reverse_peak >= 0V` | The ratings (the last from the chain, ADR 0091), and `.protected_limit`: the absolute maximum of what is behind it. Catalog `tvs`. |
| `std::parts::diode::TvsBidirectional` | `.bias <= .standoff_voltage`, `.clamping_voltage <= .protected_limit` | The ratings, and `.protected_limit`. Catalog `tvs_bidirectional`. |

A `Tvs` also has an optional `.breakdown_voltage`, the minimum breakdown voltage VBR: between the stand-off voltage and it the diode leaks a little, below it the diode does not conduct (ADR 0073, provisional). A design that needs a margin between the highest the line reaches and the point where the diode starts to conduct states it, `.tvs.breakdown_voltage >= .input.peak * 1.1`. It is a catalog column that only one standard row fills, the Littelfuse SMDJ36A (`data/jlcpcb/README.md`), so a design that states it is offered that row and no other standard one, and gets any other from a project's `tvs` catalog file that records the column; a file written before the column may leave it out, and a design that does not state the field picks as before and has no new check.

A Zener directly across a rail is refused when it surely conducts (issue #293, ADR 0098, proposed): it states `min(.bias) <= max(.zener_voltage)`, so a rail whose least voltage is above the Zener's greatest breakdown voltage puts the part in breakdown all the time, with nothing to limit its current. The message names the part, the rail's least voltage and the greatest breakdown (`min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V`), and the way out is a Zener that breaks down at the rail's voltage or a resistor in series. The rule catches only the sure case: a rail range that straddles the breakdown (4 V to 6 V against 4.7 V to 5.5 V) may conduct, and passes. A Zener behind a series resistor has no rail across it, so its `.bias` has no value, and the rule waits and is listed as not evaluated. The rule also chooses among the catalog rows, as any relation of a part does: a design that states a breakdown the rail is above gets the error, but one that states none is given a row whose breakdown reaches the rail (a 12 V rail across a Zener with no `.zener_voltage` is given the BZT52C12), not an error.

```nsm fails
import std::parts::diode::Zener
import std::sources::Rail

board Conducts {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .z: Zener { .zener_voltage &= 3.1V..3.5V }
    .input.pos ~> .z ~> .input.neg
}
```

```text
error: no `zener` candidate satisfies `z`: the closest of the 1 that fit, BZT52C3V3 3.3V 500mW Zener SOD-123 C2112, fails because min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V: no value of the two ranges meets it, so change one of them
    --> std/parts/diode.nsm:231:5
    |
231 |     min(.bias) <= max(.zener_voltage)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V: no value of the two ranges meets it, so change one of them
    --> std/parts/diode.nsm:231:5
    |
231 |     min(.bias) <= max(.zener_voltage)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A Zener has the figures a design needs to bound a clamp over temperature (issue #199; the first four are optional catalog columns, ADR 0078, which the standard `zener` rows fill from the datasheet table that each row's notes name):

| Member | What it is |
|---|---|
| `.test_current` | The current the datasheet publishes `.zener_voltage` at (IZT). The row's `.zener_voltage` is the minimum to maximum at this current. |
| `.temperature_coefficient` | The change of `.zener_voltage` for one kelvin, as the datasheet's range. A `Voltage`, because no kind holds volts per kelvin: `18.4mV..22mV` is "18.4 to 22 mV/C". The datasheet's figure is a typical one, so the margin is the design's. |
| `.leakage_current`, `.leakage_test_voltage` | The reverse current below the breakdown, and the reverse voltage it is stated at. |
| `.temperature_shift` | How far the temperature goes from the 25 C the datasheet states, in kelvin: `-65K..100K` for -40 C to 125 C. A design states it; it is an `optional` member (ADR 0074). |
| `.voltage_over_temperature` | `.zener_voltage + .temperature_coefficient * .temperature_shift`, worst case: the upper end of the range is the voltage at the hot corner and the lower end the voltage at the cold one. |

A column that a row leaves empty is no obstacle to a design that does not mention it, and a design that names one (`.z.leakage_current <= 1uA`) is offered only the rows that state it. A design that states no `.temperature_shift` has no `.voltage_over_temperature`: nothing is checked, nothing is listed for the relation, and the row's coefficient is not asked for, so every board written before the columns picks as it did. A design that states the shift is offered only the rows that state the coefficient, so a row without it is never picked with a wrong number. A bound on `.voltage_over_temperature` with no shift stated is listed as not evaluated.

A supply pin held under its absolute maximum by a series resistor and a Zener. The board's own numbers: a 24 V input, a pin that stands 18 V, and a clamp that must hold from -40 C to 125 C. The picked row is BZT52C15 (13.8 V to 15.6 V at 5 mA, 9.2 to 13.0 mV/C). Its hot corner is 15.6 V + 13.0 mV * 100 = 16.9 V, under the 18 V; its cold corner is 13.8 V + 13.0 mV * (-65) = 12.955 V (a message prints it as `12.95V`). The Zener's voltage is that whole range, so the current through it and its power are checked at the worst end of it.

```nsm evaluate
import std::parts::diode::Zener
import std::parts::passive::Resistor
import std::sources::Rail

board PinClamp {
    .input: Rail {
        .voltage &= 24V +/- 10%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .r: Resistor { .resistance &= 2.2kohm +/- 1% }
    .z: Zener {
        .zener_voltage &= 13V..16V
        .temperature_shift &= -65K..100K
    }
    .z.bias == .z.voltage_over_temperature
    .z.current == (.input.voltage - .z.voltage_over_temperature) / .r.resistance
    .z.voltage_over_temperature <= 18V
    .input.pos ~> .r ~> .z ~> .input.neg
}
```

A pin that cannot stand the clamp's hot corner is refused with the numbers (the limit here is below what any Zener of that voltage holds):

```nsm fails
import std::parts::diode::Zener
import std::parts::passive::Resistor
import std::sources::Rail

board PinClamp {
    .input: Rail {
        .voltage &= 24V +/- 10%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .r: Resistor { .resistance &= 2.2kohm +/- 1% }
    .z: Zener {
        .zener_voltage &= 13V..16V
        .temperature_shift &= -65K..100K
    }
    .z.bias == .z.voltage_over_temperature
    .z.current == (.input.voltage - .z.voltage_over_temperature) / .r.resistance
    .z.voltage_over_temperature <= 10V
    .input.pos ~> .r ~> .z ~> .input.neg
}
```

```text
error: no `zener` candidate satisfies `z`: the closest of the 1 that fit, BZT52C15 15V 500mW Zener SOD-123 C2104, fails because z.voltage_over_temperature must always be at most 10V, but 12.95V..16.9V is not always below 10V
   --> main.nsm:18:5
   |
18 |     .z.voltage_over_temperature <= 10V
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A 12 V input with a fuse and a TVS diode. The values are the board's own, as if copied from datasheets.

```nsm evaluate
import std::parts::diode::Tvs
import std::parts::passive::Fuse
import std::sources::Rail

board Protected {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 100mA..400mA
        .pos.net &= "VFUSED"
        .neg.net &= "GND"
    }
    .fuse: Fuse { .rated_current &= 1A }
    .tvs: Tvs { .protected_limit &= 36V }
    .input.pos ~> .fuse ~> .load.pos
    .input.pos ~> .tvs ~> .input.neg
    .input.neg ~ .load.neg
}
```

The rail across the TVS diode must stay under its stand-off, and the clamp must stay under what the board says it protects:

```nsm fails
import std::parts::diode::Tvs
import std::sources::Rail

board Standoff {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .tvs: Tvs { .protected_limit &= 14V }
    .input.pos ~> .tvs ~> .input.neg
}
```

```text
error: no `tvs` candidate satisfies `tvs`: the closest of the 10 that fit, SMAJ12A 12V TVS SMA C113957, fails because tvs.bias must always be at most tvs.standoff_voltage, but 11.4V..12.6V is not always below 12V
    --> std/parts/diode.nsm:307:5
    |
307 |     .bias <= .standoff_voltage
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A margin on the breakdown voltage is a requirement almost no standard row can meet: only the SMDJ36A records one (40 V), and a margin of three times the peak, 48 V, is more than that. The message names the closest row:

```nsm fails
import std::parts::diode::Tvs
import std::sources::Rail

board Margin {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .peak &= 16V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .tvs: Tvs {}
    .tvs.breakdown_voltage >= .input.peak * 3
    .input.pos ~> .tvs ~> .input.neg
}
```

```text
error: no `tvs` candidate satisfies `tvs`: the closest of the 1 that fit, SMDJ36A 36V TVS SMC C108363, fails because tvs.breakdown_voltage must always be at least (input.peak * 3), but 40V is not always above 48V (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 9 for `breakdown_voltage`)
   --> main.nsm:12:5
   |
12 |     .tvs.breakdown_voltage >= .input.peak * 3
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A `TvsBidirectional` is the TVS diode for a line that swings both ways or may be wired backwards, such as a power input on a vehicle (ADR 0089, provisional). It is two junctions back to back, so it has no anode and no cathode: its pins are `a1` and `a2` (KiCad's `Device:D_TVS`, drawn with two opposed triangles, where `Tvs` is drawn with the Zener symbol), and it is the same either way round. Its fields and checks are those of `Tvs`, read for either direction: the rail across it must stay under the stand-off, and the clamp under `.protected_limit`. Like a capacitor it takes the rail's voltage as its `.bias` whichever pin is on `pos`, so there is no wrong way to wire it. The standard `tvs_bidirectional` catalog has two Littelfuse rows, the SMDJ12CA and the SMDJ36CA (SMC), each with its minimum breakdown voltage from the datasheet table. The 12 V rail below reaches 12.6 V, so the picker passes over the SMDJ12CA (12 V stand-off) and chooses the SMDJ36CA, whichever way the diode is drawn into the chain:

```nsm evaluate
import std::parts::diode::TvsBidirectional
import std::sources::Rail

board Input {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .tvs: TvsBidirectional { .protected_limit &= 60V }
    .input.neg ~> .tvs ~> .input.pos
}
```

A Zener used as a clamp, and a diode that blocks less than the rail it sits across:

```nsm fails
import std::parts::diode::{Diode, Zener}
import std::sources::Rail

board Clamp {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .z: Zener { .zener_voltage &= 11V..13V }
    .z.bias <= .z.zener_voltage
    .d: Diode { .max_reverse_voltage &= 10V }
    .input.pos ~> .z ~> .input.neg
    .input.neg ~> .d ~> .input.pos
}
```

```text
error: no `zener` candidate satisfies `z`: the closest of the 1 that fit, BZT52C12 12V 500mW Zener SOD-123 C43491, fails because z.bias must always be at most z.zener_voltage, but 11.4V..12.6V is not always below 11.4V..12.7V
   --> main.nsm:11:5
   |
11 |     .z.bias <= .z.zener_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: d.bias must always be at most d.max_reverse_voltage, but 11.4V..12.6V is not always below 10V
   --> std/parts/diode.nsm:91:5
   |
91 |     .bias <= .max_reverse_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: z.bias must always be at most z.zener_voltage, but 11.4V..12.6V is not always below 11V..13V
   --> main.nsm:11:5
   |
11 |     .z.bias <= .z.zener_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A diode wired the wrong way round across a rail conducts and is a short, and a rating of the voltage it blocks cannot see that by itself. The bias of one is negative, so the part's relation on `.bias` fails and names it. Here the rectifier has its anode on `pos`, and the Zener its anode on `pos` too. The rectifier's check compares with its `.forward_voltage`, which a `Schottky` has from its catalog row and a `Diode` has when the design states it; a `Diode` with none is listed as not evaluated:

```nsm fails
import std::parts::diode::{Diode, Zener}
import std::sources::Rail

board Reversed {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .d: Diode {
        .max_reverse_voltage &= 40V
        .forward_voltage &= 0.7V
    }
    .z: Zener { .zener_voltage &= 4.7V..5.5V }
    .input.pos ~> .d ~> .input.neg
    .input.neg ~> .z ~> .input.pos
}
```

```text
error: no `zener` candidate satisfies `z`: the closest of the 1 that fit, BZT52C5V1 5.1V 500mW Zener SOD-123 C2117, fails because z.bias must always be at least 0V, but -12.6V..-11.4V is not always above 0V
    --> std/parts/diode.nsm:230:5
    |
230 |     .bias >= 0V
    |     ^^^^^^^^^^^
error: d.bias must always be at least -d.forward_voltage, but -12.6V..-11.4V is not always above -700mV
   --> std/parts/diode.nsm:92:5
   |
92 |     .bias >= -.forward_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
error: z.bias must always be at least 0V, but -12.6V..-11.4V is not always above 0V
    --> std/parts/diode.nsm:230:5
    |
230 |     .bias >= 0V
    |     ^^^^^^^^^^^
```

### Resistor power

A `Resistor` has a power rating, `.max_power`, and states that the power it dissipates, `.drop * .current`, is at most that rating, in the worst case over the ranges of its current and its resistance. The rating is a column of the `resistor` catalog (JLCPCB lists it for every part: 62.5 mW for the 0402 parts, 100 mW for the 0603 and 250 mW for the KOA 0805 parts), so the solver picks a resistor that is big enough as well as one of the right value. A design that needs more says so with the rating or the package: `.r.max_power >= 250mW`.

The worst case is over the current the circuit really carries. `.led.current &= 5mA..15mA` is a window the LED must stay in, and `.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance` says what the current is for the resistor that was tried: once the resistor is known, the current is the equation's range (7.05 mA to 10.56 mA for 330 ohm), and the resistor is checked at that, not at the 15 mA that ends the window. A window that is not tied to the circuit by an equation (`.current &= 45mA..49mA` on a resistor alone) is still taken at its upper end.

A resistor whose current nothing states has no power to check, unless Ohm's law bounds it: when the voltage at each end of the resistor lies in a range the solver knows (a rail's stated range, `0 V` up to the supply of a `Logic` line that drives it, the ends of the resistors beside it, the ground), the current is at most the largest difference between the two ends over the least resistance, and the power at most the square of that difference over the least resistance. A check that this bound proves (a 10 kohm pull-down on a 3.3 V line carries at most 350 uA and 1.2 mW against 62.5 mW) is made, and its note goes. A check that the bound does not prove is left not evaluated, as before, and nothing says it failed: the bound does not know what the pins on the line draw, so it is far above the real current for a 100 ohm resistor in series with an input, and the real current may well pass. The bound is never given to the part as its current: a value that nothing states is still not known, and no pick, range or track width reads the bound as a current. It needs a range at both ends: a line that a pin drives that is not the `line` of a `Logic` is held to the supplies of that pin's own part, and a net with a connector pin, a part whose pins conduct, or a rail with no voltage on it has no range and so no bound. A catalog row with no `max_power` (an empty cell) is an unrated part: it is never offered to any design, and the "nothing for it" message says how many rows were left out for that reason. Fill the cell from the datasheet or leave the part out.

A 10 ohm resistor that carries 100 A dissipates 100 kW, far more than any surface-mount resistor is rated for. The catalog's biggest 10 ohm part is rated 250 mW, so the solver names it as the closest and says by how much it fails:

```nsm fails
import std::parts::passive::Resistor

board Hot {
    .r: Resistor {
        .resistance &= 10ohm +/- 1%
        .current &= 100A
    }
}
```

```text
error: no `resistor` candidate satisfies `r`: the closest of the 3 that fit, 10Ω 1% 0805 C317221, fails because (r.drop * r.current) must always be at most r.max_power, but 99kW..101kW is not always below 250mW
   --> std/parts/passive.nsm:41:5
   |
41 |     .drop * .current <= .max_power
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

And a row without a rating:

```csv file=catalogs/resistor.csv set=unrated
mpn,resistance,max_power,package,library,price,stock,date,source
ACME-R316,316ohm +/- 1%,,0402,Basic,0.0011,5000,2026-10-31,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=unrated
import std::parts::passive::Resistor

board Unrated {
    .r: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: the `resistor` catalog has nothing for `r`, which needs resistance within 312.8Ω..319.2Ω (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 1 for `max_power`)
  --> main.nsm:4:5
  |
4 |     .r: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^
```

### Adjustable outputs

An adjustable regulator holds its feedback pin at a reference voltage `vref`, and a divider from the output to the pin sets the output: `vout = vref * (1 + rtop / rbottom)`. `std::parts::regulator::FeedbackDivider` is that divider as a module (ADR 0025). Its interface is three pins, `top` (joined to the output's positive pin), `fb` (the regulator's feedback pin) and `bottom` (the output's ground), and three values: `vref`, which the board states with the datasheet's tolerance, `vout`, the output it wants, and `current`, what it draws from the output. The solver picks `rtop` and `rbottom` from the resistor catalog so that, with the tolerance of the reference and of both resistors, the output stays inside the wanted range in the worst case. It evaluates the formula tightly (each resistance appears in the ratio once and the reference once), so the tolerance a design has to allow is the real one.

The feedback pin is taken to draw nothing, so the divider carries `(vout - vref) / rtop` through both resistors, and each resistor's power rating is checked against it. The divider is pins and voltages and no rail, so it adds nothing to the currents that are summed at a net (ADR 0007): its current, a few microamps to a milliamp, is not added to the load.

A 3.8 V output from a 1.0 V reference, each good to 1%, with the output allowed 3% either way:

```nsm solve
import std::parts::regulator::FeedbackDivider
import std::sources::Rail

board Adjustable {
    .divider: FeedbackDivider {
        .vref &= 1.0V +/- 1%
        .vout &= 3.8V +/- 3%
        .current <= 1mA
    }
    .out: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }
    .divider.top ~ .out.pos
    .divider.bottom ~ .out.neg
    .out.voltage == .divider.vout
}
```

The output fixes the ratio of the two resistors and not their size, so a board also says how much the divider may draw (`.current <= 1mA`); without it the resistors are unconstrained in size and the build says so (see "A value that nobody stated"). The solver picks 6.2 kohm over 2.2 kohm, whose worst-case output is 3.686 V to 3.914 V. The reference alone is good to 1% and the two resistors add their tolerance to it, so asking for 1% either way cannot be met, whatever resistors a catalog holds. The message names the closest pair and the range its output works out to:

```nsm fails
import std::parts::regulator::FeedbackDivider
import std::sources::Rail

board TooTight {
    .divider: FeedbackDivider {
        .vref &= 1.0V +/- 1%
        .vout &= 3.8V +/- 1%
    }
    .out: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }
    .divider.top ~ .out.pos
    .divider.bottom ~ .out.neg
    .out.voltage == .divider.vout
}
```

```text
error: no combination of candidates satisfies `divider.rbottom` and `divider.rtop` together: the closest, divider.rbottom = 20kΩ 1% 0402 C25765 and divider.rtop = 56kΩ 1% 0603 C23206, fails because `divider.vout` must stay within 3.762V..3.838V, but (divider.vref * (1 + (divider.rtop.resistance / divider.rbottom.resistance))) works out to 3.707V..3.896V
   --> std/parts/regulator.nsm:90:5
   |
90 |     .vout == .vref * (1 + .rtop.resistance / .rbottom.resistance)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

An output below the reference cannot be made, because the divider can only raise it: `.vout &= 0.9V +/- 1%` with a 1.0 V reference is refused in the same way.

### Switching regulators

A buck converter is an IC, an inductor, two capacitors and a feedback divider. The IC is the one part that differs, so the library splits the work (ADR 0025). `std::parts::regulator::Buck` is an interface of what a datasheet says: the switching node `sw` (a rail the IC supplies, joined to the inductor), the feedback pin `fb`, the reference `vref`, the switching frequency `fsw`, the input range, the largest output current, the efficiency as a range, the smallest inductance, optionally the least saturation current (`min_inductor_saturation`) and the largest DC resistance (`max_inductor_dcr`) an inductor may have, optionally the minimum on-time and off-time of the switch (`min_on_time`, `min_off_time`, ADR 0093), and the capacitance asked for on each side. `std::parts::regulator::BuckStage` is a module of everything else: the inductor and the two capacitors, picked from the catalogs (each capacitor rated for the `.peak` of its rail when the board states one and always for the rail's steady voltage, so a peak stated too low cannot weaken it; each side a bank of identical capacitors in parallel when one is not enough), and a `FeedbackDivider` that sets the output from the reference.

A concrete IC is a `part` that writes its pin table, its datasheet numbers, one relation, the input current from the output current and the efficiency (`vin = vout * iout / efficiency` over the whole efficiency range, so it reaches the supply as any load does, ADR 0007 and ADR 0014), and what it is at DC, `dc switching_regulator .vin .buck.sw .buck.efficiency` (and, for an IC whose datasheet gives its own current with nothing loaded, a fourth operand, `.quiescent_current`, ADR 0096; see "What a part is at DC" and "DC analysis": it lets the solver work out the input current through the parts in front of the IC, and the input voltage they leave it). This one is invented for this guide:

```nsm module=src::zbk2
import std::parts::regulator::Buck
import std::sources::Rail

/// A 2 A buck converter for 4.5 V to 6 V. Illustrative; not a real part.
part Zbk2 {
    .vin: Rail
    .buck: Buck {
        .vref &= 0.8V +/- 1.5%
        .fsw &= 1.2MHz +/- 15%
        .min_input_voltage &= 4.5V
        .max_input_voltage &= 6V
        .max_output_current &= 2A
        .efficiency &= 0.85..0.95
        .min_inductance &= 1.5uH
        .min_inductor_saturation &= 3A
        .max_inductor_dcr &= 0.1ohm
        .min_input_capacitance &= 2uF
        .min_output_capacitance &= 10uF
    }
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)
    dc switching_regulator .vin .buck.sw .buck.efficiency

    pins {
        //  pin          kind    name   pads
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .buck.sw.pos   power   "SW"   "4" "5"
        .buck.fb       input   "FB"   "6"
        .en            input   "EN"   "7"
    }
}
```

The board places the IC and the stage, states the input and the output it wants, and joins the supply to both, the stage's output to the load and the IC's `buck` to the stage's:

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk2
    .stage: BuckStage { .vout.voltage &= 1.8V +/- 3% }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

The solver picks the inductor (a 2.2 uH 1210 part saturating at 3.6 A, over the IC's 3 A, with 85 mohm, under its 0.1 ohm, and a tolerance that still leaves 1.76 uH, over the IC's 1.5 uH), a capacitor for each side that keeps the effective capacitance the IC asks for at the voltage across it, and the two divider resistors. The supply draws 0 to 459 mA for the 0 to 1 A load. The ripple current is `(vin - vout) * vout / (vin * L * fsw)`, 0.30 A to 0.67 A here, and the inductor must carry the load plus half of it: 1.33 A of 3.6 A.

Each side of the stage is a bank of identical capacitors (the section on capacitors above): `.input_cap` and `.output_cap` stand for one capacitor of the bank, and `.input_count` and `.output_count` say how many. The solver chooses the row and the count together, from 1 to 8, so that `count * effective_capacitance` meets what the IC asks for (the capacitance the datasheet states is for the whole side) and each capacitor is rated for its rail. With the IC of this guide changed to ask for 47 uF at the output and 40 uF at the input, the stage takes two capacitors at the 1.8 V output (the 47 uF row keeps 26 uF of its value there) and four at the 5 V input (it keeps 10 uF). The board reads the count from `.stage.output_count`, states it when it wants one (`.stage.output_count &= 3`, which the checks then hold to), or limits what the solver may choose (`.stage.output_count &= 1..3`). A bank of one is a single capacitor, as the stage had before.

```nsm-fragment
.stage: BuckStage {
    .vout.voltage &= 1.8V +/- 3%
    .output_count &= 3
}
```

A margin on the capacitors' ratings is the stage's `.voltage_margin` (ADR 0085, provisional): each capacitor is rated for at least the margin times the highest voltage of its rail, `.input_cap.max_voltage >= .vin.voltage * .voltage_margin` and `>= .vin.peak * .voltage_margin`, and the same on the output. It is 1 unless the board states it, which asks for no more than the rail's highest voltage and changes nothing for a board that never mentions it. A board that derates its ceramic capacitors states it from outside the stage, which cannot be reached any other way (`input_cap` is inside `BuckStage` and is not part of its interface). A module that wraps the stage forwards its own member to it (`.stage.voltage_margin &= .margin`). The stage has no bootstrap or bypass capacitor, since those are inside the IC's own circuit; a module that adds them states its own bound.

```nsm-fragment
.stage: BuckStage {
    .vout.voltage &= 1.8V +/- 3%
    .voltage_margin &= 1.25
}
```

The 5 V input of this guide peaks at 5.25 V, so 1.25 times it is 6.56 V and a 6.3 V capacitor, which the stage picks by default, is no longer enough: the input takes a row rated for at least that. A margin no row of the catalog can give is refused with the numbers of the closest row, and only the rating fails: with a margin of 5 the input capacitor would have to be rated for 26.25 V, and no row that keeps the capacitance the IC asks for has more than 25 V.

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board TooMuchMargin {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk2
    .stage: BuckStage {
        .vout.voltage &= 1.8V +/- 3%
        .voltage_margin &= 5
    }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

```text
error: TooMuchMargin: L1: footprint `Inductor_SMD:L_1210_3225Metric` not found: looked for /tmp/netsmith-sample-libraries-442f93da51a2cbe0/footprints/Inductor_SMD.pretty/L_1210_3225Metric.kicad_mod; set NETSMITH_KICAD_FOOTPRINTS to the folder that holds the .pretty libraries, or list the library in kicad.toml
```

The two inductor limits are optional, as are the other numbers an IC may leave open: an IC that states `.min_inductor_saturation` (a datasheet that asks for more than the current limit, say) or `.max_inductor_dcr` has them checked against the picked inductor's `saturation_current` and `dc_resistance`, and an inductor that does not meet them is refused with the numbers. Both are `optional` members of `Buck` (ADR 0074): one that the IC leaves unstated has no such check, and the build says nothing about it, where it lists a check on any other value that nothing states as not evaluated. A limit is stated for the stage when the IC states it on its side of `.stage.buck ~ .u.buck`. A heating (RMS) rating of the inductor is not checked by the stage: `Inductor` has the optional column `rms_current`, which the standard rows leave empty (see Dielectric and package size), so a check that needed it would refuse every standard inductor.

Misuse: a load larger than the IC can give is refused twice, once on the stage's output and once on the IC's switch node (`Buck` states `-.sw.current <= .max_output_current`, ADR 0076, amendment 6, which is also the limit that encloses a load nothing states before the picks); the lower end of the current is the divider's, which the DC analysis of the stage's output side gives. An input above its range, such as 12 V, is refused the same way, by `stage.vin.voltage must always be at most stage.buck.max_input_voltage`.

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board Overload {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..3A
    }
    .u: Zbk2
    .stage: BuckStage { .vout.voltage &= 1.8V +/- 3% }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

```text
error: -u.buck.sw.current must always be at most u.buck.max_output_current, but 6.402uA..3A is not always below 2A (the currents on net `net-stage.inductor.a`: stage.buck.sw.current (6.402uA..3A) + u.buck.sw.current (-3A..-6.402uA) add up to zero; net `net-stage.inductor.a` in `stage`: -stage.buck.sw.current (6.402uA..3A) + stage.inductor.current (6.402uA..3A) add up to zero; net `1V8` in `stage`: -stage.inductor.current (6.402uA..3A) + -stage.vout.current (-3A..-6.402uA) add up to zero, but the balance is incomplete: `stage.divider.rtop.a` is not modelled; net `1V8`: output.current (0A..3A) + stage.vout.current (-3A..-6.402uA) add up to zero)
    --> std/parts/regulator.nsm:136:5
    |
136 |     -.sw.current <= .max_output_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: -stage.vout.current must always be at most stage.buck.max_output_current, but 6.402uA..3A is not always below 2A (the currents on net `1V8`: output.current (0A..3A) + stage.vout.current (-3A..-6.402uA) add up to zero; net `1V8` in `stage`: -stage.inductor.current (6.402uA..3A) + -stage.vout.current (-3A..-6.402uA) add up to zero, but the balance is incomplete: `stage.divider.rtop.a` is not modelled; net `net-stage.inductor.a` in `stage`: -stage.buck.sw.current (6.402uA..3A) + stage.inductor.current (6.402uA..3A) add up to zero; net `net-stage.inductor.a`: stage.buck.sw.current (6.402uA..3A) + u.buck.sw.current (-3A..-6.402uA) add up to zero)
    --> std/parts/regulator.nsm:384:5
    |
384 |     -.vout.current <= .buck.max_output_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

An IC cannot switch for less than a minimum on-time, so at a frequency the output cannot be made from an input that is too high, and a minimum off-time sets how close the input may come to the output (ADR 0093, provisional). `Buck` has both as `optional` members, `.min_on_time` and `.min_off_time` (ADR 0074), which a part states from its datasheet at the largest figure given. The stage has the two times it asks of the IC, `.on_time = vout / (vin * fsw)` and `.off_time = (vin - vout) / (vin * fsw)`, in the worst case the ranges give (the lowest output at the highest input and the highest frequency), and checks them against the IC's. An IC that states neither has no such check and the build says nothing. The checks are on the ideal duty cycle, so they are never looser than the real one, which is a little larger because of the switch drops; an IC with an equation of its own (the `Lt8641`'s Equations 4 and 5) writes that instead. Past either limit a real IC still regulates, at a lower frequency, so the ripple that the stage computes at the programmed one is not the board's. An invented IC for 4.5 V to 60 V that needs 150 ns of on-time, on a board that makes 5 V from an input that reaches 35 V at 1 MHz:

```nsm module=src::zbk7
import std::parts::regulator::Buck
import std::sources::Rail

/// A 2 A buck converter for 4.5 V to 60 V that needs 150 ns of on-time. Illustrative; not a real part.
part Zbk7 {
    .vin: Rail
    .buck: Buck {
        .vref &= 0.8V +/- 1.5%
        .fsw &= 1MHz +/- 10%
        .min_input_voltage &= 4.5V
        .max_input_voltage &= 60V
        .max_output_current &= 2A
        .efficiency &= 0.85..0.95
        .min_inductance &= 1.5uH
        .min_on_time &= 150ns
        .min_input_capacitance &= 2uF
        .min_output_capacitance &= 10uF
    }
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)
    dc switching_regulator .vin .buck.sw .buck.efficiency

    pins {
        //  pin          kind    name   pads
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .buck.sw.pos   power   "SW"   "4" "5"
        .buck.fb       input   "FB"   "6"
        .en            input   "EN"   "7"
    }
}
```

```nsm fails
import src::zbk7::Zbk7
import std::parts::regulator::BuckStage
import std::sources::Rail

board HighInput {
    .input: Rail {
        .voltage &= 12V..35V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk7
    .stage: BuckStage { .vout.voltage &= 5V +/- 6% }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

```text
error: stage.on_time must always be at least stage.buck.min_on_time, but 122ns..490.8ns is not always above 150ns
    --> std/parts/regulator.nsm:394:5
    |
394 |     .on_time >= .buck.min_on_time
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The input of a 12 V board that stays under 16 V holds it (4.7 V over 16 V at 1.1 MHz is 267 ns); a datasheet that gives no figure leaves the check out. A linear regulator's dropout is checked the same way by `LinearRegulator` and by a part that states `.vin.voltage >= .vout.voltage + .dropout` (the section on `LinearRegulator`).

The ripple of the output voltage is a closed-form check, not a simulation (`docs/research/closed-form-checks.md`, check 3; the stage has no time in it, only the charge balance of a capacitor over one period). `BuckStage` has `.output_ripple`, the sum of the capacitor's term and the ESR's, `dI / (8 * fsw * n * C) + dI * ESR / n`, with `dI` the inductor's ripple current above, `n` the output bank's count (`.output_count`), `C` one capacitor's effective capacitance and `ESR` the stage's `.output_esr`. A board bounds it (`.output_ripple <= 20mV`) and the bound holds in the worst case of every range: the highest input, the lowest inductance, frequency and capacitance (each with its tolerance, the capacitance at its DC bias) and the highest ESR. Each term moves one way with each of them, so the range is exact. The ESR is the board's own figure, since the standard capacitor catalog has no ESR column (a column needs the maker's number with its conditions as the recorded source, `data/jlcpcb/README.md`): `.output_esr` is `optional` (ADR 0074), and the board states the datasheet's maximum for the capacitor it means to fit, at the switching frequency, the bias and the temperature of the board; a range gives the worst end. The solver picks the capacitor without checking its own ESR against the figure. The member and where the board states it (on the stage, or on the capacitor as a catalog column) are provisional, the owner's to confirm.

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board Quiet {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk2
    .stage: BuckStage {
        .vout.voltage &= 1.8V +/- 3%
        .output_esr &= 5mohm
        .output_ripple <= 20mV
    }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

The check assumes the converter switches at its programmed frequency every period (the on-time and off-time checks above hold it), that the load draws a steady current so the capacitors take only the ripple, that the bank's capacitors share the ripple current equally, and that the ESR is the one stated. It is wrong, and optimistic, where the converter skips pulses or runs in burst mode at light load (the ripple is larger and the frequency lower, as an LT8641 does in Burst Mode), where the capacitors' inductance puts spikes on the output at the switch edges (there is none in the formula), for a constant-on-time or hysteretic converter, whose ripple the ESR sets by design, and for a load that steps or a loop that rings. Adding the two terms is the conservative sum: in a ceramic capacitor they are 90 degrees apart. A limit the worst case exceeds is refused through the capacitor search, because the capacitor's capacitance is in the ripple: the message names the closest candidate and the first limit it breaks. Here is a limit no part can meet, 0.5 mV with one capacitor of 100 mohm (an electrolytic's, say): the ESR's drop alone is 100 mohm times the ripple current, 67 mV at the worst. The closest candidate is named for the limit it misses by least, which here is the capacitance the IC asks for; when the ripple is the limit that is nearest, the message is `stage.output_ripple must always be at most 9mV, but 2.558mV..9.536mV is not always below 9mV` (a board of the tests of `crates/netsmith-solve/tests/buck_ripple.rs`, whose numbers are worked out by hand there).

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board TooQuiet {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk2
    .stage: BuckStage {
        .vout.voltage &= 1.8V +/- 3%
        .output_count &= 1
        .output_esr &= 100mohm
        .output_ripple <= 0.5mV
    }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

```text
error: no `capacitor` candidate satisfies `stage.output_cap`: the closest of the 34 that fit, 1 x 10uF 20% 10V X5R 0402 C315248, fails because (stage.output_cap.count * stage.output_cap.effective_capacitance) must always be at least stage.buck.min_output_capacitance, but 4.908uF..7.686uF is not always above 10uF (`stage.output_cap.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
    --> std/parts/regulator.nsm:401:5
    |
401 |     .output_cap.count * .output_cap.effective_capacitance >= .buck.min_output_capacitance
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A board that states a limit and no ESR is not judged: the relation that reads `.output_esr` is skipped (ADR 0074), so `.output_ripple` has no value and the limit is listed as not evaluated, never passed. The capacitor's term alone would pass a limit that the ESR's would break, so it is not used. A board that states neither has no ripple check and the build says nothing about it.

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::BuckStage
import std::sources::Rail

board NoEsr {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .u: Zbk2
    .stage: BuckStage {
        .vout.voltage &= 1.8V +/- 3%
        .output_ripple <= 20mV
    }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}
```

A frequency set by a resistor is a relation the IC writes. Here `rt_value` is a field of the IC and the board states the resistor against it; the stage then uses the frequency that follows:

```nsm-fragment
part Zbk3 {
    .rt_value: Resistance
    .buck.fsw == 24GHz * 1ohm / .rt_value
    pins {
        .rt passive "RT" "8"
    }
}
```

```nsm-fragment
.rt: Resistor { .resistance &= 20kohm +/- 1% }
.u.rt ~ .rt.a
.rt.b ~ .input.neg
.u.rt_value == .rt.resistance
```

What is not modelled: the divider's current is not added to the load, and the IC's quiescent current is not in the inductor's (it is in the IC's own input current, if the IC states it, as the `Lt8641` does), the loop and its stability are not checked (the IC's capacitor and inductor ranges stand for them), the inductor's saturation is its datasheet current at a fall of the inductance (30% for the catalog's parts) and not a curve, and the ripple current of the capacitors is not modelled, so it is not shared out between the capacitors of a bank.

The library has one real buck IC, `std::parts::regulator::Lt8641`, Analog Devices' 3 V to 65 V, 3.5 A LT8641 in its 18-lead 3 x 4 mm QFN (datasheet Rev. C). It is written as above, with each number's datasheet page and table in its doc comment and in `data/jlcpcb/README.md`: the reference 0.79 V to 0.822 V, the input range, 3.5 A, the efficiency, `min_inductance` from the datasheet's Equation 10, a DCR of at most 0.04 ohm, 2.2 uF at the input, and 47 uF at the output by default (every Typical Application of the datasheet uses it; the datasheet states no minimum, so a board replaces it with `.u.buck.min_output_capacitance &= 22uF`). The switching frequency follows the resistor on RT, `.fsw == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)` with the oscillator's accuracy (Equation 3 and Table 1); the part refuses a resistor outside 18.2 kohm to 60.4 kohm (2 MHz to 700 kHz), the span the datasheet gives the accuracy for. It also refuses an input and output at which the datasheet's Equations 4 and 5 say the frequency is no longer held, because the stage computes its ripple at the programmed frequency. The automotive ordering code `LT8641IUDC#WPBF` is the same die and tables as the other grades, so the board names its code with `.mpn`, as for any IC. The standard `lt8641` catalog has a header and no row, like `p_mosfet` (a row needs a stock above zero on a record that was read), so the board's project has the row, in its own file of that name (the part number is the datasheet's Order Information; a project adds the LCSC number it has read):

```csv file=catalogs/lt8641.csv set=lt8641
mpn,package,library,source
LT8641IUDC#WPBF,UDC20(18),Extended,"datasheet Rev. C, Order Information p.2 (automotive products)"
```

The datasheet asks for a DC resistance of 0.04 ohm at most (p.16), and the four inductors of the standard catalog have more, so a stage around the IC needs an inductor of the project's own. This one is invented, like the part numbers of the other files of this guide:

```csv file=catalogs/inductor.csv set=lt8641
mpn,name,inductance,saturation_current,dc_resistance,package,library,source
ZL-3R3,ZL-3R3 3.3uH 4.5A inductor 1210,3.3uH +/- 20%,4.5A,0.03ohm,1210,Extended,https://example.invalid/zl-3r3
```

The board around it: 9 V to 16 V in with a 36 V peak, 3.3 V out at up to 2 A, the stage, and what the datasheet asks for on the pins that the stage does not hold: the resistor on RT (47 kohm is 0.89 MHz, Equation 3), the boost capacitor from BST to SW, the INTVCC capacitor, EN/UV to the input, BIAS to the output, and SYNC/MODE low (Burst Mode).

```nsm evaluate catalogs=lt8641
import std::parts::passive::{Capacitor, Resistor}
import std::parts::regulator::{BuckStage, Lt8641}
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A..2A
    }
    .u: Lt8641 { .mpn &= "LT8641IUDC#WPBF" }
    .stage: BuckStage { .vout.voltage &= 3.3V +/- 4% }
    .rt: Resistor { .resistance &= 47kohm +/- 1% }
    .boost: Capacitor { .capacitance &= 100nF +/- 20% }
    .intvcc_cap: Capacitor { .capacitance &= 1uF +/- 20% }

    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en_uv ~ .input.pos
    .u.bias ~ .output.pos
    .u.bias_voltage == .output.voltage
    .u.sync_mode ~ .input.neg
    .u.rt ~> .rt ~> .input.neg
    .u.rt_resistance == .rt.resistance
    .u.bst ~> .boost ~> .u.buck.sw.pos
    .u.intvcc ~> .intvcc_cap ~> .input.neg
}
```

The part states the absolute maximum of three pins besides VIN and BIAS, from its Absolute Maximum Ratings (p.2): `EN/UV` 65 V, `TR/SS` 4 V and `SYNC/MODE` 6 V, each as a `Logic` (`.tr_ss_limit` and the like) whose `.line` is the pin inside the part, so the solver compares the voltage of the pin's net with the limit as it does for any `Logic` input (ADR 0046, ADR 0095, provisional). A board that ties `SYNC/MODE` or `TR/SS` to a 9 V to 16 V input is refused with the net, the limit and the field that states it (`net `VIN` reaches 16V above its ground `GND` ... but `u.sync_mode_limit.line.abs_max` is 6V`). A pin left to float is not judged, and one with only a soft-start capacitor on it is a `not evaluated` note, since nothing sets its voltage. `PG` (42 V) and `FB` (4 V) have limits too and state none, for the reasons in the part's doc comment and ADR 0095.

The IC draws current from its input for itself with nothing loaded, and the input current of the part is the load's and that (ADR 0096, provisional): `.quiescent_current`, a range from zero to the largest figure of the mode the converter runs in, which the DC analysis draws at the input too (`dc switching_regulator ... .quiescent_current`), so a sleep-current budget summed from `.u.vin.current` is judged against it. The datasheet gives 10 uA over temperature in Burst Mode, with `SYNC/MODE` tied to ground (`.iq_max`, Electrical Characteristics, p.2), and "several hundred uA" with it floating, high or clocked (0.5 mA at most at 25 C, `.iq_skipping_max`). The part cannot see how the board ties the pin, so the default is the larger, and a board that grounds `SYNC/MODE` says so: `.u.quiescent_current &= 0A .. .u.iq_max`. The 10 uA is a maximum and not a minimum, so the least the input draws is still zero, and a board cannot ask for `.u.vin.current >= 10uA`. The stage's feedback divider is a load on the output and is not in this current (`.divider_current` has it).

The datasheet's other pins and capacitors (the two 1 uF capacitors at VIN1 and VIN2, the feedback phase-lead capacitor, the enable divider) are the board's. The package has no footprint or symbol in KiCad's libraries, so `std/kicad.toml` maps `Lt8641` in the package `UDC20(18)` to netsmith's own footprint, `netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA` (the _Standard libraries of footprints_ paragraph below says what that is), and names no symbol, so the symbol is drawn from the pin table: a board built around it exports with no KiCad install and no `kicad.toml`. The footprint's pads are the datasheet's recommended solder pad pitch and dimensions (p.24); a project that wants another land pattern maps the part and package to its own, as for any other part (`std/kicad.toml` and a project's `kicad.toml` may not both map one pair, so the project uses its own package name in its row).

**Standard libraries of footprints** (ADR 0087, provisional). A package that KiCad's libraries lack and the generators of ADR 0043 do not make is drawn in `std/libs/`: footprints as `.kicad_mod` files in `std/libs/netsmith.pretty/` and symbols in `std/libs/netsmith.kicad_sym`, in KiCad's own formats, compiled into the binary. They are the library `netsmith`, named in a row of `std/kicad.toml` like any other (`footprint = "netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA"`), found before any KiCad install is read, and the same on every machine. A project cannot list a library called `netsmith` (`kicad.toml: library `netsmith` is the name of the standard library's own footprints and symbols ...`). When a board uses one of them, `build`, `fab` and the editor write the library into the board's output folder (`netsmith.pretty/`, `netsmith.kicad_sym`) and list it in the `fp-lib-table` and `sym-lib-table` beside the `.kicad_pro`, as they list a project's own libraries, so KiCad opens the board with the footprint in its library table. Every footprint of the folder is named by a row of `std/kicad.toml`, and a test checks each against the pin table of its part (every pad of the table is a pad of the footprint and a pin of the symbol, and the other way round). A symbol of that library is one netsmith drew: a row of `std/kicad.toml` (or of a project's `kicad.toml`) that has no `symbol` names none, and a part with a pin table is then drawn as a plain box, one box for the part with its pins on the sides (supply above, ground below, inputs left, outputs right), each pin with its datasheet name, its pad number and the electrical type of its kind. A pin table says `power` and not whether the pin takes power from its net or gives it, because that is the sign of the current of the rail on the pin, which the board's loads decide: the design stage reads it from the solved model (a pin that is the `pos` of a rail whose current is negative, from below 0 A to at most 0 A, gives power; one whose current is drawn, from at least 0 A to above 0 A, takes it; and a pin that the part joins to such a pin has its direction). A pin that gives power, such as the switch node of a converter, is a power output on the right of the box, and the others, including a power pin whose direction is not known (a rail whose current has no value or may take either sign), are power inputs above it, so that a net with no output gets a power flag. The symbol is named for the part and its package (`netsmith:Lt8641_UDC20_18`), with `_out` and the first pad of each pin that gives power when there is one (`netsmith:Lt8641_UDC20_18_out8`), so that two parts of one kind that differ in direction are two symbols, and written into the board's `netsmith.kicad_sym` with the rest of the library. A row that does name a `symbol` is drawn with it, and the standard rows of the passives, diodes, FETs and connectors still do (ADR 0097, step 1).

A footprint is added only from a package drawing that was read: each pad's size and place is a dimension of the manufacturer's land pattern, `std/libs/README.md` says which were read from the drawing and which were assumed, and a package with a dimension that cannot be read is not added.

### Current through a part

A `Fuse`, `Inductor`, `FerriteBead`, `Switch`, `Diode`, `Schottky`, `NMosfet` and `PMosfet` have a `bridge` and a `.current`, the current through it, and state that it is at most the part's rating: `.rated_current`, `.saturation_current`, `.max_forward_current` or `.max_drain_current`. In a `~>` chain from a rail the current is the draw of what is behind the chain (ADR 0014), so a fuse is checked against the budget of its load with no line of its own. An inductor outside a chain gets its current from the design: state the largest current through it, the peak and not the average.

```nsm fails
import std::parts::passive::Fuse
import std::sources::Rail

board Budget {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 100mA..1kA
        .pos.net &= "VFUSED"
        .neg.net &= "GND"
    }
    .fuse: Fuse {}
    .input.pos ~> .fuse ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no `fuse` candidate satisfies `fuse`: the closest of the 8 that fit, 1206HV-3A fuse 3A 125V 1206 C21261120, fails because fuse.current must always be at most fuse.rated_current, but 100mA..1kA is not always below 3A (the currents on net `VFUSED`: -fuse.current (100mA..1kA) + load.current (100mA..1kA) add up to zero; net `VIN`: fuse.current (100mA..1kA) + input.current (-1kA..-100mA) add up to zero)
    --> std/parts/passive.nsm:287:5
    |
287 |     .current <= .rated_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Inductor
import std::sources::Rail

board Saturates {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .l: Inductor {
        .inductance &= 2.2uH +/- 30%
        .current &= 500A
    }
    .supply.pos ~ .l.a
    .supply.neg ~ .l.b
}
```

```text
error: no `inductor` candidate satisfies `l`: the closest of the 1 that fit, MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758, fails because l.current must always be at most l.saturation_current, but 500A is not always below 3.6A
    --> std/parts/passive.nsm:171:5
    |
171 |     .current <= .saturation_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### What a part in a chain sees of its rail

A part reads its own fields, and the checker derives values for it from a rail and from the chain it is in: `.bias` for a part across the rail (ADR 0013) and, for a part in a `~>` chain from a rail, `.supply_peak`, the `.peak` of the rail the chain starts on (ADR 0086, provisional). A `Fuse` has it, so it is checked for the highest voltage it has to interrupt, `.supply_peak <= .rated_voltage`, with no line on the board, and the catalog picks a fuse rated for it. The rail is the one whose `pos` is on the net the chain starts on; where the scope has several, the rails it declares itself come before the rails of what it holds, and the first by path stands. Every part of the chain gets the start's peak, the ones after other parts too: a part before it only drops voltage, and the drop is not subtracted. A chain that starts on no rail's `pos` (a signal, a rail's `neg`) gives its parts nothing, and a fuse in no chain from a rail has no `.supply_peak`: the board states it (`.fuse.supply_peak &= 24V`) or the check is listed as not evaluated, as is a check on a rail that states no `.peak`. A value the board states for a fuse in a chain is checked against the rail's.

Two more values reach a part through the chain (ADR 0091, provisional), both for a diode behind a fuse:

- **The fault current.** A `Fuse` has `.limit_current`, the most it lets through (its `.rated_current`, conservatively: a real fuse carries more for a time before it opens). A `Diode` or `Schottky` has `.supply_fault_current`, which the checker gives it as the `.limit_current` of the nearest part in front of it in its chain that has one, and states `.supply_fault_current <= .max_forward_current`. So a diode behind a fuse is rated for what the fuse lets through, and not only for the load's draw (which its `.current` is, and which is checked as before). A diode with no fuse in front of it has none, and the check is left out. The chain needs no rail for it.
- **The reverse peak.** A `Rail` has `.reverse_peak`, the highest voltage it reaches the wrong way round, as a positive number, which a design states for its input (`.input.reverse_peak &= 100V`). The first part of a chain from the rail that has `.supply_reverse_peak` is given it: a `Diode` or `Schottky` states `.supply_reverse_peak <= .max_reverse_voltage` (a reverse-polarity diode blocks it), and a unidirectional `Tvs` states `-.supply_reverse_peak >= 0V` (it conducts when it is reversed). The parts behind that part are given nothing: a TVS diode behind a reverse-polarity diode is not found to conduct a transient that the diode stops. `.reverse_peak` is `optional` (ADR 0074): a rail that states none checks nothing and lists nothing, and a diode is as it was.

The rest is the board's, because only these values reach a part:

- The voltage a diode blocks in series with a load when it is not the supply's reverse peak: a board states `.d.bias`, which is checked against `.max_reverse_voltage`. A diode in series is across no rail, so its `.bias` is unset until then, and the rating is listed as not evaluated. The reverse voltage across a reverse-polarity diode is the reverse peak plus what the rail behind it holds, and the second is the board's (`.d.max_reverse_voltage >= .input.reverse_peak + .load.peak`).
- A `Switch`'s `.rated_voltage` and a `Tvs`'s `.protected_limit` are compared with nothing the chain gives them: the board writes `.sw.rated_voltage >= .input.peak` or `.tvs.clamping_voltage <= .reg.vin_abs_max`.

A rail with the peak stated and a fuse in a chain from it is checked with no further line:

```nsm evaluate
import std::parts::passive::Fuse
import std::sources::Rail

board Interrupts {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 5.5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VFUSED"
        .neg.net &= "GND"
    }
    .fuse: Fuse {}
    .input.pos ~> .fuse ~> .load.pos
    .input.neg ~ .load.neg
}
```

The library's 6 V fuses interrupt that peak. A peak far above what any fuse in the library interrupts is refused, with no line of the board's:

```nsm fails
import std::parts::passive::Fuse
import std::sources::Rail

board Arcs {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 5kV
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VFUSED"
        .neg.net &= "GND"
    }
    .fuse: Fuse {}
    .input.pos ~> .fuse ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no `fuse` candidate satisfies `fuse`: the closest of the 8 that fit, 1206HV-3A fuse 3A 125V 1206 C21261120, fails because fuse.supply_peak must always be at most fuse.rated_voltage, but 5kV is not always below 125V
    --> std/parts/passive.nsm:288:5
    |
288 |     .supply_peak <= .rated_voltage
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A diode behind a fuse is rated for the fuse's rating, and a reverse-polarity diode for the rail's reverse peak, with no line of the board's:

```nsm evaluate
import std::parts::diode::Schottky
import std::parts::passive::Fuse
import std::sources::Rail

board Protected {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 5.5V
        .reverse_peak &= 60V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VPROTECTED"
        .neg.net &= "GND"
    }
    .fuse: Fuse { .rated_current &= 1A }
    .d: Schottky { .max_reverse_voltage &= 100V }
    .input.pos ~> .fuse ~> .d ~> .load.pos
    .input.neg ~ .load.neg
}
```

A diode that the fuse can overload is refused:

```nsm fails
import std::parts::diode::Schottky
import std::parts::passive::Fuse
import std::sources::Rail

board Overloaded {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 5.5V
        .reverse_peak &= 60V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VPROTECTED"
        .neg.net &= "GND"
    }
    .fuse: Fuse { .rated_current &= 3A }
    .d: Schottky { .max_forward_current &= 1A }
    .input.pos ~> .fuse ~> .d ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no combination of candidates satisfies `d` and `fuse` together: the closest, d = 1N5819WS 40V 1A Schottky SOD-323 C191023, fails because d.supply_fault_current must always be at most d.max_forward_current, but 3A is not always below 1A
    --> std/parts/diode.nsm:142:5
    |
142 |     .supply_fault_current <= .max_forward_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: d.supply_fault_current must always be at most d.max_forward_current, but 3A is not always below 1A
    --> std/parts/diode.nsm:142:5
    |
142 |     .supply_fault_current <= .max_forward_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

`Crystal` has `.frequency` (state it with its tolerance, `16MHz +/- 20ppm`) and `.load_capacitance`, and no relation: the two load capacitors are the design's. `FerriteBead` has `.impedance` at `.impedance_frequency`.

### Transistors

`NMosfet` and `PMosfet` have the pins `gate` (an input, which must be connected), `source` and `drain`. All their voltages and currents are magnitudes. The design states the stress it puts on the part: `.drain_source_voltage`, the voltage the part blocks while off, and `.gate_source_voltage`, the voltage the gate is driven to while on. They are checked against the maxima, and the gate drive against the threshold, which is a range: the drive must be above the highest threshold.

The on-resistance of a part is the datasheet's figure at the gate-source voltage its table names, and a smaller drive gives a larger resistance. `.on_resistance_vgs` is that voltage (ADR 0092, provisional) and the drive must always be at least it, so a part picked from a catalog is one whose guarantee holds at the drive the design gives. The 2N7002's 7 ohm is guaranteed at 5 V and the AO3400A's 48 mohm at 2.5 V, so a 3.3 V logic rail is given the AO3400A. A row that records no test voltage is offered all the same, and the check is not evaluated for it; a design that wants the check states a requirement on the member (`.q.on_resistance_vgs <= 3V`), and is then offered only the rows that record a voltage.

```nsm fails
import std::parts::transistor::NMosfet
import std::sources::Rail

board Switch {
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .logic: Rail {
        .voltage &= 0.3V +/- 5%
        .pos.net &= "V0V3"
        .neg.net &= "GND"
    }
    .q: NMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= logic.voltage
    }
    .q.gate ~ .logic.pos
    .q.source ~ .supply.neg
    .q.drain ~ .supply.pos
}
```

```text
error: no `n_mosfet` candidate satisfies `q`: the closest of the 2 that fit, AO3400A 30V 5.7A N-MOSFET SOT-23 C20917, fails because q.gate_source_voltage must always be at least q.gate_threshold, but 285mV..315mV is not always above 650mV..1.45V
   --> std/parts/transistor.nsm:77:5
   |
77 |     .gate_source_voltage >= .gate_threshold
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A logic rail of 3.3 V is below the 5 V the 2N7002's figure holds at. A design that states the voltage it needs the figure at gets the refusal with both numbers (here with an invented 5 V; without the line, the catalog is read and a part guaranteed at 2.5 V is picked instead):

```nsm fails
import std::parts::transistor::NMosfet
import std::sources::Rail

board Switch {
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "V3V3"
        .neg.net &= "GND"
    }
    .q: NMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= logic.voltage
        .on_resistance_vgs &= 5V
    }
    .q.gate ~ .logic.pos
    .q.source ~ .supply.neg
    .q.drain ~ .supply.pos
}
```

```text
error: no `n_mosfet` candidate satisfies `q`: the closest of the 1 that fit, 2N7002 60V 115mA N-MOSFET SOT-23 C8545, fails because q.gate_source_voltage must always be at least q.on_resistance_vgs, but 3.135V..3.465V is not always above 5V
   --> std/parts/transistor.nsm:78:5
   |
78 |     .gate_source_voltage >= .on_resistance_vgs
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: q.gate_source_voltage must always be at least q.on_resistance_vgs, but 3.135V..3.465V is not always above 5V
   --> std/parts/transistor.nsm:78:5
   |
78 |     .gate_source_voltage >= .on_resistance_vgs
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

`PMosfet` is a generic like `NMosfet`, with the catalog `p_mosfet` (the same columns as `n_mosfet`), and its standard catalog has no rows yet, because no P-channel part has been entered with a recorded source (`data/jlcpcb/README.md`). A project that needs one writes the rows of its parts in a file named `p_mosfet.csv` in `catalogs/`; the rows are added after the standard ones, as for any standard catalog. Without a row, a `PMosfet` is refused with the message of an empty catalog. This is a high-side switch with an invented part: the source of a P-channel MOSFET is the supply, its gate is pulled to ground to turn it on, and `.gate_source_voltage` is the magnitude of the drive.

```csv file=catalogs/p_mosfet.csv set=pfet
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,package,library,source
ZP-30,ZP-30 30V 4A P-MOSFET,30V,4A,12V,0.4V..1.3V,0.07ohm,SOT-23,Extended,https://example.invalid/zp-30
```

```nsm evaluate catalogs=pfet
import std::parts::passive::Resistor
import std::parts::transistor::PMosfet
import std::sources::Rail

board HighSide {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "V5"
        .neg.net &= "GND"
    }
    .q: PMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= supply.voltage
        .current &= 0A..1A
    }
    .load: Resistor { .resistance &= 1kohm +/- 1% }
    .q.gate ~ .supply.neg
    .q.source ~ .supply.pos
    .q.drain ~ .load.a
    .load.b ~ .supply.neg
}
```

The switch's drain goes to a load. Joined to the ground instead, a switch that is on is a short across the supply, and the DC analysis says so (`q.current` is stated up to 1 A, and the circuit puts 5.25 V across 70 mohm, 75 A): a MOSFET is solved as a switch (see "DC analysis").

The stresses are checked against the maxima as they are for an `NMosfet`. A 100 V supply puts 100 V across the drain and the source and between the gate and the source, which is more than this part takes, and the design is refused with the numbers:

```nsm fails catalogs=pfet
import std::parts::transistor::PMosfet
import std::sources::Rail

board HighSide {
    .supply: Rail {
        .voltage &= 100V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .q: PMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= supply.voltage
        .current &= 1A
    }
    .q.gate ~ .supply.neg
    .q.source ~ .supply.pos
    .q.drain ~ .supply.neg
}
```

```text
error: no `p_mosfet` candidate satisfies `q`: the closest of the 1 that fit, ZP-30 30V 4A P-MOSFET, fails because q.drain_source_voltage must always be at most q.max_drain_source_voltage, but 95V..105V is not always below 30V
    --> std/parts/transistor.nsm:118:5
    |
118 |     .drain_source_voltage <= .max_drain_source_voltage
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### What a part is at DC

A part says what it is to a DC analysis with a `dc` statement, once, in the part or in the generic that it provides (ADR 0076, amendment 1, provisional). The statement names the element, then its pins, then its values, in the order the element fixes:

```nsm-fragment
dc resistor .a .b .resistance
dc diode .anode .cathode .forward_voltage
dc nmos .gate .drain .source .gate_threshold .on_resistance
dc pmos .gate .drain .source .gate_threshold .on_resistance
dc linear_regulator .vin .vout .quiescent_current
dc switching_regulator .vin .buck.sw .buck.efficiency
dc source .vbatt
```

- `dc resistor <terminal> <terminal> <resistance>`: a resistance between two pins, over the range of the field (a resistor, a fuse, a ferrite bead's `.dc_resistance`).
- `dc diode <anode> <cathode> <forward voltage>`: blocks, or holds the forward voltage and carries a current of at least zero.
- `dc nmos` and `dc pmos`, `<gate> <drain> <source> <gate threshold> <on resistance>`: a MOSFET as a switch, `.on_resistance` between drain and source when the gate drive is above the threshold and open when it is below. The gate draws nothing, and the values are magnitudes as in `NMosfet` and `PMosfet`.
- `dc linear_regulator <input rail> <output rail> [<quiescent current>]` and `dc switching_regulator <input rail> <output rail> <efficiency> [<quiescent current>]` (ADR 0076, amendment 3, provisional; the optional operand, ADR 0096, provisional): a regulator. The first two operands are the part's rails (a rail inside an interface is named through it: `.buck.sw`), not pins. The output rail is an ideal source over the range of its `.voltage`; the input rail draws what the output asks. A linear regulator draws what it supplies and its quiescent current, a range of currents that is not negative, taken at both ends (amendment 9, provisional): the third operand is optional, a field of the part that is a current, and a linear regulator that leaves it out draws what it supplies alone. A switching one draws `Pout / (efficiency * Vin)`, with the efficiency a number (a range, `0.85..0.95`, is taken at both ends); an efficiency has to be above 0 and at most 1 (ADR 0076, amendment 14, provisional), and one that is not (`85..95` for `85%..95%`, a converter that makes power) is an error that names the part and the value, and the regulator is not modelled. A switching regulator draws its quiescent current too when its statement names one, as a fourth operand that is optional in the same way (ADR 0096, provisional); with the output unloaded it still draws that current. A quiescent current that the part names and leaves with no value (or a negative one) is a number nothing can model with: the regulator is not modelled and the island in front of it is left alone, as for a regulator that states no `dc`.
- `dc source <supply rail>` (ADR 0076, amendment 5, provisional): the part gives this rail from outside the board, as a connector gives the supply of a vehicle. The operand is a rail of the part. Its `pos` net is held over the range of the rail's `.voltage` that the design states, and its `neg` net at zero, as a `Rail` that the board declares itself is (see "Supplies" below). A part with a rail it takes its supply from (what a load is) does not state it.

The standard `Resistor`, `Fuse`, `FerriteBead`, `Inductor` (its winding: `dc resistor .a .b .dc_resistance`, ADR 0076, amendment 6, provisional), `Diode`, `Schottky`, `NMosfet`, `PMosfet`, `LinearRegulator`, `Lt8641Buck`, and the two connectors that give a supply, `Obd2M` (`.vbatt`) and `UsbCPower` (`.vbus`), state theirs (any other buck converter's IC is a part of the design's own, and states `dc switching_regulator` itself, as the one in "Switching regulators" does), so a provider of one of them has it on its own pins and fields without writing it. A part with a `bridge` and no `dc` statement is a boundary: a DC analysis cannot see through it. An inductor is its winding at DC, so the node past a buck stage's inductor is the switch node less what the load makes the winding drop, and the output side of the stage is an island of its own. `Led`, `Switch`, `Zener` and `Tvs` are boundaries for now. A module has no `dc` statement of its own: its network is the elements of the parts inside it.

The rules. A `dc` statement is in a part (or a generic that has pins); the element is one of `resistor`, `diode`, `nmos`, `pmos`, `linear_regulator`, `switching_regulator` and `source`; the pins are pins of the part (the rails of a regulator or a source are rails of the part) and are different; each value is a field of the part with the quantity of its place; a part has one `dc` statement; and when the part has a `bridge`, the element runs between the two pins the `bridge` joins (the drain and the source for a MOSFET). The order of the statements does not matter.

The resistor, the diode, the regulators and the MOSFET are solved by the DC analysis (next section).

A part of your own that is a resistor at DC, and a provider of the standard fuse, which has the fuse's statement without writing one:

```nsm
import std::parts::passive::Fuse

/// Invented: a current-sense resistor.
part Shunt {
    .designator &= "R"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// Invented: a fuse from a project's own parts. `dc resistor .a .b .resistance` comes from `Fuse`.
part SlowBlow provides Fuse {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Protected {
    .shunt: Shunt
    .fuse: SlowBlow
}
```

A regulator's own current is a third operand of `dc linear_regulator` and a fourth of `dc switching_regulator`, and the standard `LinearRegulator` and `Lt8641` name it. A provider of it narrows `.quiescent_current` from its datasheet, and the DC analysis then draws it at the regulator's input besides the load (ADR 0076, amendment 9, provisional): the island behind the regulator is solved, and the stage in front of it is asked for the load and this current.

```nsm evaluate
import std::parts::regulator::LinearRegulator
import std::sources::Rail

/// Invented: a 3.3 V regulator that keeps up to 140 uA for itself with the output unloaded.
part SmallLdo provides LinearRegulator {
    pins {
        .vin.pos  power "IN"  "3"
        .vin.neg  power "GND" "1"
        .vout.pos power "OUT" "2"
    }
    .vout.voltage &= 3.3V +/- 3%
    .dropout &= 0.3V
    .max_current &= 0.5A
    .max_power &= 1W
    .quiescent_current &= 0A..140uA
}

board Island {
    .supply: Rail {
        .voltage &= 4.5V..5.5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .reg: SmallLdo
    .supply.pos ~ .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .load: Rail {
        .current &= 20mA..50mA
        .pos.net &= "V33"
        .neg.net &= "GND"
    }
    .reg.vout ~ .load
}
```

The statement is checked, and each refusal names the element or the operand. An element that is not one of the four lists them:

```nsm fails
part Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .forward_voltage: Voltage
    dc coil .a .b .resistance
}

board B {
    .r: Shunt
}
```

```text
error: `dc` does not know the element `coil`; the elements are resistor, diode, nmos, pmos, linear_regulator, switching_regulator and source
  --> main.nsm:8:8
  |
8 |     dc coil .a .b .resistance
  |        ^^^^
```

The wrong number of operands shows the whole statement:

```nsm fails
part Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .forward_voltage: Voltage
    dc resistor .a .resistance
}

board B {
    .r: Shunt
}
```

```text
error: `dc resistor` takes 2 pins and then 1 value, as in `dc resistor .a .b .resistance`; this has 2 operands
  --> main.nsm:8:5
  |
8 |     dc resistor .a .resistance
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A value must be a field of the quantity of its place; here a resistor is given the forward voltage:

```nsm fails
part Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .forward_voltage: Voltage
    dc resistor .a .b .forward_voltage
}

board B {
    .r: Shunt
}
```

```text
error: `dc resistor` takes the resistance here, a resistance, and `.forward_voltage` is a voltage
  --> main.nsm:8:23
  |
8 |     dc resistor .a .b .forward_voltage
  |                       ^^^^^^^^^^^^^^^^
```

The element runs between the pins the `bridge` joins:

```nsm fails
part Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
        .c passive "C" "3"
    }
    .resistance: Resistance
    bridge .a ~> .b
    dc resistor .a .c .resistance
}

board B {
    .r: Shunt
}
```

```text
error: `dc resistor` runs between .a and .c, and this part's `bridge` joins .a and .b: a `~>` chain passes through the element the part is at DC, so the two must be the same pins
  --> main.nsm:9:5
  |
9 |     dc resistor .a .c .resistance
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A part is one element, so a second statement is refused, with the place of the first (a provider that repeats the statement of its generic gets the same message):

```nsm fails
part Shunt {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .forward_voltage: Voltage
    dc resistor .a .b .resistance
    dc diode .a .b .forward_voltage
}

board B {
    .r: Shunt
}
```

```text
error: this part already has a `dc` statement (at main.nsm:8); a part is one element at DC
  --> main.nsm:9:5
  |
9 |     dc diode .a .b .forward_voltage
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A module has no element of its own (nor has a board): its network is the elements of the parts inside it.

```nsm fails
module Filter {
    dc resistor .a .b .resistance
}

board B {
    .f: Filter
}
```

```text
error: `dc` can only appear inside a part, and this is a module: a module's DC network is the elements of the parts inside it
  --> main.nsm:2:5
  |
2 |     dc resistor .a .b .resistance
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### Capacitor voltage rating

`Capacitor` does not state a derating: a design that wants one writes it, in the capacitor, in one line that does not name the rail, because `.bias` is the rail across it. Here every capacitor on a 400 V input is rated for 1.5 times the voltage it sees:

```nsm fails
import std::parts::passive::Capacitor
import std::sources::Rail

board Derate {
    .input: Rail {
        .voltage &= 400V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .bulk: Capacitor {
        .capacitance &= 47uF +/- 20%
        .max_voltage >= .bias * 1.5
    }
    .input.pos ~> .bulk ~> .input.neg
}
```

```text
error: no `capacitor` candidate satisfies `bulk`: the closest of the 1 that fit, 47uF 20% 6.3V X5R 0805 C16780, fails because bulk.max_voltage must always be at least (bulk.bias * 1.5), but 6.3V is not always above 570V..630V
   --> main.nsm:12:9
   |
12 |         .max_voltage >= .bias * 1.5
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A capacitor across no rail has no bias to rate: state `.bias`, or place it between a rail's `pos` and `neg`.

### Connectors, switches and mechanical parts

The connectors are `Header1x2`, `Header1x4`, `Header1x8`, `Header1x10`, the sockets `Socket1x2`, `Socket1x4`, `Socket1x8`, `Socket1x10`, and the two-row headers `Header2x3`, `Header2x5` and `Header2x10`. A pin is `pN`, on pad N, and contact N of a connector is its pin `pN`: a header and its socket number so that contact N meets contact N. There is one definition for each size because the language has no parameter for a count: a part with `N` pins has `N` rows in its table. The pitch is the package (`.package &= "2.54mm"`), which the export mapping reads. Each states its `.mating` (kind, gender, pitch, rows, positions), which a system uses to mate boards (see Systems and ports). The single-row ones are rated at 3 A a contact, from the datasheets of the `header` catalog; the two-row headers have no row, are unrated, and the check of the current through a mate lists them as not evaluated.

`Coax` has `signal` and `shield`. `ProgrammingPads6` is six pads, `p1` to `p6`, for a spring-pin programming cable; the board decides what each carries. `UsbCPower` is a six-contact USB Type-C receptacle with power only: a `.vbus` rail (the receptacle supplies, so its current is negative and follows from the loads on the net), the two configuration pins `cc1` and `cc2`, and the `shell`. A device that takes power from a source needs a 5.1 kohm pull-down from each configuration pin to ground, written as a resistor in a chain. `UsbC2` is the sixteen-contact USB 2.0 receptacle: the same pins, the `Usb2` port, and the open sideband pins `sbu1` and `sbu2`. `Obd2M` is the male OBD-II (SAE J1962) vehicle connector: a `.vbatt` rail (the vehicle supplies, so its current is negative and follows from the loads on the net; `pos` is battery power on pin 16 and `neg` chassis ground on pin 4), the optional `signal_gnd` (pin 5), and the bus pins `can_high`, `can_low`, `iso_k`, `iso_l`, `j1850_bus_pos` and `j1850_bus`; the pins J1962 leaves to the manufacturer are `p1`, `p3`, `p8`, `p9`, `p11`, `p12` and `p13`. Its catalog `obd2_m` has no standard row yet: a project's own `catalogs/obd2_m.csv` lists the connector it buys.

```nsm evaluate
import std::parts::connector::UsbCPower
import std::parts::passive::Resistor
import std::sources::Rail

board UsbPowered {
    .usb: UsbCPower {
        .vbus.voltage &= 5V +/- 5%
        .vbus.pos.net &= "VBUS"
        .vbus.neg.net &= "GND"
    }
    .system: Rail { .current &= 0mA..400mA }
    .usb.vbus ~ .system
    .rd1: Resistor { .resistance &= 5.1kohm +/- 1% }
    .rd2: Resistor { .resistance &= 5.1kohm +/- 1% }
    .usb.cc1 ~> .rd1 ~> .usb.vbus.neg
    .usb.cc2 ~> .rd2 ~> .usb.vbus.neg
}
```

`MountingHole`, `PlatedHole`, `Fiducial`, `TestPoint`, `Label` and `ProgrammingPads6` are fitted by nobody (`.assembly` is `"none"`), so they are on no BOM. `MountingHole` and `Fiducial` have no pins, so they are on no net. `PlatedHole` has one pin, `p1`, which is the plating: a board joins it to a net or leaves it open. `TestPoint` has one pin, `p1`, which must be on a net.

```nsm evaluate
import std::parts::mechanical::{PlatedHole, TestPoint}
import std::parts::passive::Resistor
import std::sources::Rail

board Mechanical {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
    .screw: PlatedHole
    .screw.p1 ~ .supply.neg
    .tp: TestPoint
    .tp.p1 ~ .supply.pos
}
```

A board declares its mounting holes and fiducials in its physical file (see _The physical file_), not here: they have no circuit meaning.

```nsm-physical
import std::parts::mechanical::{Fiducial, MountingHole}

physical for Mechanical {
    case {
        .hole: MountingHole
        .fid: Fiducial
    }
}
```

### DC analysis

After the parts are picked, the solver builds the network of the parts that state what they are at DC and solves it (ADR 0076, provisional). Where the equations of the design cannot say what a part carries (a net with two currents that nothing states, ADR 0026), the network can: the pass writes `.current` and `.drop` of a part, and the checks that waited for them (`.drop == .current * .resistance`, `.drop * .current <= .max_power`, a fuse's `.current <= .rated_current`) are judged with the numbers. A pass that finds a rating exceeded is an error, like any relation, and says which states of the circuit give it.

What it models in this stage:

- **Resistors.** A `dc resistor` part (a resistor, a fuse, a ferrite bead) is its resistance over the range of the field, `10kohm +/- 1%` taken at both ends. A resistance that can be zero (a bead's `0ohm..70mohm`) is solved at zero too, as a short (ADR 0076, amendment 16, provisional), and one under 1 mohm is its own value: a 0 to 0.2 mohm link at 90 A to 100 A leaves 11.98 V to 12 V of a 12 V supply, not the 11.9 V to 11.91 V that a floor of 1 mohm gave. The exceptions are a resistor between two nodes that a source holds, or from a node to itself, and a combination whose zero-ohm parts make a loop (two beads side by side at 0 ohm): those are solved at 1 mohm, as every resistance under it once was.
- **Diodes.** A `dc diode` part blocks, or holds its forward voltage (both ends of the range) and carries a current of at least zero. A state that is not what the circuit says is refused and not counted: a diode in front of a load that always draws cannot block, and one with its anode below its cathode cannot conduct.
- **Supplies.** A `Rail` that the board itself declares, with a `.voltage` that the board states, is an ideal source over that range between its `pos` and its `neg`, and so is the rail that a part says it gives with `dc source` (a connector's supply: `Obd2M` and `UsbCPower` do) when the design states its `.voltage`. Any other rail of a part or of a module is no source: a part's rail is what the part draws, and a module's is a port whose range may be a seed or a promise. A range that is not a source cannot be treated as one: the voltage of a node would then move apart from the current that makes it.
- **Loads.** A rail of a part, or a rail of the board that is not a supply, with a `.current` that is never negative is a current source over that range from its `pos` to its `neg`. A part with a `bridge` and no `dc` statement (an LED, an inductor) that has a `.current` with a value, stated or worked out, is a current source over it between the pins of its `bridge`: this is how a board states what is behind a pin that nothing models (a boundary takes a stated `.current`). A pin that the checker says draws nothing (ADR 0027: a capacitor, an input) adds nothing, and a capacitor is open at DC.

- **Regulators.** A part with `dc linear_regulator` or `dc switching_regulator` holds the net of its output rail over the range of the rail's `.voltage` (an ideal source, so what is behind it is an island of its own) and draws from its input what its output asks. A board rail on the output net is then what the regulator supplies: a load, whatever voltage the board states for it. A linear regulator draws what it supplies and its quiescent current, when its statement names one (the range of the current, both ends added to the load's: the load and the quiescent current are drawn at once). A switching one draws `Pout / (efficiency * Vin)`, and `Vin` is what the parts in front of it leave after the current they carry, so the current is the solution of a fixed point, which the pass finds from zero: the iterates rise to the least solution, and when there is none the input falls to nothing. The power tree is solved in order, the output side first: what the loads behind a regulator take (as a range, summed over everything on its output net) is the regulator's load when the island in front of it is solved. A regulator whose output has a pin that nothing models falls back on its own `.vout.current` when something states it (or the balances worked it out), and is otherwise left alone, with the island in front of it. The pass writes the regulator's `.vin.current` and `.vin.voltage` by the rules below, so the part's own relations (`.vin.voltage >= .vout.voltage + .dropout`, `.vin.voltage >= .buck.min_input_voltage`, a stage's `.vin.voltage <= ...`) are judged with the voltage the circuit gives it, and the parts in front (a fuse, a diode, a shunt, a bead) take their currents from it.

- **Logic pins that drive** (ADR 0076, amendment 4, provisional). A pin of kind `output`, `tri_state` or `bidirectional` that states its `voh` and its `vol` with the current each is measured at, as the datasheet does (`.voh &= .vdd.voltage - 0.4V at 2mA`, `.vol &= 0.4V at 2mA`), is high, low or (a tri-state or bidirectional pin) high impedance. High joins the line to the pin's supply through an output resistance, low joins it to the pin's ground, and high impedance joins it to nothing. The resistance is the drop over the test current, `0.4 V / 2 mA` = 200 ohm, taken from 1 mohm up to that (a lighter load drops less). The pass tries every state of every pin with the states of everything else, and writes what the pin's load carries: a pull-down on the line gets its `.current` and `.drop`, and its relations are judged. The pin needs a reference rail whose two nets a supply or a regulator's output holds, no `.swing` (a line that goes to a supply with no pin has a high the reference rail does not give), and no other pin that could drive its net: two such pins are not modelled, since their states include driving it apart (an input that only leaks, below, is no driver, and may share the net). A pin that states a level without its test current, or a `voh` and no `vol`, is a boundary, unless the part states the output resistances itself (below).

- **Leakage and output resistance** (ADR 0076, amendment 8, provisional). `Logic` and `Levels` have three `optional` members that a part states when its datasheet does: `.ii`, the most current the pin takes or gives when it is high impedance, at any voltage from its ground to its supply (`.io.ii &= 1uA`, the datasheet's `Ii`, a magnitude), and `.roh` and `.rol`, the output resistance when the pin drives high and when it drives low (`.io.rol &= 20ohm..50ohm`). A part that states `.roh` or `.rol` has that resistance, with no test current on its level; one that does not has the drop over the test current, as above. No part has to state any of them. A pin that is high impedance and states `.ii` is, besides open, a resistance to its supply or to its ground, `the supply's greatest voltage / .ii` (3.399 V and 1 uA are 3.399 Mohm): the leak that a pull-down or a pull-up on the line has to carry, and that lowers an open-drain line's high. An input that states `.ii` is such a pin, and draws it, also on a line that a pin drives: the line is the pin's states with the input's leak beside them, to the input's own supply or ground; an input that states none draws nothing, as before. Both directions are states, so the worst of them is in every range.

- **Open-drain pins** (ADR 0076, amendment 8, provisional). A pin that states a `vol` at a test current and no `voh` is open drain, by the inference the levels check makes (ADR 0075): it is low, through `0.4 V / 3 mA` or whatever its low gives, or high impedance, and never high. Any number of such pins on a line never drive it apart (none drives high), so they are modelled together; a pin that can drive the line high next to them is the two-drivers case above and none of the line is modelled. The pass writes nothing for a part on such a line, since the inference might call a push-pull pin that left its high out open drain, and a current written from it would be too low (a pull-down that the pin drives high). It gives the levels check the voltage of the line, below. A pin with `.rol` and a `vol`, and no test current, is open drain with the resistance it states.
- **MOSFETs.** A part with `dc nmos` or `dc pmos` is off, or on with its `.on_resistance` between drain and source (both ends of the range). The gate draws nothing. A gate above the greatest of `.gate_threshold` (above the source for an N-channel part, below it for a P-channel one) turns it on and never off; a gate below the least turns it off and never on; a gate in between keeps both states. A gate that nothing sets in a state (a tri-state pin that let go of a line with no pull) floats, and both states stay. The switch is modelled when what sets its gate is known: the gate's net is held by a supply, or a resistor, a diode or a driving pin is on it, and no pin that nothing models is. Otherwise it is a boundary, a current source over its `.current` when that has a value, as above.

What it does not model, and leaves as it was: a logic pin that does not state its drive, a pin that shares its net with another driver, and every pin of a part with no `dc` statement and no `.current`. A net that has such a pin on it is not solved, and neither is the island it is in (below): a part there keeps its `not evaluated` checks, and the cheap bound of Ohm's law (ADR 0026) is what it gets, as before. A rail of a part that can give current (a `.current` that has a negative end) and that has no `dc` statement is an output that holds its net, and is not modelled.

**Islands and corners.** A net that a supply holds is cut: what is on its two sides is solved on its own, with the supply's range as one of its values. The network between the cuts is an *island*. For each island the pass solves every *corner* of its ranges (both ends of each resistance, supply and load, both ends of a diode's forward voltage, and a diode that blocks) and keeps the range each current and voltage takes over all of them. A corner solve gives each value one number in each solve, so nothing is counted twice: a divider of two resistors of 1 % is `4.95 V..5.05 V` and not the `4.90 V..5.10 V` that arithmetic on ranges gives. The corners are the circuit's range where the quantity is monotone in each value, which is so for a voltage or a current of a network with one supply, and for a series chain; a network with two supplies through resistors need not be. An island with more combinations than a limit is left alone, and a note names it and says to cut it at a node a rail holds. The limit is 65536, and less for a large island: the work is the combinations times the cube of the number of nodes, which is capped, so that no island takes more than a tenth of a second (an island of 15 nodes and 16 resistors of 1 % is the largest that is solved).

**A source that cannot give what the load asks.** A regulator whose output asks for more power than the source can give through the resistance in front of it has no operating point: 12 V through 10 ohm can give at most `12^2 / (4 * 10)` = 3.6 W, and a 5 V output at 1 A and 90 % asks 5.56 W. That is an error that names the regulator, what it asks and the states, in the combinations of the ranges where it happens (a battery at 10 V may not give what it gives at 16 V). A state that is not the circuit's (a diode that blocks in front of the regulator) is refused and is never the finding. An input below the regulator's minimum is an error too: it is the relation the part states for it, judged with the voltage the analysis wrote.

**What it writes.** A field of a modelled part that nothing states takes the range the circuit gives it. A range that a statement gave a field is a budget (ADR 0038): the circuit has to fit inside it, and when it needs more the error says what it needs and the states that give it. A value the solver worked out from a balance is narrowed to what both say, and when the two share no value (for a current) the design contradicts itself: an error names the field, both ranges and the states that give the analysis's nearest end (ADR 0076, amendment 15, provisional), and the field keeps the balance's number. The ranges are widened by a billionth, and by a picoampere or a nanovolt, so that a rounding does not take a value out of the range it is in.

**A chain carries the voltage** (ADR 0076, amendment 5, provisional). The network of the parts in a `~>` chain is solved with the supply behind it, so a rail on a node of it has the voltage the chain leaves there: the supply's, less what each part in series drops at the current that it carries, which the loads behind it set. The pass writes that range into the rail's `.voltage` where nothing states it (a rail joined to it with `~` takes it with it), as it writes a part's `.current`: the board does not restate it with `.vout.voltage == .vin.voltage - .fuse.drop - .diode.drop`, which would list the parts of the chain a second time and go stale when one changes. A relation that reads the voltage is judged with it. A voltage that a statement gave the rail (or any rail joined to it by `~`) is the design's, and is left as it is, not narrowed: a window the board states is a claim about the rail, and it is checked (ADR 0076, amendment 17, provisional; next to the fuse's budget below): the range it states has to enclose the range the circuit gives the node, and one that does not is an error. A window that encloses it is kept as it is, so a start window above the real range stays valid. Not checked are a rail past a regulator's output (the part holds that node, and a rail past an inductor sits a little below the window the board states for it), a node a supply holds, and a network the analysis does not solve. The chain carries the voltage and not the `peak`: a transient is not a DC quantity, and `.vout.peak` is still stated (a `Tvs`'s `.clamping_voltage` on the rail it clamps, say), as ADR 0086 has a fuse take the rail's peak from the chain's start.

The chain below is invented: a vehicle's 4.75 V to 5.25 V supply (as a `Plug` gives it), a link of 0.1 ohm and a diode that drops 0.3 V to 0.5 V, in series, with a load of 0.1 A to 0.3 A at the end. The module `Protection` does not say what its output is, and the board asks for 4.5 V at least. The circuit leaves the output at 4.22 V to 4.94 V (4.75 V less 0.03 V less 0.5 V at the least), so the check fails with the voltage the chain carries:

```nsm fails
import std::sources::Rail

/// Invented: a connector that gives the supply of a vehicle.
part Plug {
    .designator &= "J"
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}

/// Invented: a link, the shape of a fuse, with a stated resistance.
part Link {
    .designator &= "F"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// Invented: a diode that drops 0.3 V to 0.5 V.
part Drop {
    .designator &= "D"
    pins {
        .anode passive "A" "1"
        .cathode passive "K" "2"
    }
    .forward_voltage: Voltage
    .current: Current
    .drop: Voltage
    bridge .anode ~> .cathode
    dc diode .anode .cathode .forward_voltage
}

/// A link and a diode in series from `vin` to `vout`.
module Protection {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .link: Link { .resistance &= 0.1ohm }
    .diode: Drop { .forward_voltage &= 0.3V..0.5V }
    .vin.pos ~> .link ~> .diode ~> .vout.pos
}

board Powered {
    .plug: Plug {
        .supply.voltage &= 4.75V..5.25V
        .supply.pos.net &= "VIN"
        .supply.neg.net &= "GND"
    }
    .protection: Protection
    .load: Rail {
        .pos.net &= "OUT"
        .neg.net &= "GND"
        .current &= 100mA..300mA
    }
    .plug.supply ~ .protection.vin
    .protection.vout ~ .load
    .protection.vout.voltage >= 4.5V
}
```

```text
error: protection.vout.voltage must always be at least 4.5V, but 4.22V..4.94V is not always above 4.5V (the DC analysis puts `protection.vout.voltage` at 4.22V..4.94V, its high end with plug.supply at 5.25V, protection.diode conducting at 300mV, load at 100mA, its low end with plug.supply at 4.75V, protection.diode conducting at 500mV, load at 300mA)
   --> main.nsm:67:5
   |
67 |     .protection.vout.voltage >= 4.5V
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**A circle that a pick waits on.** A part picked from a catalog can read a voltage that only the circuit gives: a capacitor's effective capacitance falls with the bias across it (ADR 0013), and the bias of the capacitor across a regulator's input is the voltage behind a fuse and a diode, which depends on the regulator's input current, which depends on that voltage. Nothing has a value to start from, and the board used to write a window by hand (`.reg.vin.voltage &= 7V..16V`) that was not a requirement but a start. Now, when a pick waits on a bias that has no value, the solver solves the network once before the picks, with each part still to be picked taken at the hull of the values its candidates can have (a fuse's resistance over the rows that fit what the design says of it), and gives each rail and each regulator's input the voltage range that results. The picks are then judged on that range, which holds whatever they choose; after the picks the pass narrows it to the circuit that was chosen. It starts a circle that the network can solve, and only that: the network behind a regulator must be known, or enclosed, before the picks: the loads on a regulator's output are either modelled (a rail with a current, a part with a stated current, a resistor that is a pick, at the hull of its candidates) or, when one is not (a pin the analysis does not model), the regulator's own limit on what its output supplies (`-.vout.current <= .max_current` of `LinearRegulator`, `-.sw.current <= .max_output_current` of `Buck`) is the load, from nothing up to that limit. That is an enclosure of every load a pick could make, and it is wide: the pass after the picks has the loads themselves and narrows the voltage to the circuit that was picked. A regulator whose output has no such limit, and a part still to be picked whose value a catalog row leaves to the design with nothing bounding it (a generic inductor with no `.max_inductor_dcr`), has no enclosure, and its network is left to the pass after the picks, as before. The start also leaves out what cannot be enclosed without refusing parts that are fine: a resistor that is a pick has every resistance of its catalog in the hull, so the voltage behind a series resistor whose value a pick decides is not enclosed in a way a check on a rail could use: that pick is judged on the circuit once it is chosen ("A pick is refused on the circuit's own voltage", below). A board that builds without it is solved as before, since the start runs only when a pick waits on a bias. A window stated by the board is used as stated and starts nothing, as ever.

**The load of a regulator before the picks.** The parts in front of a regulator (a fuse, a shunt, a bead) are picked against the current that the regulator draws, which is what its output supplies, which is the sum of what the loads on its output draw. A load whose current depends on a pick that has not been made (a pull-up whose resistor is still to be chosen, a bleeder, a divider) makes that sum unknown, and a part in front of the regulator was picked with no current to judge its rating against: the first row of the catalog that has none refused was taken, and a board that stated a window on the regulator's output (`.ldo.vout.current &= -0.3A..0A`) got other parts than a board that did not, though the loads state a smaller current than the window. Now the solver works the balances of the currents once before the picks with every column of every part still to be picked at the hull of its candidates (and inside what the design holds the column equal to: `.resistance == .resistor.resistance` with the window the board states for `.resistance`), and a regulator whose output current they give has it written as a worked-out current (the sum of what the loads state, each over every pick it could have), which the balances narrow as the picks make the loads known. It is written only when every load behind the regulator has a current the design's relations give, and only when the sum lies inside the limit the regulator is rated for; a load that states nothing (a pin the analysis does not model, a part whose current no relation gives) leaves the regulator's current with no value, and the limit is the load it takes in the start, as before: an unstated load is never guessed. A window the board states is used as stated. A board that has deleted such a window may pick different, usually smaller, parts in front of its regulators, and the pass after the picks judges them on the circuit as ever.

**The parts that read the analysis are picked after it.** When the network cannot be solved before the picks (a Zener whose current is a window that reads its own leakage leaves its net open, and so does a divider whose resistors are all still to be picked), the solver picks first the parts that read no voltage that nobody has given (a fuse, a diode, a shunt, a bead, the resistors in the chain), solves the network with them in, and picks the rest on what it gives (a capacitor's bias, a divider's draw, a Zener's test voltage). A regulator's input has no voltage while the parts in series with it are picked, and a fuse or a bead is judged on the current that it carries; for those picks the input is taken between what the part and the design need of it (`.vin.voltage >= 3V`, `>= .vout.voltage`, the largest of them) and the highest supply that reaches it, which is what a board used to write by hand as `.reg.vin.voltage &= 7V..16V`. That window is an assumption about a design that is valid and not a result: it is taken out again before the picks are kept, so nothing it gave stays in the model, and the pass after the picks judges each pick on the circuit. A part still to be picked that the design limits the current of (a divider's `.top.current <= .max_draw`, which `.bottom.current` is held to) is a load of at most that limit; a part with no `dc` element whose `.current` is a window that reads a column of the part (`.clamp.current &= 0A .. .clamp.leakage_current`) has the hull of its candidates in that column; and no voltage is written for a node that only a part still to be picked joins to the supplies (the supply of a transceiver behind a series resistor that is a pick). A board with no pick that waits on the analysis after the first solve is picked in one go, as before. A pick that can be refused on the analysis' own numbers, such as a series resistor in front of a clamp, is the next paragraph's.

The input capacitor below is picked from the catalog and states only what it must be (4 uF effective at its bias); the board writes no window for the regulator's input voltage:

```nsm evaluate
import std::parts::passive::Capacitor
import std::sources::Rail

/// Invented: a connector that gives the supply.
part Plug {
    .designator &= "J"
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}

/// Invented: a link, the shape of a fuse, with a stated resistance.
part Link {
    .designator &= "F"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// Invented: a converter that passes 3.3 V at 90 % efficiency, from at least 3 V.
part Converter {
    .designator &= "U"
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .vin.neg ~ .vout.neg
    .vin.voltage >= 3V
    dc switching_regulator .vin .vout .efficiency
    pins {
        .vin.pos power "VIN" "1"
        .vin.neg power "GND" "2"
        .vout.pos power "VOUT" "3"
    }
}

board Started {
    .plug: Plug {
        .supply.voltage &= 4V..5.5V
        .supply.pos.net &= "VIN"
        .supply.neg.net &= "GND"
    }
    .link: Link { .resistance &= 0.2ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 3.3V }
    .load: Rail {
        .pos.net &= "V33"
        .neg.net &= "GND"
        .current &= 1A
    }
    .input_cap: Capacitor { .effective_capacitance >= 4uF }
    .plug.supply.pos ~> .link ~> .reg.vin.pos
    .plug.supply.neg ~ .reg.vin.neg
    .reg.vin.pos ~> .input_cap ~> .reg.vin.neg
    .reg.vout ~ .load
}
```

**A pick is refused on the circuit's own voltage.** A series resistor in front of a Zener clamp sets the voltage of the node behind it with the load's current and the Zener's, and used to be given that voltage by an equation the board wrote (`.out.voltage == .supply.voltage - .r.drop`), so that a pair of candidates that leaves a load under its minimum was refused when the pair was picked. The equation is no longer needed (ADR 0076, amendment 10, provisional): when the last part of a group of parts picked together is chosen, the solver solves the island of the network that has a part of the group, with the picks in it, and gives the rails on it whose voltage nothing has given the voltage that results; the checks that read them (`.out.voltage >= 4.5V`) judge the pick with it, and a pair that fails is refused with the message of any other pair that fails (`no combination of candidates satisfies ... together`). The island is cut at the rails whose voltage an earlier pass worked out (the supply of the module, behind a fuse and a shunt that were picked first), so what is solved is the resistor, the clamp and the load, and not the whole power tree for each of the candidates; an island with a regulator in it is left to the pass after the picks, and so is a group with no such rail (a rail whose voltage is stated, or no check that reads one). An equation the board wrote stays valid. The board below picks the pair on the analysis' number alone; with a supply that sags to 6 V it fails at the pick, as it did with the equation:

```nsm evaluate
import std::parts::diode::Zener
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: a connector that gives the supply.
part Plug {
    .designator &= "J"
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}

board Clamped {
    .plug: Plug {
        .supply.voltage &= 9V..16V
        .supply.peak &= 58V
        .supply.pos.net &= "VIN"
        .supply.neg.net &= "GND"
    }
    .out: Rail {
        .pos.net &= "OUT"
        .neg.net &= "GND"
        .current &= 0A..200uA
    }
    .plug.supply.neg ~ .out.neg
    .r: Resistor
    .clamp: Zener
    .plug.supply.pos ~> .r ~> .out.pos
    .out.pos ~> .clamp ~> .out.neg
    .clamp.leakage_test_voltage >= .out.voltage
    .clamp.current &= 0A .. .clamp.leakage_current
    .clamp.zener_voltage >= 36V
    .surge_current: Current
    .surge_current == (.plug.supply.peak - .clamp.zener_voltage) / .r.resistance
    .surge_current <= .clamp.test_current
    .out.voltage >= 4.5V
}
```

A divider with an LED on its middle. The top resistor carries what the bottom one and the LED take; nothing states it, and the checks of both resistors are made:

```nsm solve
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board Lit {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .top: Resistor { .resistance &= 1kohm +/- 1% }
    .bottom: Resistor { .resistance &= 2kohm +/- 1% }
    .led: Led { .current &= 1mA..2mA; .forward_voltage &= 1.8V..2.4V }
    .supply.pos ~ .top.a
    .top.b ~ .bottom.a
    .top.b ~ .led.anode
    .bottom.b ~ .supply.neg
    .led.cathode ~ .supply.neg
}
```

A pin that drives a pull-down, and a switch that the pin turns on. The controller and the switch are invented. The controller's output is high at 3.3 V less 0.4 V at 2 mA, so its output resistance is at most 200 ohm; the pull-down is allowed no more than the supply over it, and the load carries nothing while the pin is low:

```nsm evaluate
import std::logic::Logic
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: an output that states its levels at the load of its datasheet.
part Mcu {
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V at 2mA
    .gpio.vol &= 0.4V at 2mA
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .gpio.line output "IO"  "3"
    }
}

/// Invented: an N-channel switch.
part LowSideSwitch {
    pins {
        .gate   input   "G" "1"
        .source passive "S" "2"
        .drain  passive "D" "3"
    }
    .gate_threshold: Voltage
    .on_resistance: Resistance
    .current: Current
    bridge .drain ~> .source
    dc nmos .gate .drain .source .gate_threshold .on_resistance
}

board Switched {
    .main: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .mcu: Mcu
    .pd: Resistor { .resistance &= 10kohm +/- 1% }
    .q: LowSideSwitch { .gate_threshold &= 1V..2.5V; .on_resistance &= 50mohm..100mohm }
    .load: Resistor { .resistance &= 10kohm +/- 1% }
    .logic ~ .mcu.vdd
    .mcu.gpio.line ~ .q.gate
    .mcu.gpio.line ~ .pd.a
    .pd.b ~ .logic.neg
    .main.pos ~ .load.a
    .load.b ~ .q.drain
    .q.source ~ .main.neg
}
```

A stated range is a budget. Here the fuse is stated to carry at most 100 mA, and what hangs behind it needs up to 300 mA:

```nsm fails
import std::parts::passive::Fuse
import std::sources::Rail

part Mcu {
    .vdd: Rail
}

board Fused {
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .fuse: Fuse { .rated_current &= 1A; .current &= 0A..100mA }
    .mcu: Mcu { .vdd.current &= 50mA..300mA }
    .supply.pos ~> .fuse ~> .mcu.vdd.pos
    .supply.neg ~ .mcu.vdd.neg
}
```

```text
error: `fuse.current` is stated 0A..100mA, but the DC analysis of the circuit around it finds 50mA..300mA: at most 300mA at the high end with mcu.vdd at 300mA
   --> main.nsm:14:41
   |
14 |     .fuse: Fuse { .rated_current &= 1A; .current &= 0A..100mA }
   |                                         ^^^^^^^^^^^^^^^^^^^^^
```

A stated voltage is a budget as well. The load below is stated to take 1.7 V to 3.6 V on its input, and the 12 V supply reaches it through a resistor that drops 1 V to 2 V at the LED's current: the circuit puts the node at 9.98 V to 11.01 V, outside the claim. The same holds when the statement is on a module's port that the board joins to a regulator's input (a seed): it is a claim about the node, and a regulator in dropout does not build. A range that encloses what the circuit gives is accepted and stays as written.

```nsm fails
import std::sources::Rail
import std::parts::passive::{Resistor, Led}
import std::parts::connector::Header1x2

module Load {
    .vin: Rail
    .led: Led { .current &= 10mA..20mA }
    .vin.pos ~> .led ~> .vin.neg
}

board Claim {
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .r: Resistor { .resistance &= 100ohm +/- 1% }
    .load: Load { .vin.voltage &= 1.7V..3.6V }
    .supply.pos ~> .r ~> .load.vin.pos
    .supply.neg ~ .load.vin.neg
    .p: Header1x2
    .supply.pos ~ .p.p1
    .supply.neg ~ .p.p2
}
```

```text
error: `load.vin.voltage` is stated 1.7V..3.6V, but the DC analysis of the circuit around it finds 9.98V..11.01V: at most 11.01V at the high end with r at 99Ω, load.led at 10mA
   --> main.nsm:18:19
   |
18 |     .load: Load { .vin.voltage &= 1.7V..3.6V }
   |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A source that cannot give what a regulator asks. The link is invented, and so is the regulator (a part that states `dc switching_regulator`, as a buck converter's IC does):

```nsm fails
import std::sources::Rail

/// Invented: a link of 10 ohm.
part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// Invented: a 5 V converter.
part Converter {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .vin.neg ~ .vout.neg
    dc switching_regulator .vin .vout .efficiency
}

board Starved {
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .link: Link { .resistance &= 10ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 5V }
    .out: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 1A
    }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout ~ .out
}
```

```text
error: the source cannot give what `reg` asks (5W at its output, efficiency 90 %): its input falls to 0V and no operating point exists
   --> main.nsm:31:5
   |
31 |     .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 5V }
   |     ^^^^^^^^^^^^^^^
```

A supply that can give only so much states it as its `.current`, a negative number because a rail's current is what is drawn at its `pos` (ADR 0014): `.supply.current &= -500mA..0A` is a supply of at most 500 mA (ADR 0076, amendment 13, provisional). The loads that the analysis finds on the supply's net are held to it by the rule that ADR 0038 gives two stated ranges, which must be able to meet: the range the loads draw over every corner and the budget. An error says that they cannot, that even the least the loads draw is more than the supply gives, and names the supply, the budget, the least and the most the loads draw, and the corner that gives the least. A supply that states no `.current` has no budget, one with no negative end in its range is left to the balance of its net, and a balance that has already refused the same supply says it once. The draw is the sum over the networks on the net that the analysis solved, so a load that no network models is not in it and the finding is never made on a load that is not there. The converter is invented: at 90 % a 5 V output at 2 A asks 11.1 W, which is 0.70 A from 16 V and 1.25 A from 9 V, so even from 16 V it is more than the 500 mA the supply states.

```nsm fails
import std::sources::Rail

/// Invented: a link of 0.1 ohm.
part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// Invented: a 5 V converter.
part Converter {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .vin.neg ~ .vout.neg
    dc switching_regulator .vin .vout .efficiency
}

board Small {
    .supply: Rail {
        .voltage &= 9V..16V
        .current &= -500mA..0A
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .link: Link { .resistance &= 0.1ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 5V }
    .out: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 2A
    }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout ~ .out
}
```

```text
error: `supply.current` is stated -500mA..0A, so the supply `supply` gives at most 500mA, but the DC analysis finds the loads on its net drawing at least 697.5mA and up to 1.252A, the least with supply at 16V
   --> main.nsm:25:5
   |
25 |     .supply: Rail {
   |     ^^^^^^^^^^^^^
```

**Limits.** The pass tries every combination of the states of the parts it sees, which is the worst case. A regulator's other pins on the nets of its input (an IC with two input pins, an enable tied to the input) are its input's and draw nothing more, and its output is held at the node its part names, which for a buck converter is the switch node: the output beyond the inductor is a little below what a stated window asks, and a window the board states is therefore not checked against the circuit. A power tree is solved only when every pin on its nets is modelled: a regulator that states no `dc` element, or a load whose current is not known, leaves the island in front of it unsolved. Nothing is solved before the picks but the start of a circle, which needs the currents of the loads to be known by then. A design cannot say that two pins are never high together, or that two loads never run together: the pass does not know, and it adds both (issue #45 asks for a way to say it). Temperature is a stated tolerance and nothing else. A capacitor is open, so nothing charges: an inrush current is not seen. A diode's reverse leakage and a MOSFET's body diode are not modelled. A regulator's efficiency is one range and not a curve over its load and its input, a regulator's quiescent current is modelled only when its `dc` statement names it (the standard `LinearRegulator` and `Lt8641` do), a regulator is an ideal source (its output does not sag, and a current limit is the part's own relation), and an input below its minimum is an error and not a shutdown. A logic pin has no leakage (high impedance is an open circuit: the language has no member for the datasheet's `Ii`), and a pin's output resistance is the one at the test current its level states. A regulator's output current is summed over the islands on its output net, each solved on its own, so a load whose current follows the output voltage is taken over the whole range of the output (wider, never narrower).

### Logic levels and absolute maximums

A digital line is a `Logic` (`std::logic::Logic`): a pin `.line` and the rail `.reference` whose `neg` is its ground. It also has four voltages, `.vih`, `.vil`, `.voh` and `.vol`, as a datasheet gives them (ADR 0046). A part states those it has, with fixed volts or with an equation on its rail: `.in.vih &= 2V`, `.in.vih &= 0.7 * .vdd.voltage`, `.out.voh &= .vdd.voltage - 0.1V`. Every pin also has `.abs_min` and `.abs_max`, the voltages it survives relative to its ground, stated the same way: `.in.line.abs_max &= 3.6V`.

After the rails are solved, the solver checks two things. A fault is an error; a check that cannot be made is a note `not evaluated: ...`, never a pass.

- **Levels.** For every output or bidirectional `Logic` line on a net and every input or bidirectional `Logic` line on it, the output's `voh` must always be at least the input's `vih`, and its `vol` at most the input's `vil`, in the worst case of both ranges. A level with no value, or two lines whose grounds (`reference.neg`) are not on one net or in one ground group, is not evaluated. A pair in which neither line states a level is not looked at. An output that states a `vol` and no `voh` is open-drain, and the line's high is what its pull-up gives it (see "An open-drain line" below).
- **Absolute maximum.** A pin with `.abs_min` or `.abs_max` must have a net whose worst-case voltage relative to its ground stays within them. A net that the DC analysis solved (a divider's middle, a net behind a chain of resistors and diodes: see "DC analysis") has the range of voltage the analysis finds for it over every corner of the ranges and every state of the parts around it, and the finding says `(the DC analysis of the network it is in)` (ADR 0076, amendment 12, provisional). Any other net's voltage is the hull of: its ground (0 V), each rail whose `pos` is on it, each `Logic` line that drives it (between its reference rail's `neg` and its voltage), and each rail or ground that a resistor joins it to (a pull-up or a pull-down). It is not evaluated for a pin that is not the `.line` of a `Logic` (its ground is not known), for a net that something drives that is not a `Logic` line, for a rail or driver on another ground, and for a net nothing sets. A pin that nothing outside its own part is joined to (an optional pin left to float, such as the `TR/SS` of the LT8641) has no voltage to judge and is skipped, with no note; a pin with only a capacitor on it is the note "nothing sets its voltage".

A 5 V rail through a divider into a pin that survives 3.6 V is judged by the voltage the divider gives, not by the rail: 10 kohm over 20 kohm from 5.25 V is at most 3.52 V, inside the maximum, and the same divider over 30 kohm is 3.96 V, which is refused. The parts are invented.

```nsm evaluate
import std::logic::Logic
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: survives -0.5 V to 3.6 V on its input.
part Pad {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.line.abs_min &= -0.5V
    .in.line.abs_max &= 3.6V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Divided {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC5"
        .neg.net &= "GND"
    }
    .core: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .pad: Pad
    .top: Resistor { .resistance &= 10kohm +/- 1% }
    .bottom: Resistor { .resistance &= 20kohm +/- 1% }
    .core ~ .pad.vdd
    .supply.pos ~> .top ~> .pad.in.line
    .pad.in.line ~> .bottom ~> .supply.neg
}
```

Join the lines of two parts pin to pin, `.a.out.line ~ .b.in.line`, and each part's reference rail to its own supply. Joining the two `Logic`s (`.a.out ~ .b.in`) also joins their reference rails, which is right only when both parts have the same supply; it does not equate their levels, which are each pin's own.

The levels below are invented, from no datasheet: `Driver` drives within 0.1 V of its supply and `Receiver` reads a high from 70 % of its supply.

```nsm evaluate
import std::logic::Logic
import std::sources::Rail

/// Invented: drives a high to its supply less 0.1 V and a low to 0.1 V.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

/// Invented: reads a high from 70 % of its supply and a low up to 30 %.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Agree {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Driver
    .b: Receiver
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.out.line ~ .b.in.line
}
```

The same driver into a receiver on a 5 V supply is refused with the numbers: a high of 3.035 V is not enough for 70 % of 5 V.

```nsm fails
import std::logic::Logic
import std::sources::Rail

/// Invented: drives a high to its supply less 0.1 V and a low to 0.1 V.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

/// Invented: reads a high from 70 % of its supply and a low up to 30 %.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Mismatch {
    .low: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .high: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC5"
        .neg.net &= "GND"
    }
    .a: Driver
    .b: Receiver
    .low ~ .a.vdd
    .high ~ .b.vdd
    .a.out.line ~ .b.in.line
}
```

```text
error: net `net-a.out.line`: `a.out.voh` is 3.035V..3.365V but `b.in.vih` needs 3.325V..3.675V: a high of 3.035V is not always read as a high, which takes 3.675V
   --> main.nsm:43:5
   |
43 |     .a: Driver
   |     ^^^^^^^^^^
```

The library has one real part that states levels, `std::parts::logic::Sn74lvc1g04`, a single inverter from TI's datasheet SCES214AF, modelled for a supply of 3 V to 3.6 V. Its source is in the definition, table by table: `vih` 2 V and `vil` 0.8 V (table 5.3), `voh` as VCC - 0.1 V and `vol` 0.1 V at 100 uA (table 5.5), and the absolute maximums of table 5.1. Its symbol is drawn from its pin table (the row in `std/kicad.toml` names none). Two of them in a row, with a header on the input, check and solve with one note, because nothing on the board sets the voltage of the header's net:

```nsm evaluate
import std::parts::connector::Header1x2
import std::parts::logic::Sn74lvc1g04
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Inverters {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u1: Sn74lvc1g04 { .package &= "SOT-23-5" }
    .u2: Sn74lvc1g04 { .package &= "SOT-23-5" }
    .j: Header1x2
    .supply ~ .u1.vcc
    .supply ~ .u2.vcc
    .j.p1 ~ .u1.a.line
    .u1.y.line ~ .u2.a.line
    .j.p2 ~ .u2.y.line
}
```

The note, in `netsmith build`'s output: `not evaluated: the absolute maximum of `u1.a.line` against net `net-j.p1`: nothing sets its voltage: no rail is on it and no `Logic` line drives it`.

The level checks apply to the lines that are `Logic` and to the pins of a bus interface that states `.levels` (see "Logic levels of a bus" below). A rating of any other pin (its `.abs_max` stated on a plain `Pin`) is listed as not evaluated.

#### Levels that are fractions of one supply

A datasheet often states a level as a fraction of the supply: a digital microphone's output is high at 0.7 of its supply, and the processor's input reads a high from 0.7 of its supply. When both parts are on one rail, at any actual supply voltage the output's high is exactly the input's threshold. Compared as two independent ranges (3.3 V +/- 5 % gives 2.194 V..2.426 V on both sides), the output's lowest high would be below the input's highest threshold, and the pair would be refused for a margin that does not exist.

So a level that is stated as a number times a rail's voltage, `.out.voh &= 0.7 * .vdd.voltage` (or `.vdd.voltage * 0.7`, or the voltage alone), is compared with another such level as a fraction of one supply voltage `V`. This holds when both name rails whose `pos` pins are on one net and whose `neg` pins are the same ground (ADR 0082). For every `V` above zero, `k_driver * V >= k_receiver * V` is exactly `k_driver >= k_receiver`, so the check compares the two factors: a high passes when the driver's factor is at least the receiver's, a low when it is at most.

```nsm evaluate
import std::logic::Logic
import std::sources::Rail

/// Invented: a driver whose levels are 0.7 and 0.3 of its supply.
part Mic {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.7 * .vdd.voltage
    .out.vol &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

/// Invented: reads a high from 0.7 of its supply and a low up to 0.3 of it.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board OneSupply {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Mic
    .b: Receiver
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.out.line ~ .b.in.line
}
```

An output that is high only at 0.65 of its supply into the same input is refused, and the message names the factors and no ranges:

```nsm fails
import std::logic::Logic
import std::sources::Rail

/// Invented: a driver whose high is 0.65 of its supply.
part Weak {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 0.65 * .vdd.voltage
    .out.vol &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

/// Invented: reads a high from 0.7 of its supply and a low up to 0.3 of it.
part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board OneSupply {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Weak
    .b: Receiver
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.out.line ~ .b.in.line
}
```

```text
error: net `net-a.out.line`: `a.out.voh` is 0.65 x `a.vdd.voltage` but `b.in.vih` needs 0.7 x `b.vdd.voltage`, the same supply voltage: a high of 0.65 x the supply is not always read as a high, which takes 0.7 x the supply
   --> main.nsm:38:5
   |
38 |     .a: Weak
   |     ^^^^^^^^
```

The rule is narrow, and anything it does not recognise is compared as independent ranges, as before:

- A level that is a sum or any other form (`.vdd.voltage - 0.1V`, a clamp, a number alone) is a range, so a fraction against a sum is compared by ranges.
- A level that is stated by more than one statement (also narrowed by a number, or mentioned by another relation) is not only a fraction.
- Two rails on different nets (two supplies, even of the same nominal voltage) are two voltages, so their ranges are independent.
- A rail whose voltage can be zero or negative is not used (the reduction divides by it).

#### A line that states its own swing

A module whose outputs swing to an internal supply that has no pad (a cellular module's `STATUS` pin swings to an internal 1.8 V rail that the module does not bring out) cannot state that rail as a `Logic`'s `reference`: a rail in a part needs pads. It states the swing instead (ADR 0081, provisional): `.swing`, a voltage range on the `Logic` or on the `Levels` of a bus interface, the range of voltage the line can have **relative to its ground**, the `neg` of its `.reference` rail, which is where the four levels and the absolute maximums are read from. A signed range is read as it is written: `-0.3V .. 1.8V` goes 0.3 V below the ground and 1.8 V above it. The part joins `.reference` to a supply it does have (`~ .vbat`) for the ground only; that rail's voltage is then not what the line swings to.

The absolute-maximum check takes the stated range as what the line contributes to the voltage of its net (and the Ohm's-law bound on a resistor's current takes it too), in place of "from the ground to the top of the reference rail". A line that states no swing is judged exactly as before. The invented `Modem` below swings to 1.8 V from a 3.8 V battery, and `Sensor` survives 2.1 V:

```nsm evaluate
import std::logic::Logic
import std::sources::Rail

/// Invented: a module on a 3.8 V battery whose STATUS output swings to an internal 1.8 V
/// rail that has no pad.
part Modem {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V .. 1.8V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// Invented: an input that survives 2.1 V.
part Sensor {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.line.abs_max &= 2.1V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Telemetry {
    .battery: Rail {
        .voltage &= 3.8V +/- 5%
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .m: Modem
    .s: Sensor
    .battery ~ .m.vbat
    .io ~ .s.vdd
    .m.status.line ~ .s.in.line
}
```

Without the statement the line would be taken to swing to the battery's 3.99 V, and the same sensor on a 1.0 V maximum is refused with the stated range named:

```nsm fails
import std::logic::Logic
import std::sources::Rail

/// Invented: a module on a 3.8 V battery whose STATUS output swings to an internal 1.8 V
/// rail that has no pad.
part Modem {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V .. 1.8V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// Invented: an input that survives 1.0 V.
part Sensor {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.line.abs_max &= 1V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Telemetry {
    .battery: Rail {
        .voltage &= 3.8V +/- 5%
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .m: Modem
    .s: Sensor
    .battery ~ .m.vbat
    .io ~ .s.vdd
    .m.status.line ~ .s.in.line
}
```

```text
error: net `net-m.status.line` reaches 1.8V above its ground `GND` (`m.status.line`, a line that states a swing of 0V..1.8V) but `s.in.line.abs_max` is 1V: it must not exceed 1V
   --> main.nsm:43:5
   |
43 |     .s: Sensor
   |     ^^^^^^^^^^
```

A line that states a swing and no `voh` or `vol` also has its unloaded output levels from it (ADR 0081, amendment 1, provisional): the high is the top of the swing and the low is its bottom, as single values. The notes `the high level from ... : ... .voh has no value` and `the low level ... .vol has no value` then close, and the receiver's `vih` and `vil` are checked against those levels. A level the line states always wins, so a part that states `voh` or `vol` is judged exactly as before. A line that states a low and no high is still an open-drain line (ADR 0075): its high is what its pull-up holds the line at, not the top of the swing. A line that states no swing behaves as before. The swing alone does not make a pair of lines worth checking: the pair is looked at when one of them states a level, as before. The same modem reaches 1.8 V, and a receiver that reads a high only from 1.9 V is refused, with the swing named:

```nsm fails
import std::logic::Logic
import std::sources::Rail

/// Invented: a module on a 3.8 V battery whose STATUS output swings to an internal 1.8 V
/// rail that has no pad, and states no level.
part Modem {
    .vbat: Rail
    .status: Logic
    .status.reference ~ .vbat
    .status.swing &= 0V .. 1.8V
    pins {
        .vbat.pos     power  "VBAT"   "1"
        .vbat.neg     power  "GND"    "2"
        .status.line  output "STATUS" "3"
    }
}

/// Invented: an input that reads a high only from 1.9 V and a low up to 0.4 V.
part Sensor {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 1.9V
    .in.vil &= 0.4V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

board Telemetry {
    .battery: Rail {
        .voltage &= 3.8V +/- 5%
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .m: Modem
    .s: Sensor
    .battery ~ .m.vbat
    .io ~ .s.vdd
    .m.status.line ~ .s.in.line
}
```

```text
error: net `net-m.status.line`: `m.status.voh` (not stated: the top of its swing, `m.status.swing`) is 1.8V but `s.in.vih` needs 1.9V: a high of 1.8V is not always read as a high, which takes 1.9V
   --> main.nsm:43:5
   |
43 |     .m: Modem
   |     ^^^^^^^^^
```

#### A level and the current it was measured at

A datasheet gives an output level at a load: `VOH = VDD - 0.4 V at IOH = 2 mA`. A part states it the same way, with `at` and the current after the value (ADR 0076, amendment 1, provisional):

```nsm-fragment
.out.voh &= .vdd.voltage - 0.4V at 2mA
.out.vol &= 0.4V at 8mA
```

The statement means what it means without `at`: the same narrowing, the same check of the levels above. The current is recorded beside the level, for the analysis that will derive the pin's output resistance from it (`(vdd - voh) / 2 mA` at most). Only `voh` and `vol` take one, of a `Logic` or of a bus's `Levels`; every comparison (`&=`, `?=`, `==`, `>=`, `<=`) may carry it. The current is one positive number written in the statement, not a range and not another field, and a level is stated at one load.

```nsm evaluate
import std::logic::Logic
import std::sources::Rail

/// Invented: drives a high to its supply less 0.4 V at 2 mA and a low to 0.4 V at 8 mA.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.4V at 2mA
    .out.vol &= 0.4V at 8mA
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board Stated {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Driver
    .supply ~ .u.vdd
}
```

An input level is measured at no load, and the current must be a current:

```nsm fails
import std::logic::Logic
import std::sources::Rail

part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.vih &= 2V at 2mA
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Driver
    .supply ~ .u.vdd
}
```

```text
error: `at <current>` states the load of an output level; `.out.vih` is a level that a datasheet states without a load: only an output level, `voh` or `vol`, is measured at a current
  --> main.nsm:8:23
  |
8 |     .out.vih &= 2V at 2mA
  |                       ^^^
```

```nsm fails
import std::logic::Logic
import std::sources::Rail

part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 2.4V at 2V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Driver
    .supply ~ .u.vdd
}
```

```text
error: `at` takes the current the level is measured at, such as `2mA`, not a voltage
  --> main.nsm:8:25
  |
8 |     .out.voh &= 2.4V at 2V
  |                         ^^
```

A level has one load. A datasheet that gives it at two currents gives two values, and a level holds one:

```nsm fails
import std::logic::Logic
import std::sources::Rail

part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= 2.4V at 2mA
    .out.voh >= 2V at 8mA
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Driver
    .supply ~ .u.vdd
}
```

```text
error: `.out.voh` is already stated at 2mA (at main.nsm:8): a level is one value at one load, so keep one `at`
  --> main.nsm:9:23
  |
9 |     .out.voh >= 2V at 8mA
  |                       ^^^
```

### A level translator

`std::parts::logic::LevelTranslator` is the generic of a single-supply translating buffer: a gate whose input accepts a lower level than its own supply and whose output swings between ground and that supply, so a signal from a 1.8 V part reaches a 3.3 V one. It has the rail `vcc` and the lines `a` (an input `Logic`, which must be connected) and `y` (an output `Logic`), and the pin `nc`, in the pad order of the 5-pin single-gate packages (SOT-23-5 and SOT-353_SC-70-5, drawn with KiCad's `74xGxx:74LVC1G17`). Join `.a.line` and `.y.line`, as for any `Logic`. A part states what its datasheet gives, as columns of the catalog `level_translator`:

- `.vcc_min` and `.vcc_max`: the supply range; the rail on `vcc` must be inside it.
- `.vcc_abs_max` and `.a_abs_max`: the absolute maximums of the supply and of the input. The design states the highest voltage it can put on each, transients included, as `.vcc_peak` and `.a_peak`, and each must be at most its maximum. The input line's `.abs_max` is `.a_abs_max` too, so the solver also checks the worst voltage of the input's net against it (ADR 0046).
- `.vt_plus_max` and `.vt_minus_min`: the input thresholds. An input is a high for sure above the first (the positive-going threshold's highest value) and a low for sure below the second (the negative-going threshold's lowest value). They are the input's levels: `.a.vih` is `.vt_plus_max` and `.a.vil` is `.vt_minus_min`, so the solver judges the driver on the net of `.a.line` with no statement of the design: its `voh` must always be at least the first and its `vol` at most the second.
- `.y` states no level, because the catalog has no column for the output's `voh` and `vol` (a datasheet gives them at a stated load). A receiver on `.y.line` that states its own levels is listed as not evaluated against it; a design that has the datasheet's pair states it, `.u.y.voh &= .u.vcc.voltage - 0.1V`.
- `.max_output_current`: the output may source or sink that much, and `.output_current`, which the design states, is at most that.
- `.max_supply_current`: the largest current the supply pin draws in the use the row is for. The part states it as its own draw, `.vcc.current &= 0A .. .max_supply_current` (a range with a value at its end, see Values), so the rail the translator is on carries it with nothing stated by the design. A design may narrow it (`.u.vcc.current &= 0A..500nA`) and cannot widen it: a draw wholly above the maximum is a contradiction, and a wider one is narrowed to it. That is what `&=` does everywhere (atopile's does the same: it intersects the field's set with yours and fails only if the result is empty), so a design's wider draw is not an error and is not kept: the checks that follow are made on the row's range. A row from a datasheet that gives only a static current for a part that switches states the figure for its use.

The standard catalog has no rows, because no translator has been entered with a recorded source (`data/jlcpcb/README.md`). A project writes the rows of its parts in a file named `level_translator.csv`, as for any standard catalog, or a part of its own says `provides LevelTranslator`. The part of this example is invented, and so is its row:

```csv file=catalogs/level_translator.csv set=translator
mpn,name,vcc_min,vcc_max,vcc_abs_max,a_abs_max,vt_plus_max,vt_minus_min,max_output_current,max_supply_current,package,library,source
ZT-17,ZT-17 single-supply translator,2.5V,3.6V,4.2V,4.2V,1.1V,0.45V,3mA,750nA,SOT-23-5,Extended,https://example.invalid/zt-17
```

```nsm module=src::sensor
import std::logic::Logic
import std::sources::Rail

/// Invented: a sensor with a push-pull output of a given swing.
part Sensor {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.2V
    .out.vol &= 0.2V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}
```

A 1.8 V sensor drives a 3.3 V line through the translator. The design states the stresses; the input is judged against the driver by the solver, and the supply pin's draw is the row's own:

```nsm evaluate catalogs=translator
import src::sensor::Sensor
import std::parts::logic::LevelTranslator
import std::sources::Rail

board Sensing {
    .low: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .high: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .sensor: Sensor
    .u: LevelTranslator {
        .vcc_peak &= 3.6V
        .a_peak &= 1.98V
        .output_current &= 1mA
    }
    .sensor.vdd ~ .low
    .u.vcc ~ .high
    .sensor.out.line ~ .u.a.line
}
```

The same sensor on a 0.5 V rail drives a high of 0.3 V, which the translator does not read as a high for sure:

```nsm fails catalogs=translator
import src::sensor::Sensor
import std::parts::logic::LevelTranslator
import std::sources::Rail

board Sensing {
    .low: Rail {
        .voltage &= 0.5V +/- 5%
        .pos.net &= "0V5"
        .neg.net &= "GND"
    }
    .high: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .sensor: Sensor
    .u: LevelTranslator {
        .vcc_peak &= 3.6V
        .a_peak &= 0.55V
        .output_current &= 1mA
    }
    .sensor.vdd ~ .low
    .u.vcc ~ .high
    .sensor.out.line ~ .u.a.line
}
```

```text
error: net `net-sensor.out.line`: `sensor.out.voh` is 275mV..325mV but `u.a.vih` needs 1.1V: a high of 275mV is not always read as a high, which takes 1.1V
   --> main.nsm:16:5
   |
16 |     .sensor: Sensor
   |     ^^^^^^^^^^^^^^^
```

### Logic levels of a bus

Every bus interface but `Usb2` has `.levels`, an interface `std::logic::Levels` with the rail `.reference` and the four voltages `.vih`, `.vil`, `.voh` and `.vol` of a `Logic` (ADR 0064). A datasheet gives one set for the supply of a group of pins, so a part states them once for the interface, in the way it states a `Logic`: `.spi.levels.reference ~ .vdd`, `.spi.levels.vih &= 0.7 * .vdd.voltage`, and the absolute maximum of an input on the pin, `.spi.sclk.abs_max &= 2.1V`. Which lines a part drives and which it reads is what its pin table says (`output` and `tri_state` drive, `input` reads, `bidirectional` does both), so `voh` and `vol` judge the lines it drives and `vih` and `vil` the lines it reads. The checks are those of the previous section, with the same numbers and the same notes: a pair of pins of which neither states a level is not looked at, and one that states them on one side only is a note. A part that has two supplies for two groups of pins states two interfaces.

The reference rail is a reference for the checker, not a load: `Levels` states `.reference.current &= 0A` itself, so a part that joins it to its supply (`.spi.levels.reference ~ .vdd`) does not state a current for it, and the net of that supply keeps no term that nobody can settle. The part's whole draw is `.vdd.current`.

`~` between two bus interfaces joins no levels: they belong to two parts, and the reference rail of each is that part's supply, so a join would be a short between a 3.3 V net and a 1.8 V net.

Three invented parts: a processor on 3.3 V, a sensor with its pins on a 1.8 V supply of their own that survives 2.1 V on an input, and a translator for SPI that is a slave on its side `a` and a master on its side `b`, each side on its own rail.

```nsm module=src::zlevels
import std::bus::{Spi, SpiRole}
import std::sources::Rail

/// Zproc, an invented processor on 3.3 V.
part Zproc {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Master
    // Table 11, DC characteristics of the I/O pins.
    .spi.levels.reference ~ .vdd
    .spi.levels.vih &= 0.7 * .vdd.voltage
    .spi.levels.vil &= 0.3 * .vdd.voltage
    .spi.levels.voh &= .vdd.voltage - 0.4V
    .spi.levels.vol &= 0.4V

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .spi.sclk output "SCK"  "3"
        .spi.mosi output "MOSI" "4"
        .spi.miso input  "MISO" "5"
        .spi.cs   output "CS"   "6"
    }
}

/// Zsense, an invented sensor. The core is on `vdd` and the pins on their own supply `vddio`.
part Zsense {
    .vdd: Rail
    .vddio: Rail
    .vddio.neg ~ .vdd.neg
    .spi: Spi
    .spi.role &= SpiRole::Slave
    // Table 5, the levels of the pins on `vddio`, and table 3, the absolute maximum of an
    // input: 2.1 V.
    .spi.levels.reference ~ .vddio
    .spi.levels.vih &= 0.7 * .vddio.voltage
    .spi.levels.vil &= 0.3 * .vddio.voltage
    .spi.levels.voh &= .vddio.voltage - 0.2V
    .spi.levels.vol &= 0.2V
    .spi.sclk.abs_max &= 2.1V
    .spi.mosi.abs_max &= 2.1V
    .spi.cs.abs_max &= 2.1V

    pins {
        .vdd.pos   power     "VDD"   "8"
        .vddio.pos power     "VDDIO" "5"
        .vdd.neg   power     "GND"   "6" "7"
        .spi.sclk  input     "SCL"   "1"
        .spi.mosi  input     "SDI"   "2"
        // Table 14: SDO is "high impedance while CS is high".
        .spi.miso  tri_state "SDO"   "3"
        .spi.cs    input     "CS"    "4"
    }
}

/// Zshift, an invented SPI level translator: side `a` faces the processor on `vcca`, side `b`
/// faces the sensor on `vccb`.
part Zshift {
    .vcca: Rail
    .vccb: Rail
    .vccb.neg ~ .vcca.neg
    .a: Spi
    .b: Spi
    .a.role &= SpiRole::Slave
    .b.role &= SpiRole::Master
    .a.levels.reference ~ .vcca
    .a.levels.vih &= 0.7 * .vcca.voltage
    .a.levels.vil &= 0.3 * .vcca.voltage
    .a.levels.voh &= .vcca.voltage - 0.2V
    .a.levels.vol &= 0.2V
    .b.levels.reference ~ .vccb
    .b.levels.vih &= 0.7 * .vccb.voltage
    .b.levels.vil &= 0.3 * .vccb.voltage
    .b.levels.voh &= .vccb.voltage - 0.2V
    .b.levels.vol &= 0.2V
    .a.sclk.abs_max &= 3.6V
    .a.mosi.abs_max &= 3.6V
    .a.cs.abs_max &= 3.6V
    .b.miso.abs_max &= 2.1V

    pins {
        .vcca.pos  power     "VCCA" "1"
        .vccb.pos  power     "VCCB" "2"
        .vcca.neg  power     "GND"  "3"
        .a.sclk    input     "A1"   "4"
        .a.mosi    input     "A2"   "5"
        .a.miso    tri_state "A3"   "6"
        .a.cs      input     "A4"   "7"
        .b.sclk    output    "B1"   "8"
        .b.mosi    output    "B2"   "9"
        .b.miso    input     "B3"   "10"
        .b.cs      output    "B4"   "11"
    }
}
```

Both on 3.3 V they would build. Here the sensor's pins are on `vddio`, which is the 1.8 V rail: the processor's 3.3 V lines are refused, once for each input that states a maximum, and the way back, a sensor's 1.69 V high that the processor does not read as a high, is refused with the numbers of both:

```nsm fails
import src::zlevels::{Zproc, Zsense}
import std::sources::Rail

board Mismatch {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .proc: Zproc
    .sense: Zsense
    .s3 ~ .proc.vdd
    .s3 ~ .sense.vdd
    .s18 ~ .sense.vddio
    .proc.spi ~ .sense.spi
}
```

```text
error: net `net-proc.spi.miso`: `sense.spi.levels.voh` is 1.51V..1.69V but `proc.spi.levels.vih` needs 2.194V..2.426V: a high of 1.51V is not always read as a high, which takes 2.426V
   --> main.nsm:16:5
   |
16 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.sclk` reaches 3.465V above its ground `GND` (`proc.spi.sclk`, a line of `proc.spi.levels.reference`) but `sense.spi.sclk.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:16:5
   |
16 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.mosi` reaches 3.465V above its ground `GND` (`proc.spi.mosi`, a line of `proc.spi.levels.reference`) but `sense.spi.mosi.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:16:5
   |
16 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.cs` reaches 3.465V above its ground `GND` (`proc.spi.cs`, a line of `proc.spi.levels.reference`) but `sense.spi.cs.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:16:5
   |
16 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
```

A translator between the two supplies is a part with a bus on each side. The board joins the processor to its `a` side and the sensor to its `b` side, and neither message appears, because each side's levels are judged on its own rail:

```nsm evaluate
import src::zlevels::{Zproc, Zsense, Zshift}
import std::sources::Rail

board Translated {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .proc: Zproc
    .shift: Zshift
    .sense: Zsense
    .s3 ~ .proc.vdd
    .s3 ~ .shift.vcca
    .s18 ~ .shift.vccb
    .s3 ~ .sense.vdd
    .s18 ~ .sense.vddio
    .proc.spi ~ .shift.a
    .shift.b ~ .sense.spi
}
```

A single-supply buffer used as a translator (`LevelTranslator`) states plain pins and thresholds and is judged by its own relations: a bus does not pass through it by role.

#### An open-drain line

An open-drain line, such as the `sda` of an `I2c` bus, is never driven high: a device only pulls it low or lets go, and a resistor to a rail holds it high. A datasheet states its `VOL` and no `VOH`, because the part does not set the high. The levels check reads such a part as it is, without a new word in the language (ADR 0075, provisional): a driver that states `vol` and no `voh`, on a net that a resistor joins to the positive pin of a rail (`I2cPullups`, `PullUp` or a plain `Resistor`), has as its high the rail's voltage range, and the receivers' `vih` is judged against that. A finding names the pull-up resistor, its rail and the receiver's `vih`, and the driver's `vol` is judged against each receiver's `vil` as before. The rule is narrow:

- A driver that states `voh` is judged by it, whatever else is on the net: the datasheet says what the part does.
- A driver that states no `vol` either says nothing about its output, and gets the notes for its high and its low, as before.
- A net with a resistor to its ground as well is a divider, and this rule does not apply. When the DC analysis solves the line (below) its voltage is the high. When it does not, the note says the high has no value.
- A rail on another ground than the receiver's, or a rail with no voltage, is a note that names it, not a pass.

When the DC analysis solves the line, its voltage is the high, and not the rail's (ADR 0076, amendment 8, provisional). The line is solved when every pin on it is modelled: each driver that states `vol at 3mA` (a low with the current it is measured at, as in "DC analysis"), the receivers that draw nothing, and the resistors, diodes and rails between it and a supply. The high is then the voltage of the line with every open-drain pin let go, over the corners of the rails and the resistances: a pull-up through a series resistor and a diode is the rail less the diode's drop, and a pull-up with a pull-down is the divider. The finding says `the DC analysis holds the line at 2.164V..2.327V with every driver of it high impedance ... but ... needs ...`. A line that is not solved (a driver whose `vol` has no test current, a pin that can drive it high, a pin that nothing models) is judged by the rule above, as before. The driver's `vol` is judged as before: the number the datasheet states.

Two invented I2C parts that state `VOL` and the thresholds, and no `VOH`:

```nsm module=src::zi2c
import std::bus::{I2c, I2cRole}
import std::sources::Rail

/// A controller whose pins are open-drain: VOL and the thresholds, no VOH.
part Zihost {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target with the same levels, on its own 3.3 V supply.
part Zitarget {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}
```

On one 3.3 V rail with `I2cPullups`, the build says nothing about the levels of the two lines: each part's high is the rail's, 3.135V to 3.465V, and each reads it above 70 % of its own supply.

```nsm evaluate
import src::zi2c::{Zihost, Zitarget}
import std::bus::I2cPullups
import std::sources::Rail

board Pulled {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .m: Zihost
    .t: Zitarget
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .s3 ~ .m.vdd
    .s3 ~ .t.vdd
    .s3 ~ .pullups.supply
    .m.i2c ~ .t.i2c
    .m.i2c ~ .pullups.bus
}
```

The same bus pulled up to 1.8 V, with parts that read a high from 70 % of 3.3 V, is refused with the numbers: a high of 1.71 V is not a high for them.

```nsm fails
import src::zi2c::{Zihost, Zitarget}
import std::bus::I2cPullups
import std::sources::Rail

board PulledLow {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .m: Zihost
    .t: Zitarget
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .s3 ~ .m.vdd
    .s3 ~ .t.vdd
    .s18 ~ .pullups.supply
    .m.i2c ~ .t.i2c
    .m.i2c ~ .pullups.bus
}
```

```text
error: net `net-m.i2c.scl`: the pull-up `pullups.scl_pullup` to rail `s18` holds the line at 1.71V..1.89V (a driver that states `vol` and no `voh` leaves the high to the pull-up) but `t.i2c.levels.vih` needs 2.194V..2.426V: a high of 1.71V is not always read as a high, which takes 2.426V
   --> main.nsm:17:5
   |
17 |     .t: Zitarget
   |     ^^^^^^^^^^^^
error: net `net-m.i2c.sda`: the pull-up `pullups.sda_pullup` to rail `s18` holds the line at 1.71V..1.89V (a driver that states `vol` and no `voh` leaves the high to the pull-up) but `t.i2c.levels.vih` needs 2.194V..2.426V: a high of 1.71V is not always read as a high, which takes 2.426V
   --> main.nsm:17:5
   |
17 |     .t: Zitarget
   |     ^^^^^^^^^^^^
error: net `net-m.i2c.sda`: the pull-up `pullups.sda_pullup` to rail `s18` holds the line at 1.71V..1.89V (a driver that states `vol` and no `voh` leaves the high to the pull-up) but `m.i2c.levels.vih` needs 2.194V..2.426V: a high of 1.71V is not always read as a high, which takes 2.426V
   --> main.nsm:16:5
   |
16 |     .m: Zihost
   |     ^^^^^^^^^^
```

### Buses

A bus interface names its lines, and its definition in `std::bus` says more: for each role of the bus, which way each line goes (the lines and roles are below). A part states the kind of each pin in its table (`output`, `input`, `bidirectional`, `tri_state`), and the rules of the previous sections judge the nets: two outputs on one net are refused, and so are an `input` with nothing on its net and a net of nothing but inputs. The buses are `std::bus::Uart`, `Spi`, `I2c`, `Swd` and `Usb2`. The names are the device's own.

- A UART's `tx` and `rx` cross, and the language crosses them: `~` between two `Uart` interfaces joins one end's `tx` to the other's `rx` and its `rts` to the other's `cts`. A board that joins pin by pin writes the crossing (`.a.uart.tx ~ .b.uart.rx`), and a `tx` joined to a `tx` is refused. A device without flow control gives no pin to `rts` and `cts`, and the join leaves them out.
- An SPI bus does not cross. The master's `mosi` is an output and the slave's is an input, and the two join with `~`.
- An I2C bus needs a pull-up on each line, and the checker refuses a line without one. `std::bus::I2cPullups` is the pair, with the resistance as a value (`.pullups.resistance &= 4.7kohm +/- 5%`) and a supply rail. The module states the current it draws from that rail (each pull-up carries nothing while its line is released and at most the supply voltage over its resistance while it is held low, the two together on `supply.current`), so a board states no load for it; a current the board does state on `supply` is a budget the sum must fit in (ADR 0038).
- A debug port `Swd` meets a programming connector pin by pin: the connector's pins are passive.
- `Usb2` joins a host and a device with `~`: D+ to D+. `vbus` is a `Rail`. A receptacle between them is a pass-through, not an end (below).

Some invented parts for the examples. A UART has the same pins at both ends, so one part serves for both; SPI has a master and a slave.

```nsm module=src::devices
import std::bus::{I2c, Spi, Swd, Uart, Usb2}
import std::sources::Rail

/// A part with a UART. Illustrative; not a real part.
part Uart32 {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos       power         "VDD"    "1"
        .vdd.neg       power         "VSS"    "2"
        .uart.tx       output        "TX"     "3"
        .uart.rx       input         "RX"     "4"
    }
}

/// An SPI master. Illustrative.
part SpiMaster {
    .vdd: Rail
    .spi: Spi

    pins {
        .vdd.pos       power         "VDD"    "1"
        .vdd.neg       power         "VSS"    "2"
        .spi.sclk      output        "SCK"    "3"
        .spi.mosi      output        "MOSI"   "4"
        .spi.miso      input         "MISO"   "5"
        .spi.cs        output        "CS"     "6"
    }
}

/// An SPI slave. Illustrative.
part SpiSlave {
    .vdd: Rail
    .spi: Spi

    pins {
        .vdd.pos       power         "VDD"    "1"
        .vdd.neg       power         "GND"    "2"
        .spi.sclk      input         "SCLK"   "3"
        .spi.mosi      input         "SDI"    "4"
        .spi.miso      output        "SDO"    "5"
        .spi.cs        input         "CSN"    "6"
    }
}

/// A controller with an I2C port, a debug port and USB. Illustrative.
part Controller {
    .vdd: Rail
    .i2c: I2c
    .swd: Swd
    .usb: Usb2
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.current &= 0A

    pins {
        .vdd.pos       power         "VDD"    "1"
        .vdd.neg       power         "VSS"    "2"
        .i2c.sda       bidirectional "SDA"    "3"
        .i2c.scl       bidirectional "SCL"    "4"
        .swd.swdio     bidirectional "SWDIO"  "5"
        .swd.swclk     input         "SWCLK"  "6"
        .swd.swo       output        "SWO"    "7"
        .swd.reset     input         "NRST"   "8" optional
        .usb.dp        bidirectional "DP"     "9"
        .usb.dm        bidirectional "DM"     "10"
        .usb.vbus.pos  input         "VBUS"   "11" optional
    }
}

/// Anything on an I2C bus. Illustrative.
part Sensor {
    .vdd: Rail
    .i2c: I2c

    pins {
        .vdd.pos       power         "VDD"    "1"
        .vdd.neg       power         "GND"    "2"
        .i2c.sda       bidirectional "SDA"    "3"
        .i2c.scl       bidirectional "SCL"    "4"
    }
}
```

The board that joins them as the buses want:

```nsm evaluate
import src::devices::{Controller, Sensor, SpiMaster, SpiSlave, Uart32}
import std::bus::I2cPullups
import std::parts::connector::{ProgrammingPads6, UsbC2}
import std::sources::Rail

board Buses {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .ctl: Controller
    .sensor: Sensor
    .master: SpiMaster
    .flash: SpiSlave
    .a: Uart32
    .b: Uart32
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .master.vdd
    .supply ~ .flash.vdd
    .supply ~ .a.vdd
    .supply ~ .b.vdd

    // The UART crosses: `~` joins `tx` to `rx` and `rx` to `tx`.
    .a.uart ~ .b.uart
    // SPI joins as it is named.
    .master.spi ~ .flash.spi
    // I2C, with a pull-up on each line.
    .ctl.i2c ~ .sensor.i2c
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .supply ~ .pullups.supply
    .ctl.i2c ~ .pullups.bus

    // A programming connector on the debug port.
    .prog: ProgrammingPads6
    .prog.p1 ~ .supply.pos
    .prog.p2 ~ .ctl.swd.swdio
    .prog.p3 ~ .ctl.swd.reset
    .prog.p4 ~ .ctl.swd.swclk
    .prog.p5 ~ .supply.neg
    .prog.p6 ~ .ctl.swd.swo

    // The USB port.
    .usb: UsbC2 {
        .usb.vbus.voltage &= 5V +/- 5%
    }
    .ctl.usb ~ .usb.usb
}
```

A UART joined with `~` crosses, and builds:

```nsm evaluate
import src::devices::Uart32
import std::sources::Rail

board Straight {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Uart32
    .b: Uart32
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.uart ~ .b.uart
}
```

Joined pin by pin with the same names, a `tx` meets a `tx` and an `rx` an `rx`, and each net is refused once, with the way to write it:

```nsm fails
import src::devices::Uart32
import std::sources::Rail

board SameLine {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Uart32
    .b: Uart32
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.uart.tx ~ .b.uart.tx
    .a.uart.rx ~ .b.uart.rx
}
```

```text
error: net `net-a.uart.rx` joins `rx` to `rx` (`a.uart.rx`, `b.uart.rx`): `rx` crosses `tx` in a `Uart`, so it is joined to the other end's `tx`; write `.a.uart.rx ~ .b.uart.tx`, or join the two interfaces with `.a.uart ~ .b.uart`, which crosses `tx` to `rx` and `rts` to `cts`
   --> main.nsm:10:5
   |
10 |     .a: Uart32
   |     ^^^^^^^^^^
error: net `net-a.uart.tx` joins `tx` to `tx` (`a.uart.tx`, `b.uart.tx`): `tx` crosses `rx` in a `Uart`, so it is joined to the other end's `rx`; write `.a.uart.tx ~ .b.uart.rx`, or join the two interfaces with `.a.uart ~ .b.uart`, which crosses `tx` to `rx` and `rts` to `cts`
   --> main.nsm:10:5
   |
10 |     .a: Uart32
   |     ^^^^^^^^^^
```

A receive line left open:

```nsm fails
import src::devices::Uart32
import std::sources::Rail

board OneWay {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Uart32
    .b: Uart32
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.uart.tx ~ .b.uart.rx
}
```

```text
error: `a.uart.rx` is an input that must be connected (pad 4, `RX`), but nothing else is on its net
   --> main.nsm:10:5
   |
10 |     .a: Uart32
   |     ^^^^^^^^^^
```

Two masters on one SPI bus:

```nsm fails
import src::devices::SpiMaster
import std::sources::Rail

board TwoMasters {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: SpiMaster
    .b: SpiMaster
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.spi ~ .b.spi
}
```

```text
error: net `net-a.spi.cs` has two outputs: `a.spi.cs`, `b.spi.cs`
   --> main.nsm:10:5
   |
10 |     .a: SpiMaster
   |     ^^^^^^^^^^^^^
error: net `net-a.spi.miso` has only inputs (`a.spi.miso`, `b.spi.miso`): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor
   --> main.nsm:10:5
   |
10 |     .a: SpiMaster
   |     ^^^^^^^^^^^^^
error: net `net-a.spi.mosi` has two outputs: `a.spi.mosi`, `b.spi.mosi`
   --> main.nsm:10:5
   |
10 |     .a: SpiMaster
   |     ^^^^^^^^^^^^^
error: net `net-a.spi.sclk` has two outputs: `a.spi.sclk`, `b.spi.sclk`
   --> main.nsm:10:5
   |
10 |     .a: SpiMaster
   |     ^^^^^^^^^^^^^
```

A net whose part pins are all `input` has nothing that drives it, and is refused with the net and the pins (an enable pin wired to another enable pin). An `output`, a `bidirectional` pin, a `power` pin, a `passive` pin (a pull resistor, a connector), a rail the board declares, or a port of the board itself each count as a driver, and so does a pin of a part that has no pin table, which the checker knows nothing about (ADR 0044).

An `I2c` bus (`std::bus::I2c`, by that name) is open-drain, so each of its two lines needs a pull-up: a resistor, or a chain of resistors, from the line to the `pos` of a rail. One resistor on a line is enough for that line, and `I2cPullups` is one on each. A line tied to the rail itself, or pulled down to `neg`, is not a pull-up. Two cases are not judged, because the pull-up may be where this board cannot see it: a bus that is a port of the board, and a line that reaches a connector (a part of several passive pins and no `bridge`), except in a `system` that mates every connector the line reaches (below). A pull-up may be made of any resistors in any arrangement, in series, in parallel or in a divider, or be a `PullUp` or an `I2cPullups`; a part that pulls a line up some other way (a current source, a bus buffer) is not found, and the line is refused (ADR 0090). A bus with no pull-up:

```nsm fails
import src::devices::Sensor
import std::sources::Rail

board NoPullups {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Sensor
    .b: Sensor
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.i2c ~ .b.i2c
}
```

```text
error: net `net-a.i2c.scl` carries the scl line of an I2C bus (`a.i2c.scl`, `b.i2c.scl`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line
   --> main.nsm:10:5
   |
10 |     .a: Sensor
   |     ^^^^^^^^^^
error: net `net-a.i2c.sda` carries the sda line of an I2C bus (`a.i2c.sda`, `b.i2c.sda`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line
   --> main.nsm:10:5
   |
10 |     .a: Sensor
   |     ^^^^^^^^^^
```

In a `system`, a line that goes out through a connector is followed through the mates (ADR 0040) to the board behind it, contact N to contact N, and on through any connector there. The bus is judged as one: it needs a pull-up on the board that has it or on any board it reaches, and a pull-up on either side of a mate satisfies it. If none has one, the error names the connectors the line passed (`carrier.j`, `card.h`) and is reported once for the line. A line that reaches a connector the system does not mate, or a port of a board, goes somewhere this check cannot see and is not judged, as is a board checked alone behind its own connector (ADR 0090, provisional).

### Roles and lines of a bus

A bus interface can say more than the names of its lines. `lines` is a statement of an interface, and it says, for each role of the bus, the kind of each line and a few properties of a line (ADR 0064). `std::bus` declares them for the five buses, so the protocol is written once, in a file that a reader can open:

```nsm-fragment
enum SpiRole { Master, Slave }

interface Spi {
    .role: SpiRole
    .sclk: Pin
    .mosi: Pin
    .miso: Pin
    .cs: Pin

    lines SpiRole::Master limit 1 {
        .sclk output
        .mosi output
        .miso input optional
        .cs   output optional
    }
    lines SpiRole::Slave {
        .sclk input
        .mosi input
        .miso output required
        .cs   input single
    }
}
```

A row is `.line kind` and optional words. `optional` and `required` decide whether the line must be connected, as on a pin row: a part's own word wins, then the role's, then the kind's (an output is optional, an input is required). `limit N` after the role allows N interfaces of that role on one bus. `crosses .other` says the line joins the other end's `.other`, not its own name (a `Uart`'s `.tx output crosses .rx`; the pair is written on one of the two lines). `single` says a net carries this line of one interface of the role: the select of one slave. A bus with one role (`Uart`) writes `lines limit 2 { ... }` with no role. `lines` is a statement of an interface and nothing else.

A part states the role it plays with an ordinary statement, `.spi.role &= SpiRole::Slave`, and its pin table is what its datasheet says. A processor whose unit can be either leaves the role out, and the board states it (`.cpu.spi.role &= SpiRole::Master`), as a board chooses a function. `~` between two interfaces does not equate the roles, as it does not equate a current or a logic level: a master joins a slave. The rules read the nets and need no order of statements. A **bus** is the interfaces of one definition that share a net, and a rule about a net says nothing that the rules about a net of ADR 0044 would repeat.

- **One line on one net.** All the bus pins on a net are one line, or two that cross. A `mosi` on a `miso`, a `tx` on a `tx` and a net of two interfaces are refused, whether or not any part states a role.
- **The limit.** The interfaces of one role on one bus are counted against `limit`: one master, one host, one probe, two ends of a UART.
- **`single`.** A net carries the select of one slave.
- **The table against the role.** A part that states a role has a row for each line the role has, with the kind the role gives it. The same kind agrees, and so do `bidirectional` (a pin that drives and receives contradicts nothing, and the guideline below asks for the honest kind), `passive`, `no_connect`, and a `tri_state` output for an `output`. For a line that the role makes `bidirectional` (I2C), any signal kind agrees.
- **A line the role requires** is connected, for an interface that has any line connected: the board that never uses a bus a part declares is left alone.
- **A part that states no role** is not judged by the table or the limit, an interface that is joined to another end of its bus and states no role is a guideline (below), and a bus on which one part states its role and another does not gets a note, `not evaluated: the role of `x.spi` is not stated, so the limit and the table of `x` are not judged`. The first three rules above are judged for its pins all the same.
- **A pass-through.** A role that an interface has no `lines` for passes the bus through: `UsbRole::Passthrough`, which `std::parts::connector::UsbC2` states, says that a receptacle is not an end of the bus and that the role is the one of the end behind it. The part has no table to judge and counts toward no limit, it satisfies the guideline below, and the note and the guideline name the end behind it that states none, never the receptacle. Two receptacles joined to each other say nothing special. A role that is an end of the bus has `lines`; a variant without them is a pass-through, whatever its name.

Three invented parts: a processor that is an SPI master, a flash that is a slave and writes `tri_state` for its data out (the page is in the comment), and an IMU.

```nsm module=src::zparts
import std::bus::{Spi, SpiRole, Uart}
import std::sources::Rail

/// Zcpu, an invented processor: one SPI master, a second select on a plain pin, and a UART.
part Zcpu {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi
    .uart: Uart
    .spi.role &= SpiRole::Master

    pins {
        .vdd.pos  power         "VDD"  "1"
        .vdd.neg  power         "VSS"  "2"
        .spi.sclk output        "SCK"  "3"
        .spi.mosi output        "MOSI" "4"
        .spi.miso input         "MISO" "5"
        .spi.cs   output        "CS0"  "6"
        .sel1     bidirectional "IO7"  "7"
        .uart.tx  output        "TXD"  "8"
        .uart.rx  input         "RXD"  "9" optional
    }
}

/// Zflash, an invented serial flash on 3.3 V.
part Zflash {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power     "VCC" "8"
        .vdd.neg  power     "GND" "4"
        .spi.sclk input     "CLK" "6"
        .spi.mosi input     "DI"  "5"
        // Page 8, pin description: DO is "high impedance when /CS is high".
        .spi.miso tri_state "DO"  "2"
        .spi.cs   input     "/CS" "1"
    }
}

/// Zimu, an invented IMU.
part Zimu {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power     "VDD" "8"
        .vdd.neg  power     "GND" "6"
        .spi.sclk input     "SCL" "1"
        .spi.mosi input     "SDI" "2"
        // Table 14: SDO is "high impedance while CS is high".
        .spi.miso tri_state "SDO" "3"
        .spi.cs   input     "CS"  "4"
    }
}
```

A board that joins them as the bus wants. The flash joins as a whole interface; the IMU shares `sclk`, `mosi` and `miso` and has the select of its own on a pin of the processor:

```nsm evaluate
import src::zparts::{Zcpu, Zflash, Zimu}
import std::sources::Rail

board Sensors {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .flash: Zflash
    .imu: Zimu
    .supply ~ .cpu.vdd
    .supply ~ .flash.vdd
    .supply ~ .imu.vdd

    .cpu.spi ~ .flash.spi
    .cpu.spi.sclk ~ .imu.spi.sclk
    .cpu.spi.mosi ~ .imu.spi.mosi
    .cpu.spi.miso ~ .imu.spi.miso
    .cpu.sel1 ~ .imu.spi.cs
}
```

`mosi` joined to `miso`, which each slave's `tri_state` and `bidirectional` pins let through the net rules, is found from the line each pin is of:

```nsm fails
import src::zparts::{Zcpu, Zflash, Zimu}
import std::sources::Rail

board Swapped {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .flash: Zflash
    .imu: Zimu
    .supply ~ .cpu.vdd
    .supply ~ .flash.vdd
    .supply ~ .imu.vdd

    .cpu.spi ~ .flash.spi
    .cpu.spi.sclk ~ .imu.spi.sclk
    .cpu.spi.mosi ~ .imu.spi.miso
    .cpu.spi.miso ~ .imu.spi.mosi
    .cpu.sel1 ~ .imu.spi.cs
}
```

```text
error: net `net-cpu.spi.miso` joins two different lines of a `Spi`: `miso` (`cpu.spi.miso`, `flash.spi.miso`) and `mosi` (`imu.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `imu.spi.mosi` is on the net of `miso`
   --> main.nsm:10:5
   |
10 |     .cpu: Zcpu
   |     ^^^^^^^^^^
error: net `net-cpu.spi.mosi` joins two different lines of a `Spi`: `miso` (`imu.spi.miso`) and `mosi` (`cpu.spi.mosi`, `flash.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `imu.spi.miso` is on the net of `mosi`
   --> main.nsm:10:5
   |
10 |     .cpu: Zcpu
   |     ^^^^^^^^^^
```

Two masters on one bus are one message for the pair of interfaces, and the rules of a net say nothing more about the nets it names:

```nsm fails
import src::zparts::{Zcpu, Zflash}
import std::sources::Rail

board TwoMasters {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .host: Zcpu
    .flash: Zflash
    .supply ~ .cpu.vdd
    .supply ~ .host.vdd
    .supply ~ .flash.vdd
    .cpu.spi ~ .flash.spi
    .host.spi ~ .flash.spi
}
```

```text
error: `cpu.spi` and `host.spi` are both `SpiRole::Master` on one `Spi` bus, and `Spi` allows 1 `Master` on a bus: they are on one bus through `net-cpu.spi.cs` (`cpu.spi.cs`, `flash.spi.cs` and `host.spi.cs`); the slave on it is `flash.spi`; give `host.spi` a bus of its own
   --> main.nsm:11:5
   |
11 |     .host: Zcpu
   |     ^^^^^^^^^^^
```

Each slave has a select of its own. The select of the IMU joined to the flash's, which is on the net of the master's `cs`:

```nsm fails
import src::zparts::{Zcpu, Zflash, Zimu}
import std::sources::Rail

board SharedSelect {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .flash: Zflash
    .imu: Zimu
    .supply ~ .cpu.vdd
    .supply ~ .flash.vdd
    .supply ~ .imu.vdd

    .cpu.spi ~ .flash.spi
    .cpu.spi.sclk ~ .imu.spi.sclk
    .cpu.spi.mosi ~ .imu.spi.mosi
    .cpu.spi.miso ~ .imu.spi.miso
    .imu.spi.cs ~ .flash.spi.cs
}
```

```text
error: net `net-cpu.spi.cs` carries the `cs` of two slaves, `flash.spi.cs` and `imu.spi.cs`: `Spi` gives each slave a `cs` of its own (`cs` is `single` for `SpiRole::Slave`); join `.imu.spi.cs` to a pin of its own
   --> main.nsm:11:5
   |
11 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
```

A row that contradicts the role names the datasheet's pin, so that the author reads the page:

```nsm fails
import std::bus::{Spi, SpiRole}
import std::sources::Rail

/// Zimu, an invented IMU, with a mistake in its table.
part Zimu {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power     "VDD" "8"
        .vdd.neg  power     "GND" "6"
        .spi.sclk input     "SCL" "1" optional
        .spi.mosi output    "SDI" "2"
        .spi.miso tri_state "SDO" "3"
        .spi.cs   input     "CS"  "4" optional
    }
}

board Wrong {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .imu: Zimu
    .supply ~ .imu.vdd
}
```

```text
error: `imu.spi.mosi` is an output in the table of `Zimu`, but `SpiRole::Slave` makes `mosi` an input of `Spi`; either the datasheet's `SDI` pin is not an output, or the role is the other one
   --> main.nsm:26:5
   |
26 |     .imu: Zimu
   |     ^^^^^^^^^^
```

A slave's data out left open is refused once the bus is used, and says that the role requires it (an interface with no line connected is left alone):

```nsm fails
import src::zparts::{Zcpu, Zflash}
import std::sources::Rail

board OpenMiso {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .flash: Zflash
    .supply ~ .cpu.vdd
    .supply ~ .flash.vdd
    .cpu.spi.sclk ~ .flash.spi.sclk
    .cpu.spi.mosi ~ .flash.spi.mosi
    .cpu.spi.cs ~ .flash.spi.cs
}
```

```text
error: `flash.spi.miso` is a tri_state output that must be connected (pad 2, `DO`): `SpiRole::Slave` of `Spi` requires it, and nothing else is on its net
   --> main.nsm:11:5
   |
11 |     .flash: Zflash
   |     ^^^^^^^^^^^^^^
```

A part whose table is older than the roles may write every pin `bidirectional`. It is accepted, and a guideline (ADR 0030, `guidelines = "warn"`, an error under `--strict`) asks for the honest kind:

```text
warning: Sensors: bus role guideline: `imu.spi.miso` is `bidirectional` in the table of `Zimu`, and `SpiRole::Slave` makes `miso` a `tri_state` output: write `tri_state`, as the datasheet says, unless the datasheet says the pin also receives
```

The same advice is given for a pin that carries a function of a `peripheral` that is an end of a bus when the function states no kind of its own and the pin is `bidirectional` (every pin of an any-pin processor is): `` `u.p02` is `bidirectional` in the table of `Zmcu`, and its function `SPIM0_MISO` states no kind of its own, and `SpiRole::Master` makes `miso` an input: write `SPIM0_MISO:input`, as the datasheet says, unless the datasheet says the pin also drives ``. A function that states its kind is judged where the peripheral is defined.

An interface that is joined to another end of its bus and states no role is the same guideline, because the table and the limit of a part that states none are not judged (a bus whose interfaces nobody joins, a `Uart`, which has no roles, and a peripheral, which states its role after `as`, are not asked). Every part of the library that has a bus and every example states its role. The datasheet says which it is, and a part that does not know writes the one its pin table implies and says so in a comment:

```text
warning: Sensors: bus role guideline: `old.spi` is joined to another end of a `Spi` bus and states no role, so the limit and the table of `old` are not judged: write `.spi.role &=` `SpiRole::Master` or `SpiRole::Slave`, as its datasheet says
```

An error with `--strict` or `guidelines = "error"`, as every guideline is.

A module that forwards a bus is a way through: its pins are not terminals, and the part inside has the role. The module says the role of its port in the same way a function is forwarded, with an equation:

```nsm
import src::zparts::{Zcpu, Zflash}
import std::bus::Spi
import std::sources::Rail

module FlashCard {
    .supply: Rail
    .bus: Spi
    .chip: Zflash
    .supply ~ .chip.vdd
    .bus ~ .chip.spi
    .bus.role == .chip.spi.role
}

board WithCard {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .card: FlashCard
    .supply ~ .cpu.vdd
    .supply ~ .card.supply
    .cpu.spi ~ .card.bus
}
```

A module that draws a `Uart` port from the other end's side (its `tx` is the line the other end sends on) joins its port to a part's `.uart` with `~`: that join now crosses, and the board's join crosses again, so the two crossings undo each other and the net rules find two outputs. A module that forwards a `Uart` writes its port as a part would (its `tx` is the line the module sends on), joins the part's pins to it one by one with the same names, and lets the board's `~` cross.

A receptacle is a pass-through, not an end of the bus: the `UsbC2` of the library states `UsbRole::Passthrough`, so a board that joins its `.usb` to a processor's port is judged by the processor's role, and is asked for no role of the receptacle. An invented device with a USB port, and the board:

```nsm module=src::usbparts
import std::bus::{Usb2, UsbRole}
import std::sources::Rail

/// Zdongle, an invented USB device that draws its supply from the bus.
part Zdongle {
    .designator &= "U"
    .vdd: Rail
    .usb: Usb2
    .usb.role &= UsbRole::Device
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.current &= 0A

    pins {
        .vdd.pos       power         "VDD"  "1"
        .vdd.neg       power         "VSS"  "2"
        .usb.dp        bidirectional "DP"   "3"
        .usb.dm        bidirectional "DM"   "4"
        .usb.vbus.pos  input         "VBUS" "5" optional
    }
}
```

```nsm evaluate
import src::usbparts::Zdongle
import std::parts::connector::UsbC2
import std::sources::Rail

board Dongle {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .dongle: Zdongle
    .supply ~ .dongle.vdd
    .jack: UsbC2 {
        .usb.vbus.voltage &= 5V +/- 5%
    }
    .dongle.usb ~ .jack.usb
}
```

Had `Zdongle` stated no role, the `bus role` guideline would name `dongle.usb` and not `jack.usb`.

### The clock of a bus

`Spi`, `I2c` and `Swd` have a `.frequency`, and `Uart` has a `.baud`, a value in the unit of a frequency (ADR 0064). A part states what its datasheet allows as a bound (`.spi.frequency <= 10MHz`, with the page in a comment), and the board states the clock it will run (`.cpu.spi.frequency &= 8MHz`). `~` between two interfaces equates the two values, as it does for every value but a current and a logic level, so one value runs along the bus and each part's bound judges it. The firmware sets the clock, so the board's statement is its intention, and the check is that the intention is within every part's limit. A board that states none is left alone.

```nsm module=src::clocked
import std::bus::Spi
import std::sources::Rail

/// Zflash, an invented serial flash.
part Zflash {
    .vdd: Rail
    .spi: Spi
    // Section 7: the clock is at most 33 MHz.
    .spi.frequency <= 33MHz

    pins {
        .vdd.pos  power         "VCC" "8"
        .vdd.neg  power         "GND" "4"
        .spi.sclk bidirectional "CLK" "6"
        .spi.mosi bidirectional "DI"  "5"
        .spi.miso bidirectional "DO"  "2"
        .spi.cs   bidirectional "/CS" "1"
    }
}

/// Zimu, an invented IMU.
part Zimu {
    .vdd: Rail
    .spi: Spi
    // Table 4: the clock is at most 10 MHz.
    .spi.frequency <= 10MHz

    pins {
        .vdd.pos  power         "VDD" "8"
        .vdd.neg  power         "GND" "6"
        .spi.sclk bidirectional "SCL" "1"
        .spi.mosi bidirectional "SDI" "2"
        .spi.miso bidirectional "SDO" "3"
        .spi.cs   bidirectional "CS"  "4"
    }
}
```

A clock above a bound is refused in the words of the bus: the clock it is joined at, the part that does not allow it, its limit, and what the other parts allow, so that the author sees which part sets the speed of the bus:

```nsm fails
import src::clocked::{Zflash, Zimu}
import std::sources::Rail

board TooFast {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .flash: Zflash
    .imu: Zimu
    .supply ~ .flash.vdd
    .supply ~ .imu.vdd
    .flash.spi ~ .imu.spi
    .flash.spi.frequency &= 20MHz
}
```

```text
error: bus `flash.spi` is joined at 20MHz, which `imu` (`Zimu`) does not allow: its limit `imu.spi.frequency` is at most 10MHz; `flash` (`Zflash`) allows 33MHz, so the bus runs at 10MHz at most
   --> main.nsm:11:5
   |
11 |     .imu: Zimu
   |     ^^^^^^^^^^
```

The interfaces that the nets show to be on one bus hold one clock, whether the board joined them with `~` or one pin at a time, so the board states the clock on any one of them. A bus whose parts state limits and whose board states no clock is not a pass: the build says so as a note, `not evaluated: the clock of bus `flash.spi` is not stated, so the limits on it are not checked (...); state the clock the board runs`.

The SPI mode is `.mode`, an enum `SpiMode` (`Mode0` to `Mode3`), and it is equated along the bus like the clock. A part that supports exactly one mode states it (`.spi.mode &= SpiMode::Mode3`, with the page in a comment); a part that supports several states nothing and says which in a comment, because a value is one mode and no list. That refuses the pair that cannot work, a mode 1 part with a mode 3 part, and nothing that is fine:

```nsm fails
import std::bus::{Spi, SpiMode}
import std::sources::Rail

/// Zone, an invented sensor that supports mode 1 only.
part Zone {
    .vdd: Rail
    .spi: Spi
    // Page 9: CPOL = 0, CPHA = 1 only.
    .spi.mode &= SpiMode::Mode1

    pins {
        .vdd.pos  power         "VDD" "8"
        .vdd.neg  power         "GND" "4"
        .spi.sclk bidirectional "SCK" "6"
        .spi.mosi bidirectional "SDI" "5"
        .spi.miso bidirectional "SDO" "2"
        .spi.cs   bidirectional "CS"  "1"
    }
}

/// Zthree, an invented flash that supports mode 3 only.
part Zthree {
    .vdd: Rail
    .spi: Spi
    // Page 7: CPOL = 1, CPHA = 1 only.
    .spi.mode &= SpiMode::Mode3

    pins {
        .vdd.pos  power         "VCC" "8"
        .vdd.neg  power         "GND" "4"
        .spi.sclk bidirectional "CLK" "6"
        .spi.mosi bidirectional "DI"  "5"
        .spi.miso bidirectional "DO"  "2"
        .spi.cs   bidirectional "/CS" "1"
    }
}

board Modes {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Zone
    .b: Zthree
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.spi ~ .b.spi
}
```

```text
error: connected values disagree: `a.spi.mode` is SpiMode::Mode1 but `b.spi.mode` is SpiMode::Mode3
  --> main.nsm:9:5
  |
9 |     .spi.mode &= SpiMode::Mode1
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### A pull-up on one line

A reset, an enable or an open-drain output often needs a resistor from a rail to the line. A plain `Resistor` between the rail's `pos` and the line works for the circuit and leaves the net unsolved: the resistor starts a chain on the rail's net, nothing says what it carries, so the net has two unknowns and every check on it waits (the regulator's current, its power, the resistor's own power). `std::bus::PullUp` is that resistor with its draw stated.

- `.supply` is the rail whose `pos` the resistor is on, `.line` is the pin it pulls, and `.resistance` is the value the board states, with its tolerance (`.pull.resistance &= 10kohm +/- 5%`). A board that forgets it gets the unconstrained pick finding for the resistor, with the member named (see "A value that nobody stated"). The resistor inside is `.resistor`.
- The line is held low for the share `.line_low` of the time, 0 to 1 and never narrower unless the board says so (`.pull.line_low &= 0..0.1`). While it is low the resistor carries `supply.voltage / resistance`, and while it is released nothing, so the resistor's `.current`, and the draw on `supply`, is 0 up to the supply over the resistance. That is the worst case: the lowest the line goes is 0 V, and a driver's real low level draws a little less. It is the steady draw, with two assumptions the module offers and the board may replace (ADR 0070): `.overdrive`, how far above the supply a driver holds the line, 0 V unless stated (`.pull.overdrive &= 0V..1.7V` for a 5 V driver on a 3.3 V pull-up: the draw then reaches `-overdrive / resistance`, current pushed back into the rail), and `.supply_top`, the voltage the draw is worked out at, `supply.voltage` unless stated (`.pull.supply_top &= .pull.supply.peak` for the draw at the transient the rail's stated `.peak` allows).
- A current the board states on `supply` is a budget (ADR 0038): the draw has to be able to fit in it. Two `PullUp`s on one rail add on its net.
- The resistor is a pull-up for the checker's I2C rule too: a `PullUp` on `sda` and one on `scl` satisfy it. `I2cPullups` is the same circuit for both lines of a bus; it is not built from two `PullUp`s, because its members keep the paths that boards and tests name (`.pullups.sda_pullup.current`).

```nsm evaluate
import std::bus::PullUp
import std::sources::Rail

/// A gate with an active-low reset. Illustrative; not a real part.
part ResetGate {
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 100uA..1mA

    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .reset_n input "RST#" "3"
    }
}

board Reset {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .gate: ResetGate
    .supply ~ .gate.vcc

    // The reset line is held high by 10 kohm and is low a tenth of the time at most.
    .pull: PullUp {
        .resistance &= 10kohm +/- 5%
        .line_low &= 0..0.1
    }
    .supply ~ .pull.supply
    .pull.line ~ .gate.reset_n
}
```

A board that forgets the resistance is told so, with the member to state:

```nsm solve warns
import std::bus::PullUp
import std::sources::Rail

board Forgetful {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .pull: PullUp
    .supply ~ .pull.supply
}
```

```text
Forgetful: unconstrained pick guideline: `pull.resistor` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104; it is held to `pull.resistance` (`==` at std/bus.nsm:235), which nothing states: state it, `.pull.resistance &= 4.7kohm +/- 5%`, or say that any value will do, `.pull.resistance &= 10kohm..100kohm`
```

A module that wraps a `PullUp` states the resistance itself or forwards it through a member of its own (`.pull.resistance &= .resistance`, with the wrapped member on the left: the solver works a value out from the right side towards the left). A board that forgets a forwarded member is told about the wrapper's member, which is the one it can state.

### A pull-down on one line

A boot-mode pin, an enable or a gate often needs a resistor from the line to ground, so that it reads low when its driver lets go. A plain `Resistor` from the line to the rail's `neg` works for the circuit and leaves the resistor's current unknown: nothing in the circuit says what the driver puts into it, and its drop and power wait. `std::bus::PullDown` is that resistor with its current stated, the mirror of `PullUp`.

- `.reference` is the rail whose `neg` the resistor is on, `.line` is the pin it holds low, and `.resistance` is the value the board states, with its tolerance (`.pull.resistance &= 100kohm +/- 5%`). A board that forgets it gets the unconstrained pick finding for the resistor, with the member named. The resistor inside is `.resistor`.
- The line is high for the share `.line_high` of the time, 0 to 1 and never narrower unless the board says so (`.pull.line_high &= 0..0.1`). While it is high the resistor carries the line's voltage over its resistance, and while it is low or released nothing, so the resistor's `.current` is 0 up to `.line_top / resistance`. `.line_top`, the highest voltage the line is held at above `reference`'s `neg`, is `reference.voltage` unless the board states it (`.pull.line_top &= 0V..5.25V` for a 5 V driver on a line judged against 3.3 V; ADR 0070's way of offering an assumption).
- Unlike a `PullUp`, it draws nothing from a rail: the current comes from the line's driver, whose supply the module does not know. `.reference` names the ground and the level, it is not a load, and the module states `.reference.current &= 0A`, as `Levels` does, so joining it to the board's rail adds no unknown to that net. The driver's own output current, and the input current of the pins on the line, are not stated by anything yet (issue 192).

```nsm evaluate
import std::bus::PullDown
import std::sources::Rail

/// A gate with an enable input that must read low at power-up. Illustrative; not a real part.
part EnableGate {
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 100uA..1mA

    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .enable input "EN" "3"
    }
}

board Enable {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .gate: EnableGate
    .supply ~ .gate.vcc

    // The enable line is held low by 100 kohm and is high a tenth of the time at most.
    .pull: PullDown {
        .resistance &= 100kohm +/- 5%
        .line_high &= 0..0.1
    }
    .supply ~ .pull.reference
    .pull.line ~ .gate.enable
}
```

A board that forgets the resistance is told so, with the member to state:

```nsm solve warns
import std::bus::PullDown
import std::sources::Rail

board Forgetful {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .pull: PullDown
    .supply ~ .pull.reference
}
```

```text
Forgetful: unconstrained pick guideline: `pull.resistor` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104; it is held to `pull.resistance` (`==` at std/bus.nsm:324), which nothing states: state it, `.pull.resistance &= 4.7kohm +/- 5%`, or say that any value will do, `.pull.resistance &= 10kohm..100kohm`
```

## The standard library

The library is `.nsm` source in `std/`. It states only what is always true of a part. Import a definition by its full path, for example `import std::parts::passive::Resistor`.

Parts are numbered by their letters in the order the design declares them (a module's parts where the module is declared, depth first; see Designators), and every kind states its own letters in its definition (`.designator &= "R"` in `Resistor`): `R` resistor, `C` capacitor, `L` inductor, `FB` ferrite bead, `D` LED, diode, Schottky, Zener and TVS, `Q` MOSFET, `Y` crystal, `F` fuse, `SW` switch, `J` header, socket, coax, USB-C and programming pads, `TP` test point, `H` mounting hole and plated hole, `FID` fiducial, `LBL` label, `U` linear regulator (ADR 0046; see Designators).

### Parts

`Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal`, `Fuse`, `Schottky`, `Zener`, `Tvs`, `NMosfet` and `LinearRegulator` are written `generic`, not `part` (ADR 0068, stage 2): each is its own first provider, so a design uses it as it always did, and a project can add a provider of it (see _The standard passives are generics_). Every other definition in this table is a plain `part`.

| Definition | What it is |
|---|---|
| `std::parts::passive::Resistor` | A resistor with `.resistance`, `.temperature_coefficient`, a through-current and a voltage drop tied by Ohm's law. Passive pins `a` and `b` (pads 1 and 2), bridge `a ~> b`. Requires `.drop * .current <= .max_power`. Catalog `resistor`: resistance, max_power, package, temperature_coefficient (a fraction per kelvin, empty in the standard rows). |
| `std::parts::passive::Capacitor` | A ceramic capacitor with `.capacitance`, `.effective_capacitance`, `.bias`, `.max_voltage` `.count`, the number of identical capacitors in parallel (a bank; unset is one, a range is chosen by the solver) and `.dielectric` (a `Dielectric`, from the row, optional). Pins `a` and `b`, bridge `a ~> b`. Catalog `capacitor`. |
| `std::parts::passive::Led` | A light-emitting diode with `.color`, `.forward_voltage`, `.current` and `.drop`. Pins `anode` and `cathode`, bridge `anode ~> cathode`. Catalog `led`. |
| `std::parts::regulator::LinearRegulator` | A fixed-output linear regulator. Rails `vin` and `vout` that share a ground, a pin table for SOT-23 and one for SOT-223, `.dropout`, `.max_current`, `.max_power`. It requires `vin >= vout + dropout`, a supplied current within `.max_current` and a dissipation within `.max_power`; `.quiescent_current` is the current it keeps for itself (default none), added to the input current. Catalog `ldo`. |
| `std::parts::connector::Header1x2` | A single-row header with two pins `p1` and `p2`. Soldered by hand. |
| `std::parts::connector::Header1x4` | A single-row header with 4 pins `p1` to `p4`, pin N on pad N. |
| `std::parts::connector::Header1x8` | A single-row header with 8 pins `p1` to `p8`, pin N on pad N. |
| `std::parts::connector::Header1x10` | A single-row header with 10 pins `p1` to `p10`, pin N on pad N. |
| `std::parts::connector::Socket1x2` | A single-row socket with 2 positions `p1` to `p2`, the mate of `Header1x2`. |
| `std::parts::connector::Socket1x4` | A single-row socket with 4 positions `p1` to `p4`, the mate of `Header1x4`. |
| `std::parts::connector::Socket1x8` | A single-row socket with 8 positions `p1` to `p8`, the mate of `Header1x8`. |
| `std::parts::connector::Socket1x10` | A single-row socket with 10 positions `p1` to `p10`, the mate of `Header1x10`. |
| `std::parts::connector::Header2x3` | A two-row header with 6 pins `p1` to `p6`, pin N on pad N. |
| `std::parts::connector::Header2x5` | A two-row header with 10 pins `p1` to `p10`, pin N on pad N. |
| `std::parts::connector::Header2x10` | A two-row header with 20 pins `p1` to `p20`, pin N on pad N. |
| `std::parts::connector::Coax` | A coaxial (RF) connector: pins `signal` and `shield`, `.impedance`, `.max_frequency`. |
| `std::parts::connector::UsbCPower` | A USB Type-C receptacle, power only: rail `vbus` (pos on VBUS, neg on GND), pins `cc1`, `cc2`, `shell`. |
| `std::parts::connector::UsbC2` | A USB Type-C receptacle for USB 2.0: the pins `cc1`, `cc2`, `shell`, `sbu1`, `sbu2`, and the port `usb` (`Usb2`, role `UsbRole::Passthrough`). |
| `std::parts::connector::Obd2M` | A male OBD-II (SAE J1962) 16-pin vehicle connector: rail `vbatt` (pos on pin 16, neg on pin 4), optional `signal_gnd`, `can_high`, `can_low`, `iso_k`, `iso_l`, `j1850_bus_pos`, `j1850_bus` and `p1`, `p3`, `p8`, `p9`, `p11`, `p12`, `p13`. Catalog `obd2_m`, with no standard row. |
| `std::parts::connector::ProgrammingPads6` | Six pads `p1` to `p6` for a spring-pin programming cable. The board decides what each carries. |
| `std::parts::passive::Inductor` | An inductor: `.inductance`, `.saturation_current`, `.dc_resistance` and the optional `.rms_current` (a catalog column the standard rows leave empty). Pins `a`, `b`, bridge `a ~> b`. Requires `.current <= .saturation_current`. At DC it is its winding (`dc resistor .a .b .dc_resistance`). Catalog `inductor`. |
| `std::parts::passive::FerriteBead` | A ferrite bead: `.impedance` at `.impedance_frequency`, `.rated_current`, `.dc_resistance`. Pins `a`, `b`, bridge `a ~> b`. Requires `.current <= .rated_current`. Catalog `ferrite_bead`. |
| `std::parts::passive::Crystal` | A two-terminal crystal: `.frequency` (with its tolerance) and `.load_capacitance`. Pins `a`, `b`. Catalog `crystal`. |
| `std::parts::passive::Fuse` | A fuse: `.rated_current`, `.rated_voltage`, `.resistance`. Pins `a`, `b`, bridge `a ~> b`. Requires `.current <= .rated_current` and `.supply_peak <= .rated_voltage` (`.supply_peak` is the `.peak` of the rail its chain starts on, ADR 0086). `.limit_current == .rated_current` is what it lets through to a part behind it (ADR 0091). Catalog `fuse`. |
| `std::parts::passive::Switch` | A single-pole single-throw switch or button: `.rated_current`, `.rated_voltage`. Pins `a`, `b`, bridge `a ~> b`. Requires `.current <= .rated_current`. |
| `std::parts::diode::Diode` | A diode: `.forward_voltage`, `.max_reverse_voltage`, `.max_forward_current`, `.bias`. Pins `anode` and `cathode`, bridge `anode ~> cathode`. Requires `.bias <= .max_reverse_voltage`, `.current <= .max_forward_current` and, for a diode across a rail, the right way round (`.bias >= -.forward_voltage`, from `.bridge_voltage`, ADR 0088); behind a fuse, `.supply_fault_current <= .max_forward_current`, and first in a chain from a rail that states a `.reverse_peak`, `.supply_reverse_peak <= .max_reverse_voltage` (ADR 0091). |
| `std::parts::diode::Schottky` | A Schottky diode, with the fields and ratings of `Diode`. Catalog `schottky`. |
| `std::parts::diode::Zener` | A Zener diode: `.zener_voltage` (with tolerance, at `.test_current`), `.max_power`, `.bias`, and the optional catalog columns `.test_current`, `.temperature_coefficient`, `.leakage_current` and `.leakage_test_voltage`. Pins `anode` and `cathode`, bridge `cathode ~> anode`. Requires `.bias * .current <= .max_power` and `min(.bias) <= max(.zener_voltage)` (a Zener across a rail that surely conducts is an error), and, when the design states `.temperature_shift`, defines `.voltage_over_temperature`. |
| `std::parts::diode::Tvs` | A unidirectional TVS diode: `.standoff_voltage`, `.clamping_voltage`, `.peak_pulse_current`, `.protected_limit`, `.bias` and the optional `.breakdown_voltage` (a catalog column that only the SMDJ36A row of the standard rows fills). Pins `anode` and `cathode`, bridge `cathode ~> anode`. Requires `.bias <= .standoff_voltage` and `.clamping_voltage <= .protected_limit`. |
| `std::parts::diode::TvsBidirectional` | A bidirectional TVS diode, the same either way round: `.standoff_voltage`, `.clamping_voltage`, `.peak_pulse_current`, `.protected_limit`, `.bias` and the optional `.breakdown_voltage`. Pins `a1` and `a2`, bridge `a1 ~> a2`. Requires `.bias <= .standoff_voltage` and `.clamping_voltage <= .protected_limit`. Catalog `tvs_bidirectional`. |
| `std::parts::transistor::NMosfet` | An N-channel MOSFET: pins `gate` (input), `source`, `drain`, bridge `drain ~> source`. Maxima for drain-source voltage, drain current and gate-source voltage; `.gate_threshold`; `.on_resistance` with `.on_resistance_vgs` (an optional catalog column), the gate-source voltage it is guaranteed at. Requires the stresses `.drain_source_voltage` and `.gate_source_voltage` and `.current` within them, the gate drive above the threshold and at least `.on_resistance_vgs` (ADR 0092). Catalog `n_mosfet`. |
| `std::parts::transistor::PMosfet` | A P-channel MOSFET: as `NMosfet` with bridge `source ~> drain`; voltages are magnitudes. A generic with the catalog `p_mosfet`, whose standard file has no rows: a project adds its own. |
| `std::parts::logic::Sn74lvc1g04` | A single inverter, TI SN74LVC1G04 (SCES214AF), for a supply of 3 V to 3.6 V: rail `vcc`, lines `a` (input) and `y` (output) as `Logic` with the levels and absolute maximums of the datasheet, SOT-23-5. No catalog row. |
| `std::parts::regulator::Lt8641` | The LT8641 buck converter IC, Analog Devices (datasheet Rev. C): a `.buck: Buck` with the datasheet's numbers, the frequency from the resistor on RT (`.rt_resistance`), and a pin table for the 18-lead 3 x 4 mm QFN. A keyed catalog `lt8641` (`.mpn`) whose standard file has no row: a board adds its own. Mapped in `std/kicad.toml` to netsmith's own footprint (`netsmith:`, ADR 0087); its symbol is drawn from the pin table (ADR 0097). |
| `std::parts::logic::LevelTranslator` | A single-supply level translator, a buffer whose input takes a lower level than its supply and whose output swings to the supply: rail `vcc`, the `Logic` lines `a` (input) and `y` (output) and the pin `nc`, a pin table for SOT-23-5 and one for SOT-353_SC-70-5. `.vcc_min` and `.vcc_max` bound the supply; the stresses `.vcc_peak`, `.a_peak` and `.output_current` are at most `.vcc_abs_max`, `.a_abs_max` and `.max_output_current`; `.vt_plus_max` and `.vt_minus_min` are the input's `.vih` and `.vil`, and `.y` states no level; `.vcc.current` is 0 up to `.max_supply_current`, stated by the part. A generic with the catalog `level_translator`, whose standard file has no rows: a project adds its own. |
| `std::parts::mechanical::MountingHole` | A drill with no copper. No pins. |
| `std::parts::mechanical::PlatedHole` | A plated mounting hole. One pin `p1`, the plating, that the board joins to a net or leaves open. |
| `std::parts::mechanical::Fiducial` | A fiducial mark. No pins. |
| `std::parts::mechanical::TestPoint` | A test point. One pin `p1`, which must be on a net. |
| `std::parts::mechanical::Label` | Silkscreen text. No pins. `.text` and `.text_height`. |

### Interfaces

| Definition | What it is |
|---|---|
| `std::sources::Rail` | A DC rail: pins `pos` and `neg`, a `.voltage`, a `.current` (the draw), a `.peak` (the highest voltage, transients included), an optional `.reverse_peak` (the highest the wrong way round, as a positive number, ADR 0091) and a `.min_track` (the narrowest track of its nets). |
| `std::parts::regulator::Buck` | What a buck converter IC says about itself: the switching node `sw` (a rail the IC supplies), the feedback pin `fb`, `vref`, `fsw`, the input range, `max_output_current`, `efficiency`, `min_inductance`, the optional `min_inductor_saturation` and `max_inductor_dcr`, and the capacitance asked for on each side. It states `-.sw.current <= .max_output_current`: the switch node supplies what the datasheet allows and no more (ADR 0076, amendment 6, provisional). A concrete IC is a part with a `.buck: Buck` member (ADR 0025). |
| `std::logic::Levels` | The logic levels of all the lines of a bus interface, the member `.levels` of `Uart`, `Spi`, `I2c` and `Swd`: the rail `.reference` and the four voltages `.vih`, `.vil`, `.voh` and `.vol` (ADR 0064). |
| `std::logic::Logic` | A digital line `.line`, the `.reference` rail that defines its levels, and the levels `.vih`, `.vil`, `.voh` and `.vol` a part may state (ADR 0046). |
| `std::bus::Uart` | Pins `tx`, `rx`, `rts`, `cts`, named by the device, and `.baud`. `~` between two joins `tx` to `rx` and `rts` to `cts`; two ends make a bus (`lines limit 2`). |
| `std::bus::Spi` | Pins `sclk`, `mosi`, `miso`, `cs`, a `.role` (`SpiRole`), a `.frequency` and a `.mode` (`SpiMode`). One master on a bus, a `single` select for each slave, a slave's `miso` is required. |
| `std::bus::I2c` | Pins `sda` and `scl`, a `.role` (`I2cRole`) and a `.frequency`. A pull-up on each line. |
| `std::bus::Swd` | Pins `swdio`, `swclk`, `swo`, `reset`, a `.role` (`SwdRole`) and a `.frequency`. One probe on a port. |
| `std::bus::Usb2` | Pins `dp` and `dm`, a rail `vbus` and a `.role` (`UsbRole`). One host and one device on a bus; a receptacle states `UsbRole::Passthrough` and is not an end of it. |
| `std::parts::connector::Mating` | What a connector says about how it meets another (ADR 0040), the member `.mating` of a header or socket: `.kind` (`"pin_header"`), `.gender` (`"header"` or `"socket"`), `.pitch`, `.rows`, `.positions`, `.pin_current` (the rated current of one contact, from the `header` catalog; unset is unrated) and `.keyed` (`"none"`, or `"shrouded"` or `"polarised"`; unset is the same as `"none"`). A system mates two with `~`. |
| `std::mfg::stackup::Stackup` | Layers, thickness, copper weight and, for inner layers, the prepreg and core. |
| `std::mfg::stackup::Plane` | An inner layer that is one sheet of copper on a net. The board names the net. |
| `std::mfg::stackup::Pour` | Copper poured on an outer layer, `.top` or `.bottom` of a process: a `.net` and the most space between stitching vias, `.stitch_pitch`. |
| `std::pcb::Outline` | The board edge, the board's member `.outline`: `.width`, `.height` (ranges allowed) and `.corner_radius`. |
| `std::pcb::Cutout` | A rectangular hole inside the outline: `.x`, `.y`, `.width`, `.height`, `.corner_radius`. |
| `std::pcb::Keepout` | An area of a board, or of a part in the part's own frame, where tracks, vias, copper or parts are not allowed: `.x`, `.y`, `.width`, `.height`, `.layers` and `.tracks`, `.vias`, `.copper`, `.parts` (each `"forbid"`, the default, or `"allow"`); and, provisionally, `.max_height`, the tallest a part may stand there (see _Height limits_), with which the four default to `"allow"`. |

### Modules

| Definition | What it is |
|---|---|
| `std::parts::regulator::FeedbackDivider` | The resistors that set an adjustable output: pins `top`, `fb`, `bottom`, voltages `vref` and `vout`, the `current` it draws, with `vout == vref * (1 + rtop / rbottom)` in the worst case. |
| `std::parts::regulator::BuckStage` | Everything around a buck converter IC: inductor, input and output capacitors and a `FeedbackDivider`, with ports `vin`, `vout` and `buck`, and the values `ripple`, `divider_current`, `voltage_margin`, `input_count` and `output_count` (how many capacitors stand in parallel on each side, 1 to 8, chosen by the solver or stated by the board). Checks the input range, the output current, the ripple and the inductor's saturation, the inductor limits the IC states, the capacitance the IC asks for of each side's bank, and that each capacitor is rated for the rail's voltage and for its `.peak`, each times `.voltage_margin` (1 unless the board states it, ADR 0085). |
| `std::bus::PullUp` | A pull-up resistor from a rail's `pos` to one line: a `supply` (`Rail`), a `line` (`Pin`), a `.resistance` the board states, `.line_low`, the share of the time the line is low (0 to 1), and the offers `.overdrive` (0 V) and `.supply_top` (`supply.voltage`). It states what it draws from `supply`: its resistor carries 0 up to `supply.voltage / resistance` (below 0 when a stated `.overdrive` lets a driver push current back), so the net it is on is solved and the checks after it are evaluated. |
| `std::bus::PullDown` | A pull-down resistor from one line to the `neg` of a rail: a `reference` (`Rail`, a reference and not a load: `reference.current` is 0 A), a `line` (`Pin`), a `.resistance` the board states, `.line_high`, the share of the time the line is high (0 to 1), and the offer `.line_top` (`reference.voltage`, the highest the line is held at). Its resistor carries 0 up to `line_top / resistance`, so its drop and power are evaluated; it draws nothing from a rail. |
| `std::bus::I2cPullups` | The two pull-up resistors of an I2C bus: a `bus` (`I2c`), a `supply` (`Rail`) and a `.resistance` the board states. It states what it draws from `supply`: each line's pull-up carries 0 up to `supply.voltage / resistance` (`.sda_low` and `.scl_low`, 0 to 1, are the share; `.overdrive` and `.supply_top` are offered as for `PullUp`), and `supply.current` is the sum, so the net it is on is solved and the checks after it are evaluated. |

### Enums

| Definition | What it is |
|---|---|
| `std::bus::SpiRole` | The role of a part on an SPI bus: `Master` or `Slave`. The type of `Spi`'s `.role`. |
| `std::bus::I2cRole` | The role of a part on an I2C bus: `Controller` or `Target`. |
| `std::bus::SwdRole` | The role of a part on a debug port: `Probe` or `Target`. |
| `std::bus::SpiMode` | The clock mode of an SPI bus: `Mode0` to `Mode3`. The type of `Spi`'s `.mode`, equated along the bus. |
| `std::bus::UsbRole` | The role of a part on a USB port: `Host` or `Device`, or `Passthrough` for a connector that the bus goes through (no `lines`, no limit, judged from the end behind it). |
| `std::parts::passive::Dielectric` | The class of a ceramic capacitor, `C0G`, `X5R` or `X7R`: the type of `Capacitor`'s `.dielectric` and of the `dielectric` column of the capacitor catalog (ADR 0055). Import it with the part. |
| `std::prelude::AssemblyMode` | Who puts a part on the board: `Fab`, `Hand` or `None`. The type of the built-in `.assembly` of every part and of the `.assembly_mode` a module offers. In scope in every file without an import (ADR 0054). |

### Manufacturing processes

| Definition | What it is |
|---|---|
| `std::mfg::jlcpcb::TwoLayer` | JLCPCB's two-layer board, 1 oz copper. Rules from `.min_track` to `.min_mask_bridge`, `.max_temperature_rise` (the rise the track width check designs to, unstated by the process), `.via_in_pad` (the largest via the fabricator fills and caps so that it may stand in a pad: an upper bound, `0mm` refuses vias in pads), `.stackup`, and the outer pours `.top` and `.bottom`. Thickness 0.4 mm to 2 mm, chosen by the board. |
| `std::mfg::jlcpcb::FourLayer` | JLCPCB's four-layer board, 1 oz outer and 0.5 oz inner copper, 1.6 mm. Planes `.inner1` and `.inner2`, each on a net the board names. |
| `std::mfg::jlcpcb::VCutPanel` | JLCPCB's V-cut panel, the member `.fab` of a `panel`: `.columns`, `.rows` and `.rail`, and the limits of the board house (panel size, lines, line spacing, copper clearance, thickness, tooling holes and fiducials), each with its source. |

Every number in a process cites its source in a doc comment in `std/mfg/jlcpcb.nsm`.
