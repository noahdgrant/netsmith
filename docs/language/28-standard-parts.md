## Standard parts

The library has the parts a two-sided four-layer board is made of, and the interfaces they talk over (ADR 0023). They state what a datasheet says. Each rating is a field that the part leaves open and a relation that is always true of it: a design narrows the field with `&=`, as it narrows a resistance, and the solver checks the relation in the worst case. A rating the design never states is listed as not evaluated, and is no error. Most of them have a small catalog of real parts (ADR 0025): a design that states no rating gets the first row that works, and a rating it does state must be a range that contains the row's (`.zener_voltage &= 4.7V..5.5V` takes a row that reads `4.8V..5.4V`). The guide's examples state what the board needs and let the catalog supply the rest. `build` stops at the export of any of them that has no KiCad symbol yet (see Parts), so the examples here are checked and solved and not built (the guide marks them `evaluate`).

### Voltage across a part

A diode, a Zener, a TVS diode and a capacitor have a `.bias`: the DC voltage across them. A part placed between a rail's `pos` and `neg` takes it from that rail (ADR 0013), so a rating on the voltage across the part needs no line of its own. A part across no rail has no bias until the design states `.bias`.

A capacitor is the same either way round, so its bias is the rail's voltage and the sign is not looked at. A diode is not: its `.bias` is the voltage in the direction it blocks, the cathode relative to the anode, and the checker gives a diode across a rail the rail's voltage with the sign of the way it is wired (ADR 0088). It does so through `.bridge_voltage`, a field of `Diode`, `Schottky`, `Zener` and `Tvs`: the voltage from the first pin of the part's `bridge` to the second (the anode relative to the cathode for a `Diode` or a `Schottky`, the cathode relative to the anode for a `Zener` or a `Tvs`, whose bridge is the way they conduct when they clamp), `+` the rail's voltage when the first pin is on the rail's `pos` and its negative when it is on the `neg`. A diode with its cathode on `pos` blocks the rail and has the rail's voltage as its bias. A diode wired the other way round conducts across the rail, so its bias is negative and the part's own relation names it: a `Diode` or a `Schottky` must have `.bias >= -.forward_voltage` (it never has more than a forward drop the wrong way), and a `Zener` or a `Tvs` must have `.bias >= 0V`, the way round it is a clamp. `.bridge_voltage` is an `optional` member (ADR 0074): a diode across no rail has none, and the relations that read it are neither checked nor listed as not evaluated. A design does not write it; the ratings of the table below follow from it.

| Part | What is always true | What the design states |
|---|---|---|
| `std::parts::diode::Diode`, `std::parts::diode::Schottky` | `.bias <= .max_reverse_voltage`, `.current <= .max_forward_current`, `.bias >= -.forward_voltage`, `.supply_fault_current <= .max_forward_current`, `.supply_reverse_peak <= .max_reverse_voltage` | The ratings; the last two are from the chain (ADR 0091). |
| `std::parts::diode::Zener` | `.bias * .current <= .max_power`, `.bias >= 0V`, `min(.bias) <= max(.zener_voltage)` | `.zener_voltage` with its tolerance. For a clamp, the line `.z.bias <= .z.zener_voltage`; over a temperature range, `.temperature_shift` (below). Catalog `zener`. |
| `std::parts::diode::Tvs` | `.bias <= .standoff_voltage`, `.clamping_voltage <= .protected_limit`, `.bias >= 0V`, `-.supply_reverse_peak >= 0V` | The ratings (the last from the chain, ADR 0091), and `.protected_limit`: the absolute maximum of what is behind it. Catalog `tvs`. |
| `std::parts::diode::TvsBidirectional` | `.bias <= .standoff_voltage`, `.clamping_voltage <= .protected_limit` | The ratings, and `.protected_limit`. Catalog `tvs_bidirectional`. |

A `Tvs` also has an optional `.breakdown_voltage`, the minimum breakdown voltage VBR: between the stand-off voltage and it the diode leaks a little, below it the diode does not conduct (ADR 0073). A design that needs a margin between the highest the line reaches and the point where the diode starts to conduct states it, `.tvs.breakdown_voltage >= .input.peak * 1.1`. It is a catalog column that only one standard row fills, the Littelfuse SMDJ36A (`data/jlcpcb/README.md`), so a design that states it is offered that row and no other standard one, and gets any other from a project's `tvs` catalog file that records the column; a file written before the column may leave it out, and a design that does not state the field picks as before and has no new check.

A Zener directly across a rail is refused when it surely conducts (issue #293, ADR 0098, proposed): it states `min(.bias) <= max(.zener_voltage)`, so a rail whose least voltage is above the Zener's greatest breakdown voltage puts the part in breakdown all the time, with nothing to limit its current. The message names the part, the rail's least voltage and the greatest breakdown (`min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V`), and the way out is a Zener that breaks down at the rail's voltage or a resistor in series. The rule catches only the sure case: a rail range that straddles the breakdown (4 V to 6 V against 4.7 V to 5.5 V) may conduct, and passes. A Zener behind a series resistor has no rail across it, so its `.bias` has no value, and the rule waits and is listed as not evaluated. The rule also chooses among the catalog rows, as any relation of a part does: a design that states a breakdown the rail is above gets the error, but one that states none is given a row whose breakdown reaches the rail (a 12 V rail across a Zener with no `.zener_voltage` is given the BZT52C12), not an error.

```nsm fails
import std::parts::diode::Zener
import std::sources::Rail

board Conducts {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
    }
    .z: Zener { .zener_voltage &= 3.1V..3.5V }
    .input.pos ~> .z ~> .input.neg
}
```

```text
error: no `zener` candidate satisfies `z`: the closest of the 1 that fit, BZT52C3V3 3.3V 500mW Zener SOD-123 C2112, fails because min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V: no value of the two ranges meets it, so change one of them
    --> std/parts/diode.nsm:297:5
    |
297 |     min(.bias) <= max(.zener_voltage)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V: no value of the two ranges meets it, so change one of them
    --> std/parts/diode.nsm:297:5
    |
297 |     min(.bias) <= max(.zener_voltage)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A Zener has the figures a design needs to bound a clamp over temperature (issue #199; the first four are optional catalog columns, ADR 0078, which the standard `zener` rows fill from the datasheet table that each row's notes name):

| Member | What it is |
|---|---|
| `.test_current` | The current the datasheet publishes `.zener_voltage` at (IZT). The row's `.zener_voltage` is the minimum to maximum at this current. |
| `.temperature_coefficient` | The change of `.zener_voltage` for one kelvin, as the datasheet's range. A `Voltage`, because no kind holds volts per kelvin: `18.4mV..22mV` is "18.4 to 22 mV/C". The datasheet's figure is a typical one, so the margin is the design's. |
| `.leakage_current`, `.leakage_test_voltage` | The reverse current below the breakdown, and the reverse voltage it is stated at. |
| `.temperature_shift` | How far the temperature goes from the 25 C the datasheet states, in kelvin: `-65K..100K` for -40 C to 125 C. A board that states an environment has it derived (below); a design may state it, which overrides the derivation. It is an `optional` member (ADR 0074). |
| `.voltage_over_temperature` | `.zener_voltage + .temperature_coefficient * .temperature_shift`, worst case: the upper end of the range is the voltage at the hot corner and the lower end the voltage at the cold one. |

A column that a row leaves empty is no obstacle to a design that does not mention it, and a design that names one (`.z.leakage_current <= 1uA`) is offered only the rows that state it. A design that states no `.temperature_shift` has no `.voltage_over_temperature`: nothing is checked, nothing is listed for the relation, and the row's coefficient is not asked for, so every board written before the columns picks as it did. A design that states the shift is offered only the rows that state the coefficient, so a row without it is never picked with a wrong number. A bound on `.voltage_over_temperature` with no shift stated and no environment is listed as not evaluated.

A board that states an environment (`.environment.temperature`, _The environment of a board_) does not need to state the shift: the checker derives it as the environment's temperature less 25 C, so `-40degC..125degC` is `-65K..100K`, and `.voltage_over_temperature` is the row's coefficient carried over that range, the hot corner at the environment's hot end and the cold corner at its cold end. The standard rows state the datasheet's typical coefficient as its minimum to maximum, so the derived band is the typical value times the range, with the margin the design's. A `.temperature_shift` the design states overrides the derivation, and an environment that does not hold 25 C (`50degC..80degC` is `25K..55K`) gives the band over its own range. The shift is derived only for a board that bounds `.voltage_over_temperature` in a relation of its own: the shift asks every row for its coefficient, so a board that never bounds it keeps the rows that state none (a project's rows with an empty `temperature_coefficient`), as it did without an environment. A board that bounds it and has a row with no coefficient is not offered that row, and the message names the column, as for a stated shift.

```nsm evaluate
import std::env::Environment
import std::parts::diode::Zener
import std::parts::passive::Resistor
import std::sources::Rail

board PinClamp {
    .environment: Environment { .temperature &= -40degC..125degC }
    .input: Rail {
        .voltage &= 24V +/- 10%
        .pos.net &= "VIN"
    }
    .r: Resistor { .resistance &= 2.2kohm +/- 1% }
    .z: Zener { .zener_voltage &= 13V..16V }
    .z.bias == .z.voltage_over_temperature
    .z.current == (.input.voltage - .z.voltage_over_temperature) / .r.resistance
    .z.voltage_over_temperature <= 18V
    .input.pos ~> .r ~> .z ~> .input.neg
}
```

A supply pin held under its absolute maximum by a series resistor and a Zener. The board's own numbers: a 24 V input, a pin that stands 18 V, and a clamp that must hold from -40 C to 125 C. The picked row is BZT52C15 (13.8 V to 15.6 V at 5 mA, 9.2 to 13.0 mV/C). Its hot corner is 15.6 V + 13.0 mV * 100 = 16.9 V, under the 18 V; its cold corner is 13.8 V + 13.0 mV * (-65) = 12.955 V (a message prints it as `12.95V`). The Zener's voltage is that whole range, so the current through it and its power are checked at the worst end of it.

```nsm evaluate
import std::parts::diode::Zener
import std::parts::passive::Resistor
import std::sources::Rail

board PinClamp {
    .input: Rail {
        .voltage &= 24V +/- 10%
        .pos.net &= "VIN"
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
   --> main.nsm:17:5
   |
17 |     .z.voltage_over_temperature <= 10V
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
    }
    .load: Rail {
        .current &= 100mA..400mA
        .pos.net &= "VFUSED"
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
    }
    .tvs: Tvs { .protected_limit &= 14V }
    .input.pos ~> .tvs ~> .input.neg
}
```

```text
error: no `tvs` candidate satisfies `tvs`: the closest of the 10 that fit, SMAJ12A 12V TVS SMA C113957, fails because tvs.bias must always be at most tvs.standoff_voltage, but 11.4V..12.6V is not always below 12V
    --> std/parts/diode.nsm:415:5
    |
415 |     .bias <= .standoff_voltage
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
    }
    .tvs: Tvs {}
    .tvs.breakdown_voltage >= .input.peak * 3
    .input.pos ~> .tvs ~> .input.neg
}
```

```text
error: no `tvs` candidate satisfies `tvs`: the closest of the 1 that fit, SMDJ36A 36V TVS SMC C108363, fails because tvs.breakdown_voltage must always be at least (input.peak * 3), but 40V is not always above 48V (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 9 for `breakdown_voltage`)
   --> main.nsm:11:5
   |
11 |     .tvs.breakdown_voltage >= .input.peak * 3
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A `TvsBidirectional` is the TVS diode for a line that swings both ways or may be wired backwards, such as a power input on a vehicle (ADR 0089). It is two junctions back to back, so it has no anode and no cathode: its pins are `a1` and `a2` (KiCad's `Device:D_TVS`, drawn with two opposed triangles, where `Tvs` is drawn with the Zener symbol), and it is the same either way round. Its fields and checks are those of `Tvs`, read for either direction: the rail across it must stay under the stand-off, and the clamp under `.protected_limit`. Like a capacitor it takes the rail's voltage as its `.bias` whichever pin is on `pos`, so there is no wrong way to wire it. The standard `tvs_bidirectional` catalog has two Littelfuse rows, the SMDJ12CA and the SMDJ36CA (SMC), each with its minimum breakdown voltage from the datasheet table. The 12 V rail below reaches 12.6 V, so the picker passes over the SMDJ12CA (12 V stand-off) and chooses the SMDJ36CA, whichever way the diode is drawn into the chain:

```nsm evaluate
import std::parts::diode::TvsBidirectional
import std::sources::Rail

board Input {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
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
   --> main.nsm:10:5
   |
10 |     .z.bias <= .z.zener_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: d.bias must always be at most d.max_reverse_voltage, but 11.4V..12.6V is not always below 10V
   --> std/parts/diode.nsm:97:5
   |
97 |     .bias <= .max_reverse_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: z.bias must always be at most z.zener_voltage, but 11.4V..12.6V is not always below 11V..13V
   --> main.nsm:10:5
   |
10 |     .z.bias <= .z.zener_voltage
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
    --> std/parts/diode.nsm:296:5
    |
296 |     .bias >= 0V
    |     ^^^^^^^^^^^
error: d.bias must always be at least -d.forward_voltage, but -12.6V..-11.4V is not always above -700mV
   --> std/parts/diode.nsm:98:5
   |
98 |     .bias >= -.forward_voltage
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
error: z.bias must always be at least 0V, but -12.6V..-11.4V is not always above 0V
    --> std/parts/diode.nsm:296:5
    |
296 |     .bias >= 0V
    |     ^^^^^^^^^^^
```

### Resistor power

A `Resistor` has a power rating, `.max_power`, and states that the power it dissipates, `.drop * .current`, is at most that rating, in the worst case over the ranges of its current and its resistance. The rating is a column of the `resistor` catalog (JLCPCB lists it for every part: 62.5 mW for the 0402 parts, 100 mW for the 0603 and 250 mW for the KOA 0805 parts), so the solver picks a resistor that is big enough as well as one of the right value. A design that needs more says so with the rating or the package: `.r.max_power >= 250mW`.

The worst case is over the current the circuit really carries. `.led.current &= 5mA..15mA` is a window the LED must stay in, and the relation of the `~>` chain, `.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance` (see "The current of a chain"), says what the current is for the resistor that was tried: once the resistor is known, the current is the equation's range (7.05 mA to 10.56 mA for 330 ohm), and the resistor is checked at that, not at the 15 mA that ends the window. A window that is not tied to the circuit by an equation (`.current &= 45mA..49mA` on a resistor alone) is still taken at its upper end.

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
   --> std/parts/passive.nsm:68:5
   |
68 |     .drop * .current <= .max_power
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

An adjustable regulator holds its feedback pin at a reference voltage `vref`, and a divider from the output to the pin sets the output: `vout = vref * (1 + rtop / rbottom)`. `std::parts::regulator::FeedbackDivider` is that divider as a module (ADR 0025). Its interface is three pins, `top` (joined to the output's positive pin), `fb` (the regulator's feedback pin) and `bottom` (the output's ground), and three values: `vref`, which the board states with the datasheet's tolerance, `vout`, the output it wants, and `current`, what it draws from the output, which a board bounds to fix the size of the two resistors. The solver picks `rtop` and `rbottom` from the resistor catalog so that, with the tolerance of the reference and of both resistors, the output stays inside the wanted range in the worst case. It evaluates the formula tightly (each resistance appears in the ratio once and the reference once), so the tolerance a design has to allow is the real one.

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
    }
    .divider.top ~ .out.pos
    .divider.bottom ~ .out.neg
    .out.voltage == .divider.vout
}
```

```text
error: no combination of candidates satisfies `divider.rbottom` and `divider.rtop` together: the closest, divider.rbottom = 20kΩ 1% 0402 C25765 and divider.rtop = 56kΩ 1% 0603 C23206, fails because `divider.vout` must stay within 3.762V..3.838V, but (divider.vref * (1 + (divider.rtop.resistance / divider.rbottom.resistance))) works out to 3.707V..3.896V
    --> std/parts/regulator.nsm:108:5
    |
108 |     .vout == .vref * (1 + .rtop.resistance / .rbottom.resistance)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

An output below the reference cannot be made, because the divider can only raise it: `.vout &= 0.9V +/- 1%` with a 1.0 V reference is refused in the same way.

A board that needs the divider's resistors in one size states it on the divider, `.package &= "0603"`: both resistors are then picked in that package. A board that states nothing leaves the pick to the solver, as above.

```nsm solve
import std::parts::regulator::FeedbackDivider
import std::sources::Rail

board SizedDivider {
    .divider: FeedbackDivider {
        .vref &= 1.0V +/- 1%
        .vout &= 3.8V +/- 3%
        .current <= 1mA
        .package &= "0603"
    }
    .out: Rail {
        .pos.net &= "VOUT"
    }
    .divider.top ~ .out.pos
    .divider.bottom ~ .out.neg
    .out.voltage == .divider.vout
}
```

### Switching regulators

A buck converter is an IC, an inductor, two capacitors and a feedback divider. The IC is the one part that differs, so the library splits the work (ADR 0025, ADR 0115). `std::parts::regulator::BuckIc` is the generic of what a datasheet says: the switching node `sw` (a rail the IC supplies, joined to the inductor), the feedback pin `fb`, the reference `vref`, the switching frequency `fsw`, the input range, the largest output current, the efficiency as a range, the smallest inductance, optionally the least saturation current (`min_inductor_saturation`) and the largest DC resistance (`max_inductor_dcr`) an inductor may have, optionally the minimum on-time and off-time of the switch (`min_on_time`, `min_off_time`, ADR 0093), optionally the time the output takes to rise (`soft_start_time`, below), optionally the current its datasheet asks the feedback divider to carry (`recommended_divider_current`, below), and the capacitance asked for on each side. `std::parts::regulator::Buck`, written `Buck<Ic: BuckIc>`, is a module that takes the IC as a type parameter (_A module that takes a part_) and holds it with everything else: the inductor and the two capacitors, picked from the catalogs (each capacitor rated for the `.peak` of its rail when the board states one and always for the rail's steady voltage, so a peak stated too low cannot weaken it; each side a bank of identical capacitors in parallel when one is not enough), and a `FeedbackDivider` that sets the output from the reference.

A concrete IC is a `part` that provides `BuckIc` and writes its pin table, its datasheet numbers, one relation, the input current from the output current and the efficiency (`vin = vout * iout / efficiency` over the whole efficiency range, so it reaches the supply as any load does, ADR 0007 and ADR 0014), and what it is at DC, `dc switching_regulator .vin .sw .efficiency` (and, for an IC whose datasheet gives its own current with nothing loaded, a fourth operand, `.quiescent_current`, ADR 0096; see "What a part is at DC" and "DC analysis": it lets the solver work out the input current through the parts in front of the IC, and the input voltage they leave it). This one is invented for this guide:

```nsm module=src::zbk2
import std::parts::regulator::BuckIc

/// A 2 A buck converter for 4.5 V to 6 V. Illustrative; not a real part.
part Zbk2 provides BuckIc {
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
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency

    pins {
        //  pin          kind    name   pads
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .sw.pos        power   "SW"   "4" "5"
        .fb            input   "FB"   "6"
        .en            input   "EN"   "7"
    }

    support {
        .en ~ .vin.pos
    }
}
```

The board names the IC as the argument of `Buck`, states the input and the output it wants, and joins the supply to the converter's input and its output to the load:

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> { .vout.voltage &= 1.8V +/- 3% }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The solver picks the inductor (a 2.2 uH 1210 part saturating at 3.6 A, over the IC's 3 A, with 85 mohm, under its 0.1 ohm, and a tolerance that still leaves 1.76 uH, over the IC's 1.5 uH), a capacitor for each side that keeps the effective capacitance the IC asks for at the voltage across it, and the two divider resistors. The supply draws 0 to 459 mA for the 0 to 1 A load. The ripple current is `(vin - vout) * vout / (vin * L * fsw)`, 0.30 A to 0.67 A here, and the inductor must carry the load plus half of it: 1.33 A of 3.6 A.

Each side of the converter is a `CapacitorBank` of identical capacitors (the section on capacitors above): `.input_cap` and `.output_cap` stand for one capacitor of the bank, and their `.count` says how many. The solver chooses the row and the count together, from 1 to 8, so that the bank's `total_effective_capacitance` meets what the IC asks for (the capacitance the datasheet states is for the whole side) and each capacitor is rated for its rail. With the IC of this guide changed to ask for 47 uF at the output and 40 uF at the input, the converter takes two capacitors at the 1.8 V output (the 47 uF row keeps 26 uF of its value there) and four at the 5 V input (it keeps 10 uF). The board states the count when it wants one (`.reg.output_cap.count &= 3`, which the checks then hold to), or limits what the solver may choose (`.reg.output_cap.count &= 1..3`), by the path into the converter (ADR 0116). A bank of one is a single capacitor, as the converter had before.

```nsm-fragment
.reg: Buck<Zbk2> {
    .vout.voltage &= 1.8V +/- 3%
    .output_cap.count &= 3
}
```

A margin on the capacitors' ratings is the converter's `.voltage_margin` (ADR 0085): each capacitor is rated for at least the margin times the highest voltage of its rail, `.input_cap.max_voltage >= .vin.voltage * .voltage_margin` and `>= .vin.peak * .voltage_margin`, and the same on the output. It is 1 unless the board states it, which asks for no more than the rail's highest voltage and changes nothing for a board that never mentions it. A board that derates its ceramic capacitors states it from outside the converter. A narrowing could reach the capacitors (ADR 0116), but only with a constant (`.reg.input_cap.max_voltage &= 16V..100V`), and the margin is a relation to the rail's voltage, so it stays a member. A module that wraps the converter states its own member for it (`.reg.voltage_margin &= .margin`). `Buck` has no bootstrap or bypass capacitor, since those are in the IC's own `support` section, which states their ratings itself.

```nsm-fragment
.reg: Buck<Zbk2> {
    .vout.voltage &= 1.8V +/- 3%
    .voltage_margin &= 1.25
}
```

The 5 V input of this guide peaks at 5.25 V, so 1.25 times it is 6.56 V and a 6.3 V capacitor, which the converter picks by default, is no longer enough: the input takes a row rated for at least that. A margin no row of the catalog can give is refused with the numbers of the closest row, and only the rating fails: with a margin of 5 the input capacitor would have to be rated for 26.25 V, and no row that keeps the capacitance the IC asks for has more than 25 V.

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board TooMuchMargin {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> {
        .vout.voltage &= 1.8V +/- 3%
        .voltage_margin &= 5
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: TooMuchMargin: L1: part `Inductor` in package `1210` needs KiCad's footprint `Inductor_SMD:L_1210_3225Metric` (std/kicad.toml line 297).
footprint `Inductor_SMD:L_1210_3225Metric` not found: looked for /tmp/netsmith-sample-libraries-442f93da51a2cbe0/footprints/Inductor_SMD.pretty/L_1210_3225Metric.kicad_mod; set NETSMITH_KICAD_FOOTPRINTS to the folder that holds the .pretty libraries, or list the library in kicad.toml
KiCad is installed at /tmp/netsmith-sample-libraries-442f93da51a2cbe0/footprints but has no `Inductor_SMD:L_1210_3225Metric`: check the spelling, or use a project library (a `[[library]]` of kicad.toml) or a netsmith generated footprint: give that row a `gen:` footprint, for a package a generator makes (`footprint = "gen:SOT23(pins=3)"`).
```

The output fixes only the ratio of the divider's two resistors, so the converter bounds the divider's current, which fixes their size: by default (`?=`) it is the IC's `.recommended_divider_current`, and that is 10 uA to 1 mA by default for an IC whose datasheet gives no figure. The floor keeps the feedback pin's input current (tens of nanoamps) small beside the divider's, and the ceiling keeps the divider from wasting far more than the IC draws for itself. An IC whose datasheet gives a range states it (the `Lmr16006y`'s bottom resistor of 1 kohm to 100 kohm is `.recommended_divider_current &= 7.65uA..765uA`, its 0.765 V reference over each). A board that wants another bound narrows the divider's current by its path (ADR 0116), which replaces the default and the IC's figure with it:

```nsm-fragment
.reg: Buck<Zbk2> {
    .vout.voltage &= 1.8V +/- 3%
    .divider.current &= 50uA..200uA
}
```

A line of a board's `defaults` block for `FeedbackDivider.current` does not reach a converter's divider: the converter's own `?=` beats it (ADR 0128), and the line is a note that it reached nothing. A `FeedbackDivider` used alone has no default, and a board bounds its current itself, as in _Adjustable outputs_. The bound and its defaults were decided by the orchestrating agent while the owner was away (2026-11-13, issue #702); `std::parts::regulator` has the reasons and the datasheets' figures.

The two inductor limits are optional, as are the other numbers an IC may leave open: an IC that states `.min_inductor_saturation` (a datasheet that asks for more than the current limit, say) or `.max_inductor_dcr` has them checked against the picked inductor's `saturation_current` and `dc_resistance`, and an inductor that does not meet them is refused with the numbers. Both are `optional` members of `BuckIc` (ADR 0074): one that the IC leaves unstated has no such check, and the build says nothing about it, where it lists a check on any other value that nothing states as not evaluated. A limit is checked when the IC states it. A heating (RMS) rating of the inductor is not checked by the converter: `Inductor` has the optional column `rms_current`, which the standard rows leave empty (see Dielectric and package size), so a check that needed it would refuse every standard inductor.

Misuse: a load larger than the IC can give is refused twice, once on the converter's output and once on the IC's switch node (`BuckIc` states `-.sw.current <= .max_output_current`, ADR 0076, amendment 6, which is also the limit that encloses a load nothing states before the picks); the lower end of the current is the divider's, which the DC analysis of the converter's output side gives. An input above its range, such as 12 V, is refused the same way, by `reg.vin.voltage must always be at most reg.ic.max_input_voltage`.

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board Overload {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..3A
    }
    .reg: Buck<Zbk2> { .vout.voltage &= 1.8V +/- 3% }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: -reg.ic.sw.current must always be at most reg.ic.max_output_current, but 15.11uA..3A is not always below 2A (the currents on net `net-reg.ic.sw.pos` in `reg`: reg.ic.sw.current (-3A..-15.11uA) + reg.inductor.current (15.11uA..3A) add up to zero; net `1V8` in `reg`: -reg.inductor.current (15.11uA..3A) + -reg.vout.current (-3A..-15.11uA) add up to zero, but the balance is incomplete: `reg.divider.rtop.a` is not modelled; net `1V8`: output.current (0A..3A) + reg.vout.current (-3A..-15.11uA) add up to zero)
    --> std/parts/regulator.nsm:199:5
    |
199 |     -.sw.current <= .max_output_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: -reg.vout.current must always be at most reg.ic.max_output_current, but 15.11uA..3A is not always below 2A (the currents on net `1V8`: output.current (0A..3A) + reg.vout.current (-3A..-15.11uA) add up to zero; net `1V8` in `reg`: -reg.inductor.current (15.11uA..3A) + -reg.vout.current (-3A..-15.11uA) add up to zero, but the balance is incomplete: `reg.divider.rtop.a` is not modelled; net `net-reg.ic.sw.pos` in `reg`: reg.ic.sw.current (-3A..-15.11uA) + reg.inductor.current (15.11uA..3A) add up to zero)
    --> std/parts/regulator.nsm:547:5
    |
547 |     -.vout.current <= .ic.max_output_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

An IC cannot switch for less than a minimum on-time, so at a frequency the output cannot be made from an input that is too high, and a minimum off-time sets how close the input may come to the output (ADR 0093). `BuckIc` has both as `optional` members, `.min_on_time` and `.min_off_time` (ADR 0074), which a part states from its datasheet at the largest figure given. The converter has the two times it asks of the IC, `.on_time = vout / (vin * fsw)` and `.off_time = (vin - vout) / (vin * fsw)`, in the worst case the ranges give (the lowest output at the highest input and the highest frequency), and checks them against the IC's. An IC that states neither has no such check and the build says nothing. The checks are on the ideal duty cycle, so they are never looser than the real one, which is a little larger because of the switch drops; an IC with an equation of its own (the `Lt8641`'s Equations 4 and 5) writes that instead. Past either limit a real IC still regulates, at a lower frequency, so the ripple that the converter computes at the programmed one is not the board's. An invented IC for 4.5 V to 60 V that needs 150 ns of on-time, on a board that makes 5 V from an input that reaches 35 V at 1 MHz:

```nsm module=src::zbk7
import std::parts::regulator::BuckIc

/// A 2 A buck converter for 4.5 V to 60 V that needs 150 ns of on-time. Illustrative; not a real part.
part Zbk7 provides BuckIc {
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
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency

    pins {
        //  pin          kind    name   pads
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .sw.pos        power   "SW"   "4" "5"
        .fb            input   "FB"   "6"
        .en            input   "EN"   "7"
    }

    support {
        .en ~ .vin.pos
    }
}
```

```nsm fails
import src::zbk7::Zbk7
import std::parts::regulator::Buck
import std::sources::Rail

board HighInput {
    .input: Rail {
        .voltage &= 12V..35V
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "5V"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk7> { .vout.voltage &= 5V +/- 6% }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: reg.on_time must always be at least reg.ic.min_on_time, but 122ns..490.8ns is not always above 150ns
    --> std/parts/regulator.nsm:558:5
    |
558 |     .on_time >= .ic.min_on_time
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The input of a 12 V board that stays under 16 V holds it (4.7 V over 16 V at 1.1 MHz is 267 ns); a datasheet that gives no figure leaves the check out. A linear regulator's dropout is checked the same way by `LinearRegulator` and by a part that states `.vin.voltage >= .vout.voltage + .dropout` (the section on `LinearRegulator`).

The ripple of the output voltage is a closed-form check, not a simulation (`docs/research/closed-form-checks.md`, check 3; the converter has no time in it, only the charge balance of a capacitor over one period). `Buck` has `.output_ripple`, the sum of the capacitor's term and the ESR's, `dI / (8 * fsw * C) + dI * ESR / n`, with `dI` the inductor's ripple current above, `C` the output bank's effective capacitance (`.output_cap.total_effective_capacitance`), `n` its count (`.output_cap.count`) and `ESR` the converter's `.output_esr`. A board bounds it (`.output_ripple <= 20mV`) and the bound holds in the worst case of every range: the highest input, the lowest inductance, frequency and capacitance (each with its tolerance, the capacitance at its DC bias) and the highest ESR. Each term moves one way with each of them, so the range is exact. The ESR is the board's own figure, since the standard capacitor catalog has no ESR column (a column needs the maker's number with its conditions as the recorded source, `data/jlcpcb/README.md`): `.output_esr` is `optional` (ADR 0074), and the board states the datasheet's maximum for the capacitor it means to fit, at the switching frequency, the bias and the temperature of the board; a range gives the worst end. The solver picks the capacitor without checking its own ESR against the figure. The member and where the board states it (on the converter, or on the capacitor as a catalog column) are provisional, the owner's to confirm.

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board Quiet {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> {
        .vout.voltage &= 1.8V +/- 3%
        .output_esr &= 5mohm
        .output_ripple <= 20mV
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The check assumes the converter switches at its programmed frequency every period (the on-time and off-time checks above hold it), that the load draws a steady current so the capacitors take only the ripple, that the bank's capacitors share the ripple current equally, and that the ESR is the one stated. It is wrong, and optimistic, where the converter skips pulses or runs in burst mode at light load (the ripple is larger and the frequency lower, as an LT8641 does in Burst Mode), where the capacitors' inductance puts spikes on the output at the switch edges (there is none in the formula), for a constant-on-time or hysteretic converter, whose ripple the ESR sets by design, and for a load that steps or a loop that rings. Adding the two terms is the conservative sum: in a ceramic capacitor they are 90 degrees apart. A limit the worst case exceeds is refused through the capacitor search, because the capacitor's capacitance is in the ripple: the message names the closest candidate and the first limit it breaks. Here is a limit no part can meet, 0.5 mV with one capacitor of 100 mohm (an electrolytic's, say): the ESR's drop alone is 100 mohm times the ripple current, 67 mV at the worst. The closest candidate is named for the limit it misses by least, which here is the capacitance the IC asks for; when the ripple is the limit that is nearest, the message is `stage.output_ripple must always be at most 9mV, but 2.558mV..9.536mV is not always below 9mV` (a board of the tests of `crates/netsmith-solve/tests/buck_ripple.rs`, whose numbers are worked out by hand there).

```nsm fails
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board TooQuiet {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> {
        .vout.voltage &= 1.8V +/- 3%
        .output_cap.count &= 1
        .output_esr &= 100mohm
        .output_ripple <= 0.5mV
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: no `capacitor` candidate satisfies `reg.output_cap`: the closest of the 34 that fit, 1 x 10uF 20% 10V X5R 0402 C315248, fails because reg.output_cap.total_effective_capacitance must always be at least reg.ic.min_output_capacitance, but 4.908uF..7.686uF is not always above 10uF (`reg.output_cap.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
    --> std/parts/regulator.nsm:565:5
    |
565 |     .output_cap.total_effective_capacitance >= .ic.min_output_capacitance
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A board that states a limit and no ESR is not judged: the relation that reads `.output_esr` is skipped (ADR 0074), so `.output_ripple` has no value and the limit is listed as not evaluated, never passed. The capacitor's term alone would pass a limit that the ESR's would break, so it is not used. A board that states neither has no ripple check and the build says nothing about it.

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board NoEsr {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> {
        .vout.voltage &= 1.8V +/- 3%
        .output_ripple <= 20mV
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The start-up time is a closed-form check against a requirement (`docs/research/closed-form-checks.md`, check 4; no simulation: the figure is the datasheet's). `BuckIc` has an `optional` member `.soft_start_time` (ADR 0074), how long the output takes to rise to its regulation point at the IC's own ramp, as its datasheet gives it, a time or the span between its minimum and maximum, with the page in the part's doc comment. `Buck` forwards it as `.soft_start_time`, and a board states its requirement as a bound on that member, as it does for any other (`.soft_start_time <= 5ms`, or `>=` for a ramp slow enough to keep the inrush down). The bound holds over the whole span the IC gives. A board that bounds nothing has no check, and the build says nothing about it. An IC whose datasheet gives no time states nothing, and a bound on it is listed as not evaluated, never passed. An IC whose ramp a capacitor on a soft-start pin sets, and whose datasheet gives the pin's current and no time (the `Lt8641`'s, p.10, p.19), states a time only for a capacitor the board states: see the soft-start paragraph of the `Lt8641` below. The figure is the IC's ramp: a rise that a current limit stretches, because of a large output capacitance or a load that draws while the output rises, is longer than it. This IC, invented for this guide, ramps in 2 ms to 4 ms:

```nsm module=src::zbk4
import std::parts::regulator::BuckIc

/// A 2 A buck converter for 4.5 V to 6 V with an internal soft-start. Illustrative; not a real part.
part Zbk4 provides BuckIc {
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
    .soft_start_time &= 2ms..4ms
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency

    pins {
        //  pin          kind    name   pads
        .vin.pos       power   "VIN"  "1" "2"
        .vin.neg       power   "GND"  "3" "9"
        .sw.pos        power   "SW"   "4" "5"
        .fb            input   "FB"   "6"
        .en            input   "EN"   "7"
    }

    support {
        .en ~ .vin.pos
    }
}
```

A board that needs the output up within 5 ms holds it, since 4 ms is the slowest:

```nsm evaluate
import src::zbk4::Zbk4
import std::parts::regulator::Buck
import std::sources::Rail

board Quick {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk4> {
        .vout.voltage &= 1.8V +/- 3%
        .soft_start_time <= 5ms
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

A requirement of 3 ms is not held by the slowest end, and the message names the span and the bound:

```nsm fails
import src::zbk4::Zbk4
import std::parts::regulator::Buck
import std::sources::Rail

board TooSlow {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk4> {
        .vout.voltage &= 1.8V +/- 3%
        .soft_start_time <= 3ms
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: reg.soft_start_time must always be at most 3ms, but 2ms..4ms is not always below 3ms
   --> main.nsm:16:9
   |
16 |         .soft_start_time <= 3ms
   |         ^^^^^^^^^^^^^^^^^^^^^^^
```

With `Zbk2`, which states no time, the same bound is not evaluated: the converter's member has no value, and the bound is listed as such.

```nsm evaluate
import src::zbk2::Zbk2
import std::parts::regulator::Buck
import std::sources::Rail

board NoTime {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> {
        .vout.voltage &= 1.8V +/- 3%
        .soft_start_time <= 5ms
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

A frequency set by a resistor is a relation the IC writes, and the resistor is a part of the IC's own `support` section: here `rt_value` is a field of the IC, and the converter then uses the frequency that follows:

```nsm-fragment
part Zbk3 provides BuckIc {
    .rt_value: Resistance
    .fsw == 24GHz * 1ohm / .rt_value
    pins {
        .rt passive "RT" "8"
    }
    support {
        .rt_resistor: Resistor { .resistance &= 20kohm +/- 1% }
        .rt ~> .rt_resistor ~> .vin.neg
        .rt_value == .rt_resistor.resistance
    }
}
```

**The circuit an IC carries** (ADR 0115). `Buck` wires what every buck IC has: its input, its switch node and its feedback pin. What one IC alone needs, the parts and connections its datasheet's application circuit puts around it (a bootstrap capacitor, the resistor on `RT`, the capacitor on an internal supply, an enable divider, a pin tied to the input), is in that part's `support` section, and comes with the part wherever it is placed. `Zbk2` joins its enable to its input there; the `Lt8641` below has six parts in its own.

- `support { ... }` is written only inside a part's own definition, once or more. It holds parts (`.boost: Capacitor { ... }`) and the statements that wire and size them: connections to the part's pins (`.bst ~> .boost ~> .sw.pos`), narrowings, defaults and relations. Values, pins, tables and catalogs belong to the part itself.
- A part declares no other part outside its `support` section.
- A support part is at the path below its part (`reg.ic.boost`) and is a part like any other: picked from its catalog, checked, drawn on the schematic, placed and listed in the bill of materials.
- A board that names the IC (`.reg: Buck<Lt8641>`) may narrow a support part from outside the module, as any member inside a module (ADR 0116): `.reg.ic.soft_start.capacitance &= 22nF +/- 10%`, which replaces the part's `?=` default. The module's own text still reaches only its bound's members.
- The same board may join the IC's own pins that `Buck` leaves open (ADR 0120): `.reg.ic.pg ~ .pg_pullup.line` gives the `Lt8641`'s power-good output a pull-up. `examples/two_buck` joins its invented IC's `PG` this way, with a pull-up and a test point in each stage.

```nsm fails
import std::parts::passive::Capacitor

part Chip {
    .boost: Capacitor { .capacitance &= 100nF +/- 20% }
    pins {
        .bst passive "BST" "1"
        .sw  passive "SW"  "2"
    }
    .bst ~> .boost ~> .sw
}
```

```text
error: a part holds other parts only in its `support` section, the circuit its datasheet puts around it: move `.boost: Capacitor` into `support { ... }`
  --> main.nsm:4:13
  |
4 |     .boost: Capacitor { .capacitance &= 100nF +/- 20% }
  |             ^^^^^^^^^
```

What is not modelled: the divider's current is not added to the load, and the IC's quiescent current is not in the inductor's (it is in the IC's own input current, if the IC states it, as the `Lt8641` does), the loop and its stability are not checked (the IC's capacitor and inductor ranges stand for them), the inductor's saturation is its datasheet current at a fall of the inductance (30% for the catalog's parts) and not a curve, and the ripple current of the capacitors is not modelled, so it is not shared out between the capacitors of a bank.

The library has four real buck ICs. `Lmr16006y` (TI LMR16006Y, 4 V to 60 V, 0.6 A, asynchronous, with its catch diode in its `support` section), `Rt8059` (Richtek RT8059, 2.8 V to 5.5 V, 1 A) and `Tps54202` (TI TPS54202, 4.5 V to 28 V, 2 A) each have a catalog row, so a board picks among them with `BuckConverter` (_A module that takes a part_); their datasheet figures, pages and what each leaves unstated are in their doc comments and `data/jlcpcb/README.md`. The fourth is `std::parts::regulator::Lt8641`, Analog Devices' 3 V to 65 V, 3.5 A LT8641 in its 18-lead 3 x 4 mm QFN (datasheet Rev. C). It is written as above, with each number's datasheet page and table in its doc comment and in `data/jlcpcb/README.md`: the reference 0.79 V to 0.822 V, the input range, 3.5 A, the efficiency, `min_inductance` from the datasheet's Equation 10, a DCR of at most 0.04 ohm, 2.2 uF at the input, and 47 uF at the output by default (every Typical Application of the datasheet uses it; the datasheet states no minimum). The switching frequency follows the resistor on RT, `.fsw == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)` with the oscillator's accuracy (Equation 3 and Table 1); the part refuses a resistor outside 18.2 kohm to 60.4 kohm (2 MHz to 700 kHz), the span the datasheet gives the accuracy for. It also refuses an input and output at which the datasheet's Equations 4 and 5 say the frequency is no longer held, because the converter computes its ripple at the programmed frequency. Its `support` section is the datasheet's application circuit: 0.1 uF from BST to SW, 1 uF on INTVCC, 1 uF at VIN1 and at VIN2, 10 nF on TR/SS, the resistor on RT (47 kohm, 0.89 MHz, by default), BIAS to ground and EN/UV to the input; SYNC/MODE and PG are left open. The automotive ordering code `LT8641IUDC#WPBF` is the same die and tables as the other grades. The standard `lt8641` catalog has a header and no row, like `p_mosfet` (a row needs a stock above zero on a record that was read), so the board's project has the row, in its own file of that name (the part number is the datasheet's Order Information; a project adds the LCSC number it has read). The catalog is not keyed by `.mpn`: the solver takes the first row that fits, and a board that wants one ordering code narrows `.reg.ic.mpn` (ADR 0116).

```csv file=catalogs/lt8641.csv set=lt8641
mpn,package,library,source
LT8641IUDC#WPBF,UDC20(18),Extended,"datasheet Rev. C, Order Information p.2 (automotive products)"
```

The datasheet asks for a DC resistance of 0.04 ohm at most (p.16), and every inductor of the standard catalog has more (0.085 ohm at the least), so a converter around the IC needs an inductor of the project's own. This one is invented, like the part numbers of the other files of this guide:

```csv file=catalogs/inductor.csv set=lt8641
mpn,name,inductance,saturation_current,dc_resistance,package,library,source
ZL-3R3,ZL-3R3 3.3uH 4.5A inductor 1210,3.3uH +/- 20%,4.5A,0.03ohm,1210,Extended,https://example.invalid/zl-3r3
```

The board around it: 9 V to 16 V in with a 36 V peak and 3.3 V out at up to 2 A.

```nsm evaluate catalogs=lt8641
import std::parts::regulator::{Buck, Lt8641}
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0A..2A
    }
    .reg: Buck<Lt8641> { .vout.voltage &= 3.3V +/- 4% }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The IC's soft-start is set by a capacitor on `TR/SS`, and the datasheet gives no time for one: a pull-up current of 1.2 uA to 2.6 uA, 1.9 uA typical (Electrical Characteristics, p.3), and an output that tracks the pin "from 0V to 0.8V" (p.19). The part has an `optional` member `.css`, the capacitor, and the time it computes from those figures alone, `.soft_start_time == .css * 0.8V / .tr_ss_current` (the pull-up current is the member `.tr_ss_current`, a range). Because the current is a range the time is one, and the smallest current gives the longest time. Its `support` section puts 10 nF of 10 % on the pin, the capacitor of the datasheet's Typical Applications, and states `.css` from it: 9 nF at 2.6 uA to 11 nF at 1.2 uA is 2.77 ms to 7.33 ms, and a bound is held against the end that misses it. This board needs the output up within 8 ms:

```nsm evaluate catalogs=lt8641
import std::parts::regulator::{Buck, Lt8641}
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0A..2A
    }
    .reg: Buck<Lt8641> {
        .vout.voltage &= 3.3V +/- 4%
        .soft_start_time <= 8ms
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The same board with a bound of 5 ms is not held, and the message names the range:

```nsm fails catalogs=lt8641
import std::parts::regulator::{Buck, Lt8641}
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 36V
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0A..2A
    }
    .reg: Buck<Lt8641> {
        .vout.voltage &= 3.3V +/- 4%
        .soft_start_time <= 5ms
    }

    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
error: reg.soft_start_time must always be at most 5ms, but 2.769ms..7.334ms is not always below 5ms
   --> main.nsm:16:9
   |
16 |         .soft_start_time <= 5ms
   |         ^^^^^^^^^^^^^^^^^^^^^^^
```

The part states the absolute maximum of three pins besides VIN and BIAS, from its Absolute Maximum Ratings (p.2): `EN/UV` 65 V, `TR/SS` 4 V and `SYNC/MODE` 6 V, each at the end of the pin's row (`.tr_ss passive "TR/SS" "16" abs_max 4V`, ADR 0124), measured from the part's ground `GND`, so the solver compares the voltage of the pin's net with the limit as it does for any `Logic` input (ADR 0046). A board that ties `SYNC/MODE` or `TR/SS` to a 9 V to 16 V input is refused with the net, the limit and the field that states it (`net `VIN` reaches 16V above its ground `GND` ... but `u.sync_mode.abs_max` is 6V`). A pin left to float is not judged, and one with only a soft-start capacitor on it is a `not evaluated` note, since nothing sets its voltage. `PG` (42 V) and `FB` (4 V) have limits too and state none, for the reasons in the part's doc comment and ADR 0095.

The IC draws current from its input for itself with nothing loaded, and the input current of the part is the load's and that (ADR 0096): `.quiescent_current`, a range from zero to the largest figure of the mode the converter runs in, which the DC analysis draws at the input too (`dc switching_regulator ... .quiescent_current`), so a sleep-current budget summed from `.u.vin.current` is judged against it. The datasheet gives 10 uA over temperature in Burst Mode, with `SYNC/MODE` tied to ground (`.iq_max`, Electrical Characteristics, p.2), and "several hundred uA" with it floating, high or clocked (0.5 mA at most at 25 C, `.iq_skipping_max`). The part cannot see how the board ties the pin, so the default is the larger, and a board that grounds `SYNC/MODE` says so: `.u.quiescent_current &= 0A .. .u.iq_max`. The 10 uA is a maximum and not a minimum, so the least the input draws is still zero, and a board cannot ask for `.u.vin.current >= 10uA`. The converter's feedback divider is a load on the output and is not in this current (`.divider_current` has it).

The datasheet's other parts (the feedback phase-lead capacitor, an enable divider for an undervoltage lockout above the IC's own) are not in its `support` section. The package has no footprint or symbol in KiCad's libraries, so `std/kicad.toml` maps `Lt8641` in the package `UDC20(18)` to netsmith's own footprint, `netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA` (the _Standard libraries of footprints_ paragraph below says what that is), and names no symbol, so the symbol is drawn from the pin table: a board built around it exports with no KiCad install and no `kicad.toml`. The footprint's pads are the datasheet's recommended solder pad pitch and dimensions (p.24); a project that wants another land pattern maps the part and package to its own, as for any other part (`std/kicad.toml` and a project's `kicad.toml` may not both map one pair, so the project uses its own package name in its row).

**Standard libraries of footprints** (ADR 0087). A package that KiCad's libraries lack and the generators of ADR 0043 do not make is drawn in `std/libs/`: footprints as `.kicad_mod` files in `std/libs/netsmith.pretty/` and symbols in `std/libs/netsmith.kicad_sym`, in KiCad's own formats, compiled into the binary. They are the library `netsmith`, named in a row of `std/kicad.toml` like any other (`footprint = "netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA"`), found before any KiCad install is read, and the same on every machine. A project cannot list a library called `netsmith` (`kicad.toml: library `netsmith` is the name of the standard library's own footprints and symbols ...`). When a board uses one of them, `build`, `fab` and the editor write the library into the board's output folder (`netsmith.pretty/`, `netsmith.kicad_sym`) and list it in the `fp-lib-table` and `sym-lib-table` beside the `.kicad_pro`, as they list a project's own libraries, so KiCad opens the board with the footprint in its library table. Every footprint of the folder is named by a row of `std/kicad.toml`, and a test checks each against the pin table of its part (every pad of the table is a pad of the footprint and a pin of the symbol, and the other way round). A symbol of that library is one netsmith drew: a row of `std/kicad.toml` (or of a project's `kicad.toml`) that has no `symbol` names none, and a part with a pin table is then drawn as a plain box, one box for the part with its pins on the sides (inputs left, outputs right, ground below, and the supply above, except a supply that takes power in a part that also gives power, which is on the left), each pin with its datasheet name, its pad number and the electrical type of its kind. A pin table says `power` and not whether the pin takes power from its net or gives it, because that is the sign of the current of the rail on the pin, which the board's loads decide: the design stage reads it from the solved model (a pin that is the `pos` of a rail whose current is negative, from below 0 A to at most 0 A, gives power; one whose current is drawn, from at least 0 A to above 0 A, takes it; and a pin that the part joins to such a pin has its direction). A pin that gives power, such as the switch node of a converter, is a power output on the right of the box, and the others, including a power pin whose direction is not known (a rail whose current has no value or may take either sign), are power inputs, so that a net with no output gets a power flag: a power input of a part that also has a pin that gives power (the input of a regulator or a converter) is on the left of the box, where the schematic layout reads a converter's input, and any other is above it. The symbol is named for the part and its package (`netsmith:Lt8641_UDC20_18`), with `_out` and the first pad of each pin that gives power when there is one (`netsmith:Lt8641_UDC20_18_out8`), so that two parts of one kind that differ in direction are two symbols, and written into the board's `netsmith.kicad_sym` with the rest of the library. A row that does name a `symbol` is drawn with it, except where netsmith draws that id itself: the ids of the standard rows of the resistor, capacitor, inductor, ferrite bead, fuse, crystal, LED, the four diodes, the switch and the two FETs (`Device:R`, `Device:C`, `Device:L`, `Device:FerriteBead`, `Device:Fuse`, `Device:Crystal`, `Device:LED`, `Device:D`, `Device:D_Schottky`, `Device:D_Zener`, `Device:D_TVS`, `Switch:SW_SPST`, `Transistor_FET:Q_NMOS_GSD` and `Transistor_FET:Q_PMOS_GSD`) are answered from the binary with a symbol of netsmith's own (`netsmith:R`, `netsmith:D_Schottky`, ...) before any KiCad library is read, on every machine, so a board of those parts needs no KiCad symbol library. The glyph is the conventional drawing of the part (a rectangle for a resistor, two plates for a capacitor, a triangle and a bar for a diode), and the pins, their numbers, names and types, the places of the reference and the value, and the extent of the body are those of KiCad's symbol of the id, so the layout of the schematic is the same. The same holds for the generic connectors (`Connector_Generic:Conn_01x02` to `Conn_02x10_Odd_Even`, the `Connector:Conn_01x02_Socket` family), the two USB-C receptacles (`Connector:USB_C_Receptacle_PowerOnly_6P` and `USB_C_Receptacle_USB2.0_16P`), `Connector:Conn_Coaxial`, `Connector:TestPoint`, `Mechanical:MountingHole`, `MountingHole_Pad` and `Fiducial`, and for `power:PWR_FLAG`, the one-pin power symbol that the exporter places on a net that takes power and has nothing on the board to drive it: it is answered with `netsmith:PWR_FLAG`, a power symbol with the same `power_out` pin and the value `PWR_FLAG`, which KiCad's electrical rules check takes as it takes its own, and it is written into the board's `netsmith.kicad_sym` when the board has such a net. The pins of these symbols (the headers' odd pins left and even pins right, the USB-C receptacle's power on the right and its ground below), the places of the fields and the extent of the body are those of KiCad's symbol, for the layout; the drawing is netsmith's. The linear regulator is drawn from its pin table like an IC, so no row of the standard mapping reads a symbol from KiCad's libraries any more (ADR 0097, steps 1 and 2); every footprint of the standard rows is generated or one of these files (step 3), so no standard row needs KiCad's footprint library. A project that lists a library named `Device`, `Switch` or `Transistor_FET` in its `kicad.toml` is read from that library, as before.

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
    }
    .load: Rail {
        .current &= 100mA..1kA
        .pos.net &= "VFUSED"
    }
    .fuse: Fuse {}
    .input.pos ~> .fuse ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no `fuse` candidate satisfies `fuse`: the closest of the 8 that fit, 1206HV-3A fuse 3A 125V 1206 C21261120, fails because fuse.current must always be at most fuse.rated_current, but 100mA..1kA is not always below 3A (the currents on net `VFUSED`: -fuse.current (100mA..1kA) + load.current (100mA..1kA) add up to zero; net `VIN`: fuse.current (100mA..1kA) + input.current (-1kA..-100mA) add up to zero)
    --> std/parts/passive.nsm:396:5
    |
396 |     .current <= .rated_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Inductor
import std::sources::Rail

board Saturates {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
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
    --> std/parts/passive.nsm:271:5
    |
271 |     .current <= .saturation_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### What a part in a chain sees of its rail

A part reads its own fields, and the checker derives values for it from a rail and from the chain it is in: `.bias` for a part across the rail (ADR 0013) and, for a part in a `~>` chain from a rail, `.supply_peak`, the `.peak` of the rail the chain starts on (ADR 0086). A `Fuse` has it, so it is checked for the highest voltage it has to interrupt, `.supply_peak <= .rated_voltage`, with no line on the board, and the catalog picks a fuse rated for it. The rail is the one whose `pos` is on the net the chain starts on; where the scope has several, the rails it declares itself come before the rails of what it holds, and the first by path stands. Every part of the chain gets the start's peak, the ones after other parts too: a part before it only drops voltage, and the drop is not subtracted. A chain that starts on no rail's `pos` (a signal, a rail's `neg`) gives its parts nothing, and a fuse in no chain from a rail has no `.supply_peak`: the board states it (`.fuse.supply_peak &= 24V`) or the check is listed as not evaluated, as is a check on a rail that states no `.peak`. A value the board states for a fuse in a chain is checked against the rail's.

Two more values reach a part through the chain (ADR 0091), both for a diode behind a fuse:

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
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VFUSED"
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
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VFUSED"
    }
    .fuse: Fuse {}
    .input.pos ~> .fuse ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no `fuse` candidate satisfies `fuse`: the closest of the 8 that fit, 1206HV-3A fuse 3A 125V 1206 C21261120, fails because fuse.supply_peak must always be at most fuse.rated_voltage, but 5kV is not always below 125V
    --> std/parts/passive.nsm:397:5
    |
397 |     .supply_peak <= .rated_voltage
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
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VPROTECTED"
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
    }
    .load: Rail {
        .current &= 10mA..100mA
        .pos.net &= "VPROTECTED"
    }
    .fuse: Fuse { .rated_current &= 3A }
    .d: Schottky { .max_forward_current &= 1A }
    .input.pos ~> .fuse ~> .d ~> .load.pos
    .input.neg ~ .load.neg
}
```

```text
error: no combination of candidates satisfies `d` and `fuse` together: the closest, d = 1N5819WS 40V 1A Schottky SOD-323 C191023, fails because d.supply_fault_current must always be at most d.max_forward_current, but 3A is not always below 1A
    --> std/parts/diode.nsm:154:5
    |
154 |     .supply_fault_current <= .max_forward_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: d.supply_fault_current must always be at most d.max_forward_current, but 3A is not always below 1A
    --> std/parts/diode.nsm:154:5
    |
154 |     .supply_fault_current <= .max_forward_current
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

`Crystal` has `.frequency` (state it with its tolerance, `16MHz +/- 20ppm`) and `.load_capacitance`, and no relation: the two load capacitors are the design's. `FerriteBead` has `.impedance` at `.impedance_frequency`.

### Transistors

`NMosfet` and `PMosfet` have the pins `gate` (an input, which must be connected), `source` and `drain`. All their voltages and currents are magnitudes. The design states the stress it puts on the part: `.drain_source_voltage`, the voltage the part blocks while off, and `.gate_source_voltage`, the voltage the gate is driven to while on. They are checked against the maxima, and the gate drive against the threshold, which is a range: the drive must be above the highest threshold.

The on-resistance of a part is the datasheet's figure at the gate-source voltage its table names, and a smaller drive gives a larger resistance. `.on_resistance_vgs` is that voltage (ADR 0092) and the drive must always be at least it, so a part picked from a catalog is one whose guarantee holds at the drive the design gives. The 2N7002's 7 ohm is guaranteed at 5 V and the AO3400A's 48 mohm at 2.5 V, so a 3.3 V logic rail is given the AO3400A. A row that records no test voltage is offered all the same, and the check is not evaluated for it; a design that wants the check states a requirement on the member (`.q.on_resistance_vgs <= 3V`), and is then offered only the rows that record a voltage.

```nsm fails
import std::parts::transistor::NMosfet
import std::sources::Rail

board Switch {
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
    }
    .logic: Rail {
        .voltage &= 0.3V +/- 5%
        .pos.net &= "V0V3"
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
   --> std/parts/transistor.nsm:79:5
   |
79 |     .gate_source_voltage >= .gate_threshold
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
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "V3V3"
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
   --> std/parts/transistor.nsm:80:5
   |
80 |     .gate_source_voltage >= .on_resistance_vgs
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: q.gate_source_voltage must always be at least q.on_resistance_vgs, but 3.135V..3.465V is not always above 5V
   --> std/parts/transistor.nsm:80:5
   |
80 |     .gate_source_voltage >= .on_resistance_vgs
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
    --> std/parts/transistor.nsm:135:5
    |
135 |     .drain_source_voltage <= .max_drain_source_voltage
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### What a part is at DC

A part says what it is to a DC analysis with a `dc` statement, once, in the part or in the generic that it provides (ADR 0076, amendment 1). The statement names the element, then its pins, then its values, in the order the element fixes:

```nsm-fragment
dc resistor .a .b .resistance
dc diode .anode .cathode .forward_voltage
dc nmos .gate .drain .source .gate_threshold .on_resistance
dc pmos .gate .drain .source .gate_threshold .on_resistance
dc linear_regulator .vin .vout .quiescent_current
dc switching_regulator .vin .sw .efficiency
dc source .vbatt
```

- `dc resistor <terminal> <terminal> <resistance>`: a resistance between two pins, over the range of the field (a resistor, a fuse, a ferrite bead's `.dc_resistance`).
- `dc diode <anode> <cathode> <forward voltage>`: blocks, or holds the forward voltage and carries a current of at least zero.
- `dc nmos` and `dc pmos`, `<gate> <drain> <source> <gate threshold> <on resistance>`: a MOSFET as a switch, `.on_resistance` between drain and source when the gate drive is above the threshold and open when it is below. The gate draws nothing, and the values are magnitudes as in `NMosfet` and `PMosfet`.
- `dc linear_regulator <input rail> <output rail> [<quiescent current>]` and `dc switching_regulator <input rail> <output rail> <efficiency> [<quiescent current>]` (ADR 0076, amendment 3; the optional operand, ADR 0096): a regulator. The first two operands are the part's rails (a rail inside an interface is named through it: `.out.sw`), not pins. The output rail is an ideal source over the range of its `.voltage`; the input rail draws what the output asks. A linear regulator draws what it supplies and its quiescent current, a range of currents that is not negative, taken at both ends (amendment 9): the third operand is optional, a field of the part that is a current, and a linear regulator that leaves it out draws what it supplies alone. A switching one draws `Pout / (efficiency * Vin)`, with the efficiency a number (a range, `0.85..0.95`, is taken at both ends); an efficiency has to be above 0 and at most 1 (ADR 0076, amendment 14), and one that is not (`85..95` for `85%..95%`, a converter that makes power) is an error that names the part and the value, and the regulator is not modelled. A switching regulator draws its quiescent current too when its statement names one, as a fourth operand that is optional in the same way (ADR 0096); with the output unloaded it still draws that current. A quiescent current that the part names and leaves with no value (or a negative one) is a number nothing can model with: the regulator is not modelled and the island in front of it is left alone, as for a regulator that states no `dc`.
- `dc source <supply rail>` (ADR 0076, amendment 5): the part gives this rail from outside the board, as a connector gives the supply of a vehicle. The operand is a rail of the part. Its `pos` net is held over the range of the rail's `.voltage` that the design states, and its `neg` net at zero, as a `Rail` that the board declares itself is (see "Supplies" below). A part with a rail it takes its supply from (what a load is) does not state it.

The standard `Resistor`, `Fuse`, `FerriteBead`, `NtcThermistor` (over the range of its maker's resistance-temperature table), `Inductor` (its winding: `dc resistor .a .b .dc_resistance`, ADR 0076, amendment 6), `Diode`, `Schottky`, `Led` (a diode that drops its `.forward_voltage`, ADR 0113), `NMosfet`, `PMosfet`, `LinearRegulator`, `Lt8641`, and the two connectors that give a supply, `Obd2M` (`.vbatt`) and `UsbCPower` (`.vbus`), state theirs (any other buck converter's IC is a part of the design's own, and states `dc switching_regulator` itself, as the one in "Switching regulators" does; the statement is a power balance that assumes no topology, so a buck-boost or boost IC states it the same way on its own input and output rails), so a provider of one of them has it on its own pins and fields without writing it. A part with a `bridge` and no `dc` statement is a boundary: a DC analysis cannot see through it. An inductor is its winding at DC, so the node past a buck stage's inductor is the switch node less what the load makes the winding drop, and the output side of the stage is an island of its own. `Switch`, `Zener` and `Tvs` are boundaries for now. A module has no `dc` statement of its own: its network is the elements of the parts inside it.

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

A regulator's own current is a third operand of `dc linear_regulator` and a fourth of `dc switching_regulator`, and the standard `LinearRegulator` and `Lt8641` name it. A provider of it narrows `.quiescent_current` from its datasheet, and the DC analysis then draws it at the regulator's input besides the load (ADR 0076, amendment 9): the island behind the regulator is solved, and the stage in front of it is asked for the load and this current.

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
    }
    .reg: SmallLdo
    .supply.pos ~ .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .load: Rail {
        .current &= 20mA..50mA
        .pos.net &= "V33"
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
   --> main.nsm:11:9
   |
11 |         .max_voltage >= .bias * 1.5
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A capacitor across no rail has no bias to rate: state `.bias`, or place it between a rail's `pos` and `neg`.

### Connectors, switches and mechanical parts

The connectors are `Header1x2`, `Header1x4`, `Header1x8`, `Header1x10`, the sockets `Socket1x2`, `Socket1x4`, `Socket1x8`, `Socket1x10`, and the two-row headers `Header2x3`, `Header2x5` and `Header2x10`. A pin is `pN`, on pad N, and contact N of a connector is its pin `pN`: a header and its socket number so that contact N meets contact N. There is one definition for each size because the language has no parameter for a count: a part with `N` pins has `N` rows in its table. The pitch is the package (`.package &= "2.54mm"`), which the export mapping reads. Each states its `.mating` (kind, gender, pitch, rows, positions), which a system uses to mate boards (see Systems and ports). The single-row ones are rated at 3 A a contact, from the datasheets of the `header` catalog; the two-row headers have no row, are unrated, and the check of the current through a mate lists them as not evaluated.

`Coax` has `signal` and `shield`. `ProgrammingPads6` is six pads, `p1` to `p6`, for a spring-pin programming cable; the board decides what each carries. `UsbCPower` is a six-contact USB Type-C receptacle with power only: a `.vbus` rail (the receptacle supplies, so its current is negative and follows from the loads on the net), the two configuration pins `cc1` and `cc2`, and the `shell`. A device that takes power from a source needs a 5.1 kohm pull-down from each configuration pin to ground, written as a resistor in a chain. `UsbC2` is the sixteen-contact USB 2.0 receptacle: the same pins, the `Usb2` port, and the open sideband pins `sbu1` and `sbu2`. `UsbCSink` is the receptacle of a device that takes its power from a host (a sink): a module of a sixteen-contact receptacle (a provider of the generic `UsbCSinkReceptacle`, `UsbCReceptacle16P` by default) and the two Rd resistors of the Type-C specification, 5.1 kohm at 1 % from each of CC1 and CC2 to ground, so a board writes none. It has the `.vbus` rail (the host supplies: a source for the DC analysis over the `.voltage` the board states, with a current that is negative and follows from the loads), the data pair `.dp` and `.dm`, and nothing else; the shell is joined to ground inside and the sideband contacts are left open. The module does not provide `Connector` (a module cannot, since the generic has a `pins` table); its receptacle does, so the placer and the schematic treat it as a connector. A board narrows what it needs inside it (ADR 0116): `.receptacle.package` is the receptacle's footprint (`"GCT_USB4105_16P"`, the default, or `"HRO_TYPE-C-31-M-12"`), and `.cc1_rd.package` and `.cc2_rd.package` the resistors' (`"0402"`). The receptacle is a use of the generic `UsbCSinkReceptacle`, so a project adds the receptacle of another maker, with its own pads, footprint and LCSC number, as a part that says `provides UsbCSinkReceptacle, Connector` (the generic gives it `.vbus`, `.package` and `.mpn`, the shell is the generic's `.shell`, and it has its own pin table named for its package and its own catalog), in a file that the board's file imports. The standard receptacle comes first in the order and has pin tables for its two footprints only, so the board takes the project's by narrowing its package and number: `.jack: UsbCSink { .receptacle.package &= "SMD"; .receptacle.mpn &= "ACME-USBC-16" }`. A board that states neither keeps the standard receptacle, however many providers are in scope. A port that gives power, a charger's or a host's downstream port, uses the generic `UsbCSourceReceptacle` (#463): the same sixteen contacts and no `dc` statement, since the board's own converter or switch drives VBUS, and `.vbus.current >= 0A`, since the device on the cable draws from the board. The board states the most the port allows (`.vbus.current &= 0A..3A`); a current that would supply the board is refused. A dual-role port, which takes power or gives it, uses `UsbCDualRoleReceptacle`, whose `.vbus.current` the board states as a signed range (`-3A..1.5A`). No standard part provides either, since `UsbCReceptacle16P` provides the sink generic and a provider takes its `dc source`: a project's receptacle says `provides UsbCSourceReceptacle, Connector`. A receptacle that carries no power role is a `Connector` with a plain `Rail`. `Obd2M` is the male OBD-II (SAE J1962) vehicle connector: a `.vbatt` rail (the vehicle supplies, so its current is negative and follows from the loads on the net; `pos` is battery power on pin 16 and `neg` chassis ground on pin 4), the optional `signal_gnd` (pin 5), and the bus pins `can_high`, `can_low`, `iso_k`, `iso_l`, `j1850_bus_pos` and `j1850_bus`; the pins J1962 leaves to the manufacturer are `p1`, `p3`, `p8`, `p9`, `p11`, `p12` and `p13`. Its catalog `obd2_m` has no standard row yet: a project's own `catalogs/obd2_m.csv` lists the connector it buys.

```nsm evaluate
import std::parts::connector::UsbCPower
import std::parts::passive::Resistor
import std::sources::Rail

board UsbPowered {
    .usb: UsbCPower {
        .vbus.voltage &= 5V +/- 5%
        .vbus.pos.net &= "VBUS"
    }
    .system: Rail { .current &= 0mA..400mA }
    .usb.vbus ~ .system
    .rd1: Resistor { .resistance &= 5.1kohm +/- 1% }
    .rd2: Resistor { .resistance &= 5.1kohm +/- 1% }
    .usb.cc1 ~> .rd1 ~> .usb.vbus.neg
    .usb.cc2 ~> .rd2 ~> .usb.vbus.neg
}
```

A device with a receptacle that has the Rd built in. The board writes no resistor; the host's 5 V is what the rest of the board draws from:

```nsm evaluate
import std::parts::connector::UsbCSink
import std::sources::Rail

board UsbDevice {
    .usb: UsbCSink {
        .cc1_rd.package &= "0402"
        .cc2_rd.package &= "0402"
    }
    .usb.vbus.voltage &= 5V +/- 5%
    .usb.vbus.pos.net &= "VBUS"
    .system: Rail { .current &= 0mA..400mA }
    .usb.vbus ~ .system
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

### A connector of your own

A connector is a part that provides the generic `Connector` of `std::parts::connector` (ADR 0068). The schematic's layout puts a connector at the start of a signal's walk and the placer puts it on the board's edge, and both ask the part for the generic: not its name, which no list holds, and not its designator, which is a label. Every header, socket, `Coax`, `UsbCPower`, `UsbC2`, `UsbCReceptacle16P`, `Obd2M` and `ProgrammingPads6` provides it, so a project's own connector does what they do by saying the same word. `Connector` states only what is true of every connector: its letters are `"J"` (so the provider does not state them again, and cannot state others). It has no pads, no package and no catalog, so a provider has its own pin table and its own `.package: Package`, and a part named `HeaderBuffer` that does not provide it is an ordinary part, an IC with the letters `U`.

```nsm evaluate
import std::parts::connector::{Connector, Header1x2}

/// A 2-pin wire-to-board connector of the project. Invented.
part MolexPicoBlade provides Connector {
    pins {
        .vcc passive "VCC" "1"
        .gnd passive "GND" "2"
    }
    .package: Package
}

/// An IC whose name starts like a header's. It is no connector: it provides nothing.
part HeaderBuffer {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board Mix {
    /// `J1`, and laid out like `Header1x2`.
    .battery: MolexPicoBlade
    /// `J2`.
    .debug: Header1x2
    /// `U1`.
    .buffer: HeaderBuffer
}
```

### DC analysis

After the parts are picked, the solver builds the network of the parts that state what they are at DC and solves it (ADR 0076). Where the equations of the design cannot say what a part carries (a net with two currents that nothing states, ADR 0026), the network can: the pass writes `.current` and `.drop` of a part, and the checks that waited for them (`.drop == .current * .resistance`, `.drop * .current <= .max_power`, a fuse's `.current <= .rated_current`) are judged with the numbers. A pass that finds a rating exceeded is an error, like any relation, and says which states of the circuit give it.

What it models in this stage:

- **Resistors.** A `dc resistor` part (a resistor, a fuse, a ferrite bead) is its resistance over the range of the field, `10kohm +/- 1%` taken at both ends. A resistance that can be zero (a bead's `0ohm..70mohm`) is solved at zero too, as a short (ADR 0076, amendment 16), and one under 1 mohm is its own value: a 0 to 0.2 mohm link at 90 A to 100 A leaves 11.98 V to 12 V of a 12 V supply, not the 11.9 V to 11.91 V that a floor of 1 mohm gave. The exceptions are a resistor between two nodes that a source holds, or from a node to itself, and a combination whose zero-ohm parts make a loop (two beads side by side at 0 ohm): those are solved at 1 mohm, as every resistance under it once was.
- **Diodes.** A `dc diode` part blocks, or holds its forward voltage (both ends of the range) and carries a current of at least zero. A state that is not what the circuit says is refused and not counted: a diode in front of a load that always draws cannot block, and one with its anode below its cathode cannot conduct.
- **Supplies.** A `Rail` that the board itself declares, with a `.voltage` that the board states, is an ideal source over that range between its `pos` and its `neg`, and so is the rail that a part says it gives with `dc source` (a connector's supply: `Obd2M` and `UsbCPower` do) when the design states its `.voltage`. Any other rail of a part or of a module is no source: a part's rail is what the part draws, and a module's is a port whose range may be a seed or a promise. A range that is not a source cannot be treated as one: the voltage of a node would then move apart from the current that makes it.
- **Loads.** A rail of a part, or a rail of the board that is not a supply, with a `.current` that is never negative is a current source over that range from its `pos` to its `neg`. A part with a `bridge` and no `dc` statement (an LED, an inductor) that has a `.current` with a value, stated or worked out, is a current source over it between the pins of its `bridge`: this is how a board states what is behind a pin that nothing models (a boundary takes a stated `.current`). A pin that the checker says draws nothing (ADR 0027: a capacitor, an input) adds nothing, and a capacitor is open at DC.

- **Regulators.** A part with `dc linear_regulator` or `dc switching_regulator` holds the net of its output rail over the range of the rail's `.voltage` (an ideal source, so what is behind it is an island of its own) and draws from its input what its output asks. A board rail on the output net is then what the regulator supplies: a load, whatever voltage the board states for it. A linear regulator draws what it supplies and its quiescent current, when its statement names one (the range of the current, both ends added to the load's: the load and the quiescent current are drawn at once). A switching one draws `Pout / (efficiency * Vin)`, and `Vin` is what the parts in front of it leave after the current they carry, so the current is the solution of a fixed point, which the pass finds from zero: the iterates rise to the least solution, and when there is none the input falls to nothing. The power tree is solved in order, the output side first: what the loads behind a regulator take (as a range, summed over everything on its output net) is the regulator's load when the island in front of it is solved. A regulator whose output has a pin that nothing models falls back on its own `.vout.current` when something states it (or the balances worked it out), and is otherwise left alone, with the island in front of it. The pass writes the regulator's `.vin.current` and `.vin.voltage` by the rules below, so the part's own relations (`.vin.voltage >= .vout.voltage + .dropout`, `.vin.voltage >= .buck.min_input_voltage`, a stage's `.vin.voltage <= ...`) are judged with the voltage the circuit gives it, and the parts in front (a fuse, a diode, a shunt, a bead) take their currents from it.

- **Logic pins that drive** (ADR 0076, amendment 4). A pin of kind `output`, `tri_state` or `bidirectional` that states its `voh` and its `vol` with the current each is measured at, as the datasheet does (`.voh &= .vdd.voltage - 0.4V at 2mA`, `.vol &= 0.4V at 2mA`), is high, low or (a tri-state or bidirectional pin) high impedance. High joins the line to the pin's supply through an output resistance, low joins it to the pin's ground, and high impedance joins it to nothing. The resistance is the drop over the test current, `0.4 V / 2 mA` = 200 ohm, taken from zero up to that (a lighter load drops less; under 1 mohm the pin is a branch with its own resistance, as a resistor is), so the pin can be exactly at its supply. The pass tries every state of every pin with the states of everything else, and writes what the pin's load carries: a pull-down on the line gets its `.current` and `.drop`, and its relations are judged. The pin needs a reference rail whose two nets a supply or a regulator's output holds, no `.swing` (a line that goes to a supply with no pin has a high the reference rail does not give), and no other pin that could drive its net: two such pins are not modelled, since their states include driving it apart (an input that only leaks, below, is no driver, and may share the net). A pin that states a level without its test current, or a `voh` and no `vol`, is a boundary, unless the part states the output resistances itself (below).

- **Leakage and output resistance** (ADR 0076, amendment 8). `Logic` and `Levels` have three `optional` members that a part states when its datasheet does: `.ii`, the most current the pin takes or gives when it is high impedance, at any voltage from its ground to its supply (`.io.ii &= 1uA`, the datasheet's `Ii`, a magnitude), and `.roh` and `.rol`, the output resistance when the pin drives high and when it drives low (`.io.rol &= 20ohm..50ohm`). A part that states `.roh` or `.rol` has that resistance, with no test current on its level; one that does not has the drop over the test current, as above. No part has to state any of them. A pin that is high impedance and states `.ii` is, besides open, a resistance to its supply or to its ground, `the supply's greatest voltage / .ii` (3.399 V and 1 uA are 3.399 Mohm): the leak that a pull-down or a pull-up on the line has to carry, and that lowers an open-drain line's high. An input that states `.ii` is such a pin, and draws it, also on a line that a pin drives: the line is the pin's states with the input's leak beside them, to the input's own supply or ground; an input that states none draws nothing, as before. Both directions are states, so the worst of them is in every range.

- **Open-drain pins** (ADR 0076, amendment 8). A pin that states a `vol` at a test current and no `voh` is open drain, by the inference the levels check makes (ADR 0075): it is low, through `0.4 V / 3 mA` or whatever its low gives, or high impedance, and never high. Any number of such pins on a line never drive it apart (none drives high), so they are modelled together; a pin that can drive the line high next to them is the two-drivers case above and none of the line is modelled. The pass writes nothing for a part on such a line, since the inference might call a push-pull pin that left its high out open drain, and a current written from it would be too low (a pull-down that the pin drives high). It gives the levels check the voltage of the line, below. A pin with `.rol` and a `vol`, and no test current, is open drain with the resistance it states.
- **MOSFETs.** A part with `dc nmos` or `dc pmos` is off, or on with its `.on_resistance` between drain and source (both ends of the range). The gate draws nothing. A gate above the greatest of `.gate_threshold` (above the source for an N-channel part, below it for a P-channel one) turns it on and never off; a gate below the least turns it off and never on; a gate in between keeps both states. A gate that nothing sets in a state (a tri-state pin that let go of a line with no pull) floats, and both states stay. The switch is modelled when what sets its gate is known: the gate's net is held by a supply, or a resistor, a diode or a driving pin is on it, and no pin that nothing models is. Otherwise it is a boundary, a current source over its `.current` when that has a value, as above.

What it does not model, and leaves as it was: a logic pin that does not state its drive, a pin that shares its net with another driver, and every pin of a part with no `dc` statement and no `.current`. A net that has such a pin on it is not solved, and neither is the island it is in (below): a part there keeps its `not evaluated` checks, and the cheap bound of Ohm's law (ADR 0026) is what it gets, as before. A rail of a part that can give current (a `.current` that has a negative end) and that has no `dc` statement is an output that holds its net, and is not modelled.

**Islands and corners.** A net that a supply holds is cut: what is on its two sides is solved on its own, with the supply's range as one of its values. The network between the cuts is an *island*. For each island the pass solves every *corner* of its ranges (both ends of each resistance, supply and load, both ends of a diode's forward voltage, and a diode that blocks) and keeps the range each current and voltage takes over all of them. A corner solve gives each value one number in each solve, so nothing is counted twice: a divider of two resistors of 1 % is `4.95 V..5.05 V` and not the `4.90 V..5.10 V` that arithmetic on ranges gives. The corners are the circuit's range where the quantity is monotone in each value, which is so for a voltage or a current of a network with one supply, and for a series chain; a network with two supplies through resistors need not be. An island with more combinations than a limit is left alone, and a note names it and says to cut it at a node a rail holds. The limit is 65536, and less for a large island: the work is the combinations times the cube of the number of nodes, which is capped, so that no island takes more than a tenth of a second (an island of 15 nodes and 16 resistors of 1 % is the largest that is solved).

**A source that cannot give what the load asks.** A regulator whose output asks for more power than the source can give through the resistance in front of it has no operating point: 12 V through 10 ohm can give at most `12^2 / (4 * 10)` = 3.6 W, and a 5 V output at 1 A and 90 % asks 5.56 W. That is an error that names the regulator, what it asks and the states, in the combinations of the ranges where it happens (a battery at 10 V may not give what it gives at 16 V). A state that is not the circuit's (a diode that blocks in front of the regulator) is refused and is never the finding. An input below the regulator's minimum is an error too: it is the relation the part states for it, judged with the voltage the analysis wrote.

**What it writes.** A field of a modelled part that nothing states takes the range the circuit gives it. A range that a statement gave a field is a budget (ADR 0038): the circuit has to fit inside it, and when it needs more the error says what it needs and the states that give it. A value the solver worked out from a balance is narrowed to what both say, and when the two share no value (for a current) the design contradicts itself: an error names the field, both ranges and the states that give the analysis's nearest end (ADR 0076, amendment 15), and the field keeps the balance's number. The ranges are widened by a billionth, and by a picoampere or a nanovolt, so that a rounding does not take a value out of the range it is in.

**A chain carries the voltage** (ADR 0076, amendment 5). The network of the parts in a `~>` chain is solved with the supply behind it, so a rail on a node of it has the voltage the chain leaves there: the supply's, less what each part in series drops at the current that it carries, which the loads behind it set. The pass writes that range into the rail's `.voltage` where nothing states it (a rail joined to it with `~` takes it with it), as it writes a part's `.current`: the board does not restate it with `.vout.voltage == .vin.voltage - .fuse.drop - .diode.drop`, which would list the parts of the chain a second time and go stale when one changes. A relation that reads the voltage is judged with it. A voltage that a statement gave the rail (or any rail joined to it by `~`) is the design's, and is left as it is, not narrowed: a window the board states is a claim about the rail, and it is checked (ADR 0076, amendment 17; next to the fuse's budget below): the range it states has to enclose the range the circuit gives the node, and one that does not is an error. A window that encloses it is kept as it is, so a start window above the real range stays valid. Not checked are a rail past a regulator's output (the part holds that node, and a rail past an inductor sits a little below the window the board states for it), a node a supply holds, and a network the analysis does not solve. The chain carries the voltage and not the `peak`: a transient is not a DC quantity, and `.vout.peak` is still stated (a `Tvs`'s `.clamping_voltage` on the rail it clamps, say), as ADR 0086 has a fuse take the rail's peak from the chain's start.

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
    #[role(current)]
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
    #[role(current)]
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
    }
    .protection: Protection
    .load: Rail {
        .pos.net &= "OUT"
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
    #[role(current)]
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
    }
    .link: Link { .resistance &= 0.2ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 3.3V }
    .load: Rail {
        .pos.net &= "V33"
        .current &= 1A
    }
    .input_cap: Capacitor { .effective_capacitance >= 4uF }
    .plug.supply.pos ~> .link ~> .reg.vin.pos
    .plug.supply.neg ~ .reg.vin.neg
    .reg.vin.pos ~> .input_cap ~> .reg.vin.neg
    .reg.vout ~ .load
}
```

**A pick is refused on the circuit's own voltage.** A series resistor in front of a Zener clamp sets the voltage of the node behind it with the load's current and the Zener's, and used to be given that voltage by an equation the board wrote (`.out.voltage == .supply.voltage - .r.drop`), so that a pair of candidates that leaves a load under its minimum was refused when the pair was picked. The equation is no longer needed (ADR 0076, amendment 10): when the last part of a group of parts picked together is chosen, the solver solves the island of the network that has a part of the group, with the picks in it, and gives the rails on it whose voltage nothing has given the voltage that results; the checks that read them (`.out.voltage >= 4.5V`) judge the pick with it, and a pair that fails is refused with the message of any other pair that fails (`no combination of candidates satisfies ... together`). The island is cut at the rails whose voltage an earlier pass worked out (the supply of the module, behind a fuse and a shunt that were picked first), so what is solved is the resistor, the clamp and the load, and not the whole power tree for each of the candidates; an island with a regulator in it is left to the pass after the picks, and so is a group with no such rail (a rail whose voltage is stated, or no check that reads one). An equation the board wrote stays valid. The board below picks the pair on the analysis' number alone; with a supply that sags to 6 V it fails at the pick, as it did with the equation:

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
    }
    .out: Rail {
        .pos.net &= "OUT"
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

A divider with an LED on its middle. The LED holds the middle at its forward voltage, so it takes 1.36 mA to 2.34 mA, inside the window the board states. The top resistor carries what the bottom one and the LED take; nothing states it, and the checks of both resistors are made:

```nsm solve
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board Lit {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
    }
    .top: Resistor { .resistance &= 1kohm +/- 1% }
    .bottom: Resistor { .resistance &= 2kohm +/- 1% }
    .led: Led { .current &= 1mA..3mA; .forward_voltage &= 1.8V..2.4V }
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
    #[role(current)]
    .current: Current
    bridge .drain ~> .source
    dc nmos .gate .drain .source .gate_threshold .on_resistance
}

board Switched {
    .main: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
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
    }
    .fuse: Fuse { .rated_current &= 1A; .current &= 0A..100mA }
    .mcu: Mcu { .vdd.current &= 50mA..300mA }
    .supply.pos ~> .fuse ~> .mcu.vdd.pos
    .supply.neg ~ .mcu.vdd.neg
}
```

```text
error: `fuse.current` is stated 0A..100mA, but the DC analysis of the circuit around it finds 50mA..300mA: at most 300mA at the high end with mcu.vdd at 300mA
   --> main.nsm:13:41
   |
13 |     .fuse: Fuse { .rated_current &= 1A; .current &= 0A..100mA }
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
error: `load.led.current` is stated 10mA..20mA, but the DC analysis of the circuit around it finds 95.04mA..103.1mA: at most 103mA at the high end with r at 99Ω, load.led conducting at 1.8V
  --> main.nsm:7:17
  |
7 |     .led: Led { .current &= 10mA..20mA }
  |                 ^^^^^^^^^^^^^^^^^^^^^^
error: `r.current` is inconsistent: the solver worked it out as 10mA..20mA, but the DC analysis of the circuit around it finds 95.04mA..103.1mA, and the two share no value (the least the circuit gives, with r at 101Ω, load.led conducting at 2.4V)
   --> main.nsm:16:5
   |
16 |     .r: Resistor { .resistance &= 100ohm +/- 1% }
   |     ^^^^^^^^^^^^
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
    #[role(current)]
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
    }
    .link: Link { .resistance &= 10ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 5V }
    .out: Rail {
        .pos.net &= "5V"
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

A supply that can give only so much states it as its `.current`, a negative number because a rail's current is what is drawn at its `pos` (ADR 0014): `.supply.current &= -500mA..0A` is a supply of at most 500 mA (ADR 0076, amendment 13). The loads that the analysis finds on the supply's net are held to it by the rule that ADR 0038 gives two stated ranges, which must be able to meet: the range the loads draw over every corner and the budget. An error says that they cannot, that even the least the loads draw is more than the supply gives, and names the supply, the budget, the least and the most the loads draw, and the corner that gives the least. A supply that states no `.current` has no budget, one with no negative end in its range is left to the balance of its net, and a balance that has already refused the same supply says it once. The draw is the sum over the networks on the net that the analysis solved, so a load that no network models is not in it and the finding is never made on a load that is not there. The converter is invented: at 90 % a 5 V output at 2 A asks 11.1 W, which is 0.70 A from 16 V and 1.25 A from 9 V, so even from 16 V it is more than the 500 mA the supply states.

```nsm fails
import std::sources::Rail

/// Invented: a link of 0.1 ohm.
part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    #[role(current)]
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
    }
    .link: Link { .resistance &= 0.1ohm }
    .reg: Converter { .efficiency &= 0.9; .vout.voltage &= 5V }
    .out: Rail {
        .pos.net &= "5V"
        .current &= 2A
    }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vout ~ .out
}
```

```text
error: `supply.current` is stated -500mA..0A, so the supply `supply` gives at most 500mA, but the DC analysis finds the loads on its net drawing at least 697.5mA and up to 1.252A, the least with supply at 16V
   --> main.nsm:26:5
   |
26 |     .supply: Rail {
   |     ^^^^^^^^^^^^^
```

**Limits.** The pass tries every combination of the states of the parts it sees, which is the worst case. A regulator's other pins on the nets of its input (an IC with two input pins, an enable tied to the input) are its input's and draw nothing more, and its output is held at the node its part names, which for a buck converter is the switch node: the output beyond the inductor is a little below what a stated window asks, and a window the board states is therefore not checked against the circuit. A power tree is solved only when every pin on its nets is modelled: a regulator that states no `dc` element, or a load whose current is not known, leaves the island in front of it unsolved. Nothing is solved before the picks but the start of a circle, which needs the currents of the loads to be known by then. A design cannot say that two pins are never high together, or that two loads never run together: the pass does not know, and it adds both (issue #45 asks for a way to say it). Temperature is a stated tolerance and nothing else. A capacitor is open, so nothing charges: an inrush current is not seen. A diode's reverse leakage and a MOSFET's body diode are not modelled. A regulator's efficiency is one range and not a curve over its load and its input, a regulator's quiescent current is modelled only when its `dc` statement names it (the standard `LinearRegulator` and `Lt8641` do), a regulator is an ideal source (its output does not sag, and a current limit is the part's own relation), and an input below its minimum is an error and not a shutdown. A logic pin has no leakage (high impedance is an open circuit: the language has no member for the datasheet's `Ii`), and a pin's output resistance is the one at the test current its level states. A regulator's output current is summed over the islands on its output net, each solved on its own, so a load whose current follows the output voltage is taken over the whole range of the output (wider, never narrower).

### Logic levels and absolute maximums

A digital line is a `Logic` (`std::logic::Logic`): a pin `.line` and the rail `.reference` whose `neg` is its ground. It also has four voltages, `.vih`, `.vil`, `.voh` and `.vol`, as a datasheet gives them (ADR 0046). A part states those it has, with fixed volts or with an equation on its rail: `.in.vih &= 2V`, `.in.vih &= 0.7 * .vdd.voltage`, `.out.voh &= .vdd.voltage - 0.1V`. Every pin also has `.abs_min` and `.abs_max`, the voltages it survives relative to its ground. A part writes them at the end of the pin's row, `abs_min -0.5V abs_max 3.6V` (ADR 0126), and nowhere else: a statement such as `.in.line.abs_max &= 3.6V` is an error that names the row. A `Logic` line or a bus pin is measured from its interface's ground; any other pin names its ground in the row, or takes the part's ("Several packages, and the absolute limits in the row").

After the rails are solved, the solver checks two things. A fault is an error; a check that cannot be made is a note `not evaluated: ...`, never a pass.

- **Levels.** For every output or bidirectional `Logic` line on a net and every input or bidirectional `Logic` line on it, the output's `voh` must always be at least the input's `vih`, and its `vol` at most the input's `vil`, in the worst case of both ranges. A level with no value, or two lines whose grounds (`reference.neg`) are not on one net or in one ground group, is not evaluated. A pair in which neither line states a level is not looked at. An output that states a `vol` and no `voh` is open-drain, and the line's high is what its pull-up gives it (see "An open-drain line" below).
- **Absolute maximum.** A pin with `.abs_min` or `.abs_max` must have a net whose worst-case voltage relative to its ground stays within them. A net that the DC analysis solved (a divider's middle, a net behind a chain of resistors and diodes: see "DC analysis") has the range of voltage the analysis finds for it over every corner of the ranges and every state of the parts around it, and the finding says `(the DC analysis of the network it is in)` (ADR 0076, amendment 12). Any other net's voltage is the hull of: its ground (0 V), each rail whose `pos` is on it, each `Logic` line that drives it (between its reference rail's `neg` and its voltage), and each rail or ground that a resistor joins it to (a pull-up or a pull-down). It is not evaluated for a net that something drives that is not a `Logic` line, for a rail or driver on another ground, and for a net nothing sets. A pin that nothing outside its own part is joined to (an optional pin left to float, such as the `TR/SS` of the LT8641) has no voltage to judge and is skipped, with no note; a pin with only a capacitor on it is the note "nothing sets its voltage".

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
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3" abs_min -0.5V abs_max 3.6V
    }
}

board Divided {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC5"
    }
    .core: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
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
    }
    .high: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC5"
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
   --> main.nsm:41:5
   |
41 |     .a: Driver
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

The level checks apply to the lines that are `Logic` and to the pins of a bus interface that states `.levels` (see "Logic levels of a bus" below). A rating of any other pin is judged the same way, from its pin row (`abs_max 65V`), which names its ground. A rating is stated nowhere but in the row, so no pin has one whose ground is not known.

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
   --> main.nsm:37:5
   |
37 |     .a: Weak
   |     ^^^^^^^^
```

The rule is narrow, and anything it does not recognise is compared as independent ranges, as before:

- A level that is a sum or any other form (`.vdd.voltage - 0.1V`, a clamp, a number alone) is a range, so a fraction against a sum is compared by ranges.
- A level that is stated by more than one statement (also narrowed by a number, or mentioned by another relation) is not only a fraction.
- Two rails on different nets (two supplies, even of the same nominal voltage) are two voltages, so their ranges are independent.
- A rail whose voltage can be zero or negative is not used (the reduction divides by it).
- A division (`.vdd.voltage / 2`) is not read as a fraction, because the factor would be a rounded `1 / k`: write `0.5 * .vdd.voltage`. A factor that is itself a range (`0.65..0.7`) is not read as one either.

#### A line that states its own swing

A module whose outputs swing to an internal supply that has no pad (a cellular module's `STATUS` pin swings to an internal 1.8 V rail that the module does not bring out) cannot state that rail as a `Logic`'s `reference`: a rail in a part needs pads. It states the swing instead (ADR 0081): `.swing`, a voltage range on the `Logic` or on the `Levels` of a bus interface, the range of voltage the line can have **relative to its ground**, the `neg` of its `.reference` rail, which is where the four levels and the absolute maximums are read from. A signed range is read as it is written: `-0.3V .. 1.8V` goes 0.3 V below the ground and 1.8 V above it. The part joins `.reference` to a supply it does have (`~ .vbat`) for the ground only; that rail's voltage is then not what the line swings to.

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
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3" abs_max 2.1V
    }
}

board Telemetry {
    .battery: Rail {
        .voltage &= 3.8V +/- 5%
        .pos.net &= "VBAT"
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3" abs_max 1V
    }
}

board Telemetry {
    .battery: Rail {
        .voltage &= 3.8V +/- 5%
        .pos.net &= "VBAT"
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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
   --> main.nsm:40:5
   |
40 |     .s: Sensor
   |     ^^^^^^^^^^
```

A line that states a swing and no `voh` or `vol` also has its unloaded output levels from it (ADR 0081, amendment 1): the high is the top of the swing and the low is its bottom, as single values. The notes `the high level from ... : ... .voh has no value` and `the low level ... .vol has no value` then close, and the receiver's `vih` and `vil` are checked against those levels. A level the line states always wins, so a part that states `voh` or `vol` is judged exactly as before. A line that states a low and no high is still an open-drain line (ADR 0075): its high is what its pull-up holds the line at, not the top of the swing. A line that states no swing behaves as before. The swing alone does not make a pair of lines worth checking: the pair is looked at when one of them states a level, as before. The same modem reaches 1.8 V, and a receiver that reads a high only from 1.9 V is refused, with the swing named:

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
    }
    .io: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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
   --> main.nsm:41:5
   |
41 |     .m: Modem
   |     ^^^^^^^^^
```

#### A level and the current it was measured at

A datasheet gives an output level at a load: `VOH = VDD - 0.4 V at IOH = 2 mA`. A part states it the same way, with `at` and the current after the value (ADR 0076, amendment 1):

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

`std::parts::logic::LevelTranslator` is the generic of a single-supply translating buffer: a gate whose input accepts a lower level than its own supply and whose output swings between ground and that supply, so a signal from a 1.8 V part reaches a 3.3 V one. It has the rail `vcc` and the lines `a` (an input `Logic`, which must be connected) and `y` (an output `Logic`), and the pin `nc`, in the pad order of the 5-pin single-gate packages (SOT-23-5 and SOT-353_SC-70-5, drawn from the pin table as a box). Join `.a.line` and `.y.line`, as for any `Logic`. A part states what its datasheet gives, as columns of the catalog `level_translator`:

- `.vcc_min` and `.vcc_max`: the supply range; the rail on `vcc` must be inside it.
- `.vcc_abs_max` and `.a_abs_max`: the absolute maximums of the supply and of the input. The design states the highest voltage it can put on each, transients included, as `.vcc_peak` and `.a_peak`, and each must be at most its maximum. The row of `.a.line` ends with `abs_max .a_abs_max`, so the solver also checks the worst voltage of the input's net against it (ADR 0046); a provider has that limit too, and may state a stricter one in its own row of `.a.line` (a looser one fails against the row's `.a_abs_max`).
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
    }
    .high: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
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
    }
    .high: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
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
   --> main.nsm:14:5
   |
14 |     .sensor: Sensor
   |     ^^^^^^^^^^^^^^^
```

### Logic levels of a bus

Every bus interface but `Usb2` has `.levels`, an interface `std::logic::Levels` with the rail `.reference` and the four voltages `.vih`, `.vil`, `.voh` and `.vol` of a `Logic` (ADR 0064). A datasheet gives one set for the supply of a group of pins, so a part states them once for the interface, in the way it states a `Logic`: `.spi.levels.reference ~ .vdd`, `.spi.levels.vih &= 0.7 * .vdd.voltage`, and the absolute maximum of an input at the end of its pin row, `.spi.sclk input "SCL" "1" abs_max 2.1V` (ADR 0126), measured from the ground of `.levels.reference`. Which lines a part drives and which it reads is what its pin table says (`output` and `tri_state` drive, `input` reads, `bidirectional` does both), so `voh` and `vol` judge the lines it drives and `vih` and `vil` the lines it reads. The checks are those of the previous section, with the same numbers and the same notes: a pair of pins of which neither states a level is not looked at, and one that states them on one side only is a note. A part that has two supplies for two groups of pins states two interfaces.

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

    pins {
        .vdd.pos   power     "VDD"   "8"
        .vddio.pos power     "VDDIO" "5"
        .vdd.neg   power     "GND"   "6" "7"
        .spi.sclk  input     "SCL"   "1" abs_max 2.1V
        .spi.mosi  input     "SDI"   "2" abs_max 2.1V
        // Table 14: SDO is "high impedance while CS is high".
        .spi.miso  tri_state "SDO"   "3"
        .spi.cs    input     "CS"    "4" abs_max 2.1V
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

    pins {
        .vcca.pos  power     "VCCA" "1"
        .vccb.pos  power     "VCCB" "2"
        .vcca.neg  power     "GND"  "3"
        .a.sclk    input     "A1"   "4" abs_max 3.6V
        .a.mosi    input     "A2"   "5" abs_max 3.6V
        .a.miso    tri_state "A3"   "6"
        .a.cs      input     "A4"   "7" abs_max 3.6V
        .b.sclk    output    "B1"   "8"
        .b.mosi    output    "B2"   "9"
        .b.miso    input     "B3"   "10" abs_max 2.1V
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
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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
   --> main.nsm:14:5
   |
14 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.sclk` reaches 3.465V above its ground `GND` (`proc.spi.sclk`, a line of `proc.spi.levels.reference`) but `sense.spi.sclk.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:14:5
   |
14 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.mosi` reaches 3.465V above its ground `GND` (`proc.spi.mosi`, a line of `proc.spi.levels.reference`) but `sense.spi.mosi.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:14:5
   |
14 |     .sense: Zsense
   |     ^^^^^^^^^^^^^^
error: net `net-proc.spi.cs` reaches 3.465V above its ground `GND` (`proc.spi.cs`, a line of `proc.spi.levels.reference`) but `sense.spi.cs.abs_max` is 2.1V: it must not exceed 2.1V
   --> main.nsm:14:5
   |
14 |     .sense: Zsense
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
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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

An open-drain line, such as the `sda` of an `I2c` bus, is never driven high: a device only pulls it low or lets go, and a resistor to a rail holds it high. A datasheet states its `VOL` and no `VOH`, because the part does not set the high. The levels check reads such a part as it is (ADR 0075, provisional): a pin of the kind `open_drain` (see _Open-drain outputs_), or a driver of another kind that states `vol` and no `voh`, on a net that a resistor joins to the positive pin of a rail (`I2cPullups`, `PullUp` or a plain `Resistor`), has as its high the rail's voltage range, and the receivers' `vih` is judged against that. A finding names the pull-up resistor, its rail and the receiver's `vih`, and the driver's `vol` is judged against each receiver's `vil` as before. The rule is narrow:

- A driver that states `voh` is judged by it, whatever else is on the net: the datasheet says what the part does. An `open_drain` pin is the exception: its kind says the pull-up sets the high.
- A driver that states no `vol` either says nothing about its output, and gets the notes for its high and its low, as before.
- A net with a resistor to its ground as well is a divider, and this rule does not apply. When the DC analysis solves the line (below) its voltage is the high. When it does not, the note says the high has no value.
- A rail on another ground than the receiver's, or a rail with no voltage, is a note that names it, not a pass.

When the DC analysis solves the line, its voltage is the high, and not the rail's (ADR 0076, amendment 8). The line is solved when every pin on it is modelled: each driver that states `vol at 3mA` (a low with the current it is measured at, as in "DC analysis"), the receivers that draw nothing, and the resistors, diodes and rails between it and a supply. The high is then the voltage of the line with every open-drain pin let go, over the corners of the rails and the resistances: a pull-up through a series resistor and a diode is the rail less the diode's drop, and a pull-up with a pull-down is the divider. The finding says `the DC analysis holds the line at 2.164V..2.327V with every driver of it high impedance ... but ... needs ...`. A line that is not solved (a driver whose `vol` has no test current, a pin that can drive it high, a pin that nothing models) is judged by the rule above, as before. The driver's `vol` is judged as before: the number the datasheet states.

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
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
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
error: PulledLow: U1: there is no KiCad symbol for a `Zihost` yet
```

### Buses

A bus interface names its lines, and its definition in `std::bus` says more: for each role of the bus, which way each line goes (the lines and roles are below). A part states the kind of each pin in its table (`output`, `input`, `bidirectional`, `tri_state`), and the rules of the previous sections judge the nets: two outputs on one net are refused, and so are an `input` with nothing on its net and a net of nothing but inputs. The buses are `std::bus::Uart`, `Spi`, `Qspi`, `I2c`, `Swd` and `Usb2`. The names are the device's own.

- A UART's `tx` and `rx` cross, and the language crosses them: `~` between two `Uart` interfaces joins one end's `tx` to the other's `rx` and its `rts` to the other's `cts`. A board that joins pin by pin writes the crossing (`.a.uart.tx ~ .b.uart.rx`), and a `tx` joined to a `tx` is refused. A device without flow control gives no pin to `rts` and `cts`, and the join leaves them out.
- An SPI bus does not cross. The master's `mosi` is an output and the slave's is an input, and the two join with `~`.
- A quad SPI bus (`Qspi`, a processor and its program flash) has a clock, a select and four data lines `io0` to `io3`, each `bidirectional` for both roles, because in quad mode every data line carries data both ways. It is not an `Spi`, whose `mosi` and `miso` each have one direction.
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
  --> main.nsm:9:5
  |
9 |     .a: Uart32
  |     ^^^^^^^^^^
error: net `net-a.uart.tx` joins `tx` to `tx` (`a.uart.tx`, `b.uart.tx`): `tx` crosses `rx` in a `Uart`, so it is joined to the other end's `rx`; write `.a.uart.tx ~ .b.uart.rx`, or join the two interfaces with `.a.uart ~ .b.uart`, which crosses `tx` to `rx` and `rts` to `cts`
  --> main.nsm:9:5
  |
9 |     .a: Uart32
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
  --> main.nsm:9:5
  |
9 |     .a: Uart32
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
  --> main.nsm:9:5
  |
9 |     .a: SpiMaster
  |     ^^^^^^^^^^^^^
error: net `net-a.spi.miso` has only inputs (`a.spi.miso`, `b.spi.miso`): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor
  --> main.nsm:9:5
  |
9 |     .a: SpiMaster
  |     ^^^^^^^^^^^^^
error: net `net-a.spi.mosi` has two outputs: `a.spi.mosi`, `b.spi.mosi`
  --> main.nsm:9:5
  |
9 |     .a: SpiMaster
  |     ^^^^^^^^^^^^^
error: net `net-a.spi.sclk` has two outputs: `a.spi.sclk`, `b.spi.sclk`
  --> main.nsm:9:5
  |
9 |     .a: SpiMaster
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
  --> main.nsm:9:5
  |
9 |     .a: Sensor
  |     ^^^^^^^^^^
error: net `net-a.i2c.sda` carries the sda line of an I2C bus (`a.i2c.sda`, `b.i2c.sda`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line
  --> main.nsm:9:5
  |
9 |     .a: Sensor
  |     ^^^^^^^^^^
```

In a `system`, a line that goes out through a connector is followed through the mates (ADR 0040) to the board behind it, contact N to contact N, and on through any connector there. The bus is judged as one: it needs a pull-up on the board that has it or on any board it reaches, and a pull-up on either side of a mate satisfies it. If none has one, the error names the connectors the line passed (`carrier.j`, `card.h`) and is reported once for the line. A line that reaches a connector the system does not mate, or a port of a board, goes somewhere this check cannot see and is not judged, as is a board checked alone behind its own connector (ADR 0090).

### The rise time of an I2C bus

The rise time of an I2C line is a closed-form check, not a simulation (`docs/research/closed-form-checks.md`, check 5). The pull-up resistance and the capacitance of the line set how long the line takes to go from 30 % to 70 % of the supply, `tr = 0.8473 * Rp * Cb` (`0.8473` is `ln(7/3)`, and the NXP I2C-bus specification UM10204, Rev. 7.0, section 7.1, derives the same figure and measures the rise time between these two points). `I2cPullups` has `.rise_time` for it and `.bus_capacitance`, the capacitance of one line. A board bounds the rise time with the limit of the mode it runs, and the bound holds in the worst case: the highest resistance (the tolerance the board states) and the highest capacitance. The supply does not enter, since both points are fractions of it. The limits are in UM10204 Table 11: 1000 ns for Standard-mode, 300 ns for Fast-mode, 120 ns for Fast-mode Plus, and at most 400 pF on a line (550 pF in Fast-mode Plus; Table 10 gives each pin at most 10 pF). The module builds none of them in: the board states the one of its mode (`.pullups.rise_time <= 300ns`, `.pullups.bus_capacitance <= 400pF`), because which mode a bus runs is the board's, and a table of the modes in `std::bus` is the owner's to decide. The lower bound of the resistance, from the sink current, is the next section.

The capacitance has two sources, and the checker adds them (ADR 0102, proposed). A part states the capacitance of its own pin as `.i2c.capacitance` (`Ci` of the datasheet), an `optional` member of `I2c` (ADR 0074) that `~` does not equate between two parts. The traces, the connector and the cable are no part's: the board states its own estimate, `.pullups.trace_capacitance &= 20pF..40pF`, an `optional` member of `I2cPullups`. The checker finds the parts on the bus by the nets, the `I2c` interfaces that share a net with the pull-up's `.bus` (the same bus the pull-up finder looks at, ADR 0044), and gives `.bus_capacitance` the sum of their `.capacitance` and `.trace_capacitance`. A part that states no capacitance leaves the sum without a value, so a bound on the rise time is listed as not evaluated, never passed, and a second line names the part (`` `sensor.i2c` states no `.capacitance` ``); a bus whose board states no estimate of the traces derives nothing, since the pins alone would pass a bound the traces break, and the line says what to state. A board that states neither a bound nor an estimate is told nothing. A board may still state `.pullups.bus_capacitance` itself, a range or the sum of its own writing, and is then judged on its figure: the sum is not made for it, since an equation against a narrower range the board states would read as a figure that cannot be proved and not as a narrowing. Two invented parts that state the capacitance of their pins, 5 pF and 8 pF, and one that states none:

```nsm module=src::zicap
import std::bus::{I2c, I2cRole}
import std::sources::Rail

/// A controller with the capacitance of its pin from an invented datasheet: 5 pF.
part Zcc {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.capacitance &= 5pF
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target with 8 pF on its pin.
part Zct {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.capacitance &= 8pF
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose datasheet states no capacitance for its pin.
part Zcn {
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

A bus of the two that state it, with 20 pF to 40 pF for the traces, has a capacitance of 33 pF to 53 pF, is at worst 4.935 kohm over 53 pF, 221.6 ns (the figure is worked out by hand in `crates/netsmith-solve/tests/bus_figures.rs`), and Fast-mode's 300 ns holds:

```nsm evaluate
import src::zicap::{Zcc, Zct}
import std::bus::I2cPullups
import std::sources::Rail

board FastBus {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .ctl: Zcc
    .sensor: Zct
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .pullups.supply
    .ctl.i2c ~ .sensor.i2c
    .ctl.i2c ~ .pullups.bus
    .pullups.trace_capacitance &= 20pF..40pF
    .pullups.rise_time <= 300ns
}
```

At 200 ns the same bus is refused with the numbers, the range of the rise time and the limit:

```nsm fails
import src::zicap::{Zcc, Zct}
import std::bus::I2cPullups
import std::sources::Rail

board TooSlow {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .ctl: Zcc
    .sensor: Zct
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .pullups.supply
    .ctl.i2c ~ .sensor.i2c
    .ctl.i2c ~ .pullups.bus
    .pullups.trace_capacitance &= 20pF..40pF
    .pullups.rise_time <= 200ns
}
```

```text
error: pullups.rise_time must always be at most 200ns, but 124.8ns..221.7ns is not always below 200ns
   --> main.nsm:19:5
   |
19 |     .pullups.rise_time <= 200ns
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A part that states no capacitance leaves the sum without a value, so the bound is listed as not evaluated and not passed, though the pins that are stated would pass it, and the build names the part (`crates/netsmith-solve/tests/bus_figures.rs` has the line):

```nsm evaluate
import src::zicap::{Zcc, Zcn}
import std::bus::I2cPullups
import std::sources::Rail

board NoPinFigure {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .ctl: Zcc
    .sensor: Zcn
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .pullups.supply
    .ctl.i2c ~ .sensor.i2c
    .ctl.i2c ~ .pullups.bus
    .pullups.trace_capacitance &= 20pF..40pF
    .pullups.rise_time <= 300ns
}
```

The check is wrong, and optimistic, where the capacitance is underestimated (traces, connectors and cable are the large part of a real bus, and a pin's figure is a maximum the datasheet gives under its own conditions), where two pull-up pairs are on one bus (they are in parallel, and the board states the parallel resistance), for a current-source pull-up or a bus buffer (neither is an `RC`), and for levels that are not 30 % and 70 % of the supply, as the specification takes them whatever a part states.

### The least pull-up of an I2C bus

The other bound on a pull-up is from below. A device only pulls a line low, so the weakest driver on the bus must be able to hold it there against the pull-up: its datasheet guarantees a low level `VOL` while it sinks `IOL`, and the resistor then carries `(Vdd - VOL) / IOL` at most, so `Rp >= (Vdd - VOL) / IOL` (UM10204 Rev. 7.0, section 7.1; Table 10 gives `VOL` 0.4 V at 3 mA, and 20 mA in Fast-mode Plus). `I2cPullups` has the relation, and `.sink_current` and `.low_level`, the weakest driver's `IOL` and the `VOL` at that current, which the board states. The relation holds in the worst case: the supply at its greatest (`supply.voltage`, so a rail of 3.3 V +/- 5 % counts as 3.465 V), the figures as stated (a range is taken at the end that asks for the most: the least `VOL` and `IOL`), and the resistance at its low end, the tolerance the board states.

A part states its own figure, as it states its pin's capacitance: `.i2c.sink_current` is an `optional` member of `I2c` (ADR 0074), for the datasheet's `IOL` at the `.i2c.levels.vol` it states with it, and `~` does not equate it between two parts. The checker makes the relation for each part on the bus (ADR 0102, proposed): one `.resistance >= (supply.voltage - part.levels.vol) / part.sink_current` for each `I2c` interface that shares a net with the pull-up's `.bus`, so the board states nothing and a failure names the part. Each part has its own pair of `IOL` and `VOL`, since no single pair stands for the bus: the least current and the greatest level can belong to two parts, and `min(.f)` reads the end of a range inside a `<=` or `>=` and never gives a value (ADR 0098). A part that states no `.sink_current` has no relation, and when another part on the bus states one, the build names it (`` `sensor.i2c` states no `.sink_current` ``) and says the bound was made for the others only. A bus on which no part states one has no bound and nothing is listed, as before.

A board may still state the figure itself: `.pullups.sink_current &= .sensor.i2c.sink_current` and `.pullups.low_level &= .sensor.i2c.levels.vol` (the members of `I2cPullups`, `optional`) are the relation of the module, judged on the figures the board gives, and the parts' relations are not made for it, since both would say one fault twice. A board that states the current and no `.low_level` is listed as not evaluated, and so is one that takes the figure from a part that states none; neither passes.

The module builds no figure in: UM10204's `VOL` and `IOL` are the specification's limits for a device, a part's datasheet may state better, and no std part or catalog has an I2C interface to fill. A part with a datasheet figure states it with the page cited.

```nsm module=src::ziol
import std::bus::{I2c, I2cRole}
import std::sources::Rail

/// A controller from an invented datasheet: it sinks 6 mA at 0.4 V.
part Zlc {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.sink_current &= 6mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target that sinks 3 mA at 0.4 V, the specification's figure.
part Zlt {
    .designator &= "U"
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    .i2c.sink_current &= 3mA
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}
```

On 3.3 V +/- 5 %, the target's 3 mA at 0.4 V asks for at least `(3.465 V - 0.4 V) / 3 mA = 1021.7 ohm` (worked out by hand in `crates/netsmith-solve/tests/bus_figures.rs`), and the controller's 6 mA asks for half of that, so 4.7 kohm +/- 5 % holds for both parts, and the board states nothing about the sink current:

```nsm evaluate
import src::ziol::{Zlc, Zlt}
import std::bus::I2cPullups
import std::sources::Rail

board WeakestDriver {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .ctl: Zlc
    .sensor: Zlt
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .pullups.supply
    .ctl.i2c ~ .sensor.i2c
    .ctl.i2c ~ .pullups.bus
}
```

At 800 ohm the same bus is refused with the numbers, the part whose figures ask for the most, the range of the resistance and the range of the least it may be (its top end is the worst case, the supply at 3.465 V):

```nsm fails
import src::ziol::{Zlc, Zlt}
import std::bus::I2cPullups
import std::sources::Rail

board TooStrong {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .ctl: Zlc
    .sensor: Zlt
    .pullups: I2cPullups { .resistance &= 800ohm +/- 5% }
    .supply ~ .ctl.vdd
    .supply ~ .sensor.vdd
    .supply ~ .pullups.supply
    .ctl.i2c ~ .sensor.i2c
    .ctl.i2c ~ .pullups.bus
}
```

```text
error: pullups.resistance must always be at least ((pullups.supply.voltage - sensor.i2c.levels.vol) / sensor.i2c.sink_current), but 760Ω..840Ω is not always above 911.6Ω..1.022kΩ
   --> main.nsm:12:5
   |
12 |     .pullups: I2cPullups { .resistance &= 800ohm +/- 5% }
   |     ^^^^^^^^^^^^^^^^^^^^
```

The two bounds make a window. On a heavy Fast-mode bus (400 pF and `.pullups.rise_time <= 300ns`) the rise time allows at most `300 ns / (0.8473 * 400 pF) = 885.2 ohm`, and a 3 mA driver asks for at least 1021.7 ohm: the window is empty, and whatever resistance the board states, one of the two bounds fails and its message says which (`pullups.rise_time must always be at most ...` or `pullups.resistance must always be at least ...`). With a 6 mA driver the window is 510.8 ohm to 885.2 ohm, and 700 ohm +/- 5 % is inside it. `crates/netsmith-solve/tests/bus_sink_current.rs` has both buses.

The check is wrong, and optimistic, where the figures are not the weakest's (a part on the bus that states no `.sink_current` is named and not judged, and a figure the board states by hand is taken as it is), where `VOL` at the stated current is not the level the receivers need (`levels.vol` is judged against their `vil` by the levels check, which is a different one), where two pull-up pairs are on one bus (the board states the parallel value), and for a current-source pull-up or a bus buffer. It reads the line as held low by one driver at a time.

### Roles and lines of a bus

A bus interface can say more than the names of its lines. `lines` is a statement of an interface, and it says, for each role of the bus, the kind of each line and a few properties of a line (ADR 0064). `std::bus` declares them for the six buses, so the protocol is written once, in a file that a reader can open:

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
  --> main.nsm:9:5
  |
9 |     .cpu: Zcpu
  |     ^^^^^^^^^^
error: net `net-cpu.spi.mosi` joins two different lines of a `Spi`: `miso` (`imu.spi.miso`) and `mosi` (`cpu.spi.mosi`, `flash.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `imu.spi.miso` is on the net of `mosi`
  --> main.nsm:9:5
  |
9 |     .cpu: Zcpu
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
   --> main.nsm:10:5
   |
10 |     .host: Zcpu
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
   --> main.nsm:10:5
   |
10 |     .flash: Zflash
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
    }
    .imu: Zimu
    .supply ~ .imu.vdd
}
```

```text
error: `imu.spi.mosi` is an output in the table of `Zimu`, but `SpiRole::Slave` makes `mosi` an input of `Spi`; either the datasheet's `SDI` pin is not an output, or the role is the other one
   --> main.nsm:25:5
   |
25 |     .imu: Zimu
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
   --> main.nsm:10:5
   |
10 |     .flash: Zflash
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
   --> main.nsm:10:5
   |
10 |     .imu: Zimu
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

- `.supply` is the rail whose `pos` the resistor is on, `.line` is the pin it pulls, and the resistor inside is `.resistor`, whose value the board states with its tolerance (`.pull.resistor.resistance &= 10kohm +/- 5%`, ADR 0116). A board that forgets it gets the unconstrained pick finding for the resistor (see "A value that nobody stated").
- The line is held low for the share `.line_low` of the time, 0 to 1 and never narrower unless the board says so (`.pull.line_low &= 0..0.1`). While it is low the resistor carries `supply.voltage / resistor.resistance`, and while it is released nothing, so the resistor's `.current`, and the draw on `supply`, is 0 up to the supply over the resistor's resistance. That is the worst case: the lowest the line goes is 0 V, and a driver's real low level draws a little less. It is the steady draw, with two assumptions the module offers and the board may replace (ADR 0070): `.overdrive`, how far above the supply a driver holds the line, 0 V unless stated (`.pull.overdrive &= 0V..1.7V` for a 5 V driver on a 3.3 V pull-up: the draw then reaches `-overdrive / resistor.resistance`, current pushed back into the rail), and `.supply_top`, the voltage the draw is worked out at, `supply.voltage` unless stated (`.pull.supply_top &= .pull.supply.peak` for the draw at the transient the rail's stated `.peak` allows).
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
    }
    .gate: ResetGate
    .supply ~ .gate.vcc

    // The reset line is held high by 10 kohm and is low a tenth of the time at most.
    .pull: PullUp {
        .resistor.resistance &= 10kohm +/- 5%
        .line_low &= 0..0.1
    }
    .supply ~ .pull.supply
    .pull.line ~ .gate.reset_n
}
```

A board that forgets the resistance is told so, with the path to state:

```nsm solve warns
import std::bus::PullUp
import std::sources::Rail

board Forgetful {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .pull: PullUp
    .supply ~ .pull.supply
}
```

```text
Forgetful: unconstrained pick guideline: `pull.resistor` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104: state it, `.pull.resistor.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.pull.resistor.resistance &= 10kohm..100kohm`
```

A module that wraps a `PullUp` states the resistance itself (`.pull.resistor.resistance &= 10kohm +/- 5%`) or leaves it to the board, which narrows it by path (`.reset.pull.resistor.resistance &= 10kohm +/- 5%`). A member of the wrapper that only passes the value on (`.pull.resistor.resistance &= .resistance`) says one fact twice (ADR 0116); keep one only when it ties several parts or relates the value to a rail.

A board that needs a size for the resistor narrows it inside the module (ADR 0116): `.pull: PullUp { .resistor.resistance &= 10kohm +/- 1%; .resistor.package &= "0402" }`, and the same for `PullDown`. `I2cPullups` and `FeedbackDivider` have an `optional` `.package` (a text such as `"0402"`) that gives one package to both of their resistors. A board that states nothing leaves the pick to the solver, as before: the relation that ties the member to the resistors is skipped while nothing states it (see _A value that may stay open_), so no pick, lock or message changes for such a board. A package that no catalog row has is refused with the usual "has nothing for" message, which names the resistor and the package.

```nsm evaluate
import std::bus::PullUp
import std::sources::Rail

/// A gate with an active-low reset. Illustrative; not a real part.
part SizedGate {
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 100uA..1mA

    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .reset_n input "RST#" "3"
    }
}

board Sized {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .gate: SizedGate
    .supply ~ .gate.vcc

    // The same pull-up as above, in the 0402 package.
    .pull: PullUp {
        .resistor.resistance &= 10kohm +/- 1%
        .resistor.package &= "0402"
    }
    .supply ~ .pull.supply
    .pull.line ~ .gate.reset_n
}
```

### A pull-down on one line

A boot-mode pin, an enable or a gate often needs a resistor from the line to ground, so that it reads low when its driver lets go. A plain `Resistor` from the line to the rail's `neg` works for the circuit and leaves the resistor's current unknown: nothing in the circuit says what the driver puts into it, and its drop and power wait. `std::bus::PullDown` is that resistor with its current stated, the mirror of `PullUp`.

- `.reference` is the rail whose `neg` the resistor is on, `.line` is the pin it holds low, and the resistor inside is `.resistor`, whose value the board states with its tolerance (`.pull.resistor.resistance &= 100kohm +/- 5%`, ADR 0116). A board that forgets it gets the unconstrained pick finding for the resistor.
- The line is high for the share `.line_high` of the time, 0 to 1 and never narrower unless the board says so (`.pull.line_high &= 0..0.1`). While it is high the resistor carries the line's voltage over its resistance, and while it is low or released nothing, so the resistor's `.current` is 0 up to `.line_top / resistor.resistance`. `.line_top`, the highest voltage the line is held at above `reference`'s `neg`, is `reference.voltage` unless the board states it (`.pull.line_top &= 0V..5.25V` for a 5 V driver on a line judged against 3.3 V; ADR 0070's way of offering an assumption).
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
    }
    .gate: EnableGate
    .supply ~ .gate.vcc

    // The enable line is held low by 100 kohm and is high a tenth of the time at most.
    .pull: PullDown {
        .resistor.resistance &= 100kohm +/- 5%
        .line_high &= 0..0.1
    }
    .supply ~ .pull.reference
    .pull.line ~ .gate.enable
}
```

A board that forgets the resistance is told so, with the path to state:

```nsm solve warns
import std::bus::PullDown
import std::sources::Rail

board Forgetful {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .pull: PullDown
    .supply ~ .pull.reference
}
```

```text
Forgetful: unconstrained pick guideline: `pull.resistor` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104: state it, `.pull.resistor.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.pull.resistor.resistance &= 10kohm..100kohm`
```
