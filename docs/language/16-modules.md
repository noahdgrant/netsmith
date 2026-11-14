## Modules

A module is a reusable sub-circuit. Its interfaces, pins and declared values are its interface to the outside. The parts and modules inside it may only be narrowed from outside (ADR 0116).

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with the resistor that sets its current.
module Indicator {
    .supply: Rail
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Lamp {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .lamp: Indicator
    .input ~ .lamp.supply
    .header: Header1x2
    .input.pos ~ .header.p1
    .input.neg ~ .header.p2
}
```

Rules.

- A board joins and sets a module through the module's rails, pins and declared values: `.lamp.supply.voltage`.
- A board, or a module that holds the module, may also narrow any value inside it by its path, with `&=` only: `.lamp.led.current &= 8mA..12mA`, `.lamp.r1.package &= "0603"`, at any depth (`.stage.divider.rtop.resistance &= 100kohm +/- 1%`). A narrowing only takes values away, so whatever the module states still holds, and a narrowing that misses it is the usual contradiction at the line that writes it. Nothing else reaches inside: a join (`~`, `~>`), a relation (`==`, `<=`, `>=`), a default (`?=`) and the right side of a `&=` are refused at the member that enters the module. The one exception is a join to the IC that the user named for a module's type parameter (`.reg.ic.pg` of `Buck<Lt8641>`, ADR 0120; see _A module that takes a part_). A narrowing counts as a statement of the field, so it replaces a default the module wrote on it with `?=`.
- A module author exposes as a rail or a declared value what a user joins, reads or relates, and a value that ties several parts together (one package for both resistors of a divider). A member that would only pass one value to one part inside is not written: the user narrows the part.
- A declared value with a sensible value is a default (`?=`). One without is left for the board to state: a board that forgets it gets a finding when a pick or a check waits for it (see "A value that nobody stated"), and a bound the module writes on it (`.balance_resistance >= 1kohm`) reaches the parts it is held to.
- Two instances of a module are separate. Their internal nets get names from their instance paths, so they never collide.
- A module may hold modules and parts. It names no nets.

A board that wants the module's LED brighter and its resistor larger narrows them inside the module, without a member of the module for either:

```nsm solve
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

/// An LED with the resistor that sets its current.
module Indicator {
    .supply: Rail
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Brighter {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Indicator {
        .led.current &= 8mA..15mA
        .r1.package &= "0603"
    }
    .input ~ .lamp.supply
}
```

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
    .stage.r1.resistance == 1kohm +/- 1%
}
```

```text
error: `r1` is inside `Stage` and is not part of its interface: only narrowing (`&=`) reaches inside a module; narrow a value inside it with `&=`, or use a rail, a pin, or a value that `Stage` declares
   --> main.nsm:12:12
   |
12 |     .stage.r1.resistance == 1kohm +/- 1%
   |            ^^
```

A join reaches inside no more than a relation does:

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
    .probe: Resistor
    .stage.r1.a ~ .probe.a
}
```

```text
error: `r1` is inside `Stage` and is not part of its interface: only narrowing (`&=`) reaches inside a module; narrow a value inside it with `&=`, or use a rail, a pin, or a value that `Stage` declares
   --> main.nsm:13:12
   |
13 |     .stage.r1.a ~ .probe.a
   |            ^^
```

### A module in the KiCad schematic

The schematic that `build` writes (issue #563) draws each module instance of the board as a KiCad hierarchical sheet, and the board's own parts, the sheet symbols and the power flags of the nets that reach them on the root sheet, `<board>.kicad_sch`. A board with no module is one sheet, as it always was.

- One `.kicad_sch` is written for each module definition, beside the root sheet, and placed by every instance that is the same circuit: the same parts, values and pins, joined the same way inside, with the same nets through the same members. `.left: Indicator` and `.right: Indicator` share `Indicator.kicad_sch`, each part with its own reference designator in each instance. An instance that differs (another value of a resistor) has a file of its own, `Indicator_2.kicad_sch`.
- A module inside a module is drawn flat in the sheet of the board's module it is in.
- A net that leaves a sheet is a sheet pin, and a hierarchical label inside, named for the first member of the module it is on (`supply.pos`); a net on no member keeps the board's name. A net inside one sheet keeps its name, without the instance's path (`net-lamp.r1.p2` is `net-r1.p2` in the sheet). A name that would meet another, two nets of one name in a sheet or two names in the schematic that differ only in case (KiCad's electrical rules check reports `gnd` beside `GND`), is the board's name instead.
- A net that takes power and has nothing to drive it gets its power flag on the one sheet it is on, or on the root when it leaves a sheet.
- Each footprint of the board names its symbol and the sheet it is on (`(path ...)`, `(sheetname ...)`, `(sheetfile ...)`, as KiCad writes them), beside its `netsmith_path`, so KiCad's update of the board from the schematic matches every part.
