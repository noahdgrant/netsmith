---
status: provisional, awaiting the owner's review (the owner chose a marker in the `catalog` statement; its exact place, what it refuses and the fate of the standard kinds' rule were settled by an agent, each with the alternatives it rejected)
---

# A catalog column is marked optional in the catalog statement

ADR 0073 gave `Tvs` an optional `.breakdown_voltage` and needed a Rust list for it, `OPTIONAL_OF_KINDS` in `crates/netsmith-solve/src/table.rs`. Whether a column may be empty was a fact about a catalog name, held in Rust: the four generics with synthesized parts (`Standard`, ADR 0055) and that one entry. A project's own part could not declare an optional column of its own catalog at all, so a real board that needed one had to give every row a value. ADR 0073 named the placement as the owner's to change, and issue #178 asked for the marker in the language.

## Decision

A column of a `catalog` statement may end with the word `optional`:

```nsm-fragment
catalog tvs { .standoff_voltage, .clamping_voltage, .peak_pulse_current, .package, .breakdown_voltage optional }
```

An optional column has the meaning that ADR 0055, ADR 0058 and ADR 0073 gave the columns they called optional, and nothing new:

- A file of the catalog may leave the column out of its first line, and a row may leave its cell empty.
- A row with an empty cell is offered to a design that says nothing about the field. A design says something by narrowing it, by a relation that names it, by a `~` that joins it or by a balance of currents that sums it (`asked`, `crates/netsmith-solve/src/solve.rs`); such a design is offered only the rows that record a value.
- Any column that is not marked is as it was: an empty cell takes the row out of every design, and the file must have the column.

`OPTIONAL_OF_KINDS` is gone. `std/parts/diode.nsm` marks `.breakdown_voltage optional` in `catalog tvs`, and a project's part marks a column of its own catalog the same way.

The spelling is the one the language already has. A member says `.max_inductor_dcr: Resistance optional` (ADR 0074), a `pins` row ends with `optional` (ADR 0016) and a line of `lines` the same. In each the word comes last on the line of the thing it qualifies and means "may be absent". Here it follows the column, `.breakdown_voltage optional`, which is the order a pins row has (`.boot0 input "BOOT0" "31" optional`). The parser reads it as a plain word: `.optional` is still a column's name, because a column starts with its leading dot, and a `catalog` with `optional` on its own line is a syntax error as before. A column cannot be marked twice, and nothing follows the word but `,` or `}`.

## What the checker refuses

- A column marked `optional` that the part does not have: the message is the one for any unknown column, which names the path (`.missing`).
- The key of a keyed catalog (`catalog mcu by .mpn { .mpn optional }`): the key names the one row a design chooses, so a row without it could not be named. The message names the column and the catalog.
- `catalog` outside a part is refused as before, with or without the marker.

The checker records the marked columns in the model's `Catalog` (`optional`, a subset of `fields`). The solver reads them there, so the rule is the part's own and does not depend on the catalog's name.

## What happens to the standard kinds

The columns that were optional before the marker, `temperature_coefficient` of `resistor`, `dielectric` of `capacitor` and `rms_current` of `inductor`, are marked `optional` in `std/parts/passive.nsm`, so the library says in `.nsm` what the guide says. The rule that `Standard` keeps (a synthesized part leaves exactly those columns empty, `Fill::Empty`) stays in Rust, because synthesis is a different job. A test of `standard.rs` compares the two: the columns the standard statements mark must equal the ones `Standard` leaves empty, so the marker and the rule cannot drift.

`is_optional` still answers yes for those three columns under the standard catalog's name, even when a part lists them without the marker. This is for compatibility only: a project's part with `catalog resistor { .resistance, .max_power, .package, .temperature_coefficient }` built before this ADR with the column optional, and it still does. Dropping the name-based answer would break such a file, so it is the owner's to remove later, with a deprecation, if the marker is to be the only way.

## What was rejected

- **A separate statement or line, `optional .breakdown_voltage`** (the form the issue gave as an example). It would be a second place to list columns, so a column could be listed in the statement and marked elsewhere, and it reads as a new statement and not as a modifier. The marker beside the column keeps the one list and puts the word where `pins` rows and members put it.
- **A leading word, `optional .breakdown_voltage`, inside the braces.** The same marker in another order. `pins` rows and members both put the word last, and a word before a path would be the first place in the language where it came first.
- **A flag on the catalog, `catalog tvs optional { ... }`, or a list `optional { ... }` after the columns.** Both separate the word from the column it qualifies, and the first would need a way to say which.
- **Marking the member instead (`.breakdown_voltage: Voltage optional`, ADR 0074).** The two meanings differ. A member marked `optional` skips the relations that read it when nothing states it; a catalog column marked `optional` lets a row be empty. A part may want one without the other (a column the rows may omit but a relation always checks, or the reverse), and the member's marker is refused in the definition that states it. One word on two things would also let a reader take one for the other.
- **Deriving it from the rows**, treating a column as optional when some row of a file leaves it empty. The file is data and the part is the contract; the same file would be read differently as rows are added, and a typo that blanks a rating would silently make the row unfit instead of an error.
- **Removing the name-based rule of `Standard` at once.** It is a break of the files that listed those columns without the marker. See above.

## Cost and limits

- The marker is per part. Two parts that share a catalog name (a project's `Resistor` and the standard one) each say which columns are optional, and a file is read once for each part's statement, so the same file could be read with different columns allowed. A part whose statement does not mark a column the file leaves out gets the error "there is no `x` column, which `catalog y` lists".
- The cell of an optional column that a row leaves empty means "no value", as before. The marker does not say how a design should treat a part with no value; that stays the design's (a bound refuses the row).
- Nothing about price, stock or the fixed columns changes.
- The guide's list of the columns that may be empty (`docs/LANGUAGE.md`, catalog files) is now "the columns the statement marks". The glossary has a term for it.

## What to look at

- The place of the word: after the column, as in a `pins` row and a member, and not a line of its own.
- That the marker is a modifier of the column and the one list is kept.
- The refusals: an unknown column, and the key.
- `is_optional` still answering by the standard catalog's name for the three passive columns (compatibility), and the test that keeps the two sources equal.
- Whether the three passive columns should stay in Rust at all, once no project relies on listing them without the marker.
