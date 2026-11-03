---
status: accepted (user review 2026-11-02)
---

# Orders: which boards go into a fabrication order, and how many

Amends ADR 0011 (the manifest) and ADR 0020 item 6 (panels stay deferred, see below).

## Problem

A project holds many boards (ADR 0010, `docs/PROJECT-LAYOUT.md`): a development kit of several boards, a production unit. `netsmith fab` made a package for every board, and nothing said which boards go into one order to the board house, or how many of each. The package was the files for one board.

## Decision

An order is a named table of `netsmith.toml`, and `netsmith fab --order <name>` makes it. There is one way to choose boards: a named order. A plain `netsmith fab` with no order still makes every board of the project, and `netsmith fab <file.nsm>` still makes one file's boards, as before.

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10
```

- **A key is a board as its output folder names it**: `Board`, or `System/member` for a board of a system. A key that is a system's name (`DevKit`) means all its members, each with that quantity. A name that is neither is an error naming the manifest line and listing the boards that exist. Naming one board twice (alone, and through its system) is an error naming both lines.
- **The value is the quantity**: a whole number of at least 1. 0 is an error that says to leave the board out. There is no default quantity, so the order says how many of each, and a reader of the order sees every number.
- **The order's name** is letters, digits, `-` and `_` (it is a folder name) and is unique, because TOML does not allow a table twice; the TOML crate reports the second with its line. An order with no boards is an error.
- **`--order` with no name lists the orders**, with their boards and quantities. `--order` takes no file (an order names its boards in the manifest). `--out` and `--route-timeout` work as for `fab`.
- The tables are read by the `toml` crate through serde, with `deny_unknown_fields` on the file and on each order, so any other key refuses with the crate's line and column. `Manifest` keeps its `[project]` fields and gains `orders`, read from the sibling `[order.*]` tables; `render` writes them back. Mistakes the types cannot say (a quantity of 0, a board name of the wrong form) are `netsmith.toml:LINE: order `name`: ...`.

### The spelling

Three were weighed.

1. `boards = ["DevKit/carrier", "Unit"]` and `quantity = { "DevKit/carrier" = 2, "Unit" = 10 }` with a default of 1. Rejected: a board is named in two places, which allows a quantity for a board that is not in the order (a new error) and a duplicate in the list (another), and a default of 1 is a second way to say "1".
2. An array of tables, `[[order.devkit.board]] name = "..." quantity = 2`. Rejected: it is a block of three lines for each board, and the name still needs the same uniqueness check.
3. **A table from board to quantity** (chosen). Each board is named once, TOML itself refuses a duplicate key with its line, there is nothing to keep in step, and a line reads "this board, this many". The cost is that the order has no room for other per-board fields. None is wanted; if one is, that order's table becomes a table of tables and this form is replaced, not extended (ADR 0001).

## What `fab --order devkit` does

It builds every top-level file once (the checker and solver, no KiCad) to learn the project's boards, resolves the order against them, then runs the usual `fab` for the boards named only (`FabOptions::only`). Each board writes its package as `fab` does, under `<fab>/orders/<name>/<board folder>/`: `fab/orders/devkit/DevKit/carrier/`, `fab/orders/devkit/Unit/`. We chose `orders/<name>/` over `fab/<name>/` so an order's name can never be the folder of a board; a board or system called `orders` is refused when an order runs, naming it. A folder of the same order from an earlier run is removed first (only if it holds an `order.txt`, so nothing else is deleted), so a board dropped from the order does not linger. If any board fails, nothing of the order's summary is written and the problems are all reported.

Beside the packages the order writes:

- `order.txt`, for a person: the number of board designs and of boards; for each board its copies, copper layers, size in mm, thickness (or "thickness open"), the finish (`finish not stated`: the process definitions do not carry a surface finish, and netsmith will not say one it does not know), the parts the board house assembles and on how many BOM lines, the hand-solder parts (per board and in all); the combined bill of materials; "before you order", each board's `fab-notes.txt` lines; and the total of unverified items.
- `order-bom.csv`: `Comment,Footprint,LCSC Part #,Quantity,Used on`, one row for each LCSC number. The quantity is the copies of each board times the parts of that number on one board, summed over the boards: **no spares or overage**. How many spares to buy is a decision about the assembler's minimums and the user's risk, not a fact of the design. `Used on` reads `2 x 4 DevKit/carrier; 10 x 1 Unit` (copies x parts on one board) so a number can be checked by hand. The file is named apart from each board's `bom.csv` because that one is the upload JLCPCB reads for one board and has other columns.
- The unverified total counts the parts whose rotation or side `fab-notes.txt` says to check in the order preview (`Package::unverified`), once for each board and not once for each copy, because the check is of the design.

## The editor

The Build panel gains an Order section: a select of the manifest's orders, the picked order's boards with quantities, and a "Make order" button that runs the same function (`order::fab_order`) with the stages reported as for `fab`. The page shows the result, the folder, links to `order.txt` and `order-bom.csv`, and the text of `order.txt`. The server has `GET /api/orders`, `POST /api/order?name=` and `GET /api/order/download?name=&file=` (`summary` or `bom`); the types are in `api.rs` and the generated `api.ts` (ADR 0019). Orders are read when the editor starts, like the rest of the manifest.

## Panels: still deferred

_Update: ADR 0040 decided the questions below and built stage P0 (identical boards on a V-cut grid); an order key may name a panel and its quantity counts panels. What follows is the text as it stood._

ADR 0020 item 6 deferred panels, and the user has not asked for them. An order is not a panel: it is a list of separate packages to order together. A panel is one outline made from several boards. The open questions, for when it is built:

- **How a panel references boards.** By an order's name (`panel p { .order: devkit }`), so the quantities are written once, or by listing boards itself. A panel of one board repeated four times is a quantity inside the panel, not four packages, so the order's per-board quantity and a panel's copies would have to be told apart (order quantity = panels x boards in a panel?).
- **Quantity inside a panel.** Whether a board can appear several times at different rotations, and whether the order's quantity of a board in a panel is a count of boards or of panels.
- **Frame, tabs and rails.** V-score or routed tabs with mouse bites; rail width; where tooling holes and fiducials go and what the board house needs of them (to be read and sourced, not remembered); the spacing between boards.
- **Where it lives.** ADR 0020 recommends a top-level `panel` definition separate from `system`, so each board stays stand-alone. It needs its own package writer (merged outline, routed slots, rails) and its own checks (does the board house accept the panel size and the number of designs in it).
- **What `fab` writes for a panel**, and whether a panel of a project's order replaces the per-board packages or adds one.

`panel` is not implemented and has no syntax here.

## Consequences

- The `fab` output of a project gains `orders/`. The order's boards are written again by the order; a plain `netsmith fab` does not touch `orders/`.
- Every board of an order is routed and checked again when the order runs, even if the same board was made by a plain `fab`. A cache of finished packages is a later optimization and is not needed to be correct.
- `fab_with_options` gained `only`, and `Fabricated` gained `facts`, the numbers of the summary. `netsmith-fab`'s `Package` gained `unverified`, the designators of the parts that need checking.
