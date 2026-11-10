---
status: proposed
---

# A member marked optional is skipped by the checks that read it when nothing states it

The owner chose the marker, its word and its place (`optional` after the type). An agent wrote the meaning of "stated", what the checker refuses, and where the skip is made, each with the alternatives it rejected. Tests 1 (a word in members that std and projects write) and 3 (a default, a solver rule, a conditional). Cut from `git show 8023ee9:docs/adr/0074-a-member-marked-optional-is-skipped-by-the-checks-that-read-it-when-nothing-states-it.md`.

## The problem

`std::parts::regulator::BuckStage` checks the inductor's saturation current and DC resistance against two members of the IC, `.buck.min_inductor_saturation` and `.buck.max_inductor_dcr`. A datasheet states them only sometimes. When an IC stated neither, the two relations were deferred and every build printed `note: not evaluated: ...` for each. The check does not apply, so the note is noise. ADR 0058 ended by asking whether a way to write "only when the IC states it" is wanted. This ADR says yes, as a marker on the member.

## Decision

A member declaration may end with the word `optional` after its type, on the same line, before an optional body: `.max_inductor_dcr: Resistance optional`. A relation that reads an optional member that nothing states is skipped: not checked, not listed as not evaluated, nothing asked of a catalog row, no finding. A member that something states behaves as every member does.

`optional` now has three meanings, one in each place it is written, and they do not meet:

- after a `pins` row: the pin may float (ADR 0016);
- after a column in a `catalog` statement: the column may be empty in a row (ADR 0055, ADR 0078);
- after a member's type: the checks that read the member are skipped until something states it (this ADR).

The word is a modifier of a declaration, not a statement and not a type. The parser reads it as a plain word, so a member, a type or a variable may still be called `optional`.

"Stated" is a test on what is written, as ADR 0069 made it, and not on the solved value. A member is stated when its class (the members `~` joins to it) has one that an `&=` names or a `?=` default was applied to; that an equation has alone on its left when the right side reads no unstated optional member; or that a bound (`>=`, `<=`) holds against numbers alone. The three rules the owner is asked to confirm:

1. A bound against numbers alone states the member. A bound against another member is a check.
2. The library's own relation does not state it (`.dcr <= .buck.max_dcr` has the member alone on one side; counting it would make every optional member stated by the relation that reads it).
3. The statement travels across `~`, both ways, also through a wrapper module.

A value that a catalog row or a pick gives is never a statement. The skip guards only the optional member: `reg.vin.peak <= reg.vin_abs_max` is still listed as not evaluated when nothing states the peak (ADR 0057).

The checker refuses `optional` on a pin, on a member whose type is an interface, a part or a module, and in a board, a system, a panel or a physical file. It also refuses a statement of an optional member in the definition that declares it, because that would state it for every use. The messages and the rules are in `docs/LANGUAGE.md` ("A value that may stay open") and in the header of `crates/netsmith-check/src/optional.rs`.

## What was rejected

- A default for the member in `Buck` (`?= 0A`, a wide range): a default on one side of a `~` join is not carried (`connected values disagree`), and with a wide range `>=` and `<=` mean "always", so the relation says nothing. ADR 0058 rejected it too.
- A solver rule that skips any relation with an unstated interface member: it would silence `reg.vin.peak <= reg.vin_abs_max`, whose note ADR 0057 requires.
- Dropping the note in `notes()` for these relations: it would hide a check that was not made, for a reason the library does not state.
- A conditional in the language (`if .max_inductor_dcr is stated { ... }`): a larger construct for one use, and a second way to write what the relation already says.
- `optional` with a default in the same declaration: the default makes the member always stated, so the word would mean nothing.
- Counting only a statement that gives the member a value (`&=`, `?=`, `==`): a bound is what ADR 0069 and ADR 0065 count as a statement everywhere else, and a second meaning of "stated" is a second rule to learn.

## Cost

- The two members of `Buck` were the first. Other std members carry it now (`grep -rn ': [A-Z][a-z]* optional' std`: `I2c.capacitance`, the diodes' `.bridge_voltage` and the like). Skipping hides a check, so each further member is a choice.
- The skip is on the whole relation. A relation over an optional member and a value that must always be checked (`.a <= .limit + .b`) is skipped when the limit is unstated; the library writes it as two.
- "Stated" is a test on text. `.max_dcr >= 0A` states the member and checks nothing; the relation is then listed as not evaluated, which says so.
- A module checked alone is its own root: a relation over an optional member of its own interface is skipped there when the module states nothing.

## Left open

- Whether other members of `Buck` and of std should be marked (`min_inductance` and the rest an IC may leave open). It is the owner's, member by member.
- Whether the three rules of "stated" are right (the owner's question): a bound against numbers states the member; the library's own relation does not; the statement travels across `~`.
