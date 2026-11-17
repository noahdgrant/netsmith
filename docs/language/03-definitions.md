## Definitions: board, part, interface, module, system, panel

A definition is a named, reusable description. There are nine kinds.

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
| `enclosure` | What a case fixes of the boards in it (the outline, openings, holes, keepouts and the case's STEP files), shared by the boards whose physical files say `case Name` (see The shape of a board, _A shared enclosure_). |

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

### The linter: an unused definition or member

`netsmith project lint` (_The linter_, in the chapter on commands) finds what the project declares and never uses, `lint.unused_definition`:

- a definition (a module, an interface, a part, an enum, a generic, an enclosure) that no board, system or panel of the project reaches, through the names the definitions write, the physical files and the definition the manifest names as the `environment`. A provider is reached with the generic it provides. It is judged only when the whole project is linted (`project lint` with no file, or a file outside a project), and only in a project that has a board: a project of modules alone is a library of them;
- a member that a module or a board declares and that no instance of it joins: a part, a module, an interface or a pin none of whose pins is on a net with another pin. A member with no pin (a mounting hole, an `Environment`) is not judged so, nor is one of a physical file;
- a value member of a module or a board (`.gain: Voltage`) that no statement of any instance narrows, defaults or reads.

Join the member, read or state the value, or delete it; use the definition, or delete it.

```nsm lints
import std::parts::passive::Resistor
import std::sources::Rail

/// Holds a line low until something drives it.
module Pulldown {
    .input: Rail
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .spare: Resistor { .resistance &= 1kohm +/- 1% }
    .gain: Voltage
    .input.pos ~> .r ~> .input.neg
}

/// A pull-up, which no board uses.
module Pullup {
    .input: Rail
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .input.pos ~> .r ~> .input.neg
}

board Probe {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3" }
    .hold: Pulldown
    .supply ~ .hold.input
}
```

```text
main.nsm:8:5: lint.unused_definition: `.spare` is declared and never joined: no pin of it is on a net; join it, or delete it
main.nsm:9:5: lint.unused_definition: `.gain` is declared and never used: no statement states or reads it; state it, read it, or delete it
main.nsm:14:8: lint.unused_definition: the module `Pullup` is never used: no board of the project reaches it; use it, or delete it
```

### The linter: names

The linter finds a name that breaks the convention above, `lint.naming`: a definition, a variant of an enum and a type parameter are `CamelCase` (a capital first, and no `_` but between two digits, which keeps the numbers of a part number apart: `HroK3_1270`); a member, a pin of a pin table, a layout guideline and an anchor are `snake_case` (no capital). A quoted name (`'5'`) is as the datasheet prints it and is not judged, and neither is what a datasheet names: a peripheral, a function of a pin, a shared resource, a net. The finding gives the name to use.

```nsm lints
import std::parts::passive::Resistor
import std::sources::Rail

board probe_board {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3" }
    .R1: Resistor { .resistance &= 10kohm +/- 5% }
    .supply.pos ~> .R1 ~> .supply.neg
}
```

```text
main.nsm:4:7: lint.naming: `probe_board` is a board, whose name is `CamelCase` in the guide; call it `ProbeBoard`
main.nsm:6:6: lint.naming: `.R1` is a member, whose name is `snake_case` in the guide; call it `.r1`
```
