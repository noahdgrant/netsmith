---
status: accepted (owner decisions 2026-11-04) for decisions 1 to 7 and 10 and for the syntax choices recorded under "Syntax decisions"; the board level is built (issue #52; see "Built", with its provisional readings); anchors in module files, how a module file attaches, a physical file for a `system` or a `panel`, and the exact list of what leaves are open
---

# Physical intent is written in a file of its own, apart from the circuit source

Amends ADR 0020 (item 7A), ADR 0045 (sections 3 and 8, and its open question 8) and the glossary's statement about what the editor writes. Qualifies ADR 0004 and ADR 0042 for placement. Answers issue #2; the unfinished part is the language syntax and is the owner's.

**Owner decisions (2026-11-04)** are the answers to the ranked decisions in `docs/research/durable-home-for-pcb-intent.md` section 8. This ADR records them as given. The owner confirmed decision 1 after comparing a second file with a `pcb { }` block on a fuller example; that comparison is kept in `docs/research/physical-file-syntax-options.md` section 1 as the trade-off that was decided.

## Terms used here

**Physical intent** is what the product, the case and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector, antenna or sensor must be, a fixed spacing or alignment between two parts, the face a module is on, a region with a height limit, a pattern of test pads, keepouts a board draws, local pours. It is requirements, never solutions. The solution (poses, tracks, vias, fills) stays in the layout file (ADR 0032).

**The physical file** is the file that holds physical intent. Its name, extension and header are chosen under "Syntax decisions" (`<stem>.physical.nsm`, `physical for Board { ... }`); the glossary's term is _Physical_.

**The circuit source** is the `.nsm` files that hold boards, modules, parts, interfaces and systems, with what the circuit's checks consume.

## Why

The owner's direction was a durable home for complicated boards, tested against their main test board, and unease that the schematic view shows things that are not schematic. The test board is private; the note records it only in ranges. What it showed:

- Results (poses, tracks, vias, fills) outnumber requirements by well over an order of magnitude, so the size of the intent is not the argument. A few dozen to a couple of hundred lines is the estimate for the first layout pass.
- All the externally fixed parts are inside sheet modules, and the board declares no part of its own. ADR 0004 forbids a board from naming them, so ADR 0045 section 3 and ADR 0042 would put a pose handle in each module's interface. That fills the circuit's contract with physical lines.
- A fixed spacing that a datasheet gives (two socket rows) is true of the module on every board. In the circuit source it is either repeated per board or written into the reusable circuit. That blocks issue #16.
- The trigger the first research note set for a split (about 30 lines of physical intent) is met by the first layout pass, and ADR 0001 allows no grace period after it.
- A case revision and a resistor change have different owners and different rates, and different people review them. They should not share a diff.

Rejected: leaving the intent among the circuit's declarations with an editor panel (the first research note's recommendation, withdrawn in the second). It leaves the handle clutter, the reuse problem and the unease. Rejected: a `pcb { }` block inside the board (see decision 1). Rejected for now: making the layout file hold requirements (ADR 0045 section 3 gave the reasons: no expressions, no link to other members, a second expression language); and building a shared `enclosure` definition first (decision 3).

## Decisions

**1 and 5. A file of its own (owner, 2026-11-04).** Physical intent is written in a file separate from the circuit source. It is not a view over the same file, and not a `pcb { }` block inside the board or module. Two reasons carry the decision. The people who review a case revision and a circuit change are different, and a file is the unit review and ownership already work on. And the circuit source stays strictly the circuit: a person or an agent reading it, and the editor's source pane for it, never meet a case number. A block would have given a lower cost (below) at the price of both.

**2. Layered (owner, 2026-11-04).** A part, a module and a board can each own physical intent. A part keeps the datasheet geometry it has today, the keepouts it carries (ADR 0029, ADR 0030 tier 2). The board's physical file is built first. The module layer is added later and is additive: nothing written for the board's file is thrown away. How a module's file attaches to its module and what a board's file may override are open.

**3. The enclosure is not a shared definition for now (owner, 2026-11-04).** The case numbers (outline, openings, holes, the plug at the board's end, case regions) live in a separable case section of the board's physical file, which lifts out unchanged if a shared `enclosure` definition is built later. That definition is held until the mechanical side can export data; it is tracked in issue #46. How the section is marked is open.

**4. A board's physical file reaches a part inside a module by path (owner, 2026-11-04).** The reach is read-only: it can state physical intent about the part and cannot change anything the circuit means. A module may also declare stable anchor names as an option, so that a rename inside the module changes one alias and not every board's lines. The two ways to reach the same part are a second spelling (ADR 0001); the owner chose to allow both. This qualifies ADR 0004 for the physical file only, and replaces for placement the handle through the module's interface that ADR 0042 and ADR 0045 section 3 proposed. The circuit source still cannot reach into a module. How the reach and the anchors are written is open.

**6. What stays and what moves (owner, 2026-11-04).** The circuit source keeps what the circuit's checks consume: the process, the stackup, the plane nets, who assembles a part, the designator letters, currents, impedance and pairs. They are about what the copper must do, and a fab package without them is wrong. Local pours, openings, keepouts a board draws, faces and relations between parts go to the physical file. The test is "would a circuit check consume it?".

**7. What the editor writes where (owner, 2026-11-04).** The editor writes requirements to the physical file and solutions to the layout file. A keepout drawn in the editor and the holes from `netsmith dxf import --write` are requirements and go to the physical file. This settles ADR 0045's open question 8 (the editor may write requirements, into the physical file) and the two places ADR 0029 and ADR 0045 section 9 used for them. The glossary says the editor writes both.

**10. The old form leaves, with no grace period (owner, 2026-11-04).** ADR 0001 applies. Before the first layout pass of the owner's test board, every `.place` requirement, `Outline`, `Cutout` and board-drawn `Keepout` in the circuit source becomes an error whose message names where it goes. The same change moves this repository's own uses.

## What this amends

- **ADR 0020 item 7A** said placement geometry lives in one place, the layout file. Now requirements on placement live in the physical file and solutions in the layout file. ADR 0045 had already moved requirements into the board's source; this moves them again, and ADR 0020's statement that nothing says `near` and the circuit's implications are derived (7C) stands.
- **ADR 0045 section 3** chose "a requirement in the board source". The semantics stay: ranges, tolerances, the body as the measure, relations, hard requirements that block `fab`, the solution in the layout file. Only the home changes, and the spelling inside the physical file is open (it may or may not stay `.place`). **Section 8** records `.place` as a built-in member of every part and module narrowed in the board's source; that home moves. Its **open question 8** (may the editor write constraint lines) is answered by decision 7. Sections 9 to 11 (the DXF reader, overhang, height limits) keep their rules and their provisional marks. Section 10 (h), that a module cannot state an overhang because its author does not know the case, stands until the owner says whether a module's physical file may state one.
- **ADR 0029** put a keepout the editor draws in the layout file and a board's keepouts in the board's source. A board's keepouts and the ones the editor draws are physical intent and go to the physical file. A keepout carried by a part stays in the part.
- **ADR 0032** keeps `parts`, `rotation`, `side`, `lock`, `seed` and `source` in the layout file. Entries that state a place the case fixes (a keepout's `at`, a hole locked by `dxf import --write`) are requirements and move. A requirement is fixed by the case, the fab or the datasheet; a solution is chosen by the person laying out the board.
- **ADR 0004 and ADR 0042** by decision 4, as above.
- **`GLOSSARY.md`**: the _Editor_ entry said it writes `.nsm` files and nothing else of the design. It writes the source, the physical file and the layout file. The new entry _Physical intent_ is added. _Placement requirement_ and _Layout file_ carry a pointer to this ADR.

## Migration

The owner's test board has no physical line in netsmith source today, so for it this costs nothing. What moves is this repository's:

- `examples/outline_and_pours.nsm` (`Outline`, `Cutout`, `Keepout`) and its layout file `outline_and_pours.pcb.toml` (the antenna keepout's place), and `examples/led_panel.nsm` (`Outline`).
- `docs/LANGUAGE.md`: 23 `.place.` lines and 10 `Outline {` lines in examples that `crates/netsmith/tests/guide.rs` runs. That test takes one complete file from each ` ```nsm ` block, so a board with a physical file needs a second kind of block.
- About a dozen Rust files that read or test `.place`, `Outline` and `Keepout` in `netsmith-check`, `netsmith-solve`, `netsmith-lower`, `netsmith-pcb`, `netsmith-fab` and `netsmith`, and `netsmith dxf import` (see below).
- Four skills: `netsmith-use-constraints`, `netsmith-design-a-board`, `netsmith-write-nsm` and `netsmith-read-errors`. `netsmith init` copies them into projects (ADR 0015), so a project made earlier keeps the old text.
- `README.md` and `docs/DESIGN-RULES.md`.

`netsmith dxf import` prints an `Outline`, `Cutout`s and `MountingHole`s to paste, and with `--write` puts the places of cutouts and holes in the layout file and never writes the source (ADR 0045 section 9). After decision 7 the places are requirements, so `--write` has a physical file to write. That is the first time a netsmith command writes source text, and it needs the span-based edit that ADR 0045 section 4 sketched for the editor (C5), because a comment in the physical file must survive. This ADR does not change the command.

The change that built the board level (issue #52) also removed the old form and moved this repository's uses (see "Built").

## Costs, accepted

A block would have avoided these. The owner chose the file knowing them.

- **A module's physical file must travel with the module.** The module layer (decision 2) puts a module's relative geometry in a file of its own, beside the module's source. A module used from another project or a library must bring that file. The file is optional, so a module without one is complete, but how modules are distributed has to carry it: the module's package ships its physical file with it, or a board that uses the module loses the geometry silently. The attach rule is open.
- **A rename touches two files.** Renaming a part inside a module leaves every physical file that reaches it by path with a missing path. The checker names the path and the line, as the layout file's keys break today, and the editor can rewrite both files in one edit. An anchor (decision 4) limits the damage to one alias.
- **A board's file must be paired with its board.** A rule says which physical file belongs to which board. The layout file's rule (`<stem>.pcb.toml` beside the top-level source, one table per board) is the model.
- **Three files per board** (circuit source, physical file, layout file) and a new kind of example in the guide.
- **The checker reads across files** to put a board's physical file together with the board, and a module's with the module.
- More places for a newcomer to look. The checker needs a read-only reach into modules, a new rule for a language that has kept modules closed since ADR 0004. The language needs several constructs, none designed here.

## Syntax decisions (owner, 2026-11-04)

The owner chose these from `docs/research/physical-file-syntax-options.md`, each time the option the note leaned to. The examples are in that note and use invented names; nothing has been parsed or built.

1. **Header, name and pairing.** The unit is `physical`, and the file starts `physical for Trellis { ... }` (N1: it names the board it belongs to, and one file may hold the units of several boards). The file is `trellis.physical.nsm`, found beside the source by stem, as the layout file is (P1). The word is `physical`, the glossary's term.
2. **Reach.** A board's physical file names a part inside a module by a plain path (R1), for example `.radio.row_a.place.left &= 8mm +/- 0.2mm`. The checker allows a path to cross a module boundary only to state physical intent, and an attempt to change what a part is, or a connection, is an error. A marked reach (`->`) is not added.
3. **Anchors.** An anchor is an alias for a path, declared in a module's physical file (A1: `anchor .dock = .row_a`). Anchors come with the module layer; until then a board's file uses paths.
4. **Relations between two parts.** The equation form of ADR 0045 (L1), for example `.radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm`. A `pitch` form for a row of pads is not added now. If a real fixture of a dozen or more pads is written by hand, that is the case to design one for. That case came (a row of twelve test pads, 22 relation lines, issue #81) and decision 10 is the form.
5. **A face for a module.** `.place.side` on a module is a hard requirement on every part of it (M1). A module with an exception states its parts' faces one by one (M3). A group construct (M2) is not added.
6. **A body region with a height limit.** A `Region` carried by a part and stated in the module's physical file (G2), so that it moves with the part. Until the module layer exists the keepout with `.max_height` (ADR 0045 section 11) stands. A `Region` is a new type; its exact members are not decided.
7. **The case section** is a nested `case { ... }` inside the physical file (C1), so that a shared `enclosure` definition (issue #46) can take it unchanged.
8. **Mounting holes, fiducials and labels** of a board are declared in the physical file's case section. They have no circuit meaning, and `netsmith dxf import --write` puts hole positions in the physical file (decision 7), so one thing is not split across two files.
9. **A part's `.height`** is stated on the part definition, which is the datasheet fact (a catalog column later, issue #27). A board's physical file may narrow it for the board (ADR 0045 section 11).
10. **A row statement (owner, 2026-11-04, issue #81).** A numbered run of parts on a pitch is one statement in the physical file, spelled as a range of names: `row .pads.tp1 .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`. The first and the last name the run: the same path prefix, a number at the end of the last segment, every number between them present as a part. The statement puts the parts on one common line (the other axis equal) and each neighbour at the pitch along the stated axis (`along x` or `along y`), within the window. It lowers to the same relations the equation form of decision 4 produces (the `Offset` of ADR 0045 section 12), so the placer and the check evaluate it with no new mechanism, and a finding names the row statement's file and line and the two neighbouring parts. The pitch may carry a window; a pitch without one is exact within the placer's tolerance, as a relation without a window is. It is physical file only, not in the circuit source. This is the `pitch` form that decision 4 held back, with the range of names (L3's list of paths is not used) and one statement for the pitch and the line together (L3's `align` is not added: `.a.place.y == .b.place.y` remains the way to align two parts that are not a row).
11. **Natural order for numbered paths (owner, 2026-11-04, issue #81).** A number at the end of a path segment sorts numerically wherever parts are numbered, so `.pads.tp2` has designator `TP2` and `tp10` sorts after `tp9`. Before, designators were numbered in the order of the paths as text, so `.pads.tp2` of a row of twelve was `TP5` and the names of the physical file and the labels of the board disagreed. The order stays deterministic and is a total order.

12. **A part's `.pin_protrusion` (owner, 2026-11-04).** How far the pins of a through-hole part protrude out of the far side of the board (a `Length`, ADR 0045 section 15) is stated like `.height`: a part kind may bound it, and a board's physical file narrows it for one part (`.radio.row_a.pin_protrusion &= 0.2mm`). It is a requirement of the board, because it says what the board does with a part (the pins are cut after soldering), so a physical file may state it. The owner chose the name `.pin_protrusion` on 2026-11-04, so the name is no longer provisional; the rule built on it (ADR 0045 section 14) keeps its provisional readings. Like decision 9, it may also be narrowed in the board's circuit source, which is a second spelling (ADR 0001) until the owner says which one stays.

## Not decided here

These are the owner's, because they are language design (`AGENTS.md`). `docs/research/physical-file-syntax-options.md` gives options and the effect of each.

- **How a module's physical file attaches to its module** (T1, T2 or T3 of the note), what a board's file may override of it, and whether the module layer may state an overhang.
- **A physical file for a `system` or a `panel`** (ADR 0040). Mounting holes of a panel, mated-connector alignment across boards and a panel's rails are not designed.
- **The shared `enclosure` definition** (issue #46).
- **The exact list of what leaves** under decision 10 beyond `.place`, `Outline`, `Cutout` and board-drawn `Keepout`.

## Built (issue #52, board level)

What exists: the parser reads `physical for Board { ... }` units and a nested `case { ... }` section. The loader pairs `<stem>.physical.nsm` with its source by stem (`build`, `fab`, orders, the editor), and the checker puts each unit with the board it names when the board is made, so a `panel` or a `system` that places the board sees it. A path in a physical file may cross a module boundary; the statement may then set or compare only a part's `.place`, a part's `.height` and the fields of what the unit declares itself, and a connection, a `tie`, a `pins` table or any other field is an error that says a physical file states where a part is and not what it is. `.place.side` on a module narrows the face of every part of it, at any depth, and two lines that disagree are an error naming both. A relation between two parts is accepted in the equation form and kept; the placer and the check do not evaluate it yet (ADR 0045 C2). A board's `Outline`, `Cutout`, `MountingHole`, `Fiducial`, `Label`, a `Keepout` it draws and any `.place` in its circuit source are errors that name the unit and the file. `netsmith dxf import --write` edits the physical file at the spans of its syntax tree (`netsmith::physical::edit`), keeping its comments, and puts the places of holes and cutouts there as requirements. The editor lists the physical file, checks it and draws it. The guide has a second kind of block for it.

**Provisional readings** (the simplest reading of what the owner decided, marked for the owner to confirm or change):

- The outline, cutouts, mounting holes, fiducials and labels are refused outside `case { }`, and a `Keepout` may be anywhere in the unit (decision 7 names the outline, openings, holes, fiducials and labels for the case, and "case regions").
- The rule against the old form applies to a **board's** circuit source. A module's or a part's own source may still state a `.place` or a keepout it carries (decision 2 puts the module layer later, and a keepout a part carries stays in the part).
- A part's `.height` may be narrowed in a board's circuit source as before, and in its physical file; decision 9 says the physical file "may narrow" it, so the two places are two spellings (ADR 0001) until the owner says which one stays.
- A statement in a physical file may mention only physical fields on either side of a relation. A relation that compares a `.place` field with a circuit value (`.r1.place.x >= .r1.resistance`) is refused.
- A `physical for` unit that names a module, a part or a `system` is refused, with the reason; so is a unit in a file that is not a physical file, and a definition in a physical file.
- A file with several units: one unit for each board, and a repeated unit is an error.
- **Row statement (decision 10), all PROVISIONAL.** The words `along` and `pitch` are read as words only inside a `row` statement, and `row` begins one only when a path with a leading dot follows, so `row`, `along` and `pitch` stay usable as names. Each part is tied to its neighbour (the pitch of tp(n+1) from tp(n), and the line of tp(n+1) the line of tp(n)), not to the first part, so a finding names the two neighbours; with a window the error may add up along a row of N parts (N-1 windows), as it does for the equations. A negative pitch runs the row toward smaller coordinates. A number written with leading zeros keeps the width of the first (`tp01 .. tp12`); a last number zero-padded when the first is not is refused. A run of one part is refused. A part of a row must be a part (not a module, a pin or a value) and have a `.place` (every part has one, so the check cannot fail today). The checker reports the first part at fault, the two ends first, then the numbers between them in order, and one message for a pitch that is not a length. A `row` outside a physical file is an error that says where it goes. The statement's text is the finding's source line for each of its relations; the relation's own name (`pads.tp7.place.x - pads.tp6.place.x`) is what a finding quotes. Not built: a start offset (`from`), a row of a module's parts addressed by the module (the editor's "pin here" does not write a row), and a row whose parts are not one numbered run.
- **Natural order (decision 11), PROVISIONAL.** Only a number at the end of a path segment counts as a number (`tp2a` is text), segments are compared in turn and a segment without a number sorts before one with it; two spellings of one number (`tp02`, `tp2`) are ordered by the number of digits so that the order is total. The designators of this repository's boards that change are listed in the pull request: only `examples/enclosure_sensor` has a numbered row of ten or more parts (its twelve test pads).
- The editor writes the keepouts it draws and the moves of a keepout the physical file sizes to the layout file, as before; writing them to the physical file (a requirement, decision 7) is not built. A keepout whose place the physical file states is shown and not moved.
