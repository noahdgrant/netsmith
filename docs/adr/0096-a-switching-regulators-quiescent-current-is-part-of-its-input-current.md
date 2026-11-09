---
status: provisional, awaiting the owner's review (the engineer took the fourth operand that ADR 0076, amendment 9 left as choice 1(d), a range from zero, and the larger mode's figure as the default; each alternative is under "Choices for the owner")
---

# A switching regulator's quiescent current is part of its input current

## The problem

The std `Lt8641` declared `.iq_max &= 10uA` and nothing read it (issue #291). Its input current was `-.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)`, so with the output unloaded the solver proved the converter draws exactly 0 A, and a sleep-current budget summed from `.vin.current` passed whatever it was. `LinearRegulator` has had its quiescent current in the input current and in the DC analysis since ADR 0076, amendment 9; that amendment left the same for a switching regulator as choice 1(d) ("a fourth operand for `dc switching_regulator` too ... not taken because the figure of a buck stage is a function of its input and of the mode (the LT8641's `.iq_max` is 'not switching, VEN/UV = 2 V, SYNC/MODE low'), which a range does not say").

The datasheet (LT8641 Rev. C, the PDF the part cites, SHA-256 `09763bcb...965a43`, read again with `pdftotext -layout`):

- Electrical Characteristics, p.2, "VIN Quiescent Current": VEN/UV = 0 V: 0.75 uA typical, 3 uA at 25 C, 10 uA over temperature; VEN/UV = 2 V, not switching, VSYNC = 0 V: 1.7 uA typical, 4 uA at 25 C, 10 uA over temperature; VEN/UV = 2 V, not switching, VSYNC = 2 V: 0.3 mA typical, 0.5 mA maximum (no row over temperature).
- Pin Functions, p.11: SYNC/MODE tied to ground is Burst Mode, "ultralow quiescent current"; floating is pulse-skipping; tied high to INTVCC or a 3 V to 4 V supply is pulse-skipping with spread spectrum; a clock is synchronization. Operation, p.12: in pulse-skipping mode, in light loads, "the quiescent current will be several hundred uA".

So the 10 uA is the no-load supply current of the Burst Mode only, which `SYNC/MODE` tied to ground selects; with the pin floating (which the datasheet allows), held high or clocked it is several hundred uA, and the only number is 0.5 mA at 25 C.

## Decision

`Lt8641` has `.quiescent_current`, a range of currents with the default (`?=`, ADR 0051) `0A .. .iq_skipping_max`, `.iq_skipping_max` being the 0.5 mA of the VSYNC = 2 V row, and its input current and its DC element carry it:

```nsm-fragment
.quiescent_current: Current
.quiescent_current ?= 0A .. .iq_skipping_max
.vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency) + .quiescent_current
dc switching_regulator .vin .buck.sw .buck.efficiency .quiescent_current
```

A board that grounds `SYNC/MODE` states the ultralow figure, which replaces the default: `.u.quiescent_current &= 0A .. .u.iq_max` (10 uA). `.iq_max` keeps its name and its 10 uA.

`dc switching_regulator` takes an optional fourth operand, as `dc linear_regulator` takes its third (`DcKind::optional_value_roles`): a field of the part that is a current, none of whose values is below zero. The analysis draws `Pout / (eff * Vin) + Iq`, with the ends of the range taken as corners (the draw is the largest at the highest `Iq`, and the corners include `Iq` at each end with every corner of the power and the efficiency, which is wider than the circuit and never narrower). A statement that names a field with no value, or a negative one, leaves the regulator unmodelled, as for a linear one. A statement without the operand is as before. In the solver, `Draw::Switching` has the quiescent current (`iq`, a point at zero when the part states none) and the fixed point adds it: `want = p / (eff * vin) + iq`.

The least the input draws is still zero: 10 uA is a maximum over temperature, with 1.7 uA typical. The issue's expected result reads as "`u.vin.current` of at least `.iq_max`", and a line `.u.vin.current >= 10uA` still fails, as it should: the datasheet never says the part draws at least 10 uA. What the change gives is the upper end: an unloaded converter's input is `0 A .. 10 uA` in Burst Mode and `0 A .. 0.5 mA` by default, so a budget of 5 uA is refused in the first, and a budget of 100 uA in the default.

## What was rejected

- **`.vin.current >= .iq_max`** (a lower bound of 10 uA). It is a maximum read as a minimum: it would claim, falsely, that the part always draws 10 uA, and a design that needs the least current (a battery estimate that credits the converter's sleep) would be wrong.
- **10 uA as the default.** True only if `SYNC/MODE` is grounded, and the part cannot see how the board ties it: a board that leaves it floating (allowed) would be judged at a twentieth of what the datasheet says, the false pass the issue is about. The default is the larger figure, and the cost is one line for a board that grounds the pin.
- **The 0.5 mA as a datasheet guarantee.** It is the table's 25 C maximum (no row over temperature), and the text says "several hundred uA": it is the only number the datasheet gives, taken for the other modes, and not a limit over the junction temperature range.
- **Reading the member by name in the analysis** (amendment 9, choice 1(a)): a part that calls it `.iq_max` would be silently not modelled; the operand names it.
- **A mode member and a conditional** (one figure per mode, picked by a member the board states, or by the voltage of `SYNC/MODE`'s net): it needs a way for a relation to choose, which the language does not have. The owner's.
- **Adding the stage's feedback divider here.** The divider is a load on the output (about 350 uA at 3.3 V in a test board, which at the input is tens of microamperes): `BuckStage` keeps it apart as `.divider_current`, and the part's relation is the IC's own current. The DC analysis, which does solve the output side, has a larger input current than the part's relation in that case (72.63 uA least in the test board); where it disagrees with the relation its figure is not written, which is the existing behaviour (see #296) and not changed here.

## Cost

One member and three members' worth of text in the part, one operand in `DcKind::optional_value_roles`, one field in `Draw::Switching` and `Point::Sink`, and a factor of two in the combinations of a regulator whose quiescent current is a range. A board with a std `Lt8641` has its input current's upper end 0.5 mA higher (10 uA with the Burst Mode statement), which is small beside its load and visible in a budget. The real 12 V automotive board has its own LT8641 part and builds as before.

## Choices for the owner

1. The fourth operand of `dc switching_regulator` (taken: it is amendment 9's choice 1(d), now with the mode problem handled by the default and the board's statement). Alternatives: the name-based read, a mode member and a way to choose by it, or a statement per mode.
2. The default: the pulse-skipping figure (here), against the Burst Mode's 10 uA (the datasheet's headline number: the part's `.iq_max` doc comment had it, and a board that floats the pin would then be judged at the wrong figure), or no default and a required statement.
3. The floor of the range: zero (here, as for `LinearRegulator`), against the typical figure as a floor, which the datasheet gives for Burst Mode only.
4. Whether the 0.5 mA, a 25 C figure, is a fair upper end for the modes that have no number over temperature, or the part should state none for them and leave the default open.
5. Whether the stage's divider belongs in the IC's input current (it does at the physical converter) and the relation and the analysis should agree on it; it is #296's.
