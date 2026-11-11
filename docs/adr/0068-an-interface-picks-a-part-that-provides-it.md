---
status: accepted
---

# A generic is the interface that picks a part: parts and modules `provide` it

The layering is the owner's decision of 2026-11-06: a part, and above it a general interface that picks a part from requirements. The status line said the owner accepted the syntax of the summary and took as decided the word `generic`, the import closure as the candidates, the order of section 4 with no JLCPCB class in stage 1, regulators as providers of a module-shaped `BuckConverter` with `interface Buck` unchanged, a strict rule that a provider does not redeclare the generic's members, and the other open questions of section 9 as recommended there. Tests 1 (`generic`, `provide`), 2 (the first that works is picked, not the best) and 3 (the rejected layerings). Cut from `git show 8023ee9:docs/adr/0068-an-interface-picks-a-part-that-provides-it.md`; sources cite its sections and stages.

## The problem

The solver picked a row of a catalog, never a part. A board that wanted "any connector that reaches 1.61 GHz" could not say so, and a board fed 12 V today and 48 V tomorrow had to change the regulator's name and know which of the project's ICs reaches 48 V. The same shape existed for passives in another spelling: `Resistor` was both the contract and the one provider, and the solver, the checker and `table.rs` each hard-coded the names.

## Decision

- `generic Name { ... }` states what the users of a part or module may rely on: pins with kinds, values with types, and the checks that hold for every provider. It has no pads, no package and no catalog (one exception, below). The word is not `interface`: a member of an interface type is a port, a use of a generic is placed as a part or a module, and one keyword for both would make the line `.rf: CoaxReceptacle` a port or a part depending on a `provides` elsewhere in the project.
- `part X provides Name { ... }` or `module X provides Name { ... }` says X can stand wherever `Name` is used. X is a hand-written part or module with its own pin table, numbers and one-row catalog.
- A board writes `.rf: Name { .max_frequency >= 1.61GHz }`. The solver picks the first provider, in a fixed order, for which every check of the whole design holds, and names it. `.rf: X` (the type) forces one, and nothing is picked; `.rf: Name { .mpn &= "..." }` picks the provider whose row has the number.
- The candidates of a use are every provider of the generic in a file the board's file imports, directly or through others, and in its own file. The order is fixed: the generic itself if it is its own provider, then the standard library's providers, then the project's, within each by module path and then name. Not source or hash order; no catalog class between providers. Nothing compares candidates: order is the policy (ADR 0022, ADR 0047), so the pick is the first that works.
- A provider has every member and statement of the generic and does not declare one again (strict, one correct way); each pin of the generic has a row of the same kind in every package or is an alias of one; names that differ are tied with `==` and `~`; a use reaches only the generic's members, so a board that works with one provider works with all. Only a `generic` is provided.
- The choice is made one level above `solve`: a provider changes the model's structure, so a trial is a whole elaboration and solve. The search is bounded (`MAX_PROVIDER_TRIALS`, 64) and a shortcut gives up on a failure no provider changes. The build says which provider it took and why the others were passed over. A trial that cannot be judged (a bound that names a value inside the use that nothing states, a level that could not be checked) does not hold: a provider that can be judged is preferred, so a pick never rests on a check that was not made.
- The twelve standard definitions that have a catalog (`Resistor`, `Capacitor`, `Inductor`, `Led`, `FerriteBead`, `Crystal`, `Fuse`, `Schottky`, `Zener`, `Tvs`, `NMosfet`, `LinearRegulator`; later `PMosfet` and `LevelTranslator`) are generics that are their own first provider, so they keep the kind name, the rows and every output until a project adds a provider. The synthesized E-series provider (ADR 0047) stays Rust, in one table, `crates/netsmith-solve/src/standard.rs`.

The syntax rules, the conformance checks and their messages are in the header of `crates/netsmith-check/src/providers.rs`; the search is in the header of `crates/netsmith/src/providers.rs`; the user's view is in `docs/LANGUAGE.md` ("Generics and providers", "The standard passives are generics").

## What was rejected

Extending `interface` so that one with providers is picked (a line's meaning would depend on another file); `interface part` or `abstract part` (two words for one kind); a `part` with no pads, abstract by absence (`.u: Zbk2` must mean exactly that IC); `role`, `spec`, `family`, `trait`, `protocol` (new vocabulary for no gain). `part X implements G` outside the part: it makes the set of providers depend on which files are loaded; a module around a foreign part is the way to adapt one. A mapping block (`provides G { .signal ~ .rf }`), and providing as inheritance with extra members. A keyword to force a pick (`pick X`). Every file under `src/` as candidates (adding a file would silently add a candidate to every board); a list on the board or in the manifest (a second and third list where the import list is the list). A named provider beside a pure generic for the standard passives: its name would replace `Resistor` in the bill of materials, `std/kicad.toml`, the guide's output and the placer's lists. Keying the E-series by the generic's name: a project's own part that lists `catalog resistor` has always had both, so the table is found by the catalog's name. A guideline when more than one provider works: every board with a generic would be noisy. A provider that contains a use of its own generic is refused, at the use inside it, naming the cycle (leaving the provider out of the candidates would make a wrapper valid by how its name sorts). Opening the search on a trial of the first provider's check fault: the choice starts after a clean check, so such a fault is seen only for the first provider, and opening the search on it would change the pick for every kind of checker error.

## Cost

A trial is a full solve (0.22 s for a 58-part board, release build), so a use with four providers whose first three fail costs under a second, and the bound is about 14 s on such a board. A board that takes tens of seconds to solve makes the failing case minutes. The first provider that works is not the best, and a pick changes when a file is added; the note names the pick and forcing is one word. A failure inside a provider names the provider's field paths, which the author has not written. A contract that is too tight or too loose is the library author's. Two providers need different names, because a part is known by its name in the bill of materials, `kicad.toml` and the notes. A provider needs its own `kicad.toml` entry, the catalog file in the manifest and a source for the part number. Existing parts become providers by one mechanical edit (drop the members the generic has, `.buck: Buck { ... }` becomes statements). `.mpn` is not available on a generic of modules.

## Left open

Nothing. The owner accepted these on 2026-11-11, as built:

- No fallback to the generic's own kind in `kicad.toml`: providers that draw alike each need an entry.
- _generic part_ (what the synthesized provider makes) stays beside _generic_.
- No syntax for a bank, an optional column or the E-series until a second user exists. A standard generic with a numeric `.count` is a bank only if `BANKS` in `model.rs` (`["Capacitor"]`) names it.
- A use of a generic inside a generic is refused; only the uses a diagnostic names are varied.
- A pick is a note, not a finding.
