---
status: provisional, awaiting the owner's review (chosen by the agent for issue #199 with no new syntax; the unit of the coefficient, the name and kind of the shift, which marker guards what, and the relation's form are the owner's to confirm or replace)
---

# A Zener states its figures over temperature in optional columns

## The problem

A real 12 V automotive board held a supply pin under its absolute maximum with a series resistor and a Zener across it. The Zener's voltage range is published at a test current and at 25 C; the board needed the voltage at the hot and the cold end of its temperature range. It stated the test current, the temperature shift and the leakage by hand in the board, with the datasheet page in a comment, and nothing tied them to the row the solver picked: another row would pass in silence with the wrong numbers (issue #199).

## Decision

`std::parts::diode::Zener` gains members, and the standard `zener` catalog the columns for four of them, each marked `optional` in the `catalog` statement (ADR 0078):

- `.test_current: Current`, the current `.zener_voltage` is published at (IZT). `.zener_voltage` already is the datasheet's minimum to maximum of VZ at that current, so no second pair of members for the minimum and the maximum is added.
- `.temperature_coefficient: Voltage`, the datasheet's range of the change of the voltage for one kelvin. A row writes `18.4mV..22mV` for "18.4 to 22 mV/C".
- `.leakage_current: Current` and `.leakage_test_voltage: Voltage`: the reverse current below the breakdown and the voltage it is stated at, as two members (a member holds one quantity).
- `.temperature_shift: Temperature optional`, kelvin away from 25 C, that a design states (`-65K..100K` for -40 C to 125 C).
- `.voltage_over_temperature: Voltage`, with the relation `.voltage_over_temperature == .zener_voltage + .temperature_coefficient * (.temperature_shift / 1K)`. Interval arithmetic gives the hull: the upper end is the hot-corner voltage (`vz_max + coefficient_max * shift_max` when the coefficient and the shift are positive at their ends) and the lower end the cold-corner voltage.

A design that states no `.temperature_shift` has no such relation: it is skipped by the `optional` marker (ADR 0074), so nothing is listed for it and the coefficient column is not asked for, and every existing board picks the rows it picked. A design that states the shift asks for the coefficient, so only a row that states it is offered. A bound on `.voltage_over_temperature` with no shift is listed as not evaluated, as any check on a value nothing gives.

Figures are not guessed: each of the seven existing rows states all four columns from the datasheet table, with the table and page in `notes`, or states none (a catalog test requires all four together).

## What was rejected

- **The coefficient as a `Number` fraction per kelvin, like a resistor's.** The datasheet publishes millivolts per kelvin; a fraction needs a division by a voltage that is itself a range, which would be a figure the datasheet does not give. A `Voltage` per kelvin keeps the datasheet's number and its unit. No kind holds volts per kelvin; a new quantity kind is a language choice, not made here.
- **`.temperature_coefficient` marked `optional` as a member (ADR 0074).** ADR 0074 says a value a catalog row gives is never a statement, so the relation would be skipped for every design that does not state the coefficient by hand, and the row's figure would be ignored: the opposite of the issue. The member marker guards what a design states (the shift); the column marker (ADR 0078) lets a row be empty.
- **`.temperature_shift` as a plain `Number`.** `Temperature` exists (kelvin, a difference). The relation divides by `1K` to reach a plain count.
- **Absolute temperatures, with `25C` in the library.** There is no absolute-temperature literal; a shift from the datasheet's 25 C is what the datasheet states.
- **A separate hot and cold member.** One range carries both corners and a design asks for either end with `<=` or `>=`.

## Cost and limits

- The datasheet's coefficient is a typical figure, not a bound on a part. The corners are typical-based; the margin is the design's.
- The test current of the coefficient is `IZTC`, which equals IZT in every row filled; a datasheet where they differ needs another member.
- The Zener's voltage is the 25 C range at the test current; the dynamic impedance at another current is not modelled.
- Rows beyond the existing seven (for example a 39 V part whose figures are public) need a person to read their library type on JLCPCB (ADR 0039), so they are not added here.
- The diode catalog's line numbers in messages that quote `std/parts/diode.nsm` moved; the guide's `Tvs` message was re-blessed.

## What to look at

- The coefficient as volts, and that `Voltage` stands for volts per kelvin.
- The shift as a `Temperature` member that a design states, and that the relation is skipped without it.
- The choice of which marker guards what, above all that the coefficient is a column marker and not a member marker.
