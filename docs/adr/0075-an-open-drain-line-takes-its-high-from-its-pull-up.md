---
status: provisional, awaiting the owner's review (chosen by the agent on a bug found on a real board, with no new syntax; the rule that a driver is open-drain when it states `vol` and no `voh`, and the option of a declared open-drain pin, are the owner's to confirm or replace)
---

# An open-drain line takes its high from its pull-up

## The problem

Two parts on an `I2c` bus state `.i2c.levels.vil`, `.vih` and `.vol` and no `.voh`, as their datasheets do: an open-drain pin does not set its high, the pull-up does. The levels check (ADR 0046, ADR 0064, `crates/netsmith-solve/src/levels.rs`, `pair`) compares a driver's `voh` with a receiver's `vih`, and with no `voh` it could not, so every ordered pair of devices on each line was a note:

```text
note: not evaluated: the high level from `monitor.sensor.sensor.i2c.levels` to `processor.p1_14` on net `I2C_SDA`: `monitor.sensor.sensor.i2c.levels.voh` has no value
```

The notes were true and no help. The check that mattered, that the pull-up's rail is a high for every reader, was never made, and a bus pulled up to 1.8 V with a 3.3 V reader would have passed in silence.

## Decision

In `pair`, a driver that states `vol` and no `voh`, on a net that has a pull-up, has as its high the voltage range of the pull-up's rail. Each receiver's `vih` is compared with it: the lowest end of the rail's range must be at least the highest end of `vih`, as for a `voh`. The finding names the net, the pull-up resistor, the rail, the receiver's `vih` and the numbers. The low is judged as before, `vol` against `vil`.

A pull-up is a resistor (a part with a `bridge` and a `resistance`) from the net to a net that holds the positive pin of a rail: `std::bus::I2cPullups`, `std::bus::PullUp` and a plain `Resistor` between a rail's `pos` and the line are all recognised, because the rule reads the same structure that `net_range` reads for the absolute maximums (ADR 0046) and that the I2C pull-up rule reads (ADR 0044). Several pull-ups give the hull of their rails.

Nothing is added to the language, the checker or `std/`, and no part changes. The rule applies to `Logic` lines and to the pins of a bus interface alike, as both go through `levels.rs`.

## What does not change

- A driver that states `voh` is judged by it, whatever is on the net. A push-pull output with a pull-up (a reset line with a resistor) is checked as before.
- A driver that states no `vol` says nothing about its output: its high and its low are notes, as before.
- A net with a resistor to its own ground as well is a divider, not a pull-up: the rule does not apply and the high has no value, a note as before.
- A rail whose ground is not the receiver's ground, or a rail with no voltage, is a note that names the rail and the reason, never a pass.
- The absolute maximum check already followed a resistor to a rail and is unchanged.

## What was rejected

- **`I2cPullups` and `PullUp` state a `voh` on their bus.** A module is configured through its interface and cannot set a member of a part inside another module (`docs/DESIGN-RULES.md`), and its `.bus` is an interface of its own, joined to the part's interface pin by pin: `~` between two bus interfaces joins no levels (ADR 0064), so a `voh` stated on `.bus.levels` would be read by no driver. Making `~` join levels contradicts ADR 0064's reasons (a driver and a receiver would equate their levels). It also fixes the high to the module's own rail when a board may have a second pull-up.
- **A pin kind or marker for open-drain** (`open_drain`, KiCad's `open_collector`, ADR 0064 question 10). It would make the datasheet's fact data instead of an inference, and would let the I2C name test in `buses.rs` go. It is new syntax, so it is the owner's choice and is not built; see below.
- **Ignoring a stated `voh` when a pull-up is on the net.** A part that states one means it, and a pull-up on a push-pull line (a reset) is a normal circuit.

## The inference, and its limit

"States `vol` and no `voh`" is how an open-drain pin looks in a datasheet, but it is also how a push-pull part with an incomplete model looks. Such a part on a net with a pull-up has its high taken from the pull-up and not reported as missing. The note it used to get was the only sign that the model is incomplete. The rule is narrow (it needs `vol`, a pull-up, and a rail with a voltage), and an incomplete model whose net has no pull-up still gets its note, but the risk is the owner's to weigh.

## Left open

For the owner: whether to keep the inference, or to add a way for a part to say that a pin is open-drain (a pin kind, or a marker on the interface's `lines`), after which the rule would apply to a declared pin and a part that states `vol` alone would be a note again. If the pin kind is chosen, this rule's test for a driver changes and nothing else.

A chain of resistors is not followed (one resistor from the net to a rail, as `net_range` reads a pull); the I2C pull-up rule of ADR 0044 follows a chain. A pull-up behind a series resistor or a switch is a net with no rail and keeps the note.
