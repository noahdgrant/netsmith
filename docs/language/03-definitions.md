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
- A part is built into a schematic only if a row of `kicad.toml` names its kind: `std/kicad.toml` has the rows of the standard library's parts, and a project adds one for each part of its own (_A part of your own, with no KiCad installed_). A part with no row checks and solves, and `build` stops with the message below.

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

Misuse: a part with no row in `kicad.toml`.

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
