## Attributes

An attribute says what a definition or a member is to the checker, the solver and the tools, in a way a project's own definition says it too (ADR 0125). It is written `#[name]` or `#[name(argument, ...)]` on its own line, after the `///` lines and before a `generic` or a `part`, or before a member of one. The standard library states its own with the same attributes, so a project's generic can do whatever a standard one does.

| Attribute | On | What it says |
|---|---|---|
| `#[bank]`, `#[bank(Capacitor)]` | a generic or a part | A part of it stands for a number of identical parts in parallel, its `#[role(count)]` member (see "A bank of capacitors in parallel"). The argument names the generic or part each of them is in the design (`CapacitorBank`'s parts are `Capacitor`s); without it, each is of the bank's own kind. |
| `#[series(E12, E24, E96)]` | a generic or a part with a `catalog` | The E-series the solver may make values from when no row serves (ADR 0047). The project's `series` picks one: itself when offered, else the finest offered series below it, so a capacitor that offers E12 and E24 gets E24 values in an E96 project. |
| `#[kind(resistor)]` | a generic or a part | What the tools treat it as, its declared kind: `resistor`, `capacitor`, `inductor`, `ferrite_bead`, `fuse`, `led`, `diode`, `transistor`, `crystal`, `ic`, `connector`, `mounting_hole`, `plated_hole`, `fiducial` or `label`. The placer decouples with a `capacitor`, the schematic draws an `inductor`, a `ferrite_bead` and a `fuse` in series, the placer keeps a `connector` at the edge and a `mounting_hole` in a corner, and `unused` follows a `resistor`, a `fuse` and a `ferrite_bead` (ADR 0083). |
| `#[value("LED")]` | a generic or a part | What a schematic shows as its value when the solver chose no row. Without it, the name of the kind. |
| `#[assembly(hand)]` | a generic or a part | Who assembles it when the design, its catalog row and the project say nothing (ADR 0024): `fab`, `hand`, `none` or `consigned`. The standard headers and sockets are `hand`. |
| `#[role(bias)]` | a member of a generic or a part | Where the member's value comes from, whatever its name: `bias`, the voltage of the rail the part is across (ADR 0013); `bridge_voltage`, the same with its sign (ADR 0088); `current`, the current through its `bridge` (ADR 0014); `supply_peak` (ADR 0086), `supply_reverse_peak`, `limit_current` and `supply_fault_current` (ADR 0091), from the chain it is in; `count`, the number of a `#[bank]`. |

A part has the attributes of its own definition and of the generics it provides, its own first. A member with a role keeps the checks the definition states on it, and a member named `bias` or `current` with no role is a plain member: the role, not the name, gives it a value.

A project's thermistor that the solver may give any E24 value when no row of its catalog serves, with a chain current read through a member of its own name:

```nsm
import std::sources::Rail

/// An NTC thermistor, invented for this example.
#[kind(resistor)]
#[series(E12, E24)]
generic Thermistor {
    .designator &= "RT"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    /// The current through it, from `a` to `b`.
    #[role(current)]
    .through: Current
    .through <= 1mA
    bridge .a ~> .b
    catalog thermistor { .resistance }
}

board Probe {
    .supply: Rail { .voltage &= 3.3V }
    .t: Thermistor { .resistance &= 9kohm..11kohm }
    .supply.pos ~> .t ~> .supply.neg
}
```

The checker knows the attributes and refuses any other, and a role or a series it does not know, with the nearest name:

```nsm fails
#[bank]
generic Cell {
    #[role(cuont)]
    .count: Number
}
```

```text
error: there is no role `cuont` (did you mean `count`?); the roles are `bias`, `bridge_voltage`, `current`, `supply_peak`, `supply_reverse_peak`, `limit_current`, `supply_fault_current`, `count`
  --> main.nsm:3:12
  |
3 |     #[role(cuont)]
  |            ^^^^^
```

A whole-number count of identical parts that the board chooses itself is a group (`3 x Capacitor`, "A group of identical members"); a bank is for a count the solver chooses.
