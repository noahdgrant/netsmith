## Connections: `~` and `~>`

`~` puts two pins on one net, or joins two interfaces of the same kind: their pins pair by name and their values become equal. Between two other ports (parts, modules, interfaces of different kinds) it joins their members by name (below). `~>` runs a series chain from a pin through parts to another pin. Each part in a chain needs a `bridge`.

```nsm
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board Chains {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
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

- `~` joins pin to pin, interface to interface when both are the same definition, and the members of two other ports by name. It does not join a pin and an interface.
- `~>` starts at a pin. A part in the chain is entered at the first pin of its bridge and left at the second. Chains may continue from a part, `.a ~> .b`, and may end at a pin.
- `a ~> pin ~> b` puts `a`, the pin and `b` on one net.
- A chain across a rail, from a logic pin or between two rails sets its own current (below). Any other chain is a statement about nets and about which currents are equal.
- A system joins ports with `~` only (see Systems).
- A group of identical members alone between a chain's two ends puts each member in parallel between them, and a group may be written in the chain (`.supply.pos ~> 2 x Capacitor { ... } ~> .supply.neg`, A group of identical members, above).

### Two ports, member by member

`.a ~ .b`, where `a` and `b` are parts, modules or interfaces of different kinds, joins each pin or interface of `a` to the member of `b` with the same name, as one `~` line each would. The ports may be at any path (`.sensor.bus ~ .probe.bus`). A rename map pairs members whose names differ, the member of `a` first: `.a ~ .b { gpio4 = io4; gpio5 = io5 }`, one rename per line or several separated by `;`. `except { gnd vdd }` leaves out members on both sides. The join is strict: a member with no partner on the other side is an error at the line, unless it is renamed or left out, so nothing is left unjoined by accident.

```nsm
import std::sources::Rail

part Header {
    .designator &= "J"
    pins {
        .gpio4 passive "GPIO4" "1"
        .gpio5 passive "GPIO5" "2"
        .en    passive "EN"    "3"
        .gnd   passive "GND"   "4"
    }
}

part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
        .io5     bidirectional "IO5" "4"
        .en      bidirectional "EN"  "5"
    }
}

board Pinout {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .header: Header
    .chip: Chip
    .supply ~ .chip.vdd
    .header.gnd ~ .supply.neg
    .header ~ .chip { gpio4 = io4; gpio5 = io5 } except { gnd vdd }
}
```

This joins `.header.gpio4 ~ .chip.io4`, `.header.gpio5 ~ .chip.io5` and `.header.en ~ .chip.en`. The nets are those of the three lines, and so are their names.

Rules.

- The members are the pins and the interfaces that hold a pin. Values, the parts inside a module and the built-in `.place` and `.model` take no part.
- Two members of the same name join as `~` joins them: a pin to a pin, an interface to an interface of the same kind whole (a rail with a rail), and two interfaces of different kinds by name in turn. A pin against an interface is an error; rename one or leave it out.
- Two interfaces of the same kind join whole, as they always have; a rename map or `except` is refused for them, and for two pins.
- A rename or an `except` names a member that exists, one level deep (`vdd`, not `vdd.pos`), and a member has one partner.
- A rename map or `except` follows a join of two ports with one `~`: it is not part of a chain.

Misuse: a member with no partner. The message names each one, with a near name on the other side where there is one.

```nsm fails
import std::sources::Rail

part Header {
    .designator &= "J"
    pins {
        .gpio4 passive "GPIO4" "1"
        .gpio5 passive "GPIO5" "2"
        .en    passive "EN"    "3"
        .gnd   passive "GND"   "4"
    }
}

part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
        .io5     bidirectional "IO5" "4"
        .en      bidirectional "EN"  "5"
    }
}

board Pinout {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .header: Header
    .chip: Chip
    .supply ~ .chip.vdd
    .header.gnd ~ .supply.neg
    .header ~ .chip { gpio4 = io4 } except { gnd vdd }
}
```

```text
error: these members have no member of the same name on the other side: `.header.gpio5` (did you mean `gpio5 = io5`?), `.chip.io5`; give each a partner with a rename, `{ a = b }`, or leave it out with `except { a }`
   --> main.nsm:35:5
   |
35 |     .header ~ .chip { gpio4 = io4 } except { gnd vdd }
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a rail against a pin of the same name.

```nsm fails
import std::sources::Rail

part PinSupply {
    .designator &= "U"
    pins {
        .vdd passive "VDD" "1"
        .io4 passive "IO4" "2"
    }
}

part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
    }
}

board Supplies {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .chip: Chip
    .other: PinSupply
    .supply ~ .chip.vdd
    .other ~ .chip
}
```

```text
error: `.other.vdd` is a pin and `.chip.vdd` is a `Rail`; `~` joins a pin to a pin and an interface to an interface, so rename one or leave it out with `except`
   --> main.nsm:30:5
   |
30 |     .other ~ .chip
   |     ^^^^^^^^^^^^^^
```

Misuse: a rename to a member that does not exist.

```nsm fails
import std::sources::Rail

part Header {
    .designator &= "J"
    pins {
        .gpio4 passive "GPIO4" "1"
        .gpio5 passive "GPIO5" "2"
        .en    passive "EN"    "3"
        .gnd   passive "GND"   "4"
    }
}

part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
        .io5     bidirectional "IO5" "4"
        .en      bidirectional "EN"  "5"
    }
}

board Pinout {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .header: Header
    .chip: Chip
    .supply ~ .chip.vdd
    .header.gnd ~ .supply.neg
    .header ~ .chip { gpio4 = io44; gpio5 = io5 } except { gnd vdd }
}
```

```text
error: the rename's right side is a member of `.chip`, and `io44` is not a pin or an interface of it; did you mean `io4`?
   --> main.nsm:35:31
   |
35 |     .header ~ .chip { gpio4 = io44; gpio5 = io5 } except { gnd vdd }
   |                               ^^^^
```

Misuse: leaving out a member that does not exist.

```nsm fails
import std::sources::Rail

part Header {
    .designator &= "J"
    pins {
        .gpio4 passive "GPIO4" "1"
        .gpio5 passive "GPIO5" "2"
        .en    passive "EN"    "3"
        .gnd   passive "GND"   "4"
    }
}

part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
        .io5     bidirectional "IO5" "4"
        .en      bidirectional "EN"  "5"
    }
}

board Pinout {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .header: Header
    .chip: Chip
    .supply ~ .chip.vdd
    .header.gnd ~ .supply.neg
    .header ~ .chip { gpio4 = io4; gpio5 = io5 } except { gnd vd }
}
```

```text
error: `except` leaves out `vd`, but neither `.header` nor `.chip` has a pin or an interface of that name; did you mean `vdd`?
   --> main.nsm:35:63
   |
35 |     .header ~ .chip { gpio4 = io4; gpio5 = io5 } except { gnd vd }
   |                                                               ^^
```

### The current of a chain

A chain that starts on a rail's `pos`, ends on the same rail's `neg` and has nothing else on the nets between its parts passes one current, and the rail's voltage is shared between its parts (Kirchhoff's voltage law). A chain driven by a logic pin, or one between two rails, is read the same way (see below). When every part in it states a `dc resistor` or a `dc diode` (a DC element, see "What a part is at DC"), the checker writes that law as a relation on the chain's current, before anything is picked:

```nsm-fragment
.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
```

The current is the one a statement of the design narrowed (here the LED's window), or the first part's: the chain passes one current, so they are the same.

A resistor drops its current times its resistance, and a diode or an LED its forward voltage. The relation is that of every diode conducting. A supply that could fall below the forward voltages makes the current negative at that end, and an LED's `.current >= 0mA` refuses it: an LED that may stay dark is a fault. So an LED's current window picks its resistor, and the board writes no equation:

```nsm solve report
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Indicator {
    .process: TwoLayer
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor {}
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
board Indicator
  D1   Led             KT-0603R        LED_SMD:LED_0603_1608Metric                                C2286 Basic
  R1   Resistor        120             Resistor_SMD:R_0603_1608Metric                             C22787 Basic
  net GND: D1.1
  net VCC: R1.1
  net net-led.anode: D1.2, R1.2
```

The 120 ohm resistor keeps the LED between 5 mA and 15 mA with the supply anywhere in 3.135 V to 3.465 V and the LED's forward voltage anywhere in its row's range. Two resistors in the chain share the drop: the relation divides by the sum of their resistances. A relation the design writes itself about the same current is allowed, and both must hold.

A part that is not a `dc resistor` or a `dc diode` the way the chain runs through it (a part with no `dc` statement, a MOSFET, a diode turned round) leaves the current free, because the chain cannot say what it drops. A part of the chain whose value nothing else limits is then an unconstrained pick, and the finding names the part that stopped the chain:

```nsm evaluate warns
import std::parts::passive::Resistor
import std::parts::diode::Zener
import std::sources::Rail

board Reference {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .r1: Resistor
    /// A shunt reference: the resistor sets the current the Zener breaks down at.
    .zener: Zener { .current &= 1mA..5mA }
    .supply.pos ~> .r1 ~> .zener ~> .supply.neg
}
```

```text
Reference: unconstrained pick guideline: `r1` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104; the `~>` chain it is in sets no current, because `zener` (Zener) in it is not a `dc resistor` or a `dc diode` the way the chain runs: state it, `.r1.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.r1.resistance &= 10kohm..100kohm`
```

A Zener in breakdown is not a DC element yet (a decision of its own, ADR 0076), so here the board writes the relation itself, `.zener.current == (.supply.voltage - .zener.zener_voltage) / .r1.resistance`. A part of a project's own that has a resistance or a forward drop at DC states it with a `dc` statement, and the chain reads it. After the pick, the DC analysis solves the whole network of the picked parts, this chain with the rest (see "DC analysis").

A chain may start on a logic pin that drives it high: the `.line` of a part's `Logic` (an output, a tri-state or a bidirectional pin), with nothing else on its net but the chain. The pin is at least its `voh` and at most its reference rail's voltage, so the relation's source is the range `.voh .. .reference.voltage`. A chain may also end on a pin that sinks it low (an open-drain pin too), which is at most its `vol`: `0V .. .vol`. The level must be stated at the current its datasheet measures it at (`.gpio.voh &= .vdd.voltage - 0.4V at 8mA`, see "Logic levels and absolute maximums"): the level holds only up to that load, so the chain's current is at most it, and a window above it is refused at the level's line. A GPIO that drives an LED picks its resistor from the LED's window:

```nsm evaluate
import std::parts::passive::{Resistor, Led}
import std::logic::Logic
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Mcu {
    .designator &= "U"
    .vdd: Rail
    .gpio: Logic
    .gpio.reference ~ .vdd
    .gpio.voh &= .vdd.voltage - 0.4V at 8mA
    .gpio.vol &= 0.4V at 8mA
    pins {
        .vdd.pos    power  "VDD" "1"
        .vdd.neg    power  "GND" "2"
        .gpio.line  output "IO"  "3"
    }
}

board Status {
    .process: TwoLayer
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .mcu: Mcu
    .supply ~ .mcu.vdd
    .led: Led { .current &= 2mA..8mA }
    .r1: Resistor {}
    .mcu.gpio.line ~> .r1 ~> .led ~> .supply.neg
}
```

The relation the checker writes is

```nsm-fragment
.led.current == ((.mcu.gpio.voh .. .mcu.gpio.reference.voltage) - .led.forward_voltage) / .r1.resistance
```

and the pin's limit is `.led.current <= 8mA`. The window is what the LED carries while the pin drives it: when the pin is low the LED is dark by design, and the DC analysis judges the window with the pin high. A level stated with no test current leaves the chain's current free, and the unconstrained-pick finding names the level.

A chain from one rail's `pos` to another rail's `pos` on the same ground has the difference of the two voltages across it: `.v12.pos ~> .r1 ~> .led ~> .ldo.vout.pos` gives `.led.current == (.v12.voltage - .ldo.vout.voltage - .led.forward_voltage) / .r1.resistance`. The rail at the end must hold its voltage: a regulator's output, or the rail of a part's `dc source` (a connector's supply). A rail whose voltage is what the chain leaves it is a load, and the chain says nothing of its current: a module's output behind a fuse, and a board's own rail at the end of a chain, which as often stands for a load with the window of voltage it takes (`.load.voltage &= 5V..16V`). As across one rail, a corner where the current would be negative is refused by an LED's `.current >= 0mA`.

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
error: these members have no member of the same name on the other side: `.supply.pos`, `.supply.neg`, `.signal.line`, `.signal.reference`; give each a partner with a rename, `{ a = b }`, or leave it out with `except { a }`
  --> main.nsm:7:5
  |
7 |     .supply ~ .signal
  |     ^^^^^^^^^^^^^^^^^
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
