---
status: provisional, awaiting the owner's review (the owner chose the marker, its word and its place, `optional` after the type; the meaning of "stated", what the checker refuses, and the placement of the skip in the checker were written by an agent, each with the alternatives it rejected)
---

# A member marked optional is skipped by the checks that read it when nothing states it

`std::parts::regulator::BuckStage` checks the inductor's saturation current and DC resistance against two members of the IC, `.buck.min_inductor_saturation` and `.buck.max_inductor_dcr`. A datasheet states them only sometimes, and the doc comment said each is checked only when the IC states it. The language could not say that. When an IC stated neither, the two relations were deferred and every build printed `note: not evaluated: reg3v8.stage.inductor.dc_resistance <= reg3v8.stage.buck.max_inductor_dcr` and its twin. The check is not applicable, so the note is noise. ADR 0058 recorded the question and ended with "whether a way to write 'only when the IC states it' is wanted". This ADR answers it: yes, as a marker on the member.

## Decision

A member declaration may end with the word `optional` after its type, before an optional body:

```nsm-fragment
interface Buck {
    .min_inductor_saturation: Current optional
    .max_inductor_dcr: Resistance optional
}
```

The relations that read the member are written as before (`.inductor.dc_resistance <= .buck.max_inductor_dcr`). A relation that reads an optional member that nothing states is skipped: it is not checked, not listed as not evaluated, asks nothing of a catalog row, and no finding is made for it. A member that something states behaves as every member does: the relation is checked, and fails with the message it always had when it is violated.

`optional` is the word that a `pins` row (ADR 0016) and the catalog columns (ADR 0055) already use for "may be absent". It is a modifier of the declaration, not a new statement and not a new type, so no second way to say a thing is added. The parser reads it as a plain word where a declaration has one, so a member, a type or a variable may still be called `optional`. It must be on the line of the type: on the next line it is the start of a statement.

## What "stated" means

ADR 0069 made "stated" a test on what is written and not on the solved value, because the solver narrows too: a value that a pick gave would be taken for one the design stated. That part holds here, for the same reason. The test is made by the checker after it has read every statement and joined every pair of interfaces, and before the solve. A member is stated when its class (the members that `~` joins to it, which hold one value) has a member that:

- an `&=` names, or a `?=` default was applied to (the default is the offer of the definition around the member, ADR 0051);
- an equation (`==`) or an `&=` has alone on its left, when the right side reads no optional member that is itself unstated, so a chain that ends in a stated value states every member on the way and a chain that ends in an unstated optional member states none;
- a bound (`>=`, `<=`) holds against numbers alone (`.max_dcr <= 0.5ohm`), as a bound states what the design needs of a value.

A bound against another member is a check and not a statement. This is where ADR 0069's lesson applied. `required` was dropped because it guarded the wrong thing: it was a test on text, it was opt-in per member, and it said nothing when a bound was satisfied by a statement that did nothing. Three consequences for `optional`:

- It guards exactly the member and no other operand. The relation is skipped because an optional member in it is unstated, never because another operand is unset. `reg.vin.peak <= reg.vin_abs_max` is still listed as not evaluated when nothing states the peak, as ADR 0057 and the test in `crates/netsmith/tests/providers.rs` require; and a relation that reads an optional member that is stated and also a value that nothing states is still listed as not evaluated.
- The library's own relation must not state the member. `.inductor.dc_resistance <= .buck.max_inductor_dcr` has the member alone on one side of a bound, and counting it would make every optional member stated by the relation that reads it. Only a bound against numbers alone is a statement.
- A statement that does nothing is not silent. `.max_inductor_dcr <= 0.5ohm` states the member, so the relation is kept; the bound gives the member no value, so the relation is then listed as not evaluated, as any check on a value that has only a bound is. The note says the check was not made, which a skip would not. The alternative, counting only a statement that gives the member a value (`&=`, `?=`, `==`), was rejected: a bound is what ADR 0069 and ADR 0065 count as a statement everywhere else, and a second meaning of "stated" is a second rule to learn.

A value that a catalog row or a pick gives is never a statement.

Across a `~` join, "stated" travels both ways. When `.stage.buck ~ .u.buck` joins two `Buck`s and the IC states the limit on its side, the stage's member is stated and the stage's relation is kept; when neither side states it, neither is. The join is read from the equalities the checker made, so it works through a wrapper module that forwards the interface, and a statement in a module around the member, on the board, or in a part, all state it.

## What the checker refuses

`optional` is allowed on a value (a quantity, text or an enum) declared in a part, a module, an interface or a generic: those definitions have a user, and the user is who states the member or leaves it. It is an error, with a message that names the member and says what to write:

- on a pin: the other use of the word is the pin row's, and the message says so;
- on a member whose type is an interface, a part or a module (`.supply: Rail optional`): the marker is for values, and the message says to mark the values inside it;
- in a board, a system, a panel or a physical file: they have no user above them, so the board can state the value where it declares it or leave it out;
- a statement of an optional member in the definition that declares it (`.max_dcr &= 0.1ohm`, `?=`, `==` or a bound against numbers, written in `Buck` itself): it would state the member for every use, so the relations that read it would never be skipped. Like `?=` against `required` in ADR 0069, it is an error that names both places. A definition that uses the member declared by another (a part that has a `.buck: Buck` and states `.buck.max_inductor_dcr &= 0.1ohm`) may state it: that is the use's statement. A check against another member (`.typical <= .max_dcr`) is not a statement and is allowed.

Rejected: allowing `optional` with a default in the same declaration and letting the default win. A default makes the member always stated, so `optional` would mean nothing; the second spelling for the same thing is the error.

## Where the skip is made

In the checker (`crates/netsmith-check/src/optional.rs`), at the end of checking a root: the relations that read an unstated optional member are taken out of the model's list. The solver, the lowering and the exporters do not see them, so none of them needs a rule: nothing is deferred, nothing is noted, no candidate is asked about them, and the findings of ADR 0065 do not mention them. The alternatives were:

- A rule in the solver that skips any relation with an unstated interface member. Rejected: it would silence `reg.vin.peak <= reg.vin_abs_max`, whose note ADR 0057 requires, and every other check that is meant to say "not evaluated".
- A default for the member in `Buck` (`?= 0A`, `?= 1Mohm`, a wide range). Rejected in ADR 0058 and measured again: a default on one side of a `~` join is not carried (`connected values disagree: stage.buck.min_inductor_saturation is 0A but u.buck.min_inductor_saturation is 4A`), and with a wide range `>=` and `<=` mean "always", so the relation holds for every inductor and says nothing.
- Dropping the note in `notes()` of `crates/netsmith/src/lib.rs` for these relations. Rejected in ADR 0058: it would hide a check that was not made, for a reason the library does not state.
- A conditional in the language (`if .max_inductor_dcr is stated { ... }`). A larger construct for one use, and a second way to write what a member's relation already says.

## What changed

`netsmith-lang`: `Decl` has `optional` (the word's place), the parser reads it after the type on the same line, the printer prints it back. `netsmith-check`: `Field` records where the declaration was marked, `optional.rs` has the refusals above and the skip, and the checker calls them where it declares members and where it reads a statement. The solver, the lowering and the exporters are unchanged; the editor's highlighter already knows `optional` as a keyword (ADR 0069 added it for the pin row), and no source of the editor's page changed. `std/parts/regulator.nsm`: `Buck.min_inductor_saturation` and `Buck.max_inductor_dcr` are `optional`, and the doc comments of `Buck` and `BuckStage` say so. The guide has a section with examples that its test runs, the glossary a term (_Optional member_), and ADR 0058's closing question is answered here.

## Cost and limits

- Only the two members of `Buck` use it. The other numbers an IC may leave open (`min_inductance` and the rest) are as they were; whether they should be marked is the owner's, member by member, because skipping a check hides it.
- The skip is on the whole relation. A relation that reads an optional member and a value that must always be checked (`.a <= .limit + .b`) is skipped when the limit is unstated. The library writes such a relation as two, or does not mark the member.
- "Stated" is a test on text: a bound `>= 0A` states the member and checks nothing useful. The relation is then listed as not evaluated, as above.
- A module that is checked alone is judged as its own root: its own members are the contract of its users, and a relation over an optional member of its own interface is skipped there when the module states nothing, as it is for a board that uses the module and states nothing.
- The provider search (ADR 0068) is unaffected: a provider whose statements state the member is judged on the relation, and one that does not has no relation to judge.

## What to look at

- The word and its place, and that it is also the word of a pin row and of the catalog columns.
- The three rules of "stated", above all that a bound against numbers states the member, that the library's own relation does not, and that the statement travels across `~`.
- That the skip is made where the model is finished, and removes the relation: nothing is listed for it.
- What is refused: a pin, an interface, a part or a module, a board, and a statement in the declaring definition.
- Whether other members of `Buck` and the standard library should be marked.
