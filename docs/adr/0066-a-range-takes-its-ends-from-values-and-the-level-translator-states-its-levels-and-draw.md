---
status: proposed for owner review (implemented on 2026-11-06 by an agent that was told to go with what makes sense from earlier decisions; every choice below, the syntax, the meaning of an end with no value, the `&=` that states the draw, the input's levels and the output that states none, is the agent's and is listed with the alternatives it rejected and a list to look at)
---

# A range takes its ends from values, and the level translator states its levels and its draw

Amends ADR 0063 (the two open questions it ends with) and ADR 0027 (what counts as a stated term of a budget).

## Summary

1. A range may have a value at either end: `.vcc.current &= 0A .. .max_supply_current`. The grammar already read an expression at each end of a range; only the checker refused it (`both ends of a range must be fixed numbers`). No syntax is added. The statement waits for its ends like any statement that names a value, then means what the same range written with numbers would mean. While an end has no value it claims nothing and is named as not evaluated.
2. `std::parts::logic::LevelTranslator` has `.a` and `.y` as `Logic` lines. The input's `.vih` is `.vt_plus_max`, its `.vil` is `.vt_minus_min` and its pin's `.abs_max` is `.a_abs_max`, so the solver judges the driver without a statement of the board. The output states no level (the catalog has no column for it). The part's supply draw is its own: `.vcc.current &= 0A .. .max_supply_current`, so a design that picks a row gets the row's range on its rail.

Both parts land together, because the second is the use that the first was made for. The breaking change is the pin paths of `LevelTranslator` (`.a.line` and `.y.line`); the generic has no standard row and nothing but its tests and the guide used it.

## 1. A range whose ends are values

### The syntax

The smallest syntax is none. `range := tol (".." tol)?` in the parser (`crates/netsmith-lang/src/parser.rs`) takes an additive expression at each end since the language had ranges (`1V +/- 5%..2V` parses, a tolerance binds tighter than the range), and the AST has an `Expr` at each end. Checked against the other forms:

- A range with names at its ends reads unambiguously: `..` is its own token, never part of a name or a number (`1..5` is a range, `.a.b` a path), and the first end is whatever precedes it. The only thing to take care of is a name with a leading dot after `..`: `0A...max` reads (the lexer takes `..` first, then `.max`), and is ugly, so the printer writes `0A.. .max` and the guide writes `0A .. .max_supply_current`. A space is optional on input.
- `+/-` is not changed. A tolerance with a name for the centre or the width still needs fixed numbers (`a tolerance needs a fixed value and a fixed tolerance`). It was not asked for and a tolerance has a second wrinkle (a relative width is a ratio of the centre), which is another decision.
- `0A .. .max_supply_current` is a range and `.x +/- 5%` a value with a tolerance, so there is still one spelling of each.

Rejected:

- `up to` or `max(...)` or `min(...)` as new words. A range is already how the language says "from here to there"; a function for the maximum of two values would say the same thing a second way.
- A bound pair instead of a range: `.vcc.current >= 0A` and `.vcc.current <= .max_supply_current`, which is the form `LevelTranslator` had (`<=` only). A bound is a check on a value some other statement gives, and never a value. The rail's current stays unknown and everything that reads it waits, which is the fault.
- A range keyword with a different end marker (`0A ..= x`, `0A to x`). One more thing to learn for nothing the existing marker cannot say.

### What it means

`lo .. hi` with a value at an end is the smallest range that holds the lowest value of `lo` and the highest value of `hi`: the same as `Quantity::range` for numbers (`80nA..120nA .. 700nA..800nA` is `80nA..800nA`). An end that is itself a range is never trusted to be narrower than it says. It is evaluated by the solver, over the values the fields have at that moment, as an expression is.

- **Both ends have a value:** the statement is the range, as if it had been written with the numbers, in every place it can stand (`&=`, `?=`, `==`, a bound, an argument of arithmetic inside parentheses).
- **An end has no value:** the statement claims nothing. It narrows nothing, refuses nothing, and the build lists it as not evaluated, with the range as written: `not evaluated: gate.vcc.current &= (gate.min_draw..gate.max_draw)`. A relation that waits for a value does the same today; a range is no exception, and a range with an unknown end is never read as the other end alone or as unbounded. Nothing is guessed to fill a missing end.
- **The low end is above the high end:** the statement is refused with the two ends as the solver evaluated them: `(gate.min_draw..gate.max_draw): the low end of the range is above the high end: 2uA against 750nA`. A range with numbers at both ends was refused at check time with `the low end of the range is above the high end`, and still is.
- **The ends have different dimensions, or one is text:** refused at check time, `cannot form a range from V and A`, `the ends of a range are numbers, not text`.
- **Order of statements:** nothing depends on it. The statement is a relation like any other; the solver settles values until nothing changes.

Soundness. A narrowing only ever intersects, and a range read late is the same range as one read early: the fields an end reads only narrow, so a range read from a wider value is wider than the one read later, and the intersection with it holds the later one too. A range whose ends were never known is not applied. The one place where timing could matter is the balance of currents, and it is guarded (next).

**The balance and a part's own range.** A rail's current that nothing states is worked out from the others on its net (ADR 0007). A part that states its draw with a range whose end is not known yet (a catalog row not picked) used to be worked out by the net first: a supply stated as allowed 500 nA made the translator's draw `0..500nA`, and the row's `0..750nA` was met with it afterwards and absorbed in the overlap, so the board passed with a supply that could not feed its part. That is a hole of order, because the same design with the end known from the start was refused (after section 3 it is refused for both forms of the range). The solver now leaves the field alone while it is the left side of a range with a value at an end and has no value, and works it out from nothing: the part's own statement comes first, and the net verifies it afterwards with the numbers (`Solver.claimed`, `claimed_by_ranges`). A design whose range never gets its ends keeps the field unknown, named as above.

A second thing the translator showed (it is the reason for `.a.reference.current &= 0A` below): a stated current of exactly zero adds nothing to a net and is not an allowance, so it is no longer the "stated" term of a budget (`Solver::budget`). Before, a board that stated `0A` on a rail and a budget on another had two stated terms and no budget was checked. After, the budget is. The solver already ignored a zero term when it worked a part's current out (ADR 0026); the budget now does too.

### Where it is implemented

`crates/netsmith-check/src/lower.rs` lowers a range with a value at an end to a new `RExpr::Range` (a range with numbers at both ends is the same `RExpr::Const` as before); `crates/netsmith-solve/src/eval.rs` evaluates it with plain interval arithmetic and does not tighten an expression that contains one (the hull of two ends is not a smooth function, so the plain result is the sound one, wider at worst); `crates/netsmith-lang/src/printer.rs` prints the space after `..`.

## 2. `LevelTranslator`: its levels and its draw

### The input and the output as `Logic` lines

```nsm-fragment
    .a: Logic
    .y: Logic
    .a.reference ~ .vcc
    .y.reference ~ .vcc
    .a.reference.current &= 0A
    .y.reference.current &= 0A
    .a.vih &= .vt_plus_max
    .a.vil &= .vt_minus_min
    .a.line.abs_max &= .a_abs_max
    .vcc.current &= 0A .. .max_supply_current
```

- **The input.** `.vt_plus_max` is the highest the positive-going threshold can be, so an input above it is a high for sure, which is what `vih` is (ADR 0046: the lowest voltage an input reads as high, guaranteed). `.vt_minus_min` is likewise `vil`. The statements are `&=` as in `Sn74lvc1g04`, not `==`: it is a statement of the part about its own pin, and a board cannot change a standard kind's levels either way. The solver's net rule then holds a driver on `.a.line` to `voh >= vih` and `vol <= vil`, which the guide's example had to state on the board (`.sensor.out.voh >= .u.vt_plus_max`); the two lines are gone from the example. A driver with no levels is not evaluated, as for any `Logic`.
- **The input's absolute maximum.** `.a.line.abs_max &= .a_abs_max`, so the net rule also holds the worst voltage of the input's net to the rating, with numbers (`net net-d.out.line reaches 3.465V above its ground GND ... but u.a.line.abs_max is 2.3V`). `.a_peak <= .a_abs_max` stays: it is the design's statement of the transient, which the net rule does not see. `.a.line.abs_min` is not stated because the catalog has no column for it.
- **The reference rails.** A `Logic` line's `.reference` is joined to `.vcc` by `~`, which equates everything but the current: the reference is a rail of its own on the same net with a current nobody states, and the balance of the supply net had two unknown terms for the two lines, so no budget on that net was ever checked. The generic states `.a.reference.current &= 0A` and `.y.reference.current &= 0A`: the lines name the supply for their levels and carry nothing, all the draw is `.vcc.current`. This is true of any part that joins a `Logic` reference to its own rail (`Sn74lvc1g04` too); `Logic` itself was not changed here, which would change every part and example that has one. It is left for the owner (open question 3).
- **The output states no level.** The part's datasheet gives `VOH` and `VOL`, but only at a stated load (the pair `VCC - 0.1 V` and `0.1 V` at 100 uA in the datasheet of `Sn74lvc1g04`, and larger drops at larger currents). So a pair exists. Columns for it were not added, for two reasons that are not about the data: the loader of a catalog file requires every column its generic lists, so a new column breaks every project row, provider catalog and guide example that was written to the header of ADR 0063 (the optional columns of ADR 0055 and ADR 0058 are three named columns of the standard kinds, not a mechanism of the language); and the right columns (a drop from the supply, the load current it holds at, one pair or one per load) are a choice about the generic's interface which the owner has not made. `.y` is therefore a `Logic` on the supply that states no level: a receiver on `.y.line` that states its own levels is listed as not evaluated against it (`a level has no value`), never as passed, and a design that has the datasheet's pair states it (`.u.y.voh &= .u.vcc.voltage - 0.1V`). The alternative, `.y.voh &= .vcc.voltage` and `.y.vol &= 0V`, is a claim about an unloaded output that no datasheet gives and a loaded one does not keep.

### The draw is the row's own

`.vcc.current &= 0A .. .max_supply_current` replaces `.vcc.current <= .max_supply_current`. The part states what it draws; a bound on a value that the design states checks it, and the design had to state it for the rail to count it. Now a design that picks a row gets the row's range on its rail, the net is solved or checked, and the checks that read it run (a budget on the supply, a regulator behind it).

What a design that states `.u.vcc.current` meets:

- A range partly above the maximum is narrowed to the row (`&= 0A..2uA` gives `0A..750nA`). It was refused before (`u.vcc.current must always be at most u.max_supply_current, but 0A..2uA is not always below 750nA`). The design's number was a claim about the load, and the part's datasheet bounds it: a design never states what a datasheet already bounds (`docs/DESIGN-RULES.md`, "A part states its own requirements").
- A range wholly above is a contradiction: `no level_translator candidate satisfies u: ... fails because contradiction on u.vcc.current: it is already 2uA..3uA, which does not overlap 0A..750nA`.
- A narrower range stays (`0A..500nA`): the design claims less than the datasheet's maximum, and a claim of the design is what it is (ADR 0038 treats a stated current as the design's).

`.max_supply_current` is, by ADR 0063, "the largest current the supply pin draws". A row's author enters the figure for the use the row is for: a datasheet that gives only a static current for a part that switches does not give the draw, and the doc of the generic says so. A design cannot widen the draw beyond the row's, so a switching design needs a row whose figure covers it (a project's own row). That is the cost of the part stating its own draw and not offering it.

Rejected:

- `.vcc.current ?= 0A .. .max_supply_current` (ADR 0051, a default a board can replace). It would keep every design that states a draw byte for byte as before and give one with none the row's range; the old relation would keep judging the stated one. It was not chosen because a draw is a fact about the part and not an offer (the reason ADR 0070 gives for `PullUp`), and because it would let a design state a draw above the datasheet's maximum. Its attraction is that no existing statement changes meaning. Say if you want it: it is one operator.
- `.vcc.current == 0A .. .max_supply_current`. An equation on a field that has a range holds only if the equation's range lies inside it, so a design that states the draw exactly as the datasheet's maximum (`0A..750nA`) passes and one that states any other range fails, in either direction; an invented provider whose row has a larger maximum than the design states (8 uA against `0A..750nA`, in `an_invented_provider_of_the_translator_conforms_and_serves_a_5v_supply`) is refused. Narrowing with `&=` keeps that design.
- Keeping `<=` and adding the `&=`. After the `&=` has narrowed the field to the row's range, the `<=` can never fail (a range wholly above is already a contradiction), so it is dead text.

### Places in `std/` that tie a number to a range with a relation

Listed, and converted only where the result is the same or better. One was.

- Converted: `LevelTranslator.vcc.current <= .max_supply_current` (above).
- Not converted, the part's rating of a value the circuit decides: the current through a part against `.max_drain_current` (`NMosfet`, `PMosfet`), `.max_forward_current` (`Diode`, `Schottky`, `Zener`, the like), `.rated_current` and `.saturation_current` (`FerriteBead`, `Fuse`, `Inductor`), `-.vout.current <= .max_current` (`LinearRegulator`), `-.vout.current <= .buck.max_output_current` (`BuckStage`); the voltages `.drain_source_voltage`, `.gate_source_voltage`, `.bias` against their maxima; `.vcc.voltage >= .vcc_min` and `<= .vcc_max` of `LevelTranslator`, `.vin.voltage` of `BuckStage`; the inductor's, capacitors' and the buck stage's minimums. A `&=` there would narrow a value that the circuit or the supply decides, and a range that only partly overlapped would be cut to the overlap with no failure: the overload that the relation exists to refuse. They are checks and must stay checks; none is a statement of the part's own draw.
- Not converted, a stress the design states: `.vcc_peak <= .vcc_abs_max`, `.a_peak <= .a_abs_max`, `.output_current <= .max_output_current`. Same reason; and the design, not the part, gives the number.
- Not converted, a draw that is not the datasheet's maximum: `Sn74lvc1g04` states no supply current, because the datasheet gives only the static maximum (`ICC`, table 5.5, 10 uA) and the current of a switching input depends on the use. A board states `.vcc.current` for its use. `PullUp` and `I2cPullups` state their draw as an equation of the resistor, which is the circuit's, not a catalog column.

`LevelTranslator` is the only std definition that states a draw from a catalog column.

## 3. After a review: three findings on the draw (2026-11-07)

A review of this ADR's implementation found three faults in the draw of `LevelTranslator` (findings 1 to 3 of the review). The owner decided to follow what atopile does. `docs/references/atopile-v2-language-guide.md` is the reference, and the agent also read `ElectricPower` in atopile's source (`src/faebryk/library/ElectricPower.py`) for the supply side.

**Finding 1: a design's wider draw is intersected with the part's, silently.** Kept, with no change of behaviour. atopile's `&=` is an intersection and nothing else: section 2 of the guide says "Writing `&=` intersects the field's set with yours; the build fails if any set becomes empty", and section 4 says "Repeated `&=` on one field accumulates: everything ever asserted about it intersects, wherever it was written. Compatible requirements solve; incompatible ones fail with a contradiction, never a silent override." A design's `0A..3mA` and the part's `0A..750nA` are compatible requirements, and their intersection is `0A..750nA`; only a range wholly outside the part's is a contradiction (and it is, as before). The guide says so now. The cost the review named is real (the checks after it are made on 750 nA, and a design that has a true draw above the row's must give the part a row of its own), and it is what a set-valued `&=` means. `?=` stays the owner's open question 2.

**Finding 3: the verdict depended on how the end was written.** Fixed. A current counted as a stated term of a budget (ADR 0038) when its range was a number at the start of the solver, so a part that wrote its draw `0A..750nA` was a second stated term beside the supply's allowance and the net got only the weak test (can the currents add up to zero), while `0A .. .max_draw` was not stated until its end had a value and the net was held to the allowance (`supply.current must stay within -500nA..0A, but the others on it work out to -750nA..0A`). The same draw passed in one form and failed in the other, and the form that passed accepted a supply that cannot feed the part (the claim made in this ADR, "the same design with the end known from the start is refused", was false for the literal form). atopile gives no rule for a budget, but its sets give the answer for soundness: a part's draw is what the part says about itself (the datasheet's guarantee) and an allowance is what a supply promises; a check must not pass on a draw tighter than the part guarantees. So:

- A range on a current that is inside a part (a field whose owner is a part or inside one: the draw of its supply pin) is the part's claim and never a stated term of a budget, whoever writes it (the part's body or a board that states the draw) and in either form. The supply's allowance is then the one stated term and the claim must lie inside it. A board that states `.u.vcc.current &= 0A..750nA` on a part is judged the same way as a part that states it itself (`Solver::stated`, `inside_part`).
- A range with a value at an end on a current that is not inside a part (a board's or a module's own rail) is a stated term once its ends have a value, as the same range with numbers is (`Solver::stated_late`). Before, it was never one, so the verdict differed there too.

What changes for a design: a board that stated a draw on a part with a range beside a supply with an allowance used to be held only to "can add up to zero" and is now held to "the claim lies inside the allowance". No example, example project or test of the repository does either, so no output of theirs changes (the gate). A board that did that and passed because of the weak test may now fail with the budget message, which names both ranges; it is the check the ADR 0038 gives a net with one stated term. Tests: `crates/netsmith-solve/tests/range_forms_agree.rs`.

**Finding 2: the output current the translator sources is not in its supply draw.** Not decided. The question is whether a part's supply current includes the load that its outputs source. atopile's model does not answer it: `ElectricPower.max_current` is a number a design or a component states on a rail, and carries `is_sum_bus_parameter` (a bus sums the rails joined to it), and nothing in the guide or in that file says that a component's supply current contains what its outputs deliver. The options are written for the owner in `/home/noah/.t3/netsmith-translator-draw-options.md`; `LevelTranslator` keeps `.vcc.current &= 0A .. .max_supply_current`.

## Tests that changed

- `crates/netsmith-check/tests/errors.rs`: `a_range_needs_fixed_ends_and_a_tolerance_a_fixed_value` became `a_tolerance_needs_a_fixed_value_and_a_range_end_may_be_a_value`: the range with a value at an end is no longer an error; the tolerance's is kept; a dimension mismatch and a unit mismatch are the new assertions.
- `crates/netsmith/tests/std_additions.rs`: the provider's pin table and the board's joins name `.a.line` and `.y.line`; the assertion `u.vcc.current must always be at most u.max_supply_current, but 0A..2uA is not always below 750nA` is replaced by the two cases above (narrowed, contradiction).
- `crates/netsmith-check/tests/std_generics.rs`: the pins a provider of `LevelTranslator` must give are `a.line` and `y.line` (they were `a` and `y`).
- `docs/LANGUAGE.md`: the translator's example and its failing example (the driver's levels are judged by the solver: the message names the net and the two levels instead of the pick's relation).

No other existing test changed.

## New tests

- `crates/netsmith-solve/tests/member_range_ends.rs` (seven): a member end is read once it has a value; an end that is a range gives the widest range; an end with no value is not a claim and is named, nothing is refused; a low end above a high end is refused with the two values; the range reaches the rail and the regulator that supplies it; a supply budget that the draw does not fit is refused with the numbers; the order of the statements, in the board and in the part, changes nothing.
- `crates/netsmith-solve/tests/translator_levels.rs` (seven): the input's levels come from the row; a driver of 0.94 V to 1.06 V onto a 1.1 V input is refused with the numbers and no statement of the design; a 3.3 V driver onto the input of a 1.8 V row (2.3 V maximum) is refused with the numbers; the row's draw reaches the rail with nothing stated, for each of two rows; a stated draw is narrowed, kept if narrower, and a draw wholly above is refused; a receiver on the output that states levels is not evaluated, naming the output's level; a supply allowed 500 nA is refused with a picked row of 750 nA (it fails without the guard of section 1).
- `crates/netsmith/tests/std_additions.rs`: a picked row's own supply current on the rail, with a supply budget refused; the stated draw cases.
- `crates/netsmith-lang/tests/parse.rs`: the range with names at its ends parses, prints with the space, and prints to text that parses to the same tree.
- Guide examples: a part with a member-ended range (`evaluate`) and the budget it fails (`fails`).

## Checked

The build output of every `examples/*.nsm` (default and `--strict`) and of the two example projects (`enclosure_sensor`, `schematic_bench`) is byte for byte the output of the snapshot taken before this change, files, standard output, standard error and exit code. No example uses `LevelTranslator`, a range with a value at an end, or a `0A` rail beside a budget. The tests of `netsmith-lang`, `netsmith-quantity`, `netsmith-check`, `netsmith-solve`, `netsmith-lower` and `netsmith` pass, with the changes listed above.

## What to look at

- That no syntax was added, and the printed form `0A.. .max`; and that `+/-` still needs numbers.
- `&=` for the part's own draw (and not `?=` or `==`), and its consequence: a design's wider draw is narrowed and cannot widen the row's.
- That `.a` and `.y` changed from pins to `Logic` lines, so a board joins `.u.a.line` and `.u.y.line` and a provider's pin table names them.
- That `.y` states no level, and why no catalog columns were added.
- The `0A` on the two reference rails, and that `Logic` itself was not changed.
- The guard in the balance (`claimed`) and the budget that ignores a zero term: both are solver rules, with the cases above.
- The sentence of ADR 0064 under the translator ("a single-supply buffer used as a translator states plain pins and thresholds and is judged by its own relations") is no longer true; it was left as written because ADR 0064 is another job's text.

## Deviations from the request

None in the two parts. The output's levels are not columns and the requested `==` between `.a.vih` and `.vt_plus_max` is `&=` (the same value; the statement of a part about its own pin). Two solver rules were added that the request did not name, because the feature was unsound or useless without them (the balance guard and the zero term in a budget).

## Open questions

1. Output levels: columns (`.voh_drop` and `.vol_max` at a stated `.load_current`, or a pair per load) and how a catalog marks a column optional, so that existing rows still load.
2. `?=` instead of `&=` for the draw, if a design must be able to state a draw wider than the row's.
3. Whether `Logic` states `.reference.current &= 0A` itself, for every part that joins its reference to its own rail.
4. A tolerance with names (`.x +/- .y`): not done.
