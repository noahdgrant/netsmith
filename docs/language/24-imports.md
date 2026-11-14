## Imports

`import` brings definitions into a file. Paths start at `std` for the standard library or `src` for the files of your project.

```nsm-fragment
import std::parts::passive::Resistor
import std::parts::passive::{Resistor, Led}
import src::power::Bypassed
```

A project file `src/power.nsm` is the module `src::power`:

```nsm module=src::power
//! Power stages.

import std::parts::passive::Capacitor
import std::sources::Rail

/// A rail with a bypass capacitor across it.
module Bypassed {
    .rail: Rail
    .cap: Capacitor { .capacitance &= 100nF +/- 20% }
    .rail.pos ~> .cap ~> .rail.neg
}
```

```nsm solve
import src::power::Bypassed
import std::sources::Rail

board Filtered {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .filter: Bypassed
    .supply ~ .filter.rail
}
```

Rules.

- An import names a module path and a definition. The last segment is the definition. There is no import of a whole module.
- A name is imported once. Importing a name that the file defines or already imports is an error.
- A type is written as a bare name. `A::B` in a type position is not supported.

Misuse.

```nsm fails
import std::parts::passive::Resistors

board Empty {
}
```

```text
error: module `std::parts::passive` has no definition `Resistors`
  --> main.nsm:1:29
  |
1 | import std::parts::passive::Resistors
  |                             ^^^^^^^^^
```

```nsm fails
import src::powr::Bypassed

board Empty {
}
```

```text
error: unknown module `src::powr`
  --> main.nsm:1:1
  |
1 | import src::powr::Bypassed
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^
```
