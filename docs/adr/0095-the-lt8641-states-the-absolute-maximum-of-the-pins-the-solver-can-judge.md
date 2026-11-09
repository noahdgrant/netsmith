---
status: provisional, awaiting the owner's review (the engineer chose to state the limits as `Logic` lines joined inside the part, and to leave `PG` and `FB` unstated; each alternative is under "Choices for the owner")
---

# The LT8641 states the absolute maximum of the pins the solver can judge

## The problem

The standard `Lt8641` stated an absolute maximum for two pins only: `VIN` (`.vin_abs_max`, compared with the rail's `.peak`) and `BIAS` (`.bias_abs_max`, compared with a voltage the board states). The datasheet's Absolute Maximum Ratings (LT8641 Rev. C, p.2, read again from the PDF that the part cites, SHA-256 `09763bcb...965a43`) give seven:

| pin | limit |
| --- | --- |
| VIN, EN/UV | 65 V |
| PG | 42 V |
| BIAS | 25 V |
| FB, TR/SS | 4 V |
| SYNC/MODE | 6 V |

The table does not say what the voltages are measured from. The part takes them from `GND`, the usual reading of such a table. No other pin (`INTVCC`, `BST`, `RT`, `SW`) has a row, so none states one.

A board that tied `SYNC/MODE` or `TR/SS` to a 9 V to 16 V input built with no finding (issue #290): the pins are not `Logic` lines, and the solver compares an absolute maximum only for a pin that is one (ADR 0046).

## Decision

For `EN/UV`, `TR/SS` and `SYNC/MODE` the part has a `Logic` each, `.en_uv_limit`, `.tr_ss_limit` and `.sync_mode_limit`:

```nsm-fragment
.sync_mode_limit: Logic
.sync_mode_limit.reference ~ .vin
.sync_mode_limit.reference.current &= 0A
.sync_mode_limit.line ~ .sync_mode
.sync_mode_limit.line.abs_max &= 6V
```

The `.line` is joined to the pin inside the part and has no pad. The pin keeps its name, its kind and its place in the pin table, so no board changes (`.u.sync_mode ~ .input.neg` stays what it was), and the solver's existing check judges the pin's net against the limit in the worst case, from the part's ground. The finding names the net, the limit and the field: `net VIN reaches 16V above its ground GND (rail input) but u.sync_mode_limit.line.abs_max is 6V`.

One change in the solver's check goes with it: a pin that nothing outside its own part is joined to is skipped. `TR/SS` and `SYNC/MODE` may float (Pin Functions, p.10 and p.11), and without this each build of such a board would carry a note, `nothing sets its voltage`, for a pin that has no voltage to judge. A pin with a capacitor on it only (the usual soft start) still gets that note, which is true: a capacitor sets no DC voltage.

`PG` and `FB` have limits and the part states none:

- `PG` is an open-drain output. The solver says how far an output swings only for the `line` of a `Logic`, so every net with `PG` on it is "not evaluated" whatever its limit: a `.pg_limit` would be a note on every board that wires the pin, and never a finding. Making `PG` a `Logic` line is a change of the pin's interface (`.u.pg.line`) and of what an open-drain `Logic` swings.
- `FB` is the tap of the stage's divider. When the DC analysis does not solve the output side (on the invented board of issue #290 with a 5 V stage it solved the input island only; why the output side is left out was not looked into, `BIAS` on the output net is one candidate, ADR 0076, amendment 11), the solver reads the net as the hull of its two resistors, from ground to the output, so a 4 V limit refuses every output above 4 V, a 5 V stage included (tried on an invented board: `net net-stage.divider.rbottom.a reaches 5.2V ... u.fb_limit.line.abs_max is 4V`). The divider holds the pin at `.buck.vref`, under 0.83 V, by its own relation, so a board built as the stage builds it never nears 4 V.

## What was rejected

- **A voltage the board states, like `.bias_voltage`.** A board that forgets to state it is not judged, which is the false pass this issue is about, and it asks the board to say what it wired.
- **Changing the pins to `Logic` lines** (`.u.sync_mode.line ~ ...`). It breaks every board that wires the part (the guide's example, the tests, the LCSC-ordered boards of a project), for a limit that the join inside the part states with no change.
- **A pin-level limit with its own ground** (a `Pin` member that names the rail it is measured against, so any pin of any part could state one). It is the general fix for `PG` and `FB` too, and a change to the language and the checker (`abs_max` is a field of every pin and is judged only through a `Logic`): the owner's.
- **A note for a pin left to float.** It is a note on each build of a legal board.
- **Stating `FB` at 4 V anyway.** It refuses the 5 V stage, the most common one.

## Cost

Three `Logic` members of the part, three lines each, and one skip in `absolute_maximums` (`crates/netsmith-solve/src/levels.rs`). They have no pad and appear in no export. A board with `SYNC/MODE` or `TR/SS` on a rail above their limits is refused; every board of the repository and the real 12 V automotive board (which has its own part) build as before.

## Choices for the owner

1. Whether the limits are stated as a `Logic` joined inside the part (here), as a voltage the board states (`BIAS`'s way), or through a general pin-level limit (the third rejected option, which would also give `PG` and `FB` theirs).
2. `PG` and `FB` left unstated until the solver can judge them: the alternative is a limit that is only a note for `PG`, and a false finding above 4 V for `FB`.
3. The reading of the table as voltages from `GND` (the datasheet does not say).
4. The skip of a pin nothing else is joined to, for every part and not only this one (an optional input that is left floating has no voltage to judge; a required one is already an error).
