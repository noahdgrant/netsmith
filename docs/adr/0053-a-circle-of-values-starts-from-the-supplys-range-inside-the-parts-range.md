---
status: proposed for owner review (written overnight on 2026-11-05 by an agent that was told to pick an option and go on; what counts as a state of a part, below, is the owner's to accept or change)
---

# A circle of values starts from the supply's range, inside the part's range

A switching regulator makes a circle of values. Its input current is the power it passes over the efficiency and the input voltage, and its input voltage is the supply's less the drop that current makes in what is in series. Propagation sets a field from an equation only when everything the equation reads has a value, so with no value in the circle nothing starts: the build names the circle in a note (`crates/netsmith-solve/src/cycles.rs`, added earlier on 2026-11-05) and the checks that need it are not evaluated. A board then states a range on one field by hand. The first board of the real design did it with `3V..65V`, the regulator's whole input range, and the solver settled at 4.4 A where the true worst case was 1.3 A.

## What was found

The loose answer is not the interval arithmetic widening, and not an order of evaluation. It is a rule of `propagate` (`crates/netsmith-solve/src/solve.rs`, the equation on a field that already has a range): the equation narrows the range only when the value the equation gives lies wholly inside it. A range written with `&=` is a claim that the check holds against ("`load.voltage` must stay within 3V..30V"), so narrowing it to the overlap would make a check pass for a range nobody guaranteed. That rule is right and stays.

In a circle it has this effect. A load of 0.5 W behind 36 ohm of series resistance on a supply of 9 V to 16 V, with the load's voltage started at its whole range `3V..30V` (invented values, `crates/netsmith-solve/tests/seed.rs`, the last test): the first pass works the current out at the bottom of the range, 3 V, so it is 0.5 W / 3 V, up to 166.7 mA. The drop at that current is 6 V, so the equation gives 2.94 V to 15.41 V. That reaches below 3 V, so it is not inside `3V..30V` and does not narrow it. The voltage stays at 3 V, the current stays at the value worked out from 3 V, and the equation gives 2.94 V again. It is a fixed point of the propagator, not of the circuit, and it is the whole answer: the board gets "must stay within 3V..30V, but ... works out to 2.94V..15.41V". With a smaller drop the first pass lies inside the range, the equation narrows it, and the answer is tight. So whether a board gets the tight answer or the loose one depends on how large the first pass's drop is, which is a property of the start and not of the circuit: that is the looseness.

A start taken from the other end does not have this effect. It is the supply's range (9 V to 16 V, which the board states). For the 10 ohm of the other tests the equation maps it into 8.44 V to 15.7 V, and it settles at 8.398 V to 15.69 V, the range the circuit really works in. But it is a claim and not a derived value, and the circle has more than one solution. A constant-power load behind a resistor has a second working point near 0 V for any resistance: `v * v - s * v + r * p = 0` has two roots, 8.4 V and 0.6 V for 10 ohm, 9 V and 0.5 W. The interval arithmetic cannot tell the points apart, and no range stated by the board decides between them either: the hand seed was a claim that the circuit does not leave the range, which the equation checked (and, at 3 V, could not show).

## Decision

When a circle is found (the note's group) and nothing in it has a value, the solver tries to start it itself, once before the parts are picked and again after, for a circle that a pick closed:

1. The field to start (the cut) is a field of the circle whose own equation (`==`) sets it. Its start is the hull of the ranges of the values of the same kind that the equation reads, directly or through other fields without a value, that already have one: the supply's voltage for a voltage. If there is none, nothing is done.
2. The part's range is what the relations (`>=`, `<=`) on the field or the fields joined to it by `~` bound, as values the model has: the lowest lower bound and the highest upper bound. If either end is missing, nothing is done.
3. The start must be inside the part's range. It is grown until the equation maps it into itself (`propagate` with the field held to it gives a value inside it; an end that moves is moved twice as far, so the sequence does not creep), then narrowed to what the equation gives from it until it stops moving, with a room of one part in 10^9 so that rounding in the last digit cannot fail the equation's own check. If it leaves the part's range, or does not settle in 64 steps, nothing is done.
4. The rest of the part's range, below the claim and above it, is shown to hold no value that the equation allows: the field is held to the region, `propagate` runs, and the region is narrowed to what the equation gives, again and again. If propagation finds the model inconsistent, or the region becomes empty, it is shown; if it stops getting narrower, or does not settle, it is not, and nothing is done.
5. Otherwise the field has the claim as its range, as if the board had stated it, and propagation goes on. The equation's check then holds the claim to what it gives, so a claim that is not a fixed point cannot pass; step 3 has already shown it is one.

The solver says so in a note (`Solution::seeded`, `netsmith build` prints `note: ...`): the field, what it started from, the range, and the part's range. A board states a range on the field with `&=` to replace it. A range the board states on any field of the circle is used as before and nothing is started.

What this does and does not claim. It claims that every value of the field that satisfies the circuit's equations, with the part inside its stated range, is in the range found. It does not claim anything about states below the part's range: for a constant-power load there is always one near 0 V, and the equations cannot be solved without ruling it out. The part's range is where that is done. A regulator that states `.vin.voltage >= 3V` and `<= 65V` is a part that works between 3 V and 65 V and is switched off below, and a state in which the input is 0.2 V is the converter off, which the input-current equation (written for a converter that is on) does not describe.

## Alternatives rejected

Narrowing a stated range onto the overlap with the equation (the first idea of the task). It makes the answer tight and is not sound for a stated range, which is a claim: a board that states `7V..16V` where the circuit gives `5.9V..16V` would pass.

Starting from the whole part's range and iterating down (a descending pass). That is what a hand seed of `3V..65V` was, and it stops where `propagate` stops, which is the loose answer; widening the narrowing rule to finish it is the unsound idea above.

Starting from the supply's range and accepting the first range the equation maps into itself, with no check of the rest. It is what a hand seed of the right range does, and it passes checks on the strength of a range that the circle does not guarantee, if a second working point is inside the part's range. The checks of steps 2 and 4 are what keep it sound within the part's range.

Excluding the region without the part's range (the whole real line). It cannot be done for a constant-power load: the second working point near 0 V is always there.

Refusing to start a circle at all, and keeping the hand seed. It leaves the checks of a regulator board unevaluated until a person writes a tight range by hand and verifies it, which is the problem.

Doing it for a range the board stated. A stated range is a claim and not a start, and the solver does not replace it (above).

## Cost, limits, left open

A circle costs a few dozen copies of the model and propagation passes when it is started, and a handful when it is refused (a refusal usually comes on the first step). A board with no circle pays one scan of its relations. The model is copied for each step, which is slow for a very large board, and the circle is looked at twice.

The cut is the first field of the circle (in field order) that has an equation and passes the steps. Another cut gives another claim, and every claim accepted has been shown to hold, so the answers are all sound; the first found is used so that the same board always gives the same note.

The start is a guess about where to look and the checks are what make it sound, so a circle that has no value of its own kind to start from (a current circle with no stated current) is not started even when the equations could be. The guide's example, a circle with no range on the load, is such a board and keeps its note.

The part's range is read from the relations on the field, so a part that states its limits the other way (a check on an expression, `.vin.voltage - .vout.voltage >= 1V`) has no range for this purpose. A relation like that is not a bound on the field, and nothing is started on its account.

A design that must also hold in the states the part's range leaves out (a supply that sags below the regulator's minimum and the regulator turning off and on again) is not checked by this. That is for a relation the design writes.

## What to look at

- The assumption above: the part's own `>=` and `<=` on a field say where its equations apply, and a state outside is not a state of the circuit for the checks. This is the one thing the owner may want decided differently; the alternative is to refuse more circles.
- That a value starts only from a stated range of the same kind (the supply's voltage for a voltage), and that `Solution::seeded` and the note are the whole interface: no new syntax, no new member.
- The note's wording in `crates/netsmith/src/lib.rs` (`started_note`) and the changed wording of the circle note, which now mentions the part's range.
- The guide's new section, "A circle the solver starts from the supply", and the claim there that a stated range wider than the first pass can narrow stays loose.
- `crates/netsmith-solve/src/solve/seed.rs`, for step 4: it is the part that decides soundness.
