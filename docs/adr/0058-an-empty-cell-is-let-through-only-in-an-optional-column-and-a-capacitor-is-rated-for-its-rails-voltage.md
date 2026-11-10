---
status: proposed
---

# An empty cell is let through only in an optional column, and a capacitor is rated for its rail's voltage

Status notes: Written overnight on 2026-11-06 by an agent that was told to pick an option and go on, after an independent review of the day's diff. Each choice below is the agent's, with what it rejected.

An independent review of ADRs 0051 to 0057 found six defects. Three are about the bank and are in ADR 0052 ("After the review"). The other three are here.

## An empty cell: only an optional column is let through

ADR 0055 made the solver offer a row with an empty cell to a design that does not mention the column (`fits`: `(None, _) => !asked`), so that the columns added that day (`temperature_coefficient`, `rms_current`, `dielectric`), which are empty in the standard rows, do not take every row out of every design. The rule was written for all columns. The review showed two gaps. `asked` counted a narrowing and a relation but not a `~` join or a balance of currents, so a requirement that reached an empty-cell column only that way did not exclude the row. And a row with an empty `package` (or `resistance`, or a rating) was offered to a design that never mentions it, and the pick then failed later ("no package is chosen") or left the value unset, where before the change the row was never offered.

Decided:

- Only the columns of `table::OPTIONAL` may be empty in an offered row: `resistor.temperature_coefficient`, `inductor.rms_current`, `capacitor.dielectric`. For every other column an empty cell takes the row out, whether or not the design mentions the field, as before ADR 0055. A row with no package is therefore never offered, which settles "when anything else needs a footprint to resolve" without a rule about footprints.
- `asked` counts a field as asked when it is narrowed, named by a relation, joined by `~` (in `Model::equalities`) or summed by a balance. This only matters for the three optional columns (it is what lets a row with an empty cell be offered), and none of them is a field a `~` or a balance can reach in the standard kinds; it is made complete so that the rule does not depend on that. No test fails without this half in the final configuration (the gate above makes the join test pass either way); the join test fails if both halves are removed.
- The message that says how many rows were left out for an empty cell (`blank_note`) counts the columns that take a row out whatever the design says, and the asked optional ones.

Rejected: a rule for footprints in particular (an empty `package` is the one empty cell that fails late). It would be the second special case where one list covers it. Rejected: leaving it and documenting it: the row that is offered with no package is a build error with a message about pads, far from the cause.

The cost: a project's own catalog (a standard kind, or a user part's) with an empty cell in a column other than the three is as it was before ADR 0055, one row fewer. The three names are in one list in `table.rs`; a catalog column added to a standard kind later is added there.

## A capacitor is rated for the rail's voltage as well as its peak

`Rail.peak` ("the highest voltage the rail reaches, transients included") had no check against `voltage`, so a board that stated a peak below the steady range (`.supply.voltage &= 12V..36V`, `.supply.peak &= 20V`, a typo for 40V) let a capacitor rated for 25 V pass `max_voltage >= peak`, though the rail steadily reaches 36 V.

First decided (the same day): `BuckStage` states `.vin.peak >= .vin.voltage` and `.vout.peak >= .vout.voltage`. That was a bug. `>=` is "always at least": the peak's lowest value must be at least the voltage's highest. A board that says a rail has no overshoot, `.peak == .voltage`, states one value twice, but interval arithmetic cannot see that the two ranges move together, so a range of it is never "always at least" itself: the check refused exactly the statement that is true. The owner asked for it to be fixed on 2026-11-06.

Decided now: the two peak relations are removed. `BuckStage` states `.input_cap.max_voltage >= .vin.voltage` and `.output_cap.max_voltage >= .vout.voltage` next to the two ratings that read the peak. A capacitor is then rated for the steady maximum whatever the peak says, which is what the sanity check was for: a mistyped low peak can no longer let a capacitor through that the steady voltage would refuse. `.peak == .voltage` is accepted. A rating of a range against a range is no longer needed anywhere, so no dependency problem arises. With no peak stated, the steady-voltage rating is still checked (it was not before: the item "not done" in the first version of this section), and only the two peak ratings are listed as not evaluated.

What is given up: a peak stated below the steady voltage is no longer reported as a mistake of its own; it is harmless, because each capacitor must meet both numbers. The parts that read the peak elsewhere (an IC's absolute maximum against `.vin.peak`) are unchanged: a peak that is too low still understates the transient there, as it always could.

Rejected: comparing the upper ends of the two ranges in the language (`peak >= voltage` on their highest values): the language has no way to name a range's end, and adding one is a language choice. Rejected: teaching the solver that `a == b` makes `a >= b` hold: it is true only for these two fields and would be a new rule for every `==`. Rejected: leaving the check and having boards state the top of the range (the workaround used on a first full build of a real board): it moves a modelling choice into every board.

Rejected: the relation in `Rail` (the reviewer's suggestion for the original check). It needs a way to say "only when stated", which the language does not have (below).

## The not-evaluated lines of the stage's optional limits are left

With the IC's `min_inductor_saturation` and `max_inductor_dcr` open, the stage's relations over them are listed as not evaluated on every build, with the two ratings that read the peak, and now the two peak checks above: six lines for a design that states no peak and no limit (four before the peak checks). The review asked whether the stage can skip a relation whose limit is unset without losing a check that should run.

It cannot, with what the language has. A relation whose side has no value is listed as not evaluated: that is ADR 0057's rule and the guide's for every unset value, and the earlier optional numbers of the interface (`min_inductance` and the rest, when an IC leaves them open) behave the same, so the new limits are consistent with them. A module has no conditional (no "if the IC states it"), so skipping is a construct, and a language-design choice is the owner's.

Rejected: a default that makes the relation trivially true (`.min_inductor_saturation ?= 0A`; no neutral number exists for a largest resistance, so only half of the limits would be quiet, and the interface would show a number nobody stated). Rejected: dropping the notes in the presentation (`notes` in `crates/netsmith/src/lib.rs`) for these relations: it would hide a check that was not made, which is what ADR 0057 says must be named.

The guide now says that these checks are listed as not evaluated when the IC leaves them open (it said "has no such check").

## What to look at

- That an empty `package`, `resistance` or rating takes a row out again, and the three optional columns are the only exceptions (one list, `table.rs`).
- `BuckStage` rates each capacitor for the rail's voltage as well as its peak, and no longer checks the peak against the voltage.
- The six not-evaluated lines, and whether a way to write "only when the IC states it" is wanted.
