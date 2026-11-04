# Where schematic intent and PCB intent live

Research note for issue #2 (the owner's question, 2026-11-03). Written while the owner was away. It decides nothing: every syntax in section 3 is invented to compare options, and language-design choices are the owner's (`AGENTS.md`). Vocabulary follows `GLOSSARY.md`; the numbers and the case in the examples are made up.

The owner said: "I think we might need to think more about what information goes in each place. It's hard for me to tell if it's a good UX to have the PCB constraints in the .nsm files or if the .nsm files should be strictly for schematics and all PCB-related things go in a separate file. I think we should do more research on this and explore more options."

Issue #2 itself asks for three things that would add to this pile: a minimum or exact gap and alignment between parts (ADR 0045 C2), a face for a group of parts (`side` for a module), and agreement between the annealer and the overhang check (ADR 0045 section 10 (i)). They are the "min gap, alignment, face for a group" of section 3.

## Summary

- Today a board's `.nsm` file holds the circuit and also the outline, keepouts, height limits and requirements on where parts go. The layout file holds solved positions. The manifest holds three policy switches that touch the board. That is already three places, and a few facts sit in the wrong one or in two (section 1.2).
- The tools I read all keep schematic and board in separate files, and the schematic never holds placement. The two code-first tools differ: tscircuit puts `pcbX`, `schX` and the rest on the same element in one file, atopile leaves layout to KiCad. I found no tool that states a gap or alignment between parts as a rule in the pages I read (section 2).
- Four options are in section 3. Recommendation: do not split the file yet. In the examples there are 7 `.place` lines in total, so nothing shows the file is too crowded. Fix the places that break "one place per fact", and let the editor give the PCB lines their own panel. Split only when a real board shows the pain. The owner may want otherwise, and section 5 lists the questions that decide it.

## 1. What netsmith has today

### 1.1 Inventory

Sources: `docs/LANGUAGE.md` ("The shape of a board", "Where a part goes", "The layout file", "Projects and netsmith.toml"), ADR 0020, 0029, 0032, 0045 (sections 1 to 4 and 8 to 11), `std/pcb.nsm`, `crates/netsmith-pcb/src/layout.rs`, `crates/netsmith/src/project.rs`.

| Information | Where it is stated | Written by | Built? |
|---|---|---|---|
| Parts, modules, nets, rails, catalog choices | `.nsm` | person or agent | yes |
| Who assembles a part (`.assembly`), the designator letters | `.nsm`; default in `netsmith.toml` (`assembly`) | person | yes |
| Manufacturing process of a board (`.process`) | `.nsm` | person | yes |
| Outline (`Outline`), cutouts (`Cutout`) | `.nsm` for size; place of a cutout in `.nsm` (`.x`, `.y`) or the layout file | person; `dxf import` | yes |
| Keepout size and what it forbids | `.nsm` (a board member, or a member of a part in the part's frame) | person | yes |
| Keepout drawn in the editor (size, layers, `forbid`/`allow`) | layout file, `[Board.keepouts.name]` with a `size` | editor | yes |
| Height limit of a zone | `.nsm`, `Keepout.max_height` (provisional, ADR 0045 section 11) | person | yes |
| Height of a part | `.nsm`, `.height` on the part or the board | person | yes; no catalog column |
| Mounting holes, fiducials | `.nsm` as parts; place in a fixed outline's corners, or the layout file with `lock = true` | person; `dxf import --write` | yes |
| Requirement on where a part or module goes (`.place.x`, `.y`, `.left`, `.right`, `.top`, `.bottom`, `.rotation`, `.side`) | `.nsm`, on the board, as ranges and bounds | person or agent | yes (ADR 0045 C0a, C0b) |
| Overhang | `.nsm`, an edge field whose window is at or beyond the outline's edge; or a lock in the layout file | person | yes, provisional (section 10) |
| Solved position, rotation, face, reference text side, lock, seed | layout file `<stem>.pcb.toml` | placer, editor | yes |
| Whether an unmet requirement stops `fab` (`constraints`), whether a guideline does (`guidelines`) | `netsmith.toml`, `[project]` | person | yes |
| Spacing or alignment between two parts | nowhere | | no (C2) |
| A `side` or `rotation` for a whole module | nowhere: a module's `.place` has no `rotation` or `side` | | no |
| Position of a symbol on the schematic sheet | nowhere: the schematic is a grid of cells with labels (ADR 0021, "Limits") | | no (ADR 0019 designed a sidecar) |
| Where a mating connector's pin 1 is on the board, stack height | nowhere | | no (ADR 0040 M2, C7) |

### 1.2 Where it breaks the rules the ADRs set

ADR 0045 recommended "one place for the requirement (the board source), one for the solution (the layout file)". Reading the code against that:

1. **A keepout can live in three places.** In the board's source, in a part's source (its own frame), and in the layout file as a keepout the editor drew, with its size, layers and what it forbids (`KeepoutPlace::is_drawn`). The third is a requirement stored where the solutions are. ADR 0001 (one correct way) and ADR 0045's "an entry that disagrees is a finding, not a second truth" point the other way.
2. **A case's hole positions are a requirement stored as a solution.** `netsmith dxf import --write` puts the holes into the layout file as `lock = true` parts with `source = "hand"` (ADR 0045 section 9). The reason is sound (the source holds no positions, ADR 0032), but a hole's place comes from the enclosure, which is what `.place` was built to state. A locked hole also states every edge it is past under the current overhang rule, which owner decision on #11 changes (see the ADR 0045 section 10 edit in this PR).
3. **Two files hold facts about the same keepout or cutout.** A cutout has `.x`, `.y` in source or an entry in the layout file; stating both is an error. The error keeps it to one place per cutout, but it is a rule a person must learn.
4. **`.height` is on the part, `.max_height` is on a keepout.** One is a fact about the part, the other about the case. That split is right, but the part's height has no home in the catalog yet, so for now it is written in the board source, next to case numbers.
5. **The manifest has policy switches that change what a board means.** `constraints = "error" | "warn"` decides whether an unmet requirement stops `fab`, and the strict reading of an unknown height hangs on it. The manifest table in `docs/LANGUAGE.md` ("Projects and netsmith.toml") does not list `constraints`, `series` or the `[supply]` table; they are described in other sections.
6. **The glossary says the editor "writes `.nsm` files and nothing else of the design"**, but it also writes the layout file (`crates/netsmith/src/editor/mod.rs`, `write_atomic(file, &layout.to_text())`). One of the two should change.
7. **How big the problem is.** Across all `examples/*.nsm` (683 lines) there are 7 `.place.` lines, five in `outline_and_pours.nsm` and two in `led_panel.nsm`. The ADR 0045 invented widget needs about eight lines per constrained part. Nobody has yet written a board with enough lines to hurt.

## 2. How other tools split schematic intent from PCB intent

I opened each page cited below. Where a page did not say, I say so; I did not fill gaps from memory.

| Tool | Schematic side | PCB side | How intent crosses | Source |
|---|---|---|---|---|
| KiCad 9 | Each sheet is its own `.kicad_sch` file and a complete schematic; a sheet can be used several times. Net classes may be created in the schematic or in Board Setup, and a label field `Net Class` assigns one. | Board Setup holds minimums and net classes. Custom rules are text in Board Setup; the constraints I saw named are `courtyard_clearance`, `disallow`, `skew`, `length`, `track_angle`, `via_count`, `creepage`. Rule areas are board objects that keep out tracks, vias, pads, zone fills and footprints. | "Update PCB from Schematic is the preferred way to transfer design information from the schematic to the PCB." Pins match pads by number. | [pcbnew](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html), [eeschema](https://docs.kicad.org/9.0/en/eeschema/eeschema.html) |
| Altium Designer | Components, nets and classes derived from the schematic. With the Constraint Manager, constraints can be entered in the schematic editor too. | The board, its footprints and placement. Without the Constraint Manager, constraints are design directives plus the PCB Rule and Constraints Editor. | An Engineering Change Order: Design, Update PCB Document, validate, execute. Constraints set from the schematic side "are transferred to the PCB design during the regular ECO process." | [Managing design changes](https://www.altium.com/documentation/altium-designer/sch-pcb), [Constraint Manager](https://www.altium.com/altium-designer/features/constraint-manager) |
| atopile | `.ato` source: modules, connections, values. The sample `main.ato` also declares the stackup. | The `.kicad_pcb`, laid out in KiCad. | The compiler "solves constraints, picks parts, runs checks, and updates your KiCad layout." What it does with hand edits in KiCad is not covered by the README. | [README](https://github.com/atopile/atopile), [docs](https://docs.atopile.io/) |
| tscircuit | One TSX file, one element per part. | The same element: `pcbX`, `pcbY`, `pcbPositionAnchor`, `pcbRotation`, `layer`. | No transfer: `schX`, `schY`, `schRotation` and the `pcb*` props sit side by side on the element, in millimetres by default. | [layout properties](https://docs.tscircuit.com/guides/tscircuit-essentials/layout-properties) |
| Horizon EDA | Schematic and netlist are deliberately separate representations. Hierarchy is blocks you instantiate; net classes are set per block. | A board editor with a Rules page: rules are evaluated top to bottom, the first match applies. | "To board" starts the place tool with the packages of the selected symbols; the board editor reloads the netlist. Where rules are stored on disk: not covered in the pages I read. | [why another EDA package](https://docs.horizon-eda.org/en/v2.5.0/why-another-eda-package.html), [schematic editor](https://docs.horizon-eda.org/en/latest/imp-sch.html), [rules](https://docs.horizon-eda.org/en/latest/rules.html) |
| LibrePCB | Schematic editor. | Board Setup (F7) has design rules (manufacturer capabilities, used to compute things like restrings) and DRC settings (verification only). The outline is a polygon on the board outline layer. | "For every component in the schematic, you need to place a device in the board." Airwires show what is still unrouted. How the files are stored: not covered in the pages I read. | [create board](https://librepcb.org/docs/quickstart/create-project/board/) |

What the six have in common, as far as these pages show:

1. **Placement never lives in the schematic.** Net classes (a width, a clearance) can be set from either side in KiCad and Altium; where a part sits cannot. tscircuit is the exception, and it is code, with the two sets of coordinates as separate props.
2. **Manufacturing rules belong to the board** in every tool (KiCad's Board Setup, LibrePCB's Board Setup, Horizon's rules). netsmith's `.process` already does this, and it is a member of the board in `.nsm`.
3. **The code-first tools differ on who owns layout.** atopile hands it to KiCad and updates the file; tscircuit states it in source. netsmith is between them: the source states requirements and the layout file holds the solution.
4. **I found no tool whose documentation states a gap or alignment between two parts as a design rule.** KiCad has courtyard clearance as a check and an align tool as an editing command, not a rule that persists. This is a limit of the pages I read, not a proven absence. If it holds, C2 is a feature the others do not offer, which argues for care in its syntax.
5. **Hierarchy and the board.** KiCad sheets and Horizon blocks are schematic structure that is flattened for the board. Horizon says the topmost net's name and class win when flattened. This matters for issue #16 (see the other note in this folder).

Not read: Altium rooms and component classes; the KiCad custom rules page (it returned 404 and the main manual excerpt did not reach it); LibrePCB's file format page (404). The comparison for KiCad rule syntax is therefore partial.

## 3. Options

The three kinds of data in the owner's question:

- **Schematic-level data.** What the circuit is: parts, modules, nets, rails, catalog choices. Also `.assembly` and `.designator`, which are about the circuit and the order, not the copper.
- **Placement constraints.** Between parts or on a group: a minimum gap, alignment, a face for a module.
- **Enclosure constraints.** Where the case, panel or carrier fixes things: outline, cutouts, holes, keepouts, height limits, a connector's mouth.

Every option keeps the solution (positions the placer or a person chose) in the layout file, as ADR 0032 and 0045 decided. The syntax is invented. Statement order never matters, ADR 0004 (a board reaches a module only through its interface) and ADR 0005 (only a board names nets) are assumed to stand.

### Option A. One file per board, as today, extended

The board's `.nsm` holds everything. Add what issue #2 asks for inside the existing `.place` idea.

```nsm-new
board Widget {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .outline: Outline { .width &= 62mm  .height &= 30mm }

    .usb: UsbCReceptacle
    .led1: Led
    .led2: Led
    .radio: RadioModule

    // Case drawing, slot A.
    .usb.place.left &= -0.5mm +/- 0.05mm
    // Light pipes are cut 12.7 mm apart.
    .led2.place.x - .led1.place.x == 12.7mm +/- 0.1mm
    .led2.place.y == .led1.place.y
    // The whole radio block goes on the back.
    .radio.place.side &= "bottom"
}
```

- **Pros.** Nothing to build for the layout of the files; ADR 0045 section 3 recommended it and the owner decided it on 2026-11-02. One file explains the board. A change that touches the circuit and a requirement together is one diff. Agents already read and write one file (`netsmith-use-constraints`).
- **Cons.** The file mixes two audiences and two rates of change: a case drawing revision edits the same file as a resistor change. A schematic-only reader sees constraint lines between parts. The group `side` and the gap are new fields on the one built-in `.place`, which grows toward a bag of unrelated fields (`rotation` of a module is already refused).
- **Editor.** The source pane shows all of it; "pin here" (C5) writes lines. A "Constraints" list in the PCB view can show only the `.place` lines, so the separation the owner wants can be a view, not a file.
- **Agents.** One file, one skill. The risk is a long file read in full to find the six constraint lines.

### Option B. A second file per board, in the same language

The `.nsm` board is the circuit. A sibling file holds the PCB intent in the same syntax, naming the board it belongs to.

```nsm-new
// widget.nsm: no outline, no placement
board Widget {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .usb: UsbCReceptacle
    .led1: Led
    .led2: Led
    .radio: RadioModule
}
```

```nsm-new
// widget.pcb.nsm
pcb for Widget {
    .outline: Outline { .width &= 62mm  .height &= 30mm }
    // Case drawing, slot A.
    .usb.place.left &= -0.5mm +/- 0.05mm
    .led2.place.x - .led1.place.x == 12.7mm +/- 0.1mm
    .radio.place.side &= "bottom"
}
```

- **Pros.** Matches the owner's question most directly: `.nsm` is for the schematic and everything about copper and the case is in one file. A circuit review does not see case numbers. The pcb file could be handed to a mechanical engineer.
- **Cons.** It is a language change: a construct that extends a board from another file, which breaks "a definition is where it is stated" unless carefully scoped, and it reaches into the board's members from outside (ADR 0004 is about modules, but the same question comes up). The process, `.assembly`, `.height` and `.designator` each need a ruling on which file they go in. Two files must be kept in step (a rename in one breaks the other). Existing examples and guide blocks move; the guide test runs complete files, so each block becomes two. ADR 0001 requires the old form to go, so every `.place` and `Outline` in a board file becomes an error.
- **Editor.** A tab per file; the PCB view reads the pcb file; the file tree needs to pair them. "Pin here" writes only to the pcb file, a cleaner rule than in A. The schematic view never needs the pcb file.
- **Agents.** Two files per board. A new rule in the skills: "constraints go in the `.pcb.nsm`". An agent that adds a part must decide whether it also touches the pcb file. More instructions, less to read at once.

### Option C. A `pcb` block inside the board

One file, but the PCB intent is in a block with its own name, so the separation is visible and tools can fold or list it.

```nsm-new
board Widget {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .usb: UsbCReceptacle
    .led1: Led
    .led2: Led

    pcb {
        .outline: Outline { .width &= 62mm  .height &= 30mm }
        // Case drawing, slot A.
        .usb.place.left &= -0.5mm +/- 0.05mm
        .led2.place.x - .led1.place.x == 12.7mm +/- 0.1mm
    }
}
```

- **Pros.** The cheapest of the three that separate things: no second file, no pairing, no cross-file name. The editor, the formatter and the checker can treat the block as a unit, and `build` for a schematic-only run can skip it. ADR 0004 and 0005 are untouched, because the block is the board.
- **Cons.** Still a language change (a block form), and every old `.place` line moves into it. The owner's wish that the `.nsm` be "strictly for schematics" is only met visually. A block inside a module would raise the question whether modules may carry their own `pcb` block (that would give group `side` and relative gaps a home, and would help issue #16).
- **Editor.** The block is a natural fold and the PCB view can show its lines; "pin here" has a defined place to write.
- **Agents.** One file, one more word to learn; the skill says "inside `pcb { }`".

### Option D. A data file for requirements, next to the layout file

Requirements as TOML, separate from the solution: `widget.pcb.toml` stays the solution and a new `widget.rules.toml` holds the enclosure and placement requirements. ADR 0045 section 3 weighed this as Option B (requirements in the layout file) and rejected it.

```toml
# widget.rules.toml
[Widget.outline]
width = "62mm"
height = "30mm"

[Widget.place.usb]
left = "-0.5mm +/- 0.05mm"      # case drawing, slot A

[[Widget.relation]]
parts = ["led1", "led2"]
x_offset = "12.7mm +/- 0.1mm"
```

- **Pros.** No language change. The editor already reads and writes TOML. A tool other than netsmith can generate the file from a CAD export.
- **Cons.** ADR 0045 gave the reasons, and they still hold: no expressions (`.outline.height / 2`), no link to other members, a second expression language for tolerances, and two TOML files side by side that mean different things (one solution, one requirement). A person who edits the wrong one changes the meaning. Errors lose their line numbers into a language the checker understands. Agents write a second syntax.
- **Editor.** Easiest to build. Poorest at showing a requirement beside the circuit it constrains.
- **Agents.** Two syntaxes to know. The weakest fit with "the language's design rules" in `README.md`.

### An orthogonal question: the enclosure as a definition

Whichever option is chosen, enclosure constraints could be a thing of their own that several boards and a system share, instead of lines in one board:

```nsm-new
enclosure SnapCase {
    .inner: Outline { .width &= 62mm  .height &= 30mm }
    .slot_a: Opening { .edge &= "left"  .y &= 15mm +/- 0.1mm  .overhang &= 0.5mm }
    .ribs: Keepout { .x &= 0mm  .y &= 0mm  .width &= 62mm  .height &= 18mm  .max_height &= 3mm }
}

board Widget {
    .case: SnapCase
    .usb.place ~ .case.slot_a
}
```

This fits ADR 0045 C7 (a plug-in and a carrier share a frame) and `dxf import`, which could print an `enclosure`. It costs a new definition kind, a glossary entry and an ADR, and it only pays off when a case is reused. No real board of the owner's has one yet that I know of.

### Comparison

| | A. one file | B. second file | C. `pcb` block | D. TOML |
|---|---|---|---|---|
| Language change | small (new `.place` fields) | large (cross-file extension) | medium (block form) | none |
| Meets "`.nsm` only for schematics" | no | yes | visually | yes |
| One place per fact (ADR 0001) | yes, if section 1.2 is fixed | yes, if the old forms go | yes, if the old forms go | no: three files hold board facts |
| Expressions, tolerances, line-numbered errors | yes | yes | yes | no |
| Migration of examples and the guide | none | every board | every board | every board |
| Editor work | list view, C5 | pairing, tabs, C5 into one file | fold, C5 into the block | read and write TOML |
| Agent instructions | unchanged | one more rule, two files | one more word | a second syntax |

## 4. Recommendation

Do not split the file yet. This is my reading and the owner may weigh it differently.

1. **Keep Option A for now and fix the places where a fact has two homes** (section 1.2, items 1, 2 and 6). Decide where a drawn keepout and a DXF hole belong, and make the glossary say what the editor writes. This costs little and removes the part of the owner's unease that is a real inconsistency.
2. **Answer the UX worry in the editor, not in the language.** A "Constraints" panel in the PCB view that lists the `.place`, outline, keepout and height lines, grouped and clickable, with their findings, gives the schematic reader a clean view and the mechanical reader a focused one. It needs no syntax decision and is where C4 and C5 already point.
3. **Revisit B or C when a real board has more than about 30 PCB lines**, or when two people own the circuit and the case. At that point there is a sample to judge by. Of the two, C is the cheaper step and leaves ADR 0004 and 0005 alone; B is the one the owner's wording describes. D I would not build.
4. **Add the group `side` and the gap by the owner's choice of syntax** (questions 4 and 5), whichever option stands. Both need new language, so they wait.
5. **Hold the `enclosure` definition** until a second board shares a case.

The risk of waiting: if B or C is the end state, every line written in A has to move later, and ADR 0001 means no grace period. The cost is bounded by the number of lines, which is small today (7 in the examples) and grows with each board.

## 5. Questions for the owner

1. What is the discomfort exactly: the file getting long, circuit changes and case changes landing in one diff, or the schematic view showing things that are not schematic? The answer picks between A with a better editor, C and B.
2. Should a board with no PCB intent be a complete, valid `.nsm` file that builds and checks as a schematic only? (It is today. B and C keep it so; the question is whether it is a rule.)
3. Is the enclosure something several boards share (an `enclosure` definition), or per board?
4. A face for a group: should a module have its own `.place.side`, or should there be a set or group construct that names parts from several modules? (Language choice.)
5. Gap and alignment (ADR 0045 C2): the `a.place.x - b.place.x == 12.7mm +/- 0.1mm` form of the ADR, or a named relation such as a gap between two parts? (Language choice.)
6. Are hole positions taken from a case drawing a requirement (so in the source, with the layout file holding any solution) or a solution (the layout file, as `dxf import --write` does now)?
7. May a keepout drawn in the editor keep its size and what it forbids in the layout file, or should the editor write it into the source?
8. May the editor write constraint lines into the source (ADR 0045 question 8, C5)? In B and C this decides where it writes.
9. Is the glossary right that the editor writes only `.nsm`, or should it say it also writes the layout file?
