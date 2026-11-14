---
status: accepted
---

# A pin's absolute limits are written only in its pin row

ADR 0124 gave a pin row a form for the absolute maximum (`abs_max 65V [from .pin]`) and kept a second spelling for the pins whose ground an interface gives: the narrowing of the pin's field, `.a.line.abs_max &= 6.5V` for a `Logic` line, `.spi.sclk.abs_max &= 2.1V` for a bus pin. Issue #695 (epic #638) asked to end the second spelling.

## Decision

- **One spelling of an absolute maximum, the row.** The owner decided this on 2026-11-13 (issue #695). A `Logic` line and a bus pin write `abs_max` at the end of their row, `.a.line input "A" "2" abs_max 6.5V`, and measure from their interface's ground, as ADR 0124 already says (`from` is refused there). The value is any expression a narrowing takes (`abs_max .vcc.voltage + 0.5V`). The std parts `Sn74lvc1g04` and `LevelTranslator` moved their limits into their rows.
- **The statement is an error.** A statement that narrows (`&=`) or defaults (`?=`) the `.abs_min` or the `.abs_max` of a pin is refused wherever it stands, in a part or in a board's narrowing of a part, with the row that says it: "a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.a.line` in the `pins` table of its part with `abs_max 6.5V`" (`narrow_statement` in `crates/netsmith-check/src/lower.rs`). The checker still narrows the fields itself, once for the part, from the row. A relation (`<=`, `>=`) on the field is not a statement of it and stays.
- **`abs_min` gets the same row form.** Decided by the orchestrating agent while the owner was away (2026-11-13), and accepted on that authority. A row ends with `[abs_min VALUE] [abs_max VALUE] [from .pin]`, either limit alone or both, in that order only (`abs_max 6V abs_min 0V` is a parse error that says so). One `from` serves both, since a pin has one ground. The word `abs_min` ends the list of functions as `abs_max` does, so no function can be called `abs_min`. The tables of a part must agree on the whole text (`one set of absolute limits in every package`). The model keeps one `RowLimit` per row (`PinRow::limits`) with the text as written and the reference.

- **A generic's row limit is a contract every provider meets.** Also decided by the orchestrating agent while the owner was away. A row of a generic may state limits, after the kind, with no name and no pads (`.in input abs_max 6V`), and a generic that is its own provider (`LevelTranslator`) has them in its full rows. A provider whose row for the pin states no `abs_max` (or no `abs_min`) has the generic's, written into its row before the table is read (`with_inherited_limits` in `crates/netsmith-check/src/pins.rs`), so its ground, its package agreement and its narrowing are those of a row it wrote. A provider that states its own must be at least as strict: `abs_max` no higher, `abs_min` no lower. Numbers are compared at the provider's row, and a looser one is an error that names the generic's limit; a value that waits for a pick (`abs_max .a_abs_max`) is the provider's own member, and the provider's number becomes the relation `abs_max <= .a_abs_max` the solver holds. Losing a provider's input limit silently was a safety regression, which is why this is not left open.

## Rejected

- **`abs_min` stays a statement.** It keeps one spelling of each field, but a `Logic` line then has its limits in two places, the maximum in the row and the minimum in a statement, which is the split the owner ended for the maximum. A plain pin could never state a minimum: its ground is only known from the row.
- **A separate clause for each, each with its own `from`.** Two references for one pin's net: a pin has one ground, and the solver measures both limits from it.
- **Either order of the two words.** A second spelling of the same row.
- **Refusing the statement only on a pin of a table.** A pin with no row has no limit to state, and a board that narrows a part's limit would be a third place for a datasheet fact.

## Cost

Every part that stated a limit by narrowing needs a pin table, and a limit that was one line is now the end of a row. The tests and the guide examples that stated one moved to rows. A provider's pin table is read as kinds plus limits, where it was kinds only (ADR 0068): a generic's limit reaches every provider, as its statements did. A pin's limit is the same in every package. The std doc comments and the guide say so.

This replaces the Cost and Left open paragraphs of ADR 0124, whose first decision, the shared pinout and the row's `abs_max`, stands.

## Left open

- `PG` and `FB` of the `Lt8641` still state no limit, for the solver's reasons in its doc comment (ADR 0095).
