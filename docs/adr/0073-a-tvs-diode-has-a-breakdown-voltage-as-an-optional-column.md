---
status: provisional, awaiting the owner's review (the direction was approved; the written record, the rejected alternatives and the placement of the optional-column rule are for the owner to confirm)
---

# A TVS diode has a breakdown voltage as an optional catalog column

A design could not say that a TVS diode must stay non-conducting with a margin above the highest its line reaches, because `std::parts::diode::Tvs` had the stand-off voltage and the clamping voltage and no breakdown voltage. A 12 V automotive board's input protection needed `.clamp.breakdown_voltage >= .vin.peak * 1.1` and carried the column in a project part, `TvsDiodeBr`, whose column was required.

## Decision

`Tvs` has a field `.breakdown_voltage: Voltage`, the minimum breakdown voltage VBR: between the stand-off voltage and it the diode leaks a little, below it the diode does not conduct. It is listed in the `catalog tvs` statement of the part, as an optional column in the sense of ADR 0055 and ADR 0058.

- A file of the `tvs` catalog may leave the column out of its first line, and a row may leave its cell empty. A project's `tvs` file written before the column stays valid.
- A design that does not mention the field is offered every row, as before, and has no new check. `Tvs` states no relation on it, so the standard `.bias <= .standoff_voltage` and `.clamping_voltage <= .protected_limit` are as they were.
- A design that states it (`.d.breakdown_voltage >= .input.peak * 1.1`, or `&=`) is offered only the rows that record one, as for `.dielectric` and `.rms_current`. A row with an empty cell fits no such design (ADR 0047, ADR 0058, "asked").
- The standard `tvs.csv` has the column and every cell is empty. The `notes` of the rows carry the JLCPCB record's figures, but no datasheet table was read for a VBR, and a value enters a catalog only with its source (`docs/DESIGN-RULES.md`). `data/jlcpcb/README.md` says so. So a design that states `.breakdown_voltage` is refused with the standard rows (the usual "the `tvs` catalog has nothing for") until a project's catalog gives rows with the column.

How it was built (before ADR 0078). The optional columns of ADR 0055 were the ones of `Standard` in `crates/netsmith-solve/src/standard.rs`, the four generics whose catalogs have synthesized parts. `Tvs` has none, so `is_optional` in `crates/netsmith-solve/src/table.rs` also reads a short list of the optional columns of the other standard catalogs (`OPTIONAL_OF_KINDS`), which holds `tvs` and `breakdown_voltage` and nothing else. A later optional column of another standard kind is one more entry there.

*Amended by ADR 0078:* the list is gone. The `catalog tvs` statement marks `.breakdown_voltage optional`, and any part, standard or a project's, marks its own columns the same way.

## What was rejected

A required column, as the project part `TvsDiodeBr` had it. Every standard row would need a number that has no recorded source, or a design that never mentions VBR would be refused every standard row. The optional column is what ADR 0055 did for the same reason.

A relation in `Tvs`, such as `.standoff_voltage <= .breakdown_voltage`. It is always true of a real part, but the solver counts a field as asked for as soon as a relation mentions it (`asked`, `crates/netsmith-solve/src/solve.rs`), so the relation would make every standard row unfit for every TVS diode. ADR 0055 refused the same relation for the inductor's heating current.

A separate generic or part for the diode with a breakdown column (a `TvsBr`). Two spellings of one thing; a design that needs the margin would have to know to pick it, and a project would copy the whole part, as a real project did.

Numbers entered from memory or from the JLCPCB strings in `notes`, to make the standard rows usable for a margin. They are not read from a datasheet table with a stated condition (VBR is given at a test current, minimum and maximum), and nothing is entered without it.

## Cost and limits

The check is the design's own: the library does not say which margin is enough, and a design with a TVS diode and no `.breakdown_voltage` line has no margin check, as before. With the standard rows only, the line cannot be satisfied, so a board that wants it needs a project catalog file with VBR (add rows with the `netsmith-dev-add-part` steps). The generic series have no TVS diode, so nothing changes there. The record does not say at which test current the minimum VBR is given; that goes in the row's `notes`.

## What to look at

- The column being optional, and not required as in the project part.
- The place of the rule for a standard kind that `Standard` does not describe: first a list in `table.rs` (`OPTIONAL_OF_KINDS`), now the marker in the `catalog` statement (ADR 0078).
- The standard rows leaving the cell empty, so the line in a design is refused until a project has rows.
- No relation between `.standoff_voltage` and `.breakdown_voltage`.
