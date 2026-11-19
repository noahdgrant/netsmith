## Load current

A `Rail` has a `current` field: the current drawn from the net through the rail's `pos` pin by whatever is behind the rail. It is positive when the thing draws and negative when it supplies (ADR 0007; see _The sign of a current_ at the end of this section). A design states the consuming side.

```nsm solve
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

board Supply {
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .stage: Stage { .vout.voltage &= 3.3V +/- 5% }
    .output: Rail { .pos.net &= "3V3"; .current &= 0mA..50mA }
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
- `LinearRegulator` states that its input draws what its output supplies and its own quiescent current (`.quiescent_current`, none unless a part states it from its datasheet: `.quiescent_current &= 0A..140uA`, ADR 0076, amendment 9), that the output current is at most `.max_current`, and that `(vin - vout) * current + vin * quiescent` is at most `.max_power`.
- The first part of a `~>` chain that starts on a rail's net adds its through-current to that net's sum (ADR 0014). The solver sets the current of a rail from the rest of the net; a part's current is set from a net only when the net has two terms (a fuse in series with a load), and is otherwise verified, never solved by subtraction (ADR 0026). If its own relations do not determine it, the relations that use it are `not evaluated`. A balance is set or verified only when it is complete (ADR 0027): every part pin on the net is a term or belongs to a part that draws nothing at DC (one that states no `current`, has no rail and has only `passive` pins: a capacitor, a header, a test point). An `input` or `no_connect` pin of a part that has a `Rail` also draws nothing (its leakage is ignored; the rail is the part's supply). A pin that is none of these, such as an inductor joined with `~`, an IC's output or bidirectional pin, or an input of a part with no rail, leaves the balance incomplete, and the build lists those pins in a note.
- A net with two or more unknown currents stays unsolved, and `build` prints a note that names the net and the currents (`net `VBUS` has 2 unknown currents, `ldo.vin.current`, `usb.vbus.current`, so none is worked out from the net: state all but one of them`). A module's own rail and the rail of the part inside it that it is joined to are one current, so the module's balance of just the two has no note. State a current to leave one unknown, and the note goes.
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
    .supply: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= "3V3" }
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
    .supply: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= "3V3" }
    .monitor: Monitor { .vin.current &= 0A..10uA }
    .supply ~ .monitor.vin
}
```

```text
error: the currents on net `3V3` do not add up: `-monitor.vin.current` must stay within -10uA..0A, but the others on it work out to -17.17uA..-15.84uA (all of them: `monitor.r_top.current` is 15.84uA..17.17uA, `-monitor.vin.current` is -10uA..0A)
   --> main.nsm:14:25
   |
14 |     .monitor: Monitor { .vin.current &= 0A..10uA }
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
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .stage: Stage { .vout.voltage &= 3.3V +/- 5% }
    .output: Rail { .pos.net &= "3V3"; .current &= 0mA..50A }
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
   --> std/parts/regulator.nsm:73:5
   |
73 |     -.vout.current <= .max_current
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
    .supply: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= "3V3" }
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

### Loads that take turns

Every load on a net draws at once unless the design says otherwise: three heaters of up to 60 mA each are 180 mA at the regulator, even when the firmware runs one at a time. `shared NAME [limit N] { .load ... }` in a board or a module says that at most N of the named loads are on at once, one when it states no `limit` (issue #806). It is the statement a part uses for peripherals that share hardware (_Peripherals that share hardware_), and in a board or a module its members are loads: rails that draw, by their path (a rail of the board or the module, a rail of a part, or a module's port, which stands for what the module draws).

```nsm solve
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Heaters {
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .regulator: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .regulator.vin
    .heater_a: Rail { .current &= 0mA..60mA }
    .heater_b: Rail { .current &= 0mA..60mA }
    .heater_c: Rail { .current &= 0mA..60mA }
    .regulator.vout ~ .heater_a
    .regulator.vout ~ .heater_b
    .regulator.vout ~ .heater_c
    .heater_a.pos.net &= "3V3"
    shared HEATERS { .heater_a .heater_b .heater_c }
    .in_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
}
```

The regulator supplies 0 to 60 mA, which its 100 mA allows. A load that is off draws the low end of its range and one that is on anything in it, so the members of a group on one net add up to the sum of their low ends and the N largest spans: a heater of `10mA..30mA` and two of `0mA..60mA` with `limit 1` are 10 mA to 70 mA. Every check that sums the currents of a net reads the group: the balance of the net (and so the current the supply gives and a regulator's `.max_current`), a stated budget, the DC analysis, which leaves out the corners with more than N members at the top of their range, the width of the net's tracks, and the demand of the power view. A load that is in no group is always on with the others. With `limit 2` the regulator is refused, and the message names the group:

```nsm fails
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Heaters {
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .regulator: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .regulator.vin
    .heater_a: Rail { .current &= 0mA..60mA }
    .heater_b: Rail { .current &= 0mA..60mA }
    .heater_c: Rail { .current &= 0mA..60mA }
    .regulator.vout ~ .heater_a
    .regulator.vout ~ .heater_b
    .regulator.vout ~ .heater_c
    .heater_a.pos.net &= "3V3"
    shared HEATERS limit 2 { .heater_a .heater_b .heater_c }
    .in_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
}
```

```text
error: no `ldo` candidate satisfies `regulator`: the closest of the 1 that fit, XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446, fails because -regulator.vout.current must always be at most regulator.max_current, but 0A..120mA is not always below 100mA (the currents on net `3V3`: heater_a.current (0A..60mA) + heater_b.current (0A..60mA) + heater_c.current (0A..60mA) + regulator.vout.current (-120mA..0A) add up to zero, with at most 2 of `HEATERS` on at once)
   --> std/parts/regulator.nsm:73:5
   |
73 |     -.vout.current <= .max_current
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A group in a module holds for each instance of the module. A load is in one group, and a group lists each load once, at least two of them, and a limit fewer than its loads (one that allows every load at once says nothing). The members are not worked out from the rest of the net: a load that is on or off is not what the others leave. The group says which loads run together and nothing about when: a load's peak (below) is checked alone, as before.

The power view and `netsmith board power` weigh the demand of a rail against what its regulator can give, the rating the part states for its output: `.max_current` of a `LinearRegulator`, `.max_output_current` of a `Buck`'s IC (issue #1316). A buck converter that feeds a rail has the IC's rating as the rail's capacity, so loads that can draw more than it are a power issue (an error) and `board power` no longer says `no limit stated for the supply`; no new member is needed. A source that is no regulator with such a rating (a connector, a rail the board states) still has no limit there.

### A peak current

The DC analysis has no time in it, and a capacitor is open at DC, so a store charged through a resistor draws nothing in the balance. When it is empty, though, it takes the whole rail across the resistor for an instant, and that is the largest current the source in front of it sees. A rail states that current with `.peak_current`, the most its load draws at an instant, as a positive number (issue #750). It is checked and never drawn: the balance of the net, the power of a part, a shunt's window and every other check of the steady state read `.current` and never this one.

What the source can give is the rail's `.max_peak_current`, which a regulator states for its output: `Buck` its IC's `.max_output_current`, `LinearRegulator` its `.max_current`. `~` shares it, as it shares `.peak`, so every rail joined to the regulator's output has it, and `Rail` checks its own `.peak_current` against it. `.peak_current` is not shared: two loads on one regulator each state their own, and each is checked alone. The steady current of the other loads is not added to it, so a board whose loads start together states the sum on one of them. Both members are `optional` (ADR 0074): a rail that states no peak has no check, and the build says nothing about it.

Here a 1 ohm resistor charges 10 uF from a 3.3 V regulator rated 0.6 A:

```nsm fails
import std::parts::passive::{Capacitor, Resistor}
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

/// A store: a capacitor charged through a resistor. At DC the capacitor is open, so the steady
/// current is nil, but an empty store takes the whole rail across the resistor for an instant.
module Store {
    .supply: Rail
    .cell: Capacitor { .capacitance &= 10uF +/- 20% }
    .limit: Resistor { .resistance &= 1ohm +/- 1% }
    .supply.pos ~> .limit ~> .cell ~> .supply.neg
    .supply.current &= 0A..10uA
    .supply.peak_current == .supply.voltage / .limit.resistance
}

board Inrush {
    .input: Rail { .voltage &= 9V..16V; .peak &= 24V; .pos.net &= "VIN" }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V3V3"; .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
    .store: Store
    .reg.vout ~ .store.supply
}
```

```text
error: store.supply.peak_current must always be at most store.supply.max_peak_current, but 3.103A..3.5A is not always below 600mA
   --> std/sources.nsm:54:5
   |
54 |     .peak_current <= .max_peak_current
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The rail's voltage is the regulator's output, whose range the solver widens by the tolerance of the divider, so the peak is 3.1 A to 3.5 A. A 10 ohm resistor limits it to 0.35 A, and the board builds; a load that states its peak directly states `.load.peak_current &= 0.4A`.

### A load that bursts

A radio draws little most of the time and much while it transmits: 0.5 A for a few hundred microseconds, and 10 to 40 mA on average. One range hides which is which. A rail that states `.burst`, how long one burst lasts, is such a load (issue #1279): its `.current` is its average, the bursts included, and its `.peak_current` is what it draws during a burst. The average is what the balance of the net, a regulator's `.max_current`, its dissipation and every other check of the steady state read. The peak is what the source must give, and the source gives it while the other loads on the net keep drawing, so after the solve the peak and the other loads' steady current (each at its most; a group of loads that take turns at its worst) are added and held against the rail's `.max_peak_current`.

What the source cannot give comes from the capacitors across the rail: every part with a bias between the rail's `pos` and `neg` nets, a bank by its `.total_effective_capacitance`, at the least of each. They give the rest for the length of the burst, and the rail falls by that charge over their capacitance from the least of its steady voltage. It must stay at or above the least voltage the parts on the net need, the highest that a `.voltage >= ...` of a rail on the net states. A net on which no part states one cannot lend the burst anything.

```nsm evaluate
import std::parts::passive::CapacitorBank
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Radio {
    .input: Rail { .voltage &= 9V..16V; .peak &= 24V; .pos.net &= "VIN" }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V3V3"; .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .mcu: Rail { .current &= 5mA..150mA }
    .reg.vout ~ .mcu
    .radio: Rail {
        .current &= 10mA..40mA
        .peak_current &= 500mA
        .burst &= 200us
        .duty &= 0.05
        .voltage >= 3V
    }
    .reg.vout ~ .radio
    .bulk: CapacitorBank
    .bulk.total_effective_capacitance >= 80uF
    .reg.vout.pos ~> .bulk ~> .reg.vout.neg
}
```

The regulator gives 0.6 A, and during a burst the radio and the MCU draw 650 mA. The 50 mA the regulator cannot give for 200 us is 10 uC, and the bank and the regulator's own output capacitor hold it with the rail above the radio's 3 V. Without the bank the build fails, and the message says how much capacitance holding the burst takes:

```nsm fails
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Radio {
    .input: Rail { .voltage &= 9V..16V; .peak &= 24V; .pos.net &= "VIN" }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V3V3"; .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .mcu: Rail { .current &= 5mA..150mA }
    .reg.vout ~ .mcu
    .radio: Rail { .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= 200us; .voltage >= 3V }
    .reg.vout ~ .radio
}
```

```text
error: during a burst of `radio` (`radio.peak_current` 500mA for 200us) the other loads on its net draw up to 150mA, so the source gives up to 650mA, above `radio.max_peak_current` (600mA); the 16.09uF across the rail give the other 50mA for 200us, and the rail falls by 621.4mV from 3.135V, below the 3V that `radio` needs: holding the burst takes 74.07uF of effective capacitance across the rail
   --> main.nsm:10:68
   |
10 |     .radio: Rail { .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= 200us; .voltage >= 3V }
   |                                                                    ^^^^^^^^^^^^^^^
```

Rules.

- `.burst` and `.duty` are optional (ADR 0074) and the load's own: `~` shares neither, as it does not share `.peak_current`. A rail with a peak and no burst is an inrush, checked alone (_A peak current_). A rail whose source states no `.max_peak_current` is not looked at.
- `.duty` is the fraction of the time the load bursts (one 3 ms slot in every 100 ms is `0.03`). The bursts alone average the peak times the duty, so the most of the stated average must be at least that: `max(.current) >= max(.peak_current) * max(.duty)`, which a radio of `10mA..40mA` with `.duty &= 0.1` and a 500 mA peak fails (40 mA is below 50 mA).
- A peak above the source's limit alone is refused by `Rail`'s own `.peak_current <= .max_peak_current`, which also makes the solver pick a regulator that gives it. The capacitance across the rail helps only with the other loads' share.
- The capacitance is read at 25 C and new, and its ESR is not added to the fall (issue #1256 has the corners). A burst seen on the input of the converter that feeds the load is not judged (issue #1340).
