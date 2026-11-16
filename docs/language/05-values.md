## Values: units, ranges and tolerances

Every value is a quantity with a dimension, and it is always a range in the worst case. A single number is a range with equal ends.

Literals.

| Form | Meaning |
|---|---|
| `3.3V`, `10kohm`, `100nF`, `5mA` | A number and a unit, with an SI prefix. |
| `1.8V..2.2V` | A range, low end first. |
| `1.6mm..`, `..5mm` | A range with an open end: no upper limit, or no lower limit (below). |
| `3.3V +/- 5%` | A value and a relative tolerance: 3.135 V to 3.465 V. |
| `100ohm +/- 5ohm` | A value and an absolute tolerance, in the value's unit. |
| `20ppm` | A relative fraction, like `%`. |
| `-40degC..125degC` | A temperature in degrees Celsius (ADR 0103): `0degC` is 273.15 K. A range of them is a range of temperatures. |
| `"VCC"` | Text. |

Units: `m`, `g`, `s`, `A`, `K`, `degC`, `V`, `ohm` (or `Ω`), `W`, `F`, `H`, `Hz`, `S`, `C` (the coulomb, a charge: a Celsius temperature is `degC`, never `C`). Prefixes: `f p n u (or µ) m k M G T`. Lowercase `m` is milli and uppercase `M` is mega, so `1mohm` is a milliohm and `1Mohm` is a megohm. Arithmetic uses `+ - * /` and parentheses. A sum or difference needs the same dimension. A product or quotient makes a new one.

```nsm solve
import std::parts::passive::{Resistor, Capacitor}
import std::sources::Rail

board Units {
    .rail: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .r1: Resistor { .resistance &= 4.7kohm +/- 1% }
    .r2: Resistor { .resistance &= 100ohm +/- 5ohm }
    .r3: Resistor { .resistance &= 900ohm..1.1kohm }
    .c1: Capacitor { .capacitance &= 100nF +/- 20% }
    .rail.pos ~> .r1 ~> .r2 ~> .r3 ~> .c1 ~> .rail.neg
}
```

**Temperatures.** `K` is kelvin and `degC` is degrees Celsius (ADR 0103). `degC` is the one unit whose zero is not the zero of its dimension, so it is converted where the literal is read: `0degC` is 273.15 K, `25degC` is 298.15 K, and `-40degC..125degC` is the range 233.15 K to 398.15 K. A sign written directly on a literal belongs to the number (`-40degC` is minus forty degrees Celsius, 233.15 K, and not minus 313.15 K). Every value is kelvin underneath, so a temperature in either spelling is the same value, a message shows the kelvin and the Celsius figure together, and a field of a `Temperature` takes both. `degC` takes no prefix (`mdegC` is an error), and a difference of temperatures, such as a rise or a tolerance, is written in kelvin: `10degC + 5K` is a temperature, 15degC (288.15 K); `20degC - 10degC` is a difference, 10 K; `85degC +/- 5K` is 80degC to 90degC, and `85degC +/- 5degC` is refused. A quantity does not remember whether it is a temperature or a difference, so the sum of two Celsius literals (`10degC + 20degC`, which is 576.3 K) is not refused: it is a number nobody means, and the language does not say it is wrong.

A tolerance in `degC` is the one misuse that the language refuses:

```nsm fails
import std::parts::passive::Led

board Tolerance {
    .led: Led {}
    .led.temp_max &= 85degC +/- 5degC
}
```

```text
error: a tolerance is a difference of temperatures: write it in kelvin (`20degC +/- 5K`), not in degC
  --> main.nsm:5:22
  |
5 |     .led.temp_max &= 85degC +/- 5degC
  |                      ^^^^^^^^^^^^^^^^
```

Rules.

- A tolerance needs fixed numbers on both sides: `.x &= .y +/- 1%` is an error, and so is a tolerance on a range with an open end. A range may have a value at an end (see _A range whose end is a value_) or an open end (see _A range with an open end_).
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

### A range with an open end

A limit with only one side is a range with an open end: `lo..` is every value from `lo` up, with no upper limit, and `..hi` is every value up to `hi`, with no lower limit. A manufacturing process states each rule as `0.15mm..`, the least the factory can make; a design that must stay under a height states `..5mm`. An open end is not a very large number: nothing is written for it and nothing is shown for it (`1.6mm..`), so no value is too large for it, and a message prints the range as it was written.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Bare {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_track &= 0.15mm..
    }
    .header: Header1x2
}
```

The statements read an open end as they read a closed one:

- `&=` narrows: `0.1mm..` narrowed by `0.15mm..` is `0.15mm..`, by `..1mm` is `0.1mm..1mm`, and by `..0.05mm` is a contradiction that names the field and prints both ranges.
- A requirement is met only by a value that is inside it whatever the open end is: a value that is `5mA..` is not always at most a rating of 25 mA, and is always at least 5 mA.
- Arithmetic keeps the end open (`(1V..) + 2V` is `3V..`, `6V - (1V..)` is `..5V`), and zero times an open end is zero. A divisor whose range includes zero is still an error, so `1V / ..1A` is refused.
- `min(.x)` of `1.6mm..` is 1.6 mm. `max(.x)` and `nominal(.x)` of it are errors, since an open end is not a value; a part's range is read at the end that it has.
- A catalog cell has both ends, and `1.6mm..` in one is refused.
- A place that reads a number at the open end says so and names the field: an `environment` temperature needs its coldest and hottest, and a `polygon` corner is one value.

A line that ends at `..` leaves the end open, so the next line is a new statement. To wrap a range before its high end, put it in parentheses, `(0A..` newline `.max_supply_current)`; inside them a newline is a space.

```nsm fails
import std::parts::passive::Resistor

board Open {
    .r1: Resistor { .resistance &= (100ohm..) +/- 5% }
}
```

```text
error: cannot give a tolerance to a range with an open end (`lo..` or `..hi`)
  --> main.nsm:4:36
  |
4 |     .r1: Resistor { .resistance &= (100ohm..) +/- 5% }
  |                                    ^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board TooFine {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_track &= ..0.05mm
    }
    .header: Header1x2
}
```

```text
error: no TwoLayer provider satisfies `process` at main.nsm:5: none of the 2 in scope works
  JlcTwoLayer1oz (the closest):
    contradiction on `process.min_track`: it is already 100um.., which does not overlap ..50um
  JlcTwoLayer2oz:
    contradiction on `process.min_track`: it is already 160um.., which does not overlap ..50um
  --> main.nsm:5:5
  |
5 |     .process: TwoLayer {
  |     ^^^^^^^^^^^^^^^^^^
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
        .current &= -500nA..0A
    }
    .gate: Gate { .max_supply_current &= 750nA }
    .supply ~ .gate.vcc
}
```

```text
error: the currents on net `3V3` do not add up: `supply.current` must stay within -500nA..0A, but the others on it work out to -750nA..0A (all of them: `gate.vcc.current` is 0A..750nA, `supply.current` is -500nA..0A)
   --> main.nsm:19:9
   |
19 |         .current &= -500nA..0A
   |         ^^^^^^^^^^^^^^^^^^^^^^
```
