---
status: stage 0, stage 1 and stage 2 implemented on 2026-11-06, and section 9 (three solver bugs found by a review, with their fixes) on the same day, with the answers to the open questions as recommended (section 8 says what was built and where it differs from sections 1 to 7; the owner decided to replace `required` by the findings and to drop it, and the rest was written by an agent, each choice with the alternatives it rejected). The status before: proposed for owner review (the owner decided on 2026-11-06 to replace `required` by a finding about unconstrained picks and to drop `required`; the definition of "constrained", the transfer of bounds, the second finding for values that are not picks, the severity and the staging were written by an agent, each with the alternatives it rejected)
---

# An unconstrained pick is a finding, and `required` goes

## Summary

`required` (ADR 0069) was meant to stop a board from leaving a value to the solver's first row. A probe of 2026-11-06 showed two things.

1. A bound on a required member satisfied `required` and then did nothing: with `.lamp.balance_resistance >= 20kohm` the build picked two 330 ohm resistors and printed only `note: not evaluated: lamp.top.resistance &= lamp.balance_resistance`. That is a bug in the solver, not in `required`, and it is fixed first (stage 0, section 1): a bound on a value that a part's field is held to by `&=` or `==` now reaches the pick, and where the part is not a pick the check afterwards fails with the numbers.
2. `required` is the wrong tool for the thing it guards. It is opt-in per member, so a plain `.r: Resistor` in a board is not covered at all. It is a test on text, so `>= 0ohm` passes it. And it fires on members that did not need to be stated (a divider whose value the solver could choose from a threshold).

The owner decided: delete `required` and put a finding in its place. The solver already knows, at the moment it picks a part, whether anything limits the pick. When nothing does, it says so.

- **Stage 1a: an unconstrained pick is a guideline finding.** A resistor, capacitor or inductor whose value nothing limits is reported with the part, the row the solver took, and what to write. A warning by default, an error with `--strict` or `guidelines = "error"`, as for open circles (ADR 0057). A board says "any" on purpose with a wide range (`.r.resistance &= 10kohm..100kohm`).
- **Stage 1b: an unstated value is a guideline finding.** A value declared in a module or board that a design's check reads and that nothing states or works out is reported with the checks that wait for it. This replaces `required` for values that are not picks (a voltage a board states for a check).
- **Stage 2: `required` after a type is removed.** Pin rows keep their `required`; it is another word with another meaning.

Recommendation: all three, in this order, and nothing else. The finding is also what atopile's model implies (section 6).

## The evidence, with invented parts

```nsm-fragment
module Lamp {
    .balance_resistance: Resistance required
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    // wiring and the draw of each resistor omitted
}
```

- A board that stated nothing: `required` refused it. Good.
- A board that stated `.lamp.balance_resistance >= 20kohm`: `required` was satisfied (a bound counts as stated, ADR 0069) and the build picked 330 ohm and 330 ohm.
- Before `required`, a board that stated nothing built with 330 ohm resistors that each drew 8 mA, and nothing was said.

## 1. A stated bound reaches the pick (stage 0)

### The mechanism

Reading `crates/netsmith-solve/src/solve.rs`:

1. A bound is a relation and never a domain. `Model::relations` holds `lamp.balance_resistance >= 20kohm` as a relation (`netsmith-check/src/lower.rs`, `relation_statement`). Only `&=` with a constant narrows a domain at once (`narrow_statement`, `apply_checked`). `propagate` evaluates only `==` and `&=` that have a field alone on the left. So the member stays `Unset`.
2. The link `top.resistance &= .balance_resistance` carries a value in one direction only: from the member, on the right, into the part's field, on the left. When the member has a stated range, `propagate` sets the part's field from it and the pick honours it. When the member has only a bound, nothing flows, and nothing flows back from the part to the member.
3. `try_candidate` checks, for a candidate, the relations that name a field inside the part, and the relations that name a field that the candidate's values changed, if no part still to be chosen feeds them (ADR 0057). The bound names only the member. The member is not inside the part, and it does not change, because nothing sets it. So the bound is never judged against a candidate.
4. `Coupling::feeding` (the parts a relation's value waits for) follows `from[out] = the fields the right side of an equation reads`. From the member there is no edge to the part: the edge goes the other way, from the part's field to the member. So the bound has no feeding part either.
5. After the picks, `solve_model` verifies every relation. The member is still `Unset`, so `verify` returns `Unknown` for the bound and for the `&=`, and both go into `deferred`, which is printed as `not evaluated`. The probe printed the `&=` line; the bound should be listed the same way (I expect a second line from reading the code and did not run it).

ADR 0057's case works because the equation is written the other way round: with `.member == .part.field` the member is on the left, the pick sets the member, the member changes, and the bound is judged (`try_candidate`, the `changed` set). ADR 0069's remark that "the member follows" the pick holds for that direction and for `~` joins only. It does not hold for `part.field &= member`, which is how `Lamp` and several downstream modules write it. The two spellings mean the same to a reader and differ in the solver. That is the root cause.

### The fix

A bound on a field `g` that another field `x` is held to by `x &= g` or `x == g` (with `g` the whole right side) is a bound on `x` too. If `x &= g` (or `x == g`) and the design states `g >= e`, then `x` lies in the set of `g`, so `x >= e`. If it states `g <= e`, then `x <= e`. The rule repeats along a chain (`x &= g`, `g &= h`, a bound on `h`).

The solver adds these as relations of its own, once, before anything is propagated, and takes them out of the model it returns. They are judged where every relation is judged:

- at a candidate, because they name a field of the part (`try_candidate`): a row that violates the bound is refused where it is picked;
- after the picks, because they are verified like the others: where the part is not a pick (a stated `330ohm +/- 1%`), the build fails with the numbers;
- they are never listed as `not evaluated` (they are not the design's lines), and when the part's field has no value they say nothing, since the bound that the design wrote is already listed.

Soundness. The rule adds a refusal and never a value: no domain, no range and no pick changes except that a candidate or a state that violates a bound the design wrote is refused. It cannot refuse a design that satisfies what the design stated, because `x &= g` means that `x` is inside `g`, and a bound on all of `g` is a bound on every part of it. It does not claim anything about `g` itself. This is the hard line of ADR 0053 and ADR 0057: a pick is never accepted for want of a check, only refused earlier, and a check is never made against a range nobody wrote.

A build that passed before can fail now. It is exactly a build that violated a stated bound and said nothing.

The message names the part, the member, and both lines (invented):

```text
error: `fixed.r.resistance` must always be at least 20kohm, but 326.7Ω..333.3Ω is not always above 20kΩ: it is held to `fixed.value` (`.r.resistance &= .value`, main.nsm:9), and `.fixed.value >= 20kohm` (main.nsm:15) bounds it
```

When no candidate meets a bound the search says what it says for any bound: `no resistor candidate satisfies lamp.top: the closest of the 214 that fit, 20kΩ 1% 0603, fails because lamp.top.resistance must always be at least 20kohm, but ... is not always above ...` followed by the same explanation of the link.

Tolerance counts, as for a stated part: `20kohm +/- 1%` is not always at least 20 kohm.

### Rejected for stage 0

- **A value flows back from the part to the member** (`x &= g` sets an unset `g` from `x`, as `~` does). The member would then follow the pick, the bound would be judged by the existing `changed` rule, and ADR 0070's trouble with `==` chains would go away. It is not sound as a general rule: with `top.resistance &= .m` and `bottom.resistance &= .m` and `.m` unset, the first pick would set `.m` to a point value and force the second part to the same row. That is a new requirement nobody wrote, and which part is picked first would decide it. Where `.m` is stated, the rule does nothing. A symmetric `==` between two bare fields could be treated as a join; that is a separate decision about `==` and is left open.
- **A bound narrows the field's domain** (`.m >= 20kohm` becomes `20kohm..`). It changes every domain that a bound reaches, the starts of ADR 0053 read bounds as the part's range, and a half-open range has no value in an expression. Too wide for a fix.
- **Only the check afterwards.** It would fail the build with the numbers, but the solver would have chosen the 330 ohm row when a 20 kohm row existed. A pick that a stated bound could have steered is a worse build than one that is refused.
- **Transfer through `~` joins and through arithmetic** (`x &= g * 2`). Left open; a join is not written for values of parts, and arithmetic needs monotonic reasoning.

## 2. An unconstrained pick (stage 1a)

### What is reported

A part of a generic with a value column that the solver walks, picked while nothing limited that value. The first list is the three E-series generics (`std::parts::passive::Resistor`, `Capacitor`, `Inductor`); the columns are in one list in `table.rs`, next to `OPTIONAL` (ADR 0058):

| catalog | value columns |
| --- | --- |
| `resistor` | `resistance` |
| `capacitor` | `capacitance`, `effective_capacitance` |
| `inductor` | `inductance` |

Other catalogs (a Zener's voltage, a crystal's frequency, a regulator's output) are rows that a design names or states; they are added to the list when their use shows the same failure.

### What counts as constrained

Just before the pick, after the first propagation and the start of circles (ADR 0053), a pick of one of these parts is **limited** when at least one of these holds for its value columns:

1. A value column has a domain: a stated value, a range or a tolerance, a `?=` default that applied, a narrowing that `propagate` carried in from a member that has one, a `~` join to a field that has one. Any range counts, however wide.
2. A bound (`>=` or `<=`) has a value column alone on one side, whether the design wrote it or the transfer of section 1 made it.
3. A check that the pick decides: a relation, not written inside the part's own definition (a part's own `drop * current <= max_power` does not count), that names a field worked out from a value column (the fields the equations, joins and balances carry the pick's value to, the traversal that `Coupling::feeding` already makes), and that is a check: a bound; or an equation or `&=` whose left side is a field that has a stated range before the pick (`.led.current &= 5mA..15mA` with `.led.current == (V - Vf) / .r.resistance`).
4. A budget: a stated current on a net that a field worked out from the pick is a term of (ADR 0038).

And the check must have been judged: of the relations of 2 to 4, at least one is evaluated (passes or fails) once the pick is made. A bound that waits on a value nothing sets (the open circle of ADR 0057) judges nothing, and a pick that only such a bound reaches is reported as unconstrained, unless an open circle finding already names that check. One finding for one cause.

What does not count: a bound on the part's rating (`max_power >= 0.25W`) or package; the part's own relations; the part's `catalog` rows being fewer than all (a filter on the package does not limit the value). A stated range `0ohm..10Mohm` counts: it says "any" on purpose, and the finding cannot tell a wide range from a narrow one, as `required` could not tell `>= 0ohm`. This is how a board says "any".

### Where it is found

In `pick_parts`, where the jobs and the `Coupling` exist, before the groups are searched, for each job whose catalog has a value column: the checks 1 to 4 are collected syntactically, and after the group is solved the checks of 2 to 4 are verified once more (`Solver::verify`). A job with no judged check becomes an `UnconstrainedPick { part, column, row, held_by }` in `Solution::unconstrained`, where `row` is the label of the pick and `held_by` is the member that the value is held to by a link of section 1 when that member has no value. `crates/netsmith/src/lib.rs` turns the list into findings as `open_circle_findings` does, for a board or for one board of a system. Two parts that hang from one unstated member are one finding that names both and the member.

The cost is one scan of the relations per part with a value column, and one `verify` of a few relations after the group is solved.

### Severity

A guideline, as ADR 0057 decided for the open circle: a `warning:` by default, an error with `--strict` or `guidelines = "error"`. The design may be right (a pull-up of any value is sometimes right, and a board in progress should build), and the finding is exactly what was missing when the 330 ohm resistors were taken without a word. An error by default would refuse boards that build today. A note would be what failed. A project that wants the strict reading has the switch it already has.

### Saying "any" on purpose

The language accepts a range on a quantity (`3V..30V`, `1.98kohm..2.02kohm` are in the guide). A board writes:

```nsm-fragment
.lamp.balance_resistance &= 10kohm..100kohm
```

and the finding is silent for the two resistors: the member has a domain, the domain is carried to both (rule 1), and the solver takes the first row of the catalog inside it. This is atopile's spelling (section 6). No keyword `any` is added: it would be a second way to say a range, and a language-design choice is the owner's.

### Complete examples

The board below states nothing for a resistor of its own and for the module's member (invented parts):

```nsm-fragment
module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Plain {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .pull: Resistor
    .supply.pos ~> .pull ~> .supply.neg
}
```

```text
Plain: unconstrained pick guideline: `lamp.top` and `lamp.bottom` (Resistor) were chosen with nothing limiting their resistance, so the solver took the first row of the catalog, 330Ω 1% 0603 C23138, for each; both are held to `lamp.balance_resistance` (`&=` at src/lamp.nsm:6 and 7), which nothing states: state it, `.lamp.balance_resistance &= 4.7kohm +/- 5%`, or say that any value will do, `.lamp.balance_resistance &= 10kohm..100kohm`
Plain: unconstrained pick guideline: `pull` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row of the catalog, 330Ω 1% 0603 C23138: state it, `.pull.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.pull.resistance &= 10kohm..100kohm`
```

With `--strict` the same text starts `error:` and the build stops.

A board that bounds what the resistors decide is silent, because rule 3 holds and the check is judged:

```nsm-fragment
board Bounded {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .lamp.balance_resistance >= 20kohm
}
```

With stage 0, the pick takes the first row of the catalog that meets the bound, tolerance included, for both resistors, and no finding is made: rule 2 reached the part through the transfer.

When the pick is limited only by a check that cannot be made, the open circle finding speaks and this one does not:

```text
Behind: open circle guideline: these values depend on each other and none has a value: ... so the checks `divider.top.current <= divider.max_draw`, ... are not evaluated, and the parts they would judge are chosen without them; ...
```

### How many picks of the existing material it would flag

Counted by reading text (not by running the solver, which this ADR did not do), with a script over the files and a count of `.name: Resistor`, `Capacitor` and `Inductor` declarations whose value (`name.resistance`, `name.capacitance`, `name.effective_capacitance`, `name.inductance`) is never written in the same block or file, or inside the part's braces. A mention is not a limit (a relation may only read the value), so the script gives a lower bound; the second column checks by hand.

| source | declarations | no mention | judged by hand |
| --- | --- | --- | --- |
| `examples/*.nsm` and the two example projects | 59 | 0 | 0 (the LED resistors are limited by a stated LED current through an equation, rule 3; every other value is stated or bounded) |
| `std/**/*.nsm` (`BuckStage`, `FeedbackDivider`, `PullUp`, `I2cPullups`) | 8 | 0 | 0 when the module's own members are stated; a board that forgets them is exactly the case |
| the `nsm` blocks of `docs/LANGUAGE.md` | 84 | 21 | 0 of the blocks that reach the solver: the 21 are in 7 blocks that are only checked (`nsm`) and in 14 blocks marked `nsm fails` that stop at the checker |
| the tests that solve (`crates/netsmith`, `netsmith-solve`, `netsmith-lower`, `netsmith-pcb`, `netsmith-editor`, counted by test function) | 151 | 66 | all 66 are parts that a test about placement, lowering, notes or a message does not give a value; each would get a warning |

So no example and no block of the guide is flagged, and about 66 declarations in tests are. Tests that only look at the picks, the notes or the lowered board are not affected by an added warning (the findings are made in `crates/netsmith/src/lib.rs`, after lowering). The tests that assert that there is no guideline (`crates/netsmith/tests/guidelines.rs` line 106, `open_circle.rs` lines 128 and 137) read boards that state their values; I expect at most five tests to need a value line, and the number is what stage 1's gate reports. A finding that fired on half the examples would be wrong; this one fires on none of them, and the gate of stage 1 is that every example and every guide block builds with no new line.

## 3. Values that are not picks (stage 1b)

An enum, a text and a voltage that a board states for a check (the processor's high level on a line) are not picks. Nothing walks a catalog for them. A relation that reads one that nothing states is today `not evaluated` in the notes, and `required` was the only thing that made it an error. ADR 0057 rejected a finding for every relation that is not evaluated, since data missing in a part (ADR 0046) puts such notes on every board. The finding is small if it names only the **root**: a value that has no source.

A field is reported when all of these hold:

- it is declared in a module or a board itself (`.dtr_high: Voltage`), not in a part, not inside an interface, not a column. A part's unset datasheet value is the library's gap (ADR 0046); an interface's optional values are offered open on purpose (ADR 0058: the six notes of the buck stage); a rail's voltage is stated or joined by the board;
- it has no value after the solve;
- it is not the left side of an equation or `&=` that works it out from other fields, and no join, balance or column sets it. A member that a wrapper forwards to (`.inner.dtr_high == .dtr_high`) is not a root: the wrapper's member is, and it is the one that is named, which is the one the board can state;
- at least one relation that the design wrote, or a library module wrote for its user, and that reads it (directly or through values worked out from it) is among the relations not evaluated.

A member that no relation reads is not reported, so a member of a module that is only passed along costs nothing. A text or enum that only offers a choice (`.j.package &= .package` left alone, ADR 0042) is not a relation that fails to evaluate and is not reported.

Severity as for the pick: `warning:` by default, an error with `--strict`. The rule name is `unstated value`. A member that is both a root and feeds an unconstrained pick is named once, by the pick finding.

```nsm-fragment
module Receiver {
    .line: Pin
    .vih: Voltage
    .vil: Voltage
    .driver_high: Voltage
    .driver_high >= .vih
    .driver_high <= 5.5V
    .vil &= 0.3V
}

board Probe {
    .receiver: Receiver
}
```

```text
Probe: unstated value guideline: `receiver.vih` and `receiver.driver_high` are declared by `Receiver` and nothing states or works them out, so the checks `receiver.driver_high >= receiver.vih` and `receiver.driver_high <= 5.5V` are not evaluated; state them, for example `.receiver.driver_high &= 3.3V +/- 5%`
```

Rejected: reporting every relation that is not evaluated (ADR 0057's reason). Rejected: marking members, which is `required`. Rejected: an error always: a board in progress states its values one at a time.

What it costs against `required`: a member that nothing reads goes unreported. That is not a failure: a value nothing checks and nothing picks from changes nothing.

## 4. `required` goes (stage 2)

The owner's decision. What goes:

- `netsmith-lang`: `Declaration.required` in the syntax tree (`ast.rs`), the word after a declaration's type in the parser (`parser.rs`, about 15 lines) and the printer (3 lines). The word stays a plain word elsewhere, so `.required &= 1` still parses, and a `pins` row keeps `required` and `optional` (ADR 0016).
- `netsmith-check`: `required.rs` (318 lines), the hooks in `checker.rs` (the list of required members, `note_required`, `check_required_marker`, the call after the nets are made), the refusal of `?=` against `required` in `lower.rs`, and `stated_const`. `stated`, which `?=` needs (ADR 0051), stays. The error `... is a required member of ...` and the one for `?=` with `required` no longer exist.
- the guide: the section "Values the design must state" (95 lines, 4 blocks), two lines of the table of the standard library, and a new section for the two findings; the glossary term **Required member**, replaced by **Unconstrained pick** and **Unstated value**; one line in each of three skills.
- the editor's highlighter keeps `required` for pin rows. Its source does not change and the committed bundle is not rebuilt.
- ADR 0069 is marked superseded by this ADR. ADR 0070 and ADR 0063 get a line each: `PullUp.resistance` and `I2cPullups.resistance` are not `required` any more.

What happens to each use that exists:

| use | after the removal |
| --- | --- |
| `std::bus::PullUp.resistance` (ADR 0070) | The word goes. A board that forgets the resistance gets the unconstrained pick finding for the pull-up's resistor, with the member named. A board that states a current budget on the supply gets a limit, since a budget is rule 4. The doc of the module says that the resistance should be stated. |
| `std::bus::I2cPullups.resistance` (ADR 0063) | The same, for the two resistors. |
| a member of a module that feeds a part's value (`.balance_resistance &= ...` into a resistor, a decoupling capacitance) | Covered by stage 1a, once stage 0 is in (the bound reaches the pick). The finding names the part and the member. |
| a member that a check reads (a high level, a drop, a threshold) | Covered by stage 1b. |
| a member of a module that wraps another and forwards by `==` | The wrapper's member is the root and the one named. |
| an enum or text member read by nothing | Not reported. Nothing depends on it. |
| `required` on a generic's member | Same as a module's. No generic in the library has one. |

What gets weaker: a module author can no longer force the board to state a value that the author knows no check reads. The remedy is to write the check, which is what the value was for.

What gets better: a plain `.r: Resistor` in a board is covered; a bound that the pick honours is not an error; a divider whose value could be left to a threshold is no longer refused for want of a number.

A downstream project that wrote `required` has to delete the word. Removing it first is accepted by the current parser, so a project can do it before stage 2 lands. After stage 2, the word after a type is a parse error with the parser's ordinary message; no special message is kept, because a message for a removed word is another spelling to carry (one correct way).

## 5. Messages

1. `unconstrained pick` for a part with no limit, for a member that is held to a part (section 2, first example) and for a part by itself (second line).
2. `unstated value`, as in section 3.
3. The stage 0 failure after the picks, section 1, `fixed.r.resistance must always be at least 20kohm...`
4. The search failure at a pick, section 1: `no resistor candidate satisfies lamp.top: the closest ... fails because lamp.top.resistance must always be at least 20kohm ..., and it is held to lamp.balance_resistance by ..., which .lamp.balance_resistance >= 20kohm bounds`.
5. With `--strict`: `error: Plain: unconstrained pick guideline: ...` and the build stops. A board states `.pull.resistance &= 10kohm..100kohm` and builds without a line.

## 6. How it relates to the rest

- **atopile.** A parameter there is the set of values it may still take, and `&=` intersects and fails when the set is empty. A wide set lets the picker choose (`10kohm..100kohm`), a bound is honoured by the picker, a condition must be decided before the solve or the build stops and names it, and a library's design check runs after instantiation or after the solve. The guide says nothing about reporting a part whose set is the whole domain; it has no `required`. In that model the bug of section 1 does not exist, because a bound is a set and the sets of `x` and of `g` meet by intersection. Stage 0 gives netsmith's `&=` that behaviour for bounds, without changing its domains. The finding is the netsmith counterpart of atopile's check stage: it runs after propagation, on the structure, and names the thing. It is closer to a post-instantiation design check than to a type.
- **`?=` (ADR 0051).** A default is a statement of the library, so a field with a default that applied is limited (rule 1): a module that offers `.balance_resistance ?= 4.7kohm +/- 5%` never gets the finding unless the board's own statement removes the default and states nothing. `?=` is how a library makes `required` unnecessary.
- **The open circle (ADR 0057).** The same kind of finding (a guideline, a warning by default, an error under strict) for the same kind of fault (a pick made without what the design meant to say), found after propagation. The pick finding does not repeat a check that an open circle names.
- **`generic` and `provider` (ADR 0068).** The choice of a provider is a choice among parts and is not what is reported. A part chosen for a generic is a pick like any other, and its value columns are judged as above. A generic's own values that a use states are statements, and rule 1 reads them.
- **Soundness (ADR 0053).** Nothing here changes a value or a range. Stage 0 adds a refusal that the design's own bound entails. The findings only report.

## 7. Staged plan

Stage 0, the fix of section 1. About 120 lines in `solve.rs` and 10 in `netsmith-check/src/model.rs` (a way to add relations and to cut them off again), a new test file of about 8 tests with invented catalogs, a paragraph and one failing block in the guide. By itself it makes a bound on a member that feeds a part honoured at the pick, or refused with the numbers.

Stage 1, the findings. 1a: `Solution::unconstrained`, the list of value columns in `table.rs`, the finding in `netsmith/src/lib.rs` for a board and for a system's board, about 220 lines in all and 12 tests. 1b: the roots of the relations not evaluated, about 110 lines and 6 tests. The guide section, the glossary terms, three skill lines. The gate: every example and every block of the guide builds with no new line, and the number of tests that need a value line is listed.

Stage 2, removal of `required` with the edits to every use, with every example byte-identical, since no example uses `required` (`grep` finds the word only in the two members of `std/bus.nsm` and in pin rows): compare the output of `netsmith build` for every file of `examples/` and the two projects before and after. About 480 lines deleted from `netsmith-check`, 40 from `netsmith-lang`, 520 lines of tests (the 23 tests of `crates/netsmith-check/tests/required.rs`, the 4 of `crates/netsmith/tests/required.rs`, 2 in `netsmith-lang/tests/parse.rs`, and 3 of the 10 in `pullup.rs` rewritten to the finding), 95 lines of guide.

## 8. What was built, and where it differs

All three stages are in, on 2026-11-06. The answers to the eight open questions are the recommended ones.

**Stage 0** (`crates/netsmith-solve/src/solve/transfer.rs`, `Model::push_relation`, `Model::truncate_relations`). The solver works the transferred bounds out once, and keeps them out of the model except while it picks parts and checks them afterwards. Where it differs from section 1:

- It works them out just before the picks, after the first propagation and the start of circles (ADR 0053), not before anything is propagated. A member that has a value by then is judged on it, as before. Before the picks it had added `stage.input_cap.bias >= ...` to an example, because the member had no value at that moment and got one a step later, and the extra relation joined the group of two capacitors with the regulator. That example would then have failed (see the last bullet).
- A bound on a member that has a value, a link `x == g` whose left side has a value of its own, and a field that is a position (`.place`) are left alone. A chain goes on only through a field that has no value. An `==` puts the right side inside what the left side allows, so a range on the left says nothing about the right; a position's bounds are lowering's.
- A candidate is judged by the added bound only when no part still to be chosen feeds the right side of the bound (the rule of ADR 0057 for a value that a candidate changed), as for every other relation that names a value other parts decide; the check after the picks judges it whatever order the parts came in.
- The message is `X must always be at least E, but V is not always above E: it is held to `g` (`.x &= .g`, F:N), and `.g >= E` (F:N) bounds it`. `E` is the quantity as the checker prints it (`20kΩ`), as in every message of a bound. For a chain it says `, which is held to` for each link. The lines come from a snapshot of the source files that every model keeps (`Model::place_of_statement`, `Model::statement_text`).
- When no row meets the bound each part with no row has its own message (`no resistor candidate satisfies lamp.top: ...` and one for `lamp.bottom`), because they are separate parts that nothing joins in one search.
- A part with a stated value that a catalog row stands for is picked too, so the bound refuses it at the pick and the check after the picks says what the stated value is: two messages, as a bound written on the part itself gives. A part with no catalog gets the message of section 1 alone.
- Found while building it: `hopeless` (ADR 0057) skipped a row that the design could not tell from a row it had refused, comparing only the columns that another field reads. A bound that names one column alone (`.a.effective_capacitance >= 1uF`) reads no other field, so a 2 uF row looked like its 0.5 uF twin of the same rating, and a part in a group of two was called hopeless though a row meets the bound. It also failed without any transfer. The refused row now carries the columns of the relation that refused it, and a row is skipped only when it equals the refused one there too. A group made by an added bound is the first design that reached it in the examples, so it showed.

**Stage 1** (`crates/netsmith-solve/src/solve/findings.rs`, `table.rs`, `crates/netsmith/src/values.rs`). `Solution::unconstrained` and `Solution::unstated`, and the findings `unconstrained pick` and `unstated value`, as sections 2 and 3 decide: a warning, an error under `--strict` or `guidelines = "error"`, the E-series generics' value columns only, rating bounds not counted, a wide range counted as constrained, no keyword. Where it differs:

- A pick is judged on the solved model, not right after its group: a limit counts if it passes or fails once everything is picked and propagated. A bound that waits for a value that another pick sets is therefore judged, and limits the pick it concerns.
- A relation written inside the definition of any part (a diode's `current >= 0mA` as much as a resistor's own power check) is the library's and limits nothing; the section named only the part's own. Otherwise an LED's own bound would limit every resistor next to it.
- A budget (rule 4) counts when it is judged. A budget on a net that an open circle runs through is named by that circle, as a check is.
- The message says the solver took "the first row that fits", where the section said "the first row of the catalog": a part with a stated package gets the first row of that package. Two parts held to one member are one finding in declaration order, with the lines of their links.
- `UnstatedValue` holds the fields that the same checks wait for (one finding, as in the example of section 3) and not one field. The checks that count are the bounds and the equations whose left side is not a value still to be worked out; a definition that waits (`.x == .y * 2`, `.part.field &= .member`) is not one. A bound on a member that reached a part and was judged there is how the board states the member, so it is not a check that waits: that is why a board that bounds `.lamp.balance_resistance` and states nothing more gets no line. A value that an open circle finding asks to be stated is left to it.
- Gate: every `examples/*.nsm` in both modes and the two example projects build with output byte-identical to the snapshot before the change, and every solving block of the guide builds with neither finding (the guide's test now fails for a block that has one and is not marked `warns`).
- No test needed a line that states a value. One needed the finding to leave a pick whose only limits are a budget and a check inside an open circle to that circle (`nothing_else_on_the_net_leaves_an_ordinary_circle`), and that is how the finding works now.

**Stage 2.** `required` after a type is gone: `Declaration.required` (`ast.rs`), the parser, the printer, `crates/netsmith-check/src/required.rs` and its hooks in `checker.rs` and `lower.rs` (`stated_const`, `refuse_default_on_required`), the 23 tests of `crates/netsmith-check/tests/required.rs`, the 4 of `crates/netsmith/tests/required.rs`, the 2 of `crates/netsmith-lang/tests/parse.rs` (one test says that the word after a type is the parser's ordinary error), `required` on `std::bus::PullUp.resistance` (two tests of `pullup.rs` became tests of the finding, a third lost the word), the guide section, the glossary term, the skills' lines. `stated`, which `?=` needs, stays. A pin row keeps `required`. The editor's source changed in a comment and a test only (the page is the same bytes, its hash is new). ADR 0069 is marked superseded, ADR 0070 and ADR 0063 have their line.

**Left open.** The transfer through `~` joins and through arithmetic, and a bound on a column other than the three E-series generics' value columns, as sections 1 and 2 decided. Another catalog joins `VALUE_COLUMNS` when its use shows the same failure. The downstream project still has the word to delete; until it does, its build stops at the parser.

## 9. Three bugs found on a real board (2026-11-06), and how they were fixed

A probe of a real board with invented reproductions (two small files of invented modules, kept with the probe notes and not in the repository) found three faults in what sections 1 to 8 built and in ADR 0057. The owner decided that all three are fixed, each with tests that fail without the fix. The choices below are the agent's (LC1), for review.

### 9.1 A window on a value that two equations state did not reach the pick

The mechanism. `.osc.fsw` is the left side of two equations: the module's `.fsw == .fsw_nominal * .fsw_tolerance` and the timer's `.osc.fsw == .fsw`, with `.timer.fsw &= 0.9MHz..1.1MHz` stated by the board. `propagate` gives an unset field the range of the first equation that can be worked out. With a range on the member `.rt_resistance` the computed equation can be worked out before any pick (from the whole range, 182.8 kHz to 3.289 MHz), so `.osc.fsw` took that range; without the range it could not, and the link took the window. Then:

- the field is not the sole left side of an equation, so `propagate` never narrowed it to what a pick makes the equation say (stale range);
- `verify` of `.osc.fsw == .fsw` asks that the field's range enclose the right side, and a stale wide range encloses the window whatever the pick is; `verify` of the computed equation asks the same of a range that the pick made narrower and passes;
- `try_candidate` judges a relation only if it names a field the candidate changed, and the field did not change.

So the window was never compared with the pick. It was order dependent, against the design rule that statement order never matters: the same board without the one `rt_resistance` range line judged the window. ADR 0057 says a stated window on a frequency takes part in the pick; this is the case it missed.

The fix (two places in `solve.rs`, about 90 lines). A field with several equations, no statement that narrowed it, and a value that a pick can change (`Solver::fed`, every field worked out from a catalog column by an equation, a join or a balance) is *what the equations say*, not an allowance:

- `propagate` lets it follow an equation whose range lies inside it, as a sole equation does (`several_equations`, sound for the same reason: the value is each equation's value);
- `verify` judges each of its equations against the others, not against the field's range: an equation whose right side no pick changes (the window, reached through `.fsw`) is an allowance, and what the other equations work out must lie inside it; with no allowance among them the equations only have to share a value. A stated range on the field itself (`narrowed_at`) keeps the rule of before.

Messages (the guide has the exact text). A pick outside the window is refused where it is picked, with `` `timer.osc.fsw` must stay within 900kHz..1.1MHz, but (timer.osc.fsw_nominal * timer.osc.fsw_tolerance) works out to 2.113MHz..2.494MHz ``; a stated resistor outside it fails the build with the same line; when no row meets it the search says `no resistor candidate satisfies ...: the closest ... fails because ...` as for any bound. A check that cannot be evaluated is still named as before (the relations not evaluated are untouched).

Not done: an equation on a value that two equations state, when no pick changes any of them, keeps the old rule. A window that only the whole domain of a member can meet is a window the member's range must allow.

Rejected. Narrowing the field to the intersection of its equations everywhere: the report would name a range nobody wrote and partly overlapping ranges would pass. Treating `x == y` between two fields as a join (the open question 6): a separate decision about `==`. Only checking afterwards: the 15 kohm row would have been picked, then refused, when a row inside the window existed.

### 9.2 Two gaps in the unconstrained pick finding

(a) A load that the pick does not feed counted as a limit on the pick. Rule 4 said "a current stated on a net that a value worked out from the pick is a term of". The traversal that finds what the pick decides followed every balance both ways, so a pull-up's current reached a load's stated current through two balances although the solver never sets a part's current from a net of three terms. The sound rule: **a budget limits a pick only if the pick's value changes whether the budget holds**, that is, a term of the net other than the stated one is worked out from the pick. Two edits in `findings.rs`: a balance carries a value to a term only where the solver could set that term from the others (`solve_balance_but`: not to a stated current, which is a budget and is judged and never set, and not to a part's own current on a net of more than two live terms or an incomplete balance), and a budget counts only when such a term is among what the pick reaches. The traversal is also used by rule 3, where it is now exact in the same way. No existing example, guide block or test changed because of it.

(b) A ratio is not a size. The bounds `.out >= 1.5V` and `.out <= 1.8V` of a divider are judged and pass, so each resistor was "limited"; they hold for 330 ohm and for 330 kohm. The sound rule: a check limits the size of a pick only if scaling the unlimited picks together changes it. `SizeBlind` in `findings.rs` scales the value fields of the unlimited picks by one factor and expands each check through the equations that define what it reads (exactly one `==`, no join, no balance), by degree of homogeneity (`+` and `-` need equal degrees, `*` adds, `/` subtracts, a field no pick feeds has degree 0); a check whose two sides have the same degree is blind to the scaling, and what it cannot expand it does not call blind (the finding is then missed, never made wrongly). It is a fixed point: a pick that a check limits leaves the set, and a check that read it is then not blind to the picks that remain. A pick limited only by blind checks is reported, and says so: `... chosen with nothing limiting their resistance in size, ...; their resistance is limited as a ratio, not in size, by `.out >= 1.5V` and `.out <= 1.8V` (main.nsm:13 and 14), which hold whatever the size is: state one of them, ...`. Parts that only the same checks limit are one finding. A range on one resistor or on the total, or a bound on a current that the divider draws, limits the size and ends it.

Consequence for the standard library: a board that uses `FeedbackDivider` (and so an adjustable regulator or a switching stage) and states the output but nothing that depends on the size of the divider now gets this warning, which says to state `.divider.current <= ...`. No example or example project does (the gate); the guide's `Adjustable` block states `.divider.current <= 1mA` and says why.

Rejected. Treating the finding as made for every check on a value that two picks share: it would drop the finding for the divider and bring back the silence. A message without the checks: the owner could not tell a ratio from a size.

### 9.3 The result depended on the name of an instance

Not reproduced as written. A module with a member `sense` and a board instance `.sense` give the same result as any other name in every invented module tried (a `Pin`, a `Rail`, a `Voltage` member; with and without a draw bound), and no lookup in the checker or the solver keys on path strings in a way that two paths could collide (`Model::lookup`, `resolve_path` and the column keys of `pick_parts` were read). The real board was not available.

What was found instead, and does depend on the name: the solver picks groups of parts one after the other in path order (ADR 0057), and a bound that reads values of two groups waits for the parts that feed it (`Coupling::feeding`). The group that comes first is therefore picked without the bound, and its first fit can leave the second group nothing: a tap divider picked first (330 ohm over 1 Mohm, a voltage of nearly zero) against `.tap.sense_at_cell_max >= 0.75 * .ref.out` that the reference divider then cannot meet. Named `.ref`, the reference sorts first and the board builds; named `.zref`, the tap does and the build fails with a message about the reference's parts (`no combination of candidates satisfies zref.bottom and zref.top together ... closest ...`). In the real board `.sense` and `.supercap_sense` sort differently against the other instances, so the order of groups differs, which is a likely cause of what was seen and not a proven one. `sense.draw_at_cell_max ... 8.25mA..8.418mA` is the nearest miss that the failed search prints (330 ohm over 330 ohm on 5.5 V), not the cause.

The fix (`Solver::search_again_together`, `pick_parts`). The state before each group is kept (`Mark`: the model, the diagnostics, the picks, the unresolved parts, the refused counts, the derived currents). When a group fails and an earlier group shares a relation with it, the solver goes back to before the first such group and searches again with the failed group first, then the linked ones, then the groups between them in their old order, each as before (with the bound now judged by the groups that come after). If every group then has parts, the result replaces the first; otherwise the state is put back as the first failure left it and its message stands. Parts that were found build are never searched again, so a board that built keeps its picks (the identity gate). The result of a failed first attempt equals the result of the order in which it would have succeeded, so a board gets the same parts whatever its instances are called when either order works.

Limits. Only the order "failed group first" is tried, not every order and not a joint search of the parts of both groups (a joint search of the four resistors of two such dividers, each from the 700 rows of the real catalog, had not ended after two minutes in a debug build); a pair that no greedy order serves and a joint choice would still fails with the first message. A board whose picks differ by order in other ways (both orders work, with different first fits) is as before. The retry is made once for the groups involved, and the search it adds is paid only by a board that failed. The state before each group is kept for every board, though: `Mark` clones the model, one clone per group. Measured on the largest example (`examples/enclosure_sensor`, 11 groups) the clones took 1.5 ms of a 1.1 s build, and the build time is the same within noise (1.12 s before, 1.14 s after, median of nine); the cost grows with the groups times the size of the model, and a clone kept only for the groups that share a relation (the only ones a retry goes back to) would remove it if a board ever shows it. Each search has its own budget of trials (`MAX_TRIALS`), so a retry does not take trials from the groups after it.

### 9.4 Two corrections after a review (2026-11-06)

An independent review of sections 9.1 and 9.3 found two places where the code did less than the text above says.

- **A failed retry put back the model and the picks but not the diagnostics and the unresolved parts.** `Mark` keeps the length of those two lists, which cuts a list and cannot bring back what the first restore cut off. When the retry failed at a later group, the message was the retry's (a part of the second attempt), and a part with a pick could be listed as unresolved while the part that failed was not. The solver now keeps both lists as the first failure left them and puts them back with the model. The test `a_retry_that_fails_at_a_later_group_leaves_the_first_failure_whole` has a board whose retry fails at its second group; the earlier test fails at the first group of the retry, where both attempts say the same.
- **The several-equations rule of 9.1 reached fields that no pick changes.** The `verify` half tests that a pick feeds the field, as 9.1 says ("Not done: ... when no pick changes any of them, keeps the old rule"); the half that narrows the field did not, so `.x == .a` and `.x == .b` with `.a &= 1V..3V` and `.b &= 1.5V..2V` was refused, where it had built. Both halves now test it (the first propagation runs before the parts are looked at, so the set of fields a pick can change is also worked out at the start, from every part that has a catalog), and `two_equations_that_no_pick_changes_keep_the_old_rule` builds that board.

### 9.5 A bound that waits for a later group did not reach the pick (2026-11-07)

Found on a real board: a buck stage's quiescent current, `iq + vout * divider_current / (vin * efficiency)`, bounded at 20 uA, with the feedback divider picked at its smallest sizes (5 kohm over 910 ohm, 273 to 805 uA) and the bound refused afterwards. On a small board with the same module the bound reached the pick (1 Mohm over 182 kohm). Cause: `try_candidate` judges a bound that names a value the candidate changed only when no part still to be chosen feeds it (`Coupling::feeding`, rule 3 above), and on the whole board the input voltage of the stage is fed, through the current of the rails, by parts in groups that come after the divider's (the pull-ups of an unrelated flash). The bound waited, so the divider was picked as if it were not there. The retry of 9.3 does not help: it goes back only when a group fails after an earlier one that shares a relation with it, and here nothing fails until the final check.

The fix (`verify_possible`, in `try_candidate`). A bound (`>=` or `<=`) that waits is still not judged as a check, but it refuses a candidate when no value in the ranges as they are can meet it: `lhs <= rhs` with the lowest value of `lhs` above the highest of `rhs`, and the mirror for `>=`. A range only narrows when the parts that are still to be chosen are chosen (a pick takes a value its field already allowed, and propagation intersects), so a bound that holds for no value of the range holds for none of the narrower one, and the refusal never turns away a candidate that the check afterwards would accept. A range that is unknown (a value that nothing has worked out yet) is not judged. The refusal says `... must be at most X, but R is always above X`. Nothing changes for a board where no bound waits, or where every bound that waits can still hold for the candidate: the examples and the example projects build byte for byte as before.

Limit. The bound is judged on the range as it is, so a candidate for which the bound holds for some value of the range and not for all (the range is still wide because the later parts are not chosen) is accepted, and the check afterwards can still refuse the board. The first such candidate in catalog order is taken, not the first for which the bound holds over the whole range. Choosing among them, or searching the later groups first, is a choice of the solver's order that this section leaves as it was. (9.6 makes that choice for the first of the two.)

### 9.6 Two tiers in the pick (2026-11-07)

The limit of 9.5 was a choice of the first candidate in catalog order that a waiting bound could still hold for, not the first for which it holds over the whole range. The owner decided that the pick prefers the second.

The rule. When the candidates of a part are tried, a candidate is accepted in the first tier only if every bound that waits for a part still to be chosen (`verify_possible`, 9.5) also holds for every value of the range as it stands (`verify` of the same relation passes). A candidate that is let through by `verify_possible` and not by `verify` is left for the second tier, which is tried only when the first has nothing that works, and tries those candidates in the same order. Within each tier the order is the catalog's, so the result is deterministic and a catalog that has no such candidate is searched as before. A bound that cannot be judged (a side is unknown) does not put a candidate in the second tier.

Where it applies. `Solver::search`, for each count of a bank and within it: the fewest count still comes first (ADR 0052), the tiers are inside it. A candidate that is deferred is not counted as failed (it is tried again in the second tier, and is counted as a trial again); a candidate that fails deeper in the first tier is not tried again. `try_candidate` reports whether the candidate was whole (`Accepted.whole`). The tiers are not applied to `hopeless`, which only asks whether a candidate can pass alone.

Effect on the soundness of 9.5. None: a candidate is only ever reordered, never turned away because of it, so what the search can find is the same as before and only which solution comes first changes. A board where no bound waits, or where every bound that waits holds over the whole range, picks as before; so does one where every candidate that is let through holds only for some values (the second tier is then the old order). The examples and the two example projects build byte for byte as before.

Tests (`crates/netsmith-solve/tests/pick_tiers.rs`, invented resistors): a row for which the bound holds only for some values and comes first in the catalog is passed over for a later row for which it holds for all (it picks the first such by catalog order); with no such row the first partly-passing row is accepted and the board builds; a bound no value can meet still refuses in both tiers. `bank.rs` keeps the fewest count first.

## Rejected

- Keeping `required` as built: it is opt-in, text-based and fires where it need not. Decided against by the owner.
- Keeping `required` with a stricter meaning (a value, not a bound): it would refuse `.balance_resistance >= 20kohm`, which is a legitimate statement once the pick honours it, and it still leaves a plain `Resistor` uncovered.
- A keyword `any`: a second spelling of a range.
- An error always for an unconstrained pick: it cannot be waived without writing a range that is there only to silence it, and it would refuse boards in progress.
- Reporting at the checker, before the solve: whether a pick is limited depends on what propagation settles (a member narrowed by a join) and on whether a bound can be judged. The test on the solved model is the one that is true. ADR 0069 chose the text for a different reason (the solver's own pick makes a value look stated); here the test is made before the pick, not after.

## Open questions, with a recommendation each

1. **Severity.** Warning by default and error under strict, as for open circles. Recommended.
2. **Which catalogs have value columns.** The three E-series generics now, others when needed. Recommended; the list is one table.
3. **Does a bound on a rating count?** No. It limits the row, not the value.
4. **A trivial bound or a range of the whole domain.** It counts, as `>= 0ohm` counted for `required`. Recommended: the finding cannot judge width, and the range is the documented way to say "any".
5. **Should stage 1b exist, or should the notes stay as they are?** It exists, in stage 1, because seven of the eighteen marks in one downstream project are voltages that only a check reads, and without it dropping `required` would let a board omit them silently. Recommended.
6. **Transfer of bounds through `~` joins and `==` between two bare fields as a join.** Not now. A separate decision about `==`.
7. **A parse error that points at the removed word.** No.
8. **Should a module checked alone report an unstated member?** No. Both findings are made on a solved board, where the use is known. A module that no board uses builds, as an unused one did with `required`.

## What to look at

- The mechanism of section 1, in `try_candidate`, `Coupling::feeding` and `propagate`, and the claim that the transfer is sound because `x &= g` places `x` inside `g`.
- That the transfer is the smallest change and that a value flowing back from the part was rejected for the order dependence of the pick.
- The definition of "limited": the four rules, and that a part's own relations and a rating do not count.
- That a range says "any", and that no keyword is added.
- The severity, and the choice of one finding for one cause when an open circle names the check.
- The roots rule of stage 1b, above all the exclusion of parts, interfaces and columns, which keeps the finding from firing on the buck stage's optional limits.
- The count: none of the examples or guide blocks that reach the solver is flagged, and the gate of stage 1 is that this stays true.
- That pin rows keep `required`, and the editor's highlighter and its bundle are not touched.
