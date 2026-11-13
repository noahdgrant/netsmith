---
name: netsmith-dev-add-part
description: Add a real part, a capacitor bias curve or a catalog column to netsmith's JLCPCB catalogs (CSV files) with recorded evidence. Use when the solver says a catalog has nothing for a value, or when asked to support a new LCSC part or IC.
---

# Add a part to the JLCPCB catalogs

Entry point: `AGENTS.md`. The rules and every past source are in `data/jlcpcb/README.md`. Read it first. The format of the files is ADR 0022 and the "Catalog files" section of `docs/LANGUAGE.md`.

## The sourcing rule

- Never write an LCSC number from memory. Part names are search keywords only.
- A row exists only if a fetched record returned that exact C number with the expected value and package.
- Put the evidence in the row's `source` cell and in `data/jlcpcb/README.md`: the LCSC record (manufacturer number, package) and the row's library type as a person read it on JLCPCB's parts page (do not script JLCPCB's parts library, ADR 0039), cross-checked, and the manufacturer datasheet for every number the model uses. The loader refuses a row with an `lcsc` or `mpn` and no `source`.
- If you cannot fetch the evidence, stop and say so. Do not add the row.

## `data/jlcpcb/catalog/<catalog>.csv`

One file for each catalog, named by it (`resistor.csv`, `capacitor.csv`, `led.csv`, `ldo.csv`, `schottky.csv`, `zener.csv`, `tvs.csv`, `ferrite_bead.csv`, `inductor.csv`, `fuse.csv`, `n_mosfet.csv`, `crystal.csv`, `header.csv`; `p_mosfet.csv` and `level_translator.csv` have a header and no row yet, because no part of theirs has a recorded source: the first real row is added after the header, Basic first). A new catalog is a new file: `build.rs` embeds the directory, so no Rust changes. The header names the columns of the part's `catalog X { ... }` declaration in `std/` (the field path without the dot: `resistance`, `vout.voltage`) and the fixed columns `lcsc,mpn,name,label,library,price,stock,date,source,notes`. Copy the header of a neighbouring row. Cells use the language's syntax with units: `10kohm +/- 1%`, `1.8V..2.4V`, `0.68V`; text such as `package` is bare.

- Every row in a file is offered, whatever its `stock` cell says (the solver does not read stock, ADR 0047). The standard files ship no `price`, `stock`, `date` or `checked`: leave them empty in a row you add to `data/jlcpcb/catalog/` (a project's own file may state them; the `source` cell says when the record was read).
- `name` is what a message calls the row (`10kΩ 1% 0402 C25744`) and `label` the schematic's value field (`10k`). Both default to the `mpn`.
- A part you looked at and decided against goes in `data/jlcpcb/considered.csv` with its reason, not in a catalog.
- A catalog keyed by a column (`catalog mcu by .mpn`) has one row per orderable part, and the design names it with `.mpn` when several rows fit (one fitting row is taken without a name).

## Starting a row

`netsmith catalog add --lcsc C123 --to <catalog.csv>` asks LCSC's record of the number (never JLCPCB's) and appends a row with `lcsc`, `mpn`, `package` and a `source` that names the record and the day it was read. It leaves every other cell empty and prints the columns to fill: the ratings and curves from the datasheet (add `datasheet:<url>` to `source`) and `library` from JLCPCB's page. A row with an empty cell a part reads is never offered, so the row does nothing until you fill it. Check the `package` against `std/kicad.toml`; LCSC spells some packages its own way. Without `--to` it prints the record and writes nothing.

## Refreshing the numbers

`netsmith catalog refresh --dry-run --lcsc C5446` (or `--only resistor`) checks a row's `mpn` and `package` against LCSC's product record (never JLCPCB's) and prints what it would write: only the day in the `checked` column, for a row that agrees. It never writes `price`, `stock`, `price_breaks`, `library` or `date`: those are JLCPCB's numbers, which you read from its parts page by hand, and LCSC's differ. A new row leaves `checked` empty. A `CONFLICT` (LCSC names another `mpn` or package) or `MISSING` (the number is gone) row is left as it was: look at it, then fix or move it to `considered.csv`. When it notes that LCSC lists stock 0, read JLCPCB's stock on its page; if that is 0 too, the solver does not offer the row and the std tests want stock above 0, so move it to `considered.csv`. `netsmith catalog age` lists rows whose `date` is older than 90 days. Do not run it over a whole catalog without being asked: it makes one request for each row to an unofficial endpoint whose terms of use are unread (ADR 0039). For a project, the same command checks the files its `netsmith.toml` lists.

## Order is the policy

The solver takes the first row that works and nothing sorts the file. The standard files are Basic first, then in the order of the unit prices read on a past date, then ascending value. Put a new row after the Basic rows and where the price you read puts it; `cargo test -p netsmith-solve --test main data::` checks only that Basic rows come before Extended ones. To change the policy itself, ask the user (`netsmith-dev-decision`).

## Where the model fields come from

- Resistors: 1%, from the supplier's value, with `max_power` from the record's `Power(Watts)` (check it on the LCSC record too). A 0 ohm link is left out. A row with an empty `max_power` is never offered, so do not add a resistor whose rating you cannot read.
- Capacitors: value, tolerance and rated voltage from the supplier's record. `effective_capacitance` is `derate capacitance by bias: 0:1 ...`, the manufacturer's DC-bias curve: `volts:fraction` pairs from 0 V (`0:1`) to the rated voltage, volts rising, fractions between 0 and 1, lowered by a margin. Say where the curve is in `source` after `bias-curve:`. A part with no published curve gets `derate capacitance by bias:` with nothing after the colon. Do not estimate a curve, and do not add a class-level fallback.
- LEDs: color and forward-voltage range from the datasheet; say how in `notes`.
- Diodes, TVS, ferrite beads, inductors, fuses, MOSFETs and crystals (ADR 0025): the worst case of the datasheet table for each rating (maximum forward voltage at the rated current, minimum to maximum Zener voltage, largest RDS(on), an inductor's Isat and maximum DC resistance, a crystal's whole range over temperature), the package name of `std/kicad.toml` so the pick can be exported, and the JLCPCB and LCSC records agreeing on the model. When the record and the datasheet disagree, leave the part out and put it in `considered.csv`. `catalog` lines are in `std/parts/*.nsm`; `crates/netsmith-solve/tests/data.rs` lists the columns.
- Regulators: output voltage, dropout, current, power and package from the datasheet; say how in `notes`. The pads are not a column: the part's `pins` table in `std/` has a block for the package (ADR 0016), so a row for a new package needs its table too.

## ICs in `std/` with datasheet numbers

A part in `std/` that states logic levels or ratings (`std/parts/logic.nsm`, ADR 0046) puts the datasheet's number, revision and table next to each value in its doc comment, and a paragraph in `data/jlcpcb/README.md`. Read the numbers from the PDF text (`pdftotext -layout`), not from a summary. A value that depends on the supply is stated only for the datasheet row it comes from, and the part refuses supplies outside that row. If you cannot fetch the datasheet, do not write the part.

## Footprints and placement

A new package needs a footprint in `crates/netsmith-kicad/src/mapping.rs` (the row in `std/kicad.toml`). If the footprint is a chip, SOT-23, SOIC/TSSOP/MSOP, QFN/DFN or pin header, netsmith generates it by the KiCad name (ADR 0043, `crates/netsmith-pcb/src/generate/`): a name in `generate::names()` needs no library. For another package add a row to the generator that fits (its parameters are a table in that file), run `cargo test -p netsmith-pcb --test main generated_vs_kicad:: -- --nocapture`, and make it agree with KiCad's footprint of the same name or record why in `DIFFERENCES`; otherwise the footprint comes from a library file. Its pick-and-place rotation is unverified until a board with it has been assembled, so `crates/netsmith-fab/src/rotation.rs` leaves it out and `fab-notes.txt` flags it.

## Tests

Run `cargo test -p netsmith-solve` first. It checks that every row has its source, the file order, every curve, and that nothing in `considered.csv` is offered. Update counts in those tests when you add rows. Then run the gates in `netsmith-dev-verify`.

Then run `cargo test -p netsmith --test main guide::`. A new row changes the counts in the guide's messages (`the closest of the 6 that fit`): run it with `GUIDE_BLESS=1` and read the diff. A guide example that is meant to fail must fail for a reason no row can remove (a rating or a value far beyond any real part, or a requirement that contradicts itself), never because the catalog happens to lack a row; if a new row makes one build, change the example (`docs/LANGUAGE.md`), not the row.

## Refuse

- A number you cannot cite.
- A curve you read from nowhere.
- Marking a part Basic that the API lists as Extended.
- A part JLCPCB shows with zero stock.
