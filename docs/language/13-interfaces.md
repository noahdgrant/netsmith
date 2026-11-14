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
