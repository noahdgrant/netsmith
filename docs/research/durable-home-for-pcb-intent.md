# A durable home for PCB intent

Design note for issue #2, second round (the owner's direction of 2026-11-04). Written while the owner was away. It decides nothing and builds nothing: every syntax below is invented to compare architectures, and language-design choices are the owner's (`AGENTS.md`). Vocabulary follows `GLOSSARY.md`. Snippets marked `nsm-new` use constructs that do not exist.

This repo is public. The owner's main test board is private, so this note describes it only in ranges ("dozens", "a few") and every name and number in an example is invented. The full inventory, with the real details, is kept outside the repo.

The owner's direction, in short: not the cheapest change but a long-term solution for complicated boards, tested against the main test board, which has many PCB constraints; and unease that the schematic view shows things that are not schematic. The earlier note ([where-schematic-and-pcb-intent-live.md](where-schematic-and-pcb-intent-live.md)) was based on tiny examples and recommended keeping one file. This note redoes the question at the real board's scale.

## Summary

- At full scale the intent is small and concentrated, and it sits on the wrong side of a module boundary. Dozens of parts, about a dozen facts that come from outside the board, a handful about copper, and several hundred copper objects that are results. All the externally fixed parts are inside sheet modules; the board declares no part of its own.
- That breaks the earlier recommendation in three places. A board-level `.place` cannot name a part inside a module (ADR 0004), so each module's interface fills with pose handles, which is the opposite of keeping the circuit clean. Relative geometry that belongs to a reusable module (a fixed row spacing, an antenna aligned to its connector) has no home that travels with the module, so it cannot help issue #16. And the trigger the earlier note set for splitting the file (a real board with more than about 30 PCB lines) is met by the first layout pass.
- The useful cut is not "schematic versus PCB". It is three kinds of fact: what the circuit needs of copper (current, impedance, a pair), what the product needs of the board (the case), and what the layout designer decided (the solution). The first stays in the circuit source because the checks consume it. The second and the layout intent that goes with it move to a physical source. The third stays in the layout file.
- Four architectures are compared in section 5. Recommendation: a physical source of its own, layered, so that a part keeps its datasheet keepouts, a module keeps its relative geometry and anchors, a board keeps the case and the placement decisions, and an `enclosure` definition can later be shared with the mechanical repo. Build the board layer first; the module layer and the enclosure then add to it with no second migration.
- Section 8 lists the ten decisions the owner must make, ranked. The first three decide the shape; the rest can follow.

## 1. The question and the test board

Issue #2 asked where PCB constraints live. The owner is unsure whether they belong in `.nsm` at all, and said the discomfort is that the schematic view shows things that are not schematic.

The test board, as far as this note needs it:

- one multilayer board with dozens of placed parts, laid out and built once, and now ported to netsmith from KiCad for a revision that will have more parts;
- a sheet structure of a dozen or more sheet modules, ported one to one into modules; the board declares only module instances and its process;
- a larger sibling in the same KiCad project: a panel of several plug-in boards on a base board, joined by board-to-board connectors, which is a `system` and a `panel` in netsmith terms (ADR 0040);
- a mechanical repository for the case that, when I read it, held no drawing or export.

The port has no PCB line in source today. Everything about the PCB side is in the KiCad board file, where a requirement and a result look alike.

## 2. What the inventory shows

The private inventory lists every kind of PCB-side fact with its count, its home, its owner, how often it changes and whether it is a requirement or a result. In ranges:

| Kind | How many | Requirement or result | Owner role | Changes |
|---|---|---|---|---|
| Outline, cutouts | one outline, a cutout or two | requirement | mechanical | with the case, rarely |
| Mounting holes, fiducials, tooling | none on the single board, a few on the panel | requirement or process | mechanical, fab | rarely |
| Fixed positions of connectors, antennas, sensors | under a dozen | requirement (my reading) | shared | with the case or a module |
| Relations between parts (fixed row spacing, an aligned pair) | a few | requirement | electrical, from a datasheet | rarely |
| A region with a height limit (a plug-in module's body) | one or two | requirement | mechanical | with the case |
| Keepouts | a handful of zones, all carried by parts | requirement, already part-owned | part (datasheet) | with the part |
| Face (front or back) | every part; most of it decided per module | decision | layout | each pass |
| Test point pattern on a fixture pitch | about a dozen pads, new in the next revision | requirement | test | with the fixture |
| Net classes and track widths | one class, about ten widths used by hand | derivable from currents (ADR 0036) | electrical | with the loads |
| Impedance, differential pairs, length matching | one or two candidates now, a few pairs next | requirement | electrical | with the interfaces |
| Stackup, process, planes | one each | process | fab | rarely |
| Local pours | one or two | decision | layout | each pass |
| Parts' poses, tracks, vias, filled zones, silk, reference text | several hundred | result | layout | every pass |
| Board-to-board alignment (pin 1, pitch, shared holes) | a few mated pairs on the panel | requirement | shared | rarely |

What follows from it:

1. **Results outnumber requirements by well over an order of magnitude.** The language's home for intent is a few dozen lines to a couple of hundred, never thousands, a small fraction of the circuit source by lines. Size is not the argument for or against a split.
2. **Almost all of the intent sits on parts inside modules.** Every externally fixed part belongs to one of about half a dozen sheet modules. The board names none of them without a handle.
3. **Most requirements are unrecorded.** Only a small fraction is written down as a requirement in the KiCad file (a few dimension drawings); the rest is visible only as a solved position. The PCB repo has no requirements document and the mechanical repo has no data. A language home would create the record, not move one.
4. **Part-owned intent already exists and works.** The keepouts are in the footprints. ADR 0030's tier 2 and ADR 0029 handle them in the part.
5. **Physical intent about nets is mostly derived or electrical.** Track widths follow from currents (ADR 0036), a pair follows from a bus interface, and a 50 ohm feed is already a value of the part. What is not covered is a local pour and a rule for the board's own classes.
6. **The enclosure is not shared today.** There is no producer on the mechanical side. `dxf import` reads an outline, cutouts and hole circles (ADR 0045 section 9); heights, a region over which a module hovers, a connector mouth and a window have no export.
7. **The hardware is more than one board.** The same project holds a panel of plug-in boards. Whatever the home is, it must work for a `system` and a `panel` and for alignment across boards (ADR 0040 M2 and C7, not built).

## 3. What the earlier note leaves unsolved at this scale

The earlier note's recommendation was: keep one file, fix the places where a fact has two homes, give the editor a Constraints panel, and revisit when a real board passes about 30 PCB lines. Some of it stands. The solution stays in the layout file, the three kinds of data it separated are right, and its worry about a second spelling is right. At this scale, though, it leaves these unsolved:

1. **The module boundary.** The earlier note treated `.place` as a board-level line about a part. On the test board the board has no part of its own. ADR 0004 forbids `.radio.j1.place &= ...` from the board; ADR 0042 and ADR 0045 section 3 answer with a handle value in the module's interface. For the test board that is a handle in about half a dozen modules, and one for each part that a module has several of. The module interface, which is the circuit's contract, gets pose members, and the schematic view then shows more non-schematic things, not fewer. An editor panel lists the lines; it does not move them.
2. **Reuse (issue #16).** A fixed row spacing or an aligned antenna and connector is a fact of the module, true on every board that uses it. In one file it is either repeated in each board's constraint lines or put in the module's own source, which puts geometry into the reusable circuit. In both cases it leaves the module with no clean layout of its own to hand to the next board.
3. **The trigger has already fired.** The earlier note said to revisit at 30 PCB lines. At the form ADR 0045 estimates (about eight lines per constrained part) the first layout pass of the test board is a few dozen to a couple of hundred lines. Waiting means moving them later, and ADR 0001 allows no grace period.
4. **Two owners, two rates.** A case revision should be a small diff reviewed by the person who owns the case. In one file it edits the same file as a resistor change in the same pull request. The circuit changes weekly; the case rarely. A reviewer of the circuit reads case lines, and the reviewer of the case reads a circuit file many times longer.
5. **The numbers.** The earlier note's evidence was "7 `.place` lines in the examples". Counted on this tree the examples have none; the guide has a few dozen. Neither is a board with dozens of constrained parts, so neither tells how the file feels at scale.
6. **Net-level intent was not inventoried.** Widths, impedance and pairs are intent about nets, which only a board names (ADR 0005). The earlier note's options did not say where a pair or a local pour goes.
7. **The editor writes two files.** The glossary says the editor writes `.nsm` and nothing else of the design; it also writes the layout file. Under any option where requirements move, the editor's "pin here" needs a defined target file.
8. **A panel and a system have intent too.** Mounting holes, mated-connector alignment and a panel's rails are neither one board's circuit nor one board's layout. One file per board has no place for them.

## 4. The cut: three kinds of fact

Ask three questions of any fact.

1. **Does the circuit need it, and would the checks consume it?** A current, an impedance, a pair, a plane's net, who assembles a part, a process. It is about what the copper must do. It stays in the circuit source, stated where the circuit states it (on a rail, a net or an interface). The physical side reads it. This is also where KiCad and Altium put net classes: they come from the schematic.
2. **Does the product need it, or the layout designer's intent about positions?** The outline, the openings, the holes, the plug at the board's end, a keep-in region with a height limit, a face for a module, a relation between two parts, a test pad pattern. It is about where and how big. Nothing in the circuit checks it. It is physical intent: requirements, not solutions.
3. **Is it a solution?** A pose, a track, a via, a filled zone, a silk position. It is the layout file's, and never written by a person as a requirement.

The earlier note's three kinds were schematic-level, placement and enclosure; the cut here moves `.assembly`, `.process` and the plane nets to where they already are, the circuit source, and puts placement and enclosure together as physical intent, because on the test board the same people change them at the same time. The glossary already calls a board "described as a manufacturing process, parts, and the modules placed on it", so the process stays on the board.

A third place for a part's datasheet geometry already exists: a keepout carried by a part (ADR 0029, ADR 0030 tier 2). Four homes in total: the part, the circuit source, the physical source, the layout file. Any architecture below picks what the physical source looks like and how far it reaches.

## 5. Four architectures

Each is tested on six cases from the test board, rewritten with invented names and numbers:

- **P1** a plug that ends the board at the bottom edge, hand-soldered;
- **P2** a plug-in radio on two socket rows with a fixed spacing and a body region over the board with a height limit;
- **P3** a patch antenna and a coax receptacle on one centreline near an edge, with the datasheet's clear area round the feed;
- **P4** a dozen test pads on a fixture pitch, all on one face;
- **P5** one sheet wholly on the back, the rest on the front;
- **P6** a 50 ohm feed and a USB pair.

### 5.1 Architecture A. Everything in the circuit source, extended (the earlier note's option A)

The board's `.nsm` and each module's `.nsm` hold the requirements. Each module that has a fixed part declares a handle. The editor offers a Constraints panel.

```nsm-new
module RadioSockets {
    .row_a: Socket1x10
    .row_b: Socket1x10
    // The board may place the pair through this handle.
    .pose: Pose
    .row_a.place ~ .pose
    // The module's radio has its rows 20 mm apart.
    .row_b.place.y - .row_a.place.y == 20mm +/- 0.05mm
}

board Widget {
    .radio: RadioSockets
    .radio.pose.left &= 4mm +/- 0.2mm
    .radio.place.side &= "top"
    .power.place.side &= "bottom"
    .input.pose.bottom >= .outline.height      // the plug ends the board
}
```

- **Reuse.** The relation sits in the module, which makes the module reusable with its geometry, at the cost that the circuit source now carries physical lines. A second module needs the same ritual.
- **Hierarchy.** Every constrained part needs a handle through each module above it. A board that nests modules two deep has handles at both levels.
- **Editor.** The panel lists `.place` lines from many files. "Pin here" writes into a module's source, which another board also uses.
- **Agents.** One kind of file; a long read to find the lines. The skill says "constraints go in the module that owns the part" and "handles are poses", two rules for the same thing.
- **Review diffs.** A case revision touches the board file and, when a handle moves, the module files. The circuit reviewer reads them.
- **Migration.** None now, which is its only advantage. Moving later costs the same as moving now, plus what the test board accumulates.
- **Verdict on the test board.** P1 to P4 need handles in half a dozen modules. P5 is fine. P6 is fine and stays in the circuit source in every architecture. The owner's unease is not answered.

### 5.2 Architecture B. One physical file per board, reaching into modules by path

The circuit source holds circuit facts only. A sibling file for each board holds the physical requirements, in the same language, and may name a part inside a module by its path. It can read the circuit and cannot change it.

```nsm-new
// widget.physical.nsm
physical Widget {
    .outline: Outline { .width &= 40mm  .height &= 70mm  .corner_radius &= 1mm }
    .opening: Cutout { .x &= 2mm  .y &= 60mm  .width &= 4mm  .height &= 3.5mm }

    .input.plug.place.bottom >= .outline.height          // P1: the plug ends the board
    .radio.row_a.place.left &= 4mm +/- 0.2mm             // P2
    .radio.row_b.place.y - .radio.row_a.place.y == 20mm +/- 0.05mm
    .radio.place.side &= "top"                           // P2, P5
    .power.place.side &= "bottom"
    .gnss.antenna.place.y == .gnss.coax.place.y          // P3
}
```

- **Reuse.** The row spacing above is repeated by every board that uses the radio module. A second board states it again, and a change to the datasheet means editing each. Issue #16 gets no help here.
- **Hierarchy.** Path reach works at any depth with no handle. Renaming a part inside a module breaks the physical file; the checker names the missing path and its line, as the layout file's keys already break.
- **Editor.** A tab for the physical file; the PCB view reads it; "pin here" has one target. The schematic view never shows it by construction.
- **Agents.** Two files per board, one rule: "where and how big goes in the physical file". The agent that works on the circuit does not read it, which is a smaller context. The agent that lays out reads it and the layout file.
- **Review diffs.** A case revision is a diff of the physical file only, reviewed by the case's owner. A circuit change is a diff of circuit files only. This is the biggest gain.
- **Migration.** Moves `Outline`, `Cutout`, `Keepout`, `MountingHole` and every `.place` out of the board source; the guide has a few dozen such lines in a dozen blocks, a few dozen test files embed them, and a few examples use them. The test board has no such line. Costs once, now, and the cost only rises with each board written in the old form.
- **Verdict on the test board.** P1 to P5 fit with no handle. P2's relation is repeated per use, and a panel or system has no file of its own. A good first step, and a subset of C.

### 5.3 Architecture C. Layers: part, module and board physical sources

A part keeps its datasheet keepouts (as today). A module has an optional physical file that travels with it and holds its relative geometry, its own face rule if it has one, and a few named anchors that are its physical interface. A board's physical file places module anchors and states the case. A layout file at each level holds the solution, so a module's layout can be regenerated on its own (ADR 0030 goal 3).

```nsm-new
// radio_sockets.physical.nsm: travels with the module
physical RadioSockets {
    // Fixed by the radio's datasheet, true on every board.
    .row_b.place.y - .row_a.place.y == 20mm +/- 0.05mm
    .body: Region { .width &= 32mm  .height &= 27mm  .max_height &= 6mm }
    // The module's physical interface: the board places this, never the sockets.
    .anchor ~ .row_a.place
}

// antenna_front.physical.nsm
physical AntennaFront {
    .antenna.place.y == .coax.place.y                // P3, a module fact
}

// widget.physical.nsm
physical Widget {
    .outline: Outline { .width &= 40mm  .height &= 70mm  .corner_radius &= 1mm }
    .input.plug.place.bottom >= .outline.height      // P1
    .radio.anchor.left &= 4mm +/- 0.2mm              // P2
    .radio.place.side &= "top"
    .power.place.side &= "bottom"                    // P5
    .eol.place.side &= "bottom"                      // P4: the pattern belongs to eol
}
```

- **Reuse (issue #16).** A module with a physical file is reusable with its geometry. A second board imports the module and states only where its anchor goes. The module's regenerated layout (its own layout file) applies to each use. This is the only architecture that gives a sheet its own physical form.
- **Hierarchy.** The module's anchors take the place of ADR 0042's handles, but they live in the module's physical file, so the circuit interface stays clean. At each level the reach rule is the module boundary of ADR 0004: a module's physical file reaches its own parts; a board's reaches a module's anchors, or any path if the owner picks path reach (decision 4).
- **Editor.** The PCB view, when a module is selected, shows that module's physical file, then the board's. "Pin here" writes the board's file or the module's according to what is selected. The schematic view shows none of it.
- **Agents.** Three kinds of source and one rule per kind. An agent adding a part to a module writes its keepout in the part and a relative fact in the module's physical file. An agent laying out a board reads the board's file and the modules' anchors.
- **Review diffs.** As in B, plus a module's geometry change appears as a diff of the module's physical file, which every board using it picks up. That is a change a board owner wants to see: the board's checks run again.
- **Migration.** As in B, plus a module-level step that is additive. Nothing written for B is thrown away if B's board file may reach by path (decision 4); it must be rewritten to anchors if the owner chooses strict encapsulation.
- **Cost.** New constructs: a `physical` unit, anchors and the reach rule, a `Region`. More files: on the test board about half the modules would have a physical file, most with two or three lines.
- **Verdict on the test board.** P1 to P5 fit, P2 and P3 fit once, and the board's file is a page. The unit of reuse, the review and the editor all line up with the owner's two worries.

### 5.4 Architecture D. Enclosure-led, with C beneath

The case is the source of truth and lives as a definition owned by the mechanical repo. The board's physical file says "this part meets that feature", not a number.

```nsm-new
// snap_case.enclosure.nsm: owned by the mechanical repo, copied or pinned into the project
enclosure SnapCase {
    .inner: Outline { .width &= 40mm  .height &= 70mm  .corner_radius &= 1mm }
    .port: Opening { .edge &= "bottom"  .x &= 20mm +/- 0.1mm  .width &= 24mm  .overhang &= 0.4mm }
    .lid_rib: Keepout { .x &= 0mm  .y &= 30mm  .width &= 40mm  .height &= 18mm  .max_height &= 3mm }
}

// widget.physical.nsm
physical Widget {
    .case: SnapCase
    .input.plug.place ~ .case.port                   // P1, no numbers
    .radio.anchor.left &= 4mm +/- 0.2mm
}
```

- **Reuse.** Several boards and a system share one case; a plug-in board and its carrier share a frame (ADR 0045 C7). The `enclosure` is a second reusable definition kind besides the module.
- **Editor.** The case appears as a read-only underlay in the PCB view; a requirement that names a feature is shown on the feature.
- **Agents.** A new vocabulary of features (opening, rib, light hole). Agents cannot read the case drawing; they need the file.
- **Review diffs.** A case revision in the mechanical repo is a diff of one enclosure file, which the board repository picks up. ADR 0041 says a person with the binary alone must do everything, so the file is copied or pinned into the project, as `dxf import` already writes source for you to paste.
- **Migration.** Additive over C. The board's numbers move into the enclosure file once it exists.
- **Cost and risk.** The mechanical repo has no data today, so the producer is unproven: nothing exports heights, openings, a module region or a connector mouth. Building the `enclosure` kind before a real case has used it risks a vocabulary that is wrong for the second case. The earlier note held it for the same reason.
- **Verdict on the test board.** It fits P1 and P2 best and would make a case revision the cleanest. It cannot be the first step, because there is nothing yet to put in it. As a later addition to C it costs nothing now.

### 5.5 The in-file block variant

One more spelling of B and C is a `physical { }` block inside the board or module declaration, in the same file, as the earlier note's option C proposed. The layering and the reach rule would be the same; what changes is that a case revision still edits the file that holds the circuit, and the circuit view still has to fold the block. The review-diff gain is lost; the pairing problem of two files goes away. This is decision 5. I count the earlier note's use of the keyword `pcb` as one more reason to pick another word: the glossary lists "PCB" under Avoid for Board.

## 6. Comparison at the test board's scale

| | A. circuit source | B. physical file per board | C. layered | D. enclosure-led |
|---|---|---|---|---|
| Needs a handle through every module | yes | no | anchors, in the module's physical file | as C |
| Module keeps its own geometry (issue #16) | in the circuit source | no | yes | yes |
| A case revision is its own diff | no | yes | yes | yes, in one file |
| Schematic view shows only the circuit | by view | by file | by file | by file |
| Works for a system and a panel | no | no | partly | yes, with the case as a shared definition |
| Number of language additions | handles, relation, module face | `physical`, reach rule | B plus anchors, `Region` | C plus `enclosure`, `Opening` |
| Cost of moving from today's one-file form | none now, all later | once | once; module layer later is additive | additive on C |
| Evidence it is the right shape | none yet | partial | fits all six cases | not provable until the mechanical side has data |

## 7. Recommendation

**Choose C, build B first, and hold D.** In order:

1. **Make the physical source a separate unit from the circuit source** (decision 1). The earlier note's reading that a split can wait is withdrawn. The test board's first layout pass will pass its own threshold, ADR 0001 makes a late move costly, and the owner's own worry is about the schematic view.
2. **Build the board-level physical file first** (architecture B). Everything a board needs from it is on the test board already: the outline, cutouts, the externally fixed parts, the faces of modules and the test pad pattern. It is also what an owner who stops there still has.
3. **Add the module layer** (architecture C) with the second board that uses a module with fixed geometry, or the first time the test board reuses a sheet. It is additive if board-level files may reach by path (decision 4). My lean is path reach for the board's file, which is read-only and physical-only, and anchors only as an option a module author uses to pin a stable name; the layout file already reaches by path (ADR 0032), and handles through the circuit interface are the thing the owner wants to avoid. Two ways to reach, though, is a second spelling (ADR 0001): if the owner prefers one, the choice is path reach everywhere, or anchors everywhere with a one-time rewrite of whatever B produced.
4. **Keep what the circuit consumes in the circuit source**, as section 4 draws it: process, stackup, plane nets, assembly, designator letters, current, impedance and pairs.
5. **Hold `enclosure` until the mechanical side can export something** (decision 3), but give the board's physical file a clearly separable case section so that it lifts out later. I could not check how the owner's CAD exports; this is the weakest part of the recommendation and the first thing to ask.
6. **Let the editor write both new files** and have the glossary say so (decision 7): requirements go to the physical file, the solution to the layout file.

Risks. More files per board; a rename inside a module breaks a path reference with a named error; three new constructs (the unit, anchors, a region) are language and need the owner's syntax; a physical unit for a `system` or a `panel` is not designed here; and C trades the handle clutter for module-level files whose payoff depends on real reuse that the test board has not yet shown. If the owner thinks reuse is unlikely, B alone is the honest stopping point.

## 8. Decisions for the owner, ranked

1. **Is the boundary a file or a view?** Keep PCB intent in the circuit source with an editor panel (A), or give the physical source its own file (B, C, D). This is the question the owner asked. Recommend its own file.
2. **Do reusable modules own physical intent** (relative geometry, a body region, anchors), or only boards? Recommend modules do, for issue #16. Decides B against C.
3. **Is the enclosure a shared definition now, or numbers in the board's physical file until the mechanical side exports data?** Recommend numbers now, a case section that lifts out later. Decides whether D is built or held.
4. **How does a board's physical file reach a part inside a module: by path, or only through anchors the module declares?** Recommend path for the board's file, a read-only reach that cannot change the circuit. The other choice is strict encapsulation, which costs a rewrite if B is built first.
5. **A separate file, or a block in the same file?** Same layering either way. Recommend a separate file for the review-diff and ownership gains; the cost is pairing two files.
6. **What stays in the circuit source?** Recommend the rule "the circuit's checks consume it": process, stackup, plane nets, assembly, designator letters, current, impedance and pairs. Local pours, openings, keepouts a board draws, faces and relations go to the physical source.
7. **Which file does the editor write for which kind of fact?** Requirements to the physical file, solutions to the layout file; the glossary says the editor writes both. This also settles ADR 0045's open question 8 and the earlier note's questions 7 and 8: a keepout the editor draws, and the holes `dxf import --write` puts in the layout file, are requirements and go to the physical file.
8. **Relations between two parts**, such as a fixed row spacing or an aligned pair (ADR 0045 C2). The test board needs them. The form `a.place.y - b.place.y == 20mm +/- 0.05mm` from the ADR, or a named relation. Language syntax: the owner's.
9. **A face for a module** (`.place.side` on a module), and a body region with a height limit as a first-class thing. The test board needs both; neither is built. Language syntax: the owner's.
10. **When to move.** Recommend before the test board's first layout pass, when its source has no PCB line to rewrite. The cost then is the guide, the examples and the test files of this repo, once. Because ADR 0001 allows no grace period, the old form leaves with the change.

When these are decided, an ADR is warranted: the choice is hard to reverse, surprising without context and a real trade-off. It should amend ADR 0020 7A and ADR 0045 sections 3 and 8, and `GLOSSARY.md` needs the terms the owner chooses for the new unit, its anchors and the region. I wrote no ADR and changed no glossary entry, because nothing is decided.

## 9. What I did not check

- The owner's CAD and the real case: nothing in the mechanical repo, so section 5.4's producer is a guess.
- How a physical unit would work for a `system` or a `panel` (the panel's per-cell placement is in the layout file, ADR 0040). It needs its own pass.
- Whether KiCad's own mechanisms (design blocks, rule areas, custom rules) could carry any of this on export. The earlier note's comparison of tools stands and I added none.
- The count of requirement lines for the next revision is an estimate from ADR 0045's eight-lines-per-part figure and the sheets' declarations; nobody has written them yet.
- The issue tracker of the private PCB repo, which `gh` could not reach. The requirements in its issues are not in the inventory.
