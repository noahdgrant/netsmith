## Capacitors: marked value, effective capacitance and bias

A `Capacitor` has two capacitances because they are two quantities (ADR 0013).

- `.capacitance` is the marked value with its tolerance, as on a bill of materials. Use it for a bypass capacitor where you mean a part.
- `.effective_capacitance` is what the circuit can use: the marked value after the DC bias across the part and after the tolerance, in the worst case over the range the bias can take. Use it where a datasheet asks for capacitance in use, such as a regulator's output capacitor.
- `.bias` is the DC voltage across the capacitor. A capacitor placed between a rail's `pos` and `neg` takes its bias from that rail. State `.bias` only for a capacitor that is across no rail.

```nsm solve report
import std::parts::passive::Capacitor
import std::sources::Rail

board Caps {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .bypass: Capacitor { .capacitance &= 100nF +/- 20% }
    .bulk: Capacitor { .effective_capacitance >= 1uF }
    .supply.pos ~> .bypass ~> .supply.neg
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
board Caps
  C1   Capacitor       4.7u            Capacitor_SMD:C_0402_1005Metric                            C23733 Basic
  C2   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  net 3V3: C1.1, C2.1
  net GND: C1.2, C2.2
```

The solver picks the cheapest Basic part whose effective capacitance stays at or above 1 uF. A marked 1 uF part does not qualify: the 25 V one keeps about 0.7 of its value at 3.465 V, before the tolerance.

Rules.

- The solver evaluates the effective capacitance from the part's published curve (the `effective_capacitance` cell of `data/jlcpcb/catalog/capacitor.csv`). A part with no curve is refused, but only when the design constrains `.effective_capacitance`.
- Temperature and ageing are not modelled. The curves are typical data at 25 C.
- State what you need after derating (`.effective_capacitance >= 4.7uF`), not the marked value.

Misuse: a capacitor in a chain that is across no rail, and a requirement no part meets.

```nsm fails
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board NoBias {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .filter: Capacitor { .effective_capacitance >= 1uF }
    .supply.pos ~> .r1 ~> .filter ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `filter`, which needs nothing in particular (`filter.bias` is not known, so `filter.effective_capacitance` cannot be worked out: place `filter` between a rail's pos and neg, whose voltage is known, or state `filter.bias`)
   --> main.nsm:10:5
   |
10 |     .filter: Capacitor { .effective_capacitance >= 1uF }
   |     ^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Capacitor
import std::sources::Rail

board TooBig {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .bypass: Capacitor { .effective_capacitance >= 10mF }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
error: no `capacitor` candidate satisfies `bypass`: the closest of the 34 that fit, 47uF 20% 6.3V X5R 0805 C16780, fails because bypass.effective_capacitance must always be at least 10mF, but 16.09uF..26.7uF is not always above 10mF (`bypass.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
  --> main.nsm:9:26
  |
9 |     .bypass: Capacitor { .effective_capacitance >= 10mF }
  |                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### Dielectric and package size

A ceramic capacitor's class (`C0G`, `X5R`, `X7R`) and its size (`"0402"`, `"0603"`) are two more things a design may ask for (ADR 0055, proposed).

- `.dielectric` has the enum type `Dielectric` of `std::parts::passive` (import it with the part). A design states it with `&=`, `.dielectric &= Dielectric::X7R`, or compares it with `==`. Every row of the standard capacitor catalog has one, read from the class that JLCPCB's record prints for the row; a generic capacitor (the project states an E-series) has none, so a design that states a class is given a row, never a generic part. A design that does not mention it picks as before.
- `.package` is the size as text, `.package &= "0603"`. It narrows to the rows of that package, and a generic capacitor of that size. It names one size: a design that would take 0402 or 0603 leaves `.package` open and lets the catalog's order decide (Basic parts first, then the file's order), or states `.package` on the capacitors where the size matters.
- A column is optional in a project's catalog file: a file that has no `dielectric` in its first line is read as empty there, and a design that states a class is offered no row of it.

A package that no row of the part's catalog is in is refused at the statement that names it, with the catalog's packages listed and the nearest named (ADR 0118); no row is tried for the part:

```nsm fails
import std::parts::passive::Led

board Lamp {
    .led: Led { .package &= "603" }
}
```

```text
error: `led.package` is "603", and no row of the `led` catalog is in that package; its packages are "0402", "0603"; did you mean "0603"?
  --> main.nsm:4:17
  |
4 |     .led: Led { .package &= "603" }
  |                 ^^^^^^^^^^^^^^^^^
```

```nsm solve report
import std::parts::passive::{Capacitor, Dielectric}
import std::sources::Rail

board Classes {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .bypass: Capacitor {
        .capacitance &= 100nF +/- 20%
        .dielectric &= Dielectric::X7R
        .package &= "0402"
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
board Classes
  C1   Capacitor       100n            Capacitor_SMD:C_0402_1005Metric                            C1525 Basic
  net 3V3: C1.1
  net GND: C1.2
```

A class that no row of the catalog has for the value is reported as any requirement no part meets:

```nsm fails
import std::parts::passive::{Capacitor, Dielectric}
import std::sources::Rail

board NoSuchClass {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .bypass: Capacitor {
        .capacitance &= 100nF +/- 20%
        .dielectric &= Dielectric::C0G
    }
    .supply.pos ~> .bypass ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `bypass`, which needs capacitance within 80nF..120nF and dielectric within Dielectric::C0G
  --> main.nsm:9:5
  |
9 |     .bypass: Capacitor {
  |     ^^^^^^^^^^^^^^^^^^
```

An `Inductor` has an optional `.rms_current`, the heating (RMS) current, as a catalog column. The standard rows leave it empty (see `data/jlcpcb/README.md`), and nothing in the library checks it, so `Buck` does not either: a design that needs a heating rating states `.inductor.rms_current >= 2A` itself and gets a row from a project's catalog file that records one.

### A bank of capacitors in parallel

A `Capacitor` is one part. A requirement that no single capacitor meets is met by several in parallel (ADR 0052, ADR 0123), and `CapacitorBank` is the type for that: the same capacitor, with the number of them in `.count`, a whole number the solver chooses or a module passes in. A number the board knows is `3 x Capacitor` (A group of identical members), not a bank. A `.count` on a `Capacitor` is an error that says so. Only the standard `CapacitorBank` has this meaning: a part of your own with a member named `count` (the segments of an LED bar, say) is one part, and its `.count` is a plain number. Left unset the solver chooses it from 1 to 8; a range (`.count &= 1..4`) limits the choice. It takes the fewest of the range for which every check that mentions the part holds, together with the catalog row, trying one capacitor of every row before two of any, so a part that one capacitor can serve is picked as before. `.capacitance`, `.effective_capacitance`, `.package` and the rest are the value of one capacitor, and a rating (`.max_voltage`) holds for each. What the bank is as a whole is read from its totals: `.total_effective_capacitance`, the count times what one capacitor keeps at the voltage across it, and `.total_capacitance`, the count times the marked value. The effective total is the one that means what the circuit can use, so it is what a requirement on the bank states (`.bulk.total_effective_capacitance >= 60uF`) and what `Buck` reads; a ceramic keeps a fraction of its marked value at a few volts, so the marked total is the bill of materials' figure (`4 x 22uF`) and not a capacitance to design with. A board may read and narrow either total, and the solver chooses the count and the row to meet it.

```nsm solve report
import std::parts::passive::CapacitorBank
import std::sources::Rail

board Bank {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "5V"
    }
    .bulk: CapacitorBank { .count &= 1..8 }
    .bulk.total_effective_capacitance >= 60uF
    .bulk.max_voltage >= 6V
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
board Bank
  C1   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C2   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C3   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C4   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C5   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  C6   Capacitor       47u             Capacitor_SMD:C_0805_2012Metric                            C16780 Basic
  net 5V: C1.1, C2.1, C3.1, C4.1, C5.1, C6.1
  net GND: C1.2, C2.2, C3.2, C4.2, C5.2, C6.2
```

The design has one `Capacitor` for each capacitor of the bank, numbered one after the other, all on the same nets, and the bill of materials lists the row once with the quantity. The capacitors of a bank of two or more have the paths `bulk[0]`, `bulk[1]`, ..., counted from 0 as the members of a group are (the keys of a layout file; a bank of one is `bulk`). A whole number written as the `.count` of the board's own bank (`.bulk: CapacitorBank { .count &= 3 }`) is refused with a pointer to `3 x Capacitor`, whose members the board can narrow and place one by one. A bank is for a count the solver chooses (`.count &= 1..8`) or a module passes in (`.reg.output_cap.count &= 2`, by the path into the module, ADR 0116). A bank carries what one capacitor carries (`.capacitance`, `.package`, `.max_voltage`, `.dielectric`), each as the value of one capacitor, so a narrowing on the bank is one on every member; what is stated about one member of a group (`.decoupling[1].package`) cannot be stated about one capacitor of a bank. The schematic, the KiCad files and the placer see real parts. The placer treats each as any other capacitor, so a bank is not kept in a row. A requirement in the physical file on the part's path (`.bulk.place.top &= 3mm..6mm`, `.bulk.place.side &= "bottom"`, a `.height`) holds for every capacitor of the bank: the design has one requirement for each, the check measures each, and a finding names the one it is about (`bulk[1].place.top`, with its designator). A relation between the place of the bank and that of another part (`.bulk.place.x - .r1.place.x == 5mm +/- 3mm`) holds for each pair, and the build lists a note that says so (`` `bulk` is a bank of 4 capacitors ``). A requirement on the module the bank is in reaches them all, as for any part.

When no count in the range is enough, the message names the count that came nearest:

```nsm fails
import std::parts::passive::CapacitorBank
import std::sources::Rail

board Short {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "5V"
    }
    .bulk: CapacitorBank { .count &= 1..2 }
    .bulk.total_effective_capacitance >= 10mF
    .supply.pos ~> .bulk ~> .supply.neg
}
```

```text
error: no `capacitor` candidate satisfies `bulk`: the closest of the 34 that fit, 2 x 47uF 20% 6.3V X5R 0805 C16780, fails because bulk.total_effective_capacitance must always be at least 10mF, but 21.86uF..32.81uF is not always above 10mF (`bulk.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27)
   --> main.nsm:10:5
   |
10 |     .bulk.total_effective_capacitance >= 10mF
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A bank is one choice: the capacitors of a bank are identical, and the ripple current each takes is not modelled (the catalog has no ripple rating), so it is not divided between them.
