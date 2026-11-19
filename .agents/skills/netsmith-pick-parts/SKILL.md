---
name: netsmith-pick-parts
description: Find out why netsmith picked a catalog part, keep a pick, choose another row by hand, or add a part the catalogs lack to the project. Use when a user asks "why this part", wants a pick held or changed, when `board build` or `fab` says no row fits, or when a part number must be added or checked against a supplier.
---

# Pick parts

The solver picks, for each generic part, the first row of its catalog that fits and passes every relation (the guide's `docs/netsmith/language/12-catalogs-and-how-the-solver-picks-parts.md`; the pick lock and hand picks are in `23-the-shape-of-a-board.md`, the commands in `27-commands.md`). `netsmith-read-errors` has the messages and `netsmith-prepare-fab-order` the order checks; this skill is what to do about the picks themselves.

## What stays with the person

- A part number goes into a catalog only with a source: the supplier's record, and the datasheet for every rating the row states. Never write an LCSC or manufacturer's number from memory, and never guess a rating. If you cannot fetch the evidence, say so and add nothing.
- Which part a design should use when several fit is the user's choice. List the rows that work and what differs (price, library type, rating margin); do not pick for them.
- A change to what a stated value or relation means is a language decision: ask first (`AGENTS.md`).
- Say what each command printed. Do not edit `<stem>.lock.toml` by hand: `project lock` writes it. The one line a person adds is `held = "reason"` to a part's entry (below), and only when the user asks to hold that part.

## Why did it pick this one

1. `netsmith board explain reg.inductor [file.nsm]` prints the row the part has, what held it, the rows passed over and which relation refused each (`file:line`), then the values the solve left (`--all-rows` lists every row, `--board NAME` picks one board). A module's path covers every part inside it; a path nothing has lists the parts that have rows.
2. For a program, or for every pick at once: `netsmith board build --emit-solved solved.json` (the `picks` of each board; a board that fails is written too, with `unpicked` and the rows its first turn passed over).
3. A part the pick lock holds has no rows passed over, because no other row was tried. To see what the solver would take now, `netsmith project lock --check` prints the diff and writes nothing.
4. A build that fails with "no row" names the closest row and the relation it failed: change the design's value or rating, or add a row (below), not the relation.

## Keep a pick: the lock

`build` and `fab` write `<stem>.lock.toml` beside a source when it does not exist and never rewrite it; commit it. A change to a catalog, std or the solver cannot then move a pick unseen.

- `netsmith project lock --check` exits 1 and prints the diff when the lock is stale or missing, 0 when current (rows held on purpose are listed, below). In CI run it with `netsmith board build --locked`, which fails when a pick would change.
- `netsmith project lock --update` takes the solver's picks now and prints each change as `board: part: old -> new`. Read the diff to the user before it is committed. `--update --part r1,stage.cap` refreshes only those parts, `--update --except u1` all but those; the others stay on the lock's row until an update that does not name them. After a release added a part to a built-in catalog that the project's own catalog has, the build says "a part is in a catalog once" and names the built-in row: remove the project's row from its catalog file, then `project lock --update` sets the lock's `row` to the built-in row and changes no other row.
- A row that no longer passes stops the build and says the lock held it and no other row was tried. Change the design or the catalog so it fits, or run `--update` and review the new row.
- A pick held on purpose, one row kept even though the solver would take another, is `held = "reason"` in that part's entry of `<stem>.lock.toml` (ask the user for the reason and write theirs). `project lock --check` then passes on that row and lists it once as `held` with the reason (text; `held` in `--format json`), `--update` keeps the row and the key unless `--update --part <path>` names the part, and a held row that no longer passes is an error that says to remove the `held` line or name the part. Without `held` the same row is not current and `--check` exits 1.
- `netsmith project lock --check --except r1,stage.cap` judges every part but those and lists them as not judged (`not_judged` in JSON): for a CI that must not fail on parts a person is not ready to move. Prefer `held` for a hold that should stay, as it is in the committed file with its reason.
- A decision that is a requirement of the design (this part must be this row for every board) is a hand pick in source, not a held lock entry; `held` is a decision about the lock, and the source does not say it.

## Choose a row by hand

- A hand pick is a narrowing in source (ADR 0122): `.r1: Resistor { .lcsc &= "C23151" }`, or `.mpn &=`; inside a module from the board, `.stage.output_cap.lcsc &= "C307331"`. The solver then offers that row only, judges it by every relation, and says when none passes or the catalog lacks the number. A hand pick wins over the lock: build, then `project lock --update` to record it.
- For a generic part no row stands for (`fab` refuses it), `netsmith board swap R1` lists the nearest rows with what the board's checks say for each and the change that chooses it. `netsmith board swap R1 --apply C25094` (name, LCSC number or list number) writes it, as the editor's Swap does. Ask the user which row.
- A part that states `.variants` (order codes and their features, issue #1311) takes its `.mpn` from what the design needs: `.u.needs &= GNSS` leaves the codes with that feature, and the build asks for a need or a `.mpn &=` when several are left. To change the code, change the need, not the lock; a code named with `.mpn &=` that lacks a need is refused. Through a use of a generic that declares `.needs` (`.sw: LoadSwitch`, issue #1402), a need also picks the provider: one whose `.variants` table has no code with it is passed over, and the pick report says why.
- A second part for the same footprint (another maker's module) is an `alternate .u: ModuleB` beside `.u` (issue #1312), not a second pick: the board stays drawn and locked with `.u`, the build checks it with the alternate fitted too, and `board bom` and `fab-notes.txt` list the alternate not fitted (DNP). Its row is picked fresh each build and is not in the lock.
- To prefer a row for every generic part without changing the design's picks, the project's `preferred.csv` lists `lcsc` or `mpn` (the chapter on projects, "Preferred parts"); it changes resolution only.

## Add a part the catalogs lack

The solver says a catalog has nothing for a value, or the user names a part.

1. Look at the nearest rows first (`board explain`); a value change is often cheaper than a new part.
2. `netsmith catalog add --lcsc C123 --to catalogs/resistor.csv` reads LCSC's record (never JLCPCB's) and appends a row with `lcsc`, `mpn`, `package` and a `source` naming the record and the day. Without `--to` it prints the record and writes nothing. It exits 1 when LCSC has no record or the file already has the number.
3. Fill the empty cells it lists from the datasheet (add `datasheet:<url>` to `source`) and `library` from JLCPCB's parts page, read by a person. A row with an empty cell a part reads is never offered. Every `*.csv` in `catalogs/` is a catalog, named by its file name; a row that names a part needs its `source`. A capacitor's bias curve is the manufacturer's, never estimated.
4. `netsmith board build`, then `explain` the part to see the new row picked or refused and why. `project lock --update` records it.

The standard catalogs ship with netsmith and cannot be edited from a project.

## Check against a supplier

- `netsmith catalog age` lists the rows whose price and stock (`date`) are old or unknown (`--days N`, `--only catalog`). It uses no network.
- `netsmith catalog refresh --dry-run` checks each row's `mpn` and package against LCSC's record and writes only the day in `checked`. It never writes price, stock or library: read those from JLCPCB's page. `CONFLICT` and `MISSING` rows are left as they were: look at them and fix the row. It makes a request for each row to an unofficial endpoint: run it only when the user asks, for a file or `--lcsc C123` at a time.
- `--offline` on `catalog refresh` and `catalog add` uses the kept answers and asks nothing (a row with none is `SKIPPED`). On `board fab` and `board bom` it asks no supplier, so stock and price are `unknown` and `fab-notes.txt` says so: tell the user to check each part before ordering.

## Report

For a question, name the row, what held it and the rows passed over. For a change, list the parts whose row changed (old, new, LCSC number), the diff of `project lock --check`, and every number that has a source and every one that does not yet.
