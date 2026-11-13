---
status: proposed
---

# Packages share a pinout, and a pin row states its absolute maximum

The owner decided on 2026-11-13 (issue #630, epic #638) two shorter forms for a part's pinouts (ADR 0016). Before, a part in two packages with the same pads wrote the same pinout twice (`LinearRegulator`, `LevelTranslator`), and a pin that is no `Logic` line stated its absolute maximum through a `Logic` joined to it inside the part, five lines for each pin (the `Lt8641`'s `.en_uv_limit`, `.tr_ss_limit` and `.sync_mode_limit`, ADR 0095):

```text
.en_uv_limit: Logic
.en_uv_limit.reference ~ .vin
.en_uv_limit.reference.current &= 0A
.en_uv_limit.line ~ .en_uv
.en_uv_limit.line.abs_max &= 65V
```

## Decision

- **A shared pinout.** A header lists one or more packages, `pins "SOT-23" "SOT-223" { ... }`, and the table is the table of each (`pins` in `crates/netsmith-check/src/pins.rs` makes one model `PinTable` per package, so nothing after the checker changed). A package is named once in a header (`package "A" is listed twice in this `pins` header`) and in one table of the part. The rule of ADR 0016 holds: one unnamed table, or tables for packages.
- **The absolute maximum in the row.** A row may end with `abs_max VALUE`, optionally `from .pin`: `.en_uv input "EN/UV" "14" abs_max 65V`. It narrows the pin's `.abs_max` (`&=`), the field every pin has, once for the part however many tables state it. The value is any expression a narrowing takes. `from` names a pin of the part, not the pin itself. The pinouts of a part give a pin one limit, as they give it one kind and one name. `abs_max` with no value is a parse error at the word.
- **The reference.** The solver measures the worst-case voltage of the pin's net from a reference pin (`absolute_maximums` in `crates/netsmith-solve/src/levels.rs`). It is, in order: the ground of the pin's `Logic`, or of its bus's `levels`, the `neg` of their reference rail, as before (`from` is refused there: the interface already says it); else the row's `from`; else **the part's ground, which is the one row of the table that is the `neg` of a rail** (a `neg` pin of an interface that also has a `pos` pin). A table with no such row, or with several, needs `from`, and the error says so. The model keeps the reference in the row (`RowLimit` in `crates/netsmith-check/src/model.rs`, read by `Model::limit_reference`).

The default is **proposed, for the owner**. It is what std means today: the `Lt8641`'s wrappers took their ground from `.vin`, whose `neg` is `GND`, the one rail ground of its table, so its three limits are judged against the same net with the same numbers. The only change a board sees is the field a refusal names: `u.sync_mode.abs_max` in place of `u.sync_mode_limit.line.abs_max`. The `Logic` and its zero-current reference rail are gone from the part, so no balance or DC relation changed.

## Rejected

- **A part-level ground member** (`.ground ~ .vin.neg`, read by every row): a new reserved member name the checker recognises (epic #638 lists those as a problem), and one line more in every part with one ground.
- **The pin named `GND` as the default**: a reference found by a datasheet name, which differs from part to part (`VSS`, `AGND`, `PGND`).
- **`from` always required**: correct but long, and every one of today's limits would write `from .vin.neg`.
- **A separate statement**, `.en_uv abs_max 65V`: a second place for a fact of the datasheet's pinout, and the owner chose the row.
- **A shared pinout as a copy written by the printer or by a `like "SOT-23"` reference**: a second spelling of the same pinout.

## Cost

A pin's absolute maximum has two spellings: the row, and a narrowing of `.abs_max`. The narrowing remains for a `Logic` line and a bus pin (`.a.line.abs_max &= .vcc.voltage + 0.5V`, `.spi.sclk.abs_max &= 2.1V`), whose ground the interface gives; on a plain pin it has no ground and stays a `not evaluated` note. `abs_min` has no row form. A pin's limit is the same in every package. The word `abs_max` ends a row's list of functions, so no function can be called `abs_max`.

## Left open

- The default reference (above), for the owner.
- Whether the rows of `Logic` lines and bus pins should state their limits in the row as well, and the narrowing of `.abs_max` on a table pin be refused, so a limit has one place; and whether `abs_min` should get a row form. Neither is built.
- `PG` and `FB` of the `Lt8641` still state no limit, for the solver's reasons in its doc comment (ADR 0095), which a reference in the row does not change.
