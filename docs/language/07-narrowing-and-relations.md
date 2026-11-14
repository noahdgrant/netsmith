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
    }
    .b: Rail {
        .voltage &= 4.7V..5.5V
        .pos.net &= "B"
    }
    min(.a.voltage) <= max(.b.voltage)
}
```

Rules.

- The argument is a field, written as a member path (`min(.bias)`, `max(.z.zener_voltage)`). It is not an expression (`min(.a + .b)`), a value (`max(5V)`) or text, and `min` and `max` take one argument: `min(.a, .b)` is not the smaller of two values, it is an error. The only functions are `min` and `max`; any other name is an error.
- They are read in a `<=` or `>=` statement, on either side and inside arithmetic (`min(.a.voltage) + 1V <= max(.b.voltage)`). A `==`, `&=` or `?=` is refused them: nothing is worked out from the end of a range, and no field takes a value from one.
- They never narrow. The solver reads the end of the field's range as it is when the check is made and gives nothing back: the ranges of the fields in a relation with `min` or `max` are the ranges they had. A relation of a part also decides which catalog rows are offered, as every relation of a part does (a row that fails it is not picked); that is a check on each candidate, not a narrowing. A relation that reads an end also gives no bound to another field (`.c.max_voltage >= max(.v)` does not make the capacitor's rating at least what `.v` reaches when the part is chosen: a range narrows after it was read, and the end moves) and is never used to turn away a group of rows without trying them (a row's own figure can meet it where the group's edge does not). It is verified on the values the fields have in the end.
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
    }
    .b: Rail {
        .voltage &= 4.7V..5.5V
        .pos.net &= "B"
    }
    min(.a.voltage) <= max(.b.voltage)
}
```

```text
error: min(a.voltage) must be at most max(b.voltage), but 12V is above 5.5V: no value of the two ranges meets it, so change one of them
   --> main.nsm:12:5
   |
12 |     min(.a.voltage) <= max(.b.voltage)
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
error: `abs` is not a function; the functions are `min(.field)` and `max(.field)`, the least and the greatest value of a field's range, and `nominal(.field)`, its centre
  --> main.nsm:8:5
  |
8 |     abs(.a.voltage) <= 5V
  |     ^^^
```

### The nominal value

A datasheet gives some limits for the value a part is meant to be built with, not for the worst case of its tolerance: a charger's programming resistor is "2 kohm to 67 kohm", and a 2 kohm resistor at 1% is 1.98 kohm to 2.02 kohm, so `.rprog >= 2kohm` (for every value) refuses it. `nominal(.f)` is the value the field is stated at, without its tolerance, as one number of the field's own kind, and `nominal(.rprog) >= 2kohm` compares that: the 2 kohm resistor meets it, and one of 1.8 kohm does not. It follows `min` and `max` (ADR 0098) in everything but the point it reads, and the worst-case rule is the same for every other statement: `.rprog >= 2kohm` still reads 1.98 kohm.

What the language can say is the centre of the range. A range is stored as its two ends and nothing more, so `nominal(.f)` is their mean: `2kohm +/- 1%` is 2 kohm, `3.3V +/- 5%` is 3.3 V, a value is itself, and a range with no stated centre, `1kohm..3kohm`, is 2 kohm. The mean is rounded to 12 significant digits, so that the ends of `3.3V +/- 5%` (3.135 V and 3.465 V, not exact in a float) average to 3.3 V and `nominal(.v) >= 3.3V` holds. A field that is a range because a design narrowed it, or because it is worked out from other fields (`.f == .a / .b`), has the centre of the range it has when the check is made, which is not always the value anyone wrote: a quotient's range is not centred on the quotient of the centres.

```nsm solve
import std::sources::Rail

board Nominal {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "A"
    }
    nominal(.supply.voltage) >= 3.3V
    nominal(.supply.voltage) <= 3.4V
}
```

Rules.

- The rules of the ends apply to it unchanged. The argument is one field, written as a member path; `nominal` is read in a `<=` or `>=` statement, on either side and inside arithmetic, and a `==`, `&=` or `?=` is refused it; it never narrows, a field with no value yet makes the relation wait (listed as not evaluated), and a relation that reads it gives no bound to another field and never turns away a group of catalog rows without trying them (the centre of a group's range is not the value of any row in it).
- A failed relation names the call: `nominal(supply.voltage) must be at least 3.4V, but 3.3V is below 3.4V: it compares the value a field is stated at, without its tolerance, so change the value or the bound`.

A nominal below the bound, and a plain bound on the same field:

```nsm fails
import std::sources::Rail

board Nominal {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "A"
    }
    nominal(.supply.voltage) >= 3.4V
    .supply.voltage >= 3.3V
}
```

```text
error: nominal(supply.voltage) must be at least 3.4V, but 3.3V is below 3.4V: it compares the value a field is stated at, without its tolerance, so change the value or the bound
  --> main.nsm:8:5
  |
8 |     nominal(.supply.voltage) >= 3.4V
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: supply.voltage must always be at least 3.3V, but 3.135V..3.465V is not always above 3.3V
  --> main.nsm:9:5
  |
9 |     .supply.voltage >= 3.3V
  |     ^^^^^^^^^^^^^^^^^^^^^^^
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
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Dim {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .lamp: Lamp
    .input ~ .lamp.supply
}

board Bright {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
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

### Defaults for every part of a kind

A board that wants one package for all its resistors says it once, in a `defaults` block (issue #625, ADR 0128). Each line names a kind, one of its values and `?=`, and offers the value to every part or module of that kind on the board, at any depth inside its modules, that states nothing on the value itself.

```nsm solve report
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with the resistor that sets its current.
module Lamp {
    .supply: Rail
    .led: Led { .current &= 2mA..3mA }
    .r1: Resistor
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Panel {
    defaults {
        Resistor.package ?= "0402"
    }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .lamp: Lamp
    .input ~ .lamp.supply
    .pull: Resistor {
        .resistance &= 10kohm +/- 5%
        .package &= "0603"
    }
    .input.pos ~> .pull ~> .input.neg
}
```

```text
board Panel
  D1   Led             XL-1608SURC-06  LED_SMD:LED_0603_1608Metric                                C965799 Extended
  R1   Resistor        1.2k            Resistor_SMD:R_0402_1005Metric                             C25862 Extended
  R2   Resistor        10k             Resistor_SMD:R_0603_1608Metric                             C25804 Basic
  net GND: D1.1, R2.2
  net VCC: R1.1, R2.1
  net net-lamp.led.anode: D1.2, R1.2
```

The lamp's resistor is in 0402, from the line. `.pull` states its own package, so the line does not reach it.

Rules.

- A line is `Kind.value ?= v`. The kind is a part, a module or a generic in scope; a generic reaches every part that provides it, a part's name reaches that part only, and a module's name reaches the module's own value (`FeedbackDivider.package`). A bank is its own kind (ADR 0123): `CapacitorBank.package` reaches a bank, whose package is that of each capacitor in it, and `Capacitor.package` reaches each `Capacitor`, the members of `3 x Capacitor` among them. The value is one the kind declares, not a pin, a part inside it or a built-in member such as `.assembly`.
- Anything a statement says of the value replaces the line: a `&=` on it anywhere (the board, a module around it, the part), a `?=` on it (the part's own default, a module's, a member's on the board), and a `&=` or `?=` on a value that `==` or a join holds equal to it. A `FeedbackDivider` whose `.package` the board states keeps its two resistors to it, and a line for `Resistor` does not reach them. Where nothing states it, the line is the value, as a `?=` that applied is: it narrows the candidates, and the checks of a package (its pin tables and its catalog) name the line once.
- The value is a number with a unit, a range, `x +/- y%`, text or a variant, of the type the value holds. A value read from a field is refused: the line is about every part of the kind, and a field of one of them is not.
- One kind and value has one line. Two lines that reach one part (`Resistor.package` and `Thin.package` for a `part Thin provides Resistor`) are an error at the second line: state that part's package with `&=`.
- A line that reaches nothing, because the board has no part of the kind or every one states the value itself, is a note and not an error.
- Only a board holds `defaults`, in its source and not in its physical file. A project-wide setting in `netsmith.toml` was not taken: a board says what its parts are. The editor's card for a part says which statement gave its package: a line of `defaults`, a `?=` or a `&=`, with its file and line.

Misuse: a kind that is not in scope.

```nsm fails
import std::parts::passive::Resistor

board Typo {
    defaults {
        Resistr.package ?= "0402"
    }
    .r1: Resistor
}
```

```text
error: no part, module or generic `Resistr` is in scope; did you mean `Resistor`?
  --> main.nsm:5:9
  |
5 |         Resistr.package ?= "0402"
  |         ^^^^^^^
```

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
- a check that the pick decides: a bound, or an equation or `&=` whose left side has a range of its own, that names a value worked out from the pick (an LED's stated current, which the relation of its `~>` chain ties to the resistor: see "The current of a chain");
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

Some numbers are on a datasheet only sometimes: an IC may name the largest DC resistance of the inductor it works with, or leave it out. A check against such a number is made when the IC states it and means nothing when it does not. A member says so with the word `optional` after its type (ADR 0074), the word that a `pins` row and a catalog column already use for "may be absent":

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

A datasheet states some limits for a function whether or not a design uses it: a microcontroller's datasheet limits the voltage on its USB VBUS pin to 4.35 V to 5.5 V, and a board that has no USB port leaves the pin open. The part states the limit once and it is true for every board, so it stays in the part (libraries state only what is always true). A board that never connects the port was left with two notes it could do nothing about: `not evaluated: processor.usb.vbus.voltage >= 4.35V` (and its twin for 5.5 V), because nothing gives the pin a voltage, and `processor.usb.vbus.pos has no source: nothing supplies it` in the power view. The board says that nothing on it uses the interface, with a statement of its own (ADR 0083):

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

/// A linear regulator that makes a rail from another. Illustrative; not a real part.
part Regulator {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    dc linear_regulator .vin .vout

    pins {
        .vin.pos  power "IN"  "1"
        .vin.neg  power "GND" "2"
        .vout.pos power "OUT" "3"
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
error: `processor.usb.dm` is connected here to `jack.usb.dm` (net `net-jack.usb.dm`), but `processor.usb` is stated `unused` at main.nsm:14: the interface is either used or not; remove the connection, or remove `unused`
   --> main.nsm:13:5
   |
13 |     .processor.usb ~ .jack.usb
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
error: `processor.usb.vbus.pos` is connected here through `fuse` (a `Fuse`) to `jack.usb.vbus.pos` (net `net-fuse.b`), but `processor.usb` is stated `unused` at main.nsm:16: the interface is either used or not; remove the connection, or remove `unused`
   --> main.nsm:15:5
   |
15 |     .processor.usb.vbus.pos ~> .fuse ~> .jack.usb.vbus.pos
   |     ^^^^^^^^^^^^^^^^^^^^^^^
```

A pull to a rail that nothing on the board sources is no use. A series part whose far end is the return `neg`, or the `pos` of a rail of the board that nothing on the board sources (the supply, which comes from outside the board), only pulls the pin up or down: a pull-up or a pull-down on a pin of the interface does not make the pin used, although the supply's net has pins of other parts that are not passive. A rail that something on the board sources is not the supply, and a path to it is a use, as it was before: the rail is made or fed on the board, so the interface's pin feeds the board. A rail is sourced on the board when its `pos` net holds the output rail of a `dc linear_regulator` or `dc switching_regulator`, the rail of a `dc source`, or a pin whose table says it is an `output` (a charger's). A path from the pin to another part's pin that is not passive is still a use, also when the same pin has a pull as well. Here `.processor.usb.dp` has a pull-up to the supply and nothing else, and the board builds:

```nsm evaluate
import src::processor::{Processor, Receptacle}
import std::parts::passive::Resistor
import std::sources::Rail

board Pulled {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .processor: Processor
    .jack: Receptacle
    .pull_up: Resistor
    .supply ~ .processor.vdd
    .jack.usb.vbus.neg ~ .supply.neg
    .processor.usb.dp ~> .pull_up ~> .supply.pos
    unused .processor.usb
}
```
The same pull to a rail that a regulator on the board makes is a use: `.vbus` is the regulator's output, so the pin feeds the board.

```nsm fails
import src::processor::{Processor, Receptacle, Regulator}
import std::parts::passive::Resistor
import std::sources::Rail

board Fed {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "5V"
    }
    .vbus: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .regulator: Regulator
    .processor: Processor
    .jack: Receptacle
    .pull_up: Resistor
    .supply ~ .regulator.vin
    .vbus ~ .regulator.vout
    .vbus ~ .processor.vdd
    .jack.usb.vbus.neg ~ .supply.neg
    .processor.usb.dp ~> .pull_up ~> .vbus.pos
    unused .processor.usb
}
```

```text
error: `processor.usb.dp` is connected here through `pull_up` (a `Resistor`) to `regulator.vout.pos` (net `3V3`), but `processor.usb` is stated `unused` at main.nsm:23: the interface is either used or not; remove the connection, or remove `unused`
   --> main.nsm:22:5
   |
22 |     .processor.usb.dp ~> .pull_up ~> .vbus.pos
   |     ^^^^^^^^^^^^^^^^^
```


A series part that leads only to test pads, to a rail nothing on the board sources (a pull-up or a pull-down proves nothing), or to another interface that is stated unused, brings no use. A rail whose source is outside the board is the board's supply, and only a pull to it proves nothing; a `dc source` that a connector states (`UsbCPower` and `UsbCSinkReceptacle` state `dc source .vbus`, `Obd2M` `dc source .vbatt`) counts as a source on the board, so a pull-up from an unused interface to a supply joined to such a connector is an error: the interface is used. Only a `Resistor`, a `Fuse` and a `FerriteBead` are series parts for this rule; a capacitor, an inductor, a diode or a switch in series is not followed (it is not yet decided whether one is a use).

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
    }
    .r1: Resistor { .resistance &= 10ohm +/- 1% }
    .load: Rail {
        .pos.net &= "VLOAD"
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
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
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
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop

    .divider: Divider { .max_draw &= 40uA }
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
    .lamp: Rail {
        .pos.net &= "AFTER"
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
    }
    .series: Resistor { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop

    .divider: Divider { .max_draw &= 40uA }
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
    .lamp: Rail {
        .pos.net &= "AFTER"
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
    }
    .r1: Resistor { .resistance &= 10ohm +/- 1% }
    .load: Rail {
        .pos.net &= "VLOAD"
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
