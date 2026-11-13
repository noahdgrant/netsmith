---
status: superseded by ADR-0123
---

# A bank of capacitors is one part with a count

Written on 2026-11-05 by an agent told to pick an option and go on. Accepted as built by the owner, 2026-11-11. Tests 2 (the solver and lowering read a member by its name) and 3 (no repeat or array construct). Cut from `git show 8023ee9:docs/adr/0052-a-bank-of-capacitors-is-one-part-with-a-count.md`.

## The problem

A datasheet that asks for 47 uF of effective capacitance at the output of a 5 V rail is met by no single capacitor of the catalog: a ceramic part marked 47 uF keeps about 11 uF at 5 V. `BuckStage` picked one capacitor for each side, so such a board could not be built by the library. The language has no repeat or array construct and no parameter for a count, so a board that needed five capacitors had to write them itself.

## Decision

A `Capacitor` has a member `.count: Number`, the number of identical capacitors in parallel in that place. This is a bank (`GLOSSARY.md`).

- Unset, it is one. A number (`.count &= 3`) is that many. A range of whole numbers (`.count &= 1..8`) lets the solver choose: it tries every catalog row at one capacitor, then at two, and keeps the first pair of count and row for which every relation that mentions the part holds. The fewest capacitors win; among rows, the catalog's order.
- Every other field is the value of one capacitor. A requirement on the whole bank multiplies (`.count * .effective_capacitance >= 47uF`). A rating, the voltage at the rail's `.peak` and the package are per capacitor.
- `BuckStage` gives each of its two capacitors `.count ?= 1..8` (ADR 0051) and has `.input_count` and `.output_count`, which a board may state. A stated number is a requirement the pick must meet; a range limits the choice; nothing gives the fewest from 1 to 8.
- Lowering draws one part for each capacitor, numbered one after the other, on the same nets, sharing the pick. The first has the path of the instance (`stage.output_cap`); the second and later add `#2`, `#3`. The bill of materials lists the row once with the quantity.
- Only the standard `Capacitor` (a definition of the standard library, or a provider of it) is a bank. A user part with a `.count` (the segments of an LED bar) is one part. A review found the first version wrong on this.
- A requirement on a bank's part path (`.place`, `.height`, `.pin_protrusion`) holds for every capacitor of it, and a relation between its place and another part's becomes one relation for each pair. The search's record of what a failure depends on includes the count, so a refusal never skips a count that has a solution.

## What was rejected

- Fixed numbered slots (`cap_a`, `cap_b`, each optional): they put the maximum into the shape of the module, and an absent part is a new idea in a language where a declared part is always there.
- A board that writes the capacitors itself: a board cannot know the count before the solver has chosen the row.
- A keyword for the count's range (`bank .count 1..8`): `?=` already says "the library offers this and the board may replace it", and a range is already a value.
- A separate `CapacitorBank` part: two definitions of one thing that differ by a field.
- The row first and the count second, or each row at every count before the next: the first would put the order of the rows second, the second would pick five of a small row over two of a later, larger one. A bank of one must be what the stage did before.
- A count in `Pick` only: the count is a value of the part, so the design, the report and the editor read it from the model.
- A marker in the language (`bank` before a member), and keying on the catalog's name: new syntax is the owner's to choose, and a user part may use the catalog's name too.
- A note that the later capacitors of a bank are not covered by a requirement: it would leave the defect in place with a warning.
- `.2` for the second path: it would read as a member.

## Cost

The search is longer for a part with a range. The most a count may be is 64 (`MAX_BANK`), whatever range a design states. Ripple current is not modelled, so it is not divided. The placer does not keep a bank as a row: each capacitor is a decoupling capacitor and sits near its pin, so a bank of six on one net is dense and the silkscreen may overlap. A user part that wants to be a bank cannot be one. The search details, the message for no count (`2 x <row>`) and the numbers are in the header of `crates/netsmith-solve/src/solve.rs`; the user's view is in `docs/LANGUAGE.md` ("A bank of capacitors in parallel"), the `.count` doc in `std/parts/passive.nsm` and the tests `crates/netsmith-solve/tests/bank.rs`, `crates/netsmith-design/tests/bank.rs`.

## Left open

Nothing. The choices below were made by the author and are accepted as built:

- The name `.count`, read by the solver and lowering by name, and the language rule that a std `Capacitor` with a range of whole numbers is a bank.
- `?=` for `1..8` in `BuckStage` and the 8 (one number in `std/parts/regulator.nsm`); `.input_count` and `.output_count` as the module's way to state a count.
- The path `stage.output_cap#2`.
- `Capacitor` carrying `.count` for every capacitor, against a separate part.
- A user part cannot be a bank, and the language has no marker for one.
- The limit of 64, and a requirement on a bank's part holds for every capacitor, not the first.
