## Members

A member is a named thing inside a definition: a pin, a value, a part, a module or an interface. You declare one with `.name: Type`. A declaration may have a block that sets members of the new thing.

```nsm
import std::parts::passive::Capacitor
import std::sources::Rail

board Members {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .bypass: Capacitor {
        .effective_capacitance >= 1uF
        .max_voltage >= supply.voltage * 2
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

Rules.

- `.name` is a member of the block you are writing in. A path continues with dots: `.supply.pos.net`.
- A name without the leading dot is looked up outward. `supply.voltage` inside the `.bypass` block finds the board's `supply`. The search goes through the enclosing blocks of the same definition, innermost first.
- At the top level of a definition a bare name finds nothing, because nothing encloses it. Write the dot.
- A bare name never leaves its definition. A module cannot see the board that uses it.
- A declaration needs a single name: `.name: Type`. A name is declared once per block.
- Statements end at a newline or at `;`.
- A name that is not a plain identifier is written in single quotes: `.'1'`.

Types you can declare: `Pin`; the quantities `Number`, `Voltage`, `Current`, `Resistance`, `Capacitance`, `Inductance`, `Frequency`, `Time`, `Length`, `Power`, `Charge`, `Temperature` (kelvin, `10K`, or Celsius, `25degC`: see Temperatures below; a rise is a difference, so it is kelvin too); the text types `String` and `Package` (a package's name, checked against the part's pin tables and its catalog: Pin tables, below); an `enum` that is in scope (Enums, below), such as `Color` of `std::parts::passive`, the colour of an LED; and any definition you can name.

Misuse: a typo, a bare name where the dot belongs, and a name declared twice. A member name that is close to one the block has (the same letters in another case, or one or two edits away) gets the nearest as `did you mean`.

```nsm fails
import std::parts::passive::Resistor

board Typo {
    .r1: Resistor { .resitance &= 1kohm +/- 1% }
}
```

```text
error: `resitance` is not a member of this Resistor; did you mean `.resistance`?
  --> main.nsm:4:22
  |
4 |     .r1: Resistor { .resitance &= 1kohm +/- 1% }
  |                      ^^^^^^^^^
```

```nsm fails
import std::parts::passive::{Color, Led}

board Spelling {
    .led: Led { .colour &= Color::Red }
}
```

```text
error: `colour` is not a member of this Led; did you mean `.color`?
  --> main.nsm:4:18
  |
4 |     .led: Led { .colour &= Color::Red }
  |                  ^^^^^^
```

```nsm fails
import std::parts::passive::Resistor

board Bare {
    .r1: Resistor
    r1.resistance &= 1kohm +/- 1%
}
```

```text
error: `r1` is not defined in any enclosing block; write `.r1` for a member of this block
  --> main.nsm:5:5
  |
5 |     r1.resistance &= 1kohm +/- 1%
  |     ^^
```

```nsm fails
import std::parts::passive::Resistor

board Twice {
    .r: Resistor
    .r: Resistor
}
```

```text
error: `r` is already declared in this block
  --> main.nsm:5:5
  |
5 |     .r: Resistor
  |     ^^
```

### A group of identical members

Several members that are the same, such as the decoupling capacitors of a rail, are one statement (ADR 0119): a count and `x` before the type, `.decoupling: 4 x Capacitor { ... }`. The group is that many members, `.decoupling[0]` to `.decoupling[3]`, each a member of its own as if declared one by one, with the group's block. A chain with the group alone between its two ends, `.supply.pos ~> .decoupling ~> .supply.neg`, puts every member in parallel between them. A group may also be written in the chain itself, where it has no name to give: `.supply.pos ~> 2 x Capacitor { ... } ~> .supply.neg`.

```nsm solve report
import std::parts::passive::Capacitor
import std::sources::Rail

board Groups {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    /// The decoupling of the supply, one by each supply pin.
    .decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }
    .decoupling[3].package &= "0603"
    .supply.pos ~> .decoupling ~> .supply.neg
    .supply.pos ~> 2 x Capacitor { .capacitance &= 10uF +/- 20% } ~> .supply.neg
}
```

```text
board Groups
  C1   Capacitor       10u             Capacitor_SMD:C_0603_1608Metric                            C19702 Basic
  C2   Capacitor       10u             Capacitor_SMD:C_0603_1608Metric                            C19702 Basic
  C3   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  C4   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  C5   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  C6   Capacitor       100n            Capacitor_SMD:C_0603_1608Metric                            C14663 Basic
  net 3V3: C1.1, C2.1, C3.1, C4.1, C5.1, C6.1
  net GND: C1.2, C2.2, C3.2, C4.2, C5.2, C6.2
```

Rules.

- The count is a whole number of at least two, written with a space each side of the `x` (`4 x`). A group is declared by one name of its own, not by a path into a member.
- The block holds for every member. One member is narrowed further by its index, `.decoupling[3].package &= "0603"`, as any member is; the index counts from 0. A path to the whole group outside its block (`.decoupling.package`) is an error that names the members.
- A chain passes through a group only with the group alone between its two ends, every operator `~>`. To join one member to something else, name it: `.decoupling[0].a ~ .u.vdd`.
- A group written in a chain is named from its kind and the chain's two ends: `2 x Capacitor` between `.supply.pos` and `.supply.neg` is `capacitor_supply_pos_supply_neg[0]` and `[1]`. Two such groups of one kind between the same ends would have one name, which is an error: name each group yourself.
- Each member is a part of the design with a designator of its own. The members' paths differ only in the index, which sorts as a number, so they are numbered one after the other in index order (Designators, below); the group's doc comment is one note, on the first.
- A physical file and a layout file name a member by its path with the index: `.decoupling[2].place.side &= "bottom"`, `[Groups.parts."decoupling[2]"]`. A pick lock has an entry for each member.

A group is not a bank (A bank of capacitors in parallel, below). A bank is one member whose count the solver may choose, and its capacitors are identical and have no paths a board can narrow. A group's count is the board's, and its members are members.

```nsm fails
import std::parts::passive::Capacitor

board Whole {
    .decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }
    .decoupling.package &= "0603"
}
```

```text
error: `.decoupling` is a group of 4, not one member: name one, `.decoupling[0]` to `.decoupling[3]`; a statement in the group's block holds for every member
  --> main.nsm:5:6
  |
5 |     .decoupling.package &= "0603"
  |      ^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Capacitor

board Past {
    .decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }
    .decoupling[4].package &= "0603"
}
```

```text
error: `.decoupling` is a group of 4, `.decoupling[0]` to `.decoupling[3]`, and has no member `[4]`
  --> main.nsm:5:6
  |
5 |     .decoupling[4].package &= "0603"
  |      ^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board Shape {
    .supply: Rail {
        .voltage &= 3.3V
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .r: Resistor
    .decoupling: 2 x Capacitor { .capacitance &= 100nF +/- 10% }
    .supply.pos ~> .r ~> .decoupling ~> .supply.neg
}
```

```text
error: `.decoupling` is a group of 2, and a group in a chain stands alone between the chain's two ends, each member in parallel (`.v.pos ~> .decoupling ~> .v.neg`); join one member by its index, `.decoupling[0]` to `.decoupling[1]`
   --> main.nsm:12:26
   |
12 |     .supply.pos ~> .r ~> .decoupling ~> .supply.neg
   |                          ^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Capacitor

board One {
    .c: 1 x Capacitor
}
```

```text
error: a group has at least two members; `1 x` is not one, so declare the member alone (`.c: Capacitor`)
  --> main.nsm:4:9
  |
4 |     .c: 1 x Capacitor
  |         ^^^
```
