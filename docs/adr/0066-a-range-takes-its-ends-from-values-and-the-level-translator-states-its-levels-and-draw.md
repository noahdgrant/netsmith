---
status: accepted
---

# A range takes its ends from values, and the level translator states its levels and its draw

Amends ADR 0063 (the two open questions it ended with) and ADR 0027 (what counts as a stated term of a budget). Implemented on 2026-11-06 by an agent told to go with what makes sense from earlier decisions. The meaning of an end with no value, the `&=` that states the draw, the input's levels and the output that states none are the agent's choices, accepted as built by the owner, 2026-11-11. Tests 2 (an unknown end claims nothing) and 3. Cut from `git show 8023ee9:docs/adr/0066-a-range-takes-its-ends-from-values-and-the-level-translator-states-its-levels-and-draw.md`.

## Decision

**A range may have a value at either end:** `.vcc.current &= 0A .. .max_supply_current`. The grammar already read an expression at each end (`range := tol (".." tol)?`); only the checker refused it. No syntax is added; the printer writes `0A.. .max` (`0A...max` reads but is ugly). `+/-` still needs numbers.

- `lo .. hi` is the smallest range that holds the lowest value of `lo` and the highest value of `hi`; an end that is itself a range is never trusted to be narrower than it says. The solver evaluates it over the values the fields have then, with plain interval arithmetic.
- With both ends known it is the range written with numbers, in every place it can stand. **While an end has no value it claims nothing**: it narrows and refuses nothing and is listed as not evaluated, with the range as written. A range with an unknown end is never read as the other end alone or as unbounded. Nothing is guessed.
- A low end above the high end is refused with the two ends as evaluated; ends of different dimensions, or text, are refused at check time. Order of statements changes nothing.
- Two solver rules had to follow. (1) A field that is the left side of such a range and has no value yet is left alone by the balance of a net, so the part's own statement comes first and the net verifies it afterwards (`Solver.claimed`); otherwise a supply allowed 500 nA made the translator's draw `0..500nA` and the row's `0..750nA` was absorbed in the overlap. (2) A stated current of exactly zero adds nothing to a net and is no longer a stated term of a budget (`Solver::budget`).
- A range on a current that is inside a part (the draw of its supply pin) is the part's claim and never a stated term of a budget, whoever writes it and in either form; the supply's allowance is then the one stated term and the claim must lie inside it. A range with a value at an end on a current that is not inside a part is a stated term once its ends have values (`Solver::stated_late`). Before, the same draw passed written `0A..750nA` and failed written `0A .. .max_draw`.

**`LevelTranslator`** (`std/parts/logic.nsm`): `.a` and `.y` are `Logic` lines. The input's `.vih` is `.vt_plus_max`, its `.vil` is `.vt_minus_min` and `.a.line.abs_max` is `.a_abs_max`, so the solver judges the driver with no statement of the board. The output states no level, because the datasheet gives `VOH` and `VOL` only at a stated load. The draw is the part's own, `.vcc.current &= 0A .. .max_supply_current`, so a design that picks a row gets the row's range on its rail. Both `Logic` references state `.reference.current &= 0A`. The breaking change is the pin paths (`.a.line`, `.y.line`); nothing but tests and the guide used the generic. The reasoning is in its doc comment; the language side is in `docs/LANGUAGE.md` ("A range whose end is a value", "A wrapper module's rail") and `crates/netsmith-solve/src/eval.rs`.

A design's wider draw is intersected with the part's, silently: a design's `0A..3mA` against the row's `0A..750nA` gives `0A..750nA`, and only a range wholly outside is a contradiction. The owner chose to follow atopile, whose `&=` is an intersection and nothing else (its v2 language guide, which is not in this repository).

## What was rejected

- `up to`, `max(...)` or `min(...)`, a different end marker (`0A ..= x`, `0A to x`): a range already says "from here to there".
- A bound pair (`.vcc.current >= 0A` and `<= .max_supply_current`): a bound checks a value some other statement gives and never is one, so the rail's current stays unknown.
- For the draw: `?=` (a draw is a fact about the part and not an offer, and a design could state a draw above the datasheet's maximum); `==` (it holds only if the range lies inside, so a design stating any other range than the maximum fails); keeping `<=` beside `&=` (dead text after the narrowing).
- Output levels as catalog columns: a loader requires every column a generic lists, so a new one breaks every project row and provider catalog, and the columns (a drop from the supply, the load current, one pair or one per load) are a choice about the generic's interface.
- Converting the other relations of std that tie a number to a range (a part's rating of a value the circuit decides, a stress the design states): a `&=` there would cut a partly overlapping range to the overlap with no failure, the overload the relation exists to refuse.

## Cost

A design that has a true draw above the row's must give the part a row of its own. A row's author enters the figure for the use the row is for. A board that stated a draw on a part beside a supply with an allowance used to be held only to "can add up to zero" and is now held to "the claim lies inside the allowance"; no example or test of the repository did either.

## Left open

Nothing. The choices below are accepted as built:

- An end with no value claims nothing while it waits.
- The output of `LevelTranslator` states no level; the columns `.voh_drop` and `.vol_max` at a stated `.load_current`, or a pair per load, are not built (ADR 0078 answered how a catalog marks a column optional). They are not planned until a board needs them.
- The draw is stated with `&=`, an intersection with no note, and not with `?=`: a design cannot state a draw wider than the row's.
- `Logic` does not state `.reference.current &= 0A` itself; `Levels` and the two references of `LevelTranslator` do.
- A tolerance with names (`.x +/- .y`) is not built.
- A part's supply current does not include the load its outputs source. atopile's model does not answer it, and `LevelTranslator` keeps `0A .. .max_supply_current`.
