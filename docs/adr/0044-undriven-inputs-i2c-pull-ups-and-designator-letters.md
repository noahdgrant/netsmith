---
status: accepted (user confirmed 2026-11-02)
---

# Undriven inputs, I2C pull-ups and designator letters

Porting a real board found rules that the checker lacked. They are generic, and each is an error or a guideline warning by ADR 0030's rule: an electrical contradiction is an error, a matter of style or advice is a warning under `guidelines` (`[project] guidelines = "warn" | "error"`, `--strict`). This ADR is written a rule at a time, in the order they were built. Three more rules were asked for and not built; they are under "Not built", and ADR 0046 built them after the user decided their syntax.

## A net of only inputs is an error

A net whose part pins are all `input` has nothing that drives it: a UART's `rx` wired to the other `rx`, or two enable pins wired together and to nothing else. ADR 0023 listed this as proposed; it is a net rule in `pin_faults`, beside "two outputs on one net", and an error because the net floats.

```text
error: net `net-a.uart.rx` has only inputs (`a.uart.rx`, `b.uart.rx`): nothing drives it, so it floats; join an output, a bidirectional pin or a rail to it, or a pull resistor
```

What counts as a driver, so that the rule has no false alarm: any pin whose kind is not `input` (`output`, `bidirectional`, `power`, `passive`, `no_connect`), a pin of a part that has no pin table (its kind is not known), a rail that the board or a module declares, and a port of the board itself (a system joins it to another board). A module's port the board leaves open is not a driver: it is only a way through. A net of one part's own pins is left to the rule that an input must be connected, which says it already.

## An I2C line with no pull-up is an error

`std::bus::I2c` is two open-drain lines: no device drives them high. A line with no resistor to a supply stays low and the bus does not work, which is an electrical fact, so it is an error and not a guideline.

ADR 0023 proposed a pin kind `open_drain` and a net rule on it. That needs a language change (a new kind in the grammar, the symbol check and the mapping). The rule is built without it: the checker looks at every interface named `I2c` (as it looks at `Rail` by its members, not by a catalog), takes the net of its `sda` and of its `scl`, and searches from the net, hop by hop through parts that have a `bridge` and a `resistance` (a resistor), for a net that has the `pos` pin of a rail. One resistor on a line is enough, a series chain counts, `I2cPullups` is two resistors to the supply, and a net wired to the rail itself is a short and not a pull-up. A pull-up built from anything but resistors (a current source IC) is not seen. The choice by name means a project's own interface called `I2c` is judged too; that is the point.

Not judged, because the pull-up may be on the other side: a bus that is a port of the board (`.bus: I2c` in a board), and a line that reaches a connector, which is a part of several pins that are all `passive` and has no `bridge`. A test point on the line does not hide a missing pull-up.

```text
error: net `net-ctl.i2c.scl` carries the scl line of an I2C bus (`ctl.i2c.scl`, `sensor.i2c.scl`) and has no pull-up: no resistor joins it to the positive pin of a rail; add `std::bus::I2cPullups` to the bus, or a resistor from a rail's `pos` to the line
```

## Designator letters follow the kind

`netsmith-lower` gave every kind but `Resistor`, `Capacitor`, `Led`, `Header1x2` and `Label` the letter `U`, so a Zener, a MOSFET, a test point and a fuse were all `U1`, `U2`, ... on the schematic, the BOM and the placement file. The `match` now has the usual letters: `L` inductor, `FB` ferrite bead, `D` for `Led`, `Diode`, `Schottky`, `Zener` and `Tvs`, `Q` for the MOSFETs, `Y` crystal, `F` fuse, `SW` switch, `J` for every header, socket, `Coax`, `UsbCPower`, `UsbC2` and `ProgrammingPads6`, `TP` test point, `H` for the two holes, `FID` fiducial. A test reads `std/` and fails for a `part` that is neither in the `match` nor in the short list of ICs, so a new kind gets its letters on purpose.

This changes the designators of a board that has any of these kinds: a hole was `U1` and is `H1`. A layout file is keyed by path (ADR 0032), so a saved placement is not lost. A fab package that was already ordered keeps its old designators in its own files.

The letters were a table in Rust when this ADR was written. ADR 0046 moved them into the parts: every part has a field `.designator`, each standard kind states its letters, and the Rust table and its test are gone. The letters above are unchanged.

## Not built, and what each needs

Three rules were asked for and were not built here, because the library had nothing for them to read. None was guessed at with invented numbers. The user decided all three on 2026-11-02, and ADR 0046 records what was built; the text below is what each needed, as it stood.

- **Logic levels** (an output's `voh` and `vol` against the `vih` and `vil` of the inputs it drives). `std::logic::Logic` had a line and a reference rail and no levels, and no part in the library stated any. The design is in ADR 0016 (Models): `.vih`, `.vil`, `.voh` and `.vol` on `Logic`, stated by a part relative to its rail, and a net-level check in the solver after the rails have their ranges, with "not evaluated when the grounds differ". Built by ADR 0046, with `Sn74lvc1g04` as the first real part that states levels.
- **A pin's absolute maximum voltage against the net's worst case.** A signal pin had no field to hold `.abs_max` and `.abs_min`, and the net's voltage was known only from a rail on it. A rail pin was already checked (`.vdd.voltage <= 3.6V` in the part, ADR 0016, decision 9). Built by ADR 0046: every pin has `.abs_min` and `.abs_max`.
- **GPIO direction: a function chosen that conflicts with the pin kind.** A pin had one kind and a list of function names (`fn GPIO UART1_TX`). Which functions are outputs and which are inputs was not in the table, and guessing from a name (`TX`, `RX`) would be a convention, not data. It needed a kind for each function in the `pins` block, which is a language change and so the user's to choose. Built by ADR 0046: `fn GPIO:bidirectional UART1_TX:output`.
