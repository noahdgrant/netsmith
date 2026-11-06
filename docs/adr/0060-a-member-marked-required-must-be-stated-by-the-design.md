---
status: superseded by ADR 0065 on 2026-11-06 (the owner dropped `required` after a type; an unconstrained pick and an unstated value are findings instead; the status it had before: proposed for owner review (the marker `required` after the type is the owner's pick of 2026-11-06; every other choice below, and the meaning of "stated", was made by an agent that was told to pick an option and go on, each with the alternatives it rejected))
---

# A member marked required must be stated by the design

Superseded by ADR 0065. `required` after a type was built, found to guard the wrong thing (a test on text, opt-in per member, silent about a bound that did nothing), and removed; the findings of ADR 0065 take its place. The text below is the history of the decision and is not rewritten. A `pins` row keeps its own `required`, which is another word with another meaning (ADR 0016).

A module or part may declare a value for the board to state: `.balance_resistance: Resistance`. A board that forgets it builds. Nothing is wrong with the text, so the checker says nothing. The relation that reads the value is "not evaluated". The solver picks any part that fits: two 330 ohm resistors, each drawing 8 mA, were picked for a balancing resistance nobody had stated. The only trace was a note.

## The decision

`.name: Type required` marks a declared value that the design must state. `required` goes after the type and before an optional body, and it is the word a `pins` row already uses for a pin that must be connected (`.p1 passive "TP" "1" required`). A use of the definition whose required member nothing states is an error that names the member by its path, the definition that declared it and the use, and says how to fix it:

```text
error: `lamp.balance_resistance` is a required member of `Lamp`, and `.lamp: Lamp` does not state it; state it with `.lamp.balance_resistance &= ...`
```

It points at the line that declares the use (`.lamp: Lamp`), in the file that has the use. `required` is not a new statement and not a new type: it is a modifier of the declaration, as `required` is a modifier of a pin row, so there is no second way to say that a thing is needed.

## What "stated" means: the text, before the solve

Two tests were possible. The first is on the solved value: the member is stated if its value is narrower than the whole of its type. The second is on the text: some statement of the design gives the member a value. This ADR takes the second.

The test on the solved value is wrong because the solver narrows too. When the member feeds a relation to a part, the pick narrows the part's field to one row, and the member follows. A balancing resistance that nobody stated becomes "330 ohm" the moment a resistor is picked, so the value that the solver chose would be taken for a value that the design stated, which is the bug itself. It also needs a solve to run before the error can be shown, so the checker's own errors would not be enough to stop a design.

The test on the text is cheap (one pass over the checked model, after the statements are read and before the solve) and does not depend on the catalogs. A member is stated when:

- an `&=` states it, or a `?=` default is applied to it. A default is the offer of the definition around the member (ADR 0051), and the board may replace it;
- an equation or an `&=` has the member alone on its left, and its right side has only numbers and stated members. That is the solver's own rule for setting a value: `propagate` evaluates a relation only with a field alone on the left, from a right side whose fields all have values. A member on the right of an `==` is read and not stated: the solver does not work a value back from the left side. This was found, not designed: re-expressing `I2cPullups` with two `PullUp`s (ADR 0061) left the inner resistor unset because `.resistance == .sda_pullup.resistance` has the wrapper's value on the left, and `required` now says so;
- a bound, `>=` or `<=`, has the member alone on one side and only numbers and stated members on the other. A bound is a statement of what the design needs of the value, as a range is;
- a `~` joins an interface that has the member to one whose member is stated. A join makes the two equal, in both directions.

The rules repeat until nothing more is stated, so a chain that ends in a stated value states every member on the way. The cost is that the test is on text: `.balance_resistance >= 0ohm` states the member and is no help to anyone. `required` guards against a value nobody wrote, not against a value that is nonsense.

A value that a catalog row or a pick gives is never a statement. A required member on a catalog column of a part would be stated by the board only.

## Where `required` is allowed

On a value (a quantity, text or an enum) declared in a module, a part, a generic or an interface. These are the definitions that have a user, and a user is who can state it.

It is an error in a board, a system, a panel and a physical file: they have no user above them, so the member could not be stated by anyone but the board itself, which can write `&=` at once. It is an error on a pin, on a part, on a module and on an interface-typed member (`.supply: Rail required`). Those are not values the design states. A pin that must be connected says so in its `pins` row, which is the other use of the word. A part or module is stated through its own values, and `required` can be put on those. `.supply: Rail required` was considered: the intent, "the board must join this rail", is the rule for a rail's pins that already exists, and a second spelling for it would break "one correct way".

## Which definition is judged

Every required member of every instance in the elaborated model is judged, except the members of the root of the model: the board or module that is being checked, and its interface members (`.supply: Rail` in a module). Those are the contract the root's users state. The root's required members count as stated while the root is judged, so the module `Desk` that forwards its own `.balance` to an inner `.lamp.balance_resistance == .balance` passes when it is checked alone. A module nobody uses is therefore not an error, and a module that is used is judged where it is used, with the full path.

A module that wraps another and states nothing is refused, whether used or checked alone. The fix is to state the value inside the wrapper or to forward it through the wrapper's own `required` member. When the board forgets the wrapper's member, the inner member is unstated too, and only the wrapper's is reported: a member linked by an equation or a join to an unstated required member of something that holds it is not reported again. The board can state only the outer one.

## `required` and `?=`

A default makes a member not required, so `required` and `?=` on one member of one definition are an error that points at the default and names the line of `required`:

```text
error: `balance_resistance` is `required` (at main.nsm:2) and has a `?=` default here: a default is an offer that makes the member not required; keep `required` and remove the default, or keep the default and remove `required`
```

A default written by a definition around the member, for a member the inner definition declared required, is allowed and counts as stated: it is the wrapper's offer, and the board that uses the wrapper may replace it. That is the only way `?=` and `required` meet, and it keeps a module able to say "the lamp's balance resistance is the desk's to decide, and the desk offers 3.3 kohm". Refusing it would make a wrapper repeat the board's statement.

## Generics

A generic's required member (`.max_frequency: Frequency required`) is read into every provider's instance as the generic's other members are (ADR 0059). It is stated by the use (`.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }`), or by the provider (`.max_frequency &= 6GHz` in the part). A use with a provider that states nothing, and nothing in the use, is the error above with the generic's name. The member is judged on the instance made from the provider in place.

The choice of a provider starts after a clean check, so a fault that depends on the provider is seen only for the provider that comes first. If the first provider leaves a number to the use and the use does not state it, the build stops there, though a later provider states it. A later provider that fails the same way is passed over as any provider that fails a check is, with the message as the reason. Making a trial of the first provider's check fault open the search was rejected here because it changes the pick for every kind of checker error, not only this one, and that is a decision about ADR 0059. It is the first open question below.

## What changed

`netsmith-lang`: the syntax tree's declaration has `required`, the parser reads it after the type, the printer prints it back. `required` is read as a plain word where a declaration has one, so a member, a type or a variable may still be called `required`. `netsmith-check`: `crates/netsmith-check/src/required.rs` has the rules above; the checker records every required member while it declares members, refuses the misuse where it is written, refuses `?=` against `required` where the default is read, and judges the model after the nets are made. The solver, the lowering and the exporters do not see `required`: it is an error before any of them runs. The editor's highlighter knows `required` (and `optional`, which a pin row also writes) as keywords, and the committed page was rebuilt. The guide has a section, the glossary a term, and two skills a line.

## Limits and left open

- A definition may be required to be stated and be stated in a way that says nothing (`>= 0ohm`). The text test cannot know.
- A required member in a physical file or a board's own interfaces is not judged (it is a contract of the root), so a board in a system that is not joined to the right port leaves the member unstated and unreported until a later rule looks at ports.
- `I2cPullups.resistance` is not marked `required` here: that changes what builds today for a board that forgets it (none in the repository does), and the owner decides.
- Warning instead of error for a use inside a std module was not considered: the std library has no required member yet except `std::bus::PullUp.resistance`.

## What to look at

- The word, its place (after the type, before the body), and that `required` is also the word of a pin row.
- The test of "stated" (text, before the solve) and each of its four rules, above all that `>=` and `<=` count and that a member on the right of `==` does not.
- That `required` is refused on pins, parts, modules, rails and in boards, systems, panels and physical files.
- That a wrapper's `?=` for an inner required member counts as stated, and that `required` plus `?=` in one definition is an error.
- The first provider decides (the section on generics), and whether the search should start from a check fault that depends on the provider.
- `PullUp.resistance` is the first required member of the standard library, and `I2cPullups.resistance` is not.
