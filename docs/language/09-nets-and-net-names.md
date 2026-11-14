## Nets and net names

A net is all the pins joined at one node. Only a board names a net (ADR 0005). Naming is a narrowing of the `net` field of a pin: `.supply.pos.net &= "VCC"`. Every pin that asks for `VCC` is on the net `VCC`. A net nobody names is called `net-` and its smallest terminal path, for example `net-led.anode`.

```nsm
import std::parts::passive::Resistor
import std::sources::Rail

board Named {
    .supply: Rail {
        .pos.net &= "VCC"
    }
    .r1: Resistor
    .supply.pos ~> .r1 ~> .supply.neg
}
```

Rules.

- A module, part or interface that wrote `.net &=` would give every instance of itself the same net name, so the checker refuses it outside a board. A board names a module's nets through the module's rails and pins: `.stage.vout.pos.net &= "3V3"`. The one name a library offers is a default (`?=`) in an interface, such as the ground of a `Rail` (see Ground below).
- The rule covers any field called `net`, such as the net of a plane in a manufacturing process.
- Pins that are joined and name their net differently are an error.
- Two boards of a system may both have `GND`. They are different nets.

Misuse.

```nsm fails
import std::sources::Rail

module Stage {
    .supply: Rail
    .supply.pos.net &= "VCC"
}

board Uses {
    .stage: Stage
}
```

```text
error: `supply.pos.net` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name
  --> main.nsm:5:5
  |
5 |     .supply.pos.net &= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
error: `stage.supply.pos.net` names a net, and only a board names nets: set it where the board uses this, so that every instance does not share one name
  --> main.nsm:5:5
  |
5 |     .supply.pos.net &= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::sources::Rail
import std::parts::passive::Resistor

board Disagree {
    .supply: Rail { .pos.net &= "A" }
    .r: Resistor
    .supply.pos ~ .r.a
    .r.a.net &= "B"
}
```

```text
error: these pins are connected but name their net differently: `A` on `supply.pos`, `B` on `r.a`
  --> main.nsm:8:5
  |
8 |     .r.a.net &= "B"
  |     ^^^^^^^^^^^^^^^
```

### Ground

Ground is stated once, by not stating it. `std::sources::Rail` offers `.neg.net ?= "GND"` (a default, ADR 0051), so a rail whose ground no statement names is on `GND`, and every such rail on a board shares that one net. A rail that has its own ground, an analog one or an isolated one, states it with `&=`, which replaces the default on that rail (ADR 0121).

```nsm
import std::parts::passive::Resistor
import std::sources::Rail

board TwoGrounds {
    .digital: Rail { .pos.net &= "3V3" }
    .analog: Rail {
        .pos.net &= "3V3A"
        .neg.net &= "AGND"
    }
    .r1: Resistor
    .r2: Resistor
    .digital.pos ~> .r1 ~> .digital.neg
    .analog.pos ~> .r2 ~> .analog.neg
}
```

The board has two ground nets, `GND` (the first rail's) and `AGND`. A board with one ground says nothing about it. A module's rail takes part in the same way: the module says nothing, and the board's statement for the rail it joins it to names the ground.

```nsm
import std::parts::passive::Resistor
import std::sources::Rail

module Stage {
    .out: Rail
    .r: Resistor
    .out.pos ~> .r ~> .out.neg
}

board Analog {
    .stage: Stage
    .rail: Rail { .neg.net &= "AGND" }
    .stage.out ~ .rail
}
```

Here the module's ground is `AGND`, and no net is called `GND`.

Rules.

- The default names a net only where a part is on the ground and a second pin is joined to it. A rail whose ground nothing is joined to (`.slot: Rail` that only its `pos` is used) names no net; join its `neg` to another rail's `neg`, or to a pin, to put it on that ground.
- A name that a statement gives a pin beats the default of every pin joined to it: the joined pins are one net, and the statement names it. A module's rail joined to a board's rail that states `AGND` is on `AGND`, with no error from the module's default.
- A default is for a board. A module checked on its own, with no board above its rails, names no net, and two boards of a system each have their own `GND` (they are different nets).
- Only an interface may offer a net name (`.line.net ?= "CLK"` in an `interface`): every instance of an interface such as a rail is meant to share the name. A module, a part, a generic and a system may not.
- A board may not offer a different ground with `?=`, since the rail already has one default and a field has one (ADR 0051). State it with `&=`.
- Two rails joined with `~` that state different grounds are still an error: only a statement can name a net.

Misuse: a module offers a net name.

```nsm fails
import std::sources::Rail

module Stage {
    .out: Rail
    .out.pos.net ?= "VCC"
}

board Uses {
    .stage: Stage
}
```

```text
error: `out.pos.net` names a net by default, and only a board names nets: an interface such as a rail may offer a net name, since every instance of it is meant to share one; set it where the board uses this
  --> main.nsm:5:5
  |
5 |     .out.pos.net ?= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^
error: `stage.out.pos.net` names a net by default, and only a board names nets: an interface such as a rail may offer a net name, since every instance of it is meant to share one; set it where the board uses this
  --> main.nsm:5:5
  |
5 |     .out.pos.net ?= "VCC"
  |     ^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a board offers a ground where the rail already has a default.

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

board Offers {
    .supply: Rail
    .r: Resistor
    .supply.pos ~> .r ~> .supply.neg
    .supply.neg.net ?= "RET"
}
```

```text
error: `supply.neg.net` has 2 defaults, at main.nsm:8 and std/sources.nsm:67: a field has one default; keep one `?=`
   --> std/sources.nsm:67:5
   |
67 |     .neg.net ?= "GND"
   |     ^^^^^^^^^^^^^^^^^
```

Misuse: joined rails that state different grounds.

```nsm fails
import std::sources::Rail

board Split {
    .a: Rail { .neg.net &= "GND" }
    .b: Rail { .neg.net &= "AGND" }
    .a.neg ~ .b.neg
}
```

```text
error: these pins are connected but name their net differently: `GND` on `a.neg`, `AGND` on `b.neg`
  --> main.nsm:5:16
  |
5 |     .b: Rail { .neg.net &= "AGND" }
  |                ^^^^^^^^^^^^^^^^^^
```
