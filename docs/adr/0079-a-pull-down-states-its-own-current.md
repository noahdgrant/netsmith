---
status: provisional, awaiting the owner's review (a std module in today's language, no new syntax; the names `reference`, `line_high` and `line_top`, and the choice that the module draws nothing from a rail, were made by an agent with the alternatives below; the pin's input current was not built and is left to the owner, see the end)
---

# A pull-down states its own current

## The problem

A resistor from a logic line to ground, the pull-down of a boot-mode pin, an enable or a gate, is a chain that starts on a signal pin. A plain `Resistor` leaves its through-current unknown: the net of the line is not balanced (a balance exists only for a net a rail draws from or returns to, ADR 0007, ADR 0026), so nothing sets it. The drop and the power of the resistor then wait, and the report has a note for each, `the current of part ... is not known`. The Ohm bound (the amendment of ADR 0026) proves the power of a resistor whose two ends have known voltage ranges, and sets no current, so every other check that reads the current still waits, and the track width of the net stays a lower bound.

## Decision

`std::bus::PullDown` is one resistor from `.line` to the negative pin of `.reference`, and the statement of what it carries. It mirrors `PullUp` (ADR 0070): the board states `.resistance`, the module states

```nsm-fragment
.resistor.current == .line_high * .line_top / .resistor.resistance
```

with `.line_high` the share of the time the line is high (0 to 1, as `PullUp`'s `line_low`) and `.line_top` an offer (`?= .reference.voltage`, ADR 0051, as `supply_top`): the highest voltage the line is held at above the ground of `reference`. The current is 0 up to `line_top / resistance`: the line is high while a driver puts current into the resistor, and low or released while it carries nothing.

The module draws nothing from a rail, because the current comes from the line's driver, whose supply is not the module's. `.reference` is therefore a reference and not a load, as the `.reference` of `Levels` is, and states `.reference.current &= 0A` so that joining it to the board's rail leaves no unknown on that net (ADR 0066, ADR 0077).

## Rejected

- Naming the rail `.supply` and stating a draw on it, as `PullUp` does. A pull-down returns into the rail's `neg` what the driver sent; it takes nothing from `pos`. A stated draw would be a lie that makes the net's current wrong.
- A pair, `.line_low` and `.line_high`. One share is enough: the resistor carries current only while the line is high.
- A negative current for a line driven below ground (`overdrive` of `PullUp`). No driver in the cases at hand does it, and a board can state `.line_top` with a negative end for the current the other way. Add it when a board needs it.
- A bare range `0..line_top / resistance` without the share. The share is what `PullUp` offers a board that knows its line is low only part of the time, and a board that knows its line is high only part of the time has the same need.

## What it does not do

- It does not give the driver's output current, nor the net's balance: the signal net still has none. A check that needs the driver's current waits as before.
- It states no input current for the pins on the line. An optional `.input_current` on `Logic` was asked for (issue 192). It cannot join a balance that does not exist, a leakage as a current source was refused in ADR 0076 section 8, and owner decision 2 of ADR 0076 asks for the datasheet's own form of `Ii`, shown to the owner first. It is left open and the options are in the issue.

## What to look at

- The names, and that `.reference` is a `Rail` whose current is stated 0 A, as `Levels` does it.
- That the share is of the line being high, and that `.line_top` is the board's to widen for a stronger driver.
