---
status: provisional, awaiting the owner's review (the rule is the first option of issue 221, which the owner chose on 2026-11-08; the reduction and its limits below are the agent's reading of it)
---

# Levels stated as fractions of one rail are compared at one supply voltage

## The problem

The levels check (ADR 0046, `crates/netsmith-solve/src/levels.rs`, `pair`) compares a driver's `voh` with a receiver's `vih` as two intervals: the driver's lowest high against the receiver's highest threshold. An interval counts a value once for each time it is written (ADR 0026, ADR 0076): `0.7 * .vdd.voltage` on a 3.3 V +/- 5 % rail is 2.194 V..2.426 V wherever it appears. A digital microphone that states `.out.voh &= 0.7 * .vdd.voltage` and a processor that states `.in.vih &= 0.7 * .vdd.voltage`, both on one rail, were refused: the driver's lowest high, 2.194 V, is below the input's highest threshold, 2.426 V. At any actual supply voltage the two are the same number, so the refusal was for a margin of a width that the supply's tolerance only seems to take away.

## Decision

A level that is, as written, a number times the voltage of a rail (`k * V`, `V * k`, or `V`, so `k = 1`) is compared with another level of that form as a fraction of one supply voltage, when:

- both levels are stated by one `&=` statement of that form and by nothing else: no constant narrows the field and no other relation mentions it;
- the `V` of each is the `voltage` of a rail whose `neg` is the level's ground (the reference rail's `neg`, as `pair` already requires of the two lines) and whose lowest voltage is above zero;
- the `pos` pins of the two rails are on one net.

Then `k_driver * V >= k_receiver * V` for every `V` in the rail's range is `k_driver >= k_receiver`, and the comparison is that, with no tolerance. A high is a fault when `k_driver < k_receiver`; a low (`vol` against `vil`) when `k_driver > k_receiver`. The message names the two levels, their factors and their rail voltages. Anything else (a sum, a clamp, a different net, a level a number also narrows, a rail that may be zero) is compared exactly as before, as two independent ranges, with the same message.

Nothing is added to the language or to `std/`, and a program with no such pair of levels is checked, and reports, exactly as before.

## Where the form is recovered

The model keeps a statement `.out.voh &= 0.7 * .vdd.voltage` as a `Relation` (`lhs: Field(voh)`, `op: Narrow`, `rhs: Bin(Mul, Const(0.7), Field(voltage))`) in `Model::relations()`, beside the range the solver gives the field. The field's domain does not remember the form: the check reads the relation. It is recovered once, per run, over the relations (`Run::fractions`). A level that is not the left side of exactly one relation, or whose `narrowed_at` is not empty, is not read as a fraction, because the field's value is then narrower than `k * V` says. Whether two rails have one voltage is read from the nets (the `pos` pins on one net, and the same ground), the way `pair` reads grounds, and not from the voltage fields: the net is the physical fact that decides it, and the check does not depend on how the model merges the fields of rails that a board joins.

## What was rejected

- **Compare the two at the corners of the rail's range** (`V` at its lowest and at its highest), the second option of the issue. For `k * V` against `k' * V` both give the same answer as the reduction, as the two lines are linear in `V`. It is more general (it would also accept `V - 0.1 V` against `0.7 * V`, any pair of affine forms, which is exact at the two ends) but it needs a form for each level (a slope and an offset), a decision for the forms that are not affine, and the corner of a rail that has a range of its own on each side. It is the right next step if the owner wants sums correlated, and is not built: a sum was left unchanged on purpose, so the rule is the smallest that makes the case in the issue pass.
- **Leave the check and have the board add margin** (a series resistor). That moves a fault in the model into the circuit, to satisfy an arithmetic that counts a value twice.
- **Match on the text of the two statements.** The structure is enough, and text would miss `.vdd.voltage * 0.7` and a part that spells its rail another way.

## Limits

- Division (`.vdd.voltage / 2`) is not read as a fraction: the factor would be a rounded `1 / k`, and the comparison of two rounded numbers is not exact. Write the factor (`0.5 * .vdd.voltage`).
- A factor that is a range (`0.65..0.7`) is not read as a fraction.
- `pos` pins on one net is what makes the voltage one number. Two rails that a board gives the same range on two nets (two regulators of 3.3 V) are two voltages, and their ranges are independent, as before.

## Left open

For the owner: whether to extend the rule to affine forms (a fraction or the supply with an offset, `.vdd.voltage - 0.4V`), which a corner comparison decides exactly; and whether the pull-up path of ADR 0075 (an open-drain high that is the pull-up's rail) wants the same treatment for a receiver at a fraction of that rail.
