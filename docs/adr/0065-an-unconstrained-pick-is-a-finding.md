---
status: stage 0, stage 1 and stage 2 implemented on 2026-11-06, with the answers to the open questions as recommended (section 8 says what was built and where it differs from sections 1 to 7; the owner decided to replace `required` by the findings and to drop it, and the rest was written by an agent, each choice with the alternatives it rejected). The status before: proposed for owner review (the owner decided on 2026-11-06 to replace `required` by a finding about unconstrained picks and to drop `required`; the definition of "constrained", the transfer of bounds, the second finding for values that are not picks, the severity and the staging were written by an agent, each with the alternatives it rejected)
---

# An unconstrained pick is a finding, and `required` goes

## Summary

`required` (ADR 0060) was meant to stop a board from leaving a value to the solver's first row. A probe of 2026-11-06 showed two things.

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
- A board that stated `.lamp.balance_resistance >= 20kohm`: `required` was satisfied (a bound counts as stated, ADR 0060) and the build picked 330 ohm and 330 ohm.
- Before `required`, a board that stated nothing built with 330 ohm resistors that each drew 8 mA, and nothing was said.

## 1. A stated bound reaches the pick (stage 0)

### The mechanism

Reading `crates/netsmith-solve/src/solve.rs`:

1. A bound is a relation and never a domain. `Model::relations` holds `lamp.balance_resistance >= 20kohm` as a relation (`netsmith-check/src/lower.rs`, `relation_statement`). Only `&=` with a constant narrows a domain at once (`narrow_statement`, `apply_checked`). `propagate` evaluates only `==` and `&=` that have a field alone on the left. So the member stays `Unset`.
2. The link `top.resistance &= .balance_resistance` carries a value in one direction only: from the member, on the right, into the part's field, on the left. When the member has a stated range, `propagate` sets the part's field from it and the pick honours it. When the member has only a bound, nothing flows, and nothing flows back from the part to the member.
3. `try_candidate` checks, for a candidate, the relations that name a field inside the part, and the relations that name a field that the candidate's values changed, if no part still to be chosen feeds them (ADR 0057). The bound names only the member. The member is not inside the part, and it does not change, because nothing sets it. So the bound is never judged against a candidate.
4. `Coupling::feeding` (the parts a relation's value waits for) follows `from[out] = the fields the right side of an equation reads`. From the member there is no edge to the part: the edge goes the other way, from the part's field to the member. So the bound has no feeding part either.
5. After the picks, `solve_model` verifies every relation. The member is still `Unset`, so `verify` returns `Unknown` for the bound and for the `&=`, and both go into `deferred`, which is printed as `not evaluated`. The probe printed the `&=` line; the bound should be listed the same way (I expect a second line from reading the code and did not run it).

ADR 0057's case works because the equation is written the other way round: with `.member == .part.field` the member is on the left, the pick sets the member, the member changes, and the bound is judged (`try_candidate`, the `changed` set). ADR 0060's remark that "the member follows" the pick holds for that direction and for `~` joins only. It does not hold for `part.field &= member`, which is how `Lamp` and several downstream modules write it. The two spellings mean the same to a reader and differ in the solver. That is the root cause.

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

- **A value flows back from the part to the member** (`x &= g` sets an unset `g` from `x`, as `~` does). The member would then follow the pick, the bound would be judged by the existing `changed` rule, and ADR 0061's trouble with `==` chains would go away. It is not sound as a general rule: with `top.resistance &= .m` and `bottom.resistance &= .m` and `.m` unset, the first pick would set `.m` to a point value and force the second part to the same row. That is a new requirement nobody wrote, and which part is picked first would decide it. Where `.m` is stated, the rule does nothing. A symmetric `==` between two bare fields could be treated as a join; that is a separate decision about `==` and is left open.
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
- ADR 0060 is marked superseded by this ADR. ADR 0061 and ADR 0063 get a line each: `PullUp.resistance` and `I2cPullups.resistance` are not `required` any more.

What happens to each use that exists:

| use | after the removal |
| --- | --- |
| `std::bus::PullUp.resistance` (ADR 0061) | The word goes. A board that forgets the resistance gets the unconstrained pick finding for the pull-up's resistor, with the member named. A board that states a current budget on the supply gets a limit, since a budget is rule 4. The doc of the module says that the resistance should be stated. |
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
- **`generic` and `provider` (ADR 0059).** The choice of a provider is a choice among parts and is not what is reported. A part chosen for a generic is a pick like any other, and its value columns are judged as above. A generic's own values that a use states are statements, and rule 1 reads them.
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

**Stage 2.** `required` after a type is gone: `Declaration.required` (`ast.rs`), the parser, the printer, `crates/netsmith-check/src/required.rs` and its hooks in `checker.rs` and `lower.rs` (`stated_const`, `refuse_default_on_required`), the 23 tests of `crates/netsmith-check/tests/required.rs`, the 4 of `crates/netsmith/tests/required.rs`, the 2 of `crates/netsmith-lang/tests/parse.rs` (one test says that the word after a type is the parser's ordinary error), `required` on `std::bus::PullUp.resistance` (two tests of `pullup.rs` became tests of the finding, a third lost the word), the guide section, the glossary term, the skills' lines. `stated`, which `?=` needs, stays. A pin row keeps `required`. The editor's source changed in a comment and a test only (the page is the same bytes, its hash is new). ADR 0060 is marked superseded, ADR 0061 and ADR 0063 have their line.

**Left open.** The transfer through `~` joins and through arithmetic, and a bound on a column other than the three E-series generics' value columns, as sections 1 and 2 decided. Another catalog joins `VALUE_COLUMNS` when its use shows the same failure. The downstream project still has the word to delete; until it does, its build stops at the parser.

## Rejected

- Keeping `required` as built: it is opt-in, text-based and fires where it need not. Decided against by the owner.
- Keeping `required` with a stricter meaning (a value, not a bound): it would refuse `.balance_resistance >= 20kohm`, which is a legitimate statement once the pick honours it, and it still leaves a plain `Resistor` uncovered.
- A keyword `any`: a second spelling of a range.
- An error always for an unconstrained pick: it cannot be waived without writing a range that is there only to silence it, and it would refuse boards in progress.
- Reporting at the checker, before the solve: whether a pick is limited depends on what propagation settles (a member narrowed by a join) and on whether a bound can be judged. The test on the solved model is the one that is true. ADR 0060 chose the text for a different reason (the solver's own pick makes a value look stated); here the test is made before the pick, not after.

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
