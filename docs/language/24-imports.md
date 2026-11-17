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

### The linter: an unused import

`netsmith project lint` (_The linter_, in the chapter on commands) finds a name that an `import` brings in and the file never writes, `lint.unused_import`. The name is written when it appears anywhere in the file but its imports: as a type, an enum (`AssemblyMode::Hand`), the kind of a `defaults` line or a generic a part provides. Delete the name from the import, or the import when it names nothing else.

```nsm lints
import std::parts::passive::{Resistor, Capacitor}
import std::sources::Rail

board Pull {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .r: Resistor { .resistance &= 10kohm +/- 5% }
    .supply.pos ~> .r ~> .supply.neg
}
```

```text
main.nsm:1:40: lint.unused_import: `Capacitor` is imported from `std::parts::passive` and never used in this file; delete it from the import
```

An import that brings the providers of its file into scope and is not used by name (`import src::zbk9::Zbk9   // not used by name: brings the IC in scope`, _Generics and providers_) is used when one of the file's providers was a candidate of a use on a board of the project, and no other import of the file already names that file by a name the file writes.
