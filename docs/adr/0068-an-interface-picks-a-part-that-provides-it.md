---
status: stages 1, 2 and 3 implemented on 2026-11-06 (section 12 says what stage 2 built and where it differs from section 7; the layering is the owner's decision of 2026-11-06: a part, and above it a general interface that picks a part from requirements, with the passives moving to it in three stages; the owner reviewed the syntax in the summary below and accepted it, and took as decided the word `generic`, the import closure as the candidates, the order of section 4 with no JLCPCB class in stage 1, regulators as providers of a module-shaped `BuckConverter` with `interface Buck` unchanged, and a strict rule that a provider does not redeclare the generic's members; the other open questions of section 9 were decided as recommended there; section 14 says what stage 3 built, which special cases it removed and which stay, and why; section 11 says what stage 1 built and where it differs from the text above; section 13 says what the second independent review found and how each of its nine findings was fixed)
---

# A generic is the interface that picks a part: parts and modules `provide` it

The file name keeps the owner's phrase, "an interface picks a part". The body calls that interface a **generic**, for the reason in section 2: the language already uses `interface` for something that a board's member of that type is a port of, and the two cannot share a word.

## Summary

Today the solver picks a row of a catalog, never a part. A board that wants a particular hand-written connector or regulator IC writes its name, and a board that wants "any connector that reaches 1.61 GHz" cannot say so.

The decision, in four sentences:

- `generic Name { ... }` states what the users of a part or module may rely on: pins with their kinds, values with their types, and the checks that hold for every provider. It has no pads, no package and no catalog.
- `part X provides Name { ... }` (or `module X provides Name { ... }`) says that X can stand wherever `Name` is used. X is a hand-written part or module, with its own pin table, its own datasheet numbers and its own one-row catalog, as today.
- A board writes `.rf: Name { .max_frequency >= 1.61GHz }`. The solver picks the first provider, in a fixed order, for which every check of the whole design holds, and names it. A board that wants one provider declares it by its type (`.rf: X`) and no pick happens.
- The standard passives (`Resistor`, `Capacitor`, `Inductor`, `LinearRegulator` and eight more) become generics that are their own first provider, so that a project can add a provider of any of them, and behave exactly as before until it does.

## The syntax at a glance

All names and numbers are invented. The first example is a connector with one hand-written provider, the second a regulator with two. Section 3 gives the rules and what was rejected.

### A connector

The generic, in the project's `src/rf.nsm`:

```nsm
/// A coaxial receptacle that carries an antenna lead off the board.
generic CoaxReceptacle {
    .designator &= "J"

    pins {
        .signal passive required
        .shield passive required
    }

    /// The characteristic impedance. Every receptacle of this generic is 50 ohm.
    .impedance: Resistance
    .impedance &= 50ohm
    /// The highest frequency it is specified for.
    .max_frequency: Frequency
    /// The manufacturer's number, so that a design can name the part.
    .mpn: String
}
```

A hand-written provider, in `src/parts/acme_micro_coax.nsm`. The body is a part as written today: its own pin table (names and pads), its own numbers, its own one-row catalog. It does not declare the generic's members again.

```nsm
import src::rf::CoaxReceptacle

/// ACME-MC-1, a surface-mount micro coaxial receptacle, DC to 6 GHz. Invented.
part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        //  pin     kind     name      pads
        .signal     passive  "SIGNAL"  "1"
        .shield     passive  "GND"     "2" "3"
    }
    .max_frequency &= 6GHz

    .package: Package
    catalog acme_micro_coax { .mpn, .package }
}
```

```csv
mpn,package,library,source
ACME-MC-1,ACME_MC_1,Extended,https://example.invalid/acme-mc-1.pdf
```

The project lists the catalog file in `netsmith.toml` and the part in `kicad.toml` (`kind = "AcmeMicroCoax"`), as for any hand-written part. A board file brings the provider's file into scope with an import and uses the generic:

```nsm
import src::rf::CoaxReceptacle
import src::parts::acme_micro_coax::AcmeMicroCoax

module AntennaFeed {
    .signal: Pin
    .gnd: Pin

    // Any receptacle specified up to the radio's top frequency. The solver picks one.
    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}
```

`AcmeMicroCoax` is not named in the module. The import is what puts the part's file, and so its provider, in scope (section 4).

The three ways to say what part the board wants:

```nsm
.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }   // a requirement: the solver picks
.rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }          // the part's number: the solver finds its provider
.rf: AcmeMicroCoax                                   // the provider itself: nothing is picked
```

The last line changes only the type. Every other line that mentions `.rf` still holds, because a provider has every member of the generic with the same names.

A provider whose own names differ states the generic's names with the aliases and equations the language already has, and says nothing more:

```nsm
part OtherCoax provides CoaxReceptacle {
    pins {
        .rf      passive  "RF"   "1"
        .shield  passive  "GND"  "2"
    }
    .signal ~ .rf               // the generic's name for the pin this table calls .rf
    .fmax: Frequency
    .fmax &= 3GHz
    .max_frequency == .fmax

    .package: Package
    catalog other_coax { .mpn, .package }
}
```

A part or module may provide several generics: `part AcmeMicroCoax provides CoaxReceptacle, RfConnector`.

### A buck regulator with two ICs

A buck converter's IC has pins that only it has (enable, boot, frequency), so what a board needs is the IC with its stage and support parts: a **module**. A module may be a provider. The generic is a module-shaped contract; the existing interface `Buck` (the numbers an IC states for `BuckStage`) is unchanged and is used inside the providers.

```nsm
import std::sources::Rail

/// A buck regulator with everything around it. A board states the input, the output and the
/// load; the provider is an IC with its stage.
generic BuckConverter {
    .vin: Rail
    .vout: Rail
    /// The highest voltage the input pin survives.
    .vin_abs_max: Voltage
    // A transient never goes past what the input survives.
    .vin.peak <= .vin_abs_max
}
```

The providers, in one file. `Zbk2` is the guide's invented IC (4.5 V to 6 V, 2 A); `Zbk9` is another (4.5 V to 60 V, 3.5 A).

```nsm
import std::parts::regulator::BuckStage
import src::buck::BuckConverter
import src::zbk2::Zbk2
import src::zbk9::Zbk9

/// Zbk2 with its stage.
module Zbk2Buck provides BuckConverter {
    .vin_abs_max &= 7V

    .controller: Zbk2
    .stage: BuckStage
    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
    .controller.en ~ .vin.pos
}

/// Zbk9 with its stage.
module Zbk9Buck provides BuckConverter {
    .vin_abs_max &= 65V

    .controller: Zbk9
    .stage: BuckStage
    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
    .controller.en ~ .vin.pos
}
```

A board that picks by requirement. The input is 12 V, which `Zbk2` cannot take, so the pick is `Zbk9Buck`:

```nsm
import src::buck::BuckConverter
import src::buck_ics::Zbk9Buck    // not used by name: brings every provider of the file into scope
import std::sources::Rail

board Converter {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }

    .reg: BuckConverter {
        .vout.voltage &= 3.3V +/- 3%
        .vin_abs_max >= 40V
    }
    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

The build says which provider it took and why the others were passed over (wording proposed, in the style of ADR 0057):

```text
note: reg is a BuckConverter: picked Zbk9Buck; passed over Zbk2Buck (reg.vin_abs_max must always be at least 40V, but 7V is not always above 40V)
```

Forcing the choice, with no pick: `.reg: Zbk9Buck { .vout.voltage &= 3.3V +/- 3% }`. When nothing works:

```text
error: no BuckConverter provider satisfies reg: the closest of the 2 in scope, Zbk9Buck, fails because reg.vin_abs_max must always be at least 70V, but 65V is not always above 70V
```

When nothing provides it:

```text
error: `reg` is a `BuckConverter`, and no part or module in the files this file imports provides it; import the file of a provider (`src::buck_ics`), or declare one, `.reg: Zbk9Buck`
```

## 1. The problem

A generic is today a catalog row, never a hand-written part.

A catalog row carries the columns that the part's `catalog X { ... }` lists, and nothing else: `Resistor`'s `resistance`, `max_power`, `package`. The pins, the relations and the other members belong to the one `part` that holds the catalog (`std/parts/passive.nsm`). The solver (`pick_parts` in `crates/netsmith-solve/src/solve.rs`) reads the catalog of each part that has one, in file order, and takes the first row whose values keep every relation that mentions the part. Nothing in it can choose between two parts that have different pin tables, different relations or different support circuits.

Two examples show where this stops.

- A connector. The standard `Coax` has `.signal`, `.shield`, `.impedance` and `.max_frequency` and no catalog, so it can be placed but never picked. A design that needs an antenna connector writes a hand-made part's name (`.connector: AcmeMicroCoax`), and its other sheets check `.connector.max_frequency >= .antenna.max_frequency` against that one part. Adding a second receptacle to the project changes nothing; the board has to be edited by hand to use it.
- A buck regulator. `BuckStage` is written against the `Buck` interface, so any IC can drive it, but the board still names the IC's module, one module per IC. A board that is fed 12 V today and 48 V tomorrow has to change the regulator's name, and the author has to know which of the project's ICs reaches 48 V. The datasheet numbers that decide it are already in the project.

The same shape exists for passives, in a different spelling. `Resistor` is both the contract (two passive pins, an Ohm's law relation, a power rating) and the one provider (the `resistor` catalog and, after the rows, the synthesized E-series parts of ADR 0047). `crates/netsmith-solve/src/generic.rs` hard-codes the four catalog names that have synthesized parts and the columns each can fill; `Model::bank_count` hard-codes the type name `Capacitor`; `table.rs` hard-codes three optional columns by catalog name. That is the second system this ADR replaces with the first.

## 2. The model and its terms

Three terms are new, one is widened.

- **Generic**: a definition (`generic Name { ... }`) of what a part or module must have to stand for it, and what its users may rely on. Pins with kinds, values with types, relations and defaults. It has no pads, no package and no catalog of its own (one exception, below).
- **Provider**: a part or module whose definition says `provides Name`. It has every member and every statement of the generic, and adds its own pin table, numbers, catalog and, for a module, parts.
- **Use** (of a generic): a member declared with a generic's type, `.rf: CoaxReceptacle { ... }`. After the solver's pick a use is an instance of the provider, at the same path.
- **Candidate** (`GLOSSARY.md`) is now one row of a catalog, or one provider.

**Why a new word and not `interface`.** `interface` already means a typed set of pins and quantities that is connected as one. A member of an interface type is a port when a board or module declares it (`.supply: Rail` is a source, `.output: Rail` is a port in a system), it holds no parts, and it has no pads. A use of a generic is a thing that is placed: it ends as a part with a package or a module with parts. If one keyword made both, the line `.rf: CoaxReceptacle` would be a port or a part depending on what the definition holds, and the reader could not tell from the line.

Rejected:

- Extending `interface` so that an interface with providers is picked. Whether a line is a port would then depend on a `provides` somewhere else in the project, and adding a file would change what an existing line means. That breaks "statement order never matters" in spirit and gives one word two meanings (ADR 0001).
- `interface part Name` or `abstract part Name`. Two words for one kind, and either reads as a variant of something placed; `abstract part` suggests that a part can be both. A definition kind of its own is one keyword.
- `part` with no pads and no catalog, abstract by absence. `.u: Zbk2` today means exactly that IC; if any part could be provided by another, a board could not say "this one". A kind keeps the two apart: a part or module type in a member is exact, a generic type is any provider.
- `role`, `spec`, `family`, `trait`, `protocol`. New vocabulary for no gain; the owner's own word for `Resistor` and `UflReceptacle` is "generic".

`generic` is one token in one place in the lexer's keyword table and the parser; if the owner prefers another word, only text changes.

**What the glossary says about the old word.** *Generic part* (ADR 0047) stays as it is: a part the solver chose by value alone, with no supplier. It is what the synthesized E-series provider makes. The new word is only the definition; "a generic part" is a pick, "a generic" is a definition. Both are in the glossary with a line each, because the two will be read together. Renaming the old term was rejected here (it touches ADR 0047, the guide, `generic.rs` and `Pick.generic`) and is an open question.

**One exception: a generic that is its own provider.** A generic whose body has a pin table with pads, or a `catalog`, is itself the first provider (section 4). The standard passives are written this way (section 7), which is what keeps `Resistor` the same text, the same kind name and the same rows. A generic with neither, such as `CoaxReceptacle`, has no provider of its own and is useless until a provider is in scope.

**What a generic is not.** It holds no parts and no modules (like an interface), it has no connections (`~`) except between its own members, and it has no `catalog`/pad rows unless it is its own provider. It is no inheritance: a provider is not a subtype that can add members a use may reach. A use reaches only the generic's members (section 3), so a board that works with one provider works with every provider.

## 3. Syntax

The examples above are the rules in use. The decisions behind them:

**Declaring a generic.** A body of the same grammar as a part's, restricted:

- Members `.name: Type` of any type a part may have: `Pin`, quantities, text, an enum, an interface (`.vin: Rail`). No part or module members.
- A `pins` table whose rows are `.pin kind`, with an optional `required` or `optional`, and no name and no pads. The kind is a requirement on every part that provides the generic: the provider's row must give the same kind, because the kind decides what the checker asks of the board (`GLOSSARY.md`, _Pin kind_). A module provider has `Pin` members with no kinds, so for a module the table's kinds are not compared and its pins only fix the names; a generic that has a `pins` table is provided by parts only.
- Statements: narrowings (`.impedance &= 50ohm`), relations (`==`, `>=`, `<=`), defaults (`?=`), `bridge` (for a part provider), and `.designator &= "J"`.
- A relation or narrowing in a generic is a check that every provider meets and every user may rely on. The solver evaluates it on whichever provider is tried, like any relation of that provider.
- Optional member `.mpn: String`. A provider's catalog lists `.mpn` as a column, as hand-written parts do today (a keyed catalog is the same idea). Stating it picks the provider whose row has that number. It is not implicit, because a generic of modules has none.

All providers of one generic are parts, or all are modules; a generic is placed as the kind its providers have. A generic with providers of both kinds is an error at the second provider.

**Providing.** `part X provides A, B { ... }` and `module X provides A, B { ... }`. The word `provides` follows the name, as `for` follows `physical`. It is one more header token and no new statement, so a provider's body can be moved, and the generic stays ignorant of its providers (it cannot import them: a provider imports the generic). The checker elaborates every definition, so an unused provider is checked too, at its own definition, with these rules:

1. It has every member of each generic it provides, with the same name and type, and every statement of each; it does not declare one again (error: "`max_frequency` is declared by `CoaxReceptacle`, which this part provides; narrow it with `.max_frequency &= ...`"). Members are the generic's, never copied by hand. A member that carries a block in a part today (`.buck: Buck { .vref &= ... }`) becomes statements (`.buck.vref &= ...`).
2. Each pin of the generic has a row in the provider's pin table of every package, with the same kind, or is an alias of a pin that has one (`.signal ~ .rf`, ADR 0016's alias). A `required` or `optional` on the generic's pin holds, and a row may leave it out.
3. A value of the generic that has a different name in the provider is tied with an equation (`.max_frequency == .fmax`); a pin with an alias. No mapping block is needed, because the two forms exist and one correct way is enough.
4. The generic's narrowings and relations are added to the provider; a provider that contradicts one (`.impedance &= 75ohm` against the generic's 50 ohm) fails the narrowing where it is written.
5. A part that provides a part, or a module that provides an interface, is an error: only a `generic` is provided.

Rejected: `part X implements G` or `impl G for X` outside the part, which would let one add a provider to a part one does not own. It makes the set of providers depend on which files are loaded and says one thing in two places; a module that wraps the foreign part (modules may hold parts) is the way to adapt one. A `provides G { .signal ~ .rf }` block that maps names: a second spelling of aliases and equations. Providing as inheritance with extra members reachable from outside: it would let a board depend on one provider.

**Using.** `.name: Generic { ... }`. The block takes the same statements as any member's block, over the generic's members only. A path through a use to a member the generic does not declare is an error (the module rule of ADR 0004 for a use: "`gain` is not part of the interface of `CoaxReceptacle`"), which is what makes the board independent of the pick. `Checker::visible` (`crates/netsmith-check/src/checker.rs`) is where the inside of a module is refused today and where this is added.

**Forcing a choice.** Two ways, and no third.

- By type: `.rf: AcmeMicroCoax`. It is a part declaration like any other; the generic is not involved, and nothing is picked. This is the answer to "I want exactly this one": the abstraction level is the type the board writes.
- By `.mpn`: `.rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }`, for a generic that declares `.mpn`. The solver tries the providers in order and the catalog row that has the number; a provider whose catalog has no such row is passed over, not an error.

Rejected: a keyword, `.rf: pick AcmeMicroCoax`, or a statement `use AcmeMicroCoax`: it is the first form again with more words. A statement `from A, B` that restricts the candidates (section 4).

**Member of a module, per instance.** A board cannot reach into a module (ADR 0004). A module that wants its user to choose a provider declares a value that it ties to its use (`.connector_mpn: String`, `.connector.mpn == .connector_mpn`), as ADR 0042 does for the assembly mode. No new rule.

## 4. The candidate rule

The owner's phrase: a generic picks a specific part "when it's added to a design".

**Decision: the candidates of a use are every provider of the generic in a file that the board's file imports, directly or through other files, and in the board's own file.** The set is whatever the loader loaded for that file. That is the existing import graph (`Checker::add_module` caches each module and resolves each import), so a file added to the design is a file imported by it. A std provider is a candidate when its std file is imported, and a project's when its file is.

The order is fixed, and the same sources give the same pick:

1. the generic itself, when it is its own provider (section 2);
2. the providers in files of the standard library, then those of the project;
3. within each, by module path, then by the provider's name.

Not source order inside a file, so that moving a definition changes nothing, and not hash order. The first candidate for which the whole design holds is taken. Nothing compares candidates (the policy of ADR 0022 and ADR 0047: order is the policy), so the pick is the first that works, not the best; a design that cares states a requirement or names the type. The catalog preference order (JLCPCB Basic, Preferred, Extended, then file order) already decides the row inside a provider that has a catalog. It is not used between providers in stage 1, because a provider's class is known only after its catalog is read and a module has none; that is an open question with a recommendation.

**Determinism.** The pick depends on the sources, the catalog files and the manifest only. It does not depend on the KiCad libraries or the mapping (the editor and `compare` run without them, and the same project must pick the same part in every tool), on a supply provider, on stock or on the time. A provider that has no symbol in `kicad.toml` is therefore not skipped: the build fails at export, naming the provider and the use, and the author forces another type or adds the mapping.

**Rejected:**

- Every file under `src/`. A project may keep unadopted parts in a folder of their own and several unrelated boards side by side; adding a file would silently add a candidate to every board. The import is a deliberate act.
- A list stated on the board (`from A, B`, or a manifest key `providers = [...]`). The import list is the list: a provider is a candidate only if the board's file can reach its file, and "only these two" is two imports and nothing else imported that defines another. A second list says the same thing a second way (ADR 0001), and a manifest list would add a third place to the two a hand-written part has already (the catalog file in the manifest, the mapping in `kicad.toml`).
- Both, with the import as default. Rejected for the same reason: the narrowing form adds a keyword and a rule about which list wins.

An import that is used only to bring providers in scope is a plain import. The checker has no unused-import check, so nothing changes; the guide says to comment it.

**When nothing works.** Three messages, the first two in the style of ADR 0047 and ADR 0057:

- No provider in scope: ``reg is a BuckConverter, and no part or module in the files this file imports provides it``. It names the generic's definition and says how to bring one in scope or name one.
- Every provider that was tried fails: ``no BuckConverter provider satisfies reg: the closest of the 2 in scope, Zbk9Buck, fails because ...``. The closest is the trial with the fewest diagnostics, ties in candidate order; the other providers' first diagnostics are listed after it, one line each.
- A pick that works is a note (the one above), not a finding.

A provider passed over because its own catalog has no row for the design is counted in the second message with the `catalog` message of ADR 0047 for it.

## 5. Solver integration

The structure of a model depends on the provider (its relations, members, pin tables, parts), and the model is built by the checker before the solver runs. So the choice cannot be a candidate row in `Solver::pick_parts`: a row only supplies values for fields that already exist. **A provider is a choice made one level above `solve`, and a trial is a whole elaboration and solve.** This keeps `solve`, the joint search and ADR 0057 untouched.

**Where it lives.**

- `netsmith-check`: `check` gains `check_with(name, text, loader, &Assignment)`, where an `Assignment` maps a board's or system's name and a use's path (`reg`, `gnss.rf`) to a provider. `check` is `check_with` of the empty assignment. `Checker::instantiate` (`checker.rs`) sees a member whose type is a generic, finds its candidates (new module `providers.rs`: scan `Checker::modules` for definitions whose `provides` names it, order them as section 4 says), takes the provider the assignment names or the first, and instantiates the provider's definition with the generic's items first (`self.declare` and `self.process` over `generic.items` then `provider.items`, each in its own module's scope, which `instantiate` already swaps for one definition). Every board's model records its uses: `Board.uses: Vec<GenericUse { path, inst, generic, candidates, chosen, declared }>`; `Instance` gains `provides: Vec<String>` (the generics a definition provides) and `placed_as: Option<String>` (the generic a use was placed as). With no provider in scope `check` reports it (section 4); the checker needs no solver for that.
- `netsmith-solve`: nothing in `solve`. `pick_parts` runs per trial exactly as now, with the trial's catalogs.
- `netsmith` (`outcome_with`, `crates/netsmith/src/lib.rs`): the loop. It checks once (the default assignment: the first provider of every use), solves, and if the solution has no diagnostics that is the result, at the cost of one solve, as today. Only a failure with a use that has more than one candidate starts the search. `notes` lists the picks and the passed-over providers; `Built` carries them for the editor.
- `netsmith/src/editor/analysis.rs` (check only) sees the default assignment and the check-time errors; the editor's problems list shows a provider's failure only after a build, like any solver message.
- Systems: the same loop around `solve_system` with the system's model.

**The search.** A trial is `check_with(assignment)` then `solve`. The order is depth first over the uses in path order, with the last use varying fastest, and candidates in section 4's order. So the first use keeps its preferred provider as long as any assignment of the later ones works, and the same inputs give the same pick. The trials are bounded: `MAX_PROVIDER_TRIALS` (proposed 64, a constant like `MAX_TRIALS` of the part search), and on reaching it the build says `gave up after trying 64 provider combinations` and names the uses, in the form of the existing message.

Before the first trial of a failed design, a shortcut: the first use with more than one candidate is varied alone over its candidates; if every candidate gives the same diagnostics (rendered text), the failure is not the use's and the default's diagnostics are the result, at the cost of one extra solve per candidate of that use and no product. Without it a design with an unrelated mistake would be solved once per combination.

**How a provider choice and a row choice interact.** The provider is the outer choice and the rows of its catalog are the inner choice, searched by ADR 0057's joint search inside the trial. A provider with a one-row catalog has one candidate row. A provider that has a catalog with several rows (a standard passive, a header) is the same search as today. The provider whose catalog cannot serve the design fails its trial with the existing "the `x` catalog has nothing for `.rf`" message, and the next provider is tried. `Catalogs::with_pin` (rechecks of ADR 0047) keeps working because the path of a use does not change.

**Size and cost.** The number of full solves is 1 when the first candidate of every use works, which is every design with no generic of more than one provider (all of today's designs and, after stage 2, all passives). Otherwise it is at most the product of the candidate counts of the uses that fail, bounded by the constant above. For `BuckConverter` with two providers the cost is two solves of a board. A solve of a large board was measured for stage 1 (section 11): about 0.22 s for an invented board of 58 parts, in a release build, of which the elaboration is 2 ms. The multiplier matters for a design with several uses and many providers, so the cap and the shortcut stay. If the product is the problem, the next step is to vary only the uses whose subtrees the diagnostics name; it is not built now.

ADR 0057's fail-fast runs inside each trial. A provider whose parts nothing can serve is rejected at the cost of that trial's fail-fast (0.4 s in ADR 0057's example), not of 250000 combinations, and a provider that is hopeless for the design costs one trial.

**Checks on the generic and on the provider.** The generic's relations are on the provider's instance from the start, so the solver judges them with the provider's values, like the provider's own. The conformance rules of section 3 are static checks at the provider's definition and cost nothing at pick time. A requirement written on the use (`.max_frequency >= 1.61GHz`) is a relation of the use's instance, so it is judged in the trial of each provider, with the values that provider has, including the ones it works out (a `.vin_abs_max` that is equal to the IC's maximum through a relation inside the provider).

**What the solver reports as the pick.** `Solution` does not change. The driver records for each use the generic, the provider chosen and each passed-over provider with its first diagnostic (`Built.providers`), and `notes` prints them. The row inside a provider is the existing `Pick`.

## 6. Lowering, the bill of materials and the exporters

The picked provider is an instance at the use's own path: `gnss.rf` stays `gnss.rf`. Everything that is keyed by path keeps working: the layout file (a pick that changes keeps the old position, and the footprint is the new one), the physical file's requirements, the placer's and the check's findings.

- **Kind and designator.** The kind that lowering gives a part is its definition's name, `AcmeMicroCoax` (`Instance.type_name`), as for a part written by hand. The designator letters come from `.designator`, which the generic states (`J`) and a provider may narrow. A module provider adds no part of its own; its parts are listed as they are today, with the module's path.
- **Symbol and footprint.** `[[part]] kind = "AcmeMicroCoax" package = "..."` in `kicad.toml`, as today, so a provider needs the entry a hand-written part needs. A fallback to the generic's own kind (`kind = "CoaxReceptacle"` as a shared symbol for providers that draw alike) is possible and is not in stage 1; the mapping call is made in seven places (`schematic.rs`, `fab.rs`, `compare.rs`, `lib.rs` twice, `sourcing.rs`, `glyph.rs`) and the kind would need to carry the generics' names (`Part.provides`) for it.
- **Bill of materials.** Unchanged: a provider's row is a `Pick`, a part with no catalog has none. `BomLine` gains a field `via` (the generic) so that the editor's BOM can say `AcmeMicroCoax (as CoaxReceptacle)`. `fab` is unchanged: an assembled provider with no supplier number is refused by name like any part.
- **Schematic and editor.** A module provider is a sheet like any module, titled with the instance's name and its type; a part provider is drawn from its mapping. The editor's hover on a use shows the pick and the providers passed over, and go-to-definition on the use opens the generic (`crates/netsmith/src/editor/definition.rs`); on a provider it can list the generics it provides. A use with no provider is an error at the line that declares it (`Instance::declared`).

## 7. Passives

**The model.** `Resistor` is the generic `Resistor`: the same members, the same relations, the same `.designator &= "R"`, and, as its own first provider, the pin table with pads and `catalog resistor { ... }` that it has today. The catalog-backed provider is therefore the generic itself, which is what keeps the kind name `Resistor` for the mapping (68 `[[part]]` entries), the bill of materials and every test. A separate named provider (`ResistorRows provides Resistor`) was rejected: its type name would replace `Resistor` in the bill of materials, `std/kicad.toml`, the report text the guide's examples print and the placer's kind lists, which is the behaviour change stage 2 must not make.

The synthesized E-series parts are the same provider's other candidates: `Catalogs::resolve` appends them after the rows, in the order of ADR 0047, as now. The ADR calls this the **synthesized provider**: a provider with no supplier. It stays Rust code (`generic.rs`), because making `.nsm` say "a value of the series, in these packages, at that power rating" is a language choice (section 9). What stage 3 changes is that the part-specific tables inside it come from the generic, not from `match` arms.

The three layers a design can choose between, for one resistor:

```nsm
.r1: Resistor { .resistance &= 10kohm +/- 1% }       // the catalog, then the series: as today
.r1: PrecisionThinFilm { .resistance &= 10kohm +/- 0.1% }   // a provider of Resistor, named
.r1: Resistor { .resistance &= 10kohm +/- 0.1% }     // no standard row or series value is 0.1%: the provider is tried next
```

(`PrecisionThinFilm` is invented: a hand-written part that provides `Resistor` with its own catalog.)

**What stays identical.** For a design that defines no provider of a standard generic: the elaborated model (the same instances, fields, relations, in the same order), the candidates and their order, every pick, `Solution::trials`, every message, the report, the bill of materials, the exported files. The first candidate of a use is the generic itself, so no trial loop runs, no extra elaboration happens, and the existing search runs once.

**What can change.** Only for a design that adds a provider of a standard generic: the pick order (the standard rows and the series first, then the provider, section 4), the messages (a failure mentions the providers that were tried, the way the pick report does), and the cost (a failed passive with several providers is several trials; the existing tests have none). Nothing changes for a project that has no provider. The kind-name tables outside the solver (`netsmith-view/src/layout.rs` and `layout/engine.rs`, which treat `Inductor`, `FerriteBead` and `Fuse` as series parts, and `netsmith-pcb/src/quality.rs`, which finds a capacitor by `kind.starts_with("Capacitor")`) must read the generics a part provides, or a provider with another name would not be treated as an inductor or capacitor: `Part` gets `provides: Vec<String>`, and those three sites check it too. This is in stage 2.

### The three stages

Counts are from this tree (`git status` shows large uncommitted work; counted on its working copy, 2026-11-06). How: `grep -c '^part ' std/parts/*.nsm`; `grep -h '^\s*catalog ' std/parts/*.nsm`; `ls data/jlcpcb/catalog`; `grep -c '^\[\[part\]\]' std/kicad.toml`; test files by `grep -lE '\b(Resistor|Capacitor|Inductor|Led|LinearRegulator|FerriteBead)\b' crates/*/tests/*.rs` (63 files) joined with `grep -li generic crates/*/tests/*.rs` (13 files, 2 more than the first), 65 files in all, and `#[test]` counted in them (1031; an upper bound on the tests that touch the path, since many of these files use a resistor only incidentally); guide blocks by counting the fenced `nsm` blocks of `docs/LANGUAGE.md` (194) and those that mention a standard passive, regulator or `BuckStage` (75); `examples/*.nsm` that mention one (7 of 10).

1. **Stage 1: `provides` and picking, proved on a connector and `BuckConverter`.**
   - Source: about 20 files (13 of them source in 4 crates, the rest the guide, the glossary and skills) and one new module: `netsmith-lang` (`ast.rs`, `parser.rs`, `printer.rs`; `lexer.rs` only if keywords are tokens), `netsmith-check` (`checker.rs`, `model.rs`, `pins.rs`, `lib.rs`, new `providers.rs`), `netsmith` (`lib.rs`, `editor/definition.rs`, `editor/sourcing.rs`), `netsmith-editor/web/src/nsm.ts` and its test with the rebuilt `web-dist` bundle (`scripts/web-bundle update`), the guide, `GLOSSARY.md`, and four skills.
   - Tests: about 40 new (parse 6, checker 14, driver 14, guide 5, editor 2); existing tests changed: none expected, since `generic` and `provides` are read only where a definition starts and after its name.
   - Done when: a connector with two providers, one failing a requirement, picks the other; a `BuckConverter` with two modules picks by the input voltage; forcing by type and by `.mpn` works; none in scope is an error at the use; a provider that does not conform is an error at its definition.
2. **Stage 2: the standard generics as generics.**
   - Source: 4 std files and 12 parts: `passive.nsm` (`Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal`, `Fuse`), `diode.nsm` (`Schottky`, `Zener`, `Tvs`), `transistor.nsm` (`NMosfet`), `regulator.nsm` (`LinearRegulator`): each is `generic X` instead of `part X`, the body unchanged. 0 of 13 catalog files, 0 of 68 mapping entries. Plus `Part.provides` and the three kind-name sites above.
   - The nine connector parts that have a catalog (`Header*`, `Socket*`, `Obd2M`) stay parts: they differ by contact count, and a generic over them would be a design of its own.
   - Tests: about 12 new (a provider added to each of three generics, order, messages). Existing tests changed: 0, which is the acceptance criterion. Proof: all 1031 `#[test]` of the 65 files, the 75 guide blocks and the 10 examples pass unmodified; `netsmith build` of every example and project prints byte-identical output before and after (the comparison ADR 0057 made); and `Solution::trials` is equal on every solver test that asserts it, which is the performance check.
3. **Stage 3: remove the duplicated special cases.**
   - `crates/netsmith-solve/src/generic.rs` (`candidates`: the four catalog names, the `known` column lists and `value_column`; `part_word`), `table.rs` (`OPTIONAL`), `catalog.rs` (`Catalogs::resolve` calling `generic::candidates` by name), `netsmith-check/src/model.rs` (`bank_count` matching `type_name == "Capacitor"` and `std`): five sites in four crates. Each becomes data of the generic or a registry keyed by the generic's name; no `match` on a standard name remains in the solver or the checker.
   - Two of these need a language choice, so they are questions, not steps (section 9): how a column is declared optional, and how a generic says it may be a bank.
   - Tests: existing tests changed 0; about 4 new. Done when `grep -nE '"(resistor|capacitor|inductor|led)"|"Capacitor"' crates/netsmith-solve/src crates/netsmith-check/src` finds nothing outside tests.
   - Implemented on 2026-11-06 (section 14). The grep above is not clean, and section 14 says why: the table that names the four generics is the one place that must, since the synthesized provider is Rust (open question 10), and the one list of banks is kept as open question 9 decided.

## 8. Compatibility

- **Existing projects.** Neither word occurs in the standard library, `examples/` or the one private design that was read for this ADR (a `grep` for both found only prose), and both are read as keywords only where a definition starts and after its name (members are `.name`; `import` and `physical` are read the same way), so a project that has a member or a type with either name is not affected. Whether every example and guide block parses and elaborates as before is a stage 1 test, not something checked here.
- **`.mpn &=` and keyed catalogs.** Unchanged. `catalog mcu by .mpn` on a part still means the design must name the row; `.mpn &=` on a part picks the row; on a generic that declares `.mpn` it picks the provider whose row has the number.
- **The one-row-catalog rule.** A hand-written part still has its own one-row catalog with a `source` (the part number needs a source, `docs/DESIGN-RULES.md`), and the project still lists the file in `netsmith.toml`. A provider needs the same three things as the part it is today: the part, the catalog file in the manifest, and the `kicad.toml` entry. This ADR does not reduce that.
- **`netsmith.toml` catalogs.** Unchanged; a catalog file is read by name as before. A provider's catalog is any catalog name.
- **`kicad.toml` kinds.** The kind of a picked part is the provider's name, so a provider needs a `[[part]]` entry, as a hand-written part does; for the std generics the kind stays `Resistor` and so on. The fallback to the generic's kind is left for later (section 9).
- **Outputs.** The path of an instance does not change by the pick, so the layout file, the physical file and the `#2`, `#3` of a bank stay valid. A pick that changes between two builds changes the part at that path and moves nothing else.
- **Std.** A std generic gains no behaviour until a project defines a provider of it.

## 9. Risks, performance and open questions

**Risks.**

- **The cost of a trial** is a full elaboration and solve (section 5). Measured in stage 1 (section 11): 0.22 s for a 58-part board, so a use with four providers whose first three fail costs under a second, and the cap of 64 combinations is at most about 14 s on a board of that size. A board that takes tens of seconds to solve would make the failing case minutes. Mitigations: the first candidate usually works; the shortcut for unrelated failures; the bound on trials; a later change that varies only the uses a diagnostic names.
- **A provider that is first in order and works but is not the one the author wanted** (the first that works is not the best). The note names the pick, and forcing is one word, but a pick that changes when a file is added is a surprise by the nature of the rule. Adding a file is deliberate and the note makes the change visible.
- **Messages.** A failure inside a provider is reported with the provider's field paths (`reg.controller.vin`), which a board's author has not written. The messages name the provider first.
- **Pick depends on imports**, so two top-level files of one project may pick differently. They are different designs, and the report names the pick.
- **A contract that is too tight or too loose** is the library author's: a relation in a generic fails every provider, and a generic with no relations constrains nothing. The conformance checks catch missing members, not missing meaning.
- **Redeclaring is an error** (section 3): the existing parts of a project need a mechanical edit to become providers (remove the declarations the generic has, turn `.buck: Buck { ... }` into statements). One correct way costs the edit once.
- **`.mpn` on a generic of modules** is not available; a module that wants it declares a value.

**Open questions for the owner, each with a recommendation.**

1. **The word.** `generic` (this ADR), or another? Recommend `generic`: it is the owner's own word for `Resistor` and `UflReceptacle`, and the clash with `interface` is real.
2. **Candidate scope.** The providers in the files the board's file imports, with no list on the board? Recommend yes. The alternative is a manifest list of provider files, which is a third list.
3. **Order between providers.** Name order after std, with no use of the JLCPCB class? Recommend yes for stage 1, and look at the first real use before adding the class.
4. **Providers may be modules, and the generic for a buck converter is `BuckConverter`, not `Buck`.** Recommend yes: an IC alone cannot be a provider, because its other pins need circuit that only it knows, and `Buck` stays the interface `BuckStage` reads.
5. **A provider does not declare the generic's members again.** Recommend strict (an error) for one correct way, and a one-time edit of existing parts. The tolerant alternative (a same-typed redeclaration merges) is easier to adopt and gives two spellings.
6. **The std generics are their own first provider**, so they keep their kind name and every output. Recommend yes. The alternative, a named provider next to a pure generic, is cleaner as a model and changes the kind name and the outputs.
7. **A fallback to the generic's kind in `kicad.toml`** so that providers that draw alike need one entry. Recommend later, with `Part.provides` which stage 2 adds anyway.
8. **The old term.** Keep *generic part* as the synthesized pick, next to *generic*? Recommend keep both for now and rename the old one in a later ADR if the two are confused in use.
9. **Stage 3's two language choices.** How a catalog column is declared optional (today `OPTIONAL` in `table.rs`), and how a generic says it may be a bank (today `type_name == "Capacitor"` and `std`). Recommend neither gets syntax in stage 3: keep them as data keyed by the generic's name in one place each, and add syntax when a second user of either exists.
10. **Where the E-series is declared.** Rust only (recommended), or a line in the generic (`catalog resistor { ... } series`)? Recommend Rust only, because the language has no way to say how a value is generated, and one use does not justify it.
11. **Should a pick be a finding or a note?** Recommend a note, as above. A guideline-style warning (error under `--strict`) when more than one provider works would make every board with a generic noisy.

## 10. What to look at

- The word `generic`, and the sentence in section 2 that says why `interface` could not be extended.
- Section 3's five conformance rules, above all rule 1 (no redeclaring) and the rule that a use reaches only the generic's members.
- Section 4: the candidate set is the import closure; the order has no source-order and no catalog class.
- Section 5: the choice is above `solve`, a trial is a whole solve, and the bound and the shortcut. `Checker::instantiate`, `Checker::visible`, `outcome_with`.
- Section 7: that the std generics are their own provider, and the list of what stays identical.
- That `BuckConverter` is a module-shaped generic next to the unchanged `Buck` interface.
- The three stages' counts, and the criterion of stage 2 (no existing test edited).
- `GLOSSARY.md` is not edited by this ADR. The entries it proposes: **Generic**, **Provider**, **Use of a generic**; widened: **Candidate**, **Interface** (a member of an interface type is a port; what picks a part is a generic), **Generic part** (what the synthesized provider makes).

## 11. Stage 1 as built (2026-11-06)

Everything in sections 3 to 6 that concerns parts and modules as providers, the use, the candidate rule and the pick is built, with the syntax of the summary. Stages 2 and 3 (the standard passives as generics, the special cases removed) are not started. This section is what was measured, what was built in each place, and each place where the code differs from the text above, with the reason.

### What was measured first

An invented board of 58 parts (14 LED branches of a resistor and an LED, 20 decoupling capacitors, three LDO stages of a regulator and two capacitors, a supply and a header), release build, one thread: `check` (parse and elaborate the whole file) 2 ms, `solve` 215 to 236 ms, `lower` 0.3 ms, `outcome_with` with no KiCad libraries 218 ms. A trial is therefore one solve and costs what a build costs; the loop is usable. The 64-combination bound is at most about 14 s on a board of that size, which was judged acceptable, and no change to the design was needed. `examples/schematic_bench` (532 lines) and `examples/enclosure_sensor` build in under a second each. The example that measures it is `crates/netsmith/examples/trial_cost.rs`.

The other checks of the plan, as run: the elaboration is pure (the same board checked twice, and in one run beside another board, gives models that are equal when printed; a test); `Checker::modules` for one root holds every file once, a cyclic import included, and two roots are two `Checker`s with nothing shared; `visible` had no restriction on the members of a part, which is why the rule for a use is new code and the physical file (which returns before it) and the built-in members (`assembly`, `designator`, `height`, `pin_protrusion`, `place`) are exempt; a generic's pin row with no name and no pads is stored as a row with an empty `pads` (every row of a part has at least one pad, so the two cannot be mistaken); a module's pins have no kind, so a generic that has a `pins` table is provided by parts only, which the checker says.

### What was built, by place

- **Language** (`netsmith-lang`): `DefKind::Generic` (`generic`), `Definition.provides` (a list of names after `provides`, only on a `part` or `module`; any other kind is a parse error), a pin row of a generic that may stop after its kind (and `required` or `optional`), and the printer for both. Neither word is read anywhere but where a definition starts and after its name, so `.generic` and `.provides` stay ordinary members and a definition may still be called either.
- **Checker** (`netsmith-check`): `Instance.provides`, `Instance.placed_as`; `Model.uses` (a `GenericUse` for each member declared with a generic's type: its path, the candidates in order, the one chosen); `check_with` and `check_one` taking an `Assignment`; a new `providers.rs` for the candidate scan, the order, the conformance checks and the use. A provider's instance is made from the generic's members, then its own, and the generic's statements are read before the provider's, each in its own file.
- **Driver** (`netsmith`): `providers.rs` holds the search; `outcome_with` calls it only when the first solve fails and a use has more than one provider. `Built.providers` and a `note:` line for each pick. The bill of materials line has `via`.
- **Editor**: `provides` and `generic` are keywords of the highlighter; go to definition on a provider's header name opens the generic (it already worked on a use's type and on a member through a use, because a use's declared type is the generic); the bill of materials says `as CoaxReceptacle` under the value of a picked part.

### Where the code differs from the text above

1. **Uses are in `Model`, not in `Board`** (section 5 said `Board.uses`). A system and a panel are models too, and a system's boards hold uses; one place serves all. `netsmith_check::GenericUse` is the type, as proposed.
2. **The search varies each use alone first, not only the first** (section 5 said "the first use with more than one candidate"). With several uses, an unrelated mistake that does not come from the first would have been given up on. Each use that has several providers is varied alone over its other providers; a use none of whose other providers changes the diagnostics is left at its first provider for the depth-first search; if that search finds nothing and every combination still fits in the bound, the left-out uses are searched too. The pick is still the first assignment in the order of section 5 for which nothing fails, so it is a function of the sources. The cost is the sum of the providers of the uses, not the product (a test: two uses of three providers and an unrelated failure cost four trials, not eight).
3. **A trial is a check of one definition** (`check_one`), not of the whole file, and trials are not repeated for the same choices.
4. **A provider that is not instantiated is checked where it is written**, not where it is not used: the conformance rules (rules 1, 2 and 5 of section 3) run whenever a provider is made, and a provider that no board in the file reaches is read only when its own file is checked. The mixed part-and-module rule is checked for every candidate of a use, at the later provider's own name. So a mistake in a provider that is in scope and not picked is reported when its file is checked, not when a board is built, unless the search tries it.
5. **`required` and `optional` of the generic's pin reach the provider's row** (they were not stated in section 3): a provider may leave the word out and the generic's holds; a provider that says the opposite is refused. A pin that is an alias of another does not carry the word, because the alias has no row; the pin it is joined to keeps its own.
6. **The message when nothing provides the generic** does not name a provider file, because the checker knows only the files that were loaded: ``` `reg` is a `BuckConverter`, and no part or module in the files this file imports provides it; import the file of a provider, or name one as the type of `reg` (a part or module declared with `provides BuckConverter`) ```.
7. **The message when no provider works** carries the closest trial's first failure, which for a provider with a catalog is the catalog's own message nested in it (`... fails because no `acme_micro_coax` candidate satisfies `rf`: the closest of the 1 that fit, ACME-MC-1, fails because rf.max_frequency must always be at least 10GHz ...`). The others follow as `; the others: X (why), Y (why)`. With one provider in scope the message is the part's own, exactly as when it is written by its type.
8. **The example of section 3 had a mistake**: a doc comment (`///`) before the statement `.vin.peak <= .vin_abs_max` is refused (doc comments attach to declarations). It is a `//` comment above.
9. **A generic with a pin table or a catalog is its own first provider** (section 2), and the code already lists it first and instantiates it as written, so stage 2 needs no new rule here. A note is not printed for a pick of the generic itself. No standard definition is a generic in stage 1, and the test for it uses an invented one.
10. **A provider's id is `module::Name`** where the module is the file's name without `.nsm` and with `::` for `/` (`src::parts::acme_micro_coax::AcmeMicroCoax`; for the file `check` is given, its own name). That is what an `Assignment` names.
11. **The alias example of section 3 is a tested block of the guide** (`OtherCoax`, with a catalog so that it is complete), and a test.

12. **A provider that cannot be judged does not hold** (widened by section 13, item 6) (accepted by the owner on 2026-11-06; section 4 said the first for which "every check of the whole design holds", and the solver, as ADR 0057 left it, passes a bound whose value nothing states with a note `not evaluated`). A provider that is put in a use's place and leaves a value of the use open (`.max_frequency` never stated) while a bound names it would be picked first by name and the bound would stay unchecked. Soundness (ADR 0053, ADR 0057) is that a pick never rests on a check that was not made, so such a trial counts as unjudged: a provider that can be judged is preferred, and when none can the first in order stands and the bound is named in the notes as before. Only a bound that names a value of the use itself that is still unset counts; a bound that waits on something else (a rail's `.peak` that nothing states, a value a provider's own parts work out) is the design's, is named as before, and does not start a search. The cost is the same one extra trial for each other provider of such a use, and none for a design whose bounds are all judged.

### Left open, as before

The nine questions of section 9 stand as decided there. Not done and not in stage 1: the fallback to the generic's own kind in `kicad.toml` (a provider needs its own entry, and the build says so naming it); a use that names a generic inside a generic; a trial per use subtree (only the cost of the failing case would change).

## 12. Stage 2 as built (2026-11-06)

The twelve standard definitions of section 7 are `generic`s that are their own first provider: `Resistor`, `Capacitor`, `Led`, `Inductor`, `FerriteBead`, `Crystal` and `Fuse` (`passive.nsm`), `Schottky`, `Zener` and `Tvs` (`diode.nsm`), `NMosfet` (`transistor.nsm`) and `LinearRegulator` (`regulator.nsm`). In each file the change is the keyword and nothing else (not a line added or removed, so the `std/parts/passive.nsm:248:5` of a message and of the guide are the same). `Diode`, `PMosfet` and `Switch` have no catalog and stay `part`s, as section 7 lists. (Later, ADR 0063: `PMosfet` became a generic with the catalog `p_mosfet`, which has no standard row, and `LevelTranslator` is a generic of the same kind; `Diode` and `Switch` stay parts.) Stage 3 was not started then; section 14 is what it did.

### The proof that nothing changed

Taken before the first keyword was changed, with the release binary of the tree as it was, and repeated after each part (the build of the examples on the debug binary of the tree, and once more at the end on the release binary that is kept):

- `netsmith build` of the eight `examples/*.nsm` (also with `--strict`) and of `examples/enclosure_sensor` and `examples/schematic_bench` as projects: standard output, standard error, exit status and every file written, 89 files, byte for byte equal after `NMosfet`, after the three diodes, after `LinearRegulator`, after `Fuse` and `Crystal`, and after the other five passives. (`carrier_and_cards` and `schematic_bench` end in an error of the sample libraries both before and after; the error text is among the files compared.)
- For 117 boards and systems, those of the guide's `nsm` blocks that check, and the examples: `Solution::trials`, the picks (a hash of path, row and count), the number and text of the findings, the numbers of unresolved, deferred and not-evaluated entries, and the elaborated model before the solve (printed with `{:#?}`, without source offsets, and without the new fields `uses` and `provides_std`). Equal after every one of the twelve conversions, taken one at a time. The first run of `Resistor` differed in one block (a panel that holds a resistor, which said `a generic`), and the fix is item 2 below. The check ran on a temporary test that is not kept.
- A kept test, `crates/netsmith/tests/std_generics.rs`, records the picks and `Solution::trials` of thirteen boards that use every converted kind (the examples led, status, linear regulator and two boards; a fuse and a TVS; a Schottky with a capacitor; a Zener; two N-channel MOSFETs, one failing; an inductor, a ferrite bead and a crystal; an inductor over its saturation current; a capacitor bank; a resistor no row serves). The expected text was printed by the tree with the twelve written `part` again, then pasted. It passes on the generics.
- A solve of a 58-part board (`examples`-sized, `crates/netsmith/examples/trial_cost.rs`, release): `check` 2.1 to 2.8 ms (2 ms in section 11), `solve` about 250 ms. A use now records a `GenericUse` and finds its providers, which costs nothing that shows.

### What was built, by place

- **Std** (`std/parts/*.nsm`): the keyword of the twelve.
- **Checker**: a provider reads a standard generic's items differently from a generic that has no pads (`generic_items`): the `catalog` is not read into it, and the `pins` tables (one for each package, for `LinearRegulator`) are read as one table of kinds with no names and no pads, which is what `conform` compares the provider's rows with. Everything else of the generic (members, `bridge`, relations, narrowings) is read as stage 1 reads it. `Instance.provides_std` names the standard generics a definition provides.
- **Lowering and core**: `Part.provides` (the generic names the definition provides; empty for a standard part), set by `netsmith-lower`.
- **View and placer**: `netsmith-view` (`layout.rs` and `layout/engine.rs`, through `is_series_kind`) takes a part for an inductor, ferrite bead or fuse when its kind or what it provides is one; `netsmith-pcb` (`quality.rs`, through `Placed.provides`) takes a part for a capacitor the same way.
- **Driver**: `Built.providers` has no entry for a use whose first provider (the generic itself) was kept.

### Where the code differs from section 7

1. **A provider of a standard generic does not read the generic's catalog and pads.** Section 7 says the generic is its own first provider and says nothing about the others. A provider that read the generic's `catalog resistor { ... }` and `pins` table would have two catalogs and a pin table that is not its own. So a provider reads the generic's pins as kinds only (`.a passive`, `.b passive`), and has the one catalog it writes. It does not declare `.package: Package`, because the generic does (rule 1, strict): a hand-written part that becomes a provider drops that line. A provider cannot state other designator letters than the generic's (rule 4: `.designator &= "RN"` against `"R"` is a contradiction). A module cannot provide a standard generic (it has a `pins` table; stage 1 already said so).
2. **A message that says what a definition is says `part` for a generic that has pads or a catalog.** `a system holds boards, but `Resistor` is a part`, `a panel holds ... but `Resistor` is a part`, ``Resistor` is a part, not an enum``, ``r` is a part (`Resistor`)` (in a generic): they are what the existing tests and the guide print, and a standard generic is placed as a part. A generic with neither says `generic`.
3. **`Built.providers` and the BOM `via` say nothing for a use that kept its own generic.** Section 11 item 9 says no note is printed for it; the list was not empty. It is, for a design that adds no provider.
4. **A use whose first provider is the generic itself is not "unjudged"** (section 11 item 12). The rule made a build that did not fail try every other provider of every use that left a value open after the pick: a resistor with no stated current leaves its rating check not evaluated, which is the design's, whichever provider it is. With one provider of `Resistor` in scope and thirty such resistors the build took 36.9 s instead of 0.5 s (63 trials, measured on the 58-part board with thirty more). A catalog row sets every value its catalog lists, so what is open after the pick of the generic itself is what the design supplies; for a provider written by hand an open value is still its own, and the rule is unchanged for it. A design that does fail still tries the other providers of each use, as before (about a solve for each use, at most 64).
5. **A provider of the standard `Capacitor` is a bank** (`Model::bank_count`, section 7's stage 3 item, touched here): without it `.count &= 2` on a provider of `Capacitor` would have been ignored without a word. The solver and the lowering already read `bank_count` and nothing else; the condition is that the part provides a standard generic called `Capacitor` (`Instance.provides_std`). A generic of the project that is merely called `Capacitor` does not make a bank. The rest of stage 3 (the four catalog names of `generic.rs`, `OPTIONAL` of `table.rs`, the `match` on the standard name in `Catalogs::resolve`) is untouched, and so is the synthesized E-series provider, which stays Rust-only (open question 10); it is found by the catalog name `resistor` and so on, so a provider with a catalog of another name has no synthesized candidates. (Stage 3, section 14, left every one of these facts as they are and put the names in one table.)
6. **`Part.provides` is not all of `placed_as`.** It is the generics the definition provides, by name, whether the part was placed by its own type or through a use. The view and the placer read it; they do not read a use's pick. A user generic that is called `Inductor` is read as one by name, as a user part called `Inductor` was by kind.
7. **Tests that had to change** (no assertion about behaviour was changed): `netsmith-lower` `std_parts` reads `generic ` as well as `part ` (it scans the text of `std/`); `netsmith-lang` `passive_parts_parse` counts seven generics and eight definitions of either kind; `netsmith` `editor.rs` looks for the line `generic Resistor` (it asked for `part Resistor`); `netsmith-check` `only_a_generic_is_provided` uses `Diode`, a part of the standard library that is not a generic, where it used `Resistor`, which now is one; and the 31 literals of `Part { ... }` in 19 files (13 in the unit tests of `netsmith-core`, `netsmith-kicad`, `netsmith-pcb` and `netsmith-view`, 18 in the integration tests of the last two) have `provides: Vec::new()`: a field was added to a struct that tests build by hand, which only that edit can answer. All the other tests are as they were.
8. **The guide's `nsm` blocks and every example build print what they printed.** The guide gained one subsection (_The standard passives are generics_, with an invented `PrecisionThinFilm` and its catalog, a block that runs, and a note on the rules of a provider of a standard generic) and a line before the standard library's table of parts.

### Tests added

`crates/netsmith-check/tests/std_generics.rs` (16): the twelve are generics and the rest are parts; each is placed as itself, a part of its own name with its catalog and tables, one candidate; a provider of each of the twelve (invented, with a pin table and nothing else) conforms and is the second candidate; a provider has the generic's members, `bridge` and statements and not its catalog or pads; a wrong kind, a missing pin and no table are refused with the stage 1 messages; `.resistance` and `.package` declared again are refused; other designator letters are a contradiction; an alias and an equation map a name; the interface pins of `LinearRegulator`; a module provider is refused; the order is the generic, then module path and name, never the import order; a provider in a file that is not imported is not a candidate; a use reaches only the generic's members; bank for a provider of `Capacitor` and not for a project's own generic of that name; the words of the messages.
`crates/netsmith/tests/std_generics.rs` (9): the picks and trials of thirteen boards as they were; a provider of `Resistor` picked for a resistor no row serves (the note, `Part.provides`, the row's number); a provider in scope changes nothing while the rows serve (no trial, same design); thirty resistors with no current cost no trial with a provider in scope; the failure without the provider is the same text; the provider named by its type is not picked; the failure with a provider that also cannot serve names every provider; a provider of `LinearRegulator` picked for a load the standard row cannot carry and not picked at 50 mA; a provider of `Capacitor` as a bank of three parts.
Unit tests: `is_series_kind` (`netsmith-view`), `is_capacitor_kind` (`netsmith-pcb`).

### Left open

The fallback to a generic's kind in `kicad.toml` (a provider of `Resistor` needs its own `[[part]]` entry; open question 7); the rest of stage 3 (done in section 14); a provider of a standard generic that is a module (refused). A project that adds a provider of `Resistor` pays one solve for each use in a design that fails, as in section 11.

## 13. The second independent review, as fixed (2026-11-06)

A second reading of stages 1 and 2 (nothing in it was run) found nine faults; the owner decided that none stays half done. Each is fixed here, with the decision where there was one. The numbers are invented, as everywhere in the tests.

### 1. A provider that contains a use of its own generic

`module AFilteredBuck provides BuckConverter { .inner: BuckConverter ... }` takes provider 0 for `.inner`, and when it sorts first that is itself: the elaboration never ended and the process died of a stack overflow (the editor's server with it). The wrapper pattern of section 3 is a module that wraps a foreign part, not the generic it provides.

Decision: refuse, at the use inside the provider, naming the cycle. The checker keeps the stack of the definitions it is making (`Checker::making`); a member whose type is a definition already on the stack, or a generic one of whose providers is on it, is an error and a poisoned member, and nothing is made. Whatever the order of the providers: a pick that *can* take the provider being made is what is refused, not only one that does.

```text
error: `inner` is a `BuckConverter`, and `AFilteredBuck`, which is being made here, can be the provider picked for it, so `AFilteredBuck` would contain itself without end (the cycle: `AFilteredBuck` > `AFilteredBuck`); a provider cannot contain a use of the generic it provides, directly or through the parts and modules inside it: wrap a part, or a module that does not provide `BuckConverter`
```

A definition that holds itself (`module Ring { .inner: Ring }`) says ``` `inner` would make `Ring` inside itself without end (the cycle: `Ring` > `Ring`) ```, and a cycle through two generics names each definition (`P1` > `P2` > `P1`).

Rejected: leaving the provider on the stack out of the candidates of the inner use (the other way the review offered). It makes a wrapper valid or not by how its name sorts (`Zwrap` would wrap `Zbk9Buck`; `AFilteredBuck` would be refused or silently take another provider), which is the fault of finding 3 again. Whoever wants an adapter around a foreign part writes a module around that part.

Where it is seen: when the board that picks the provider is built, and when the provider's own file is the root of a check, which elaborates every definition of it (the editor checks each module of a project as a root as well). A cycle that a pick would not take (a provider of a generic that no board in scope picks, an indirect cycle whose providers sort so that the elaboration never enters it) is seen when the elaboration enters it, like any other mistake in a provider that is not picked (section 11 item 4). Tests: `crates/netsmith-check/tests/generics.rs` (the wrapper sorting first, the same wrapper sorting last and read as the root, a cycle through two generics, a definition in itself) and `crates/netsmith/tests/providers_editor.rs` (the editor reports it and does not die). Run against the code before the fix, the first and the cycle tests abort the process.

### 2. A failing design with many uses hid its own error behind the bound

With more than about 63 uses that have several providers and a failure that no provider changes (an LDO row nobody has), the alone-variation of section 11 item 2 spent trial after trial, reached `MAX_PROVIDER_TRIALS` before it had varied every use, and the build said only `gave up after trying 64 provider combinations for ...`: the real error of the first solve was never shown. Two changes:

- When the search stops at the bound without an answer, the build prints the diagnostics of the first solve first, and then the bound's message. That message names the first six uses it did not reach and counts the rest (`... for `c00` and `c01` and `c02` and `c03` and `c04` and `c05` and 51 more; ...`), not all of them. When the search ran to its end the closest provider is named as before (that is the answer, not an absence of one).
- A trial that passes while a use is varied alone is kept (`Search::passes`) and taken when the search in order reaches the same assignment, where it used to be dropped and solved again, counting twice against the bound.

Measured on an invented board of 82 uses with two providers each (40 resistors, 40 capacitors, a project provider of each; one LDO whose row no catalog has), release builds, one run after the other on a loaded machine: the snapshot before this change 34.1 s and 34.5 s (23.8 s on a quieter run), ending only in the bound's message with all 63 uses it had not reached named; after, 36.5 s and 34.3 s, ending in `the `ldo` catalog has nothing for `ldo`, which needs vout.voltage within 3.2996V..3.3004V` and then the bound's message with 6 uses named and 51 counted. Both stop at the bound after 63 trials, so the time is the same: the fix is in what is said, not in the cost.

Not changed: the cost. Every use with a second provider is still varied alone, one whole solve each, up to the bound. Varying fewer uses (only those a diagnostic names) would give up designs whose cause the diagnostic does not name (a rail's voltage that a provider sets), so it is not done; the bound is what stops a long wait, and what it says is now true. The double count showed in `a_pass_found_while_a_use_is_varied_alone_is_not_solved_again`: two assignments are two trials, where the same design cost three.

### 3 and 4. The root file is one module, whatever its name

The module path of a file (`src::main`) is the second key of the order of section 4 and the start of a provider's id, and the root's was its name as typed: `src/main.nsm` and `./src/main.nsm` were `src::main` and `.::src::main`, which sort differently, so the pick between a provider in the root and one imported depended on the spelling. And the root was not held in `Checker::cache`, so a file that imports the root (a cycle through it) read a second copy, with generics that are other generics, and the providers in that file never reached a use written in the root, though section 11 says every file is held once.

Now the root is read as the module it is: `Loader::root_key` finds the module of the project whose file name is the typed name with its `.` and `..` steps taken out, or the key a tool attached to the spelling (`Target::loader` attaches the project's key for each top-level file it is given, by the canonical path, so an absolute path is `src::main` too), and the root is cached under that key like every other file. A root the loader does not hold takes its key from its name with the `.` steps taken out and is not cached, since nothing can import it. Tests: `crates/netsmith-check/tests/generics.rs` (four spellings, with and without the file among the modules, and a key attached to an absolute name; the cycle through the root) and `crates/netsmith/tests/root_spelling.rs` (a project built under four spellings of its file's absolute path).

### 5. The bill of materials line of a bank

A provider of `Capacitor` is a bank when the design says how many (section 12 item 5): the first capacitor has the use's path and the others `bulk#2`, `bulk#3`, so the editor's lookup of the use (`via`) found it for the first only, and one bank printed as two lines. The lookup now takes the use's path from the part's by removing the `#k` of a bank. Test: `providers_editor.rs`, a bank of three picked as a provider is one line of three with `via`.

### 6. What cannot be judged

Section 11 item 12 looked at the fields the use itself owns, and at deferred relations only. Its reason, that a pick never rests on a check that was not made, is wider than that, so a trial of a use whose pick is not the generic itself is also unjudged when:

- a deferred bound or equation reads a value that is unset and lies inside the use, in an interface or a part of it that nothing outside the use shares: the datasheet number of the IC inside a module provider (`reg.controller.max_vin`) is the provider's, where it was said to be the design's; or
- a logic level or an absolute maximum could not be checked (`Solution::not_evaluated`) because a `Logic`, or the part, inside the use has no value for it (`NotEvaluated::involves`, new, names the instances the check was about).

What stays the design's: a value inside the use that is one and the same value as one outside it (`Model::equalities`, followed through the use), such as the `.peak` of the board's rail that `.vin` is joined to and that the board leaves open. Every provider leaves that as it is, so it does not start a search (the test `the_same_converter_at_five_volts_takes_the_first_ic` is unchanged and still costs no trial). The review's sentence counted every unset field inside the use, `reg.vin.peak` included; that would have tried every provider of every `BuckConverter` on a rail with no stated peak, for nothing, so the shared values are left out. The silent case of `levels.rs`, a pair of pins of which neither side states any level, is not a check anyone asked for, so it is not counted. The circle records (`open_circles`, `circular`) are made of deferred relations, so a circle that reads an open value inside the use is found by the first rule; no list of its own is needed. The exemption for a use whose chosen provider is the generic itself (section 12 item 4) is as it was, and so is the cost: one more trial for each other provider of such a use, up to the bound. Tests: `providers.rs` (`a_provider_whose_logic_levels_are_open_is_passed_over_for_one_that_states_them`, `a_module_provider_whose_inner_part_leaves_a_datasheet_number_open_is_passed_over`).

### 7. An alias carries the generic's kind

`.signal ~ .rf` replaced the kind of `.rf`'s row by the generic's own before comparing, so an alias was accepted with any kind, and the board met "two outputs on one net" instead of the provider being refused. The row the alias is joined to is compared, and the message names it: ``` `.signal` is a passive pin in `CoaxReceptacle`, which `OtherCoax` provides, but the row of `.rf`, which `.signal` is joined to, says an output; ... ```.

### 8. Providers with one name

`src::vendor_a::Coax` and `src::vendor_b::Coax` would lower to one kind (`Coax`), one `[[part]]` entry and one name in the notes. Refused at the later one's name, in its file: ``` `Coax` is a provider of `CoaxReceptacle` in `src::vendor_b` and is also one in `src::vendor_a::Coax`; two providers of one generic need different names, because a part is known by its name in the bill of materials, in `kicad.toml` and in the notes: rename one ```. Rejected: qualifying the kind with the module path (`vendor_b::Coax`): it changes every kind name, the mapping and the bill of materials for a case the author controls.

### 9. The draw of a pull-up

Recorded in ADR 0070 (the note of 2026-11-06): two offered members, `.overdrive` (0 V) and `.supply_top` (`supply.voltage`), for `PullUp` and `I2cPullups`.

### The proof that nothing changed

`netsmith build` of the eight `examples/*.nsm`, also with `--strict`, and of `examples/enclosure_sensor` and `examples/schematic_bench` as projects (89 files: standard output, standard error, exit status and every file written), release binary of this tree against the snapshot taken before it: identical, no difference. `enclosure_sensor` uses `I2cPullups`, so the new equation of finding 9 is covered. The changes of behaviour that exist are those of the findings: the message of the bound (finding 2), and what a design that has the mistakes of findings 1, 3, 4, 7 and 8 is told.

## 14. Stage 3 as built (2026-11-06)

Stage 3 took away the places that read the standard generics by a name or a list of their own. The two language questions it depended on were decided as recommended (section 9, questions 9 and 10): neither the columns a file may leave out nor the banks got syntax, and the synthesized E-series provider is still Rust. Nothing a design, a build or a test prints changed, and no test was edited. The sites, with what was done to each:

### Every special case found, and what became of it

How they were found: `grep` over `crates/*/src` for the kind names (`"Resistor"`, `"Capacitor"`, `"Inductor"`, `"Led"`, `"FerriteBead"`, `"Crystal"`, `"Fuse"`, `"NMosfet"`, `"PMosfet"`, `"Schottky"`, `"Zener"`, `"Tvs"`, `"LinearRegulator"`, `"LevelTranslator"`) and the catalog names (`"resistor"`, `"capacitor"`, `"inductor"`, `"led"`, `"ldo"`, `"schottky"`, `"zener"`, `"tvs"`, `"ferrite_bead"`, `"fuse"`, `"n_mosfet"`, `"p_mosfet"`, `"crystal"`, `"header"`, `"level_translator"`), then `type_name ==`, `kind.starts_with(`, `kind ==` and `matches!` on a kind, then every file that mentions a standard name outside a test. The hits in tests, in comments and in the KiCad footprint families of `netsmith-pcb/src/generate/chip.rs` (library names such as `Resistor_SMD`, which have nothing to do with a generic) are left out of the list.

**Removed.**

1. `netsmith-solve/src/generic.rs`, `candidates`: a `match` on the four catalog names for the columns a synthesized part can fill (`known`), another for the value column, a third to choose the function that makes the parts. Now `Standard::of_catalog(name)` finds the entry, the entry's columns are the ones the synthesized provider can fill, its marked column is the value column, and its function makes the parts. Where there is no entry (`ldo`, `header`, a project's own catalog) there is no synthesized part, as before.
2. `generic.rs`, the four functions that make the parts (`resistors`, `capacitors`, `inductors`, `leds`): each had a `match` on column names (`"resistance"`, `"max_power"`, `"temperature_coefficient"`, `"effective_capacitance"`, `"dielectric"`, `"rms_current"`, `"color"`, ...). They now match on how the column is filled (`Fill`: the marked value, the package's power, no curve, open for the design, the package, nothing), which the entry states for each column once. A column with a fill the function cannot make returns no parts, and a test makes the parts of each from the columns that the standard library's own `catalog` statement lists.
3. `generic.rs`, `kinds`: the column kinds a row is read with when a generic part is resolved, a `match` on the catalog name with the resistor's as the default. Now the entry's (`Standard::of_spec(spec).kinds()`; the default no longer exists, since a `Generic` is made only for the four).
4. `generic.rs`, `part_word` (what a message calls the part) and `Generic::value_column`: a `match` on the catalog name and one on the `Spec`. Now the entry's `word` and `marked`.
5. `netsmith-solve/src/table.rs`, `OPTIONAL` (`("resistor", "temperature_coefficient")`, `("inductor", "rms_current")`, `("capacitor", "dielectric")`): **derived**, not kept. They are the columns the synthesized provider leaves empty, and they must be the same ones: a synthesized part is empty there, so a row that may not be empty there would leave the synthesized part with no design it fits. The list was the same fact written twice. `is_optional` asks the entry whether it leaves the column empty. This needed no syntax (question 9 allowed the list to stay if it could not be derived; it could, from the one table that already has to say what the synthesized provider leaves out).
6. `table.rs`, `VALUE_COLUMNS` (ADR 0065: `resistance`, `capacitance` and `effective_capacitance`, `inductance`): a flag on the entry's column (`value`).
7. `netsmith-check/src/model.rs`, `bank_count`: a test of `type_name == "Capacitor"` together with the `std` flag of the instance, and a second test of `provides_std`. A standard generic is its own first provider, so it now provides itself: `Instance.standard` names the generics of the standard library that a definition is (when it is a standard generic) or provides, and `std` and `provides_std` are gone. The type-name test no longer exists.

**Kept, with the reason.**

8. `standard.rs` (new) is the one place in the solver that names the four generics and their catalogs. The synthesized provider is Rust (question 10), so some Rust must say which generics it serves; what stage 3 did is make that one table and not seven lists. Its entries are checked against the definitions: a test elaborates `Resistor`, `Capacitor`, `Led` and `Inductor` from the standard library and compares the catalog name and the columns, in order, with the entry. The entry is found by the **catalog's** name, not the generic's, on purpose: a catalog is one name shared by every part that lists it (ADR 0022, `resistor.csv` is read by every part with `catalog resistor`), so the columns a file may leave out and the series belong to the catalog, and a project's own part that lists `catalog resistor` has always had both. Keying by the generic would have taken both away from it, which is a change in behaviour; `crates/netsmith-solve/tests/joint_search.rs` and the record below have such a part. (ADR 0052 rejected the catalog's name for the bank, because the bank is a property of the part. These three are properties of the catalog.)
9. `generic.rs`: the `Spec` enum, `Need`, `Sense`, the package and tolerance tables of each series (`PACKAGES`, `CAPACITOR_TOLERANCES`, ...), `Series::for_reactive`, `with_needs`, `misses` and `nearest`. This is the synthesized provider itself, which stays Rust.
10. `model.rs`, `BANKS`: the one list of the generics that are banks, `["Capacitor"]`. Question 9 decided that a bank gets no syntax and stays data keyed by the generic's name in one place. A provider of `Capacitor` is a bank through `Instance.standard`, as in stage 2. One way to derive it without syntax exists and is not taken, since it is a choice about the language: a standard generic that declares a numeric `.count` is a bank (today only `Capacitor` does). It would make `Capacitor` the first of a rule and not an exception; the owner may want it, or the marker ADR 0052 rejected.
11. `netsmith-view/src/layout.rs`, `is_series_kind` (`"Inductor" | "FerriteBead" | "Fuse"`), and `netsmith-pcb/src/quality.rs`, `is_capacitor_kind` (`starts_with("Capacitor")`): each reads the kind **and** `Part.provides`. The kind half cannot go without changing what is drawn or placed: the unit tests of both crates build a `Part` by hand with a kind and no `provides`, and a project's own part called `Inductor` has been drawn in series by its kind since before generics (stage 2 item 6). `Part.provides` is empty for a standard part, so lowering would have to list the generic itself in it for the kind half to be dropped, and the hand-built parts would still carry only a kind. The two lists are about different things (what a layout hangs in series, what the placer calls a decoupling capacitor) and not the same fact twice.
12. `netsmith-lower/src/lib.rs`, `default_value`: `"Led"` shows `LED` as the value of an LED the solver did not pick. It is a display default that the language cannot declare (a part has no member for what its value shows), and a provider of `Led` shows its own name today; reading `provides` would change that line of a bill of materials for a provider. It is also the only `Led` left outside the standard library's and the solver's tables.

**Looked at and not a special case of a generic.** `hand_soldered` and `is_connector` (the prefixes `Header`, `Socket`, `Connector`), `is_mechanical` (`MountingHole`, `PlatedHole`, `Fiducial`) and `Label`: the nine connector parts and the mechanical parts are parts, not generics (section 7 keeps them so). `Mating`, `Logic`, `Place`, `Outline`, `Cutout`, `Keepout` and `I2c` in the checker and the solver are interfaces of the standard library read by name, and `levels.rs` finds a resistor by its shape (a `bridge` and a `resistance`), not its name. `init.rs` `TEMPLATE_IMPORTS` lists what the starter board imports, and a test compares it with the template. The mapping of `std/kicad.toml` is data keyed by kind and is the way a provider is drawn (open question 7).

The check of the plan, `grep -nE '"(resistor|capacitor|inductor|led)"|"Capacitor"' crates/netsmith-solve/src crates/netsmith-check/src`, finds `standard.rs` (items 8 and 9: the table, and its tests) and `BANKS` (item 10) and nothing else outside tests. It could not be empty: Rust that serves a generic names it once.

### The proof that nothing changed

Taken before the first change with the debug binary of the tree as it was (`netsmith build`; the second run of the same binary is byte-identical, so nothing in the output depends on the run), and again after each change:

- `netsmith build` of the eight `examples/*.nsm` (also with `--strict`) and of `examples/enclosure_sensor` and `examples/schematic_bench` as projects: standard output, standard error, exit status and every file written, 89 files. Byte for byte equal after the table of the solver (items 1 to 6) and after `Instance.standard` (item 7), and at the end.
- The new test `crates/netsmith/tests/std_catalogs.rs` records, for ten boards and three series (none, E24, E96), and for four of them with a project's own file of `resistor` rows that has none of the optional columns: `Solution::trials`, each pick with its row, label and the generic part's description, what `resolve` finds for each generic part and the nearest rows `alternatives` offers, the findings (count and a hash of the text), the unresolved parts and why, the deferred and not-evaluated counts, and the unconstrained picks of ADR 0065 (38 lines). The text was printed by the tree before the change and the tree after it prints the same. It covers a resistor, a capacitor, an inductor and an LED with no row, a column that a file leaves out and one asked for, a rating no row has, and a project's part that lists `catalog resistor`.
- A temporary test that is not kept recorded `Solution::trials`, the picks (a hash of each path, row, count, label and generic part), the findings (count and text hash), the unresolved, deferred and not-evaluated entries and the findings of ADR 0065 for every board and system of the guide's `nsm` blocks (218 blocks; 74 of them do not check on their own and are recorded as that), each with no series, E24 and E96 (448 records). Equal after the table of the solver and after `Instance.standard`.
- The kept test `crates/netsmith/tests/std_generics.rs` (the picks and `Solution::trials` of thirteen boards) and the guide's own test pass unmodified.

### Tests added

`crates/netsmith-solve/src/standard.rs` (5): each entry names the catalog and the columns of its standard generic's `catalog` statement, in order; each entry makes parts for exactly those columns, and a synthesized part is empty where, and only where, a row may be; a part that lists a column the entry cannot fill, or lacks the one the series is made of, gets none; the optional columns and the value columns are the ones ADR 0055, ADR 0058 and ADR 0065 name; the entries are found by catalog name, and only the four have one. `crates/netsmith/tests/std_catalogs.rs` (1, the record above). Existing tests changed: none.

### Where the code differs from section 7, and what is left

- Section 7 and the plan of stage 3 said "a registry keyed by the generic's name". The table is found by the catalog's name (item 8), and carries the generic's name for the test; the reason is behaviour.
- `Catalogs::resolve` in `catalog.rs` was listed as a site. It held no name: it calls `generic::candidates` with the part's catalog name, and that function was the site (item 1). It is unchanged.
- Left, as section 9 decided: syntax for a bank, syntax for an optional column, the E-series in the language. Left open, with the options in item 10: whether a standard generic that declares `.count` is a bank without a list.
