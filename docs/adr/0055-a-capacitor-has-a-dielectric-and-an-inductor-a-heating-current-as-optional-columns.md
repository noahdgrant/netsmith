---
status: proposed
---

# A capacitor has a dielectric and an inductor a heating current, as optional columns

Status notes: Written overnight on 2026-11-05 by an agent that was told to pick an option and go on. The type of `.dielectric`, the variants and the filling of the column are the owner's to accept or change.

A design could not say that a bypass capacitor must be X7R, or that an inductor must carry a heating current, because the catalogs had no column for either. The two are alike in what was done and unlike in what could be filled, so one record holds both.

## Decision

Two optional catalog columns, each a field of the part (ADR 0022, and the `OPTIONAL` list of `crates/netsmith-solve/src/table.rs`: a file that leaves the column out of its first line is read as empty there, so a project's catalog written before the column stays valid).

- `Capacitor.dielectric`, of the enum `Dielectric` (`C0G`, `X5R`, `X7R`) defined in `std/parts/passive.nsm` (ADR 0054). A design requires a class with `.dielectric &= Dielectric::X7R`, or compares it with `==`, and imports `Dielectric` with the part. The catalog column holds the variant name as it is (`X7R`); a cell that is not a variant is refused, naming the row and the variants. A row with an empty cell fits no design that states a class, as for any column (ADR 0047); a design that does not mention the field picks as before.
- `Inductor.rms_current`, a current: the heating (RMS) rating. The standard `inductor.csv` has the column and every cell is empty.

Filled from the sources: `capacitor.csv` has a `dielectric` for all 58 rows. The class is in the `name` cell of each row (`18pF 5% 50V C0G 0402 C1549`) and in the `JLCPCB lists ...` line of its `notes`, both from "JLCPCB and LCSC part records, read 2026-11-01" (the row's `source`), and the two agree on all 58 rows (20 `C0G`, 14 `X5R`, 24 `X7R`). Nothing was entered from memory or from a datasheet. `data/jlcpcb/README.md` says so.

Left empty: the heating current. The notes of the four inductor rows mention a temperature-rise current, but not as one rating (2.4 A for one row with no rise stated; 700 mA, 1200 mA and 1000 mA at 40 K for the three others, from different makers), and a value enters a catalog only with its source and the rise it is for. The README lists the four figures and what is missing.

Generic parts (ADR 0047) have neither value: their cell is empty, so a design that states a class or a heating rating is given a catalog row and never a generic part. `generic.rs` lists both columns as known (a part with a column the generic code does not know gets no generic candidates at all) and fills neither.

`BuckStage` and `Buck` are not changed. The stage would check the inductor's heating current against the load only by a relation that mentions `.inductor.rms_current`, and the solver counts a field as asked for as soon as any relation mentions it (`asked`, `crates/netsmith-solve/src/solve.rs`), independent of whether the IC states a limit. So the relation would make every standard inductor, whose cell is empty, unfit for every stage, and would do the same to a project's inductor catalog that predates the column. A design that needs the check states `.stage.inductor.rms_current >= 2A` itself and a project's catalog gives the rows.

How a design requires a package today: `.package &= "0603"` states one size as text, and the solver takes the rows of that package (and a generic part of that size). It does not state a set of sizes, so the advice "0402 or 0603" is not a statement the language has: a design that would take either leaves `.package` open and the catalog's order decides (Basic first, then the file), or states it on the capacitors that need a size. This is not changed here. A set of sizes would be a language choice (a range of text, an `enum` of sizes, a list), and it is the owner's.

## What was rejected

A `String` `.dielectric`, compared with `==`. A mistyped class (`"X7r"`) would be a text that matches no row, and the message would say the catalog has nothing for the capacitor, not that the class does not exist. The enum catches it where it is written and lists the three. The cost is that a new class is a change to the library (a variant and the rows that have it) and not a cell, which is the point of a closed list.

More variants than the catalog has (`NP0`, `X6S`, `X8R`, `Y5V`). No row has them, and a variant with no row can only produce a "nothing in the catalog" error. `NP0` and `C0G` are one class written two ways; JLCPCB's record prints `C0G`, so that is the variant, and a second spelling would break "one correct way".

Entering the four heating currents of the notes as the column's cells. They are not one rating, and the first has no rise. A column of mixed definitions is a check that cannot be read, and the number goes in only with its source (`docs/DESIGN-RULES.md`, "A part number needs a source").

A `Buck.min_inductor_rms_current` and a stage check `.inductor.rms_current >= .buck.min_inductor_rms_current`, in the style of the saturation and resistance limits. See above: with the column empty in the standard rows, an unconditional relation refuses every standard inductor. If the owner fills the column, or changes how `asked` reads a relation that cannot be evaluated, the check is two lines in `regulator.nsm`.

A change to `asked` so that a relation with an unset limit does not ask. It would let this check stand and it would change which rows every other check refuses (a capacitor row with no `max_voltage` would pass for a rail whose `.peak` is unset). That is a solver decision with its own tests, and it is not made here.

## Cost and limits

A design that states `.dielectric` against a project catalog without the column gets no row, and the message is the usual "nothing for". The enum lives in `std::parts::passive`, so a board imports it; the prelude (ADR 0054) holds only `AssemblyMode`, because that is the one type a built-in member needs. A part of the generic series cannot be made X7R. The `dielectric` of the row is not used to derate the bias curve or to choose a temperature coefficient: the curve is the row's own, and temperature is not modelled.

## What to look at

- `Dielectric` as an enum of the three classes the rows have, and in `std::parts::passive` and not the prelude.
- That all 58 capacitor rows were filled from the listing text, with no new source read.
- That the inductor column is empty and `BuckStage` has no heating check, and the reason (`asked`).
- That "0402 or 0603" is not expressible and was left alone.
- The generic parts having neither column, so a design that states a class never gets a generic part.

After the review of 2026-11-06 (ADR 0058, proposed): the leniency for an empty cell applies only to the optional columns named above, and `asked` also counts a `~` join and a balance. For any other column an empty cell takes the row out, as before.
