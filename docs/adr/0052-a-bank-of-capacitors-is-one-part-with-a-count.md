---
status: proposed for owner review (written overnight on 2026-11-05 by an agent that was told to pick an option and go on; the syntax and the semantics are the owner's to accept or change)
---

# A bank of capacitors is one part with a count

A datasheet that asks for 47 uF of effective capacitance at the output of a 5 V rail is met by no single capacitor of the catalog: a ceramic part that is 47 uF marked keeps about 11 uF at 5 V. `BuckStage` picked one capacitor for each side and checked its effective capacitance against the IC's minimum, so such a board could not be built by the library. The guide said so ("one capacitor stands for each side's capacitors"). The language has no repeat or array construct and no parameter for a count (the standard library has one definition per connector size for that reason), so a board that needed five capacitors had to write them itself.

## Decision

A `Capacitor` has a member `.count: Number`, the number of identical capacitors that stand in parallel in that place. This is a bank (`GLOSSARY.md`).

- Unset, it is one. A capacitor that nothing gives a count behaves exactly as before.
- A number (`.count &= 3`) is that many.
- A range of whole numbers (`.count &= 1..8`) lets the solver choose. It tries every catalog row at one capacitor, then at two, and so on, and keeps the first pair of count and row for which every relation that mentions the part holds. The fewest capacitors win; among rows, the catalog's order, as always. A range with no whole number in it is refused ("the `capacitor` catalog has nothing for `bulk`, which needs ... count within 2.2..2.8").
- Every other field of the part is the value of one capacitor. A requirement on the whole bank multiplies (`.count * .effective_capacitance >= 47uF`). A rating, the voltage at the rail's `.peak`, the package, are per capacitor.

`BuckStage` uses it on both sides. Its `.input_cap` and `.output_cap` keep their names and stand for one capacitor of the bank. The stage adds two members, `.input_count` and `.output_count`, the numbers a board reads and may state:

```
.stage: BuckStage { .vout.voltage &= 1.8V +/- 3%  .output_count &= 2 }
```

The stage gives each capacitor `.count ?= 1..8` (ADR 0051), ties `.output_count == .output_cap.count`, and checks `.output_cap.count * .output_cap.effective_capacitance >= .buck.min_output_capacitance`, and the same for the input. A board that states `.output_count` states a requirement that the pick must meet: the solver tries counts up to the first that meets it and that equals what the board said, and says what it tried if none does. A board that states a range (`.output_count &= 1..3`) limits the choice. A board that states nothing gets the fewest capacitors from 1 to 8, and a bank of one is the single capacitor the stage picked before.

Lowering draws one part for each capacitor. They are numbered one after the other (`C4`, `C5`, `C6`), have the pins of one capacitor on the same nets, and share the pick, so the bill of materials lists the row once with the quantity and the designators, and the schematic, the KiCad files, the placer and the router see real parts. The first has the path of the instance (`stage.output_cap`, which a layout file already keys it by) and the second and later add `#2`, `#3` (`stage.output_cap#2`), each a part with a path of its own. `origins` and `sourcing` have an entry for each part, so everything that reads the design by index is unchanged.

## What was rejected

Fixed numbered slots (`cap_a`, `cap_b`, ... in the stage, each optional). They put the maximum into the shape of the module, the board sees a different set of members for each count, a count of three means that two members are absent, and an absent part is a new idea in a language where a declared part is always there. The brief for this change excluded it, and the glossary says the language has no repetition construct (_Row statement_).

A board that writes the capacitors itself. That is what the library exists to prevent, and a board cannot know the count before the solver has chosen the row.

A new keyword for the count's range (`bank .count 1..8`). The default (`?=`, ADR 0051) already says "the library offers this and the board may replace it", and a range is already a value, so the maximum needs no syntax. The cost is that the solver and lowering read a field by its name, `count`, as lowering already reads `designator`, `assembly` and `package`. As first written, any part with a member of that name was a bank; a review found that wrong (a user part `LedBar` with `.count &= 8` was drawn as eight parts) and it is now only the standard `Capacitor` (see "After the review" below). If the owner wants the name kept out of the solver altogether, the same behaviour is a declared construct and this ADR is the place to say it.

A separate `CapacitorBank` part. Two definitions of one thing that differ by a field, and every board that has a plain `Capacitor` would have to know which to use. Putting `.count` on `Capacitor` costs nothing where it is unset: the solver adds a pass for each count only for a part whose `.count` is a range.

Choosing the row first and the count second (the cheapest row that works at any count). Catalog order is the whole policy (ADR 0047, audit), and a rule that picks by count would put the order of the rows second. The count is the first key because a bank of one must be what the stage did before, and fewer parts is the smaller bill of materials.

Row-major order (each row at every count before the next row). It would pick five of a small row over two of a larger one that is later in the file, which no one asks for.

A count in `Pick` only. The count is a value of the part like its capacitance, so the design, the solver's report and the editor all read it from the model, and `Pick.count` repeats it for the tools that read picks.

## Cost and limits

The search is longer for a part with a range. To keep it bounded, a candidate that failed a relation that no count can change (a rating below the rail's peak: the failing relation reads no field that is worked out from the count) is not tried again at the next count, so a rating that fits none of the rows costs one trial for each row, as before. A bank that needs the fourth count has tried the rows that failed for capacitance at one, two and three. The most a count may be is 64, whatever range a design states; the stage's own range is 8, chosen as a round number above what a datasheet asks of a 5 V rail with the catalog's parts. If the owner wants another maximum, it is one number in `std/parts/regulator.nsm`.

When no count meets the requirement, the message names the nearest as `2 x <row>` (the count and the row that came nearest). That is the one error: a relation that mentions a field of a part the pick has refused (the `==` that ties a stated `.output_count` to the capacitor's `.count` is one) is not judged again on the value the part was left with, because the part's own message says what failed.

The ripple current that a capacitor takes is not modelled (the catalog has no ripple rating), so it is not divided between the capacitors of a bank, and a bank is checked per capacitor only for what is checked for one. A placement requirement on `.stage.output_cap.place` holds for every capacitor of the bank (see "After the review" below). The placer does not keep a bank together as a row: it treats each capacitor as a decoupling capacitor, which it is, so they sit near the power pin they bridge. A bank of six on one net is dense, and the silkscreen text of the reference designators may overlap (a warning, as for any dense part). The fab check and the router ran on a bank of six without change; a bank's later capacitors have no entry in the editor's list of pins that must be connected, which only matters for a pin that is left open, and a capacitor's are not.

## What to look at

- The name `.count`, and that the solver and lowering read it by name, but only on the standard `Capacitor` ("After the review").
- `?=` for the range `1..8` in `BuckStage` and the 8.
- `.input_count` and `.output_count` as the module's way to state a count, and that a stated number is a requirement the pick must meet.
- The path `stage.output_cap#2` for the second capacitor. The choice of `#` is for layout files and the editor; `.2` would read as a member.
- `Capacitor` carrying `.count` for every capacitor, against a separate part.
- The guide's text on the bank in the section on capacitors and in the switching regulator section, and the changed test that asked for 100 uF (it is now met by a bank; the test asks for 1 mF).

## After the review (2026-11-06, proposed for owner review)

An independent review of the day's diff found three defects in this ADR's mechanism. Each is fixed and has a test that fails without the fix. The choices below were made by an agent and are the owner's to change.

**A bank is a property of the standard `Capacitor`, not of a member's name.** The solver (`Job.count`) and lowering (`bank_of`) asked only for a numeric member named `count`, so a user part with one (`.count: Number` for the segments of an LED bar, stated `&= 8`) was searched for a count and drawn as eight parts. They now ask `Model::bank_count(part)`, which answers only for an instance whose definition is in a file of the standard library (a new `std` flag on `Instance`, set from the loader's own record of which files are the library) and is named `Capacitor`. Rejected: an explicit marker in the language (`bank` before a member, or an attribute on the part): it is new syntax, which is the owner's to choose, and the std flag does the same job with no syntax. Rejected: keying on the catalog's name (`catalog capacitor`), which a user part may also use. The cost is that a user part that wants to be a bank cannot be one; the owner may want the marker after all.

**A requirement on a bank's part path holds for every capacitor.** The paths `bulk#2`, `bulk#3` are parts of the design with their own paths, and the requirements (`.place` bounds and face, `.height`, `.pin_protrusion`) were keyed by the path of the instance, so only the first capacitor was held and checked, with nothing said. Lowering now makes one requirement for each capacitor (the same bounds, face and source lines), so the placer, the check and the explanations see ordinary requirements and a finding names the capacitor (`constraint bulk#2.place.top ... body of C2`). A requirement on a module already reached every part inside it and is unchanged. A relation between the place of a bank's part and another part's becomes one relation for each pair of capacitors (all four against the other part, or all pairs if both are banks), and a note says so (`` `bulk` is a bank of 4 capacitors ``, and for a relation "holds for each pair of capacitors (N pairs)"). Choice: every capacitor, not the first. A distance from "the output capacitor" to the inductor is a statement about the bank as a whole, and a bank that only had its first capacitor near the inductor would meet the sentence in the file and not its purpose. The cost is that a bank of eight with a tight window may be unplaceable where the first alone was (not tried: the placer reports an unmet requirement for each part it concerns, as for any part). Rejected: a note that the later capacitors are not covered (the reviewer's other option): it would leave the defect in place with a warning.

**The count is part of what a pick sets, for the backjump.** The search's record of which earlier parts a failure depends on (`Coupling.columns`) held a part's columns and not its count, so a failure that read an earlier bank's count (a board value that is the sum of two banks' counts, which must reach a number) was recorded as depending on no earlier part, and the earlier bank's other counts were skipped without being tried. The result was a refusal of a board that has a solution. The count's class is now in the bank's `columns`. Skipping less is always the sound direction.

The reviewer's first example (`.a.count == .b.count` between two ranges) does not reach this: such a relation names a field of the part just chosen and is judged against the other bank's whole range at once, and is refused there as any `==` between two undecided ranges is; the test uses a board value (`.total == .a.count + .b.count`, `.total >= 5`) that the first bank does not name.
