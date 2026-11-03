---
status: accepted (user review 2026-11-02)
---

# Catalog rows are CSV files, one per catalog, and a project can add its own

ADR 0016 left open "whether a catalog should be able to hold rows in `.nsm` or CSV so that adding an IC stops being a Rust edit". The decision is provisional, to be reviewed by the maintainer; this ADR records the decision and the choices made inside it. Nothing here changes what the language says: a part still declares `catalog X by .key { .a, .b }`, and the solver still takes the first candidate that works.

## The problem

Every catalog row was Rust code in `crates/netsmith-solve/src/jlcpcb.rs`: one function for each part kind, with the engineering decisions in its comments and a filter over `data/jlcpcb/parts.csv` in front. Adding a regulator meant editing the public repository and the solver crate. A private project could not have a part of its own at all, and a new kind of part (an IC) needed a new Rust function before it had a single row.

## Decision

**1. A catalog is one CSV file, named by the catalog.** `resistor.csv` is the `resistor` catalog. The standard catalogs are `data/jlcpcb/catalog/<name>.csv`, embedded in the binary by the solve crate's `build.rs`, which lists the directory, so adding a part or a whole catalog is a change to `data/`, not to Rust. The solver's sample catalogs (a made-up E24 resistor set, for the solver's own tests) are `data/sample/<name>.csv`, embedded the same way. A project's catalogs are `catalogs/<name>.csv` by convention and are listed in its manifest.

**2. The first line names the columns; a column is a field path without its leading dot.** `resistance`, `package`, `vout.voltage`, `mpn`. Each must be in the `catalog X { ... }` declaration of the part that uses the catalog. The file alone cannot say what a cell means (`0402` is a number or a package), so a file is read in two steps. Reading the file checks what needs no part. A part that uses the catalog then reads its cells as the kind of its fields, and that is where an unknown column or a cell in the wrong dimension is found.

**3. Values are the language's own syntax, text is bare.** A quantity cell is what `.nsm` accepts after `&=`: `10kohm +/- 1%`, `3.3V +/- 5%`, `1.8V..5.5V`, `0.3V`. It is read by the language's parser (`netsmith_lang::parse_expr`), so a number and its tolerance are the same interval a source would write; a test compares them. A text cell is its text as it is (`0402`, `SOT-23-5`, `RED`). The task as handed over wrote text as `"SOT-23-5"`. In a CSV a quote needs doubling (`"""SOT-23-5"""`), and a forgotten quote would silently never match, so the cell is bare and a cell that starts with a quote is refused with the fix. The value of a capacitor's `effective_capacitance` (ADR 0013) is a curve: `derate capacitance by bias: 0:1 5:0.5` says "the `capacitance` of this row, times the fraction left at the voltage across the part's `bias`". Nothing after the colon is a part with no published curve.

**4. Fixed columns.** `lcsc`, `mpn`, `library` (`Basic`, `Extended` or `Preferred`), `price`, `stock`, `source`, `date`, plus `name` (what a message calls the row), `label` (the schematic's value field) and `notes`. They are about the row, not the part, and a part may also list one as a column (`mpn` in a keyed catalog): its cell is then used for both. `name` and `label` default to the `mpn`, then the `lcsc`. They exist because messages and schematics must stay as they were (`10kΩ 1% 0402 C25744`, a value of `4.7k`), and a generic rule could not make those. The task did not list them; this is the one addition.

**5. A source is required for every part.** A row with an `lcsc` or an `mpn` must have a `source`: the URL or text where its numbers were read. That is the project rule that no part number comes from memory, now enforced by the loader instead of by a test over one file. A row with neither is a made-up part, as the sample catalogs are, and needs a `name`.

**6. The order of the rows is the selection policy, and nothing sorts it.** The solver takes the first candidate that works, so the file order decides which part a design gets. `jlcpcb.rs` used to sort by Basic, then price, then value; the standard files are now written in that order and a test checks it. A project that wants "cheapest first" or "in stock first" writes its rows that way. Every row in a file is offered (except a row whose `stock` is `0`, ADR 0039): there is no other filter on stock or on a row's status, so a part that is not wanted is removed from the file. The parts that were looked at and not offered (the 5-pin regulators, a 0 ohm link, parts with no stock) are in `data/jlcpcb/considered.csv` with the reason, not in a catalog.

**7. A project lists its catalog files in `netsmith.toml`.** `catalogs = ["catalogs/acme.csv", ...]`, relative and inside the project, each named after its catalog, with no catalog twice. They are in addition to the standard catalogs, which stay embedded. A file named like a standard catalog adds its rows after the standard ones. A part in a catalog once: the same `lcsc` or the same `mpn` twice, in one file or across the standard file and a project's, is an error naming both file:line places ("one answer"). `build`, `fab`, `view` and `compare` read the files on each run, so `view` sees an edit.

**8. Every mistake names its place.** An unreadable CSV, a ragged row, a repeated column, a bad `library`, `price`, `stock` or `date`, a row without a source, a duplicate, an unknown or missing column, a cell that does not parse or is in the wrong dimension, a quoted text cell, and a bad curve each give `file:line: ...` and say what to change. A mistake found when a board uses the catalog is an error at the part that uses it.

**9. The API stays.** `Catalogs::jlcpcb()` and `Catalogs::sample()` return the embedded catalogs; `Catalogs::add_file(name, file, text)` adds a file; `Catalogs::resolve` gives a catalog typed for a part's columns. `Catalogs::insert` still takes a catalog built in code, which tests use. The behaviour of the standard catalogs did not change: the candidates are the same, in the same order, with the same values, names and labels (checked by dumping every candidate before and after the move).

## Consequences

- `data/jlcpcb/parts.csv` and `data/jlcpcb/capacitor_bias.csv` are gone. Their rows are in `catalog/*.csv`, with the evidence in `source` (and the curve's source after `bias-curve:`), and the decisions that were Rust comments (the XC6206's +/- 5%, the LED forward-voltage ranges) are in each row's `notes` and in `data/jlcpcb/README.md`.
- `jlcpcb.rs` is a loader of a few lines. A new std part kind (ADR 0016's ICs) adds a CSV and no Rust.
- A private project keeps its parts in its own repository and never in netsmith's.
- `Library` has a third value, `Preferred`, for Extended parts JLCPCB does not charge a feeder fee for. No standard row uses it yet (C25102 stays `Extended`, as before).

## Not decided here

- Rows in `.nsm`. CSV was chosen because a spreadsheet and a diff both read it. A second way to say a row would break ADR 0001.
- A shared catalog of several projects (a company parts library). It is a directory of files that several manifests list.
- Filtering by stock or price at build time, beyond a stock of `0` (ADR 0039). A snapshot goes stale, so the number to trust is checked on the order page (`netsmith-prepare-fab-order`); `netsmith catalog refresh` (ADR 0039) does not update it: it only checks the rows against LCSC's record and records the day in `checked`.
- The units of `price` (US dollars for the standard files) are not checked.
- A catalog name shared by two part kinds with different columns. The first file would need the union of the columns; the checker does not forbid it and this has not come up.
