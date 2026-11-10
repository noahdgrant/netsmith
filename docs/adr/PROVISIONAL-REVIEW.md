# The provisional ADRs: what is open, for the owner

A digest for issue #10. It decides nothing and changes no ADR. Each entry says what was built and where, the choices the ADR leaves to the owner (with
the ADR's own recommendation or the option it took), and what already depends on them. Language and semantics are the owner's.

Written on 2026-11-08 against `main` at `9f5a5b9`. Method: I read each ADR in full and checked every file, symbol, count and std statement named below
with `grep`, `ls`, `git` and a read of the CSV files. I ran no build and no test, so a claim about behaviour is the ADR's own unless the entry says
"checked".

**Update 2026-11-09 (the ADR cleanup pilot).** ADR 0076 and ADRs 0085 to 0096 were cut to one page or folded into a three-line stub, and the
text this digest names (sections, amendments, "Choices for the owner") moved. The ADR files keep their numbers. The text before the cut is
`git show 20f8f0c:docs/adr/0076-a-post-pick-dc-analysis-solves-the-network-of-the-picked-parts.md` (the sections and amendments below are in it),
and, for each of 0085 to 0096, the commit its stub names. Each entry below says where its content is now. Source files that cite "ADR 0076,
amendment N" cite that text.

**Update 2026-11-09 (ADR cleanup, batch 1).** ADRs 0023, 0025, 0046, 0063, 0070, 0073, 0075, 0077, 0079, 0080, 0081 and 0082 were
folded into a three-line stub, and each stub names the commit its full text is in (`git show ce12e04:docs/adr/<file>`). The choices that this
digest names for them are now in the doc comment or header comment of the code they are about, as "Left open, for the owner": 0063 in
`LevelTranslator` (`std/parts/logic.nsm`) and `PMosfet`; 0070 in `PullUp` and 0079 in `PullDown` (`std/bus.nsm`); 0073 in `Tvs` and 0080 in
`Zener` (`std/parts/diode.nsm`); 0081 in `Logic` (`std/logic.nsm`); 0075 and 0082 in the header of `crates/netsmith-solve/src/levels.rs`; 0077 in
`rail_aliases` (`crates/netsmith-check/src/connect.rs`); 0025 in `Resistor`, `FeedbackDivider`, `Buck` and `BuckStage`; 0046 in `designator_of`
(`crates/netsmith-design/src/lib.rs`) and the header of `levels.rs`. The line numbers this digest gives into `std/parts/diode.nsm`,
`std/parts/logic.nsm` and `std/parts/regulator.nsm` moved with the longer comments.

**Update 2026-11-10 (ADR cleanup, batch 2).** ADRs 0039, 0053 and 0057 were folded into a three-line stub, ADR 0069 became a stub that names ADR 0065 as
what replaced it, and ADRs 0022, 0026, 0065, 0078 and 0084 were cut to one page; each says the commit its full text is in (`git show 8a15457:docs/adr/<file>`).
The choices that this digest names for them are now where the code is: 0039 in the header of `crates/netsmith/src/refresh/mod.rs` (LCSC's terms of
use are unread, and whether "only this command touches the network" is a rule); 0053 in the header of `crates/netsmith-solve/src/solve/seed.rs` (what counts as
a state of a part); 0057 in the headers of `crates/netsmith-solve/src/solve.rs` and `cycles.rs`; 0065 in the ADR (its eight open questions) and the headers
of `solve/findings.rs`, `solve/transfer.rs` and `solve.rs`; the amendment of 0026 in the header of `crates/netsmith-solve/src/ohm.rs`; 0084 in the ADR (its
seven choices) and the doc comment of `bom_entries` (`crates/netsmith/src/fab.rs`); 0078 in the ADR. The line numbers this digest gives into `table.rs`,
`ohm.rs` and `solve.rs` moved with the longer headers.

**Update 2026-11-10 (ADR cleanup, batch 3).** ADRs 0020, 0031, 0033 and 0036 were folded into a three-line stub, ADR 0021 (already superseded by ADR 0097) became
a stub, and ADRs 0034, 0035, 0037, 0040, 0041 and 0043 were cut to one page; each says the commit its full text is in (`git show 0eaf326:docs/adr/<file>`). The
choices that this digest names for them are now where the code is: 0034's closing condition (a package through JLCPCB's order preview) in the ADR and the header
of `crates/netsmith-fab/src/lib.rs`, with its status now `accepted` (the ADR's own line "The owner accepted the design on 2026-11-04"; the owner may say
`proposed`); 0036's amendments 3 and 4 and the choices of amendment 2 in the header of `crates/netsmith-design/src/guidelines.rs`; 0043 in the ADR (now
`proposed`: the text does not say the owner accepted it) and the header of `crates/netsmith-pcb/src/generate/mod.rs`, including the question whether reading GPL code
to learn a published formula taints what is written; 0033's follow-up (a public reference board) in the header of `crates/netsmith-pcb/src/anneal.rs`; 0031's
choices for the owner in the headers of `route.rs` and `anneal.rs`; 0037's list of rules that are not KiCad's in the header of `crates/netsmith-fab/src/drc/mod.rs`;
0040's open items in the ADR and the headers of `crates/netsmith-check/src/mate.rs` and `crates/netsmith-fab/src/panel.rs`; 0041's seven questions in the ADR, and its
inventory and licence reading in `docs/research/kicad-library-licensing.md`. 0021's installed-library paths and their verification (issue #6) are in the header of
`crates/netsmith-kicad/src/install.rs`. The line numbers this digest gave into `place.rs`, `std/pcb.nsm`, `libraries.rs` and `guidelines.rs` follow the longer comments.

**Reversal cost** is my reading from the dependents I counted, not a statement of the ADRs: **cheap** (a solver or message rule, nothing written in
`.nsm` files depends on it), **migration** (source in `std/`, the guide or users' files must change), **hard** (users' designs change meaning, or the
answer is a legal or interface commitment).

## Decide these first

Ranked: hardest to reverse first, then by how much code depends on the choice today.

1. **ADR 0074 + 0078: the word `optional` now has three meanings** (a member, a catalog column, a pin row). Three members of std use the member marker
   (`Buck` twice, `Zener` once), four std `catalog` statements use the column marker, and 0074's three rules for what "stated" means are checker code
   (`crates/netsmith-check/src/optional.rs`). 0078 also keeps a name-based rule for three passive columns "for compatibility". Migration.
2. **ADR 0076 amendments 1, 3 and 5: the `dc` statement** (`dc resistor|diode|nmos|pmos|linear_regulator|switching_regulator|source`, and `at` after
   a level). 12 `dc` statements in std, 25 lines in `docs/LANGUAGE.md`, six test files, and per amendment 6 the real board's own parts. The owner
   chose "a `dc` statement" and "a level with its test current" (questions 1 and 2); the words, the operand order and "one `dc` per part" are the
   engineer's. Migration.
3. **ADR 0077: `~` between a module's rail and its part's rail carries the current** (amends ADR 0007). A change in what `~` means, in one case. Two
   test files and the guide depend on it. Hard.
4. **ADR 0084: `AssemblyMode::Consigned`**, its manifest word and its `bom.csv` form (empty LCSC cell, manufacturer's number as the comment). The
   variant name is public (std prelude, manifest, catalog column). The BOM form was never tried on JLCPCB's order page. Migration (name) and hard
   (format).
5. **ADR 0047: supplier data from a provider, generic passives.** The `[supply]` keys, the quote-file and cache formats are interfaces; open question
   3 (generic parts appended after the rows, or instead of them) would change which part existing designs get. Hard once designs rely on it.
6. **ADR 0075: an open-drain line is inferred from "states `vol` and no `voh`".** Cheap today, but ADR 0076 amendment 4 (choice 1) and ADR 0081
   amendment 1 are written around it. Decide before more parts rely on the inference.
7. **ADR 0043: whether reading GPL code to learn a published formula taints the generators.** A legal judgement the ADR leaves to the owner. If the
   answer is yes, the generated footprints (468 names per the ADR) need a rewrite. Hard.
8. **ADR 0086 and 0085: a field name the checker looks for (`supply_peak`) and a member of a std module (`.voltage_margin`).** Cheap now (one part and
   one module use them); each new part that follows the convention makes it dearer.
9. **ADR 0036 amendments 3 and 4: nets are drawn at the width the known current needs.** Every board's track widths depend on it, and the IPC-2221
   constants are from secondary sources. Cheap to change the rule, but boards already routed with it would move.
10. **ADR 0045 (sections 11 to 15) and ADR 0048 (readings):** the physical file's `row` statement and the height, relation and pin-protrusion rules.
    One example (`examples/enclosure_sensor`) uses them. Migration (a few files).

Everything else is cheap: a message, a threshold, a name nothing else uses.

## Contents

| ADR | Title (short) | Choices listed |
|---|---|---|
| [0034](#adr-0034) | Own fabrication files | the closing condition (issue #12) |
| [0036](#adr-0036) | Track width, amendments 1 to 4 | 6 |
| [0043](#adr-0043) | Generated footprints | 4 |
| [0045](#adr-0045) | Constraints, provisional parts | 6 |
| [0047](#adr-0047) | Supplier data, generic parts | 6 |
| [0048](#adr-0048) | The physical file, provisional readings | 5 |
| [0064](#adr-0064) | Bus roles: two provisional passages | 5 |
| [0073](#adr-0073) | TVS breakdown voltage | 2 |
| [0074](#adr-0074) | `optional` member | 4 |
| [0075](#adr-0075) | Open-drain high | 2 |
| [0076](#adr-0076) | DC analysis (sections 1 to 10) | 3 |
| [0076-A1](#adr-0076-amendment-1) | amendment 1: `dc` and `at` syntax | 5 |
| [0076-A2](#adr-0076-amendment-2) | amendment 2: resistive islands | 4 |
| [0076-A3](#adr-0076-amendment-3) | amendment 3: regulators | 4 |
| [0076-A4](#adr-0076-amendment-4) | amendment 4: logic pins, MOSFETs | 7 |
| [0076-A5](#adr-0076-amendment-5) | amendment 5: chain voltage, `dc source` | 8 |
| [0076-A6](#adr-0076-amendment-6) | amendment 6: inductor, enclosure | 5 |
| [0077](#adr-0077) | Module rail has the part's current | 3 |
| [0078](#adr-0078) | Catalog column marker | 3 |
| [0079](#adr-0079) | Pull-down | 3 |
| [0080](#adr-0080) | Zener over temperature | 4 |
| [0081](#adr-0081) | Logic swing (and amendment 1) | 4 |
| [0082](#adr-0082) | Levels as fractions | 2 |
| [0083](#adr-0083) | `unused` | 4 |
| [0084](#adr-0084) | Consigned part | 4 (7 in the ADR) |
| [0085](#adr-0085) | Buck stage voltage margin | 4 |
| [0086](#adr-0086) | Chain carries the peak | 4 (9 in the ADR) |
| [0087](#adr-0087) | netsmith's own footprints | 4 (7 in the ADR) |

**Not covered.** Only ADRs whose status or body says *provisional* are here. These say "proposed for owner review" or "implemented ... for review" and
are a separate batch: 0051 to 0058, 0060 to 0063, 0065 to 0068, 0070, 0071, 0072 (and the review notes inside 0049, 0050, 0059, 0061). Ask if you want
them digested the same way. ADR 0069 is superseded by 0065. Old reviews of 0021 to 0045 are in `docs/research/provisional-adr-reviews.md`; the owner
decided those on 2026-11-04 (issue #10 comments).

**Where an ADR no longer matches the code** the entry says so under *Stale*. I did not edit the ADRs.

## ADR 0034
Own fabrication files. Status: accepted in design, provisional until a real package has been through JLCPCB's order preview.

**Built.** `crates/netsmith-fab` (`artwork.rs`, `plot.rs`, `gbr.rs`, `drill.rs`, `cpl.rs`, `fill.rs`, `raster/`, `crosscheck.rs`; all exist).

**Open.** The closing condition, not a design choice: one real package through JLCPCB's order preview and design check, the planes (clear-polarity
pours in a copper file) and the silkscreen font looked at, the result written into the ADR. It still ends "Result: none yet." Issue #12 is open. Only
the owner can do it (it needs an order account). Nothing to reverse; until then the ADR claims no acceptance by JLCPCB. Every `fab` run uses these
writers; nothing waits on a choice.

## ADR 0036
Track width against current, amendments 1 to 4 (all marked PROVISIONAL; 3 and 4 "await the owner's review").

**Built.** `plan_tracks` in `crates/netsmith-design/src/guidelines.rs`, with `K_EXTERNAL = 0.048`, `K_INTERNAL = 0.024`, the exponents 0.44
and 0.725, `Features::net_widths`. Amendment 4 uses `netsmith_solve::CurrentBound` (the Ohm bound, `crates/netsmith-solve/src/ohm.rs`). Tests:
`crates/netsmith-design/tests/guidelines.rs`.

**Choices.**
1. **Amendment 1: the IPC-2221 constants.** The standard is paywalled and was not read; formula and constants come from calculators and a crate.
   Assumed: 1 oz copper, 10 K rise, outer layers, no duty cycle. Recommendation: keep, confirm against the standard when someone has it. Cheap. Issue
   #45 lists it.
2. **Amendment 1: the router sizes every net; an explicit `.min_track` stays an override.** Taken. Cheap.
3. **Amendment 2: the net's current is the larger of the two sums** (loads out, supplies in), all loads on at once, with a module's port left out when
   something inside is on the net, and no quiescent current added. The pessimism at a regulator's ground is left in. Taken. Cheap.
4. **Amendment 3: a net with an unknown current is drawn for the known sum** (a lower bound), with a note, not a guideline. Alternative rejected:
   leave it at the process least. Open: should a net with `.min_track` also get the note; should the note be a guideline that `--strict` can fail.
   Cheap, but boards with a missing current draw wider nets than before.
5. **Amendment 4: the note is dropped when the width carries the known sum plus every Ohm bound.** Taken. Cheap.
6. **Whether the amendments stay provisional.** 3 and 4 await review.

**Depends.** Every board's net widths, the router, the board file's net classes and `fab-notes.txt` read `plan_tracks`. No example changed width
(their currents are milliamps, per the ADR).

## ADR 0043
Generated footprints. Status: `provisional` (no text on what is open).

**Built.** `crates/netsmith-pcb/src/generate/`: `chip.rs`, `sot.rs`, `gullwing.rs`, `qfn.rs`, `qfn_rows.rs`, `qfp.rs`, `tab.rs`, `bga.rs`,
`crystal.rs`, `diode.rs`, `header.rs`, `ipc.rs`, `outline.rs`, `draw.rs`. `FootprintLibrary` consults the generators before the installed KiCad folder
(`crates/netsmith-pcb/src/footprint.rs`, doc at line 794). The validation test is `crates/netsmith-pcb/tests/generated_vs_kicad.rs`; it needs KiCad
installed, so I could not re-run it and did not re-count the ADR's "468 names" and "712 of 792 agree".

**Choices (the ADR's own "Provisional" line plus two it records).**
1. **The GPL question.** The QFN and no-lead formulas were learned by reading `kicad-footprint-generator` (GPL-3.0-or-later); the Rust shares no code.
   "Whether reading GPL code to learn a published formula taints what is written is the owner's to judge." Recommendation: none given. Hard (rewrite).
2. **Land pattern density.** IPC-7351B nominal (Level B), F = 0.1 mm, P = 0.05 mm. Taken. Migration: pads of every generated footprint would move.
3. **The generator wins over an installed KiCad footprint of the same name.** Taken, so a build gives the same pads on every machine. Alternative:
   KiCad's file wins. Cheap.
4. **Growth rule: a new row is added only if it agrees with KiCad within 0.01 mm.** Rows that do not agree are left to the library (nine QFNs, nine
   QFPs with an exposed pad and others). Whether a depopulated BGA grid can be a rule is open. Cheap.

**Depends.** `std/kicad.toml` rows name generated footprints; every example's DRC runs with them. ADR 0087 adds a second source
(netsmith-owned files) beside the generators.

## ADR 0045
Constraints from the enclosure. Provisional parts only (the owner decided section 10 on 2026-11-03 and 2026-11-04).

**Built.** In `crates/netsmith-pcb/src`: `overhang.rs`, `constraints.rs`, `explain.rs` (`before_placing`, line 173), `protrusion.rs`, `anneal.rs`
(`HEIGHT_ZONE_MARGIN` = `KEEPOUT_GAP` = 0.25 mm), `place.rs` (`turn_to_fit`, line 1266). `Keepout.max_height` is `std/pcb.nsm:92`; `pin_protrusion` is
added in `crates/netsmith-check/src/checker.rs:810`; `constraints_strict` is `crates/netsmith/src/fab.rs:71`; `precheck_findings` is in
`crates/netsmith/src/editor/findings.rs`.

**Choices.**
1. **Section 10, 3a and B6:** the "body centre outside the outline" refusal is skipped for a part that states the overhang on that edge; a part pinned
   inside a keepout for parts is refused for every seed (a first version let the pinning requirement win and was taken back: "which of the two is
   meant is the owner's to say", issue #99). Cheap.
2. **`turn_to_fit`:** the placer turns a part that does not fit by 90 degrees before placing, also a part that states a face or a window. It moved one
   existing test expectation. Cheap.
3. **Section 11, height limits.** Open: whether an unknown height is an error with the key unset (now only with `--constraints error` or `constraints
   = "error"`); whether the 0.25 mm margin comes from the process; whether a catalog `height` column comes first (issues #27, #28); reading a limit
   from a DXF. Cheap.
4. **Section 12, relations between two parts:** recognised only as one axis of two parts with coefficients 1 and -1; a module's box, a turn and
   three-way relations are not offered. Widening is cheap, narrowing is a migration. A layout file stores places to 0.01 mm, so a window narrower than
   that may not survive a save.
5. **Sections 13 and 14, the proofs and their wording:** only proved conflicts are errors; locks are left out of the proofs. Cheap.
6. **Section 15, pin protrusion** (design decided): the readings are provisional: one value (`&=`), `hand` or `none` counts as fitted by hand, a
   `trim:` note for every stated protrusion, a hint for a part over unstated pins. Cheap.

**Depends.** `examples/enclosure_sensor/enclosure_sensor.physical.nsm` uses `.height`, `.max_height` and `row`; no example states `.pin_protrusion`.
Tests: `crates/netsmith-pcb/tests/`, `crates/netsmith/tests/overhang.rs`.

**Stale.** Section 11's last "Open for the maintainer" list names two choices the owner confirmed on 2026-11-04 (a height-only keepout forbids nothing
else; `.layers` names the faces).

## ADR 0047
Supplier data comes from a provider; passives are generic. Status: provisional (open questions 1, 2, 5, 7
decided by the owner).

**Built.** `crates/netsmith/src/supply/` (`mod.rs`, `cache.rs`, `file.rs`), the `[supply]` table and the `series` key in
`crates/netsmith/src/project.rs` (lines 80 to 135: only the `file` provider is accepted), `crates/netsmith-solve/src/generic.rs` (E12, E24, E96),
`crates/netsmith/src/resolve.rs` (`MAX_RECHECKS = 30`), `BomLine.supply|unknown|unresolved` in `crates/netsmith-editor/src/api.rs`. Checked: none of
the 349 rows in `data/jlcpcb/catalog/*.csv` has a price, stock or date; `--offline` exists for `catalog refresh` only.

**Choices still open.**
1. **Q3: generic parts appended after the rows (built) or instead of them.** Instead is the clean model and changes which part existing designs get.
   Recommendation: keep appended until the owner says. Hard once designs rely on picks.
2. **Q4: the preferred-parts list** (Basic, Preferred, Extended, then file order) or a file of its own. Cheap.
3. **Q6: which real providers**, and whose key. The ADR reads no supplier's terms. Hard (legal).
4. **Q8: the editor's swap action** (not built); `fab` refuses an unresolved part (built). Cheap.
5. **Q9: keep the `price`, `stock` and `date` columns** as the last-resort `catalog` provider or remove them. Migration (users' CSV files).
6. **Provisional assumptions 1 to 5** of the generic-capacitor/inductor/LED section: E96 means E24 for a capacitor or inductor; the packages and
   tolerance classes; `Stated` for an LED's forward voltage; an unbounded rating stays unknown; the cap of 30 re-solves. Also not decided: the
   spelling of the manifest field that names a credential's environment variable (`grep key_env` finds nothing, so it is unbuilt) and the user-level
   credentials file. Cheap.

**Depends.** `[supply]` and `series` are manifest keys (migration if renamed); ADR 0022 and 0039 point at this ADR; the editor API has three BOM
fields for it. Issues #8, #9, #30 to #33 track the rest.

## ADR 0048
The physical file. Status accepted for decisions 1 to 7 and 10; the provisional readings of the board-level build.

**Built.** `physical for Board { ... }` units and `case { }` (parser in `crates/netsmith-lang/src/parser.rs`, the `row` statement at line 486);
`crates/netsmith/src/physical.rs` (editing at the syntax tree's spans); the checker pairs a unit with its board. Three physical files in `examples/`
and one in `scripts/editor-verify-js/fixtures/`.

**Choices (the ADR's "Provisional readings" and "Not decided here").**
1. **Which things are refused outside `case { }`** (outline, cutouts, holes, fiducials, labels) while a `Keepout` may be anywhere in the unit. Cheap.
2. **`.height` may be narrowed in a board's circuit source and in its physical file**: two spellings until the owner says which stays (design rule:
   one correct way). Migration.
3. **The `row` statement** (`row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`): the words `along` and `pitch`, neighbour-to-neighbour
   ties, zero-padded numbers, negative pitch. Decision 10 accepted a row statement; every spelling detail is provisional. Used by
   `examples/enclosure_sensor/enclosure_sensor.physical.nsm:96`. Migration.
4. **Natural order** (decision 11): only a trailing number counts, `tp02` and `tp2` ordered by digits. Cheap.
5. **Not decided** (language design, the owner's): how a module's physical file attaches; a physical file for a `system` or `panel`; the shared
   `enclosure` definition (issue #46); the exact list of what leaves under decision 10.

## ADR 0064
Bus roles and levels. Status: implemented; two passages are provisional.

**Built.** `lines` and roles in `crates/netsmith-check/src/lines.rs`, `busnets.rs`; levels in `crates/netsmith-solve/src/levels.rs`; `std/bus.nsm`,
`std/logic.nsm`.

**Choices.**
1. **Amendment 2026-11-07: `Levels` states `.reference.current &= 0A`** (`std/logic.nsm:60`), so a bus's reference rail is not a load. Taken. Open:
   `Logic` has the same shape and does not state it (the translator states it by hand); moving it is ADR 0066 open question 3. Cheap.
2. **Section 10: `UsbRole::Passthrough`** (`std/bus.nsm:175`), stated by `UsbC2` (`std/parts/connector.nsm:395`): a receptacle is neither host nor
   device. Name options: `Receptacle`, `Through`, `Transparent`. Cheap to change while only `UsbC2` uses it; migration for boards that state it.
3. **A role is transparent when the bus declares no `lines` for it** (`BusDef::passes_through`), with no new syntax. Cost the ADR names: a bus author
   who forgets `lines` makes a role transparent silently. Alternative: an explicit mark. Recommendation: as built. Cheap.
4. **`Uart` has no roles.** Taken. Cheap.
5. **The `bus role` guideline stays a guideline** (an error only under `--strict` or `guidelines = "error"`); making it an error by default is "not
   planned". Cheap.

## ADR 0073
A TVS diode has a breakdown voltage as an optional catalog column.

**Built.** `std/parts/diode.nsm:189` (`.breakdown_voltage: Voltage`) and line 200 (`catalog tvs { ..., .breakdown_voltage optional }`);
`data/jlcpcb/catalog/tvs.csv` has the column.

**Choices.**
1. **Optional, not required** (as the project part `TvsDiodeBr` had it). Taken: a required column would need a sourced number in every row. Cheap.
2. **No relation between `.standoff_voltage` and `.breakdown_voltage` in `Tvs`.** Taken: the solver counts a field as asked for once a relation
   mentions it, so the relation would make every row unfit. Cheap.

**Depends.** ADR 0078 replaced its Rust list (`OPTIONAL_OF_KINDS`, checked: gone) with the marker.

**Stale.** The ADR says every cell of the standard `tvs.csv` is empty. One row states it now (SMDJ36A, 40 V, with a datasheet source, added by #233);
`data/jlcpcb/README.md` says so.

## ADR 0074
A member marked `optional` is skipped by the checks that read it when nothing states it.

**Built.** `Decl.optional` (`crates/netsmith-lang/src/ast.rs:458`), `crates/netsmith-check/src/optional.rs`, `Field.optional` in `model.rs`. Users in
std: `Buck.min_inductor_saturation`, `Buck.max_inductor_dcr` (`std/parts/regulator.nsm:134,138`) and `Zener.temperature_shift`
(`std/parts/diode.nsm:135`). Tests: `crates/netsmith-check/tests/optional.rs`.

**Choices (the owner chose the word, its place and "after the type").**
1. **What "stated" means.** A bound against numbers alone states the member; a bound against another member does not; a catalog row or a pick never
   does; "stated" travels across `~`. Rejected: counting only `&=`, `?=` and `==`. Recommendation: as built. Migration (semantics of every optional
   member).
2. **A bound such as `.max_dcr <= 0.5ohm` states the member but gives it no value**, so the relation is listed as not evaluated, not skipped. Cheap.
3. **What is refused:** `optional` on a pin, on an interface, part or module member, in a board, system, panel or physical file, and a statement of
   the member in the definition that declares it. Cheap.
4. **Which other members of `Buck` and std should be marked** (`min_inductance` and the rest): the owner's, member by member, because a skip hides a
   check. Cheap.

**Depends.** ADR 0080 (Zener) and ADR 0083 refer to its rules. Tests and guide section.

**Stale.** "Only the two members of `Buck` use it": three do now.

## ADR 0075
An open-drain line takes its high from its pull-up.

**Built.** `pair` in `crates/netsmith-solve/src/levels.rs` (around lines 301 to 408); no language change.

**Choices.**
1. **Keep the inference** "a driver that states `vol` and no `voh`, on a net with a pull-up, is open-drain", or add a way to declare it (a pin kind or
   a marker on `lines`). The ADR names its risk: a push-pull part with an incomplete model also gets its high from the pull-up, and the note that
   showed the model was incomplete is gone. If a pin kind is chosen, only this rule's driver test changes. Cheap today.
2. **A pull-up behind a chain, a series resistor or a switch is not followed** (it keeps the note), while ADR 0044's I2C rule follows a chain. Cheap.

**Depends.** ADR 0076 amendment 4 choice 1 treats such a pin as a boundary and points here; ADR 0081 amendment 1 says the swing's top stands in only
when there is no pull-up. In std, only `Sn74lvc1g04` (`std/parts/logic.nsm:45`) states levels with a test current; it states both `voh` and `vol`.

## ADR 0076
**Moved.** The ADR is one page: the decision, the owner's decisions of 2026-11-07, what was rejected and a "Left open" list. Its items 1 to 3 are the
three "left to the owner at the top level" below, word for word. Behaviour is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"), and the
mechanism notes (determinism, the island limit, the interior check, what is not built) are the header of `crates/netsmith-solve/src/dc/mod.rs` and
`src/solve/dcpass.rs`. The amendments are no longer in the ADR; their "Choices for the owner" below are still open. The prototype crate `netsmith-dc` named below was removed by #305.
A post-pick DC analysis (sections 1 to 10). The owner decided questions 1 to 7 on 2026-11-07
(question 1: `dc` statement; 2: the datasheet's form, a level with its test current; 3: no feature
for which states coexist; 4: narrow where unstated, check where stated; 5: a stated `.current` on a
boundary; 6: severities as recommended; 7: capacitors open). The amendments below stay provisional.

**Built.** `crates/netsmith-solve/src/dc/` (`extract.rs`, `net.rs`, `mod.rs`, `tests.rs`) and `crates/netsmith-solve/src/solve/dcpass.rs` (`dc_pass`
line 122, `dc_start` line 290; called from `solve.rs:2563` and `:2602`); the checker's records in `crates/netsmith-check/src/dc.rs` and `model.rs`
(`DcKind`, `Instance::dc`, `dc_elements`, `level_tests`). The prototype `crates/netsmith-dc` was removed on 2026-11-09 (it is in git history at
`b504854` and before). The glossary has _Operating point_, _Island_ and _Corner_; the guide has a "DC
analysis" section.

**Left to the owner at the top level.**
1. **A sound enclosure for non-monotone quantities.** The corner hull is the circuit's range only where the quantity is monotone; a power can peak
   inside the box. The ADR proposes a stated margin of about `(tolerance/2)^2` for any element with a range wider than 5 %, and says a sound enclosure
   (interval Newton, affine arithmetic) is possible and not built. Recommendation: no change until the interior check fails on a real design. Cheap.
2. **The island limit** (65536 combinations, work 2^28; checked in `dc/net.rs:74,80`): over it a note names the island and nothing is written. Cheap.
3. **What was not established** (the section lists nine): the cost on a whole board (the real board is not a file of this repository), the fixed
   point's convergence for a regulator with hysteresis, and the fuse's missing maximum resistance. Not choices; cheap.

## ADR 0076 amendment 1
**Moved.** Amendment 1 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
The syntax of `dc` and of a level with its test current.

**Built.** `dc resistor|diode|nmos|pmos` and `at` in `crates/netsmith-lang` (`tests/dc.rs`) and `crates/netsmith-check/src/dc.rs`. Checked: 12 `dc`
statements in std (Resistor, Inductor, FerriteBead, Fuse, Diode, Schottky, NMosfet, PMosfet, LinearRegulator, Lt8641, Obd2M, UsbCPower); `at` is used
on two lines of `std/parts/logic.nsm`; `docs/LANGUAGE.md` has 25 `dc` lines.

**Choices (all inside today's grammar; the owner "confirms or changes it").**
1. **The words and the operand order:** pins first, values last (`dc resistor .a .b .resistance`). Rejected: a word per operand, a block, the element
   inside `bridge`, one `mosfet` word with a polarity. Migration.
2. **`dc` is a keyword at the start of a statement only when a word follows**, as `bridge` is; `at` is read after the right side only. Cheap.
3. **One `dc` per part; a part or a generic may state it, a module, board or physical file may not.** A provider of a generic takes the generic's
   `dc`. Migration.
4. **`at` goes on `voh` or `vol` only, with one positive number, once per level.** Cheap.
5. **Left open by the ADR:** the leakage `Ii` at a test voltage (no member on `Logic` or `Levels`); a part with two elements (body diode, dual diode);
   whether a part with a `bridge` and no `dc` is an error or a note (a note today). Cheap.

**Depends.** The 12 std statements above, six test files (`netsmith-check/tests/dc.rs`, `netsmith-lang/tests/dc.rs`,
`netsmith-solve/tests/dc_chain.rs`, `dc_regulators.rs`, `dc_buck.rs`, `dc_switches.rs`) and the guide.

**Stale.** The ADR and `docs/LANGUAGE.md:7997` say `Lt8641Buck`; the std part is `Lt8641` (`std/parts/regulator.nsm:356`).

## ADR 0076 amendment 2
**Moved.** Amendment 2 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
The DC analysis of the resistive islands (phase 2, stage 2a).

**Built.** `dc/extract.rs`, `dc/net.rs`, `solve/dcpass.rs` (writes `.current` and `.drop`, checks stated fields). Amendment 2's text says "each is a
line to change" without numbering them; these are the ones that change behaviour:

**Choices.**
1. **What is a source:** only a `Rail` that the board itself declares and whose `.voltage` a statement narrowed. A module's rail (a port that may
   carry a seed) and a solved rail are not sources. This is why issue #153's seed stays. Cheap.
2. **A stated field is a budget:** the pass errors when the circuit needs more than the stated `.current`. Designs that built with a note can now fail
   (a resistor across a rail, a first-row pick over its rating). Question 4 decided it; the effect on existing designs is the part to review. Hard for
   users whose designs now fail.
3. **Ranges widened outward by a billionth, a picoampere or a nanovolt** so that zero-in-the-circuit quantities do not miss. Issue #253 is a false
   error caused by this widening. Cheap.
4. **A conductance of a femtosiemens added only where the matrix is singular** (a node behind a blocking diode). Cheap.

## ADR 0076 amendment 3
**Moved.** Amendment 3 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
Regulators in the DC analysis (stage 2b, slice 1).

**Built.** `dc/extract.rs`, `dc/net.rs`, `dcpass.rs` (`dc_collapse` line 433); `dc linear_regulator <in rail> <out rail>` and `dc switching_regulator
<in rail> <out rail> <eff>`; std: `LinearRegulator` (`regulator.nsm:28`), `Lt8641` (`:392`).

**Choices.**
1. **The operands are rails, not pins** (the first two operands). Cheap while only two std parts state it; migration for boards' own regulator parts
   (amendment 5 says the real board's two ICs do).
2. **The quiescent current is left out** (no std regulator states one). An optional operand later changes nothing else. Cheap.
3. **A collapse** (input under 1 mV, no operating point) is an error, `the source cannot give what ... asks`, and an input below the regulator's
   minimum is the part's own relation failing. Question 6 decided the severity; the 1 mV threshold and the wording are the engineer's. Cheap.
4. **The hand-written equalities and the seed stay** in this slice. See amendments 5 and 6.

## ADR 0076 amendment 4
**Moved.** Amendment 4 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
Logic pins that drive, and MOSFETs as switches (stage 2b, slice 2). Seven choices, all numbered in the ADR.

**Built.** `dc/extract.rs` (line 1130 on, the driver test; `unused_by` at 1148), `dc/net.rs`, `dcpass.rs`.

**Choices.**
1. **Open-drain and a pin that states one level:** taken as a boundary. Alternative: model it as low or high impedance with ADR 0075's inference.
   Cheap. Ties to 0075 choice 1.
2. **Leakage:** a high-impedance pin is an open circuit (no leakage), because no member exists. Alternative: members `ii`, or `at` on a third level;
   both change `Logic` and `Levels`. Migration if added.
3. **Several drivers on a net:** no pin of the net is modelled. Alternatives: try all combinations; or a "one driver at a time" statement (the feature
   question 3 refused). Cheap.
4. **A line with `.swing`:** a boundary. Alternative: read the high from the swing's top as a node. Cheap.
5. **`roh` from the datasheet's drop at its test current.** Alternative: a table of levels over several currents (`at` refuses a second load today).
   Cheap.
6. **A floating gate keeps both states.** Alternative: ground it. Cheap.
7. **`Sn74lvc1g04` states `at 100uA`** on `voh` and `vol` (checked, `std/parts/logic.nsm:45-46`). Cheap.

## ADR 0076 amendment 5
**Moved.** Amendment 5 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
A chain carries the voltage, a part can give a supply, a circle a pick waits on starts without a window
(phase 3, slice 1, issue #153). Eight choices.

**Built.** `dc/extract.rs`, `dcpass.rs` (`voltages_picks_read` line 252, `dc_start`), `netsmith-check/src/dc.rs` (`DcKind::Source`).

**Choices.**
1. **`dc source <rail>` is a new word.** Alternatives: infer a source from a stated `.voltage` on a connector rail (a guess); a marker on the board; a
   `source: Rail` kind. Std: `Obd2M` (`connector.nsm:421`) and `UsbCPower` (`:369`). Migration.
2. **Which std parts state it:** the two connectors. Cheap.
3. **A voltage a statement gave a rail is not checked against the circuit** (a buck's rail past the inductor sits a few millivolts below the window).
   Alternative: check it and let the regulator name its held node. Same question as amendment 6 choice 1. Cheap.
4. **The carried voltage, not the peak.** `.peak` stays stated. Alternative: `.vout.peak <= .vin.peak` along a chain. Cheap.
5. **The start before the picks runs only when a pick waits on a voltage** (one test changed). Alternatives: always; or pick the series parts first.
   Cheap.
6. **The start uses the hull of the candidates that fit** (`fits`), not of those that survive the relations. Cheap.
7. **A regulator's other input pins on its input nets are its input's,** with no note. Cheap.
8. **The hand-written chain equalities stay valid** and are the board's to delete; std has none. Cheap.

## ADR 0076 amendment 6
**Moved.** Amendment 6 is only in the text before the cut (`git show 20f8f0c:...`); the behaviour it built is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis"). The choices below are still open.
The winding of an inductor, an enclosure of the load behind a regulator, what the real board still needs
(phase 3, slice 2). Five choices.

**Built.** `std/parts/passive.nsm` (`Inductor` states `dc resistor .a .b .dc_resistance`, line 175), `std/parts/regulator.nsm` (`Buck` states
`-.sw.current <= .max_output_current`, line 124; `LinearRegulator` `-.vout.current <= .max_current`, line 31), `dc/mod.rs`, `dcpass.rs`.

**Choices.**
1. **Which node a buck holds:** the IC's switch pin, so a window the board states on the rail past the inductor is not checked. Alternatives (a) hold
   the stage's output (needs `dc` to name a rail outside the part); (b) check the window against the node past the winding (fails a right design by 9
   mV); (c) write the narrowed range (hides a stage that cannot hold its window). Same question as amendment 5 choice 3. Cheap, but no seed on the
   real board can go until it is decided.
2. **`Buck` states `-.sw.current <= .max_output_current`.** It adds one check to every buck IC (the LT8641 states 3.5 A, `regulator.nsm:382`).
   Alternatives: an optional operand of `dc switching_regulator`; each IC states it. Migration for a project's own ICs.
3. **`dc` stays in the part, not in the `Buck` interface,** so each buck IC repeats one line. Alternative: let an interface state the element (changes
   what an interface is). Cheap.
4. **The equalities of the sheets stay,** because they let a pick refuse a candidate on its drop; deleting them moves one module test's expected
   message from pick time to the later pass. Alternative: change the tests' messages. Cheap.
5. **A generic row takes `0..bound` from a relation that bounds it, no enclosure otherwise.** Alternative: the catalog's largest real value. Cheap.

**Open.** The order of the picks (series parts first, the pass, then parts that read it) is what would let the two seeds go. It is "not built"; issues
#153 and #196 stay open.

## ADR 0077
A module rail joined to the rail of its part has the part's current. Amends ADR 0007 for one case.

**Built.** `crates/netsmith-check/src/connect.rs` (around lines 306, 997, 1057) and `model.rs:885`; the balance makes the module's rail the part's
current. Tests: `crates/netsmith-check/tests/rail_alias.rs`, `crates/netsmith-solve/tests/rail_alias.rs`.

**Choices.**
1. **The rule is narrow:** a `~` inside a module between its own rail and a rail of a part directly inside, when the two are each joined to one other
   rail and nothing else draws on the net. The owner decided "narrowly"; the exact conditions are the agent's. Cheap to narrow, migration to widen.
2. **Rejected: an equation between the two `current` fields,** because a budget at the module's rail would then narrow the part's claim and the
   refusal of ADRs 0066 and 0038 would be lost. Taken: the alias is made in the balance. Cheap.
3. **Left open for the owner:** the same for a module that wraps another module (`.vdd ~ .inner.vdd`); a wrapper's bus interface
   (`.spi.levels.reference ~ .vcc`, issue 188). Cheap.

**Depends.** The guide's Load current section and the guide's example. The ADR says redundant hand-written equations in std and tests "agree and
pass"; I did not list them.

## ADR 0078
A catalog column is marked `optional` in the `catalog` statement.

**Built.** The parser reads the word after a column; `Catalog.optional` in `crates/netsmith-check/src/model.rs` (line 307); `is_optional` in
`crates/netsmith-solve/src/table.rs:155`. std marks four columns (`tvs.breakdown_voltage`, `resistor.temperature_coefficient`, `capacitor.dielectric`,
`inductor.rms_current`; `diode.nsm:200`, `passive.nsm:46,110,178`). Tests: `crates/netsmith-check/tests/catalog_optional.rs`.

**Choices.**
1. **The place of the word:** after the column, as in a `pins` row. Rejected: a statement of its own, a leading word, a flag on the catalog.
   Migration.
2. **`is_optional` still answers yes by the standard catalog's name for the three passive columns,** whether or not a part marks them (compatibility
   for a project's part written before the marker). Recommendation in the ADR: remove it later, with a deprecation. Migration.
3. **The key of a keyed catalog cannot be optional.** Cheap.

## ADR 0079
A pull-down states its own current.

**Built.** `module PullDown` in `std/bus.nsm:318`. Tests: `crates/netsmith/tests/pulldown.rs`. Checked: no std module or example uses it.

**Choices.**
1. **The names** `.line_high`, `.line_top`, `.reference`, and that it draws nothing from a rail (an `.reference.current &= 0A`). Taken. Cheap (nothing
   in std uses it).
2. **Not built: a `.input_current` on `Logic`** for the pins' leakage (issue 192). It cannot join a balance that does not exist, and ADR 0076 decision
   2 asks for the datasheet's form `Ii` at a test voltage, to be shown to the owner first. Migration if added (changes `Logic`).
3. **No negative current for a line driven below ground.** Cheap.

## ADR 0080
A Zener states its figures over temperature in optional columns.

**Built.** `std/parts/diode.nsm` (`.test_current`, `.temperature_coefficient`, `.leakage_current`, `.leakage_test_voltage`, `.temperature_shift:
Temperature optional`, `.voltage_over_temperature`); `data/jlcpcb/catalog/zener.csv`. Tests: `crates/netsmith/tests/std_zener.rs`.

**Choices.**
1. **The coefficient is a `Voltage` meaning volts per kelvin** (`18.4mV..22mV`). Alternative: a new quantity kind (a language choice). Migration if
   changed (every row).
2. **The unit of the coefficient is the datasheet's typical figure,** so the corners are typical-based. Cheap.
3. **Which marker guards what:** the shift is a member marker, the coefficient is a column marker, so the row's figure is not ignored. Cheap.
4. **The relation's form** `.voltage_over_temperature == .zener_voltage + .temperature_coefficient * (.temperature_shift / 1K)`. Cheap.

**Depends.** All eight rows of `zener.csv` state all four columns (checked). Issue #199.

**Stale.** The ADR says "each of the seven existing rows"; there are eight now (BZT52C39, #233).

## ADR 0081
A `Logic` line states its own swing; amendment 1 (2026-11-08, provisional): a swing gives the unloaded levels.

**Built.** `.swing: Voltage` on `Logic` and `Levels` (`std/logic.nsm`); `net_range` and the derived levels in `crates/netsmith-solve/src/levels.rs`
(lines 145 to 174, 625 to 657); `logic_swing` in `crates/netsmith-solve/src/ohm.rs:255`. Tests: `crates/netsmith-solve/tests/swing_levels.rs`. No std
part states a swing yet (checked).

**Choices.**
1. **What it is relative to:** the line's ground, the `neg` of `.reference`; a signed range is read as written. Alternative: relative to `pos`. Cheap.
2. **The way a part with no pad for its internal supply gets a ground:** it joins `.reference` to a supply or ground it has. Cheap.
3. **Amendment 1: a line with a swing and no `voh` gets `swing.hi` as `voh`, no `vol` gets `swing.lo`.** A stated level always wins; ADR 0075's
   pull-up rule comes first; the derived level does not start a check of a pair. Rejected: `swing.lo` as `voh`. Cheap.
4. **Whether a derived level counts as stated:** no, so a swing alone does not start the check of every pair on the net. Cheap.

**Depends.** ADR 0076 amendment 4 treats a line with a swing as a boundary.

## ADR 0082
Levels stated as fractions of one rail are compared at one supply voltage.

**Built.** `Run::fractions` in `crates/netsmith-solve/src/levels.rs` (line 478). Test: `crates/netsmith-solve/tests/correlated_levels.rs` (and
`bus_levels.rs` uses the `0.7 * .vdd.voltage` form).

**Choices.**
1. **Extend to affine forms** (`.vdd.voltage - 0.4V`), decided exactly by comparing at the rail's two ends. Not built; the rule is the smallest that
   passes the issue's case. Cheap.
2. **Whether ADR 0075's pull-up path** (an open-drain high that is the pull-up's rail) wants the same treatment for a receiver at a fraction of that
   rail. Cheap.

**Limits the ADR states:** division and a range factor are not read as fractions.

## ADR 0083
A board states that a function is unused.

**Built.** `unused .processor.usb`; `crates/netsmith-check/src/unused.rs`, `Model::unused`/`unused_by` (`model.rs:978`); the level checks skip its
pins. Tests: `crates/netsmith-check/tests/unused.rs`, `crates/netsmith/tests/unused.rs`. Checked: the DC extraction reads `unused_by` for logic-pin
drivers (`dc/extract.rs:1148`). No example uses it.

**Choices.**
1. **The spelling `unused .processor.usb`** (a keyword statement, as `tie` and `bridge`). The owner's example was `.processor.usb: unused`, rejected
   because a declaration reads `.name: Type`. The owner "may still choose the other spelling". Migration.
2. **What counts as a use:** a pin on a net with a non-passive pin of another part, a `~` to another interface, a chosen function. A test pad or a
   header is not a use; nor is a ground. Alternative: any connection is a use (useless for a module's pads brought out to test pads). Cheap.
3. **Left open:** whether a part may state a function unused by default; whether a module or part may forward `unused` to an interface inside it.
   Migration if added.
4. **The DC analysis:** the ADR says it was not changed and still reads the rails of an unused interface. Logic-pin drivers do check it now; I did not
   check whether rails of an unused interface are still read.

## ADR 0084
A part can be consigned: the owner ships it to the board house.

**Built.** `AssemblyMode::Consigned` (`std/prelude.nsm:21`), `Assembly::Consigned` (`crates/netsmith-lower/src/lib.rs`), `Fit::Consigned` and
`Plan::consigned` (`crates/netsmith-fab/src/board.rs`, `lib.rs:220`), the manifest and catalog-column word (`crates/netsmith/src/project.rs:64,240`,
`crates/netsmith-solve/src/table.rs:492`); the stock note and the resolver skip it. Tests: `crates/netsmith/tests/consigned.rs`. Glossary and
`docs/LANGUAGE.md:1667` have it.

**Choices (7 in the ADR).**
1. **The name `Consigned`.** Rejected: `Supplied`, `OwnerSupplied`, `Customer`, `Fab` plus a flag. Migration.
2. **One variant for every consignment**, with or without an `lcsc`. Cheap.
3. **The mark in `bom.csv`:** (a) empty LCSC cell and the manufacturer's number as comment (built), (b) a fifth column, (c) a word in the LCSC cell,
   (d) leave the part out. JLCPCB documents no marker; (a) changes in one function (`bom_entries`) if one appears. Never tried on JLCPCB's order page.
   Hard (format).
4. **Smaller choices, each cheap:** the comment is the manufacturer's number; `assembly = "consigned"` is allowed as the project default (alternative:
   refuse it); a consigned generic stays generic (no LCSC number chosen); the `consigned:` line in `fab-notes.txt` has no spares count.

**Open.** The editor's BOM shows a consigned part as an ordinary line (its API has `hand_solder` and no consigned field; adding one changes the
generated types and the bundle).

## ADR 0085
**Folded.** The ADR is a stub; the reasoning and the alternatives are in the doc comment of `.voltage_margin` in `std/parts/regulator.nsm`, the behaviour in the guide's "Switching regulators". The choices below are still open.
A buck stage states a voltage margin for its capacitors.

**Built.** `.voltage_margin: Number` with `?= 1` and four relations in `BuckStage` (`std/parts/regulator.nsm:214-247`). References: 14 in
`crates/netsmith/tests/std_models.rs`, 4 in the guide; the real board states `1.25` (per the ADR; I cannot check it).

**Choices.**
1. **The name** `.voltage_margin` (alternatives `.derating`, `.rating_margin`). Migration for boards that state it.
2. **The margin applies to the steady voltage as well as to the peak** (here yes; alternative: peak only, as the project's copy has it). Cheap.
3. **Expose the capacitors of the banks instead** (rejected: each board repeats the rule). Migration.
4. **Whether other std modules that hold capacitors across a rail take the same member** when added; none exists. Cheap.

**Stale.** None found.

## ADR 0086
**Cut.** The ADR is one page; the alternatives it did not take are in its "Rejected" paragraph and its "Left open" paragraph. ADR 0088 and 0091 are folded and use the same field-name convention.
A chain carries the peak of its rail to the parts in series (extends ADR 0013 and 0014).

**Built.** `crates/netsmith-check/src/connect.rs` (five references) and `model.rs`: a part with a `bridge` and a field named `.supply_peak`, in a `~>`
chain, gets the `.peak` of the rail on the chain's start net. `Fuse` is the only std definition that has it (`std/parts/passive.nsm:257,261`). Tests:
`crates/netsmith-check/tests/supply_peak.rs`, `crates/netsmith/tests/std_kinds.rs`.

**Choices.** The ADR takes "the smallest reading" and lists what it did not take (each a language or std change). Cheap each unless said:
1. **A convention of a field name, not a keyword** (as `bias`). Alternative: syntax such as `.fuse: Fuse { .supply_peak ~ rail.peak }`. Migration.
2. **The first rail by path on the start net,** not all of them, and not one named by the part. Cheap.
3. **Every part of the chain gets the start's peak;** the drop of parts before it is not subtracted (conservative). A fuse on a rail with no stated
   `.peak` gets one more `not evaluated` check.
4. **Not built:** a fault current along the chain (a diode behind a fuse sees the fuse's rated current); a reverse peak (`Rail.reverse_peak`, changes
   `Rail`, which every board uses); the same field for `Switch`, `Inductor` and other rated parts; a std input-protection module;
   `Tvs.protected_limit` stays as it is.

**Depends.** `examples/carrier_and_cards.nsm` has a `Fuse` in a chain from a rail that states `.peak &= 3.6V` (lines 83 to 96). Issue #175 is open.

## ADR 0087
**Cut.** The ADR is one page; the record of what was read from the datasheet, derived and assumed is `std/libs/README.md`. The choices below are still open.
netsmith ships its own footprints for the packages KiCad lacks. Merged as PR #255 while this digest was open. Status: provisional.

**Built.** `std/libs/` (checked: `netsmith.pretty/UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA.kicad_mod`, `netsmith.kicad_sym`), embedded by `crates/netsmith-kicad/src/stdlib.rs`, and answered as library `netsmith`; one footprint, `UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA`,
for the LT8641; `Libraries::write_tables` copies the library into a project's tables.

**Choices (7 in the ADR; each cheap now and dearer once more footprints depend on it).**
1. **Symbols as well as footprints** (taken; alternatives: an optional `symbol` in a row with a generated box; leave it to each project). Migration.
2. **Form, naming and precedence:** plain `.kicad_mod` and `.kicad_sym` files (not a generator row or a fetched file); library name `netsmith` and ids
   `netsmith:Name` in the existing keys, reserved so no project library shares it; always answered from the binary, never overridden. Migration.
3. **What the export writes:** the library only for a board that uses it. Cheap.
4. **A footprint checked only against its datasheet** (KiCad has none to compare), with one person's check recommended before a board is ordered; the
   footprint's name is the package's, not the part's. Cheap.

**Depends.** One row of `std/kicad.toml` (`netsmith:LT8641`, `netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA`) and `Libraries::write_tables` (`crates/netsmith/src/libraries.rs:113`). It added three lines to ADR 0043.

