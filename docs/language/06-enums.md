## Enums: a value from a fixed list

An `enum` names a type whose values are a fixed list of variants (ADR 0054). It is declared at the top of a file, like a definition, and a variant is written `Enum::Variant` where the enum is in scope, or bare where the variant itself is imported (ADR 0118):

```nsm
import std::parts::connector::Header1x2
import std::sources::Rail

/// How a lamp is driven.
enum Drive {
    /// Always on.
    Steady,
    Dimmed,
}

module Lamp {
    .supply: Rail
    .drive: Drive
    .j: Header1x2
    .supply.pos ~ .j.p1
    .supply.neg ~ .j.p2
}

board Lamps {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .lamp: Lamp {
        .drive &= Drive::Dimmed
    }
    .input ~ .lamp.supply
}
```

- A member has an enum as its type: `.drive: Drive`. An enum that another file declares is imported like a definition (`import src::lamp::Drive`).
- A value is stated with `&=` or given as a default with `?=` (ADR 0051), and two statements of one field must name the same variant, or the usual empty-intersection error names both: ``contradiction on `lamp.drive`: it is already Drive::Dimmed, which does not overlap Drive::Steady``.
- `==` compares two values of one enum, a field and a variant or a field and a field, and is checked like any other relation: `.drive == Drive::Steady` fails when the field is `Drive::Dimmed`. Only `==` compares variants; they have no order.
- `&=`, `?=` and `==` take only a variant of the field's own enum. Text is not one: `.drive &= "Dimmed"` is refused and the message says what to write.
- A variant follows the import system like any other name (ADR 0118). It is written with its enum's name (`Drive::Dimmed`) where the enum is in scope: declared in the file, imported, or in the prelude. It is written bare (`Dimmed`) only where the file imports the variant itself, `import src::lamp::Drive::Dimmed`, or several at once, `import src::lamp::Drive::{Steady, Dimmed}`. The rule is the same for every enum. A bare name that is not imported is a member of an enclosing block, as everywhere else; where there is none, the error says how the variant is written.
- An LED's colour is the enum `Color` of `std::parts::passive` (`Red`, `Green`, `Blue`, `White`, `Yellow`): `import std::parts::passive::{Led, Color}` and `.led: Led { .color &= Color::Red }`. A colour is added to the enum with the catalog rows that have it.
- `AssemblyMode` (variants `Fab`, `Hand`, `None` and `Consigned`) is the one enum that every file knows without an import, because every part has a built-in `.assembly` of that type (Who assembles a part, above). It is defined in `std/prelude.nsm`.

A file that writes the same variants often imports them and writes them bare:

```nsm
import std::parts::passive::Led
import std::parts::passive::Color::{Red, Green}

board Signals {
    .fault: Led { .color &= Red }
    .ready: Led { .color &= Green }
}
```

A variant the enum does not have is refused where it is written, with the variants listed and the nearest one named:

```nsm fails
import std::parts::passive::Resistor

board Solder {
    .r1: Resistor {
        .assembly &= AssemblyMode::Hnad
    }
}
```

```text
error: `AssemblyMode` has no variant `Hnad`; its variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`; did you mean `AssemblyMode::Hand`?
  --> main.nsm:5:36
  |
5 |         .assembly &= AssemblyMode::Hnad
  |                                    ^^^^
```

A bare variant that the file does not import is refused, with the two ways to write it:

```nsm fails
import std::parts::passive::Led

board Lamp {
    .led: Led { .color &= Gren }
}
```

```text
error: `Gren` is not defined here; a variant of `Color` is written `Color::Green`, or import the variant: `import std::parts::passive::Color::Green`
  --> main.nsm:4:27
  |
4 |     .led: Led { .color &= Gren }
  |                           ^^^^
```

Text where an enum belongs is refused, and says which variant it was meant to be (this is what the quoted words of earlier versions, `.assembly &= "hand"`, now report):

```nsm fails
import std::parts::passive::Resistor

board Quoted {
    .r1: Resistor {
        .assembly &= "hand"
    }
}
```

```text
error: `r1.assembly` holds a variant of `AssemblyMode`, but this value is text; a variant of `AssemblyMode` is not written as text: write `AssemblyMode::Hand` (the variants are `AssemblyMode::Fab`, `AssemblyMode::Hand`, `AssemblyMode::None` and `AssemblyMode::Consigned`)
  --> main.nsm:5:9
  |
5 |         .assembly &= "hand"
  |         ^^^^^^^^^^^^^^^^^^^
```
