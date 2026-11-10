---
status: accepted
---

# Orders: which boards go into a fabrication order, and how many

Amends ADR 0011 (the manifest). The status line it had said "accepted (user review 2026-11-02)". Meets test 1 (the `[order.*]` table of `netsmith.toml` is a file format projects write) and test 3 (one way to choose boards). The text before the cut is `git show 0eaf326:docs/adr/0035-orders.md`. What `fab --order` does, the files it writes and the editor's part are the header of `crates/netsmith/src/order.rs`; the user's view is `docs/LANGUAGE.md` ("Orders").

**Problem.** A project holds many boards (ADR 0010): a development kit of several boards, a production unit. `netsmith fab` made a package for every board, and nothing said which boards go into one order to the board house, or how many of each.

**Decision.** An order is a named table of `netsmith.toml`, and `netsmith fab --order <name>` makes it. There is one way to choose boards: a named order. A plain `netsmith fab` still makes every board, and `netsmith fab <file.nsm>` one file's boards.

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10
```

A key is a board as its output folder names it (`Board`, or `System/member`), or a system's name for all its members. The value is the quantity, a whole number of at least 1, with no default, so a reader of the order sees every number. A name that is not a board, a board named twice, and an order with no boards are errors that name the manifest line. The packages go under `<fab>/orders/<name>/`, so that an order's name can never be the folder of a board, with an `order.txt` for a person and an `order-bom.csv` of the whole order. The bill has no spares or overage: how many to buy is a decision about the assembler's minimums and the user's risk, not a fact of the design.

**Rejected, the spelling.** `boards = [...]` with `quantity = { ... }` and a default of 1: a board is named in two places, which allows a quantity for a board not in the order and a duplicate in the list, and a default of 1 is a second way to say "1". An array of tables, `[[order.devkit.board]] name = "..." quantity = 2`: three lines for each board, and the name still needs the same uniqueness check. Chosen: a table from board to quantity. Each board is named once, TOML itself refuses a duplicate key with its line, and a line reads "this board, this many".

**The cost.** An order has no room for other per-board fields. None is wanted; if one is, that order's table becomes a table of tables and this form is replaced, not extended (ADR 0001). The `fab` output of a project gains `orders/`. Every board of an order is routed and checked again when the order runs, even if a plain `fab` made it. `fab_with_options` gained `only` and `Fabricated` gained `facts`.

**Panels.** ADR 0020 item 6 deferred panels, and an order is not a panel: it is a list of separate packages to order together. ADR 0040 decided the panel questions this ADR listed and built stage P0: an order key may name a panel, and its quantity then counts panels.

**Left open.** A cache of finished packages, which is an optimisation and not needed to be correct. A finish for the surface (`finish not stated` in `order.txt`: the process definitions do not carry one, and netsmith will not say one it does not know).
