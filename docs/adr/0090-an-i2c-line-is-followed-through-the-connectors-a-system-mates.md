---
status: provisional, awaiting the owner's review (the smallest reading of issue 63's "the undriven-input and I2C pull-up checks do not find a pull-up made of anything but resistors, or one on the other side of a connector" is built, by an agent, with the alternatives it did not take under "Choices for the owner")
---

# An I2C line is followed through the connectors a system mates

Extends ADR 0044 ("An I2C line with no pull-up is an error") and uses ADR 0040 (connectors mated in a system).

## The problem

ADR 0044 leaves a line that reaches a connector unjudged, because the pull-up may be on the other side. That is the right answer for a board alone, and a hole in a system that mates it: a carrier and a card whose I2C lines meet in a mate, with no pull-up on either board, was never reported. The issue also names a pull-up made of something other than resistors.

## What was checked, and what was found

The finder reads a pull-up as a path of parts that have a `bridge` and a `resistance` from the line to a net with the `pos` of a rail. A test now pins what that already covers: resistors in series, in parallel, a divider (a resistor to ground on a node of the path), `std::bus::PullUp` and `std::bus::I2cPullups` (whose resistors are inside a module: the nets pass through the module's ports). A resistor to a net that no rail is on, and a line tied to a rail or pulled down, are not pull-ups. Nothing needed a change there.

The undriven-input rule (a net of only inputs) already counts every passive pin as a driver: a resistor of any network, a `PullUp`, a connector pin. Tests pin it. Nothing needed a change there either.

## Decision

In a model that has mates (a `system`), a line that leaves its board through connectors is followed. From each connector pin on the line's net, the mate gives the pin it meets (contact N of one is contact N of the other), and the line is looked for, by the same finder, on the net of that pin in the other board, and through any connector there.

- If any of those nets has a pull-up, the line is satisfied, wherever the pull-up is.
- If a connector on the way has no mate in the model (an unmated header, a mate that did not read), or the line reaches a port of a board, the line goes somewhere the model does not show and is not judged, as before.
- If every connector is mated and no board has a pull-up, the line is an error, once for the bus and not once per board, naming the connectors the line passed and the pins of the bus on every board it reaches: `net `net-a.i2c.scl` carries the scl line of an I2C bus (`carrier.a.i2c.scl`, `card.b.i2c.scl`) and has no pull-up, on this board or on the boards its mated connectors lead to (`card.h`, `carrier.j`): ...`.

A board checked alone behind its connector is not judged, as before: a mate is the one thing that says where the connector leads.

A part that is not a resistor and pulls a line up (a current source, a bus buffer with a built-in pull-up) is still not found: nothing in the part says that it is one, and the checker does not guess from a name.

## Choices for the owner

- **A marker for a pull-up that is not a resistor.** A part states that a pin pulls its line up, for example a `pins` row flag (`.sda bidirectional "SDA" "3" pulls_up`), or an interface member. The checker would then count the pin as a pull-up. It needs a new word in the `pins` grammar (a language change) or a field-name convention, and the owner decides the form. Not built; a board with such a part states `I2cPullups` or a resistor beside it, or is left with the error.
- **A pull-up beyond an unmated connector.** A board that goes out through a connector to a board outside the design (a header to a harness) could say so, for example `.bus.pulled_up_outside &= true`, which the checker would take as the other side's pull-up. It is a new member of `I2c`, and today the unmated connector already stops the check. Not built.
- **A guideline instead of an error across a mate.** A bus with no pull-up anywhere is an electrical fact (ADR 0044), so it is an error here too. A guideline would let a design that is still being drawn build.
- **Following mates for other rules.** The undriven-input rule could follow a mate (a net of inputs on one board may be driven on the other). It counts a connector pin as a driver, so it has no false alarm to remove. Not built.
- **Where the error points.** At the board holding the first instance of the interface, as before. A system-level location (the mate statement) would say more; the paths name both sides.

## Cost

A system that mates a bus with no pull-up on either side is refused now. The error is an electrical fact, and each board checked alone is as it was. A system whose I2C lines cross a connector that it does not mate, or reach a port, is as it was.

## Left open

- A pull-up that is not a resistor (above), and a pull-up outside the design, beyond an unmated connector.
