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
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
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
- `LinearRegulator` states that its input draws what its output supplies and its own quiescent current (`.quiescent_current`, none unless a part states it from its datasheet: `.quiescent_current &= 0A..140uA`, ADR 0076, amendment 9), that the output current is at most `.max_current`, and that `(vin - vout) * current + vin * quiescent` is at most `.max_power`.
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
    }
    .monitor: Monitor { .vin.current &= 0A..10uA }
    .supply ~ .monitor.vin
}
```

```text
error: the currents on net `3V3` do not add up: `-monitor.vin.current` must stay within -10uA..0A, but the others on it work out to -17.17uA..-15.84uA (all of them: `monitor.r_top.current` is 15.84uA..17.17uA, `-monitor.vin.current` is -10uA..0A)
   --> main.nsm:17:25
   |
17 |     .monitor: Monitor { .vin.current &= 0A..10uA }
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
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
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
   --> std/parts/regulator.nsm:42:5
   |
42 |     -.vout.current <= .max_current
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
