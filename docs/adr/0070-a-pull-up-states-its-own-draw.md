---
status: proposed for owner review (the module `PullUp` and the marker `required` on its resistance were decided by the owner on 2026-11-06 in principle; the placement, the names, the draw model and the decision to leave `I2cPullups` alone were made by an agent that was told to pick an option and go on, with the alternatives it rejected)
---

# A pull-up states its own draw

A resistor from a rail's positive pin to one signal line, the pull-up of a reset or an enable or an open-drain output, is a chain that starts on the rail's net. A plain `Resistor` leaves its current unknown. The net then has two unknowns, the resistor's through-current and the rail's current, so the balance of the net cannot be solved (ADR 0026) and every check that reads it waits: the regulator's output current, its power, the resistor's own power. The build lists them as not evaluated. ADR 0044 and `I2cPullups` solved this for the two lines of an I2C bus. A pull-up on any other line had no module.

## The decision

`std::bus::PullUp` is one resistor and the statement of what it draws.

```nsm-fragment
module PullUp {
    .supply: Rail
    .line: Pin
    .resistance: Resistance required
    .resistor: Resistor
    .resistance == .resistor.resistance
    .supply.pos ~> .resistor ~> .line

    .line_low: Number
    .line_low &= 0..1
    .resistor.current == .line_low * .supply.voltage / .resistor.resistance
}
```

The board states the resistance, as a value with a tolerance (`.pull.resistance &= 10kohm +/- 5%`), joins `.supply` to a rail and `.line` to a pin. The line is held low for the share `line_low` of the time, 0 to 1, at worst, and the draw is `line_low * supply.voltage / resistance`: 0 up to `supply.voltage / resistance`. It is the model of `I2cPullups` (the lowest the line goes is 0 V, the line gives no low level of a driver, so this is the worst case and a driver's real level draws a little less). A board that knows its line is low only part of the time narrows `line_low` (`&= 0..0.1`). A current the board states on `supply` is a budget (ADR 0038), and two `PullUp`s on one rail add on its net: both are tested.

The members are the owner's interface (`.supply`, `.line`, `.resistance`, the resistor inside, a stated draw). `.line_low` is the owner's name for the share, and the resistor is called `.resistor`.

## `.resistance` is required

Note of 2026-11-06 (ADR 0065): `required` was removed, so `PullUp.resistance` is a plain member now. A board that forgets it gets the finding of ADR 0065 for the pull-up's resistor, with the member named, and a current budget stated on the supply limits the pick. The text of this section is the decision as it was made.

`PullUp.resistance` is the first required member in the library (ADR 0069). A board that forgets it is refused, and the solver does not pick the first row of the resistor catalog. `I2cPullups.resistance` is not marked required here, because that changes what builds today for a board that forgets it. The change is one word, and the owner decides.

## Where it lives: `std/bus.nsm`

Next to `I2cPullups`, which has the same circuit twice. The file already imports `Resistor` and `Rail`, and a pull-up is a circuit around a line. `std/parts/passive.nsm` holds parts and imports nothing: a module there would make it import the rails, and it would put a circuit with a rail among two-terminal parts. The module regulators, `BuckStage` and `FeedbackDivider`, sit beside the parts they are built from; the cost of this choice is that a reader looking for a pull-up looks in `bus`, and the file's own header and the guide say that it is also for single lines. The doc of the module says so as well.

## `I2cPullups` is left alone

It was to be written with two `PullUp`s only if every existing test, example and guide block stayed the same, with the same picks, the same trials and the same messages. It does not:

- Tried, with the two `PullUp`s as `.sda_pullup` and `.scl_pullup` and each `.sda_pullup.resistance == .resistance`: `examples/enclosure_sensor` builds byte-identical to the snapshot of stage 2 (the same files and the same notes), and `netsmith-check`'s `nets` tests and the guide's blocks pass. Five existing tests fail, so it is not the same: `std_models.rs` reads `pullups.sda_pullup.current` and `pullups.sda_pullup.resistance` (the resistor is now at `pullups.sda_pullup.resistor`, so `i2c_pullups_state_their_supply_current_so_the_net_is_solved`, `the_pick_of_the_pullup_resistors_does_not_change` and `a_pullup_of_a_few_ohms_is_picked_for_its_power_now_that_its_current_is_known` fail), `std_kinds.rs` lists the picked parts by path (`the_i2c_pullups_pick_resistors_inside_the_stated_resistance` gets `pullups.sda_pullup.resistor` where it expects `pullups.sda_pullup`), and `a_current_the_board_states_on_the_pullup_supply_is_a_budget` finds `pullups.supply.current` still at the stated 0A..2mA where it expects the sum of the two draws (1.4 to 1.6 mA): through two inner modules the budget is not narrowed to what the circuit draws, which is a difference in behaviour and not only in paths. A member's path is what a board and a test name, and keeping the old paths needs the wrapper to declare `.sda_pullup` as a `Resistor`, which is what it is now.
- The first try had `.resistance == .sda_pullup.resistance`, the direction `I2cPullups` states it in today. With the two `PullUp`s the resistor inside stays unset, because the solver evaluates a relation with a field alone on its left from the right side only, and the wrapper's value is on the left. The pick was 330 ohm and the build was refused (`pullups.resistance must stay within 4.465kΩ..4.935kΩ, but pullups.scl_pullup.resistance works out to 326.7Ω..333.3Ω`). That is the case `required` (ADR 0069) now catches: a member on the right of an `==` is read, not stated.

`I2cPullups` keeps its own text. The two circuits repeat, and one change to the draw model needs two edits. That is the cost.

## What was tried and rejected

- `PullUp` with the pull-up resistor's draw as a `?=` default on the supply. A default is the module's offer, and a draw is a fact about the circuit, not an offer.
- A pull-up as a `generic` that the solver picks a provider for. There is one circuit; there is nothing to pick between.
- A `PullDown` to the `neg` pin. A pull-down has the same equation with the other rail pin; it can be added with the same file when a board needs it (one correct way: not a flag on `PullUp`).

## What to look at

- The names `.line`, `.line_low`, `.resistor`, and that `.resistance` is `required`.
- That the draw is the worst case 0 to `supply.voltage / resistance` unless the board narrows `.line_low`.
- That the module is in `std/bus.nsm` and not in `std/parts/passive.nsm`.
- That `I2cPullups` is not re-expressed, for the reason of the paths, and that its `.resistance` is not `required`.

## Note of 2026-11-06 (second review): what the draw assumes

A second review found that `0 .. supply.voltage / resistance` is stated as a fact with two assumptions that it did not say: that no driver holds the line above `supply` (a 5 V push-pull driver on a line pulled to 3.3 V pushes current back into the rail, a negative draw, so a budget that counts only draws would pass), and that the rail has no transient (the draw uses `supply.voltage`, the steady range, not `supply.peak`). The owner asked for both to be modelled, with results unchanged where nothing states them.

The smallest sound change is two members that `PullUp` and `I2cPullups` offer with a default (`?=`, ADR 0051), and an equation that reads them:

```nsm-fragment
    .overdrive: Voltage
    .overdrive ?= 0V
    .supply_top: Voltage
    .supply_top ?= .supply.voltage
    .resistor.current == (.line_low * .supply_top - .overdrive) / .resistor.resistance
```

- `.overdrive` is how far above `supply` a driver holds the line, at most. It is 0 V, so the lower end of the draw is 0 as before; a board with such a driver states it (`.pull.overdrive &= 0V..1.7V`) and the draw reaches down to `-overdrive / resistance` (-171.8 uA with 10 kohm +/- 5% and 1.7 V in the test). The net then sees a current that can be negative, and so does the regulator behind it, so its checks are made with it.
- `.supply_top` is the voltage the draw is worked out at. It is the rail's steady `voltage`, which is what a budget and a power rating read. A board that wants the draw at the transient its rail can reach states `.pull.supply_top &= .pull.supply.peak` (606 uA at 6 V in the test, 350 uA at the steady 3.465 V).

With nothing stated the results are the ones before: the same ranges (the default of `.overdrive` is the point 0 V, so `x - 0` and the quotient are exact), no new note and no new finding, which the tests of the pull-ups, `examples/enclosure_sensor` (it uses `I2cPullups`) and the guide check.

Rejected: reading `supply.peak` in the equation. A relation that reads an unset value waits and is listed as not evaluated, so every pull-up on a rail with no stated peak would print a note that it did not print before, and its resistor's current would be unknown (the fault this ADR fixed). A default with the rail's peak has the same effect. There is no way in the language to read a value only when it is stated, and adding one is the owner's decision; the board states `.supply_top` instead, and the member is the documented place to do it. Rejected: widening the lower end always, to `-(peak / resistance)`: it makes every budget on a pull-up's rail fail or lose its meaning for a board with no such driver. Rejected: only writing both assumptions in the doc: a board that has the driver then has nothing to state.

Not changed: `.line_low`, which is the share of the time the line is held low, 0 to 1. The time a line is driven above the supply is not a share the module models; `.overdrive` is the worst case the line reaches, and the draw's range is the span from it to the draw with the line low, so the two cannot be told apart in time (the interval is wider than a mix of the two would give, never narrower).
